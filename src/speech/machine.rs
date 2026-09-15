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

/// 首版固定音频规格。
#[derive(Debug, Serialize)]
pub struct SpeechAudioDto {
    /// 格式。
    pub format: String,
    /// 采样率。
    pub sample_rate: u32,
    /// 码率。
    pub bitrate: u32,
    /// 声道数。
    pub channel: u32,
}

/// Machine JSON response for speech voices.
#[derive(Debug, Serialize)]
pub struct VoiceCatalogResponse {
    /// Shared Machine JSON schema version.
    pub schema_version: u32,
    /// Catalog receipt.
    pub receipt: VoiceCatalogReceipt,
}

/// Speech voices receipt.
#[derive(Debug, Serialize)]
pub struct VoiceCatalogReceipt {
    /// Stable operation name.
    pub operation: &'static str,
    /// Provider whose account catalog was queried.
    pub provider: String,
    /// Time the displayed catalog was fetched.
    pub fetched_at: String,
    /// Whether the displayed catalog is a stale fallback.
    pub stale: bool,
    /// Account-visible entries, preserving exact provider metadata.
    pub voices: Vec<VoiceCatalogVoiceDto>,
    /// Honest warnings, including stale fallback.
    pub warnings: Vec<SpeechWarning>,
}

/// Provider-neutral machine representation of one catalog entry.
#[derive(Debug, Serialize)]
pub struct VoiceCatalogVoiceDto {
    /// Provider name.
    pub provider: String,
    /// system, cloned, or generated.
    pub source_type: CatalogSourceType,
    /// Exact provider voice ID.
    pub voice_id: String,
    /// Provider display name.
    pub voice_name: String,
    /// Provider-owned emotion label, if explicitly returned.
    pub emotion_label: Option<String>,
    /// Provider-owned style label, if explicitly returned.
    pub style_label: Option<String>,
    /// Provider-owned display descriptions.
    pub description: Vec<String>,
    /// Provider creation time, if returned.
    pub created_time: Option<String>,
}

impl VoiceCatalogResponse {
    /// Convert a catalog outcome into the stable Machine JSON envelope.
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
            audio: SpeechAudioDto {
                format: profile.audio.format.clone(),
                sample_rate: profile.audio.sample_rate,
                bitrate: profile.audio.bitrate,
                channel: profile.audio.channel,
            },
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

/// Map a Voice Catalog cache/provider failure without exposing credentials or
/// the raw provider response.
pub fn voice_catalog_error(error: &VoiceCatalogError) -> MachineError {
    match error {
        VoiceCatalogError::Storage(SpeechStoreError::Unavailable { path, message }) => {
            storage_unavailable(path, message)
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
}
