# Bước 1 — `codec` và `dict`: đọc, ghi FIX 4.4 không cấp phát bộ nhớ

> **Loại:** Plan · **Ngày:** 2026-08-27 · **Trạng thái:** Chờ duyệt
> **Phạm vi:** `DESIGN.md` §7 bước 1 — hai crate đầu tiên của workspace

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

## Những gì đã biết chắc

| Sự thật | Nguồn |
|---|---|
| Mảng field 512 phần tử nằm trong struct làm parse chậm 4–6 lần; tách index ra, N=64 → heartbeat 95 ns, `NewOrderSingle` 139 ns trên M5 | `reference/measured-costs.md` §1, đo 2026-08-27 |
| Thứ tự field trong bản tin phát ra: `8, 9, 35` cố định → header **tag tăng dần** → body **tag tăng dần** → `10`. Không phải thứ tự XML. 247/247 dòng expected, không ngoại lệ | `reference/quickfix-acceptance-def-format.md`, mục "The ordering rule" |
| Bộ so sánh của acceptance test so **theo vị trí** và so `9=` **bằng chuỗi chính xác** — BodyLength phải đúng từng byte, không đệm số 0 | cùng trang, mục "Comparison rules" |
| Ký tự phân cách là byte `0x01` thật trong file, không phải escape | cùng trang, hexdump |
| `spec/FIX44.xml` dùng thuộc tính **nháy đơn** (`name='BeginString'`); header khai báo 8, 9, 35, 49, 56, 115, 128… | `vendor/quickfix/spec/FIX44.xml` dòng 2–10 |
| Trong FIX 4.4, field kiểu DATA đi kèm một field độ dài đứng ngay trước nó (ví dụ `RawDataLength(95)` → `RawData(96)`, `XmlDataLen(212)` → `XmlData(213)`); giá trị DATA **được phép chứa `0x01`** | Đặc tả FIX 4.4, và danh sách `type='DATA'` trong XML — sinh lúc build, không liệt kê tay |
| 289 dòng `I` và 247 dòng `E` trong 59 file `.def` là bản tin FIX **thật** do QuickFIX phát — dữ liệu test có sẵn, không phải tự bịa | `vendor/quickfix/test/definitions/server/fix44/` |
| QuickFIX Software License cho phép dùng XML và `.def` làm dữ liệu; **không** commit chúng | ADR-0001 |
| `MessageView` phải là hai word, `Copy`; `FieldIndex<const N>` do người gọi chọn N | ADR-0003 (Accepted) |
| Serialize theo template: phần tĩnh encode sẵn một lần, mỗi lần gửi chỉ ghi phần động; `SendingTime` cache tiền tố `YYYYMMDD-HH:MM` | `DESIGN.md` D9 |
| Gate: parse `NewOrderSingle` ≤ 150 ns, serialize `ExecutionReport` ≤ 60 ns, **0** cấp phát trên hot path | `DESIGN.md` §6 |

## Cách làm

### `crates/dict` — sinh bảng từ XML lúc build

- `build.rs` đọc `vendor/quickfix/spec/FIX44.xml` (đường dẫn ghi đè được bằng biến môi
  trường `NANOFIX_FIX44_XML`). **Thiếu file → build fail với thông báo chỉ thẳng tới
  `scripts/fetch-quickfix-assets.sh`.** Không im lặng, không fallback.
- Dependency **chỉ ở build**: `roxmltree`. Runtime của `dict` không có dependency nào.
- Sinh ra `$OUT_DIR/fix44.rs`, gồm:
  - `pub mod tag` — hằng số `pub const MSG_SEQ_NUM: u32 = 34;` cho mọi field.
  - `pub mod msg_type` — hằng `pub const NEW_ORDER_SINGLE: &[u8] = b"D";`.
  - `pub fn is_header(tag: u32) -> bool` — bảng tra, dùng để tách header/body khi ghi.
  - `pub fn data_length_tag(tag: u32) -> Option<u32>` — với field kiểu DATA, trả về tag
    độ dài đứng trước nó. Dùng để parse đúng giá trị có chứa `0x01`.
  - `pub fn required(msg_type: &[u8]) -> &'static [u32]` — field bắt buộc theo loại bản
    tin. Session layer dùng sau; sinh luôn ở đây vì cùng nguồn.
- `dict` implement trait `codec::Dictionary` (định nghĩa bên `codec`, xem dưới).

### `crates/codec` — đọc và ghi, `#![no_std]`, không dependency

**Kiểu dữ liệu** (đúng ADR-0003):

```rust
#[repr(C)] pub struct FieldEntry { tag: u32, offset: u32, len: u16, _pad: u16 }   // 12 byte
pub struct FieldIndex<const N: usize> { count: u16, fields: [FieldEntry; N] }
#[derive(Clone, Copy)] pub struct MessageView<'a, const N: usize> { buf: &'a [u8], idx: &'a FieldIndex<N> }
pub type OrderView<'a> = MessageView<'a, 64>;
```

**Trait `Dictionary`** — `codec` chỉ cần hai câu hỏi, và hỏi qua trait để không phụ thuộc
`dict`:

```rust
pub trait Dictionary { fn is_header(tag: u32) -> bool; fn data_length_tag(tag: u32) -> Option<u32>; }
```

Hàm được `#[inline]`, không `dyn`. `codec` ship sẵn `NoDict` (mọi câu trả lời là "không")
để test nội bộ.

**Đọc — `parse_into::<D: Dictionary, const N>(buf, &mut FieldIndex<N>) -> Result<usize, ParseError>`**

1. Kiểm tra `8=` ở byte 0, `9=` ngay sau, `35=` ngay sau nữa — sai vị trí là lỗi.
2. Quét tuyến tính: đọc tag (số thập phân, tràn `u32` là lỗi), `=`, rồi tìm `0x01`. Với
   tag mà `D::data_length_tag` trả `Some(len_tag)`, **không tìm `0x01`** mà lấy đúng số
   byte đã đọc được từ `len_tag` ngay trước đó.
3. Ghi `(tag, offset, len)` vào index. Vượt `N` → `ParseError::TooManyFields`, **không** cắt
   bớt.
4. Khi gặp `10=`: kiểm `BodyLength` (số byte từ sau `0x01` của `9=` đến trước `10=`) và
   `CheckSum` (tổng byte mod 256 của mọi thứ trước `10=`, ba chữ số). Hai kiểm tra này
   tắt được bằng `Validation` flags — mặc định bật.
5. Trả về số byte đã tiêu thụ, để caller xử lý buffer chứa nhiều bản tin.

`ParseError` là enum `Copy`, mang tối đa một `u32` (tag hoặc vị trí). Không `String`.

**Tra field**: `view.get(tag) -> Option<&[u8]>` quét tuyến tính, trả lần xuất hiện đầu.
`view.find_from(pos, tag)` cho repeating group. Bộ chuyển kiểu: `as_u32`, `as_i64`,
`as_char` — trả `Result`, không panic. **Không** có kiểu decimal trong bước này.

**Ghi — `Template`**

Một template là **danh sách đã sắp thứ tự** các mục, dựng một lần cho mỗi cặp (session,
loại bản tin):

```rust
enum Part { Static(&'static [u8]) /* "49=ISLD\x0156=TW44\x01" đã encode sẵn */, Slot(u32) }
pub struct Template<const P: usize> { parts: [Part; P], len: u8 }
```

- Lúc dựng: nhận các tag tĩnh kèm giá trị, các tag động; sắp theo quy tắc đã xác lập —
  `35` trước, header tăng dần, body tăng dần — rồi gộp các tag tĩnh liền nhau thành một
  `Static`. **Sắp xếp xảy ra lúc dựng, không xảy ra lúc gửi.**
- Lúc gửi, `encode(&self, out: &mut [u8], get: impl Fn(u32) -> &[u8]) -> Result<Range<usize>>`:
  1. Ghi body **từ vị trí `K`** trong `out` (`K` = độ dài tối đa của `8=FIX.4.4\x019=NNNNN\x01`).
  2. Ghi `8=FIX.4.4\x019=<len>\x01` **kết thúc đúng tại `K`**, canh phải. Không dịch chuyển
     buffer. `BodyLength` là số thật, không đệm `0` — bộ so sánh yêu cầu vậy.
  3. Tính checksum, ghi `10=NNN\x01`. Trả về khoảng `[start, end)` — caller gửi đúng
     khoảng đó.
- `SendingTime`: `TimestampCache` giữ 15 byte `YYYYMMDD-HH:MM:` và phút hiện tại; mỗi bản
  tin chỉ format `SS.mmm`. Đổi phút thì format lại tiền tố. Đây là một `Slot` đặc biệt.

**Cấu trúc file**

```
crates/codec/src/lib.rs        #![no_std], re-export
crates/codec/src/index.rs      FieldEntry, FieldIndex, MessageView, tra field
crates/codec/src/parse.rs      parse_into, ParseError, Validation
crates/codec/src/checksum.rs   tổng mod 256 — bản thường trước, SIMD chỉ khi đo thấy cần
crates/codec/src/template.rs   Template, Part, encode
crates/codec/src/timestamp.rs  TimestampCache
crates/codec/src/dict.rs       trait Dictionary, NoDict
crates/codec/benches/{parse,serialize,alloc}.rs
crates/codec/tests/defs.rs     nạp 536 dòng I/E từ vendor/ — nếu thiếu vendor: FAIL, không skip
crates/dict/build.rs, src/lib.rs
```

`Cargo.toml` workspace: thêm hai member, `[workspace.lints]` đã có. Thêm lint
`clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic` = `deny` cho hai crate này
(cho phép trong `#[cfg(test)]` và `benches/`).

## Bất biến bị đụng tới

| # | Điều | Cách giữ |
|---|---|---|
| 1 | Không cấp phát trên hot path | `#![no_std]` không có `alloc` → **không thể** cấp phát, trình biên dịch chặn. `benches/alloc.rs` đếm để chứng minh cả với caller |
| 5 | Thứ tự field từ bảng sinh, không từ call site | `Template` sắp lúc dựng theo `D::is_header` + sort tag. Không có API nào cho caller tự chọn thứ tự |
| 6 | Feature flag gate `mod` | Bước này không có feature nào. `build.rs` của `dict` chỉ đọc file XML, không gọi toolchain ngoài |
| 7 | Không `unwrap`/`expect`/`panic` | Clippy `deny` ở cấp crate — build fail nếu vi phạm |
| 8 | `unsafe` phải có chứng minh | **Mục tiêu: 0 `unsafe`** trong bước này. Nếu SIMD checksum cần, để bước sau kèm Miri |
| 9 | Không copy mã QuickFIX | `dict` chỉ đọc XML lúc build. Không nhìn `src/C++` khi viết parser |
| 10 | Số hiệu năng phải kèm bench + máy + settings | Số trong nhật ký giao hàng ghi rõ "M5, macOS, không pin, so sánh tương đối" |

Điều 2, 3, 4 (session thuần, 59/59, engine không ngủ) chưa đụng — chưa có session, chưa
có engine.

## Chia việc

| Bước | Kết quả | Thời gian | Phụ thuộc |
|---|---|---|---|
| 0 | Nhánh `plan/codec-dict`. Workspace lint `deny` unwrap/expect/panic. `cargo build` **fail đúng thông báo** khi thiếu `vendor/` | ½ ngày | — |
| 1 | `dict`: `build.rs` đọc XML → `is_header`, `data_length_tag`, `tag::*`, `msg_type::*`, `required`. Test: `is_header(34)==true`, `is_header(11)==false`, `data_length_tag(96)==Some(95)`, `required(b"D")` chứa 11, 21, 55, 54, 60, 40 | 2–3 ngày | 0 |
| 2 | `codec::index` + `parse_into` với `NoDict`. Đọc được **cả 536 dòng** I/E; `TooManyFields`; `BodyLength`/`CheckSum` đúng với 244 dòng có `9=` | 3 ngày | 0 |
| 3 | Parse với `dict` thật: field DATA chứa `0x01`. Fuzz parser (`cargo fuzz`, 10 phút) không panic | 1 ngày | 1, 2 |
| 4 | `Template` + `TimestampCache`. **Round-trip 247 dòng E**: parse → dựng template cùng tập tag → encode → **byte-identical** | 3 ngày | 2 |
| 5 | Ba bench, mỗi cái assert bound của nó. Chạy, ghi số vào nhật ký. Cập nhật docs theo bảng dưới. Merge | 1 ngày | 3, 4 |

**Tổng: ~11 ngày làm việc nếu quen Rust. Team chưa quen → tính 3–4 tuần.** Bước 2 là chỗ
học ownership thật sự; bước 4 là chỗ học lifetime. Đừng làm hai bước đó cùng lúc.

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 0 | `mv vendor vendor.bak && cargo build -p dict; mv vendor.bak vendor` | Output chứa đúng dòng "run scripts/fetch-quickfix-assets.sh". Không phải lỗi khác |
| 1 | `cargo test -p dict` | Xanh, và **đọc** output thấy tên 5 test kể trên |
| 2 | `cargo test -p codec --test defs` | In ra `parsed 536/536`, `bodylength ok 244/244`. Số thấp hơn là đỏ |
| 2 | Test `TooManyFields`: parse một dòng E thật với `FieldIndex<4>` | Trả `Err(TooManyFields)`, và **không** có index nào chứa 4 field "thành công" |
| 3 | `cargo fuzz run parse -- -max_total_time=600` | Không crash, không timeout |
| 4 | `cargo test -p codec --test roundtrip` | `identical 247/247`. Một byte lệch là đỏ, in ra dòng lệch dưới dạng `|` |
| 5 | `cargo bench -p codec` | Ba bench xanh **theo assert trong bench**, không phải theo mắt nhìn. Copy nguyên output vào nhật ký |
| 5 | `benches/alloc.rs` | In `allocations: 0` cho parse và encode |
| mọi bước | `cargo clippy --all-targets -- -D warnings` và `cargo test --no-default-features` | Sạch |

**Dữ liệu thật:** mọi test parse/round-trip chạy trên 536 dòng QuickFIX phát ra, nạp từ
`vendor/`. Test **không được skip** khi thiếu vendor — phải fail với thông báo rõ. Một test
skip âm thầm là test đã bị tắt.

**Bằng chứng đỏ trước:** với mỗi bước, commit đầu tiên là test đỏ, output trích trong
commit message. Xem `CLAUDE.md` §10.

## Tài liệu phải cập nhật

Theo bảng đồng bộ ở `CLAUDE.md` §4.

- [ ] `DESIGN.md` §3 bảng crate: `dict` phụ thuộc `codec` (vì trait `Dictionary`), không
      phải "—". Sửa cả sơ đồ nếu cần.
- [ ] `DESIGN.md` D3: sửa câu chữ — bảng sinh cần thiết là **tập header**, quy tắc thứ tự là
      **sort theo tag trong từng phần**. Ghi nguồn: `reference/quickfix-acceptance-def-format.md`.
- [ ] `DESIGN.md` D9: sửa "patch tại offset tính sẵn" thành "danh sách phần đã sắp, phần
      tĩnh encode sẵn, body ghi trước rồi prefix canh phải" — vì field FIX có độ rộng thay
      đổi, offset cố định không đúng.
- [ ] `README.md` layout: thêm `crates/codec`, `crates/dict`.
- [ ] `reference/measured-costs.md`: thêm mục §5 — số của chính mình, kèm máy và settings.
- [ ] `STATUS.md`: đóng plan, ghi số đo, ghi cái chưa làm.
- [ ] `CHANGELOG.md`: tạo mới, mục `Unreleased`.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Thứ tự field — sort theo tag, không theo XML | `roundtrip`: 247/247 byte-identical. Sai thứ tự là lệch byte |
| `BodyLength` đếm từ **sau** `0x01` của `9=` đến **trước** `10=` — lệch 1 là lỗi kinh điển | So với 244 giá trị `9=` thật trong `.def` |
| `BodyLength` phải là số thật, không đệm `0` — vì bộ so sánh so chuỗi | `roundtrip` bắt được; thêm test riêng với body dài 9, 99, 999 byte |
| `CheckSum` tính trên **mọi byte** trước `10=`, kể cả `8=` và `9=`; ba chữ số có đệm `0` | Test đối chiếu với cài đặt tham chiếu ngây thơ trong file test; xác nhận lần cuối khi conformance runner chạy thật |
| Field DATA chứa `0x01` — parser tìm `0x01` sẽ cắt sai | Test với `RawDataLength=5, RawData=ab\x01cd`. **Bịa** — không có mẫu thật trong `.def`; ghi rõ là mẫu theo đặc tả |
| Giá trị rỗng `55=` — hợp lệ về cú pháp nhưng FIX cấm | Trả `ParseError::EmptyValue`, không panic. Fuzz canh |
| Tag tràn số (`99999999999=`) hoặc không phải số | `ParseError::BadTag`, không panic. Fuzz canh |
| `MessageView` phình ra khi ai đó thêm field | `const _: () = assert!(size_of::<MessageView<64>>() == 16);` — compile fail |
| Vượt `N` bị cắt âm thầm thay vì báo lỗi | Test `FieldIndex<4>` trên dòng thật → phải `Err`, không `Ok` |
| `TimestampCache` sai khi đổi phút, đổi ngày | Test tại `23:59:59.999 → 00:00:00.000`; test `12:34:59.999 → 12:35:00.000` |
| Index tái dùng khi view cũ còn sống → view trỏ sai | Kiểu: `parse_into` mượn `&mut` index, view mượn `&` — trình biên dịch chặn. Thêm doc-test `compile_fail` chứng minh |
| Test skip âm thầm khi thiếu vendor | Test đọc vendor gọi `panic!` với thông báo trong `#[cfg(test)]` — cho phép panic ở test |
| Bench xanh nhờ compiler bỏ code chết | `black_box` trên input **và** output; kiểm bằng cách so ns/op với và không có `black_box` — phải gần nhau |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Team chưa quen Rust; bước 2 và 4 là chỗ ownership/lifetime khó nhất | Cao | Ước lượng đã nhân 1,5–2. Bước 2 xong hẳn rồi mới sang 4. Không `unsafe` để "cho qua" borrow checker |
| Số đo trên M5/macOS không phản ánh Linux | Trung bình | Ghi rõ trong nhật ký là số so sánh tương đối. Gate ≤150/≤60 ns coi là **tạm đạt** trên Mac; xác nhận trên Linux ở bước `engine` |
| `#![no_std]` gây ma sát (không `Vec`, không `format!`) | Trung bình | Đó là mục đích. Nếu thật sự kẹt, `alloc` feature cho `dict` được phép — `codec` thì không |
| Quy tắc thứ tự có ngoại lệ ngoài 247 dòng đã xem (repeating group) | Thấp | Ngoài phạm vi bước này. Ghi trong `Ngoài phạm vi` |
| `roxmltree` đổi API | Thấp | Chỉ ở build; pin version |
| CI không có `vendor/` | Trung bình | CI job chạy `scripts/fetch-quickfix-assets.sh` trước `cargo test`; test fail rõ nếu thiếu |

## Ngoài phạm vi

- **Repeating group có thứ tự riêng** — chỉ có `find_from`, không sắp xếp group khi ghi.
- **Kiểu decimal / giá** — chỉ bytes và số nguyên. Giá là việc của người dùng, hoặc bước sau.
- **SIMD cho checksum/quét `0x01`** — 139 ns không SIMD đã đạt gate. Đo trước, tối ưu sau.
- FIX 5.0 / FIXT 1.1, FIXML, FAST, SBE.
- Session, socket, engine, dispatch — bước 3 và 4 của `DESIGN.md` §7.
- Conformance runner `.def` — bước 2 của §7, plan riêng.

## Nhật ký giao hàng

*(trống — điền khi đóng từng bước)*
