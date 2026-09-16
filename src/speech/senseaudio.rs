//! SenseAudio Voice Catalog 适配器与缓存 use case。
//!
//! 本模块拥有供应商特有的 HTTP 形状。缓存通过现有 SpeechStore 落盘，
//! 因此 Profile 校验和目录浏览共用一个状态根、一份目录文档。

use super::catalog::{CatalogSourceType, CatalogVoice, VoiceCatalog, VoiceCatalogSource};
use super::store::{SpeechStore, SpeechStoreError};
use super::SpeechWarning;
use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde::Deserialize;
use std::env;
use std::future::Future;
use std::time::Duration;

/// 非秘密的本地 endpoint 覆盖；只给合同测试和受控环境用。
pub const SENSEAUDIO_API_BASE_URL_ENV: &str = "SENSEAUDIO_API_BASE_URL";
/// 默认 SenseAudio API origin。
pub const SENSEAUDIO_DEFAULT_BASE_URL: &str = "https://api.senseaudio.cn";
const VOICE_LIST_PATH: &str = "/v1/get_voice";

/// 供应商失败刻意保持粗粒度：诊断信息不得回显 Authorization 头或不受信的响应体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SenseAudioError {
    /// 配置的 API Key 环境变量缺失或为空。
    MissingApiKey,
    /// 供应商拒绝了当前凭证。
    AuthenticationFailed,
    /// 供应商要求调用方降速。
    RateLimited,
    /// 请求未能拿到响应。
    Transport,
    /// 供应商响应不符合已接受的目录合同。
    InvalidResponse,
    /// 供应商返回了非成功状态或明确的失败状态。
    ProviderFailed,
}

impl SenseAudioError {
    /// 稳定的 Machine JSON 错误码。
    pub const fn machine_code(&self) -> &'static str {
        match self {
            Self::MissingApiKey | Self::AuthenticationFailed => "SPEECH_AUTH_FAILED",
            Self::RateLimited => "SPEECH_RATE_LIMITED",
            Self::Transport | Self::InvalidResponse | Self::ProviderFailed => {
                "SPEECH_PROVIDER_FAILED"
            }
        }
    }

    /// 短且不含秘密的原因码；只进入 stale warning。
    pub const fn reason_code(&self) -> &'static str {
        match self {
            Self::MissingApiKey => "missing_api_key",
            Self::AuthenticationFailed => "authentication_failed",
            Self::RateLimited => "rate_limited",
            Self::Transport => "transport_failed",
            Self::InvalidResponse => "invalid_response",
            Self::ProviderFailed => "provider_failed",
        }
    }
}

impl std::fmt::Display for SenseAudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::MissingApiKey => {
                "SenseAudio authentication requires an API key in the configured environment variable"
            }
            Self::AuthenticationFailed => "SenseAudio authentication failed",
            Self::RateLimited => "SenseAudio rate-limited the Voice Catalog request",
            Self::Transport => "the SenseAudio Voice Catalog request failed before a response was received",
            Self::InvalidResponse => "SenseAudio returned a malformed Voice Catalog response",
            Self::ProviderFailed => "SenseAudio rejected the Voice Catalog request",
        };
        f.write_str(message)
    }
}

/// 缓存边界或 SenseAudio 供应商失败。
#[derive(Debug)]
pub enum VoiceCatalogError {
    /// 本地目录状态读不到或写不了。
    Storage(SpeechStoreError),
    /// 供应商请求或响应失败。
    Provider(SenseAudioError),
}

impl VoiceCatalogError {
    /// 稳定的 Machine JSON 错误码。
    pub const fn machine_code(&self) -> &'static str {
        match self {
            Self::Storage(_) => "SPEECH_STORAGE_UNAVAILABLE",
            Self::Provider(error) => error.machine_code(),
        }
    }

    /// 远程适配器失败时的供应商错误。
    pub fn provider_error(&self) -> Option<&SenseAudioError> {
        match self {
            Self::Storage(_) => None,
            Self::Provider(error) => Some(error),
        }
    }
}

impl From<SpeechStoreError> for VoiceCatalogError {
    fn from(error: SpeechStoreError) -> Self {
        Self::Storage(error)
    }
}

impl From<SenseAudioError> for VoiceCatalogError {
    fn from(error: SenseAudioError) -> Self {
        Self::Provider(error)
    }
}

impl std::fmt::Display for VoiceCatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(error) => write!(f, "{error}"),
            Self::Provider(error) => write!(f, "{error}"),
        }
    }
}

/// 显式 Voice Catalog 操作的小型供应商适配器。
#[derive(Clone)]
pub struct SenseAudioClient {
    client: reqwest::Client,
    base_url: String,
    api_key_env: String,
}

impl std::fmt::Debug for SenseAudioClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SenseAudioClient")
            .field("base_url", &self.base_url)
            .field("api_key_env", &self.api_key_env)
            .finish_non_exhaustive()
    }
}

impl SenseAudioClient {
    /// 用显式 endpoint 和环境变量名构造客户端。在真正发起请求前不读、不保留密钥。
    pub fn new(
        base_url: impl Into<String>,
        api_key_env: impl Into<String>,
    ) -> Result<Self, SenseAudioError> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err(SenseAudioError::Transport);
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| SenseAudioError::Transport)?;
        Ok(Self {
            client,
            base_url,
            api_key_env: api_key_env.into(),
        })
    }

    /// 使用文档默认 endpoint；仅当存在受控覆盖时改走本地地址。
    pub fn from_environment(api_key_env: impl Into<String>) -> Result<Self, SenseAudioError> {
        let base_url = env::var(SENSEAUDIO_API_BASE_URL_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| SENSEAUDIO_DEFAULT_BASE_URL.to_string());
        Self::new(base_url, api_key_env)
    }

    /// 从显式的全部音色 endpoint 拉取账号可见目录。密钥只为这次请求读取，
    /// 永不进入错误值或返回的目录。
    pub async fn fetch_catalog(&self) -> Result<Vec<CatalogVoice>, SenseAudioError> {
        let api_key = env::var(&self.api_key_env)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or(SenseAudioError::MissingApiKey)?;
        let url = format!("{}{}", self.base_url, VOICE_LIST_PATH);
        let response = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({ "voice_type": "all" }))
            .send()
            .await
            .map_err(|_| SenseAudioError::Transport)?;
        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|_| SenseAudioError::Transport)?;

        if status == StatusCode::UNAUTHORIZED {
            return Err(SenseAudioError::AuthenticationFailed);
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(SenseAudioError::RateLimited);
        }
        if !status.is_success() {
            return Err(SenseAudioError::ProviderFailed);
        }
        parse_voice_catalog_response_with_secret(status.as_u16(), &body, Some(&api_key))
    }
}

/// 解析成功或失败的供应商响应，不保留原始 JSON。这是给 fixture 和合同测试用的纯函数缝。
pub fn parse_voice_catalog_response(
    http_status: u16,
    body: &[u8],
) -> Result<Vec<CatalogVoice>, SenseAudioError> {
    parse_voice_catalog_response_with_secret(http_status, body, None)
}

fn parse_voice_catalog_response_with_secret(
    http_status: u16,
    body: &[u8],
    secret: Option<&str>,
) -> Result<Vec<CatalogVoice>, SenseAudioError> {
    if let Some(secret) = secret.filter(|secret| !secret.is_empty()) {
        if body
            .windows(secret.len())
            .any(|window| window == secret.as_bytes())
        {
            return Err(SenseAudioError::InvalidResponse);
        }
    }
    if http_status == StatusCode::UNAUTHORIZED.as_u16() {
        return Err(SenseAudioError::AuthenticationFailed);
    }
    if http_status == StatusCode::TOO_MANY_REQUESTS.as_u16() {
        return Err(SenseAudioError::RateLimited);
    }
    if !(200..300).contains(&http_status) {
        return Err(SenseAudioError::ProviderFailed);
    }

    let response: SenseAudioVoiceResponse =
        serde_json::from_slice(body).map_err(|_| SenseAudioError::InvalidResponse)?;
    let base_resp = response.base_resp.ok_or(SenseAudioError::InvalidResponse)?;
    if base_resp.status_code != 0 {
        return Err(SenseAudioError::ProviderFailed);
    }

    let mut voices = Vec::new();
    append_group(
        &mut voices,
        response.system_voice,
        CatalogSourceType::System,
        secret,
    )?;
    append_group(
        &mut voices,
        response.voice_cloning,
        CatalogSourceType::Cloned,
        secret,
    )?;
    append_group(
        &mut voices,
        response.voice_generation,
        CatalogSourceType::Generated,
        secret,
    )?;
    Ok(voices)
}

fn append_group(
    voices: &mut Vec<CatalogVoice>,
    entries: Option<Vec<SenseAudioVoice>>,
    source_type: CatalogSourceType,
    secret: Option<&str>,
) -> Result<(), SenseAudioError> {
    for entry in entries.unwrap_or_default() {
        let voice_id = non_empty(Some(entry.voice_id)).ok_or(SenseAudioError::InvalidResponse)?;
        let voice_name = non_empty(entry.voice_name).unwrap_or_else(|| voice_id.clone());
        let description = entry.description.unwrap_or_default();
        if description.iter().any(|label| label.trim().is_empty())
            || secret.is_some_and(|secret| {
                [voice_id.as_str(), voice_name.as_str()]
                    .into_iter()
                    .chain(description.iter().map(String::as_str))
                    .chain(entry.emotion_label.as_deref())
                    .chain(entry.style_label.as_deref())
                    .chain(entry.created_time.as_deref())
                    .any(|value| value.contains(secret))
            })
        {
            return Err(SenseAudioError::InvalidResponse);
        }
        voices.push(CatalogVoice {
            source_type,
            voice_id,
            voice_name,
            emotion_label: non_empty(entry.emotion_label),
            style_label: non_empty(entry.style_label),
            description,
            created_time: non_empty(entry.created_time),
        });
    }
    Ok(())
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

#[derive(Debug, Deserialize)]
struct SenseAudioVoiceResponse {
    #[serde(default)]
    system_voice: Option<Vec<SenseAudioVoice>>,
    #[serde(default)]
    voice_cloning: Option<Vec<SenseAudioVoice>>,
    #[serde(default)]
    voice_generation: Option<Vec<SenseAudioVoice>>,
    #[serde(default)]
    base_resp: Option<BaseResponse>,
}

#[derive(Debug, Deserialize)]
struct SenseAudioVoice {
    voice_id: String,
    #[serde(default)]
    voice_name: Option<String>,
    #[serde(default, alias = "emotion")]
    emotion_label: Option<String>,
    #[serde(default, alias = "style")]
    style_label: Option<String>,
    #[serde(default)]
    description: Option<Vec<String>>,
    #[serde(default)]
    created_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BaseResponse {
    status_code: i64,
}

/// 读到新鲜目录，或展示 stale 回退时的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceCatalogOutcome {
    /// 账号可见的目录条目。
    pub catalog: VoiceCatalog,
    /// 仅当最近一次刷新被请求或必须刷新且失败时为 true。
    pub stale: bool,
    /// 伴随 stale 回退或空目录的诚实 warning。
    pub warnings: Vec<SpeechWarning>,
}

/// 读取新鲜缓存，或通过注入的 fetcher 显式刷新。
///
/// 缓存文档不可读（缺失、损坏、schema 不匹配、读取失败）一律按“没有可用缓存”处理，
/// 不能在刷新前短路，否则 `--refresh` 会卡在损坏文件上。只有真的读到可用旧目录时，
/// 刷新失败才回退到 stale；否则如实返回供应商失败。
///
/// 注入的 future 是公开 mock 缝：测试可以覆盖缓存、stale 回退和供应商失败，
/// 而不把凭证或网络调用放进 store。
pub async fn load_or_refresh_voice_catalog<F, Fut>(
    store: &SpeechStore,
    provider: &str,
    now: DateTime<Utc>,
    force_refresh: bool,
    fetch: F,
) -> Result<VoiceCatalogOutcome, VoiceCatalogError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Vec<CatalogVoice>, SenseAudioError>>,
{
    // 损坏的缓存文档是死路：`--refresh` 的语义就是绕过缓存，而这些缓存本身已不可用。
    // 把它当成“没有可用缓存”，让刷新有机会原子替换掉它。
    let cached = match store.load_voice_catalog(provider) {
        Ok(catalog) => catalog,
        Err(_) => None,
    };
    if !force_refresh && cached.as_ref().is_some_and(|catalog| catalog.is_fresh(now)) {
        let catalog = cached.expect("fresh cache exists");
        let warnings = if catalog.voices.is_empty() {
            vec![SpeechWarning::empty_catalog()]
        } else {
            Vec::new()
        };
        return Ok(VoiceCatalogOutcome {
            catalog,
            stale: false,
            warnings,
        });
    }

    match fetch().await {
        Ok(voices) => {
            let catalog = VoiceCatalog {
                provider: provider.to_string(),
                fetched_at: now,
                voices,
            };
            catalog
                .validate()
                .map_err(|_| SenseAudioError::InvalidResponse)?;
            store.save_voice_catalog(&catalog)?;
            let warnings = if catalog.voices.is_empty() {
                vec![SpeechWarning::empty_catalog()]
            } else {
                Vec::new()
            };
            Ok(VoiceCatalogOutcome {
                catalog,
                stale: false,
                warnings,
            })
        }
        Err(error) => match cached {
            Some(catalog) => {
                let mut warnings = vec![SpeechWarning::stale_catalog(
                    catalog.fetched_at,
                    error.reason_code(),
                )];
                if catalog.voices.is_empty() {
                    warnings.push(SpeechWarning::empty_catalog());
                }
                Ok(VoiceCatalogOutcome {
                    warnings,
                    catalog,
                    stale: true,
                })
            }
            None => Err(VoiceCatalogError::Provider(error)),
        },
    }
}

/// Profile 校验用的只读目录来源。从不刷新，也不做网络 I/O；缺失或无效缓存保持不可用。
#[derive(Debug, Clone)]
pub struct CachedVoiceCatalogSource {
    store: SpeechStore,
}

impl CachedVoiceCatalogSource {
    /// 用现有 SpeechStore 构造缓存目录来源。
    pub fn new(store: SpeechStore) -> Self {
        Self { store }
    }
}

impl VoiceCatalogSource for CachedVoiceCatalogSource {
    fn current_catalog(&self, provider: &str) -> super::catalog::CatalogAvailability {
        match self.store.load_voice_catalog(provider) {
            Ok(Some(catalog)) => super::catalog::CatalogAvailability::Available(catalog),
            Ok(None) | Err(_) => super::catalog::CatalogAvailability::Unavailable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speech::catalog::CatalogAvailability;
    use crate::speech::profile::SENSEAUDIO_PROVIDER;
    use crate::speech::CATALOG_FRESHNESS_HOURS;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tempfile::TempDir;

    fn timestamp(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("timestamp")
            .with_timezone(&Utc)
    }

    fn store() -> (TempDir, SpeechStore) {
        let home = tempfile::tempdir().expect("home");
        let store = SpeechStore::from_home(home.path());
        (home, store)
    }

    /// 直接落一个原始缓存文档，用来模拟损坏或异构的本地状态。
    fn seed_catalog_document(store: &SpeechStore, body: &str) {
        let path = store.voice_catalog_path(SENSEAUDIO_PROVIDER);
        std::fs::create_dir_all(path.parent().expect("catalog directory"))
            .expect("catalog dir");
        std::fs::write(path, body).expect("seed catalog document");
    }

    fn entry(
        source_type: CatalogSourceType,
        voice_id: &str,
        voice_name: &str,
        description: &[&str],
    ) -> CatalogVoice {
        CatalogVoice {
            source_type,
            voice_id: voice_id.to_string(),
            voice_name: voice_name.to_string(),
            emotion_label: None,
            style_label: None,
            description: description
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            created_time: Some("2026-09-11T00:00:00Z".to_string()),
        }
    }

    #[test]
    fn parser_preserves_all_account_response_groups_and_metadata() {
        let body = br#"{
          "system_voice": [{
            "voice_id": "system-voice",
            "voice_name": "\u7cfb\u7edf\u64ad\u97f3",
            "description": ["\u5e73\u7a33", "\u8bb2\u89e3"],
            "created_time": "2026-09-11T00:00:00Z",
            "emotion": "\u5e73\u7a33",
            "style": "\u8bb2\u89e3"
          }],
          "voice_cloning": [{
            "voice_id": "cloned-voice",
            "voice_name": "\u6211\u7684\u514b\u9686",
            "description": ["\u8d26\u53f7\u63d0\u4f9b"],
            "created_time": "2026-09-10T00:00:00Z"
          }],
          "voice_generation": [{
            "voice_id": "generated-voice",
            "voice_name": "\u6211\u7684\u751f\u6210",
            "description": ["\u5b9e\u9a8c"],
            "created_time": null
          }],
          "base_resp": {"status_code": 0, "status_msg": "success"}
        }"#;

        let voices = parse_voice_catalog_response(200, body).expect("catalog");

        assert_eq!(voices.len(), 3);
        assert_eq!(voices[0].source_type, CatalogSourceType::System);
        assert_eq!(voices[0].voice_id, "system-voice");
        assert_eq!(voices[0].emotion_label.as_deref(), Some("平稳"));
        assert_eq!(voices[0].style_label.as_deref(), Some("讲解"));
        assert_eq!(voices[0].description, ["平稳", "讲解"]);
        assert_eq!(voices[1].source_type, CatalogSourceType::Cloned);
        assert_eq!(voices[1].voice_id, "cloned-voice");
        assert_eq!(voices[2].source_type, CatalogSourceType::Generated);
        assert_eq!(voices[2].created_time, None);
    }

    #[test]
    fn parser_uses_only_provider_metadata_and_never_voice_id_suffixes() {
        let body = br#"{
          "system_voice": [{
            "voice_id": "male_0004_a_yuansheng",
            "voice_name": "\u9ed8\u8ba4\u97f3\u8272",
            "description": ["\u4f9b\u5e94\u5546\u6807\u7b7e"]
          }],
          "base_resp": {"status_code": 0}
        }"#;

        let voices = parse_voice_catalog_response(200, body).expect("catalog");

        assert_eq!(voices[0].voice_id, "male_0004_a_yuansheng");
        assert_eq!(voices[0].emotion_label, None);
        assert_eq!(voices[0].style_label, None);
        assert_eq!(voices[0].description, ["供应商标签"]);
    }

    #[test]
    fn parser_maps_authentication_failure_without_echoing_a_secret() {
        let secret = "catalog-secret-not-for-output";
        let error = parse_voice_catalog_response(
            401,
            format!(r#"{{"message":"Bearer {secret}"}}"#).as_bytes(),
        )
        .expect_err("401");

        assert_eq!(error, SenseAudioError::AuthenticationFailed);
        let debug = format!("{error:?}");
        let display = error.to_string();
        assert!(!debug.contains(secret));
        assert!(!display.contains(secret));
    }

    #[test]
    fn malformed_json_and_provider_status_are_rejected() {
        assert_eq!(
            parse_voice_catalog_response(200, br#"not json"#),
            Err(SenseAudioError::InvalidResponse)
        );
        assert_eq!(
            parse_voice_catalog_response(200, br#"{"base_resp":{"status_code":0}}"#)
                .expect("an empty successful catalog is valid"),
            Vec::<CatalogVoice>::new()
        );
        assert_eq!(
            parse_voice_catalog_response(
                200,
                br#"{"system_voice":[{"voice_id":"x"}],"base_resp":{"status_code":9}}"#
            ),
            Err(SenseAudioError::ProviderFailed)
        );
        assert_eq!(
            parse_voice_catalog_response(
                200,
                br#"{"system_voice":[{"voice_id":""}],"base_resp":{"status_code":0}}"#
            ),
            Err(SenseAudioError::InvalidResponse)
        );
    }

    #[tokio::test]
    async fn an_empty_provider_catalog_is_valid_and_warns() {
        let (_home, store) = store();
        let now = timestamp("2026-09-11T12:00:00Z");

        let outcome =
            load_or_refresh_voice_catalog(&store, SENSEAUDIO_PROVIDER, now, false, || async {
                Ok(Vec::new())
            })
            .await
            .expect("empty catalog");

        assert!(!outcome.stale);
        assert!(outcome.catalog.voices.is_empty());
        assert_eq!(outcome.warnings.len(), 1);
        assert_eq!(outcome.warnings[0].code, SpeechWarning::EMPTY_CATALOG_CODE);
        assert_eq!(outcome.warnings[0].reason, "empty_catalog");
        assert!(outcome.warnings[0].message.contains("账号未返回音色"));
    }

    #[tokio::test]
    async fn fresh_cache_is_reused_without_calling_the_fetcher() {
        let (_home, store) = store();
        let now = timestamp("2026-09-11T12:00:00Z");
        let cached = VoiceCatalog {
            provider: SENSEAUDIO_PROVIDER.to_string(),
            fetched_at: now - chrono::Duration::hours(1),
            voices: vec![entry(
                CatalogSourceType::System,
                "cached",
                "Cached",
                &["平稳"],
            )],
        };
        store.save_voice_catalog(&cached).expect("cache");
        let calls = Arc::new(AtomicUsize::new(0));
        let fetch_calls = Arc::clone(&calls);

        let outcome = load_or_refresh_voice_catalog(
            &store,
            SENSEAUDIO_PROVIDER,
            now,
            false,
            move || async move {
                fetch_calls.fetch_add(1, Ordering::SeqCst);
                Ok(vec![entry(
                    CatalogSourceType::System,
                    "network",
                    "Network",
                    &[],
                )])
            },
        )
        .await
        .expect("fresh cache");

        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(!outcome.stale);
        assert_eq!(outcome.catalog.voices[0].voice_id, "cached");
    }

    #[tokio::test]
    async fn explicit_refresh_bypasses_fresh_cache_and_replaces_it() {
        let (_home, store) = store();
        let now = timestamp("2026-09-11T12:00:00Z");
        let cached = VoiceCatalog {
            provider: SENSEAUDIO_PROVIDER.to_string(),
            fetched_at: now - chrono::Duration::hours(1),
            voices: vec![entry(CatalogSourceType::System, "cached", "Cached", &[])],
        };
        store.save_voice_catalog(&cached).expect("cache");

        let outcome =
            load_or_refresh_voice_catalog(&store, SENSEAUDIO_PROVIDER, now, true, || async {
                Ok(vec![entry(
                    CatalogSourceType::Cloned,
                    "refreshed",
                    "Refreshed",
                    &["账号提供"],
                )])
            })
            .await
            .expect("refresh");

        assert!(!outcome.stale);
        assert_eq!(outcome.catalog.fetched_at, now);
        assert_eq!(outcome.catalog.voices[0].voice_id, "refreshed");
        assert_eq!(
            store
                .load_voice_catalog(SENSEAUDIO_PROVIDER)
                .expect("load")
                .expect("cache")
                .voices[0]
                .voice_id,
            "refreshed"
        );
    }

    /// 损坏的缓存文档不能变成 `--refresh` 的死路。
    #[tokio::test]
    async fn a_corrupt_cache_document_is_repaired_by_an_explicit_refresh() {
        let (_home, store) = store();
        let now = timestamp("2026-09-11T12:00:00Z");
        seed_catalog_document(&store, "{ not a catalog");

        let outcome =
            load_or_refresh_voice_catalog(&store, SENSEAUDIO_PROVIDER, now, true, || async {
                Ok(vec![entry(
                    CatalogSourceType::System,
                    "repaired",
                    "Repaired",
                    &[],
                )])
            })
            .await
            .expect("refresh repairs the cache");

        assert!(!outcome.stale);
        assert_eq!(outcome.catalog.voices[0].voice_id, "repaired");
        assert_eq!(
            store
                .load_voice_catalog(SENSEAUDIO_PROVIDER)
                .expect("load")
                .expect("cache")
                .voices[0]
                .voice_id,
            "repaired"
        );
    }

    /// 缓存不可用且刷新失败时如实报告供应商失败，不把损坏文档当成权限证据。
    #[tokio::test]
    async fn a_corrupt_cache_with_a_failed_refresh_reports_the_provider_failure() {
        let (_home, store) = store();
        seed_catalog_document(&store, "{ not a catalog");

        let error = load_or_refresh_voice_catalog(
            &store,
            SENSEAUDIO_PROVIDER,
            timestamp("2026-09-11T12:00:00Z"),
            true,
            || async { Err(SenseAudioError::AuthenticationFailed) },
        )
        .await
        .expect_err("provider failure");

        assert_eq!(
            error.provider_error(),
            Some(&SenseAudioError::AuthenticationFailed)
        );
        assert!(error.to_string().contains("authentication"));
    }

    #[tokio::test]
    async fn refresh_failure_returns_timestamped_stale_fallback() {
        let (_home, store) = store();
        let now = timestamp("2026-09-12T12:00:00Z");
        let fetched_at = now - chrono::Duration::hours(CATALOG_FRESHNESS_HOURS + 1);
        let cached = VoiceCatalog {
            provider: SENSEAUDIO_PROVIDER.to_string(),
            fetched_at,
            voices: vec![entry(
                CatalogSourceType::System,
                "stale",
                "Stale",
                &["旧标签"],
            )],
        };
        store.save_voice_catalog(&cached).expect("cache");

        let outcome =
            load_or_refresh_voice_catalog(&store, SENSEAUDIO_PROVIDER, now, false, || async {
                Err(SenseAudioError::AuthenticationFailed)
            })
            .await
            .expect("stale fallback");

        assert!(outcome.stale);
        assert_eq!(outcome.catalog.fetched_at, fetched_at);
        assert_eq!(outcome.warnings[0].code, SpeechWarning::STALE_CATALOG_CODE);
        assert!(outcome.warnings[0]
            .message
            .contains(&fetched_at.to_rfc3339()));
        assert!(outcome.warnings[0]
            .message
            .contains("not current permission evidence"));
    }

    #[tokio::test]
    async fn provider_failure_without_a_cache_is_returned_without_sensitive_context() {
        let (_home, store) = store();
        let secret = "missing-cache-secret";
        let error = load_or_refresh_voice_catalog(
            &store,
            SENSEAUDIO_PROVIDER,
            timestamp("2026-09-11T12:00:00Z"),
            false,
            || async { Err(SenseAudioError::AuthenticationFailed) },
        )
        .await
        .expect_err("provider failure");

        assert_eq!(
            error.provider_error(),
            Some(&SenseAudioError::AuthenticationFailed)
        );
        assert!(!error.to_string().contains(secret));
    }

    #[test]
    fn cached_source_reads_the_same_catalog_used_by_the_voices_command() {
        let (_home, store) = store();
        let catalog = VoiceCatalog {
            provider: SENSEAUDIO_PROVIDER.to_string(),
            fetched_at: timestamp("2026-09-11T00:00:00Z"),
            voices: vec![entry(CatalogSourceType::System, "exact-id", "Exact", &[])],
        };
        store.save_voice_catalog(&catalog).expect("cache");
        let source = CachedVoiceCatalogSource::new(store);

        match source.current_catalog(SENSEAUDIO_PROVIDER) {
            CatalogAvailability::Available(value) => {
                assert_eq!(value.voices[0].voice_id, "exact-id")
            }
            other => panic!("expected cached catalog, got {other:?}"),
        }
    }

    #[test]
    fn client_debug_contains_configuration_names_but_not_credentials() {
        let client =
            SenseAudioClient::new("http://127.0.0.1:1", "SENSEAUDIO_API_KEY").expect("client");
        let debug = format!("{client:?}");
        assert!(debug.contains("127.0.0.1"));
        assert!(debug.contains("SENSEAUDIO_API_KEY"));
        assert!(!debug.contains("Bearer"));
    }

    #[test]
    fn timestamps_for_machine_warnings_are_stable() {
        assert_eq!(
            timestamp("2026-09-11T12:00:00.123Z").to_rfc3339(),
            "2026-09-11T12:00:00.123+00:00"
        );
    }
}
