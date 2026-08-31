//! rustls configuration and the handshake driver.
//!
//! The handshake is the one place `ADR-0005` permits allocation, and it is the
//! one place this spike does any. Everything after the key handover is
//! `read(2)` and `write(2)`.

use std::sync::Arc;

use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};

pub type R<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// `AES-128-GCM` only, TLS 1.3 only.
///
/// Not for security — for determinism. kTLS carries a narrower set of cipher
/// suites than rustls will negotiate (ADR-0005, open question 2), so a spike
/// that let the two ends negotiate freely would be measuring the negotiation as
/// much as the kernel.
pub fn provider() -> CryptoProvider {
    let mut p = rustls::crypto::ring::default_provider();
    p.cipher_suites
        .retain(|cs| cs.suite() == rustls::CipherSuite::TLS13_AES_128_GCM_SHA256);
    p
}

pub struct Pki {
    pub cert: CertificateDer<'static>,
    pub key: PrivateKeyDer<'static>,
}

/// A self-signed `localhost` certificate, generated per run.
///
/// Generated rather than committed: a committed certificate expires, and a spike
/// whose re-run fails in a year for a reason unrelated to kTLS is worse than no
/// spike. `CLAUDE.md` §10 — the check has to still be readable later.
pub fn pki() -> R<Pki> {
    let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])?;
    let key = PrivatePkcs8KeyDer::from(ck.signing_key.serialize_der());
    Ok(Pki {
        cert: ck.cert.der().clone(),
        key: PrivateKeyDer::Pkcs8(key),
    })
}

/// `tickets` is the number of TLS 1.3 session tickets the server sends once the
/// handshake completes, and it is a parameter because the spike needs it both
/// ways.
///
/// With tickets on, a client that offloads receive immediately after the
/// handshake meets a handshake record the kernel refuses to decode, which is the
/// `EIO` path `ktls_core::Context` exists for — phase 1 wants that.
///
/// With tickets off, the wire after the handshake carries nothing but what the
/// test sends, which is the only way a wire observation can be attributed —
/// phases 2 and 3 want that. `[measured 2026-08-31]` with tickets on, phase 2's
/// receiver read a 184-byte session-ticket record and asserted "this is a TLS
/// record" against it, passing while the byte it was supposed to be looking at
/// had never been sent.
pub fn server_config(pki: &Pki, tickets: usize) -> R<Arc<ServerConfig>> {
    let mut cfg = ServerConfig::builder_with_provider(Arc::new(provider()))
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_no_client_auth()
        .with_single_cert(vec![pki.cert.clone()], pki.key.clone_key())?;
    // Required by `dangerous_into_kernel_connection`; without it the conversion
    // fails rather than silently handing out no keys.
    cfg.enable_secret_extraction = true;
    cfg.send_tls13_tickets = tickets;
    Ok(Arc::new(cfg))
}

pub fn client_config(pki: &Pki) -> R<Arc<ClientConfig>> {
    let mut roots = RootCertStore::empty();
    roots.add(pki.cert.clone())?;
    let mut cfg = ClientConfig::builder_with_provider(Arc::new(provider()))
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_root_certificates(roots)
        .with_no_client_auth();
    cfg.enable_secret_extraction = true;
    Ok(Arc::new(cfg))
}

/// Hand the negotiated keys to the kernel.
///
/// `tx` and `rx` are separate because two of this spike's checks deliberately
/// enable one direction and not the other — that asymmetry is what makes the
/// wire observation and the reversal mean anything.
pub fn hand_keys_to_kernel<S: std::os::fd::AsFd>(
    sock: &S,
    secrets: rustls::ExtractedSecrets,
    version: rustls::ProtocolVersion,
    tx: bool,
    rx: bool,
) -> R<()> {
    ktls_core::setup_ulp(sock)?;

    let secrets = ktls_core::ExtractedSecrets::try_from(secrets)?;
    let version = ktls_core::ProtocolVersion::from(version);

    if tx {
        ktls_core::TlsCryptoInfoTx::new(version, secrets.tx.1, secrets.tx.0)?.set(sock)?;
    }
    if rx {
        ktls_core::TlsCryptoInfoRx::new(version, secrets.rx.1, secrets.rx.0)?.set(sock)?;
    }
    Ok(())
}

/// Drive an unbuffered rustls connection through its handshake on a
/// non-blocking socket, with no runtime and no blocking call.
///
/// A macro rather than a function because rustls implements
/// `process_tls_records` separately on the client and server connection types
/// rather than through a shared trait; there is nothing to be generic over.
///
/// Evaluates to `R<(leftover_bytes, socket_reads)>`. `leftover_bytes` must be
/// zero at the handover: bytes still sitting in *this* buffer are ciphertext the
/// kernel will never see, and the kernel's receive sequence number will already
/// have counted them.
#[macro_export]
macro_rules! drive_handshake {
    ($conn:expr, $sock:expr) => {{
        use rustls::unbuffered::{ConnectionState, UnbufferedStatus};

        let mut incoming = vec![0u8; 32 * 1024];
        let mut used: usize = 0;
        let mut outgoing = vec![0u8; 32 * 1024];
        let mut out_used: usize = 0;
        let mut reads: usize = 0;

        loop {
            let UnbufferedStatus { discard, state } =
                $conn.process_tls_records(&mut incoming[..used]);

            let mut want_read = false;
            let mut done = false;
            let mut failed: Option<String> = None;

            match state {
                Ok(ConnectionState::EncodeTlsData(mut s)) => {
                    match s.encode(&mut outgoing[out_used..]) {
                        Ok(n) => out_used += n,
                        Err(e) => failed = Some(format!("encode: {e:?}")),
                    }
                }
                Ok(ConnectionState::TransmitTlsData(s)) => {
                    if let Err(e) = $crate::net::spin_write_all(&mut $sock, &outgoing[..out_used]) {
                        failed = Some(format!("transmit: {e}"));
                    }
                    out_used = 0;
                    s.done();
                }
                Ok(ConnectionState::BlockedHandshake) => want_read = true,
                Ok(ConnectionState::WriteTraffic(_)) => done = true,
                Ok(other) => failed = Some(format!("unexpected handshake state: {other:?}")),
                Err(e) => failed = Some(format!("rustls: {e}")),
            }

            if let Some(e) = failed {
                break ::core::result::Result::<
                    (usize, usize),
                    Box<dyn ::std::error::Error + Send + Sync>,
                >::Err(e.into());
            }

            if discard > 0 {
                incoming.copy_within(discard..used, 0);
                used -= discard;
            }

            // Break only once nothing more was consumed. A server emits its
            // TLS 1.3 session tickets after reaching WriteTraffic, and
            // `dangerous_into_kernel_connection` refuses a connection with TLS
            // data still queued to send.
            if done && discard == 0 {
                break ::core::result::Result::Ok((used, reads));
            }

            if want_read {
                if out_used > 0 {
                    if let Err(e) = $crate::net::spin_write_all(&mut $sock, &outgoing[..out_used]) {
                        break ::core::result::Result::Err(
                            format!("flush before read: {e}").into(),
                        );
                    }
                    out_used = 0;
                }
                match $crate::net::spin_read(&mut $sock, &mut incoming[used..]) {
                    Ok(n) => {
                        used += n;
                        reads += 1;
                    }
                    Err(e) => {
                        break ::core::result::Result::Err(format!("read: {e}").into());
                    }
                }
            }
        }
    }};
}
