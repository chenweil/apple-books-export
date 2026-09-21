//! Speech Clip 领域纯函数：内容种类、Speech Text 规范化、provider 安全控制标记转义、
//! 规范化 fingerprint / clip ID 与计费字符估算（ADR 0007 第 4 节）。
//!
//! 本模块不做网络、不碰磁盘、不读取 Apple Books。它只把「已经取到的 Annotation 文本」
//! 与「已解析的 Voice Profile」变成稳定的内容身份，供生成 use case 与测试复用。

use serde::Serialize;
use sha2::{Digest, Sha256};

/// 单个 Speech Text 的 provider 字符上限。超过即 `SPEECH_TEXT_TOO_LONG`，不截断、不总结。
pub const SPEECH_TEXT_LIMIT: usize = 10_000;

/// fingerprint payload 的版本。任何影响 provider payload 的字段变化都要升级它。
pub const FINGERPRINT_VERSION: u8 = 1;
/// Speech Text 规范化 / 控制标记 escape 规则的版本。规则变化必须升级它。
pub const SPEECH_TEXT_POLICY_VERSION: u8 = 1;
/// SenseAudio 首版计费字符估算器版本；字段名保留「estimated」，不当成最终账单。
pub const BILLING_ESTIMATOR_VERSION: &str = "senseaudio-docs-2026-09-10";

/// 一个 Speech Clip 只朗读 Annotation 的一个内容部分。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechContentKind {
    /// 只读取 `Annotation.selected_text`。
    Highlight,
    /// 只读取 `Annotation.note`。
    Note,
}

impl SpeechContentKind {
    /// 稳定的机器可读取值：`highlight` / `note`。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Highlight => "highlight",
            Self::Note => "note",
        }
    }

    /// 解析内容种类；只接受精确的 `highlight` / `note`，不做模糊匹配。
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "highlight" => Some(Self::Highlight),
            "note" => Some(Self::Note),
            _ => None,
        }
    }

    /// 从 Annotation 中取该内容部分对应的原始字段。
    pub fn select<'a>(self, selected_text: Option<&'a str>, note: Option<&'a str>) -> Option<&'a str> {
        match self {
            Self::Highlight => selected_text,
            Self::Note => note,
        }
    }
}

/// Speech Text 规范化失败的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechTextError {
    /// 规范化后的 Unicode 标量数超过 [`SPEECH_TEXT_LIMIT`]。
    TooLong {
        /// 规范化后的字符数。
        characters: usize,
    },
}

/// 规范化 Speech Text，顺序固定：
/// 1. `CRLF` 与单独 `CR` 统一成 `LF`；
/// 2. 删除整个字符串边界的 Unicode whitespace；
/// 3. 保留内部空格、换行与段落；
/// 4. 不删除 URL、emoji、标点、括号、代码或脚注；不翻译、不改写、不总结。
pub fn normalize_speech_text(raw: &str) -> String {
    let unified = raw.replace("\r\n", "\n").replace('\r', "\n");
    unified.trim_matches(char::is_whitespace).to_string()
}

/// 规范化后校验长度上限；超过返回 [`SpeechTextError::TooLong`]，绝不截断或总结。
pub fn validate_speech_text_length(normalized: &str) -> Result<(), SpeechTextError> {
    let characters = normalized.chars().count();
    if characters > SPEECH_TEXT_LIMIT {
        return Err(SpeechTextError::TooLong { characters });
    }
    Ok(())
}

/// 把 Annotation 文本里的 provider 控制标记转成普通内容。
///
/// SenseAudio 支持 `<break time=...>` 等控制语法。Annotation 是普通文本，其中的
/// `<tag>` 不得直接控制请求。这里在每个「看起来像标签起点」的 ASCII `<` 后插入一个
/// U+200B（零宽空格）作为 guard，使 `<break ...>` 失效为普通字符，同时不删除任何原文。
/// 产品明确创建的控制标记（首版不创建）才会真正生效。
pub fn escape_control_markup(normalized: &str) -> String {
    let mut out = String::with_capacity(normalized.len());
    let mut chars = normalized.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '<' {
            if let Some(&next) = chars.peek() {
                if next.is_ascii_alphabetic() || next == '/' {
                    out.push('<');
                    out.push('\u{200B}');
                    continue;
                }
            }
        }
        out.push(character);
    }
    out
}

/// 规范化文本的 Unicode 标量计数。
pub fn unicode_characters(normalized: &str) -> u64 {
    normalized.chars().count() as u64
}

/// 按 SenseAudio 官方「按万字符」规则的**估算**计费字符数：
/// 一个汉字计 2，英文字母、标点、空格等计 1。仅供估算，不是最终账单。
pub fn estimate_billing_characters(normalized: &str) -> u64 {
    normalized
        .chars()
        .map(|character| if is_han_ideograph(character) { 2 } else { 1 })
        .sum()
}

fn is_han_ideograph(character: char) -> bool {
    matches!(character as u32,
        0x4E00..=0x9FFF    // CJK Unified Ideographs
        | 0x3400..=0x4DBF  // CJK Unified Ideographs Extension A
        | 0x20000..=0x2A6DF // CJK Unified Ideographs Extension B
        | 0xF900..=0xFAFF) // CJK Compatibility Ideographs
}

/// 规范化文本的 SHA-256 小写 hex。缓存、receipt、attempt 只保存这个，不保存原文。
pub fn text_sha256(normalized: &str) -> String {
    hex_sha256(normalized.as_bytes())
}

/// SHA-256 小写 hex。
pub fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// 进入 fingerprint 的全部 provider-effective 输入。字段顺序固定，序列化时逐字段长度分隔，
/// 因此任意字段内含分隔符都不会造成歧义或碰撞。
#[derive(Debug, Clone)]
pub struct ClipFingerprint<'a> {
    /// Speech Provider。
    pub provider: &'a str,
    /// 供应商模型。
    pub model: &'a str,
    /// 书籍稳定 ID。
    pub asset_id: &'a str,
    /// Annotation 稳定 ID。
    pub annotation_id: &'a str,
    /// 内容种类。
    pub content_kind: SpeechContentKind,
    /// 规范化 Speech Text 快照。
    pub normalized_text: &'a str,
    /// 已解析的具体音色 ID。
    pub voice_id: &'a str,
    /// 语速，百分之一单位。
    pub speed_x100: i32,
    /// 音量，百分之一单位。
    pub volume_x100: i32,
    /// 声调。
    pub pitch: i32,
    /// 音频格式。
    pub audio_format: &'a str,
    /// 采样率。
    pub sample_rate: u32,
    /// 码率。
    pub bitrate: u32,
    /// 声道数。
    pub channel: u32,
}

impl<'a> ClipFingerprint<'a> {
    /// 规范化、版本化、字段顺序固定的字节编码。
    fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"speech-clip-fingerprint");
        push_field(&mut out, b"fingerprint_version", &[FINGERPRINT_VERSION]);
        push_field(
            &mut out,
            b"speech_text_policy_version",
            &[SPEECH_TEXT_POLICY_VERSION],
        );
        push_field(&mut out, b"provider", self.provider.as_bytes());
        push_field(&mut out, b"model", self.model.as_bytes());
        push_field(&mut out, b"asset_id", self.asset_id.as_bytes());
        push_field(&mut out, b"annotation_id", self.annotation_id.as_bytes());
        push_field(
            &mut out,
            b"content_kind",
            self.content_kind.as_str().as_bytes(),
        );
        push_field(&mut out, b"normalized_text", self.normalized_text.as_bytes());
        push_field(&mut out, b"voice_id", self.voice_id.as_bytes());
        push_field(&mut out, b"speed_x100", &self.speed_x100.to_be_bytes());
        push_field(&mut out, b"volume_x100", &self.volume_x100.to_be_bytes());
        push_field(&mut out, b"pitch", &self.pitch.to_be_bytes());
        push_field(&mut out, b"audio_format", self.audio_format.as_bytes());
        push_field(&mut out, b"sample_rate", &self.sample_rate.to_be_bytes());
        push_field(&mut out, b"bitrate", &self.bitrate.to_be_bytes());
        push_field(&mut out, b"channel", &self.channel.to_be_bytes());
        out
    }

    /// 计算稳定 clip ID：canonical payload 的 SHA-256 小写 hex。
    pub fn clip_id(&self) -> String {
        hex_sha256(&self.to_canonical_bytes())
    }
}

/// 写入一个「tag=len:bytes\n」字段，长度分隔保证编码无歧义。
fn push_field(out: &mut Vec<u8>, tag: &[u8], value: &[u8]) {
    out.extend_from_slice(tag);
    out.push(b'=');
    out.extend_from_slice(&(value.len() as u64).to_be_bytes());
    out.push(b':');
    out.extend_from_slice(value);
    out.push(b'\n');
}

/// 生成 use case 的文本摘要，只含不含原文的计数与哈希，供 receipt / state 复用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpeechTextSummary {
    /// 规范化文本 SHA-256。
    pub text_sha256: String,
    /// 规范化文本 Unicode 标量数。
    pub unicode_characters: u64,
    /// 估算计费字符数。
    pub estimated_billing_characters: u64,
    /// 计费估算器版本。
    pub billing_estimator_version: &'static str,
}

impl SpeechTextSummary {
    /// 由规范化文本计算摘要；不保留原文。
    pub fn from_normalized(normalized: &str) -> Self {
        Self {
            text_sha256: text_sha256(normalized),
            unicode_characters: unicode_characters(normalized),
            estimated_billing_characters: estimate_billing_characters(normalized),
            billing_estimator_version: BILLING_ESTIMATOR_VERSION,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint() -> ClipFingerprint<'static> {
        ClipFingerprint {
            provider: "senseaudio",
            model: "sensenova-tts-2.0",
            asset_id: "book-1",
            annotation_id: "annotation-41",
            content_kind: SpeechContentKind::Highlight,
            normalized_text: "高亮正文",
            voice_id: "male_0004_a",
            speed_x100: 100,
            volume_x100: 100,
            pitch: 0,
            audio_format: "mp3",
            sample_rate: 32_000,
            bitrate: 128_000,
            channel: 2,
        }
    }

    #[test]
    fn content_kind_parses_only_exact_values() {
        assert_eq!(SpeechContentKind::parse("highlight"), Some(SpeechContentKind::Highlight));
        assert_eq!(SpeechContentKind::parse("note"), Some(SpeechContentKind::Note));
        assert_eq!(SpeechContentKind::parse("Highlight"), None);
        assert_eq!(SpeechContentKind::parse("both"), None);
        assert_eq!(SpeechContentKind::parse(""), None);
        assert_eq!(SpeechContentKind::Highlight.select(Some("h"), Some("n")), Some("h"));
        assert_eq!(SpeechContentKind::Note.select(Some("h"), Some("n")), Some("n"));
    }

    #[test]
    fn normalization_converts_crlf_and_lone_cr_and_trims_boundaries() {
        assert_eq!(normalize_speech_text("  hello\r\nworld  "), "hello\nworld");
        assert_eq!(normalize_speech_text("\r\n\r\nlead\r\ntrail\r\n"), "lead\ntrail");
        assert_eq!(normalize_speech_text("a\rb\rc"), "a\nb\nc");
        // 内部换行与段落保留。
        assert_eq!(normalize_speech_text("\n\npara1\n\npara2\n\n"), "para1\n\npara2");
        // 内部空格保留，URL / emoji / 标点不删。
        assert_eq!(normalize_speech_text(" see https://a.b/c 😀 (x) "), "see https://a.b/c 😀 (x)");
        // 全空白归一为空。
        assert_eq!(normalize_speech_text("   \r\n  "), "");
    }

    #[test]
    fn overlong_text_is_rejected_and_never_truncated() {
        let exactly = "a".repeat(SPEECH_TEXT_LIMIT);
        validate_speech_text_length(&exactly).expect("exactly at the limit is allowed");
        let over = "a".repeat(SPEECH_TEXT_LIMIT + 1);
        assert_eq!(
            validate_speech_text_length(&over),
            Err(SpeechTextError::TooLong {
                characters: SPEECH_TEXT_LIMIT + 1
            })
        );
        // 多字节字符按 Unicode 标点计，不按字节。
        let han = "汉".repeat(SPEECH_TEXT_LIMIT + 1);
        assert!(matches!(
            validate_speech_text_length(&han),
            Err(SpeechTextError::TooLong { .. })
        ));
    }

    #[test]
    fn control_markup_is_neutralized_with_a_zero_width_guard() {
        let escaped = escape_control_markup("before <break time=\"500\"/> after");
        assert!(escaped.contains("<\u{200B}break"));
        // 原文没有被删除。
        assert!(escaped.contains("break time=\"500\"/>"));
        assert!(escaped.contains("before "));
        assert!(escaped.contains(" after"));
        // 关闭标签同样被中和（guard 紧跟 `<`）。
        assert!(escape_control_markup("</tag>").contains("<\u{200B}/tag"));
        // 普通小于号（非标签）保持原样，不注入 guard。
        assert_eq!(escape_control_markup("a < b"), "a < b");
        assert_eq!(escape_control_markup("x<1"), "x<1");
        // 没有控制标记的文本逐字节不变。
        assert_eq!(escape_control_markup("plain text"), "plain text");
    }

    #[test]
    fn billing_estimate_counts_han_as_two_and_others_as_one() {
        assert_eq!(estimate_billing_characters("你好"), 4);
        assert_eq!(estimate_billing_characters("hi there"), 8);
        assert_eq!(estimate_billing_characters("你a"), 3);
        assert_eq!(BILLING_ESTIMATOR_VERSION, "senseaudio-docs-2026-09-10");
    }

    #[test]
    fn clip_id_is_stable_for_identical_inputs() {
        assert_eq!(fingerprint().clip_id(), fingerprint().clip_id());
        assert_eq!(fingerprint().clip_id().len(), 64);
        assert!(fingerprint().clip_id().chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn clip_id_changes_for_every_provider_effective_input() {
        let base = fingerprint().clip_id();
        // 每个 provider-effective 字段变化都必须产生新 clip ID。
        let text_change = ClipFingerprint { normalized_text: "different", ..fingerprint() }.clip_id();
        let voice_change = ClipFingerprint { voice_id: "female_0007_b", ..fingerprint() }.clip_id();
        let speed_change = ClipFingerprint { speed_x100: 101, ..fingerprint() }.clip_id();
        let volume_change = ClipFingerprint { volume_x100: 90, ..fingerprint() }.clip_id();
        let pitch_change = ClipFingerprint { pitch: 1, ..fingerprint() }.clip_id();
        let kind_change = ClipFingerprint { content_kind: SpeechContentKind::Note, ..fingerprint() }
            .clip_id();
        let annotation_change = ClipFingerprint { annotation_id: "annotation-42", ..fingerprint() }
            .clip_id();
        let asset_change = ClipFingerprint { asset_id: "book-2", ..fingerprint() }.clip_id();
        let rate_change = ClipFingerprint { sample_rate: 44_100, ..fingerprint() }.clip_id();
        let model_change = ClipFingerprint { model: "other-model", ..fingerprint() }.clip_id();
        for changed in [
            text_change,
            voice_change,
            speed_change,
            volume_change,
            pitch_change,
            kind_change,
            annotation_change,
            asset_change,
            rate_change,
            model_change,
        ] {
            assert_ne!(base, changed, "a provider-effective change must change the clip id");
        }
    }

    #[test]
    fn identical_speed_and_volume_text_map_to_the_same_clip_id() {
        // 相同的百分之一单位不得因为不同文本表示产生不同身份。
        let a = ClipFingerprint { speed_x100: 100, volume_x100: 100, ..fingerprint() }.clip_id();
        let b = ClipFingerprint { speed_x100: 100, volume_x100: 100, ..fingerprint() }.clip_id();
        assert_eq!(a, b);
    }

    #[test]
    fn field_boundaries_cannot_be_forged_by_field_content() {
        // 文本里含分隔符不得让两个不同 fingerprint 碰撞。
        let a = ClipFingerprint {
            asset_id: "book\n1",
            annotation_id: "x",
            ..fingerprint()
        }
        .clip_id();
        let b = ClipFingerprint {
            asset_id: "book",
            annotation_id: "1\nx",
            ..fingerprint()
        }
        .clip_id();
        assert_ne!(a, b, "length-delimited encoding must not be ambiguous");
    }

    #[test]
    fn text_summary_keeps_counts_and_hash_but_not_the_text() {
        let summary = SpeechTextSummary::from_normalized("你好 hi");
        assert_eq!(summary.unicode_characters, 5);
        // 你=2, 好=2, space=1, h=1, i=1 → 7.
        assert_eq!(summary.estimated_billing_characters, 7);
        assert_eq!(summary.text_sha256, text_sha256("你好 hi"));
        assert_eq!(summary.billing_estimator_version, BILLING_ESTIMATOR_VERSION);
    }
}
