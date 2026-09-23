# Phase 4: năm hạng mục anh chọn, mỗi cái có một vạch "không đáng thì bỏ"

> **Loại:** Plan · **Ngày:** 2026-09-23 · **Trạng thái:** Chờ duyệt
> **Phạm vi:** phạm vi phase 4 — đề xuất bởi [ADR-0098](../decisions/ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md), kèm [ADR-0099](../decisions/ADR-0099-kernel-tcp-stays-the-default-and-the-headline-and-a-bypass-figure-is-a-second-labelled-row.md) và [ADR-0100](../decisions/ADR-0100-simd-is-reopened-as-an-experiment-whose-kill-line-is-written-before-the-code.md); `PRD.md` §2 *Phase 4*

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Anh đã chọn nội dung phase 4: **kernel bypass / AF_XDP, `io_uring`, SIMD, store dùng database, và
dashboard.** Đây đúng là năm thứ mà phase 3 (ADR-0097) cố tình để ra ngoài. Bốn trong năm thứ va
vào quyết định đã chốt:

- **Bypass** va ADR-0077 (định vị: acceptor *trên kernel TCP*) và ADR-0074 (Onload chỉ được chạy
  thử, không được ra số); và `PRD.md` §5 ghi nó là non-goal.
- **`io_uring`** từng bị ADR-0074 hoãn "cho tới khi có số đo NIC ở interval 0".
- **SIMD** bị ADR-0045 từ chối: parse chỉ chiếm 0.36–0.62 % một vòng khứ hồi, phần lợi còn nhỏ
  hơn độ nhiễu của chính dụng cụ đo.
- **Dashboard** nằm trong `PRD.md` §5 (non-goal).
- **Store database** không va ADR nào, nhưng nếu ghi database ngay trên luồng engine thì phạm
  bất biến 1 (cấp phát) và 4 (ngủ trong kernel).

Việc của đề xuất này không phải cãi lại lựa chọn của anh, mà là **làm cho từng thứ vào được một
cách trung thực**: quyết định đã chốt thì thay bằng ADR mới (không sửa ADR cũ); từng thứ nói rõ
đụng bất biến nào và giữ nó ra sao; từng thứ có **một phép đo trước/sau trên bàn §9** chứng
minh nó đáng tiền; và **một vạch bỏ viết sẵn trước khi viết code**. Thứ nào không qua vạch thì bị
gỡ ra, con số được ghi lại — và vẫn tính là *xong*, không phải *hỏng*.

## Những gì đã biết chắc

Trong repo và trên máy bàn (đọc hoặc đo ngày 2026-09-23, chỉ đọc, không chạy cargo):

- **Máy bàn §9**: Ryzen 7 3700X, kernel `7.0.0-31-generic`, NIC `enp9s0` = Intel I211, driver
  `igb` (`ethtool -i enp9s0`), 1 GbE, nối thẳng cáp sang Mac mini (`docs/hft-playbook.md`,
  `DESIGN.md` §6). I211 chỉ giữ được một TX timestamp đang chờ.
- **`igb` trên máy bàn đã có đường AF_XDP zero-copy**: `/proc/kallsyms` có `igb_run_xdp_zc`,
  `igb_clean_rx_irq_zc`, `igb_construct_skb_zc`. ADR-0074 viết "igb không có zero-copy" — điều đó
  đã cũ với kernel này. *Mới thấy symbol, chưa bind thử socket XSK nào* — nên chưa biết I211 có
  thật sự chạy zero-copy được không.
- **`io_uring` đang bật trên máy bàn**: `/proc/sys/kernel/io_uring_disabled` = `0`.
- **Điều kiện của ADR-0074 cho `io_uring` đã xảy ra**: số đo NIC ở interval 0 đã có từ
  2026-09-18 (`DESIGN.md` §6: admin p50 27 050 ‖ 27 114 ns, application 28 894 ‖ 28 878 ns,
  `hft`, hai procedure lệch nhau ≤ 0.9 %). Nên `io_uring` không cần ADR thay thế nào.
- **Chi phí nằm ở syscall**: `read(socket) → EAGAIN` tốn 703 ns, trong đó 354 ns chỉ là vào/ra
  kernel; một vòng `Engine::turn` rảnh tốn 449 ns mỗi session (`docs/reference/measured-costs.md`
  *The engine is syscall-bound*). Đây là khoản mà `io_uring` và bypass nhắm vào.
- **Parse đang nhanh sẵn**: `parse NewOrderSingle (validated)` 120.4 ns, `parse Heartbeat
  (validated)` 60.9 ns (`benches/baselines.tsv`). **Chưa có bench nào đo riêng checksum.**
- **Journal `Async` hiện có** (`crates/engine/src/journal.rs`): luồng engine đẩy vào một ring,
  một writer thread ghi đĩa; ring đầy thì message đó không được ghi và sau này thành gap fill —
  không bao giờ bắt luồng engine chờ đĩa. Store database sẽ dùng đúng khuôn này.
- **Quan sát đã có sẵn và nằm ngoài hot path**: `Observer::snapshot()` chỉ lấy khi được hỏi
  (ADR-0032) và luồng event có đếm mất mát (ADR-0035). Dashboard chỉ đọc hai thứ này.

Ngoài repo (lời người khác, chưa kiểm ở đây):

- AF_XDP zero-copy cho `igb` (gồm I210/I211) vào Linux 6.14 —
  <https://www.phoronix.com/news/IntelIGB-AF-XDP-Zero-Copy>, <https://lwn.net/Articles/986006/>.
- AF_XDP tốt nhất 6.5 µs (ConnectX-6) và 9.7 µs (X710) khi có busy poll; zero-copy mà không
  poll thì chạy tệ trên driver Intel — <https://arxiv.org/html/2402.10513v1>.
- Onload chạy được trên card không phải Solarflare qua AF_XDP, hỗ trợ cộng đồng —
  <https://github.com/Xilinx-CNS/onload/blob/master/README.md>; có báo cáo **Onload AF_XDP chậm
  hơn** trên AWS — <https://github.com/Xilinx-CNS/onload/issues/139>.
- ef_vi 1.866 µs, DPDK 3.506 µs RTT trung vị trên card Solarflare —
  <https://github.com/ASherjil/ABTRDA3>.
- Stack TCP userspace bằng Rust: smoltcp (không cần heap, chưa có chứng cứ dùng cho giao dịch) —
  <https://github.com/smoltcp-rs/smoltcp>; Demikernel (nghiên cứu) —
  <https://github.com/microsoft/demikernel>; AF_XDP trong Rust: `xsk-rs`, `xdp` —
  <https://github.com/DouglasGray/xsk-rs>, <https://docs.rs/xdp/latest/xdp/>.
- `io_uring` NAPI busy poll: ping UDP trung bình 37.0 → 29.8 µs; có SQPOLL còn **chậm hơn**
  (44.4 → 37.3 µs) — <https://lwn.net/Articles/961189/>, <https://github.com/lano1106/io_uring_udp_ping>.
  Một nghiên cứu DBMS: SQPOLL tệ hơn, registered buffer gần như không giúp với message nhỏ —
  <https://arxiv.org/html/2512.04859v1>.
- `io_uring` zero-copy receive cần card có header split — <https://docs.kernel.org/networking/iou-zcrx.html>.
- **An ninh `io_uring`**: Docker ≥ 25 chặn mặc định; Google tắt trên ChromeOS, app Android và
  server vì 60 % exploit gửi vào bug bounty kernel 2022 của họ nhắm vào nó —
  <https://github.com/moby/moby/pull/46762>, <https://en.wikipedia.org/wiki/Io_uring>.
- Crate `io-uring` không kèm runtime — <https://deepwiki.com/tokio-rs/io-uring>.
- **SIMD cho FIX**: AVX2 chỉ nhanh hơn **5 %** khi parse message 166 byte, nhưng checksum nhanh
  gấp ~2 lần — <https://www.klittlepage.com/articles/accelerated-fix-processing-via-avx2-vector-instructions/>;
  NexusFIX tự báo ~250 ns cho một ExecutionReport (chậm hơn parse hiện nay của mình) —
  <https://github.com/StratCraftsAI/NexusFix>.
- **Store database**: `JdbcStore` của QuickFIX/J ghi hai lần cho mỗi message, không có
  transaction, người dùng phải tự gom lô —
  <https://github.com/quickfix-j/quickfixj/issues/357>,
  <https://www.quickfixj.org/jira/si/jira.issueviews:issue-html/QFJ-119/QFJ-119.html>;
  QuickFIX/Go có SQL, MongoDB — <https://pkg.go.dev/github.com/quickfixgo/quickfix/store/mongo>.
  Crate `postgres` thực chất bọc `tokio-postgres` **kèm một runtime Tokio** —
  <https://docs.rs/postgres/latest/postgres/>.
- **Giám sát**: QuickFIX/J lộ trạng thái qua JMX, người ta dùng JMX agent đẩy sang Prometheus —
  <https://quickfixj.org/docs/architecture/>; Artio ghi counter vào file counter của Aeron, công
  cụ bên ngoài đọc — <https://github.com/real-logic/artio/wiki/Operational-Concerns>; encode dạng
  text của Prometheus chạy trên `std::io::Write`, không cần runtime —
  <https://docs.rs/prometheus-client>.

**Tìm mà không thấy:** số đo Onload-trên-AF_XDP với card `igb`; số đo engine FIX nào chạy trên
`io_uring`; engine FIX mã nguồn mở nào tự ship Grafana dashboard hay Prometheus exporter; số đo
SIMD so với một parser đã lập chỉ mục field trong một lượt như codec này.

## Cách làm

Năm hạng mục, theo thứ tự (lý do thứ tự ở cuối mục):

1. **Metrics exporter + dashboard.** Crate mới `crates/metrics` (`fixbolt-metrics`), tuỳ chọn.
   Nó chạy trên **luồng riêng**, chỉ đọc `Snapshot` (khi được hỏi) và luồng event, trả dữ liệu
   dạng text của Prometheus qua `std::net::TcpListener` — không runtime async, bộ encode tự
   viết (~100 dòng) thay vì thêm dependency. Thêm vào `Snapshot` hai số `PRD.md` §3 đang thiếu:
   độ sâu ring và số socket đang chờ Logon (đọc counter có sẵn). Dashboard là một file
   `tools/grafana/fixbolt.json`; **Grafana là giao diện**, mình không tự viết web UI.
2. **Store SQLite.** Crate mới `crates/store-sqlite` (`fixbolt-store-sqlite`), không phải
   dependency của `fixbolt-engine`. Khuôn giống hệt `FileJournal` `Async`: luồng engine chỉ đẩy
   vào ring cấp sẵn; một writer thread gom lô, mỗi lô một transaction. Ring đầy → bản ghi đó
   không được lưu, được đếm, sau thành gap fill. **Không có chế độ "ghi xong database mới gửi"**
   — chờ database trên luồng engine là phạm bất biến 4. Khôi phục đọc database qua `Recovery`
   trên luồng acceptor. Dùng `rusqlite` (đồng bộ, không runtime); Postgres thì cần ADR riêng vì
   kéo theo Tokio.
3. **`io_uring`.** Một `Transport` thứ hai trong `crates/engine/src/transport/uring.rs`, sau
   feature `io-uring` (tắt mặc định, `#[cfg]` trên chính `mod`). Nhận nhiều lần một lệnh
   (multishot `recv`) vào vùng đệm cấp và chạm trước lúc khởi động; mỗi luồng engine một hàng
   hoàn tất (CQ) — vòng rảnh thành một lần nhìn CQ thay vì N lần `read`. `hft`: nộp rồi nhìn CQ,
   không bao giờ chờ; `standard`: chờ trong `io_uring_enter` — tức là **ngủ**, đúng luật. SQPOLL
   không dùng ở `standard`; ở `hft` chỉ là một nhánh đo thử, ghim vào một lõi cô lập có tên.
   Nếu `io_uring_setup` bị chặn (seccomp, sysctl) thì engine **từ chối khởi động và nói lý do**,
   không lặng lẽ quay về `read`.
4. **Kernel bypass: Onload trên AF_XDP, trên chính I211 của máy bàn.** **Không viết code
   engine** — engine chạy nguyên dưới `onload`. Việc phải làm: thêm các dòng §9 cho một boot
   bypass (phiên bản Onload, đăng ký `afxdp`, chế độ zero-copy hay copy đọc lại từ kernel,
   `EF_POLL_USEC`), thêm dòng kiểm vào `scripts/check-machine.sh`, và một quy trình `w2w` chạy
   cả hai nhánh trong một boot. Chỉ ở `hft`, chỉ plaintext. **Bẫy lớn**: dưới stack userspace có
   thể không còn hardware timestamp trên NIC của acceptor, nên hai nhánh được so bằng thời gian
   khứ hồi đo **từ phía Mac mini**.
5. **SIMD — SWAR trước.** Trong `codec`, Rust an toàn, không dependency, cho quét SOH và
   checksum. AVX2 (`core::arch`, `unsafe`) chỉ là nhánh hai, chỉ khi SWAR hụt vạch và phần hụt
   nằm ở checksum — khi đó cần plan riêng theo bất biến 8. **Trước khi viết SWAR phải thêm một
   bench đo riêng checksum** để có số "trước".

**Vì sao thứ tự này:** 1 và 2 nằm ngoài hot path, không cần boot §9, và phục vụ ngay người dùng
mà phase 3 mời vào. 3, 4, 5 cần máy bàn §9; SIMD đi cuối vì vạch của nó tính theo mẫu số (thời
gian khứ hồi nhanh nhất) mà 3 và 4 để lại. 3 và 4 có thể dùng chung một boot nếu build sẵn cả hai
nhánh (ADR-0090).

**Ba ADR đi kèm, cùng duyệt một lần:**

- **ADR-0098** — phạm vi phase 4 (file này giải thích nó).
- **ADR-0099** — thay **ADR-0077 quyết định 2** và **ADR-0074 quyết định 1**: tiêu đề vẫn là
  *kernel TCP*; số bypass chỉ được công bố như **dòng thứ hai có nhãn**, đặt cạnh số kernel
  TCP đo cùng boot. Thứ tự Onload → `ef_vi` → không bao giờ DPDK vẫn giữ.
- **ADR-0100** — thay **ADR-0045 quyết định 1**: SIMD mở lại thành một thí nghiệm có vạch bỏ
  viết trước.
- Postgres: **một ADR riêng**, chỉ khi anh muốn Postgres (câu hỏi 4).

## Bất biến bị đụng tới

- **1 (không cấp phát trên hot path)**: `io_uring` cấp vùng đệm lúc khởi động; store và exporter
  mỗi cái có một case `alloc.rs` đếm luồng engine khi chúng đang chạy; SWAR không cấp phát.
- **2 (session thuần)**: không hạng mục nào đụng `crates/session`.
- **3 (59 / 59)**: `--test wire` phải 59 / 59 qua transport `io_uring` và dưới `onload`.
- **4 (ngủ/spin theo mode)**: bị ép mạnh nhất. `io_uring`: `hft` không chờ, `standard` phải chờ,
  SQPOLL cấm ở `standard`; cả hai script mode phải xanh với transport mới và phải đỏ khi đổi
  sai mode. Bypass: chỉ `hft`, trừ khi `standard` dưới Onload tắt spin và qua được
  `check-standard-gives-the-core-back.sh`. Store và exporter: luồng của chúng không phải luồng
  engine; script mode chạy khi chúng đang gắn.
- **5 (thứ tự field từ bảng sinh)**: không đổi.
- **6 (feature flag chặn chính `mod`)**: `io-uring` là feature thật; hai crate mới chỉ build khi
  người dùng chọn; `rusqlite` biên dịch C chỉ khi build crate store; `check-no-optional-deps.sh`
  phủ cả hai crate mới.
- **7 (không panic)**: như mọi crate thư viện.
- **8 (`unsafe` cần plan)**: SWAR không có `unsafe`; nhánh AVX2 nếu có thì cần plan riêng, fuzz so
  với bản scalar. Crate `io-uring` gói phần `unsafe` của nó; chỗ nào mình tự gọi raw thì phải
  ghi bằng chứng.
- **9**: không đụng.
- **10 (số đo)**: mọi con số phase 4 kèm lệnh, máy bàn, output `check-machine.sh`; boot bypass
  cần dòng §9 riêng **trước** con số đầu tiên.

## Chia việc

Mỗi hàng là một pull request, vừa một phiên. **Không hàng nào bắt đầu trước khi phase 3 đóng**
(crate mới sinh ra đã phải theo khuôn publish và semver gate của phase 3; transport mới đổi API
của một crate đã publish).

| Bước | Kết quả | Người làm (đề xuất) | Phụ thuộc |
|---|---|---|---|
| 0 | ADR-0098/0099/0100 được duyệt; nếu anh chọn Postgres thì thêm ADR Postgres | architect | anh duyệt; phase 3 xong |
| 1 | `fixbolt-metrics`: exporter trên luồng riêng, hai số mới trong `Snapshot`, test mọi series của dashboard có thật, case alloc | senior developer (đụng `engine`'s `Snapshot`) | 0 |
| 2 | `tools/grafana/fixbolt.json` + tài liệu; cặp `w2w` scrape bật/tắt trên máy bàn | developer (sonnet) + manager chạy đo | 1 |
| 3 | `fixbolt-store-sqlite`: writer thread, gom lô, test sập-rồi-khôi-phục, case alloc | senior developer | 0 |
| 4 | Store: chạy 50 000 msg/s × 60 s và cặp `w2w` so với `FileJournal` `Async` trên máy bàn | manager chạy, runner (haiku) trích output | 3 |
| 5 | Transport `io_uring` (`hft` + `standard`), từ chối khởi động khi bị chặn, 59 / 59 qua nó, hai script mode | senior developer (hot path) | 0 |
| 6 | Các dòng §9 và `check-machine.sh` cho boot bypass; quy trình `w2w` hai nhánh đo từ phía Mac | developer (sonnet) | 0 |
| 7 | **Một boot §9**: A/B `io_uring` (`w2w` NIC + `turn.rs`) và A/B Onload (phía Mac), mỗi cái hai procedure; áp phán quyết giữ/bỏ | manager + senior developer | 5, 6 |
| 8 | Thêm bench đo riêng checksum, lấy baseline trên máy bàn | developer (sonnet) | 7 |
| 9 | SWAR cho quét SOH + checksum, test so với scalar, Miri; A/B `bench.sh --strict`; phán quyết giữ/bỏ theo ADR-0100 | senior developer | 8 |
| 10 | Đóng phase: `DESIGN.md` §8 (dòng bypass nếu giữ), `measured-costs.md` (mọi cặp, kể cả cặp bị bỏ), `PRD.md`, `STATUS.md` | manager | 1–9 |

Bước 1, 3, 5, 6 chạy song song được (file khác nhau). Bước 7 là boot duy nhất bắt buộc; bước 9
có thể cần một boot nữa.

## Cách kiểm chứng

| # | Tiêu chí | Lệnh |
|---|---|---|
| 1 | Mặc định không build thêm gì | `cargo build --workspace --no-default-features`; `scripts/check-no-optional-deps.sh` |
| 2 | Phán quyết `io_uring` được áp | giữ: `cargo test -p fixbolt-engine --features io-uring --test wire` 59 / 59, hai script mode, alloc 0; bỏ: feature không còn, cặp số trong `measured-costs.md` |
| 3 | Phán quyết bypass được áp | giữ: dòng thứ hai có nhãn trong `DESIGN.md` §8 cạnh số kernel cùng boot, 59 / 59 dưới `onload`; bỏ: cặp số âm trong `measured-costs.md` |
| 4 | Phán quyết SIMD được áp | output `bench.sh --strict` A/B; giữ: fuzz so sánh + Miri xanh; bỏ: code không còn |
| 5 | Store khôi phục được | `cargo test -p fixbolt-store-sqlite` có test sập-rồi-khôi-phục; alloc 0; số 50 000 msg/s × 60 s |
| 6 | Exporter không đụng hot path | `cargo test -p fixbolt-metrics`; alloc 0 khi scrape 10 Hz; cặp scrape bật/tắt trong band |
| — | Phase 1–3 vẫn giữ | 59 / 59, FIXT 179 / 180, interop 7 / 7 với cả hai engine, `cargo semver-checks` |

**Vạch giữ/bỏ** (viết trước code, chi tiết trong ADR-0098 và ADR-0100):

| Hạng mục | Giữ lại chỉ khi |
|---|---|
| `io_uring` | wire p50 NIC tốt hơn ≥ 3 % ở cả hai procedure (gấp 3 lần độ lệch 0.9 % giữa hai procedure), **hoặc** vòng rảnh ở N = 16 tốt hơn ≥ 25 % |
| Onload / AF_XDP | p50 đo từ Mac tốt hơn ≥ 10 % ở cả hai procedure, p99 không tệ hơn, zero-copy bind thật, 59 / 59 dưới `onload` |
| SIMD | các case codec tốt hơn ≥ 15 % **và** (parse ≥ 2 % vòng khứ hồi nhanh nhất còn lại **hoặc** density N = 64 tốt hơn ≥ 3 %) |
| Store SQLite | alloc luồng engine 0; wire p50 trong band so với `FileJournal` `Async`; 50 000 msg/s × 60 s không rớt bản ghi |
| Exporter | scrape 10 Hz không làm p50/p99 ra khỏi band; alloc luồng engine 0 |

"Test pass" một mình không đủ: mọi vạch trên đo trên máy bàn §9, cùng boot, hai procedure, output
`check-machine.sh` đi kèm.

## Tài liệu phải cập nhật

- [ ] `PRD.md` §2 *Phase 4* (đã thêm, trạng thái *Proposed*), §5 (hai gạch đầu dòng đã đánh
      dấu "đề xuất thu hẹp"; sửa hẳn khi duyệt)
- [ ] `DESIGN.md` §1 *Positioning* (ADR-0099), §3 (hai crate mới), D5 (transport `io_uring`), D7
      (store), §5 (SIMD), §8 (dòng bypass nếu giữ), §9 (dòng boot bypass)
- [ ] `README.md` layout; `Cargo.toml` members; `docs/internals/` một trang mỗi crate mới
- [ ] `docs/GUIDE.md` — `io_uring` và Docker/sysctl; store không có chế độ đồng bộ; bypass chỉ
      `hft`, plaintext
- [ ] `docs/CONFIGURATION.md` — feature `io-uring`, khoá của store và exporter
- [ ] `docs/best-practices-hft.md`, `docs/hft-playbook.md` — quy trình Onload/AF_XDP
- [ ] `docs/reference/measured-costs.md` — mọi cặp A/B, kể cả cặp làm hạng mục bị bỏ
- [ ] `CHANGELOG.md` — tên metric là API công khai
- [ ] `CLAUDE.md` đoạn đầu trích câu định vị — **anh sửa** nếu ADR-0099 được duyệt
- [ ] `STATUS.md` khi từng bước đóng

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Dưới Onload không còn hardware timestamp → so hai nhánh bằng hai thước khác nhau | cả hai nhánh đo từ phía Mac; chỉ so cặp đo từ phía Mac |
| Tưởng là zero-copy nhưng thật ra Onload rơi về chế độ copy | dòng `check-machine.sh` đọc lại chế độ XDP từ kernel; FAIL nếu không phải zero-copy |
| `io_uring` bị seccomp chặn → lặng lẽ quay về `read` và số đo nói dối | test: `io_uring_setup` lỗi thì engine từ chối khởi động, có tên nguyên nhân |
| `standard` + `io_uring` spin (SQPOLL, hoặc peek không chờ) | `check-standard-gives-the-core-back.sh` với transport mới, và phải đỏ khi đổi mode |
| `hft` + `io_uring` ngủ trong `io_uring_enter` | `check-no-kernel-sleep.sh` và bản đếm context switch |
| Writer SQLite chậm → ring đầy → mất bản ghi mà không ai biết | counter mất mát lộ ra trong exporter; test throughput đếm 0 bản ghi rớt |
| Scrape exporter chạm luồng engine (khoá, cấp phát) | case alloc khi scrape 10 Hz; cặp `w2w` scrape bật/tắt |
| Đổi tên metric làm hỏng dashboard của người dùng mà không báo | test so tên series giữa exporter và file dashboard; ghi `CHANGELOG` |
| Không có "trước" cho checksum → A/B SIMD vô nghĩa | bước 8 thêm bench checksum và baseline trước bước 9 |
| Build lại bench giữa boot đo → layout đổi (items 99, 101) | build sẵn cả hai nhánh trước boot (ADR-0090); build lại chỉ theo ADR-0096 quyết định 3 |
| Máy bàn đang có tải (LM Studio, gnome-shell sau reboot) | quy tắc sẵn có: chạy đầu tiên sau reboot bỏ đi, `check-machine.sh` trước mỗi bước |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Onload trên AF_XDP chậm hơn kernel (đã có báo cáo trên AWS) | Cao | vạch bỏ; kết quả âm vẫn được ghi và công bố dạng hai dòng |
| SIMD không qua vạch (nghiên cứu duy nhất tìm được chỉ 5 % cho parse) | Cao | vạch bỏ; checksum là ứng viên sống sót |
| `io_uring` là bề mặt tấn công mà phần còn lại của engine không có | Trung bình | tắt mặc định, từ chối khởi động khi bị chặn, `GUIDE.md` nói thẳng |
| Câu định vị dài hơn, người ngoài trích dòng bypass mà bỏ dòng kernel | Trung bình | ADR-0099 cấm trong repo; ngoài repo thì không kiểm soát được |
| Mỗi hạng mục hot path cần một boot §9 (đổi grub, tắt máy…) | Trung bình | gom `io_uring` và Onload vào một boot |
| Hai crate mới phải bảo trì; `rusqlite` kéo C vào build của người dùng chọn nó | Trung bình | crate riêng, tuỳ chọn |
| I211 1 GbE: số bypass trên nó nói ít về card 25 GbE | Thấp | ghi rõ card trong dòng công bố |

## Ngoài phạm vi

- **Transport AF_XDP tự viết và mọi stack TCP userspace** (smoltcp…). Chỉ đáng một ADR nếu Onload
  qua vạch **và** anh không chấp nhận phụ thuộc Onload bản cộng đồng.
- **`ef_vi` / TCPDirect** — không có phần cứng Solarflare/X2.
- **DPDK** — không bao giờ (không có TCP).
- **`io_uring` zero-copy receive** — cần header split, I211 không có.
- **SQPOLL ở `standard`**.
- **Postgres** — trừ khi có ADR riêng (câu hỏi 4).
- **Web UI tự viết** — Grafana là giao diện.
- **HA / replication** — `PRD.md` §5 giữ nguyên.
- **Session FIXP** — do ADR-0097 quyết định 5 quản.

## Anh cần quyết

1. **Duyệt nguyên tắc "mỗi hạng mục có vạch bỏ viết trước; bị bỏ vẫn tính là xong"?**
   *Khuyến nghị: có.* Không có nó thì ADR-0045 và ADR-0074 bị lật mà không có gì thay chỗ.
2. **Duyệt ADR-0099 — tiêu đề vẫn là kernel TCP, số bypass chỉ là dòng thứ hai có nhãn?**
   *Khuyến nghị: có.* Phương án kia — đổi tiêu đề sang số bypass — làm mất đúng điều tiêu đề
   hứa: ai cũng tái lập được trên một máy Linux bình thường. Nếu duyệt, anh cần tự sửa câu định
   vị ở đoạn đầu `CLAUDE.md`.
3. **Bypass trong phase 4 chỉ là Onload trên AF_XDP (không viết code engine), không tự viết
   transport AF_XDP + stack TCP?** *Khuyến nghị: có.* Tự viết stack TCP là một dự án riêng; chỉ
   xét khi Onload đã qua vạch.
4. **Database nào: SQLite (không runtime) hay Postgres (kéo Tokio, cần ADR riêng)?**
   *Khuyến nghị: SQLite trong phase 4.* Postgres để sau, có ADR riêng nếu có người dùng cần.
5. **Có mua card Solarflare/X2-class để thử `ef_vi` không?** *Khuyến nghị: không, trong phase 4.*
   Chỉ đáng khi Onload trên I211 cho thấy khoản kernel đáng kể.
6. **Vạch bỏ của SIMD có giữ điều kiện "≥ 2 % vòng khứ hồi" không, hay chỉ cần codec nhanh hơn
   ≥ 15 %?** *Khuyến nghị: giữ* (kèm nhánh "density N = 64 ≥ 3 %" cho người quan tâm CPU). Chỉ
   xét codec thì SIMD luôn được giữ dù trên dây không ai thấy — đúng điều ADR-0045 cảnh báo.
7. **Các con số vạch (3 %, 25 %, 10 %, 15 %, 2 %, 50 000 msg/s) anh có muốn chỉnh không?**
   *Khuyến nghị: giữ như đề xuất* — mỗi số đã nêu lý do trong ADR-0098; nếu chỉnh thì chỉnh
   **trước** khi viết code, không phải sau khi thấy kết quả.
8. **SQPOLL ở `hft` (tốn thêm một lõi mỗi engine) có được đo không?** *Khuyến nghị: có, như một
   nhánh đo* — nghiên cứu cho thấy nó có thể chậm hơn, nên đo chứ không bật mặc định.

## Nhật ký giao hàng

*(Chưa có — plan đang chờ duyệt, và chưa bắt đầu trước khi phase 3 đóng.)*
