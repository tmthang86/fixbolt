# Máy trạng thái session FIX 4.4 — từ 0/59 lên 59/59

> **Loại:** Plan · **Ngày:** 2026-08-28 · **Trạng thái:** Đã duyệt 2026-08-28, đang làm (bước 3/6 xong)
> **Phạm vi:** Phase 1, tiêu chí 1 của `PRD.md` §2. Đây là plan lớn nhất dự án.

## Bối cảnh

Cổng đã dựng xong và **nó biết phân biệt đúng sai**: `NullSession` cho `0/59`, `Replay` cho
`59/59` ([plan trước](2026-08-28-conformance-runner.md)). Giờ mới xây cái mà nó đo.

Điểm đặc biệt của plan này: **mỗi bước có một con số**. Không có bước nào "xong rồi" mà không
kèm điểm số thay đổi. `DESIGN.md` §6 gọi 59/59 là cổng chính của phase 1, và từ đây tới đó có
sáu nấc đếm được.

Việc này **không** đụng vào socket, không đụng vào `engine`, không đụng vào journal trên đĩa.

## Những gì đã biết chắc

Đo trên corpus ngày 2026-08-28, và trên code đã có.

### Phân loại 59 file theo thứ session phải phát ra

Bỏ qua `35=D` — `EchoApp` trong `conformance` đã lo, và nó tái tạo đủ 22/22 cặp.

| Session phải phát | Số file | Nhóm |
|---|---|---|
| *không gì cả*, chỉ ngắt kết nối | **6** | `1c`×2, `1d`×3, `1e` |
| `A` Logon + `5` Logout | **12** | luồng bình thường |
| `A` + `3` Reject | **15** | `14a`–`14i`, `2k`, `2o`, `2q`, `ReverseRoute`×2 |
| `A` + `0` Heartbeat / `1` TestRequest | **11** | `4a`, `4b`, `6`, `10_*`, `11a`, `11b`, `2t`, `SessionReset` |
| `A` + `2` ResendRequest / `4` SequenceReset | **12** | `2b`, `2d`, `2m`, `3b`, `3c`, `8_*`, `11c`, `20`, `RejectResentMessage` |
| `A` một mình, hoặc trả lời mức ứng dụng | **3** | `1b`, `21` (`35=d`), `2r` (`35=j`) |
| | **59** | |

### Bốn ràng buộc bất ngờ, mỗi cái là một bẫy

**1. `TestReqID` do engine tự sinh phải đúng chữ `TEST`.** Tag `112` **không** nằm trong
`fields.fmt`, nên nó được so từng byte. Phân bố trên dòng `E`: `HELLO` ×23 (engine ném lại ID
mà đối tác gửi), **`TEST` ×2** (ID engine tự sinh), `HELLO1`/`HELLO2` ×1, `1` ×2. Sinh ID
kiểu counter hay timestamp thì trượt hai file.

**2. `HeartBtInt` là của đối tác, không phải của mình.** `108` trên dòng `E`: `30` ×43, `2`
×13, `6` ×2 — luôn bằng giá trị đối tác gửi trong Logon. Logon response ném lại nó.

**3. 33 dòng `E` không đứng ngay sau một dòng `I`.** Đó là output engine tự phát: heartbeat
hết hạn, test request, gap fill. **Đây là chỗ `Input::Tick` vào cuộc**, và `.def` không có
chỉ thị "chờ" — sự chờ đợi được diễn đạt bằng chính việc thiếu dòng `I`.

**4. Một input sinh tới 5 output.** Độ dài chuỗi dòng `E` liên tiếp: 196 lần 1, 15 lần 2, 2
lần 3, 2 lần 4, **2 lần 5**. Trait `SessionUnderTest` đã nhận `emit` gọi nhiều lần nên chịu
được, nhưng bộ đệm và mọi giả định "một vào một ra" thì không.

### Đã có sẵn, dùng lại

- `nanofix_conformance::text::SessionText` — 17 chuỗi `58=`, 12 mã `373=`, dựng không
  `format!`, không cấp phát. **Viết sẵn để dời**: nó chuyển sang `session` ở bước 3.
- `nanofix_conformance::runner::{Input, Link, SessionUnderTest, Conn}` — hình dạng đã chốt.
- `codec`: `parse_into`, `MessageView`, `Template`, `TimestampCache`, `GroupIter`.
- `dict`: `is_header`, `required`, `group_*`, `GROUP_KEYS`.

## Cách làm

Crate mới **`session`**. `#![no_std]` là mục tiêu chứ chưa phải luật (`CLAUDE.md` §6), nhưng
**không cấp phát và không `format!` là luật** (bất biến 1 và 2).

```
crates/session/src/lib.rs        Session<R: Role>, Input, Output
crates/session/src/state.rs      máy trạng thái: Disconnected → LogonSent/Received → Active → LogoutSent
crates/session/src/seq.rs        số thứ tự vào/ra, khoảng trống, PossDup
crates/session/src/validate.rs   kiểm mức session → SessionText + 373
crates/session/src/text.rs       chuyển từ conformance sang, không sửa nội dung
crates/session/src/store.rs      trait MessageStore + InMemory dùng cho test
```

### Máy trạng thái thuần, tham số hoá theo vai (ADR-0004)

```rust
pub trait Role { const IS_ACCEPTOR: bool; }
pub struct Acceptor; pub struct Initiator;

pub struct Session<R: Role, const N: usize> { /* … */ }

impl<R: Role, const N: usize> Session<R, N> {
    /// Một input, 0..5 output. `out` là bộ đệm của người gọi; không có
    /// socket, không có clock, không có allocator trong chữ ký.
    pub fn step<F: FnMut(&[u8])>(&mut self, input: Input<'_>, emit: F) -> Link;
}
```

**Nếu chữ ký này cần thêm gì ngoài `Input`, thì bất biến 2 sai và phải dừng lại** — không
nới trait. Plan trước đã ghi đúng câu này và nó đã giữ được.

### Luật `Tick` của runner, suy từ chính định dạng

`.def` không có chỉ thị "chờ". Một dòng `E` không có `I` đứng trước nghĩa là *engine tự nói*.
Nên luật là:

> Trước khi khớp một dòng `E`, nếu session chưa phát gì, đẩy đồng hồ lên **một
> `HeartBtInt`** rồi thử lại, tối đa **3 lần**. Hết 3 lần mà vẫn im lặng thì đó là
> `Reason::NoOutput`.

Xác định, không phụ thuộc đồng hồ thật. `HeartBtInt` lấy từ Logon của chính file đó — 6 giây
ở `4a` và `6`, 30 ở phần lớn còn lại. Đây là thay đổi ở `conformance/src/runner.rs`, và nó
thuộc bước 4.

### Kho tin nhắn cho resend

`20_SimultaneousResendRequest` và `RejectResentMessage` đòi phát lại bản tin ứng dụng đã gửi.
Session không sở hữu đĩa (đó là `engine`), nên:

```rust
pub trait MessageStore {
    fn put(&mut self, seq: u32, bytes: &[u8]);
    fn get(&self, seq: u32) -> Option<&[u8]>;
}
```

`conformance` cấp một bản `InMemory`. Journal mmap ba chính sách nằm ở plan `engine`, không ở
đây. **Trait này là chỗ duy nhất session chạm tới trạng thái bền vững**, và nó không được
phép cấp phát trên đường gửi.

## Bất biến bị đụng tới

| # | Cách giữ |
|---|---|
| 1 — không cấp phát trên hot path | Đây **là** hot path. `benches/alloc.rs` thêm ca "một vòng logon → 100 bản tin → logout" và phải in `0`. Chứng minh bằng đảo ngược, như bốn ca hiện có |
| 2 — session layer thuần | Plan này tồn tại để giữ nó. Chữ ký `step` không có socket/clock/allocator. Thời gian vào bằng `Input::Tick`. Lỗi là enum không trường, trừ `MsgSeqNumTooLow` đã có tên |
| 3 — 59 định nghĩa là cổng | **Mỗi bước của plan này báo cáo điểm.** Không có bước nào đóng mà không chạy `cargo test -p nanofix-conformance --test fix44` |
| 5 — thứ tự trường từ bảng sinh | Mọi bản tin phát ra đi qua `Template` + `Fix44`. Không call site nào tự xếp |
| 7 — không `unwrap`/`expect`/`panic` | `session` là crate thư viện. Lint workspace đã chặn, `check-lint-config.sh` chứng minh bằng đảo ngược |
| 4 — engine thread không ngủ | Chưa đụng tới: không có thread ở đây |

## Chia việc

**Con số ở cột "Điểm" là *dự đoán*, tính từ bảng phân loại ở trên.** Bước nào không đạt dự
đoán thì dừng lại tìm hiểu trước khi đi tiếp — lệch dự đoán là thông tin, không phải phiền
toái.

| Bước | Kết quả | Điểm dự đoán |
|---|---|---|
| 1 | Khung `Session<R>`, `Role`, trạng thái. Từ chối Logon sai: CompID, SendingTime, BodyLength, BeginString, "bản tin đầu phải là Logon" | **6 / 59** |
| 2 | Chấp nhận Logon (ném lại `108`), Logout, số thứ tự vào đúng thứ tự, `TimestampCache` cho `52` | **18 / 59** |
| 3 | `Reject (35=3)` với 12 mã `373` và 17 chuỗi. `SessionText` **dời** từ `conformance` sang | **33 / 59** |
| 4 | Heartbeat, TestRequest (`112=TEST` khi tự sinh, ném lại ID khi trả lời). Luật `Tick` vào runner | **44 / 59** |
| 5 | `ResendRequest`, `SequenceReset`/gap fill, `PossDup`/`PossResend`, `MessageStore` | **56 / 59** |
| 6 | Trùng danh tính (nhiều connection), hai trả lời mức ứng dụng (`35=d`, `35=j`). Bench, docs, merge | **59 / 59** |

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| mọi bước | `cargo test -p nanofix-conformance --test fix44` | In đúng điểm dự đoán ở bảng trên. **Cao hơn dự đoán cũng phải dừng lại giải thích** — nó nghĩa là bảng phân loại sai |
| 1 | `cargo test -p nanofix-session state` | Máy trạng thái từ chối bản tin đầu không phải Logon, và **không phát gì** trước khi ngắt |
| 2 | `cargo test -p nanofix-session logon` | Logon response ném lại `108` của đối tác, không phải hằng số. Đảo ngược: hard-code `108=30` → `4a` và `6` đỏ |
| 3 | `cargo test -p nanofix-session reject` | 12 mã `373` ánh xạ đúng. Byte của `58=` khớp corpus — test đã có sẵn ở `conformance`, chỉ đổi nguồn |
| 4 | `cargo test -p nanofix-session heartbeat` | `112=TEST` khi tự sinh; ném lại ID khi trả lời. Đảo ngược: sinh ID kiểu counter → 2 file đỏ |
| 5 | `cargo test -p nanofix-session resend` | Gap fill đúng `36=NewSeqNo`, `123=Y`. Bản tin ứng dụng phát lại có `43=Y` và `122=` |
| 6 | `cargo bench -p nanofix-codec --bench alloc` | Thêm dòng `allocations: session 0` |
| mọi bước | `cargo test --all`, `--no-default-features`, `clippy -D warnings`, `fmt --check` | Exit 0, **đọc exit code chứ không đọc dòng chữ** |

**Chứng minh bằng đảo ngược, bắt buộc ở mỗi bước.** Điểm số tăng là bằng chứng mạnh nhưng
không đủ: một bước có thể tăng điểm vì lý do sai. Mỗi bước phải phá một thứ và thấy đúng
những file mình vừa làm xanh chuyển đỏ.

## Tài liệu phải cập nhật

- [ ] `DESIGN.md` §3 bảng crate, `README.md` layout, `Cargo.toml` members
- [ ] `DESIGN.md` §4 D1 — hình dạng thật của `Session<R>`, sau khi viết chứ không trước
- [ ] `DESIGN.md` §6 — dòng gate 59/59 đổi từ "0/59 hôm nay" sang số thật
- [ ] `reference/` — mỗi bẫy đo được: `112=TEST`, `108` ném lại, luật `Tick`
- [ ] `PRD.md` §2 tiêu chí 1; `STATUS.md`; `CHANGELOG.md`
- [ ] ADR mới nếu `MessageStore` hoá ra cần hình dạng khác `put`/`get`

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Sinh `TestReqID` kiểu counter/timestamp | bước 4, `6_SendTestRequest` và `4b` |
| Hard-code `108=30` trong Logon response | bước 2, `4a` và `6` dùng `108=6` |
| Regenerate `60=` thay vì ném lại | đã canh: `echo.rs::body_length_is_one_hundred_and_one` |
| `52=` thiếu mili giây | đã canh: 3 dòng `E` có `9=` phụ thuộc vào nó |
| Giả định một input một output | 6 file có chuỗi 4–5 dòng `E` liên tiếp |
| `format!` lẻn vào đường sinh `58=` | `benches/alloc.rs`, đã canh bằng đảo ngược |
| Số thứ tự ra tăng cả khi bản tin không được gửi | bước 5, `20_SimultaneousResendRequest` |
| Ngắt kết nối mà quên phát Logout trước | bước 2, `13b_UnsolicitedLogoutMessage` |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Bảng phân loại 59 file sai, dự đoán điểm lệch | **Cao** | Chấp nhận và dùng: lệch là tín hiệu. Sửa bảng theo dữ liệu, ghi vào nhật ký, như bốn lần trước |
| `MessageStore` hình dạng sai, phải đổi khi làm `engine` | Trung bình | Trait hai hàm, đổi rẻ. Nhưng nếu phải thêm I/O vào chữ ký thì viết ADR chứ không nới |
| 6 bước là quá lớn cho một plan | Trung bình | Mỗi bước tự có cổng và tự merge được. Nếu bước 5 phình ra thì tách plan riêng cho resend |
| `112=TEST` là cấu hình của QuickFIX chứ không phải hằng số | Thấp | Comparator so từng byte, nên với cổng này nó **là** hằng số. Ghi rõ là ràng buộc của oracle, không phải của đặc tả FIX |
| Bộ 51 định nghĩa initiator vẫn chưa tồn tại | **Cao**, nhưng ngoài phạm vi | ADR-0004 đã định giá. `Role` được tham số hoá từ bước 1 để sau này không phải mổ lại |

## Ngoài phạm vi

- **Không socket, không `transport`, không `engine`.**
- **Không journal trên đĩa.** `MessageStore` là trait; bản mmap ba chính sách thuộc `engine`.
- **Không lịch phiên** (giờ mở/đóng, ngày trong tuần) — chưa có định nghĩa nào kiểm nó.
- **Không viết 51 định nghĩa initiator.** `Role` có mặt để sau này không phải mổ; việc viết
  chúng là plan khác.
- **Không tối ưu.** Bench chỉ để chứng minh 0 cấp phát, không để đuổi con số.

## Sửa plan giữa chừng

Rule Zero: plan sai giữa chừng thì **sửa plan, ghi lại**, không âm thầm đi lệch.

### Sửa 1 — runner phải gieo đồng hồ ngay từ bước 1, không phải bước 4

**Plan viết:** luật `Tick` vào runner ở bước 4.

**Thực tế:** bước 1 phải từ chối `1d_InvalidLogonBadSendingTime`, và muốn so lệch giờ thì
session phải biết "bây giờ là mấy giờ". Chưa có `Tick` nào thì nó không biết.

**Đã làm:** `run_scenario` gửi `Input::Tick(FIXED_TIME_MILLIS)` trước `iCONNECT` và trước mỗi
dòng `I`. Giá trị **cố định** — đây chỉ là gieo đồng hồ, đúng như engine thật đọc clock mỗi
vòng lặp. Cái *đẩy* đồng hồ lên một `HeartBtInt` vẫn nguyên ở bước 4.

`Replay` phải bỏ qua `Tick`, nếu không nó ăn nhầm một dòng của file và `59/59` thành vô nghĩa.

### Sửa 2 — `<TIME>` phải là một thời điểm có thật

Không có trong plan, phát hiện khi làm bước 1. Loader đang thay `<TIME>` bằng
`00000000-00:00:00`, **không phải một ngày** (tháng 00, ngày 00). Đó là placeholder của corpus
cho phần *output* mà comparator không bao giờ đọc theo giá trị — không phải cho input.

Đã đổi sang `20260828-12:00:00`. Việc này còn sửa luôn một lỗi im lặng: `<TIME-121>` trước đây
chạy **tới** 86 279 giây chứ không lùi 121 giây. Chi tiết trong
[reference/quickfix-acceptance-def-format.md](../reference/quickfix-acceptance-def-format.md).

### Sửa 3 — thêm `DESIGN.md` D13, mốc thời gian của `Tick`

Plan không nói `Tick(u64)` đếm từ đâu. Đếm từ 1970 thì hơn một phần năm dải năm mà
`SendingTime` viết được không tồn tại, và phép trừ lệch giờ sẽ tràn ngầm. Đổi sang đếm từ
0000-01-01. Đây là quyết định kiến trúc nên nó nằm ở `DESIGN.md` D13, không nằm trong plan.

### Sửa 4 — bảng phân loại 59 file sai, và mọi con số dự đoán sai theo

**Phát hiện ở bước 2.** Bảng trong plan nói 12 file chỉ chờ `{A, 5}`. Giải lại từ chính corpus
— gom mọi dòng `E` theo tập `35=` mà nó chờ — thì là **9**. Bảng đúng, đo ngày 2026-08-28:

| `35=` mà file chờ nhận | Số file | Bước |
|---|---|---|
| *(không gì cả)* | 6 | 1 |
| `A` `5` | 9 | 2, **trừ `AlreadyLoggedOn` → bước 6** |
| `A` | 1 | 6 — `1b_DuplicateIdentity` |
| `A` `5` `3` | 13 | 3 |
| `A` `5` `0` | 8 | 4 |
| `A` `1` `0` | 1 | 4 |
| `A` `5` `3` `0` | 1 | 4 |
| `A` `5` `2` | 3 | 5 |
| `A` `5` `0` `4` | 1 | 5 |
| `A` `5` `3` `2` `0` | 1 | 5 |
| còn lại, đều có `D`/`d`/`j` | 15 | 6 |

Điểm dự đoán mới, cộng dồn: **6 → 14 → 27 → 37 → 42 → 59**. Vẫn là dự đoán.

Hai file mất đi ở bước 2 và **trước đó đang đậu nhờ ăn may**: `1b_DuplicateIdentity` và
`AlreadyLoggedOn`. Cả hai mở connection thứ hai với cùng danh tính. Khi `connect` chưa reset số
thứ tự, Logon thứ hai (`34=1`) bị từ chối vì *số quá thấp* chứ không phải vì trùng danh tính —
đúng kết quả, sai lý do. Reset số thứ tự (mà `2i` bắt buộc phải có) làm lộ ra.

**Bài học ghi lại: một bước chỉ đếm lên thì không thấy chỗ này.** Cổng bước 2 vì thế liệt kê
đủ tên 14 file, và có thêm một test riêng nói "sáu file của bước 1 vẫn còn trong đó".

### Sửa 5 — `SessionText` dời sang `session` ở bước 2, không phải bước 3

`2c` và `2i` cần `58=` trên Logout ngay ở bước 2. Đã `git mv` cả module lẫn test sang
`session`; `codec/benches/alloc.rs` bỏ ca `text` vì bảng đi rồi, và ca đó nay nằm ở
`session/benches/alloc.rs`.

### Sửa 6 — bước 3 bị chặn bởi một plan khác

**Phát hiện khi bắt đầu bước 3.** 8 trong 12 mã `373` không phải luật của session mà là câu hỏi
cho từ điển: tag có tồn tại không, tag có thuộc bản tin này không, giá trị có trong enum không,
giá trị có đúng kiểu không, MsgType có thật không. `dict` hôm nay chỉ sinh `is_header`,
`data_length_tag`, `required` và `group_members` — không có bảng nào trả lời được.

Đây là thay đổi codegen và public API của `dict`, đúng là **tiêu chí 3 của `PRD.md`**, thứ PRD
vốn ghi là gap chưa có plan. Chủ dự án chọn: plan riêng, duyệt trước.

→ [2026-08-28-dict-validation.md](2026-08-28-dict-validation.md). Xong 2026-08-28, bước 3 chạy
tiếp.

### Sửa 7 — thiếu một bảng nữa: `required_header()`

Phát hiện khi làm bước 3, **sau khi** plan dict đã đóng. `14b_RequiredFieldMissing.def` gửi
Heartbeat không có `56=` và chờ `373=1` với `371=56`. Nhưng `required(msg_type)` trả lời cho
*thân* bản tin, và doc của chính nó nói vậy — `56` là trường header.

Đã thêm `Fix44::required_header()` (7 tag). Đây là **hoàn thiện một bảng đã có**, không phải năng
lực mới, nhưng nó vẫn là public API nên ghi ra đây thay vì làm im. Plan dict đã đóng và
`CLAUDE.md` §5 cấm sửa nội dung một bản ghi đã đóng, nên chỗ ghi là plan đang chạy — plan này.

## Nhật ký giao hàng

### Bước 1 — 2026-08-28 — **6 / 59**, đúng dự đoán

Crate `nanofix-session`: `Session<R, N>`, `Role`/`Acceptor`/`Initiator`, `Config`, `clock`.
Từ chối theo 5 luật; chưa phát gì.

**Đảo ngược, 6 lần.** Năm lần đầu đưa điểm về 5/59:

| Bỏ đi | Điểm |
|---|---|
| kiểm `8=` BeginString | 5 / 59 |
| kiểm `49=` SenderCompID | 5 / 59 |
| kiểm `56=` TargetCompID | 5 / 59 |
| kiểm lệch `52=` SendingTime | 5 / 59 |
| `Validation::ALL` → `NONE` | 5 / 59 |
| **"bản tin đầu phải là Logon"** | **6 / 59 — không đổi** |

Lần thứ sáu là một phát hiện: `1e_NotLogonMessage.def` gửi `35=0` **và** `56=DLSI`. Kiểm
CompID bắt trước, nên corpus không phân biệt được hai luật. Đã viết
`crates/session/tests/logon.rs` cầm luật đó, lấy chính dòng đó và sửa `56=` lại cho đúng.

Hai bẫy nữa, cả hai là "xanh giả":
- `Name::fits` lúc đầu vô dụng — tràn thì `len` đã bằng 0 rồi, nên bỏ `fits` đi vẫn xanh. Đã
  đổi để `fits` là thứ duy nhất chặn, và đảo ngược mới đỏ.
- Một bản tin thiếu `10=` parse ra `Incomplete`, mà session coi `Incomplete` là "chờ thêm" →
  `Link::Up`. Cả hai vế của một test hai chiều cùng xanh trên một bản tin chưa hề bị xét.

**Cấp phát:** `accept 0 refuse 0 tick 0 clock 0`. Đảo ngược (một `format!` trên đường lỗi) cho
`refuse 30000`.

**Cổng:** `cargo fmt --check`, `clippy --all-targets --all-features -D warnings`, `clippy
--no-default-features`, `cargo test --all`, `cargo test --all --no-default-features` — tất cả
rc=0. Máy: Apple M5, macOS 25.5.0, cargo 1.95.0.


### Bước 2 — 2026-08-28 — **14 / 59**, dự đoán 18, và chênh lệch đã hiểu

Trả lời Logon (ném lại `98=` và `108=`), trả lời Logout, số thứ tự vào/ra, `TimestampCache`
cho `52=`. Thêm trạng thái `AwaitingLogout`, và Logout kèm `58=` cho hai lỗi xảy ra *sau khi*
đã logon — cùng lỗi ấy trước khi logon thì im lặng ngắt (`1d` so với `2i`).

Chênh lệch: xem "Sửa 4". Trần thật của bước 2 là 14, và nó đạt đúng 14.

**Đảo ngược, 11 lần.** Chín lần đầu:

| Bỏ đi | Điểm |
|---|---|
| không trả lời Logon | test riêng đỏ |
| không trả lời Logout | 8 / 59 |
| `43=Y` không còn tha số thứ tự thấp | 12 / 59 |
| số quá thấp ngắt im lặng thay vì nói lý do | 13 / 59 |
| BeginString sai sau logon ngắt im lặng | 13 / 59 |
| `connect` không reset số thứ tự | 13 / 59 |
| trả lời cả byte đến sau khi link đã xuống | 13 / 59 |
| `next_out` không tăng | 6 / 59 |
| **`52=` đóng dấu từ hằng số, không từ đồng hồ** | **14 / 59 — không đổi, tất cả xanh** |

Lần thứ chín là một phát hiện: `52` nằm trong `fields.fmt`, nên comparator chỉ so **hình
dạng**. **Corpus không nhìn thấy giá trị của trường này.** Đã viết
`the_reply_carries_the_clock_the_session_was_ticked_to` và
`the_clock_moves_and_the_next_message_says_so`; hai đảo ngược cuối (đóng dấu từ hằng số, và
đóng dấu một lần rồi cache mãi) nay đều đỏ.

Một bẫy nữa: test cũ dựng "Logon" bằng cách sửa `35=0` thành `35=A` trên dòng của
`1e_NotLogonMessage` — dòng đó **không có `98=` và `108=`**, mà FIX 4.4 bắt buộc cả hai. Đã
dựng lại từ dòng Logon thật của `1c_InvalidTargetCompID`, và thêm test cho Logon thiếu trường
bắt buộc.

**Cấp phát:** `accept 0 refuse 0 tick 0 clock 0 text 0`.

**Cổng:** `fmt --check`, `clippy --all-targets --all-features -D warnings`, `test --all`,
`test --all --no-default-features`, `check-lint-config.sh`, `check-links.py` — tất cả rc=0.
Máy: Apple M5, macOS 25.5.0, cargo 1.95.0.

### Bước 3 — 2026-08-28 — **27 / 59**, đúng dự đoán đã sửa

`Reject (35=3)`, đủ 13 file nhóm `{A, 5, 3}`. 12 mã `373`, 6 tag định tuyến đảo chiều, và
`SessionText` đã dời sang từ bước 2.

**Thứ tự kiểm là thứ tự corpus trả lời, và nó chịu lực.** Nhiều bản tin sai *hai* kiểu cùng lúc,
mà corpus chỉ thấy mã nào về:

| File | Sai gì | Mã thắng | Suy ra |
|---|---|---|---|
| `14d` | `56=` rỗng **và** lệch CompID | `373=4` | quét trường chạy trước kiểm CompID |
| `14b` | thiếu hẳn `56=` **và** lệch CompID | `373=1` | trường bắt buộc cũng chạy trước CompID |
| `2q` | `35=*` — kiểu sai, nên **mọi** tag đều "không thuộc bản tin này" | `373=11` | MsgType phải xong trước mọi câu hỏi theo trường |

Hai mã kèm Logout ngay sau Reject (`2k` CompID, `2o` SendingTime); mười mã còn lại giữ phiên
sống — `14a` từ chối bốn bản tin liên tiếp trên một kết nối.

`14a` gửi `-1=HI`, không phải tag và không bao giờ là tag. Codec trả `ParseError::BadTag { at }`
và **đặc tả rằng index vẫn giữ mọi trường đọc trước đó** — nhờ vậy `34=` và `35=` vẫn còn để trả
lời, và `371=` lấy nguyên văn bản chứ không phải một số.

**Đảo ngược, 13 lần, cả 13 đỏ** — sau khi sửa một lần đảo ngược sai. Lần đầu tôi xoá một dòng
`next_in = seq + 1` **không bao giờ chạy tới** nằm trong nhánh Reject; tất nhiên không có gì đổi.
**Một đảo ngược không làm đổi hành vi thì không chứng minh gì về cái chốt.** Đã xoá dòng chết,
đảo ngược lại đúng chỗ, và nó đỏ ở `tests/reject.rs`.

Một chốt corpus **không** thấy: Reject có tiêu thụ số thứ tự vào hay không. Nhánh "số quá cao"
chưa có (bước 5), nên bản tin vượt trước hiện được đọc như đúng thứ tự, và số thứ tự không tăng
trông y hệt số thứ tự có tăng. `tests/reject.rs` cầm luật đó.

Thêm một cổng ngoài điểm số: `all_twelve_session_reject_reasons_are_produced` quét chính các dòng
`E` của corpus, rút ra 12 mã `373`, rồi kiểm rằng session **thực sự phát ra đủ 12**. Đếm file
không nói được điều này — `14a` có bốn ca và một session trả cùng một mã cho cả bốn vẫn qua file.

**Cấp phát:** `accept 0 refuse 0 tick 0 clock 0 text 0`.

**Cổng:** `fmt --check`, `clippy --all-targets --all-features -D warnings`, `test --all`,
`test --all --no-default-features`, `check-lint-config.sh`, `check-links.py` — tất cả rc=0.
Máy: Apple M5, macOS 25.5.0, cargo 1.95.0.