# Cuốn nhật ký trả lời được câu hỏi gì

> **Loại:** Plan · **Ngày:** 2026-09-02 · **Trạng thái:** Xong
> *(tự viết, tự duyệt theo uỷ quyền thường trực 2026-09-01.)*
>
> **Phạm vi:** `STATUS.md` item 30 (e). Chạm `engine` (module `journal`) và thêm **một crate
> nhị phân mới** `tools/jrnl`. Không chạm `codec`, `dict`, `session`, `transport`.
>
> **Máy chạy:** đóng trọn vẹn trên macOS.

## Bối cảnh

**Câu hỏi mà mọi bộ phận vận hành đều bị hỏi:** *"chúng tôi gửi lệnh X lúc 10:32, các anh có
nhận được không?"* Hôm nay câu trả lời là **không biết**.

`[verified 2026-09-02]` file journal có định dạng rõ ràng — `[seq:u32-le][len:u32-le][bytes]`,
`len == 0` là dấu inbound (ADR-0017) — nhưng **thứ duy nhất đọc được nó là `FileJournal::open`**,
và nó nạp vào một ring cố định `<N, LEN>`. Ring giữ N message gần nhất. Câu hỏi ở trên là về một
message *có thể đã rất cũ*, nên ring là sai công cụ. Và muốn mở file thì phải là **tiến trình
Rust biết đúng `N` và `LEN`** — người trực đêm không có cái nào trong ba thứ đó.

**Và có một chỗ giấu sự thật.** Vòng lặp đọc đếm số bản ghi rách ở đuôi vào biến `torn`, rồi
`let _ = torn;`. Một tiến trình bị giết giữa lúc ghi để lại cái đuôi đó; hôm nay **không ai được
báo**. Với một cơ chế khôi phục thì bỏ qua là đúng — replay byte chưa từng lên dây còn tệ hơn
replay không gì cả — nhưng **im lặng thì không đúng**, và với một audit trail thì nó là lỗi.

## Những gì đã biết chắc

| Sự thật | Nguồn |
|---|---|
| Định dạng bản ghi: `[seq u32-le][len u32-le][bytes]`; `len == 0` là dấu inbound | `crates/engine/src/journal.rs`, hằng `RECORD_HEADER` / `INBOUND_MARK` |
| **Độ dài là thứ làm file đọc được.** `[measured 2026-08-30]` bản đầu không có nó và file không parse nổi | comment cùng file |
| Đuôi rách được **đếm rồi vứt đi** | `let _ = torn;` |
| Engine owes a byte stream, not an archive | [ADR-0027](../decisions/ADR-0027-the-engine-owes-a-byte-stream-not-an-archive.md) |
| Ring của `FileJournal` là `<N, LEN>` cố định, nạp lại khi mở | như trên |

## Quyết định trung tâm

**Đọc offline là một `Iterator`, không phải một ring.** `journal::Reader` mở file, đi từ đầu,
sinh từng bản ghi. Không `N`, không `LEN`, không giới hạn — vì câu hỏi là về *toàn bộ* lịch sử.
Nó **không** dùng lại `FileJournal`: hai mục đích khác nhau (khôi phục vs. tra cứu) và ADR-0027
đã nói engine nợ dòng byte chứ không nợ kho lưu trữ.

**Đuôi rách được báo, không bị nuốt.** `Reader` sinh ra một mục cuối nói rõ *"còn N byte không
thành bản ghi"*. Và `FileJournal::open` **thôi vứt `torn`** — nó trở thành thứ đọc được
(`FileJournal::torn_records()`), vì một tiến trình từng bị giết giữa lúc ghi là chuyện người vận
hành phải biết.

**Một nhị phân, vì đó chính là lời phàn nàn.** *"Không gì ngoài tiến trình đọc được"* không được
sửa bằng một hàm thư viện. `tools/jrnl` đọc file và in ra; nó **không** phụ thuộc `session` hay
`dict` — chỉ `engine::journal` — nên không kéo theo gì.

**Nó không diễn giải FIX.** In ra byte với `|` thay `SOH` và để `grep` làm việc của `grep`.
Diễn giải cần dictionary, và dictionary là một dependency mà một công cụ đọc file không cần.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **1 — không cấp phát** | `Reader` cấp phát thoải mái | **Nó không ở trên hot path** và không chạy trên luồng engine. Ghi rõ điều đó trong rustdoc. `FileJournal::torn_records()` chỉ đọc một số đã có |
| **6 — feature flag** | crate mới | `tools/jrnl` không có dependency tuỳ chọn nào; `check-no-optional-deps.sh` phải xanh với nó |
| **7 — không `unwrap`** | crate mới | `tools/` là nhị phân, nhưng vẫn theo lint của workspace |
| **3 — 59 định nghĩa** | không đụng session | vẫn chạy, phải 59/59 |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | **Test đặc tả, đỏ ở assertion.** Ghi 5 000 message vào một `FileJournal<8, 512>`, đóng lại, rồi hỏi *"message số 3 nói gì?"* — ring chỉ giữ 8 cái cuối nên hôm nay câu trả lời là không có | — |
| 2 | `journal::Reader` + `Record`. Đuôi rách là một mục, không phải im lặng. `FileJournal::torn_records()` | 1 |
| 3 | `tools/jrnl` — in tất cả, lọc theo seq, đếm | 2 |

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 1 | `cargo test -p fixbolt-engine --test journal_reader` | **đỏ ở assertion** |
| 2 | như trên | xanh; có test cho đuôi rách, dựng bằng cách **cắt file thật** |
| 3 | `cargo run -p jrnl -- <file>` trên file do test tạo | in đúng số bản ghi |
| mọi bước | `--test wire` 59/59 cả hai mode; `cargo test --all`; `check-no-optional-deps.sh`; clippy; fmt; links | xanh |

**Đảo ngược, bắt buộc:**

1. `Reader` dừng im lặng ở đuôi rách thay vì báo → phải có test đỏ.
2. `Reader` bỏ qua dấu inbound (`len == 0`) coi như bản ghi rỗng → phải có test đỏ, vì ADR-0017
   nói con số ấy nằm cùng chỗ.
3. `torn_records()` luôn trả 0 → phải có test đỏ.

**Bẫy đã lường trước:**

| Bẫy | Test canh |
|---|---|
| Test ghi rồi đọc **mà không đóng** journal → đọc trúng cache chứ không phải file | Thả `FileJournal` trước khi đọc, như `tests/recovery.rs` đã làm |
| `Durability::Async` ghi ở luồng khác, test đọc quá sớm | Dùng `Fsync` cho các test này và nói rõ lý do |
| Đuôi rách dựng bằng byte bịa → chứng minh parser xử lý thứ không ai tạo ra | **Cắt một file thật** do journal ghi ra, ở một offset giữa bản ghi |
| Số bản ghi đọc được đúng nhưng nội dung sai | So sánh nội dung message, không chỉ đếm |

## Tài liệu phải cập nhật

- [x] ADR mới — [ADR-0037](../decisions/ADR-0037-reading-a-journal-is-not-recovering-from-one.md)
- [x] `DESIGN.md` §3 (hai dòng: `tools/w2w` **cũng chưa từng có dòng nào**, và `tools/jrnl`) + `README.md` layout (mục `tools/` trước đó không tồn tại) + `Cargo.toml` members
- [x] `CHANGELOG.md`; `GUIDE.md`; `STATUS.md` item 30; `PRD.md`
- [x] Đi lại bảng §4, đọc lại *Not proven* — thêm bốn mục
- [x] `docs/reference/` — [the-strongest-knob-is-not-the-settle-point](../reference/the-strongest-knob-is-not-the-settle-point.md), gắn `[to testing-skills]`

## Ngoài phạm vi

- **Diễn giải FIX trong `jrnl`** — cần dictionary; `grep` làm được việc đó.
- **Nén, xoay vòng, hay dọn file** — ADR-0027: engine nợ dòng byte, không nợ kho.
- **Đọc trong khi engine đang ghi** — file đang được append; đọc song song là một plan khác.
