# Repeating group — đọc, ghi, và kiểm đếm

> **Loại:** Plan · **Ngày:** 2026-08-27 · **Trạng thái:** Chờ duyệt
> **Phạm vi:** [PRD.md](../PRD.md) phase 1, tiêu chí 2 — lỗ hổng lớn nhất so với QuickFIX

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Một bản tin FIX có thể chứa **danh sách lặp**: 3 bên tham gia, 5 mức giá, 2 phiên giao dịch.
Danh sách đó mở đầu bằng một field đếm (`386=3`), rồi lặp lại một cụm field. Đây không phải
tính năng bên lề — **75 trong 93 loại bản tin FIX 4.4 có ít nhất một danh sách lặp**.

Việc này nổi lên khi viết [PRD.md](../PRD.md): đối chiếu với QuickFIX thì repeating group là
lỗ hổng lớn nhất, và nó **không nằm trong plan nào**. Plan `codec + dict` cố ý để nó ra ngoài
phạm vi bước 1, đúng — nhưng "ngoài phạm vi bước 1" đã âm thầm thành "ngoài phạm vi luôn".

Điều làm việc này nguy hiểm hơn vẻ ngoài: **bộ 59 acceptance definition gần như không kiểm
được nó.** Đạt 59/59 rồi vẫn có thể sai hoàn toàn về group. Cổng chính của cả dự án có một
điểm mù, và đây là điểm mù đó.

Xong việc này thì `codec` đọc/ghi được bản tin ứng dụng thật — `NewOrderSingle` có `Parties`,
market data có `MDEntries`, allocation có `NoAllocs`. Chưa xong thì engine chỉ nói được tầng
session.

## Những gì đã biết chắc

Mọi con số dưới đây đếm bằng script trên `vendor/quickfix/spec/FIX44.xml` và
`test/definitions/server/fix44/`, ngày 2026-08-27.

| Sự thật | Con số |
|---|---|
| Loại bản tin có ít nhất một group | **75 / 93** |
| Khai báo `<group>` trong dictionary | **93** |
| **Trong đó khai báo bên trong `<component>`** | **91 / 93** — chỉ 1 nằm thẳng trong `<message>` |
| Vị trí group thực tế sau khi mở hết component | **1028** |
| Counter tag khác nhau | **59**. Mỗi counter tag ứng đúng một tên group, không dùng lại |
| Độ lồng sâu nhất trong một bản tin | **4** (`TradeCaptureReport`) |
| **Counter tag có delimiter KHÁC NHAU tùy ngữ cảnh** | **4** — xem bảng dưới |
| Group được điền trong 59 file `.def` | **1** — `386=3` trong `14i`, và đó là test đếm SAI |
| `454` xuất hiện trong `.def` | 2 lần, cả hai đều `=0` |

**Bốn counter tag không thể tra bằng một bảng phẳng:**

| Counter | Delimiter | Ở đâu |
|---|---|---|
| `268` NoMDEntries | **269** | `MarketDataSnapshotFullRefresh` |
| `268` NoMDEntries | **279** | `MarketDataIncrementalRefresh` |
| `124` NoExecs | **17** | `ExecCollGrp` — 6 bản tin Collateral* |
| `124` NoExecs | **32** | `ExecAllocGrp` — `AllocationInstruction`, `AllocationReport` |
| `420` NoBidComponents | **12** / **66** | `BidCompRspGrp` / `BidCompReqGrp` |
| `295` NoQuoteEntries | **55** / **299** | `QuotCxlEntriesGrp` / `QuotEntryGrp` |

`268` là ca đau nhất: market data snapshot và incremental dùng **cùng counter tag, khác
delimiter**. Một bảng `delimiter(counter)` toàn cục sẽ **cắt sai im lặng** mọi incremental
refresh — đúng loại dữ liệu cần `FieldIndex<512>` và chạy nhiều nhất.

**Mẩu dữ liệu group thật duy nhất tồn tại**, `14i_RepeatingGroupCountNotEqual.def`:

```
I … 55=INTC ␁ 386=3 ␁ 336=PRE-OPEN ␁ 336=AFTER-HOURS ␁ 60=<TIME> ␁
E … 35=3 ␁ 45=2 ␁ 58=Incorrect NumInGroup count for repeating group ␁ 371=386 ␁ 372=D ␁ 373=16 ␁
```

Đọc được ba điều từ đây:

1. `386=3` khai 3 entry nhưng chỉ có 2 delimiter (`336`) theo sau → sai đếm.
2. Session phải trả `Reject` mang **`371=386`** (chính counter tag), `372=D`, `373=16`.
3. **`60=` kết thúc group** — `NoTradingSessions(386)` chỉ gồm `{336, 625}`, mà `60` không
   thuộc tập đó. Đây là quy tắc kết thúc group, xác nhận bằng dữ liệu thật.

Điều 2 khớp đúng ranh giới đã chốt ở [plan codec-dict](2026-08-27-codec-dict.md) D12:
**`codec` chỉ từ chối cái nó không đọc nổi; sai luật là việc của `session`.** Nếu parser trả
`Err` khi đếm sai thì session không có gì để đặt vào `371=386` và không tăng được seq —
`14i` không thể pass. Giống hệt `14d`.

| Sự thật khác | Nguồn |
|---|---|
| Thứ tự trong group là **thứ tự khai báo, delimiter trước** — chứ không phải tag tăng dần | [reference/quickfix-acceptance-def-format.md](../reference/quickfix-acceptance-def-format.md). **CHƯA kiểm chứng** — 59 def không có group nào được điền để đối chiếu |
| `dict::required()` hiện không đệ quy qua `<component>` | `STATUS.md` open item 8 |
| `Template` sắp header tăng dần rồi body tăng dần | plan codec-dict, quyết định D7/D13 |

## Cách làm

### Nguyên tắc: parser không đổi. Group được giải lười.

```
   byte trên dây
        │
        ▼
  parse_into ──────────► FieldIndex PHẲNG, y như cũ, 139 ns
        │                (386, off, len) (336, off, len) (336, …) (60, …)
        │                  không biết gì về group, không tốn gì cho bản tin không có group
        ▼
  view.group(msg_type, 386)
        │  hỏi dict: (D, 386) → delimiter 336, tập thành viên {336, 625}
        ▼
  GroupIter — con trỏ chạy trên index phẳng
        ├─ entry 1: gặp 336 → bắt đầu; gom tới trước 336 kế
        ├─ entry 2: gặp 336 → entry mới
        └─ gặp 60 (ngoài tập thành viên) → HẾT group.  Đếm được 2, khai 3 → lệch
```

Vì sao hình dạng này đúng với repo này:

1. **`parse_into`, `FieldIndex`, `MessageView` không đổi một chữ.** API công khai vừa chốt ở
   plan codec-dict giữ nguyên. Không breaking change.
2. **Bản tin không có group trả đúng 0 đồng.** Con số 139 ns không bị đụng.
3. **Không cấp phát.** `GroupIter` là con trỏ trên mảng có sẵn, không dựng cấu trúc cây.
4. **Tri thức thứ tự nằm ở bảng sinh, không ở call site** — giữ bất biến §2 điều 5.

### `crates/dict` — ba bảng mới, và đệ quy component

`build.rs` phải mở hết `<component>` mới thấy được group: **91/93 group nằm trong component**.
Việc này đóng luôn `STATUS.md` open item 8.

```rust
// Khoá là (msg_type, counter) — KHÔNG phải counter một mình.
// Bốn counter 268/124/420/295 có delimiter khác nhau tùy bản tin.
pub fn group_delimiter(msg_type: &[u8], counter: u32) -> Option<u32>;

// Tập tag thuộc group này. Gặp tag ngoài tập → group kết thúc.
// Bao gồm cả counter tag của group con, để đệ quy được.
pub fn group_members(msg_type: &[u8], counter: u32) -> &'static [u32];

// Thứ tự khai báo trong group, delimiter đứng đầu. Dùng khi GHI.
pub fn group_order(msg_type: &[u8], counter: u32) -> &'static [u32];
```

### `crates/codec` — `Dictionary` mở rộng, `GroupIter`, lồng nhau

Trait `Dictionary` hiện có 2 hàm. Thêm 3 hàm trên. **Việc này phải làm ở bước 1 của plan
codec-dict, không đợi plan này** — xem *Rủi ro*.

```rust
pub struct GroupIter<'a, D: Dictionary, const N: usize> { … }   // không cấp phát

impl<'a, const N: usize> MessageView<'a, N> {
    /// None nếu không có counter tag. Sai đếm KHÔNG phải lỗi ở đây — xem `GroupIter::declared`.
    pub fn group<D: Dictionary>(&self, msg_type: &[u8], counter: u32) -> Option<GroupIter<'a, D, N>>;
}

impl<'a, D: Dictionary, const N: usize> GroupIter<'a, D, N> {
    pub fn declared(&self) -> u32;        // giá trị của counter tag, ví dụ 3
    pub fn counted(&self) -> u32;         // số entry thật sự tìm thấy, ví dụ 2
    pub fn next(&mut self) -> Option<GroupEntry<'a, N>>;
}

impl<'a, const N: usize> GroupEntry<'a, N> {
    pub fn get(&self, tag: u32) -> Option<&'a [u8]>;
    /// Group lồng trong entry này. Sâu nhất trong FIX 4.4 là 4 tầng.
    pub fn group<D: Dictionary>(&self, msg_type: &[u8], counter: u32) -> Option<GroupIter<'a, D, N>>;
}
```

`declared()` và `counted()` tách rời là điểm mấu chốt: **parser không phán, nó chỉ đếm.**
Session so hai số và tự quyết có `Reject 373=16` hay không, và nó có sẵn counter tag để đặt
vào `371=`.

### `crates/codec/src/template.rs` — thêm `Part::Group`

Đây là chỗ plan này **đụng vào quyết định vừa chốt**. D7/D13 định nghĩa `Template` sắp
`35` trước, header tăng dần, body tăng dần. **Bên trong group thì quy tắc đó không áp dụng** —
thứ tự là thứ tự khai báo, delimiter đứng đầu.

```rust
enum Part {
    Static(Range<u16>),
    Slot(u32),
    Group { counter: u32, order: &'static [u32] },   // MỚI — thứ tự khai báo, không sort
}
```

Lúc ghi: đặt counter tag ở đúng vị trí tag tăng dần của nó trong body, rồi ghi n entry liên
tiếp, mỗi entry theo `group_order`.

### File tạo/sửa

```
crates/dict/build.rs              + đệ quy component, + 3 bảng group
crates/dict/src/lib.rs            + 3 hàm public
crates/codec/src/dict.rs          + 3 hàm vào trait Dictionary, NoDict trả None/rỗng
crates/codec/src/group.rs         MỚI — GroupIter, GroupEntry
crates/codec/src/index.rs         + MessageView::group()
crates/codec/src/template.rs      + Part::Group
crates/codec/tests/groups.rs      MỚI
crates/codec/tests/group_roundtrip.rs  MỚI
tools/interop/                    MỚI — sinh/đối chiếu group qua libquickfix
```

## Bất biến bị đụng tới

| # | Điều | Cách giữ |
|---|---|---|
| 1 | Không cấp phát trên hot path | `GroupIter` là con trỏ trên `FieldIndex` có sẵn, không dựng cây, không `Vec`. `benches/alloc.rs` thêm ca "duyệt group" và phải in `allocations: 0` |
| 3 | 59/59 là cổng của session | Việc này **không được làm tụt** con số đó. Chạy lại bộ 59 sau khi đổi `Dictionary` trait |
| 5 | Thứ tự field từ bảng sinh, không từ call site | `group_order` sinh từ XML. Không có API nào cho caller tự chọn thứ tự trong group |
| 7 | Không `unwrap`/`expect`/`panic` | Đếm lệch, group lồng quá sâu, counter phi số — tất cả trả `Option`/`Err`, không panic |
| 8 | `unsafe` phải có chứng minh | **Mục tiêu: 0 `unsafe`** |
| 10 | Số hiệu năng phải kèm bench + máy + settings | Bench mới: parse bản tin **có** group so với **không** group, chứng minh đường không-group không đổi |

Điều 2, 4, 6, 9 không đụng.

## Chia việc

| Bước | Kết quả | Thời gian | Phụ thuộc |
|---|---|---|---|
| 0 | Nhánh `plan/repeating-groups`. `dict/build.rs` đệ quy `<component>`; `required()` đầy đủ. **Đóng `STATUS.md` open item 8** | 2 ngày | plan codec-dict xong bước 1 |
| 1 | `dict`: 3 bảng group khoá theo `(msg_type, counter)`. Test: `group_delimiter(b"W", 268) == Some(269)` **và** `group_delimiter(b"X", 268) == Some(279)` | 2 ngày | 0 |
| 2 | `GroupIter` một tầng. `declared()` / `counted()` tách rời. Chạy được trên dòng `I` của `14i`: `declared()==3`, `counted()==2` | 2 ngày | 1, và codec bước 2 |
| 3 | Group lồng tới 4 tầng (`TradeCaptureReport`). `GroupEntry::group()` | 2 ngày | 2 |
| 4 | `Part::Group` trong `Template`. Round-trip bản tin có group | 2 ngày | 3, và codec bước 4 |
| 5 | `tools/interop`: **xác minh quy tắc thứ tự trong group** bằng libquickfix. Bench. Cập nhật docs. Merge | 2 ngày | 4 |

**Tổng ~10 ngày nếu quen Rust.** Bước 3 là chỗ khó nhất — lồng nhau trên một index phẳng.

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 0 | `cargo test -p dict -- component_recursion` | `required(b"D")` chứa field nằm trong `Parties`, `PreAllocGrp`, `TrdgSesGrp` — hiện đang thiếu |
| 1 | `cargo test -p dict -- group_tables` | In `groups: 59 counters, 1028 positions`. Và **4 ca nhập nhằng phải đúng**: `(W,268)→269`, `(X,268)→279`, `(J,124)→32`, `(BA,124)→17` |
| 2 | `cargo test -p codec --test groups -- count_mismatch` | Trên dòng `I` thật của `14i`: `declared()==3`, `counted()==2`, và `parse_into` vẫn trả **`Ok`** — không phải `Err` |
| 2 | `cargo test -p codec --test groups -- terminator` | Group `386` dừng đúng ở `60=`, không nuốt `60` vào entry cuối |
| 3 | `cargo test -p codec --test groups -- nested` | Bản tin `TradeCaptureReport` dựng theo dictionary, lồng 4 tầng, đọc đúng field ở tầng sâu nhất |
| 4 | `cargo test -p codec --test group_roundtrip` | parse → encode → **byte-identical**, trên tập bản tin sinh từ dictionary phủ cả 59 counter tag |
| 5 | `tools/interop --verify-group-order` | **Xác minh** quy tắc "thứ tự khai báo, delimiter trước" bằng byte do libquickfix phát ra. Không khớp → sửa `group_order`, không sửa test |
| 5 | `cargo bench -p codec -- group` | Parse bản tin **không** group không đổi so với baseline. Duyệt group in `allocations: 0` |
| mọi bước | Bộ 59 acceptance | **Vẫn 59/59.** Đổi trait `Dictionary` mà làm tụt là chặn merge |

**Dữ liệu thật:** chỉ có đúng một dòng — `14i`. Bước 2 chạy trên chính nó. Mọi thứ khác là
bản tin **sinh từ dictionary** (ghi rõ là sinh, không phải bắt được), cho tới khi bước 5 lấy
được byte thật từ libquickfix.

**Bằng chứng đỏ trước:** mỗi bước, commit đầu là test đỏ, output trích trong commit message.

## Tài liệu phải cập nhật

Theo bảng đồng bộ `CLAUDE.md` §4. *(Ghi chú: `_template.md` chỉ sai sang §3 — sửa luôn.)*

- [ ] `DESIGN.md` §4 D2: `MessageView` thêm `group()`; nêu rõ index vẫn phẳng, group giải lười.
- [ ] `DESIGN.md` §4 D3: bổ sung — quy tắc tag tăng dần **không** áp dụng bên trong group.
- [ ] `DESIGN.md` §6: thêm dòng gate "group round-trip" và "thứ tự trong group đã xác minh".
- [ ] `reference/quickfix-acceptance-def-format.md`: ghi điểm mù — 59 def điền đúng 1 group, và
      đó là test âm. Hạ câu về thứ tự trong group xuống "chưa kiểm chứng" cho tới bước 5.
- [ ] `reference/` — trang mới cho 4 counter tag nhập nhằng. **Đây là bẫy đắt nhất ở đây**, và
      `CLAUDE.md` §4 xếp `reference/` ưu tiên cao nhất.
- [ ] `PRD.md` §3: đổi hàng "Repeating groups" khi xong.
- [ ] `STATUS.md`: đóng open item 8, ghi số đo.
- [ ] `CHANGELOG.md`.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| **Tra delimiter chỉ bằng counter tag** → cắt sai mọi `MarketDataIncrementalRefresh` | `(W,268)→269` và `(X,268)→279` trong cùng một test. Bảng khoá một chiều là đỏ |
| Sai đếm bị coi là lỗi parse → `14i` không pass được | `parse_into` phải trả `Ok`; `declared()`/`counted()` tách rời. Cùng nguyên tắc D12 |
| Group nuốt field đứng sau nó (`60=`) | Test `terminator` trên `14i` |
| Group rỗng `454=0` | 2 dòng thật trong `.def`. `counted()==0`, `next()` trả `None` ngay, không lặp vô hạn |
| Counter khai lớn hơn số entry thật → vòng lặp không dừng | Vòng lặp bị chặn bởi **hết index**, không bởi `declared()`. Fuzz với `386=4294967295` |
| Delimiter xuất hiện *trước* counter tag | Trả `None`, không quét ngược |
| Group lồng: entry con nuốt sang entry cha kế tiếp | Test `nested` 4 tầng, kiểm số entry ở **từng** tầng |
| Field DATA chứa `0x01` bên trong group | Đường DATA đã có ở codec bước 3; thêm ca DATA trong group |
| `Template` sort tag tăng dần bên trong group | `group_roundtrip` byte-identical bắt được; và `Part::Group` không gọi hàm sort |
| Quy tắc "thứ tự khai báo" chỉ là lời văn trong reference, chưa ai kiểm | Bước 5 xác minh bằng byte libquickfix phát ra. Cho tới đó, đánh dấu chưa kiểm chứng |
| Đổi trait `Dictionary` làm tụt 59/59 | Chạy lại bộ 59 ở mọi bước |
| Bản tin không có group phải chậm đi | Bench so hai đường; lệch quá trần hồi quy là đỏ |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| **Trait `Dictionary` là API công khai; thêm hàm sau là breaking change** | **Cao** | Ba hàm group phải vào trait **ngay ở bước 1 của plan codec-dict**, dù `NoDict` chỉ trả `None`/rỗng. Cùng logic `Role` ở ADR-0004: rẻ bây giờ, gãy sau |
| Không có oracle thật cho group | Cao | Đúng một dòng dữ liệu thật (`14i`). Bước 5 dựng interop với libquickfix — ADR-0004 đã kéo sẵn toolchain đó vào CI, việc này dùng ké |
| Quy tắc thứ tự trong group có thể sai | Trung bình | Nó đang là lời văn chưa kiểm chứng. Bước 5 kiểm; nếu sai thì sửa `group_order`, và ghi vào `reference/` như một bẫy đã trả giá |
| Lồng 4 tầng trên index phẳng khó viết đúng | Trung bình | Bước 3 tách riêng, làm sau khi một tầng đã xanh. Không gộp bước 2 và 3 |
| `group_members` sinh ra bảng lớn, phình nhị phân | Thấp | 59 counter × trung bình vài chục tag. Đo `cargo bloat` ở bước 1; nếu lớn thì gộp bảng dùng chung |
| Plan này chạy song song với codec-dict gây xung đột | Trung bình | **Không chạy song song.** Bước 0 chỉ bắt đầu sau khi codec-dict xong bước 1 |

## Ngoài phạm vi

- **Validation đầy đủ theo dictionary** — kiểu field, giá trị enum, tag lạ, field bắt buộc
  trong group. Là tiêu chí 3 của PRD phase 1, plan riêng. Việc này chỉ lo **cấu trúc**.
- **Kiểu decimal / giá** — vẫn là bytes và số nguyên.
- **FIX 5.0 / FIXT 1.1** — bảng group sinh cho FIX 4.4. Phase 2.
- **Group trong SBE** — SBE có khái niệm group riêng, không dùng delimiter. Phase 2.
- **Tối ưu tra group** — quét tuyến tính trước. Đo rồi mới tối ưu.

## Nhật ký giao hàng

*(trống — điền khi đóng từng bước)*
