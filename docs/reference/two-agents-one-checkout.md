# Two agents, one checkout

`[found 2026-09-20, docs/plans/2026-09-20-the-desk-free-residue.md steps 1 and 2]` Two
subagents were briefed on disjoint files in one checkout, which `CLAUDE.md` §12 allows —
"one file, one writer at a time." One ran `cargo fmt` at workspace scope while the other had
`crates/dict/build.rs` deliberately damaged, mid-reversal, for `Not proven` bullet (b). Nothing
was corrupted this time: `build.rs` was restored byte-identical and both gates came back green.
But the fmt run could have reformatted the other agent's in-flight file, and neither agent had
any way to detect it — a diff read after the fact would have looked like ordinary drift.

**Disjoint files are not sufficient on their own.** A workspace-scoped command is a write to
every file in the workspace, whatever the brief said: `cargo fmt` with no `-p`, `sed -i` over a
glob, `cargo clippy --fix`. "One writer per file" holds only if every writer's commands are
scoped to its own files too.

**Rule.** A step that shares a checkout with another step scopes its formatters and fixers to
its own package — `cargo fmt -p <crate>`, not `cargo fmt --all` — or takes a worktree instead.

**Honest limit.** Nothing machine-checks this, and a reviewer reading a diff cannot see a
near-miss that left no trace. This is a brief-writing rule, guarded by the manager choosing
what to hand out, and this page is the record of why.
