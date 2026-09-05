//! A file can configure the other role, and the twelve keys a FIX desk expects.
//!
//! Step 4 of [settings-for-both-roles], written to be red.
//!
//! `[verified 2026-09-05]` before this, `engine::settings` accepted **eleven
//! keys, all of them the acceptor's**. There was no way to declare an initiator
//! from a file — no host, no port, no reconnect rule — so the one role that
//! *dials* was the one role that needed a rebuild to point somewhere else.
//!
//! # A file names one role, and the wrong door says so with a line number
//!
//! `into_table` and `into_initiator` are the two doors. A file that declares
//! the other role is refused **by line**, not by returning something plausible:
//! an initiator file poured into `into_table` would build a perfectly good
//! acceptor that serves the venue instead of dialling it, and every symptom of
//! that is a phone call at four in the afternoon.
//!
//! That refusal is also the answer to the sharded question the plan asks
//! separately: `serve_sharded_hft` takes a `Table` and nothing else, so an
//! initiator file reaches it through `into_table` and meets the same line
//! number. **One mechanism, not two** — a second check would be a second rule
//! to disagree with the first.
//!
//! [settings-for-both-roles]: ../../../docs/plans/2026-09-04-settings-for-both-roles.md
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fixbolt_engine::reconnect::Next;
use fixbolt_engine::settings::{ConnectionType, Problem, Settings};

const ACCEPTOR: &str = "\
[DEFAULT]
BeginString=FIX.4.4
SenderCompID=ISLD

[SESSION]
TargetCompID=TW44
";

const INITIATOR: &str = "\
[DEFAULT]
ConnectionType=initiator
BeginString=FIX.4.4
SenderCompID=TW44

[SESSION]
TargetCompID=ISLD
SocketConnectHost=fix.venue.example
SocketConnectPort=9876
";

fn err(text: &str) -> fixbolt_engine::settings::SettingsError {
    Settings::parse(text).expect_err("this file should be refused")
}

// ---------------------------------------------------------------------------
// The two doors
// ---------------------------------------------------------------------------

#[test]
fn a_file_without_a_connection_type_is_still_an_acceptor() {
    // Every file written before today, and the default has to stay this way:
    // 39 existing tests are acceptor files with no `ConnectionType` in them.
    let s = Settings::parse(ACCEPTOR).expect("parses");
    assert_eq!(s.connection_type(), ConnectionType::Acceptor);
    assert_eq!(s.into_table().expect("an acceptor file").len(), 1);
}

#[test]
fn an_initiator_file_becomes_a_config_an_address_and_a_policy() {
    let s = Settings::parse(INITIATOR).expect("parses");
    assert_eq!(s.connection_type(), ConnectionType::Initiator);

    let (cfg, addr, _policy) = s.into_initiator().expect("an initiator file");
    assert_eq!(cfg.sender_comp_id(), b"TW44");
    assert_eq!(cfg.target_comp_id(), b"ISLD");
    assert_eq!(addr, "fix.venue.example:9876");
}

#[test]
fn the_host_is_kept_as_written_and_not_resolved_here() {
    // **Deliberately a string and not a `SocketAddr`.** Resolving here would
    // pin the engine to one IP at startup; `TcpStream::connect` takes the text
    // and resolves it on every dial, which is what a venue with a DNS failover
    // needs. A parser that does name lookups is also a parser that blocks.
    let s = Settings::parse(INITIATOR).expect("parses");
    let (_, addr, _) = s.into_initiator().expect("an initiator file");
    assert_eq!(
        addr, "fix.venue.example:9876",
        "the name survives to the dial"
    );
}

#[test]
fn an_initiator_file_refuses_into_table_and_names_the_line() {
    let e = Settings::parse(INITIATOR)
        .expect("parses")
        .into_table()
        .expect_err("an initiator file is not a table of counterparties");
    assert_eq!(*e.problem(), Problem::WrongRole);
    assert_eq!(e.line(), 2, "the ConnectionType= line, not the end of file");
    assert!(
        e.to_string().contains("into_initiator"),
        "and it says which door to use instead: {e}"
    );
}

#[test]
fn an_acceptor_file_refuses_into_initiator_and_names_the_line() {
    let e = Settings::parse(ACCEPTOR)
        .expect("parses")
        .into_initiator()
        .expect_err("an acceptor file has nowhere to dial");
    assert_eq!(*e.problem(), Problem::WrongRole);
    assert!(
        e.to_string().contains("into_table"),
        "and it says which door to use instead: {e}"
    );
}

// ---------------------------------------------------------------------------
// A file that half-declares a role
// ---------------------------------------------------------------------------

#[test]
fn a_dialling_key_in_an_acceptor_file_is_a_line_numbered_error() {
    let e = err(&format!("{ACCEPTOR}SocketConnectHost=fix.venue.example\n"));
    assert_eq!(*e.problem(), Problem::WrongRole);
    assert_eq!(e.line(), 7);
    assert!(e.to_string().contains("SocketConnectHost"), "{e}");
}

#[test]
fn an_initiator_without_a_host_is_a_missing_key() {
    let text = INITIATOR.replace("SocketConnectHost=fix.venue.example\n", "");
    let e = err(&text);
    assert_eq!(*e.problem(), Problem::MissingKey);
    assert!(e.to_string().contains("SocketConnectHost"), "{e}");
}

#[test]
fn an_initiator_with_two_sessions_is_a_line_numbered_error() {
    // An initiator holds one session; `connect_and_serve` takes one `Config`.
    // Two `[SESSION]` blocks is a file whose author expected something this
    // engine does not do, and picking the first would be a guess.
    let text = format!(
        "{INITIATOR}\n[SESSION]\nTargetCompID=BANZAI\nSocketConnectHost=b\nSocketConnectPort=1\n"
    );
    let e = err(&text);
    assert_eq!(*e.problem(), Problem::OneSessionPerInitiator);
    assert_eq!(e.line(), 12, "the second [SESSION]'s TargetCompID");
}

// ---------------------------------------------------------------------------
// The reconnect rule, in seconds because QuickFIX writes seconds
// ---------------------------------------------------------------------------

#[test]
fn reconnect_interval_is_seconds_and_reaches_the_policy_as_milliseconds() {
    let text = format!("{INITIATOR}ReconnectInterval=2\nReconnectCeiling=32\n");
    let s = Settings::parse(&text).expect("parses");
    let (_, _, mut policy) = s.into_initiator().expect("an initiator file");

    policy.dropped(0);
    assert_eq!(
        policy.next(0),
        Next::At(2_000),
        "ReconnectInterval=2 is two seconds, and Policy counts milliseconds"
    );
    for _ in 0..20 {
        policy.dropped(0);
    }
    assert_eq!(policy.next(0), Next::At(32_000), "and the ceiling is 32 s");
}

#[test]
fn a_reconnect_ceiling_below_the_interval_is_a_line_numbered_error() {
    let text = format!("{INITIATOR}ReconnectInterval=30\nReconnectCeiling=5\n");
    let e = err(&text);
    assert_eq!(*e.problem(), Problem::ImpossiblePolicy);
    assert_eq!(e.line(), 11, "the ReconnectCeiling= line");
}

#[test]
fn the_default_ceiling_is_sixteen_times_the_interval() {
    let text = format!("{INITIATOR}ReconnectInterval=2\n");
    let (_, _, mut policy) = Settings::parse(&text)
        .expect("parses")
        .into_initiator()
        .expect("an initiator file");
    for _ in 0..20 {
        policy.dropped(0);
    }
    assert_eq!(policy.next(0), Next::At(32_000), "16 x 2 s");
}

// ---------------------------------------------------------------------------
// The keys that reach `Config`
// ---------------------------------------------------------------------------

#[test]
fn the_three_reset_keys_reach_the_config() {
    let text = format!("{ACCEPTOR}ResetOnLogon=Y\nResetOnDisconnect=Y\n");
    let s = Settings::parse(&text).expect("parses");
    let reset = s.configs()[0].reset();
    assert!(reset.resets_on_logon());
    assert!(reset.resets_on_disconnect());
    assert!(
        !reset.resets_on_logout(),
        "a key that was not written stays off"
    );
}

#[test]
fn the_two_timeouts_are_seconds_and_reach_the_config_as_milliseconds() {
    let text = format!("{ACCEPTOR}LogonTimeout=10\nLogoutTimeout=5\n");
    let cfg = Settings::parse(&text).expect("parses").configs()[0];
    assert_eq!(cfg.logon_timeout_ms(), 10_000);
    assert_eq!(cfg.logout_timeout_ms(), 5_000);
}

#[test]
fn the_two_validation_knobs_reach_the_config() {
    let text = format!("{ACCEPTOR}AllowUnknownMsgFields=Y\nValidateUserDefinedFields=N\n");
    let cfg = Settings::parse(&text).expect("parses").configs()[0];
    assert!(cfg.validation().allows_unknown_msg_fields());
    assert!(cfg.validation().skips_user_defined_fields());
}

#[test]
fn validate_user_defined_fields_reads_y_as_keep_checking() {
    // The one key whose `Y` means *do the work* rather than *skip it*. Getting
    // this backwards would silently accept every tag above 5000 on a desk that
    // wrote the safer value.
    let text = format!("{ACCEPTOR}ValidateUserDefinedFields=Y\n");
    let cfg = Settings::parse(&text).expect("parses").configs()[0];
    assert!(!cfg.validation().skips_user_defined_fields());
}

#[test]
fn a_flag_that_is_not_y_or_n_is_a_line_numbered_error() {
    let e = err(&format!("{ACCEPTOR}ResetOnLogon=true\n"));
    assert_eq!(*e.problem(), Problem::NotAFlag);
    assert_eq!(e.line(), 7);
    assert!(e.to_string().contains("ResetOnLogon=true"), "{e}");
}

#[test]
fn a_connection_type_that_is_neither_role_is_a_line_numbered_error() {
    let e = err(
        "[DEFAULT]\nConnectionType=both\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\n[SESSION]\nTargetCompID=TW44\n",
    );
    assert_eq!(*e.problem(), Problem::BadConnectionType);
    assert_eq!(e.line(), 2);
}

#[test]
fn connection_type_belongs_in_default_because_a_file_names_one_role() {
    let e = err(
        "[DEFAULT]\nBeginString=FIX.4.4\nSenderCompID=ISLD\n\n[SESSION]\nTargetCompID=TW44\nConnectionType=initiator\n",
    );
    assert_eq!(*e.problem(), Problem::DefaultOnly);
    assert_eq!(e.line(), 7);
}
