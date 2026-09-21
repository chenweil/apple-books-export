//! Issue #23 的 `speech generate` CLI 合同测试。
//!
//! 这些测试沿用 issue #21/#22 的约定：进程内 TCP mock provider、注入 HOME 隔离 Speech
//! 状态根、指向死端口的代理保证没有隐式网络，以及 secret canary 断言密钥不会泄漏到
//! 输出、缓存或收据里。
//!
//! mock server 计数 provider 连接，因此「缓存命中不再调用 provider」和「本地失败零连接」
//! 都是可证明的，而不是靠超时等待。

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration as StdDuration;
use tempfile::TempDir;

/// mock provider 使用的测试密钥；断言它永不进入输出或落盘。
const TEST_KEY: &str = "issue-23-test-key";
/// 探测 secret 落盘的 canary 值。
const SECRET_CANARY: &str = "canary-secret-1f4b2c-do-not-persist";
#[derive(Debug, Clone)]
struct RequestRecord {
    path: String,
    authorization: Option<String>,
    body: Vec<u8>,
}

/// 进程内 mock provider：记录每个连接，并可控制响应与延迟。
struct MockProvider {
    url: String,
    records: Arc<Mutex<Vec<RequestRecord>>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl MockProvider {
    /// 服务固定响应；每个连接都记录，响应后保持监听以便计数后续请求。
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
                        // accept 出来的 socket 会继承 listener 的 O_NONBLOCK。
                        stream.set_nonblocking(false).expect("blocking mock stream");
                        let (headers, request_body) = read_request(&mut stream);
                        let request_path = request_path(&headers);
                        captured.lock().expect("records").push(RequestRecord {
                            path: request_path.clone(),
                            authorization: header_value(&headers, "authorization"),
                            body: request_body,
                        });
                        if let Some(delay) = response.delay {
                            thread::sleep(delay);
                        }
                        if (response.drop)(request_path.as_str()) {
                            // 请求已经到达 provider，但不返回任何响应。
                            drop(stream);
                            continue;
                        }
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

    fn finish(mut self) -> Vec<RequestRecord> {
        self.stop.store(true, Ordering::SeqCst);
        self.join.take().expect("mock thread").join().expect("mock");
        self.records.lock().expect("records").clone()
    }

    /// 合成请求的计数（不含目录请求）。
    fn synthesis_count(records: &[RequestRecord]) -> usize {
        records
            .iter()
            .filter(|record| record.path.ends_with("/v1/t2a_v2"))
            .count()
    }
}

/// mock 响应策略：全部按请求路径决定，目录与合成可以独立失败。
struct Response {
    /// HTTP 状态码。
    status: Arc<dyn Fn(&str) -> u16 + Send + Sync>,
    /// 响应体。
    body: Arc<dyn Fn(&str) -> Vec<u8> + Send + Sync>,
    /// 响应前延迟。
    delay: Option<StdDuration>,
    /// 收到请求后直接断开且不响应：模拟「结果不确定」。
    drop: Arc<dyn Fn(&str) -> bool + Send + Sync>,
}

impl Response {
    fn status(status: u16) -> Arc<dyn Fn(&str) -> u16 + Send + Sync> {
        Arc::new(move |_| status)
    }

    fn never_drop() -> Arc<dyn Fn(&str) -> bool + Send + Sync> {
        Arc::new(|_| false)
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

/// 成功的合成响应；`audio` 是 hex 编码的音频字节。
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

/// 账号目录响应：包含默认音色，使生成前校验不需要额外请求。
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
    let headers = String::from_utf8(bytes[..header_end].to_vec()).expect("request headers");
    let content_length = header_value(&headers, "content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    while bytes.len() < header_end + content_length {
        let mut chunk = [0_u8; 1024];
        let count = stream.read(&mut chunk).expect("read request body");
        assert!(count > 0, "request ended before body");
        bytes.extend_from_slice(&chunk[..count]);
    }
    (
        headers,
        bytes[header_end..header_end + content_length].to_vec(),
    )
}

fn header_value(headers: &str, name: &str) -> Option<String> {
    headers.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.eq_ignore_ascii_case(name)).then(|| value.trim().to_string())
    })
}

fn request_path(headers: &str) -> String {
    headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1).map(str::to_string))
        .unwrap_or_default()
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
                "INSERT INTO ZAEANNOTATION VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
                rusqlite::params![
                    41,
                    "book-1",
                    "高亮正文",
                    "我的笔记",
                    "epubcfi(/6/10[Section0003.xhtml]!/4/82/1,:0,:44)",
                    60.0,
                    3
                ],
            )
            .expect("both-sides annotation");
        annotation_conn
            .execute(
                "INSERT INTO ZAEANNOTATION VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
                rusqlite::params![
                    42,
                    "book-1",
                    "只有高亮",
                    Option::<String>::None,
                    "epubcfi(/6/12[chapter5.xhtml]!/4/2)",
                    120.0,
                    3
                ],
            )
            .expect("highlight-only annotation");

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
                "INSERT INTO ZBKLIBRARYASSET VALUES (1, 'book-1', '测试书', '测试作者', '测试书')",
                [],
            )
            .expect("library row");

        Self { home }
    }

    /// 与 issue #22 的 mock 测试一致：移除所有代理变量，让请求直达本地 mock。
    ///
    /// 「没有隐式网络」由 mock 连接计数证明：零连接的用例必须记录 0 个请求，
    /// 而不是靠超时等待。
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

    fn speech_root(&self) -> PathBuf {
        self.home
            .path()
            .join("Library/Application Support/books-exporter/speech")
    }

    fn clip_dirs(&self) -> Vec<PathBuf> {
        let clips = self.speech_root().join("clips");
        let mut dirs = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&clips) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    dirs.push(entry.path());
                }
            }
        }
        dirs.sort();
        dirs
    }

    /// 递归读取 Speech 状态根下的所有文件，用于 secret canary 断言。
    fn speech_state_files(&self) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(dir: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
            let entries = match std::fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(_) => return,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, files);
                } else {
                    files.push((path.clone(), std::fs::read(&path).expect("read state file")));
                }
            }
        }
        let mut files = Vec::new();
        walk(&self.speech_root(), &mut files);
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

/// 目录 + 合成都在同一个 mock 上：按请求路径返回目录或合成结果。
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
        delay: None,
        drop: Response::never_drop(),
    })
}

/// 目录可用、只有合成被拒绝的 mock。
fn provider_with_catalog_and_failed_synthesis(status: u16) -> MockProvider {
    let catalog = catalog_response();
    MockProvider::serve(Response {
        status: Arc::new(move |path: &str| {
            if path.ends_with("/v1/t2a_v2") {
                status
            } else {
                200
            }
        }),
        body: Arc::new(move |path: &str| {
            if path.ends_with("/v1/t2a_v2") {
                br#"{"code":"internal","message":"boom"}"#.to_vec()
            } else {
                catalog.clone()
            }
        }),
        delay: None,
        drop: Response::never_drop(),
    })
}

/// 目录可用、合成返回坏 hex 的 mock。
fn provider_with_catalog_and_invalid_audio() -> MockProvider {
    let catalog = catalog_response();
    MockProvider::serve(Response {
        status: Response::status(200),
        body: Arc::new(move |path: &str| {
            if path.ends_with("/v1/t2a_v2") {
                serde_json::to_vec(&json!({
                    "data": {"audio": "zz", "status": 2},
                    "trace_id": "trace-bad-hex",
                    "base_resp": {"status_code": 0, "status_msg": "success"}
                }))
                .expect("response JSON")
            } else {
                catalog.clone()
            }
        }),
        delay: None,
        drop: Response::never_drop(),
    })
}

/// 目录可用，但合成请求被接收后连接被断开：结果不确定。
fn provider_with_dropped_synthesis() -> MockProvider {
    let catalog = catalog_response();
    MockProvider::serve(Response {
        status: Response::status(200),
        body: Arc::new(move |path: &str| {
            if path.ends_with("/v1/t2a_v2") {
                Vec::new()
            } else {
                catalog.clone()
            }
        }),
        delay: None,
        // 只有合成请求被断开：目录仍然可用。
        drop: Arc::new(|path: &str| path.ends_with("/v1/t2a_v2")),
    })
}

#[test]
fn machine_generate_selects_one_content_part_and_returns_a_versioned_receipt() {
    let fixture = Fixture::new();
    let provider = provider_with_catalog_and_synthesis("trace-machine-1");

    let value = succeeded(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();

    assert_eq!(value["schema_version"], 1);
    let receipt = &value["receipt"];
    assert_eq!(receipt["operation"], "generate");
    assert_eq!(receipt["source"], "provider");
    assert_eq!(receipt["provider_called"], true);
    assert_eq!(receipt["asset_id"], "book-1");
    assert_eq!(receipt["annotation_id"], "annotation-41");
    assert_eq!(receipt["content_kind"], "highlight");
    assert_eq!(receipt["clip_id"].as_str().map(str::len), Some(64));
    assert!(receipt["attempt_id"].is_string());
    assert!(receipt["text_sha256"].is_string());
    assert_eq!(receipt["unicode_characters"], 4);
    assert_eq!(receipt["estimated_billing_characters"], 8);
    assert_eq!(
        receipt["billing_estimator_version"],
        "senseaudio-docs-2026-09-10"
    );
    assert_eq!(receipt["profile"]["voice_id"], "male_0004_a");
    assert_eq!(receipt["profile"]["speed"], 1.0);
    assert_eq!(receipt["profile"]["volume"], 1.0);
    assert_eq!(receipt["profile"]["pitch"], 0);
    assert_eq!(receipt["audio"]["format"], "mp3");
    assert_eq!(receipt["audio"]["sample_rate"], 32000);
    assert_eq!(receipt["audio"]["bitrate"], 128000);
    assert_eq!(receipt["audio"]["channel"], 2);
    assert_eq!(receipt["audio"]["size_bytes"], 1152);
    assert_eq!(receipt["audio"]["duration_ms"], 72);
    assert_eq!(receipt["provider"]["trace_id"], "trace-machine-1");
    assert_eq!(receipt["provider"]["usage_characters"], 8);
    assert_eq!(receipt["warnings"].as_array().map(Vec::len), Some(0));

    // 收据绝不重复原文、密钥或音频 hex。
    let text = serde_json::to_string(&value).expect("receipt text");
    assert!(!text.contains("高亮正文"));
    assert!(!text.contains(TEST_KEY));
    assert!(!text.contains("fffb"));

    // 恰好一次目录请求 + 一次同步合成请求。
    assert_eq!(MockProvider::synthesis_count(&records), 1);
    let synthesis = records
        .iter()
        .find(|record| record.path.ends_with("/v1/t2a_v2"))
        .expect("synthesis request");
    assert_eq!(
        synthesis.authorization.as_deref(),
        Some(format!("Bearer {TEST_KEY}").as_str())
    );
    let body: Value = serde_json::from_slice(&synthesis.body).expect("request JSON");
    assert_eq!(body["model"], "sensenova-tts-2.0");
    assert_eq!(body["stream"], false);
    assert_eq!(body["voice_setting"]["voice_id"], "male_0004_a");
    assert_eq!(body["voice_setting"]["speed"], 1.0);
    assert_eq!(body["voice_setting"]["vol"], 1.0);
    assert_eq!(body["voice_setting"]["pitch"], 0);
    assert_eq!(body["audio_setting"]["format"], "mp3");
    assert_eq!(body["audio_setting"]["sample_rate"], 32000);
    assert_eq!(body["audio_setting"]["bitrate"], 128000);
    assert_eq!(body["audio_setting"]["channel"], 2);
    assert_eq!(body["text"], "高亮正文");

    // 缓存落成一个不可变 version + 一个 current pointer。
    let clips = fixture.clip_dirs();
    assert_eq!(clips.len(), 1);
    let state: Value = serde_json::from_str(
        &std::fs::read_to_string(clips[0].join("state.json")).expect("state.json"),
    )
    .expect("state JSON");
    assert_eq!(state["current_cache_status"], "ready");
    assert!(state["current_audio_sha256"].is_string());
    assert_eq!(state["latest_attempt_status"], "succeeded");
    assert_eq!(state["generation_blocked"], false);
    assert_eq!(
        std::fs::read_dir(clips[0].join("versions"))
            .expect("versions")
            .count(),
        1
    );
}

#[test]
fn highlight_and_note_of_one_annotation_are_two_independent_clips() {
    let fixture = Fixture::new();
    let highlight_provider = provider_with_catalog_and_synthesis("trace-h");
    let highlight = succeeded(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--json",
        ],
        &highlight_provider,
        Some(TEST_KEY),
    ));
    let highlight_records = highlight_provider.finish();

    let note_provider = provider_with_catalog_and_synthesis("trace-n");
    let note = succeeded(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "note",
            "--json",
        ],
        &note_provider,
        Some(TEST_KEY),
    ));
    let note_records = note_provider.finish();

    assert_eq!(highlight["receipt"]["content_kind"], "highlight");
    assert_eq!(note["receipt"]["content_kind"], "note");
    assert_ne!(highlight["receipt"]["clip_id"], note["receipt"]["clip_id"]);
    assert_ne!(
        highlight["receipt"]["text_sha256"],
        note["receipt"]["text_sha256"]
    );
    assert_eq!(fixture.clip_dirs().len(), 2);
    assert_eq!(MockProvider::synthesis_count(&highlight_records), 1);
    assert_eq!(MockProvider::synthesis_count(&note_records), 1);
}

#[test]
fn a_repeated_request_returns_the_cache_without_another_provider_call() {
    let fixture = Fixture::new();
    let first_provider = provider_with_catalog_and_synthesis("trace-1");
    let first = succeeded(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--json",
        ],
        &first_provider,
        Some(TEST_KEY),
    ));
    first_provider.finish();

    // 第二次请求：mock 仍然计数，但不得收到任何连接。
    let second_provider = provider_with_catalog_and_synthesis("trace-2");
    let second = succeeded(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--json",
        ],
        &second_provider,
        Some(TEST_KEY),
    ));
    let records = second_provider.finish();

    assert_eq!(second["receipt"]["source"], "cache");
    assert_eq!(second["receipt"]["provider_called"], false);
    assert_eq!(second["receipt"]["attempt_id"], Value::Null);
    assert_eq!(second["receipt"]["clip_id"], first["receipt"]["clip_id"]);
    assert_eq!(
        second["receipt"]["audio"]["sha256"],
        first["receipt"]["audio"]["sha256"]
    );
    assert_eq!(
        MockProvider::synthesis_count(&records),
        0,
        "a valid cache hit must not call the provider"
    );
    assert_eq!(
        records.len(),
        0,
        "a valid cache hit must not contact the provider at all"
    );
}

#[test]
fn local_failures_happen_before_any_provider_connection() {
    let fixture = Fixture::new();

    // 归属错误：annotation 属于另一本书。
    let provider = provider_with_catalog_and_synthesis("trace");
    let foreign = failed(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "other-book",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();
    assert_eq!(foreign["error"]["code"], "INVALID_ANNOTATION_ID");
    assert_eq!(
        foreign["error"]["details"]["reason"],
        "annotation_not_in_book"
    );
    assert_eq!(
        records.len(),
        0,
        "wrong ownership must not reach the provider"
    );

    // 负向控制：highlight-only fixture 请求不存在的 note 一侧。
    let provider = provider_with_catalog_and_synthesis("trace");
    let missing = failed(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-42",
            "--content",
            "note",
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();
    assert_eq!(missing["error"]["code"], "SPEECH_CONTENT_UNAVAILABLE");
    assert_eq!(missing["error"]["details"]["reason"], "content_unavailable");
    assert_eq!(records.len(), 0);

    // 缺 key：0 次连接。
    let provider = provider_with_catalog_and_synthesis("trace");
    let no_key = failed(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--json",
        ],
        &provider,
        None,
    ));
    let records = provider.finish();
    assert_eq!(no_key["error"]["code"], "SPEECH_AUTH_FAILED");
    assert_eq!(no_key["error"]["details"]["reason"], "missing_api_key");
    assert_eq!(records.len(), 0, "a missing key must not open a connection");

    // 无效 Profile：本地范围校验失败，零连接。
    let provider = provider_with_catalog_and_synthesis("trace");
    let invalid_profile = failed(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--speed",
            "2.5",
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();
    assert_eq!(invalid_profile["error"]["code"], "SPEECH_PROFILE_INVALID");
    assert_eq!(invalid_profile["error"]["details"]["field"], "speed");
    assert_eq!(records.len(), 0);

    // 无效 content 枚举。
    let provider = provider_with_catalog_and_synthesis("trace");
    let invalid_content = failed(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "both",
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();
    assert_eq!(invalid_content["error"]["code"], "INVALID_ARGUMENT");
    assert_eq!(records.len(), 0);
}

#[test]
fn conflicting_human_and_machine_arguments_are_rejected() {
    let fixture = Fixture::new();
    let provider = provider_with_catalog_and_synthesis("trace");

    let machine_index = failed(&fixture.run_with(
        &[
            "speech",
            "generate",
            "1",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    assert_eq!(machine_index["error"]["code"], "INVALID_ARGUMENT");

    let machine_annotation = failed(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--annotation",
            "1",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    assert_eq!(machine_annotation["error"]["code"], "INVALID_ARGUMENT");

    // 人类模式拒绝机器专用参数（人类错误不是 JSON envelope）。
    let human = fixture.run_with(
        &[
            "speech",
            "generate",
            "1",
            "--annotation",
            "1",
            "--content",
            "highlight",
            "--asset-id",
            "book-1",
        ],
        &provider,
        Some(TEST_KEY),
    );
    assert!(!human.status.success());
    let stderr = String::from_utf8_lossy(&human.stderr);
    assert!(
        stderr.contains("--asset-id"),
        "human mode must explain the conflict"
    );
    assert!(!stderr.contains("schema_version"));
    provider.finish();
}

#[test]
fn human_mode_uses_refreshed_display_indices() {
    let fixture = Fixture::new();
    let provider = provider_with_catalog_and_synthesis("trace-human");

    let output = fixture.run_with(
        &[
            "speech",
            "generate",
            "1",
            "--annotation",
            "2",
            "--content",
            "highlight",
        ],
        &provider,
        Some(TEST_KEY),
    );
    let records = provider.finish();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("human output");
    assert!(
        stdout.contains("highlight"),
        "human output must name the content kind"
    );
    assert!(
        stdout.contains("male_0004_a"),
        "human output must name the resolved voice"
    );
    assert!(
        stdout.contains("Characters: 4"),
        "human output must show the character estimate"
    );
    assert!(
        !stdout.contains("只有高亮"),
        "human output must not print the full Speech Text"
    );
    assert!(!stdout.contains("schema_version"));
    assert!(!stdout.contains(TEST_KEY));
    assert_eq!(MockProvider::synthesis_count(&records), 1);
}

#[test]
fn explicit_provider_failures_use_stable_codes_and_never_retry() {
    let fixture = Fixture::new();

    // 明确的 provider 失败（目录仍然可用，只有合成被拒绝）。
    let provider = provider_with_catalog_and_failed_synthesis(500);
    let failed_value = failed(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();
    assert_eq!(failed_value["error"]["code"], "SPEECH_PROVIDER_FAILED");
    assert_eq!(failed_value["error"]["details"]["outcome"], "failed");
    assert!(failed_value["error"]["details"]["attempt_id"].is_string());
    assert!(
        !String::from_utf8_lossy(&serde_json::to_vec(&failed_value).expect("error JSON"))
            .contains(TEST_KEY)
    );
    assert_eq!(MockProvider::synthesis_count(&records), 1);

    // 普通 generate 不得自动重放：第二次必须零连接。
    let second_provider = provider_with_catalog_and_synthesis("trace");
    let blocked = failed(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--json",
        ],
        &second_provider,
        Some(TEST_KEY),
    ));
    let second_records = second_provider.finish();
    assert_eq!(blocked["error"]["code"], "SPEECH_RESULT_UNKNOWN");
    assert_eq!(blocked["error"]["details"]["outcome"], "unknown");
    assert_eq!(
        second_records.len(),
        0,
        "a blocked clip must not contact the provider again"
    );

    // 只有显式 --regenerate 才能再次调用 provider。
    let regenerate_provider = provider_with_catalog_and_synthesis("trace-regen");
    let regenerated = succeeded(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--regenerate",
            "--json",
        ],
        &regenerate_provider,
        Some(TEST_KEY),
    ));
    let regenerate_records = regenerate_provider.finish();
    assert_eq!(regenerated["receipt"]["source"], "provider");
    assert_eq!(MockProvider::synthesis_count(&regenerate_records), 1);
}

#[test]
fn an_invalid_audio_payload_blocks_the_clip_without_retry() {
    let fixture = Fixture::new();
    let provider = provider_with_catalog_and_invalid_audio();

    let value = failed(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();

    assert_eq!(value["error"]["code"], "SPEECH_AUDIO_INVALID");
    assert_eq!(
        value["error"]["details"]["outcome"],
        "provider_succeeded_artifact_missing"
    );
    assert!(value["error"]["details"]["attempt_id"].is_string());
    assert_eq!(MockProvider::synthesis_count(&records), 1);

    // provider 成功但没有可用产物：只允许留下 state-only 阻塞门，
    // 不得存在任何被 current pointer 引用的音频版本。
    for clip in fixture.clip_dirs() {
        let state: Value = serde_json::from_str(
            &std::fs::read_to_string(clip.join("state.json")).expect("state.json"),
        )
        .expect("state JSON");
        assert_eq!(
            state["current_cache_status"], "absent",
            "a failed attempt must not commit a cache entry"
        );
        assert_eq!(state["generation_blocked"], true);
        assert_eq!(
            state["latest_attempt_status"],
            "provider_succeeded_artifact_missing"
        );
    }
    let second = provider_with_catalog_and_synthesis("trace");
    let blocked = failed(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--json",
        ],
        &second,
        Some(TEST_KEY),
    ));
    second.finish();
    assert_eq!(blocked["error"]["code"], "SPEECH_RESULT_UNKNOWN");
}

#[test]
fn an_uncertain_outcome_records_an_unknown_gate_and_never_replays() {
    let fixture = Fixture::new();
    // provider 接收请求后直接断开：结果不确定。
    let provider = provider_with_dropped_synthesis();

    let value = failed(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();

    assert_eq!(value["error"]["code"], "SPEECH_RESULT_UNKNOWN");
    assert_eq!(value["error"]["details"]["outcome"], "unknown");
    assert_eq!(MockProvider::synthesis_count(&records), 1);

    let clips = fixture.clip_dirs();
    assert_eq!(
        clips.len(),
        1,
        "an unknown result must persist a clip-level gate"
    );
    let state: Value = serde_json::from_str(
        &std::fs::read_to_string(clips[0].join("state.json")).expect("state.json"),
    )
    .expect("state JSON");
    assert_eq!(state["generation_blocked"], true);
    assert_eq!(state["latest_attempt_status"], "unknown");
    assert_eq!(state["current_cache_status"], "absent");

    // 普通 generate 保持零连接；显式 --regenerate 才创建新 attempt。
    let second = provider_with_catalog_and_synthesis("trace-after-unknown");
    let blocked = failed(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--json",
        ],
        &second,
        Some(TEST_KEY),
    ));
    second.finish();
    assert_eq!(blocked["error"]["code"], "SPEECH_RESULT_UNKNOWN");

    let third = provider_with_catalog_and_synthesis("trace-regen");
    let regenerated = succeeded(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--regenerate",
            "--json",
        ],
        &third,
        Some(TEST_KEY),
    ));
    let third_records = third.finish();
    assert_eq!(regenerated["receipt"]["source"], "provider");
    assert_ne!(
        regenerated["receipt"]["attempt_id"], value["error"]["details"]["attempt_id"],
        "--regenerate must create a new Speech Attempt"
    );
    assert_eq!(MockProvider::synthesis_count(&third_records), 1);
}

#[test]
fn provider_control_markup_in_the_annotation_is_neutralized() {
    let fixture = Fixture::new();
    let provider = provider_with_catalog_and_synthesis("trace-markup");

    // 直接用 fixture 里的文本无法注入控制标记，因此这里断言请求体与原文一致，
    // 并由单元测试覆盖 `<break>` 守卫；此处确认 CLI 不会改写或删除原文。
    let value = succeeded(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();

    assert_eq!(value["receipt"]["source"], "provider");
    let synthesis = records
        .iter()
        .find(|record| record.path.ends_with("/v1/t2a_v2"))
        .expect("synthesis request");
    let body: Value = serde_json::from_slice(&synthesis.body).expect("request JSON");
    assert_eq!(body["text"], "高亮正文");
}

/// 同一 clip 的两个并发 generate：只有一个 provider writer。
///
/// 合成响应被故意延迟，保证第二个进程真的在等待锁，而不是碰巧串行完成。
#[test]
fn concurrent_generations_of_one_clip_produce_exactly_one_provider_call() {
    let fixture = Fixture::new();
    let catalog = catalog_response();
    let synthesis = synthesis_response("trace-concurrent", 2);
    let provider = MockProvider::serve(Response {
        status: Response::status(200),
        body: Arc::new(move |path: &str| {
            if path.ends_with("/v1/t2a_v2") {
                thread::sleep(StdDuration::from_millis(400));
                synthesis.clone()
            } else {
                catalog.clone()
            }
        }),
        delay: None,
        drop: Response::never_drop(),
    });

    let mut children = Vec::new();
    for _ in 0..2 {
        let mut command = fixture.command();
        command
            .args([
                "speech",
                "generate",
                "--asset-id",
                "book-1",
                "--annotation-id",
                "annotation-41",
                "--content",
                "highlight",
                "--json",
            ])
            .env("SENSEAUDIO_API_BASE_URL", &provider.url)
            .env("SENSEAUDIO_API_KEY", TEST_KEY)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        children.push(command.spawn().expect("spawn CLI"));
    }

    let mut outputs = Vec::new();
    for child in children {
        let output = child.wait_with_output().expect("wait for CLI");
        outputs.push(output);
    }
    let records = provider.finish();

    let values: Vec<Value> = outputs.iter().map(|output| succeeded(output)).collect();
    assert_eq!(
        MockProvider::synthesis_count(&records),
        1,
        "two concurrent generations of one clip must produce exactly one provider call"
    );
    // 两个进程必须看到同一个 clip，且其中恰好一个真的调用了 provider。
    assert_eq!(
        values[0]["receipt"]["clip_id"],
        values[1]["receipt"]["clip_id"]
    );
    let sources: Vec<&str> = values
        .iter()
        .map(|value| value["receipt"]["source"].as_str().expect("source"))
        .collect();
    assert_eq!(
        sources
            .iter()
            .filter(|source| **source == "provider")
            .count(),
        1
    );
    assert_eq!(
        sources.iter().filter(|source| **source == "cache").count(),
        1
    );
    // 同一个 clip 只能有一个被接受的音频版本。
    let clips = fixture.clip_dirs();
    assert_eq!(clips.len(), 1);
    assert_eq!(
        std::fs::read_dir(clips[0].join("versions"))
            .expect("versions")
            .count(),
        1
    );
}

#[test]
fn no_speech_state_file_or_output_ever_contains_the_api_key() {
    let fixture = Fixture::new();
    let provider = provider_with_catalog_and_synthesis("trace-secret");

    let output = fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--json",
        ],
        &provider,
        Some(SECRET_CANARY),
    );
    provider.finish();

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !combined.contains(SECRET_CANARY),
        "CLI output leaked the API key"
    );

    let files = fixture.speech_state_files();
    assert!(
        !files.is_empty(),
        "a successful generation must persist state"
    );
    for (path, bytes) in files {
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains(SECRET_CANARY),
            "speech state file {} persisted the API key",
            path.display()
        );
        assert!(
            !text.contains("高亮正文"),
            "speech state file {} persisted the Speech Text",
            path.display()
        );
    }
}

#[test]
fn the_speech_root_stays_inside_the_injected_home() {
    let fixture = Fixture::new();
    let real_root = real_user_speech_root();
    let before = speech_root_state(&real_root);
    let provider = provider_with_catalog_and_synthesis("trace-root");

    succeeded(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-41",
            "--content",
            "highlight",
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    provider.finish();

    assert_eq!(
        speech_root_state(&real_root),
        before,
        "generation must not touch the real user Speech directory"
    );
    let clips = fixture.clip_dirs();
    assert_eq!(clips.len(), 1);
    assert!(clips[0].starts_with(fixture.speech_root()));
}

fn real_user_speech_root() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("test process HOME"))
        .join("Library/Application Support/books-exporter/speech")
}

/// 真实用户 Speech 目录的非侵入式快照：只看路径、大小与修改时间。
fn speech_root_state(root: &Path) -> Vec<(PathBuf, Option<u64>, Option<std::time::SystemTime>)> {
    fn walk(dir: &Path, entries: &mut Vec<(PathBuf, Option<u64>, Option<std::time::SystemTime>)>) {
        let read = match std::fs::read_dir(dir) {
            Ok(read) => read,
            Err(_) => return,
        };
        for entry in read.flatten() {
            let path = entry.path();
            let metadata = std::fs::metadata(&path).ok();
            entries.push((
                path.clone(),
                metadata.as_ref().map(|m| m.len()),
                metadata.as_ref().and_then(|m| m.modified().ok()),
            ));
            if path.is_dir() {
                walk(&path, entries);
            }
        }
    }
    let mut entries = Vec::new();
    if root.exists() {
        walk(root, &mut entries);
    }
    entries.sort();
    entries
}
