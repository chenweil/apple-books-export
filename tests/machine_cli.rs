use rusqlite::{params, Connection};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

struct Fixture {
    home: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let home = tempfile::tempdir().expect("fixture home");
        let annotation_dir = home
            .path()
            .join("Library/Containers/com.apple.iBooksX/Data/Documents/AEAnnotation");
        let library_dir = home
            .path()
            .join("Library/Containers/com.apple.iBooksX/Data/Documents/BKLibrary");
        std::fs::create_dir_all(&annotation_dir).expect("annotation directory");
        std::fs::create_dir_all(&library_dir).expect("library directory");

        let annotation_db = annotation_dir.join("annotations.sqlite");
        let annotation_conn = Connection::open(annotation_db).expect("annotation database");
        annotation_conn
            .execute_batch(
                "CREATE TABLE ZAEANNOTATION (
                    Z_PK INTEGER PRIMARY KEY,
                    ZANNOTATIONASSETID TEXT,
                    ZANNOTATIONSELECTEDTEXT TEXT,
                    ZANNOTATIONNOTE TEXT,
                    ZANNOTATIONLOCATION TEXT,
                    ZANNOTATIONCREATIONDATE REAL,
                    ZANNOTATIONTYPE INTEGER,
                    ZANNOTATIONDELETED INTEGER
                );",
            )
            .expect("annotation schema");
        annotation_conn
            .execute(
                "INSERT INTO ZAEANNOTATION VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
                params![
                    41,
                    "book-1",
                    "高亮正文",
                    "我的笔记",
                    "epubcfi(/6/10[Section0003.xhtml]!/4/82/1,:0,:44)",
                    60.0,
                    3
                ],
            )
            .expect("annotation row");
        annotation_conn
            .execute(
                "INSERT INTO ZAEANNOTATION VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
                params![
                    42,
                    "book-1",
                    "只有高亮",
                    Option::<String>::None,
                    "epubcfi(/6/12[chapter5.xhtml]!/4/2)",
                    120.0,
                    3
                ],
            )
            .expect("highlight row");

        let library_db = library_dir.join("library.sqlite");
        let library_conn = Connection::open(library_db).expect("library database");
        library_conn
            .execute_batch(
                "CREATE TABLE ZBKLIBRARYASSET (
                    Z_PK INTEGER PRIMARY KEY,
                    ZASSETID TEXT,
                    ZTITLE TEXT,
                    ZAUTHOR TEXT,
                    ZSORTKEY TEXT
                );",
            )
            .expect("library schema");
        library_conn
            .execute(
                "INSERT INTO ZBKLIBRARYASSET VALUES (1, 'book-1', '测试书', '测试作者', '测试书')",
                [],
            )
            .expect("library row");

        Self { home }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_apple-books-exporter"))
            .args(args)
            .env("HOME", self.home.path())
            .output()
            .expect("run CLI")
    }

    fn home(&self) -> &Path {
        self.home.path()
    }

    fn output_dir(&self) -> PathBuf {
        self.home().join("exports")
    }
}

#[test]
fn annotations_json_returns_normalized_machine_dtos() {
    let fixture = Fixture::new();

    let output = fixture.run(&["annotations", "--asset-id", "book-1", "--json"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON only");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["asset_id"], "book-1");
    assert_eq!(value["title"], "测试书");
    assert_eq!(value["annotations"][0]["id"], "annotation-41");
    assert_eq!(value["annotations"][0]["type"], "note");
    assert_eq!(value["annotations"][0]["content_text"], "高亮正文");
    assert_eq!(value["annotations"][0]["note_text"], "我的笔记");
    assert_eq!(value["annotations"][0]["chapter_title"], "第3章");
    assert_eq!(
        value["annotations"][0]["location"],
        "epubcfi(/6/10[Section0003.xhtml]!/4/82/1,:0,:44)"
    );
    assert_eq!(
        value["annotations"][0]["created_at"],
        "2001-01-01T00:01:00Z"
    );
}

#[test]
fn invalid_asset_id_is_structured_json_on_stderr() {
    let fixture = Fixture::new();

    let output = fixture.run(&["annotations", "--asset-id", "missing-book", "--json"]);

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let value: Value = serde_json::from_slice(&output.stderr).expect("stderr is JSON only");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["error"]["code"], "INVALID_ASSET_ID");
    assert!(value["error"]["message"]
        .as_str()
        .unwrap()
        .contains("missing-book"));
}

#[test]
fn annotations_json_without_asset_id_is_structured_json_on_stderr() {
    let fixture = Fixture::new();

    let output = fixture.run(&["annotations", "--json"]);

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let value: Value = serde_json::from_slice(&output.stderr).expect("stderr is JSON only");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["error"]["code"], "INVALID_ASSET_ID");
    assert!(value["error"]["remediation"]
        .as_str()
        .unwrap()
        .contains("list --json"));
}

#[test]
fn export_modes_reject_conflicting_arguments_instead_of_ignoring_them() {
    let fixture = Fixture::new();

    let machine = fixture.run(&["export", "1", "--asset-id", "book-1", "--json"]);
    assert!(!machine.status.success());
    assert!(machine.stdout.is_empty());
    let machine_error: Value =
        serde_json::from_slice(&machine.stderr).expect("structured machine error JSON");
    assert_eq!(machine_error["error"]["code"], "INVALID_ARGUMENT");

    let human_asset_id = fixture.run(&["export", "--asset-id", "book-1"]);
    assert!(!human_asset_id.status.success());
    assert!(String::from_utf8_lossy(&human_asset_id.stderr).contains("--json"));

    let human_overwrite = fixture.run(&["export", "1", "--overwrite"]);
    assert!(!human_overwrite.status.success());
    assert!(String::from_utf8_lossy(&human_overwrite.stderr).contains("--overwrite"));
}

#[test]
fn export_json_writes_markdown_and_returns_receipt() {
    let fixture = Fixture::new();
    let output_dir = fixture.output_dir();
    let output_dir_arg = output_dir.to_string_lossy().into_owned();

    let output = fixture.run(&[
        "export",
        "--asset-id",
        "book-1",
        "--json",
        "--output",
        &output_dir_arg,
        "--format",
        "markdown",
    ]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON only");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["receipt"]["asset_id"], "book-1");
    assert_eq!(value["receipt"]["title"], "测试书");
    assert_eq!(value["receipt"]["annotation_count"], 2);
    assert_eq!(value["receipt"]["format"], "markdown");
    assert_eq!(value["receipt"]["output_directory"], output_dir_arg);

    let generated = value["receipt"]["generated_files"][0]
        .as_str()
        .expect("generated file path");
    let content = std::fs::read_to_string(generated).expect("generated Markdown exists");
    assert!(content.contains("# 测试书"));
    assert!(content.contains("> 高亮正文"));
    assert!(content.contains("**笔记**: 我的笔记"));
}

#[test]
fn export_json_keeps_generated_files_inside_selected_output_directory() {
    let fixture = Fixture::new();
    let library_path = fixture
        .home()
        .join("Library/Containers/com.apple.iBooksX/Data/Documents/BKLibrary/library.sqlite");
    let connection = Connection::open(library_path).expect("open library fixture");
    connection
        .execute(
            "UPDATE ZBKLIBRARYASSET SET ZTITLE = '../escaped' WHERE ZASSETID = 'book-1'",
            [],
        )
        .expect("set traversal title");
    drop(connection);

    let output_dir = fixture.output_dir();
    let output_dir_arg = output_dir.to_string_lossy().into_owned();
    let output = fixture.run(&[
        "export",
        "--asset-id",
        "book-1",
        "--json",
        "--output",
        &output_dir_arg,
    ]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON only");
    let generated = PathBuf::from(
        value["receipt"]["generated_files"][0]
            .as_str()
            .expect("generated file path"),
    );
    assert!(generated.starts_with(&output_dir));
    assert!(!fixture.home().join("escaped").exists());
}

#[test]
fn export_json_refuses_existing_file_unless_overwrite_is_explicit() {
    let fixture = Fixture::new();
    let output_dir_arg = fixture.output_dir().to_string_lossy().into_owned();
    let base_args = [
        "export",
        "--asset-id",
        "book-1",
        "--json",
        "--output",
        &output_dir_arg,
    ];
    let first = fixture.run(&base_args);
    assert!(first.status.success());

    let second = fixture.run(&base_args);
    assert!(!second.status.success());
    assert!(second.stdout.is_empty());
    let error: Value = serde_json::from_slice(&second.stderr).expect("structured error JSON");
    assert_eq!(error["error"]["code"], "OUTPUT_FILE_EXISTS");
    assert!(error["error"]["remediation"]
        .as_str()
        .unwrap()
        .contains("--overwrite"));

    let overwritten = fixture.run(&[
        "export",
        "--asset-id",
        "book-1",
        "--json",
        "--output",
        &output_dir_arg,
        "--overwrite",
    ]);
    assert!(
        overwritten.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&overwritten.stderr)
    );
}

#[test]
fn doctor_json_reports_binary_and_database_readiness() {
    let fixture = Fixture::new();

    let output = fixture.run(&["doctor", "--json"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON only");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["status"], "ok");
    assert_eq!(value["binary"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(value["binary"]["os"].is_string());
    assert!(value["binary"]["architecture"].is_string());
    assert_eq!(value["databases"]["annotation"]["status"], "readable");
    assert_eq!(value["databases"]["library"]["status"], "readable");
    assert!(value["databases"]["annotation"]["path"]
        .as_str()
        .unwrap()
        .ends_with("annotations.sqlite"));
}

#[test]
fn doctor_json_reports_missing_databases_with_stable_error_code() {
    let home = tempfile::tempdir().expect("empty home");

    let output = Command::new(env!("CARGO_BIN_EXE_apple-books-exporter"))
        .args(["doctor", "--json"])
        .env("HOME", home.path())
        .output()
        .expect("run CLI");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let value: Value = serde_json::from_slice(&output.stderr).expect("stderr is JSON only");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["error"]["code"], "DATABASE_NOT_FOUND");
    assert!(value["error"]["remediation"]
        .as_str()
        .unwrap()
        .contains("Apple Books"));
}

#[test]
fn list_json_uses_structured_database_errors() {
    let home = tempfile::tempdir().expect("empty home");

    let output = Command::new(env!("CARGO_BIN_EXE_apple-books-exporter"))
        .args(["list", "--json"])
        .env("HOME", home.path())
        .output()
        .expect("run CLI");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let value: Value = serde_json::from_slice(&output.stderr).expect("stderr is JSON only");
    assert_eq!(value["error"]["code"], "DATABASE_NOT_FOUND");
}

#[cfg(unix)]
#[test]
fn doctor_json_reports_full_disk_access_when_database_directory_is_denied() {
    use std::os::unix::fs::PermissionsExt;

    let home = tempfile::tempdir().expect("fixture home");
    let annotation_dir = home
        .path()
        .join("Library/Containers/com.apple.iBooksX/Data/Documents/AEAnnotation");
    std::fs::create_dir_all(&annotation_dir).expect("annotation directory");
    std::fs::set_permissions(&annotation_dir, std::fs::Permissions::from_mode(0o000))
        .expect("deny annotation directory");

    let output = Command::new(env!("CARGO_BIN_EXE_apple-books-exporter"))
        .args(["doctor", "--json"])
        .env("HOME", home.path())
        .output()
        .expect("run CLI");

    std::fs::set_permissions(&annotation_dir, std::fs::Permissions::from_mode(0o700))
        .expect("restore annotation directory");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let value: Value = serde_json::from_slice(&output.stderr).expect("stderr is JSON only");
    assert_eq!(value["error"]["code"], "FULL_DISK_ACCESS_REQUIRED");
    assert!(value["error"]["remediation"]
        .as_str()
        .unwrap()
        .contains("Full Disk Access"));
}

#[test]
fn doctor_json_reports_unreadable_database_schema() {
    let fixture = Fixture::new();
    let library_path = fixture
        .home()
        .join("Library/Containers/com.apple.iBooksX/Data/Documents/BKLibrary/library.sqlite");
    let connection = Connection::open(library_path).expect("open library fixture");
    connection
        .execute("DROP TABLE ZBKLIBRARYASSET", [])
        .expect("break library schema");
    drop(connection);

    let output = fixture.run(&["doctor", "--json"]);

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let value: Value = serde_json::from_slice(&output.stderr).expect("stderr is JSON only");
    assert_eq!(value["error"]["code"], "DATABASE_UNREADABLE");
    assert!(value["error"]["remediation"].is_string());
}

#[test]
fn list_json_returns_versioned_book_dtos_only() {
    let fixture = Fixture::new();

    let output = fixture.run(&["list", "--json"]);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON only");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["books"][0]["asset_id"], "book-1");
    assert_eq!(value["books"][0]["title"], "测试书");
    assert_eq!(value["books"][0]["author"], "测试作者");
    assert_eq!(value["books"][0]["note_count"], 2);
}

#[test]
fn export_json_defaults_to_home_books_exported_and_obsidian() {
    let fixture = Fixture::new();

    let output = fixture.run(&["export", "--asset-id", "book-1", "--json"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON only");
    assert_eq!(value["receipt"]["format"], "obsidian");
    assert_eq!(
        value["receipt"]["output_directory"],
        fixture
            .home()
            .join("books-exported")
            .to_string_lossy()
            .as_ref()
    );
}

#[test]
fn human_list_and_positional_export_remain_compatible() {
    let fixture = Fixture::new();
    let list = fixture.run(&["list"]);
    assert!(list.status.success());
    assert!(String::from_utf8_lossy(&list.stdout).contains("测试书"));

    let output_dir_arg = fixture.output_dir().to_string_lossy().into_owned();
    let export = fixture.run(&["export", "1", "--output", &output_dir_arg]);
    assert!(
        export.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&export.stderr)
    );
    assert!(String::from_utf8_lossy(&export.stdout).contains("导出完成"));
}
