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

/// Cached Speech Clip 的默认总容量预算：1 GiB（实施 spec 7.4，ADR 0007）。
pub const DEFAULT_CACHE_BUDGET_BYTES: u64 = 1024 * 1024 * 1024;
/// 调用 provider 前必须保留的磁盘安全余量：128 MiB（实施 spec 7.4）。
pub const CACHE_SAFETY_MARGIN_BYTES: u64 = 128 * 1024 * 1024;
/// 只允许**调高**安全余量的测试/运维注入变量（毫秒形式同 `APPLE_BOOKS_SPEECH_LOCK_TIMEOUT_MS`）。
///
/// 默认 128 MiB 已经写死在 [`CACHE_SAFETY_MARGIN_BYTES`]；这个变量让测试和运维可以在
/// 小磁盘、临界卷上证明「空间不足就本地失败」的 guard 真的会触发，而不能把保护调弱。
pub const MIN_FREE_BYTES_ENV: &str = "APPLE_BOOKS_SPEECH_MIN_FREE_BYTES";

/// 预算里真正可以给缓存内容使用的部分：budget 减去安全余量。
pub const fn usable_cache_budget(budget_bytes: u64) -> u64 {
    budget_bytes.saturating_sub(CACHE_SAFETY_MARGIN_BYTES)
}

/// phonem 调用前要求保留的空闲空间。
///
/// 取默认安全余量与注入值的较大者：注入只能让要求更严格。
pub fn required_free_bytes() -> u64 {
    let injected = std::env::var(MIN_FREE_BYTES_ENV)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(0);
    injected.max(CACHE_SAFETY_MARGIN_BYTES)
}

/// Speech 根所在文件系统报告的可用空间（bytes）；探测失败时返回 `None`。
///
/// 拿不到数字时不猜测失败：真正的写入仍然由原子提交和 `SPEECH_STORAGE_UNAVAILABLE`
/// 兜底，这里只是「先付费再发现无法落盘」前的提前量。
pub fn available_bytes(root: &Path) -> Option<u64> {
    let path = std::ffi::CString::new(root.as_os_str().to_string_lossy().as_bytes()).ok()?;
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    // statvfs 只读，不改文件系统状态；失败（例如路径不存在）时不阻塞调用方。
    let status = unsafe { libc::statvfs(path.as_ptr(), &mut stats) };
    if status != 0 {
        return None;
    }
    let block = u64::try_from(stats.f_frsize).ok()?;
    let available = u64::try_from(stats.f_bavail).ok()?;
    block.checked_mul(available)
}

/// 一个 Cached Speech Clip 当前被哪种操作占用。
///
/// ADR 0007「存储预算与淘汰」：正在生成、播放、导出或持锁的 entry 不参与淘汰，手动
/// `speech cache clear` 也同样跳过。`Generation` 由跨进程 writer 锁表达；`Playback`
/// 与 `Export` 由显式 usage marker 表达——`speech play` / `speech export` 落地时通过
/// [`ClipUseGuard`] 取用同一套 marker，guard 因此天然覆盖它们。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipUseKind {
    /// 正在生成（writer 锁）。
    Generation,
    /// 正在播放（usage marker）。
    Playback,
    /// 正在导出（usage marker）。
    Export,
}

impl ClipUseKind {
    /// 收据与人类输出里的稳定取值。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Generation => "generation",
            Self::Playback => "playback",
            Self::Export => "export",
        }
    }

    fn marker_suffix(self) -> &'static str {
        match self {
            Self::Generation => "lock",
            Self::Playback => "play",
            Self::Export => "export",
        }
    }
}

/// 播放/导出占用一个 Cached Speech Clip 的跨进程凭证；Drop 时释放。
///
/// `speech play`（已实现，见 [`crate::speech::play`]）与 `speech export`（尚未实现）
/// 必须持有它：LRU 维护和
/// `speech cache clear` 因此不会在音频被读取时抽走 entry。
#[derive(Debug)]
pub struct ClipUseGuard {
    path: PathBuf,
}

impl ClipUseGuard {
    /// 占用指定 clip；`kind` 只能是 [`ClipUseKind::Playback`] 或 [`ClipUseKind::Export`]。
    pub fn acquire(
        cache: &ClipCache,
        clip_id: &str,
        kind: ClipUseKind,
        now: DateTime<Utc>,
    ) -> Result<Self, ClipLockError> {
        if matches!(kind, ClipUseKind::Generation) {
            // 生成必须走 writer 锁：single flight 才不会被 marker 旁路。
            return Err(ClipLockError::Unavailable(
                cache.locks_dir(),
                std::io::Error::other("generation must use the writer lock"),
            ));
        }
        let path = cache.usage_marker_path(clip_id, kind)?;
        fs::create_dir_all(cache.locks_dir())
            .map_err(|error| ClipLockError::Unavailable(cache.locks_dir(), error))?;
        if path.exists() {
            // marker 已经存在：占不到就不放行，绝不覆盖别人的凭证。
            if cache.usage_marker_is_stale(&path) {
                let _ = fs::remove_file(&path);
            } else {
                return Err(ClipLockError::InProgress);
            }
        }
        // create_new 保证只可能是自己创建的这个文件：不会静默改写别人的凭证。
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| ClipLockError::Unavailable(path.clone(), error))?;
        let body = format!(
            "{{\"kind\":\"{}\",\"acquired_at\":\"{}\"}}\n",
            kind.as_str(),
            now.to_rfc3339()
        );
        file.write_all(body.as_bytes())
            .map_err(|error| ClipLockError::Unavailable(path.clone(), error))?;
        let _ = file.sync_all();
        Ok(Self { path })
    }
}

impl Drop for ClipUseGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// `speech cache status` 的本地视图：预算、占用与异常 entry。
///
/// 只读，不调用 provider，也不改写任何 clip 状态（实施 spec 5.7）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheStatusReport {
    /// 配置的总预算。
    pub budget_bytes: u64,
    /// 调用 provider 前必须保留的安全余量。
    pub safety_margin_bytes: u64,
    /// 预算中可给缓存内容使用的部分（budget - margin）。
    pub usable_budget_bytes: u64,
    /// 当前缓存占用（`clips/` 下所有文件的 apparent 大小之和）。
    pub used_bytes: u64,
    /// 已接受 entry 数：current pointer 指向一个通过校验的不可变 version。
    pub accepted_entries: usize,
    /// 没有有效音频的 entry 数（absent / unknown gate 等）。
    pub absent_entries: usize,
    /// 被 generation gate 阻塞的 entry 数。
    pub blocked_entries: usize,
    /// 状态不可信或音频校验失败的 entry 数。
    pub corrupt_entries: usize,
    /// 正在生成/播放/导出或持锁的 entry 数。
    pub locked_entries: usize,
    /// 可回收的孤立 version 目录数（没有 lock、没有被 current pointer 引用）。
    pub reclaimable_versions: usize,
    /// 每个 clip 的明细，按 clip ID 排序，便于稳定输出。
    pub entries: Vec<CacheStatusEntry>,
}

/// 单个 clip 的状态明细。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheStatusEntry {
    /// 完整 clip ID。
    pub clip_id: String,
    /// `ready` / `absent` / `corrupt`。
    pub status: ClipCacheStatus,
    /// 是否已接受：current pointer 指向通过校验的 version。
    pub accepted: bool,
    /// clip 级 generation gate（unknown / provider-succeeded-no-artifact）。
    pub generation_blocked: bool,
    /// 占用该 entry 的操作，没有占用时为 `null`。
    pub in_use: Option<ClipUseKind>,
    /// 该 clip 占用的字节。
    pub used_bytes: u64,
    /// 该 clip 下未被 current pointer 引用的 version 目录数。
    pub reclaimable_versions: usize,
    /// 最近一次被使用（生成/播放/导出/复用）的时间。
    pub last_used_at: Option<String>,
}

/// LRU / budget 维护的报告。
///
/// 只删除「没有 lock/reference 且可淘汰」的内容：当前 clip、正在生成、播放、导出或
/// 持锁的 entry 一律保留，并在 `skipped` 里说明原因。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetMaintenanceReport {
    /// 配置的总预算。
    pub budget_bytes: u64,
    /// 安全余量。
    pub safety_margin_bytes: u64,
    /// 可给缓存内容使用的部分。
    pub usable_budget_bytes: u64,
    /// 维护前占用。
    pub used_bytes_before: u64,
    /// 维护后占用。
    pub used_bytes_after: u64,
    /// 被 LRU 淘汰的 clip ID。
    pub evicted: Vec<String>,
    /// 因正在生成/播放/导出或持锁而保留的 clip ID。
    pub skipped: Vec<String>,
    /// 被回收的孤立 version 目录数。
    pub reclaimed_versions: usize,
    /// 回收后是否仍然保住安全余量（budget - margin）。
    pub safety_margin_preserved: bool,
}

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
    /// 最近一次被使用的时间（生成、缓存命中、播放、导出）。
    ///
    /// LRU 的「使用」定义在 ADR 0007「存储预算与淘汰」：只有真的被读走的 entry 才算
    /// 最近使用过。缺失时退回 `updated_at`，因此旧 state.json 不需要迁移即可排序。
    #[serde(default)]
    pub last_used_at: Option<String>,
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
    /// 生成该版本的 Speech Attempt ID。
    ///
    /// `None` 表示这个版本不是某次 provider 调用直接产出的，而是 Speech Cache
    /// Rehydration 从一个已验证的 Exported Speech Clip 复制回来的版本（ADR 0007
    /// 「显式 generate 发现缓存已淘汰但存在 checksum 匹配的 Exported Speech Clip …
    /// 该动作不创建 Speech Attempt」）。
    #[serde(default)]
    pub attempt_id: Option<String>,
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
    /// 因正在生成/播放/导出或持锁而跳过的 clip ID。
    pub skipped: Vec<String>,
    /// 每个被跳过的 clip 的占用原因；与 `skipped` 同序。
    pub skipped_reasons: Vec<ClipUseSkip>,
    /// 被显式清除的 generation gate 数量。
    pub cleared_generation_gates: usize,
}

/// 被跳过（保留）的 entry 与占用它的操作。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipUseSkip {
    /// 完整 clip ID。
    pub clip_id: String,
    /// 占用该 entry 的操作。
    pub in_use: ClipUseKind,
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
                let file = file.map_err(|error| ClipCacheError::unavailable(&day.path(), error))?;
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
            if removed_in_day > 0
                && fs::read_dir(day.path())
                    .ok()
                    .is_some_and(|mut left| left.next().is_none())
            {
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

    /// 配置的 Cached Speech Clip 总预算。
    pub fn cache_budget_bytes(&self) -> Result<u64, ClipCacheError> {
        self.store
            .load_config()
            .map(|config| config.cache_budget_bytes)
            .map_err(store_error)
    }

    /// 播放/导出 usage marker 的路径（`locks/<clip_id>.play` / `.export`）。
    pub fn usage_marker_path(
        &self,
        clip_id: &str,
        kind: ClipUseKind,
    ) -> Result<PathBuf, ClipCacheError> {
        Ok(self
            .locks_dir()
            .join(format!("{clip_id}.{}", kind.marker_suffix())))
    }

    /// marker 文件年龄超过阈值即视为崩溃遗留。
    fn usage_marker_is_stale(&self, path: &Path) -> bool {
        is_stale_lock(path)
    }

    /// 该 clip 是否仍被显式占用（播放/导出 marker）。
    fn usage_marker(&self, clip_id: &str, kind: ClipUseKind) -> Option<PathBuf> {
        let path = self.usage_marker_path(clip_id, kind).ok()?;
        if !path.exists() {
            return None;
        }
        if self.usage_marker_is_stale(&path) {
            // 崩溃遗留的凭证不永久钉住 entry：清掉后按未占用处理。
            let _ = fs::remove_file(&path);
            return None;
        }
        Some(path)
    }

    /// 该 entry 是否正在被生成、播放或导出（含持锁）。
    ///
    /// 生成由跨进程 writer 锁表达；播放与导出由 [`ClipUseGuard`] 维护的 usage marker
    /// 表达。`speech play` 已经通过 [`ClipUseGuard`] 取用同一套 marker（见
    /// [`crate::speech::play`]），`speech export` 落地时照搬即可；这个 guard 因此天然
    /// 覆盖这两种操作，并已被播放占用与 `clear_or_eviction_skips_*` 直接证明。
    pub fn clip_in_use(&self, clip_id: &str) -> Option<ClipUseKind> {
        match ClipLock::acquire_with_timeout(self, clip_id, Utc::now(), Duration::ZERO) {
            // 拿不到 writer 锁：同一个 clip 正在生成（或别的进程持锁）。
            Err(ClipLockError::InProgress) => Some(ClipUseKind::Generation),
            Err(ClipLockError::Unavailable(..)) => None,
            // 锁在 Drop 时立刻释放；这里只回答「现在有没有人占着」。
            Ok(_lock) => self
                .usage_marker(clip_id, ClipUseKind::Export)
                .map(|_| ClipUseKind::Export)
                .or_else(|| {
                    self.usage_marker(clip_id, ClipUseKind::Playback)
                        .map(|_| ClipUseKind::Playback)
                }),
        }
    }

    /// 记录一次「使用」：缓存命中、播放、导出都算最近使用过（LRU 的 U）。
    ///
    /// 尽力而为：写不进 `state.json` 不能取消一次已经成功的读，排序退回 `updated_at`。
    pub fn touch_clip(&self, clip_id: &str, now: DateTime<Utc>) {
        let Ok(Some(mut state)) = self.load_state(clip_id) else {
            return;
        };
        let stamp = now.to_rfc3339();
        if state.last_used_at.as_deref() == Some(stamp.as_str()) {
            return;
        }
        state.last_used_at = Some(stamp);
        let _ = self.save_state(&state);
    }

    /// LRU 排序键：优先 `last_used_at`，缺失退回 `updated_at`。
    fn recency_key(state: &ClipState) -> &str {
        state
            .last_used_at
            .as_deref()
            .unwrap_or(state.updated_at.as_str())
    }

    /// 按 clip ID 排序的所有 clip 目录名；不猜测、不删除非 sha256 的残留。
    fn sorted_clip_ids(&self) -> Result<Vec<String>, ClipCacheError> {
        let clips_dir = self.clips_dir();
        let entries = match fs::read_dir(&clips_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(ClipCacheError::unavailable(&clips_dir, error)),
        };
        let mut clip_ids = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| ClipCacheError::unavailable(&clips_dir, error))?;
            if !entry.path().is_dir() {
                continue;
            }
            let clip_id = entry.file_name().to_string_lossy().into_owned();
            if is_clip_id(&clip_id) {
                clip_ids.push(clip_id);
            }
        }
        clip_ids.sort();
        Ok(clip_ids)
    }

    /// 该 clip 占用的字节（`state.json` + 所有 version 的 apparent 大小）。
    pub fn clip_used_bytes(&self, clip_id: &str) -> Result<u64, ClipCacheError> {
        Ok(dir_size(&self.clip_dir(clip_id)?))
    }

    /// `speech cache status`：本地 budget / 占用 / 异常 entry 视图。
    ///
    /// 只读：不调用 provider，不改写任何 clip 状态，也不删除任何东西。当前不可信的
    /// entry（state 读不出、音频校验失败）和未被 current pointer 引用的 version 目录
    /// 都只报告，不做猜测性修复（实施 spec 5.7 / 7.2）。
    pub fn cache_status(&self) -> Result<CacheStatusReport, ClipCacheError> {
        let budget_bytes = self.cache_budget_bytes()?;
        let mut report = CacheStatusReport {
            budget_bytes,
            safety_margin_bytes: CACHE_SAFETY_MARGIN_BYTES,
            usable_budget_bytes: usable_cache_budget(budget_bytes),
            ..CacheStatusReport::default()
        };
        for clip_id in self.sorted_clip_ids()? {
            let clip_dir = self.clip_dir(&clip_id)?;
            let used_bytes = dir_size(&clip_dir);
            let in_use = self.clip_in_use(&clip_id);
            // 只读一次 state：pointer、blocked 与最近使用时间都从这一份来。
            let stored = self.load_state(&clip_id)?;
            let referenced = stored
                .as_ref()
                .and_then(|state| state.current_audio_sha256.clone());
            // 孤立 version 只在确认没有 lock / reference 后才可回收：生成中或持锁的 clip
            // 不计入 reclaimable，避免把正在放置的新 version 当成垃圾。
            let orphans = version_dirs(&clip_dir)
                .into_iter()
                .filter(|sha| Some(sha) != referenced.as_ref())
                .count();
            let reclaimable_versions = if in_use.is_none() { orphans } else { 0 };
            let last_used_at = stored.as_ref().and_then(|state| state.last_used_at.clone());

            let (status, accepted, blocked) = match self.load_ready_clip(&clip_id) {
                Ok(Some(_)) => (ClipCacheStatus::Ready, true, false),
                Ok(None) => match &stored {
                    Some(state) => {
                        let status = if state.current_cache_status == ClipCacheStatus::Corrupt {
                            ClipCacheStatus::Corrupt
                        } else {
                            ClipCacheStatus::Absent
                        };
                        (status, false, state.generation_blocked)
                    }
                    // clip 目录里没有 current pointer：既不是缓存也不是 gate，只能报告。
                    None => (ClipCacheStatus::Corrupt, false, false),
                },
                // 音频/metadata 与 pointer 矛盾：报 corrupt，不猜测、不调用 provider。
                Err(_) => (ClipCacheStatus::Corrupt, false, false),
            };

            report.used_bytes += used_bytes;
            match status {
                ClipCacheStatus::Ready if accepted => report.accepted_entries += 1,
                ClipCacheStatus::Corrupt => report.corrupt_entries += 1,
                _ => report.absent_entries += 1,
            }
            if blocked {
                report.blocked_entries += 1;
            }
            if in_use.is_some() {
                report.locked_entries += 1;
            }
            report.reclaimable_versions += reclaimable_versions;
            report.entries.push(CacheStatusEntry {
                clip_id,
                status,
                accepted,
                generation_blocked: blocked,
                in_use,
                used_bytes,
                reclaimable_versions,
                last_used_at,
            });
        }
        Ok(report)
    }

    /// LRU / budget 维护：超预算时先回收无引用的孤立 version，再淘汰最近最少使用的
    /// clip，最后如实报告是否保住安全余量。
    ///
    /// `keep` 是调用方**已经持锁**的 clip（例如当前正在生成的 clip）：它不参与淘汰。
    /// 正在生成、播放、导出或持锁的 entry 也不参与淘汰。没有超预算时什么都不删：
    /// 回收与淘汰都只由压力触发，不把「顺手清理」变成隐性删除。
    ///
    /// 只有确认没有 lock/reference 的 version 目录才会被回收（实施 spec 7.2）。
    pub fn maintain_budget(
        &self,
        keep: &[&str],
        now: DateTime<Utc>,
    ) -> Result<BudgetMaintenanceReport, ClipCacheError> {
        let budget_bytes = self.cache_budget_bytes()?;
        let usable_budget_bytes = usable_cache_budget(budget_bytes);
        let mut report = BudgetMaintenanceReport {
            budget_bytes,
            safety_margin_bytes: CACHE_SAFETY_MARGIN_BYTES,
            usable_budget_bytes,
            ..BudgetMaintenanceReport::default()
        };

        /// 一个可淘汰候选：占用字节 + LRU 排序键。
        struct Candidate {
            clip_id: String,
            used_bytes: u64,
            recency: String,
        }
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut in_use: Vec<(String, ClipUseKind)> = Vec::new();
        for clip_id in self.sorted_clip_ids()? {
            let clip_dir = self.clip_dir(&clip_id)?;
            let size = dir_size(&clip_dir);
            let protected = keep.contains(&clip_id.as_str());
            match self.clip_in_use(&clip_id) {
                // 别的进程正在生成/播放/导出：既不能淘汰，也不能回收它的任何东西。
                Some(kind) if !protected => in_use.push((clip_id, kind)),
                _ => {
                    if !protected {
                        let recency = self
                            .load_state(&clip_id)
                            .ok()
                            .flatten()
                            .map(|state| Self::recency_key(&state).to_string())
                            .unwrap_or_default();
                        candidates.push(Candidate {
                            clip_id: clip_id.clone(),
                            used_bytes: size,
                            recency,
                        });
                    }
                }
            }
        }
        let mut used_bytes = self.total_used_bytes()?;
        report.used_bytes_before = used_bytes;

        // 超预算时才动手：先回收确认没有 lock/reference 的孤立 version，再淘汰 clip。
        if used_bytes > usable_budget_bytes {
            for candidate in &candidates {
                report.reclaimed_versions +=
                    self.reclaim_unreferenced_versions(&candidate.clip_id)?;
            }
            used_bytes = self.total_used_bytes()?;
        }

        while used_bytes > usable_budget_bytes {
            // 最近最少使用优先淘汰；排序键相同时按 clip ID 决定，保证输出稳定。
            let Some(next) = candidates
                .iter()
                .enumerate()
                .min_by(|(_, left), (_, right)| {
                    left.recency
                        .cmp(&right.recency)
                        .then_with(|| left.clip_id.cmp(&right.clip_id))
                })
                .map(|(index, _)| index)
            else {
                break;
            };
            let candidate = candidates.swap_remove(next);
            let clip_dir = self.clip_dir(&candidate.clip_id)?;
            fs::remove_dir_all(&clip_dir)
                .map_err(|error| ClipCacheError::unavailable(&clip_dir, error))?;
            used_bytes = used_bytes.saturating_sub(candidate.used_bytes);
            report.evicted.push(candidate.clip_id);
        }

        report.used_bytes_after = used_bytes;
        report.skipped = in_use.into_iter().map(|(clip_id, _)| clip_id).collect();
        report.safety_margin_preserved =
            used_bytes.saturating_add(CACHE_SAFETY_MARGIN_BYTES) <= budget_bytes;
        let _ = now;
        Ok(report)
    }

    /// `clips/` 下所有文件的 apparent 大小之和。
    fn total_used_bytes(&self) -> Result<u64, ClipCacheError> {
        let clips_dir = self.clips_dir();
        match fs::read_dir(&clips_dir) {
            Ok(entries) => {
                let mut total = 0;
                for entry in entries {
                    let entry =
                        entry.map_err(|error| ClipCacheError::unavailable(&clips_dir, error))?;
                    if entry.path().is_dir() {
                        total += dir_size(&entry.path());
                    }
                }
                Ok(total)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
            Err(error) => Err(ClipCacheError::unavailable(&clips_dir, error)),
        }
    }

    /// 回收没有被 current pointer 引用的 version 目录。
    ///
    /// 只按「没有 reference」判断，调用方负责先确认没有 lock（或自己持锁）：生成中的
    /// clip 会先完成 `state.json` 切换，孤立 version 只可能出现在 pointer 提交成功之后
    /// （实施 spec 7.2「旧 version 只在 pointer 提交成功后进入垃圾回收」）。
    fn reclaim_unreferenced_versions(&self, clip_id: &str) -> Result<usize, ClipCacheError> {
        let clip_dir = self.clip_dir(clip_id)?;
        let referenced = self
            .load_state(clip_id)?
            .and_then(|state| state.current_audio_sha256);
        let mut reclaimed = 0;
        for sha in version_dirs(&clip_dir) {
            if Some(&sha) == referenced.as_ref() {
                continue;
            }
            let version_dir = clip_dir.join("versions").join(&sha);
            fs::remove_dir_all(&version_dir)
                .map_err(|error| ClipCacheError::unavailable(&version_dir, error))?;
            reclaimed += 1;
        }
        Ok(reclaimed)
    }

    /// `speech cache clear`：删除可淘汰 clip，显式清除无有效音频的阻塞门。
    ///
    /// 每个 clip 先问 [`ClipCache::clip_in_use`]：正在生成（writer 锁）、播放或导出
    /// （usage marker）的 entry 跳过并计入 `skipped`，不终止正在进行的操作；没有被占用
    /// 才删除，因此并发生成不会被清掉（ADR 0007「存储预算与淘汰」）。
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
            // 先问占用：正在生成（writer 锁）、播放或导出（usage marker）的 entry 一律
            // 跳过，不终止正在进行的操作（ADR 0007「存储预算与淘汰」）。
            if let Some(kind) = self.clip_in_use(&clip_id) {
                report.skipped.push(clip_id.clone());
                report.skipped_reasons.push(ClipUseSkip {
                    clip_id,
                    in_use: kind,
                });
                continue;
            }
            // 没人占用时再取 writer 锁：删除期间并发生成会被锁挡住而不是被删掉。
            match ClipLock::acquire_with_timeout(self, &clip_id, now, Duration::ZERO) {
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
                Err(ClipLockError::InProgress) => {
                    report.skipped.push(clip_id.clone());
                    report.skipped_reasons.push(ClipUseSkip {
                        clip_id,
                        in_use: ClipUseKind::Generation,
                    });
                }
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

/// 把 store 错误收敩成 clip 缓存错误；不引入新的错误码，也不泄漏秘密。
fn store_error(error: crate::speech::store::SpeechStoreError) -> ClipCacheError {
    match error {
        crate::speech::store::SpeechStoreError::Unavailable { path, message } => {
            ClipCacheError::Unavailable { path, message }
        }
        crate::speech::store::SpeechStoreError::InvalidConfig(_) => ClipCacheError::Corrupt {
            path: PathBuf::new(),
            reason: "the speech configuration is not a valid Voice Profile",
        },
        crate::speech::store::SpeechStoreError::UnsupportedSchemaVersion(version) => {
            ClipCacheError::UnsupportedSchemaVersion {
                path: PathBuf::new(),
                version,
            }
        }
    }
}

/// 一个 clip 目录下所有 version 目录名（audio sha256）；顺序不保证。
fn version_dirs(clip_dir: &Path) -> Vec<String> {
    let versions_dir = clip_dir.join("versions");
    let Ok(entries) = fs::read_dir(&versions_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}

/// 目录树的 apparent 字节数（`metadata.len()` 之和）。
///
/// 用 apparent 大小而不是磁盘块：LRU 比较的是「占用了多少预算」，稀疏文件也如实计入。
fn dir_size(root: &Path) -> u64 {
    fn walk(directory: &Path, total: &mut u64) {
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, total);
            } else if let Ok(metadata) = fs::metadata(&path) {
                *total += metadata.len();
            }
        }
    }
    let mut total = 0;
    walk(root, &mut total);
    total
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

/// `clip_id` 必须是 64 位小写 sha256 hex，否则一律拒绝（避免路径逃逸）。
///
/// 供 `speech play` 等只需要判断标识形状、不需要读缓存的调用方使用。
pub fn is_valid_clip_id(value: &str) -> bool {
    is_clip_id(value)
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
            last_used_at: None,
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
            attempt_id: Some("attempt-1".to_string()),
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
            .commit_version(
                &state(&clip_id),
                &metadata(&clip_id, &audio_sha256, &audio),
                &audio,
                now(),
            )
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
        cache
            .record_attempt(&record, now())
            .expect("record attempt");
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
        let kept = cache
            .load_state(&clip_id)
            .expect("load state")
            .expect("state");
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

    /// 构造一条 attempt 记录；`attempt_id` 用于让 clip state 指向它。
    fn attempt(
        attempt_id: &str,
        clip_id: &str,
        at: DateTime<Utc>,
        status: AttemptStatus,
    ) -> AttemptRecord {
        AttemptRecord {
            schema_version: ATTEMPT_SCHEMA_VERSION,
            attempt_id: attempt_id.to_string(),
            clip_id: clip_id.to_string(),
            provider: "senseaudio".to_string(),
            model: "sensenova-tts-2.0".to_string(),
            voice_id: "male_0004_a".to_string(),
            started_at: at.to_rfc3339(),
            finished_at: Some(at.to_rfc3339()),
            status,
            unicode_characters: 4,
            estimated_billing_characters: 8,
            provider_usage_characters: None,
            product_error_code: None,
            provider_code: None,
            trace_id: None,
        }
    }

    /// 90 天保留窗口是自动维护：写新 attempt 时过期的旧 metadata 必须被删掉。
    #[test]
    fn attempt_history_older_than_the_retention_window_is_pruned_automatically() {
        let (_home, cache) = cache();
        let clip_id = "4".repeat(64);
        let expired_at = now() - chrono::Duration::days(ATTEMPT_HISTORY_RETENTION_DAYS + 10);
        cache
            .record_attempt(
                &attempt(
                    "attempt-expired",
                    &clip_id,
                    expired_at,
                    AttemptStatus::Succeeded,
                ),
                expired_at,
            )
            .expect("record the expired attempt");
        assert_eq!(
            cache.load_attempt("attempt-expired").expect("load"),
            Some(attempt(
                "attempt-expired",
                &clip_id,
                expired_at,
                AttemptStatus::Succeeded
            ))
        );

        // 新 attempt 的写入就是「正常维护」：过期记录不能继续留在历史里。
        cache
            .record_attempt(
                &attempt("attempt-fresh", &clip_id, now(), AttemptStatus::Succeeded),
                now(),
            )
            .expect("record the fresh attempt");

        assert_eq!(
            cache.load_attempt("attempt-expired").expect("load expired"),
            None,
            "an attempt older than the retention window must be pruned automatically"
        );
        assert!(
            cache
                .load_attempt("attempt-fresh")
                .expect("load fresh")
                .is_some(),
            "an attempt inside the retention window must survive"
        );
        // 空的按天目录一起回收，不留垃圾。
        assert_eq!(
            fs::read_dir(cache.attempts_dir())
                .expect("attempts dir")
                .count(),
            1,
            "the emptied retention day directory must be removed"
        );

        let report = cache
            .prune_attempt_history(now(), ATTEMPT_HISTORY_RETENTION_DAYS)
            .expect("prune again");
        assert_eq!(
            report,
            AttemptPruneReport::default(),
            "pruning must be idempotent"
        );
    }

    /// 清理只删 attempt metadata：缓存、generation gate 与 current pointer 都不动，
    /// 只有指向已删除 attempt 的 `latest_attempt_id` 被纠正。
    #[test]
    fn pruning_reconciles_a_dangling_latest_attempt_id_without_touching_the_gate() {
        let (_home, cache) = cache();
        let clip_id = "5".repeat(64);
        let audio = silent_mp3(1);
        let audio_sha256 = sha256_hex(&audio);
        let metadata = metadata(&clip_id, &audio_sha256, &audio);
        cache
            .commit_version(&state(&clip_id), &metadata, &audio, now())
            .expect("commit");

        let expired_at = now() - chrono::Duration::days(ATTEMPT_HISTORY_RETENTION_DAYS + 10);
        cache
            .record_attempt(
                &attempt(
                    "attempt-expired",
                    &clip_id,
                    expired_at,
                    AttemptStatus::Unknown,
                ),
                expired_at,
            )
            .expect("record the expired attempt");
        // gate 与 latest attempt 在提交音频之后写：unknown gate 不随 attempt history 到期。
        let mut gated = cache.load_state(&clip_id).expect("load").expect("state");
        gated.latest_attempt_id = Some("attempt-expired".to_string());
        gated.latest_attempt_status = Some(AttemptStatus::Unknown);
        gated.generation_blocked = true;
        cache.save_state(&gated).expect("save the gated state");

        let report = cache
            .prune_attempt_history(now(), ATTEMPT_HISTORY_RETENTION_DAYS)
            .expect("prune");

        assert_eq!(report.removed_attempts, 1);
        assert_eq!(
            report.reconciled_clip_states, 1,
            "clip state must never keep naming a pruned attempt"
        );
        let kept = cache.load_state(&clip_id).expect("load").expect("state");
        assert_eq!(
            kept.latest_attempt_id, None,
            "a pruned attempt must not stay referenced by state.json"
        );
        assert_eq!(
            kept.latest_attempt_status,
            Some(AttemptStatus::Unknown),
            "the recorded outcome still explains the surviving gate"
        );
        assert!(
            kept.generation_blocked,
            "pruning must never lift a generation gate"
        );
        assert_eq!(
            kept.current_audio_sha256.as_deref(),
            Some(audio_sha256.as_str()),
            "pruning must never move the current pointer"
        );
        assert_eq!(kept.current_cache_status, ClipCacheStatus::Ready);
        let ready = cache
            .load_ready_clip(&clip_id)
            .expect("load")
            .expect("ready");
        assert_eq!(fs::read(ready.audio_path).expect("audio"), audio);
    }

    /// 同一个 attempt 的终态更新必须落在同一个文件里：按 attempt 自己的开始时间归档，
    /// 跨天也不会留下两条记录。
    #[test]
    fn an_attempt_record_is_updated_in_place_across_a_day_boundary() {
        let (_home, cache) = cache();
        let clip_id = "6".repeat(64);
        let started_at = DateTime::parse_from_rfc3339("2026-09-11T23:30:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        let mut record = attempt(
            "attempt-overnight",
            &clip_id,
            started_at,
            AttemptStatus::InProgress,
        );
        record.finished_at = None;
        cache
            .record_attempt(&record, started_at)
            .expect("record the in-progress attempt");
        assert_eq!(record.status, AttemptStatus::InProgress);
        assert_eq!(record.finished_at, None);

        // provider 返回后按同一 attempt_id 更新；`now` 已经跨过午夜。
        let finished_at = started_at + chrono::Duration::minutes(45);
        record.status = AttemptStatus::Succeeded;
        record.finished_at = Some(finished_at.to_rfc3339());
        cache
            .record_attempt(&record, finished_at)
            .expect("update the attempt in place");

        let days: Vec<String> = fs::read_dir(cache.attempts_dir())
            .expect("attempts dir")
            .flatten()
            .map(|day| day.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            days,
            vec!["2026-09-11".to_string()],
            "an attempt must stay in the day it started"
        );
        let files: Vec<String> = fs::read_dir(cache.attempts_dir().join("2026-09-11"))
            .expect("day dir")
            .flatten()
            .map(|file| file.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            files,
            vec!["attempt-overnight.json".to_string()],
            "one attempt must stay one record"
        );
        let stored = cache
            .load_attempt("attempt-overnight")
            .expect("load")
            .expect("record");
        assert_eq!(stored.status, AttemptStatus::Succeeded);
        assert_eq!(
            stored.finished_at.as_deref(),
            Some(finished_at.to_rfc3339().as_str())
        );
    }

    /// 为同一 clip 排过队的调用方拿到的锁必须知道自己等过：它要在锁内重新检查首个终态。
    #[test]
    fn a_queued_writer_lock_reports_that_it_waited() {
        let (_home, cache) = cache();
        let clip_id = "7".repeat(64);
        // 持锁方确认「锁已经在手」后才放行，因此第二个调用方一定排队，不依赖时序假设。
        let (holder, held) = {
            let cache = cache.clone();
            let clip_id = clip_id.clone();
            let (held_sender, held) = std::sync::mpsc::channel();
            let holder = std::thread::spawn(move || {
                let lock =
                    ClipLock::acquire(&cache, &clip_id, now()).expect("hold the writer lock");
                held_sender.send(()).expect("report the held lock");
                std::thread::sleep(std::time::Duration::from_millis(150));
                drop(lock);
            });
            (holder, held)
        };
        held.recv().expect("the writer lock is held");

        let queued = ClipLock::acquire(&cache, &clip_id, now()).expect("acquire after the writer");
        assert!(
            queued.waited(),
            "a caller that queued behind another writer must know it waited"
        );
        holder.join().expect("join the lock holder");
        drop(queued);

        let uncontended = ClipLock::acquire(&cache, &clip_id, now()).expect("acquire freely");
        assert!(
            !uncontended.waited(),
            "a fresh writer must not be treated as a waiter"
        );
    }

    /// 一个带自定义预算的 Speech 根；预算由非秘密 config 提供。
    fn cache_with_budget(budget_bytes: u64) -> (tempfile::TempDir, ClipCache) {
        let home = tempfile::tempdir().expect("temp home");
        let store = SpeechStore::from_home(home.path());
        store
            .save_config(&crate::speech::SpeechConfig {
                api_key_env: crate::speech::profile::DEFAULT_API_KEY_ENV.to_string(),
                profile: VoiceProfile::default(),
                cache_budget_bytes: budget_bytes,
            })
            .expect("save config");
        (home, ClipCache::new(store))
    }

    /// 提交一个真实 version，返回 (state, audio sha256)。
    fn committed_clip(cache: &ClipCache, clip_id: &str) -> (ClipState, String) {
        let audio = silent_mp3(1);
        let audio_sha256 = sha256_hex(&audio);
        let metadata = metadata(clip_id, &audio_sha256, &audio);
        let mut state = state(clip_id);
        state.latest_attempt_id = Some("attempt-1".to_string());
        state.latest_attempt_status = Some(AttemptStatus::Succeeded);
        cache
            .commit_version(&state, &metadata, &audio, now())
            .expect("commit");
        (state, audio_sha256)
    }

    /// 用稀疏文件把占用撑到 apparent `bytes`，不真的写这么多磁盘。
    fn pad(cache: &ClipCache, clip_id: &str, bytes: u64) {
        let path = cache.clip_dir(clip_id).expect("clip dir").join("bulk.bin");
        let file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&path)
            .expect("pad file");
        file.set_len(bytes).expect("pad length");
    }

    fn attempt_record(attempt_id: &str, clip_id: &str, started_at: DateTime<Utc>) -> AttemptRecord {
        AttemptRecord {
            schema_version: ATTEMPT_SCHEMA_VERSION,
            attempt_id: attempt_id.to_string(),
            clip_id: clip_id.to_string(),
            provider: "senseaudio".to_string(),
            model: "sensenova-tts-2.0".to_string(),
            voice_id: "male_0004_a".to_string(),
            started_at: started_at.to_rfc3339(),
            finished_at: Some(started_at.to_rfc3339()),
            status: AttemptStatus::Succeeded,
            unicode_characters: 4,
            estimated_billing_characters: 8,
            provider_usage_characters: Some(8),
            product_error_code: None,
            provider_code: None,
            trace_id: Some("trace-1".to_string()),
        }
    }

    /// LRU 不淘汰当前 clip：调用方用 `keep` 声明「我正拿着这把锁」。
    #[test]
    fn budget_maintenance_never_evicts_the_current_clip() {
        let (_home, cache) = cache_with_budget(200 * 1024 * 1024);
        let current = "a".repeat(64);
        let stale = "b".repeat(64);
        committed_clip(&cache, &current);
        committed_clip(&cache, &stale);
        // current 反而更旧：去掉 keep 保护它会第一个被淘汰，因此这条断言对 guard 敏感。
        let mut current_state = cache.load_state(&current).expect("state").expect("state");
        current_state.last_used_at = Some("2026-09-01T00:00:00Z".to_string());
        cache.save_state(&current_state).expect("save");
        let mut stale_state = cache.load_state(&stale).expect("state").expect("state");
        stale_state.last_used_at = Some("2026-09-20T00:00:00Z".to_string());
        cache.save_state(&stale_state).expect("save");
        pad(&cache, &current, 5 * 1024 * 1024);
        pad(&cache, &stale, 100 * 1024 * 1024);

        let report = cache
            .maintain_budget(&[&current], now())
            .expect("maintain budget");

        assert_eq!(report.evicted, vec![stale.clone()]);
        assert!(
            cache.clip_dir(&current).expect("clip dir").exists(),
            "the clip being generated must never be evicted"
        );
        assert!(
            !cache.clip_dir(&stale).expect("clip dir").exists(),
            "the least recently used clip must be evicted"
        );
        assert!(report.safety_margin_preserved);
    }

    /// LRU 不淘汰正在生成、播放、导出或持锁的 entry：判定只看占用，不看 keep 列表。
    #[test]
    fn budget_maintenance_never_evicts_an_entry_that_is_in_use() {
        let kinds = [
            (ClipUseKind::Generation, "lock"),
            (ClipUseKind::Playback, "play"),
            (ClipUseKind::Export, "export"),
        ];
        for (kind, marker) in kinds {
            // 200 MiB 预算 → 72 MiB 可用：总额超预算，淘汰掉 stale 之后剩下的占用
            // 仍然在可用预算内，因此安全余量是保得住的。
            let (_home, cache) = cache_with_budget(200 * 1024 * 1024);
            let held = "a".repeat(64);
            let stale = "b".repeat(64);
            committed_clip(&cache, &held);
            committed_clip(&cache, &stale);
            // held 是最早使用的：去掉占用检查它会第一个被淘汰。
            let mut held_state = cache.load_state(&held).expect("state").expect("state");
            held_state.last_used_at = Some("2026-09-01T00:00:00Z".to_string());
            cache.save_state(&held_state).expect("save");
            let mut stale_state = cache.load_state(&stale).expect("state").expect("state");
            stale_state.last_used_at = Some("2026-09-20T00:00:00Z".to_string());
            cache.save_state(&stale_state).expect("save");
            pad(&cache, &held, 60 * 1024 * 1024);
            pad(&cache, &stale, 60 * 1024 * 1024);

            // 生成用 writer 锁表达；播放与导出用 ClipUseGuard 的 usage marker 表达。
            let _lock = match kind {
                ClipUseKind::Generation => {
                    Some(ClipLock::acquire(&cache, &held, now()).expect("writer lock"))
                }
                _ => None,
            };
            let _use = match kind {
                ClipUseKind::Generation => None,
                _ => Some(ClipUseGuard::acquire(&cache, &held, kind, now()).expect("usage marker")),
            };
            let _ = marker;

            let report = cache.maintain_budget(&[], now()).expect("maintain budget");

            assert!(
                cache.clip_dir(&held).expect("clip dir").exists(),
                "{kind:?}: an entry in use must never be evicted"
            );
            assert_eq!(report.skipped, vec![held.clone()], "{kind:?}");
            assert_eq!(report.evicted, vec![stale.clone()], "{kind:?}");
            assert!(report.safety_margin_preserved, "{kind:?}");
            drop(_lock);
            drop(_use);
        }
    }

    /// 超预算时按 LRU 顺序淘汰，直到回到预算内。
    #[test]
    fn budget_maintenance_evicts_the_least_recently_used_clips_first() {
        let (_home, cache) = cache_with_budget(200 * 1024 * 1024);
        let oldest = "a".repeat(64);
        let middle = "b".repeat(64);
        let newest = "c".repeat(64);
        for (index, clip_id) in [&oldest, &middle, &newest].into_iter().enumerate() {
            committed_clip(&cache, clip_id);
            pad(&cache, clip_id, 60 * 1024 * 1024);
            let mut state = cache.load_state(clip_id).expect("state").expect("state");
            state.last_used_at = Some(format!("2026-09-0{}T00:00:00Z", index + 1));
            cache.save_state(&state).expect("save");
        }

        let report = cache.maintain_budget(&[], now()).expect("maintain budget");

        assert_eq!(report.evicted, vec![oldest.clone(), middle.clone()]);
        assert!(cache.clip_dir(&oldest).expect("dir").exists() == false);
        assert!(cache.clip_dir(&middle).expect("dir").exists() == false);
        assert!(cache.clip_dir(&newest).expect("dir").exists());
        assert!(report.safety_margin_preserved);
        assert_eq!(report.used_bytes_after <= report.usable_budget_bytes, true);
    }

    /// 没有可淘汰空间时如实报告，并且不删已经接受、正在使用的 entry。
    #[test]
    fn budget_maintenance_reports_insufficient_room_without_deleting_anything() {
        let (_home, cache) = cache_with_budget(200 * 1024 * 1024);
        let held = "a".repeat(64);
        committed_clip(&cache, &held);
        pad(&cache, &held, 200 * 1024 * 1024);
        let lock = ClipLock::acquire(&cache, &held, now()).expect("writer lock");

        let report = cache.maintain_budget(&[], now()).expect("maintain budget");

        assert!(report.evicted.is_empty());
        assert_eq!(report.skipped, vec![held.clone()]);
        assert!(
            !report.safety_margin_preserved,
            "a cache that cannot keep the safety margin must be reported, not hidden"
        );
        assert!(cache.clip_dir(&held).expect("dir").exists());
        drop(lock);
    }

    /// 孤立 version 只在超预算时回收：没有压力就不做隐性删除，引用中的 version 永不回收。
    #[test]
    fn budget_maintenance_reclaims_orphan_versions_only_under_pressure() {
        let (_home, cache) = cache_with_budget(200 * 1024 * 1024);
        let orphaned = "a".repeat(64);
        let stale = "b".repeat(64);
        let (_, referenced) = committed_clip(&cache, &orphaned);
        committed_clip(&cache, &stale);
        let orphan = cache
            .clip_dir(&orphaned)
            .expect("clip dir")
            .join("versions")
            .join("c".repeat(64));
        fs::create_dir_all(&orphan).expect("orphan dir");
        fs::write(orphan.join("audio.mp3"), b"orphan").expect("orphan audio");
        for (clip_id, used_at) in [
            (&orphaned, "2026-09-20T00:00:00Z"),
            (&stale, "2026-09-01T00:00:00Z"),
        ] {
            pad(&cache, clip_id, 60 * 1024 * 1024);
            let mut state = cache.load_state(clip_id).expect("state").expect("state");
            state.last_used_at = Some(used_at.to_string());
            cache.save_state(&state).expect("save");
        }

        // 没有压力（默认 1 GiB 预算）：什么都不动。
        let relaxed_home = tempfile::tempdir().expect("temp home");
        let relaxed_store = SpeechStore::from_home(relaxed_home.path());
        let relaxed = ClipCache::new(relaxed_store);
        let relaxed_id = "d".repeat(64);
        let (_, relaxed_referenced) = committed_clip(&relaxed, &relaxed_id);
        let relaxed_orphan = relaxed
            .clip_dir(&relaxed_id)
            .expect("clip dir")
            .join("versions")
            .join("e".repeat(64));
        fs::create_dir_all(&relaxed_orphan).expect("relaxed orphan dir");
        let relaxed_report = relaxed
            .maintain_budget(&[], now())
            .expect("maintain budget");
        assert_eq!(relaxed_report.reclaimed_versions, 0);
        assert!(
            relaxed_orphan.exists(),
            "without pressure nothing may be deleted"
        );
        assert!(relaxed
            .clip_dir(&relaxed_id)
            .expect("clip dir")
            .join("versions")
            .join(&relaxed_referenced)
            .exists());

        // 超预算（200 MiB 预算 → 72 MiB 可用）：先回收无引用的孤立 version，再淘汰 clip。
        let pressured = cache.maintain_budget(&[], now()).expect("maintain budget");
        assert_eq!(pressured.reclaimed_versions, 1);
        assert!(!orphan.exists(), "an unreferenced version is reclaimable");
        assert_eq!(pressured.evicted, vec![stale.clone()]);
        assert!(
            cache
                .clip_dir(&orphaned)
                .expect("clip dir")
                .join("versions")
                .join(&referenced)
                .exists(),
            "the referenced version is never reclaimable"
        );
    }

    /// attempt history 清理与音频 LRU 互相独立：清理不碰缓存，淘汰不碰 attempt metadata。
    #[test]
    fn attempt_history_pruning_and_lru_eviction_are_independent() {
        let (_home, cache) = cache_with_budget(200 * 1024 * 1024);
        let kept = "a".repeat(64);
        let evicted = "b".repeat(64);
        let (_, kept_audio) = committed_clip(&cache, &kept);
        committed_clip(&cache, &evicted);
        // kept 最近使用且被调用方持锁；evicted 是最早使用的淘汰候选。
        let mut kept_state = cache.load_state(&kept).expect("state").expect("state");
        kept_state.last_used_at = Some("2026-09-30T00:00:00Z".to_string());
        kept_state.latest_attempt_id = Some("attempt-expired".to_string());
        cache.save_state(&kept_state).expect("save");
        let mut evicted_state = cache.load_state(&evicted).expect("state").expect("state");
        evicted_state.last_used_at = Some("2026-09-01T00:00:00Z".to_string());
        cache.save_state(&evicted_state).expect("save");
        pad(&cache, &evicted, 100 * 1024 * 1024);
        pad(&cache, &kept, 5 * 1024 * 1024);

        // archived attempt 早于 90 天保留窗：必须被自动删除。直接落盘而不是走
        // record_attempt，否则记录刚写下就被它自带的维护删掉，断言不到东西。
        let expired_at = now() - chrono::Duration::days(120);
        let expired_day = expired_at.format("%Y-%m-%d").to_string();
        let expired_dir = cache.attempts_dir().join(&expired_day);
        fs::create_dir_all(&expired_dir).expect("expired day dir");
        let expired_record = attempt_record("attempt-expired", &kept, expired_at);
        let mut json = serde_json::to_string_pretty(&expired_record).expect("record JSON");
        json.push('\n');
        fs::write(expired_dir.join("attempt-expired.json"), json).expect("expired record");
        // 窗内的 attempt 同样直接落盘：`record_attempt` 自带维护，会在写下终态前
        // 就把过期记录删掉，那样就断言不到「清理真的发生了」。
        let fresh_dir = cache
            .attempts_dir()
            .join(now().format("%Y-%m-%d").to_string());
        fs::create_dir_all(&fresh_dir).expect("fresh day dir");
        let fresh_record = attempt_record("attempt-fresh", &evicted, now());
        let mut fresh_json = serde_json::to_string_pretty(&fresh_record).expect("record JSON");
        fresh_json.push('\n');
        fs::write(fresh_dir.join("attempt-fresh.json"), fresh_json).expect("fresh record");

        let maintenance = cache
            .maintain_budget(&[&kept], now())
            .expect("maintain budget");
        assert_eq!(maintenance.evicted, vec![evicted.clone()]);

        let prune = cache
            .prune_attempt_history(now(), ATTEMPT_HISTORY_RETENTION_DAYS)
            .expect("prune");

        assert_eq!(prune.removed_attempts, 1, "the expired metadata must go");
        assert_eq!(
            prune.reconciled_clip_states, 1,
            "a state naming a removed attempt must be reconciled"
        );
        assert!(
            cache
                .load_attempt("attempt-expired")
                .expect("load")
                .is_none(),
            "expired attempt history is pruned"
        );
        assert!(
            cache.load_attempt("attempt-fresh").expect("load").is_some(),
            "in-window attempt history survives"
        );
        let state = cache.load_state(&kept).expect("state").expect("state");
        assert_eq!(
            state.latest_attempt_id, None,
            "clip state must never keep naming a pruned attempt"
        );
        let ready = cache.load_ready_clip(&kept).expect("load").expect("ready");
        assert_eq!(
            ready.state.current_audio_sha256.as_deref(),
            Some(kept_audio.as_str()),
            "pruning must never remove a valid cache entry"
        );
        assert!(
            !state.generation_blocked,
            "pruning must never create a gate"
        );
        assert!(
            !cache.clip_dir(&evicted).expect("dir").exists(),
            "LRU still evicted the least recently used clip independently"
        );
    }

    /// 单独跑 attempt 清理也不碰缓存、gate 或用户导出投影。
    #[test]
    fn history_clear_removes_only_attempt_history() {
        let (_home, cache) = cache_with_budget(1024 * 1024 * 1024);
        let clip_id = "a".repeat(64);
        let (state, audio_sha256) = committed_clip(&cache, &clip_id);
        // 一个 unknown gate + 一份 export locator 投影 + 一份用户导出的音频。
        let mut gated = state.clone();
        gated.generation_blocked = true;
        gated.latest_attempt_status = Some(AttemptStatus::Unknown);
        gated.latest_attempt_id = Some("attempt-gate".to_string());
        gated.current_cache_status = ClipCacheStatus::Absent;
        gated.current_audio_sha256 = None;
        cache.save_state(&gated).expect("save gate");
        cache
            .record_attempt(&attempt_record("attempt-gate", &clip_id, now()), now())
            .expect("record");
        let exports = cache.root().join("exports.json");
        fs::write(&exports, b"{\"schema_version\":1}").expect("export projection");
        let user_audio = cache.root().join("user-owned.mp3");
        fs::write(&user_audio, b"user audio").expect("user audio");

        let report = cache.clear_history(now()).expect("clear history");

        assert_eq!(report.removed_attempts, 1);
        assert_eq!(report.cleared_generation_gates, 0);
        assert!(cache.load_attempt("attempt-gate").expect("load").is_none());
        let after = cache.load_state(&clip_id).expect("state").expect("state");
        assert_eq!(
            after.latest_attempt_id, None,
            "history clear must not leave a dangling attempt reference"
        );
        assert!(
            after.generation_blocked,
            "history clear must not lift the unknown gate"
        );
        assert!(
            exports.exists() && user_audio.exists(),
            "history clear must not touch export state or user-owned audio"
        );
        // gate clip 不再有有效音频：被 clear 的是 attempt history，缓存/门都保持原样。
        assert_eq!(after.current_cache_status, ClipCacheStatus::Absent);
        assert_eq!(after.text_sha256, gated.text_sha256);
        let _ = audio_sha256;
    }

    /// 只读状态视图：预算、占用、损坏与孤立 version 的计数方式。
    #[test]
    fn cache_status_counts_accepted_corrupt_locked_and_reclaimable_entries() {
        let (_home, cache) = cache_with_budget(200 * 1024 * 1024);
        let ready = "a".repeat(64);
        let broken = "b".repeat(64);
        let (_, referenced) = committed_clip(&cache, &ready);
        committed_clip(&cache, &broken);
        pad(&cache, &ready, 4096);
        // 破坏 broken 的音频字节：它必须是 corrupt 而不是 accepted。
        let audio_path = cache
            .clip_dir(&broken)
            .expect("clip dir")
            .join("versions")
            .join(
                cache
                    .load_state(&broken)
                    .expect("state")
                    .expect("state")
                    .current_audio_sha256
                    .expect("pointer"),
            )
            .join("audio.mp3");
        let mut bytes = fs::read(&audio_path).expect("audio");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        fs::write(&audio_path, &bytes).expect("tamper");
        // broken 正在播放：它既有占用也有孤立 version 之外的损坏状态。
        let playing = ClipUseGuard::acquire(&cache, &broken, ClipUseKind::Playback, now())
            .expect("acquire playback");
        // ready 未被占用：它的孤立 version 可回收；broken 正在播放：它的孤立 version 不可回收。
        for clip_id in [&ready, &broken] {
            let orphan = cache
                .clip_dir(clip_id)
                .expect("clip dir")
                .join("versions")
                .join("c".repeat(64));
            fs::create_dir_all(&orphan).expect("orphan dir");
        }

        let status = cache.cache_status().expect("cache status");

        assert_eq!(status.budget_bytes, 200 * 1024 * 1024);
        assert_eq!(
            status.usable_budget_bytes,
            200 * 1024 * 1024 - 128 * 1024 * 1024
        );
        assert_eq!(status.accepted_entries, 1);
        assert_eq!(status.corrupt_entries, 1);
        assert_eq!(status.locked_entries, 1);
        assert_eq!(
            status.reclaimable_versions, 1,
            "only the unlocked clip's orphan version may be reported as reclaimable"
        );
        assert_eq!(
            status
                .entries
                .iter()
                .find(|entry| entry.clip_id == ready)
                .expect("ready entry")
                .status,
            ClipCacheStatus::Ready
        );
        assert_eq!(
            status
                .entries
                .iter()
                .find(|entry| entry.clip_id == broken)
                .expect("broken entry")
                .in_use,
            Some(ClipUseKind::Playback)
        );
        assert_eq!(
            cache
                .clip_dir(&ready)
                .expect("clip dir")
                .join("versions")
                .join(&referenced)
                .exists(),
            true,
            "the referenced version must still be there"
        );
        drop(playing);
    }

    /// 预算连安全余量都留不出时，可用预算为 0：生成必须在 provider 前失败。
    #[test]
    fn a_budget_below_the_safety_margin_leaves_no_usable_cache_room() {
        let (_home, cache) = cache_with_budget(64 * 1024 * 1024);

        assert_eq!(crate::speech::usable_cache_budget(64 * 1024 * 1024), 0);
        let status = cache.cache_status().expect("cache status");
        assert_eq!(status.usable_budget_bytes, 0);
        assert_eq!(status.budget_bytes, 64 * 1024 * 1024);
    }

    ///播放/导出 marker 过期后不再钉住 entry：崩溃遗留的凭证不能永久占用缓存。
    #[test]
    fn a_stale_usage_marker_does_not_pin_an_entry_forever() {
        let (_home, cache) = cache_with_budget(200 * 1024 * 1024);
        let clip_id = "a".repeat(64);
        committed_clip(&cache, &clip_id);
        let marker = cache
            .usage_marker_path(&clip_id, ClipUseKind::Playback)
            .expect("marker path");
        fs::create_dir_all(marker.parent().expect("locks dir")).expect("locks dir");
        fs::write(&marker, "{\"kind\":\"playback\"}\n").expect("marker");
        // 把 marker 的 mtime 推到崩溃遗留阈值之外。
        let stale = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        let file = fs::OpenOptions::new()
            .write(true)
            .open(&marker)
            .expect("open marker");
        file.set_modified(stale).expect("backdate marker");

        assert_eq!(
            cache.clip_in_use(&clip_id),
            None,
            "a crashed holder must not pin the entry forever"
        );
        assert!(!marker.exists(), "the stale marker is dropped on sight");
    }
}
