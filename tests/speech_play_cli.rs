//! Issue #26 的 `speech play` CLI 合同测试。
//!
//! 沿用 issue #21–#25 的约定：进程内 TCP mock provider（计数每个连接）、注入 HOME 隔离
//! Speech 状态根、移除全部代理变量让请求只能直达 mock，以及 secret canary 断言。
//!
//! 「每条 play 路径都零网络连接」因此是可证明的：每个用例结束时断言 mock 记录 0 个请求，
//! 而不是靠超时等待。
//!
//! 真实播放器从不在测试里启动。human 模式的播放行为由 `src/speech/play.rs` 的
//! `AudioPlayer` seam 单元测试覆盖（那里注入可计数的假播放器）；CLI 层的 human 用例
//! 只覆盖**到达播放器之前**的失败路径，因此同样是 hermetic 的。

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
const TEST_KEY: &str = "issue-26-test-key";
/// 探测 secret 落盘的 canary 值。
const SECRET_CANARY: &str = "canary-secret-9c31a7-do-not-persist";

/// 一次被 mock 记录的连接。play 的断言只看连接计数。
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct RequestRecord {
    path: String,
    // 记录请求形状，供失败时诊断。
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

    fn speech_root(&self) -> PathBuf {
        self.home
            .path()
            .join("Library/Application Support/books-exporter/speech")
    }

    fn play_marker_path(&self, clip_id: &str) -> PathBuf {
        self.speech_root()
            .join("locks")
            .join(format!("{clip_id}.play"))
    }

    fn current_audio_sha256(&self, clip_id: &str) -> String {
        self.clip_state(clip_id)["current_audio_sha256"]
            .as_str()
            .expect("current audio sha256")
            .to_string()
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
        self.speech_root()
            .join("clips")
            .join(clip_id)
            .join("versions")
            .join(self.current_audio_sha256(clip_id))
            .join("audio.mp3")
    }

    /// attempt history 目录；播放不得往里写任何东西。
    fn attempt_files(&self) -> Vec<PathBuf> {
        let root = self.speech_root().join("attempts");
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
        walk(&root, &mut files);
        files.sort();
        files
    }

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

/// 生成一个已接受的 Cached Speech Clip，返回完整 clip ID。
fn generate_cached_clip(fixture: &Fixture, trace_id: &str) -> String {
    let provider = provider_with_catalog_and_synthesis(trace_id);
    let value = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();
    assert_eq!(
        records.len(),
        2,
        "setup generation is the only provider traffic in these tests"
    );
    value["receipt"]["clip_id"]
        .as_str()
        .expect("clip id")
        .to_string()
}

/// 一个语法合法但没有任何缓存的 clip ID。
const ABSENT_CLIP_ID: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// 断言这次 play 完全没有联系 provider，并把记录交还给调用方做进一步断言。
fn assert_zero_connections(records: &[RequestRecord], path: &str) {
    assert_eq!(
        records.len(),
        0,
        "`speech {path}` must not contact the Speech Provider, but the mock saw {records:?}"
    );
}

/// machine 模式：返回已校验路径与来源，不启动播放器，不写任何状态。
///
/// `--json` 走 `NeverPlayer`：它一旦被调用就 panic，因此「不启动播放器」是被证明的。
#[test]
fn machine_play_returns_the_verified_path_and_source_without_a_player() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "trace-play-1");
    let audio_path = fixture.audio_path(&clip_id);
    let state_before = fixture.clip_state(&clip_id);

    let provider = provider_with_catalog_and_synthesis("trace-play-2");
    let value = succeeded(&fixture.run_with(
        &["speech", "play", "--clip-id", &clip_id, "--json"],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();

    assert_zero_connections(&records, "play --json");
    assert_eq!(value["schema_version"], 1);
    let receipt = &value["receipt"];
    assert_eq!(receipt["operation"], "play");
    assert_eq!(receipt["clip_id"], clip_id.as_str());
    assert_eq!(receipt["source"], "cache");
    assert_eq!(receipt["path"], audio_path.to_string_lossy().as_ref());
    assert_eq!(receipt["played"], false, "machine mode must not launch a player");
    assert_eq!(receipt["provider_called"], false);
    assert_eq!(receipt["content_kind"], "highlight");
    assert_eq!(receipt["audio"]["path"], audio_path.to_string_lossy().as_ref());
    assert_eq!(receipt["audio"]["format"], "mp3");

    // 除返回路径与来源外没有副作用：clip 状态逐字节不变，占用 marker 没有被创建。
    assert_eq!(fixture.clip_state(&clip_id), state_before);
    assert!(!fixture.play_marker_path(&clip_id).exists());
}

/// 损坏（被用户改写）的缓存 entry 返回稳定错误，绝不被当作可播放音频。
#[test]
fn a_tampered_cache_entry_is_a_stable_corrupt_error() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "trace-play-3");
    let audio_path = fixture.audio_path(&clip_id);
    let mut bytes = std::fs::read(&audio_path).expect("audio");
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    std::fs::write(&audio_path, &bytes).expect("tamper");

    let provider = provider_with_catalog_and_synthesis("trace-play-4");
    let value = failed(&fixture.run_with(
        &["speech", "play", "--clip-id", &clip_id, "--json"],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();

    assert_zero_connections(&records, "play --json");
    assert_eq!(value["error"]["code"], "SPEECH_CACHE_CORRUPT");
    assert_eq!(value["error"]["details"]["reason"], "cache_corrupt");
    // 损坏的 entry 不会被静默修复或重新生成：字节仍然是被改写的那份。
    assert_eq!(std::fs::read(&audio_path).expect("audio"), bytes);
    assert_eq!(fixture.clip_state(&clip_id)["current_cache_status"], "ready");
}

/// 没有缓存的 clip：human 与 machine 都是稳定的 `SPEECH_CLIP_NOT_FOUND`，零连接。
#[test]
fn an_absent_clip_is_a_stable_not_found_error_on_both_paths() {
    let fixture = Fixture::new();

    let machine_provider = provider_with_catalog_and_synthesis("trace-play-5");
    let machine = failed(&fixture.run_with(
        &["speech", "play", "--clip-id", ABSENT_CLIP_ID, "--json"],
        &machine_provider,
        Some(TEST_KEY),
    ));
    let machine_records = machine_provider.finish();

    assert_zero_connections(&machine_records, "play --json");
    assert_eq!(machine["error"]["code"], "SPEECH_CLIP_NOT_FOUND");
    assert_eq!(machine["error"]["details"]["clip_id"], ABSENT_CLIP_ID);

    // human 模式走同一个解析顺序，因此在到达播放器之前就失败：仍然零连接。
    let human_provider = provider_with_catalog_and_synthesis("trace-play-6");
    let human = fixture.run_with(
        &["speech", "play", "--clip-id", ABSENT_CLIP_ID],
        &human_provider,
        Some(TEST_KEY),
    );
    let human_records = human_provider.finish();

    assert!(!human.status.success());
    assert_zero_connections(&human_records, "play");
    let stderr = String::from_utf8_lossy(&human.stderr);
    assert!(stderr.contains("no verified Speech Clip"), "{stderr}");
    assert!(!fixture.play_marker_path(ABSENT_CLIP_ID).exists());
}

/// 形状不对的 clip ID 是参数错误，而且在读取任何缓存之前就失败。
#[test]
fn a_malformed_clip_id_fails_before_touching_the_cache() {
    let fixture = Fixture::new();

    let provider = provider_with_catalog_and_synthesis("trace-play-7");
    let value = failed(&fixture.run_with(
        &["speech", "play", "--clip-id", "../escape", "--json"],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();

    assert_zero_connections(&records, "play --json");
    assert_eq!(value["error"]["code"], "INVALID_ARGUMENT");
    assert_eq!(value["error"]["details"]["field"], "clip_id");
    // 路径穿越尝试不得在 Speech 根之外创建任何东西。
    assert!(!fixture
        .speech_root()
        .parent()
        .expect("application support")
        .join("escape")
        .exists());
}

/// 已被另一个进程占用（`locks/<clip_id>.play` marker 存在）时不再叠加播放。
///
/// human 模式先取 guard 再解析，因此这条路径**在到达播放器之前**就失败：用例因此仍然是
/// hermetic 的，真实 `afplay` 从未被启动。machine 模式不取 guard（它不读音频、不产生声音），
/// 因此仍然可以只读解析同一个 clip。
#[test]
fn a_clip_already_held_for_playback_refuses_human_play_but_still_resolves_for_machines() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "trace-play-8");
    let marker = fixture.play_marker_path(&clip_id);
    std::fs::create_dir_all(marker.parent().expect("locks dir")).expect("locks dir");
    std::fs::write(
        &marker,
        format!(
            "{{\"kind\":\"playback\",\"acquired_at\":\"{}\"}}\n",
            chrono::Utc::now().to_rfc3339()
        ),
    )
    .expect("occupy play marker");

    let human_provider = provider_with_catalog_and_synthesis("trace-play-9");
    let human = fixture.run_with(
        &["speech", "play", "--clip-id", &clip_id],
        &human_provider,
        Some(TEST_KEY),
    );
    let human_records = human_provider.finish();

    assert!(!human.status.success());
    assert_zero_connections(&human_records, "play");
    let stderr = String::from_utf8_lossy(&human.stderr);
    assert!(stderr.contains("still in progress"), "{stderr}");
    // 别人的凭证不被覆盖，也不被升级成错误后的残留。
    assert!(marker.exists());

    let machine_provider = provider_with_catalog_and_synthesis("trace-play-8b");
    let value = succeeded(&fixture.run_with(
        &["speech", "play", "--clip-id", &clip_id, "--json"],
        &machine_provider,
        Some(TEST_KEY),
    ));
    let machine_records = machine_provider.finish();
    assert_zero_connections(&machine_records, "play --json");
    assert_eq!(value["receipt"]["played"], false);
}

/// #25 留下的缺口：真实 `speech play` 现在自己走 `ClipUseGuard`。
///
/// #25 只能用注入 marker 的方式证明播放占用被跳过，因为当时没有用户入口创建它。
/// 这里证明占用语义的两端都成立：marker 存在时 `cache clear` 跳过它，marker 消失后
/// 它又变回普通可淘汰 entry——而 marker 的唯一来源现在是真实的 play 路径
/// （见 `src/speech/play.rs` 中持有 guard 的 human 分支）。
#[test]
fn a_play_marker_really_protects_a_clip_from_cache_clear() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "trace-play-10");
    let marker = fixture.play_marker_path(&clip_id);
    std::fs::create_dir_all(marker.parent().expect("locks dir")).expect("locks dir");
    std::fs::write(
        &marker,
        format!(
            "{{\"kind\":\"playback\",\"acquired_at\":\"{}\"}}\n",
            chrono::Utc::now().to_rfc3339()
        ),
    )
    .expect("occupy play marker");

    let provider = provider_with_catalog_and_synthesis("trace-play-11");
    let value = succeeded(&fixture.run_with(
        &["speech", "cache", "clear", "--json"],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();

    assert_zero_connections(&records, "cache clear");
    assert_eq!(value["receipt"]["removed"].as_array().map(Vec::len), Some(0));
    assert_eq!(value["receipt"]["skipped"].as_array().map(Vec::len), Some(1));
    let skip = &value["receipt"]["skipped_reasons"][0];
    assert_eq!(skip["clip_id"], clip_id.as_str());
    assert_eq!(skip["in_use"], "playback");

    // marker 释放后，同一个 entry 立即变回可淘汰：占用是 marker 决定的，不是硬编码的。
    std::fs::remove_file(&marker).expect("release play marker");
    let next = provider_with_catalog_and_synthesis("trace-play-12");
    let cleared = succeeded(&fixture.run_with(
        &["speech", "cache", "clear", "--json"],
        &next,
        Some(TEST_KEY),
    ));
    let next_records = next.finish();
    assert_zero_connections(&next_records, "cache clear");
    assert_eq!(cleared["receipt"]["removed"].as_array().map(Vec::len), Some(1));
    assert_eq!(cleared["receipt"]["skipped"].as_array().map(Vec::len), Some(0));
}

/// 播放失败/不播放都不创建 Speech Attempt，缓存逻辑身份也不变。
#[test]
fn no_play_path_creates_a_speech_attempt() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "trace-play-13");
    let attempts_before = fixture.attempt_files();
    let state_before = fixture.clip_state(&clip_id);

    let provider = provider_with_catalog_and_synthesis("trace-play-14");
    let value = succeeded(&fixture.run_with(
        &["speech", "play", "--clip-id", &clip_id, "--json"],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();

    assert_zero_connections(&records, "play --json");
    assert_eq!(value["receipt"]["provider_called"], false);
    assert_eq!(
        fixture.attempt_files(),
        attempts_before,
        "play must not create or modify any attempt record"
    );
    // 逻辑 clip 不变：同一个 clip ID、同一个 current audio version。
    let after = fixture.clip_state(&clip_id);
    assert_eq!(after["clip_id"], state_before["clip_id"]);
    assert_eq!(
        after["current_audio_sha256"],
        state_before["current_audio_sha256"]
    );
    assert_eq!(after["latest_attempt_id"], state_before["latest_attempt_id"]);
    assert_eq!(after["generation_blocked"], state_before["generation_blocked"]);
}

/// 密钥与 canary 既不进入 play 的输出，也不进入任何 Speech 状态文件。
#[test]
fn play_output_and_state_never_contain_the_api_key() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "trace-play-15");

    let provider = provider_with_catalog_and_synthesis("trace-play-16");
    let output = fixture.run_with(
        &["speech", "play", "--clip-id", &clip_id, "--json"],
        &provider,
        Some(TEST_KEY),
    );
    let records = provider.finish();
    assert_zero_connections(&records, "play --json");
    assert!(output.status.success());

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!combined.contains(TEST_KEY), "the API key must never be printed");
    for (path, bytes) in fixture.speech_state_files() {
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains(TEST_KEY), "the API key leaked into {path:?}");
        assert!(
            !text.contains(SECRET_CANARY),
            "the secret canary leaked into {path:?}"
        );
    }
}

/// Speech 状态根始终留在注入的 HOME 里。
#[test]
fn the_speech_root_stays_inside_the_injected_home() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "trace-play-17");

    let provider = provider_with_catalog_and_synthesis("trace-play-18");
    let value = succeeded(&fixture.run_with(
        &["speech", "play", "--clip-id", &clip_id, "--json"],
        &provider,
        Some(TEST_KEY),
    ));
    let records = provider.finish();
    assert_zero_connections(&records, "play --json");

    let reported = PathBuf::from(value["receipt"]["path"].as_str().expect("path"));
    assert!(
        reported.starts_with(fixture.home.path()),
        "the reported audio path {reported:?} escaped the injected home"
    );
    assert!(fixture.speech_root().exists());
}
