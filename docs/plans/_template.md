# <Tiêu đề>

> **Loại:** Plan · **Ngày:** YYYY-MM-DD · **Trạng thái:** Draft | Chờ duyệt | Đã duyệt | Xong | Bỏ
> **Phạm vi:** <milestone hoặc mảng nào>

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Vì sao làm việc này? Giải quyết vấn đề gì, cái gì châm ngòi, muốn đạt kết quả gì?

Viết sao cho người mới vào sau sáu tháng hiểu được động cơ mà không phải đi hỏi ai.

## Những gì đã biết chắc

Sự thật đã xác lập trước khi lập kế hoạch — trích dẫn đặc tả, số đo được, code có sẵn dùng lại
được, ràng buộc. Ghi rõ nguồn: đường dẫn file, số mục trong spec, số hiệu ADR, con số đã đo.

**Mục này không được có phỏng đoán.** Cái gì là giả định thì đưa xuống mục Rủi ro.

## Cách làm

Chỉ ghi phương án được chọn. Phương án đã cân nhắc rồi loại thuộc về ADR, không thuộc về đây.

Nêu tên những file sẽ tạo hoặc sửa.

## Bất biến bị đụng tới

Trong mười điều bất di bất dịch ở `CLAUDE.md` §2, việc này có thể ảnh hưởng cái nào, và giữ nguyên chúng bằng
cách nào.

Chỉ ghi "không" khi việc này không đụng vào `codec`, `session`, `engine` lẫn `transport`.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | | |
| 2 | | |

## Cách kiểm chứng

Từng bước được chứng minh chạy đúng bằng cách nào. Lệnh nào chạy, output thế nào thì coi là đạt,
phát lại bản ghi nào.

**"Test pass" một mình chưa đủ** — phải nêu rõ đã chạy thử với bản ghi thật hoặc feed thật ra sao.

## Tài liệu phải cập nhật

Theo bảng đồng bộ ở `CLAUDE.md` §4.

- [ ] …

## Bẫy đã lường trước

Nêu tên trước khi bắt đầu, mỗi cái kèm test canh nó. Xem `CLAUDE.md` §10.

| Bẫy | Test canh |
|---|---|
| | |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| | | |

## Ngoài phạm vi

Việc này cố tình KHÔNG làm những gì — để phạm vi phình ra thì nhìn thấy được, chứ không âm thầm.

## Nhật ký giao hàng

Điền vào mỗi khi đóng một phase: đã dựng gì, ở đâu, gate nào xanh, cái gì chưa làm và vì sao.

**Đây là phần sống sót qua nén context** — phiên sau đọc mục này trước tiên.
