# ADR-0093 — A campaign driver is committed, `sudo` in a committed script names what root can find, and a timer due inside the window is a `FAIL` row

- **Status**: Proposed — 2026-09-22
- **Revised 2026-09-22, same day, after the plan's step 3 landed (`ebe0525`)** — revised in
  place as `CLAUDE.md` §5 allows for a `Proposed` ADR, the revision recorded here: decision 2
  gains gap **G5** (the gate names its own verdict test out of scope, so a real bad line
  *there* is never caught — with a proposed remedy, the plan's *Sửa 2* step 3b), **G2** is
  softened to say it has no fixture, and the CI wording names the step the gate follows, not
  a job name. No decision changes. **Second pass, same day, after steps 3b, 2, 4 and the
  tokeniser fix landed (`9e33aa9`, `c320b7a`, `99e8564`, `67e2898`)**, in this same block
  rather than a second one: decision 3 says **UTC**, not local time, because the code does and
  the reason is worth recording; decision 2 says **what a token is**; **G2's first half was
  wrong, not merely untested** — R2 did *not* reach a wrapper body when a quote was fused to
  the word, and R1 flagged clean lines for the same cause — and is replaced by what `67e2898`
  fixed and what remains; **G5 is closed** by step 3b; **G6** (a word fused to a shell
  metacharacter, the script header's `G2b`) is added. Still no decision changes.
- **Approved by**: nobody yet. Written by the architect for the plan
  [the-detector-and-the-campaign-preconditions](../plans/2026-09-22-the-detector-and-the-campaign-preconditions.md),
  step 0; the owner approved the *scope* of that plan on 2026-09-22, not this text.
- **Date**: 2026-09-22
- **Deciders**: Tran Manh Thang. Written by the architect (Fable). The `sudo` inventory of
  `scripts/` is the manager's, reproduced by the architect the same day; the `sudo` and
  `systemd` facts were read from those projects' source (*What the search found*).
- **Related**:
  [ADR-0090](ADR-0090-a-measurement-boot-is-pre-built-shares-one-control-arm-and-a-five-step-drift-is-closed-one-named-segment-at-a-time.md)
  decision 2 and its *Good* consequence *"the rotation driver is a committed procedure; the
  next boot does not rewrite `c91.sh` by hand"* — decision 1 below generalises that sentence;
  [ADR-0061](ADR-0061-the-scratch-fixture-gate-is-a-regex-and-says-so.md) (a grep-shaped gate
  states its gaps and stays a regex — the precedent for decision 2's shape);
  [ADR-0023](ADR-0023-section-9-records-the-cpu-mitigations.md) (a `DESIGN.md` §9 row is an
  OS condition `check-machine.sh` reads — decision 3 adds one);
  [ADR-0087](ADR-0087-a-socket-harness-settles-on-counted-records-and-the-clock-waits-for-the-engine.md)
  decision 5 (a count, never a latency number).
  `docs/reference/`
  [perf-record-exits-zero-when-sudo-cannot-find-the-workload](../reference/perf-record-exits-zero-when-sudo-cannot-find-the-workload.md)
  and
  [a-quiet-machine-check-cannot-see-a-timer-that-has-not-fired](../reference/a-quiet-machine-check-cannot-see-a-timer-that-has-not-fired.md)
  — the two traps this ADR gives a gate. `STATUS.md` open item 98; `CLAUDE.md` §2
  non-negotiable 10, §4 (*every recorded trap gets a regression test*), §10.
- **Answers**: where a measurement campaign's driver scripts live; what a gate over `sudo`
  lines can and cannot check, and why it is worth building only if the first answer is "in
  the repository"; and what `check-machine.sh` says about a systemd timer that will fire
  during the campaign, with which window and which verdict.

## Context

1. **The `sudo` trap was in a driver that no longer exists.** Boot D's `run-d2.sh`,
   `run-d3.sh` and `run-d5.sh` lived in gitignored `target/boot-d-evidence/` and are gone with
   the desk's teardown; they cannot be recovered (`STATUS.md` item 98). The line that cost
   five silent runs was `sudo -n perf record … -- cargo bench …`, and **the boot-d plan's own
   D2 cell wrote it that way** (`docs/plans/2026-09-20-boot-d.md:309`): the driver copied the
   plan. `perf` is on root's path; `cargo` is not; `perf` printed the failure on stdout, wrote
   a header-only `.data` and exited 0.
2. **There is not one real `sudo` invocation in `scripts/` today.** `[measured 2026-09-22,
   manager, cloud container, `0629111`]` — the command-position regex of
   `check-scratch-fixtures.sh:185` with `sudo` in place of `cp`, plus the `is_live()` comment
   filter, over every `scripts/*.sh`, finds **six** lines, all in `check-machine.sh` and all
   inside a double-quoted advisory string handed to `row()` as the fix command:

   ```text
   scripts/check-machine.sh:391:     "on AMD: echo 0 | sudo tee /sys/devices/system/cpu/cpufreq/boost"
   scripts/check-machine.sh:394:     "echo 1 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo   (Intel) or boost=0 (AMD)"
   scripts/check-machine.sh:414:   *) row FAIL "SMT / hyperthreading" "$smt" "echo off | sudo tee /sys/devices/system/cpu/smt/control" ;;
   scripts/check-machine.sh:425:     "echo never | sudo tee /sys/kernel/mm/transparent_hugepage/enabled"
   scripts/check-machine.sh:620:         "echo <non-isolated-cpu> | sudo tee /proc/irq/<n>/smp_affinity_list"
   scripts/check-machine.sh:669:         "sudo systemctl stop irqbalance && sudo systemctl disable irqbalance"
   ```

   The remaining `sudo` occurrences (`check-machine.sh:381,440,450,563,566,651,652,696`,
   `check-ktls-available.sh:60`, `check-no-kernel-sleep.sh:73`,
   `check-standard-gives-the-core-back.sh:80`; architect's `grep`, same day) sit after a
   quote or a plain word, not in command position. So: a gate anchored on **the position of
   `sudo`** is red on `main` from birth with zero true positives, and a gate scoped to
   `scripts/` would have read nothing on the day the trap fired. Its value is entirely
   contingent on where drivers live — decision 1 decides that first.
3. **The advisory strings are the thing a human pastes.** A fix line saying `sudo cargo …`
   would carry the trap to whoever runs it. So a string is *not* a false positive to be
   filtered out; it is a line to be read by the same rule as an invocation. What made the six
   look like false positives is that the borrowed regex read where `sudo` **sits**, not what
   it **runs** — and `tee` and `systemctl` are on root's path.
4. **The timer trap cost eight rounds and no wrong number.** `apt-daily-upgrade.timer` at
   06:51 during a 23:36 → 11:46 rotation; the per-round quiet rule dropped rounds 13–20
   correctly. `check-machine.sh`'s quiet row reads one second, now. Boot D's `pass 16 fail 0
   unknown 0` was true at 21:16 and said nothing about 06:51. The reference page records the
   remedy the manager applied by hand (`systemctl stop` — not `disable` — of six units) and
   the rule *"before a campaign longer than an hour, read `systemctl list-timers --all`"*.
5. **`check-machine.sh`'s contract**: `row PASS|FAIL|UNKNOWN name value fixcmd` (`:40-48`);
   exit 1 when `fail > 0` or `unknown > 1` (`:704-710`); *"`unknown` is NOT a pass"* (header
   `:13-15`). An ordinary desktop is already red on this script for a dozen rows
   (mitigations, governor, isolation, NIC), so a new `FAIL` row does not change which
   machines are red; `bench.sh` ignores the exit code except under `--strict`, which is for a
   §9 machine. `ab-rotation.sh` reads only the quiet row (`quiet_status`, `:440-450`).
   `check-machine.sh` does not use `jq` today (`grep -c jq` → 0); `ab-rotation.sh` and
   `bench.sh` do.
6. **The campaign length is not knowable by the check.** The check runs at one instant; the
   rotation that follows runs for as long as `ROUNDS × arms × suite time` takes. Boot D's
   D4 ran 12 h 10 min; boot C ran ~6 h. The longest campaign on record is the only
   principled default for a window the caller did not declare.

### What the search found

`[searched 2026-09-22]`

- **`sudo` applies `secure_path` whether or not `env_reset` is on, so `sudo -E` does not
  rescue `cargo`.** `plugins/sudoers/env.c`, `rebuild_env()`, *after* the
  `if (def_env_reset || …)` block closes:

  ```c
  /* Replace the PATH envariable with a secure one? */
  if (def_secure_path && !user_is_exempt(ctx)) {
      CHECK_SETENV2("PATH", def_secure_path, true, true);
  ```

  (<https://raw.githubusercontent.com/sudo-project/sudo/main/plugins/sudoers/env.c>,
  fetched). `sudoers(5)` under `env_reset`: *"If the secure_path setting is enabled, its
  value will be used for the PATH environment variable"*
  (<https://raw.githubusercontent.com/sudo-project/sudo/main/docs/sudoers.man.in>, fetched;
  the `secure_path` entry itself was beyond what the fetch returned, and `sudo.ws` and
  `man7.org` are blocked from this container — the source file above is the authority
  used). The only escape is `exempt_group`, a sudoers setting, not a flag.
- **The value is readable at run time only as root.** `sudo -V` as root prints
  `Value to override user's $PATH with: /usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/snap/bin`
  `[measured 2026-09-22, architect, this container, sudo 1.9.15p5]`; `/etc/sudoers` is
  `0440 root`. `cargo` here is `/root/.cargo/bin/cargo` — rustup's home, not on that list.
  The reference page quoted the Ubuntu list without `/snap/bin`; the measured one has it.
  A gate that must run on a CI runner (no `perf`, no root) cannot resolve names against a
  live path — it must carry a **pinned** answer.
- **No off-the-shelf lint for this.** ShellCheck has no rule about `sudo` and `PATH`
  (nothing found for it); the rustup documentation's advice for `sudo cargo` is an absolute
  path or `sudo -E env PATH=$PATH …` — and the second is wrong under `secure_path`, per the
  source above. The search found no project gating this in CI; the shape below is this
  project's own, on ADR-0061's precedent.
- **`systemctl list-timers` has a JSON form with fixed keys.**
  `src/systemctl/systemctl-list-units.c`, `output_timers_list`:
  `table_new("next", "left", "last", "passed", "unit", "activates")`, `next`/`left` typed
  `TABLE_TIMESTAMP`/`TABLE_TIMESTAMP_LEFT`; `src/systemctl/systemctl-util.c`,
  `output_table()`: `if (OUTPUT_MODE_IS_JSON(arg_output)) r = table_print_json(table, …)`;
  `src/shared/format-table.c`, JSON conversion: a timestamp cell is
  `sd_json_variant_new_unsigned(ret, d->timestamp)` in **microseconds since the epoch**, or
  **`null` when `USEC_INFINITY`** (an inactive timer under `--all`) — all three files fetched
  from `raw.githubusercontent.com/systemd/systemd/main`. `[measured 2026-09-22, this
  container, systemd 255]` `systemctl list-timers --output=bogus` is refused at argument
  parsing (`Unknown output 'bogus'.`) while `--output=json` proceeds to the bus — so 255,
  the version Ubuntu 24.04 ships and the desk runs, accepts it. **Not found**: the NEWS
  entry naming the version that added it (the fetch of `NEWS` returned nothing on it); the
  step that builds the row reads the value off the desk and records the version.
- **`systemd-analyze calendar`** predicts elapses from a `OnCalendar=` expression but not the
  randomized delay; `list-timers`'s `next` is the instant systemd has already scheduled,
  delay included — the reference page's *"prints the next one outright"*. The JSON form is
  therefore strictly better than parsing the table or recomputing the schedule.

## Decision

### 1. A campaign driver is committed to `scripts/` before the boot, and `target/` holds evidence only

Any script a measurement campaign runs whose output a plan, `STATUS.md` or
`measured-costs.md` will cite **is in `scripts/` on the plan's branch before the reboot**,
named in the plan's *Chia việc* row that runs it, and its header states what it cannot see —
the shape `ab-rotation.sh` already has. `target/` and the evidence directory hold **evidence**
(`.data`, `.txt`, manifests), never code. This is `CLAUDE.md` §2 non-negotiable 10 read
literally — *"no performance number without the committed benchmark that produced it"* — a
driver that wraps `perf` around the benchmark is part of what produced the number.

What the next campaign owes, concretely: its D2/D3/D5-shaped steps name a committed script
(`scripts/perf-endpoints.sh` or whatever the plan calls it) instead of a command line in a
plan cell; the plan cell quotes the script path and its arguments; the script runs the
manifest-pinned binary by absolute path (the fix boot D applied by hand); and decision 2's
gate is green on the branch before D0. The three dead files are **not** reconstructed — their
contents are not evidence of anything and are not on disk.

### 2. `scripts/check-sudo-names-what-root-can-find.sh` reads what `sudo` runs, not where `sudo` sits

Over every shell file in `scripts/` (the enumeration of `check-scratch-fixtures.sh:135-152`)
and any paths given as arguments, on **live lines** (`is_live()`; backslash-continued lines
joined first, so a workload on the next physical line is still on the same logical line),
**every** occurrence of the word `sudo` — in command position, after a pipe, inside a quoted
advisory string alike (fact 3: over-reading is the safe direction) — is read by three rules:

- **R1 — the command word.** After `sudo`'s own options (`-n`, `-E`, `-u <user>`, `--`, and
  `VAR=value` assignments) the next token is the command. It passes when it **contains a
  `/`** (resolved by path, not by `PATH`), when it **begins with `$`** (a variable — the gate
  cannot see its value; gap G1), or when it is on the pinned **ALLOW** list of names that
  live under the default `secure_path` on Debian/Ubuntu: `apt`, `apt-get`, `bash`, `cat`,
  `chrt`, `cpupower`, `dmesg`, `env`, `ethtool`, `ip`, `journalctl`, `kill`, `modprobe`,
  `nft`, `nice`, `perf`, `pkill`, `setcap`, `sh`, `sysctl`, `systemctl`, `taskset`, `tee`,
  `update-grub`. Anything else is a **FAIL** naming file, line and word. A name is added to
  ALLOW only with the evidence recorded in the script's header: `dpkg -S "$(command -v X)"`
  showing a path under one of the seven `secure_path` directories.
- **R2 — the toolchain by name, anywhere after `sudo`.** Any bare token (no `/`) on the
  logical line after `sudo` that is `cargo`, `cargo-<anything>`, `rustc`, `rustup`, `rustdoc`
  or `w2w` is a **FAIL** — this is the rule that catches boot D's line, where the command
  word `perf` passes R1 and the workload after `--` is what root could not find. It also
  catches `sudo sh -c 'cargo …'` and `sudo env PATH=$PATH cargo …` (which does not work under
  `secure_path` anyway).
\1- **What a token is** *(added in the revision)*: the logical line is split on whitespace\n  **and on every `'` and `"`**, each quote replaced by a space, never deleted (`67e2898`,
  `unfuse_quotes`, used at all three tokenisation sites). A shell quote is a delimiter in
  every context and can never be part of a command *name*, so unfusing cannot change what a
  legitimate command word is; deleting instead of replacing would turn `sh -c'cargo bench'`
  into `-ccargo` and miss it again. The shell metacharacters `&`, `;`, `|` are **not**
  delimiters here — G6 says why.

Zero scripts scanned is a FAIL (`check-scratch-fixtures.sh` B4). The script prints one `ok`
line with the counts it read (`N scripts, M sudo lines, 0 findings`) so a green says what
it looked at. Its rules are pure functions sourced by a sibling
`scripts/check-sudo-verdicts.sh`, which feeds them boot D's exact line, the six advisory
lines above, the absolute-path form and the `-E` form, the way `check-machine-verdicts.sh`
feeds `virt_verdict`. In CI the gate runs **immediately after the `check-scratch-fixtures.sh`
step** (whichever job holds that step — `lint-config` at `ebe0525`), the verdict test runs
**beside `check-machine-verdicts.sh`** (`script-logic`), and both scripts join the
`shellcheck -S info` list of the step next to `check-scratch-fixtures.sh`. Named by step, not
by job, so a renamed job does not make this sentence wrong.

**On `main` at `0629111` the gate is green by construction**: every `sudo` there runs `tee`,
`systemctl`, `sysctl`, `cpupower`, `modprobe` or `ethtool` — all on ALLOW — and no line
carries an R2 word. The plan's step quotes the count.

**Known gaps, stated as ADR-0061 states its own**: **G1** a command word in a variable
(`sudo -n "$BIN"`) passes R1 unread — the *intended* fix pattern uses exactly this, so the
gate cannot forbid it. **G2** *(first half replaced in the revision — the original text was
**wrong**, not "untested")*: as built at `ebe0525` the gate did **not** apply R2 inside a
wrapper body when a quote touched the word — `sudo sh -c 'cargo bench -q'` read `ok`, the
gate's entire purpose passed, and `sudo nice -n -20 'cargo' bench`, not a wrapper body at all,
read `ok` too — while R1 flagged clean lines, `sudo 'tee' /sys/x` and `sudo "systemctl" stop x`
→ `FAIL R1`, a false positive that gets a gate switched off. `[measured 2026-09-22, manager,
before any fix existed]`. The cause was not wrapper handling: `read -ra` split on whitespace
only, the token was `'cargo` or `'tee'`, and every comparison in R1, R2 **and** R3 is exact —
one tokenisation defect shared by all three rules, with two signs. Fixed in `67e2898` by the
token rule above, four reversals, one per site plus replace-versus-delete. **What G2 now is,
narrower and deliberate**: R1 is still not applied inside a wrapper body (`sh -c '…'`,
`bash -c '…'`, `env …`, `nice …`), because a body's first word is often not its command (`cd /x
&& …`, `echo 0 > …`, `VAR=1 …`) and ALLOW over body words would flag ordinary arguments; what
that leaves uncovered, exactly, is *an unresolvable **name** inside a wrapper body that is not
one of R2's six toolchain names*. Both halves have fixtures in `check-sudo-verdicts.sh`.
**G3** prose — a plan cell — is not a script, and boot D's trap was written in a plan cell
first; decision 1 is what moves the line into a file the gate reads. **G4** ALLOW is static and
says nothing about whether the package is installed on the machine that will run the line —
that failure is loud (`command not found`, exit 127) and is not the trap. **G5** *(added, then
closed, in the revision)*: at `ebe0525` the gate named exactly one file out of its default
scope, `scripts/check-sudo-verdicts.sh`, because its fixtures are byte-identical to real bad
lines — boot D's among them — handed to `sudo_verdict()` as strings; the first self-scan read 9
"findings", all fixtures, and nothing structural tells such a string from an advisory one. The
consequence was that a real bad `sudo` line pasted into that file — the likeliest place for
one — was never caught. **Closed at `9e33aa9`** (plan *Sửa 2*, step 3b): no file is excluded;
a trailing `# check-sudo: fixture` marker is honoured **only** inside that one file, gated on
basename before the helper is consulted, so a marker elsewhere is inert by construction; the
skipped count is printed on the `ok` line (`32 scripts scanned … 11 fixture lines skipped by
marker` at `9e33aa9`, N up by exactly one from 31); proved in both directions — an unmarked
`sudo -n perf record -o x.data -- cargo bench` appended to the verdict test was caught at
`scripts/check-sudo-verdicts.sh:182`, and the marker helper's test was red before it existed
(`want [skip] got []`). **G6** *(added in the revision; the script header calls it `G2b`)*: a
target word fused to a shell **metacharacter** rather than a quote is still unread — `sudo sh
-c 'cd /x&&cargo bench'`, `'true;cargo bench'`, `'echo x|cargo bench'` all read `ok`
`[measured 2026-09-22, manager]` — because `&`, `;` and `|` are deliberately **not** unfused:
unfusing `|` makes `sudo grep -E "cargo|rustc" /etc/x` tokenise to `grep -E  cargo rustc
/etc/x`, and R2 would then flag a grep pattern list as a toolchain invocation (verified by
applying the substitution by hand) — the false positive that gets a gate switched off. Quote
characters carry no such risk; metacharacters do. Pinned by an `ok`-expecting fixture in
`check-sudo-verdicts.sh`, so it is a measured statement, not prose. Open by decision.

### 3. `check-machine.sh` gains a row `no timer due`, a `FAIL` inside a declared window, default 12 hours

- The row runs `systemctl list-timers --all --output=json`, reads each object's `unit` and
  `next` (µs since the epoch, or `null`), and passes them with `now` and the window to a pure
  function `timers_verdict <now_usec> <window_sec> <json>` tested by
  `check-machine-verdicts.sh`.
- **`FAIL`** when any timer's `next` is `≤ now + window` — including a `next` in the past
  (elapsed and its service possibly still running; the quiet row's territory, but
  over-reading is the safe direction). The value names every such unit with its next firing **in UTC** and how far
  off it is (`in 12h00m`, `overdue 5m00s`) — UTC, not the reader's local time *(revised to
  match `99e8564`)*: `timers_verdict` takes `now` as an argument and never calls `date`, which
  is what lets `check-machine-verdicts.sh` pin it with a fixed epoch; a formatter keyed to the
  test runner's timezone cannot be pinned by an exact-match assertion and would push that test
  down to a substring check, and the operationally useful part — how far off — is
  timezone-free anyway; the fix line is `sudo -n systemctl stop <unit>` per unit — *stop, not
  disable*, so the next boot restores it (the reference page's rule; `systemctl` is on
  ALLOW, so the fix line passes decision 2). `null` (`--all`'s inactive timers) is ignored.
- **`UNKNOWN`**, with the reason in the value, when there is no `systemctl`, when it cannot
  reach PID 1 (a container, macOS, WSL without systemd), or when `jq` is not installed. A
  desktop, a laptop and the CI runner already carry `unknown`s or `FAIL`s; the row adds one
  more true statement.
- **The window is `FIXBOLT_TIMER_WINDOW`, in hours, default `12`** — the length of the
  longest campaign on record (fact 6). The row prints the window it used in its value on
  every verdict, `PASS` included, so a reader sees what it could *not* see: a timer at 13 h
  on a 14-hour campaign. A campaign declares its own window (`FIXBOLT_TIMER_WINDOW=14`); the
  default is for a check nobody parametrised.
- **`ab-rotation.sh` refuses to start** (preflight, before round 1, the same shape as its
  `/tmp` refusal) when that row reads `FAIL`, printing the units and the fix lines. Per round
  it still reads only the quiet row: a timer that fires anyway is caught there, per arm, as
  boot D proved.
- `DESIGN.md` §9 gains the row (*"No systemd timer due inside the campaign window"*), because
  a §9 row is an OS condition `check-machine.sh` reads (ADR-0023), and `docs/hft-playbook.md`
  §6 gains the procedure line. Verdict `FAIL`, not `UNKNOWN`: the row *can* see the answer,
  and `UNKNOWN` is reserved for when it cannot (fact 5).

## Alternatives considered

- **Leave drivers in `target/`, disposable per boot.** Simpler for the person at the desk;
  rejected because the three that died took with them the only record of how eleven `.data`
  files were produced, and because non-negotiable 10 does not have a "per-boot" exception.
- **Gate B scoped to `scripts/` with drivers in `target/`.** Rejected by fact 2: a gate over
  a directory where the footgun has never appeared, built to satisfy §4's sentence rather
  than to catch anything. B without C is not built.
- **Gate B anchored on the position of `sudo`** (the borrowed `CP_POSITION_RE`). Rejected by
  fact 2: red on `main` with zero true positives, and blind to the actual trap (`perf` in
  command position, `cargo` after `--`).
- **Gate B reading `secure_path` at run time** (`sudo -V`, `/etc/sudoers`). Rejected: needs
  root, and a runner without `perf` would read every `sudo perf` as red. The list is pinned;
  the price is a list to maintain, with the evidence rule above.
- **A `check-machine.sh` row that is `UNKNOWN` on a pending timer.** Rejected: `UNKNOWN`
  means "could not read", and this row can. One `unknown` is tolerated by the verdict
  (`unknown > 1` fails) — precisely the tolerance that would let a known 06:51 firing pass.
- **A fixed window with no parameter.** Rejected: the check cannot know the campaign's
  length, and a default that cannot be raised would be raised by editing the script during
  a boot.
- **Parsing the `list-timers` table, or `systemctl show -p NextElapseUSecRealtime`.**
  Rejected: the table's `NEXT` is a localised timestamp string; `show` prints a formatted
  time too; the JSON form gives µs integers and `null`, from the same table object.
- **`systemd-analyze calendar` on each timer's `OnCalendar=`.** Rejected: it does not include
  `RandomizedDelaySec`; `next` does.

## Consequences

**Good**

- The next campaign's drivers are reviewable, gated, and survive the desk; a plan cell that
  writes `sudo … cargo` is caught the moment it becomes a script.
- Boot D's exact line is a red fixture in CI on every commit, and the six advisory lines are
  green on the same rule — the gate reads the command, so advice is held to the same standard
  as invocation.
- A campaign that will cross a timer is told so before its first round, with the `stop`
  lines to paste, and the §9 checklist says why.
- Every verdict of the new row names its window, so a green cannot be read as "nothing will
  fire tonight".

**Bad — and accepted**

- **ALLOW is a list somebody maintains.** A new tool after `sudo` is red until added with
  evidence; that is the over-matching direction chosen on purpose, and the cost is one
  commit per new name.
- **G1, G2, G3, G4 and G6 are real; G5 was real for five commits.** A `$BIN` after `sudo` is
  unread; an unresolvable name inside a wrapper body that is not a toolchain name is unread
  (G2, the narrowed one); a word fused to `&`, `;` or `|` is unread (G6); a plan cell is
  unread; the gate is a regex over lines, as ADR-0061 accepted for its sibling. And the
  first G2 text was **false**: the gate shipped at `ebe0525` passed `sudo sh -c 'cargo bench
  -q'` — its whole purpose — and flagged `sudo 'tee' /sys/x`. An ADR that had recorded
  "untested" where the truth was "wrong" would have taught the wrong lesson; this one says
  wrong. The mitigation for G1 is decision
  1's rule that the variable holds a manifest-pinned absolute path, held by the driver's own
  sha256 check, not by this gate.
- **The timers row makes `check-machine.sh` red on any systemd desktop with default package
  timers.** Those desktops were already red on a dozen rows; a §9 desk must now `stop` its
  timers before a campaign — which is what boot D ended up doing by hand at 07:00.
- **The CI runner's machine block gains a row that may read `FAIL`** (GitHub's Ubuntu images
  carry timers). `bench.sh` ignores the exit code there; the printed count changes and
  nothing else.
- **`jq` becomes a soft dependency of `check-machine.sh`** (`UNKNOWN` without it). The desk
  and every machine that runs `ab-rotation.sh` already have it.
- **Twelve hours is a number chosen from two campaigns.** A longer campaign that does not set
  the variable is protected only up to twelve; the row says so on every line it prints.

## Sources

- `scripts/check-machine.sh` at `0629111`: `:1-33` (contract), `:40-48` (`row`), `:454-546`
  (quiet row), `:704-712` (verdict). `scripts/ab-rotation.sh` `:440-450`, `:229-240` (the
  `/tmp` refusal shape). `scripts/check-scratch-fixtures.sh` `:135-200`, ADR-0061.
- `docs/plans/2026-09-20-boot-d.md:309` (the D2 cell), `STATUS.md` item 98 and the
  2026-09-22 *Start here*.
- sudo: `plugins/sudoers/env.c` and `docs/sudoers.man.in` on `github.com/sudo-project/sudo`,
  fetched 2026-09-22; `sudo -V` and `/etc/sudoers` on this container, same day.
- systemd: `src/systemctl/systemctl-list-units.c`, `src/systemctl/systemctl-util.c`,
  `src/shared/format-table.c` on `github.com/systemd/systemd`, fetched 2026-09-22;
  `systemctl --version` 255 and the `--output=bogus` probe on this container, same day.
