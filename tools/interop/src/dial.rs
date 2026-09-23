//! `--role dial` — this engine's **real initiator door**, driven from a
//! settings file, for the `initiator-plain` (and later `initiator-tls`) arms
//! of `scripts/interop-qfj.sh`.
//!
//! # Why this is a fourth role and not a flag on `--role initiator`
//!
//! `--role initiator` in `main.rs` drives [`fixbolt_session::Session`] by hand
//! over a blocking [`std::net::TcpStream`] — its own rustdoc says so: *not
//! under test: the engine's polling loop*. That is exactly right for
//! `scripts/interop.sh`'s C++ direction, and exactly wrong for
//! [ADR-0130](../../../docs/decisions/ADR-0130-a-jvm-enters-ci-as-a-second-oracle-quickfixj-by-pinned-jar-and-our-own-judge.md),
//! whose plan says why the `initiator-plain` arm has to run the engine's own
//! door: TLS on the initiator side exists **only** inside
//! [`fixbolt::connect_and_serve_tls`], and plaintext has to be the same
//! kind of run as TLS — the same settings file, the same application, the
//! same `--role dial` — or a TLS-only failure could not be told apart from a
//! difference in how the two arms were driven.
//!
//! So this role reads a settings file exactly as `--role acceptor` does, and
//! calls [`fixbolt::connect_and_serve`] with `fixbolt::app(desk::Desk::default())`
//! — the same `Handler` every other role plays the counterparty against — and
//! [`fixbolt::NoRecovery`], because this arm's whole run is one connection: an
//! in-memory recovery has nothing to carry across a reconnect that never
//! happens (`ReconnectInterval` is set high enough in the generated config that
//! one should not be attempted before the judge finishes).
//!
//! # It scores nothing
//!
//! Exactly as `--role acceptor`: the judge is `tools/interop-qfj/Judge.java`,
//! on the other end of the socket, acting as the QuickFIX/J **acceptor** for
//! this arm. This process prints only what it is doing and what it stopped
//! with; `scripts/interop-qfj.sh` reads the judge's transcript for the seven
//! steps and reads *this* process's stdout for the one line that proves the
//! engine came back through the front door.

use fixbolt::{Config, Settings};

/// Stopped through `Admin::shutdown`, on stdin — the front door
/// `scripts/interop-qfj.sh` uses for every long-running role in this binary.
/// See `main.rs::stop_on_stdin` for why a line on stdin and not a signal.
pub fn run(args: &[String]) -> std::process::ExitCode {
    let Some(cfg_path) = crate::arg(args, "--cfg") else {
        println!("interop: FAIL --role dial needs --cfg <settings file>");
        return std::process::ExitCode::FAILURE;
    };

    // A mistyped key, a mistyped path, or a file describing an acceptor all
    // stop here with the line and what was written — the same argument
    // `--role acceptor` already makes about `Settings::load`. `into_initiator`
    // is the plaintext door; the TLS door (`into_tls_initiator`) is added in
    // the plan's next step, behind the `tls` feature, on this exact call site.
    let (session_cfg, addr, policy): (Config, String, fixbolt::reconnect::Policy) =
        match Settings::load(&cfg_path).and_then(Settings::into_initiator) {
            Ok(t) => t,
            Err(e) => {
                println!("interop: FAIL settings {cfg_path}: {e}");
                return std::process::ExitCode::FAILURE;
            }
        };

    println!("interop: fixbolt dial -> {addr}");

    let handles = fixbolt::Handles::new();
    crate::stop_on_stdin(handles.admin());

    match fixbolt::connect_and_serve::<_, fixbolt::Store, fixbolt::NoRecovery, fixbolt::NoLog>(
        &addr,
        session_cfg,
        fixbolt::app(crate::desk::Desk::default()),
        policy,
        fixbolt::NoRecovery,
        fixbolt::NoLog,
        handles,
    ) {
        Ok(shutdown) => {
            println!("interop: dial stopped: {shutdown:?}");
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            println!("interop: FAIL connect_and_serve {addr}: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
