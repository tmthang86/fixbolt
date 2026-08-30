# `standard` mode — engine chặn khi rỗi và trả core lại

> **Loại:** Plan · **Ngày:** 2026-08-30 · **Trạng thái:** **ĐÓNG 2026-08-30**, với đúng một việc
> để lại và nói rõ vì sao — xem nhật ký giao hàng, mục *bước 8*
> **Phạm vi:** `engine` — public API, vòng lặp rỗi, `Transport`, `Waiting`. Không đụng `codec`,
> không đụng `session`.

## Bối cảnh

[ADR-0013](../decisions/ADR-0013-two-modes-standard-and-hft.md) đã được ký 2026-08-30: engine có
**hai mode**. `hft` quay vòng, ghim core, mua lấy microsecond. `standard` chặn khi rỗi, chạy được
mọi nơi, **và là mặc định** — cái người ta nhận được khi không nói gì.

**`standard` chưa tồn tại.** Hôm nay engine quay vòng, luôn luôn. Nên tình trạng hiện tại là:
mặc định đã ghi trong tài liệu nhưng chưa có code. [GUIDE.md](../GUIDE.md) §0 đang phải in ra câu
đó cho người đọc — *"`[2026-08-30]` `standard` is not built yet"* — và
[DESIGN.md](../DESIGN.md) D8 cũng vậy. Ai thử engine ngay lúc này sẽ thấy một tiến trình đốt 100%
một core và tưởng nó hỏng.

Ba thứ nữa treo theo nó, và cả ba đều nằm trong ADR-0013:

1. **Nửa `standard` của nguyên tắc 4 không có cổng máy nào canh.** `CLAUDE.md` §2 rule 4 giờ có
   hai nửa và tự nói ra rằng nửa thứ hai chưa được kiểm. `scripts/check-no-kernel-sleep.sh` chỉ
   chứng minh nửa `hft`.
2. **`wait::Park` không thuộc mode nào** — nó là `std::thread::yield_now()`, nhường scheduler mà
   **không chặn**, nên vẫn đốt core. Và **mọi test trong repo đều dùng nó**.
3. **`std` không có API readiness nào.** Chặn cho tới khi socket có dữ liệu cần `poll`, `epoll`,
   `kqueue` hay IOCP — `std` không phơi cái nào. Mà `engine` hôm nay **không có dependency ngoài**.

Plan này làm cả ba, theo đúng thứ tự đó.

**Điều plan này KHÔNG hứa:** `standard` không nhanh hơn `hft`. Nó chậm hơn — đó là cả điểm của
nó. Cái nó mua là engine chạy được trên máy chia sẻ, trong container, trên laptop, không cần
`isolcpus`, và **trả core lại**.

## Những gì đã biết chắc

Đọc từ code, từ ADR đã ký, và từ số đã đo. Không có phỏng đoán ở mục này.

| Sự thật | Nguồn |
|---|---|
| `Waiting::idle(&mut self)` **không nhìn thấy socket nào** — nó không có tham số | `crates/engine/src/wait.rs` |
| `Spin::idle` = `core::hint::spin_loop()`; `Park::idle` = `std::thread::yield_now()` | như trên |
| `Park` nhường scheduler và **không chặn**, nên vẫn đốt core nó đang ở | ADR-0013, "ba sự thật quyết định hình dạng", điểm 2 |
| `std` chỉ cho `set_nonblocking` và `WouldBlock`; **không có API readiness** | ADR-0013, điểm 3 |
| `Transport` chỉ có `recv` và `send`; `TcpTransport` giữ một `TcpStream` và có `socket()` | `crates/engine/src/transport.rs` |
| `Connection::has_pending_output()` đã có sẵn, đọc `tx_len > 0` | `crates/engine/src/conn.rs:160` |
| `Acceptor` tách rời `Engine`; `serve()` và `w2w::pump` **tự ghép** accept + `turn()` + `idle()` | `crates/engine/src/lib.rs`, `tools/w2w/src/main.rs` |
| `Dispatch::OUT_OF_BAND`; `RingDispatch` đặt nó `true`, `InlineDispatch` đặt `false`; `turn()` nhặt kết quả qua `collect` | `crates/engine/src/dispatch.rs:43,86,158` |
| `engine` **không có dependency ngoài nào** | `crates/engine/Cargo.toml` |
| Trong `crates/engine/src/` **không có `cfg(target_os)` / `cfg(unix)` / `cfg(windows)` nào** | ADR-0013, điểm 1 |
| `tools/w2w` in `engine-tid:` trên Linux, đọc từ `/proc/thread-self` | `tools/w2w/src/main.rs` |
| `check-no-kernel-sleep.sh` quy syscall **theo tid**, chạy binary **hai lần** và **bắt lần thứ hai (`--park`) phải trượt** | đọc script |
| Danh sách `SLEEPERS` của script đó gồm `poll`, `ppoll`, `epoll_wait`, `select`, `futex`, `sched_yield` — **đúng những thứ `standard` phải gọi** | như trên |
| Mọi chỗ dùng `Park` (`wire.rs`, `dispatch.rs`, `alloc.rs`, `transport.rs`, `w2w`) đều **tự lái `turn()`** | `grep -rn 'Park' crates/ tools/` |
| Cổng 59 qua socket: **59 / 59 trên cả M5 lẫn Linux**, sau khi client được `TCP_NODELAY` | `DESIGN.md` §6 |
| `check-machine.sh` có dòng **`machine is quiet`**, FAIL khi CPU bận quá 3% trong một giây | `scripts/check-machine.sh:191,268` |
| Một lượt quét rỗi ở `hft` = `N × 703 ns`, phẳng từ N=1 tới N=256 | `[đo 2026-08-30]` [measured-costs.md](../reference/measured-costs.md) |
| Con số 2–5 µs cho wakeup kiểu `epoll` là **lấy từ tài liệu, chưa đo ở đây** | `DESIGN.md` §8, và §8 tự nói vậy |
| `RingDispatch` một chiều: **128.0 ns**, khứ hồi **242.5 ns** (M5, chưa ghim) | `DESIGN.md` §6 |
| Job CI `no-default-features` chạy trên runner riêng, `cargo test --all --no-default-features` | `.github/workflows/ci.yml` |

## Cách làm

Bảy quyết định. Cái nào là quyết định kiến trúc thì bước 1 (ADR) chốt, không phải chỗ này —
nhưng phương án được chọn thì ghi ở đây để người duyệt thấy mình đang duyệt cái gì.

### 1. Cơ chế: `poll(2)` qua `libc`, sau feature `standard`, bật mặc định

Đây là câu hỏi mở số 1 của ADR-0013 và nó phải trả lời trước mọi thứ khác.

`poll(2)` là thứ nhỏ nhất chạy được trên Linux và macOS, có sẵn trong `libc`, không kéo theo
async runtime nào — mà một dependency kéo async runtime thì `CLAUDE.md` §6 bắt phải có ADR riêng.
`libc` **cũng chính là dependency mà plan [threads-and-affinity](2026-08-30-threads-and-affinity.md)
đã lấy** cho `sched_setaffinity`, nên đây không phải một cây dependency thứ hai, mà là cùng một cây.

Không chọn `polling` hay `mio`: chúng mua Windows, mà **Windows chưa bao giờ được quyết định là có
trong phạm vi hay không** — và mua một nền tảng chưa quyết là mua một thứ không ai bảo trì.

Không chọn `epoll` ngay: `poll(2)` là O(N) trên số socket, `epoll` là O(1). Ở hình dạng mà
`standard` phục vụ — hàng chục tới hàng trăm session trên một thread — khác biệt đó **chưa đo**, và
đổi cơ chế vì lý do chưa đo là đúng thứ `CLAUDE.md` §2 rule 10 cấm. `epoll` là một ADR sau, kèm số.

**Feature `standard` bật mặc định.** Kết quả phụ, và nó tốt: `--no-default-features` cho ra một
engine **không dependency nào**, chỉ có `hft`. Job CI đã có sẵn canh chuyện đó.

**Windows: ngoài phạm vi, và trả lỗi có kiểu chứ không âm thầm quay về spin.** Một engine im lặng
quay về spin trên nền tảng nó không hỗ trợ chính là thứ ADR-0013 sinh ra để tránh.

### 2. Chỗ nối: `Waiting` phải nhìn thấy được các nguồn

ADR-0013 quyết định 2 nói chặn-trên-readiness là chuyện của `Transport` chứ không của `Waiting`,
**vì kẻ chờ phải biết các socket**. Nhưng vòng lặp rỗi thì vẫn là `Waiting`. Cách nối hai cái:
cho `Waiting` **nhìn thấy** các nguồn, và cho `Transport` **nói ra** nguồn của mình.

Phác thảo, bước 1 chốt:

```rust
/// Thứ mà một kẻ chờ có thể chờ trên đó. Trên POSIX là một fd.
#[derive(Clone, Copy)]
pub struct Source(/* RawFd trên unix */);

/// Một nguồn, và cái đang chờ ở nó.
pub struct Interest { pub source: Source, pub writable: bool }

pub trait Waiting {
    const SLEEPS: bool;
    /// Kẻ chờ nào cần biết các nguồn thì đặt `true`. `Spin` đặt `false`.
    const NEEDS_SOURCES: bool;
    fn idle(&mut self, interests: &[Interest]);
}

pub trait Transport {
    fn recv(&mut self, buf: &mut [u8]) -> Io;
    fn send(&mut self, buf: &[u8]) -> Io;
    /// Nguồn để chờ, khi có. `None` nghĩa là **không chờ được** — `Loopback`.
    fn source(&self) -> Option<Source> { None }
}
```

`Spin::idle` bỏ qua tham số và biến mất khi tối ưu. `Block::idle` gọi `poll` với timeout.

**Transport không chờ được thì từ chối lúc biên dịch, không phải lúc chạy.** Trong thân
`Engine::run`:

```rust
const { assert!(!W::NEEDS_SOURCES || T::POLLABLE, "standard mode needs a pollable transport") };
```

Cách này không đổi chữ ký `Engine::add`, không tốn gì lúc chạy, và **idiom này đã có trong repo** —
`crates/engine/tests/transport.rs:110` đang dùng `const { assert!(...) }`. Chứng minh bằng đảo
ngược: một doctest `compile_fail` ghép `Block` với `Loopback` — `compile_fail` là của rustdoc,
`cargo test` chạy nó, **không cần dependency nào**.

### 3. Timeout là hạt của `Tick`, và mặc định 100 ms

Engine phải tick được ngay cả khi không có I/O nào, vì heartbeat sống bằng tick. Nên timeout của
`poll` chính là hạt thời gian thô nhất mà session nhìn thấy.

100 ms, đổi được. `HeartBtInt` tính bằng giây nguyên và ba ngưỡng là 1.0 / 1.2 / 2.4 lần, nên
100 ms là một phần mười của khoảng nhỏ nhất có nghĩa. Giá phải trả khi rỗi hoàn toàn là 10 lần
thức mỗi giây — cổng ở bước 7 đo con số đó chứ không đoán.

### 4. Tập nguồn phải **đủ**, và mỗi thứ thiếu là một timeout latency

Đây là chỗ dễ sai nhất trong cả plan, và cả bốn ca đều **vẫn chạy đúng** — chỉ chậm thêm đúng một
timeout. Một lỗi chạy đúng là lỗi không ai thấy.

| Thiếu gì | Hậu quả | Ai canh |
|---|---|---|
| Listener không nằm trong tập poll | kết nối mới chờ tới **một timeout** trước khi được nhận | bước 4, và cổng bước 7 đo connect → Logon reply |
| Không đăng ký `POLLOUT` khi còn `tx` tồn đọng | một flush bị nghẽn chờ tới **một timeout** | bước 4, dùng `has_pending_output()` đã có |
| Kết quả từ thread khác (`OUT_OF_BAND`) không đánh thức được ai | reply của ứng dụng chờ tới **một timeout** | bước 5, waker |
| Timeout đặt bằng 0 | không phải chặn, mà là quay vòng đội lốt | cổng bước 7 đo CPU |

Waker là một **self-pipe** (POSIX, không cần `eventfd` nên chạy cả macOS): một đầu nằm trong tập
poll, `RingDispatch` ghi một byte vào đầu kia khi push. Bỏ waker đi thì `standard` có một sàn
latency bằng đúng timeout đối với mọi ứng dụng chạy ngoài luồng — và `RingDispatch` tồn tại chính
là để ứng dụng chạy ngoài luồng.

### 5. `Park` đổi tên thành `Yield`, và tài liệu nói thẳng nó **trượt cả hai cổng**

Câu hỏi mở số 3 của ADR-0013. Câu trả lời: **giữ nó, nhưng gọi đúng tên và thôi để nó trông như
một mode.**

Nó không xóa được: mọi test trong repo dùng nó, và không test nào trong số đó cần chặn — chúng tự
lái `turn()` bằng tay. Một test suite quay vòng là một test suite ghim core vô cớ; một test suite
chặn thì phải có socket thật. `Yield` là cái ở giữa và nó **hữu ích cho test**.

Cái phải sửa là nó đang nằm cạnh `Spin` như thể hai anh em ngang hàng. Sau plan này:

- `wait::Spin` — mode `hft`.
- `wait::Block` — mode `standard`, sau feature.
- `wait::Yield` — **không phải mode nào**, cho test, và rustdoc nói rõ: nó **trượt cổng `hft`**
  (`sched_yield` nằm trong danh sách `SLEEPERS`) **và trượt cổng `standard`** (nó đốt 100% core).
  Đó không phải khiếm khuyết của nó, đó là định nghĩa của nó.

Và một cái lợi kèm theo: **nửa đỏ của `check-no-kernel-sleep.sh` chuyển từ `--park` sang
`--mode standard`.** Nửa đỏ hiện tại trượt vì `sched_yield`, mà `sched_yield` thì không ai vô tình
viết vào engine. `ppoll` thì có — nó là hình dạng của một hồi quy thật. Nửa đỏ mạnh lên.

### 6. `w2w` và benchmark giữ `hft` làm mặc định

Câu hỏi mở số 2 của ADR-0013. **Giữ.** Chúng sinh ra để tạo số cho `hft`; đổi mặc định là âm thầm
đổi mọi con số đã công bố. `w2w` nhận thêm `--mode standard|hft`, mặc định `hft`, và **in ra mode
trong mọi lần chạy** — ADR-0013 quyết định 4 bắt mọi con số phải nêu mode của nó.

### 7. `density` là một **hình dạng trong `standard`**, không phải mode thứ ba

Câu hỏi mở số 4 của ADR-0013. Nhiều session trên một thread có chặn chính là cách chạy `standard`
bình thường; đặt cho nó một cái tên riêng là tạo ra mode thứ ba không có gì khác biệt để nói.
Cái tên `density` vẫn dùng được như một **nhãn cho con số**, cạnh `N` của nó, và ADR-0012 quyết
định 3 vẫn nguyên.

### File sẽ tạo hoặc sửa

- `crates/engine/src/wait.rs` — `Waiting` đổi chữ ký, `Spin` giữ, `Park` → `Yield`, `Block` mới sau `#[cfg]`
- `crates/engine/src/block.rs` — mới, `poll(2)`, sau `#[cfg(feature = "standard")]`
- `crates/engine/src/waker.rs` — mới, self-pipe, cùng feature
- `crates/engine/src/transport.rs` — `Source`, `Transport::source()`, `POLLABLE`
- `crates/engine/src/conn.rs` — phơi `Interest` của một connection
- `crates/engine/src/dispatch.rs` — `RingDispatch` đánh thức khi push
- `crates/engine/src/lib.rs` — khai báo `mod` có `#[cfg]`, tập interest, `Acceptor::source()`, `serve()` đăng ký listener
- `crates/engine/Cargo.toml` — feature `standard` (mặc định), `libc` chỉ trong feature đó
- `crates/engine/tests/standard.rs` — mới
- `crates/engine/benches/alloc.rs` — case `idle_standard`
- `tools/w2w/src/main.rs` — `--mode`, in mode ra
- `scripts/check-standard-gives-the-core-back.sh` — mới
- `scripts/check-no-kernel-sleep.sh` — nửa đỏ đổi sang `--mode standard`
- `.github/workflows/ci.yml` — job mới
- `docs/decisions/ADR-0014-standard-mode-blocks-on-poll.md` — bước 1, **đã viết 2026-08-30**
- `docs/DESIGN.md` D8, D5, §3, §6, §8 · `docs/GUIDE.md` §0 · `CHANGELOG.md` · `STATUS.md` · `CLAUDE.md` §2

> **Số hiệu ADR — đã giải quyết 2026-08-30.** Plan này viết ADR trước, nên nó lấy **0014**;
> [threads-and-affinity](2026-08-30-threads-and-affinity.md) lấy **0015** khi tới lượt. Và
> `ls docs/decisions/` phát hiện thêm một thứ đáng ghi: **không có ADR-0009**, và đó là **khoảng
> trống có chủ ý chứ không phải file bị mất** — plan
> [gates-that-can-be-trusted](2026-08-30-gates-that-can-be-trusted.md) đã xin số đó cho một thay
> đổi API của `SessionUnderTest::step`, rồi bỏ thiết kế và xoá hook thay vì ship. `CLAUDE.md` §5
> cấm dùng lại số, nên 0009 để trống vĩnh viễn. Ghi ở đây và trong ADR-0014 vì người đọc sau sẽ đi
> tìm một file không tồn tại.

## Bất biến bị đụng tới

| # | Đụng thế nào | Giữ bằng cách nào |
|---|---|---|
| **1** — không cấp phát trên hot path | Tập `Interest` / `pollfd` dựng lại mỗi lượt rỗi | Buffer đặt sẵn trong `Engine`, `reserve` lúc khởi động, mỗi lượt chỉ `clear()` + `push`. `benches/alloc.rs` thêm case **`idle_standard`** và nó phải ra **0** |
| **2** — session layer thuần | Không đụng `session` | Không có file nào của `crates/session` trong danh sách trên |
| **3** — 59 định nghĩa là cổng | Đổi `Waiting` là đổi type parameter của `Engine` mà `wire.rs` dùng | Chạy lại **cả hai**: `-p fixbolt-session --test score` và `-p fixbolt-engine --test wire`, phải **59 / 59**. Thêm một lần chạy `wire` **ở `standard`** |
| **4** — mode-scoped, và **cả hai nửa đều là luật** | Đây là plan làm ra nửa thứ hai | Hai script, mỗi cái có nửa đỏ của riêng nó. `check-no-kernel-sleep.sh` vẫn phải xanh cho `hft`; script mới phải xanh cho `standard` **và đỏ khi chạy với `hft`** |
| **5** — thứ tự field từ bảng sinh | Không đụng | — |
| **6** — feature phải gate chính `mod` | `standard` là feature mới | `#[cfg(feature = "standard")] mod block;` trong `lib.rs`, **không chỉ trong `Cargo.toml`**. Job CI `no-default-features` là thứ canh. `CLAUDE.md` §10 đã liệt kê sẵn bẫy này |
| **7** — không `panic`/`unwrap`/`expect` | Code mới đọc mã trả về của `poll`, dựng self-pipe | Mọi đường trả `Result` với enum không trường. `EINTR` **không phải lỗi** — nó là "quay lại chờ tiếp". Lint workspace canh |
| **8** — `unsafe` cần bình luận nêu thứ chứng minh nó đúng | `poll(2)` và `pipe2` qua `libc` là `unsafe` | Đúng hai khối `unsafe`, mỗi khối bao quanh một lời gọi, kèm bình luận nêu **tên test** đọc lại kết quả. Không có `unsafe` nào khác |
| **9** — không copy nguồn QuickFIX | Không đụng | — |
| **10** — không có số nào thiếu benchmark, máy, và §9 | Plan này sinh ra số mới: giá một lần thức của `standard` | Dòng "2–5 µs" trong `DESIGN.md` §8 hiện là **số lấy từ tài liệu** và tự nói vậy. Nó chỉ được thay bằng số đo được **trên máy §9**, kèm khối machine. Chưa đo được thì **giữ nguyên nhãn**, không đoán |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | ~~**ADR**~~ — **xong 2026-08-30**: [ADR-0014](../decisions/ADR-0014-standard-mode-blocks-on-poll.md), `Proposed`. Cơ chế readiness và dependency (`poll(2)` + `libc`), phạm vi Windows, số phận của `Park`, mặc định của `w2w`/bench, `density`. Trả lời cả bốn câu hỏi mở của ADR-0013. Chốt hình dạng API | ADR-0013 (đã ký) |
| 2 | ~~seam~~ — **xong 2026-08-30.** `Source`, `Interest`, `Transport::POLLABLE`/`source()`; `Waiting` đổi chữ ký + `NEEDS_SOURCES`; `Park` → `Yield`; **và `libc` + feature `standard` + `poll::Poller`** — xem "Sửa 1" bên dưới | 1 |
| 3 | ~~`wait::Block`~~ — **xong 2026-08-30.** Timeout 100 ms mặc định có sàn 5 ms, `EINTR` quay lại chờ **phần còn lại**, `NEEDS_SOURCES = true`, lỗi `poll` được **ghi lại** và vẫn trả core lại | 2 |
| 4 | ~~tập interest~~ — **xong 2026-08-30.** `refresh_interests[_with]`, `idle_with`, `Acceptor::source()`, `serve()` đăng ký listener, const-assert thật của ADR-0014 quyết định 4, và `sources_missing()` | 3 |
| 5 | ~~waker~~ — **xong 2026-08-30.** Self-pipe; engine tự bỏ đầu đọc vào tập poll và **tự drain**; **`RingApp`** đánh thức khi đẩy reply — không phải `RingDispatch`, xem "Sửa 3" | 4 |
| 6 | ~~`w2w --mode`~~ — **xong 2026-08-30.** Ba giá trị `hft\|standard\|yield`, mặc định `hft`, in mode mỗi lần chạy và **cổng đọc lại**. Nửa đỏ của `check-no-kernel-sleep.sh` sang `--mode standard`. **Và `serve()` thành `standard`** — xem "Sửa 4" | 4 |
| 7 | ~~cổng `standard`~~ — **xong 2026-08-30.** `scripts/check-standard-gives-the-core-back.sh` + job CI `standard-blocks`. **Bốn** khẳng định, nửa đỏ là **`hft` và `yield`**, và hai mã thoát tách *hỏng chính sách* khỏi *hỏng phép đo* | 6 |
| 8 | ~~59 ở cả hai mode~~ — **xong 2026-08-30, 59/59 ở `standard`**. **Đo trên máy §9: KHÔNG LÀM ĐƯỢC** ở container này (`check-machine.sh`: `pass 2 fail 6 unknown 3`); dòng §8 giữ nguyên nhãn "từ tài liệu", đúng như plan đã dự phòng | 7, máy §9 |

## Cách kiểm chứng

Từng bước, và **đọc output chứ không đọc exit code**.

- **Bước 2 — `source()` trả về fd thật, không phải một con số.** Test so
  `TcpTransport::source()` với `as_raw_fd()` của chính stream đó, rồi `poll` một lần với timeout 0
  trên fd ấy sau khi đã ghi dữ liệu vào đầu kia: phải báo readable. **Đảo ngược**: trả về `fd + 1`,
  test phải đỏ.
- **Bước 2 — không có gì gãy.** `cargo test --all` và `cargo test --all --no-default-features`
  xanh, và cổng 59 vẫn 59 / 59 ở **cả hai** đường (in-process và qua socket). Bước này đổi public
  API mà không đổi hành vi, nên bất kỳ thay đổi nào ở điểm số đều là hồi quy.
- **Bước 3 — `Block` thật sự chặn, và thật sự thức đúng lúc.** Hai test, cùng một socket:
  socket rỗi → `idle` trả về sau **≈ timeout** (đo bằng `Instant`, chấp nhận sai số rộng);
  ghi một byte vào đầu kia rồi gọi `idle` → trả về **≪ timeout** (đặt ngưỡng ở 1/10 timeout).
  Test thứ hai là cái thật sự có nghĩa: một `Block` luôn trả về sau đúng timeout cũng "chặn", và
  nó là bug tệ nhất của cả plan này.
- **Bước 3 — `EINTR` không giết engine.** Test gửi một tín hiệu tới thread đang chờ; `idle` phải
  trả về bình thường và vòng lặp đi tiếp. **Đảo ngược**: coi `EINTR` là lỗi, test phải đỏ.
- **Bước 4 — mỗi thứ thiếu trong bảng ở mục "Cách làm" §4 có một test.** Kết nối mới được nhận
  trong ≪ timeout; một flush bị nghẽn tiếp tục trong ≪ timeout. Mỗi ca **đảo ngược bằng cách bỏ
  đúng nguồn đó ra khỏi tập poll**, và test phải đỏ vì **chờ đúng một timeout**, không phải vì
  hỏng — kiểm bằng cách so thời gian, không chỉ so `is_err()`.
- **Bước 4 — `benches/alloc.rs` case `idle_standard` ra 0**, và case đó phải tự khẳng định
  đường của nó có chạy — cùng nếp mà `busy` đã học được.
- **Bước 5 — waker.** Reply sinh trên thread khác tới trong ≪ timeout. **Đảo ngược**: bỏ waker,
  test phải đỏ **vì đúng một timeout trôi qua**.
- **Bước 6 — nửa đỏ mới của `check-no-kernel-sleep.sh` thật sự đỏ.** Chạy script trên Linux, đọc
  output: nửa xanh (`hft`) không có syscall chặn nào, nửa đỏ (`standard`) phải trượt **vì `ppoll`**,
  và tên syscall đó phải in ra.
- **Bước 7 — cổng mới, và nó phải đo được ba thứ cùng lúc.** Với cửa sổ rỗi 3 giây:
  1. **CPU của engine thread < 5%** — `utime + stime` đọc từ `/proc/<pid>/task/<tid>/stat` trước và
     sau cửa sổ, chia cho wall clock. `getconf CLK_TCK` là 100, nên 3 giây cho 300 tick, đủ phân giải.
  2. **Thread còn sống và thật sự đang chặn** — `strace` quy theo tid phải thấy `ppoll`. CPU 0%
     cũng đúng với một thread đã chết; đây là cùng cái bẫy mà `ran` trong script cũ đã canh.
  3. **p50 của `w2w` ≪ timeout** — nếu engine thức vì đồng hồ chứ không vì dữ liệu, nó vẫn chạy
     đúng và vẫn 0% CPU, và **hai kiểm tra đầu vẫn xanh**. Chỉ dòng này bắt được nó.

  Rồi chạy lại **cùng binary với `--mode hft`**, và script phải **trượt** — CPU ≈ 100%, không có
  `ppoll`. Một cổng chưa từng được thấy đỏ thì không được biết là chạy được;
  `check-no-kernel-sleep.sh` có hai tiền lệ đúng như vậy.
- **Bước 7 — chạy trên máy yên.** `check-machine.sh` dòng `machine is quiet` phải xanh trước khi
  đọc bất kỳ con số CPU nào, vì tải cạnh tranh đã một lần làm hỏng cả một chuỗi kết luận
  ([measured-costs.md](../reference/measured-costs.md)).
- **Bước 8 — 59 / 59 ở cả hai mode**, và mọi con số in ra đều kèm mode, `N`, và khối machine.

## Tài liệu phải cập nhật

Theo bảng đồng bộ `CLAUDE.md` §4.

- [x] [`ADR-0014`](../decisions/ADR-0014-standard-mode-blocks-on-poll.md) — bước 1, **viết 2026-08-30**, `Proposed`
- [ ] `docs/DESIGN.md` D8 — bỏ câu *"not built yet"*, ghi cơ chế và timeout thật
- [ ] `docs/DESIGN.md` D5 — `Transport` có `source()`, và feature `standard` gate cái gì
- [ ] `docs/DESIGN.md` §3 — module mới trong `engine`
- [ ] `docs/DESIGN.md` §6 — hàng cổng mới: *"a `standard` engine gives the core back"*
- [ ] `docs/DESIGN.md` §8 — dòng wakeup của `standard`: thay bằng số đo được, hoặc giữ nhãn "từ tài liệu"
- [ ] `docs/GUIDE.md` §0 — bỏ *"`standard` is not built yet"*; §5 — timeout là hạt của tick
- [ ] `CHANGELOG.md` — `Waiting` đổi chữ ký, `Transport` thêm phương thức, `Park` → `Yield`
- [ ] `CLAUDE.md` §2 — danh sách "machine-checked today": nguyên tắc 4 từ **nửa** thành **đủ**
- [ ] `STATUS.md` — mục "Where the work is", và ba điểm bắt đầu ở đầu file
- [ ] `crates/engine` rustdoc — `wait.rs` mở đầu đang nói *"the engine thread busy-polls"* như thể vô điều kiện

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| `Block` chặn nhưng **thức vì timeout chứ không vì dữ liệu** — vẫn đúng, chỉ chậm 100 ms mỗi tin | Bước 3 test thứ hai, và điều 3 của cổng bước 7 (p50 ≪ timeout) |
| Listener không nằm trong tập poll → kết nối mới chờ một timeout | Bước 4, đo connect → Logon reply |
| Không `POLLOUT` khi còn `tx` tồn đọng → flush nghẽn chờ một timeout | Bước 4, đảo ngược bằng cách bỏ writable interest |
| Kết quả `OUT_OF_BAND` không đánh thức được ai | Bước 5, đảo ngược bằng cách bỏ waker |
| `Yield` bị nhầm là `standard` | Nó **trượt cả hai cổng**, và rustdoc nói vậy. Chạy cổng bước 7 với `Yield` phải đỏ |
| Feature có trong `Cargo.toml` mà `mod` không có `#[cfg]` | Job CI `no-default-features`. Bẫy này `CLAUDE.md` §10 liệt kê sẵn |
| `EINTR` bị coi là lỗi → engine thoát khi có tín hiệu | Bước 3, có test và có đảo ngược |
| Cấp phát lọt vào đường rỗi (dựng lại `Vec<pollfd>` mỗi lượt) | `benches/alloc.rs` case `idle_standard` = 0 |
| CPU ≈ 0% vì thread **đã chết**, không phải vì nó chặn | Cổng bước 7 điều 2: phải thấy `ppoll` quy theo tid |
| Layout `pollfd` hoặc `nfds` sai qua FFI, `poll` trả về mà không ai để ý | Bước 2: fd đã có dữ liệu phải được báo readable; sai layout thì không bao giờ báo |
| Đo CPU trên máy đang bận rồi kết luận | `check-machine.sh` dòng `machine is quiet`, đọc trước khi đọc số |
| Đổi `Waiting` làm rơi điểm cổng 59 mà không ai nhìn | Bước 2 chạy lại **cả hai** đường của cổng 59 trước khi có bất kỳ code `Block` nào |
| Số hiệu ADR trùng với plan `threads-and-affinity` | `ls docs/decisions/` ngay trước khi tạo file |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| `libc` là dependency ngoài đầu tiên của `engine` | Trung bình | Chỉ trong feature `standard`; `--no-default-features` không kéo nó, và job CI chứng minh. Cùng dependency mà plan affinity đã lấy — một cây, không phải hai |
| `unsafe` trong một crate chưa có `unsafe` nào | Trung bình | Đúng hai khối, mỗi khối một lời gọi, kèm bình luận nêu tên test. `unsafe_code = "warn"` sẽ kêu, và đó là đúng |
| Đổi chữ ký `Waiting` và thêm phương thức vào `Transport` là phá public API | Thấp | Chưa publish gì lên crates.io. `CHANGELOG.md` ghi. `Transport::source()` có bản mặc định nên transport của người dùng vẫn biên dịch được |
| `poll(2)` là O(N); ở `density` cao nó thành chi phí thật | Trung bình | Đã nói trong "Cách làm" §1: `epoll` là ADR sau, **kèm số**, không đổi vì linh cảm. Bước 8 đo giá một lần thức để có điểm so sánh đầu tiên |
| Hai mode = hai lần đo, mãi mãi | Cao, và ADR-0013 đã lường | Bảng §7 của `CLAUDE.md` đã có dòng *"Both modes"*. Cổng bước 7 tồn tại để `hft` không âm thầm thành đường không ai chạy |
| Đo xong thấy `standard` chậm hơn nhiều so với 2–5 µs của tài liệu | Trung bình | **Đó là kết quả, và ghi đúng như vậy.** ADR-0013 đã nói trước rằng con số này sẽ bị trích dẫn để chê dự án, và rằng nó là con số trung thực của mode đó |
| Máy §9 chưa `pass 10 fail 0` nên bước 8 không đo được | Trung bình | Bước 8 tách làm hai: 59/59 ở cả hai mode **không** cần máy §9 và làm ngay; dòng §8 giữ nhãn "từ tài liệu" cho tới khi có máy. Không hạ chuẩn để đóng |

## Ngoài phạm vi

- **Windows và IOCP.** Trả lỗi có kiểu, không âm thầm spin. Muốn có thì là một ADR và một plan riêng.
- **`epoll`, `kqueue`, `io_uring`.** `poll(2)` trước. Đổi cơ chế cần số đo, không cần linh cảm.
- **Không gỡ 703 ns** (item 22) và **không kernel bypass** (item 14). Plan này quyết định engine
  *ngủ thế nào*, không phải nó *đọc socket thế nào*.
- **Không shard, không ghim core.** Đó là plan [threads-and-affinity](2026-08-30-threads-and-affinity.md).
- **Không đổi tiêu chí settle của `crates/engine/tests/wire.rs`.** `standard` có thể làm nó tốt hơn;
  đó là một việc khác và nó đang xanh 59/59 trên cả hai máy.
- **Không đo lại toàn bộ `DESIGN.md` §8 ở `standard`.** Chỉ dòng wakeup.
- **Không chọn timeout "tối ưu".** 100 ms là một con số đủ dùng và đổi được; tối ưu nó cần một
  workload thật, mà cái đó chưa có.

## Nhật ký giao hàng

### 2026-08-30 — ĐÓNG

**Tám bước, bảy rưỡi làm xong.** `standard` mode tồn tại, **là mặc định** (`serve()` chặn,
`serve_hft()` quay vòng), và nguyên tắc 4 của `CLAUDE.md` hết nửa vời.

**Việc duy nhất để lại, và lý do:** đo giá một lần thức của `standard` cần máy `DESIGN.md` §9.
Container của phiên này đọc `pass 2 fail 6 unknown 3`. Dòng wakeup trong `DESIGN.md` §8 **giữ
nguyên nhãn "lấy từ tài liệu"**. Plan đã dự phòng đúng tình huống này trong bảng Rủi ro ngay từ
lúc viết — *"không hạ chuẩn để đóng"* — nên đây là kế hoạch chạy đúng, không phải kế hoạch trượt.
Nó về desktop của chủ dự án cùng với mục 6, 11, 13.

**Bốn vực latency mà ADR-0014 quyết định 6 gọi tên: bịt cả bốn.** Listener trong tập poll,
`POLLOUT` khi còn byte tồn đọng, waker cho dispatch ngoài luồng, và timeout 0 bị từ chối ở
constructor.

**Điều đáng nhớ nhất của plan này không phải `standard` mode.** Là **bảy lần một thứ xanh (hoặc
đỏ) vì lý do khác với thứ nó nói mình đang kiểm**, và cả bảy đều bị bắt bởi việc chạy và đọc chứ
không phải bởi việc đọc lại code:

| # | Thứ báo sai | Bắt được nhờ |
|---|---|---|
| 1 | *"30 test binaries"* — thật ra là `head -30` cắt output | đếm lại cho tử tế |
| 2 | job CI `no-default-features` xanh về một bản build chưa từng xảy ra | **số test lệch 4** |
| 3 | test hỏi về fd đã đóng — xanh 30 lần, đỏ lần chạy nguội đầu | vị trí panic chỉ đúng nhánh |
| 4 | `compile_fail` đỏ vì trait bound sai, không vì const-assert | đọc thông điệp lỗi |
| 5 | test listener xanh cả khi xoá đúng dòng nó phải canh | đảo ngược |
| 6 | `--mode standard` in banner rồi không chạy gì | chạy công cụ, thấy thiếu khối latency |
| 7 | cổng `standard`: p50 trả về **50** — chữ số trong *nhãn* `p50` | nhìn con số và thấy nó vô lý |

Cộng thêm một lần **prose sai**: doc comment của bài test `standard` khẳng định nó bắt được lỗi
nối dây, và ba phép đo thời gian bác bỏ.

**Bảy trong bảy đều là false green, không phải bug.** Code gần như luôn đúng; thứ hỏng là **bằng
chứng**. Đó chính là điều `CLAUDE.md` §10 nói và là điều repo này nợ `testing-skills`. Bốn case
đã có `[to testing-skills]` trong `docs/reference/`.

**PR upstream `testing-skills`** — §11 nói mở khi plan đóng. **Chưa mở**, và đó là việc phải
hỏi chủ dự án trước: nó đẩy nội dung ra một repo **công khai**, và đó là một trong ba thứ mà uỷ
quyền "tự duyệt tự chạy" không bao gồm.

### 2026-08-30 — bước 8: nửa làm được đã xong, nửa kia cần máy khác

**Đã dựng.** `crates/engine/tests/wire.rs` nhận mode làm tham số kiểu, và có bài chạy thứ hai:
**59 / 59 với engine thật sự chặn giữa các bước.** Đây là hoá đơn mà ADR-0013 biết mình đang ký
— *"hai mode là hai thứ phải kiểm, mãi mãi"* — và là chỗ duy nhất corpus gặp `standard`, vì mọi
dòng khác trong file đó lái `turn` bằng tay, nơi chiến lược rỗi không bao giờ được chạm tới.

Chạy hết 3.00 s so với 0.78 s ở `Yield`, và `user 0.247s` trên 3 giây wall — engine ngủ thật.

**`[đo 2026-08-30]` Và tôi đã viết một lời khẳng định sai vào chính doc comment của bài test
đó, rồi đảo ngược bác bỏ nó.** Bản đầu viết: *"lỗi nối dây — quên listener, không hỏi
writability, không drain waker — sẽ hiện ra thành test chạy hàng phút thay vì hàng giây."*

Hai đảo ngược, và **không cái nào làm nó chậm đi**:

| | wall |
|---|---|
| baseline | 3.28 s |
| `Block` bỏ qua readiness hoàn toàn | 3.30 s |
| listener bị bỏ khỏi tập poll | 3.34 s |

Lý do nằm ở tiêu chí settle: một bước kết thúc khi engine không động gì trong `STEP_QUIET` =
1 ms, còn timeout ở đây là sàn 5 ms. Nên **một lần block luôn thoả tiêu chí đó** — dù nó thức
sau 0.1 ms vì có dữ liệu hay sau 5 ms vì hết giờ. Harness không phân biệt được hai cái, và thời
gian chạy là `số bước × 5 ms` trong cả hai trường hợp. Nâng timeout lên không giúp gì: nó nhân
đều cả hai nhánh.

**Bài test vẫn có giá trị thật** — nó chứng minh giao thức không đổi khi engine chặn — nhưng nó
**không** chứng minh phần nối dây, và comment giờ nói đúng điều đó, kèm chỗ phần nối dây thật sự
được canh: `tests/standard.rs` đọc thẳng danh sách interest, và khẳng định p50 trong cổng
`standard`. `CLAUDE.md` §4 nói *prose không giữ nổi một ràng buộc*; ở đây prose còn **sai**, và
thứ phát hiện ra là đảo ngược chứ không phải đọc lại. `[to testing-skills]`

**Nửa còn lại KHÔNG làm được, và không hạ chuẩn để đóng.** Đo giá một lần thức của `standard`
cần máy `DESIGN.md` §9. `[đo 2026-08-30]` container của phiên này: `check-machine.sh` cho
**`pass 2 fail 6 unknown 3`** — nó tự nói ra rằng số ở đây dùng được cho phép đếm và cho so sánh
A/B với chính nó, **không** dùng được làm số latency. Nên `DESIGN.md` §8 **giữ nguyên nhãn "lấy
từ tài liệu"** cho dòng wakeup, đúng như plan đã dự phòng ngay từ mục Rủi ro.

Việc còn lại thuộc về máy desktop của chủ dự án, cùng chỗ với các mục 6, 11, 13 vốn đã chờ ở đó.

**Gate cho commit này:**

```
cargo fmt --all --check                    sach
cargo clippy --all-targets -- -D warnings  sach
cargo test --all                           230 passed, 0 failed
cargo test --all --no-default-features     0 failed
-p fixbolt-engine --test wire              2 passed: 59/59 hft-style, 59/59 standard
scripts/check-machine.sh                   pass 2 fail 6 unknown 3 -- KHONG phai may §9
```

### 2026-08-30 — bước 7 xong: nguyên tắc 4 hết nửa vời

**Đã dựng.** `scripts/check-standard-gives-the-core-back.sh` và job CI `standard-blocks`.
`CLAUDE.md` §2 danh sách "machine-checked today" giờ ghi nguyên tắc 4 là **cả hai nửa**, thay cho
*"half of it"*.

**Bốn khẳng định, vì CPU gần 0 là thứ mà ba loại engine hỏng khác nhau đều đạt được:**

| # | Khẳng định | Nó bắt cái gì mà cái khác không bắt |
|---|---|---|
| 1 | mode mà binary **tự khai** đúng với mode được xin | một lần chạy không hề tới được mode đó — đã xảy ra một lần ở bước 6 |
| 2 | CPU của engine thread dưới trần 5% | engine quay vòng |
| 3 | trạng thái scheduler là `S` chứ không `R`, lấy mẫu 20 lần | **thread đã chết** — nó cũng cho 0% CPU |
| 4 | p50 khứ hồi thấp hơn hẳn timeout | **engine thức vì đồng hồ của chính nó, không vì dữ liệu** |

**`[đo 2026-08-30]` Khẳng định 4 không phải lý thuyết, và không cái nào khác thay được nó.** Đảo
ngược `Block` để nó bỏ qua readiness và luôn chờ hết timeout: CPU **0%**, tìm thấy đang ngủ
**20/20 mẫu** — khẳng định 2 và 3 **đều xanh** — và p50 nhảy lên **99 046 599 ns, đúng một
timeout**. Chỉ khẳng định 4 nhìn thấy. Đó là con engine *đúng, rỗi, và chậm 100 ms mỗi tin* mà
plan này đã gọi tên từ đầu, giờ có số đo.

**Hai lỗi trong chính cổng, và cái thứ hai đáng sợ hơn:**

1. **`$12` trong bash là `${1}2`.** Thiếu ngoặc nhọn, và dưới `set -u` nó thành một biến chưa
   khai báo mang tên trạng thái của thread. Mọi phép đo hỏng.
2. **Và khi mọi phép đo hỏng, cả ba arm đều báo cái trông như đáp án đúng**: nửa xanh trượt, và
   **cả hai nửa đỏ báo `RED ok`**. Vì `judge` trả 1 cho cả *hỏng chính sách* lẫn *không đo được*,
   còn arm đỏ coi mọi thất bại là thành công. **Một nửa đỏ đỏ vì harness hỏng chứng minh đúng
   bằng một nửa xanh xanh vì không có gì chạy** — §10 áp vào chính cái nửa đáng lẽ là lưới an
   toàn. Sửa: hai mã thoát tách bạch, `1` là chính sách và `2` là không đo được; nửa đỏ **chỉ
   chấp nhận `1`**.

**Và một false green thứ ba, ngay bên trong khẳng định 4.** Bản đầu lấy p50 bằng
`grep -oE '[0-9]+' | head -1`, và nó trả **50** cho mọi mode — chữ *50* trong **nhãn** `p50`, vì
đó là dãy số đầu tiên trên dòng. Nên khẳng định *thứ duy nhất phân biệt được thức-vì-dữ-liệu với
thức-vì-đồng-hồ* đang so một hằng số với trần của nó và **luôn luôn đạt**. Nó đạt ở cả ba arm,
mà đó chính là lý do không có gì trông sai. Sửa bằng `awk` lấy đúng trường thứ hai; số thật của
`standard` là **10 917 ns**, thấp hơn timeout bốn bậc. `[to testing-skills]`

**`yield` giờ được *chứng minh* là trượt cả hai cổng**, thay vì được khẳng định. Từ bước 2 tới
giờ tài liệu vẫn nói vậy và chưa gì cho thấy. `[đo 2026-08-30]` `yield` cho CPU **99.70%**, ngủ
**0/20 mẫu**.

**Số đo trên container 4 vCPU dùng chung, không phải máy §9:**

```
standard   CPU  0.00%   ngu 20/20   p50 10 917 ns
hft        CPU 98.81%   ngu  0/20   p50 19 909 ns
yield      CPU 99.70%   ngu  0/20   p50 18 096 ns
```

CPU và trạng thái ngủ **không phải số latency** — chúng là tỉ lệ và một phép đếm, không cần máy
§9. p50 thì cần, nên nó vẫn không được công bố ở đâu và `DESIGN.md` §8 giữ nguyên nhãn.

**Còn lại:** bước 8 — chạy 59 định nghĩa ở cả hai mode, và đo giá một lần thức trên máy §9.

**Gate cho commit này:**

```
cargo fmt --all --check                          sach
cargo clippy --all-targets -- -D warnings        sach
cargo test --all                                 229 passed, 0 failed
cargo test --all --no-default-features           0 failed
scripts/check-no-optional-deps.sh                exit 0
scripts/check-no-kernel-sleep.sh                 exit 0
scripts/check-standard-gives-the-core-back.sh    exit 0; hai nua do truot tren chinh sach
  (dao nguoc tran p50 -> exit 1; dao nguoc Block bo qua readiness -> exit 1, p50 99 ms)
-p fixbolt-session --test score                  59/59 trong process
-p fixbolt-engine  --test wire                   59/59 qua socket that
scripts/bench.sh                                 exit 0; 8/8; 0 invariant failure
scripts/check-links.py                           292 link, 0 chet
```

### 2026-08-30 — bước 6 xong: cả hai mode chạy được, và một mode từng chỉ giả vờ chạy

**Đã dựng.** `w2w --mode hft|standard|yield`, mặc định `hft`, in `mode: <tên>` ngay dòng đầu.
Nửa đỏ của `check-no-kernel-sleep.sh` chuyển sang `--mode standard`. `serve()` giờ là
`standard`; `serve_hft()` là bản quay vòng.

**Sửa 4 — plan thiếu một việc mà ADR-0013 bắt buộc.** Không bước nào trong tám bước làm `serve()`
thành `standard`, trong khi ADR-0013 quyết định 1 nói `standard` là *"cái người ta nhận được khi
không nói gì"* — và `serve()` **chính là** cái đó. Thiếu nó thì `standard` chỉ *tồn tại*, chứ
chưa *là mặc định*, mà "là mặc định" mới là toàn bộ nội dung của ADR. Đưa vào bước 6.
`TcpAcceptorEngine<A>` thành `TcpAcceptorEngine<A, W>` với hai alias `HftAcceptorEngine` và
`StandardAcceptorEngine`, và **một hàm `pump` dùng chung cho cả hai `serve`** — hai vòng lặp viết
riêng là hai vòng lặp sẽ trôi khỏi nhau, mà listener được đăng ký ở cái này và không ở cái kia
đúng là kiểu trôi tốn một timeout và không hiện ra ở đâu cả.

**`--mode yield` không thừa.** Tài liệu **khẳng định** `Yield` trượt cả hai cổng từ bước 2 và
chưa gì chứng minh. Giờ có một arm để chạy nó qua từng cổng và **nhìn** nó trượt, thay vì đọc
câu nói nó sẽ trượt. `CLAUDE.md` §4: *prose không giữ nổi một ràng buộc.*

**`[đo 2026-08-30]` `--mode standard` từng được chấp nhận, in banner, và không chạy gì cả.**
Nhánh chọn chiến lược nằm sau `#[cfg(all(feature = "standard", unix))]` — mà **feature là của
từng crate, và `cfg` không bao giờ với sang được feature của dependency**. `w2w` không khai
feature nào trong manifest của chính nó, nên điều kiện đơn giản là **sai**, mọi nhánh lấy `else`,
và vòng đo không hề chạy. Binary vẫn chạy. Mode thì không tồn tại.

Triệu chứng im lặng đến khó chịu: banner vẫn in (nó in **trước** nhánh), tiến trình vẫn exit 0,
dấu hiệu duy nhất là **khối latency vắng mặt** — mà người đọc lướt tìm "nó có chạy không" thì
nhìn thấy dòng mode rồi dừng. `cargo build` **có cảnh báo** đúng dòng đó suốt thời gian ấy
(`unexpected_cfg_condition_value`), và `clippy -D warnings` sẽ biến nó thành lỗi. Nhưng thứ bắt
được nó trước tiên là **chạy công cụ và đọc cái nó trả về**.

Đây là **mặt lật ngược của nguyên tắc 6**: rule 6 canh "feature có trong manifest mà `mod` không
có `#[cfg]`" — làm crate không ai build được. Đây là cùng lỗi từ phía kia, và hỏng theo chiều
ngược lại: **mọi thứ build được, và một nhánh code lặng lẽ biến mất.** Viết vào
[feature-flags-unify-across-a-workspace.md](../reference/feature-flags-unify-across-a-workspace.md)
làm case thứ hai. `[to testing-skills]`

**Và cổng thôi giả định nó đã chạy đúng arm nó xin.** `check-no-kernel-sleep.sh` gọi binary hai
lần và **toàn bộ ý nghĩa của nó nằm ở chỗ lần hai xử sự khác lần một**. Nếu script này có hình
dạng hiện tại sớm hơn một ngày, nó đã **xanh về hai lần chạy cùng một mode**. Nên `w2w` in mode
ra và script **đọc lại, sai thì trượt**. Đảo ngược: xin `hft` mà truyền `--mode yield` →
`w2w ran mode 'yield' when 'hft' was asked for`, exit 1.

**Số đầu tiên của `standard`, và nó không phải số để công bố.** Container 4 vCPU dùng chung, chưa
phải máy §9: `hft` p50 **17.7 µs**, `standard` p50 **29.0 µs**, `yield` p50 **18.2 µs**. Điều
đáng đọc không phải con số mà là **p50 của `standard` nhỏ hơn timeout 100 ms tới hơn ba bậc** —
tức là engine thức **vì dữ liệu, không vì đồng hồ**, chính là khẳng định thứ ba mà cổng ở bước 7
cần. `DESIGN.md` §8 **vẫn giữ nhãn "lấy từ tài liệu"**: đây không phải máy §9.

**Gate cho commit này:**

```
cargo fmt --all --check                     sạch
cargo clippy --all-targets -- -D warnings   sạch
cargo test --all                            229 passed, 0 failed
cargo test --all --no-default-features      0 failed
scripts/check-no-optional-deps.sh           exit 0
crates/engine --test standard               18 passed; 0/30 lần đỏ khi chạy lặp
cargo test -p fixbolt-engine --doc          1 passed
-p fixbolt-session --test score             59/59 trong process
-p fixbolt-engine  --test wire              59/59 qua socket thật
scripts/check-no-kernel-sleep.sh            exit 0; nửa xanh hft không có syscall chặn,
                                            nửa đỏ standard trượt vì 6 poll
                                            (đảo ngược mode read-back: exit 1)
scripts/bench.sh                            exit 0; 8/8 target; 0 invariant failure
scripts/check-links.py                      290 link, 0 chết
```

### 2026-08-30 — bước 5 xong: waker, và ADR-0014 gọi tên nhầm đầu

**Đã dựng.** `crates/engine/src/waker.rs`: `Waker` (đầu đọc, ở engine) và `WakeHandle`
(`Clone + Send`, ở thread khác). `Engine::with_waker` và `RingApp::with_waker`.

**Sửa 3 — ADR-0014 quyết định 6 gọi tên nhầm đầu, và tôi ghi lại chứ không lặng lẽ sửa.** Nó
viết *"`RingDispatch` writes one byte on push"*. Nhưng `RingDispatch::deliver` và `collect` đều
được gọi **từ `Engine::turn`**, tức là trên chính thread engine, tại thời điểm engine **đang
thức** — nó không bao giờ cần đánh thức ai. Thread phải đánh thức là **của ứng dụng**, và điểm
gọi là `RingApp::pump`, sau khi nó đẩy reply về. `CLAUDE.md` §5 cấm sửa nội dung một ADR đã
`Accepted`, mà đây là **lỗi sự thật ở một chi tiết**, không phải đổi ý — cơ chế (self-pipe), lý
do (`poll` thức vì fd chứ không vì ring buffer) và yêu cầu đều không đổi. Nên nó được ghi thành
một khối đính chính ngay trong ADR, và nhắc lại ở đầu `waker.rs` nơi người đọc code sẽ gặp.

**Ba quyết định thiết kế, mỗi cái vì một cái bẫy:**

1. **Engine tự bỏ waker của nó vào tập poll, không để người gọi thêm.** Quên nó chính là toàn bộ
   thất bại mà cơ chế này sinh ra để chặn; để một call site có thể quên là thiết kế sai.
2. **Drain sau *mỗi* lần chờ.** Một self-pipe còn byte chưa đọc thì **vẫn readable**, nên mọi
   `poll` sau đó trả về tức thì, mãi mãi. Engine vẫn chạy hoàn hảo và **đốt một core** — đúng
   thứ duy nhất `standard` sinh ra để tránh, và không bộ test đúng-sai nào lẫn phép đo độ trễ
   đánh thức nào nhìn thấy. Đây là cái bẫy lớn nhất của bước này và nó có test riêng.
3. **`wake()` không chặn, và một lần ghi bị từ chối không phải là việc bị mất.** Pipe đầy (64 KiB)
   thì `write` trả `EAGAIN` — mà một pipe còn byte chưa đọc thì **đã readable rồi**, tức là tín
   hiệu đã ở đó. Một wake đang chờ và một trăm nghìn wake nói cùng một điều: *nhìn lại đi.*

**Ba đảo ngược, ba lần đỏ đúng chỗ:**

| Đảo ngược | Đỏ ở đâu |
|---|---|
| bỏ `drain()` sau khi chờ | `a_wake_is_drained_so_the_next_wait_still_waits` |
| engine không bỏ waker của mình vào tập poll | **5 test** cùng đỏ |
| `RingApp` không đánh thức sau khi đẩy reply | `a_reply_from_another_thread_wakes_a_sleeping_engine` |

**Clippy bắt được hai thứ mà `cargo test` không bắt** — nhắc rằng "test xanh" không phải "gate
xanh". Một là `cast_signed()` **ổn định từ Rust 1.87 trong khi workspace ghim MSRV 1.85**; thay
bằng `usize::try_from`, và hoá ra đúng hơn thật: `read` trả về số **có dấu** có thể âm, mà `as`
sẽ biến số âm thành một độ dài khổng lồ. Hai là hai nhánh `if` giống hệt nhau trong `fcntl` —
viết lại thành ba bước tuần tự, mỗi bước trả lỗi riêng.

**Chưa làm:** bốn vực latency của ADR-0014 quyết định 6 giờ **đã bịt cả bốn** (listener,
`POLLOUT`, waker, timeout 0), nhưng **chưa mode nào chạy đầu-cuối**: `serve()` và `w2w` đều còn
`Spin`. Bước 6.

**Gate cho commit này:**

```
cargo fmt --all --check                     sạch
cargo clippy --all-targets -- -D warnings   sạch (0 lỗi)
cargo test --all                            229 passed, 0 failed
cargo test --all --no-default-features      0 failed
scripts/check-no-optional-deps.sh           exit 0
crates/engine --test standard               18 passed; 0/40 lần đỏ khi chạy lặp
cargo test -p fixbolt-engine --doc          1 passed (compile_fail,E0080)
-p fixbolt-session --test score             59/59 trong process
-p fixbolt-engine  --test wire              59/59 qua socket thật
scripts/check-no-kernel-sleep.sh            exit 0; nửa đỏ trượt vì 2970 sched_yield
scripts/bench.sh                            exit 0; 8/8 target; 0 invariant failure
  engine alloc: idle 0 send 0 recv 0 frame 0 turn 0 busy 0 ring 0 interests 0
scripts/check-links.py                      289 link, 0 chết
```

Hai target vượt trần vẫn là `fixbolt-codec/groups` và `serialize` — mục 11 và 20. **Không đo gì
mới.**

### 2026-08-30 — bước 4 xong: tập nguồn thật, và một test mang tên thứ nó không canh

**Đã dựng.** `Engine::refresh_interests[_with]` và `idle_with(extra)`; `Connection::source()`;
`Acceptor::source()`; `serve()` đăng ký listener; `sources_missing()`; và **const-assert thật**
của ADR-0014 quyết định 4 thay cho cái tạm ở bước 3.

**Ba điều đáng nói về cách dựng:**

1. **`writable` bất đối xứng với `readable`, và đó là điểm mấu chốt.** Một connection *luôn*
   đáng chờ để nhận byte, nhưng chỉ đáng chờ để *gửi* khi còn byte tồn đọng. Hỏi writable vô
   điều kiện thì engine bị đánh thức mỗi lần socket còn chỗ trống — tức là liên tục — và
   `standard` thành vòng quay trở lại. Cả hai nửa đều có test và **hỏng theo hai kiểu khác nhau**.
2. **Dựng lại mỗi lần, không cache.** `Source` mượn fd chứ không sở hữu; một danh sách giữ qua
   lượt trước có thể gọi tên một socket đã đóng và đã bị cấp lại cho người khác. Đây chính là bài
   học đã trả giá ở bước 2.
3. **`if W::NEEDS_SOURCES` là hằng số, nên với `Spin`/`Yield` toàn bộ việc dựng biến mất khi biên
   dịch.** `hft` có ngân sách 703 ns mỗi socket mỗi lượt và không chịu nổi việc trả tiền cho một
   danh sách không ai đọc.

**Test đọc thẳng danh sách, không đo thời gian.** Bốn cách làm sai đều cho ra engine chạy đúng và
chậm hơn đúng một timeout, nên test hiển nhiên là test thời gian — mà test thời gian cho một khác
biệt 100 ms trên runner dùng chung chính là loại flaky. Danh sách là **sự thật**; latency chỉ là
triệu chứng của nó.

**Sáu đảo ngược, sáu lần đỏ đúng chỗ** — sau khi một cái trong số đó lộ ra rằng test của tôi sai:

| Đảo ngược | Đỏ ở đâu |
|---|---|
| không bao giờ hỏi writable | `writable_is_asked_for_exactly_while_bytes_are_queued` |
| luôn hỏi writable | ba test cùng đỏ |
| nguồn thiếu bị bỏ im lặng, không đếm | `a_connection_with_no_source_is_counted` |
| `idle_with` bỏ qua nguồn phụ (quên listener) | `the_listener_reaches_the_set_idle_with_waits_on` |
| `Loopback` + `Block` | không biên dịch, đúng thông điệp const-assert |
| bỏ const-assert | doctest `compile_fail` **FAILED** |
| `rebuild` cấp phát `Vec` mới mỗi lượt | `benches/alloc.rs` → **interests 10000**, invariant đỏ |

**`[đo 2026-08-30]` Test listener bản đầu xanh cả khi tôi xoá đúng dòng nó phải canh.** Nó tự tay
ghép danh sách — `refresh_interests().to_vec()` rồi `push(listener)` — và khẳng định listener có
trong đó. Tất nhiên là có: chính nó đặt vào. Xoá `extend_from_slice(extra)` trong `idle_with` để
lại test **xanh**, vì test chưa từng đi tới dòng ấy. **Một test đặt tên theo hành vi nó không hề
chạm tới.** Sửa bằng cách thêm `refresh_interests_with` — đúng lời gọi mà `idle_with` dùng — rồi
cho test đi qua đó. Bài học ngắn gọn: *một test tự lắp ráp thứ nó đang kiểm là đang kiểm chính
nó.* `[to testing-skills]`

**Và một cái nữa bắt được nhờ triệu chứng đọc như thứ khác.** Test writable đầu tiên trượt với
danh sách interest **rỗng**, đọc hệt như "danh sách chưa bao giờ được dựng". Thật ra Logon của
test bị Reject vì lệch giờ — tôi **bịa** một timestamp thay vì dùng
`fixbolt_conformance::script::FIXED_TIME_IN`, mà engine trong test chạy `ManualClock` tại
`FIXED_TIME_MILLIS`. Connection bị bỏ, nên `conns` rỗng, nên danh sách rỗng. Đã ghi thẳng trong
rustdoc của hàm `logon()` trong test.

**Chưa làm:** waker cho `RingDispatch` (bước 5). Cho tới lúc đó, một reply sinh trên thread khác
vẫn chờ tới một timeout ở `standard` — cái ô thứ ba trong bảng bốn vực latency ở mục "Cách làm"
§4, vẫn còn nguyên. `serve()` vẫn dùng `Spin`, nên listener đã nối dây nhưng chưa được đọc; đổi
mode là bước 6.

**Gate cho commit này:**

```
cargo fmt --all --check                     sạch
cargo clippy --all-targets -- -D warnings   sạch
cargo test --all                            224 passed, 0 failed
cargo test --all --no-default-features      0 failed
scripts/check-no-optional-deps.sh           exit 0
crates/engine --test standard               13 passed; 0/40 lần đỏ khi chạy lặp
cargo test -p fixbolt-engine --doc          1 passed (compile_fail,E0080)
-p fixbolt-session --test score             59/59 trong process
-p fixbolt-engine  --test wire              59/59 qua socket thật
scripts/check-no-kernel-sleep.sh            exit 0; nửa đỏ trượt vì 2901 sched_yield
scripts/bench.sh                            exit 0; 8/8 target; 0 invariant failure
  engine alloc: idle 0 send 0 recv 0 frame 0 turn 0 busy 0 ring 0 interests 0
scripts/check-links.py                      289 link, 0 chết
```

Hai target vượt trần vẫn là `fixbolt-codec/groups` và `serialize` — mục 11 và 20. **Không đo gì
mới:** `Block` giờ đã ghép được với `Engine`, nhưng chưa có đường nào chạy nó đầu-cuối (`serve()`
và `w2w` đều còn `Spin`), nên `DESIGN.md` §8 vẫn giữ nhãn "lấy từ tài liệu".

### 2026-08-30 — bước 3 xong: `wait::Block`, và một quả mìn được gỡ bằng compile error

**Đã dựng.** `crates/engine/src/block.rs` sau `#[cfg(all(feature = "standard", unix))]`:
`Block` với `DEFAULT_TIMEOUT_MS = 100` và sàn `MIN_TIMEOUT_MS = 5`, `SLEEPS = true`,
**`NEEDS_SOURCES = true`** — strategy đầu tiên khai như vậy.

**Ba quyết định nhỏ, mỗi cái vì một lý do:**

1. **`EINTR` chờ tiếp phần thời gian còn lại, không chờ lại từ đầu.** Chờ lại đủ timeout thì một
   luồng tín hiệu kéo dài lượt rỗi vô hạn, và kéo theo hạt thời gian mà session nhìn thấy.
2. **Timeout 0 bị nâng lên sàn, không được tôn trọng.** Nó là một vòng quay đội tên mode này —
   ADR-0014 quyết định 6 đã liệt kê sẵn. Đọc lại bằng `timeout_ms()` nếu cần biết.
3. **Lỗi `poll` không trả về được thì phải quan sát được.** `Waiting::idle` trả `()`, nên một
   `poll` hỏng không có chỗ nào để đi. Hai thứ xảy ra và không cái nào là im lặng: **vẫn trả
   core lại** (ngủ nốt phần còn lại, chứ không biến mode này thành cái nó sinh ra để thay), và
   **lỗi được giữ** ở `last_error()`. Cùng nguyên tắc ADR-0011 đã chốt cho ring đầy: *lời từ
   chối không bao giờ im lặng.*

**Quả mìn, và cách gỡ.** `Block` khai `NEEDS_SOURCES = true` trong khi `Engine::idle` vẫn truyền
slice **rỗng**. Ghép hai cái lại cho ra một engine **đúng**: trả lời mọi tin, qua đủ 59 định
nghĩa, đọc 0% CPU — và **chậm 100 ms mỗi tin**. Không bộ test đúng-sai nào và không phép đo CPU
nào nhìn thấy nó. Nên nó bị chặn ở chỗ nó được viết ra: một `const assert!(!W::NEEDS_SOURCES)`
tạm thời trong `Engine::idle` và `run`, kèm doctest `compile_fail,E0080` canh vĩnh viễn. Bước 4
thay nó bằng assert thật của ADR-0014 quyết định 4, cùng lúc với danh sách nguồn thật.

**Năm đảo ngược, năm lần đỏ vì đúng lý do:**

| Đảo ngược | Đỏ ở đâu |
|---|---|
| `EINTR` coi là wakeup, trả về ngay | `a_signal_does_not_end_the_wait_early` |
| `poll` hỏng thì trả về ngay (thành vòng quay) | `a_failing_poll_is_recorded_and_still_sleeps` |
| `Ok(_)` không trả về — luôn chờ hết timeout | `bytes_wake_it_far_sooner_than_the_timeout`, hết 2.13 s |
| bỏ `const assert` khỏi `Engine::idle` | doctest `compile_fail` **FAILED** (nó biên dịch được) |
| bỏ `optional = true` khỏi `libc` | `check-no-optional-deps.sh` exit **1**, in cả cây |

**`compile_fail` xanh khi code hỏng vì bất kỳ lý do gì — và tôi đã vấp đúng cái đó.** Lần thử
đầu, file kiểm mìn không biên dịch được vì **trait bound sai** (`InlineDispatch<Store>` không hợp
lệ), chứ không phải vì const assert. Nó "đỏ" mà chẳng chứng minh gì. Sửa: dựng đúng kiểu `Engine`
hợp lệ, đọc thông điệp lỗi để xác nhận đó là câu của mình, rồi **ghim mã lỗi `E0080`** vào
doctest. Đây chính là §10 bản thu nhỏ: một kết quả đỏ cũng cần đúng lý do, y như một kết quả xanh.

**Một thứ nữa tự đến.** Test tín hiệu cần `pthread_kill`, mà integration test không thừa hưởng
dependency của crate — nên `libc` thành dev-dependency **không điều kiện**. Việc đó **làm cổng
`check-no-optional-deps.sh` chuyển sang trạng thái "không phân biệt được" và trượt**: thông điệp
của `cargo tree -i` đổi từ *"did not match any packages"* sang *"nothing to print"*, vì `libc`
giờ có trong đồ thị nhưng không qua cạnh `-e normal`. **Trượt là đúng** — một cổng không phân
biệt được thì tuyệt đối không được báo ok. Đã sửa để nhận cả hai thông điệp là "vắng mặt khỏi thứ
được ship", giữ nguyên nhánh từ chối đoán, và **chạy lại đảo ngược để chứng minh nó vẫn đỏ được**
(exit 1, in cả cây). Dev-dependency không tới tay người dùng, và cổng hỏi bằng `-e normal` nên
loại nó theo đúng cấu trúc — ghi thẳng trong manifest.

**Chưa làm:** danh sách nguồn thật, listener, `POLLOUT`, waker. Bước 4 và 5.

**Gate cho commit này — chạy và đọc output:**

```
cargo fmt --all --check                     sạch
cargo clippy --all-targets -- -D warnings   sạch (0 dòng)
cargo test --all                            220 passed, 0 failed
cargo test --all --no-default-features      0 failed
scripts/check-no-optional-deps.sh           exit 0; đảo ngược cho exit 1
crates/engine --test standard               9 passed; 0/40 lần đỏ khi chạy lặp
cargo test -p fixbolt-engine --doc          1 passed (compile_fail,E0080)
-p fixbolt-session --test score             59/59 trong process
-p fixbolt-engine  --test wire              59/59 qua socket thật
scripts/check-no-kernel-sleep.sh            exit 0; nửa đỏ trượt vì 2646 sched_yield
scripts/bench.sh                            exit 0; 8/8 target; 0 invariant failure
  engine alloc: idle 0 send 0 recv 0 frame 0 turn 0 busy 0 ring 0
```

Hai target vượt trần thời gian vẫn là `fixbolt-codec/groups` và `fixbolt-codec/serialize` — mục
11 và 20, crate này không đụng tới. **Không đo gì mới:** giá một lần thức của `standard` vẫn mang
nhãn "lấy từ tài liệu" — `Block` chưa nối vào `Engine` thì chưa có đường nào để đo nó.

### 2026-08-30 — bước 2 xong: chỗ nối mở, và hai cổng hoá ra đang canh nhầm thứ

**Đã dựng.** `Source`, `Interest`, `Transport::POLLABLE` + `source()`, `Waiting` đổi chữ ký kèm
`NEEDS_SOURCES`, `Park` → `Yield`, và module `poll` (`Poller`, `PollError`, `Ready`) sau
`#[cfg(all(feature = "standard", unix))]`. Dependency ngoài **đầu tiên** và khối `unsafe`
**đầu tiên** của crate, cả hai nằm sau feature. Test mới: `crates/engine/tests/standard.rs`.

**Sửa 1 — `libc` và `poll::Poller` bị kéo từ bước 3 lên bước 2.** Plan viết bước 2 là "chỉ mở chỗ
nối, chưa có `Block`", nhưng cách kiểm chứng của chính bước 2 lại là *"`poll` một lần với timeout
0 trên fd ấy"* — tức là cần `libc`. Không có nó, test khả dĩ duy nhất là so `source()` với
`as_raw_fd()`, mà **đó là so một hàm với chính thân nó**: `TcpTransport::source` *là*
`as_raw_fd`, nên phép so sẽ xanh kể cả với một phiên bản trả về fd của socket khác, miễn là nó
lấy sai theo cùng một cách. Nên bước 2 nhận cả syscall thô; bước 3 giờ là chính sách timeout,
`EINTR`, và `NEEDS_SOURCES = true`.

**Sửa 2 — `Source::from_raw_fd` phải là public, và đó là lỗ thiết kế chứ không phải tiện tay.**
Bản đầu chỉ có `as_raw_fd`. Nghĩa là **không crate nào ngoài repo dựng nổi một `Source`**, nên
`Transport::POLLABLE = true` không thể implement từ bên ngoài — `Transport` sẽ là trait chỉ trên
danh nghĩa. Nó an toàn (giữ sai số không phải unsound; `poll` trả `POLLNVAL`), nhưng hợp đồng
"fd phải còn sống" thì viết đậm trong rustdoc.

**Đảo ngược — mỗi guard đã được thấy đỏ vì đúng lý do:**

| Đảo ngược | Kết quả |
|---|---|
| `source()` trả fd hằng số `0` | `poll_can_tell_the_two_sockets_apart_by_their_source` **FAILED** |
| `Poller::wait` tin `rc` thay vì đếm cờ `revents` | `an_unknown_descriptor_is_an_error_and_not_a_quiet_socket` **FAILED** |
| bỏ `optional = true` khỏi `libc` | `check-no-optional-deps.sh` **FAILED**, in ra cả cây |

Ghi thêm một quan sát về đảo ngược thứ nhất: nó đỏ **luân phiên giữa hai test** qua các lần chạy,
vì fd 0 là stdin — một tài nguyên dùng chung mà test nào chộp trước thì test đó đỏ. Guard vẫn
đỏ, nhưng nếu ai đó chỉ chạy một lần rồi kết luận "test X canh cái này" thì kết luận đó sai.

**Hai thứ tìm ra khi làm, và cả hai đều là cổng đang canh nhầm:**

1. **Một test flaky, bắt được vì nó đỏ đúng lần chạy nguội đầu tiên.**
   `an_unknown_descriptor_...` bản đầu đóng một socket rồi hỏi về fd của nó. Đỏ 1 lần, rồi xanh
   30 lần liên tiếp. Vị trí panic chỉ thẳng nhánh `Ok(count == 0)`: **một thread test khác trong
   cùng binary đã được cấp lại đúng số fd đó**, nên fd hợp lệ, sống, và im lặng — mà "im lặng"
   không phân biệt được với "đã đóng" ở tầng này, tức là đúng thứ khẳng định kia định canh. Sửa
   không phải bằng retry: hỏi về `i32::MAX`, số mà tiến trình này không bao giờ được cấp. Sau đó
   **0/40 lần đỏ**.
2. **`[đo 2026-08-30]` Job CI `no-default-features` xanh về một bản build chưa từng xảy ra.**
   `cargo test --all --no-default-features` **vẫn build `libc`**, vì `tools/w2w` là thành viên
   workspace phụ thuộc `fixbolt-engine` với default features, và cargo hợp nhất feature trong một
   lần gọi. Cờ đang bị kiểm bị chính một crate anh em bật lại. Phát hiện **không phải nhờ cổng**
   mà nhờ **số test**: bản `--no-default-features` lẽ ra 210, nó ra 214 — đúng 4 test của
   `standard.rs`. Nếu module mới không mang test riêng, hai con số đã bằng nhau và không gì chỉ
   vào đó. Sửa: `scripts/check-no-optional-deps.sh`, hỏi **theo từng crate**, nối vào CI.
   Viết vào [feature-flags-unify-across-a-workspace.md](../reference/feature-flags-unify-across-a-workspace.md).
   `[to testing-skills]`

**Chưa làm, và vì sao:** `Engine::idle` vẫn truyền slice **rỗng**. An toàn *chỉ vì* chưa có
strategy nào khai `NEEDS_SOURCES = true`, và `const`-assert của quyết định 4 hạ cánh ở bước 4
cùng lúc với danh sách nguồn thật. Điều này ghi thẳng trong rustdoc của `Engine::idle` chứ không
để ai đó phải suy ra.

**Gate cho commit này — chạy và đọc output, không đọc exit code:**

```
cargo fmt --all --check                     sạch
cargo clippy --all-targets -- -D warnings   sạch
cargo test --all                            49 dòng test result, 214 passed, 0 failed
cargo test --all --no-default-features      0 failed  (và xem phát hiện 2 ở trên)
scripts/check-no-optional-deps.sh           ok — libc vắng mặt, crate vẫn build và test
cargo test -p fixbolt-session --test score  4 passed        (59/59 trong process)
cargo test -p fixbolt-engine  --test wire   1 passed        (59/59 qua socket thật)
scripts/check-no-kernel-sleep.sh            exit 0 — nửa xanh không có syscall chặn,
                                            nửa đỏ trượt vì 2861 sched_yield
scripts/bench.sh                            8/8 target đo được, 0 invariant failure
  engine alloc: idle 0 send 0 recv 0 frame 0 turn 0 busy 0 ring 0
```

`bench.sh` báo **2 target vượt trần thời gian: `fixbolt-codec/groups` và
`fixbolt-codec/serialize`**. Cả hai nằm trong `codec`, crate mà thay đổi này **không đụng tới một
dòng nào**, và cả hai đúng là các ca mục 11 và 20 đã gọi tên sẵn. Không phải của commit này.

**Không đo gì mới.** Giá một lần thức của `standard` vẫn mang nhãn "lấy từ tài liệu" trong
`DESIGN.md` §8 — chưa có `wait::Block` thì chưa có gì để đo.

### 2026-08-30 — plan được duyệt, bước 1 xong

**Duyệt 2026-08-30.** Bước 1 làm ngay sau đó.

**Đã dựng:** [`ADR-0014 — standard blocks on poll(2), and the waiter is given the sockets`](../decisions/ADR-0014-standard-mode-blocks-on-poll.md),
trạng thái **`Proposed`**. Chín quyết định, trả lời **cả bốn** câu hỏi mở của ADR-0013 đúng như
plan hứa. Hình dạng API đã chốt trong đó: `Source`, `Interest`, `Waiting::NEEDS_SOURCES`,
`Transport::POLLABLE` + `source()`, và const-assert trong thân `Engine::run`.

**Hai thứ tìm ra khi làm, không có trong plan lúc viết:**

1. **Không có `dyn Transport` / `dyn Waiting` / `dyn Dispatch` nào trong repo** —
   `grep -rn 'dyn ' crates/ tools/` trả về rỗng. Đây là điều kiện mà cả quyết định 3 lẫn 4 dựa
   vào: associated const làm trait mất object safety, và ở đây **không có gì đang dùng object
   safety để mà mất**. Nếu grep ra kết quả thì cả hai quyết định đã phải khác. Nó đã được kiểm,
   không phải được giả định — và hệ quả xấu của nó đã ghi vào mục Consequences của ADR.
2. **`docs/decisions/` không có ADR-0009, và đó là khoảng trống có chủ ý.** Plan
   `gates-that-can-be-trusted` xin số đó cho một thay đổi API của `SessionUnderTest::step`, rồi
   bỏ thiết kế và xoá hook thay vì ship — nhật ký của chính plan đó nói vậy. Nhưng lời giải thích
   chỉ nằm trong một plan tiếng Việt, còn người đi tìm sẽ tìm ở `docs/decisions/`. Ghi vào cả
   ADR-0014 lẫn plan này.

**Chưa làm, và vì sao:** **bước 2 chưa bắt đầu.** ADR-0014 là `Proposed`, và `CLAUDE.md` §5 nói
`Proposed` → `Accepted` là chữ ký của chủ dự án. Không có dòng code nào được viết cho tới lúc đó.
Cùng nếp mà ADR-0013 đã đi: đề xuất ở một commit, ký ở commit sau.

**Gate của commit này** — chỉ có tài liệu, **không đo gì mới**:

```
scripts/check-links.py  → 125 files, 284 internal links, no dead internal links
cargo test --all        → 48 test-result lines, 210 passed, 0 failed
```

`[đo 2026-08-30]` **Con số test ở commit trước là sai, và nó sai theo đúng kiểu §10 cảnh báo.**
Commit `fa87192` ghi *"30 test binaries"* — đó không phải số đo, đó là `head -30` cắt output rồi
người đọc đếm số dòng còn lại. Số thật ở cùng cây code là **48 dòng `test result`, 210 test
passed, 0 failed**. Không có gì hỏng, và đó mới là chỗ đáng sợ: **một con số bịa ra bởi chính lệnh
đi kiểm tra nó vẫn nói "xanh", nên không có gì mâu thuẫn để ai đó nhận ra.** Bài học đi vào
`docs/reference/` khi bước 7 đóng — cổng ở bước đó đọc `/proc` bằng shell và cắt output là rủi ro
y hệt. `[to testing-skills]`

Mọi con số trích trong ADR đều dẫn nguồn tới file hoặc lần chạy sinh ra nó. Giá một lần thức của
`standard` **vẫn mang nhãn "lấy từ tài liệu"** trong `DESIGN.md` §8 và là câu hỏi mở số 1 của
ADR-0014 — không có gì ở bước này đo được nó.
