# Boot F: đóng mọi open item còn lại trong một lần boot, rồi tắt máy

> **Loại:** Plan · **Ngày:** 2026-09-23 · **Trạng thái:** Chờ duyệt
> **Phạm vi:** `STATUS.md` *Open items* 51, 85, 89, 96, 99, 100 (và 76-b, 1, 14 — nói rõ vì sao không đóng); `scripts/bench.sh --strict` xanh trở lại trên bàn §9

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Boot E (2026-09-22/23) đóng 93, 95, 97 và để lại ba việc mới: `--strict` **không thể xanh** trên
bàn vì hai case `wakeup` p50 chưa bao giờ có cơ chế baseline (item 100); một case `journal`
**đổi số 7.4 → 12.4 ns giữa hai lần chạy trong cùng một boot, cùng một binary** (item 99); và giá
của "descent" (ADR-0086) mới có **cận dưới** 3 % vì `-g` không unwind được bản release (item 96).
Ngoài ba việc đó, bảng *Open items* còn ba hàng cũ chưa gạch: 51 (loopback đắt 32 syscall — 55–58 %
là mitigation của kernel), 85 (một figure lệch 15.9 % giữa hai procedure giống hệt, chưa có công
cụ so hai procedure), 89 (cadence của listener: cơ chế của +0.6…1.9 % ở admin path chưa có tên,
ADR-0069 vẫn `Proposed`).

Chủ sở hữu yêu cầu: **làm hết mọi item còn lại rồi tắt máy.** Bàn **đang** boot trên dòng §9
(`isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1`, mitigations **on**, uptime vài
phút, `sudo -n` chạy) — nên mọi bước §9 chạy ngay trong boot này, không reboot. Bước nào cần dòng
kernel khác (item 51: `mitigations=off`) thì plan nói thẳng là **không đóng được trong phiên
này** và vì sao.

Kết quả muốn đạt: mỗi item có verdict (đóng, đóng-theo-quyết-định, hay "không đóng được vì X"),
`bench.sh --strict` đọc **xanh** trên bàn trên commit đóng, handoff viết xong, bàn tắt.

## Những gì đã biết chắc

**Về máy và trạng thái hiện tại** (đọc trực tiếp 2026-09-23 07:16 +07):

- `/proc/cmdline`: `isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1`, không có
  `mitigations=off`; `uptime` 6 phút; `kernel.nmi_watchdog = 1`,
  `kernel.perf_event_max_sample_rate = 100000`, `kernel.perf_cpu_time_max_percent = 25`,
  `kernel.perf_event_paranoid = 4`. **`scripts/check-machine.sh` không có hàng nào đọc
  `nmi_watchdog`** (17 hàng: coalescing, CPU mitigations, C-states, eee, governor, irqbalance,
  isolcpus, kTLS, quiet, busy_poll, NIC IRQ, no nohz_full, no timer due, not virtualised, SMT,
  THP, turbo).
- Binary đã pin: `../fb-s9e/MANIFEST.txt` (cột `name sha package bench features rustflags
  binary_path sha256`); dòng `main-tree 9374820 fixbolt-engine journal` →
  `target/release/deps/journal-0f5dc4e01fbfa6df` sha256 `6db87d74…`; dòng `ms badc144
  fixbolt-session validate fix50sp2` → `../fb-s9e/ms/target/release/deps/validate-ea1ac0706c5fc955`
  sha256 `e34e60f9…`. `git diff --stat 9374820 fd85f47 -- crates/engine/benches/journal.rs
  crates/codec/benches/harness.rs crates/codec/benches/verdict.rs crates/engine/benches/wakeup.rs
  crates/engine/src/` **rỗng** — binary pin vẫn là `main` cho các bench này.
- `readelf -S` trên binary `ms validate`: có `.eh_frame` và `.eh_frame_hdr`, **không** có
  `.debug_*`. `perf --call-graph dwarf` unwind bằng CFI trong `.eh_frame` (libunwind/libdw), không
  cần `.debug_*`: [perf-record(1)](https://man7.org/linux/man-pages/man1/perf-record.1.html);
  rustc bật unwind tables mặc định trên `x86_64-unknown-linux-gnu`
  (`-C force-unwind-tables`, [rustc codegen options](https://doc.rust-lang.org/rustc/codegen-options/index.html)).
  Chỉ việc **gán mẫu cho frame đã inline** (`perf report --inline`) mới cần line tables.
- `target/release/w2w` (sha256 `1cbe3f0…`, build 2026-09-19 09:18, binary D5 đã dùng) còn nguyên;
  `strings` thấy `ktls` (123 dòng) — arm TLS chạy được. `target/boot-d-evidence/d5-n*-*.data`
  (bản ghi `perf` của D5) còn trên bàn. `scripts/compare-w2w-procedures.sh` và
  `scripts/check-w2w-compare.sh` **đã tồn tại** trên `main`.
- Grub: `/etc/default/grub.fixbolt-desktop-20260922` là dòng desktop; `grub.fixbolt-s9` là dòng §9.

**Về item 99** — sự thật mới, đọc từ evidence boot E, chưa có trong `STATUS.md`:

- `benches/baselines.tsv` dài **24 226 byte** ở `9374820`/`badc144` (S3 chạy trên file này) và
  **24 897 byte** ở `8153ba0`/`fd85f47` (S4 chạy sau re-record, +671 byte). Bench đọc file này
  **lúc chạy** bằng `std::fs::read_to_string` (`harness.rs:27-28`, ADR-0067) **trước** khi
  closure của bench cấp phát `Store` (`Box<[Slot]>`, `journal.rs:81`) và `msg` (`vec![b'x'; 191]`,
  `journal.rs:129-130`): `suite()` gọi `read_baselines()` rồi mới `f(&mut suite)`
  (`harness.rs:267-275`). Nên **heap của bench sau khi đọc file lệch đi đúng bằng kích thước file**.
- So S3 ↔ S4 từng case (`target/boot-e-evidence/s3-strict.txt` ↔ `s4-strict.txt`, cùng binary):
  không chỉ `one slot` (7.4 → 12.4, ×1.676) mà `inline deliver + reply` 7.7 → 8.5 (×1.104),
  `encode 1 group, 2 entries` 101.9 → 107.7 (×1.057), `validate TradeCaptureReport` 87 006 → 81 764
  (×0.940), `walk nested group + varData` 161.6 → 152.4 (×0.943). Bốn lần chạy lại sau S4 đều 12.4
  — đều trên file 24 897 byte.
- Tiền lệ trong repo: ADR-0067 — nối một dòng vào tsv khi còn `include_str!` đã đẩy một case 8.2 →
  6.3 ns ([recording-a-baseline-changed-the-baseline](../reference/recording-a-baseline-changed-the-baseline.md)).
  Tài liệu ngoài: Mytkowicz et al., *Producing Wrong Data Without Doing Anything Obviously Wrong*
  (ASPLOS 2009) — kích thước môi trường/thứ tự link dời heap/stack và đổi kết quả benchmark, cơ chế
  là load–store overlap ([pdf](https://users.cs.northwestern.edu/~robby/courses/322-2013-spring/mytkowicz-wrong-data.pdf));
  4K-aliasing giữa nguồn và đích của một bản sao khi hai địa chỉ trùng 12 bit thấp
  ([Kobzol/hardware-effects](https://github.com/Kobzol/hardware-effects/blob/master/4k-aliasing/README.md));
  Zen 2 có đường chậm cho `rep movs` phụ thuộc 5 bit thấp của hai con trỏ
  ([xen-devel 2026-01](https://ratatoskr.run/xen-devel/2026/01/15396505/t)). Case `one slot` sao
  191 byte từ **cùng một** `msg` vào **cùng một** slot mỗi vòng — đúng hình dạng nhạy với quan hệ
  địa chỉ; hai case `walking` đổi slot mỗi vòng nên chỉ 1/64 vòng gặp cùng quan hệ.
- Về nghi vấn cũ (PMU/NMI sau `perf record`): tìm thấy NMI watchdog **chiếm một counter** và
  `perf stat` in cảnh báo về nó ([perf stat patch 2026-06](https://ratatoskr.run/linux-perf-users/2026/06/17122660/t),
  [AMDESE/amd-perf-tools](https://github.com/AMDESE/amd-perf-tools)); **không tìm thấy** tài liệu
  nào nói một `perf record` đã thoát để lại trạng thái làm chậm code thường sau đó. Nghi vấn này
  vẫn được kiểm bằng một arm đối chứng (bước F1c), nhưng không còn là giả thuyết chính.

**Về item 100:**

- `crates/engine/benches/wakeup.rs:450-467` in `"{name} p50 … ns/op NO BASELINE n=20000"` và dòng
  `cases without a baseline: 2` **cứng**, không tra `baselines.tsv`; `scripts/bench.sh:163-167` đếm
  dòng đó → `--strict` đỏ về cấu trúc từ 2026-09-18. `bench.sh` **không** đặt `WAKEUP_CORES`, nên
  mọi lần `--strict` chạy `wakeup` **unpinned** (S3 in `unpinned`). Số đo hai lần trong boot E:
  epoll p50 5060 / 5050, poll p50 4900 / 4909 ns — lệch 0.2 %.
- Bộ so sánh là một chỗ: `crates/codec/benches/verdict.rs` (`include!` bởi `harness.rs` và test
  `crates/codec/tests/bench_verdict.rs`, ADR-0031 quyết định 4); `density.rs:97` và `dispatch.rs:27`
  đã include `harness.rs` bằng `#[path]`. `Suite::bench` tự đo (best-of-7 × 200 000) rồi so; không
  có hàm nào so **một con số đo sẵn**.
- Tài liệu ngoài về so percentile với baseline: Criterion.rs so **mean/median** bằng bootstrap
  t-test với ngưỡng nhiễu 1 % ([Analysis](https://bheisler.github.io/criterion.rs/book/analysis.html)),
  không có band cho p99/p99.9; các nguồn về p50/p95/p99 vận hành
  ([StatsTest](https://www.statstest.com/percentiles-latency-comparing-p50-p95-correctly)) đều nói
  percentile đuôi cần nhiều mẫu hơn nhiều lần so với p50. Với 20 000 mẫu/arm, p99.9 là 20 mẫu.

**Về item 96:**

- Boot E S2: `bad_nested_count` children 2.94 % ≡ self 2.93 % vì `-g` (frame pointer) unwind rác;
  giá cận dưới 2 455.6 ns; callee `defers` 34.6 %, `group_members` 31.3 + 4.6 %, `region_end` 5.7 %,
  `open` 3.9 % ([measured-costs](../reference/measured-costs.md) *Item 96*). C1 giữ (3 symbol),
  C3 giữ (`bad_group_count` inline vào `validate_with`).
- Zen 2 **không có** LBR/BRS dùng được cho `perf`: BRS từ Zen 3 (kernel 5.19,
  [LWN](https://lwn.net/Articles/877245/)), LbrExtV2 từ Zen 4 ([LWN](https://lwn.net/Articles/904482/)).
  Còn hai lựa chọn: frame pointer (cần build lại với `-C force-frame-pointers` → đổi layout) hoặc
  dwarf (không cần build lại). Chi phí dwarf: sao stack mỗi mẫu, ~1–3 % overhead và file lớn hơn
  ~10× frame pointer ([R. W. M. Jones](https://rwmj.wordpress.com/2023/02/14/frame-pointers-vs-dwarf-my-verdict/));
  frame pointer bỏ sót stack trong code tối ưu, dwarf chính xác hơn cho bản release
  ([linuxvox](https://linuxvox.com/blog/what-do-the-perf-record-choices-of-lbr-vs-dwarf-vs-fp-do/)).
- Debuginfo **không được** đổi codegen theo thiết kế của LLVM, nhưng LLVM **không bảo đảm** điều
  đó và có meta-issue theo dõi các vi phạm ([llvm #37076](https://github.com/llvm/llvm-project/issues/37076));
  cách kiểm là so `.text` của hai bản build có/không `-g`. Cargo: `debug = "line-tables-only"` chỉ
  sinh line table ([Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html));
  `[profile.bench]` kế thừa `release`; workspace hiện **không có** mục `[profile.*]` nào
  (`grep '^\[profile' Cargo.toml` rỗng, không có `.cargo/config.toml`). Một profile có thể đặt bằng
  biến môi trường `CARGO_PROFILE_BENCH_DEBUG=line-tables-only` mà không sửa file nào.

**Về 51, 85, 89:**

- 51: boot C `mitigations=off` → `TCP loopback, 8 in 8 out` 12 594 → 5 241 ns (−58.4 %), w2w admin
  p50 −55.9 ‖ −55.5 %; conntrack 3.3 %; **flush arm** (tắt Tailscale + xoá ruleset) chưa chạy —
  hai lần bị bỏ theo quyết định của chủ (measured-costs dòng 4110). Phoronix đo mitigation Retbleed
  trên Zen 2 tốn 14–39 % tuỳ tải ([Phoronix](https://www.phoronix.com/review/amd-3950x-retbleed));
  kernel doc SRSO nói Zen 1/2 dùng `srso_untrain_ret`/`srso_safe_ret`
  ([kernel.org](https://www.kernel.org/doc/html/latest/admin-guide/hw-vuln/srso.html)). Tách từng
  mitigation (`retbleed=off`, `spec_rstack_overflow=off`, `spectre_v2=off`) **mỗi arm là một
  reboot**.
- 85: guard đã có (`w2w-baseline.sh` in commit/tree/uptime/sha; `check-w2w-baseline-summary.sh` đọc
  hai phía); ADR-0068 quyết định 2: "reproduced" = hai median lệch ≤ 5 % ở p50, p99, p99.9. Còn mở:
  **nguyên nhân** (candidate: procedure 1 bắt đầu ~7 phút sau một lần build) và **không gì chạy
  hai procedure rồi so** bằng máy. Plan 2026-09-18 đặt probe C-85: P1 ≤ 2 phút sau `cargo build
  --release`, P2 ≥ 10 phút máy rảnh, cùng arm — chưa chạy.
- 89: ADR-0069 quyết định 2 (sửa 2026-09-18, boot C): default **16**, app −12.7…13.0 % p50 qua hai
  procedure, **N = 256 không phân biệt được với 16** (≤ 0.3 %), admin **+1.7–1.9 %** chưa rõ cơ
  chế; boot D D5: admin N=1 16 250 → N=16 16 351 (+0.62 %), `perf` gắn vào engine tid, file `.data`
  còn trên bàn, chưa `perf diff`. A/B ba mức đã đo → điều kiện "default chỉ đổi kèm figure"
  (quyết định 4) đã thoả; **chưa ai đổi trạng thái ADR**.

**Về luật của boot:** ADR-0090 quyết định 2 "a measurement boot measures and never compiles";
ADR-0095 quyết định 4: hai hàng `--strict` mỗi boot, `Compiling` trong output = run vô hiệu. Boot
này **phải** build lại (cơ chế wakeup + sửa harness) rồi mới `--strict` được xanh — mâu thuẫn
với chữ của ADR-0090; ADR-0096 quyết định 3 giải quyết (bên dưới).

## Cách làm

Bốn quyết định thiết kế nằm ở [ADR-0096](../decisions/ADR-0096-a-figure-measured-elsewhere-meets-the-same-band-the-baseline-file-stays-out-of-the-heap-and-a-boot-may-rebuild-on-its-housekeeping-cores.md)
(Proposed). Plan này chỉ ghi phương án đã chọn.

### Item 99 — tìm nguyên nhân bằng thí nghiệm không cần build, chạy **trước mọi `perf record`**

Giả thuyết chính **H1**: số đo phụ thuộc kích thước `benches/baselines.tsv` qua layout heap (sự
thật ở trên). Kiểm bằng ba arm theo thứ tự, trên binary pin `journal-0f5dc4e01fbfa6df` (sha256 so
với manifest trước mỗi arm, ADR-0093), `taskset -c 6`, mỗi lần chạy đọc dòng `one slot` và dòng
`walking` 191:

- **F1a — hiện trạng**: file 24 897 byte, chạy 3 lần. Kỳ vọng 12.4.
- **F1b — đổi mỗi kích thước file**: `git show 9374820:benches/baselines.tsv > benches/baselines.tsv`
  (24 226 byte, nội dung S3 đã đọc), chạy 3 lần; rồi **quét**: lấy file 9374820 và nối một dòng
  comment `# pad …` dài k byte, k = 0, 16, 32, …, 1024 (65 điểm, mỗi điểm 1 lần chạy, ghi bảng
  `k  size  one_slot  walking191`). Khôi phục bằng `git checkout -- benches/baselines.tsv` và chạy
  1 lần (kỳ vọng 12.4 trở lại). Dự đoán viết trước: F1b lần đầu đọc **7.4**, bảng quét là hàm bậc
  thang của k với ít nhất một bậc giữa 7.4 và 12.4; `walking` không đổi quá band.
- **F1c — đối chứng `perf`** (chỉ chạy **sau** F1b và sau khi mọi số đo timing khác của boot đã
  xong — bước F7): một `perf record -e cycles -F 4999 -g` của binary `ms validate` đúng lệnh S2,
  rồi chạy journal 3 lần trên file hiện tại. Dự đoán: vẫn đúng số của F1a (perf không đổi gì).

Verdict: H1 **được xác nhận** khi F1b đọc 7.4 và bảng quét có bậc → nguyên nhân có tên: *heap
layout của bench do kích thước file baseline đặt* — lần thứ hai cùng một bẫy của ADR-0067, lần này
ở run time. H1 **bị bác** khi F1b vẫn 12.4 → item 99 ở lại mở với bảng quét làm bằng chứng âm, F1c
là bước tiếp theo, và không sửa gì trong `crates/`.

Sửa (chỉ khi H1 xác nhận, desk-free, bước F5): `load_baselines` và `cpu_model` đọc vào `String`
có `with_capacity(1 << 20)` — buffer có **kích thước cố định**, không đổi theo độ dài file, nên
heap sau đó **không phụ thuộc** kích thước file (ADR-0096 quyết định 2, đã sửa 2026-09-23).
*Sửa lại lời giải thích*: bản đầu nói buffer 1 MiB đi qua `mmap`, ngoài brk heap — **sai** với
binary này. `cpu_model` chạy trước, buffer 1 MiB của nó được `mmap` rồi giải phóng; glibc thấy
một khối `mmap` được giải phóng thì **nâng ngưỡng mmap** lên bằng cỡ khối đó (1 052 672 byte,
`mallopt(3)`), nên buffer của `load_baselines` sau đó lấy từ brk (`strace` đọc `brk(+0x100000)`
quanh `benches/baselines.tsv`). Điều đó không làm hỏng cách sửa: cái case nhạy là *kích thước*
khối đứng trước, không phải khối nằm ở brk hay `mmap`. Tác dụng phụ: ngưỡng đã nâng giữ nguyên
đến hết tiến trình bench (cấp phát 128 KiB–1 MiB sau đó cũng từ brk); không đường đo nào cấp
phát nên chỉ ảnh hưởng chỗ đặt các khối set-up. Bằng chứng đảo: quét k lại trên binary mới (F8)
phải **phẳng** — đó mới là cái chứng minh, không phải lý thuyết về allocator. `--strict` không cần luật mới: sau sửa, nếu
`one slot` đọc ngoài band [6.7, 8.1] trên binary mới thì re-record **có nguyên nhân** (ADR-0095
quyết định 2, trích ADR-0096) — đây không phải "dời baseline cho xanh", vì nguyên nhân và bằng
chứng đảo đi trước con số.

### Item 100 — hai case `wakeup` p50 vào `baselines.tsv` qua **cùng một bộ so**

ADR-0096 quyết định 1: `harness.rs` thêm `Suite::figure(name, ns)` — so **một con số đo sẵn** với
band của máy, in đúng dòng `bench` in (`baseline … = [floor, ceiling]`, `OVER/UNDER BASELINE`,
`NO BASELINE` kèm dòng paste sẵn, và dòng tổng `cases without a baseline: N …`); `Suite::bench`
trở thành "đo best-of-7 rồi gọi `figure`". `wakeup.rs` include `harness.rs` bằng `#[path]` như
`density.rs`, gọi `figure("wakeup epoll p50", p50)` và `figure("wakeup poll p50", p50)`, **xoá**
dòng in `NO BASELINE` cứng và dòng `cases without a baseline: 2` cứng (lines 59–66, 153–159,
450–467). p99/p99.9/min/max vẫn in như cũ, **không có band** (20 mẫu cho p99.9 — không đủ để nói
"band"; ghi rõ trong module doc).

Ý nghĩa dòng tsv cho hai case này: `baseline` = **median của 20 lần chạy nguyên**, mỗi lần chạy
cho một p50 trên 20 000 mẫu; `margin` từ thang ADR-0016; `n = 20`; **unpinned** — vì `bench.sh
--strict` chạy unpinned và baseline phải cùng thủ tục với gate đọc nó. Tên case không đổi (`wakeup
epoll p50`, `wakeup poll p50`); nếu sau này có dòng pinned thì tên case sẽ mang hậu tố, không phải
bây giờ. Ghi ở F2 (binary cũ, dự phòng) và **kiểm lại** ở F8 (binary mới): nếu F8 đọc ngoài band
của dòng F2 thì lấy dòng từ F8 (đó là re-record lần đầu, có nguyên nhân: binary khác).

### Item 96 — `--call-graph dwarf` trên **đúng binary đã pin**, không build gì

Vì binary có `.eh_frame` và dwarf unwind dùng nó, bước F7 ghi trên `ms validate` (sha256 so
manifest): `sudo -n perf record -e cycles -F 999 --call-graph dwarf,32768 -o
target/boot-f-evidence/d96f-<k>.data -- "$VALIDATE_BIN"`, k = 1…3. Đọc off-desk (hoặc trên core
0–5 sau khi mọi timing xong) với `DEBUGINFOD_URLS= perf report -f -i … --children -s sym
--percent-limit 0 | grep bad_nested_count` và `--no-children`. Điều kiện đọc số (thêm vào C1–C3
của ADR-0094): **C4** — chain không rác: `perf report -f -g caller --stdio -S bad_nested_count…`
cho thấy caller là `validate_with`/`bad_group_count` và callee là `group_members`/`region_end`/
`open`, không phải địa chỉ FIX bytes; **C5** — `children% > self%` (nếu bằng nhau thì unwind không
chạy → tăng lên `dwarf,65528` và ghi lại). Giá = `children% × median` như ADR-0094 quyết định 1,
vẫn là **"≥"** vì C3 (tầng đầu inline). Item 96 đóng với số đó; câu về tầng inline đứng cạnh.

Tầng hai, **chỉ nếu** F7 cho children% ≥ 20 % (khi đó phần inline có thể đáng kể): build một bản
`validate` với `CARGO_PROFILE_BENCH_DEBUG=line-tables-only` (biến môi trường, **không** sửa
`Cargo.toml`), so `.text`: `objcopy -O binary --only-section=.text` hai file rồi `sha256sum`; bằng
nhau → ghi thêm 3 record và đọc `perf report --inline`; khác nhau → **không dùng**, ghi là "layout
khác, không đọc". Đây là build change chỉ trong boot này, đặt tên trong manifest với cột
`rustflags` như cũ và một cột `profile_env`.

### Item 89 — đặt tên (hoặc "unnamed") cho +0.62 % bằng luật ADR-0095, rồi ADR-0069 → Accepted

Không đo thêm: A/B ba mức đã có (boot C), tier boot D đã có. F4 (senior developer, phân tích, không
đo): `perf diff -c delta-abs -s symbol -o 1` trên các cặp `d5-n1-*.data` ↔ `d5-n16-*.data` (bốn cặp
chéo + hai cặp cùng-arm), quy ra ns theo p50 của run, verdict theo ADR-0095 quyết định 3 (một
symbol lớn nhất ở cả bốn cặp chéo, cùng dấu, ≥ 2× nhiễu cùng-arm) → *named* hoặc *accept, unnamed*
kèm IPC từ `.stat`. Kết quả ghi `measured-costs.md` *Boot D, D5 — the admin gap, diffed*. Trong cùng
PR: ADR-0069 `Proposed` → `Accepted` (chỉ đổi status và một dòng *Measured* trỏ tới bảng; quyết định
4 đã thoả). Item 89 đóng.

### Item 85 — công cụ so hai procedure chạy trong CI, và probe C-85 chạy trong boot này

- Công cụ: `scripts/compare-w2w-procedures.sh` đã tồn tại; F5 kiểm nó đọc hai `summary` của
  `w2w-baseline.sh`, in **per percentile** (p50, p99, p99.9) `median1 median2 diff% verdict` theo
  ngưỡng 5 % của ADR-0068, exit 1 khi có percentile lệch; `scripts/check-w2w-compare.sh` là test
  đảo của nó (đưa hai summary giả: một cặp lệch 6 % ở p99 phải FAIL, cặp 4 % phải PASS). Nếu hai
  file đã làm đúng thế — bước F5 chỉ nối vào job `gates` của CI; nếu chưa — sửa cho đúng thế.
- Probe C-85 (một biến: thời gian từ lần build gần nhất), arm `hft:admin` (off), `RUNS=20`,
  `target/release/w2w` pin sha `1cbe3f0…`: **P1** ở F3 (uptime ~15 phút, chưa build gì trong
  boot); **P2** ở F6 **≤ 2 phút** sau khi F6 build xong; **P3** ở F8 **≥ 10 phút** máy rảnh sau
  P2. Đọc bằng `compare-w2w-procedures.sh` từng cặp. Verdict: P1≈P3 và P2 lệch > 5 % ở p50 →
  candidate "sau build" **được xác nhận**, ghi vào
  [a-tight-spread-…](../reference/a-tight-spread-inside-one-procedure-did-not-reproduce-across-two.md)
  *candidate tested* và guard là dòng `uptime`/`build age` mà `w2w-baseline.sh` đã in — ADR-0068
  thêm một câu: procedure đầu **≥ 10 phút** sau lần build cuối (ADR-0096 quyết định 4b). Cả ba
  trùng nhau ≤ 5 % → candidate **bị bác**, item 85 đóng với *cause unnamed, comparator built*, và
  ADR-0068 không đổi.

### Item 51 — đóng **theo quyết định**, flush arm chỉ chạy nếu không cắt phiên điều khiển

Không thể tách từng mitigation trong boot này (mỗi arm một reboot, chủ muốn tắt máy). ADR-0096
quyết định 5: item 51 đóng *accept, named at the mitigation tier*: 55–58 % của loopback là
mitigation Zen 2 (đo boot C), 3.3 % conntrack, phần còn lại không tách; `DESIGN.md` §8 ghi floor
"mitigations on" (đã đúng) và một câu trỏ tới đo này; tách từng mitigation là plan riêng nếu chủ
muốn, **không** phải nghĩa vụ mở. Flush arm: chạy ở F9 **chỉ khi** phiên điều khiển không đi qua
`tailscale0` (kiểm `who`/`ss -tnp | grep 100.` trước; nếu có kết nối qua 100.x.y.z → **bỏ**, ghi
"skipped: session rides Tailscale"). Lệnh: lưu ruleset `sudo -n nft list ruleset >
target/boot-f-evidence/nft-before.txt`, `sudo -n systemctl stop tailscaled`, `sudo -n nft flush
ruleset`, chạy `payload` bench 5 lần (case `TCP loopback, 8 in 8 out`), rồi `sudo -n nft -f
nft-before.txt; sudo -n systemctl start tailscaled`, chạy lại 5 lần (A–B–A). Đọc: median B so A.

### Những item không đóng trong phiên này, nói thẳng

| Item | Vì sao | Ghi ở đâu |
|---|---|---|
| 51, phần tách từng mitigation | mỗi arm một reboot, chủ muốn tắt máy sau boot này | ADR-0096 quyết định 5 đóng item ở tầng mitigation; tách sâu hơn là plan mới |
| 76-b (`HeartBtInt=0` parse được, CONFIGURATION.md nói "positive integer") | quyết định hành vi của chủ: từ chối 0 (đổi session, cần plan + 59/59) hay sửa câu trong doc | hàng 76 giữ, thêm một dòng "chờ chủ chọn (a) từ chối, (b) sửa doc"; plan này không chọn thay |
| 1 (tên cuối), 14 (kernel bypass) | bảng *Open items* đã nói: không có plan theo thiết kế | không đụng |

### Kết thúc: handoff rồi tắt máy

F10: handoff trong `STATUS.md` *Start here 2026-09-23 (boot F)*; khôi phục dòng grub desktop
**trước** khi tắt (`sudo -n cp /etc/default/grub.fixbolt-desktop-20260922 /etc/default/grub &&
sudo -n update-grub`), vì sau boot này không còn phép đo nào được lên lịch và dòng §9 giữ 4 thread
khỏi desktop; `grub.fixbolt-s9` còn đó cho lần sau. Timer đã stop tự trở lại khi boot. Rồi
`sudo -n poweroff`.

### File tạo hoặc sửa

- Tạo: plan này; `docs/decisions/ADR-0096-…md`; `target/boot-f-evidence/` (ngoài repo).
- Sửa (developer/senior developer): `crates/codec/benches/harness.rs` (`figure`,
  `with_capacity`), `crates/engine/benches/wakeup.rs`, `crates/codec/tests/bench_verdict.rs` (test
  cho `figure` qua verdict), `benches/baselines.tsv` (+2 dòng wakeup; có thể 1 dòng journal
  re-record có nguyên nhân), `scripts/compare-w2w-procedures.sh` / `scripts/check-w2w-compare.sh`
  (nếu chưa đúng), `.github/workflows/*.yml` (job `gates` gọi `check-w2w-compare.sh`).
- Sửa (docs): `docs/reference/measured-costs.md` (*Boot F*), `docs/reference/
  recording-a-baseline-changed-the-baseline.md` (đoạn "the second time, at run time" + regression
  test tên gì), `docs/reference/a-tight-spread-…md`, `docs/decisions/ADR-0069` (status),
  `docs/decisions/ADR-0068` (một câu nếu 85 xác nhận), `docs/DESIGN.md` §6 (dòng wakeup có band) và
  §8 (câu item 51), `docs/internals/engine.md` (wakeup dùng harness), `CHANGELOG.md`, `STATUS.md`.

## Bất biến bị đụng tới

Việc này đụng `crates/codec/benches/` và `crates/engine/benches/` — **bench, không phải src** —
nhưng bench là gate của §2 rule 1 và 10, nên vẫn walk:

- **Rule 1** (không cấp phát trên hot path): `harness.rs` cấp phát 1 MiB **trước** vòng đo, ngoài
  closure; `benches/alloc.rs` của từng crate vẫn đọc 0 — chạy lại ở F6.
- **Rule 7**: không `unwrap`/`expect` mới; `harness.rs` là bench (không phải lib) nhưng giữ cùng
  chuẩn.
- **Rule 10**: mọi con số ghi kèm binary (sha256 trong manifest), máy, và verdict
  `check-machine.sh` **của lần chạy đó**; `--strict` sau build lại phải không có dòng `Compiling`.
- **Rule 4**: không đụng wait strategy; `wakeup` chỉ đổi cách in.
- `session`, `codec` src, `engine` src, `transport`: **không đụng**.

## Chia việc

Thứ tự là thứ tự thời gian trên **một** máy: bàn đo cũng là máy dev, nên **không** chạy bước
desk-free nào song song với một bước đo (memory: một tool call của manager làm hỏng một arm-round).
Manager idle trong mọi bước đo. Evidence vào `target/boot-f-evidence/` (không `/tmp`).

| Bước | Vai trò / model | Kết quả | File đụng (không đụng gì khác) | Cần bàn §9 | Gate (lệnh) | Xong khi | Quote về | Phụ thuộc |
|---|---|---|---|---|---|---|---|---|
| **F0** | manager | Worktree `../fb-strict` nhánh `plan/bench-strict-on-the-desk` từ `main` `fd85f47`; PR draft; plan + ADR-0096 `Proposed` commit đầu; `mkdir target/boot-f-evidence`; `FIXBOLT_NIC=enp9s0 scripts/check-machine.sh` → `pass 17 fail 0 unknown 0` (nếu đỏ: chạy dòng `fix:`, chạy lại; vẫn đỏ → dừng, handoff); `ps -eo pcpu,comm --sort=-pcpu \| head -5`; run bỏ đầu tiên: `taskset -c 6 <journal bin> >/dev/null` | docs, `target/boot-f-evidence/` | có | `python3 scripts/check-links.py`; `scripts/check-adr-numbers.sh` | cả hai xanh, check-machine 17/0/0 | ba output nguyên văn | — |
| **F1** | runner (haiku), brief tự chứa (lệnh nguyên văn từ mục *Item 99*) | F1a (3 run), F1b (3 run trên file 9374820, quét k = 0…1024 bước 16, restore, 1 run) — **F1c hoãn tới F7**. Trước mỗi arm: `sha256sum <journal bin>` khớp manifest. Sau F1: `git status --porcelain benches/` **rỗng** | chỉ `benches/baselines.tsv` **tạm thời** (restore bằng `git checkout --`), `target/boot-f-evidence/f1-*.txt` | có | `sha256sum` khớp; `git diff --exit-code -- benches/baselines.tsv` sau khi xong | bảng `k size one_slot walking191` 65 dòng; 7 run rời | ba số F1a, ba số F1b-đầu, số sau restore, và bảng quét nguyên văn | F0 |
| **F2** | runner (haiku), brief tự chứa | `wakeup` unpinned n = 20: `for i in $(seq 20); do <wakeup bin> \| grep 'p50.*ns/op'; sleep 8; done` (binary `main-tree` wakeup trong `target/release/deps/`, sha256 ghi lại — **không** có trong manifest, thêm dòng vào `MANIFEST.txt`), run 1 bỏ; median và max/median cho hai arm; `check-machine.sh` một lần trước, một lần sau | `target/boot-f-evidence/f2-wakeup.txt`, `../fb-s9e/MANIFEST.txt` (+1 dòng) | có | `check-machine.sh` `fail 0` cả hai lần | 20 cặp số; median; max/median | F1 |
| **F3** | runner (haiku), brief tự chứa | Probe C-85 **P1**: `ARMS="hft:admin" RUNS=20 scripts/w2w-baseline.sh` với `target/release/w2w` (sha `1cbe3f0…`), output → `f3-p1/`; ghi `uptime` và `stat -c %Y target/release/w2w` trước khi chạy | `target/boot-f-evidence/f3-p1/` | có | script tự in `qualified N of 20`, ≥ 10 | summary p50/p99/p99.9 | F2 |
| **F4** | senior developer (opus) — phân tích, không đo, chạy trên `taskset -c 0-5` | Item 89: `DEBUGINFOD_URLS= perf diff -f -c delta-abs -s symbol -o 1` trên các cặp `d5-n1-<i>.data` ↔ `d5-n16-<j>.data` (4 chéo + 2 cùng-arm); bảng `symbol share_1 share_16 ns_1 ns_16 Δns`; verdict theo ADR-0095 quyết định 3; IPC từ `.stat`; mục mới `### Boot D, D5 — the admin gap, diffed` trong `measured-costs.md`; ADR-0069 status → `Accepted` với một dòng trỏ bảng | `docs/reference/measured-costs.md`, `docs/decisions/ADR-0069-…md`, `target/boot-d-evidence/d5-analysis.txt` | không (đọc file trên bàn, core 0–5, **sau F3, trước F6**) | `python3 scripts/check-links.py` | một verdict: *named <symbol>* hoặc *accept, unnamed, IPC a→b* | verdict; top-5 Δns; lệnh nguyên văn của một cặp | F3 |
| **F5** | developer (sonnet); rồi **senior review** (opus, context mới, một lần cho F5) | (a) `harness.rs`: `Suite::figure(&mut self, name, ns)`; `bench` = đo rồi `figure`; `load_baselines`/`cpu_model` đọc vào `String::with_capacity(1 << 20)` (chỉ khi F1 xác nhận H1; nếu H1 bị bác → **không** đổi phần đọc file, nói rõ); (b) `wakeup.rs`: include harness, `figure` cho hai p50, xoá in cứng; module doc sửa câu "until boot C"; (c) test `crates/codec/tests/bench_verdict.rs`: `figure` qua `verdict` cho ba trường hợp; test capacity ≥ 1 MiB của `load_baselines` (nếu (a) làm); (d) `compare-w2w-procedures.sh` đúng mô tả mục *Item 85*, `check-w2w-compare.sh` đảo 6 %/4 %, nối vào job `gates`; (e) `benches/baselines.tsv` +2 dòng wakeup từ F2 (n = 20, margin theo thang, verdict F2) | `crates/codec/benches/harness.rs`, `crates/engine/benches/wakeup.rs`, `crates/codec/tests/bench_verdict.rs`, `scripts/compare-w2w-procedures.sh`, `scripts/check-w2w-compare.sh`, `.github/workflows/ci.yml` (job `gates`), `benches/baselines.tsv`, `docs/internals/engine.md`, `CHANGELOG.md`; **không** đụng `crates/*/src/`, `scripts/bench.sh` | không (compile trên bàn — **sau F4**, không có bước đo nào đang chạy) | `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `cargo test -p fixbolt-codec bench_verdict`; `cargo bench -p fixbolt-engine --bench wakeup -- --test` (in `baseline … = [` cho hai case trên máy có dòng, `NO BASELINE` ở máy khác); `scripts/check-w2w-compare.sh`; `cargo test --no-default-features` | mọi gate exit 0, output đọc đúng hình | diff per file; output từng lệnh nguyên văn; điều gì mơ hồ — dừng | F1, F2, F4 |
| **F6** | manager | Build lại cây `main` bench trên core housekeeping: `F="$(scripts/check-bench-alignment.sh --flags)"; RUSTFLAGS="$F" nice taskset -c 0-5 cargo bench --no-run --workspace --features fix50sp2`; `scripts/check-bench-alignment.sh` xanh; ghi sha256 của journal/wakeup mới vào `MANIFEST.txt` (name `boot-f`); `cargo bench -p fixbolt-engine --bench alloc --features fix50sp2` và `-p fixbolt-codec` → 0 mọi case; **ngay ≤ 2 phút sau build**: probe **P2** = lệnh F3 → `f3-p2/` (haiku); rồi máy rảnh ≥ 10 phút (manager không gọi tool) | `../fb-s9e/MANIFEST.txt`, `target/boot-f-evidence/f3-p2/` | có (ADR-0096 quyết định 3: build trên core 0–5, run sau đó không được có `Compiling`) | `check-bench-alignment.sh`; alloc rows | alignment xanh; alloc 0; P2 có ≥ 10 run qualified | F5 |
| **F7** | runner (haiku), brief tự chứa; đọc off-desk bởi senior developer (opus) | Item 96: 3 × `sudo -n perf record -e cycles -F 999 --call-graph dwarf,32768 -o target/boot-f-evidence/d96f-<k>.data -- <ms validate bin>` (sha256 khớp `e34e60f9…`), ghi `.out`; rồi **F1c**: 1 × lệnh S2 (`-g`) và 3 run journal → `f1c.txt`. Đọc: `DEBUGINFOD_URLS= perf report -f -i … --children -s sym --percent-limit 0 \| grep bad_nested_count`, `--no-children`, và `-g caller --stdio` cho C4; `Total Lost Samples`; verdict C1–C5; giá `children% × median`; nếu C5 fail → ghi lại với `dwarf,65528` (thêm 3 record) | `target/boot-f-evidence/d96f-*`, `f1c.txt`; `docs/reference/measured-costs.md` *Boot F, Item 96* (opus) | có — **sau mọi timing** trừ F8 | `Total Lost Samples: 0`; C4 và C5 giữ | giá "≥ X ns (children c %, self s %, n = 3, range)"; F1c ba số | F6 (và ≥ 10 phút rảnh) |
| **F8** | manager (chạy) + runner (haiku) cho quét | (1) probe **P3** = lệnh F3 → `f3-p3/`; `scripts/compare-w2w-procedures.sh` cho P1↔P2, P1↔P3, P2↔P3; (2) quét k như F1b trên **journal mới** (haiku) → `f8-sweep.txt`, kỳ vọng **phẳng** (max/min ≤ 1.10) nếu (a) của F5 đã làm; (3) `scripts/bench.sh --strict` lần 1 → `f8-strict-1.txt`: **0** dòng `Compiling`, đọc từng dòng OVER/UNDER/no-baseline; nếu chỉ `one slot` OVER với sweep phẳng → re-record dòng đó (median 20 run journal mới, có nguyên nhân = ADR-0096 quyết định 2) và nếu wakeup ngoài band F2 → lấy median F8; `--strict` lần 2 → `f8-strict-2.txt` phải **PASS** | `benches/baselines.tsv` (≤ 3 dòng, mỗi dòng có nguyên nhân trong commit body), `target/boot-f-evidence/f8-*` | có | `scripts/bench.sh --strict` (đọc **cả** ba hàng đếm, không chỉ `FAIL:` đầu) | `--strict` lần 2 in không `FAIL:`; `cases w/o a baseline 0`; sweep phẳng; ba verdict compare | F7 |
| **F9** | manager (điều kiện) + runner (haiku) | Item 51 flush arm **chỉ nếu** `ss -tnp \| grep -c ' 100\.'` = 0 và `who` không có địa chỉ 100.x: A (5 run `payload` case `TCP loopback, 8 in 8 out`), stop tailscaled + `nft flush ruleset`, B (5 run), restore ruleset + start tailscaled, A′ (5 run); nếu điều kiện không thoả → ghi `skipped: session rides Tailscale` | `target/boot-f-evidence/f9-*`, `docs/reference/measured-costs.md` *Boot F, Item 51 flush arm* | có | `sudo -n nft list ruleset \| diff - nft-before.txt` rỗng sau restore; `systemctl is-active tailscaled` = active | ba median, diff B−A và A′−A; hoặc dòng skipped | F8 |
| **F10** | manager; **senior review** (opus, context mới) trước merge | Docs theo §4 (mục *Tài liệu phải cập nhật*); `STATUS.md`: hàng 51 (đóng theo ADR-0096 q.5), 85, 89, 96, 99, 100 (gạch, verdict, bằng chứng), 76 (+1 dòng chờ chủ), *Start here 2026-09-23 boot F*, *Not proven* gạch/thêm; commit đóng; CI xanh **trên commit đó**; merge (uỷ quyền 2026-09-18); rồi grub desktop + `update-grub` + `grep CMDLINE /etc/default/grub` (không còn `isolcpus`); `git status` sạch mọi worktree; `sudo -n poweroff` | `STATUS.md`, docs nêu trên | có (tắt máy) | `check-links.py`; `check-adr-numbers.sh`; CI run id | run id ghi vào *Start here*; máy tắt | F9 |

Ước lượng thời gian bàn: F0 5′ · F1 15′ · F2 5′ · F3 5′ · F4 20′ (core 0–5) · F5 45–90′ (compile,
review) · F6 10′ + 10′ rảnh · F7 15′ · F8 25′ · F9 10′ · F10 30′ + CI. Tổng ≈ 3.5–4.5 giờ.

## Cách kiểm chứng

- **Item 99**: bằng chứng là bảng quét F1b (65 điểm, một binary, một boot, chỉ kích thước file
  đổi) — dự đoán viết trước: bậc thang; F1c là đối chứng âm cho nghi vấn `perf`. Đảo của guard:
  quét F8 trên binary mới **phẳng** (max/min ≤ 1.10) — nếu không phẳng thì guard **không** được
  nhận, `one slot` **không** re-record, item 99 ở lại mở với hai bảng.
- **Item 100**: `cargo bench --bench wakeup -- --test` trên máy CI in `NO BASELINE for '<cpu>'` và
  dòng `cases without a baseline: 2` **do harness in** (không phải wakeup.rs); trên bàn F8 in
  `baseline … = [floor, ceiling]` cho hai case và `--strict` lần 2 `cases w/o a baseline 0`. Test
  `bench_verdict` có case cho `figure`. Đảo: sửa tạm một dòng wakeup trong tsv thành 1 ns → `--strict`
  phải in `OVER BASELINE` cho đúng case đó (ghi câu FAIL kỳ vọng trước khi chạy), rồi restore.
- **Item 96**: C4 (chain có nghĩa) và C5 (`children > self`) đọc trên output `perf report`, quote
  nguyên văn; `Total Lost Samples: 0`; số cuối ghi "≥".
- **Item 89**: bảng `perf diff` sáu cặp; verdict theo luật có sẵn, hai người đọc lại được.
- **Item 85**: `check-w2w-compare.sh` đảo (6 % FAIL, 4 % PASS) trong CI; ba cặp P1/P2/P3 so bằng
  chính công cụ đó.
- **Item 51**: A–B–A′ với A′ ≈ A (≤ 5 %) là điều kiện để đọc B; ruleset sau restore `diff` rỗng.
- **Gate chung**: `cargo test --all`, `cargo test --no-default-features`, clippy, fmt; `bench.sh
  --strict` hai lần theo ADR-0095 quyết định 4, **không** dòng `Compiling`; CI xanh trên commit đóng.

## Tài liệu phải cập nhật

- [ ] `docs/reference/measured-costs.md` — mục *Boot F* (settings in force, F1 bảng quét, F2, F3
      ba procedure, F7 item 96, F8 strict ×2, F9), và *Boot D, D5 diffed* (F4)
- [ ] `docs/reference/recording-a-baseline-changed-the-baseline.md` — "the second time, at run
      time": kích thước file dời heap; guard = `with_capacity(1 << 20)` + test; quét làm regression
      trên bàn (nếu H1 xác nhận). Nếu H1 bị bác: mục mới trong `docs/reference/` ghi bảng quét âm
- [ ] `docs/reference/a-tight-spread-inside-one-procedure-did-not-reproduce-across-two.md` —
      *candidate tested* (C-85), verdict
- [ ] `docs/decisions/ADR-0096` — Proposed → Accepted khi merge; ADR-0069 → Accepted (F4);
      ADR-0068 một câu (chỉ nếu 85 xác nhận)
- [ ] `docs/DESIGN.md` §6 (hai case wakeup nay có band; cách tính n) và §8 (câu item 51: floor đo
      với mitigations on, 55–58 % là mitigation)
- [ ] `docs/internals/engine.md` — `wakeup.rs` dùng `harness.rs`, test canh
- [ ] `docs/CONFORMANCE.md` — không đổi (không có số conformance)
- [ ] `CHANGELOG.md` — `Suite::figure`, wakeup có baseline, comparator hai procedure trong CI
- [ ] `STATUS.md` — các hàng và *Start here* (F10); *Not proven*: gạch "Item 96's inclusive cost",
      "Item 99's cause" (nếu xác nhận); thêm "Item 51 per-mitigation split" là **decided, not
      measured**
- [ ] `benches/baselines.tsv` header — một câu: "một figure đo ngoài `Suite::bench` đi qua
      `Suite::figure`, cùng band"

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Manager gọi tool trong lúc một bước đo chạy → arm-round hỏng (boot E S1) | mỗi bước đo là **một** haiku foreground; manager không gọi tool cho tới khi nó về |
| `perf report` treo vì debuginfod, từ chối file root | `DEBUGINFOD_URLS=` và `-f` trong mọi lệnh ([reference](../reference/perf-report-hangs-on-debuginfod-and-refuses-a-root-owned-record.md)) |
| `perf record` qua `sudo` exit 0 khi không tìm thấy workload | đường dẫn tuyệt đối + `sha256sum` trước ([reference](../reference/perf-record-exits-zero-when-sudo-cannot-find-the-workload.md)) |
| Run đầu sau boot chậm (gnome-shell chưa lắng) | F0 có run bỏ; F2/F3 bỏ run 1 |
| `Compiling` lọt vào `--strict` sau F6 | F8 grep `-c Compiling` = 0, ghi vào evidence |
| Quét F1b chỉnh `baselines.tsv` mà quên restore → commit bẩn | gate `git diff --exit-code -- benches/baselines.tsv` ở cuối F1 |
| Dòng wakeup ghi từ binary cũ, đọc bằng binary mới → OVER do layout | F8 kiểm; nếu ngoài band thì lấy F8 làm re-record lần đầu, có nguyên nhân |
| dwarf copy 32 KiB không đủ, chain cụt ở phía `main` (không sao) hay ở lá (có sao) | C4: `bad_nested_count` phải thấy callee; C5: `children > self`; nếu fail → `65528` |
| `perf record` dwarf ở `-F 4999` sinh file hàng GB và làm chậm chính nó | `-F 999`; `df -h` trước F7 |
| `--strict` chỉ in `FAIL:` đầu tiên | đọc cả ba hàng đếm (Do-not list boot E) |
| Stop `tailscaled` cắt phiên điều khiển của chủ | điều kiện F9; nếu có kết nối 100.x → bỏ |
| `/tmp` là tmpfs, mất khi tắt máy | mọi evidence vào `target/boot-f-evidence/` |
| Test `check-w2w-compare.sh` đảo bằng số giả — chỉ chứng minh điều được thử | hai case (6 % FAIL, 4 % PASS) ở cả ba percentile |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| H1 (kích thước file → heap) bị bác ở F1b | trung | Không sửa phần đọc file; F1c vẫn chạy; item 99 ở lại mở với bảng quét âm; `--strict` F8 sẽ đỏ ở `one slot` và được **đọc bằng tay** như boot E, không re-record |
| `with_capacity(1 << 20)` không đi qua `mmap` (glibc `M_MMAP_THRESHOLD` đã bị nâng động) | thấp | quét F8 là bằng chứng, không phải lý thuyết; nếu không phẳng → guard không nhận |
| Build trên core 0–5 làm nóng/dơ máy cho F7/F8 | trung | 10 phút rảnh sau F6; `check-machine.sh` trước F7 và F8; F7 (perf) đặt **sau** mọi timing trừ F8 |
| dwarf unwind vẫn không cho chain (C4/C5 fail cả ở 65528) | thấp | item 96 giữ "≥ 3 %", ghi *unnamed inclusive*; tầng hai (line tables) chỉ khi `.text` hash bằng nhau |
| Probe C-85 ba procedure trùng nhau — candidate bị bác | trung | đó là một verdict hợp lệ: item 85 đóng với comparator và *cause unnamed*; không kéo dài |
| CI đỏ ở job mới `check-w2w-compare.sh` trên runner khác shell | thấp | script bash 3.2-safe như `bench.sh` (không `mapfile`) |
| Phiên hết thời gian trước F10 | trung | mỗi bước xanh đã commit + push; handoff ghi bước đang dở; **không** tắt máy khi F8 chưa xong — tắt là bước cuối, có điều kiện |

## Ngoài phạm vi

- Không tách từng mitigation của item 51 (mỗi arm một reboot).
- Không đặt band cho p99/p99.9 của wakeup (20 mẫu ở p99.9 không phải band).
- Không chạy wakeup pinned và không sửa `bench.sh` để pin — dòng ghi là unpinned, đúng thủ tục gate.
- Không sửa `Cargo.toml` profile; line tables chỉ qua biến môi trường, trong một boot, có hash `.text`.
- Không quyết định 76-b thay chủ; không đụng 1, 14.
- Không đo lại item 95 (PR B, commit nào) — không có trong Open items như một hàng mở riêng.
- Không đổi `crates/*/src/`.

## Nhật ký giao hàng

(điền khi đóng từng bước: F<k>, commit, gate quote, cái gì chưa làm và vì sao)
