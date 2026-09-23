//! `--role dial` — this engine's **real initiator door**, driven from a
//! settings file, for the `initiator-plain` and `initiator-tls` arms
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
//! `fixbolt_engine::connect_and_serve_tls` (behind the `tls` feature, and not
//! re-exported by the `fixbolt` facade), and plaintext has to be the same
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

use fixbolt::Settings;

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
    // `--role acceptor` already makes about `Settings::load`. A file carrying
    // `SocketUseSSL=Y` in a build without the `tls` feature stops here too:
    // `Settings::load` refuses it with `NeedsFeature` rather than dialling the
    // venue in plaintext.
    let settings = match Settings::load(&cfg_path) {
        Ok(s) => s,
        Err(e) => {
            println!("interop: FAIL settings {cfg_path}: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    // `[2026-09-23]` the `initiator-tls` arm: the same role, the same file
    // shape, the same `Desk`, one door different — ADR-0130's reason for
    // building this role at all.
    #[cfg(all(feature = "tls", target_os = "linux"))]
    if settings.client_tls().is_some() {
        return run_tls(&cfg_path, settings);
    }

    let (session_cfg, addr, policy) = match settings.into_initiator() {
        Ok(t) => t,
        Err(e) => {
            println!("interop: FAIL settings {cfg_path}: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    println!("interop: fixbolt dial -> {addr}");

    let handles = fixbolt::Handles::new();
    crate::stop_on_stdin(handles.admin());
    let events = crate::Events::watch(&handles);

    let served = fixbolt::connect_and_serve::<_, fixbolt::Store, fixbolt::NoRecovery, fixbolt::NoLog>(
        &addr,
        session_cfg,
        fixbolt::app(crate::desk::Desk::default()),
        policy,
        fixbolt::NoRecovery,
        fixbolt::NoLog,
        handles,
    );
    events.finish();
    stopped(&addr, "connect_and_serve", served)
}

/// **`--role dial` over TLS** — [`fixbolt_engine::connect_and_serve_tls`],
/// brought up from the file exactly as a deployment would: `into_tls_initiator`
/// for the session, the address and the policy, then
/// [`fixbolt_engine::tls::load_client_pem`] for the roots and the name the
/// venue's certificate must carry.
///
/// **The name is `SocketConnectHost` as written** — the host part of the dial
/// address `into_tls_initiator` hands back. `127.0.0.1` becomes
/// `ServerName::IpAddress`, so the judge's certificate needs an IP SAN, which
/// `scripts/interop-qfj.sh` §2 gives it. `TlsRequireKernel` travels inside
/// `ClientTls`, read from the file, not forced here.
#[cfg(all(feature = "tls", target_os = "linux"))]
fn run_tls(cfg_path: &str, settings: Settings) -> std::process::ExitCode {
    let (session_cfg, addr, policy, tls_settings) = match settings.into_tls_initiator() {
        Ok(t) => t,
        Err(e) => {
            println!("interop: FAIL settings {cfg_path}: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let host = addr.rsplit_once(':').map_or(addr.as_str(), |(h, _)| h);
    let tls = match fixbolt_engine::tls::load_client_pem(&tls_settings, host) {
        Ok(t) => t,
        Err(e) => {
            println!("interop: FAIL certificate {cfg_path}: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    println!(
        "interop: fixbolt dial -> {addr}, TLS, TlsRequireKernel={}",
        if tls.require_kernel { "Y" } else { "N" }
    );

    let handles = fixbolt::Handles::new();
    crate::stop_on_stdin(handles.admin());
    let events = crate::Events::watch(&handles);

    let served = fixbolt_engine::connect_and_serve_tls::<
        _,
        fixbolt::Store,
        fixbolt::NoRecovery,
        fixbolt::NoLog,
    >(
        &addr,
        session_cfg,
        fixbolt::app(crate::desk::Desk::default()),
        policy,
        fixbolt::NoRecovery,
        fixbolt::NoLog,
        handles,
        tls,
    );
    events.finish();
    stopped(&addr, "connect_and_serve_tls", served)
}

/// The one line `scripts/interop-qfj.sh` reads for `shutdown`, or the error.
fn stopped(
    addr: &str,
    door: &str,
    served: Result<fixbolt::Shutdown, fixbolt::ServeError>,
) -> std::process::ExitCode {
    match served {
        Ok(shutdown) => {
            println!("interop: dial stopped: {shutdown:?}");
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            println!("interop: FAIL {door} {addr}: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
