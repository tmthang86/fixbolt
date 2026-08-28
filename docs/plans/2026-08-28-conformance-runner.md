# Bộ chạy 59 định nghĩa acceptance

> **Loại:** Plan · **Ngày:** 2026-08-28 · **Trạng thái:** Xong
> **Phạm vi:** Phase 1 — cái cổng cho session layer, dựng **trước** session layer

## Bối cảnh

`DESIGN.md` §7 xếp thứ tự: **bộ chạy `.def` có trước session layer**. Lý do là một câu trong
`CLAUDE.md` §10 — *"một phép kiểm chưa chứng minh được gì cho tới khi có thứ gì đọc nó"*.
Viết session layer trước rồi mới viết cổng cho nó thì cổng sẽ được viết vừa vặn với thứ đã
làm, chứ không phải với thứ phải làm.

Bộ 59 định nghĩa là **cổng chính** của phase 1 (`PRD.md` §2, tiêu chí 1). Nó không có thư
viện nào chạy hộ: QuickFIX chạy chúng bằng `Reflector.rb` qua socket thật. Ở đây phải chạy
**trong tiến trình, không socket** — vì bất biến 2 nói session layer thuần, và một cổng đi
qua socket sẽ không đo được cái mình muốn đo.

Việc này **không viết một dòng nào của session layer**. Kết thúc plan này, bộ chạy tồn tại,
chạy được cả 59 file, và báo **0/59** — vì chưa có gì trả lời. Đó là kết quả đúng.

## Những gì đã biết chắc

Tất cả đo trên `vendor/quickfix/test/definitions/server/fix44/`, ngày 2026-08-28.

| | |
|---|---|
| File | **59** |
| Dòng `I` (gửi vào) | **289** |
| Dòng `E` (mong nhận) | **250** |
| Dòng `i` (hành động vào) | **66** — `iCONNECT` 61, `i1,CONNECT` 2, `i2,CONNECT` 2, `i1,DISCONNECT` 1 |
| Dòng `e` (hành động mong đợi) | **64** — `eDISCONNECT` 61, `e2,DISCONNECT` 2, `e1,DISCONNECT` 1 |
| File dùng nhiều session | **2** — `1b_DuplicateIdentity.def`, `AlreadyLoggedOn.def` |

Đã có sẵn và dùng lại được, không viết lại:

- `crates/codec/tests/common/mod.rs` — bộ nạp 5 bước: bỏ chỉ thị, bỏ tiền tố `N,`, thay
  `<TIME>` và `<TIME±N>`, `fixify` (chèn `9=`, nối `10=`), phân loại. Đã chạy trên 539 dòng.
- `docs/reference/quickfix-acceptance-def-format.md` — 7 chỉ thị, quy tắc so sánh **theo vị
  trí**, các tag 10/42/52/60/122 so bằng regex.
- `[measured]` **đo lại 2026-08-28, sửa hai con số của chính plan này.** Bản đầu viết "250
  dòng mang `9=`" — 250 là số dòng `E`, không phải số dòng mang `9=`. Số đúng:

  | | Toàn bộ I+E | Riêng E |
  |---|---|---|
  | Mang `9=` | **255** | **247** |
  | Mang `10=` | **251** | **244** |

  Trong đó **6 dòng có `9=` sai** (cố ý, `defs.rs::BAD_BODY_LENGTH` liệt kê cả sáu), và
  trong 246 dòng mang `10=` đi tới được phép so (251 trừ 5 dòng bị từ chối), **0 dòng có
  `10=` thật** — 238 dòng ghi thẳng `10=0`. Đã có trong
  [quickfix-acceptance-def-format.md](../reference/quickfix-acceptance-def-format.md) và
  `crates/codec/tests/defs.rs`.

### Ba sự thật quyết định hình dạng của bộ chạy

**1. Server trong bộ này là một echo server, và nó sắp lại thứ tự.** Dòng `E` mang
`35=D` **42 lần**. Trong `15_HeaderAndBodyFieldsOrderedDifferently.def`, input có thứ tự
`49,34,56,52,40,55,60,54,21,11` và output mong đợi là header rồi body **tăng dần**. Nghĩa là
bộ chạy phải có một **ứng dụng**, không chỉ máy trạng thái session, và ứng dụng ấy ném lại
bản tin qua đúng bộ mã hoá đã có. MsgType trên dòng `E`:

| MsgType | Số dòng | |
|---|---|---|
| `A` Logon | 58 | |
| `5` Logout | 55 | |
| `D` NewOrderSingle | **42** | **ứng dụng ném lại** |
| `3` Reject | 38 | session |
| `0` Heartbeat | 33 | session |
| `4` SequenceReset | 11 | session |
| `2` ResendRequest | 9 | session |
| `1` TestRequest | 2 | session |
| `j` BusinessMessageReject | 1 | ứng dụng |
| `d` SecurityDefinition | 1 | ứng dụng |

**2. Chuỗi `58=` phải khớp từng byte, và hai chuỗi có số nhúng bên trong.** So sánh theo vị
trí nghĩa là giá trị cũng phải đúng. 17 chuỗi phân biệt:

```
10  Value is incorrect (out of range) for this tag      2  Tag specified out of required order
 7  Tag specified without a value                       2  Incorrect data format for value
 4  Invalid tag number                                  2  Incorrect BeginString
 3  SendingTime accuracy problem                        1  Unsupported Message Type
 3  Required tag missing                                1  Tag not defined for this message type
 3  CompID problem                                      1  Tag appears more than once
 1  Invalid MsgType                                     1  No Products found for this Class Symbol
 1  Incorrect NumInGroup count for repeating group
 1  MsgSeqNum too low, expecting 5 but received 2   ← có số
 1  MsgSeqNum too low, expecting 3 but received 1   ← có số
```

**Hai dòng cuối đụng thẳng bất biến 2** — *session layer thuần, không `format!`*. Xem mục
*Cách làm*, đây là quyết định lớn nhất của plan.

`373=` SessionRejectReason dùng **12 giá trị**: `5`(10), `4`(7), `0`(4), `9`(3), `10`(3),
`1`(3), `6`(2), `14`(2), `2`, `16`, `13`, `11`.

**3. ~~Chú thích `#` nằm cùng dòng với chỉ thị `i`/`e`.~~ SAI — sửa 2026-08-28 khi làm
bước 1.** Trong corpus có **0** dòng như vậy. Cái tôi thấy là do `cat *.def`: **35/59 file
không kết thúc bằng newline**, nên dòng cuối file này dính vào dòng đầu file kia, mà dòng
đầu phần lớn các file là một chú thích `#`. Đếm `eDISCONNECT` kiểu ấy ra **28** thay vì 64.

Sự thật thay thế, và nó quan trọng hơn: **đọc từng file một, đừng bao giờ `cat`.** Vào
[quickfix-acceptance-def-format.md](../reference/quickfix-acceptance-def-format.md) làm bẫy
riêng, và `tests/script.rs::concatenating_the_files_corrupts_the_corpus` tái hiện lại đúng
nó. Thay cho việc cắt `#` — vốn chỉ là code chết — bộ nạp **báo lỗi** khi gặp dòng chỉ thị
không hiểu được, thay vì bỏ qua im lặng.

## Cách làm

Một crate mới **`conformance`**, là thư viện + một test target. Không phải binary: nó phải
chạy trong `cargo test --all`, vì một cổng chạy bằng lệnh riêng là một cổng sẽ không ai chạy.

```
crates/conformance/src/lib.rs      bộ nạp kịch bản, bộ so sánh, bộ chạy
crates/conformance/src/script.rs   .def -> Vec<Step>
crates/conformance/src/compare.rs  so sánh theo vị trí, regex cho 10/42/52/60/122
crates/conformance/src/text.rs     bảng 17 chuỗi 58= và 12 mã 373=
crates/conformance/tests/fix44.rs  chạy cả 59 file, in bảng đạt/trượt
```

### Máy trạng thái là một trait, chưa phải một cài đặt

**Sửa 2026-08-28 khi làm bước 4: trait phải theo *engine*, không theo connection.**
`1b_DuplicateIdentity.def` mở connection 1, logon, mở connection 2 **cùng danh tính**, và
chờ connection 2 bị ngắt — connection 2 bị từ chối **vì** connection 1 đang tồn tại. Trait
cấp mỗi connection một đối tượng session riêng thì test đó không đời nào pass được. Nên
`Conn` là tham số, một thể hiện thấy mọi connection. 2/59 file cần điều này.

```rust
pub struct Conn(pub u32);
pub enum Input<'a> { Connect, Disconnect, Bytes(&'a [u8]), Tick(u64) }
pub enum Link { Up, Dropped }

pub trait SessionUnderTest {
    /// `emit` là generic chứ không phải `dyn` — cài đặt không trả phí vtable.
    fn step<F: FnMut(&[u8])>(&mut self, conn: Conn, input: Input<'_>, emit: F) -> Link;
}
```

Plan này giao **trait và một cài đặt rỗng** `NullSession` trả về không gì cả. `session` plan
sau sẽ thay chỗ đó. Trait sống trong `conformance` chứ không trong `session`: cổng định
nghĩa hình dạng, không phải ngược lại.

### Chuỗi `58=` — quyết định lớn nhất

**Sửa 2026-08-28 khi làm bước 3: tên `RejectText` sai.** Đo lại: 17 chuỗi xuất hiện 44 lần,
nhưng chỉ **12** đi kèm `373=` và nằm trên `Reject (35=3)`. Năm chuỗi còn lại không phải
reject:

| Chuỗi | Nằm trên |
|---|---|
| `Incorrect BeginString` | `Logout (35=5)` |
| `MsgSeqNum too low, expecting 3 but received 1` | `Logout (35=5)` |
| `MsgSeqNum too low, expecting 5 but received 2` | `Logout (35=5)` |
| `No Products found for this Class Symbol` | `SecurityDefinition (35=d)` |
| `Unsupported Message Type` | `BusinessMessageReject (35=j)` |

**Hai chuỗi có số là lý do Logout, không phải lý do reject.** Nên enum tên `SessionText`.
Cặp text↔code là 1:1 hai chiều, nên `session_reject_reason()` suy ra được từ variant.

Session layer **không dựng chuỗi**. Nó trả một enum không trường cộng với số:

```rust
pub enum SessionText {
    InvalidTagNumber, RequiredTagMissing, /* … 13 cái nữa */
    MsgSeqNumTooLow { expecting: u32, received: u32 },   // biến thể DUY NHẤT có trường
}
```

Bộ **serialiser** dựng byte, vào một `[u8; 64]` trên stack, bằng `render_u32` đã có sẵn
trong `template.rs`. Không `format!`, không cấp phát, không `String`. Bất biến 2 giữ nguyên
vì cái nó cấm là *session layer* dựng chuỗi, và session layer không dựng.

`MsgSeqNumTooLow` mang trường là ngoại lệ có tên, giống cách ADR-0005 khoanh vùng ngoại lệ
cấp phát cho TLS handshake. Nếu ca thứ hai xuất hiện, viết ADR.

### Ứng dụng echo

`EchoApp` — nhận bản tin ứng dụng, ném lại qua `Template`, đảo `49`/`56`, giữ `34` của
session. 42 dòng `E` phụ thuộc vào nó. Nó nằm trong `conformance`, **không** nằm trong
`engine`: nó là một phần của bộ đo, không phải của sản phẩm.

## Bất biến bị đụng tới

| # | Cách giữ |
|---|---|
| 1 — không cấp phát trên hot path | `conformance` là code test, **không** trên hot path. Nhưng `RejectText` và bộ dựng chuỗi thì có: chúng sẽ chạy trong `session`. Bộ dựng viết vào `[u8; 64]` trên stack; `benches/alloc.rs` thêm ca "dựng một Reject" và phải in `0` |
| 2 — session layer thuần | Đây là bất biến plan này tồn tại để bảo vệ. Trait `SessionUnderTest` **không có** socket, clock hay allocator trong chữ ký. Thời gian vào bằng `Input::Tick`. Nếu chữ ký cần thêm gì ngoài bốn thứ đó thì dừng lại và sửa plan |
| 3 — 59 định nghĩa là cổng | Plan này **là** cổng đó |
| 5 — thứ tự trường từ bảng sinh | `EchoApp` dùng `Template` + `group_order`, không tự xếp |
| 7 — không `unwrap`/`expect`/`panic` trong crate thư viện | `conformance/src/` theo luật này. `conformance/tests/` được phép, như mọi test khác |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | `script.rs`: đọc cả 59 file thành `Vec<Step>`. Test khẳng định **289 I, 250 E, 66 i, 64 e**, và chú thích cùng dòng bị bỏ đúng | — |
| 2 | `compare.rs`: so theo vị trí, regex cho 5 tag. Test: hai bản tin khác thứ tự phải **trượt**; giống nhau trừ `52=` phải **đạt** | 1 |
| 3 | `text.rs`: 17 chuỗi + 12 mã. Test khẳng định mỗi chuỗi khớp **từng byte** với chuỗi lấy ra từ corpus | 1 |
| 4 | Trait `SessionUnderTest`, `NullSession`, bộ chạy. `cargo test -p conformance` chạy cả 59 file và in **0/59** | 2, 3 |
| 5 | `EchoApp`. Chưa đổi được điểm số (chưa có session), nhưng có test riêng: ném lại một `35=D` xáo trộn và ra đúng byte của dòng `E` trong `15_…def` | 4 |

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 1 | `cargo test -p conformance script` | In `289 I, 250 E, 66 i, 64 e` trên 59 file. Sai một con số là bộ nạp sai, không phải corpus sai |
| 2 | `cargo test -p conformance compare` | Đảo hai trường trong một bản tin đúng → **trượt**. Đây là chứng minh bằng đảo ngược, bắt buộc |
| 3 | `cargo test -p conformance text` | 17 chuỗi lấy thẳng từ file `.def` lúc chạy test, so với bảng. Không hard-code hai lần |
| 4 | `cargo test -p conformance fix44` | Chạy hết 59 file, in **`0 / 59`**, không panic, không treo |
| 5 | `cargo test -p conformance echo` | Byte ra khớp **chính xác** dòng `E` thứ hai của `15_HeaderAndBodyFieldsOrderedDifferently.def`, kể cả `9=101` |
| mọi bước | `cargo test --all`, `--no-default-features`, `clippy -D warnings` | Xanh |

**Chứng minh bằng đảo ngược, bắt buộc ở bước 2 và 4:** một bộ so sánh luôn báo đạt và một bộ
chạy không chạy gì đều cho `0/59` nếu chưa có session — nên `0/59` một mình **không** là bằng
chứng bộ chạy hoạt động. Bước 4 phải kèm một `AlwaysCorrectSession` giả, phát đúng dòng `E`
đã nạp sẵn, và bộ chạy phải in **`59 / 59`** với nó. Không có test đó thì bước 4 chưa xong.

## Tài liệu phải cập nhật

- [x] `DESIGN.md` §3: thêm crate `conformance` vào bảng; `README.md` layout; `Cargo.toml` members
- [x] `DESIGN.md` §6: dòng gate "session conformance" trỏ vào lệnh chạy thật
- [x] `reference/quickfix-acceptance-def-format.md`: chú thích cùng dòng trên `i`/`e`; echo
      server; 17 chuỗi `58=` và hai chuỗi có số
- [x] `PRD.md` §2 tiêu chí 1: ghi lệnh chạy được
- [x] `STATUS.md`, `CHANGELOG.md`

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Chú thích `#` cùng dòng với `eDISCONNECT` bị nuốt vào chỉ thị | bước 1, đếm 64 dòng `e` |
| Bộ so sánh luôn báo đạt, `0/59` trông như đúng | bước 4, `AlwaysCorrectSession` phải cho `59/59` |
| So sánh chuỗi `58=` bằng "chứa" thay vì bằng nhau | bước 3, so từng byte với chuỗi lấy từ file |
| `10=` trong dòng `E` là giá trị giả — so nguyên văn sẽ trượt hết | đã ghi: **244 dòng `E`** mang `10=`, 238 trong đó là `10=0`. Comparator khớp tag 10 bằng regex `\d{3}`, không so nguyên văn |
| Bỏ quên 2 file nhiều session, chạy chúng như một session | bước 4, hai file ấy phải nằm trong danh sách chạy và không panic |
| `EchoApp` tự xếp thứ tự trường thay vì dùng `Template` | bước 5, byte phải khớp `9=101` — thứ tự sai thì độ dài vẫn đúng nhưng byte thì không |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Chữ ký `SessionUnderTest` sai, phải đổi khi viết session thật | **Cao** | Chấp nhận. Nó là trait nội bộ của crate test, đổi rẻ. Nhưng nếu phải thêm socket/clock vào chữ ký thì đó là dấu hiệu bất biến 2 sai, và phải dừng lại chứ không nới trait |
| 17 chuỗi `58=` là của QuickFIX, không phải của đặc tả FIX | Trung bình | Đúng vậy, và ghi rõ: cổng này đo *khớp với QuickFIX*, không phải *đúng theo đặc tả*. `ADR-0001` đã nói QuickFIX là oracle |
| `MsgSeqNumTooLow` mở đường cho biến thể có trường thứ hai | Trung bình | Một biến thể là ngoại lệ có tên. Cái thứ hai thì viết ADR |
| Corpus theo `master` có thể đổi | Thấp nhưng có thật | `STATUS.md` open item 7 đã ghi. Plan này không sửa, nhưng mọi con số ở trên đều kèm ngày đo |

## Ngoài phạm vi

- **Không viết session layer.** Không state machine, không quản lý sequence, không heartbeat.
- **Không socket.** Không `transport`, không `engine`.
- **Không bộ 51 định nghĩa initiator** — ADR-0004 nói chúng phải tự viết, và đó là plan khác.
- **Không sửa `10=` trong corpus.** Tính lại lúc so sánh, không sửa file.

## Nhật ký giao hàng

*(chưa bắt đầu)*

## Nhật ký giao hàng

### Đóng plan — 2026-08-28

**Xanh:** `cargo test --all` 23 binary, 0 fail. `--no-default-features` xanh. `fmt` +
`clippy -D warnings` sạch (đọc exit code, không đọc dòng chữ). Links sạch.
`benches/alloc.rs`: parse 0, encode 0, lookup 0, group 0, **text 0**.

| Bước | Kết quả |
|---|---|
| 1 | `script` 6/6. In `289 I, 250 E, 65 Connect, 1 Disconnect, 64 ExpectDisconnect` = 669 |
| 2 | `compare` 10/10. Năm tag lỏng **đọc từ `fields.fmt`**, không hard-code |
| 3 | `text` 5/5. 17 chuỗi lấy từ corpus lúc chạy, khớp từng byte |
| 4 | `fix44` 7/7. `NullSession` → **0/59**. `Replay` → **59/59** |
| 5 | `echo` 5/5. `9=101` byte-exact, và **22/22** cặp echo của cả corpus |

**Bốn chỗ plan sai, sửa theo dữ liệu:**

1. **"Chú thích `#` cùng dòng chỉ thị"** — không tồn tại. Là ảo giác của `cat *.def`:
   35/59 file không có newline cuối. Đếm `eDISCONNECT` kiểu ấy ra 28 thay vì 64.
2. **"250 dòng mang `9=`"** — 250 là số dòng `E`. Số đúng: 255 dòng mang `9=` (247 dòng `E`),
   251 mang `10=` (244 dòng `E`).
3. **`RejectText`** — sai tên. 5/17 chuỗi không nằm trên `Reject`, và **hai chuỗi có số nằm
   trên `Logout`**. Đổi thành `SessionText`.
4. **Trait theo connection** — phải theo **engine**. `1b_DuplicateIdentity` từ chối Logon thứ
   hai *vì* connection 1 tồn tại; cấp mỗi connection một session riêng thì test đó vô phương.

**Ba phát hiện mới, mỗi cái một test canh:**

- **`<TIME>` có hai độ rộng.** Giải từ chính `9=` của corpus: dòng `I` là **17** byte, dòng
  `E` là **21**. Cả hai bộ nạp trước đó dùng 21 cho tất cả — lệch 4 byte mỗi mốc thời gian,
  vô hình cho tới khi có thứ so `9=`. Ba dòng `E` "lệch 4" hoá ra không cũ: chúng viết `52=`
  17 byte trong khi `9=` tính theo 21, **đúng hiện tượng của `10=0`**.
- **Server trong bộ 59 là echo server có sắp lại thứ tự.** 22 cặp `(I, E)` ứng dụng, tái tạo
  đủ 22. Máy trạng thái session một mình không qua được bộ này.
- **Trường header nào được ném lại thì corpus nói rất rõ**: `97` PossResend **có**, `122`
  OrigSendingTime **không**. Đoán "tất cả" hỏng cái thứ hai, đoán "không cái nào" hỏng cái đầu.

**Một bộ nạp, một chỗ.** `crates/codec/tests/common/mod.rs` từng giữ bản sao riêng và **đã
bất đồng** với bản mới về độ rộng `<TIME>`. Nay nó chỉ còn là lớp chuyển đổi hình dạng trên
`nanofix_conformance::script`. Toàn bộ test của `codec` vẫn xanh không sửa một con số nào.

**Chứng minh bằng đảo ngược, mỗi lần khôi phục lại xanh:**

| Phá | Kết quả |
|---|---|
| Bộ nạp âm thầm bỏ chỉ thị lạ | 65 connect → 0, test đỏ |
| Bộ nạp bỏ thay `<TIME>` | `10_MsgSeqNumEqual.def:4 kept a placeholder` |
| Comparator luôn `Ok` | 8/9 test `compare` đỏ; `fix44` báo cả 59 file "pass" |
| Comparator so theo tag thay vì theo vị trí | hai test thứ tự đỏ — và **đây mới là cái đáng giá**: so theo tag là điều một comparator FIX tử tế sẽ làm, và bộ này không làm thế |
| Coi tag `9` là lỏng | test body-length đỏ |
| Đổi một byte của một chuỗi `58=` | `no table entry renders …` |
| Dựng chuỗi có số bằng `format!` | `allocations: text 10000`, assert nổ |
| Runner bỏ qua mọi dòng `E` | `Replay` tụt 59 → 53 |
| Bỏ kiểm "đầu ra thừa" | **vẫn xanh** — cho tới khi viết test cho nó |
| Echo giữ thứ tự đầu vào | cặp bị xáo trộn đỏ |

**Cái đáng ghi nhất:** kiểm tra "đầu ra thừa" đã được viết, chạy đúng, và **xoá đi thì không
gì đổi màu** — cho tới khi có một fake vừa trả lời đúng vừa nói thêm một câu. *Một phép kiểm
không có gì kiểm nó là một phép kiểm không tồn tại.*

**Chưa làm, nói rõ:**

- **`0/59` là điểm thật.** Chưa có session layer. Runner chỉ mới được chứng minh là **biết
  phân biệt** đúng/sai.
- **`Input::Tick` khai báo rồi nhưng chưa ai gửi.** `4a_NoDataSentDuringHeartBtInt.def` sẽ cần
  nó.
- **Bộ 51 định nghĩa initiator** vẫn phải tự viết — ADR-0004, plan khác.
- `SessionText` sẽ dời sang `session` khi crate đó tồn tại; viết sẵn để dời được.
