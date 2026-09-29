//! Issue #28：定位并重新验证一个 Exported Speech Clip（实施 spec 5.5 / 5.4 / §7.5，
//! ADR 0007「播放与本地回退」）。
//!
//! 本模块回答两个问题，**只回答这两个问题**：
//!
//! 1. 有哪些**候选**导出根可能含有某个 clip（显式 `--export-root` + 非权威 locator 投影）；
//! 2. 某个候选根里的某个 clip 是否**现在仍然可信**。
//!
//! 两条硬边界：
//!
//! - **绝不递归扫描用户目录。** 候选只来自两处：用户显式给出的 `--export-root`，和
//!   Speech 状态根里那一个 `exports.json` 文件。之后只读
//!   `<export-root>/assets/audio/manifest.json` 和 manifest 里记录的那一个相对路径。
//!   从不 `read_dir` 导出根、Documents、iCloud、`$HOME` 或任何父目录。
//! - **locator 是提示，不是证明。** 投影里记的路径可能已过期、可能被用户搬走、manifest
//!   可能已经换了内容。因此每个候选都必须重新读取 manifest 并逐项复核：书籍身份、
//!   Annotation + content kind、clip ID、active 状态（需要时）、路径 containment、
//!   checksum 与音频可解析性。任何一项不成立就是一次**结构化拒绝**，调用方据此
//!   失败或继续往下走，绝不退化成「按文件名猜一个」。
//!
//! 与 [`crate::speech::play`] 一样，本模块**不依赖** provider 客户端：结构上没有
//! 发起网络调用的能力，因此「回退路径不联网」不靠约定。

use crate::speech::audio::inspect_mp3;
use crate::speech::cache::ClipCache;
use crate::speech::clip::SpeechContentKind;
use crate::speech::export::{
    locator_candidates, record_export_locator, resolve_contained_path, ExportedClipRecord,
    ExportedContentRecord, SpeechExportManifest, AUDIO_SUBDIRECTORY,
};
use crate::speech::SpeechWarning;
use chrono::{DateTime, Utc};
use std::fs;
use std::path::{Path, PathBuf};

/// 候选导出根是怎么被找到的。两者都只是提示，都必须经过完整复核。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportCandidateOrigin {
    /// 用户显式传入的 `--export-root`。
    ExplicitExportRoot,
    /// 非权威 locator 投影 `exports.json` 里记录的路径。
    ExportLocator,
}

impl ExportCandidateOrigin {
    /// 稳定的机器可读值。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitExportRoot => "explicit_export_root",
            Self::ExportLocator => "export_locator",
        }
    }
}

/// 生成路径掌握的完整书籍身份。`speech play` 只有 clip ID，因此可以不提供。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedBookIdentity {
    /// 书籍稳定 ID。
    pub asset_id: String,
    /// Annotation 稳定 ID。
    pub annotation_id: String,
    /// 内容部分。
    pub content_kind: SpeechContentKind,
}

/// 一次导出回退查找的条件。
#[derive(Debug, Clone)]
pub struct ExportedClipQuery {
    /// 完整 clip ID（64 位小写 sha256 hex）。
    pub clip_id: String,
    /// 生成路径有完整书籍身份；play 路径只有 clip ID。
    pub book: Option<ExportedBookIdentity>,
    /// 用户显式给出的导出根（`--export-root`），优先于 locator。
    pub explicit_root: Option<PathBuf>,
    /// 是否只接受该内容部分的 Active Exported Speech Clip。
    ///
    /// `speech play` 传 `true`（实施 spec 5.5 的查找顺序只提到 Active Exported
    /// Speech Clip）；rehydration 传 `false`：此时 clip ID 已经是内容 + Voice Profile
    /// 的密码学身份，同一 `annotation_id + content_kind` 下记录过的同一 clip ID 变体
    /// 就是同一个逻辑 clip，不是猜测。
    pub require_active: bool,
}

/// 一个**已重新验证**的 Exported Speech Clip。只含稳定身份与本地音频事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedExportedClip {
    /// 完整 clip ID。
    pub clip_id: String,
    /// 通过复核的导出根（用户拥有；回退路径只读它）。
    pub export_root: PathBuf,
    /// 该导出目录所属书籍的稳定 ID（来自 manifest）。
    pub asset_id: String,
    /// Annotation 稳定 ID。
    pub annotation_id: String,
    /// 内容部分。
    pub content_kind: SpeechContentKind,
    /// 该变体是否为该内容部分当前的 active 变体。
    pub active: bool,
    /// 相对导出根的路径。
    pub relative_path: String,
    /// 已校验音频的绝对路径。
    pub audio_path: PathBuf,
    /// 音频字节的 SHA-256（等于 manifest 记录的值）。
    pub audio_sha256: String,
    /// 音频字节数。
    pub audio_size_bytes: u64,
    /// 音频时长（毫秒）。
    pub audio_duration_ms: u64,
    /// 采样率（Hz）。
    pub sample_rate: u32,
    /// 码率（bps）。
    pub bitrate: u32,
    /// 声道数。
    pub channel: u32,
    /// 音频格式；首版只接受 `mp3`。
    pub format: String,
}

/// 一次候选复核失败的结构化记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRejection {
    /// 候选导出根。
    pub export_root: PathBuf,
    /// 候选来源。
    pub origin: ExportCandidateOrigin,
    /// 稳定原因串。
    pub reason: &'static str,
    /// 供人类诊断的细节，不含原文或密钥。
    pub detail: String,
}

/// 一次查找的结果。
///
/// 「没有找到」是**正常结果**而不是错误：调用方据此继续走自己的下一条路径（play 返回
/// `SPEECH_CLIP_NOT_FOUND`；generate 继续走 unknown gate 与 provider 请求）。
#[derive(Debug, Clone, Default)]
pub struct ExportLookup {
    /// 第一个通过全部复核的候选。
    pub found: Option<VerifiedExportedClip>,
    /// 找到它所用的导出根。
    pub found_root: Option<PathBuf>,
    /// 找到它所用的候选来源。
    pub found_origin: Option<ExportCandidateOrigin>,
    /// 被拒绝的候选与原因，按尝试顺序排列。
    pub rejections: Vec<ExportRejection>,
    /// 非致命 warning（例如 locator 投影刷新失败）。
    pub warnings: Vec<SpeechWarning>,
}

impl ExportLookup {
    /// 找到的 clip 与其来源。
    pub fn found(&self) -> Option<(&VerifiedExportedClip, ExportCandidateOrigin)> {
        match (&self.found, self.found_origin) {
            (Some(clip), Some(origin)) => Some((clip, origin)),
            _ => None,
        }
    }

    /// 被拒绝候选的结构化 warning，供 receipt 报告「为什么没有回退」而不猜测。
    pub fn rejection_warnings(&self) -> Vec<SpeechWarning> {
        self.rejections
            .iter()
            .map(|rejection| SpeechWarning {
                code: SpeechWarning::EXPORT_LOCATOR_STALE_CODE,
                reason: rejection.reason,
                message: format!(
                    "the Exported Speech Clip in {} (found via {}) was not used: {}",
                    rejection.export_root.display(),
                    rejection.origin.as_str(),
                    rejection.detail
                ),
            })
            .collect()
    }
}

/// 在候选导出根里查找并**重新验证**一个已导出的 Speech Clip。
///
/// 顺序：显式 `--export-root` → 非权威 locator 投影里的候选（已知书籍身份时该书的
/// 候选排在前面）。每个候选独立复核，第一个通过全部检查的即返回。
///
/// 验证成功后刷新 locator 投影，因此用户把导出目录搬到新位置后，只要给出一次
/// `--export-root`，后续 play / rehydration 就不再需要它。
pub fn find_verified_exported_clip(
    cache: &ClipCache,
    query: &ExportedClipQuery,
    now: DateTime<Utc>,
) -> ExportLookup {
    let mut lookup = ExportLookup::default();
    for (root, origin) in candidate_roots(cache, query) {
        let (clip, manifest) = match verify_candidate(&root, origin, query) {
            Ok(found) => found,
            Err(rejection) => {
                lookup.rejections.push(rejection);
                continue;
            }
        };
        // 复核通过：把这个根记回 locator 投影（`--export-root` 指向移动后的新位置时，
        // 这就是刷新；投影写失败只返回 warning，不影响已经验证的 clip）。
        if let Err(reason) = record_export_locator(cache, &root, &manifest, now) {
            lookup.warnings.push(SpeechWarning {
                code: SpeechWarning::EXPORT_LOCATOR_STALE_CODE,
                reason: "export_locator_not_updated",
                message: format!(
                    "the Exported Speech Clip in {} was verified, but the non-authoritative export locator projection could not be updated ({reason})",
                    root.display()
                ),
            });
        }
        lookup.found = Some(clip);
        lookup.found_root = Some(root);
        lookup.found_origin = Some(origin);
        return lookup;
    }
    lookup
}

/// 枚举候选导出根：显式根优先，其后是 locator 投影里的路径。
///
/// 两者都只是**已知路径**，这里不做任何目录遍历、匹配或发现。
fn candidate_roots(
    cache: &ClipCache,
    query: &ExportedClipQuery,
) -> Vec<(PathBuf, ExportCandidateOrigin)> {
    let mut roots: Vec<(PathBuf, ExportCandidateOrigin)> = Vec::new();
    if let Some(explicit) = query.explicit_root.clone() {
        roots.push((explicit, ExportCandidateOrigin::ExplicitExportRoot));
    }
    let preferred_asset = query.book.as_ref().map(|book| book.asset_id.as_str());
    // 已知书籍身份时，该书的候选先试；其余候选保持 locator 的稳定顺序，仍然逐一复核。
    let mut locator: Vec<(bool, PathBuf)> = locator_candidates(cache)
        .into_iter()
        .map(|candidate| {
            let same_book = preferred_asset == Some(candidate.asset_id.as_str());
            (same_book, candidate.export_root)
        })
        .filter(|(_, root)| !roots.iter().any(|(seen, _)| seen == root))
        .collect();
    locator.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    roots.extend(
        locator
            .into_iter()
            .map(|(_, root)| (root, ExportCandidateOrigin::ExportLocator)),
    );
    roots
}

/// 在一个候选导出根里复核一个 clip。
///
/// 复核项：manifest 可解析且属于同一本书 → 找到对应记录 → （需要时）active 指向该
/// clip → 相对路径 containment → 文件存在 → 大小与 checksum 一致 → 格式与可解析性。
/// 任何一项不成立都返回结构化拒绝，不返回「大概是的」结果。
fn verify_candidate(
    export_root: &Path,
    origin: ExportCandidateOrigin,
    query: &ExportedClipQuery,
) -> Result<(VerifiedExportedClip, SpeechExportManifest), ExportRejection> {
    let reject = |reason: &'static str, detail: String| ExportRejection {
        export_root: export_root.to_path_buf(),
        origin,
        reason,
        detail,
    };

    let manifest = match SpeechExportManifest::load(&export_root) {
        Ok(Some(manifest)) => manifest,
        Ok(None) => {
            return Err(reject(
                "manifest_missing",
                format!("there is no {AUDIO_SUBDIRECTORY}/manifest.json in this export directory"),
            ))
        }
        Err(error) => {
            return Err(reject(
                "manifest_unusable",
                format!(
                    "the Speech Export Manifest could not be trusted: {}",
                    error.message()
                ),
            ))
        }
    };

    // 书籍身份：manifest 必须属于请求的那本书。
    if let Some(book) = query.book.as_ref() {
        if manifest.asset_id != book.asset_id {
            return Err(reject(
                "manifest_asset_mismatch",
                format!(
                    "the manifest belongs to asset_id {} rather than {}",
                    manifest.asset_id, book.asset_id
                ),
            ));
        }
    }

    let (annotation_id, content_kind, matched, active) = match query.book.as_ref() {
        Some(book) => {
            let Some(record) = manifest.records.iter().find(|record| {
                record.annotation_id == book.annotation_id
                    && record.content_kind == book.content_kind
            }) else {
                return Err(reject(
                    "clip_not_recorded",
                    format!(
                        "the manifest records no exported clip for annotation {} ({})",
                        book.annotation_id,
                        book.content_kind.as_str()
                    ),
                ));
            };
            let Some(clip) = record
                .clips
                .iter()
                .find(|clip| clip.clip_id == query.clip_id)
            else {
                return Err(reject(
                    "clip_not_recorded",
                    format!(
                        "the manifest records no exported clip with id {} for annotation {} ({})",
                        query.clip_id,
                        book.annotation_id,
                        book.content_kind.as_str()
                    ),
                ));
            };
            if query.require_active && record.active_clip_id != query.clip_id {
                return Err(reject(
                    "clip_not_active",
                    format!(
                        "the recorded active clip for annotation {} ({}) is {} rather than {}",
                        book.annotation_id,
                        book.content_kind.as_str(),
                        record.active_clip_id,
                        query.clip_id
                    ),
                ));
            }
            let active = record.active_clip_id == query.clip_id;
            (
                record.annotation_id.clone(),
                record.content_kind,
                manifest_clip(clip),
                active,
            )
        }
        // play 路径没有书籍上下文：只认 manifest 里**完整记录**的 clip ID，
        // 且必须是该内容部分的 active 变体（实施 spec 5.5）。
        None => {
            let mut matched: Option<(&ExportedContentRecord, &ExportedClipRecord)> = None;
            for record in &manifest.records {
                if query.require_active && record.active_clip_id != query.clip_id {
                    continue;
                }
                if let Some(clip) = record
                    .clips
                    .iter()
                    .find(|clip| clip.clip_id == query.clip_id)
                {
                    matched = Some((record, clip));
                    break;
                }
            }
            let Some((record, clip)) = matched else {
                return Err(reject(
                    "clip_not_recorded",
                    format!(
                        "the manifest records no {}exported clip with id {}",
                        if query.require_active { "active " } else { "" },
                        query.clip_id
                    ),
                ));
            };
            let active = record.active_clip_id == query.clip_id;
            (
                record.annotation_id.clone(),
                record.content_kind,
                manifest_clip(clip),
                active,
            )
        }
    };
    let clip = matched;

    // 路径 containment：复用 #27 的 guard 语义（逐段拒绝 `..`、绝对根与 symlink）。
    let audio_path =
        resolve_contained_path(&export_root, &clip.relative_path).map_err(|error| {
            reject(
                "path_not_contained",
                format!(
                    "the recorded relative path {} was rejected: {}",
                    clip.relative_path,
                    error.message()
                ),
            )
        })?;

    let bytes = match fs::read(&audio_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(reject(
                "audio_missing",
                format!("{} does not exist", clip.relative_path),
            ))
        }
        Err(error) => {
            return Err(reject(
                "audio_unreadable",
                format!("{} could not be read: {error}", clip.relative_path),
            ))
        }
    };

    if bytes.is_empty() || bytes.len() as u64 != clip.size_bytes {
        return Err(reject(
            "checksum_mismatch",
            format!(
                "{} no longer matches the size recorded in the manifest",
                clip.relative_path
            ),
        ));
    }
    if crate::speech::text::sha256_hex(&bytes) != clip.sha256 {
        return Err(reject(
            "checksum_mismatch",
            format!(
                "{} no longer matches the checksum recorded in the manifest; the user-owned file is not a verified clip",
                clip.relative_path
            ),
        ));
    }
    if clip.format != "mp3" {
        return Err(reject(
            "unsupported_format",
            format!(
                "the manifest records audio format {} rather than mp3",
                clip.format
            ),
        ));
    }
    let facts = inspect_mp3(&bytes).map_err(|error| {
        reject(
            "audio_unparsable",
            format!("{} is not a parseable MP3: {error:?}", clip.relative_path),
        )
    })?;

    Ok((
        VerifiedExportedClip {
            clip_id: query.clip_id.clone(),
            export_root: export_root.to_path_buf(),
            asset_id: manifest.asset_id.clone(),
            annotation_id,
            content_kind,
            active,
            relative_path: clip.relative_path.clone(),
            audio_path,
            audio_sha256: clip.sha256.clone(),
            audio_size_bytes: clip.size_bytes,
            audio_duration_ms: facts.duration_ms,
            sample_rate: facts.sample_rate,
            bitrate: facts.bitrate,
            channel: facts.channel,
            format: clip.format.clone(),
        },
        manifest,
    ))
}

/// manifest 里关于某个已导出变体的本地事实（复核时逐项对照磁盘字节）。
struct RehydratedManifestClip {
    relative_path: String,
    sha256: String,
    size_bytes: u64,
    format: String,
}

/// 复制 manifest 记录里的本地事实，避免长期借用 manifest。
fn manifest_clip(clip: &ExportedClipRecord) -> RehydratedManifestClip {
    RehydratedManifestClip {
        relative_path: clip.relative_path.clone(),
        sha256: clip.sha256.clone(),
        size_bytes: clip.size_bytes,
        format: clip.format.clone(),
    }
}
