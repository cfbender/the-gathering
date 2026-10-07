//! Rewrites RTP sequence numbers, timestamps, and VP8 picture ids so packets taken from
//! several simulcast encodings form one continuous stream (`ExWebRTC.RTP.Munger`).

use std::time::Instant;

use str0m::rtp::Vp8Descriptor;

use crate::codec::VideoCodec;

const BREAKPOINT: i32 = 0x7FFF;

/// One packet as the publisher sent it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RtpIn<'a> {
    pub(crate) sequence_number: u16,
    pub(crate) timestamp: u32,
    pub(crate) payload: &'a [u8],
    /// When the packet arrived; the wallclock a layer switch bases its timestamp gap on.
    pub(crate) arrival: Instant,
}

/// What to put on the forwarded copy of a packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RtpOut {
    pub(crate) sequence_number: u16,
    pub(crate) timestamp: u32,
    /// New VP8 payload descriptor fields, when the packet is VP8.
    pub(crate) vp8: Option<Vp8Rewrite>,
}

/// VP8 payload descriptor fields to write into the forwarded packet; `None` keeps a field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Vp8Rewrite {
    pub(crate) picture_id: Option<u16>,
    pub(crate) tl0_pic_idx: Option<u8>,
    pub(crate) key_idx: Option<u8>,
}

#[derive(Clone, Copy, Debug)]
struct Last {
    sequence_number: u16,
    timestamp: u32,
    arrival: Instant,
}

/// The rewrite state of one viewer's copy of one publisher's video.
#[derive(Clone, Debug)]
pub(crate) struct Munger {
    clock_rate: u32,
    last: Option<Last>,
    sn_offset: u16,
    ts_offset: u32,
    update: bool,
    vp8: Option<Vp8Munger>,
}

impl Munger {
    pub(crate) fn new(codec: VideoCodec, clock_rate: u32) -> Self {
        Self {
            clock_rate,
            last: None,
            sn_offset: 0,
            ts_offset: 0,
            update: false,
            vp8: (codec == VideoCodec::Vp8).then(Vp8Munger::default),
        }
    }

    /// The next packet passed to [`Munger::munge`] comes from a different encoding.
    pub(crate) fn update(&mut self) {
        self.update = true;
    }

    /// The packet's fields in the common sequence-number and timestamp domain.
    pub(crate) fn munge(&mut self, packet: &RtpIn<'_>) -> RtpOut {
        let Some(last) = self.last else {
            // The first packet ever: its numbers start the domain.
            if let Some(vp8) = &mut self.vp8 {
                vp8.init(packet.payload);
            }
            self.last = Some(Last {
                sequence_number: packet.sequence_number,
                timestamp: packet.timestamp,
                arrival: packet.arrival,
            });
            return RtpOut {
                sequence_number: packet.sequence_number,
                timestamp: packet.timestamp,
                vp8: None,
            };
        };

        if self.update {
            if let Some(vp8) = &mut self.vp8 {
                vp8.update(packet.payload);
            }
            let vp8 = self.vp8.as_mut().map(|vp8| vp8.munge(packet.payload));

            // At least one tick, in case the last packet of the old encoding and the first of
            // the new one arrived (almost) together.
            let elapsed = packet
                .arrival
                .saturating_duration_since(last.arrival)
                .as_nanos();
            let ticks = (elapsed * u128::from(self.clock_rate) + 500_000_000) / 1_000_000_000;
            let ticks = u32::try_from(ticks).unwrap_or(u32::MAX).max(1);

            self.ts_offset = packet
                .timestamp
                .wrapping_sub(last.timestamp)
                .wrapping_sub(ticks);
            self.sn_offset = packet
                .sequence_number
                .wrapping_sub(last.sequence_number)
                .wrapping_sub(1);
            let out = self.adjust(packet, vp8);
            self.last = Some(Last {
                sequence_number: out.sequence_number,
                timestamp: out.timestamp,
                arrival: packet.arrival,
            });
            self.update = false;
            return out;
        }

        let vp8 = self.vp8.as_mut().map(|vp8| vp8.munge(packet.payload));
        let out = self.adjust(packet, vp8);
        let delta = i32::from(out.sequence_number) - i32::from(last.sequence_number);
        if delta < -BREAKPOINT || (delta > 0 && delta < BREAKPOINT) {
            self.last = Some(Last {
                sequence_number: out.sequence_number,
                timestamp: out.timestamp,
                arrival: packet.arrival,
            });
        }
        out
    }

    fn adjust(&self, packet: &RtpIn<'_>, vp8: Option<Option<Vp8Rewrite>>) -> RtpOut {
        RtpOut {
            sequence_number: packet.sequence_number.wrapping_sub(self.sn_offset),
            timestamp: packet.timestamp.wrapping_sub(self.ts_offset),
            vp8: vp8.flatten(),
        }
    }
}

/// One rewritable VP8 descriptor field.
#[derive(Clone, Copy, Debug, Default)]
struct Field {
    used: bool,
    last: i32,
    offset: i32,
}

impl Field {
    fn init(&mut self, value: Option<i32>) {
        self.used = value.is_some();
        self.last = value.unwrap_or(0);
    }

    /// The new encoding's value continues one past the last value forwarded.
    ///
    /// ex_webrtc subtracted from a missing value and crashed the room when the new encoding's
    /// descriptor lacked the field; a missing field keeps the old offset here.
    fn update(&mut self, value: Option<i32>) {
        if let (true, Some(value)) = (self.used, value) {
            self.offset = value - self.last - 1;
        }
    }

    fn munge(&mut self, value: Option<i32>, modulus: i32) -> Option<i32> {
        let munged = value.map(|value| (value + modulus - self.offset).rem_euclid(modulus));
        // ex_webrtc stored the missing value and crashed on the next switch.
        if let Some(munged) = munged {
            self.last = munged;
        }
        munged
    }
}

/// VP8 picture ids, `TL0PICIDX`, and `KEYIDX` made continuous across encodings
/// (`ExWebRTC.RTP.VP8.Munger`).
#[derive(Clone, Copy, Debug, Default)]
struct Vp8Munger {
    picture_id: Field,
    tl0_pic_idx: Field,
    key_idx: Field,
}

/// The descriptor fields of a VP8 payload, with the picture id's width.
#[derive(Clone, Copy, Debug)]
struct Vp8Fields {
    picture_id: Option<(i32, i32)>,
    tl0_pic_idx: Option<i32>,
    key_idx: Option<i32>,
}

impl Vp8Fields {
    /// ex_webrtc crashed the room on a payload it could not parse; such a packet is forwarded
    /// unchanged here.
    fn parse(payload: &[u8]) -> Option<Self> {
        let descriptor = Vp8Descriptor::parse(payload).ok()?;
        // The M bit of the first PictureID octet says whether it is 15 bits or 7.
        let long = payload.get(2).is_some_and(|octet| octet & 0x80 != 0);
        let modulus = if long { 1 << 15 } else { 1 << 7 };
        Some(Self {
            picture_id: descriptor.picture_id().map(|id| (i32::from(id), modulus)),
            tl0_pic_idx: descriptor.tl0_pic_idx().map(i32::from),
            key_idx: descriptor.key_idx().map(i32::from),
        })
    }
}

impl Vp8Munger {
    fn init(&mut self, payload: &[u8]) {
        let Some(fields) = Vp8Fields::parse(payload) else {
            return;
        };
        self.picture_id.init(fields.picture_id.map(|(id, _)| id));
        self.tl0_pic_idx.init(fields.tl0_pic_idx);
        self.key_idx.init(fields.key_idx);
    }

    fn update(&mut self, payload: &[u8]) {
        let Some(fields) = Vp8Fields::parse(payload) else {
            return;
        };
        self.picture_id.update(fields.picture_id.map(|(id, _)| id));
        self.tl0_pic_idx.update(fields.tl0_pic_idx);
        self.key_idx.update(fields.key_idx);
    }

    fn munge(&mut self, payload: &[u8]) -> Option<Vp8Rewrite> {
        let fields = Vp8Fields::parse(payload)?;
        let picture_id = fields
            .picture_id
            .and_then(|(id, modulus)| self.picture_id.munge(Some(id), modulus));
        let tl0_pic_idx = self.tl0_pic_idx.munge(fields.tl0_pic_idx, 1 << 8);
        let key_idx = self.key_idx.munge(fields.key_idx, 1 << 5);
        Some(Vp8Rewrite {
            picture_id: picture_id.and_then(|id| u16::try_from(id).ok()),
            tl0_pic_idx: tl0_pic_idx.and_then(|idx| u8::try_from(idx).ok()),
            key_idx: key_idx.and_then(|idx| u8::try_from(idx).ok()),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn packet(seq: u16, ts: u32, payload: &[u8], at: Instant) -> RtpIn<'_> {
        RtpIn {
            sequence_number: seq,
            timestamp: ts,
            payload,
            arrival: at,
        }
    }

    #[test]
    fn a_switch_continues_sequence_numbers_and_timestamps_by_elapsed_time() {
        let start = Instant::now();
        let mut munger = Munger::new(VideoCodec::H264, 90_000);
        let first = munger.munge(&packet(100, 1_000, &[1], start));
        assert_eq!((first.sequence_number, first.timestamp), (100, 1_000));

        munger.update();
        let switched = munger.munge(&packet(
            40_000,
            500_000,
            &[1],
            start + Duration::from_millis(100),
        ));
        assert_eq!(switched.sequence_number, 101);
        assert_eq!(switched.timestamp, 1_000 + 9_000);

        let next = munger.munge(&packet(
            40_001,
            503_000,
            &[1],
            start + Duration::from_millis(133),
        ));
        assert_eq!((next.sequence_number, next.timestamp), (102, 13_000));
    }

    #[test]
    fn a_switch_with_no_elapsed_time_still_advances_the_timestamp() {
        let start = Instant::now();
        let mut munger = Munger::new(VideoCodec::H264, 90_000);
        munger.munge(&packet(65_535, u32::MAX, &[1], start));
        munger.update();
        let switched = munger.munge(&packet(7, 42, &[1], start));
        assert_eq!((switched.sequence_number, switched.timestamp), (0, 0));
    }

    #[test]
    fn vp8_picture_ids_continue_across_a_switch() {
        let start = Instant::now();
        let mut munger = Munger::new(VideoCodec::Vp8, 90_000);
        // X=1, I=1 with a 15-bit PictureID of 0x0123, then the VP8 header.
        let first = [0x90, 0x80, 0x81, 0x23, 0x00];
        assert_eq!(munger.munge(&packet(1, 0, &first, start)).vp8, None);
        let second = [0x90, 0x80, 0x81, 0x24, 0x01];
        let out = munger.munge(&packet(2, 0, &second, start));
        assert_eq!(out.vp8.and_then(|vp8| vp8.picture_id), Some(0x124));

        munger.update();
        let other_layer = [0x90, 0x80, 0xff, 0xf0, 0x00];
        let out = munger.munge(&packet(900, 0, &other_layer, start));
        assert_eq!(out.vp8.and_then(|vp8| vp8.picture_id), Some(0x125));

        // A payload that is not VP8 at all passes through unchanged.
        assert_eq!(munger.munge(&packet(901, 0, &[], start)).vp8, None);
    }
}
