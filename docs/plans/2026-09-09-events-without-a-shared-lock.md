# Đọc luồng sự kiện không được làm mất sự kiện

> **Loại:** Plan · **Ngày:** 2026-09-09 · **Trạng thái:** Đã duyệt, đang làm
> **Phạm vi:** `STATUS.md` item 60. Chạm `engine` (`observe`), test của `engine`, `GUIDE.md`,
> docs. **Không chạm** `codec`, `session`, `transport`, và **không đổi API công khai**.
> Quyết định gate: **ADR-0059** (`Accepted` 2026-09-09).
> **Không viết một dòng code nào trước khi ADR-0059 được duyệt.**

## Bối cảnh

Hai lần CI đỏ, hai crate khác nhau, đều bị xếp là "flake". Cả hai là **cùng một lỗi**: một sự kiện
`Ended` không bao giờ tới, vì nó đã bị vứt đi trong lúc một reader đang giữ mutex của ring.

`EventRing::push` dùng `try_lock`; trượt thì bỏ sự kiện và tăng bộ đếm. `try_lock` là **đúng** —
non-negotiable 4 cấm engine thread chặn sau thread của operator. Nhưng hệ quả là **đọc luồng thì
phá luồng**, và chờ bao lâu cũng không lấy lại được.

Điều làm nó đáng sửa ở tầng thiết kế chứ không phải tầng test: **mất mát không phải cái giá của
việc không chặn.** Nó là cái giá của việc producer và consumer dùng chung một mutex — thứ chưa ai
quyết định bao giờ. `crates/engine/src/ring.rs` trong chính crate này là hàng đợi SPSC lock-free,
không một dòng `unsafe`, đã có ADR-0007 và test riêng.

## Những gì đã biết chắc

Mọi dòng đo hoặc đọc ngày 2026-09-09.

| Sự thật | Nguồn |
|---|---|
| `push` dùng `try_lock`, trượt thì `lost += 1` và **bỏ hẳn** sự kiện | `crates/engine/src/observe.rs:829-833` |
| `ring: Mutex<EventRing>` — một mutex cho cả producer lẫn consumer | `observe.rs:798` |
| Chỉ có **một** điểm push, trên engine thread | `observe.rs:383`, grep toàn crate |
| `Event` là `Copy`, `EVENT_CAPACITY = 256`, `slots: [Option<Event>; 256]` | `observe.rs:635, 765, 807` |
| `Observer` là **`Clone`**, ôm `Arc<Shared>` → **nhiều consumer là có thật** | `observe.rs:483-484`; và `[đo 2026-09-05]` `connect_and_serve` từng rút mất ring của caller (ADR-0054) |
| `drain` **nối thêm** vào `out`, không ghi đè | `observe.rs:847-862` — đã kiểm, vì tôi từng đoán ngược lại |
| `ring.rs` là hàng đợi **byte** (`push(&[&[u8]])`), `AtomicUsize` head/tail, không `unsafe` | `crates/engine/src/ring.rs:49-51, 157, 190` |
| **Tái hiện được**: bỏ `sleep(2ms)` giữa hai lần poll → **9 lỗi / 20 lần**, có đúng chữ ký CI kèm `events_lost=1` | `[đo 2026-09-09]` |
| Đói CPU **không** tái hiện: 32 spinner trên 2 nhân → 0.12 s / ngân sách 5 s; 60/60 xanh; full suite 2 nhân 3/3 xanh | `[đo 2026-09-09]` |
| 4 file test poll luồng này; **3 file chưa từng nhắc `events_lost`** (`originate.rs`, `admin.rs`, `msglog.rs`) | grep |
| Bốn engine khác **không mất sự kiện**, và cả bốn mua điều đó bằng cách chặn session thread | khảo sát trong ADR-0059 |

## Cách làm

1. **`observe.rs`: tách producer khỏi consumer.** Slot array cộng chỉ số nguyên tử cho phía ghi;
   engine `push` không lấy khoá nào. Phía đọc giữ một mutex **chỉ để các reader xếp hàng với
   nhau**. Không `unsafe` — ADR-0007 đã chứng minh làm được.
2. **`events_lost` chỉ còn một nghĩa**: ring đầy. Rustdoc nói đúng như vậy.
3. **Test: `wait_for` hỏi `events_lost()`** khi hết giờ và nói thẳng *"luồng đã mất N sự kiện, test
   này không thể thấy chúng"* thay vì đổ cho timeout. Áp cho cả 4 file, kể cả 3 file chưa nhắc tới
   nó bao giờ.
4. **Test mới, chứng minh cái vừa sửa**: một reader poll thật chặt trong khi engine sinh sự kiện —
   đúng vòng lặp đã tái hiện được 9/20 — và khẳng định **không mất gì**.
5. **`GUIDE.md`**: luồng này lossy khi ring đầy, trong khi bốn engine khác thì không. Ràng buộc
   người dùng phải biết mà compiler không kiểm được.

File sửa: `crates/engine/src/observe.rs`, 4 file test của `engine`, `GUIDE.md`, `CHANGELOG.md`,
`STATUS.md`, ADR-0035 (thêm dòng trỏ sang ADR-0059, **không sửa quyết định** — §5).

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **4 — engine thread không ngủ trong kernel** | đây là điều khoản khiến `try_lock` tồn tại | phía ghi **không lấy khoá nào cả**, nên còn mạnh hơn trước; `check-no-kernel-sleep.sh` chạy lại cả hai mode |
| **1 — không cấp phát** | `push` nằm trên đường đóng kết nối | `benches/alloc.rs` case `events-idle` và `events-busy` phải vẫn đọc **0** |
| **8 — `unsafe` cần bằng chứng** | hàng đợi đồng thời viết tay | **không dùng `unsafe`**; nếu buộc phải thì dừng lại và viết ADR mới |
| 7 — không panic/unwrap | code mới trong library crate | `clippy -D warnings`; `check-indexing-debt.sh` ≤ 184 |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 0 | ADR-0059 được duyệt | — |
| 1 | **Test đỏ trước**: reader poll chặt + engine sinh sự kiện → đỏ trên code hôm nay, `events_lost > 0` | 0 |
| 2 | Phía ghi bỏ khoá; test bước 1 xanh | 1 |
| 3 | `wait_for` của cả 4 file test báo `events_lost` khi hết giờ | 2 |
| 4 | `GUIDE.md`, `CHANGELOG.md`, rustdoc, `STATUS.md`, trỏ ADR-0035 → ADR-0059 | 2–3 |

## Cách kiểm chứng

- **Bước 1 phải đỏ trước, và đỏ nhiều lần**: chạy ≥ 20 lần, phải có lỗi, và **trích nguyên** một
  dòng `events_lost=` khác 0. Một lần đỏ không đủ cho một lỗi xác suất.
- **Sau khi sửa: chạy lại đúng vòng lặp đó ≥ 200 lần, phải 0 lỗi và `events_lost` bằng 0.** Con số
  lần chạy là một phần của bằng chứng, vì lỗi này 9/20 chứ không phải 20/20.
- **Đảo ngược**: trả phía ghi về `try_lock`, test bước 1 phải đỏ lại.
- `benches/alloc.rs`: `events-idle` và `events-busy` vẫn **0**.
- `cargo test --all` và `--no-default-features` — đọc output, không đọc exit code, không qua `tail`.
- 59 / 59.
- `check-no-kernel-sleep.sh` **và** `check-standard-gives-the-core-back.sh` — đổi cơ chế đồng bộ
  của engine thread thì phải chạy **cả hai mode** (ADR-0013).
- `fmt`, `clippy -D warnings`, `check-indexing-debt.sh`, `check-links.py`,
  `check-every-crate-is-licensed.sh`.
- **CI xanh trên đúng commit đóng plan**, nêu run id.

## Tài liệu phải cập nhật

- [ ] `docs/GUIDE.md` — luồng sự kiện lossy khi ring đầy; bốn engine khác thì không
- [ ] `CHANGELOG.md` — hành vi quan sát được đổi
- [ ] `docs/decisions/ADR-0035-*.md` — **chỉ thêm một dòng trỏ sang ADR-0059**, không sửa quyết định (§5)
- [ ] `docs/decisions/ADR-0059-*.md` — `Proposed` → `Accepted`
- [ ] `STATUS.md` — đóng item 60; rà mục *Not proven*
- [ ] `docs/reference/the-same-commit-went-red-and-green-in-the-same-minute.md` — ghi nguyên nhân thật, vì trang đó hiện nói *chưa tìm ra*
- [ ] `docs/reference/prior-art.md` — bảng khảo sát năm engine về giao sự kiện
- [ ] `DESIGN.md` §4 — **chỉ khi** hành vi dispatch/backpressure đổi. Plan này **không** đổi.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| **Dựng SPSC trong khi consumer là multi.** `Observer` là `Clone`; hai reader cùng lúc là ca thật, ADR-0054 đã trả giá | test hai thread cùng `events()`, khẳng định không hỏng và không trùng sự kiện |
| Sửa xong vẫn mất, chỉ hiếm hơn — rồi tưởng là xong | ≥ 200 lần chạy sau khi sửa, không phải 5 |
| Bộ đếm `lost` thành ra không bao giờ tăng, và gate mất ý nghĩa | test **ép ring đầy** và khẳng định `events_lost` **tăng** |
| Phía ghi lặng lẽ cấp phát | `benches/alloc.rs` hai case, phải là 0 |
| Dùng `unsafe` cho nhanh | non-negotiable 8; nếu cần thì dừng và viết ADR |
| Thứ tự sự kiện đảo | test khẳng định thứ tự đúng như đã push |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Hàng đợi đồng thời viết tay có lỗi tinh vi | **Cao** | không `unsafe`; một producer duy nhất đã xác minh; test đa luồng; nếu không chứng minh được thì quay về `try_lock` và chỉ làm bước 3 + 5 |
| Sửa xong che mất một defect khác cũng gây mất sự kiện | Trung bình | giữ `events_lost` và bắt nó vẫn tăng khi ring đầy |
| Chậm hơn trên đường đóng kết nối | Thấp | đo `benches/` trước/sau cùng máy; đây **không** phải máy §9 nên ghi kèm tên máy |

## Ngoài phạm vi

- **Không** làm cho phía đọc lock-free. Nhiều reader vẫn xếp hàng với nhau — cố ý.
- **Không** đổi `EVENT_CAPACITY`, không bỏ chặn kích thước.
- **Không** đổi API công khai: `Observer`, `events()`, `events_lost()` giữ nguyên chữ ký.
- **Không** đụng luồng lệnh (`Admin`), dù nó có bất đối xứng tương tự — ADR-0036, việc khác.
- **Không** đo trên máy §9.

## Nhật ký giao hàng

**2026-09-09 — đóng, cả bốn bước.** ADR-0059 `Accepted` sau khi chủ dự án yêu cầu khảo sát các
engine khác thay vì duyệt ngay, và **khảo sát đó là lý do việc này thành ADR chứ không thành một
bản vá test**.

**Bằng chứng, trích nguyên chứ không tóm tắt.**

Bước 1 phải đỏ trước, và đỏ nhiều lần — `[đo 2026-09-09]` **6 lỗi / 30 lần** trên code cũ, ghim
vào 2 nhân:

```
assertion `left == right` failed: the stream lost 200 events while somebody was
reading it — reading must not destroy events (ADR-0059 decision 1); saw 0 of 200
  left: 200
 right: 0
```

Sau khi sửa: **0 lỗi / 220 lần chạy**. Con số 220 là một phần của bằng chứng — lỗi này 6/30 chứ
không phải 30/30, nên 5 lần xanh chứng minh được rất ít.

```
cargo test --all                    619 -> 621 passed, 0 failed
cargo test --all --no-default-features  614 -> 616 passed, 0 failed
59 definition                       59 / 59
benches/alloc.rs                    events-idle 0, events-busy 0 (cả 30 case đều 0)
check-no-kernel-sleep.sh            GREEN ok; RED ok — standard trips it
check-standard-gives-the-core-back  GREEN ok; RED ok — yield trips it, 98% CPU
check-indexing-debt.sh              181, trần hạ 184 -> 181 cùng commit
fmt, clippy -D warnings             sạch
`unsafe`                            KHÔNG có — 5 lần xuất hiện đều trong doc comment
```

**Đảo ngược, và nói đúng nó yếu ở đâu.** Bản đảo ngược tổng hợp — bắt phía ghi lấy khoá của phía
đọc — chỉ đỏ **1/30**, yếu hơn defect thật, vì nó chỉ giữ khoá chớp nhoáng còn bản gốc giữ mutex
cả ring suốt lúc drain. **Bằng chứng thật là cặp số đo trên chính code gốc: 6/30 đỏ so với 0/220
sau khi sửa.** Ghi vậy thay vì nhận vơ cho bản tổng hợp.

**Hai thứ ngoài dự kiến.**

1. **Ratchet indexing đỏ vì nợ đi XUỐNG**, 184 → 181: thay `slots[head]` bằng `.get()` bỏ được 3
   chỗ. Script bắt hạ trần cùng commit — đúng thiết kế, và là lần đầu nó kêu theo chiều này.
2. **Test phải là unit test trong `observe.rs`, không phải integration test.** `Shared::emit` là
   `pub(crate)` và `Handles` không mở nó ra. Thêm một cửa test-only vào API công khai để kiểm một
   giao kèo nội bộ là đánh đổi sai, nên test sống cùng chỗ với giao kèo.

**Chưa chứng minh:** không có số §9, không đo trước/sau trên máy §9 (desk này còn hai lần reboot
nữa), và `benches/turn.rs` không chạy lại. Hàng đợi đồng thời viết tay là rủi ro `Cao` trong bảng
trên và vẫn là rủi ro Cao sau khi làm xong — cái giảm nó là **không dùng `unsafe`**, một producer
duy nhất đã xác minh bằng grep, và 220 lần chạy hai luồng.
