# Đưa bộ test TLS vào CI, và làm cho "máy không chạy được" không giả dạng "engine hỏng"

> **Loại:** Plan · **Ngày:** 2026-09-10 · **Trạng thái:** Chờ duyệt
> **Phạm vi:** `STATUS.md` item 62. Chạm `.github/workflows/ci.yml`, `scripts/`,
> `crates/engine/tests/tls.rs`, docs. **Không chạm** `crates/*/src` — không một dòng code
> engine nào đổi.
>
> **Thời lượng dự kiến:** nửa ngày. Bước 1 cần một vòng push–đọc-log của CI; các bước sau
> làm được ở máy.

## Bối cảnh

`[measured 2026-09-10]` **không job CI nào từng chạy một test TLS.** `ci.yml` truyền
`--features affinity` cho bộ test shard, và `--all-features` cho `cargo deny` và (từ item 61)
`cargo doc` — nhưng **không chỗ nào truyền `--features tls` cho `cargo test`**.

Hệ quả: `crates/engine/tests/tls.rs` (6 test, merge trong PR #57 ngày 2026-09-09) và
`crates/engine/tests/tls_wire.rs` (bước 4a) **chỉ từng chạy trên một cái bàn** — máy của chủ
repo. Mọi PR xanh kể từ #57 đều xanh về một feature mà nó không compile lấy một test.

**Tìm ra bằng cách đọc log của một run XANH**, không phải bằng một run đỏ: `grep` tên bất kỳ
test TLS nào trong toàn bộ log job của `20a3282` trả về **0**, trong khi `interop`, `bench` và
`deny` đều in dòng riêng của chúng. Bảng check ghi 26/26.

Đây là **item 61 lùi ra một lớp**. Ở item 61, gate `rustdoc` không thấy một *file* bị feature
che. Ở đây, **toàn bộ bộ test của một feature nằm ngoài mọi gate**, và không có gì trong CI
nói ra điều đó.

**Vì sao nó không được sửa ngay trong commit tìm ra nó:** một trong sáu test cũ khẳng định
`/proc/net/tls_stat` có dịch chuyển. Trên kernel không có `CONFIG_TLS`, test đó **đỏ chứ không
skip**, và runner của GitHub có kTLS hay không thì ở đây **không ai biết**. Chọn giữa các cách
xử lý là một quyết định, và `CLAUDE.md` §1 nói quyết định thì phải có plan. Đây là plan đó.

## Những gì đã biết chắc

| Sự thật | Nguồn |
|---|---|
| `ci.yml` không truyền `--features tls` cho `cargo test` ở bất kỳ job nào | `.github/workflows/ci.yml`, `grep -n 'features'` — chỉ có `affinity` (dòng 270-272) và `--all-features` (dòng 97, 126, 192, 194) |
| Tên test TLS xuất hiện **0 lần** trong log run xanh của `20a3282` | `gh run view 34489514617 --log`, `[measured 2026-09-10]` |
| `tls.rs` có **6** test; `tls_wire.rs` có **1** | `cargo test -p fixbolt-engine --features tls --test tls` → `6 passed`; `--test tls_wire` → `1 passed`, `[measured 2026-09-10]` |
| **Đúng một** trong bảy test cần kernel: `after_the_handover_the_kernel_holds_the_keys_and_read_returns_plaintext` assert `tls_stat("TlsTxSw") > before_tx` và `TlsRxSw` tương tự | `crates/engine/tests/tls.rs:371-378` |
| `tls_stat()` trả **0** khi không đọc được `/proc/net/tls_stat`, nên trên kernel không có `CONFIG_TLS` assertion đó **đỏ**, không skip | `crates/engine/tests/tls.rs:266-268` |
| Sáu test còn lại **không cần kTLS**: bắt tay, hang-up giữa chừng, `Logon` ngay sau `Finished`, `keeps_the_hot_path`, fallback userspace, và `serve_tls` qua socket thật — cái cuối phục vụ được qua fallback | đọc từng test, `crates/engine/tests/tls.rs`, `tls_wire.rs` |
| `scripts/check-ktls-available.sh` trả lời "máy này chạy được kTLS không" trong một lệnh, và có chế độ `KTLS_SOURCE_ONLY=1` để script khác dùng lại phần phân loại | `scripts/check-ktls-available.sh:22-24` |
| `scripts/check-ktls-classify.sh` **đã chạy trong CI** ở job `script-logic`, nhưng nó kiểm **logic phân loại**, không kiểm kernel — comment của job nói thẳng *"on a runner whose own kernel state is irrelevant to the result"* | `.github/workflows/ci.yml:43-61` |
| **Runner của GitHub có kTLS hay không: CHƯA BIẾT.** Không có số đo nào trong repo này trả lời được | — |
| Idiom skip của repo là **`exit 2`, không phải pass** — `check-ktls-on-a-plain-socket.sh` in `SKIPPED, NOT PASSED` rồi `exit 2` | `scripts/check-ktls-on-a-plain-socket.sh:26-28, 57-59` |
| **Một `#[test]` của Rust không có `exit 2`.** Nó chỉ pass, fail, hoặc `#[ignore]`. Một test skip **trông y hệt** một test pass trong output | `cargo test` — hành vi của harness |

## Cách làm

**Nguyên tắc dẫn đường, và nó là lý do plan này tồn tại:** *một môi trường không chạy được
phải hỏng khác một engine hỏng.* Hôm nay hai thứ đó cho ra cùng một dòng đỏ —
`tls_stat TlsTxSw did not move` — và dòng đó đổ lỗi cho engine trong khi lỗi là kernel không có
module. Đó đúng là hình dạng `CLAUDE.md` §10 gọi là *một cause được nhận vì có cái knob nhúc
nhích cùng nó*.

Nên **điều kiện môi trường được kiểm ở một chỗ riêng, hỏng trước và hỏng bằng câu nói riêng của
nó**, chứ không để nó lộ ra dưới dạng một assertion về `tls_stat`.

1. **Bước 1 là ĐO, không phải sửa.** Thêm tạm một bước CI in kết quả
   `scripts/check-ktls-available.sh` trên runner, push, đọc log. **Plan này không giả định câu
   trả lời**, và các bước sau rẽ theo nó.
2. **Một job `tls` riêng trong `ci.yml`**, không nhét vào job `fmt · clippy · test` đang có.
   Lý do: job đó chạy nhiều lệnh và một job hỏng vì kernel sẽ làm mờ một job hỏng vì code.
   Job mới chạy `cargo clippy --all-targets --features tls -- -D warnings` và
   `cargo test -p fixbolt-engine --features tls`.
3. **Bước đầu tiên của job đó là `scripts/check-ktls-available.sh`**, và nó **phải in verdict
   `READY`**. Nếu runner không có kTLS, job đỏ **ngay ở bước đó**, với câu chữ của chính script
   nói về kernel — chứ không đỏ ở một assertion về `tls_stat` bốn phút sau.
4. **Nếu bước 1 nói runner KHÔNG có kTLS**, phương án là: job vẫn tồn tại, vẫn chạy sáu test
   không cần kernel, và **test cần kernel bị `#[ignore]` với lý do viết tại chỗ** — cộng thêm
   một dòng trong job in ra rằng nó đang bỏ qua cái gì và vì sao. **Không được để test đó
   lặng lẽ pass**: một test skip trông y hệt một test pass, đó là cả vấn đề, nên chỗ duy nhất
   nó được phép biến mất là kèm một câu in ra tên nó.
5. **`docs/CONFORMANCE.md` nói job này canh cái gì và không canh cái gì**, kèm id run CI.

File sửa: `.github/workflows/ci.yml`, `crates/engine/tests/tls.rs` (chỉ khi bước 4 áp dụng),
`docs/CONFORMANCE.md`, `STATUS.md`. **Không file nào trong `crates/*/src`.**

## Bất biến bị đụng tới

- **Bất biến 6 — feature gate.** Đây chính là bất biến plan này phục vụ, ở dạng ít ai nghĩ tới:
  `#[cfg]` trên `mod` là đúng và đã đúng, nhưng **không gate nào từng compile cái `mod` đó với
  feature bật**. Job mới là nửa còn thiếu. `check-no-optional-deps.sh` vẫn phải xanh cả 10
  case — feature `tls` vẫn phải tắt mặc định.
- **Bất biến 10 — số phải có máy.** Plan này **không công bố số nào**. Job mới là gate hành vi;
  không có figure nào từ nó được vào `DESIGN.md` §8.
- **Bất biến 4.** Không đụng. Job này **không** nói gì về engine thread dưới TLS — cái đó là
  arm TLS của `check-no-kernel-sleep.sh`, thuộc bước 6 của plan `tls`, vẫn chưa làm.
- Các bất biến còn lại: không đụng, vì không dòng `crates/*/src` nào đổi.

## Chia việc

| Bước | Kết quả | Phụ thuộc | Máy |
|---|---|---|---|
| 1 | Runner có kTLS hay không, **đo được, in ra log** | — | **CI** |
| 2 | Job `tls` mới: clippy + test dưới `--features tls` | 1 | CI |
| 3 | Điều kiện kernel hỏng **trước** và hỏng bằng câu của chính nó | 2 | CI |
| 4 | Nếu runner không có kTLS: test cần kernel `#[ignore]` + in tên nó ra | 1, 3 | CI |
| 5 | `CONFORMANCE.md` + `STATUS.md` item 62 đóng, kèm id run | 2-4 | bất kỳ |

## Cách kiểm chứng

- **Bước 1 là bằng chứng, không phải giả định.** Đọc verdict trong log, không suy từ việc
  ubuntu-latest *thường* có gì.
- **Đảo chiều cho bước 3, và đây là đảo chiều quan trọng nhất của plan:** giả lập một runner
  không có kTLS (đặt biến môi trường ép `check-ktls-available.sh` ra verdict khác, hoặc chạy
  script với input phân loại giả như `check-ktls-classify.sh` đã làm) và xác nhận job **đỏ ở
  bước môi trường**, với câu chữ về kernel, **chứ không phải** ở `tls_stat did not move`. Nếu
  nó vẫn đỏ ở `tls_stat` thì bước 3 chưa xong.
- **Đảo chiều cho bước 2:** bỏ `--features tls` khỏi job và xác nhận số test tụt về 0 cho hai
  file đó — tức là job thật sự đang chạy chúng chứ không phải chạy lại bộ mặc định. Cùng hình
  dạng với cặp số `1 passed` / `0 passed` mà plan `hft` phải in ra cả hai.
- **Đọc log, không đọc dấu tick.** Tiêu chí đạt là **tên từng test TLS xuất hiện trong log
  CI** — đúng cái `grep` trả về 0 hôm nay.

## Tài liệu phải cập nhật

- [ ] `docs/CONFORMANCE.md` — job mới canh gì, không canh gì, id run CI
- [ ] `STATUS.md` item 62 — đóng, kèm id run
- [ ] `docs/reference/a-doc-gate-never-opened-the-file-it-was-guarding.md` — thêm case anh em
      này vào mục *Sibling cases*, vì nó là cùng một câu ở một lớp khác
- [ ] `CHANGELOG.md` — **không**. Không có thay đổi API hay hành vi nào của crate

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| **Test skip trông y hệt test pass.** Nếu bước 4 áp dụng và không in gì ra, CI lại xanh về thứ nó không chạy — đúng cái bug này | Bước 4 bắt buộc in tên test bị bỏ qua; tiêu chí đạt của bước 2 là *tên test xuất hiện trong log* |
| **Job đỏ vì kernel bị đọc thành engine hỏng** | Bước 3 và đảo chiều của nó — điều kiện môi trường phải hỏng trước, bằng câu của nó |
| **`--features tls` bật lây sang crate khác qua workspace** — hình dạng đã cháy một lần, `docs/reference/feature-flags-unify-across-a-workspace.md` | `check-no-optional-deps.sh` hỏi **theo từng crate** và vẫn phải xanh cả 10 case |
| **Thêm job nhưng nhét vào job cũ**, làm một lỗi kernel mờ đi sau một lỗi code | Bước 2 nói rõ là job riêng |
| **Sửa `tls.rs` để nó xanh trên runner** thay vì để nó nói thật | `CLAUDE.md` §10: *một fixture bị sửa để việc mới pass là failure mode phải canh* — không assertion nào của sáu test cũ được nới |
| **Đóng item 62 mà chưa có run xanh cho đúng commit** | §9 hộp cuối: id run được ghi cho merge commit, kiểm sau khi merge |

## Ngoài phạm vi

- **Arm TLS cho `check-no-kernel-sleep.sh`** — bước 6 của plan `tls`, và nó đo mode chứ không
  đo hành vi.
- **Bất cứ số §8 nào.** Job này không đo gì.
- **`interop.sh` qua TLS.** Đáng làm, nhưng cần `libquickfix` phía kia nói TLS, và đó là một
  plan khác.
- **Hai warning rustdoc còn lại trong `fixbolt-session`** (`parse_utc` → `LEN_SECONDS`,
  `LEN_MAX`). Cũ hơn item 61, là warning chứ không phải error, để nguyên.

## Nhật ký giao hàng

*(chờ duyệt — chưa bắt đầu.)*
