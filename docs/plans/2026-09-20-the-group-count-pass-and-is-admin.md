# Lượt kiểm `373=16` và câu hỏi "message này có phải admin không"

> **Loại:** Plan · **Ngày:** 2026-09-20 · **Trạng thái:** Chờ duyệt
> **Phạm vi:** `crates/session`, `crates/dict` — đóng hai open item của `STATUS.md`

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Hai lỗi, một kế hoạch — và chúng tách rời được

Kế hoạch này gom **hai lỗi không liên quan nhau về mặt kỹ thuật**. Chúng nằm chung một file vì
cùng là nợ còn treo của phase 2 và cùng đụng `crates/session`, nên gom lại thì chỉ mất một lần
chạy gate, một lần review, một pull request.

- **Lỗi 1** — bước 1 và bước 2, chỉ đụng `bad_group_count` trong `crates/session`.
- **Lỗi 2** — bước 3 và bước 4, đụng `crates/dict/build.rs` và hai chỗ gọi `ADMIN`.

Hai nhóm bước **không phụ thuộc nhau**, không sửa chung một dòng nào. Nếu owner muốn hai pull
request riêng thì cắt ở ranh giới bước 2 / bước 3; phần *Chia việc* đã ghi sẵn ranh giới đó.
Mặc định của kế hoạch này là **một pull request**.

## Bối cảnh

### Lỗi 1 — một group lồng nhau làm chết cả lượt kiểm `373=16`

`bad_group_count` (`crates/session/src/lib.rs:4309`) là chỗ duy nhất engine trả
`SessionRejectReason 16` — *Incorrect NumInGroup count for repeating group*. Nó trả `Option` và
dùng `?` hai lần:

```rust
let (counter, _) = view.field_at(i)?;          // lib.rs:4314
...
let group = view.group::<D>(msg_type, counter)?; // lib.rs:4318
```

Trong một hàm trả `Option`, `?` trên `None` nghĩa là **thoát cả hàm với `None`**, mà `None` ở đây
có nghĩa "không có lỗi nào". Nên chỉ cần **một** counter mà view không dựng được group là **mọi
counter đứng sau nó không bao giờ được kiểm**.

Và điều đó xảy ra thường xuyên, không phải hiếm: `MessageView::group` là API **top-level**, theo
đúng rustdoc của nó (`crates/codec/src/group.rs:165-176`) — *"asking a TradeCaptureReport for
`NoAllocs(78)` — which exists only inside `NoSides(552)` — gives `None`"*. Hàm `open`
(`group.rs:191`) với `top_level = true` bước **qua** mọi vùng group trên đường đi, nên một counter
lồng bên trong không bao giờ được tìm thấy. Nhưng vòng lặp của `bad_group_count` lại duyệt **mọi
field** bằng `view.field_at(i)`, và `D::group_delimiter(msg_type, counter)` trả `Some` cho counter
lồng y như cho counter top-level (bảng phẳng, khoá theo `(msg_type, counter)`, không biết gì về độ
lồng). Vậy vòng lặp **đi vào** nhánh đó rồi nhận `None` và chết.

`STATUS.md` đã đo: **12 trong 45 counter dưới 5000 của message type `AE` là counter lồng**. Nghĩa
là một `TradeCaptureReport` thật hôm nay **không được kiểm `373=16` gì cả** kể từ counter lồng đầu
tiên trở đi.

Đây là lỗi có sẵn từ trước, không phải do PR B gây ra, và đã được ghi ở `STATUS.md` là "cần một
hàng riêng". Đây là hàng đó.

### Lỗi 2 — hai câu trả lời khác nhau cho "message này có phải admin không"

`const ADMIN: [&[u8]; 7]` ở `crates/session/src/lib.rs:291` là một **danh sách viết tay cạnh chỗ
gọi** — đúng thứ `DESIGN.md` D3 cấm. Nó có bảy phần tử `0 1 2 3 4 5 A` và **thiếu `n`**
(XMLnonFIX).

Cùng lúc đó, `is_transport_message` (sinh ra ở `crates/dict/build.rs:360`, đọc thẳng từ
`<messages>` của `FIXT11.xml`) kể **tám** message type, có cả `n`.

Hệ quả cụ thể, với một message `35=n` trên phiên FIXT:

- `Fixt11Fix50Sp2Tables::is_defined_tag_for` (`crates/dict/src/lib.rs:144`) coi nó là message của
  transport → chỉ cho phép 71 tag của `FIXT11.xml`;
- nhưng `out_of_family_appl_ver_id` (`lib.rs:4296`) hỏi `ADMIN.contains(&msg_type)`, không thấy
  `n`, nên **vẫn áp luật `1128` vốn chỉ dành cho application message** (ADR-0080 decision 3).

Một message, hai câu trả lời trái ngược trong **cùng một lượt validate**.

Kế hoạch phase 2 (`docs/plans/2026-09-19-phase-2-fixt-and-sbe.md:535`) đã hứa sinh `is_admin` từ
thuộc tính `msgcat` của XML. ADR-0084 *Decision 1* ghi lại rằng lời hứa đó **chưa bao giờ được
xây**: bảng sinh ra không export `is_admin`, `build.rs` không đọc `msgcat` lần nào.

## Những gì đã biết chắc

**Code trong repo — đã đọc, kèm file:dòng**

- `bad_group_count` ở `crates/session/src/lib.rs:4309-4324`; hai chỗ `?` ở dòng `4314` và `4318`.
- **Hình mẫu đúng đã có sẵn ngay bên dưới**: `in_a_group_before` (`lib.rs:4364`) và `in_a_group`
  (`lib.rs:4395`) cùng duyệt `view.field_at(i)` nhưng dùng
  `let Some((counter, _)) = view.field_at(i) else { continue; };`. Hai hàm đó **không** bỏ sót
  field nào. `bad_group_count` là hàm duy nhất trong ba hàm dùng sai `?`.
- `MessageView::group` chỉ tìm group **top-level**: `crates/codec/src/group.rs:179-186` gọi
  `open::<D, N>(..., top_level = true)`, và rustdoc `group.rs:165-176` nói thẳng `None` cho counter
  lồng. Đường xuống group lồng là `GroupEntry::group` (`group.rs:148`), gọi `open` với
  `top_level = false` và giới hạn `[entry.start, entry.end)` — **đã có sẵn, không phải viết mới**.
- `GroupIter` cho `declared()` và `counted()`; `size_hint` (`group.rs:125`) nói số entry được tính
  ngay lúc mở group, nên `counted()` là **miễn phí**, không duyệt lại.
- Hai — và chỉ hai — chỗ gọi `ADMIN`: `lib.rs:3407` (`let is_application = !ADMIN.contains(&msg_type);`)
  và `lib.rs:4296` (trong `out_of_family_appl_ver_id`). Kiểm bằng
  `grep -rn "ADMIN" crates/ tools/`, kết quả còn lại đều là `CAP_NET_ADMIN` trong `tools/w2w`.
- `is_application` được dùng đúng **một** chỗ: `lib.rs:3834`, cổng vào `app.on_message` và vào
  journal để resend. Vậy chỗ gọi này là **luật định tuyến**, không phải luật validate.
- `is_transport_message` sinh ở `crates/dict/build.rs:353-370` từ `transport_msg_types`
  (`build.rs:270-278`), lấy từ `<messages>` của `FIXT11.xml`. `build.rs:262` cho thấy danh sách
  `messages` mà `emit` nhận là FIXT11 nối với FIX50SP2 — nên một lượt đọc `msgcat` trong `emit`
  phủ được **cả hai bảng** (`Fix44` và cặp FIXT).
- `msgcat` trong XML: `docs/reference/fixt-dictionary-traps.md:22` — FIXT11 có **8** `<message>`,
  *all `msgcat='admin'`*; FIX50SP2 có 156, **không cái nào** admin.
  `docs/reference/fix44-dictionary-traps.md:100` trích nguyên văn
  `<message name='XMLnonFIX' msgtype='n' msgcat='admin' />`. Vậy **cả hai file XML đều nói `n` là
  admin**.
- `crates/dict/tests/fixt.rs:242-247` đã ghi sẵn nhận xét này thành comment và assert
  `is_transport_message(b"n")`.
- `crates/session/benches/alloc.rs:1007` đã có case `validate TradeCaptureReport (33 groups)` và
  fixture của nó (`alloc.rs:882-930`) — **đúng message có group lồng**, nên bước 2 không phải dựng
  case allocation mới, chỉ phải giữ nó ở 0.
- `vendor/` **rỗng trên máy này** (chưa `scripts/fetch-quickfix-assets.sh`), nên mọi khẳng định về
  XML ở trên đọc từ `docs/reference/`, không đọc trực tiếp XML. Bước 3 phải chạy fetch trước.

**Đặc tả và engine khác — tra trên internet, có URL**

- `SessionRejectReason(373)` giá trị **16** = *"Incorrect NumInGroup count for repeating group"*,
  FIX 4.4 dictionary: <https://www.onixs.biz/fix-dictionary/4.4/tagnum_373.html>. Đặc tả **không
  hề giới hạn luật này ở group top-level** — nó nói về "a repeating group", và mọi `NumInGroup`
  đều là một. Tra thêm ở B2BITS knowledge base cũng chỉ mô tả cùng một câu, không có ngoại lệ cho
  group lồng: <https://kb.b2bits.com/display/B2BITS/Explanation+of+log+messages+about+validation+and+parsing+errors>.
  **Tìm không thấy** một câu đặc tả nào miễn trừ group lồng khỏi `373=16`.
- **QuickFIX kiểm đếm ở mọi cấp lồng.** QuickFIX/n `DataDictionary.cs`: `Iterate` gọi
  `IterateGroup` cho từng instance của group top-level, và `IterateGroup` tự gọi lại chính nó cho
  từng group lồng bên trong; cả hai đều gọi `CheckGroupCount`, ném
  `RepeatingGroupCountMismatch(field.Tag)` khi
  `map.GetInt(field.Tag) != map.GroupCount(field.Tag)`.
  <https://github.com/connamara/quickfixn/blob/master/QuickFIXn/DataDictionary/DataDictionary.cs>.
  QuickFIX/J có cùng cặp `iterate()` / `checkGroupCount()`.
  Vậy **"đếm cả group lồng" là hành vi của cả hai engine tham chiếu**, không phải sáng kiến riêng.
- **`msgcat` của XML nói `n` là admin, nhưng ba engine QuickFIX không thống nhất.** Đây là phát
  hiện quan trọng nhất của lượt tra cứu này:
  - QuickFIX **C++** `Message.h:295-300`:
    `return strchr("0A12345", msgType.getValue().c_str()[0]) != 0;` — **bảy** type, **không có
    `n`**. <https://github.com/quickfix/quickfix/blob/master/src/C%2B%2B/Message.h>
  - QuickFIX/**J** `MessageUtils.java:121-123`:
    `return msgType.length() == 1 && "0A12345".contains(msgType);` — cũng **bảy**, không có `n`.
    <https://github.com/quickfix-j/quickfixj/blob/master/quickfixj-base/src/main/java/quickfix/MessageUtils.java>
  - QuickFIX/**n** `Message.cs:70-73`:
    `return msgType.Length == 1 && "0A12345n".Contains(msgType[0]);` — **tám**, **có `n`**, và
    release notes của nó ghi đây là một bugfix (đổi `'h'` thành `'n'`).
    <https://github.com/connamara/quickfixn/blob/master/QuickFIXn/Message/Message.cs>
  - Cả ba engine đều định nghĩa `isApp()` là phủ định của `isAdminMsgType()` — một câu hỏi, hai
    tên. Chính chỗ đó là nơi kế hoạch này **cố tình đi khác**, xem *Cách làm*.

**Ràng buộc từ repo**

- `CLAUDE.md` §2.1 — không allocation trên đường parse/validate, chứng bằng `benches/alloc.rs`.
- `CLAUDE.md` §2.2 — session layer thuần: không socket, không clock, không allocation, không
  `format!`.
- `CLAUDE.md` §2.3 — 59 acceptance definitions là cổng của session layer.
- `CLAUDE.md` §2.5 — thứ tự field đến từ bảng sinh, **không bao giờ từ chỗ gọi**. `ADMIN` vi phạm
  tinh thần điều này.
- `CLAUDE.md` §2.7 — không `panic!`/`unwrap()`/`expect()` trong crate thư viện.
- ADR-0085 *Decision 2 và 4*: `bad_group_count` (`373=16`) chạy **trước** `scan_group_members`
  (`373=5`/`373=6`), và thứ tự đó là thứ tự counterparty quan sát được. Đổi tập counter được kiểm
  ở `373=16` **có thể đổi mã lỗi** mà một message nhận được — đó là rủi ro chính của lỗi 1.
- ADR-0084 *Decision 1* đã ghi rõ: quyết định `is_defined_tag_for` **cố tình không** rẽ theo
  `ADMIN` của session, vì "a call-site list is exactly what D3 forbids, and it disagrees with the
  transport file on `n`".

## Cách làm

### Lỗi 1 — counter lồng **được đếm thật**, không bỏ qua

Hai quyết định, theo đúng thứ tự.

**(a) Ngừng làm chết cả lượt kiểm.** `bad_group_count` đổi hai chỗ `?` thành `else { continue; }`,
đúng hình mẫu `in_a_group_before` ngay bên dưới nó. Một counter mà view không dựng được group
không còn là "không có lỗi nào"; nó chỉ là một field mà vòng lặp đi qua.

**(b) Counter lồng được đếm thật, bằng đường xuống có sẵn trong `codec`.** Không chọn phương án
"bỏ qua rồi ghi vào tài liệu". Lý do: đặc tả không miễn trừ group lồng, cả QuickFIX/n lẫn
QuickFIX/J đều kiểm ở mọi cấp, và một lỗ 12/45 counter trên `AE` là **hầu hết công việc thật của
một sàn** chứ không phải một góc hiếm.

Cách đếm, **không cấp phát một byte nào**:

```rust
/// Độ lồng tối đa mà lượt kiểm chịu đi xuống.
const MAX_GROUP_NESTING: usize = 8;

fn bad_group_count<D: Tables, const N: usize>(...) -> Option<(SessionText, Option<Held<12>>)> {
    for i in 0..view.len() {
        let Some((counter, _)) = view.field_at(i) else { continue; };
        if D::group_delimiter(msg_type, counter).is_none() { continue; }
        let Some(group) = view.group::<D>(msg_type, counter) else { continue; };
        if group.declared() != Some(group.counted()) {
            return Some((SessionText::IncorrectNumInGroupCount, tag_text(counter)));
        }
        for entry in group {
            if let Some(f) = bad_nested_count::<D, N>(&entry, msg_type, counter, 1) {
                return Some(f);
            }
        }
    }
    None
}
```

`bad_nested_count` nhận một `GroupEntry`, duyệt `D::group_members(msg_type, parent)`, và với mỗi
member `m` mà `D::group_delimiter(msg_type, m).is_some()` thì gọi `entry.group::<D>(msg_type, m)`
— API đã có ở `group.rs:148` — so `declared()` với `counted()`, rồi gọi lại chính nó cho từng
entry con với `depth + 1`. Đạt `MAX_GROUP_NESTING` thì dừng và trả `None`.

Vì sao cách này thoả `CLAUDE.md` §2.1 và §2.2:

- `GroupIter` và `GroupEntry` là view mượn vào buffer của caller, đúng quy ước §6 *Public API takes
  borrowed views*. Không có `Vec`, không có owned struct.
- Đệ quy là **stack**, không phải heap; `D` và `N` cố định nên monomorphise đúng một bản.
  `MAX_GROUP_NESTING` chặn cả tràn stack lẫn một bảng từ điển bệnh hoạn mà member set chứa chính
  counter của nó.
- Không `format!`, không clock, không socket. Fault trả về vẫn là `SessionText` fieldless +
  `Held<12>` như cũ.
- Case `validate TradeCaptureReport (33 groups)` trong `crates/session/benches/alloc.rs` là bằng
  chứng, không phải lời hứa — nó đọc đúng message có group lồng và phải vẫn bằng **0**.

**Thứ tự báo lỗi, quyết định rõ ràng:** depth-first, ngay sau khi counter cha được kiểm. Đó là
**thứ tự trên dây**, vì một group lồng nằm giữa counter cha và field top-level kế tiếp. Nếu cả
counter cha lẫn counter con đều sai thì counterparty nhận `371=` của **counter cha**.

**`MAX_GROUP_NESTING = 8` không phải số bịa đặt tuỳ tiện, nhưng cũng chưa được đo.** Bước 2 thêm
một test trong `crates/dict/tests/group_tables.rs` gấp `GROUP_KEYS` lại để tính **độ lồng thật lớn
nhất** của cả hai bảng, in ra, và assert nó `<= MAX_GROUP_NESTING`. Ngày một từ điển vượt qua,
test nói ngay, đúng khuôn ADR-0085 decision 3 đã dựng cho `SEEN = 32`.

### Lỗi 2 — **hai câu hỏi, hai cái tên**, và cả hai đều nói ra tại sao

Đây là chỗ có rủi ro thiết kế thật, và kế hoạch này chọn **không** gộp hai chỗ gọi vào một hàm.

**`is_admin` được sinh ra, trong `crates/dict/build.rs`, từ `msgcat`.** Thêm vào `emit` (hàm dựng
bảng cho cả `Fix44` lẫn cặp FIXT, `build.rs:775` trở đi) một lượt đọc thuộc tính `msgcat` của từng
`<message>`, và emit:

```rust
/// Whether the dictionary files this table is built from call this message
/// administrative — `msgcat='admin'`, read from the XML and never from a list
/// beside a call site (DESIGN.md D3).
pub fn is_admin(msg_type: &[u8]) -> bool
```

`<message>` thiếu `msgcat` thì `die(...)`, y như `build.rs` đang làm với `name` và `number` thiếu
(`build.rs:392`). Không đoán mặc định.

`Tables` (`crates/dict/src/tables.rs`) mọc thêm `fn is_admin(msg_type: &[u8]) -> bool;`, **không có
default method** — đúng lập luận ADR-0084 decision 1 đã dùng cho `is_defined_tag_for`: một default
sẽ lặng lẽ trao ngữ nghĩa FIX 4.4 cho bảng thứ ba nào đó sau này.

**Hai chỗ gọi `ADMIN` đi hai đường khác nhau, và đó là quyết định, không phải sơ suất:**

| Chỗ gọi | Câu hỏi thật sự là gì | Sau khi sửa | `35=n` ra sao |
|---|---|---|---|
| `lib.rs:4296` `out_of_family_appl_ver_id` | "message này thuộc tầng session/transport, nên luật `1128` của ADR-0080 không áp dụng?" — một câu hỏi về **từ điển** | `D::is_admin(msg_type)` | **Đổi hành vi**: `n` nay được miễn luật `1128`, khớp với `is_defined_tag_for` vốn đã coi nó là transport. Lỗi 2 đóng ở đây. |
| `lib.rs:3407` `is_application` | "session layer này có tự trả lời message đó không, hay giao cho application và lưu để resend?" — một câu hỏi về **định tuyến của engine này** | `SESSION_OWNED.contains(&msg_type)`, **vẫn bảy phần tử**, chỉ đổi tên và viết lại rustdoc | **Không đổi hành vi**: `n` vẫn đi tới `app.on_message` và vẫn được lưu để resend. |

Vì sao chỗ gọi thứ hai **không** dùng `is_admin`:

- `35=n` XMLnonFIX không có nội dung session nào để session layer trả lời. Session layer không biết
  làm gì với nó; giao cho application là hành vi duy nhất đúng.
- QuickFIX **C++** và QuickFIX/**J** đều trả `isApp() == true` cho `35=n` (`"0A12345"`, không có
  `n`). Đổi chỗ này sẽ làm engine **lệch khỏi hai trong ba engine tham chiếu** ở một hành vi quan
  sát được trên dây (resend / gap fill), vì một message không còn được lưu là một message bị gap
  fill đè lên. Không có `.def` nào tập luyện `35=n`, nên đây là thay đổi **không gate nào nhìn
  thấy** — đúng loại thay đổi `CLAUDE.md` §10 bảo phải kiểm bằng tay và không được làm thầm.
- `ADMIN` bị **đổi tên** chứ không giữ nguyên, vì cái tên cũ chính là nguyên nhân: "admin" nghe như
  câu trả lời của từ điển, trong khi nó là danh sách của engine này. Tên mới `SESSION_OWNED` nói
  đúng nó là gì, và rustdoc của nó nêu cả ba engine cùng con số bảy.

**Một test ghim khoảng cách giữa hai danh sách**, trong `crates/session/src/lib.rs` (chỗ duy nhất
`SESSION_OWNED` nhìn thấy được): với mỗi `msg_type` mà `D::is_admin` trả `true`, hoặc nó nằm trong
`SESSION_OWNED`, hoặc nó đúng bằng `b"n"`. Ngày một từ điển thêm một message admin thứ chín, test
nói ngay thay vì để nó lặng lẽ rơi về phía application.

### Cần một ADR — ADR-0086

**Có.** Số trống kế tiếp là **ADR-0086**. Kế hoạch này **không** viết nó; architect viết ở bước 0.
Nó quyết định ba điều, và mỗi điều đều đắt hoặc khó đảo:

1. `373=16` được hỏi ở **mọi cấp lồng**, đi xuống bằng `GroupEntry::group`, chặn ở
   `MAX_GROUP_NESTING`, thứ tự báo lỗi là depth-first tức thứ tự trên dây. Hệ quả xấu phải ghi:
   `validate` làm thêm việc trên mọi message có group, và **chưa ai đo** thêm bao nhiêu.
2. Câu hỏi "admin?" **tách làm hai**: `Tables::is_admin` sinh từ `msgcat` cho luật validate, và
   `SESSION_OWNED` bảy phần tử cho luật định tuyến. Đây là điều **contested**: nó vừa nhận
   `msgcat` là nguồn sự thật (như ADR-0084 decision 1 đòi), vừa cố tình **không** áp nó vào định
   tuyến, nơi engine này đứng cùng QuickFIX C++/J và khác QuickFIX/n.
3. `35=n` từ nay được miễn luật `1128` của ADR-0080 decision 3. Đây là **sửa đổi phạm vi** một
   quyết định đã Accepted, nên phải là ADR mới chứ không phải sửa ADR-0080 tại chỗ (`CLAUDE.md`
   §5).

## Bất biến bị đụng tới

| Bất biến §2 | Việc này đụng ra sao | Giữ bằng gì |
|---|---|---|
| **1 — không allocation trên hot path** | `bad_group_count` nằm trên đường validate và nay làm thêm việc | `crates/session/benches/alloc.rs` case `validate TradeCaptureReport (33 groups)` (`alloc.rs:1007`) phải đọc **0**. Đây đúng là message có group lồng, nên case đã có sẵn là bằng chứng thật. |
| **2 — session layer thuần** | Thêm đệ quy và một const mới trong `crates/session` | Không clock, không socket, không `format!`, không allocation. Fault vẫn là `SessionText` fieldless + `Held<12>`. Đệ quy là stack, chặn bởi `MAX_GROUP_NESTING`. |
| **3 — 59 acceptance definitions là cổng** | Đổi tập counter được kiểm `373=16`, và `14i_RepeatingGroupCountNotEqual.def` là định nghĩa duy nhất populate một repeating group | `cargo test -p fixbolt-session --test score` phải in **59 / 59**, và `--test wire` cũng vậy. Một con số **khác 59, kể cả cao hơn**, là dừng lại. |
| **5 — thứ tự field từ bảng sinh, không từ chỗ gọi** | Đây chính là điều `ADMIN` đang vi phạm | `is_admin` sinh từ `msgcat`; `SESSION_OWNED` còn lại là danh sách của engine, có rustdoc nói rõ nó **không** là câu trả lời của từ điển, và có test ghim khoảng cách. |
| **7 — không `panic!`/`unwrap()`/`expect()`** | Code mới trong hai crate thư viện | Mọi `Option` xử lý bằng `let ... else`; không index bằng `[]`; `cargo clippy --all-targets -- -D warnings` im lặng. `die()` trong `build.rs` là build script, không phải crate thư viện — cùng khuôn với `build.rs:392` sẵn có. |

## Chia việc

Mọi bước đụng `crates/session` hoặc `crates/dict` → **senior developer (opus)**, theo `CLAUDE.md`
§12. Không bước nào commit; manager chạy lại gate và commit.

| Bước | Ai | Kết quả | Test viết trước, và câu FAIL chờ đợi | Gate đóng bước | Phụ thuộc |
|---|---|---|---|---|---|
| **0** | **architect (fable), nền** | `docs/decisions/ADR-0086-*.md`, ba quyết định ở mục trên, kèm *Consequences* cả tốt lẫn xấu. Không đụng `crates/` | — | `python3 scripts/check-links.py` sạch | — |
| **1** | **senior dev (opus)** | `crates/session/src/lib.rs` `bad_group_count`: hai `?` (`lib.rs:4314`, `4318`) → `let ... else { continue; }`. **Chỉ vậy** — chưa đi xuống group lồng | `crates/session/tests/group_member_values.rs::a_counter_after_a_nested_group_is_still_checked` — một `AE` có một group lồng đứng **trước** một counter top-level khai sai. Viết trước, chạy trước khi sửa, FAIL: `expected Reject 373=16, engine sent no reject` | `cargo test -p fixbolt-session --test group_member_values` và `--test score` in `59 / 59` | 0 |
| **2** | **senior dev (opus)** | `crates/session/src/lib.rs`: `MAX_GROUP_NESTING`, `bad_nested_count`, vòng `for entry in group` trong `bad_group_count`. `crates/dict/tests/group_tables.rs`: test gấp `GROUP_KEYS` đo độ lồng thật của cả hai bảng và assert `<= MAX_GROUP_NESTING`, **in con số ra** | `group_member_values.rs::a_nested_counter_that_lies_is_rejected` (group lồng khai 3 gửi 2 → `373=16` với `371=` của counter **lồng**) và `a_parent_counter_is_named_before_its_child` (cả hai sai → `371=` của counter **cha**). FAIL chờ đợi cho cái đầu: `expected Reject 373=16, engine sent no reject`. `group_tables.rs::the_generated_tables_never_nest_deeper_than_the_walk_goes` | `cargo test -p fixbolt-session --test group_member_values`, `cargo test -p fixbolt-dict --test group_tables`, `--test score` in `59 / 59`, và `scripts/bench.sh` cho case alloc `validate TradeCaptureReport (33 groups)` đọc **0** | 1 |
| **3** | **senior dev (opus)** | `crates/dict/build.rs`: đọc `msgcat` trong `emit`, `die` nếu thiếu, emit `pub fn is_admin`. `crates/dict/src/tables.rs`: `Tables::is_admin`, **không default**. `crates/dict/src/lib.rs`: impl cho `Fix44` và `Fixt11Fix50Sp2Tables` | `crates/dict/tests/fixt.rs::the_transport_files_admin_set_is_the_tables_admin_set` — `is_admin(b"n")`, `is_admin(b"A")`, `!is_admin(b"D")`, và với FIXT thì `is_admin == is_transport_message` cho cả 8. `crates/dict/tests/tables.rs::fix44_calls_xmlnonfix_admin`. FAIL chờ đợi: `error[E0599]: no function or associated item named 'is_admin' found` | `cargo test -p fixbolt-dict`, `cargo test -p fixbolt-dict --features fix50sp2`, và `cargo test --no-default-features -p fixbolt-dict` | 0. **Ranh giới cắt PR nếu owner muốn hai PR** |
| **4** | **senior dev (opus)** | `crates/session/src/lib.rs`: `ADMIN` → `SESSION_OWNED` (bảy phần tử, rustdoc nêu C++/J/n và con số bảy); `lib.rs:3407` dùng `SESSION_OWNED`; `lib.rs:4296` dùng `D::is_admin`. `out_of_family_appl_ver_id` phải nhận được `D` — đọc chữ ký hiện tại trước khi sửa | `crates/session/tests/fixt.rs::an_xmlnonfix_message_is_not_asked_the_appl_ver_id_rule` (FAIL: `expected no reject, engine sent 373=5 371=1128`) và `::xmlnonfix_still_reaches_the_application` (ghim hành vi định tuyến **không** đổi). Unit test trong `lib.rs`: `every_admin_type_is_session_owned_except_xmlnonfix` | `cargo test -p fixbolt-session`, `--features fix50sp2 --test score_fixt` in `179 / 180`, `--test score` in `59 / 59` | 2, 3 |
| **5** | **developer (sonnet)** | Chỉ tài liệu, theo danh sách *Tài liệu phải cập nhật*. Không đụng `crates/` | — | `python3 scripts/check-links.py` sạch | 4 |
| **6** | **manager** | Chạy lại đủ bộ gate ở *Cách kiểm chứng*, làm hai reversal, commit từng bước xanh, mở PR nháp từ commit đầu, đóng bằng CI run id | — | tất cả | 5 |

## Cách kiểm chứng

Bộ gate đầy đủ, chạy trên Mac (không bước nào cần máy `DESIGN.md` §9):

```
cargo test --all
cargo test --no-default-features
cargo test -p fixbolt-session --test score                                   # 59 / 59
cargo test -p fixbolt-engine --test wire                                     # 59 / 59
cargo test -p fixbolt-session --features fix50sp2 --test score_fixt          # 179 / 180
cargo test -p fixbolt-engine --features fix50sp2 --test wire_fixt            # 60 / 60
scripts/bench.sh                                                             # alloc.rs, mọi case 0
cargo fmt --check && cargo clippy --all-targets -- -D warnings
```

`vendor/` phải fetch trước (`scripts/fetch-quickfix-assets.sh`), nếu không `cargo test --all`
không compile — và bước 3 đọc thẳng `msgcat` từ XML nên không có `vendor/` là không làm được gì.

**"Test pass" một mình chưa đủ.** Hai reversal, câu FAIL viết ra **trước** khi chạy:

1. **Reversal lỗi 1** — đưa `?` ở `lib.rs:4318` trở lại. Chờ:
   `a_counter_after_a_nested_group_is_still_checked` và `a_nested_counter_that_lies_is_rejected`
   cùng đỏ với `expected Reject 373=16, engine sent no reject`; `--test score` **vẫn 59 / 59**, vì
   không định nghĩa nào trong 59 cái mang group lồng. Chính sự im lặng đó là lý do lỗi này sống sót
   qua phase 2, và nó phải được dán vào nhật ký giao hàng.
2. **Reversal lỗi 2** — đưa `SESSION_OWNED.contains(&msg_type)` trở lại `lib.rs:4296` thay cho
   `D::is_admin`. Chờ: `an_xmlnonfix_message_is_not_asked_the_appl_ver_id_rule` đỏ với
   `expected no reject, engine sent 373=5 371=1128`.

**Kiểm bằng bytes thật, không chỉ test đơn vị**: bước 2 và bước 4 mỗi bước dán vào nhật ký bytes
của message gửi đi và Reject engine trả về, đọc bằng mắt, không chỉ dán `test result: ok`.

## Tài liệu phải cập nhật

Đi từng hàng bảng đồng bộ `CLAUDE.md` §4.

- [ ] `docs/SESSION-BEHAVIOUR.md` — mục `373=16`: nay hỏi ở **mọi cấp lồng**, thứ tự là depth-first
      tức thứ tự trên dây, trần `MAX_GROUP_NESTING`; nêu tên `14i_RepeatingGroupCountNotEqual.def`
      và ba test mới. Thêm một dòng cho `35=n`: admin với từ điển, application với bộ định tuyến.
      *(hàng "Session boundary behaviour")*
- [ ] `docs/DESIGN.md` §4 D16 — danh sách method của `Tables` mọc thêm `is_admin`.
      *(hàng "Codec, session, dispatch … behaviour" và hàng "public API")*
- [ ] `crates/dict/src/tables.rs` rustdoc của `Tables::is_admin`, cùng commit với code.
- [ ] `CHANGELOG.md` — public API của `fixbolt-dict` mọc thêm một method; hành vi `373=16` mở rộng;
      `35=n` không còn bị hỏi luật `1128`.
- [ ] `docs/decisions/ADR-0086-*.md` — bước 0.
- [ ] `docs/reference/<tên mới>.md` — **bắt buộc theo §4 "nếu nó tốn của bạn thì ghi lại"**: hai
      trap ở đây đều đã tốn thời gian. (a) `MessageView::group` là API top-level và một `?` trên nó
      biến "không dựng được" thành "không có lỗi"; (b) ba engine QuickFIX có **ba** tập admin khác
      nhau và `msgcat` của XML khác cả C++ lẫn J. Kèm URL đã trích ở *Những gì đã biết chắc*.
- [ ] `STATUS.md` — gạch ngang hai bullet: *"`bad_group_count` abandons the whole `373=16` pass on
      a nested group, silently"* và *"Two answers to 'is this a session message?' in one validate
      pass"*; và bullet *"`is_admin` was never built"* ở trên chúng. Trỏ commit và CI run id.
- [ ] `docs/plans/2026-09-19-phase-2-fixt-and-sbe.md` — hàng traps `is_admin` nay có nơi đóng, trỏ
      sang kế hoạch này. Không sửa lại nội dung cũ, chỉ thêm một dòng trỏ đi.

**Các hàng §4 cố tình KHÔNG áp dụng, và vì sao:**

- `docs/CONFORMANCE.md` — **không** đổi: 59 / 59 và 179 / 180 phải giữ nguyên. Nếu một con số
  **có** đổi thì đó là dừng lại, không phải sửa tài liệu.
- `docs/CONFIGURATION.md` — không thêm hằng số hay key nào người dùng đặt được;
  `MAX_GROUP_NESTING` là nội bộ, ghim bằng test chứ không phải bằng cấu hình.
- `DESIGN.md` §6, §8, §9, `docs/hft-playbook.md`, `docs/best-practices-*.md` — không có số hiệu
  năng mới, không có hàng nào của latency budget dịch chuyển, không có dòng OS nào đổi.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Sửa `?` thành `continue` nhưng **chỉ ở một trong hai dòng** (`4314` hoặc `4318`), lỗ vẫn còn | `a_counter_after_a_nested_group_is_still_checked`, cộng với mắt người đọc diff ở bước 6 — diff phải cho thấy **hai** dòng đổi |
| Đếm counter lồng **hai lần**: vòng top-level cũng vào nó rồi lại đếm khi đi xuống | `view.group` với `top_level = true` bước qua vùng group (`group.rs:191-215`), nên `else { continue; }` là nhánh nó đi. Ghim bằng `a_nested_counter_that_lies_is_rejected`: đúng **một** Reject, và `371=` là của counter lồng |
| Đệ quy vô tận vì một bảng có member set chứa chính counter của nó | `MAX_GROUP_NESTING` chặn, và `group_tables.rs::the_generated_tables_never_nest_deeper_than_the_walk_goes` đo độ lồng thật và in ra |
| Thứ tự báo lỗi đổi ngầm: một message trước đây nhận `373=5` nay nhận `373=16` | `--test score` **59 / 59**, `--test score_fixt` **179 / 180**, và toàn bộ `group_member_values.rs` **xanh không sửa một fixture nào**. Một fixture bị sửa để bài mới pass là chế độ hỏng §10 cảnh báo |
| Con số 59 vẫn xanh nên tưởng đã kiểm xong lỗi 1 | Reversal 1 bắt buộc, và kết quả "59 / 59 dù đã đưa lỗi trở lại" phải được dán vào nhật ký như một phát hiện, không giấu đi |
| `bad_nested_count` cấp phát, hoặc `for entry in group` copy entry ra owned struct | `scripts/bench.sh`, case `validate TradeCaptureReport (33 groups)` đọc **0**. §2.1 nói chứng bằng allocator, không bằng đọc code |
| `build.rs` đọc `msgcat` bằng regex và trượt ở `<message ... />` tự đóng — đúng trap 4 của `docs/reference/fix44-dictionary-traps.md`, và `XMLnonFIX` **chính là** cái tự đóng đó | `build.rs` đã dùng `roxmltree`, không dùng regex; giữ nguyên. Ghim bằng `fix44_calls_xmlnonfix_admin` — nếu parser trượt đúng cái tự đóng thì test này đỏ |
| `<message>` thiếu `msgcat` bị đoán mặc định thành `app` | `die()` trong `build.rs`, cùng khuôn `build.rs:392`. Build đỏ, không đoán |
| Đổi luôn `lib.rs:3407` sang `is_admin` cho "nhất quán", làm `35=n` không còn tới application và không còn được lưu để resend — **không gate nào thấy** | `xmlnonfix_still_reaches_the_application`, viết chính vì bẫy này. Cộng hàng kiểm tay §10 ở bước 6 |
| Một từ điển tương lai thêm message admin thứ chín, nó lặng lẽ rơi về phía application | `every_admin_type_is_session_owned_except_xmlnonfix` |
| Test viết sau code rồi bảo là viết trước | Bước 1–4 mỗi bước dán output **đỏ** trước khi dán output xanh, đúng `CLAUDE.md` §10 |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| `validate` chậm đi vì đi xuống group lồng, mà **không có máy §9 để đo** | **Trung bình, và không đóng được trong PR này** | Ghi thẳng vào *Ngoài phạm vi* và vào `STATUS.md` phần *Not proven*. Số đối chiếu duy nhất đang có — `validate TradeCaptureReport (33 groups)` **47 793,4 ns/op** — là từ một Xeon chưa tune, `NO BASELINE`, nên **không publish được** và chỉ dùng được dạng tỉ lệ (`STATUS.md`). Cùng một ràng buộc, cùng một máy, cùng một open item với ADR-0085 *Alternatives* D |
| Một counterparty đang gửi group lồng sai đếm **hôm nay được chấp nhận**, ngày mai bị `373=16` | Trung bình | Đây là hành vi đúng theo đặc tả và theo cả QuickFIX/n lẫn QuickFIX/J. Phải vào `CHANGELOG.md` như một thay đổi hành vi, không phải một dòng "fix" |
| `35=n` được miễn luật `1128` là sai với một sàn nào đó | Thấp | Không `.def` nào tập luyện `35=n`; quyết định dựa trên `msgcat` của chính XML và khớp với `is_defined_tag_for` sẵn có. ADR-0086 ghi lại để đảo được |
| Fable và Opus 5 cùng lấy số ADR-0086 trên hai nhánh | Thấp, nhưng đã có tiền lệ | `STATUS.md` vẫn ghi *"No guard against two branches taking the same ADR number"* là open. Manager kiểm `ls docs/decisions/` trên `main` ngay trước khi merge |

## Ngoài phạm vi

- **Không đo gì trên máy `DESIGN.md` §9.** Không có máy đó trong lần làm này, nên **ảnh hưởng thời
  gian của lượt đi xuống group lồng lên `validate` là hoàn toàn chưa đo** — không phải "nhỏ", không
  phải "không đáng kể", mà là **chưa biết**. Kế hoạch này không công bố con số nào, và
  `CLAUDE.md` §2.10 cấm nói ngược lại. Phải ghi vào `STATUS.md` *Not proven* cùng commit.
- **Không** đụng `ADR-0080` decision 3 tại chỗ; phạm vi của nó bị ADR-0086 thu hẹp, theo `CLAUDE.md`
  §5 supersede-only.
- **Không** đổi hành vi định tuyến của `35=n`. Đó là một quyết định riêng, cần ADR riêng và một
  `.def` hoặc một capture thật làm bằng, mà hiện không có.
- **Không** đụng `373=2` / `373=15` cho stray member hay mis-ordered member — ADR-0085 decision 4 đã
  từ chối và kế hoạch này không mở lại.
- **Không** thêm job CI mới; mọi test mới chạy sẵn trong `cargo test --all` và `scripts/bench.sh`.
- **Không** sửa `CLAUDE.md`, kể cả hàng §2 *Machine checks* số 3 và hàng §7 mà `STATUS.md` nói đang
  nói thiếu (59 nhưng thật ra là 59 **cộng** 179/180). Owner đã rào `CLAUDE.md`; nó vẫn nằm ở
  `STATUS.md`.

## Nhật ký giao hàng

| Bước | Commit | Bằng chứng manager tự chạy lại |
|---|---|---|
| 0 | `585e11c` | ADR-0086 viết xong; `check-links.py` `no dead internal links`. Architect phát hiện ADR-0080 vẫn là `Proposed` dù đã merge — kế hoạch ghi nhầm là Accepted. Không quyết định nào sai, đã ghi vào ADR-0086 |
| 1 | `c8709a1` | Đỏ trước: `expected Reject 373=16, engine sent no reject`. Reversal (đặt `?` lại): đúng câu đó, 16 passed 1 failed; khôi phục 17 passed. `--test score` 4 passed (assert 59/59). **Lệch kế hoạch**: fixture là `NewOrderSingle` với `802` lồng trong `453`, không phải `TradeCaptureReport` — file test đó toàn FIX 4.4 và `35=D`; có test tiền đề assert đúng là có lồng |
| 3 | `438228a` | Đỏ trước: `error[E0599]: no associated function … named 'is_admin'`. Ba dạng `cargo test -p fixbolt-dict` (mặc định, `--features fix50sp2`, `--no-default-features`) đều ok. Hàm sinh ra: `matches!(msg_type, b"0"|…|b"A"|b"n")`, không cấp phát. `FIX50SP2.xml` đánh `app` cho cả 156 message, `FIXT11.xml` đánh `admin` cho cả 8 — `is_admin` trùng `is_transport_message` hôm nay, test ghim sự trùng hợp đó |
| 2 | `8e81aae` | Đỏ trước: `expected Reject 373=16, engine sent no reject`. Hai reversal: nâng phần đi xuống lên trước phần kiểm cha → đỏ nêu `371=802` thay vì `371=453`; hạ trần xuống 1 → đỏ đúng câu ban đầu. Độ sâu do test in: FIX 4.4 4 tầng (`AB`/555), FIXT 7 tầng (`b`/296), trần 8. `group_member_values` 19 passed |
| 4 | `6e84ef1` | Đỏ trước: `expected no reject, engine sent 373=5 371=1128`. Reversal (đặt `SESSION_OWNED` lại vào chỗ luật 1128): đúng câu đó; khôi phục 11 passed. Bytes thật: `35=n` mang `1128=4` trước bị `Reject 373=5 371=1128`, sau không trả gì, link vẫn up |
| 5 | `caaf14e` | `check-links.py` sạch. Đo thêm trong lúc đóng bước: test socket `fix50sp2` **phụ thuộc tải và có sẵn trên `main`** — 0/40 chạy tuần tự trên nhánh này, 11/50 khi chạy 10 bản song song, **8/50 cùng cách trên `main` tại `64ea6c2`**. Dưới tải một timer bắn, engine phát `35=5` mà `.def` không hỏi, đẩy lệch comparator đúng một message. Đã đếm và ghi vào `docs/reference/`, **không sửa** |
| review | `9fdbeaa` | Senior review, context sạch, tìm ra gate không nhìn được quá tầng lồng thứ 2. **Manager tái hiện**: truyền `parent` thay `*member` ở bước đệ quy → **không test nào đỏ**. `a_counter_three_levels_down_that_lies_is_rejected` (`552 → 453 → 802`) bịt lỗ: đỏ `expected Reject 373=16 naming 802, engine sent no reject`, khôi phục 20 passed. Ba finding còn lại cũng đã kiểm chứng và sửa: ADR-0086 ghi "Nothing here is built" (thêm phụ lục có ngày), câu "không đo được" sai về công cụ (`benches/validate.rs` có case; A/B trên laptop ≈ +2,6 µs ≈ +8%, **không công bố được** theo §2 luật 10), và rustdoc mâu thuẫn với docs về vì sao chọn 8 |

**Đóng plan**: commit `9fdbeaa`, **CI run [`35484818878`](https://github.com/tmthang86/fixbolt/actions/runs/35484818878), 14 jobs of 14**.

**Chuyển cho architect, không sửa ở đây**: `view.group` quét lại từ index 0 cho mỗi nested
counter, nên lượt kiểm là O(số lần xuất hiện × số field). Đây là câu hỏi hình dạng, không phải lỗi.

**Không chứng minh được**: không có máy `DESIGN.md` §9, nên băng số thật của phần đi xuống vẫn nợ;
nhánh `die()` của `build.rs` khi thiếu `msgcat` chưa bao giờ chạy vì `vendor/` chỉ đọc.
