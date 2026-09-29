//! `speech play`：播放一个已接受、已校验的 Speech Clip（实施 spec 5.5 / ADR 0007）。
//!
//! 播放是独立于生成的动作。本模块**故意不依赖** `senseaudio` 客户端与 `generate`：
//! 任何 play 路径在结构上都无法发起 provider 调用，因此「播放不联网」不是靠约定，
//! 而是没有可用的传输能力。播放器本身是可替换的 seam（[`AudioPlayer`]），
//! human 模式才启动 macOS 系统播放器，machine 模式只返回已校验路径与来源。

use crate::speech::cache::{
    is_valid_clip_id, ClipCache, ClipCacheError, ClipLockError, ClipUseGuard, ClipUseKind,
};
use crate::speech::clip::SpeechContentKind;
use crate::speech::store::{SpeechStore, SpeechStoreError};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// 播放来源。首版只有 Cached Speech Clip：Active Exported Speech Clip 回退依赖
/// `speech export` 的 manifest 与 locator，尚未落地（实施 spec 5.5 的查找顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackSource {
    /// 通过校验的本地 Speech Cache Entry。
    Cache,
}

impl PlaybackSource {
    /// 稳定的机器可读取值。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cache => "cache",
        }
    }
}

/// human / machine 分流。`--json` 走 [`PlayMode::Machine`]，不产生声音。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayMode {
    /// 校验后调用 macOS 系统播放器。
    Human,
    /// 只返回已校验路径与来源，不启动播放器、不产生其他副作用。
    Machine,
}

/// 一次播放请求。
#[derive(Debug, Clone)]
pub struct PlayRequest {
    /// 用户级 Speech 状态根。
    pub store: SpeechStore,
    /// 完整 clip ID（64 位小写 sha256 hex）。
    pub clip_id: String,
    /// human 还是 machine 模式。
    pub mode: PlayMode,
}

/// 播放解析结果。含已校验的本地音频事实，不含原文、密钥或音频字节。
#[derive(Debug, Clone)]
pub struct PlayOutcome {
    /// 完整 clip ID。
    pub clip_id: String,
    /// 音频来源。
    pub source: PlaybackSource,
    /// 已校验音频的绝对路径。
    pub audio_path: PathBuf,
    /// 音频字节的 SHA-256。
    pub audio_sha256: String,
    /// 音频字节数。
    pub audio_size_bytes: u64,
    /// 音频时长（毫秒）。
    pub audio_duration_ms: u64,
    /// 采样率（Hz）。
    pub sample_rate: u32,
    /// 书籍稳定 ID。
    pub asset_id: String,
    /// Annotation 稳定 ID。
    pub annotation_id: String,
    /// 内容部分。
    pub content_kind: SpeechContentKind,
    /// 是否真的启动了播放器；machine 模式恒为 `false`。
    pub played: bool,
}

/// 播放失败。全部发生在任何 provider 调用之前。
#[derive(Debug)]
pub enum PlayError {
    /// Speech 状态根或缓存不可用。
    Storage(SpeechStoreError),
    /// clip ID 不是 64 位小写 sha256 hex。
    InvalidClipId(String),
    /// 没有可播放的 clip。
    ClipNotFound {
        /// 被请求的 clip ID。
        clip_id: String,
    },
    /// 缓存损坏：字节、metadata 或 checksum 互相矛盾。
    CacheCorrupt {
        /// 出问题的路径。
        path: PathBuf,
        /// 稳定原因。
        reason: String,
    },
    /// 另一个进程正在播放或导出同一 clip。
    ClipInUse,
    /// 播放器启动或播放失败。
    PlaybackFailed {
        /// 失败原因。
        reason: String,
    },
}

impl PlayError {
    /// 稳定的 Machine JSON 错误码。
    pub const fn machine_code(&self) -> &'static str {
        match self {
            Self::Storage(..) => "SPEECH_STORAGE_UNAVAILABLE",
            Self::InvalidClipId(..) => "INVALID_ARGUMENT",
            Self::ClipNotFound { .. } => "SPEECH_CLIP_NOT_FOUND",
            Self::CacheCorrupt { .. } => "SPEECH_CACHE_CORRUPT",
            Self::ClipInUse => "SPEECH_IN_PROGRESS",
            Self::PlaybackFailed { .. } => "SPEECH_PLAYBACK_FAILED",
        }
    }

    /// 面向用户/机器消费者的说明。
    pub fn message(&self) -> String {
        match self {
            Self::Storage(error) => error.to_string(),
            Self::InvalidClipId(clip_id) => {
                format!("'{clip_id}' is not a 64 character lowercase sha256 clip id")
            }
            Self::ClipNotFound { clip_id } => format!(
                "no verified Speech Clip is available for {clip_id}; run `speech generate` first (playback never calls the Speech Provider)"
            ),
            Self::CacheCorrupt { path, reason } => format!(
                "the Speech Cache Entry is corrupt at {}: {reason}; run `speech generate --regenerate` to replace it",
                path.display()
            ),
            Self::ClipInUse => {
                "another playback or export of this Speech Clip is still in progress".to_string()
            }
            Self::PlaybackFailed { reason } => {
                format!("the macOS system player could not play this Speech Clip: {reason}")
            }
        }
    }

    /// 用户可以做什么。
    pub fn remediation(&self) -> &'static str {
        match self {
            Self::Storage(..) => {
                "check that ~/Library/Application Support/books-exporter/speech is readable and writable"
            }
            Self::InvalidClipId(..) => {
                "pass the full 64 character clip id reported by `speech generate --json`"
            }
            Self::ClipNotFound { .. } => {
                "run `speech generate` for the same content; `speech play` never contacts the Speech Provider"
            }
            Self::CacheCorrupt { .. } => {
                "run `speech generate --regenerate` to replace the corrupt entry with a newly accepted version"
            }
            Self::ClipInUse => "wait for the current playback or export to finish and try again",
            Self::PlaybackFailed { .. } => {
                "open the reported audio path in the macOS system player to hear it anyway"
            }
        }
    }
}

impl std::fmt::Display for PlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl From<ClipCacheError> for PlayError {
    fn from(error: ClipCacheError) -> Self {
        match error {
            ClipCacheError::Unavailable { path, message } => Self::Storage(
                SpeechStoreError::Unavailable { path, message },
            ),
            ClipCacheError::Corrupt { path, reason } => Self::CacheCorrupt {
                path,
                reason: reason.to_string(),
            },
            ClipCacheError::UnsupportedSchemaVersion { path, version } => Self::Storage(
                SpeechStoreError::Unavailable {
                    path,
                    message: format!("unsupported Speech cache schema version {version}"),
                },
            ),
        }
    }
}

/// 播放器 seam。
///
/// human 模式用它把一个**已校验**的本地音频路径交给 macOS 系统播放器；machine 模式
/// 传入 [`NeverPlayer`]，因此「不启动播放器」是可证明的，而不只是一句约定。
pub trait AudioPlayer {
    /// 播放一个已经校验过的音频文件。
    fn play(&self, audio_path: &Path) -> Result<(), String>;
}

/// 生产播放器：`afplay`，macOS 自带的系统播放器。
#[derive(Debug, Clone, Copy, Default)]
pub struct AfplayPlayer;

impl AudioPlayer for AfplayPlayer {
    fn play(&self, audio_path: &Path) -> Result<(), String> {
        // 同步等待：用户显式执行 play 就是要听到声音，进程退出前必须播完。
        let status = std::process::Command::new("afplay")
            .arg(audio_path)
            .status()
            .map_err(|error| format!("could not start afplay: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(match status.code() {
                Some(code) => format!("afplay exited with status {code}"),
                None => "afplay was terminated by a signal".to_string(),
            })
        }
    }
}

/// machine 模式的播放器：任何调用都是 bug，直接 panic 让测试立刻失败。
#[derive(Debug, Clone, Copy, Default)]
pub struct NeverPlayer;

impl AudioPlayer for NeverPlayer {
    fn play(&self, audio_path: &Path) -> Result<(), String> {
        panic!("machine `speech play` must not launch a player for {}", audio_path.display());
    }
}

/// `speech play` use case。
///
/// 顺序固定且不联网：校验标识形状 → 解析并校验缓存 → （human）占用 usage marker →
/// 刷新最近使用时间 → 启动播放器 → 释放 marker。
///
/// 校验一定发生在播放器之前：损坏或 checksum 不匹配的 entry 永远不会被播放。
/// human 模式全程持有 [`ClipUseGuard`]（`locks/<clip_id>.play`），因此 LRU 维护与
/// `speech cache clear` 不会在音频被读取时抽走它。machine 模式不占用 marker、
/// 不刷新 recency、不启动播放器，除返回已校验路径与来源外没有副作用。
pub fn play_clip(
    request: PlayRequest,
    player: &dyn AudioPlayer,
    now: DateTime<Utc>,
) -> Result<(PlayOutcome, Option<ClipUseGuard>), PlayError> {
    if !is_valid_clip_id(&request.clip_id) {
        return Err(PlayError::InvalidClipId(request.clip_id));
    }
    let cache = ClipCache::new(request.store);

    match request.mode {
        PlayMode::Machine => {
            // 只读解析：不占用、不刷新、不启动播放器。
            let outcome = resolve(&cache, &request.clip_id, false)?;
            Ok((outcome, None))
        }
        PlayMode::Human => {
            // 先占用再解析：解析与播放期间 entry 都不能被淘汰或清除。
            let guard = ClipUseGuard::acquire(&cache, &request.clip_id, ClipUseKind::Playback, now)
                .map_err(|error| match error {
                    ClipLockError::InProgress => PlayError::ClipInUse,
                    other => PlayError::Storage(SpeechStoreError::Unavailable {
                        path: PathBuf::new(),
                        message: other.message(),
                    }),
                })?;
            // 校验一定在播放器之前：损坏或 checksum 不匹配的 entry 永远不会被播放。
            let mut outcome = resolve(&cache, &request.clip_id, true)?;
            // marker 在整个播放期间有效，LRU 与 `speech cache clear` 不会抽走它。
            player
                .play(&outcome.audio_path)
                .map_err(|reason| PlayError::PlaybackFailed { reason })?;
            outcome.played = true;
            Ok((outcome, Some(guard)))
        }
    }
}

/// 解析并校验当前 Speech Cache Entry。
///
/// `load_ready_clip` 复用 #25 的完整性校验：音频字节的 SHA-256、size、metadata、
/// current pointer 与 MP3 可解析性必须全部成立，否则是稳定的 `SPEECH_CACHE_CORRUPT`。
/// 播放只读取，绝不修复、重新生成或刷新目录。
///
/// `record_use` 只在真正播放时为真：machine 模式除返回已校验路径与来源外不得留下
/// 任何状态变化，因此不刷新 recency。
fn resolve(cache: &ClipCache, clip_id: &str, record_use: bool) -> Result<PlayOutcome, PlayError> {
    let ready = cache
        .load_ready_clip(clip_id)
        .map_err(PlayError::from)?
        .ok_or_else(|| PlayError::ClipNotFound {
            clip_id: clip_id.to_string(),
        })?;
    // 播放算一次「最近使用过」（LRU 的 U）；只改 recency，不改逻辑 clip。
    if record_use {
        cache.touch_clip(clip_id, Utc::now());
    }
    let metadata = ready.metadata;
    Ok(PlayOutcome {
        clip_id: clip_id.to_string(),
        source: PlaybackSource::Cache,
        audio_path: ready.audio_path,
        audio_sha256: metadata.audio_sha256,
        audio_size_bytes: metadata.size_bytes,
        audio_duration_ms: metadata.duration_ms,
        sample_rate: metadata.sample_rate,
        asset_id: ready.state.asset_id,
        annotation_id: ready.state.annotation_id,
        content_kind: metadata.content_kind,
        played: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speech::audio::inspect_mp3;
    use crate::speech::cache::{ClipState, ClipVersionMetadata, CLIP_STATE_SCHEMA_VERSION};
    use crate::speech::text::sha256_hex;
    use crate::speech::SpeechContentKind;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tempfile::TempDir;

    /// 记录调用并可注入失败的假播放器：证明测试从不真正启动 afplay。
    struct FakePlayer {
        calls: Arc<Mutex<Vec<PathBuf>>>,
        failure: Option<String>,
    }

    impl FakePlayer {
        fn new() -> (Self, Arc<Mutex<Vec<PathBuf>>>) {
            let calls = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    calls: Arc::clone(&calls),
                    failure: None,
                },
                calls,
            )
        }

        fn failing(reason: &str) -> (Self, Arc<Mutex<Vec<PathBuf>>>) {
            let (player, calls) = Self::new();
            (
                Self {
                    calls: player.calls,
                    failure: Some(reason.to_string()),
                },
                calls,
            )
        }
    }

    impl AudioPlayer for FakePlayer {
        fn play(&self, audio_path: &Path) -> Result<(), String> {
            self.calls
                .lock()
                .expect("player calls")
                .push(audio_path.to_path_buf());
            match &self.failure {
                Some(reason) => Err(reason.clone()),
                None => Ok(()),
            }
        }
    }

    /// 线程安全版本：只记录调用次数，跨线程共享。
    struct CountingPlayer {
        calls: Arc<AtomicUsize>,
    }

    impl AudioPlayer for CountingPlayer {
        fn play(&self, _audio_path: &Path) -> Result<(), String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    use std::sync::Mutex;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-29T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc)
    }

    /// 一个可解析的最小 MP3。
    fn silent_mp3() -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xFB, 0x98, 0x0C];
        bytes.extend(std::iter::repeat(0u8).take(144 * 128_000 / 32_000 - 4));
        bytes
    }

    const CLIP_ID: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";

    /// 落盘一个已接受的 cache entry，返回 (store, 音频路径)。
    fn accepted_clip(home: &TempDir) -> (SpeechStore, PathBuf) {
        let store = SpeechStore::from_home(home.path());
        let cache = ClipCache::new(store.clone());
        let bytes = silent_mp3();
        let sha = sha256_hex(&bytes);
        let facts = inspect_mp3(&bytes).expect("parseable mp3");
        let state = ClipState {
            schema_version: CLIP_STATE_SCHEMA_VERSION,
            clip_id: CLIP_ID.to_string(),
            asset_id: "book-1".to_string(),
            annotation_id: "annotation-1".to_string(),
            content_kind: SpeechContentKind::Highlight,
            text_sha256: "b".repeat(64),
            current_cache_status: crate::speech::ClipCacheStatus::Ready,
            current_audio_sha256: Some(sha.clone()),
            latest_attempt_id: Some("attempt-1".to_string()),
            latest_attempt_status: None,
            latest_error_code: None,
            generation_blocked: false,
            updated_at: "2026-09-29T10:00:00Z".to_string(),
            last_used_at: Some("2026-09-29T10:00:00Z".to_string()),
        };
        let metadata = ClipVersionMetadata {
            schema_version: crate::speech::cache::CLIP_VERSION_SCHEMA_VERSION,
            clip_id: CLIP_ID.to_string(),
            audio_sha256: sha.clone(),
            asset_id: "book-1".to_string(),
            annotation_id: "annotation-1".to_string(),
            content_kind: SpeechContentKind::Highlight,
            text_sha256: "b".repeat(64),
            unicode_characters: 4,
            estimated_billing_characters: 4,
            billing_estimator_version: "v1".to_string(),
            provider: "senseaudio".to_string(),
            model: "sensenova-tts-2.0".to_string(),
            voice_id: "male_0004_a".to_string(),
            speed_x100: 100,
            volume_x100: 100,
            pitch: 0,
            format: facts.format.clone(),
            sample_rate: facts.sample_rate,
            bitrate: facts.bitrate,
            channel: facts.channel,
            duration_ms: facts.duration_ms,
            size_bytes: bytes.len() as u64,
            attempt_id: "attempt-1".to_string(),
            trace_id: Some("trace-1".to_string()),
            provider_usage_characters: Some(4),
            created_at: "2026-09-29T10:00:00Z".to_string(),
        };
        cache.commit_version(&state, &metadata, &bytes, now()).expect("commit version");
        let audio_path = cache
            .load_ready_clip(CLIP_ID)
            .expect("ready")
            .expect("accepted entry")
            .audio_path;
        (store, audio_path)
    }

    fn play_request(store: SpeechStore, mode: PlayMode) -> PlayRequest {
        PlayRequest {
            store,
            clip_id: CLIP_ID.to_string(),
            mode,
        }
    }

    /// 负向控制：machine 模式传入的播放器一旦被调用就 panic，
    /// 因此「不启动播放器」由类型系统加上这个 seam 一起保证。
    #[test]
    fn machine_mode_resolves_without_launching_a_player() {
        let home = TempDir::new().expect("home");
        let (store, audio_path) = accepted_clip(&home);

        let (outcome, guard) =
            play_clip(play_request(store.clone(), PlayMode::Machine), &NeverPlayer, now())
                .expect("resolve");

        assert_eq!(outcome.source, PlaybackSource::Cache);
        assert!(!outcome.played);
        assert_eq!(outcome.audio_path, audio_path);
        assert!(
            guard.is_none(),
            "machine mode must not hold a usage marker"
        );
    }

    /// machine 模式除返回已校验路径与来源外没有副作用：不刷新 recency。
    #[test]
    fn machine_mode_does_not_change_cache_state() {
        let home = TempDir::new().expect("home");
        let (store, _) = accepted_clip(&home);
        let cache = ClipCache::new(store.clone());
        let before = cache.load_state(CLIP_ID).expect("state").expect("present");

        let _ = play_clip(play_request(store, PlayMode::Machine), &NeverPlayer, now())
            .expect("resolve");

        let after = cache.load_state(CLIP_ID).expect("state").expect("present");
        assert_eq!(after, before, "machine play must not rewrite clip state");
    }

    /// human 模式先把已校验路径交给播放器，且占用 marker 在播放期间存在。
    #[test]
    fn human_mode_holds_a_play_marker_while_the_player_runs() {
        let home = TempDir::new().expect("home");
        let (store, audio_path) = accepted_clip(&home);
        let cache = ClipCache::new(store.clone());
        let (player, calls) = FakePlayer::new();

        let (outcome, guard) = play_clip(play_request(store, PlayMode::Human), &player, now())
            .expect("play");

        assert!(outcome.played);
        assert_eq!(calls.lock().expect("calls").as_slice(), &[audio_path]);
        let marker = cache
            .usage_marker_path(CLIP_ID, ClipUseKind::Playback)
            .expect("marker path");
        assert!(
            marker.exists(),
            "the guard must hold locks/<clip_id>.play while the player has the audio"
        );
        drop(guard);
        assert!(
            !marker.exists(),
            "the marker must be released after playback"
        );
    }

    /// 播放刷新 recency，但不改变逻辑 clip，也不创建 Speech Attempt。
    #[test]
    fn playback_updates_recency_without_changing_the_clip_or_creating_an_attempt() {
        let home = TempDir::new().expect("home");
        let (store, _) = accepted_clip(&home);
        let cache = ClipCache::new(store.clone());
        let before = cache.load_state(CLIP_ID).expect("state").expect("present");
        let (player, _calls) = FakePlayer::new();

        let _ = play_clip(play_request(store, PlayMode::Human), &player, now()).expect("play");

        let after = cache.load_state(CLIP_ID).expect("state").expect("present");
        assert_ne!(
            after.last_used_at, before.last_used_at,
            "playback must refresh last_used_at"
        );
        assert_eq!(after.current_audio_sha256, before.current_audio_sha256);
        assert_eq!(after.current_cache_status, before.current_cache_status);
        assert_eq!(after.latest_attempt_id, before.latest_attempt_id);
        assert_eq!(after.generation_blocked, before.generation_blocked);
        assert_eq!(after.clip_id, before.clip_id);
        // 播放不写 attempt history。
        assert!(
            cache.load_attempt("attempt-1").expect("attempt").is_none(),
            "playback must not create or finalize an attempt record"
        );
    }

    /// 校验失败时播放器永远不会被调用：损坏的音频不会被播放。
    #[test]
    fn a_tampered_cache_entry_is_rejected_before_the_player_runs() {
        let home = TempDir::new().expect("home");
        let (store, audio_path) = accepted_clip(&home);
        let mut bytes = std::fs::read(&audio_path).expect("audio");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        std::fs::write(&audio_path, &bytes).expect("tamper");
        let (player, calls) = FakePlayer::new();

        let error = play_clip(play_request(store, PlayMode::Human), &player, now())
            .expect_err("corrupt entry must not be played");

        assert_eq!(error.machine_code(), "SPEECH_CACHE_CORRUPT");
        assert!(
            calls.lock().expect("calls").is_empty(),
            "a corrupt clip must never reach the player"
        );
    }

    /// 找不到 clip 时是稳定错误，且不启动播放器。
    #[test]
    fn a_missing_clip_is_a_stable_not_found_error() {
        let home = TempDir::new().expect("home");
        let store = SpeechStore::from_home(home.path());
        let (player, calls) = FakePlayer::new();

        let error =
            play_clip(play_request(store.clone(), PlayMode::Human), &player, now())
                .expect_err("absent clip");

        assert_eq!(error.machine_code(), "SPEECH_CLIP_NOT_FOUND");
        assert!(calls.lock().expect("calls").is_empty());
        // 播放失败不得留下任何缓存状态。
        assert!(ClipCache::new(store)
            .load_state(CLIP_ID)
            .expect("state")
            .is_none());
    }

    /// 形状不对的 clip ID 是参数错误，不是「缓存损坏」。
    #[test]
    fn a_malformed_clip_id_is_rejected_before_touching_the_cache() {
        let home = TempDir::new().expect("home");
        let store = SpeechStore::from_home(home.path());
        let (player, calls) = FakePlayer::new();
        let mut request = play_request(store, PlayMode::Machine);
        request.clip_id = "../escape".to_string();

        let error = play_clip(request, &player, now()).expect_err("malformed clip id");

        assert_eq!(error.machine_code(), "INVALID_ARGUMENT");
        assert!(calls.lock().expect("calls").is_empty());
    }

    /// 播放失败只是播放失败：不触发生成、修复或任何 provider 调用。
    #[test]
    fn a_player_failure_does_not_repair_or_regenerate_anything() {
        let home = TempDir::new().expect("home");
        let (store, audio_path) = accepted_clip(&home);
        let cache = ClipCache::new(store.clone());
        let versions_before = std::fs::read_dir(audio_path.parent().expect("version dir"))
            .expect("versions")
            .count();
        let state_before = cache.load_state(CLIP_ID).expect("state").expect("present");
        let (player, _calls) = FakePlayer::failing("afplay exited with status 1");

        let error = play_clip(play_request(store, PlayMode::Human), &player, now())
            .expect_err("player failure");

        assert_eq!(error.machine_code(), "SPEECH_PLAYBACK_FAILED");
        // 音频字节仍然是原样，缓存没有被隔离或重写。
        assert_eq!(std::fs::read(&audio_path).expect("audio"), silent_mp3());
        let state_after = cache.load_state(CLIP_ID).expect("state").expect("present");
        assert_eq!(state_after.current_audio_sha256, state_before.current_audio_sha256);
        assert_eq!(state_after.current_cache_status, state_before.current_cache_status);
        assert!(
            cache.load_attempt("attempt-1").expect("attempt").is_none(),
            "a playback failure must not create a Speech Attempt"
        );
        assert_eq!(
            std::fs::read_dir(audio_path.parent().expect("version dir"))
                .expect("versions")
                .count(),
            versions_before
        );
    }

    /// 已被占用时不再叠加第二个播放：占用语义由同一个 guard 决定。
    #[test]
    fn a_clip_held_for_playback_is_not_played_twice() {
        let home = TempDir::new().expect("home");
        let (store, _) = accepted_clip(&home);
        let cache = ClipCache::new(store.clone());
        let first =
            ClipUseGuard::acquire(&cache, CLIP_ID, ClipUseKind::Playback, now()).expect("guard");
        let (player, calls) = FakePlayer::new();

        let error =
            play_clip(play_request(store, PlayMode::Human), &player, now()).expect_err("held");

        assert_eq!(error.machine_code(), "SPEECH_IN_PROGRESS");
        assert!(calls.lock().expect("calls").is_empty());
        drop(first);
    }

    /// machine 模式在已有播放占用时仍然只读解析，不受占用影响。
    #[test]
    fn machine_mode_resolves_while_another_process_is_playing() {
        let home = TempDir::new().expect("home");
        let (store, audio_path) = accepted_clip(&home);
        let cache = ClipCache::new(store.clone());
        let _guard =
            ClipUseGuard::acquire(&cache, CLIP_ID, ClipUseKind::Playback, now()).expect("guard");

        let (outcome, _guard) =
            play_clip(play_request(store, PlayMode::Machine), &NeverPlayer, now())
                .expect("resolve");

        assert_eq!(outcome.audio_path, audio_path);
    }

    /// 负向控制：占用的判定真的来自 marker，而不是 guard 永远成功。
    #[test]
    fn a_play_marker_blocks_the_occupancy_check() {
        let home = TempDir::new().expect("home");
        let (store, _) = accepted_clip(&home);
        let cache = ClipCache::new(store.clone());
        let guard =
            ClipUseGuard::acquire(&cache, CLIP_ID, ClipUseKind::Playback, now()).expect("guard");

        assert_eq!(
            cache.clip_in_use(CLIP_ID),
            Some(ClipUseKind::Playback),
            "the in-use check must observe the play marker this module writes"
        );
        drop(guard);
    }

    /// 播放器调用次数可观测：human 恰好一次，machine 零次。
    #[test]
    fn the_player_is_invoked_exactly_once_in_human_mode() {
        let home = TempDir::new().expect("home");
        let (store, _) = accepted_clip(&home);
        let calls = Arc::new(AtomicUsize::new(0));
        let player = CountingPlayer {
            calls: Arc::clone(&calls),
        };

        let _ = play_clip(play_request(store, PlayMode::Human), &player, now()).expect("play");

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
