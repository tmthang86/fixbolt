# Initiator nối lại được sau khi rớt

> **Loại:** Plan · **Ngày:** 2026-09-02 · **Trạng thái:** Đã duyệt (chủ dự án yêu cầu làm hết
> những item làm được trên máy này, 2026-09-02)
> **Phạm vi:** `STATUS.md` mục mở **35**. Không đụng tiêu chí thoát nào của phase 1 —
> tiêu chí 4 đã đạt, tiêu chí 6 cần phần cứng.

## Bối cảnh

`fixbolt_engine::connect(addr)` là **toàn bộ** mặt initiator hôm nay: một `TcpStream::connect`,
một `TcpTransport`. Rớt kết nối thì hết.

Không phải là chưa ai nghĩ tới — là **chưa cổng nào phủ**:

| Cổng | Vì sao không thấy |
|---|---|
| 59 file `.def` | viết cho acceptor, và không file nào nối lại một initiator |
| Bộ soi gương | đang 2 / 50 |
| `scripts/interop.sh` | nối **một lần**, rồi logout |

Nên đây là một khoảng trống có thật, và nó là khoảng trống của `engine`, không phải của
`session`: máy trạng thái thuần không có socket để mở lại.

## Những gì đã biết chắc

- **`Session::resume(cfg, next_out, next_in)` đã có**, và `connect()` **không** reset số thứ tự
  của một session đã resume — [ADR-0010](../decisions/ADR-0010-a-reconnect-is-not-a-restart.md),
  đúng chỗ luật này cần. Việc còn thiếu nằm ở tầng trên: ai mở lại socket, và khi nào.
- **`Schedule` đã có** trong `fixbolt_session::schedule`, UTC, với `contains(ms)` —
  [ADR-0033](../decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md).
  Một initiator không nên gọi ra ngoài giờ giao dịch, và câu trả lời đó đã tồn tại.
- **`clock::ManualClock` đã có** trong `fixbolt-engine`, nên chính sách chờ kiểm được **không
  cần đồng hồ thật** — không test nào phải `sleep`.
- **Bất biến 4 cấm ngủ trong kernel trên engine thread ở `hft`.** Nên chính sách **trả về một
  mốc thời gian**, không tự ngủ; vòng lặp dùng `Waiting` như hiện tại.
- **`Config::initiator` + `connect()` + `tick()` phát Logon** đã xanh và có
  `crates/session/tests/initiator.rs` chốt.

## Cách làm

### 1. `engine::reconnect::Policy` — thuần, không socket, không đồng hồ riêng

```rust
pub enum Next {
    /// Mở kết nối ngay.
    Now,
    /// Chưa tới lúc; chờ tới mốc này (thang của Tick).
    At(u64),
    /// Đừng nối lại nữa — người gọi đã bảo dừng.
    Stop,
}

impl Policy {
    pub fn new(first_ms: u64, ceiling_ms: u64) -> Result<Self, PolicyError>;
    pub fn with_schedule(self, s: Schedule) -> Self;
    /// Kết nối vừa rớt lúc `now_ms`.
    pub fn dropped(&mut self, now_ms: u64);
    /// Session vừa logon xong — thang backoff về lại `first_ms`.
    pub fn logged_on(&mut self);
    pub fn stop(&mut self);
    /// Lần thử tiếp theo, tính từ `now_ms`.
    pub fn next(&self, now_ms: u64) -> Next;
}
```

**Nhân đôi có trần, không jitter.** `first_ms`, `2×`, `4×`… tới `ceiling_ms` rồi dừng ở đó.
**Không có số ngẫu nhiên** — `codec` có luật zero-dependency và `engine` chưa có RNG nào; thêm
một cái để rải tải là một quyết định cần ADR riêng, và cái giá của việc không có nó
(nhiều initiator cùng nối lại cùng lúc sau một sự cố chung) **ghi vào ADR chứ không giấu**.

**Lịch chặn trước backoff.** Ngoài giờ thì **không** trả `Now` — nối ra một sàn đang đóng là
một kết nối bị từ chối, và một chuỗi backoff bắt đầu vì lý do sai.

`[sửa plan trước khi viết dòng code đầu tiên, 2026-09-02]` bản đầu của mục này viết
*"trả `At(mốc mở cửa)`"*. **`Schedule` không có hàm ấy** — nó có `contains(t)` và
`session_start(t)`, cả hai trả lời về một thời điểm cho trước, không cái nào tính được lần mở
cửa **tiếp theo**. Thêm `next_open` là sửa API của `session`, cần lý do riêng, và plan này nói
rõ là chỉ đụng `engine`.

Nên: ngoài giờ trả `At(now + ceiling_ms)` — **hỏi lại sau**, chứ không giả vờ biết một mốc nó
không tính được. Với trần 30 giây thì đó là hỏi lại hai lần một phút trong lúc sàn đóng, không
tốn gì. Ghi ở đây chứ không để người đọc code tự đoán.

### 2. `connect_and_serve` — vòng lặp initiator

Cùng hình dạng `serve`, khác ba chỗ: mở kết nối thay vì nhận, một session thay vì N, và khi rớt
thì hỏi `Policy` chứ không kết thúc.

**Số thứ tự đi qua lần nối lại** — ADR-0010. Vòng lặp giữ `next_out`/`next_in` của session vừa
rớt và dựng session mới bằng `Session::resume`, chứ không `Session::new`.

### 3. Cổng qua socket thật

`crates/engine/tests/reconnect_wire.rs`: dựng một acceptor thật, để initiator logon, **đóng
acceptor**, và khẳng định initiator nối lại — với số thứ tự **tiếp tục**, không quay về 1.

### File sẽ tạo hoặc sửa

| File | Việc |
|---|---|
| `crates/engine/src/reconnect.rs` | mới: `Policy`, `Next`, `PolicyError` |
| `crates/engine/src/lib.rs` | `mod reconnect;` + `connect_and_serve` |
| `crates/engine/tests/reconnect.rs` | mới: chính sách thuần, đồng hồ tay |
| `crates/engine/tests/reconnect_wire.rs` | mới: qua socket thật |
| `crates/engine/benches/alloc.rs` | case mới: đường `dropped` + `next` không cấp phát |
| `docs/decisions/ADR-00xx` | backoff không jitter, và cái giá của nó |
| `DESIGN.md` §6 · `GUIDE.md` · `PRD.md` · `STATUS.md` · `CHANGELOG.md` | theo bảng §4 |

## Bất biến bị đụng tới

| # | Đụng thế nào | Giữ bằng cách nào |
|---|---|---|
| 1 | `Policy::next` chạy mỗi vòng khi chưa nối | Case mới trong `benches/alloc.rs`, chứng minh bằng tiêm |
| 2 | Không đụng `session` | `reconnect` nằm trong `engine`. `session` không đổi một dòng |
| 3 | Cổng 59/59 | Chạy `--test score` và `--test wire` ở mọi bước |
| 4 | **Chờ giữa hai lần thử** | `Policy` **trả về mốc**, không ngủ. `check-no-kernel-sleep.sh` chạy lại |
| 6 | `connect_and_serve` dùng `Block` như `serve` | `#[cfg(feature = "standard")]` **trên item**, không chỉ trong `Cargo.toml` |
| 7 | Không `unwrap`/`expect`/`panic` | `PolicyError` là enum không trường |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | `Policy` + test thuần (đồng hồ tay) + alloc | — |
| 2 | `connect_and_serve`, số thứ tự đi qua lần nối lại | 1 |
| 3 | Cổng qua socket thật | 2 |
| 4 | ADR + đồng bộ tài liệu | 1–3 |

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 1 | `cargo test -p fixbolt-engine --test reconnect` | Thang nhân đôi dừng đúng ở trần; `logged_on` đặt lại; lịch chặn trước backoff |
| 1 | `cargo bench -p fixbolt-engine --bench alloc` | Case `reconnect` đọc `0`; tiêm `to_vec()` phải thấy nó đỏ |
| 2–3 | `cargo test -p fixbolt-engine --test reconnect_wire` | Acceptor biến mất → initiator nối lại; `34=` **không** về 1 |
| mọi bước | `--test score`, `--test wire` | vẫn 59 / 59 |
| mọi bước | `cargo test --all`, `--no-default-features`, `clippy -D warnings`, `fmt --check` | rc = 0 |
| mọi bước | `scripts/check-no-kernel-sleep.sh` | Không syscall ngủ nào mới trên engine thread |

**Đảo ngược bắt buộc ở mỗi bước.** Và theo bài học vừa trả giá hôm nay: **commit xong rồi mới
đảo ngược**, vì `git restore` khi ấy chỉ xoá được thí nghiệm.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Nối lại reset số thứ tự về 1 — ADR-0010 nói rõ là sai | `reconnect_wire.rs` chốt `34=` sau khi nối lại |
| Backoff không trần → im lặng hàng giờ | Test chốt thang dừng đúng ở `ceiling_ms` |
| Chờ bằng `thread::sleep` trên engine thread | `Policy` trả mốc; `check-no-kernel-sleep.sh` |
| Nối ra ngoài giờ rồi bị từ chối, khởi động một chuỗi backoff sai nguyên nhân | Test: ngoài giờ trả `At(giờ mở)`, không phải `Now` |
| **Mọi test ở đây là tự nghĩ ra** — không file `.def` nào phủ | Nói thẳng trong plan và trong `DESIGN.md` §6, như dòng `field_types.rs` đã làm |
| Test chờ bằng đồng hồ thật rồi chậm/flaky | `ManualClock` ở bước 1; bước 3 dùng mốc, không `sleep` |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Không jitter → nhiều initiator nối lại đồng loạt sau sự cố chung | Trung bình | **Chấp nhận và ghi vào ADR.** Thêm RNG là quyết định riêng |
| `connect_and_serve` phình thành một bản sao của `serve` | Trung bình | Một session, một socket. Sharding và registry **ngoài phạm vi** |
| Cổng bước 3 xanh vì acceptor giả dễ tính | Cao | Đảo ngược: bỏ `Policy` khỏi vòng lặp thì test phải đỏ |

## Ngoài phạm vi

- **Nhiều initiator trong một tiến trình**, sharding cho initiator, registry cho chiều ra.
- **Jitter / rải tải.** Nêu trong ADR là nợ, không làm.
- **Lịch tự khởi động một phiên mới** (reset số thứ tự theo ngày giao dịch). `Schedule` đã làm
  việc đó ở tầng session; nối nó vào vòng lặp initiator là việc khác.
- **Tiêu chí thoát 6.** Không con số nào từ máy này.

## Nhật ký giao hàng

*(điền khi đóng từng bước)*
