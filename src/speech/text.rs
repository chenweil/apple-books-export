//! Speech Text 规范化、provider 控制标记转义与计费字符估算。
//!
//! ADR 0007 与实施 spec 4.2 固定了 Speech Text 的处理顺序：
//! 1. `CRLF` 与单独 `CR` 统一成 `LF`；
//! 2. 只删除整个字符串边界的 Unicode whitespace，内部空格、换行与段落原样保留；
//! 3. 不删除 URL、emoji、标点、括号、代码或脚注；
//! 4. 不翻译、不改写、不总结、不截断；
//! 5. Annotation 内容始终是普通文本，`<break>` 等 provider 控制语法不得生效。
//!
//! 缓存、receipt、attempt history 与 export manifest 都不保存 Speech Text，只保存
//! `text_sha256` 与字符统计。

use serde::Serialize;
use sha2::{Digest, Sha256};

/// Speech Text 的 provider 上限：超过就在联网前失败，不截断、不总结、不拆分。
pub const MAX_SPEECH_TEXT_CHARS: usize = 10_000;

/// Speech Text 规范化策略版本。规则或控制标记转义方式变化必须升级它，
/// 否则同一段文字会得到同一个 clip ID 却发出不同请求。
pub const SPEECH_TEXT_POLICY_VERSION: u32 = 1;

/// SenseAudio 计费字符估算规则的版本标识；估算值永远不是最终账单。
pub const BILLING_ESTIMATOR_VERSION: &str = "senseaudio-docs-2026-09-10";

/// U+200B：紧跟在 ASCII `<` 之后插入的零宽空格，用来中和 provider 控制标记。
///
/// SenseAudio 没有公开「普通文本里控制标记」的可靠 escape 规则，因此这里选择
/// 不删除任何字符：原文快照与指纹保持不变，只有真正发往 provider 的文本被插入
/// 这个不可见字符，`<break time=...>` 之类的语法随之失去控制效果。
pub const CONTROL_MARKUP_GUARD: char = '\u{200b}';

/// Speech Text 规范化失败。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechTextError {
    /// 规范化后的文本超过 provider 上限。
    TooLong {
        /// 规范化后的 Unicode 字符数。
        characters: usize,
    },
}

impl SpeechTextError {
    /// 稳定的 Machine JSON 错误码。
    pub const fn machine_code(&self) -> &'static str {
        match self {
            Self::TooLong { .. } => "SPEECH_TEXT_TOO_LONG",
        }
    }
}

impl std::fmt::Display for SpeechTextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLong { characters } => write!(
                f,
                "the normalized Speech Text is {characters} characters, which exceeds the provider limit of {MAX_SPEECH_TEXT_CHARS}"
            ),
        }
    }
}

/// 规范化后的 Speech Text 及其非秘密摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechText {
    /// 规范化后的完整文本；只保存在内存里，不落盘、不进 receipt。
    pub normalized: String,
    /// Unicode 字符数（本地计数）。
    pub unicode_characters: usize,
    /// 规范化文本的 SHA-256 小写 hex。
    pub text_sha256: String,
    /// 按当前 provider 规则得到的计费字符估算；只是估算，不是账单。
    pub estimated_billing_characters: usize,
}

impl SpeechText {
    /// 发往 provider 的文本：原文字符一个不减，只插入控制标记守卫。
    pub fn provider_safe_text(&self) -> String {
        escape_provider_control_markup(&self.normalized)
    }
}

/// 规范化原始 Annotation 内容并校验 provider 上限。
///
/// 失败时绝不产生任何部分结果：调用方必须在不联网的情况下返回 `SPEECH_TEXT_TOO_LONG`。
pub fn normalize_speech_text(raw: &str) -> Result<SpeechText, SpeechTextError> {
    let normalized = normalize(raw);
    let unicode_characters = normalized.chars().count();
    if unicode_characters > MAX_SPEECH_TEXT_CHARS {
        return Err(SpeechTextError::TooLong {
            characters: unicode_characters,
        });
    }
    Ok(SpeechText {
        text_sha256: sha256_hex(normalized.as_bytes()),
        estimated_billing_characters: estimate_billing_characters(&normalized),
        normalized,
        unicode_characters,
    })
}

/// 只做换行统一与边界空白删除；内部内容一个字符都不动。
pub fn normalize(raw: &str) -> String {
    let unified = if raw.contains('\r') {
        raw.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        raw.to_string()
    };
    unified
        .trim_matches(|character: char| character.is_whitespace())
        .to_string()
}

/// 把 ASCII `<` 后面的控制标记守卫插进发往 provider 的文本。
///
/// 每个 ASCII `<` 都会被插入 U+200B，因此任何 provider 控制标签都无法闭合生效；
/// 普通文本（包括 `<` 本身）在语音上保持不变，原文也从不被删除或改写。
pub fn escape_provider_control_markup(text: &str) -> String {
    if !text.contains('<') {
        return text.to_string();
    }
    let mut escaped = String::with_capacity(text.len() + text.len() / 8 + 1);
    for character in text.chars() {
        escaped.push(character);
        if character == '<' {
            escaped.push(CONTROL_MARKUP_GUARD);
        }
    }
    escaped
}

/// SenseAudio 文档的计费规则：一个汉字计 2 个字符，其余字符计 1 个。
///
/// 这只是估算，用来在生成前告诉用户规模；最终金额以供应商账单为准，
/// 程序不硬编码任何价格。
pub fn estimate_billing_characters(text: &str) -> usize {
    text.chars()
        .map(|character| usize::from(is_cjk_ideograph(character)) + 1)
        .sum()
}

/// 汉字表意文字计 2 个字符（含扩展区）。
fn is_cjk_ideograph(character: char) -> bool {
    matches!(character as u32,
        0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xF900..=0xFAFF
        | 0x20000..=0x2A6DF
        | 0x2A700..=0x2EBEF
        | 0x2F800..=0x2FA1F
        | 0x30000..=0x3134F)
}

/// 非秘密文本摘要：SHA-256 小写 hex。
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// 计费估算的版本化快照，供 receipt 与 attempt history 复用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BillingEstimate {
    /// 本地 Unicode 字符数。
    pub unicode_characters: usize,
    /// 按当前 provider 规则得到的计费字符估算。
    pub estimated_billing_characters: usize,
    /// 估算规则版本。
    pub billing_estimator_version: &'static str,
}

impl From<&SpeechText> for BillingEstimate {
    fn from(text: &SpeechText) -> Self {
        Self {
            unicode_characters: text.unicode_characters,
            estimated_billing_characters: text.estimated_billing_characters,
            billing_estimator_version: BILLING_ESTIMATOR_VERSION,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_line_endings_and_boundary_whitespace_only() {
        let normalized = normalize("  \r\n第一段\r\n\r\n第二段  \t");

        assert_eq!(normalized, "第一段\n\n第二段");
        // 内部空白与段落必须原样保留，一个字符都不能丢。
        assert_eq!(normalize("a  b\n\n\nc"), "a  b\n\n\nc");
        assert_eq!(normalize("\u{3000}文字\u{3000}"), "文字");
        assert_eq!(normalize("\u{200b}零宽\u{200b}"), "\u{200b}零宽\u{200b}");
    }

    #[test]
    fn normalization_keeps_urls_emoji_punctuation_and_newlines() {
        let raw = "见 https://example.com/a?b=1 🙂（脚注[1]）<break time=500>\n第二行";

        let text = normalize_speech_text(raw).expect("normalize");

        assert_eq!(
            text.normalized,
            "见 https://example.com/a?b=1 🙂（脚注[1]）<break time=500>\n第二行"
        );
        assert_eq!(text.unicode_characters, text.normalized.chars().count());
    }

    #[test]
    fn overlong_text_fails_before_any_provider_call() {
        let raw = "字".repeat(MAX_SPEECH_TEXT_CHARS + 1);

        let error = normalize_speech_text(&raw).expect_err("too long");

        assert_eq!(error.machine_code(), "SPEECH_TEXT_TOO_LONG");
        match error {
            SpeechTextError::TooLong { characters } => {
                assert_eq!(characters, MAX_SPEECH_TEXT_CHARS + 1)
            }
        }
        // 边界值必须仍然可用，不能被提前判为超长。
        let at_limit = "字".repeat(MAX_SPEECH_TEXT_CHARS);
        assert_eq!(
            normalize_speech_text(&at_limit)
                .expect("exactly at the limit")
                .unicode_characters,
            MAX_SPEECH_TEXT_CHARS
        );
    }

    #[test]
    fn control_markup_is_neutralized_without_deleting_any_character() {
        let text =
            normalize_speech_text("停顿<break time=500>结束 < 比较 </close>").expect("normalize");

        let provider_text = text.provider_safe_text();

        assert!(provider_text.contains(&format!("<{CONTROL_MARKUP_GUARD}break time=500>")));
        assert!(
            !provider_text.contains("<break"),
            "provider text must not contain an active control tag"
        );
        // 原文一个字符都没删：去掉守卫后必须完全等于原文快照。
        let stripped: String = provider_text
            .chars()
            .filter(|character| *character != CONTROL_MARKUP_GUARD)
            .collect();
        assert_eq!(stripped, text.normalized);
        assert_eq!(text.text_sha256, sha256_hex(text.normalized.as_bytes()));
    }

    #[test]
    fn text_without_markup_is_untouched() {
        let text = normalize_speech_text("普通文本 a < b").expect("normalize");

        assert_eq!(text.provider_safe_text(), "普通文本 a <\u{200b} b");
        assert!(normalize("没有标记").contains('<') == false);
        assert_eq!(escape_provider_control_markup("无标记"), "无标记");
    }

    #[test]
    fn billing_estimate_counts_a_han_character_as_two() {
        // 一个汉字 2，其余 1：与 SenseAudio 文档的按万字符规则一致，且只是估算。
        assert_eq!(estimate_billing_characters("你好"), 4);
        assert_eq!(estimate_billing_characters("hi"), 2);
        assert_eq!(estimate_billing_characters("你a"), 3);

        let text = normalize_speech_text("你好 world").expect("normalize");
        assert_eq!(text.unicode_characters, 8);
        // 两个汉字各计 2，空格与 5 个英文字母各计 1。
        assert_eq!(text.estimated_billing_characters, 10);
        assert_eq!(BILLING_ESTIMATOR_VERSION, "senseaudio-docs-2026-09-10");
    }

    #[test]
    fn text_sha256_is_stable_and_changes_with_content() {
        let first = normalize_speech_text("同一段文字").expect("normalize");
        let again = normalize_speech_text("  同一段文字\r\n").expect("normalize");
        let other = normalize_speech_text("另一段文字").expect("normalize");

        assert_eq!(first.text_sha256, again.text_sha256);
        assert_ne!(first.text_sha256, other.text_sha256);
        assert_eq!(first.text_sha256.len(), 64);
    }
}
