//! Voice Profile 领域类型与非秘密配置校验。
//!
//! 这里的数值全部以「百分之一」为规范单位保存与比较：`speed` 和 `volume` 从 CLI 原始
//! 十进制文本精确解析成整数 `x100`，绝不经过 f64 四舍五入，因此不会出现「显示值相同、
//! Speech 身份不同」的浮点漂移。

use serde::{Deserialize, Serialize};
use std::fmt;

/// 首版唯一支持的 Speech Provider。
pub const SENSEAUDIO_PROVIDER: &str = "senseaudio";
/// 首版默认 TTS 模型。
pub const DEFAULT_MODEL: &str = "sensenova-tts-2.0";
/// ADR 0007 规定的默认音色。
pub const DEFAULT_VOICE_ID: &str = "male_0004_a";
/// 默认 API Key 环境变量名；配置文件只保存这个名字，永远不保存密钥本身。
pub const DEFAULT_API_KEY_ENV: &str = "SENSEAUDIO_API_KEY";

/// `speed` 的合法范围（百分之一单位）。
pub const SPEED_MIN_X100: i32 = 50;
/// 见 [`SPEED_MIN_X100`]。
pub const SPEED_MAX_X100: i32 = 200;
/// `volume` 的合法范围（百分之一单位）。
pub const VOLUME_MIN_X100: i32 = 1;
/// 见 [`VOLUME_MIN_X100`]。
pub const VOLUME_MAX_X100: i32 = 1000;
/// `pitch` 的合法范围。
pub const PITCH_MIN: i32 = -12;
/// 见 [`PITCH_MIN`]。
pub const PITCH_MAX: i32 = 12;

/// 首版用户可见音频规格：MP3、32000Hz、128kbps、双声道。
pub const AUDIO_FORMAT: &str = "mp3";
/// 见 [`AUDIO_FORMAT`]。
pub const AUDIO_SAMPLE_RATE: u32 = 32_000;
/// 见 [`AUDIO_FORMAT`]。
pub const AUDIO_BITRATE: u32 = 128_000;
/// 见 [`AUDIO_FORMAT`]。
pub const AUDIO_CHANNEL: u32 = 2;

/// 百分之一单位的精确数值。
///
/// 内部只保存整数 `x100`；`Display` 与 JSON 序列化都输出精确的十进制文本，
/// 保证 `0.29` 这类值不会因为 f64 格式化而变成 `0.29000000000000004`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hundredths(i32);

impl Hundredths {
    /// 由百分之一单位构造。
    pub const fn from_x100(value: i32) -> Self {
        Self(value)
    }

    /// 百分之一单位的整数值。
    pub const fn x100(self) -> i32 {
        self.0
    }

    /// 供需要浮点的调用方使用；不要用它做比较或序列化。
    pub fn as_f64(self) -> f64 {
        f64::from(self.0) / 100.0
    }
}

impl fmt::Display for Hundredths {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 / 100;
        let fraction = (self.0 % 100).abs();
        if fraction == 0 {
            write!(f, "{whole}.0")
        } else if fraction % 10 == 0 {
            write!(f, "{whole}.{}", fraction / 10)
        } else {
            write!(f, "{whole}.{fraction:02}")
        }
    }
}

impl Serialize for Hundredths {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_f64(self.as_f64())
    }
}

/// 配置校验失败的结构化原因；`as_str()` 是稳定的机器字符串。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileErrorReason {
    /// 必填字段缺失或只有空白。
    Missing,
    /// `NaN`、`inf`、`Infinity` 等非有限值。
    NotFinite,
    /// 不是普通十进制文本（科学计数、十六进制、多个小数点等）。
    NotANumber,
    /// 小数位超过两位，需要四舍五入才能表示。
    NotHundredth,
    /// 整数字段给了小数。
    NotAnInteger,
    /// 数值超出 ADR 0007 的范围。
    OutOfRange,
    /// 字符串字段格式不合法，例如带空格的精确 ID。
    InvalidFormat,
    /// 不是首版支持的 Speech Provider。
    UnsupportedProvider,
    /// 存储的音频规格不是首版固定规格。
    UnsupportedAudioSetting,
    /// 磁盘上的配置不是可解析的 Voice Profile。
    StoredConfigInvalid,
}

impl ProfileErrorReason {
    /// 机器可读的稳定字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::NotFinite => "not_finite",
            Self::NotANumber => "not_a_number",
            Self::NotHundredth => "not_hundredth",
            Self::NotAnInteger => "not_an_integer",
            Self::OutOfRange => "out_of_range",
            Self::InvalidFormat => "invalid_format",
            Self::UnsupportedProvider => "unsupported_provider",
            Self::UnsupportedAudioSetting => "unsupported_audio_setting",
            Self::StoredConfigInvalid => "stored_config_invalid",
        }
    }
}

/// Voice Profile 校验错误。`field` 缺失表示错误针对整个文档结构。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileError {
    /// 出错的字段名，例如 `speed`、`audio`。
    pub field: Option<&'static str>,
    /// 结构化原因。
    pub reason: ProfileErrorReason,
    /// 被拒绝的原始输入（截断后），用于诊断；不含任何秘密。
    pub value: Option<String>,
}

/// 原始输入过长时的截断长度，避免把超大字符串写进错误输出。
const VALUE_PREVIEW_CHARS: usize = 64;

impl ProfileError {
    fn new(field: Option<&'static str>, reason: ProfileErrorReason, value: Option<&str>) -> Self {
        Self {
            field,
            reason,
            value: value.map(|value| value.chars().take(VALUE_PREVIEW_CHARS).collect()),
        }
    }

    /// 磁盘上的配置无法被解析成 Voice Profile。
    pub fn stored_config_invalid() -> Self {
        Self {
            field: None,
            reason: ProfileErrorReason::StoredConfigInvalid,
            value: None,
        }
    }

    /// 面向人类的说明。
    pub fn message(&self) -> String {
        let field = self.field.unwrap_or("voice profile");
        let got = self
            .value
            .as_deref()
            .map(|value| format!(", got '{value}'"))
            .unwrap_or_default();
        match self.reason {
            ProfileErrorReason::Missing => format!("{field} is required and must not be empty"),
            ProfileErrorReason::NotFinite => {
                format!("{field} must be a finite number{got}")
            }
            ProfileErrorReason::NotANumber => format!(
                "{field} must be a plain decimal number with at most two decimal places{got}"
            ),
            ProfileErrorReason::NotHundredth => format!(
                "{field} must be exactly representable in hundredths; values are never rounded{got}"
            ),
            ProfileErrorReason::NotAnInteger => {
                format!("{field} must be a whole number{got}")
            }
            ProfileErrorReason::OutOfRange => {
                format!("{field} must be {}{got}", range_hint(self.field))
            }
            ProfileErrorReason::InvalidFormat => format!("{field} has an invalid format{got}"),
            ProfileErrorReason::UnsupportedProvider => format!(
                "speech provider{got} is not supported by this version; the only supported provider is '{SENSEAUDIO_PROVIDER}'"
            ),
            ProfileErrorReason::UnsupportedAudioSetting => format!(
                "stored audio settings are not supported by this version; expected {AUDIO_FORMAT}/{AUDIO_SAMPLE_RATE}Hz/{AUDIO_BITRATE}bps/{AUDIO_CHANNEL} channels"
            ),
            ProfileErrorReason::StoredConfigInvalid => {
                "the stored speech configuration is not a valid Voice Profile".to_string()
            }
        }
    }
}

fn range_hint(field: Option<&'static str>) -> &'static str {
    match field {
        Some("speed") => "between 0.5 and 2.0",
        Some("volume") => "between 0.01 and 10.0",
        Some("pitch") => "between -12 and 12",
        Some("audio") => "a supported v1 audio specification",
        _ => "within the supported range",
    }
}

/// 校验结果类型别名。
type ProfileResult<T> = Result<T, ProfileError>;

/// 判断是否为 `NaN` / `inf` / `Infinity` 这类非有限字面量（允许一个前导符号，
/// 与 Rust 浮点字面量的写法一致）。
fn is_non_finite_literal(raw: &str) -> bool {
    let unsigned = raw.strip_prefix(['+', '-']).unwrap_or(raw);
    matches!(unsigned.to_ascii_lowercase().as_str(), "nan" | "inf" | "infinity")
}

/// 把 CLI 原始十进制文本精确解析成百分之一单位。
///
/// 只接受 `[+-]?digits[.digits{1,2}]`。科学计数、十六进制、超过两位小数（会隐式四舍五入）
/// 和 `NaN`/`inf` 全部拒绝。
pub fn parse_hundredths(raw: &str, field: &'static str) -> ProfileResult<Hundredths> {
    if raw.trim().is_empty() {
        return Err(ProfileError::new(Some(field), ProfileErrorReason::Missing, None));
    }
    if is_non_finite_literal(raw) {
        return Err(ProfileError::new(
            Some(field),
            ProfileErrorReason::NotFinite,
            Some(raw),
        ));
    }

    let (negative, digits) = match raw.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, raw.strip_prefix('+').unwrap_or(raw)),
    };

    let (whole, fraction) = match digits.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (digits, None),
    };

    let digits_only = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    if !digits_only(whole) {
        return Err(ProfileError::new(
            Some(field),
            ProfileErrorReason::NotANumber,
            Some(raw),
        ));
    }
    if let Some(fraction) = fraction {
        if !digits_only(fraction) {
            return Err(ProfileError::new(
                Some(field),
                ProfileErrorReason::NotANumber,
                Some(raw),
            ));
        }
        if fraction.len() > 2 {
            return Err(ProfileError::new(
                Some(field),
                ProfileErrorReason::NotHundredth,
                Some(raw),
            ));
        }
    }

    let whole: i64 = whole.parse().map_err(|_| {
        ProfileError::new(Some(field), ProfileErrorReason::OutOfRange, Some(raw))
    })?;
    let fraction: i64 = match fraction {
        None => 0,
        Some(single) if single.len() == 1 => single.parse::<i64>().unwrap_or(0) * 10,
        Some(pair) => pair.parse::<i64>().unwrap_or(0),
    };

    let magnitude = whole
        .checked_mul(100)
        .and_then(|value| value.checked_add(fraction))
        .ok_or_else(|| ProfileError::new(Some(field), ProfileErrorReason::OutOfRange, Some(raw)))?;
    let signed = if negative { -magnitude } else { magnitude };
    let value = i32::try_from(signed).map_err(|_| {
        ProfileError::new(Some(field), ProfileErrorReason::OutOfRange, Some(raw))
    })?;

    Ok(Hundredths::from_x100(value))
}

/// 解析 `pitch`：必须是整数，拒绝小数、非有限值和科学计数。
pub fn parse_pitch(raw: &str) -> ProfileResult<i32> {
    const FIELD: &'static str = "pitch";
    if raw.trim().is_empty() {
        return Err(ProfileError::new(Some(FIELD), ProfileErrorReason::Missing, None));
    }
    if is_non_finite_literal(raw) {
        return Err(ProfileError::new(
            Some(FIELD),
            ProfileErrorReason::NotFinite,
            Some(raw),
        ));
    }

    let digits = raw.strip_prefix('-').or_else(|| raw.strip_prefix('+'));
    let (negative, digits) = match digits {
        Some(digits) => (raw.starts_with('-'), digits),
        None => (false, raw),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        let reason = if digits.contains('.') {
            ProfileErrorReason::NotAnInteger
        } else {
            ProfileErrorReason::NotANumber
        };
        return Err(ProfileError::new(Some(FIELD), reason, Some(raw)));
    }

    let magnitude: i64 = digits.parse().map_err(|_| {
        ProfileError::new(Some(FIELD), ProfileErrorReason::OutOfRange, Some(raw))
    })?;
    let value = if negative { -magnitude } else { magnitude };
    if !(i64::from(PITCH_MIN)..=i64::from(PITCH_MAX)).contains(&value) {
        return Err(ProfileError::new(
            Some(FIELD),
            ProfileErrorReason::OutOfRange,
            Some(raw),
        ));
    }
    Ok(value as i32)
}

/// 校验 API Key 环境变量名。只保存名字，不保存密钥。
pub fn parse_api_key_env(raw: &str) -> ProfileResult<String> {
    const FIELD: &'static str = "api_key_env";
    if raw.trim().is_empty() {
        return Err(ProfileError::new(Some(FIELD), ProfileErrorReason::Missing, None));
    }
    let mut characters = raw.chars();
    let first = characters.next().unwrap_or(' ');
    let valid_first = first.is_ascii_alphabetic() || first == '_';
    let valid_rest = characters.all(|character| character.is_ascii_alphanumeric() || character == '_');
    if !valid_first || !valid_rest {
        return Err(ProfileError::new(
            Some(FIELD),
            ProfileErrorReason::InvalidFormat,
            Some(raw),
        ));
    }
    Ok(raw.to_string())
}

/// 精确字符串字段校验：不能为空，也不能有首尾空白（避免模糊匹配出不存在的身份）。
fn parse_exact_string(raw: &str, field: &'static str) -> ProfileResult<String> {
    if raw.trim().is_empty() {
        return Err(ProfileError::new(Some(field), ProfileErrorReason::Missing, None));
    }
    if raw.trim() != raw {
        return Err(ProfileError::new(
            Some(field),
            ProfileErrorReason::InvalidFormat,
            Some(raw),
        ));
    }
    Ok(raw.to_string())
}

/// Voice Profile 的语音可用性验证状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    /// 已对照当前 Voice Catalog 验证。
    Verified,
    /// 本地合法但可用性未验证，不能授权 Speech Attempt。
    Unverified,
}

impl VerificationStatus {
    /// 机器可读的稳定字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Unverified => "unverified",
        }
    }
}

/// Voice Profile 的验证记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileVerification {
    /// 验证状态。
    pub status: VerificationStatus,
    /// 验证时间（RFC 3339）；只有 `verified` 才有值。
    pub verified_at: Option<String>,
}

impl ProfileVerification {
    /// 未验证。
    pub fn unverified() -> Self {
        Self {
            status: VerificationStatus::Unverified,
            verified_at: None,
        }
    }

    /// 已验证，并记录验证时间。
    pub fn verified(verified_at: &str) -> Self {
        Self {
            status: VerificationStatus::Verified,
            verified_at: Some(verified_at.to_string()),
        }
    }
}

impl Default for ProfileVerification {
    fn default() -> Self {
        Self::unverified()
    }
}

/// 首版固定的音频规格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSettings {
    /// 容器/编码格式。
    pub format: String,
    /// 采样率（Hz）。
    pub sample_rate: u32,
    /// 码率（bps）。
    pub bitrate: u32,
    /// 声道数。
    pub channel: u32,
}

impl AudioSettings {
    /// 首版唯一允许的规格。
    pub fn v1() -> Self {
        Self {
            format: AUDIO_FORMAT.to_string(),
            sample_rate: AUDIO_SAMPLE_RATE,
            bitrate: AUDIO_BITRATE,
            channel: AUDIO_CHANNEL,
        }
    }
}

/// 全局 Voice Profile。不含任何秘密字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceProfile {
    /// Speech Provider 标识。
    pub provider: String,
    /// 供应商模型。
    pub model: String,
    /// 已解析的具体音色 ID。
    pub voice_id: String,
    /// provider 拥有的展示标签，不进入请求。
    pub emotion_label: Option<String>,
    /// 同 [`VoiceProfile::emotion_label`]。
    pub style_label: Option<String>,
    /// 语速，百分之一单位。
    pub speed: Hundredths,
    /// 音量，百分之一单位；adapter 映射为 SenseAudio 的 `vol`。
    pub volume: Hundredths,
    /// 声调。
    pub pitch: i32,
    /// 音频规格。
    pub audio: AudioSettings,
    /// 可用性验证状态。
    pub verification: ProfileVerification,
}

impl Default for VoiceProfile {
    fn default() -> Self {
        Self {
            provider: SENSEAUDIO_PROVIDER.to_string(),
            model: DEFAULT_MODEL.to_string(),
            voice_id: DEFAULT_VOICE_ID.to_string(),
            emotion_label: None,
            style_label: None,
            speed: Hundredths::from_x100(100),
            volume: Hundredths::from_x100(100),
            pitch: 0,
            audio: AudioSettings::v1(),
            verification: ProfileVerification::unverified(),
        }
    }
}

impl VoiceProfile {
    /// 本地结构、范围与固定音频规格校验。不访问网络，也不读写磁盘。
    pub fn validate(&self) -> ProfileResult<()> {
        if self.provider != SENSEAUDIO_PROVIDER {
            return Err(ProfileError::new(
                Some("provider"),
                ProfileErrorReason::UnsupportedProvider,
                Some(&self.provider),
            ));
        }
        parse_exact_string(&self.model, "model")?;
        parse_exact_string(&self.voice_id, "voice_id")?;

        for label in [&self.emotion_label, &self.style_label] {
            if let Some(label) = label {
                parse_exact_string(label, "emotion_label")?;
            }
        }

        if !(SPEED_MIN_X100..=SPEED_MAX_X100).contains(&self.speed.x100()) {
            return Err(ProfileError::new(
                Some("speed"),
                ProfileErrorReason::OutOfRange,
                Some(&self.speed.to_string()),
            ));
        }
        if !(VOLUME_MIN_X100..=VOLUME_MAX_X100).contains(&self.volume.x100()) {
            return Err(ProfileError::new(
                Some("volume"),
                ProfileErrorReason::OutOfRange,
                Some(&self.volume.to_string()),
            ));
        }
        if !(PITCH_MIN..=PITCH_MAX).contains(&self.pitch) {
            return Err(ProfileError::new(
                Some("pitch"),
                ProfileErrorReason::OutOfRange,
                Some(&self.pitch.to_string()),
            ));
        }
        if self.audio != AudioSettings::v1() {
            return Err(ProfileError::new(
                Some("audio"),
                ProfileErrorReason::UnsupportedAudioSetting,
                None,
            ));
        }

        Ok(())
    }
}

/// 命令行传入的 Profile 覆盖项。`None` 表示沿用当前 Profile 的值。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileDraft {
    /// 覆盖 provider。
    pub provider: Option<String>,
    /// 覆盖 model。
    pub model: Option<String>,
    /// 覆盖音色 ID；`set` 要求必填。
    pub voice_id: Option<String>,
    /// 覆盖情感展示标签。
    pub emotion_label: Option<String>,
    /// 覆盖风格展示标签。
    pub style_label: Option<String>,
    /// 覆盖语速（原始十进制文本）。
    pub speed: Option<String>,
    /// 覆盖音量（原始十进制文本）。
    pub volume: Option<String>,
    /// 覆盖声调（原始整数文本）。
    pub pitch: Option<String>,
    /// 覆盖 API Key 环境变量名。
    pub api_key_env: Option<String>,
}

/// 把覆盖项合并进当前 Profile 并做本地校验。
///
/// 这里刻意不继承任何已验证状态：本切片无法检查 provider 可用性，
/// 验证结果由 [`crate::speech::catalog`] 的验证步骤决定，避免 `set` 替用户宣称可用。
pub fn resolve_profile(current: &VoiceProfile, draft: &ProfileDraft) -> ProfileResult<VoiceProfile> {
    let provider = match draft.provider {
        Some(ref provider) => parse_exact_string(provider, "provider")?,
        None => current.provider.clone(),
    };
    let model = match draft.model {
        Some(ref model) => parse_exact_string(model, "model")?,
        None => current.model.clone(),
    };
    let voice_id = match draft.voice_id {
        Some(ref voice_id) => parse_exact_string(voice_id, "voice_id")?,
        None => {
            return Err(ProfileError::new(
                Some("voice_id"),
                ProfileErrorReason::Missing,
                None,
            ))
        }
    };
    let emotion_label = match draft.emotion_label {
        Some(ref label) => Some(parse_exact_string(label, "emotion_label")?),
        None => current.emotion_label.clone(),
    };
    let style_label = match draft.style_label {
        Some(ref label) => Some(parse_exact_string(label, "style_label")?),
        None => current.style_label.clone(),
    };
    let speed = match draft.speed {
        Some(ref raw) => parse_hundredths(raw, "speed")?,
        None => current.speed,
    };
    let volume = match draft.volume {
        Some(ref raw) => parse_hundredths(raw, "volume")?,
        None => current.volume,
    };
    let pitch = match draft.pitch {
        Some(ref raw) => parse_pitch(raw)?,
        None => current.pitch,
    };

    let resolved = VoiceProfile {
        provider,
        model,
        voice_id,
        emotion_label,
        style_label,
        speed,
        volume,
        pitch,
        audio: AudioSettings::v1(),
        verification: ProfileVerification::unverified(),
    };
    resolved.validate()?;
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_follows_adr_0007() {
        let profile = VoiceProfile::default();

        assert_eq!(profile.provider, SENSEAUDIO_PROVIDER);
        assert_eq!(profile.model, DEFAULT_MODEL);
        assert_eq!(profile.voice_id, DEFAULT_VOICE_ID);
        assert_eq!(profile.emotion_label, None);
        assert_eq!(profile.style_label, None);
        assert_eq!(profile.speed, Hundredths::from_x100(100));
        assert_eq!(profile.volume, Hundredths::from_x100(100));
        assert_eq!(profile.pitch, 0);
        assert_eq!(profile.audio, AudioSettings::v1());
        assert_eq!(profile.verification.status, VerificationStatus::Unverified);
        assert_eq!(profile.verification.verified_at, None);
        profile.validate().expect("default profile is locally valid");
    }

    #[test]
    fn parses_exact_decimal_text_into_hundredths() {
        let cases = [
            ("0.01", 1),
            ("0.1", 10),
            ("0.29", 29),
            ("0.5", 50),
            ("1", 100),
            ("1.0", 100),
            ("1.00", 100),
            ("1.01", 101),
            ("1.25", 125),
            ("2", 200),
            ("2.0", 200),
            ("10", 1000),
            ("10.0", 1000),
            ("0.29", 29),
            ("+1.5", 150),
        ];

        for (raw, expected) in cases {
            let parsed = parse_hundredths(raw, "speed").expect(raw);
            assert_eq!(parsed.x100(), expected, "{raw} must parse exactly");
        }
    }

    #[test]
    fn rejects_rounding_non_finite_and_malformed_text() {
        let reasons = [
            ("1.005", ProfileErrorReason::NotHundredth),
            ("0.290", ProfileErrorReason::NotHundredth),
            ("1.0050", ProfileErrorReason::NotHundredth),
            ("NaN", ProfileErrorReason::NotFinite),
            ("nan", ProfileErrorReason::NotFinite),
            ("-nan", ProfileErrorReason::NotFinite),
            ("+NaN", ProfileErrorReason::NotFinite),
            ("inf", ProfileErrorReason::NotFinite),
            ("-inf", ProfileErrorReason::NotFinite),
            ("+INF", ProfileErrorReason::NotFinite),
            ("Infinity", ProfileErrorReason::NotFinite),
            ("-Infinity", ProfileErrorReason::NotFinite),
            ("1e1", ProfileErrorReason::NotANumber),
            ("1E0", ProfileErrorReason::NotANumber),
            ("1e-2", ProfileErrorReason::NotANumber),
            ("0x10", ProfileErrorReason::NotANumber),
            ("0b1", ProfileErrorReason::NotANumber),
            ("1,5", ProfileErrorReason::NotANumber),
            ("1.2.3", ProfileErrorReason::NotANumber),
            ("1.", ProfileErrorReason::NotANumber),
            (".5", ProfileErrorReason::NotANumber),
            ("--1", ProfileErrorReason::NotANumber),
            ("1 2", ProfileErrorReason::NotANumber),
            ("", ProfileErrorReason::Missing),
            (" ", ProfileErrorReason::Missing),
        ];

        for (raw, reason) in reasons {
            let error = parse_hundredths(raw, "speed").expect_err(raw);
            assert_eq!(error.reason, reason, "unexpected reason for {raw:?}");
            assert_eq!(error.field, Some("speed"));
        }
    }

    #[test]
    fn rejects_values_that_do_not_fit_in_an_i32() {
        let huge = "999999999999999999999999.99";
        let error = parse_hundredths(huge, "volume").expect_err(huge);
        assert_eq!(error.reason, ProfileErrorReason::OutOfRange);
    }

    #[test]
    fn hundredths_text_and_json_stay_exact() {
        let cases = [
            (1, "0.01"),
            (10, "0.1"),
            (29, "0.29"),
            (50, "0.5"),
            (100, "1.0"),
            (101, "1.01"),
            (125, "1.25"),
            (200, "2.0"),
            (1000, "10.0"),
        ];

        for (x100, expected) in cases {
            let value = Hundredths::from_x100(x100);
            assert_eq!(value.to_string(), expected, "human text for {x100}");
            assert_eq!(
                serde_json::to_string(&value).expect("serialize hundredths"),
                expected,
                "JSON number for {x100} must be identical to the exact text"
            );
        }

        // 全量负向控制：每个合法百分位都必须能通过 JSON 文本精确往返，
        // 不能出现 f64 四舍五入制造“显示相同、内部不同”的数值。
        for x100 in 1..=1200 {
            let value = Hundredths::from_x100(x100);
            let text = serde_json::to_string(&value).expect("serialize hundredths");
            let fraction = text.split('.').nth(1).expect("decimal point in JSON number");
            assert!(
                fraction.len() <= 2,
                "JSON {text} for {x100} exposes more than two decimal places"
            );
            let round_tripped = parse_hundredths(&text, "speed").expect(&text);
            assert_eq!(round_tripped, value, "JSON {text} did not round-trip exactly");
        }
    }

    #[test]
    fn validates_speed_and_volume_ranges() {
        for (field, raw, expected) in [
            ("speed", "0.49", ProfileErrorReason::OutOfRange),
            ("speed", "2.01", ProfileErrorReason::OutOfRange),
            ("speed", "0", ProfileErrorReason::OutOfRange),
            ("volume", "0", ProfileErrorReason::OutOfRange),
            ("volume", "10.01", ProfileErrorReason::OutOfRange),
        ] {
            let mut profile = VoiceProfile::default();
            if field == "speed" {
                profile.speed = parse_hundredths(raw, field).expect(raw);
            } else {
                profile.volume = parse_hundredths(raw, field).expect(raw);
            }
            let error = profile.validate().expect_err(raw);
            assert_eq!(error.field, Some(field));
            assert_eq!(error.reason, expected);
        }

        for (field, raw) in [("speed", "0.5"), ("speed", "2.0"), ("volume", "0.01"), ("volume", "10.0")] {
            let mut profile = VoiceProfile::default();
            if field == "speed" {
                profile.speed = parse_hundredths(raw, field).expect(raw);
            } else {
                profile.volume = parse_hundredths(raw, field).expect(raw);
            }
            profile
                .validate()
                .unwrap_or_else(|error| panic!("{raw} must be accepted: {error:?}"));
        }
    }

    #[test]
    fn validates_pitch_range() {
        for pitch in [-13, 13, 100] {
            let mut profile = VoiceProfile::default();
            profile.pitch = pitch;
            let error = profile.validate().expect_err(&pitch.to_string());
            assert_eq!(error.field, Some("pitch"));
            assert_eq!(error.reason, ProfileErrorReason::OutOfRange);
        }

        for pitch in [-12, 0, 12] {
            let mut profile = VoiceProfile::default();
            profile.pitch = pitch;
            profile.validate().expect("pitch boundary is valid");
        }
    }

    #[test]
    fn parses_pitch_as_an_integer_only() {
        assert_eq!(parse_pitch("-12").expect("-12"), -12);
        assert_eq!(parse_pitch("12").expect("12"), 12);
        assert_eq!(parse_pitch("+3").expect("+3"), 3);

        for (raw, reason) in [
            ("1.5", ProfileErrorReason::NotAnInteger),
            ("0.0", ProfileErrorReason::NotAnInteger),
            ("NaN", ProfileErrorReason::NotFinite),
            ("-nan", ProfileErrorReason::NotFinite),
            ("inf", ProfileErrorReason::NotFinite),
            ("+inf", ProfileErrorReason::NotFinite),
            ("abc", ProfileErrorReason::NotANumber),
            ("1e1", ProfileErrorReason::NotANumber),
            ("", ProfileErrorReason::Missing),
        ] {
            let error = parse_pitch(raw).expect_err(raw);
            assert_eq!(error.reason, reason, "unexpected reason for {raw:?}");
            assert_eq!(error.field, Some("pitch"));
        }

        let error = parse_pitch("13").expect_err("13");
        assert_eq!(error.reason, ProfileErrorReason::OutOfRange);
    }

    #[test]
    fn resolve_requires_an_exact_voice_id() {
        let current = VoiceProfile::default();

        let missing = resolve_profile(&current, &ProfileDraft::default()).expect_err("missing");
        assert_eq!(missing.field, Some("voice_id"));
        assert_eq!(missing.reason, ProfileErrorReason::Missing);

        for padded in [" male_0004_a", "male_0004_a ", ""] {
            let draft = ProfileDraft {
                voice_id: Some(padded.to_string()),
                ..ProfileDraft::default()
            };
            let error = resolve_profile(&current, &draft).expect_err(padded);
            assert_eq!(error.field, Some("voice_id"));
        }

        let draft = ProfileDraft {
            voice_id: Some("female_0007_b".to_string()),
            ..ProfileDraft::default()
        };
        let resolved = resolve_profile(&current, &draft).expect("exact id");
        assert_eq!(resolved.voice_id, "female_0007_b");
    }

    #[test]
    fn resolve_keeps_current_values_for_unspecified_fields() {
        let current = VoiceProfile::default();
        let draft = ProfileDraft {
            voice_id: Some("female_0007_b".to_string()),
            speed: Some("1.25".to_string()),
            pitch: Some("-2".to_string()),
            emotion_label: Some("平稳".to_string()),
            ..ProfileDraft::default()
        };

        let resolved = resolve_profile(&current, &draft).expect("resolved profile");
        assert_eq!(resolved.provider, SENSEAUDIO_PROVIDER);
        assert_eq!(resolved.model, DEFAULT_MODEL);
        assert_eq!(resolved.speed, Hundredths::from_x100(125));
        assert_eq!(resolved.volume, Hundredths::from_x100(100), "volume keeps its current value");
        assert_eq!(resolved.pitch, -2);
        assert_eq!(resolved.emotion_label.as_deref(), Some("平稳"));
        assert_eq!(resolved.style_label, None);
        assert_eq!(resolved.audio, AudioSettings::v1());
    }

    #[test]
    fn resolve_never_claims_verification_by_itself() {
        let mut current = VoiceProfile::default();
        current.verification = ProfileVerification::verified("2026-09-11T00:00:00Z");

        let draft = ProfileDraft {
            voice_id: Some(DEFAULT_VOICE_ID.to_string()),
            ..ProfileDraft::default()
        };
        let resolved = resolve_profile(&current, &draft).expect("resolved profile");

        assert_eq!(
            resolved.verification.status,
            VerificationStatus::Unverified,
            "set must not inherit an unverified-able claim from stored state"
        );
        assert_eq!(resolved.verification.verified_at, None);
    }

    #[test]
    fn resolve_rejects_unsupported_provider_and_empty_model() {
        let current = VoiceProfile::default();

        let provider = resolve_profile(
            &current,
            &ProfileDraft {
                voice_id: Some(DEFAULT_VOICE_ID.to_string()),
                provider: Some("openai".to_string()),
                ..ProfileDraft::default()
            },
        )
        .expect_err("provider");
        assert_eq!(provider.field, Some("provider"));
        assert_eq!(provider.reason, ProfileErrorReason::UnsupportedProvider);

        let model = resolve_profile(
            &current,
            &ProfileDraft {
                voice_id: Some(DEFAULT_VOICE_ID.to_string()),
                model: Some("   ".to_string()),
                ..ProfileDraft::default()
            },
        )
        .expect_err("model");
        assert_eq!(model.field, Some("model"));
        assert_eq!(model.reason, ProfileErrorReason::Missing);

        // 带首尾空白的模型名必须被拒绝，而不是被静默 trim 成另一个身份。
        let padded_model = resolve_profile(
            &current,
            &ProfileDraft {
                voice_id: Some(DEFAULT_VOICE_ID.to_string()),
                model: Some(" sensenova-tts-2.0".to_string()),
                ..ProfileDraft::default()
            },
        )
        .expect_err("padded model");
        assert_eq!(padded_model.field, Some("model"));
        assert_eq!(padded_model.reason, ProfileErrorReason::InvalidFormat);
    }

    #[test]
    fn reject_labels_that_are_provided_but_empty() {
        for field in ["emotion_label", "style_label"] {
            let draft = if field == "emotion_label" {
                ProfileDraft {
                    voice_id: Some(DEFAULT_VOICE_ID.to_string()),
                    emotion_label: Some("  ".to_string()),
                    ..ProfileDraft::default()
                }
            } else {
                ProfileDraft {
                    voice_id: Some(DEFAULT_VOICE_ID.to_string()),
                    style_label: Some(String::new()),
                    ..ProfileDraft::default()
                }
            };
            let error = resolve_profile(&VoiceProfile::default(), &draft).expect_err(field);
            assert_eq!(error.field, Some(field));
            assert_eq!(error.reason, ProfileErrorReason::Missing);
        }
    }

    #[test]
    fn validates_api_key_env_name() {
        for valid in ["SENSEAUDIO_API_KEY", "_X", "A1"] {
            assert_eq!(parse_api_key_env(valid).expect(valid), valid);
        }

        for invalid in ["", " ", "1KEY", "NOT A NAME", "KEY-NAME", "KEY=1", "KEY\n"] {
            let error = parse_api_key_env(invalid).expect_err(invalid);
            assert_eq!(error.field, Some("api_key_env"));
            assert!(matches!(
                error.reason,
                ProfileErrorReason::Missing | ProfileErrorReason::InvalidFormat
            ));
        }
    }

    #[test]
    fn stored_profiles_must_keep_the_fixed_audio_settings() {
        let mut profile = VoiceProfile::default();
        profile.audio.sample_rate = 44_100;

        let error = profile.validate().expect_err("44100 is not part of v1");
        assert_eq!(error.field, Some("audio"));
        assert_eq!(error.reason, ProfileErrorReason::UnsupportedAudioSetting);
    }

    #[test]
    fn error_reasons_have_stable_machine_strings() {
        let cases = [
            (ProfileErrorReason::Missing, "missing"),
            (ProfileErrorReason::NotFinite, "not_finite"),
            (ProfileErrorReason::NotANumber, "not_a_number"),
            (ProfileErrorReason::NotHundredth, "not_hundredth"),
            (ProfileErrorReason::NotAnInteger, "not_an_integer"),
            (ProfileErrorReason::OutOfRange, "out_of_range"),
            (ProfileErrorReason::InvalidFormat, "invalid_format"),
            (ProfileErrorReason::UnsupportedProvider, "unsupported_provider"),
            (ProfileErrorReason::UnsupportedAudioSetting, "unsupported_audio_setting"),
            (ProfileErrorReason::StoredConfigInvalid, "stored_config_invalid"),
        ];

        for (reason, expected) in cases {
            assert_eq!(reason.as_str(), expected);
        }
    }
}
