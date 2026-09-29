//! `speech generate` use case：从一个 Annotation 内容部分得到一个已验证的
//! Cached Speech Clip。
//!
//! 状态流固定（实施 spec 第 9 节）：
//!
//! ```text
//! 解析参数冲突 → 选择内容 → 规范化并校验 Speech Text → 解析 Voice Profile
//!   → 计算 clip_id → 取跨进程 writer 锁
//!   → 有效缓存？        → receipt(source=cache)，0 次 provider 调用
//!   → 已验证的导出音频？ → rehydrate（0 次 provider 调用、0 个 Speech Attempt）
//!                         → receipt(source=export_rehydration)
//!   → unknown gate？     → SPEECH_RESULT_UNKNOWN，不自动重放
//!   → 排队等待方 + provider_failed 终态 → 原样返回首个失败，0 次 provider 调用
//!   → 音色可用性 + API Key + 存储预检
//!   → 落 attempt(in_progress) → 同步合成 → hex 解码 → MP3 校验 → 不可变 version
//!   → 原子切换 pointer → 回填 attempt 终态 → receipt(source=provider)
//! ```
//!
//! 本地失败（内容缺失、归属错误、超长文本、参数冲突、Profile 无效）发生在任何 provider
//! 连接之前；不确定结果与「provider 成功但没有可用产物」都进入阻塞态，只有显式
//! `--regenerate` 才能越过。

use crate::models::Annotation;
use crate::speech::audio::{validate_audio, AudioDecodeError, AudioFacts};
use crate::speech::cache::{
    AttemptRecord, AttemptStatus, ClipCache, ClipCacheError, ClipLock, ClipLockError, ClipState,
    ClipVersionMetadata, ReadyClip, ATTEMPT_SCHEMA_VERSION, CLIP_LOCK_TIMEOUT,
    CLIP_STATE_SCHEMA_VERSION, CLIP_VERSION_SCHEMA_VERSION,
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
use crate::speech::rehydrate::{
    find_verified_exported_clip, ExportedBookIdentity, ExportedClipQuery,
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
use std::time::Duration;

/// `speech generate` 的输入。调用方负责把数据库行与 CLI 参数准备好。
#[derive(Debug, Clone)]
pub struct GenerationInput {
    /// 用户级 Speech 状态根。
    pub store: SpeechStore,
    /// 候选 Annotation（通常是一本书的全部标注）。
    pub annotations: Vec<Annotation>,
    /// 稳定内容身份。
    pub request: GenerationRequest,
    /// 等待同 clip writer 锁的上限；`None` 用生产默认值 [`CLIP_LOCK_TIMEOUT`]。
    ///
    /// 测试与运维可以注入更短的上限来证明 `SPEECH_IN_PROGRESS` 路径；只能缩短，
    /// 不能越过默认上限，避免把「不自动重放」的安全边界拖长。
    pub lock_timeout: Option<Duration>,
    /// 显式给出的书籍导出根（`--export-root`）。缓存没有这个 clip 时，rehydration
    /// 把它当作第一个候选；验证通过后会刷新非权威 locator 投影。
    pub export_root: Option<PathBuf>,
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
///
/// 三种来源对应实施 spec 6.1 的 `source` 取值，机器消费者据此区分「复用本地状态」
/// 与「发生了付费调用」。`Cache` 与 `ExportRehydration` 的 `provider_called` 都是
/// `false`，且 `attempt_id` 为空。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechClipSource {
    /// 复用现有 Speech Cache Entry，没有 provider 调用。
    Cache,
    /// 从已验证的 Exported Speech Clip 恢复缓存：没有 provider 调用，也没有 Speech Attempt。
    ExportRehydration,
    /// 创建了 Speech Attempt。
    Provider,
}

impl SpeechClipSource {
    /// 稳定的机器可读取值。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cache => "cache",
            Self::ExportRehydration => "export_rehydration",
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
        /// 可从 clip state 读到的当前 attempt ID；没有记录时为 `None`。
        ///
        /// 实施 spec 5.4/9：`SPEECH_IN_PROGRESS` 在可用时必须携带当前
        /// `attempt_id`，让等待方能把这个错误对回具体的 Speech Attempt。
        attempt_id: Option<String>,
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
    ///
    /// 锁等待超时也可能带上当前 attempt ID：等待方因此可以把 `SPEECH_IN_PROGRESS`
    /// 对回真正在跑的那个 Speech Attempt。
    pub fn attempt_id(&self) -> Option<&str> {
        match self {
            Self::Unknown { attempt_id, .. }
            | Self::AudioInvalid { attempt_id, .. }
            | Self::ArtifactCommit { attempt_id, .. }
            | Self::ProviderFailed { attempt_id, .. } => Some(attempt_id.as_str()),
            Self::InProgress { attempt_id, .. } => attempt_id.as_deref(),
            _ => None,
        }
    }

    /// 结果语义：`failed` 是明确失败，`unknown` 与
    /// `provider_succeeded_artifact_missing` 都不得自动重放。
    pub const fn outcome(&self) -> &'static str {
        match self {
            Self::Unknown { .. } => "unknown",
            Self::AudioInvalid { .. } | Self::ArtifactCommit { .. } => {
                "provider_succeeded_artifact_missing"
            }
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
            Self::InProgress {
                clip_id,
                attempt_id,
            } => match attempt_id {
                Some(attempt_id) => format!(
                    "another generation for Speech Clip '{clip_id}' is still in progress (Speech Attempt '{attempt_id}')"
                ),
                None => format!(
                    "another generation for Speech Clip '{clip_id}' is still in progress"
                ),
            },
        }
    }

    /// 该失败是否阻止普通 `generate` 自动重放（unknown / provider-success-no-artifact /
    /// provider 成功但本地提交失败）。
    pub const fn blocks_generation(&self) -> bool {
        matches!(
            self,
            Self::Unknown { .. } | Self::AudioInvalid { .. } | Self::ArtifactCommit { .. }
        )
    }

    /// attempt 终态；没有创建 attempt 的本地失败返回 `None`。
    pub fn attempt_status(&self) -> Option<AttemptStatus> {
        match self {
            Self::ProviderFailed { .. } => Some(AttemptStatus::ProviderFailed),
            Self::Unknown { .. } => Some(AttemptStatus::Unknown),
            Self::AudioInvalid { .. } | Self::ArtifactCommit { .. } => {
                Some(AttemptStatus::ProviderSucceededArtifactMissing)
            }
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
        lock_timeout: input_lock_timeout,
        export_root,
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
    let lock_timeout = input_lock_timeout.unwrap_or(CLIP_LOCK_TIMEOUT);
    let lock = ClipLock::acquire_with_timeout(&cache, &id, now, lock_timeout).map_err(|error| {
        match error {
            ClipLockError::InProgress => GenerationError::InProgress {
                clip_id: id.clone(),
                // 锁被占用时只读一次 clip state：把当前 attempt ID 带给等待方。
                // 读不到（还没有 attempt 记录）时留空，绝不让错误消失。
                attempt_id: cache
                    .load_state(&id)
                    .ok()
                    .flatten()
                    .and_then(|state| state.latest_attempt_id),
            },
            ClipLockError::Unavailable(_, error) => {
                GenerationError::Storage(SpeechStoreError::Unavailable {
                    path: cache.locks_dir(),
                    message: error.to_string(),
                })
            }
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
    // 只要进入 attempt 前仍有通过校验的当前版本，失败或不确定的 regenerate 就不能把
    // clip 置为阻塞态：旧音频要继续可播放、可导出，普通 generate 继续复用缓存
    // （实施 spec 7.2「regenerate 失败/unknown：旧 cache 仍 ready」）。
    let has_valid_cache = cached.is_some();
    if let Some(ready) = cached {
        if !request.regenerate {
            // 缓存命中就是一次「使用」：LRU 因此知道这个 entry 最近被读走过。
            cache.touch_clip(&id, now);
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

    // 5b. Speech Cache Rehydration：缓存已被清空或淘汰，但还有一个**已验证**的
    //     Exported Speech Clip 时，把那份字节复制成一个新的已接受 cache version。
    //
    //     这里的位置很重要，它就是实施 spec 第 9 节状态流里的那一格：先 cache，再
    //     export rehydration，最后才允许 provider 请求。同时它解释了为什么
    //     rehydration 能救回一个被 unknown gate 阻塞的 clip：新版本就是证据。
    //
    //     硬保证（ADR 0007「显式 generate 发现缓存已淘汰但存在 checksum 匹配的
    //     Exported Speech Clip 时 …provider_called=false …该动作不创建 Speech
    //     Attempt」）：
    //
    //     - 不检查 API Key、不取 Voice Catalog、不创建 attempt、不发请求；
    //     - `--regenerate` 是唯一可以越过 gate 的入口，因此它**不**参与 rehydration；
    //     - 只复制并重新校验用户导出的字节，绝不把导出目录当成可淘汰缓存，也绝不
    //       修改或删除用户文件；
    //     - 候选只来自显式 `--export-root` 与非权威 locator，绝不扫描用户目录。
    if !request.regenerate {
        let lookup = find_verified_exported_clip(
            &cache,
            &ExportedClipQuery {
                clip_id: id.clone(),
                book: Some(ExportedBookIdentity {
                    asset_id: selection.asset_id.clone(),
                    annotation_id: selection.annotation_id.clone(),
                    content_kind,
                }),
                explicit_root: export_root.clone(),
                // clip_id 本身已经是内容 + Voice Profile 的密码学身份：同一
                // annotation/content kind 下记录过的同一 clip ID 变体就是同一个逻辑
                // clip，因此这里不要求它是 active（play 才要求 active）。
                require_active: false,
            },
            now,
        );
        warnings.extend(lookup.rejection_warnings());
        warnings.extend(lookup.warnings.clone());
        if let Some((clip, origin)) = lookup.found() {
            let outcome =
                rehydrate_from_export(&cache, &state, &id, &selection, &profile, clip, origin, now)
                    .map(|ready| {
                        cache.touch_clip(&id, now);
                        let _ = cache.maintain_budget(&[&id], now);
                        drop(lock);
                        outcome_from_ready(
                            &id,
                            &selection,
                            &profile,
                            &ready,
                            SpeechClipSource::ExportRehydration,
                            None,
                            warnings,
                        )
                    });
            return outcome;
        }
    }

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

    // 6b. 明确的 provider 失败同样是终态：为同一 clip 排过队的等待方必须原样拿到
    //     首个终态错误，绝不能发起第二个 provider 请求——explicit failure 不写 gate
    //     （`blocks_generation == false`），所以只能在这里按状态拦截。
    //     条件由控制流保证：走到这里且没有 `--regenerate` 时一定不存在有效缓存
    //     （有缓存已在第 5 步返回 cache hit）。
    //     ADR 0007「并发、取消与接受证据」与实施 spec 9「failed：返回相同稳定失败，
    //     不自动调用 provider」。
    if lock.waited()
        && !request.regenerate
        && state.latest_attempt_status == Some(AttemptStatus::ProviderFailed)
    {
        drop(lock);
        return Err(recorded_provider_failure(&cache, &state));
    }

    // 7. 调用 provider 前的预检：API Key、存储可写与 budget、音色可用性。
    if api_key.as_deref().unwrap_or_default().trim().is_empty() {
        return Err(GenerationError::MissingApiKey);
    }
    // 7a. budget 预检放在 Voice Catalog 之前：存储不足时连目录请求都不该发出。
    ensure_cache_room(&cache, &id, now)?;
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
    // 8a. attempt 记录先落盘：请求一旦发出可能已经计费，崩溃也必须留下这次请求的
    //     历史（实施 spec 9「create attempt record → call SenseAudio」）。终态、
    //     trace 与用量在返回后按同一 attempt_id 原地更新。
    let mut attempt = pending_attempt(&attempt_id, &id, &profile, &selection, &started_at);
    cache.record_attempt(&attempt, now).map_err(storage_error)?;

    // 8b. 唯一一次同步 provider 请求。
    let response = match synthesize(synthesis_request).await {
        Ok(response) => response,
        Err(error) => {
            let (mapped, status) = map_provider_error(&error, &attempt_id);
            let mut next = state.clone();
            next.latest_attempt_id = Some(attempt_id.clone());
            finish_attempt(
                &mut attempt,
                now,
                status,
                Some(mapped.machine_code()),
                None,
                None,
            );
            record_attempt_and_gate(&cache, &attempt, now, has_valid_cache, &mut next);
            return Err(mapped);
        }
    };

    // 9. 校验音频：hex 已在 adapter 解码，这里校验格式与本地 metadata 一致性。
    //    同时按 spec 8.2 步骤 7 交叉核对供应商声明的音频规格与本地 MP3 帧解析结果。
    let declared = DeclaredAudio::from_response(&response);
    let facts = match validate_audio(&response.audio_bytes, &profile.audio) {
        Ok(facts) if !declared.contradicts(&facts) => facts,
        _ => {
            let mut next = state.clone();
            next.latest_attempt_id = Some(attempt_id.clone());
            finish_attempt(
                &mut attempt,
                now,
                AttemptStatus::ProviderSucceededArtifactMissing,
                Some("SPEECH_AUDIO_INVALID"),
                None,
                response.trace_id.clone(),
            );
            record_attempt_and_gate(&cache, &attempt, now, has_valid_cache, &mut next);
            return Err(GenerationError::AudioInvalid {
                attempt_id,
                trace_id: response.trace_id,
                message: audio_invalid_message(&AudioDecodeError::InvalidCharacter),
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
        attempt_id: Some(attempt_id.clone()),
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
        finish_attempt(
            &mut attempt,
            now,
            AttemptStatus::ProviderSucceededArtifactMissing,
            Some("SPEECH_ARTIFACT_COMMIT_FAILED"),
            None,
            response.trace_id.clone(),
        );
        record_attempt_and_gate(&cache, &attempt, now, has_valid_cache, &mut next);
        return Err(GenerationError::ArtifactCommit {
            attempt_id,
            message: error.message(),
        });
    }

    // 终态回填同一条 attempt 记录：status、trace、用量。
    attempt.status = AttemptStatus::Succeeded;
    attempt.finished_at = Some(now.to_rfc3339());
    attempt.provider_usage_characters = response.usage_characters;
    attempt.trace_id = response.trace_id.clone();
    let _ = cache.record_attempt(&attempt, now);

    // 新 entry 接受后再执行 LRU：把总量压回预算内。当前 clip 由本进程持锁，正在
    // 生成、播放、导出或持锁的 entry 一律保留；压不回去也如实保留（已接受的 entry
    // 不会因为预算压力被丢掉）。
    cache.touch_clip(&id, now);
    let _ = cache.maintain_budget(&[&id], now);

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

/// Speech Cache Rehydration：把一个已验证的 Exported Speech Clip 复制成一个新的已接受
/// cache version（实施 spec 第 9 节状态流第 3 格 / ADR 0007「播放与本地回退」）。
///
/// 复制的字节是**用户导出目录里那份已校验文件**，并且在写入缓存之前再校验一次：本地
/// 音频规格必须与已解析的 Voice Profile 一致（`validate_audio`），否则这份导出音频
/// 不是当前 clip 的可用产物。
///
/// 这里不写任何 attempt：`ClipVersionMetadata::attempt_id` 为 `None`，clip state 的
/// `latest_attempt_id` / `latest_attempt_status` 保持原样（那是过去真实请求的历史，
/// rehydration 不得改写它），因此一次 rehydration 在 `attempts/` 目录下**不留任何文件**。
/// 用户导出的文件全程只读：既不修改也不删除。
fn rehydrate_from_export(
    cache: &ClipCache,
    state: &ClipState,
    clip_id: &str,
    selection: &crate::speech::clip::SpeechContentSelection,
    profile: &VoiceProfile,
    clip: &crate::speech::rehydrate::VerifiedExportedClip,
    origin: crate::speech::rehydrate::ExportCandidateOrigin,
    now: DateTime<Utc>,
) -> Result<ReadyClip, GenerationError> {
    let bytes = std::fs::read(&clip.audio_path).map_err(|error| {
        GenerationError::Storage(SpeechStoreError::Unavailable {
            path: clip.audio_path.clone(),
            message: format!(
                "the verified Exported Speech Clip could not be read for rehydration: {error}"
            ),
        })
    })?;
    // 再次核对：字节仍然是 manifest 记录的那份，且与本地音频规格一致。
    if bytes.is_empty() || crate::speech::text::sha256_hex(&bytes) != clip.audio_sha256 {
        return Err(GenerationError::Storage(SpeechStoreError::Unavailable {
            path: clip.audio_path.clone(),
            message: "the exported audio changed between verification and rehydration".to_string(),
        }));
    }
    let facts = validate_audio(&bytes, &profile.audio).map_err(|error| {
        GenerationError::Storage(SpeechStoreError::Unavailable {
            path: clip.audio_path.clone(),
            message: format!(
                "the exported audio is not a valid Speech Clip for this Voice Profile: {error:?}"
            ),
        })
    })?;

    let metadata = ClipVersionMetadata {
        schema_version: CLIP_VERSION_SCHEMA_VERSION,
        clip_id: clip_id.to_string(),
        audio_sha256: clip.audio_sha256.clone(),
        asset_id: selection.asset_id.clone(),
        annotation_id: selection.annotation_id.clone(),
        content_kind: selection.content_kind,
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
        // 关键：rehydration 没有 Speech Attempt，也没有 provider trace 与用量。
        attempt_id: None,
        trace_id: None,
        provider_usage_characters: None,
        created_at: now.to_rfc3339(),
    };
    let mut next = state.clone();
    next.latest_error_code = None;
    cache
        .commit_version(&next, &metadata, &bytes, now)
        .map_err(|error| GenerationError::Storage(SpeechStoreError::Unavailable {
            path: cache.root().to_path_buf(),
            message: format!(
                "the rehydrated Speech Cache Version could not be committed ({}); the Exported Speech Clip in {} via {} is unchanged",
                error.message(),
                clip.export_root.display(),
                origin.as_str()
            ),
        }))?;
    cache
        .load_ready_clip(clip_id)
        .map_err(|error| {
            GenerationError::Storage(SpeechStoreError::Unavailable {
                path: cache.root().to_path_buf(),
                message: error.message(),
            })
        })?
        .ok_or_else(|| {
            GenerationError::Storage(SpeechStoreError::Unavailable {
                path: cache.root().to_path_buf(),
                message: "the rehydrated cache version could not be read back".to_string(),
            })
        })
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
        last_used_at: Some(now.to_rfc3339()),
    };
    cache.save_state(&state).map_err(storage_error)?;
    Ok(state)
}

/// 把 adapter 失败映射成产品错误与 attempt 终态。
///
/// transport 失败是「不确定」：请求可能已到达供应商，因此记 unknown gate 并且不自动重放；
/// 供应商 `extra_info` 声明的音频规格（实施 spec 8.2 步骤 7）。
///
/// 声明缺失不算矛盾；声明存在但与本地的 MP3 帧解析结果不一致才算矛盾，
/// 此时产物必须被拒绝，不能进入不可变 version。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DeclaredAudio {
    /// 声明的采样率（Hz）。
    sample_rate: Option<u32>,
    /// 声明的声道数。
    channel: Option<u32>,
    /// 声明的码率（bps）。
    bitrate: Option<u32>,
    /// 声明的音频格式。
    format: Option<String>,
}

impl DeclaredAudio {
    /// 从成功响应里取出供应商声明；只保留音频规格，不保存原始响应体。
    fn from_response(response: &SynthesisResponse) -> Self {
        Self {
            sample_rate: response.audio_sample_rate,
            channel: response.audio_channel,
            bitrate: response.audio_bitrate,
            format: response.audio_format.clone(),
        }
    }

    /// 是否有任何字段与本地解析到的 MP3 事实矛盾。
    fn contradicts(&self, facts: &AudioFacts) -> bool {
        if let Some(sample_rate) = self.sample_rate {
            if sample_rate != facts.sample_rate {
                return true;
            }
        }
        if let Some(channel) = self.channel {
            if channel != facts.channel {
                return true;
            }
        }
        if let Some(bitrate) = self.bitrate {
            if bitrate != facts.bitrate {
                return true;
            }
        }
        if let Some(format) = &self.format {
            if format.as_str() != facts.format {
                return true;
            }
        }
        false
    }
}

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

/// 记录 attempt history，并按终态决定是否把 clip 置为阻塞态（普通 generate 不得自动重放）。
///
/// 只有「结果不确定」与「provider 成功但没有可用产物」才阻塞；显式 provider 失败是
/// 确定的终态，交给调用方原样返回，普通 generate 可以再次尝试，不需要 `--regenerate`。
/// `keep_current_cache` 表示仍有通过校验的当前音频版本：此时即使新 attempt 失败或
/// 不确定，也只是记录 latest attempt，不解除也不设置阻塞门——旧版本继续可用，
/// 普通 generate 继续复用缓存（实施 spec 7.2）。
///
/// 同一 clip 的排队等待方不在这里处理：它在锁内按 `provider_failed` 终态原样返回
/// 首个错误（[`recorded_provider_failure`]），不创建新 attempt。
fn record_attempt_and_gate(
    cache: &ClipCache,
    attempt: &AttemptRecord,
    now: DateTime<Utc>,
    keep_current_cache: bool,
    state: &mut ClipState,
) {
    let _ = cache.record_attempt(attempt, now);
    if attempt.status.blocks_generation() && !keep_current_cache {
        state.generation_blocked = true;
    }
    state.latest_attempt_status = Some(attempt.status);
    if let Some(code) = attempt.product_error_code.clone() {
        state.latest_error_code = Some(code);
    }
    state.updated_at = now.to_rfc3339();
    let _ = cache.save_state_only(state);
}

/// 把 attempt 记录推进到终态；`finished_at` 用同一个 `now`，保持记录自洽。
fn finish_attempt(
    record: &mut AttemptRecord,
    now: DateTime<Utc>,
    status: AttemptStatus,
    product_error_code: Option<&str>,
    provider_code: Option<String>,
    trace_id: Option<String>,
) {
    record.status = status;
    record.finished_at = Some(now.to_rfc3339());
    record.product_error_code = product_error_code.map(str::to_string);
    record.provider_code = provider_code;
    record.trace_id = trace_id;
}

/// 排队等待方拿到的首个明确 provider 失败：原样返回已记录的终态错误。
///
/// 优先从 attempt history 还原（错误码、provider code、trace ID、attempt ID），
/// 记录已被清理时退回 `state.json` 里的 `latest_error_code` / `latest_attempt_id`。
/// 不创建 attempt、不调用 provider——同一 clip 的第二个请求可能已经被计费过一次。
fn recorded_provider_failure(cache: &ClipCache, state: &ClipState) -> GenerationError {
    let recorded = state
        .latest_attempt_id
        .as_deref()
        .and_then(|attempt_id| cache.load_attempt(attempt_id).ok().flatten());
    let code = recorded
        .as_ref()
        .and_then(|record| record.product_error_code.clone())
        .or_else(|| state.latest_error_code.clone())
        .unwrap_or_else(|| "SPEECH_PROVIDER_FAILED".to_string());
    let attempt_id = recorded
        .as_ref()
        .map(|record| record.attempt_id.clone())
        .or_else(|| state.latest_attempt_id.clone())
        .unwrap_or_default();
    GenerationError::ProviderFailed {
        code: stable_failure_code(&code),
        trace_id: recorded.as_ref().and_then(|record| record.trace_id.clone()),
        provider_code: recorded.and_then(|record| record.provider_code),
        attempt_id,
    }
}

/// 明确失败的稳定错误码集合：auth / rate limit / provider。
///
/// `ProviderFailed::code` 是 `&'static str`，而记录里读回来的是 `String`；未知值按
/// provider 明确失败返回，绝不把不确定结果伪装成已知失败。
fn stable_failure_code(code: &str) -> &'static str {
    match code {
        "SPEECH_AUTH_FAILED" => "SPEECH_AUTH_FAILED",
        "SPEECH_RATE_LIMITED" => "SPEECH_RATE_LIMITED",
        _ => "SPEECH_PROVIDER_FAILED",
    }
}

/// provider 调用前的 attempt 记录：只有身份与计费估算，`status = in_progress`。
///
/// 先落盘再调用（实施 spec 9「create attempt record → call SenseAudio」）：
/// 请求一旦发出可能已经计费，崩溃也必须留下这次请求的历史。
fn pending_attempt(
    attempt_id: &str,
    clip_id: &str,
    profile: &VoiceProfile,
    selection: &crate::speech::clip::SpeechContentSelection,
    started_at: &str,
) -> AttemptRecord {
    AttemptRecord {
        schema_version: ATTEMPT_SCHEMA_VERSION,
        attempt_id: attempt_id.to_string(),
        clip_id: clip_id.to_string(),
        provider: profile.provider.clone(),
        model: profile.model.clone(),
        voice_id: profile.voice_id.clone(),
        started_at: started_at.to_string(),
        finished_at: None,
        status: AttemptStatus::InProgress,
        unicode_characters: selection.text.unicode_characters,
        estimated_billing_characters: selection.text.estimated_billing_characters,
        provider_usage_characters: None,
        product_error_code: None,
        provider_code: None,
        trace_id: None,
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

/// 调用 provider 前的存储预检（实施 spec 7.4）。
///
/// 依次确认：Speech 根可写；配置预算留得出安全余量（`budget - 128 MiB`）；文件系统
/// 报告的可用空间保得住安全余量；清理已过 budget 且可淘汰的旧 entry（含确认没有
/// lock/reference 的 orphan version）之后仍然在预算内。任一条不成立都在**付费之前**
/// 本地返回 `SPEECH_STORAGE_UNAVAILABLE`——先计费再发现无处落盘是不可接受的。
fn ensure_cache_room(
    cache: &ClipCache,
    clip_id: &str,
    now: DateTime<Utc>,
) -> Result<(), GenerationError> {
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

    let budget = cache.cache_budget_bytes().map_err(storage_error)?;
    let usable = crate::speech::usable_cache_budget(budget);
    if usable == 0 {
        // 预算连安全余量都留不出：这不是运行时压力，是配置问题，本地失败。
        return Err(GenerationError::Storage(SpeechStoreError::Unavailable {
            path: root.clone(),
            message: format!(
                "the Speech cache budget of {budget} bytes leaves no room above the {} byte safety margin",
                crate::speech::CACHE_SAFETY_MARGIN_BYTES
            ),
        }));
    }
    let required = crate::speech::cache::required_free_bytes();
    if let Some(available) = crate::speech::cache::available_bytes(&root) {
        if available < required {
            return Err(GenerationError::Storage(SpeechStoreError::Unavailable {
                path: root.clone(),
                message: format!(
                    "only {available} bytes are available on the Speech cache volume; {required} bytes must stay free"
                ),
            }));
        }
    }

    // 写入前先清理已过 budget 且可淘汰的旧 entry；当前 clip 由调用方持锁，不参与淘汰。
    let maintenance = cache
        .maintain_budget(&[clip_id], now)
        .map_err(storage_error)?;
    if !maintenance.safety_margin_preserved {
        let held = if maintenance.skipped.is_empty() {
            "none".to_string()
        } else {
            maintenance.skipped.join(", ")
        };
        return Err(GenerationError::Storage(SpeechStoreError::Unavailable {
            path: root.clone(),
            message: format!(
                "the Speech cache still holds {} bytes of clips in use ({held}) and cannot keep {} bytes free",
                maintenance.used_bytes_after, crate::speech::CACHE_SAFETY_MARGIN_BYTES
            ),
        }));
    }
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
    use serde_json::Value;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

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
            lock_timeout: None,
            export_root: None,
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

    /// 默认 fixture 请求的 clip ID（与本地测试输入一致）。
    fn clip_id_of(kind: SpeechContentKind) -> String {
        clip_id(&ClipFingerprint {
            provider: SENSEAUDIO_PROVIDER,
            model: DEFAULT_MODEL,
            asset_id: "book-1",
            annotation_id: "annotation-41",
            content_kind: kind,
            normalized_speech_text: "高亮正文",
            voice_id: DEFAULT_VOICE_ID,
            speed_x100: 100,
            volume_x100: 100,
            pitch: 0,
            audio: &AudioSettings::v1(),
        })
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
    async fn an_explicit_provider_failure_never_masquerades_as_an_unknown_result() {
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
        assert_eq!(error.reason_code(), "provider_failed");
        assert_eq!(error.outcome(), "failed");
        assert!(
            !error.blocks_generation(),
            "an explicit failure is a known outcome"
        );
        assert_eq!(
            error.attempt_status(),
            Some(AttemptStatus::ProviderFailed),
            "an explicit failure must not be recorded as provider-succeeded-artifact-missing"
        );
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 1);

        // 普通 generate 不得被 unknown gate 挡住，也不需要 --regenerate。
        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace")));
        let retried = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("a plain generate may retry after an explicit failure");
        assert_eq!(retried.source, SpeechClipSource::Provider);
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 1);
    }

    /// 排队等待方必须在锁内原样拿到首个明确失败：0 次 provider 调用、0 次目录请求，
    /// 且 attempt ID 就是首个 attempt——同一 clip 的第二个请求可能已经被计费过一次。
    #[tokio::test]
    async fn a_queued_waiter_receives_the_first_provider_failure_without_a_second_call() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let (catalog_fetch, synthesize, counters) = fake(Err(SenseAudioError::ProviderFailed));

        let first = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("the first attempt fails explicitly");
        assert_eq!(first.machine_code(), "SPEECH_PROVIDER_FAILED");
        assert!(
            !first.blocks_generation(),
            "an explicit failure is a known outcome"
        );
        let first_attempt = first.attempt_id().map(str::to_string).expect("attempt id");
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 1);

        // 另一个 writer 先持锁：第二个调用因此排队等待，而不是碰巧串行完成。
        // 持锁方用 channel 确认「锁已经在手」后才放行测试，等待方因此一定排队；
        // 锁的释放由一个独立计时器触发，与等待方的返回解耦，避免互相死等。
        let (holder, held) = {
            let cache = ClipCache::new(store.clone());
            let clip_id = clip_id_of(SpeechContentKind::Highlight);
            let (held_sender, held) = std::sync::mpsc::channel();
            let holder = std::thread::spawn(move || {
                let lock =
                    ClipLock::acquire(&cache, &clip_id, now()).expect("hold the writer lock");
                held_sender.send(()).expect("report the held lock");
                std::thread::sleep(std::time::Duration::from_millis(300));
                drop(lock);
            });
            (holder, held)
        };
        held.recv().expect("the writer lock is held");

        let (catalog_fetch, synthesize, waiter_counters) =
            fake(Ok(success_response("trace-waiter")));
        let mut waiting_input = input(store.clone(), SpeechContentKind::Highlight);
        // 持锁方在等待方拿到终态错误后才释放锁，因此把等待上限缩短到秒级：
        // 测试不必真的等生产默认的 20 秒，也不会把「不自动重放」的边界拖长。
        waiting_input.lock_timeout = Some(std::time::Duration::from_secs(5));
        let waited = generate_clip(
            waiting_input,
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("a queued waiter must not start a second provider request");

        assert_eq!(waited.machine_code(), "SPEECH_PROVIDER_FAILED");
        assert_eq!(waited.reason_code(), "provider_failed");
        assert_eq!(waited.outcome(), "failed");
        assert_eq!(
            waited.attempt_id().map(str::to_string).as_deref(),
            Some(first_attempt.as_str()),
            "the waiter must receive the first attempt's terminal error verbatim"
        );
        assert_eq!(
            waiter_counters.synthesis.load(Ordering::SeqCst),
            0,
            "the waiter must never reach the provider"
        );
        assert_eq!(
            waiter_counters.catalog.load(Ordering::SeqCst),
            0,
            "the recorded terminal error is returned before any preflight"
        );

        // 第一位调用方自己的显式重试（没有排队）仍然可以重新生成。
        let (catalog_fetch, synthesize, retry_counters) = fake(Ok(success_response("trace-retry")));
        let retried = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("an explicit generate is a new user action");
        holder.join().expect("join the lock holder");
        assert_eq!(retried.source, SpeechClipSource::Provider);
        assert_ne!(
            retried.attempt_id.as_deref(),
            Some(first_attempt.as_str()),
            "the retry is a new Speech Attempt"
        );
        assert_eq!(retry_counters.synthesis.load(Ordering::SeqCst), 1);
    }

    /// attempt 记录必须先于 provider 调用落盘：调用期间就能读到 `in_progress` 记录，
    /// 返回后按同一 attempt ID 原地更新为终态。
    #[tokio::test]
    async fn the_attempt_record_is_persisted_before_the_provider_call() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let probe_cache = ClipCache::new(store.clone());
        let cache_for_probe = probe_cache.clone();
        let clip_id = clip_id_of(SpeechContentKind::Highlight);
        let in_flight = Arc::new(Mutex::new(String::new()));
        let probe = Arc::clone(&in_flight);

        let catalog_fetch = || std::future::ready(Ok(fresh_catalog()));
        let synthesize = move |_request: SpeechSynthesisRequest| {
            let probe = Arc::clone(&probe);
            let cache = cache_for_probe.clone();
            async move {
                // provider 已经被调用：这次可能计费的请求必须已经有历史记录。
                let mut seen = String::new();
                for day in std::fs::read_dir(cache.attempts_dir())
                    .expect("attempts dir")
                    .flatten()
                {
                    for file in std::fs::read_dir(day.path()).expect("records").flatten() {
                        seen = std::fs::read_to_string(file.path()).expect("attempt record");
                    }
                }
                *probe.lock().expect("probe") = seen;
                Ok(success_response("trace-in-flight"))
            }
        };

        let outcome = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("generate");

        let during: Value =
            serde_json::from_str(&in_flight.lock().expect("probe")).expect("in-flight record");
        assert_eq!(during["status"], "in_progress");
        assert!(
            during["finished_at"].is_null(),
            "an in-flight attempt has no outcome yet"
        );
        assert_eq!(
            during["attempt_id"].as_str(),
            outcome.attempt_id.as_deref(),
            "the in-flight record must name the attempt that is being billed"
        );
        assert_eq!(during["clip_id"].as_str(), Some(clip_id.as_str()));

        let stored = probe_cache
            .load_attempt(outcome.attempt_id.as_deref().expect("attempt id"))
            .expect("load")
            .expect("record");
        assert_eq!(
            stored.status,
            AttemptStatus::Succeeded,
            "the same record must be updated in place with the outcome"
        );
        assert!(stored.finished_at.is_some());
        assert_eq!(stored.provider_usage_characters, Some(8));
        assert_eq!(stored.trace_id.as_deref(), Some("trace-in-flight"));
        assert!(stored.product_error_code.is_none());
        // 一条 attempt 一个文件：原地更新不能留下 second record。
        let files: Vec<PathBuf> = std::fs::read_dir(probe_cache.attempts_dir())
            .expect("attempts dir")
            .flatten()
            .flat_map(|day| {
                std::fs::read_dir(day.path())
                    .expect("records")
                    .flatten()
                    .map(|f| f.path())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(files.len(), 1, "one attempt must stay one history record");
    }

    #[tokio::test]
    async fn a_rate_limited_failure_is_explicit_and_does_not_gate() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let (catalog_fetch, synthesize, _) = fake(Err(SenseAudioError::RateLimited));

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
        assert_eq!(error.outcome(), "failed");
        assert!(!error.blocks_generation());

        let cache = ClipCache::new(store.clone());
        let state = cache
            .load_state(&clip_id_of(SpeechContentKind::Highlight))
            .expect("load state")
            .expect("state");
        assert!(
            !state.generation_blocked,
            "an explicit failure must not write an unknown gate"
        );
        assert_eq!(
            state.latest_attempt_status,
            Some(AttemptStatus::ProviderFailed),
            "an explicit failure must stay an explicit attempt status"
        );
        assert_eq!(
            state.latest_error_code.as_deref(),
            Some("SPEECH_RATE_LIMITED")
        );
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
    async fn declared_audio_metadata_that_contradicts_the_parsed_mp3_is_rejected() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let mut response = success_response("trace-declared");
        // 本地 MP3 帧解析是 32000Hz/128000bps/2 声道：声明值必须与之一致。
        response.audio_sample_rate = Some(44_100);
        let (catalog_fetch, synthesize, counters) = fake(Ok(response));

        let error = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("contradicting declared metadata");

        assert_eq!(error.machine_code(), "SPEECH_AUDIO_INVALID");
        assert_eq!(error.outcome(), "provider_succeeded_artifact_missing");
        assert!(error.blocks_generation());
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 1);
        let cache = ClipCache::new(store.clone());
        let clip_id = clip_id_of(SpeechContentKind::Highlight);
        assert!(
            cache
                .clip_dir(&clip_id)
                .expect("clip dir")
                .join("versions")
                .read_dir()
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(true),
            "a rejected artifact must not create an immutable audio version"
        );
    }

    #[tokio::test]
    async fn declared_audio_metadata_matching_the_parsed_mp3_is_accepted() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let mut response = success_response("trace-declared-ok");
        response.audio_format = Some("mp3".to_string());
        let (catalog_fetch, synthesize, _) = fake(Ok(response));

        let outcome = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("consistent declared metadata");

        assert_eq!(outcome.sample_rate, 32_000);
        assert_eq!(outcome.bitrate, 128_000);
        assert_eq!(outcome.channel, 2);
        assert_eq!(outcome.audio_format, "mp3");
    }

    #[tokio::test]
    async fn missing_declared_audio_metadata_is_not_a_contradiction() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let mut response = success_response("trace-declared-absent");
        response.audio_sample_rate = None;
        response.audio_channel = None;
        response.audio_bitrate = None;
        response.audio_format = None;
        let (catalog_fetch, synthesize, _) = fake(Ok(response));

        let outcome = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("absent declarations must not block a valid artifact");

        assert_eq!(outcome.source, SpeechClipSource::Provider);
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
        assert!(!ready.state.generation_blocked);
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

    /// 读取 clip state；测试里用于构造「已经有过 attempt」的现场。
    fn stored_state(store: &SpeechStore, clip_id: &str) -> ClipState {
        ClipCache::new(store.clone())
            .load_state(clip_id)
            .expect("load state")
            .expect("state exists")
    }

    /// 手动占用 writer 锁文件：模拟另一个进程正在生成（O_EXCL 语义与真实 writer 相同）。
    fn occupy_lock(store: &SpeechStore, clip_id: &str) {
        let cache = ClipCache::new(store.clone());
        std::fs::create_dir_all(cache.locks_dir()).expect("locks dir");
        std::fs::write(
            cache.locks_dir().join(format!("{clip_id}.lock")),
            format!("{}\n", now().to_rfc3339()),
        )
        .expect("occupy lock");
    }

    fn version_count(store: &SpeechStore, clip_id: &str) -> usize {
        ClipCache::new(store.clone())
            .clip_dir(clip_id)
            .expect("clip dir")
            .join("versions")
            .read_dir()
            .map(|entries| entries.count())
            .unwrap_or(0)
    }

    fn attempt_files(store: &SpeechStore) -> Vec<PathBuf> {
        let attempts = store.root().join("attempts");
        let mut files = Vec::new();
        if let Ok(days) = std::fs::read_dir(&attempts) {
            for day in days.flatten() {
                if let Ok(entries) = std::fs::read_dir(day.path()) {
                    files.extend(entries.flatten().map(|entry| entry.path()));
                }
            }
        }
        files.sort();
        files
    }

    #[tokio::test]
    async fn a_lock_timeout_returns_a_stable_in_progress_error_with_the_attempt_id() {
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

        // 另一个进程正在生成同一 clip：锁文件存在且是新鲜的（不是崩溃遗留）。
        occupy_lock(&store, &first.clip_id);

        let mut waiting = input(store.clone(), SpeechContentKind::Highlight);
        waiting.request.regenerate = true;
        waiting.lock_timeout = Some(Duration::from_millis(40));
        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace-2")));
        let error = generate_clip(
            waiting,
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("lock timeout");

        assert_eq!(error.machine_code(), "SPEECH_IN_PROGRESS");
        assert_eq!(error.reason_code(), "in_progress");
        assert_eq!(
            error.attempt_id(),
            first.attempt_id.as_deref(),
            "a lock timeout must carry the current attempt id when it is available"
        );
        assert!(
            error
                .message()
                .contains(first.attempt_id.as_deref().expect("attempt id")),
            "the human message must name the in-progress attempt: {}",
            error.message()
        );
        assert_eq!(
            counters.synthesis.load(Ordering::SeqCst),
            0,
            "waiting for the lock must never start a second provider request"
        );
        assert_eq!(version_count(&store, &first.clip_id), 1);

        // 锁释放后同一请求继续正常命中缓存。
        let cache = ClipCache::new(store.clone());
        std::fs::remove_file(cache.locks_dir().join(format!("{}.lock", first.clip_id)))
            .expect("release lock");
        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace-3")));
        let cached = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("cache hit after the writer finished");
        assert_eq!(cached.source, SpeechClipSource::Cache);
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_failed_regeneration_keeps_the_previous_version_usable_and_ungated() {
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

        // 失败的 regenerate：请求可能已到达 provider，因此结果不确定。
        let mut regenerate = input(store.clone(), SpeechContentKind::Highlight);
        regenerate.request.regenerate = true;
        let (catalog_fetch, synthesize, counters) = fake(Err(SenseAudioError::Transport));
        let error = generate_clip(
            regenerate,
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("uncertain regeneration");
        assert_eq!(error.machine_code(), "SPEECH_RESULT_UNKNOWN");
        assert!(error.blocks_generation());
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 1);

        // 旧 version 仍是 current pointer，且没有留下阻塞门。
        let state = stored_state(&store, &first.clip_id);
        assert_eq!(
            state.current_audio_sha256.as_deref(),
            Some(first.audio_sha256.as_str()),
            "an uncertain regeneration must not move the current pointer"
        );
        assert_eq!(
            state.current_cache_status,
            crate::speech::cache::ClipCacheStatus::Ready
        );
        assert!(
            !state.generation_blocked,
            "a failed regeneration must not gate a clip that still has a valid version"
        );
        assert_eq!(
            state.latest_attempt_status,
            Some(AttemptStatus::Unknown),
            "the failed attempt is still recorded on the clip state"
        );
        assert_eq!(version_count(&store, &first.clip_id), 1);

        // 普通 generate 继续复用旧音频，零 provider 调用。
        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace-3")));
        let replayed = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("cache hit after a failed regeneration");
        assert_eq!(replayed.source, SpeechClipSource::Cache);
        assert_eq!(replayed.audio_sha256, first.audio_sha256);
        assert_eq!(
            counters.synthesis.load(Ordering::SeqCst),
            0,
            "the previous cache version must stay usable without another provider call"
        );
    }

    #[tokio::test]
    async fn every_real_request_creates_a_distinct_metadata_only_attempt_record() {
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

        assert_ne!(
            first.attempt_id, second.attempt_id,
            "each real provider request gets its own attempt id"
        );
        let files = attempt_files(&store);
        assert_eq!(files.len(), 2, "two real requests, two history records");
        let mut recorded = Vec::new();
        for file in &files {
            let text = std::fs::read_to_string(file).expect("attempt record");
            assert!(
                !text.contains("高亮正文"),
                "attempt history must not store the Speech Text"
            );
            assert!(
                !text.contains("test-key"),
                "attempt history must not store the API key"
            );
            for forbidden in ["audio.mp3", "audio_bytes", "audio_hex", "fffb"] {
                assert!(
                    !text.contains(forbidden),
                    "attempt history must not store audio payloads ({forbidden})"
                );
            }
            let record: AttemptRecord = serde_json::from_str(&text).expect("attempt JSON");
            recorded.push(record.attempt_id);
        }
        recorded.sort();
        let mut expected = vec![
            first.attempt_id.clone().expect("first attempt"),
            second.attempt_id.clone().expect("second attempt"),
        ];
        expected.sort();
        assert_eq!(recorded, expected);
    }

    #[tokio::test]
    async fn clearing_the_cache_lifts_the_gate_while_history_clear_keeps_it() {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        let (catalog_fetch, synthesize, _) = fake(Err(SenseAudioError::Transport));
        let error = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("unknown result");
        assert_eq!(error.machine_code(), "SPEECH_RESULT_UNKNOWN");
        let clip_id = clip_id_of(SpeechContentKind::Highlight);
        assert!(stored_state(&store, &clip_id).generation_blocked);

        // history clear 只删 attempt metadata：gate 与 clip state 原样保留。
        let history = crate::speech::clear_speech_history(&store, now()).expect("clear history");
        assert_eq!(history.removed_attempts, 1);
        assert_eq!(
            history.cleared_generation_gates, 0,
            "history clear must never lift a generation gate"
        );
        assert!(stored_state(&store, &clip_id).generation_blocked);
        assert!(attempt_files(&store).is_empty());

        // 普通 generate 仍然被 gate 挡住。
        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace")));
        let blocked = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect_err("still gated");
        assert_eq!(blocked.machine_code(), "SPEECH_RESULT_UNKNOWN");
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 0);

        // cache clear 是显式清除入口：gate 与 clip state 一起消失。
        let cleared = crate::speech::clear_speech_cache(&store, now()).expect("clear cache");
        assert_eq!(cleared.removed, vec![clip_id.clone()]);
        assert!(cleared.skipped.is_empty());
        assert_eq!(cleared.cleared_generation_gates, 1);
        assert!(
            ClipCache::new(store.clone())
                .load_state(&clip_id)
                .expect("load state")
                .is_none(),
            "clearing the cache must remove the clip-level gate"
        );

        // 清除后普通 generate 可以重新创建 attempt。
        let (catalog_fetch, synthesize, counters) = fake(Ok(success_response("trace-new")));
        let outcome = generate_clip(
            input(store.clone(), SpeechContentKind::Highlight),
            Some("test-key".to_string()),
            now(),
            catalog_fetch,
            synthesize,
        )
        .await
        .expect("generate after clearing the gate");
        assert_eq!(outcome.source, SpeechClipSource::Provider);
        assert_eq!(counters.synthesis.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn clearing_the_cache_skips_a_locked_entry() {
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

        // 持锁 entry 不可淘汰：cache clear 必须跳过而不是删掉正在使用的音频。
        occupy_lock(&store, &first.clip_id);
        let cleared = crate::speech::clear_speech_cache(&store, now()).expect("clear cache");
        assert_eq!(cleared.removed, Vec::<String>::new());
        assert_eq!(cleared.skipped, vec![first.clip_id.clone()]);
        assert_eq!(cleared.cleared_generation_gates, 0);
        assert!(
            stored_state(&store, &first.clip_id).current_cache_status
                == crate::speech::cache::ClipCacheStatus::Ready
        );

        // 释放锁之后同一命令就能删掉它。
        let cache = ClipCache::new(store.clone());
        std::fs::remove_file(cache.locks_dir().join(format!("{}.lock", first.clip_id)))
            .expect("release lock");
        let cleared = crate::speech::clear_speech_cache(&store, now()).expect("clear cache");
        assert_eq!(cleared.removed, vec![first.clip_id.clone()]);
    }
}
