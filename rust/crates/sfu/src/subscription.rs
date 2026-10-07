//! One viewer's copy of one publisher's video (`TheGathering.WebcamTables.Sfu.Subscription`).
//!
//! The publisher sends up to three simulcast layers; the viewer receives exactly one. This
//! module decides, packet by packet, which layer's packets are forwarded and rewrites their
//! sequence numbers and timestamps so the viewer's decoder sees a single continuous stream
//! across switches. A switch (or a resume after a private reveal) only happens on a keyframe
//! of the new layer, because a decoder cannot pick up a stream mid-frame.
//!
//! It is pure: the room owns the connections and asks this module what to do with each
//! packet. A publisher without simulcast has the single encoding [`Encoding::Single`].

use str0m::media::Mid;

use crate::codec::CodecParams;
use crate::ids::PeerId;
use crate::layer::Encoding;
use crate::munger::{Munger, RtpIn, RtpOut};

/// How far behind the newest forwarded packet a late one may still arrive and be forwarded.
/// About a second of 1080p; anything older is as good as lost to the viewer anyway.
const WINDOW: u16 = 256;

/// What to do with one packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    /// Send the packet with these fields.
    Forward(RtpOut),
    Skip,
}

#[derive(Clone, Debug)]
pub(crate) struct Subscription {
    pub(crate) owner: PeerId,
    /// The viewer's `sendonly` media section carrying this board, once offered.
    pub(crate) mid: Option<Mid>,
    pub(crate) wanted: Encoding,
    pub(crate) layer: Option<Encoding>,
    pub(crate) pending: Option<Encoding>,
    pub(crate) allowed: bool,
    sender: Option<(CodecParams, Munger)>,
    started: bool,
    /// The newest publisher sequence number forwarded from the current layer, and which of
    /// the ones before it were forwarded.
    newest: Option<u16>,
    seen: SeenWindow,
}

impl Subscription {
    /// A subscription that starts on `wanted` as soon as that layer sends a keyframe.
    pub(crate) fn new(owner: PeerId, wanted: Encoding) -> Self {
        Self {
            owner,
            mid: None,
            wanted,
            layer: None,
            pending: Some(wanted),
            allowed: true,
            sender: None,
            started: false,
            newest: None,
            seen: SeenWindow::default(),
        }
    }

    /// Records the codec the viewer's sender sends in. Packets are rewritten in that codec's
    /// clock, and VP8 picture ids are made continuous as well.
    pub(crate) fn set_codec(&mut self, codec: CodecParams) {
        self.sender = Some((codec, Munger::new(codec.codec, codec.clock_rate)));
    }

    /// The viewer's payload type and codec, once applied.
    pub(crate) fn codec(&self) -> Option<&CodecParams> {
        self.sender.as_ref().map(|(codec, _)| codec)
    }

    /// True once the sender codec is applied and the viewer may receive this board.
    pub(crate) fn ready(&self) -> bool {
        self.allowed && self.sender.is_some()
    }

    /// Asks for `layer`. Returns whether a keyframe of that layer must be requested from the
    /// publisher before the switch can happen.
    pub(crate) fn request_layer(&mut self, layer: Encoding) -> bool {
        self.wanted = layer;
        if self.layer == Some(layer) {
            self.pending = None;
            false
        } else if self.pending == Some(layer) {
            false
        } else {
            self.pending = Some(layer);
            true
        }
    }

    /// Moves to `layer` without changing which one the viewer wants, for when the wanted
    /// layer has stopped arriving (or started again). Returns whether a keyframe of `layer`
    /// must be requested.
    pub(crate) fn fall_back(&mut self, layer: Encoding) -> bool {
        if self.layer == Some(layer) {
            self.pending = None;
            false
        } else if self.pending == Some(layer) {
            false
        } else {
            self.pending = Some(layer);
            true
        }
    }

    /// Allows or blocks forwarding. Blocking stops packets immediately; allowing again waits
    /// for a keyframe of the wanted layer, so the result says whether to request one.
    pub(crate) fn set_allowed(&mut self, allowed: bool) -> bool {
        if self.allowed == allowed {
            return false;
        }
        self.allowed = allowed;
        self.layer = None;
        self.pending = Some(self.wanted);
        allowed
    }

    /// Decides what to do with one packet the publisher sent on `rid`.
    ///
    /// The pending layer is adopted on its first keyframe. A subscription that is not showing
    /// anything yet also adopts a keyframe of any other layer rather than staying black until
    /// the wanted layer delivers one (the browser may have paused that layer under CPU or
    /// bandwidth pressure).
    pub(crate) fn route(&mut self, rid: Encoding, packet: &RtpIn<'_>, keyframe: bool) -> Route {
        if !self.ready() {
            return Route::Skip;
        }
        if Some(rid) == self.layer {
            return self.forward(packet);
        }
        if keyframe && Some(rid) == self.pending {
            self.switch_to(rid);
            self.pending = None;
            return self.forward(packet);
        }
        if keyframe && self.layer.is_none() {
            self.switch_to(rid);
            self.pending = (self.wanted != rid).then_some(self.wanted);
            return self.forward(packet);
        }
        Route::Skip
    }

    /// The munger is told about a stream change only once something was forwarded; told
    /// before its first packet, it would treat the second packet as the start of a new stream.
    fn switch_to(&mut self, rid: Encoding) {
        self.layer = Some(rid);
        self.newest = None;
        self.seen = SeenWindow::default();
        if self.started {
            if let Some((_, munger)) = &mut self.sender {
                munger.update();
            }
        }
    }

    fn forward(&mut self, packet: &RtpIn<'_>) -> Route {
        if !self.mark_seen(packet.sequence_number) {
            return Route::Skip;
        }
        let Some((_, munger)) = &mut self.sender else {
            return Route::Skip;
        };
        let out = munger.munge(packet);
        self.started = true;
        Route::Forward(out)
    }

    /// Whether the publisher's packet is one this subscription has not forwarded yet.
    /// Browsers resend recent packets over RTX to probe for bandwidth; a sequence number sent
    /// twice fails the viewer's SRTP replay check. Late packets within the window are still
    /// forwarded once.
    fn mark_seen(&mut self, seq: u16) -> bool {
        let Some(newest) = self.newest else {
            self.newest = Some(seq);
            self.seen = SeenWindow::default();
            self.seen.set(0);
            return true;
        };
        let ahead = seq.wrapping_sub(newest);
        if ahead == 0 {
            return false;
        }
        if ahead < 0x8000 {
            self.seen.shift(ahead);
            self.seen.set(0);
            self.newest = Some(seq);
            return true;
        }
        let behind = newest.wrapping_sub(seq);
        if behind >= WINDOW || self.seen.get(behind) {
            return false;
        }
        self.seen.set(behind);
        true
    }
}

/// The layer in `live` a viewer wanting `wanted` should get: the sharpest live one no sharper
/// than `wanted`, else the softest live one. `rids` lists layers softest first.
pub(crate) fn nearest_live(
    wanted: Encoding,
    live: &[Encoding],
    rids: &[Encoding],
) -> Option<Encoding> {
    let wanted_at = rids
        .iter()
        .position(|rid| *rid == wanted)
        .unwrap_or(rids.len());
    rids.iter()
        .enumerate()
        .filter(|(_, rid)| live.contains(rid))
        .min_by_key(|(index, _)| (*index > wanted_at, index.abs_diff(wanted_at)))
        .map(|(_, rid)| *rid)
}

/// A bit per sequence number up to [`WINDOW`] behind the newest (bit n = `newest - n`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SeenWindow([u64; 4]);

impl SeenWindow {
    fn get(&self, bit: u16) -> bool {
        let (word, offset) = (usize::from(bit / 64), bit % 64);
        self.0
            .get(word)
            .is_some_and(|value| value >> offset & 1 == 1)
    }

    fn set(&mut self, bit: u16) {
        let (word, offset) = (usize::from(bit / 64), bit % 64);
        if let Some(value) = self.0.get_mut(word) {
            *value |= 1 << offset;
        }
    }

    /// Moves every bit `by` places older; bits past the window fall off.
    fn shift(&mut self, by: u16) {
        if by >= WINDOW {
            self.0 = [0; 4];
            return;
        }
        let words = usize::from(by / 64);
        let bits = u32::from(by % 64);
        let old = self.0;
        for (index, slot) in self.0.iter_mut().enumerate() {
            let source = index.checked_sub(words);
            let high = source
                .and_then(|source| old.get(source))
                .copied()
                .unwrap_or(0);
            let low = source
                .and_then(|source| source.checked_sub(1))
                .and_then(|source| old.get(source))
                .copied()
                .unwrap_or(0);
            *slot = if bits == 0 {
                high
            } else {
                high << bits | low >> (64 - bits)
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use str0m::media::Pt;

    use super::*;
    use crate::codec::VideoCodec;
    use crate::layer::Layer;

    const L: Encoding = Encoding::Layer(Layer::Low);
    const M: Encoding = Encoding::Layer(Layer::Medium);
    const H: Encoding = Encoding::Layer(Layer::High);

    // H.264 packets need no payload rewriting, so any payload serves as a frame here.
    fn codec() -> CodecParams {
        CodecParams {
            pt: Pt::from(96),
            codec: VideoCodec::H264,
            clock_rate: 90_000,
            bitstream: Some((Some(0x42e0_1f), 1)),
        }
    }

    fn subscription(wanted: Encoding) -> Subscription {
        let mut sub = Subscription::new(PeerId::from("owner"), wanted);
        sub.set_codec(codec());
        sub
    }

    fn packet(seq: u16) -> RtpIn<'static> {
        packet_at(seq, 0)
    }

    fn packet_at(seq: u16, timestamp: u32) -> RtpIn<'static> {
        RtpIn {
            sequence_number: seq,
            timestamp,
            payload: &[1],
            arrival: Instant::now(),
        }
    }

    fn forwarded(route: Route) -> RtpOut {
        match route {
            Route::Forward(out) => out,
            Route::Skip => panic!("expected the packet to be forwarded"),
        }
    }

    #[test]
    fn nothing_is_forwarded_until_the_sender_codec_is_applied() {
        let mut sub = Subscription::new(PeerId::from("o"), M);
        assert!(!sub.ready());
        assert_eq!(sub.route(M, &packet(1), true), Route::Skip);
    }

    #[test]
    fn starts_on_the_wanted_layer_at_its_first_keyframe_dropping_earlier_delta_frames() {
        let mut sub = subscription(M);
        assert_eq!(sub.route(M, &packet(1), false), Route::Skip);
        assert_eq!(sub.route(H, &packet(1), false), Route::Skip);
        forwarded(sub.route(M, &packet(2), true));
        assert_eq!((sub.layer, sub.pending), (Some(M), None));
        assert_eq!(sub.route(H, &packet(3), false), Route::Skip);
    }

    #[test]
    fn switches_layers_only_on_a_keyframe_of_the_new_layer_and_keeps_sequence_numbers_continuous() {
        let mut sub = subscription(L);
        let first = forwarded(sub.route(L, &packet_at(100, 1_000), true));
        let second = forwarded(sub.route(L, &packet_at(101, 1_000), false));
        assert_eq!(second.sequence_number, first.sequence_number + 1);

        assert!(sub.request_layer(H));
        assert_eq!((sub.layer, sub.pending), (Some(L), Some(H)));

        // Delta frames on the new layer are dropped; the old layer keeps flowing meanwhile.
        assert_eq!(sub.route(H, &packet_at(5_000, 9_000), false), Route::Skip);
        let third = forwarded(sub.route(L, &packet_at(102, 1_000), false));
        assert_eq!(third.sequence_number, second.sequence_number + 1);

        // The high layer's sequence numbers are far away, but the viewer sees the next number.
        let fourth = forwarded(sub.route(H, &packet_at(5_001, 9_000), true));
        assert_eq!(fourth.sequence_number, third.sequence_number + 1);
        assert_eq!((sub.layer, sub.pending), (Some(H), None));
        assert_eq!(sub.route(L, &packet_at(103, 1_000), true), Route::Skip);
    }

    #[test]
    fn asking_for_the_current_or_already_pending_layer_needs_no_new_keyframe() {
        let mut sub = subscription(M);
        forwarded(sub.route(M, &packet(1), true));
        assert!(!sub.request_layer(M));

        assert!(sub.request_layer(H));
        assert!(!sub.request_layer(H));
    }

    #[test]
    fn falling_back_moves_only_the_pending_layer_and_keeps_the_wanted_one() {
        let mut sub = subscription(H);
        forwarded(sub.route(H, &packet(1), true));

        assert!(sub.fall_back(L));
        assert_eq!((sub.wanted, sub.layer, sub.pending), (H, Some(H), Some(L)));
        assert!(!sub.fall_back(L));
        assert_eq!((sub.wanted, sub.layer, sub.pending), (H, Some(H), Some(L)));

        // The old layer keeps flowing until the new one's keyframe arrives, then the viewer is
        // on the fallback but still wants the sharp layer.
        forwarded(sub.route(H, &packet(2), false));
        forwarded(sub.route(L, &packet(3), true));
        assert_eq!((sub.wanted, sub.layer, sub.pending), (H, Some(L), None));

        // Returning to the wanted layer once it is back clears the pending state on arrival.
        assert!(sub.fall_back(H));
        forwarded(sub.route(H, &packet(4), true));
        assert_eq!((sub.wanted, sub.layer, sub.pending), (H, Some(H), None));
        assert!(!sub.fall_back(H));
    }

    #[test]
    fn the_nearest_live_layer_is_the_sharpest_at_or_below_the_wanted_one_else_the_softest() {
        let rids = [L, M, H];
        assert_eq!(nearest_live(H, &[L, M], &rids), Some(M));
        assert_eq!(nearest_live(H, &[L], &rids), Some(L));
        assert_eq!(nearest_live(M, &[L, H], &rids), Some(L));
        assert_eq!(nearest_live(L, &[M, H], &rids), Some(M));
        assert_eq!(nearest_live(M, &[M], &rids), Some(M));
        assert_eq!(nearest_live(H, &[], &rids), None);
    }

    #[test]
    fn a_hidden_board_stops_immediately_and_resumes_only_on_a_keyframe() {
        let mut sub = subscription(M);
        forwarded(sub.route(M, &packet(1), true));

        assert!(!sub.set_allowed(false));
        assert!(!sub.ready());
        assert_eq!(sub.route(M, &packet(2), true), Route::Skip);

        assert!(sub.set_allowed(true));
        assert_eq!(sub.pending, Some(M));
        assert_eq!(sub.route(M, &packet(3), false), Route::Skip);
        forwarded(sub.route(M, &packet(4), true));
        assert!(!sub.set_allowed(true));
    }

    #[test]
    fn a_blank_viewer_adopts_any_layers_keyframe_but_keeps_waiting_for_the_wanted_one() {
        let mut sub = subscription(H);
        forwarded(sub.route(L, &packet(1), true));
        assert_eq!((sub.layer, sub.pending), (Some(L), Some(H)));
        forwarded(sub.route(L, &packet(2), false));
        forwarded(sub.route(H, &packet(50), true));
        assert_eq!((sub.layer, sub.pending), (Some(H), None));
    }

    #[test]
    fn a_publisher_without_simulcast_has_one_layer() {
        let mut sub = subscription(Encoding::Single);
        forwarded(sub.route(Encoding::Single, &packet(1), true));
        forwarded(sub.route(Encoding::Single, &packet(2), false));
    }

    #[test]
    fn a_packet_the_publisher_resends_is_forwarded_once_a_late_one_still_once() {
        let mut sub = subscription(M);
        forwarded(sub.route(M, &packet(10), true));
        forwarded(sub.route(M, &packet(11), false));
        // Packet 12 is lost for now; 13 arrives.
        forwarded(sub.route(M, &packet(13), false));

        // RTX probing resends the newest and an older packet.
        assert_eq!(sub.route(M, &packet(13), false), Route::Skip);
        assert_eq!(sub.route(M, &packet(11), false), Route::Skip);

        // The genuinely late packet gets through, but only the first time.
        let late = forwarded(sub.route(M, &packet(12), false));
        assert_eq!(late.sequence_number, 12);
        assert_eq!(sub.route(M, &packet(12), false), Route::Skip);

        forwarded(sub.route(M, &packet(14), false));
    }

    #[test]
    fn duplicate_detection_survives_sequence_number_wraparound() {
        let mut sub = subscription(M);
        forwarded(sub.route(M, &packet(65_534), true));
        forwarded(sub.route(M, &packet(65_535), false));
        forwarded(sub.route(M, &packet(1), false));

        assert_eq!(sub.route(M, &packet(65_535), false), Route::Skip);
        forwarded(sub.route(M, &packet(0), false));
        assert_eq!(sub.route(M, &packet(0), false), Route::Skip);
        // Far too old to be a late packet; it would fail the viewer's replay check anyway.
        assert_eq!(sub.route(M, &packet(60_000), false), Route::Skip);
    }

    #[test]
    fn the_duplicate_window_starts_over_on_a_layer_switch() {
        let mut sub = subscription(L);
        forwarded(sub.route(L, &packet(500), true));
        forwarded(sub.route(L, &packet(501), false));

        assert!(sub.request_layer(H));
        // The new layer happens to reuse numbers the old one already forwarded.
        forwarded(sub.route(H, &packet(500), true));
        forwarded(sub.route(H, &packet(501), false));
        assert_eq!(sub.route(H, &packet(501), false), Route::Skip);
    }

    #[test]
    fn the_seen_window_shifts_across_words() {
        let mut window = SeenWindow::default();
        window.set(0);
        window.set(63);
        window.shift(1);
        assert!(window.get(1) && window.get(64) && !window.get(0));
        window.shift(130);
        assert!(window.get(131) && window.get(194));
        window.shift(100);
        assert!(window.get(231), "bit 131 moved to 231");
        assert_eq!(
            window.0.iter().map(|word| word.count_ones()).sum::<u32>(),
            1,
            "bit 294 fell off"
        );
    }
}
