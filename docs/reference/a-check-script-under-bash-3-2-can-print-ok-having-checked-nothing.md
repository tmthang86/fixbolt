# A check script under bash 3.2 can print ok having checked nothing

`[measured 2026-09-26]`

## The trap

macOS ships `/bin/bash` 3.2.57, and `#!/usr/bin/env bash` picks it up on a Mac with no newer
bash on `PATH`. Bash 3.2 has no associative arrays (`declare -A`) and no `mapfile`. A script
written for CI's bash 5 does not necessarily stop when it meets them: without `set -e`, a failed
`declare -A` or a missing `mapfile` is one line of stderr, and the script runs on with an empty or
mangled array.

Two of this repository's gates did exactly that, exit 0:

```
$ /bin/bash scripts/check-dict-spec-pin.sh; echo rc=$?
scripts/check-dict-spec-pin.sh: line 71: FIX44.xml: syntax error: invalid arithmetic operator (error token is ".xml")
scripts/check-dict-spec-pin.sh: line 81: FIX44.xml: syntax error: invalid arithmetic operator (error token is ".xml")
check-dict-spec-pin: ok — pin 386ce46e917ae494ab6e90b1be90fd421cdbe3f9, three files, two NOTICE copies
rc=0
```

The table of expected sha256 values never existed, so **no hash was compared**, and the verdict
was `ok`. `check-sudo-names-what-root-can-find.sh` did the same: under 3.2 it printed
`ok — 49 scripts scanned, 0 sudo lines read`; under bash 5 on the same tree it reads 37.

Three others failed, but loudly for the wrong reason: `check-no-crate-root-allow.sh` exit 127
(`mapfile: command not found`), `check-scratch-fixtures.sh` exit 2 (`declare: -A: invalid
option`), `check-feature-gated-tests-ran.sh` exit 1 after two `unbound variable` lines. A sixth,
`check-semver-against-tag.sh`, could not be run on the desk (no `cargo-semver-checks`).

## What it cost

It was found while closing step 18 of plan `2026-09-26-docs-for-embedders` on a Mac:
`check-dict-spec-pin.sh`'s `ok` was about to be quoted as evidence that the three shipped
QuickFIX files were unchanged. The hashes were then checked by hand. CI was never affected — its
runners have bash 5 — so the damage is to the desk, where a green is read and believed before a
push.

## The fix

Every `scripts/check-*.sh` whose code uses `declare -A`, `mapfile` or `readarray` refuses to run
under bash < 4, right after its `set` line:

```
$ /bin/bash scripts/check-dict-spec-pin.sh; echo rc=$?
check-dict-spec-pin: FAIL — needs bash 4 or newer (declare -A); this is bash 3.2.57(1)-release. Put a newer bash first on PATH; under macOS's /bin/bash 3.2 this script can report ok having checked nothing.
rc=2
```

The guard went into all six. Only two passed silently, but the other three fail for reasons that
look unrelated, and one rule is easier to hold than a list of exceptions. Scripts outside
`check-*.sh` that use the same features (`bench.sh`, `ab-rotation.sh`, `interop.sh` and others)
are not gates, and were left alone.

To run the gates on a Mac, put a bash ≥ 4 first on `PATH` (Homebrew's `bash`), or run them in a
Linux container as [a-linux-only-module-is-invisible-to-a-mac-gate](a-linux-only-module-is-invisible-to-a-mac-gate.md)
describes.

## Regression test

`scripts/check-old-bash-is-refused.sh`, in the `lint-config` CI job. It finds every
`scripts/check-*.sh` that uses the three features, requires the guard in each, then **runs** each
one under the official `bash:3.2` container image and requires exit 2 with the guard's sentence.
Reading the guard proves nothing about where it sits; running it does. Its two reversals are in
its header, each measured red: guard deleted (check 1, and check 2 reads `exit 0`), and guard
moved below the first `mapfile` (check 2: the script exits on something else first).

## The transferable half

A script that cannot do its job must fail. `set -u` does not make that true on its own — under
3.2 an unbound array did not stop `check-sudo`. Guard the precondition explicitly, and prove the
guard by running the old interpreter, not by reading the guard.
