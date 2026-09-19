# Internals — a map, not a second copy

A new contributor already has [DESIGN.md](../DESIGN.md) (**how** the system is built) and
the ADRs in [decisions/](../decisions/) (**why**, at what cost). Neither says which file in
which crate holds a given piece of behaviour, or in what order to read a crate's source.
This directory is that map: one page per crate, each answering four questions — which files
it holds, what each one keeps, in what order to read them, and which test guards it.

**This is not a third description of the design.** A page here points at a `DESIGN.md`
section or an ADR number rather than restating it — CLAUDE.md §4, "one rule, one place": a
rule (or a design decision) kept in two places is two that will disagree. If a page and
`DESIGN.md` ever say something different about the same fact, that is a bug in this
directory, not in `DESIGN.md`.

No measurements, no dates, no status live here — those belong to `STATUS.md`,
[reference/](../reference/) and the ADRs, and a page that quotes one is wrong on arrival.

## Pages

| Page | Answers |
|---|---|
| [codec.md](codec.md) | Where parsing and serialising in place happens, and how the hot path stays allocation-free |
| [dict.md](dict.md) | Where the FIX 4.4 tables come from, and what is generated versus hand-written |
| [session.md](session.md) | Where the pure session state machine lives, and how it takes time without a clock |
| [engine.md](engine.md) | Where the TCP acceptor/connector and its many modules sit, and what each owns |
| [library.md](library.md) | Where the application-facing `fixbolt` API adapts the session layer |
| [conformance.md](conformance.md) | Where the 59 acceptance definitions are turned into a runnable gate |
| [tools.md](tools.md) | Where the `tools/` binaries (`w2w`, `interop`, `jrnl`, `attr-scan`) live and what each measures or checks |

## Reading order across crates

Follow the dependency order in [DESIGN.md §3](../DESIGN.md#3-crates): `codec` → `dict` →
`session` → `engine` → `library`, with `conformance` built alongside `session` as its gate
and `tools/` last, since every tool depends on crates above it.

## Why crate pages instead of one file

`STATUS.md` item 33's *engine-has-no-map* half named the problem directly: `engine` alone
has more than 20 modules and nothing said what each one was for. One page per crate keeps
each page short enough to read in one sitting (this directory's own gate holds every page to
80 lines) and lets a change to one crate's internals touch one page, not a shared one.
