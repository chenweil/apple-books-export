//! Speech Clip 内容选择与 canonical fingerprint。
//!
//! 一个 Speech Clip 只朗读 Annotation 的一个内容部分：高亮或个人笔记。
//! Machine 调用用 `asset_id + annotation_id + content_kind` 稳定选择内容；
//! 人类 CLI 的显示序号只在本模块之外解析成同样的稳定身份。

use crate::models::Annotation;
use crate::speech::profile::AudioSettings;
use crate::speech::text::{normalize_speech_text, sha256_hex, SpeechText, SpeechTextError};
use serde::{Deserialize, Serialize};

/// `clip_id` fingerprint payload 的版本。影响 provider 请求的字段变化必须升级它。
pub const FINGERPRINT_VERSION: u32 = 1;

/// 一个 Speech Clip 朗读的内容部分。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpeechContentKind {
    /// 只读 `Annotation.selected_text`。
    Highlight,
    /// 只读 `Annotation.note`。
    Note,
}

impl SpeechContentKind {
    /// 稳定的机器可读取值。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Highlight => "highlight",
            Self::Note => "note",
        }
    }

    /// 解析 CLI 传入的 content kind；不接受的取值返回 `None`，由调用方报 `INVALID_ARGUMENT`。
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "highlight" => Some(Self::Highlight),
            "note" => Some(Self::Note),
            _ => None,
        }
    }
}

/// 内容选择失败。全部发生在任何 provider 调用之前。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeechContentError {
    /// Annotation 不存在，或不属于给定的 `asset_id`。
    InvalidAnnotationId {
        /// 请求的书籍稳定 ID。
        asset_id: String,
        /// 请求的 Annotation ID。
        annotation_id: String,
    },
    /// 目标内容部分缺失或 trim 后为空。
    ContentUnavailable {
        /// 请求的内容部分。
        content_kind: SpeechContentKind,
    },
    /// 规范化文本超过 provider 上限。
    TooLong(SpeechTextError),
}

impl SpeechContentError {
    /// 稳定的 Machine JSON 错误码。
    pub const fn machine_code(&self) -> &'static str {
        match self {
            Self::InvalidAnnotationId { .. } => "INVALID_ANNOTATION_ID",
            Self::ContentUnavailable { .. } => "SPEECH_CONTENT_UNAVAILABLE",
            Self::TooLong(error) => error.machine_code(),
        }
    }

    /// 面向人类的说明；不包含任何原文内容。
    pub fn message(&self) -> String {
        match self {
            Self::InvalidAnnotationId {
                asset_id,
                annotation_id,
            } => format!("Annotation '{annotation_id}' does not belong to asset_id '{asset_id}'."),
            Self::ContentUnavailable { content_kind } => format!(
                "This Annotation has no {} content to speak.",
                content_kind.as_str()
            ),
            Self::TooLong(error) => error.to_string(),
        }
    }
}

/// 选中后的内容身份与文本快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechContentSelection {
    /// 被选中的内容部分。
    pub content_kind: SpeechContentKind,
    /// 书籍稳定 ID（来自 Annotation 本身，不是调用方声明）。
    pub asset_id: String,
    /// Annotation 稳定 ID。
    pub annotation_id: String,
    /// 规范化后的 Speech Text 摘要。
    pub text: SpeechText,
}

/// 用稳定身份选择恰好一个内容部分。
///
/// 规则（实施 spec 4.1）：
/// - `highlight` 只读 `selected_text`，`note` 只读 `note`；
/// - 不读 `annotation_type`，也不用 Machine DTO 的 `type` 推断 content kind；
/// - `annotation_id` 不属于给定 `asset_id` 时返回 `INVALID_ANNOTATION_ID`；
/// - 目标字段缺失或 trim 后为空返回 `SPEECH_CONTENT_UNAVAILABLE`；
/// - 文本超过 10000 字符在联网前返回 `SPEECH_TEXT_TOO_LONG`。
pub fn select_speech_content(
    asset_id: &str,
    annotation_id: &str,
    content_kind: SpeechContentKind,
    annotations: &[Annotation],
) -> Result<SpeechContentSelection, SpeechContentError> {
    let annotation = annotations
        .iter()
        .find(|annotation| annotation.id == annotation_id && annotation.asset_id == asset_id)
        .ok_or_else(|| SpeechContentError::InvalidAnnotationId {
            asset_id: asset_id.to_string(),
            annotation_id: annotation_id.to_string(),
        })?;

    let raw = match content_kind {
        SpeechContentKind::Highlight => annotation.selected_text.as_deref(),
        SpeechContentKind::Note => annotation.note.as_deref(),
    }
    .filter(|raw| !raw.trim().is_empty())
    .ok_or(SpeechContentError::ContentUnavailable { content_kind })?;

    let text = normalize_speech_text(raw).map_err(SpeechContentError::TooLong)?;
    Ok(SpeechContentSelection {
        content_kind,
        asset_id: annotation.asset_id.clone(),
        annotation_id: annotation.id.clone(),
        text,
    })
}

/// canonical fingerprint payload：字段顺序固定，禁止对无序 map 直接序列化。
#[derive(Debug, Serialize)]
struct FingerprintPayload<'a> {
    fingerprint_version: u32,
    speech_text_policy_version: u32,
    provider: &'a str,
    model: &'a str,
    asset_id: &'a str,
    annotation_id: &'a str,
    content_kind: SpeechContentKind,
    normalized_speech_text: &'a str,
    voice_id: &'a str,
    speed_x100: i32,
    volume_x100: i32,
    pitch: i32,
    audio_format: &'a str,
    sample_rate: u32,
    bitrate: u32,
    channel: u32,
}

/// 进入 fingerprint 的全部 provider 有效输入。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipFingerprint<'a> {
    /// Speech Provider。
    pub provider: &'a str,
    /// 供应商模型。
    pub model: &'a str,
    /// 书籍稳定 ID。
    pub asset_id: &'a str,
    /// Annotation 稳定 ID。
    pub annotation_id: &'a str,
    /// 内容部分。
    pub content_kind: SpeechContentKind,
    /// 规范化后的 Speech Text 原文快照。
    pub normalized_speech_text: &'a str,
    /// 已解析的具体音色 ID。
    pub voice_id: &'a str,
    /// 语速的百分之一单位。
    pub speed_x100: i32,
    /// 音量的百分之一单位。
    pub volume_x100: i32,
    /// 声调。
    pub pitch: i32,
    /// 首版固定音频规格。
    pub audio: &'a AudioSettings,
}

/// 计算 canonical fingerprint payload 的 SHA-256 小写 hex，即 `clip_id`。
///
/// 情感/风格展示标签不进入 fingerprint：真正影响声音的是 resolved `voice_id`。
/// `speed`/`volume` 用百分之一整数参与，避免浮点漂移制造「显示相同、身份不同」。
pub fn clip_id(fingerprint: &ClipFingerprint<'_>) -> String {
    let payload = FingerprintPayload {
        fingerprint_version: FINGERPRINT_VERSION,
        speech_text_policy_version: crate::speech::text::SPEECH_TEXT_POLICY_VERSION,
        provider: fingerprint.provider,
        model: fingerprint.model,
        asset_id: fingerprint.asset_id,
        annotation_id: fingerprint.annotation_id,
        content_kind: fingerprint.content_kind,
        normalized_speech_text: fingerprint.normalized_speech_text,
        voice_id: fingerprint.voice_id,
        speed_x100: fingerprint.speed_x100,
        volume_x100: fingerprint.volume_x100,
        pitch: fingerprint.pitch,
        audio_format: &fingerprint.audio.format,
        sample_rate: fingerprint.audio.sample_rate,
        bitrate: fingerprint.audio.bitrate,
        channel: fingerprint.audio.channel,
    };
    let canonical = serde_json::to_vec(&payload).expect("fingerprint payload serializes");
    sha256_hex(&canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speech::profile::AudioSettings;

    fn annotation(
        id: &str,
        asset_id: &str,
        selected_text: Option<&str>,
        note: Option<&str>,
    ) -> Annotation {
        Annotation {
            id: id.to_string(),
            asset_id: asset_id.to_string(),
            selected_text: selected_text.map(str::to_string),
            note: note.map(str::to_string),
            location: Some("epubcfi(/6/2)".to_string()),
            annotation_type: 3,
            creation_date: Some(0.0),
        }
    }

    fn fixture() -> Vec<Annotation> {
        vec![
            annotation(
                "annotation-41",
                "book-1",
                Some("高亮正文"),
                Some("我的笔记"),
            ),
            annotation("annotation-42", "book-1", Some("只有高亮"), None),
            annotation("annotation-43", "book-1", None, Some("只有笔记")),
        ]
    }

    fn fingerprint(kind: SpeechContentKind) -> TestFingerprint {
        let audio = AudioSettings::v1();
        let shared: &'static AudioSettings = Box::leak(Box::new(audio.clone()));
        TestFingerprint {
            audio,
            inner: ClipFingerprint {
                provider: "senseaudio",
                model: "sensenova-tts-2.0",
                asset_id: "book-1",
                annotation_id: "annotation-41",
                content_kind: kind,
                normalized_speech_text: "高亮正文",
                voice_id: "male_0004_a",
                speed_x100: 100,
                volume_x100: 100,
                pitch: 0,
                audio: shared,
            },
        }
    }

    /// 让测试可以就地改写单个字段，同时保持 `audio` 引用有效。
    struct TestFingerprint {
        audio: AudioSettings,
        inner: ClipFingerprint<'static>,
    }

    impl TestFingerprint {
        fn with(mut self, changed: impl FnOnce(&mut ClipFingerprint<'static>)) -> Self {
            changed(&mut self.inner);
            self
        }

        fn with_audio(mut self, audio: AudioSettings) -> Self {
            self.inner.audio = Box::leak(Box::new(audio.clone()));
            self.audio = audio;
            self
        }

        fn clip_id(&self) -> String {
            clip_id(&self.inner)
        }
    }

    #[test]
    fn content_kind_parses_only_the_two_supported_values() {
        assert_eq!(
            SpeechContentKind::parse("highlight"),
            Some(SpeechContentKind::Highlight)
        );
        assert_eq!(
            SpeechContentKind::parse("note"),
            Some(SpeechContentKind::Note)
        );
        for invalid in ["", "Highlight", "both", "annotation", "note "] {
            assert_eq!(
                SpeechContentKind::parse(invalid),
                None,
                "{invalid} must be rejected"
            );
        }
    }

    #[test]
    fn highlight_and_note_of_one_annotation_are_two_distinct_clips() {
        let annotations = fixture();

        let highlight = select_speech_content(
            "book-1",
            "annotation-41",
            SpeechContentKind::Highlight,
            &annotations,
        )
        .expect("highlight");
        let note = select_speech_content(
            "book-1",
            "annotation-41",
            SpeechContentKind::Note,
            &annotations,
        )
        .expect("note");

        assert_eq!(highlight.text.normalized, "高亮正文");
        assert_eq!(note.text.normalized, "我的笔记");
        assert_ne!(highlight.text.text_sha256, note.text.text_sha256);
        assert_ne!(
            fingerprint(SpeechContentKind::Highlight).clip_id(),
            fingerprint(SpeechContentKind::Note).clip_id()
        );
    }

    #[test]
    fn wrong_annotation_ownership_fails_before_any_provider_call() {
        let annotations = fixture();

        let error = select_speech_content(
            "other-book",
            "annotation-41",
            SpeechContentKind::Highlight,
            &annotations,
        )
        .expect_err("foreign annotation");

        assert_eq!(error.machine_code(), "INVALID_ANNOTATION_ID");
        let unknown = select_speech_content(
            "book-1",
            "annotation-999",
            SpeechContentKind::Highlight,
            &annotations,
        )
        .expect_err("unknown annotation");
        assert_eq!(unknown.machine_code(), "INVALID_ANNOTATION_ID");
    }

    /// 负向控制：note-only / highlight-only fixture 请求不存在的另一侧。
    #[test]
    fn requesting_the_absent_side_is_content_unavailable() {
        let annotations = fixture();

        for (annotation_id, kind) in [
            ("annotation-42", SpeechContentKind::Note),
            ("annotation-43", SpeechContentKind::Highlight),
        ] {
            let error = select_speech_content("book-1", annotation_id, kind, &annotations)
                .expect_err("missing side");
            assert_eq!(error.machine_code(), "SPEECH_CONTENT_UNAVAILABLE");
        }
    }

    #[test]
    fn whitespace_only_content_is_content_unavailable() {
        let annotations = vec![annotation("annotation-1", "book-1", Some("   \n\t "), None)];

        let error = select_speech_content(
            "book-1",
            "annotation-1",
            SpeechContentKind::Highlight,
            &annotations,
        )
        .expect_err("blank highlight");

        assert_eq!(error.machine_code(), "SPEECH_CONTENT_UNAVAILABLE");
    }

    #[test]
    fn overlong_highlight_fails_before_any_provider_call() {
        let raw = "字".repeat(crate::speech::text::MAX_SPEECH_TEXT_CHARS + 1);
        let annotations = vec![annotation("annotation-1", "book-1", Some(&raw), None)];

        let error = select_speech_content(
            "book-1",
            "annotation-1",
            SpeechContentKind::Highlight,
            &annotations,
        )
        .expect_err("too long");

        assert_eq!(error.machine_code(), "SPEECH_TEXT_TOO_LONG");
    }

    #[test]
    fn clip_id_is_stable_for_identical_input() {
        let id = fingerprint(SpeechContentKind::Highlight).clip_id();

        assert_eq!(fingerprint(SpeechContentKind::Highlight).clip_id(), id);
        assert_eq!(id.len(), 64, "clip_id is a lowercase sha256 hex digest");
        assert!(id.chars().all(|character| character.is_ascii_hexdigit()));
        assert!(!id.chars().any(|character| character.is_ascii_uppercase()));
    }

    #[test]
    fn every_provider_effective_input_change_yields_a_new_clip_id() {
        let base = fingerprint(SpeechContentKind::Highlight).clip_id();

        let cases: Vec<(&str, TestFingerprint)> = vec![
            (
                "asset_id",
                fingerprint(SpeechContentKind::Highlight).with(|f| f.asset_id = "book-2"),
            ),
            (
                "annotation_id",
                fingerprint(SpeechContentKind::Highlight)
                    .with(|f| f.annotation_id = "annotation-42"),
            ),
            (
                "text snapshot",
                fingerprint(SpeechContentKind::Highlight)
                    .with(|f| f.normalized_speech_text = "高亮正文。"),
            ),
            (
                "voice_id",
                fingerprint(SpeechContentKind::Highlight).with(|f| f.voice_id = "female_0007_b"),
            ),
            (
                "speed",
                fingerprint(SpeechContentKind::Highlight).with(|f| f.speed_x100 = 101),
            ),
            (
                "volume",
                fingerprint(SpeechContentKind::Highlight).with(|f| f.volume_x100 = 99),
            ),
            (
                "pitch",
                fingerprint(SpeechContentKind::Highlight).with(|f| f.pitch = 1),
            ),
            (
                "model",
                fingerprint(SpeechContentKind::Highlight)
                    .with(|f| f.model = "senseaudio-tts-1.5-260319"),
            ),
            (
                "provider",
                fingerprint(SpeechContentKind::Highlight).with(|f| f.provider = "other-provider"),
            ),
        ];

        for (label, changed) in cases {
            assert_ne!(changed.clip_id(), base, "{label} must change the clip ID");
        }

        // 音频规格：固定字段里任何一个变化都必须是新的生成身份。
        for (label, audio) in [
            (
                "sample rate",
                AudioSettings {
                    sample_rate: 44_100,
                    ..AudioSettings::v1()
                },
            ),
            (
                "bitrate",
                AudioSettings {
                    bitrate: 64_000,
                    ..AudioSettings::v1()
                },
            ),
            (
                "channel",
                AudioSettings {
                    channel: 1,
                    ..AudioSettings::v1()
                },
            ),
            (
                "format",
                AudioSettings {
                    format: "wav".to_string(),
                    ..AudioSettings::v1()
                },
            ),
        ] {
            let changed = fingerprint(SpeechContentKind::Highlight).with_audio(audio);
            assert_ne!(changed.clip_id(), base, "{label} must change the clip ID");
        }

        // 内容部分不同也必须是新的 clip。
        assert_ne!(
            fingerprint(SpeechContentKind::Note).clip_id(),
            base,
            "content kind must change the clip ID"
        );
    }
}
