# A known-limitations list rots in one direction, and every individual document stays correct

> `[measured 2026-09-01]` — found by reading this repository's own `STATUS.md` against the
> code, during a design review after four days of measurement on a real Linux desktop.
> **`[to testing-skills]`**
>
> This page is about a **documentation** failure with a testing shape. It is here rather than
> in an ADR because the mechanism generalises past this project and past FIX.

## The number

`STATUS.md` carries a section titled **"Not proven — claimed, researched, or simply not yet
run"**. It is the honesty ledger: everything this project asserts it has *not* established.
On 2026-09-01 it held 22 bullets. **Eight of them were false.**

| The bullet | What was actually true, and since when |
|---|---|
| *"The **150 ns gates** in `DESIGN.md` §6 are anchored to one macOS laptop"* | There are no absolute gates. ADR-0016 withdrew every one of them — **2026-08-31** |
| *"**Every figure in `DESIGN.md` §8** is from the literature, not measured"* | Five rows measured on the tuned desktop — **2026-08-31** |
| *"The ring-buffer hop (200–500 ns) … [is a] literature figure"* | Measured at 267.4 ns one way — **2026-09-01** |
| *"**DATA fields inside a repeating group are untested**"* | Closed with its plan — **2026-08-30** |
| *"**32 of the 59 definitions still fail**"* | **59 / 59** — **2026-08-29, three days earlier** |
| *"**`Input::Tick` is sent but never advances**"* | Advances; the file it named passes — **2026-08-29** |
| *"**Sequence numbers reset on every connect**"* | `Session::resume` — **2026-08-31** |
| *"**only `Acceptor` is exercised**"* | Two initiator steps merged — **2026-08-30** |

The worst of them, *"32 of the 59 definitions still fail"*, sat on the page for three days
underneath a heading whose entire purpose is to be believed.

## The mechanism, which is not laziness

Every one of those closures was done properly. Each plan that closed:

- struck **its own** open item, with the measurement and the date;
- updated the design document the change touched;
- walked the project's documentation sync table, row by row, by hand.

**No row of that sync table points at the list of things not yet proven.** The table is
organised by *what you changed* — change the codec, update the codec's page; change a gate,
update the gates section. The known-limitations list is not the subject of any change; it is
the shadow of all of them. So it is the one page that no change is ever *about*, and it decays
by exactly one bullet per closure while every page anybody actually edits stays correct.

**The decay is directional, and that is what makes it dangerous.** A limitations list can only
rot toward *understating* the system. Nothing ever adds a false pessimism; work landing is what
turns each entry into a lie. So the page degrades into a systematically out-of-date, uniformly
too-modest account — and it degrades *fastest* exactly when the project is going *best*.

## Why nothing caught it

- **The tests were all green**, and they had nothing to say: no test asserts a sentence about
  what is untested.
- **The link checker was green** — every link in the false bullets resolved. The documents they
  pointed at were the ones that had been correctly updated.
- **The reviewer of each change was looking at that change.** A diff that closes item 16 does
  not put the item-16-shaped bullet three hundred lines away on screen.
- **Nobody re-reads a page that says nothing is finished.** It is the section a reader skims to
  feel reassured about the project's honesty, which is the opposite of reading it.

## The ninth bullet, found by walking into the trap this page describes

`[measured 2026-09-01]` **Eight bullets were struck. There were nine.**

Hours after this page was written, a feature review of the same repository produced a new
finding: *"the application-message resend path is implemented and has never run."* It was filed
as an open item, put in a product document, and written into a pull request. Its whole basis was
**another bullet of the same rotten list** — *"No application message has ever been replayed. The
inbound `ResendRequest` path answers with a gap fill because everything this session has sent so
far is administrative."*

It is false, and one command settles it:

```
$ cargo test -p fixbolt-engine --test journal
running 7 tests
test a_replay_says_when_it_is_being_sent_and_when_it_first_was ... ok
...
test result: ok. 7 passed; 0 failed
```

That test feeds a real application message, takes the reply, sends a resend request, and asserts
**a replay and not a gap fill** — the exact behaviour the bullet said had never happened. Three
sibling tests do the same under the other durability policies, and a fourth is the arm that *does*
gap-fill, so the two outcomes are distinguished rather than assumed.

**What makes this worth a section rather than an edit** is what was and was not checked. The
implementation *was* verified — the function exists, it reads the store, it re-emits the message,
and the file and line were quoted. **Whether any test reached it was not.** The claim *"has never
run"* was carried over whole from the stale list, wearing the credibility of the checking done
beside it.

And the person doing it had, that same day, in that same session, written this page.

> **A written-down failure mode is not a checked one.** Documenting a trap does not install a
> guard against it; it produces a document. The author of a warning about stale claims is not
> thereby immune to stale claims — and being *mid-way through* writing that warning is not
> immunity either, it is just a stronger feeling of immunity.
>
> **What actually settled it was running something**, which is the same lesson at a different
> altitude: *a check proves nothing until something reads it*, and *a claim proves nothing until
> something runs it*. The cost of the run was one command and six seconds. The cost of not running
> it was a false item in a tracker, a false row in a product document, and a false paragraph in a
> pull request — each of which would have been quoted by the next person as established fact.
>
> **The practical rule, and it is narrow enough to follow:** when a finding's basis is *"X has
> never happened"*, that is not a code-reading claim and cannot be settled by reading code. Find
> the thing that would happen if it had, and run it. Verifying the *implementation* of X while
> inheriting the *history* of X is the specific move that fails here, and it feels like diligence
> because half of it is.

## Generalised

> **A project's list of known limitations decays in exactly one direction, and a green board is
> not evidence against it.** Every item on such a list is a claim that something has *not* been
> established — so each one is falsified silently by the work that establishes it, and never by
> a test, a linter, or a link checker, none of which can read a sentence about absence. The
> people best placed to notice are the ones who just did the work, and they are looking at
> their own diff, where the limitation is not.
>
> The list therefore ends up **uniformly too pessimistic**, and it rots fastest when the project
> is moving fastest — which is when it is most likely to be quoted to somebody new.
>
> **Two fixes, and the cheap one is the process one.** Either the definition of done for a unit
> of work includes *re-read the limitations list line by line* — not *strike your own entry* —
> or the list stops being maintained by hand and is generated from something a machine can
> falsify: a test that is `#[ignore]`d, a gate marked unimplemented, a to-do with an owner.
> **When a false entry is found, strike it in place with its closing date rather than deleting
> it.** A deleted lie leaves a tidy page and no evidence that the page cannot be trusted; a
> struck one shows the reader exactly how far behind it ran, which is the only thing that makes
> the next reader check.

## What was done here

The eight bullets were struck in place with their closing dates, the section gained a note
saying it had rotted and why, and it became `STATUS.md` **open item 27**. The process fix —
adding a row to the documentation sync table — touches the file that governs how this project
works, so it is proposed rather than applied.

**Nothing here is proven by a gate**, and that is the honest state of it: the finding was made
by one person reading one page against the code, and the same page will rot again unless
something changes that a reader cannot forget.
