# Security policy

## Reporting a vulnerability

Report it **privately, through GitHub's private vulnerability reporting**, and nowhere else:

- open the repository's **Security** tab and choose **Report a vulnerability**, or go straight to
  <https://github.com/tmthang86/fixbolt/security/advisories/new>.

The report is visible only to you and the maintainers until an advisory is published. **Do not**
open a public issue, a pull request or a discussion about it, and there is no email address for
security reports — private vulnerability reporting is the only channel.

A useful report names the version (tag or commit), the crate and feature flags involved, the
mode (`standard` or `hft`), and the bytes or the configuration that trigger the problem —
ideally as a failing test or a minimal program.

This is a small project with no on-call and no service-level commitment
([README.md](README.md), *Support level*), so there is no promised response time. There is no bug
bounty.

## Supported versions

fixbolt is released as git tags, not on crates.io
([ADR-0161](docs/decisions/ADR-0161-0-1-0-is-a-git-tag-not-a-crates-io-upload-and-the-stranger-and-the-semver-gate-read-the-tag.md)).
Only the newest release tag is supported; a fix lands on `main` and is released as a new tag.

| Version | Supported |
|---|---|
| `main` | Yes — fixes land here first |
| Newest release tag — today `v0.1.0` ([CHANGELOG.md](CHANGELOG.md)) | Yes |
| Anything older | No |

## Scope

**In scope:** the crates in this repository's workspace under `crates/`, as built from a
supported version — above all, whatever a counterparty's bytes reach before anything has
authenticated them. For example:

- input that makes a library crate panic, which the project's rules forbid
  ([CLAUDE.md](CLAUDE.md) §2 item 7), or that corrupts memory through an `unsafe` block;
- input that makes the engine thread allocate without bound, hang, or stop serving other
  sessions — framing, parsing (`parse_into`, `Framer`, `SbeView`), the session's validation, and
  the pre-session stage that owns a socket until its Logon (`PendingSet`, `Limits`);
- a Logon, or any message, accepted when the configured `Table` or the session's checks should
  have refused it;
- a secret that reaches the message log or the journal file in clear although `redact` is meant
  to mask it
  ([ADR-0110](docs/decisions/ADR-0110-a-secret-is-masked-in-the-message-log-and-leaves-only-its-number-in-the-journal-file.md));
- the TLS transport (feature `tls`) accepting a connection it should refuse, or leaking
  plaintext;
- the `fixbolt-metrics` HTTP endpoint exposing more than its series, or being made to allocate or
  block an engine.

**Out of scope:**

- `tools/`, `spikes/` and `fuzz/` themselves — test harnesses and experiments that no application
  depends on. A crash that a fuzz target *finds* in a crate is in scope.
- Third-party code: QuickFIX, QuickFIX/J, the SBE reference implementation and anything else
  fetched into `vendor/`, and the crates this workspace depends on — report those to their own
  projects. If fixbolt's *use* of a dependency makes it exploitable, that part is in scope.
- The host a deployment runs on — the OS, kernel and network settings in
  [DESIGN.md §9](docs/DESIGN.md#9-deployment--the-os-is-part-of-the-design) are the operator's.
- Latency or throughput below a published figure, unless it is caused by input a counterparty
  controls.
