# Ba gate khớp *cách viết* thay vì khớp *nghĩa* — đóng item 66, 67, 68

> **Loại:** Plan · **Ngày:** 2026-09-12 · **Trạng thái:** **Đã duyệt 2026-09-12**, đang làm — nhánh `plan/gates-that-match-a-meaning`
> **Phạm vi:** `STATUS.md` item **66** (hai script canh gác bị qua mặt bằng cách viết khác),
> **67** (gate doc↔key chỉ kiểm *có hàng*, không kiểm *hàng đúng*) và **68** (danh sách file test
> không thấy được test bị `cfg` che trong file không ai liệt kê). Cả ba do senior review của PR
> [#63](https://github.com/tmthang86/fixbolt/pull/63) tìm ra bằng cách **tấn công gate bằng những
> cách viết tác giả chưa thử**. Chạm `scripts/`, `.github/workflows/ci.yml`, một tool crate mới
> `tools/attr-scan`, `crates/engine/src/settings.rs` (chỉ module test `doc_table`), docs.
> **Không chạm** `codec`, `session`, `transport.rs`, `lib.rs` của `engine`.
>
> Plan này **nối tiếp** [plan 2026-09-12 trước](2026-09-12-crate-root-allow-scratch-fixture-and-tls-4c.md):
> hai script ở bước 1–2 và gate ở bước 4 của plan đó là thứ bị tấn công. Plan đó đã đóng; sửa gate
> dưới một plan đã đóng là điều `STATUS.md` item 66 cấm, nên mới có plan này.
>
> **Không bước nào cần máy §9, không cần reboot, không cần máy rảnh.** Bước nào không thoả điều đó
> thì không nằm ở đây.

## Bối cảnh

Ba gate, cùng một lỗi: chúng **so chuỗi ký tự**, trong khi thứ cần canh là **ý nghĩa** mà trình
biên dịch, `bash`, hoặc parser cấu hình thật sự hiểu.

1. `scripts/check-no-crate-root-allow.sh` tìm `#![allow(` bằng regex. `#![/*x*/allow(clippy::unwrap_used, …)]`
   là cùng một attribute đối với `rustc`, nhưng regex không thấy, `cargo fmt --check` cũng để yên,
   và một `x.unwrap()` thật trong `crates/session` sau đó **biên dịch sạch** — bất biến 7 tắt trong
   một dòng. `scripts/check-scratch-fixtures.sh` tìm `cp` bằng regex, nên `# TODO: cp rust-toolchain.toml "$TMP"`
   — một dòng **đã bị comment** — được tính là bằng chứng có copy; `cd "$(mktemp -d)"` không gán biến
   nên không bao giờ được gieo; `readonly TMP=…` không nằm trong bộ tiền tố `local|declare|export`.
2. `doc_table` trong `settings.rs` so **ô đầu tiên** mỗi hàng của `docs/CONFIGURATION.md` §1 với
   enum `Key`. Đổi ô *Values* của `SocketUseSSL` thành `` `1` or `0` ``, đổi *Default* thành `Y`, viết
   `TlsRequireKernel` là *tắt* TLS, đổi câu đếm thành "Four hundred keys" — suite vẫn `3 passed`.
   Lỗi thật đã xảy ra trong chính việc dựng gate đó: ô ghi chú của `TlsRequireKernel` ghi *"does
   nothing without `SocketUseSSL=Y`"* trong khi `settle` **từ chối**; chỉ kiểm tay mới thấy.
3. Job `tls` của CI liệt kê binary test bằng tên file. `crates/engine/tests/settings.rs` có ba test
   `#[cfg(feature = "tls")]`; `cargo test --all` biên dịch bỏ chúng, job `tls` không có lý do gì
   để gọi tên file `settings` — nên ba test đó chạy trên một bàn và không CI nào. PR #63 thêm
   `--test settings` (14 → 55), nhưng **lớp lỗi** vẫn mở: không gì liệt kê test bị `cfg` che rồi so
   với danh sách của job. Đây là item 62 lần thứ ba trong một ngày.

Kết quả muốn có: mỗi gate đọc **thứ mà công cụ thật đọc** — lexer của Rust cho attribute, vị trí
lệnh trong dòng `bash` sống cho `cp`, chính binary test cho danh sách test, chính parser cho ô giá
trị trong tài liệu — và mỗi cái được **đảo chiều đúng bằng cách viết đã qua mặt nó**.

## Những gì đã biết chắc

### Đo tại chỗ hôm nay `[measured 2026-09-12]`, máy này, cây sạch ở `78b25e3`

- **`proc-macro2` 1.0.107** (đã có sẵn trong `Cargo.lock`, kéo theo bởi `thiserror`/`syn`) tokenize
  một file nguồn **ngoài** proc-macro bằng `src.parse::<TokenStream>()`. Thử với crate rác trong
  scratchpad (có copy `rust-toolchain.toml`, đúng bẫy item 23), duyệt token **cấp cao nhất** tìm bộ
  ba `#` `!` `Group(Bracket)`:

  ```
  inner attr at line 1: allow (clippy :: unwrap_used)        <- từ  #![/*x*/allow(clippy::unwrap_used)]
  inner attr at line 2: expect (clippy :: panic)             <- từ  # ! [ expect ( clippy::panic ) ]
  inner attr at line 3: cfg_attr (test , allow (dead_code))  <- từ  /* c */ #![cfg_attr(test, allow(dead_code))]
  inner attr at line 4: warn (clippy :: indexing_slicing)    <- từ  #!\n[warn(clippy::indexing_slicing)]
  inner attr at line 6: doc = "/* not a comment #![allow(x)] */"
  inner attr at line 8: deny (unsafe_code)
  ```

  Cả **bốn** cách viết đã qua mặt regex đều ra đúng ident đầu và đúng số dòng của dấu `#`.
  Chuỗi `"#![allow(x)]"` bên trong `#![doc = …]` và bên trong một `fn` là **literal**, không phải
  attribute. `/* /* nested */ #![allow(y)] */` biến mất (comment lồng). `mod m { #![allow(…)] }`
  nằm trong một Group ngoặc nhọn nên **không** ở cấp cao nhất — đúng phạm vi: đó là item 55, không
  phải script này. Feature `span-locations` cho `span().start().line`.
- **libtest `--list --format terse`** in mỗi test một dòng `tên: test`; `cargo` in dòng
  `Running tests/settings.rs (target/debug/deps/settings-<hash>)` ra stderr trước mỗi binary.
  `cargo test -p fixbolt-engine --test settings -- --list --format terse` đếm **39** không có
  `tls` và **41** có `--features tls` — khớp 3 test `#[cfg(feature = "tls")]` trừ 1 test
  `#[cfg(not(feature = "tls"))]` (`tests/settings.rs:1061`).
- **CI run `34688952076`** (commit đóng PR #63): job `tls` **40 s**, job `fmt · clippy · test`
  **252 s**, job `bench` **431 s** (đường găng). Chạy cả bộ test `engine` dưới `--features tls` tốn
  thêm nhiều nhất cỡ phần `engine` của 252 s, và không đổi đường găng.
- **`Settings::parse` kiểm `[DEFAULT]`-only trước mọi kiểm giá trị**: `settings.rs:912`, `:922`,
  `:948` trả `Problem::DefaultOnly` ngay khi thấy key đó trong `[SESSION]`, bất kể giá trị. Nên một
  probe *"key này có phải `[DEFAULT]`-only không"* **không cần ngữ cảnh** (không cần cert, không
  cần host/port).
- `Config` derive `PartialEq, Eq` (`crates/session/src/lib.rs:412`); `Problem` derive `PartialEq`
  (`settings.rs:263`), `SettingsError::problem()` (`:416`); `TlsSettings` derive `PartialEq`
  (`:735`); `Policy` (reconnect) chỉ có `Debug`. `Settings` không có `PartialEq` nhưng module test
  nằm **trong** `settings.rs` nên đọc được field riêng. → Item 67 làm được **không đụng `session`**.
- `flag()` (`settings.rs:799`) nhận đúng `Y`/`N`, còn lại `Problem::NotAFlag`; `number()` →
  `NotANumber`; `ConnectionType` → `BadConnectionType` (`:933`); precision → `UnsupportedPrecision`
  (`:1279`). Tập "lỗi về *giá trị*" là hữu hạn và có tên.
- `CONFIGURATION.md` §1 có **bốn bảng**, hai kiểu cột: `| Key | Meaning | Values | Default | Where | Source |`
  và `| Key | Meaning | Values | Default |` (bảng *session's own behaviour* không có *Where*). Câu
  đếm ở dòng 21: `**Thirty keys** are recognised`.
- `shellcheck` **không** có trên bàn này (`STATUS.md:3812`); bốn script cũ mang chỉ thị
  `# shellcheck` mà chưa từng chạy qua nó.

### Tra cứu trên internet `[researched 2026-09-12]`

- **Lint `clippy::allow_attributes` không nhìn thấy inner attribute.** Mã nguồn
  [`clippy_lints/src/attrs/allow_attributes.rs`](https://raw.githubusercontent.com/rust-lang/rust-clippy/master/clippy_lints/src/attrs/allow_attributes.rs)
  có điều kiện `if let AttrStyle::Outer = attr.style` — chỉ `#[allow]`, không `#![allow]`. Nên hướng
  "dùng chính clippy với `--force-warn clippy::allow_attributes` rồi lọc JSON" **đóng lại** — đây
  là điều đáng ghi để sau này không ai thử lại.
- **`--force-warn` thắng mọi attribute trong nguồn**, kể cả `forbid`:
  [rustc book, *Lint levels*](https://doc.rust-lang.org/rustc/lints/levels.html) — *"`--force-warn`
  forces a lint to warning level, and takes precedence over attributes and all other CLI flags"*.
  Đây chính là lý do `check-indexing-debt.sh` đọc đúng số trong khi `deny` đã tắt (item 58); nó
  không giúp *phát hiện* một `#![allow]`, vì `--force-warn` chỉ ép lint hiện có, không báo có
  attribute nào đang che.
- **`-Zunpretty=expanded` / `cargo expand` chỉ có trên nightly**
  ([rust-lang/rust#43364](https://github.com/rust-lang/rust/issues/43364),
  [users.rust-lang.org](https://users.rust-lang.org/t/cargo-rustc-zunpretty-expanded/100217)) —
  repo này ghim toolchain stable, loại.
- **`syn::parse_file`** ([docs.rs](https://docs.rs/syn/latest/syn/fn.parse_file.html)) parse cả
  file và trả `File { attrs, items, … }`, cần feature `full` + `parsing`. Đủ, nhưng nặng hơn cần
  thiết: thứ cần là **lexer**, không phải parser; `proc-macro2` (cùng lexer `syn` dùng) đủ và đã
  có sẵn trong lock. [docs.rs `proc_macro2`](https://docs.rs/proc-macro2/latest/proc_macro2/):
  *"`proc_macro2` types may exist anywhere including non-macro code"*; `LineColumn` sau feature
  `span-locations`.
- **Liệt kê test**: libtest `--list` — *"Prints a list of all tests and benchmarks. Does not run any
  of the tests"* ([rustc book, *Tests*](https://doc.rust-lang.org/rustc/tests/index.html));
  `cargo-nextest` có `cargo nextest list --message-format json` theo từng binary
  ([nexte.st](https://nexte.st/docs/listing/)) — làm được cùng việc nhưng thêm một công cụ phải cài
  trên runner; libtest có sẵn nên chọn libtest. Quy tắc chung của cộng đồng: *"If a feature is
  disabled, the guarded code never reaches the test runner, and the test runner doesn't know the
  test ever existed"* ([Rust Project Primer, *Crate Features*](https://rustprojectprimer.com/checks/features.html))
  — tức là **liệt kê phải chạy dưới đúng feature**, không có đường tắt bằng grep.
- **Tài liệu làm fixture**: không tìm thấy dự án Rust nào parse một bảng Markdown rồi đưa ô vào
  parser thật. Cái gần nhất: `rstest` (bảng case viết trong code,
  [docs.rs](https://docs.rs/rstest)), `datatest-stable` (fixture là file, không phải bảng,
  [docs.rs](https://docs.rs/datatest-stable)), `trybuild` (fixture là file `.rs`). **Không có tiền
  lệ để chép** — nên phần D dưới đây là thiết kế riêng và được giữ hẹp có chủ đích.
- **`shellcheck` 0.9.0 và `jq` 1.7.1 có sẵn trên runner `ubuntu-24.04`** (`ubuntu-latest`):
  [actions/runner-images, Ubuntu2404-Readme](https://github.com/actions/runner-images/blob/main/images/ubuntu/Ubuntu2404-Readme.md).
  Không tìm thấy lint nào của `shellcheck` bắt "lệnh bị comment" — đó không phải việc của nó.
- Không tìm thấy công cụ nào chuyên "cấm inner attribute ở gốc crate" (`cargo-deny` lo dependency
  và license; `clippy.toml` không có khoá nào cho việc này; RFC 3389 `[lints]` không nói gì về
  attribute trong nguồn — [rust-lang/rfcs 3389](https://rust-lang.github.io/rfcs/3389-manifest-lint.html)).

### Về code hiện có

- `scripts/check-no-crate-root-allow.sh` (190 dòng): A0 (0 gốc crate = lỗi), A1 regex, A2 regex +
  danh sách `deny` rút từ `Cargo.toml`, A3 `[lints] workspace = true`. Phạm vi từ `cargo metadata`.
  Header dòng 102–115 đã ghi bốn cách qua mặt.
- `scripts/check-scratch-fixtures.sh` (237 dòng): B0–B4, `refers_to`, gieo B1a/B1b, B2 tìm `cp`
  bằng `grep -E '(^|[[:space:]])cp[[:space:]]' | grep -F pin`. Header dòng 116–127 đã ghi ba cách
  qua mặt. Hiện đọc `19 scripts, 1 enter a scratch dir, 1 pins`.
- `ci.yml` job `tls` (dòng 435–520): gọi `--test tls --test tls_wire --test tls_mode --test tls_settings_wire --test settings`,
  đếm `TLS tests that ran: N` bằng `grep -c`, rồi lặp `--test tls` ba lần.
- `settings.rs:1519` trở đi, `mod doc_table`: `arm_literals`, `doc_rows` (chỉ ô đầu), ba test,
  sàn `FLOOR = 30`.

## Cách làm

### A. Item 66a — `check-no-crate-root-allow.sh` đọc bằng lexer của Rust, không bằng regex

Một tool crate mới, **`tools/attr-scan`** (binary `attr-scan`, một dependency: `proc-macro2` với
feature `span-locations`, `[lints] workspace = true`). Nhận đường dẫn file, với mỗi file:

1. Đọc, `parse::<TokenStream>()`. **Không lex được → in `PATH:0\tLEX_ERROR\t<lỗi>` và exit 2** —
   một file không đọc nổi là *từ chối*, không phải *pass*.
2. Duyệt token **cấp cao nhất** (không đi vào Group nào ngoài chính bracket của attribute): mỗi bộ
   ba `#` `!` `Group(Bracket)` là một inner attribute ở gốc crate. In một dòng
   `PATH:LINE\tHEAD\tIDENTS`, với `LINE` là dòng của dấu `#`, `HEAD` là ident đầu trong ngoặc
   (`allow`, `expect`, `warn`, `deny`, `forbid`, `doc`, `cfg_attr`, `no_std`, …), `IDENTS` là mọi
   ident trong ngoặc, **đệ quy qua mọi Group con** (để `cfg_attr(a, cfg_attr(b, allow(x)))` lộ ra
   `allow`).
3. Cuối cùng in ra stderr `attr-scan: N files, M inner attributes`.

Script giữ nguyên A0, A3 và cách lấy phạm vi. **A1 và A2 viết lại** trên output của tool:

- **A1** — `HEAD ∈ {allow, expect}` → FAIL `crate-root allow at PATH:LINE: HEAD(…)`; `HEAD = cfg_attr`
  và `IDENTS ∋ allow | expect` → cùng câu.
- **A2** — `HEAD = warn` (hoặc `cfg_attr` có `warn`) và `IDENTS ∩ DENY_LINTS ≠ ∅` → FAIL
  `crate-root warn lowers a workspace deny at PATH:LINE`.
- **A0b, mới** — tổng `M` trên mọi gốc crate bằng **0** → FAIL *"attr-scan read nothing"*. Hôm nay
  mọi `lib.rs` có `//!` (lexer biến thành `#![doc = …]`), nên 0 là tool hỏng, không phải cây sạch;
  con số thật ghi vào nhật ký giao hàng ở bước 1.
- Tool được gọi bằng `cargo run -q -p attr-scan -- FILE…` **trong cây** — `rustup` đi lên tìm
  `rust-toolchain.toml` bình thường, không có scratch dir, `check-scratch-fixtures.sh` không có gì
  để nói.

Header script: xoá bốn dòng "qua mặt" (102–115), thay bằng điều nó *thấy* (mọi inner attribute mà
lexer của Rust thấy ở gốc crate — comment, khoảng trắng, xuống dòng không có nghĩa; chuỗi là chuỗi)
và *không thấy* (danh sách cũ dòng 40–51 giữ nguyên; thêm: attribute bên trong `mod x { … }` là
item 55).

**Vì sao không phải là** một pha "bỏ comment rồi khớp regex": `#![doc = "/*"]` rồi `#![allow(…)]`
rồi `#![doc = "*/"]` — bộ bỏ-comment thấy comment, `rustc` thấy chuỗi; đó là cách viết thứ năm mà
reviewer sẽ thử tiếp. Regex sửa mãi vẫn là regex. Lexer thật thì hết chuyện.

### B. Item 66b — `check-scratch-fixtures.sh`: dòng sống, mọi tiền tố gán, `cd` không tên

Ba sửa, mỗi cái đúng một cách qua mặt, cộng một quy ước mới về "dòng sống":

- **Dòng sống.** Một dòng mà ký tự đầu (sau khoảng trắng) là `#` **không được đọc** ở mọi bước
  (B1a, B1b, B2, tìm `cp`). Trong `bash`, ngoài heredoc và chuỗi nhiều dòng, đó luôn là comment;
  một `cp` trong heredoc là dữ liệu và bị loại cũng đúng.
- **`cp` phải ở vị trí lệnh.** Bằng chứng B2 chỉ nhận `cp` đứng **đầu dòng** hoặc ngay sau
  `;` `&&` `||` `|` `(` `{` `then` `do` `else` — regex
  `(^|[;&|({]|[[:space:]](then|do|else))[[:space:]]*cp[[:space:]]`. `echo x # cp …` không khớp
  (`cp` đứng sau `#`, không phải sau dấu phân cách lệnh); `echo cp …` không khớp (`cp` sau một từ). `sudo cp`, `command cp`,
  `\cp` cũng không khớp — **hướng sai là đỏ**, người viết đổi sang `cp` thường; ghi trong header.
- **Mọi tiền tố gán.** Regex gieo B1a/B1b nhận *một* từ đứng trước kèm cờ tuỳ chọn:
  `^[[:space:]]*([A-Za-z]+([[:space:]]+-[A-Za-z]+)*[[:space:]]+)?([A-Za-z_][A-Za-z0-9_]*)=(.*)$`
  — bắt `readonly TMP=`, `declare -r TMP=`, `typeset -g TMP=`, `export TMP=`, `local TMP=`.
  Nó cũng bắt `echo TMP=…` — gieo thừa chỉ làm kiểm *nhiều hơn*, hướng an toàn.
- **B2b, mới — `cd` vào chỗ không tên.** Một dòng vào scratch (`cd`/`pushd`/`--manifest-path`/`-C`)
  mà **chính dòng đó** chứa `mktemp`, `TMPDIR` hoặc `/tmp/` và **không** nhắc biến scratch nào →
  FAIL `enters a scratch dir it never named at F:N — assign it to a variable so the copy can be checked`.
  Không có cách nào kiểm một bản copy vào một chỗ không có tên; luật là *đặt tên*.

Header: xoá ba dòng "qua mặt" (116–127), ghi ba luật trên vào chỗ B1/B2, và nói rõ hai giới hạn còn
lại của cách đọc theo dòng: **thứ tự** (`cp` sau `cd` vẫn được tính — không kiểm thứ tự) và một
`cd` nằm sau `#` bên trong một chuỗi trên cùng dòng (rất hiếm, và hướng sai là *đỏ thừa*, không
phải xanh giả — vì dòng vào scratch vẫn được quét nguyên dòng).

**Vì sao vẫn là `bash` và regex ở đây**, khác với A: không có lexer `bash` nào có sẵn trên runner
(`shfmt --to-json` cho AST nhưng là binary Go phải cài thêm; `shellcheck` không lộ AST). Ba luật
trên đều là *vị trí trong dòng*, thứ mà `bash` tự định nghĩa bằng cú pháp và regex diễn tả được;
điều A cần là *lexer* vì comment lồng và chuỗi, điều B không có.

### C. Item 68 — bỏ danh sách; build tự liệt kê, log phải khớp từng test

Script mới **`scripts/check-feature-gated-tests-ran.sh PACKAGE FEATURE LOG`**:

1. **Liệt kê**: `cargo test -p PACKAGE --tests --features FEATURE -- --list --format terse 2>&1`.
   Đọc dòng `Running <path> (…)` để biết binary hiện tại, dòng `<tên>: test` để ghi
   `<path>\t<tên>`. `--tests` = unit test của lib + mọi integration test; bench/example **không**
   nằm trong (nói ở header). **0 test → FAIL C0** *"listed nothing"* — item 62 một lớp nữa.
   Lệnh `cargo` lỗi → exit 2 in nguyên lỗi (không phải pass).
2. **Đối chiếu**: `LOG` là output của `cargo test -p PACKAGE --tests --features FEATURE --no-fail-fast`.
   Đọc cùng dòng `Running` để chia mục, và `^test <tên> \.\.\. (ok|FAILED|ignored)$`. **Mỗi dòng đã
   liệt kê phải có mặt trong đúng mục binary của nó** — `ok` hoặc `FAILED` là *đã chạy*; `ignored`
   được đếm riêng và in ra. Thiếu một → FAIL `<path>::<tên> is in the FEATURE build and did not run`.
3. In: `check-feature-gated-tests-ran: ok — PACKAGE --features FEATURE: B binaries, N listed, N accounted for (I ignored)`.

Job `tls` trong `ci.yml`: lệnh chạy đổi từ năm `--test …` thành **`--tests`** (không còn danh sách
nào để quên); dòng `TLS tests that ran: N` bằng `grep -c` thay bằng lời gọi script trên với
`/tmp/tls-tests.log`; vòng lặp ba lần `--test tls` **giữ nguyên**. Khối comment dài (dòng 456–479)
rút lại: *danh sách đã bỏ; bằng chứng là build tự liệt kê và log phải khớp*.

**Vì sao chạy cả bộ `engine`** thay vì tính ra danh sách binary có test `tls`-only: một test *không*
bị `cfg` che vẫn có thể **đổi nghĩa** dưới `tls` (ví dụ `settings.rs:707` có nhánh
`#[cfg(not(feature = "tls"))]` — test của `into_table` chạy hai đường khác nhau tuỳ build). Chỉ chạy
cả bộ mới nói được "bộ test này xanh **dưới `tls`**". Giá: job `tls` từ 40 s lên nhiều nhất ~5 phút,
dưới job `bench` 431 s, đường găng CI không đổi. Nếu một ngày nó thành đường găng, phương án dự
phòng có tên: liệt kê hai lần (có/không feature), lấy hiệu, chỉ chạy binary có test trong hiệu —
cùng script, thêm một cờ. Không làm bây giờ.

**Không** so `#[cfg(feature = "tls")]` bằng grep: `#[cfg(all(feature = "tls", unix))]`, một `mod`
bị che cả khối, một `cfg_attr` — lại là cách viết. Danh sách đến từ **binary đã build** dưới đúng
feature, đó là nghĩa.

### D. Item 67 — khuyến nghị: **thu hẹp** về những ô *là giá trị*, rồi đóng phần đó

**Điều thành thật là item 67 không đóng được như hàng của nó ngầm giả định.** Ô *Meaning* là văn
xuôi; câu ghi chú *"does nothing without `SocketUseSSL=Y`"* — lỗi thật đã xảy ra — là văn xuôi. Máy
không kiểm được văn xuôi mà không bắt người viết đổi sang một ngôn ngữ máy đọc, và khi đó tài liệu
thôi là tài liệu. Nên đề xuất là:

> **Những ô đã là giá trị thì phải đúng với parser; ô là văn xuôi thì kiểm tay, và tài liệu nói rõ
> ranh giới đó.** Item 67 được ghi là *thu hẹp rồi đóng*, phần dư có tên.

Bốn probe mới trong `mod doc_table` (`settings.rs`, chỉ `#[cfg(test)]`), thêm vào — không thay —
ba test hiện có. `doc_rows` đổi thành đọc **theo tên cột của header** (`Key`, `Values`, `Default`,
`Where`), vì bốn bảng §1 có hai kiểu cột.

1. **Câu đếm là thật.** Dòng `**<Số bằng chữ> keys** are recognised` trong §1: số bằng chữ phải bằng
   `english(n)` với `n = arm_literals(NAME_FN).len()`; `english` phủ 1–99 (`thirty`, `thirty-one`).
   "Four hundred" → đỏ.
2. **Default viết ra thì không đổi gì.** Ô *Default* **bắt đầu bằng một literal trong backtick**
   (`` `30` ``, `` `N` ``, `` `120000` (2 minutes) ``, `` `acceptor` — every file… ``) → đó là default
   được khai; phần sau literal là ghi chú. Với mỗi key như vậy: parse **mẫu tối thiểu** không có key,
   và mẫu ấy cộng dòng `KEY=<default>` ở cuối `[DEFAULT]`; hai kết quả phải **bằng nhau** trên
   `configs()`, `role`, `dial` (`{:?}`), `tls()`, `log()` — **không** so `role_line`/`tls_line`, vì
   đó là vị trí, không phải cấu hình. Ô *Default* bắt đầu bằng chữ (`required`, `none`,
   `all seven days`, `16 × …`) → bỏ qua và **đếm**. Ba mẫu, chọn theo nhóm key (exhaustive `match`
   trên `Key`, không có `_`, nên key mới **buộc** phải khai nhóm): `ACCEPTOR` (`BeginString`,
   `SenderCompID`, `[SESSION]` + `TargetCompID`), `INITIATOR` (thêm `ConnectionType=initiator`,
   `SocketConnectHost`, `SocketConnectPort`), `TLS` (thêm `SocketUseSSL=Y` và hai đường dẫn PEM;
   chỉ `#[cfg(feature = "tls")]`, còn lại là *Skip, đếm*). Dự kiến probe được **15** key
   (16 với `tls`): `HeartBtInt`, `MaxSkewMillis`, `ConnectionType`, ba `ResetOn*`, `LogonTimeout`,
   `LogoutTimeout`, `AllowUnknownMsgFields`, `ValidateUserDefinedFields`,
   `SendNextExpectedMsgSeqNum`, `EnableLastMsgSeqNumProcessed`, `TimestampPrecision`,
   `ReconnectInterval`, `SocketUseSSL`, (+`TlsRequireKernel`). Con số thật đo ở bước 4 rồi thành
   **sàn chỉ được tăng**, như `FLOOR = 30`.
3. **Ô Values chỉ gồm literal thì đúng là tập parser nhận.** Ô *Values* mà sau khi bỏ hết literal
   trong backtick chỉ còn `or`, dấu phẩy và khoảng trắng (`` `Y` or `N` ``; `` `3`, `6` or `9` ``;
   `` `acceptor` or `initiator` ``) là một **liệt kê**. Với mỗi key như vậy, trên mẫu của nhóm nó:
   viết từng literal → parse **không** được thất bại bằng một *lỗi giá trị* (tập hữu hạn:
   `NotAFlag`, `NotANumber`, `BadConnectionType`, `UnsupportedPrecision`, `BadTime`, `BadWeekday`,
   `ValueTooLong` — lỗi khác như `MissingKey`, `NeedsFeature`, `WrongRole` là *ngữ cảnh*, chấp
   nhận); viết một literal **lạ** (cái đầu tiên trong `1`, `true`, `nope` không có trong liệt kê) →
   parse **phải** thất bại bằng một lỗi giá trị. `` `1` or `0` `` cho `SocketUseSSL` → đỏ ở
   `SocketUseSSL=1 → NotAFlag`. Ô có chữ khác (`ASCII, max 32 bytes, e.g. …`,
   `` `Monday`/`Mon` … ``) → bỏ qua, đếm.
4. **Ô Where nói `[DEFAULT]` only thì parser từ chối ở `[SESSION]`.** Ô *Where* chứa
   `` `[DEFAULT]` **only** `` hoặc `` `[DEFAULT]` only `` → key đó trong `[SESSION]` của mẫu
   `ACCEPTOR` với giá trị bất kỳ phải cho `Problem::DefaultOnly`; ô chứa `` `[DEFAULT]` or `[SESSION]` ``
   hoặc `` `[SESSION]` (or `[DEFAULT]`) `` → cùng cách đặt **không** được là `DefaultOnly`. Ô khác
   (`initiator only`, không có cột) → bỏ qua, đếm. Đây là **anh em** của lỗi đã xảy ra: câu ghi
   chú "does nothing" không kiểm được, nhưng lời khai `[DEFAULT]`-only ngay cạnh nó thì kiểm được.

Kèm theo, trong `CONFIGURATION.md` ngay dưới câu đếm, **một đoạn ngắn nói ranh giới**: *ô Default
bắt đầu bằng literal, ô Values chỉ gồm literal, và lời khai `[DEFAULT]` only/or `[SESSION]` trong
Where được `settings.rs` `doc_table` kiểm; mọi câu khác trong bảng là lời hứa kiểm tay.* Đó là
"everything that implies" của hàng 67, viết thành chữ: **người viết một ô Default bằng văn xuôi thì
mất máy canh cho hàng đó, và sàn đếm là cái nhìn thấy điều đó.**

**Không đề xuất, và vì sao:**

- *Sinh bảng §1 từ code* (một `Key::doc()` rồi render): tài liệu thành bản in; các đoạn văn giữa
  bảng, `[changed …]`, lý do đặt tên theo QuickFIX C++ — tất cả không có chỗ. Người đọc mất, máy
  được. Không đáng.
- *Một ngữ pháp cho mọi ô Values/Default*: ~10 hàng có default là câu có nghĩa (`required when
  SocketUseSSL=Y; refused otherwise`, `16 × ReconnectInterval`). Ép vào ngữ pháp là mất nghĩa; để
  ngoài ngữ pháp thì cũng là "bỏ qua, đếm" như trên. Cùng kết quả, đắt hơn.
- *Kiểm ô Meaning và các câu ghi chú*: không có cách nào không phải là đọc.
- *Mở rộng sang §2 (`Limits`)*: bảng khác, cơ chế khác, plan khác.

### E. `shellcheck` — quyết định: **vào plan này một phần, phần còn lại là plan riêng**

Ba script plan này viết hoặc viết lại (`check-no-crate-root-allow.sh`, `check-scratch-fixtures.sh`,
`check-feature-gated-tests-ran.sh`) phải qua `shellcheck -S warning` — thêm **một dòng** vào job
`lint-config` của `ci.yml`, chỉ gọi tên ba file đó. Trên bàn: `sudo apt install shellcheck` (Ubuntu
đóng gói; runner có sẵn 0.9.0). Bullet *Not proven* của `STATUS.md` được **thu hẹp** (còn 16 script
kia), không gạch.

Vì sao **không** quét cả `scripts/`: đó không phải lỗi "cách viết vs nghĩa", và trong 19 script có
`check-no-kernel-sleep.sh`, `check-standard-gives-the-core-back.sh` — sửa quoting ở đó rồi chứng
minh không đổi hành vi **cần máy §9**, vi phạm điều kiện đầu của plan này. Plan riêng, machine-bound.

## Bất biến bị đụng tới

Việc này không đụng `codec`, `session`, `transport`; `engine` chỉ ở module `#[cfg(test)]` của
`settings.rs`. Nhưng nó **sửa gate của bất biến 7** và **tinh thần của bất biến 10**:

- **7** — A là cái canh bất biến 7 ở gốc crate. Sau A, câu ở `CLAUDE.md` §2 được mở rộng lại
  **đúng chừng đã đo**: từ *"the ordinary spelling"* thành *"mọi inner attribute mà lexer của Rust
  thấy ở gốc crate, đo trên bốn cách viết đã qua mặt regex"* — không hơn (module-level vẫn là
  item 55; `RUSTFLAGS`, `--cap-lints`, file `include!` vẫn ngoài).
- **7, cả code test** — module `doc_table` mới không có `unwrap`/`expect`/`panic` (bước 4 của plan
  trước đã đo: `clippy` từ chối cả trong `#[cfg(test)]`; dùng `assert!`, `strip_prefix`, `split_once`).
- **10** — không có con số hiệu năng nào. Con số duy nhất là thời gian job CI, trích từ run
  `34688952076`, ghi là số của run đó.
- **1, 2, 4, 5, 6, 8, 9** — không chạm. `tools/attr-scan` là tool, không phải library crate, không
  nằm trên đường nóng nào; nó cũng không được bất kỳ crate nào phụ thuộc.

## Chia việc

Tier theo `CLAUDE.md` §12 và tín hiệu định tuyến trong bảng. **Không bước nào là runner (haiku)**:
mỗi bước đều có ít nhất một đảo chiều phải *dự đoán câu đỏ* rồi đối chiếu — đó là phán đoán. Bước
1, 2, 3, 4 **file rời nhau**, chạy song song được; bước 5 sau tất cả; bước 6 sau bước 5.

| Bước | Kết quả | Được chạm | Không được chạm | Gate đóng bước | Xong khi | Đảo chiều (chi tiết ở *Cách kiểm chứng*) | Tier · tín hiệu |
|---|---|---|---|---|---|---|---|
| 1 | `tools/attr-scan` (Cargo.toml, `src/main.rs`); `check-no-crate-root-allow.sh` A1/A2/A0b trên output của tool, header viết lại; `Cargo.toml` gốc thêm member; `DESIGN.md` §3 bảng crate thêm hàng; `README.md` mục layout thêm dòng | `tools/attr-scan/**`, `scripts/check-no-crate-root-allow.sh`, `Cargo.toml` (chỉ `members`), `docs/DESIGN.md` §3, `README.md` layout | mọi thứ dưới `crates/`, `ci.yml`, script khác | `scripts/check-no-crate-root-allow.sh` in `ok — 6 crate roots, 9 manifests, M inner attributes`; `cargo clippy --all-targets -- -D warnings`; `cargo fmt --all --check`; `scripts/check-scratch-fixtures.sh` vẫn `ok` | R-A1…R-A4 đỏ đúng câu, R-A5 xanh, con số `M` ghi lại | R-A1 `#![/*x*/allow(clippy::unwrap_used)]` ở `crates/session/src/lib.rs:1` → `FAIL — crate-root allow at crates/session/src/lib.rs:1: allow(…)`; R-A2 `# ! [ expect ( clippy::panic ) ]`; R-A3 `#!` xuống dòng `[cfg_attr(test, allow(dead_code))]`; R-A4 `#![warn(clippy::unwrap_used)]` → câu A2; R-A5 (đối chứng xanh) `#![doc = "#![allow(clippy::unwrap_used)]"]` → `ok` | **opus** · đáp án sai = bất biến 7 tắt (§12: chạm bất biến → senior ít nhất) |
| 2 | `check-scratch-fixtures.sh` với dòng-sống, `cp` ở vị trí lệnh, tiền tố gán bất kỳ, B2b; header viết lại | `scripts/check-scratch-fixtures.sh` | mọi thứ khác | script in `ok — 19 scripts, 1 enter a scratch dir, 1 pins` trên cây sạch | R-B1…R-B3 đỏ đúng câu; R-B4 xanh | R-B1 sửa `check-lint-config.sh:48` thành `# TODO: cp …` → `FAIL — …:59 enters … and never copies rust-toolchain.toml`; R-B2 `:19` thành `readonly TMP="$(mktemp -d)"` **và** xoá `:48` → cùng câu (nếu chỉ đổi `readonly` mà giữ `cp` thì phải **xanh** — đó là R-B4, đối chứng); R-B3 thêm `cd "$(mktemp -d)"` vào một script → `FAIL — enters a scratch dir it never named at …` | **sonnet** · một file, từng assertion đã viết sẵn; đáp án sai = một đảo chiều đỏ sai câu |
| 3 | `scripts/check-feature-gated-tests-ran.sh`; job `tls` dùng `--tests` + script; dòng `shellcheck` vào job `lint-config` (mục E) | `scripts/check-feature-gated-tests-ran.sh`, `.github/workflows/ci.yml` (job `tls` và một dòng ở `lint-config`) | mọi thứ khác; **không** sửa test nào trong `crates/` ngoài canary tạm của R-C1, gỡ trước khi báo cáo | trên bàn: `scripts/check-feature-gated-tests-ran.sh fixbolt-engine tls <log giả>` với log giả sinh từ chính danh sách → `ok — … N listed, N accounted for`; `shellcheck -S warning` ba script sạch; trên CI: job `tls` xanh **và** in dòng `ok` của script | R-C1…R-C3 đỏ đúng câu; CI run id cho commit này in dòng của script | R-C1 thêm `#[cfg(feature = "tls")] #[test] fn canary_in_a_file_nobody_listed() {}` vào `crates/engine/tests/admin.rs`, liệt kê → có `tests/admin.rs\tcanary_in_a_file_nobody_listed`; log giả **thiếu** dòng đó → `FAIL — tests/admin.rs::canary_in_a_file_nobody_listed is in the tls build and did not run`; R-C2 log rỗng → `FAIL — 0 accounted for`; R-C3 `--features tsl` → exit 2 kèm lỗi của `cargo`, không phải `ok`; R-C4 (shellcheck) chèn `echo $x` không ngoặc → `SC2086` | **sonnet** · hai file, output đã đo hình dạng; đáp án sai = một run CI đỏ |
| 4 | Bốn probe mục D trong `mod doc_table`; `doc_rows` theo header; sàn probe; đoạn ranh giới trong `CONFIGURATION.md`; **mọi ô sai tìm thấy được sửa trong cùng commit và ghi vào nhật ký** | `crates/engine/src/settings.rs` (**chỉ** `mod doc_table`), `docs/CONFIGURATION.md` §1 | phần không-test của `settings.rs`, `crates/session`, mọi `tests/*.rs` | `cargo test -p fixbolt-engine --lib doc_table` xanh **cả hai**: mặc định và `--features tls`; `cargo clippy --all-targets -- -D warnings` (và `--features tls`); `cargo test --all`; `--no-default-features` | R-D1…R-D5 đỏ đúng câu; số key probe được ghi lại và thành sàn | R-D1 "Thirty" → "Four hundred"; R-D2 `SocketUseSSL` Values → `` `1` or `0` `` → `doc lists \`1\` as a value of SocketUseSSL but the parser refuses it: NotAFlag`; R-D3 `ResetOnLogon` Default → `` `Y` `` → `documents default \`Y\` but writing ResetOnLogon=Y changes the parsed settings`; R-D4 `TlsRequireKernel` Where → `` `[DEFAULT]` or `[SESSION]` `` → `documented as allowed in [SESSION] but the parser says DefaultOnly`; R-D5 **phía code** `DEFAULT_RECONNECT_INTERVAL_SECS` 30 → 31 → câu R-D3 cho `ReconnectInterval` (chứng minh gate đọc code, không chỉ đọc doc) | **opus** · `engine` (§12); thứ tự từ chối của parser quyết định mẫu nào dùng được — cần đọc parser, không chỉ đọc brief |
| 5 | Docs đồng bộ (mục *Tài liệu phải cập nhật*), `STATUS.md` item 66/67/68 đóng với câu đã quan sát, *Not proven* thu hẹp, *Start here* handoff, nhật ký giao hàng plan này | `CLAUDE.md` §2, `docs/DESIGN.md` §6, hai `docs/reference/`, `STATUS.md`, plan này | code, script, `ci.yml` | `python3 scripts/check-links.py` không link chết; `cargo test --all` không đổi | mọi câu "ordinary spelling"/"by the shapes it knows" được mở lại **đúng chừng đo được**, không hơn | không có đảo chiều — là văn bản; kiểm bằng `grep -n 'ordinary spelling' CLAUDE.md docs` phải trống | **sonnet** · đọc và chọn chữ |
| 6 | **Senior review PR**, context mới, đề bài duy nhất: *tấn công ba gate mới bằng cách viết chưa thử*; mỗi cách qua mặt tìm được → sửa ngay trong PR nếu ≤ 20 dòng, nếu không thì ghi thành open item với cách viết nguyên văn | như bước 1–4 tuỳ phát hiện | — | các gate của bước 1–4 chạy lại xanh sau sửa | reviewer nêu **ít nhất** năm cách viết đã thử cho mỗi gate và kết quả từng cái | chính review là đảo chiều | **opus**, context mới · lăng kính khác, không phải bản sao |

**Nếu phải cắt:** sau bước 1 + 2 + 5 thì item 66 đóng; sau bước 3 thì 68 đóng; bước 4 độc lập.
Không có cặp bước nào "không được dừng giữa".

## Cách kiểm chứng

Mỗi đảo chiều **viết câu đỏ dự đoán trước**, chạy, so, khôi phục, chạy lại thấy xanh, và **nói đỏ
ở assertion nào**. Một đảo chiều xanh ngoài dự kiến là *phát hiện*, không phải pass — bài học ở
[a-reversal-that-removed-the-guard-s-label-not-the-guard](../reference/a-reversal-that-removed-the-guard-s-label-not-the-guard.md):
mỗi đảo chiều dưới đây **đổi thứ mà gate quan sát**, không đổi nhãn.

**Bước 1.** Chạy `attr-scan` trực tiếp trước, xem output cho sáu `lib.rs` (con số `M`). Rồi
R-A1→R-A4 mỗi cái sửa **một** `lib.rs` thật, chạy script, trích nguyên câu FAIL và `exit=1`, `git
checkout` file, chạy lại `ok`. R-A5 là đối chứng **xanh**: cái mà một bộ bỏ-comment sẽ đỏ sai. Kèm
một lần chạy `cargo fmt --all --check` với R-A1 tại chỗ để ghi lại rằng nó vẫn im — chứng minh CI cũ
không nhìn thấy, gate mới nhìn thấy. **Không** làm lại R-A4 của plan trước (hiệu ứng compiler) — đã
đo, không đổi.

**Bước 2.** R-B1, R-B2, R-B3 trên `check-lint-config.sh` (script duy nhất hôm nay vào scratch), mỗi
lần một sửa, mỗi lần trích câu FAIL; R-B4 (`readonly` + giữ `cp`) **phải xanh** và in
`1 enter a scratch dir` — nếu nó đỏ là regex gieo sai, nếu `0 enter` là B2b nuốt mất. Khôi phục
bằng `git checkout scripts/check-lint-config.sh`.

**Bước 3.** Log giả cho R-C1/R-C2 sinh bằng `awk` từ output `--list` thật (mỗi `tên: test` →
`test tên ... ok` dưới cùng dòng `Running`); đó là unit test của phần *đối chiếu*, và nói rõ là log
giả. Phần *đầu-cuối* — script chạy trên log thật của `--tests --features tls` — chỉ CI có kTLS
làm được; **id run** của commit bước 3 là bằng chứng, và dòng đáng đọc là dòng `ok — … listed, …
accounted for`, không phải dấu tick. Canary R-C1 **gỡ khỏi `tests/admin.rs`** trước khi báo cáo;
`git status` phải sạch dưới `crates/`.

**Bước 4.** Chạy `cargo test -p fixbolt-engine --lib doc_table` **trước** khi sửa tài liệu: nếu một
probe đỏ trên cây sạch, đó là **một ô đang sai hôm nay** — sửa ô, ghi vào nhật ký với câu đỏ, đây là
giá trị đầu tiên của bước. Rồi R-D1→R-D4 sửa `CONFIGURATION.md`, R-D5 sửa một hằng trong code, mỗi
lần một cái, trích câu, `git checkout`. Chạy toàn bộ **hai lần**: mặc định và `--features tls` (mẫu
`TLS` chỉ tồn tại dưới `tls`; bước 3 làm cho lần thứ hai cũng chạy trên CI).

**Bước 6.** Reviewer nhận plan này (mục *Cách làm*, không nhận reasoning) và ba script; báo cáo là
bảng *cách viết → kết quả*; mọi hàng "qua mặt" kèm lệnh và output.

**Manager tự chạy lại** gate đóng mỗi bước trên đúng commit đóng, và đặt tên run CI cho commit cuối.

## Tài liệu phải cập nhật

Theo bảng `CLAUDE.md` §4, đi từng hàng, **trong cùng commit với bước**:

- [ ] **Bước 1** — *thêm crate*: `DESIGN.md` §3 bảng crate (hàng `tools/attr-scan`: "reads the
      inner attributes of a crate root with `proc-macro2`, for `check-no-crate-root-allow.sh`; no
      crate depends on it"), `README.md` layout, `Cargo.toml` members. *Dependency mới*: `proc-macro2`
      lý do ở plan này, mục A. Header script: bốn dòng "qua mặt" thay bằng thấy/không-thấy.
- [ ] **Bước 2** — header script: ba dòng "qua mặt" thay bằng ba luật + hai giới hạn còn lại.
- [ ] **Bước 3** — `ci.yml` khối comment job `tls` (rút lại); header script mới nói `--tests`
      không gồm bench/example; `STATUS.md:9`, `:141`, `:198` nhắc `TLS tests that ran: 55` — **để
      nguyên** (lịch sử), nhưng *Start here* mới nói dòng đáng đọc từ nay là dòng của script.
- [ ] **Bước 4** — `CONFIGURATION.md` §1: đoạn ranh giới ngay dưới câu đếm; mọi ô sai tìm thấy;
      docstring `mod doc_table` nói bốn câu hỏi nó trả lời và câu nào không (Meaning, ghi chú).
      Không có API công khai đổi → không `CHANGELOG.md`.
- [ ] **Bước 5** — *sync gate*: `CLAUDE.md` §2 dòng 92–96 (bỏ "ordinary spelling", thay bằng câu
      đã đo, nêu bốn cách viết) và dòng 105–110 (`check-scratch-fixtures.sh`: ba luật);
      `DESIGN.md` §6 hàng 860, 861 (cột *what red means* mở lại đúng chừng) và **một hàng mới** cho
      `check-feature-gated-tests-ran.sh` ("every test the `tls` build lists ran in the `tls` job");
      `docs/reference/an-allow-at-the-top-of-a-file-silenced-the-whole-crate.md:133` và
      `a-scratch-fixture-inherits-the-machine.md:88` (đoạn *Guarded since*); `STATUS.md` item 66,
      67 (**thu hẹp rồi đóng**, phần dư nêu tên: Meaning và ghi chú), 68 đóng, item 62 thêm một
      dòng; *Not proven*: bullet `shellcheck` thu hẹp còn 16 script; *Start here* handoff.
- [ ] **Bẫy mới tìm ra** ở bất kỳ bước nào → `docs/reference/`, cùng commit; nếu là bài học về
      *testing* → đánh `[to testing-skills]`. Ứng viên đã thấy trước: *"a lint that only looks at
      outer attributes"* (mục *Tra cứu*) — nếu bước 1 đo được thêm gì thì viết, nếu không thì một
      câu trong header script là đủ.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| `attr-scan` đọc `#![doc = "…"]` sinh từ `//!` là attribute — đúng, nhưng làm `M` lớn; ai đó "tối ưu" bỏ `doc` khỏi đếm rồi A0b thành vô nghĩa | A0b so `M` với **0**, không so với sàn; header nói `M` gồm `doc` |
| `cargo run -p attr-scan` biên dịch bằng profile `dev` mỗi lần CI — chậm | đo ở bước 1, ghi vào nhật ký; nếu > 30 s thì `--release` là một cờ, không phải thiết kế lại |
| Trong bước 1, `#!` viết `# !` cho hai Punct rời, tool so ký tự `#` rồi `!` liền nhau về **token**, không về **byte** — đã đo (R-A2 dạng `# ! [`) | R-A2 |
| Bước 2: regex gieo mới bắt cả `echo TMP=…` → gieo thừa, có thể **đỏ thừa** ở một script tương lai | hướng sai là đỏ; header ghi; không có script hôm nay khớp (kiểm bằng `19 scripts … ok`) |
| Bước 3: `--tests` kéo `hft_wire.rs`, `shard_hft.rs` vào job `tls` — nếu chúng flaky trên runner thì job `tls` đỏ vì lý do không phải TLS | chúng đã chạy trong job `fmt · clippy · test` cùng loại runner; `--no-fail-fast` + script in **tên** test đỏ; nếu xảy ra là open item riêng, không phải lý do quay về danh sách |
| Bước 3: `ignored` được tính là "có mặt" — một test `tls` bị `#[ignore]` sẽ lọt | script in `(I ignored)`; sàn `I == 0` hôm nay, ghi vào nhật ký; nếu ai `#[ignore]` một test `tls` thì con số đổi |
| Bước 4: mẫu chứa sẵn key đang probe → `RepeatedKey` thay vì lỗi giá trị → đỏ sai | mẫu `ACCEPTOR` không chứa `ConnectionType`/`SocketUseSSL`; probe `ConnectionType` và `SocketUseSSL` dùng `ACCEPTOR`; `TLS` chỉ cho ba key cert/kernel; nói trong brief |
| Bước 4: `settle` từ chối `TlsRequireKernel` không có `SocketUseSSL=Y` **trước** khi đọc giá trị → literal lạ không cho `NotAFlag` | vì thế `TlsRequireKernel` dùng mẫu `TLS`; R-D2 chạy trên `SocketUseSSL` (mẫu `ACCEPTOR`), không trên `TlsRequireKernel` |
| Bước 4: `doc_rows` cũ nhận hàng 4 cột lẫn 6 cột nhờ chỉ đọc ô đầu; đọc theo header mà một bảng thiếu `Where` → index sai | đọc theo **tên** cột từ dòng header của *từng* bảng; bảng không có cột → probe đó *Skip, đếm* |
| Bước 4: "Skip, đếm" là cửa thoát — ai cũng có thể chuyển key sang Skip | `match` exhaustive không `_` buộc khai nhóm; sàn số key probe được chỉ tăng; mỗi Skip có comment vì sao |
| Reversal no-op (bài học ba lần trên nhánh trước) | mỗi đảo chiều ở bảng *Chia việc* **đổi thứ gate quan sát** và có câu đỏ viết trước; R-A5, R-B4 là đối chứng xanh có chủ đích và được gọi tên là đối chứng |

## Rủi ro

| Rủi ro | Mức | Cách xử lý · **cái bắt được nó** |
|---|---|---|
| `proc-macro2` fallback lexer khác `rustc` ở một cạnh hiếm (raw string nhiều `#`, lifetime `'a` cạnh `'` literal) → tool lex lỗi trên một `lib.rs` hợp lệ | thấp | tool exit 2 nêu file, script **đỏ** (không pass) · **A0b và exit 2**; nếu xảy ra: ghi `docs/reference/`, và `syn` là bước lùi có tên |
| Bước 1 đổi gate của bất biến 7, và bản mới lại có lỗ khác | trung bình | bước 6 là senior review context mới với đề bài *tấn công*; R-A5 chứng minh hướng "đỏ thừa" cũng được nghĩ tới · **bước 6** |
| Job `tls` chậm hơn dự kiến (bộ `engine` dưới `tls` > 5 phút) | thấp | số đo ở run CI đầu tiên của bước 3; ngưỡng: nếu job `tls` > job `bench` (431 s) thì bật phương án hiệu-hai-danh-sách, cùng script · **thời gian job trong run id** |
| Một ô `CONFIGURATION.md` đang sai hôm nay mà probe bước 4 phát hiện — không phải rủi ro, là mục đích; rủi ro là sửa **doc cho khớp code** trong khi **code sai** | trung bình | quy tắc ở brief bước 4: ô sai → dừng, hỏi manager trước khi sửa, kèm cả hai phía; manager quyết định phía nào sai · **`git diff` của bước 4 chỉ được chạm `doc_table` và `.md`** |
| Sàn số key probe được đặt bằng số đo, rồi không ai tăng khi thêm key có default literal | thấp | cùng cơ chế `FLOOR = 30` đã có; item 67 ghi rõ *sàn phải tăng theo*; bước 6 hỏi con số |
| `shellcheck` 0.9.0 trên runner khác bản trên bàn → cảnh báo khác nhau | thấp | gate là CI; trên bàn chỉ cần `-S warning` sạch với bản apt · **run id** |
| Hai developer song song (1, 2, 3, 4) đụng `ci.yml`: chỉ bước 3 chạm nó — bước 1 **không** sửa `ci.yml` (job `lint-config` đã gọi script bằng tên, không đổi) | thấp | bảng *Được chạm* · manager kiểm `git diff --stat` từng bước |

## Ngoài phạm vi

- `shellcheck` trên 16 script còn lại — plan riêng, cần máy §9 cho hai script (mục E).
- Inner attribute ở **module** (`mod x { #![allow] }`) — item 55, không đổi.
- `RUSTFLAGS`, `--cap-lints`, file sinh bởi `include!` — vẫn ở danh sách "không thấy" của script A.
- Kiểm ô *Meaning*, câu ghi chú, đoạn văn giữa bảng — kiểm tay, nói rõ trong tài liệu (mục D).
- §2 của `CONFIGURATION.md` (`Limits`, const generics) — không probe.
- Phương án hiệu-hai-danh-sách cho job `tls` — có tên, không làm trừ khi thời gian bắt buộc.
- `cargo-nextest`, `syn`, `shfmt` — đã cân nhắc, không dùng (lý do ở *Tra cứu* và mục B).
- Arm TLS của `check-no-kernel-sleep.sh`, `DESIGN.md` §8 hàng TLS — plan TLS, không phải đây.

## Nhật ký giao hàng

*(trống — plan chưa duyệt, chưa dựng gì)*

**Đây là phần sống sót qua nén context** — phiên sau đọc mục này trước tiên.
