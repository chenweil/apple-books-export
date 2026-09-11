//! Voice Profile CLI 合同测试（GitHub issue #21）。
//!
//! 这些测试通过 HOME 注入隔离的 Speech 状态根，并证明 profile 操作：
//! - 不读写真实用户目录；
//! - 不访问网络，也不依赖 Apple Books 数据库；
//! - 不把 API Key 或任何秘密写入磁盘。

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

/// 用于探测 secret 落盘的 canary 值，不会出现在任何期望输出中。
const SECRET_CANARY: &str = "canary-secret-1f4b2c-do-not-persist";

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
            .env_remove("HTTP_PROXY")
            .env_remove("http_proxy")
            // 这些代理指向本地死端口：任何真实网络请求都会立即失败。
            .env("HTTPS_PROXY", DEAD_PROXY)
            .env("https_proxy", DEAD_PROXY)
            .env("ALL_PROXY", DEAD_PROXY)
            .env("all_proxy", DEAD_PROXY)
            .env_remove("NO_PROXY")
            .env_remove("no_proxy");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().expect("run CLI")
    }

    fn home(&self) -> &Path {
        self.home.path()
    }

    fn speech_root(&self) -> PathBuf {
        self.home()
            .join("Library/Application Support/books-exporter/speech")
    }

    fn config_path(&self) -> PathBuf {
        self.speech_root().join("config.json")
    }

    fn config(&self) -> Value {
        let text = fs::read_to_string(self.config_path()).expect("config.json exists");
        serde_json::from_str(&text).expect("config.json is JSON")
    }

    /// 递归读取 speech 根目录下所有文件的字节内容。
    fn speech_root_files(&self) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(dir: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
            let entries = match fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(_) => return,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, files);
                } else {
                    let bytes = fs::read(&path).expect("read speech state file");
                    files.push((path, bytes));
                }
            }
        }
        let mut files = Vec::new();
        walk(&self.speech_root(), &mut files);
        files
    }
}

/// 本地死端口代理，保证被误触发的网络调用快速失败而不是穿透出去。
const DEAD_PROXY: &str = "http://127.0.0.1:9";

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

#[test]
fn show_json_reports_adr_default_profile_as_unverified() {
    let fixture = Fixture::new();

    let value = succeeded(&fixture.run(&["speech", "profile", "show", "--json"]));

    assert_eq!(value["schema_version"], 1);
    let receipt = &value["receipt"];
    assert_eq!(receipt["operation"], "profile_show");
    assert_eq!(receipt["profile"]["provider"], "senseaudio");
    assert_eq!(receipt["profile"]["model"], "sensenova-tts-2.0");
    assert_eq!(receipt["profile"]["voice_id"], "male_0004_a");
    assert_eq!(receipt["profile"]["emotion_label"], Value::Null);
    assert_eq!(receipt["profile"]["style_label"], Value::Null);
    assert_eq!(receipt["profile"]["speed"], 1.0);
    assert_eq!(receipt["profile"]["volume"], 1.0);
    assert_eq!(receipt["profile"]["pitch"], 0);
    assert_eq!(receipt["profile"]["verification_status"], "unverified");
    assert_eq!(receipt["profile"]["verified_at"], Value::Null);
    assert_eq!(receipt["profile"]["audio"]["format"], "mp3");
    assert_eq!(receipt["profile"]["audio"]["sample_rate"], 32000);
    assert_eq!(receipt["profile"]["audio"]["bitrate"], 128000);
    assert_eq!(receipt["profile"]["audio"]["channel"], 2);
    assert_eq!(receipt["api_key_env"], "SENSEAUDIO_API_KEY");
    assert_eq!(
        receipt["config_path"],
        fixture.config_path().to_string_lossy().as_ref()
    );
    assert_eq!(receipt["warnings"][0]["code"], "SPEECH_VOICE_UNVERIFIED");
    assert_eq!(receipt["warnings"][0]["reason"], "stored_unverified");
    assert!(
        !fixture.config_path().exists(),
        "show must not create a config file"
    );
}

#[test]
fn set_json_persists_exact_hundredths_and_reports_unverified() {
    let fixture = Fixture::new();

    let value = succeeded(&fixture.run(&[
        "speech",
        "profile",
        "set",
        "--voice-id",
        "female_0007_b",
        "--speed",
        "1.25",
        "--volume",
        "0.29",
        "--pitch",
        "-3",
        "--emotion-label",
        "平稳",
        "--style-label",
        "新闻",
        "--json",
    ]));

    let receipt = &value["receipt"];
    assert_eq!(receipt["operation"], "profile_set");
    assert_eq!(receipt["profile"]["voice_id"], "female_0007_b");
    assert_eq!(receipt["profile"]["speed"], 1.25);
    assert_eq!(receipt["profile"]["volume"], 0.29);
    assert_eq!(receipt["profile"]["pitch"], -3);
    assert_eq!(receipt["profile"]["emotion_label"], "平稳");
    assert_eq!(receipt["profile"]["style_label"], "新闻");
    assert_eq!(receipt["profile"]["verification_status"], "unverified");
    assert_eq!(receipt["profile"]["verified_at"], Value::Null);
    assert_eq!(receipt["warnings"][0]["reason"], "no_catalog");

    let config = fixture.config();
    let stored = &config["voice_profile"];
    assert_eq!(stored["voice_id"], "female_0007_b");
    assert_eq!(stored["speed_x100"], 125);
    assert_eq!(stored["volume_x100"], 29);
    assert_eq!(stored["pitch"], -3);
    assert_eq!(stored["verification_status"], "unverified");
    assert_eq!(stored["audio"]["format"], "mp3");
    assert_eq!(stored["audio"]["sample_rate"], 32000);
    assert_eq!(stored["audio"]["bitrate"], 128000);
    assert_eq!(stored["audio"]["channel"], 2);
    assert_eq!(config["schema_version"], 1);
    assert_eq!(config["api_key_env"], "SENSEAUDIO_API_KEY");

    // 整数百分位必须精确往返，不能因为浮点格式化而漂移。
    let reloaded = succeeded(&fixture.run(&["speech", "profile", "show", "--json"]));
    assert_eq!(reloaded["receipt"]["profile"]["speed"], 1.25);
    assert_eq!(
        reloaded["receipt"]["profile"]["volume"], 0.29,
        "0.29 must round-trip through CLI text -> hundredths -> JSON without drift"
    );
}

#[test]
fn set_rejects_values_that_cannot_be_expressed_in_hundredths() {
    let fixture = Fixture::new();

    let rounded = failed(&fixture.run(&[
        "speech",
        "profile",
        "set",
        "--voice-id",
        "male_0004_a",
        "--speed",
        "1.005",
        "--json",
    ]));
    assert_eq!(rounded["error"]["code"], "SPEECH_PROFILE_INVALID");
    assert_eq!(rounded["error"]["details"]["field"], "speed");
    assert_eq!(rounded["error"]["details"]["reason"], "not_hundredth");
    assert_eq!(rounded["error"]["details"]["value"], "1.005");

    let trailing_zero = failed(&fixture.run(&[
        "speech",
        "profile",
        "set",
        "--voice-id",
        "male_0004_a",
        "--volume",
        "0.290",
        "--json",
    ]));
    assert_eq!(trailing_zero["error"]["code"], "SPEECH_PROFILE_INVALID");
    assert_eq!(trailing_zero["error"]["details"]["reason"], "not_hundredth");

    assert!(
        !fixture.config_path().exists(),
        "a rejected set must not write a profile"
    );
}

#[test]
fn set_rejects_non_finite_scientific_and_malformed_numbers() {
    let fixture = Fixture::new();

    let cases = [
        ("speed", "NaN", "not_finite"),
        ("speed", "inf", "not_finite"),
        ("speed", "-inf", "not_finite"),
        ("volume", "Infinity", "not_finite"),
        ("speed", "-nan", "not_finite"),
        ("speed", "+INF", "not_finite"),
        ("speed", "1e1", "not_a_number"),
        ("speed", "1E0", "not_a_number"),
        ("speed", "0x10", "not_a_number"),
        ("volume", "1,5", "not_a_number"),
        ("speed", "", "missing"),
        ("pitch", "1.5", "not_an_integer"),
        ("pitch", "NaN", "not_finite"),
    ];

    for (flag, value, reason) in cases {
        let output = fixture.run(&[
            "speech",
            "profile",
            "set",
            "--voice-id",
            "male_0004_a",
            &format!("--{flag}"),
            value,
            "--json",
        ]);
        let error = failed(&output);
        assert_eq!(
            error["error"]["code"], "SPEECH_PROFILE_INVALID",
            "flag {flag}={value} should be rejected with a stable code"
        );
        assert_eq!(
            error["error"]["details"]["field"], flag,
            "flag {flag}={value} must report the offending field"
        );
        assert_eq!(
            error["error"]["details"]["reason"], reason,
            "flag {flag}={value} must report the exact reason"
        );
    }

    assert!(
        !fixture.config_path().exists(),
        "rejected values must not write a profile"
    );
}

#[test]
fn set_rejects_out_of_range_boundaries_and_accepts_the_limits() {
    let fixture = Fixture::new();

    for (flag, value) in [
        ("speed", "0.49"),
        ("speed", "2.01"),
        ("volume", "0"),
        ("volume", "10.01"),
        ("pitch", "-13"),
        ("pitch", "13"),
    ] {
        let error = failed(&fixture.run(&[
            "speech",
            "profile",
            "set",
            "--voice-id",
            "male_0004_a",
            &format!("--{flag}"),
            value,
            "--json",
        ]));
        assert_eq!(error["error"]["code"], "SPEECH_PROFILE_INVALID");
        assert_eq!(error["error"]["details"]["field"], flag);
        assert_eq!(error["error"]["details"]["reason"], "out_of_range");
    }

    // 0.29 是合法的百分位数值，但不在 speed 的 [0.5, 2.0] 内：越界必须报 out_of_range。
    let speed_too_slow = failed(&fixture.run(&[
        "speech",
        "profile",
        "set",
        "--voice-id",
        "male_0004_a",
        "--speed",
        "0.29",
        "--json",
    ]));
    assert_eq!(speed_too_slow["error"]["code"], "SPEECH_PROFILE_INVALID");
    assert_eq!(speed_too_slow["error"]["details"]["field"], "speed");
    assert_eq!(speed_too_slow["error"]["details"]["reason"], "out_of_range");

    for (flag, value, expected) in [
        ("speed", "0.5", 0.5),
        ("speed", "2.0", 2.0),
        ("volume", "0.01", 0.01),
        ("volume", "10.0", 10.0),
    ] {
        let value_json = succeeded(&fixture.run(&[
            "speech",
            "profile",
            "set",
            "--voice-id",
            "male_0004_a",
            &format!("--{flag}"),
            value,
            "--json",
        ]));
        assert_eq!(value_json["receipt"]["profile"][flag], expected);
    }

    for pitch in ["-12", "0", "12"] {
        let value_json = succeeded(&fixture.run(&[
            "speech",
            "profile",
            "set",
            "--voice-id",
            "male_0004_a",
            "--pitch",
            pitch,
            "--json",
        ]));
        assert_eq!(value_json["receipt"]["profile"]["pitch"], pitch.parse::<i32>().unwrap());
    }
}

#[test]
fn set_requires_an_exact_voice_id_and_known_provider() {
    let fixture = Fixture::new();

    let missing = failed(&fixture.run(&["speech", "profile", "set", "--json"]));
    assert_eq!(missing["error"]["code"], "SPEECH_PROFILE_INVALID");
    assert_eq!(missing["error"]["details"]["field"], "voice_id");
    assert_eq!(missing["error"]["details"]["reason"], "missing");

    let padded = failed(&fixture.run(&[
        "speech",
        "profile",
        "set",
        "--voice-id",
        " male_0004_a",
        "--json",
    ]));
    assert_eq!(padded["error"]["code"], "SPEECH_PROFILE_INVALID");
    assert_eq!(padded["error"]["details"]["field"], "voice_id");
    assert_eq!(padded["error"]["details"]["reason"], "invalid_format");

    let provider = failed(&fixture.run(&[
        "speech",
        "profile",
        "set",
        "--voice-id",
        "male_0004_a",
        "--provider",
        "openai",
        "--json",
    ]));
    assert_eq!(provider["error"]["code"], "SPEECH_PROFILE_INVALID");
    assert_eq!(provider["error"]["details"]["field"], "provider");
    assert_eq!(provider["error"]["details"]["reason"], "unsupported_provider");

    let api_key_env = failed(&fixture.run(&[
        "speech",
        "profile",
        "set",
        "--voice-id",
        "male_0004_a",
        "--api-key-env",
        "NOT A NAME",
        "--json",
    ]));
    assert_eq!(api_key_env["error"]["code"], "SPEECH_PROFILE_INVALID");
    assert_eq!(api_key_env["error"]["details"]["field"], "api_key_env");
    assert_eq!(api_key_env["error"]["details"]["reason"], "invalid_format");

    assert!(!fixture.config_path().exists());
}

#[test]
fn reset_restores_the_default_profile_and_keeps_api_key_env_name() {
    let fixture = Fixture::new();

    succeeded(&fixture.run(&[
        "speech",
        "profile",
        "set",
        "--voice-id",
        "female_0007_b",
        "--speed",
        "1.75",
        "--api-key-env",
        "CUSTOM_SENSEAUDIO_KEY",
        "--json",
    ]));
    assert_eq!(
        fixture.config()["api_key_env"],
        "CUSTOM_SENSEAUDIO_KEY",
        "set must persist the environment variable name"
    );

    let value = succeeded(&fixture.run(&["speech", "profile", "reset", "--json"]));
    let receipt = &value["receipt"];
    assert_eq!(receipt["operation"], "profile_reset");
    assert_eq!(receipt["profile"]["voice_id"], "male_0004_a");
    assert_eq!(receipt["profile"]["speed"], 1.0);
    assert_eq!(receipt["profile"]["volume"], 1.0);
    assert_eq!(receipt["profile"]["pitch"], 0);
    assert_eq!(receipt["profile"]["verification_status"], "unverified");
    assert_eq!(receipt["api_key_env"], "CUSTOM_SENSEAUDIO_KEY");

    let config = fixture.config();
    assert_eq!(config["voice_profile"]["voice_id"], "male_0004_a");
    assert_eq!(config["voice_profile"]["speed_x100"], 100);
    assert_eq!(config["api_key_env"], "CUSTOM_SENSEAUDIO_KEY");
}

#[test]
fn set_never_carries_forward_a_hand_edited_verified_status() {
    let fixture = Fixture::new();

    // 手工把配置改成 verified：本切片没有目录校验能力，set 必须重新标记 unverified，
    // 不能替用户宣称当前 provider 可用。
    fs::create_dir_all(fixture.speech_root()).expect("speech root");
    fs::write(
        fixture.config_path(),
        r#"{
  "schema_version": 1,
  "api_key_env": "SENSEAUDIO_API_KEY",
  "voice_profile": {
    "provider": "senseaudio",
    "model": "sensenova-tts-2.0",
    "voice_id": "male_0004_a",
    "emotion_label": null,
    "style_label": null,
    "speed_x100": 100,
    "volume_x100": 100,
    "pitch": 0,
    "verification_status": "verified",
    "verified_at": "2026-09-11T00:00:00Z",
    "audio": { "format": "mp3", "sample_rate": 32000, "bitrate": 128000, "channel": 2 }
  }
}"#,
    )
    .expect("seed verified config");

    let value = succeeded(&fixture.run(&[
        "speech",
        "profile",
        "set",
        "--voice-id",
        "male_0004_a",
        "--speed",
        "1.5",
        "--json",
    ]));

    assert_eq!(value["receipt"]["profile"]["verification_status"], "unverified");
    assert_eq!(value["receipt"]["profile"]["verified_at"], Value::Null);
    assert_eq!(
        fixture.config()["voice_profile"]["verification_status"],
        "unverified"
    );
}

#[test]
fn corrupt_or_unsupported_stored_config_fails_with_stable_codes() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.speech_root()).expect("speech root");

    fs::write(fixture.config_path(), "{ not json").expect("write corrupt config");
    let corrupt = failed(&fixture.run(&["speech", "profile", "show", "--json"]));
    assert_eq!(corrupt["error"]["code"], "SPEECH_PROFILE_INVALID");
    assert_eq!(corrupt["error"]["details"]["reason"], "stored_config_invalid");

    fs::write(
        fixture.config_path(),
        r#"{"schema_version": 99, "api_key_env": "X", "voice_profile": {}}"#,
    )
    .expect("write future config");
    let future = failed(&fixture.run(&["speech", "profile", "show", "--json"]));
    assert_eq!(future["error"]["code"], "UNSUPPORTED_SCHEMA_VERSION");

    fs::write(
        fixture.config_path(),
        r#"{
  "schema_version": 1,
  "api_key_env": "SENSEAUDIO_API_KEY",
  "api_key": "canary-secret-1f4b2c-do-not-persist",
  "voice_profile": {
    "provider": "senseaudio",
    "model": "sensenova-tts-2.0",
    "voice_id": "male_0004_a",
    "emotion_label": null,
    "style_label": null,
    "speed_x100": 100,
    "volume_x100": 100,
    "pitch": 0,
    "verification_status": "unverified",
    "verified_at": null,
    "audio": { "format": "mp3", "sample_rate": 32000, "bitrate": 128000, "channel": 2 }
  }
}"#,
    )
    .expect("write config with an unexpected secret field");
    let unexpected = failed(&fixture.run(&["speech", "profile", "show", "--json"]));
    assert_eq!(unexpected["error"]["code"], "SPEECH_PROFILE_INVALID");
    assert_eq!(unexpected["error"]["details"]["reason"], "stored_config_invalid");
}

#[test]
fn stored_config_is_an_allowlisted_non_secret_document() {
    let fixture = Fixture::new();

    succeeded(
        &fixture
            .command()
            .env("SENSEAUDIO_API_KEY", SECRET_CANARY)
            .args([
                "speech",
                "profile",
                "set",
                "--voice-id",
                "male_0004_a",
                "--api-key-env",
                "SENSEAUDIO_API_KEY",
                "--json",
            ])
            .output()
            .expect("run CLI"),
    );

    let config = fixture.config();
    let mut keys: Vec<&str> = config
        .as_object()
        .expect("config object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["api_key_env", "schema_version", "voice_profile"]);

    let mut profile_keys: Vec<&str> = config["voice_profile"]
        .as_object()
        .expect("profile object")
        .keys()
        .map(String::as_str)
        .collect();
    profile_keys.sort_unstable();
    assert_eq!(
        profile_keys,
        [
            "audio",
            "emotion_label",
            "model",
            "pitch",
            "provider",
            "speed_x100",
            "style_label",
            "verification_status",
            "verified_at",
            "voice_id",
            "volume_x100",
        ]
    );

    let mut audio_keys: Vec<&str> = config["voice_profile"]["audio"]
        .as_object()
        .expect("audio object")
        .keys()
        .map(String::as_str)
        .collect();
    audio_keys.sort_unstable();
    assert_eq!(audio_keys, ["bitrate", "channel", "format", "sample_rate"]);

    assert_eq!(config["api_key_env"], "SENSEAUDIO_API_KEY");
    assert_eq!(
        config["voice_profile"]["voice_id"], "male_0004_a",
        "only non-secret profile values may be persisted"
    );
}

#[test]
fn no_speech_state_file_ever_contains_the_api_key_secret() {
    let fixture = Fixture::new();

    for args in [
        vec!["speech", "profile", "show", "--json"],
        vec![
            "speech",
            "profile",
            "set",
            "--voice-id",
            "male_0004_a",
            "--speed",
            "1.25",
            "--json",
        ],
        vec!["speech", "profile", "reset", "--json"],
    ] {
        let output = fixture
            .command()
            .env("SENSEAUDIO_API_KEY", SECRET_CANARY)
            .args(&args)
            .output()
            .expect("run CLI");
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !combined.contains(SECRET_CANARY),
            "CLI output for {args:?} leaked the API key"
        );
    }

    let files = fixture.speech_root_files();
    assert!(!files.is_empty(), "set must persist a config file");
    for (path, bytes) in files {
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains(SECRET_CANARY),
            "speech state file {} persisted the API key",
            path.display()
        );
    }
}

#[test]
fn profile_operations_only_touch_the_isolated_speech_root() {
    let fixture = Fixture::new();
    let real_speech_root = real_user_speech_root();
    let before = real_speech_root_state(&real_speech_root);

    succeeded(&fixture.run(&["speech", "profile", "show", "--json"]));
    succeeded(&fixture.run(&[
        "speech",
        "profile",
        "set",
        "--voice-id",
        "male_0004_a",
        "--json",
    ]));
    succeeded(&fixture.run(&["speech", "profile", "reset", "--json"]));

    assert_eq!(
        real_speech_root_state(&real_speech_root),
        before,
        "profile operations must not touch the real user Speech directory"
    );

    // 隔离根目录里只允许出现非秘密 config.json：没有 catalog/cache/attempt 等 provider 状态。
    let files = fixture.speech_root_files();
    assert_eq!(files.len(), 1, "unexpected speech state files: {files:?}");
    assert_eq!(files[0].0, fixture.config_path());
}

#[test]
fn a_missing_home_directory_is_a_stable_storage_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_apple-books-exporter"))
        .args(["speech", "profile", "show", "--json"])
        .env_remove("HOME")
        .output()
        .expect("run CLI");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let value: Value = serde_json::from_slice(&output.stderr).expect("stderr is JSON only");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["error"]["code"], "SPEECH_STORAGE_UNAVAILABLE");
    assert_eq!(value["error"]["details"]["reason"], "storage_unavailable");
    assert!(value["error"]["message"]
        .as_str()
        .expect("message")
        .contains("home"));
}

#[test]
fn the_speech_root_ignores_the_config_flag() {
    let fixture = Fixture::new();
    let elsewhere = fixture.home().join("elsewhere/knowledge_config.json");
    let elsewhere_arg = elsewhere.to_string_lossy().into_owned();

    let value = succeeded(&fixture.run(&[
        "--config",
        &elsewhere_arg,
        "speech",
        "profile",
        "set",
        "--voice-id",
        "male_0004_a",
        "--speed",
        "1.5",
        "--json",
    ]));

    assert_eq!(
        value["receipt"]["config_path"],
        fixture.config_path().to_string_lossy().as_ref(),
        "the Speech root must be derived from the user home, never from --config"
    );
    assert!(fixture.config_path().exists());
    assert!(
        !elsewhere.exists(),
        "a Voice Profile must never be written next to --config"
    );
}

#[test]
fn profile_operations_need_no_network_and_no_apple_books_database() {
    let fixture = Fixture::new();

    // HOME 里没有 Apple Books 数据库，代理指向死端口，没有任何凭证。
    let show = succeeded(&fixture.run(&["speech", "profile", "show", "--json"]));
    assert_eq!(show["receipt"]["operation"], "profile_show");
    let set = succeeded(&fixture.run(&[
        "speech",
        "profile",
        "set",
        "--voice-id",
        "male_0004_a",
        "--json",
    ]));
    assert_eq!(set["receipt"]["operation"], "profile_set");
    assert_eq!(set["receipt"]["warnings"][0]["reason"], "no_catalog");
    assert_eq!(set["receipt"]["profile"]["verification_status"], "unverified");
    let reset = succeeded(&fixture.run(&["speech", "profile", "reset", "--json"]));
    assert_eq!(reset["receipt"]["operation"], "profile_reset");
}

#[test]
fn human_modes_print_readable_profile_output() {
    let fixture = Fixture::new();

    let show = fixture
        .command()
        .env("SENSEAUDIO_API_KEY", SECRET_CANARY)
        .args(["speech", "profile", "show"])
        .output()
        .expect("run CLI");
    assert!(
        show.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&show.stderr)
    );
    let show_stdout = String::from_utf8_lossy(&show.stdout);
    assert!(show_stdout.contains("male_0004_a"));
    assert!(show_stdout.contains("senseaudio"));
    assert!(show_stdout.contains("SENSEAUDIO_API_KEY"));
    assert!(show_stdout.contains("unverified"));
    assert!(!show_stdout.contains("schema_version"));
    assert!(!show_stdout.contains("api_key\":"));

    let set = fixture.run(&[
        "speech",
        "profile",
        "set",
        "--voice-id",
        "female_0007_b",
        "--speed",
        "1.5",
        "--volume",
        "2",
    ]);
    assert!(
        set.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&set.stderr)
    );
    let set_stdout = String::from_utf8_lossy(&set.stdout);
    assert!(set_stdout.contains("female_0007_b"));
    assert!(set_stdout.contains("1.5"));
    assert!(set_stdout.contains("unverified"));

    let reset = fixture.run(&["speech", "profile", "reset"]);
    assert!(
        reset.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&reset.stderr)
    );
    assert!(String::from_utf8_lossy(&reset.stdout).contains("male_0004_a"));

    // 人类模式下的失败仍然是可读错误，而不是 JSON envelope。
    let invalid = fixture.run(&["speech", "profile", "set", "--voice-id", "x", "--speed", "1.005"]);
    assert!(!invalid.status.success());
    assert!(!String::from_utf8_lossy(&invalid.stderr).contains("schema_version"));
}

fn real_user_speech_root() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("test process HOME"))
        .join("Library/Application Support/books-exporter/speech")
}

/// 真实用户 Speech 目录的非侵入式快照：只看是否存在、大小与修改时间，不读取内容。
fn real_speech_root_state(root: &Path) -> Vec<(PathBuf, Option<u64>, Option<std::time::SystemTime>)> {
    fn walk(dir: &Path, entries: &mut Vec<(PathBuf, Option<u64>, Option<std::time::SystemTime>)>) {
        let read = match fs::read_dir(dir) {
            Ok(read) => read,
            Err(_) => return,
        };
        for entry in read.flatten() {
            let path = entry.path();
            let metadata = fs::metadata(&path).ok();
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
