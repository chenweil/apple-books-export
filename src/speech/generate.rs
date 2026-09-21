//! `speech generate` use case：把一条 Annotation 的一个内容部分变成一个可缓存的 Speech Clip。
//!
//! 顺序固定（ADR 0007 第 9 节）：
//! 解析内容 → 规范化并校验文本 → 解析并本地校验 Voice Profile → 计算 clip_id →
//! 获取跨进程 clip 锁 → 有效缓存？→ unknown gate？→ 音色/密钥 preflight →
//! 创建 attempt → 同步调用 SenseAudio → 校验音频 → 提交不可变 version → 原子切换 pointer。
//!
//! 只有显式 `generate` 会发送内容并产生付费请求；缓存命中、unknown gate、本地校验失败
//! 都在接触供应商前返回。未知结果与产物缺失绝不自动重放。

use crate::db::DB;
use crate::models::Annotation;
use crate::speech::audio::{validate_mp3, AudioValidation};
use crate::speech::catalog::{verify_voice, VoiceCatalogSource, VoiceVerification};
use crate::speech::clip::{
    escape_control_markup, normalize_speech_text, validate_speech_text_length, ClipFingerprint,
    SpeechTextError, SpeechTextSummary,
};
use crate::speech::profile::{ProfileDraft, ProfileError, ProfileVerification};
use crate::speech::senseaudio::{
    CachedVoiceCatalogSource, SenseAudioClient, SenseAudioError, SynthesisRequest,
};
use crate::speech::store::{
    AttemptRecord, AttemptStatus, ClipState, ClipVersionMetadata, CurrentCacheStatus, SpeechStore,
    SpeechStoreError, ATTEMPT_SCHEMA_VERSION, CLIP_STATE_SCHEMA_VERSION,
};
use crate::speech::{SpeechWarning, VoiceProfile};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;

/// 跨进程 clip 锁等待上限；超时返回 `SPEECH_IN_PROGRESS`。
const CLIP_LOCK_TIMEOUT: Duration = Duration::from_secs(30);

/// `speech generate` 的领域输入。
#[derive(Debug, Clone)]
pub struct GenerateRequest {
    /// 书籍稳定 ID。
    pub asset_id: String,
    /// Annotation 稳定 ID。
    pub annotation_id: String,
    /// 内容种类。
    pub content_kind: crate::speech::clip::SpeechContentKind,
    /// 覆盖音色 ID；`None` 用全局 Profile。
    pub voice_id: Option<String>,
    /// 覆盖语速（原始十进制文本）。
    pub speed: Option<String>,
    /// 覆盖音量（原始十进制文本）。
    pub volume: Option<String>,
    /// 覆盖声调（原始整数文本）。
    pub pitch: Option<String>,
    /// 显式重新生成：越过 unknown gate 并替换有效缓存。
    pub regenerate: bool,
}

/// 生成结果来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerateSource {
    /// 复用现有 Speech Cache Entry。
    Cache,
    /// 创建了 Speech Attempt。
    Provider,
}

impl GenerateSource {
    /// 稳定的机器可读取值。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cache => "cache",
            Self::Provider => "provider",
        }
    }
}

/// 一次生成的领域结果。不含原文、密钥或音频 hex。
#[derive(Debug, Clone)]
pub struct GenerateOutcome {
    /// 完整 clip ID。
    pub clip_id: String,
    /// 本次 attempt ID；缓存命中为 `None`。
    pub attempt_id: Option<String>,
    /// 来源。
    pub source: GenerateSource,
    /// 是否真的调用了供应商。
    pub provider_called: bool,
    /// 书籍稳定 ID。
    pub asset_id: String,
    /// Annotation 稳定 ID。
    pub annotation_id: String,
    /// 内容种类。
    pub content_kind: crate::speech::clip::SpeechContentKind,
    /// 文本摘要（不含原文）。
    pub text_summary: SpeechTextSummary,
    /// 解析后的 Voice Profile。
    pub profile: VoiceProfile,
    /// 本地音频绝对路径。
    pub audio_path: PathBuf,
    /// 音频 SHA-256。
    pub audio_sha256: String,
    /// 音频格式。
    pub audio_format: String,
    /// 音频字节大小。
    pub audio_size_bytes: u64,
    /// 估算时长（毫秒）。
    pub duration_ms: u64,
    /// 采样率。
    pub sample_rate: u32,
    /// 码率。
    pub bitrate: u32,
    /// 声道数。
    pub channel: u32,
    /// 供应商 trace ID。
    pub trace_id: Option<String>,
    /// 供应商返回的实际用量字符数。
    pub usage_characters: Option<u64>,
    /// 结构化 warning。
    pub warnings: Vec<SpeechWarning>,
}

/// 一次供应商调用的失败详情；`kind` 决定稳定的产品错误码与 outcome。
#[derive(Debug, Clone)]
pub struct ProviderFailure {
    /// 供应商失败类型。
    pub kind: SenseAudioError,
    /// 完整 clip ID。
    pub clip_id: String,
    /// 本次 attempt ID；preflight 失败（未发起请求）为 `None`。
    pub attempt_id: Option<String>,
    /// 供应商 trace ID（若有）。
    pub trace_id: Option<String>,
}

/// 生成 use case 的错误。
#[derive(Debug)]
pub enum GenerateError {
    /// Apple Books 数据库读不到。
    Database {
        /// 底层错误说明，不含任何秘密。
        message: String,
    },
    /// 书籍身份不存在。
    AssetMissing {
        /// 被查询的 asset_id。
        asset_id: String,
    },
    /// Annotation 不存在或不属于该书。
    AnnotationMissing {
        /// 被查询的 asset_id。
        asset_id: String,
        /// 被查询的 annotation_id。
        annotation_id: String,
    },
    /// 目标高亮或笔记为空。
    ContentUnavailable {
        /// 请求的内容种类。
        content_kind: crate::speech::clip::SpeechContentKind,
    },
    /// 规范化文本超过 10000 字符。
    TextTooLong {
        /// 规范化后的字符数。
        characters: usize,
    },
    /// Voice Profile 本地校验失败。
    Profile(ProfileError),
    /// voice ID 当前不可用或无法验证。
    VoiceUnavailable {
        /// Speech Provider。
        provider: String,
        /// 被拒绝的音色 ID。
        voice_id: String,
    },
    /// Speech 状态根不可用。
    Storage(SpeechStoreError),
    /// 同 clip writer 锁等待超时。
    InProgress {
        /// 完整 clip ID。
        clip_id: String,
    },
    /// 之前的 attempt 留下阻塞 gate；普通 generate 不得重放。
    Blocked {
        /// 完整 clip ID。
        clip_id: String,
        /// 造成阻塞的 attempt ID。
        attempt_id: Option<String>,
        /// 阻塞终态。
        status: AttemptStatus,
        /// 之前记录的产品错误码。
        error_code: Option<String>,
    },
    /// 供应商调用失败（明确失败、未知、产物缺失、鉴权、限流）。
    Provider(ProviderFailure),
}

impl std::fmt::Display for GenerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database { message } => write!(f, "{message}"),
            Self::AssetMissing { asset_id } => {
                write!(f, "no Apple Books item was found for asset_id '{asset_id}'")
            }
            Self::AnnotationMissing { asset_id, annotation_id } => write!(
                f,
                "annotation '{annotation_id}' was not found for asset_id '{asset_id}'"
            ),
            Self::ContentUnavailable { content_kind } => write!(
                f,
                "the requested {} content is empty for this annotation",
                content_kind.as_str()
            ),
            Self::TextTooLong { characters } => {
                write!(f, "normalized speech text is {characters} characters, over the limit")
            }
            Self::Profile(error) => write!(f, "{}", error.message()),
            Self::VoiceUnavailable { provider, voice_id } => write!(
                f,
                "voice '{voice_id}' is not available for provider '{provider}'"
            ),
            Self::Storage(error) => write!(f, "{error}"),
            Self::InProgress { clip_id } => {
                write!(f, "another generation for clip '{clip_id}' is already in progress")
            }
            Self::Blocked { clip_id, .. } => write!(
                f,
                "clip '{clip_id}' is blocked by an earlier uncertain outcome; retry with --regenerate"
            ),
            Self::Provider(failure) => write!(f, "{}", failure.kind),
        }
    }
}

/// 生成一个 Speech Clip。只有走到供应商调用时才产生付费请求。
pub async fn generate_clip(
    db: &DB,
    store: &SpeechStore,
    client: &SenseAudioClient,
    request: &GenerateRequest,
    now: DateTime<Utc>,
) -> Result<GenerateOutcome, GenerateError> {
    // 1. 解析内容：稳定身份 + 内容种类。
    let annotation = resolve_annotation(db, &request.asset_id, &request.annotation_id)?;
    let raw = request
        .content_kind
        .select(annotation.selected_text.as_deref(), annotation.note.as_deref())
        .ok_or(GenerateError::ContentUnavailable {
            content_kind: request.content_kind,
        })?;

    // 2. 规范化并校验 Speech Text。
    let normalized = normalize_speech_text(raw);
    if normalized.is_empty() {
        return Err(GenerateError::ContentUnavailable {
            content_kind: request.content_kind,
        });
    }
    if let Err(error) = validate_speech_text_length(&normalized) {
        return Err(match error {
            SpeechTextError::TooLong { characters } => GenerateError::TextTooLong { characters },
        });
    }
    let text_summary = SpeechTextSummary::from_normalized(&normalized);

    // 3. 解析并本地校验 Voice Profile（覆盖项合并进全局 Profile）。
    let config = store.load_config().map_err(GenerateError::Storage)?;
    let draft = ProfileDraft {
        voice_id: Some(
            request
                .voice_id
                .clone()
                .unwrap_or_else(|| config.profile.voice_id.clone()),
        ),
        speed: request.speed.clone(),
        volume: request.volume.clone(),
        pitch: request.pitch.clone(),
        ..ProfileDraft::default()
    };
    let mut profile =
        crate::speech::profile::resolve_profile(&config.profile, &draft).map_err(GenerateError::Profile)?;

    // 4. 计算 clip_id（规范化 fingerprint）。
    let clip_id = ClipFingerprint {
        provider: &profile.provider,
        model: &profile.model,
        asset_id: &request.asset_id,
        annotation_id: &request.annotation_id,
        content_kind: request.content_kind,
        normalized_text: &normalized,
        voice_id: &profile.voice_id,
        speed_x100: profile.speed.x100(),
        volume_x100: profile.volume.x100(),
        pitch: profile.pitch,
        audio_format: &profile.audio.format,
        sample_rate: profile.audio.sample_rate,
        bitrate: profile.audio.bitrate,
        channel: profile.audio.channel,
    }
    .clip_id();

    // 5. 获取跨进程 clip 锁（单飞：同 clip 并发只有一个 writer）。
    let _lock = match store
        .try_acquire_clip_lock(&clip_id, CLIP_LOCK_TIMEOUT)
        .map_err(GenerateError::Storage)?
    {
        Some(lock) => lock,
        None => return Err(GenerateError::InProgress { clip_id: clip_id.clone() }),
    };

    // 6. 有效缓存？非 regenerate 命中直接返回，不接触供应商。
    if !request.regenerate {
        if let Some(state) = store.load_clip_state(&clip_id) {
            if let Some(outcome) = cache_hit_outcome(store, &state, request, &text_summary, &profile) {
                return Ok(outcome);
            }
            // unknown / artifact-missing gate：普通 generate 不得重放。
            if state.generation_blocked {
                let status = state.latest_attempt_status.unwrap_or(AttemptStatus::Unknown);
                return Err(GenerateError::Blocked {
                    clip_id,
                    attempt_id: state.latest_attempt_id,
                    status,
                    error_code: state.latest_error_code,
                });
            }
        }
    }

    // 7. Preflight：API Key 与音色可用性都在接触供应商前完成。
    let api_key_present = std::env::var(&config.api_key_env)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .is_some();
    if !api_key_present {
        return Err(GenerateError::Provider(ProviderFailure {
            kind: SenseAudioError::MissingApiKey,
            clip_id,
            attempt_id: None,
            trace_id: None,
        }));
    }
    let availability =
        CachedVoiceCatalogSource::new(store.clone()).current_catalog(&profile.provider);
    match verify_voice(&profile, &availability, now) {
        VoiceVerification::Verified => {
            profile.verification = ProfileVerification::verified(&rfc3339(now));
        }
        VoiceVerification::Unavailable | VoiceVerification::Unverified(_) => {
            return Err(GenerateError::VoiceUnavailable {
                provider: profile.provider.clone(),
                voice_id: profile.voice_id.clone(),
            });
        }
    }

    // 8. 创建 attempt 并同步调用 SenseAudio（唯一一次付费请求）。
    let attempt_id = new_attempt_id();
    let started_at = rfc3339(now);
    let synthesis_request = SynthesisRequest {
        model: profile.model.clone(),
        text: escape_control_markup(&normalized),
        voice_id: profile.voice_id.clone(),
        speed: profile.speed.as_f64(),
        volume: profile.volume.as_f64(),
        pitch: profile.pitch,
        audio_format: profile.audio.format.clone(),
        sample_rate: profile.audio.sample_rate,
        bitrate: profile.audio.bitrate,
        channel: profile.audio.channel,
    };

    match client.synthesize(&synthesis_request).await {
        Ok(response) => {
            let declared = DeclaredAudio {
                sample_rate: response.audio_sample_rate,
                channel: response.audio_channel,
                bitrate: response.audio_bitrate,
                format: response.audio_format.clone(),
            };
            finish_success(
                store,
                request,
                &clip_id,
                &attempt_id,
                &started_at,
                &profile,
                &text_summary,
                response.audio_bytes,
                declared,
                response.trace_id,
                response.usage_characters,
                now,
            )
            .await
        }
        Err(kind) => {
            // 记录 attempt 终态；unknown / artifact-missing 且无现有缓存时设置阻塞 gate。
            record_failure_attempt(
                store,
                request,
                &clip_id,
                &attempt_id,
                &started_at,
                &profile,
                &text_summary,
                kind,
                None,
                now,
            );
            Err(GenerateError::Provider(ProviderFailure {
                kind,
                clip_id,
                attempt_id: Some(attempt_id),
                trace_id: None,
            }))
        }
    }
}

/// 供应商 `extra_info` 声明的音频规格（若有）。
#[derive(Debug, Clone, Default)]
struct DeclaredAudio {
    /// 声明的采样率。
    sample_rate: Option<u32>,
    /// 声明的声道数。
    channel: Option<u32>,
    /// 声明的码率。
    bitrate: Option<u32>,
    /// 声明的格式。
    format: Option<String>,
}

/// 供应商成功后的收尾：校验音频、检查 metadata、提交不可变 version、原子切换 pointer。
#[allow(clippy::too_many_arguments)]
async fn finish_success(
    store: &SpeechStore,
    request: &GenerateRequest,
    clip_id: &str,
    attempt_id: &str,
    started_at: &str,
    profile: &VoiceProfile,
    text_summary: &SpeechTextSummary,
    audio_bytes: Vec<u8>,
    declared: DeclaredAudio,
    trace_id: Option<String>,
    usage_characters: Option<u64>,
    now: DateTime<Utc>,
) -> Result<GenerateOutcome, GenerateError> {
    let validation = match validate_mp3(&audio_bytes, &profile.audio) {
        Ok(validation) => validation,
        Err(_invalid) => {
            return Err(reject_artifact(
                store, request, clip_id, attempt_id, started_at, profile, text_summary, trace_id, now,
            ));
        }
    };

    // 与供应商 metadata 不矛盾：extra_info 声明值必须与本地 MP3 帧解析一致。
    if declared.contradicts(&validation) {
        return Err(reject_artifact(
            store, request, clip_id, attempt_id, started_at, profile, text_summary, trace_id, now,
        ));
    }

    // 提交不可变音频 version（内容寻址、幂等、原子放置）。
    let metadata = store
        .commit_audio_version(
            clip_id,
            attempt_id,
            &text_summary.text_sha256,
            &profile.voice_id,
            &audio_bytes,
            validation.sample_rate,
            validation.bitrate,
            validation.channels,
            validation.duration_ms,
            &rfc3339(now),
        )
        .map_err(GenerateError::Storage)?;

    // 原子切换 current pointer 到新 version。
    let mut state = store.load_clip_state(clip_id).unwrap_or_else(|| {
        new_clip_state(clip_id, request, &text_summary.text_sha256, now)
    });
    state.current_cache_status = CurrentCacheStatus::Ready;
    state.current_audio_sha256 = Some(metadata.audio_sha256.clone());
    state.latest_attempt_id = Some(attempt_id.to_string());
    state.latest_attempt_status = Some(AttemptStatus::Succeeded);
    state.latest_error_code = None;
    state.generation_blocked = false;
    state.updated_at = rfc3339(now);
    store.save_clip_state(&state).map_err(GenerateError::Storage)?;

    store
        .save_attempt(&AttemptRecord {
            schema_version: ATTEMPT_SCHEMA_VERSION,
            attempt_id: attempt_id.to_string(),
            clip_id: clip_id.to_string(),
            provider: profile.provider.clone(),
            model: profile.model.clone(),
            voice_id: profile.voice_id.clone(),
            started_at: started_at.to_string(),
            finished_at: Some(rfc3339(now)),
            status: AttemptStatus::Succeeded,
            unicode_characters: text_summary.unicode_characters,
            estimated_billing_characters: text_summary.estimated_billing_characters,
            provider_usage_characters: usage_characters,
            product_error_code: None,
            provider_code: None,
            trace_id: trace_id.clone(),
        })
        .map_err(GenerateError::Storage)?;

    let audio_path = store.clip_version_audio_path(clip_id, &metadata.audio_sha256);
    Ok(GenerateOutcome {
        clip_id: clip_id.to_string(),
        attempt_id: Some(attempt_id.to_string()),
        source: GenerateSource::Provider,
        provider_called: true,
        asset_id: request.asset_id.clone(),
        annotation_id: request.annotation_id.clone(),
        content_kind: request.content_kind,
        text_summary: text_summary.clone(),
        profile: profile.clone(),
        audio_path,
        audio_sha256: metadata.audio_sha256,
        audio_format: metadata.format,
        audio_size_bytes: metadata.size_bytes,
        duration_ms: metadata.duration_ms,
        sample_rate: metadata.sample_rate,
        bitrate: metadata.bitrate,
        channel: metadata.channel,
        trace_id,
        usage_characters,
        warnings: Vec::new(),
    })
}

/// 记录一次失败的 attempt，并按需设置/保留阻塞 gate。绝不自动重放。
#[allow(clippy::too_many_arguments)]
fn record_failure_attempt(
    store: &SpeechStore,
    request: &GenerateRequest,
    clip_id: &str,
    attempt_id: &str,
    started_at: &str,
    profile: &VoiceProfile,
    text_summary: &SpeechTextSummary,
    kind: SenseAudioError,
    trace_id: Option<String>,
    now: DateTime<Utc>,
) {
    let status = attempt_status_for(&kind);
    let error_code = product_error_code_for(&kind);
    let _ = store.save_attempt(&AttemptRecord {
        schema_version: ATTEMPT_SCHEMA_VERSION,
        attempt_id: attempt_id.to_string(),
        clip_id: clip_id.to_string(),
        provider: profile.provider.clone(),
        model: profile.model.clone(),
        voice_id: profile.voice_id.clone(),
        started_at: started_at.to_string(),
        finished_at: Some(rfc3339(now)),
        status,
        unicode_characters: text_summary.unicode_characters,
        estimated_billing_characters: text_summary.estimated_billing_characters,
        provider_usage_characters: None,
        product_error_code: Some(error_code.to_string()),
        provider_code: None,
        trace_id: trace_id.clone(),
    });

    let has_valid_cache = store
        .load_clip_state(clip_id)
        .filter(|state| state.current_cache_status == CurrentCacheStatus::Ready)
        .and_then(|state| state.current_audio_sha256)
        .and_then(|sha| store.valid_cache_audio_path(clip_id, &sha))
        .is_some();

    let mut state = store
        .load_clip_state(clip_id)
        .unwrap_or_else(|| new_clip_state(clip_id, request, &text_summary.text_sha256, now));
    state.latest_attempt_id = Some(attempt_id.to_string());
    state.latest_attempt_status = Some(status);
    state.latest_error_code = Some(error_code.to_string());
    state.updated_at = rfc3339(now);
    if status.blocks_generation() && !has_valid_cache {
        state.generation_blocked = true;
        state.current_cache_status = CurrentCacheStatus::Absent;
        state.current_audio_sha256 = None;
    } else {
        // 有现有有效缓存（regenerate 失败/unknown）时保留旧音频，仅记录 warning。
        state.generation_blocked = false;
    }
    let _ = store.save_clip_state(&state);
}

/// 供应商失败类型 → attempt 终态。
fn attempt_status_for(kind: &SenseAudioError) -> AttemptStatus {
    match kind {
        SenseAudioError::Transport => AttemptStatus::Unknown,
        SenseAudioError::InvalidAudio => AttemptStatus::ProviderSucceededArtifactMissing,
        _ => AttemptStatus::Failed,
    }
}

/// 供应商失败类型 → 稳定产品错误码。
fn product_error_code_for(kind: &SenseAudioError) -> &'static str {
    match kind {
        SenseAudioError::MissingApiKey | SenseAudioError::AuthenticationFailed => "SPEECH_AUTH_FAILED",
        SenseAudioError::RateLimited => "SPEECH_RATE_LIMITED",
        SenseAudioError::Transport => "SPEECH_RESULT_UNKNOWN",
        SenseAudioError::InvalidAudio => "SPEECH_AUDIO_INVALID",
        SenseAudioError::InvalidResponse | SenseAudioError::ProviderFailed => "SPEECH_PROVIDER_FAILED",
    }
}

/// 供应商 `extra_info` 声明值不与本地解析的音频矛盾。
impl DeclaredAudio {
    /// 声明缺失不算矛盾；声明存在但与本地 MP3 帧解析不一致才算矛盾。
    fn contradicts(&self, validation: &AudioValidation) -> bool {
        if let Some(rate) = self.sample_rate {
            if rate != validation.sample_rate {
                return true;
            }
        }
        if let Some(channel) = self.channel {
            if channel != validation.channels {
                return true;
            }
        }
        if let Some(bitrate) = self.bitrate {
            if bitrate != validation.bitrate {
                return true;
            }
        }
        if let Some(format) = &self.format {
            if format != "mp3" {
                return true;
            }
        }
        false
    }
}

/// 供应商成功但产物无效：记录 `SPEECH_AUDIO_INVALID` attempt 并按需设置阻塞 gate，绝不自动重放。
#[allow(clippy::too_many_arguments)]
fn reject_artifact(
    store: &SpeechStore,
    request: &GenerateRequest,
    clip_id: &str,
    attempt_id: &str,
    started_at: &str,
    profile: &VoiceProfile,
    text_summary: &SpeechTextSummary,
    trace_id: Option<String>,
    now: DateTime<Utc>,
) -> GenerateError {
    record_failure_attempt(
        store,
        request,
        clip_id,
        attempt_id,
        started_at,
        profile,
        text_summary,
        SenseAudioError::InvalidAudio,
        trace_id.clone(),
        now,
    );
    GenerateError::Provider(ProviderFailure {
        kind: SenseAudioError::InvalidAudio,
        clip_id: clip_id.to_string(),
        attempt_id: Some(attempt_id.to_string()),
        trace_id,
    })
}

/// 由 Apple Books 数据库解析一条 Annotation；校验 asset 与 annotation 归属。
fn resolve_annotation(
    db: &DB,
    asset_id: &str,
    annotation_id: &str,
) -> Result<Annotation, GenerateError> {
    let book = db
        .get_book_info(asset_id)
        .map_err(|error| GenerateError::Database {
            message: error.to_string(),
        })?
        .ok_or_else(|| GenerateError::AssetMissing {
            asset_id: asset_id.to_string(),
        })?;
    // 只在该 asset 的标注里找：不属于此书的 annotation 一律视为不存在（归属错误）。
    let annotations = db
        .get_annotations(&book.asset_id)
        .map_err(|error| GenerateError::Database {
            message: error.to_string(),
        })?;
    annotations
        .into_iter()
        .find(|annotation| annotation.id == annotation_id)
        .ok_or_else(|| GenerateError::AnnotationMissing {
            asset_id: asset_id.to_string(),
            annotation_id: annotation_id.to_string(),
        })
}

/// 命中有效缓存时构造 cache 来源结果；否则返回 `None`。
fn cache_hit_outcome(
    store: &SpeechStore,
    state: &ClipState,
    request: &GenerateRequest,
    text_summary: &SpeechTextSummary,
    profile: &VoiceProfile,
) -> Option<GenerateOutcome> {
    if state.current_cache_status != CurrentCacheStatus::Ready {
        return None;
    }
    let audio_sha256 = state.current_audio_sha256.as_ref()?;
    let audio_path = store.valid_cache_audio_path(&state.clip_id, audio_sha256)?;
    let metadata: ClipVersionMetadata =
        store.load_clip_version_metadata(&state.clip_id, audio_sha256)?;
    Some(GenerateOutcome {
        clip_id: state.clip_id.clone(),
        attempt_id: None,
        source: GenerateSource::Cache,
        provider_called: false,
        asset_id: request.asset_id.clone(),
        annotation_id: request.annotation_id.clone(),
        content_kind: request.content_kind,
        text_summary: text_summary.clone(),
        profile: profile.clone(),
        audio_path,
        audio_sha256: metadata.audio_sha256,
        audio_format: metadata.format,
        audio_size_bytes: metadata.size_bytes,
        duration_ms: metadata.duration_ms,
        sample_rate: metadata.sample_rate,
        bitrate: metadata.bitrate,
        channel: metadata.channel,
        trace_id: None,
        usage_characters: None,
        warnings: Vec::new(),
    })
}

/// 构造一个空的 clip state（首次生成前使用）。
fn new_clip_state(
    clip_id: &str,
    request: &GenerateRequest,
    text_sha256: &str,
    now: DateTime<Utc>,
) -> ClipState {
    ClipState {
        schema_version: CLIP_STATE_SCHEMA_VERSION,
        clip_id: clip_id.to_string(),
        asset_id: request.asset_id.clone(),
        annotation_id: request.annotation_id.clone(),
        content_kind: request.content_kind.as_str().to_string(),
        text_sha256: text_sha256.to_string(),
        current_cache_status: CurrentCacheStatus::Absent,
        current_audio_sha256: None,
        latest_attempt_id: None,
        latest_attempt_status: None,
        latest_error_code: None,
        generation_blocked: false,
        updated_at: rfc3339(now),
    }
}

/// RFC 3339（秒精度、`Z`）时间戳。
fn rfc3339(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// 生成一个 opaque、进程内唯一的 attempt ID。
fn new_attempt_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    format!("sa-{nanos:x}-{sequence:x}")
}

/// 生成 receipt 的 Machine JSON 响应。永远不含原文、密钥或音频 hex。
#[derive(Debug, Serialize)]
pub struct SpeechGenerateResponse {
    /// 与现有 Machine JSON 协议一致的 schema 版本。
    pub schema_version: u32,
    /// 结构化收据。
    pub receipt: SpeechGenerateReceipt,
}

/// `speech generate` 的收据。
#[derive(Debug, Serialize)]
pub struct SpeechGenerateReceipt {
    /// 稳定操作名：`generate`。
    pub operation: &'static str,
    /// 完整 clip ID。
    pub clip_id: String,
    /// 本次 attempt ID；缓存命中为 `null`。
    pub attempt_id: Option<String>,
    /// `cache` 或 `provider`。
    pub source: &'static str,
    /// 是否真的调用了供应商。
    pub provider_called: bool,
    /// 书籍稳定 ID。
    pub asset_id: String,
    /// Annotation 稳定 ID。
    pub annotation_id: String,
    /// 内容种类。
    pub content_kind: &'static str,
    /// 规范化文本 SHA-256。
    pub text_sha256: String,
    /// Unicode 字符数。
    pub unicode_characters: u64,
    /// 估算计费字符数。
    pub estimated_billing_characters: u64,
    /// 计费估算器版本。
    pub billing_estimator_version: &'static str,
    /// 解析后的 Voice Profile（不含秘密）。
    pub profile: crate::speech::machine::SpeechProfileDto,
    /// 本地音频元数据。
    pub audio: SpeechGenerateAudioDto,
    /// 供应商用量与 trace。
    pub provider: SpeechGenerateProviderDto,
    /// 结构化 warning。
    pub warnings: Vec<SpeechWarning>,
}

/// 生成 receipt 的本地音频元数据。
#[derive(Debug, Serialize)]
pub struct SpeechGenerateAudioDto {
    /// 本地音频绝对路径。
    pub path: String,
    /// 音频 SHA-256。
    pub sha256: String,
    /// 音频格式。
    pub format: String,
    /// 音频字节大小。
    pub size_bytes: u64,
    /// 估算时长（毫秒）。
    pub duration_ms: u64,
    /// 采样率。
    pub sample_rate: u32,
    /// 码率。
    pub bitrate: u32,
    /// 声道数。
    pub channel: u32,
}

/// 生成 receipt 的供应商用量与 trace。
#[derive(Debug, Serialize)]
pub struct SpeechGenerateProviderDto {
    /// 供应商 trace ID。
    pub trace_id: Option<String>,
    /// 供应商返回的实际用量字符数；只作为记录。
    pub usage_characters: Option<u64>,
}

impl SpeechGenerateResponse {
    /// 由 use case 结果构造稳定 Machine JSON envelope。
    pub fn new(outcome: &GenerateOutcome) -> Self {
        use crate::machine::SCHEMA_VERSION;
        let profile = crate::speech::machine::SpeechProfileDto::from(&outcome.profile);
        Self {
            schema_version: SCHEMA_VERSION,
            receipt: SpeechGenerateReceipt {
                operation: "generate",
                clip_id: outcome.clip_id.clone(),
                attempt_id: outcome.attempt_id.clone(),
                source: outcome.source.as_str(),
                provider_called: outcome.provider_called,
                asset_id: outcome.asset_id.clone(),
                annotation_id: outcome.annotation_id.clone(),
                content_kind: outcome.content_kind.as_str(),
                text_sha256: outcome.text_summary.text_sha256.clone(),
                unicode_characters: outcome.text_summary.unicode_characters,
                estimated_billing_characters: outcome.text_summary.estimated_billing_characters,
                billing_estimator_version: outcome.text_summary.billing_estimator_version,
                profile,
                audio: SpeechGenerateAudioDto {
                    path: outcome.audio_path.to_string_lossy().into_owned(),
                    sha256: outcome.audio_sha256.clone(),
                    format: outcome.audio_format.clone(),
                    size_bytes: outcome.audio_size_bytes,
                    duration_ms: outcome.duration_ms,
                    sample_rate: outcome.sample_rate,
                    bitrate: outcome.bitrate,
                    channel: outcome.channel,
                },
                provider: SpeechGenerateProviderDto {
                    trace_id: outcome.trace_id.clone(),
                    usage_characters: outcome.usage_characters,
                },
                warnings: outcome.warnings.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speech::profile::{parse_hundredths, parse_pitch};

    #[test]
    fn attempt_ids_are_unique() {
        let a = new_attempt_id();
        let b = new_attempt_id();
        assert_ne!(a, b);
        assert!(a.starts_with("sa-"));
    }

    #[test]
    fn attempt_status_and_error_code_map_from_provider_kind() {
        assert_eq!(attempt_status_for(&SenseAudioError::Transport), AttemptStatus::Unknown);
        assert_eq!(
            attempt_status_for(&SenseAudioError::InvalidAudio),
            AttemptStatus::ProviderSucceededArtifactMissing
        );
        assert_eq!(attempt_status_for(&SenseAudioError::ProviderFailed), AttemptStatus::Failed);
        assert_eq!(product_error_code_for(&SenseAudioError::Transport), "SPEECH_RESULT_UNKNOWN");
        assert_eq!(
            product_error_code_for(&SenseAudioError::InvalidAudio),
            "SPEECH_AUDIO_INVALID"
        );
        assert_eq!(
            product_error_code_for(&SenseAudioError::MissingApiKey),
            "SPEECH_AUTH_FAILED"
        );
        assert!(AttemptStatus::Unknown.blocks_generation());
        assert!(AttemptStatus::ProviderSucceededArtifactMissing.blocks_generation());
        assert!(!AttemptStatus::Failed.blocks_generation());
        assert!(!AttemptStatus::Succeeded.blocks_generation());
    }

    #[test]
    fn source_strings_are_stable() {
        assert_eq!(GenerateSource::Cache.as_str(), "cache");
        assert_eq!(GenerateSource::Provider.as_str(), "provider");
    }

    #[test]
    fn declared_audio_only_contradicts_when_present_and_mismatched() {
        let validation = AudioValidation {
            sample_rate: 32_000,
            bitrate: 128_000,
            channels: 2,
            duration_ms: 36,
        };
        // 全部缺失不算矛盾。
        assert!(!DeclaredAudio::default().contradicts(&validation));
        // 声明一致不算矛盾。
        assert!(!DeclaredAudio {
            sample_rate: Some(32_000),
            channel: Some(2),
            bitrate: Some(128_000),
            format: Some("mp3".to_string()),
        }
        .contradicts(&validation));
        // 任一声明不一致都算矛盾。
        assert!(DeclaredAudio { sample_rate: Some(44_100), ..DeclaredAudio::default() }
            .contradicts(&validation));
        assert!(DeclaredAudio { format: Some("wav".to_string()), ..DeclaredAudio::default() }
            .contradicts(&validation));
    }

    #[test]
    fn override_parsers_used_by_the_cli_are_exact() {
        assert_eq!(parse_hundredths("1.5", "speed").expect("speed").x100(), 150);
        assert_eq!(parse_pitch("-3").expect("pitch"), -3);
    }
}
