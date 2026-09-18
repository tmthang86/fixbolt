# Hỏi listener thưa hơn hỏi session — và đo nó ra một con số latency

> **Loại:** Plan · **Ngày:** 2026-09-18 · **Trạng thái:** Đã duyệt 2026-09-18 (owner: theo đề nghị cho cả ba câu hỏi)
> **Phạm vi:** `engine` (vòng lặp `pump` của `hft`), `tools/w2w`, STATUS item 89

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Boot B, bước B9 (`docs/reference/measured-costs.md`, *B9 — perf on the engine thread*): trong
một lần chạy `hft` loopback, thread engine dành **49,9 %** (admin) / **36,7 %** (app) số mẫu
`perf` cho `Acceptor::accept` → `accept4` trên một listener **trống**. Mỗi lần hỏi, kernel cấp
phát một socket file + inode rồi huỷ đi trước khi trả `EAGAIN`.

Con số đó là **tỷ lệ của vòng quay**, không phải chi phí mỗi tin. Thread quay không ngừng, nên
"một nửa số mẫu" chỉ nói rằng `accept4` đắt hơn một vòng `turn` rỗng, chứ chưa nói nó làm chậm
bao nhiêu cái khoảnh khắc giữa lúc tin đến socket và lúc engine đọc được. STATUS item 89 giao cho
kiến trúc sư hai câu hỏi: **có nên hỏi listener thưa hơn hỏi session không, và đo cái đó ra là
gì**.

Kết quả muốn có sau plan này:

1. Vòng `pump` của `hft` hỏi listener theo **nhịp** (mỗi N vòng), `standard` **không đổi**.
2. Một cánh tay `w2w` đo được (i) round trip khi listener được hỏi mỗi vòng so với mỗi N vòng,
   và (ii) độ trễ chấp nhận một kết nối mới — hai con số latency thật, thay cho một tỷ lệ mẫu.
3. Giá trị mặc định của N **chỉ được đặt sau khi có số đo trên bàn §9** (boot C). Trước đó
   mặc định là 1, tức là hành vi hôm nay, không đổi một byte nào trên đường nóng.

## Những gì đã biết chắc

### Từ code và tài liệu trong repo — đọc 2026-09-18

- **Nơi duy nhất `hft` gọi `accept()` trên đường nóng là `pump`**, `crates/engine/src/lib.rs`
  ~3070–3160: mỗi vòng `while set.len() < limits.pending() { acceptor.accept() … }` rồi
  `set.turn`, rồi `engine.turn()`, rồi `engine.idle_with(&extra)` nếu không có gì động. `serve`,
  `serve_hft`, `serve_tls` đều đi qua hàm này ("một vòng lặp, hai mode khác nhau đúng một
  kiểu"). `tools/w2w/src/main.rs` có bản `pump` riêng (~2310–2400) cùng hình dạng:
  `while let Some(t) = acceptor.accept() { … }` mỗi vòng.
- **`serve_sharded_hft` không bị ảnh hưởng**: `crates/engine/src/shard.rs` có một acceptor
  thread riêng được phép block (đầu file, *Why the acceptor thread is allowed to block*); các
  shard thread không bao giờ gọi `accept4`.
- **`Acceptor::accept`** (`lib.rs` ~3269) là `listener.accept()` non-blocking, `Err → None`.
- **`standard` đưa listener vào poll set**: `pump` ghép `listener` vào `extra` rồi
  `engine.idle_with(&extra)`; `Engine::idle_with` (`lib.rs` ~1495) chỉ dựng danh sách khi
  `W::NEEDS_SOURCES`. D8 (DESIGN.md §4) ghi: *"`serve` hands the listener to the poller, so a
  connection is accepted on the connect rather than on the next timeout."*
- **Chi phí một vòng `turn`**: DESIGN.md §8 bảng *Stage by stage* — `[measured 2026-08-31]`
  ~449 ns × N session (`benches/turn.rs`). Vòng `turn` với 0 session chưa có số.
- **Round trip loopback `hft` admin p50 ~15,1–16,0 µs, app ~19,0–20,2 µs** (DESIGN.md §8
  *Boot B*), và **9/10 cánh tay interval-0 không tái lập được giữa hai procedure (lệch 4,7–6,7 %)**.
  Nghĩa là: một hiệu ứng dưới ~5 % phải được đo **A/B xen kẽ trong cùng một procedure** mới có
  ý nghĩa, không được so hai procedure khác giờ.
- **Gate rule 4 nửa `hft`**: `scripts/check-no-kernel-sleep.sh` dòng ~96–98:
  `accept4`, `recvfrom`, `sendto` **được phép** (đường socket, non-blocking); `SLEEPERS` =
  `epoll_wait|epoll_pwait|epoll_pwait2|poll|ppoll|select|pselect6|futex|nanosleep|clock_nanosleep|sched_yield|io_uring_enter`.
  Bớt số lần `accept4` không làm gate đổi màu; thêm `poll(…, 0)` hay `io_uring_enter` thì đỏ.
- **`w2w --interval <us>`** (main.rs ~136, ~879–960): giữa hai lần gửi, client quay chờ; đã có
  hai cánh tay paced 1 ms / 10 ms trong `scripts/w2w-baseline.sh` (Boot B). `w2w` logon trước
  cửa sổ đo; chưa in thời gian từ `connect()` tới `Logon` trả lời.
- **ADR-0068**: một con số công bố = hai procedure đặt cạnh nhau, tái lập = hai median lệch ≤ 5 %.
- **ADR-0012 / ADR-0013**: `hft` quay, `standard` block; rule 4 có hai nửa và một phép đo không
  nêu mode là chưa đủ.

### Từ internet — tra 2026-09-18, mỗi dòng một nguồn

- **Cái giá EAGAIN là cố hữu của `accept4`, mọi phiên bản kernel.** `do_accept` trong
  `net/socket.c` (torvalds master) gọi `sock_alloc()` → `sock_alloc_file()` →
  `security_socket_accept()` → **rồi mới** `ops->accept()`; queue rỗng chỉ được phát hiện ở bước
  cuối, sau khi file + inode đã cấp phát, và `out_fd` huỷ chúng. Nguồn:
  <https://raw.githubusercontent.com/torvalds/linux/master/net/socket.c>. B9 đo đúng cái này.
- **Không hỏi được "queue có gì không" mà không trả giá syscall.** `tcp_ioctl` trả `-EINVAL` cho
  `SIOCINQ` lẫn `SIOCOUTQ` khi `sk_state == TCP_LISTEN`:
  <https://raw.githubusercontent.com/torvalds/linux/master/net/ipv4/tcp.c>. `poll(fd, 1, 0)` là
  một syscall khác (không cấp phát, nhưng vẫn ~vài trăm ns) và tên nó nằm trong `SLEEPERS`.
- **io_uring multishot accept** (kernel ≥ 5.19): một lần đăng ký, kernel tự gắn lại, CQE khi có
  kết nối — bỏ được vòng `accept4` lặp: <https://man7.org/linux/man-pages/man3/io_uring_prep_multishot_accept.3.html>,
  <https://lwn.net/Articles/868303/>. Nhưng cần `io_uring_enter` (trong `SLEEPERS`), một
  dependency mới, `unsafe`, chỉ Linux, và vẫn là tách listener khỏi vòng session. Ghi ở *Ngoài
  phạm vi*.
- **Onload**: socket non-blocking "always return immediately, unaffected by spinning"; spin của
  Onload chỉ áp cho `accept()` blocking (`EF_SPIN_USEC`/`EF_POLL_USEC`). Không có hướng dẫn hỏi
  listener thưa hơn: <https://docs.amd.com/r/en-US/ug1586-onload-user/EF_SPIN_USEC>.
- **Seastar**: reactor gọi "poll" — việc đắt — theo chu kỳ ~0,5 ms chứ không mỗi vòng, và ngừng
  poll I/O khi backlog task quá ngưỡng: <https://github.com/scylladb/seastar/issues/652>,
  <https://groups.google.com/g/seastar-dev/c/YCi-jbD4TC4>. Đây là mẫu "việc điều hành ở nhịp
  thưa hơn việc dữ liệu".
- **Aeron**: Sender/Receiver là agent riêng với idle strategy riêng; Conductor (điều hành, timer)
  chạy duty cycle riêng và "có một khoảng giữa các lần kiểm tra timer": <https://aeron.io/docs/aeron/media-driver/>,
  <https://aeron.io/docs/agrona/agents-idle-strategies/>. Cùng mẫu: đường dữ liệu và đường
  điều hành không chung nhịp.
- **Không tìm thấy** engine FIX hay busy-poll server nào công bố con số "accept4 trên listener
  trống làm chậm mỗi tin bao nhiêu". Nghĩa là số đo ở bước 5 dưới đây là số đầu tiên repo này
  có, và phải được công bố theo ADR-0068.

## Cách làm

Phương án chọn: **(a) nhịp đếm vòng, N cố định, đọc từ cấu hình, mặc định 1**; áp **chỉ cho
nửa spin**. Lý do (các phương án khác thuộc ADR-0069):

- Đếm vòng không cần đọc đồng hồ, không cần trạng thái nào ngoài một `u32`; chi phí trên đường
  nóng là một phép trừ và một nhánh. Nhịp thời gian (b) phải đọc `now` từ `pump` — có sẵn, nhưng
  đổi ý nghĩa theo tải; đếm vòng ràng buộc rõ hơn: **độ trễ chấp nhận kết nối tối đa = N × một
  vòng `turn`**, tức ≈ N × 449 ns × số session.
- Giữ nguyên (c) thì item 89 đóng mà không có số latency nào; plan này đo trước rồi mới quyết.
- Tách listener sang thread blocking riêng (như `shard.rs`) là (d), triệt để nhất (0 syscall
  cho listener trên thread engine) nhưng đổi hợp đồng "`serve_hft` không sinh thread"; chỉ xét
  nếu (a) đo được lợi mà N lớn vẫn chưa đủ — câu hỏi Q3.

### Thiết kế

1. **Một nút `listener_every`** trên `presession::Limits`: `NonZeroU32`, mặc định 1, builder
   `Limits::listener_every(self, n)`. `Limits::new(pending, logon_ms)` giữ nguyên chữ ký. Khoá
   settings `listener_every_turns` trong `settings.rs`, mặc định 1.
2. **Trong `pump`** (`lib.rs`): thêm bộ đếm cục bộ `until_listener: u32`. Mỗi vòng:
   `if until_listener == 0 { accept-loop như hôm nay; until_listener = every - 1 } else { until_listener -= 1 }`.
   **Sau mỗi lần `idle_with` trả về, đặt `until_listener = 0`** — vì ở `standard`, `poll` thức
   dậy có thể là do listener readable; bỏ qua accept lúc đó là vòng lặp nóng (`poll` trả ngay
   mãi mãi) = vi phạm rule 4 nửa `standard`. Với `Spin`, `idle` trả ngay nên đường này cũng
   đúng nhưng vô hại: khi rảnh hoàn toàn, listener vẫn được hỏi mỗi vòng — **đó là chủ ý**: nhịp
   chỉ thưa khi có việc (có tin đang chảy), là lúc `accept4` chen vào giữa hai tin.
   > Ghi chú cho người đọc: nghĩa là khi engine hoàn toàn rảnh, N không tiết kiệm gì. Cái nó
   > tiết kiệm là `accept4` xen vào **giữa** hai tin liên tiếp — đúng cái B9 nhìn thấy khi
   > 400 000 round trip chảy qua.
3. **`tools/w2w`**: cờ `--listener-every <N>` (mặc định 1), truyền vào bản `pump` của w2w theo
   cùng quy tắc (đếm vòng, reset về 0 sau `idle_with`). Thêm dòng in `logon-rtt <ns>`: thời gian
   từ ngay trước `connect()` của client tới khi nhận được `Logon` trả lời, đo bằng cùng đồng hồ
   của cửa sổ đo. Đây là (ii) — độ trễ chấp nhận kết nối mới, nhìn từ phía client.
4. **`scripts/w2w-baseline.sh`**: biến `LISTENER_EVERY` (mặc định rỗng = không truyền cờ), để
   cánh tay A/B chạy xen kẽ `1` và `N` trong **một** procedure.
5. **Đo trên bàn §9 (boot C, bước riêng, cờ riêng)**: `hft` admin + app, loopback, interval 0,
   `N ∈ {1, 16, 256}`, xen kẽ trong một procedure, rồi procedure thứ hai theo ADR-0068. Ghi
   p50/p99/p99.9 và `logon-rtt`. Mặc định của `listener_every_turns` được đặt **trong commit
   công bố số đo**, không sớm hơn.

Files tạo/sửa: `crates/engine/src/presession.rs` (Limits), `crates/engine/src/settings.rs`,
`crates/engine/src/lib.rs` (`pump`), `crates/engine/tests/listener_cadence.rs` (mới),
`tools/w2w/src/main.rs`, `scripts/w2w-baseline.sh`, `docs/decisions/ADR-0069-*.md` (mới),
và các tài liệu ở mục *Tài liệu phải cập nhật*.

## Bất biến bị đụng tới

| §2 | Bị đụng thế nào | Giữ bằng cách |
|---|---|---|
| 1 — không cấp phát trên đường nóng | `pump` thêm một `u32`; không heap | `benches/alloc.rs` case hiện có vẫn đọc 0; `tools/w2w` đếm allocation cả hai thread trong cửa sổ đo, vẫn phải 0 |
| 4 — `hft` không ngủ, `standard` phải block | Nhịp thưa **chỉ** áp khi có việc; sau `idle_with` luôn hỏi listener | `check-no-kernel-sleep.sh` (không đổi: `accept4` vốn được phép), `check-standard-gives-the-core-back.sh` chạy với `--listener-every 64`, test `standard_accepts_on_the_wake_not_after_n_wakeups` |
| 7 — không `unwrap`/`panic` | `NonZeroU32::new(...)` trả `Option`, xử lý bằng `LimitError` | clippy `-D warnings`, `check-lint-config.sh` |
| 10 — không số nào thiếu benchmark/máy/§9 | Mặc định N=1 cho tới khi boot C có số | commit đặt mặc định phải trích run id và `check-machine.sh` |

Gate rule 4 **không cần sửa**: `accept4` đã nằm ngoài `SLEEPERS`, và số lần gọi ít đi không phải
thứ gate đó nhìn. Không thêm `poll(0)` hay `io_uring_enter`, nên cũng không có gì mới để cho phép.

## Chia việc

| Bước | Kết quả | Ai / model | Phụ thuộc |
|---|---|---|---|
| 1 | `docs/decisions/ADR-0069-the-listener-is-polled-on-a-cadence-in-hft.md`, trạng thái Proposed: bốn phương án (a)–(d), chọn (a), hệ quả tốt/xấu (kể cả "khi rảnh N không tiết kiệm gì" và "độ trễ accept tối đa = N × turn") | architect | — |
| 2 | `Limits::listener_every` + khoá `listener_every_turns` trong `settings.rs`, test `limits_refuse_listener_every_zero`, `settings_parse_listener_every_turns`. Touch: `presession.rs`, `settings.rs`, test của hai file đó. Không touch `lib.rs` | developer (sonnet) | 1 |
| 3 | `pump` đếm vòng theo thiết kế §Cách làm 2. Test mới `crates/engine/tests/listener_cadence.rs`: (i) `hft_accepts_within_n_turns_while_a_session_is_busy` — một session bơm tin liên tục, một `connect()` mới, được `add` trong ≤ N vòng; (ii) `standard_accepts_on_the_wake_not_after_n_wakeups` — `Block`, N=64, `connect()` một lần, engine accept ở vòng ngay sau wake, và **không** quay (đếm số lần `idle` trả về giữa connect và accept ≤ 1); (iii) `listener_every_one_is_todays_loop` — N=1 chạy 59 def qua socket như `tests/wire.rs`. Touch: `lib.rs` chỉ trong `pump`; file test mới. Không touch `shard.rs`, `block.rs`, `wait.rs` | senior developer (opus) | 2 |
| 4 | `w2w --listener-every`, dòng `logon-rtt`, `LISTENER_EVERY` trong `w2w-baseline.sh`; `--listen`/`--connect` từ chối cờ này giống cách từ chối `--mode` (main.rs ~840–860). Test hiện có của w2w vẫn xanh; test mới `listener_every_is_refused_by_connect` | developer (sonnet) | 3 |
| 5 | **Cần bàn §9 — boot C.** Chạy A/B N ∈ {1,16,256} xen kẽ, 2 procedure, `hft` admin+app, ghi `check-machine.sh` trước mỗi run; ghi `logon-rtt`; đặt mặc định N nếu có lợi ≥ 5 % và tái lập; chuyển ADR-0069 sang Accepted với con số | manager, trên bàn; runner (haiku) chạy và trích | 4, có bàn |
| 6 | Cập nhật tài liệu theo mục dưới; STATUS item 89 | manager | 5 (hoặc 4 nếu bàn chưa có — ghi rõ "chưa đo") |

Bước 1–4 làm được trên macOS. Bước 5 là bước duy nhất cần Linux; chủ sở hữu hiện không có bàn,
nên plan này có thể đóng ở bước 4 + 6 với mặc định 1 và STATUS ghi "đo ở boot C".

## Cách kiểm chứng

- Bước 2, 3, 4: `cargo test -p fixbolt-engine listener_ cadence limits_ settings_` và
  `cargo test -p fixbolt-w2w`; `cargo clippy --all-targets -- -D warnings`;
  `cargo test --no-default-features` (đường `Spin` không có `standard`).
- Bước 3 thêm: 59 def qua socket (`crates/conformance`, cả in-process lẫn socket) — đây là thay
  đổi session-layer-adjacent, §7 bắt buộc. `benches/alloc.rs` đọc 0 (job `bench` CI).
- **Đảo ngược** (§7): trong `pump`, bỏ dòng `until_listener = 0` sau `idle_with`; câu FAIL
  mong đợi: `standard_accepts_on_the_wake_not_after_n_wakeups` đỏ với "idle returned 64 times
  before the connection was accepted"; `check-standard-gives-the-core-back.sh` với
  `--listener-every 64` phải đỏ ở dòng CPU. Khôi phục, xanh lại.
- Bước 5 (Linux): `scripts/check-no-kernel-sleep.sh` với `W2W_EXTRA="--listener-every 256"`
  và không có — cả hai xanh; `strace -c` đếm `accept4` trên tid engine giảm ~N lần so với N=1
  trong cửa sổ có tải (đọc output, không suy diễn). Kết quả A/B ghi vào
  `docs/reference/measured-costs.md` theo ADR-0068.

## Tài liệu phải cập nhật

Theo bảng đồng bộ ở `CLAUDE.md` §4.

- [ ] `DESIGN.md` §4 D8 — đoạn *As built, the shared loop*: listener theo nhịp ở nửa spin, luôn
      sau wake ở nửa block; §8 bảng *Stage by stage* thêm dòng "listener poll" khi có số.
- [ ] `docs/decisions/ADR-0069-*.md` — mới (bước 1), Accepted ở bước 5.
- [ ] `docs/CONFIGURATION.md` — khoá `listener_every_turns`, mặc định, giới hạn, ý nghĩa
      "độ trễ accept tối đa = N × turn".
- [ ] `docs/GUIDE.md` — ràng buộc người dùng: N lớn kéo dài reconnect storm; `serve_sharded_hft`
      không dùng nút này.
- [ ] `docs/best-practices-hft.md` — khuyến nghị N (chỉ khi bước 5 có số).
- [ ] `docs/reference/measured-costs.md` — bảng A/B bước 5; nếu chưa đo, ghi mục *What is not
      proven*.
- [ ] `CHANGELOG.md` — API `Limits::listener_every`, cờ `w2w --listener-every`.
- [ ] `STATUS.md` — item 89: đóng hoặc thu hẹp thành "đo ở boot C"; *Not proven* bullet tương ứng.
- [ ] `tools/w2w` rustdoc đầu file — cờ mới và dòng `logon-rtt`.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Ở `standard`, `poll` thức vì listener readable nhưng vòng đó bỏ qua accept → `poll` trả ngay mãi, engine quay 100 % CPU mà vẫn "đúng" | `standard_accepts_on_the_wake_not_after_n_wakeups`; `check-standard-gives-the-core-back.sh` với `--listener-every 64` |
| Nhịp thưa áp cả khi engine rảnh → kết nối mới chờ N vòng vô ích (và ở `hft` rảnh, vòng `turn` 0 session rất ngắn nên N=256 vẫn là micro-giây, nhưng vẫn là chờ không vì lý do gì) | `hft_idle_engine_accepts_on_the_next_turn` (bổ sung vào file test bước 3) |
| `--no-default-features`: đường `Block` bị `cfg` nhưng dòng reset sau `idle_with` không được đặt sau `#[cfg]` — nếu đặt nhầm trong nhánh `standard` thì `hft` reset sai hoặc không compile | job `no-default-features` CI; `listener_every_one_is_todays_loop` chạy cả hai cấu hình |
| Con số bước 5 dưới ngưỡng drift 5 % của Boot B → công bố "lợi 3 %" mà thực ra là giờ chạy | Chỉ A/B xen kẽ trong một procedure; hai procedure; verdict theo ADR-0068; dưới 5 % thì ghi "không phân biệt được", không đặt mặc định |
| `logon-rtt` gộp cả TCP handshake (~vài chục µs loopback) nên khó thấy N × turn | In cả hai: `connect-rtt` (tới khi `connect()` trả) và `logon-rtt`; hiệu của chúng mới là phần engine |
| w2w `--connect` (generator) nhận `--listener-every` mà im lặng bỏ qua → cánh tay đo tưởng có nhịp | `listener_every_is_refused_by_connect`; và `w2w` in `listener-every: N` đầu run để script đọc lại (bài học `ran_mode`) |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Lợi ích đo được < 1 µs, không tái lập → nút thêm vào mà không đổi mặc định | Trung bình | Chấp nhận: item 89 đóng bằng con số "không phân biệt được", đó vẫn là một kết quả; nút giữ mặc định 1 |
| Bàn §9 chưa có, bước 5 treo | Cao (hiện tại) | Đóng plan ở bước 4+6, STATUS ghi rõ chưa đo; bước 5 thành một dòng trong plan boot C |
| N lớn làm reconnect chậm khi có tải nặng nhiều session (N × 449 ns × sessions) | Thấp ở N ≤ 256, 1 session | `CONFIGURATION.md` ghi công thức; `GUIDE.md` cảnh báo |

## Ngoài phạm vi

- **(d) acceptor thread riêng cho `serve_hft`** (như `shard.rs`): đổi hợp đồng "không sinh
  thread"; chỉ mở nếu Q3 = có.
- **io_uring multishot accept**: dependency mới, `unsafe`, `io_uring_enter` nằm trong `SLEEPERS`;
  cần ADR riêng và một cách thức khác để chạy task_work không qua syscall.
- **Nhịp theo thời gian (b)** và **N thích ứng theo tải**: chưa có số cho (a) thì chưa có lý do.
- `serve_sharded_hft`, `block.rs`, `wait.rs`: không đụng.
- Hai bất thường khác của Boot B (bimodality, TX stamp `igb`): item riêng.

## Câu hỏi cho chủ sở hữu

- **Q1.** Đồng ý mặc định `listener_every_turns = 1` cho tới khi boot C có số, và mặc định mới
  chỉ được đặt trong commit công bố số đo? (Đề nghị: có.)
- **Q2.** Nút đặt ở `presession::Limits` (không đổi chữ ký `serve_*`) hay thành tham số riêng
  của `serve_hft`? (Đề nghị: `Limits`, vì `pending` đã ở đó và cùng là chuyện "trước khi có
  session".)
- **Q3.** Nếu bước 5 cho thấy N=256 vẫn còn > 1 µs `accept4` trong round trip, có mở plan (d)
  acceptor thread riêng cho `serve_hft` không? (Đề nghị: quyết sau khi có số, không quyết bây giờ.)

## Nhật ký giao hàng

**2026-09-18, trên Mac, branch `plan/listener-poll-share` (worktree `../fixbolt-wt-89`), PR #76.**

- Bước 1 — `d5ae645`: ADR-0069 Proposed; hai con số ước lượng dán nhãn `[estimated]`.
- Bước 2 — `1ae2761`: `Limits::listener_every`, `with_listener_every` từ chối 0, khoá `ListenerEveryTurns`, hàng `docs/CONFIGURATION.md` (34 khoá). Developer dừng đúng vì brief cấm docs trong khi gate `doc_table` bắt buộc; cho phép rồi làm tiếp.
- Bước 3 — `ab476a1`: `ListenerCadence` (struct thuần, `poll_now`/`woke`) trong `pump`; `tests/listener_cadence.rs` 4 test. **Khác plan**: test (iii) không chạy 59 def qua `pump` được vì `serve_*` tự dựng `SystemClock`; thay bằng một session thật qua `serve_hft` N=1 cộng `--test wire` 59/59 không đổi. Test (ii) không đếm được idle vì không có seam công khai; thay bằng N = 5 000 000 để nhịp tự thành đồng hồ. Reversal: bỏ reset sau `idle_with` → test `standard_…` đỏ (WouldBlock sau 5 s); bộ đếm không về 0 → 4 test socket **vẫn xanh**, nên đã tách struct thuần và test đơn vị: reversal đó làm `every_three_asks_once_in_three` đỏ.
- Bước 4 — `79b3ec1`: `w2w --listener-every`, `logon-rtt`, `LISTENER_EVERY` trong baseline script. w2w có `pump` riêng, không đi qua `serve_*`, nên nhịp được chép, không dùng chung.
- Senior review (Opus, context mới): không chặn merge. F1–F3 sửa ở `79ce314` (bỏ hai `unreachable!`, in `listener-every: N` và `connect-rtt`). F4 (khoá settings không tự nối vào `Limits`) ghi vào GUIDE/CONFIGURATION. F5–F6 ghi ở đây. Năm `#[allow(too_many_arguments)]` mức hàm trong `w2w` để nguyên.
- Bước 5 **chưa làm** — cần bàn §9 (boot C). Mặc định N = 1, đường nóng không đổi. Không có số đo nào.
- Bước 6 — commit này: DESIGN §4 D8, GUIDE, measured-costs *What is not proven*, CHANGELOG, STATUS item 89 thu hẹp về "đo ở boot C".
- Bẫy gặp ngoài plan: `check-links.py 2>&1 | tail -1` che dòng FAIL vì hai stream qua pipe đảo thứ tự — ghi thành luật 5 trong `docs/reference/reading-the-output-you-grepped-for.md`, script giờ flush stdout trước FAIL.
- CI: ghi ở *Start here* của `STATUS.md` khi đóng.
