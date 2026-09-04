# Resend trả lại đúng message, không phải gap fill

> **Loại:** Plan · **Ngày:** 2026-09-03 · **Trạng thái:** **ĐÓNG 2026-09-04**, sáu bước
> **Phạm vi:** `STATUS.md` item 43. Chạm `session` (trait `Journal`, vòng resend, hai bộ đếm,
> một trường `Config`), `engine` (`MemJournal`, `FileJournal`, `observe`, `Store`), `library`
> (re-export), docs. **Không chạm** `codec`, `dict`, `transport`.
>
> **Máy chạy:** viết và test trên macOS. Số đo bộ nhớ và `benches/alloc.rs` chạy trên CI là đủ;
> **không** có số latency mới để công bố nên không cần máy §9. Nếu bước 3 làm `busy` trong
> `benches/alloc.rs` đổi thời gian, ghi nhận và để đợt C đo.
>
> **Thời lượng dự kiến:** 2–3 ngày.

## Bối cảnh

Một acceptor gửi 100 `ExecutionReport` trong ngày. Đối tác rớt mạng, kết nối lại, hỏi
`35=2 7=1 16=0` ("gửi lại tất cả"). Hôm nay fixbolt trả lời: **replay 8 message gần nhất, gap
fill 92 cái còn lại**. Trên dây đó là câu trả lời hợp lệ — FIX cho phép người gửi gap fill. Với
đối tác, đó là **92 báo cáo khớp lệnh biến mất**, và không có bộ đếm nào ở phía mình nói điều
đó vừa xảy ra.

Vì sao lại thế: vòng resend của session (`crates/session/src/lib.rs:2097–2130`) hỏi
`journal.get(seq)`; có thì replay, không thì gộp thành một `SequenceReset` gap fill.
`Store` mặc định là `MemJournal<8, 512>` — ring 8 slot — và ngay cả `FileJournal<N, LEN>`
cũng chỉ trả `get` từ ring `mem` của nó, chưa bao giờ từ đĩa (`journal.rs:440–470`, và đó là
quyết định đúng: đọc đĩa trên engine thread là bất biến 4 vỡ). Tám là con số chọn cho corpus
("never asks for more than three at once", `journal.rs:37`) và rustdoc nói "a real acceptor sets
its own" — nhưng không có gì ép, không có gì đếm, và `docs/GUIDE.md` §6 không có phép tính
nào để chọn N.

Có ba lỗ hổng nhỏ hơn cùng chỗ:

1. `Journal::put` **từ chối trong im lặng** khi message dài hơn `LEN` (`journal.rs:99–104`);
   trait nói "there is nothing the session could do about it" — đúng, nhưng *đếm* thì làm được.
2. Một resend lớn **tự giết session**: vòng replay emit tất cả trong một lần gọi; `TX` mặc định
   8 KiB; `Out::push` từ chối message không vừa (`conn.rs:581`) → `overflow` → D10 `Disconnect`
   → `Logout 58=slow consumer`. 50 message × 200 byte là đủ. Đối tác vừa hỏi resend bị đuổi vì
   "chậm", trong khi socket của họ trống.
3. `MemJournal::get` và `highest` **quét tuyến tính** cả N slot (`journal.rs:112–121`). Vô hại ở
   N = 8; ở N = 4096 một resend 1000 message là 4 triệu phép so sánh trên engine thread.

Kết quả muốn đạt: (a) mặc định giữ đủ cho một ngày giao dịch bình thường, và người vận hành có
phép tính để chọn; (b) một resend dài không làm rớt session; (c) mỗi lần resend "đụng đáy" ring
hay `put` bị từ chối đều là **một sự kiện có tên**; (d) tất cả không thêm allocation, không
blocking, 59/59 và 10/50 không đổi.

## Những gì đã biết chắc

| Sự thật | Nguồn |
|---|---|
| Vòng resend: `end` được kẹp về `next_out - 1` (kể cả `16=0` và range vượt quá đã gửi); replay từng `n`, run không replay được gộp một gap fill | `crates/session/src/lib.rs:2097–2130` |
| Chỉ application message được `put`; admin không bao giờ replay (giống QuickFIX) | `crates/session/src/journal.rs:22–32` |
| `Journal` trait: `put`, `get`, `highest` (không default), `mark_in`, `highest_in`, `mark_active`/`last_active` (default rỗng) | `crates/session/src/journal.rs`, ADR-0008, ADR-0017, ADR-0039 |
| `MemJournal<N, LEN>`: `slots: [Slot<LEN>; N]` **inline**, `Slot = { seq: u32, len: u16, buf: [u8; LEN] }`; `at` tăng dần, ghi đè `at % N` | `crates/engine/src/journal.rs:48–110` |
| `FileJournal` = `MemJournal` + file; `Async` đẩy record qua `ring::pair(1 << 20)` sang writer thread, ring đầy thì **bỏ record, không chờ** | `journal.rs:356–372, 440–455` |
| `Store = MemJournal<SLOTS, SLOT_LEN>`, `SLOTS = 8`, `SLOT_LEN = 512`; `TcpAcceptorEngine<A, W, J = Store>` | `journal.rs:40, 45, 140`; `lib.rs:970` |
| Session tự gửi `ResendRequest 16=0` khi thấy gap; `send_resend_request` của operator cũng cho `16=0` | `lib.rs:1340, 966` |
| `Out::push`: message nguyên vẹn hoặc không gì cả; `blocks == false` thì từ chối và đặt `overflow`; engine gọi `slow_consumer()` → Logout | `crates/engine/src/conn.rs:575–600`, D10 |
| `Engine::turn`: flush → tick → recv một lần → cắt và judge từng message → flush | `DESIGN.md` §4 D8 *As built* |
| `Session::tick` được gọi **mỗi turn** cho mọi connection | `conn.rs:245` |
| Corpus: 59 / 59 in-process và qua socket; mirror 10 / 50 assert theo tên file; `8_AdminAndApplicationMessages.def` hỏi 2..=8 và mong `fill(2..5), 5, 6, fill(7..9)` | `DESIGN.md` §6; comment tại `lib.rs:2113` |
| Sự kiện có trường đã tồn tại và là `Copy`: `EventKind::Administered { change, to, outcome }` | `crates/engine/src/observe.rs:475–500` |
| Mẫu "session ghi một số, engine đọc lại và phát sự kiện": `last_skew_ms`, `last_drop_reason`, `was_on` | ADR-0032 quyết định 5, ADR-0035, `lib.rs:700–725` của engine |
| Alloc bench session có 16 đường, trong đó `resend` và `fill`; engine có 21 đường trong đó `busy` | `DESIGN.md` §6 |
| Stack mặc định của thread chính là 8 MiB (Linux, macOS); một mảng 32 MiB tạo bằng giá trị rồi move là stack overflow | thực tế Rust; xem bẫy 3 |

## Quyết định trung tâm — sẽ là ADR-0046

Viết ADR trước khi code (bước 0). Nội dung dự kiến, để người duyệt thấy trước:

1. **Ring trong bộ nhớ là toàn bộ kho resend. Đĩa là để khôi phục sau restart và để audit,
   không phải để resend.** Engine thread không bao giờ đọc đĩa để trả lời một `ResendRequest`
   (bất biến 4). Message cũ hơn ring thì gap fill — **và điều đó được đếm và phát sự kiện**.
2. **Kích thước ring là tham số triển khai, có công thức, và mặc định giữ được một ngày bình
   thường.** `SLOTS` 8 → **4096**. Bộ nhớ = `N × (LEN + 8)` ≈ 2 MiB mỗi session ở mặc định.
   Công thức trong `GUIDE.md` §6: *N ≥ số application message bạn gửi trong khoảng mất kết nối
   dài nhất bạn chấp nhận resend được, thường là một ngày giao dịch*.
3. **Ring đánh địa chỉ theo `seq % N`, O(1)**, không quét. `oldest()` là một trường.
4. **Replay theo lô.** Session giữ một con trỏ resend và replay tối đa `resend_batch` message
   mỗi lần được gọi (`received_with` hoặc `tick`); phần còn lại đi ở các turn sau, xen kẽ hợp lệ
   với message mới (số `34=` của bản replay là số cũ, không ai nhầm). `resend_batch × LEN`
   phải nhỏ hơn `TX`; mặc định 8 × 512 = 4 KiB < 8 KiB. D10 không đổi.
5. **Bị loại, có lý do**: (a) `pread` trên engine thread khi ring không có — vi phạm bất biến 4
   ở `hft`, còn ở `standard` là "hai chế độ, hai quy tắc"; (b) thread phụ đọc đĩa và tự gửi —
   session sở hữu `34=` và `52=`, một thread khác không chen vào giữa được mà không phá D1;
   (c) `Vec` lớn dần như QuickFIX — bất biến 1. **Hoãn, không loại**: fallback đọc đĩa cho
   riêng `standard`, chỉ khi có deployment thật cần và có ADR riêng.

## Cách làm

**Session (`crates/session`)**

- `journal.rs`: `fn put(&mut self, seq, bytes) -> bool` (đã giữ hay từ chối) và
  `fn oldest(&self) -> Option<u32>` — **không default**, cùng lý do `highest` không default.
  `NoJournal` trả `false`/`None`.
- `lib.rs`:
  - Hai bộ đếm `u32` trên `Session`: `puts_refused`, `resend_beyond_journal`; accessor
    `pub const fn puts_refused()`, `resend_beyond_journal()`; **không phải** hot-path
    accessor, giống `next_out()`.
  - `send_application`/đường ghi journal (`lib.rs:~1621`): `if !journal.put(..) { self.puts_refused += 1 }`.
  - `Config::resend_batch: u16`, mặc định `8`, `Config::with_resend_batch(n)`; `0` bị từ chối
    (không có nghĩa).
  - Trạng thái `resend: Option<Resend { next: u32, end: u32 }>` trên `Session`.
  - Vòng ở `lib.rs:2097`: thay bằng `self.start_resend(begin, end)` rồi
    `self.continue_resend(journal, emit)?`. `continue_resend` replay/fill tối đa `resend_batch`
    message rồi dừng, giữ con trỏ. Gọi lại ở cuối `tick` và cuối `received_with` khi
    `resend.is_some()`. Trong lúc replay, nếu `n < journal.oldest().unwrap_or(n)` thì
    `resend_beyond_journal += 1` cho **mỗi** số bị fill dưới đáy ring (đếm số message có thể
    đã mất, không đếm số lần).
  - Huỷ con trỏ ở `disconnect`, khi nhận `Logout`, và khi một `ResendRequest` mới đến (cái mới
    thay cái cũ — không xếp hàng).
  - Gap fill trong lô: một run fill có thể bị cắt ở biên lô; gửi `SequenceReset` cho phần đã
    duyệt, lô sau tiếp tục. Hai gap fill liền nhau là hợp lệ.

**Engine (`crates/engine`)**

- `journal.rs`: `MemJournal` giữ `slots: Box<[Slot<LEN>]>` cấp **một lần** trong `new()`
  (startup, không phải hot path; ghi rõ trong rustdoc và trong `benches/alloc.rs` là ngoài
  cửa sổ đếm). `get`: `let s = &self.slots[(seq as usize) % N]; (s.seq == seq && s.len > 0).then(..)`.
  Thêm trường `oldest: Option<u32>` cập nhật khi ghi đè. `highest` = `at`-based, O(1).
  `SLOTS = 4096`. `const _: () = assert!(size_of::<MemJournal<4096, 512>>() <= 64)` — để lùi về
  mảng inline là **lỗi compile**, không phải một test có thể tình cờ xanh.
- `FileJournal` chuyển tiếp `put`/`oldest` sang `mem`; định dạng file **không đổi**.
- `observe.rs`: `EventKind::ResendBeyondJournal { filled: u32, oldest: Option<u32> }` và
  `EventKind::JournalRefused { count: u32 }`; `SessionSnapshot` thêm hai bộ đếm. Engine so sánh
  bộ đếm trước/sau `conn.turn` như `was_on`, chỉ khi `self.observe.is_some()`; một `try_lock`
  mỗi sự kiện, không mỗi message (ADR-0035).
- `lib.rs` không đổi chữ ký public; `Store` đổi kích thước là đổi mặc định user-visible →
  `CONFIGURATION.md`, `CHANGELOG.md`.

**Library**: re-export không đổi; `docs/GUIDE.md` §6 thêm phép tính chọn N và cách đọc hai sự
kiện.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **1 — không cấp phát** | `Box<[Slot]>` một lần lúc tạo; con trỏ resend là `Option<{u32,u32}>` | `benches/alloc.rs` session: `resend` phải giữ **0** với một resend 100 message qua 13 tick; engine `busy` 0; case mới `resend-long` trong engine alloc bench: 4096-slot store, 100 message, `7=1 16=0`, đếm cả cửa sổ replay → **0** |
| **2 — session thuần** | thêm trạng thái, hai bộ đếm, một trường `Config` | không clock (batch tiếp tục theo `tick` đã có), không alloc, `Refusal` không đổi; 59 / 59 và 10 / 50 là cổng |
| **3 — 59 định nghĩa** | vòng resend viết lại | `cargo test -p fixbolt-session --test score` **59 / 59**, `--test mirror` **10 / 50** đúng tên file, `-p fixbolt-engine --test wire` 59 / 59 cả hai mode, `shard_wire` 59 qua hai shard |
| **4 — không ngủ trong kernel** | không đọc đĩa để resend (đó là điểm của ADR-0046) | không có `read` mới; `check-no-kernel-sleep.sh` không đổi kết quả |
| **7 — không unwrap** | code mới | clippy workspace |
| 5, 6, 8, 9, 10 | không đụng | — |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 0 | **ADR-0046** `Proposed`, năm quyết định ở trên, mục *Consequences* có cả giá: +2 MiB mỗi session ở mặc định, một resend dài giờ kéo dài nhiều turn, hai method mới trên trait public | — |
| 1 | `Journal::put -> bool`, `Journal::oldest`; `NoJournal`, `MemJournal`, `FileJournal`, mọi fake trong test và bench compile lại. Test đỏ trước: `crates/engine/tests/journal.rs::the_journal_says_what_its_oldest_kept_number_is` và `::a_put_that_is_refused_says_so` | 0 |
| 2 | `MemJournal` boxed, O(1), `oldest`, `SLOTS = 4096`, `const` assert kích thước. Test: `::four_thousand_puts_keep_the_last_4096_and_oldest_moves_with_them`; `::get_finds_by_number_after_the_ring_wraps` (put 10 000, get mọi số 5905..=10 000 đúng, mọi số ≤ 5904 là `None`) | 1 |
| 3 | Hai bộ đếm trên session; `resend_beyond_journal` tăng đúng số. Test đỏ trước trong `crates/session/tests/resend.rs`: `::a_resend_that_reaches_below_the_ring_counts_every_number_it_filled` (store 8 slot, gửi 20, hỏi `7=1 16=0`, mong `fill(1..13)`, replay 13..=20, bộ đếm **12**), `::a_put_the_journal_refuses_is_counted` (message dài hơn `LEN`) | 1, 2 |
| 4 | Replay theo lô: `Config::resend_batch`, con trỏ, tiếp tục ở `tick`/`received_with`, huỷ đúng lúc. Test đỏ trước: `resend.rs::a_long_resend_is_replayed_over_several_ticks_in_order` (100 kept, batch 8, mong 13 lần gọi mới hết, thứ tự tăng, không thiếu), `::a_disconnect_cancels_a_resend_in_progress`, `::a_new_resend_request_replaces_the_one_in_progress`, `::a_message_sent_during_a_resend_carries_the_next_new_number`. **Wire test đỏ trước** `crates/engine/tests/backpressure.rs::a_resend_larger_than_tx_does_not_end_the_session`: 100 message 200 byte kept, `TX = 8192`, policy `Disconnect`, `7=1 16=0` → session **còn sống**, đủ 100 bản `43=Y` về đúng thứ tự (hôm nay: `Logout 58=slow consumer`) | 3 |
| 5 | Sự kiện và snapshot trong `engine::observe`; `tests/events.rs::a_resend_past_the_ring_is_an_event_with_the_numbers`; alloc bench `resend-long`, `events-busy` vẫn 0 | 3, 4 |
| 6 | Docs theo bảng dưới; ADR-0046 → `Accepted`; `STATUS.md` item 43 gạch, *Not proven* đọc lại; CI xanh, run id | 5 |

## Cách kiểm chứng

Mỗi test mới **đỏ trước**, output dán vào nhật ký. Gate chạy khi đóng:

```
cargo test -p fixbolt-session --test score        # 59 / 59
cargo test -p fixbolt-session --test mirror       # 10 / 50, đúng tên file, Report::driven không đổi
cargo test -p fixbolt-session --test resend       # 5 cũ + 6 mới
cargo test -p fixbolt-engine --test wire          # 59 / 59 hai mode
cargo test -p fixbolt-engine --test backpressure  # test mới xanh
cargo test -p fixbolt-engine --test journal --test events
cargo test --all && cargo test --all --no-default-features
scripts/bench.sh                                  # invariant: alloc session/engine 0, kể cả resend-long
scripts/interop.sh                                # bước resend cả hai chiều vẫn ok (sau plan acceptor-interop)
```

**Reversal bắt buộc, mỗi cái ghi output:**

| Reversal | Phải thấy |
|---|---|
| `continue_resend` bỏ giới hạn lô (replay hết một lần) | `a_resend_larger_than_tx_does_not_end_the_session` **đỏ** với `Logout 58=slow consumer`; 59/59 **vẫn xanh** — chứng minh corpus không nhìn thấy lô, nên test wire là cái canh |
| Không tăng `resend_beyond_journal` | test bước 3 đỏ `left: 0, right: 12` |
| `oldest` trả `None` luôn | test bước 3 đỏ (không đếm được), test bước 2 đỏ |
| Lùi `slots` về mảng inline | **lỗi compile** ở `const` assert — không phải test |
| Huỷ con trỏ khi disconnect bị bỏ | `a_disconnect_cancels_a_resend_in_progress` đỏ: sau `connect` lại, tick đầu tiên emit bản `43=Y` của phiên trước |

**Số đo phải ghi (không phải gate):** RSS của `tools/w2w` với `Store` 4096 so với 8 — kỳ vọng
+2 MiB; nếu khác nhiều thì ring không phải 4096 × 520 byte và phải hiểu tại sao.

## Tài liệu phải cập nhật

- [ ] `docs/decisions/ADR-0046-*.md` — mới, `Proposed` ở bước 0, `Accepted` ở bước 6
- [ ] `docs/DESIGN.md` §4 D7: đoạn *As built* mới `[2026-09-xx]`: ring là kho resend, N, O(1), lô, hai sự kiện; **và** bảng §6 hàng alloc session thêm đường, hàng engine thêm `resend-long`; §3 hàng `engine` một dòng
- [ ] `docs/CONFIGURATION.md` §2: `SLOTS` 8 → 4096 với lý do; hàng mới `resend_batch` (mặc định 8, ràng buộc `resend_batch × SLOT_LEN < TX`)
- [ ] `docs/GUIDE.md` §6: phép tính chọn N; hai sự kiện và ý nghĩa vận hành ("ring quá nhỏ", "message dài hơn slot"); câu "`tools/jrnl` là cách lấy message cũ hơn ring, bằng tay"
- [ ] `docs/SESSION-BEHAVIOUR.md` §4: resend theo lô, kẹp `16=0`, gap fill dưới đáy ring — **nêu tên test canh**
- [ ] `docs/best-practices-standard.md` và `-hft.md`: một dòng về bộ nhớ mỗi session ở N mới, **nêu mode**
- [ ] `CHANGELOG.md`: `Journal` trait đổi (breaking), `Store` đổi kích thước, `Config::resend_batch`, hai `EventKind`
- [ ] `STATUS.md`: item 43; *Not proven* đọc lại từng dòng
- [ ] `docs/reference/`: nếu reversal nào vô hiệu hoặc corpus mù ở chỗ bất ngờ → một file, `[to testing-skills]`

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| 1. Corpus **mù** với lô: mọi file hỏi ≤ 3 message; batch 8 thì 59/59 xanh dù lô sai | `a_long_resend_is_replayed_over_several_ticks_in_order` (100 message, 13 lần gọi) và wire test `backpressure` |
| 2. Gap fill bị cắt ở biên lô phải **không** bỏ sót số: `fill(from, n)` với `n` là số đầu tiên **chưa** được cover | test bước 4 đếm đủ 100 và kiểm `36=` của từng `SequenceReset` nối liền nhau |
| 3. `MemJournal<4096, 512>::new()` tạo mảng 2 MiB trên stack rồi move; N lớn hơn (65536 = 32 MiB) là **SIGSEGV**, không phải test đỏ | `Box<[Slot]>` + `const` assert kích thước struct; reversal là lỗi compile |
| 4. Số thứ tự quấn: `seq % N` với `seq` là `u32` tăng dần — OK; nhưng `set_next_out` **lùi** số (ADR-0036) làm một slot cũ trùng số mới | `get` kiểm `s.seq == seq`; test `::a_number_reused_after_an_admin_reset_does_not_return_the_old_bytes` |
| 5. `resend_beyond_journal` đếm sai đơn vị (số lần thay vì số message) | test bước 3 mong **12**, không phải 1 |
| 6. Con trỏ resend sống qua reconnect và bơm `43=Y` của phiên cũ vào phiên mới | `a_disconnect_cancels_a_resend_in_progress` |
| 7. Event phát mỗi message thay vì mỗi thay đổi → `try_lock` mỗi message | `events-busy` alloc 0 **và** `Observer::events_lost` = 0 sau 10 000 message trong test `events.rs` |
| 8. `FileJournal` `Async` bỏ record khi ring 1 MiB đầy, im lặng; không thuộc plan này nhưng cùng hình | ghi vào ADR-0046 *Consequences* là nợ còn lại; **không** sửa ở đây (đợt C đo trước) |
| 9. `mirror` gate assert `Report::driven` bằng số chính xác; lô mới không được đổi số drive | `--test mirror` xanh nguyên, không sửa số |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| `Journal::put -> bool` là breaking change cho ai đã impl trait | thấp (chưa publish) | `CHANGELOG.md`; mọi impl trong repo đổi cùng commit |
| +2 MiB mỗi session ở `density` với hàng trăm session | trung bình | mặc định là cho `hft` N = 1 và acceptor thông thường; `GUIDE.md` §1a nói gateway chọn N nhỏ hơn bằng const generic — đã có cơ chế |
| Replay theo lô làm resend chậm hơn (13 turn thay vì 1) | thấp | một turn `standard` là một `poll` wakeup, `hft` là ~449 ns; 13 turn vẫn dưới 1 ms ở `standard`. Ghi số vào ADR |
| Tương tác với `Schedule` reset `34=1` giữa lúc resend | thấp | `resume_at`/reset gọi huỷ con trỏ như `disconnect`; test `::a_schedule_reset_cancels_a_resend_in_progress` |

## Ngoài phạm vi

- Resend từ đĩa (bị loại/hoãn theo ADR-0046). `tools/jrnl` là cách đọc message cũ, bằng tay.
- `FileJournal` `Async` ring 1 MiB đầy → bỏ record: đợt C đo và item riêng.
- Chunking **phía nhận** (`ResendRequestChunkSize` của QuickFIX — chia nhỏ câu hỏi của mình): không cần khi phía gửi đã trả theo lô; nếu một đối tác gửi ồ ạt vào mình, D10b và `RX` đã có chính sách.
- Ghi bộ đếm ra đĩa: không; đó là chỉ số vận hành, snapshot là đủ.

## Nhật ký giao hàng

**Đóng 2026-09-04, cả sáu bước.** ADR-0046 `Accepted`. `cargo test --all` 446 → 463.

### Số đã đo

| | |
|---|---|
| RSS `tools/w2w --mode standard --messages 2000`, `SLOTS = 4096` | **4 702 208 byte** |
| cùng lệnh, `SLOTS = 8` | **2 506 752 byte** |
| chênh lệch | **+2 195 456 byte = 2.09 MiB** (phép tính 4096 × 520 = 2.03 MiB; phần dư là allocator) |

Máy: Apple M5, macOS 15. **Không phải máy §9 và không có số latency nào ở đây.**

### Ba thứ tìm ra mà plan không nêu

1. **`get` cũ trả bytes sai.** `slots.iter().find()` lấy slot **đầu tiên** mang số đó; sau khi
   `Admin::SetNextOut` lùi số (ADR-0036), replay trả **message cũ** — đúng số, đúng checksum,
   sai nội dung. Địa chỉ hoá `seq % N` chữa. Test đỏ trước:
   `left: Some("the first nine")  right: Some("the second nine")`.
2. **`tick` không journal sẽ gap-fill đè lên replay dở.** `NoJournal` trả "không giữ" cho mọi
   số → một `SequenceReset` nuốt toàn bộ phần còn lại, đối tác mất message mình vẫn đang giữ.
   `tick` giờ đứng yên; `tick_with` mới chạy tiếp. Đứng yên thì cứu được, fill thì không.
3. **`Engine::add` cấp phát ~2 MiB trên engine thread.** `benches/alloc.rs` bắt được:
   `events-busy` 0 → 2000, session `deliver`/`resend`/`originate` 0 → 10000 mỗi cái. Plan đã
   lường ("startup, ngoài cửa sổ đếm") nhưng không truy ra hệ quả này. Sửa theo plan **và**
   không bằng cách xoá call khỏi cửa sổ: engine bench dựng journal ngoài rồi gọi
   `add_with_journal` trong cửa sổ, nên một allocation xuất hiện trong accept **vì lý do khác**
   vẫn bị bắt. Ghi vào ADR-0046 *Consequences*, `GUIDE.md` §6a0, `best-practices-hft.md` §6.

### Hai chỗ khác plan, đều nhỏ

- **`oldest` là một phép đọc một slot, không phải một trường được duy trì** — sửa đổi ghi trong
  ADR khi còn `Proposed`. Chỉ application message được journal nên số trong ring **thưa**, và
  "số vừa rời đi + 1" gọi tên một message chưa từng gửi. Nó là **sàn**, không phải lời hứa số
  đó có mặt — rustdoc nói rõ, và đó chính là nửa mà bộ đếm cần.
- **`highest` vẫn quét.** Nó chỉ được hỏi một lần mỗi connection lúc recovery, không nằm trên
  đường message; làm O(1) buộc phải chọn giữa *max trên các slot* và *số ghi sau cùng*, hai thứ
  khác nhau sau một lần lùi số, và không có phép đo nào nói rủi ro đó đáng.
- **Test bộ đếm nằm ở `crates/engine/tests/journal.rs`**, không phải `crates/session/tests/resend.rs`:
  helper dựng order và `ResendRequest` thật đã ở đó, và bản sao thứ hai là hai fixture rồi sẽ lệch nhau.
- **Số trong test bước 3 là 13, không phải 12** như plan viết: `34=1` là Logon nên dải lấp là
  1..=13. Con số minh hoạ của plan giả định app message bắt đầu từ 1.

### Reversal

| Reversal | Thấy gì |
|---|---|
| `continue_replay` không giới hạn lô (`with_resend_batch(10_000)`) | `the session ended on turn 1 while answering a resend`. **`--test score` vẫn 4 passed và `--test wire` vẫn 2 passed** — corpus mù, đúng bẫy 1 |
| Bỏ dòng tăng `resend_beyond_journal` | `left: 0  right: 13`, 1 failed / 14 filtered out — chỉ đúng case nhắm tới |
| `oldest` trả `None` luôn | 5 failed / 10 passed: hai test bước 1, hai test ring bước 2, và test bộ đếm |
| `slots` về mảng inline | `error[E0080]: evaluation panicked: assertion failed: core::mem::size_of::<Store>() <= 64` — **lỗi compile**, không phải test |
| Chặn emit `ResendBeyondJournal` | `timed out waiting for a resend past the ring; saw: []` |

### Gate

| Lệnh | Kết quả |
|---|---|
| `cargo test --all` | **463 passed, 0 failed** (446 lúc bắt đầu) |
| `cargo test --all --no-default-features` | sạch |
| `cargo test -p fixbolt-session --test score` | 59 / 59 |
| `cargo test -p fixbolt-engine --test wire` | 59 / 59 hai mode |
| `cargo bench -p fixbolt-session --bench alloc` | 16 / 16 số 0 |
| `cargo bench -p fixbolt-engine --bench alloc` | 21 / 21 số 0 |
| `cargo clippy --all-targets --all-features -D warnings` | sạch |
| `scripts/check-links.py` | 1256 link, 0 chết |

### Không làm, nói rõ

- **Resend từ đĩa**: loại theo ADR-0046 quyết định 5, hoãn cho `standard` nếu có deployment thật.
- **`FileJournal` `Async` ring đầy thì bỏ record trong im lặng**: cùng hình một tầng dưới, ghi
  vào ADR-0046 *Consequences* là nợ, đợt C đo trước.
- **Số latency**: không có. Chỉ có một số bộ nhớ, từ laptop, và nói rõ là từ laptop.
