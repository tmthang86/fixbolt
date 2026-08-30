# Ring đầy thì làm gì

> **Loại:** Plan · **Ngày:** 2026-08-30 · **Trạng thái:** Đã duyệt (2026-08-30)
> **Phạm vi:** open item 5 — `engine`, và một ADR

## Bối cảnh

`DESIGN.md` D10 (backpressure phía gửi) đã dựng xong và **không phải** cái này. D10 trả lời câu
"người nhận trên dây chậm thì làm gì" — `Disconnect`, `Queue { max_bytes }`, `Block`. Item 5
hỏi một câu khác hẳn: **ứng dụng ở đầu kia của ring chậm thì làm gì.**

Hôm nay câu trả lời là: đếm. `RingDispatch::refused()` đếm message ring không nhận, và doc
comment của nó nói thẳng là *"cái phải làm gì với một ring đầy là câu hỏi của DESIGN.md D10"* —
tức là chính nó cũng biết mình chưa trả lời. Đếm là một câu trả lời hợp lệ **cho một
benchmark**; nó không phải một câu trả lời cho một engine đang chạy thật, vì một message bị từ
chối trong im lặng là một lệnh không bao giờ tới nơi.

Đây là nợ của ADR-0002 và của plan `engine`, và nó là một quyết định **có giá, khó đảo** —
nên nó cần ADR chứ không chỉ cần code.

## Những gì đã biết chắc

- `RingDispatch` có hai bộ đếm: `refused()` (ring không nhận, vì đầy hoặc message dài hơn `M`)
  và `dropped()` (trả lời về mà dài hơn `M`, nên mất). Cả hai là `usize`, không có ngưỡng,
  không có cảnh báo.
- **Đường inline là mặc định**, ring là tuỳ chọn — ADR-0002 đã đảo chiều theo hướng đó.
- **Một lần nhảy ring tốn ~50× lần gọi inline:** inline 2,7 ns, ring một chiều 128,0 ns, khứ
  hồi 242,5 ns (M5, không ghim core). Nghĩa là ai chọn ring thì đã chấp nhận cái giá đó vì lý
  do khác — thường là ứng dụng của họ có thể khựng hàng mili-giây.
- **Ring là `AtomicU8`, không dùng `unsafe`** — [ADR-0007](../decisions/ADR-0007-spsc-ring-without-unsafe.md).
  Copy từng byte, ~0,8 ns/byte.
- **D10 đã có sẵn ba hình dạng chính sách** (`Disconnect`, `Queue`, `Block`) trong
  `crates/engine/src/backpressure.rs`, và `Block` đã có tiền lệ quay vòng trên socket
  (`conn.rs:397`). Hình dạng đó dùng lại được; ngữ nghĩa thì không.
- **Không có test nào hiện chạy ring tới lúc đầy.** Bộ đếm `refused` chưa từng được quan sát
  khác 0 trong một test có chủ đích.

## Cách làm

1. **ADR-0011 trước, code sau.** ADR nêu ba lựa chọn và chọn một, kèm hệ quả cả tốt lẫn xấu:
   - *Từ chối và đếm* (hôm nay) — engine không bao giờ khựng, ứng dụng mất lệnh trong im lặng.
   - *Ngắt kết nối* — counterparty biết ngay có chuyện; mất mọi thứ đang bay.
   - *Chặn engine cho tới khi có chỗ* — không mất gì; **vi phạm tinh thần của bất di bất dịch
     số 4**, vì luồng engine đứng chờ ứng dụng.
   Không có lựa chọn nào miễn phí, và ADR phải nói rõ cái giá chứ không chỉ nói cái lợi.
2. **Trước khi chọn, phải đo.** Ring đầy trong bao lâu thì lấp đầy được, ở kích thước nào?
   Một benchmark bơm nhanh hơn tốc độ rút, đọc `refused()`, và cho ra con số. Chọn chính sách
   mà không biết ring đầy nhanh cỡ nào là chọn mù.
3. Cài chính sách đã chọn, mặc định là cái an toàn nhất cho một acceptor thật.
4. **Bộ đếm phải nói ra được.** Dù chọn gì, `refused` khác 0 phải quan sát được từ ngoài — nếu
   không thì đây vẫn là một thất bại im lặng, chỉ là có thêm một trường trong struct.

## Bất biến bị đụng tới

- **Số 4** (*luồng engine không bao giờ ngủ trong kernel*). Nếu ADR chọn "chặn", phải nói rõ
  đó là quay vòng chứ không phải ngủ, và phải chứng minh bằng cổng ở plan `w2w-and-linux-numbers`.
  Nếu không chứng minh được thì **không được chọn lựa chọn đó**.
- **Số 1** (không cấp phát trên hot path). Đường ring hiện ra 0; phải giữ 0.
- **Số 7** — ring đầy không được panic.
- **Số 10** — con số ở bước 2 phải kèm benchmark, máy và cấu hình.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | Benchmark làm ring đầy có chủ đích; `refused()` khác 0 và **được quan sát**, không suy ra | — |
| 2 | **ADR-0011** đề xuất, dựa trên số ở bước 1. Chờ duyệt | 1 |
| 3 | Cài chính sách đã chọn; test cho từng nhánh | 2 |
| 4 | Bộ đếm quan sát được từ ngoài; test đảo ngược chứng minh nó tăng đúng lúc | 3 |

## Cách kiểm chứng

- **Bước 1 phải nhìn thấy `refused()` tăng.** Một benchmark báo `refused == 0` rồi kết luận
  "ring không bao giờ đầy" là đúng cái bẫy đã trả giá ở `benches/alloc.rs`: con số 0 chỉ có
  nghĩa khi có thứ khác chứng minh đường đó **đã chạy**. Nên assert cả hai: ring đã nhận > 0
  message, **và** đã từ chối > 0.
- **Từng nhánh chính sách một test**, và mỗi test đảo ngược được.
- Nếu chọn "chặn": phải chạy cổng "không ngủ trong kernel" của plan `w2w` và thấy nó **xanh**.
  Chưa có cổng đó thì chưa được chọn nhánh đó.
- `cargo test --all`, `--no-default-features`, `benches/alloc.rs`, `benches/dispatch.rs` mỗi
  bước. Cổng wire 59/59 không được đổi.

## Tài liệu phải cập nhật

- [ ] `DESIGN.md` §4 — D10 tách rõ hai câu hỏi: chậm trên dây, và chậm ở ứng dụng
- [ ] `docs/decisions/ADR-0011-*.md` — mới; ADR-0002 cần một dòng trỏ tới
- [ ] `CHANGELOG.md` — nếu API `RingDispatch` đổi
- [ ] rustdoc `RingDispatch` — bỏ câu "đây là câu hỏi của D10", vì nó sẽ đã được trả lời
- [ ] `STATUS.md` — đóng item 5
- [ ] `docs/reference/measured-costs.md` — số đo ở bước 1

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Benchmark báo `refused == 0` và bị đọc là "không bao giờ đầy" | Assert đồng thời: đã nhận > 0 **và** đã từ chối > 0 |
| Chọn "chặn" rồi vô tình đưa một chỗ ngủ vào luồng engine | Cổng `strace` của plan `w2w` |
| Chính sách mới làm hỏng tính byte-identical giữa inline và ring | `crates/engine/tests/dispatch.rs` — cùng message, cùng output |
| Trả lời cho một kết nối đã ngắt lại đi tới người vừa lấy slot đó | Test đã có (định tuyến theo id, `swap_remove` dùng lại index) — phải giữ xanh |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Không có ứng dụng thật nào để biết ring đầy thật sự trông thế nào | Cao | ADR quyết trên lập luận + số bench, và **nói rõ là chưa gặp tải thật** |
| Chọn xong lại phải đảo khi có counterparty thật | Trung bình | ADR mới supersede, không sửa ADR đã accept |

## Ngoài phạm vi

- **Không** đụng D10 phía socket — cái đó đã xong.
- **Không** đổi ring sang `unsafe` để nhanh hơn (ADR-0007 đã quyết; đảo thì cần ADR mới).
- **Không** đụng inline dispatch.

## Nhật ký giao hàng

**Duyệt 2026-08-30.** Chủ dự án duyệt cả sáu plan cùng lúc, kèm một uỷ quyền ghi rõ:
*trong quá trình làm, nếu plan sai thì được sửa plan theo tình hình thực tế.* Điều đó nới
`CLAUDE.md` §1 — chỗ bảo "dừng lại, sửa plan, xin duyệt lại" — thành "sửa plan, **ghi lại
vào đây**, đi tiếp". Mỗi lần sửa plan phải có một mục dưới đây nói rõ **sửa gì và vì sao**,
nếu không thì uỷ quyền này biến thành giấy phép đi chệch trong im lặng.
