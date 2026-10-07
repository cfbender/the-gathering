//! The two video codecs the table negotiates, and what the SFU needs to know about their
//! RTP payloads: whether a packet starts a keyframe.

use str0m::format::{Codec, PayloadParams};
use str0m::media::Pt;
use str0m::rtp::Vp8Descriptor;

/// H.264 lets hardware encoders and decoders carry the load in every browser; VP8 is the
/// fallback every WebRTC stack ships. No audio: the table is video-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VideoCodec {
    H264,
    Vp8,
}

impl VideoCodec {
    pub(crate) fn mime_type(self) -> &'static str {
        match self {
            Self::H264 => "video/H264",
            Self::Vp8 => "video/VP8",
        }
    }
}

/// A negotiated payload type and the parts of its format that decide whether two peers'
/// entries carry the same bitstream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CodecParams {
    pub(crate) pt: Pt,
    pub(crate) codec: VideoCodec,
    pub(crate) clock_rate: u32,
    /// H.264 `profile-level-id` and `packetization-mode` (0 when absent); `None` for VP8.
    pub(crate) bitstream: Option<(Option<u32>, u8)>,
}

impl CodecParams {
    /// The SFU's view of `params`, if it is H.264 or VP8.
    pub(crate) fn from_payload(params: &PayloadParams) -> Option<Self> {
        let spec = params.spec();
        let codec = match spec.codec {
            Codec::H264 => VideoCodec::H264,
            Codec::Vp8 => VideoCodec::Vp8,
            _ => return None,
        };
        let bitstream = (codec == VideoCodec::H264).then(|| {
            (
                spec.format.profile_level_id,
                spec.format.packetization_mode.unwrap_or(0),
            )
        });
        Some(Self {
            pt: params.pt(),
            codec,
            clock_rate: spec.clock_rate.get(),
            bitstream,
        })
    }

    /// Whether a viewer's entry carries the same bitstream as this publisher's. Only the
    /// format parameters that change the bitstream count: H.264 profile and packetization
    /// mode. Browsers decorate the rest differently (Firefox's VP8 carries `max-fs`/`max-fr`,
    /// Chrome's nothing), and a viewer decodes either just the same.
    pub(crate) fn same_bitstream(&self, other: &Self) -> bool {
        self.codec == other.codec
            && self.clock_rate == other.clock_rate
            && self.bitstream == other.bitstream
    }

    /// Whether `payload` begins a keyframe of this codec.
    pub(crate) fn keyframe(&self, payload: &[u8]) -> bool {
        match self.codec {
            VideoCodec::H264 => h264_keyframe(payload),
            VideoCodec::Vp8 => vp8_keyframe(payload),
        }
    }
}

/// Whether a VP8 RTP payload starts a keyframe: the descriptor's S bit is set with partition
/// index 0, and the VP8 payload header's P bit is clear (RFC 7741 §4.2, §4.3).
pub(crate) fn vp8_keyframe(payload: &[u8]) -> bool {
    Vp8Descriptor::parse(payload).is_ok_and(|descriptor| descriptor.starts_keyframe(payload))
}

/// Whether an H.264 RTP payload begins an intra frame, judged by the SPS that precedes every
/// IDR (`ExWebRTC.RTP.H264.keyframe?/1`). Looking for the IDR slice itself breaks layer
/// switches whenever the SPS packet was lost; the SPS is what a decoder must see first.
pub(crate) fn h264_keyframe(payload: &[u8]) -> bool {
    let Some((&header, rest)) = payload.split_first() else {
        return false;
    };
    match header & 0x1f {
        // A single NAL unit.
        nalu_type @ 1..=23 => nalu_type == 7,
        // STAP-A.
        24 => aggregate_has_sps(rest, 0),
        // STAP-B, MTAP16, MTAP24: a decoding order number precedes the units.
        nalu_type @ 25..=27 => {
            let offset = match nalu_type {
                26 => 3,
                27 => 4,
                _ => 0,
            };
            rest.get(2..)
                .is_some_and(|units| aggregate_has_sps(units, offset))
        }
        // FU-A, FU-B: the start of a fragmented SPS.
        28 | 29 => rest
            .first()
            .is_some_and(|fu| fu & 0x80 != 0 && fu & 0x1f == 7),
        _ => false,
    }
}

fn aggregate_has_sps(mut units: &[u8], offset: usize) -> bool {
    loop {
        let Some(size_bytes) = units.get(..2) else {
            return false;
        };
        let size = usize::from(u16::from_be_bytes([
            size_bytes.first().copied().unwrap_or(0),
            size_bytes.get(1).copied().unwrap_or(0),
        ]));
        let start = 2 + offset;
        let Some(nalu) = units.get(start..start + size) else {
            return false;
        };
        if nalu
            .first()
            .is_some_and(|byte| byte & 0x80 == 0 && byte & 0x1f == 7)
        {
            return true;
        }
        units = units.get(start + size..).unwrap_or(&[]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h264_keyframes_are_recognised_by_their_sps() {
        assert!(h264_keyframe(&[0x67, 0x42]));
        assert!(
            !h264_keyframe(&[0x65, 0x88]),
            "an IDR slice without its SPS"
        );
        assert!(!h264_keyframe(&[0x41, 0x9a]));
        assert!(!h264_keyframe(&[]));
        // STAP-A carrying SPS and PPS.
        assert!(h264_keyframe(&[
            0x78, 0x00, 0x02, 0x67, 0x42, 0x00, 0x02, 0x68, 0xce
        ]));
        // STAP-A without an SPS, and a truncated one.
        assert!(!h264_keyframe(&[0x78, 0x00, 0x02, 0x68, 0xce]));
        assert!(!h264_keyframe(&[0x78, 0x00, 0x09, 0x67]));
        // FU-A start of an SPS, and a continuation.
        assert!(h264_keyframe(&[0x7c, 0x87, 0x00]));
        assert!(!h264_keyframe(&[0x7c, 0x07, 0x00]));
    }

    #[test]
    fn vp8_keyframes_need_the_partition_start_and_a_clear_p_bit() {
        assert!(vp8_keyframe(&[0x10, 0x00, 0x9d]));
        assert!(!vp8_keyframe(&[0x10, 0x01, 0x9d]), "an interframe");
        assert!(
            !vp8_keyframe(&[0x00, 0x00, 0x9d]),
            "not the start of a partition"
        );
        // With an extended PictureID.
        assert!(vp8_keyframe(&[0x90, 0x80, 0x81, 0x23, 0x00]));
        assert!(!vp8_keyframe(&[0x90]));
    }
}
