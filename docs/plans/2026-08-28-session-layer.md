# Máy trạng thái session FIX 4.4 — từ 0/59 lên 59/59

> **Loại:** Plan · **Ngày:** 2026-08-28 · **Trạng thái:** Chờ duyệt
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

## Nhật ký giao hàng

*(chưa bắt đầu)*
