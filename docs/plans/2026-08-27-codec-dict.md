# Bước 1 — `codec` và `dict`: đọc, ghi FIX 4.4 không cấp phát bộ nhớ

> **Loại:** Plan · **Ngày:** 2026-08-27 · **Trạng thái:** **Đã duyệt** — 2026-08-27, sau review kỹ thuật + outside voice
> **Sửa lần 3 — 2026-08-28, duyệt lại cùng ngày.** Một tiêu chí nghiệm thu của Bước 1 sai so
> với dữ liệu và không thể đạt. Chi tiết ở *Nhật ký giao hàng → Bước 1*.
> **Phạm vi:** `DESIGN.md` §7 bước 1 — hai crate đầu tiên của workspace
> **Sửa lần 2 — 2026-08-27**, sau `/plan-eng-review` + outside voice (Codex). 16 quyết định
> ghi ở mục *Nhật ký review* cuối file. Bản đầu dựa trên vài con số đếm sai; mục
> *Những gì đã biết chắc* đã được đếm lại bằng script trên chính `vendor/`.

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Toàn bộ engine đứng trên hai việc: **đọc** một bản tin FIX từ mảng byte ra chỗ nào là tag
nào, và **ghi** một bản tin ra mảng byte. Mọi tầng trên — session, engine, dispatch — chỉ
gọi hai việc đó. Nếu hai việc này chậm hoặc cấp phát bộ nhớ, không tầng nào trên cứu được.

Đây là bước đầu tiên vì nó **đo được ngay** và **không phụ thuộc gì**: không cần socket,
không cần session, không cần Linux. Kết quả của bước này là con số parse/serialize thật của
chính mình, thay cho con số đi mượn trong `reference/measured-costs.md`.

`dict` đi kèm vì `codec` cần biết ba thứ từ đặc tả: tag nào thuộc header, tag nào là kiểu
DATA (giá trị có thể chứa ký tự phân cách), và tag nào bắt buộc trong từng loại bản tin.
Ba thứ đó sinh ra từ file XML của QuickFIX lúc build, không gõ tay.

**Bước này đóng khi API dùng được, không phải khi con số đẹp.** Xem *Điều kiện đóng bước*.

## Những gì đã biết chắc

Mọi con số ở nhóm dưới đều đếm lại bằng script tách theo byte `0x01` trên `vendor/`, ngày
2026-08-27. Bản plan đầu ghi 536 / 247 / 244 và cả ba đều sai hoặc lẫn tập.

| Sự thật | Nguồn |
|---|---|
| Mảng field 512 phần tử nằm trong struct làm parse chậm 4–6 lần; tách index ra, N=64 → heartbeat 95 ns, `NewOrderSingle` 139 ns trên M5 | `reference/measured-costs.md` §1, đo 2026-08-27 |
| **`size_of::<MessageView<64>>()` = 24 byte, không phải 16.** `&[u8]` là con trỏ béo (16) + `&FieldIndex` (8). Không có cách nào giữ hai tham chiếu an toàn mà xuống 16 | Chạy `rustc -O` với đúng định nghĩa ADR-0003, 2026-08-27 |
| **59 file `.def` chứa 289 dòng `I` và 250 dòng `E` — tổng 539** | đếm bằng script trên `vendor/quickfix/test/definitions/server/fix44/` |
| **Trong 250 dòng `E`: 247 có `9=`, 244 có cả `9=` lẫn `10=`.** 3 dòng không có `9=` (`2d`/`3c_GarbledMessage`), 3 dòng có `9=` mà không `10=` (`3b_InvalidChecksum`, `SessionReset` ×2) | cùng nguồn |
| **Trong 289 dòng `I`: chỉ 8 có `9=`, chỉ 7 có `10=`.** Dòng `I` là *template*, không phải bản tin dây. `Reflector.rb::fixify!` tính `9=` và `10=` lúc gửi | cùng nguồn, và `reference/quickfix-acceptance-def-format.md` |
| **`<TIME>` xuất hiện 350 lần trong `I`, 2 lần trong `E`.** `<TIME>` dài 6 byte; timestamp thật 17 hoặc 21 byte. BodyLength/CheckSum tính trên dòng thô là vô nghĩa | cùng nguồn |
| **8 dòng mang tiền tố số phiên** (`I1,`, `I2,`, `E1,`) — 5 dòng `I`, 3 dòng `E` | cùng nguồn |
| **7 dòng cố tình sai chuẩn**: `8=FIX.3.9` (`1d`), `8=FIX.4.1` ×2 (`2i`), `35=` đứng trước `8=` (`2t`), checksum sai (`3b`), garbled (`2d`, `3c`) | cùng nguồn |
| **7 field giá trị rỗng trên dòng `I`, trong 2 file, đòi hai hành vi khác nhau.** `14d`: `56=` rỗng → session phải trả Reject có `371=56`, `373=4`, **và tăng inbound MsgSeqNum**. `ReverseRouteWithEmptyRoutingTags`: 6 field rỗng (`115=`,`128=`,`116=`,`129=`,`144=`,`145=`) → **không** reject | cùng nguồn |
| **Một loại bản tin có nhiều tập tag khác nhau.** Trên 244 dòng `E` sạch: `35=3` có **8** mẫu, `35=D` có 4, `35=A`/`35=5` có 2 | cùng nguồn |
| Thứ tự field: `8, 9, 35` cố định → header **tag tăng dần** → body **tag tăng dần** → `10`. Không phải thứ tự XML. 247/247 dòng, không ngoại lệ | `reference/quickfix-acceptance-def-format.md` |
| Bộ so sánh so **theo vị trí**; tag `10/42/52/60/122` so bằng **regex**, mọi tag khác so **chuỗi chính xác**. `9=` không nằm trong `fields.fmt` → so chính xác | cùng trang |
| `FIX44.xml` dùng `<component>` ở **632 chỗ**; `NewOrderSingle` tham chiếu `Parties`, `PreAllocGrp`, `TrdgSesGrp` | đếm trên `vendor/quickfix/spec/FIX44.xml` |
| Trong FIX 4.4, field DATA đi kèm field độ dài đứng ngay trước (`95`→`96`, `212`→`213`); giá trị DATA **được phép chứa `0x01`**. `.def` **không có mẫu DATA nào** | Đặc tả FIX 4.4 + `type='DATA'` trong XML |
| QuickFIX Software License cho phép dùng XML và `.def` làm dữ liệu; **không** commit chúng | ADR-0001 |
| Gate: parse `NewOrderSingle` ≤ 150 ns, serialize `ExecutionReport` ≤ 60 ns, **0** cấp phát | `DESIGN.md` §6 |

## Cách làm

### Ranh giới ngữ nghĩa `codec` ↔ `session` — chốt trước mọi thứ khác

Đây là quyết định gốc, mọi chữ ký hàm bên dưới suy ra từ nó.

```
                 byte từ counterparty
                          │
                          ▼
        ┌─────────────────────────────────────────┐
        │  codec — biết CÚ PHÁP                    │
        │  Từ chối chỉ khi KHÔNG ĐỌC NỔI:          │
        │    · tag phi số / tràn u32               │
        │    · độ dài DATA vượt biên buffer        │
        │    · thiếu field độ dài của DATA         │
        │    · BodyLength / CheckSum sai (bật cờ)  │
        └───────────────┬─────────────────────────┘
                        │  đọc được → MessageView
                        ▼
        ┌─────────────────────────────────────────┐
        │  session — biết LUẬT                     │
        │  Phán mọi thứ đọc được nhưng sai luật:   │
        │    · giá trị rỗng   (14d → Reject 373=4) │
        │    · BeginString lạ (1d, 2i)             │
        │    · sai thứ tự     (2t)                 │
        │  Vì chỉ session biết phải TRẢ LỜI gì.    │
        └─────────────────────────────────────────┘
```

Hệ quả trực tiếp: **không có `ParseError::EmptyValue`.** Field `56=` được ghi vào index với
`len = 0`; `view.get(56)` trả lát rỗng. Nếu parser từ chối sớm, session không đọc được
`34=2` để tăng seq và không biết tag nào để đặt vào `371=56` — `14d` và
`ReverseRouteWithEmptyRoutingTags` **không thể pass**, tức tự nguyện bỏ 2 trong 59.

### `crates/dict` — sinh bảng từ XML lúc build

- `build.rs` đọc `vendor/quickfix/spec/FIX44.xml` (ghi đè bằng `NANOFIX_FIX44_XML`).
  **Thiếu file → build fail với thông báo chỉ thẳng tới `scripts/fetch-quickfix-assets.sh`.**
  Không im lặng, không fallback.
- Dependency **chỉ ở build**: `roxmltree`, pin version. Runtime của `dict` không dependency.
- Sinh ra `$OUT_DIR/fix44.rs`:
  - `pub mod tag` — `pub const MSG_SEQ_NUM: u32 = 34;` cho mọi field.
  - `pub mod msg_type` — `pub const NEW_ORDER_SINGLE: &[u8] = b"D";`.
  - `pub fn is_header(tag: u32) -> bool` — bảng tra, tách header/body khi ghi.
    **Sửa 2026-08-28:** phải **đi vào cả `<group>`** trong `<header>`. `NoHops(627)` cùng 3
    field con (628, 629, 630) là field header. Chỉ đọc `<field>` con trực tiếp ra 26 tag thay
    vì 30, và 4 tag thiếu sẽ xếp nhầm vào body khi ghi — đúng kiểu hỏng của bất biến 5.
    **59 `.def` không có tag nào trong số đó**, nên 59/59 vẫn xanh khi sai. Bẫy 3.
  - `pub fn data_length_tag(tag: u32) -> Option<u32>` — tag độ dài của field DATA.
    **Sửa 2026-08-28:** khớp **theo tên** (`XLen` / `XLength`), **không** theo `tag − 1`.
    `Signature(89)` dùng `SignatureLength(93)`. 15/16 field DATA theo `tag − 1` và đúng cái
    thứ 16 thì không. Bẫy 1.
  - `pub fn required(msg_type: &[u8]) -> &'static [u32]` — field bắt buộc theo loại bản tin.
    **Giới hạn đã biết:** bản này chỉ đọc `<field>` con trực tiếp, **không đệ quy qua
    `<component>`**. Với 632 chỗ dùng component trong XML, bảng này thiếu field. Chấp nhận
    ở bước 1 vì chưa ai gọi; ghi vào `STATUS.md` Open items, chặn plan `session`.
- `dict` implement trait `codec::Dictionary`, nên **`dict` phụ thuộc `codec`** — `DESIGN.md`
  §3 hiện ghi "—", phải sửa.

### `crates/codec` — đọc và ghi, `#![no_std]`, không dependency

**Kiểu dữ liệu** (ADR-0003, với số byte đã đếm lại):

```rust
#[repr(C)] pub struct FieldEntry { tag: u32, offset: u32, len: u16, _pad: u16 }   // 12 byte
pub struct FieldIndex<const N: usize> { count: u16, fields: [FieldEntry; N] }
#[derive(Clone, Copy)] pub struct MessageView<'a, const N: usize> { buf: &'a [u8], idx: &'a FieldIndex<N> }
pub type OrderView<'a> = MessageView<'a, 64>;

const _: () = assert!(core::mem::size_of::<MessageView<64>>() == 24);   // 3 word, KHÔNG phải 2
```

`MessageView` là **24 byte**. Cả x86-64 SysV lẫn AArch64 truyền struct >16 byte gián tiếp,
nên phải `#[inline]` mọi hàm nhận nó qua ranh giới crate. Guard vẫn giữ nguyên tác dụng: ai
thêm field vào view thì build fail.

**Trait `Dictionary`** — `codec` chỉ hỏi hai câu, hỏi qua trait để không phụ thuộc `dict`:

```rust
pub trait Dictionary {
    fn is_header(tag: u32) -> bool;
    fn data_length_tag(tag: u32) -> Option<u32>;

    // Ba hàm dưới thuộc plan repeating-groups, KHÔNG cài đặt ở bước này.
    // Chúng nằm đây vì `Dictionary` là API công khai: thêm hàm sau là breaking change.
    // Cùng logic `Role` ở ADR-0004 — rẻ bây giờ, gãy sau.
    // Khoá là (msg_type, counter), KHÔNG phải counter một mình: 4 counter tag
    // (268, 124, 420, 295) có delimiter khác nhau tùy bản tin. Chi tiết trong plan đó.
    fn group_delimiter(_msg_type: &[u8], _counter: u32) -> Option<u32> { None }
    fn group_members(_msg_type: &[u8], _counter: u32) -> &'static [u32] { &[] }
    fn group_order(_msg_type: &[u8], _counter: u32) -> &'static [u32] { &[] }
}
```

Hàm `#[inline]`, không `dyn`. `codec` ship sẵn `NoDict` (mọi câu trả lời là "không").

**Đọc**

```rust
pub enum Parsed { Complete { consumed: usize }, Incomplete }

pub fn parse_into<D: Dictionary, const N: usize>(
    buf: &[u8], idx: &mut FieldIndex<N>, v: Validation,
) -> Result<Parsed, ParseError>;
```

`Incomplete` nằm trong nhánh `Ok` vì nó **không phải lỗi** — TCP giao byte, không giao bản
tin. Trộn nó vào `Err` là trộn "mọi thứ vẫn tốt, đợi thêm" với "phiên hỏng, ngắt đi"; mọi
call site sẽ phải trả giá. `Validation` là tham số thật, không phải cờ trong tài liệu.

```
parse_into  ─┬─ Ok(Complete { consumed })  → caller tiến buf, parse tiếp
             ├─ Ok(Incomplete)             → đợi read sau, GIỮ NGUYÊN buf
             └─ Err(ParseError)            → ngắt phiên
```

1. Kiểm `8=` ở byte 0, `9=` ngay sau, `35=` ngay sau nữa — sai vị trí là lỗi. Chỉ kiểm
   **vị trí**, không kiểm **giá trị**: `8=FIX.3.9` parse bình thường, session phán.
2. Quét tuyến tính: đọc tag (thập phân, tràn `u32` là lỗi), `=`, rồi tìm `0x01`. Với tag mà
   `D::data_length_tag` trả `Some(len_tag)`, **không tìm `0x01`** mà lấy đúng số byte từ
   `len_tag` đọc được ngay trước.
3. Ghi `(tag, offset, len)` vào index. `len` được phép bằng 0. Vượt `N` →
   `ParseError::TooManyFields`, **không** cắt bớt.
4. Hết buffer mà chưa gặp `10=` → `Ok(Parsed::Incomplete)`.
5. Gặp `10=`: kiểm `BodyLength` (số byte từ sau `0x01` của `9=` đến trước `10=`) và
   `CheckSum` (tổng byte mod 256 của mọi thứ trước `10=`, ba chữ số). Cả hai theo cờ
   `Validation`, mặc định bật. Trả `Ok(Complete { consumed })`.

```rust
pub enum ParseError {                       // Copy, mang tối đa một u32. Không String.
    BadTag(u32),            // phi số hoặc tràn u32
    TooManyFields,
    MissingLengthField(u32),// field DATA mà tag độ dài vắng hoặc không đứng liền trước
    LengthOutOfBounds(u32), // độ dài DATA vượt phần buffer còn lại  ← chỗ duy nhất đọc ngoài biên
    BadBodyLength,
    BadCheckSum,
    FieldTooLong(u32),      // giá trị > u16::MAX byte, không tràn im lặng
}
```

**Tra field**: `view.get(tag) -> Option<&[u8]>` quét tuyến tính, trả lần xuất hiện đầu.
`view.find_from(pos, tag)` cho repeating group. Bộ chuyển kiểu `as_u32`, `as_i64`, `as_char`
trả `Result`, không panic. **Không** có kiểu decimal trong bước này.

**Ghi — `Template`**

Template **sở hữu** byte tĩnh của nó. Không lifetime, cất thẳng vào struct mỗi phiên.
`&'static` không dùng được: `49=ISLD\x0156=TW44\x01` đến từ Logon lúc chạy, giữ nó bằng
`'static` là phải leak mỗi phiên. Mượn arena của session thì thành struct tự tham chiếu.

```rust
enum Part { Static(Range<u16>), Slot(u32) }          // Static trỏ vào scratch
pub struct Template<const P: usize, const S: usize> {
    scratch: [u8; S],                                 // byte tĩnh, chép vào một lần lúc dựng
    parts:   [Part; P],
    len:     u8,                                      // ≤ P ≤ 255
}

pub fn encode(&self, out: &mut [u8], slots: &[(u32, &[u8])]) -> Result<Range<usize>, EncodeError>;
```

- **Lúc dựng**: nhận tag tĩnh kèm giá trị và tag động; sắp theo quy tắc đã xác lập —
  `35` trước, header tăng dần, body tăng dần — rồi gộp các tag tĩnh liền nhau thành một
  `Static`. **Sắp xếp xảy ra lúc dựng, không xảy ra lúc gửi.**
- **Slot tùy chọn**: một `Slot(tag)` không có mặt trong `slots` thì **bỏ qua**. Đây là điều
  bắt buộc, không phải tiện nghi: `35=3` có 8 mẫu tập-tag khác nhau trong dữ liệu thật, một
  template cứng không biểu diễn nổi.
- **Lúc gửi** — bố cục buffer, phần khó nhất:

```
out:  [ ...... chừa K byte ...... | body ................. | 10=NNN␁ ]
                                  ^                        ^
                                  K                        end
      ┌───────── bước 1: ghi body từ vị trí K ─────────────┘
      │
      └─ bước 2: ghi "8=FIX.4.4␁9=<len>␁" KẾT THÚC ĐÚNG TẠI K, canh phải
                 ├─ start = K - độ_dài_prefix        ← trả về [start, end)
                 └─ KHÔNG dịch chuyển buffer, KHÔNG đệm 0 vào 9=
                    (bộ so sánh so 9= bằng chuỗi chính xác)
         bước 3: checksum trên [start, trước 10=), ba chữ số CÓ đệm 0
```

  `K` = độ dài tối đa của `8=FIX.4.4␁9=NNNNN␁`. BodyLength ≥ 100000 → `EncodeError::BodyTooLong`,
  không tràn qua `K`. `out` quá nhỏ → `EncodeError::OutputTooSmall`, không panic.
- `SendingTime`: `TimestampCache` giữ 15 byte `YYYYMMDD-HH:MM:` và phút hiện tại; mỗi bản
  tin chỉ format `SS.mmm`. Đổi phút thì format lại tiền tố. Đây là một `Slot` đặc biệt.

**Cấu trúc file**

```
crates/codec/src/lib.rs        #![no_std], re-export
crates/codec/src/index.rs      FieldEntry, FieldIndex, MessageView, tra field
crates/codec/src/parse.rs      parse_into, Parsed, ParseError, Validation
crates/codec/src/checksum.rs   tổng mod 256 — bản thường trước, SIMD chỉ khi đo thấy cần
crates/codec/src/template.rs   Template, Part, encode, EncodeError
crates/codec/src/timestamp.rs  TimestampCache
crates/codec/src/dict.rs       trait Dictionary, NoDict
crates/codec/benches/{parse,serialize,alloc}.rs
crates/codec/tests/defs.rs         bảng phân loại 539 dòng
crates/codec/tests/roundtrip.rs    244 dòng E byte-identical
crates/codec/tests/template_reuse.rs  một template, nhiều bản tin
crates/codec/tests/stream.rs       vòng lặp đọc TCP giả lập
crates/dict/build.rs, src/lib.rs
fuzz/                          crate RIÊNG, ngoài workspace — chỉ nightly
```

`Cargo.toml` workspace: thêm hai member (`fuzz/` **không** phải member). Thêm lint
`clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic` = `deny` cho hai crate này
(cho phép trong `#[cfg(test)]` và `benches/`).

### Nạp fixture — chuẩn hoá trước, rồi mới phân loại

`.def` không phải bản tin dây. Loader dùng chung cho bước 1 **và** bước `conformance`:

```
dòng thô  "I1,8=FIX.4.4␁35=A␁…␁52=<TIME>␁"
   │
   ├─ 1. bỏ ký tự directive (I / E)
   ├─ 2. cắt tiền tố số phiên "N,"          ← 8 dòng có, bỏ sót là BadTag ngay field đầu
   ├─ 3. thay <TIME> bằng MỘT mốc UTC CỐ ĐỊNH ← để checksum tái lập được giữa các lần chạy
   ├─ 4. fixify!: chèn 9= tính sẵn nếu vắng, 10= tính sẵn nếu vắng  (giống Reflector.rb)
   └─ 5. tra bảng phân loại → kỳ vọng Ok, hay Err(<biến thể cụ thể>)
```

Bảng phân loại liệt kê 7 dòng sai chuẩn **theo tên file**. Parser từ chối đúng chỗ là XANH;
từ chối sai chỗ, hoặc nhận sai chỗ, là ĐỎ. Đây là lý do không dùng bộ đếm thành công: một
bộ đếm sẽ đỏ ngày đầu và phản xạ tự nhiên là nới assertion — đúng cái bẫy `CLAUDE.md` §10.

## Điều kiện đóng bước 1

Không phải "đạt 150/60 ns". Bước 1 đóng khi **API dùng được theo cách `engine` sẽ dùng**:

`tests/stream.rs` nạp byte của cả 244 dòng E vào một vòng lặp đọc giả lập TCP — mỗi lần
"read" trả về một mẩu dài ngẫu nhiên từ 1 byte tới cả bản tin, đôi khi chứa nhiều bản tin —
và phải: đẩy hết 244 bản tin qua `parse_into` mà **không** bỏ sót, **không** nhân đôi, mọi
mẩu chưa đủ trả `Incomplete`; rồi encode trả lại và so byte-identical. Số đo được ghi lại
kèm máy và cài đặt, nhưng số **không** là điều kiện đóng.

## Bất biến bị đụng tới

| # | Điều | Cách giữ |
|---|---|---|
| 1 | Không cấp phát trên hot path | `#![no_std]` **không** tự nó chứng minh điều này — crate vẫn khai `alloc` được, caller vẫn cấp phát được. Thứ chứng minh là `benches/alloc.rs` với allocator đếm |
| 5 | Thứ tự field từ bảng sinh, không từ call site | `Template` sắp lúc dựng theo `D::is_header` + sort tag. Không có API nào cho caller tự chọn thứ tự |
| 6 | Feature flag gate `mod` | Bước này không có feature nào — nên `cargo test --no-default-features` ở bước này là **kiểm tra rỗng**, ghi rõ vậy chứ không kể là cổng. `build.rs` của `dict` chỉ đọc XML, không gọi toolchain ngoài |
| 7 | Không `unwrap`/`expect`/`panic` | Clippy `deny` ở cấp crate. Riêng đường DATA: `LengthOutOfBounds` tồn tại chính để tránh panic khi cắt lát |
| 8 | `unsafe` phải có chứng minh | **Mục tiêu: 0 `unsafe`.** Đây là lý do nhận `MessageView` 24 byte thay vì ép về 16 bằng con trỏ thô |
| 9 | Không copy mã QuickFIX | `dict` chỉ đọc XML lúc build. Không nhìn `src/C++` khi viết parser |
| 10 | Số hiệu năng phải kèm bench + máy + settings | Nhật ký ghi rõ "M5, macOS, không pin, so sánh tương đối" |

Điều 2, 3, 4 (session thuần, 59/59, engine không ngủ) chưa đụng — chưa có session, chưa
có engine.

## Chia việc

| Bước | Kết quả | Thời gian | Phụ thuộc |
|---|---|---|---|
| 0 | Nhánh `plan/codec-dict`. Workspace lint `deny`. `cargo build` **fail đúng thông báo** khi thiếu `vendor/`. CI: job `--no-default-features`, job fetch vendor | ½ ngày | — |
| 1 | `dict`: `build.rs` → `is_header`, `data_length_tag`, `tag::*`, `msg_type::*`, `required` (không đệ quy component) | 2–3 ngày | 0 |
| 2 | Loader fixture (chuẩn hoá 5 bước) + `codec::index` + `parse_into` với `NoDict`. Bảng phân loại 539 dòng | 3 ngày | 0 |
| 3 | Parse với `dict` thật: field DATA chứa `0x01`, hai lỗi biên. Fuzz `cargo +nightly fuzz`, 10 phút, không panic | 1 ngày | 1, 2 |
| 4 | `Template` + `TimestampCache` + slot tùy chọn. Round-trip 244 dòng E; test một-template-nhiều-bản-tin | 3 ngày | 2 |
| 5 | `tests/stream.rs` (điều kiện đóng bước). Ba bench assert trần hồi quy. Ghi số vào nhật ký. Cập nhật docs. Merge | 1½ ngày | 3, 4 |

**Tổng: ~11,5 ngày nếu quen Rust. Team chưa quen → 3–4 tuần.** Bước 2 là chỗ học ownership;
bước 4 là chỗ học lifetime. Đừng làm hai bước đó cùng lúc.

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 0 | `mv vendor vendor.bak && cargo build -p dict; mv vendor.bak vendor` | Output chứa đúng dòng "run scripts/fetch-quickfix-assets.sh". Không phải lỗi khác |
| 1 | `cargo test -p fixbolt-dict` | Xanh, và **đọc** output thấy tên các test. **Sửa 2026-08-28:** `required(b"D")` là `[11, 40, 54, 60]` — **không** chứa 21 và 55. Cả hai là `required='N'` trong `FIX44.xml`, đệ quy component cũng không thêm chúng. Xem `reference/fix44-dictionary-traps.md` bẫy 2 |
| 2 | `cargo test -p fixbolt-codec --test defs` | In `classified 539/539`. **Sửa 2026-08-28: 533 dòng `Ok`, 6 dòng `Err`**, không phải 532/7 — `8=FIX.3.9` và `8=FIX.4.1` ×2 parse bình thường (parser chỉ kiểm vị trí, không kiểm giá trị), còn `2t` có **hai** bản tin sai nhưng chỉ một cái không đóng khung được |
| 2 | `--test defs` (body length) | **Sửa 2026-08-28:** 250 dòng mang `9=` của chính nó, **6** lệch. Ba do cố ý (`1d`, `2m` ×2), ba là dòng `E` của QuickFIX mang `9=` cũ, lệch **đúng 4 byte** vì timestamp 17 ký tự trong khi `9=` tính cho 21 |
| 2 | `--test defs` (checksum) | **Sửa 2026-08-28: `checksum ok 244/244` là BẤT KHẢ THI.** 246 dòng mang `10=` và **0** dòng nào là checksum thật — 238 dòng là `10=0`. Bộ so sánh khớp tag 10 bằng regex nên giá trị chưa từng cần đúng. Thay bằng: xác thực checksum trên **287** bản tin mà loader tự tính, tức hai cài đặt độc lập đồng ý với nhau |
| 2 | Test `TooManyFields`: parse một dòng E thật với `FieldIndex<4>` | Trả `Err(TooManyFields)`, và **không** có index nào chứa 4 field "thành công" |
| 3 | `cargo +nightly fuzz run parse -- -max_total_time=600` | Không crash, không timeout. Corpus lưu lại thành test hồi quy |
| 4 | `cargo test -p codec --test roundtrip` | `identical 244/244`. Một byte lệch là đỏ, in dòng lệch dưới dạng `\|` |
| 4 | `cargo test -p codec --test template_reuse` | **Một** template `35=3` duy nhất, dựng từ hợp của cả 8 mẫu, encode đúng cả 38 dòng Reject. Đây là test duy nhất chứng minh cơ chế D9 |
| 5 | `cargo test -p codec --test stream` | 244 bản tin qua vòng lặp đọc giả lập, không sót, không nhân đôi, `Incomplete` đúng mọi mẩu cụt |
| 5 | `cargo bench -p codec` | Ba bench xanh **theo assert trong bench**. Ngưỡng là **trần hồi quy ~1,5–2× baseline đo được**, không phải 150/60 ns — xem *Rủi ro*. Copy nguyên output vào nhật ký |
| 5 | `benches/alloc.rs` | In `allocations: 0` cho parse và encode |
| mọi bước | `cargo clippy --all-targets -- -D warnings` và `cargo test --no-default-features` | Sạch. Ghi rõ: `--no-default-features` ở bước này không kiểm được gì vì chưa có feature |

**Dữ liệu thật:** mọi test parse/round-trip chạy trên 539 dòng QuickFIX phát ra, nạp từ
`vendor/`. Test **không được skip** khi thiếu vendor — phải fail với thông báo rõ.

**Bằng chứng đỏ trước:** với mỗi bước, commit đầu tiên là test đỏ, output trích trong commit
message. Xem `CLAUDE.md` §10.

## Tài liệu phải cập nhật

Theo bảng đồng bộ `CLAUDE.md` §4.

> **[x] đánh dấu 2026-08-27, trong đợt quét sau khi duyệt** — không đợi tới lúc viết code.
> Lý do: chúng là **sai ngay bây giờ**, không phải "sẽ sai khi code xong". `CLAUDE.md` §4:
> *tài liệu cũ tệ hơn không có tài liệu*. Mục nào còn `[ ]` là mục **chỉ làm được khi có
> code**, không phải mục bị bỏ quên.

- [x] `DESIGN.md` §2 **sơ đồ**: vẽ lại thành hai nhánh — inline mặc định, ring tùy chọn.
      Sơ đồ hiện đặt ring bắt buộc giữa L4/L3, cũ hơn D4.
- [x] `DESIGN.md` §3 bảng crate: `dict` phụ thuộc `codec`; `conformance` phụ thuộc
      `codec` + `session` (không phải chỉ `codec`).
- [x] `DESIGN.md` §1 vs §8: sàn độ trễ ghi hai con số khác nhau (15–25 µs / 10–20 µs).
      Chốt một, sửa cả README.
- [x] `DESIGN.md` D2 + ADR-0003: `MessageView` là **24 byte**, không phải 16 / "two words".
      Đính chính tại chỗ, có ghi ngày, theo hình dạng ADR-0002 đã dùng. Ghi kèm hệ quả ABI
      (>16 byte → truyền gián tiếp → phải `#[inline]`).
- [x] `DESIGN.md` D2: chữ ký `parse_into` đổi sang `Result<Parsed, ParseError>` + tham số
      `Validation`.
- [x] `DESIGN.md` D4 dòng 147: bỏ `MessageView<'_, 64>` hardcode, để N là tham số.
- [x] `DESIGN.md` D9: sửa "patch tại offset tính sẵn" → "danh sách phần đã sắp, phần tĩnh
      encode sẵn trong buffer template tự sở hữu, body ghi trước rồi prefix canh phải, slot
      tùy chọn bỏ qua được".
- [x] `DESIGN.md` D10 dòng 240: bỏ câu "byte xếp hàng chính là byte journal đang giữ" —
      sai dưới `JournalPolicy::None`.
- [x] `reference/quickfix-acceptance-def-format.md`: sửa "247 E lines" → 250 dòng E, 247 có
      `9=`, 244 có `10=`. Sửa dòng 121 "cần một TCP client" → runner chạy **thuần trong tiến
      trình** đối với máy trạng thái session, không socket.
- [~] `reference/measured-costs.md` dòng 81, 86: `MessageView` 24 byte **đã sửa 2026-08-27**;
      §5 (số của chính mình) còn thiếu — chưa có code nên chưa có số. Thêm §5 — số của
      chính mình, kèm máy và settings.
- [x] `CLAUDE.md` §6: "`MessageView` is two words" → three words / 24 bytes.
- [ ] `README.md` layout: thêm `crates/codec`, `crates/dict`.
- [ ] `STATUS.md`: đóng plan, ghi số đo, ghi cái chưa làm, thêm 3 dòng Open items (mục dưới).
- [~] `CHANGELOG.md`: **đã tạo 2026-08-28** với mục `Unreleased`. Mục `Added` cho
      `codec` + `dict` viết khi crate thật sự phát hành.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Dòng `I` không phải bản tin dây — 281/289 thiếu `9=` và `10=` | Loader chuẩn hoá 5 bước; `defs.rs` phân loại 539/539 |
| Tiền tố số phiên `I1,` bị đưa thẳng vào parser | Bước 2 của loader; test riêng trên 8 dòng đó |
| `<TIME>` dài 6 byte, timestamp thật 17–21 byte | Bước 3 của loader thay bằng mốc cố định trước khi tính `9=`/`10=` |
| 7 dòng cố tình sai chuẩn bị coi là thất bại | Bảng phân loại khai theo tên file: từ chối đúng chỗ là XANH |
| Giá trị rỗng `55=` bị parser từ chối → `14d` không pass | **Không** có `EmptyValue`. Test: parse dòng `I` của `14d`, `view.get(56)` trả `Some(&[])`, `view.get(34)` trả `b"2"` |
| Bản tin cụt bị coi là hỏng → ngắt phiên của counterparty bình thường | `tests/stream.rs`: cắt 244 dòng tại **mọi** điểm byte, mọi tiền tố phải trả `Incomplete` |
| Thứ tự field — sort theo tag, không theo XML | `roundtrip`: 244/244 byte-identical |
| Một template cứng không biểu diễn nổi 8 mẫu của `35=3` | `template_reuse`: một template, 38 dòng |
| `BodyLength` đếm từ **sau** `0x01` của `9=` đến **trước** `10=` — lệch 1 là lỗi kinh điển | So với 247 giá trị `9=` thật |
| `BodyLength` phải là số thật, không đệm `0` | `roundtrip` bắt được; thêm test riêng body dài 9, 99, 999 byte |
| `BodyLength` ≥ 100000 làm vỡ canh phải tại `K` | `EncodeError::BodyTooLong`; test dựng body 100001 byte |
| `CheckSum` tính trên **mọi byte** trước `10=`, kể cả `8=` và `9=`; ba chữ số đệm `0` | Đối chiếu cài đặt tham chiếu ngây thơ trong file test |
| Field DATA chứa `0x01` — parser tìm `0x01` sẽ cắt sai | Test `RawDataLength=5, RawData=ab\x01cd`. **Bịa** — `.def` không có mẫu thật; ghi rõ là mẫu theo đặc tả |
| Độ dài DATA vượt buffer → đọc ngoài biên hoặc panic khi cắt lát | `LengthOutOfBounds`; test `95=999999` trên buffer 20 byte; làm hạt giống fuzz |
| Field độ dài của DATA vắng mặt hoặc không đứng liền trước | `MissingLengthField`; hạt giống fuzz |
| Giá trị field > `u16::MAX` byte tràn im lặng vào `len: u16` | `FieldTooLong`; test dựng field 65536 byte |
| Tag tràn số (`99999999999=`) hoặc không phải số | `BadTag`, không panic. Fuzz canh |
| `MessageView` phình ra khi ai đó thêm field | `const _: () = assert!(size_of::<MessageView<64>>() == 24);` — compile fail |
| Vượt `N` bị cắt âm thầm | Test `FieldIndex<4>` trên dòng thật → phải `Err`, không `Ok` |
| `out` quá nhỏ khi encode | `EncodeError::OutputTooSmall`, không panic. Test với `out` đúng thiếu 1 byte |
| `TimestampCache` sai khi đổi phút, đổi ngày, hoặc lần gọi đầu | Test `23:59:59.999 → 00:00:00.000`; `12:34:59.999 → 12:35:00.000`; cache trống |
| Index tái dùng khi view cũ còn sống → view trỏ sai | Kiểu: `parse_into` mượn `&mut`, view mượn `&`. Thêm doc-test `compile_fail` |
| Test skip âm thầm khi thiếu vendor | Test đọc vendor gọi `panic!` với thông báo, trong `#[cfg(test)]` |
| Bench xanh nhờ compiler bỏ code chết | `black_box` trên input **và** output; so ns/op có và không có `black_box` |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Team chưa quen Rust; bước 2 và 4 là chỗ ownership/lifetime khó nhất | Cao | Ước lượng đã nhân 1,5–2. Bước 2 xong hẳn rồi mới sang 4. Không `unsafe` để "cho qua" borrow checker. Bước 4 đã tránh sẵn bẫy struct tự tham chiếu bằng cách cho `Template` tự sở hữu buffer |
| **Bench assert cứng ≤150/≤60 ns sẽ nhấp nháy trên macOS** | Cao | Baseline 138,8 ns chỉ cách 150 ns 8%, mà macOS không pin nhân dao động hơn thế; ≤60 ns thì **chưa từng đo trên máy này**. Bench assert **trần hồi quy ~1,5–2× baseline thật**, số cụ thể vào nhật ký. 150/60 ns giữ nguyên là **mục tiêu công bố**, xác nhận trên Linux ở bước `engine`. Một cổng đỏ ngẫu nhiên là cổng sẽ bị tắt |
| Số đo trên M5/macOS không phản ánh Linux | Trung bình | Ghi rõ là số so sánh tương đối |
| `#![no_std]` gây ma sát | Trung bình | Đó là mục đích. Nếu thật sự kẹt, `alloc` feature cho `dict` được phép — `codec` thì không |
| `fuzz/` cần nightly, workspace ghim stable | Trung bình | `fuzz/` là crate riêng **ngoài** workspace; chạy `cargo +nightly fuzz`. Thư viện vẫn ship trên stable. CI cài hai toolchain |
| Quy tắc thứ tự có ngoại lệ ngoài 244 dòng đã xem (repeating group) | Thấp | Ngoài phạm vi. `.def` chỉ có một `454=0` |
| `roxmltree` đổi API | Thấp | Chỉ ở build; pin version |
| CI không có `vendor/` | Trung bình | CI job chạy `scripts/fetch-quickfix-assets.sh` trước `cargo test` |
| **`fetch-quickfix-assets.sh` bám nhánh `master` đổi được** | Trung bình | Mọi con số nghiệm thu (539, 247, 244, 8 mẫu Reject) có thể đổi mà không ai sửa plan. Chưa xử lý ở bước này — ghi vào `STATUS.md` Open items |

## Ngoài phạm vi

Cân nhắc rồi hoãn, kèm lý do:

- **Bất biến DATA khi ghi** — `encode` chưa tự sinh lại field độ dài cho DATA động. Chưa cần
  vì không counterparty nào trong bộ test gửi DATA. → `STATUS.md` Open items.
- **`<component>` đệ quy trong `required()`** — 632 chỗ dùng component; bảng `required` bước
  này thiếu field. Chưa ai gọi nó ở bước 1. → `STATUS.md` Open items, chặn plan `session`.
- **Ghim commit QuickFIX trong fetch script** — rẻ nhưng ngoài phạm vi hai crate.
  → `STATUS.md` Open items.
- **Repeating group** — chỉ có `find_from`, không đọc/ghi group. Đã có plan riêng:
  [2026-08-27-repeating-groups.md](2026-08-27-repeating-groups.md). Bước 1 chỉ nhận **hình
  dạng** của trait `Dictionary` (3 hàm mặc định trả rỗng) để không phải đổi API công khai sau.
- **Kiểu decimal / giá** — chỉ bytes và số nguyên.
- **SIMD cho checksum/quét `0x01`** — 139 ns không SIMD đã đạt gate. Đo trước, tối ưu sau.
- **Metadata cho conformance** (allowed-tag, enum, type-format, group) — `dict` bước này chỉ
  sinh 5 bảng. Phần còn lại thuộc plan `conformance`.
- FIX 5.0 / FIXT 1.1, FIXML, FAST, SBE.
- Session, socket, engine, dispatch — bước 3 và 4 của `DESIGN.md` §7.
- Conformance runner `.def` — bước 2 của §7, plan riêng. **Đã chốt: chạy thuần trong tiến
  trình, không socket** (xem Nhật ký review D5).
- Phân phối / publish crates.io — tên `fixbolt` còn là placeholder; publish trước khi
  đổi tên là sai lầm không đảo được.

## Nhật ký giao hàng

### Bước 0 — xong 2026-08-28

Workspace có 2 crate (`fixbolt-codec`, `fixbolt-dict` — tên `codec`/`dict` đã có người lấy
trên crates.io). `cargo build -p fixbolt-dict` khi thiếu `vendor/` → `EXIT=101` kèm đúng dòng
`run scripts/fetch-quickfix-assets.sh`; có `vendor/` → `EXIT=0`; `NANOFIX_FIX44_XML` trỏ file
không tồn tại → cũng báo đúng đường dẫn đó. CI thêm bước fetch vendor.

### Bước 1 — xong 2026-08-28. Plan sai 3 chỗ, sửa và duyệt lại

Generator sinh 5 bảng từ XML: `tag::*` (912 hằng), `msg_type::*` (93), `is_header` (30 tag),
`data_length_tag` (16), `required` (84 nhánh). 11/11 test xanh.

**Ba chỗ plan sai so với dữ liệu**, phát hiện khi viết generator, đo lại bằng parser XML:

1. **`required(b"D")` không chứa 21 và 55.** Plan đòi `[11,21,40,54,55,60]`. `HandlInst(21)`
   là `required='N'` ngay trong `<message>`; `Symbol(55)` là `required='N'` trong component
   `Instrument` — mà `Instrument` thì `required='Y'`. Một component bắt buộc **không** kéo
   theo field bắt buộc nào. Đáp án đúng: `[11, 40, 54, 60]`. Đệ quy component không đổi kết
   quả này (nhưng đổi ở **21/93** message khác).
   → **Quyết định của chủ repo, 2026-08-28: sửa test theo dữ liệu, giữ hoãn đệ quy.** Đệ quy
   vẫn thuộc plan repeating-groups. Thêm 2 test ghim giới hạn để không ai dùng nhầm.

2. **`data_length_tag` không phải `tag − 1`** — `Signature(89)` → `SignatureLength(93)`.
   Khớp theo tên. Generator **từ chối sinh bảng** nếu có field DATA không khớp được, thay vì
   trả `None` — `None` nghĩa là "quét `0x01`", tức trả lời sai dưới dạng mặc định.

3. **`<header>` có `<group>`** — `NoHops(627)` + 628/629/630. Phải đi vào group.

Cả ba vào `reference/fix44-dictionary-traps.md`, mỗi cái kèm test canh. Bẫy 1 đã kiểm chứng
bằng đảo ngược: đổi sang `tag − 1` → đúng 1 test đỏ, `left: Some(88), right: Some(93)`.

### Bước 2 — xong 2026-08-28. Plan sai thêm 2 chỗ

`codec` đọc được FIX. 29 test xanh (11 dict, 14 parser, 4 corpus). `classified 539/539`.

**Chỗ sai thứ tư — plan tự mâu thuẫn về `2t`.** Sơ đồ ranh giới xếp "sai thứ tự (2t)" vào
phần session; thuật toán parse bước 1 lại bắt parser từ chối cả ba vị trí `8=`, `9=`, `35=`.
Dữ liệu cho thấy `2t` có **hai** bản tin sai khác hẳn nhau: một cái `35=` đứng trước `8=`
(**không đóng khung được** — không biết bản tin kết thúc ở đâu), một cái `34=` đứng trước
`35=` (đóng khung bình thường). QuickFIX bỏ im lặng cả hai, không tăng seq.
→ **Quyết định của chủ repo: chỉ từ chối cái không đóng khung được.** Thêm
`ParseError::BadFrameStart`. Vị trí `35=` để session phán.

**Chỗ sai thứ năm — `14a` không pass được với `Err` thuần.** `-1=HI` là tag không đọc nổi
thành `u32`, nhưng `@expected` ghi rõ *"Send Reject … Increment inbound MsgSeqNum"* — session
phải đọc `34=4` và đặt text `-1` vào `371=`. Đây là ca duy nhất trong 539 dòng.
→ **Quyết định của chủ repo:** `ParseError::BadTag` mang **offset byte** thay vì giá trị, và
index **giữ mọi field đọc được trước chỗ hỏng**. Thêm `tag_text_at(buf, at)`. Cùng lý lẽ
với D12. Cùng file đó gửi `999=`, `0=`, `5000=` — cả ba parse bình thường, session tra từ
điển rồi Reject; chỉ cái không đọc nổi mới dừng ở codec.

**Phát hiện lớn nhất, đã ghi vào `reference/quickfix-acceptance-def-format.md`:** trong 244
dòng `E` mang `10=`, **không dòng nào** là checksum thật. Một conformance runner đi xác thực
checksum trên dòng kỳ vọng sẽ đỏ cả 244 và không học được gì.

### Bước 3, 4, 5 — xong 2026-08-28. **Bước 1 ĐÓNG.**

**54 test xanh.** Điều kiện đóng bước — `tests/stream.rs` — đạt: 533 bản tin thật đi qua vòng
lặp đọc TCP giả lập, **5 kiểu chia mẩu khác nhau** cộng thêm kiểu **từng byte một**, không sót,
không nhân đôi, mọi mẩu cụt trả `Incomplete`.

| Đo được | Số | Mục tiêu công bố |
|---|---|---|
| parse `NewOrderSingle`, validation đầy đủ | **77,0 ns** | ≤ 150 — đạt, dư 2× |
| parse `Heartbeat` | 35,0 ns | — |
| encode `ExecutionReport`, 3 field cố định + 14 slot | **93,8 ns** | ≤ 60 — **KHÔNG đạt, thiếu 56%** |
| `SendingTime` từ cache | 1,8 ns | — |
| **Cấp phát: parse / encode / tra field** | **0 / 0 / 0** | 0 |
| Fuzz | **304.230.294 lượt / 601 giây, 0 crash** | — |

Máy: Apple M5, macOS, **không pin nhân**. Ước lượng là **tốt nhất trong 7 lần × 200.000 vòng**,
tức lạc quan. Hai lần chạy liên tiếp lệch ~6% (72,8 → 77,0 ns) — đó là độ chính xác thật của
setup này. Chi tiết ở `reference/measured-costs.md` §5.

**Vì sao encode chậm hơn mục tiêu:** `encode` tra mỗi slot bằng quét tuyến tính danh sách
caller đưa, nên chi phí là slot × part. **Không tối ưu**, đúng theo *Ngoài phạm vi* — đo trước,
tối ưu sau, và số quyết định là số trên Linux ở bước `engine`.

**Round-trip có một kết quả đáng chú ý.** 533 bản tin parse rồi dựng lại bằng `Template`:
**505 trùng byte, 28 bị sắp lại**. Và khẳng định mạnh hơn một con số: bản tin trùng byte
**khi và chỉ khi** nguồn đã đúng thứ tự chuẩn và `9=` đúng — kiểm trên cả 533 dòng, không dòng
nào phá quy tắc. 28 dòng lệch tự giải thích: `14g`, `15`, `2t` cố tình sai thứ tự; 6 dòng có
`9=` sai; phần còn lại là dòng `I` viết tay không tăng dần.

**Chỗ sai thứ sáu của plan — placeholder `<TIME±N>`.** Plan đếm 352 `<TIME>` và bỏ sót 4 dạng
có độ lệch: `<TIME+10>`, `<TIME+121>`, `<TIME-121>`, `<TIME-1>`. Loader không thay chúng thì
chèn 9 byte vào chỗ đáng lẽ 21 byte, và mọi độ dài đều sai mà không có gì nói tại sao.

**Criterion hoãn, có lý do.** `DESIGN.md` §6 gọi tên Criterion. Bench ở đây là harness 24 dòng
tự viết, không dependency, vì **bench phải assert** mà Criterion thì đo chứ không assert. Đổi
lại mất outlier detection và khoảng tin cậy. Ghi vào `STATUS.md` Open items.

**Chưa làm, ghi lại:** 3 tag trailer (`89`, `93`, `10`) không được phân loại — `is_header` trả
`false` nên chúng sẽ xếp vào body nếu có ai ghi. Chưa ai ghi. Có test ghim.

---

## Nhật ký review — 2026-08-27

16 quyết định từ `/plan-eng-review` + outside voice (Codex, `model_reasoning_effort=high`).
Mọi con số dưới đây đếm bằng script trên `vendor/`, không lấy từ tài liệu.

| # | Quyết định | Nguồn |
|---|---|---|
| D2 | Giữ phạm vi 2 crate / ~14 file. Vấn đề là số liệu và kiểu, không phải độ lớn | Step 0 |
| D3 | `MessageView` = 24 byte. Sửa 6 chỗ, assert `== 24`, đính chính ADR-0003 tại chỗ có ghi ngày | review |
| D4 | Fixture `.def`: bảng phân loại + chuẩn hoá `fixify!`. 539 / 247 / 244 | review + Codex #1, #2 |
| D5 | Conformance runner chạy **thuần trong tiến trình**, không socket. Sửa `DESIGN.md` §3, §7 và reference dòng 121 | review |
| D6 | Sửa cả 5 mâu thuẫn trong `DESIGN.md`, cùng commit | review |
| D7 | `Template` **tự sở hữu** buffer inline; `encode(out, &[(u32, &[u8])])` | review + Codex #6 |
| D8 | Thêm `MissingLengthField` + `LengthOutOfBounds`, kèm test và hạt giống fuzz | review |
| D9 | `parse_into` trả `Ok(Parsed::Incomplete)` — trạng thái riêng trong nhánh Ok | review + Codex #5 |
| D10 | `fuzz/` là crate ngoài workspace, chạy `cargo +nightly fuzz`. Thư viện giữ stable | review |
| D11 | Bench assert **trần hồi quy ~1,5–2× baseline**, không phải 150/60 ns | review + Codex #13 |
| D12 | **Bỏ `ParseError::EmptyValue`.** Parser biết cú pháp, session biết luật | Codex #3, kiểm chứng trên `14d` + `ReverseRouteWithEmptyRoutingTags` |
| D13 | Slot tùy chọn + test `template_reuse` (một template, 38 dòng Reject, 8 mẫu) | Codex #7, kiểm chứng bằng đếm |
| D14 | Giữ thứ tự `DESIGN.md` §7, nhưng **đổi điều kiện đóng bước 1** sang `tests/stream.rs` | CROSS-MODEL TENSION, Codex #14 |
| D15 | Gấp vào plan: tham số `Validation` thật + ngữ nghĩa giới hạn (`FieldTooLong`, `BodyTooLong`, `OutputTooSmall`) | Codex #4, #11 |
| D16 | 3 mục hoãn → `STATUS.md` Open items, không tạo `TODOS.md` | review |
| D18 | Trait `Dictionary` nhận sẵn 3 hàm group (mặc định rỗng) ở bước 1 | plan repeating-groups, rủi ro mức Cao |

**Codex #8, #9, #10, #12 được xem xét và hoãn có chủ ý** — xem *Ngoài phạm vi*. #10 (câu chữ
`no_std`) đã sửa luôn trong bảng *Bất biến bị đụng tới*.

## GSTACK REVIEW REPORT

| Runs | Status | Findings |
|---|---|---|
| `/plan-eng-review` (Architecture, Code Quality, Tests, Performance) | issues_found | 9 |
| Outside voice — Codex `exec`, `model_reasoning_effort=high`, read-only | issues_found | 14 (7 mới, 5 trùng, 2 hoãn) |
| Kiểm chứng độc lập — `rustc -O`, script đếm trên `vendor/` | confirmed | 6 claim sai trong tài liệu |

**Độ phủ test:** trước review 17/32 nhánh (53%). Sau 16 quyết định: 30/32 nhánh có test khai
tên. Còn 2 GAP có chủ ý — `NANOFIX_FIX44_XML` override, và XML hỏng giữa chừng.

**Critical gaps đã đóng:** 2 — bản tin cụt trên luồng TCP (D9), và giá trị rỗng chặn 2/59
định nghĩa acceptance (D12).

**Song song hoá:** Lane A = `dict` (bước 1). Lane B = loader fixture + `codec::parse`
(bước 2). Hai lane không chung module, chạy song song được. Bước 3 chờ cả hai; bước 4 chỉ
chờ bước 2; bước 5 chờ tất cả. Xung đột duy nhất: cả hai lane đụng `Cargo.toml` workspace —
làm bước 0 xong hẳn trước khi tách lane.

CODEX absorbed: #1, #2, #3, #4, #5, #6, #7, #11, #13 → D4, D12, D15, D9, D7, D13, D11.
CODEX deferred: #8, #9, #12 → `STATUS.md` Open items. #10 → sửa trực tiếp.
CROSS-MODEL resolved: #14 → D14, giữ thứ tự build, đổi điều kiện đóng bước.

**VERDICT: APPROVED WITH CHANGES.** Plan này khi chưa sửa sẽ đỏ ngay commit đầu (assert
`== 16` không biên dịch) và không thể đạt 59/59 (giá trị rỗng). Sau 16 quyết định, mọi tiêu
chí nghiệm thu đều đếm được trên dữ liệu thật và mọi chữ ký đều biên dịch được.

NO UNRESOLVED DECISIONS
