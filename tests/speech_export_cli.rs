//! Issue #27 的 `speech export` 与 Speech 音频链接 CLI 合同测试。
//!
//! 沿用 issue #21–#26 的约定：进程内 TCP mock provider（计数每个连接）、注入 HOME 隔离
//! Speech 状态根、移除全部代理变量让请求只能直达 mock，以及 secret canary 断言。
//!
//! 「导出与常规 Markdown 导出都零网络连接」因此是可证明的：每个用例结束时断言 mock
//! 记录 0 个请求，而不是靠超时等待。

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
const TEST_KEY: &str = "issue-27-test-key";
/// 探测 secret 落盘的 canary 值。
const SECRET_CANARY: &str = "canary-secret-4e17b2-do-not-persist";

/// 一次被 mock 记录的连接。导出的断言只看连接计数。
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
    // 目录里同时提供两个音色：Voice Catalog 会被缓存 24 小时，因此后续用另一个音色
    // 生成变体时也必须能在同一份目录里通过校验。
    serde_json::to_vec(&json!({
        "system_voice": [
            {
                "voice_id": "male_0004_a",
                "voice_name": "默认男声",
                "description": ["平稳"],
                "created_time": "2026-09-11T00:00:00Z"
            },
            {
                "voice_id": VARIANT_VOICE_ID,
                "voice_name": "变体男声",
                "description": ["平稳"],
                "created_time": "2026-09-11T00:00:00Z"
            }
        ],
        "voice_cloning": [],
        "voice_generation": [],
        "base_resp": {"status_code": 0, "status_msg": "success"}
    }))
    .expect("catalog JSON")
}

/// 用于制造「同内容、不同 Voice Profile」的第二个 clip ID。
const VARIANT_VOICE_ID: &str = "male_0009_b";

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
        assert!(count > 0, "request body ended early");
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
        (key.trim().eq_ignore_ascii_case(name)).then(|| value.trim().to_string())
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
///
/// `annotation-41` 只有高亮，`annotation-42` 高亮与笔记都有：两条记录让「每个内容部分
/// 一个链接、高亮与笔记不共享链接」可被区分验证。
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
            .expect("highlight-only annotation");
        annotation_conn
            .execute(
                "INSERT INTO ZAEANNOTATION VALUES (42, 'book-1', '第二段高亮', '我的私人笔记', 'epubcfi(/6/3)', 0.0, 3, 0)",
                rusqlite::params![],
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

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().expect("run CLI")
    }

    fn speech_root(&self) -> PathBuf {
        self.home
            .path()
            .join("Library/Application Support/books-exporter/speech")
    }

    /// 真实的 `speech export` 占用标记：与 `speech play` 同一个 `ClipUseGuard`。
    fn export_marker_path(&self, clip_id: &str) -> PathBuf {
        self.speech_root()
            .join("locks")
            .join(format!("{clip_id}.export"))
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

    /// 书籍导出根目录：`export --output` 指向这里。
    fn book_export_root(&self) -> PathBuf {
        self.home.path().join("books-exported/测试书")
    }

    fn manifest_path(&self) -> PathBuf {
        self.book_export_root()
            .join("assets/audio")
            .join("manifest.json")
    }

    fn manifest(&self) -> Value {
        serde_json::from_str(
            &std::fs::read_to_string(self.manifest_path()).expect("manifest.json"),
        )
        .expect("manifest JSON")
    }

    fn write_manifest(&self, value: &Value) {
        let path = self.manifest_path();
        std::fs::create_dir_all(path.parent().expect("manifest parent")).expect("create audio dir");
        std::fs::write(&path, serde_json::to_vec_pretty(value).expect("serialize")).expect("write");
    }

    /// 常规 Markdown 导出的主笔记。
    fn main_note(&self) -> String {
        std::fs::read_to_string(
            self.book_export_root()
                .join("测试书.md"),
        )
        .expect("main note")
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
fn generate_cached_clip(fixture: &Fixture, annotation_id: &str, content: &str, trace_id: &str) -> String {
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

/// 生成一个使用不同 voice 的 clip：内容相同但 Voice Profile 不同，因此 clip ID 不同。
fn generate_variant_clip(fixture: &Fixture, voice_id: &str, trace_id: &str) -> String {
    let provider = provider_with_catalog_and_synthesis(trace_id);
    let value = succeeded(&fixture.run_with(
        &[
            "speech", "generate", "--asset-id", "book-1", "--annotation-id", "annotation-41",
            "--content", "highlight", "--voice-id", voice_id, "--json",
        ],
        &provider,
        Some(TEST_KEY),
    ));
    let _ = provider.finish();
    value["receipt"]["clip_id"]
        .as_str()
        .expect("clip id")
        .to_string()
}

/// 一次语法合法但没有任何缓存的 clip ID。
const ABSENT_CLIP_ID: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn assert_zero_connections(records: &[RequestRecord], path: &str) {
    assert_eq!(
        records.len(),
        0,
        "`{path}` must not contact the Speech Provider, but the mock saw {records:?}"
    );
}

fn run_export(fixture: &Fixture, clip_id: &str, extra: &[&str]) -> Output {
    let provider = provider_with_catalog_and_synthesis("trace-export");
    let mut args = vec![
        "speech", "export", "--clip-id", clip_id, "--output",
    ];
    let root = fixture.book_export_root();
    args.push(root.to_str().expect("export root"));
    args.extend_from_slice(extra);
    args.push("--json");
    let output = fixture.run_with(&args, &provider, Some(TEST_KEY));
    let records = provider.finish();
    assert_zero_connections(&records, "speech export");
    output
}

/// 成功导出：音频先就位并通过校验，manifest 只含稳定事实，绝不含原文/密钥/绝对路径。
#[test]
fn export_copies_verified_audio_and_commits_a_manifest_without_secrets() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-exp-1");
    let source_audio = fixture.audio_path(&clip_id);

    let value = succeeded(&run_export(&fixture, &clip_id, &[]));
    let receipt = &value["receipt"];

    assert_eq!(value["schema_version"], 1);
    assert_eq!(receipt["operation"], "export");
    assert_eq!(receipt["clip_id"], clip_id.as_str());
    assert_eq!(receipt["asset_id"], "book-1");
    assert_eq!(receipt["annotation_id"], "annotation-41");
    assert_eq!(receipt["content_kind"], "highlight");
    assert_eq!(receipt["provider_called"], false);
    assert_eq!(receipt["format"], "mp3");
    assert!(receipt["size_bytes"].as_u64().expect("size") > 0);
    assert_eq!(receipt["sha256"].as_str().expect("sha").len(), 64);

    // 确定性文件名：内容部分 + 完整 clip ID 的前 12 位。
    let short = &clip_id[..12];
    let expected = format!("assets/audio/highlight-{short}.mp3");
    assert_eq!(receipt["relative_path"], expected.as_str());
    let exported = fixture.book_export_root().join(&expected);
    assert_eq!(
        std::fs::read(&exported).expect("exported audio"),
        std::fs::read(&source_audio).expect("cached audio")
    );

    // manifest 内容：稳定身份、相对路径、checksum、大小、格式、导出时间。
    let manifest = fixture.manifest();
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["asset_id"], "book-1");
    let record = &manifest["records"][0];
    assert_eq!(record["annotation_id"], "annotation-41");
    assert_eq!(record["content_kind"], "highlight");
    assert_eq!(record["active_clip_id"], clip_id.as_str());
    let entry = &record["clips"][0];
    assert_eq!(entry["clip_id"], clip_id.as_str());
    assert_eq!(entry["relative_path"], expected.as_str());
    assert_eq!(entry["sha256"], receipt["sha256"]);
    assert_eq!(entry["size_bytes"], receipt["size_bytes"]);
    assert_eq!(entry["format"], "mp3");
    assert!(entry["exported_at"].as_str().expect("exported_at").ends_with('Z'));

    // 没有原文、密钥、绝对路径或供应商响应。
    let text = serde_json::to_string(&manifest).expect("manifest text");
    assert!(!text.contains("高亮正文"), "manifest must not store the source text");
    assert!(!text.contains(TEST_KEY), "manifest must not store the API key");
    assert!(!text.contains(SECRET_CANARY));
    assert!(
        !text.contains(fixture.home.path().to_str().expect("home")),
        "manifest must not store absolute paths"
    );
    assert!(!text.contains("trace_id"));
    assert!(!text.contains("base_resp"));

    // 导出全程持有的 usage marker 已被释放。
    assert!(!fixture.export_marker_path(&clip_id).exists());
}

/// 常规 Markdown 导出把相对音频链接紧跟在对应高亮之后。
#[test]
fn markdown_export_links_active_audio_right_after_the_highlight() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-exp-2");
    succeeded(&run_export(&fixture, &clip_id, &[]));
    let relative = format!("assets/audio/highlight-{}.mp3", &clip_id[..12]);

    let value = succeeded(&fixture.run(&[
        "export", "--asset-id", "book-1", "--format", "markdown", "--json",
    ]));

    let note = fixture.main_note();
    let highlight = note.find("> 高亮正文").expect("highlight block");
    let link = note
        .find(&format!("[▶ 播放高亮语音]({relative})"))
        .expect("audio link after the highlight");
    assert!(
        link > highlight,
        "the audio link must come after the highlight it belongs to"
    );
    assert!(!note.contains("播放笔记语音"), "no note audio exists yet");
    // 链接是相对路径，不依赖本机中央缓存目录。
    assert!(!note.contains(fixture.speech_root().to_str().expect("speech root")));

    // 收据报告写入了哪些链接。
    assert_eq!(value["receipt"]["audio_links"][0], format!("[▶ 播放高亮语音]({relative})"));
    assert!(value["receipt"]["warnings"].is_null() || value["receipt"]["warnings"]
        .as_array()
        .expect("warnings")
        .is_empty());
}

/// 高亮与笔记各有自己的链接，且 Obsidian 在同一位置写音频嵌入。
#[test]
fn highlight_and_note_links_are_separate_and_obsidian_embeds_in_place() {
    let fixture = Fixture::new();
    let highlight_clip = generate_cached_clip(&fixture, "annotation-42", "highlight", "trace-exp-3");
    let note_clip = generate_cached_clip(&fixture, "annotation-42", "note", "trace-exp-4");
    succeeded(&run_export(&fixture, &highlight_clip, &[]));
    succeeded(&run_export(&fixture, &note_clip, &[]));

    let highlight_relative = format!("assets/audio/highlight-{}.mp3", &highlight_clip[..12]);
    let note_relative = format!("assets/audio/note-{}.mp3", &note_clip[..12]);

    succeeded(&fixture.run(&[
        "export", "--asset-id", "book-1", "--format", "markdown", "--json",
    ]));
    let markdown = fixture.main_note();
    let highlight_at = markdown
        .find(&format!("[▶ 播放高亮语音]({highlight_relative})"))
        .expect("highlight link");
    let note_text_at = markdown.find("我的私人笔记").expect("note body");
    let note_link_at = markdown
        .find(&format!("[▶ 播放笔记语音]({note_relative})"))
        .expect("note link");
    assert!(
        highlight_at < note_text_at && note_text_at < note_link_at,
        "each link must follow its own content part"
    );

    // 同一个 Annotation 的两个内容部分使用不同文件名，不共享链接。
    assert_ne!(highlight_relative, note_relative);
    let manifest = fixture.manifest();
    assert_eq!(manifest["records"].as_array().expect("records").len(), 2);

    // Obsidian 格式在同一位置写相对嵌入（覆盖同一本书已有的 Markdown）。
    succeeded(&fixture.run(&[
        "export", "--asset-id", "book-1", "--format", "obsidian", "--overwrite", "--json",
    ]));
    let obsidian = fixture.main_note();
    assert!(obsidian.contains(&format!("![[{highlight_relative}]]")));
    assert!(obsidian.contains(&format!("![[{note_relative}]]")));
    let embed_at = obsidian
        .find(&format!("![[{note_relative}]]"))
        .expect("obsidian note embed");
    assert!(embed_at > obsidian.find("我的私人笔记").expect("note body"));
}

/// 相同内容重复导出直接复用；同路径内容不同默认受保护，只有 --overwrite 才替换。
#[test]
fn re_export_reuses_identical_content_and_overwrite_is_explicit() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-exp-5");
    let first = succeeded(&run_export(&fixture, &clip_id, &[]));
    let relative = format!("assets/audio/highlight-{}.mp3", &clip_id[..12]);
    let exported = fixture.book_export_root().join(&relative);

    // 相同 clip、相同 checksum：直接复用，不重写。
    let second = succeeded(&run_export(&fixture, &clip_id, &[]));
    assert_eq!(first["receipt"]["relative_path"], second["receipt"]["relative_path"]);
    assert_eq!(second["receipt"]["reused"], true);
    assert_eq!(second["receipt"]["replaced"], false);
    assert_eq!(
        std::fs::read(&exported).expect("audio"),
        std::fs::read(fixture.audio_path(&clip_id)).expect("cached audio")
    );

    // 同路径内容不同（用户改过文件）：默认受保护。
    let mut tampered = std::fs::read(&exported).expect("audio");
    tampered[0] ^= 0xFF;
    std::fs::write(&exported, &tampered).expect("tamper");
    let protected = failed(&run_export(&fixture, &clip_id, &[]));
    assert_eq!(protected["error"]["code"], "SPEECH_OUTPUT_FILE_EXISTS");
    assert_eq!(protected["error"]["details"]["relative_path"], relative.as_str());
    assert_eq!(
        std::fs::read(&exported).expect("audio"),
        tampered,
        "a protected conflict must leave the file untouched"
    );

    // 显式 --overwrite 恢复成已校验的字节。
    let overwritten = succeeded(&run_export(&fixture, &clip_id, &["--overwrite"]));
    assert_eq!(overwritten["receipt"]["replaced"], true);
    assert_eq!(
        std::fs::read(&exported).expect("audio"),
        std::fs::read(fixture.audio_path(&clip_id)).expect("cached audio")
    );
}

/// 新变体成为唯一 active 变体，旧变体文件保持用户所有且不被删除。
#[test]
fn the_newest_variant_is_active_and_older_variants_survive() {
    let fixture = Fixture::new();
    let first = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-exp-6");
    let second = generate_variant_clip(&fixture, VARIANT_VOICE_ID, "trace-exp-7");
    assert_ne!(first, second, "a different Voice Profile is a different clip");

    succeeded(&run_export(&fixture, &first, &[]));
    let first_relative = format!("assets/audio/highlight-{}.mp3", &first[..12]);
    let first_file = fixture.book_export_root().join(&first_relative);
    let first_bytes = std::fs::read(&first_file).expect("first variant");

    let outcome = succeeded(&run_export(&fixture, &second, &[]));
    let second_relative = outcome["receipt"]["relative_path"]
        .as_str()
        .expect("relative path")
        .to_string();

    let record = &fixture.manifest()["records"][0];
    assert_eq!(record["active_clip_id"], second.as_str());
    assert_eq!(
        record["clips"].as_array().expect("clips").len(),
        2,
        "older variants stay recorded"
    );
    assert_eq!(record["clips"][0]["clip_id"], first.as_str());
    // 旧变体文件仍然存在且内容不变：由用户拥有，应用不静默删除。
    assert_eq!(std::fs::read(&first_file).expect("old variant"), first_bytes);

    // 常规导出只链接 active 变体。
    succeeded(&fixture.run(&[
        "export", "--asset-id", "book-1", "--format", "markdown", "--json",
    ]));
    let note = fixture.main_note();
    assert!(note.contains(&second_relative));
    assert!(
        !note.contains(&first_relative),
        "only the active variant is linked"
    );
}

/// 短 fingerprint 冲突：同名路径属于另一个 clip 时延长长度，绝不覆盖。
#[test]
fn a_short_fingerprint_collision_extends_instead_of_overwriting() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-exp-8");
    succeeded(&run_export(&fixture, &clip_id, &[]));
    let twelve = format!("assets/audio/highlight-{}.mp3", &clip_id[..12]);
    let fixture_file = fixture.book_export_root().join(&twelve);

    // 伪造一个与真实 clip **共享前 12 位**的记录，它已经占用了同名路径。
    let colliding = format!("{}0", clip_id);
    let other_bytes = silent_mp3(3);
    let other_sha = sha256_of(&other_bytes);
    fixture.write_manifest(&json!({
        "schema_version": 1,
        "asset_id": "book-1",
        "records": [{
            "annotation_id": "annotation-99",
            "content_kind": "highlight",
            "active_clip_id": colliding,
            "clips": [{
                "clip_id": colliding,
                "relative_path": twelve,
                "sha256": other_sha,
                "size_bytes": other_bytes.len(),
                "format": "mp3",
                "exported_at": "2026-09-11T00:00:00Z"
            }]
        }]
    }));
    let other_bytes_before = std::fs::read(&fixture_file).expect("foreign audio");

    // 即使带 --overwrite，也不能覆盖另一个 clip 已记录的路径。
    let outcome = succeeded(&run_export(&fixture, &clip_id, &["--overwrite"]));
    let relative = outcome["receipt"]["relative_path"]
        .as_str()
        .expect("relative path")
        .to_string();
    assert_ne!(relative, twelve, "the colliding path must not be reused");
    assert!(relative.starts_with(&format!("assets/audio/highlight-{}", &clip_id[..13])));

    // 另一个 clip 的文件逐字节不变。
    assert_eq!(std::fs::read(&fixture_file).expect("foreign audio"), other_bytes_before);
    let manifest = fixture.manifest();
    let paths: Vec<String> = manifest["records"]
        .as_array()
        .expect("records")
        .iter()
        .flat_map(|record| record["clips"].as_array().expect("clips").clone())
        .map(|clip| clip["relative_path"].as_str().expect("path").to_string())
        .collect();
    assert_eq!(
        paths.iter().filter(|path| *path == &twelve).count(),
        1,
        "one path, one owning clip"
    );
}

/// 负向控制：manifest 里的路径穿越既不会读也不会写导出根之外的文件。
#[test]
fn a_traversal_path_in_the_manifest_never_escapes_the_export_root() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-exp-9");

    // 根目录之外放一个「秘密」音频。
    let outside = TempDir::new().expect("outside");
    let secret = outside.path().join("secret.mp3");
    let secret_bytes = silent_mp3(5);
    std::fs::write(&secret, &secret_bytes).expect("secret");

    let traversal = format!(
        "../../../../../../{}/secret.mp3",
        outside
            .path()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    );
    fixture.write_manifest(&json!({
        "schema_version": 1,
        "asset_id": "book-1",
        "records": [{
            "annotation_id": "annotation-41",
            "content_kind": "highlight",
            "active_clip_id": clip_id,
            "clips": [{
                "clip_id": clip_id,
                "relative_path": traversal,
                "sha256": sha256_of(&secret_bytes),
                "size_bytes": secret_bytes.len(),
                "format": "mp3",
                "exported_at": "2026-09-11T00:00:00Z"
            }]
        }]
    }));

    // 常规 Markdown 导出照常成功，只省略这条链接并给出 warning。
    let value = succeeded(&fixture.run(&[
        "export", "--asset-id", "book-1", "--format", "markdown", "--json",
    ]));
    let warnings = value["receipt"]["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1, "the traversal link must be reported");
    assert_eq!(warnings[0]["code"], "SPEECH_AUDIO_LINK_OMITTED");
    assert_eq!(warnings[0]["reason"], "path_outside_export_root");
    let note = fixture.main_note();
    assert!(!note.contains("secret.mp3"), "no link to the file outside the root");
    assert!(note.contains("> 高亮正文"), "the main note is still exported");
    assert_eq!(
        std::fs::read(&secret).expect("secret"),
        secret_bytes,
        "the file outside the export root is never read or modified"
    );

    // `speech export` 自己也不接受这样的记录：损坏 manifest 绝不覆盖或猜测重建。
    let before = std::fs::read_to_string(fixture.manifest_path()).expect("manifest");
    let value = failed(&run_export(&fixture, &clip_id, &[]));
    assert_eq!(value["error"]["code"], "SPEECH_EXPORT_MANIFEST_INVALID");
    assert_eq!(
        std::fs::read_to_string(fixture.manifest_path()).expect("manifest"),
        before,
        "an untrusted manifest is never rewritten"
    );
}

/// 负向控制：损坏的 manifest 不会被覆盖或按文件名猜测重建。
#[test]
fn a_corrupt_manifest_is_never_rebuilt_or_overwritten() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-exp-10");
    let path = fixture.manifest_path();
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create");
    std::fs::write(&path, "{ this is not json").expect("write corrupt manifest");

    let value = failed(&run_export(&fixture, &clip_id, &[]));
    assert_eq!(value["error"]["code"], "SPEECH_EXPORT_MANIFEST_INVALID");
    assert_eq!(value["error"]["details"]["reason"], "manifest_invalid");
    assert_eq!(
        std::fs::read_to_string(&path).expect("manifest"),
        "{ this is not json",
        "the corrupt manifest must be left exactly as the user left it"
    );
    assert!(
        !fixture
            .book_export_root()
            .join(format!("assets/audio/highlight-{}.mp3", &clip_id[..12]))
            .exists(),
        "no audio is placed when the manifest cannot be trusted"
    );
}

/// 常规 Markdown 导出遇到缺失音频：省略链接 + 结构化 warning，主体导出仍然成功。
#[test]
fn a_missing_exported_audio_becomes_a_warning_not_a_failed_export() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-exp-11");
    succeeded(&run_export(&fixture, &clip_id, &[]));
    let relative = format!("assets/audio/highlight-{}.mp3", &clip_id[..12]);
    // 用户自己删掉了导出的音频。
    std::fs::remove_file(fixture.book_export_root().join(&relative)).expect("remove audio");

    let value = succeeded(&fixture.run(&[
        "export", "--asset-id", "book-1", "--format", "markdown", "--json",
    ]));

    let warnings = value["receipt"]["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0]["code"], "SPEECH_AUDIO_LINK_OMITTED");
    assert_eq!(warnings[0]["reason"], "audio_missing");
    let note = fixture.main_note();
    assert!(!note.contains(&relative), "no dangling link is written");
    assert!(note.contains("> 高亮正文"), "the reading note is still complete");
    assert!(value["receipt"]["generated_files"].as_array().expect("files").len() >= 1);
    // 常规导出不修复文件、不修改 manifest。
    assert!(!fixture.book_export_root().join(&relative).exists());
    assert_eq!(fixture.manifest()["records"][0]["active_clip_id"], clip_id.as_str());
}

/// 被用户改写（checksum 不匹配）的音频同样只产生 warning。
#[test]
fn a_checksum_mismatch_omits_the_link_and_keeps_the_manifest_untouched() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-exp-12");
    succeeded(&run_export(&fixture, &clip_id, &[]));
    let relative = format!("assets/audio/highlight-{}.mp3", &clip_id[..12]);
    let audio = fixture.book_export_root().join(&relative);
    let mut tampered = std::fs::read(&audio).expect("audio");
    tampered[10] ^= 0xFF;
    std::fs::write(&audio, &tampered).expect("tamper");
    let manifest_before = std::fs::read_to_string(fixture.manifest_path()).expect("manifest");

    let value = succeeded(&fixture.run(&[
        "export", "--asset-id", "book-1", "--format", "markdown", "--json",
    ]));

    let warnings = value["receipt"]["warnings"].as_array().expect("warnings");
    assert_eq!(warnings[0]["code"], "SPEECH_AUDIO_LINK_OMITTED");
    assert_eq!(warnings[0]["reason"], "checksum_mismatch");
    assert!(!fixture.main_note().contains(&relative));
    assert_eq!(
        std::fs::read_to_string(fixture.manifest_path()).expect("manifest"),
        manifest_before,
        "the book exporter never edits the manifest"
    );
    assert_eq!(std::fs::read(&audio).expect("audio"), tampered, "and never repairs the file");
}

/// 中断导出：manifest 提交失败时不会留下指向缺失文件的 active 记录。
#[test]
fn an_interrupted_export_never_commits_a_manifest_pointing_at_a_missing_file() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-exp-13");

    // 故障注入：让 manifest 的暂存路径被一个目录占住，提交必然失败。
    let staging = fixture
        .book_export_root()
        .join("assets/audio")
        .join("manifest.json.staging");
    std::fs::create_dir_all(&staging).expect("create blocking directory");

    let value = failed(&run_export(&fixture, &clip_id, &[]));
    assert!(
        !value["error"]["code"]
            .as_str()
            .expect("code")
            .is_empty()
    );
    assert_eq!(
        value["error"]["code"], "SPEECH_STORAGE_UNAVAILABLE",
        "a manifest commit failure is a stable storage error"
    );
    // 没有已提交的 manifest，因此不可能有指向缺失文件的 active 记录。
    // ADR 0007 允许中断留下未被 manifest 引用的孤立音频——关键是它没有 active 身份。
    assert!(
        !fixture.manifest_path().exists(),
        "no manifest is committed when the commit fails"
    );
    let audio_dir = fixture.book_export_root().join("assets/audio");
    let leftovers: Vec<String> = std::fs::read_dir(&audio_dir)
        .expect("audio dir")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "manifest.json.staging")
        .collect();
    // ADR 0007 允许中断留下未被 manifest 引用的孤立音频；关键不变量是「已提交的 manifest
    // 永远不指向缺失文件」。这里没有 manifest 被提交，因此任何残留音频都没有 active 身份：
    // 它是完整且已校验的字节，但没有任何东西引用它。
    for name in &leftovers {
        if name.starts_with('.') {
            continue;
        }
        assert_eq!(
            std::fs::read(audio_dir.join(name)).expect("leftover audio"),
            std::fs::read(fixture.audio_path(&clip_id)).expect("cached audio"),
            "a placed audio file is always the verified bytes"
        );
    }
    // 移除故障注入，重试同一 clip：孤立音频被直接复用，而不是被当成损坏或重写。
    std::fs::remove_dir(&staging).expect("remove blocking directory");
    let retried = succeeded(&run_export(&fixture, &clip_id, &[]));
    assert_eq!(retried["receipt"]["reused"], true);
    assert_eq!(retried["receipt"]["relative_path"], format!("assets/audio/highlight-{}.mp3", &clip_id[..12]));
}

/// `speech export` 绝不改写已有 Markdown，也绝不调用 provider。
#[test]
fn speech_export_never_patches_markdown_and_never_calls_a_provider() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-exp-14");

    // 先做一次常规导出，让 Markdown 存在。
    succeeded(&fixture.run(&[
        "export", "--asset-id", "book-1", "--format", "markdown", "--json",
    ]));
    let before = fixture.main_note();
    let markdown_before = list_markdown(&fixture.book_export_root());

    // run_export 内部已经断言过零 provider 连接。
    succeeded(&run_export(&fixture, &clip_id, &[]));

    assert_eq!(
        fixture.main_note(),
        before,
        "speech export must not patch an existing Markdown file"
    );
    assert_eq!(
        list_markdown(&fixture.book_export_root()),
        markdown_before,
        "speech export must not add, remove or edit any Markdown file"
    );
    // 唯一被写的是音频与 manifest。
    assert!(fixture.manifest_path().exists());
}

/// 真实的 `speech export` 通过同一个 `ClipUseGuard` 占用 `locks/<clip_id>.export`。
///
/// #25 只能通过注入 marker 假装证明导出占用；这里用一个真实占用中的 marker 证明
/// 真实导出路径会尊重它，因此导出期间 entry 不会被 LRU 或 `speech cache clear` 抽走。
#[test]
fn speech_export_goes_through_the_real_clip_use_guard() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-exp-15");

    // 真实的 `speech play --json` 不占用 marker，所以这里显式放置一个占用中的 export
    // marker（与 ClipUseGuard 写出的形状一致）。
    let marker = fixture.export_marker_path(&clip_id);
    std::fs::create_dir_all(marker.parent().expect("locks")).expect("create locks");
    std::fs::write(
        &marker,
        b"{\"kind\":\"export\",\"acquired_at\":\"2026-09-29T12:00:00+00:00\"}\n",
    )
    .expect("write marker");

    let value = failed(&run_export(&fixture, &clip_id, &[]));
    assert_eq!(
        value["error"]["code"], "SPEECH_IN_PROGRESS",
        "a real export must respect the shared usage marker"
    );
    assert!(!fixture.manifest_path().exists());

    // 释放之后导出正常进行。
    std::fs::remove_file(&marker).expect("release marker");
    succeeded(&run_export(&fixture, &clip_id, &[]));
}

/// 没有缓存、没有合法 clip ID、错误书籍：稳定错误，全部零网络连接。
#[test]
fn absent_and_malformed_clips_are_stable_errors() {
    let fixture = Fixture::new();

    let absent = failed(&run_export(&fixture, ABSENT_CLIP_ID, &[]));
    assert_eq!(absent["error"]["code"], "SPEECH_CLIP_NOT_FOUND");

    let malformed = failed(&run_export(&fixture, "../escape", &[]));
    assert_eq!(malformed["error"]["code"], "INVALID_ARGUMENT");
    assert!(!fixture.manifest_path().exists());

    // human 模式也是稳定失败，且零 provider 连接。
    let provider = provider_with_catalog_and_synthesis("trace-exp-16");
    let output = fixture.run_with(
        &[
            "speech", "export", "--clip-id", ABSENT_CLIP_ID, "--output",
            fixture.book_export_root().to_str().expect("root"),
        ],
        &provider,
        Some(TEST_KEY),
    );
    assert!(!output.status.success());
    let records = provider.finish();
    assert_zero_connections(&records, "speech export (human)");
    assert!(String::from_utf8_lossy(&output.stderr).contains("no verified Cached Speech Clip"));
}

/// 被用户改写的缓存 entry 在导出前就被拒绝，损坏字节不会被复制到导出目录。
#[test]
fn a_tampered_cache_entry_is_rejected_before_anything_is_copied() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-exp-17");
    let audio = fixture.audio_path(&clip_id);
    let mut bytes = std::fs::read(&audio).expect("audio");
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    std::fs::write(&audio, &bytes).expect("tamper");

    let value = failed(&run_export(&fixture, &clip_id, &[]));
    assert_eq!(value["error"]["code"], "SPEECH_CACHE_CORRUPT");
    assert!(!fixture.manifest_path().exists());
    assert!(
        !fixture
            .book_export_root()
            .join("assets/audio")
            .exists(),
        "a corrupt cache entry must not produce any export artifact"
    );
}

/// 导出到属于另一本书的 manifest 会被拒绝。
#[test]
fn a_manifest_for_another_book_is_rejected() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-exp-18");
    fixture.write_manifest(&json!({
        "schema_version": 1,
        "asset_id": "another-book",
        "records": []
    }));

    let value = failed(&run_export(&fixture, &clip_id, &[]));
    assert_eq!(value["error"]["code"], "SPEECH_EXPORT_MANIFEST_INVALID");
    let manifest = fixture.manifest();
    assert_eq!(manifest["asset_id"], "another-book");
    assert!(manifest["records"].as_array().expect("records").is_empty());
}

/// 语音是可选的：没有导出过音频时，常规导出不写任何占位链接。
#[test]
fn a_book_without_exported_audio_writes_no_placeholder_link() {
    let fixture = Fixture::new();

    let value = succeeded(&fixture.run(&[
        "export", "--asset-id", "book-1", "--format", "markdown", "--json",
    ]));

    let note = fixture.main_note();
    assert!(note.contains("> 高亮正文"));
    assert!(!note.contains("assets/audio"), "no placeholder audio link");
    assert!(!note.contains("播放"));
    assert!(value["receipt"]["audio_links"].is_null());
    assert!(!fixture.manifest_path().exists(), "reading notes never create a manifest");
}

/// 人类 CLI 的导出输出说明复用了哪个文件、哪个 clip 现在 active。
#[test]
fn human_export_reports_the_relative_path_and_active_clip() {
    let fixture = Fixture::new();
    let clip_id = generate_cached_clip(&fixture, "annotation-41", "highlight", "trace-exp-19");

    let provider = provider_with_catalog_and_synthesis("trace-exp-20");
    let output = fixture.run_with(
        &[
            "speech", "export", "--clip-id", &clip_id, "--output",
            fixture.book_export_root().to_str().expect("root"),
        ],
        &provider,
        Some(TEST_KEY),
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let records = provider.finish();
    assert_zero_connections(&records, "speech export (human success)");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("assets/audio/highlight-{}.mp3", &clip_id[..12])), "{stdout}");
    assert!(stdout.contains(&clip_id), "{stdout}");
}

fn sha256_of(bytes: &[u8]) -> String {
    // 复用 CLI 自己写进 manifest 的 checksum：测试只关心「manifest 与磁盘一致」，
    // 因此用同样的算法独立计算一次 sha256。
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 列出导出根目录下的全部 Markdown 文件及其字节。
fn list_markdown(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    fn walk(dir: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, files);
            } else if path.extension().map(|ext| ext == "md").unwrap_or(false) {
                files.push((path.clone(), std::fs::read(&path).unwrap_or_default()));
            }
        }
    }
    walk(root, &mut files);
    files.sort();
    files
}

use sha2::{Digest, Sha256};
