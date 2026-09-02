# ADR-0040 — A configuration file refuses what it does not understand

- **Status:** Accepted
- **Date:** 2026-09-02
- **Related:** [ADR-0001](ADR-0001-relationship-to-quickfix.md),
  [ADR-0026](ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md),
  [ADR-0030](ADR-0030-one-engine-holds-many-counterparties.md),
  [ADR-0033](ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md)
- **Plan:** [2026-09-02-a-registry-from-a-file.md](../plans/2026-09-02-a-registry-from-a-file.md)
- **Closes:** `docs/PRD.md`'s last open line under `many counterparties`

## Context

`presession::Table` has served many counterparties since 2026-09-01, and the only way to put one
in was `Table::serving(cfg)` — Rust, and therefore a recompilation.

Adding a counterparty to a running acceptor is an operator's job, usually the evening before that
counterparty reaches UAT. Behind a rebuild it needs a toolchain and the source; it makes changing
a `HeartBtInt` the same class of release as changing the hot path; and it leaves no way to compare
two environments' configuration except by reading two programs. Every FIX engine that has ever
been deployed has a configuration file, and not for convenience.

## Decision

### 1. The file's shape is QuickFIX's, and the parser is this repository's

`[DEFAULT]` and repeated `[SESSION]` blocks, `Key=Value` inside them, `[DEFAULT]` supplying and
`[SESSION]` overriding. Every FIX operator alive has already read a file shaped like this, which
is the whole argument for it.

Nothing is copied. ADR-0001 already treats QuickFIX's XML and `.def` files as data and an oracle;
a file layout is the same kind of thing. `NOTICE` does not become due.

### 2. No dependency

The format is *name = value*. `crates/engine/src/settings.rs` is about 300 lines against `serde`
plus `toml` plus the tree underneath them, and `codec` has stayed at zero dependencies by making
this trade every time it came up.

### 3. **An unrecognised key is an error**

This is where the file parts from QuickFIX, which ignores settings it does not know.

The cost of ignoring is specific and it is not hypothetical. A mistyped `Starttime` falls back to
the default schedule, and the default is `Schedule::always` — a session that should close at five
stays open all night, and nothing anywhere says so. The same argument applies to `TargetCompId`,
which would leave a counterparty unconfigured, and ADR-0026 decision 6 already refused exactly
that shape: **an acceptor that admits an identity nobody configured is an open port.**

The general form: *a key that is spelled almost correctly and has no effect is worse than a key
that is rejected*, because the file reads as though it says something it does not.

### 4. **A file naming no counterparty is an error**

An empty `Table` refuses every connection — ADR-0026 decision 6, and correct. But a *file* that
produces one is a mistake, and its symptom is a port that accepts TCP and answers nothing, which
is indistinguishable from a firewall. `ServeError::NoCounterparties` already refuses an empty
table at startup for this reason; this refuses it one step earlier, where the line number is.

### 5. A half-written schedule is refused, not completed

`StartTime` with no `EndTime` means the writer meant something the parser cannot know. Filling in
midnight would be a guess that reads as a decision. A `StartDay` with no hours is refused for the
sharper version of the same reason: the key is spelled correctly and would have no effect.

### 6. Every error carries its line, and quotes what was written

The person editing this file does not read Rust. *"line 14: unknown key: Starttime"* is usable;
a type name is not.

### 7. The two name limits become public constants of `fixbolt-session`

`MAX_BEGIN_STRING_LEN` and `MAX_COMP_ID_LEN`. `Config` stores names in a fixed buffer and records
an over-long one as *not fitting*, which then matches nothing — so a parser that truncated would
build an acceptor that starts cleanly and serves nobody. The parser must refuse, and to refuse it
must know the limit. **A second copy of `32` in another crate would be a second rule**, and the
one that disagreed would be the one deciding whether a counterparty is served.

## Consequences

**Good**

- Adding a counterparty is an edit and a restart, not a release.
- `[measured 2026-09-02]` the refusals discriminate: ignoring an unknown key turns exactly one
  test red; accepting a file with no `[SESSION]` turns two; dropping the parsed schedule turns
  five, and the control — a file with no hours at all — stays green.
- The whole path is proven through a real listener and real sockets, because
  `tests/registry.rs` and ADR-0034 both record what happens when a layer is finished and the seam
  above it is never asked about.

**Bad, and named**

- **This is a second place where a counterparty is described.** `Table::serving` still exists and
  a deployment may use both. Nothing reconciles them, and nothing needs to yet — but two ways to
  say the same thing is the shape that eventually disagrees.
- **No reload.** The table is read-only after startup, and changing that is a decision about
  synchronisation on the connection path rather than about a file format.
- **No credentials.** ADR-0026 decision 3 makes `Registry::lookup` returning `None` the
  authentication hook and says there will be no second one; a password field here would be that
  second hook.
- **No per-counterparty journal path.** It belongs to `Recovery` ([ADR-0039](ADR-0039-a-fresh-journal-is-the-deployments-to-build.md)),
  not to `Registry`: `Entry` carries a `Config` and nothing else.
- **No `50=` / `57=`.** `Config` has no room for them; ADR-0026 already says a deployment told
  apart by a sub-ID writes its own `Registry`.
- **Weekday names are case-sensitive and English.** `monday` is refused. Accepting it means
  accepting `MONDAY` next, and each is a file that reads as correct to a person and differently to
  the parser.
- **The file cannot express `Schedule::with_utc_offset_ms`.** ADR-0033 is emphatic that a fixed
  offset is not daylight saving; putting one in a file makes it look like a setting rather than
  like the standing hazard it is. A deployment that needs it builds the `Config` in code.

## Alternatives rejected

| Alternative | Why not |
|---|---|
| TOML via `serde` | Two dependencies and their tree, for a format that is `Key=Value`. `codec` has stayed at zero by making this trade every time |
| Ignore unknown keys, as QuickFIX does | Decision 3. A mistyped schedule key silently opens a session all night |
| Accept an empty file and let `ServeError::NoCounterparties` catch it | It would, and without the line number — and `Settings` is usable outside `serve` |
| Complete a half-written schedule with midnight | A guess that reads as a decision |
| Duplicate the name limits in the parser | Two rules, and the one that disagreed would decide whether a stranger is served |
| A `[SESSION]` key naming a journal file | It belongs to `Recovery`, not to `Registry` — ADR-0039 |
