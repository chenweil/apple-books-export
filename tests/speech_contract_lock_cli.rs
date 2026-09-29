//! Issue #29 的 Speech **聚合合同锁**。
//!
//! #21–#28 各自锁住了自己那一步的合同。这个文件不引入任何新行为，只把整个 Speech
//! 功能的**跨命令**保证钉死，避免「每一步都对、组合起来悄悄变了」：
//!
//! - **criterion 1**：每个 Speech 叶命令都有人类 help，且 `--json` 成功/失败都带
//!   `schema_version`，成功只写 stdout、失败只写 stderr。
//! - **criterion 2**：每条稳定错误码都在文档化的集合内，错误信封带可执行的
//!   `remediation`，且不泄漏原文、密钥或供应商原始响应。
//! - **criterion 3/4**：负向控制——内容选择、无隐式网络、缓存完整性、导出包含性、
//! 清单权威性、秘密安全，以及**既有的非 Speech 命令不获得任何隐式 Speech 行为**。
//!
//! 沿用既有约定：进程内 TCP mock provider（计数每个连接）、注入 HOME 隔离 Speech
//! 状态根、移除全部代理变量让请求只能直达 mock、secret canary 断言。

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration as StdDuration;
use tempfile::TempDir;

/// mock provider 使用的测试密钥；断言它永不进入输出或落盘。
const TEST_KEY: &str = "issue-29-test-key";
/// 探测 secret 落盘的 canary 值。
const SECRET_CANARY: &str = "canary-secret-29f4b1-do-not-persist";
/// Annotation fixture 的高亮原文；同时用作「原文不得进入 JSON」的探测串。
const HIGHLIGHT_TEXT: &str = "高亮正文";
/// Annotation fixture 的个人笔记。
const NOTE_TEXT: &str = "我的私人笔记";

/// 一次被 mock 记录的连接。多数断言只看连接计数。
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct RequestRecord {
    path: String,
    // 记录请求形状与凭证，仅供失败时诊断与 canary 断言。
    #[allow(dead_code)]
    authorization: Option<String>,
    #[allow(dead_code)]
    body: Vec<u8>,
}

/// 进程内 mock provider：记录每个连接，并可控制响应。
struct MockProvider {
    url: String,
    records: Arc<Mutex<Vec<RequestRecord>>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl MockProvider {
    fn serve(response: Response) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock provider");
        let address = listener.local_addr().expect("mock address");
        let records = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&records);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let join = thread::spawn(move || {
            listener.set_nonblocking(true).expect("nonblocking accept");
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).expect("blocking mock stream");
                        let (headers, request_body) = read_request(&mut stream);
                        let request_path = request_path(&headers);
                        captured.lock().expect("records").push(RequestRecord {
                            path: request_path.clone(),
                            authorization: header_value(&headers, "authorization"),
                            body: request_body,
                        });
                        let body = (response.body)(request_path.as_str());
                        let status = (response.status)(request_path.as_str());
                        let header = format!(
                            "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        stream.write_all(header.as_bytes()).expect("write headers");
                        stream.write_all(&body).expect("write response");
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if stop_flag.load(Ordering::SeqCst) {
                            return;
                        }
                        thread::sleep(StdDuration::from_millis(5));
                    }
                    Err(error) => panic!("accept request: {error}"),
                }
            }
        });
        Self {
            url: format!("http://{address}"),
            records,
            stop,
            join: Some(join),
        }
    }

    /// 关闭 mock 并返回它记录的全部连接。
    fn finish(mut self) -> Vec<RequestRecord> {
        self.stop.store(true, Ordering::SeqCst);
        self.join.take().expect("mock thread").join().expect("mock");
        self.records.lock().expect("records").clone()
    }
}

struct Response {
    status: Arc<dyn Fn(&str) -> u16 + Send + Sync>,
    body: Arc<dyn Fn(&str) -> Vec<u8> + Send + Sync>,
}

impl Response {
    fn status(status: u16) -> Arc<dyn Fn(&str) -> u16 + Send + Sync> {
        Arc::new(move |_| status)
    }
}

/// 一个可解析的最小 MP3：MPEG1 Layer III、128kbps、32000Hz、立体声。
fn silent_mp3(frames: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    for _ in 0..frames {
        bytes.extend_from_slice(&[0xFF, 0xFB, 0x98, 0x0C]);
        let length = 144 * 128_000 / 32_000;
        bytes.extend(std::iter::repeat(0u8).take(length - 4));
    }
    bytes
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut hex = String::new();
    for byte in bytes {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

fn synthesis_response(trace_id: &str, frames: usize) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "data": {"audio": hex_encode(&silent_mp3(frames)), "status": 2},
        "extra_info": {
            "audio_length": 72,
            "audio_sample_rate": 32000,
            "audio_size": frames * 576,
            "bitrate": 128000,
            "audio_format": "mp3",
            "audio_channel": 2,
            "word_count": 4,
            "usage_characters": 8
        },
        "trace_id": trace_id,
        "base_resp": {"status_code": 0, "status_msg": "success"}
    }))
    .expect("response JSON")
}

fn catalog_response() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "system_voice": [{
            "voice_id": "male_0004_a",
            "voice_name": "默认男声",
            "description": ["平稳"],
            "created_time": "2026-09-11T00:00:00Z"
        }],
        "voice_cloning": [],
        "voice_generation": [],
        "base_resp": {"status_code": 0, "status_msg": "success"}
    }))
    .expect("catalog JSON")
}

fn read_request(stream: &mut TcpStream) -> (String, Vec<u8>) {
    let mut bytes = Vec::new();
    let header_end;
    loop {
        let mut chunk = [0_u8; 1024];
        let count = stream.read(&mut chunk).expect("read request");
        assert!(count > 0, "request ended before headers");
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            header_end = index + 4;
            break;
        }
    }
    let headers = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
    let body = bytes[header_end..].to_vec();
    (headers, body)
}

fn header_value(headers: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}: ");
    headers.lines().find_map(|line| {
        line.strip_prefix(&prefix)
            .or_else(|| line.strip_prefix(&name.to_lowercase()))
            .map(|value| value.trim().to_string())
    })
}

fn request_path(headers: &str) -> String {
    headers
        .lines()
        .find(|line| line.starts_with("POST ") || line.starts_with("GET "))
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or_default()
        .to_string()
}

/// Apple Books fixture + 隔离 HOME 的 Speech 状态根。
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

        let annotation_conn = rusqlite::Connection::open(annotation_dir.join("annotations.sqlite"))
            .expect("annotation database");
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
                "INSERT INTO ZAEANNOTATION VALUES (41, 'book-1', ?1, NULL, 'epubcfi(/6/2)', 0.0, 3, 0)",
                rusqlite::params![HIGHLIGHT_TEXT],
            )
            .expect("highlight-only annotation");
        annotation_conn
            .execute(
                "INSERT INTO ZAEANNOTATION VALUES (42, 'book-1', '第二段高亮', ?1, 'epubcfi(/6/3)', 0.0, 3, 0)",
                rusqlite::params![NOTE_TEXT],
            )
            .expect("highlight plus note annotation");

        let library_conn = rusqlite::Connection::open(library_dir.join("library.sqlite"))
            .expect("library database");
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
                "INSERT OR IGNORE INTO ZBKLIBRARYASSET VALUES (1, 'book-1', '测试书', '测试作者', 'book-1')",
                rusqlite::params![],
            )
            .expect("library row");

        Self { home }
    }

    /// 移除全部代理变量与 Speech 凭证：请求只能直达本地 mock。
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_apple-books-exporter"));
        command
            .env("HOME", self.home.path())
            .env_remove("SENSEAUDIO_API_KEY")
            .env_remove("SENSEAUDIO_API_BASE_URL")
            .env_remove("HTTP_PROXY")
            .env_remove("http_proxy")
            .env_remove("HTTPS_PROXY")
            .env_remove("https_proxy")
            .env_remove("ALL_PROXY")
            .env_remove("all_proxy")
            .env_remove("NO_PROXY")
            .env_remove("no_proxy");
        command
    }

    /// 指向 mock provider 的命令；`SENSEAUDIO_API_BASE_URL` 是显式注入，dead-proxy
    /// 变量已全部移除，因此「零连接」是可计数的而不是靠超时推断。
    fn run_with(&self, args: &[&str], provider: &MockProvider, key: Option<&str>) -> Output {
        let mut command = self.command();
        command
            .args(args)
            .env("SENSEAUDIO_API_BASE_URL", &provider.url);
        if let Some(key) = key {
            command.env("SENSEAUDIO_API_KEY", key);
        }
        command.output().expect("run CLI")
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().expect("run CLI")
    }

    fn speech_root(&self) -> PathBuf {
        self.home
            .path()
            .join("Library/Application Support/books-exporter/speech")
    }

    fn book_export_root(&self) -> PathBuf {
        self.home.path().join("books-exported/测试书")
    }

    fn manifest_path(&self) -> PathBuf {
        self.book_export_root()
            .join("assets/audio")
            .join("manifest.json")
    }

    /// 隔离 HOME 下 Speech 状态根的完整文件清单（相对路径 + 字节数）。
    fn speech_root_files(&self) -> Vec<(PathBuf, u64)> {
        let root = self.speech_root();
        if !root.exists() {
            return Vec::new();
        }
        let mut files = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(directory) = stack.pop() {
            let entries = std::fs::read_dir(&directory).expect("read speech root");
            for entry in entries {
                let entry = entry.expect("dir entry");
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    files.push((
                        path.strip_prefix(&root).expect("relative path").to_path_buf(),
                        size,
                    ));
                }
            }
        }
        files.sort();
        files
    }
}

fn succeeded(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "expected success, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "success must keep stderr empty, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout is a single JSON document")
}

fn failed(output: &Output) -> Value {
    assert!(!output.status.success(), "expected failure");
    assert!(
        output.stdout.is_empty(),
        "failure must keep stdout empty, got: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stderr).expect("stderr is a single JSON error document")
}

fn provider_with_catalog_and_synthesis(trace_id: &str) -> MockProvider {
    let catalog = catalog_response();
    let synthesis = synthesis_response(trace_id, 2);
    MockProvider::serve(Response {
        status: Response::status(200),
        body: Arc::new(move |path: &str| {
            if path.ends_with("/v1/t2a_v2") {
                synthesis.clone()
            } else {
                catalog.clone()
            }
        }),
    })
}

fn assert_zero_connections(records: &[RequestRecord], path: &str) {
    assert_eq!(
        records.len(),
        0,
        "`{path}` must not contact the Speech Provider, but the mock saw {records:?}"
    );
}

/// 生成一个已接受的 Cached Speech Clip，返回完整 clip ID。
fn generate_cached_clip(
    fixture: &Fixture,
    annotation_id: &str,
    content: &str,
    trace_id: &str,
) -> String {
    let provider = provider_with_catalog_and_synthesis(trace_id);
    let value = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", annotation_id,
            "--content", content, "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();
    assert!(
        !records.is_empty(),
        "setup generation must reach the Speech Provider at least once"
    );
    value["receipt"]["clip_id"]
        .as_str()
        .expect("clip id")
        .to_string()
}

/// 断言一份 Machine JSON 文档带有版本号，且不泄漏原文、密钥或音频 hex。
fn assert_versioned_and_secret_free(document: &Value, context: &str) {
    assert_eq!(
        document["schema_version"], 1,
        "{context} must carry the versioned Machine JSON schema"
    );
    let text = serde_json::to_string(document).expect("serialize document");
    for forbidden in [
        TEST_KEY,
        SECRET_CANARY,
        "SENSEAUDIO_API_KEY\":",
        "Authorization",
        "Bearer ",
    ] {
        assert!(
            !text.contains(forbidden),
            "{context} leaked `{forbidden}`: {text}"
        );
    }
}

/// 断言一份错误信封的 code 在稳定集合内，且带可执行的 remediation。
fn assert_stable_error_envelope(error: &Value, context: &str) {
    assert_eq!(
        error["schema_version"], 1,
        "{context} must carry the versioned Machine JSON schema"
    );
    let code = error["error"]["code"]
        .as_str()
        .unwrap_or_else(|| panic!("{context} must have a stable error code"));
    assert!(
        code.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
        "{context} error code `{code}` is not a stable SCREAMING_SNAKE_CASE identifier"
    );
    assert!(
        !code.is_empty(),
        "{context} must have a non-empty error code"
    );
    let remediation = error["error"]["remediation"]
        .as_str()
        .unwrap_or_else(|| panic!("{context} error `{code}` must carry actionable remediation"));
    assert!(
        remediation.len() > 20,
        "{context} error `{code}` remediation is not actionable: {remediation}"
    );
    let message = error["error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("{context} error `{code}` must have a message"));
    for forbidden in [TEST_KEY, SECRET_CANARY, "Authorization", "Bearer "] {
        assert!(
            !message.contains(forbidden),
            "{context} error message leaked `{forbidden}`: {message}"
        );
    }
}

// ---------------------------------------------------------------------------
// criterion 1：每个叶命令都有人类 help 与版本化 Machine JSON 合同
// ---------------------------------------------------------------------------

/// 实施 spec 5.1 的命令族全集。漏掉任何一条叶命令，这条测试就该失败。
const LEAF_COMMANDS: &[&[&str]] = &[
    &["speech", "voices"],
    &["speech", "profile", "show"],
    &["speech", "profile", "set"],
    &["speech", "profile", "reset"],
    &["speech", "generate"],
    &["speech", "play"],
    &["speech", "export"],
    &["speech", "cache", "status"],
    &["speech", "cache", "clear"],
    &["speech", "history", "clear"],
];

/// 每条叶命令都必须有真实的人类 help：退出码 0、非空正文、描述了 `--json`。
#[test]
fn every_speech_leaf_command_has_human_help_and_a_json_flag() {
    for command in LEAF_COMMANDS {
        let mut args: Vec<&str> = command.to_vec();
        args.push("--help");
        let output = Command::new(env!("CARGO_BIN_EXE_apple-books-exporter"))
            .args(&args)
            .output()
            .expect("run --help");

        let label = command.join(" ");
        assert!(
            output.status.success(),
            "`{label} --help` must exit 0, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let help = String::from_utf8_lossy(&output.stdout);
        assert!(
            help.contains("--json"),
            "`{label} --help` must document the machine JSON flag, got:\n{help}"
        );
        assert!(
            help.lines().any(|line| line.starts_with("Usage:")),
            "`{label} --help` must show a usage line, got:\n{help}"
        );
    }
}

/// 每个叶命令的失败路径都必须给出**同一份**版本化错误信封：stdout 空、只有 stderr
/// 是带 `code`/`message`/`remediation` 的 JSON。
///
/// `speech cache status` / `speech history clear` 不在此列：它们是纯本地维护命令，
/// 在没有任何凭证时也必须成功（它们的成功信封由下一条测试覆盖）。把它们写进失败表
/// 会把「本地操作不需要联网」这个正确行为误判成缺陷。
#[test]
fn every_speech_leaf_command_fails_with_a_versioned_error_envelope() {
    // 每个叶命令都能在**不联网、不需要有效 clip** 的前提下触发的失败：
    // 要么是缺少必需参数，要么是语法非法的 clip ID。
    let cases: &[(&[&str], &str)] = &[
        (&["speech", "voices", "--json"], "speech voices"),
        (&["speech", "profile", "set", "--json"], "speech profile set"),
        (&["speech", "generate", "--json"], "speech generate"),
        (&["speech", "play", "--clip-id", "not-a-clip", "--json"], "speech play"),
        (
            &[
                "speech", "export", "--clip-id", "not-a-clip", "--output", "/tmp/does-not-matter",
                "--json",
            ],
            "speech export",
        ),
    ];

    for (args, label) in cases {
        let fixture = Fixture::new();
        let output = fixture.run(args);
        let error = failed(&output);
        assert_stable_error_envelope(&error, label);
    }
}

/// 成功路径的成功信封也必须带版本号；这里覆盖**每个**会成功的叶命令形状。
#[test]
fn every_speech_success_receipt_is_versioned_and_keeps_streams_separate() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-29-ok");

    // 纯本地、不需要凭证的成功命令。
    for args in [
        vec!["speech", "profile", "show", "--json"],
        vec!["speech", "profile", "reset", "--json"],
        vec!["speech", "cache", "status", "--json"],
        vec!["speech", "history", "clear", "--json"],
        vec!["speech", "play", "--clip-id", &clip_id, "--json"],
    ] {
        let label = args.join(" ");
        let value = succeeded(&fixture.run(&args));
        assert_versioned_and_secret_free(&value, &label);
    }

    // 显式 remote generation：只有这一条会真的联系 provider。
    let provider = provider_with_catalog_and_synthesis("trace-29-generate");
    let value = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-42",
            "--content", "note", "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    assert!(!provider.finish().is_empty());
    assert_versioned_and_secret_free(&value, "speech generate (note)");

    // 显式 export：本地复制，不联系 provider。
    let provider = provider_with_catalog_and_synthesis("trace-29-export");
    let root = fixture.book_export_root();
    let value = succeeded(&fixture.run_with(
        &[
            "speech", "export", "--clip-id", &clip_id, "--output",
            root.to_str().expect("export root"), "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech export");
    assert_versioned_and_secret_free(&value, "speech export");

    // `speech voices` 是唯一会联网的浏览命令。用**全新** fixture，否则前面的显式生成
    // 已经把 24 小时 Voice Catalog 缓存写好，第二次调用会合法地复用缓存而不再联网。
    let fresh = Fixture::new();
    let provider = provider_with_catalog_and_synthesis("trace-29-voices");
    let value = succeeded(&fresh.run_with(&["speech", "voices", "--json"], &provider, Some(TEST_KEY)));
    assert!(!provider.finish().is_empty());
    assert_versioned_and_secret_free(&value, "speech voices");
}

// ---------------------------------------------------------------------------
// criterion 2：稳定错误码 + 可执行 remediation + 不泄漏
// ---------------------------------------------------------------------------

/// 播放被用户自己改坏的导出音频时，必须返回缓存损坏这一**稳定**码，而不是任何
/// 随文件内容变化的字符串。
#[test]
fn a_tampered_entry_is_a_stable_code_rather_than_a_varying_one() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-29-tamper");

    // 先确认这份 clip 确实可播放，再破坏它。
    succeeded(&fixture.run(&["speech", "play", "--clip-id", &clip_id, "--json"]));

    let state_path = fixture
        .speech_root()
        .join("clips")
        .join(&clip_id)
        .join("state.json");
    let state: Value =
        serde_json::from_str(&std::fs::read_to_string(&state_path).expect("state.json"))
            .expect("state JSON");
    let audio_path = fixture
        .speech_root()
        .join("clips")
        .join(&clip_id)
        .join("versions")
        .join(state["current_audio_sha256"].as_str().expect("audio sha256"))
        .join("audio.mp3");

    let mut bytes = std::fs::read(&audio_path).expect("read audio");
    bytes[0] ^= 0xFF; // 破坏帧头：checksum 与 MP3 可解析性同时失效。
    std::fs::write(&audio_path, &bytes).expect("tamper audio");

    // 同一个损坏 entry 必须每次都返回同一个稳定码。
    for _ in 0..3 {
        let error = failed(&fixture.run(&["speech", "play", "--clip-id", &clip_id, "--json"]));
        assert_stable_error_envelope(&error, "speech play (tampered)");
        assert_eq!(
            error["error"]["code"], "SPEECH_CACHE_CORRUPT",
            "a tampered cache entry must report the documented stable code"
        );
    }
}

/// 错误信封可以带 `details`（provider、trace、失败原因），但 `details` 与 `message`
/// 都不得包含原文、密钥或供应商原始响应。
#[test]
fn error_details_carry_diagnostics_without_source_text_or_secrets() {
    let fixture = Fixture::new();
    // 显式 remote 生成，但 provider 凭证无效 → 稳定鉴权失败。
    let provider = provider_with_catalog_and_synthesis("trace-29-auth");
    let output = fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--regenerate", "--json",
        ],
        &provider,
        // 空字符串 = 配置的环境变量存在但没有值。
        Some(""),
    );
    let _ = provider.finish();
    let error = failed(&output);
    assert_stable_error_envelope(&error, "speech generate (missing key)");

    let text = serde_json::to_string(&error).expect("serialize error");
    for forbidden in [
        TEST_KEY,
        SECRET_CANARY,
        HIGHLIGHT_TEXT,
        "Authorization",
        "Bearer ",
        "base_resp",
    ] {
        assert!(
            !text.contains(forbidden),
            "error envelope leaked `{forbidden}`: {text}"
        );
    }
}

// ---------------------------------------------------------------------------
// criterion 3：负向控制
// ---------------------------------------------------------------------------

/// 内容选择的负向控制：note-only / highlight-only 的另一侧必须稳定失败，且不联网。
#[test]
fn content_selection_refuses_the_side_an_annotation_does_not_have() {
    let fixture = Fixture::new();
    // annotation-41 只有高亮，没有笔记。
    let provider = provider_with_catalog_and_synthesis("trace-29-content");
    let error = failed(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "note", "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech generate (missing note)");
    assert_stable_error_envelope(&error, "speech generate (missing note)");
    assert_eq!(error["error"]["code"], "SPEECH_CONTENT_UNAVAILABLE");

    // 属于另一本书的 annotation ID 必须失败。
    let provider = provider_with_catalog_and_synthesis("trace-29-wrongbook");
    let error = failed(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-999",
            "--content", "highlight", "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech generate (unknown annotation)");
    assert_eq!(error["error"]["code"], "INVALID_ANNOTATION_ID");
}

/// 缓存命中 + 播放 + 导出 + 缓存维护的负向控制：每一条都必须是**零** provider 连接。
#[test]
fn only_explicit_generation_and_voice_browsing_may_contact_the_provider() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-29-zero");

    // 缓存命中：同样的请求不得再产生一次计费调用。
    let provider = provider_with_catalog_and_synthesis("trace-29-hit");
    let value = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech generate (cache hit)");
    assert_eq!(value["receipt"]["source"], "cache");
    assert_eq!(value["receipt"]["provider_called"], false);

    // 播放（机器模式不启动播放器，也不联网）。
    let provider = provider_with_catalog_and_synthesis("trace-29-play");
    succeeded(&fixture.run_with(
        &["speech", "play", "--clip-id", &clip_id, "--json"],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech play");

    // 导出：只复制本地已验证音频。
    let root = fixture.book_export_root();
    let provider = provider_with_catalog_and_synthesis("trace-29-export");
    succeeded(&fixture.run_with(
        &[
            "speech", "export", "--clip-id", &clip_id, "--output",
            root.to_str().expect("export root"), "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech export");

    // 缓存与历史维护是纯本地动作。
    for args in [
        vec!["speech", "cache", "status", "--json"],
        vec!["speech", "cache", "clear", "--json"],
        vec!["speech", "history", "clear", "--json"],
    ] {
        let provider = provider_with_catalog_and_synthesis("trace-29-maint");
        let _ = fixture.run_with(&args, &provider, Some(TEST_KEY));
        assert_zero_connections(&provider.finish(), &args.join(" "));
    }
}

/// 导出包含性的负向控制：manifest 里的 `../` 相对路径不得让导出读或写到导出根之外。
#[test]
fn a_manifest_traversal_path_never_escapes_the_export_root() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-29-traverse");
    let root = fixture.book_export_root();
    succeeded(&fixture.run(&[
        "speech", "export", "--clip-id", &clip_id, "--output",
        root.to_str().expect("export root"), "--json",
    ]));

    // 根目录外的一个诱饵文件：任何越界读取都会在这里暴露。
    let outside = fixture.home.path().join("outside-canary.mp3");
    std::fs::write(&outside, silent_mp3(2)).expect("write outside canary");
    let outside_before = std::fs::read(&outside).expect("read outside canary");

    // 把 manifest 里的相对路径改成目录穿越。清单结构是
    // `records[].clips[]`：`records` 按 Annotation + 内容部分分组。
    let manifest_path = fixture.manifest_path();
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).expect("manifest.json"))
            .expect("manifest JSON");
    manifest["records"][0]["clips"][0]["relative_path"] = json!("../../../outside-canary.mp3");
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest).expect("serialize"))
        .expect("write manifest");

    // 常规 Markdown 导出必须成功但省略该链接，而不是跟随路径穿越。
    // 音频链接与 warning 都在 ExportReceipt 顶层（`receipt.warnings` /
    // `receipt.audio_links`），因为常规导出的主文档成功与否与语音无关。
    let provider = provider_with_catalog_and_synthesis("trace-29-traverse-md");
    let value = succeeded(&fixture.run_with(
        &[
            "export", "--asset-id", "book-1", "--format", "markdown", "--output",
            root.parent().expect("export parent").to_str().expect("parent"),
            "--overwrite", "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "export (traversal manifest)");

    let warnings: Vec<(String, String)> = value["receipt"]["warnings"]
        .as_array()
        .expect("receipt warnings array")
        .iter()
        .map(|warning| {
            (
                warning["code"].as_str().unwrap_or_default().to_string(),
                warning["reason"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    assert!(
        warnings
            .iter()
            .any(|(code, reason)| code == "SPEECH_AUDIO_LINK_OMITTED"
                && reason == "path_outside_export_root"),
        "a traversing manifest path must be reported as an omitted link for escaping the root, \
         got {warnings:?}"
    );
    assert!(
        value["receipt"]["audio_links"]
            .as_array()
            .map(|links| links.is_empty())
            .unwrap_or(true),
        "a traversing manifest path must never become an audio link: {value}"
    );

    // 根目录外的内容不得被读取或改写。
    assert_eq!(
        std::fs::read(&outside).expect("read outside canary"),
        outside_before,
        "the export must not read or write outside the chosen export root"
    );
    let markdown = std::fs::read_to_string(root.join("测试书.md")).expect("main note");
    assert!(
        !markdown.contains("outside-canary"),
        "the traversing path must never be linked from the Markdown export"
    );
}

/// 清单权威性的负向控制：损坏的 manifest 不得被猜测重建，也不得被覆盖。
#[test]
fn a_corrupt_manifest_is_never_rebuilt_or_guessed() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-29-manifest");
    let root = fixture.book_export_root();
    succeeded(&fixture.run(&[
        "speech", "export", "--clip-id", &clip_id, "--output",
        root.to_str().expect("export root"), "--json",
    ]));

    let manifest_path = fixture.manifest_path();
    let corrupt = b"{ this is not a valid speech export manifest";
    std::fs::write(&manifest_path, corrupt).expect("corrupt manifest");

    let error = failed(&fixture.run(&[
        "speech", "export", "--clip-id", &clip_id, "--output",
        root.to_str().expect("export root"), "--json",
    ]));
    assert_stable_error_envelope(&error, "speech export (corrupt manifest)");
    assert_eq!(error["error"]["code"], "SPEECH_EXPORT_MANIFEST_INVALID");

    assert_eq!(
        std::fs::read(&manifest_path).expect("read manifest"),
        corrupt.to_vec(),
        "a corrupt manifest must be left untouched, never rebuilt or guessed"
    );
}

/// 秘密安全的负向控制：任何 Speech 落盘状态与任何命令输出都不得包含 API Key。
#[test]
fn no_speech_state_or_command_output_ever_contains_the_api_key() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-29-secret");
    let root = fixture.book_export_root();
    succeeded(&fixture.run(&[
        "speech", "export", "--clip-id", &clip_id, "--output",
        root.to_str().expect("export root"), "--json",
    ]));

    // 隔离 HOME 下每一个 Speech 状态文件都不含密钥值。
    for (relative, _) in fixture.speech_root_files() {
        let path = fixture.speech_root().join(&relative);
        let bytes = std::fs::read(&path).expect("read state file");
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains(TEST_KEY),
            "{} leaked the API key",
            relative.display()
        );
    }

    // 人类与机器输出同样不得泄漏。
    for args in [
        vec!["speech", "profile", "show", "--json"],
        vec!["speech", "play", "--clip-id", &clip_id, "--json"],
        vec!["speech", "cache", "status", "--json"],
    ] {
        let output = fixture.run(&args);
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !text.contains(TEST_KEY),
            "`{}` leaked the API key: {text}",
            args.join(" ")
        );
    }
}

// ---------------------------------------------------------------------------
// criterion 4：既有非 Speech 命令不获得任何隐式 Speech 行为
// ---------------------------------------------------------------------------

/// `list`、`annotations`、`export`、`doctor` 都是只读本地命令：即使注入了
/// `SENSEAUDIO_API_BASE_URL` 与 API Key，它们也**不创建** Speech 状态、不联系 provider、
/// 不返回任何 speech 字段。
///
/// 缺少这条负向控制时，「读取标注悄悄触发 TTS」这种回归不会被任何测试发现。
#[test]
fn existing_read_only_commands_gain_no_implicit_speech_behavior() {
    let fixture = Fixture::new();
    let export_root = fixture.home.path().join("plain-export");
    let output_dir = export_root.to_str().expect("export dir").to_string();

    let commands: Vec<Vec<&str>> = vec![
        vec!["list", "--json"],
        vec!["annotations", "--asset-id", "book-1", "--json"],
        vec![
            "export", "--asset-id", "book-1", "--format", "markdown", "--output", &output_dir,
            "--json",
        ],
        vec!["doctor", "--json"],
    ];

    for args in commands {
        let label = args.join(" ");
        // 给足凭证，让「隐式联网」成为可能——正因如此，零连接才是有意义的断言。
        let provider = provider_with_catalog_and_synthesis("trace-29-implicit");
        let output = fixture.run_with(&args, &provider, Some(TEST_KEY));
        let records = provider.finish();
        assert_zero_connections(&records, &label);

        let value = succeeded(&output);
        assert_versioned_and_secret_free(&value, &label);
        // `list` / `annotations` / `doctor` 不允许出现任何 Speech 领域标识。
        // `export` 例外：它本来就有一等公民的 `receipt.audio_links` 与
        // `receipt.warnings`（ADR 0007 要求缺失音频时给结构化 warning），但
        // **不得**出现 clip_id / attempt_id / provider receipt 这类生成期身份，
        // 也不得联系 provider——这两点在本测试里分别由上面与下面断言。
        if label.starts_with("export") {
            let receipt_text =
                serde_json::to_string(&value["receipt"]).expect("serialize receipt");
            for forbidden in ["clip_id", "attempt_id", "provider_called", "source"] {
                assert!(
                    !receipt_text.contains(forbidden),
                    "`{label}` must not expose Speech generation identity: {receipt_text}"
                );
            }
        } else {
            let text = serde_json::to_string(&value).expect("serialize");
            assert!(
                !text.contains("SPEECH_") && !text.contains("speech"),
                "`{label}` must not leak any Speech domain identifier: {text}"
            );
        }
    }

    // 整轮跑完，隔离 HOME 下不得存在任何 Speech 状态目录。
    assert!(
        !fixture.speech_root().exists(),
        "read-only commands must not create the Speech state root at {}",
        fixture.speech_root().display()
    );
}

/// 同上，但覆盖**人类**模式：`list` 与 `annotations` 的表格输出也必须完全不碰 Speech。
#[test]
fn human_read_only_commands_create_no_speech_state() {
    let fixture = Fixture::new();
    for args in [vec!["list"], vec!["annotations", "--asset-id", "book-1"]] {
        let label = args.join(" ");
        let output = fixture.run(&args);
        assert!(
            output.status.success(),
            "`{label}` must succeed, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(
            !text.contains("SPEECH_") && !text.contains("Speech"),
            "`{label}` human output must not mention Speech: {text}"
        );
    }
    assert!(
        !fixture.speech_root().exists(),
        "human read-only commands must not create the Speech state root"
    );
}
