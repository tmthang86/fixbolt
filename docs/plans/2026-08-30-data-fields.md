# DATA field: đường ghi, và bên trong repeating group

> **Loại:** Plan · **Ngày:** 2026-08-30 · **Trạng thái:** Đã duyệt (2026-08-30)
> **Phạm vi:** open item 8, 9 — `codec` và `dict`

## Bối cảnh

DATA field là chỗ **duy nhất** trong FIX tag=value mà giá trị được phép chứa chính ký tự phân
cách `0x01`. Vì thế độ dài của nó không đọc được từ dữ liệu — nó phải lấy từ một field độ dài
đứng **ngay trước**. Đọc sai một byte là cắt nhầm cả message.

Đường **đọc** đã làm và có test. Còn thiếu hai mảnh, và cả hai đều là mảnh mà một counterparty
thật sẽ chạm vào ngay ngày đầu:

- **Đường ghi không có bất biến nào.** Ghi một DATA field động phải tự sinh lại field độ dài,
  đặt nó ngay trước, và đếm byte kể cả `0x01` nhúng bên trong. Hôm nay chưa có gì bắt buộc
  điều đó — `Template` chỉ xếp field theo thứ tự từ điển.
- **DATA nằm trong repeating group chưa được thử ở cả hai đường.**
  `crates/codec/tests/group_roundtrip.rs` **cố ý bỏ qua** mọi member kiểu DATA (dòng 80 ghi rõ
  lý do), nên 357 vị trí round-trip byte-identical kia không nói gì về trường hợp này.

Corpus không cứu được ở đây: **không một file `.def` nào có DATA message**. Nên đây là chỗ mà
`CLAUDE.md` §7 "real captures over invented messages" **không thể** áp dụng, và plan phải nói
thẳng điều đó thay vì giả vờ.

## Những gì đã biết chắc

- `crates/codec/src/dict.rs` có `Dictionary::data_length_tag(tag) -> Option<u32>`, và
  `crates/codec/src/parse.rs:260` dùng nó để lấy độ dài thay vì quét `0x01`.
- `crates/dict` sinh **16 cặp DATA→length** từ `FIX44.xml`.
- **Quy tắc `tag - 1` là sai** và đã trả giá một lần: `Signature(89)` lấy độ dài từ
  `SignatureLength(93)`, không phải 88. Ghi ở
  [reference/fix44-dictionary-traps.md](../reference/fix44-dictionary-traps.md).
- `crates/codec/tests/data_fields.rs` đã che đường đọc ở mức top-level, gồm ba cách parser có
  thể bị dắt ra ngoài biên. Header file nói rõ các frame này **được dựng theo spec, không phải
  dữ liệu thật**.
- `crates/codec/tests/group_roundtrip.rs:80` bỏ qua DATA member vì "cần field độ dài đặt ngay
  trước" — tức là item 9 nhìn từ bên trong group.
- `dict` sinh bảng group theo `(msg_type, counter)`: **59 counter, 731 vị trí**, lồng tới độ
  sâu 4.
- `crates/codec/src/template.rs` có `TemplateBuilder::{field, slot, group}` và
  `Template::{encode, encode_with}`. Không có gì trong đó biết DATA là gì.

## Cách làm

**Item 9 trước, vì item 8 là item 9 nhìn từ trong group.**

### 9 — bất biến DATA trên đường ghi

`TemplateBuilder::build::<D>()` đã nhận dictionary, nên nó **biết** tag nào là DATA. Ba quy tắc,
ép ở lúc build và lúc encode:

1. Khai báo một slot có tag DATA mà template không có slot cho tag độ dài của nó → `build`
   trả `EncodeError`. Sai này bắt được lúc dựng template, tức là một lần lúc khởi động, không
   phải mỗi message.
2. Field độ dài phải nằm **ngay trước** field DATA trong template. Không phải "đâu đó phía
   trước" — spec nói ngay trước.
3. Lúc encode, giá trị của field độ dài **do encoder tự ghi** từ độ dài thật của slot DATA,
   không lấy từ caller. Caller không được phép đưa vào một con số sai.

Quy tắc 3 là cái quan trọng nhất và cũng là cái dễ quên nhất: nếu caller vẫn ghi được độ dài
thì bất biến chỉ là lời khuyên.

### 8 — DATA bên trong group

Ba quy tắc trên áp y nguyên cho member của group, cộng thêm một chuyện chỉ có ở trong group:
**field độ dài và field DATA không được nằm hai bên ranh giới entry**. Nếu delimiter của group
rơi vào giữa cặp đó thì entry bị cắt sai.

Bỏ dòng bỏ-qua ở `group_roundtrip.rs:80` và cho DATA member chạy như mọi member khác.

## Bất biến bị đụng tới

- **Số 1** (không cấp phát trên hot path). Encoder ghi độ dài bằng `render_u32` sẵn có vào
  buffer của caller. `benches/alloc.rs` phải vẫn ra 0 trên mọi đường.
- **Số 5** (thứ tự field lấy từ bảng sinh ra, không từ call site). Quy tắc "độ dài ngay trước
  DATA" là một ràng buộc **thêm** vào thứ tự của từ điển, không thay nó. Nếu từ điển và quy tắc
  này mâu thuẫn ở một message nào đó thì **dừng lại** — đó là một phát hiện về từ điển, không
  phải chuyện chọn bên.
- **Số 7** (không `unwrap`/`expect`/`panic`). Lỗi build template là `EncodeError`, không panic.
- **Số 10**. Không công bố số đo hiệu năng nào ở đây, trừ khi `benches/alloc.rs` đổi.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | Test đỏ trước: một template khai báo DATA mà thiếu field độ dài, và một template đặt độ dài không sát trước. Cả hai hiện **đang qua** — phải thấy chúng đỏ sau khi thêm assert | — |
| 2 | **9 xong.** Ba quy tắc ép trong `TemplateBuilder::build` và `Template::encode_with`. Round-trip: ghi DATA có `0x01` nhúng, đọc lại ra đúng byte | 1 |
| 3 | Test đỏ trước cho group: một entry có DATA member, hiện `group_roundtrip.rs` bỏ qua | 2 |
| 4 | **8 xong.** DATA member chạy trong group, lồng tới độ sâu 4, và cặp độ dài–DATA không bao giờ bị delimiter cắt ngang | 3 |
| 5 | `benches/alloc.rs` thêm case DATA, ra 0, chứng minh bằng injection | 4 |

## Cách kiểm chứng

- **Round-trip byte-identical** với giá trị DATA **có chứa `0x01`** — nếu giá trị không chứa
  `0x01` thì test này không phân biệt được DATA với một field thường, và cả bài toán biến mất.
  Đây là "fixture đồng ý với bug" ở dạng thuần nhất.
- **Đảo ngược, từng quy tắc một.** Bỏ quy tắc "ngay trước" → test đỏ. Cho caller ghi đè độ dài
  → test đỏ. Bỏ kiểm tra delimiter trong group → test đỏ. Mỗi lần đảo phải **xác nhận cái sửa
  đã thật sự vào file** trước khi kết luận, vì một reversal không làm gì cũng báo PASS.
- **Đối chiếu với QuickFIX.** `crates/dict/tests/interop_quickfix_order.rs` đã so thứ tự group
  với C++ sinh sẵn của QuickFIX trên 730/730 group. Thêm khẳng định: ở mọi group có DATA
  member, thứ tự của QuickFIX **cũng** đặt length ngay trước data. Nếu không trùng thì đó là
  phát hiện, ghi vào `docs/reference/` ngay.
- `cargo test --all` và `cargo test --no-default-features` mỗi bước.

## Tài liệu phải cập nhật

- [ ] `DESIGN.md` §4 — hành vi codec đổi (đường ghi có bất biến mới)
- [ ] `CHANGELOG.md` — `TemplateBuilder::build` giờ có thể trả lỗi ở trường hợp mới
- [ ] rustdoc của `TemplateBuilder` và `Template`
- [ ] `docs/reference/fix44-dictionary-traps.md` — nếu lộ ra bẫy mới
- [ ] `STATUS.md` — đóng item 8 và 9
- [ ] `PRD.md` — nếu điều này chạm vào một tiêu chí thoát phase 1

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Test dùng giá trị DATA không chứa `0x01` → chứng minh được đúng con số 0 | Test bắt buộc có `0x01` nhúng; đảo ngược bằng cách bỏ nó ra và xem test mất hết ý nghĩa |
| Độ dài đếm bằng ký tự thay vì byte | Case có byte non-ASCII |
| Độ dài đúng nhưng `9=` (BodyLength) sai vì quên đếm DATA | Round-trip qua chính parser, có bật validation `9=` |
| Cặp length–DATA bị delimiter group cắt ngang | Test có DATA là member **đầu tiên** và **cuối cùng** của entry |
| Bảng 16 cặp DATA của `dict` sai một cặp → mọi test đều tự nhất quán và đều sai | Đối chiếu với QuickFIX C++, như đã làm cho tag number và enum |
| Quy tắc mới làm hỏng message không có DATA | 357 vị trí round-trip cũ phải xanh **không sửa một dòng nào** |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Không có capture thật nào để đối chiếu | Cao, không gỡ được | Nói thẳng trong tài liệu: đường DATA được canh bằng spec + oracle QuickFIX, **không** bằng dữ liệu thật. Đây là bằng chứng yếu nhất của crate và phải ghi vào mục "Not proven" của `STATUS.md` |
| Ràng buộc "ngay trước" mâu thuẫn với thứ tự từ điển ở một message | Trung bình | Dừng, ghi lại, xin duyệt lại plan. Không tự chọn bên |
| `EncodeError` mới làm vỡ caller hiện có | Thấp | Chưa có caller ngoài test; `CHANGELOG.md` ghi lại |

## Ngoài phạm vi

- **Không** đụng đường đọc top-level (đã có test, đã xanh).
- **Không** thêm kiểu DATA mới ngoài 16 cặp `dict` đang sinh.
- **Không** tối ưu tốc độ encode (item 11 — plan khác).
- **Không** đụng session, engine, transport.

## Nhật ký giao hàng

**Duyệt 2026-08-30.** Chủ dự án duyệt cả sáu plan cùng lúc, kèm một uỷ quyền ghi rõ:
*trong quá trình làm, nếu plan sai thì được sửa plan theo tình hình thực tế.* Điều đó nới
`CLAUDE.md` §1 — chỗ bảo "dừng lại, sửa plan, xin duyệt lại" — thành "sửa plan, **ghi lại
vào đây**, đi tiếp". Mỗi lần sửa plan phải có một mục dưới đây nói rõ **sửa gì và vì sao**,
nếu không thì uỷ quyền này biến thành giấy phép đi chệch trong im lặng.
