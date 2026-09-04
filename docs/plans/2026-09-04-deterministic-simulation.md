# Mô phỏng tất định: bắt lỗi mà corpus không viết ra

> **Loại:** Plan · **Ngày:** 2026-09-04 · **Trạng thái:** Draft
> **Phạm vi:** `STATUS.md` item 45, đợt D, plan thứ hai. Chạm `engine` (một `Transport` giả
> có lịch lỗi, chỉ trong `tests/`/`benches/`), `fuzz/` (target thứ hai), CI (job fuzz ngắn),
> docs. **Không chạm code ship** — nếu harness tìm ra lỗi, lỗi có plan riêng.
>
> **Draft viết 2026-09-04.** Khi đến lượt: đọc lại `Loopback`, `ManualClock`, danh sách
> `DropReason`/`EventKind` sau đợt A–B (harness assert trên chúng), và `fuzz/` (nightly).
> Sửa rồi *Chờ duyệt*. **Máy chạy:** macOS đủ; fuzz dài chạy trên Linux CI. **Thời lượng:** 2–3 ngày.

## Bối cảnh

Repo tự viết: *"a conformance corpus is not an adversarial one"*. 59 file kiểm những kịch bản
QuickFIX nghĩ ra năm 2003; mọi lỗi tìm được từ đó đến nay (initiator trả `Logon` bằng `Logon`,
`Logout` thứ ba, `58=` rỗng, socket đóng giữa message, shard trùng identity) đều nằm **ngoài**
corpus và được tìm bởi một harness khác mỗi lần. Chi phí mỗi harness là một ngày.

Kiến trúc đã trả tiền cho việc này: session thuần, clock inject, `Transport` là trait,
`Loopback` có sẵn, `Engine::turn` gọi tay được. Cái thiếu là **một** harness sinh kịch bản từ
seed, chạy hai engine fixbolt đối diện nhau (acceptor và initiator) trong một process, tiêm
lỗi có lịch, và kiểm **bất biến** thay vì kiểm output mong đợi. Phase 2 nhân đôi codec; harness
này chạy trên cả hai mà không viết lại.

## Những gì đã biết chắc (2026-09-04 — xác minh lại khi làm)

| Sự thật | Nguồn |
|---|---|
| `Loopback` transport, `ManualClock`, `Engine::turn` gọi tay: `wire.rs` chạy 59 file qua socket thật không thread, không sleep | D8 *As built*, `crates/engine/tests/wire.rs` |
| Alloc bench đã dựng engine + `Loopback` + `RingDispatch` trong một process | `crates/engine/benches/alloc.rs` |
| `fuzz/fuzz_targets/parse.rs` — target duy nhất, 304M lần, nightly, ngoài workspace | `DESIGN.md` §6 |
| Framing tách ở mọi biên byte có test | `crates/engine/tests/transport.rs`, `standard.rs` |
| `DropReason` không trường, `From<Refusal>` không `_` arm; `EventKind` có `Ended(DropReason)` | ADR-0035 |
| Journal `highest()`, `highest_in()`, `oldest()` (sau đợt A) | ADR-0017, plan resend |
| Không RNG trong `engine`, không dep; bài học "không jitter" | ADR-0043 |
| Mirror gate assert `Report::driven` — một harness lái mạnh có thể làm score giả | `DESIGN.md` §6 hàng mirror |

## Cách làm — hình dạng dự kiến

**`crates/engine/tests/sim/`** (module dùng chung bởi vài file test):

- `FaultyLoopback`: bọc `Loopback`; một **lịch** sinh từ seed (xorshift64, ~10 dòng, không
  dep): mỗi `recv`/`send` có thể trả `Idle` (chia nhỏ tới 1 byte), trì hoãn k turn, `Closed` giữa
  message, lặp lại một đoạn byte (duplicate delivery — TCP không làm, nhưng một proxy có thể),
  đảo hai message (không — TCP không làm; **không** tiêm lỗi TCP không có, ghi rõ).
- `World`: acceptor engine + initiator engine (`connect_and_serve`-shaped nhưng gọi `turn` tay),
  `ManualClock` chung nhảy theo lịch (0 ms, 1 ms, qua `HeartBtInt`, qua biên schedule), một
  `Application` mỗi bên gửi order/ER theo seed, **`FileJournal` thật vào tempdir** để restart
  có nghĩa.
- **Hành động** theo seed: gửi app message, kill một bên (drop engine, giữ journal), resume từ
  journal (`serve_with_recovery` shape), `Admin::SetNextOut`/`SendSequenceReset`, `shutdown`.
- **Bất biến**, kiểm sau **mỗi** turn, mỗi cái là một hàm có tên:
  1. không panic (đã có lint, nhưng harness là nơi nó bị ép);
  2. `next_out` mỗi bên đơn điệu không giảm trừ khi `141=Y`/`SequenceReset-Reset` xảy ra
     trong turn đó;
  3. mọi message app bên A gửi mà bên B **đã** `mark_in` thì B đã giao cho application đúng
     một lần **không** `43=Y`, hoặc ≥ 1 lần với `43=Y` (ADR-0017 nói vậy — kiểm đúng câu đó);
  4. `journal.highest() == next_out - 1` hoặc `None` khi chưa gửi app;
  5. sau khi lịch lỗi **ngừng** và 2 × `HeartBtInt` trôi qua: cả hai `LoggedOn`, `next_in` A ==
     `next_out` B và ngược lại ("hội tụ");
  6. mọi `Link::Dropped` có `DropReason` (không `EndedWithoutReason` — ADR-0035 đã làm nó
     thành lỗi nhìn thấy được);
  7. `benches`-style: 0 alloc trên cả hai engine trong cửa sổ (dùng cùng counting allocator).
- Chạy: `N` seed cố định trong CI (`cargo test --test sim`, ~10 s), và `SIM_SEEDS=10000` cho
  chạy dài; seed thất bại in **kịch bản tối thiểu** (lịch lỗi + hành động) để tái hiện — không
  có shrink tự động ở v1, chỉ in lịch.

**`fuzz/fuzz_targets/session.rs`**: byte tuỳ ý → cắt thành chuỗi `Input` (message/tick/
disconnect) cho một `Session<Acceptor, 256>` với `Store` — mục tiêu: không panic, `next_out`
đơn điệu, và **0 alloc** (counting allocator trong fuzz). Nightly, cùng chỗ với `parse`.

## Bất biến bị đụng tới

Không chạm code ship. Nhưng: harness **không được** làm gate mù — mirror gate đã dạy rằng một
harness "lái" được có thể làm session sai trông đúng. Vì vậy: harness **không trả lời thay
session** (không gửi `Logout`, không gap fill hộ); nó chỉ tiêm lỗi vận chuyển và hành động
operator có thật.

## Chia việc (dự kiến)

| Bước | Kết quả |
|---|---|
| 1 | `FaultyLoopback` + seed; test đơn: với seed 0 (không lỗi) 59-style logon/order/logout hội tụ |
| 2 | `World` + 7 bất biến; 100 seed trong CI; **ghi seed đầu tiên đỏ** — kỳ vọng có, vì đó là lý do plan tồn tại |
| 3 | Restart/resume và schedule trong lịch hành động |
| 4 | `fuzz_targets/session.rs`; job nightly |
| 5 | Docs: `DESIGN.md` §6 hàng "simulation, N seeds, invariants", `CONFORMANCE.md` §6 sửa dòng "corpus is not adversarial" thành "…and the simulation is", `reference/a-conformance-corpus-is-not-an-adversarial-one.md` ghi chú, `STATUS.md` |

## Cách kiểm chứng

Reversal bắt buộc: tắt một bất biến (ví dụ 3) và tiêm một lỗi cố ý vào một bản copy của
session (giao message hai lần không `43=Y`) — harness **phải** đỏ ở đúng bất biến, với seed và
lịch in ra. Nếu 1 000 seed xanh **và** reversal xanh → harness không nhìn thấy gì, không được
merge.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Harness lái session (gửi hộ) → score giả | code review + rule: harness chỉ có `Transport` và `Admin` |
| Tiêm lỗi TCP không có (đảo thứ tự) → đuổi theo lỗi không thật | danh sách lỗi cho phép ghi trong rustdoc, và một test cho từng loại |
| Seed thất bại không tái hiện được vì thời gian thật lọt vào | `ManualClock` là clock **duy nhất**; `Instant::now` cấm trong `sim/` (grep trong test) |
| CI chậm | 100 seed ≤ 10 s; dài hơn là nightly |
| Bất biến 5 sai khi lịch có `ResetOnDisconnect` (đợt B) | hội tụ định nghĩa "bằng nhau sau reset", không "bằng số cũ" |

## Ngoài phạm vi

Shrinking tự động; mô phỏng nhiều hơn hai engine; mô phỏng `shard` (Linux, thread thật —
không tất định); một DSL kịch bản.

## Nhật ký giao hàng

*(draft — chưa duyệt, chưa bắt đầu)*
