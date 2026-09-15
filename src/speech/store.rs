//! 用户级 Speech 状态根与 `config.json`。
//!
//! 根目录固定为用户级 `~/Library/Application Support/books-exporter/speech/`，不跟随
//! 当前工作目录、`--config` 或 Markdown 导出目录。测试通过 [`SpeechStore::from_home`]
//! 注入临时根目录，因此不会写真实 User Application Support。
//!
//! 配置文件只保存非秘密字段：Voice Profile 与 API Key 的**环境变量名**。
//! 任何密钥值都不会进入这个文件。

use crate::speech::catalog::VoiceCatalog;
use crate::speech::profile::{
    parse_api_key_env, ProfileError, ProfileVerification, VerificationStatus, VoiceProfile,
    DEFAULT_API_KEY_ENV,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// config.json schema version.
pub const SPEECH_CONFIG_SCHEMA_VERSION: u32 = 1;
/// Voice Catalog cache schema version.
pub const VOICE_CATALOG_SCHEMA_VERSION: u32 = 1;
/// 配置文件文件名。
const CONFIG_FILE_NAME: &str = "config.json";
/// Voice Catalog directory name.
const VOICE_CATALOG_DIR_NAME: &str = "voices";
/// 同文件系统的临时文件；rename 保证提交是原子的。
const CONFIG_TMP_FILE_NAME: &str = "config.json.tmp";

/// 用户级非秘密 Speech 配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechConfig {
    /// API Key 的环境变量**名**；密钥值永远不落盘。
    pub api_key_env: String,
    /// 全局 Voice Profile。
    pub profile: VoiceProfile,
}

impl Default for SpeechConfig {
    fn default() -> Self {
        Self {
            api_key_env: DEFAULT_API_KEY_ENV.to_string(),
            profile: VoiceProfile::default(),
        }
    }
}

/// 读不到或写不了 Speech 状态根，或者磁盘上的配置不可信。
#[derive(Debug)]
pub enum SpeechStoreError {
    /// 根目录不可用：不可写、不可读、空间或权限问题。
    Unavailable {
        /// 出问题的路径。
        path: PathBuf,
        /// 底层错误说明，不含任何秘密。
        message: String,
    },
    /// 磁盘上的配置不是合法 Voice Profile。
    InvalidConfig(ProfileError),
    /// 配置声明的 schema 版本不是本版本支持的版本。
    UnsupportedSchemaVersion(u32),
}

impl SpeechStoreError {
    fn unavailable(path: &Path, error: std::io::Error) -> Self {
        Self::Unavailable {
            path: path.to_path_buf(),
            message: error.to_string(),
        }
    }
}

impl std::fmt::Display for SpeechStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable { path, message } => write!(
                f,
                "the Speech state directory is not usable at {}: {message}",
                path.display()
            ),
            Self::InvalidConfig(error) => write!(f, "{}", error.message()),
            Self::UnsupportedSchemaVersion(version) => write!(
                f,
                "speech configuration schema version {version} is not supported"
            ),
        }
    }
}

/// 用户级 Speech 状态根。
#[derive(Debug, Clone)]
pub struct SpeechStore {
    root: PathBuf,
}

impl SpeechStore {
    /// 由调用方注入的主目录推导状态根。
    ///
    /// 只依赖注入的 `home`：不读当前工作目录，也不读 `--config`，所以测试可以完全隔离。
    pub fn from_home(home: &Path) -> Self {
        Self {
            root: home
                .join("Library")
                .join("Application Support")
                .join("books-exporter")
                .join("speech"),
        }
    }

    /// Speech 状态根目录。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 非秘密配置文件路径。
    pub fn config_path(&self) -> PathBuf {
        self.root.join(CONFIG_FILE_NAME)
    }

    /// Provider Voice Catalog cache path.
    ///
    /// Provider names are part of the application contract, not arbitrary
    /// filesystem paths. Unknown path characters are collapsed to a safe
    /// placeholder so a malformed caller can never escape the Speech root.
    pub fn voice_catalog_path(&self, provider: &str) -> PathBuf {
        let safe_provider = if is_safe_provider_name(provider) {
            provider
        } else {
            "invalid-provider"
        };
        self.root
            .join(VOICE_CATALOG_DIR_NAME)
            .join(format!("{safe_provider}.json"))
    }

    fn config_tmp_path(&self) -> PathBuf {
        self.root.join(CONFIG_TMP_FILE_NAME)
    }

    /// 读取配置；文件不存在时返回 ADR 0007 的默认 Profile，不创建任何目录。
    pub fn load_config(&self) -> Result<SpeechConfig, SpeechStoreError> {
        let path = self.config_path();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(SpeechConfig::default())
            }
            Err(error) => return Err(SpeechStoreError::unavailable(&path, error)),
        };

        // 先只探测 schema 版本：未来版本的文件必须报 UNSUPPORTED_SCHEMA_VERSION，
        // 而不是被当成损坏的 Profile。
        #[derive(Deserialize)]
        struct SchemaProbe {
            schema_version: u32,
        }
        let probe: SchemaProbe = serde_json::from_str(&text)
            .map_err(|_| SpeechStoreError::InvalidConfig(ProfileError::stored_config_invalid()))?;
        if probe.schema_version != SPEECH_CONFIG_SCHEMA_VERSION {
            return Err(SpeechStoreError::UnsupportedSchemaVersion(probe.schema_version));
        }

        let file: ConfigFile = serde_json::from_str(&text)
            .map_err(|_| SpeechStoreError::InvalidConfig(ProfileError::stored_config_invalid()))?;
        file.into_config()
    }

    /// Read a provider Voice Catalog cache without creating the Speech root.
    pub fn load_voice_catalog(
        &self,
        provider: &str,
    ) -> Result<Option<VoiceCatalog>, SpeechStoreError> {
        if !is_safe_provider_name(provider) {
            return Err(SpeechStoreError::Unavailable {
                path: self.voice_catalog_path(provider),
                message: "the provider name is not a safe catalog identifier".to_string(),
            });
        }

        let path = self.voice_catalog_path(provider);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(SpeechStoreError::unavailable(&path, error)),
        };
        #[derive(Deserialize)]
        struct SchemaProbe {
            schema_version: u32,
        }
        let probe: SchemaProbe = serde_json::from_str(&text).map_err(|_| {
            SpeechStoreError::Unavailable {
                path: path.clone(),
                message: "the Voice Catalog cache is invalid".to_string(),
            }
        })?;
        if probe.schema_version != VOICE_CATALOG_SCHEMA_VERSION {
            return Err(SpeechStoreError::UnsupportedSchemaVersion(
                probe.schema_version,
            ));
        }

        let file: VoiceCatalogFile = serde_json::from_str(&text).map_err(|error| {
            SpeechStoreError::Unavailable {
                path: path.clone(),
                message: format!("the Voice Catalog cache is invalid: {error}"),
            }
        })?;
        let catalog = file.into_catalog();
        if catalog.provider != provider {
            return Err(SpeechStoreError::Unavailable {
                path,
                message: "the Voice Catalog cache belongs to a different provider".to_string(),
            });
        }
        catalog.validate().map_err(|error| {
            SpeechStoreError::Unavailable {
                path,
                message: format!("the Voice Catalog cache is invalid: {error}"),
            }
        })?;
        Ok(Some(catalog))
    }

    /// Atomically persist a non-secret provider Voice Catalog cache.
    pub fn save_voice_catalog(
        &self,
        catalog: &VoiceCatalog,
    ) -> Result<(), SpeechStoreError> {
        catalog
            .validate()
            .map_err(|error| SpeechStoreError::Unavailable {
                path: self.voice_catalog_path(&catalog.provider),
                message: format!("the Voice Catalog is invalid: {error}"),
            })?;
        if !is_safe_provider_name(&catalog.provider) {
            return Err(SpeechStoreError::Unavailable {
                path: self.voice_catalog_path(&catalog.provider),
                message: "the provider name is not a safe catalog identifier".to_string(),
            });
        }

        let directory = self.root.join(VOICE_CATALOG_DIR_NAME);
        fs::create_dir_all(&directory)
            .map_err(|error| SpeechStoreError::unavailable(&directory, error))?;
        let path = self.voice_catalog_path(&catalog.provider);
        let temporary = path.with_extension("json.tmp");
        let mut json = serde_json::to_string_pretty(&VoiceCatalogFile::from(catalog))
            .map_err(|error| SpeechStoreError::Unavailable {
                path: directory.clone(),
                message: format!("could not serialize the Voice Catalog: {error}"),
            })?;
        json.push('\n');
        write_synced(&temporary, json.as_bytes())
            .map_err(|error| SpeechStoreError::unavailable(&temporary, error))?;
        fs::rename(&temporary, &path).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            SpeechStoreError::unavailable(&path, error)
        })?;
        Ok(())
    }

    /// 原子写入配置：同目录临时文件 + fsync + rename。
    ///
    /// 写入前先做完整校验，因此磁盘上不会出现无法再次加载的 Profile。
    pub fn save_config(&self, config: &SpeechConfig) -> Result<(), SpeechStoreError> {
        config.profile.validate().map_err(SpeechStoreError::InvalidConfig)?;
        let api_key_env =
            parse_api_key_env(&config.api_key_env).map_err(SpeechStoreError::InvalidConfig)?;

        let root = self.root();
        fs::create_dir_all(root).map_err(|error| SpeechStoreError::unavailable(root, error))?;

        let file = ConfigFile::from(&SpeechConfig {
            api_key_env,
            profile: config.profile.clone(),
        });
        let mut json = serde_json::to_string_pretty(&file)
            .map_err(|error| SpeechStoreError::unavailable(root, std::io::Error::other(error.to_string())))?;
        json.push('\n');

        let tmp = self.config_tmp_path();
        write_synced(&tmp, json.as_bytes())
            .map_err(|error| SpeechStoreError::unavailable(&tmp, error))?;
        fs::rename(&tmp, self.config_path()).map_err(|error| {
            let _ = fs::remove_file(&tmp);
            SpeechStoreError::unavailable(&self.config_path(), error)
        })?;
        Ok(())
    }
}

fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn is_safe_provider_name(provider: &str) -> bool {
    !provider.is_empty()
        && provider != "."
        && provider != ".."
        && provider
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_'))
}

/// `config.json` 的磁盘 schema。字段名就是文件字段名。
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    schema_version: u32,
    api_key_env: String,
    voice_profile: ProfileFile,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileFile {
    provider: String,
    model: String,
    voice_id: String,
    #[serde(default)]
    emotion_label: Option<String>,
    #[serde(default)]
    style_label: Option<String>,
    /// 语速的百分之一单位整数：精确往返，避免浮点漂移。
    speed_x100: i32,
    /// 音量的百分之一单位整数。
    volume_x100: i32,
    pitch: i32,
    verification_status: VerificationStatus,
    #[serde(default)]
    verified_at: Option<String>,
    audio: AudioFile,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AudioFile {
    format: String,
    sample_rate: u32,
    bitrate: u32,
    channel: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VoiceCatalogFile {
    schema_version: u32,
    provider: String,
    fetched_at: chrono::DateTime<chrono::Utc>,
    voices: Vec<crate::speech::catalog::CatalogVoice>,
}

impl From<&VoiceCatalog> for VoiceCatalogFile {
    fn from(catalog: &VoiceCatalog) -> Self {
        Self {
            schema_version: VOICE_CATALOG_SCHEMA_VERSION,
            provider: catalog.provider.clone(),
            fetched_at: catalog.fetched_at,
            voices: catalog.voices.clone(),
        }
    }
}

impl VoiceCatalogFile {
    fn into_catalog(self) -> VoiceCatalog {
        VoiceCatalog {
            provider: self.provider,
            fetched_at: self.fetched_at,
            voices: self.voices,
        }
    }
}

impl From<&SpeechConfig> for ConfigFile {
    fn from(config: &SpeechConfig) -> Self {
        Self {
            schema_version: SPEECH_CONFIG_SCHEMA_VERSION,
            api_key_env: config.api_key_env.clone(),
            voice_profile: ProfileFile::from(&config.profile),
        }
    }
}

impl From<&VoiceProfile> for ProfileFile {
    fn from(profile: &VoiceProfile) -> Self {
        Self {
            provider: profile.provider.clone(),
            model: profile.model.clone(),
            voice_id: profile.voice_id.clone(),
            emotion_label: profile.emotion_label.clone(),
            style_label: profile.style_label.clone(),
            speed_x100: profile.speed.x100(),
            volume_x100: profile.volume.x100(),
            pitch: profile.pitch,
            verification_status: profile.verification.status,
            verified_at: profile.verification.verified_at.clone(),
            audio: AudioFile {
                format: profile.audio.format.clone(),
                sample_rate: profile.audio.sample_rate,
                bitrate: profile.audio.bitrate,
                channel: profile.audio.channel,
            },
        }
    }
}

impl ConfigFile {
    fn into_config(self) -> Result<SpeechConfig, SpeechStoreError> {
        let api_key_env =
            parse_api_key_env(&self.api_key_env).map_err(SpeechStoreError::InvalidConfig)?;
        let profile = self.voice_profile.into_profile()?;
        profile.validate().map_err(SpeechStoreError::InvalidConfig)?;
        Ok(SpeechConfig {
            api_key_env,
            profile,
        })
    }
}

impl ProfileFile {
    fn into_profile(self) -> Result<VoiceProfile, SpeechStoreError> {
        let invalid = || SpeechStoreError::InvalidConfig(ProfileError::stored_config_invalid());

        // verified 必须有验证时间；unverified 不允许携带任何验证声明。
        let verification = match (self.verification_status, self.verified_at) {
            (VerificationStatus::Verified, Some(verified_at)) if !verified_at.trim().is_empty() => {
                ProfileVerification::verified(&verified_at)
            }
            (VerificationStatus::Unverified, None) => ProfileVerification::unverified(),
            _ => return Err(invalid()),
        };

        Ok(VoiceProfile {
            provider: self.provider,
            model: self.model,
            voice_id: self.voice_id,
            emotion_label: self.emotion_label,
            style_label: self.style_label,
            speed: crate::speech::profile::Hundredths::from_x100(self.speed_x100),
            volume: crate::speech::profile::Hundredths::from_x100(self.volume_x100),
            pitch: self.pitch,
            audio: crate::speech::profile::AudioSettings {
                format: self.audio.format,
                sample_rate: self.audio.sample_rate,
                bitrate: self.audio.bitrate,
                channel: self.audio.channel,
            },
            verification,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speech::catalog::{CatalogSourceType, CatalogVoice};
    use crate::speech::profile::{Hundredths, ProfileErrorReason, VerificationStatus};
    use chrono::Utc;
    use tempfile::TempDir;

    fn store() -> (TempDir, SpeechStore) {
        let home = tempfile::tempdir().expect("temp home");
        let store = SpeechStore::from_home(home.path());
        (home, store)
    }

    fn seed(store: &SpeechStore, json: &str) {
        std::fs::create_dir_all(store.root()).expect("speech root");
        std::fs::write(store.config_path(), json).expect("seed config");
    }

    fn config_json(profile_body: &str) -> String {
        format!(
            r#"{{
  "schema_version": 1,
  "api_key_env": "SENSEAUDIO_API_KEY",
  "voice_profile": {{ {profile_body} }}
}}"#
        )
    }

    fn valid_profile_body() -> &'static str {
        r#""provider": "senseaudio",
    "model": "sensenova-tts-2.0",
    "voice_id": "male_0004_a",
    "emotion_label": null,
    "style_label": null,
    "speed_x100": 100,
    "volume_x100": 100,
    "pitch": 0,
    "verification_status": "unverified",
    "verified_at": null,
    "audio": { "format": "mp3", "sample_rate": 32000, "bitrate": 128000, "channel": 2 }"#
    }

    #[test]
    fn root_is_derived_only_from_the_injected_home() {
        let store = SpeechStore::from_home(Path::new("/tmp/some-home"));

        assert_eq!(
            store.root(),
            Path::new("/tmp/some-home/Library/Application Support/books-exporter/speech")
        );
        assert_eq!(
            store.config_path(),
            Path::new("/tmp/some-home/Library/Application Support/books-exporter/speech/config.json")
        );

        let other = SpeechStore::from_home(Path::new("/tmp/other-home"));
        assert_ne!(store.root(), other.root());
    }

    #[test]
    fn missing_config_yields_the_adr_default_profile_directory_free() {
        let (_home, store) = store();

        let config = store.load_config().expect("default config");

        assert_eq!(config.api_key_env, DEFAULT_API_KEY_ENV);
        assert_eq!(config.profile, VoiceProfile::default());
        assert_eq!(
            config.profile.verification.status,
            VerificationStatus::Unverified
        );
        assert!(!store.root().exists(), "loading must not create directories");
    }

    #[test]
    fn saving_and_loading_preserves_exact_hundredths_and_labels() {
        let (_home, store) = store();
        let profile = VoiceProfile {
            voice_id: "female_0007_b".to_string(),
            emotion_label: Some("平稳".to_string()),
            style_label: Some("新闻".to_string()),
            speed: Hundredths::from_x100(101),
            volume: Hundredths::from_x100(1000),
            pitch: -12,
            ..VoiceProfile::default()
        };
        let config = SpeechConfig {
            api_key_env: "CUSTOM_KEY_NAME".to_string(),
            profile: profile.clone(),
        };

        store.save_config(&config).expect("save config");
        let loaded = store.load_config().expect("load config");

        assert_eq!(loaded, config);
        assert_eq!(loaded.profile.speed.to_string(), "1.01");
        assert_eq!(loaded.profile.volume.to_string(), "10.0");

        let raw = std::fs::read_to_string(store.config_path()).expect("read config");
        assert!(raw.contains("\"speed_x100\": 101"));
        assert!(raw.contains("\"volume_x100\": 1000"));
        assert!(raw.contains("\"api_key_env\": \"CUSTOM_KEY_NAME\""));
    }

    #[test]
    fn saving_is_atomic_and_leaves_no_temporary_files() {
        let (_home, store) = store();
        std::fs::create_dir_all(store.root()).expect("speech root");
        // 模拟上次崩溃留下的半成品：保存必须复用它并最终只留 config.json。
        std::fs::write(store.root().join("config.json.tmp"), "{ truncated").expect("stray tmp");

        store
            .save_config(&SpeechConfig::default())
            .expect("save config");

        let entries: Vec<String> = std::fs::read_dir(store.root())
            .expect("read root")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries, vec!["config.json".to_string()]);

        let raw = std::fs::read_to_string(store.config_path()).expect("read config");
        assert!(
            serde_json::from_str::<serde_json::Value>(&raw).is_ok(),
            "the committed file must always be complete JSON"
        );
    }

    #[test]
    fn corrupt_config_is_reported_as_an_invalid_stored_profile() {
        let (_home, store) = store();
        seed(&store, "{ not json");

        let error = store.load_config().expect_err("corrupt config");

        match error {
            SpeechStoreError::InvalidConfig(profile_error) => {
                assert_eq!(profile_error.reason, ProfileErrorReason::StoredConfigInvalid);
                assert_eq!(profile_error.field, None);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn unknown_schema_version_is_rejected_instead_of_guessed() {
        let (_home, store) = store();
        seed(
            &store,
            &config_json(valid_profile_body()).replace("\"schema_version\": 1", "\"schema_version\": 2"),
        );

        let error = store.load_config().expect_err("future schema");

        match error {
            SpeechStoreError::UnsupportedSchemaVersion(version) => assert_eq!(version, 2),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn unexpected_fields_are_rejected_so_no_secret_can_hide_in_the_file() {
        let (_home, store) = store();

        seed(
            &store,
            &config_json(valid_profile_body()).replace(
                "\"schema_version\": 1,",
                "\"schema_version\": 1,\n  \"api_key\": \"canary\",",
            ),
        );
        let top_level = store.load_config().expect_err("top-level secret field");
        assert!(matches!(top_level, SpeechStoreError::InvalidConfig(_)));

        seed(
            &store,
            &config_json(valid_profile_body()).replace(
                "\"pitch\": 0,",
                "\"pitch\": 0,\n    \"api_key\": \"canary\",",
            ),
        );
        let nested = store.load_config().expect_err("nested secret field");
        assert!(matches!(nested, SpeechStoreError::InvalidConfig(_)));
    }

    #[test]
    fn stored_verification_must_be_self_consistent() {
        let (_home, store) = store();

        seed(
            &store,
            &config_json(&valid_profile_body().replace(
                "\"verification_status\": \"unverified\"",
                "\"verification_status\": \"verified\"",
            )),
        );
        let verified_without_time = store.load_config().expect_err("verified without time");
        assert!(matches!(
            verified_without_time,
            SpeechStoreError::InvalidConfig(_)
        ));

        seed(
            &store,
            &config_json(&valid_profile_body().replace(
                "\"verified_at\": null",
                "\"verified_at\": \"2026-09-11T00:00:00Z\"",
            )),
        );
        let unverified_with_time = store.load_config().expect_err("unverified with time");
        assert!(matches!(
            unverified_with_time,
            SpeechStoreError::InvalidConfig(_)
        ));

        seed(
            &store,
            &config_json(&valid_profile_body().replace(
                "\"verification_status\": \"unverified\"",
                "\"verification_status\": \"verified\"",
            ).replace(
                "\"verified_at\": null",
                "\"verified_at\": \"2026-09-11T00:00:00Z\"",
            )),
        );
        let verified = store.load_config().expect("consistent verified config");
        assert_eq!(verified.profile.verification.status, VerificationStatus::Verified);
    }

    #[test]
    fn stored_out_of_range_values_surface_the_offending_field() {
        let (_home, store) = store();
        seed(
            &store,
            &config_json(&valid_profile_body().replace("\"speed_x100\": 100", "\"speed_x100\": 5000")),
        );

        match store.load_config().expect_err("out of range") {
            SpeechStoreError::InvalidConfig(error) => {
                assert_eq!(error.field, Some("speed"));
                assert_eq!(error.reason, ProfileErrorReason::OutOfRange);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn stored_audio_settings_must_match_the_v1_specification() {
        let (_home, store) = store();
        seed(
            &store,
            &config_json(&valid_profile_body().replace("\"sample_rate\": 32000", "\"sample_rate\": 44100")),
        );

        match store.load_config().expect_err("unsupported audio") {
            SpeechStoreError::InvalidConfig(error) => {
                assert_eq!(error.field, Some("audio"));
                assert_eq!(error.reason, ProfileErrorReason::UnsupportedAudioSetting);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn an_unusable_root_is_reported_as_storage_unavailable() {
        let home = tempfile::tempdir().expect("temp home");
        let store = SpeechStore::from_home(home.path());
        std::fs::create_dir_all(store.root().parent().expect("parent")).expect("parent dirs");
        std::fs::write(store.root(), "not a directory").expect("occupy root with a file");

        let error = store
            .save_config(&SpeechConfig::default())
            .expect_err("root is a file");

        match error {
            SpeechStoreError::Unavailable { path, .. } => assert_eq!(path, store.root()),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_root_is_reported_as_storage_unavailable() {
        use std::os::unix::fs::PermissionsExt;

        let (_home, store) = store();
        std::fs::create_dir_all(store.root()).expect("speech root");
        std::fs::set_permissions(store.root(), std::fs::Permissions::from_mode(0o500))
            .expect("make root read-only");

        let result = store.save_config(&SpeechConfig::default());
        std::fs::set_permissions(store.root(), std::fs::Permissions::from_mode(0o700))
            .expect("restore root permissions");

        assert!(
            matches!(result, Err(SpeechStoreError::Unavailable { .. })),
            "read-only root must fail loudly: {result:?}"
        );
        assert!(!store.config_path().exists());
    }

    #[test]
    fn the_config_document_is_an_allowlisted_set_of_non_secret_keys() {
        let (_home, store) = store();
        store
            .save_config(&SpeechConfig::default())
            .expect("save config");

        let raw = std::fs::read_to_string(store.config_path()).expect("read config");
        let value: serde_json::Value = serde_json::from_str(&raw).expect("config JSON");
        let object = value.as_object().expect("config object");

        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["api_key_env", "schema_version", "voice_profile"]);
        assert_eq!(
            object["api_key_env"],
            serde_json::Value::String(DEFAULT_API_KEY_ENV.to_string())
        );
    }

    #[test]
    fn saved_configuration_never_contains_a_primary_key_or_token() {
        let (_home, store) = store();
        let profile = VoiceProfile::default();
        store
            .save_config(&SpeechConfig {
                api_key_env: "SENSEAUDIO_API_KEY".to_string(),
                profile,
            })
            .expect("save config");

        // 磁盘上只有环境变量名，没有任何密钥字面量的痕迹。
        let raw = std::fs::read_to_string(store.config_path()).expect("read config");
        assert!(!raw.contains("Bearer"));
        assert!(!raw.contains("sk-"));
        assert!(!raw.to_lowercase().contains("\"api_key\""));
        assert!(!raw.to_lowercase().contains("token"));
    }

    fn catalog(provider: &str, voice_id: &str) -> VoiceCatalog {
        VoiceCatalog {
            provider: provider.to_string(),
            fetched_at: Utc::now(),
            voices: vec![CatalogVoice {
                source_type: CatalogSourceType::System,
                voice_id: voice_id.to_string(),
                voice_name: "Cached Voice".to_string(),
                emotion_label: None,
                style_label: None,
                description: vec!["缓存标签".to_string()],
                created_time: None,
            }],
        }
    }

    fn seed_catalog(store: &SpeechStore, provider: &str, body: &str) {
        let path = store.voice_catalog_path(provider);
        std::fs::create_dir_all(path.parent().expect("catalog directory"))
            .expect("catalog dir");
        std::fs::write(path, body).expect("seed catalog document");
    }

    #[test]
    fn a_missing_catalog_cache_reads_as_absent_instead_of_an_error() {
        let (_home, store) = store();

        assert_eq!(store.load_voice_catalog("senseaudio").expect("load"), None);
    }

    #[test]
    fn saving_a_catalog_is_atomic_and_round_trips_the_exact_voice_id() {
        let (_home, store) = store();
        store
            .save_voice_catalog(&catalog("senseaudio", "exact-id"))
            .expect("save catalog");
        let temporary = store
            .voice_catalog_path("senseaudio")
            .with_extension("json.tmp");

        assert!(!temporary.exists(), "rename 之后不应残留临时文件");
        assert_eq!(
            store
                .load_voice_catalog("senseaudio")
                .expect("load")
                .expect("cache")
                .voices[0]
                .voice_id,
            "exact-id"
        );
    }

    #[test]
    fn a_corrupt_catalog_document_is_rejected_instead_of_guessed() {
        let (_home, store) = store();
        seed_catalog(&store, "senseaudio", "{ not a catalog");

        assert!(matches!(
            store.load_voice_catalog("senseaudio"),
            Err(SpeechStoreError::Unavailable { .. })
        ));
    }

    #[test]
    fn an_unknown_catalog_schema_version_is_rejected_instead_of_guessed() {
        let (_home, store) = store();
        seed_catalog(&store, "senseaudio", r#"{"schema_version": 99}"#);

        assert!(matches!(
            store.load_voice_catalog("senseaudio"),
            Err(SpeechStoreError::UnsupportedSchemaVersion(99))
        ));
    }

    #[test]
    fn a_catalog_document_owned_by_another_provider_is_rejected() {
        let (_home, store) = store();
        seed_catalog(
            &store,
            "senseaudio",
            r#"{
  "schema_version": 1,
  "provider": "other-provider",
  "fetched_at": "2026-09-11T00:00:00Z",
  "voices": []
}"#,
        );

        assert!(matches!(
            store.load_voice_catalog("senseaudio"),
            Err(SpeechStoreError::Unavailable { .. })
        ));
    }

    #[test]
    fn an_unsafe_catalog_provider_name_can_never_escape_the_speech_root() {
        let (_home, store) = store();

        assert!(matches!(
            store.load_voice_catalog("../escape"),
            Err(SpeechStoreError::Unavailable { .. })
        ));
        assert!(matches!(
            store.save_voice_catalog(&catalog("../escape", "exact-id")),
            Err(SpeechStoreError::Unavailable { .. })
        ));
        assert!(!store.root().join("escape.json").exists());
    }
}
