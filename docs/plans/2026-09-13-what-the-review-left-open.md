# Bốn chỗ bản review PR #69 để lại — đóng items 80, 81, 82, 83

> **Loại:** Plan · **Ngày:** 2026-09-13 · **Trạng thái:** Chờ duyệt
> **Phạm vi:** bộ probe `doc_table` trong `crates/engine/src/settings.rs` (probe 3, probe 7), một ô của `docs/CONFIGURATION.md` §1, doc comment của `crates/session/src/clock.rs`, `scripts/check-links.py`, cờ rustdoc của job CI `feature-sets`, một ADR mới (ADR-0066)

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

PR #69 đóng items 75–79 và mở bốn item mới, `STATUS.md` hàng 80–83. Cả bốn đều nhỏ và cùng một
dáng: **một cái kiểm tra đang xanh về một điều nó không thực sự nhìn thấy.**

- **80** — test `the_reader_table_matches_the_call_sites` kiểm tra một key được *đưa cho hàm đọc
  nào*, không kiểm tra *giá trị nào lọt tới hàm đó*. Viết thêm một phép so sánh ngay cạnh lời
  gọi (`v.1 == "yes" || flag(v, …)`) là parser nhận thêm `yes`, và cả bộ test vẫn xanh.
- **81** — parser từ chối `ReconnectInterval=0`, nhưng tài liệu chỉ ghi "integer, seconds". Tài
  liệu nói ít hơn parser. Sửa ô thành "positive" ngay bây giờ lại làm probe 7 đỏ **sai**.
- **82** — `scripts/check-links.py` không báo một URL tuyệt đối vào chính repo này trỏ tới đường
  dẫn không tồn tại (`…/fixbolt/blob/main/DESIGN.md` — file thật nằm ở `docs/DESIGN.md`).
- **83** — rustdoc cảnh báo tài liệu public của `parse_utc` link tới hằng private. Chỉ là cảnh
  báo, nên job CI in nó ra mỗi lần chạy và vẫn xanh.

Muốn đạt: mỗi item hoặc đóng hẳn, hoặc đóng phần đo được và **ghi rõ phần còn lại** — kèm một
phép đảo chiều (reversal) cho mỗi cái kiểm tra mới, viết sẵn câu FAIL trước khi dựng.

## Những gì đã biết chắc

Mọi số dưới đây đo ngày 2026-09-13 trên Apple M5 (bàn làm việc), cây `4ea5349`, worktree
`fixbolt-wt-80`. Những phép đo cần sửa code được làm trên **một bản sao vứt đi** (`git archive
HEAD` vào scratchpad, target dir riêng) — không file nào trong `crates/` của worktree bị sửa.

**Số gốc của `doc_table` hôm nay** (`cargo test -p fixbolt-engine --lib doc_table -- --nocapture`,
cả bộ mặc định lẫn `--no-default-features`, hai lần in giống nhau):

```
probe 7 — bounded Values cells: 4 probed, 4 skipped, 25 not Numeric
probe 3, call-site leg: 25 call sites, 8 presence-only calls not counted
probe 3 — enumerated Values cells: 10 probed, 23 skipped
test result: ok. 11 passed; 0 failed; …
```

### Item 80

- Hàm `flag` (`settings.rs:974`) chỉ nhận `"Y"` và `"N"`. Lời gọi cho `ResetOnLogon` ở
  `settings.rs:1532-1533`. Ô tài liệu: `` `Y` or `N` ``.
- Chiều ngược của probe 3 là **tìm kiếm có biên** qua `candidates` (`settings.rs:2885`): mọi chuỗi
  1–2 ký tự trên bảng 66 ký tự, mọi chuỗi 3 chữ số, và 9 "hàng xóm" của mỗi literal được liệt kê
  (đổi hoa/thường, thêm `0`, `+`, `x`, nhân đôi). `yes`, `true`, `false`, `off` nằm ngoài tập đó.
- **Đo trên bản sao, cây gốc + nhánh `v.1 == "yes" || …`**: `test result: ok. 11 passed` — xác
  nhận item 80 bằng chạy.
- **Đo trên bản sao, `candidates` nới thêm một danh sách cách viết boolean** (yes/no/true/false/
  on/off/enable/enabled/disable/disabled, mỗi từ ba kiểu: thường, HOA, Viết-hoa-chữ-đầu):
  - cây không sửa: `probe 3 — enumerated Values cells: 10 probed, 23 skipped` · `1 passed`,
    `finished in 0.26s` (cùng test trên cây gốc: `0.22s`). `--no-default-features`: như trên.
    **Không có đỏ giả.**
  - thêm `v.1 == "yes" || …`: `docs/CONFIGURATION.md §1: ResetOnLogon lists ["Y", "N"] but the
    parser also accepts ["yes"] — either the document is short or the parser is lax`.
  - dạng "bọc giá trị" `flag((v.0, if v.1 == "true" { "Y" } else { v.1 }), Key::ResetOnLogon)`:
    đỏ với `… also accepts ["true"] …`; nhánh call-site vẫn `ok` (đúng như item 80 nói).
  - thêm `v.1 == "always" || …`: **vẫn xanh**. Đây là phần còn lại, đo được, không đoán.
- Danh sách có nguồn, không tự chọn: kiểu boolean của YAML 1.1
  ([yaml.org/type/bool](https://yaml.org/type/bool.html)) nhận đúng
  `y|Y|yes|Yes|YES|n|N|no|No|NO|true|True|TRUE|false|False|FALSE|on|On|ON|off|Off|OFF` — 22 chuỗi,
  đúng ba kiểu hoa/thường như trên. `configparser` của Python nhận `1/yes/true/on` và
  `0/no/false/off`, **không phân biệt hoa thường**
  ([docs.python.org](https://docs.python.org/3/library/configparser.html)). 22 chuỗi của YAML là
  **tập con** của tập đã đo xanh ở trên, và mỗi ứng viên được xét độc lập, nên tập con cũng xanh.
- ADR-0061 kết luận quét văn bản không đóng được bằng cách thêm pattern; test đã ghi điều đó
  (`settings.rs:3242-3250`).

### Item 81

- `Policy::new` (`reconnect.rs:62`) trả `FirstIsZero` khi `first_ms == 0`; `settings.rs:1644-1653`
  đổi lỗi đó thành `Problem::ImpossiblePolicy`.
- `is_about_the_value` (`settings.rs:2820-2831`) **không** có `ImpossiblePolicy`, và probe 3 cần nó
  hẹp như vậy (một từ chối ngoài tập đó bị probe 3 coi là lỗi của chính probe).
- Nhánh `positive` của probe 7 (`settings.rs:3455-3461`) đòi `is_about_the_value`; nhánh
  "0 được phép" và nhánh khoảng đã dùng `refused_for_its_value` (`settings.rs:3450-3454`) — một từ
  chối **khác với từ chối mà file mẫu tự nó đã có**. Lý do được ghi ngay đó, từ item 76.
- **Hôm nay không ô nào ghi `positive`** (các ô có dấu bao: `HeartBtInt` non-negative,
  `SocketConnectPort` `0`–`65535`, `LogonTimeout`/`LogoutTimeout` `0` is off). Nhánh `positive`
  chưa từng chạy trên một hàng thật; `ReconnectInterval` sẽ là hàng đầu tiên.
- **Đo trên bản sao:**
  - ô đổi thành `positive integer, **seconds**`, predicate giữ nguyên → đỏ giả:
    `docs/CONFIGURATION.md §1: ReconnectInterval says positive but the parser accepts 0 — read FIX
    4.4 before changing either side (Some(ImpossiblePolicy))` — câu còn nói sai: parser **không**
    nhận `0`.
  - thêm việc đổi nhánh `positive` sang `refused_for_its_value(&refusal)` → xanh:
    `probe 7 — bounded Values cells: 5 probed, 3 skipped, 25 not Numeric`, và **probe 3 không
    đổi**: `25 call sites, 8 presence-only` · `10 probed, 23 skipped` · `11 passed`.
  - thêm việc cho parser nhận `0` (`number::<u64>(v, Key::ReconnectInterval)?.max(1)`) → đỏ đúng:
    `… ReconnectInterval says positive but the parser accepts 0 … (None)`.
- Ô `ReconnectCeiling` (`integer, **seconds**, not below ReconnectInterval`) đã có test giữ:
  `a_reconnect_ceiling_below_the_interval_is_a_line_numbered_error`
  (`crates/engine/tests/settings_roles.rs:179`) đòi `Problem::ImpossiblePolicy` ở dòng
  `ReconnectCeiling=`. Không cần sửa.

### Item 82

- `names_a_repo_file` (`check-links.py:129-206`) chỉ tìm "đuôi" URL **khớp một file có thật**. URL
  của chính repo trỏ tới đường dẫn không có → không đuôi nào khớp → im lặng.
- Ngoại lệ `crates/library/README.md` (`check-links.py:231-236`) — file này bắt buộc dùng URL tuyệt
  đối vì được đưa lên crates.io/docs.rs — đếm URL là "checked" ngay khi **một đuôi nào đó** tồn
  tại. **Đo trên bản sao với script ở HEAD**: thêm vào README
  `https://github.com/tmthang86/fixbolt/blob/main/crates/docs/GUIDE.md` (sai đường dẫn, nhưng
  đuôi `docs/GUIDE.md` có thật) → `2034 internal links checked … no dead internal links`, exit 0.
  **Đây là một lỗ thứ hai, rộng hơn item 82 ghi.**
- Kiểm kê URL của chính repo trong `.md`/`.rs` hôm nay: 221 `actions/…`, 115 `pull/…`, 6 `blob/…`,
  1 gốc repo. Trong 6 `blob`, 3 là link Markdown trong README thư viện (đều có thật), 3 là URL trần trong
  code span ở plan/reference — không phải cú pháp link, nên script không đọc.
- Chạy script ở HEAD: `379 markdown and rust files, 2033 internal links checked, 0 absolute URLs
  …, 9 foreign GitHub URLs naming another repository (not judged)` · `no dead internal links`.
- **Nguyên mẫu trên bản sao** (URL chủ repo dạng `blob|tree|raw|blame/<ref>/<path>` → kiểm
  `os.path.exists(root/path)`): cây gốc xanh, không đổi số; `blob/main/DESIGN.md`,
  README `blob/main/docs/GUIDES.md` và README `blob/main/crates/docs/GUIDE.md` → cả ba FAIL với
  `this repository has no …`; `tree/main/docs/decisions`, `tree/main`, `pull/69` → không báo.
- Tiền lệ: `remark-validate-links` kiểm offline URL GitHub trỏ về repo cục bộ, bỏ tiền tố
  `/<owner>/<repo>/blob/` rồi **bỏ đúng một đoạn** làm tên nhánh
  (`value.split(slash).slice(1).join(slash)` trong `lib/index.js`) — tức là giả định ref một đoạn,
  giống cách làm dưới đây ([README](https://github.com/remarkjs/remark-validate-links)).

### Item 83

- `clock.rs:57-58`: `` [`LEN_SECONDS`] `` và `` [`LEN_MAX`] `` trong doc của `pub fn parse_utc`
  (`clock.rs:86`, module `pub mod clock`); hai hằng là `const` private (`clock.rs:34`, `:52`).
- Mức mặc định theo [tài liệu rustdoc](https://doc.rust-lang.org/rustdoc/lints.html):
  `private_intra_doc_links` **warn**; cùng mức warn: `broken_intra_doc_links`, `bare_urls`,
  `invalid_html_tags`, `invalid_codeblock_attributes`, `invalid_rust_codeblocks`,
  `redundant_explicit_links`. Mức allow: `missing_docs`, `unescaped_backticks`, `private_doc_tests`…
- Job `feature-sets` (`ci.yml:222`, `:227`) dùng
  `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::redundant_explicit_links"`,
  đúng như ADR-0065 quyết định 1 ghi. ADR-0065 đã `Accepted` → đổi cờ cần ADR mới (`CLAUDE.md` §5).
- Thêm `-D rustdoc::private_intra_doc_links`, `cargo doc --workspace --no-deps` →
  `error: public documentation for parse_utc links to private item LEN_SECONDS` (và `LEN_MAX`),
  `exit 101`; `--no-default-features` y hệt.
- **Toàn bộ cảnh báo rustdoc của workspace, gom nhóm**, là đúng hai cảnh báo đó, không có gì
  khác: host mặc định, host `--no-default-features`, `cargo hack doc --workspace --no-deps
  --feature-powerset --depth 2 --keep-going --target x86_64-unknown-linux-gnu` (`32` bộ, `EXIT=0`,
  với biến môi trường `ring` của
  [a-linux-only-module-is-invisible-to-a-mac-gate](../reference/a-linux-only-module-is-invisible-to-a-mac-gate.md)),
  và `cargo doc --workspace --no-deps --all-features --target x86_64-unknown-linux-gnu`.
- **Đo trên bản sao:** hai link viết thành code span → `RUSTDOCFLAGS="-D warnings" cargo doc
  --workspace --no-deps` và `--no-default-features` đều `Finished`, không một chẩn đoán. Đặt lại
  link → `error: public documentation for parse_utc links to private item LEN_SECONDS`. Với cờ
  **hiện tại** của CI, cùng cây đó chỉ `warning: … generated 2 warnings` rồi `Finished`.
- **Đo trên bản sao:** thêm `//! See https://example.com/fix for the spec.` vào
  `crates/codec/src/lib.rs` → với cờ hiện tại: `warning: this URL is not a hyperlink` rồi
  `Finished`; với `-D warnings`: `error: this URL is not a hyperlink`. Tức là đặt tên thêm một lint
  chỉ đóng item 83, lint warn kế tiếp sẽ là item 84.

## Cách làm

**Duyệt plan này là duyệt luôn hai lựa chọn sau** — nêu ra để chủ dự án thấy, không phải để chặn:

1. **Item 80 đóng có phần dư, ghi rõ.** Probe 3 bắt được mọi cách viết boolean trong danh sách
   YAML 1.1; một cách viết 3+ ký tự **ngoài** danh sách đó mà parser lén nhận (`always`, đã đo
   xanh) vẫn lọt. Không có tìm kiếm có biên nào đóng hết được, và quét văn bản thì ADR-0061 đã
   bác. Khuyến nghị: chấp nhận, ghi phần dư vào doc của test và vào *Not proven*.
2. **Item 83 đổi cổng rustdoc sang `-D warnings`** thay vì thêm một tên lint —
   [ADR-0066](../decisions/ADR-0066-the-rustdoc-gate-denies-every-warning.md) (`Proposed`), thay
   phần cờ của ADR-0065 quyết định 1. Khuyến nghị: chấp nhận; cái giá là một lần nâng toolchain
   có thể làm job đỏ, và toolchain đã được ghim.

### Item 83 — sửa doc, rồi siết cổng

- `crates/session/src/clock.rs:57-58`: bỏ ngoặc vuông, thành code span kèm giá trị —
  `` `LEN_SECONDS` (17) `` và `` `LEN_MAX` (30) ``. Không làm hai hằng thành `pub`: đó là đổi API
  public để sửa một câu tài liệu.
- `.github/workflows/ci.yml` dòng 222 và 227: `RUSTDOCFLAGS="-D warnings"`. Comment ngay trên
  bước đó nêu ADR-0066.
- `docs/DESIGN.md` §6 hàng "Every feature set…" (dòng 887): thay chuỗi cờ, dẫn ADR-0066.
- `docs/reference/a-linux-only-module-is-invisible-to-a-mac-gate.md`: lệnh trên bàn đổi cờ.
- ADR-0065: **chỉ dòng Status** thêm "decision 1's `RUSTDOCFLAGS` superseded by ADR-0066"; không
  sửa nội dung. ADR-0066 → `Accepted` khi plan được duyệt.

### Item 81 — ô tài liệu và predicate cùng lúc

- `docs/CONFIGURATION.md` §1, ô *Values* của `ReconnectInterval`: `positive integer, **seconds**`.
- `settings.rs`, probe 7, nhánh `says_positive`: điều kiện thành `refused_for_its_value(&refusal)`
  — **cùng một định nghĩa "bị từ chối vì giá trị"** mà hai nhánh kia đã dùng. `is_about_the_value`
  không đổi, nên probe 3 không đổi (đã đo). Câu FAIL đổi thành:
  `docs/CONFIGURATION.md §1: {name} says positive but the parser does not refuse {name}=0 for its value: {refusal:?}`
  — câu cũ nói "accepts 0" cả khi parser đã từ chối.
- `FLOOR` của probe 7: 4 → **5**; doc comment của hằng ghi `ReconnectInterval` là hàng thứ năm và
  là hàng đầu tiên nhánh `positive` chạy trên dữ liệu thật. Comment `settings.rs:3442-3449` thêm
  một câu: nhánh `positive` giờ dùng cùng predicate, vì `ReconnectInterval=0` bị từ chối là
  `ImpossiblePolicy`.
- Nhánh khoảng (`above`, `settings.rs:3480-3485`) giữ `is_about_the_value`: không hàng nào cần
  đổi, và đổi một assertion không có case là suy đoán.

### Item 80 — cho probe 3 thử cả những cách viết boolean quen thuộc

- `settings.rs`, trong `mod doc_table`: hằng mới
  `const SPELLINGS: &[&str] = &["y", "Y", "yes", "Yes", "YES", "n", "N", "no", "No", "NO", "true", "True", "TRUE", "false", "False", "FALSE", "on", "On", "ON", "off", "Off", "OFF"];`
  — **viết nguyên văn 22 chuỗi của YAML 1.1**, không tính hoa/thường bằng code (xem *Bẫy*:
  `indexing_slicing` đang `deny`). Doc comment của hằng dẫn nguồn YAML 1.1 và `configparser`.
- `candidates`: chèn `SPELLINGS` vào tập **trước** khi trừ `listed` (để `Y`/`N` được liệt kê vẫn bị
  trừ). Doc của `candidates` và của probe 3 thêm một chân: "các cách viết boolean quen thuộc", kèm
  số đo item 80. `MIN_UNIVERSE` giữ nguyên (tập chỉ lớn lên).
- Doc của `the_reader_table_matches_the_call_sites`, đoạn *What it cannot see*: giữ câu "nhánh này
  không thấy giá trị", thêm rằng chiều ngược của probe 3 **giờ thấy** hai dạng đã chứng minh
  (`yes` inline, `true` bọc), và nêu phần dư đo được: một cách viết 3+ ký tự ngoài `SPELLINGS`
  (`always`) vẫn xanh.

### Item 82 — URL vào chính repo phải trỏ tới đường dẫn có thật

- `scripts/check-links.py`, trong `names_a_repo_file`, ngay sau khi tính `own`: một quy tắc
  **(d)**, xét **trước** (c) và tìm đuôi. Áp dụng khi host là `github.com` (có hoặc không `www.`),
  owner là owner của repo (không phân biệt hoa thường), tên repo là tên hiện tại **hoặc** một
  tên trong `FORMER_NAMES`, và đoạn thứ tư là `blob`, `tree`, `raw` hoặc `blame`. Khi đó
  `path = unquote("/".join(parts[5:]))`; rỗng = gốc repo = có thật; nếu không
  `os.path.exists(root/path)` → báo **mất**. `os.path.exists` chứ không `isfile`: `tree/` trỏ thư
  mục. `commit/<sha>` không có đường dẫn → không xét.
- Ref được coi là **đúng một đoạn** (như `remark-validate-links`). Ref có `/` sẽ đỏ — ồn ào, sửa
  được bằng dùng `main` hoặc SHA; ghi trong docstring là giới hạn.
- Quy tắc (d) áp dụng cho **mọi** file, kể cả `crates/library/README.md`: ngoại lệ của README chỉ
  miễn "phải dùng đường dẫn tương đối", không miễn "phải có thật". Nó đóng luôn lỗ thứ hai đo ở
  trên.
- Khi đường dẫn có thật, hành vi giữ nguyên: ngoài README → vẫn báo "dùng đường dẫn tương đối"
  (quy tắc a); trong README → đếm là checked.
- Dòng tóm tắt thêm hai số: `{n} absolute URLs into this repository checked against its tree`
  và `{m} own-repository URLs that are not file links (not judged)` (actions, pull, …) — thấy được,
  không bị bỏ im lặng.
- **Sàn** cho quy tắc (d): `n ≥ 3` (README thư viện một mình đã có 3). Dưới sàn →
  `FAIL: the own-repository URL rule checked {n} URLs, below its floor of 3 — crates/library/README.md alone carries 3, so the rule has stopped matching`.
- Báo mất:
  `FAIL: {k} absolute URL(s) into this repository name a path it does not have`, mỗi dòng
  `  {rel}:{line}  →  {target}` và `      this repository has no {path}`.
- Docstring của script thêm đoạn (d), cùng giọng với (a)–(c), nêu số đo lỗ README.

## Bất biến bị đụng tới

Đã đi qua mười điều của `CLAUDE.md` §2:

- **3 (59 định nghĩa)** — bước 1 sửa `crates/session`, nhưng **chỉ dòng `///`**. Không đổi hành vi
  nên điều 3 không đòi chạy 59 định nghĩa; bằng chứng là `git diff` của bước 1 chỉ gồm dòng bắt
  đầu bằng `///`. `cargo test --all` của §7 vẫn chạy như mọi commit.
- **7 (không `unwrap`/`expect`/`panic`, không index gây panic)** — bước 3 và 4 sửa code test trong
  `mod doc_table`; `cargo clippy --all-targets -- -D warnings` giữ. Viết `SPELLINGS` nguyên văn để
  khỏi cắt chuỗi.
- **1, 2, 4, 5, 6, 8, 9, 10** — không đụng: không đường nóng, không session logic, không wait
  strategy, không feature flag, không `unsafe`, không nguồn QuickFIX, không số hiệu năng.

## Chia việc

Vai và model theo `CLAUDE.md` §12. Bước đụng `crates/engine` hoặc `crates/session` do **senior
developer (opus)**. Mở PR draft ngay commit đầu (`CLAUDE.md` §8). **Mọi bước chạy nối tiếp**: bước
2, 3, 4, 5 cùng sửa `docs/DESIGN.md` §6, bước 3 và 4 cùng sửa `settings.rs`.

| Bước | Kết quả | File đụng | Vai · model | §2 | Lệnh đóng bước | Phụ thuộc |
|---|---|---|---|---|---|---|
| 1 | **Item 83, doc**: hai link thành code span kèm giá trị | `crates/session/src/clock.rs` (chỉ dòng 57-58) | senior dev · opus | 3 (chỉ doc) | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` và thêm `--no-default-features`: `Finished`, không chẩn đoán; `git diff` chỉ dòng `///`; `cargo test -p fixbolt-session` | — |
| 2 | **Item 83, cổng**: `-D warnings` ở `ci.yml:222,227`; `DESIGN.md` §6 dòng 887; lệnh trong reference Linux; dòng Status ADR-0065; ADR-0066 `Accepted`; R83-1, R83-2 chạy trên bàn, trích, hoàn lại | `.github/workflows/ci.yml`, `docs/DESIGN.md`, `docs/reference/a-linux-only-module-is-invisible-to-a-mac-gate.md`, `docs/decisions/ADR-0065-*.md` (Status), `docs/decisions/ADR-0066-*.md` | developer · sonnet | — | lệnh Linux của reference với `RUSTDOCFLAGS="-D warnings"`: `EXIT=0`, đếm được `32` dòng `running cargo doc`; và `--all-features --target x86_64-unknown-linux-gnu`: `Finished` | 1 |
| 3 | **Item 81**: ô `ReconnectInterval`; nhánh `positive` dùng `refused_for_its_value`; câu FAIL mới; `FLOOR` 5; R81-1..3 | `docs/CONFIGURATION.md`, `crates/engine/src/settings.rs` (chỉ `mod doc_table`), `docs/DESIGN.md` §6 dòng 886 (≥ 4 → ≥ 5 ô) | senior dev · opus | 7 | `cargo test -p fixbolt-engine --lib doc_table -- --nocapture` và `--no-default-features`: `5 probed, 3 skipped, 25 not Numeric`, `25 call sites, 8 presence-only`, `10 probed, 23 skipped`, `11 passed`; `cargo clippy --all-targets -- -D warnings` | 2 |
| 4 | **Item 80**: `SPELLINGS` vào `candidates`; ba doc comment; R80-1..3 | `crates/engine/src/settings.rs` (chỉ `mod doc_table`), `docs/DESIGN.md` §6 dòng 886 (nêu chân "cách viết boolean") | senior dev · opus | 7 | như bước 3, số probe 3 **không đổi** `10 probed, 23 skipped`; hàng `tls` (11 probed) lấy từ job CI chạy `cargo test -p fixbolt-engine --tests --features tls` — manager trích dòng `probe 3 — … 11 probed` từ log | 3 (cùng file) |
| 5 | **Item 82**: quy tắc (d), hai số đếm mới, sàn 3, docstring; mục mới trong reference; `DESIGN.md` §6 dòng 916; R82-1..6 | `scripts/check-links.py`, `docs/reference/a-bare-filename-is-not-evidence-of-a-repository.md`, `docs/DESIGN.md` | developer · sonnet | — | `scripts/check-links.py`: exit 0, dòng tóm tắt có `3 absolute URLs into this repository checked against its tree` (hoặc hơn) và `no dead internal links`; `python3 -m py_compile scripts/check-links.py` | 4 (cùng `DESIGN.md`) |
| 6 | **Senior review**, context mới, đưa plan + lệnh gate, không đưa lý luận của manager; một lần cho cả PR | đọc; finding → sửa trên nhánh | senior dev · opus | 3, 7 | chạy lại mọi lệnh đóng bước 1–5 sau khi sửa | 1–5 |
| 7 | **Đóng**: `STATUS.md` (hàng 80–83, *Start here* mới, *Not proven*), CI xanh có run id cho commit cuối, merge | `STATUS.md`, plan này (*Nhật ký giao hàng*, trạng thái `Xong`) | manager | — | CI xanh trên commit đóng, **run id ghi vào `STATUS.md`** | 6 |

## Cách kiểm chứng

"Xanh" là output trích nguyên văn, không phải mã thoát. Mỗi guard mới hoặc đổi được chứng minh
bằng đảo chiều: sửa hỏng → thấy **đúng** câu FAIL dưới đây → hoàn lại → xanh. Câu FAIL viết sẵn ở
đây; đỏ bằng câu khác = chưa chứng minh.

| Đảo chiều | Sửa hỏng thế nào | Phải in (nguyên văn, hoặc phần đầu) | Bước |
|---|---|---|---|
| R83-1 | đặt lại `` [`LEN_SECONDS`] `` trong `clock.rs`, chạy `RUSTDOCFLAGS="-D warnings" cargo doc -p fixbolt-session --no-deps` | `error: public documentation for `parse_utc` links to private item `LEN_SECONDS`` | 2 |
| R83-2 | thêm `//! See https://example.com/fix for the spec.` vào `crates/codec/src/lib.rs`, cùng lệnh cho `-p fixbolt-codec` | `error: this URL is not a hyperlink` — và với cờ **cũ** của CI chỉ là `warning`, `Finished` (chứng minh vì sao không đặt tên lint) | 2 |
| R81-1 | **trước khi sửa predicate**: đổi ô thành `positive`, chạy probe 7 | `ReconnectInterval says positive but the parser accepts 0 — read FIX 4.4 before changing either side (Some(ImpossiblePolicy))` — đây là đỏ **giả** mà item 81 báo trước | 3 |
| R81-2 | sau khi sửa: `Some(v) => number::<u64>(v, Key::ReconnectInterval)?.max(1),` ở `settings.rs:1633` | `docs/CONFIGURATION.md §1: ReconnectInterval says positive but the parser does not refuse ReconnectInterval=0 for its value: None` | 3 |
| R81-3 | trả ô về `integer, **seconds**`, giữ `FLOOR` 5 | `probe 7 reached 4 rows, below its floor of 5 — either a bound was rewritten as prose, or this probe has stopped matching` | 3 |
| R80-1 | `&& (v.1 == "yes" \|\| flag(v, Key::ResetOnLogon)?)` ở `settings.rs:1533` | `docs/CONFIGURATION.md §1: ResetOnLogon lists ["Y", "N"] but the parser also accepts ["yes"] — either the document is short or the parser is lax` | 4 |
| R80-2 | `&& flag((v.0, if v.1 == "true" { "Y" } else { v.1 }), Key::ResetOnLogon)?` | như R80-1 với `["true"]`; `the_reader_table_matches_the_call_sites` vẫn `ok` | 4 |
| R80-3 | `&& (v.1 == "always" \|\| flag(v, Key::ResetOnLogon)?)` | **phải xanh** — `10 probed, 23 skipped`, `ok`. Đây là phần dư được ghi lại, không phải guard | 4 |
| R82-1 | một link Markdown nhãn `a`, đích `https://github.com/tmthang86/fixbolt/blob/main/DESIGN.md`, trong một file `docs/` | `FAIL: 1 absolute URL(s) into this repository name a path it does not have` · `this repository has no DESIGN.md` | 5 |
| R82-2 | trong `crates/library/README.md`: một link Markdown tới `https://github.com/tmthang86/fixbolt/blob/main/crates/docs/GUIDE.md` | như R82-1 với `this repository has no crates/docs/GUIDE.md` — script ở HEAD xanh trên đúng dòng này (đã đo) | 5 |
| R82-3 | đối chứng: `tree/main/docs/decisions`, `tree/main`, `pull/69` của chính repo | **không** báo; số `not file links (not judged)` tăng đúng 1 (cho `pull/69`) | 5 |
| R82-4 | một link Markdown tới `https://github.com/tmthang86/nanofixengine/blob/main/NOPE.md` | `FAIL: 1 absolute URL(s) …` · `this repository has no NOPE.md` | 5 |
| R82-5 | ref có `/`: `…/fixbolt/blob/plan/x/docs/GUIDE.md` | `FAIL` với `this repository has no x/docs/GUIDE.md` — giới hạn đã nêu, đỏ ồn ào chứ không xanh im | 5 |
| R82-6 | bỏ `"blob"` khỏi `GITHUB_FILE_VERBS` | `FAIL: the own-repository URL rule checked 0 URLs, below its floor of 3 — crates/library/README.md alone carries 3, so the rule has stopped matching` | 5 |

Mọi đảo chiều hoàn lại trước commit; `git diff` sau khi hoàn lại chỉ còn thay đổi của bước.

**Không chạy** `benches/alloc.rs`, bộ Criterion, `tools/w2w`, hai script bất biến 4, 59 định
nghĩa như một gate riêng: không bước nào đụng đường nóng, session logic, wait strategy hay
readiness (`CLAUDE.md` §7 — mở rộng phạm vi là nêu thêm case). `cargo test --all` và `cargo test
--no-default-features` vẫn chạy mỗi commit.

## Tài liệu phải cập nhật

Đi qua bảng `CLAUDE.md` §4 từng hàng:

- [ ] **Giá trị/khóa cấu hình người dùng thấy** → `docs/CONFIGURATION.md` §1, ô `ReconnectInterval` (bước 3)
- [ ] **Cổng hoặc cách đo cổng** → `DESIGN.md` §6: dòng 887 cờ rustdoc (bước 2), dòng 886 probe 7 ≥ 5 và chân boolean của probe 3 (bước 3, 4), dòng 916 quy tắc (d) của `check-links` (bước 5)
- [ ] **Đảo một quyết định / đổi kỹ thuật** → ADR-0066 (đã viết, `Proposed`), dòng Status ADR-0065 (bước 2)
- [ ] **Bẫy / bất ngờ đo được** → `docs/reference/a-bare-filename-is-not-evidence-of-a-repository.md` mục mới: quy tắc (d) và lỗ README đếm URL sai đường là "checked" (bước 5); lệnh trong `a-linux-only-module-is-invisible-to-a-mac-gate.md` (bước 2). Bất ngờ của item 83 (lint warn in mỗi lần, không ai đọc) nằm trong *Context* của ADR-0066 kèm số đo
- [ ] **Chứng minh điều từng ghi là chưa chứng minh** → `STATUS.md` *Not proven*: không bullet nào về 80–83 (đã grep `check-links`, `rustdoc`, `probe 7`, `ReconnectInterval` khi đóng — manager làm lại ở bước 7); **thêm** bullet phần dư của item 80
- [ ] `STATUS.md` hàng 80–83 đóng, *Start here* mới có run id (bước 7)
- [ ] Không áp dụng, đã xét: `PRD.md` (không đổi phase), `GUIDE.md` (không ràng buộc mới cho người nhúng), public API / `CHANGELOG.md` (chỉ doc comment, không đổi chữ ký), `SESSION-BEHAVIOUR.md` (không đổi hành vi session), `CONFORMANCE.md` (không đổi số conformance), `best-practices-*`, `hft-playbook.md`, `README.md` layout

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Tính hoa/thường của `SPELLINGS` bằng cắt chuỗi (`lower[1..]`, như bản sao đo đã làm) vi phạm `indexing_slicing = "deny"` | viết 22 chuỗi nguyên văn; `cargo clippy --all-targets -- -D warnings` |
| `SPELLINGS` gây đỏ giả trên hàng chỉ có khi bật `tls` (`SocketUseSSL`, `TlsRequireKernel`) — bàn không build được `tls` | job CI `cargo test -p fixbolt-engine --tests --features tls`; manager trích `11 probed` từ log trước khi coi bước 4 là xong |
| Đổi predicate nhánh `positive` làm nó quá dễ dãi (một từ chối bất kỳ cũng tính) | R81-2: parser nhận `0` → đỏ với `None`. Mọi file mẫu hôm nay có baseline `None` (comment `settings.rs:3436-3438`), nên "khác baseline" = "có từ chối" |
| Quên nâng `FLOOR` của probe 7 → hàng mới có thể biến mất im lặng | R81-3 |
| Sửa `is_about_the_value` thay vì predicate của probe 7 → đổi hành vi probe 3 | số probe 3 trích ở bước 3 phải y hệt số gốc: `25 call sites, 8 presence-only` · `10 probed, 23 skipped` |
| README thư viện: URL sai đường nhưng đuôi có thật được đếm "checked" | R82-2 |
| Quy tắc (d) tắt im lặng (đổi tên verb, sai chỉ số đoạn) | sàn 3, R82-6 |
| `isfile` thay `exists` → mọi link `tree/` tới thư mục đỏ giả | R82-3 |
| Tên cũ `nanofixengine` dưới owner của repo lọt qua (d) | R82-4 |
| `-D warnings` đỏ trên một bộ Linux-only mà bàn không build | lệnh Linux cross-target 32 bộ chạy ở bước 2 **sau** bước 1, trích `EXIT=0` |
| Bước 2 lên CI trước bước 1 → job đỏ vì chính item 83 | thứ tự phụ thuộc trong *Chia việc*; R83-1 là đỏ đó, thấy trên bàn trước |
| Bước 1 lén đổi code của `session` | `git diff crates/session` chỉ gồm dòng `///` |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Phần dư item 80 bị đọc thành "đã đóng hẳn" | Trung bình | Doc của test và bullet *Not proven* nêu `always` đã đo xanh; hàng 80 trong `STATUS.md` ghi "closed, with a residue", không ghi "closed" trơn |
| Nâng toolchain sau này thêm một lint rustdoc mức warn → job `feature-sets` đỏ không do commit nào | Thấp | Toolchain ghim trong `rust-toolchain.toml`; đỏ nêu tên lint; ghi trong ADR-0066 *Consequences* |
| Senior review tìm ra lỗ tiếp theo trong quy tắc (d) (URL mã hoá, `?plain=1`, `#L10`) | Trung bình | `#` và `?` đã bị bỏ trước khi tách đoạn (`check-links.py:160`), `unquote` xử lý `%`; finding mới đi về senior dev theo §12, cái không đáng sửa thì ghi vào docstring như (b), (c) đã làm |

## Ngoài phạm vi

- URL của chính repo **không** có dạng `blob|tree|raw|blame/<ref>/…` mà trỏ tới đường dẫn không
  có (`github.com/tmthang86/fixbolt/DESIGN.md` — không có `blob/main`). Muốn bắt cần danh sách
  route của GitHub, danh sách đó sẽ cũ. Được đếm trong `not file links (not judged)`, không bỏ im.
- Link `tree/` của chính repo tới **thư mục có thật** ngoài README vẫn không bị báo "dùng đường dẫn
  tương đối" — tìm đuôi đòi đoạn cuối có dấu `.`, có từ trước, không thuộc item nào ở đây.
- Ref nhiều đoạn (`plan/x`): đỏ, là giới hạn đã nêu, không đoán nhánh.
- Lint rustdoc mức **allow** (`missing_docs`, `unescaped_backticks`, …): không bật.
- Nhánh khoảng của probe 7 vẫn dùng `is_about_the_value`.
- Cách viết boolean ngoài YAML 1.1 (`yEs`, `enable`, `1`/`0` đã có sẵn trong tập 1 ký tự): không
  thêm; phần dư ghi rõ.
- Không đổi `Policy`, không đổi parser, không đổi hành vi nào người dùng thấy ngoài một ô tài liệu.

## Nhật ký giao hàng

_Chưa bắt đầu — plan đang chờ duyệt._

**Đây là phần sống sót qua nén context** — phiên sau đọc mục này trước tiên.
