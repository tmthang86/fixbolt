# Ba cái gate mà một engine anh em có, còn cái này thì không

> **Loại:** Plan · **Ngày:** 2026-09-07 · **Trạng thái:** **Đã đóng 2026-09-08**, đủ cả năm bước
> **Phạm vi:** hạ tầng gate — lint, CI, và tầng đọc socket của `tools/interop`

> **Duyệt nguyên văn, đủ cả năm bước** — bước 4 giữ nguyên trong phạm vi.
> **`[quyết định 2026-09-08]` việc này chạy trên máy Linux, không trên laptop.** Đó là câu trả lời
> cho ô §9 "hot-path đo trên Linux": số của bước 4 là số Linux ngay từ đầu, không phải số laptop
> chờ đo lại sau.

## Bối cảnh

Ngày 2026-09-07 khảo sát [TrueFix](https://github.com/truefix-labs/truefix) (bản local
`/Users/tranmanhthang/Projects/truefix`, Apache-2.0 OR MIT), một FIX engine Rust khác, để xem có gì
học được — câu hỏi ban đầu là về black-box conformance harness của họ.

**Về conformance thì không có gì để vay.** Harness của họ (`crates/truefix-at`, 606 dòng runner +
2 364 dòng scenario viết tay) yếu hơn `crates/conformance` ở đây trên ba trục: oracle của họ là
scenario tự viết còn ở đây là corpus `.def` thật của QuickFIX; comparator của họ mặc định chỉ khớp
vài tag và chế độ chặt là opt-in, còn ở đây so positional và bắt buộc bằng số field; và họ không có
reversal nào chứng minh harness biết fail — ở đây có `NullSession` (0/59) với `Replay` (59/59).

**Nhưng khảo sát tìm ra ba lỗ ở phía repo này**, và một trong ba là bug thật đã nằm sẵn trong code.
Plan này bịt cả ba. Không cái nào đụng vào hot path.

## Những gì đã biết chắc

**Lỗ 1 — `tools/interop` gộp ba kết cục socket khác nhau làm một.**
`tools/interop/src/main.rs:231` viết `Ok(0) | Err(_) => return None`. Peer đóng socket sạch sẽ, hết
`READ_TIMEOUT` (đặt ở `:499`), và lỗi socket thật — cả ba cùng trả `None`. Một scenario fail vì
engine chết sẽ in ra đúng một thông điệp như fail vì engine im lặng. TrueFix tách năm nhánh
(`ReadMessageOutcome`, `crates/truefix-at/src/runner.rs`) và có meta-test
`tests/read_message_three_outcomes.rs` chứng minh ba nhánh phân biệt được.

**Lỗ 2 — `clippy::indexing_slicing` không bật, và nó chính là lint đáng ra bắt được finding hai của
plan `seq-resync-789-369`.** `Cargo.toml` `[workspace.lints.clippy]` hiện có đúng ba lint
restriction: `unwrap_used`, `expect_used`, `panic`. Không cái nào thấy `buf[a..b]`. STATUS.md ghi
bug đó: record layout của ring dispatch thêm một field, `fixed[OUT_SEQ..OUT_LAST_PROCESSED]` đi từ
bốn byte lên năm trong im lặng, `copy_from_slice` **panic trong một library crate** — đúng chủ đề
của bất biến 7, và vô hình với ba lint đang có. Chỗ đó nay đã sửa thành `start..start + LEN`
(`crates/engine/src/dispatch.rs:273,275,381,385`) nhưng **không có gì canh để nó không tái diễn ở
chỗ khác**.

`[đo 2026-09-07]` `cargo clippy --all-targets -- -W clippy::indexing_slicing`, đếm trong `crates/*/src/`:
**207 site — 82 `slicing may panic`, 125 `indexing may panic`.** Phân bố:

| File | slicing | tổng |
|---|---|---|
| `crates/session/src/lib.rs` | 16 | 42 |
| `crates/codec/src/template.rs` | 7 | 32 |
| `crates/dict/src/field_type.rs` | 8 | 22 |
| `crates/engine/src/lib.rs` | 0 | 21 |
| `crates/engine/src/journal.rs` | 14 | 18 |
| `crates/conformance/src/runner.rs` | 8 | 13 |
| `crates/engine/src/msglog.rs` | 4 | 10 |
| 14 file còn lại | 25 | 49 |

Ngoài `src/` còn khoảng 400 site nữa trong `tests/`, `benches/`, `tools/` — bất biến 7 nói về
library crate nên chúng không thuộc phạm vi.

**Lỗ 3 — không có gate nào về license hay advisory của dependency.** `deny.toml` không tồn tại,
`cargo-deny` chưa cài (`cargo deny --version` → `no such command`). Repo này **sắp open-source**
(CLAUDE.md, dòng đầu), và một dependency copyleft đi vào theo đường transitive là vấn đề thật chứ
không phải giả định. TrueFix có `deny.toml` với thói quen đáng lấy: mỗi `ignore` phải viết lý do
tại sao đường code đó không với tới được.

**Về license khi vay:** TrueFix là Apache-2.0 OR MIT. Chép code là hợp pháp nhưng Apache-2.0 kéo
theo nghĩa vụ `NOTICE` — đúng thứ ADR-0001 tránh với QuickFIX. **Plan này vay hình dạng, viết lại
từ đầu, không chép dòng nào.**

## Cách làm

Ba việc độc lập, một branch, làm theo thứ tự rẻ-trước.

**Bước 1 — `cargo-deny`.** Thêm `deny.toml` ở gốc: `[licenses]` chỉ cho phép các license permissive
tương thích với thế phát hành của repo; `[advisories] yanked = "deny"`; `[bans] multiple-versions =
"warn"`. Thêm một CI job chạy `cargo deny check`. Mọi `ignore` phải kèm lý do viết bằng chữ.

**Bước 2 — tách kết cục đọc socket của `interop`.** `tools/interop/src/main.rs` có một enum mới
thay cho `Option<Vec<u8>>`: message đọc được / hết giờ / peer đóng sạch / lỗi socket. Mỗi nhánh in
ra một câu khác nhau. Kèm một test chứng minh ba nhánh không lẫn nhau, dựng bằng `TcpListener` cục
bộ như cách TrueFix làm — viết lại, không chép.

**Bước 3 — bật `indexing_slicing` bằng cơ chế trần, không bằng lời hứa.**
`[workspace.lints.clippy]` thêm `indexing_slicing = "deny"`. 21 file `src/` đang vi phạm nhận
`#![allow(clippy::indexing_slicing)]` kèm một dòng comment ghi số site của nó vào ngày này.

Cái làm cho bước này là gate chứ không phải thủ tục: `scripts/check-indexing-debt.sh` chạy clippy
với `--force-warn clippy::indexing_slicing` — cờ này **đè lên cả `allow`** — đếm số warning trong
`crates/*/src/`, và **fail nếu con số vượt trần đang ghi trong script**. Trần chỉ được đi xuống.
Code mới trong file sạch bị `deny` chặn ngay; code mới trong file đang nợ vẫn làm tăng số đếm và
bị trần chặn. CI chạy script này.

**Bước 4 — hạ trần lần đầu, ở chỗ rủi ro nhất.** Dọn `crates/engine/src/journal.rs` (14 slicing
trên 18 site — mật độ slicing cao nhất repo) và `crates/engine/src/dispatch.rs` (nơi bug đã xảy ra,
5 site). Gỡ `allow` của hai file đó, hạ trần theo số thật.

**Bước 5 — tài liệu.**

File sẽ tạo: `deny.toml`, `scripts/check-indexing-debt.sh`,
`docs/reference/three-outcomes-collapsed-into-one-none.md`.
File sẽ sửa: `Cargo.toml`, `.github/workflows/`, `tools/interop/src/main.rs`,
`crates/engine/src/journal.rs`, `crates/engine/src/dispatch.rs`, 21 file `src/` nhận `allow`,
`CLAUDE.md` §2, `STATUS.md`.

## Bất biến bị đụng tới

**Bất biến 7 (no `panic`/`unwrap`/`expect` trong library crate)** — đây là bất biến mà cả plan phục
vụ. Bước 3 biến một phần của nó từ prose thành máy đọc được. Giữ nguyên bằng chính
`scripts/check-indexing-debt.sh`, chứng minh bằng reversal.

**Bất biến 1 (không cấp phát trên hot path)** — bước 4 sửa code trong `engine`. Thay `a[i..j]` bằng
`a.get(i..j)` trả `Option`, xử lý nhánh `None` bằng enum lỗi không field. **Không `format!`, không
`String`, không cấp phát trên nhánh lỗi.** Canh bằng `benches/alloc.rs` chạy lại sau bước 4.

**Bất biến 10 (không có số hiệu năng nào không kèm benchmark)** — bước 4 đụng `journal.rs` và
`dispatch.rs`, cả hai đều nằm trên đường dispatch. `benches/dispatch.rs` phải chạy lại và số phải
được ghi, hoặc phải nói rõ là không đo.

Không đụng `codec` lẫn `session` ở bước 4.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | `deny.toml` + CI job; `cargo deny check` xanh, mỗi `ignore` có lý do viết ra | — |
| 2 | `interop` phân biệt bốn kết cục đọc; meta-test chứng minh ba nhánh không lẫn | — |
| 3 | `indexing_slicing = "deny"` + 21 `allow` + `check-indexing-debt.sh` với trần 207 | — |
| 4 | `journal.rs` và `dispatch.rs` sạch; trần hạ xuống theo số thật | 3 |
| 5 | Tài liệu đồng bộ, write-up `[to testing-skills]` cho lỗ 1 | 1–4 |

## Cách kiểm chứng

**Bước 1.** Chạy `cargo deny check`, đọc output đầy đủ chứ không đọc exit code. Reversal: thêm tạm
một license không nằm trong allow-list vào `deny.toml` dưới dạng một dep giả, thấy nó đỏ, gỡ ra,
thấy xanh. Nếu không dựng được reversal thì nói thẳng là gate này chưa được chứng minh.

**Bước 2.** Meta-test phải **đỏ trước** khi code sửa: viết test trên enum chưa tồn tại, chụp lại
output compile-fail, rồi mới viết enum. Sau đó chạy `scripts/interop.sh` đủ sáu scenario, số phải
giữ nguyên `7/7 + 8/8 + 6/6 + 6/6 + 6/6 + 9/9` — bước này đổi cách báo lỗi, không đổi hành vi, nên
điểm đổi là dấu hiệu sai.

**Bước 3.** Reversal của `check-indexing-debt.sh`: thêm một dòng `let _ = v[0];` vào một file `src/`
bất kỳ, chạy script, **thấy nó fail vì vượt trần**, gỡ ra, thấy xanh. Không có reversal này thì
script chỉ là một lệnh chạy được chứ chưa phải gate. Kiểm riêng rằng `--force-warn` thật sự đè lên
`#![allow]` — đếm trước và sau khi thêm `allow` vào một file, hai số phải bằng nhau.

**Bước 4.** `cargo test --all` và `cargo test --all --no-default-features`, đọc số test chứ không
đọc exit code. `crates/engine/benches/alloc.rs` phải 0 trên mọi case. `benches/dispatch.rs` chạy
lại, số ghi vào nhật ký giao hàng kèm máy đã chạy. 59 acceptance definitions phải giữ 59/59.

**Toàn bộ.** Một CI run xanh, gọi tên bằng id, cho đúng commit đóng plan — Definition of Done ô cuối.

## Tài liệu phải cập nhật

- [x] `CLAUDE.md` §2, danh sách "Machine-checked today": bất biến 7 nay có thêm
      `scripts/check-indexing-debt.sh`, ghi rõ nó canh cái gì và trần là bao nhiêu
- [x] `STATUS.md` mục Open items: ba item mới **54** (interop gộp kết cục), **55** (nợ
      `indexing_slicing`, kèm trần hiện tại), **56** (chưa có gate license/advisory) — item 55 và
      56 đóng ngay trong plan này, 54 cũng vậy
- [x] `docs/reference/three-outcomes-collapsed-into-one-none.md` — write-up lỗ 1, đánh dấu
      **`[to testing-skills]`** (§11: đây là "một gate pass hoặc fail vì lý do khác với thứ đang
      được kiểm", đúng hàng `references/false-greens.md`)
- [x] `docs/reference/` — nếu bước 4 lộ ra thêm bẫy nào thì viết vào, ưu tiên cao nhất theo §4
- [x] `CHANGELOG.md` nếu bước 4 đổi public API của `engine` (dự kiến không)

**Không cần cập nhật:** `DESIGN.md` (không thêm/bớt crate, không đổi public API), `PRD.md`,
`CONFORMANCE.md` (không có số conformance nào đổi), `docs/best-practices-*.md`.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| `check-indexing-debt.sh` chạy được nhưng không bao giờ đỏ | Reversal ở bước 3: thêm một `v[0]`, phải thấy fail |
| `--force-warn` hóa ra không đè `#![allow]`, trần đếm nhầm | Đếm hai lần quanh việc thêm một `allow`, hai số phải bằng |
| Sửa `journal.rs` làm phát sinh cấp phát trên nhánh lỗi | `crates/engine/benches/alloc.rs`, 0 trên mọi case |
| Sửa `journal.rs` làm chậm đường dispatch | `benches/dispatch.rs`, so với số đã ghi |
| `interop` đổi cách báo lỗi rồi vô tình đổi hành vi | `scripts/interop.sh` sáu scenario, điểm phải y nguyên |
| `cargo deny` xanh vì nó không đọc crate nào (config sai `[graph]`) | Đọc output, đếm số crate nó báo đã duyệt, so với `cargo tree` |
| 21 cái `allow` biến thành vĩnh viễn | Trần trong script chỉ đi xuống; item 55 ở `STATUS.md` mở cho tới khi trần về 0 |
| CI job mới xanh vì nó không chạy | Đọc log của job, thấy dòng lệnh và output thật — §9 đã có tiền lệ |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| 207 site là nợ lớn, cơ chế trần có thể bị hiểu là đã xong | Vừa | `STATUS.md` item 55 nói rõ trần hiện tại và rằng nó chưa về 0; CLAUDE.md §2 ghi bất biến 7 là *một phần* máy canh |
| Nhiều trong 125 site `indexing` là index hằng trên mảng cố định, sửa hết chỉ thêm nhiễu | Vừa | Bước 4 chỉ dọn hai file, ưu tiên **slicing** — đúng lớp bug đã xảy ra. Phần còn lại để plan sau, có số dẫn đường |
| `cargo deny` đỏ ngay lần chạy đầu vì một dep transitive | Thấp | Nếu xảy ra: viết lý do vào `ignore`, hoặc bỏ dep. Không im lặng nới allow-list |
| `cargo-deny` phải cài trên máy CI, thêm thời gian build | Thấp | Dùng action có cache; nếu chậm quá thì tách job chạy song song |
| ~~Bước 4 đụng đường dispatch mà chỉ đo trên Mac~~ | — | **Đã gỡ `[quyết định 2026-09-08]`**: việc chạy trên máy Linux, nên số của bước 4 là số Linux ngay từ đầu. Vẫn phải ghi rõ máy nào và §9 settings nào đang bật, theo bất biến 10 |

## Ngoài phạm vi

- **Dọn hết 207 site.** Chỉ dọn `journal.rs` và `dispatch.rs`. Phần còn lại là plan khác.
- **`indexing_slicing` trong `tests/`, `benches/`, `tools/`** (~400 site). Bất biến 7 nói về library
  crate.
- **Ma trận nhiều phiên bản FIX** như TrueFix có (9 phiên bản). Repo này cố ý chỉ FIX 4.4; nếu PRD
  mở thêm phiên bản thì hình dạng ma trận của họ đáng đọc lại lúc đó.
- **`SessionTweaks`** — bật/tắt feature acceptor theo từng scenario. Ý hay, nhưng `.def` corpus
  không cần nó và `interop` hiện đủ dùng.
- **Mọi thứ khác của TrueFix**: async Tokio engine, message store (SQL/MongoDB/redb), broker SDK
  (Futu/IBKR/OKX/IG), FAST/SBE. Đối nghịch trực tiếp với bất biến 1 và 2.
- **Chép code từ TrueFix.** Vay hình dạng, viết lại. Xem mục "Về license khi vay".

## Nhật ký giao hàng

*(chưa bắt đầu — plan đang chờ duyệt)*


---

## Đã làm xong — 2026-09-08

**Cả năm bước, chạy trên máy Linux của chủ dự án** (AMD Ryzen 7 3700X, Linux 7.0.0-31),
`scripts/check-machine.sh` **10 pass / 2 fail / 1 unknown** — hai cái fail là `isolcpus` và
C-states, cả hai cần sửa kernel command line rồi reboot, ngoài phạm vi plan này.

`cargo test --all` **589 passed, 0 failed** (585 trước đó, +4 test mới của bước 2);
`--no-default-features` **584**; 59 acceptance definitions **59 / 59**
(`score.rs` assert thẳng `report.passed == 59`); `benches/alloc.rs` **0 trên cả 30 case**;
`benches/dispatch.rs` nằm trong band ở cả ba case.

**Ba phát hiện mà plan không lường trước, cả ba đều đã viết ra:**

1. **`cargo-deny` không nhìn thấy dev-dependency.** Reversal đầu tiên dùng dev-dependency và
   xanh — trông như reversal hỏng. `STATUS.md` item 57, đóng bằng cách so số crate với
   `cargo tree` trong CI chứ không vá được ở tầng công cụ.
2. **`#![allow]` ở đầu `lib.rs` là inner attribute, tắt lint cho cả crate.** Nửa `deny` của
   ratchet chết ở `engine`, `session`, `dict` trong khi bộ đếm vẫn đúng. `STATUS.md` item 58.
   Đã đổi sang `#[allow]` phạm vi từng hàm, 15 hàm.
3. **Số file nhận `allow` là 81 chứ không phải 21.** `[workspace.lints]` với tới cả `tests/`,
   `benches/` và `tools/`; 21 file `src/` là phần trong phạm vi và là phần trần đếm, 60 file
   còn lại nhận `allow` kèm một câu nói rõ chúng nằm ngoài bất biến 7.

**Không làm được ở đây:** `scripts/interop.sh` không chạy trên desk này vì thiếu `cmake`
(script `exit 1` đúng — cái nuốt exit code là `| tail` trong lệnh gọi, đúng bài
`reading-the-output-you-grepped-for`). Yêu cầu "sáu scenario giữ nguyên điểm" của bước 2 do
job `interop` trong CI gánh, ở đúng commit đóng plan, và không có gì khác gánh.

**Trần `indexing_slicing`: 207 → 188.** Sàn không phải 0 — xem ghi chú cạnh `CEILING` trong
`scripts/check-indexing-debt.sh`.
