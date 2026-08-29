//! The messages this layer sends, and the one rule that governs their shape.
//!
//! Non-negotiable 5: **field ordering comes from generated tables, never from a
//! call site.** Every message here is a [`Template`] built once and sorted by
//! `Fix44`, so no function in this crate ever decides that `49` precedes `52`.
//! The acceptance comparator is positional; a hand-ordered message would pass
//! review and fail the gate.

use nanofix_codec::{Template, TemplateBuilder};
use nanofix_dict::Fix44;

/// Parts and scratch bytes a session message needs.
///
/// `[measured]` the widest is the Reject at **17** parts — three static fields
/// and fourteen slots, six of them routing tags that `ReverseRoute.def`
/// exercises one pair at a time. 24 leaves room for the Resend shapes step 5
/// adds without re-sizing every template.
///
/// Scratch holds `BeginString` plus two CompIDs plus tag digits — 320 covers
/// the 32-byte maximum [`crate::Config`] can hold, which
/// [`tests::the_widest_configuration_still_builds`] proves rather than assumes.
pub(crate) type Skeleton = Template<24, 320>;

/// Every message a session generates itself, pre-sorted, plus the buffer it
/// writes into.
///
/// Owned by the session so nothing on the send path allocates. 512 bytes is
/// ample for a session message — the longest in the corpus is 101 — and a
/// resend replays stored bytes rather than re-encoding, so it does not size
/// this.
pub(crate) struct Outbound {
    pub(crate) logon: Skeleton,
    pub(crate) logout: Skeleton,
    pub(crate) reject: Skeleton,
    pub(crate) heartbeat: Skeleton,
    pub(crate) test_request: Skeleton,
    pub(crate) resend_request: Skeleton,
    pub(crate) gap_fill: Skeleton,
    pub(crate) buf: [u8; 512],
}

impl Outbound {
    /// `None` if any template cannot be built.
    ///
    /// The only ways that happens are a `BeginString` or CompID too long for
    /// the scratch, or a tag `Fix44` does not know — both configuration errors.
    /// The session treats `None` as *refuse everything*, the same fail-closed
    /// answer [`crate::Config`] gives a CompID it cannot hold.
    pub(crate) fn new(begin: &[u8], sender: &[u8], target: &[u8]) -> Option<Self> {
        Some(Self {
            logon: TemplateBuilder::<24, 320>::new(begin)
                .field(tag::MSG_TYPE, b"A")
                .field(tag::SENDER_COMP_ID, sender)
                .field(tag::TARGET_COMP_ID, target)
                .slot(tag::MSG_SEQ_NUM)
                .slot(tag::SENDING_TIME)
                .slot(tag::ENCRYPT_METHOD)
                .slot(tag::HEART_BT_INT)
                // `SessionReset.def` logs on a second time with `141=Y` and
                // expects it echoed, in the dictionary's order: `98, 108, 141`.
                .slot(tag::RESET_SEQ_NUM_FLAG)
                .build::<Fix44>()
                .ok()?,
            logout: TemplateBuilder::<24, 320>::new(begin)
                .field(tag::MSG_TYPE, b"5")
                .field(tag::SENDER_COMP_ID, sender)
                .field(tag::TARGET_COMP_ID, target)
                .slot(tag::MSG_SEQ_NUM)
                .slot(tag::SENDING_TIME)
                .slot(tag::TEXT)
                .build::<Fix44>()
                .ok()?,
            reject: TemplateBuilder::<24, 320>::new(begin)
                .field(tag::MSG_TYPE, b"3")
                .field(tag::SENDER_COMP_ID, sender)
                .field(tag::TARGET_COMP_ID, target)
                .slot(tag::MSG_SEQ_NUM)
                .slot(tag::SENDING_TIME)
                // The routing tags, reversed. An `OnBehalfOf` on the way in is
                // a `DeliverTo` on the way out, and `ReverseRoute.def` sends
                // all six one pair at a time.
                .slot(tag::ON_BEHALF_OF_COMP_ID)
                .slot(tag::ON_BEHALF_OF_SUB_ID)
                .slot(tag::ON_BEHALF_OF_LOCATION_ID)
                .slot(tag::DELIVER_TO_COMP_ID)
                .slot(tag::DELIVER_TO_SUB_ID)
                .slot(tag::DELIVER_TO_LOCATION_ID)
                .slot(tag::REF_SEQ_NUM)
                .slot(tag::TEXT)
                .slot(tag::REF_TAG_ID)
                .slot(tag::REF_MSG_TYPE)
                .slot(tag::SESSION_REJECT_REASON)
                .build::<Fix44>()
                .ok()?,
            // A Heartbeat carries `112=` only when it answers a TestRequest.
            // An unset slot is not written, which is what makes one template
            // serve both `4a`'s bare heartbeat and `4b`'s reply.
            heartbeat: TemplateBuilder::<24, 320>::new(begin)
                .field(tag::MSG_TYPE, b"0")
                .field(tag::SENDER_COMP_ID, sender)
                .field(tag::TARGET_COMP_ID, target)
                .slot(tag::MSG_SEQ_NUM)
                .slot(tag::SENDING_TIME)
                .slot(tag::TEST_REQ_ID)
                .build::<Fix44>()
                .ok()?,
            test_request: TemplateBuilder::<24, 320>::new(begin)
                .field(tag::MSG_TYPE, b"1")
                .field(tag::SENDER_COMP_ID, sender)
                .field(tag::TARGET_COMP_ID, target)
                .slot(tag::MSG_SEQ_NUM)
                .slot(tag::SENDING_TIME)
                .slot(tag::TEST_REQ_ID)
                .build::<Fix44>()
                .ok()?,
            resend_request: TemplateBuilder::<24, 320>::new(begin)
                .field(tag::MSG_TYPE, b"2")
                .field(tag::SENDER_COMP_ID, sender)
                .field(tag::TARGET_COMP_ID, target)
                .slot(tag::MSG_SEQ_NUM)
                .slot(tag::SENDING_TIME)
                .slot(tag::BEGIN_SEQ_NO)
                .slot(tag::END_SEQ_NO)
                .build::<Fix44>()
                .ok()?,
            // A `SequenceReset` sent as a gap fill: it stands in for messages
            // this session will not replay, so it carries `43=Y` and the
            // `122=` a resent message would have carried.
            gap_fill: TemplateBuilder::<24, 320>::new(begin)
                .field(tag::MSG_TYPE, b"4")
                .field(tag::SENDER_COMP_ID, sender)
                .field(tag::TARGET_COMP_ID, target)
                .slot(tag::MSG_SEQ_NUM)
                .slot(tag::POSS_DUP_FLAG)
                .slot(tag::SENDING_TIME)
                .slot(tag::ORIG_SENDING_TIME)
                .slot(tag::NEW_SEQ_NO)
                .slot(tag::GAP_FILL_FLAG)
                .build::<Fix44>()
                .ok()?,
            buf: [0; 512],
        })
    }
}

use crate::tag;

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a test asserting a constant is not a library call site"
)]
mod tests {
    use super::*;

    #[test]
    fn the_widest_configuration_still_builds() {
        // `Config` holds CompIDs up to 32 bytes. If the scratch could not take
        // two of them, `Outbound::new` would answer `None` and the session
        // would refuse every message — fail-closed, but for a configuration a
        // user is entitled to. This is the assertion that keeps `Skeleton`'s
        // second parameter honest.
        let wide = [b'X'; 32];
        assert!(Outbound::new(b"FIX.4.4", &wide, &wide).is_some());
    }

    #[test]
    fn a_comp_id_wider_than_the_scratch_is_refused_not_truncated() {
        let far_too_wide = [b'X'; 250];
        assert!(Outbound::new(b"FIX.4.4", &far_too_wide, &far_too_wide).is_none());
    }
}
