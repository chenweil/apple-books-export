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

/// `config.json` 的 schema 版本。
pub const SPEECH_CONFIG_SCHEMA_VERSION: u32 = 1;
/// Voice Catalog 缓存的 schema 版本。
pub const VOICE_CATALOG_SCHEMA_VERSION: u32 = 1;
/// 配置文件文件名。
const CONFIG_FILE_NAME: &str = "config.json";
/// Voice Catalog 缓存目录名。
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

    /// 某个 provider 的 Voice Catalog 缓存路径。
    ///
    /// provider 名是应用合同的一部分，不是任意文件系统路径。不安全字符会收成占位符，
    /// 畸形调用方不能逃出 Speech 根目录。
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

    /// 读取某个 provider 的 Voice Catalog 缓存；不创建 Speech 根目录。
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

    /// 原子写入非秘密的 provider Voice Catalog 缓存。
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

/// `state.json` 的 schema 版本。
pub const CLIP_STATE_SCHEMA_VERSION: u32 = 1;
/// 不可变 version `metadata.json` 的 schema 版本。
pub const CLIP_VERSION_SCHEMA_VERSION: u32 = 1;
/// attempt 记录的 schema 版本。
pub const ATTEMPT_SCHEMA_VERSION: u32 = 1;
/// clip 目录名。
const CLIPS_DIR_NAME: &str = "clips";
/// attempt 目录名。
const ATTEMPTS_DIR_NAME: &str = "attempts";
/// 跨进程锁目录名。
const LOCKS_DIR_NAME: &str = "locks";
/// 同文件系统临时目录名；保证 rename 原子性。
const TMP_DIR_NAME: &str = "tmp";
/// clip 内不可变音频版本目录名。
const VERSIONS_DIR_NAME: &str = "versions";
/// 当前指针 / unknown gate 文件名。
const STATE_FILE_NAME: &str = "state.json";
/// 不可变版本音频文件名。
const AUDIO_FILE_NAME: &str = "audio.mp3";
/// 不可变版本元数据文件名。
const VERSION_METADATA_FILE_NAME: &str = "metadata.json";

/// clip 当前缓存状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurrentCacheStatus {
    /// 有已验证、可播放/导出的当前音频。
    Ready,
    /// 没有当前音频。
    Absent,
    /// 当前音频校验不一致。
    Corrupt,
}

impl CurrentCacheStatus {
    /// 稳定的机器可读取值。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Absent => "absent",
            Self::Corrupt => "corrupt",
        }
    }
}

/// Speech Attempt 的终态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    /// 成功并接受了产物。
    Succeeded,
    /// 供应商明确失败。
    Failed,
    /// 请求可能已处理但结果不确定。
    Unknown,
    /// 发送前取消。
    CancelledBeforeSend,
    /// 供应商成功但本地无法形成有效音频。
    ProviderSucceededArtifactMissing,
}

impl AttemptStatus {
    /// 稳定的机器可读取值。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
            Self::CancelledBeforeSend => "cancelled_before_send",
            Self::ProviderSucceededArtifactMissing => "provider_succeeded_artifact_missing",
        }
    }

    /// 该终态是否阻塞普通 `generate`（无现有 cache 的 unknown / artifact-missing）。
    pub const fn blocks_generation(self) -> bool {
        matches!(self, Self::Unknown | Self::ProviderSucceededArtifactMissing)
    }
}

/// clip 的原子 current state / unknown gate。只保存身份与摘要，不保存原文、密钥或音频。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipState {
    /// schema 版本。
    pub schema_version: u32,
    /// 完整 clip ID。
    pub clip_id: String,
    /// 书籍稳定 ID。
    pub asset_id: String,
    /// Annotation 稳定 ID。
    pub annotation_id: String,
    /// 内容种类：`highlight` / `note`。
    pub content_kind: String,
    /// 规范化文本 SHA-256。
    pub text_sha256: String,
    /// 当前缓存状态。
    pub current_cache_status: CurrentCacheStatus,
    /// 当前音频 version 的 SHA-256；无音频时为 `null`。
    #[serde(default)]
    pub current_audio_sha256: Option<String>,
    /// 最近一次 attempt ID。
    #[serde(default)]
    pub latest_attempt_id: Option<String>,
    /// 最近一次 attempt 终态。
    #[serde(default)]
    pub latest_attempt_status: Option<AttemptStatus>,
    /// 最近一次产品错误码。
    #[serde(default)]
    pub latest_error_code: Option<String>,
    /// 无现有 cache 的 unknown / artifact-missing 时为 true。
    pub generation_blocked: bool,
    /// 最近更新时间（RFC 3339）。
    pub updated_at: String,
}

/// 不可变音频 version 的元数据。一旦可见就不可原地修改；不含原文、密钥或音频 hex。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipVersionMetadata {
    /// schema 版本。
    pub schema_version: u32,
    /// 完整 clip ID。
    pub clip_id: String,
    /// 生成该 version 的 attempt ID。
    pub attempt_id: String,
    /// 音频字节 SHA-256。
    pub audio_sha256: String,
    /// 音频格式。
    pub format: String,
    /// 采样率。
    pub sample_rate: u32,
    /// 码率。
    pub bitrate: u32,
    /// 声道数。
    pub channel: u32,
    /// 音频字节大小。
    pub size_bytes: u64,
    /// 估算时长（毫秒）。
    pub duration_ms: u64,
    /// 规范化文本 SHA-256（不是原文）。
    pub text_sha256: String,
    /// 解析后的音色 ID。
    pub voice_id: String,
    /// 创建时间（RFC 3339）。
    pub created_at: String,
}

/// 一次真实供应商请求的 attempt 元数据。不保存原文、请求体、响应体、密钥或音频。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptRecord {
    /// schema 版本。
    pub schema_version: u32,
    /// 独立 opaque attempt ID。
    pub attempt_id: String,
    /// 完整 clip ID。
    pub clip_id: String,
    /// Speech Provider。
    pub provider: String,
    /// 供应商模型。
    pub model: String,
    /// 解析后的音色 ID。
    pub voice_id: String,
    /// 开始时间（RFC 3339）。
    pub started_at: String,
    /// 结束时间（RFC 3339）。
    #[serde(default)]
    pub finished_at: Option<String>,
    /// 终态。
    pub status: AttemptStatus,
    /// Unicode 字符数。
    pub unicode_characters: u64,
    /// 估算计费字符数。
    pub estimated_billing_characters: u64,
    /// 供应商返回的实际用量字符数。
    #[serde(default)]
    pub provider_usage_characters: Option<u64>,
    /// 产品错误码。
    #[serde(default)]
    pub product_error_code: Option<String>,
    /// 供应商原始错误码（进入 attempt 历史，不进入稳定协议 message）。
    #[serde(default)]
    pub provider_code: Option<String>,
    /// 供应商 trace ID。
    #[serde(default)]
    pub trace_id: Option<String>,
}

impl SpeechStore {
    /// 同文件系统临时目录；所有临时文件都在这里，保证 rename 原子性。
    pub fn tmp_dir(&self) -> PathBuf {
        self.root.join(TMP_DIR_NAME)
    }

    /// clip 目录。
    pub fn clip_dir(&self, clip_id: &str) -> PathBuf {
        self.root.join(CLIPS_DIR_NAME).join(clip_id)
    }

    /// clip 的 current state 路径。
    pub fn clip_state_path(&self, clip_id: &str) -> PathBuf {
        self.clip_dir(clip_id).join(STATE_FILE_NAME)
    }

    /// clip 的不可变版本目录。
    pub fn clip_version_dir(&self, clip_id: &str, audio_sha256: &str) -> PathBuf {
        self.clip_dir(clip_id)
            .join(VERSIONS_DIR_NAME)
            .join(audio_sha256)
    }

    /// clip 的不可变版本音频路径。
    pub fn clip_version_audio_path(&self, clip_id: &str, audio_sha256: &str) -> PathBuf {
        self.clip_version_dir(clip_id, audio_sha256)
            .join(AUDIO_FILE_NAME)
    }

    /// 跨进程 clip 锁路径。clip ID 是 64 位小写 hex，可安全用作文件名。
    pub fn clip_lock_path(&self, clip_id: &str) -> PathBuf {
        self.root.join(LOCKS_DIR_NAME).join(format!("{clip_id}.lock"))
    }

    fn attempts_dir(&self) -> PathBuf {
        self.root.join(ATTEMPTS_DIR_NAME)
    }

    /// 某天（`YYYY-MM-DD`）的 attempt 目录。
    pub fn attempts_day_dir(&self, day: &str) -> PathBuf {
        self.attempts_dir().join(day)
    }

    /// 读取 clip 的 current state。不存在或不可解析时返回 `None`（当作没有可信缓存）。
    pub fn load_clip_state(&self, clip_id: &str) -> Option<ClipState> {
        let path = self.clip_state_path(clip_id);
        let text = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// 原子写入 clip current state：同文件系统临时文件 + fsync + rename。
    pub fn save_clip_state(&self, state: &ClipState) -> Result<(), SpeechStoreError> {
        let clip_dir = self.clip_dir(&state.clip_id);
        fs::create_dir_all(&clip_dir)
            .map_err(|error| SpeechStoreError::unavailable(&clip_dir, error))?;
        fs::create_dir_all(self.tmp_dir())
            .map_err(|error| SpeechStoreError::unavailable(&self.tmp_dir(), error))?;
        let path = self.clip_state_path(&state.clip_id);
        let mut json = serde_json::to_string_pretty(state).map_err(|error| {
            SpeechStoreError::unavailable(&clip_dir, std::io::Error::other(error.to_string()))
        })?;
        json.push('\n');
        let tmp = self.unique_tmp_path(&format!("{}-state.json", state.clip_id));
        write_synced(&tmp, json.as_bytes())
            .map_err(|error| SpeechStoreError::unavailable(&tmp, error))?;
        fs::rename(&tmp, &path).map_err(|error| {
            let _ = fs::remove_file(&tmp);
            SpeechStoreError::unavailable(&path, error)
        })?;
        Ok(())
    }

    /// 读取一个不可变 version 的元数据；不存在或不可解析时返回 `None`。
    pub fn load_clip_version_metadata(
        &self,
        clip_id: &str,
        audio_sha256: &str,
    ) -> Option<ClipVersionMetadata> {
        let path = self
            .clip_version_dir(clip_id, audio_sha256)
            .join(VERSION_METADATA_FILE_NAME);
        let text = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// 若 version 目录含非空音频与一致 metadata，返回可验证的音频路径；否则 `None`。
    pub fn valid_cache_audio_path(&self, clip_id: &str, audio_sha256: &str) -> Option<PathBuf> {
        let metadata = self.load_clip_version_metadata(clip_id, audio_sha256)?;
        if metadata.audio_sha256 != audio_sha256 || metadata.clip_id != clip_id {
            return None;
        }
        let audio_path = self.clip_version_audio_path(clip_id, audio_sha256);
        let is_non_empty = fs::metadata(&audio_path)
            .map(|meta| meta.is_file() && meta.len() > 0)
            .unwrap_or(false);
        is_non_empty.then_some(audio_path)
    }

    /// 提交一个不可变音频 version：先在 tmp 写音频与 metadata，再原子放置 version 目录。
    ///
    /// 内容寻址：`audio_sha256` 相同则幂等返回既有 metadata，绝不原地改写已可见的 version。
    /// 该方法只放置 version，不切换 current pointer；调用方成功后自行 [`Self::save_clip_state`]。
    pub fn commit_audio_version(
        &self,
        clip_id: &str,
        attempt_id: &str,
        text_sha256: &str,
        voice_id: &str,
        audio_bytes: &[u8],
        sample_rate: u32,
        bitrate: u32,
        channel: u32,
        duration_ms: u64,
        created_at: &str,
    ) -> Result<ClipVersionMetadata, SpeechStoreError> {
        let audio_sha256 = crate::speech::clip::hex_sha256(audio_bytes);
        let version_dir = self.clip_version_dir(clip_id, &audio_sha256);
        // 幂等：相同内容已经落成不可变 version。
        if let Some(existing) = self.load_clip_version_metadata(clip_id, &audio_sha256) {
            return Ok(existing);
        }

        let metadata = ClipVersionMetadata {
            schema_version: CLIP_VERSION_SCHEMA_VERSION,
            clip_id: clip_id.to_string(),
            attempt_id: attempt_id.to_string(),
            audio_sha256: audio_sha256.clone(),
            format: crate::speech::profile::AUDIO_FORMAT.to_string(),
            sample_rate,
            bitrate,
            channel,
            size_bytes: audio_bytes.len() as u64,
            duration_ms,
            text_sha256: text_sha256.to_string(),
            voice_id: voice_id.to_string(),
            created_at: created_at.to_string(),
        };

        fs::create_dir_all(self.tmp_dir())
            .map_err(|error| SpeechStoreError::unavailable(&self.tmp_dir(), error))?;
        let staging = self.tmp_dir().join(format!("{attempt_id}-{audio_sha256}"));
        if staging.exists() {
            fs::remove_dir_all(&staging)
                .map_err(|error| SpeechStoreError::unavailable(&staging, error))?;
        }
        fs::create_dir_all(&staging)
            .map_err(|error| SpeechStoreError::unavailable(&staging, error))?;

        let metadata_json = {
            let mut json = serde_json::to_string_pretty(&metadata).map_err(|error| {
                SpeechStoreError::unavailable(&staging, std::io::Error::other(error.to_string()))
            })?;
            json.push('\n');
            json
        };
        write_synced(&staging.join(VERSION_METADATA_FILE_NAME), metadata_json.as_bytes())
            .map_err(|error| SpeechStoreError::unavailable(&staging, error))?;
        write_synced(&staging.join(AUDIO_FILE_NAME), audio_bytes)
            .map_err(|error| SpeechStoreError::unavailable(&staging, error))?;

        fs::create_dir_all(version_dir.parent().expect("versions dir"))
            .map_err(|error| SpeechStoreError::unavailable(&staging, error))?;
        // 原子放置：同文件系统 rename。并发下 version 可能已被放置，此时清理 staging。
        match fs::rename(&staging, &version_dir) {
            Ok(()) => {}
            Err(_) if version_dir.exists() => {
                let _ = fs::remove_dir_all(&staging);
            }
            Err(error) => {
                let _ = fs::remove_dir_all(&staging);
                return Err(SpeechStoreError::unavailable(&version_dir, error));
            }
        }
        Ok(metadata)
    }

    /// 原子写入一条 attempt 记录到 `attempts/<day>/<attempt_id>.json`。
    pub fn save_attempt(&self, record: &AttemptRecord) -> Result<(), SpeechStoreError> {
        let day = record
            .started_at
            .get(..10)
            .unwrap_or("1970-01-01")
            .to_string();
        let dir = self.attempts_day_dir(&day);
        fs::create_dir_all(&dir)
            .map_err(|error| SpeechStoreError::unavailable(&dir, error))?;
        fs::create_dir_all(self.tmp_dir())
            .map_err(|error| SpeechStoreError::unavailable(&self.tmp_dir(), error))?;
        let path = dir.join(format!("{}.json", record.attempt_id));
        let mut json = serde_json::to_string_pretty(record).map_err(|error| {
            SpeechStoreError::unavailable(&dir, std::io::Error::other(error.to_string()))
        })?;
        json.push('\n');
        let tmp = self.unique_tmp_path(&format!("attempt-{}.json", record.attempt_id));
        write_synced(&tmp, json.as_bytes())
            .map_err(|error| SpeechStoreError::unavailable(&tmp, error))?;
        fs::rename(&tmp, &path).map_err(|error| {
            let _ = fs::remove_file(&tmp);
            SpeechStoreError::unavailable(&path, error)
        })?;
        Ok(())
    }

    /// 在 tmp 目录生成一个唯一临时路径（带纳秒时间戳与计数器），避免并发踩踏。
    fn unique_tmp_path(&self, label: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        self.tmp_dir()
            .join(format!("{label}.{nanos}.{unique}.tmp"))
    }

    /// 尝试获取跨进程 clip 锁。
    ///
    /// 返回 `Ok(Some(guard))` 表示拿到锁；`Ok(None)` 表示在 `timeout` 内一直被别人持有
    /// （映射到 `SPEECH_IN_PROGRESS`）。锁用 `create_new`（O_EXCL）实现，guard drop 时释放。
    pub fn try_acquire_clip_lock(
        &self,
        clip_id: &str,
        timeout: std::time::Duration,
    ) -> Result<Option<ClipLock>, SpeechStoreError> {
        let locks_dir = self.root.join(LOCKS_DIR_NAME);
        fs::create_dir_all(&locks_dir)
            .map_err(|error| SpeechStoreError::unavailable(&locks_dir, error))?;
        let path = self.clip_lock_path(clip_id);
        let deadline = std::time::Instant::now() + timeout;
        loop {
            match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(Some(ClipLock { path })),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if std::time::Instant::now() >= deadline {
                        return Ok(None);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(error) => return Err(SpeechStoreError::unavailable(&path, error)),
            }
        }
    }
}

/// 跨进程 clip 锁 guard。drop 时删除锁文件；单进程内也可用于串行化同一 clip 的生成。
#[derive(Debug)]
pub struct ClipLock {
    path: PathBuf,
}

impl Drop for ClipLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
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

    fn sample_audio() -> Vec<u8> {
        // 一个合法的 MPEG1 Layer III 128kbps 32kHz 立体声帧（576 字节）：FF FB 98 00.
        let mut frame = vec![0xFF, 0xFB, 0x98, 0x00];
        frame.resize(576, 0);
        frame
    }

    fn ready_state(clip_id: &str, audio_sha256: &str) -> ClipState {
        ClipState {
            schema_version: CLIP_STATE_SCHEMA_VERSION,
            clip_id: clip_id.to_string(),
            asset_id: "book-1".to_string(),
            annotation_id: "annotation-41".to_string(),
            content_kind: "highlight".to_string(),
            text_sha256: "deadbeef".to_string(),
            current_cache_status: CurrentCacheStatus::Ready,
            current_audio_sha256: Some(audio_sha256.to_string()),
            latest_attempt_id: Some("attempt-1".to_string()),
            latest_attempt_status: Some(AttemptStatus::Succeeded),
            latest_error_code: None,
            generation_blocked: false,
            updated_at: "2026-09-11T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn clip_state_round_trips_and_missing_state_loads_as_none() {
        let (_home, store) = store();
        assert!(store.load_clip_state("clip-x").is_none());

        let audio = sample_audio();
        let sha = crate::speech::clip::hex_sha256(&audio);
        store.save_clip_state(&ready_state("clip-x", &sha)).expect("save state");
        let loaded = store.load_clip_state("clip-x").expect("load state");
        assert_eq!(loaded.current_cache_status, CurrentCacheStatus::Ready);
        assert_eq!(loaded.current_audio_sha256.as_deref(), Some(sha.as_str()));
        // state.json 不得保存原文或密钥（这里只存摘要）。
        let text = std::fs::read_to_string(store.clip_state_path("clip-x")).expect("read state");
        assert!(text.contains("deadbeef"));
        assert!(!text.contains("Bearer"));
    }

    #[test]
    fn committing_an_audio_version_is_idempotent_and_content_addressed() {
        let (_home, store) = store();
        let audio = sample_audio();
        let meta = store
            .commit_audio_version("clip-x", "attempt-1", "deadbeef", "male_0004_a", &audio, 32_000, 128_000, 2, 36, "2026-09-11T00:00:00Z")
            .expect("commit");
        assert_eq!(meta.audio_sha256, crate::speech::clip::hex_sha256(&audio));
        assert_eq!(meta.size_bytes, audio.len() as u64);

        // 相同内容再次提交返回既有 metadata，且不改变音频字节。
        let again = store
            .commit_audio_version("clip-x", "attempt-2", "deadbeef", "male_0004_a", &audio, 32_000, 128_000, 2, 36, "2026-09-11T01:00:00Z")
            .expect("idempotent commit");
        assert_eq!(again, meta);
        assert_eq!(
            std::fs::read(store.clip_version_audio_path("clip-x", &meta.audio_sha256)).expect("audio"),
            audio
        );

        // version metadata 不含音频 hex 或密钥。
        let meta_text = std::fs::read_to_string(
            store.clip_version_dir("clip-x", &meta.audio_sha256).join("metadata.json"),
        )
        .expect("read metadata");
        assert!(!meta_text.contains("Bearer"));
        assert!(!meta_text.contains("fffb"));
    }

    #[test]
    fn valid_cache_audio_path_requires_matching_metadata_and_non_empty_audio() {
        let (_home, store) = store();
        let audio = sample_audio();
        let sha = crate::speech::clip::hex_sha256(&audio);
        store
            .commit_audio_version("clip-x", "attempt-1", "deadbeef", "male_0004_a", &audio, 32_000, 128_000, 2, 36, "2026-09-11T00:00:00Z")
            .expect("commit");

        assert_eq!(
            store.valid_cache_audio_path("clip-x", &sha),
            Some(store.clip_version_audio_path("clip-x", &sha))
        );
        // 未提交的 sha 没有有效缓存。
        assert!(store.valid_cache_audio_path("clip-x", &"0".repeat(64)).is_none());

        // 破坏音频字节后不再算有效缓存。
        std::fs::write(store.clip_version_audio_path("clip-x", &sha), b"").expect("truncate audio");
        assert!(store.valid_cache_audio_path("clip-x", &sha).is_none());
    }

    #[test]
    fn switching_the_current_pointer_is_atomic_and_replaceable() {
        let (_home, store) = store();
        let first = sample_audio();
        let second = {
            let mut frame = vec![0xFF, 0xFB, 0x90, 0x00];
            frame.resize(576, 0);
            frame[4] = 1; // 不同内容 → 不同 sha
            frame
        };
        let sha1 = crate::speech::clip::hex_sha256(&first);
        let sha2 = crate::speech::clip::hex_sha256(&second);
        for (attempt, audio) in [("attempt-1", &first), ("attempt-2", &second)] {
            store
                .commit_audio_version("clip-x", attempt, "deadbeef", "male_0004_a", audio, 32_000, 128_000, 2, 36, "2026-09-11T00:00:00Z")
                .expect("commit");
        }

        store.save_clip_state(&ready_state("clip-x", &sha1)).expect("state 1");
        assert_eq!(
            store.load_clip_state("clip-x").expect("load").current_audio_sha256.as_deref(),
            Some(sha1.as_str())
        );
        store.save_clip_state(&ready_state("clip-x", &sha2)).expect("state 2");
        let loaded = store.load_clip_state("clip-x").expect("load 2");
        assert_eq!(loaded.current_audio_sha256.as_deref(), Some(sha2.as_str()));
        // 旧 version 仍在磁盘上（新 pointer 提交后才可能被 GC）。
        assert!(store.clip_version_audio_path("clip-x", &sha1).exists());
        assert!(store.clip_version_audio_path("clip-x", &sha2).exists());
    }

    #[test]
    fn attempts_are_persisted_without_text_or_secrets() {
        let (_home, store) = store();
        let record = AttemptRecord {
            schema_version: ATTEMPT_SCHEMA_VERSION,
            attempt_id: "attempt-1".to_string(),
            clip_id: "clip-x".to_string(),
            provider: "senseaudio".to_string(),
            model: "sensenova-tts-2.0".to_string(),
            voice_id: "male_0004_a".to_string(),
            started_at: "2026-09-11T00:00:00Z".to_string(),
            finished_at: Some("2026-09-11T00:00:02Z".to_string()),
            status: AttemptStatus::Succeeded,
            unicode_characters: 4,
            estimated_billing_characters: 8,
            provider_usage_characters: Some(30),
            product_error_code: None,
            provider_code: None,
            trace_id: Some("trace-abc".to_string()),
        };
        store.save_attempt(&record).expect("save attempt");
        let path = store.attempts_day_dir("2026-09-11").join("attempt-1.json");
        assert!(path.exists());
        let text = std::fs::read_to_string(&path).expect("read attempt");
        assert!(text.contains("trace-abc"));
        assert!(!text.contains("Bearer"));
    }

    #[test]
    fn clip_lock_is_exclusive_and_released_on_drop() {
        let (_home, store) = store();
        let first = store
            .try_acquire_clip_lock("clip-x", std::time::Duration::from_millis(50))
            .expect("acquire")
            .expect("first lock");
        // 已被持有时，短超时内拿不到第二个锁。
        let second = store
            .try_acquire_clip_lock("clip-x", std::time::Duration::from_millis(50))
            .expect("second acquire");
        assert!(second.is_none(), "a held clip lock must not be re-acquired");
        drop(first);
        // 释放后可以再次获取。
        let third = store
            .try_acquire_clip_lock("clip-x", std::time::Duration::from_millis(50))
            .expect("third acquire");
        assert!(third.is_some());
    }
}
