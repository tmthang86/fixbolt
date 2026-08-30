# Vai initiator cho session layer

> **Loại:** Plan · **Ngày:** 2026-08-29 · **Trạng thái:** Đã duyệt (2026-08-30, cả bốn bước)
> **Phạm vi:** M2 — bước 5 trong `DESIGN.md` §7, tiêu chí thoát số 4 của phase 1

## Bối cảnh

Acceptor đã đạt **59 / 59**. Vai còn lại — initiator, tức đầu **chủ động gọi ra** — mới là cái
làm `nanofixengine` dùng được cho một công ty vừa chạy acceptor cho khách vừa nối ra sàn, mà
theo [ADR-0004](../decisions/ADR-0004-bidirectional-engine.md) là hầu hết công ty chạy một
trong hai.

Hôm nay `Role::Initiator` tồn tại nhưng **là đồ trang trí**: `connect()` đọc hằng số
`SPEAKS_FIRST` rồi trả về `Link::Up` giống hệt acceptor, và comment trong code nói thẳng là
"bước 2 sẽ cho nó cái để nói". Không một dòng nào trong 59 file chạy qua nhánh ấy.

Bước này làm ba việc: cho initiator cái để nói, dựng cổng đo nó, và **đo nó bằng một ý kiến
không phải của mình** — QuickFIX C++ thật, chạy như acceptor, trong CI.

## Những gì đã biết chắc

- **51 trong 59 file soi gương được.** [ADR-0004](../decisions/ADR-0004-bidirectional-engine.md)
  quyết định 6 nêu tiêu chí máy làm được: một file soi gương được khi **mọi** dòng `I` của nó là
  thứ một initiator đúng đắn thật sự sẽ gửi — bắt đầu bằng `8=FIX.4.4`, mọi tag là số, không
  trường nào rỗng. Tám file trượt là tám file có mục đích duy nhất là để acceptor từ chối rác:
  `14a_BadField`, `2d_GarbledMessage`, `3c_GarbledMessage` (tag không phải số),
  `14d_TagSpecifiedWithoutValue`, `ReverseRouteWithEmptyRoutingTags` (trường rỗng),
  `1d_InvalidLogonWrongBeginString`, `2i_BeginStringValueUnexpected` (`BeginString` khác),
  `2t_FirstThreeFieldsOutOfOrder` (`35=` trước `8=`).
- **Runner đã có sẵn chỗ để đảo.** `Kind::Send` và `Kind::Expect` là hai nhánh của cùng một
  enum trong `crates/conformance/src/script.rs`; `run_scenario` đọc chúng theo thứ tự file. Đảo
  vai là đọc `E` như input và `I` như output mong đợi.
- **~90% máy trạng thái là chung.** Đếm số thứ tự, gap, resend, heartbeat, test request,
  logout, `PossDup`, kho bản tin gửi ra — tất cả đã viết một lần và đã xanh 59/59.
- **CI đã chạy trên `ubuntu-latest`**, bốn job trong `.github/workflows/ci.yml`. Thêm một job
  build `libquickfix` là thêm job, không phải dựng hạ tầng mới.
- **Bất biến 6 cấm toolchain ngoài trong `build.rs`.** ADR-0004 quyết định 7 nói rõ hơn: C++
  chỉ được sống trong CI và `tools/`, không bao giờ vào `Cargo.toml` hay máy người dùng.

## Cách làm

### 1. Chế độ soi gương trong `conformance`

Thêm `runner::run_mirrored`, dùng lại toàn bộ `run_scenario` bằng cách **đảo file trước khi
chạy**, không phải bằng cách viết một runner thứ hai:

- `Kind::Send(m)` ↔ `Kind::Expect(m)`
- `Kind::Connect` giữ nguyên (initiator cũng bắt đầu bằng một kết nối)
- `Kind::Disconnect` ↔ `Kind::ExpectDisconnect`

Tiêu chí loại trừ **được tính ra, không chép tay**: một hàm `mirrors(&Scenario) -> bool` áp đúng
ba điều kiện ADR-0004 nêu, và một test khẳng định tập nó loại đúng bằng tám tên trên. Chép tay
tám tên vào một mảng là để một thay đổi corpus im lặng đi qua.

**Vai của CompID cũng đảo.** File viết từ góc nhìn acceptor `ISLD`; initiator của ta là `TW44`.

### 2. Cho initiator cái để nói

`Config::initiator(...)` và `connect()` phát `Logon` — `98=0`, `108=<HeartBtInt cấu hình>`, và
`141=Y` khi được yêu cầu. Đây là chỗ ~10% bất đối xứng nằm:

| Việc | Acceptor | Initiator |
|---|---|---|
| Logon | trả lời, ném lại `108` của đối tác | **gửi trước**, `108` là của mình |
| `ResetSeqNumFlag` | nghe theo | **quyết định** rồi báo |
| Logon không được trả lời | không có tình huống | timeout → rớt kết nối |

### 3. Chạy tới 50 / 50

Cổng: `cargo test -p nanofix-session --test mirror`. Mỗi lần tăng điểm phải đi kèm một lần đảo
ngược đỏ, như năm bước trước.

### 4. Interop với `libquickfix` trong CI

Một job mới trên `ubuntu-latest`: build `libquickfix` từ nguồn (đã có
`scripts/fetch-quickfix-assets.sh` để lấy về), chạy nó làm acceptor, và lái initiator của ta
qua logon → heartbeat → `TestRequest` → `ResendRequest` → gap fill → logout. Kịch bản là một
binary trong `tools/`, **không phải một test của crate nào**, để bất biến 6 và ADR-0004 quyết
định 7 vẫn đúng: `cargo test --no-default-features` trên một máy không có CMake vẫn phải xanh.

**Đã duyệt 2026-08-30**, cùng với chi phí duy trì mà Rủi ro nêu ra.

### File sẽ tạo hoặc sửa

| File | Việc |
|---|---|
| `crates/conformance/src/runner.rs` | `run_mirrored`, `mirrors()` |
| `crates/conformance/tests/mirror.rs` | tám file bị loại đúng là tám file ADR-0004 nêu |
| `crates/session/src/lib.rs` | `connect()` phát Logon khi `SPEAKS_FIRST`; timeout Logon |
| `crates/session/src/out.rs` | Logon của initiator có slot `141` |
| `crates/session/tests/mirror.rs` | cổng 50 / 50 |
| `crates/session/tests/initiator.rs` | cái corpus soi gương không thấy |
| `tools/interop/` | kịch bản chạy với `libquickfix` (bước 4) |
| `.github/workflows/ci.yml` | job interop (bước 4) |

## Bất biến bị đụng tới

| # | Đụng thế nào | Giữ bằng cách nào |
|---|---|---|
| 1 | Logon do initiator phát là một đường gửi mới | Thêm case `logon_out` vào `benches/alloc.rs`, chứng minh bằng tiêm một `to_vec()` |
| 2 | `connect()` giờ phát bản tin | Vẫn không socket, không đồng hồ riêng: thời gian vào bằng `tick` như cũ, `emit` là closure của người gọi |
| 3 | Cổng 59/59 không được rớt | Chạy cả hai cổng ở mọi bước: `--test score` **và** `--test mirror` |
| 5 | Logon của initiator là bản tin mới | Dựng bằng `Template` như sáu bản tin kia, không xếp tay |
| 6 | Job CI mới dùng CMake | Job riêng; `build.rs` không đổi; job `no-default-features` vẫn chạy trên máy không có gì |
| 7 | Không `unwrap`/`expect`/`panic` | `check-lint-config.sh` như cũ |
| 9 | Job interop build QuickFIX từ nguồn | Nguồn ấy **không** vào repo. `vendor/` vẫn gitignore |

## Chia việc

| Bước | Kết quả | Phụ thuộc | Điểm dự đoán |
|---|---|---|---|
| 1 | `run_mirrored` + `mirrors()`, chín file bị loại đúng tên | — | 0 / 50 |
| 2 | Initiator phát Logon; `141=Y` | 1 | ~30 / 50 |
| 3 | Phần bất đối xứng còn lại + đảo ngược | 2 | **50 / 50** |
| 4 | Job interop `libquickfix` | 3 | tiêu chí 4 của phase 1 |

Dự đoán bước 2 là **ước**, không phải đo — mọi file soi gương đều bắt đầu bằng một Logon, nên
chỉ riêng Logon đã mở được phần lớn; con số thật sẽ ghi vào nhật ký giao hàng.

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 1 | `cargo test -p nanofix-conformance --test mirror` | Chín file bị loại đúng chín tên ADR-0004 + ADR-0006 nêu, **tính ra chứ không chép** |
| 2–3 | `cargo test -p nanofix-session --test mirror` | In đúng điểm dự đoán. **Cao hơn cũng phải dừng giải thích** |
| mọi bước | `cargo test -p nanofix-session --test score` | Vẫn **59 / 59** — không được lùi một file nào |
| mọi bước | `cargo bench -p nanofix-session --bench alloc` | Thêm `logon_out 0`, và tiêm một `to_vec()` phải thấy `logon_out 10000` |
| 4 | job CI `interop` | Logon → heartbeat → `TestRequest` → `ResendRequest` → gap fill → logout, đọc **output** chứ không đọc exit code |
| mọi bước | `cargo test --all`, `--no-default-features`, `clippy -D warnings`, `fmt --check` | rc = 0 |

**Đảo ngược bắt buộc ở mỗi bước**, như năm bước trước: phá một thứ, thấy đúng những file vừa làm
xanh chuyển đỏ. Đảo ngược nào xanh thì hoặc là chốt thừa, hoặc là test viết hụt — phải nói ra
cái nào.

## Tài liệu phải cập nhật

- [ ] `DESIGN.md` §4 D1 — hình dạng thật của vai initiator sau khi viết
- [ ] `DESIGN.md` §6 — thêm dòng gate 50 / 50 soi gương
- [ ] `docs/reference/quickfix-acceptance-def-format.md` — luật soi gương và mọi bẫy đo được
- [ ] `PRD.md` §2 tiêu chí 4; `STATUS.md`; `CHANGELOG.md`
- [ ] ADR mới nếu phần bất đối xứng hoá ra lớn hơn ~10% ADR-0004 dự đoán

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Soi gương xanh vì **cách đọc của chính mình** sai, không phải vì code đúng | Job interop bước 4 — đó là lý do nó tồn tại |
| Danh sách chín file bị loại chép tay, corpus đổi mà không ai biết | `tests/mirror.rs`: tập tính ra phải bằng tập tên |
| Logon của initiator xếp trường bằng tay | Dựng bằng `Template`, và comparator vị trí bắt |
| `108` lấy nhầm của đối tác thay vì của mình | `tests/initiator.rs`, và đảo ngược phải đỏ |
| `connect()` phát Logon rồi `tick` phát thêm heartbeat ngay | `tests/initiator.rs`: một `connect` ra đúng một bản tin |
| Sửa fixture cho code mới chạy được | 59/59 chạy lại ở mọi bước, fixture không đụng |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| **Bước 4 kéo theo C++ vào CI** — CMake, C++17, pipeline chậm hơn, một job sẽ đỏ vì lý do không liên quan tới Rust nào trong repo | Cao | ADR-0004 đã cân nhắc và chấp nhận. **Nhưng đây là mục cần chủ dự án gật đầu lại**, vì nó là chi phí duy trì dài hạn chứ không phải một lần |
| Soi gương là cách đọc riêng của dự án, đọc sai vẫn xanh | Cao | Đúng cái bước 4 chặn. Nếu bước 4 hoãn thì rủi ro này **không được che**, phải ghi vào `STATUS.md` |
| Reconnect, backoff, session schedule: **0 / 59 file phủ** | Cao | Ngoài phạm vi bước này, ghi rõ dưới đây. ADR-0004 đã nêu là nợ |
| Bước 2 vượt dự đoán vì phân loại sai | Trung bình | Dừng, giải thích, sửa plan — như Sửa 10 và Sửa 11 ở plan trước |

## Ngoài phạm vi

- **Reconnect và backoff.** Không file nào phủ, và nó là việc của `engine`, không của máy trạng
  thái thuần.
- **Session schedule** (giờ mở, giờ đóng, luật ngày trong tuần, reset số thứ tự theo lịch).
  QuickFIX có hai mươi năm hành vi tích luỹ ở đây và bộ `.def` không kiểm cái nào.
- **Sequence persistence qua lần khởi động lại.** Journal hôm nay nằm trong bộ nhớ và nằm sai
  crate; cả hai thuộc plan của `engine`.
- **`tools/w2w`** và mọi con số Linux. Bước 7 trong `DESIGN.md` §7.

## Sửa plan giữa chừng

### Sửa 1 — soi gương được là **50**, không phải 51

**Phát hiện ngay ở bước 1**, khi bản `Replay` trả lời đúng từng dòng vẫn chỉ được 50/51.

Tiêu chí của ADR-0004 quyết định 6 chỉ nhìn **dòng bản tin**. Một file `.def` còn có **chỉ
thị**, và một chỉ thị soi gương ra thứ không initiator nào làm: `1b_DuplicateIdentity.def` kết
thúc bằng `i1,DISCONNECT` — soi gương thành `e1,DISCONNECT`, tức **máy này** phải tự cúp kết nối
1 trong khi không có gì trên dây bảo nó cúp. Ở bản gốc đó là harness dọn dẹp, không phải luật
giao thức.

`[đo 2026-08-30]` `1b` là file **duy nhất** trong 59 file có `iDISCONNECT`.

Đã ghi thành [ADR-0006](../decisions/ADR-0006-mirrored-corpus-is-fifty.md), thay thế **riêng**
quyết định 6 của ADR-0004. Mọi cổng trong plan này đổi 51 → **50**.

### Sửa 2 — **cần quyết định**: soi gương đo được ít hơn plan tưởng, và ở chỗ nào

**Phát hiện ở cuối bước 2, bằng số đo chứ không bằng đọc.** Logon của initiator đã đúng —
45 trong 50 file không rớt ở dòng mang nó. Nhưng điểm là **0 / 50**, và hai số đo dưới đây nói
vì sao:

**Số đo 1 — 46 / 50 file đòi đầu này *tự phát* một bản tin mà máy trạng thái không thể tự nghĩ ra:**

| Phải tự phát | Số file |
|---|---|
| `5` Logout | 42 |
| `D` / `d` / `8` bản tin ứng dụng | 19 |
| `0` Heartbeat, không ai hỏi | 14 |
| `1` TestRequest, với `112=` cho sẵn | 13 |
| `4` SequenceReset | 6 |
| `2` ResendRequest | 4 |

Không dòng nào trên dây yêu cầu chúng, và không đồng hồ nào sinh ra một Logout. Muốn chúng ra
được thì **harness phải đóng vai người vận hành** và bảo session "gửi cái này, bây giờ". Chỗ nào
harness lái thì chỗ đó cổng đo **đánh số và đóng khung**, không đo quyết định giao thức.

**Số đo 2 — 5 / 50 file đòi đầu này gửi một bản tin *sai có chủ đích*:**
`1c_InvalidSenderCompID`, `1c_InvalidTargetCompID`, `1d_InvalidLogonBadSendingTime`,
`1d_InvalidLogonLengthInvalid`, `1e_NotLogonMessage`. CompID không khớp, `SendingTime` lệch 2001
năm, `9=` thiếu 23 byte, và một cái không phải Logon. Một engine đúng đắn **không gửi được**
chúng — mà `sendable()` cũng không thấy được, vì cả năm cái đều đúng cú pháp hoàn hảo. Đó chính
là điều khiến chúng đáng chú ý: tiêu chí của ADR-0004 là **cú pháp**, còn cái sai trong corpus
phần lớn là **ngữ nghĩa**.

Trần thật của cổng soi gương vì thế là **45**, không phải 50 — và 45 ấy chỉ đạt được nếu harness
lái mọi lần tự phát.

**Chờ chủ dự án quyết**, xem tin nhắn kèm theo. Bước 3 chưa bắt đầu.

## Nhật ký giao hàng

*(trống — plan chưa được duyệt)*
