# Bộ chạy 59 định nghĩa acceptance

> **Loại:** Plan · **Ngày:** 2026-08-28 · **Trạng thái:** Chờ duyệt
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
- `[measured]` 250 dòng mang `9=`, **6 dòng sai** (cố ý). 246 dòng mang `10=`, **0 dòng
  đúng** — `10=` là giá trị giả, phải tính lại. Cả hai đã ghi trong `measured-costs.md`.

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

**3. Chú thích `#` nằm cùng dòng với chỉ thị `i`/`e`, không nằm cùng dòng `I`/`E`.**
`eDISCONNECT# If message is garbled, it should be ignored` là **một** dòng. `[measured]` 0
dòng `I`/`E` nào chứa `#`. Bộ nạp hiện tại chưa gặp ca này vì nó chỉ đọc `I`/`E`.

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

```rust
pub enum Input<'a> { Connect, Disconnect, Bytes(&'a [u8]), Tick(u64) }
pub enum Output<'a> { Send(&'a [u8]), Disconnect }

pub trait SessionUnderTest {
    /// Đưa một input, nhận về 0..n output. Không cấp phát: output ghi vào bộ đệm
    /// của người gọi và trả về lát cắt.
    fn step<'a>(&mut self, input: Input<'_>, out: &'a mut [u8]) -> Outputs<'a>;
}
```

Plan này giao **trait và một cài đặt rỗng** `NullSession` trả về không gì cả. `session` plan
sau sẽ thay chỗ đó. Trait sống trong `conformance` chứ không trong `session`: cổng định
nghĩa hình dạng, không phải ngược lại.

### Chuỗi `58=` — quyết định lớn nhất

Session layer **không dựng chuỗi**. Nó trả một enum không trường cộng với số:

```rust
pub enum RejectText {
    ValueOutOfRange, TagWithoutValue, InvalidTagNumber, /* … 15 cái */
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

- [ ] `DESIGN.md` §3: thêm crate `conformance` vào bảng; `README.md` layout; `Cargo.toml` members
- [ ] `DESIGN.md` §6: dòng gate "session conformance" trỏ vào lệnh chạy thật
- [ ] `reference/quickfix-acceptance-def-format.md`: chú thích cùng dòng trên `i`/`e`; echo
      server; 17 chuỗi `58=` và hai chuỗi có số
- [ ] `PRD.md` §2 tiêu chí 1: ghi lệnh chạy được
- [ ] `STATUS.md`, `CHANGELOG.md`

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Chú thích `#` cùng dòng với `eDISCONNECT` bị nuốt vào chỉ thị | bước 1, đếm 64 dòng `e` |
| Bộ so sánh luôn báo đạt, `0/59` trông như đúng | bước 4, `AlwaysCorrectSession` phải cho `59/59` |
| So sánh chuỗi `58=` bằng "chứa" thay vì bằng nhau | bước 3, so từng byte với chuỗi lấy từ file |
| `10=` trong dòng `E` là giá trị giả — so nguyên văn sẽ trượt hết | đã ghi: 246 dòng mang `10=`, **0 đúng**. Bộ so sánh phải tính lại |
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
