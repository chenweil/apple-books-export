//! Stable machine-readable protocol for CLI consumers.

use crate::speech::SpeechWarning;
use crate::{cfi::extract_chapter_title, Annotation, Book};
use chrono::{DateTime, Utc};
use serde::Serialize;

pub const SCHEMA_VERSION: u32 = 1;
const APPLE_EPOCH_UNIX_SECONDS: i64 = 978_307_200;

#[derive(Debug, Serialize)]
pub struct BookListResponse<'a> {
    pub schema_version: u32,
    pub books: &'a [Book],
}

impl<'a> BookListResponse<'a> {
    pub fn new(books: &'a [Book]) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            books,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AnnotationResponse {
    pub schema_version: u32,
    pub asset_id: String,
    pub title: String,
    pub author: String,
    pub annotation_count: usize,
    pub annotations: Vec<AnnotationDto>,
}

impl AnnotationResponse {
    pub fn new(book: &Book, annotations: &[Annotation]) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            asset_id: book.asset_id.clone(),
            title: book.title.clone(),
            author: book.author.clone(),
            annotation_count: annotations.len(),
            annotations: annotations.iter().map(AnnotationDto::from).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AnnotationDto {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub content_text: Option<String>,
    pub note_text: Option<String>,
    pub chapter_title: Option<String>,
    pub location: Option<String>,
    pub created_at: Option<String>,
}

impl From<&Annotation> for AnnotationDto {
    fn from(annotation: &Annotation) -> Self {
        let content_text = non_empty(annotation.selected_text.as_deref());
        let note_text = non_empty(annotation.note.as_deref());
        Self {
            id: annotation.id.clone(),
            kind: if note_text.is_some() {
                "note"
            } else {
                "highlight"
            },
            content_text,
            note_text,
            chapter_title: annotation
                .location
                .as_deref()
                .and_then(extract_chapter_title),
            location: annotation.location.clone(),
            created_at: annotation.creation_date.and_then(format_apple_timestamp),
        }
    }
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn format_apple_timestamp(seconds: f64) -> Option<String> {
    if !seconds.is_finite() {
        return None;
    }
    let unix_seconds = APPLE_EPOCH_UNIX_SECONDS.checked_add(seconds.trunc() as i64)?;
    DateTime::<Utc>::from_timestamp(unix_seconds, 0)
        .map(|date_time| date_time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

#[derive(Debug, Serialize)]
pub struct MachineError {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    /// 可选的结构化细节（speech 领域用于字段、原因、provider trace 等）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl MachineError {
    pub fn from_database_error(error: crate::DatabaseAccessError) -> Self {
        match error {
            crate::DatabaseAccessError::NotFound { path } => Self {
                details: None,
                code: "DATABASE_NOT_FOUND",
                message: format!("Apple Books database was not found: {}", path.display()),
                remediation: Some(
                    "Open Apple Books once, download at least one book, then retry.".to_string(),
                ),
            },
            crate::DatabaseAccessError::PermissionDenied { path } => Self {
                details: None,
                code: "FULL_DISK_ACCESS_REQUIRED",
                message: format!(
                    "Permission was denied while reading Apple Books data: {}",
                    path.display()
                ),
                remediation: Some(
                    "Grant Full Disk Access to this terminal or application in System Settings > Privacy & Security > Full Disk Access, then retry."
                        .to_string(),
                ),
            },
            crate::DatabaseAccessError::Unreadable { path, message } => Self {
                details: None,
                code: "DATABASE_UNREADABLE",
                message: format!("Apple Books database is unreadable at {}: {message}", path.display()),
                remediation: Some(
                    "Close Apple Books, verify the database is intact and readable, then retry."
                        .to_string(),
                ),
            },
        }
    }

    /// 附加结构化细节。
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn unsupported_schema_version(version: u32) -> Self {
        Self {
            details: None,
            code: "UNSUPPORTED_SCHEMA_VERSION",
            message: format!("Schema version {version} is not supported."),
            remediation: Some(
                "Update the consumer or use a compatible apple-books-exporter binary.".to_string(),
            ),
        }
    }

    pub fn missing_asset_id() -> Self {
        Self {
            details: None,
            code: "INVALID_ASSET_ID",
            message: "This machine command requires --asset-id.".to_string(),
            remediation: Some(
                "Run `apple-books-exporter list --json`, then pass one returned asset_id."
                    .to_string(),
            ),
        }
    }

    pub fn invalid_asset_id(asset_id: &str) -> Self {
        Self {
            details: None,
            code: "INVALID_ASSET_ID",
            message: format!("No Apple Books item was found for asset_id '{asset_id}'."),
            remediation: Some("Run `apple-books-exporter list --json` and use an asset_id from the refreshed response.".to_string()),
        }
    }

    pub fn database_unreadable(message: impl Into<String>) -> Self {
        Self {
            details: None,
            code: "DATABASE_UNREADABLE",
            message: message.into(),
            remediation: Some(
                "Close Apple Books, verify its databases are present and readable, then retry."
                    .to_string(),
            ),
        }
    }

    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self {
            details: None,
            code: "INVALID_ARGUMENT",
            message: message.into(),
            remediation: Some(
                "Run the command with --help and use either the human positional form or the JSON asset_id form."
                    .to_string(),
            ),
        }
    }

    pub fn protocol_serialization_failed(message: impl Into<String>) -> Self {
        Self {
            details: None,
            code: "PROTOCOL_SERIALIZATION_FAILED",
            message: message.into(),
            remediation: None,
        }
    }

    pub fn binary_incompatible() -> Self {
        Self {
            details: None,
            code: "BINARY_INCOMPATIBLE",
            message: format!(
                "This binary cannot read Apple Books on {} {}.",
                std::env::consts::OS,
                std::env::consts::ARCH
            ),
            remediation: Some(
                "Run a native macOS aarch64 or x86_64 build of apple-books-exporter.".to_string(),
            ),
        }
    }

    pub fn output_unwritable(message: impl Into<String>) -> Self {
        Self {
            details: None,
            code: "OUTPUT_UNWRITABLE",
            message: message.into(),
            remediation: Some(
                "Choose a writable output directory and retry the export.".to_string(),
            ),
        }
    }

    pub fn output_file_exists(path: &std::path::Path) -> Self {
        Self {
            details: None,
            code: "OUTPUT_FILE_EXISTS",
            message: format!("Output file already exists: {}", path.display()),
            remediation: Some(
                "Choose another output directory or pass --overwrite to replace existing files."
                    .to_string(),
            ),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub schema_version: u32,
    pub error: MachineError,
}

impl ErrorResponse {
    pub fn new(error: MachineError) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            error,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ExportResponse {
    pub schema_version: u32,
    pub receipt: ExportReceipt,
}

#[derive(Debug, Serialize)]
pub struct ExportReceipt {
    pub asset_id: String,
    pub title: String,
    pub annotation_count: usize,
    pub format: &'static str,
    pub output_directory: String,
    pub generated_files: Vec<String>,
    /// 已写入的相对音频链接（只有用户显式 `speech export` 过才有）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio_links: Vec<String>,
    /// Speech 音频是可选的：缺失或校验失败只产生 warning，主体导出不失败。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<SpeechWarning>,
}

impl ExportResponse {
    pub fn new(
        book: &Book,
        annotation_count: usize,
        format: crate::ExportFormat,
        output_directory: &std::path::Path,
        generated_files: &[std::path::PathBuf],
        speech: &crate::exporter::SpeechExportLinks,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            receipt: ExportReceipt {
                asset_id: book.asset_id.clone(),
                title: book.title.clone(),
                annotation_count,
                format: match format {
                    crate::ExportFormat::Obsidian => "obsidian",
                    crate::ExportFormat::Markdown => "markdown",
                },
                output_directory: output_directory.to_string_lossy().into_owned(),
                generated_files: generated_files
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect(),
                audio_links: speech.written.clone(),
                warnings: speech.warnings.clone(),
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DoctorResponse {
    pub schema_version: u32,
    pub status: &'static str,
    pub binary: BinaryStatus,
    pub databases: DatabaseStatuses,
    pub environment: EnvironmentStatus,
}

/// #14 环境预检：在真正写文件之前告诉消费者环境是否可用。
///
/// `list` / `annotations` / `export` 之前各自失败，会把同一个环境问题在三个地方
/// 重复报告；集中在这里，消费者只需在启动时问一次。
#[derive(Debug, Serialize)]
pub struct EnvironmentStatus {
    pub home: HomeStatus,
    pub default_output_dir: OutputDirStatus,
    pub free_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct HomeStatus {
    pub status: &'static str,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct OutputDirStatus {
    pub status: &'static str,
    pub path: String,
    pub writable: bool,
}

#[derive(Debug, Serialize)]
pub struct BinaryStatus {
    pub version: &'static str,
    pub os: &'static str,
    pub architecture: &'static str,
}

#[derive(Debug, Serialize)]
pub struct DatabaseStatuses {
    pub annotation: DatabaseStatus,
    pub library: DatabaseStatus,
}

#[derive(Debug, Serialize)]
pub struct DatabaseStatus {
    pub status: &'static str,
    pub path: String,
}

impl DoctorResponse {
    pub fn ready(annotation_path: &std::path::Path, library_path: &std::path::Path) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            status: "ok",
            binary: BinaryStatus {
                version: env!("CARGO_PKG_VERSION"),
                os: std::env::consts::OS,
                architecture: std::env::consts::ARCH,
            },
            databases: DatabaseStatuses {
                annotation: DatabaseStatus {
                    status: "readable",
                    path: annotation_path.to_string_lossy().into_owned(),
                },
                library: DatabaseStatus {
                    status: "readable",
                    path: library_path.to_string_lossy().into_owned(),
                },
            },
            environment: EnvironmentStatus::probe(),
        }
    }
}

impl EnvironmentStatus {
    /// 探测 HOME、默认输出目录可写性与所在卷的可用字节数。
    ///
    /// 预检只报告、不修复：目录不存在时报告 `missing` 而不是顺手创建，
    /// 否则 `doctor` 会把「用户还没导出过」这种正常状态说成错误。
    pub fn probe() -> Self {
        let home = crate::utils::home_dir();
        let home_status = match &home {
            Some(path) => HomeStatus {
                status: if path.is_dir() { "ok" } else { "missing" },
                path: path.to_string_lossy().into_owned(),
            },
            None => HomeStatus {
                status: "missing",
                path: String::new(),
            },
        };

        let default_output_dir = home
            .as_ref()
            .map(|path| path.join("books-exported"))
            .unwrap_or_else(|| std::path::PathBuf::from("books-exported"));

        // 只判断「能否写」，不创建目录：父目录不存在时用最近的上层目录判断。
        let probe_dir = nearest_existing_ancestor(&default_output_dir);
        let writable = probe_dir
            .as_deref()
            .is_some_and(crate::utils::dir_is_writable);
        let output_status = OutputDirStatus {
            status: match &home {
                None => "unknown",
                _ if !writable => "unwritable",
                _ if default_output_dir.is_dir() => "ok",
                _ => "missing",
            },
            path: default_output_dir.to_string_lossy().into_owned(),
            writable,
        };

        // 必须在已存在的目录上查询：`statvfs` 对不存在的路径返回 ENOENT，
        // 而全新机器上 ~/books-exported 正是如此，否则每个新用户都会看到
        // free_bytes == 0 并被误判为磁盘已满。
        let free_bytes = probe_dir
            .as_deref()
            .and_then(free_bytes_for)
            .unwrap_or_default();

        Self {
            home: home_status,
            default_output_dir: output_status,
            free_bytes,
        }
    }
}

/// 从 `path` 向上找到第一个已存在的祖先目录。
fn nearest_existing_ancestor(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        if candidate.is_dir() {
            return Some(candidate.to_path_buf());
        }
        current = candidate.parent();
    }
    None
}

/// 报告 `path` 所在卷的可用字节数；无法查询时返回 `None` 而非编造一个数字。
#[cfg(unix)]
fn free_bytes_for(path: &std::path::Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `c_path` is a valid NUL-terminated C string that outlives the call,
    // and `statvfs` only writes the single `statvfs` output argument we initialise.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stat) != 0 {
            return None;
        }
        Some(u64::from(stat.f_bavail).saturating_mul(stat.f_frsize))
    }
}

#[cfg(not(unix))]
fn free_bytes_for(_path: &std::path::Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_errors_keep_their_exact_json_shape_without_details() {
        let error = MachineError::invalid_asset_id("book-1");
        let value = serde_json::to_value(&error).expect("serialize error");

        assert!(
            value.get("details").is_none(),
            "adding optional details must not change existing error payloads"
        );
        assert_eq!(value["code"], "INVALID_ASSET_ID");
        assert!(value["message"].is_string());
        assert!(value["remediation"].is_string());
    }

    #[test]
    fn details_are_serialized_only_when_present() {
        let error = MachineError::missing_asset_id()
            .with_details(serde_json::json!({ "field": "voice_id" }));
        let value = serde_json::to_value(&error).expect("serialize error");

        assert_eq!(value["details"]["field"], "voice_id");
        assert_eq!(value["code"], "INVALID_ASSET_ID");
    }

    #[test]
    fn required_compatibility_errors_keep_stable_codes() {
        assert_eq!(
            MachineError::unsupported_schema_version(2).code,
            "UNSUPPORTED_SCHEMA_VERSION"
        );
        assert_eq!(
            MachineError::binary_incompatible().code,
            "BINARY_INCOMPATIBLE"
        );
    }
}
