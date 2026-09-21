//! 最小 MP3 校验：解析首个 MPEG 音频帧头，确认文件非空、可解析，且采样率 / 码率 / 声道
//! 与首版固定规格一致（ADR 0007：MP3 / 32000Hz / 128kbps / 双声道）。
//!
//! 这是验收一个 provider 成功响应的本地防线：HTTP 成功但 hex/MP3 无效时必须能识别，
//! 映射为 `SPEECH_AUDIO_INVALID`，不能把坏产物提交成缓存。

use crate::speech::profile::AudioSettings;

/// 从首个有效 MPEG 帧解析出的音频信息。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mp3Info {
    /// 采样率（Hz）。
    pub sample_rate: u32,
    /// 码率（bps）。
    pub bitrate: u32,
    /// 声道数。
    pub channels: u32,
}

/// 校验失败的稳定原因；都归结为产品错误 `SPEECH_AUDIO_INVALID`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioInvalid {
    /// 文件为空。
    Empty,
    /// 找不到可解析的 MPEG 帧（不是 MP3）。
    NotMp3,
    /// 采样率与固定规格不符。
    SampleRateMismatch,
    /// 码率与固定规格不符。
    BitrateMismatch,
    /// 声道数与固定规格不符。
    ChannelMismatch,
}

/// 通过校验的音频元数据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioValidation {
    /// 采样率。
    pub sample_rate: u32,
    /// 码率。
    pub bitrate: u32,
    /// 声道数。
    pub channels: u32,
    /// 由文件大小与恒定码率估算的时长（毫秒）。
    pub duration_ms: u64,
}

/// MPEG1 Layer III 码率表（kbps），index 1..=14。
const MPEG1_LAYER3_BITRATES_KBPS: [u32; 15] = [
    0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
];
/// MPEG1 采样率表（Hz），index 0..=2。
const MPEG1_SAMPLE_RATES: [u32; 4] = [44_100, 48_000, 32_000, 0];

/// 跳过 ID3v2 tag，返回音频帧起始偏移。
fn id3v2_length(bytes: &[u8]) -> usize {
    if bytes.len() >= 10 && &bytes[..3] == b"ID3" {
        // synchsafe integer：每个字节只用低 7 位。
        let size = ((bytes[6] as usize & 0x7f) << 21)
            | ((bytes[7] as usize & 0x7f) << 14)
            | ((bytes[8] as usize & 0x7f) << 7)
            | (bytes[9] as usize & 0x7f);
        10 + size
    } else {
        0
    }
}

/// 解析 4 字节 MPEG 帧头。只接受 MPEG1 Layer III，且码率 / 采样率 index 合法。
fn parse_frame_header(header: &[u8]) -> Option<Mp3Info> {
    if header.len() < 4 || header[0] != 0xFF || (header[1] & 0xE0) != 0xE0 {
        return None;
    }
    let version_bits = (header[1] >> 3) & 0x03;
    let layer_bits = (header[1] >> 1) & 0x03;
    if version_bits != 0b11 || layer_bits != 0b01 {
        return None; // 只支持 MPEG1 Layer III
    }
    let bitrate_index = ((header[2] >> 4) & 0x0F) as usize;
    let sample_rate_index = ((header[2] >> 2) & 0x03) as usize;
    if bitrate_index == 0 || bitrate_index == 15 || sample_rate_index == 3 {
        return None;
    }
    let bitrate = MPEG1_LAYER3_BITRATES_KBPS[bitrate_index] * 1000;
    let sample_rate = MPEG1_SAMPLE_RATES[sample_rate_index];
    if sample_rate == 0 {
        return None;
    }
    let channel_mode = (header[3] >> 6) & 0x03;
    let channels = if channel_mode == 0b11 { 1 } else { 2 };
    Some(Mp3Info {
        sample_rate,
        bitrate,
        channels,
    })
}

/// 查找并解析首个有效 MPEG 帧。
pub fn parse_first_mp3_frame(bytes: &[u8]) -> Option<Mp3Info> {
    let start = id3v2_length(bytes);
    let data = bytes.get(start..)?;
    for offset in 0..data.len().saturating_sub(3) {
        if data[offset] == 0xFF && (data[offset + 1] & 0xE0) == 0xE0 {
            if let Some(info) = parse_frame_header(&data[offset..offset + 4]) {
                return Some(info);
            }
        }
    }
    None
}

/// 校验 provider 返回的音频字节：非空、可解析、规格匹配固定 v1 设置。
pub fn validate_mp3(bytes: &[u8], expected: &AudioSettings) -> Result<AudioValidation, AudioInvalid> {
    if bytes.is_empty() {
        return Err(AudioInvalid::Empty);
    }
    let info = parse_first_mp3_frame(bytes).ok_or(AudioInvalid::NotMp3)?;
    if info.sample_rate != expected.sample_rate {
        return Err(AudioInvalid::SampleRateMismatch);
    }
    if info.bitrate != expected.bitrate {
        return Err(AudioInvalid::BitrateMismatch);
    }
    if info.channels != expected.channel {
        return Err(AudioInvalid::ChannelMismatch);
    }
    let duration_ms = (bytes.len() as u64)
        .saturating_mul(8)
        .saturating_mul(1000)
        / u64::from(info.bitrate);
    Ok(AudioValidation {
        sample_rate: info.sample_rate,
        bitrate: info.bitrate,
        channels: info.channels,
        duration_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speech::profile::AudioSettings;

    fn v1_audio() -> AudioSettings {
        AudioSettings::v1()
    }

    /// 生成一个合法的 MPEG1 Layer III 128kbps 32kHz 立体声帧（576 字节）。
    fn stereo_frame() -> Vec<u8> {
        // FF FB 98 00：128kbps (idx 9), 32000Hz (idx 2), stereo. byte2 = 1001_10_00 = 0x98.
        let mut frame = vec![0xFF, 0xFB, 0x98, 0x00];
        frame.resize(576, 0);
        frame
    }

    #[test]
    fn parses_the_fixed_v1_stereo_frame() {
        let frame = stereo_frame();
        let info = parse_first_mp3_frame(&frame).expect("valid frame");
        assert_eq!(info.sample_rate, 32_000);
        assert_eq!(info.bitrate, 128_000);
        assert_eq!(info.channels, 2);
    }

    #[test]
    fn validates_a_realistic_mp3_and_estimates_duration() {
        let mut bytes = stereo_frame();
        bytes.extend(stereo_frame());
        bytes.extend(stereo_frame());
        let validation = validate_mp3(&bytes, &v1_audio()).expect("valid mp3");
        assert_eq!(validation.sample_rate, 32_000);
        assert_eq!(validation.bitrate, 128_000);
        assert_eq!(validation.channels, 2);
        // 3 × 576 bytes at 128000 bps ≈ 108 ms.
        assert_eq!(validation.duration_ms, (576 * 3 * 8 * 1000) / 128_000);
    }

    #[test]
    fn skips_a_leading_id3v2_tag() {
        let mut bytes = b"ID3\x04\x00\x00\x00\x00\x00\x0a".to_vec(); // 10-byte empty tag
        bytes.extend_from_slice(&[0_u8; 10]);
        bytes.extend(stereo_frame());
        let info = parse_first_mp3_frame(&bytes).expect("frame after id3");
        assert_eq!(info.sample_rate, 32_000);
    }

    #[test]
    fn rejects_empty_and_non_mp3_bytes() {
        assert_eq!(validate_mp3(&[], &v1_audio()), Err(AudioInvalid::Empty));
        assert_eq!(
            validate_mp3(b"not an mp3 at all", &v1_audio()),
            Err(AudioInvalid::NotMp3)
        );
        // 只有同步位但码率 index 非法。
        assert_eq!(
            validate_mp3(&[0xFF, 0xFB, 0x00, 0x00], &v1_audio()),
            Err(AudioInvalid::NotMp3)
        );
    }

    #[test]
    fn rejects_spec_mismatches() {
        // 采样率 44100：byte2 = 1001_00_00 = 0x90 (128kbps, 44100, stereo)
        let mut wrong_rate = vec![0xFF, 0xFB, 0x90, 0x00];
        wrong_rate.resize(576, 0);
        assert_eq!(
            validate_mp3(&wrong_rate, &v1_audio()),
            Err(AudioInvalid::SampleRateMismatch)
        );

        // 码率 32000：byte2 = 0001_10_00 = 0x18 (32kbps, 32000, stereo)
        let mut wrong_bitrate = vec![0xFF, 0xFB, 0x18, 0x00];
        wrong_bitrate.resize(576, 0);
        assert_eq!(
            validate_mp3(&wrong_bitrate, &v1_audio()),
            Err(AudioInvalid::BitrateMismatch)
        );

        // 单声道：byte3 channel mode = 11 → 0xC0
        let mut mono = vec![0xFF, 0xFB, 0x98, 0xC0];
        mono.resize(576, 0);
        assert_eq!(
            validate_mp3(&mono, &v1_audio()),
            Err(AudioInvalid::ChannelMismatch)
        );
    }
}
