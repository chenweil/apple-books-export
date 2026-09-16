//! Annotation Speech 领域（ADR 0007）。
//!
//! 本模块拥有 Voice Profile、Voice Catalog 与非秘密本地 Speech 状态。
//! Profile 命令只读取注入的目录来源；真实网络请求只由显式的 speech voices
//! use case 发起。

pub mod catalog;
pub mod machine;
pub mod profile;
pub mod senseaudio;
pub mod store;

pub use catalog::{
    CatalogAvailability, CatalogSourceType, CatalogVoice, NoCatalogSource, UnverifiedReason,
    VoiceCatalog, VoiceCatalogSource, VoiceVerification, CATALOG_FRESHNESS_HOURS,
};
pub use machine::{
    SpeechProfileDto, SpeechProfileResponse, VoiceCatalogResponse, VoiceCatalogReceipt,
    VoiceCatalogVoiceDto,
};
pub use profile::{
    AudioSettings, Hundredths, ProfileDraft, ProfileError, ProfileErrorReason, ProfileVerification,
    VerificationStatus, VoiceProfile, DEFAULT_API_KEY_ENV, DEFAULT_MODEL, DEFAULT_VOICE_ID,
    SENSEAUDIO_PROVIDER,
};
pub use senseaudio::{
    load_or_refresh_voice_catalog, CachedVoiceCatalogSource, SenseAudioClient, SenseAudioError,
    VoiceCatalogError, VoiceCatalogOutcome, SENSEAUDIO_API_BASE_URL_ENV,
    SENSEAUDIO_DEFAULT_BASE_URL,
};
pub use store::{
    SpeechConfig, SpeechStore, SpeechStoreError, SPEECH_CONFIG_SCHEMA_VERSION,
    VOICE_CATALOG_SCHEMA_VERSION,
};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use std::path::PathBuf;

/// `speech profile` 的操作名。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileOperation {
    /// `speech profile show`
    Show,
    /// `speech profile set`
    Set,
    /// `speech profile reset`
    Reset,
}

impl ProfileOperation {
    /// 收据中 `operation` 字段的稳定取值。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Show => "profile_show",
            Self::Set => "profile_set",
            Self::Reset => "profile_reset",
        }
    }
}

/// 结构化 warning：`code` 稳定，`reason` 说明为什么不能宣称当前可用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpeechWarning {
    /// 稳定 warning code。
    pub code: &'static str,
    /// 稳定原因字符串。
    pub reason: &'static str,
    /// 面向人类的说明。
    pub message: String,
}

impl SpeechWarning {
    /// 未验证 Voice Profile 的 warning code。
    pub const UNVERIFIED_CODE: &'static str = "SPEECH_VOICE_UNVERIFIED";
    /// 刷新失败、正在展示旧缓存目录时的 warning code。
    pub const STALE_CATALOG_CODE: &'static str = "SPEECH_VOICE_CATALOG_STALE";
    /// 账号目录三组全缺、当前没有任何可用音色时的 warning code。
    pub const EMPTY_CATALOG_CODE: &'static str = "SPEECH_VOICE_CATALOG_EMPTY";

    /// 构造未验证 warning；`reason` 为 `None` 表示磁盘上的 Voice Profile 本来就没被验证过。
    pub fn unverified(reason: Option<UnverifiedReason>) -> Self {
        let clause = match reason {
            None => "This stored Voice Profile is not verified against a current Voice Catalog.",
            Some(UnverifiedReason::NoCatalog) => {
                "No Voice Catalog was available, so voice availability was not checked."
            }
            Some(UnverifiedReason::StaleCatalog) => {
                "The cached Voice Catalog is older than 24 hours and is not current permission evidence."
            }
            Some(UnverifiedReason::OtherProvider) => {
                "The available Voice Catalog belongs to a different Speech Provider, so voice availability was not checked."
            }
        };
        // 只有 set/reset 会写入文件；show 只是读取磁盘上的状态。
        let consequence = if reason.is_some() {
            "The Voice Profile is saved as unverified and cannot authorize a Speech Attempt until generation-time validation succeeds."
        } else {
            "It cannot authorize a Speech Attempt until generation-time validation succeeds."
        };
        Self {
            code: Self::UNVERIFIED_CODE,
            reason: reason
                .map(UnverifiedReason::as_str)
                .unwrap_or(machine::STORED_UNVERIFIED_REASON),
            message: format!("{clause} {consequence}"),
        }
    }

    /// 构造 stale Voice Catalog 回退时的诚实 warning。
    pub fn stale_catalog(fetched_at: DateTime<Utc>, refresh_reason: &str) -> Self {
        let fetched_at = fetched_at.to_rfc3339();
        Self {
            code: Self::STALE_CATALOG_CODE,
            reason: "stale_catalog",
            message: format!(
                "Voice Catalog refresh failed ({refresh_reason}); showing the catalog fetched at {fetched_at}. It is stale and is not current permission evidence."
            ),
        }
    }

    /// 账号可见目录为空：合法，但不能当成任何音色的权限证据。
    pub fn empty_catalog() -> Self {
        Self {
            code: Self::EMPTY_CATALOG_CODE,
            reason: "empty_catalog",
            message: "账号未返回音色。This catalog contains no voices and cannot authorize a Speech Attempt.".to_string(),
        }
    }
}

/// profile use case 的错误。
#[derive(Debug)]
pub enum SpeechError {
    /// Speech 状态根不可用或磁盘配置不可信。
    Storage(SpeechStoreError),
    /// Profile 本地校验失败。
    Profile(ProfileError),
    /// Voice Catalog 缓存或供应商访问失败。
    VoiceCatalog(senseaudio::VoiceCatalogError),
    /// 新鲜 Voice Catalog 明确没有这个音色。
    VoiceUnavailable {
        /// Speech Provider。
        provider: String,
        /// 被拒绝的音色 ID。
        voice_id: String,
    },
}

impl std::fmt::Display for SpeechError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(error) => write!(f, "{error}"),
            Self::Profile(error) => write!(f, "{}", error.message()),
            Self::VoiceCatalog(error) => write!(f, "{error}"),
            Self::VoiceUnavailable { provider, voice_id } => write!(
                f,
                "voice '{voice_id}' is not available for provider '{provider}'"
            ),
        }
    }
}

/// 一次 profile 操作的结果。
#[derive(Debug)]
pub struct ProfileOutcome {
    /// 操作名。
    pub operation: ProfileOperation,
    /// 落盘后的非秘密配置。
    pub config: SpeechConfig,
    /// 配置文件路径。
    pub config_path: PathBuf,
    /// 结构化 warning。
    pub warnings: Vec<SpeechWarning>,
}

impl ProfileOutcome {
    /// 转换成 Machine JSON 响应。
    pub fn to_machine_response(&self) -> SpeechProfileResponse {
        SpeechProfileResponse::new(
            self.operation,
            &self.config,
            &self.config_path,
            self.warnings.clone(),
        )
    }
}

fn finish(
    operation: ProfileOperation,
    config: SpeechConfig,
    store: &SpeechStore,
    warnings: Vec<SpeechWarning>,
) -> ProfileOutcome {
    ProfileOutcome {
        operation,
        config,
        config_path: store.config_path(),
        warnings,
    }
}

/// 读取全局 Voice Profile。不联网，也不创建任何文件。
pub fn show_profile(store: &SpeechStore) -> Result<ProfileOutcome, SpeechError> {
    let config = store.load_config().map_err(SpeechError::Storage)?;
    let warnings = if config.profile.verification.status == VerificationStatus::Verified {
        Vec::new()
    } else {
        vec![SpeechWarning::unverified(None)]
    };
    Ok(finish(ProfileOperation::Show, config, store, warnings))
}

/// 设置全局 Voice Profile。
///
/// 顺序固定：本地结构/范围校验 → API Key 环境变量名校验 → 目录可用性决策 → 落盘。
/// 只有通过校验才会写入；新鲜目录缺少该音色时返回 `SPEECH_VOICE_UNAVAILABLE` 且不落盘。
pub fn set_profile(
    store: &SpeechStore,
    draft: &ProfileDraft,
    catalog_source: &dyn VoiceCatalogSource,
    now: DateTime<Utc>,
) -> Result<ProfileOutcome, SpeechError> {
    let current = store.load_config().map_err(SpeechError::Storage)?;
    let profile = profile::resolve_profile(&current.profile, draft).map_err(SpeechError::Profile)?;
    let api_key_env = match draft.api_key_env {
        Some(ref raw) => profile::parse_api_key_env(raw).map_err(SpeechError::Profile)?,
        None => current.api_key_env.clone(),
    };

    let availability = catalog_source.current_catalog(&profile.provider);
    let (profile, warnings) = match catalog::verify_voice(&profile, &availability, now) {
        VoiceVerification::Verified => {
            let mut verified = profile;
            verified.verification = ProfileVerification::verified(&format_timestamp(now));
            (verified, Vec::new())
        }
        VoiceVerification::Unverified(reason) => {
            (profile, vec![SpeechWarning::unverified(Some(reason))])
        }
        VoiceVerification::Unavailable => {
            return Err(SpeechError::VoiceUnavailable {
                provider: profile.provider,
                voice_id: profile.voice_id,
            })
        }
    };

    let config = SpeechConfig {
        api_key_env,
        profile,
    };
    store.save_config(&config).map_err(SpeechError::Storage)?;
    Ok(finish(ProfileOperation::Set, config, store, warnings))
}

/// 恢复默认全局 Voice Profile。
///
/// 只重置 Profile 本身；`api_key_env` 等非秘密配置项保持不变。不联网。
pub fn reset_profile(store: &SpeechStore) -> Result<ProfileOutcome, SpeechError> {
    let mut config = store.load_config().map_err(SpeechError::Storage)?;
    config.profile = VoiceProfile::default();
    store.save_config(&config).map_err(SpeechError::Storage)?;
    let warnings = vec![SpeechWarning::unverified(Some(UnverifiedReason::NoCatalog))];
    Ok(finish(ProfileOperation::Reset, config, store, warnings))
}

fn format_timestamp(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use tempfile::TempDir;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-11T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc)
    }

    fn store() -> (TempDir, SpeechStore) {
        let home = tempfile::tempdir().expect("temp home");
        let store = SpeechStore::from_home(home.path());
        (home, store)
    }

    fn catalog_at(fetched_at: DateTime<Utc>, voice_ids: &[&str]) -> CatalogAvailability {
        CatalogAvailability::Available(VoiceCatalog {
            provider: SENSEAUDIO_PROVIDER.to_string(),
            fetched_at,
            voices: voice_ids
                .iter()
                .map(|voice_id| CatalogVoice {
                    source_type: catalog::CatalogSourceType::System,
                    voice_id: voice_id.to_string(),
                    voice_name: format!("voice {voice_id}"),
                    emotion_label: Some("平稳".to_string()),
                    style_label: None,
                    description: vec!["平稳".to_string()],
                    created_time: None,
                })
                .collect(),
        })
    }

    struct FixedSource(CatalogAvailability);

    impl VoiceCatalogSource for FixedSource {
        fn current_catalog(&self, _provider: &str) -> CatalogAvailability {
            self.0.clone()
        }
    }

    /// 负向控制：本地校验失败时不允许触碰目录来源。
    struct PanicSource;

    impl VoiceCatalogSource for PanicSource {
        fn current_catalog(&self, _provider: &str) -> CatalogAvailability {
            panic!("profile validation must fail before any catalog lookup")
        }
    }

    fn draft(voice_id: &str) -> ProfileDraft {
        ProfileDraft {
            voice_id: Some(voice_id.to_string()),
            ..ProfileDraft::default()
        }
    }

    #[test]
    fn set_with_a_fresh_catalog_hit_verifies_the_profile() {
        let (_home, store) = store();
        let source = FixedSource(catalog_at(now() - Duration::hours(1), &[DEFAULT_VOICE_ID]));

        let outcome = set_profile(&store, &draft(DEFAULT_VOICE_ID), &source, now()).expect("set");

        assert_eq!(outcome.operation, ProfileOperation::Set);
        assert_eq!(
            outcome.config.profile.verification.status,
            VerificationStatus::Verified
        );
        assert_eq!(
            outcome.config.profile.verification.verified_at.as_deref(),
            Some("2026-09-11T12:00:00Z")
        );
        assert!(outcome.warnings.is_empty());

        let stored = store.load_config().expect("stored config");
        assert_eq!(stored.profile.verification.status, VerificationStatus::Verified);
        assert_eq!(
            stored.profile.verification.verified_at.as_deref(),
            Some("2026-09-11T12:00:00Z")
        );
    }

    #[test]
    fn set_with_a_fresh_catalog_miss_fails_without_writing_anything() {
        let (_home, store) = store();
        let source = FixedSource(catalog_at(now(), &["female_0007_b"]));

        let error = set_profile(&store, &draft(DEFAULT_VOICE_ID), &source, now())
            .expect_err("missing voice");

        match error {
            SpeechError::VoiceUnavailable { provider, voice_id } => {
                assert_eq!(provider, SENSEAUDIO_PROVIDER);
                assert_eq!(voice_id, DEFAULT_VOICE_ID);
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert!(
            !store.config_path().exists(),
            "an unavailable voice must not be persisted"
        );
    }

    #[test]
    fn set_with_a_stale_catalog_saves_unverified_with_a_stable_reason() {
        let (_home, store) = store();
        let source = FixedSource(catalog_at(now() - Duration::hours(25), &[DEFAULT_VOICE_ID]));

        let outcome = set_profile(&store, &draft(DEFAULT_VOICE_ID), &source, now()).expect("set");

        assert_eq!(
            outcome.config.profile.verification.status,
            VerificationStatus::Unverified
        );
        assert_eq!(outcome.config.profile.verification.verified_at, None);
        assert_eq!(outcome.warnings.len(), 1);
        assert_eq!(outcome.warnings[0].code, SpeechWarning::UNVERIFIED_CODE);
        assert_eq!(outcome.warnings[0].reason, "stale_catalog");
        assert_eq!(
            store.load_config().expect("stored").profile.verification.status,
            VerificationStatus::Unverified
        );
    }

    #[test]
    fn set_without_a_catalog_saves_unverified_with_a_stable_reason() {
        let (_home, store) = store();
        let source = NoCatalogSource;

        let outcome = set_profile(&store, &draft(DEFAULT_VOICE_ID), &source, now()).expect("set");

        assert_eq!(outcome.warnings.len(), 1);
        assert_eq!(outcome.warnings[0].reason, "no_catalog");
        assert_eq!(outcome.config.api_key_env, DEFAULT_API_KEY_ENV);
    }

    #[test]
    fn set_validates_locally_before_consulting_the_catalog() {
        let (_home, store) = store();
        let invalid = ProfileDraft {
            voice_id: Some(DEFAULT_VOICE_ID.to_string()),
            speed: Some("1.005".to_string()),
            ..ProfileDraft::default()
        };

        let error = set_profile(&store, &invalid, &PanicSource, now()).expect_err("invalid speed");

        match error {
            SpeechError::Profile(error) => {
                assert_eq!(error.field, Some("speed"));
                assert_eq!(error.reason, ProfileErrorReason::NotHundredth);
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert!(!store.config_path().exists());
    }

    #[test]
    fn set_never_carries_forward_a_stored_verified_status() {
        let (_home, store) = store();
        let verified_source = FixedSource(catalog_at(now(), &[DEFAULT_VOICE_ID]));
        set_profile(&store, &draft(DEFAULT_VOICE_ID), &verified_source, now()).expect("verify");
        assert_eq!(
            store.load_config().expect("stored").profile.verification.status,
            VerificationStatus::Verified
        );

        let outcome = set_profile(
            &store,
            &ProfileDraft {
                voice_id: Some(DEFAULT_VOICE_ID.to_string()),
                speed: Some("1.5".to_string()),
                ..ProfileDraft::default()
            },
            &NoCatalogSource,
            now() + Duration::hours(1),
        )
        .expect("set again");

        assert_eq!(
            outcome.config.profile.verification.status,
            VerificationStatus::Unverified
        );
        assert_eq!(outcome.warnings[0].reason, "no_catalog");
        let stored = store.load_config().expect("stored");
        assert_eq!(stored.profile.verification.status, VerificationStatus::Unverified);
        assert_eq!(stored.profile.verification.verified_at, None);
    }

    #[test]
    fn set_persists_the_api_key_environment_variable_name_only() {
        let (_home, store) = store();
        let draft = ProfileDraft {
            voice_id: Some(DEFAULT_VOICE_ID.to_string()),
            api_key_env: Some("CUSTOM_SENSEAUDIO_KEY".to_string()),
            ..ProfileDraft::default()
        };

        let outcome = set_profile(&store, &draft, &NoCatalogSource, now()).expect("set");

        assert_eq!(outcome.config.api_key_env, "CUSTOM_SENSEAUDIO_KEY");
        let raw = std::fs::read_to_string(store.config_path()).expect("read config");
        assert!(raw.contains("CUSTOM_SENSEAUDIO_KEY"));
    }

    #[test]
    fn set_rejects_an_invalid_api_key_environment_name() {
        let (_home, store) = store();
        let draft = ProfileDraft {
            voice_id: Some(DEFAULT_VOICE_ID.to_string()),
            api_key_env: Some("NOT A NAME".to_string()),
            ..ProfileDraft::default()
        };

        let error = set_profile(&store, &draft, &NoCatalogSource, now()).expect_err("bad env name");

        match error {
            SpeechError::Profile(error) => {
                assert_eq!(error.field, Some("api_key_env"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert!(!store.config_path().exists());
    }

    #[test]
    fn reset_restores_defaults_and_keeps_the_api_key_env_name() {
        let (_home, store) = store();
        let verified_source = FixedSource(catalog_at(now(), &["female_0007_b"]));
        set_profile(
            &store,
            &ProfileDraft {
                voice_id: Some("female_0007_b".to_string()),
                speed: Some("1.75".to_string()),
                api_key_env: Some("CUSTOM_SENSEAUDIO_KEY".to_string()),
                ..ProfileDraft::default()
            },
            &verified_source,
            now(),
        )
        .expect("set");

        let outcome = reset_profile(&store).expect("reset");

        assert_eq!(outcome.operation, ProfileOperation::Reset);
        assert_eq!(outcome.config.profile, VoiceProfile::default());
        assert_eq!(outcome.config.api_key_env, "CUSTOM_SENSEAUDIO_KEY");
        assert_eq!(outcome.warnings[0].reason, "no_catalog");

        let stored = store.load_config().expect("stored");
        assert_eq!(stored.profile, VoiceProfile::default());
        assert_eq!(
            stored.profile.verification.status,
            VerificationStatus::Unverified
        );
    }

    #[test]
    fn show_reports_stored_state_honestly() {
        let (_home, store) = store();

        let default = show_profile(&store).expect("show default");
        assert_eq!(default.operation, ProfileOperation::Show);
        assert_eq!(default.config.profile, VoiceProfile::default());
        assert_eq!(default.warnings.len(), 1);
        assert_eq!(default.warnings[0].reason, "stored_unverified");

        let verified_source = FixedSource(catalog_at(now(), &[DEFAULT_VOICE_ID]));
        set_profile(&store, &draft(DEFAULT_VOICE_ID), &verified_source, now()).expect("verify");

        let verified = show_profile(&store).expect("show verified");
        assert_eq!(
            verified.config.profile.verification.status,
            VerificationStatus::Verified
        );
        assert!(verified.warnings.is_empty());
    }

    #[test]
    fn show_does_not_create_the_speech_root() {
        let (_home, store) = store();

        show_profile(&store).expect("show");

        assert!(!store.root().exists());
    }
}
