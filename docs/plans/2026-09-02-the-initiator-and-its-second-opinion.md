# Initiator: cái để tự nói, và một ý kiến không phải của mình

> **Loại:** Plan · **Ngày:** 2026-09-02 · **Trạng thái:** Đã duyệt (nối tiếp phần đã duyệt 2026-08-30)
> **Phạm vi:** **Tiêu chí thoát số 4 của phase 1** — bước 3 và bước 4 của
> [session-initiator](2026-08-29-session-initiator.md), đang tạm dừng từ 2026-08-30

## Bối cảnh

`PRD.md` §2 nói phase 1 còn **đúng hai** tiêu chí chưa đạt, và **không cái nào là một quyết định
còn bỏ ngỏ**:

| Tiêu chí | Còn thiếu gì | Làm được ở phiên này không |
|---|---|---|
| **4** — initiator interop-green với `libquickfix` trong CI | một job CI, và cái để lái nó | **Được.** Đây là toàn bộ nội dung plan này |
| **6** — wire-to-wire trên máy đạt `DESIGN.md` §9 | **một cái máy** | **Không.** Xem *Ngoài phạm vi* |

Tiêu chí 4 dừng lại ngày 2026-08-30 ở *Sửa 2* của plan cũ, và dừng vì một **số đo**, không vì
hết giờ: bộ soi gương đòi đầu này **tự phát** 46 trên 50 file một bản tin mà không dòng nào trên
dây yêu cầu, và không đồng hồ nào sinh ra được. Session layer thuần thì không thể tự nghĩ ra một
`Logout`. Cái còn thiếu không phải máy trạng thái mà là **API cho người vận hành**.

Sửa 2 để lại ba việc đã biết trước khi quay lại. Plan này làm cả ba, **theo thứ tự đảo lại**, và
lý do đảo nằm ngay trong câu Sửa 2 viết:

> *"Soi gương là cách đọc riêng của dự án, đọc sai vẫn xanh."*

Cổng soi gương **không tự kiểm được cách đọc của chính nó**. Cổng interop thì kiểm được. Nên bước
làm trước là cái kiểm, không phải cái được kiểm: API → interop → soi gương. Nếu phiên này chỉ
xong được một cổng, cổng đáng có là cổng nói cho ta biết ta sai.

## Những gì đã biết chắc

- **Bước 1 và 2 của plan cũ đã xanh và đã merge.** `run_mirrored` + `mirrors()` có trong
  `crates/conformance/src/runner.rs`; `Config::initiator` và `connect()` phát Logon có trong
  `crates/session/src/lib.rs`. `crates/session/tests/mirror.rs` chốt **0 / 50**, và chốt luôn
  **45 file không rớt ở dòng mang Logon** — tức Logon của initiator đã đúng.
- **Mọi template cần cho việc tự phát đã dựng sẵn.** `crates/session/src/out.rs` có đủ bảy:
  `logon`, `logout`, `reject`, `heartbeat`, `test_request`, `resend_request`, `gap_fill`. Không
  phải thêm template nào; `test_request` và `resend_request` hôm nay **chưa có đường gọi nào tới**.
- **Ba trong sáu ý định đã có API rồi**: `send_sequence_reset(n)`, `begin_logout(text)`,
  `send_application(msg, journal)`. Còn thiếu đúng ba: heartbeat không ai hỏi,
  `TestRequest` với `112=` mình chọn, và `ResendRequest` với `7=`/`16=` mình chọn.
- **`send_as` là chỗ duy nhất viết `34=` và `52=`.** Mọi ý định mới đi qua nó, nên số thứ tự và
  đồng hồ vẫn thuộc về session chứ không thuộc về người gọi.
- **`libquickfix` build được trên máy này.** `[đo 2026-09-02]` `cmake 3.28.3`, `g++ 13.3.0`,
  cấu hình xong trong 2.4 s với `-DHAVE_SSL=OFF`. Đây là điều kiện tiên quyết của bước 4 và nó
  đã được thử **trước khi** plan này được viết, không phải giả định.
- **`vendor/` đã gitignore và script fetch đã ghim SHA** `386ce46e…`. Bản build C++ lấy đúng
  SHA ấy, và **không có gì của nó vào repo** — `CLAUDE.md` §2 luật 9.
- **CI đã có 4 job trên `ubuntu-latest`.** Thêm một job là thêm job.

## Cách làm

### 1. Ý định của người vận hành, không phải cửa sau gửi byte

Ba hàm mới trên `Session<R, N>`, cùng hình dạng với `send_sequence_reset` đã có:

| Hàm | Bản tin | Người gọi cấp | Session vẫn giữ |
|---|---|---|---|
| `send_heartbeat(emit)` | `35=0` | không gì | `34`, `52`, `49`, `56`, `8`, `9`, `10` |
| `send_test_request(id, emit)` | `35=1` | `112=` | như trên |
| `send_resend_request(from, to, emit)` | `35=2` | `7=`, `16=` | như trên |

**Không có hàm nào nhận nguyên byte một bản tin session.** Người gọi cấp *ý định*; session dựng
bản tin từ `Template` của nó. Đó là ranh giới làm cho cổng soi gương còn đo được cái gì đó: nếu
harness đọc thẳng dòng `I` rồi bơm ra dây thì cổng đo chính cái file nó đang đọc.

Cả ba trả `bool`: `false` và **không gửi gì** khi session chưa logon hoặc không dựng nổi bản tin —
đúng cách fail-closed của tầng này.

### 2. Cổng interop với `libquickfix` — cái kiểm cách đọc

Ba mảnh, không mảnh nào vào `Cargo.toml` của người dùng:

- **`tools/interop/src/main.rs`** — Rust thuần. Mở `TcpStream` tới acceptor, chạy
  `Session<Initiator, 256>` qua đó, và lái đúng kịch bản ADR-0004 nêu:
  `Logon → Heartbeat → TestRequest → ResendRequest → gap fill → Logout`. In từng bước ra stdout
  dạng máy đọc được; **đọc output chứ không đọc exit code**.
- **`tools/interop/acceptor.cpp`** — acceptor viết bằng `FIX::Application` của `libquickfix`.
  **Code của dự án này**, không chép của QuickFIX. Nó gửi hai bản tin ứng dụng rồi im, để bước
  `ResendRequest` có cái để đòi.
- **`scripts/interop.sh`** — clone `libquickfix` đúng SHA đã ghim, build, build acceptor, chạy
  nó, chạy driver Rust, **đọc stdout** và fail nếu thiếu một bước.

`build.rs` không đổi. `cargo test --all --no-default-features` trên máy không có CMake vẫn xanh —
`tools/interop` là một crate Rust bình thường, C++ chỉ do script gọi.

### 3. Hạ trần cổng soi gương 50 → 45, bằng một ADR

Năm file mà một engine đúng đắn **không gửi được**, đã đo và đã nêu tên trong
`crates/session/tests/mirror.rs` hôm nay. ADR mới nêu năm tên, lý do, và chốt trần là 45.

### 4. Harness đóng vai người vận hành, và **đếm mọi lần nó lái**

`Input::Originate(Intent)` thêm vào trait `SessionUnderTest`, **chỉ dùng ở chế độ soi gương**:

- Ở một dòng `Expect`, harness **tick trước** đúng như hôm nay. Cái gì máy trạng thái tự làm được
  thì vẫn tự làm.
- Chỉ khi tick xong vẫn im, harness mới suy ra `Intent` từ dòng `I` ấy và bảo session gửi.
- `Intent` **chỉ mang những trường người vận hành sở hữu** — `112`, `7`/`16`, `36`/`123`, `58`,
  và thân bản tin ứng dụng. `8`, `9`, `34`, `49`, `52`, `56`, `10` không bao giờ đi qua nó.
- `Report` đếm số lần lái, theo `MsgType`. `tests/mirror.rs` chốt bảng đếm ấy bằng số cụ thể,
  nên một lần lái mọc thêm là một test đỏ chứ không phải một điểm tăng.

### File sẽ tạo hoặc sửa

| File | Việc |
|---|---|
| `crates/session/src/lib.rs` | ba hàm ý định mới |
| `crates/session/tests/initiator.rs` | test cho ba hàm ấy + đảo ngược |
| `crates/session/benches/alloc.rs` | ba case mới, chứng minh bằng tiêm |
| `crates/conformance/src/runner.rs` | `Intent`, `Input::Originate`, đếm lần lái |
| `crates/session/tests/mirror.rs` | cổng 45 / 50 + bảng đếm |
| `tools/interop/` | crate mới: driver Rust + `acceptor.cpp` |
| `scripts/interop.sh` | build C++, chạy, **đọc output** |
| `.github/workflows/ci.yml` | job `interop` |
| `docs/decisions/ADR-00xx-…` | trần soi gương là 45 |
| `docs/decisions/ADR-00xx-…` | harness lái, và cái đó đo được gì |

## Bất biến bị đụng tới

| # | Đụng thế nào | Giữ bằng cách nào |
|---|---|---|
| 1 | Ba đường gửi mới | Ba case mới trong `benches/alloc.rs`, mỗi case tự chứng minh bằng tiêm một `to_vec()` |
| 2 | Session vẫn phải thuần | Ba hàm mới không có socket, không đồng hồ riêng, không `format!`. Thời gian vào bằng `tick` như cũ |
| 3 | Cổng 59/59 | Chạy `--test score` ở **mọi** bước, fixture không đụng |
| 5 | Ba bản tin mới | Dựng bằng `Template` đã có trong `out.rs`; không call site nào xếp trường |
| 6 | Job CI mới dùng CMake | Job riêng. `build.rs` không đổi, `tools/interop` là Rust thuần, `scripts/check-no-optional-deps.sh` chạy như cũ |
| 7 | Không `unwrap`/`expect`/`panic` trong crate thư viện | `tools/interop` là binary, không phải thư viện; `check-lint-config.sh` như cũ |
| 9 | Bước 4 build QuickFIX từ nguồn | Nguồn vào `vendor/`, đã gitignore. `git status` phải sạch sau khi chạy `scripts/interop.sh` |
| 10 | Không có số hiệu năng nào ở đây | Plan này không công bố con số nào |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | Ba hàm ý định + test + alloc | — |
| 2 | `tools/interop` driver + acceptor C++ + script, chạy xanh **trên máy này** | 1 |
| 3 | Job CI `interop`, xanh trên GitHub Actions | 2 |
| 4 | ADR trần 45; harness lái; cổng soi gương lên **45 / 50** | 1 |

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 1 | `cargo test -p fixbolt-session --test initiator` | Mỗi hàm ra **đúng một** bản tin, `34=` tăng đúng một, `52=` là của session |
| 1 | `cargo bench -p fixbolt-session --bench alloc` | Ba case mới đọc `0`; tiêm `to_vec()` phải thấy chúng đỏ |
| 2 | `bash scripts/interop.sh` | stdout có đủ sáu bước, **đọc dòng chứ không đọc rc** |
| 3 | job `interop` trên Actions | Xanh, và **nêu id run** theo `CLAUDE.md` §9 |
| 4 | `cargo test -p fixbolt-session --test mirror` | **45 / 50**, và bảng đếm lần lái đúng số |
| mọi bước | `cargo test -p fixbolt-session --test score` | Vẫn **59 / 59** |
| mọi bước | `cargo test --all`, `--no-default-features`, `clippy -D warnings`, `fmt --check`, `scripts/check-no-optional-deps.sh` | rc = 0 |

**Đảo ngược bắt buộc ở mỗi bước.** Đảo ngược nào xanh thì hoặc là chốt thừa, hoặc test viết hụt —
phải nói ra cái nào. `[2026-09-02]` và theo luật rút ra hôm nay: **commit trước khi đảo ngược**.

## Tài liệu phải cập nhật

- [ ] `PRD.md` §2 tiêu chí 4, và cây phase 1
- [ ] `DESIGN.md` §4 D1 — hình dạng thật của vai initiator; §6 — dòng gate 45/50 và gate interop
- [ ] `GUIDE.md` — ba hàm ý định là ràng buộc người dùng phải tự giữ (gọi khi chưa logon thì im)
- [ ] `STATUS.md` — tiêu chí 4, và mọi mục mở liên quan
- [ ] `CHANGELOG.md`
- [ ] ADR trần 45 · ADR harness lái
- [ ] `docs/reference/` — mọi bẫy đo được, và `[to testing-skills]` nếu là bài học về kiểm thử

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Harness lái quá tay, cổng đo chính file nó đọc | `Intent` không mang `34`/`52`/`49`/`56`; `Report` đếm lần lái và test chốt bảng đếm |
| Số lần lái âm thầm tăng ở lần sửa sau | Bảng đếm là số cụ thể trong assert, không phải `<=` |
| `112=` của mình bị lấy nhầm từ bản tin đối tác | `tests/initiator.rs` chọn một `112=` không có ở đâu khác, đảo ngược phải đỏ |
| Gọi hàm ý định khi chưa logon mà vẫn gửi | Test khẳng định **im lặng**, và đảo ngược (bỏ điều kiện state) phải đỏ |
| Acceptor C++ xanh vì nó dễ tính, không vì ta đúng | Kịch bản có `ResendRequest`, cái mà một Logon sai không đi tới được. Và **một lần chạy đối chứng**: hỏng cố ý một bước, script phải đỏ |
| `vendor/quickfix-src` lọt vào commit | `.gitignore`, và `git status --porcelain` phải rỗng sau `scripts/interop.sh` |
| Job CI xanh vì nó không chạy | Đọc log job, đếm đủ sáu dòng; không đọc dấu tick |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| **C++ vào CI**: CMake, pipeline chậm hơn, một job có thể đỏ vì lý do không phải Rust | Cao | ADR-0004 đã cân và đã duyệt 2026-08-30. Job để **riêng**, không chặn bốn job kia |
| Bước 4 (soi gương 45) không kịp trong phiên | Trung bình | Thứ tự đã đảo cho đúng lý do đó: bước 2–3 là tiêu chí thoát, bước 4 là cổng phụ. Thiếu thì **nói ra**, không im |
| `libquickfix` bản ghim không build trên `ubuntu-latest` của Actions dù build ở đây | Trung bình | Cùng distro, cùng cờ. Nếu đỏ thì đọc log, không đoán |

## Ngoài phạm vi

- **Tiêu chí thoát số 6.** Máy này là VM 4 vCPU dùng chung, có hypervisor, không core cô lập,
  không ghim tần số. `DESIGN.md` §9 không đạt được ở đây và **không con số nào từ đây đóng được
  tiêu chí 6**. Mục mở 6 vẫn mở sau plan này.
- **Reconnect, backoff, session schedule cho initiator.** Việc của `engine`, và bộ `.def` không
  phủ dòng nào.
- **Năm file soi gương đòi gửi bản tin sai có chủ đích.** ADR ở bước 4 nêu tên và đóng khung, chứ
  không tìm cách làm cho chúng xanh.

## Nhật ký giao hàng

*(điền khi đóng từng bước)*
