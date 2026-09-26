# Why fixbolt is built this way

These pages explain the reasoning behind fixbolt for a developer deciding whether to embed it. They
describe; they do not instruct. How to build on the engine is in the tutorials and how-to guides,
and the exact behaviour is in the reference.

- **[Why fixbolt](why-fixbolt.md)**: why the engine is fast on the round trip that matters, one
  mechanism at a time, what each avoids paying, and what fixbolt does not do.
- **[Why Rust, and what it costs](why-rust.md)**: what the language gives this engine, shown in
  this repository's own code and gates, and its real drawbacks, with sources.
- **[The design in ten decisions](design-rationale.md)**: decisions D1 to D10 of the design, each
  with its problem, its cost and where the full reasoning is.

For the depth behind them: [FIX 4.4 and the acceptor role](../INTRODUCTION.md) for the protocol,
[DESIGN.md](../DESIGN.md) for the full design and its latency budget, and [PRD.md](../PRD.md) for
scope, phases and gaps.

No page here states a fixbolt figure without the benchmark, machine and settings that produced it;
most link to where the figure is recorded instead of repeating it.
