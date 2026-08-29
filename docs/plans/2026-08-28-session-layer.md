# Máy trạng thái session FIX 4.4 — từ 0/59 lên 59/59

> **Loại:** Plan · **Ngày:** 2026-08-28 · **Trạng thái:** Đã duyệt 2026-08-28, đang làm (bước 5/6 xong)
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

- [x] `DESIGN.md` §3 bảng crate, `README.md` layout, `Cargo.toml` members — bước 1
- [x] `DESIGN.md` §4 D1 — hình dạng thật của `Session<R>`, sau khi viết chứ không trước; thêm
      `Application` ở 6a và món nợ `Store` ở 6b
- [x] `DESIGN.md` §6 — dòng gate đổi sang **59 / 59**
- [x] `reference/quickfix-acceptance-def-format.md` — mười bẫy đo được, từ `112=TEST` và luật
      `Tick` tới luật tag số nguyên có dấu và luật đóng khung theo `9=`
- [x] `PRD.md` §2 tiêu chí 1; `STATUS.md`; `CHANGELOG.md`
- [x] Không cần ADR: `MessageStore` đúng hình dạng `put`/`get` đã phác

**Một dòng ở bảng kiểm chứng đã làm khác:** bench cấp phát của session là
`cargo bench -p nanofix-session --bench alloc`, không phải thêm một dòng vào bench của `codec`.
Lý do: nó cần corpus, mà `codec` không phụ thuộc `conformance` và không được phép phụ thuộc.

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

### Sửa 8 — bước 4 phải làm nhiều hơn dòng mô tả của nó

**Dòng "Chia việc" của bước 4 viết:** Heartbeat, TestRequest, luật `Tick`.

**Thực tế:** bảng phân loại đã sửa (Sửa 4) giao cho bước 4 **10 file**, và ba trong số đó cần
những thứ dòng ấy không nhắc tới. Bảng phân loại nhóm file theo *cái session phát ra*, còn dòng
mô tả nói về *cái session phải hiểu* — hai chuyện khác nhau, và với ba file này chúng lệch:

| File | Phát ra | Nhưng phải hiểu |
|---|---|---|
| `10_*`, `11a`, `11b`, `11c` | `{A, 5, 0}` hoặc `{A, 5, 3, 0}` | `SequenceReset (35=4)` vào, cả gap fill lẫn không |
| `SessionReset` | `{A, 5, 0}` | `141=Y` `ResetSeqNumFlag` trên Logon |
| `2t` | `{A, 5, 0}` | bản tin hỏng phải **bỏ qua**, không ngắt kết nối |

`SequenceReset` nằm ở dòng bước 5 của bảng "Chia việc". Nhưng bước 5 là *phát* `2` và `4`;
phần *nhận* thuộc về những file này. Đã làm ở bước 4 và ghi ở đây. Dự đoán điểm không đổi:
bảng phân loại vốn đã tính đúng 10 file, chỉ dòng mô tả là hụt.

**Bài học: một plan mô tả việc bằng tên bản tin thì sẽ hụt ở mọi chỗ mà đọc và viết không đối
xứng.**

### Sửa 9 — `MessageStore` và `PossDup`/`PossResend` không thuộc bước 5

**Dòng bước 5 viết:** ResendRequest, SequenceReset/gap fill, PossDup/PossResend, `MessageStore`.

**Thực tế:** năm file của bước 5 không cần cái nào trong hai thứ sau.

- `8_OnlyAdminMessages.def` — tên file nói thẳng — chỉ chứa bản tin quản trị, mà QuickFIX
  **không bao giờ phát lại bản tin quản trị**: nó lấp khoảng trống bằng một `SequenceReset`. Không
  có gì để lưu, nên không có `MessageStore`.
- `PossDup`/`PossResend` nằm ở `2f`, `2g`, `19a`, `19b` — cả bốn đều chờ thêm bản tin ứng dụng dội
  lại, tức nhóm bước 6.

Thêm một trait mà chưa file nào chạm tới là thêm trạng thái không có gì kiểm — đúng thứ plan này
đã viết ở bước 1 là không làm. Dời cả hai sang bước 6, ghi ở đây. Dự đoán điểm không đổi.

**Đổi lại, bước 5 phải làm một thứ dòng mô tả không nhắc: hàng đợi bản tin đến sớm.**
`RejectResentMessage.def` giữ một `TestRequest` `34=3`, xử lý `34=2`, rồi phát lại `34=3` *trước*
`34=4` vừa tới. Không có hàng đợi thì mất file đó.

### Sửa 10 — bước 6 tách làm 6a và 6b

**Phát hiện khi bắt đầu bước 6.** 17 file còn lại, và chúng cần **năm** năng lực khác nhau, trong
đó hai cái là thay đổi API công khai. Một commit ôm cả năm thì không có cổng nào ở giữa, và
`CLAUDE.md` §8 nói một commit là một thay đổi mạch lạc.

Chia theo cái chúng cần, đo từ chính corpus:

| Bước | Năng lực | File | Điểm |
|---|---|---|---|
| **6a** | Giao bản tin cho **ứng dụng** (trait mới, API công khai); `PossDup` thiếu `122=` và `122=` lớn hơn `52=`; trùng danh tính giữa nhiều connection | `15`, `14e`, `21`, `2r`, `19a`, `19b`, `2f`, `2g`, `1b`, `AlreadyLoggedOn` | **52 / 59** |
| **6b** | `MessageStore`: lưu bản tin ứng dụng đã gửi và **phát lại** chúng; xen kẽ với gap fill cho các đoạn quản trị | `8_OnlyApplicationMessages`, `8_AdminAndApplicationMessages`, `20`, `2d`, `2m`, `3b`, `3c` | **59 / 59** |

**Bảng trên sai ba dòng, và đã đo ra chỗ sai.** Bước 6a về đích ở **55**, không phải 52.
Cách chia được rút ra từ *tập `35=` mà mỗi file chờ đợi*, mà một tập `35=` **không phân biệt được
echo với phát lại**: `2d`, `3b` và `3c` trông như cần kho bản tin gửi ra nhưng không cần — đối tác
tự gửi lại, đầu này chỉ echo. Ba file ấy thuộc 6a. `2d` và `3c` còn cần thêm một luật nữa, ghi ở
nhật ký bên dưới. Bước **6b** còn đúng **bốn** file: `20`, `2m`, `8_AdminAndApplicationMessages`,
`8_OnlyApplicationMessages`.

**Trait `Application` là API công khai mới**, nên nó được ghi ra đây thay vì làm im:

```rust
pub trait Application {
    fn on_message(&mut self, msg: &[u8], seq: u32, stamp: &[u8], out: &mut [u8])
        -> Option<Range<usize>>;
}
```

Session sở hữu bảy loại bản tin quản trị (`0 1 2 3 4 5 A`) và giao mọi thứ khác cho đây. Nó cấp
hai thứ ứng dụng không sở hữu — số thứ tự phát và đồng hồ — rồi gửi nguyên văn cái nhận lại.
`received` cũ giữ nguyên chữ ký và gọi `received_with` với một ứng dụng không bao giờ trả lời.

**Luật "trùng danh tính" không nằm trong session.** Nó là luật của *engine*: connection nào đang
giữ danh tính. `engine` chưa tồn tại, nên `tests/score.rs` đóng vai nó — và điều đó được ghi rõ
ở đó, ở đây, và trong `STATUS.md`. Doc của `runner.rs` đã lường trước: `SessionUnderTest` là một
instance cho cả engine, không phải cho một connection.

### Sửa 11 — `2m` không cần kho bản tin, nó cần **đóng khung**

**Phát hiện khi bắt đầu bước 6b.** Bảng ở Sửa 10 xếp `2m_BodyLengthValueNotCorrect` vào nhóm
"cần `MessageStore`". Đọc kỹ file thì không: nó không chờ một bản tin phát lại nào cả. Nó chờ
**hai bản tin sai `9=` bị bỏ đi đúng cách**, và hai dòng comment trong chính file nói ra luật:

- `9=30` (khai báo *ngắn* hơn thân thật) — *"Invalid message was ignored, and valid one was
  processed"*. Bản tin ấy biến mất; bản tin kế tiếp tới ở lần đọc khác nên nguyên vẹn.
- `9=111` (khai báo *dài* hơn) — *"it will combine with the next message and be ignored"*. Nó
  **nuốt** bản tin kế tiếp, và cả hai cùng biến mất.

Một luật tái tạo được cả hai: **`9=` được tin theo đúng nghĩa đen.** Đếm tới cuối thân mà không
gặp `10=NNN|` ở đúng chỗ thì cả **bộ đệm** bị vứt, không phải chỉ bản tin ấy.

**Chỗ đặt: `tests/score.rs`, không phải `session`.** Đóng khung là việc của engine —
`DESIGN.md` §2 xếp nó ở L3 — và session nhận vào một bản tin đã trọn vẹn. Giống hệt luật
"một danh tính, một connection" ở bước 6a.

**Và luật cũ không bị chép lại.** Khi bộ đệm bị vứt, adapter vẫn đưa nguyên bộ đệm ấy cho
session một lần: session tự thấy không đọc nổi, tự chạy `garbled()`, và tự quyết định —
`1d_InvalidLogonLengthInvalid.def` (`9=40` trên một Logon) vẫn phải **rớt kết nối**, vì luật
"khung hỏng chỉ chí mạng khi nó tự xưng là Logon" nằm ở đúng một chỗ và không được nhân đôi.

**Bước 6b vì thế còn ba file cần kho**: `20`, `8_AdminAndApplicationMessages`,
`8_OnlyApplicationMessages`.

**Kho nằm trong `out::Outbound`, `pub(crate)` — không đổi API công khai.** Doc của `Outbound`
đã lường trước: *"a resend replays stored bytes rather than re-encoding"*. Đây là **tạm**:
`DESIGN.md` D1 phác `Action::Store` và §2 nói engine giữ journal. Một acceptor thật cần journal
sống qua lần khởi động lại, và cái đó thuộc về `engine`.

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
### Bước 4 — 2026-08-29 — **37 / 59**, đúng dự đoán đã sửa

Đồng hồ. `Heartbeat (35=0)`, `TestRequest (35=1)`, `SequenceReset` vào, `141=Y`, và bản tin
hỏng thì bỏ qua thay vì ngắt. Cộng thêm luật `Tick` ở runner.

**Corpus không nhìn thấy một ngưỡng nào cả.** Harness chỉ đẩy đồng hồ được từng `HeartBtInt`
một, nên `[đo 2026-08-29]` mọi ngưỡng test request trong (1×, 2×] và mọi ngưỡng bỏ cuộc trong
(2×, 3×] đều tái tạo `6_SendTestRequest.def` y hệt. Ba con số 1.0 / 1.2 / 2.4 là của QuickFIX
và chỉ `tests/heartbeat.rs` giữ chúng — nó đẩy đồng hồ từng mili giây và kiểm cả hai phía biên.

**Thứ tự kiểm số thứ tự là *của từng `MsgType`*, không phải một luật chung.** QuickFIX gọi
`verify(msg, checkTooHigh, checkTooLow)` với tham số khác nhau từ mỗi handler:

| `35=` | kiểm quá thấp | tăng `34=` vào | File chứng minh |
|---|---|---|---|
| `5` Logout | **không** | có | `10_MsgSeqNumEqual` — gap fill lên 20 rồi logout với `34=3` |
| `4` có `123=Y` | có | **không** | `10_MsgSeqNumLess` |
| `4` không gap fill | **không** | **không** | `11a`, `11b`, `11c` — cả ba gửi `34=0` |
| còn lại | có | có | `2c`, `14a` |

Áp một luật cho tất cả thì được **36 / 59**, và mất file nào thì tuỳ luật chọn — đã đảo ngược
đủ bốn cách.

**Một luật tôi tự bịa, và corpus bác.** `FieldType::SeqNum` từ chối `34=0`, chú thích viện dẫn
`11c_NewSeqNoLess.def` làm bằng chứng — và đọc sai file ấy. `11c` từ chối vì `36=1` thấp hơn số
đã tới (`373=5`, **không có `371=`**), chứ không phải vì `34=0` sai kiểu (thì đã là `373=6` với
`371=34`). Ca kiểm thử phủ định tương ứng nằm đúng trong khối mà chú thích của chính nó ghi
"những ca này viết tay, không lấy từ capture". **Một ca bịa ra đồng ý với một luật bịa ra là
hai lần phát biểu cùng một phỏng đoán, và nó đọc y như một cái test.** Khôi phục luật cũ:
37 → 34.

**Đảo ngược: 32 lần, cả 32 đỏ — sau khi sửa ba lần đảo ngược vô giá trị.**

22 lần đầu chạy vào cổng điểm số:

| Bỏ đi | Điểm |
|---|---|
| runner không đẩy đồng hồ ở dòng `E` | 35 |
| runner không đẩy đồng hồ ở `eDISCONNECT` | 36 |
| runner đặt lại đồng hồ về mốc corpus trước mỗi `I` | 35 |
| runner hard-code `HeartBtInt` 30 s | 35 |
| `112=` tự sinh không phải chữ `TEST` | 36 |
| trả lời TestRequest bằng ID của mình | **29** |
| một tick phát hai bản tin | 36 |
| kiểm test request trước khi kiểm hết hạn | 36 |
| bản tin đến không xoá test request đang treo | 36 |
| bản tin hỏng lại thành chí mạng | 36 |
| Logon hỏng bị bỏ qua như mọi bản tin khác | 36 |
| `35=` không cần là trường thứ ba | 36 |
| Logout bị kiểm số thứ tự | 36 |
| `SequenceReset` thường bị kiểm số thứ tự | 34 |
| `SequenceReset` tăng số thứ tự vào | 36 |
| `36=NewSeqNo` lùi mà không bị Reject | 36 |
| `141=Y` không đặt lại hai bộ đếm | 36 |
| `141=Y` không được ném lại | 36 |
| khoảng nhịp hard-code thay vì lấy của đối tác | 35 |
| im lặng không đo từ lần nhận cuối | **6** |
| im lặng không đo từ lần gửi cuối | **6** |
| `SeqNum` từ chối số 0 trở lại | 34 |

10 lần sau chạy vào `tests/heartbeat.rs`, mỗi lần đỏ đúng test giữ luật đó: ba ngưỡng 1.0 /
1.2 / 2.4, việc kiên nhẫn nới ra theo số test request đang treo, việc một test request chưa
trả lời làm im nhịp tim, `108=0`, và cả hai vế của luật "bản tin hỏng chỉ chí mạng khi nó tự
xưng là Logon".

**Ba lần đảo ngược vô giá trị, và cả ba là cùng một hình dạng: hai chốt che nhau.**

- `else if` cho nhịp tim **và** điều kiện `test_requests == 0` — bỏ cái nào cũng xanh, vì cái
  còn lại đủ. Giữ điều kiện (nó chặn được cả khoảng giữa 1.2× và 2.4×, `else` thì không), bỏ
  `else`. Đảo ngược lại thì đỏ.
- `connect` xoá đồng hồ **và** `tick` trả về sớm khi chưa logon — bỏ cái nào cũng xanh. Giữ
  cái kiểm trạng thái, bỏ khối xoá trong `connect`, vì mọi trường nó xoá đều bị Logon ghi đè
  trước khi có ai đọc.
- Một test viết sai: nó kiểm output rỗng mà không kiểm `Link`, nên đảo ngược làm rớt kết nối
  vẫn xanh. Đã kiểm cả hai.

**Bench cấp phát mọc thêm hai ca**, vì bước này thêm hai đường *gửi*: `beat` (tick tự phát nhịp
tim) và `answer` (trả lời TestRequest). Ca `accept` cũng sửa: nó phát lại một Logon vào **cùng**
một session 10 000 lần, nên 9 999 lần đo nhánh thoát sớm chứ không đo đường nó mang tên.

`allocations: accept 0 refuse 0 tick 0 beat 0 answer 0 clock 0 text 0`. Đảo ngược (một
`format!` trên cả hai đường mới) cho `beat 10000 answer 10000`.

**Cổng:** `fmt --check`, `clippy --all-targets --all-features -D warnings`, `test --all`,
`test --all --no-default-features`, `check-lint-config.sh`, `check-links.py` — tất cả rc=0.
Máy: Apple M5, macOS 25.5.0, cargo 1.95.0.

### Bước 5 — 2026-08-29 — **42 / 59**, đúng dự đoán

Khoảng trống số thứ tự. Bản tin chạy trước bị **giữ lại** chứ không bị từ chối, session hỏi lại
phần đã mất bằng `ResendRequest (35=2)`, và một `ResendRequest` đến được trả lời bằng một
`SequenceReset` lấp trống.

**Hai luật, mỗi luật corpus chỉ nói đúng một lần.**

- **Hỏi một lần cho một khoảng trống.** `10_MsgSeqNumGreater.def` gửi `34=10` rồi `34=20` khi
  khoảng trống còn mở, và chờ **một** `ResendRequest`. Hỏi hai lần thì lần thứ hai là output không
  dòng nào yêu cầu, và file rớt vì nó.
- **Trả lời Logon trước, rồi mới hỏi.** `1a_ValidLogonMsgSeqNumTooHigh.def` mở phiên bằng `34=5`
  và chờ `35=A` rồi mới `35=2`. Một Logon chạy trước vẫn là một Logon.

**Bảng kiểm số thứ tự mọc thêm hai dòng** so với bước 4: `ResendRequest` không bị kiểm (nó có thể
đến sau khi bộ đếm đã vượt qua nó — `8_OnlyAdminMessages` gửi `34=5` hai lần), và `Logon` kiểm
"quá cao" **sau** khi đã trả lời.

**Hình dạng của gap fill, mọi phần đều bị so:** `34=` là *số đầu của khoảng được lấp*, không phải
số phát tiếp theo — và phát nó **không tiêu một số nào**. `8_OnlyAdminMessages` lấp `34=1` trong khi
bản tin thật kế tiếp là `34=5`, và cả hai đều nằm trong file. `36=` là một số sau số cuối được lấp.

**Đảo ngược: 19 lần, cả 19 đỏ** — 13 lần vào cổng điểm số:

| Bỏ đi | Điểm |
|---|---|
| không hỏi lại khi bản tin chạy trước | 39 |
| Logon chạy trước không hỏi lại | 41 |
| hỏi trước rồi mới trả lời Logon | 41 |
| hỏi lại một khoảng trống đã hỏi | 41 |
| bản tin chạy trước bị bỏ chứ không giữ | 41 |
| không bao giờ phát lại bản tin đã giữ | 41 |
| `ResendRequest` bị kiểm số thứ tự | 41 |
| `ResendRequest` đến không được trả lời | 41 |
| `16=0` không đọc là "và tất cả sau đó" | 41 |
| gap fill tiêu một số phát ra | 41 |
| gap fill tự đánh số theo bộ đếm phát | 41 |
| `36=` là số cuối chứ không phải một số sau nó | 41 |
| `ResendRequest` hỏi từ 1 thay vì từ bộ đếm | 39 |

6 lần sau vào `tests/resend.rs`. **Hai trong sáu lần đầu xanh, và cả hai là test của tôi viết
hụt**, không phải chốt thừa:

- "khoảng trống đóng ở bản tin đầu thay vì bản tin cuối" — test lấp đủ khoảng trống trong một
  nhịp nên không bao giờ ở trạng thái *lấp một nửa*. Đã thêm một bản tin chạy trước ngay giữa
  chừng: khoảng trống còn mở nên nó **không** được hỏi lại.
- "bản tin dài quá chỗ giữ thì cứ chép" — corpus dài nhất 101 byte, chỗ giữ 512, nên không có gì
  chạm biên. Bỏ chốt ấy đi là `copy_from_slice` lệch độ dài, tức **panic trên một đường mà đối
  tác điều khiển hoàn toàn**. Đã viết test hai vế: 400 byte thì giữ, 500 byte thì bỏ.

**Ba luật corpus không nhìn thấy**, vì mọi file mở khoảng trống đều kết thúc trước khi mở cái thứ
hai, và sâu nhất chỉ giữ hai bản tin: khoảng trống đã lấp thì phải **đóng** (không đóng thì lần sau
session câm lặng vì "đã hỏi rồi"), bản tin giữ phát lại **theo thứ tự số**, và hết chỗ thì **bỏ**
chứ không cắt.

**Cấp phát:** `accept 0 refuse 0 tick 0 beat 0 answer 0 gap 0 fill 0 clock 0 text 0`. Đảo ngược
(một `format!` trên cả hai đường mới) cho `gap 10000 fill 10000`.

**Cổng:** `fmt --check`, `clippy --all-targets --all-features -D warnings`, `test --all`,
`test --all --no-default-features`, `check-lint-config.sh`, `check-links.py` — tất cả rc=0.
Máy: Apple M5, macOS 25.5.0, cargo 1.95.0.

### Bước 6a — 2026-08-29 — **55 / 59**, dự đoán 52 và vượt vì một lý do đo được

Session sở hữu bảy loại bản tin quản trị (`0 1 2 3 4 5 A`) và **giao mọi thứ khác cho ứng dụng**.
Trait `Application` là API công khai thứ hai của crate này; nó được cho mượn hai thứ nó không sở
hữu — số thứ tự phát ra và đồng hồ — cùng một vùng đệm để viết câu trả lời vào.

**Trả lời `None` thì không tiêu số nào.** `19a` chứng minh: nó gửi một order `97=Y` có `11=` đã
thấy, chờ **không** hồi đáp, rồi đánh số bản tin sau như thể bản tin ấy chưa từng tới.

**Vượt dự đoán vì cách chia 6a/6b vẽ từ tập `35=` mong đợi**, và tập ấy không phân biệt được echo
với phát lại. `2d`, `3b`, `3c` không cần kho bản tin nào cả.

**Một luật, một giờ, và một bản sửa đã bị revert.** Giả thuyết đầu: `2d_InvalidBodyLength.def`
rớt ở `9=`, nên `codec/src/parse.rs` được sửa để kiểm khung *trước* khi tách trường. Rồi đếm thật
khung của `2d`: `9=52` **đúng**. Sự thật là QuickFIX đọc tag bằng một phép chuyển **số nguyên có
dấu**, nên `-1=x` **là** một trường (Reject, `14a`) còn `4garbled9=x` **không** là trường nào cả
(cả bản tin bị bỏ qua, `2d` và `3c`). Ba file đứng trên đúng một dòng ấy. 53 → 55. Ba file codec
đã `git checkout` về nguyên trạng.

**Đảo ngược: 10 lần, cả 10 đỏ.** Tám lần vào cổng điểm số:

| Bỏ đi | Điểm |
|---|---|
| không giao bản tin cho ứng dụng | 44 |
| tag không phải số nguyên có dấu vẫn là một trường | 53 |
| hai connection cùng danh tính đều được trả lời | 53 |
| `PossDup` thiếu `122=` không bị từ chối | 54 |
| `122=` lớn hơn `52=` không bị từ chối | 54 |
| ứng dụng im lặng vẫn tiêu một số | 54 |
| `35=8` được echo thay vì trả `35=j` | 54 |
| `97=Y` với `11=` đã thấy vẫn được trả lời | 54 |

Hai lần còn lại **xanh ở cổng điểm số và đó là điều đúng**: corpus không nhìn thấy chúng. Cả hai
được giữ bằng `crates/session/tests/application.rs`:

- **Đồng hồ trao cho ứng dụng bị dời 4 giây** — `52` là một trong năm tag `fields.fmt` so theo
  *hình dạng*, nên corpus chỉ chốt bề rộng, không chốt giá trị. Điểm vẫn 55.
- **Bỏ miễn trừ `122=` cho `SequenceReset`** — mọi `43=Y` `SequenceReset` trong corpus đều có
  `122=` sẵn. Điểm vẫn 55.

**"Một danh tính, một connection" là luật của engine, không phải của session.** `engine` chưa có,
nên `tests/score.rs` đóng vai engine nhỏ nhất giữ được hai connection. Ghi rõ ở đó, ở
`reference/quickfix-acceptance-def-format.md` và trong `STATUS.md`.

**Cấp phát:** `accept 0 refuse 0 tick 0 beat 0 answer 0 gap 0 fill 0 deliver 0 clock 0 text 0`.
Case `deliver` là mới — nó chạy cả echo thật (một `FieldIndex<256>` nữa, một
`TemplateBuilder<128, 4096>`, một lần encode) sau `received_with`. Đảo ngược (một `to_vec()`
trong `on_message`) cho `deliver 10000`.

**Cổng:** `fmt --check`, `clippy --all-targets -D warnings`, `test --all`,
`test --all --no-default-features`, `check-lint-config.sh`, `check-links.py` — tất cả rc=0.
Máy: Apple M5, macOS 26.6.2 (Darwin 25.6.0), cargo 1.95.0.

### Bước 6b — 2026-08-29 — **59 / 59**, đúng dự đoán sau khi Sửa 11 chia lại

Hai thay đổi độc lập, và chỉ một trong hai nằm trong `session`.

**Đóng khung, ở `tests/score.rs`.** `9=` được tin theo nghĩa đen: đếm tới cuối thân mà không gặp
`10=` thì **cả bộ đệm** là rác. `9=30` mất chính nó, `9=111` nuốt luôn bản tin sau. Rác vẫn được
đưa cho session đúng một lần, nên `1d_InvalidLogonLengthInvalid` vẫn rớt kết nối và luật "khung
hỏng chỉ chí mạng khi tự xưng là Logon" không bị chép ra hai chỗ. `2m` xanh → **56**.

**Kho và phát lại, trong `session`.** Vòng 8 chỗ × 512 byte trong `out::Outbound`, chỉ giữ bản
tin **ứng dụng**. Trả lời một `ResendRequest` là đi từ `7=` tới `16=` (0 nghĩa là "tới hết"):
số nào còn giữ thì phát lại, đoạn liền nhau nào không giữ được thì **một** gap fill phủ lên.
`8_AdminAndApplicationMessages` hỏi bốn lần với bốn dải khác nhau và cả bốn câu trả lời là bốn
cách xen kẽ khác nhau — đoạn phải *tìm ra*, không đoán được. Ba file cuối xanh → **59**.

Hình dạng một lần phát lại: nguyên bản cộng đúng hai trường — `43=Y`, và `122=` giữ cái `52=`
lần đầu gửi; `52=` được viết lại thành bây giờ. `9=132` so với `9=101` của bản gốc nói ra con số:
31 byte, đúng bằng `43=Y` và một `122=` 21 byte. **Phát lại không tiêu số nào**, nên nhịp tim kế
tiếp trong file vẫn là `34=10`.

**Một lỗi tìm ra bằng đảo ngược, không phải bằng đọc.** Đảo ngược "không phát lại, luôn lấp trống"
làm treo vô hạn: vòng quét đoạn dựa vào `kept()` đồng ý với `replay()`, mà bản đảo ngược làm hai
cái bất đồng. Đã sửa để đoạn luôn dài ít nhất một số — vòng không đứng yên được nữa dù hai hàm
có bất đồng.

**Đảo ngược: 12 lần, cả 12 đỏ.** Chín lần vào cổng điểm số:

| Bỏ đi | Điểm |
|---|---|
| khung hỏng không vứt cả bộ đệm | 56 |
| khung hỏng không đưa cho session | 58 |
| không phát lại, luôn lấp trống | 56 |
| phát lại tiêu một số thứ tự | 56 |
| phát lại không có `43=Y` | 56 |
| phát lại không mang `122=` | 56 |
| lấp trống cả dải thay vì từng đoạn | 58 |
| `36=` là số cuối đoạn chứ không phải một số sau nó | 56 |
| bản tin quản trị cũng được lưu | 56 |

Ba lần còn lại **xanh ở cổng điểm số**, và cả ba được giữ bằng `crates/session/tests/journal.rs`:
`52=` giữ nguyên thay vì làm mới (cùng bề rộng, so theo hình dạng); bỏ chốt độ dài khi lưu
(là `copy_from_slice` lệch độ dài — **panic**, và độ dài ấy do đối tác quyết vì nó theo độ dài
lệnh gửi vào); vòng không ghi đè cái cũ nhất.

**Cấp phát:** `accept 0 refuse 0 tick 0 beat 0 answer 0 gap 0 fill 0 deliver 0 resend 0 clock 0
text 0`. Case `resend` là mới; đảo ngược (một `to_vec()` trên đường phát lại) cho `resend 30000`.

**Cổng:** `fmt --check`, `clippy --all-targets --all-features -D warnings`, `test --all`,
`test --all --no-default-features`, `check-lint-config.sh`, `check-links.py`, `benches/alloc.rs`
— tất cả rc=0. Máy: Apple M5, macOS 26.6.2 (Darwin 25.6.0), cargo 1.95.0.

**Bất biến số 5, đi bộ bằng tay và bắt được một lỗi.** Bản `as_resend` đầu tiên chèn `43=Y` ngay
sau `34=` và `122=` ngay sau `56=` bằng tay — đúng chỗ, đúng kết quả, và **vi phạm luật**: thứ tự
trường phải đến từ bảng sinh chứ không từ call site. Đã viết lại: mọi trường đọc ra được đưa vào
một `TemplateBuilder` theo thứ tự nào cũng được, `Fix44` sắp. Điểm giữ nguyên 59, cấp phát giữ
nguyên 0, và ba lần đảo ngược về hình dạng phát lại vẫn đỏ.

**Chưa chứng minh:** journal nằm trong bộ nhớ, mất khi khởi động lại, và nằm sai crate —
`DESIGN.md` D1 nói engine giữ nó. Không có số nào đo trên Linux.
