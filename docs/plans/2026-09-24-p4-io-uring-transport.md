# Phase 4, hàng 5: transport `io_uring` cho cả `hft` lẫn `standard`

> **Loại:** Plan · **Ngày:** 2026-09-24 · **Trạng thái:** Đề xuất
> **Phạm vi:** phase 4, hàng 5 của bảng *Chia việc* trong
> [2026-09-23-phase-4-scope.md](2026-09-23-phase-4-scope.md); hạng mục 1 của
> [ADR-0098](../decisions/ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
> (kèm câu trả lời Q8 của anh); thiết kế ở
> [ADR-0190](../decisions/ADR-0190-the-io-uring-transport-is-reaped-by-the-idle-strategy-and-an-hft-turn-enters-the-kernel-once-without-waiting.md)
> và [ADR-0191](../decisions/ADR-0191-the-hft-sleeper-list-reads-io-uring-enter-by-its-min-complete.md),
> cả hai *Proposed*, duyệt cùng plan này. Hàng 7 (boot §9, A/B) **không** thuộc plan này; plan
> chỉ viết sẵn hàng 7 sẽ đo gì và vạch bỏ đọc thế nào (mục *Hàng 7 sẽ đo gì*).

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Hôm nay, mỗi vòng của engine đọc từng socket một lần bằng `read`. Socket nào im lặng thì lần
đọc đó chỉ để nghe kernel trả lời "chưa có gì" — và riêng việc vào rồi ra khỏi kernel đã tốn
khoảng một nửa của 703 ns mỗi lần (`measured-costs.md`, *The engine is syscall-bound*). Với 16
phiên trên một luồng, một vòng rảnh là 16 lần như vậy.

`io_uring` cho phép đăng ký trước: "khi socket này có dữ liệu thì chép vào vùng đệm này và báo
tôi". Mọi thông báo của mọi socket đổ vào **một** hàng hoàn tất (CQ). Vòng rảnh khi đó không cần
N lần `read` nữa — chỉ cần một lần vào kernel để gom mọi thông báo. Đó là điều ADR-0098 muốn đo.

Anh đã chọn hạng mục này và đã duyệt vạch bỏ trước khi có code (Q1, Q7): giữ lại chỉ khi độ trễ
dây p50 đo ở NIC tốt hơn ≥ 3 % ở cả hai procedure, **hoặc** vòng rảnh ở N = 16 tốt hơn ≥ 25 %.
Hàng này dựng transport, chứng minh nó đúng (59 / 59, hai script mode, không cấp phát), và dựng
sẵn mọi thứ hàng 7 cần để đo — không công bố con số độ trễ nào.

Khi viết plan này có bốn điều ADR-0098 chưa trả lời, và ADR-0190 trả lời chúng:

1. **Gom thông báo ở đâu?** Engine có một `Transport` cho mỗi kết nối và một `Waiting` cho cả
   luồng. Hàng hoàn tất thì một cho cả luồng. → Việc gom giao cho **chiến lược chờ** (`Waiting`),
   vì đó đúng là lời gọi engine làm khi một vòng không có gì để làm.
2. **Chỉ "nhìn CQ" từ userspace có đủ không?** **Không**, trừ khi dùng SQPOLL. Việc chép dữ liệu
   của một `recv` trên `io_uring` chạy trong ngữ cảnh luồng đã nộp lệnh, và chỉ chạy khi luồng đó
   vào kernel — hoặc khi kernel **ngắt** luồng đó để chạy (mặc định). Với cờ `DEFER_TASKRUN` thì
   không có ngắt, nhưng CQ sẽ trống mãi cho đến khi luồng gọi `io_uring_enter` với `GETEVENTS`.
   → `hft` gọi `io_uring_enter(min_complete = 0)` **một lần mỗi vòng rảnh**: vào kernel, không
   bao giờ chờ.
3. **Script `hft` hiện coi mọi `io_uring_enter` là ngủ.** → ADR-0191: script đọc tham số thứ ba
   (`min_complete`); bằng 0 là không chờ, khác 0 là chờ, không đọc được là FAIL.
4. **Bị chặn trông ra sao?** Docker ≥ 25 và sysctl `kernel.io_uring_disabled` đều trả `EPERM`.
   → Engine từ chối khởi động, nói rõ nguyên nhân, không bao giờ lặng lẽ quay về `read`.

## Những gì đã biết chắc

**Code, worktree `fb-p4r5`, commit `094bfc3`, ngày 2026-09-24:**

- `Transport` (`crates/engine/src/transport.rs:168-232`) có `recv`, `send`, `POLLABLE`,
  `source()`, `tls_mode()`, `handshake_refused()`; `TcpTransport` đọc bằng `read` không chặn
  (`transport.rs:267-311`).
- `Waiting` (`crates/engine/src/wait.rs:34-55`) có `SLEEPS`, `NEEDS_SOURCES`, `idle(&[Interest])`.
  `Spin::idle` chỉ là `spin_loop()`.
- `Engine::idle_with` (`lib.rs:1670-1706`) đã từ chối lúc biên dịch cặp *chiến lược cần danh
  sách nguồn + transport không có nguồn* bằng một khối `const`; sau khi chờ, nó xả pipe của
  waker (`#[cfg(all(feature = "standard", unix))]`). `Engine::run` gọi `idle` chỉ khi `turn()` trả
  `false` (`lib.rs:1721-1730`).
- `pump`/`pump_loop` (`lib.rs:3459-3570`) nhận một `wrap: FnMut(TcpTransport) -> Option<T>` — đường
  `serve_tls` dùng để biến socket vừa accept thành transport khác. Tập chờ Logon
  (`presession::PendingSet<T, …>`) cũng generic theo `T`.
- `ServeError` là `#[non_exhaustive]`, đã có biến thể gắn feature (`Tls`, `Affinity`) được thêm
  mà không phá API (`lib.rs:3217-3267`).
- `crates/engine/Cargo.toml`: `default = ["standard"]`; `--no-default-features` build ra engine
  **không dependency, không `unsafe`**, chỉ `hft`. `libc` là optional, dùng chung cho `standard`,
  `affinity`, `tls`.
- `scripts/check-no-kernel-sleep.sh:116`: `SLEEPERS` có `io_uring_enter`, so theo **tên**. Script
  đọc mode từ output (`ran_mode`), chỉ xét cửa sổ phục vụ (ADR-0152), và yêu cầu nửa đỏ.
- `scripts/check-no-kernel-sleep-by-ctxt.sh` đếm voluntary context switch của luồng engine,
  không cần tracer (ADR-0072). `scripts/check-standard-gives-the-core-back.sh` khẳng định bốn điều
  (mode đọc lại, CPU, trạng thái `S`, p50 xa dưới timeout).
- `crates/engine/tests/wire.rs`: 59 định nghĩa qua socket thật; harness `Wire<W>` gắn cứng
  `TcpTransport` (`wire.rs:170-195`), vòng `pump` gọi `idle_with` khi không có gì chuyển động
  (`wire.rs:302-333`); hai test `the_fifty_nine_definitions_pass_through_a_real_socket` (`Yield`)
  và `..._in_standard_mode_too` (`Block`, timeout 5 ms).
- `crates/engine/benches/turn.rs`: đo `engine.turn()` với N = 1, 4, 16 phiên TCP im lặng, cộng
  `recv on a quiet socket`; **không** đo `idle`.
- `tools/w2w` có `pump` riêng; `--mode hft|standard|yield`, `--tls`, `--listen`/`--connect`,
  `--wire-timestamps`; **từ chối** `--mode standard --wire-timestamps` trên NIC thật
  (`DESIGN.md` §3, dòng `tools/w2w`). `scripts/w2w-baseline.sh` nhận cờ thêm qua `W2W_EXTRA` và
  đọc lại danh tính mỗi lần chạy (`extra_val`, dòng 449-461).
- `scripts/check-feature-gated-tests-ran.sh` FAIL khi một test dưới feature bị `#[ignore]` (R3) —
  nên test chỉ chạy được trên máy bàn không thể là test `#[ignore]` dưới `io-uring`.

**Máy bàn `tmt-B450-I-AORUS-PRO-WIFI`, đọc 2026-09-24:** kernel `7.0.0-31-generic`;
`/proc/sys/kernel/io_uring_disabled` = `0`; `io_uring_group` = `-1`; có `strace`; **không có
`docker`**; có toolchain `nightly`; `/sys/devices/system/cpu/isolated` **rỗng** (đang ở dòng grub
desktop, không phải boot §9).

**Tài liệu kernel và crate** (nguồn đầy đủ trong ADR-0190 *Research*):

- `io_uring_setup(2)`: `SINGLE_ISSUER` từ 6.0; `DEFER_TASKRUN` từ 6.1, cần `SINGLE_ISSUER`,
  **không dùng chung được với SQPOLL**; mặc định kernel ngắt luồng đang chạy userspace khi có hoàn
  tất (`COOP_TASKRUN` tắt việc đó); `EPERM` khi `io_uring_disabled` = 2, hoặc = 1 mà không có
  `CAP_SYS_ADMIN`.
- `io_uring_enter(2)`: `GETEVENTS` chờ `min_complete` hoàn tất; `EXT_ARG` mang timeout; `EINTR`
  có thể xảy ra khi đang chờ.
- `io_uring_multishot(7)`, `io_uring_provided_buffers(7)`: `recv` multishot dừng khi lỗi, khi
  đóng, hoặc khi hết vùng đệm (`-ENOBUFS`, không có `IORING_CQE_F_MORE`) — phải nộp lại.
- liburing #568, `io_uring_cancelation(7)`: `close` **không** huỷ lệnh đang treo trên socket; lệnh
  giữ tham chiếu file, socket vẫn mở, phía bên kia không nhận FIN.
- Docker 25.0 bỏ `io_uring_setup`, `io_uring_enter`, `io_uring_register` khỏi seccomp mặc định;
  profile đó trả `defaultErrnoRet: 1` (**EPERM**). containerd `RuntimeDefault` cũng vậy.
- Crate `io-uring` 0.7.15 (2026-09-07): Rust thuần, binding dựng sẵn (bindgen chỉ sau feature
  `overwrite`), `rust-version = "1.63"`, dependency `bitflags`, `cfg-if`, `libc`. `RecvMulti` cần
  kernel 6.0. `register_buf_ring_with_flags`, `Submitter::enter`, `SubmissionQueue::push` là
  `unsafe`. PR #408 (merge 2026-09-06) sửa lỗi `submit()` không gửi `GETEVENTS` dưới
  `DEFER_TASKRUN`.
- Kết quả đo công khai: một kết nối, tin 64 B, epoll 1 565 K QPS so với io_uring 506 K (liburing
  #536); ping-pong 8 byte: registered buffers không giúp tin nhỏ, SQPOLL thua DeferTR khi có NAPI
  (arXiv 2512.04859). **Không tìm thấy** số đo nào của một FIX engine trên `io_uring`.

## Cách làm

Chỉ ghi phương án được chọn; phương án bị loại nằm ở ADR-0190 và ADR-0191.

### 1. Feature và dependency

- `crates/engine/Cargo.toml`: feature `io-uring = ["dep:io-uring", "dep:libc"]`, tắt mặc định;
  `[target.'cfg(target_os = "linux")'.dependencies] io-uring = { version = "0.7.15", optional = true }`.
  Thêm `"io-uring"` vào `[package.metadata.docs.rs] features`.
- `crates/engine/src/transport.rs`:
  `#[cfg(all(feature = "io-uring", target_os = "linux"))] pub mod uring;` — **gate đặt ngay trên
  `mod`** (bất biến 6). File mới `crates/engine/src/transport/uring.rs`.
- Phần `standard` của transport (`UringBlock`, `serve_uring`) cần thêm `feature = "standard"`,
  vì cơ chế xả waker chỉ tồn tại dưới `standard`.
- `tools/w2w/Cargo.toml`: feature `io-uring = ["fixbolt-engine/io-uring"]`, không vào `default`.

### 2. Kiểu dữ liệu (API công khai mới)

- `UringConfig` — số vùng đệm (luỹ thừa của 2), độ dài mỗi vùng, số kết nối tối đa; kiểm hợp lệ
  khi tạo, không có giá trị ẩn (`CLAUDE.md` §6).
- `HftArm::{Enter, Sqpoll { core }}` — `Enter` là mặc định; `Sqpoll` phải được gọi tên rõ.
- `Uring` — sở hữu ring, vùng đệm và bảng kết nối; `Rc`, không `Send`. `Uring::hft(cfg, arm)`
  trả `(Uring, UringSpin)`; `Uring::standard(cfg)` trả `(Uring, UringBlock)` — cấu hình của
  `standard` **không có trường SQPOLL**, nên "standard + SQPOLL" không viết ra được.
  `Uring::register(TcpTransport) -> Option<UringTransport>`; `Uring::report() -> UringReport`
  (số hoàn tất đã gom, số byte, số lần `ENOBUFS`, số lần nộp lại, arm thật sự đang chạy — đọc
  từ tham số kernel trả về, không từ cờ).
- `UringTransport: Transport` — `recv` chép từ danh sách đã gom của kết nối vào buffer người gọi,
  trả vùng đệm về ring, **không syscall**; `send` là `write(2)` như cũ; `POLLABLE = true`.
- `UringSpin: Waiting` (`SLEEPS = false`) và `UringBlock: Waiting` (`SLEEPS = true`,
  `NEEDS_SOURCES = true`).
- `UringRefused::{Disabled { sysctl }, Blocked, NotInKernel, KernelTooOld, Other(ErrorKind)}`,
  `Display` nói người vận hành phải đi đâu (bảng ở ADR-0190 quyết định 7).
- Hai hằng có mặc định, **không phá code bên ngoài**: `Transport::NEEDS_REAPER = false`,
  `Waiting::REAPS = false`; `Engine::new` khẳng định `!T::NEEDS_REAPER || W::REAPS` trong khối
  `const`. Ghép `UringTransport` với `Spin` hay `Block` là **lỗi biên dịch** — nếu không, engine
  sẽ chạy và không bao giờ nhận được byte nào.
- `ServeError::Uring(UringRefused)` dưới cùng `cfg` với `mod uring`.
- Hai điểm vào: `serve_hft_uring` (tham số như `serve_hft`, cộng `UringConfig`, `HftArm`) và
  `serve_uring` (như `serve`, cộng `UringConfig`). Cả hai tạo ring **trước khi bind listener**,
  rồi gọi `pump` có sẵn với `wrap = |t| uring.register(t)`.

### 3. Cơ chế (chi tiết ở ADR-0190)

- **Ring**: `SINGLE_ISSUER | DEFER_TASKRUN` (arm `Enter` và `standard`); `SQPOLL | SQ_AFF` với
  `sq_thread_cpu = core` đã qua `affinity::Topology` (arm `Sqpoll`). Kiểm opcode bằng probe
  (`RECV`, `POLL_ADD`, `ASYNC_CANCEL`). Kernel sàn: 6.1.
- **Nhận**: mỗi kết nối một `RecvMulti`, chọn vùng đệm từ một buffer ring (đăng ký một lần). Bộ nhớ
  vùng đệm cấp một lần, căn trang, **ghi vào từng trang một lần lúc khởi động** (chạm trước).
  Mỗi `user_data` mang chỉ số khe **và thế hệ** của kết nối; hoàn tất của kết nối đã bị bỏ thì bị
  vứt, không đến tay kết nối mới dùng lại khe đó. Hết vùng đệm → `ENOBUFS` → nộp lại ngay khi
  có vùng trả về; dữ liệu chưa nhận vẫn nằm trong socket (TCP tự hãm), không mất.
- **Gửi**: không đổi — `write(2)` trên socket không chặn.
- **`hft`, arm `Enter`**: `UringSpin::idle` gọi thẳng `Submitter::enter(to_submit, 0,
  IORING_ENTER_GETEVENTS)` (không qua `submit()` của crate, vì lỗi PR #408), rồi gom CQ.
  **Không chờ, không bị ngắt.**
- **`hft`, arm `Sqpoll`**: `idle` chỉ nhìn CQ; khi kernel bật `IORING_SQ_NEED_WAKEUP` thì gọi
  `enter` với `SQ_WAKEUP`, `min_complete = 0`. Đốt thêm một lõi — mọi con số từ arm này ghi rõ.
- **`standard`**: `UringBlock::idle` gọi `enter(to_submit, 1, GETEVENTS | EXT_ARG)` với timeout
  100 ms (hoặc `with_timeout_ms` cho test). `EINTR` là một lần thức, không phải lỗi. Nguồn nào
  không phải kết nối đã đăng ký (listener, pipe của waker) và mọi nguồn `writable` được gắn một
  `POLL_ADD` **một lần** (không multishot), ghi trong bảng cố định để không gắn trùng; nguồn đã
  gắn mà biến khỏi danh sách của vòng này thì bị huỷ theo `user_data`.
- **Đóng kết nối**: `shutdown(SHUT_RDWR)` (dừng `recv` đang treo, phía kia nhận FIN), xếp một
  `ASYNC_CANCEL` theo `user_data` cho lần `idle` sau, trả danh sách đã gom về ring, tăng thế hệ.
  **Huỷ `Uring`**: gỡ đăng ký buffer ring **trước** khi giải phóng bộ nhớ của nó.

### 4. `tools/w2w` và bench

- `tools/w2w/src/main.rs`: `--transport kernel|uring` (mặc định `kernel`), `--uring-arm
  enter|sqpoll`, `--sqpoll-core <cpu>`. In một dòng đọc lại **từ `Uring::report()`**:
  `transport: uring arm=<enter|sqpoll> cqes=<n> bytes=<n> enobufs=<n>` (và `transport: kernel` cho
  arm cũ). Từ chối: `--transport uring` khi thiếu feature hoặc không phải Linux; `--uring-arm
  sqpoll` với `--mode standard`; `--sqpoll-core` khi thiếu `--uring-arm sqpoll`. Đếm cấp phát hai
  luồng như cũ.
- `crates/engine/benches/turn.rs`: **giữ nguyên** các case cũ; thêm, dưới `cfg(feature =
  "io-uring")`, cặp case *idle loop, N idle sessions, kernel* (`turn()` + `Spin::idle`) và
  *…, uring* (`turn()` + `UringSpin::idle`), N = 1, 16, 64. So đúng "một vòng rảnh" ở cả hai arm,
  vì với uring `turn()` không còn syscall nào — so riêng `turn()` sẽ là so sai.
- `crates/engine/benches/alloc.rs`: case `uring-exchange` (Logon, `NewOrderSingle` →
  `ExecutionReport` qua `UringTransport` + `UringSpin`), đếm luồng engine = 0, tự khẳng định
  đường chạy là thật (`report().cqes > 0`).
- `scripts/w2w-baseline.sh`: đọc lại `transport:` như đã đọc `journal:`/`log:` — nếu `W2W_EXTRA`
  có `--transport uring` mà output không có `transport: uring … cqes=<n>` với n > 0 thì FAIL.

### 5. Script mode (ADR-0191)

- `scripts/check-no-kernel-sleep.sh`: phân loại `io_uring_enter` theo tham số thứ ba thành
  `io_uring_enter_nowait` / `_wait` / `_unparsed`; `SLEEPERS` thay `io_uring_enter` bằng hai tên
  sau. Thêm lần chạy `--mode hft --transport uring` (phải xanh **và** có `_nowait` > 0), lần
  `--mode standard --transport uring` (phải đỏ với `_wait`), lần arm SQPOLL chỉ khi có
  `FIXBOLT_SQPOLL_CORE`, ngược lại in *SKIPPED, NOT PASSED*.
- `scripts/check-no-kernel-sleep-by-ctxt.sh`: thêm `hft` + uring (voluntary 0) và `standard` +
  uring (phải đỏ).
- `scripts/check-standard-gives-the-core-back.sh`: thêm `standard` + uring với đủ bốn khẳng
  định cộng dòng `transport:` đọc lại; nửa đỏ `hft` + uring phải vấp.
- `scripts/check-no-optional-deps.sh`: `io-uring`, `bitflags`, `cfg-if` không có trong cây mặc
  định và `--no-default-features`; có khi bật `io-uring`.
- `scripts/check-uring-refused-under-sysctl.sh` (mới, **chỉ máy bàn**, cần `sudo -n`): đặt
  `kernel.io_uring_disabled=2`, chạy `w2w --transport uring`, đòi exit khác 0 với câu của
  `UringRefused::Disabled`, **luôn khôi phục** giá trị cũ (`trap`), in lại giá trị sau khi khôi
  phục. Là script chứ không là test `#[ignore]`, vì R3 của `check-feature-gated-tests-ran.sh`.
- `.github/workflows/ci.yml`: bước `io-uring` — `cargo test -p fixbolt-engine --tests --features
  io-uring` qua `check-feature-gated-tests-ran.sh`; `cargo test -p fixbolt-engine
  --no-default-features --features io-uring`; clippy với feature; build `w2w --features
  io-uring`, chạy ba script mode ở trên; in `uname -r` và `io_uring_disabled` của runner đầu job.

## Bất biến bị đụng tới

- **1 — không cấp phát trên hot path.** Ring, vùng đệm, bảng kết nối, bảng `POLL_ADD` cấp **lúc
  tạo `Uring`**; `register` chỉ lấy một khe có sẵn; clone `Rc` không cấp phát. Chứng minh:
  case `uring-exchange` trong `benches/alloc.rs` đọc 0 và đỏ khi tiêm
  `std::hint::black_box(Vec::<u8>::with_capacity(1))` vào đường gom; `tools/w2w --transport
  uring` đếm 0 trên cả hai luồng. Vùng đệm được chạm trước: test
  `buffers_are_resident_before_the_first_message` đọc `VmRSS` tăng ≥ tổng dung lượng vùng đệm
  ngay sau `Uring::hft`.
- **2 — session thuần.** Không đụng `crates/session`.
- **3 — 59 / 59.** Hai test mới trong `crates/engine/tests/wire.rs`: `hft` + uring và `standard` +
  uring, cùng harness, cùng `LIFELINE_HITS == 0`. Hai test cũ giữ nguyên thân.
- **4 — mode, cả hai nửa.** `hft`: `io_uring_enter` chỉ với `min_complete = 0`; không ngắt (nhờ
  `DEFER_TASKRUN`); ctxt 0. `standard`: chờ trong `io_uring_enter(min_complete = 1)` có timeout;
  SQPOLL không viết ra được. Cả ba script chạy với transport mới và phải đỏ khi đổi mode. Thay
  đổi gate hft có ADR-0191 và được chứng minh bằng đảo ngược.
- **5 — thứ tự field.** Không đổi.
- **6 — feature chặn chính `mod`.** `#[cfg]` trên `pub mod uring;`; dependency chỉ trên Linux;
  crate `io-uring` không chạy bindgen trừ feature `overwrite` của nó (không bật); CI build
  `--no-default-features --features io-uring` và `check-no-optional-deps.sh` phủ ba crate mới.
- **7 — không panic.** File mới dùng `get`/`checked_*`; `scripts/check-indexing-debt.sh` không
  tăng; `id`/độ dài từ CQE được kiểm biên, sai thì `Io::Failed`, không panic.
- **8 — `unsafe` cần plan và tên của thứ chứng minh nó.** Mỗi khối có
  `#[allow(unsafe_code)]` cục bộ và comment `SAFETY:` nêu tên test (mẫu `poll.rs:20-26, 118-127`).
  Miri **không** chạy được `io_uring`, và **không sanitizer nào thấy kernel ghi vào bộ nhớ** — nên
  thứ chứng minh là một sổ sở hữu vùng đệm được kiểm trong test cộng các chạy đối chiếu từng byte:

  | # | Chỗ `unsafe` | Điều phải đúng | Thứ chứng minh |
  |---|---|---|---|
  | U1 | `std::alloc::alloc`/`dealloc` bộ nhớ vùng đệm căn trang | layout khớp khi cấp và khi trả | `buffers_are_resident_before_the_first_message`; chạy ASan nightly (bước 7) |
  | U2 | `register_buf_ring_with_flags` | bộ nhớ sống đến khi gỡ đăng ký hoặc ring bị huỷ | thứ tự trường + `Drop` tường minh; `unregistered_buffers_are_not_written_after_the_ring_is_dropped` (ghi mẫu canary sau khi huỷ, gửi thêm dữ liệu vào socket cũ, mẫu phải còn nguyên) |
  | U3 | `SubmissionQueue::push` (`RecvMulti`, `PollAdd`, `AsyncCancel`) | mọi bộ nhớ SQE trỏ tới sống đến khi hoàn tất | các SQE này không trỏ vào bộ nhớ người dùng nào ngoài buffer ring (U2); `a_ring_that_runs_out_of_buffers_rearms_and_loses_nothing` |
  | U4 | `Submitter::enter` (cả `EXT_ARG` với timeout trên stack) | con trỏ tham số sống suốt lời gọi đồng bộ | `standard_wakes_on_its_own_timeout_to_tick` (timeout được đọc đúng), `standard_is_woken_by_the_data_not_the_timeout` |
  | U5 | tạo slice từ `(buffer id, độ dài)` của CQE | `id < số vùng`, `độ dài ≤ độ dài vùng`, vùng đang thuộc userspace | hàm kiểm biên thuần có test riêng; sổ sở hữu kiểm sau mỗi lần gom trong `sixty_four_connections_interleaved_are_byte_exact` |
  | U6 | ghi `tail` của buffer ring (trả vùng về kernel) | chỉ trả vùng không còn ai đọc, mỗi vùng một lần | sổ sở hữu (mỗi id ở đúng một chỗ: trong kernel, hoặc trong đúng một danh sách kết nối); test đối chiếu từng byte |
  | U7 | `libc::shutdown` trên fd của kết nối | fd còn được sở hữu | `a_closed_connection_is_seen_as_closed_and_its_peer_sees_fin` |

  Ngoài thư viện: test từ chối cài một bộ lọc seccomp thật trên luồng của chính nó (`prctl`,
  `seccomp`) — `unsafe` trong file test, có comment.
- **9 — không chép QuickFIX.** Không đụng.
- **10 — số đo.** Plan này **không công bố con số độ trễ nào**. Số đếm (cấp phát, context
  switch, hoàn tất) không phải số độ trễ. Mọi con số độ trễ thuộc hàng 7.

## Chia việc

Toàn bộ là **một** pull request, **một** senior developer (opus) làm bước 1–7 liên tục (bước sau
gửi tiếp bằng `SendMessage` cho cùng agent — cùng các file `engine`, một người ghi một file).
Bước 8 là docs (sonnet), bước 9 review. Manager chạy lại gate đóng mỗi bước và commit; developer
không commit. **Phụ thuộc chung:** bước 0 của plan phạm vi phase 4 (phase 3 đã đóng) và việc
duyệt plan này cùng ADR-0190, ADR-0191.

| Bước | Kết quả | Người làm | File được sửa / không được sửa | Gate | Phụ thuộc |
|---|---|---|---|---|---|
| 1 | **Khung + test đỏ.** Feature, dependency, `transport/uring.rs` chỉ có kiểu và chữ ký, thân trả `Err(UringRefused::Other(..))` / `Io::Idle` (không panic). `crates/engine/tests/uring.rs` với mọi test ở *Cách kiểm chứng* mục 2–4, trừ `serve_hft_uring_under_seccomp_binds_no_socket` (nó gọi điểm vào của bước 4, nên được viết đỏ ở đầu bước 4); hai test mới trong `wire.rs` (harness được làm generic theo transport **và** hàm `wrap`; thân hai test cũ không đổi). Kiểm `io-uring` 0.7.15 có PR #408 và build trên 1.89 | senior developer (opus) | Sửa: `crates/engine/{Cargo.toml, src/transport.rs, src/transport/uring.rs (mới), tests/uring.rs (mới), tests/wire.rs}`, `Cargo.lock`. **Không**: `src/lib.rs`, `src/wait.rs`, `crates/session/`, `scripts/`, `.github/` | `cargo test -p fixbolt-engine --features io-uring --test uring --test wire` **đỏ**, trích nguyên văn, câu FAIL mong đợi viết trước (xem *Cách kiểm chứng* mục 1); `cargo test --all`, `cargo test --no-default-features` xanh; `cargo +1.89 check -p fixbolt-engine --features io-uring` (cài 1.89 nếu thiếu) | duyệt plan |
| 2 | **Lõi ring + `hft`.** `Uring::hft` (arm `Enter` và `Sqpoll`), probe, phân loại từ chối, buffer ring chạm trước, `RecvMulti`, gom, thế hệ, `UringTransport`, `UringSpin`, đóng kết nối, huỷ ring; bảy chỗ `unsafe` với comment `SAFETY:` | senior developer, cùng agent | Như bước 1, chỉ `src/transport/uring.rs`, `tests/uring.rs` | `cargo test -p fixbolt-engine --features io-uring --test uring` — mọi test `hft`, từ chối, byte-exact, đóng, canary xanh; test `standard` còn đỏ; `cargo clippy -p fixbolt-engine --all-targets --features io-uring -- -D warnings`; `scripts/check-indexing-debt.sh`; `scripts/check-no-crate-root-allow.sh` | 1 |
| 3 | **`standard`.** `Uring::standard`, `UringBlock` (`EXT_ARG`, `POLL_ADD` một lần, bảng gắn, huỷ khi nguồn biến mất); hai hằng `NEEDS_REAPER`/`REAPS`, khẳng định `const` trong `Engine::new`; ba doctest `compile_fail` (`UringTransport` + `Spin`, `UringTransport` + `Block`, `standard` + SQPOLL) | senior developer, cùng agent | Sửa: `src/transport/uring.rs`, `src/transport.rs`, `src/wait.rs`, `src/lib.rs` (chỉ `Engine::new`), `tests/uring.rs`. **Không**: `turn`, `pump`, `presession.rs`, `block.rs`, `poll.rs` | `cargo test -p fixbolt-engine --features io-uring --test uring` xanh hết; `cargo test -p fixbolt-engine --features io-uring --doc`; `cargo test --all`; `cargo test --no-default-features` | 2 |
| 4 | **Điểm vào + 59 / 59 + alloc.** Trước tiên viết `serve_hft_uring_under_seccomp_binds_no_socket` và cho thấy nó đỏ; rồi `serve_hft_uring`, `serve_uring`, `ServeError::Uring`; test không-quay-về (`serve_hft_uring_under_seccomp_binds_no_socket`); case `uring-exchange` trong `benches/alloc.rs` | senior developer, cùng agent | Sửa: `src/lib.rs` (điểm vào, `ServeError`), `benches/alloc.rs`, `tests/uring.rs`. **Không**: `tests/wire.rs` (đã xong ở bước 1), `crates/session/` | `cargo test -p fixbolt-engine --features io-uring --test wire` **59 / 59 ở cả bốn test**, `lifeline hit: 0`; `cargo bench -p fixbolt-engine --features io-uring --bench alloc` (`uring-exchange 0`, mọi case cũ vẫn 0); `cargo test -p fixbolt-session --test score` 59 / 59; `cargo test -p fixbolt-engine --features fix50sp2 --test wire_fixt` không đổi | 3 |
| 5 | **`w2w` + bench.** Cờ `--transport`, `--uring-arm`, `--sqpoll-core`, dòng đọc lại, các lời từ chối; case mới trong `turn.rs`; đọc lại `transport:` trong `w2w-baseline.sh` | senior developer, cùng agent | Sửa: `tools/w2w/{Cargo.toml, src/main.rs}`, `crates/engine/benches/turn.rs`, `scripts/w2w-baseline.sh`. **Không**: `tools/w2w/src/pair.rs`, case cũ của `turn.rs` | `cargo build --release -p fixbolt-w2w --features io-uring`; `target/release/w2w --mode hft --transport uring` và `--mode standard --transport uring` in `transport: uring … cqes=<n>` với n > 0 và `allocs 0`; `cargo bench -p fixbolt-engine --features io-uring --bench turn` chạy ra đủ case (số **không** công bố); `scripts/check-w2w-baseline-summary.sh` xanh | 4 |
| 6 | **Gate.** ADR-0191 trong `check-no-kernel-sleep.sh`; lần chạy uring trong ba script mode; `check-no-optional-deps.sh`; `check-uring-refused-under-sysctl.sh` (mới); bước `io-uring` trong `ci.yml` | senior developer, cùng agent | Sửa: bốn script nêu tên + một script mới, `.github/workflows/ci.yml`. **Không**: `crates/` | Ba script mode xanh trên Linux, **trích đủ output** kể cả nửa đỏ; `scripts/check-no-optional-deps.sh` xanh | 5 |
| 7 | **Đảo ngược + máy bàn.** R1–R10 ở *Cách kiểm chứng* mục 6, mỗi cái viết câu FAIL trước, đỏ, khôi phục, xanh; trên máy bàn: `check-uring-refused-under-sysctl.sh`, arm SQPOLL qua ba script với `FIXBOLT_SQPOLL_CORE` (và `--allow-unisolated` nếu chưa ở boot §9, ghi rõ), `tests/uring.rs` dưới ASan nightly | senior developer, cùng agent; manager kiểm máy bàn không có phiên khác đang đo | Như bước 2–6; mọi đảo ngược khôi phục, `git diff` cuối giống hệt trước bước 7 | Từng đảo ngược đỏ đúng câu đã ghi; `RUSTFLAGS=-Zsanitizer=address cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu -p fixbolt-engine --features io-uring --test uring` xanh (thiếu nightly → ghi *SKIPPED, NOT PASSED*, không phải xanh) | 6 |
| 8 | **Tài liệu**, cùng commit với code: danh sách ở *Tài liệu phải cập nhật*; ADR-0190/0191 → `Accepted` (manager ghi dòng trạng thái) | developer (sonnet); runner (haiku) chạy `check-links.py` | Chỉ `docs/`, `CHANGELOG.md`, `README.md` nếu cần. **Không** `crates/`, `STATUS.md`, `CLAUDE.md` (manager/anh sửa dòng *Machine checks*) | `python3 scripts/check-links.py`; `scripts/check-adr-numbers.sh` | 7 |
| 9 | **Review + CI**: một senior review, context mới, được đưa plan này, ADR-0190/0191 và các gate; manager kiểm từng phát hiện theo `CLAUDE.md` §12; PR nháp mở từ commit đầu; CI xanh trên commit đóng, ghi run id | senior developer (opus) review; manager | — | CI xanh trên commit đóng, run id vào *Nhật ký giao hàng* | 8 |

## Cách kiểm chứng

| # | Tiêu chí | Lệnh | Đạt khi |
|---|---|---|---|
| 1 | Đỏ trên code chưa viết | bước 1 | Mọi test `uring.rs` đỏ ở khẳng định của chính nó (vd. `a_message_arrives_through_the_ring`: *"Uring::hft refused: …"*); hai test uring của `wire.rs` đỏ với *"over io_uring: 0 / 59"* hoặc lỗi dựng ring; test từ chối dưới seccomp đỏ vì khung trả `Other`, không phải `Blocked`. Không test nào đỏ vì panic của harness |
| 2 | Đúng dữ liệu | `cargo test -p fixbolt-engine --features io-uring --test uring` | `a_message_arrives_through_the_ring` (`report().cqes > 0`); `a_ring_that_runs_out_of_buffers_rearms_and_loses_nothing` (2 vùng, 1 MiB dồn dập, byte-exact, `enobufs > 0` **và** `rearms > 0` — đường chạy thật); `sixty_four_connections_interleaved_are_byte_exact` (khúc ngẫu nhiên, hạt giống in ra, sổ sở hữu kiểm sau mỗi lần gom); `a_late_completion_for_a_dropped_connection_reaches_nobody`; `a_closed_connection_is_seen_as_closed_and_its_peer_sees_fin` (phía kia đọc 0 trong 1 s); `buffers_are_resident_before_the_first_message`; `unregistered_buffers_are_not_written_after_the_ring_is_dropped` |
| 3 | `standard` thức vì dữ liệu, không vì timeout | cùng lệnh | `standard_is_woken_by_the_data_not_the_timeout` (timeout 10 s, trả lời < 1 s); `standard_is_woken_by_a_connect`; `standard_is_woken_by_the_waker` (trả lời từ luồng ứng dụng); `standard_wakes_on_its_own_timeout_to_tick` (timeout 50 ms, `idle` trở về trong [40, 500] ms) |
| 4 | Bị chặn thì từ chối, có tên, không quay về | cùng lệnh; `scripts/check-uring-refused-under-sysctl.sh` trên máy bàn | `a_blocked_io_uring_refuses_to_start_and_names_seccomp` (bộ lọc seccomp trả EPERM cho `io_uring_setup` trên luồng test → `UringRefused::Blocked`, câu có chữ *seccomp*); `serve_hft_uring_under_seccomp_binds_no_socket` (trả `ServeError::Uring`, không cổng nào nghe); `the_refusal_is_classified_by_errno_and_sysctl` (bảng thuần: EPERM+0→`Blocked`, EPERM+1/2→`Disabled`, ENOSYS→`NotInKernel`, EINVAL→`KernelTooOld`); script sysctl: exit ≠ 0, câu `Disabled { sysctl: 2 }`, giá trị sau khôi phục đọc lại = `0` |
| 5 | Ghép sai không biên dịch | `cargo test -p fixbolt-engine --features io-uring --doc` | ba doctest `compile_fail` xanh — và **đảo ngược R8**: bỏ khối `const` thì doctest thứ nhất biên dịch được, tức đỏ |
| 6 | Đảo ngược (bước 7) | từng cái | **R1** `UringSpin` dùng `min_complete = 1` → `check-no-kernel-sleep.sh` FAIL *"the engine thread slept in the kernel"* có `io_uring_enter_wait`, và ctxt script đỏ; **R2** dòng trace sửa tay có tham số thứ ba không đọc được → FAIL có `io_uring_enter_unparsed`; **R3** `UringBlock` dùng `min_complete = 0` → `check-standard-gives-the-core-back.sh` đỏ ở khẳng định CPU/trạng thái `S`; **R4** bỏ `POLL_ADD` cho listener → `standard_is_woken_by_a_connect` đỏ; **R5** bỏ tăng thế hệ → `a_late_completion_…` đỏ; **R6** bỏ `shutdown` khi đóng → `…_peer_sees_fin` đỏ; **R7** bỏ nộp lại sau `ENOBUFS` → `…_rearms_and_loses_nothing` đỏ (hết thời gian, byte thiếu); **R8** như mục 5; **R9** tiêm `black_box(Vec::with_capacity(1))` vào đường gom → `uring-exchange` > 0; **R10** bỏ chạm trước vùng đệm → `buffers_are_resident_…` đỏ |
| 7 | Mode, cả hai nửa | ba script mode | `hft` + uring xanh, `io_uring_enter_nowait` > 0, ctxt 0; `standard` + uring vấp script `hft` với `_wait` và vấp ctxt; `standard` + uring qua đủ bốn khẳng định; `hft` + uring vấp script `standard` |
| 8 | Không phá gì đã có | `cargo test --all`; `cargo test --no-default-features`; `cargo test -p fixbolt-engine --no-default-features --features io-uring`; `cargo clippy --all-targets -- -D warnings` (và với `--features io-uring`); `cargo fmt --check`; `scripts/check-lint-config.sh`; `scripts/check-indexing-debt.sh`; `scripts/check-no-crate-root-allow.sh`; `scripts/check-no-optional-deps.sh`; `cargo semver-checks` | xanh; test có sẵn **không sửa** (riêng harness `wire.rs` được làm generic, thân hai test cũ giữ nguyên, vẫn 59 / 59) |
| 9 | CI thấy test dưới feature đã chạy | bước `io-uring` của CI | `check-feature-gated-tests-ran.sh fixbolt-engine io-uring <log>` xanh; đầu job in `uname -r` và `io_uring_disabled` của runner |

"Bản ghi thật": 59 định nghĩa QuickFIX qua socket kernel thật, `Logon` có checksum thật; không
capture nào của đối tác được dùng hay commit. Không có con số độ trễ nào ở đây.

## Hàng 7 sẽ đo gì (không thuộc plan này)

Viết trước code, theo ADR-0190 quyết định 10. Một boot §9, cả hai binary build sẵn và ghi sha256
(ADR-0090), `check-machine.sh` trước mỗi procedure, hai procedure cách nhau ≥ 30 phút, **thứ tự
arm đảo ngược ở procedure hai** (ADR-0068):

| Arm | Lệnh | Vai trò |
|---|---|---|
| **K** | `scripts/w2w-baseline.sh`, `ARMS="hft:admin"`, `W2W_EXTRA="--wire-timestamps --nic enp9s0 --observer-core <c>"`, bên phát là Mac mini, interval 0, 20 000 request × 10 lần | đối chứng |
| **U** | như K, thêm `--transport uring` vào `W2W_EXTRA` | **được xét** |
| **S** | như U, thêm `--uring-arm sqpoll --sqpoll-core <lõi cô lập>` | ghi cạnh, **một mình không giữ được hạng mục** |
| **idle** | `cargo bench -p fixbolt-engine --features io-uring --bench turn`, ghim vào lõi engine; *idle loop, 16 idle sessions, kernel* với *…, uring* (in cả N = 1, 64) | **được xét** ở N = 16 |
| **std** | engine `--listen --mode standard [--transport uring]`, bảng *as the counterparty sees it* đo từ Mac | chỉ quyết nửa `standard` |
| **density** | `cargo bench -p fixbolt-engine --features io-uring --bench density` | ghi lại, không xét |

**Vạch giữ/bỏ:**

- **Giữ** nếu (a) p50 dây admin của U ≤ 0,97 × K ở **cả hai** procedure và p99 của U ≤ 1,05 × K ở
  cả hai; **hoặc** (b) vòng rảnh N = 16 của uring ≤ 0,75 × kernel ở cả hai procedure **và** p50
  dây admin của U ≤ 1,05 × K ở cả hai (5 % là *band* của ADR-0068 quyết định 2).
- **S một mình không giữ được.** Chỉ S qua vạch → hạng mục bị bỏ, cặp số của S được ghi (kèm
  tên lõi thứ hai bị đốt), báo anh; muốn SQPOLL thành mặc định thì cần ADR mới thay Q8.
- **Nửa `standard`**: nếu p50 phía Mac với uring tệ hơn không uring > 5 % ở cả hai procedure thì
  bỏ `UringBlock` và `serve_uring`, transport chỉ còn `hft`.
- **Bỏ** nghĩa là: xoá feature, module, cờ `w2w`, lần chạy trong script, ngay trên nhánh của hàng
  7; mọi cặp số vào `measured-costs.md`; ADR-0190 ghi kết quả.

## Tài liệu phải cập nhật

Theo bảng đồng bộ ở `CLAUDE.md` §4, đi từng dòng:

- [ ] `docs/DESIGN.md` — D5 (transport thứ hai sau feature, Linux, kernel ≥ 6.1); D8 (*As built*
  cho `UringSpin`/`UringBlock`, SQPOLL chỉ `hft`); §3 bảng module (`transport::uring`) và dòng
  `tools/w2w` (cờ mới); §6 dòng *never sleeps in the kernel (`hft`)* (ADR-0191) và dòng
  `standard`; §8 **không đổi** (chưa có số)
- [ ] `docs/GUIDE.md` — `io_uring` tắt mặc định; Docker ≥ 25 / containerd chặn, sysctl, cách
  engine từ chối và câu nó in; kernel ≥ 6.1; SQPOLL đốt thêm một lõi và chỉ `hft`; không dùng
  chung với TLS, sharded, initiator ở bản này
- [ ] `docs/CONFIGURATION.md` — feature `io-uring`; `UringConfig` (số vùng, độ dài, số kết nối);
  `HftArm`; timeout 100 ms của `UringBlock`
- [ ] `CHANGELOG.md` `[Unreleased]` — *Added*: feature, module, `serve_uring`, `serve_hft_uring`,
  `ServeError::Uring`, `Transport::NEEDS_REAPER`, `Waiting::REAPS`; cờ `w2w`
- [ ] `docs/internals/engine.md` — `transport/uring.rs`: file giữ gì, thứ tự đọc, test canh
- [ ] `docs/best-practices-hft.md`, `docs/best-practices-standard.md` — khi nào thử `io_uring`
  (nhiều phiên một luồng), nói rõ chưa có số đo
- [ ] `docs/reference/` — trang mới *"a CQ peek alone never receives: io_uring completion work
  runs on a kernel entry or an interrupt"* và *"closing a socket does not cancel its io_uring
  receive"*, mỗi trang nêu test canh
- [ ] `docs/decisions/ADR-0190`, `ADR-0191` → `Accepted` khi plan được duyệt
- [ ] `CLAUDE.md` §2 bảng *Machine checks* dòng 4 — ghi chú `io_uring_enter` xét theo
  `min_complete` — **manager đề xuất, anh sửa** (file quy tắc)
- [ ] `STATUS.md` — manager viết (handoff, *Not proven*)
- Không đổi: `PRD.md` (phase 4 đã ghi), `README.md` (không thêm crate), `docs/hft-playbook.md`,
  `DESIGN.md` §9, `docs/CONFORMANCE.md` (59 / 59 là cùng con số, thêm transport — ghi một dòng
  nếu trang đó liệt kê transport), `docs/SESSION-BEHAVIOUR.md`

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Chỉ nhìn CQ từ userspace mà không có SQPOLL → không bao giờ thấy hoàn tất (`DEFER_TASKRUN`) hoặc bị ngắt mỗi tin | `a_message_arrives_through_the_ring`; R1; trang `docs/reference/` mới |
| `submit()` của crate không gửi `GETEVENTS` dưới `DEFER_TASKRUN` (PR #408) | `UringSpin` gọi thẳng `enter` với `GETEVENTS`; `a_message_arrives_through_the_ring` chạy ở chế độ không chờ |
| `io_uring` bị seccomp chặn → lặng lẽ quay về `read`, số đo nói dối | `a_blocked_io_uring_refuses_to_start_and_names_seccomp`, `serve_hft_uring_under_seccomp_binds_no_socket`; dòng `transport:` đọc lại trong mọi script |
| `standard` + uring spin (SQPOLL, hoặc `min_complete = 0`) | SQPOLL không viết ra được (`compile_fail`); `check-standard-gives-the-core-back.sh`; R3 |
| `hft` + uring ngủ trong `io_uring_enter` | ADR-0191 trong `check-no-kernel-sleep.sh`; ctxt script; R1 |
| Ghép `UringTransport` với `Spin`/`Block` → engine chạy, không bao giờ nhận byte | khối `const` trong `Engine::new`; doctest `compile_fail`; R8 |
| Hết vùng đệm → multishot dừng, không ai nộp lại → kết nối im mãi | `a_ring_that_runs_out_of_buffers_rearms_and_loses_nothing`; R7 |
| `close` không huỷ `recv` đang treo → socket không đóng, đối tác không nhận FIN | `a_closed_connection_is_seen_as_closed_and_its_peer_sees_fin`; R6 |
| Hoàn tất muộn của kết nối cũ rơi vào kết nối mới dùng lại khe/fd | `a_late_completion_for_a_dropped_connection_reaches_nobody`; R5 |
| Trả một vùng về kernel khi vẫn còn đọc dở → dữ liệu bị ghi đè âm thầm | sổ sở hữu trong `sixty_four_connections_interleaved_are_byte_exact` |
| Giải phóng bộ nhớ vùng đệm trước khi gỡ đăng ký → kernel ghi vào bộ nhớ đã trả | `unregistered_buffers_are_not_written_after_the_ring_is_dropped` |
| `alloc_zeroed` trả trang chưa chạm → lỗi trang trên hot path | `buffers_are_resident_before_the_first_message`; R10 |
| Listener hay waker không được gắn `POLL_ADD` → `standard` chỉ thức theo timeout | `standard_is_woken_by_a_connect`, `standard_is_woken_by_the_waker`; R4 |
| Nguồn không phải kết nối bị đóng rồi fd được dùng lại trong khi `POLL_ADD` cũ còn treo | huỷ theo `user_data` khi nguồn biến khỏi danh sách; `standard_is_woken_by_a_connect` chạy hai lần liên tiếp với listener mới |
| Trace `strace -f` tách `io_uring_enter` thành `<unfinished>`/`<resumed>` → đếm hai lần hoặc đọc sai tham số | ADR-0191 quyết định 2; R2 |
| So `turn()` của uring (không syscall) với `turn()` của kernel → thắng giả 100 % | case bench mới đo `turn() + idle()` ở cả hai arm; case cũ giữ nguyên |
| Feature `io-uring` không có `standard` → xả waker không tồn tại | `UringBlock` gate thêm `feature = "standard"`; CI build `--no-default-features --features io-uring` |
| Test dưới feature không bao giờ chạy trên CI | `check-feature-gated-tests-ran.sh` trong bước CI; không dùng `#[ignore]` |
| MSRV: crate mới hoặc API mới cần Rust > 1.89 | `cargo +1.89 check … --features io-uring` ở bước 1; job `package` của CI |
| Máy bàn đang có phiên khác đo, hoặc sysctl bị bỏ ở 2 | manager kiểm trước bước 7; script sysctl khôi phục bằng `trap` và in giá trị sau |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Runner CI chặn `io_uring` (sysctl hoặc sandbox) → bước `io-uring` chỉ còn test từ chối | Trung bình | đầu job in `io_uring_disabled`; job **đỏ** nếu test cần ring không chạy (`check-feature-gated-tests-ran.sh`), không bao giờ xanh vì bỏ qua; nếu runner chặn thật, manager dừng và báo — không có chỗ nào được hạ gate |
| Arm dây một phiên thua (liburing #536) | Cao | đó là lý do có vạch bỏ; arm vòng rảnh N = 16 là đường giữ lại có khả năng hơn |
| `io_uring` là bề mặt tấn công mới | Trung bình | tắt mặc định, từ chối khi bị chặn, `GUIDE.md` nói thẳng |
| Không công cụ nào kiểm được kernel ghi vào bộ nhớ vùng đệm | Trung bình | sổ sở hữu + test byte-exact + canary + ASan cho phía userspace; ghi rõ giới hạn trong ADR-0190 *Consequences* |
| Tin nhận được chờ thêm một vòng userspace sau một vòng có việc | Thấp | chấp nhận có chủ đích; arm U của hàng 7 cho thấy cái giá |
| `[chưa kiểm trên kernel này]` NAPI busy poll không chạy trong `enter` với `min_complete = 0` | Thấp | NAPI không dựng ở hàng này (ADR-0190 quyết định 4) |
| Gate phụ thuộc định dạng tham số của `strace` | Thấp | không đọc được → FAIL (đỏ, không xanh); R2 |
| Hai hằng mới trên trait công khai làm `cargo semver-checks` kêu | Thấp | cả hai có mặc định (thay đổi minor); gate `semver-checks` chạy ở bước 9 |

## Ngoài phạm vi

- **Mọi con số độ trễ** — thuộc hàng 7.
- **Runtime sharded** (`serve_sharded_hft`), **initiator** (`connect_and_serve`), **TLS/kTLS** trên
  uring, **khoá trong file settings** — mỗi cái là việc tiếp theo chỉ khi hàng 7 giữ hạng mục.
- **Gửi qua ring**, **accept multishot**, registered files, registered buffers, zero-copy
  receive.
- **NAPI busy poll** (kể cả SQPOLL + NAPI) — plan riêng chỉ khi arm S tới gần vạch.
- **Arm "chỉ nhìn CQ, chịu ngắt"** — bị loại ở ADR-0190 quyết định 9 (§9: engine không bao giờ
  nhận ngắt).
- `docs/hft-playbook.md` và `DESIGN.md` §9 — không dòng máy nào đổi.
- Chạy engine trong Docker thật — máy bàn không có Docker; seccomp trên luồng test là cùng cơ chế.

## Nhật ký giao hàng

*(Manager ghi vào đây khi đóng từng bước: commit, gate đã chạy, CI run id, cái gì chưa chứng
minh.)*
