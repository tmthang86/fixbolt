# ADR-0041 — The library layer buys its API with a template per message, and says so

- **Status**: Accepted — 2026-09-02
- **Date**: 2026-09-02
- **Deciders**: Tran Manh Thang
- **Related**: [ADR-0002](ADR-0002-engine-library-split.md) — the dispatch split this sits on ·
  [ADR-0003](ADR-0003-message-representation.md) — the borrowed view it hands out ·
  [ADR-0016](ADR-0016-per-machine-baselines-replace-absolute-targets.md) — why the figures
  below are per-machine · [DESIGN.md D9](../DESIGN.md) — the template encoder this decision
  is measured against · [plan](../plans/2026-09-02-the-library-layer.md)

## Context

`DESIGN.md` §7 step 8 asks for *"the public API and the first end-to-end example"*. The seam
it would sit on already exists: `fixbolt_session::Application` takes raw bytes and returns a
`Range<usize>`.

That signature is right for the layer it is in and wrong for the person writing a trading
application, who must then, for every message: parse bytes the session has already parsed;
build a `TemplateBuilder` by hand; remember that `49`/`56` **reverse**; remember that `52` is
the session's clock and that regenerating it moves the body by four bytes; and return a range
into a buffer whose message does not start at index 0. `crates/conformance/src/echo.rs` does
all five, and three of them are written up as traps in the comment at the top of that file.

So the library layer's job is to do those five once. The question this ADR answers is **what
that costs**, because the answer decides whether it can be the front door.

## What was measured

`[measured 2026-09-02]` on an **Intel(R) Xeon(R) Processor @ 2.80GHz**, a shared 4-vCPU cloud
VM running Linux 6.18.44. **This machine does not satisfy `DESIGN.md` §9** — no isolated
cores, no `nohz_full`, no frequency pinning — so none of these is a publishable latency
figure and none is recorded in `benches/baselines.tsv`. They are recorded here because the
*ratios* are what the decision turns on, and the run-to-run spread was measured before they
were trusted: **±3–4% over five whole runs**, which is tight enough for a 50× ratio to mean
something.

The committed benchmark is `crates/library/benches/cost.rs`.

**One twelve-field `ExecutionReport`, written three ways:**

| | ns/op | |
|---|---|---|
| Encode a template **built once**, D9's shape | **40** | `crates/library/benches/attrib` experiment, 2026-09-02 |
| Build a template per message, `P=64, S=1024` | **1992 – 2197** | the library's default |
| Build a template per message, `P=128, S=4096` | **3841 – 4008** | the library's *first* default |
| The second parse, alone | **188 – 195** | |
| The whole of `App::on_message` | **2062 – 2131** | |

**The finding, and it is the opposite of what the plan expected.** The plan named the second
parse as the cost and the template build as a secondary worry. The parse is **~190 ns, about
9% of the path**. Building a `Template` per message is **~1.9 µs, the other 91%** — and it is
**~50× the 40 ns** it costs to encode one that was built once. `DESIGN.md` D9 says outbound
messages are pre-encoded templates patched per send; this layer, as written, is the thing D9
says not to do.

`P` and `S` are both on the clock because both are copied on every `.field()` call, which
takes `self` by value:

| `P` | `S` | ns/op |
|---|---|---|
| 128 | 4096 | 3841 – 4008 |
| **64** | **1024** | **1992 – 2197** |
| 32 | 512 | 1447 – 1552 |
| 32 | 256 | 1421 – 1504 |

Below `S = 512` the curve flattens: what remains is the parts array and the sort.

`[measured 2026-09-02]` **one earlier version of this layer cost 2.5× what it needed to**, and
the cause was inside the convenience rather than inside the codec: `Message` held its builder
in an `Option` so that `send` could take it out, and the `take`/put pair added two moves of an
`S`-byte struct to **every** `.field()` call. That is fixed. It is recorded because it is the
shape of mistake this layer invites — a wrapper whose own bookkeeping costs more than the
thing it wraps.

## Decision

**Ship the library layer with the template built per message, publish the number, and keep the
raw seam open.** Concretely:

1. **`Handler`/`Reply`/`App` are a convenience layer, not the `hft` path**, and every document
   that describes them says so with the figure above rather than with an adjective. `README`,
   `GUIDE.md` and the crate's own rustdoc each carry it.
2. **`fixbolt_session::Application` stays public, stays re-exported from `fixbolt`, and stays
   the documented way to write a handler that cares.** An application that pre-builds its own
   `Template` per message type gets D9's 40 ns and gives up nothing but the five conveniences.
   `crates/conformance/src/echo.rs` is a worked example of it.
3. **The default sizes are `P = 64, S = 1024`, chosen off the sweep above** — 1.9× faster than
   the 128/4096 this layer started with, with room for a realistic `ExecutionReport` carrying
   a small repeating group. They are the caller's: `Handler<256, 32, 512>` takes the rest.
4. **The second parse is accepted and is not the thing to fix.** Removing it would mean
   changing `Application::on_message` to hand down the session's own `MessageView` — a
   session-layer API change, a re-run of the 59 definitions, and a saving of ~9%.
5. **The template build is the thing to fix, and it needs `codec` to gain something it does
   not have**: a way to sort and encode straight out of a builder without materialising a
   `Template<P, S>` value, or a builder that can be cleared and reused across messages. That
   is its own plan and its own ADR. **`STATUS.md` carries it as an open item, with this ADR's
   numbers as its baseline.**
6. **No `library` case is recorded in `benches/baselines.tsv`.** This CPU has no rows and the
   machine fails §9. `cost.rs` prints `NO BASELINE` and `scripts/bench.sh` counts it, which is
   the honest state rather than a number nobody can reproduce.

## Consequences

**Good**

- **The five traps are paid once, in one place, and cannot be got wrong by a handler.** `49`/`56`
  cannot be reversed the wrong way because neither is reachable from the API a handler is
  given; `52` cannot be regenerated for the same reason; field order is the dictionary's
  because every reply goes through `TemplateBuilder::build::<Fix44>()`.
- **A number instead of an adjective.** *"The convenience layer costs about two microseconds
  per reply on an untuned box, against forty nanoseconds for a template built once"* is a
  sentence somebody can act on. It was not knowable before this was built and measured.
- **The fast path was not removed, or even moved.** Nothing about `Application` changed.
- **`hft` is not quietly compromised.** An `hft` deployment that used this layer would spend
  ~2 µs in the application for a message the rest of the engine handles in hundreds of
  nanoseconds. Saying that in the `GUIDE` is what keeps ADR-0013's mode split honest.

**Bad — and these are real**

- **The front door is 50× slower than the house.** For an engine positioned as *the fastest
  acceptor on kernel TCP*, the API most readers will try first is the one that does not
  demonstrate the claim. That is the whole cost of this decision and it is not small.
- **Two ways to write a handler**, which is two things to document, two to keep correct, and a
  choice a newcomer has to make before understanding either. ADR-0002 already accepted a
  version of this for dispatch; this doubles it.
- **The `P`/`S` defaults are a cliff, not a slope.** A reply with more fields than `P` or more
  bytes than `S` is `Answer::Failed`, not a slower success — correct, and still a surprise
  that only shows up under a message shape nobody tested. The counter
  (`App::failed_replies`) exists so it is at least visible.
- **The measurement is from a machine that fails §9**, so the ratios are trustworthy and the
  absolute figures are not. A §9 run could move them in either direction and nothing here
  predicts by how much.
- **This ADR names work it does not do.** Point 5 is a promise, and `STATUS.md` is where a
  promise goes to be checked.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| **Ship no library layer; `Application` is the API** | It is `DESIGN.md` §7 step 8, and the five traps stay in every user's lap. `echo.rs`'s own comment is the evidence that they cost somebody a day each |
| **Hold the library until `codec` can encode from a reused builder** | The codec change is unplanned, unmeasured and unbounded, and holding a working, tested layer behind it trades something real for something hoped for. The layer is correct today; it is only fast tomorrow |
| **Change `Application::on_message` to pass the session's `MessageView`** | Buys ~9%. Costs a session-layer public API change, a re-run of the 59 definitions, and a const parameter on the trait every application would then carry. Wrong lever, and the measurement is what says so |
| **Cache one `Template` per `(MsgType, field set)` inside `App`** | The right long-term shape and D9's own idea, but the cache key is *the set of tags a handler names*, which the handler chooses per message. Getting that wrong emits a message with the wrong fields — a correctness bug bought with a performance win. Needs its own plan, its own reversal, and a corpus run |
| **Keep `P = 128, S = 4096`** | Measured at 1.9× the cost of 64/1024 for headroom no message shape in the acceptance corpus needs |

## Open questions

1. **What are these figures on a `DESIGN.md` §9 machine?** Every number here is from a shared
   cloud VM. The ratio 50:1 is unlikely to move much; the absolutes will.
2. **Why do the three cases not add up?** `[measured 2026-09-02]` parse 190 + reply 2140 =
   2330, and `on_message` reads 2062–2131 — about 200 ns *less* than the sum, against a 3%
   spread. The parse costs ~190 ns measured alone and about nothing measured inside the whole.
   Recorded, not explained, and `cost.rs` says so in its own module comment rather than
   carrying a claim the numbers do not support.
3. **Does the `P`/`S` cliff want a compile-time answer?** A handler that names its fields at
   compile time could have `P` inferred. Nothing in this workspace needs it yet.
