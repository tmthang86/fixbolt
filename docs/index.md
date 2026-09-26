# fixbolt

fixbolt is a FIX 4.4 protocol engine written in Rust, as a library you embed in your own
application. It plays both FIX roles over one session state machine: the **acceptor**, which
listens for counterparties to connect, and the **initiator**, which dials out to them
([ADR-0004](decisions/ADR-0004-bidirectional-engine.md)). The acceptor is the role it was built
around.

It is positioned as **a FIX acceptor on ordinary kernel TCP whose latency is a published,
reproduced number**. A kernel-bypass figure, where one exists, appears only as a second, labelled
row beside a kernel-TCP figure from the same boot
([ADR-0099](decisions/ADR-0099-kernel-tcp-stays-the-default-and-the-headline-and-a-bypass-figure-is-a-second-labelled-row.md)).
It is not a port of QuickFIX; it takes QuickFIX's dictionaries and acceptance tests as data
([ADR-0001](decisions/ADR-0001-relationship-to-quickfix.md)).

This site is for a Rust developer outside the project who is building an acceptor or an initiator
on fixbolt. Contributors to fixbolt itself start at [Contributing](contributing.md).

## What you get

- **A handler, not a framework to subclass.** You implement `Handler`; the engine calls it with a
  message already parsed and a `Reply` to answer through. `Reply` writes the fields your
  application does not own (`8`, `9`, `10`, `34`, `49`, `52`, `56`) and orders every field from
  generated tables, never from your call site. See [API](api.md) and
  [Getting started](GETTING-STARTED.md).
- **Your code runs on the engine's thread.** A handler that blocks stops the session with it,
  and the counterparty sees missed heartbeats. [GUIDE.md §2](GUIDE.md) says what to do instead;
  the whole of [GUIDE.md](GUIDE.md) is the list of constraints the compiler cannot check for you.
- **Messages are borrowed, not copied.** A message is a view into the engine's read buffer
  ([GUIDE.md §3](GUIDE.md), [ADR-0003](decisions/ADR-0003-message-representation.md)).
- **Two modes, and the default is the portable one**
  ([ADR-0013](decisions/ADR-0013-two-modes-standard-and-hft.md)). `standard` blocks when idle and
  gives the core back; it is what `serve` runs and what you get if you say nothing. `hft` is
  opt-in and Linux-only: `serve_hft`'s engine thread polls and never sleeps in the kernel on the
  hot path, so it burns the core it runs on, which should be an isolated core you name and pin
  ([best-practices-hft.md §3](best-practices-hft.md)). Choosing between the two is
  [GUIDE.md §0](GUIDE.md).
- **A session layer with no I/O.** The FIX session protocol is a pure state machine: no socket,
  no clock, no heap allocation. That is what lets QuickFIX's 59 FIX 4.4 acceptance definitions run
  against it as tests; the results, with the commands that produce them, are in
  [CONFORMANCE.md §1](CONFORMANCE.md).
- **No heap allocation on the hot path**, asserted by a counting allocator on every hot path
  rather than by reading the code ([CONFORMANCE.md §4](CONFORMANCE.md)).
- **What the session does before your code sees a message** — logon, resends, gap fills, session
  rejects — is written down case by case in [SESSION-BEHAVIOUR.md](SESSION-BEHAVIOUR.md).

## Before you choose it

These are stated plainly because anyone comparing FIX engines will find them. The full list, and
where the project stands against QuickFIX capability by capability, is [PRD.md §3](PRD.md).

- **It is released as git tags, not on crates.io**, and there is no docs.rs page. This is a
  decision, not an omission
  ([ADR-0161](decisions/ADR-0161-0-1-0-is-a-git-tag-not-a-crates-io-upload-and-the-stranger-and-the-semver-gate-read-the-tag.md)).
  [Getting started](GETTING-STARTED.md) has the install line; [API](api.md) says how to read the
  API documentation.
- **It is pre-1.0.** The conditions for calling it 1.0 are written in
  [CHANGELOG.md](../CHANGELOG.md), under *Conditions to reach `1.0`*.
- **Its production track record is zero.** No amount of testing substitutes for counterparties
  having found an engine's bugs ([PRD.md §3](PRD.md)).
- **It is a single-owner project.** Issues are read, but there is no SLA and no on-call
  ([README.md](../README.md), *Support level*).
- **Logon admits by identity only.** A configured comp-ID pair is admitted; there is no credential
  check behind the hook today ([PRD.md §3](PRD.md)).
- **The dictionary is compiled in.** The FIX 4.4 tables are generated at build time; changing the
  dictionary means rebuilding, and today it means replacing the whole FIX 4.4 file
  ([CONFIGURATION.md §5](CONFIGURATION.md)). Adding venue-specific fields on top of FIX 4.4 is
  the work of phase 5 ([PRD.md §2](PRD.md)).

## Where to start

| You are… | Start with | Then |
|---|---|---|
| New to FIX | [INTRODUCTION.md](INTRODUCTION.md): FIX 4.4, its vocabulary, and why the acceptor role is hard | [Getting started](GETTING-STARTED.md) |
| Wanting an acceptor running now | [Getting started](GETTING-STARTED.md): a configuration file and one Rust file | [Tutorial](TUTORIAL.md), which builds the same acceptor step by step |
| Embedding fixbolt in a real application | [GUIDE.md](GUIDE.md): every constraint that shows up as latency or lost messages rather than as a compile error | [Configuration](CONFIGURATION.md), [Session behaviour](SESSION-BEHAVIOUR.md) |
| Running it in production | [standard mode](best-practices-standard.md) or [hft mode](best-practices-hft.md), whichever you chose | [Tuning a Linux host](hft-playbook.md), for `hft` |
| Looking something up | [Configuration](CONFIGURATION.md) for settings, limits and features; [API](api.md) for types | [Session behaviour](SESSION-BEHAVIOUR.md) for what comes back on the wire |
| Deciding whether to use it at all | [PRD.md §3](PRD.md): the gaps, named | [Conformance evidence](CONFORMANCE.md): what is proven, and how |
| Wanting to know why it is built this way | [DESIGN.md §4](DESIGN.md): the decisions | [decisions/](decisions/): the record behind each one |

## How to read a number here

Every performance figure in these documents names the committed benchmark that produced it, the
machine it ran on, and the operating-system settings in force
([DESIGN.md §9](DESIGN.md)); every latency figure also names its mode and its session count. A
figure missing any of these is somebody else's claim and is labelled that way. This page quotes
none: the latency budget is [DESIGN.md §8](DESIGN.md), and every measured figure with its
conditions is in [reference/measured-costs.md](reference/measured-costs.md). Your own figures
will differ; [GUIDE.md §8](GUIDE.md) says how to measure them without fooling yourself.

## What is in this book, and what is not

The pages here are the Markdown files under `docs/` in the repository, rendered as they are; the
repository copy is the authored one. Three things stay in the repository and are not part of the
book: [STATUS.md](../STATUS.md), where the work stands day by day; the engineering rules in
[CLAUDE.md](../CLAUDE.md); and the plans under [plans/](plans/), which are internal working
documents written in Vietnamese.

## Licence

The code is dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at your
option. `fixbolt-dict` also ships QuickFIX's FIX 4.4, FIXT 1.1 and FIX 5.0 SP2 XML dictionaries,
under the QuickFIX Software License, so a binary built with fixbolt carries QuickFIX-derived
tables and owes that licence's attribution conditions. [NOTICE](../NOTICE) holds the text, and
`fixbolt::NOTICE` is the same text as a constant for an application's third-party notices
([ADR-0104](decisions/ADR-0104-the-published-dictionary-is-quickfixs-xml-shipped-with-a-notice.md)).
