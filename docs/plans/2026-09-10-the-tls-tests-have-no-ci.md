# Đưa bộ test TLS vào CI, và làm cho "máy không chạy được" không giả dạng "engine hỏng"

> **Loại:** Plan · **Ngày:** 2026-09-10 · **Trạng thái:** Chờ duyệt (Sửa 1)
>
> **Sửa 1 — 2026-09-12.** Job `tls` do chính kế hoạch này dựng lên đã tìm ra hai test
> đỏ trên runner, và **không bước nào của bảng sở hữu việc đó**. Kế hoạch chuyển từ
> *Xong* về *Chờ duyệt*, thêm bước 6-8. Xem *Sửa 1* ở cuối file. **Phạm vi có thể
> phải mở sang `crates/engine/src/tls.rs`, và chỉ bước 6 mới nói được là có hay không.**
>
> **Phạm vi:** `STATUS.md` item 62 và item 65. Chạm `.github/workflows/ci.yml`, `scripts/`,
> `crates/engine/tests/tls.rs`, docs. **Không chạm** `crates/*/src` — không một dòng code
> engine nào đổi. **Dòng này đúng cho bước 1-5 và là câu hỏi mở của bước 6**: nếu bước 6 kết
> luận (b), phạm vi phải mở và kế hoạch phải được duyệt lại lần nữa trước khi viết code.
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

**2026-09-10 — XONG, và bước 1 làm cho bước 4 biến mất.**

**Bước 1 đo được, và câu trả lời là tin tốt.** `[measured 2026-09-10]` runner của GitHub:

```
kernel: Linux 6.17.0-1022-azure
config: CONFIG_TLS=m
tls_stat: present
module: loaded=yes on_disk=yes
setsockopt(TCP_ULP, "tls"): ACCEPTED
READY
```

Nên **bước 4 không cần làm**: không `#[ignore]` test nào, không phải nghĩ ra cách skip mà không
giả dạng pass, không phải đụng một dòng nào của `crates/engine/tests/tls.rs`. Toàn bộ phần khó
nhất của plan — *một test skip trông y hệt một test pass* — **không phát sinh**, vì đo trước rồi
mới quyết. Nếu làm ngược lại thì đã viết xong cơ chế ignore rồi mới biết là thừa.

**Bước 2 và 3: job `tls` riêng**, không nhét vào `fmt · clippy · test`. Một trong các test khẳng
định `/proc/net/tls_stat` dịch chuyển — đó là khẳng định về **runner**, không phải về engine.
Trộn vào một job chạy sáu thứ khác thì một kernel mất module `tls` sẽ đọc thành *"bộ test hỏng"*.

Bước đầu của job là khẳng định môi trường và **hỏng bằng câu chữ của chính nó**.

**Bước 5: cả hai đảo chiều chạy ở máy, trước khi đẩy.**

| Đảo chiều | Kết quả |
|---|---|
| bỏ `--features tls` khỏi lệnh test | `[measured 2026-09-10]` **cargo exit 0, và 0 test chạy** — đúng hình dạng item 62; có feature thì **11 test**. Guard đếm số bắt được |
| verdict môi trường không phải `READY` | `NOT_BUILT`, `OTHER`, `NOT_LOADED` và chuỗi rỗng đều làm job đỏ ở **bước môi trường**; chỉ `READY` đi tiếp |

**Cái đầu là đảo chiều đáng giá nhất của cả plan**, vì nó chứng minh bằng số rằng một lệnh
`cargo test` xanh mà không compile gì trông **giống hệt** một lệnh xanh thật.

**Không làm:** bước 4 (không cần), arm TLS cho `check-no-kernel-sleep.sh` (bước 6 của plan
`tls`), `interop.sh` qua TLS. **Không số đo nào** ra từ plan này.

## Sửa 1 — 2026-09-12: job này tìm ra hai test đỏ, và không bước nào sở hữu việc sửa

**Kế hoạch được đánh *Xong* ngày 2026-09-10. Nó chưa xong.** Việc của nó vẫn nằm trên nhánh
`plan/tls-4b`, chưa merge, và **PR [#61](https://github.com/tmthang86/fixbolt/pull/61) đang bị
chặn bởi chính cái job mà kế hoạch này dựng lên**. Mục *Ngoài phạm vi* không có dòng nào cho
việc này, bảng *Chia việc* dừng ở bước 5, và *Nhật ký giao hàng* kết thúc bằng "Không số đo nào
ra từ plan này" — nên khi job đỏ, **không có chỗ nào trong kế hoạch nhận việc**. Cùng hình dạng
với *Sửa 3* của plan `tls`: một việc xuất hiện trong văn xuôi rồi không có bước nào sở hữu nó.

Đây là kết quả **đúng và đáng giá** của kế hoạch, không phải thất bại của nó. Mục đích được viết
ra là để một feature không còn nằm ngoài mọi gate; gate vừa bật lên đã tìm ra thứ mà một cái bàn
không thấy được. Nhưng *Xong* là sai trạng thái cho một kế hoạch có gate đang đỏ.

### Cái đã đo, và nó khác với những gì `STATUS.md` item 65 đang ghi

`[measured 2026-09-12, đọc từ log CI]` cùng **một** commit `aa4f46e`, hai run, **kết quả ngược
nhau**:

| Run | Trigger | Job `TLS, with the kernel it needs` |
|---|---|---|
| [`34510700732`](https://github.com/tmthang86/fixbolt/actions/runs/34510700732) | `push` | **pass**, 50s |
| [`34510705777`](https://github.com/tmthang86/fixbolt/actions/runs/34510705777) | `pull_request` | **fail**, 37s |

`gh run view … --json headSha` xác nhận cả hai là `aa4f46e`. **Nên đây không phải "đỏ trên
runner, xanh ở nhà" — nó flaky ngay trên chính runner đó.** Item 65 ghi một lần đỏ và đặt câu
hỏi "máy nào"; câu hỏi đúng là "lần chạy nào".

**Và có HAI test đỏ, không phải một.** Log job đỏ, `running 6 tests`:

```
test a_handshake_completes_without_the_acceptor_ever_blocking ... FAILED
test a_logon_sent_straight_after_finished_is_not_lost ... FAILED
test result: FAILED. 4 passed; 2 failed
```

- `crates/engine/tests/tls.rs:236` — `left: []`, `right: [104, 101, 108, 108, 111]`. Cái item 65
  đã ghi.
- `crates/engine/tests/tls.rs:150` — *"the handshake completed without ever yielding"*, tức
  `pendings == 0`. **Cái này hoàn toàn mới, item 65 không có nó.**

**Một chi tiết nữa đọc được và nó đổi việc phải làm:** job báo **6 test**, không phải 5 như item
65 ghi — nhưng `cargo test` vẫn dừng ở binary đỏ đầu tiên, nên **`tls_wire.rs` và `tls_mode.rs`
vẫn chưa từng chạy ở bất kỳ máy nào ngoài bàn này.** Bước 4a và 4b của plan `tls` **chưa hề được
CI chứng minh**, và không có gì trong kế hoạch này nói ra điều đó.

### Giả thuyết, và vì sao nó KHÔNG được quyết ở đây

Đọc `crates/engine/src/tls.rs:204-268`, `pump` trả `Step::Done` ngay khi đạt `WriteTraffic` với
`discard == 0` và `out_used == 0`. **Nó không đợi application data của client.** Client trong
`client_thread` chỉ ghi `hello` **sau khi** bắt tay phía nó xong, mà bắt tay phía nó xong thì cần
flight của server — nên `early` có byte hay không phụ thuộc vào việc record `hello` có tình cờ
nằm trong `self.incoming` đúng lúc `process_tls_records` chạy hay không. Cùng một đường đua giải
thích luôn `pendings == 0`: server được lên lịch trễ, cả flight đã nằm sẵn trong buffer kernel,
không sweep nào phải đợi.

**Đọc như vậy là giả thuyết (a) của item 65, và nó vẫn KHÔNG phải bằng chứng.** Suy từ hình dạng
của feature đúng là thứ đã tạo ra dự đoán sai ở đảo chiều 1 của bước 4a, mười hai tiếng trước lần
đỏ đầu tiên. Hai khả năng vẫn còn nguyên:

- **(a)** Test khẳng định một kết quả đua. Byte `hello` chưa tới lúc `Done`, và engine **không
  mất gì** — nó sẽ nhận số byte đó qua đường `Traffic` sau handover.
- **(b)** Byte đã tới mà `take_early_data` trả rỗng. Đó là lỗi thật trong
  `crates/engine/src/tls.rs`, đúng cái tên test khẳng định nó canh.

**Chênh lệch giữa (a) và (b) không phải chuyện học thuật:** (a) thì sửa test, không chạm
`crates/*/src`, phạm vi gốc giữ nguyên; (b) thì kế hoạch này phải mở phạm vi sang code engine và
PR #61 đang chở một lỗi mất dữ liệu.

### Bước thêm

**Bước 6 là ĐO, không phải sửa** — cùng luật đã làm bước 4 của bản gốc biến mất.

6. **In ra, ở cả hai máy, trạng thái tại thời điểm `Step::Done`:** số byte `early`, `leftover()`,
   và số sweep `Pending`. Rồi — đây là cái phân biệt (a) với (b) — **sau khi `Done`, đọc tiếp
   socket trong một cửa sổ có chặn** và in ra có byte nào tới muộn không.
   - Runner in `early 0` **và** có byte tới sau `Done` → **(a)**, client chưa kịp ghi.
   - Runner in có byte tới trước `Done` mà `early` vẫn rỗng → **(b)**, lỗi engine.
   - Không rơi vào cả hai → chưa kết luận, và **không được đoán tiếp**.

   Chạy ở máy này nhiều lần **và** trên runner. Một lần xanh trên runner không trả lời gì, vì
   bảng trên đã cho thấy cùng commit ra hai kết quả.
7. **Rẽ theo bước 6.** Nếu (a): sửa test để nó khẳng định thứ nó định khẳng định chứ không khẳng
   định một kết quả đua — và **không nới assertion nào** (bẫy đã lường trước, dòng *Sửa `tls.rs`
   để nó xanh trên runner*). Một test đòi `hello` phải tới trước `Done` thì phải **làm cho điều
   đó đúng**, không phải hy vọng nó đúng. Nếu (b): phạm vi mở sang `crates/engine/src/tls.rs`,
   và việc đó cần duyệt lại lần nữa trước khi viết code.
8. **Bắt `cargo test` chạy hết các binary TLS.** Hôm nay một binary đỏ giấu hai binary kia; sau
   bước 7, job phải in được tên test của `tls_wire.rs` và `tls_mode.rs` — đúng tiêu chí đạt bản
   gốc đã đặt ra cho bước 2, chỉ là chưa ai kiểm nó cho hai file này.

| Bước | Kết quả | Phụ thuộc | Máy |
|---|---|---|---|
| 6 | (a) hay (b), **đo được, in ra log**, ở cả hai máy | — | máy này **và** CI |
| 7 | Test nói thật, hoặc phạm vi mở sang `src` + duyệt lại | 6 | theo 6 |
| 8 | Tên test của `tls_wire.rs` và `tls_mode.rs` xuất hiện trong log CI | 7 | CI |

### Cách kiểm chứng, thêm vào

- **Một lần xanh không đóng được việc này.** Tiêu chí đạt cho bước 7 là **job `tls` xanh nhiều
  lần liên tiếp trên runner**, không phải một lần. Bảng ở đầu *Sửa 1* là lý do: một run xanh của
  chính commit đang hỏng đã tồn tại.
- **Đảo chiều cho bước 7**, nếu kết luận là (a): cố tình làm cho `hello` tới **sau** `Done` và
  xác nhận test **vẫn xanh** nếu nó được sửa đúng — vì engine thật sự không mất byte đó. Nếu nó
  đỏ, bản sửa đang khẳng định đường đua lần nữa, chỉ ở chỗ khác.
- **Đảo chiều cho bước 8:** bỏ một binary TLS ra khỏi lệnh và xác nhận tên test của nó biến mất
  khỏi log — cùng hình dạng với đảo chiều `0 test / 11 test` của bản gốc.

### Tài liệu phải cập nhật, thêm vào

- [ ] `STATUS.md` item 65 — **đang sai ở ba chỗ**: nó ghi một test đỏ (thực tế hai), ghi 5 test
      chạy (thực tế 6), và không ghi rằng **cùng commit có một run xanh**. Sửa kèm id của cả hai
      run.
- [ ] `STATUS.md` item 62 — mở lại, vì kế hoạch này quay về *Chờ duyệt*.
- [ ] `docs/reference/` — **một case mới, `[to testing-skills]`**, nếu kết luận là (a): *một test
      khẳng định một kết quả của bộ lập lịch, và nó xanh hai mươi lần trên một cái bàn*. Điểm
      đáng viết không phải là cái race — mà là **cùng một commit cho hai kết quả ngược nhau trên
      cùng một runner, cách nhau vài giây**, nên "chạy lại thấy xanh" là bằng chứng của không gì
      cả. Nối vào [a-reversal-can-fail-by-hanging.md](../reference/a-reversal-can-fail-by-hanging.md)
      nếu cùng họ, chứ không mở file mới nếu không cần.

### Ngoài phạm vi, thêm vào

- **Không đụng bốn test TLS đang xanh.** Chỉ hai test có tên ở trên được sửa, và chỉ sau bước 6.
- **Vẫn không số §8 nào.** Bước 6 in ra số byte và số sweep; đó là số chẩn đoán, không phải số
  latency, và không dòng nào của nó vào `DESIGN.md` §8.
