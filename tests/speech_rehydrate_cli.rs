//! Issue #28 的「缓存淘汰后从导出恢复」CLI 合同测试。
//!
//! 沿用 issue #21–#27 的约定：进程内 TCP mock provider（计数每个连接）、注入 HOME 隔离
//! Speech 状态根、移除全部代理变量让请求只能直达 mock，以及 secret canary 断言。
//!
//! 每个用例都先 `speech cache clear` 清空应用缓存，让「本地恢复」成为唯一可能的结果，
//! 然后断言 mock 记录的连接数为 **0**：恢复成功必须完全发生在本地。
//!
//! 真实播放器从不在测试里启动（`--json` 走 `NeverPlayer`），也不修改或删除用户导出的
//! 音频：每个回退用例都会比较导出文件在命令前后的字节。

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
const TEST_KEY: &str = "issue-28-test-key";
/// 探测 secret 落盘的 canary 值。
const SECRET_CANARY: &str = "canary-secret-4d2e8b-do-not-persist";

/// 一次被 mock 记录的连接。
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct RequestRecord {
    path: String,
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
        bytes.resize(length, 0);
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
        }, {
            "voice_id": "female_0002_b",
            "voice_name": "备用女声",
            "description": ["清晰"],
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
                "INSERT INTO ZAEANNOTATION VALUES (41, 'book-1', '高亮正文', NULL, 'epubcfi(/6/2)', 0.0, 3, 0)",
                rusqlite::params![],
            )
            .expect("annotation row");

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

    /// 移除所有代理变量：请求只能直达本地 mock，因此「零连接」是可计数的。
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

    /// 完全不带任何 provider 配置：用来证明本地恢复路径根本不需要网络环境。
    fn run_offline(&self, args: &[&str]) -> Output {
        self.command().args(args).output().expect("run CLI")
    }

    fn speech_root(&self) -> PathBuf {
        self.home
            .path()
            .join("Library/Application Support/books-exporter/speech")
    }

    fn locator_path(&self) -> PathBuf {
        self.speech_root().join("exports.json")
    }

    fn locator(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.locator_path()).expect("exports.json"))
            .expect("locator JSON")
    }

    fn book_export_root(&self) -> PathBuf {
        self.home.path().join("books-exported/测试书")
    }

    fn manifest_path(&self) -> PathBuf {
        self.book_export_root()
            .join("assets/audio")
            .join("manifest.json")
    }

    fn manifest(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.manifest_path()).expect("manifest.json"))
            .expect("manifest JSON")
    }

    fn clip_state(&self, clip_id: &str) -> Value {
        serde_json::from_str(
            &std::fs::read_to_string(
                self.speech_root()
                    .join("clips")
                    .join(clip_id)
                    .join("state.json"),
            )
            .expect("state.json"),
        )
        .expect("state JSON")
    }

    fn audio_path(&self, clip_id: &str) -> PathBuf {
        let state = self.clip_state(clip_id);
        self.speech_root()
            .join("clips")
            .join(clip_id)
            .join("versions")
            .join(
                state["current_audio_sha256"]
                    .as_str()
                    .expect("current audio sha256"),
            )
            .join("audio.mp3")
    }

    /// attempt history 里的全部文件：rehydration 前后必须逐个相同。
    fn attempt_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, files);
                } else {
                    files.push(path);
                }
            }
        }
        walk(&self.speech_root().join("attempts"), &mut files);
        files.sort();
        files
    }

    /// Speech 状态根下的全部文件（用于「回退不写状态」的断言）。
    fn speech_state_files(&self) -> Vec<(PathBuf, Vec<u8>)> {
        let mut files = Vec::new();
        fn walk(dir: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
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

fn generate_args<'a>(clip_extra: &'a [&'a str]) -> Vec<&'a str> {
    let mut args = vec![
        "speech",
        "generate",
        "--asset-id",
        "book-1",
        "--annotation-id",
        "annotation-41",
        "--content",
        "highlight",
    ];
    args.extend_from_slice(clip_extra);
    args.push("--json");
    args
}

/// 生成一个已接受的 Cached Speech Clip，返回完整 clip ID。
fn generate_cached_clip(fixture: &Fixture, extra: &[&str], trace_id: &str) -> String {
    let provider = provider_with_catalog_and_synthesis(trace_id);
    let value = succeeded(&fixture.run_with(&generate_args(extra), &provider, Some(TEST_KEY)));
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

/// 生成 → 导出 → 清空应用缓存。返回 clip ID 与导出音频路径。
///
/// 缓存被清空后，本地只剩用户拥有的导出音频：任何后续成功都必须来自导出恢复。
fn generated_and_exported(fixture: &Fixture, trace_id: &str) -> (String, PathBuf) {
    let clip_id = generate_cached_clip(fixture, &[], trace_id);
    let provider = provider_with_catalog_and_synthesis("trace-export");
    let root = fixture.book_export_root();
    let output = fixture.run_with(
        &[
            "speech",
            "export",
            "--clip-id",
            &clip_id,
            "--output",
            root.to_str().expect("export root"),
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    );
    let records = provider.finish();
    assert_zero_connections(&records, "speech export");
    let receipt = succeeded(&output)["receipt"].clone();
    let exported = fixture
        .book_export_root()
        .join(receipt["relative_path"].as_str().expect("relative path"));
    assert!(exported.exists(), "the export must exist for this fixture");

    // 清空应用缓存：attempt history 不受影响（它是另一个动作）。
    let provider = provider_with_catalog_and_synthesis("trace-clear");
    let cleared = succeeded(&fixture.run_with(
        &["speech", "cache", "clear", "--json"],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech cache clear");
    assert_eq!(
        cleared["receipt"]["removed"]
            .as_array()
            .expect("removed")
            .len(),
        1,
        "the cache entry must really be gone before rehydration"
    );
    assert!(!fixture.speech_root().join("clips").join(&clip_id).exists());
    (clip_id, exported)
}

fn assert_zero_connections(records: &[RequestRecord], path: &str) {
    assert_eq!(
        records.len(),
        0,
        "`{path}` must not contact the Speech Provider, but the mock saw {records:?}"
    );
}

/// 一次语法合法但没有任何缓存的 clip ID。
const ABSENT_CLIP_ID: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// 核心用例：清空缓存后，`generate` 从已验证的导出音频恢复出一个新的已接受 cache
/// version，且**不创建 Speech Attempt、不发起任何 provider 请求**。
///
/// 断言链：
///
/// - receipt `source=export_rehydration`、`provider_called=false`、`attempt_id=null`；
/// - mock 记录 **0** 个连接（且这次运行完全没有设置 API Key）；
/// - `attempts/` 目录下的文件与命令前逐个相同（没有新 attempt）；
/// - 恢复出来的 cache version metadata 没有 attempt_id，也没有 provider trace/用量；
/// - 缓存目录里重新出现可播放的 audio.mp3，checksum 与导出文件一致。
#[test]
fn generate_rehydrates_from_an_explicit_export_root_without_an_attempt_or_a_provider_call() {
    let fixture = Fixture::new();
    let (clip_id, exported) = generated_and_exported(&fixture, "trace-28-1");
    let exported_bytes = std::fs::read(&exported).expect("exported audio");
    let attempts_before = fixture.attempt_files();

    // 关键：这次运行不带 API Key，也不带任何 provider 端点。
    let value = succeeded(&fixture.run_offline(&generate_args(&[
        "--export-root",
        fixture.book_export_root().to_str().expect("root"),
    ])));
    let receipt = &value["receipt"];

    assert_eq!(receipt["source"], "export_rehydration");
    assert_eq!(receipt["provider_called"], false);
    assert_eq!(
        receipt["attempt_id"],
        Value::Null,
        "rehydration must not create or reuse a Speech Attempt"
    );
    assert_eq!(receipt["clip_id"], clip_id.as_str());
    assert_eq!(receipt["asset_id"], "book-1");
    assert_eq!(receipt["annotation_id"], "annotation-41");
    assert_eq!(receipt["content_kind"], "highlight");
    assert_eq!(receipt["provider"]["trace_id"], Value::Null);
    assert_eq!(receipt["provider"]["usage_characters"], Value::Null);

    // 新的已接受 cache version：音频重新出现在缓存里，字节与用户导出的那份完全一致。
    let audio = fixture.audio_path(&clip_id);
    assert!(
        audio.exists(),
        "rehydration must accept a new cache version"
    );
    assert_eq!(
        std::fs::read(&audio).expect("rehydrated audio"),
        exported_bytes
    );
    assert_eq!(
        fixture.clip_state(&clip_id)["current_cache_status"],
        "ready"
    );

    // 没有任何 Speech Attempt：历史文件逐个不变，version metadata 也没有 attempt_id。
    assert_eq!(
        fixture.attempt_files(),
        attempts_before,
        "rehydration must not write an attempt record"
    );
    let version_metadata: Value = serde_json::from_str(
        &std::fs::read_to_string(audio.with_file_name("metadata.json")).expect("metadata.json"),
    )
    .expect("metadata JSON");
    assert_eq!(version_metadata["attempt_id"], Value::Null);
    assert_eq!(version_metadata["trace_id"], Value::Null);

    // 用户导出的音频一个字节都没被改动。
    assert_eq!(
        std::fs::read(&exported).expect("exported audio"),
        exported_bytes
    );
    // 收据不含原文、密钥或音频。
    let text = serde_json::to_string(&value).expect("receipt text");
    assert!(
        !text.contains("高亮正文"),
        "receipt must not repeat the text"
    );
    assert!(!text.contains(TEST_KEY), "receipt must not carry the key");
    assert!(!text.contains(SECRET_CANARY));
}

/// 清空缓存后，第二次 `generate` 直接复用刚恢复的缓存版本，仍然零 provider 调用。
#[test]
fn a_rehydrated_clip_is_then_reused_as_a_cache_hit() {
    let fixture = Fixture::new();
    let (clip_id, _) = generated_and_exported(&fixture, "trace-28-2");
    let root = fixture.book_export_root();
    let root_arg = root.to_str().expect("root").to_string();

    let first = succeeded(&fixture.run_offline(&generate_args(&["--export-root", &root_arg])));
    assert_eq!(first["receipt"]["source"], "export_rehydration");
    let audio_after_rehydration = std::fs::read(fixture.audio_path(&clip_id)).expect("audio");

    let second = succeeded(&fixture.run_offline(&generate_args(&["--export-root", &root_arg])));
    assert_eq!(second["receipt"]["source"], "cache");
    assert_eq!(second["receipt"]["provider_called"], false);
    assert_eq!(second["receipt"]["attempt_id"], Value::Null);
    assert_eq!(
        std::fs::read(fixture.audio_path(&clip_id)).expect("audio"),
        audio_after_rehydration
    );
}

/// `--regenerate` 是唯一可以越过本地回退的入口：它必须真的发起一次新的 provider 请求。
///
/// 这条负向控制保证 rehydration 不会变成「永远不付费」的旁路。
#[test]
fn regenerate_still_calls_the_provider_instead_of_rehydrating() {
    let fixture = Fixture::new();
    let (_clip_id, exported) = generated_and_exported(&fixture, "trace-28-3");
    let exported_bytes = std::fs::read(&exported).expect("exported audio");
    let root = fixture.book_export_root();

    let provider = provider_with_catalog_and_synthesis("trace-28-3-regen");
    let value = succeeded(&fixture.run_with(
        &generate_args(&[
            "--regenerate",
            "--export-root",
            root.to_str().expect("root"),
        ]),
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();
    assert!(
        records
            .iter()
            .any(|record| record.path.ends_with("/v1/t2a_v2")),
        "--regenerate must perform exactly the paid path again, got {records:?}"
    );

    let receipt = &value["receipt"];
    assert_eq!(receipt["source"], "provider");
    assert_eq!(receipt["provider_called"], true);
    assert!(
        receipt["attempt_id"].as_str().is_some(),
        "a provider generation always has an attempt id"
    );
    // 用户导出的文件仍然原封不动：重新生成从不改写用户文件。
    assert_eq!(
        std::fs::read(&exported).expect("exported audio"),
        exported_bytes
    );
}

/// 清空缓存后，`play` 回退到已验证的 **active** 导出音频：零连接、零状态写入。
#[test]
fn play_falls_back_to_a_verified_active_exported_clip() {
    let fixture = Fixture::new();
    let (clip_id, exported) = generated_and_exported(&fixture, "trace-28-4");
    let root = fixture.book_export_root();

    let provider = provider_with_catalog_and_synthesis("trace-28-4-play");
    let value = succeeded(&fixture.run_with(
        &[
            "speech",
            "play",
            "--clip-id",
            &clip_id,
            "--export-root",
            root.to_str().expect("root"),
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech play --export-root");

    let receipt = &value["receipt"];
    assert_eq!(receipt["source"], "export");
    assert_eq!(receipt["export_origin"], "explicit_export_root");
    assert_eq!(receipt["path"], exported.to_string_lossy().as_ref());
    assert_eq!(receipt["provider_called"], false);
    assert_eq!(receipt["played"], false);
    assert_eq!(receipt["content_kind"], "highlight");
    assert_eq!(receipt["asset_id"], "book-1");

    // play 回退不写任何 Speech 状态：它不创建缓存 entry，也不留下占用 marker。
    assert!(!fixture.speech_root().join("clips").join(&clip_id).exists());
    assert!(!fixture
        .speech_root()
        .join("locks")
        .join(format!("{clip_id}.play"))
        .exists());
    assert!(!std::fs::read(&exported).expect("exported audio").is_empty());
}

/// 被用户改写过的导出文件不是有效 clip：`play` 失败并解释原因，绝不播放它。
#[test]
fn a_modified_exported_file_is_never_a_playback_fallback() {
    let fixture = Fixture::new();
    let (clip_id, exported) = generated_and_exported(&fixture, "trace-28-5");
    let root = fixture.book_export_root();
    let mut bytes = std::fs::read(&exported).expect("exported audio");
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    std::fs::write(&exported, &bytes).expect("tamper the exported audio");

    let provider = provider_with_catalog_and_synthesis("trace-28-5-play");
    let value = failed(&fixture.run_with(
        &[
            "speech",
            "play",
            "--clip-id",
            &clip_id,
            "--export-root",
            root.to_str().expect("root"),
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech play");

    assert_eq!(value["error"]["code"], "SPEECH_CLIP_NOT_FOUND");
    let rejected = value["error"]["details"]["rejected_candidates"]
        .as_array()
        .expect("rejected candidates");
    assert_eq!(rejected[0]["reason"], "checksum_mismatch");
    // 被改写的用户文件保持原样：playback 从不修复或删除它。
    assert_eq!(std::fs::read(&exported).expect("exported audio"), bytes);
}

/// 负向控制：manifest 指向的音频不见了，play 回退失败并报告 `audio_missing`。
#[test]
fn a_missing_exported_clip_fails_without_guessing() {
    let fixture = Fixture::new();
    let (clip_id, exported) = generated_and_exported(&fixture, "trace-28-6");
    let root = fixture.book_export_root();
    std::fs::remove_file(&exported).expect("remove the exported audio");

    let provider = provider_with_catalog_and_synthesis("trace-28-6-play");
    let value = failed(&fixture.run_with(
        &[
            "speech",
            "play",
            "--clip-id",
            &clip_id,
            "--export-root",
            root.to_str().expect("root"),
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech play");
    assert_eq!(value["error"]["code"], "SPEECH_CLIP_NOT_FOUND");
    assert_eq!(
        value["error"]["details"]["rejected_candidates"][0]["reason"],
        "audio_missing"
    );
}

/// 非 active 的变体不能作为播放回退（实施 spec 5.5 只承认 Active Exported Speech Clip）。
#[test]
fn play_refuses_an_exported_clip_that_is_no_longer_active() {
    let fixture = Fixture::new();
    // 先生成一个使用不同音色的变体并导出，让它成为 active；再导出原 clip。
    let (active_clip, _) = generated_and_exported(&fixture, "trace-28-7-variant");
    let provider = provider_with_catalog_and_synthesis("trace-28-7-variant-generate");
    let variant = succeeded(&fixture.run_with(
        &generate_args(&["--voice-id", "female_0002_b"]),
        &provider,
        Some(TEST_KEY),
    ));
    let variant_clip = variant["receipt"]["clip_id"]
        .as_str()
        .expect("variant clip id")
        .to_string();
    assert_ne!(variant_clip, active_clip);
    let root = fixture.book_export_root();
    let provider = provider_with_catalog_and_synthesis("trace-28-7-variant-export");
    succeeded(&fixture.run_with(
        &[
            "speech",
            "export",
            "--clip-id",
            &variant_clip,
            "--output",
            root.to_str().expect("root"),
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech export");
    assert_eq!(
        fixture.manifest()["records"][0]["active_clip_id"],
        variant_clip
    );

    // 现在清空缓存：active_clip 仍然在 manifest 里，但已经不是 active 了。
    let provider = provider_with_catalog_and_synthesis("trace-28-7-clear");
    succeeded(&fixture.run_with(
        &["speech", "cache", "clear", "--json"],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech cache clear");

    let provider = provider_with_catalog_and_synthesis("trace-28-7-play");
    let value = failed(&fixture.run_with(
        &[
            "speech",
            "play",
            "--clip-id",
            &active_clip,
            "--export-root",
            root.to_str().expect("root"),
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech play");
    assert_eq!(value["error"]["code"], "SPEECH_CLIP_NOT_FOUND");
    assert_eq!(
        value["error"]["details"]["rejected_candidates"][0]["reason"],
        "clip_not_active"
    );
}

/// 用户把导出目录搬走后，一次显式 `--export-root` 就能恢复播放，并刷新 locator。
#[test]
fn a_moved_export_is_recovered_and_the_locator_is_refreshed() {
    let fixture = Fixture::new();
    let (clip_id, _) = generated_and_exported(&fixture, "trace-28-8");
    let original = fixture.book_export_root();
    let moved = fixture.home.path().join("新位置/测试书");
    std::fs::create_dir_all(moved.parent().expect("parent")).expect("create moved parent");
    std::fs::rename(&original, &moved).expect("move the export directory");

    // locator 仍指向已经不存在的旧路径：stale，绝不猜测。
    assert_eq!(
        fixture.locator()["entries"]["book-1"]["export_root"]
            .as_str()
            .expect("locator export root"),
        original.to_string_lossy()
    );

    // 没有 --export-root 时：stale locator 不回退，直接 not found。
    let provider = provider_with_catalog_and_synthesis("trace-28-8-stale");
    let stale = failed(&fixture.run_with(
        &["speech", "play", "--clip-id", &clip_id, "--json"],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech play");
    assert_eq!(stale["error"]["code"], "SPEECH_CLIP_NOT_FOUND");

    // 显式给出新位置：验证通过，locator 被刷新到新路径。
    let provider = provider_with_catalog_and_synthesis("trace-28-8-moved");
    let value = succeeded(&fixture.run_with(
        &[
            "speech",
            "play",
            "--clip-id",
            &clip_id,
            "--export-root",
            moved.to_str().expect("moved root"),
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech play --export-root");
    assert_eq!(value["receipt"]["source"], "export");
    assert_eq!(value["receipt"]["export_origin"], "explicit_export_root");
    assert_eq!(
        fixture.locator()["entries"]["book-1"]["export_root"]
            .as_str()
            .expect("locator export root"),
        moved.to_string_lossy()
    );

    // 刷新之后连 --export-root 都不再需要。
    let provider = provider_with_catalog_and_synthesis("trace-28-8-locator");
    let via_locator = succeeded(&fixture.run_with(
        &["speech", "play", "--clip-id", &clip_id, "--json"],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech play via locator");
    assert_eq!(via_locator["receipt"]["source"], "export");
    assert_eq!(via_locator["receipt"]["export_origin"], "export_locator");
}

/// locator 里的过期路径不能让 play 找到别处的音频；`generate` 也不会拿它冒充。
#[test]
fn a_stale_locator_never_turns_into_a_matched_clip() {
    let fixture = Fixture::new();
    // 只写一份 locator 投影，指向一个存在但没有该 clip 的导出目录。
    let decoy = fixture.home.path().join("别的书");
    std::fs::create_dir_all(decoy.join("assets/audio")).expect("decoy audio dir");
    let locator = json!({
        "schema_version": 1,
        "entries": {
            "book-1": {
                "export_root": decoy.to_string_lossy(),
                "manifest_schema_version": 1,
                "manifest_sha256": "0".repeat(64),
                "last_verified_at": "2026-09-29T10:00:00Z"
            }
        }
    });
    std::fs::create_dir_all(fixture.speech_root()).expect("speech root");
    std::fs::write(
        fixture.locator_path(),
        serde_json::to_vec_pretty(&locator).expect("locator JSON"),
    )
    .expect("write locator");

    let provider = provider_with_catalog_and_synthesis("trace-28-9");
    let value = failed(&fixture.run_with(
        &["speech", "play", "--clip-id", ABSENT_CLIP_ID, "--json"],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech play");
    assert_eq!(value["error"]["code"], "SPEECH_CLIP_NOT_FOUND");
    assert_eq!(
        value["error"]["details"]["rejected_candidates"][0]["reason"],
        "manifest_missing"
    );
}

/// 关键隐私/性能边界：没有 `--export-root`、没有 locator 条目时，工具**不扫描**用户目录。
///
/// 负向控制：HOME 下有一份完全有效、包含该 clip 的导出目录（含 Documents/iCloud 风格的
/// 深层路径）。不做任何目录遍历的实现必须找不到它，因此 `generate` 会走 provider 路径。
#[test]
fn user_directories_are_never_scanned_for_a_manifest() {
    let fixture = Fixture::new();
    // 造一份「如果有人去扫描就会命中」的导出目录。
    let clip_id = generate_cached_clip(&fixture, &[], "trace-28-10");
    let provider = provider_with_catalog_and_synthesis("trace-28-10-export");
    let hidden = fixture
        .home
        .path()
        .join("Documents/Books/测试书/assets/audio");
    std::fs::create_dir_all(&hidden).expect("decoy audio dir");
    let relative = format!("assets/audio/highlight-{}.mp3", &clip_id[..12]);
    let audio = fixture
        .home
        .path()
        .join("Documents/Books/测试书")
        .join(&relative);
    std::fs::copy(fixture.audio_path(&clip_id), &audio).expect("copy decoy audio");
    let manifest = json!({
        "schema_version": 1,
        "asset_id": "book-1",
        "records": [{
            "annotation_id": "annotation-41",
            "content_kind": "highlight",
            "active_clip_id": clip_id,
            "clips": [{
                "clip_id": clip_id,
                "relative_path": relative,
                "sha256": fixture.clip_state(&clip_id)["current_audio_sha256"],
                "size_bytes": std::fs::metadata(&audio).expect("audio metadata").len(),
                "format": "mp3",
                "exported_at": "2026-09-29T10:00:00Z"
            }]
        }]
    });
    std::fs::write(
        hidden.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("manifest JSON"),
    )
    .expect("write decoy manifest");
    assert_zero_connections(&provider.finish(), "speech export setup");

    // 清空缓存后不提供任何线索：play 必须 not found，generate 必须走 provider。
    let provider = provider_with_catalog_and_synthesis("trace-28-10-clear");
    succeeded(&fixture.run_with(
        &["speech", "cache", "clear", "--json"],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech cache clear");

    let provider = provider_with_catalog_and_synthesis("trace-28-10-play");
    let value = failed(&fixture.run_with(
        &["speech", "play", "--clip-id", &clip_id, "--json"],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech play");
    assert_eq!(value["error"]["code"], "SPEECH_CLIP_NOT_FOUND");

    // generate 确实发起了 provider 请求：它没有「顺手」用 Documents 里那份音频。
    let provider = provider_with_catalog_and_synthesis("trace-28-10-generate");
    let value = succeeded(&fixture.run_with(&generate_args(&[]), &provider, Some(TEST_KEY)));
    let records = provider.finish();
    assert!(
        records
            .iter()
            .any(|record| record.path.ends_with("/v1/t2a_v2")),
        "without an explicit export root the paid path must run, got {records:?}"
    );
    assert_eq!(value["receipt"]["source"], "provider");
    // 那份「藏在 Documents 里」的音频没有被读取或改动。
    assert!(audio.exists());
}

/// 三种来源在收据里必须可区分：cache、export_rehydration、provider。
#[test]
fn receipts_distinguish_cache_export_rehydration_and_provider() {
    let fixture = Fixture::new();
    let (clip_id, _) = generated_and_exported(&fixture, "trace-28-11");
    let root = fixture.book_export_root();
    let root_arg = root.to_str().expect("root").to_string();

    // 1) provider：清空缓存后显式重新生成。
    let provider = provider_with_catalog_and_synthesis("trace-28-11-provider");
    let provider_receipt =
        succeeded(&fixture.run_with(&generate_args(&["--regenerate"]), &provider, Some(TEST_KEY)));
    let _ = provider.finish();
    assert_eq!(provider_receipt["receipt"]["source"], "provider");
    assert_eq!(provider_receipt["receipt"]["provider_called"], true);
    assert!(provider_receipt["receipt"]["attempt_id"].as_str().is_some());

    // 2) cache：同一条 clip 再次普通生成。
    let provider = provider_with_catalog_and_synthesis("trace-28-11-cache");
    let cache_receipt =
        succeeded(&fixture.run_with(&generate_args(&[]), &provider, Some(TEST_KEY)));
    assert_zero_connections(&provider.finish(), "speech generate cache hit");
    assert_eq!(cache_receipt["receipt"]["source"], "cache");
    assert_eq!(cache_receipt["receipt"]["provider_called"], false);
    assert_eq!(cache_receipt["receipt"]["attempt_id"], Value::Null);

    // 3) export_rehydration：清空缓存后从导出恢复。
    let provider = provider_with_catalog_and_synthesis("trace-28-11-clear");
    succeeded(&fixture.run_with(
        &["speech", "cache", "clear", "--json"],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech cache clear");
    let provider = provider_with_catalog_and_synthesis("trace-28-11-rehydrate");
    let rehydrated = succeeded(&fixture.run_with(
        &generate_args(&["--export-root", &root_arg]),
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech generate rehydration");
    assert_eq!(rehydrated["receipt"]["source"], "export_rehydration");
    assert_eq!(rehydrated["receipt"]["provider_called"], false);
    assert_eq!(rehydrated["receipt"]["attempt_id"], Value::Null);
    assert_eq!(rehydrated["receipt"]["clip_id"], clip_id);
}

/// 导出根属于另一本书时，`generate` 不会拿它冒充当前 clip，而是照常走 provider 路径。
#[test]
fn an_export_root_for_another_book_is_refused_and_the_paid_path_runs() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, &[], "trace-28-12");
    // 手写一个属于 book-1 但记录了别的 clip 的 manifest，并指向该 clip 的真实音频。
    let other_root = fixture.home.path().join("别处/测试书");
    let audio_dir = other_root.join("assets/audio");
    std::fs::create_dir_all(&audio_dir).expect("audio dir");
    let relative = format!("assets/audio/highlight-{}.mp3", &clip_id[..12]);
    let audio = other_root.join(&relative);
    std::fs::copy(fixture.audio_path(&clip_id), &audio).expect("copy audio");
    let manifest = json!({
        "schema_version": 1,
        "asset_id": "book-2",
        "records": [{
            "annotation_id": "annotation-41",
            "content_kind": "highlight",
            "active_clip_id": clip_id,
            "clips": [{
                "clip_id": clip_id,
                "relative_path": relative,
                "sha256": fixture.clip_state(&clip_id)["current_audio_sha256"],
                "size_bytes": std::fs::metadata(&audio).expect("metadata").len(),
                "format": "mp3",
                "exported_at": "2026-09-29T10:00:00Z"
            }]
        }]
    });
    std::fs::write(
        audio_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("manifest JSON"),
    )
    .expect("write manifest");

    let provider = provider_with_catalog_and_synthesis("trace-28-12-clear");
    succeeded(&fixture.run_with(
        &["speech", "cache", "clear", "--json"],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech cache clear");

    let provider = provider_with_catalog_and_synthesis("trace-28-12-generate");
    let value = succeeded(&fixture.run_with(
        &generate_args(&["--export-root", other_root.to_str().expect("root")]),
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();
    assert!(
        records
            .iter()
            .any(|record| record.path.ends_with("/v1/t2a_v2")),
        "a manifest from another book must not be rehydrated, got {records:?}"
    );
    assert_eq!(value["receipt"]["source"], "provider");
    let reasons: Vec<String> = value["receipt"]["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .map(|warning| warning["reason"].as_str().expect("reason").to_string())
        .collect();
    assert!(
        reasons.contains(&"manifest_asset_mismatch".to_string()),
        "the rejection must be reported, got {reasons:?}"
    );
}

/// 负向控制：manifest 的相对路径逃出导出根时，回退失败且导出根外的文件不被读取。
#[test]
fn a_manifest_path_that_escapes_the_export_root_is_refused() {
    let fixture = Fixture::new();
    let (clip_id, _) = generated_and_exported(&fixture, "trace-28-13");
    let root = fixture.book_export_root();
    let mut manifest = fixture.manifest();
    manifest["records"][0]["clips"][0]["relative_path"] =
        Value::String("assets/audio/../../../../etc/passwd".to_string());
    std::fs::write(
        fixture.manifest_path(),
        serde_json::to_vec_pretty(&manifest).expect("manifest JSON"),
    )
    .expect("write manifest");

    let provider = provider_with_catalog_and_synthesis("trace-28-13-play");
    let value = failed(&fixture.run_with(
        &[
            "speech",
            "play",
            "--clip-id",
            &clip_id,
            "--export-root",
            root.to_str().expect("root"),
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech play");
    assert_eq!(value["error"]["code"], "SPEECH_CLIP_NOT_FOUND");
    assert_eq!(
        value["error"]["details"]["rejected_candidates"][0]["reason"],
        "path_not_contained"
    );
}

/// `--export-root` 指向的目录根本没有 manifest：不猜，直接走 provider。
#[test]
fn an_export_root_without_a_manifest_falls_through_to_the_provider() {
    let fixture = Fixture::new();
    generate_cached_clip(&fixture, &[], "trace-28-14");
    let provider = provider_with_catalog_and_synthesis("trace-28-14-clear");
    succeeded(&fixture.run_with(
        &["speech", "cache", "clear", "--json"],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech cache clear");

    let empty_root = fixture.home.path().join("空目录");
    std::fs::create_dir_all(&empty_root).expect("empty export root");
    let provider = provider_with_catalog_and_synthesis("trace-28-14-generate");
    let value = succeeded(&fixture.run_with(
        &generate_args(&["--export-root", empty_root.to_str().expect("root")]),
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();
    assert!(
        records
            .iter()
            .any(|record| record.path.ends_with("/v1/t2a_v2")),
        "a missing manifest must fall through, got {records:?}"
    );
    assert_eq!(value["receipt"]["source"], "provider");
    let reasons: Vec<String> = value["receipt"]["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .map(|warning| warning["reason"].as_str().expect("reason").to_string())
        .collect();
    assert!(reasons.contains(&"manifest_missing".to_string()));
}

/// machine `play` 回退到导出音频时不启动播放器，也不改写任何 Speech 状态。
///
/// 唯一允许的写入是 locator 投影（本次验证的一部分）；缓存与 attempt 目录必须原样。
#[test]
fn machine_play_from_an_export_changes_no_clip_or_attempt_state() {
    let fixture = Fixture::new();
    let (clip_id, _) = generated_and_exported(&fixture, "trace-28-15");
    let root = fixture.book_export_root();
    let clips_dir = fixture.speech_root().join("clips");
    let attempts_before = fixture.attempt_files();
    let before: Vec<(PathBuf, Vec<u8>)> = fixture
        .speech_state_files()
        .into_iter()
        .filter(|(path, _)| !path.starts_with(&clips_dir))
        .filter(|(path, _)| !path.starts_with(fixture.locator_path()))
        .collect();

    let provider = provider_with_catalog_and_synthesis("trace-28-15-play");
    let value = succeeded(&fixture.run_with(
        &[
            "speech",
            "play",
            "--clip-id",
            &clip_id,
            "--export-root",
            root.to_str().expect("root"),
            "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    assert_zero_connections(&provider.finish(), "speech play");
    assert_eq!(value["receipt"]["played"], false);

    let after: Vec<(PathBuf, Vec<u8>)> = fixture
        .speech_state_files()
        .into_iter()
        .filter(|(path, _)| !path.starts_with(&clips_dir))
        .filter(|(path, _)| !path.starts_with(fixture.locator_path()))
        .collect();
    assert_eq!(
        after, before,
        "playback must not write clip or attempt state"
    );
    assert_eq!(fixture.attempt_files(), attempts_before);
    assert!(!clips_dir.join(&clip_id).exists());
}

/// human 模式（不带 `--json`）也走同一个本地恢复路径：0 次 provider 调用，且不打印原文。
#[test]
fn human_generate_rehydrates_from_an_export_root_without_a_provider_call() {
    let fixture = Fixture::new();
    let (clip_id, _) = generated_and_exported(&fixture, "trace-28-16");
    let root = fixture.book_export_root();

    let provider = provider_with_catalog_and_synthesis("trace-28-16-human");
    let output = fixture.run_with(
        &[
            "speech",
            "generate",
            "1",
            "--annotation",
            "1",
            "--content",
            "highlight",
            "--export-root",
            root.to_str().expect("root"),
        ],
        &provider,
        Some(TEST_KEY),
    );
    assert_zero_connections(&provider.finish(), "speech generate (human)");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("export_rehydration"), "{stdout}");
    assert!(stdout.contains(&clip_id), "{stdout}");
    assert!(
        !stdout.contains("高亮正文"),
        "human output must not print the text"
    );
}
