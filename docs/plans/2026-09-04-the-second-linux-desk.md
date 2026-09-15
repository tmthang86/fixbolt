# Lần thứ hai ở bàn Linux: NIC thật, cache lạnh, và những con số còn thiếu

> **Loại:** Plan · **Ngày:** 2026-09-04 · **Trạng thái:** **Đã duyệt 2026-09-13** (Sửa 1, theo đề xuất Q1–Q7) — **Cửa sổ A đã xong 2026-09-14** (A1–A8, nhánh `plan/the-second-linux-desk-a`, PR [#72](https://github.com/tmthang86/fixbolt/pull/72)) — **boot B đo xong 2026-09-15 07:52** (B0–B9, nhánh `plan/the-second-linux-desk-b`, PR [#73](https://github.com/tmthang86/fixbolt/pull/73); B10 review và merge xem nhật ký) — **tiếp theo: boot C (Q1), rồi boot D** — **Sửa 3 đã duyệt 2026-09-14, theo đề xuất Q12–Q17** — **Sửa 2 (chỉ A3b) đã duyệt 2026-09-14, theo đề xuất Q8–Q11**
> **Phạm vi:** `STATUS.md` item 45, đợt C — **một plan cho một lần ngồi ở máy §9**. Đóng item
> **40** (NIC-to-NIC), **49** (2 770 ns chưa quy được), **51** (32 syscall cho một write loopback),
> **52** (bảng baseline nằm trong binary); điền hàng §8 *journal/log* còn `[unmeasured]`; đo
> cache lạnh; đối chứng `matthart1983/nanofix`; **và chia chung một lần boot §9 với bước 6 của
> plan `tls`**. Chạm `tools/w2w`, `scripts/`, `crates/codec/benches/harness.rs`,
> `crates/engine/benches/`, docs `DESIGN.md` §6 §8 §9, `hft-playbook.md`, `measured-costs.md`.
>
> **Draft viết 2026-09-04; xác minh lại toàn bộ 2026-09-13 (Sửa 1, cuối file).** Ba trong bảy
> việc của draft đã xong hoặc sai; hai việc draft định viết code thì không cần code; cách đo NIC
> draft mô tả không chạy được vì một giới hạn của kernel. Tất cả ghi ở Sửa 1.
>
> **Máy chạy:** máy §9 (Ryzen 7 3700X, `enp9s0` = Intel I211, driver `igb`) **và một máy thứ
> hai bất kỳ** — laptop Mac là đủ — nối **cáp Ethernet trực tiếp**. **Thời lượng dự kiến:**
> 1,5 ngày viết code và script trước (máy nào cũng được), **1 ngày ở máy §9 (boot B)**, **1 giờ
> cho boot C** nếu chủ sở hữu đồng ý tắt mitigations, rồi một lần boot về desktop.
> **Mỗi lần reboot là hết một phiên** — plan này trải ra **ít nhất ba pull request**, ranh giới
> là ranh giới boot, đúng `CLAUDE.md` §12 *một bước sống trong một phiên*.

## Bối cảnh

Mọi số wire-to-wire fixbolt công bố là **loopback**, back-to-back, cache nóng, N = 1. Lần đầu ở
bàn Linux đóng phase 1; lần sau (2026-09-05) đóng item 14, 34, 39, 41 và mở ra ba item mới mà
không đóng được ngay vì chúng cần đổi **máy**, không phải đổi code:

- **Item 51**: một write TCP loopback tốn **5 450 ns**, bằng 32 lần `getppid`. Hai nghi phạm —
  netfilter/conntrack (Tailscale, Docker) và mitigations của Zen 2 — **chưa nghi phạm nào bị
  thử**, vì thử là đổi máy của chủ sở hữu. Mọi số §8 từng công bố đều đi qua `127.0.0.1`, nên
  nếu nghi phạm nào đúng thì hạng mục lớn nhất trong ngân sách §8 không thuộc về repo này.
- **Item 49**: app round trip cao hơn admin **3 898 ns**; benchmark đã cam kết giải thích được
  **~1 128 ns (28,9 %)**; **~2 770 ns** còn lại chưa có bench nào tách. Hai lần nghi phạm lớn
  nhất đều không phải câu trả lời.
- **Item 52**: `benches/baselines.tsv` được `include_str!` vào binary bench, nên **ghi kết quả đo
  làm đổi chính binary vừa đo** — một case nhỏ lệch 23 %.
- **Item 40**: hàng NIC-to-NIC của §6 chưa đo. Phần khó nhất — NIC có timestamp phần cứng — đã
  có sẵn trên máy này (`ethtool -T enp9s0`: `hardware-transmit`, `hardware-receive`,
  `hardware-raw-clock`); thiếu **một sợi cáp** và một bản `w2w` tách đôi.

Máy hôm nay đang ở **cấu hình desktop**: `/proc/cmdline` không có `isolcpus` (STATUS 2026-09-05
trả máy về desktop trọn vẹn). **§9 không còn cách một lệnh** — phải sửa grub, reboot, rồi
`fixbolt-machine on`. Vì reboot đắt, plan này gom **mọi** việc cần boot §9 vào một lần, **kể cả
bước 6 của plan `tls`** (ba số §8 plain / kTLS / userspace, cũng cần đúng boot này).

## Những gì đã biết chắc (xác minh lại 2026-09-13)

### Về máy và code hôm nay — đọc trực tiếp ngày 2026-09-13

| Sự thật | Nguồn |
|---|---|
| `/proc/cmdline` = `quiet splash crashkernel=…` — **không** `isolcpus`, **không** `rcu_nocbs`, **không** `processor.max_cstate`. Máy ở cấu hình desktop | `cat /proc/cmdline` 2026-09-13 |
| Dòng §9 đúng (sau ADR-0021, **không** `nohz_full`) nằm ở `/etc/default/grub.fixbolt-backup-20260905-175130` và `…fixbolt-adr21` (1 629 byte, giống nhau). **`grub.fixbolt-s9` là bản cũ còn `nohz_full=6,7,14,15`** — khôi phục nhầm file này là đo sai 160 ns mỗi lần vào kernel | `sudo -n grep CMDLINE /etc/default/grub.fixbolt-*`; ADR-0021 |
| `fixbolt-machine on` đặt governor, boost 0, SMT off, THP never, `busy_poll=50` **và** `busy_read=50`; `tls`/`untls` nạp/gỡ module `tls`; `status` đọc lại. `sudo -n` không cần mật khẩu (`/etc/sudoers.d/fixbolt-all`) | `/usr/local/sbin/fixbolt-machine`, `sudo -n true` |
| `check-machine.sh` hàng *NIC IRQ affinity* **luôn** in `UNKNOWN` (dòng 361–367): chỉ đếm dòng trong `/proc/interrupts`, không đọc `smp_affinity_list`. Hàng `busy_poll` chỉ đọc `net.core.busy_poll`, không đọc `busy_read` | `scripts/check-machine.sh:237–243, 361–367` |
| `enp9s0`: Intel I211, `igb`, **5 IRQ** (`enp9s0`, `rx-0`, `rx-1`, `tx-0`, `tx-1`), affinity hiện `0-15`; `irqbalance` **inactive**; `ethtool -c` `rx-usecs 3`; auto-MDIX (`MDI-X: off (auto)`); `Link detected: no`; `/sys/class/ptp/ptp0` là clock của I211 | `/proc/interrupts`, `/proc/irq/86/smp_affinity_list`, `ethtool -c/-T enp9s0` |
| Netfilter: `nf_tables nf_conntrack nft_compat xt_MASQUERADE …` đều nạp; `nf_conntrack_count` = 291; ruleset của Tailscale có chain `ts-input` **với một rule `iifname "lo"`** — nghĩa là gói loopback đi qua chain của Tailscale. Bảng `raw` của iptables rỗng. Docker **inactive**, `tailscaled` **active** | `sudo -n nft list ruleset`, `iptables -t raw -S`, `systemctl is-active` |
| Mitigations đang bật: `spectre_v2` Retpolines + IBPB conditional + STIBP always-on; `retbleed` untrained return thunk; `spec_rstack_overflow` Safe RET | `/sys/devices/system/cpu/vulnerabilities/` |
| `perf`, `tcpdump`, `nmcli`, `ethtool`, `strace` có trên máy. **`cmake` không có** | `which`; memory `fixbolt-sudo-helper` |
| **Item 39 và 41 đã ĐÓNG 2026-09-05** — dictionary pass có bench (`crates/session/benches/validate.rs`, 897,3 ns), `bench.sh --strict` xanh ba lần liên tiếp, 28 baseline ghi lại dưới ADR-0049 | `STATUS.md` hàng 39, 41; `DESIGN.md` §6 |
| `crates/engine/benches/alloc.rs` có **27** case kể cả `log-record`, `log-idle`, **`log-busy`**; **không** có case nào cho `FileJournal` `Durability::Async` | `DESIGN.md` §6 *Allocation*; grep `alloc.rs` |
| `crates/engine/benches/density.rs`: `engine turn, N busy sessions` qua transport `Feed` (copy vào buffer của caller, không syscall), **chỉ shape app** (`35=D` → `35=8`), **1 659,8 ns** ở N = 1; seq patch tại chỗ | `density.rs` header; `DESIGN.md` §6 |
| `benches/baselines.tsv` được **`include_str!`** vào `crates/codec/benches/harness.rs:81` (`const BASELINES`) — **một** chỗ; 266 dòng | `harness.rs:79–81` |
| `tools/w2w`: cờ `--messages --warmup --hold-ms --mode --path --engine-core --client-core --allow-unisolated`; **không** `--interval`, `--connect`, `--listen`, `--tls`, `--wire-timestamps`, `--journal`, `--log`; `pump()` dùng `Store` (MemJournal) và **không có** `FileLog`; socket nhận được qua `acceptor.accept()` rồi `engine.add(t)` — **`TcpTransport::socket(&self) -> &TcpStream` là `pub`**, nên w2w đặt được socket option **trước** `add` mà không chạm `crates/engine` | `tools/w2w/src/main.rs:332–396, 665–690`; `crates/engine/src/transport.rs:48` |
| `fixbolt-w2w` không phụ thuộc `libc`; `fixbolt-engine` có `libc` sau `standard`, `affinity`, `tls` | hai `Cargo.toml` |
| `scripts/check-no-kernel-sleep.sh` chạy `w2w --messages 300 --warmup 50 --hold-ms 400 ${2:-}` và gán syscall theo tid engine; `check-standard-gives-the-core-back.sh` bốn assertion | hai script |
| **Ba plan nợ cùng một script `check-no-kernel-sleep.sh`**: bước 6 của `the-hft-front-doors-have-no-gate` (Đã duyệt, cần Linux, binary gọi `serve_hft`), bước 6 của `tls` (arm `--tls ktls`), và plan này (chạy với `--wire-timestamps`). Ai tới trước, người sau đọc lại script | `tls.md` Sửa 2; `front-doors.md` bước 6 |
| Loopback §9: `hft` 16 010 / 20 589 / 22 127 ns admin, 19 908 / 24 657 / 26 150 app; `standard` 19 447 / 24 106 / 25 609 và 20 920 / 25 618 / 27 092 — **đo 2026-09-02**, trước ADR-0046, 0053, 0054, 0056, 0059 | `DESIGN.md` §8 |
| §8 hàng *if `FileLog` is on* vẫn `~340 ns [unmeasured]`; không có hàng cho `FileJournal Async` | `DESIGN.md` §8 dòng 1055 |
| `isolcpus` đáng 11× ở p99.9, không đáng gì ở p50; mitigations tắt làm mọi syscall rẻ **59–63 %** (`turn` 448,9 → 175,2 ns) — **§9 yêu cầu BẬT**, và ADR-0023 nói rõ số đo khi tắt *không được công bố như số so được* nhưng **không cấm đo** | `DESIGN.md` §9; ADR-0023 *Decision* 2 |
| Lần bench đầu sau reboot tự loại vì `gnome-shell` còn khởi động; `GAP=8` giây giữa hai run | memory `first-bench-run-after-reboot`; `w2w-baseline.sh` |
| `matthart1983/nanofix` đã được build ở đây một lần, `build.rs` vô hiệu hoá để không cần Aeron; `measured-costs.md` §1 ghi cách làm | `measured-costs.md` §1 |

### Từ internet — tra 2026-09-13, mỗi dòng một nguồn

| Sự thật | Nguồn |
|---|---|
| `SOF_TIMESTAMPING_TX_HARDWARE` / `RX_HARDWARE` xin timestamp từ NIC; `RAW_HARDWARE` báo nó ra; timestamp **TX** trả về qua **error queue của socket gửi** (`recvmsg(MSG_ERRQUEUE)`, `struct scm_timestamping`, **`ts[2]`** là phần cứng); với stream socket cần `OPT_ID` **+ `OPT_ID_TCP`**, id tăng theo **byte**; một request là "lúc **toàn bộ** buffer đã qua điểm timestamp"; phải bật trên thiết bị bằng `SIOCSHWTSTAMP` (`HWTSTAMP_TX_ON`, `HWTSTAMP_FILTER_ALL`) | [kernel `timestamping`](https://docs.kernel.org/networking/timestamping.html) |
| **Tap AF_PACKET / libpcap không có timestamp phần cứng chiều TX** — `tp_status` luôn `TP_STATUS_TS_SOFTWARE` cho gói gửi đi; issue còn mở | [libpcap #894](https://github.com/the-tcpdump-group/libpcap/issues/894) |
| **Patch `igb` ngày 2026-09-10** (Pascal Kneuper, intel-wired-lan): trên i210/i211 `igb_setup_tx_mode()` xoá `CFG_TS_EN` ở mỗi `igb_up()` → **RX timestamp rơi về software một cách im lặng** sau mỗi lần link thay đổi, **và `igb_ptp_hwtstamp_get()` trả về cấu hình đã cache nên đọc lại không thấy**. Kernel 7.0.0-31 của máy này có thể chưa có patch | [intel-wired-lan 2026-09](https://ratatoskr.run/intel-wired-lan/2026/09/17545610/t) |
| Busy polling: bật bằng `SO_BUSY_POLL` trên socket **hoặc** sysctl `net.core.busy_poll` / `busy_read` toàn hệ; `SO_PREFER_BUSY_POLL` là lời hứa poll đều để kernel **giữ IRQ tắt**, bị rút nếu `gro_flush_timeout` trôi qua không có poll; `napi_defer_hard_irqs` = số lần poll rỗng trước khi quay về IRQ. *Tài liệu không nói thẳng loopback không có NAPI* — `prior-art.md:391` của repo nói điều đó | [kernel `napi`](https://docs.kernel.org/networking/napi.html) |
| `notrack` (kernel ≥ 4.9) phải nằm ở chain có priority **< −200** (`raw`, −300) để đi trước conntrack; ví dụ `nft add chain my_table prerouting { type filter hook prerouting priority -300 \; }` | [nftables wiki](https://wiki.nftables.org/wiki-nftables/index.php/Setting_packet_connection_tracking_metainformation) |
| iptables tương đương: `iptables -t raw -A PREROUTING … -j NOTRACK` và `-A OUTPUT … -j NOTRACK`. **Bài không có số đo**, chỉ nói "giảm CPU" — nên con số là của buổi đo này, không có số tham chiếu bên ngoài | [ADHDecode](https://adhdecode.com/articles/iptables/iptables-raw-table-conntrack-bypass/) |
| Retbleed đụng Zen 1/1+/2; paper ETH ghi overhead **14–39 %**; `mitigations=off` tắt cả retbleed. *Không tìm thấy số syscall cụ thể cho Zen 2 ngoài số ADR-0023 của repo* | [Phoronix](https://www.phoronix.com/review/retbleed-benchmark) (403 khi fetch, chỉ đọc được tóm tắt tìm kiếm); [kernel SRSO](https://docs.kernel.org/admin-guide/hw-vuln/srso.html) |
| `ethtool -C <nic> rx-usecs 0` tắt interrupt moderation trên `igb` (một IRQ mỗi gói); `1` = dynamic. Nghĩa của `3` (giá trị hiện tại) **không tìm thấy tài liệu** — đọc `igb_ethtool.c` khi ở máy | [e1000-devel](https://www.mail-archive.com/e1000-devel@lists.sourceforge.net/msg12765.html) |
| Criterion.rs: `--save-baseline` / `--baseline` / `--load-baseline` so với baseline **đã lưu từ lần chạy trước** — baseline là *dữ liệu sinh ra khi chạy*, không nằm trong binary. *Tìm kiếm không cho ví dụ nào về harness cố ý compile baseline vào binary* | [Criterion CLI](https://bheisler.github.io/criterion.rs/book/user_guide/command_line_options.html) |
| `nanofix` README: hỗ trợ **FIX 4.4** (và 4.0–4.3, 5.0 SP2/FIXT 1.1); có `FixServer` nhận TCP nhưng **ứng dụng phải tự trả lời** — không có ví dụ trả `ExecutionReport`; build mặc định cần CMake + Aeron trừ khi `AERON_LIB_DIR` | [github nanofix](https://github.com/matthart1983/nanofix) |

## Cách làm

### Hình dạng: ba cửa sổ, ba phiên, theo thứ tự

```text
Cửa sổ A  — code + script, máy nào cũng được (Linux desktop cho gate Linux)   → PR 1
            (bước 6 của plan tls — Sửa 6 — phải lên main TRƯỚC boot B)
Boot B    — §9, mitigations BẬT: mọi số công bố được; tls step 6; NIC; NOTRACK → PR 2
Boot C    — §9 + mitigations=off: CHỈ hai phép đo A/B của item 51, 1 giờ     → PR 3 (tuỳ Q1)
Boot D    — về desktop, fixbolt-machine off
```

Vì sao tách: một số đo ở boot B là **số công bố**; một số đo ở boot C là **hiệu số A/B** và
ADR-0023 nói nó không bao giờ được đứng cạnh số §8 như số so được. Hai thứ không chung một boot
thì không ai nhầm được.

### Cửa sổ A — những gì phải có trước khi reboot

**A1 — `baselines.tsv` ra khỏi binary (item 52).** `crates/codec/benches/harness.rs` đọc file
**lúc chạy** từ đường dẫn cố định lúc biên dịch
(`concat!(env!("CARGO_MANIFEST_DIR"), "/../../benches/baselines.tsv")`); file thiếu hay hỏng là
**lỗi thoát ≠ 0 ở mọi chế độ**, không chỉ `--strict` — giữ đúng tính chất "quên baseline không
thể im lặng" mà `include_str!` từng mua. Thuộc tính mới: **ghi thêm dòng vào file không đổi một
byte nào của binary**. ADR mới (ADR-0062) ghi quyết định và ghi rõ nó thay câu *compiled in* của
`harness.rs`. Ở boot B: chạy `bench.sh --strict` **ba lần** trên binary mới; case nào ra ngoài band
thì đo lại n = 20 và ghi `n` thật; case trong band giữ nguyên (band của ADR-0031 sinh ra để chịu
đúng loại trôi này). **Không** đo lại cả 28 dòng trừ khi có case đỏ — xem Q6.

**A2 — bench tách 2 770 ns (item 49).** `density.rs` thêm **một** case `engine turn, 1 busy,
admin`: `TestRequest` vào, `Heartbeat` ra, qua `Feed`, cùng khung với case app N = 1 đang có.
Hiệu `app − admin` **trong tiến trình** (D_in) đặt cạnh **3 898 ns** trên dây: phần của D_in là
của engine (framing, buffer, session, dispatch, serialise — **kể cả** `Heartbeat` serialise mà §8
nói chưa có case, nên không cần case riêng nữa); phần còn lại là **ngoài tiến trình** — kernel,
hai bản copy, hai syscall với kích thước khác. Đó là phép tách item 49 xin. Chẩn đoán thêm ở boot
B: `perf record -t <engine tid>` hai path, **chỉ để chỉ hướng, không công bố số**.

**A3a — `w2w` tách đôi và pacing.** `--listen <addr>` (nửa engine, máy này), `--connect <addr>`
(nửa generator, build được trên macOS vì không cần `affinity`), `--interval <us>` (gửi một message
mỗi `interval` µs, **chờ bằng spin** trên client thread; `0` là hôm nay). Không cờ nào thì hành vi
hôm nay giữ nguyên byte-for-byte để `check-no-kernel-sleep.sh` hiện tại không đổi nghĩa.

**A3b — `--wire-timestamps` (Linux, nửa engine).** Cách đo **một clock**: engine ở máy này, §9;
generator ở Mac chỉ cần gửi và đọc. Timestamp **RX của request** và **TX của reply** đều lấy ở
**I211 của máy này**, cùng PHC `ptp0`, không PTP sync. Hai nguồn, vì kernel không cho một:

- **RX**: một thread *observer* của `w2w` (pin `--observer-core`, **không** phải 6/7) mở **AF_PACKET**
  trên `--nic enp9s0` với `PACKET_TIMESTAMP = SOF_TIMESTAMPING_RAW_HARDWARE` và `SIOCSHWTSTAMP`
  (`rx_filter ALL`, `tx_type ON`), lọc theo port; mọi frame vào có `ts` phần cứng. Cần
  `CAP_NET_RAW`: **một dòng `sudo -n setcap cap_net_raw+ep target/release/w2w` sau mỗi lần build**,
  ghi trong runbook — không chạy w2w dưới `sudo`.
- **TX**: `setsockopt(SO_TIMESTAMPING, TX_HARDWARE | RAW_HARDWARE | OPT_ID | OPT_ID_TCP | OPT_TSONLY)`
  trên **fd của socket đã accept** — lấy qua `TcpTransport::socket().as_raw_fd()` **trước**
  `engine.add(t)`, trong w2w, **không sửa `crates/engine`**. Observer thread đọc
  `recvmsg(MSG_ERRQUEUE)` trên cùng fd. Engine thread không thêm syscall nào — gate 4 xác nhận.
- **Ghép**: một request trong chuyến (back-to-back), ghép theo thứ tự; `ts[2] == 0` ở bất kỳ mẫu
  nào là **đếm vào `hw-rx-missing` / `hw-tx-missing` và in ra**, không bao giờ thay bằng
  software timestamp. Vì patch `igb` 2026-09-10 nói readback của driver **nói dối**, bằng chứng
  duy nhất là từng mẫu có `ts[2] ≠ 0`.
- **In**: `wire p50/p99/p99.9` (wire-in → wire-out ở acceptor), bên cạnh `allocs` của engine +
  observer thread. Mac in RTT phần mềm của nó thành **bảng riêng**, nhãn *"as the counterparty
  sees it"*, không bao giờ trừ cho nhau.
- Phụ thuộc mới: `libc` trong `fixbolt-w2w` chỉ cho `cfg(target_os = "linux")`. Lý do: `setsockopt`,
  `recvmsg`, `socket(AF_PACKET)` không có trong `std`. `w2w` không phải library crate, nhưng vẫn
  ghi lý do ở đây theo §6 *Dependencies*.
- `unsafe` cho ba call FFI: comment nêu thứ chứng minh — test ghép thuần (`pair.rs`) và arm
  `check-no-kernel-sleep.sh` chạy với `W2W_EXTRA="--wire-timestamps --nic lo"` (loopback **không**
  có timestamp phần cứng, nhưng script chỉ hỏi tập syscall của **engine tid** — nên gate này chạy
  được ở mọi máy Linux, không cần cáp).

**A4 — giá của journal/log.** `w2w --journal mem|file-async` và `--log none|file`; case alloc mới
`journal-async-busy` trong `crates/engine/benches/alloc.rs` (một message qua `FileJournal`
`Async`, đọc file sau `close`, **0**). `log-busy` **đã có** — draft nói chưa là sai.

**A5 — `check-machine.sh` biết NIC.** Khi `FIXBOLT_NIC=<tên>` (hoặc tự chọn NIC **có carrier**):
hàng *NIC IRQ affinity* đọc từng `/proc/irq/<n>/smp_affinity_list` của NIC, **PASS** nếu rời khỏi
`/sys/devices/system/cpu/isolated`, **FAIL** nêu tên IRQ; hàng mới *coalescing* (`ethtool -c`,
PASS khi `rx-usecs 0`); hàng mới *irqbalance inactive*; hàng `busy_poll` đọc thêm `busy_read`.
**Không có NIC có carrier thì giữ `unknown 1` y như hôm nay**, để chuỗi `pass 12 fail 0 unknown 1`
trong `baselines.tsv` vẫn là chuỗi của máy loopback — pass count là một phần của record.

**A6 — `w2w-baseline.sh` cho NIC và interval.** `ARMS` nhận `mode:path:interval`; `LISTEN=`,
`GENERATOR_SSH=user@mac` chạy nửa generator qua ssh mỗi run; `FIXBOLT_NIC` truyền xuống
`check-machine.sh`. Không có biến mới thì script chạy đúng như hôm nay.

**A7 — chuẩn bị `nanofix`.** Clone vào `vendor/nanofix` (gitignored) ở commit ghim; vô hiệu hoá
`build.rs` như `measured-costs.md` §1 đã làm (không cần cmake — máy này không có); viết một bin
ví dụ dùng `FixServer`, CompID `ISLD`/`W2W`, `8=FIX.4.4`; thử `w2w --connect` vào nó **trên
loopback ở máy bất kỳ**: Logon phải được nhận, `TestRequest` phải ra `Heartbeat`. Nếu API cho
phép trả `35=8` thì thêm; nếu không, đối chứng **chỉ path admin** và nói thế. Bản vá ghi bằng
**lời + hash commit**, không dán source của họ vào repo.

**Không làm trong cửa sổ A, và vì sao** (draft từng định làm):

- **`SO_BUSY_POLL` / `SO_PREFER_BUSY_POLL` như `Transport` option — bỏ.** `fixbolt-machine on` đã
  đặt `net.core.busy_read=50`, và `sk_ll_usec` của mọi socket lấy mặc định từ sysctl đó lúc tạo —
  nên phép đo *"busy poll có đáng không"* là **A/B sysctl 0 ↔ 50 ở boot B, không cần code**. Chỉ
  `SO_PREFER_BUSY_POLL` không có sysctl tương đương; nó thành code **chỉ nếu** A/B sysctl cho thấy
  hạng mục IRQ còn lớn — khi đó là plan riêng, có số để biện minh.
- **`mlockall` read-back — đề nghị bỏ khỏi plan này** (Q5). Không phép đo nào ở đây cần nó,
  warmup của `w2w` đã chạm mọi trang, và bẫy của chính draft nói trên máy rảnh sẽ không thấy gì.

### Boot B — §9, mitigations BẬT — thứ tự trong ngày

**B0 — vào §9, và chứng minh đã vào.**
`sudo -n cp /etc/default/grub /etc/default/grub.fixbolt-desktop-$(date +%Y%m%d)`;
`sudo -n cp /etc/default/grub.fixbolt-backup-20260905-175130 /etc/default/grub`;
`sudo -n grep CMDLINE /etc/default/grub` **phải không có `nohz_full`**; `sudo -n update-grub`;
reboot. Sau boot: `cat /proc/cmdline` phải có `isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15
processor.max_cstate=1`; `sudo -n fixbolt-machine on` rồi `status`; cắm cáp, `nmcli con add type
ethernet ifname enp9s0 ip4 192.168.77.1/24` (Mac: 192.168.77.2/24 thủ công), `ethtool enp9s0` →
`Link detected: yes`, `Speed: 1000Mb/s`; `sudo -n ethtool -C enp9s0 rx-usecs 0`; ghi `4` vào
`/proc/irq/{85..89}/smp_affinity_list` (cùng CCX với 6/7, không isolated);
`FIXBOLT_NIC=enp9s0 scripts/check-machine.sh` → `fail 0 unknown 0`. **Bỏ run bench đầu tiên.**

| Bước | Đo gì | Cho item / hàng |
|---|---|---|
| **B1** | `bench.sh --strict` ×3 trên binary A1 → quyết định đo lại case nào; ghi baseline `engine turn, 1 busy, admin` n = 20; tính D_in | 52, 49 |
| **B2** | `w2w-baseline.sh` bốn arm loopback như 2026-09-02 — code đã đổi qua năm ADR, số lệch là một **phát hiện**, không phải nhiễu | §8 bảng 1 |
| **B3** | **tls bước 6**: `fixbolt-machine tls`; ba arm plain / kTLS / userspace theo runbook Sửa 6 của plan `tls`; `check-no-kernel-sleep.sh` arm `--tls ktls`. Đặt **ngay sau B2** để ba số TLS và số plain cùng binary, cùng giờ — phép so của chúng là hiệu số | §8 hàng TLS, ADR-0005 câu 2, 6 |
| **B4** | `--interval 0 / 1 000 / 10 000 / 1 000 000` loopback, bốn arm → hàng §8 *"latency lúc 3 giờ sáng"* | §8 hàng mới |
| **B5** | `--journal file-async` và `--log file` so với không, `hft` + `standard`, path app | §8 hàng journal/log |
| **B6** | **NIC** (cần cáp): `--listen` ở máy này, generator ở Mac qua ssh; `hft`/`standard` × admin/app × interval 0 / 1 000 000; `wire p50/p99/p99.9` từ timestamp phần cứng + bảng Mac riêng; **A/B `busy_read`/`busy_poll` 0 ↔ 50** (sysctl); **một arm IRQ pin vào cpu6** để *cho thấy* hàng §9 đáng bao nhiêu; ghi topology: cáp trực tiếp, I211, `igb`, `rx-usecs 0`, MTU 1500, **kernel 7.0.0-31 và `hw-rx-missing` đếm được bao nhiêu** | **40**, §6 hàng NIC, §9 hàng IRQ + coalescing |
| **B7** | **item 51, nghi phạm 1**: `payload.rs` case `TCP loopback` + `w2w` admin `hft` loopback, **trước**; `sudo -n nft add table ip fixbolt; … chain pre/out priority -300; iif lo notrack; oif lo notrack`; đo lại; `nft delete table ip fixbolt`. Nếu chủ sở hữu **ở bàn** (không điều khiển qua Tailscale): thêm arm `systemctl stop tailscaled` + `nft flush ruleset` 10 phút, rồi `start` — tách *conntrack* khỏi *chain traversal* | **51** |
| **B8** | Đối chứng `nanofix`, **`standard`**, admin (+ app nếu A7 cho phép), loopback, median 20 run, cùng `check-machine.sh` output | item 45 (b), `measured-costs.md` |
| **B9** | `perf record -t <engine tid>` hai path w2w, bảng symbol chênh → `measured-costs.md` nhãn *diagnostic* | 49 (hướng) |

**B3 vì sao chung boot với plan này**: bước 6 của `tls` cần đúng máy §9, đúng boot, và module
`tls`; tách riêng là thêm hai reboot. Code của nó (Sửa 6: `w2w --tls`, arm script) **phải lên
`main` trước B0** — nếu chưa, B3 bị bỏ và plan `tls` nói rõ bước 6 còn nợ; boot B **không chờ**.

### Boot C — §9 + `mitigations=off` — chỉ nếu Q1 = có

Thêm `mitigations=off` vào cùng dòng grub, reboot, `fixbolt-machine on`. `check-machine.sh`
**phải FAIL** ở hàng mitigations — đó là đảo chiều miễn phí cho ADR-0023. Đo **đúng hai thứ** của
B7 (trước NOTRACK), ghi thành hiệu số cạnh số boot B, nhãn *A/B, không công bố*. Rồi bỏ
`mitigations=off`, về B0's cmdline hoặc thẳng về desktop (Boot D). **Không chạy `w2w-baseline.sh`
đầy đủ ở boot này** — số nào sinh ra ở đây cũng không được đứng cạnh §8.

### Boot D — về desktop

`sudo -n cp /etc/default/grub.fixbolt-desktop-<ngày> /etc/default/grub && sudo -n update-grub`,
reboot, `fixbolt-machine off`, `nmcli con delete` kết nối tĩnh, `ethtool -C enp9s0 rx-usecs 3`
(giá trị cũ). Ghi vào STATUS *Start here* rằng máy đã về desktop.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **1 — không cấp phát** | `journal-async-busy`; observer thread của w2w | case alloc mới đọc **0**; `w2w` in `allocs` cho cả engine + observer trong cửa sổ đo |
| **4 — hai nửa** | socket option `SO_TIMESTAMPING` trên socket engine; thread observer | `check-no-kernel-sleep.sh` với `W2W_EXTRA="--wire-timestamps --nic lo"`: tập syscall engine tid **không đổi** (`recvfrom`, `sendto`); `check-standard-gives-the-core-back.sh` với cùng cờ: **POLLERR** từ errqueue không được biến `standard` thành spin — bốn assertion phải xanh |
| **8 — `unsafe`** | ba call FFI trong `w2w` | comment nêu test `pair.rs` và hai script trên; `w2w` đã `#![allow(unsafe_code)]` có lý do ghi sẵn |
| **10 — số có benchmark, máy, §9** | mọi số mới | mỗi hàng ghi lệnh, `check-machine.sh` output (kể cả pass count mới khi có NIC), topology, kernel, `hw-*-missing`; số boot C nhãn A/B |
| 6 — feature gate | không chạm `crates/engine` | `check-no-optional-deps.sh` không đổi; `libc` của w2w sau `cfg(target_os)` chứ không phải feature, vì w2w không phải library và không có build "không tuỳ chọn" |
| 9 — không copy nguồn | `nanofix` trong `vendor/` gitignored; bản vá ghi bằng lời | `git status` trước `add`; không file nào từ `vendor/` vào commit |

## Chia việc

Tier theo `CLAUDE.md` §12. *Máy*: **bất kỳ** · **Linux-desktop** (máy này, cấu hình hôm nay) ·
**boot B** · **boot B + cáp** · **boot C**. Senior review một lần cho PR 1 (A3b và A1 là đường
nóng của rule 4 và rule 10), một lần cho PR 2.

| Bước | Kết quả | File | Test / gate | §2 | Máy | Tier | Phụ thuộc |
|---|---|---|---|---|---|---|---|
| A1 | `baselines.tsv` đọc lúc chạy; ADR-0062 | `crates/codec/benches/harness.rs`, `benches/baselines.tsv` (header), `docs/decisions/ADR-0062-*.md` | `scripts/bench.sh` in cùng verdict như trước; **đảo chiều**: xoá một dòng → case đó `NO BASELINE`; đổi tên file → thoát ≠ 0 nêu đường dẫn | 10 | bất kỳ | sonnet, **review opus** | — |
| A2 | case `engine turn, 1 busy, admin` | `crates/engine/benches/density.rs` | `cargo bench -p fixbolt-engine --bench density` in case mới; setup assert một `Heartbeat` ra mỗi turn (như case app assert một order) | 10 | bất kỳ | sonnet | — |
| A3a | `--listen`, `--connect`, `--interval` | `tools/w2w/src/main.rs` | `cargo test -p fixbolt-w2w`; `cargo build -p fixbolt-w2w` **trên macOS**; `strace -f` client trong cửa sổ đo **không** `nanosleep`/`clock_nanosleep`; không cờ → output byte-giống hôm nay | 4 | Linux-desktop (+ một lần build Mac) | sonnet | — |
| A3b | `--wire-timestamps`, `--nic`, `--observer-core`; module `pair.rs` | `tools/w2w/src/main.rs`, `tools/w2w/src/pair.rs`, `tools/w2w/Cargo.toml` (`libc`, cfg linux), `scripts/check-no-kernel-sleep.sh` (`W2W_EXTRA`) | tests `pair::pairs_in_order_one_in_flight`, `pair::a_missing_hw_stamp_is_counted_not_interpolated`, `pair::never_mixes_software_into_the_hardware_column`; `check-no-kernel-sleep.sh` với `W2W_EXTRA="--wire-timestamps --nic lo"` → engine tid chỉ `recvfrom sendto` (+ `accept4`); `check-standard-gives-the-core-back.sh` cùng cờ xanh; trên `lo` phải in `hw-rx-missing = tổng` và **không** in cột wire | 1, 4, 8 | Linux-desktop | **opus** | A3a |
| A4 | `--journal`, `--log`; `journal-async-busy` | `tools/w2w/src/main.rs`, `crates/engine/benches/alloc.rs` | `scripts/bench.sh` → `journal-async-busy 0`; **đảo chiều**: một `to_vec()` trong đường `Async` push → đọc ≥ 1; file journal sau `close` đúng số record | 1 | bất kỳ | sonnet | A3b (cùng file `main.rs`, chạy **sau**) |
| A5 | `check-machine.sh` đọc IRQ, coalescing, irqbalance, `busy_read` | `scripts/check-machine.sh` | không `FIXBOLT_NIC`, không carrier → `pass 12 fail 0 unknown 1` **y nguyên**; với NIC: **đảo chiều** ghi `6` vào một `smp_affinity_list` → FAIL nêu đúng IRQ; `rx-usecs 3` → FAIL, `0` → PASS; `shellcheck -S info` sạch | 4, 10 | Linux-desktop (NIC không cần carrier để test FAIL/UNKNOWN; arm PASS chờ boot B) | sonnet | — |
| A6 | `w2w-baseline.sh` nhận interval, `LISTEN`, `GENERATOR_SSH`, `FIXBOLT_NIC` | `scripts/w2w-baseline.sh` | không biến mới → lệnh sinh ra byte-giống hôm nay (so `bash -x` hai bản); `shellcheck -S info` | 10 | bất kỳ | sonnet | A3a |
| A7 | `nanofix` build được không Aeron; bin ví dụ nhận Logon của w2w | `vendor/nanofix/` (gitignored), `docs/reference/measured-costs.md` (mục mới, chỉ cách vá, chờ số) | `w2w --connect 127.0.0.1:<port> --path admin --messages 100` vào bin đó: Logon nhận, 100 `Heartbeat` về; ghi **hash commit** nanofix | 9 | bất kỳ | sonnet | A3a |
| A8 | **Senior review PR 1** theo plan này và gate, không theo lý do của manager | — | mọi gate A1–A7 chạy lại, quote | — | Linux-desktop | **opus** | A1–A7 |
| B0 | Vào §9, chứng minh bằng `/proc/cmdline`, `fixbolt-machine status`, `check-machine.sh` | runbook ở trên | `cat /proc/cmdline` có ba tham số, **không** `nohz_full`; `FIXBOLT_NIC=enp9s0 check-machine.sh` → `fail 0 unknown 0` | 10 | boot B + cáp | manager tự chạy (lệnh đọc) + **haiku** quote output | PR 1 merge; Sửa 6 `tls` merge |
| B1 | 52 đóng; D_in có số | `benches/baselines.tsv`, `DESIGN.md` §6 §8, `STATUS.md` | `bench.sh --strict` ×3 `timing over baseline 0 · cases w/o a baseline 0 · cases under the band 0` | 10 | boot B | haiku chạy + quote; manager viết docs | B0 |
| B2–B5 | bảng §8 loopback 2026-09-13, TLS ba arm, hàng cold, hàng journal/log | `DESIGN.md` §8, `measured-costs.md`, plan `tls` | `w2w-baseline.sh` mỗi arm 20 run, `check-machine.sh` quiet mỗi run | 4, 10 | boot B | haiku chạy + quote | B1 |
| B6 | item 40: hàng NIC-to-NIC **met** với topology, hoặc *not met* nêu lý do (cáp, `hw-rx-missing`) | `DESIGN.md` §6 §8 §9, `hft-playbook.md` §4, `best-practices-hft.md` | `wire p50/p99/p99.9` 20 run; `hw-rx-missing 0 hw-tx-missing 0`; A/B busy_read; arm IRQ-on-engine-core | 4, 10 | boot B + cáp | haiku chạy + quote; **opus** đọc kết quả bất thường | B0, A3b, A6 |
| B7 | item 51 nghi phạm 1 có số | `docs/reference/a-loopback-write-costs-thirty-two-syscalls.md` (mục mới), `STATUS.md` | hai phép đo trước/sau; `nft list ruleset` sạch sau khi xoá | 10 | boot B | manager chạy (đổi máy) + haiku quote | B2 |
| B8 | đối chứng nanofix, `standard` | `measured-costs.md`; `prior-art.md` trỏ sang, **hàng claim giữ nguyên** | 20 run mỗi bên, cùng `check-machine.sh` | 10 | boot B | haiku chạy + quote | A7 |
| B9 | bảng symbol chênh giữa hai path | `measured-costs.md` (nhãn diagnostic) | `perf report --stdio` hai file | — | boot B | haiku chạy; **opus** đọc | B1 |
| B10 | **Senior review PR 2**: số, nhãn, bảng §4 đi từng hàng, *Not proven* đọc từng dòng | — | — | — | bất kỳ | **opus** | B1–B9 |
| C1 | item 51 nghi phạm 2 (tuỳ Q1) | `a-loopback-write-costs-thirty-two-syscalls.md`, ADR-0023 (ghi chú ngày, không sửa quyết định) | `check-machine.sh` **FAIL** hàng mitigations; hai phép đo B7 lặp lại | 10 | boot C | manager + haiku | B7, Q1 |
| D | Về desktop | STATUS *Start here* | `/proc/cmdline` không `isolcpus`; `fixbolt-machine status` powersave | — | boot D | manager | — |

**Việc không thuộc plan này, và vì sao** (quyết định của architect, chủ sở hữu có thể đảo):

- **Item 32 (a)** — `serve_sharded_hft` không có `_with_recovery`: là **thay đổi entry point của
  engine** (§1 hàng 1, cần plan riêng), cần **Linux** nhưng **không cần boot §9** và không cần máy
  này — bất kỳ Linux nào, kể cả cloud. Gom vào đây là để một thay đổi API chờ một sợi cáp. Plan
  riêng, có thể làm ngay trong cửa sổ A bằng một phiên khác nếu muốn.
- **Item 36** — corpus mirrored 10/50: thuần `session`, không chạm máy; việc kế tiếp là **phân loại
  34 file còn lại** rồi một ADR thay trần 45 của ADR-0006. Không liên quan gì tới bàn Linux.
- **`const-templates-in-dict`** (đợt D): xem Q4.

## Cách kiểm chứng

- **Mọi gate quote nguyên văn**, đọc output chứ không đọc exit code (§10). Haiku chạy và quote;
  manager chạy lại gate đóng bước trên commit đóng bước.
- **Đảo chiều cho mỗi guard mới** (bảng *Chia việc* cột test): A1 xoá dòng / đổi tên file; A3b
  chạy trên `lo` phải in `missing = tổng`; A4 `to_vec()` trong đường Async; A5 ghi `6` vào một IRQ.
- **Số đo**: mỗi hàng mới kèm lệnh, `check-machine.sh` output đầy đủ (pass count có thể là 13–15
  khi có NIC — ghi đúng số, không "chuẩn hoá" về 12), kernel, topology, `hw-*-missing`.
- **Hai mode, hai path** cho mọi số w2w (ADR-0013); TLS ba arm cùng boot với plain.
- **Số boot C** chỉ được xuất hiện dạng *hiệu số A/B* cạnh số boot B và câu của ADR-0023.
- **CI run id** cho commit đóng mỗi PR (§9 hộp cuối).

## Câu hỏi cho chủ sở hữu — mỗi câu một quyết định, kèm đề nghị

| # | Câu hỏi | Đề nghị | Nếu "không" |
|---|---|---|---|
| **Q1** | Boot thêm một lần với `mitigations=off` (boot C, ~1 giờ) để thử nghi phạm 2 của item 51? | **Có**, lần cuối của buổi, chỉ hai phép đo, nhãn A/B, không công bố. ADR-0023 không cấm đo — nó cấm so | Item 51 đóng với câu *"conntrack đã thử, mitigations chưa"*, và `DESIGN.md` §8 *Floor* ghi rõ là số của máy có mitigations |
| **Q2** | Thêm rule `notrack` cho `lo` **tạm thời** (vài phút, xoá ngay) ở boot B? Và — **chỉ nếu ông ở bàn** — dừng `tailscaled` + flush ruleset 10 phút? | **Có** cả hai, tạm thời. Rule `notrack` chỉ trên `lo` không đụng NAT của Tailscale. **Không** đề nghị để vĩnh viễn | Chỉ đo arm flush (nếu ở bàn) hoặc item 51 đóng nửa |
| **Q3** | Mua/cắm **một cáp Ethernet** (Cat5e trở lên, ≥ 1 m, thẳng — I211 auto-MDIX) và **một adapter USB-C → Gigabit Ethernet** cho Mac? | **Có** — không có thì item 40 vẫn mở; mọi việc khác của boot B vẫn làm được | B6 bỏ, §6 hàng NIC giữ *not met*, plan nói rõ |
| **Q4** | `const-templates-in-dict` (đợt D): giữ hay bỏ? | **Bỏ khỏi thứ tự đợt** (đánh `Bỏ`). Ông đã quyết 2026-09-05 *"không đáng một thay đổi codec cho 2,9 %"*; hạng mục template **đã đo** (237,6 ns) và **đã trừ** khỏi 3 898 ns, nên kết quả item 49 không thể làm nó đáng hơn 570 ns | Giữ Draft, chờ D_in của B1; không có gì ở đây phụ thuộc nó |
| **Q5** | `mlockall` + read-back `VmLck` thành code trong plan này? | **Không** — không phép đo nào cần; §9 hàng đó giữ là prose và ghi thêm một câu nói rõ nó là prose | Thêm bước A-mlock (sonnet + review opus, chạm `engine`), +½ ngày |
| **Q6** | Sau A1, đo lại **cả 28** baseline n = 20 hay chỉ case đỏ sau ba lần `--strict`? | **Chỉ case đỏ**, ghi `n` thật. Band ADR-0031 sinh ra để chịu đúng trôi này; 28 × 20 run là nửa ngày máy | Cả 28, boot B dài thêm nửa ngày |
| **Q7** | B3 (tls bước 6) có trong boot B **chỉ khi Sửa 6 đã lên `main`**. Nếu tới ngày đó chưa xong: **chờ** hay **boot không có tls**? | **Boot không chờ** — tls bước 6 nhận một boot riêng sau | Boot B lùi |

## Phần cứng chủ sở hữu phải chuẩn bị (cho B6)

1. **Cáp Ethernet Cat5e/Cat6, thẳng, ≥ 1 m** — I211 báo `MDI-X: off (auto)` nên không cần cáp chéo.
2. **Adapter USB-C (hoặc Thunderbolt) → Gigabit Ethernet** cho Mac; adapter 2.5G cũng được, sẽ
   negotiate 1G.
3. Mac có Rust toolchain, `cargo build -p fixbolt-w2w --release` (không `affinity`), và ssh từ máy
   Linux vào Mac (cho `GENERATOR_SSH`).
4. Địa chỉ tĩnh hai đầu: `192.168.77.1/24` (Linux, `nmcli`) và `192.168.77.2/24` (Mac).

Không có (1) + (2): boot B vẫn đóng 49, 51, 52 và điền §8; chỉ item 40 chờ.

## Tài liệu phải cập nhật

- [ ] `DESIGN.md` §6: hàng *NIC to NIC* (met + topology, hoặc not met + lý do); hàng *Timing*
      thêm `engine turn, 1 busy, admin`; §6 *Recorded baselines* ghi ngày đo lại case nào
- [ ] `DESIGN.md` §8: bảng 1 đo lại 2026-09-13; hàng journal/log có số; hàng cold (`--interval`);
      bảng *3 898 ns* thêm dòng D_in và phần *ngoài tiến trình*; *Floor* ghi kết quả item 51
- [ ] `DESIGN.md` §9: hàng IRQ (giờ có check); hàng mới *coalescing off*; hàng `SO_BUSY_POLL` viết
      lại thành *`net.core.busy_read`/`busy_poll`* với số A/B; hàng `mlockall` ghi *prose*
- [ ] `docs/hft-playbook.md` §4: lệnh IRQ, `ethtool -C`, `FIXBOLT_NIC`; `best-practices-hft.md`
      nêu mode
- [ ] `docs/reference/measured-costs.md`: mọi số; mục nanofix; bảng perf (diagnostic)
- [ ] `docs/reference/a-loopback-write-costs-thirty-two-syscalls.md`: kết quả hai nghi phạm
- [ ] `docs/reference/recording-a-baseline-changed-the-baseline.md`: ghi cách đóng (A1)
- [ ] `docs/reference/prior-art.md`: hàng claim nanofix **giữ nguyên là claim**, thêm trỏ sang
- [ ] `docs/decisions/ADR-0062-*.md` (A1); ADR-0023 ghi chú ngày nếu boot C chạy
- [ ] `docs/CONFORMANCE.md` §6: câu *no latency figure lives here* không đổi; thêm CI run id
- [ ] `STATUS.md`: item 40, 49, 51, 52, 45 (b); **`Not proven` đọc từng dòng** — bullet *four
      buffer defaults … `benches/turn.rs` at two values of `RX` … belongs to wave C* **không** được
      làm ở đây, nói rõ ra thay vì để nó ngụ ý đã xong
- [ ] `benches/baselines.tsv` header: cơ chế đọc mới; dòng nào đo lại
- [ ] `CHANGELOG.md`: cờ w2w mới (tool, không phải API crate)
- [ ] Plan `tls`: *Nhật ký* bước 6 nếu B3 chạy

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Khôi phục `grub.fixbolt-s9` — file đó còn `nohz_full` | B0: `grep CMDLINE` **trước** `update-grub`, và `/proc/cmdline` sau boot; `nohz_full` xuất hiện = dừng |
| RX timestamp của `igb` rơi về software im lặng sau link change, readback nói dối (patch 2026-09-10) | A3b: `ts[2] == 0` đếm vào `hw-rx-missing`, in ra; B6 chỉ công bố khi `missing 0`; nếu ≠ 0 thử `ethtool -T` + down/up, và ghi kernel version |
| Tap AF_PACKET có timestamp TX phần mềm — trộn vào cột phần cứng | A3b: TX **chỉ** từ errqueue `ts[2]`; test `never_mixes_software_into_the_hardware_column` |
| `POLLERR` từ errqueue đánh thức `poll` của `standard` → engine spin đọc `EAGAIN` | `check-standard-gives-the-core-back.sh` với `W2W_EXTRA`; CPU > 5 % là đỏ |
| `igb` chỉ giữ **một** TX timestamp đang chờ (đọc của architect từ `igb_ptp.c`, **chưa xác minh**) — Heartbeat trùng reply làm mất một stamp | `hw-tx-missing` in ra; back-to-back chỉ có một reply trong chuyến nên kỳ vọng 0; ≠ 0 là phát hiện, ghi lại |
| `--interval` bằng `sleep` → jitter scheduler vào số | A3a: `strace` client không `nanosleep` trong cửa sổ |
| So NIC với loopback như cùng thang; so RTT của Mac với wire của acceptor | §8 bảng riêng; w2w in hai bảng nhãn khác nhau |
| `busy_read` "không thay đổi gì" vì đo trên loopback | A/B busy_read **chỉ ở B6** (NIC); loopback ghi *not applicable* |
| `check-machine.sh` pass count đổi (13–15 với NIC) làm chuỗi trong `baselines.tsv` "khác" | A5: không NIC → y nguyên `pass 12 … unknown 1`; có NIC → ghi chuỗi thật, header tsv giải thích |
| Ghi baseline làm đổi binary (item 52) lặp lại ở A2 | A1 đi **trước** A2 trong cùng PR; B1 chạy `--strict` trên binary cuối |
| `nanofix` từ chối Logon (CompID, BeginString) hoặc không có app reply | A7 thử trên loopback ở máy bất kỳ **trước** boot; đối chứng chỉ arm có thật |
| Đem `hft` so với engine chặn | B8: `standard`; `hft` nếu in là hàng riêng tự khai |
| Số của bản nanofix đã vá đọc như số của nanofix | hash commit + mô tả vá bằng lời cạnh con số |
| Dừng `tailscaled` khi đang điều khiển qua Tailscale → mất phiên | B7 arm flush **chỉ khi ở bàn**; Q2 |
| NetworkManager chạy DHCP trên `enp9s0` khi cắm cáp, link flap, IRQ affinity bị đặt lại | B0 đặt tĩnh **trước** khi cắm; đọc lại `smp_affinity_list` sau khi link up |
| Run đầu sau reboot | bỏ, N + 1 |
| Boot C sinh số đẹp và ai đó dán vào §8 | Boot C không chạy `w2w-baseline.sh`; mọi số ghi dạng hiệu số cạnh câu ADR-0023 |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Kernel 7.0.0-31 không có patch igb → `hw-rx-missing` ≠ 0 sau link change | Trung | Thử `ethtool -T` bật lại, down/up; nếu vẫn mất: item 40 ghi *not met, vì driver*, kèm link patch — đó là kết quả |
| `AF_PACKET` + `setcap` làm `w2w` khác binary "thật" | Thấp | Chỉ `--wire-timestamps` mở socket đó; không cờ, không cap cần thiết |
| D_in gần 3 898 → framing là câu trả lời; hoặc D_in ≈ 1 200 → kernel là câu trả lời lần ba | — | Cả hai là kết quả; item 49 đóng hoặc thu hẹp với số |
| Sửa 6 của `tls` chưa lên `main` khi tới ngày boot | Trung | Q7: boot không chờ |
| Hai architect sửa hai plan cùng lúc, cùng nhắc `check-no-kernel-sleep.sh` | Thấp | Plan này chỉ thêm biến `W2W_EXTRA`; ai tới sau đọc script trước khi sửa |
| Boot B dài hơn một ngày | Trung | Thứ tự B1→B9 là thứ tự ưu tiên; cắt từ B9 ngược lên, nói rõ cái gì bị cắt |

## Ngoài phạm vi

Kernel bypass (item 14); `io_uring`; NIC 10/25G hay switch; đo trên cloud; `SO_PREFER_BUSY_POLL`
thành code (chỉ sau khi A/B sysctl có số); `mlockall` code (Q5); item 32 (a) và 36 (plan riêng, lý
do ở *Chia việc*); sửa hay gửi PR cho `nanofix`; để `notrack` hay `mitigations=off` lại trên máy.

## Nhật ký giao hàng

*(Đã duyệt 2026-09-13. Cửa sổ A bắt đầu và xong ngày 2026-09-14 — các mục ngày đó bên dưới.)*

**`[2026-09-14]` B3 đã làm xong, ngoài plan này.** Tls bước 6 (6-M và 7b) chạy trong một boot §9
riêng ngày 2026-09-14, theo đúng Q7 ("boot không chờ — tls bước 6 nhận một boot riêng"), nhánh
`plan/tls-numbers-on-s9` — xem nhật ký phiên B của [plan `tls`](2026-09-04-tls.md). **Boot B bỏ
B3.** Hai điều từ lần đó chạm vào plan này:

- **B2 so với cả hai lần đo của ngày 2026-09-14, không chỉ với bảng 2026-09-02.** Ba arm plain
  loopback (`hft` admin, `hft` app, `standard` admin, arm `off`) đã chạy hai lần trong boot đó —
  `DESIGN.md` §8 *The round trip under TLS, measured* và `measured-costs.md` *TLS on the wire* —
  và lệch bảng 2026-09-02 tối đa 2,1% ở p50. B2 chạy arm `standard` app thì chưa có số mới để so.
- **Item 85 chạm cách B2 công bố.** Cùng lệnh, cùng boot, cách nhau nửa giờ, p50 của một arm dịch
  15,9% trong khi spread của nó ở lần 1 ghi 1,008, và p99 của hai arm kTLS dịch 7–19% (một arm có hai
  cụm run trong lần 1). Nếu B2 chỉ chạy `w2w-baseline.sh` một lần rồi đưa median vào
  §8, nó lặp lại đúng điều item 85 ghi. Trước khi B2 công bố, đọc
  [a-tight-spread-inside-one-procedure-did-not-reproduce-across-two](../reference/a-tight-spread-inside-one-procedure-did-not-reproduce-across-two.md)
  và xem item 85 đã có plan chưa.

**`[2026-09-14]` Bắt đầu Cửa sổ A (PR 1).** Nhánh `plan/the-second-linux-desk-a` từ `main` `25e54dc`.
Máy: bàn Linux, đang boot dòng §9 nhưng `fixbolt-machine off` (12 lõi) — cửa sổ A không đo gì để
công bố. Xác minh lại trên `25e54dc` trước khi chia việc, đọc thẳng từ code:

- `crates/codec/benches/harness.rs:81` vẫn `include_str!("../../../benches/baselines.tsv")` — A1 còn nguyên.
- `crates/engine/src/transport.rs:243` `pub const fn socket(&self) -> &TcpStream` — A3b lấy fd được
  mà không sửa `crates/engine`, như Sửa 1 viết.
- `crates/engine/benches/density.rs` chưa có case `engine turn, 1 busy, admin`; `tools/w2w/src/main.rs`
  chưa có `--listen`/`--connect`/`--interval`; không script nào có `W2W_EXTRA`;
  `scripts/check-machine.sh:362-367` hàng NIC IRQ vẫn chỉ đếm dòng.
- `enp9s0` là `igb`, **NO-CARRIER** — chưa có cáp, nên nhánh PASS của A5 chờ boot B như plan đã ghi.

**Ba chỗ khác plan, nói ra trước khi làm:**

1. **Số ADR của A1 là ADR-0067, không phải ADR-0062.** ADR-0062 đến ADR-0066 đã được dùng sau ngày
   plan viết. Chỉ đổi số, quyết định giữ nguyên.
2. **ADR-0067 do architect viết, không phải developer như cột Tier của A1.** `CLAUDE.md` §12 giao ADR
   cho architect, và luật đó thắng cột Tier của plan. Developer vẫn làm phần code của A1.
3. **A3a "build một lần trên macOS" bị chặn**: các phiên Mac đang offline. Mọi thứ khác của A3a vẫn
   làm; bằng chứng thay thế (nếu cài được target) là `cargo check --target x86_64-apple-darwin -p
   fixbolt-w2w`, **ghi rõ là check chéo, không phải một lần build trên Mac**, và việc build Mac còn nợ.
4. **`ARMS` của A6 là `mode:path:tls:interval`, không phải `mode:path:interval`.** Plan viết A6 trước
   khi bước 6b của plan `tls` biến trường thứ ba thành `tls`; interval thành trường thứ **tư**, tuỳ
   chọn, mặc định `0`, và `mode:path`, `mode:path:tls` giữ đúng nghĩa hôm nay. Chỉ đổi chỗ đặt trường.
5. **A3a do senior developer làm, không phải developer.** Plan không nói hai nửa `--listen`/`--connect`
   dừng khi nào và in gì, và A3b (opus) xây trên cùng file ngay sau — theo luật route lên của
   `CLAUDE.md` §12. Mỗi lựa chọn nó đưa ra nằm trong module doc của `tools/w2w/src/main.rs`, mục
   *Two halves, and pacing*.

**`[2026-09-14]` A5 xong, `f48f3ae`.** **A3a xong, `658b5c6`.** Gate đóng từng bước manager chạy lại
trên cây của commit đó; bằng chứng nằm trong thân commit. Nhánh PASS của hàng NIC IRQ thấy được ngay
hôm nay bằng cách đặt tạm năm IRQ của `enp9s0` sang `cpu4` rồi trả về `0-15` — plan tưởng phải chờ
boot B.

- *A5 — đã chứng minh:* không có NIC thì output y nguyên bản cũ (`diff` exit 0); đặt một IRQ vào
  CPU isolated → FAIL nêu đúng IRQ; `rx-usecs 3` → FAIL, `0` → PASS. *Chưa:* nhánh PASS trên một
  NIC **có cáp**. Bẫy `comm` so theo locale chứ không theo số — ghi ở
  [comm-compares-by-locale-collation-not-number](../reference/comm-compares-by-locale-collation-not-number.md);
  lúc đó chỉ có đảo chiều bằng tay, từ `970b612` có test tự động (xem A8).
- *A3a — đã chứng minh:* hai nửa `--listen`/`--connect` chạy với nhau, `allocs 0` mỗi bên;
  `strace` client dùng `--interval` không có `nanosleep`/`futex`; không cờ thì output giống bản
  cũ. *Chưa:* build thật trên Mac (chỉ `cargo check --target x86_64-apple-darwin`), chạy qua cáp,
  tách đôi có TLS. Bẫy mới:
  [a-masked-diff-compared-the-padding](../reference/a-masked-diff-compared-the-padding.md), chưa
  có test tự động.

**`[2026-09-14]` A1 xong, `bdd673f`.** `benches/baselines.tsv` được đọc **lúc chạy**, không còn
nằm trong binary; [ADR-0067](../decisions/ADR-0067-the-baselines-are-read-at-run-time-not-compiled-in.md)
do architect viết, phần code do developer (sonnet). *Đã chứng minh:* sha256 của binary bench
`parse` giữ nguyên sau khi ghi thêm một dòng vào file (cargo không build lại); đổi tên file →
thoát 1, nêu đường dẫn; xoá một dòng → riêng case đó `NO BASELINE`. *Chưa:* ba lần
`bench.sh --strict` trên binary mới (B1); vì sao case nhỏ từng nhảy 8,2 → 6,3 ns; lỗ layout của
ADR-0049 vẫn mở. Item 52 đóng **một nửa**.

**`[2026-09-14]` A7 xong, `a7d5943`.** `nanofix` clone vào `vendor/nanofix` (gitignored), **ghim ở
`0f79bae`**. Hai bản vá ghi bằng lời trong `measured-costs.md`, không chép source. Phát hiện một lỗi
của chính nanofix: `FixServer` không cho Logon đi qua `validate_inbound_seq`, nên mọi message sau
đó bị coi là hở số thứ tự; bin ví dụ tự dựng vòng accept từ các mảnh public của nanofix và đặt sẵn
số thứ tự kỳ vọng, **không** vá nanofix. *Đã chứng minh:* `w2w --connect` vào nó, cả path admin
lẫn app, đều exit 0. *Chưa:* không có con số nào — đó là B8. Path app có trả lời, nên B8 có thể
đối chứng cả hai path.

**`[2026-09-14]` A6 xong, `dbeb135`.** `ARMS` nhận interval ở trường thứ tư (đã nói ở mục *Ba chỗ
khác plan*, điều 4); `LISTEN`, `GENERATOR_SSH`; `FIXBOLT_NIC` truyền xuống `check-machine.sh`. Sửa
luôn một lỗi có sẵn: dưới `pipefail`, dòng `VERDICT` in thành hai dòng mỗi khi `check-machine.sh`
thoát ≠ 0. *Đã chứng minh:* `bash -x` bản cũ và bản mới sinh cùng dòng lệnh `w2w` cho các arm hôm
nay; arm hỏng và TLS + `LISTEN` bị từ chối trước khi chạy. *Chưa:* `GENERATOR_SSH` qua ssh thật —
máy này không có sshd, Mac offline.

**`[2026-09-14]` A2 xong, `ed96d9a`.** Case `engine turn, 1 busy, admin` trong `density.rs`, cùng
khung với case app N = 1. **Đổi tier:** developer (sonnet) dừng giữa chừng **hai lần**, nên theo
`CLAUDE.md` §12 bước này lên senior developer (opus). **Một sự cố cần ghi:** một agent đã bị dừng
lại **tự chạy tiếp một lần** và bắt đầu làm; manager dừng nó lần nữa. *Đã chứng minh:* case in `NO BASELINE for AMD Ryzen 7 3700X` (đúng như chờ, tới B1); đảo
chiều đưa order thay cho `TestRequest` → assertion *"each of the 1 sessions must get exactly one
Heartbeat back"* đỏ, exit 101. *Chưa:* D_in (hiệu app − admin trong tiến trình) chưa đo — B1; chưa
thử đảo chiều giới hạn số turn setup.

**`[2026-09-14]` A4 xong, `607d40f` — chạy trước A3b.** Cùng file `tools/w2w/src/main.rs` với A3b,
nhưng A3b đang chờ Sửa 2, nên A4 đi trước (plan ghi ngược lại). **Làm lại bởi senior developer:**
bản của developer đưa mọi lời gọi journal qua một enum lúc chạy, nên **kiểu engine khi không có
cờ đã khác** — benchmark không cờ sẽ đo một đoạn code khác với đoạn `DESIGN.md` §8 đã đo. Bản cuối
chọn cờ **một lần** trước turn đầu, thành bốn kiểu engine cụ thể. *Đã chứng minh:* không cờ thì
`type_name` của engine giống hệt `ed96d9a` ở năm arm; `journal-async-busy 0`, đảo chiều bằng
`to_vec()` đọc 1; hai cờ ở `hft` và `standard` exit 0, `allocs 0`. *Chưa:* hai script luật 4 không
chạy arm có cờ; record ghi trong lúc `--listen` đang chạy không được đọc giữa chừng. **Và F13 của
review** — xem ghi chú cho B5 bên dưới.

**`[2026-09-14]` Sửa 2 viết (`e83d06b`) và được duyệt (`cce8bd4`).** Khi xây A3b, ba điều plan
viết sai: capability từ file mất khi chạy dưới `strace`; `SIOCSHWTSTAMP` cần `cap_net_admin`;
`POLLERR` từ error queue làm `standard` spin mà không gate nào thấy. A3b giữ chưa commit tới khi
chủ sở hữu trả lời Q8–Q11. Chi tiết ở mục *Sửa 2*.

**`[2026-09-14]` A3b xong, `f9abc1a`**, làm lại theo *A3b sau Sửa 2*, chồng lên A4. *Đã chứng
minh:* ba test `pair::` và ba test từ chối, mỗi cái đảo chiều đỏ đúng câu đã đoán; hai script luật
4 với `W2W_EXTRA='--wire-timestamps --nic lo --observer-core 2'` tự chạy trong `unshare -Urn` và
xanh, nửa đỏ vẫn đỏ; cùng lệnh dưới `aa-exec -p unconfined` → exit 2, *SKIPPED, NOT PASSED*;
`--mode standard --wire-timestamps --nic enp9s0` → exit 1 nêu `POLLERR`, card vẫn tắt timestamp;
tập syscall engine có/không cờ giống nhau ở `hft` và `standard`. **CI run
[`34842390918`](https://github.com/tmthang86/fixbolt/actions/runs/34842390918) trên `f9abc1a`: 14
job / 14 xanh** — lần đầu hai step user namespace chạy trên CI: runner **từ chối** userns, step bật
sysctl AppArmor (log in ra `0`), cả hai arm xanh. *Chưa:* chưa đọc được một stamp phần cứng nào
(`enp9s0` không cáp), nên phần đọc cmsg chỉ được chứng minh với stamp **vắng**; `POLLERR` chỉ thấy
bằng một bản build tạm dùng stamp phần mềm, đã xoá — không gate nào thấy nó, chỉ lời từ chối của
`w2w` canh. Hai ý để sau thành item mở trong `STATUS.md`: **86** (R5, gate không cần tracer) và
**87** (S3, stamp TX bằng BPF).

**`[2026-09-14]` A8 — senior review, sửa ở `970b612` và `6e716a5`.** Reviewer có context mới (opus), theo
`CLAUDE.md` §12. **16 phát hiện: 7 lỗi, 9 góp ý**; manager tái hiện hoặc đọc lại từng cái trước
khi giao sửa; sửa hết, **trừ F13**. Những cái đáng kể nhất:

- **F1** — `w2w-baseline.sh` không thể chạy B6: không có chế độ wire. Giờ có `WIRE_NIC`,
  `OBSERVER_CORE`, từ chối arm `standard` (Q10), và **FAIL mọi run thiếu stamp** (thiếu stamp là lỗi
  dụng cụ đo, không phải máy bận). Thử trên `lo` trong `unshare -Urn`: FAIL đúng câu
  `hw-rx-missing 2000, hw-tx-missing 2000`; nhánh thành công chỉ thử với một `w2w` giả.
- **F3** — Ctrl-C giữa run để lại card `enp9s0` **đang bật timestamp** (`tx on, rx-filter all`,
  thấy tận mắt). Giờ SIGINT/SIGTERM trả card về cũ (đọc lại: `tx off, rx-filter none`). Tín hiệu
  thứ hai, `kill -9` hay crash thì không — runbook `hft-playbook.md` §6 mục 4 ghi cấu hình trước B6
  và kiểm tra sau.
- **F4** (luật 8) — observer có thể `dup` một fd mà engine đã đóng sau khi hết thời gian chờ. Thêm
  trạng thái `CLAIMED`; bốn test, hai cái đỏ với cách cũ. *Chưa:* race dưới tranh chấp thật (test
  một luồng + lập luận CAS).
- **F8** — hai script luật 4 vẫn xanh với một build **bỏ qua** cờ `--wire-timestamps`. Giờ bắt buộc
  có dòng `wire-timestamps:` và `hw-rx/tx-missing N of M` với M > 0.
- **F9** — `baselines.tsv` nhận `nan`, `inf`, số âm. Giờ từ chối, nêu dòng;
  `crates/codec/tests/bench_baselines.rs` 6 test.
- **F10** — `Never` bị đổi cho `--listen` làm code của judge admin không cờ khác đi. Trả `Never` về
  như `25e54dc`, `--listen` dùng `ListenNever` riêng; `objdump` diff rỗng.
- **F11, F12** — `check-machine.sh` lấy IRQ từ `msi_irqs` trước, `smp_affinity_list` không đọc
  được thì `UNKNOWN`; `scripts/check-machine-verdicts.sh` test `irq_overlap`, `nic_irqs`,
  `coalesce_verdict` (`pass 27 fail 0`). Đảo chiều về `comm -12` làm mất CPU 14 và 15 — **mục bẫy
  `comm` cũ nói giao của hai tập vẫn đúng là sai**, đã sửa. **Bẫy `comm` giờ có test tự động.**
- F2, F5, F6, F7, F14, F15 — nhãn và tài liệu: generator qua ssh báo *không pin*; `LISTEN=…:0` dùng
  được; `DESIGN.md` §6 liệt kê đủ 31 đường cấp phát (trước ghi 27); tham chiếu theo số dòng đổi
  thành tên; mục *recording-a-baseline* ghi A1 đã đóng một nửa; nhãn `allocs` nêu mọi thread được
  đếm.

*Gate manager chạy lại trên `6e716a5`* (nguyên văn trong thân commit): fmt, clippy `-D warnings` với
`affinity,tls` / mặc định / `--no-default-features`, `cargo check --target x86_64-apple-darwin`,
`cargo test -p fixbolt-w2w` 23 passed, hai script luật 4 có và không `W2W_EXTRA` exit 0, shellcheck,
check-links. **Chưa:** một tín hiệu rơi đúng lúc run phần cứng đang chạy đường gộp.

**`[2026-09-14]` Ghi chú cho B5 — F13, chưa sửa.** `--journal file-async` dựng `FileJournal<64,
512>`, còn mặc định (`--journal mem`) là `Store` = `MemJournal<4096, 512>`. Nên B5 so hai arm đó là
**đổi hai thứ cùng lúc**: loại journal **và** kích thước vòng (4 096 → 64 ô) — hiệu số không gán
được cho cái nào (`CLAUDE.md` §10, mỗi lần một biến). **Phải giải quyết trước khi chạy B5**: cho
hai vòng cùng kích thước, hoặc thêm arm tách riêng kích thước. `STATUS.md` item **88**. Hàng B5 của
bảng *Chia việc* không sửa.

**`[2026-09-14]` Bẫy của cửa sổ A, mỗi cái ở đâu:**

- `comm` so theo locale — [comm-compares-by-locale-collation-not-number](../reference/comm-compares-by-locale-collation-not-number.md); test tự động `check-machine-verdicts.sh`.
- Diff có mặt nạ so luôn phần đệm — [a-masked-diff-compared-the-padding](../reference/a-masked-diff-compared-the-padding.md); chưa có test.
- Stamp TX đánh thức engine đang chặn — [a-transmit-timestamp-wakes-a-blocking-engine](../reference/a-transmit-timestamp-wakes-a-blocking-engine.md); canh bằng lời từ chối của `w2w` và test `standard_is_refused_on_a_hardware_nic_and_not_on_loopback`.
- **Tiến trình bị trace mất capability từ file, và AppArmor chặn user namespace tuỳ terminal (CI thì step phải bật sysctl)** — mục mới [a-traced-process-gets-no-file-capabilities](../reference/a-traced-process-gets-no-file-capabilities.md); canh bằng exit 2 *SKIPPED* của hai script và đảo chiều `aa-exec` (làm bằng tay, chưa có test tự động).
- `igb` chỉ giữ một stamp TX đang chờ — bộ đếm `tx_hwtstamp_skipped` đọc trước/sau mỗi run B6, ghi ở `hft-playbook.md` §6 mục 4.

**`[2026-09-14]` Đóng cửa sổ A (PR 1).** Mọi bước A1–A8 đã commit trên
`plan/the-second-linux-desk-a`; commit cuối của review là `6e716a5`; CI của commit đóng PR:
`0861edf` xanh, run [`34849545025`](https://github.com/tmthang86/fixbolt/actions/runs/34849545025), 14/14 job — run `34848285886` trên `2ad9bb0` đỏ một job rustdoc vì một link doc do bản sửa F3 thêm vào, bước nào của manager cũng chưa chạy `cargo doc -D warnings`. **Không có con số nào** được tạo ra trong cửa sổ này — máy ở dòng boot §9 nhưng
`fixbolt-machine off` và đang có việc khác chạy. Lúc 2026-09-14T20:09+07:00 `/proc/cmdline` vẫn có
`isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1`; `enp9s0` không có carrier.
**Tiếp theo: boot B** — đọc `/proc/cmdline` trước khi đụng grub (B0 có thể đã xong nửa grub),
`sudo -n /usr/local/sbin/fixbolt-machine on`, runbook B0, bỏ run bench đầu; **B1 trước tiên**
(ba lần `--strict`, baseline case admin n = 20, D_in); B3 đã xong ở PR #71; B5 chờ item 88; **B6
cần cáp và Mac**.

**`[2026-09-14]` Bắt đầu boot B (PR 2).** Nhánh `plan/the-second-linux-desk-b` từ `main` `4c373e0`.
**Không reboot**: máy đã ở dòng boot §9 từ 07:18 (`uptime` 14:08 lúc 21:27), nên bẫy *run đầu sau
reboot* không áp dụng; run bench đầu vẫn bỏ theo runbook. Không session nào khác đang đo.

*B0, đọc 21:27–21:29:*

- `cat /proc/cmdline` → `... isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1 ...`,
  **không** `nohz_full`. Grub không đụng.
- Trước khi bật: `scripts/check-machine.sh` → `pass 8 fail 7 unknown 0` (knobs OFF; NIC tự chọn
  `enp9s0`; IRQ 85–89 `0-15`; `rx-usecs 3`).
- `sudo -n /usr/local/sbin/fixbolt-machine on` → `governor performance · boost 0 · smt off · thp
  never · busy_poll 50 · tls loaded · nproc 6`; `sudo -n ethtool -C enp9s0 rx-usecs 0`; `4` ghi vào
  `/proc/irq/{85..89}/smp_affinity_list`.
- Sau: `FIXBOLT_NIC=enp9s0 scripts/check-machine.sh` → **`pass 15 fail 0 unknown 0`**.
- Cáp đã cắm, **thẳng vào cổng Ethernet có sẵn của một Mac mini** (không cần adapter của Q3):
  `192.168.77.1 ↔ .2`, `Speed: 1000Mb/s`, `Link detected: yes`. **Topology B6 của plan không nói
  tới hai điều đang bật ở cả hai đầu:** EEE (802.3az) `enabled - active`, và pause frames RX/TX.

**Bẫy gặp ở B0.** ~~`ethtool -C enp9s0 rx-usecs 0` làm `igb` **bật lại link**~~ **— nguyên nhân
này sai, sửa bên dưới.** Link nhảy (`dmesg`: `NIC Link is Up` lúc 21:29:01; `carrier_changes` 4).
`check-machine.sh` **không** `FIXBOLT_NIC`, chạy lúc 21:28:58, không thấy carrier nên **không chọn
NIC nào** — in `pass 12 fail 0 unknown 1` và vẫn *§9 satisfied*. Ba giây sau, cùng lệnh in `pass 15
fail 0 unknown 0`. Từ đây mọi bước B đặt `FIXBOLT_NIC=enp9s0` tường minh.

**`[2026-09-14]` Sửa nguyên nhân — architect tìm ra, manager kiểm lại.** `sudo -n journalctl
_COMM=sudo --since 21:20` ghi `ethtool -C enp9s0 rx-usecs 0` lúc 21:28:49.24 — link **không** nhảy
khi đó — và **`ethtool --set-eee enp9s0 eee off` lúc 21:28:58.387**, tiếp theo `ethtool --show-eee
enp9s0` 21:29:02.08 và `journalctl -k --since 21:28` 21:29:21.87. **Ba lệnh đó không phải của
session boot B**: cùng user, cùng thư mục repo, không có trong lịch sử lệnh của session này. Tắt EEE
gọi `igb_reinit_locked`, đó mới là lần link nhảy. Nên từ 21:29:02 **EEE ở desk đã tắt** (`EEE
status: disabled`) và Mac `en0` đọc `1000baseT <full-duplex,flow-control>`, không còn
`energy-efficient-ethernet`. Bẫy của `check-machine.sh` vẫn y nguyên, chỉ khác thứ làm link nhảy.
**Ai chạy ba lệnh đó: đang hỏi chủ sở hữu** — một tác nhân khác đổi máy trong boot B là rủi ro cho
mọi số đo.

**Việc chặn, gửi architect (Sửa 3):** B5 chờ item 88; B2 (và mọi hàng §8 của boot này) công bố
thế nào khi item 85 chưa có plan; EEE và pause frames cho B6; bẫy `igb` ở trên cần một test.

**`[2026-09-14]` Sửa 3 tới tay chủ sở hữu, chờ duyệt Q12–Q17.** Architect viết mục *Sửa 3* (cuối
file) và [ADR-0068](../decisions/ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md)
(Proposed), chỉ đọc máy 21:36–21:45. Hai điều manager ghi thêm, không đổi thiết kế:

- **Chủ sở hữu ở bàn.** Manager hỏi, chủ sở hữu trả lời *"tôi có ở bàn"* → arm flush của B7 chạy
  (Q2). Manager thêm một bước an toàn: `sudo -n nft list ruleset` ra file **trước** flush, và nạp lại
  bằng `sudo -n nft -f <file>` **sau** — vì `systemctl start tailscaled` chỉ dựng lại bảng của
  Tailscale, còn chain của Docker/iptables-nft thì không; `nft list ruleset` trước và sau phải giống
  nhau.
- **B1 chạy trước khi duyệt** (không phụ thuộc Sửa 3).

**`[2026-09-14]` Ai tắt EEE, và Sửa 3 được duyệt.** Chủ sở hữu trả lời: *"lệnh đó session khác chạy
theo chỉ đạo, tôi dừng rồi. Duyệt"*. Vậy ba lệnh 21:28:58–21:29:21 là của một session khác, theo lệnh
chủ sở hữu, và session đó đã dừng trước khi B1 đo run nào được giữ. Không có số nào trước thời điểm
này bị ảnh hưởng (chưa có số nào). Sửa 3 duyệt theo đề xuất — xem cuối mục *Sửa 3*.

**`[2026-09-15]` B1 xong — gate xanh ba lần.** Manager tự chạy (không qua runner: một vòng lặp nền
và vài lệnh đọc; runner Haiku hay treo khi chờ lệnh nền).

- *Run bỏ* 22:01–22:09, `bench.sh --strict` mất **7 phút 42 giây**, `exit 1`: `cases w/o a baseline
  3` — **plan chỉ nói tới một case thiếu baseline, thực tế có ba**: ngoài `engine turn, 1 busy,
  admin` còn `SendingTime from the cache, micros` và `…, nanos` (thêm từ plan timestamp-micros,
  chưa từng có baseline trên máy §9). Không ghi hai case đó thì gate B1 không bao giờ xanh, nên ghi
  cả ba, theo tinh thần Q6. Thêm hai case đỏ: `SendingTime from the cache` 5.8 (trần 5.4) và
  `validate NewOrderSingle, w2w bytes` 991.5 (trần 987.0).
- *Đo* 21 lượt `serialize` + `density` + `validate`, 22:10–00:14, mỗi lượt ~6 phút (`density` không
  lọc được case). Lượt 1 hàng quiet FAIL (`code 5% claude 3%` — chính session này) → loại; n = 20
  là lượt 2–21, lượt nào cũng `pass 15 fail 0 unknown 0`. S2 và S3 chạy song song tới khoảng lượt 5:
  median lượt 2–20 và 6–20 như nhau.
- *Ghi* 5 dòng `benches/baselines.tsv` (đoạn chú thích cuối file): mới `engine turn, 1 busy, admin`
  954.2, `…micros` 10.0, `…nanos` 12.7; sửa `SendingTime from the cache` 4.9 → 5.8 (+18%) và
  `validate NewOrderSingle, w2w bytes` 897.3 → 994.5 (+10,8%). Không nhận nguyên nhân.
- **Phát hiện:** mọi case khác của ba target nằm trong band nhưng chậm hơn dòng 2026-09-05 3,6–8,4%
  (`density` +3,6–4,8%, `validate NewOrderSingle` 882.1 → 955.9). Không ghi lại (Q6), chưa tách
  nguyên nhân: code đã merge từ 2026-09-05, máy, hay cả hai.
- **D_in = 765.5 ns** (paired 767.7) → `DESIGN.md` §8 *The 3 898 ns, added back*, `STATUS.md` item 49.
- *Gate*, 00:15–00:38, cây có `baselines.tsv` sửa (đọc lúc chạy, ADR-0067), ba lần như nhau:
  `pass 15 fail 0 unknown 0` · `targets measuring 16 of 16` · `timing over baseline 0` · `cases w/o a
  baseline 0` · `cases under the band 0` · `bench_exit=0`. Item 52: ba lần `--strict` còn nợ đã trả.

**`[2026-09-15]` Khe build (00:40–00:57).** S3 (`46811f6`) và S2 (`d5be4d6`) làm trong hai worktree
riêng, manager chạy lại gate rồi cherry-pick; S1 (`5ca3889`) làm ở cây chính. Gate quote trong
thân từng commit. Thêm: rustdoc `-D warnings` cho `w2w` bốn bộ feature và `fixbolt-engine` — sạch
(STATUS bảo thêm sau lần CI đỏ của PR #72). `setcap` trên `w2w` (sha256 `350d3c17320f`); Mac pull
`5ca3889`, build lại (`7b2b52cb9be7`). Lần chạy thật đầu tiên của script S2: header, thư mục output,
`summary.txt`, `extra`, kiểm tra `journal: file-async` — đều đúng. Bin ví dụ nanofix `0f79bae` còn
nguyên, không build lại. B8 không chạy được bằng `w2w-baseline.sh` (acceptor ngoài), nên manager
viết một script cùng khuôn split; nội dung chép nguyên văn vào `measured-costs.md` *Boot B*.

**`[2026-09-15]` Procedure 1 và 2 (B2 → B5 → B8 → B4), commit `5ca3889`, cây sạch, 0 run bị loại.**
Procedure 1 00:58–02:54, procedure 2 02:54–04:47. Số và nhận xét: `DESIGN.md` §8 *Boot B* và
`measured-costs.md` *Boot B*. Ba điều đáng nói:

- **Tám arm interval 0 (B2, B5, B8 phía fixbolt) đều nhanh hơn 5,0–6,7 % ở procedure 2**, trong khi
  dispersion trong từng procedure rất chặt — theo ADR-0068 là *không tái lập*. Các arm có pacing,
  nanofix, và bảng Mac thì tái lập. Manager chạy thêm **B2 lần 3 (chẩn đoán, không công bố)**
  04:48–05:01: `hft` khớp procedure 2 trong 0,1 % → procedure 1 là lần lệch. Ứng viên: procedure 1
  bắt đầu ~7 phút sau khe build.
- **Plan sai một con số ở B4 (Sửa 3 Điều 5 d):** `MESSAGES=120 WARMUP=5` ở interval 1 s làm engine
  từ chối seq 122 (`35=3 373=10`) — `w2w` render `52=` trước khi đo, 125 s > `MaxLatency` 120 s.
  Engine đúng. Manager chạy lại với `MESSAGES=100` (105 s), giữ nguyên quy tắc "1 s chỉ công bố p50".
  Bẫy mới, chưa có guard — STATUS item 90.
- `/tmp` là tmpfs: số B5 không phải số đĩa.

**`[2026-09-15]` B7 (05:03–05:29), A–B–A, nửa notrack.** Arm flush **bỏ**: chủ sở hữu đi ngủ, không ở
bàn (Q2). Bảng `ip fixbolt` thêm rồi xoá; ruleset sau khi xoá giống trước (diff chỉ khác bộ đếm
packet). conntrack trên `lo` ≈ 420 ns của một round trip TCP loopback 8 byte; `w2w` `hft` admin
18 249 → 15 364 → 18 179 với hai pha A **hai mode** — ứng viên, không phải nguyên nhân (STATUS item 51).
Payload bench chạy thẳng binary (không cargo), 5 lần mỗi pha vì mỗi lần 74 s.

**`[2026-09-15]` B6 (05:31–07:49), qua cáp tới Mac.** Lượt thử đọc được **stamp phần cứng lần đầu**
(`hw-rx-missing 0 hw-tx-missing 0`). Hai procedure: wire `hft` ở 1 s đo được nhưng không tái lập
(5,2 % / 10,1 %); **interval 0 hỏng ở mọi lần** vì `igb` bỏ một TX stamp trong 1–4 run
(`tx_hwtstamp_skipped` tăng theo), và luật Sửa 2 cho FAIL run thiếu stamp → không có số wire
back-to-back (STATUS item 40, cần một quyết định của architect). Bảng Mac `standard` tái lập. A/B:
EEE bật +14,6 µs ở 1 s → Q15 kích hoạt; busy_poll 0 và IRQ trên cpu6 chỉ có run đơn chẩn đoán vì
cũng hỏng vì thiếu stamp. Trap EXIT trả máy về busy_poll 50, IRQ cpu4, EEE disabled — đọc lại lúc 07:49.

**`[2026-09-15]` B9 (07:51), perf, chẩn đoán.** Engine thread `hft`: `Acceptor::accept` 49,9 %
(admin) / 36,7 % (app) số sample; `accept4` trên listener rỗng cấp phát rồi huỷ socket file + inode
trong kernel. Không phải chi phí mỗi message — STATUS item 89.

**`[2026-09-15]` Q15 (`0057678`).** `check-machine.sh` hàng `eee`, `DESIGN.md` §9 hàng *EEE off*;
verdicts `pass 37 fail 0`; desk `pass 16 fail 0 unknown 0`.

**Một sự cố nhỏ:** file output thô của B1 (`b1-discard.txt`, `b1-strict-*.txt`) mất khỏi scratchpad
giữa 00:38 và 05:05, không rõ vì sao (nghi một subagent dọn scratchpad). Dòng verdict còn trong thân
`547c873`.

## Sửa 1 — 2026-09-13, xác minh lại trước khi duyệt

Draft 2026-09-04 được đọc lại từng dòng đối chiếu với code, máy và STATUS ngày 2026-09-13. Những
gì đổi, và vì sao:

**Đã xong, bỏ khỏi plan:**

- **Item 39** (dictionary pass) và **item 41** (`bench.sh --strict` đỏ) — draft ghi là việc ngày 1
  bước 1 và 4; cả hai **đóng 2026-09-05**. `benches/dict_pass.rs` draft định viết đã tồn tại dưới
  tên `crates/session/benches/validate.rs`.
- Case alloc **`log-busy`** — draft nói "nếu plan message-log chưa thêm"; đã có, là một trong 27.

**Sai, sửa:**

- **Cách đo NIC.** Draft đặt `SO_TIMESTAMPING` "trên client" và đọc cmsg. Hai điều sai: (1) client
  là máy kia, không có I211; (2) muốn RX **và** TX phần cứng trên một clock thì RX phải qua tap
  (engine dùng `recv`, không `recvmsg`) và TX phải qua errqueue của socket engine — tap không có TX
  phần cứng (libpcap #894). Thiết kế mới ở A3b, không chạm `crates/engine` nhờ `TcpTransport::socket()`.
- **`SO_BUSY_POLL` như `Transport` option** — không cần code: sysctl `busy_read` đã đặt mặc định
  cho mọi socket; phép đo là A/B sysctl. Chỉ `SO_PREFER_BUSY_POLL` cần code, và chỉ sau khi có số.
- **"Máy thứ hai với NIC 10/25G, qua switch"** — STATUS item 40 `[measured 2026-09-02]` đã chỉ ra:
  một clock, máy kia chỉ cần echo, **Mac + cáp trực tiếp là đủ**, switch là thêm một hạng mục không
  ai đo.
- **Draft không biết máy đang ở cấu hình desktop** và không biết file grub backup nào đúng —
  `grub.fixbolt-s9` còn `nohz_full`. B0 viết rõ.
- **Draft không có item 49, 51, 52** (mở sau ngày draft) — giờ là ba trong bốn mục tiêu chính.
- **Draft không tách code khỏi đo theo boot** — mỗi reboot là hết phiên; giờ ba cửa sổ, ba PR.
- **Draft không nói gì về bước 6 của `tls`** — giờ B3 và Q7.

**Thêm:** bảng câu hỏi Q1–Q7 với đề nghị; phần cứng cần mua; quyết định về 32 (a), 36,
`const-templates-in-dict`; tra cứu internet có nguồn cho từng dòng; bẫy `igb` 2026-09-10.

**Giữ nguyên từ draft:** đối chứng `nanofix` (bước 7 cũ → A7 + B8) với ba điều phải nói cạnh số;
`--interval`; case `journal-async-busy`; bảng bất biến; hàng §9 coalescing và IRQ.

**Trạng thái: Đã duyệt 2026-09-13, theo đề xuất Q1–Q7** (chủ sở hữu: *"Duyệt cả 3 theo đề xuất"*). Buildable với điều kiện: cáp + adapter (Q3) cho B6; Sửa 6 của `tls`
trên `main` cho B3; Q1 cho boot C. Không có ba điều đó plan vẫn chạy được và nói rõ cái gì không
đóng.

## Sửa 2 — 2026-09-14, A3b: ba điều plan viết sai

A3b đã được senior developer xây xong trong worktree `a3b` (chưa commit). Khi xây, ba điều plan
khẳng định hoá ra sai, và cách làm rời văn bản plan ở năm chỗ. Theo `CLAUDE.md` §1 (*plan sai giữa
chừng → dừng, sửa plan, duyệt lại*), phần này ghi từng điều: sai ở đâu, bằng chứng, các lựa chọn,
**đề nghị**, và plan đổi gì. Manager đã tự tái hiện điều 1; architect tái hiện thêm hai đường chạy
gate hôm nay (ghi rõ bên dưới, nhãn *thăm dò*). Chủ sở hữu quyết ở bảng Q8–Q11 cuối phần.

Vài chữ dùng suốt phần này, nói bằng lời thường một lần:

- **capability** — quyền hệ thống gắn vào *một file thực thi* (`setcap`), để tiến trình chạy
  file đó làm được một việc root mà không cần là root. `cap_net_raw` = mở socket "nghe thô" trên
  card mạng; `cap_net_admin` = **đổi cấu hình** mạng (card, route, tường lửa — rộng hơn nhiều).
- **error queue** — mỗi socket có một hàng đợi phụ, kernel bỏ vào đó *lỗi* và **cả timestamp
  TX**. Đọc bằng `recvmsg(MSG_ERRQUEUE)`. Hàng đợi này không rỗng thì `poll` báo cờ **`POLLERR`**
  — và `poll` **luôn** báo cờ đó, dù mình có xin hay không.
- **user namespace** — một "thế giới riêng" mà tiến trình thường tự mở (`unshare -Urn`): trong
  đó nó được coi là root, có bộ mạng ảo riêng (kể cả `lo` riêng), nhưng ra ngoài thì không có
  thêm quyền gì.

### Điều 1 — gate của A3b không chạy được như plan viết

**Plan viết** (A3b, dòng 153–156; *Chia việc* A3b): arm `check-no-kernel-sleep.sh` với
`W2W_EXTRA="--wire-timestamps --nic lo"` "chạy được ở mọi máy Linux, không cần cáp".

**Sai ở đâu.** Script chạy `w2w` **dưới `strace`**. Kernel có luật: một tiến trình được exec
**khi đang bị trace bởi tracer không có `CAP_SYS_PTRACE`** thì **không được nhận capability từ
file** — cap bị cắt về đúng cái tiến trình cha đã có (`security/commoncap.c`,
`cap_bprm_creds_from_file`, ~dòng 1063–1075: `if ((is_setid || __cap_gained(permitted, new,
old)) && ((bprm->unsafe & ~LSM_UNSAFE_PTRACE) || !ptracer_capable(current, new->user_ns))) {
… new->cap_permitted = cap_intersect(new->cap_permitted, old->cap_permitted); }`). Nên
`setcap` trên `target/release/w2w` **đúng** khi chạy thẳng và **vô dụng** khi chạy dưới `strace`
của user thường.

**Bằng chứng.** Manager tái hiện 2026-09-14: `getcap target/release/w2w` đọc
`cap_net_admin,cap_net_raw=ep`; chạy thẳng exit 0; dưới script → `socket(AF_PACKET): Operation
not permitted`, exit 1. Architect lặp lại hôm nay với bản copy binary **không** cap:

```text
$ ./w2w-nocap --mode hft --messages 100 --warmup 10 --hold-ms 50 --wire-timestamps --nic lo --observer-core 2
Error: … "w2w: --wire-timestamps: socket(AF_PACKET): Operation not permitted (os error 1)"
rc=1
```

**Các lựa chọn** (mỗi cái đã thử hoặc đã tra):

| # | Cách | Đã thử? | Được | Mất |
|---|---|---|---|---|
| R1 | **Chạy arm này trong user namespace**: `unshare -Urn sh -c 'ip link set lo up && strace -f … w2w --wire-timestamps --nic lo …'` | **Có, hôm nay** — binary **không** cap, dưới strace: `mode: hft`, `engine-tid: 88645`, `hw-rx-missing 300 of 300`, `hw-tx-missing 300 of 300`, `tap drops 0`, rc=0; engine tid có 33 618 syscall được trace. Arm `standard` cũng chạy (tid 88777, 2 195 syscall) | Không sudo, không `setcap`, `w2w` không bao giờ là root thật; trong namespace nó có `cap_net_raw` trên `lo` *riêng* (`af_packet.c` ~3949: `ns_capable(net->user_ns, CAP_NET_RAW)`); `SIOCSHWTSTAMP` không bị đụng vì `lo` là loopback. Đường kernel qua `lo` trong namespace **giống hệt** `lo` ngoài | **Ubuntu ≥ 24.04 chặn user namespace cho tiến trình không có profile AppArmor cho phép** (`kernel.apparmor_restrict_unprivileged_userns = 1` trên máy này). Hôm nay chạy được **chỉ vì** terminal VS Code mang profile `vscode` có quyền `userns`; từ shell thường (`aa-exec -p unconfined`) → `unshare: write failed /proc/self/uid_map: Operation not permitted`. Mở khoá: một dòng `sudo -n sysctl -w kernel.apparmor_restrict_unprivileged_userns=0` **cho phiên đó** (mất sau reboot — cố ý, vì Qualys 2025 đã công bố ba cách vượt rào dựa trên userns). Runner CI (Ubuntu 24.04) **chưa biết**: mặc định cũng là 1; runner có sudo để bật, thấy ở lần CI đầu |
| R2 | **`sudo -n strace -f -u tmt …`**: strace là root (nên `ptracer_capable` đúng), `-u` hạ con về `tmt` trước exec, cap từ file được giữ (`strace(1)`: `-u` "only useful when running as root, as it enables the correct execution of setuid … binaries") | **Có** — developer chạy một lần; architect lặp lại hôm nay: `mode: hft`, `engine-tid: 88873`, rc=0; file trace thuộc root (đọc được) | Không đổi cơ chế script, một chữ `sudo` | **Không thể vào CI**; gate của một bất biến thành gate-chỉ-ở-bàn; `strace` chạy root trên máy của chủ sở hữu mỗi lần gate chạy |
| R3 | `perf trace` thay `strace` (không dùng ptrace, dùng tracepoint) | Không | Không đụng cap | Ubuntu đặt `perf_event_paranoid = 4` → vẫn cần root; đổi công cụ của cả gate vì một arm |
| R4 | `setcap cap_sys_ptrace+ep` lên một bản copy `strace` | Không | Không sudo mỗi lần | Bản `strace` đó trace được **mọi** tiến trình, kể cả root — rộng hơn R2 mà không được gì hơn |
| R5 | Bỏ `strace` cho arm này; `w2w` tự in `voluntary_ctxt_switches` của engine tid (đọc `/proc/self/task/<tid>/status`) — `hft` phải là 0 trong cửa sổ đo | Không | Không cần tracer, không cần cap gì thêm; cùng ý "không ngủ trong kernel" | Là **một gate mới** với cách đo mới → plan riêng, không phải sửa A3b |

**Đề nghị: R1**, R2 là đường dự phòng ở bàn. Cụ thể:

- `check-no-kernel-sleep.sh` và `check-standard-gives-the-core-back.sh`: khi `W2W_EXTRA` có
  `--wire-timestamps`, script **tự chạy lại chính nó** trong `unshare -Urn` (bật `lo`, đánh dấu
  bằng một biến để không lồng vô hạn); nếu `unshare -Urn true` bị từ chối → in `SKIPPED, NOT
  PASSED`, **exit 2**, nêu đúng sysctl ở trên. Không có `W2W_EXTRA` → dòng lệnh **y nguyên** hôm
  nay (giữ lời hứa của A3a). Exit 2 là đỏ trên CI, nên "userns bị chặn" không bao giờ đọc thành
  xanh (`CLAUDE.md` §10).
- **Đảo chiều cho guard mới**: `aa-exec -p unconfined -- env W2W_EXTRA=… scripts/check-no-kernel-sleep.sh`
  trên máy này phải cho exit 2 và câu SKIP — chứ không phải 0, không phải 1.
- CI: một **step riêng** cho arm `W2W_EXTRA` ở cả hai job (không trộn vào step hiện có, để đỏ
  của arm mới không che arm cũ); nếu runner từ chối userns, step thêm dòng
  `echo 0 | sudo tee /proc/sys/kernel/apparmor_restrict_unprivileged_userns` trước — đó là cách
  Ubuntu ghi trong release notes 24.04, không phải sáng kiến của repo này.
- Runbook (`hft-playbook.md` mục 4): giữ câu "không chạy `w2w` dưới `sudo`"; ghi R2 là cách khi
  cần trace **trên NIC thật** (R1 chỉ có `lo` ảo — namespace không thấy `enp9s0`).
- R5 ghi vào `STATUS.md` *Open items* như một ý cho sau, không làm ở đây.

**Plan đổi gì.** A3b dòng 153–156: câu "chạy được ở mọi máy Linux" thay bằng "chạy trong user
namespace; cần userns không bị AppArmor chặn, nếu chặn thì SKIP exit 2". *Chia việc* A3b cột
*Test / gate*: thêm hai script chạy được **bên trong `unshare -Urn`** và đảo chiều `aa-exec`.
Cột *File*: thêm `scripts/check-standard-gives-the-core-back.sh` (đã đụng trong build, plan chỉ
ghi script kia) và `.github/workflows/ci.yml` (hai step). *Bẫy*: hàng mới — *cap từ file mất
khi bị trace bởi user thường* (test: gate chạy trong userns, không cần `setcap`) và *userns bị
AppArmor chặn tuỳ terminal* (test: đảo chiều `aa-exec`). *Rủi ro*: hàng mới cho runner CI.

### Điều 2 — dòng capability trong plan thiếu một nửa

**Plan viết** (A3b dòng 137): `sudo -n setcap cap_net_raw+ep target/release/w2w`.

**Sai ở đâu.** `SIOCSHWTSTAMP` (bật timestamp phần cứng trên card) cần **`CAP_NET_ADMIN`**, không
phải `CAP_NET_RAW`: `net/core/dev_ioctl.c` ~dòng 1025, trước khi rơi vào `dev_ifsioc`:
`if (!ns_capable(net->user_ns, CAP_NET_ADMIN)) return -EPERM;`. Đọc lại (`SIOCGHWTSTAMP`) thì
không cần. Tài liệu kernel nói thẳng: *"Only a processes with admin rights may change the
configuration"* và cấu hình là **của cả card**, không phải của socket — mọi tiến trình khác trên
`enp9s0` (ví dụ `ptp4l`, không có ở máy này) đều bị ảnh hưởng. `hwstamp_ctl` của linuxptp cũng
chỉ là cái ioctl này chạy bằng root.

**Dòng đúng**: `sudo -n setcap cap_net_raw,cap_net_admin+ep target/release/w2w` — code đã in
đúng dòng này khi bị từ chối. **Hậu quả phải nói rõ**: `cap_net_admin` trên một binary benchmark
là quyền **đổi cấu hình mạng của máy** — route, tường lửa, card — rộng hơn hẳn "nghe thô". Ba
điều làm nó chấp nhận được ở đây, không ở đâu khác: (1) chỉ `tmt` chạy được file đó, và `tmt` đã
có `sudo -n` không mật khẩu trên máy này (memory `fixbolt-sudo-helper`) — cap không cho thêm
quyền nào `tmt` chưa có; (2) cap gắn vào **file**, mỗi lần `cargo build` ghi đè là mất, phải đặt
lại có chủ ý; (3) `w2w` chỉ đụng `SIOCSHWTSTAMP`, in cấu hình **trước/sau**, và **trả lại** cấu
hình cũ khi thoát (`HwConfig::drop`) — kể cả khi run lỗi giữa chừng.

**Lựa chọn khác**: chỉ `cap_net_raw` trên `w2w`, còn bật/tắt timestamp bằng
`sudo -n hwstamp_ctl -i enp9s0 -t 1 -r 1` trước run và trả lại sau (hoặc một verb mới của
`/usr/local/sbin/fixbolt-machine`). Được: binary hẹp hơn. Mất: nếu run chết giữa chừng, card ở
trạng thái "bật" cho tới khi ai đó nhớ; hai lệnh thay vì một; `hwstamp_ctl` chưa chắc có (gói
`linuxptp`). **Đề nghị: giữ như đã build** — cap cả hai trên binary, dòng runbook sửa, hậu quả
ghi trong `hft-playbook.md` mục 4 (đã có) và ở *Rủi ro*. Nếu chủ sở hữu không muốn
`cap_net_admin` trên binary: đường `hwstamp_ctl`, và `HwConfig` chỉ **đọc** để in.

**Plan đổi gì.** A3b dòng 137–138: dòng `setcap` mới, một câu về `cap_net_admin`. *Rủi ro* hàng
"`AF_PACKET` + `setcap`": mức **Thấp → Trung**, cách xử lý ghi ba điều trên. Bất biến 8: ba
call FFI thành nhiều hơn ba (`socket`, `bind`, `setsockopt` ×n, `ioctl` ×3, `recvmsg` ×2,
`fcntl`, `getpeername`, `if_nametoindex`) — mỗi `unsafe` trong `mod wire` có comment SAFETY nêu
thứ chứng minh; cột *Giữ bằng cách nào* đổi "ba call" thành "mọi call trong `mod wire`".

### Điều 3 — bẫy `POLLERR` là thật ở `standard`, và không gate nào thấy

**Plan viết** (*Bất biến* hàng 4; *Bẫy* dòng 347): `check-standard-gives-the-core-back.sh` với
`W2W_EXTRA` canh được "`POLLERR` từ errqueue đánh thức `poll` của `standard` → engine spin";
"CPU > 5 % là đỏ".

**Sai ở đâu — hai lớp.**

1. *Cơ chế là chắc chắn, không phải nghi ngờ.* `poll(2)`: `POLLERR` "will be set in the revents
   field whenever the corresponding condition is true" — bất kể `events` xin gì. Với TCP, điều
   kiện đó là `net/ipv4/tcp.c` `tcp_poll` ~dòng 604: `if (READ_ONCE(sk->sk_err) ||
   !skb_queue_empty_lockless(&sk->sk_error_queue)) mask |= EPOLLERR;` — **error queue không rỗng
   là đủ**, và đó chính là nơi timestamp TX nằm chờ observer đọc. Khi kernel bỏ stamp vào,
   `sock_def_error_report` (`net/core/sock.c` ~4425) gọi `wake_up_interruptible_poll(…, EPOLLERR)`
   — đánh thức thẳng engine đang ngủ trong `poll`. Engine dậy, `recvfrom` → `EAGAIN` (không có
   dữ liệu), quay lại `poll` → trả về **ngay** vì hàng đợi vẫn chưa rỗng → lặp cho tới khi
   observer kịp `recvmsg(MSG_ERRQUEUE)`. Engine không tự thoát được: chỉ ai đọc errqueue mới xoá
   được điều kiện, và engine không đọc (đúng thiết kế — nó không biết gì về timestamp).
2. *Gate không nhìn thấy.* `check-standard-gives-the-core-back.sh` đo CPU trong **cửa sổ rảnh**
   (sau 300 message, `--hold-ms`); mỗi vòng lặp trên chỉ dài tới khi observer (đang spin) đọc
   xong — micro giây — rồi engine ngủ lại; tới cửa sổ rảnh thì không còn gì. Và trên `lo` **không
   có** stamp TX phần cứng nên errqueue **không bao giờ** có gì. Nên gate xanh trong mọi trường
   hợp, kể cả khi bẫy đang xảy ra trên NIC thật.

**Bằng chứng** (senior developer, 2026-09-14, bàn Linux, **thăm dò**, không phải số): build tạm
dùng stamp TX **phần mềm** làm vật thay thế trên `lo` (đã xoá sau khi đo), `--interval 2000`,
1 000 message, dưới strace: **111 trong 1 051 reply** đánh thức engine `standard` bằng `POLLERR`;
mỗi lần engine lặp `poll → recvfrom EAGAIN` **tới 67 lần liên tiếp** cho tới khi observer đọc.
Không strace, 1 000 msg/s, CPU engine 2,48 % không cờ so với 3,48 % có cờ (mỗi bên **một** run,
độ phân giải 0,5 % — hướng, không phải số). Ở `hft` thì tập syscall engine **không đổi** có/không
cờ (engine gọi `recvfrom` không chặn mỗi vòng, không `poll`, nên `POLLERR` không có ai để đánh
thức).

**Nghĩa là gì.** Một engine `standard` đo với `--wire-timestamps` trên NIC thật là một engine bị
công cụ đo làm cho **spin từng đợt** — vi phạm đúng nửa thứ hai của luật 4 ("`standard` mà spin
là lỗi"), dù lỗi thuộc về **cách đo**, không phải engine. Con số `wire` của arm đó là số của một
thứ không phải engine `standard`. `w2w` không sửa được điều này; nó chỉ bị chặn bởi observer đọc
nhanh tới đâu.

**Các lựa chọn** (mỗi cái đã tra):

| # | Cách | Được | Mất |
|---|---|---|---|
| S1 | **Arm `standard` của B6 không công bố số wire.** `w2w` **từ chối** `--mode standard --wire-timestamps` trên NIC không phải loopback (kiểm tra **trước** khi cần cap nào, nên thử được hôm nay trên `enp9s0` không cáp); arm `standard` NIC chạy **không** cờ, công bố **chỉ** bảng của Mac *"as the counterparty sees it"* | Không đụng `crates/engine`; không số nào sai lọt ra; `lo` gate vẫn chạy được ở `standard` (loopback không bị từ chối); một `if` | Item 40 hàng NIC-to-NIC của §6 thành **`hft` only**; `standard` trên NIC chỉ có RTT của máy kia (phần mềm, gồm cả kernel Mac) |
| S2 | Engine `standard` xử lý `POLLERR` khác đi — ví dụ khi `poll` trả `POLLERR` mà `recvfrom` là `EAGAIN` thì tự `recvmsg(MSG_ERRQUEUE)` bỏ đi, hoặc đưa cho một hook | Số `standard` wire đo được | **Sửa `crates/engine`** — plan này hứa không đụng; engine biết về timestamp, thêm một syscall trên đường nóng của `standard`; và thứ đo được là một engine **khác** engine đang có. Là plan + ADR riêng nếu muốn |
| S3 | Lấy stamp TX bằng **cơ chế khác**, không qua errqueue của socket engine: **BPF sock_ops timestamping** (Jason Xing, vào kernel 6.15; kernel 7.0 của máy này có). Chương trình BPF gắn vào cgroup bật `SK_BPF_CB_TX_TIMESTAMPING`; callback `BPF_SOCK_OPS_TSTAMP_SND_HW_CB` nhận stamp phần cứng; `SKBTX_HW_TSTAMP = SKBTX_HW_TSTAMP_NOBPF \| SKBTX_BPF` (`include/linux/skbuff.h` ~482) nên driver **vẫn** stamp cho yêu cầu chỉ-BPF. Mục tiêu công bố của loạt patch là "không phải sửa ứng dụng" | Ứng dụng (engine) **không** bị đụng socket; **không** có gì vào errqueue nếu socket không tự xin `SO_TIMESTAMPING` — *đọc từ cách chia cờ NOBPF/BPF, **chưa** xác minh trên `__skb_tstamp_tx`; phải đọc trước khi tin* | Cần chương trình BPF (viết + nạp: `aya` hoặc `libbpf` → dependency mới, ADR), `CAP_BPF` + `CAP_NET_ADMIN`, cgroup cho tiến trình engine, ringbuf ra observer; ghép theo byte vẫn qua `OPT_ID`-tương-đương của BPF (`sk_tskey_bpf_offset`). **Không phải một sửa của A3b** — là A3c hoặc plan riêng, ~2–3 ngày |
| S4 | Stamp ngoài host: tap thụ động + thiết bị ghi có timestamp (Arista MetaWatch, Cisco 3550-F/Exablaze, SolarCapture) — cách ngành HFT đo wire thật | Không đụng gì trong host | Phần cứng ngoài phạm vi plan (*Ngoài phạm vi* đã nói không switch); AF_PACKET tap trên chính host **không** có TX phần cứng (libpcap #894, đã ghi ở Sửa 1) |
| S5 | Chỉ đo `hft` với stamp wire, im lặng về `standard` | Đơn giản nhất | Không nói ra vì sao → người sau lại thử và đo sai; §6 hàng NIC không nói mode |

**Đề nghị: S1**, ghi S3 là hướng đúng cho sau (một item mới trong `STATUS.md`, kèm điều kiện
"đọc `__skb_tstamp_tx` trước"), S2 chỉ khi có ADR. Thêm vào B6 một **A/B miễn phí** để chặn câu
hỏi "cờ đo có làm `hft` chậm không": arm `hft` NIC chạy **có** và **không** `--wire-timestamps`,
so **bảng Mac** của hai arm — cùng nguồn, cùng đồng hồ; lệch trong band ADR-0031 thì stamping
không phải hạng mục. (Observer đọc errqueue trên `dup` của socket engine chạm cùng `struct sock`
— đó là thứ A/B này bắt.)

**Plan đổi gì.**

- *Bất biến* hàng 4, cột *Giữ bằng cách nào*, viết lại thành ba ý: (a) `hft`: tập syscall
  engine tid không đổi có/không cờ — `check-no-kernel-sleep.sh` trong userns; (b) `standard`
  trên `lo`: bốn assertion của `check-standard-gives-the-core-back.sh` xanh — **và header script
  nói rõ nó không thấy `POLLERR`** (đã viết trong build); (c) `standard` trên NIC thật: **không
  đo với cờ** — `w2w` từ chối, đảo chiều là chạy `--mode standard --wire-timestamps --nic enp9s0
  --observer-core 2 --listen 127.0.0.1:0` bằng binary **không** cap → exit ≠ 0, câu từ chối nêu
  `POLLERR`.
- *Bẫy* dòng 347: cột *Test canh* đổi từ "`check-standard…` với `W2W_EXTRA`; CPU > 5 % là đỏ"
  (sai — không thấy được) thành "`w2w` từ chối `standard` trên NIC thật; header
  `check-standard…` ghi giới hạn; số thăm dò 111/1 051 ghi ở `docs/reference/`".
- B6: "`hft`/`standard` × admin/app × interval" → **wire figure chỉ `hft`**; `standard` × admin/app
  chạy không cờ, chỉ bảng Mac; thêm arm A/B có/không cờ ở `hft`; cột *Cho item / hàng*: §6 hàng
  NIC ghi **mode `hft`** ngay trong hàng. *Chia việc* B6 cột *Test / gate*: thêm "arm `standard`
  không có cột wire, có ghi lý do".
- *Tài liệu phải cập nhật*: `docs/reference/` file mới (đề nghị tên
  `a-transmit-timestamp-wakes-a-blocking-engine.md`): cơ chế, ba dòng kernel dẫn ở trên, số thăm
  dò, và S3; `docs/GUIDE.md` §8 một câu: *đặt `SO_TIMESTAMPING` TX lên socket của engine
  `standard` mà không có ai đọc errqueue là biến nó thành engine spin từng đợt*; `DESIGN.md` §6
  hàng NIC ghi mode; `STATUS.md` item mới cho S3.

### Điều 4 — năm chỗ code rời văn bản plan

| | Plan viết | Đã build | Kết luận |
|---|---|---|---|
| **(a)** RX | tap `AF_PACKET` với **`PACKET_TIMESTAMP`** (ring) | tap `AF_PACKET` **`SOCK_DGRAM` + `recvmsg` + `SO_TIMESTAMPING`** (cmsg mang bộ ba `ts[0..3]`), lọc BPF theo port | **Xác nhận.** Ring `tpacket_rcv` (`af_packet.c` ~3438) khi không có stamp phần cứng/phần mềm thì **điền giờ hệ thống** (`ktime_get_real_ts64`) và **không** đặt cờ `TP_STATUS_TS_*` nào — đọc ring mà quên xem cờ là nhận nhầm giờ phần mềm thành phần cứng. Với cmsg, `ts[2] == 0` là "không có", đúng cái `hw-rx-missing` cần. Giá: một `recvmsg` mỗi frame trên observer — observer đang spin, không đáng kể. Ring vẫn dùng được **nếu** đọc cờ; ghi lại để ai sau này không tưởng ring bị cấm |
| **(b)** ai đặt `SO_TIMESTAMPING` | "trong w2w, trước `engine.add`" — không nói thread nào | observer đặt trên **`dup`** của socket đã accept; engine thread đưa fd qua atomic rồi **spin không syscall** tới khi observer trả lời (timeout 2 s) — **cả ở `standard`**, lúc accept | **Xác nhận, có ghi chú.** Bắt buộc phải đặt trên socket **đã accept**, không đặt sẵn trên socket listen: `net/core/sock.c` `sock_set_timestamping` ~1040 — `OPT_ID` trên TCP đang `CLOSE`/`LISTEN` trả `-EINVAL`, và `OPT_ID_TCP` lấy `write_seq` **của socket đó** làm mốc. Spin ở đây nằm trên **đường accept**, một lần cho connection đầu, bị chặn 2 s, chỉ khi có cờ — không phải "rảnh" theo nghĩa luật 4; `check-standard…` đo cửa sổ rảnh nên không bị ảnh hưởng, **header script phải ghi** điều đó. Cách thay thế đơn giản hơn (engine thread tự `setsockopt` + `fcntl(F_DUPFD_CLOEXEC)` — `setsockopt` đã có trong tập syscall của engine, `fcntl` là thêm một tên **không** thuộc danh sách ngủ) ghi lại làm dự phòng nếu handoff có ngày trục trặc; không làm lại bây giờ |
| **(c)** ghép request–reply | "theo thứ tự, một request trong chuyến" | theo **byte của luồng TCP**: key `OPT_ID_TCP` của reply (`ee_data` = offset byte cuối của một `send`, `tcp.c` ~796: `tskey = seq + len - 1`) so với `ack` của request **kế tiếp**; mất một stamp → **một** request thiếu, không dịch cả dãy | **Xác nhận** — tốt hơn plan. Và bẫy `igb` "một stamp TX đang chờ" ở *Bẫy* dòng 348 giờ **đã xác minh**: `igb_xmit_frame_ring` — `if (adapter->tstamp_config.tx_type == HWTSTAMP_TX_ON && !test_and_set_bit_lock(__IGB_PTP_TX_IN_PROGRESS, &adapter->state)) { … } else { adapter->tx_hwtstamp_skipped++; }`; loạt patch intel-wired-lan 2026-08 mô tả đúng: *"keeps a single outstanding Tx hardware timestamp request in adapter->ptp_tx_skb"*. Stamp bị bỏ **không** vào errqueue (w2w chỉ xin phần cứng, không xin phần mềm). **Bộ đếm đọc được**: `ethtool -S enp9s0 \| grep tx_hwtstamp_skipped` — máy này có, đang `0`. B6 đọc **trước/sau mỗi run** và ghi cạnh `hw-tx-missing`; hai số phải khớp nhau hoặc được giải thích |
| **(d)** cửa sổ của `--listen` | không nói | `--listen` tính **mọi** request sau logon, **kể cả warmup** của generator; run TLS không có cửa sổ | **Bác phần warmup.** Với 20 000 request, p99.9 là **mẫu chậm thứ 20 từ trên xuống**; 50 request warmup (cache lạnh) sẽ **là** cả cái đuôi đó — p99.9 của `--listen` thành p99.9 của warmup. Sửa: `--listen` nhận **`--warmup <n>`** *chỉ khi* có `--wire-timestamps`, nghĩa là "bỏ n request đầu sau logon khỏi cửa sổ wire"; `w2w-baseline.sh` (A6) truyền cùng số với generator; nhãn in ra nói rõ "sau logon, bỏ n đầu". Không có cờ thì `--listen` vẫn từ chối `--warmup` như A3a. **Chấp nhận** phần TLS: `--listen` đã từ chối `--tls` (A3a), còn run gộp TLS đi loopback nên không có stamp — vô hại |
| **(e)** run gộp trên `--nic enp9s0` | không nói | tap không thấy bắt tay TCP (run gộp đi loopback) → **lỗi to**, câu lỗi bảo dùng `--listen` | **Xác nhận.** Đúng ý "thà đỏ còn hơn đếm nhầm". Thêm một hàng *Bẫy*; test là chính câu lỗi đó (cần cap, chạy ở bàn) |

**Plan đổi gì.** A3b dòng 134–142 viết lại theo (a)–(c) đã build; *Chia việc* A3b cột *Test /
gate* thêm `--listen --warmup N --wire-timestamps` được nhận, `--listen --warmup N` không cờ bị
từ chối; *Bẫy* dòng 348 bỏ chữ "chưa xác minh", thêm bộ đếm `ethtool -S`; *Bẫy* hàng mới cho
(d) và (e); B6 thêm "đọc `tx_hwtstamp_skipped` trước/sau".

### A3b sau Sửa 2 — văn bản thay cho dòng 130–156

**A3b — `--wire-timestamps --nic <ifname> --observer-core <cpu>` (Linux, nửa engine).** Một
đồng hồ: cả RX của request lẫn TX của reply lấy ở I211 của máy này, PHC `ptp0`, không PTP sync.

- **RX**: thread observer (pin `--observer-core`, không phải 6/7, spin) mở tap `AF_PACKET`
  `SOCK_DGRAM` trên `--nic`, `SO_TIMESTAMPING` RX phần cứng, lọc BPF theo port; mỗi frame đọc bằng
  `recvmsg`, bộ ba stamp trong cmsg. Card được `SIOCSHWTSTAMP` (`tx_type ON`, `rx_filter ALL`) cho
  run và **trả lại** khi thoát; in cấu hình trước/sau nhưng **không tin readback** (patch `igb`
  2026-09-10). Loopback: không đụng card, mọi stamp đếm là thiếu, không in cột wire.
- **TX**: observer đặt `SO_TIMESTAMPING` (TX phần cứng, `OPT_ID | OPT_ID_TCP | OPT_TSONLY`) lên
  **`dup` của socket đã accept**, trước `engine.add`; engine thread đưa fd qua atomic và spin
  (không syscall, ≤ 2 s) chờ trả lời — một lần, lúc accept. Observer đọc errqueue trên `dup`.
- **Ghép** theo byte của luồng TCP (`pair.rs`): mất một stamp = thiếu **một** request. `ts[2] == 0`
  đếm vào `hw-rx-missing`/`hw-tx-missing`, không bao giờ thay bằng phần mềm.
- **Cửa sổ**: run gộp = `n` request sau warmup; `--listen` = mọi request sau logon **trừ
  `--warmup n` đầu** (cờ này chỉ hợp lệ cùng `--wire-timestamps`).
- **Mode**: `hft` là mode có số wire. `--mode standard` + NIC thật → **từ chối** (điều 3);
  `standard` + loopback chạy, để gate chạy được.
- **Quyền**: `sudo -n setcap cap_net_raw,cap_net_admin+ep target/release/w2w` sau mỗi build, ở
  bàn, cho NIC thật. Gate trên `lo` chạy trong `unshare -Urn`, không cần cap, không sudo.
- **In**: `wire p50/p99/p99.9`, `requests`, `hw-rx-missing`, `hw-tx-missing`, `tx stamps seen`,
  `tap drops`, `allocs` engine + observer; Mac in bảng riêng *"as the counterparty sees it"*.
- Phụ thuộc `libc` theo `cfg(target_os = "linux")`; mọi `unsafe` trong `mod wire` có comment nêu
  thứ chứng minh (test `pair.rs`, hai script trong userns, câu từ chối).

**Cột *Test / gate* của A3b (thay dòng 255):** tests `pair::pairs_in_order_one_in_flight`,
`pair::a_missing_hw_stamp_is_counted_not_interpolated`,
`pair::never_mixes_software_into_the_hardware_column`; `W2W_EXTRA="--wire-timestamps --nic lo
--observer-core 2" scripts/check-no-kernel-sleep.sh` — tự chạy trong `unshare -Urn`, engine tid
chỉ `recvfrom sendto` (+ `accept4`), nửa đỏ vẫn đỏ; cùng biến với
`check-standard-gives-the-core-back.sh` bốn assertion xanh; **đảo chiều**: `aa-exec -p unconfined`
→ exit 2 + câu SKIP; trên `lo` in `hw-rx-missing = hw-tx-missing = tổng`, không cột wire; binary
**không** cap + `--mode standard --wire-timestamps --nic enp9s0 --observer-core 2 --listen
127.0.0.1:0` → exit ≠ 0 nêu `POLLERR`; `--listen --warmup 50 --wire-timestamps …` nhận, không cờ
thì từ chối; `shellcheck -S info` sạch hai script. Tier **opus**, máy Linux-desktop — không đổi.

### Câu hỏi cho chủ sở hữu — mỗi câu một quyết định, đề nghị đứng trước

| # | Câu hỏi | Đề nghị | Nếu "không" |
|---|---|---|---|
| **Q8** | Gate `W2W_EXTRA` chạy bằng cách nào? | **R1 — user namespace**, script tự `unshare -Urn`, SKIP exit 2 khi bị chặn; CI có step riêng; R2 (`sudo -n strace -u`) chỉ trong runbook cho NIC thật | R2 thành gate: arm này **chỉ ở bàn**, ghi vào `CLAUDE.md` §2 bảng *Machine checks* hàng 4 cột *Note* như "`hft` under TLS is unchecked" đang ghi |
| **Q9** | `cap_net_admin` trên `target/release/w2w`? | **Có** — cùng `cap_net_raw`, đặt lại sau mỗi build, hậu quả ghi ở runbook và *Rủi ro* | `w2w` chỉ `cap_net_raw`; bật/tắt bằng `sudo -n hwstamp_ctl` (cần gói `linuxptp`) hoặc verb mới của `fixbolt-machine`; `HwConfig` chỉ đọc |
| **Q10** | Bỏ số wire của arm `standard` khỏi B6, và `w2w` **từ chối** `standard` + NIC thật? | **Có cả hai** — §6 hàng NIC ghi mode `hft`; `standard` NIC chỉ có bảng Mac; S3 (BPF) ghi thành item mở cho sau | Giữ arm: cần S2 (sửa engine, plan + ADR riêng) **trước** boot B, hoặc công bố số của một engine spin — architect không ký cái thứ hai |
| **Q11** | `--listen` nhận `--warmup <n>` khi có `--wire-timestamps`? | **Có** — nếu không, p99.9 của B6 là p99.9 của warmup | Giữ như đã build, và mọi số `--listen` chỉ công bố p50/p99, **không** p99.9 |

Không câu nào ở trên chặn A1, A2, A4–A7. A3b **chưa commit** cho tới khi có trả lời; developer
sửa theo câu trả lời rồi manager chạy lại toàn bộ cột gate ở trên trên commit đóng bước.

**`[2026-09-14]` Chủ sở hữu duyệt Sửa 2 theo đề xuất**, nguyên văn *"Duyệt theo đề xuất"*: Q8 = R1 (user namespace, SKIP exit 2 khi bị chặn), Q9 = có `cap_net_admin`, Q10 = bỏ số wire của arm `standard` và `w2w` từ chối `standard` + NIC thật, Q11 = `--listen` nhận `--warmup`. A3b làm lại theo mục *A3b sau Sửa 2*, chồng lên A4.

### Nguồn (tra 2026-09-14)

- Kernel `security/commoncap.c` v6.16, `cap_bprm_creds_from_file` —
  <https://elixir.bootlin.com/linux/v6.16/source/security/commoncap.c> (đọc qua
  raw.githubusercontent.com cùng tag); `LSM_UNSAFE_PTRACE` / `ptracer_capable`.
- Kernel `net/core/dev_ioctl.c` v6.16, `dev_ioctl` — `CAP_NET_ADMIN` cho `SIOCSHWTSTAMP`:
  <https://elixir.bootlin.com/linux/v6.16/source/net/core/dev_ioctl.c>.
- `poll(2)` — `POLLERR` luôn báo: <https://man7.org/linux/man-pages/man2/poll.2.html>.
- Kernel `net/ipv4/tcp.c` v6.16, `tcp_poll` (`EPOLLERR` khi errqueue không rỗng) và
  `tcp_tx_timestamp` (`tskey = seq + len - 1`): <https://elixir.bootlin.com/linux/v6.16/source/net/ipv4/tcp.c>.
- Kernel `net/core/sock.c` v6.16, `sock_set_timestamping` (`OPT_ID` từ chối `CLOSE`/`LISTEN`;
  `OPT_ID_TCP` lấy `write_seq`) và `sock_def_error_report`:
  <https://elixir.bootlin.com/linux/v6.16/source/net/core/sock.c>.
- Kernel `net/packet/af_packet.c` v6.16, `tpacket_rcv` fallback `ktime_get_real_ts64`, và
  `packet_create` `ns_capable(net->user_ns, CAP_NET_RAW)`:
  <https://elixir.bootlin.com/linux/v6.16/source/net/packet/af_packet.c>.
- Kernel `include/linux/skbuff.h` v6.16, `SKBTX_HW_TSTAMP = SKBTX_HW_TSTAMP_NOBPF | SKBTX_BPF`:
  <https://elixir.bootlin.com/linux/v6.16/source/include/linux/skbuff.h>.
- Tài liệu kernel *Timestamping* — errqueue, `OPT_ID_TCP`, `OPT_TSONLY`, "only a process with
  admin rights may change the configuration": <https://docs.kernel.org/networking/timestamping.html>.
- BPF sock_ops timestamping: LWN cover letter <https://lwn.net/Articles/996139/>; v6 trên
  netdev <https://lists.openwall.net/netdev/2025/01/21/18>; kfunc
  `bpf_sock_ops_enable_tx_tstamp` <https://github.com/torvalds/linux/commit/59422464266f8baa091edcb3779f0955a21abf00>;
  eBPF docs <https://docs.ebpf.io/linux/program-type/BPF_PROG_TYPE_SOCK_OPS/>. *Không tìm thấy
  câu tài liệu nào nói thẳng "không vào errqueue"* — suy từ cách chia cờ, ghi là chưa xác minh.
- `igb` một stamp TX đang chờ: loạt patch intel-wired-lan 2026-08 *igb: PTP Tx timestamp state
  fixes* (trích `igb_xmit_frame_ring` và `tx_hwtstamp_skipped`)
  <https://ratatoskr.run/intel-wired-lan/2026/08/17415163/t>; bộ đếm ethtool trong
  `igb_ethtool.c` dòng ~41–43: <https://elixir.bootlin.com/linux/v6.16/source/drivers/net/ethernet/intel/igb/igb_ethtool.c>.
- `strace(1)` `-u`: <https://man7.org/linux/man-pages/man1/strace.1.html>.
- `hwstamp_ctl(8)`: <https://linuxptp.nwtime.org/documentation/hwstamp_ctl/>.
- Ubuntu chặn user namespace không đặc quyền: release notes 24.04
  <https://documentation.ubuntu.com/release-notes/24.04/>; Qualys 2025, ba cách vượt rào
  <https://www.qualys.com/2025/three-bypasses-of-Ubuntu-unprivileged-user-namespace-restrictions.txt>.
- Cách người khác lấy stamp TX: Onload cũng qua `recvmsg(MSG_ERRQUEUE)` trên socket gửi
  (`ONLOAD_SOF_TIMESTAMPING_STREAM`) <https://github.com/majek/openonload/blob/master/src/tests/onload/hwtimestamping/tx_timestamping.c>;
  đo wire ngoài host: Arista MetaWatch <https://www.arista.com/en/products/7130-meta-watch>,
  FMADIO về trailer timestamp của packet broker
  <https://www.fmad.io/blog/packet-broker-hardware-timestamps-getting-the-most-from-your-networks-timing-data>;
  `rxtxcpu` (stackpath) chỉ là capture theo CPU, không có TX phần cứng —
  <https://github.com/stackpath/rxtxcpu>. *Không tìm thấy công cụ nào lấy stamp TX phần cứng trong
  host mà không qua errqueue của socket gửi, ngoài đường BPF ở trên.*
- Tap AF_PACKET không có TX phần cứng: libpcap #894 (đã dẫn ở Sửa 1).
- Thăm dò của architect hôm nay (bàn Linux, `fixbolt-machine off`, binary a3b copy không cap):
  `unshare -Urn` chạy hết cả `hft` và `standard` dưới strace; `aa-exec -p unconfined -- unshare
  -Urn true` → EPERM; `sudo -n strace -f -u tmt` → rc=0; `sysctl
  kernel.apparmor_restrict_unprivileged_userns = 1`; `/proc/self/attr/current` của shell này là
  `vscode (unconfined)`.

## Sửa 3 — 2026-09-14, boot B: bốn việc chặn B2, B5, B6

Boot B bắt đầu 21:27, B0 xong và đã commit (`9bee2f2`). Nhật ký *Bắt đầu boot B* gửi architect
bốn việc: B5 chờ item 88; B2 (và mọi hàng §8 của boot này) công bố thế nào khi item 85 chưa có
plan; EEE và pause frames trên dây B6; bẫy `igb` ở B0 cần một test. Phần này trả lời từng việc,
một *Điều* mỗi việc, cùng khuôn với Sửa 2: plan viết gì, thấy gì, bằng chứng, lựa chọn, **đề
nghị**, plan đổi gì. Điều 5 gom những gì Mac mini và `warp-svc` đổi ở B6/B7/B8 và thứ tự B2–B9.

Architect **chỉ đọc**: không `cargo`, không benchmark, không đổi gì trên máy. Mọi lệnh trên desk
chạy 21:36–21:45 (đọc file, `ethtool` đọc, `nft list`, `journalctl`); trên Mac qua ssh 21:38–21:44
(`ifconfig`, `networksetup -getMedia`, `system_profiler`, `git`, `shasum`). Chủ sở hữu quyết ở
bảng Q12–Q17. **B1 không phụ thuộc phần này** và chạy song song.

Vài chữ dùng suốt phần này, nói bằng lời thường một lần:

- **EEE / LPI** — *Energy-Efficient Ethernet* (chuẩn 802.3az). Hai đầu dây thoả thuận lúc bắt tay;
  đầu phát không có gì để gửi thì "ngủ" (*Low Power Idle*), muốn gửi lại phải "đánh thức" đầu thu
  bên kia trước, mất khoảng **16,5 µs** ở 1000BASE-T. Chỉ dùng khi **cả hai** đầu quảng bá hỗ trợ;
  một đầu không quảng bá thì cả dây không ngủ.
- **pause frame** — khung "xin dừng" mà đầu thu gửi khi bộ đệm sắp đầy. Chỉ xuất hiện khi nghẽn.
- **procedure** — một lần chạy trọn `scripts/w2w-baseline.sh` cho một arm (20 run). Số §8 hôm nay
  là median của 20 run trong **một** procedure.
- **carrier** — tín hiệu "dây đang nối, link đang lên" của card mạng.

### Điều 1 — item 88: B5 đang đổi hai thứ cùng lúc

**Plan viết** (B5, *Chia việc* B2–B5): so `--journal file-async` và `--log file` với "không", hai
mode, path app. Nhật ký *Ghi chú cho B5 — F13* nói vòng của hai arm khác cỡ; item 88 nói *giải
quyết trước B5*.

**Thấy gì, đọc từ code.**

- `tools/w2w/src/main.rs:1998`, `2001`, `2039`: `--journal file-async` dựng `FileJournal<64, 512>`.
  Mặc định (`--journal mem`) là `Store = MemJournal<4096, 512>` (`crates/engine/src/journal.rs:51`,
  `257`). Vòng 64 ô là 32 KiB — vừa L1; vòng 4 096 ô là 2 MiB — lớn hơn L2 của Zen 2 (512 KiB), nên
  mỗi `put` ghi vào một dòng cache không có sẵn. Đó chính là biến thứ hai: vài chục ns mỗi message,
  nhỏ cạnh 17 µs, nhưng không phải 0 và không tách được khỏi "giá của journal ra file".
- Lý do duy nhất để chọn 64 (sợ 2 MiB trên stack) **không còn**: `MemJournal` giữ vòng trong
  `Box<[Slot]>` (`journal.rs:81`, comment dòng 72–80 nói rõ vì sao), và `FileJournal<N, LEN>` chỉ
  chứa `mem: MemJournal<N, LEN>` (dòng 281) — kích thước trên stack không đổi theo `N`.
- **Pre-fault.** `MemJournal::new` (`journal.rs:128–141`) ghi từng ô bằng `resize_with`, nên mọi
  trang của vòng được chạm lúc mở, **ngoài** cửa sổ đo; `FileJournal::open_with` gọi đúng hàm này
  (dòng 512 và 655 — hai lần, một bản tạm bị vứt ở dòng 668) cộng vòng ghi cho thread writer
  `ring::pair(1 << 20)` = 1 MiB (`ring.rs:47`, `124`). Ở 4 096 ô, `--journal file-async` tốn 2 MiB
  vòng + 1 MiB ring + 2 MiB tạm lúc mở, cho **một** session. Không đáng kể, và cả hai arm cùng
  cách pre-fault.
- Arm "không journal" của B5 **chính là** arm B2 (A4 đã chứng minh: không cờ thì `type_name` engine
  y hệt bản trước). Nên B5 không cần chạy lại arm "không" — nếu cùng procedure với B2.
- `crates/engine/benches/alloc.rs:1438`: case `journal-async-busy` (gate luật 1 của A4) cũng mở
  `FileJournal<64, 512>`; comment dòng 1431 nói nó cùng hình với case `mark_file`.

**Các lựa chọn.**

| # | Cách | Được | Mất |
|---|---|---|---|
| J1 | **`file-async` của w2w dùng `FileJournal<SLOTS, SLOT_LEN>`** — hai hằng public của `journal.rs`, đúng cỡ `Store` | B5 đổi **một** biến; arm "không" = B2, đỡ hai arm; +3 MiB | sửa ba dòng w2w + một dòng `alloc.rs`; phải build lại w2w (cargo — chỉ trong *khe build*, Điều 5) |
| J2 | Thêm arm `--journal mem-64` để tách riêng kích thước vòng | có số "vòng nhỏ đáng bao nhiêu" | thêm kiểu engine thứ năm trong w2w, thêm hai arm × hai procedure; số đó không ai xin |
| J3 | Giữ 64, ghi chú cạnh số | không code | B5 công bố một hiệu số không gán được cho cái nào — đúng cái F13 và `CLAUDE.md` §10 cấm |

**Đề nghị: J1.** "Vòng nhỏ đáng bao nhiêu" là câu hỏi khác, chưa ai hỏi; khi hỏi thì là J2.

**Spec S1 — developer (sonnet), review gộp vào B10 vì chạm gate luật 1:**

```text
Role: developer (sonnet). Sửa 3 Điều 1 (S1) của docs/plans/2026-09-04-the-second-linux-desk.md.
Why: B5 phải đổi đúng một biến; vòng 64 ô của --journal file-async là biến thứ hai.
Read first, exactly here: crates/engine/src/journal.rs:34-56 (SLOTS, SLOT_LEN) và 72-81;
  tools/w2w/src/main.rs:1996-2007 và 2038-2041; crates/engine/benches/alloc.rs:1423-1445.
Touch: tools/w2w/src/main.rs — một alias
  `type FileStore = fixbolt_engine::journal::FileJournal<{ fixbolt_engine::journal::SLOTS }, { fixbolt_engine::journal::SLOT_LEN }>;`
  thay cho ba chỗ `FileJournal<64, 512>`; module doc (~dòng 169) và doc của `JournalKind::FileAsync`
  (~dòng 408) nói "cùng số ô với `Store`, để B5 đổi một biến";
  crates/engine/benches/alloc.rs — case `journal-async-busy` dùng cùng hai hằng, comment dòng 1431
  sửa theo; case `mark_file` (dòng 1397) giữ nguyên.
Do not touch: crates/engine/src/, scripts/, bất kỳ file nào khác.
§2 items: 1 (`journal-async-busy` đọc 0; đảo chiều của A4: một `to_vec()` trong đường Async push
  → đọc ≥ 1, rồi trả lại), 7.
Done when: `cargo build --release -p fixbolt-w2w --features affinity` xanh;
  `target/release/w2w --mode hft --path app --journal file-async --messages 2000 --warmup 200`
  in `journal: file-async`, `allocs 0`, exit 0 — và cùng lệnh với `--mode standard`;
  `scripts/bench.sh` in `journal-async-busy 0`; đảo chiều đọc ≥ 1; không cờ thì banner/type_name
  engine y hệt bản trước (lặp lại cách A4 đã làm); `cargo fmt`, clippy -D warnings sạch.
Machine: KHÔNG chạy cargo trên desk khi có phép đo đang chạy — build và gate chỉ trong "khe
  build" (Điều 5), hoặc trên máy Linux khác.
Report: diff từng file; output nguyên văn của mọi lệnh ở Done; không commit.
```

**Plan đổi gì.** Hàng B5 của bảng *Boot B* thay bằng:

| Bước | Đo gì | Cho item / hàng |
|---|---|---|
| **B5** | `W2W_EXTRA="--journal file-async"` rồi `W2W_EXTRA="--log file"` (Điều 2, S2), `ARMS="hft:app standard:app"`; arm "không" là arm app của B2 **cùng procedure**; **cùng vòng 4 096 ô (S1)** nên hiệu số là của loại journal, của log — không phải của kích thước vòng; hai procedure như Điều 2 | §8 hàng journal/log; **item 88 đóng** |

*Chia việc* thêm hàng S1 (bảng ở Điều 5).

### Điều 2 — item 85: boot B công bố thế nào

**Plan viết** (*Chia việc* B2–B5, B6, B8): "`w2w-baseline.sh` mỗi arm 20 run, `check-machine.sh`
quiet mỗi run" — tức **một** procedure, median của nó vào §8. Nhật ký B3 nói B2 không được làm
thế "như item 85 không tồn tại", nhưng không nói làm gì thay.

**Thấy gì.**

- Bẫy [a-tight-spread-inside-one-procedure-did-not-reproduce-across-two](../reference/a-tight-spread-inside-one-procedure-did-not-reproduce-across-two.md)
  đã có mục *The rule* (năm gạch đầu dòng: tái lập bằng hai procedure, công bố cả hai, dispersion
  hai phía theo từng percentile, ghi mọi thứ xảy ra giữa hai lần, procedure tự ghi commit/cây/
  uptime). Chưa có gì thực thi nó: `scripts/w2w-baseline.sh:511–521` chỉ in `spread max/median`
  cho p50; không in HEAD, cây, uptime; không giữ output từng run trên đĩa (in ra stdout rồi
  thôi — ai `tee` thì có). `DESIGN.md` §8 dòng 1208 đã viết *"one twenty-run median is one
  observation"* nhưng phần thủ tục của §8 vẫn công bố từ một procedure.
- `w2w` **không in commit của mình** và không có `--version`/`--help` — chạy trần là bắt đầu đo
  (thử hôm nay trên Mac, `w2w --help` in banner một run). Chỉ script mới ghi được HEAD.
- Script **không có chỗ cho `--journal`/`--log`** (`ARMS` là `mode:path:tls:interval`) — B5 không
  chạy được bằng script như hàng viết. S2 thêm `W2W_EXTRA` (cùng tên với hai script luật 4).
- Giá một procedure loopback: mỗi run ~1 s quiet-check + ~1–2 s đo + `GAP` 8 s ≈ 11 s → **~3,7
  phút một arm**. Hai procedure là +3,7 phút mỗi arm công bố. Rẻ, trừ các arm `--interval` dài
  (Điều 5, mục d).

**Các lựa chọn.**

| # | Cách | Được | Mất |
|---|---|---|---|
| P1 | Item 85 có plan riêng **trước**; boot B chỉ thu run thô, không công bố gì | đúng thủ tục | mất một ngày §9 mà §8 vẫn trống; plan riêng rồi cũng đề xuất đúng P2 |
| P2 | **Quy tắc công bố hai procedure** ([ADR-0068](../decisions/ADR-0068-a-published-figure-is-two-procedures-shown-side-by-side.md), Proposed) **và sửa script trước B2** (S2) | mọi số boot B có bằng chứng tái lập ở mỗi percentile; bẫy có test; §4 hết nợ | desk time ×2 cho arm công bố (~+2,5 giờ); S2 là nửa ngày sonnet **trước** B2 |
| P3 | Chạy hai lần bằng script cũ, manager tính dispersion bằng tay từ output | không code | không ai giữ output từng run; không test → §4 vẫn nợ; lần sau lại quên; B5 vẫn không chạy được bằng script |

**Đề nghị: P2.** Nội dung quy tắc — chi tiết và hậu quả ở ADR-0068 — tóm lại:

1. **Một arm công bố = hai procedure y hệt**, cùng commit, cùng binary (sha256), cây sạch, cùng
   boot, cách nhau ít nhất một lượt qua các bước khác (≥ 30 phút). **Cả hai median vào §8, cạnh
   nhau, không lấy trung bình**, kèm hiệu %. Không có "một số duy nhất"; cặp số là số.
2. **Tái lập** = ở **mỗi** percentile công bố (p50, p99, p99.9), hai median lệch ≤ **5 %** của
   số nhỏ hơn. Con số là tạm và là *chọn*, không phải *đo*: các arm lành ngày 2026-09-14 lệch
   0,2–2,5 % p50 và ≤ 3,0 % p99.9; ba arm hỏng lệch 7,2 / 15,9 / 18,5 %. Item 85 có band đo được
   thì ADR mới thay.
3. **Không tái lập** → vẫn ghi cả hai, đánh dấu *không tái lập*; không gate hay hàng §6 nào đọc
   một trong hai là *met*; mọi thứ xảy ra giữa hai procedure ghi là ứng viên, không phải nguyên
   nhân. B6 với arm không tái lập thì item 40 là *measured, not reproduced* — đó là kết quả.
4. **Arm A/B là một procedure, `RUNS=10`, nhãn A/B, chỉ là hiệu số** — `busy_read` 0 ↔ 50, IRQ pin
   vào cpu6, `--wire-timestamps` có/không, EEE bật (Điều 3), boot C. Không bao giờ là số §8. Đây
   là câu của ADR-0023 áp dụng cho mọi A/B.
5. **Script tự ghi**: HEAD, `git status --porcelain`, uptime, sha256 + mtime của binary (và của
   binary generator qua ssh), thư mục giữ output từng run; dispersion hai phía theo từng
   percentile.

**Thời gian desk, ước lượng** (loopback ~3,7 phút/arm/procedure; qua cáp + ssh ~4 phút): B2 4 arm
×2 ≈ 30 phút; B5 4 arm ×2 ≈ 30; B8 4 arm ×2 ≈ 30; B4 ba arm interval (cỡ message ở Điều 5 mục d)
×2 ≈ 105 (+44 nếu thêm `standard` ở 1 s); B6 công bố (hft admin/app × interval 0 và 1 s, bảng Mac
`standard` admin/app) ×2 ≈ 2 giờ, A/B một lần ≈ 20 phút; B7 ≈ 20; B9 ≈ 15; B1 tuỳ. **Tổng ~7–8
giờ** — một ngày, sát. Thứ tự cắt nếu quá ngày (hàng *Rủi ro* "cắt từ B9 ngược lên" giữ, thêm):
arm `interval 10 000` của B4 → arm app ở 1 s của B6 → arm app của B8 → `standard` 1 s.

**`scripts/w2w-baseline.sh` phải đổi trước B2 — có.** Spec S2:

```text
Role: developer (sonnet). Sửa 3 Điều 2 (S2).
Why: bẫy item 85 — số công bố phải mang HEAD, cây, uptime, binary; dispersion hai phía theo
  percentile; output từng run giữ trên đĩa; và B5 cần truyền cờ w2w.
Read first, exactly here: docs/reference/a-tight-spread-inside-one-procedure-did-not-reproduce-across-two.md
  mục *The rule* và *What guards it*; ADR-0068 mục Decision 5; scripts/w2w-baseline.sh:87-135
  (biến), 176-192 (khối máy), 209-224 (helper), 259-273 (header), 316-321 và 447-455 (lệnh w2w),
  470-475 (kiểm tra identity tls — mẫu), 500-552 (summary); scripts/check-machine-verdicts.sh:16-31
  và 67-73 (mẫu source-only + `same`); scripts/check-machine.sh:174 (guard MACHINE_SOURCE_ONLY).
Touch: scripts/w2w-baseline.sh; scripts/check-w2w-baseline-summary.sh (mới); .github/workflows/ci.yml
  (job `script-logic`: một dòng `- run: scripts/check-w2w-baseline-summary.sh` sau
  check-machine-verdicts.sh); docs/hft-playbook.md §6 mục 3 (một câu: W2W_EXTRA, OUT_DIR, header).
Do not touch: tools/, crates/, check-machine.sh, check-machine-verdicts.sh, bench.sh.
Đổi gì:
 (a) Header, sau dòng `runs …`: `commit <rev-parse --short HEAD>   tree <clean | N paths: …>`;
     `uptime <giờ:phút từ /proc/uptime>`; `binary <sha256 12 ký tự đầu> <mtime ISO>`; với
     GENERATOR_SSH: `generator binary <sha256 qua ssh: shasum -a 256 … || sha256sum …>` (best
     effort, in `unknown` khi lỗi — không dừng); `output <OUT_DIR>`.
 (b) OUT_DIR=${OUT_DIR:-target/w2w-baseline/<UTC yyyymmddThhmmssZ>-<HEAD>}: mỗi run ghi
     `$OUT_DIR/<mode>-<path>-<tls>-<interval>[-<extra, ký tự lạ thành _>]-run-<i>.txt` = `$out`
     nguyên văn (+ listen_log cho split); cuối mỗi arm, khối summary nối vào `$OUT_DIR/summary.txt`.
     stdout in y như cũ.
 (c) W2W_EXTRA (mặc định rỗng): tách theo khoảng trắng thành mảng, nối sau `--warmup` ở lệnh
     combined (447) và lệnh --listen (357); KHÔNG đưa cho --connect (w2w từ chối --journal/--log ở
     đó). Identity như tls (470-475): W2W_EXTRA có `--journal X` → `$out` phải có dòng `journal: X`;
     `--log X` → `log: X`; sai → in `$out` + FAIL + exit 1. Header và khối summary in
     `extra   <W2W_EXTRA>` khi khác rỗng.
 (d) Hàm thuần `dispersion <label> <v…>` in
     `<label>  <median> ns      (across runs: <min> .. <max>)   min/median <x.xxx>   max/median <x.xxx>`
     cho p50, p99, p99.9 (và wire p50/p99/p99.9). Dòng `spread max/median` cũ (p50) GIỮ nguyên để
     bản ghi cũ so được. `median` và `dispersion` chuyển lên trên một guard
     `[ "${BASELINE_SOURCE_ONLY:-0}" = 1 ] && return 0` đặt ngay sau khối biến mặc định (trước
     mọi refuse và trước dòng 179), theo mẫu check-machine.sh:174.
 (e) Không biến mới → dòng lệnh w2w byte-giống: `bash -x` diff như A6 (chỉ khác các dòng header
     mới — nêu đúng dòng nào).
Test scripts/check-w2w-baseline-summary.sh (source với BASELINE_SOURCE_ONLY=1, helper `same` như
  verdicts): 20 giá trị lấy từ bẫy — 17323 17523 17864 18976 19136 19990 19992 19994 19996 19998
  19998 20000 20010 20020 20050 20100 20120 20140 20150 20158 → median 19998, min/median 0.866,
  max/median 1.008 (câu: "a run 13.4% under the median is visible from the min side"); một bộ
  đối xứng; n = 1 (1.000 / 1.000); n chẵn lấy trung bình nguyên như `median` hôm nay.
Đảo chiều (câu FAIL dự kiến): bỏ `min/median` khỏi hàm →
  `FAIL  want [p50  19998 ns … min/median 0.866  max/median 1.008] got [… max/median 1.008]  a run 13.4% under the median is visible from the min side`;
  trả lại → `pass N fail 0`.
Done when: test `fail 0`; `shellcheck -S info` sạch hai script; `bash -x` diff rỗng ngoài header;
  một run thật `PIN=0 RUNS=2 MESSAGES=2000 ARMS=hft:admin` — trên máy Linux BẤT KỲ hoặc trên desk
  chỉ trong khe build — cho thấy header, thư mục output, summary.txt; và
  `W2W_EXTRA="--journal file-async"` in `extra`, identity `journal: file-async` (cần binary S1).
Report: diff; output nguyên văn; không commit. Manager cập nhật mục *What guards it* của bẫy
  (tên test, ADR-0068) trong CÙNG commit — CLAUDE.md §4.
```

**Plan đổi gì.** Hàng *Chia việc* B2–B5, B6, B8 cột *Test / gate*: "20 run mỗi arm" → "**hai
procedure** mỗi arm công bố (ADR-0068), `RUNS` 20, quiet mỗi run, `FIXBOLT_NIC=enp9s0`; arm A/B
một procedure `RUNS=10`". Cột *File* thêm ADR-0068. `DESIGN.md` §8: thủ tục viết lại theo ADR-0068
ở bước docs của B2 (manager), bảng nào cũng hai cột. *Cách kiểm chứng* thêm: "số nào cũng là cặp;
lệch > 5 % ở percentile nào thì arm đó *không tái lập*". Item 85 đóng phần *guard* khi S2 lên; phần
*nguyên nhân* vẫn không ai nhận.

### Điều 3 — EEE và pause frames trên dây B6

**Plan viết** (B6): topology "cáp trực tiếp, I211, `igb`, `rx-usecs 0`, MTU 1500, kernel". Không
chữ nào về EEE hay pause. Nhật ký B0 ghi "EEE `enabled - active` và pause RX/TX đang bật ở cả hai
đầu".

**Thấy gì — và một chỗ nhật ký B0 ghi sai nguyên nhân.**

- **Nhật ký `sudo` của desk hôm nay** (`journalctl _COMM=sudo`): 21:17:31 `ethtool --show-eee
  enp9s0` (lúc đó đọc *enabled - active* — ghi chú B0 đúng cho thời điểm ấy); 21:28:49.240
  `ethtool -C enp9s0 rx-usecs 0`; **21:28:58.387 `ethtool --set-eee enp9s0 eee off`**; kernel
  `NIC Link is Up` 21:29:02.067; 21:29:02.079 `--show-eee`. Trong `igb` (v6.16, `igb_ethtool.c`),
  `igb_set_eee` khi đổi trạng thái gọi `igb_reinit_locked` — down/up, autoneg lại, ~4 s; còn
  `igb_set_coalesce` **không có** đường reset nào (toàn văn hàm: chỉ ghi `itr_val` cho từng
  q_vector). Vậy **link nhảy là do `--set-eee`, không phải `-C`** như nhật ký B0 và brief ghi; và
  **EEE đã tắt ở desk từ 21:29:02**. `carrier_changes` = 4 khớp: lên lúc 21:05:05 (cắm cáp), xuống
  rồi lên lúc 21:29:02.
- Đọc lúc 21:36:58: `EEE status: disabled`, `Tx LPI: disabled`, `Advertised EEE link modes: Not
  reported`, `Link partner advertised EEE link modes: 100baseT/Full 1000baseT/Full` (Mac vẫn quảng
  bá). Mac lúc 21:38: `ifconfig en0` → `media: autoselect (1000baseT <full-duplex,flow-control>)`,
  `networksetup -getMedia en0` → `Active: 1000baseT <full-duplex flow-control>` — **không** còn
  `energy-efficient-ethernet` trong phần active (trước 21:28:58 manager thấy có). Nghĩa là dòng
  active của macOS phản ánh **kết quả thoả thuận**, không phải cấu hình; và đúng như chuẩn, một
  đầu không quảng bá thì cả dây không dùng LPI. **Tắt ở desk là đủ; không cần đụng Mac.**
- Mac **có** tắt được phía nó nếu cần: `ifconfig -m en0` liệt kê `1000baseT mediaopt full-duplex
  mediaopt flow-control mediaopt energy-efficient-ethernet` là một media riêng, nên
  `networksetup -setMedia en0 1000baseT full-duplex flow-control` là "không EEE" — nhưng đặt thủ
  công là bỏ autoselect, và Apple Community ghi hộp thoại hiển thị sai giá trị đã đặt (macOS 15.6).
  **Không làm**: không cần, và đổi bên Mac cũng làm link nhảy.
- Chip Mac: `system_profiler SPEthernetDataType` → **Broadcom 57762-A0**, driver
  `AppleBCM5701Ethernet`, PCIe x1 2.5 GT/s, tối đa 1 Gb/s (khớp teardown ChargerLAB của Mac mini
  M4 A3238). macOS 26.6.2 (25G83), `Mac16,10`.
- **Pause**: `ethtool -a enp9s0` → autoneg on, RX on, TX on; dmesg link-up ghi `Flow Control:
  RX/TX`; bốn bộ đếm `rx_flow_control_xon/xoff`, `tx_flow_control_xon/xoff` đều **0**. Đổi pause
  (`ethtool -A`) đi qua `igb_set_pauseparam` → `igb_down`/`igb_up` → link nhảy → bẫy stamp `igb`.
  Pause chỉ phát khi bộ đệm thu đầy; một request trong chuyến ở 1 Gb/s không làm đầy gì.
- **Đầu nào ảnh hưởng cửa sổ RX→TX phần cứng của desk.** Chỉ **TX LPI của desk**: desk muốn gửi
  reply thì phải đánh thức đầu thu của Mac trước (Tw ≈ 16,5 µs ở 1000BASE-T), và stamp TX của I211
  lấy khi SFD rời MAC — tức **sau** khi đánh thức, nên chờ đánh thức nằm **trong** cửa sổ. TX LPI
  của Mac (request tới desk) và trạng thái RX của desk nằm **ngoài** cửa sổ desk và **trong** bảng
  Mac. Đây là suy luận từ chuẩn 802.3az và cách I210/I211 lấy stamp — *không tìm thấy nguồn nói
  thẳng stamp lấy trước hay sau đánh thức*, nên arm A/B bên dưới là thứ kiểm chứng. Ở interval
  1 s, nếu EEE bật thì chắc chắn ngủ; ở interval 0 (khoảng trống ~10–20 µs) không biết I211 có
  kịp vào LPI không (không tìm thấy timer vào LPI trong datasheet I211) → A/B đo cả hai.

**Đề nghị.**

- **Số công bố B6: EEE tắt ở desk** — trạng thái hiện tại. Trước **mỗi** procedure: `sudo -n
  ethtool --show-eee enp9s0` phải đọc `EEE status: disabled`; `ssh thangtran@192.168.77.2
  'ifconfig en0 | grep media'` phải **không** có `energy-efficient-ethernet`; cả hai dòng ghi tay
  vào nhật ký cạnh header (script không biết EEE).
- **Arm A/B EEE bật, một procedure, `RUNS=10`**, `hft:admin` ở interval 0 và interval 1 000 000
  (`MESSAGES=120`): `sudo -n ethtool --set-eee enp9s0 eee on` (link nhảy ~4 s: chờ `Link detected:
  yes`; đọc `--show-eee` → `EEE status: enabled - active`; Mac active có `energy-efficient-ethernet`;
  đọc lại năm `smp_affinity_list` vẫn `4`; run bỏ đầu tiên; `hw-rx-missing 0`), đo, rồi `eee off`,
  đọc lại cả hai đầu, run bỏ. Kết quả là **hiệu số**, nhãn A/B, vào `measured-costs.md` và
  `hft-playbook.md` §4. **Nếu** hiệu ở 1 s ≥ 5 % p50 hoặc cỡ 16 µs: `DESIGN.md` §9 thêm hàng *EEE
  off on the measurement NIC* và `check-machine.sh` thêm hàng `eee` đọc `ethtool --show-eee`
  (`disabled` PASS; `enabled - active` và `enabled - inactive` FAIL — *inactive* chỉ vì đầu kia
  chưa quảng bá; không `ethtool`/không hỗ trợ → UNKNOWN; test trong `check-machine-verdicts.sh`) —
  một bước code nhỏ sau B6, sonnet, cùng PR (Q15). Không đáng kể thì chỉ ghi ở playbook.
- **Pause: không đổi.** Bốn bộ đếm đọc trước/sau **mỗi** procedure, phải giữ 0; khác 0 là phát
  hiện, ghi cạnh số, không "chuẩn hoá".

**Dòng topology B6 mới** (thay cụm "ghi topology: …" trong hàng B6): *cáp thẳng giữa I211 (`igb`,
kernel `7.0.0-31-generic`) của desk và cổng Ethernet có sẵn của Mac mini `Mac16,10` (Broadcom
57762-A0, `AppleBCM5701Ethernet`, macOS 26.6.2); `192.168.77.1/24 ↔ .2/24`; 1000baseT full duplex;
**EEE tắt ở desk** (`--show-eee` → `disabled`; Mac active media không có
`energy-efficient-ethernet`); **pause RX/TX bật ở cả hai đầu, không đổi**, bốn bộ đếm
`*_flow_control_*` = 0 trước/sau; `rx-usecs 0`; MTU 1500; `tx_hwtstamp_skipped`, `hw-rx-missing`,
`hw-tx-missing` từng run; sha256 binary hai đầu.*

**Runbook B6, đọc trước và sau mỗi procedure** (thêm vào `hft-playbook.md` §6 mục 4 ở bước docs):

```sh
sudo -n ethtool --show-eee enp9s0                     # EEE status: disabled
sudo -n ethtool -a enp9s0                             # Autonegotiate on, RX on, TX on
ethtool -S enp9s0 | grep -E 'flow_control|hwtstamp'   # bốn bộ đếm pause 0; tx_hwtstamp_skipped
ethtool --get-hwtimestamp-cfg enp9s0                  # tx off, rx-filter none (như playbook)
cat /proc/irq/{85..89}/smp_affinity_list              # 4 ×5
ssh thangtran@192.168.77.2 'ifconfig en0 | grep media; shasum -a 256 Projects/nanofixengine/target/release/w2w'
```

### Điều 4 — bẫy `igb` ở B0: verdict tự thu hẹp phạm vi khi link chớp

**Thấy gì.** `scripts/check-machine.sh:214–226` chọn NIC theo **carrier**. Trong ~4 s link nhảy
(21:28:58 → 21:29:02, do `--set-eee`, Điều 3), không có NIC → hàng *NIC IRQ affinity* rơi về nhánh
đếm dòng `/proc/interrupts` và in UNKNOWN (dòng 509–513); hàng *coalescing* và *irqbalance* **biến
mất** (chỉ in khi `$NIC` khác rỗng) → 13 hàng, `pass 12 fail 0 unknown 1`; dòng 604 coi `unknown ≤
1` là đạt → *§9 satisfied*. Verdict đổi phạm vi mà không nói, và vẫn xanh. Ba giây sau, cùng lệnh:
15 hàng. `FIXBOLT_NIC` tường minh (từ B0) tránh được, nhưng không có gì canh người sau.

**Các lựa chọn.**

| # | Cách | Được | Mất |
|---|---|---|---|
| N1 | **Chọn NIC theo thiết bị vật lý**: `type` = 1, có `device/`, không `wireless/` và không `phy80211`; carrier chỉ để phân thắng bại khi có nhiều; header in dòng `nic …` | phạm vi verdict không phụ thuộc thứ chớp; hàng IRQ và coalescing vốn không cần cáp (A5 đã nói *carrier not required* cho `FIXBOLT_NIC`) | máy có NIC dây không cáp giờ đọc 15 hàng thay vì 13 — bẫy A5 đã chấp nhận "ghi chuỗi thật"; câu auto-select ở `hft-playbook.md` §4 sửa |
| N2 | Giữ carrier; thêm hàng UNKNOWN "NIC dây không carrier: enp9s0" → `unknown 2` → không satisfied | ít đổi | desk không cáp không bao giờ satisfied nếu không đặt `FIXBOLT_NIC`; vẫn phụ thuộc chớp, chỉ đổi chiều |
| N3 | Chỉ quy tắc "luôn đặt `FIXBOLT_NIC`" (đã làm từ B0) | không code | không có test; §4 nợ; người sau quên là lặp lại |

**Đề nghị: N1**, và N3 vẫn giữ trong runbook. Bẫy ghi ở `docs/reference/` tên
**`a-machine-check-narrowed-its-own-scope-when-the-link-bounced.md`** (manager viết, cùng commit
với S3): dòng thời gian ở Điều 3; vì sao dễ dính (chọn theo carrier + `unknown ≤ 1` là đạt); quy
tắc *phạm vi của một verdict không được phụ thuộc vào thứ chớp, và cái không nhìn thấy phải tự
nói ra trên một dòng của nó*; sự thật `igb`: `--set-eee` và `-A` làm link nhảy (source), `-C`
không; canh bằng test dưới đây + dòng `nic` + `FIXBOLT_NIC` trong mọi bước B.

**Spec S3 — developer (sonnet):**

```text
Role: developer (sonnet). Sửa 3 Điều 4 (S3).
Why: verdict §9 tự bỏ ba hàng NIC khi link chớp 4 s và vẫn in "satisfied".
Read first, exactly here: scripts/check-machine.sh:174 (guard), 176-190 (header), 202-226 (chọn
  NIC), 500-560 (hàng IRQ/coalescing); scripts/check-machine-verdicts.sh:16-31, 67-73, 86-105
  (mẫu cây giả); docs/hft-playbook.md:61-63.
Touch: scripts/check-machine.sh; scripts/check-machine-verdicts.sh; docs/hft-playbook.md §4 (câu
  auto-select). Do not touch: w2w-baseline.sh, bench.sh, tools/, crates/.
Đổi gì: hàm thuần `pick_nic <net-root> <explicit>` đặt TRƯỚC guard dòng 174. `explicit` khác rỗng
  → trả nguyên (như hôm nay, không cần carrier). Else duyệt `<net-root>/*/` theo thứ tự tên: giữ
  nếu `type` đọc 1, `device` tồn tại, không có `wireless` và không có `phy80211`; trả ứng viên đầu
  có `carrier` = 1, không có thì ứng viên đầu; không ứng viên → rỗng. Bỏ danh sách tên
  `lo|tailscale*|docker*|veth*|br-*|wl*` — mỗi cái đã bị loại bởi thuộc tính, test chứng minh.
  Gọi: `NIC=$(pick_nic /sys/class/net "${FIXBOLT_NIC:-}")`. Header, sau dòng `cores`:
  `nic       enp9s0 (carrier 1)` hoặc
  `nic       none — no wired NIC with a bus device under /sys/class/net; FIXBOLT_NIC=<name> names one`.
  Dòng `pass N fail N unknown N` KHÔNG đổi dạng.
Test (verdicts, mục `=== pick_nic`, cây giả mktemp):
  1. `enp9s0/type=1 carrier=0 device/` → `enp9s0`  — "a wired NIC without carrier is still picked — the §9 rows do not need a cable"
  2. `wlp7s0/type=1 carrier=1 device/ wireless/` → ""  — "wireless is never the measurement NIC"
  3. `tailscale0/type=65534`, `docker0/type=1` không device, `lo/type=772` → ""  — "virtual interfaces have no bus device"
  4. `enp8s0 carrier=0` + `enp9s0 carrier=1` → `enp9s0`  — "carrier breaks a tie, and only a tie"
  5. explicit `enp8s0`, cây rỗng → `enp8s0`  — "FIXBOLT_NIC wins without a carrier, as before"
Đảo chiều (câu FAIL dự kiến): đặt lại điều kiện carrier = 1 bắt buộc → case 1 in
  `FAIL  want [enp9s0] got []  a wired NIC without carrier is still picked — the §9 rows do not need a cable`;
  trả lại → `pass N fail 0` (N = 27 + 5).
Done when: verdicts `fail 0`; `shellcheck -S info` sạch hai script; trên desk CHỈ ĐỌC (script này
  nhẹ, chạy được giữa hai run): `scripts/check-machine.sh` không FIXBOLT_NIC in `nic enp9s0
  (carrier 1)` và cùng 15 hàng như `FIXBOLT_NIC=enp9s0`; với FIXBOLT_NIC output y hệt hôm nay
  trừ dòng `nic`; câu mới ở hft-playbook §4. Runner CI là guest (hàng virt FAIL sẵn) nên job
  `bench` không đổi kết luận; job `script-logic` xanh là bằng chứng cho runner.
Report: diff; output nguyên văn; không commit.
```

**Plan đổi gì.** *Bẫy đã lường trước* thêm hàng: *link chớp làm `check-machine.sh` tự bỏ ba hàng
NIC, vẫn "satisfied"* — canh: `pick_nic` + test case 1 + dòng `nic` + `FIXBOLT_NIC` trong mọi bước
B. Runbook B0 thêm một câu: *`--set-eee` và `ethtool -A` làm link nhảy ~4 s (`igb_reinit_locked`,
`igb_down/up`); `-C` không; sau mỗi lệnh đổi link, chờ `Link detected: yes` rồi mới `check-machine`
và run bỏ.* Hàng A5 của *Bẫy* ("không NIC → y nguyên `pass 12 … unknown 1`") giờ chỉ đúng cho máy
**không có NIC dây**; desk không cáp đọc 15 hàng.

### Điều 5 — Mac mini, `warp-svc`, và thứ tự B2–B9

**(a) `warp-svc` không đụng B7.** `warp-cli settings` → `Mode: DnsOverHttps`: không tunnel, không
interface `CloudflareWARP`. `sudo -n nft list tables` → sáu bảng (`ip`/`ip6` × `filter`, `nat`,
`mangle`), 16 chain, tất cả của Tailscale hoặc do iptables-nft tạo; **không có** `inet
cloudflare-warp` (bảng đó chỉ xuất hiện ở mode WARP theo tài liệu và cộng đồng Cloudflare). B7 giữ
nguyên. Thêm vào "trước" của B7: `warp-cli settings`, `ip rule`, `nft list ruleset` — để ai đọc số
sau biết một gói loopback đi qua `ip mangle OUTPUT` (kiểu **route**: mark đổi là kernel tra lại
route), `ip nat POSTROUTING`, `ip mangle PREROUTING`, `ip filter INPUT` (3 rule, nhảy `ts-input` 5
rule), cộng conntrack. Nếu ai bật mode WARP trước B7, đọc lại. Arm flush vẫn chỉ khi ở bàn (Q2);
chủ sở hữu đang điều khiển qua Tailscale trên Wi-Fi (`wlp7s0`, 192.168.31.125) nên bỏ, item 51 đóng
nửa như Q2 đã nói.
*`[2026-09-14, manager]` Câu trên viết trước khi chủ sở hữu trả lời. Chủ sở hữu trả lời manager:
"tôi có ở bàn" — **arm flush chạy** theo Q2, xem nhật ký giao hàng mục *Sửa 3 tới tay chủ sở hữu*.*

**(b) IRQ Wi-Fi.** `iwlwifi` IRQ 90 có mask `0-15` nhưng `effective_affinity_list` = `0`, 1,18 triệu
ngắt đều ở CPU0, **0** ở 6/7/14/15. Không phải vấn đề hôm nay. Một dòng `echo 4 | sudo -n tee
/proc/irq/90/smp_affinity_list` cho chắc là tuỳ manager, không làm giữa hai procedure.
`check-machine.sh` cố ý không nhìn Wi-Fi.

**(c) Binary trên Mac.** Checkout `4c373e0`, cây sạch, `target/release/w2w` mtime 21:20, sha256
`ef831cc944a8…`. Nửa `--connect` không dùng journal nên S1 không đổi nó; nhưng để hai đầu cùng
commit, **build lại trên Mac sau khe build** (CPU của Mac, không phải desk) và ghi sha256 hai đầu
(S2 làm tự động trong header). `w2w` không có `--help`/`--version` — chạy trần là đo (architect lỡ
làm thế ba dòng trên Mac lúc 21:38, không ảnh hưởng desk) → runbook: nhận diện binary bằng sha256 +
HEAD của checkout, không bằng cờ.

**(d) B4 thiếu cỡ message — plan viết thiếu, không phải sai.** `MESSAGES` mặc định 20 000; ở
interval 1 000 000 µs một run là 5,5 giờ. Đề nghị: `hft:admin:off:1000` giữ 20 000 (~20 s/run);
`hft:admin:off:10000` `MESSAGES=5000` (~50 s/run); `hft:admin:off:1000000` **và**
`standard:admin:off:1000000` (hàng "3 giờ sáng" là của engine ngủ) `MESSAGES=120 WARMUP=5 RUNS=10`
(~2 phút/run) — arm 1 s **chỉ công bố p50** (và min/max): với 120 mẫu, p99.9 là mẫu thứ 119 của
120 (`main.rs:1670`), không có nghĩa. Hai procedure như Điều 2.

**(e) Q3.** Không cần adapter; mục 2 của *Phần cứng chủ sở hữu phải chuẩn bị* gạch bỏ.

**(f) Thứ tự B2–B9 sau Sửa 3.** B1 đang chạy, không phụ thuộc gì. Sau khi chủ sở hữu duyệt: S2 và
S3 (bash, test không cần cargo) làm ngay trong worktree; S1 làm trong worktree nhưng **build và
gate chỉ trong khe build**. **Khe build, một lần, sau B1**: merge S1–S3 vào nhánh; `cargo build
--release -p fixbolt-w2w --features affinity`; `setcap` theo playbook; `scripts/bench.sh` cho
`journal-async-busy`; build bin ví dụ nanofix (A7); rồi run bỏ. Sau đó **không cargo cho tới B9**.
Procedure 1: **B2 → B5 → B8 → B4**; procedure 2 cùng thứ tự (khoảng cách tự nhiên ≥ 30 phút); **B7**
(đổi máy tạm, sau khi mọi số loopback đã có); **B6** (cáp; EEE tắt ×2 procedure, A/B ×1; muộn vì
nó đụng máy: EEE, IRQ pin, sysctl); **B9**; C (Q1); D. B3 bỏ (đã làm ở PR #71).

**Bảng *Chia việc* thêm ba hàng** (giữa B1 và B2–B5):

| Bước | Kết quả | File | Test / gate | §2 | Máy | Tier | Phụ thuộc |
|---|---|---|---|---|---|---|---|
| S1 | `--journal file-async` cùng vòng 4 096 ô với `Store` | `tools/w2w/src/main.rs`, `crates/engine/benches/alloc.rs` | `bench.sh` → `journal-async-busy 0`; đảo chiều `to_vec()` → ≥ 1; hai mode với cờ exit 0 `allocs 0`; không cờ y hệt | 1, 7 | Linux, **khe build** | sonnet, review ở B10 | Q12 |
| S2 | script ghi HEAD/cây/uptime/binary, giữ output, dispersion hai phía, `W2W_EXTRA` | `scripts/w2w-baseline.sh`, `scripts/check-w2w-baseline-summary.sh`, `ci.yml`, `hft-playbook.md` §6, bẫy item 85 (*What guards it*, manager) | test summary `fail 0`, đảo chiều bỏ `min/median`; `bash -x` diff; `shellcheck` | 10 | bất kỳ | sonnet | Q13 |
| S3 | `pick_nic` theo thiết bị vật lý; dòng `nic` | `scripts/check-machine.sh`, `scripts/check-machine-verdicts.sh`, `hft-playbook.md` §4, reference mới (manager) | 5 case mới, đảo chiều case 1; desk đọc 15 hàng không cần `FIXBOLT_NIC` | 4, 10 | bất kỳ (+ một lần đọc trên desk) | sonnet | Q16 |

Và *Tài liệu phải cập nhật* thêm: ADR-0068; `docs/reference/a-machine-check-narrowed-its-own-scope-when-the-link-bounced.md`;
`hft-playbook.md` §4 (EEE, pause, auto-select) và §6; `DESIGN.md` §8 thủ tục hai procedure, §9 hàng
EEE nếu Q15; `STATUS.md` item 85 (guard), 88 (đóng), item mới nếu A/B EEE đáng kể mà §9 chưa có hàng.

### Câu hỏi cho chủ sở hữu — mỗi câu một quyết định, đề nghị đứng trước

| # | Câu hỏi | Đề nghị | Nếu "không" |
|---|---|---|---|
| **Q12** | Điều 1: `--journal file-async` của w2w dùng vòng 4 096 ô như `Store` (J1), case alloc cùng cỡ? | **Có** | J2: thêm arm `mem-64` (+1 kiểu engine, +2 arm × 2 procedure); hoặc B5 bỏ, item 88 mở |
| **Q13** | Điều 2: ADR-0068 — mỗi arm công bố là **hai procedure**, tái lập = lệch ≤ 5 % ở mỗi percentile, cả hai cột vào §8; **S2 sửa script trước B2**; arm A/B một procedure `RUNS=10`? | **Có** | P3: chạy hai lần bằng script cũ, manager tính tay, §4 vẫn nợ test, B5 không chạy được bằng script; hoặc P1: boot B chỉ thu run thô, §8 trống |
| **Q14** | Điều 3: công bố B6 với EEE **tắt** ở desk (đã tắt từ 21:29); thêm A/B EEE bật hai arm (~15 phút, hai lần link nhảy); pause không đổi, chỉ đếm? | **Có** | Bỏ A/B: playbook chỉ ghi "EEE off, giá chưa đo"; công bố với EEE bật thì architect không ký |
| **Q15** | Nếu A/B cho thấy EEE đáng ≥ 5 % p50 ở 1 s: `DESIGN.md` §9 thêm hàng *EEE off* và `check-machine.sh` thêm hàng `eee` (bước code nhỏ sau B6, cùng PR)? | **Có** | Chỉ ghi ở `hft-playbook.md` §4; §9 không có hàng, không ai canh |
| **Q16** | Điều 4: `check-machine.sh` chọn NIC theo thiết bị vật lý, carrier chỉ phân thắng bại, dòng `nic` trong header, 5 test (N1)? | **Có** | N2 (hàng UNKNOWN thứ hai, desk không cáp không bao giờ satisfied) hoặc N3 (chỉ quy tắc, không test — §4 nợ) |
| **Q17** | Điều 5: một khe build sau B1; thứ tự B2→B5→B8→B4 ×2, B7, B6, B9; cỡ message B4 như (d), arm 1 s chỉ p50; cắt từ `interval 10 000` → B6 app 1 s → B8 app → `standard` 1 s nếu quá ngày? | **Có** | Giữ thứ tự cũ; B4 vẫn thiếu cỡ message — phải quyết trước khi chạy |

Không câu nào chặn B1. S2 và S3 làm được ngay sau khi duyệt, không cần cargo; S1 chờ khe build.

**`[2026-09-14]` Chủ sở hữu duyệt Sửa 3 theo đề xuất**, nguyên văn *"Duyệt"*: Q12 = vòng 4 096 ô
(J1); Q13 = ADR-0068 (giờ *Accepted*) và S2 trước B2; Q14 = B6 công bố với EEE tắt, thêm A/B EEE
bật, pause chỉ đếm; Q15 = hàng §9 *EEE off* và hàng `eee` của `check-machine.sh` nếu A/B ≥ 5 %;
Q16 = `pick_nic` theo thiết bị vật lý (N1); Q17 = khe build sau B1, thứ tự và cỡ message như Điều 5.

### Nguồn (tra 2026-09-14)

- Kernel `drivers/net/ethernet/intel/igb/igb_ethtool.c` v6.16 —
  <https://raw.githubusercontent.com/torvalds/linux/v6.16/drivers/net/ethernet/intel/igb/igb_ethtool.c>:
  `igb_set_coalesce` toàn văn (không `igb_reset`/`igb_reinit_locked`/`igb_down`; chỉ ghi
  `itr_val`); `igb_set_eee` (`if (hw->dev_spec._82575.eee_disable != !edata->eee_enabled) { …
  igb_reinit_locked(adapter) … }`); `igb_set_pauseparam` (`igb_down`/`igb_up` khi `fc_autoneg`);
  `igb_get_eee` (`eee_enabled = !hw->dev_spec._82575.eee_disable`; LP advertised đọc qua
  `igb_read_xmdio_reg`).
- Kernel `igb_main.c` v6.16 — `igb_probe`/`igb_reset` gọi `igb_set_eee_i350(hw, true, true)` cho
  i350/i210/i211 (EEE bật mặc định khi `eee_disable` chưa đặt):
  <https://raw.githubusercontent.com/torvalds/linux/v6.16/drivers/net/ethernet/intel/igb/igb_main.c>.
- IEEE 802.3az task force, Grimwood 07/2008 — thời gian đánh thức 16,5 µs (1000BASE-T), 30 µs
  (100BASE-TX): <https://www.ieee802.org/3/az/public/jul08/grimwood_02_0708.pdf>. Wikipedia
  *Energy-Efficient Ethernet* (LPI, hai đầu cùng quảng bá):
  <https://en.wikipedia.org/wiki/Energy-Efficient_Ethernet>.
- Intel I211 datasheet v3.4 (I211 hỗ trợ 802.3az):
  <https://cdrdv2-public.intel.com/333017/333017%20-%20I211_Datasheet_v_3_4.pdf> — *không tìm
  thấy* trong trích đoạn timer vào LPI của I211, cũng *không tìm thấy nguồn nào nói thẳng* stamp
  TX lấy trước hay sau đánh thức LPI; arm A/B thay cho việc đọc.
- macOS: Apple Community 2025, Mac mini macOS 15.6 — media `full-duplex,flow-control` là "mặc định
  trừ `energy-efficient-ethernet`", hộp thoại Hardware hiển thị sai giá trị đã đặt:
  <https://discussions.apple.com/thread/256119427>; thread 254643898 chỉ nói EEE hiện ở
  *Network → Hardware* khi phần cứng có. Trên Mac này, `ifconfig -m en0` liệt kê media có
  `mediaopt energy-efficient-ethernet` (đọc 21:44).
- Mac mini M4 (A3238) teardown, ChargerLAB — Broadcom BCM57762 gigabit:
  <https://www.chargerlab.com/teardown-of-apple-m4-mac-mini-a3238/>; xác nhận trên máy bằng
  `system_profiler SPEthernetDataType` → *Broadcom 57762-A0*, `AppleBCM5701Ethernet`.
- Cloudflare WARP trên Linux tạo bảng `inet cloudflare-warp` (mode WARP) và từng flush cả
  ruleset khi disconnect:
  <https://community.cloudflare.com/t/cloudflare-warp-linux-client-flush-whole-nftables-when-disconnecting/380404>;
  kiến trúc client:
  <https://developers.cloudflare.com/cloudflare-one/team-and-resources/devices/warp/configure-warp/route-traffic/warp-architecture/>.
  Desk hôm nay ở mode `DnsOverHttps`, không có bảng đó (`nft list tables`, 21:37).
- Mytkowicz et al., *Producing Wrong Data Without Doing Anything Obviously Wrong!*, ASPLOS 2009 —
  <https://users.cs.northwestern.edu/~robby/courses/322-2013-spring/mytkowicz-wrong-data.pdf>:
  môi trường đo dịch kết quả hơn cả hiệu ứng đang đo; chữa bằng đổi setup và báo spread giữa
  setup — nền của ADR-0068.
- `ethtool(8)` — `--set-eee`, `-A`, `-C`: <https://man7.org/linux/man-pages/man8/ethtool.8.html>.
- Đọc trên máy (desk 21:36–21:45; Mac qua ssh 21:38–21:44): `ethtool --show-eee/-a/-c/-S/-i
  enp9s0`; `/sys/class/net/*/{type,carrier,carrier_changes,device}`; `/proc/irq/{85..90}/
  {smp,effective}_affinity_list`; `/proc/interrupts`; `journalctl _COMM=sudo` và kernel/NM
  21:27–21:30; `sudo -n nft list tables|ruleset|table ip mangle`; `ip rule`; `warp-cli settings`;
  Mac: `sysctl hw.model kern.osproductversion`, `ifconfig -m en0`, `networksetup -getMedia en0`,
  `system_profiler SPEthernetDataType`, `git rev-parse/status`, `shasum -a 256`.
