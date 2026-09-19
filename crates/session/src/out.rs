//! The messages this layer sends, and the one rule that governs their shape.
//!
//! Non-negotiable 5: **field ordering comes from generated tables, never from a
//! call site.** Every message here is a [`Template`] built once and sorted by
//! `E::Dict`, so no function in this crate ever decides that `49` precedes
//! `52`. The acceptance comparator is positional; a hand-ordered message would
//! pass review and fail the gate.

use fixbolt_codec::{Encoding, Template, TemplateBuilder};

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
///
/// The encoding names the type (ADR-0079): `Template<24, 320>` for tag=value,
/// whatever an encoding with a different wire shape needs. `Outbound::new`
/// still builds a `codec::Template`, so it carries the equality bound that says
/// so — see its own doc.
pub(crate) type Skeleton<E> = <E as Encoding>::Template<24, 320>;

/// Every message a session generates itself, pre-sorted, plus the buffer it
/// writes into.
///
/// Owned by the session so nothing on the send path allocates. 512 bytes is
/// ample for a session message — the longest in the corpus is 101 — and a
/// resend replays stored bytes rather than re-encoding, so it does not size
/// this.
pub(crate) struct Outbound<E: Encoding, const APP: usize = { crate::DEFAULT_APP_SCRATCH }> {
    pub(crate) logon: Skeleton<E>,
    pub(crate) logout: Skeleton<E>,
    pub(crate) reject: Skeleton<E>,
    pub(crate) heartbeat: Skeleton<E>,
    pub(crate) test_request: Skeleton<E>,
    pub(crate) resend_request: Skeleton<E>,
    pub(crate) gap_fill: Skeleton<E>,
    pub(crate) buf: [u8; 512],
    /// Where an [`crate::Application`] writes its reply. Separate from `buf`
    /// because an application message is bigger than a session one: the
    /// longest reply in the corpus is `21_RepeatingGroupSpecifierWithValueOfZero`'s
    /// `35=d` at 177 body bytes, against 101 for the longest `35=D`.
    ///
    /// `[2026-09-05]` **`APP` is the caller's, and it used to be `[u8; 1024]`.**
    /// That literal was the tightest ceiling in the engine and the least
    /// visible: an acceptor could receive 4 KiB and not answer with 1 KiB, and
    /// the symptom was **silence**, because an application that cannot lay out
    /// its reply returns `None` and that is a legal answer. Found by a size
    /// sweep, not by reading —
    /// `docs/reference/a-ceiling-has-more-than-one-floor.md`.
    pub(crate) app: [u8; APP],
}

impl<E, const APP: usize> Outbound<E, APP>
where
    E: Encoding<Template<24, 320> = Template<24, 320>>,
{
    /// `None` if any template cannot be built.
    ///
    /// # Why the bound says `Template<24, 320> = Template<24, 320>`
    ///
    /// [`Encoding`] offers no way to *build* a `Self::Template`: ADR-0079's
    /// four shared operations are parse, view, read a field and encode, and
    /// building an outbound skeleton is none of them. So the seven messages
    /// below are laid out by `codec`'s own [`TemplateBuilder`], and this impl
    /// can only exist for an encoding whose skeleton *is* that type. Every
    /// tag=value encoding is one, FIXT 1.1 included, which is what PR B needs;
    /// an encoding with its own skeleton shape (SBE) gets no session, which is
    /// what ADR-0078 decided. The field **ordering** is still the dictionary's
    /// and never a call site's — `build::<E::Dict>()` below, non-negotiable 5.
    ///
    /// The only ways it answers `None` are a `BeginString` or CompID too long
    /// for the scratch, or a tag `E::Dict` does not know — both configuration
    /// errors.
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
                .slot(tag::LAST_MSG_SEQ_NUM_PROCESSED)
                .slot(tag::ENCRYPT_METHOD)
                .slot(tag::HEART_BT_INT)
                // `SessionReset.def` logs on a second time with `141=Y` and
                // expects it echoed, in the dictionary's order: `98, 108, 141`.
                .slot(tag::RESET_SEQ_NUM_FLAG)
                // `789`, written only when `Config::with_next_expected` is on.
                // An unset slot is not written at all, so an ordinary Logon is
                // byte-for-byte what it was — which is why the 59 definitions
                // cannot see this and `the_position_of_789_is_the_dictionarys_
                // and_not_this_call_sites` has to.
                .slot(tag::NEXT_EXPECTED_MSG_SEQ_NUM)
                .build::<E::Dict>()
                .ok()?,
            logout: TemplateBuilder::<24, 320>::new(begin)
                .field(tag::MSG_TYPE, b"5")
                .field(tag::SENDER_COMP_ID, sender)
                .field(tag::TARGET_COMP_ID, target)
                .slot(tag::MSG_SEQ_NUM)
                .slot(tag::SENDING_TIME)
                .slot(tag::LAST_MSG_SEQ_NUM_PROCESSED)
                .slot(tag::TEXT)
                .build::<E::Dict>()
                .ok()?,
            reject: TemplateBuilder::<24, 320>::new(begin)
                .field(tag::MSG_TYPE, b"3")
                .field(tag::SENDER_COMP_ID, sender)
                .field(tag::TARGET_COMP_ID, target)
                .slot(tag::MSG_SEQ_NUM)
                .slot(tag::SENDING_TIME)
                .slot(tag::LAST_MSG_SEQ_NUM_PROCESSED)
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
                .build::<E::Dict>()
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
                .slot(tag::LAST_MSG_SEQ_NUM_PROCESSED)
                .slot(tag::TEST_REQ_ID)
                .build::<E::Dict>()
                .ok()?,
            test_request: TemplateBuilder::<24, 320>::new(begin)
                .field(tag::MSG_TYPE, b"1")
                .field(tag::SENDER_COMP_ID, sender)
                .field(tag::TARGET_COMP_ID, target)
                .slot(tag::MSG_SEQ_NUM)
                .slot(tag::SENDING_TIME)
                .slot(tag::LAST_MSG_SEQ_NUM_PROCESSED)
                .slot(tag::TEST_REQ_ID)
                .build::<E::Dict>()
                .ok()?,
            resend_request: TemplateBuilder::<24, 320>::new(begin)
                .field(tag::MSG_TYPE, b"2")
                .field(tag::SENDER_COMP_ID, sender)
                .field(tag::TARGET_COMP_ID, target)
                .slot(tag::MSG_SEQ_NUM)
                .slot(tag::SENDING_TIME)
                .slot(tag::LAST_MSG_SEQ_NUM_PROCESSED)
                .slot(tag::BEGIN_SEQ_NO)
                .slot(tag::END_SEQ_NO)
                .build::<E::Dict>()
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
                .slot(tag::LAST_MSG_SEQ_NUM_PROCESSED)
                .slot(tag::ORIG_SENDING_TIME)
                .slot(tag::NEW_SEQ_NO)
                .slot(tag::GAP_FILL_FLAG)
                .build::<E::Dict>()
                .ok()?,
            buf: [0; 512],
            app: [0; APP],
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
    use fixbolt_dict::Fix44TagValue;

    use super::*;

    #[test]
    fn the_widest_configuration_still_builds() {
        // `Config` holds CompIDs up to 32 bytes. If the scratch could not take
        // two of them, `Outbound::new` would answer `None` and the session
        // would refuse every message — fail-closed, but for a configuration a
        // user is entitled to. This is the assertion that keeps `Skeleton`'s
        // second parameter honest.
        let wide = [b'X'; 32];
        assert!(
            Outbound::<Fix44TagValue, { crate::DEFAULT_APP_SCRATCH }>::new(
                b"FIX.4.4", &wide, &wide
            )
            .is_some()
        );
    }

    #[test]
    fn a_comp_id_wider_than_the_scratch_is_refused_not_truncated() {
        let far_too_wide = [b'X'; 250];
        assert!(
            Outbound::<Fix44TagValue, { crate::DEFAULT_APP_SCRATCH }>::new(
                b"FIX.4.4",
                &far_too_wide,
                &far_too_wide
            )
            .is_none()
        );
    }
}
