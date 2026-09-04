# ADR-0047: The four buffer sizes are the caller's, through a second function

- **Status:** Accepted
- **Date:** 2026-09-05
- **Plan:** [buffers-the-caller-can-size](../plans/2026-09-05-buffers-the-caller-can-size.md)
- **Supersedes nothing.** Amends how `CLAUDE.md` §6's *"the caller picks `N`"* is delivered.

## Context

`CLAUDE.md` §6 says *"the caller picks `N`. Aliases for the common sizes; **no hidden
constant**."* `docs/CONFIGURATION.md` told readers to *"instantiate `Engine<..., N, RX, TX>`
directly"*.

Neither was true through the front door. All six serving entry points went through type aliases
that pinned `256, 4096, 8192`, and the `fixbolt` crate does not re-export `Engine`. A deployment
whose counterparty sends messages larger than 4 KiB had one move: depend on `fixbolt-engine`
directly and rewrite the serving loop, pre-session stage included. **An alias is exactly as much
of a hidden constant as a `const` when nothing above it can say otherwise.**

Building the way out found a **fourth** size, unnamed and tighter than the other three:
`Outbound::app`, a hard-coded `[u8; 1024]` in which an `Application` lays out one reply. It
failed as silence — `on_message` returning `None` is a legal answer — so an acceptor could
receive 4 KiB and be unable to answer with 1 KiB, with no counter moving and no error anywhere.
Found by a size sweep, not by reading:
[a-ceiling-has-more-than-one-floor](../reference/a-ceiling-has-more-than-one-floor.md).

## Decision

**1. Four const parameters, not three.** `N` (field index), `RX` (receive buffer, and the
pre-session buffer with it), `TX` (write queue), `APP` (an application's reply scratch).
`fixbolt_session::DEFAULT_APP_SCRATCH = 1024` names the last one's default.

**2. A second function per entry point, not defaults on the first.** Six `*_with` twins:
`serve_with`, `serve_hft_with`, `serve_with_recovery_with`, `serve_hft_with_recovery_with`,
`connect_and_serve_with`, `shard::serve_sharded_hft_with`. The originals keep their exact
signatures and delegate with `256, 4096, 8192, 1024`.

**This shape was forced by the language, not chosen.** `[measured 2026-09-05]`, four probes:

| Tried | Result |
|---|---|
| `fn serve<..., const RX: usize = 4096>` — a default on the existing function | *"defaults for generic parameters are not allowed here"* |
| `trait Sizes { const RX; }` + `[u8; S::RX]` — one parameter carrying all four, for a shorter turbofish | *"generic parameters may not be used in const operations"* — needs `generic_const_exprs`, unstable |
| `serve_with::<256, 16384>(...)` — supply only the consts | *"function takes 6 generic arguments but 2 were supplied"* |
| `serve_with::<256, 16384, 8192, 1024, _, _>(...)` | **works** |

So the call site carries two `_`, and that is the language's cost, not this API's.

**3. `APP` goes last in every type's parameter list, after `L`.** Not for looks: inserting it
before `L` silently changed the meaning of every positional `Engine<..., N, RX, TX, L>` already
written, and four test targets stopped compiling. **A new parameter added in the middle is a
breaking change even when it has a default.**

**4. `RX` also sizes the pre-session buffer, structurally.** Both copies of
`const PRE: usize = 4096` are gone. They sat under comments promising they matched the engine —
one invariant, two copies, nothing checking either.

**5. No default changes.** Raising them is wave C's, and needs the §9 desktop.

## Consequences

**Good.**

- The claim `CLAUDE.md` §6 makes is now true for the audience `CONFIGURATION.md` addresses.
- `APP` has a name for the first time, and its failure mode is documented where an operator will
  look — `GUIDE.md` §6 gains a row whose symptom column reads *"nothing at all"*.
- `PRE == RX` cannot drift, because there is no second thing to drift from. The `PrefixTooLong`
  guard it protected is now unreachable through the entry points, which is the point.
- Every existing call site and every file under `crates/*/tests/` is untouched: 492 tests green
  before the new ones were added.

**Bad, and worth stating plainly.**

- **Twelve entry points where there were six.** Every one needs its rustdoc kept in step with its
  twin, and a reader must be told which to reach for. `docs/CONFIGURATION.md` prints the call
  rather than describing it, because the turbofish is not guessable.
- **The turbofish carries two `_` forever**, until const parameter defaults on functions exist.
- **`APP ≤ TX` is a constraint nothing checks.** An `APP` above `TX` wastes the difference
  silently. It is prose in `CONFIGURATION.md`, which `CLAUDE.md` §4 says does not hold a
  constraint — this one is on the list and has no test.
- **Two unit tests in `crates/session/src/out.rs` had to gain a turbofish.** A const parameter is
  not inferred in expression position, even with a default on the struct. They changed no
  assertion, but the plan's *"no test touched"* gate was not met exactly and says so.
- **The defaults are still unmeasured**, and now they are unmeasured in four dimensions rather
  than three. `STATUS.md`'s *Not proven* carries it.
