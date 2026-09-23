#!/usr/bin/env bash
# The six published crates release in lockstep, and a stranger can never
# resolve a mix of them that no CI run built (ADR-0160 decisions 1 and 4).
#
# WHAT IT ASSERTS, read from the manifests themselves (python `tomllib`), not
# from `cargo metadata` — metadata resolves `version.workspace = true` to the
# same string a literal `version = "0.1.0"` gives, so it cannot tell a crate
# that inherits the workspace version from one that has quietly stopped:
#
#   1. `[workspace.package] version` exists.
#   2. Each of the six published crates inherits it (`version.workspace = true`)
#      and is not `publish = false`.
#   3. Every internal **normal** dependency of those six — `[dependencies]`,
#      `[build-dependencies]` and their `[target.*]` forms, i.e. whatever cargo
#      keeps in the `.crate` — carries `version = "=<workspace version>"`
#      beside its `path`. A path-only requirement is what `cargo package`
#      refuses (docs/reference/publishing-a-workspace-to-crates-io.md, trap 1);
#      a caret requirement is what lets a stranger mix two releases.
#   4. Every other workspace member is `publish = false` (ADR-0160 decision 2):
#      `cargo publish --workspace` would otherwise upload it.
#   5. `crates/<crate>/LICENSE-MIT` and `LICENSE-APACHE` exist for each of the
#      six and are byte-identical to the root copies. Only files under a package
#      root reach its `.crate` (ADR-0104 decision 4), so each crate carries a
#      copy, and a copy that drifts is a second licence.
#
# WHAT IT CANNOT SEE: dev-dependencies (path-only on purpose — cargo strips
# them, ADR-0160 decision 1); what actually lands in a `.crate` (that is
# `scripts/check-package-contents.sh`, and `cargo package --list`); whether the
# packaged sources build (the `package` CI job); the dict's `NOTICE` pair
# (`scripts/check-dict-spec-pin.sh` check 4 already holds that).
#
# The list of six is written here on purpose. Reading "the published crates"
# from the manifests would make a crate that was switched to `publish = false`
# by mistake vanish from the check instead of failing it.
#
# Exit 0 when every assertion holds; 1 on any FAIL (each printed); 2 when the
# script itself cannot run.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 2

python3 - "${ROOT}" <<'PY'
import pathlib
import sys
import tomllib

root = pathlib.Path(sys.argv[1])

# name -> directory, the six crates ADR-0160 publishes.
PUBLISHED = {
    "fixbolt-codec": "crates/codec",
    "fixbolt-dict": "crates/dict",
    "fixbolt-session": "crates/session",
    "fixbolt-engine": "crates/engine",
    "fixbolt-sbe": "crates/sbe",
    "fixbolt": "crates/library",
}
LICENCES = ("LICENSE-MIT", "LICENSE-APACHE")

fails = []


def load(path):
    try:
        with open(path, "rb") as f:
            return tomllib.load(f)
    except (OSError, tomllib.TOMLDecodeError) as e:
        print(f"check-release-versions: cannot read {path}: {e}", file=sys.stderr)
        sys.exit(2)


ws = load(root / "Cargo.toml")
workspace = ws.get("workspace", {})
ws_version = workspace.get("package", {}).get("version")
if not isinstance(ws_version, str):
    fails.append("FAIL workspace: [workspace.package] has no version")
    ws_version = None
want = f"={ws_version}" if ws_version else None

members = workspace.get("members", [])
member_names = {}
for m in members:
    manifest = load(root / m / "Cargo.toml")
    member_names[manifest.get("package", {}).get("name", m)] = m

for name, directory in PUBLISHED.items():
    if name not in member_names:
        fails.append(f"FAIL {name}: not a workspace member")
        continue
    if member_names[name] != directory:
        fails.append(f"FAIL {name}: expected at {directory}, found at {member_names[name]}")
        continue
    manifest = load(root / directory / "Cargo.toml")
    package = manifest.get("package", {})
    if package.get("version") != {"workspace": True}:
        fails.append(f"FAIL {name}: version is not inherited (version.workspace = true)")
    if package.get("publish") is False or package.get("publish") == []:
        fails.append(f"FAIL {name}: publish = false on a crate that is published")

    tables = []
    for key in ("dependencies", "build-dependencies"):
        tables.append((key, manifest.get(key, {})))
    for target, body in manifest.get("target", {}).items():
        for key in ("dependencies", "build-dependencies"):
            tables.append((f"target.{target}.{key}", body.get(key, {})))
    for table_name, table in tables:
        for dep_key, spec in table.items():
            if not isinstance(spec, dict):
                continue
            dep_name = spec.get("package", dep_key)
            if "path" not in spec and dep_name not in PUBLISHED:
                continue
            if dep_name not in PUBLISHED:
                fails.append(
                    f"FAIL {name}: [{table_name}] {dep_name} is a path dependency "
                    "on a crate that is not published"
                )
                continue
            version = spec.get("version")
            if version is None:
                fails.append(
                    f"FAIL {name}: dependency {dep_name} has no version requirement"
                )
            elif want is not None and version != want:
                fails.append(
                    f'FAIL {name}: dependency {dep_name} requirement "{version}" '
                    f'is not "{want}"'
                )

    for lic in LICENCES:
        original = root / lic
        copy = root / directory / lic
        if not original.is_file():
            fails.append(f"FAIL workspace: {lic} missing at the repository root")
        elif not copy.is_file():
            fails.append(f"FAIL {name}: {directory}/{lic} missing")
        elif original.read_bytes() != copy.read_bytes():
            fails.append(
                f"FAIL {name}: {lic} copies differ: {lic} and {directory}/{lic} "
                "must be byte-identical"
            )

for name, directory in sorted(member_names.items()):
    if name in PUBLISHED:
        continue
    package = load(root / directory / "Cargo.toml").get("package", {})
    if package.get("publish") is not False:
        fails.append(f"FAIL {name}: not one of the six published crates, and not publish = false")

for line in fails:
    print(line)
if fails:
    print(f"check-release-versions: {len(fails)} failure(s)")
    sys.exit(1)
print(
    f"check-release-versions: OK — {len(PUBLISHED)} crates at {ws_version}, every internal "
    f'normal dependency "{want}", {len(PUBLISHED) * len(LICENCES)} licence copies identical, '
    f"{len(member_names) - len(PUBLISHED)} other members publish = false"
)
PY
