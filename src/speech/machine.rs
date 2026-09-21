//! Speech 的 Machine JSON 收据、warning 与错误映射。
//!
//! 复用 [`crate::machine`] 的 envelope；profile 相关错误额外带上稳定的 `details`，
//! 让机器消费者不用解析 message 就能知道是哪个字段、哪种原因。

use crate::machine::{MachineError, SCHEMA_VERSION};
use crate::speech::catalog::CatalogSourceType;
use crate::speech::profile::{ProfileError, VoiceProfile};
use crate::speech::{
    GenerateError, ProfileOperation, SpeechConfig, SpeechError, SpeechStoreError, SpeechWarning,
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

/// 把生成 use case 错误映射成稳定的 Machine JSON envelope。
///
/// 供应商失败按 `SenseAudioError` 类型区分明确失败（`SPEECH_PROVIDER_FAILED`）、
/// 不确定结果（`SPEECH_RESULT_UNKNOWN`）与产物缺失（`SPEECH_AUDIO_INVALID`），
/// 并在 `details` 里带 provider、trace ID、attempt ID 与 outcome，绝不回显原文或密钥。
pub fn generate_error_response(error: &GenerateError) -> MachineError {
    match error {
        GenerateError::Database { message } => MachineError::database_unreadable(message.clone()),
        GenerateError::AssetMissing { asset_id } => MachineError::invalid_asset_id(asset_id),
        GenerateError::AnnotationMissing {
            asset_id,
            annotation_id,
        } => machine_error(
            "INVALID_ANNOTATION_ID",
            format!("Annotation '{annotation_id}' was not found for asset_id '{asset_id}'."),
            "Run `apple-books-exporter annotations --asset-id <id> --json` and use an annotation id from that book.",
            json!({ "asset_id": asset_id, "annotation_id": annotation_id }),
        ),
        GenerateError::ContentUnavailable { content_kind } => machine_error(
            "SPEECH_CONTENT_UNAVAILABLE",
            format!(
                "The requested {} content is empty for this annotation.",
                content_kind.as_str()
            ),
            "Choose a content kind that has text, or pick another annotation.",
            json!({ "content_kind": content_kind.as_str() }),
        ),
        GenerateError::TextTooLong { characters } => machine_error(
            "SPEECH_TEXT_TOO_LONG",
            format!(
                "Normalized speech text is {characters} characters, over the 10000-character limit."
            ),
            "Speech text is never truncated or summarized. Choose a shorter highlight or note.",
            json!({ "characters": characters, "limit": crate::speech::SPEECH_TEXT_LIMIT }),
        ),
        GenerateError::Profile(error) => profile_invalid(error),
        GenerateError::VoiceUnavailable { provider, voice_id } => {
            voice_unavailable(provider, voice_id)
        }
        GenerateError::Storage(error) => match error {
            crate::speech::SpeechStoreError::Unavailable { path, message } => {
                storage_unavailable(path, message)
            }
            crate::speech::SpeechStoreError::InvalidConfig(error) => profile_invalid(error),
            crate::speech::SpeechStoreError::UnsupportedSchemaVersion(version) => {
                unsupported_schema_version(*version)
            }
        },
        GenerateError::InProgress { clip_id } => machine_error(
            "SPEECH_IN_PROGRESS",
            format!("Another generation for clip '{clip_id}' is already in progress."),
            "Wait for the in-progress generation to finish, then retry.",
            json!({ "clip_id": clip_id }),
        ),
        GenerateError::Blocked {
            clip_id,
            attempt_id,
            status,
            error_code,
        } => {
            let (code, outcome) = match status {
                crate::speech::store::AttemptStatus::ProviderSucceededArtifactMissing => {
                    ("SPEECH_AUDIO_INVALID", "provider_succeeded_artifact_missing")
                }
                _ => ("SPEECH_RESULT_UNKNOWN", "unknown"),
            };
            machine_error(
                code,
                format!("Clip '{clip_id}' is blocked by an earlier uncertain outcome."),
                "Retry with --regenerate only if another billed generation is acceptable.",
                json!({
                    "provider": "senseaudio",
                    "clip_id": clip_id,
                    "attempt_id": attempt_id,
                    "product_error_code": error_code,
                    "outcome": outcome,
                }),
            )
        }
        GenerateError::Provider(failure) => {
            let code = generate_provider_code(&failure.kind);
            let remediation = match &failure.kind {
                crate::speech::senseaudio::SenseAudioError::Transport => {
                    "The request may have reached the provider. Retry with --regenerate only if another billed generation is acceptable."
                }
                crate::speech::senseaudio::SenseAudioError::MissingApiKey => {
                    "Set the API key in the environment variable named by the Voice Profile, then retry."
                }
                crate::speech::senseaudio::SenseAudioError::AuthenticationFailed => {
                    "Verify the SenseAudio API key in the configured environment variable, then retry."
                }
                crate::speech::senseaudio::SenseAudioError::RateLimited => {
                    "Slow down and retry the generation later."
                }
                crate::speech::senseaudio::SenseAudioError::InvalidAudio => {
                    "The provider reported success but the audio could not be validated. Retry with --regenerate."
                }
                _ => "Check the SenseAudio status and trace id, then retry the generation.",
            };
            machine_error(
                code,
                failure.kind.to_string(),
                remediation,
                json!({
                    "provider": "senseaudio",
                    "reason": failure.kind.reason_code(),
                    "trace_id": failure.trace_id,
                    "attempt_id": failure.attempt_id,
                    "outcome": provider_outcome(&failure.kind),
                }),
            )
        }
    }
}

/// 生成场景的供应商失败 → 稳定产品错误码（与计费语义一致：transport 是结果未知）。
fn generate_provider_code(kind: &crate::speech::senseaudio::SenseAudioError) -> &'static str {
    use crate::speech::senseaudio::SenseAudioError;
    match kind {
        SenseAudioError::MissingApiKey | SenseAudioError::AuthenticationFailed => "SPEECH_AUTH_FAILED",
        SenseAudioError::RateLimited => "SPEECH_RATE_LIMITED",
        SenseAudioError::Transport => "SPEECH_RESULT_UNKNOWN",
        SenseAudioError::InvalidAudio => "SPEECH_AUDIO_INVALID",
        SenseAudioError::InvalidResponse | SenseAudioError::ProviderFailed => "SPEECH_PROVIDER_FAILED",
    }
}

/// 生成场景的供应商失败 → `details.outcome` 稳定值。
fn provider_outcome(kind: &crate::speech::senseaudio::SenseAudioError) -> &'static str {
    use crate::speech::senseaudio::SenseAudioError;
    match kind {
        SenseAudioError::Transport => "unknown",
        SenseAudioError::InvalidAudio => "provider_succeeded_artifact_missing",
        _ => "failed",
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
