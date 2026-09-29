//! Issue #22 的 Voice Catalog CLI 合同测试。
//!
//! mock server 只记录请求形状和 Authorization 头是否存在。测试从不把测试密钥
//! 作为应用程序输出持久化或打印。

use chrono::{Duration, Utc};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration as StdDuration;
use tempfile::TempDir;

const TEST_KEY: &str = "issue-22-test-key";

#[derive(Debug, Clone)]
struct RequestRecord {
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
    fn one_response(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self::serve(Some((status, body.into())))
    }

    /// 计数 mock：CLI 结束后再停 listen。缺 key 必须零连接，不靠固定超时。
    fn expect_no_connections() -> Self {
        Self::serve(None)
    }

    fn serve(response: Option<(u16, Vec<u8>)>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock server");
        let address = listener.local_addr().expect("mock address");
        let records = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&records);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let join = thread::spawn(move || {
            if response.is_none() {
                listener
                    .set_nonblocking(true)
                    .expect("nonblocking accept");
            }
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let (headers, request_body) = read_request(&mut stream);
                        captured.lock().expect("records").push(RequestRecord {
                            authorization: header_value(&headers, "authorization"),
                            body: request_body,
                        });
                        if let Some((status, body)) = response.as_ref() {
                            let reason = if *status == 200 { "OK" } else { "Error" };
                            let header = format!(
                                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            stream
                                .write_all(header.as_bytes())
                                .expect("write response headers");
                            stream.write_all(body).expect("write response");
                            return;
                        }
                    }
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && response.is_none() =>
                    {
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
        self.join.take().expect("mock thread").join().expect("mock");
        self.records.lock().expect("records").clone()
    }
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

fn catalog_response() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "system_voice": [{
            "voice_id": "female_0033_b",
            "voice_name": "嗲嗲台妹",
            "description": ["开心"],
            "emotion": "开心",
            "created_time": "2026-09-11T00:00:00Z"
        }],
        "voice_cloning": [{
            "voice_id": "account-clone-exact",
            "voice_name": "我的声音",
            "description": ["账号提供"],
            "style": "讲解",
            "created_time": "2026-09-10T00:00:00Z"
        }],
        "voice_generation": [{
            "voice_id": "account-generated-exact",
            "voice_name": "我的声音",
            "description": ["实验"],
            "created_time": null
        }],
        "base_resp": {"status_code": 0, "status_msg": "success"}
    }))
    .expect("response JSON")
}

struct Fixture {
    home: TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().expect("fixture home"),
        }
    }

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

    fn run_with_server(&self, args: &[&str], server: &MockServer) -> Output {
        self.command()
            .args(args)
            .env("SENSEAUDIO_API_KEY", TEST_KEY)
            .env("SENSEAUDIO_API_BASE_URL", &server.url)
            .output()
            .expect("run CLI")
    }

    fn speech_root(&self) -> PathBuf {
        self.home
            .path()
            .join("Library/Application Support/books-exporter/speech")
    }

    fn catalog_path(&self) -> PathBuf {
        self.speech_root().join("voices/senseaudio.json")
    }

    fn seed_catalog(&self, fetched_at: String, voice_id: &str) {
        let root = self.speech_root().join("voices");
        std::fs::create_dir_all(&root).expect("catalog root");
        let catalog = json!({
            "schema_version": 1,
            "provider": "senseaudio",
            "fetched_at": fetched_at,
            "voices": [{
                "source_type": "system",
                "voice_id": voice_id,
                "voice_name": "Cached Voice",
                "emotion_label": null,
                "style_label": null,
                "description": ["缓存标签"],
                "created_time": null
            }]
        });
        std::fs::write(
            self.catalog_path(),
            serde_json::to_vec_pretty(&catalog).expect("catalog JSON"),
        )
        .expect("seed catalog");
    }
}

fn success_json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "expected success, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "success must keep stderr empty");
    serde_json::from_slice(&output.stdout).expect("stdout JSON")
}

fn failure_json(output: &Output) -> Value {
    assert!(!output.status.success(), "expected failure");
    assert!(output.stdout.is_empty(), "failure must keep stdout empty");
    serde_json::from_slice(&output.stderr).expect("stderr JSON")
}

#[test]
fn json_output_preserves_all_groups_metadata_and_exact_request_contract() {
    let fixture = Fixture::new();
    let server = MockServer::one_response(200, catalog_response());

    let value = success_json(&fixture.run_with_server(&["speech", "voices", "--json"], &server));
    let records = server.finish();

    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["receipt"]["operation"], "voices");
    assert_eq!(value["receipt"]["provider"], "senseaudio");
    assert_eq!(value["receipt"]["stale"], false);
    assert!(value["receipt"]["fetched_at"].is_string());
    assert_eq!(
        value["receipt"]["voices"].as_array().expect("voices").len(),
        3
    );
    assert_eq!(value["receipt"]["voices"][0]["source_type"], "system");
    assert_eq!(value["receipt"]["voices"][0]["voice_id"], "female_0033_b");
    assert_eq!(value["receipt"]["voices"][0]["emotion_label"], "开心");
    assert_eq!(value["receipt"]["voices"][0]["description"][0], "开心");
    assert_eq!(value["receipt"]["voices"][1]["source_type"], "cloned");
    assert_eq!(
        value["receipt"]["voices"][1]["voice_id"],
        "account-clone-exact"
    );
    assert_eq!(value["receipt"]["voices"][1]["style_label"], "讲解");
    assert_eq!(value["receipt"]["voices"][2]["source_type"], "generated");
    assert_eq!(
        value["receipt"]["voices"][2]["voice_id"],
        "account-generated-exact"
    );

    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].authorization.as_deref(),
        Some("Bearer issue-22-test-key")
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&records[0].body).expect("request JSON"),
        json!({"voice_type": "all"})
    );
    let cached = std::fs::read_to_string(fixture.catalog_path()).expect("cache");
    assert!(cached.contains("account-clone-exact"));
    assert!(!cached.contains(TEST_KEY));
}

#[test]
fn human_output_groups_by_voice_name_and_shows_concrete_labels() {
    let fixture = Fixture::new();
    let server = MockServer::one_response(200, catalog_response());

    let output = fixture.run_with_server(&["speech", "voices"], &server);
    let records = server.finish();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).expect("human output");
    assert!(text.contains("嗲嗲台妹"));
    assert!(text.contains("female_0033_b"));
    assert!(text.contains("Emotion: 开心"));
    assert!(text.contains("我的声音"));
    assert!(text.contains("account-clone-exact"));
    assert!(text.contains("account-generated-exact"));
    assert!(text.contains("Style: 讲解"));
    assert!(text.contains("Provider labels: 实验"));
    assert_eq!(records.len(), 1);
}

#[test]
fn fresh_cache_is_reused_without_credentials_or_network() {
    let fixture = Fixture::new();
    fixture.seed_catalog(Utc::now().to_rfc3339(), "cached-exact");

    let output = fixture
        .command()
        .args(["speech", "voices", "--json"])
        .output()
        .expect("run CLI");
    let value = success_json(&output);

    assert_eq!(value["receipt"]["stale"], false);
    assert_eq!(value["receipt"]["voices"][0]["voice_id"], "cached-exact");
    assert!(output.stderr.is_empty());
}

#[test]
fn explicit_refresh_bypasses_a_fresh_cache() {
    let fixture = Fixture::new();
    fixture.seed_catalog(Utc::now().to_rfc3339(), "cached-exact");
    let server = MockServer::one_response(200, catalog_response());

    let value = success_json(
        &fixture.run_with_server(&["speech", "voices", "--refresh", "--json"], &server),
    );
    let records = server.finish();

    assert_eq!(value["receipt"]["stale"], false);
    assert_eq!(value["receipt"]["voices"][0]["voice_id"], "female_0033_b");
    assert_eq!(records.len(), 1);
}

#[test]
fn a_corrupt_cache_is_repaired_by_an_explicit_refresh() {
    let fixture = Fixture::new();
    std::fs::create_dir_all(fixture.catalog_path().parent().expect("catalog directory"))
        .expect("catalog dir");
    std::fs::write(fixture.catalog_path(), "{ not a catalog").expect("corrupt cache");
    let server = MockServer::one_response(200, catalog_response());

    let value = success_json(
        &fixture.run_with_server(&["speech", "voices", "--refresh", "--json"], &server),
    );
    let records = server.finish();

    assert_eq!(value["receipt"]["stale"], false);
    assert_eq!(value["receipt"]["voices"][0]["voice_id"], "female_0033_b");
    assert_eq!(records.len(), 1, "显式刷新必须真的调用 provider");
    assert!(std::fs::read_to_string(fixture.catalog_path())
        .expect("cache")
        .contains("account-clone-exact"));
}

#[test]
fn a_corrupt_cache_with_a_failed_refresh_is_a_secret_safe_provider_error() {
    let fixture = Fixture::new();
    std::fs::create_dir_all(fixture.catalog_path().parent().expect("catalog directory"))
        .expect("catalog dir");
    std::fs::write(fixture.catalog_path(), "{ not a catalog").expect("corrupt cache");
    let server = MockServer::one_response(
        401,
        br#"{"code":"unauthorized","message":"Bearer issue-22-test-key"}"#.to_vec(),
    );

    let output = fixture.run_with_server(&["speech", "voices", "--refresh", "--json"], &server);
    let value = failure_json(&output);
    let records = server.finish();

    assert_eq!(value["error"]["code"], "SPEECH_AUTH_FAILED");
    assert!(!String::from_utf8_lossy(&output.stderr).contains(TEST_KEY));
    assert_eq!(records.len(), 1);
}

#[test]
fn refresh_auth_failure_returns_an_honest_timestamped_stale_fallback() {
    let fixture = Fixture::new();
    let fetched_at = (Utc::now() - Duration::hours(25)).to_rfc3339();
    fixture.seed_catalog(fetched_at.clone(), "stale-exact");
    let server = MockServer::one_response(
        401,
        br#"{"code":"unauthorized","message":"Bearer issue-22-test-key"}"#.to_vec(),
    );

    let value = success_json(&fixture.run_with_server(&["speech", "voices", "--json"], &server));
    let records = server.finish();

    assert_eq!(value["receipt"]["stale"], true);
    assert_eq!(value["receipt"]["fetched_at"], fetched_at);
    assert_eq!(value["receipt"]["voices"][0]["voice_id"], "stale-exact");
    assert_eq!(
        value["receipt"]["warnings"][0]["code"],
        "SPEECH_VOICE_CATALOG_STALE"
    );
    assert_eq!(value["receipt"]["warnings"][0]["reason"], "stale_catalog");
    let warning = value["receipt"]["warnings"][0]["message"]
        .as_str()
        .expect("warning");
    assert!(warning.contains("not current permission evidence"));
    assert!(!warning.contains(TEST_KEY));
    assert_eq!(records.len(), 1);
}

#[test]
fn authentication_failure_without_cache_is_secret_safe_machine_error() {
    let fixture = Fixture::new();
    let server = MockServer::one_response(
        401,
        br#"{"code":"unauthorized","message":"Bearer issue-22-test-key"}"#.to_vec(),
    );

    let output = fixture.run_with_server(&["speech", "voices", "--json"], &server);
    let value = failure_json(&output);
    let records = server.finish();

    assert_eq!(value["error"]["code"], "SPEECH_AUTH_FAILED");
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(!text.contains(TEST_KEY));
    assert_eq!(records.len(), 1);
}

#[test]
fn malformed_provider_response_is_a_secret_safe_provider_error() {
    let fixture = Fixture::new();
    let server = MockServer::one_response(
        200,
        br#"{"system_voice":[{"voice_id":"malformed"}]}"#.to_vec(),
    );

    let output = fixture.run_with_server(&["speech", "voices", "--json"], &server);
    let value = failure_json(&output);
    let records = server.finish();

    assert_eq!(value["error"]["code"], "SPEECH_PROVIDER_FAILED");
    assert!(output.stderr.len() < 1000);
    assert_eq!(records.len(), 1);
}

#[test]
fn missing_api_key_fails_before_network_with_secret_safe_error() {
    let fixture = Fixture::new();
    let server = MockServer::expect_no_connections();
    let output = fixture
        .command()
        .env("SENSEAUDIO_API_BASE_URL", &server.url)
        .args(["speech", "voices", "--json"])
        .output()
        .expect("run CLI");
    let value = failure_json(&output);
    let records = server.finish();

    assert_eq!(value["error"]["code"], "SPEECH_AUTH_FAILED");
    assert!(!String::from_utf8_lossy(&output.stderr).contains(TEST_KEY));
    assert_eq!(records.len(), 0, "缺 key 不得发起任何供应商连接");
}

#[test]
fn stale_human_fallback_displays_warning_and_exact_cached_id() {
    let fixture = Fixture::new();
    let fetched_at = (Utc::now() - Duration::hours(25)).to_rfc3339();
    fixture.seed_catalog(fetched_at.clone(), "stale-human");
    let server = MockServer::one_response(
        401,
        br#"{"code":"unauthorized","message":"not retained"}"#.to_vec(),
    );

    let output = fixture.run_with_server(&["speech", "voices"], &server);
    server.finish();

    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("human output");
    assert!(text.contains("[STALE]"));
    assert!(text.contains("stale-human"));
    assert!(text.contains("Warning [stale_catalog]"));
    assert!(text.contains(&fetched_at));
    assert!(text.contains("not current permission evidence"));
}

#[test]
fn empty_account_catalog_warns_and_human_output_says_no_voices() {
    let fixture = Fixture::new();
    let server = MockServer::one_response(
        200,
        br#"{"base_resp":{"status_code":0,"status_msg":"success"}}"#.to_vec(),
    );

    let json_output = fixture.run_with_server(&["speech", "voices", "--json"], &server);
    let value = success_json(&json_output);
    let records = server.finish();
    assert_eq!(records.len(), 1);
    assert_eq!(value["receipt"]["voices"].as_array().expect("voices").len(), 0);
    assert_eq!(
        value["receipt"]["warnings"][0]["code"],
        "SPEECH_VOICE_CATALOG_EMPTY"
    );
    assert_eq!(value["receipt"]["warnings"][0]["reason"], "empty_catalog");

    let human_fixture = Fixture::new();
    let human_server = MockServer::one_response(
        200,
        br#"{"base_resp":{"status_code":0,"status_msg":"success"}}"#.to_vec(),
    );
    let human = human_fixture.run_with_server(&["speech", "voices"], &human_server);
    human_server.finish();
    assert!(human.status.success());
    let text = String::from_utf8(human.stdout).expect("human output");
    assert!(text.contains("账号未返回音色"));
    assert!(text.contains("Warning [empty_catalog]"));
}

#[test]
fn catalog_cache_directory_occupied_by_a_file_uses_file_remediation() {
    let fixture = Fixture::new();
    let voices_dir = fixture.speech_root().join("voices");
    std::fs::create_dir_all(fixture.speech_root()).expect("speech root");
    std::fs::write(&voices_dir, "not a directory").expect("occupy catalog dir");
    let server = MockServer::one_response(200, catalog_response());

    let output = fixture.run_with_server(&["speech", "voices", "--json"], &server);
    let value = failure_json(&output);
    server.finish();

    assert_eq!(value["error"]["code"], "SPEECH_STORAGE_UNAVAILABLE");
    let remediation = value["error"]["remediation"].as_str().expect("remediation");
    assert!(
        remediation.contains("file, not a directory"),
        "catalog 路径是文件时应区分 remediation: {remediation}"
    );
    assert!(!remediation.contains("Verify that this directory exists"));
}

#[test]
fn cached_catalog_is_read_by_profile_validation_without_network() {
    let fixture = Fixture::new();
    fixture.seed_catalog(Utc::now().to_rfc3339(), "cached-exact");

    let output = fixture
        .command()
        .args([
            "speech",
            "profile",
            "set",
            "--voice-id",
            "cached-exact",
            "--json",
        ])
        .output()
        .expect("run CLI");
    let value = success_json(&output);

    assert_eq!(
        value["receipt"]["profile"]["verification_status"],
        "verified"
    );
    assert!(value["receipt"]["warnings"]
        .as_array()
        .expect("warnings")
        .is_empty());
}
