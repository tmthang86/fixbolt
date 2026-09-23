//! The FIXP spike's probe (ADR-0140 decision 1; docs/plans/2026-09-23-p3-fixp-spike.md
//! row 3): a Rust client that speaks B3 Binary EntryPoint to Artio's acceptor
//! through **our** tables — `fixbolt-sbe-gen`'s output for the schema inside
//! the pinned `artio-binary-entrypoint-codecs-0.184.jar` — and **our** writer
//! and reader, `fixbolt-sbe`.
//!
//! **One straight-line program, not a session.** No state type, no
//! retransmission, no timer beyond a read deadline. Each arm is a fixed script:
//!
//! - `accept`: `negotiate` (send Negotiate, read NegotiateResponse),
//!   `establish` (send Establish, read EstablishAck), `terminate` (send
//!   Terminate), `echo` (read the referee's Terminate), `eof` (the referee
//!   closes the socket). Five lines, each `accept: <step> ok` or
//!   `accept: <step> FAIL: <why>`; the first FAIL stops the arm.
//! - `reject-timestamp`: a Negotiate whose timestamp is an hour old must come
//!   back as NegotiateReject `INVALID_TIMESTAMP` — Artio's own check.
//! - `reject-credentials`: a Negotiate whose `credentials` the referee does not
//!   expect must come back as NegotiateReject `CREDENTIALS` — our referee's
//!   check (`spikes/fixp-probe/referee/Referee.java`).
//!
//! Every field of every message read back is compared, so the check runs in
//! both directions: the referee (Real Logic's generated decoder) judges our
//! encoder field by field, and this program (our generated tables) judges
//! Artio's encoder. `scripts/fixp-spike.sh` reads the printed lines, never the
//! exit status alone.
//!
//! **No offset is written by hand** (non-negotiable 5): every field goes
//! through a layout generated from the schema, found by its schema `id`. The
//! one exception is the 4-byte framing header, which is not an SBE message —
//! see [`FRAME_ENCODING_TYPE`].

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use fixbolt_sbe::{
    Block, FieldLayout, MessageLayout, MessageWriter, SbeView, Schema, Value, VarDataLayout,
};

/// The tables `build.rs` generated from `vendor/fixp/binary_entrypoint.xml`.
mod be {
    include!(concat!(env!("OUT_DIR"), "/binary_entrypoint.rs"));
}
use be::B3EntrypointFixpSbe as B3;

// ---- The framing header -------------------------------------------------------------------------

/// B3's framing header, as Artio 0.184 writes and reads it
/// (`artio-codecs/.../fixp/SimpleOpenFramingHeader.java`, `writeSofh` / `readSofh`): a `u16`
/// **little-endian** message length that **includes** these four bytes, then a `u16`
/// little-endian encoding type `0xEB50`. It is not the FIXP standard's SOFH (a `u32` big-endian
/// length and a `u16` big-endian type); the schema calls it "Compact Simple Open Framing Header"
/// (`<composite name="FramingHeader">`). Artio refuses any other encoding type with
/// `IllegalArgumentException: Unsupported Encoding Type` (plan row 3, reversal A).
const FRAME_ENCODING_TYPE: u16 = 0xEB50;
/// Bytes in the framing header.
const FRAME_LEN: usize = 4;

// ---- Schema identifiers (the `id` attributes of binary_entrypoint.xml 5.6) ----------------------

const NEGOTIATE: u16 = 1;
const NEGOTIATE_RESPONSE: u16 = 2;
const NEGOTIATE_REJECT: u16 = 3;
const ESTABLISH: u16 = 4;
const ESTABLISH_ACK: u16 = 5;
const ESTABLISH_REJECT: u16 = 6;
const TERMINATE: u16 = 7;

const SESSION_ID: u16 = 35518;
const SESSION_VER_ID: u16 = 35519;
const TIMESTAMP: u16 = 35520;
const REQUEST_TIMESTAMP: u16 = 35521;
const ENTERING_FIRM: u16 = 35501;
const CLIENT_FLOW: u16 = 35516;
const SERVER_FLOW: u16 = 35524;
const NEGOTIATION_REJECT_CODE: u16 = 35522;
const KEEP_ALIVE_INTERVAL: u16 = 35525;
const NEXT_SEQ_NO: u16 = 35526;
const LAST_INCOMING_SEQ_NO: u16 = 35527;
const CANCEL_ON_DISCONNECT_TYPE: u16 = 35002;
const ESTABLISHMENT_REJECT_CODE: u16 = 35532;
const TERMINATION_CODE: u16 = 35533;

const CREDENTIALS: u16 = 35512;
const CLIENT_IP: u16 = 35513;
const CLIENT_APP_NAME: u16 = 35514;
const CLIENT_APP_VERSION: u16 = 35515;

// ---- Enum values the arms expect (validValue elements of the same schema) -----------------------

/// `FlowType.RECOVERABLE` — `NegotiateResponse.serverFlow`, a constant.
const FLOW_RECOVERABLE: u64 = 1;
/// `FlowType.IDEMPOTENT` — `clientFlow`, a constant.
const FLOW_IDEMPOTENT: u64 = 3;
/// `NegotiationRejectCode.CREDENTIALS` — what Artio sends when the authentication strategy calls
/// `reject()` with no code (`BinaryEntryPointProxy.encodeReject`).
const REJECT_CREDENTIALS: u64 = 1;
/// `NegotiationRejectCode.INVALID_TIMESTAMP`.
const REJECT_INVALID_TIMESTAMP: u64 = 7;
/// `TerminationCode.FINISHED` — sent by the probe, and echoed by Artio's `onTerminate`.
const TERMINATION_FINISHED: u64 = 1;
const CODTIMEOUT_WINDOW: u16 = 35003;
const ONBEHALF_FIRM: u16 = 35517;

/// `EstablishAck.nextSeqNo` the probe requires back. Artio 0.184 writes the **client's**
/// `Establish.nextSeqNo` here (`InternalBinaryEntryPointConnection.onEstablish` passes its own
/// `nextSeqNo` parameter to `sendEstablishAck`) — observed: the script sends a value with no zero
/// byte, not 1, and it comes back.
fn expected_ack_next_seq_no(cfg: &Config) -> u64 {
    cfg.next_seq_no
}
/// `EstablishAck.lastIncomingSeqNo`: nothing application-level was sent yet.
const EXPECTED_ACK_LAST_INCOMING_SEQ_NO: u64 = 0;

/// How far back `reject-timestamp` dates its Negotiate: one hour, against Artio's default
/// two-minute sending-time window, which the referee prints.
const STALE_BY: Duration = Duration::from_secs(3600);

// ---- Configuration ------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arm {
    Accept,
    RejectTimestamp,
    RejectCredentials,
}

impl Arm {
    fn name(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::RejectTimestamp => "reject-timestamp",
            Self::RejectCredentials => "reject-credentials",
        }
    }
}

/// What the probe sends, and the one thing it expects that it does not send.
#[derive(Debug)]
struct Config {
    arm: Arm,
    addr: SocketAddr,
    deadline: Duration,
    session_id: u64,
    session_ver_id: u64,
    entering_firm: u64,
    /// `Negotiate.onbehalfFirm` — sent non-null so its four bytes are on the wire and judged.
    onbehalf_firm: u64,
    credentials: String,
    client_ip: String,
    client_app_name: String,
    client_app_version: String,
    /// `Establish.keepAliveInterval`, milliseconds.
    keepalive_ms: u64,
    /// `EstablishAck.keepAliveInterval` expected back: the referee's own
    /// `acceptorFixPKeepaliveTimeoutInMs`, not an echo of ours.
    server_keepalive_ms: u64,
    /// `Establish.nextSeqNo`.
    next_seq_no: u64,
    /// `Establish.cancelOnDisconnectType` (a `CancelOnDisconnectType` value).
    cod_type: u64,
    /// `Establish.codTimeoutWindow`, milliseconds.
    cod_timeout_ms: u64,
}

fn usage() -> String {
    "usage: fixp-probe --arm accept|reject-timestamp|reject-credentials --addr HOST:PORT \
     --deadline-ms N --session-id N --session-ver-id N --entering-firm N --credentials S \
     --onbehalf-firm N --client-ip S --client-app-name S --client-app-version S \
     --keepalive-ms N --server-keepalive-ms N --next-seq-no N --cod-type N --cod-timeout-ms N"
        .to_string()
}

fn parse_args() -> Result<Config, String> {
    let mut arm = None;
    let mut addr = None;
    let mut deadline_ms = None;
    let mut session_id = None;
    let mut session_ver_id = None;
    let mut entering_firm = None;
    let mut credentials = None;
    let mut client_ip = None;
    let mut client_app_name = None;
    let mut client_app_version = None;
    let mut keepalive_ms = None;
    let mut server_keepalive_ms = None;
    let mut onbehalf_firm = None;
    let mut next_seq_no = None;
    let mut cod_type = None;
    let mut cod_timeout_ms = None;

    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("{flag} needs a value\n{}", usage()))?;
        let num = |v: &str| {
            v.parse::<u64>()
                .map_err(|e| format!("{flag} {v:?}: {e}\n{}", usage()))
        };
        match flag.as_str() {
            "--arm" => {
                arm = Some(match value.as_str() {
                    "accept" => Arm::Accept,
                    "reject-timestamp" => Arm::RejectTimestamp,
                    "reject-credentials" => Arm::RejectCredentials,
                    other => return Err(format!("unknown arm {other:?}\n{}", usage())),
                });
            }
            "--addr" => {
                addr = Some(
                    value
                        .parse::<SocketAddr>()
                        .map_err(|e| format!("--addr {value:?}: {e}"))?,
                );
            }
            "--deadline-ms" => deadline_ms = Some(num(&value)?),
            "--session-id" => session_id = Some(num(&value)?),
            "--session-ver-id" => session_ver_id = Some(num(&value)?),
            "--entering-firm" => entering_firm = Some(num(&value)?),
            "--credentials" => credentials = Some(value),
            "--client-ip" => client_ip = Some(value),
            "--client-app-name" => client_app_name = Some(value),
            "--client-app-version" => client_app_version = Some(value),
            "--keepalive-ms" => keepalive_ms = Some(num(&value)?),
            "--server-keepalive-ms" => server_keepalive_ms = Some(num(&value)?),
            "--onbehalf-firm" => onbehalf_firm = Some(num(&value)?),
            "--next-seq-no" => next_seq_no = Some(num(&value)?),
            "--cod-type" => cod_type = Some(num(&value)?),
            "--cod-timeout-ms" => cod_timeout_ms = Some(num(&value)?),
            other => return Err(format!("unknown argument {other:?}\n{}", usage())),
        }
    }

    let need = |name: &str| format!("{name} is required\n{}", usage());
    Ok(Config {
        arm: arm.ok_or_else(|| need("--arm"))?,
        addr: addr.ok_or_else(|| need("--addr"))?,
        deadline: Duration::from_millis(deadline_ms.ok_or_else(|| need("--deadline-ms"))?),
        session_id: session_id.ok_or_else(|| need("--session-id"))?,
        session_ver_id: session_ver_id.ok_or_else(|| need("--session-ver-id"))?,
        entering_firm: entering_firm.ok_or_else(|| need("--entering-firm"))?,
        credentials: credentials.ok_or_else(|| need("--credentials"))?,
        client_ip: client_ip.ok_or_else(|| need("--client-ip"))?,
        client_app_name: client_app_name.ok_or_else(|| need("--client-app-name"))?,
        client_app_version: client_app_version.ok_or_else(|| need("--client-app-version"))?,
        keepalive_ms: keepalive_ms.ok_or_else(|| need("--keepalive-ms"))?,
        server_keepalive_ms: server_keepalive_ms.ok_or_else(|| need("--server-keepalive-ms"))?,
        onbehalf_firm: onbehalf_firm.ok_or_else(|| need("--onbehalf-firm"))?,
        next_seq_no: next_seq_no.ok_or_else(|| need("--next-seq-no"))?,
        cod_type: cod_type.ok_or_else(|| need("--cod-type"))?,
        cod_timeout_ms: cod_timeout_ms.ok_or_else(|| need("--cod-timeout-ms"))?,
    })
}

// ---- Table lookups ------------------------------------------------------------------------------

fn field(layout: &'static MessageLayout, id: u16) -> Result<&'static FieldLayout, String> {
    layout
        .field(id)
        .ok_or_else(|| format!("{} has no field {id}", layout.name))
}

fn var_data(layout: &'static MessageLayout, id: u16) -> Result<&'static VarDataLayout, String> {
    layout
        .var_data
        .iter()
        .find(|v| v.id == id)
        .ok_or_else(|| format!("{} has no varData {id}", layout.name))
}

/// Index of composite member `name` in `field`'s element table.
fn member_index(field: &FieldLayout, name: &str) -> Result<usize, String> {
    field
        .elements
        .iter()
        .position(|e| e.name == name)
        .ok_or_else(|| format!("{} has no member {name:?}", field.name))
}

// ---- Writing ------------------------------------------------------------------------------------

/// Nanoseconds since the Unix epoch, now.
fn now_ns() -> Result<u64, String> {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("clock before 1970: {e}"))?;
    u64::try_from(d.as_nanos()).map_err(|e| format!("clock past 2554: {e}"))
}

/// Encodes message `template_id` through `fill` behind its framing header and sends it.
fn send<F>(stream: &mut TcpStream, template_id: u16, fill: F) -> Result<(), String>
where
    F: FnOnce(&mut MessageWriter<'_>, &'static MessageLayout) -> Result<(), String>,
{
    let mut buf = [0u8; 512];
    let (frame, body) = buf
        .split_at_mut_checked(FRAME_LEN)
        .ok_or_else(|| "send buffer shorter than the framing header".to_string())?;
    let mut w = MessageWriter::new::<B3>(body, template_id)
        .map_err(|e| format!("start template {template_id}: {e:?}"))?;
    let layout = w.layout();
    fill(&mut w, layout)?;
    let body_len = w
        .finish()
        .map_err(|e| format!("finish {}: {e:?}", layout.name))?;
    let total = FRAME_LEN + body_len;
    let total_u16 =
        u16::try_from(total).map_err(|_| format!("{} is {total} bytes", layout.name))?;
    let [l0, l1] = total_u16.to_le_bytes();
    let [t0, t1] = FRAME_ENCODING_TYPE.to_le_bytes();
    frame.copy_from_slice(&[l0, l1, t0, t1]);
    let bytes = buf
        .get(..total)
        .ok_or_else(|| format!("{} overran the buffer", layout.name))?;
    stream
        .write_all(bytes)
        .map_err(|e| format!("send {}: {e}", layout.name))
}

fn put_uint(w: &mut MessageWriter<'_>, f: &FieldLayout, v: u64) -> Result<(), String> {
    w.put(f, Value::UInt(v))
        .map_err(|e| format!("put {} = {v}: {e:?}", f.name))
}

fn put_member_uint(
    w: &mut MessageWriter<'_>,
    f: &FieldLayout,
    member: &str,
    v: u64,
) -> Result<(), String> {
    let i = member_index(f, member)?;
    w.put_element(f, i, Some(Value::UInt(v)))
        .map_err(|e| format!("put {}.{member} = {v}: {e:?}", f.name))
}

fn put_var(w: &mut MessageWriter<'_>, v: &VarDataLayout, bytes: &str) -> Result<(), String> {
    w.var_data(v, bytes.as_bytes())
        .map_err(|e| format!("put {} ({} bytes): {e:?}", v.name, bytes.len()))
}

fn send_negotiate(stream: &mut TcpStream, cfg: &Config, timestamp: u64) -> Result<(), String> {
    send(stream, NEGOTIATE, |w, m| {
        put_uint(w, field(m, SESSION_ID)?, cfg.session_id)?;
        put_uint(w, field(m, SESSION_VER_ID)?, cfg.session_ver_id)?;
        put_member_uint(w, field(m, TIMESTAMP)?, "time", timestamp)?;
        put_uint(w, field(m, ENTERING_FIRM)?, cfg.entering_firm)?;
        put_uint(w, field(m, ONBEHALF_FIRM)?, cfg.onbehalf_firm)?;
        put_var(w, var_data(m, CREDENTIALS)?, &cfg.credentials)?;
        put_var(w, var_data(m, CLIENT_IP)?, &cfg.client_ip)?;
        put_var(w, var_data(m, CLIENT_APP_NAME)?, &cfg.client_app_name)?;
        put_var(w, var_data(m, CLIENT_APP_VERSION)?, &cfg.client_app_version)
    })
}

fn send_establish(stream: &mut TcpStream, cfg: &Config, timestamp: u64) -> Result<(), String> {
    send(stream, ESTABLISH, |w, m| {
        put_uint(w, field(m, SESSION_ID)?, cfg.session_id)?;
        put_uint(w, field(m, SESSION_VER_ID)?, cfg.session_ver_id)?;
        put_member_uint(w, field(m, TIMESTAMP)?, "time", timestamp)?;
        put_member_uint(w, field(m, KEEP_ALIVE_INTERVAL)?, "time", cfg.keepalive_ms)?;
        put_uint(w, field(m, NEXT_SEQ_NO)?, cfg.next_seq_no)?;
        put_uint(w, field(m, CANCEL_ON_DISCONNECT_TYPE)?, cfg.cod_type)?;
        put_member_uint(w, field(m, CODTIMEOUT_WINDOW)?, "time", cfg.cod_timeout_ms)?;
        put_var(w, var_data(m, CREDENTIALS)?, &cfg.credentials)
    })
}

fn send_terminate(stream: &mut TcpStream, cfg: &Config) -> Result<(), String> {
    send(stream, TERMINATE, |w, m| {
        put_uint(w, field(m, SESSION_ID)?, cfg.session_id)?;
        put_uint(w, field(m, SESSION_VER_ID)?, cfg.session_ver_id)?;
        put_uint(w, field(m, TERMINATION_CODE)?, TERMINATION_FINISHED)
    })
}

// ---- Reading ------------------------------------------------------------------------------------

/// What `read_exact` failing means here, in words the step line can carry.
fn read_error(what: &str, deadline: Duration, e: &std::io::Error) -> String {
    match e.kind() {
        ErrorKind::WouldBlock | ErrorKind::TimedOut => {
            format!("no {what} within {}ms", deadline.as_millis())
        }
        ErrorKind::UnexpectedEof => format!("EOF where a {what} was due"),
        _ => format!("reading {what}: {e}"),
    }
}

/// Reads one framed message into `buf` and returns its SBE bytes (the frame stripped), after
/// checking the framing header the way Artio's `readSofh` does.
fn recv<'b>(
    stream: &mut TcpStream,
    buf: &'b mut [u8],
    deadline: Duration,
) -> Result<&'b [u8], String> {
    let mut frame = [0u8; FRAME_LEN];
    stream
        .read_exact(&mut frame)
        .map_err(|e| read_error("framing header", deadline, &e))?;
    let [l0, l1, t0, t1] = frame;
    let total = usize::from(u16::from_le_bytes([l0, l1]));
    let encoding = u16::from_le_bytes([t0, t1]);
    if encoding != FRAME_ENCODING_TYPE {
        return Err(format!(
            "framing encoding type {encoding:#06x} != {FRAME_ENCODING_TYPE:#06x}"
        ));
    }
    let body_len = total
        .checked_sub(FRAME_LEN)
        .ok_or_else(|| format!("framing length {total} < {FRAME_LEN}"))?;
    let capacity = buf.len();
    let body = buf
        .get_mut(..body_len)
        .ok_or_else(|| format!("framing length {total} is past the {capacity}-byte buffer"))?;
    stream
        .read_exact(body)
        .map_err(|e| read_error("message body", deadline, &e))?;
    Ok(body)
}

/// A received message, decoded through our tables, with its header checked.
struct Received<'a> {
    layout: &'static MessageLayout,
    root: Block<'a>,
}

fn decode(bytes: &[u8]) -> Result<Received<'_>, String> {
    let v = SbeView::decode::<B3>(bytes).map_err(|e| format!("decode header: {e:?}"))?;
    if v.schema_id() != B3::ID {
        return Err(format!("schemaId {} != {}", v.schema_id(), B3::ID));
    }
    if v.version() != B3::VERSION {
        return Err(format!("version {} != {}", v.version(), B3::VERSION));
    }
    let layout = v
        .layout::<B3>()
        .map_err(|e| format!("templateId {}: {e:?}", v.template_id()))?;
    let wire_block = v
        .block_length(B3::BYTE_ORDER)
        .map_err(|e| format!("blockLength: {e:?}"))?;
    if wire_block != layout.block_length {
        return Err(format!(
            "{} blockLength {wire_block} != {}",
            layout.name, layout.block_length
        ));
    }
    let root = v
        .root::<B3>()
        .map_err(|e| format!("{} root block: {e:?}", layout.name))?;
    Ok(Received { layout, root })
}

impl Received<'_> {
    /// The unsigned value of simple field `id`; `None` when optional and null.
    fn uint(&self, id: u16) -> Result<Option<u64>, String> {
        let f = field(self.layout, id)?;
        match self
            .root
            .value(f)
            .map_err(|e| format!("{}: {e:?}", f.name))?
        {
            None => Ok(None),
            Some(Value::UInt(u)) => Ok(Some(u)),
            Some(other) => Err(format!("{} is {other:?}, not unsigned", f.name)),
        }
    }

    /// Composite field `id`'s member `name`, unsigned.
    fn member_uint(&self, id: u16, name: &str) -> Result<Option<u64>, String> {
        let f = field(self.layout, id)?;
        let r = self
            .root
            .field(f)
            .map_err(|e| format!("{}: {e:?}", f.name))?
            .ok_or_else(|| format!("{} absent", f.name))?;
        match r
            .member(name)
            .map_err(|e| format!("{}.{name}: {e:?}", f.name))?
        {
            None => Ok(None),
            Some(Value::UInt(u)) => Ok(Some(u)),
            Some(other) => Err(format!("{}.{name} is {other:?}, not unsigned", f.name)),
        }
    }

    /// Requires simple field `id` to hold `want`.
    fn expect(&self, id: u16, want: u64) -> Result<(), String> {
        let name = field(self.layout, id)?.name;
        expect_eq(name, self.uint(id)?, want)
    }

    /// Requires composite field `id`'s member `member` to hold `want`.
    fn expect_member(&self, id: u16, member: &str, want: u64) -> Result<(), String> {
        let name = field(self.layout, id)?.name;
        expect_eq(
            &format!("{name}.{member}"),
            self.member_uint(id, member)?,
            want,
        )
    }
}

fn expect_eq(what: &str, got: Option<u64>, want: u64) -> Result<(), String> {
    match got {
        Some(g) if g == want => Ok(()),
        Some(g) => Err(format!("{what} {g} != {want}")),
        None => Err(format!("{what} null != {want}")),
    }
}

/// The name of a rejection, for the FAIL line when a reject arrives where an accept was due.
fn describe_reject(r: &Received<'_>) -> String {
    let (code_id, codes): (u16, &[(u64, &str)]) = match r.layout.template_id {
        NEGOTIATE_REJECT => (
            NEGOTIATION_REJECT_CODE,
            &[
                (0, "UNSPECIFIED"),
                (REJECT_CREDENTIALS, "CREDENTIALS"),
                (2, "FLOWTYPE_NOT_SUPPORTED"),
                (3, "ALREADY_NEGOTIATED"),
                (4, "SESSION_BLOCKED"),
                (5, "INVALID_SESSIONID"),
                (6, "INVALID_SESSIONVERID"),
                (REJECT_INVALID_TIMESTAMP, "INVALID_TIMESTAMP"),
                (8, "INVALID_FIRM"),
                (20, "NEGOTIATE_NOT_ALLOWED"),
            ],
        ),
        ESTABLISH_REJECT => (
            ESTABLISHMENT_REJECT_CODE,
            &[
                (0, "UNSPECIFIED"),
                (1, "CREDENTIALS"),
                (2, "UNNEGOTIATED"),
                (3, "ALREADY_ESTABLISHED"),
                (4, "SESSION_BLOCKED"),
                (5, "INVALID_SESSIONID"),
                (6, "INVALID_SESSIONVERID"),
                (7, "INVALID_TIMESTAMP"),
                (8, "INVALID_KEEPALIVE_INTERVAL"),
                (9, "INVALID_NEXTSEQNO"),
                (10, "ESTABLISH_ATTEMPTS_EXCEEDED"),
                (20, "ESTABLISH_NOT_ALLOWED"),
            ],
        ),
        _ => return r.layout.name.to_string(),
    };
    match r.uint(code_id) {
        Ok(Some(code)) => {
            let name = codes
                .iter()
                .find(|(c, _)| *c == code)
                .map_or("?", |(_, n)| *n);
            format!("{} {name}({code})", r.layout.name)
        }
        Ok(None) => format!("{} (null code)", r.layout.name),
        Err(e) => format!("{} ({e})", r.layout.name),
    }
}

/// Requires `r` to be template `want`, naming a rejection when it is one.
fn expect_template(r: &Received<'_>, want: u16) -> Result<(), String> {
    if r.layout.template_id == want {
        return Ok(());
    }
    match r.layout.template_id {
        NEGOTIATE_REJECT | ESTABLISH_REJECT => Err(describe_reject(r)),
        _ => Err(format!("{} where template {want} was due", r.layout.name)),
    }
}

// ---- The arms -----------------------------------------------------------------------------------

fn connect(cfg: &Config) -> Result<TcpStream, String> {
    let stream = TcpStream::connect_timeout(&cfg.addr, cfg.deadline)
        .map_err(|e| format!("connect {}: {e}", cfg.addr))?;
    stream
        .set_read_timeout(Some(cfg.deadline))
        .map_err(|e| format!("set_read_timeout: {e}"))?;
    stream
        .set_write_timeout(Some(cfg.deadline))
        .map_err(|e| format!("set_write_timeout: {e}"))?;
    stream
        .set_nodelay(true)
        .map_err(|e| format!("set_nodelay: {e}"))?;
    Ok(stream)
}

/// Prints one step line and passes the result on.
fn step(arm: Arm, name: &str, r: Result<(), String>) -> Result<(), ()> {
    match r {
        Ok(()) => {
            println!("{}: {name} ok", arm.name());
            Ok(())
        }
        Err(why) => {
            println!("{}: {name} FAIL: {why}", arm.name());
            Err(())
        }
    }
}

fn arm_accept(cfg: &Config) -> Result<(), ()> {
    let arm = Arm::Accept;
    let mut stream = match connect(cfg) {
        Ok(s) => s,
        Err(why) => return step(arm, "negotiate", Err(why)),
    };
    let mut buf = [0u8; 2048];

    step(arm, "negotiate", {
        (|| {
            let ts = now_ns()?;
            println!("{}: sent Negotiate.timestamp {ts}", arm.name());
            send_negotiate(&mut stream, cfg, ts)?;
            let r = decode(recv(&mut stream, &mut buf, cfg.deadline)?)?;
            expect_template(&r, NEGOTIATE_RESPONSE)?;
            r.expect(SESSION_ID, cfg.session_id)?;
            r.expect(SESSION_VER_ID, cfg.session_ver_id)?;
            r.expect_member(REQUEST_TIMESTAMP, "time", ts)?;
            r.expect(ENTERING_FIRM, cfg.entering_firm)?;
            // Constants: not on the wire, read from our table — what they check is that
            // row 2's generator resolved each field's valueRef to the schema's validValue.
            r.expect(SERVER_FLOW, FLOW_RECOVERABLE)?;
            r.expect(CLIENT_FLOW, FLOW_IDEMPOTENT)
        })()
    })?;

    step(arm, "establish", {
        (|| {
            let ts = now_ns()?;
            println!("{}: sent Establish.timestamp {ts}", arm.name());
            send_establish(&mut stream, cfg, ts)?;
            let r = decode(recv(&mut stream, &mut buf, cfg.deadline)?)?;
            expect_template(&r, ESTABLISH_ACK)?;
            r.expect(SESSION_ID, cfg.session_id)?;
            r.expect(SESSION_VER_ID, cfg.session_ver_id)?;
            r.expect_member(REQUEST_TIMESTAMP, "time", ts)?;
            r.expect_member(KEEP_ALIVE_INTERVAL, "time", cfg.server_keepalive_ms)?;
            r.expect(NEXT_SEQ_NO, expected_ack_next_seq_no(cfg))?;
            r.expect(LAST_INCOMING_SEQ_NO, EXPECTED_ACK_LAST_INCOMING_SEQ_NO)
        })()
    })?;

    step(arm, "terminate", send_terminate(&mut stream, cfg))?;

    step(arm, "echo", {
        (|| {
            let r = decode(recv(&mut stream, &mut buf, cfg.deadline)?)?;
            expect_template(&r, TERMINATE)?;
            r.expect(SESSION_ID, cfg.session_id)?;
            r.expect(SESSION_VER_ID, cfg.session_ver_id)?;
            r.expect(TERMINATION_CODE, TERMINATION_FINISHED)
        })()
    })?;

    step(arm, "eof", {
        let mut one = [0u8; 1];
        match stream.read(&mut one) {
            Ok(0) => Ok(()),
            Ok(_) => Err("bytes after the referee's Terminate".to_string()),
            Err(e) => Err(read_error("EOF", cfg.deadline, &e)),
        }
    })
}

/// Both reject arms: one Negotiate, and a NegotiateReject with `want_code` back, every field
/// checked. The line names the refusal: `<arm>: refused ok: NegotiateReject <CODE>(<n>)`.
fn arm_reject(cfg: &Config, timestamp: u64, want_code: u64) -> Result<(), ()> {
    let arm = cfg.arm;
    let mut buf = [0u8; 2048];
    let result = (|| {
        let mut stream = connect(cfg)?;
        println!("{}: sent Negotiate.timestamp {timestamp}", arm.name());
        send_negotiate(&mut stream, cfg, timestamp)?;
        let r = decode(recv(&mut stream, &mut buf, cfg.deadline)?)?;
        if r.layout.template_id != NEGOTIATE_REJECT {
            return Err(format!("{} where NegotiateReject was due", r.layout.name));
        }
        r.expect(SESSION_ID, cfg.session_id)?;
        r.expect(SESSION_VER_ID, cfg.session_ver_id)?;
        r.expect_member(REQUEST_TIMESTAMP, "time", timestamp)?;
        r.expect(ENTERING_FIRM, cfg.entering_firm)?;
        r.expect(CLIENT_FLOW, FLOW_IDEMPOTENT)?;
        r.expect(NEGOTIATION_REJECT_CODE, want_code)?;
        let named = describe_reject(&r);

        // Not asserted, recorded: whether the acceptor closes the socket after this reject, and
        // how soon (the referee's reject goes through the engine and lingers; Artio's own
        // INVALID_TIMESTAMP reject comes from the library connection).
        let started = Instant::now();
        let mut one = [0u8; 1];
        let after = match stream.read(&mut one) {
            Ok(0) => format!("EOF after {}ms", started.elapsed().as_millis()),
            Ok(_) => "more bytes after the reject".to_string(),
            Err(e) => read_error("EOF", cfg.deadline, &e),
        };
        println!("{}: note: after the reject, {after}", arm.name());
        Ok(named)
    })();
    match result {
        Ok(named) => {
            println!("{}: refused ok: {named}", arm.name());
            Ok(())
        }
        Err(why) => {
            println!("{}: refused FAIL: {why}", arm.name());
            Err(())
        }
    }
}

fn run(cfg: &Config) -> Result<(), ()> {
    match cfg.arm {
        Arm::Accept => arm_accept(cfg),
        Arm::RejectTimestamp => {
            let stale = now_ns()
                .and_then(|now| {
                    u64::try_from(STALE_BY.as_nanos())
                        .ok()
                        .and_then(|stale_by| now.checked_sub(stale_by))
                        .ok_or_else(|| "clock earlier than an hour after 1970".to_string())
                })
                .map_err(|why| println!("{}: refused FAIL: {why}", cfg.arm.name()))?;
            arm_reject(cfg, stale, REJECT_INVALID_TIMESTAMP)
        }
        Arm::RejectCredentials => {
            let ts = now_ns().map_err(|why| println!("{}: refused FAIL: {why}", cfg.arm.name()))?;
            arm_reject(cfg, ts, REJECT_CREDENTIALS)
        }
    }
}

fn main() -> ExitCode {
    let cfg = match parse_args() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(2);
        }
    };
    match run(&cfg) {
        Ok(()) => ExitCode::SUCCESS,
        Err(()) => ExitCode::FAILURE,
    }
}
