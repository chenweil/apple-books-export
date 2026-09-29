//! 音频产物校验：严格 hex 解码、MP3 解析与 metadata 一致性检查。
//!
//! ADR 0007 要求接受一个 Speech Cache Entry 至少满足：HTTP/API 成功、hex 解码成功、
//! 文件非空、音频格式可解析，并且供应商 metadata 与本地文件不矛盾。任一检查失败都不得
//! 提交最终目录，也不能把响应体当音频写盘。

use crate::speech::profile::AudioSettings;
use crate::speech::text::sha256_hex;

/// 首版唯一接受的音频格式标识。
pub const AUDIO_FORMAT: &str = crate::speech::profile::AUDIO_FORMAT;

/// hex 解码失败：奇数长度、非 hex 字符或空载荷。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioDecodeError {
    /// hex 字符个数是奇数。
    OddLength,
    /// 含有非 hex 字符。
    InvalidCharacter,
    /// 解码后是空音频。
    Empty,
}

impl AudioDecodeError {
    /// 稳定的 Machine JSON 错误码：provider 成功响应无法形成有效音频。
    pub const fn machine_code(&self) -> &'static str {
        "SPEECH_AUDIO_INVALID"
    }

    /// 面向人类的说明；不包含任何响应体内容。
    pub fn message(&self) -> &'static str {
        match self {
            Self::OddLength => "the provider audio payload is not valid hexadecimal: odd length",
            Self::InvalidCharacter => "the provider audio payload is not valid hexadecimal",
            Self::Empty => "the provider returned an empty audio payload",
        }
    }
}

/// 从本地音频字节解析出来的事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFacts {
    /// 容器/编码格式。
    pub format: String,
    /// 采样率（Hz）。
    pub sample_rate: u32,
    /// 码率（bps）。
    pub bitrate: u32,
    /// 声道数。
    pub channel: u32,
    /// 时长（毫秒）。
    pub duration_ms: u64,
    /// 音频字节数。
    pub size_bytes: u64,
    /// 音频字节的 SHA-256 小写 hex。
    pub sha256: String,
}

/// 严格 hex 解码：拒绝奇数长度、非 hex 字符和空结果。
pub fn decode_audio_hex(payload: &str) -> Result<Vec<u8>, AudioDecodeError> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return Err(AudioDecodeError::Empty);
    }
    if trimmed.len() % 2 != 0 {
        return Err(AudioDecodeError::OddLength);
    }
    let mut bytes = Vec::with_capacity(trimmed.len() / 2);
    let mut characters = trimmed.chars();
    while let (Some(high), Some(low)) = (characters.next(), characters.next()) {
        let byte = hex_value(high)
            .and_then(|high| hex_value(low).map(|low| (high << 4) | low))
            .ok_or(AudioDecodeError::InvalidCharacter)?;
        bytes.push(byte);
    }
    if bytes.is_empty() {
        return Err(AudioDecodeError::Empty);
    }
    Ok(bytes)
}

fn hex_value(character: char) -> Option<u8> {
    character.to_digit(16).map(|value| value as u8)
}

/// MPEG 版本与 Layer 的组合。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MpegVersion {
    /// 每帧采样数。
    samples_per_frame: u32,
    /// Layer III 帧长公式的系数：MPEG1 为 144，MPEG2/2.5 为 72。
    frame_coefficient: u32,
    /// 采样率表（Hz），索引 3 为保留值。
    sample_rates: [u32; 3],
    /// Layer III 码率表（kbps）；索引 0 为 free、15 为坏值，都不接受。
    bitrates: [u32; 16],
}

const MPEG1: MpegVersion = MpegVersion {
    samples_per_frame: 1152,
    frame_coefficient: 144,
    sample_rates: [44_100, 48_000, 32_000],
    bitrates: [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
    ],
};

const MPEG2: MpegVersion = MpegVersion {
    samples_per_frame: 576,
    frame_coefficient: 72,
    sample_rates: [22_050, 24_000, 16_000],
    bitrates: [
        0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0,
    ],
};

const MPEG2_5: MpegVersion = MpegVersion {
    samples_per_frame: 576,
    frame_coefficient: 72,
    sample_rates: [11_025, 12_000, 8_000],
    bitrates: [
        0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0,
    ],
};

/// 解析 MP3 帧头，得到采样率、码率、声道与时长。
///
/// 只接受 Layer III；遇到无法解析的帧头、保留位或 free/bad 码率就失败，
/// 不猜测、不补齐，也不把非音频字节当成音频。
pub fn inspect_mp3(bytes: &[u8]) -> Result<AudioFacts, AudioDecodeError> {
    if bytes.is_empty() {
        return Err(AudioDecodeError::Empty);
    }
    let mut offset = skip_id3(bytes);
    let mut frames: u64 = 0;
    let mut samples: u64 = 0;
    let mut sample_rate: Option<u32> = None;
    let mut bitrate: Option<u32> = None;
    let mut channel: Option<u32> = None;

    while offset < bytes.len() {
        if bytes[offset] == 0x00 {
            // 帧之间允许零填充。
            offset += 1;
            continue;
        }
        if offset + 4 > bytes.len() {
            break;
        }
        let header = [
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ];
        let Some(frame) = parse_frame_header(header) else {
            if frames > 0 {
                // 已经收到完整帧，尾部允许少量填充/标签字节。
                break;
            }
            return Err(AudioDecodeError::InvalidCharacter);
        };

        if let Some(previous) = sample_rate {
            if previous != frame.sample_rate {
                return Err(AudioDecodeError::InvalidCharacter);
            }
        }
        sample_rate = Some(frame.sample_rate);
        bitrate = Some(frame.bitrate);
        channel = Some(frame.channel);
        frames += 1;
        samples += u64::from(frame.samples_per_frame);
        offset += frame.length;
    }

    let sample_rate = sample_rate.ok_or(AudioDecodeError::InvalidCharacter)?;
    let bitrate = bitrate.ok_or(AudioDecodeError::InvalidCharacter)?;
    let channel = channel.ok_or(AudioDecodeError::InvalidCharacter)?;
    let duration_ms = samples
        .checked_mul(1000)
        .map(|scaled| scaled / u64::from(sample_rate))
        .unwrap_or(0);

    Ok(AudioFacts {
        format: AUDIO_FORMAT.to_string(),
        sample_rate,
        bitrate,
        channel,
        duration_ms,
        size_bytes: bytes.len() as u64,
        sha256: sha256_hex(bytes),
    })
}

struct Mp3Frame {
    sample_rate: u32,
    bitrate: u32,
    channel: u32,
    samples_per_frame: u32,
    length: usize,
}

fn parse_frame_header(header: [u8; 4]) -> Option<Mp3Frame> {
    if header[0] != 0xFF || (header[1] & 0xE0) != 0xE0 {
        return None;
    }
    let version_bits = (header[1] >> 3) & 0x03;
    let layer_bits = (header[1] >> 1) & 0x03;
    if layer_bits != 0x01 {
        // 只支持 Layer III。
        return None;
    }
    let version = match version_bits {
        0b11 => &MPEG1,
        0b10 => &MPEG2,
        0b00 => &MPEG2_5,
        _ => return None,
    };
    let bitrate_index = ((header[2] >> 4) & 0x0F) as usize;
    let sample_index = ((header[2] >> 2) & 0x03) as usize;
    if bitrate_index == 0 || bitrate_index == 15 || sample_index == 3 {
        return None;
    }
    let padding = usize::from((header[2] >> 1) & 0x01);
    let channel_mode = (header[3] >> 6) & 0x03;

    let bitrate_kbps = version.bitrates[bitrate_index];
    let sample_rate = version.sample_rates[sample_index];
    let channel = if channel_mode == 0b11 { 1 } else { 2 };
    let length =
        ((version.frame_coefficient * bitrate_kbps * 1000) / sample_rate) as usize + padding;
    if length < 4 {
        return None;
    }

    Some(Mp3Frame {
        sample_rate,
        bitrate: bitrate_kbps * 1000,
        channel,
        samples_per_frame: version.samples_per_frame,
        length,
    })
}

/// 跳过 ID3v2 标签；标签长度字段是 syncsafe 整数。
fn skip_id3(bytes: &[u8]) -> usize {
    if bytes.len() < 10 || &bytes[0..3] != b"ID3" {
        return 0;
    }
    let size = ((usize::from(bytes[6]) << 21)
        | (usize::from(bytes[7]) << 14)
        | (usize::from(bytes[8]) << 7)
        | usize::from(bytes[9]))
        + 10;
    size.min(bytes.len())
}

/// 校验音频字节符合首版固定规格，并返回本地事实。
///
/// 供应商 metadata 与本地文件不一致时返回 [`AudioDecodeError::InvalidCharacter`]，
/// 对应机器码 `SPEECH_AUDIO_INVALID`，调用方据此记录
/// `provider_succeeded_artifact_missing` 阻塞态。
pub fn validate_audio(
    bytes: &[u8],
    settings: &AudioSettings,
) -> Result<AudioFacts, AudioDecodeError> {
    let facts = inspect_mp3(bytes)?;
    if facts.format != settings.format
        || facts.sample_rate != settings.sample_rate
        || facts.bitrate != settings.bitrate
        || facts.channel != settings.channel
    {
        return Err(AudioDecodeError::InvalidCharacter);
    }
    Ok(facts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_encode(bytes: &[u8]) -> String {
        let mut hex = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            hex.push_str(&format!("{byte:02x}"));
        }
        hex
    }

    /// 构造一个可解析的最小 MP3：MPEG1 Layer III、128kbps、32000Hz、立体声。
    fn silent_mp3(frames: usize, channel_mode: u8) -> Vec<u8> {
        let mut bytes = Vec::new();
        for _ in 0..frames {
            let header = [
                0xFF,
                0xFB, // MPEG1 Layer III，无 CRC
                0x98, // 128kbps、32000Hz、无 padding
                (channel_mode << 6) | 0x0C,
            ];
            bytes.extend_from_slice(&header);
            let length = 144 * 128_000 / 32_000;
            bytes.extend(std::iter::repeat(0u8).take(length - 4));
        }
        bytes
    }

    #[test]
    fn strict_hex_decoding_rejects_odd_invalid_and_empty_payloads() {
        assert_eq!(decode_audio_hex(""), Err(AudioDecodeError::Empty));
        assert_eq!(decode_audio_hex("   "), Err(AudioDecodeError::Empty));
        assert_eq!(decode_audio_hex("abc"), Err(AudioDecodeError::OddLength));
        assert_eq!(
            decode_audio_hex("zzzz"),
            Err(AudioDecodeError::InvalidCharacter)
        );
        assert_eq!(
            decode_audio_hex("49 43304"),
            Err(AudioDecodeError::InvalidCharacter),
            "an inner space must be rejected as a non-hex character"
        );
        assert_eq!(
            decode_audio_hex("abgd"),
            Err(AudioDecodeError::InvalidCharacter)
        );

        assert_eq!(
            decode_audio_hex("49443304"),
            Ok(vec![0x49, 0x44, 0x33, 0x04])
        );
        assert_eq!(
            decode_audio_hex("  49443304  "),
            Ok(vec![0x49, 0x44, 0x33, 0x04]),
            "surrounding whitespace is trimmed, inner whitespace is not"
        );
    }

    #[test]
    fn decodes_a_silent_mp3_with_the_fixed_v1_specification() {
        let bytes = silent_mp3(4, 0b00);

        let facts = inspect_mp3(&bytes).expect("parse mp3");

        assert_eq!(facts.format, "mp3");
        assert_eq!(facts.sample_rate, 32_000);
        assert_eq!(facts.bitrate, 128_000);
        assert_eq!(facts.channel, 2);
        assert_eq!(facts.size_bytes, bytes.len() as u64);
        assert_eq!(facts.sha256, sha256_hex(&bytes));
        // 4 帧 × 1152 采样 / 32000Hz = 144ms。
        assert_eq!(facts.duration_ms, 144);
    }

    #[test]
    fn mono_mp3_reports_one_channel() {
        let facts = inspect_mp3(&silent_mp3(1, 0b11)).expect("parse mp3");

        assert_eq!(facts.channel, 1);
    }

    #[test]
    fn validation_rejects_every_metadata_mismatch() {
        let bytes = silent_mp3(2, 0b00);
        let expected = AudioSettings::v1();

        validate_audio(&bytes, &expected).expect("the fixed specification matches");

        for settings in [
            AudioSettings {
                sample_rate: 44_100,
                ..expected.clone()
            },
            AudioSettings {
                bitrate: 64_000,
                ..expected.clone()
            },
            AudioSettings {
                channel: 1,
                ..expected.clone()
            },
            AudioSettings {
                format: "wav".to_string(),
                ..expected.clone()
            },
        ] {
            let error = validate_audio(&bytes, &settings).expect_err("mismatch");
            assert_eq!(error.machine_code(), "SPEECH_AUDIO_INVALID");
        }
    }

    #[test]
    fn non_audio_payloads_are_rejected() {
        // 真实错误响应体常以 JSON 开头：绝不能被当成音频写进缓存。
        let json_body = br#"{"base_resp":{"status_code":0,"status_msg":"success"}}"#;
        let error = validate_audio(json_body, &AudioSettings::v1()).expect_err("json body");
        assert_eq!(error.machine_code(), "SPEECH_AUDIO_INVALID");

        assert_eq!(inspect_mp3(&[]), Err(AudioDecodeError::Empty));
        assert!(inspect_mp3(b"not audio at all").is_err());
        // 保留采样率索引与坏码率索引都必须失败。
        let mut bad_sample = silent_mp3(1, 0b00);
        bad_sample[2] = 0x9C; // 采样率索引 3（保留）
        assert!(inspect_mp3(&bad_sample).is_err());
        let mut bad_bitrate = silent_mp3(1, 0b00);
        bad_bitrate[2] = 0xF8; // 码率索引 15（坏值）
        assert!(inspect_mp3(&bad_bitrate).is_err());
    }

    #[test]
    fn id3_tags_are_skipped_before_the_first_frame() {
        let mut bytes = b"ID3\x04\x00\x00\x00\x00\x00\x0A".to_vec();
        bytes.extend_from_slice(&[0u8; 10]);
        bytes.extend_from_slice(&silent_mp3(1, 0b00));

        let facts = inspect_mp3(&bytes).expect("parse mp3 with an ID3 tag");

        assert_eq!(facts.sample_rate, 32_000);
        assert_eq!(facts.channel, 2);
    }

    #[test]
    fn hex_round_trip_keeps_every_byte() {
        let bytes = silent_mp3(3, 0b00);

        assert_eq!(decode_audio_hex(&hex_encode(&bytes)), Ok(bytes));
    }
}
