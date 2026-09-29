//! `speech export`：把一个已接受、已校验的 Cached Speech Clip 原子复制到书籍导出目录，
//! 并原子替换 Speech Export Manifest（实施 spec 5.6 / §10 / ADR 0007）。
//!
//! 本模块**故意不依赖** `senseaudio` 客户端与 `generate`，与 [`crate::speech::play`]
//! 同样的结构性论证：导出路径上没有传输能力，因此「导出不联网」不是靠约定，而是
//! 没有任何可以发起 provider 调用的依赖。
//!
//! 三条顺序不变量（ADR 0007「中断可以留下未被 manifest 引用的孤立音频，但不能提交
//! 指向缺失文件的 active 记录」）：
//!
//! 1. 音频先写入同一文件系统内的暂存文件，读回校验 checksum 后才 rename 到最终路径；
//! 2. 最终文件就位之后，才原子替换 manifest；
//! 3. manifest 一旦提交，永远指向已经存在且 checksum 匹配的文件。
//!
//! `speech export` 从不修改已有 Markdown。音频链接由常规
//! Markdown/Obsidian 导出（[`crate::exporter`]）在另一次独立动作里读取 manifest 产生。

use crate::speech::cache::{is_valid_clip_id, ClipCache, ClipCacheError, ClipLockError, ClipUseGuard, ClipUseKind, ReadyClip};
use crate::speech::clip::SpeechContentKind;
use crate::speech::store::{SpeechStore, SpeechStoreError};
use crate::speech::text::sha256_hex;
use crate::speech::SpeechWarning;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

/// Speech Export Manifest 的 schema 版本。
pub const SPEECH_EXPORT_MANIFEST_SCHEMA_VERSION: u32 = 1;

/// 非权威 export locator 投影的 schema 版本。
pub const EXPORT_LOCATOR_SCHEMA_VERSION: u32 = 1;

/// manifest 在书籍导出目录中的固定位置。
pub const AUDIO_SUBDIRECTORY: &str = "assets/audio";

/// 短 fingerprint 的默认长度：完整 clip ID 的前 12 位。
pub const SHORT_FINGERPRINT_LEN: usize = 12;

/// 导出根目录下 manifest 的绝对路径。
pub fn manifest_path(book_export_root: &Path) -> PathBuf {
    book_export_root.join(AUDIO_SUBDIRECTORY).join("manifest.json")
}

/// 一个 Annotation 的某个内容部分下已导出的所有 Speech Clip 变体。
///
/// 同一 `annotation_id + content_kind` 始终只有一个 [`Self::active_clip_id`]；
/// 旧变体继续留在 [`Self::clips`] 里，由用户拥有，应用不静默删除。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedContentRecord {
    /// Annotation 稳定 ID。
    pub annotation_id: String,
    /// 内容部分。
    pub content_kind: SpeechContentKind,
    /// 当前唯一 active 的完整 clip ID。
    pub active_clip_id: String,
    /// 该内容部分下所有已导出变体（含非 active）。
    pub clips: Vec<ExportedClipRecord>,
}

impl ExportedContentRecord {
    /// 该记录的唯一键。
    fn key(&self) -> (String, SpeechContentKind) {
        (self.annotation_id.clone(), self.content_kind)
    }
}

/// 一个已导出的 Speech Clip 变体。
///
/// 只保存稳定身份、相对路径、checksum、大小、格式和导出时间：**不保存** Speech Text、
/// API Key、绝对路径或供应商原始响应（ADR 0007「远程授权、缓存与导出」）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedClipRecord {
    /// 完整 clip ID（内容 + Voice Profile 的 SHA-256）。
    pub clip_id: String,
    /// 相对书籍导出根目录的规范化路径，例如 `assets/audio/highlight-ab12cd34ef56.mp3`。
    pub relative_path: String,
    /// 音频字节的 SHA-256。
    pub sha256: String,
    /// 音频字节数。
    pub size_bytes: u64,
    /// 音频格式；首版只接受 `mp3`。
    pub format: String,
    /// 导出时间（RFC 3339，秒精度 UTC）。
    pub exported_at: String,
}

/// `<book-export-directory>/assets/audio/manifest.json` 的内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeechExportManifest {
    /// 固定为 [`SPEECH_EXPORT_MANIFEST_SCHEMA_VERSION`]。
    pub schema_version: u32,
    /// 必须与正在导出的书一致。
    pub asset_id: String,
    /// 按 Annotation + 内容部分组织的记录。
    pub records: Vec<ExportedContentRecord>,
}

impl SpeechExportManifest {
    /// 一个空 manifest：还没有任何导出过的语音。
    pub fn empty(asset_id: &str) -> Self {
        Self {
            schema_version: SPEECH_EXPORT_MANIFEST_SCHEMA_VERSION,
            asset_id: asset_id.to_string(),
            records: Vec::new(),
        }
    }

    /// 读取并严格校验 manifest。
    ///
    /// 文件不存在返回 `Ok(None)`（还没有导出过语音）；存在但无法解析、schema 版本不认识
    /// 或结构非法时返回 [`ExportError::ManifestInvalid`]——**不覆盖、不猜测、不按文件名
    /// 或修改时间重建身份**。
    pub fn load(book_export_root: &Path) -> Result<Option<Self>, ExportError> {
        let path = manifest_path(book_export_root);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(ExportError::Storage(SpeechStoreError::Unavailable {
                    path,
                    message: error.to_string(),
                }))
            }
        };
        Self::parse(&text).map(Some)
    }

    /// 严格解析 manifest 文本。`deny_unknown_fields` + 显式 schema 版本检查：
    /// 任何多余或缺失的字段都让 manifest 变成「不可信」，而不是被尽力修复。
    pub fn parse(text: &str) -> Result<Self, ExportError> {
        let manifest: Self = serde_json::from_str(text).map_err(|error| ExportError::ManifestInvalid {
            reason: "the Speech Export Manifest is not valid JSON for schema version 1",
            detail: error.to_string(),
        })?;
        if manifest.schema_version != SPEECH_EXPORT_MANIFEST_SCHEMA_VERSION {
            return Err(ExportError::ManifestInvalid {
                reason: "the Speech Export Manifest declares an unsupported schema version",
                detail: format!(
                    "schema_version={} is not supported",
                    manifest.schema_version
                ),
            });
        }
        if manifest.asset_id.is_empty() {
            return Err(ExportError::ManifestInvalid {
                reason: "the Speech Export Manifest has no asset id",
                detail: "asset_id is empty".to_string(),
            });
        }
        for record in &manifest.records {
            if !record.active_clip_id.is_empty()
                && !record.clips.iter().any(|clip| clip.clip_id == record.active_clip_id)
            {
                return Err(ExportError::ManifestInvalid {
                    reason: "the Speech Export Manifest points at a clip it does not record",
                    detail: format!(
                        "annotation={} content_kind={} active_clip_id={}",
                        record.annotation_id,
                        record.content_kind.as_str(),
                        record.active_clip_id
                    ),
                });
            }
        }
        Ok(manifest)
    }

    /// 找到指定 Annotation + 内容部分的记录。
    fn record(&self, annotation_id: &str, content_kind: SpeechContentKind) -> Option<&ExportedContentRecord> {
        self.records
            .iter()
            .find(|record| record.annotation_id == annotation_id && record.content_kind == content_kind)
    }

    /// 某个 clip ID 已被记录的相对路径（无论它是否 active）。
    fn path_of_clip(&self, clip_id: &str) -> Option<&str> {
        self.records
            .iter()
            .flat_map(|record| record.clips.iter())
            .find(|clip| clip.clip_id == clip_id)
            .map(|clip| clip.relative_path.as_str())
    }

    /// 某个相对路径当前属于哪个 clip ID。
    fn owner_of_path(&self, relative_path: &str) -> Option<&str> {
        self.records
            .iter()
            .flat_map(|record| record.clips.iter())
            .find(|clip| clip.relative_path == relative_path)
            .map(|clip| clip.clip_id.as_str())
    }
}

/// `speech export` 的失败。全部发生在任何 provider 调用之前。
#[derive(Debug)]
pub enum ExportError {
    /// Speech 状态根或缓存不可用。
    Storage(SpeechStoreError),
    /// clip ID 不是 64 位小写 sha256 hex。
    InvalidClipId(String),
    /// 缓存里没有这个 clip 的有效音频。
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
    /// manifest 不可信：不覆盖、不猜测重建。
    ManifestInvalid {
        /// 稳定原因。
        reason: &'static str,
        /// 供人类诊断的细节，不含原文或密钥。
        detail: String,
    },
    /// 目标路径已有**不同内容**的音频。
    OutputFileExists {
        /// 冲突文件的绝对路径。
        path: PathBuf,
        /// 冲突文件的相对路径。
        relative_path: String,
    },
    /// 另一个进程正在播放或导出同一 clip。
    ClipInUse,
}

impl ExportError {
    /// 稳定的 Machine JSON 错误码。
    pub const fn machine_code(&self) -> &'static str {
        match self {
            Self::Storage(..) => "SPEECH_STORAGE_UNAVAILABLE",
            Self::InvalidClipId(..) => "INVALID_ARGUMENT",
            Self::ClipNotFound { .. } => "SPEECH_CLIP_NOT_FOUND",
            Self::CacheCorrupt { .. } => "SPEECH_CACHE_CORRUPT",
            Self::ManifestInvalid { .. } => "SPEECH_EXPORT_MANIFEST_INVALID",
            Self::OutputFileExists { .. } => "SPEECH_OUTPUT_FILE_EXISTS",
            Self::ClipInUse => "SPEECH_IN_PROGRESS",
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
                "no verified Cached Speech Clip is available for {clip_id}; run `speech generate` first (export never calls the Speech Provider)"
            ),
            Self::CacheCorrupt { path, reason } => format!(
                "the Speech Cache Entry is corrupt at {}: {reason}; run `speech generate --regenerate` to replace it",
                path.display()
            ),
            Self::ManifestInvalid { reason, detail } => {
                format!("{reason}: {detail}; refusing to overwrite or rebuild it")
            }
            Self::OutputFileExists { relative_path, .. } => format!(
                "{relative_path} already exists with different content; re-run with --overwrite to replace exactly that file"
            ),
            Self::ClipInUse => {
                "another playback or export of this Speech Clip is still in progress".to_string()
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
                "run `speech generate` for the same content; `speech export` never contacts the Speech Provider"
            }
            Self::CacheCorrupt { .. } => {
                "run `speech generate --regenerate` to replace the corrupt entry with a newly accepted version"
            }
            Self::ManifestInvalid { .. } => {
                "inspect or move the existing assets/audio/manifest.json yourself; this tool will not rewrite identity it cannot trust"
            }
            Self::OutputFileExists { .. } => {
                "re-run `speech export --overwrite` to replace that one file, or choose a different export directory"
            }
            Self::ClipInUse => "wait for the current playback or export to finish and try again",
        }
    }
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl From<ClipCacheError> for ExportError {
    fn from(error: ClipCacheError) -> Self {
        match error {
            ClipCacheError::Unavailable { path, message } => {
                Self::Storage(SpeechStoreError::Unavailable { path, message })
            }
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

/// 一次 `speech export` 请求。
#[derive(Debug, Clone)]
pub struct ExportRequest {
    /// 用户级 Speech 状态根。
    pub store: SpeechStore,
    /// 完整 clip ID。
    pub clip_id: String,
    /// 已选择书籍的导出根目录（**不是** `assets/audio/` 本身）。
    pub book_export_root: PathBuf,
    /// 是否允许替换同一路径上内容不同的音频。
    pub overwrite: bool,
}

/// `speech export` 的结果。只含稳定身份与已校验的本地音频事实。
#[derive(Debug, Clone)]
pub struct ExportOutcome {
    /// 完整 clip ID。
    pub clip_id: String,
    /// 书籍稳定 ID。
    pub asset_id: String,
    /// Annotation 稳定 ID。
    pub annotation_id: String,
    /// 内容部分。
    pub content_kind: SpeechContentKind,
    /// 相对书籍导出根目录的路径。
    pub relative_path: String,
    /// 已校验音频的绝对路径。
    pub audio_path: PathBuf,
    /// 音频字节的 SHA-256。
    pub audio_sha256: String,
    /// 音频字节数。
    pub audio_size_bytes: u64,
    /// 音频格式。
    pub format: String,
    /// 导出时间。
    pub exported_at: String,
    /// 该内容部分当前唯一 active 的 clip ID。
    pub active_clip_id: String,
    /// 目标文件字节完全一致，本次直接复用、没有重写。
    pub reused: bool,
    /// 本次是否显式替换了内容不同的已有文件。
    pub replaced: bool,
    /// 非致命 warning（例如非权威 locator 投影更新失败）。
    pub warnings: Vec<SpeechWarning>,
}

/// `speech export` use case。
///
/// 顺序固定且不联网：校验标识形状 → 占用 `ClipUseGuard`（`locks/<clip_id>.export`）→
/// 解析并校验缓存 entry → 严格读取 manifest → 选择 collision-safe 短 fingerprint 路径 →
/// 暂存写入 + 读回校验 + rename 放置音频 → 原子替换 manifest → 释放 marker。
///
/// 全程持有 [`ClipUseGuard`]，因此 LRU 维护与 `speech cache clear` 不会在音频被读取时
/// 抽走 entry。这也补上了 #25 只能靠注入 `locks/<clip_id>.export` 假装证明的缺口：
/// 真实的 `speech export` 现在确实通过同一个 guard 取用该 marker。
pub fn export_clip(
    request: ExportRequest,
    now: DateTime<Utc>,
) -> Result<(ExportOutcome, ClipUseGuard), ExportError> {
    if !is_valid_clip_id(&request.clip_id) {
        return Err(ExportError::InvalidClipId(request.clip_id));
    }
    let cache = ClipCache::new(request.store.clone());

    // 先占用再解析：读取与复制期间 entry 都不能被淘汰或清除。
    let guard = ClipUseGuard::acquire(&cache, &request.clip_id, ClipUseKind::Export, now).map_err(
        |error| match error {
            ClipLockError::InProgress => ExportError::ClipInUse,
            other => ExportError::Storage(SpeechStoreError::Unavailable {
                path: PathBuf::new(),
                message: other.message(),
            }),
        },
    )?;

    let outcome = run_export(&cache, &request, now)?;
    // 导出算一次「最近使用过」（LRU 的 U）；只改 recency，不改逻辑 clip。
    cache.touch_clip(&request.clip_id, now);
    Ok((outcome, guard))
}

fn run_export(
    cache: &ClipCache,
    request: &ExportRequest,
    now: DateTime<Utc>,
) -> Result<ExportOutcome, ExportError> {
    let ready = cache
        .load_ready_clip(&request.clip_id)?
        .ok_or_else(|| ExportError::ClipNotFound {
            clip_id: request.clip_id.clone(),
        })?;
    let source = read_verified_bytes(&ready)?;
    let asset_id = ready.state.asset_id.clone();
    let annotation_id = ready.state.annotation_id.clone();
    let content_kind = ready.metadata.content_kind;

    let root = &request.book_export_root;
    fs::create_dir_all(root.join(AUDIO_SUBDIRECTORY)).map_err(|error| {
        ExportError::Storage(SpeechStoreError::Unavailable {
            path: root.to_path_buf(),
            message: error.to_string(),
        })
    })?;

    let mut manifest = SpeechExportManifest::load(root)?.unwrap_or_else(|| SpeechExportManifest::empty(&asset_id));
    if manifest.asset_id != asset_id {
        return Err(ExportError::ManifestInvalid {
            reason: "the Speech Export Manifest belongs to a different book",
            detail: format!(
                "manifest asset_id={} does not match clip asset_id={asset_id}",
                manifest.asset_id
            ),
        });
    }

    let (relative_path, reused, replaced) =
        place_audio(root, &mut manifest, request, &ready, &source)?;

    let record = ExportedClipRecord {
        clip_id: request.clip_id.clone(),
        relative_path: relative_path.clone(),
        sha256: source.sha256.clone(),
        size_bytes: source.bytes.len() as u64,
        format: "mp3".to_string(),
        exported_at: format_timestamp(now),
    };
    apply_to_manifest(&mut manifest, record, &annotation_id, content_kind)?;

    // 音频已经就位并通过校验，现在才原子替换 manifest。
    commit_manifest(root, &manifest)?;

    let active_clip_id = manifest
        .record(&annotation_id, content_kind)
        .map(|record| record.active_clip_id.clone())
        .unwrap_or_else(|| request.clip_id.clone());
    let audio_path = resolve_contained_path(root, &relative_path)
        .map_err(|_| ExportError::Storage(SpeechStoreError::Unavailable {
            path: root.to_path_buf(),
            message: "the just written export path is not contained in the export root".to_string(),
        }))?;

    // 非权威 locator 投影：更新失败只返回 warning，绝不回滚已经自洽提交的导出目录。
    let mut warnings = Vec::new();
    if let Err(reason) = update_locator(cache, root, &manifest) {
        warnings.push(SpeechWarning {
            code: SpeechWarning::EXPORT_LOCATOR_STALE_CODE,
            reason: "export_locator_not_updated",
            message: format!(
                "the Speech Export Manifest was committed, but the non-authoritative export locator projection could not be updated ({reason}); `speech play` can still find this clip with an explicit --export-root"
            ),
        });
    }

    Ok(ExportOutcome {
        clip_id: request.clip_id.clone(),
        asset_id,
        annotation_id,
        content_kind,
        relative_path,
        audio_path,
        audio_sha256: source.sha256,
        audio_size_bytes: source.bytes.len() as u64,
        format: "mp3".to_string(),
        exported_at: format_timestamp(now),
        active_clip_id,
        reused,
        replaced,
        warnings,
    })
}

/// 一次「读缓存 + 校验」的产物。
struct VerifiedSource {
    bytes: Vec<u8>,
    sha256: String,
}

/// 读取并复核缓存音频字节：大小、checksum 与 metadata 必须一致。
///
/// 复制到导出目录的是**已经校验过的那份字节**，因此 manifest 记录的 checksum 一定对应
/// 落盘的真实内容。
fn read_verified_bytes(ready: &ReadyClip) -> Result<VerifiedSource, ExportError> {
    let bytes = fs::read(&ready.audio_path).map_err(|error| ExportError::CacheCorrupt {
        path: ready.audio_path.clone(),
        reason: format!("the cached audio could not be read: {error}"),
    })?;
    let sha256 = sha256_hex(&bytes);
    if bytes.is_empty()
        || sha256 != ready.metadata.audio_sha256
        || bytes.len() as u64 != ready.metadata.size_bytes
    {
        return Err(ExportError::CacheCorrupt {
            path: ready.audio_path.clone(),
            reason: "the cached audio and its metadata disagree".to_string(),
        });
    }
    Ok(VerifiedSource { bytes, sha256 })
}

/// 把音频放到最终路径上，返回 `(相对路径, 是否复用, 是否替换)`。
///
/// 三种情况：
///
/// - 该 clip 已经在 manifest 里且文件字节一致 → 直接复用，不重写；
/// - 目标路径被**另一个** clip 占用，或内容不同且未显式 `--overwrite` → 拒绝或换更长的
///   短 fingerprint，绝不覆盖别的 clip 的音频；
/// - 其余情况 → 暂存写入、读回校验、rename 放置。
fn place_audio(
    root: &Path,
    manifest: &SpeechExportManifest,
    request: &ExportRequest,
    ready: &ReadyClip,
    source: &VerifiedSource,
) -> Result<(String, bool, bool), ExportError> {
    // 稳定优先：这个 clip 之前导出过的路径就是它的规范路径。
    if let Some(existing) = manifest.path_of_clip(&request.clip_id) {
        let existing = existing.to_string();
        let path = resolve_contained_path(root, &existing)?;
        if let Some(current) = read_if_same_content(&path, &source.sha256)? {
            let _ = current;
            return Ok((existing, true, false));
        }
        // 记录在案但文件缺失或被用户改过：只有显式 --overwrite 才恢复它。
        if !request.overwrite {
            return Err(ExportError::OutputFileExists {
                path,
                relative_path: existing,
            });
        }
        write_final(root, &existing, &source.bytes)?;
        return Ok((existing, false, true));
    }

    let mut prefix_len = SHORT_FINGERPRINT_LEN;
    loop {
        let candidate = relative_path_for(ready.metadata.content_kind, &request.clip_id, prefix_len);
        // 路径被别的 clip 记录在案 → 绝不覆盖（即使带 --overwrite）。
        let claimed = manifest.owner_of_path(&candidate).is_some_and(|owner| owner != request.clip_id);
        if !claimed {
            let path = resolve_contained_path(root, &candidate)?;
            match read_if_same_content(&path, &source.sha256)? {
                // 已存在的同字节文件（孤立或旧 manifest 之外）：直接采纳，不重写。
                Some(_) => return Ok((candidate, true, false)),
                None => {
                    if path.exists() && !request.overwrite {
                        return Err(ExportError::OutputFileExists {
                            path,
                            relative_path: candidate,
                        });
                    }
                    let replaced = path.exists();
                    write_final(root, &candidate, &source.bytes)?;
                    return Ok((candidate, false, replaced));
                }
            }
        }
        if prefix_len >= request.clip_id.len() {
            // 完整 clip ID 仍然冲突：文件确实属于别人，只能拒绝。
            return Err(ExportError::OutputFileExists {
                path: resolve_contained_path(root, &candidate)?,
                relative_path: candidate,
            });
        }
        prefix_len += 1;
    }
}

/// 已存在且字节 checksum 与源一致时返回 `Ok(Some(()))`；不存在返回 `Ok(None)`。
fn read_if_same_content(path: &Path, sha256: &str) -> Result<Option<()>, ExportError> {
    match fs::read(path) {
        Ok(bytes) if sha256_hex(&bytes) == sha256 => Ok(Some(())),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ExportError::Storage(SpeechStoreError::Unavailable {
            path: path.to_path_buf(),
            message: error.to_string(),
        })),
    }
}

/// 暂存写入 → 读回校验 checksum → rename 放置。
///
/// 校验发生在 rename 之前，因此最终路径上出现的一定是已经验证过的字节；中断最多留下
/// 一个暂存文件或一个未被 manifest 引用的孤立音频，不会提交指向缺失文件的 active 记录。
fn write_final(root: &Path, relative_path: &str, bytes: &[u8]) -> Result<(), ExportError> {
    let final_path = resolve_contained_path(root, relative_path)?;
    let staging = final_path.with_file_name(format!(
        ".staging-{}-{}",
        std::process::id(),
        final_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let result = (|| -> Result<(), ExportError> {
        let mut file = fs::File::create(&staging).map_err(|error| {
            ExportError::Storage(SpeechStoreError::Unavailable {
                path: staging.clone(),
                message: error.to_string(),
            })
        })?;
        file.write_all(bytes).map_err(|error| {
            ExportError::Storage(SpeechStoreError::Unavailable {
                path: staging.clone(),
                message: error.to_string(),
            })
        })?;
        file.sync_all().map_err(|error| {
            ExportError::Storage(SpeechStoreError::Unavailable {
                path: staging.clone(),
                message: error.to_string(),
            })
        })?;
        drop(file);
        // 读回校验：rename 之前确认落盘字节的 checksum。
        let staged = fs::read(&staging).map_err(|error| {
            ExportError::Storage(SpeechStoreError::Unavailable {
                path: staging.clone(),
                message: error.to_string(),
            })
        })?;
        if sha256_hex(&staged) != sha256_hex(bytes) {
            return Err(ExportError::CacheCorrupt {
                path: staging.clone(),
                reason: "the staged export audio did not survive verification".to_string(),
            });
        }
        fs::rename(&staging, &final_path).map_err(|error| {
            ExportError::Storage(SpeechStoreError::Unavailable {
                path: final_path.clone(),
                message: error.to_string(),
            })
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staging);
    }
    result
}

/// 把 clip 记录写进 manifest，并把该内容部分的 active 指向它。
///
/// 旧变体继续留在 `clips` 里：不删除音频文件、不从记录中移除，也不取消用户的文件。
fn apply_to_manifest(
    manifest: &mut SpeechExportManifest,
    record: ExportedClipRecord,
    annotation_id: &str,
    content_kind: SpeechContentKind,
) -> Result<(), ExportError> {
    match manifest
        .records
        .iter_mut()
        .find(|existing| existing.key() == (annotation_id.to_string(), content_kind))
    {
        Some(existing) => {
            match existing
                .clips
                .iter_mut()
                .find(|clip| clip.clip_id == record.clip_id)
            {
                Some(clip) => *clip = record,
                None => existing.clips.push(record),
            }
            // 新导出的变体成为 active；旧变体保持用户所有，只是不再被自动链接。
            existing.active_clip_id = clip_id_of_last(&existing.clips);
        }
        None => {
            let active_clip_id = record.clip_id.clone();
            manifest.records.push(ExportedContentRecord {
                annotation_id: annotation_id.to_string(),
                content_kind,
                active_clip_id,
                clips: vec![record],
            });
        }
    }
    Ok(())
}

fn clip_id_of_last(clips: &[ExportedClipRecord]) -> String {
    clips
        .last()
        .map(|clip| clip.clip_id.clone())
        .unwrap_or_default()
}

/// 原子替换 manifest：同目录暂存 + fsync + rename。
fn commit_manifest(root: &Path, manifest: &SpeechExportManifest) -> Result<(), ExportError> {
    let path = manifest_path(root);
    let mut json = serde_json::to_string_pretty(manifest).map_err(|error| {
        ExportError::Storage(SpeechStoreError::Unavailable {
            path: path.clone(),
            message: error.to_string(),
        })
    })?;
    json.push('\n');
    let bytes = json.as_bytes();
    let staging = path.with_file_name("manifest.json.staging");
    let result = (|| -> Result<(), ExportError> {
        let mut file = fs::File::create(&staging).map_err(|error| {
            ExportError::Storage(SpeechStoreError::Unavailable {
                path: staging.clone(),
                message: error.to_string(),
            })
        })?;
        file.write_all(bytes).map_err(|error| {
            ExportError::Storage(SpeechStoreError::Unavailable {
                path: staging.clone(),
                message: error.to_string(),
            })
        })?;
        file.sync_all().map_err(|error| {
            ExportError::Storage(SpeechStoreError::Unavailable {
                path: staging.clone(),
                message: error.to_string(),
            })
        })?;
        drop(file);
        fs::rename(&staging, &path).map_err(|error| {
            ExportError::Storage(SpeechStoreError::Unavailable {
                path: path.clone(),
                message: error.to_string(),
            })
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staging);
    }
    result
}

/// 短 fingerprint 文件名：`<content_kind>-<clip id 前缀>.mp3`。
///
/// 内容部分来自已解析的 [`SpeechContentKind`]（`highlight` / `note`），因此同一条
/// Annotation 的高亮与笔记永远得到不同前缀的文件名。
pub fn relative_path_for(
    content_kind: SpeechContentKind,
    clip_id: &str,
    prefix_len: usize,
) -> String {
    let prefix = clip_id
        .get(..prefix_len.min(clip_id.len()))
        .unwrap_or(clip_id);
    format!("{}/{}-{prefix}.mp3", AUDIO_SUBDIRECTORY, content_kind.as_str())
}

/// 把 manifest 里的相对路径解析成导出根目录内的绝对路径。
///
/// 这是安全边界：manifest 是磁盘上的输入，因此必须防住 `..`、绝对路径和 symlink。
/// 规则是逐段检查——只接受普通文件名段，拒绝绝对路径、`.`/`..` 段，并且路径上**任何**
/// 一段是符号链接就整体拒绝（不做「解析后再看是否还在根内」这种跟随链接的检查）。
/// 解析完成后仍然规范化一次根目录，确认结果确实在根内。
pub fn resolve_contained_path(root: &Path, relative_path: &str) -> Result<PathBuf, ExportError> {
    if relative_path.is_empty() {
        return Err(ExportError::ManifestInvalid {
            reason: "the Speech Export Manifest has an empty relative path",
            detail: "relative_path is empty".to_string(),
        });
    }
    if relative_path.contains('\0') {
        return Err(ExportError::ManifestInvalid {
            reason: "the Speech Export Manifest has an invalid relative path",
            detail: "relative_path contains a NUL byte".to_string(),
        });
    }
    let mut current = root.to_path_buf();
    for component in Path::new(relative_path).components() {
        match component {
            Component::Normal(segment) => {
                if segment == ".." || segment == "." {
                    return Err(ExportError::ManifestInvalid {
                        reason: "the Speech Export Manifest escapes the export root",
                        detail: format!("relative_path={relative_path} contains a path traversal segment"),
                    });
                }
                current.push(segment);
                // 逐段拒绝 symlink：manifest 不得通过链接把读或写带出导出根。
                match fs::symlink_metadata(&current) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        return Err(ExportError::ManifestInvalid {
                            reason: "the Speech Export Manifest path crosses a symbolic link",
                            detail: format!(
                                "relative_path={relative_path} traverses a symlink at {}",
                                current.display()
                            ),
                        })
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(ExportError::Storage(SpeechStoreError::Unavailable {
                            path: current,
                            message: error.to_string(),
                        }))
                    }
                }
            }
            // `..`、绝对根、前缀盘符全部拒绝。
            Component::ParentDir | Component::RootDir | Component::Prefix(..) => {
                return Err(ExportError::ManifestInvalid {
                    reason: "the Speech Export Manifest escapes the export root",
                    detail: format!("relative_path={relative_path} is not a plain relative path"),
                })
            }
            Component::CurDir => {
                return Err(ExportError::ManifestInvalid {
                    reason: "the Speech Export Manifest path is not normalized",
                    detail: format!("relative_path={relative_path} contains a '.' segment"),
                })
            }
        }
    }
    Ok(current)
}

/// 常规 Markdown/Obsidian 导出为某个 Annotation 内容部分解析出的相对音频链接。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAudioLink {
    /// 完整 clip ID。
    pub clip_id: String,
    /// 内容部分。
    pub content_kind: SpeechContentKind,
    /// 相对**主 Markdown 文件所在目录**的路径，可直接写进 Markdown。
    pub relative_path: String,
}

/// 书籍导出读取 Speech Export Manifest 的结果。
#[derive(Debug, Clone, Default)]
pub struct ExportLinkReport {
    /// `(annotation_id, content_kind) -> 链接`，只包含已经通过校验的 active 变体。
    pub links: BTreeMap<(String, SpeechContentKind), ResolvedAudioLink>,
    /// 被省略的链接与原因；**不会**让整本阅读笔记导出失败。
    pub warnings: Vec<SpeechWarning>,
}

/// 解析一本书的 active 音频链接，供常规 Markdown/Obsidian 导出使用。
///
/// 规则（ADR 0007 / 实施 spec §10）：
///
/// - 只链接每个 `annotation_id + content_kind` 的 active 变体；
/// - 文件缺失、checksum 不匹配、格式不符、路径逃出导出根、manifest 损坏时**省略链接并
///   返回结构化 warning**，不修复文件、不修改 manifest、不调用 provider；
/// - 没有导出语音时不写占位链接。
pub fn resolve_export_links(
    book_export_root: &Path,
    asset_id: &str,
) -> ExportLinkReport {
    let mut report = ExportLinkReport::default();
    let manifest = match SpeechExportManifest::load(book_export_root) {
        Ok(Some(manifest)) => manifest,
        Ok(None) => return report,
        Err(error) => {
            report.warnings.push(SpeechWarning {
                code: SpeechWarning::EXPORT_MANIFEST_UNUSABLE_CODE,
                reason: "manifest_unusable",
                message: format!(
                    "the Speech Export Manifest was skipped and no audio link was written: {}",
                    error.message()
                ),
            });
            return report;
        }
    };
    if manifest.asset_id != asset_id {
        report.warnings.push(SpeechWarning {
            code: SpeechWarning::EXPORT_MANIFEST_UNUSABLE_CODE,
            reason: "manifest_asset_mismatch",
            message: format!(
                "the Speech Export Manifest in this export directory belongs to asset_id {} rather than {asset_id}, so no audio link was written",
                manifest.asset_id
            ),
        });
        return report;
    }

    for record in &manifest.records {
        let Some(active) = record
            .clips
            .iter()
            .find(|clip| clip.clip_id == record.active_clip_id)
        else {
            report.warnings.push(omitted(
                &record.annotation_id,
                record.content_kind,
                "active_clip_not_recorded",
                "the active clip is not present in the manifest record",
            ));
            continue;
        };
        let path = match resolve_contained_path(book_export_root, &active.relative_path) {
            Ok(path) => path,
            Err(_) => {
                report.warnings.push(omitted(
                    &record.annotation_id,
                    record.content_kind,
                    "path_outside_export_root",
                    "the recorded relative path does not stay inside the export root",
                ));
                continue;
            }
        };
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => {
                report.warnings.push(omitted(
                    &record.annotation_id,
                    record.content_kind,
                    "audio_missing",
                    "the exported audio file is missing",
                ));
                continue;
            }
        };
        if bytes.len() as u64 != active.size_bytes || sha256_hex(&bytes) != active.sha256 {
            report.warnings.push(omitted(
                &record.annotation_id,
                record.content_kind,
                "checksum_mismatch",
                "the exported audio file no longer matches the checksum recorded in the manifest",
            ));
            continue;
        }
        if active.format != "mp3" {
            report.warnings.push(omitted(
                &record.annotation_id,
                record.content_kind,
                "unsupported_format",
                "the exported audio format is not mp3",
            ));
            continue;
        }
        report.links.insert(
            (record.annotation_id.clone(), record.content_kind),
            ResolvedAudioLink {
                clip_id: active.clip_id.clone(),
                content_kind: record.content_kind,
                relative_path: active.relative_path.clone(),
            },
        );
    }
    report
}

fn omitted(
    annotation_id: &str,
    content_kind: SpeechContentKind,
    reason: &'static str,
    message: &str,
) -> SpeechWarning {
    SpeechWarning {
        code: SpeechWarning::EXPORT_AUDIO_OMITTED_CODE,
        reason,
        message: format!(
            "the audio link for annotation {annotation_id} ({}) was omitted: {message}",
            content_kind.as_str()
        ),
    }
}

/// 非权威 export locator 投影。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportLocator {
    schema_version: u32,
    entries: BTreeMap<String, ExportLocatorEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportLocatorEntry {
    /// 最近一次验证过的绝对 export root。
    export_root: String,
    /// manifest schema 版本。
    manifest_schema_version: u32,
    /// manifest 内容的 digest，用于判断是否过期。
    manifest_sha256: String,
    /// 最近一次验证时间。
    last_verified_at: String,
}

fn locator_path(cache: &ClipCache) -> PathBuf {
    cache.root().join("exports.json")
}

fn update_locator(
    cache: &ClipCache,
    book_export_root: &Path,
    manifest: &SpeechExportManifest,
) -> Result<(), String> {
    let path = locator_path(cache);
    let mut locator = match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str::<ExportLocator>(&text).unwrap_or(ExportLocator {
            schema_version: EXPORT_LOCATOR_SCHEMA_VERSION,
            entries: BTreeMap::new(),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ExportLocator {
            schema_version: EXPORT_LOCATOR_SCHEMA_VERSION,
            entries: BTreeMap::new(),
        },
        Err(error) => return Err(error.to_string()),
    };
    let manifest_bytes =
        serde_json::to_vec(manifest).map_err(|error| error.to_string())?;
    locator
        .entries
        .insert(manifest.asset_id.clone(), ExportLocatorEntry {
            export_root: book_export_root.to_string_lossy().into_owned(),
            manifest_schema_version: manifest.schema_version,
            manifest_sha256: sha256_hex(&manifest_bytes),
            last_verified_at: format_timestamp(Utc::now()),
        });
    let mut json = serde_json::to_string_pretty(&locator).map_err(|error| error.to_string())?;
    json.push('\n');
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    // 投影可丢弃：写失败只影响便利查找，不影响已经提交的导出目录。
    fs::write(&path, json).map_err(|error| error.to_string())
}

fn format_timestamp(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIP_A: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";
    const CLIP_B: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90b1b2c3d4e5f60718293a4b5c6d7e8f90";

    fn manifest_with_clips() -> SpeechExportManifest {
        SpeechExportManifest {
            schema_version: SPEECH_EXPORT_MANIFEST_SCHEMA_VERSION,
            asset_id: "book-1".to_string(),
            records: vec![ExportedContentRecord {
                annotation_id: "annotation-41".to_string(),
                content_kind: SpeechContentKind::Highlight,
                active_clip_id: CLIP_A.to_string(),
                clips: vec![ExportedClipRecord {
                    clip_id: CLIP_A.to_string(),
                    relative_path: relative_path_for(SpeechContentKind::Highlight, CLIP_A, SHORT_FINGERPRINT_LEN),
                    sha256: "c".repeat(64),
                    size_bytes: 10,
                    format: "mp3".to_string(),
                    exported_at: "2026-09-11T00:00:00Z".to_string(),
                }],
            }],
        }
    }

    /// 文件名包含内容部分，��� fingerprint 来自完整 clip ID 的前缀。
    #[test]
    fn relative_path_is_deterministic_and_carries_the_content_kind() {
        assert_eq!(
            relative_path_for(SpeechContentKind::Highlight, CLIP_A, SHORT_FINGERPRINT_LEN),
            "assets/audio/highlight-a1b2c3d4e5f6.mp3"
        );
        assert_eq!(
            relative_path_for(SpeechContentKind::Note, CLIP_A, SHORT_FINGERPRINT_LEN),
            "assets/audio/note-a1b2c3d4e5f6.mp3"
        );
        assert_eq!(
            relative_path_for(SpeechContentKind::Highlight, CLIP_A, SHORT_FINGERPRINT_LEN),
            relative_path_for(SpeechContentKind::Highlight, CLIP_A, SHORT_FINGERPRINT_LEN)
        );
    }

    /// 短 fingerprint 冲突时延长长度，而不是覆盖另一个 clip 的音频。
    #[test]
    fn a_short_fingerprint_collision_extends_instead_of_overwriting() {
        let manifest = manifest_with_clips();
        // CLIP_B 与 CLIP_A 共享前 12 位，因此它必须换更长的前缀。
        assert!(manifest.owner_of_path(&relative_path_for(
            SpeechContentKind::Highlight,
            CLIP_B,
            SHORT_FINGERPRINT_LEN
        )) == Some(CLIP_A));
        assert!(manifest.owner_of_path(&relative_path_for(
            SpeechContentKind::Highlight,
            CLIP_B,
            13
        ))
        .is_none());
    }

    /// 负向控制：路径穿越、绝对路径与 symlink 一律拒绝。
    #[test]
    fn containment_rejects_traversal_absolute_paths_and_symlinks() {
        let root = tempfile::tempdir().expect("root");
        let root = root.path();

        assert!(matches!(
            resolve_contained_path(root, "assets/audio/../../../etc/passwd"),
            Err(ExportError::ManifestInvalid { .. })
        ));
        assert!(matches!(
            resolve_contained_path(root, "/etc/passwd"),
            Err(ExportError::ManifestInvalid { .. })
        ));
        assert!(matches!(
            resolve_contained_path(root, ""),
            Err(ExportError::ManifestInvalid { .. })
        ));
        assert!(matches!(
            resolve_contained_path(root, "a\0b"),
            Err(ExportError::ManifestInvalid { .. })
        ));
        // `.` 段被路径迭代器规范化掉，因此结果仍在根内（不是逃逸，也不是必须拒绝的形状）。
        assert_eq!(
            resolve_contained_path(root, "assets/./audio/x.mp3")
                .expect("normalized"),
            root.join("assets/audio/x.mp3")
        );

        // symlink：即使它指向导出根内部也拒绝——manifest 不允许依赖链接解析。
        let outside = tempfile::tempdir().expect("outside");
        fs::write(outside.path().join("secret.mp3"), b"secret").expect("write outside");
        std::os::unix::fs::symlink(outside.path(), root.join("escape")).expect("symlink");
        assert!(matches!(
            resolve_contained_path(root, "escape/secret.mp3"),
            Err(ExportError::ManifestInvalid { .. })
        ));

        // 正常相对路径仍然可用。
        assert_eq!(
            resolve_contained_path(root, "assets/audio/highlight-a1.mp3")
                .expect("contained"),
            root.join("assets/audio/highlight-a1.mp3")
        );
    }

    /// 负向控制：损坏 manifest 绝不重建或猜测身份。
    #[test]
    fn a_malformed_manifest_is_rejected_instead_of_rebuilt() {
        assert!(matches!(
            SpeechExportManifest::parse("{not json"),
            Err(ExportError::ManifestInvalid { .. })
        ));
        // 未知字段按损坏处理。
        assert!(matches!(
            SpeechExportManifest::parse(
                r#"{"schema_version":1,"asset_id":"book-1","records":[],"extra":true}"#
            ),
            Err(ExportError::ManifestInvalid { .. })
        ));
        // 未知 schema 版本按损坏处理。
        assert!(matches!(
            SpeechExportManifest::parse(r#"{"schema_version":99,"asset_id":"book-1","records":[]}"#),
            Err(ExportError::ManifestInvalid { .. })
        ));
        // active 指向未记录的 clip：manifest 自相矛盾。
        assert!(matches!(
            SpeechExportManifest::parse(
                r#"{"schema_version":1,"asset_id":"book-1","records":[{"annotation_id":"annotation-41","content_kind":"highlight","active_clip_id":"deadbeef","clips":[]}]}"#
            ),
            Err(ExportError::ManifestInvalid { .. })
        ));
    }

    /// 每个内容部分只有一个 active clip，旧变体保持用户所有。
    #[test]
    fn the_newest_variant_becomes_active_and_older_variants_are_kept() {
        let mut manifest = manifest_with_clips();
        let old_path = manifest.records[0].clips[0].relative_path.clone();

        apply_to_manifest(
            &mut manifest,
            ExportedClipRecord {
                clip_id: CLIP_B.to_string(),
                relative_path: relative_path_for(SpeechContentKind::Highlight, CLIP_B, 13),
                sha256: "d".repeat(64),
                size_bytes: 20,
                format: "mp3".to_string(),
                exported_at: "2026-09-29T00:00:00Z".to_string(),
            },
            "annotation-41",
            SpeechContentKind::Highlight,
        )
        .expect("apply");

        let record = &manifest.records[0];
        assert_eq!(record.active_clip_id, CLIP_B);
        assert_eq!(record.clips.len(), 2, "older variants stay recorded");
        assert_eq!(record.clips[0].relative_path, old_path);
    }
}
