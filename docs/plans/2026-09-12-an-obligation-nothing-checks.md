# Một nghĩa vụ không ai kiểm — đóng item 57, 64, 69, 70

> **Loại:** Plan · **Ngày:** 2026-09-12 · **Trạng thái:** Đã giao
> **Phạm vi:** bốn open item không cần máy §9, một pull request, một phiên. Gate `deny`,
> gate `check-scratch-fixtures.sh`, probe 3 của `mod doc_table` trong `crates/engine`, và
> đồng hồ của `crates/session`.

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Bốn item còn mở trong `STATUS.md` mà không cần máy đo §9 là **một họ**: một *nghĩa vụ* được ghi
ra — trong tài liệu, trong một quy tắc, trong một kiểu dữ liệu — nhưng **không có gì đứng canh
nó**.

- **Item 57** — gate giấy phép nói `licenses ok` về một đồ thị **thiếu dev-dependencies**. Hôm
  2026-09-08 đã đo là `cargo-deny` 0.20.2 "không thể cấu hình được", nên CI chỉ làm cho lỗ
  hổng *ồn* (so đếm với `cargo tree`) và che nó bằng một script riêng. Item vẫn mở vì lỗ hổng
  trong chính cargo-deny vẫn còn.
- **Item 69** — `scripts/check-scratch-fixtures.sh` là regex trên `bash`. Bốn cách qua mặt đã đo:
  `cp` nằm trong thân heredoc; `cp` chép *ra khỏi* thư mục scratch; `read -r TMP < <(mktemp -d)`;
  `TMP=/tmp` trần. Câu hỏi để lại cho architect: giữ regex và nói thật giới hạn, hay đọc `bash`
  bằng một bộ phân tích thật.
- **Item 70** — probe 3 của `mod doc_table` kiểm chiều ngược ("giá trị không được liệt kê thì
  parser phải từ chối") bằng **ba ứng viên cố định** `1`, `true`, `nope`. Bớt `9` khỏi ô *Values*
  của `TimestampPrecision` mà suite vẫn xanh.
- **Item 64** — `Session::now_ms` khởi tạo bằng `0`, chỉ `tick_inner` ghi vào nó, và `received`
  không nhận thời gian. Không test, không lint, không assertion nào nói *"đường phán xét chỉ được
  đi tới sau một tick"*. Item 63 là một lần đã trả giá: engine phán xét `Logon` khi chưa tick, và
  đổ cho đồng hồ của đối tác lệch hai nghìn năm.

Mục tiêu: **đóng cả bốn trong một PR**, mỗi mục có guard mới (hoặc quyết định được ghi thành văn
bản có địa chỉ), mỗi guard được chứng minh bằng reversal có câu FAIL viết sẵn. Chủ sở hữu đã giao
mọi quyết định kỹ thuật cho architect; plan này **chọn**, ghi phương án bị loại và lý do, không
để lại câu hỏi.

## Những gì đã biết chắc

### Đo tại chỗ hôm nay `[measured 2026-09-12]`, máy này (desk, `tmt-B450-I-AORUS-PRO-WIFI`), cây sạch ở `3302cc5`

Công cụ: `shellcheck` 0.11.0, `cargo-deny` 0.20.2, `rustc` 1.98.0. Mọi phép đo dưới đây chạy
trong scratchpad của phiên, **không sửa file nào trong cây**.

**1. `shellcheck -f json1` không có AST — chỉ có diagnostics.** File thử `probe.sh` chứa đúng
bốn cách qua mặt của item 69:

```bash
#!/usr/bin/env bash
set -euo pipefail
TMP="$(mktemp -d)"
CRATE="$TMP/lintcheck"
cat > "$CRATE/notes.txt" <<'DATA'
cp "$ROOT/rust-toolchain.toml" "$CRATE/rust-toolchain.toml"
DATA
cp "$CRATE/rust-toolchain.toml" "$ROOT/rust-toolchain.toml.copy"
read -r TMP2 < <(mktemp -d)
TMP3=/tmp
cd "$CRATE"
cargo build
```

`shellcheck -f json1 -S info probe.sh` in ra, nguyên văn:

```
{"comments":[{"file":"probe.sh","line":9,"endLine":9,"column":9,"endColumn":13,"level":"warning","code":2034,"message":"TMP2 appears unused. Verify use (or export if used externally).","fix":null},{"file":"probe.sh","line":10,"endLine":10,"column":1,"endColumn":5,"level":"warning","code":2034,"message":"TMP3 appears unused. Verify use (or export if used externally).","fix":null}]}
```

Không có token, không có node, không có gì về heredoc hay `cp`. `shellcheck --help` liệt kê các
format: `checkstyle, diff, gcc, json, json1, quiet, tty` — tất cả đều là format *báo lỗi*.
Phương án (b) của item 69 qua `shellcheck` là **không tới được**, không phải "khó".

**2. `cargo-deny` 0.20.2 có khoá `[licenses] include-dev`, và nó chính là cái item 57 thiếu.**
Sao `deny.toml` ra scratchpad, thêm một dòng `include-dev = true` ngay dưới `[licenses]`, rồi
đếm bằng đúng pipeline mà `ci.yml` dùng:

```
cargo deny --all-features list                              37 crates
cargo deny --config <scratch> --all-features list           46 crates
cargo tree --workspace --all-features --prefix none         46 crates
cargo tree … -e normal,build --prefix none                  37 crates
cargo deny --config <scratch> --all-features check licenses licenses ok
```

Reversal, trên bản sao `git archive HEAD` trong scratchpad (có `Cargo.lock`, `--offline`,
`option-ext` 0.2.0 — giấy phép `MPL-2.0` — đã có sẵn trong registry cache), thêm
`option-ext = "0.2"` vào bảng `[dev-dependencies]` **có sẵn** của `tools/jrnl/Cargo.toml`:

```
--- deny.toml như hiện tại
licenses ok
--- deny.toml + include-dev = true
error[rejected]: failed to satisfy license requirements
   ┌─ …/option-ext-0.2.0/Cargo.toml:21:12
21 │ license = "MPL-2.0"
   │            rejected: license is not explicitly allowed
   ├ MPL-2.0 - Mozilla Public License 2.0:
   ├ option-ext v0.2.0
licenses FAILED
```

`cargo deny … list | grep -c option-ext`: **0** với config hiện tại, **1** với `include-dev`.
Kết luận: điều `docs/reference/a-license-gate-that-cannot-see-dev-dependencies.md` viết —
*"The tool's blind spot could not be configured away"* — **sai**. Khoá đã thử hôm 2026-09-08 là
`[graph] exclude-dev`; khoá có tác dụng nằm ở bảng `[licenses]`. Cùng lúc,
`scripts/check-every-crate-is-licensed.sh` hôm nay in `71 external crates, every licence allowed`
(nó hỏi `cargo metadata` không lọc platform, nên đếm cả crate của Windows/macOS — con số khác 46
là vì thế, không phải vì sai).

**3. Rust đọc `+3` và `03` thành `3`.** Chương trình một dòng, `rustc` 1.98.0:

```
"+3" -> Ok(3)      "03" -> Ok(3)      "3" -> Ok(3)
"-3" -> Err(InvalidDigit)   "9" -> Ok(9)   "+9" -> Ok(9)   "009" -> Ok(9)
" 3" -> Err(InvalidDigit)   "3 " -> Err(InvalidDigit)
```

`TimestampPrecision` đi qua `number::<u32>` (`crates/engine/src/settings.rs:1272`) rồi
`Precision::from_fractional_digits`, nên parser hôm nay **chấp nhận `03`, `+3`, `009`** trong khi
`docs/CONFIGURATION.md:85` ghi `` `3`, `6` or `9` ``. Đây là một lỗ mà probe 3 mở rộng sẽ thấy —
phải tính trước, xem §C.

**4. `clippy::panic = "deny"` không nhìn thấy `debug_assert!`.** Crate scratch có
`rust-toolchain.toml` chép vào (đúng quy tắc của `check-scratch-fixtures.sh`), `[lints.clippy]`
đặt `unwrap_used`, `expect_used`, `panic` = `deny`:

```
lib.rs có debug_assert!(now_ms != 0) và một panic!("control"):
  error: `panic` should not be present in production code
  exit 101
bỏ panic!("control"), giữ debug_assert!:
  exit 0
```

Nghĩa là gợi ý của item 64 — "a debug assertion that `now_ms != 0`" — là **một `panic` mà lint
canh bất biến 7 không thấy**: lint khớp *cách viết* `panic!`, không khớp *nghĩa*. Đúng họ lỗi
của PR này. Bị loại, xem §D.

**5. Gate scratch hôm nay:** `scripts/check-scratch-fixtures.sh` → `ok — 20 scripts, 1 enter a
scratch dir, 1 pins`, exit 0. Không có gì trong `scripts/` viết bốn cách qua mặt ở mục 1 (bước 2
sẽ đo lại bằng `grep` và trích).

**6. Về `crates/session` (số dòng ở `3302cc5`, khác với số trong item 64 vì file đã đổi):**
`now_ms: 0` ở `lib.rs:1438`; chỉ `tick_inner` ghi (`:2116`); `received` (`:2258`) và
`received_with` (`:2265`) không nhận thời gian; `judge` (`:2923`) đọc `now_ms` ở `:3000`
(schedule) và `:3021-3027` (skew). **Không có `debug_assert` nào** trong `crates/session/src`.
`Refusal` (`:1052`) là enum không trường; `DropReason` (`:1099`) là `#[non_exhaustive]`;
`impl From<Refusal> for DropReason` (`:1207`) khớp đủ, **không có nhánh `_`**. Engine dùng
`DropReason::` ở 7 chỗ (`lib.rs:611,918,925,1211`, `conn.rs:430,545,713`), không chỗ nào là
`match` trên nó; `tools/` không dùng. Runner conformance tick tới `FIXED_TIME_OUT =
"20260828-12:00:00.000"` (`crates/conformance/src/script.rs:83`) — tức trong 59 định nghĩa
`now_ms` **khác 0**. `docs/GUIDE.md:606-607` đã viết: *"A session that has never ticked holds
zero and will refuse the first message it sees for clock skew."* — câu này sẽ sai sau §D.

**7. Test session nhận trước khi tick:** một `awk` thô đếm được **11** hàm test trong
`crates/session/tests/` gọi `.received` mà thân hàm không gọi `.tick` (5 ở `application.rs`, 4 ở
`goodbye.rs`, 1 ở `heartbeat.rs`, 1 ở `numbering.rs`). Con số này **không tin được**: helper như
`logged_on()` (`application.rs:93`) có tick. Số thật do bước 4 đo bằng cách chạy test.

**8. Về `mod doc_table`** (`crates/engine/src/settings.rs:1549-2329`): probe 3 ở `:2201-2258`,
ứng viên `["1", "true", "nope"]` ở `:2237`, `FLOOR` 10 (11 với `tls`). Ô *Values* dạng liệt kê
hôm nay: 9 cờ `` `Y` or `N` ``, `ConnectionType` `` `acceptor` or `initiator` ``,
`TimestampPrecision` `` `3`, `6` or `9` `` — `StartDay`/`Weekdays` không phải liệt kê (có `/` và
`…`) nên `enumerated()` bỏ qua, đúng ý. Ba bộ đọc giá trị: `flag` (`:799`) khớp đúng `"Y"`/`"N"`;
`ConnectionType` (`:928`) khớp đúng hai chuỗi; `TimestampPrecision` qua số (mục 3). Bộ đọc INI
`trim()` giá trị (`:906`); `#`, `;`, `[` chỉ có nghĩa **ở đầu cả dòng** (`:883-887`), nên một ký
tự như vậy nằm *trong giá trị* vẫn là giá trị.

**9. CI:** `shellcheck -S info` gọi đúng ba file (`ci.yml:77`); job `deny` ở `ci.yml:154-256`,
bước so đếm dùng `-e normal,build` bên `cargo tree`; `CARGO_TERM_COLOR: always` ở `:25`.

### Tra cứu trên internet `[researched 2026-09-12]`

- **cargo-deny, tài liệu `[licenses]`**
  ([embarkstudios.github.io/cargo-deny/checks/licenses/cfg.html](https://embarkstudios.github.io/cargo-deny/checks/licenses/cfg.html)):
  khoá `include-dev`, mặc định `false` — *"If `true`, licenses are checked even for
  dev-dependencies"*, và lý do mặc định tắt: *"dev-dependencies are not used by downstream crates,
  nor part of binary artifacts."* Cùng trang: `include-build`, mặc định `true`.
- **cargo-deny release notes** ([github.com/EmbarkStudios/cargo-deny/releases](https://github.com/EmbarkStudios/cargo-deny/releases)
  — cited as the releases page and not as its `CHANGELOG.md`, because `scripts/check-links.py`
  reads any absolute URL ending in a name this repository also has as a link home; see
  `STATUS.md` item 72):
  bản mới nhất **0.20.2 (2026-07-09)** — chính bản đang dùng, **không có gì để nâng cấp**. Không
  có dòng changelog nào nhắc `include-dev`; tìm kiếm chỉ ra PR#557 (khoảng 0.14.x) là nơi
  dev-dependencies bị bỏ khỏi licence check và khoá này ra đời; `include-build` là PR#800, 0.19.0.
- **shellcheck json1**: trang man ([manpages.ubuntu.com … shellcheck.1](https://manpages.ubuntu.com/manpages/jammy/man1/shellcheck.1.html))
  và `ShellCheck.Formatter.JSON1` trên Hackage
  ([hackage.haskell.org/package/ShellCheck](https://hackage.haskell.org/package/ShellCheck/docs/ShellCheck-Formatter-JSON1.html))
  mô tả json1 là danh sách `comments` với `file, line, endLine, column, endColumn, level, code,
  message, fix`. Issue [#2570](https://github.com/koalaman/shellcheck/issues/2570) xin *thêm* đoạn
  mã nguồn vào json1 — bằng chứng gián tiếp rằng hôm nay nó không mang cấu trúc nguồn. Khớp với
  phép đo mục 1. Không tìm thấy format nào của shellcheck xuất parse tree.
- **Prior art cho item 64** đã có sẵn trong `docs/reference/prior-art.md` (item 64 trích): QuickFIX
  C++, QuickFIX/J, quickfix-go đều đọc đồng hồ *bên trong* phép kiểm; nanofix không kiểm. Không
  tìm thêm — câu hỏi ở đây không phải "họ làm gì" mà là "nghĩa vụ mà D1 dời ra ngoài thì ai giữ".
- **Cách khác để đọc `bash` cho item 69** (chỉ để ghi phương án loại): `shfmt --to-json` (Go,
  một binary nữa, đã bị loại ở plan trước §E); `bashlex` (pip); `tree-sitter-bash` (pip + grammar).
  Không cái nào có sẵn trên runner, không cái nào được đo ở đây, và §B nói vì sao không cần đo.

## Cách làm

### A. Item 57 — bật `include-dev`, và gate chuyển từ *ồn* sang *che*

**Quyết định: một dòng cấu hình, `[licenses] include-dev = true` trong `deny.toml`.** Đo tại chỗ
mục 2 chứng minh: `cargo deny check licenses` từ chối `MPL-2.0` trong `[dev-dependencies]` ngay
lập tức. Không có gì để nâng cấp (0.20.2 là bản mới nhất), không cần workaround.

Phương án bị loại: *(i)* giữ nguyên và "nói giới hạn" — giới hạn đó là **sai**, đã đo; *(ii)*
nâng cấp cargo-deny — không có bản nào mới hơn; *(iii)* bỏ `scripts/check-every-crate-is-licensed.sh`
vì cargo-deny đã che — **không bỏ**: nó là tuyến độc lập thứ hai
(`docs/reference/a-guard-that-watched-a-gate-shared-its-blind-spot.md`), và giá của nó là một
bước CI vài giây.

Việc làm, trong một bước:

1. `deny.toml`: thêm dưới `[licenses]`
   ```toml
   # `[measured 2026-09-12]` off by default because dev-dependencies never ship;
   # on here because CLAUDE.md treats every commit as already public and a test
   # helper under a copyleft licence is still a file in this repository's tree.
   # Item 57's "could not be configured away" was the wrong table: the knob
   # tried was [graph] exclude-dev. This one is what `check licenses` reads.
   include-dev = true
   ```
   và **viết lại** khối comment cuối file (dòng 88-105) — nó nói lỗ hổng không thể cấu hình, nay
   sai. Giữ lại **một** câu về lịch sử ("hôm 2026-09-08 thử `[graph] exclude-dev` và kết luận
   nhầm") vì đó là cái bẫy, phần còn lại thay bằng điều đúng.
2. `.github/workflows/ci.yml`, job `deny`: bên `cargo tree` bỏ `-e normal,build` — hai đồ thị nay
   **cùng chứa dev-dependencies**, và câu hỏi của bước so đếm trở lại nguyên nghĩa: *cargo-deny đã
   phán xét mọi crate cargo resolve chưa?* Kỳ vọng `46` và `46`. Viết lại khối comment
   `:175-235` theo cùng nguyên tắc: một câu lịch sử, phần còn lại là sự thật hôm nay. Bước
   `Every crate cargo resolves is permissively licensed` giữ nguyên.
3. `scripts/check-every-crate-is-licensed.sh`, **chỉ header** (dòng 5-19): con số `36` và câu
   "the nine it does not judge are dev-dependencies" không còn đúng; lý do tồn tại của script đổi
   thành "tuyến thứ hai, độc lập" — vốn đã là đoạn 21-32.
4. `docs/reference/a-license-gate-that-cannot-see-dev-dependencies.md`: thêm mục
   `## The hole was a knob in a different table` `[measured 2026-09-12]` với bảng đếm và reversal
   ở mục 2 trên; sửa mục *The fix, which is not a fix* thành quá khứ; **sửa phần generalisation**:
   giữ nguyên hai đoạn cũ (vẫn đúng) và thêm đoạn thứ ba — *trước khi tuyên bố một công cụ mù, đọc
   tài liệu của **phép kiểm** chứ không chỉ của **đồ thị**; một khoá ở bảng khác là câu trả lời
   thường gặp hơn "không thể".* Marker `[to testing-skills]` **giữ**: bài học mạnh hơn, không yếu
   đi.

Không đụng `Cargo.toml` nào ngoài lúc reversal (và phải khôi phục cả `Cargo.lock`).

### B. Item 69 — giữ regex, nói giới hạn ở đúng chỗ người ta đọc; ADR-0061

**Quyết định: (a).** Không đổi một dòng logic nào của `scripts/check-scratch-fixtures.sh`; bốn
cách qua mặt được **ghi tên** trong header của script như *false green đã biết*, trong hàng §6 của
`DESIGN.md`, và trong `CLAUDE.md` §2 — và quyết định được ghi thành **ADR-0061** để lần sau không
ai mở lại câu hỏi từ đầu.

Vì sao không phải (b), và đây là lý do thật chứ không phải "tốn công":

- **(b) qua `shellcheck` không tới được** — đo tại chỗ mục 1: json1 là danh sách chẩn đoán, không
  phải cây cú pháp. Điều kiện của item 69 ("prove the decision is reachable") đã có câu trả lời.
- **(b) qua một parser khác** (`shfmt --to-json`, `bashlex`, `tree-sitter`) là **một binary hoặc
  một package mới trên cả runner lẫn bàn**, một bộ duyệt JSON ~150 dòng, và một rủi ro lệch phiên
  bản mới — tất cả cho một gate mà đối thủ của nó là **fixture viết nhầm**, không phải người cố
  ý. Và một parser thật vẫn **không thấy** `eval`, `source`, một `cd` qua hàm, một `$(…)` — đúng
  danh sách "does NOT see" dòng 65-88 đã có. Nó dời đường biên, không xoá đường biên; sau nó, mục
  tiếp theo sẽ là "reviewer qua mặt bằng `eval`" và câu trả lời sẽ vẫn là "không gate tĩnh nào
  trên shell thắng được".
- **Bốn cách qua mặt là cách viết của reviewer, không phải của repo.** Bước 2 đo lại bằng `grep`
  và trích: không script nào trong `scripts/` có `cp` trong heredoc, `cp` chép ra khỏi scratch,
  `read … < <(mktemp`, hay `=/tmp$`. Ngày một script thật viết như vậy, câu trả lời vẫn là câu ở
  dòng 87-88: mở rộng script trong cùng commit.

Điều này **khác** với kết luận §A của plan trước — ở đó regex bị thay bằng lexer vì đối thủ là
*cách viết hợp lệ của Rust* mà một dev bình thường vẫn dùng (`/* */` trong ngoặc, `# ! [` tách
token); ở đây đối thủ là *một dòng data trong heredoc trông như lệnh*, thứ không script nào của
repo viết. Cùng nguyên tắc "khớp nghĩa, không khớp cách viết", nhưng nghĩa của gate này là
*"fixture vô tình kế thừa máy"*, và regex khớp đúng nghĩa đó cho mọi script đang có.

Việc làm:

1. `scripts/check-scratch-fixtures.sh`, **chỉ header dòng 65-88**: thêm bốn bullet `[measured
   2026-09-12]` vào danh sách "does NOT see", mỗi bullet một dòng, nêu đúng cách viết; thêm một
   câu trỏ ADR-0061. **Không hunk nào dưới dòng 92** — bước 2 trích `git diff` và manager kiểm.
2. `docs/decisions/ADR-0061-the-scratch-fixture-gate-is-a-regex-and-says-so.md` — tiếng Anh, nội
   dung tôi viết sẵn dưới đây, developer chép nguyên văn (chỉ điền số đo `grep` ở bước 2):

   ```markdown
   # ADR-0061 — The scratch-fixture gate is a regex, and says so

   **Status:** Accepted · **Date:** 2026-09-12 · **Plan:** docs/plans/2026-09-12-an-obligation-nothing-checks.md §B

   ## Context

   `scripts/check-scratch-fixtures.sh` guards the class behind
   `docs/reference/a-scratch-fixture-inherits-the-machine.md`: a gate that builds a
   throwaway crate outside the tree, where `rust-toolchain.toml` does not reach. It is a
   line-by-line regex over `bash`. `[measured 2026-09-12]` a senior review got past it four
   ways, each a false green: a `cp` inside a heredoc body; a `cp` copying out of the scratch
   dir rather than into it; `read -r TMP < <(mktemp -d)`; a bare `TMP=/tmp`. STATUS.md item
   69 asked whether to keep the regex or parse `bash`.

   The plan that built the sibling gate (`check-no-crate-root-allow.sh`) replaced a regex
   with the Rust lexer, because its adversary was legal Rust that ordinary code writes.

   ## Decision

   The gate stays a regex. Its four known false greens are named in its own header, in
   `DESIGN.md` §6 and in `CLAUDE.md` §2, and no further regex is added for them.

   ## Why

   - `shellcheck -f json1` — the only parser already on CI — emits diagnostics, not a parse
     tree. `[measured 2026-09-12]` on a file containing all four bypasses it reports two
     unused variables and nothing else.
   - Every other parser (`shfmt --to-json`, `bashlex`, `tree-sitter-bash`) is a new binary
     or package on the runner and the desk, a JSON walker, and a version skew — for a gate
     whose adversary is an accidental fixture. `[measured 2026-09-12]` no script in
     `scripts/` writes any of the four shapes (`grep`: <N> hits).
   - A real parser still cannot see `eval`, `source`, a `cd` behind a function, or a
     `$(…)` — the list the header already carries. It moves the frontier; it does not
     remove it. A static gate over a shell does not beat an author who is trying.

   ## Consequences

   - Good: no new dependency; the header states what the gate reads and what it cannot,
     and a reader is not told a spelling list is a meaning.
   - Bad: the four false greens stay. A fixture written in one of those four ways passes.
     The day a real script in `scripts/` does, this ADR is the pointer: extend the script
     in the same commit (`CLAUDE.md` §4), or supersede this ADR with a parser.
   - Bad: item 69's question can be reopened by the fifth spelling. The answer is the same
     until the adversary changes from an accident to a person.

   ## Alternatives rejected

   - `shellcheck -f json1` as a parser — measured, no AST.
   - `shfmt --to-json` — rejected in docs/plans/2026-09-12-gates-that-match-a-meaning.md §E
     as one more binary; that reason still holds and json1's emptiness does not change it.
   - `bashlex` / `tree-sitter-bash` — same cost, one language further from the scripts.
   - `bash`'s own parser through `declare -f` — not measured; its output is text again,
     with heredoc bodies rendered in it, so the regex loop would restart one layer down.
   ```
3. `docs/DESIGN.md` §6 dòng 862: thay đoạn "**four remain open by decision, not by oversight** …
   for the architect" bằng bốn cách viết kèm `[measured 2026-09-12]` và trỏ ADR-0061.
4. `CLAUDE.md` §2, đoạn về `check-scratch-fixtures.sh`: câu "**Four remain open by decision, for
   the architect** (STATUS.md item 69): …" đổi thành "…by decision, **ADR-0061**: …" — nội dung
   bốn cách viết giữ, câu "the plan that found them concluded that loop cannot be won by adding
   another" giữ. Nói rõ trong báo cáo là đã đổi rule nào (yêu cầu ở đầu `CLAUDE.md`).

### C. Item 70 — vét cạn chuỗi ngắn cộng lân cận, ba kết cục, và `TimestampPrecision` phải viết đúng chính tả

**Quyết định: chiều ngược của probe 3 đổi từ *lấy mẫu ba* thành *tìm kiếm có biên*, với ba kết cục
được phân biệt bằng ba câu FAIL khác nhau.** Không có "nới rộng bộ ứng viên" chung chung.

Phương án bị loại: *(i)* đọc lại mã nguồn của parser bằng `arm_literals` như probe key làm — chỉ
đúng với `flag` và `ConnectionType` (match trên chuỗi), **sai với `TimestampPrecision`** (đi qua
số rồi `from_fractional_digits` ở crate `codec`), và nó so tài liệu với *một cách đọc thứ hai của
mã* chứ không phải với *hành vi* của parser — mục 3 ở trên cho thấy hành vi và mã đọc khác nhau
(`03`); *(ii)* thêm vài ứng viên nữa vào `["1","true","nope"]` — vẫn là lấy mẫu, và là đúng cái
loop "thêm một cách viết" plan trước đã kết luận không thắng được.

**Vũ trụ ứng viên** (một `fn candidates(listed: &[&str]) -> Vec<String>` trong `mod doc_table`):

- mọi chuỗi độ dài **1 và 2** trên bảng chữ `0-9 A-Z a-z + - . _` (66 ký tự → 66 + 4 356 =
  4 422 chuỗi). **Không có** khoảng trắng, chuỗi rỗng, `=`, `#`, `;`, `[`, `]`: bộ đọc INI
  `trim()` giá trị và những ký tự kia chỉ có nghĩa ở đầu dòng, nhưng loại chúng để vũ trụ chỉ chứa
  *giá trị*, không chứa *cú pháp* — đó là cách tránh spurious red mà item 70 sợ, bằng thiết kế chứ
  không bằng may;
- **lân cận** của mỗi literal `L` được liệt kê: `L` chữ thường, `L` chữ hoa, `L` đảo hoa/thường ký
  tự đầu, `0L`, `+L`, `L0`, `Lx`, `xL`, `LL`. Đây là chỗ false accept thật sự nằm (`03`, `+3`,
  `Acceptor`, `initiators`, `y`);
- trừ đi `listed`, khử trùng lặp.

**Ba kết cục cho mỗi ứng viên `c` không nằm trong `listed`**, mỗi kết cục một câu:

| Parser trả về | Nghĩa | Câu (viết sẵn, giữ nguyên trong code) |
|---|---|---|
| `Ok(_)` | tài liệu thiếu, hoặc parser lỏng | `docs/CONFIGURATION.md §1: {name} lists {listed:?} but the parser also accepts {accepted:?} — either the document is short or the parser is lax` |
| `Err` với `is_about_the_value` | đúng | — |
| `Err` khác | **lỗi của probe**, không phải của tài liệu | `probe 3 wrote {c:?} for {name} and the parser refused it for a reason that is not about the value ({problem:?}) — the candidate universe leaked a syntax character; fix the probe, not the document` |

Kết cục 1 **gom cả hàng** rồi mới `assert!` (in ra mọi giá trị bị nhận, không chỉ giá trị đầu).
Thêm zero-guard cùng kiểu với `check-indexing-debt.sh`: `probe 3 tried {n} candidates for {name},
below 4400 — the universe was not built`. `FLOOR` giữ nguyên. Chiều xuôi giữ nguyên.

**Việc này sẽ đỏ ngay lần chạy đầu — và đó là thứ tự bắt buộc** (§10: failing test first):

1. Viết probe mới, chạy `cargo test -p fixbolt-engine --lib doc_table` → **đỏ**, câu kết cục 1
   cho `TimestampPrecision` với `accepted` chứa `+3 03 +6 06 +9 09` (thứ tự tuỳ). Trích nguyên
   văn. Nếu **xanh** ở bước này: probe không chạy — dừng, báo.
2. Sửa parser, **chỉ nhánh `TimestampPrecision` trong `settle`** (`settings.rs:1271-1287`): giá
   trị phải là **chính tả chuẩn** — chỉ chữ số, không dấu, không số 0 dẫn đầu — nếu không thì
   `Problem::UnsupportedPrecision` (thông điệp của nó đã nói *"expected TimestampPrecision=3, 6
   or 9"*, đúng nghĩa). Một test có tên bên cạnh test `UnsupportedPrecision` đang có:
   `timestamp_precision_is_refused_unless_spelled_exactly` — `03`, `+3`, `009` → `UnsupportedPrecision`
   tại đúng dòng; `3` → ok. **Không đụng `number()`**: `HeartBtInt=+30` và `030` là chuyện khác,
   ghi ở *Ngoài phạm vi*.
3. Chạy lại → xanh. Trích `probe 3 — enumerated Values cells: N probed, M skipped` và **thời gian
   chạy** của module (`--nocapture`, đọc dòng `test result:`); ước lượng ~49 000 lần parse một
   file 5 dòng, trần chấp nhận **10 s** ở debug — quá thì **báo, không tự thu nhỏ vũ trụ**.
4. `docs/CONFIGURATION.md`, mục `### TimestampPrecision — a ceiling on the way out…`: một câu
   `[measured 2026-09-12]`: *the value is read as written — `3`, `6` or `9` and nothing else;
   `03` and `+3` are refused as `UnsupportedPrecision`, though Rust's integer parser would have
   read both as `3`.* Comment doc của probe 3 (`:2178-2200`) viết lại theo thiết kế mới.

### D. Item 64 — `DropReason::NeverTicked`: phiên bị phán xét trước tick đầu tiên tự nêu tên lỗi của **phía mình**

**Quyết định: nghĩa vụ được giữ bằng chính giá trị đã có — một `Refusal`/`DropReason` mới, không
trường, tên là `NeverTicked`.** Trong `judge`, **trước** phép kiểm schedule (`lib.rs:3000`):

```rust
// A judgement with no clock is a fault on this side, and it is named as
// such rather than as the counterparty's skew. Guarded by
// crates/session/tests/skew.rs::a_message_judged_before_the_first_tick_is_refused_as_never_ticked
// and the reversal recorded in docs/plans/2026-09-12-an-obligation-nothing-checks.md §D.
if self.now_ms == 0 {
    return Err(Refusal::NeverTicked);
}
```

Định nghĩa "chưa tick" là `now_ms == 0`: comment của trường (`:1239-1243`) đã nói một phiên tick
về trước 1970 là cấu hình sai, và runner conformance tick tới 2026 (mục 6), nên `0` là nhân
chứng đủ và **không thêm trường** vào struct đã canh cache-line.

Phương án bị loại, và đây là phần quan trọng hơn phương án chọn:

- **`debug_assert!(now_ms != 0)`** (gợi ý của item 64) — đo tại chỗ mục 4: là một `panic` mà
  `clippy::panic = "deny"` **không thấy**, tức đưa vào `session` đúng loại lỗi PR này đang đóng
  (lint khớp cách viết, không khớp nghĩa). Thêm nữa: ở release nó biến mất, và một engine tương
  lai đảo thứ tự tick/read sẽ **lại** đổ cho NTP của đối tác — đúng item 63 lần hai. Ở debug nó
  giết engine thread vì một lỗi chính sách. Bị loại ở cả ba mặt.
- **Kiểu trạng thái `Session<Unticked>`** — đổi mọi chữ ký public để nói một điều mà một phép so
  sánh nói được. Bị loại vì giá.
- **Thêm `bool ticked`** — thừa khi `0` đã là nhân chứng; tốn một byte ở struct hot.

**Nó bắt được gì:** item 63 lần chạy đầu — sự kiện sẽ đọc `Ended(NeverTicked)` thay vì
`Ended(SendingTimeOutOfRange)` với `last_skew_ms` hai nghìn năm; và `last_skew_ms` **không được
ghi** (early return đứng trước `:3021`), nên con số hai nghìn năm không thể xuất hiện nữa. Mọi
hàm test hay harness nào feed trước tick đều đọc được lý do thật ngay lần đầu.

Việc làm (một bước, một người viết, **opus** vì đụng `session` và `engine`):

1. `crates/session/src/lib.rs`: `Refusal::NeverTicked` (`:1052`, doc: *no tick has run; the
   session has no clock to judge `52=` or the schedule against*); `DropReason::NeverTicked`
   (`:1099`, doc: **a fault on this side** — the engine or the harness judged a message before
   its first tick; never the counterparty's clock. `[measured 2026-09-10]` item 63 reported this
   as `SendingTimeOutOfRange` with `last_skew_ms` about two thousand years); nhánh trong
   `From<Refusal> for DropReason` (`:1207`, bắt buộc vì không có `_`); khối `if` trên trong
   `judge`.
2. `crates/session/tests/skew.rs` (dùng `acceptor()` và `good_logon()` sẵn có):
   - `a_message_judged_before_the_first_tick_is_refused_as_never_ticked`: không tick,
     `received(&good_logon())` → `Link::Dropped`; `last_drop_reason() == Some(NeverTicked)`;
     `last_skew_ms() == None` — với thông điệp *"item 63's two-thousand-year skew must not be
     measurable on a session that has no clock"*.
   - `the_same_message_after_one_tick_logs_on`: `tick(FIXED_TIME_MILLIS)` rồi cùng bytes →
     `Link::Up`, `last_drop_reason() == None`. (Nếu `two_clocks_that_agree_measure_zero_and_not_nothing`
     đã nói đúng điều này thì trỏ nó thay vì viết lại — developer quyết và nói rõ.)
3. **Chạy toàn bộ `cargo test -p fixbolt-session` trước khi sửa bất kỳ test nào.** Mỗi test đỏ vì
   `NeverTicked` được **liệt kê tên** trong báo cáo trước khi đụng. Quy tắc đã quyết sẵn: một test
   phán xét trước tick là test đang chạy ở trạng thái không engine nào tới (GUIDE §5); sửa bằng
   **một dòng** `s.tick(FIXED_TIME_MILLIS, |_| {})` trước `received` đầu tiên, **không đụng
   assertion**, và tên test vào *Nhật ký giao hàng*. Một test mà assertion của nó *dựa vào* việc
   bị từ chối lúc năm 0 là một phát hiện — báo, không sửa.
4. `crates/engine/tests/events.rs`, `an_operator_learns_why_each_connection_ended`: thêm bên cạnh
   assertion `SendingTimeOutOfRange` (`:276-280`) một assertion
   `!reasons.contains(&DropReason::NeverTicked)` với thông điệp *"and neither was judged before
   its first tick — the item 63 shape: {reasons:?}"*. Đây là assertion mà reversal R64-2 làm đỏ.
5. `crates/engine/src/conn.rs`, `lib.rs`: chỉ sửa nếu compiler bắt (không có `match` trên
   `DropReason`, mục 6 — kỳ vọng không sửa).
6. Tài liệu, cùng commit: `docs/SESSION-BEHAVIOUR.md` hàng mới ngay dưới `SendingTimeOutOfRange`
   (`:34`) nêu test canh; §1 dòng 52 (kể item 63) thêm nửa câu *"and since 2026-09-12 reads
   `NeverTicked`"*; `docs/GUIDE.md:606-607` viết lại: *holds zero and refuses the first message as
   `NeverTicked`, naming this side rather than the counterparty's clock — the engine ticks before
   it reads for exactly this reason*; `docs/DESIGN.md` §4 D1 đoạn *As built* một câu: *a session
   judged before its first tick refuses with `NeverTicked` — the obligation D1 moves to the caller
   is named at the boundary rather than assumed*; `CHANGELOG.md` *Unreleased → Changed*: variant
   mới trên `#[non_exhaustive] DropReason`, không phải breaking; `docs/reference/` mục mới
   `a-debug-assertion-is-a-panic-the-lint-cannot-see.md` `[to testing-skills]`, ngắn: phép đo mục
   4, cái nó có nghĩa (*a lint that denies a spelling does not deny the meaning; `debug_assert!`
   is `panic!` with a different name*), và cách xử ở đây (một biến thể enum thay cho assertion).

## Bất biến bị đụng tới

| # | Bất biến | Bị đụng bởi | Giữ bằng cách |
|---|---|---|---|
| 1 | không heap alloc trên hot path | §D: một phép so sánh trong `judge` | không alloc; `scripts/bench.sh` chạy `benches/alloc.rs`, trích số `0` |
| 2 | session thuần: không clock, không socket, không alloc, không `format!` | §D | thời gian vẫn chỉ tới qua `tick`; biến thể mới **không trường**; không đọc clock ở đâu |
| 3 | 59 định nghĩa là gate của session | §D | `cargo test -p fixbolt-session --test score` và `cargo test -p fixbolt-engine --test wire` phải đọc **59 / 59** trước và sau |
| 7 | không `panic!`/`unwrap`/`expect` trong lib crate | §D — bị đụng bởi *phương án bị loại*, không phải phương án chọn | không `debug_assert!`; `cargo clippy --all-targets -- -D warnings` sạch; và `docs/reference/` mới ghi vì sao lint không đủ |
| 10 | không số hiệu năng thiếu bench/máy/§9 | §C ghi thời gian chạy test | ghi là *thời gian test debug trên bàn*, không phải hiệu năng; không vào tài liệu nào ngoài nhật ký |
| — | §A, §B, §C không đụng `codec`/`session`/`transport`; §C đụng `engine` ở đường parse cấu hình (không phải hot path) | | senior review cuối PR đọc §C |

Bất biến 4, 5, 6, 8, 9: không đụng.

## Chia việc

| Bước | Mục | Kết quả | Được đụng | Không được đụng | Gate đóng bước | Tier | Song song |
|---|---|---|---|---|---|---|---|
| 1 | §A item 57 | `include-dev = true`; CI so đếm 46/46; comment và reference nói đúng | `deny.toml`; `.github/workflows/ci.yml` (chỉ job `deny`); `scripts/check-every-crate-is-licensed.sh` (chỉ header dòng 1-32); `docs/reference/a-license-gate-that-cannot-see-dev-dependencies.md` | mọi `Cargo.toml`/`Cargo.lock` ngoài lúc reversal (khôi phục xong mới báo); `STATUS.md` | `cargo deny --all-features check` bốn dòng `ok`; pipeline so đếm chạy tại chỗ in `46` và `46`; `scripts/check-every-crate-is-licensed.sh` xanh; R57 | sonnet | có — với 3 và 4 |
| 2 | §B item 69 | header script, ADR-0061, `DESIGN.md` §6 hàng 862, `CLAUDE.md` §2 | `scripts/check-scratch-fixtures.sh` **dòng 65-88 only**; `docs/decisions/ADR-0061-…md` (mới); `docs/DESIGN.md` (chỉ hàng 862); `CLAUDE.md` (chỉ đoạn §2 nêu ở §B.4) | mọi dòng ≥ 92 của script; `STATUS.md` | `scripts/check-scratch-fixtures.sh` vẫn `ok — 20 scripts, 1 enter a scratch dir, 1 pins`; `shellcheck -S info scripts/check-scratch-fixtures.sh` im lặng; `git diff -- scripts/check-scratch-fixtures.sh` không hunk nào chạm dòng ≥ 92; R69; `grep` bốn cách viết trên `scripts/` = 0 (trích) | sonnet | **sau bước 4** (cả hai sửa `docs/DESIGN.md`) |
| 3 | §C item 70 | probe 3 tìm kiếm có biên, ba câu FAIL; `TimestampPrecision` chính tả chuẩn; CONFIGURATION.md | `crates/engine/src/settings.rs` (`mod doc_table` và nhánh `TimestampPrecision` trong `settle`, `:1271-1287`); test `UnsupportedPrecision` ở nơi đang có; `docs/CONFIGURATION.md` mục `TimestampPrecision` | `number()`; `crates/codec`; mọi ô bảng khác của CONFIGURATION.md; `STATUS.md` | `cargo test -p fixbolt-engine --lib doc_table` xanh với `N probed` ≥ FLOOR và mỗi hàng ≥ 4 400 ứng viên; `cargo test -p fixbolt-engine --lib doc_table --features tls` xanh; `cargo clippy --all-targets -- -D warnings`; R70-1..4, **theo thứ tự đỏ-trước** ở §C | sonnet | có — với 1 và 4 |
| 4 | §D item 64 | `NeverTicked` ở session; test session; assertion engine; GUIDE, SESSION-BEHAVIOUR, DESIGN D1, CHANGELOG, reference mới | `crates/session/src/lib.rs`; `crates/session/tests/skew.rs`; test session khác **chỉ** theo quy tắc §D.3; `crates/engine/tests/events.rs`; `crates/engine/src/conn.rs`, `lib.rs` chỉ nếu compiler bắt; `docs/SESSION-BEHAVIOUR.md`; `docs/GUIDE.md` §5; `docs/DESIGN.md` §4 D1; `CHANGELOG.md`; `docs/reference/a-debug-assertion-is-a-panic-the-lint-cannot-see.md` (mới) | `crates/conformance`; `benches/`; assertion của test có sẵn; `STATUS.md` | `cargo test -p fixbolt-session`; `cargo test -p fixbolt-session --test score` **59 / 59**; `cargo test -p fixbolt-engine --test wire` **59 / 59**; `cargo test -p fixbolt-engine --test events`; `scripts/bench.sh` với dòng `alloc` đọc `0`; `cargo clippy --all-targets -- -D warnings`; R64-1, R64-2 | **opus** | có — với 1 và 3; **trước** 2 |
| 5 | toàn PR | senior review theo §12 bước 2: plan + gate, không kèm lý luận của manager; đọc kỹ §C (parser) và §D (hot path) | tuỳ finding, trên cùng branch | thiết kế (về architect qua manager) | mọi gate của bước 1-4 chạy lại trên commit sau sửa | opus | sau 1-4 |
| 6 | đóng | `STATUS.md` item 57/64/69/70 đóng, *Not proven* rà, nhật ký giao hàng, CI run id | `STATUS.md`; mục *Nhật ký giao hàng* của plan này | — | `cargo test --all`; `cargo test --all --no-default-features`; CI xanh **theo id** cho commit đóng | manager | cuối |

Hai file có nhiều hơn một ứng viên viết đã kiểm: `crates/engine/src/settings.rs` chỉ bước 3;
`scripts/check-scratch-fixtures.sh` chỉ bước 2. `docs/DESIGN.md` bị bước 2 và 4 cùng sửa, nên 2
chạy sau 4. `STATUS.md` chỉ manager.

## Cách kiểm chứng

**Gate của cả plan**, trên commit đóng: `cargo fmt --check`; `cargo clippy --all-targets --
-D warnings`; `cargo test --all`; `cargo test --all --no-default-features`; `cargo deny
--all-features check`; `scripts/check-scratch-fixtures.sh`; `scripts/check-every-crate-is-licensed.sh`;
`scripts/bench.sh` (dòng `alloc`); và CI xanh theo run id. Mọi output trích nguyên văn, đọc nội
dung chứ không đọc exit code.

**Reversal — mỗi cái ghi sẵn: sửa gì, câu đỏ nào, rồi khôi phục thấy xanh.** Một reversal xanh
bất ngờ là phát hiện, không phải pass; và trước khi tin đỏ, kiểm là `git diff` thật sự có thay
đổi (hai reversal hôm 2026-09-12 đã là no-op).

| Mã | Sửa tạm | Lệnh | Câu đỏ phải thấy | Khôi phục |
|---|---|---|---|---|
| R57 | thêm `option-ext = "0.2"` vào bảng `[dev-dependencies]` **có sẵn** của `tools/jrnl/Cargo.toml` (không mở bảng thứ hai — trùng khoá, `cargo metadata` từ chối; đã đo) | `cargo deny --color never --all-features check licenses` | `error[rejected]: failed to satisfy license requirements` … `option-ext v0.2.0` … `licenses FAILED` | `git checkout tools/jrnl/Cargo.toml Cargo.lock`; chạy lại thấy `licenses ok` |
| R57b | cùng sửa tạm | `scripts/check-every-crate-is-licensed.sh` | `1 of 72 crates are not allowed:` / `option-ext 0.2.0: MPL-2.0` | như trên |
| R57c | cùng sửa tạm | pipeline so đếm của job `deny` (hai lệnh `cargo deny … list` và `cargo tree …` như trong `ci.yml` sau sửa) | **không** đỏ — hai bên cùng `47`; đây là kiểm soát: bước so đếm hỏi "deny đã nhìn hết chưa", và nay nó nhìn hết. Nếu lệch, bên `cargo tree` còn sót `-e normal,build` | như trên |
| R69 | tạo `scripts/zz-probe.sh` với `TMP="$(mktemp -d)"` rồi `cd "$TMP"` và không `cp` | `scripts/check-scratch-fixtures.sh` | `FAIL — scripts/zz-probe.sh:3 enters a scratch dir ($TMP, from mktemp at :2) and never copies rust-toolchain.toml into it` | xoá file; `ok — 20 scripts, 1 enter a scratch dir, 1 pins`. Đây là kiểm chứng rằng sửa header không làm hỏng guard, không phải guard mới |
| R70-1 | `docs/CONFIGURATION.md:85`: `` `3`, `6` or `9` `` → `` `3` or `6` `` | `cargo test -p fixbolt-engine --lib doc_table` | câu kết cục 1: `TimestampPrecision lists ["3", "6"] but the parser also accepts ["9"]` (đúng lỗ đã đo của item 70) | `git checkout docs/CONFIGURATION.md` |
| R70-2 | **là bước 1 của §C**: probe mới chạy trên parser cũ | như trên | câu kết cục 1 với `accepted` chứa `+3`, `03`, `+6`, `06`, `+9`, `09` | sửa `settle` (bước 2 của §C) → xanh |
| R70-3 | trong `candidates()`, tạm thêm ứng viên `"3\nConnectionType=initiator"` | như trên | câu kết cục 3: `…refused it for a reason that is not about the value (MissingKey)…` — chứng minh kết cục 3 tách được khỏi kết cục 1 và câu nói đúng hướng "lỗi của probe" | bỏ ứng viên |
| R70-4 | bảng chữ của vũ trụ thành rỗng | như trên | `probe 3 tried 0 candidates for …, below 4400 — the universe was not built` | khôi phục |
| R64-1 | xoá khối `if self.now_ms == 0` trong `judge` | `cargo test -p fixbolt-session --test skew` | `a_message_judged_before_the_first_tick_is_refused_as_never_ticked` đỏ: `assertion … left: Some(SendingTimeOutOfRange), right: Some(NeverTicked)` và/hoặc `last_skew_ms` là `Some(<số lớn>)` thay vì `None` | khôi phục |
| R64-2 | `crates/engine/src/conn.rs`, lời gọi `session.tick_at_with(now_ms, …)` trong `turn`: đối số đầu thành `0` | `cargo test -p fixbolt-engine --test events an_operator_learns_why_each_connection_ended` | assertion mới đỏ: `and neither was judged before its first tick — the item 63 shape: [NeverTicked, NeverTicked]` (các test wire khác cũng đỏ — chấp nhận, chỉ trích test này) | khôi phục |
| R64-3 | không phải reversal — **kiểm soát**: 59 định nghĩa không đổi | `cargo test -p fixbolt-session --test score`; `cargo test -p fixbolt-engine --test wire` | `59 / 59` cả hai, trước và sau §D | — |

Cho §C, developer trích thêm dòng `probe 3 — enumerated Values cells: N probed, M skipped` và
dòng `test result:` với thời gian.

## Tài liệu phải cập nhật

Theo bảng §4 của `CLAUDE.md`, đi từng hàng:

- [ ] **Public API của crate** (§D): `crates/session` rustdoc của `DropReason::NeverTicked`;
      `docs/DESIGN.md` §4 D1 *As built*; `CHANGELOG.md` *Unreleased → Changed*.
- [ ] **Ràng buộc người dùng phải giữ mà compiler không kiểm** (§D): `docs/GUIDE.md` §5 dòng
      606-607.
- [ ] **Hằng số / khoá cấu hình người dùng thấy** (§C): `docs/CONFIGURATION.md` mục
      `TimestampPrecision`.
- [ ] **Hành vi biên của session layer** (§D): `docs/SESSION-BEHAVIOUR.md` hàng `NeverTicked`,
      **nêu tên test canh**; §1 dòng 52.
- [ ] **Gate: mục tiêu hoặc cách đo** (§A, §B): `docs/DESIGN.md` §6 hàng 862 (scratch); job `deny`
      trong `ci.yml` và `deny.toml` (§A). Không có hàng §6 nào cho cargo-deny hôm nay — bước 1
      **không thêm** hàng mới; nếu senior review muốn, là finding.
- [ ] **Bẫy / giả định sai / bất ngờ đã đo** (ưu tiên cao nhất): sửa
      `docs/reference/a-license-gate-that-cannot-see-dev-dependencies.md` (§A, giữ
      `[to testing-skills]`); mới `docs/reference/a-debug-assertion-is-a-panic-the-lint-cannot-see.md`
      (§D, `[to testing-skills]`).
- [ ] **Chọn dependency / đổi kỹ thuật / đảo quyết định**: ADR-0061 (§B). §A không cần ADR — là
      sửa cấu hình và sửa một tuyên bố sai, không phải đảo quyết định có ADR.
- [ ] **`CLAUDE.md` §2**: đoạn về `check-scratch-fixtures.sh` (§B.4) — nói rõ đã đổi rule nào.
- [ ] **`STATUS.md`**: item 57, 64, 69, 70 đóng với ngày và bằng chứng; mục *Not proven* (dòng
      3609) — hôm nay **không bullet nào** nhắc bốn item này (đã `grep`), nên không có gì để gạch;
      ghi rõ đã rà.
- [ ] **`docs/CONFORMANCE.md`**: không đổi số nào; chỉ nếu 59/59 đổi thì mới đụng — và nếu đổi
      thì plan này sai.
- [ ] Không thêm/bớt crate; không đụng `README.md`, `PRD.md`, `best-practices-*.md`,
      `hft-playbook.md`.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Reversal R57 mở bảng `[dev-dependencies]` thứ hai → `cargo metadata` lỗi `duplicate key` và mọi lệnh deny đỏ vì **lý do khác** (đã gặp hôm nay khi đo) | R57 nói rõ dùng bảng có sẵn; câu đỏ phải là `error[rejected]`, không phải `failed to load manifest` |
| Bước so đếm đỏ vì ANSI (`CARGO_TERM_COLOR: always`) — đã trả giá 2026-09-08 | giữ `--color never` cả hai bên; R57c chạy tại chỗ với `CARGO_TERM_COLOR=always` đặt trước để thấy vẫn bằng nhau |
| Sửa header script mà lỡ tay sửa logic (script 335 dòng, header 91 dòng) | gate bước 2: không hunk nào ở dòng ≥ 92; R69 vẫn đỏ đúng câu |
| Probe 3 mới **xanh ngay lần đầu** — tức vũ trụ không được xây hoặc không chạy đến kết cục 1 | thứ tự đỏ-trước là bắt buộc (R70-2); zero-guard R70-4 |
| Spurious red: ứng viên bị từ chối vì *ngữ cảnh* (không phải giá trị) đọc như lỗi tài liệu | ba kết cục, câu riêng cho kết cục 3; R70-3 chứng minh câu đó xuất hiện đúng lúc |
| Sửa `number()` thay vì nhánh `TimestampPrecision` → đổi hành vi 7 khoá số khác không có trong plan | bước 3 cấm đụng `number()`; senior review đọc diff |
| Test session có sẵn đỏ vì `NeverTicked` và bị "sửa cho xanh" bằng cách đổi assertion | §D.3: liệt kê trước, chỉ thêm một dòng tick, tên vào nhật ký; test mà assertion dựa vào năm 0 là phát hiện |
| 59 định nghĩa đỏ vì runner conformance feed trước tick ở một đường nào đó | R64-3 là kiểm soát; nếu đỏ → **không** bỏ guard, báo manager (là phát hiện về runner) |
| Reversal R64-2 là no-op vì `sed` không khớp (`pub`, khoảng trắng) — hôm 2026-09-12 đã xảy ra | kiểm `git diff` có hunk trước khi chạy test |
| `DropReason` có `match` đủ ở nơi chưa thấy (engine event → text, tool `jrnl`) | mục 6 đã grep: không; nếu compiler bắt thì sửa và báo, không thêm nhánh `_` |
| Thời gian test `doc_table` phình (49 000 parse) làm `cargo test --all` chậm rõ | trần 10 s debug; quá thì báo, không thu nhỏ |
| Chép `rust-toolchain.toml` bị quên khi developer tự đo gì đó ngoài cây | không bước nào cần scratch crate; nếu cần, đó là việc của `check-scratch-fixtures.sh` |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| `include-dev = true` làm `check licenses` thấy một dev-dependency hôm nay **không** nằm trong allow (script Python đã nói 71 crate đều ok, nhưng cargo-deny đọc SPDX chặt hơn `split("OR")`) | thấp | đo tại chỗ mục 2: `licenses ok` với `include-dev` trên cây hiện tại. Nếu CI đỏ: đó là finding thật về một licence, xử theo `deny.toml` (thêm vào allow với lý do, hoặc bỏ dependency), không tắt khoá |
| `cargo-deny-action@v2` trên runner cài bản khác 0.20.2 và hành vi `include-dev` khác | thấp | bước thứ hai của job đã pin `--version 0.20.2`; `include-dev` tồn tại từ ~0.14; nếu đỏ, đọc phiên bản trong log trước |
| Vũ trụ 4 422 + lân cận vẫn là *tìm kiếm có biên*, không phải chứng minh: một literal 3+ ký tự parser nhận mà tài liệu không ghi (ví dụ một alias tương lai `acc`) vẫn qua | trung bình, **nói thật** | comment probe nói rõ biên; lân cận `Lx`/`xL` bắt tiền tố/hậu tố một ký tự; ngày `ConnectionType` có alias, người thêm phải ghi vào *Values* và chiều xuôi bắt buộc |
| `now_ms == 0` là nhân chứng, không phải cờ: một caller tick tới đúng mili-giây 0 của năm 0 bị coi là chưa tick | rất thấp | comment trường đã nói tick trước 1970 là cấu hình sai; ghi thêm một câu ở rustdoc `NeverTicked` |
| Số test session phải thêm tick lớn hơn dự kiến (11 là số thô) | trung bình | quy tắc §D.3 áp cho mọi số; nếu > 15 test, developer dừng và báo trước khi sửa, manager quyết có tách bước |
| Runner conformance có đường feed-trước-tick chưa thấy | thấp | R64-3; là phát hiện, không phải lý do bỏ guard |
| Thời gian: bốn mục, ba song song, một senior review | — | bước 4 dài nhất, chạy trước; bước 2 ngắn, chạy sau 4 |

## Ngoài phạm vi

- **Item 40, 49, 51, 52** — cần máy §9 và reboot; reboot giết phiên đang làm. Plan khác.
- **`tls` plan bước 5/6/7** — không đụng.
- **17 script chưa qua `shellcheck`** ngoài ba file `ci.yml:77` đã gọi — §B không thêm file nào
  vào dòng đó; plan trước §E đã nói vì sao (hai script cần máy §9 để chứng minh sửa quoting không
  đổi hành vi).
- **`number()` nhận `+30` và `030`** (đo tại chỗ mục 3, áp cho `HeartBtInt`, `SocketConnectPort`,
  `MaxSkewMillis`, …): là hành vi người dùng thấy, đổi nó cần hàng CONFIGURATION cho 7 khoá và
  đối chiếu QuickFIX C++ (`IntConvertor` từ chối `+`, nhận số 0 dẫn đầu). **Manager mở open item
  mới** trong `STATUS.md` ở bước 6, trích phép đo — không sửa ở đây.
- **Đọc `bash` bằng parser thật** — ADR-0061 đóng câu hỏi cho tới khi đối thủ đổi từ *vô tình*
  sang *cố ý*.
- **Kiểu trạng thái cho `Session`** — bị loại ở §D, không phải hoãn.
- **Chiều ngược cho ô *Values* dạng văn xuôi** (`StartDay`, `Weekdays`, cổng `0`–`65535`) — vẫn
  là *skip, counted*; đếm `skipped` vẫn in ra để thấy.

## Nhật ký giao hàng

**Branch `plan/an-obligation-nothing-checks`, PR [#65](https://github.com/tmthang86/fixbolt/pull/65).**
Sáu bước, chạy liên tục theo `CLAUDE.md` §12: ba bước song song trong ba `git worktree` riêng — vì
reversal của bước 1 gắn một crate `MPL-2.0` vào `tools/jrnl` và reversal của bước 4 đặt đồng hồ
engine về `0`, chung một cây thì gate của bước 3 sẽ đỏ vì việc của người khác.

| Bước | Commit | Kết quả, và cái bất ngờ |
|---|---|---|
| Plan | `a6029e9` | Architect (Fable 5.1) quyết cả bốn mục. **Ba trên bốn quyết định lật ngược điều repo đang viết.** |
| 1 — item 57 | `d7e8608` | `include-dev = true`. `cargo deny` bốn dòng `ok`; so đếm **46 và 46** (trước là 11 và 11 trên đồ thị cụt hai đầu), giữ nguyên khi ép `CARGO_TERM_COLOR=always`. R57 / R57b / R57c đúng dự đoán. |
| 3 — item 70 | `95ab19a` | Probe 3 thành tìm kiếm có biên. **Đỏ trước, và cái đỏ là lỗi thật**: parser nhận `+3`, `03`, `+6`, `06`, `+9`, `09`. 49 000 lượt parse hết **0.32 s** / trần 10 s. `10 probed, 20 skipped` (11 / 19 với `tls`). |
| 4 — item 64 | `a85c5b3` | `DropReason::NeverTicked`. Session **147 passed / 0 failed, không sửa một test cũ nào** — sau guard này một session không thể tới `LoggedOn` mà chưa tick. `score` và `wire` đều `report.passed == 59`. `alloc` đọc 0 ở mọi case. **Danh sách test phải thêm tick ở §D.3: rỗng.** |
| 2 — item 69 | `f085f43` | ADR-0061, không đổi một dòng logic; một hunk header `@@ -84,6 +84,18 @@`, không chạm dòng ≥ 92. Bốn `grep` cho bốn cách viết đều **0**. |
| — | (tip trước 5) | `check-links.py` đỏ vì trích dẫn `CHANGELOG.md` của cargo-deny. **Gate sai, không phải plan sai** — nhưng không vá gate ở đây (§1); đổi chỗ trích dẫn, mở item 72. |
| 5 — review | `845974c` | Senior review, context mới: **11/11 reversal đỏ đúng câu**, mọi gate xanh, **5 finding phải sửa** — bốn trong năm là *câu chữ*. |
| 6 — đóng | — | `STATUS.md`, nhật ký này, CI run id. |

**Cái đắt nhất của plan này, và nó lặp lại đúng bài học của PR trước:** câu
*"the two-thousand-year number can no longer be produced at all"* trong `docs/SESSION-BEHAVIOUR.md`
**sai ngay trong ngày nó được viết**. Guard chỉ đóng `now_ms == 0`; người nhúng tick bằng mili-giây
Unix — đúng cái nhầm mà D13 và `GUIDE.md` §5 tồn tại để cảnh báo — vẫn đọc skew ≈ −1 970 năm kèm
nguyên lời buộc tội `SendingTimeOutOfRange`. `CHANGELOG.md` viết *"on that path"* nên đã đúng từ đầu.
Bảng bốn hàng nay nằm trong file, ghi rõ **chỉ hàng đầu có test canh**, ba hàng còn lại là phép đo.

**Ba phát hiện khác đáng giữ:**

1. **Một assertion không reversal nào làm đỏ được là đồ trang trí.** Assertion mới trong `events.rs`
   ban đầu đứng sau ba assertion cũ, mà test đó chỉ mở hai connection — nên nó không bao giờ tới
   lượt. Sửa bằng **thứ tự**, không phải bằng connection thứ ba: câu hỏi về *phía mình* phải hỏi
   trước, vì nếu nó đúng thì mọi câu hỏi về phía đối tác bên dưới đều vô nghĩa.
2. **`clippy::panic = "deny"` không thấy `debug_assert!`** — gợi ý của chính item 64 là một lỗi cùng
   họ với plan này. Đo có control (`panic!("control")` cạnh nó → exit 101; bỏ control → exit 0).
3. **Tiền đề của ADR-0061 yếu hơn cách nó đọc**: `scripts/check-bench-alignment.sh:102` đã viết
   `read -r … < <(…)` từ trước — idiom của cách qua mặt thứ ba, do chính tác giả repo viết vì lý do
   khác. Ghi vào *Consequences*, không sửa *Why* (§5: không sửa nội dung ADR đã Accepted).

**Không làm, nói thẳng:** item 40, 49, 51, 52 cần máy §9 và một lần reboot — `/proc/cmdline` hôm nay
không có `isolcpus`, `nohz_full` hay `rcu_nocbs`, và reboot giết chính phiên đang làm. Bốn item mới
mở: **71** (con số "eighteen places" mục trong hai ADR đã Accepted), **72** (`check-links.py` đọc
tên file trần là link về nhà), **73** (`number()` nhận `+30`/`030` trên bảy khoá), **74** (probe 3
vẫn mù với literal ≥ 3 ký tự).
