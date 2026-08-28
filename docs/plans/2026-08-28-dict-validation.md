# Bảng kiểm tra từ điển FIX 4.4 — bốn bảng còn thiếu, sinh từ XML

> **Loại:** Plan · **Ngày:** 2026-08-28 · **Trạng thái:** Đã duyệt 2026-08-28 · **Xong** 2026-08-28
> **Phạm vi:** Phase 1, **tiêu chí 3** của `PRD.md` §2. Chặn bước 3 của
> [plan session-layer](2026-08-28-session-layer.md).

## Bối cảnh

`PRD.md` §3 ghi thẳng: *"types and enum values are still not validated"* — tiêu chí 3 là một
**gap chưa có plan**. Hôm nay nó thành đường găng.

Bước 3 của plan session-layer là `Reject (35=3)` với 12 mã `373`, 13 file. Bắt tay vào mới thấy
**8 trong 12 mã đó không phải luật của session, mà là câu hỏi cho từ điển**:

| Corpus hỏi gì | Mã `373` | `dict` trả lời được chưa |
|---|---|---|
| `999=HI`, `0=HI`, `-1=HI`, `5000=HI` — tag này có tồn tại không? | 0 | **chưa** |
| `55=MSFT` trên `35=0` — tag này có thuộc bản tin này không? | 2 | **chưa** |
| `21=4`, `167=BOO`, `40=w` — giá trị này có nằm trong enum không? | 5 | **chưa** |
| `38=+200.00`, `126=20040415` — giá trị này có đúng kiểu không? | 6 | **chưa** |
| `35=*` — MsgType này có thật không? | 11 | **chưa** |
| thiếu `56`, thiếu `11` trên `35=D` | 1 | **rồi**, `required()` |
| `34` sai thứ tự header/body | 14 | **rồi**, `is_header()` |
| `386=3` mà chỉ có 2 phần tử | 16 | **rồi**, `GroupIter` |

Đây là thay đổi codegen **và** public API của `dict`. `CLAUDE.md` §1 bắt phải có plan riêng, và
tiêu chí 3 của PRD vốn đã là một tiêu chí độc lập với tiêu chí 1 — nên nó có plan của nó, có
cổng của nó. Cổng 59/59 **không nhìn thấy** phần lớn việc này: 912 trường, 59 định nghĩa chỉ
chạm tới vài chục.

## Những gì đã biết chắc

Đo ngày 2026-08-28 trên `vendor/quickfix/spec/FIX44.xml` và mã C++ QuickFIX tự sinh.

### Kích thước

| | Số đo |
|---|---|
| Trường trong FIX44.xml | **912**, tag từ **1** đến **956** |
| Kiểu dữ liệu khác nhau | **23** |
| Trường có enum | **245**, tổng **1 708** giá trị |
| Cặp (message, tag) hợp lệ, đã trải phẳng component và group | **12 524** trên **93** bản tin |
| Trường header | **30** |

Bảng "tag nào hợp lệ cho bản tin nào": bitset **15 × u64 = 120 byte** mỗi bản tin, **11 160
byte** cho cả 93 — nhỏ hơn mảng sắp xếp (12 524 × u16 = 25 048 byte) và tra cứu O(1) thay vì
O(log n). Chọn bitset.

### Ba oracle độc lập, và sức mạnh thật của từng cái

QuickFIX tự sinh mã C++ từ **cùng file XML nhưng bằng bộ sinh khác**. Đúng thủ thuật đã bắt được
lỗi thứ tự group (730/730). Đã kéo về và đối chiếu thử:

| Oracle | Nội dung | Đối chiếu với XML |
|---|---|---|
| `src/C++/FixFieldNumbers.h` | tên → số, 6 107 dòng cho mọi phiên bản | **912/912 khớp tuyệt đối.** Không một số nào lệch |
| `src/C++/FixFields.h` + `FixCommonFields.h` | kiểu của từng trường | 912/912 có mặt, **14 trường lệch tên kiểu** — đã liệt kê đủ bên dưới |
| `src/C++/FixValues.h` | hằng số enum | **yếu**: chỉ phủ 228/245 trường, và 95 trường lệch số lượng |
| `src/C++/fix44/*.h` `FIELD_SET` | tag của từng bản tin | **12 524 cặp trên 92 bản tin** — đúng bằng con số giải từ XML |

**14 trường lệch kiểu, tên đủ cả** — không cái nào bị corpus chạm tới, và tất cả đều là do
`FixFields.h` dùng chung cho mọi phiên bản nên lấy kiểu tinh hơn của bản sau:

| Tag | Tên | XML | QuickFIX |
|---|---|---|---|
| 10 | CheckSum | STRING | CHECKSUM |
| 18 | ExecInst | MULTIPLEVALUESTRING | MULTIPLECHARVALUE |
| 63 | SettlType | CHAR | STRING |
| 276 | QuoteCondition | MULTIPLEVALUESTRING | MULTIPLESTRINGVALUE |
| 277 | TradeCondition | MULTIPLEVALUESTRING | MULTIPLESTRINGVALUE |
| 286 | OpenCloseSettlFlag | MULTIPLEVALUESTRING | MULTIPLECHARVALUE |
| 291 | FinancialStatus | MULTIPLEVALUESTRING | MULTIPLECHARVALUE |
| 292 | CorporateAction | MULTIPLEVALUESTRING | MULTIPLECHARVALUE |
| 529 | OrderRestrictions | MULTIPLEVALUESTRING | MULTIPLECHARVALUE |
| 532 | MassCancelRejectReason | STRING | INT |
| 546 | Scope | MULTIPLEVALUESTRING | MULTIPLECHARVALUE |
| 587 | LegSettlType | CHAR | STRING |
| 674 | LegAllocAcctIDSource | STRING | INT |
| 877 | UnderlyingCPProgram | STRING | INT |

**XML là nguồn sự thật** (`ADR-0001`: XML là dữ liệu, C++ là oracle). 14 dòng này là danh sách
miễn trừ được viết ra và đếm được, không phải một `!=` bị làm ngơ.

### Một điều bất ngờ đã đo

`FixFieldNumbers.h` định nghĩa `UserMin = 5000, UserMax = 9999` — QuickFIX coi 5000–9999 là
**tag do người dùng tự định nghĩa**. Nhưng `14a_BadField.def` chờ `5000=HI` bị từ chối là
*Invalid tag number*. Vậy trong cấu hình của bộ acceptance, "tag hợp lệ" = "có trong FIX44.xml",
hết. Không có vùng người dùng. Ghi lại vì nó sẽ cắn người đọc code sau này.

## Cách làm

Bảng sinh ở `dict`, luật áp dụng ở `session`. Plan này chỉ làm bảng và API.

### Bốn bảng, và cái API đi kèm

`crates/dict/build.rs` sinh thêm, `crates/dict/src/lib.rs` phơi ra trên `Fix44`:

```rust
impl Fix44 {
    /// Tag có trong FIX44.xml không. → 373=0
    pub fn is_defined_tag(tag: u32) -> bool;
    /// MsgType có thật không. → 373=11
    pub fn is_msg_type(msg_type: &[u8]) -> bool;
    /// Tag này có thuộc bản tin này không (kể cả header và trailer). → 373=2
    pub fn allows(msg_type: &[u8], tag: u32) -> bool;
    /// Kiểu của trường. → 373=6
    pub fn field_type(tag: u32) -> Option<FieldType>;
    /// Giá trị có nằm trong enum không. `None` = trường này không có enum. → 373=5
    pub fn enum_allows(tag: u32, value: &[u8]) -> Option<bool>;
}
```

`FieldType` là enum 23 nhánh, `#[non_exhaustive]` **không** dùng — thêm nhánh là thay đổi phá vỡ
và phải thấy được. Kiểm tra kiểu (`FieldType::accepts(&[u8]) -> bool`) nằm ở `dict` chứ không ở
`session`: nó là thuộc tính của từ điển, và để một chỗ thì không có chỗ thứ hai bất đồng.

### Cách sinh

- **`is_defined_tag`**: bitset 15 × u64 trên toàn dải tag. Hằng số, tra bằng dịch bit.
- **`is_msg_type`**: `match` trên chuỗi, như `required()` hiện tại.
- **`allows`**: 93 bitset 120 byte, chọn bằng `match msg_type`. Header và trailer trộn sẵn vào
  từng bitset lúc sinh, để chỗ gọi không phải hỏi hai lần.
- **`field_type`**: `match tag` → `FieldType`. Trường không có trong XML trả `None`.
- **`enum_allows`**: giá trị enum là chuỗi độ dài bất kỳ (`167=EUSUPRA`), nên là mảng
  `&'static [&'static [u8]]` cho mỗi trường, `match tag`. 245 mảng, 1 708 phần tử.

### File đụng tới

| File | Việc |
|---|---|
| `scripts/fetch-quickfix-assets.sh` | mở rộng sparse-checkout thêm `FixFieldNumbers.h`, `FixFields.h`, `FixCommonFields.h`, `FixValues.h` |
| `crates/dict/build.rs` | sinh 4 bảng |
| `crates/dict/src/lib.rs` | `FieldType`, 5 hàm trên `Fix44` |
| `crates/dict/tests/interop_quickfix_fields.rs` | **mới** — đối chiếu số hiệu và kiểu với C++ sinh sẵn |
| `crates/dict/tests/interop_quickfix_messages.rs` | **mới** — đối chiếu 12 524 cặp với `FIELD_SET` |
| `crates/dict/tests/validation.rs` | **mới** — mọi ca mà corpus thật sự chạm tới |
| `crates/codec/benches/alloc.rs` | thêm ca: tra 5 hàm mới, phải in `0` |

## Bất biến bị đụng tới

| # | Cách giữ |
|---|---|
| 1 — không cấp phát | Tất cả là `&'static` và `match`. `benches/alloc.rs` thêm một ca và phải in `0`, chứng minh bằng đảo ngược |
| 5 — thứ tự trường từ bảng sinh | Không đụng: plan này thêm bảng *kiểm tra*, không thêm bảng *thứ tự* |
| 6 — feature gate `mod` | Không đụng: `dict` không có feature nào; `build.rs` vẫn chỉ đọc XML, không gọi toolchain ngoài |
| 7 — không `unwrap`/`expect`/`panic` | `dict` là crate thư viện. `build.rs` **được phép** chết — nó là build script, và chết còn hơn đoán, đúng như `collect_groups` đang làm |
| 9 — không chép mã QuickFIX | Bốn file `.h` mới **đọc như oracle trong test**, không chép vào repo. `vendor/` vẫn gitignore. Nếu điều này đổi thì `NOTICE` thành bắt buộc |
| 10 — số đo phải có benchmark | Plan này không công bố số hiệu năng nào |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | Script kéo thêm 4 file `.h`. Test đọc được cả bốn, và **đỏ nếu thiếu file** chứ không lặng lẽ bỏ qua | — |
| 2 | `is_defined_tag` + `is_msg_type` + interop số hiệu **912/912** | 1 |
| 3 | `field_type` + `FieldType::accepts` + interop kiểu **912/912, trừ 14 dòng miễn trừ có tên** | 2 |
| 4 | `allows` + interop **12 524/12 524** cặp | 2 |
| 5 | `enum_allows` — oracle yếu, nên cổng là XML cộng danh sách ca corpus chạm tới | 2 |
| 6 | Ca `alloc`, docs, merge | 2–5 |

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 1 | `scripts/fetch-quickfix-assets.sh && cargo test -p nanofix-dict` | Bốn file có mặt; xoá một file thì test đỏ |
| 2 | `cargo test -p nanofix-dict --test interop_quickfix_fields` | **912/912** số hiệu khớp `FixFieldNumbers.h` |
| 3 | cùng test | **898/912** khớp kiểu, **14** lệch và đúng 14 tag đã liệt kê ở trên — danh sách viết trong test, lệch thêm một tag là đỏ |
| 4 | `cargo test -p nanofix-dict --test interop_quickfix_messages` | **12 524/12 524** cặp khớp `FIELD_SET` |
| 5 | `cargo test -p nanofix-dict --test validation` | 245 trường enum, 1 708 giá trị nhận; mọi ca corpus (`21=4`, `167=BOO`, `40=w`, `123=N`) đúng chiều |
| 6 | `cargo bench -p nanofix-codec --bench alloc` | in `0`; đảo ngược (một `String` trong `enum_allows`) in khác 0 |

**Không có bước nào đóng bằng "test pass".** Mỗi cổng nêu con số, và con số ấy đối chiếu với một
bộ sinh khác — trừ bước 5, nơi oracle yếu và điều đó được nói thẳng.

## Tài liệu phải cập nhật

- [ ] `docs/DESIGN.md` §3 — dòng `dict` mô tả thêm bốn bảng
- [ ] `docs/DESIGN.md` §6 — ba dòng cổng mới, kèm con số
- [ ] `docs/PRD.md` §2 tiêu chí 3, và §3 dòng "Dictionary validation" — từ **gap** sang có số đo
- [ ] `CHANGELOG.md` — public API của `dict`
- [ ] `docs/reference/fix44-dictionary-traps.md` — vùng tag 5000–9999, và 14 trường lệch kiểu
- [ ] `docs/reference/quickfix-acceptance-def-format.md` — nếu phát sinh bẫy mới
- [ ] `STATUS.md` — khi đóng plan
- [ ] `README.md` — không đụng (không thêm crate)

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Sparse-checkout không kéo 4 file `.h`, test lặng lẽ bỏ qua và báo xanh | Bước 1: thiếu file là **đỏ**, không phải `skip`. Đúng bẫy đã trả giá ở `check-lint-config.sh` |
| Bitset lệch một bit — tag 956 là bit cuối của word 14 | Interop 12 524 cặp; và một ca riêng cho tag nhỏ nhất (1) và lớn nhất (956) |
| `enum_allows` trả `Some(true)` cho trường **không** có enum, nên không bao giờ từ chối | `None` ≠ `Some(true)`, và một test khẳng định `field_type(38)` có kiểu nhưng `enum_allows(38, …)` là `None` |
| `FieldType::accepts` quá dễ dãi: `+200.00` lọt qua QTY | Ca corpus `14f` là cổng, cộng bảng ca cho từng kiểu |
| 14 dòng miễn trừ kiểu bị nới thầm thành `!=` bỏ qua | Danh sách viết cứng trong test; lệch tag thứ 15 là đỏ |
| `allows` quên header/trailer, `52=` bị Reject là "tag not defined" | Interop 12 524; và một ca khẳng định `allows(b"0", 52)` |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Oracle enum yếu (228/245, 95 lệch số lượng) | **Cao** | Nói thẳng trong test và trong PRD. Cổng của bước 5 là XML + ca corpus, **không** phải QuickFIX. Không giả vờ có oracle |
| `FieldType::accepts` là chỗ dễ sai nhất và corpus chỉ chạm 2 ca | **Cao** | Bảng ca viết tay cho cả 23 kiểu, mỗi kiểu ít nhất một nhận một từ chối. Ghi rõ đây là ca tự nghĩ, không phải capture thật |
| Bảng phình `dict` lên ~30 KB dữ liệu tĩnh | Thấp | Đo bằng `cargo bloat` hoặc `size` sau bước 4, ghi con số. Bitset đã là phương án nhỏ hơn |
| `build.rs` chậm đi rõ rệt | Thấp | Đo trước và sau, ghi vào nhật ký. Nếu quá 2× thì tách bảng lớn nhất ra file riêng |
| 12 524 cặp khớp "đẹp quá" — hai cách giải cùng sai như nhau | Trung bình | Cả hai đọc cùng XML nhưng bằng bộ sinh khác. Ghi rõ đây là giới hạn của oracle này, y như dòng đã ghi cho 730/730 |

## Ngoài phạm vi

- **Không mã hoá `Reject (35=3)`.** Đó là bước 3 của plan session-layer.
- **Không áp luật.** `dict` trả lời câu hỏi; `session` quyết định làm gì với câu trả lời.
- **Không kiểm cấu trúc group** (thứ tự phần tử bên trong, group bắt buộc). `group_members` đã có,
  và corpus không kiểm phần còn lại.
- **Không làm cho FIX 5.0 / FIXT 1.1.** Cùng bộ sinh sẽ chạy được, nhưng đó là phase 2.
- **Không tối ưu.** Bảng đúng trước; nhanh sau, nếu có benchmark nói là cần.

## Sửa plan giữa chừng

### Sửa 1 — oracle enum **không** yếu; số liệu trong plan sai

Plan viết: `FixValues.h` chỉ phủ 228/245 trường, 95 trường lệch số lượng, nên "oracle yếu". Đo
lại bằng chính test thì **245/245 trường, 1 708/1 708 giá trị, không một ngoại lệ**.

Nguyên nhân: script dò đường viết vội chỉ khớp `const char Name_X = 'v';` mà bỏ
`const char Name_X[] = "vv";` — dạng mảng mà mọi enum kiểu chuỗi đều dùng. 17 trường "không được
phủ" chính là dạng mảng ấy, và `SecurityType(167)` nằm trong đó — **đúng cái trường mà
`14e_IncorrectEnumValue.def` kiểm**.

Nguy hiểm không nằm ở regex. **Một script dò đường báo thiếu làm oracle trông yếu, và "oracle
yếu" là lý lẽ để kiểm ít đi.** Nó đã vào plan và được duyệt trên cơ sở đó. Đã ghi vào
[reference/fix44-dictionary-traps.md](../reference/fix44-dictionary-traps.md), kèm test
`the_array_form_is_read_and_not_skipped` canh.

Cổng của bước 5 vì thế **mạnh hơn** plan hứa: thêm một chiều đối chiếu 245/245 trường.

### Sửa 2 — `5 154` thành `5 168`

Cùng loại lỗi, nhỏ hơn: plan đếm *số hiệu* tag, test đếm *tên trường*. 5 168 tên trên 5 154 số —
phiên bản sau đặt nhiều tên cho một số. Test đi theo tên nên đếm tên.

### Sửa 3 — `FieldType` sinh ra từ `src/field_type.rs`, không từ `build.rs`

Plan không nói ai giữ ánh xạ "tên kiểu trong XML → nhánh enum". Đặt ở `build.rs` thì `accepts`
và bảng sinh thành hai chỗ cho một luật. `build.rs` không `use` được crate nó đang dựng, nên
dùng `#[path = "src/field_type.rs"] mod field_type;` — một file, hai đơn vị biên dịch, một luật.

## Nhật ký giao hàng

### Tất cả 6 bước — 2026-08-28

| Bước | Cổng | Kết quả |
|---|---|---|
| 1 | `scripts/fetch-quickfix-assets.sh` | 4 file `.h` về đủ; script kiểm và `exit 1` nếu thiếu. Đảo ngược: giấu `FixFieldNumbers.h` → 2 test đỏ, **không** phải skip |
| 2 | `interop_quickfix_fields` | **912/912** số hiệu khớp; **5 168** tên trường mà FIX 4.4 không có đều bị từ chối; **93/93** message type khớp hai chiều |
| 3 | `interop_quickfix_fields` + `field_types` | **898/912** kiểu khớp, **14** lệch có tên đủ; 23 kiểu, mỗi kiểu ít nhất một nhận một từ chối |
| 4 | `interop_quickfix_messages` | **12 524/12 524** cặp, kiểm bằng **84 816** câu trả lời — mọi bản tin × mọi tag, cả hai chiều |
| 5 | `enums` | **245** trường, **1 708** giá trị khớp XML; và **245/245, 1 708/1 708** đều là giá trị QuickFIX cũng biết |
| 6 | `cargo bench -p nanofix-codec --bench alloc` | `validate 0`. Đảo ngược (một `format!` trong `accepts`) cho `validate 80000` |

**Đảo ngược, 9 lần, cả 9 đỏ**: file oracle biến mất; `is_defined_tag` luôn đúng; `is_msg_type`
luôn đúng; tag cao nhất (956) rơi khỏi bitset; `field_type` luôn trả `STRING`; `accepts` nhận
tất; dấu `+` được cho qua trên QTY; `allows` luôn đúng; `allows` quên tag header;
`enum_allows` luôn `Some(true)`; luôn `None`; mọi trường dùng chung một danh sách enum.

**Kích thước:** ~**33 KB** dữ liệu tĩnh (`ALLOWED` 12 648 B, con trỏ enum 18 368 B, chữ enum
2 214 B, `DEFINED_TAGS` 120 B). File sinh ra 3 701 dòng / 155 KB. `build.rs` chạy **0,83 s** —
không đo được khác biệt so với trước.

**Chưa chứng minh:** không có bảng nào được `session` gọi. Đó là bước 3 của plan session-layer.
Và **những gì mỗi kiểu chấp nhận là ca tự nghĩ, không phải capture** — corpus chỉ cấp 2 ca.

**Cổng:** `fmt --check`, `clippy --all-targets --all-features -D warnings`, `test --all`,
`test --all --no-default-features`, `check-lint-config.sh`, `check-links.py` — tất cả rc=0.
Máy: Apple M5, macOS 25.5.0, cargo 1.95.0.
