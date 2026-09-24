# Phase 4, hàng 6 và 7: dụng cụ đo cho Onload trên AF_XDP, và boot §9 duy nhất đo cả `io_uring` lẫn Onload

> **Loại:** Plan · **Ngày:** 2026-09-24 · **Trạng thái:** Đề xuất
> **Phạm vi:** hàng 6 và 7 của bảng *Chia việc* trong [phase-4-scope](2026-09-23-phase-4-scope.md);
> kèm [ADR-0200](../decisions/ADR-0200-a-bypass-arm-is-judged-from-the-counterparty-against-a-same-boot-kernel-twin-and-its-kill-line-is-arithmetic-written-first.md),
> [ADR-0201](../decisions/ADR-0201-onload-on-the-i211-runs-without-hardware-flow-filters-one-channel-count-holds-for-the-boot-and-the-control-path-leaves-the-cable.md),
> [ADR-0202](../decisions/ADR-0202-phase-4s-one-s9-boot-is-pre-built-driven-by-a-committed-script-and-onload-lives-only-inside-its-block.md)

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Tóm tắt một đoạn

Hàng 6 dựng **dụng cụ đo** cho nhánh Onload: các dòng §9, các dòng kiểm trong
`scripts/check-machine.sh`, cách chạy `w2w` hai nhánh (kernel và Onload) đo từ phía Mac, và một
script tính phán quyết giữ/bỏ. Hàng 7 là **một boot §9 duy nhất** đo A/B `io_uring` (của hàng 5) và
A/B Onload, rồi áp phán quyết. **Phát hiện quan trọng nhất khi đọc số cũ:** thời gian khứ hồi đo từ
Mac là ~232 µs, trong đó toàn bộ phần của máy bàn (từ lúc gói vào NIC tới lúc gói ra NIC) chỉ ~27 µs.
Muốn p50 phía Mac tốt hơn 10 % thì Onload phải cắt ~23 µs — tức 86 % của *toàn bộ* phần máy bàn, kể
cả phần phần cứng NIC và phần việc của chính engine mà Onload không đụng tới được. **Dự đoán: Onload
sẽ bị bỏ.** Plan đề xuất vẫn đo (một boot vốn đã phải chạy cho `io_uring`), nhưng anh có thể chọn bỏ
ngay dựa trên phép tính này (câu hỏi Q1).

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

**6.1 Cài Onload và thăm dò, trên dòng grub desktop, không phải số đo.** Cài từ tag `v9.0.2` (ghi
commit sha) vào `~/src/onload` — **ngoài repo, không gì vào git**: `sudo -n apt-get install -y gawk
libcap-dev libmnl-dev`, `git clone https://github.com/Xilinx-CNS/onload ~/src/onload`,
`git -C ~/src/onload checkout v9.0.2`, `sudo -n ~/src/onload/scripts/onload_install --no-sfc`,
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

**6.2 Dòng kiểm trong `scripts/check-machine.sh`.** Biến mới `FIXBOLT_BYPASS`, ba giá trị; **không
đặt thì không in thêm dòng nào** (khuôn `FIXBOLT_NIC`):

| `FIXBOLT_BYPASS` | Dòng | PASS khi |
|---|---|---|
| `absent` | `bypass modules` | không có module `onload` / `sfc_resource` nào đang nạp |
| `absent`, `twin` | `no xdp on nic` | `bpftool net show dev $NIC` (hoặc `ip -d link`) không có chương trình XDP |
| `twin`, `onload` | `onload version` | bản userland (`onload --version`) khớp bản module đang nạp |
| `twin` | `afxdp register` | NIC **chưa** đăng ký (đường sysfs lấy từ 6.1) |
| `onload` | `afxdp register` | NIC **đã** đăng ký và bật |
| `onload` | `af_xdp flow filters` | giá trị `enable_af_xdp_flow_filters` bằng giá trị bước 6.1 quyết định |
| `onload` | `xdp mode` | `xdpdrv` (native); `xdpgeneric` là FAIL — *chỉ khi 6.1 cho thấy chương trình được gắn lúc đăng ký; nếu chỉ gắn khi có stack thì dòng này chuyển sang đọc lại mỗi lần chạy (6.3)* |
| cả ba | `nic channels` | `ethtool -l` combined bằng `FIXBOLT_CHANNELS`; thiếu biến đó là UNKNOWN |

Phần quyết định là **hàm thuần** (nhận output lệnh, trả `PASS|FAIL|UNKNOWN<TAB>giá trị`) và có ca
trong `scripts/check-machine-verdicts.sh` — gồm output thật chép từ bước 6.1.

**6.3 Nhánh Onload trong `scripts/w2w-baseline.sh`.** Biến mới `BYPASS=onload` (mặc định rỗng →
không đổi gì). Khi bật:

- nửa `--listen` chạy dưới `onload -p latency` với `EF_NO_FAIL=0 EF_AF_XDP_ZEROCOPY=1
  EF_USE_HUGE_PAGES=0`; header in mọi biến `EF_*` và bản `onload_stackdump` đọc tùy chọn của stack;
- `WIRE_NIC` bị **từ chối** trước khi chạy (ADR-0200 quyết định 1);
- sau dòng `listening:` và **trước** khi khởi động generator: chờ `carrier` của NIC lên (nếu 6.1 thấy
  link nhảy), đọc `sudo -n ss --xdp -a -e` → phải có XSK trên `ifindex` của NIC với `zc:1`;
- đọc `Tcp: InSegs/OutSegs` trong `/proc/net/snmp` trước generator và sau khi nửa listen thoát →
  tăng **< 1 % số request**;
- vi phạm bất kỳ điều nào → **FAIL** cả nhánh (không phải loại một lần chạy); `allocs 0` giữ nguyên;
- khối tóm tắt ghi nhãn `bypass: onload <version> zc` để không ai lẫn nó với nhánh kernel;
- nhánh twin chạy **cùng script, không `BYPASS`, không `WIRE_NIC`**, cùng `GENERATOR_SSH`.

Phần đọc `zc:1` và phần so bộ đếm là **hàm thuần**, có ca trong
`scripts/check-w2w-baseline-summary.sh`. Dòng `sudo -n ss` phải qua
`scripts/check-sudo-names-what-root-can-find.sh` (ADR-0093).

**6.4 Script phán quyết.** `scripts/bypass-verdict.sh <twin P1> <onload P1> [<twin P2> <onload P2>]`
đọc bốn (hoặc hai) tóm tắt của `w2w-baseline.sh`, in từng mệnh đề cho từng path, từng procedure, và
một dòng cuối `verdict: KEEP` / `verdict: DROP (<mệnh đề>, <path>, procedure <n>)`. Số học đúng như
ADR-0200 quyết định 3. Test `scripts/check-bypass-verdict.sh` có ca: giữ; bỏ vì p50; bỏ vì p99; bỏ vì
chỉ một path hụt; bỏ ngay ở procedure 1 khi chỉ có hai file; và một ca dựng từ số *C-40*.

**6.5 Bộ 59 định nghĩa dưới Onload.** `scripts/check-wire-under-onload.sh <wire-test-binary>`: chạy
binary test `wire` đã build sẵn dưới `onload` với `EF_TCP_SERVER_LOOPBACK=1
EF_TCP_CLIENT_LOOPBACK=1 EF_NO_FAIL=0`; đọc `Tcp: PassiveOpens` trước/sau, FAIL nếu tăng bằng số
kết nối bộ test mở; trên máy không có `onload` thì từ chối và nói rõ lý do.

**6.6 Tài liệu.** `DESIGN.md` §9 thêm các dòng boot bypass (bảng 6.2 + các `EF_*` in trong header);
`docs/hft-playbook.md` mục Onload-trên-AF_XDP (các bước của 6.1, ghi rõ *chưa đo*, và rằng trên
`igb` Onload chiếm trọn NIC); `docs/GUIDE.md` (bypass chỉ `hft`, chỉ plaintext, `standard` dưới
Onload không hỗ trợ); `docs/best-practices-hft.md`; ba bẫy mới trong `docs/reference/` (bên dưới),
mỗi cái chỉ tên test canh nó.

### Hàng 7 — boot §9 (hai PR: 7a chuẩn bị, 7b chạy boot)

**7a — trước reboot (một phiên):**

1. `scripts/boot-p4.sh` — driver chạy trọn boot không cần người (ADR-0202 quyết định 2): trước mỗi
   khối đọc `check-machine.sh` với `FIXBOLT_BYPASS` đúng khối, dừng khi có dòng đỏ, lưu mọi output
   vào `target/boot-p4-evidence/`. Thứ tự (ADR-0202 quyết định 3):
   - **Procedure 1**: khối `io_uring` — control và `io_uring`, `hft` admin + app **có** dấu NIC
     (`WIRE_NIC=enp9s0`, `OBSERVER_CORE=7`), rồi `standard` admin + app (chỉ bảng phía Mac); rồi
     khối Onload — twin trước, Onload sau (nạp module → đăng ký → chạy → gỡ đăng ký → gỡ module).
   - **Giữa hai procedure** (≥ 30 phút, ADR-0068): vòng `ab-rotation.sh` cho `turn.rs` N = 1, 16, 64
     và `density.rs`, control vs `io_uring`, 20 vòng.
   - **Procedure 2**: khối Onload trước (Onload rồi twin) — **chỉ khi** `bypass-verdict.sh` của
     procedure 1 không in `DROP`; rồi khối `io_uring` đảo thứ tự.
   - Cuối: `check-wire-under-onload.sh` (nếu Onload chưa bị bỏ), rồi `check-machine.sh` lần cuối.
2. Diễn tập driver trên dòng desktop với `RUNS=2` — output ghi *không phải số đo*.
3. Build sẵn (ADR-0090 quyết định 2): worktree `../fb-p4-boot/control` và `../fb-p4-boot/uring` ở
   commit merge của 7a, cờ `RUSTFLAGS` như `bench.sh`, `w2w` với `--features affinity` (+ `io-uring`
   ở cây `uring`), binary test `wire` (`--no-run`), bench `turn`/`density`; `MANIFEST.txt` ghi sha256;
   `scripts/check-bench-alignment.sh` đọc lại. `w2w` của Mac build ở cùng commit, ghi sha256.
4. Kiểm trước reboot: ssh tới Mac qua địa chỉ **không phải cáp** chạy được
   (`ssh -o BatchMode=yes thangtran@<địa chỉ> true`); Mac không ngủ (`pmset -g`, đọc; nếu phải đổi
   thì là việc của anh — agent không có sudo trên Mac); Onload không tự nạp khi khởi động.
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
2. `grep -o 'isolcpus=[^ ]*' /proc/cmdline` → `isolcpus=6,7,14,15`.
3. Đặt runtime: `sudo -n fixbolt-machine on`; dừng (không `disable`) `apt-daily`,
   `apt-daily-upgrade`, `fwupd-refresh`, `man-db`, `update-notifier-motd`, `packagekit.service`;
   `sudo -n ethtool -L enp9s0 combined 1` (nếu ADR-0201 giữ); chờ `Link detected: yes`; IRQ của
   `enp9s0` (đọc số từ `/proc/interrupts` — **đổi sau khi đổi số hàng đợi**) → `0-5`;
   `sudo -n ethtool -C enp9s0 rx-usecs 0`; `sudo -n ethtool --set-eee enp9s0 eee off`, chờ link.
4. `FIXBOLT_NIC=enp9s0 FIXBOLT_BYPASS=absent FIXBOLT_CHANNELS=1 scripts/check-machine.sh` → không
   dòng đỏ, quote nguyên văn.
5. Một lần chạy bỏ đi (lần đầu sau reboot luôn bị gnome-shell làm bẩn).
6. Kiểm `MANIFEST.txt` (sha256), rồi chạy `scripts/boot-p4.sh` nền; **không gọi tool nào** cho tới khi
   nó thoát (mỗi lần gọi tốn một vòng — boot E).
7. Phán quyết: `scripts/bypass-verdict.sh …`, `scripts/compare-w2w-procedures.sh …`, phán quyết
   `io_uring` theo plan hàng 5; quote nguyên văn.
8. Áp phán quyết (ADR-0098: bỏ = gỡ code trên cùng nhánh, ghi cặp số vào `measured-costs.md`, ghi kết
   quả lên dòng trạng thái của ADR đã cho nó vào):
   - Onload **bỏ**: cặp số (A/B, một procedure nếu bỏ ở procedure 1) vào `measured-costs.md` cạnh
     dự đoán; `DESIGN.md` §8 không thêm dòng; ADR-0098 và ADR-0200 thêm dòng kết quả.
   - Onload **giữ**: dòng thứ hai có nhãn trong `DESIGN.md` §8 cạnh twin cùng boot (ADR-0099 quyết
     định 2); playbook bỏ chữ *chưa đo*.
   - `io_uring` **bỏ**: senior developer gỡ feature `io-uring` trên cùng nhánh; **giữ**: theo plan
     hàng 5.
9. Dọn: `sudo -n ~/src/onload/scripts/onload_uninstall` (bất kể phán quyết — ADR-0202 quyết định
   4), `combined 2`, dòng grub desktop theo lệnh của anh lúc đó, `STATUS.md` handoff, senior review,
   CI, merge.

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

Hàng 6 và 7 **không đụng `crates/`** — chỉ script, tài liệu, và cách chạy.

- **1 (không cấp phát trên hot path)**: `w2w` vẫn khẳng định `allocs 0` trên cả hai nửa dưới
  `onload`; thư viện Onload cấp phát trên luồng engine là **bỏ** (cổng G3), không trừ ra.
- **3 (59/59)**: dưới `onload` bằng `check-wire-under-onload.sh` với loopback tăng tốc (ADR-0200
  quyết định 5, câu hỏi Q2); `io_uring` theo plan hàng 5.
- **4 (ngủ/spin theo mode)**: Onload chỉ `hft`; `standard` dưới Onload không đo, ghi *không hỗ trợ*
  trong `GUIDE.md`. Nếu Onload được giữ thì chứng minh luồng engine không ngủ dưới Onload là một
  mục **còn nợ** (ghi ở *Not proven*) — dự đoán là bỏ, nên không dựng trước.
- **6 (feature chặn `mod`)**: không feature mới ở hàng 6/7.
- **8 (`unsafe`)**: không có.
- **10 (số đo)**: các dòng §9 cho boot bypass có **trước** con số đầu tiên (6.2 + 6.6); mỗi con số
  của boot kèm lệnh, máy, output `check-machine.sh` với `FIXBOLT_BYPASS` của khối đó.
- 2, 5, 7, 9: không đụng.

## Chia việc

Mỗi hàng một commit xanh; manager chạy lại gate và commit. 6.2, 6.3, 6.4, 6.5 sửa file rời nhau nên
chạy song song sau 6.1.

| Bước | Kết quả — file tạo / sửa (và **không** đụng) | Người làm | Gate | Xong khi | Reversal | Phụ thuộc |
|---|---|---|---|---|---|---|
| 6.0 | Plan này + ADR-0200/0201/0202 được duyệt; anh trả lời Q1, Q2 | anh | — | có câu trả lời | — | — |
| 6.1 | **Cài Onload + thăm dò** (dòng desktop). Không file repo nào; output vào `target/p4-probe/` | developer (sonnet) — lệnh viết sẵn trong brief; build hỏng trên kernel 7.0 thì **dừng và báo** | các lệnh ở *Cách làm* 6.1, quote nguyên văn | có output cho G1–G5; manager xếp cổng và điền *Result* của ADR-0201 | — | 6.0 |
| 6.2 | `scripts/check-machine.sh` (khối `FIXBOLT_BYPASS`), `scripts/check-machine-verdicts.sh` (ca mới, có output thật của 6.1). **Không đụng** dòng nào đang có | developer (sonnet) | `scripts/check-machine-verdicts.sh`; `FIXBOLT_NIC=enp9s0 scripts/check-machine.sh` **không** đặt `FIXBOLT_BYPASS` → danh sách tên dòng giống hệt trước khi sửa (`grep -E '^(PASS\|FAIL\|\? \? \?)' \| cut -c8-30`, so với bản chụp trước) | ca mới xanh; 17 tên dòng cũ không đổi | đổi `xdpdrv` thành `xdpgeneric` trong ca fixture → ca `xdp mode` đỏ, nêu tên ca | 6.1 |
| 6.3 | `scripts/w2w-baseline.sh` (nhánh `BYPASS=onload`), `scripts/check-w2w-baseline-summary.sh` (ca `zc`, ca bộ đếm), danh sách của `scripts/check-sudo-names-what-root-can-find.sh`. **Không đụng** `tools/w2w` | developer (sonnet) | `scripts/check-w2w-baseline-summary.sh`; `scripts/check-sudo-names-what-root-can-find.sh`; `BYPASS=onload WIRE_NIC=enp9s0 … scripts/w2w-baseline.sh` → bị từ chối trước khi chạy | ca xanh; lời từ chối quote được | ca `ss` có `zc:0` → FAIL đúng tên; ca bộ đếm tăng 22 000 → FAIL | 6.1 |
| 6.4 | `scripts/bypass-verdict.sh`, `scripts/check-bypass-verdict.sh` (mới) | developer (sonnet) | `scripts/check-bypass-verdict.sh` | sáu ca xanh, gồm ca *C-40* in `DROP (p50 …)` | đổi `0.10` thành `0.01` → ca "bỏ vì p50" đỏ | 6.0 |
| 6.5 | `scripts/check-wire-under-onload.sh` (mới) | developer (sonnet) | trên desk có Onload: `cargo test -p fixbolt-engine --test wire --no-run`, rồi script với binary đó → `59 / 59` + dòng `PassiveOpens` | 59/59 và delta nhỏ hơn số kết nối | bỏ `EF_TCP_*_LOOPBACK` → FAIL ở dòng `PassiveOpens` (chứng minh guard bắt được chạy-qua-kernel) | 6.1 |
| 6.6 | `docs/DESIGN.md` §9; `docs/hft-playbook.md`; `docs/GUIDE.md`; `docs/best-practices-hft.md`; ba file bẫy trong `docs/reference/`; `CHANGELOG.md` nếu script mới được coi là công cụ công khai | developer (sonnet); manager viết `STATUS.md` | `python3 scripts/check-links.py`; `scripts/check-adr-numbers.sh` | mỗi bẫy chỉ tên test canh nó | — | 6.2–6.5 |
| 6.7 | Senior review PR hàng 6, sửa phát hiện đã xác minh, CI xanh, merge | senior developer (opus) | mọi gate trên + CI | CI run id của commit đóng được ghi | — | 6.6 |
| 7a.1 | `scripts/boot-p4.sh` (mới) + diễn tập `RUNS=2` trên dòng desktop | **senior developer (opus)** — nối giao diện của hàng 5 và 6, sai thì hỏng cả boot | diễn tập chạy hết mọi khối, có `target/boot-p4-evidence/`; `bash -n`; shellcheck nếu có | output diễn tập ghi *không phải số đo* | cho `check-machine.sh` một dòng đỏ giả (`FIXBOLT_CHANNELS=2` khi đang 1) → driver dừng trước khối đầu | hàng 5 merge, 6.7 |
| 7a.2 | Build sẵn `../fb-p4-boot/`, `MANIFEST.txt`, `w2w` của Mac | runner (haiku) — brief tự đủ, lệnh chép từ ADR-0090 quyết định 2 | `sha256sum -c MANIFEST.txt`; `scripts/check-bench-alignment.sh` | mọi dòng `OK` | — | 7a.1 |
| 7a.3 | Kiểm trước reboot (7a bước 4), handoff, merge 7a, đổi grub, reboot | manager | `grep CMDLINE /etc/default/grub` | CI run id ghi trong handoff; `isolcpus` có, `nohz_full` không | — | 7a.2 |
| 7b.1 | Hành động đầu tiên của boot (7b bước 1–5) | manager | `check-machine.sh` bước 4 | không dòng đỏ | — | reboot |
| 7b.2 | Chạy `scripts/boot-p4.sh` | manager chạy; runner (haiku) trích output sau khi xong | output driver | driver thoát 0, hoặc dừng ở dòng đỏ đã nêu tên | — | 7b.1 |
| 7b.3 | Phán quyết + áp phán quyết: `measured-costs.md`, `DESIGN.md` §8 (nếu giữ), ADR-0098/0200 dòng kết quả, `docs/hft-playbook.md`, `PRD.md`; gỡ `io-uring` nếu bỏ | manager (tài liệu); senior developer (opus) gỡ code | `scripts/bypass-verdict.sh`; phán quyết hàng 5; `cargo test --all`, `--no-default-features`, clippy | mỗi cặp số kèm lệnh, máy, `check-machine.sh` | — | 7b.2 |
| 7b.4 | Dọn (Onload gỡ, `combined 2`, grub), senior review, CI, merge, `STATUS.md` | manager; senior developer (opus) review | CI | CI run id của commit đóng | — | 7b.3 |

## Cách kiểm chứng

| # | Tiêu chí | Lệnh | Đạt khi |
|---|---|---|---|
| 1 | Zero-copy thật | `sudo -n ss --xdp -a -e` sau `listening:` mỗi lần chạy Onload | XSK trên `ifindex` của `enp9s0`, `zc:1` — mọi lần chạy |
| 2 | Chạy thật qua Onload | `/proc/net/snmp` `Tcp: InSegs/OutSegs` trước/sau mỗi lần | tăng < 1 % số request |
| 3 | Twin sạch | `FIXBOLT_BYPASS=twin scripts/check-machine.sh` | `no xdp on nic` PASS, `afxdp register` PASS (chưa đăng ký) |
| 4 | Không cấp phát | dòng `allocs` của hai nửa `w2w` | `0` cả hai, cả hai nhánh |
| 5 | 59/59 dưới Onload | `scripts/check-wire-under-onload.sh <binary>` | `59 / 59`, `PassiveOpens` không tăng cỡ số kết nối |
| 6 | Vạch bỏ Onload | `scripts/bypass-verdict.sh` trên các tóm tắt của boot | in `verdict:` với từng mệnh đề |
| 7 | Hai procedure tái lập | `scripts/compare-w2w-procedures.sh` | theo ADR-0068; không tái lập vẫn công bố cặp, đánh dấu |
| 8 | Máy đúng §9 cho từng khối | output `check-machine.sh` mà driver lưu trước mỗi khối | không dòng đỏ |
| 9 | Script chưa hỏng gì cũ | `check-machine-verdicts.sh`, `check-w2w-baseline-summary.sh`, `check-w2w-compare.sh`, tên 17 dòng cũ | xanh, không đổi |

"Test pass" chưa đủ: số của boot là số đo trên máy bàn §9, qua cáp thật tới Mac, output của
`check-machine.sh` đi kèm từng khối; bước thăm dò 6.1 chỉ cho *sự thật* (zc, bộ lọc, allocs, bộ
đếm), không cho con số độ trễ nào.

## Tài liệu phải cập nhật

- [ ] `docs/DESIGN.md` §9 — dòng boot bypass (6.6); §8 — dòng thứ hai có nhãn **chỉ nếu giữ** (7b.3)
- [ ] `docs/hft-playbook.md` — quy trình Onload trên AF_XDP, `igb` = NIC dành riêng (6.6, 7b.3)
- [ ] `docs/GUIDE.md` — bypass chỉ `hft`, plaintext, `standard` không hỗ trợ (6.6)
- [ ] `docs/best-practices-hft.md` — ghi rõ mode `hft` (6.6)
- [ ] `docs/reference/` — ba bẫy: *igb từ chối luật n-tuple TCP của Onload* (canh: ca fixture 6.2 +
      ADR-0201 *Result*), *Onload không tăng tốc loopback mặc định* (canh: reversal của 6.5),
      *Onload lặng lẽ rơi về kernel* (canh: ca bộ đếm của 6.3); và mọi bất ngờ của 6.1 / boot
- [ ] `docs/reference/measured-costs.md` — mọi cặp A/B của boot, kể cả cặp làm bỏ, cạnh dự đoán (7b.3)
- [ ] `docs/PRD.md` §2 *Phase 4* — kết quả hai hạng mục (7b.3)
- [ ] ADR-0098, ADR-0200 — dòng kết quả; ADR-0201 *Result* (6.1)
- [ ] `STATUS.md` — handoff 7a (trước reboot), 7b (sau boot); *Not proven*
- [ ] `docs/internals/` — không (không crate nào đổi); `CONFIGURATION.md` — không (không khoá mới của engine)

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Onload lặng lẽ rơi về kernel (issue #337) → đo kernel mà dán nhãn Onload | `EF_NO_FAIL=0`; bộ đếm `Tcp: InSegs/OutSegs` mỗi lần (ca fixture 6.3); `zc:1` |
| Tưởng zero-copy mà thật ra copy / generic XDP | `ss --xdp` `zc:1` mỗi lần; dòng `xdp mode` |
| `igb` từ chối luật TCP của Onload → socket hỏng hoặc rơi về kernel | thăm dò 6.1 đọc `dmesg`; ADR-0201; dòng `af_xdp flow filters` |
| RSS chia hai hàng, XSK chỉ nghe một → luồng rơi sang hàng kia | `combined 1` cho cả boot; dòng `nic channels` |
| Tắt bộ lọc → mọi TCP trên `enp9s0` vào stack Onload → ssh qua cáp tới Mac chết giữa lần chạy | `GENERATOR_SSH` qua địa chỉ không phải cáp, kiểm ở 7a bước 4 |
| `onload cargo test --test wire` chạy trên loopback **kernel** (mặc định không tăng tốc) → 59/59 vô nghĩa | `check-wire-under-onload.sh` + reversal bỏ `EF_TCP_*_LOOPBACK` |
| Chương trình XDP vẫn gắn khi chạy twin → twin chậm hơn thật, Onload "thắng" giả | `FIXBOLT_BYPASS=twin` → `no xdp on nic` |
| Đổi số hàng đợi / gắn XDP làm link nhảy và đổi số IRQ → mất ghim IRQ | `check-machine.sh` trước mỗi khối; chờ `carrier` trước generator (6.3) |
| Thư viện Onload cấp phát trên luồng engine | `allocs 0` của `w2w` giữ nguyên (cổng G3) |
| Dùng dấu NIC cho twin mà không có cho Onload → hai thước | `BYPASS=onload` từ chối `WIRE_NIC`; twin chạy không `WIRE_NIC` |
| Build lại giữa boot → layout đổi | `MANIFEST.txt` sha256 trước/sau (ADR-0090) |
| Gọi tool giữa lúc driver chạy → máy không yên, mất vòng | không gọi tool cho tới khi driver thoát (7b bước 6) |
| Module Onload tự nạp khi khởi động → khối `io_uring` đo trên kernel có module lạ | `FIXBOLT_BYPASS=absent` → `bypass modules` |
| Lần chạy đầu sau reboot bẩn | 7b bước 5 bỏ đi một lần |
| Dùng `grub.fixbolt-s9` (có `nohz_full`) | 7a bước 6 `grep` không thấy `nohz_full` |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Onload không qua vạch p50 (phép tính ở trên) | **Rất cao** | đó là kết quả được thiết kế sẵn: ghi cặp số âm, tính là *xong* (Q1 của ADR-0098) |
| Onload `v9.0.2` không build được trên `7.0.0-31` (README ghi tới 7.0 — sát mép) | Trung bình | 6.1 dừng và báo; thử tag trước một lần; vẫn hỏng → **bỏ** với bằng chứng build |
| Module Onload làm treo máy bàn của anh (không phải bản release) | Thấp–trung bình | cài lúc anh biết; `onload_uninstall` sau boot; Secure Boot tắt nên không cần ký |
| Mac ngủ hoặc mất Tailscale/Wi-Fi giữa đêm | Trung bình | kiểm `pmset` trước reboot; driver dừng khi generator lỗi, không chạy tiếp vô ích |
| Boot dài 4–6 giờ, hỏng sớm là mất phần còn lại | Trung bình | procedure 2 của Onload chỉ chạy khi procedure 1 đạt; driver lưu từng khối |
| Hàng 5 chưa xong khi hàng 6 xong | Trung bình | 7a chờ hàng 5 merge; hàng 6 không phụ thuộc hàng 5 |
| Phía Mac nhiễu lớn (macOS, không ghim) che mất chênh lệch nhỏ | Cao | đó là thước ADR-0098 chọn; ADR-0200 ghi rõ độ lớn của nó |

## Anh cần quyết

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
- Chứng minh luồng engine không ngủ dưới Onload — chỉ nợ nếu Onload được giữ.
- Dấu thời gian phần cứng phía máy bàn cho nhánh Onload (không có đường nào trên `igb`).
- Thiết kế và code `io_uring` (hàng 5); hàng 7 chỉ chạy phép đo của nó.
- Tắt EEE phía Mac (cần sudo của anh trên Mac; tắt một đầu là đủ cho cả link — bộ nhớ 2026-09-14).

## Nhật ký giao hàng

*(Chưa có gì — điền khi từng hàng đóng: đã dựng gì, ở đâu, gate nào xanh, CI run id, cái chưa làm
và vì sao. Handoff trước reboot (7a) và sau boot (7b) ghi ở đây và ở `STATUS.md` cùng commit.)*
