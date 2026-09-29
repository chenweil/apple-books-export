//! Voice Catalog domain types 与 Voice Profile 可用性验证决策。
//!
//! 本模块只拥有「拿到的目录是否还新鲜」和「Profile 能不能被判定为 verified / unavailable」
//! 这两个本地决策。真实的 SenseAudio `get_voice` 请求由 [`super::senseaudio`] 提供，
//! 不能让 profile 命令自己联网。

use crate::speech::profile::VoiceProfile;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Voice Catalog 的新鲜期：ADR 0007 规定本地缓存 24 小时。
pub const CATALOG_FRESHNESS_HOURS: i64 = 24;

/// 与供应商无关的目录分组。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CatalogSourceType {
    /// 供应商自有/系统音色。
    System,
    /// 当前账号的克隆音色。
    Cloned,
    /// 当前账号的生成音色。
    Generated,
}

impl CatalogSourceType {
    /// 稳定的机器可读取值。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Cloned => "cloned",
            Self::Generated => "generated",
        }
    }
}

/// 目录里的一个具体音色。标签来自 provider 目录，不从 `voice_id` 后缀推断。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogVoice {
    /// 账号响应里的分组。
    pub source_type: CatalogSourceType,
    /// 供应商的精确音色 ID；用户选择的永远是它。
    pub voice_id: String,
    /// 供应商展示名。
    pub voice_name: String,
    /// provider 拥有的情感标签。
    pub emotion_label: Option<String>,
    /// provider 拥有的风格标签。
    pub style_label: Option<String>,
    /// 供应商拥有的展示描述；原样保留，绝不从 `voice_id` 拼出来。
    #[serde(default)]
    pub description: Vec<String>,
    /// 供应商返回的创建时间。
    #[serde(default)]
    pub created_time: Option<String>,
}

/// 某个 Speech Provider 的当前 Voice Catalog。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceCatalog {
    /// 目录所属 provider。
    pub provider: String,
    /// 获取时间。
    pub fetched_at: DateTime<Utc>,
    /// 账号当前可见的音色。
    pub voices: Vec<CatalogVoice>,
}

impl VoiceCatalog {
    /// 落盘或当作权限证据前，校验缓存目录的形状。
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.provider.trim().is_empty() {
            return Err("provider is empty");
        }
        for voice in &self.voices {
            if voice.voice_id.trim().is_empty() {
                return Err("voice_id is empty");
            }
            if voice.voice_name.trim().is_empty() {
                return Err("voice_name is empty");
            }
            if voice
                .description
                .iter()
                .any(|description| description.trim().is_empty())
            {
                return Err("description contains an empty label");
            }
        }
        Ok(())
    }

    /// 只有“已经过去且不足 24 小时”的目录才算新鲜。
    ///
    /// 未来时间戳（时钟回拨或被篡改的文件）一律视为不新鲜，避免用它宣称当前权限。
    pub fn is_fresh(&self, now: DateTime<Utc>) -> bool {
        let age = now.signed_duration_since(self.fetched_at);
        age >= Duration::zero() && age < Duration::hours(CATALOG_FRESHNESS_HOURS)
    }

    /// 精确匹配音色 ID，不做前缀/后缀或名称匹配。
    pub fn contains_voice(&self, voice_id: &str) -> bool {
        self.voices.iter().any(|voice| voice.voice_id == voice_id)
    }

    /// 取出精确匹配的目录项。
    pub fn voice(&self, voice_id: &str) -> Option<&CatalogVoice> {
        self.voices.iter().find(|voice| voice.voice_id == voice_id)
    }
}

/// 调用方能否拿到某个 provider 的目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogAvailability {
    /// 有本地目录；是否新鲜由 [`VoiceCatalog::is_fresh`] 判断。
    Available(VoiceCatalog),
    /// 没有目录：未刷新、没有凭证、离线，或该 provider 未知。
    Unavailable,
}

/// 注入式目录来源。
///
/// issue #21 的 profile 命令只使用 [`NoCatalogSource`]，因此永远不联网；
/// issue #22 会提供读取 24 小时磁盘缓存的实现。
pub trait VoiceCatalogSource {
    /// 返回指定 provider 当前可用的目录。
    fn current_catalog(&self, provider: &str) -> CatalogAvailability;
}

/// 没有目录可用的来源：profile 命令的默认值。
#[derive(Debug, Clone, Copy, Default)]
pub struct NoCatalogSource;

impl VoiceCatalogSource for NoCatalogSource {
    fn current_catalog(&self, _provider: &str) -> CatalogAvailability {
        CatalogAvailability::Unavailable
    }
}

/// `set` 之后 Profile 的可用性结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceVerification {
    /// 对照新鲜目录确认音色可用。
    Verified,
    /// 本地合法但无法确认可用性；原因进入结构化 warning。
    Unverified(UnverifiedReason),
    /// 新鲜目录里没有这个音色：稳定不可用，调用方必须拒绝写入。
    Unavailable,
}

/// 无法验证可用性的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnverifiedReason {
    /// 没有拿到目录。
    NoCatalog,
    /// 目录存在但超过 24 小时。
    StaleCatalog,
    /// 目录属于另一个 provider。
    OtherProvider,
}

impl UnverifiedReason {
    /// 机器可读的稳定字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoCatalog => "no_catalog",
            Self::StaleCatalog => "stale_catalog",
            Self::OtherProvider => "other_provider",
        }
    }
}

/// 决定一个 Voice Profile 能否被标记为 verified。
///
/// 顺序固定：新鲜度优先。过期目录既不能证明可用，也不能证明不可用。
pub fn verify_voice(
    profile: &VoiceProfile,
    availability: &CatalogAvailability,
    now: DateTime<Utc>,
) -> VoiceVerification {
    let catalog = match availability {
        CatalogAvailability::Unavailable => {
            return VoiceVerification::Unverified(UnverifiedReason::NoCatalog)
        }
        CatalogAvailability::Available(catalog) => catalog,
    };

    if catalog.provider != profile.provider {
        return VoiceVerification::Unverified(UnverifiedReason::OtherProvider);
    }
    if !catalog.is_fresh(now) {
        return VoiceVerification::Unverified(UnverifiedReason::StaleCatalog);
    }
    if catalog.contains_voice(&profile.voice_id) {
        VoiceVerification::Verified
    } else {
        VoiceVerification::Unavailable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speech::profile::{VoiceProfile, DEFAULT_VOICE_ID};

    fn at(rfc3339: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(rfc3339)
            .expect("valid timestamp")
            .with_timezone(&Utc)
    }

    fn voice(voice_id: &str) -> CatalogVoice {
        CatalogVoice {
            source_type: CatalogSourceType::System,
            voice_id: voice_id.to_string(),
            voice_name: "默认音色".to_string(),
            emotion_label: Some("平稳".to_string()),
            style_label: None,
            description: vec!["平稳".to_string()],
            created_time: None,
        }
    }

    fn catalog(provider: &str, fetched_at: &str, voice_ids: &[&str]) -> VoiceCatalog {
        VoiceCatalog {
            provider: provider.to_string(),
            fetched_at: at(fetched_at),
            voices: voice_ids.iter().map(|id| voice(id)).collect(),
        }
    }

    #[test]
    fn fresh_catalog_with_the_voice_verifies_the_profile() {
        let profile = VoiceProfile::default();
        let availability = CatalogAvailability::Available(catalog(
            "senseaudio",
            "2026-09-11T00:00:00Z",
            &[DEFAULT_VOICE_ID, "female_0007_b"],
        ));

        assert_eq!(
            verify_voice(&profile, &availability, at("2026-09-11T01:00:00Z")),
            VoiceVerification::Verified
        );
    }

    #[test]
    fn fresh_catalog_without_the_voice_is_stably_unavailable() {
        let profile = VoiceProfile::default();
        let availability = CatalogAvailability::Available(catalog(
            "senseaudio",
            "2026-09-11T00:00:00Z",
            &["female_0007_b"],
        ));

        assert_eq!(
            verify_voice(&profile, &availability, at("2026-09-11T01:00:00Z")),
            VoiceVerification::Unavailable
        );
    }

    #[test]
    fn stale_catalog_never_verifies_or_refutes_a_voice() {
        let profile = VoiceProfile::default();
        let now = at("2026-09-12T12:00:00Z");

        let containing = CatalogAvailability::Available(catalog(
            "senseaudio",
            "2026-09-11T00:00:00Z",
            &[DEFAULT_VOICE_ID],
        ));
        assert_eq!(
            verify_voice(&profile, &containing, now),
            VoiceVerification::Unverified(UnverifiedReason::StaleCatalog),
            "a stale catalog is not current permission evidence"
        );

        let missing = CatalogAvailability::Available(catalog(
            "senseaudio",
            "2026-09-11T00:00:00Z",
            &["female_0007_b"],
        ));
        assert_eq!(
            verify_voice(&profile, &missing, now),
            VoiceVerification::Unverified(UnverifiedReason::StaleCatalog),
            "a stale catalog cannot prove the voice is unavailable either"
        );
    }

    #[test]
    fn freshness_boundary_is_strictly_under_twenty_four_hours() {
        let fetched_at = at("2026-09-11T00:00:00Z");
        let catalog = catalog("senseaudio", "2026-09-11T00:00:00Z", &[DEFAULT_VOICE_ID]);

        assert!(catalog.is_fresh(fetched_at + Duration::minutes(1439)));
        assert!(!catalog.is_fresh(fetched_at + Duration::hours(24)));
        assert!(!catalog.is_fresh(fetched_at + Duration::hours(25)));
    }

    #[test]
    fn a_timestamp_from_the_future_is_not_fresh() {
        let catalog = catalog("senseaudio", "2026-09-11T00:00:00Z", &[DEFAULT_VOICE_ID]);

        assert!(!catalog.is_fresh(at("2026-09-10T23:59:59Z")));
    }

    #[test]
    fn a_catalog_for_another_provider_does_not_verify_this_profile() {
        let profile = VoiceProfile::default();
        let availability = CatalogAvailability::Available(catalog(
            "other-provider",
            "2026-09-11T00:00:00Z",
            &[DEFAULT_VOICE_ID],
        ));

        assert_eq!(
            verify_voice(&profile, &availability, at("2026-09-11T01:00:00Z")),
            VoiceVerification::Unverified(UnverifiedReason::OtherProvider)
        );
    }

    #[test]
    fn without_a_catalog_the_profile_stays_unverified() {
        let profile = VoiceProfile::default();

        assert_eq!(
            verify_voice(
                &profile,
                &CatalogAvailability::Unavailable,
                at("2026-09-11T01:00:00Z")
            ),
            VoiceVerification::Unverified(UnverifiedReason::NoCatalog)
        );
    }

    #[test]
    fn voice_lookup_is_exact_and_never_infers_labels_from_the_id() {
        let catalog = catalog(
            "senseaudio",
            "2026-09-11T00:00:00Z",
            &["male_0004_a_yuansheng", "female_0007_b"],
        );

        assert!(!catalog.contains_voice(DEFAULT_VOICE_ID));
        assert!(catalog.contains_voice("male_0004_a_yuansheng"));
        assert!(!catalog.contains_voice("male_0004"));

        let found = catalog
            .voice("male_0004_a_yuansheng")
            .expect("exact voice present");
        assert_eq!(found.emotion_label.as_deref(), Some("平稳"));
        assert_eq!(found.style_label, None);
        assert!(catalog.voice(DEFAULT_VOICE_ID).is_none());
    }

    #[test]
    fn the_default_catalog_source_provides_no_catalog() {
        let source = NoCatalogSource;

        assert_eq!(
            source.current_catalog("senseaudio"),
            CatalogAvailability::Unavailable,
            "profile commands must never fetch a catalog themselves"
        );
        assert_eq!(
            source.current_catalog("any-other-provider"),
            CatalogAvailability::Unavailable
        );
    }

    #[test]
    fn unverified_reasons_have_stable_machine_strings() {
        assert_eq!(UnverifiedReason::NoCatalog.as_str(), "no_catalog");
        assert_eq!(UnverifiedReason::StaleCatalog.as_str(), "stale_catalog");
        assert_eq!(UnverifiedReason::OtherProvider.as_str(), "other_provider");
    }
}
