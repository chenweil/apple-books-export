//! Speech Cache Entry、跨进程 clip 锁与 Speech Attempt History。
//!
//! 目录结构（实施 spec 7.1-7.3）：
//!
//! ```text
//! <speech root>/
//! ├── clips/<clip_id>/state.json              # 原子 current pointer / unknown gate
//! ├── clips/<clip_id>/versions/<audio_sha256>/{metadata.json,audio.mp3}
//! ├── attempts/YYYY-MM-DD/<attempt_id>.json
//! ├── locks/<clip_id>.lock
//! └── tmp/
//! ```
//!
//! version 目录一旦可见就不可原地修改：先在同一文件系统的临时目录写完音频和 metadata，
//! 再原子 rename 成 version 目录，最后原子切换 `state.json` 的 current pointer。
//! attempt history 只保存 metadata，不保存原文、密钥或音频。

use crate::speech::clip::SpeechContentKind;
use crate::speech::store::SpeechStore;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// clip state 的 schema 版本。
pub const CLIP_STATE_SCHEMA_VERSION: u32 = 1;
/// version metadata 的 schema 版本。
pub const CLIP_VERSION_SCHEMA_VERSION: u32 = 1;
/// attempt history 的 schema 版本。
pub const ATTEMPT_SCHEMA_VERSION: u32 = 1;
/// attempt history 的保留天数；不随音频 LRU 淘汰。
pub const ATTEMPT_HISTORY_RETENTION_DAYS: i64 = 90;
/// 等待同 clip 的跨进程 writer 锁的上限。
pub const CLIP_LOCK_TIMEOUT: Duration = Duration::from_secs(20);
/// 锁文件的轮询间隔。
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(25);
/// 超过这个年龄的锁文件被视为崩溃遗留，允许接管。
const LOCK_STALE_AFTER: Duration = Duration::from_secs(120);

/// clip 缓存读不到、写不了或状态不可信。
#[derive(Debug)]
pub enum ClipCacheError {
    /// 根目录或文件不可用。
    Unavailable {
        /// 出问题的路径。
        path: PathBuf,
        /// 底层错误说明，不含任何秘密。
        message: String,
    },
    /// `state.json`、version metadata 或音频与 checksum 不一致。
    Corrupt {
        /// 出问题的路径。
        path: PathBuf,
        /// 稳定的原因字符串。
        reason: &'static str,
    },
    /// 磁盘上的状态声明了本版本不支持的 schema 版本。
    UnsupportedSchemaVersion {
        /// 出问题的路径。
        path: PathBuf,
        /// 文档里的 schema 版本。
        version: u32,
    },
}

impl ClipCacheError {
    /// 稳定的 Machine JSON 错误码。
    pub const fn machine_code(&self) -> &'static str {
        match self {
            Self::Unavailable { .. } => "SPEECH_STORAGE_UNAVAILABLE",
            Self::Corrupt { .. } => "SPEECH_CACHE_CORRUPT",
            Self::UnsupportedSchemaVersion { .. } => "UNSUPPORTED_SCHEMA_VERSION",
        }
    }

    /// 面向人类的说明。
    pub fn message(&self) -> String {
        match self {
            Self::Unavailable { path, message } => {
                format!(
                    "the Speech cache is not usable at {}: {message}",
                    path.display()
                )
            }
            Self::Corrupt { path, reason } => {
                format!("the Speech cache entry at {} is {reason}", path.display())
            }
            Self::UnsupportedSchemaVersion { path, version } => format!(
                "the Speech cache state at {} uses unsupported schema version {version}",
                path.display()
            ),
        }
    }

    fn unavailable(path: &Path, error: std::io::Error) -> Self {
        Self::Unavailable {
            path: path.to_path_buf(),
            message: error.to_string(),
        }
    }
}

/// `state.json`：clip 的 current pointer 与 unknown gate。
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
    /// 内容部分。
    pub content_kind: SpeechContentKind,
    /// 规范化文本的 SHA-256。
    pub text_sha256: String,
    /// 当前缓存状态。
    pub current_cache_status: ClipCacheStatus,
    /// 当前被指向的不可变音频版本（audio SHA-256）。
    #[serde(default)]
    pub current_audio_sha256: Option<String>,
    /// 最近一次 Speech Attempt ID。
    #[serde(default)]
    pub latest_attempt_id: Option<String>,
    /// 最近一次 Speech Attempt 的状态。
    #[serde(default)]
    pub latest_attempt_status: Option<AttemptStatus>,
    /// 最近一次失败的产品错误码。
    #[serde(default)]
    pub latest_error_code: Option<String>,
    /// 没有有效 cache 的 unknown / provider-success-no-artifact 时为 true。
    #[serde(default)]
    pub generation_blocked: bool,
    /// 最近一次状态变更时间（RFC 3339）。
    pub updated_at: String,
}

/// clip 的缓存状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipCacheStatus {
    /// 有通过校验的当前音频。
    Ready,
    /// 没有可用缓存。
    Absent,
    /// 当前缓存损坏。
    Corrupt,
}

impl ClipCacheStatus {
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
    /// 请求发送前取消。
    CancelledBeforeSend,
    /// 请求记录已落盘，provider 调用还没有终态（实施 spec 9「建 attempt → 调用」）。
    InProgress,
    /// provider 明确失败。
    ProviderFailed,
    /// 成功并接受。
    Succeeded,
    /// 请求可能已处理但没有可接受结果。
    Unknown,
    /// provider 成功但本地没有可接受的音频产物。
    ProviderSucceededArtifactMissing,
}

impl AttemptStatus {
    /// 稳定的机器可读取值。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CancelledBeforeSend => "cancelled_before_send",
            Self::InProgress => "in_progress",
            Self::ProviderFailed => "provider_failed",
            Self::Succeeded => "succeeded",
            Self::Unknown => "unknown",
            Self::ProviderSucceededArtifactMissing => "provider_succeeded_artifact_missing",
        }
    }

    /// 该终态是否阻止普通 `generate` 自动重放。
    pub const fn blocks_generation(self) -> bool {
        matches!(self, Self::Unknown | Self::ProviderSucceededArtifactMissing)
    }
}

/// 不可变音频版本的 metadata；不保存原文、密钥或音频 hex。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipVersionMetadata {
    /// schema 版本。
    pub schema_version: u32,
    /// 完整 clip ID。
    pub clip_id: String,
    /// 音频字节的 SHA-256；同时是 version 目录名。
    pub audio_sha256: String,
    /// 书籍稳定 ID。
    pub asset_id: String,
    /// Annotation 稳定 ID。
    pub annotation_id: String,
    /// 内容部分。
    pub content_kind: SpeechContentKind,
    /// 规范化文本的 SHA-256。
    pub text_sha256: String,
    /// 本地 Unicode 字符数。
    pub unicode_characters: usize,
    /// 计费字符估算。
    pub estimated_billing_characters: usize,
    /// 计费估算规则版本。
    pub billing_estimator_version: String,
    /// Speech Provider。
    pub provider: String,
    /// 供应商模型。
    pub model: String,
    /// 已解析的音色 ID。
    pub voice_id: String,
    /// 语速的百分之一单位。
    pub speed_x100: i32,
    /// 音量的百分之一单位。
    pub volume_x100: i32,
    /// 声调。
    pub pitch: i32,
    /// 音频格式。
    pub format: String,
    /// 采样率（Hz）。
    pub sample_rate: u32,
    /// 码率（bps）。
    pub bitrate: u32,
    /// 声道数。
    pub channel: u32,
    /// 时长（毫秒）。
    pub duration_ms: u64,
    /// 音频字节数。
    pub size_bytes: u64,
    /// 生成该版本的 attempt ID。
    pub attempt_id: String,
    /// 供应商 trace ID。
    #[serde(default)]
    pub trace_id: Option<String>,
    /// 供应商返回的用量字符数。
    #[serde(default)]
    pub provider_usage_characters: Option<u64>,
    /// 创建时间（RFC 3339）。
    pub created_at: String,
}

/// attempt history 的一条记录；不保存原文、请求体、响应体、密钥或音频。
///
/// 记录先于 provider 调用落盘（`status = in_progress`、`finished_at = None`），
/// 调用返回后按同一 `attempt_id` 原地更新为终态：崩溃也不会丢掉一次可能已计费的
/// 请求历史（实施 spec 第 9 节「create attempt record → call SenseAudio」）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptRecord {
    /// schema 版本。
    pub schema_version: u32,
    /// 每次真实 provider 请求的独立 opaque ID。
    pub attempt_id: String,
    /// 完整 clip ID。
    pub clip_id: String,
    /// Speech Provider。
    pub provider: String,
    /// 供应商模型。
    pub model: String,
    /// 已解析的音色 ID。
    pub voice_id: String,
    /// 开始时间（RFC 3339）。
    pub started_at: String,
    /// 结束时间（RFC 3339）。
    #[serde(default)]
    pub finished_at: Option<String>,
    /// 终态。
    pub status: AttemptStatus,
    /// 本地 Unicode 字符数。
    pub unicode_characters: usize,
    /// 计费字符估算。
    pub estimated_billing_characters: usize,
    /// 供应商返回的用量字符数。
    #[serde(default)]
    pub provider_usage_characters: Option<u64>,
    /// 产品错误码。
    #[serde(default)]
    pub product_error_code: Option<String>,
    /// 供应商错误码。
    #[serde(default)]
    pub provider_code: Option<String>,
    /// 供应商 trace ID。
    #[serde(default)]
    pub trace_id: Option<String>,
}

/// 已经通过校验的 Speech Cache Entry。
#[derive(Debug, Clone)]
pub struct ReadyClip {
    /// clip 状态。
    pub state: ClipState,
    /// 不可变版本的 metadata。
    pub metadata: ClipVersionMetadata,
    /// 音频文件路径。
    pub audio_path: PathBuf,
}

/// `speech cache clear` 的报告：删除、跳过与清掉的阻塞门。
///
/// `cache clear` 是用户显式维护动作：删除可淘汰的 Cached Speech Clip，并清除没有有效
/// 音频的 clip 级阻塞状态（unknown gate / provider-success-no-artifact）。正在生成、
/// 播放、导出或持锁的 entry 跳过，不终止正在进行的操作（实施 spec 5.7）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheClearReport {
    /// 被删除的 clip ID（完整 sha256 hex）。
    pub removed: Vec<String>,
    /// 因持锁（正在生成/播放/导出）而跳过的 clip ID。
    pub skipped: Vec<String>,
    /// 被显式清除的 generation gate 数量。
    pub cleared_generation_gates: usize,
}

/// `speech history clear` 的报告。
///
/// `history clear` 只删除 attempt history 文件，不清缓存、不清 unknown gate、不碰用户
/// 导出（实施 spec 5.7），因此 `cleared_generation_gates` 恒为 0。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryClearReport {
    /// 被删除的 attempt history 记录数。
    pub removed_attempts: usize,
    /// 恒为 0：history clear 不得解除任何 generation gate。
    pub cleared_generation_gates: usize,
}

/// attempt history 自动清理的报告（实施 spec 5.7「正常维护自动删除超过 90 天」）。
///
/// 只删除过期的 attempt metadata：不删 clip、不清 generation gate、不动 current
/// pointer；只把指向已删除 attempt 的 `state.latest_attempt_id` 纠正为 `None`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptPruneReport {
    /// 被删除的 attempt history 记录数。
    pub removed_attempts: usize,
    /// 被纠正的 clip state 数量：`latest_attempt_id` 不再指向已删除的 attempt。
    pub reconciled_clip_states: usize,
}

/// Speech Cache Entry 的本地状态：不可变 version + 原子 current pointer。
#[derive(Debug, Clone)]
pub struct ClipCache {
    store: SpeechStore,
}

impl ClipCache {
    /// 基于用户级 Speech 状态根构造 clip 缓存。
    pub fn new(store: SpeechStore) -> Self {
        Self { store }
    }

    /// Speech 状态根。
    pub fn root(&self) -> &Path {
        self.store.root()
    }

    /// clip 目录；`clip_id` 必须是 64 位小写 hex，否则拒绝，避免路径逃逸。
    pub fn clip_dir(&self, clip_id: &str) -> Result<PathBuf, ClipCacheError> {
        if !is_clip_id(clip_id) {
            return Err(ClipCacheError::Corrupt {
                path: self.store.root().join("clips"),
                reason: "the clip id is not a sha256 hex digest",
            });
        }
        Ok(self.store.root().join("clips").join(clip_id))
    }

    fn state_path(&self, clip_id: &str) -> Result<PathBuf, ClipCacheError> {
        Ok(self.clip_dir(clip_id)?.join("state.json"))
    }

    /// 读取 clip 状态；不存在时返回 `None`。
    pub fn load_state(&self, clip_id: &str) -> Result<Option<ClipState>, ClipCacheError> {
        let path = self.state_path(clip_id)?;
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(ClipCacheError::unavailable(&path, error)),
        };
        #[derive(Deserialize)]
        struct SchemaProbe {
            schema_version: u32,
        }
        let probe: SchemaProbe =
            serde_json::from_str(&text).map_err(|_| ClipCacheError::Corrupt {
                path: path.clone(),
                reason: "state.json is not valid JSON",
            })?;
        if probe.schema_version != CLIP_STATE_SCHEMA_VERSION {
            return Err(ClipCacheError::UnsupportedSchemaVersion {
                path,
                version: probe.schema_version,
            });
        }
        let state: ClipState =
            serde_json::from_str(&text).map_err(|_| ClipCacheError::Corrupt {
                path: path.clone(),
                reason: "state.json is not a valid clip state",
            })?;
        if state.clip_id != clip_id {
            return Err(ClipCacheError::Corrupt {
                path,
                reason: "state.json belongs to a different clip",
            });
        }
        Ok(Some(state))
    }

    /// 原子写入 clip 状态：同目录临时文件 + fsync + rename。
    pub fn save_state(&self, state: &ClipState) -> Result<(), ClipCacheError> {
        let path = self.state_path(&state.clip_id)?;
        let directory = path.parent().expect("clip state parent").to_path_buf();
        fs::create_dir_all(&directory)
            .map_err(|error| ClipCacheError::unavailable(&directory, error))?;
        let mut json = serde_json::to_string_pretty(state).map_err(|error| {
            ClipCacheError::unavailable(&directory, std::io::Error::other(error.to_string()))
        })?;
        json.push('\n');
        write_atomic(&path, json.as_bytes())
    }

    /// 读取并校验当前 Speech Cache Entry。
    ///
    /// 音频字节与 metadata 不匹配、文件缺失或 metadata 与当前 pointer 不一致时返回
    /// [`ClipCacheError::Corrupt`]；没有缓存时返回 `Ok(None)`。
    pub fn load_ready_clip(&self, clip_id: &str) -> Result<Option<ReadyClip>, ClipCacheError> {
        let Some(state) = self.load_state(clip_id)? else {
            return Ok(None);
        };
        if state.current_cache_status != ClipCacheStatus::Ready {
            return Ok(None);
        }
        let Some(audio_sha256) = state.current_audio_sha256.clone() else {
            return Ok(None);
        };
        let version_dir = self.version_dir(clip_id, &audio_sha256)?;
        let metadata_path = version_dir.join("metadata.json");
        let audio_path = version_dir.join("audio.mp3");
        let metadata_text =
            fs::read_to_string(&metadata_path).map_err(|error| match error.kind() {
                std::io::ErrorKind::NotFound => ClipCacheError::Corrupt {
                    path: metadata_path.clone(),
                    reason: "the current audio version has no metadata",
                },
                _ => ClipCacheError::unavailable(&metadata_path, error),
            })?;
        let metadata: ClipVersionMetadata =
            serde_json::from_str(&metadata_text).map_err(|_| ClipCacheError::Corrupt {
                path: metadata_path,
                reason: "version metadata is not valid JSON",
            })?;
        let bytes = fs::read(&audio_path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => ClipCacheError::Corrupt {
                path: audio_path.clone(),
                reason: "the current audio file is missing",
            },
            _ => ClipCacheError::unavailable(&audio_path, error),
        })?;

        if metadata.clip_id != clip_id
            || metadata.audio_sha256 != audio_sha256
            || metadata.text_sha256 != state.text_sha256
            || bytes.is_empty()
            || crate::speech::text::sha256_hex(&bytes) != audio_sha256
            || bytes.len() as u64 != metadata.size_bytes
        {
            return Err(ClipCacheError::Corrupt {
                path: version_dir,
                reason: "the cached audio and its metadata disagree",
            });
        }
        // 音频格式本身也必须仍然可解析，否则播放/导出会拿到坏文件。
        crate::speech::audio::inspect_mp3(&bytes).map_err(|_| ClipCacheError::Corrupt {
            path: audio_path.clone(),
            reason: "the cached audio is not a parseable MP3",
        })?;

        Ok(Some(ReadyClip {
            state,
            metadata,
            audio_path,
        }))
    }

    /// 已存在 version 目录是否就是这次要提交的版本。    ///
    /// 目录名是内容寻址的 `audio_sha256`，因此「 intact 」意味着磁盘字节仍然哈希成名
    /// 字本身，且 `metadata.json` 与本次提交完全一致。只要有一个不成立（音频被原地
    /// 改写、metadata 被截断、size 不符），调用方就必须重写该目录，绝不能让
    /// `state.json` 继续指向字节与名字不符的版本。
    fn version_is_intact(
        &self,
        version_dir: &Path,
        metadata: &ClipVersionMetadata,
    ) -> Result<bool, ClipCacheError> {
        let audio_path = version_dir.join("audio.mp3");
        let Ok(bytes) = fs::read(&audio_path) else {
            return Ok(false);
        };
        if bytes.is_empty() || crate::speech::text::sha256_hex(&bytes) != metadata.audio_sha256 {
            return Ok(false);
        }
        if bytes.len() as u64 != metadata.size_bytes {
            return Ok(false);
        }
        let metadata_path = version_dir.join("metadata.json");
        let Ok(metadata_text) = fs::read_to_string(&metadata_path) else {
            return Ok(false);
        };
        let Ok(on_disk) = serde_json::from_str::<ClipVersionMetadata>(&metadata_text) else {
            return Ok(false);
        };
        Ok(on_disk == *metadata)
    }

    /// 提交一个不可变音频版本，并原子切换 current pointer。
    ///
    /// 顺序固定：临时目录写完音频与 metadata → rename 成不可变 version 目录 →
    /// 原子替换 `state.json`。任一步失败都不留下被 pointer 引用的半成品。
    pub fn commit_version(
        &self,
        state: &ClipState,
        metadata: &ClipVersionMetadata,
        audio_bytes: &[u8],
        now: DateTime<Utc>,
    ) -> Result<(), ClipCacheError> {
        if metadata.clip_id != state.clip_id
            || metadata.audio_sha256 != crate::speech::text::sha256_hex(audio_bytes)
        {
            return Err(ClipCacheError::Corrupt {
                path: self.clip_dir(&state.clip_id)?,
                reason: "the audio version metadata does not match the audio bytes",
            });
        }
        let version_dir = self.version_dir(&state.clip_id, &metadata.audio_sha256)?;
        if !self.version_is_intact(&version_dir, metadata)? {
            // 残留半成品、已被原地破坏或 metadata 不一致的 version 目录不能原地保留：
            // 先整体移除，否则 rename 到非空目录会失败，旧字节会继续被 pointer 引用。
            if version_dir.exists() {
                fs::remove_dir_all(&version_dir)
                    .map_err(|error| ClipCacheError::unavailable(&version_dir, error))?;
            }
            let tmp_dir = self.tmp_dir();
            fs::create_dir_all(&tmp_dir)
                .map_err(|error| ClipCacheError::unavailable(&tmp_dir, error))?;
            let staging = tmp_dir.join(format!("version-{}", metadata.audio_sha256));
            if staging.exists() {
                fs::remove_dir_all(&staging)
                    .map_err(|error| ClipCacheError::unavailable(&staging, error))?;
            }
            fs::create_dir_all(&staging)
                .map_err(|error| ClipCacheError::unavailable(&staging, error))?;
            write_staged(&staging.join("audio.mp3"), audio_bytes)?;
            let mut metadata_json = serde_json::to_string_pretty(metadata).map_err(|error| {
                ClipCacheError::unavailable(&staging, std::io::Error::other(error.to_string()))
            })?;
            metadata_json.push('\n');
            write_staged(&staging.join("metadata.json"), metadata_json.as_bytes())?;
            let versions_dir = version_dir.parent().expect("version parent").to_path_buf();
            fs::create_dir_all(&versions_dir)
                .map_err(|error| ClipCacheError::unavailable(&versions_dir, error))?;
            fs::rename(&staging, &version_dir).map_err(|error| {
                let _ = fs::remove_dir_all(&staging);
                ClipCacheError::unavailable(&version_dir, error)
            })?;
        }

        let mut next = state.clone();
        next.current_cache_status = ClipCacheStatus::Ready;
        next.current_audio_sha256 = Some(metadata.audio_sha256.clone());
        next.generation_blocked = false;
        next.updated_at = now.to_rfc3339();
        self.save_state(&next)
    }

    /// 只提交状态（unknown gate、attempt 记录），不触碰音频版本。
    pub fn save_state_only(&self, state: &ClipState) -> Result<(), ClipCacheError> {
        self.save_state(state)
    }

    /// 写入一条 attempt history 记录；只含 metadata。
    ///
    /// 归档目录取记录自己的 `started_at`：同一 attempt 的后续更新（终态、trace、
    /// 用量）因此落在同一个文件里，实现真正的就地更新，不会在跨天时留下重复记录。
    /// 写入后顺带执行正常维护：删除超过保留窗口的 attempt metadata（实施 spec 5.7）。
    pub fn record_attempt(
        &self,
        record: &AttemptRecord,
        now: DateTime<Utc>,
    ) -> Result<PathBuf, ClipCacheError> {
        let directory = self
            .attempts_dir()
            .join(attempt_day_directory(&record.started_at, now));
        fs::create_dir_all(&directory)
            .map_err(|error| ClipCacheError::unavailable(&directory, error))?;
        let path = directory.join(format!("{}.json", record.attempt_id));
        let mut json = serde_json::to_string_pretty(record).map_err(|error| {
            ClipCacheError::unavailable(&directory, std::io::Error::other(error.to_string()))
        })?;
        json.push('\n');
        write_atomic(&path, json.as_bytes())?;
        // 正常维护是尽力而为：清理失败不能让一次已经（可能）计费的 attempt 写入失败。
        let _ = self.prune_attempt_history(now, ATTEMPT_HISTORY_RETENTION_DAYS);
        Ok(path)
    }

    /// 读取一条 attempt history 记录；不存在时返回 `None`。
    pub fn load_attempt(&self, attempt_id: &str) -> Result<Option<AttemptRecord>, ClipCacheError> {
        for path in self.attempt_paths(attempt_id) {
            match fs::read_to_string(&path) {
                Ok(text) => {
                    let record: AttemptRecord =
                        serde_json::from_str(&text).map_err(|_| ClipCacheError::Corrupt {
                            path,
                            reason: "the attempt record is not valid JSON",
                        })?;
                    return Ok(Some(record));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(ClipCacheError::unavailable(&path, error)),
            }
        }
        Ok(None)
    }

    /// 同一 attempt ID 可能存在的所有路径（按天归档，通常只有一个）。
    fn attempt_paths(&self, attempt_id: &str) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if attempt_id.is_empty()
            || attempt_id.contains('/')
            || attempt_id.contains('.')
            || attempt_id.contains(std::path::MAIN_SEPARATOR)
        {
            return paths;
        }
        let Ok(days) = fs::read_dir(self.attempts_dir()) else {
            return paths;
        };
        for day in days.flatten() {
            if day.path().is_dir() {
                paths.push(day.path().join(format!("{attempt_id}.json")));
            }
        }
        paths
    }

    /// 自动维护：删除超过保留窗口的 attempt history 记录，并纠正 clip state 里指向
    /// 已删除 attempt 的 `latest_attempt_id`。
    ///
    /// 只动 attempt metadata：不删 clip、不清 generation gate、不动 current pointer。
    /// 没有有效 cache 的 Unknown Speech Result 作为 clip 级阻塞状态持续保留，不随
    /// 90 天 attempt history 到期（ADR 0007「请求失败与重试」「身份与本地状态」）。
    pub fn prune_attempt_history(
        &self,
        now: DateTime<Utc>,
        retention_days: i64,
    ) -> Result<AttemptPruneReport, ClipCacheError> {
        let Some(cutoff) = now.checked_sub_signed(chrono::Duration::days(retention_days)) else {
            return Ok(AttemptPruneReport::default());
        };
        let attempts_dir = self.attempts_dir();
        let days = match fs::read_dir(&attempts_dir) {
            Ok(days) => days,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(AttemptPruneReport::default())
            }
            Err(error) => return Err(ClipCacheError::unavailable(&attempts_dir, error)),
        };
        let mut removed: Vec<String> = Vec::new();
        for day in days {
            let day = day.map_err(|error| ClipCacheError::unavailable(&attempts_dir, error))?;
            if !day.path().is_dir() {
                continue;
            }
            let mut removed_in_day = 0usize;
            let files = fs::read_dir(day.path())
                .map_err(|error| ClipCacheError::unavailable(&day.path(), error))?;
            for file in files {
                let file =
                    file.map_err(|error| ClipCacheError::unavailable(&day.path(), error))?;
                if !file.path().is_file() {
                    continue;
                }
                let Some(attempt_id) = file
                    .path()
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                else {
                    continue;
                };
                // 读不出或解析不了的记录不猜测删除：宁可多留一条 metadata。
                let Ok(text) = fs::read_to_string(file.path()) else {
                    continue;
                };
                let stamp = serde_json::from_str::<AttemptRecord>(&text)
                    .ok()
                    .and_then(|record| attempt_timestamp(&record));
                if stamp.is_some_and(|stamp| stamp < cutoff) {
                    fs::remove_file(file.path())
                        .map_err(|error| ClipCacheError::unavailable(&file.path(), error))?;
                    removed.push(attempt_id);
                    removed_in_day += 1;
                }
            }
            if removed_in_day > 0 && fs::read_dir(day.path()).ok().is_some_and(|mut left| left.next().is_none()) {
                let _ = fs::remove_dir(day.path());
            }
        }
        let reconciled_clip_states = self.reconcile_latest_attempt_ids(&removed, now)?;
        Ok(AttemptPruneReport {
            removed_attempts: removed.len(),
            reconciled_clip_states,
        })
    }

    /// 让 clip state 不再指向已经不存在的 attempt 记录。
    ///
    /// 只清 `latest_attempt_id`：generation gate、current pointer 与缓存版本都不动，
    /// 阻塞态仍只能由 `--regenerate` 或 `speech cache clear` 解除（实施 spec 5.7 / 7.2）。
    fn reconcile_latest_attempt_ids(
        &self,
        removed: &[String],
        now: DateTime<Utc>,
    ) -> Result<usize, ClipCacheError> {
        if removed.is_empty() {
            return Ok(0);
        }
        let clips_dir = self.clips_dir();
        let entries = match fs::read_dir(&clips_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(ClipCacheError::unavailable(&clips_dir, error)),
        };
        let mut reconciled = 0usize;
        for entry in entries {
            let entry = entry.map_err(|error| ClipCacheError::unavailable(&clips_dir, error))?;
            if !entry.path().is_dir() {
                continue;
            }
            let clip_id = entry.file_name().to_string_lossy().into_owned();
            if !is_clip_id(&clip_id) {
                continue;
            }
            let Ok(Some(mut state)) = self.load_state(&clip_id) else {
                continue;
            };
            let Some(latest) = state.latest_attempt_id.clone() else {
                continue;
            };
            if !removed.iter().any(|attempt_id| attempt_id == &latest) {
                continue;
            }
            state.latest_attempt_id = None;
            state.updated_at = now.to_rfc3339();
            self.save_state(&state)?;
            reconciled += 1;
        }
        Ok(reconciled)
    }

    /// 跨进程 writer 锁目录。
    pub fn locks_dir(&self) -> PathBuf {
        self.store.root().join("locks")
    }

    /// clip 目录的父目录。
    pub fn clips_dir(&self) -> PathBuf {
        self.store.root().join("clips")
    }

    /// attempt history 目录。
    pub fn attempts_dir(&self) -> PathBuf {
        self.store.root().join("attempts")
    }

    /// `speech cache clear`：删除可淘汰 clip，显式清除无有效音频的阻塞门。
    ///
    /// 每个 clip 先用零超时尝试同一把 writer 锁：拿不到锁说明该 entry 正在生成、播放、
    /// 导出或持锁，跳过并计入 `skipped`；拿到锁才删除，因此并发生成不会被清掉。
    pub fn clear_cache(&self, now: DateTime<Utc>) -> Result<CacheClearReport, ClipCacheError> {
        let mut report = CacheClearReport::default();
        let clips_dir = self.clips_dir();
        let entries = match fs::read_dir(&clips_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(report),
            Err(error) => return Err(ClipCacheError::unavailable(&clips_dir, error)),
        };
        let mut clip_ids = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| ClipCacheError::unavailable(&clips_dir, error))?;
            if !entry.path().is_dir() {
                continue;
            }
            // 目录名不是 sha256 hex 的残留无法被 clip 身份寻址：不猜测、不删除。
            let clip_id = entry.file_name().to_string_lossy().into_owned();
            if is_clip_id(&clip_id) {
                clip_ids.push(clip_id);
            }
        }
        clip_ids.sort();

        for clip_id in clip_ids {
            match ClipLock::acquire_with_timeout(self, &clip_id, now, Duration::ZERO) {
                // 持锁 entry 不可淘汰：锁在 Drop 时释放，不留下孤儿锁文件。
                Ok(_lock) => {
                    let clip_dir = self.clip_dir(&clip_id)?;
                    let blocked = self
                        .load_state(&clip_id)
                        .ok()
                        .flatten()
                        .is_some_and(|state| state.generation_blocked);
                    fs::remove_dir_all(&clip_dir)
                        .map_err(|error| ClipCacheError::unavailable(&clip_dir, error))?;
                    if blocked {
                        report.cleared_generation_gates += 1;
                    }
                    report.removed.push(clip_id);
                }
                Err(ClipLockError::InProgress) => report.skipped.push(clip_id),
                Err(ClipLockError::Unavailable(path, error)) => {
                    return Err(ClipCacheError::unavailable(&path, error))
                }
            }
        }
        Ok(report)
    }

    /// `speech history clear`：只删除 attempt history，不清缓存或 unknown gate。
    ///
    /// 删除后纠正 `state.latest_attempt_id`：它不得再指向已经被清掉的 attempt
    /// （否则会留下悬空引用），但 generation gate 与缓存都不动。
    pub fn clear_history(&self, now: DateTime<Utc>) -> Result<HistoryClearReport, ClipCacheError> {
        let attempts_dir = self.attempts_dir();
        let mut removed_attempts = 0usize;
        let mut removed_ids: Vec<String> = Vec::new();
        let days = match fs::read_dir(&attempts_dir) {
            Ok(days) => days,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(HistoryClearReport::default())
            }
            Err(error) => return Err(ClipCacheError::unavailable(&attempts_dir, error)),
        };
        for day in days {
            let day = day.map_err(|error| ClipCacheError::unavailable(&attempts_dir, error))?;
            if !day.path().is_dir() {
                continue;
            }
            let files = fs::read_dir(day.path())
                .map_err(|error| ClipCacheError::unavailable(&day.path(), error))?;
            for file in files {
                let file = file.map_err(|error| ClipCacheError::unavailable(&day.path(), error))?;
                if file.path().is_file() {
                    removed_attempts += 1;
                    if let Some(stem) = file.path().file_stem() {
                        removed_ids.push(stem.to_string_lossy().into_owned());
                    }
                }
            }
        }
        fs::remove_dir_all(&attempts_dir)
            .map_err(|error| ClipCacheError::unavailable(&attempts_dir, error))?;
        self.reconcile_latest_attempt_ids(&removed_ids, now)?;
        Ok(HistoryClearReport {
            removed_attempts,
            // history clear 明确不清 unknown gate：阻塞态只由显式 --regenerate 或
            // speech cache clear 解除（实施 spec 5.7 / 7.2）。
            cleared_generation_gates: 0,
        })
    }

    fn lock_path(&self, clip_id: &str) -> Result<PathBuf, ClipCacheError> {
        Ok(self.locks_dir().join(format!("{clip_id}.lock")))
    }

    fn version_dir(&self, clip_id: &str, audio_sha256: &str) -> Result<PathBuf, ClipCacheError> {
        if !is_sha256_hex(audio_sha256) {
            return Err(ClipCacheError::Corrupt {
                path: self.clip_dir(clip_id)?,
                reason: "the audio version id is not a sha256 hex digest",
            });
        }
        Ok(self.clip_dir(clip_id)?.join("versions").join(audio_sha256))
    }

    fn tmp_dir(&self) -> PathBuf {
        // 与 versions 同一文件系统，保证 rename 原子。
        self.store.root().join("tmp")
    }
}

/// 同 clip 的跨进程 writer 锁；Drop 时释放。
#[derive(Debug)]
pub struct ClipLock {
    path: PathBuf,
    /// 是否排队等过另一个 writer：等待方必须在锁内重新检查首个终态，
    /// 不能对同一 clip 发起第二个 provider 请求（ADR 0007「并发、取消与接受证据」）。
    waited: bool,
}

impl ClipLock {
    /// 获取指定 clip 的 writer 锁。
    ///
    /// 第一个请求成为 writer；后来的调用等待首个 Speech Attempt 的终态。
    /// 超过 [`CLIP_LOCK_TIMEOUT`] 返回 [`ClipLockError::InProgress`]，
    /// 对应稳定的 `SPEECH_IN_PROGRESS`，绝不发起第二个 provider 请求。
    pub fn acquire(
        cache: &ClipCache,
        clip_id: &str,
        now: DateTime<Utc>,
    ) -> Result<Self, ClipLockError> {
        Self::acquire_with_timeout(cache, clip_id, now, CLIP_LOCK_TIMEOUT)
    }

    /// 与 [`ClipLock::acquire`] 相同，但等待上限可注入，供测试使用。
    pub fn acquire_with_timeout(
        cache: &ClipCache,
        clip_id: &str,
        now: DateTime<Utc>,
        timeout: Duration,
    ) -> Result<Self, ClipLockError> {
        let path = cache.lock_path(clip_id)?;
        fs::create_dir_all(cache.locks_dir())
            .map_err(|error| ClipLockError::Unavailable(cache.locks_dir(), error))?;
        let deadline = SystemTime::now() + timeout;
        let mut waited = false;
        loop {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    let _ = writeln!(file, "{}", now.to_rfc3339());
                    let _ = file.sync_all();
                    return Ok(Self { path, waited });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if is_stale_lock(&path) {
                        // 崩溃遗留的锁：接管而不是无限等待。
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                    // 确实有为同一个 clip 排过队：拿到锁后必须重新检查首个终态。
                    waited = true;
                    if SystemTime::now() >= deadline {
                        return Err(ClipLockError::InProgress);
                    }
                    std::thread::sleep(LOCK_POLL_INTERVAL);
                }
                Err(error) => return Err(ClipLockError::Unavailable(path.clone(), error)),
            }
        }
    }

    /// 这次获取是否排队等过同 clip 的另一个 writer。
    pub fn waited(&self) -> bool {
        self.waited
    }

    /// 锁文件路径；供诊断使用。
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ClipLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// 锁文件年龄超过阈值即视为崩溃遗留。
fn is_stale_lock(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .is_ok_and(|age| age > LOCK_STALE_AFTER)
}

/// 获取 writer 锁失败。
#[derive(Debug)]
pub enum ClipLockError {
    /// 锁目录不可用。
    Unavailable(PathBuf, std::io::Error),
    /// 同 clip 的 writer 还在进行中。
    InProgress,
}

impl ClipLockError {
    /// 稳定的 Machine JSON 错误码。
    pub const fn machine_code(&self) -> &'static str {
        match self {
            Self::Unavailable(..) => "SPEECH_STORAGE_UNAVAILABLE",
            Self::InProgress => "SPEECH_IN_PROGRESS",
        }
    }

    /// 面向人类的说明。
    pub fn message(&self) -> String {
        match self {
            Self::Unavailable(path, error) => format!(
                "the Speech lock directory is not usable at {}: {error}",
                path.display()
            ),
            Self::InProgress => {
                "another generation for this Speech Clip is still in progress".to_string()
            }
        }
    }
}

impl From<ClipCacheError> for ClipLockError {
    fn from(error: ClipCacheError) -> Self {
        match error {
            ClipCacheError::Unavailable { path, message } => {
                Self::Unavailable(path, std::io::Error::other(message))
            }
            other => Self::Unavailable(PathBuf::new(), std::io::Error::other(other.message())),
        }
    }
}

fn write_staged(path: &Path, bytes: &[u8]) -> Result<(), ClipCacheError> {
    let mut file =
        fs::File::create(path).map_err(|error| ClipCacheError::unavailable(path, error))?;
    file.write_all(bytes)
        .map_err(|error| ClipCacheError::unavailable(path, error))?;
    file.sync_all()
        .map_err(|error| ClipCacheError::unavailable(path, error))?;
    Ok(())
}

/// 同文件系统内的原子替换：临时文件 + fsync + rename。
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ClipCacheError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| ClipCacheError::unavailable(parent, error))?;
    let temporary = parent.join(format!(
        ".{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    write_staged(&temporary, bytes)?;
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        ClipCacheError::unavailable(path, error)
    })?;
    Ok(())
}

fn is_clip_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// attempt history 的按天归档目录名：优先用 attempt 自己的开始时间，
/// 解析不了才退回当前时间；两种情况下 `<attempt_id>.json` 都是稳定文件名。
fn attempt_day_directory(started_at: &str, now: DateTime<Utc>) -> String {
    DateTime::parse_from_rfc3339(started_at)
        .map(|started| started.with_timezone(&Utc).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|_| now.format("%Y-%m-%d").to_string())
}

/// attempt 的归档时间戳：有终态时间用 `finished_at`，否则用 `started_at`。
fn attempt_timestamp(record: &AttemptRecord) -> Option<DateTime<Utc>> {
    record
        .finished_at
        .as_deref()
        .or(Some(record.started_at.as_str()))
        .and_then(|stamp| DateTime::parse_from_rfc3339(stamp).ok())
        .map(|stamp| stamp.with_timezone(&Utc))
}

fn is_sha256_hex(value: &str) -> bool {
    is_clip_id(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speech::profile::{AudioSettings, VoiceProfile};
    use crate::speech::text::sha256_hex;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-11T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc)
    }

    fn cache() -> (tempfile::TempDir, ClipCache) {
        let home = tempfile::tempdir().expect("temp home");
        let cache = ClipCache::new(SpeechStore::from_home(home.path()));
        (home, cache)
    }

    fn state(clip_id: &str) -> ClipState {
        ClipState {
            schema_version: CLIP_STATE_SCHEMA_VERSION,
            clip_id: clip_id.to_string(),
            asset_id: "book-1".to_string(),
            annotation_id: "annotation-41".to_string(),
            content_kind: SpeechContentKind::Highlight,
            text_sha256: sha256_hex("高亮正文".as_bytes()),
            current_cache_status: ClipCacheStatus::Absent,
            current_audio_sha256: None,
            latest_attempt_id: None,
            latest_attempt_status: None,
            latest_error_code: None,
            generation_blocked: false,
            updated_at: now().to_rfc3339(),
        }
    }

    fn metadata(clip_id: &str, audio_sha256: &str, bytes: &[u8]) -> ClipVersionMetadata {
        let profile = VoiceProfile::default();
        ClipVersionMetadata {
            schema_version: CLIP_VERSION_SCHEMA_VERSION,
            clip_id: clip_id.to_string(),
            audio_sha256: audio_sha256.to_string(),
            asset_id: "book-1".to_string(),
            annotation_id: "annotation-41".to_string(),
            content_kind: SpeechContentKind::Highlight,
            text_sha256: sha256_hex("高亮正文".as_bytes()),
            unicode_characters: 4,
            estimated_billing_characters: 8,
            billing_estimator_version: "senseaudio-docs-2026-09-10".to_string(),
            provider: profile.provider.clone(),
            model: profile.model.clone(),
            voice_id: profile.voice_id.clone(),
            speed_x100: profile.speed.x100(),
            volume_x100: profile.volume.x100(),
            pitch: profile.pitch,
            format: AudioSettings::v1().format,
            sample_rate: AudioSettings::v1().sample_rate,
            bitrate: AudioSettings::v1().bitrate,
            channel: AudioSettings::v1().channel,
            duration_ms: 108,
            size_bytes: bytes.len() as u64,
            attempt_id: "attempt-1".to_string(),
            trace_id: Some("trace-1".to_string()),
            provider_usage_characters: Some(8),
            created_at: now().to_rfc3339(),
        }
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

    #[test]
    fn clip_ids_that_are_not_sha256_hex_cannot_escape_the_cache_root() {
        let (_home, cache) = cache();

        for invalid in ["", "../escape", "clip", &"A".repeat(64)] {
            assert!(
                cache.load_state(invalid).is_err(),
                "{invalid} must not address a cache path"
            );
        }
    }

    #[test]
    fn a_committed_version_is_immutable_and_selected_by_the_current_pointer() {
        let (_home, cache) = cache();
        let clip_id = "a".repeat(64);
        let audio = silent_mp3(1);
        let audio_sha256 = sha256_hex(&audio);
        let mut state = state(&clip_id);
        state.latest_attempt_id = Some("attempt-1".to_string());
        state.latest_attempt_status = Some(AttemptStatus::Succeeded);
        let metadata = metadata(&clip_id, &audio_sha256, &audio);

        cache
            .commit_version(&state, &metadata, &audio, now())
            .expect("commit");

        let ready = cache
            .load_ready_clip(&clip_id)
            .expect("load")
            .expect("ready clip");
        assert_eq!(
            ready.state.current_audio_sha256.as_deref(),
            Some(audio_sha256.as_str())
        );
        assert_eq!(ready.state.current_cache_status, ClipCacheStatus::Ready);
        assert_eq!(ready.metadata.trace_id.as_deref(), Some("trace-1"));
        assert_eq!(fs::read(ready.audio_path.clone()).expect("audio"), audio);

        // version 目录不可原地修改：改写音频字节后必须被识别为损坏。
        let mut tampered = audio.clone();
        tampered[10] ^= 0xFF;
        fs::write(&ready.audio_path, &tampered).expect("tamper");
        let error = cache.load_ready_clip(&clip_id).expect_err("tampered audio");
        assert_eq!(error.machine_code(), "SPEECH_CACHE_CORRUPT");
    }

    #[test]
    fn an_in_place_corrupted_version_is_repaired_instead_of_being_left_pointed_at() {
        let (_home, cache) = cache();
        let clip_id = "f".repeat(64);
        let audio = silent_mp3(1);
        let audio_sha256 = sha256_hex(&audio);
        let metadata = metadata(&clip_id, &audio_sha256, &audio);
        cache
            .commit_version(&state(&clip_id), &metadata, &audio, now())
            .expect("commit");

        // 原地破坏一个字节：目录名仍然声称自己是旧的 sha256。
        let version_dir = cache
            .clip_dir(&clip_id)
            .expect("clip dir")
            .join("versions")
            .join(&audio_sha256);
        let mut tampered = audio.clone();
        tampered[10] ^= 0xFF;
        fs::write(version_dir.join("audio.mp3"), &tampered).expect("tamper");
        assert!(cache.load_ready_clip(&clip_id).is_err(), "corrupt audio");

        // 修复：重新提交同样内容的版本，必须重写目录而不是保留被破坏的字节。
        cache
            .commit_version(&state(&clip_id), &metadata, &audio, now())
            .expect("repair");

        let ready = cache
            .load_ready_clip(&clip_id)
            .expect("load after repair")
            .expect("ready after repair");
        assert_eq!(fs::read(&ready.audio_path).expect("audio"), audio);
        assert_eq!(
            crate::speech::text::sha256_hex(&fs::read(&ready.audio_path).expect("audio")),
            audio_sha256,
            "the repaired bytes must hash to the version directory name"
        );
        assert!(!ready.state.generation_blocked);
    }

    #[test]
    fn a_version_with_broken_metadata_is_rewritten() {
        let (_home, cache) = cache();
        let clip_id = "0".repeat(64);
        let audio = silent_mp3(1);
        let audio_sha256 = sha256_hex(&audio);
        let metadata = metadata(&clip_id, &audio_sha256, &audio);
        cache
            .commit_version(&state(&clip_id), &metadata, &audio, now())
            .expect("commit");

        let version_dir = cache
            .clip_dir(&clip_id)
            .expect("clip dir")
            .join("versions")
            .join(&audio_sha256);
        fs::remove_file(version_dir.join("metadata.json")).expect("drop metadata");
        fs::write(version_dir.join("audio.mp3"), b"not json at all").expect("tamper");

        cache
            .commit_version(&state(&clip_id), &metadata, &audio, now())
            .expect("repair");

        let ready = cache
            .load_ready_clip(&clip_id)
            .expect("load")
            .expect("ready");
        assert_eq!(fs::read(&ready.audio_path).expect("audio"), audio);
    }

    #[test]
    fn a_missing_audio_file_is_cache_corrupt() {
        let (_home, cache) = cache();
        let clip_id = "b".repeat(64);
        let audio = silent_mp3(1);
        let audio_sha256 = sha256_hex(&audio);
        cache
            .commit_version(
                &state(&clip_id),
                &metadata(&clip_id, &audio_sha256, &audio),
                &audio,
                now(),
            )
            .expect("commit");

        let version_dir = cache
            .clip_dir(&clip_id)
            .expect("clip dir")
            .join("versions")
            .join(&audio_sha256);
        fs::remove_file(version_dir.join("audio.mp3")).expect("remove audio");

        let error = cache.load_ready_clip(&clip_id).expect_err("missing audio");
        assert_eq!(error.machine_code(), "SPEECH_CACHE_CORRUPT");
    }

    #[test]
    fn an_unknown_gate_blocks_generation_until_an_explicit_regenerate() {
        assert!(AttemptStatus::Unknown.blocks_generation());
        assert!(AttemptStatus::ProviderSucceededArtifactMissing.blocks_generation());
        assert!(!AttemptStatus::Succeeded.blocks_generation());
        assert!(!AttemptStatus::ProviderFailed.blocks_generation());
        assert!(!AttemptStatus::CancelledBeforeSend.blocks_generation());
    }

    #[test]
    fn attempt_records_hold_metadata_only() {
        let (_home, cache) = cache();
        let record = AttemptRecord {
            schema_version: ATTEMPT_SCHEMA_VERSION,
            attempt_id: "attempt-9".to_string(),
            clip_id: "c".repeat(64),
            provider: "senseaudio".to_string(),
            model: "sensenova-tts-2.0".to_string(),
            voice_id: "male_0004_a".to_string(),
            started_at: now().to_rfc3339(),
            finished_at: Some(now().to_rfc3339()),
            status: AttemptStatus::Unknown,
            unicode_characters: 4,
            estimated_billing_characters: 8,
            provider_usage_characters: None,
            product_error_code: Some("SPEECH_RESULT_UNKNOWN".to_string()),
            provider_code: None,
            trace_id: Some("trace-9".to_string()),
        };

        let path = cache
            .record_attempt(&record, now())
            .expect("record attempt");
        let text = fs::read_to_string(&path).expect("attempt record");
        assert!(text.contains("attempt-9"));
        assert!(
            !text.contains("高亮正文"),
            "attempt history must not store Speech Text"
        );
        for forbidden in ["audio.mp3", "audio_bytes", "text\":", "api_key", "Bearer"] {
            assert!(
                !text.contains(forbidden),
                "attempt history must not store {forbidden}"
            );
        }
    }

    #[test]
    fn a_second_lock_waits_for_the_first_writer() {
        let (_home, cache) = cache();
        let clip_id = "d".repeat(64);

        let first = ClipLock::acquire(&cache, &clip_id, now()).expect("first lock");
        assert!(first.path().exists());
        let waiting =
            ClipLock::acquire_with_timeout(&cache, &clip_id, now(), Duration::from_millis(150))
                .expect_err("a second writer must wait");
        assert_eq!(waiting.machine_code(), "SPEECH_IN_PROGRESS");

        drop(first);
        assert!(
            ClipLock::acquire(&cache, &clip_id, now()).is_ok(),
            "the lock must be released after the writer finishes"
        );
    }

    #[test]
    fn state_round_trips_through_disk() {
        let (_home, cache) = cache();
        let mut state = state(&"e".repeat(64));
        state.current_cache_status = ClipCacheStatus::Corrupt;
        state.generation_blocked = true;
        state.latest_error_code = Some("SPEECH_RESULT_UNKNOWN".to_string());

        cache.save_state(&state).expect("save state");
        let loaded = cache
            .load_state(&state.clip_id)
            .expect("load")
            .expect("state");

        assert_eq!(loaded, state);
        assert_eq!(loaded.current_cache_status.as_str(), "corrupt");
    }

    #[test]
    fn cache_clear_removes_clip_state_and_lifts_the_generation_gate() {
        let (_home, cache) = cache();
        let clip_id = "1".repeat(64);
        let audio = silent_mp3(1);
        let audio_sha256 = sha256_hex(&audio);
        let mut blocked = state(&clip_id);
        blocked.current_cache_status = ClipCacheStatus::Ready;
        blocked.current_audio_sha256 = Some(audio_sha256.clone());
        blocked.latest_attempt_status = Some(AttemptStatus::Unknown);
        blocked.generation_blocked = true;
        cache
            .commit_version(
                &blocked,
                &metadata(&clip_id, &audio_sha256, &audio),
                &audio,
                now(),
            )
            .expect("commit");
        cache.save_state(&blocked).expect("save gated state");

        let report = cache.clear_cache(now()).expect("clear cache");

        assert_eq!(report.removed, vec![clip_id.clone()]);
        assert!(report.skipped.is_empty());
        assert_eq!(
            report.cleared_generation_gates, 1,
            "clearing the cache is the explicit user action that lifts a generation gate"
        );
        assert!(
            cache.load_state(&clip_id).expect("load state").is_none(),
            "the clip state must be gone after cache clear"
        );
        assert!(!cache.clip_dir(&clip_id).expect("clip dir").exists());
    }

    #[test]
    fn cache_clear_skips_an_entry_that_holds_the_writer_lock() {
        let (_home, cache) = cache();
        let clip_id = "2".repeat(64);
        let audio = silent_mp3(1);
        let audio_sha256 = sha256_hex(&audio);
        cache
            .commit_version(&state(&clip_id), &metadata(&clip_id, &audio_sha256, &audio), &audio, now())
            .expect("commit");
        let held = ClipLock::acquire(&cache, &clip_id, now()).expect("hold the lock");

        let report = cache.clear_cache(now()).expect("clear cache");

        assert_eq!(report.removed, Vec::<String>::new());
        assert_eq!(
            report.skipped,
            vec![clip_id.clone()],
            "an in-flight entry must not be evicted by cache clear"
        );
        assert_eq!(report.cleared_generation_gates, 0);
        assert!(cache.clip_dir(&clip_id).expect("clip dir").exists());
        assert!(cache
            .load_ready_clip(&clip_id)
            .expect("load")
            .expect("still cached")
            .audio_path
            .exists());

        drop(held);
        let report = cache.clear_cache(now()).expect("clear after release");
        assert_eq!(report.removed, vec![clip_id]);
    }

    #[test]
    fn cache_clear_without_any_clip_state_is_a_no_op() {
        let (_home, cache) = cache();

        let report = cache.clear_cache(now()).expect("clear cache");

        assert!(report.removed.is_empty());
        assert!(report.skipped.is_empty());
        assert_eq!(report.cleared_generation_gates, 0);
    }

    #[test]
    fn history_clear_removes_attempt_metadata_but_keeps_the_cache_and_the_gate() {
        let (_home, cache) = cache();
        let clip_id = "3".repeat(64);
        let record = AttemptRecord {
            schema_version: ATTEMPT_SCHEMA_VERSION,
            attempt_id: "attempt-clear".to_string(),
            clip_id: clip_id.clone(),
            provider: "senseaudio".to_string(),
            model: "sensenova-tts-2.0".to_string(),
            voice_id: "male_0004_a".to_string(),
            started_at: now().to_rfc3339(),
            finished_at: Some(now().to_rfc3339()),
            status: AttemptStatus::Unknown,
            unicode_characters: 4,
            estimated_billing_characters: 8,
            provider_usage_characters: None,
            product_error_code: Some("SPEECH_RESULT_UNKNOWN".to_string()),
            provider_code: None,
            trace_id: None,
        };
        cache.record_attempt(&record, now()).expect("record attempt");
        let mut gated = state(&clip_id);
        gated.latest_attempt_id = Some("attempt-clear".to_string());
        gated.latest_attempt_status = Some(AttemptStatus::Unknown);
        gated.generation_blocked = true;
        cache.save_state(&gated).expect("save gated state");

        let report = cache.clear_history(now()).expect("clear history");

        assert_eq!(report.removed_attempts, 1);
        assert_eq!(
            report.cleared_generation_gates, 0,
            "history clear must never lift a generation gate"
        );
        assert!(!cache.attempts_dir().exists());
        let kept = cache.load_state(&clip_id).expect("load state").expect("state");
        assert!(
            kept.generation_blocked,
            "the unknown gate must survive history clear"
        );
        assert_eq!(
            kept.latest_attempt_id, None,
            "history clear must not leave a dangling latest_attempt_id"
        );
        assert_eq!(
            kept.latest_attempt_status,
            Some(AttemptStatus::Unknown),
            "the recorded outcome still explains the surviving gate"
        );

        let again = cache.clear_history(now()).expect("clear history again");
        assert_eq!(again.removed_attempts, 0);
    }
}
