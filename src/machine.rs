//! Stable machine-readable protocol for CLI consumers.

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
}

impl MachineError {
    pub fn from_database_error(error: crate::DatabaseAccessError) -> Self {
        match error {
            crate::DatabaseAccessError::NotFound { path } => Self {
                code: "DATABASE_NOT_FOUND",
                message: format!("Apple Books database was not found: {}", path.display()),
                remediation: Some(
                    "Open Apple Books once, download at least one book, then retry.".to_string(),
                ),
            },
            crate::DatabaseAccessError::PermissionDenied { path } => Self {
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
                code: "DATABASE_UNREADABLE",
                message: format!("Apple Books database is unreadable at {}: {message}", path.display()),
                remediation: Some(
                    "Close Apple Books, verify the database is intact and readable, then retry."
                        .to_string(),
                ),
            },
        }
    }

    pub fn unsupported_schema_version(version: u32) -> Self {
        Self {
            code: "UNSUPPORTED_SCHEMA_VERSION",
            message: format!("Schema version {version} is not supported."),
            remediation: Some(
                "Update the consumer or use a compatible apple-books-exporter binary.".to_string(),
            ),
        }
    }

    pub fn missing_asset_id() -> Self {
        Self {
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
            code: "INVALID_ASSET_ID",
            message: format!("No Apple Books item was found for asset_id '{asset_id}'."),
            remediation: Some("Run `apple-books-exporter list --json` and use an asset_id from the refreshed response.".to_string()),
        }
    }

    pub fn database_unreadable(message: impl Into<String>) -> Self {
        Self {
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
            code: "PROTOCOL_SERIALIZATION_FAILED",
            message: message.into(),
            remediation: None,
        }
    }

    pub fn binary_incompatible() -> Self {
        Self {
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
            code: "OUTPUT_UNWRITABLE",
            message: message.into(),
            remediation: Some(
                "Choose a writable output directory and retry the export.".to_string(),
            ),
        }
    }

    pub fn output_file_exists(path: &std::path::Path) -> Self {
        Self {
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
}

impl ExportResponse {
    pub fn new(
        book: &Book,
        annotation_count: usize,
        format: crate::ExportFormat,
        output_directory: &std::path::Path,
        generated_files: &[std::path::PathBuf],
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
