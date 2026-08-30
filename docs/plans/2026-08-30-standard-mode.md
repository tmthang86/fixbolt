# `standard` mode — engine chặn khi rỗi và trả core lại

> **Loại:** Plan · **Ngày:** 2026-08-30 · **Trạng thái:** **Chờ duyệt**
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
- `docs/decisions/ADR-00NN-…` — bước 1, xem ghi chú về số hiệu bên dưới
- `docs/DESIGN.md` D8, D5, §3, §6, §8 · `docs/GUIDE.md` §0 · `CHANGELOG.md` · `STATUS.md` · `CLAUDE.md` §2

> **Số hiệu ADR.** Plan [threads-and-affinity](2026-08-30-threads-and-affinity.md) cũng có bước 1 là
> một ADR, và `STATUS.md` gọi nó là ADR-0014. `CLAUDE.md` §5: số hiệu **không bao giờ dùng lại**.
> Nên plan nào viết ADR trước thì lấy 0014, plan kia lấy 0015. Kiểm bằng `ls docs/decisions/` ngay
> trước khi tạo file, đừng tin con số ghi ở đây.

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
| 1 | **ADR** — cơ chế readiness và dependency (`poll(2)` + `libc`), phạm vi Windows, số phận của `Park`, mặc định của `w2w`/bench, `density`. Trả lời cả bốn câu hỏi mở của ADR-0013. Chốt hình dạng API | ADR-0013 (đã ký) |
| 2 | `Source`, `Transport::source()`, `POLLABLE`; `Waiting` đổi chữ ký; `Spin` giữ nguyên hành vi; `Park` → `Yield` với rustdoc nói nó trượt cả hai cổng. **Chưa có `Block`** — bước này chỉ mở chỗ nối và phải giữ mọi test xanh | 1 |
| 3 | `wait::Block` sau feature `standard`: `poll(2)` với timeout, một khối `unsafe`, `EINTR` là quay lại chờ, lỗi có kiểu. `libc` chỉ trong feature | 2 |
| 4 | Engine dựng tập interest: readable luôn, **writable khi `has_pending_output()`**. `Acceptor::source()`, và `serve()` đăng ký listener. Const-assert từ chối transport không chờ được | 3 |
| 5 | Waker: self-pipe trong tập poll, `RingDispatch` đánh thức khi push | 4 |
| 6 | `w2w --mode standard\|hft`, mặc định `hft`, in mode mỗi lần chạy. Nửa đỏ của `check-no-kernel-sleep.sh` chuyển sang `--mode standard` | 4 |
| 7 | **Cổng mới** `scripts/check-standard-gives-the-core-back.sh` + job CI `standard-blocks`. Nửa xanh `standard`, **nửa đỏ `hft` và phải trượt** | 6 |
| 8 | Chạy 59 định nghĩa ở **cả hai mode**. Đo giá một lần thức của `standard` trên máy §9 và thay dòng §8 — hoặc giữ nhãn "từ tài liệu" nếu máy chưa `pass 10 fail 0` | 7, máy §9 |

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

- [ ] `docs/decisions/ADR-00NN-…` — bước 1 (dependency mới + đảo một câu hỏi mở → ADR bắt buộc)
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

*(chưa duyệt, chưa bắt đầu bước nào)*
