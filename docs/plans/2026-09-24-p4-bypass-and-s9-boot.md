# Phase 4, hàng 6 và 7: dụng cụ đo cho Onload trên AF_XDP, và boot §9 duy nhất đo cả `io_uring` lẫn Onload

> **Loại:** Plan · **Ngày:** 2026-09-24 · **Trạng thái:** Đã duyệt (anh trả lời Q1, Q2 ngày 2026-09-24; manager duyệt phần còn lại theo uỷ quyền 2026-09-18) · **Sửa 2 (2026-09-24): Onload bị bỏ ở cổng G1; hàng 7 chỉ đo `io_uring` và cặp store**
> **Phạm vi:** hàng 6 và 7 của bảng *Chia việc* trong [phase-4-scope](2026-09-23-phase-4-scope.md);
> kèm [ADR-0200](../decisions/ADR-0200-a-bypass-arm-is-judged-from-the-counterparty-against-a-same-boot-kernel-twin-and-its-kill-line-is-arithmetic-written-first.md),
> [ADR-0201](../decisions/ADR-0201-onload-on-the-i211-runs-without-hardware-flow-filters-one-channel-count-holds-for-the-boot-and-the-control-path-leaves-the-cable.md),
> [ADR-0202](../decisions/ADR-0202-phase-4s-one-s9-boot-is-pre-built-driven-by-a-committed-script-and-onload-lives-only-inside-its-block.md)

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Tóm tắt một đoạn

**Sửa 2, 2026-09-24 — Onload trên AF_XDP bị bỏ ở cổng G1 của bước thăm dò, trước khi có phép đo
nào.** Lần 1, Onload `v9.0.2` không biên dịch được trên kernel `7.0.0-31`. Lần 2, bản ghim
`174b947d0b` (nhánh `v9_2`) biên dịch và cài được, nhưng **không dựng được stack trên `enp9s0`**:
Onload đòi driver trả lời "khoá băm RSS dài bao nhiêu" (`get_rxfh_key_size`), và `igb` của kernel
này không có thao tác đó — kể cả khi đã tắt bộ lọc và để NIC còn một hàng đợi. Mã Onload ở `v9.0.2`,
`v9_2` và `master` đều coi thiếu thao tác đó là lỗi, nên đổi bản Onload không cứu được. Theo Q1 của
ADR-0098, **bị bỏ vẫn tính là xong**. Vì vậy: các dụng cụ đo riêng cho Onload (6.2–6.5) **không
dựng**; 6.6 chỉ còn gỡ Onload khỏi máy bàn và ghi lại việc bỏ; **hàng 7 là boot §9 đo `io_uring`
(hàng 5) và cặp `w2w` của store SQLite (hàng 4b)**. Phép tính "phải cắt 86 %" viết trước (bên dưới)
chưa bao giờ được đem ra thử — Onload dừng trước đó.

*Tóm tắt ban đầu (trước Sửa 2), giữ lại để thấy plan đã tính gì:* hàng 6 dựng dụng cụ đo cho nhánh
Onload, hàng 7 đo A/B `io_uring` và A/B Onload trong một boot; thời gian khứ hồi đo từ Mac ~232 µs
trong khi toàn bộ phần máy bàn ~27 µs, nên muốn p50 phía Mac tốt hơn 10 % thì Onload phải cắt ~86 %
phần máy bàn — dự đoán: bỏ.

## Bối cảnh

Phase 4 (ADR-0098) cho Onload trên AF_XDP vào bằng một **vạch bỏ viết trước**: giữ chỉ khi p50 đo từ
phía Mac tốt hơn ≥ 10 % ở cả hai procedure, p99 không tệ hơn, zero-copy bind thật, 59/59 dưới
`onload`. Engine không đổi một dòng — engine chạy nguyên dưới `onload`. Việc cần làm là **đo cho
trung thực**: đảm bảo mỗi lần chạy đúng là đi qua Onload (không lặng lẽ rơi về kernel), đảm bảo
zero-copy được đọc lại từ kernel chứ không phải tin lời cấu hình, và so hai nhánh bằng cùng một
thước.

Hàng 5 (`io_uring`) do một architect khác viết plan
(`docs/plans/2026-09-24-p4-io-uring-transport.md`, đang viết). Hàng 7 chỉ
**chạy phép đo** của nó trong cùng boot; phần *Giao diện cần từ hàng 5* bên dưới nói hàng 7 cần gì,
không hơn.

Boot §9 cần đổi dòng grub và khởi động lại máy. **Khởi động lại là hết phiên**, nên hàng 7 tách làm
hai pull request: 7a chuẩn bị mọi thứ trước reboot, 7b là phiên chạy boot.

## Những gì đã biết chắc

**Trên máy bàn, chỉ đọc, ngày 2026-09-24** (`tmt-B450-I-AORUS-PRO-WIFI`, kernel `7.0.0-31-generic`):

- Máy đang ở dòng grub desktop (`/proc/cmdline`: `quiet splash`, không có `isolcpus`);
  `/etc/default/grub` giống hệt `grub.fixbolt-desktop-20260922`. Dòng §9 dùng cho boot là
  `grub.fixbolt-s9-bootf-20260923` (`isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15
  processor.max_cstate=1`). **`grub.fixbolt-s9` có `nohz_full` — không dùng.**
- `ip -br link` / `ip -4 -br addr`: `enp9s0` (cáp thẳng sang Mac, `192.168.77.1/24`), `wlp7s0`
  (Wi-Fi, `192.168.31.125`), `tailscale0` (`100.99.156.121`). Trên Tailscale, Mac mini là
  `thangs-mac-mini` `100.108.2.110` (lúc đọc: offline 3 giờ).
- `ethtool -i enp9s0`: driver `igb`, firmware `0.6-1`, bus `0000:09:00.0`.
- `ethtool -l enp9s0`: **2 hàng đợi** (combined, tối đa 2). `ethtool -x`: RSS chia đôi — mục 0–63
  vào hàng 0, 64–127 vào hàng 1. `ethtool -k`: `ntuple-filters: off` (bật được).
- `ethtool -T enp9s0`: có hardware TX/RX timestamp, bộ lọc RX `none`/`all`.
- `/proc/kallsyms` có đường AF_XDP zero-copy của `igb`: `igb_xsk_pool_setup`, `igb_xmit_zc`,
  `igb_clean_rx_irq_zc`, `igb_run_xdp_zc`, `igb_xsk_wakeup`. **Không có** hàm `igb` nào cho
  timestamp XDP RX hay metadata XSK TX.
- `/boot/config-7.0.0-31-generic`: `CONFIG_XDP_SOCKETS=y`, `CONFIG_XDP_SOCKETS_DIAG=m`,
  `CONFIG_BPF_SYSCALL=y`, `CONFIG_BPF_JIT=y`.
- `ss -V`: iproute2 6.19.0 — có `ss --xdp`, in `zc:` cho mỗi XSK. `bpftool` có ở `/usr/sbin`.
- `mokutil --sb-state`: **Secure Boot tắt** — module không ký vẫn nạp được.
- `dpkg -l`: có `linux-headers-7.0.0-31-generic`, `g++`; **chưa có** `gawk`, `libcap-dev`,
  `libmnl-dev`. Chưa cài Onload, không có module `sfc_resource`.
- Trạng thái runtime hiện tại (sẽ đặt lại trong boot): EEE `enabled - active`, `rx-usecs 3`.

**Trong repo:**

- **Số phía Mac đã có** — `docs/reference/measured-costs.md` *C-40* (boot C, 2026-09-18, `hft`,
  interval 0, qua cáp): wire p50 phía máy bàn **27 050 ‖ 27 114 ns** (admin), **28 894 ‖ 28 878 ns**
  (app); **phía Mac p50 ~232 µs** ở cả hai nhánh, p99.9 0.37–0.95 ms.
- `tools/w2w` đo phía máy bàn bằng một tap `AF_PACKET` (RX) và error queue của socket engine (TX)
  (`tools/w2w/src/main.rs`, mục *Wire timestamps*). Chế độ tách (`--listen` / `--connect`) đã có;
  phía `--connect` in bảng *"as the counterparty sees it"*.
- `scripts/w2w-baseline.sh` đã chạy được generator trên Mac qua `GENERATOR_SSH`, đã có `W2W_EXTRA`,
  `WIRE_NIC`, `FIXBOLT_NIC`; `scripts/compare-w2w-procedures.sh` so hai procedure (ADR-0068, dải 5 %).
- `crates/engine/tests/wire.rs:205` bind `127.0.0.1:0` — 59 định nghĩa chạy qua **loopback**, hai
  đầu trong một process.
- `scripts/check-machine.sh` hiện có 17 dòng khi chọn NIC; khối NIC chỉ in khi có NIC (khuôn
  `FIXBOLT_NIC` — không chọn thì output không đổi). Hàm thuần được test ở
  `scripts/check-machine-verdicts.sh` (`MACHINE_SOURCE_ONLY=1`).
- Lệnh runtime mỗi boot (bộ nhớ boot F): `sudo -n fixbolt-machine on`, dừng timer, IRQ của
  `enp9s0` → lõi 0–5, `rx-usecs 0`, EEE off; kỳ vọng `pass 17 fail 0 unknown 0`.

**Ngoài repo (đọc 2026-09-24):**

- Onload chạy trên card không phải Solarflare qua AF_XDP, **"community-supported work in progress,
  not currently at release quality"**; hỗ trợ kernel **6.1 – 7.0**, Ubuntu LTS 24.04+; build không
  cần card Solarflare bằng `--no-sfc` / `HAVE_SFC=0`; đăng ký NIC bằng
  `echo <if> > /sys/module/sfc_resource/afxdp/register`; driver không có native XDP thì Onload thử
  generic XDP — <https://github.com/Xilinx-CNS/onload/blob/master/README.md>. Build/cài:
  `scripts/onload_mkdist`, `scripts/onload_mkpackage --install`, cần `gawk`, `libcap-devel`,
  `libmnl-devel`, header kernel — `DEVELOPING.md` cùng repo. Tag mới nhất `v9.0.2`.
- **`v9.0.2` không build được trên `7.0.0-31`** `[đo 2026-09-24, bước 6.1 lần 1]`:
  `src/lib/efhw/af_xdp.c:375:28: error: passing argument 2 of ‘kernel_bind’ from incompatible pointer
  type … expected ‘struct sockaddr_unsized *’` — kernel 6.19 đổi kiểu tham số của `kernel_bind`.
  Upstream sửa ở commit `268f1d4c8a` *"ON-17217: Add compat for sockaddr_unsized (6.19)"*
  (2026-02-23); **không có tag nào sau `v9.0.2`** chứa nó. Nhánh phát hành `v9_2` (đầu nhánh
  `174b947d0b`, 2026-08-18, `versions.env`: `ONLOAD_VERSION=9.2.2`) chứa nó (GitHub compare:
  `268f1d4c8a...v9_2` ahead 269, behind 0); `master` (`0ba003ed33`, 2026-09-21) đi trước `v9_2` 82
  commit; README ở `v9_2` ghi kernel 5.11 – 7.0.
- **Onload chèn bộ lọc luồng TCP qua ethtool n-tuple** (`ETHTOOL_SRXCLSRLINS`), trừ khi tham số
  module `enable_af_xdp_flow_filters=0` — Onload `src/lib/efhw/af_xdp.c`.
- **`igb` chỉ nhận luật Ethernet** (`ETHER_FLOW`: EtherType, VLAN, MAC), luật TCP trả `-EINVAL` —
  Linux `drivers/net/ethernet/intel/igb/igb_ethtool.c`. Issue Onload #10 (Intel 82599) cho thấy hậu
  quả: `FILTER TCP ... failed (-95)` — <https://github.com/Xilinx-CNS/onload/issues/10>.
- Chương trình XDP của Onload chuyển **mọi** gói IPv4 TCP/UDP trên hàng đợi của nó sang XSK, trả
  `XDP_PASS` cho broadcast, IPv6, không phải TCP/UDP — Onload `src/lib/efhw/af_xdp_bpf.c`.
- **Onload có thể lặng lẽ rơi về kernel**: stack tạo thất bại, socket âm thầm thành socket kernel —
  <https://github.com/Xilinx-CNS/onload/issues/337> (cùng dấu hiệu #62, #83, #10). `EF_NO_FAIL=0`
  biến thất bại đó thành lỗi (mặc định `1`) — Onload `src/include/ci/internal/opts_citp_def.h`.
- `EF_AF_XDP_ZEROCOPY` mặc định `1` và bắt buộc zero-copy (`opts_netif_def.h`); bind với
  `XDP_ZEROCOPY` "sẽ ép zero-copy hoặc thất bại" — <https://docs.kernel.org/networking/af_xdp.html>.
  Kernel báo chế độ của XSK qua `xsk_diag`: `ss --xdp -a -e` in `zc:1` (iproute2 `misc/ss.c`).
- **Loopback không được Onload tăng tốc theo mặc định**: `EF_TCP_SERVER_LOOPBACK` và
  `EF_TCP_CLIENT_LOOPBACK` mặc định `0` (`opts_netif_def.h`).
- Một XSK chỉ nhận từ **một** hàng đợi; phải giới hạn NIC còn một hàng hoặc dùng bộ lọc NIC —
  kernel `af_xdp.html`. Issue #139 chỉ chạy được sau `ethtool -L <if> combined 1`, và đo được Onload
  AF_XDP **chậm hơn kernel 16 %** (UDP ping-pong, AWS) — <https://github.com/Xilinx-CNS/onload/issues/139>.
- AF_XDP zero-copy cho `igb` vào Linux 6.14, thử trên i210 —
  <https://lwn.net/Articles/986006/>, <https://www.phoronix.com/news/IntelIGB-AF-XDP-Zero-Copy>.
- Timestamp qua AF_XDP: metadata TX của XSK có ở `igc`, `stmmac`, không nhắc `igb` —
  <https://docs.kernel.org/networking/xsk-tx-metadata.html>. Chỗ duy nhất tìm thấy Onload dùng
  hardware timestamp là `EF_RX_TIMESTAMPING` trên card Solarflare —
  <https://github.com/Xilinx-CNS/onload/issues/51>.
- AF_XDP tốt nhất 6.5–9.7 µs khứ hồi trên card 100/40 GbE **có** busy poll; zero-copy không poll chạy
  tệ trên driver Intel — <https://arxiv.org/html/2402.10513v1>.

**Tìm mà không thấy:** số đo Onload-trên-AF_XDP nào với card `igb`; tài liệu nào nói Onload cho
hardware timestamp qua AF_XDP; tài liệu nào nói stack Onload làm gì với gói TCP của một socket không
thuộc nó khi tắt bộ lọc.

### Kết luận về zero-copy trên I211

**Zero-copy không hỏng "từ thiết kế".** Driver `igb` của kernel này có đường AF_XDP zero-copy (symbol
có thật), có native XDP, và Onload bind `XDP_ZEROCOPY` — bind thất bại thì báo lỗi, không lặng lẽ
xuống copy. Chỗ hỏng thật nằm ở **bộ lọc**: Onload cần luật TCP n-tuple, `igb` chỉ nhận luật
Ethernet. Cách đi vòng (ADR-0201): tắt bộ lọc của Onload và cho NIC còn **một** hàng đợi, để hàng đợi
đó là của Onload. Việc zero-copy có bind thật hay không được **đọc lại từ kernel** (`ss --xdp`,
`zc:1`) ở bước thăm dò 6.1 — trước khi viết một dòng script nào.

**Sửa 2:** thăm dò không bao giờ tới được bước bind zero-copy hay bộ lọc — Onload dừng sớm hơn, lúc
khởi tạo NIC, vì thiếu thao tác khoá RSS (xem *Nhật ký giao hàng* và ADR-0201 *Result*). Hai kết luận
trên đúng về driver nhưng không được thử.

**Hardware timestamp không sống sót dưới Onload**: chương trình XDP chuyển gói đi trước khi kernel
tạo `skb`, nên tap `AF_PACKET` của `w2w` không thấy gói nào; gói gửi đi qua vòng TX của XSK, không
qua socket kernel có `SO_TIMESTAMPING`; `igb` không có đường metadata timestamp cho XDP. Nên — đúng
như ADR-0098 đã lường — hai nhánh chỉ so được bằng số **phía Mac**.

### Phép tính mà vạch bỏ dựa vào

| Đại lượng | Giá trị | Nguồn |
|---|---|---|
| p50 phía Mac (interval 0) | ~232 µs | *C-40* |
| 10 % của nó — phần Onload phải cắt | ~23.2 µs | tính |
| Toàn bộ cửa sổ máy bàn, NIC vào → NIC ra | 27.05 µs (admin), 28.89 µs (app) | *C-40* |
| Phần Onload phải cắt, tính trên cửa sổ đó | **86 %** (admin), **80 %** (app) | tính |

Trong cửa sổ ~27 µs đó, Onload **không** bỏ được: thời gian khung request tới hết trên dây 1 GbE (dấu
RX lấy ở đầu khung), DMA và ngắt của NIC (zero-copy vẫn chạy NAPI của driver), lệnh `sendto()` đánh
thức TX (kernel `af_xdp.html`, `XDP_USE_NEED_WAKEUP`), và việc của chính engine (riêng app − admin
đã là 1.8 µs). Không nguồn nào cho thấy AF_XDP cắt được gần chừng ấy. **Dự đoán: bỏ, vì mệnh đề
p50.** Đây là dự đoán, không phải kết quả — ADR-0200 quyết định 4 ghi nó ra trước để kết quả đo xác
nhận hoặc bác bỏ nó.

## Cách làm

### Hàng 6 — dụng cụ đo (một PR, nhánh `plan/p4-bypass-boot`)

**6.1 Cài Onload và thăm dò, trên dòng grub desktop, không phải số đo.** *Sửa 1, 2026-09-24
(manager duyệt lại theo uỷ quyền):* lần 1 ghim tag `v9.0.2` và build hỏng (G1, xem *Những gì đã biết
chắc*). Ghim mới là **commit `174b947d0b9b7b77439463afbabf8a7e417b3706`** — đầu nhánh phát hành `v9_2` của
Onload, **không có tag** — thay vì đầu `master`: nó chứa bản sửa `268f1d4c8a`, là nhánh AMD dùng để
ra bản 9.2.x (`versions.env` = 9.2.2), và không mang 82 commit chưa phát hành của `master` (trong đó
có đổi cách chọn datapath, ON-17442, đúng vùng mã quyết định card nào được tăng tốc). Mọi dòng §9 và
mọi con số ghi tên phiên bản là *"Onload `174b947d0b` (nhánh `v9_2`, không tag)"*.
Cài vào `~/src/onload` — **ngoài repo, không gì vào git**: `sudo -n apt-get install -y gawk
libcap-dev libmnl-dev`, `git clone https://github.com/Xilinx-CNS/onload ~/src/onload` (hoặc
`git -C ~/src/onload fetch origin`), `git -C ~/src/onload checkout 174b947d0b9b7b77439463afbabf8a7e417b3706`,
**cổng trước khi build**: `git -C ~/src/onload merge-base --is-ancestor 268f1d4c8a HEAD && echo
has-sockaddr-unsized-fix` phải in `has-sockaddr-unsized-fix` (không in → dừng), và
`git -C ~/src/onload rev-parse HEAD` quote nguyên văn; rồi
`sudo -n ~/src/onload/scripts/onload_install --no-sfc`,
`sudo -n onload_tool reload --onload-only` (các tham số `--no-sfc`, `--onload-only` theo README của
Onload). Rồi thăm dò: đăng ký `enp9s0` với
bộ lọc mặc định, đọc `dmesg`; nếu `igb` từ chối (dự kiến), đặt `enable_af_xdp_flow_filters=0` và
`ethtool -L enp9s0 combined 1`, thử lại. Chạy một lần `w2w --listen` dưới `onload` (không ghim lõi —
dòng desktop không có lõi cô lập) với generator trên Mac điều khiển qua địa chỉ **không phải cáp**,
và đọc: `ss --xdp -a -e` (`zc:1` trên `ifindex` của `enp9s0`), `bpftool net show dev enp9s0`
(`xdpdrv` hay `xdpgeneric`), bộ đếm TCP của kernel (`/proc/net/snmp`) trước/sau, dòng `allocs` của cả
hai nửa, `onload_stackdump lots`, và **đường dẫn sysfs** cho biết NIC đã đăng ký (dự kiến
`/sys/class/net/enp9s0/sfc_resource/enable`, từ Onload `src/driver/linux_resource/sysfs.c`), **lúc
nào** chương trình XDP được gắn (khi đăng ký hay khi tạo stack), gắn có làm link nhảy không, và XSK
đã có mặt ngay khi `listening:` in ra chưa. Chạy thêm bộ 59 định nghĩa dưới `onload` với loopback
tăng tốc. Cuối cùng gỡ đăng ký, `onload_tool unload`, trả `combined 2`, và kiểm Onload **không** tự
nạp khi khởi động (`/etc/modules-load.d`, `/etc/modprobe.d`, unit systemd).

Kết quả thăm dò là **cổng**:

| Cổng | Nếu thấy | Thì |
|---|---|---|
| G1 | Không dựng được stack Onload, kể cả khi đã tắt bộ lọc + `combined 1` | **Bỏ Onload ngay**, bằng chứng là output thăm dò; boot chỉ đo `io_uring` |
| G2 | XSK không có `zc:1`, hoặc chương trình XDP là `xdpgeneric` | **Bỏ** — mệnh đề "zero-copy bind thật" không đạt |
| G3 | `allocs` ≠ 0 trên luồng engine dưới `onload` | **Bỏ** — phạm bất biến 1 |
| G4 | Bộ đếm TCP kernel tăng cỡ số request (Onload rơi về kernel) dù `EF_NO_FAIL=0` | Sửa cấu hình **một lần**; vẫn thế thì **bỏ** |
| G5 | Cả bốn qua | Onload vào boot |

Cổng nào bỏ Onload thì các bước 6.2–6.6 chỉ còn phần tài liệu (bẫy + kết quả), và hàng 7 chỉ đo
`io_uring`.

**Kết quả 6.1 (Sửa 2): G1 hỏng hai lần — Onload bị bỏ.**

| Lần | Onload | Dừng ở | Dòng nguyên văn |
|---|---|---|---|
| 1 | `v9.0.2` (`9f330e7058`) | biên dịch | `src/lib/efhw/af_xdp.c:375:28: error: passing argument 2 of ‘kernel_bind’ from incompatible pointer type … expected ‘struct sockaddr_unsized *’` |
| 2 | `174b947d0b` (nhánh `v9_2`, không tag; cổng `merge-base` in `has-sockaddr-unsized-fix`) | đăng ký `enp9s0` | `` [sfc efhw] af_xdp_rss_get_support: enp9s0 does not support `get_rxfh_key_size` operation `` · `[sfc efrm] ?: ERROR: hardware init failed rc=-95` |

Lần 2 hỏng y hệt với bộ lọc mặc định, với `enable_af_xdp_flow_filters=0`, và với
`ethtool -L enp9s0 combined 1`; sau đó ghi `register` trả errno 114 (`EALREADY`), `unregister` trả
errno 16 (`EBUSY`); `onload_tool unload --onload-only` gỡ được module. G2–G4 không áp dụng — không có
stack nào. Output: `target/p4-probe/{onload_install-v9_2.txt, probe-v9_2.txt, after-v9_2.txt}` (trên
desk, không commit).

**Vì sao là do máy này, không do bản Onload:** hàm `af_xdp_rss_get_support` của Onload
(`src/lib/efhw/af_xdp.c`, có ở `v9.0.2`, `v9_2`, `master`) trả `-EOPNOTSUPP` khi driver thiếu
`get_rxfh_indir_size` hoặc `get_rxfh_key_size`, và `af_xdp_nic_init_hardware` trả thẳng lỗi đó.
`igb` trong kernel `7.0.0-31` có `get_rxfh_indir_size` nhưng **không có** `get_rxfh_key_size` —
chính `ethtool -x enp9s0` đọc hôm nay đã in `RSS hash key: Operation not supported`. Upstream Linux
thêm thao tác này cho `igb` bằng các commit `dfaf57ef99cf`, `1ae67b2b28bc` *"igb: expose RSS key via
ethtool get_rxfh"*, `e3c94e9782a7` (vào cây 2026-07-01), có từ **`v7.3-rc1`**, không có trong
`v7.2`. **Tìm trong issue của Onload mà không thấy** ai báo đúng lỗi `get_rxfh_key_size`; chỉ có
các báo cáo "hardware init failed" trên card ảo (vmxnet3 #257, virtio-net #270).

**Mở lại khi nào:** khi máy đo có một NIC mà driver vừa có thao tác khoá RSS (`get_rxfh_key_size`,
`get_rxfh_indir_size`) vừa có AF_XDP zero-copy — ví dụ `igb` trên kernel ≥ 7.3 — **và** một bản
Onload biên dịch được trên kernel đó (README của Onload hiện ghi tới 7.0). Kiểm trước, không cần cài
gì: `ethtool -x <nic>` phải in được khoá RSS, không phải `Operation not supported`.

**6.2, 6.3, 6.5 — không dựng (Sửa 2).** Các dòng `FIXBOLT_BYPASS` trong `check-machine.sh`, nhánh
`BYPASS=onload` của `w2w-baseline.sh`, và `check-wire-under-onload.sh` chỉ có nghĩa khi có một stack
Onload để đo; không có stack thì chúng là những cổng không ai đọc. Thiết kế của chúng vẫn nằm trong
ADR-0200 (quyết định 1, 2, 5) cho lần mở lại.

**6.4 — không dựng (Sửa 2).** `bypass-verdict.sh` chỉ có một đầu vào là nhánh Onload. Vạch bỏ của
`io_uring` có hình khác (≥ 3 % trên dòng `wire`, **hoặc** ≥ 25 % ở `turn` N = 16, ADR-0098 mục 1) và
thuộc plan hàng 5; nếu hàng 5 muốn một script phán quyết thì dựng ở đó, cho đúng vạch của nó. Số học
của vạch Onload vẫn ghi ở ADR-0200 quyết định 3.

**6.6 — gỡ Onload và ghi lại việc bỏ.**

- Gỡ khỏi máy bàn ngay (ADR-0202 quyết định 4 nói "sau boot", nay không còn boot nào cần nó):
  `sudo -n ~/src/onload/scripts/onload_uninstall`; kiểm `lsmod` không còn `onload`/`sfc_resource`,
  `/etc/modprobe.d/onload.conf` (thăm dò thấy file này) không còn; `ethtool -l enp9s0` combined 2
  (đã trả lại). `~/src/onload` giữ lại được — ngoài repo.
- `docs/reference/measured-costs.md`: mục *Onload over AF_XDP on the I211 — dropped at the probe, no
  pair*: hai dòng lỗi nguyên văn, commit của hai lần, kernel, lệnh; và phép tính 86 % ghi rõ *dự
  đoán, chưa thử*. ADR-0098 định nghĩa "bỏ" bằng *cặp số đã làm nó bị bỏ* — ở đây không có cặp nào,
  và trên kernel này không thể có; mục ghi đúng như vậy.
- `docs/reference/onload-af-xdp-needs-rss-key-ops-the-igb-driver-lacks.md` (bẫy mới). **Test canh nó
  là chính bước thăm dò G1** (các lệnh của 6.1, chạy lại được), cộng một kiểm tra trước một dòng
  không cần cài gì: `ethtool -x <nic>` in `RSS hash key: Operation not supported` → Onload AF_XDP sẽ
  hỏng lúc khởi tạo NIC.
- `docs/hft-playbook.md`: một đoạn — Onload-trên-AF_XDP không chạy được với `igb` trên kernel ≤ 7.2;
  kiểm tra trước ở trên; điều kiện mở lại.
- `docs/PRD.md` §2 *Phase 4*: hạng mục bypass — *bỏ 2026-09-24 ở bước thăm dò*.
- ADR-0098 và ADR-0099: mỗi cái thêm **một dòng status** ghi kết quả (ADR-0098 quyết định: "the ADR
  that let it in is marked with the result"); ADR-0200/0201/0202 đã ghi (Sửa 2).
- `STATUS.md`: hạng mục đóng; *Not proven*: phép tính 86 % chưa bao giờ được đo.

### Hàng 7 — boot §9 cho `io_uring` và cặp store (hai PR: 7a chuẩn bị, 7b chạy boot)

**Sửa 2:** không còn khối Onload. Boot đo **A/B `io_uring`** theo plan hàng 5
(`docs/plans/2026-09-24-p4-io-uring-transport.md`) và **cặp `w2w` store SQLite vs `FileJournal`
`Async`** theo plan hàng 4, mục *4b* (`docs/plans/2026-09-24-p4-sqlite-store.md`, ghi rõ "đi cùng boot
của hàng 7"). ADR-0201 không còn áp dụng: NIC giữ `combined 2` (đúng cấu hình của con số §6),
generator điều khiển qua cáp như trước.

**7a — trước reboot (một phiên):**

1. `scripts/boot-p4.sh` — driver chạy trọn boot không cần người (ADR-0202 quyết định 2): trước mỗi
   khối đọc `FIXBOLT_NIC=enp9s0 scripts/check-machine.sh`, dừng khi có dòng đỏ, lưu mọi output vào
   `target/boot-p4-evidence/`. Thứ tự (ADR-0202 quyết định 3, bỏ khối Onload):
   - **Procedure 1**: khối `io_uring` (theo plan hàng 5: control và `io_uring`, `hft` có dấu NIC
     `WIRE_NIC=enp9s0 OBSERVER_CORE=7`, rồi `standard` chỉ bảng phía Mac); rồi cặp store
     (`--journal file-async` rồi `--journal sqlite-async`).
   - **Giữa hai procedure** (≥ 30 phút, ADR-0068): vòng `ab-rotation.sh` cho `turn.rs` N = 1, 16, 64
     và `density.rs`, control vs `io_uring`, 20 vòng.
   - **Procedure 2**: cặp store đảo thứ tự, rồi khối `io_uring` đảo thứ tự.
   - Cuối: `check-machine.sh` lần cuối.
   **Điểm phải dừng và báo:** plan hàng 4 *4b* đòi `standard:app` "NIC có hardware timestamp", nhưng
   `w2w-baseline.sh` từ chối `WIRE_NIC` cho arm `standard` (Q10; dấu TX đánh thức engine đang chờ —
   `docs/reference/a-transmit-timestamp-wakes-a-blocking-engine.md`). Driver không tự chọn: manager
   đưa chỗ lệch này về architect của hàng 4 trước 7a.1.
2. Diễn tập driver trên dòng desktop với `RUNS=2` — output ghi *không phải số đo*.
3. Build sẵn (ADR-0090 quyết định 2): một worktree cho mỗi bộ feature dưới `../fb-p4-boot/` ở commit
   merge của 7a — `control` (`affinity`), `uring` (`affinity,io-uring`), `sqlite`
   (`affinity,sqlite`) — cờ `RUSTFLAGS` như `bench.sh`, bench `turn`/`density`; `MANIFEST.txt` ghi
   sha256; `scripts/check-bench-alignment.sh` đọc lại. `w2w` của Mac build ở cùng commit, ghi sha256.
4. Kiểm trước reboot: `ip -br link show enp9s0` có carrier; `ssh -o BatchMode=yes
   thangtran@192.168.77.2 true` thành công; Mac không ngủ (`pmset -g`, đọc; đổi là việc của anh).
   **Hai chặn hiện tại ở *Rủi ro*.**
5. Handoff: `STATUS.md` *Start here* (hành động đầu tiên, danh sách *đừng làm*, cái chưa chứng minh)
   + *Nhật ký giao hàng* của plan này, cùng commit; PR 7a merge, **ghi CI run id**; chờ run đó xong
   rồi mới làm gì tiếp (§8).
6. Đổi grub (ADR-0202 quyết định 5): kiểm `/etc/default/grub` giống `grub.fixbolt-desktop-20260922`
   (khác thì lưu bản mới có ngày), `sudo -n cp /etc/default/grub.fixbolt-s9-bootf-20260923
   /etc/default/grub && sudo -n update-grub`, `grep CMDLINE /etc/default/grub` thấy `isolcpus` và
   **không** thấy `nohz_full`. Báo anh: **một** lần reboot. `sudo -n reboot`. Hết phiên.

**7b — hành động đầu tiên của boot (phiên mới):**

1. Kiểm handoff: nhánh, commit có trên `main`, CI run id xanh. Không khớp → coi là cũ, đọc lại
   `STATUS.md` từ mục mới nhất.
2. `grep -o 'isolcpus=[^ ]*' /proc/cmdline` → `isolcpus=6,7,14,15`; `lsmod | grep -E
   '^(onload|sfc_resource)'` rỗng.
3. Đặt runtime: `sudo -n fixbolt-machine on`; dừng (không `disable`) `apt-daily`,
   `apt-daily-upgrade`, `fwupd-refresh`, `man-db`, `update-notifier-motd`, `packagekit.service`; IRQ
   của `enp9s0` (đọc số từ `/proc/interrupts`) → `0-5`; `sudo -n ethtool -C enp9s0 rx-usecs 0`;
   `sudo -n ethtool --set-eee enp9s0 eee off`, chờ `Link detected: yes`.
4. `FIXBOLT_NIC=enp9s0 scripts/check-machine.sh` → `pass 17 fail 0 unknown 0`, quote nguyên văn.
5. Một lần chạy bỏ đi (lần đầu sau reboot luôn bị gnome-shell làm bẩn).
6. Kiểm `MANIFEST.txt` (sha256), rồi chạy `scripts/boot-p4.sh` nền; **không gọi tool nào** cho tới khi
   nó thoát (mỗi lần gọi tốn một vòng — boot E).
7. Phán quyết: `scripts/compare-w2w-procedures.sh …`; phán quyết `io_uring` theo plan hàng 5; phán
   quyết store theo plan hàng 4; quote nguyên văn.
8. Áp phán quyết (ADR-0098: bỏ = gỡ code trên cùng nhánh, ghi cặp số vào `measured-costs.md`, ghi kết
   quả lên dòng trạng thái của ADR đã cho nó vào): `io_uring` bỏ → senior developer gỡ feature
   `io-uring`; giữ → theo plan hàng 5. Store theo plan hàng 4.
9. Dọn: dòng grub desktop theo lệnh của anh lúc đó, `STATUS.md` handoff, senior review, CI, merge.

### Giao diện cần từ hàng 5 (`io_uring`) — chỉ chừng này

- Feature Cargo `io-uring` trên `fixbolt-engine`, và `tools/w2w` có feature cùng tên chuyển tiếp nó.
- `w2w` có cờ chọn transport (ví dụ `--transport kernel|uring`, mặc định `kernel`), bị từ chối khi
  build thiếu feature, và nửa engine **in lại transport đọc từ engine** (một dòng `transport:` như
  dòng `tls:`), để `w2w-baseline.sh` kiểm được như đang kiểm `journal:`.
- Các ca `turn.rs` / `density.rs` cho transport mới có **tên riêng**, để `ab-rotation.sh` ghép cặp
  theo tên với ca control.
- Nếu có nhánh SQPOLL (`hft`): một cờ riêng nhận lõi, và driver chạy nó như một arm thêm.

Nếu plan hàng 5 chọn tên khác, 7a dùng tên đó; nếu thiếu một trong bốn thứ trên, 7a **dừng và báo**.

## Bất biến bị đụng tới

Sau Sửa 2, hàng 6 **không đụng `crates/` và không thêm script đo nào** — chỉ gỡ Onload khỏi máy bàn và
tài liệu. Hàng 7 không đụng `crates/`; các bất biến mà `io_uring` và store chạm tới do plan hàng 5 và
hàng 4 giữ. Riêng hàng 7 giữ:

- **4 (ngủ/spin theo mode)**: mọi con số của boot ghi mode; `standard` không chạy với `WIRE_NIC`.
- **10 (số đo)**: mọi con số kèm lệnh, máy, output `check-machine.sh` của khối đó; không build lại
  giữa boot (`MANIFEST.txt`).
- 1, 2, 3, 5, 6, 7, 8, 9: không đụng ở hai hàng này.

## Chia việc

Mỗi hàng một commit xanh; manager chạy lại gate và commit.

| Bước | Kết quả — file tạo / sửa (và **không** đụng) | Người làm | Gate | Xong khi | Reversal | Phụ thuộc |
|---|---|---|---|---|---|---|
| 6.0 | Plan + ADR được duyệt; Q1, Q2 (**xong 2026-09-24**: Q1 = A, Q2 = đồng ý cả ba) | anh | — | có câu trả lời | — | phase 3 đóng bằng tag `v0.1.0` (ADR-0161) |
| 6.1 | **Cài Onload + thăm dò — xong, G1 hỏng hai lần** (`v9.0.2`: biên dịch; `174b947d0b`: `hardware init failed rc=-95`). Không file repo nào; output ở `target/p4-probe/` | developer (sonnet) | `git merge-base --is-ancestor 268f1d4c8a HEAD` in `has-sockaddr-unsized-fix`; các lệnh ở *Cách làm* 6.1 | **xong**: G1 FAIL → Onload bỏ | — | 6.0 |
| 6.2 | ~~Dòng `FIXBOLT_BYPASS` trong `check-machine.sh`~~ — **không dựng** (Sửa 2: không có stack để kiểm) | — | — | — | — | — |
| 6.3 | ~~Nhánh `BYPASS=onload` của `w2w-baseline.sh`~~ — **không dựng** (Sửa 2) | — | — | — | — | — |
| 6.4 | ~~`bypass-verdict.sh`~~ — **không dựng** (Sửa 2: đầu vào duy nhất là nhánh Onload; vạch `io_uring` thuộc hàng 5) | — | — | — | — | — |
| 6.5 | ~~`check-wire-under-onload.sh`~~ — **không dựng** (Sửa 2) | — | — | — | — | — |
| 6.6a | **Gỡ Onload khỏi máy bàn.** Không file repo nào | runner (haiku) — lệnh tự đủ | `sudo -n ~/src/onload/scripts/onload_uninstall`; `lsmod \| grep -cE '^(onload\|sfc_resource)'` → `0`; `test ! -e /etc/modprobe.d/onload.conf && echo gone`; `ethtool -l enp9s0` combined 2 | ba output quote nguyên văn | — | 6.1 |
| 6.6b | **Ghi lại việc bỏ.** `docs/reference/measured-costs.md` (mục mới), `docs/reference/onload-af-xdp-needs-rss-key-ops-the-igb-driver-lacks.md` (mới), `docs/hft-playbook.md`, `docs/PRD.md` §2, một dòng status ở ADR-0098 và ADR-0099, `STATUS.md` (manager). **Không đụng** `scripts/`, `crates/`, `DESIGN.md` §8/§9 | developer (sonnet); manager viết `STATUS.md` | `python3 scripts/check-links.py`; `scripts/check-adr-numbers.sh`; `ethtool -x enp9s0 \| grep -A1 'RSS hash key'` in `Operation not supported` (bằng chứng cho kiểm tra trước ghi trong file bẫy) | file bẫy nêu tên test canh nó (bước thăm dò G1 + kiểm tra `ethtool -x`); hai dòng lỗi nguyên văn trong `measured-costs.md` | — | 6.6a |
| 6.7 | Senior review PR hàng 6, CI xanh, merge | senior developer (opus) | các gate trên + CI | CI run id của commit đóng được ghi | — | 6.6b |
| 7a.0 | **Gỡ hai chặn**: Mac nhận lại khoá ssh của máy bàn; cáp `enp9s0` có carrier | **anh** | `ssh -o BatchMode=yes thangtran@192.168.77.2 true`; `cat /sys/class/net/enp9s0/carrier` → `1` | cả hai đạt | — | — |
| 7a.1 | `scripts/boot-p4.sh` (mới), khối `io_uring` + cặp store, diễn tập `RUNS=2` | **senior developer (opus)** — nối giao diện của hàng 4 và 5, sai thì hỏng cả boot | diễn tập chạy hết mọi khối, có `target/boot-p4-evidence/`; `bash -n` | output diễn tập ghi *không phải số đo* | cho `check-machine.sh` một dòng đỏ giả (bật lại EEE) → driver dừng trước khối đầu, nêu tên dòng | hàng 4 (tới bước 6 của nó) và hàng 5 merge; 6.7; 7a.0 |
| 7a.2 | Build sẵn `../fb-p4-boot/{control,uring,sqlite}`, `MANIFEST.txt`, `w2w` của Mac | runner (haiku) — brief tự đủ, lệnh chép từ ADR-0090 quyết định 2 | `sha256sum -c MANIFEST.txt`; `scripts/check-bench-alignment.sh` | mọi dòng `OK` | — | 7a.1 |
| 7a.3 | Kiểm trước reboot, handoff, merge 7a, đổi grub, reboot | manager | `grep CMDLINE /etc/default/grub` | CI run id ghi trong handoff; `isolcpus` có, `nohz_full` không | — | 7a.2 |
| 7b.1 | Hành động đầu tiên của boot (7b bước 1–5) | manager | `check-machine.sh` bước 4 | `pass 17 fail 0 unknown 0` | — | reboot |
| 7b.2 | Chạy `scripts/boot-p4.sh` | manager chạy; runner (haiku) trích output sau khi xong | output driver | driver thoát 0, hoặc dừng ở dòng đỏ đã nêu tên | — | 7b.1 |
| 7b.3 | Phán quyết + áp phán quyết `io_uring` và store (`measured-costs.md`, `DESIGN.md` §8 nếu có dòng mới, dòng kết quả ADR, `PRD.md`); gỡ `io-uring` nếu bỏ | manager (tài liệu); senior developer (opus) gỡ code | `scripts/compare-w2w-procedures.sh`; phán quyết theo plan hàng 4, 5; `cargo test --all`, `--no-default-features`, clippy | mỗi cặp số kèm lệnh, máy, `check-machine.sh` | — | 7b.2 |
| 7b.4 | Dọn (grub), senior review, CI, merge, `STATUS.md` | manager; senior developer (opus) review | CI | CI run id của commit đóng | — | 7b.3 |

6.6a đi trước 6.6b vì file bẫy trích trạng thái sau khi gỡ. 7a.0 là việc của anh và chặn mọi bước
7a có Mac.

## Cách kiểm chứng

| # | Tiêu chí | Lệnh | Đạt khi |
|---|---|---|---|
| 1 | Onload bị bỏ có bằng chứng | `target/p4-probe/probe-v9_2.txt`, `onload_install.txt` | hai dòng lỗi nguyên văn được chép vào `measured-costs.md` và ADR-0201 *Result* |
| 2 | Kiểm tra trước của bẫy đúng | `ethtool -x enp9s0 \| grep -A1 'RSS hash key'` | `Operation not supported` |
| 3 | Onload đã rời máy bàn | `lsmod`, `/etc/modprobe.d/onload.conf` | không module, không file |
| 4 | Máy đúng §9 cho từng khối của boot | output `check-machine.sh` mà driver lưu trước mỗi khối | không dòng đỏ |
| 5 | Hai procedure tái lập | `scripts/compare-w2w-procedures.sh` | theo ADR-0068; không tái lập vẫn công bố cặp, đánh dấu |
| 6 | Không build lại giữa boot | `sha256sum -c MANIFEST.txt` trước và sau | `OK` cả hai lần |
| 7 | Vạch `io_uring`, vạch store | theo plan hàng 5, hàng 4 | như hai plan đó viết |

"Test pass" chưa đủ: số của boot đo trên máy bàn §9, qua cáp thật tới Mac, output
`check-machine.sh` đi kèm từng khối.

## Tài liệu phải cập nhật

- [ ] `docs/reference/measured-costs.md` — mục Onload bị bỏ ở bước thăm dò, không có cặp (6.6b); các
      cặp của boot, kể cả cặp làm hạng mục bị bỏ (7b.3)
- [ ] `docs/reference/onload-af-xdp-needs-rss-key-ops-the-igb-driver-lacks.md` — bẫy mới, canh bởi
      bước thăm dò G1 và kiểm tra `ethtool -x` (6.6b)
- [ ] `docs/hft-playbook.md` — Onload-trên-AF_XDP không chạy với `igb` ≤ 7.2; điều kiện mở lại (6.6b)
- [ ] `docs/PRD.md` §2 *Phase 4* — bypass bỏ (6.6b); kết quả `io_uring`, store (7b.3)
- [ ] ADR-0098, ADR-0099 — một dòng status mỗi cái (6.6b); ADR-0200/0201/0202 — đã ghi Sửa 2
- [ ] `docs/DESIGN.md` §9 — **không** thêm dòng bypass (không có boot bypass); §8 — theo phán quyết
      `io_uring`/store (7b.3)
- [ ] `docs/GUIDE.md`, `docs/best-practices-hft.md` — không đổi vì Onload (không có gì để khuyên)
- [ ] `STATUS.md` — 6.7, handoff 7a (trước reboot), 7b (sau boot); *Not proven*

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| **Onload AF_XDP đòi thao tác khoá RSS mà `igb` (kernel ≤ 7.2) không có** — đã gặp | bước thăm dò G1; kiểm tra trước `ethtool -x <nic>` |
| Onload `v9.0.2` không biên dịch trên kernel ≥ 6.19 (`sockaddr_unsized`) — đã gặp | cổng `git merge-base --is-ancestor 268f1d4c8a HEAD` trước khi build |
| Đăng ký hỏng để lại trạng thái kẹt (`register` → `EALREADY`, `unregister` → `EBUSY`) — đã gặp | `onload_tool unload`, rồi `lsmod` rỗng (6.6a) |
| Onload tự nạp lúc khởi động (`/etc/modprobe.d/onload.conf`) → boot đo trên kernel có module lạ | 6.6a gỡ; 7b bước 2 `lsmod` rỗng |
| Build lại giữa boot → layout đổi | `MANIFEST.txt` sha256 trước/sau (ADR-0090) |
| Gọi tool giữa lúc driver chạy → máy không yên, mất vòng | không gọi tool cho tới khi driver thoát (7b bước 6) |
| Lần chạy đầu sau reboot bẩn | 7b bước 5 bỏ đi một lần |
| Dùng `grub.fixbolt-s9` (có `nohz_full`) | 7a bước 6 `grep` không thấy `nohz_full` |
| Arm `standard` với `WIRE_NIC` (plan hàng 4 *4b*) — script từ chối | 7a.1 dừng và báo trước khi viết driver |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| **Chặn hàng 7 — cáp không có carrier** `[2026-09-24]`: `enp9s0` `NO-CARRIER` kể cả sau khi bật link và sau khi nạp lại module `igb` (khởi tạo lại PHY phía máy bàn) → đầu Mac đang tắt hoặc ngủ | **Chặn** | **chỉ anh**: kiểm Mac (nguồn, ngủ, cáp); đạt khi `cat /sys/class/net/enp9s0/carrier` → `1` (7a.0) |
| **Chặn hàng 7 — Mac từ chối khoá của máy bàn** `[2026-09-24]`: `192.168.77.2` trả lời ping nhưng ssh báo `Permission denied (publickey,password,keyboard-interactive)`; khoá được đưa ra là `SHA256:70H/+HuFNSlCQtlqXCpf4IR4zyqLiPHojYpgjBeWQvA` (`tmt@tmt-B450-fixbolt-direct`) | **Chặn** | **chỉ anh**: thêm lại khoá công khai đó vào `~/.ssh/authorized_keys` của `thangtran` trên Mac; đạt khi `ssh -o BatchMode=yes thangtran@192.168.77.2 true` thành công (7a.0). Không có generator thì diễn tập 7a.1 và cả boot đều dừng |
| Mac ngủ giữa đêm | Trung bình | kiểm `pmset` trước reboot; driver dừng khi generator lỗi, không chạy tiếp vô ích |
| Plan hàng 4 và hàng 5 đòi điều `w2w-baseline.sh` không cho (ví dụ `standard` + `WIRE_NIC`) | Trung bình | 7a.1 dừng và báo; manager đưa về architect của plan đó |
| Boot dài, hỏng sớm là mất phần còn lại | Trung bình | driver lưu từng khối; dừng ở dòng đỏ đầu tiên |
| Hàng 4 hoặc 5 chưa merge khi hàng 6 xong | Trung bình | 7a chờ cả hai; hàng 6 không phụ thuộc chúng |
| `~/src/onload` và gói `gawk`/`libcap-dev`/`libmnl-dev` còn trên máy bàn | Thấp | ngoài repo, không nạp gì; ghi trong `STATUS.md` |

## Anh cần quyết

> **Đã quyết 2026-09-24** (anh trả lời trong hội thoại), cả hai câu theo khuyến nghị:
> **Q1** A — **đo**: cài Onload, thăm dò (6.1), làm hàng 6, đo trong boot của hàng 7.
> **Q2** đồng ý cả ba cách đọc: (a) p99 Onload ≤ p99 twin × 1,05; (b) phải đạt ở **cả hai path**
> (`admin`, `app`) trong **cả hai procedure**; (c) 59/59 dưới `onload` = bộ test `wire` chạy qua
> loopback đã tăng tốc (`EF_TCP_*_LOOPBACK=1`), đường AF_XDP qua cáp chứng minh bằng các lần chạy `w2w`.
> Ba cách đọc này đã được gộp vào ADR-0200 quyết định 3 và 5 (sửa tại chỗ, ghi lần sửa — ADR vẫn
> *Proposed*). Phần còn lại của plan do manager duyệt theo uỷ quyền 2026-09-18.
> Phase 3 đóng bằng tag `v0.1.0` (ADR-0161), không publish lên crates.io.

**Q1. Onload: đo trong boot, hay bỏ ngay dựa trên phép tính?**

- **A (khuyến nghị) — đo.** Cài Onload, thăm dò (6.1), làm hàng 6, đo trong boot vốn phải chạy cho
  `io_uring`. Tốn: một PR script, module ngoài cây trên máy anh vài ngày, ~1–1.5 giờ của boot (nửa
  nếu bỏ ngay ở procedure 1). Được: một **cặp số đo thật** thay cho dự đoán — đúng cái ADR-0099 dành
  chỗ cho kết quả âm — và ba câu hỏi chưa ai trả lời (zero-copy có bind trên I211 không, bộ lọc có
  đi vòng được không, Onload có cấp phát trên luồng engine không).
- **B — bỏ ngay.** Ghi vào `measured-costs.md` phép tính 86 % với nguồn *C-40*, đánh dấu Onload là
  *bỏ theo phép tính, chưa đo*. Tốn: không gì; hàng 6 thu lại thành một commit tài liệu, hàng 7 chỉ
  đo `io_uring`. Mất: ADR-0098 định nghĩa "bỏ" là *cặp số đã làm nó bị bỏ* — B không có cặp nào, nên
  kết quả là một suy luận, không phải số đo.

**Q2. Đọc vạch bỏ thế nào** (ADR-0200 quyết định 3 và 5) — ba chỗ ADR-0098 viết chưa đủ để tính:

- (a) "p99 không tệ hơn" = p99 Onload ≤ p99 twin × 1.05 (dải tái lập 5 % của ADR-0068, cùng dải
  ADR-0098 dùng cho `io_uring`). *Khuyến nghị: đồng ý.*
- (b) Phải đạt ở **cả hai path** (`admin` và `app`), cả hai procedure. *Khuyến nghị: đồng ý.*
- (c) "59/59 dưới `onload`" = bộ test `wire` chạy qua TCP của chính Onload (loopback tăng tốc,
  `EF_TCP_*_LOOPBACK=1`), còn đường AF_XDP qua cáp được chứng minh bằng các lần chạy `w2w` (Logon,
  Heartbeat, `35=D`/`35=8` thật, generator kiểm từng `35=`). Chạy 59 định nghĩa **qua cáp** cần một
  harness conformance tách hai máy — code mới, không có trong plan. *Khuyến nghị: đồng ý (c); harness
  qua cáp chỉ làm nếu Onload được giữ.*

## Ngoài phạm vi

- Mọi code trong `crates/`; transport AF_XDP riêng; stack TCP riêng; `ef_vi`; DPDK (ADR-0098 §7).
- Harness conformance tách hai máy qua cáp (Q2 c).
- `standard` dưới Onload (ADR-0200 quyết định 6).
- Chứng minh luồng engine không ngủ dưới Onload — không còn nợ (Onload bị bỏ).
- **Sửa 2:** mọi dụng cụ đo riêng cho Onload (6.2–6.5), khối Onload của boot, và thử lại Onload trên
  kernel ≥ 7.3 — chỉ khi điều kiện mở lại ở *Cách làm* 6.1 xảy ra, bằng một plan mới.
- Dấu thời gian phần cứng phía máy bàn cho nhánh Onload (không có đường nào trên `igb`).
- Thiết kế và code `io_uring` (hàng 5); hàng 7 chỉ chạy phép đo của nó.
- Tắt EEE phía Mac (cần sudo của anh trên Mac; tắt một đầu là đủ cho cả link — bộ nhớ 2026-09-14).

## Nhật ký giao hàng

- `[2026-09-24]` 6.1 lần 1: Onload `v9.0.2` (`9f330e7058`) build hỏng trên `7.0.0-31` —
  `af_xdp.c:375:28 … kernel_bind … expected ‘struct sockaddr_unsized *’`; không gì được cài hay nạp,
  NIC không đổi; log `target/p4-probe/onload_install.txt` (trên desk, không commit). Sửa 1: ghim
  `174b947d0b` (nhánh `v9_2`, không tag). Chặn mới: Mac từ chối khoá ssh của máy bàn (xem *Rủi ro*).

- `[2026-09-24]` 6.1 lần 2: Onload `174b947d0b` (nhánh `v9_2`) — cổng `merge-base` in
  `has-sockaddr-unsized-fix`, `onload_install: Install complete.`, module nạp được; đăng ký `enp9s0`
  hỏng: `` af_xdp_rss_get_support: enp9s0 does not support `get_rxfh_key_size` operation `` / `hardware
  init failed rc=-95`, y hệt với bộ lọc tắt và `combined 1`; `register` → errno 114, `unregister` →
  errno 16; module gỡ bằng `onload_tool unload`, `combined 2` trả lại. **G1 FAIL → Onload bỏ** (Q1 của
  ADR-0098). Output `target/p4-probe/{onload_install-v9_2.txt, probe-v9_2.txt, after-v9_2.txt}`.
  **Sửa 2**: 6.2–6.5 không dựng, 6.6 thành gỡ Onload + ghi việc bỏ, hàng 7 chỉ đo `io_uring` và cặp
  store. Còn lại trên máy: `/etc/modprobe.d/onload.conf` (gỡ ở 6.6a). Chặn hàng 7: `enp9s0`
  `NO-CARRIER` và Mac từ chối khoá ssh.

*(Điền tiếp khi từng hàng đóng: đã dựng gì, ở đâu, gate nào xanh, CI run id, cái chưa làm
và vì sao. Handoff trước reboot (7a) và sau boot (7b) ghi ở đây và ở `STATUS.md` cùng commit.)*
