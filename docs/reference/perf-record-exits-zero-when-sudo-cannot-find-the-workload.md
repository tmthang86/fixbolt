# `perf record` exits 0 and writes a file when `sudo` cannot find the workload

> `[measured 2026-09-21]` — boot D, step D2, caught on the first read of the log, after 5 of 21
> runs had already "succeeded". **`[to testing-skills]`**

## The shape

`sudo` does not use the caller's `PATH`. It resolves the command against `secure_path` from
`/etc/sudoers`, which on Ubuntu is `/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin`
— and **`~/.cargo/bin` is not in it**. So

```sh
sudo -n perf record -e cycles -F 4999 -o out.data -- cargo bench -q -p fixbolt-engine --bench density
```

never runs `cargo`. What makes it a trap rather than an error is what `perf` does about it:

* it prints `Failed to collect 'cycles' for the 'cargo' workload: No such file or directory`
  **on stdout, mixed into the run's own output**, not as a shell error;
* it **still writes `out.data`** — 15 704 bytes of header with no samples;
* and it **exits 0**.

A driver that checks the exit status, or checks that the evidence file exists, or checks its
size is non-zero, records 21 successful profiling runs of a benchmark that never started. Each
one takes about a second, so even the wall-clock looks like a fast machine rather than a
failure.

`perf stat` has the same shape with a different sentence: `Workload failed: No such file or
directory`, and its `-o` file gets written with only the `# started on ...` line.

## What caught it

Nothing automatic. The manager read the log the driver was writing and saw two things that did
not fit: every run finished in **1 second** where the benchmark takes **5–6 minutes**, and the
line the driver grepped for was absent — it logged `NO DENSITY LINE`. The size and the exit
status both said green.

**That the driver logged its grep result at all is what made it visible.** A driver that logged
only "run 3/21 done" would have produced a complete, plausible, empty evidence set.

## The rule

* **A profiling run's evidence is a line from the workload, never the profiler's exit status or
  the existence of its output file.** Grep the run's own output for something only a real run
  prints, log what the grep found, and read it.
* **Never put `cargo` — or anything from `~/.cargo/bin`, `~/.local/bin` or a toolchain shim —
  after `sudo`.** Give an absolute path to a binary that exists, or arrange not to need root.
* On this desk root is needed at all only because `/proc/sys/kernel/perf_event_paranoid` is `4`.

## Guarded by

`scripts/check-sudo-names-what-root-can-find.sh`
([ADR-0093](../decisions/ADR-0093-a-campaign-driver-is-committed-sudo-in-a-committed-script-names-what-root-can-find-and-a-timer-due-inside-the-window-is-a-fail-row.md)
decision 2), since `ebe0525`: every `sudo` line in `scripts/` is read, tokenised, and judged —
**R2** fails outright on a bare `cargo`, `cargo-*`, `rustc`, `rustup`, `rustdoc` or `w2w` token
handed to root by name; **R1** requires every other bare command to carry a `/` or sit on a
fixed `ALLOW` list; **R3** lets a `perf … -- <workload>` line's token after `--` stand in for the
command R1/R2 judge, which is the exact shape of this page's trap. `scripts/check-sudo-verdicts.sh`
pins boot D's own line as a fixture — `sudo -n perf record -e cycles -F 4999 -o d.data -- cargo
bench -q -p fixbolt-engine --bench density` reads `FAIL R2 cargo` — and the two scripts are read
side by side in CI. `unfuse_quotes` (`67e2898`) turns a quote-fused word into one R2 can read
(`sudo sh -c 'cargo bench -q'` now also reads `FAIL R2 cargo`, where it used to read `ok`), and
fixed a false positive with the same cause (`sudo 'tee' /sys/x` no longer reads `FAIL R1`).
**What is still not guarded**: a word fused to a shell metacharacter — `&`, `;` or `|` — is
missed on purpose (`sudo sh -c 'true;cargo bench'` reads `ok`), because unfusing `|` would make
`sudo grep -E "cargo|rustc" /etc/x` tokenise as a toolchain invocation and turn a legitimate
line red.

## What replaced it here

`target/boot-d-evidence/run-d2.sh` runs the **prebuilt bench binary** named in
`../fb-boot-d/MANIFEST.txt` by absolute path, re-checking its sha256 before each run. This is
closer to what [boot D](../plans/2026-09-20-boot-d.md) asks for than the plan's own D2 cell was:
the boot is forbidden to compile anything, and invoking `cargo` at all was the only thing in
that cell that could have.

Related: [a-doc-command-that-exits-zero-is-not-the-doc-gate](a-doc-command-that-exits-zero-is-not-the-doc-gate.md),
[reading-the-output-you-grepped-for](reading-the-output-you-grepped-for.md),
[a-green-fraction-over-a-scenario-that-never-ran](a-green-fraction-over-a-scenario-that-never-ran.md).
