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
        // 默认 fixture：annotation-41 同时有高亮与笔记，annotation-42 只有高亮。
        Self::with_annotations(&[
            (
                41,
                "book-1",
                Some("高亮正文"),
                Some("我的笔记"),
            ),
            (42, "book-1", Some("只有高亮"), None),
        ])
    }

    /// 用给定的标注行建库：每行是 `(pk, asset_id, selected_text, note)`。
    ///
    /// CLI 测试因此可以注入任意正文，包括字面 `<break>` 形状的控制标记和超过
    /// 10000 字符的超长文本，从而端到端覆盖 provider-safe 处理与
    /// `SPEECH_TEXT_TOO_LONG` 路径。
    fn with_annotations(rows: &[(i64, &str, Option<&str>, Option<&str>)]) -> Self {
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
        for (index, (pk, asset, text, note)) in rows.iter().enumerate() {
            annotation_conn
                .execute(
                    "INSERT INTO ZAEANNOTATION VALUES (?1, ?2, ?3, ?4, 'epubcfi(/6/2)', ?5, 3, 0)",
                    rusqlite::params![pk, asset, text, note, index as f64],
                )
                .expect("annotation row");
        }

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
        for (index, (_, asset, _, _)) in rows.iter().enumerate() {
            library_conn
                .execute(
                    "INSERT OR IGNORE INTO ZBKLIBRARYASSET VALUES (?1, ?2, '测试书', '测试作者', ?3)",
                    rusqlite::params![index as i64 + 1, asset, asset],
                )
                .expect("library row");
        }

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
        self.run_with_env(args, provider, key, &[])
    }

    /// 与 [`Fixture::run_with`] 相同，但可注入额外环境变量。
    ///
    /// 锁等待上限就是这样注入的：子进程因此能用可预期的短超时进入
    /// `SPEECH_IN_PROGRESS` 分支，而不必真的等待 20 秒。
    fn run_with_env(
        &self,
        args: &[&str],
        provider: &MockProvider,
        key: Option<&str>,
        extra: &[(&str, &str)],
    ) -> Output {
        let mut command = self.command();
        command
            .args(args)
            .env("SENSEAUDIO_API_BASE_URL", &provider.url);
        if let Some(key) = key {
            command.env("SENSEAUDIO_API_KEY", key);
        }
        for (name, value) in extra {
            command.env(name, value);
        }
        command.output().expect("run CLI")
    }

    fn speech_root(&self) -> PathBuf {
        self.home
            .path()
            .join("Library/Application Support/books-exporter/speech")
    }

    /// 某个 clip 的跨进程 writer 锁文件路径。
    fn lock_path(&self, clip_id: &str) -> PathBuf {
        self.speech_root()
            .join("locks")
            .join(format!("{clip_id}.lock"))
    }

    /// 手动占用 writer 锁：模拟另一个进程正在生成同一 clip（O_EXCL 语义相同）。
    fn occupy_lock(&self, clip_id: &str) {
        let path = self.lock_path(clip_id);
        std::fs::create_dir_all(path.parent().expect("locks dir")).expect("locks dir");
        std::fs::write(&path, "2026-09-29T00:00:00Z\n").expect("occupy lock");
    }

    fn release_lock(&self, clip_id: &str) {
        let _ = std::fs::remove_file(self.lock_path(clip_id));
    }

    /// 该 clip 当前被指向的音频 sha256。
    fn current_audio_sha256(&self, clip_id: &str) -> String {
        let state: Value = serde_json::from_str(
            &std::fs::read_to_string(self.speech_root().join("clips").join(clip_id).join("state.json"))
                .expect("state.json"),
        )
        .expect("state JSON");
        state["current_audio_sha256"]
            .as_str()
            .expect("current audio sha256")
            .to_string()
    }

    fn clip_state(&self, clip_id: &str) -> Value {
        serde_json::from_str(
            &std::fs::read_to_string(self.speech_root().join("clips").join(clip_id).join("state.json"))
                .expect("state.json"),
        )
        .expect("state JSON")
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

/// 目录可用、合成返回可解析 MP3，但 `extra_info` 声明的音频规格与帧解析矛盾。
fn provider_with_contradicting_declared_audio() -> MockProvider {
    let catalog = catalog_response();
    let synthesis = || {
        serde_json::to_vec(&json!({
            "data": {"audio": hex_encode(&silent_mp3(2)), "status": 2},
            "extra_info": {
                "audio_length": 72,
                // 本地 MP3 帧是 32000Hz/128000bps/2 声道：声明必须与解析一致。
                "audio_sample_rate": 44100,
                "audio_size": 2 * 576,
                "bitrate": 128000,
                "audio_format": "mp3",
                "audio_channel": 2,
                "usage_characters": 8
            },
            "trace_id": "trace-declared-mismatch",
            "base_resp": {"status_code": 0, "status_msg": "success"}
        }))
        .expect("response JSON")
    };
    MockProvider::serve(Response {
        status: Response::status(200),
        body: Arc::new(move |path: &str| {
            if path.ends_with("/v1/t2a_v2") {
                synthesis()
            } else {
                catalog.clone()
            }
        }),
        delay: None,
        drop: Response::never_drop(),
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

    // 显式失败是确定终态：普通 generate 可以再次尝试，不需要 --regenerate，
    // 也绝不返回 SPEECH_RESULT_UNKNOWN（那会把不确定结果伪装成已知失败）。
    let second_provider = provider_with_catalog_and_synthesis("trace");
    let retried = succeeded(&fixture.run_with(
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
    assert_eq!(retried["receipt"]["source"], "provider");
    assert_eq!(
        MockProvider::synthesis_count(&second_records),
        1,
        "an explicit failure must not gate the clip: the retry reaches the provider"
    );
    let state_path = fixture
        .speech_root()
        .join("clips")
        .join(retried["receipt"]["clip_id"].as_str().expect("clip id"))
        .join("state.json");
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state_path).expect("state.json")).expect("state");
    assert_eq!(
        state["generation_blocked"], false,
        "an explicit provider failure must not write an unknown gate"
    );

    // 缓存已经有效：重复的普通 generate 返回 cache，0 次 provider 调用。
    let cache_provider = provider_with_catalog_and_synthesis("trace-cache");
    let cached = succeeded(&fixture.run_with(
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
        &cache_provider,
        Some(TEST_KEY),
    ));
    let cache_records = cache_provider.finish();
    assert_eq!(cached["receipt"]["source"], "cache");
    assert_eq!(cache_records.len(), 0, "a valid cache makes no connection");

    // 不确定结果仍然只有 --regenerate 才能越过；失败的 regenerate 保留旧的有效缓存。
    let dropped_provider = provider_with_dropped_synthesis();
    let uncertain = failed(&fixture.run_with(
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
        &dropped_provider,
        Some(TEST_KEY),
    ));
    let dropped_records = dropped_provider.finish();
    assert_eq!(uncertain["error"]["code"], "SPEECH_RESULT_UNKNOWN");
    assert_eq!(uncertain["error"]["details"]["outcome"], "unknown");
    assert_eq!(MockProvider::synthesis_count(&dropped_records), 1);

    // 失败的 --regenerate 不破坏已有缓存：普通 generate 仍然零连接命中。
    let after_regenerate = succeeded(&fixture.run_with(
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
        &provider_with_catalog_and_synthesis("trace-after"),
        Some(TEST_KEY),
    ));
    let after_records_guard = after_regenerate["receipt"]["clip_id"].as_str().map(str::to_string);
    assert_eq!(after_regenerate["receipt"]["source"], "cache");
    assert!(after_records_guard.is_some());
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
fn literal_control_markup_in_the_annotation_is_neutralized() {
    // 参数化 fixture 直接把字面 `<break>` 形状的控制标记写进高亮正文。
    let fixture = Fixture::with_annotations(&[(
        7,
        "book-markup",
        Some("停顿<break time=\"500\"/>结束 < 比较 </close>"),
        None,
    )]);
    let provider = provider_with_catalog_and_synthesis("trace-markup");

    let value = succeeded(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-markup",
            "--annotation-id",
            "annotation-7",
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
    let text = body["text"].as_str().expect("request text");

    // 每个 ASCII `<` 后面都必须紧跟 U+200B 守卫：控制标签无法闭合生效。
    let characters: Vec<char> = text.chars().collect();
    for (index, character) in characters.iter().enumerate() {
        if *character == '<' {
            assert_eq!(
                characters.get(index + 1),
                Some(&'\u{200b}'),
                "guard must follow every < : {text}"
            );
        }
    }
    assert_eq!(
        characters.iter().filter(|c| **c == '<').count(),
        3,
        "every ASCII < must be guarded: {text}"
    );
    // 去掉守卫后必须与原文快照完全一致：一个字符都没有被删除或改写。
    let stripped: String = text.chars().filter(|c| *c != '\u{200b}').collect();
    assert_eq!(stripped, "停顿<break time=\"500\"/>结束 < 比较 </close>");
    assert_eq!(
        value["receipt"]["unicode_characters"],
        "停顿<break time=\"500\"/>结束 < 比较 </close>"
            .chars()
            .count(),
        "the local character count is computed from the original snapshot"
    );
}

#[test]
fn overlong_annotation_text_fails_before_any_provider_call() {
    let overlong = "字".repeat(10_001);
    let fixture = Fixture::with_annotations(&[(9, "book-long", Some(overlong.as_str()), None)]);
    let provider = provider_with_catalog_and_synthesis("trace-long");

    let value = failed(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-long",
            "--annotation-id",
            "annotation-9",
            "--content",
            "highlight",
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();

    assert_eq!(value["error"]["code"], "SPEECH_TEXT_TOO_LONG");
    assert_eq!(value["error"]["details"]["reason"], "text_too_long");
    assert_eq!(records.len(), 0, "overlong text must not reach the provider");
    assert!(
        !String::from_utf8_lossy(&serde_json::to_vec(&value).expect("error JSON"))
            .contains("字".repeat(100).as_str()),
        "the error envelope must not carry the source text"
    );
}

#[test]
fn declared_audio_metadata_that_contradicts_the_parsed_mp3_is_rejected() {
    let fixture = Fixture::new();
    let provider = provider_with_contradicting_declared_audio();

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
    assert_eq!(MockProvider::synthesis_count(&records), 1);
    assert!(
        fixture.clip_dirs().iter().all(|dir| dir
            .join("versions")
            .read_dir()
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(true)),
        "a rejected artifact must not create an immutable audio version"
    );

    // provider 成功但没有可用产物：普通 generate 被 unknown gate 挡住，不自动重放。
    let blocked_provider = provider_with_catalog_and_synthesis("trace-declared-blocked");
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
        &blocked_provider,
        Some(TEST_KEY),
    ));
    let blocked_records = blocked_provider.finish();
    assert_eq!(blocked["error"]["code"], "SPEECH_RESULT_UNKNOWN");
    assert_eq!(blocked_records.len(), 0);
}

#[test]
fn an_unusable_speech_root_fails_the_preflight_before_any_provider_call() {
    let fixture = Fixture::new();
    // 用一个普通文件占住 Speech 状态根：存储预检必须在任何连接之前失败。
    std::fs::create_dir_all(fixture.speech_root()).expect("speech root");
    std::fs::write(fixture.speech_root().join("clips"), b"not a directory")
        .expect("block clip storage");

    let provider = provider_with_catalog_and_synthesis("trace-storage");
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

    assert_eq!(value["error"]["code"], "SPEECH_STORAGE_UNAVAILABLE");
    assert_eq!(
        records.len(),
        0,
        "a failed storage preflight must not open a connection"
    );

    // human 模式给出一句可读错误，同样零连接。
    let human_provider = provider_with_catalog_and_synthesis("trace-storage-human");
    let human = fixture.run_with(
        &[
            "speech",
            "generate",
            "1",
            "--annotation",
            "1",
            "--content",
            "highlight",
        ],
        &human_provider,
        Some(TEST_KEY),
    );
    assert!(!human.status.success());
    let stderr = String::from_utf8_lossy(&human.stderr);
    assert!(stderr.contains("Speech"), "{stderr}");
    assert_eq!(human_provider.finish().len(), 0);
}

#[test]
fn human_display_indices_out_of_range_fail_without_targeting_the_first_book() {
    let fixture = Fixture::new();
    let provider = provider_with_catalog_and_synthesis("trace-index");

    // 0 与越界都必须失败；绝不能降级成第一本书/第一条 Annotation。
    for args in [
        vec!["speech", "generate", "0", "--annotation", "1", "--content", "highlight"],
        vec!["speech", "generate", "99", "--annotation", "1", "--content", "highlight"],
        vec!["speech", "generate", "1", "--annotation", "0", "--content", "highlight"],
        vec!["speech", "generate", "1", "--annotation", "9", "--content", "highlight"],
    ] {
        let output = fixture.run_with(&args, &provider, Some(TEST_KEY));
        assert!(!output.status.success(), "{args:?} must fail");
        assert!(
            output.stdout.is_empty(),
            "{args:?} must not print a receipt on stdout"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        let error: Value = serde_json::from_slice(&output.stderr).expect("error envelope");
        assert_eq!(error["error"]["code"], "INVALID_ARGUMENT", "{args:?}");
        assert!(
            stderr.contains("序号"),
            "{args:?} must name the invalid index: {stderr}"
        );
    }
    let records = provider.finish();
    assert_eq!(
        records.len(),
        0,
        "an out-of-range index must not reach the provider at all"
    );

    // 合法序号照常工作（人类模式输出不是 JSON envelope）。
    let ok_provider = provider_with_catalog_and_synthesis("trace-index-ok");
    let ok = fixture.run_with(
        &[
            "speech",
            "generate",
            "1",
            "--annotation",
            "2",
            "--content",
            "highlight",
        ],
        &ok_provider,
        Some(TEST_KEY),
    );
    assert!(ok.status.success(), "stderr: {:?}", String::from_utf8_lossy(&ok.stderr));
    let stdout = String::from_utf8_lossy(&ok.stdout);
    assert!(stdout.contains("Voice ID: male_0004_a"), "{stdout}");
    assert_eq!(MockProvider::synthesis_count(&ok_provider.finish()), 1);
}

#[test]
fn an_in_place_corrupted_version_is_repaired_and_never_served_as_a_cache_hit() {
    let fixture = Fixture::new();

    // 第一次生成落一个不可变 version。
    let first = provider_with_catalog_and_synthesis("trace-repair-1");
    let value = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &first,
        Some(TEST_KEY),
    ));
    let clip_id = value["receipt"]["clip_id"].as_str().expect("clip id").to_string();
    assert_eq!(MockProvider::synthesis_count(&first.finish()), 1);

    let audio_path = fixture
        .speech_root()
        .join("clips")
        .join(&clip_id)
        .join("versions")
        .join(value["receipt"]["audio"]["sha256"].as_str().expect("sha256"))
        .join("audio.mp3");
    let bytes = std::fs::read(&audio_path).expect("audio version");
    let mut tampered = bytes.clone();
    tampered[10] ^= 0xFF;
    std::fs::write(&audio_path, &tampered).expect("tamper");

    // 普通 generate：损坏的 version 被识别，重新生成并修复目录。
    let second = provider_with_catalog_and_synthesis("trace-repair-2");
    let repaired = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &second,
        Some(TEST_KEY),
    ));
    assert_eq!(repaired["receipt"]["source"], "provider");
    assert_eq!(
        MockProvider::synthesis_count(&second.finish()),
        1,
        "a corrupt version must be repaired by exactly one provider call"
    );
    assert_eq!(
        std::fs::read(&audio_path).expect("repaired audio"),
        bytes,
        "the repaired version must hold the original bytes again"
    );

    // 修复后重复的普通 generate 命中缓存：0 次 provider 调用，且不再返回坏音频。
    let third = provider_with_catalog_and_synthesis("trace-repair-3");
    let cached = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &third,
        Some(TEST_KEY),
    ));
    let third_records = third.finish();
    assert_eq!(cached["receipt"]["source"], "cache");
    assert_eq!(third_records.len(), 0, "a repaired cache must make no connection");
    assert_eq!(std::fs::read(&audio_path).expect("served audio"), bytes);
}

#[test]
fn a_commit_failure_returns_artifact_commit_failed_and_gates_the_clip() {
    let fixture = Fixture::new();

    // 用一个普通文件占住 Speech 状态根的 tmp 目录：provider 成功之后的原子放置必然失败，
    // 但调用前的 state 写入仍然可行，于是失败点正好落在「provider 已计费」之后。
    std::fs::create_dir_all(fixture.speech_root()).expect("speech root");
    std::fs::write(fixture.speech_root().join("tmp"), b"not a directory").expect("block tmp");

    let failing = provider_with_catalog_and_synthesis("trace-commit-1");
    let failing_output = fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &failing,
        Some(TEST_KEY),
    );
    let failed_records = failing.finish();
    assert!(!failing_output.status.success());
    let failed_value = serde_json::from_slice::<Value>(&failing_output.stderr).expect("error JSON");
    assert_eq!(failed_value["error"]["code"], "SPEECH_ARTIFACT_COMMIT_FAILED");
    assert_eq!(
        failed_value["error"]["details"]["outcome"],
        "provider_succeeded_artifact_missing",
        "a commit failure must be recorded as provider-succeeded-artifact-missing"
    );
    assert!(failed_value["error"]["details"]["attempt_id"].is_string());
    assert_eq!(MockProvider::synthesis_count(&failed_records), 1);
    // 没有任何音频 version 被留下。
    assert!(fixture
        .clip_dirs()
        .iter()
        .all(|dir| !dir.join("versions").exists()));

    // attempt history 与 clip state 必须留下这次失败，并设置 unknown gate。
    let clip_dir = fixture.clip_dirs().into_iter().next().expect("clip dir");
    let state: Value = serde_json::from_slice(
        &std::fs::read(clip_dir.join("state.json")).expect("state.json"),
    )
    .expect("state JSON");
    assert_eq!(
        state["generation_blocked"], true,
        "provider-succeeded-artifact-missing must gate a plain generate"
    );
    assert_eq!(state["latest_error_code"], "SPEECH_ARTIFACT_COMMIT_FAILED");
    assert_eq!(
        state["latest_attempt_status"], "provider_succeeded_artifact_missing"
    );

    // 普通 generate 被 gate 挡住：零连接，不自动重放。
    let blocked = provider_with_catalog_and_synthesis("trace-commit-2");
    let blocked_value = failed(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &blocked,
        Some(TEST_KEY),
    ));
    assert_eq!(blocked_value["error"]["code"], "SPEECH_RESULT_UNKNOWN");
    assert_eq!(blocked_value["error"]["details"]["outcome"], "unknown");
    assert_eq!(blocked.finish().len(), 0);

    // 只有显式 --regenerate 才能越过 gate。
    std::fs::remove_file(fixture.speech_root().join("tmp")).expect("unblock tmp");
    let regenerated = provider_with_catalog_and_synthesis("trace-commit-3");
    let recovered = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--regenerate", "--json",
        ],
        &regenerated,
        Some(TEST_KEY),
    ));
    assert_eq!(recovered["receipt"]["source"], "provider");
    assert_eq!(MockProvider::synthesis_count(&regenerated.finish()), 1);
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

/// 目录可用，但合成请求被接收后延迟再断开：让等待方真的在等锁。
fn provider_with_catalog_and_delayed_dropped_synthesis(delay: StdDuration) -> MockProvider {
    let catalog = catalog_response();
    MockProvider::serve(Response {
        status: Response::status(200),
        body: Arc::new(move |path: &str| {
            if path.ends_with("/v1/t2a_v2") {
                thread::sleep(delay);
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

/// 锁等待超时：稳定的 `SPEECH_IN_PROGRESS`，并携带当前 attempt ID。
///
/// 锁文件是手工占用而不是靠第二个进程，因此等待方一定进入超时分支；
/// 环境变量把超时缩短到毫秒级，测试不必真的等 20 秒。
#[test]
fn a_lock_timeout_reports_speech_in_progress_with_the_current_attempt_id() {
    let fixture = Fixture::new();
    let first_provider = provider_with_catalog_and_synthesis("trace-lock-1");
    let first = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &first_provider,
        Some(TEST_KEY),
    ));
    first_provider.finish();
    let clip_id = first["receipt"]["clip_id"].as_str().expect("clip id");
    let attempt_id = first["receipt"]["attempt_id"].as_str().expect("attempt id");

    // 另一个进程正在生成同一 clip。
    fixture.occupy_lock(clip_id);
    let waiting = provider_with_catalog_and_synthesis("trace-lock-2");
    let value = failed(&fixture.run_with_env(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--regenerate", "--json",
        ],
        &waiting,
        Some(TEST_KEY),
        &[("APPLE_BOOKS_SPEECH_LOCK_TIMEOUT_MS", "40")],
    ));
    let records = waiting.finish();

    assert_eq!(value["error"]["code"], "SPEECH_IN_PROGRESS");
    assert_eq!(value["error"]["details"]["reason"], "in_progress");
    assert_eq!(
        value["error"]["details"]["attempt_id"],
        attempt_id,
        "a lock timeout must carry the current attempt id when it is available"
    );
    assert_eq!(
        records.len(),
        0,
        "waiting for the writer lock must not contact the provider at all"
    );

    // 锁释放后同一请求继续正常命中缓存。
    fixture.release_lock(clip_id);
    let after = provider_with_catalog_and_synthesis("trace-lock-3");
    let cached = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &after,
        Some(TEST_KEY),
    ));
    assert_eq!(cached["receipt"]["source"], "cache");
    assert_eq!(after.finish().len(), 0);
}

/// 失败的 regenerate：旧 version 仍是 current pointer，且不留下阻塞门。
#[test]
fn an_uncertain_regeneration_keeps_the_previous_version_and_gate_free() {
    let fixture = Fixture::new();
    let first_provider = provider_with_catalog_and_synthesis("trace-regen-1");
    let first = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &first_provider,
        Some(TEST_KEY),
    ));
    first_provider.finish();
    let clip_id = first["receipt"]["clip_id"].as_str().expect("clip id");
    let first_audio = first["receipt"]["audio"]["sha256"].as_str().expect("audio sha");

    // provider 接收请求后断开：新 attempt 结果不确定。
    let failing = provider_with_dropped_synthesis();
    let value = failed(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--regenerate", "--json",
        ],
        &failing,
        Some(TEST_KEY),
    ));
    let failing_records = failing.finish();
    assert_eq!(value["error"]["code"], "SPEECH_RESULT_UNKNOWN");
    assert_eq!(value["error"]["details"]["outcome"], "unknown");
    assert_eq!(MockProvider::synthesis_count(&failing_records), 1);

    let state = fixture.clip_state(clip_id);
    assert_eq!(
        state["current_audio_sha256"], first_audio,
        "an uncertain regeneration must not move the current pointer"
    );
    assert_eq!(state["current_cache_status"], "ready");
    assert_eq!(
        state["generation_blocked"], false,
        "a failed regeneration must not gate a clip that still has a valid version"
    );
    assert_eq!(state["latest_attempt_status"], "unknown");
    assert_eq!(
        std::fs::read_dir(fixture.speech_root().join("clips").join(clip_id).join("versions"))
            .expect("versions")
            .count(),
        1,
        "a rejected regeneration must not add an audio version"
    );

    // 普通 generate 继续复用旧音频：零连接。
    let replay = provider_with_catalog_and_synthesis("trace-regen-2");
    let cached = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &replay,
        Some(TEST_KEY),
    ));
    let replay_records = replay.finish();
    assert_eq!(cached["receipt"]["source"], "cache");
    assert_eq!(cached["receipt"]["audio"]["sha256"], first_audio);
    assert_eq!(
        replay_records.len(),
        0,
        "the previous cache version must stay usable without another provider call"
    );
}

/// 每个真实请求得到独立 attempt ID 与一条 metadata-only 历史记录。
#[test]
fn each_real_request_creates_a_distinct_metadata_only_attempt_record() {
    let fixture = Fixture::new();
    let first_provider = provider_with_catalog_and_synthesis("trace-att-1");
    let first = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &first_provider,
        Some(TEST_KEY),
    ));
    first_provider.finish();

    let regenerate_provider = provider_with_catalog_and_synthesis("trace-att-2");
    let second = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--regenerate", "--json",
        ],
        &regenerate_provider,
        Some(TEST_KEY),
    ));
    regenerate_provider.finish();

    let first_attempt = first["receipt"]["attempt_id"].as_str().expect("attempt 1");
    let second_attempt = second["receipt"]["attempt_id"].as_str().expect("attempt 2");
    assert_ne!(
        first_attempt, second_attempt,
        "each real provider request must get its own attempt id"
    );

    let clip_id = first["receipt"]["clip_id"].as_str().expect("clip id");
    let mut recorded = Vec::new();
    let mut files = 0usize;
    for day in std::fs::read_dir(fixture.speech_root().join("attempts")).expect("attempts dir") {
        for file in std::fs::read_dir(day.expect("day").path()).expect("attempt files") {
            let path = file.expect("file").path();
            files += 1;
            let text = std::fs::read_to_string(&path).expect("attempt record");
            assert!(
                !text.contains("高亮正文"),
                "attempt history must not store the Speech Text"
            );
            assert!(
                !text.contains(TEST_KEY),
                "attempt history must not store the API key"
            );
            for forbidden in ["audio.mp3", "audio_bytes", "audio_hex", "fffb"] {
                assert!(
                    !text.contains(forbidden),
                    "attempt history must not store audio payloads ({forbidden})"
                );
            }
            let record: Value = serde_json::from_str(&text).expect("attempt JSON");
            recorded.push(record["attempt_id"].as_str().expect("id").to_string());
        }
    }
    assert_eq!(files, 2, "two real requests must leave two history records");
    recorded.sort();
    let mut expected = vec![first_attempt.to_string(), second_attempt.to_string()];
    expected.sort();
    assert_eq!(recorded, expected);
    assert_eq!(fixture.current_audio_sha256(clip_id), first["receipt"]["audio"]["sha256"].as_str().expect("sha").to_string());
}

/// `speech cache clear` 是显式清除入口；`speech history clear` 不清 gate。
#[test]
fn cache_clear_lifts_the_gate_while_history_clear_keeps_it() {
    let fixture = Fixture::new();
    let unknown = provider_with_dropped_synthesis();
    let value = failed(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &unknown,
        Some(TEST_KEY),
    ));
    assert_eq!(value["error"]["code"], "SPEECH_RESULT_UNKNOWN");
    assert_eq!(MockProvider::synthesis_count(&unknown.finish()), 1);
    let clip_id = fixture.clip_dirs()[0]
        .file_name()
        .expect("clip dir name")
        .to_string_lossy()
        .into_owned();
    assert_eq!(fixture.clip_state(&clip_id)["generation_blocked"], true);

    // history clear 只删 attempt metadata；mock 必须保持零连接。
    let untouched = provider_with_catalog_and_synthesis("trace-history");
    let history = succeeded(&fixture.run_with(&["speech", "history", "clear", "--json"], &untouched, Some(TEST_KEY)));
    let history_records = untouched.finish();
    assert_eq!(history["receipt"]["operation"], "history_clear");
    assert!(history["receipt"]["removed_attempts"].as_u64().expect("removed") >= 1);
    assert_eq!(history["receipt"]["cleared_generation_gates"], 0);
    assert_eq!(history_records.len(), 0, "history clear must not contact the provider");
    assert_eq!(
        fixture.clip_state(&clip_id)["generation_blocked"], true,
        "history clear must not lift the generation gate"
    );

    // 普通 generate 仍然被 gate 挡住：零连接。
    let blocked = provider_with_catalog_and_synthesis("trace-blocked");
    let blocked_value = failed(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &blocked,
        Some(TEST_KEY),
    ));
    assert_eq!(blocked_value["error"]["code"], "SPEECH_RESULT_UNKNOWN");
    assert_eq!(blocked.finish().len(), 0);

    // cache clear：显式清除 gate 与 clip state。
    let cleared = provider_with_catalog_and_synthesis("trace-clear");
    let cleared_value = succeeded(&fixture.run_with(&["speech", "cache", "clear", "--json"], &cleared, Some(TEST_KEY)));
    let cleared_records = cleared.finish();
    assert_eq!(cleared_value["receipt"]["operation"], "cache_clear");
    assert_eq!(cleared_value["receipt"]["removed"][0], clip_id);
    assert_eq!(cleared_value["receipt"]["cleared_generation_gates"], 1);
    assert_eq!(
        cleared_records.len(),
        0,
        "cache clear must not contact the provider"
    );
    assert!(
        fixture.clip_dirs().is_empty(),
        "cache clear must remove the gated clip state"
    );

    // 清除后普通 generate 才能创建新的 attempt。
    let recovered = provider_with_catalog_and_synthesis("trace-recovered");
    let recovered_value = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &recovered,
        Some(TEST_KEY),
    ));
    assert_eq!(recovered_value["receipt"]["source"], "provider");
    assert_eq!(MockProvider::synthesis_count(&recovered.finish()), 1);
}

/// 持锁 entry 不可淘汰：cache clear 跳过它，且该缓存继续可用。
#[test]
fn cache_clear_skips_an_entry_that_holds_the_writer_lock() {
    let fixture = Fixture::new();
    let provider = provider_with_catalog_and_synthesis("trace-skip-1");
    let first = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    provider.finish();
    let clip_id = first["receipt"]["clip_id"].as_str().expect("clip id");

    fixture.occupy_lock(clip_id);
    let clearing = provider_with_catalog_and_synthesis("trace-skip-2");
    let value = succeeded(&fixture.run_with(&["speech", "cache", "clear", "--json"], &clearing, Some(TEST_KEY)));
    assert_eq!(value["receipt"]["removed"].as_array().map(Vec::len), Some(0));
    assert_eq!(value["receipt"]["skipped"][0], clip_id);
    assert_eq!(value["receipt"]["cleared_generation_gates"], 0);
    assert_eq!(clearing.finish().len(), 0);

    // 跳过的 entry 在锁释放后仍然是有效缓存。
    fixture.release_lock(clip_id);
    let replay = provider_with_catalog_and_synthesis("trace-skip-3");
    let cached = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &replay,
        Some(TEST_KEY),
    ));
    assert_eq!(cached["receipt"]["source"], "cache");
    assert_eq!(replay.finish().len(), 0);

    let cleared = provider_with_catalog_and_synthesis("trace-skip-4");
    let value = succeeded(&fixture.run_with(&["speech", "cache", "clear", "--json"], &cleared, Some(TEST_KEY)));
    assert_eq!(value["receipt"]["removed"][0], clip_id);
    assert_eq!(cleared.finish().len(), 0);
}

/// 目录可用，但合成请求被接收后延迟再以 5xx 拒绝：让等待方真的在等锁。
fn provider_with_catalog_and_delayed_failed_synthesis(delay: StdDuration) -> MockProvider {
    let catalog = catalog_response();
    MockProvider::serve(Response {
        status: Arc::new(move |path: &str| {
            if path.ends_with("/v1/t2a_v2") {
                500
            } else {
                200
            }
        }),
        body: Arc::new(move |path: &str| {
            if path.ends_with("/v1/t2a_v2") {
                thread::sleep(delay);
                br#"{"code":"internal","message":"boom"}"#.to_vec()
            } else {
                catalog.clone()
            }
        }),
        delay: None,
        drop: Response::never_drop(),
    })
}

/// 同 clip 的并发等待方收到首个终态错误，且不产生第二次 provider 请求。
///
/// 合成响应被故意延迟并断开：两个真实子进程，恰好一次 provider 连接，
/// 两个进程都得到 `SPEECH_RESULT_UNKNOWN`。
#[test]
fn a_concurrent_waiter_receives_the_first_terminal_error_without_a_second_provider_call() {
    let fixture = Fixture::new();
    let provider = provider_with_catalog_and_delayed_dropped_synthesis(StdDuration::from_millis(700));

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
        outputs.push(child.wait_with_output().expect("wait for CLI"));
    }
    let records = provider.finish();

    let values: Vec<Value> = outputs
        .iter()
        .map(|output| {
            let value = failed(output);
            assert_eq!(
                value["error"]["code"], "SPEECH_RESULT_UNKNOWN",
                "both callers must receive the first attempt's terminal error"
            );
            value
        })
        .collect();
    assert_eq!(
        MockProvider::synthesis_count(&records),
        1,
        "a same-clip waiter must never start a second provider request"
    );
    let clip_id = values[0]["error"]["details"]["attempt_id"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_default();
    assert!(
        !clip_id.is_empty(),
        "the unknown gate must name the attempt that produced it"
    );
}

/// 失败的 `--regenerate` 提交：旧 version 仍是 current pointer，且不留半成品。
///
/// 用一个普通文件占住 Speech 状态根的 tmp 目录，让「provider 已成功」之后的原子放置
/// 必然失败。`state.json` 必须继续指向旧音频，普通 generate 也必须继续命中它。
#[test]
fn a_failed_regenerate_commit_leaves_the_old_pointer_and_audio_intact() {
    let fixture = Fixture::new();
    let first_provider = provider_with_catalog_and_synthesis("trace-atomic-1");
    let first = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &first_provider,
        Some(TEST_KEY),
    ));
    first_provider.finish();
    let clip_id = first["receipt"]["clip_id"].as_str().expect("clip id");
    let first_audio = first["receipt"]["audio"]["sha256"].as_str().expect("audio sha");
    let versions = fixture.speech_root().join("clips").join(clip_id).join("versions");
    assert_eq!(std::fs::read_dir(&versions).expect("versions").count(), 1);

    // 占住 tmp：新 version 的原子放置必然失败，但旧 pointer 不能被丢弃。
    // 首次生成已经建过 tmp 目录，先移除再换成普通文件。
    std::fs::remove_dir_all(fixture.speech_root().join("tmp")).expect("remove tmp dir");
    std::fs::create_dir_all(fixture.speech_root()).expect("speech root");
    std::fs::write(fixture.speech_root().join("tmp"), b"not a directory").expect("block tmp");
    let catalog = catalog_response();
    // 不同帧数 → 不同音频字节 → 新的 version 目录（否则 intact 版本会被直接复用）。
    let synthesis = synthesis_response("trace-atomic-2", 3);
    let failing = MockProvider::serve(Response {
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
    });
    let value = failed(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--regenerate", "--json",
        ],
        &failing,
        Some(TEST_KEY),
    ));
    let failing_records = failing.finish();
    assert_eq!(value["error"]["code"], "SPEECH_ARTIFACT_COMMIT_FAILED");
    assert_eq!(
        value["error"]["details"]["outcome"],
        "provider_succeeded_artifact_missing"
    );
    assert_eq!(MockProvider::synthesis_count(&failing_records), 1);

    let state = fixture.clip_state(clip_id);
    assert_eq!(
        state["current_audio_sha256"], first_audio,
        "a failed commit must not move the current pointer"
    );
    assert_eq!(state["current_cache_status"], "ready");
    assert_eq!(
        state["generation_blocked"], false,
        "the old valid version keeps the clip ungated after a failed commit"
    );
    assert_eq!(
        state["latest_attempt_status"],
        "provider_succeeded_artifact_missing"
    );
    assert_eq!(
        std::fs::read_dir(&versions).expect("versions").count(),
        1,
        "a failed commit must not leave a partially placed version"
    );

    // 普通 generate 继续播放/复用旧音频：零连接。
    let replay = provider_with_catalog_and_synthesis("trace-atomic-3");
    let cached = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &replay,
        Some(TEST_KEY),
    ));
    assert_eq!(cached["receipt"]["source"], "cache");
    assert_eq!(cached["receipt"]["audio"]["sha256"], first_audio);
    assert_eq!(replay.finish().len(), 0);

    // 腾出 tmp 后显式 --regenerate 仍然可以替换当前版本。
    std::fs::remove_file(fixture.speech_root().join("tmp")).expect("unblock tmp");
    let recovered = provider_with_catalog_and_synthesis("trace-atomic-4");    let value = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--regenerate", "--json",
        ],
        &recovered,
        Some(TEST_KEY),
    ));
    assert_eq!(value["receipt"]["source"], "provider");
    assert_eq!(MockProvider::synthesis_count(&recovered.finish()), 1);
    assert_ne!(
        value["receipt"]["attempt_id"],
        first["receipt"]["attempt_id"],
        "the recovered regeneration is a new Speech Attempt"
    );
    // 相同音频内容是内容寻址的：version 目录被复用，pointer 仍然只有一个。
    assert_eq!(fixture.clip_state(clip_id)["generation_blocked"], false);
    assert_eq!(std::fs::read_dir(&versions).expect("versions").count(), 1);
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

/// 明确的 provider 失败也是终态：同一 clip 的排队等待方必须原样拿到首个失败，
/// 两个真实子进程合计只有一次 provider 合成连接。
///
/// 合成被故意延迟拒绝，保证第二个进程真的在等 writer 锁，而不是碰巧串行完成。
/// 显式失败不写 generation gate（`blocks_generation == false`），所以这条路径只能靠
/// 锁内的终态复查来阻止「同一 clip 的第二次计费请求」（ADR 0007、实施 spec 9
/// 「failed：返回相同稳定失败，不自动调用 provider」）。
#[test]
fn a_failed_waiter_receives_the_first_terminal_failure_with_exactly_one_provider_connection() {
    let fixture = Fixture::new();
    let provider =
        provider_with_catalog_and_delayed_failed_synthesis(StdDuration::from_millis(900));

    let mut children = Vec::new();
    // 第一个子进程先跑起来并持锁进入被延迟的 provider 调用。
    for index in 0..2 {
        if index == 1 {
            thread::sleep(StdDuration::from_millis(350));
        }
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
        outputs.push(child.wait_with_output().expect("wait for CLI"));
    }
    let records = provider.finish();

    let values: Vec<Value> = outputs
        .iter()
        .map(|output| {
            let value = failed(output);
            assert_eq!(
                value["error"]["code"], "SPEECH_PROVIDER_FAILED",
                "an explicit provider failure is a stable terminal error"
            );
            assert_eq!(value["error"]["details"]["outcome"], "failed");
            value
        })
        .collect();
    assert_eq!(
        values[0]["error"]["details"]["attempt_id"], values[1]["error"]["details"]["attempt_id"],
        "the queued waiter must receive the first attempt's terminal error verbatim"
    );
    assert!(
        !values[0]["error"]["details"]["attempt_id"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "the terminal error must name the attempt that produced it"
    );
    assert_eq!(
        MockProvider::synthesis_count(&records),
        1,
        "a same-clip waiter must never start a second provider request: duplicate billing"
    );
    assert_eq!(
        records.len(),
        2,
        "the waiter must return the recorded failure before any preflight, so it opens no connection at all"
    );

    // 明确失败不是不确定结果：不写 gate，也不留任何音频版本。
    let clip_id = fixture.clip_dirs()[0]
        .file_name()
        .expect("clip dir name")
        .to_string_lossy()
        .into_owned();
    let state = fixture.clip_state(&clip_id);
    assert_eq!(state["latest_attempt_status"], "provider_failed");
    assert_eq!(state["generation_blocked"], false);
    let versions = fixture
        .speech_root()
        .join("clips")
        .join(&clip_id)
        .join("versions");
    assert!(
        versions
            .read_dir()
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(true),
        "an explicit failure must not leave a cached audio version"
    );
}

/// attempt history 的 90 天保留是自动维护：过期 metadata 被删，generation gate、
/// current pointer 与缓存都不动，`latest_attempt_id` 也不再指向被删掉的 attempt。
#[test]
fn expired_attempt_history_is_pruned_automatically_without_touching_the_gate_or_the_cache() {
    let fixture = Fixture::new();
    let gated = provider_with_dropped_synthesis();
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
        &gated,
        Some(TEST_KEY),
    ));
    assert_eq!(value["error"]["code"], "SPEECH_RESULT_UNKNOWN");
    assert_eq!(MockProvider::synthesis_count(&gated.finish()), 1);
    let clip_id = fixture.clip_dirs()[0]
        .file_name()
        .expect("clip dir name")
        .to_string_lossy()
        .into_owned();
    assert_eq!(fixture.clip_state(&clip_id)["generation_blocked"], true);

    // 把这条 attempt 历史改到保留窗口之外（元数据只有，不碰音频与 state）。
    let attempts_dir = fixture.speech_root().join("attempts");
    let mut records = Vec::new();
    for day in std::fs::read_dir(&attempts_dir)
        .expect("attempts dir")
        .flatten()
    {
        for file in std::fs::read_dir(day.path()).expect("records").flatten() {
            records.push(file.path());
        }
    }
    assert_eq!(
        records.len(),
        1,
        "the uncertain outcome left one history record"
    );
    let expired_at = (chrono::Utc::now() - chrono::Duration::days(120)).to_rfc3339();
    let mut record: Value =
        serde_json::from_slice(&std::fs::read(&records[0]).expect("record")).expect("attempt JSON");
    record["started_at"] = json!(expired_at);
    record["finished_at"] = json!(expired_at);
    std::fs::write(
        &records[0],
        serde_json::to_vec_pretty(&record).expect("record JSON"),
    )
    .expect("backdate the record");

    // 另一个 clip 的新 attempt 触发「正常维护」：过期历史必须被自动删除。
    let other = provider_with_catalog_and_synthesis("trace-prune-2");
    let generated = succeeded(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-42",
            "--content",
            "highlight",
            "--json",
        ],
        &other,
        Some(TEST_KEY),
    ));
    assert_eq!(generated["receipt"]["source"], "provider");
    assert_eq!(MockProvider::synthesis_count(&other.finish()), 1);

    let mut remaining = Vec::new();
    for day in std::fs::read_dir(&attempts_dir)
        .expect("attempts dir")
        .flatten()
    {
        for file in std::fs::read_dir(day.path()).expect("records").flatten() {
            remaining.push(file.path());
        }
    }
    assert_eq!(
        remaining.len(),
        1,
        "only the in-window attempt history may survive the automatic prune"
    );

    let state = fixture.clip_state(&clip_id);
    assert_eq!(
        state["latest_attempt_id"],
        Value::Null,
        "clip state must never keep naming a pruned attempt"
    );
    assert_eq!(
        state["latest_attempt_status"], "unknown",
        "the recorded outcome still explains the surviving gate"
    );
    assert_eq!(
        state["generation_blocked"], true,
        "the unknown gate must survive attempt history retention"
    );

    // 阻塞态仍然挡住普通 generate：零连接。
    let blocked = provider_with_catalog_and_synthesis("trace-prune-3");
    let blocked_value = failed(&fixture.run_with(
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
        &blocked,
        Some(TEST_KEY),
    ));
    assert_eq!(blocked_value["error"]["code"], "SPEECH_RESULT_UNKNOWN");
    assert_eq!(blocked.finish().len(), 0);

    // 清理只删 attempt metadata：另一个 clip 的缓存仍然零连接命中。
    let replay = provider_with_catalog_and_synthesis("trace-prune-4");
    let cached = succeeded(&fixture.run_with(
        &[
            "speech",
            "generate",
            "--asset-id",
            "book-1",
            "--annotation-id",
            "annotation-42",
            "--content",
            "highlight",
            "--json",
        ],
        &replay,
        Some(TEST_KEY),
    ));
    assert_eq!(cached["receipt"]["source"], "cache");
    assert_eq!(replay.finish().len(), 0);
}
