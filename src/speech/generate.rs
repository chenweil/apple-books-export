//! `speech generate` use case：从一个 Annotation 内容部分得到一个已验证的
//! Cached Speech Clip。
//!
//! 状态流固定（实施 spec 第 9 节）：
//!
//! ```text
//! 解析参数冲突 → 选择内容 → 规范化并校验 Speech Text → 解析 Voice Profile
//!   → 计算 clip_id → 取跨进程 writer 锁
//!   → 有效缓存？        → receipt(source=cache)，0 次 provider 调用
//!   → unknown gate？     → SPEECH_RESULT_UNKNOWN，不自动重放
//!   → 音色可用性 + API Key + 存储预检
//!   → 建 attempt → 同步合成 → hex 解码 → MP3 校验 → 不可变 version → 原子切换 pointer
//!   → receipt(source=provider)
//! ```
//!
//! 本地失败（内容缺失、归属错误、超长文本、参数冲突、Profile 无效）发生在任何 provider
//! 连接之前；不确定结果与「provider 成功但没有可用产物」都进入阻塞态，只有显式
//! `--regenerate` 才能越过。

use crate::models::Annotation;
use crate::speech::audio::{validate_audio, AudioDecodeError};
use crate::speech::cache::{
    AttemptRecord, AttemptStatus, ClipCache, ClipCacheError, ClipLock, ClipLockError, ClipState,
    ClipVersionMetadata, ATTEMPT_SCHEMA_VERSION, CLIP_STATE_SCHEMA_VERSION,
    CLIP_VERSION_SCHEMA_VERSION,
};
use crate::speech::catalog::{verify_voice, CatalogAvailability};
use crate::speech::clip::{
    clip_id, select_speech_content, ClipFingerprint, SpeechContentError, SpeechContentKind,
};
use crate::speech::machine::SpeechGenerateReceipt;
use crate::speech::profile::{
    resolve_generation_profile, ProfileDraft, ProfileError, VoiceProfile, DEFAULT_MODEL,
    DEFAULT_VOICE_ID, SENSEAUDIO_PROVIDER,
};
use crate::speech::senseaudio::{
    load_or_refresh_voice_catalog, SenseAudioError, SynthesisRequest, SynthesisResponse,
    VoiceCatalogError,
};
use crate::speech::store::{SpeechStore, SpeechStoreError};
use crate::speech::text::{sha256_hex, BillingEstimate, BILLING_ESTIMATOR_VERSION};
use crate::speech::SpeechWarning;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::future::Future;
use std::path::PathBuf;

/// `speech generate` 的输入。调用方负责把数据库行与 CLI 参数准备好。
#[derive(Debug, Clone)]
pub struct GenerationInput {
    /// 用户级 Speech 状态根。
    pub store: SpeechStore,
    /// 候选 Annotation（通常是一本书的全部标注）。
    pub annotations: Vec<Annotation>,
    /// 稳定内容身份。
    pub request: GenerationRequest,
}

/// 一次生成请求的稳定身份与覆盖项。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GenerationRequest {
    /// 书籍稳定 ID。
    pub asset_id: String,
    /// Annotation 稳定 ID。
    pub annotation_id: String,
    /// 内容部分。
    pub content_kind: Option<SpeechContentKind>,
    /// Voice Profile 覆盖项；未提供的字段沿用全局 Profile。
    pub overrides: GenerationOverrides,
    /// 显式越过 unknown gate 并替换有效缓存。
    pub regenerate: bool,
}

/// `speech generate` 的 Voice Profile 覆盖项；与 `profile set` 共用解析与校验。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GenerationOverrides {
    /// 覆盖音色 ID。
    pub voice_id: Option<String>,
    /// 覆盖语速（原始十进制文本）。
    pub speed: Option<String>,
    /// 覆盖音量（原始十进制文本）。
    pub volume: Option<String>,
    /// 覆盖声调（原始整数文本）。
    pub pitch: Option<String>,
}

impl GenerationOverrides {
    /// 转成共用的 Profile 覆盖结构。
    pub fn to_draft(&self) -> ProfileDraft {
        ProfileDraft {
            provider: None,
            model: None,
            voice_id: self.voice_id.clone(),
            emotion_label: None,
            style_label: None,
            speed: self.speed.clone(),
            volume: self.volume.clone(),
            pitch: self.pitch.clone(),
            api_key_env: None,
        }
    }
}

/// 发往 Speech Provider 的 provider-neutral 合成请求。
#[derive(Debug, Clone, PartialEq)]
pub struct SpeechSynthesisRequest {
    /// 供应商模型。
    pub model: String,
    /// 已插入控制标记守卫的 Speech Text。
    pub text: String,
    /// 已解析的具体音色 ID。
    pub voice_id: String,
    /// 语速（百分之一单位）。
    pub speed_x100: i32,
    /// 音量（百分之一单位）。
    pub volume_x100: i32,
    /// 声调。
    pub pitch: i32,
    /// 首版固定音频格式。
    pub audio_format: String,
    /// 首版固定采样率。
    pub sample_rate: u32,
    /// 首版固定码率。
    pub bitrate: u32,
    /// 首版固定声道数。
    pub channel: u32,
}

/// 一次成功合成的产物。
#[derive(Debug, Clone)]
pub struct SpeechSynthesisOutcome {
    /// 供应商 trace ID。
    pub trace_id: Option<String>,
    /// 供应商返回的用量字符数。
    pub usage_characters: Option<u64>,
}

/// clip 的来源：只有真正创建了 Speech Attempt 时才是 `Provider`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechClipSource {
    /// 复用现有 Speech Cache Entry，没有 provider 调用。
    Cache,
    /// 创建了 Speech Attempt。
    Provider,
}

impl SpeechClipSource {
    /// 稳定的机器可读取值。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cache => "cache",
            Self::Provider => "provider",
        }
    }

    /// 是否真的向供应商发起了请求。
    pub const fn provider_called(self) -> bool {
        matches!(self, Self::Provider)
    }
}

/// 一次生成的结果；只含非秘密 metadata，不含原文、密钥或音频字节。
#[derive(Debug, Clone)]
pub struct GenerateOutcome {
    /// clip 来源。
    pub source: SpeechClipSource,
    /// 完整 clip ID。
    pub clip_id: String,
    /// 本次 Speech Attempt ID；cache hit 时为 `None`。
    pub attempt_id: Option<String>,
    /// 稳定内容身份。
    pub asset_id: String,
    /// 稳定内容身份。
    pub annotation_id: String,
    /// 内容部分。
    pub content_kind: SpeechContentKind,
    /// 文本摘要（不含原文）。
    pub text_sha256: String,
    /// 字符统计与计费估算。
    pub billing: BillingEstimate,
    /// 解析后的 Voice Profile。
    pub profile: VoiceProfile,
    /// 本地音频路径。
    pub audio_path: PathBuf,
    /// 音频字节的 SHA-256。
    pub audio_sha256: String,
    /// 音频格式。
    pub audio_format: String,
    /// 音频字节数。
    pub audio_size_bytes: u64,
    /// 音频时长（毫秒）。
    pub audio_duration_ms: u64,
    /// 采样率（Hz）。
    pub sample_rate: u32,
    /// 码率（bps）。
    pub bitrate: u32,
    /// 声道数。
    pub channel: u32,
    /// 供应商 trace ID。
    pub trace_id: Option<String>,
    /// 供应商返回的用量字符数。
    pub usage_characters: Option<u64>,
    /// 结构化 warning，例如损坏缓存被替换。
    pub warnings: Vec<SpeechWarning>,
}

impl GenerateOutcome {
    /// 转换成 Machine JSON receipt。
    pub fn to_receipt(&self) -> SpeechGenerateReceipt {
        SpeechGenerateReceipt::from_outcome(self)
    }
}

/// 生成失败。全部使用稳定的产品错误码；不确定结果不会自动重放。
#[derive(Debug)]
pub enum GenerationError {
    /// Speech 状态根不可用。
    Storage(SpeechStoreError),
    /// 内容选择失败：归属错误、内容缺失或文本超长。
    Content(SpeechContentError),
    /// Voice Profile 无效。
    Profile(ProfileError),
    /// human/machine 参数冲突或枚举无效。
    InvalidArgument(String),
    /// 当前 Voice Catalog 明确没有这个音色，或无法验证可用性。
    VoiceUnavailable {
        /// 被拒绝的音色 ID。
        voice_id: String,
        /// 无法验证的具体原因。
        reason: VoiceUnavailableReason,
    },
    /// 生成前无法取得当前 Voice Catalog。
    VoiceCatalog(VoiceCatalogError),
    /// 缺少 API Key；不发起任何连接。
    MissingApiKey,
    /// provider 明确失败（限流或错误响应）。
    ProviderFailed {
        /// 稳定错误码。
        code: &'static str,
        /// 供应商 trace ID。
        trace_id: Option<String>,
        /// 供应商错误码。
        provider_code: Option<String>,
        /// 本次 attempt ID。
        attempt_id: String,
    },
    /// 请求可能已到达 provider，但结果不确定。
    Unknown {
        /// 供应商 trace ID。
        trace_id: Option<String>,
        /// 供应商错误码。
        provider_code: Option<String>,
        /// 本次 attempt ID。
        attempt_id: String,
        /// 面向人类的说明，不含原文或密钥。
        message: String,
    },
    /// provider 成功但本地无法形成有效音频产物。
    AudioInvalid {
        /// 本次 attempt ID。
        attempt_id: String,
        /// 供应商 trace ID。
        trace_id: Option<String>,
        /// 失败原因，不含原文或密钥。
        message: String,
    },
    /// provider 成功但本地原子提交失败。
    ArtifactCommit {
        /// 本次 attempt ID。
        attempt_id: String,
        /// 失败原因，不含原文或密钥。
        message: String,
    },
    /// 同 clip 的 writer 锁等待超时。
    InProgress {
        /// 正在生成的 clip ID。
        clip_id: String,
    },
}

/// 无法把音色判定为可用的具体原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceUnavailableReason {
    /// 当前目录里没有这个音色。
    MissingFromCatalog,
    /// 拿不到当前目录，无法验证可用性。
    CatalogNotCurrent,
}

impl VoiceUnavailableReason {
    /// 稳定的机器可读取值。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingFromCatalog => "voice_unavailable",
            Self::CatalogNotCurrent => "voice_availability_unverified",
        }
    }
}

impl GenerationError {
    /// 不含秘密的原因码；进入 error `details.reason`。
    pub fn reason_code(&self) -> &'static str {
        match self {
            Self::Storage(_) => "storage_unavailable",
            Self::Content(SpeechContentError::InvalidAnnotationId { .. }) => {
                "annotation_not_in_book"
            }
            Self::Content(SpeechContentError::ContentUnavailable { .. }) => "content_unavailable",
            Self::Content(SpeechContentError::TooLong(_)) => "text_too_long",
            Self::Profile(_) => "profile_invalid",
            Self::InvalidArgument(_) => "invalid_argument",
            Self::VoiceUnavailable { reason, .. } => reason.as_str(),
            Self::VoiceCatalog(_) => "voice_catalog_unavailable",
            Self::MissingApiKey => "missing_api_key",
            Self::ProviderFailed { code, .. } => {
                if *code == "SPEECH_RATE_LIMITED" {
                    "rate_limited"
                } else {
                    "provider_failed"
                }
            }
            Self::Unknown { .. } => "result_unknown",
            Self::AudioInvalid { .. } => "provider_succeeded_artifact_missing",
            Self::ArtifactCommit { .. } => "artifact_commit_failed",
            Self::InProgress { .. } => "in_progress",
        }
    }

    /// 供应商 trace ID；本地失败为 `None`。
    pub fn trace_id(&self) -> Option<&str> {
        match self {
            Self::Unknown { trace_id, .. }
            | Self::AudioInvalid { trace_id, .. }
            | Self::ProviderFailed { trace_id, .. } => trace_id.as_deref(),
            _ => None,
        }
    }

    /// 本次 Speech Attempt ID；没有创建 attempt 时为 `None`。
    pub fn attempt_id(&self) -> Option<&str> {
        match self {
            Self::Unknown { attempt_id, .. }
            | Self::AudioInvalid { attempt_id, .. }
            | Self::ArtifactCommit { attempt_id, .. }
            | Self::ProviderFailed { attempt_id, .. } => Some(attempt_id.as_str()),
            _ => None,
        }
    }

    /// 结果语义：`failed` 是明确失败，`unknown` 与
    /// `provider_succeeded_artifact_missing` 都不得自动重放。
    pub const fn outcome(&self) -> &'static str {
        match self {
            Self::Unknown { .. } => "unknown",
            Self::AudioInvalid { .. } => "provider_succeeded_artifact_missing",
            _ => "failed",
        }
    }

    /// 面向用户的补救建议。
    pub fn remediation(&self) -> &'static str {
        match self {
            Self::Storage(_) => {
                "Verify that the Speech state directory exists and is writable, then retry."
            }
            Self::Content(SpeechContentError::InvalidAnnotationId { .. }) => {
                "Run `apple-books-exporter annotations --asset-id BOOK_ID --json` and use an annotation_id that belongs to that book."
            }
            Self::Content(SpeechContentError::ContentUnavailable { .. }) => {
                "Choose the other content part of this Annotation, or pick an Annotation that has that content."
            }
            Self::Content(SpeechContentError::TooLong(_)) => {
                "This Annotation exceeds the provider text limit; shorten it in Apple Books or choose another Annotation."
            }
            Self::Profile(_) => {
                "Run `apple-books-exporter speech profile show --json` to inspect the stored Voice Profile, then set valid values or reset it."
            }
            Self::InvalidArgument(_) => {
                "Run the command with --help and use either the human positional form or the JSON asset_id form."
            }
            Self::VoiceUnavailable { .. } => {
                "Refresh the Voice Catalog with `apple-books-exporter speech voices --refresh` and choose an available voice."
            }
            Self::VoiceCatalog(_) => {
                "Refresh the Voice Catalog with `apple-books-exporter speech voices --refresh`, then retry the generation."
            }
            Self::MissingApiKey => {
                "Set the API key in the environment variable named by `speech profile show --json`."
            }
            Self::ProviderFailed { code, .. } => {
                if *code == "SPEECH_RATE_LIMITED" {
                    "Wait for the provider rate limit to clear, then retry the generation."
                } else {
                    "Verify the SenseAudio API key and endpoint, then retry the generation."
                }
            }
            Self::Unknown { .. } => {
                "Retry with --regenerate only if another billed generation is acceptable."
            }
            Self::AudioInvalid { .. } => {
                "Retry with --regenerate; the provider was called but no acceptable audio was produced."
            }
            Self::ArtifactCommit { .. } => {
                "Free Speech cache space and retry with --regenerate; the provider was already billed."
            }
            Self::InProgress { .. } => {
                "Wait for the in-progress generation to finish, then retry."
            }
        }
    }

    /// 稳定的 Machine JSON 错误码。
    pub const fn machine_code(&self) -> &'static str {
        match self {
            Self::Storage(_) => "SPEECH_STORAGE_UNAVAILABLE",
            Self::Content(error) => error.machine_code(),
            Self::Profile(_) => "SPEECH_PROFILE_INVALID",
            Self::InvalidArgument(_) => "INVALID_ARGUMENT",
            Self::VoiceUnavailable { .. } => "SPEECH_VOICE_UNAVAILABLE",
            Self::VoiceCatalog(error) => error.machine_code(),
            Self::MissingApiKey => "SPEECH_AUTH_FAILED",
            Self::ProviderFailed { code, .. } => code,
            Self::Unknown { .. } => "SPEECH_RESULT_UNKNOWN",
            Self::AudioInvalid { .. } => "SPEECH_AUDIO_INVALID",
            Self::ArtifactCommit { .. } => "SPEECH_ARTIFACT_COMMIT_FAILED",
            Self::InProgress { .. } => "SPEECH_IN_PROGRESS",
        }
    }

    /// 面向人类的说明；不包含原文、密钥或供应商原始响应体。
    pub fn message(&self) -> String {
        match self {
            Self::Storage(error) => error.to_string(),
            Self::Content(error) => error.message(),
            Self::Profile(error) => error.message(),
            Self::InvalidArgument(message) => message.clone(),
            Self::VoiceUnavailable { voice_id, reason } => match reason {
                VoiceUnavailableReason::MissingFromCatalog => format!(
                    "Voice '{voice_id}' is not available in the current Voice Catalog."
                ),
                VoiceUnavailableReason::CatalogNotCurrent => format!(
                    "Voice '{voice_id}' could not be verified against a current Voice Catalog, so no Speech Attempt was created."
                ),
            },
            Self::VoiceCatalog(error) => error.to_string(),
            Self::MissingApiKey => {
                "the configured API key environment variable is missing or empty".to_string()
            }
            Self::ProviderFailed { code, .. } => match *code {
                "SPEECH_RATE_LIMITED" => "SenseAudio rate-limited the synthesis request".to_string(),
                _ => "SenseAudio rejected the synthesis request".to_string(),
            },
            Self::Unknown { message, .. } => message.clone(),
            Self::AudioInvalid { message, .. } => message.clone(),
            Self::ArtifactCommit { message, .. } => message.clone(),
            Self::InProgress { clip_id } => format!(
                "another generation for Speech Clip '{clip_id}' is still in progress"
            ),
        }
    }

    /// 该失败是否阻止普通 `generate` 自动重放（unknown / provider-success-no-artifact）。
    pub const fn blocks_generation(&self) -> bool {
        matches!(self, Self::Unknown { .. } | Self::AudioInvalid { .. })
    }

    /// attempt 终态；没有创建 attempt 的本地失败返回 `None`。
    pub fn attempt_status(&self) -> Option<AttemptStatus> {
        match self {
            Self::ProviderFailed { .. } => Some(AttemptStatus::ProviderFailed),
            Self::Unknown { .. } => Some(AttemptStatus::Unknown),
            Self::AudioInvalid { .. } => Some(AttemptStatus::ProviderSucceededArtifactMissing),
            _ => None,
        }
    }
}

/// 生成一个 Speech Clip。
///
/// 注入的两个 future 是公开 mock 缝：测试可以用本地假 provider 覆盖成功、明确失败和
/// 不确定结果，而不需要网络或真实凭证。
pub async fn generate_clip<CatalogFetch, CatalogFuture, Synthesize, SynthesizeFuture>(
    input: GenerationInput,
    api_key: Option<String>,
    now: DateTime<Utc>,
    catalog_fetch: CatalogFetch,
    synthesize: Synthesize,
) -> Result<GenerateOutcome, GenerationError>
where
    CatalogFetch: FnOnce() -> CatalogFuture,
    CatalogFuture:
        Future<Output = Result<Vec<crate::speech::catalog::CatalogVoice>, SenseAudioError>>,
    Synthesize: FnOnce(SpeechSynthesisRequest) -> SynthesizeFuture,
    SynthesizeFuture: Future<Output = Result<SynthesisResponse, SenseAudioError>>,
{
    let GenerationInput {
        store,
        annotations,
        request,
    } = input;

    // 1. 内容身份：错误归属、缺失内容与超长文本都在联网前失败。
    let content_kind = request.content_kind.ok_or_else(|| {
        GenerationError::InvalidArgument("--content must be highlight or note".to_string())
    })?;
    let selection = select_speech_content(
        &request.asset_id,
        &request.annotation_id,
        content_kind,
        &annotations,
    )
    .map_err(GenerationError::Content)?;

    // 2. Voice Profile：本地结构/范围校验，不联网。
    let config = store.load_config().map_err(GenerationError::Storage)?;
    let profile = resolve_generation_profile(&config.profile, &request.overrides.to_draft())
        .map_err(GenerationError::Profile)?;

    // 3. clip 身份：任何 provider 有效输入变化都会得到新的 clip ID。
    let id = clip_id(&ClipFingerprint {
        provider: &profile.provider,
        model: &profile.model,
        asset_id: &selection.asset_id,
        annotation_id: &selection.annotation_id,
        content_kind,
        normalized_speech_text: &selection.text.normalized,
        voice_id: &profile.voice_id,
        speed_x100: profile.speed.x100(),
        volume_x100: profile.volume.x100(),
        pitch: profile.pitch,
        audio: &profile.audio,
    });

    let cache = ClipCache::new(store.clone());
    let mut warnings = Vec::new();

    // 4. 跨进程 writer 锁：同一 clip 只有一个 provider writer。
    let lock = ClipLock::acquire(&cache, &id, now).map_err(|error| match error {
        ClipLockError::InProgress => GenerationError::InProgress {
            clip_id: id.clone(),
        },
        ClipLockError::Unavailable(_, error) => {
            GenerationError::Storage(SpeechStoreError::Unavailable {
                path: cache.locks_dir(),
                message: error.to_string(),
            })
        }
    })?;

    // 5. 有效缓存直接复用：0 次 provider 调用。
    let cached = match cache.load_ready_clip(&id) {
        Ok(ready) => ready,
        Err(error) => {
            // 损坏缓存不阻塞显式生成：记录 warning 后重新生成，绝不当成有效缓存返回。
            warnings.push(SpeechWarning {
                code: "SPEECH_CACHE_CORRUPT",
                reason: "corrupt_cache_entry",
                message: format!(
                    "The existing Speech Cache Entry was rejected ({}). A new version is being generated.",
                    error.message()
                ),
            });
            None
        }
    };
    if let Some(ready) = cached {
        if !request.regenerate {
            drop(lock);
            return Ok(outcome_from_ready(
                &id,
                &selection,
                &profile,
                &ready,
                SpeechClipSource::Cache,
                None,
                warnings,
            ));
        }
    }

    let state = load_or_init_state(&cache, &id, &selection, now)?;

    // 6. unknown gate：没有有效缓存时，普通 generate 不得自动重放。
    if state.generation_blocked && !request.regenerate {
        drop(lock);
        return Err(GenerationError::Unknown {
            trace_id: None,
            provider_code: None,
            attempt_id: state.latest_attempt_id.clone().unwrap_or_default(),
            message: format!(
                "The previous Speech Attempt for clip '{id}' ended as {}, so the provider may have processed the request.",
                state
                    .latest_attempt_status
                    .map(AttemptStatus::as_str)
                    .unwrap_or("unknown")
            ),
        });
    }

    // 7. 调用 provider 前的预检：API Key、音色可用性、存储可写。
    if api_key.as_deref().unwrap_or_default().trim().is_empty() {
        return Err(GenerationError::MissingApiKey);
    }
    let catalog_outcome =
        load_or_refresh_voice_catalog(&store, SENSEAUDIO_PROVIDER, now, false, catalog_fetch)
            .await
            .map_err(GenerationError::VoiceCatalog)?;
    let availability = CatalogAvailability::Available(catalog_outcome.catalog);
    match verify_voice(&profile, &availability, now) {
        crate::speech::catalog::VoiceVerification::Verified => {}
        crate::speech::catalog::VoiceVerification::Unavailable => {
            return Err(GenerationError::VoiceUnavailable {
                voice_id: profile.voice_id.clone(),
                reason: VoiceUnavailableReason::MissingFromCatalog,
            })
        }
        crate::speech::catalog::VoiceVerification::Unverified(_) => {
            // 拿不到当前目录就不能创建 Speech Attempt：不能让未经确认的音色计费。
            return Err(GenerationError::VoiceUnavailable {
                voice_id: profile.voice_id.clone(),
                reason: VoiceUnavailableReason::CatalogNotCurrent,
            });
        }
    }
    ensure_storage_writable(&cache)?;

    // 8. 建 attempt，发起唯一一次同步请求。
    let attempt_id = new_attempt_id(&id, now);
    let synthesis_request = SpeechSynthesisRequest {
        model: profile.model.clone(),
        text: selection.text.provider_safe_text(),
        voice_id: profile.voice_id.clone(),
        speed_x100: profile.speed.x100(),
        volume_x100: profile.volume.x100(),
        pitch: profile.pitch,
        audio_format: profile.audio.format.clone(),
        sample_rate: profile.audio.sample_rate,
        bitrate: profile.audio.bitrate,
        channel: profile.audio.channel,
    };
    let started_at = now.to_rfc3339();
    let response = match synthesize(synthesis_request).await {
        Ok(response) => response,
        Err(error) => {
            let (mapped, status) = map_provider_error(&error, &attempt_id);
            let mut next = state.clone();
            next.latest_attempt_id = Some(attempt_id.clone());
            record_attempt_and_gate(
                &cache,
                &attempt_id,
                &id,
                &profile,
                &selection,
                &started_at,
                now,
                status,
                mapped.machine_code(),
                None,
                None,
                &mut next,
            );
            return Err(mapped);
        }
    };

    // 9. 校验音频：hex 已在 adapter 解码，这里校验格式与本地 metadata 一致性。
    let facts = match validate_audio(&response.audio_bytes, &profile.audio) {
        Ok(facts) => facts,
        Err(error) => {
            let mut next = state.clone();
            next.latest_attempt_id = Some(attempt_id.clone());
            record_attempt_and_gate(
                &cache,
                &attempt_id,
                &id,
                &profile,
                &selection,
                &started_at,
                now,
                AttemptStatus::ProviderSucceededArtifactMissing,
                "SPEECH_AUDIO_INVALID",
                None,
                response.trace_id.clone(),
                &mut next,
            );
            return Err(GenerationError::AudioInvalid {
                attempt_id,
                trace_id: response.trace_id,
                message: audio_invalid_message(&error),
            });
        }
    };

    let audio_sha256 = sha256_hex(&response.audio_bytes);
    let metadata = ClipVersionMetadata {
        schema_version: CLIP_VERSION_SCHEMA_VERSION,
        clip_id: id.clone(),
        audio_sha256: audio_sha256.clone(),
        asset_id: selection.asset_id.clone(),
        annotation_id: selection.annotation_id.clone(),
        content_kind,
        text_sha256: selection.text.text_sha256.clone(),
        unicode_characters: selection.text.unicode_characters,
        estimated_billing_characters: selection.text.estimated_billing_characters,
        billing_estimator_version: BILLING_ESTIMATOR_VERSION.to_string(),
        provider: profile.provider.clone(),
        model: profile.model.clone(),
        voice_id: profile.voice_id.clone(),
        speed_x100: profile.speed.x100(),
        volume_x100: profile.volume.x100(),
        pitch: profile.pitch,
        format: facts.format.clone(),
        sample_rate: facts.sample_rate,
        bitrate: facts.bitrate,
        channel: facts.channel,
        duration_ms: facts.duration_ms,
        size_bytes: facts.size_bytes,
        attempt_id: attempt_id.clone(),
        trace_id: response.trace_id.clone(),
        provider_usage_characters: response.usage_characters,
        created_at: now.to_rfc3339(),
    };

    // 10. 不可变 version + 原子 current pointer。
    let mut committed = state.clone();
    committed.latest_attempt_id = Some(attempt_id.clone());
    committed.latest_attempt_status = Some(AttemptStatus::Succeeded);
    committed.latest_error_code = None;
    if let Err(error) = cache.commit_version(&committed, &metadata, &response.audio_bytes, now) {
        let mut next = committed.clone();
        record_attempt_and_gate(
            &cache,
            &attempt_id,
            &id,
            &profile,
            &selection,
            &started_at,
            now,
            AttemptStatus::ProviderSucceededArtifactMissing,
            "SPEECH_ARTIFACT_COMMIT_FAILED",
            None,
            response.trace_id.clone(),
            &mut next,
        );
        return Err(GenerationError::ArtifactCommit {
            attempt_id,
            message: error.message(),
        });
    }

    let attempt = attempt_record(
        &attempt_id,
        &id,
        &profile,
        &selection,
        &started_at,
        now,
        AttemptStatus::Succeeded,
        response.usage_characters,
        None,
        None,
        response.trace_id.clone(),
    );
    let _ = cache.record_attempt(&attempt, now);

    Ok(GenerateOutcome {
        source: SpeechClipSource::Provider,
        clip_id: id,
        attempt_id: Some(attempt_id),
        asset_id: selection.asset_id,
        annotation_id: selection.annotation_id,
        content_kind,
        text_sha256: selection.text.text_sha256.clone(),
        billing: BillingEstimate::from(&selection.text),
        profile,
        audio_path: cache
            .clip_dir(&metadata.clip_id)
            .map(|directory| {
                directory
                    .join("versions")
                    .join(&metadata.audio_sha256)
                    .join("audio.mp3")
            })
            .unwrap_or_default(),
        audio_sha256,
        audio_format: facts.format,
        audio_size_bytes: facts.size_bytes,
        audio_duration_ms: facts.duration_ms,
        sample_rate: facts.sample_rate,
        bitrate: facts.bitrate,
        channel: facts.channel,
        trace_id: response.trace_id,
        usage_characters: response.usage_characters,
        warnings,
    })
}

/// 把 provider-neutral 请求映射成 SenseAudio 适配器请求。
pub fn to_adapter_request(request: &SpeechSynthesisRequest) -> SynthesisRequest {
    SynthesisRequest {
        model: request.model.clone(),
        text: request.text.clone(),
        voice_id: request.voice_id.clone(),
        speed: request.speed_x100 as f64 / 100.0,
        volume: request.volume_x100 as f64 / 100.0,
        pitch: request.pitch,
        audio_format: request.audio_format.clone(),
        sample_rate: request.sample_rate,
        bitrate: request.bitrate,
        channel: request.channel,
    }
}

fn outcome_from_ready(
    clip_id: &str,
    selection: &crate::speech::clip::SpeechContentSelection,
    profile: &VoiceProfile,
    ready: &crate::speech::cache::ReadyClip,
    source: SpeechClipSource,
    attempt_id: Option<String>,
    warnings: Vec<SpeechWarning>,
) -> GenerateOutcome {
    let metadata = &ready.metadata;
    GenerateOutcome {
        source,
        clip_id: clip_id.to_string(),
        attempt_id,
        asset_id: selection.asset_id.clone(),
        annotation_id: selection.annotation_id.clone(),
        content_kind: selection.content_kind,
        text_sha256: selection.text.text_sha256.clone(),
        billing: BillingEstimate::from(&selection.text),
        profile: profile.clone(),
        audio_path: ready.audio_path.clone(),
        audio_sha256: metadata.audio_sha256.clone(),
        audio_format: metadata.format.clone(),
        audio_size_bytes: metadata.size_bytes,
        audio_duration_ms: metadata.duration_ms,
        sample_rate: metadata.sample_rate,
        bitrate: metadata.bitrate,
        channel: metadata.channel,
        trace_id: metadata.trace_id.clone(),
        usage_characters: metadata.provider_usage_characters,
        warnings,
    }
}

fn load_or_init_state(
    cache: &ClipCache,
    clip_id: &str,
    selection: &crate::speech::clip::SpeechContentSelection,
    now: DateTime<Utc>,
) -> Result<ClipState, GenerationError> {
    if let Some(state) = cache.load_state(clip_id).map_err(storage_error)? {
        return Ok(state);
    }
    let state = ClipState {
        schema_version: CLIP_STATE_SCHEMA_VERSION,
        clip_id: clip_id.to_string(),
        asset_id: selection.asset_id.clone(),
        annotation_id: selection.annotation_id.clone(),
        content_kind: selection.content_kind,
        text_sha256: selection.text.text_sha256.clone(),
        current_cache_status: crate::speech::cache::ClipCacheStatus::Absent,
        current_audio_sha256: None,
        latest_attempt_id: None,
        latest_attempt_status: None,
        latest_error_code: None,
        generation_blocked: false,
        updated_at: now.to_rfc3339(),
    };
    cache.save_state(&state).map_err(storage_error)?;
    Ok(state)
}

/// 把 adapter 失败映射成产品错误与 attempt 终态。
///
/// transport 失败是「不确定」：请求可能已到达供应商，因此记 unknown gate 并且不自动重放；
/// provider 成功但音频不可用记 `provider_succeeded_artifact_missing`，同样阻塞普通重放。
fn map_provider_error(
    error: &SenseAudioError,
    attempt_id: &str,
) -> (GenerationError, AttemptStatus) {
    match error {
        SenseAudioError::MissingApiKey | SenseAudioError::AuthenticationFailed => (
            GenerationError::MissingApiKey,
            AttemptStatus::ProviderFailed,
        ),
        SenseAudioError::RateLimited => (
            GenerationError::ProviderFailed {
                code: "SPEECH_RATE_LIMITED",
                trace_id: None,
                provider_code: None,
                attempt_id: attempt_id.to_string(),
            },
            AttemptStatus::ProviderFailed,
        ),
        SenseAudioError::ProviderFailed | SenseAudioError::InvalidResponse => (
            GenerationError::ProviderFailed {
                code: "SPEECH_PROVIDER_FAILED",
                trace_id: None,
                provider_code: None,
                attempt_id: attempt_id.to_string(),
            },
            AttemptStatus::ProviderFailed,
        ),
        SenseAudioError::Transport => (
            GenerationError::Unknown {
                trace_id: None,
                provider_code: None,
                attempt_id: attempt_id.to_string(),
                message: error.to_string(),
            },
            AttemptStatus::Unknown,
        ),
        SenseAudioError::InvalidAudio => (
            GenerationError::AudioInvalid {
                attempt_id: attempt_id.to_string(),
                trace_id: None,
                message: error.to_string(),
            },
            AttemptStatus::ProviderSucceededArtifactMissing,
        ),
    }
}

/// 记录 attempt history，并把 clip 置为阻塞态（普通 generate 不得自动重放）。
#[allow(clippy::too_many_arguments)]
fn record_attempt_and_gate(
    cache: &ClipCache,
    attempt_id: &str,
    clip_id: &str,
    profile: &VoiceProfile,
    selection: &crate::speech::clip::SpeechContentSelection,
    started_at: &str,
    now: DateTime<Utc>,
    status: AttemptStatus,
    product_error_code: &str,
    provider_code: Option<String>,
    trace_id: Option<String>,
    state: &mut ClipState,
) {
    let attempt = attempt_record(
        attempt_id,
        clip_id,
        profile,
        selection,
        started_at,
        now,
        status,
        None,
        Some(product_error_code),
        provider_code,
        trace_id,
    );
    let _ = cache.record_attempt(&attempt, now);
    state.generation_blocked = true;
    state.latest_attempt_status = Some(status);
    state.latest_error_code = Some(product_error_code.to_string());
    state.updated_at = now.to_rfc3339();
    let _ = cache.save_state_only(state);
}

fn attempt_record(
    attempt_id: &str,
    clip_id: &str,
    profile: &VoiceProfile,
    selection: &crate::speech::clip::SpeechContentSelection,
    started_at: &str,
    now: DateTime<Utc>,
    status: AttemptStatus,
    provider_usage_characters: Option<u64>,
    product_error_code: Option<&str>,
    provider_code: Option<String>,
    trace_id: Option<String>,
) -> AttemptRecord {
    AttemptRecord {
        schema_version: ATTEMPT_SCHEMA_VERSION,
        attempt_id: attempt_id.to_string(),
        clip_id: clip_id.to_string(),
        provider: profile.provider.clone(),
        model: profile.model.clone(),
        voice_id: profile.voice_id.clone(),
        started_at: started_at.to_string(),
        finished_at: Some(now.to_rfc3339()),
        status,
        unicode_characters: selection.text.unicode_characters,
        estimated_billing_characters: selection.text.estimated_billing_characters,
        provider_usage_characters,
        product_error_code: product_error_code.map(str::to_string),
        provider_code,
        trace_id,
    }
}

fn audio_invalid_message(error: &AudioDecodeError) -> String {
    format!(
        "SenseAudio returned audio that cannot be accepted: {}.",
        error.message()
    )
}

fn storage_error(error: ClipCacheError) -> GenerationError {
    match error {
        ClipCacheError::Unavailable { path, message } => {
            GenerationError::Storage(SpeechStoreError::Unavailable { path, message })
        }
        ClipCacheError::Corrupt { path, reason } => {
            GenerationError::Storage(SpeechStoreError::Unavailable {
                path,
                message: format!("the Speech cache entry is {reason}"),
            })
        }
        ClipCacheError::UnsupportedSchemaVersion { path, version } => {
            GenerationError::Storage(SpeechStoreError::Unavailable {
                path,
                message: format!("unsupported Speech cache schema version {version}"),
            })
        }
    }
}

/// 调用 provider 前确认 Speech 根目录可写。
fn ensure_storage_writable(cache: &ClipCache) -> Result<(), GenerationError> {
    let root = cache.root().to_path_buf();
    std::fs::create_dir_all(&root).map_err(|error| {
        GenerationError::Storage(SpeechStoreError::Unavailable {
            path: root.clone(),
            message: error.to_string(),
        })
    })?;
    let probe = root.join(".write-probe");
    std::fs::write(&probe, b"probe").map_err(|error| {
        GenerationError::Storage(SpeechStoreError::Unavailable {
            path: root.clone(),
            message: error.to_string(),
        })
    })?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// attempt ID：同一 clip 的每次真实请求都得到独立 opaque ID。
///
/// 只由本地非秘密材料派生：clip ID、请求时刻、进程 ID 与进程内单调序号，
/// 因此同一毫秒内的多次请求也不会撞号。
fn new_attempt_id(clip_id: &str, now: DateTime<Utc>) -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let seed = format!(
        "{clip_id}:{}:{}:{sequence}",
        now.to_rfc3339(),
        std::process::id()
    );
    let digest = sha256_hex(seed.as_bytes());
    format!("attempt-{}", &digest[..24])
}

/// 生成用的默认 Profile 描述，供诊断与文档使用。
pub const DEFAULT_GENERATION_VOICE_ID: &str = DEFAULT_VOICE_ID;
/// 生成用的默认模型。
pub const DEFAULT_GENERATION_MODEL: &str = DEFAULT_MODEL;
/// 默认 Profile 的 provider。
pub const DEFAULT_GENERATION_PROVIDER: &str = SENSEAUDIO_PROVIDER;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speech::catalog::CatalogVoice;
    use crate::speech::profile::AudioSettings;
    use crate::speech::store::SpeechStore;
    use crate::speech::SpeechProfileDto;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-11T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc)
    }

    fn annotations() -> Vec<Annotation> {
        vec![
            Annotation {
                id: "annotation-41".to_string(),
                asset_id: "book-1".to_string(),
                selected_text: Some("高亮正文".to_string()),
                note: Some("我的笔记".to_string()),
                location: Some("epubcfi(/6/2)".to_string()),
                annotation_type: 3,
                creation_date: Some(0.0),
            },
            Annotation {
                id: "annotation-42".to_string(),
                asset_id: "book-1".to_string(),
                selected_text: Some("只有高亮".to_string()),
                note: None,
                location: None,
                annotation_type: 3,
                creation_date: Some(0.0),
            },
        ]
    }

    fn silent_mp3(frames: usize) -> Vec<u8> {
        let mut bytes = Vec::new();
        for _ in 0..frames {
            bytes.extend_from_slice(&[0xFF, 0xFB, 0x98, 0x0C]);
            let length = 144 * 128_000 / 32_000;
            bytes.extend(std::iter::repeat(0u8).take(length - 4));
        }
        bytes
    }

    fn input(store: SpeechStore, kind: SpeechContentKind) -> GenerationInput {
        GenerationInput {
            store,
            annotations: annotations(),
            request: GenerationRequest {
                asset_id: "book-1".to_string(),
                annotation_id: "annotation-41".to_string(),
                content_kind: Some(kind),
                overrides: GenerationOverrides::default(),
                regenerate: false,
            },
        }
    }

    fn fresh_catalog() -> Vec<CatalogVoice> {
        vec![CatalogVoice {
            source_type: crate::speech::catalog::CatalogSourceType::System,
            voice_id: DEFAULT_VOICE_ID.to_string(),
            voice_name: "Default Voice".to_string(),
            emotion_label: None,
            style_label: None,
            description: vec![],
            created_time: None,
        }]
    }

    fn success_response(trace_id: &str) -> SynthesisResponse {
        success_response_with_frames(trace_id, 2)
    }

    fn success_response_with_frames(trace_id: &str, frames: usize) -> SynthesisResponse {
        SynthesisResponse {
            audio_bytes: silent_mp3(frames),
            trace_id: Some(trace_id.to_string()),
            usage_characters: Some(8),
            audio_length: Some(72),
            audio_sample_rate: Some(32000),
            audio_channel: Some(2),
            audio_format: Some("mp3".to_string()),
            audio_bitrate: Some(128000),
        }
    }

    struct Counters {
        catalog: Arc<AtomicUsize>,
        synthesis: Arc<AtomicUsize>,
    }

    /// 记录 provider 连接数的假 adapter。
    fn fake(
        result: Result<SynthesisResponse, SenseAudioError>,
    ) -> (
        impl FnOnce() -> std::future::Ready<Result<Vec<CatalogVoice>, SenseAudioError>>,
        impl FnOnce(
            SpeechSynthesisRequest,
        ) -> std::future::Ready<Result<SynthesisResponse, SenseAudioError>>,
        Counters,
    ) {
        let catalog = Arc::new(AtomicUsize::new(0));
        let synthesis = Arc::new(AtomicUsize::new(0));
        let catalog_counter = Arc::clone(&catalog);
        let synthesis_counter = Arc::clone(&synthesis);
        let catalog_fetch = move || {
            catalog_counter.fetch_add(1, Ordering::SeqCst);
            std::future::ready(Ok(fresh_catalog()))
        };
        let synthesize = move |_request: SpeechSynthesisRequest| {
            synthesis_counter.fetch_add(1, Ordering::SeqCst);
            std::future::ready(result.clone())
        };
        (catalog_fetch, synthesize, Counters { catalog, synthesis })
    }

    #[tokio::test]
    async fn a_successful_generation_stores_an_immutable_version_and_points_at_it() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace-42")));

        let outcome = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("generate");

        assert_eq!(outcome.source, SpeechClipSource::Provider);
        assert!(outcome.source.provider_called());
        assert!(
            outcome.attempt_id.is_some(),
            "a real provider request must create an attempt id"
        );
        assert_eq!(outcome.asset_id, "book-1");
        assert_eq!(outcome.annotation_id, "annotation-41");
        assert_eq!(outcome.content_kind, SpeechContentKind::Highlight);
        assert_eq!(outcome.billing.unicode_characters, 4);
        assert_eq!(outcome.sample_rate, 32000);
        assert_eq!(outcome.bitrate, 128000);
        assert_eq!(outcome.channel, 2);
        assert_eq!(outcome.trace_id.as_deref(), Some("trace-42"));
        assert_eq!(outcome.usage_characters, Some(8));
        assert!(
            outcome.audio_path.exists(),
            "the immutable audio version must exist"
        );
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 1);
        assert_eq!(counters.catalog.load(Ordering::SeqCst), 1);

        let receipt = serde_json::to_value(outcome.to_receipt()).expect("receipt");
        assert_eq!(receipt["operation"], "generate");
        assert_eq!(receipt["source"], "provider");
        assert_eq!(receipt["provider_called"], true);
        assert_eq!(receipt["content_kind"], "highlight");
        assert_eq!(receipt["profile"]["voice_id"], DEFAULT_VOICE_ID);
        assert_eq!(receipt["audio"]["sample_rate"], 32000);
        assert_eq!(receipt["provider"]["trace_id"], "trace-42");
        assert!(!serde_json::to_string(&receipt)
            .expect("receipt text")
            .contains("高亮正文"));
    }

    #[tokio::test]
    async fn a_repeated_request_reuses_the_cache_without_another_provider_call() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let (catalog_fetch, synthesize, _) = fake(Ok(success_response("trace-1")));
        let first = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("first generation");

        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace-2")));
        let second = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("cache hit");

        assert_eq!(second.source, SpeechClipSource::Cache);
        assert!(!second.source.provider_called());
        assert_eq!(second.clip_id, first.clip_id);
        assert_eq!(second.attempt_id, None);
        assert_eq!(second.audio_sha256, first.audio_sha256);
        assert_eq!(
            counters.synthesis.load(Ordering::SeqCst),
            0,
            "a cache hit must not call the provider"
        );
        assert_eq!(counters.catalog.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn highlight_and_note_are_two_independent_cached_clips() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let (catalog_fetch, synthesize, _) = fake(Ok(success_response("trace-h")));
        let highlight = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("highlight");

        let (catalog_fetch, synthesize, _) = fake(Ok(success_response("trace-n")));
        let note = generate_clip(
            input(store.clone(), SpeechContentKind::Note),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("note");

        assert_ne!(highlight.clip_id, note.clip_id);
        assert_ne!(highlight.text_sha256, note.text_sha256);
    }

    #[tokio::test]
    async fn local_failures_never_reach_the_provider() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());

        // 缺 key：0 次连接。
        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace")));
        let error = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            None,
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("missing key");
        assert_eq!(error.machine_code(), "SPEECH_AUTH_FAILED");
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 0);
        assert_eq!(counters.catalog.load(Ordering::SeqCst), 0);

        // 归属错误、缺失内容、超长文本、参数冲突、Profile 无效：全部在联网前失败。
        let mut foreign = input(store.clone(), SpeechContentKind::Highlight);
        foreign.request.annotation_id = "annotation-999".to_string();
        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace")));
        let error = generate_clip(
            foreign,
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("foreign annotation");
        assert_eq!(error.machine_code(), "INVALID_ANNOTATION_ID");
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 0);

        let mut note_only = input(store.clone(), SpeechContentKind::Note);
        note_only.request.annotation_id = "annotation-42".to_string();
        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace")));
        let error = generate_clip(
            note_only,
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("highlight-only annotation has no note");
        assert_eq!(error.machine_code(), "SPEECH_CONTENT_UNAVAILABLE");
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 0);

        let mut too_long = input(store.clone(), SpeechContentKind::Highlight);
        too_long.annotations[0].selected_text = Some("字".repeat(10_001));
        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace")));
        let error = generate_clip(
            too_long,
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("too long");
        assert_eq!(error.machine_code(), "SPEECH_TEXT_TOO_LONG");
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 0);

        let mut missing_kind = input(store.clone(), SpeechContentKind::Highlight);
        missing_kind.request.content_kind = None;
        let (catalog_fetch, synthesize, _) = fake(Ok(success_response("trace")));
        let error = generate_clip(
            missing_kind,
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("missing content kind");
        assert_eq!(error.machine_code(), "INVALID_ARGUMENT");

        let mut invalid_profile = input(store.clone(), SpeechContentKind::Highlight);
        invalid_profile.request.overrides.speed = Some("2.5".to_string());
        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace")));
        let error = generate_clip(
            invalid_profile,
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("invalid profile");
        assert_eq!(error.machine_code(), "SPEECH_PROFILE_INVALID");
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 0);
        assert_eq!(counters.catalog.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_fresh_catalog_without_the_voice_fails_before_the_request() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let mut request = input(store.clone(), SpeechContentKind::Highlight);
        request.request.overrides.voice_id = Some("missing_voice".to_string());
        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace")));

        let error = generate_clip(
            request,
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("voice unavailable");

        assert_eq!(error.machine_code(), "SPEECH_VOICE_UNAVAILABLE");
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn an_explicit_provider_failure_blocks_the_clip_and_never_retries() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let (catalog_fetch, synthesize, counters) = fake(Err(SenseAudioError::ProviderFailed));

        let error = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("provider failed");

        assert_eq!(error.machine_code(), "SPEECH_PROVIDER_FAILED");
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 1);

        // 普通 generate 不得自动重放：必须显式 --regenerate。
        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace")));
        let blocked = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("still blocked");
        assert_eq!(blocked.machine_code(), "SPEECH_RESULT_UNKNOWN");
        assert!(blocked.blocks_generation());
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn an_uncertain_outcome_records_an_unknown_gate_and_blocks_retry() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let (catalog_fetch, synthesize, counters) = fake(Err(SenseAudioError::Transport));

        let error = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("unknown");

        assert_eq!(error.machine_code(), "SPEECH_RESULT_UNKNOWN");
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 1);

        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace")));
        let blocked = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("unknown gate");
        assert_eq!(blocked.machine_code(), "SPEECH_RESULT_UNKNOWN");
        assert_eq!(
            counters.synthesis.load(Ordering::SeqCst),
            0,
            "an unknown result must never be replayed automatically"
        );

        // 只有显式 --regenerate 才能越过 unknown gate。
        let mut regenerate = input(store.clone(), SpeechContentKind::Highlight);
        regenerate.request.regenerate = true;
        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace-new")));
        let outcome = generate_clip(
            regenerate,
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("regenerate");
        assert_eq!(outcome.source, SpeechClipSource::Provider);
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn invalid_audio_is_a_blocking_artifact_missing_outcome() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let mut response = success_response("trace-bad");
        response.audio_bytes = br#"{"base_resp":{"status_code":0}}"#.to_vec();
        let (catalog_fetch, synthesize, counters) = fake(Ok(response));

        let error = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("invalid audio");

        assert_eq!(error.machine_code(), "SPEECH_AUDIO_INVALID");
        assert!(error.blocks_generation());
        assert_eq!(
            error.attempt_status(),
            Some(AttemptStatus::ProviderSucceededArtifactMissing)
        );
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 1);

        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace-ok")));
        let blocked = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("blocked");
        assert_eq!(blocked.machine_code(), "SPEECH_RESULT_UNKNOWN");
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn regenerate_replaces_the_version_atomically_and_keeps_one_current_pointer() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let (catalog_fetch, synthesize, _) = fake(Ok(success_response("trace-1")));
        let first = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("first");

        let mut regenerate = input(store.clone(), SpeechContentKind::Highlight);
        regenerate.request.regenerate = true;
        // 不同的音频内容 → 新的不可变 version 目录。
        let (catalog_fetch, synthesize, _) = fake(Ok(success_response_with_frames("trace-2", 3)));
        let second = generate_clip(
            regenerate,
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("regenerate");

        assert_eq!(
            first.clip_id, second.clip_id,
            "--regenerate keeps the clip identity"
        );
        assert_ne!(
            first.attempt_id, second.attempt_id,
            "each real request gets a new attempt id"
        );
        let cache = ClipCache::new(store.clone());
        let ready = cache
            .load_ready_clip(&first.clip_id)
            .expect("load")
            .expect("ready");
        assert_eq!(
            ready.state.current_audio_sha256.as_deref(),
            Some(second.audio_sha256.as_str())
        );
        assert_eq!(ready.state.generation_blocked, false);
        // 旧 version 仍由用户/应用所有，不被静默删除。
        let versions = cache
            .clip_dir(&first.clip_id)
            .expect("clip dir")
            .join("versions");
        assert_eq!(std::fs::read_dir(versions).expect("versions").count(), 2);
    }

    #[tokio::test]
    async fn a_corrupt_cache_is_replaced_with_a_warning() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let (catalog_fetch, synthesize, _) = fake(Ok(success_response("trace-1")));
        let first = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("first");
        std::fs::write(&first.audio_path, b"tampered").expect("tamper");

        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace-2")));
        let outcome = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("regenerate over a corrupt entry");

        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 1);
        assert_eq!(outcome.warnings.len(), 1);
        assert_eq!(outcome.warnings[0].code, "SPEECH_CACHE_CORRUPT");
        assert_eq!(outcome.warnings[0].reason, "corrupt_cache_entry");
    }

    #[tokio::test]
    async fn a_corrupt_cache_is_never_returned_as_a_valid_cache_hit() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let (catalog_fetch, synthesize, _) = fake(Ok(success_response("trace-1")));
        let first = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("first");
        std::fs::remove_file(&first.audio_path).expect("remove audio");

        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace-2")));
        let outcome = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("a missing audio file must not be a cache hit");

        assert_eq!(outcome.source, SpeechClipSource::Provider);
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 1);
        assert_eq!(outcome.warnings[0].code, "SPEECH_CACHE_CORRUPT");
    }

    #[tokio::test]
    async fn rate_limited_is_a_stable_error_without_retry() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let (catalog_fetch, synthesize, counters) = fake(Err(SenseAudioError::RateLimited));

        let error = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("rate limited");

        assert_eq!(error.machine_code(), "SPEECH_RATE_LIMITED");
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn attempt_history_holds_metadata_only() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let (catalog_fetch, synthesize, _) = fake(Ok(success_response("trace-1")));
        generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("generate");

        let attempts_dir = store.root().join("attempts");
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&attempts_dir).expect("attempts") {
            for file in std::fs::read_dir(entry.expect("day").path()).expect("files") {
                files.push(file.expect("file").path());
            }
        }
        assert_eq!(files.len(), 1);
        let text = std::fs::read_to_string(&files[0]).expect("attempt record");
        assert!(text.contains("succeeded"));
        assert!(text.contains("male_0004_a"));
        assert!(
            !text.contains("高亮正文"),
            "attempt history must not store Speech Text"
        );
        assert!(
            !text.contains("test-key"),
            "attempt history must not store the API key"
        );
        assert!(
            !text.contains("audio.mp3")
                && !text.contains("audio_bytes")
                && !text.contains("audio_hex"),
            "attempt history must not store audio payloads"
        );
    }

    #[tokio::test]
    async fn a_cached_clip_is_reused_for_a_different_profile_only_after_a_new_attempt() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let (catalog_fetch, synthesize, _) = fake(Ok(success_response("trace-1")));
        let first = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("first");

        let mut faster = input(store.clone(), SpeechContentKind::Highlight);
        faster.request.overrides.speed = Some("1.5".to_string());
        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace-2")));
        let second = generate_clip(
            faster,
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("new profile");

        assert_ne!(
            first.clip_id, second.clip_id,
            "speed changes the clip identity"
        );
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 1);
        assert_ne!(first.audio_path, second.audio_path);
        assert!(second
            .audio_path
            .ends_with(format!("versions/{}/audio.mp3", second.audio_sha256)));
    }

    #[test]
    fn generation_profile_and_audio_settings_stay_fixed_for_v1() {
        assert_eq!(DEFAULT_GENERATION_PROVIDER, SENSEAUDIO_PROVIDER);
        assert_eq!(DEFAULT_GENERATION_MODEL, DEFAULT_MODEL);
        assert_eq!(DEFAULT_GENERATION_VOICE_ID, DEFAULT_VOICE_ID);
        let settings = AudioSettings::v1();
        assert_eq!(settings.format, "mp3");
        assert_eq!(settings.sample_rate, 32_000);
        assert_eq!(settings.bitrate, 128_000);
        assert_eq!(settings.channel, 2);
        assert_eq!(CLIP_VERSION_SCHEMA_VERSION, 1);
        assert_eq!(
            SpeechProfileDto::from(&VoiceProfile::default()).voice_id,
            DEFAULT_VOICE_ID
        );
    }
}
