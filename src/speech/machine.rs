//! Speech 的 Machine JSON 收据、warning 与错误映射。
//!
//! 复用 [`crate::machine`] 的 envelope；profile 相关错误额外带上稳定的 `details`，
//! 让机器消费者不用解析 message 就能知道是哪个字段、哪种原因。

use crate::machine::{MachineError, SCHEMA_VERSION};
use crate::speech::catalog::CatalogSourceType;
use crate::speech::profile::{ProfileError, VoiceProfile};
use crate::speech::{
    ProfileOperation, SpeechConfig, SpeechError, SpeechStoreError, SpeechWarning,
    VoiceCatalogError, VoiceCatalogOutcome,
};
use serde::Serialize;
use serde_json::{json, Value};

/// profile 命令的成功响应。
#[derive(Debug, Serialize)]
pub struct SpeechProfileResponse {
    /// 与现有 Machine JSON 协议一致的 schema 版本。
    pub schema_version: u32,
    /// 结构化收据。
    pub receipt: SpeechProfileReceipt,
}

/// `speech profile show|set|reset` 的收据。
#[derive(Debug, Serialize)]
pub struct SpeechProfileReceipt {
    /// 操作名：`profile_show`、`profile_set`、`profile_reset`。
    pub operation: &'static str,
    /// 解析后的 Voice Profile；不含任何秘密。
    pub profile: SpeechProfileDto,
    /// API Key 的环境变量名；密钥值永远不出现在这里。
    pub api_key_env: String,
    /// 非秘密配置文件的绝对路径。
    pub config_path: String,
    /// 结构化 warning。
    pub warnings: Vec<SpeechWarning>,
}

/// Voice Profile 的机器表示；`speed`/`volume` 以精确百分位数值序列化。
#[derive(Debug, Serialize)]
pub struct SpeechProfileDto {
    /// Speech Provider。
    pub provider: String,
    /// 供应商模型。
    pub model: String,
    /// 具体音色 ID。
    pub voice_id: String,
    /// provider 展示标签。
    pub emotion_label: Option<String>,
    /// provider 展示标签。
    pub style_label: Option<String>,
    /// 语速。
    pub speed: crate::speech::profile::Hundredths,
    /// 音量。
    pub volume: crate::speech::profile::Hundredths,
    /// 声调。
    pub pitch: i32,
    /// `verified` 或 `unverified`。
    pub verification_status: &'static str,
    /// 验证时间；未验证时为 `null`。
    pub verified_at: Option<String>,
    /// 首版固定音频规格。
    pub audio: SpeechAudioDto,
}

/// 首版固定音频规格；`path`/`sha256`/`size_bytes`/`duration_ms` 只在生成收据里出现。
#[derive(Debug, Serialize)]
pub struct SpeechAudioDto {
    /// 本地音频绝对路径。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// 音频字节的 SHA-256。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// 格式。
    pub format: String,
    /// 音频字节数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// 时长（毫秒）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// 采样率。
    pub sample_rate: u32,
    /// 码率。
    pub bitrate: u32,
    /// 声道数。
    pub channel: u32,
}

impl SpeechAudioDto {
    /// Profile 收据里的固定规格视图：只含格式参数，不含本地产物信息。
    pub fn specification(audio: &crate::speech::profile::AudioSettings) -> Self {
        Self {
            path: None,
            sha256: None,
            format: audio.format.clone(),
            size_bytes: None,
            duration_ms: None,
            sample_rate: audio.sample_rate,
            bitrate: audio.bitrate,
            channel: audio.channel,
        }
    }
}

/// `speech voices` 的 Machine JSON 响应。
#[derive(Debug, Serialize)]
pub struct VoiceCatalogResponse {
    /// 与现有 Machine JSON 协议一致的 schema 版本。
    pub schema_version: u32,
    /// 目录收据。
    pub receipt: VoiceCatalogReceipt,
}

/// `speech voices` 的收据。
#[derive(Debug, Serialize)]
pub struct VoiceCatalogReceipt {
    /// 稳定操作名。
    pub operation: &'static str,
    /// 被查询的 Speech Provider。
    pub provider: String,
    /// 当前展示目录的获取时间。
    pub fetched_at: String,
    /// 是否为刷新失败后的 stale 回退。
    pub stale: bool,
    /// 账号可见条目，保留供应商返回的精确元数据。
    pub voices: Vec<VoiceCatalogVoiceDto>,
    /// 诚实 warning，包括 stale 回退和空目录。
    pub warnings: Vec<SpeechWarning>,
}

/// 与供应商无关的单条目录条目机器表示。
#[derive(Debug, Serialize)]
pub struct VoiceCatalogVoiceDto {
    /// Speech Provider 名。
    pub provider: String,
    /// system、cloned 或 generated。
    pub source_type: CatalogSourceType,
    /// 供应商的精确音色 ID。
    pub voice_id: String,
    /// 供应商展示名。
    pub voice_name: String,
    /// 供应商明确返回的情感标签。
    pub emotion_label: Option<String>,
    /// 供应商明确返回的风格标签。
    pub style_label: Option<String>,
    /// 供应商拥有的展示描述。
    pub description: Vec<String>,
    /// 供应商返回的创建时间。
    pub created_time: Option<String>,
}

impl VoiceCatalogResponse {
    /// 把目录 use case 结果转成稳定的 Machine JSON envelope。
    pub fn new(outcome: &VoiceCatalogOutcome) -> Self {
        let catalog = &outcome.catalog;
        Self {
            schema_version: SCHEMA_VERSION,
            receipt: VoiceCatalogReceipt {
                operation: "voices",
                provider: catalog.provider.clone(),
                fetched_at: catalog.fetched_at.to_rfc3339(),
                stale: outcome.stale,
                voices: catalog
                    .voices
                    .iter()
                    .map(|voice| VoiceCatalogVoiceDto {
                        provider: catalog.provider.clone(),
                        source_type: voice.source_type,
                        voice_id: voice.voice_id.clone(),
                        voice_name: voice.voice_name.clone(),
                        emotion_label: voice.emotion_label.clone(),
                        style_label: voice.style_label.clone(),
                        description: voice.description.clone(),
                        created_time: voice.created_time.clone(),
                    })
                    .collect(),
                warnings: outcome.warnings.clone(),
            },
        }
    }
}

impl From<&VoiceProfile> for SpeechProfileDto {
    fn from(profile: &VoiceProfile) -> Self {
        Self {
            provider: profile.provider.clone(),
            model: profile.model.clone(),
            voice_id: profile.voice_id.clone(),
            emotion_label: profile.emotion_label.clone(),
            style_label: profile.style_label.clone(),
            speed: profile.speed,
            volume: profile.volume,
            pitch: profile.pitch,
            verification_status: profile.verification.status.as_str(),
            verified_at: profile.verification.verified_at.clone(),
            audio: SpeechAudioDto::specification(&profile.audio),
        }
    }
}

impl SpeechProfileResponse {
    /// 由 use case 结果构造响应。
    pub fn new(
        operation: ProfileOperation,
        config: &SpeechConfig,
        config_path: &std::path::Path,
        warnings: Vec<SpeechWarning>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            receipt: SpeechProfileReceipt {
                operation: operation.as_str(),
                profile: SpeechProfileDto::from(&config.profile),
                api_key_env: config.api_key_env.clone(),
                config_path: config_path.to_string_lossy().into_owned(),
                warnings,
            },
        }
    }
}

/// `speech generate` 的成功响应。
#[derive(Debug, Serialize)]
pub struct SpeechGenerateResponse {
    /// 与现有 Machine JSON 协议一致的 schema 版本。
    pub schema_version: u32,
    /// 结构化收据。
    pub receipt: SpeechGenerateReceipt,
}

/// `speech generate` 的收据（实施 spec 6.1）。
///
/// 永远不包含 Speech Text、API Key、音频 hex 或供应商完整原始响应。
#[derive(Debug, Serialize)]
pub struct SpeechGenerateReceipt {
    /// 稳定操作名。
    pub operation: &'static str,
    /// 完整 clip ID（64 位 sha256）。
    pub clip_id: String,
    /// 本次 Speech Attempt ID；cache hit 为 `null`。
    pub attempt_id: Option<String>,
    /// `cache` 或 `provider`。
    pub source: &'static str,
    /// 是否真的向供应商发起请求。
    pub provider_called: bool,
    /// 书籍稳定 ID。
    pub asset_id: String,
    /// Annotation 稳定 ID。
    pub annotation_id: String,
    /// 内容部分。
    pub content_kind: &'static str,
    /// 规范化文本的 SHA-256；原文永不出现。
    pub text_sha256: String,
    /// 本地 Unicode 字符数。
    pub unicode_characters: usize,
    /// 计费字符估算；不是最终账单。
    pub estimated_billing_characters: usize,
    /// 计费估算规则版本。
    pub billing_estimator_version: String,
    /// 解析后的 Voice Profile。
    pub profile: SpeechProfileDto,
    /// 本地音频元数据。
    pub audio: SpeechAudioDto,
    /// 供应商 trace 与用量。
    pub provider: SpeechProviderUsageDto,
    /// 结构化 warning。
    pub warnings: Vec<SpeechWarning>,
}

/// 供应商 trace 与用量；只保留诊断字段。
#[derive(Debug, Serialize)]
pub struct SpeechProviderUsageDto {
    /// 供应商 trace ID。
    pub trace_id: Option<String>,
    /// 供应商返回的用量字符数；只作记录。
    pub usage_characters: Option<u64>,
}

impl SpeechGenerateResponse {
    /// 由 use case 结果构造响应。
    pub fn new(receipt: SpeechGenerateReceipt) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            receipt,
        }
    }
}

impl SpeechGenerateReceipt {
    /// 由生成结果构造收据。
    pub fn from_outcome(outcome: &crate::speech::GenerateOutcome) -> Self {
        Self {
            operation: "generate",
            clip_id: outcome.clip_id.clone(),
            attempt_id: outcome.attempt_id.clone(),
            source: outcome.source.as_str(),
            provider_called: outcome.source.provider_called(),
            asset_id: outcome.asset_id.clone(),
            annotation_id: outcome.annotation_id.clone(),
            content_kind: outcome.content_kind.as_str(),
            text_sha256: outcome.text_sha256.clone(),
            unicode_characters: outcome.billing.unicode_characters,
            estimated_billing_characters: outcome.billing.estimated_billing_characters,
            billing_estimator_version: outcome.billing.billing_estimator_version.to_string(),
            profile: SpeechProfileDto::from(&outcome.profile),
            audio: SpeechAudioDto {
                path: Some(outcome.audio_path.to_string_lossy().into_owned()),
                sha256: Some(outcome.audio_sha256.clone()),
                format: outcome.audio_format.clone(),
                size_bytes: Some(outcome.audio_size_bytes),
                duration_ms: Some(outcome.audio_duration_ms),
                sample_rate: outcome.sample_rate,
                bitrate: outcome.bitrate,
                channel: outcome.channel,
            },
            provider: SpeechProviderUsageDto {
                trace_id: outcome.trace_id.clone(),
                usage_characters: outcome.usage_characters,
            },
            warnings: outcome.warnings.clone(),
        }
    }
}

/// `speech cache status` 的成功响应。
#[derive(Debug, Serialize)]
pub struct SpeechCacheStatusResponse {
    /// 与现有 Machine JSON 协议一致的 schema 版本。
    pub schema_version: u32,
    /// 结构化收据。
    pub receipt: SpeechCacheStatusReceipt,
}

/// `speech cache status` 的收据：预算、占用与异常 entry。
///
/// 不含原文、密钥或音频字节；`in_use` 说明每个 entry 正被哪种操作占用。
#[derive(Debug, Serialize)]
pub struct SpeechCacheStatusReceipt {
    /// 稳定操作名。
    pub operation: &'static str,
    /// 配置的总预算（bytes）。
    pub budget_bytes: u64,
    /// 调用 provider 前必须保留的安全余量（bytes）。
    pub safety_margin_bytes: u64,
    /// 预算中可给缓存内容使用的部分（budget - margin）。
    pub usable_budget_bytes: u64,
    /// 当前缓存占用（bytes）。
    pub used_bytes: u64,
    /// 已接受 entry 数。
    pub accepted_entries: usize,
    /// 没有有效音频的 entry 数。
    pub absent_entries: usize,
    /// 被 generation gate 阻塞的 entry 数。
    pub blocked_entries: usize,
    /// 状态不可信或音频校验失败的 entry 数。
    pub corrupt_entries: usize,
    /// 正在生成/播放/导出或持锁的 entry 数。
    pub locked_entries: usize,
    /// 可回收的孤立 version 目录数。
    pub reclaimable_versions: usize,
    /// 每个 clip 的明细。
    pub entries: Vec<crate::speech::CacheStatusEntry>,
    /// 结构化 warning。
    pub warnings: Vec<SpeechWarning>,
}

impl SpeechCacheStatusReceipt {
    /// 从只读报告构造收据；不调用 provider，不删除任何内容。
    pub fn new(report: &crate::speech::CacheStatusReport) -> Self {
        Self {
            operation: "cache_status",
            budget_bytes: report.budget_bytes,
            safety_margin_bytes: report.safety_margin_bytes,
            usable_budget_bytes: report.usable_budget_bytes,
            used_bytes: report.used_bytes,
            accepted_entries: report.accepted_entries,
            absent_entries: report.absent_entries,
            blocked_entries: report.blocked_entries,
            corrupt_entries: report.corrupt_entries,
            locked_entries: report.locked_entries,
            reclaimable_versions: report.reclaimable_versions,
            entries: report.entries.clone(),
            warnings: Vec::new(),
        }
    }
}

/// `speech cache clear` 的成功响应。
#[derive(Debug, Serialize)]
pub struct SpeechCacheClearResponse {
    /// 与现有 Machine JSON 协议一致的 schema 版本。
    pub schema_version: u32,
    /// 结构化收据。
    pub receipt: SpeechCacheClearReceipt,
}

/// `speech cache clear` 的收据：删除、跳过与显式清除的阻塞门。
#[derive(Debug, Serialize)]
pub struct SpeechCacheClearReceipt {
    /// 稳定操作名。
    pub operation: &'static str,
    /// 被删除的 clip ID。
    pub removed: Vec<String>,
    /// 因正在生成/播放/导出或持锁而跳过的 clip ID。
    pub skipped: Vec<String>,
    /// 每个被跳过的 clip 的占用原因；与 `skipped` 同序。
    pub skipped_reasons: Vec<crate::speech::ClipUseSkip>,
    /// 被显式清除的 generation gate 数量。
    pub cleared_generation_gates: usize,
    /// 结构化 warning。
    pub warnings: Vec<SpeechWarning>,
}

impl SpeechCacheClearReceipt {
    /// 从清理报告构造收据；不包含原文、密钥或音频信息。
    pub fn new(report: &crate::speech::CacheClearReport) -> Self {
        Self {
            operation: "cache_clear",
            removed: report.removed.clone(),
            skipped: report.skipped.clone(),
            skipped_reasons: report.skipped_reasons.clone(),
            cleared_generation_gates: report.cleared_generation_gates,
            warnings: Vec::new(),
        }
    }
}

/// `speech history clear` 的成功响应。
#[derive(Debug, Serialize)]
pub struct SpeechHistoryClearResponse {
    /// 与现有 Machine JSON 协议一致的 schema 版本。
    pub schema_version: u32,
    /// 结构化收据。
    pub receipt: SpeechHistoryClearReceipt,
}

/// `speech history clear` 的收据。
#[derive(Debug, Serialize)]
pub struct SpeechHistoryClearReceipt {
    /// 稳定操作名。
    pub operation: &'static str,
    /// 被删除的 attempt history 记录数。
    pub removed_attempts: usize,
    /// 恒为 0：history clear 不清 unknown gate。
    pub cleared_generation_gates: usize,
    /// 结构化 warning。
    pub warnings: Vec<SpeechWarning>,
}

impl SpeechHistoryClearReceipt {
    /// 从清理报告构造收据；只含计数，不含任何 attempt 内容。
    pub fn new(report: &crate::speech::HistoryClearReport) -> Self {
        Self {
            operation: "history_clear",
            removed_attempts: report.removed_attempts,
            cleared_generation_gates: report.cleared_generation_gates,
            warnings: Vec::new(),
        }
    }
}

/// 把生成失败映射成稳定的 Machine JSON envelope。
///
/// 供应商 code、trace ID、attempt ID 与 outcome 进入可选 `details`；原始响应体
/// 与 Speech Text 永远不进入稳定协议。
pub fn generate_error_response(error: &crate::speech::GenerationError) -> MachineError {
    // Profile 错误沿用字段级 details，机器消费者不必解析 message。
    if let crate::speech::GenerationError::Profile(profile_error) = error {
        return profile_invalid(profile_error);
    }
    let mut details = serde_json::Map::new();
    details.insert("provider".to_string(), json!("senseaudio"));
    details.insert("reason".to_string(), json!(error.reason_code()));
    if let Some(trace_id) = error.trace_id() {
        details.insert("trace_id".to_string(), json!(trace_id));
    }
    if let Some(attempt_id) = error.attempt_id() {
        details.insert("attempt_id".to_string(), json!(attempt_id));
    }
    details.insert("outcome".to_string(), json!(error.outcome()));
    if let crate::speech::GenerationError::VoiceUnavailable { voice_id, reason } = error {
        details.insert("field".to_string(), json!("voice_id"));
        details.insert("value".to_string(), json!(voice_id));
        details.insert("voice_reason".to_string(), json!(reason.as_str()));
    }
    machine_error(
        error.machine_code(),
        error.message(),
        error.remediation(),
        Value::Object(details),
    )
}

/// `speech play --json` 的成功响应。
#[derive(Debug, Serialize)]
pub struct SpeechPlayResponse {
    /// 与现有 Machine JSON 协议一致的 schema 版本。
    pub schema_version: u32,
    /// 结构化收据。
    pub receipt: SpeechPlayReceipt,
}

/// `speech play` 的收据：已校验的路径与来源。
///
/// `--json` 不启动播放器，因此 `played` 恒为 `false`；收据不包含原文、密钥或音频字节。
#[derive(Debug, Serialize)]
pub struct SpeechPlayReceipt {
    /// 稳定操作名。
    pub operation: &'static str,
    /// 完整 clip ID。
    pub clip_id: String,
    /// 音频来源：`cache`（Speech Cache Entry）或 `export`（已验证的 Active Exported
    /// Speech Clip）。
    pub source: &'static str,
    /// 已校验音频的绝对路径。
    pub path: String,
    /// 是否真的启动了播放器；machine 模式恒为 `false`。
    pub played: bool,
    /// 恒为 `false`：任何 play 路径都不联系 Speech Provider。
    pub provider_called: bool,
    /// 稳定内容身份。
    pub asset_id: String,
    /// 稳定内容身份。
    pub annotation_id: String,
    /// 内容部分。
    pub content_kind: &'static str,
    /// 本地音频事实。
    pub audio: SpeechAudioDto,
    /// 结构化 warning：候选导出根被拒绝的原因（没有回退时解释「为什么没找到」）。
    pub warnings: Vec<SpeechWarning>,
    /// 回退到导出音频时，候选根来自显式 `--export-root` 还是 locator 投影。
    pub export_origin: Option<&'static str>,
}

impl SpeechPlayResponse {
    /// 把播放 use case 结果转成稳定的 Machine JSON envelope。
    pub fn new(outcome: &crate::speech::PlayOutcome) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            receipt: SpeechPlayReceipt {
                operation: "play",
                clip_id: outcome.clip_id.clone(),
                source: outcome.source.as_str(),
                path: outcome.audio_path.to_string_lossy().into_owned(),
                played: outcome.played,
                provider_called: false,
                asset_id: outcome.asset_id.clone(),
                annotation_id: outcome.annotation_id.clone(),
                content_kind: outcome.content_kind.as_str(),
                audio: SpeechAudioDto {
                    path: Some(outcome.audio_path.to_string_lossy().into_owned()),
                    sha256: Some(outcome.audio_sha256.clone()),
                    format: "mp3".to_string(),
                    size_bytes: Some(outcome.audio_size_bytes),
                    duration_ms: Some(outcome.audio_duration_ms),
                    sample_rate: outcome.sample_rate,
                    bitrate: 128_000,
                    channel: 2,
                },
                warnings: outcome.warnings.clone(),
                export_origin: outcome.export_origin.map(|origin| origin.as_str()),
            },
        }
    }
}

/// 把播放失败映射成稳定的 Machine JSON envelope。
///
/// 播放路径上没有任何 provider 能力，因此 `details` 只记录本地事实：来源、是否
/// 尝试过播放器、失败原因。
pub fn play_error_response(error: &crate::speech::PlayError) -> MachineError {
    let mut details = serde_json::Map::new();
    details.insert("reason".to_string(), json!(play_reason(error)));
    if let crate::speech::PlayError::ClipNotFound { clip_id, warnings } = error {
        details.insert("clip_id".to_string(), json!(clip_id));
        // 候选导出根被拒绝的原因：证明「找过、且没猜」，而不是笼统的 not found。
        details.insert(
            "rejected_candidates".to_string(),
            json!(warnings
                .iter()
                .map(|warning| json!({
                    "code": warning.code,
                    "reason": warning.reason,
                    "message": warning.message,
                }))
                .collect::<Vec<_>>()),
        );
    }
    if let crate::speech::PlayError::InvalidClipId(clip_id) = error {
        details.insert("field".to_string(), json!("clip_id"));
        details.insert("value".to_string(), json!(clip_id));
    }
    if let crate::speech::PlayError::CacheCorrupt { path, .. } = error {
        details.insert("path".to_string(), json!(path.to_string_lossy()));
    }
    machine_error(
        error.machine_code(),
        error.message(),
        error.remediation(),
        Value::Object(details),
    )
}

/// 播放失败的稳定原因串。
fn play_reason(error: &crate::speech::PlayError) -> &'static str {
    match error {
        crate::speech::PlayError::Storage(..) => "storage_unavailable",
        crate::speech::PlayError::InvalidClipId(..) => "invalid_clip_id",
        crate::speech::PlayError::ClipNotFound { .. } => "clip_not_found",
        crate::speech::PlayError::CacheCorrupt { .. } => "cache_corrupt",
        crate::speech::PlayError::ClipInUse => "clip_in_use",
        crate::speech::PlayError::PlaybackFailed { .. } => "playback_failed",
    }
}

/// `speech export --json` 的成功响应。
#[derive(Debug, Serialize)]
pub struct SpeechExportResponse {
    /// 与现有 Machine JSON 协议一致的 schema 版本。
    pub schema_version: u32,
    /// 结构化收据。
    pub receipt: SpeechExportReceipt,
}

/// `speech export` 的收据：已校验的导出事实。
///
/// 只含稳定身份、相对路径、checksum、大小、格式和导出时间；**不包含** Speech Text、
/// API Key 或供应商原始响应。
#[derive(Debug, Serialize)]
pub struct SpeechExportReceipt {
    /// 稳定操作名。
    pub operation: &'static str,
    /// 完整 clip ID。
    pub clip_id: String,
    /// 书籍稳定 ID。
    pub asset_id: String,
    /// Annotation 稳定 ID。
    pub annotation_id: String,
    /// 内容部分。
    pub content_kind: &'static str,
    /// 相对书籍导出根目录的路径。
    pub relative_path: String,
    /// 已校验音频的绝对路径。
    pub path: String,
    /// 音频字节的 SHA-256。
    pub sha256: String,
    /// 音频字节数。
    pub size_bytes: u64,
    /// 音频格式。
    pub format: &'static str,
    /// 导出时间。
    pub exported_at: String,
    /// 该内容部分当前唯一 active 的 clip ID。
    pub active_clip_id: String,
    /// 目标文件字节一致、直接复用而没有重写。
    pub reused: bool,
    /// 本次显式替换了内容不同的已有文件。
    pub replaced: bool,
    /// 恒为 `false`：任何 export 路径都不联系 Speech Provider。
    pub provider_called: bool,
    /// 非致命 warning。
    pub warnings: Vec<SpeechWarning>,
}

impl SpeechExportResponse {
    /// 把导出 use case 结果转成稳定的 Machine JSON envelope。
    pub fn new(outcome: &crate::speech::ExportOutcome) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            receipt: SpeechExportReceipt {
                operation: "export",
                clip_id: outcome.clip_id.clone(),
                asset_id: outcome.asset_id.clone(),
                annotation_id: outcome.annotation_id.clone(),
                content_kind: outcome.content_kind.as_str(),
                relative_path: outcome.relative_path.clone(),
                path: outcome.audio_path.to_string_lossy().into_owned(),
                sha256: outcome.audio_sha256.clone(),
                size_bytes: outcome.audio_size_bytes,
                format: "mp3",
                exported_at: outcome.exported_at.clone(),
                active_clip_id: outcome.active_clip_id.clone(),
                reused: outcome.reused,
                replaced: outcome.replaced,
                provider_called: false,
                warnings: outcome.warnings.clone(),
            },
        }
    }
}

/// 把导出失败映射成稳定的 Machine JSON envelope。
///
/// 导出路径上没有任何 provider 能力，因此 `details` 只记录本地事实：冲突路径、
/// manifest 判定原因或被占用的 clip。
pub fn export_error_response(error: &crate::speech::ExportError) -> MachineError {
    let mut details = serde_json::Map::new();
    details.insert("reason".to_string(), json!(export_reason(error)));
    if let crate::speech::ExportError::OutputFileExists { relative_path, .. } = error {
        details.insert("relative_path".to_string(), json!(relative_path));
    }
    if let crate::speech::ExportError::ManifestInvalid { detail, .. } = error {
        details.insert("manifest_detail".to_string(), json!(detail));
    }
    if let crate::speech::ExportError::CacheCorrupt { path, .. } = error {
        details.insert("path".to_string(), json!(path.to_string_lossy()));
    }
    if let crate::speech::ExportError::ClipNotFound { clip_id } = error {
        details.insert("clip_id".to_string(), json!(clip_id));
    }
    if let crate::speech::ExportError::InvalidClipId(clip_id) = error {
        details.insert("field".to_string(), json!("clip_id"));
        details.insert("value".to_string(), json!(clip_id));
    }
    machine_error(
        error.machine_code(),
        error.message(),
        error.remediation(),
        Value::Object(details),
    )
}

/// 导出失败的稳定原因串。
fn export_reason(error: &crate::speech::ExportError) -> &'static str {
    match error {
        crate::speech::ExportError::Storage(..) => "storage_unavailable",
        crate::speech::ExportError::InvalidClipId(..) => "invalid_clip_id",
        crate::speech::ExportError::ClipNotFound { .. } => "clip_not_found",
        crate::speech::ExportError::CacheCorrupt { .. } => "cache_corrupt",
        crate::speech::ExportError::ManifestInvalid { .. } => "manifest_invalid",
        crate::speech::ExportError::OutputFileExists { .. } => "output_file_exists",
        crate::speech::ExportError::ClipInUse => "clip_in_use",
    }
}

/// profile 校验失败：`SPEECH_PROFILE_INVALID` + 字段级 `details`。
pub fn profile_invalid(error: &ProfileError) -> MachineError {
    machine_error(
        "SPEECH_PROFILE_INVALID",
        error.message(),
        "Run `apple-books-exporter speech profile show --json` to inspect the stored Voice Profile, then set valid values or reset it.",
        profile_details(error),
    )
}

/// Speech 状态根不可用。
pub fn storage_unavailable(path: &std::path::Path, message: &str) -> MachineError {
    machine_error(
        "SPEECH_STORAGE_UNAVAILABLE",
        format!(
            "The Speech state directory is not usable at {}: {message}",
            path.display()
        ),
        "Verify that this directory exists and is writable, then retry.",
        json!({ "reason": "storage_unavailable", "path": path.to_string_lossy() }),
    )
}

/// catalog 场景的存储失败：错误码仍是 `SPEECH_STORAGE_UNAVAILABLE`，但 remediation
/// 按路径的文件系统类型区分。不嗅探 OS 错误字符串。
fn catalog_storage_unavailable(path: &std::path::Path, message: &str) -> MachineError {
    // 只看路径的文件系统类型：无扩展名却是文件，说明本该是目录的路径被文件占了。
    // 合法的 `.json` 缓存文件写失败不得走这条文案。不嗅探 OS 错误字符串。
    let remediation = if path.is_file() && path.extension().is_none() {
        "This Voice Catalog cache path is a file, not a directory. Remove or replace the file, then retry."
    } else {
        "Verify that this Voice Catalog cache path exists and is writable, then retry."
    };
    machine_error(
        "SPEECH_STORAGE_UNAVAILABLE",
        format!(
            "The Speech state directory is not usable at {}: {message}",
            path.display()
        ),
        remediation,
        json!({ "reason": "storage_unavailable", "path": path.to_string_lossy() }),
    )
}

/// 新鲜 Voice Catalog 明确没有这个音色；此时不得替用户静默换音色，也不得落盘。
pub fn voice_unavailable(provider: &str, voice_id: &str) -> MachineError {
    machine_error(
        "SPEECH_VOICE_UNAVAILABLE",
        format!("Voice '{voice_id}' is not available for provider '{provider}'."),
        "Refresh the Voice Catalog with `apple-books-exporter speech voices --refresh` and choose an available voice.",
        json!({
            "field": "voice_id",
            "reason": "voice_unavailable",
            "value": voice_id,
        }),
    )
}

/// 映射 Voice Catalog 缓存/供应商失败；不暴露凭证或原始响应。
///
/// catalog 路径可能是文件而不是目录。这里按路径的文件系统类型区分 remediation，
/// 不嗅探 OS 错误文案，也不改共用的 `storage_unavailable`。
pub fn voice_catalog_error(error: &VoiceCatalogError) -> MachineError {
    match error {
        VoiceCatalogError::Storage(SpeechStoreError::Unavailable { path, message }) => {
            catalog_storage_unavailable(path, message)
        }
        VoiceCatalogError::Storage(SpeechStoreError::InvalidConfig(error)) => {
            profile_invalid(error)
        }
        VoiceCatalogError::Storage(SpeechStoreError::UnsupportedSchemaVersion(version)) => {
            unsupported_schema_version(*version)
        }
        VoiceCatalogError::Provider(error) => machine_error(
            error.machine_code(),
            error.to_string(),
            "Verify the SenseAudio API key and endpoint, then retry the Voice Catalog refresh.",
            json!({
                "provider": "senseaudio",
                "reason": error.reason_code(),
            }),
        ),
    }
}

/// 存储 schema 版本不受支持。
pub fn unsupported_schema_version(version: u32) -> MachineError {
    MachineError::unsupported_schema_version(version)
}

/// 把 use case 错误映射成稳定的 Machine JSON envelope。
pub fn error_response(error: &SpeechError) -> MachineError {
    match error {
        SpeechError::Storage(error) => match error {
            crate::speech::SpeechStoreError::Unavailable { path, message } => {
                storage_unavailable(path, message)
            }
            crate::speech::SpeechStoreError::InvalidConfig(error) => profile_invalid(error),
            crate::speech::SpeechStoreError::UnsupportedSchemaVersion(version) => {
                unsupported_schema_version(*version)
            }
        },
        SpeechError::Profile(error) => profile_invalid(error),
        SpeechError::VoiceCatalog(error) => voice_catalog_error(error),
        SpeechError::VoiceUnavailable { provider, voice_id } => {
            voice_unavailable(provider, voice_id)
        }
    }
}

fn profile_details(error: &ProfileError) -> Value {
    let mut details = serde_json::Map::new();
    if let Some(field) = error.field {
        details.insert("field".to_string(), Value::String(field.to_string()));
    }
    details.insert(
        "reason".to_string(),
        Value::String(error.reason.as_str().to_string()),
    );
    if let Some(value) = error.value.as_deref() {
        details.insert("value".to_string(), Value::String(value.to_string()));
    }
    Value::Object(details)
}

/// 磁盘上本来就未被验证过的 Profile 使用的 warning 原因字符串。
pub const STORED_UNVERIFIED_REASON: &str = "stored_unverified";

/// 首版 Speech 领域的**稳定** Machine JSON 错误码全集。
///
/// 实施 spec 6.2 的错误码表是权威来源；`SPEECH_PLAYBACK_FAILED` 由 #26 引入并在此补录
/// （它只来自 human `speech play` 的播放器 seam，不是 provider 语义）。这张表是
/// **白名单**：任何 `machine_code()` 返回不在表内的值都是契约缺陷，而不是「新增能力」。
///
/// `UNSUPPORTED_SCHEMA_VERSION` 复用既有 Machine JSON 协议的共享码（见
/// [`unsupported_schema_version`]），因此不在本表内；Speech 不得为它再发明一个别名。
///
/// 机器消费者可以依赖这张表：跨运行可变的错误码是缺陷，而不是扩展点。
pub const STABLE_SPEECH_ERROR_CODES: &[&str] = &[
    // 共享的既有协议码。
    "INVALID_ASSET_ID",
    "INVALID_ARGUMENT",
    "UNSUPPORTED_SCHEMA_VERSION",
    // 内容与身份。
    "INVALID_ANNOTATION_ID",
    "SPEECH_CONTENT_UNAVAILABLE",
    "SPEECH_TEXT_TOO_LONG",
    // Voice Profile 与目录。
    "SPEECH_PROFILE_INVALID",
    "SPEECH_VOICE_UNAVAILABLE",
    // provider 语义。
    "SPEECH_AUTH_FAILED",
    "SPEECH_RATE_LIMITED",
    "SPEECH_PROVIDER_FAILED",
    "SPEECH_RESULT_UNKNOWN",
    "SPEECH_AUDIO_INVALID",
    "SPEECH_ARTIFACT_COMMIT_FAILED",
    // 并发与本地状态。
    "SPEECH_IN_PROGRESS",
    "SPEECH_CACHE_CORRUPT",
    "SPEECH_STORAGE_UNAVAILABLE",
    // 导出与播放。
    "SPEECH_CLIP_NOT_FOUND",
    "SPEECH_OUTPUT_FILE_EXISTS",
    "SPEECH_EXPORT_MANIFEST_INVALID",
    "SPEECH_PLAYBACK_FAILED",
];

/// 判断一个错误码是否属于首版稳定 Speech 错误码集合。
pub fn is_stable_speech_error_code(code: &str) -> bool {
    STABLE_SPEECH_ERROR_CODES.contains(&code)
}

fn machine_error(
    code: &'static str,
    message: String,
    remediation: &str,
    details: Value,
) -> MachineError {
    MachineError {
        code,
        message,
        remediation: Some(remediation.to_string()),
        details: Some(details),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speech::profile::parse_hundredths;

    #[test]
    fn profile_receipt_keeps_hundredths_and_hides_nothing_secret() {
        let config = SpeechConfig::default();
        let response = SpeechProfileResponse::new(
            ProfileOperation::Show,
            &config,
            std::path::Path::new("/tmp/speech/config.json"),
            vec![SpeechWarning::unverified(None)],
        );

        let json = serde_json::to_value(&response).expect("serialize receipt");
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["receipt"]["operation"], "profile_show");
        assert_eq!(json["receipt"]["profile"]["speed"], 1.0);
        assert_eq!(json["receipt"]["profile"]["volume"], 1.0);
        assert_eq!(
            json["receipt"]["profile"]["verification_status"],
            "unverified"
        );
        assert_eq!(json["receipt"]["api_key_env"], "SENSEAUDIO_API_KEY");
        assert_eq!(json["receipt"]["config_path"], "/tmp/speech/config.json");

        let text = serde_json::to_string(&response).expect("serialize receipt text");
        assert!(!text.contains("api_key\":"));
        assert!(!text.contains("\"secret\""));
    }

    #[test]
    fn fractional_hundredths_serialize_as_exact_json_numbers() {
        let mut config = SpeechConfig::default();
        config.profile.speed = parse_hundredths("0.29", "speed").expect("0.29");
        config.profile.volume = parse_hundredths("10.0", "volume").expect("10.0");

        let json = serde_json::to_value(SpeechProfileResponse::new(
            ProfileOperation::Set,
            &config,
            std::path::Path::new("/tmp/config.json"),
            Vec::new(),
        ))
        .expect("serialize receipt");

        assert_eq!(json["receipt"]["profile"]["speed"], 0.29);
        assert_eq!(json["receipt"]["profile"]["volume"], 10.0);
    }

    #[test]
    fn profile_errors_keep_stable_codes_and_field_details() {
        let error = parse_hundredths("1.005", "speed").expect_err("rounded");
        let machine = profile_invalid(&error);

        assert_eq!(machine.code, "SPEECH_PROFILE_INVALID");
        let json = serde_json::to_value(&machine).expect("serialize error");
        assert_eq!(json["code"], "SPEECH_PROFILE_INVALID");
        assert_eq!(json["details"]["field"], "speed");
        assert_eq!(json["details"]["reason"], "not_hundredth");
        assert_eq!(json["details"]["value"], "1.005");
        assert!(json["remediation"].is_string());
        assert!(!json["message"].as_str().unwrap().is_empty());
    }

    #[test]
    fn structural_config_errors_carry_a_reason_without_a_field() {
        let machine = profile_invalid(&ProfileError::stored_config_invalid());
        let json = serde_json::to_value(&machine).expect("serialize error");

        assert_eq!(json["code"], "SPEECH_PROFILE_INVALID");
        assert_eq!(json["details"]["reason"], "stored_config_invalid");
        assert!(json["details"].get("field").is_none());
        assert!(json["details"].get("value").is_none());
    }

    #[test]
    fn the_stored_unverified_reason_string_is_stable() {
        assert_eq!(STORED_UNVERIFIED_REASON, "stored_unverified");
    }

    #[test]
    fn voice_unavailable_is_stable_and_points_at_the_voice_field() {
        let error = error_response(&SpeechError::VoiceUnavailable {
            provider: "senseaudio".to_string(),
            voice_id: "male_0004_a".to_string(),
        });
        let json = serde_json::to_value(&error).expect("serialize error");

        assert_eq!(json["code"], "SPEECH_VOICE_UNAVAILABLE");
        assert_eq!(json["details"]["field"], "voice_id");
        assert_eq!(json["details"]["reason"], "voice_unavailable");
        assert_eq!(json["details"]["value"], "male_0004_a");
    }

    #[test]
    fn storage_errors_keep_their_stable_code_and_path() {
        let error = error_response(&SpeechError::Storage(
            crate::speech::SpeechStoreError::Unavailable {
                path: std::path::PathBuf::from("/tmp/speech"),
                message: "permission denied".to_string(),
            },
        ));
        let json = serde_json::to_value(&error).expect("serialize error");

        assert_eq!(json["code"], "SPEECH_STORAGE_UNAVAILABLE");
        assert_eq!(json["details"]["path"], "/tmp/speech");
        assert_eq!(json["details"]["reason"], "storage_unavailable");
        assert!(!json["message"].as_str().unwrap().contains("api_key"));
    }

    #[test]
    fn unsupported_config_schema_version_reuses_the_shared_code() {
        let error = error_response(&SpeechError::Storage(
            crate::speech::SpeechStoreError::UnsupportedSchemaVersion(2),
        ));

        assert_eq!(error.code, "UNSUPPORTED_SCHEMA_VERSION");
    }

    #[test]
    fn catalog_storage_unavailable_distinguishes_a_file_path_without_sniffing_os_text() {
        let home = tempfile::tempdir().expect("home");
        let file_path = home.path().join("voices");
        std::fs::write(&file_path, "not a directory").expect("occupy catalog dir");
        let error = voice_catalog_error(&VoiceCatalogError::Storage(
            crate::speech::SpeechStoreError::Unavailable {
                path: file_path.clone(),
                message: "not a directory".to_string(),
            },
        ));
        let json = serde_json::to_value(&error).expect("serialize error");

        assert_eq!(json["code"], "SPEECH_STORAGE_UNAVAILABLE");
        assert_eq!(json["details"]["reason"], "storage_unavailable");
        assert_eq!(
            json["details"]["path"],
            file_path.to_string_lossy().as_ref()
        );
        assert!(json["remediation"]
            .as_str()
            .expect("remediation")
            .contains("file, not a directory"));
        assert!(!json["remediation"]
            .as_str()
            .expect("remediation")
            .contains("Verify that this directory exists"));
    }

    #[test]
    fn catalog_storage_unavailable_does_not_treat_a_json_cache_file_as_a_directory_collision() {
        let home = tempfile::tempdir().expect("home");
        let file_path = home.path().join("senseaudio.json");
        std::fs::write(&file_path, "{}").expect("cache file");
        let error = voice_catalog_error(&VoiceCatalogError::Storage(
            crate::speech::SpeechStoreError::Unavailable {
                path: file_path,
                message: "permission denied".to_string(),
            },
        ));

        assert!(!error
            .remediation
            .as_deref()
            .expect("remediation")
            .contains("file, not a directory"));
        assert_eq!(
            error.remediation.as_deref(),
            Some("Verify that this Voice Catalog cache path exists and is writable, then retry.")
        );
    }

    #[test]
    fn shared_storage_unavailable_keeps_the_directory_remediation() {
        let error = storage_unavailable(std::path::Path::new("/tmp/speech"), "permission denied");
        assert_eq!(
            error.remediation.as_deref(),
            Some("Verify that this directory exists and is writable, then retry.")
        );
    }

    /// 契约锁：每个 Speech 错误类型的 `machine_code()` 必须落在稳定码白名单里。
    ///
    /// 跨运行可变的错误码是缺陷（机器消费者无法据此分支），因此这里枚举**每一条**可能
    /// 的错误路径，而不是只抽查几个。新增错误变体时这张表会立刻失败，强制同步 spec 6.2。
    #[test]
    fn every_speech_error_code_is_in_the_documented_stable_set() {
        use crate::speech::{
            export::ExportError, generate::GenerationError, play::PlayError, text::SpeechTextError,
        };

        let mut codes: Vec<&'static str> = Vec::new();

        // SpeechTextError
        codes.push(SpeechTextError::TooLong { characters: 1 }.machine_code());
        // SpeechContentError
        codes.push(
            crate::speech::SpeechContentError::InvalidAnnotationId {
                asset_id: "a".to_string(),
                annotation_id: "b".to_string(),
            }
            .machine_code(),
        );
        codes.push(
            crate::speech::SpeechContentError::ContentUnavailable {
                content_kind: crate::speech::SpeechContentKind::Highlight,
            }
            .machine_code(),
        );
        // PlayError
        codes
            .push(PlayError::Storage(SpeechStoreError::UnsupportedSchemaVersion(2)).machine_code());
        codes.push(PlayError::InvalidClipId("x".to_string()).machine_code());
        codes.push(
            PlayError::ClipNotFound {
                clip_id: "x".to_string(),
                warnings: Vec::new(),
            }
            .machine_code(),
        );
        codes.push(
            PlayError::CacheCorrupt {
                path: std::path::PathBuf::from("/tmp"),
                reason: "r".to_string(),
            }
            .machine_code(),
        );
        codes.push(PlayError::ClipInUse.machine_code());
        codes.push(
            PlayError::PlaybackFailed {
                reason: "r".to_string(),
            }
            .machine_code(),
        );
        // ExportError
        codes.push(
            ExportError::Storage(SpeechStoreError::UnsupportedSchemaVersion(2)).machine_code(),
        );
        codes.push(ExportError::InvalidClipId("x".to_string()).machine_code());
        codes.push(
            ExportError::ClipNotFound {
                clip_id: "x".to_string(),
            }
            .machine_code(),
        );
        codes.push(
            ExportError::CacheCorrupt {
                path: std::path::PathBuf::from("/tmp"),
                reason: "r".to_string(),
            }
            .machine_code(),
        );
        codes.push(
            ExportError::ManifestInvalid {
                reason: "d",
                detail: "d".to_string(),
            }
            .machine_code(),
        );
        codes.push(
            ExportError::OutputFileExists {
                path: std::path::PathBuf::from("/tmp"),
                relative_path: "a.mp3".to_string(),
            }
            .machine_code(),
        );
        codes.push(ExportError::ClipInUse.machine_code());
        // GenerationError：每个变体各取一个代表值。
        codes.push(
            GenerationError::Storage(SpeechStoreError::UnsupportedSchemaVersion(2)).machine_code(),
        );
        codes.push(GenerationError::MissingApiKey.machine_code());
        codes.push(
            GenerationError::InProgress {
                clip_id: "c".to_string(),
                attempt_id: None,
            }
            .machine_code(),
        );
        for code in [
            "SPEECH_AUTH_FAILED",
            "SPEECH_RATE_LIMITED",
            "SPEECH_PROVIDER_FAILED",
        ] {
            codes.push(
                GenerationError::ProviderFailed {
                    code,
                    trace_id: None,
                    provider_code: None,
                    attempt_id: "a".to_string(),
                }
                .machine_code(),
            );
        }
        // ProviderFailed 的 code 是从磁盘记录还原的：白名单之外的值必须被归一化，
        // 否则一个被篡改/过期的 attempt 记录就能让同一个 clip 每次返回不同错误码。
        assert_eq!(
            GenerationError::ProviderFailed {
                code: crate::speech::generate::normalize_recorded_failure_code(
                    "SPEECH_TEXT_TOO_LONG"
                ),
                trace_id: None,
                provider_code: None,
                attempt_id: "a".to_string(),
            }
            .machine_code(),
            "SPEECH_PROVIDER_FAILED",
            "a recorded code outside the failure set must normalize, not leak"
        );

        for code in codes {
            assert!(
                is_stable_speech_error_code(code),
                "`{code}` is emitted by the Speech feature but is not in STABLE_SPEECH_ERROR_CODES; \
                 either it belongs in implementation spec 6.2 or it is a contract defect"
            );
        }
    }

    /// `SPEECH_PLAYBACK_FAILED` 是 #26 引入的稳定码，但实施 spec 6.2 的表里还没有它。
    /// 这条断言锁定「它必须留在白名单里」，文档同步在 spec 6.2。
    #[test]
    fn the_playback_failure_code_is_a_documented_stable_code() {
        assert!(is_stable_speech_error_code("SPEECH_PLAYBACK_FAILED"));
        assert_eq!(
            crate::speech::PlayError::PlaybackFailed {
                reason: "afplay".to_string()
            }
            .machine_code(),
            "SPEECH_PLAYBACK_FAILED"
        );
    }

    /// 白名单本身必须与实施 spec 6.2 保持一致：表里没有的文档化代码会让这张表腐化。
    #[test]
    fn the_stable_set_has_no_duplicates() {
        let mut sorted = STABLE_SPEECH_ERROR_CODES.to_vec();
        sorted.sort_unstable();
        let count = sorted.len();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            count,
            "STABLE_SPEECH_ERROR_CODES has duplicates"
        );
    }
}
