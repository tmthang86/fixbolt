# ADR-0110 — A secret is masked in the message log and leaves only its number in the journal file

- **Status**: Accepted — 2026-09-23 (by the manager under the owner's standing mandate, with the phase 3 row 3 plan). Proposed 2026-09-23
- **Date**: 2026-09-23
- **Deciders**: written by the architect (Opus) for phase 3 row 3; accepted by the manager under
  the owner's standing mandate, or by the owner.
- **Related**: [ADR-0097](ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md)
  decision 1 and exit criterion 4; `DESIGN.md` D7, D14;
  [ADR-0007](ADR-0007-spsc-ring-without-unsafe.md) (the ring the writer threads drain);
  [ADR-0017](ADR-0017-the-inbound-count-is-persisted-after-delivery.md);
  [ADR-0037](ADR-0037-reading-a-journal-is-not-recovering-from-one.md) (the writer may allocate);
  [ADR-0046](ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md) (the ring is
  the resend store; `puts_refused`);
  [ADR-0053](ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md) (the
  outbound mark); `CLAUDE.md` §2 non-negotiables 1, 2, 7; plan
  [2026-09-23-p3-redact-secrets](../plans/2026-09-23-p3-redact-secrets.md)

## Context

The scope plan for phase 3 recorded, *grepped and not run*, that nothing in `crates/` masks
`554=` or `96=`. Reading the two write paths confirms the shape of the leak (2026-09-23, this
worktree at `213e73c`):

- **The message log writes both directions verbatim.** `Conn` calls `MessageLog::record` on every
  frame read, before the session judges it (`crates/engine/src/conn.rs:448`), and on every frame
  queued (`conn.rs:840`). `msglog::write_loop` (`crates/engine/src/msglog.rs:404-497`) escapes
  `\`, `\n`, `\r` and writes everything else as it came. A counterparty's `Logon` carrying
  `553=`/`554=`/`95=`/`96=` therefore lands on disk in clear. This is the acceptor's everyday
  case.
- **The journal keeps outbound application messages only.** The session calls `Journal::put`
  from `send_application` and from the reply to an inbound application message
  (`crates/session/src/lib.rs:3083`, `:3890`; the comment there: *"only application messages are
  kept: QuickFIX never replays an administrative message, it fills over it"*). A `Logon` —
  outbound or inbound — is never put. **So `554=` on a `Logon` cannot reach the journal today.**
  What can: an application message that carries a credential. In the vendored QuickFIX
  dictionaries that is `UserRequest` (`35=BE`), which carries `554` `Password`, `925`
  `NewPassword` and `96` `RawData` in FIX 4.4 and additionally `1402` `EncryptedPassword` and
  `1404` `EncryptedNewPassword` in FIX 5.0 SP2. `FileJournal` writes the bytes it is given
  (`crates/engine/src/journal.rs:755-785`, `:787-826`), so a `UserRequest` the application sends
  is on disk in clear.
- **`RawData` is not always a secret.** The same dictionaries place `96` on `Logon` (`A`),
  `UserRequest` (`BE`), `News` (`B`) and `Email` (`C`). On `A` the FIX 4.4 definition says
  *"Required for some authentication methods"*; on `B` and `C` it is message content.
- **The journal is the resend source**, and the file is what a restart reloads into the ring
  (`FileJournal::open_with`, `journal.rs:507-640`). Whatever is written in place of a secret is
  what a counterparty is sent after a restart.

Tag numbers and types, from the vendored `FIX44.xml`, `FIXT11.xml`, `FIX50SP2.xml`: `554`
Password STRING; `925` NewPassword STRING; `95` RawDataLength LENGTH → `96` RawData DATA; `1401`
EncryptedPasswordLen LENGTH → `1402` EncryptedPassword DATA; `1403` EncryptedNewPasswordLen
LENGTH → `1404` EncryptedNewPassword DATA; `1400` EncryptedPasswordMethod INT.

## Research

Searched 2026-09-23. Each line is someone else's claim, not verified here beyond what is said.

- **Artio** cleans passwords out of messages received as an acceptor and writes `***` in their
  place, "to avoid them being logged into the archive of FIX messages and leave passwords in
  plaintext on disk" — <https://github.com/artiofix/artio/wiki/Frequently-Asked-Questions>. The
  class is `PasswordCleaner` (`artio-core/.../engine/framer/PasswordCleaner.java`, read through
  the GitHub API): it replaces `554` and `925` with a **fixed three-byte** `***`, rewrites
  `9=` to the new body length, and has no code touching `10=`; `96` is not cleaned.
- **OnixS** (.NET) has `EngineSettings.ScrambledLogonFields`: *"Tags for the Logon(A) message
  fields to scramble in the session storage for security reasons"* —
  <https://ref.onixs.biz/net-core-fix-engine-guide/api/OnixS.Fix.EngineSettings.html>. The page
  does not state the default value; the search summary said `554` and `925`, which the page
  itself did not show.
- **QuickFIX/J** issue #330, *"Do not log cleartext password on logon failure"* —
  <https://github.com/quickfix-j/quickfixj/issues/330>; closed by PR #352, which added a
  **setting controlling whether the message is logged when no session is found**, not field
  masking — <https://github.com/quickfix-j/quickfixj/pull/352>. Nothing found that masks `554`
  in QFJ's `FileLog` or `FileStore`.
- **QuickFIX (C++) / QuickFIX/n**: the mailing-list answer is to write your own `Log` and strip
  the field in `onIncoming`/`onOutgoing` —
  <https://quickfix-developers.narkive.com/Zk4a3yRw/username-password-in-logon-msg>. Nothing
  built in found.
- **QuickFIX/Go**: the search found nothing about masking in its log or store.
- **Resend of a message the application does not want replayed**: QuickFIX/J's `toApp` may throw
  `DoNotSend` on a resend, and *"if … the flag is set to true, a sequence reset will be sent in
  place of the message"* — <https://www.quickfixj.org/usermanual/2.3.0/usage/application.html>.
  A gap fill in place of an application message is the family's ordinary answer.
- **FIX 4.4 Logon**: `RawData` *"Required for some authentication methods"*; `Password` *"minimal
  security exists without transport-level encryption"* —
  <https://www.onixs.biz/fix-dictionary/4.4/msgType_A_65.html>.
- **Advisories**: the class is CWE-532, *"Insertion of Sensitive Information into Log File"* —
  <https://cwe.mitre.org/data/definitions/532.html>. **The search found no CVE or advisory
  against a FIX engine** for logging credentials; the hits were other products (e.g. PAN-OS
  CVE-2020-2043).

## Options considered

**For the message log**

- **L1. Mask the value in place, same length, `*`, on the writer thread.** Chosen. `9=` and
  `95=` stay true, so the line still frames; `10=` no longer matches.
- **L2. Replace with a fixed `***` and rewrite `9=`/`95=`** (Artio's shape). Hides the length,
  but the line is no longer the bytes that arrived with one field changed — it is a different
  message, and rewriting `9=` needs a second buffer and a digit count on every masked line.
- **L3. Recompute `10=` after masking.** Makes a line checksum-valid that never existed on any
  wire. A log whose job is *"what did we receive"* should not manufacture a valid frame.
- **L4. Mask on the engine thread, before `record`.** Costs the engine thread a scan per message
  per direction for a file it does not own. The writer already owns a mutable copy.

**For the journal file**

- **J1. Write masked bytes.** Rejected: after a restart the ring is reloaded from the file, and a
  `ResendRequest` then replays `554=********` with `43=Y` — a wrong credential sent to a
  counterparty. Worse than any gap.
- **J2. Refuse the `put`** (return `false`). Gap-fills in-process too, and raises
  `puts_refused`, which ADR-0046 made mean *"the ring could not hold it"*. One counter, two
  causes.
- **J3. Keep the ring verbatim; write an ADR-0053 outbound mark for that `seq` instead of the
  message record.** Chosen. No format change, nothing to mask, nothing that fails a checksum,
  and a restart treats the number exactly as it treats an administrative message: spent, no
  bytes, gap-filled.
- **J4. A new record kind or a flag bit** saying *"kept, not replayable"*. A format version
  (V2) for something J3 already expresses.

**For configuration**

- **C1. Always on, no key.** Chosen.
- **C2. A key, default on.** An off switch for credential masking is a foot-gun in a file an
  operator copies from a forum post. A deployment that truly needs raw bytes on disk can
  implement `MessageLog` itself — the trait is public — and owns that choice in code.
- **C3. A user-extensible tag list.** Deferred, with its reopening condition below.

## Decision

1. **What is a secret.** `fixbolt_engine::redact::MASKED` names it, one place:
   - `554` Password and `925` NewPassword (STRING), in **every** message;
   - `1402` EncryptedPassword (length in `1401`) and `1404` EncryptedNewPassword (length in
     `1403`) (DATA), in every message;
   - `96` RawData (length in `95`) (DATA), **only** when `35=` is `A` or `BE` — or when no
     `35=` can be read, as on a garbage frame.

   Not masked: `553` Username, the three LENGTH fields, `1400`, `91` SecureData, `96` on `News`
   or `Email`. Identity is what a dispute needs; a length is not a secret.
2. **How a secret is found — dictionary-free, and it may over-mask but never under-mask.** The
   scanner splits on SOH. A STRING value cannot contain SOH, so every real `554=`/`925=` field
   begins right after an SOH and is found; the only error the split can make is to read bytes
   *inside* another DATA field as a field, and the only consequence is masking bytes that were
   not a secret. A DATA secret is masked over **the larger of** its declared length (the most
   recent `95=`/`1401=`/`1403=` in the message) and the bytes up to the next SOH, clamped to the
   buffer — so a RawData holding an SOH is masked whole, and a lying length masks more, not
   less. A test pins the three LENGTH→DATA pairs against `fixbolt_dict`'s tables so a dictionary
   change cannot silently move them.
3. **The message log** masks on its writer thread, in the writer's own buffer, before escaping:
   every value byte of a masked field becomes `*` (`0x2A`), length kept. `9=`, `95=`, `1401=`,
   `1403=` and `10=` are left exactly as received — **the line keeps framing and deliberately no
   longer checksums**, which is the mark that it was altered. The engine thread's `record` is
   unchanged.
4. **The journal file** never holds a message that carries a secret. When a message record
   carries one, the file gets **the ADR-0053 outbound mark for its `seq`** in its place — the
   same twelve bytes (plus CRC on a V1 file) `mark_out(seq)` writes. The in-memory ring keeps
   the message verbatim, so a `ResendRequest` inside the same process replays it with `43=Y`, as
   QuickFIX would. After a restart the ring has no bytes for that number and the session
   gap-fills it — the path every administrative message already takes. Under `Durability::Async`
   the decision is made on the writer thread; under `Durability::Fsync`, which already writes on
   the engine thread, it is made there, before the write. `MemJournal` is unchanged: it holds
   nothing on disk.
5. **Allocation-free and panic-free, proven by bench, not by reading.** The scanner is two pure
   functions over a borrowed slice — `mask(&mut [u8]) -> usize` (fields masked) and
   `carries_secret(&[u8]) -> bool` — with no `Vec`, no `String`, no `format!`, no indexing
   (`get` only), no `unwrap`. `crates/engine/benches/alloc.rs` gains cases `redact-mask` and
   `redact-scan` that read 0 and are shown non-zero under an injected allocation.
6. **Always on, no configuration key.** `FileLog` and `FileJournal` mask; `NoLog`,
   `MemJournal` and a user's own `MessageLog`/`Journal` do whatever their author wrote.
7. **The gate** is a test in `crates/engine/tests/` that drives a real `Logon` with `553`/`554`/
   `95`/`96` and an application `UserRequest` with `554`/`925`/`95`/`96` through
   `serve_with_recovery` with a `FileLog` and a `FileJournal`, and fails if any secret's bytes
   appear in either file — with positive premises that the secrets **did** cross the wire and
   that the journal file **does** hold the non-secret application reply. Shown red on the
   unwritten code, and red again under each reversal the plan lists.

## Consequences

**Good**

- ADR-0097 exit criterion 4 becomes a command. No credential the engine knows about reaches
  either file in the default build, in either mode, in either durability.
- Zero added work on the engine thread under `NoLog`, `FileLog` and `Durability::Async`; the scan
  runs on the writer threads that already exist.
- The journal format does not change. `journal::Reader`, `tools/jrnl`, a V0 file and a V1 file
  all read exactly as before; the new record is one they already know.
- In-process resend behaviour does not change.

**Bad, and accepted**

- **The password's length is on disk** in the message log (`554=` followed by N stars). Chosen
  over rewriting `9=` (L2). A reader who needs the length hidden has to rely on the file's
  permissions, as they already must for everything else in it.
- **A log line that carried a secret no longer checksums.** A tool that re-parses the log with
  checksum validation on will refuse those lines. Named in `GUIDE.md` §6c.
- **Session-visible after a restart:** an application message carrying a secret is gap-filled,
  not replayed. A counterparty that relies on a `UserRequest` being resent after our restart
  will not get it. Recorded in `SESSION-BEHAVIOUR.md` §4.
- **The journal file is no longer a complete audit of outbound application messages**: a
  `UserRequest` appears as an outbound mark. The message log (masked) still shows it was sent.
- **Over-masking is possible**: a DATA field (e.g. `213` XmlData, `355` EncodedText) whose bytes
  contain `\x01554=` is partly starred in the log, and a message carrying one is gap-filled after
  a restart. Chosen over a dictionary-aware scanner, which would need the encoding's dictionary
  on a writer thread that has none and could under-mask on a custom DATA tag it does not know.
- **`Durability::Fsync` pays one scan per `put`** on the engine thread. `[unmeasured]` — a pass
  over a ~200-byte message beside an `fsync`; no latency claim is made and none may be quoted
  without `benches/journal.rs` on the §9 machine.
- **Files written before this change still hold secrets.** Nothing rewrites them. `GUIDE.md`
  tells the operator to rotate the credentials and delete or re-permission old logs and
  journals.
- **A venue-specific secret tag (a user-defined tag, `5000+`) is not masked.** Reopening
  condition for C3: the first deployment that names one. Until then it is a line in `GUIDE.md`.
- **Secrets still live in memory** — the receive buffer, the rings, a core dump. Out of scope
  here; named in the plan.

## Sources

Listed inline under *Research*. Code read: `crates/engine/src/msglog.rs:1-106, 139-200,
355-540`; `crates/engine/src/journal.rs:1-120, 255-355, 500-560, 750-830, 905-1060`;
`crates/engine/src/conn.rs:430-460, 825-845`; `crates/session/src/lib.rs:2445-2478, 3060-3095,
3860-3900`; the vendored QuickFIX `FIX44.xml`, `FIXT11.xml`, `FIX50SP2.xml`.
