# Lần thứ hai ở bàn Linux: NIC thật, cache lạnh, và những con số còn thiếu

> **Loại:** Plan · **Ngày:** 2026-09-04 · **Trạng thái:** **Đã duyệt 2026-09-13** (Sửa 1, theo đề xuất Q1–Q7) — chưa bắt đầu
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

*(Đã duyệt 2026-09-13 — chưa bắt đầu.)*

**`[2026-09-14]` B3 đã làm xong, ngoài plan này.** Tls bước 6 (6-M và 7b) chạy trong một boot §9
riêng ngày 2026-09-14, theo đúng Q7 ("boot không chờ — tls bước 6 nhận một boot riêng"), nhánh
`plan/tls-numbers-on-s9` — xem nhật ký phiên B của [plan `tls`](2026-09-04-tls.md). **Boot B bỏ
B3.** Hai điều từ lần đó chạm vào plan này:

- **B2 so với cả hai lần đo của ngày 2026-09-14, không chỉ với bảng 2026-09-02.** Ba arm plain
  loopback (`hft` admin, `hft` app, `standard` admin, arm `off`) đã chạy hai lần trong boot đó —
  `DESIGN.md` §8 *The round trip under TLS, measured* và `measured-costs.md` *TLS on the wire* —
  và lệch bảng 2026-09-02 tối đa 2,1% ở p50. B2 chạy arm `standard` app thì chưa có số mới để so.
- **Item 85 chạm cách B2 công bố.** Cùng lệnh, cùng boot, cách nhau nửa giờ, một arm dịch 15,9%
  trong khi spread của nó ghi 1,008. Nếu B2 chỉ chạy `w2w-baseline.sh` một lần rồi đưa median vào
  §8, nó lặp lại đúng điều item 85 ghi. Trước khi B2 công bố, đọc
  [a-tight-spread-inside-one-procedure-did-not-reproduce-across-two](../reference/a-tight-spread-inside-one-procedure-did-not-reproduce-across-two.md)
  và xem item 85 đã có plan chưa.

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
