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
        SpeechError::VoiceUnavailable {
            provider,
            voice_id,
        } => voice_unavailable(provider, voice_id),
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
        assert_eq!(json["receipt"]["profile"]["verification_status"], "unverified");
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
        assert_eq!(json["details"]["path"], file_path.to_string_lossy().as_ref());
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
        let error = storage_unavailable(
            std::path::Path::new("/tmp/speech"),
            "permission denied",
        );
        assert_eq!(
            error.remediation.as_deref(),
            Some("Verify that this directory exists and is writable, then retry.")
        );
    }
}
