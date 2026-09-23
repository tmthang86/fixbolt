//! `fixbolt_dict::NOTICE` carries what ADR-0104 decision 5 promises: an
//! application that prints it satisfies the QuickFIX Software License's
//! condition 3 ("in the software itself") and never violates condition 5 by
//! accident, because the exact sentence that names both is asserted here
//! rather than trusted by eye.
//!
//! Checked against **normalised whitespace**: the acknowledgment sentence
//! (condition 3) appears twice in `NOTICE` — once in this crate's own header,
//! once inside the reproduced licence text — wrapped at different column
//! widths in each place. `cargo fmt` does not touch `include_str!` input, so
//! nothing here depends on the crate's own formatting; it depends on the
//! `NOTICE` file's line wrapping instead, and normalising is what keeps that
//! from being a third thing this test is secretly about.
#![allow(clippy::unwrap_used, clippy::panic)]

use fixbolt_dict::NOTICE;

/// Collapse all whitespace (including newlines) to single spaces, so a
/// sentence wrapped at one column width still matches one wrapped at another.
fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn notice_carries_the_condition_3_acknowledgment() {
    let text = normalize(NOTICE);
    assert!(
        text.contains(
            "This product includes software developed by quickfixengine.org (http://www.quickfixengine.org/)."
        ),
        "NOTICE is missing the QuickFIX Software License condition 3 acknowledgment sentence"
    );
}

#[test]
fn notice_carries_the_condition_5_naming_restriction() {
    let text = normalize(NOTICE);
    assert!(
        text.contains(
            "Products derived from this software may not be called \"QuickFIX\", nor may \"QuickFIX\" appear in their name, without prior written permission of quickfixengine.org"
        ),
        "NOTICE is missing the QuickFIX Software License condition 5 naming restriction"
    );
}

#[test]
fn notice_names_the_pin() {
    assert!(
        NOTICE.contains("386ce46e917ae494ab6e90b1be90fd421cdbe3f9"),
        "NOTICE does not name the QuickFIX commit the shipped dictionaries are pinned to"
    );
}

#[test]
fn notice_says_fixbolt_is_not_quickfix() {
    // ADR-0001 decision 5 / QuickFIX licence condition 5: fixbolt does not
    // present itself as QuickFIX or as endorsed by it.
    assert!(
        normalize(NOTICE).contains("fixbolt is not QuickFIX"),
        "NOTICE does not disclaim that fixbolt is QuickFIX or is endorsed by it"
    );
}
