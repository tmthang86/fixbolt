# Template dựng lúc build: đóng item 34

> **Loại:** Plan · **Ngày:** 2026-09-04 · **Trạng thái:** Draft
> **Sửa phạm vi `[2026-09-05]`, trước khi thành plan:** item 34 **đã đóng** bằng
> [ADR-0051](../decisions/ADR-0051-item-34-is-a-third-of-the-size-it-was-recorded-at.md) — trên
> máy §9 `library, reply only` = **804.1 ns** so với `encode ExecutionReport (template)` =
> **237.6 ns**, tỉ lệ **3.4×**, không phải 19–24×; con số *40 ns* draft này dựa vào **không có
> benchmark committed** và bị rút lại. Phần materialise template mỗi message ≈ **570 ns = 2.9%**
> vòng w2w app. Nên draft này, nếu còn làm, là *"một tối ưu đáng ≤ 570 ns mỗi reply"* chứ không
> phải *"đóng item 34"*, và mọi mục tiêu bên dưới phải đo lại theo bốn số §9 đó. Chủ dự án chọn
> không làm bây giờ.
>
> **Phạm vi (như viết 2026-09-04):** `STATUS.md` item 45, đợt D, plan thứ nhất; ~~đóng **item 34**~~. Chạm `codec`
> (`Template`, `TemplateBuilder` — hot path), `dict` (sinh thêm bảng), `library` (`Reply`,
> `App`), benches. **Không chạm** `session`, `engine`, `transport`.
>
> **Draft viết 2026-09-04.** Khi đến lượt: đọc lại `reply.rs`, ADR-0041, ADR-0044 và **số 766 ns**
> (đo trên VM Xeon không đạt §9 — cần đo lại trên §9 ở đợt C trước khi chọn mục tiêu). Plan này
> **phải xong trước ADR encoding của phase 2**, vì ADR đó định hình `Template`. Sửa rồi *Chờ duyệt*.
>
> **Máy chạy:** macOS để làm; **số cuối cần máy §9**. **Thời lượng dự kiến:** 2–3 ngày.

## Bối cảnh

`library` mua một API tiện — `reply.message(b"8").field(11, id).field(150, b"0").send()` — bằng
việc dựng một `Template` **mỗi message**: `TemplateBuilder::new` → `field` × n → `build::<Fix44>()`
sắp xếp theo bảng (`crates/library/src/reply.rs:169–244`). ADR-0044 đã bỏ một nửa chi phí (không
move struct mỗi field): **766 ns** một reply 12 field so với **40 ns** patch một template đã
dựng (`[measured 2026-09-02, Xeon VM không đạt §9]`). ~19× còn lại là *materialise `Template`
per message*. `GUIDE.md` §1b nói thẳng "the fast one is not the pretty one" — plan này làm cái
đẹp thành cái nhanh.

## Những gì đã biết chắc (2026-09-04 — xác minh lại khi làm)

| Sự thật | Nguồn |
|---|---|
| `Template<P, S> { scratch: [u8; S], parts: [Part; P], len }`; `encode(out, slots)` patch, không sắp | D9, `crates/codec/src/template.rs` |
| `TemplateBuilder::build::<D>()` sắp field theo `D::position`/`group_order` — một lần, lúc build | D3, D9 |
| `dict` sinh: tag constants, message shapes, required tables, **field ordering**, group delimiters/members, 4 bảng validation | `DESIGN.md` §3 |
| Skeleton một message phụ thuộc **session** (`49`, `56`) và **msg type**; phần thân phụ thuộc field nào caller đưa | D9 |
| `App<H, N, P, S>` giữ `idx` dùng lại; `Reply` sinh mỗi message | `crates/library/src/app.rs:123` |
| Đường nhanh: `Application::on_message` + `Template` dựng một lần ở handler; `w2w --path app` dùng cách này | ADR-0041, `w2w` |
| Baseline `encode ExecutionReport (template)` 239.1 ns §9; sàn của hình `Part` ~116 ns | `baselines.tsv`, ADR-0016 |
| `benches/cost.rs` của `library`: `parse only` 146, `reply only` 766, `on_message` 956 (Xeon VM) | ADR-0044 |

## Cách làm — hai bước độc lập, đo riêng

**Bước A — cache template theo `(MsgType, tập field)` trong `App`, không đổi codec.**
`App` giữ một mảng cố định `K` template (`K = 16` msg type mặc định, const generic), khoá là
`msg_type` + **bitset các tag đã dùng** (một `u128` hoặc `[u64; 2]` trên chỉ số tag trong bảng
dict — không hash, không alloc). Lần đầu gặp một khoá: `build` như hôm nay và cất; các lần sau:
`encode` với slots. Message khác tập field là khoá khác. Đầy `K`: dựng như hôm nay, không cache
(đếm `cache_misses` để thấy). **Kỳ vọng**: reply ~40–60 ns + tra cache, tức đóng item 34 gần
hết mà không chạm `codec`.

**Bước B — `dict` sinh bảng vị trí để `build` không phải sắp.** `D::position(msg_type, tag) ->
Option<u16>` hôm nay là tra bảng; `build` sắp `parts` bằng so sánh vị trí. Thay bằng **đặt
thẳng vào chỗ**: `parts` là mảng thưa theo vị trí (`P` = số vị trí của msg type, sinh bởi
`dict` là `const`), `field(tag)` ghi vào `parts[position]`, `build` chỉ nén. Đây là phần
"dựng lúc build" thật: với A xong, B chỉ còn đáng nếu số đo nói `build` vẫn lộ ra (cache miss
lần đầu, hoặc `K` không đủ). **Quyết định làm B hay không là số của A trên §9.**

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **1 — không cấp phát** | cache trong `App` | mảng cố định; `library` alloc bench 3 đường vẫn 0, thêm `reply-cached` |
| **5 — thứ tự từ bảng** | B đổi cách sắp | `group_roundtrip.rs` 357 vị trí byte-identical, `interop_quickfix_order.rs` 730/730 — **không đổi số** |
| 10 — số có máy | mục tiêu mới | đo trên §9; không lấy số VM làm gate |
| `MessageView` 24 byte, `no_std` codec | B thêm bảng `const` | `const _` assert; không `std` |

## Chia việc (dự kiến)

| Bước | Kết quả |
|---|---|
| 1 | `benches/cost.rs` của `library`: arm `reply, second time same shape` — **đỏ về ý nghĩa**: hôm nay bằng `reply only` |
| 2 | Cache trong `App`; test `crates/library/tests/cache.rs`: cùng shape → cùng byte ra; khác shape → khác template; đầy `K` → vẫn đúng byte; `cache_misses` đếm |
| 3 | Đo trên §9 (đợt C hoặc một lần riêng): điền ADR-0041 ghi chú ngày, item 34 |
| 4 | **Quyết định B** bằng số; nếu làm: `dict` sinh `POSITIONS`, `TemplateBuilder` đặt thẳng, `group_roundtrip` + `interop_quickfix_order` xanh nguyên, `serialize.rs` baseline mới |
| 5 | Docs: `GUIDE.md` §1b viết lại đoạn "the fast one is not the pretty one" theo số mới; `DESIGN.md` D9 *As built*, §6 hàng; `CHANGELOG.md`; `STATUS.md` |

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Khoá cache chỉ theo `msg_type` → hai reply khác tập field dùng nhầm template, field thiếu **im lặng** | khoá có bitset; test "khác shape → khác byte" |
| Cache theo `(49,56)` — `App` là per-engine, không per-session; hai counterparty, hai skeleton | khoá gồm connection id **hoặc** skeleton per session dựng ở `opened`; test hai counterparty qua socket (`settings_wire.rs` đã có hai) |
| Bench đo cache nóng và tuyên bố là số reply | `cost.rs` in cả `first` và `second time`; §8 ghi cả hai |
| B đổi thứ tự group | 357 vị trí byte-identical là gate |

## Ngoài phạm vi

Typed message structs sinh từ `dict` (hay, nhưng là API mới — plan riêng nếu phase 2 cần);
`Reply` cho initiator-originated messages (đã qua `send_application`).

## Nhật ký giao hàng

*(draft — chưa duyệt, chưa bắt đầu)*
