//! `speech generate` CLI 合同测试（GitHub issue #23）。
//!
//! 沿用 issue #21/#22 的约定：进程内 TCP mock 扮演 SenseAudio，HOME 注入隔离 Speech 状态根与
//! Apple Books fixture 数据库，死端口代理切断真实网络，secret canary 断言密钥/原文/音频 hex
//! 不泄漏到输出、缓存或 receipt。

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration as StdDuration;
use tempfile::TempDir;

/// 测试密钥 canary；不得出现在任何输出、缓存或 receipt 中。
const TEST_KEY: &str = "issue-23-secret-canary-do-not-leak";
/// 保存密钥的环境变量名（默认 Profile 使用它）。
const API_KEY_ENV: &str = "SENSEAUDIO_API_KEY";

/// 一个合法的 MPEG1 Layer III 128kbps 32kHz 立体声帧（FF FB 98 00 + 零填充到 576 字节）。
fn valid_mp3() -> Vec<u8> {
    let mut frame = vec![0xFF, 0xFB, 0x98, 0x00];
    frame.resize(576, 0);
    frame
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn synthesis_body() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "data": { "audio": to_hex(&valid_mp3()), "status": 2 },
        "extra_info": {
            "usage_characters": 5,
            "audio_length": 36,
            "audio_sample_rate": 32000,
            "audio_channel": 2,
            "audio_format": "mp3",
            "bitrate": 128000
        },
        "trace_id": "trace-issue-23",
        "base_resp": { "status_code": 0, "status_msg": "success" }
    }))
    .expect("synthesis JSON")
}

#[derive(Debug, Clone)]
struct RequestRecord {
    path: String,
    authorization: Option<String>,
    body: Vec<u8>,
}

struct MockServer {
    url: String,
    records: Arc<Mutex<Vec<RequestRecord>>>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl MockServer {
    /// 读完每个请求后直接断开、不返回响应（模拟不确定结果），持续 listen 直到 finish。
    fn disconnect_loop() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock server");
        let address = listener.local_addr().expect("mock address");
        let records = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&records);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let join = thread::spawn(move || {
            listener
                .set_nonblocking(true)
                .expect("nonblocking accept");
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let (headers, request_body) = match read_request(&mut stream) {
                            Some(value) => value,
                            None => continue,
                        };
                        let path = headers
                            .lines()
                            .next()
                            .and_then(|line| line.split_whitespace().nth(1).map(str::to_string))
                            .unwrap_or_default();
                        captured.lock().expect("records").push(RequestRecord {
                            path,
                            authorization: header_value(&headers, "authorization"),
                            body: request_body,
                        });
                        // 读完请求后关闭，不写响应。
                        drop(stream);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if stop_flag.load(Ordering::SeqCst) {
                            return;
                        }
                        thread::sleep(StdDuration::from_millis(10));
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

    /// 计数 mock：服务端只在被连接时记录，`finish` 前保持 listen。
    fn expect_no_connections() -> Self {
        Self::serve_counting()
    }

    /// 持续 listen、对每个连接返回成功合成响应的 mock（可服务多次调用）。
    fn synthesis_loop() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock server");
        let address = listener.local_addr().expect("mock address");
        let records = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&records);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let join = thread::spawn(move || {
            listener.set_nonblocking(true).expect("nonblocking accept");
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let (headers, request_body) = match read_request(&mut stream) {
    Some(value) => value,
    None => continue,
};
                        let path = headers
                            .lines()
                            .next()
                            .and_then(|line| line.split_whitespace().nth(1).map(str::to_string))
                            .unwrap_or_default();
                        captured.lock().expect("records").push(RequestRecord {
                            path,
                            authorization: header_value(&headers, "authorization"),
                            body: request_body,
                        });
                        let body = synthesis_body();
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&body);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if stop_flag.load(Ordering::SeqCst) {
                            return;
                        }
                        thread::sleep(StdDuration::from_millis(10));
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

    /// 只记录连接、从不响应（用于缺 key / 本地失败必须零连接的断言）。
    fn serve_counting() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock server");
        let address = listener.local_addr().expect("mock address");
        let records = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&records);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let join = thread::spawn(move || {
            listener.set_nonblocking(true).expect("nonblocking accept");
            loop {
                match listener.accept() {
                    Ok((_stream, _)) => {
                        captured
                            .lock()
                            .expect("records")
                            .push(RequestRecord {
                                path: String::new(),
                                authorization: None,
                                body: Vec::new(),
                            });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if stop_flag.load(Ordering::SeqCst) {
                            return;
                        }
                        thread::sleep(StdDuration::from_millis(10));
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

    /// 对首个连接返回固定 (status, body)，用于明确的 HTTP 失败断言。
    fn serve_fixed(status: u16, body: Vec<u8>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock server");
        let address = listener.local_addr().expect("mock address");
        let records = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&records);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let join = thread::spawn(move || {
            listener.set_nonblocking(true).expect("nonblocking accept");
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let (headers, request_body) = match read_request(&mut stream) {
    Some(value) => value,
    None => continue,
};
                        let path = headers
                            .lines()
                            .next()
                            .and_then(|line| line.split_whitespace().nth(1).map(str::to_string))
                            .unwrap_or_default();
                        captured.lock().expect("records").push(RequestRecord {
                            path,
                            authorization: header_value(&headers, "authorization"),
                            body: request_body,
                        });
                        let reason = if status == 200 { "OK" } else { "Error" };
                        let header = format!(
                            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&body);
                        return;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if stop_flag.load(Ordering::SeqCst) {
                            return;
                        }
                        thread::sleep(StdDuration::from_millis(10));
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
        if let Some(join) = self.join.take() {
            join.join().expect("mock thread");
        }
        self.records.lock().expect("records").clone()
    }
}

fn read_request(stream: &mut TcpStream) -> Option<(String, Vec<u8>)> {
    // 接受自 nonblocking listener 的 socket 可能继承 nonblocking；强制阻塞，
    // 否则高并发下 read 会在请求字节到达前返回 WouldBlock，被误判为连接结束。
    let _ = stream.set_nonblocking(false);
    let mut bytes = Vec::new();
    let header_end;
    loop {
        let mut chunk = [0_u8; 1024];
        match stream.read(&mut chunk) {
            Ok(0) => return None, // 客户端在发送头前关闭/重置
            Ok(count) => bytes.extend_from_slice(&chunk[..count]),
            Err(_) => return None,
        }
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
        match stream.read(&mut chunk) {
            Ok(0) => return None,
            Ok(count) => bytes.extend_from_slice(&chunk[..count]),
            Err(_) => return None,
        }
    }
    Some((headers, bytes[header_end..header_end + content_length].to_vec()))
}

fn header_value(headers: &str, name: &str) -> Option<String> {
    headers.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.eq_ignore_ascii_case(name)).then(|| value.trim().to_string())
    })
}

/// fixture：注入 HOME，内含 Apple Books annotation/library 数据库与 Speech 状态根。
struct Fixture {
    home: TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().expect("fixture home"),
        }
    }

    /// 用给定的标注行建库。每行是 (pk, asset_id, selected_text, note)。
    fn with_annotations(rows: &[(i64, &str, Option<&str>, Option<&str>)]) -> Self {
        let fixture = Self::new();
        let annotation_dir = fixture.annotation_dir();
        let library_dir = fixture.library_dir();
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
        for (index, (pk, asset, text, note)) in rows.iter().enumerate() {
            annotation_conn
                .execute(
                    "INSERT INTO ZAEANNOTATION VALUES (?1, ?2, ?3, ?4, 'epubcfi(/6/2)', ?5, 3, 0)",
                    rusqlite::params![pk, asset, text, note, index as f64],
                )
                .expect("annotation row");
        }

        let library_conn =
            rusqlite::Connection::open(library_dir.join("library.sqlite")).expect("library database");
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
        for (pk, (_, asset, _, _)) in rows.iter().enumerate() {
            library_conn
                .execute(
                    "INSERT OR IGNORE INTO ZBKLIBRARYASSET VALUES (?1, ?2, ?3, '作者', ?4)",
                    rusqlite::params![pk as i64 + 1, asset, format!("书 {asset}"), asset],
                )
                .expect("library row");
        }
        fixture
    }

    fn annotation_dir(&self) -> PathBuf {
        self.home
            .path()
            .join("Library/Containers/com.apple.iBooksX/Data/Documents/AEAnnotation")
    }

    fn library_dir(&self) -> PathBuf {
        self.home
            .path()
            .join("Library/Containers/com.apple.iBooksX/Data/Documents/BKLibrary")
    }

    fn speech_root(&self) -> PathBuf {
        self.home
            .path()
            .join("Library/Application Support/books-exporter/speech")
    }

    /// 落一份新鲜的 Voice Catalog，使默认音色无需联网即可通过 preflight 验证。
    fn seed_fresh_catalog(&self, voice_ids: &[&str]) {
        let dir = self.speech_root().join("voices");
        std::fs::create_dir_all(&dir).expect("catalog dir");
        let voices: Vec<Value> = voice_ids
            .iter()
            .map(|id| {
                json!({
                    "source_type": "system",
                    "voice_id": id,
                    "voice_name": format!("Voice {id}"),
                    "emotion_label": null,
                    "style_label": null,
                    "description": ["平稳"],
                    "created_time": null
                })
            })
            .collect();
        let catalog = json!({
            "schema_version": 1,
            "provider": "senseaudio",
            "fetched_at": chrono::Utc::now().to_rfc3339(),
            "voices": voices
        });
        std::fs::write(
            dir.join("senseaudio.json"),
            serde_json::to_vec_pretty(&catalog).expect("catalog JSON"),
        )
        .expect("seed catalog");
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_apple-books-exporter"));
        command.env("HOME", self.home.path());
        command.env_remove(API_KEY_ENV);
        command.env_remove("SENSEAUDIO_API_BASE_URL");
        command.env_remove("HTTP_PROXY");
        command.env_remove("http_proxy");
        command.env_remove("HTTPS_PROXY");
        command.env_remove("https_proxy");
        command.env_remove("ALL_PROXY");
        command.env_remove("all_proxy");
        command.env_remove("NO_PROXY");
        command.env_remove("no_proxy");
        command
    }

    /// 带 mock server 与密钥运行 generate。
    fn generate_with_server(&self, args: &[&str], server: &MockServer) -> Output {
        self.command()
            .args(args)
            .env(API_KEY_ENV, TEST_KEY)
            .env("SENSEAUDIO_API_BASE_URL", &server.url)
            .output()
            .expect("run CLI")
    }

    /// 递归读取 speech 根目录下所有文件字节。
    fn speech_files(&self) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(dir: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
            let entries = match std::fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(_) => return,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, files);
                } else if let Ok(bytes) = std::fs::read(&path) {
                    files.push((path, bytes));
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

const HIGHLIGHT: &str = "高亮正文";
const NOTE: &str = "我的笔记";

fn standard_fixture() -> Fixture {
    let fixture = Fixture::with_annotations(&[
        (41, "book-1", Some(HIGHLIGHT), Some(NOTE)),
        (42, "book-1", Some("只有高亮"), None),
        (43, "book-2", Some("另一本书的高亮"), Some("另一本书的笔记")),
    ]);
    fixture.seed_fresh_catalog(&["male_0004_a"]);
    fixture
}

// ---- Criterion 5 + 6 + 8: one sync request, resolved voice, fixed MP3, immutable version, receipt ----

#[test]
fn json_generate_sends_one_request_and_stores_an_immutable_version() {
    let fixture = standard_fixture();
    let server = MockServer::synthesis_loop();

    let value = succeeded(&fixture.generate_with_server(
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
        &server,
    ));
    let records = server.finish();

    // Criterion 5: exactly one synchronous request with resolved voice + fixed MP3 settings.
    assert_eq!(records.len(), 1, "exactly one provider request");
    assert_eq!(records[0].path, "/v1/t2a_v2");
    let expected_auth = format!("Bearer {TEST_KEY}");
    assert_eq!(records[0].authorization.as_deref(), Some(expected_auth.as_str()));
    let request: Value = serde_json::from_slice(&records[0].body).expect("request JSON");
    assert_eq!(request["stream"], false);
    assert_eq!(request["voice_setting"]["voice_id"], "male_0004_a");
    assert_eq!(request["voice_setting"]["speed"], 1.0);
    assert_eq!(request["voice_setting"]["vol"], 1.0);
    assert_eq!(request["voice_setting"]["pitch"], 0);
    assert_eq!(request["audio_setting"]["format"], "mp3");
    assert_eq!(request["audio_setting"]["sample_rate"], 32000);
    assert_eq!(request["audio_setting"]["bitrate"], 128000);
    assert_eq!(request["audio_setting"]["channel"], 2);

    // Criterion 8 + 6: receipt carries identity, profile, counts, artifact metadata, usage, trace.
    assert_eq!(value["schema_version"], 1);
    let receipt = &value["receipt"];
    assert_eq!(receipt["operation"], "generate");
    assert_eq!(receipt["source"], "provider");
    assert_eq!(receipt["provider_called"], true);
    assert!(receipt["attempt_id"].is_string());
    assert_eq!(receipt["asset_id"], "book-1");
    assert_eq!(receipt["annotation_id"], "annotation-41");
    assert_eq!(receipt["content_kind"], "highlight");
    assert_eq!(receipt["profile"]["voice_id"], "male_0004_a");
    assert!(receipt["text_sha256"].is_string());
    assert_eq!(receipt["unicode_characters"], HIGHLIGHT.chars().count() as u64);
    assert!(receipt["estimated_billing_characters"].is_number());
    assert_eq!(receipt["audio"]["format"], "mp3");
    assert_eq!(receipt["audio"]["sample_rate"], 32000);
    assert_eq!(receipt["audio"]["bitrate"], 128000);
    assert_eq!(receipt["audio"]["channel"], 2);
    assert!(receipt["audio"]["size_bytes"].as_u64().unwrap() > 0);
    assert_eq!(receipt["provider"]["trace_id"], "trace-issue-23");
    assert_eq!(receipt["provider"]["usage_characters"], 5);

    // Criterion 8: receipt never contains source text, key, or audio hex.
    let receipt_text = serde_json::to_string(&value).expect("receipt text");
    assert!(!receipt_text.contains(HIGHLIGHT));
    assert!(!receipt_text.contains(TEST_KEY));
    assert!(!receipt_text.contains("fffb98"));

    // Criterion 6: an immutable audio version + atomic current pointer were committed.
    let clip_id = receipt["clip_id"].as_str().expect("clip id").to_string();
    let audio_sha = receipt["audio"]["sha256"].as_str().expect("audio sha").to_string();
    let version_dir = fixture
        .speech_root()
        .join("clips")
        .join(&clip_id)
        .join("versions")
        .join(&audio_sha);
    assert!(version_dir.join("audio.mp3").is_file());
    assert!(version_dir.join("metadata.json").is_file());
    let state: Value = serde_json::from_slice(
        &std::fs::read(fixture.speech_root().join("clips").join(&clip_id).join("state.json")).expect("state"),
    )
    .expect("state JSON");
    assert_eq!(state["current_cache_status"], "ready");
    assert_eq!(state["current_audio_sha256"], audio_sha.as_str());

    // Secret canary: no key, source text, or audio hex in any cache file.
    for (path, bytes) in fixture.speech_files() {
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains(TEST_KEY), "key leaked into {:?}", path);
        assert!(!text.contains(HIGHLIGHT), "source text leaked into {:?}", path);
    }
}

// ---- Criterion 1: stable-id content selection for highlight and note ----

#[test]
fn json_selects_highlight_and_note_as_independent_clips() {
    let fixture = standard_fixture();
    let server = MockServer::synthesis_loop();

    let highlight = succeeded(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &server,
    ));
    let note = succeeded(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "note", "--json",
        ],
        &server,
    ));
    server.finish();

    assert_eq!(highlight["receipt"]["content_kind"], "highlight");
    assert_eq!(note["receipt"]["content_kind"], "note");
    assert_eq!(highlight["receipt"]["unicode_characters"], HIGHLIGHT.chars().count() as u64);
    assert_eq!(note["receipt"]["unicode_characters"], NOTE.chars().count() as u64);
    assert_ne!(
        highlight["receipt"]["clip_id"], note["receipt"]["clip_id"],
        "highlight and note are two independent clips"
    );
    assert_ne!(
        highlight["receipt"]["text_sha256"], note["receipt"]["text_sha256"]
    );
}

#[test]
fn human_mode_resolves_display_indices_to_the_same_stable_clip() {
    let fixture = standard_fixture();
    // 先用机器模式拿到稳定 clip_id。
    let machine_server = MockServer::synthesis_loop();
    let machine = succeeded(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &machine_server,
    ));
    machine_server.finish();
    let clip_id = machine["receipt"]["clip_id"].as_str().expect("clip id").to_string();

    // 人类模式用显示序号；应解析到同一 clip（缓存命中，0 连接）。
    let server = MockServer::expect_no_connections();
    let output = fixture.generate_with_server(
        &["speech", "generate", "1", "--annotation", "1", "--content", "highlight"],
        &server,
    );
    let records = server.finish();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).expect("human output");
    assert!(text.contains(&clip_id), "human mode must resolve to the same clip id");
    assert!(text.contains("highlight"));
    assert!(!text.contains(HIGHLIGHT), "human output must not echo the full speech text");
    assert_eq!(records.len(), 0, "display-index resolution hit the cache with no provider call");
}

// ---- Criterion 2: local failures happen before any provider call ----

#[test]
fn missing_content_fails_before_any_provider_call() {
    let fixture = standard_fixture();
    let server = MockServer::expect_no_connections();
    // annotation-42 只有高亮，请求 note。
    let value = failed(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-42",
            "--content", "note", "--json",
        ],
        &server,
    ));
    let records = server.finish();
    assert_eq!(value["error"]["code"], "SPEECH_CONTENT_UNAVAILABLE");
    assert_eq!(records.len(), 0, "no provider call for missing content");
}

#[test]
fn wrong_annotation_ownership_fails_before_any_provider_call() {
    let fixture = standard_fixture();
    let server = MockServer::expect_no_connections();
    // annotation-43 属于 book-2，却用 book-1 请求。
    let value = failed(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-43",
            "--content", "highlight", "--json",
        ],
        &server,
    ));
    let records = server.finish();
    assert_eq!(value["error"]["code"], "INVALID_ANNOTATION_ID");
    assert_eq!(records.len(), 0);
}

#[test]
fn missing_asset_fails_before_any_provider_call() {
    let fixture = standard_fixture();
    let server = MockServer::expect_no_connections();
    let value = failed(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "no-such-book", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &server,
    ));
    let records = server.finish();
    assert_eq!(value["error"]["code"], "INVALID_ASSET_ID");
    assert_eq!(records.len(), 0);
}

#[test]
fn overlong_text_fails_before_any_provider_call() {
    let long = "汉".repeat(10_001);
    let fixture = Fixture::with_annotations(&[(41, "book-1", Some(long.as_str()), None)]);
    fixture.seed_fresh_catalog(&["male_0004_a"]);
    let server = MockServer::expect_no_connections();
    let value = failed(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &server,
    ));
    let records = server.finish();
    assert_eq!(value["error"]["code"], "SPEECH_TEXT_TOO_LONG");
    assert_eq!(value["error"]["details"]["characters"], 10_001);
    assert_eq!(records.len(), 0);
}

#[test]
fn invalid_profile_override_fails_before_any_provider_call() {
    let fixture = standard_fixture();
    let server = MockServer::expect_no_connections();
    let value = failed(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--speed", "3.0", "--json",
        ],
        &server,
    ));
    let records = server.finish();
    assert_eq!(value["error"]["code"], "SPEECH_PROFILE_INVALID");
    assert_eq!(value["error"]["details"]["field"], "speed");
    assert_eq!(records.len(), 0);
}

#[test]
fn unavailable_voice_override_fails_before_any_provider_call() {
    let fixture = standard_fixture();
    let server = MockServer::expect_no_connections();
    let value = failed(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--voice-id", "female_9999_z", "--json",
        ],
        &server,
    ));
    let records = server.finish();
    assert_eq!(value["error"]["code"], "SPEECH_VOICE_UNAVAILABLE");
    assert_eq!(records.len(), 0);
}

#[test]
fn missing_api_key_fails_before_any_provider_call() {
    let fixture = standard_fixture();
    let server = MockServer::expect_no_connections();
    // command() 已移除 SENSEAUDIO_API_KEY；此处不设置密钥。
    let output = fixture
        .command()
        .env("SENSEAUDIO_API_BASE_URL", &server.url)
        .args([
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ])
        .output()
        .expect("run CLI");
    let value = failed(&output);
    let records = server.finish();
    assert_eq!(value["error"]["code"], "SPEECH_AUTH_FAILED");
    assert_eq!(records.len(), 0, "missing key must not open any provider connection");
}

#[test]
fn conflicting_json_arguments_are_rejected() {
    let fixture = standard_fixture();
    // --json 不接受 positional book index。
    let output = fixture
        .command()
        .args([
            "speech", "generate", "1", "--annotation", "1", "--content", "highlight", "--json",
        ])
        .output()
        .expect("run CLI");
    let value = failed(&output);
    assert_eq!(value["error"]["code"], "INVALID_ARGUMENT");

    // --json 缺少稳定 ID。
    let output = fixture
        .command()
        .args(["speech", "generate", "--asset-id", "book-1", "--content", "highlight", "--json"])
        .output()
        .expect("run CLI");
    assert_eq!(failed(&output)["error"]["code"], "INVALID_ARGUMENT");
}

#[test]
fn human_mode_rejects_stable_id_arguments() {
    let fixture = standard_fixture();
    let output = fixture
        .command()
        .args([
            "speech", "generate", "1", "--annotation", "1", "--content", "highlight",
            "--asset-id", "book-1",
        ])
        .output()
        .expect("run CLI");
    // 人类模式错误走 anyhow → 纯文本 stderr，不是 Machine JSON。
    assert!(!output.status.success(), "human mode must reject --asset-id");
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("asset-id") || stderr.contains("--asset-id"),
        "stderr should explain the conflict: {stderr}"
    );
}

#[test]
fn invalid_content_kind_is_rejected() {
    let fixture = standard_fixture();
    let output = fixture
        .command()
        .args([
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "both", "--json",
        ])
        .output()
        .expect("run CLI");
    assert_eq!(failed(&output)["error"]["code"], "INVALID_ARGUMENT");
}

// ---- Criterion 3: normalization + provider-safe control markup reach the request ----

#[test]
fn speech_text_is_normalized_and_control_markup_is_neutralized_in_the_request() {
    let raw = "  第一行\r\n第二行 <break time=\"500\"/> 结尾  ";
    let fixture = Fixture::with_annotations(&[(41, "book-1", Some(raw), None)]);
    fixture.seed_fresh_catalog(&["male_0004_a"]);
    let server = MockServer::synthesis_loop();

    succeeded(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &server,
    ));
    let records = server.finish();

    let request: Value = serde_json::from_slice(&records[0].body).expect("request JSON");
    let text = request["text"].as_str().expect("request text");
    // CRLF → LF，边界空白删除，内部换行保留。
    assert!(!text.contains('\r'));
    assert!(text.starts_with("第一行\n第二行"));
    assert!(text.ends_with("结尾"));
    // <break> 控制标记被 U+200B guard 中和，不构成控制语法。
    assert!(text.contains("<\u{200B}break"));
    assert!(!text.contains("<break time"));
}

// ---- Criterion 4: fingerprint changes for every provider-effective input ----

#[test]
fn changing_a_provider_effective_input_produces_a_new_clip_and_call() {
    let fixture = standard_fixture();
    let server = MockServer::synthesis_loop();

    let base = succeeded(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &server,
    ));
    let faster = succeeded(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--speed", "1.5", "--json",
        ],
        &server,
    ));
    let records = server.finish();

    assert_ne!(base["receipt"]["clip_id"], faster["receipt"]["clip_id"]);
    assert_eq!(records.len(), 2, "a changed speed is a new provider-effective input");
    assert_eq!(faster["receipt"]["profile"]["speed"], 1.5);
}

// ---- Criterion 7: a repeated identical request reuses the cache with no second call ----

#[test]
fn a_repeated_request_reuses_the_cache_without_another_provider_call() {
    let fixture = standard_fixture();
    let server = MockServer::synthesis_loop();

    let first = succeeded(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &server,
    ));
    let second = succeeded(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &server,
    ));
    let records = server.finish();

    assert_eq!(records.len(), 1, "cache hit must not call the provider again");
    assert_eq!(first["receipt"]["source"], "provider");
    assert_eq!(second["receipt"]["source"], "cache");
    assert_eq!(second["receipt"]["provider_called"], false);
    assert!(second["receipt"]["attempt_id"].is_null());
    assert_eq!(first["receipt"]["clip_id"], second["receipt"]["clip_id"]);
}

// ---- Criterion 9: stable errors, uncertain outcomes, and no auto-retry ----

#[test]
fn an_uncertain_outcome_blocks_a_plain_retry_without_a_second_provider_call() {
    let fixture = standard_fixture();
    let server = MockServer::disconnect_loop();

    let first = failed(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &server,
    ));
    assert_eq!(first["error"]["code"], "SPEECH_RESULT_UNKNOWN");
    assert_eq!(first["error"]["details"]["outcome"], "unknown");

    // 普通 generate 不得越过 unknown gate：仍然失败，且不发起第二个请求。
    let second = failed(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &server,
    ));
    let records = server.finish();
    assert_eq!(second["error"]["code"], "SPEECH_RESULT_UNKNOWN");
    assert_eq!(records.len(), 1, "an unknown outcome must never auto-retry");
}

#[test]
fn explicit_provider_failure_is_a_stable_error_and_does_not_block_a_later_retry() {
    let fixture = standard_fixture();
    let server = MockServer::serve_fixed(500, br#"{"code":"server_error","message":"boom"}"#.to_vec());

    let value = failed(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &server,
    ));
    let records = server.finish();
    assert_eq!(value["error"]["code"], "SPEECH_PROVIDER_FAILED");
    assert_eq!(value["error"]["details"]["outcome"], "failed");
    assert_eq!(records.len(), 1, "an explicit failure calls the provider exactly once");
    assert!(!String::from_utf8_lossy(&serde_json::to_vec(&value).expect("json")).contains(TEST_KEY));

    // 明确失败不设置阻塞 gate：换一个会成功的 mock 再生成即可成功。
    let server = MockServer::synthesis_loop();
    let retry = succeeded(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &server,
    ));
    server.finish();
    assert_eq!(retry["receipt"]["source"], "provider");
}

#[test]
fn rate_limited_and_auth_failures_map_to_stable_codes() {
    let fixture = standard_fixture();

    let limited = MockServer::serve_fixed(429, br#"{"message":"slow down"}"#.to_vec());
    let value = failed(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &limited,
    ));
    limited.finish();
    assert_eq!(value["error"]["code"], "SPEECH_RATE_LIMITED");

    let unauthorized = MockServer::serve_fixed(401, br#"{"message":"bad key"}"#.to_vec());
    let value = failed(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &unauthorized,
    ));
    unauthorized.finish();
    assert_eq!(value["error"]["code"], "SPEECH_AUTH_FAILED");
}

#[test]
fn invalid_audio_payload_is_artifact_missing_and_blocks_retry() {
    let fixture = standard_fixture();
    // 成功 envelope 但 audio 是坏 hex → 产物缺失。
    let body = serde_json::to_vec(&json!({
        "data": { "audio": "not-hex-zz", "status": 2 },
        "trace_id": "trace-bad",
        "base_resp": { "status_code": 0 }
    }))
    .expect("body");
    let server = MockServer::serve_fixed(200, body);

    let value = failed(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &server,
    ));
    server.finish();
    assert_eq!(value["error"]["code"], "SPEECH_AUDIO_INVALID");
    assert_eq!(value["error"]["details"]["outcome"], "provider_succeeded_artifact_missing");
}

// ---- regenerate replaces the cache with a new attempt ----

#[test]
fn regenerate_creates_a_new_attempt_and_switches_the_pointer() {
    let fixture = standard_fixture();
    let server = MockServer::synthesis_loop();

    let first = succeeded(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &server,
    ));
    let regenerated = succeeded(&fixture.generate_with_server(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--regenerate", "--json",
        ],
        &server,
    ));
    let records = server.finish();

    assert_eq!(records.len(), 2, "regenerate is an explicit second billed request");
    assert_eq!(regenerated["receipt"]["source"], "provider");
    assert_eq!(first["receipt"]["clip_id"], regenerated["receipt"]["clip_id"]);
    assert_ne!(
        first["receipt"]["attempt_id"], regenerated["receipt"]["attempt_id"],
        "regenerate creates a new attempt id for the same clip id"
    );
}
