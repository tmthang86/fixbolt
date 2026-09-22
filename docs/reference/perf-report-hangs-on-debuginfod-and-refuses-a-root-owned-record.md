# `perf report` hangs on `DEBUGINFOD_URLS`, and refuses a record that `sudo perf record` left owned by root

`[2026-09-23, boot E S2]` Two traps in reading a `perf record` taken on the §9 desk, each
costing minutes of a run that had already ended:

1. **`DEBUGINFOD_URLS` makes `perf report` sit at 0 % CPU.** Ubuntu's `/etc/profile.d` sets
   `DEBUGINFOD_URLS=https://debuginfod.ubuntu.com`; `perf report` queries it for every DSO and
   waits. Two reports sat 3 and 6 minutes with no output. With `DEBUGINFOD_URLS=` (empty) the
   same report finishes in 1.1 s. Prefix every `perf report` / `perf annotate` / `perf diff`
   with `DEBUGINFOD_URLS=`.
2. **A `.data` written by `sudo -n perf record` is `root`-owned, mode 600**, and `chown` to the
   user is not enough: `perf report` still says the file is "not owned by current user or
   root" unless given `-f`. Run it as the user with `-f`, or `chown` *and* `-f`.

A third, older one still applies: a `-g` record of a release binary without frame pointers
gives callchains that are garbage — `children ≡ self` for every symbol
([measured-costs](measured-costs.md) *Boot E, Item 96*). Frame pointers or
`--call-graph dwarf` with debuginfo are the only two ways to an inclusive share.

## Guarded by

Nothing. The commands live in plan rows and in `docs/hft-playbook.md`; a wrapper script that
sets `DEBUGINFOD_URLS=` and passes `-f` would close both, and is open by omission.
