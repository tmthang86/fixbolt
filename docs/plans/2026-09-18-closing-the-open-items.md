# Đóng các item còn mở — bằng một con số, một test, hoặc một quyết định

> **Loại:** Plan · **Ngày:** 2026-09-18 · **Trạng thái:** Chờ duyệt
> **Phạm vi:** `STATUS.md` *Open items* 13, 14, 21, 22, 24, 36, 40, 45, 49, 55, 76, 84, 85, 86, 87, 91 và `PRD.md` §6 hàng 1, 3, 4, 7. **Không** gồm 89 và nhánh "mitigations" của 51 — đang được đo ở bàn §9 lúc viết plan này.

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Chủ sở hữu yêu cầu: **mọi item còn mở trong `STATUS.md` phải đóng** — bằng một con số đã đo, một
test giữ nó, hoặc một ADR nói rõ "không làm, vì sao" — và bốn câu hỏi mở của `PRD.md` §6 phải có
câu trả lời, rồi mới xây phase 2. Bảng *Open items* hiện có hai loại hàng lẫn nhau: hàng đã đóng
nhưng còn một câu "chưa đo X" ở cuối (13, 14, 21, 22, 24, 76), và hàng thật sự mở, phần lớn là
câu hỏi thiết kế sinh ra từ boot B (40, 84–87, 91) hoặc từ trước (36, 49, 55, 45).

Plan này chia từng item thành **một trong bốn cách đóng**:

| Cách đóng | Nghĩa là |
|---|---|
| **ADR** | quyết định, viết thành ADR `Proposed`, chủ sở hữu duyệt → item đóng, không có code |
| **test** | một bước xây, có file và test đặt tên, chạy được trên máy bất kỳ (CI đủ) |
| **đã có số** | số đo đã nằm trong repo; việc còn lại là gạch hàng và trỏ tới nơi số đó ở |
| **needs-desk** | phải đo trên bàn §9; ghi đúng phép đo để manager gom vào boot C |

Kiến trúc sư đã tra cứu trước từng quyết định (`CLAUDE.md` §12); nguồn ghi ở mục *Những gì đã
biết chắc* và trong từng ADR. Việc đang đo ở bàn (89, 51) **không được đụng**, và plan này
**không chạy cargo, bench hay script nào** khi được viết.

## Những gì đã biết chắc

Đọc bảng này trước, mỗi dòng là một sự thật có nguồn; cái gì còn là phỏng đoán nằm ở *Rủi ro*.

| # | Sự thật | Nguồn |
|---|---|---|
| 1 | Toolchain **không đổi** giữa hai ngày đo của item 91: `rust-toolchain.toml` ghim `1.98.0` từ `904dd4b` (2026-08-30), không commit nào sau đó | `git log -- rust-toolchain.toml` |
| 2 | Kernel **đã đổi**: các số 2026-09-05 ghi `7.0.0-30-generic` (`measured-costs.md` dòng 896, 1851), các số 2026-09-14/15 ghi `7.0.0-31-generic` (`DESIGN.md` §8 *TLS*, ADR-0005 câu hỏi 2) | hai file trên |
| 3 | Code hot path đổi giữa hai ngày đó: `588b350` (2026-09-09, `52=` đọc mọi độ chính xác — đụng `crates/session` validate), toàn bộ nhánh TLS (`crates/engine`), `6074c26` (rustls bump 2026-09-15, không đụng codec/session) | `git log --since=2026-09-05 --until=2026-09-16 -- crates/*/src` |
| 4 | `validate NewOrderSingle` 882.1 → 955.9 (+8.4%) là **pure user space**; `density` (+3.0–4.7%) là **syscall-bound**. Hai loại này chỉ cùng chậm nếu nguyên nhân là máy, hoặc là hai nguyên nhân | `benches/baselines.tsv` phần đuôi, item 91 |
| 5 | Vòng đo của `w2w` client **giống hệt nhau** ở hai path: `t0` → `sock.put` → `read_one` → `elapsed`; mọi kiểm tra `field()` nằm **ngoài** đoạn tính giờ. Nên phần "ngoài engine" của 3 898 ns không phải là việc client làm thêm | `tools/w2w/src/main.rs:1990–2040` |
| 6 | D_in (chênh trong tiến trình app − admin) = **765.5 ns**; tổng các case đã cam kết ≈ 1 104 ns > D_in, nên chúng không độc lập; ~3 130 ns nằm **ngoài** một lượt engine | item 49, `DESIGN.md` §8 *The 3 898 ns* |
| 7 | `igb` (I210/I211) giữ **một** yêu cầu stamp TX; I225/I226 (`igc`) có **bốn** thanh ghi (`IGC_MAX_TX_TSTAMP_REGS`) | ADR-0071 *Sources* |
| 8 | Stamp phần cứng cho skb chỉ có cờ `SKBTX_BPF` **không vào error queue**: `__skb_tstamp_tx` gọi callback BPF rồi `return` khi `skb_tstamp_tx_report_so_timestamping` thấy thiếu `SKBTX_HW_TSTAMP_NOBPF` — đọc `net/core/skbuff.c` tag `v6.16` ngày 2026-09-18, trích trong ADR-0073 | ADR-0073 |
| 9 | `/proc/<pid>/task/<tid>/status` có `voluntary_ctxt_switches` **riêng từng thread** | proc_pid_status(5), proc_pid_task(5) — ADR-0072 |
| 10 | kTLS phần mềm: crypto chạy đồng bộ trên CPU gọi, giải mã RX xảy ra **trong `recvmsg`**; tài liệu kernel nói bộ tăng tốc bất đồng bộ "introduces extra latency on socket reads"; bài của Kicinski nói sau ba sửa RX 5.20 hiệu năng "comparable with the user space OpenSSL" — không nguồn nào nói kTLS nhanh hơn cho bản ghi nhỏ | ADR-0070 *Sources* |
| 11 | OpenOnload chạy được trên NIC không phải Solarflare qua AF_XDP (chế độ copy nếu driver không có zero-copy), "not currently at release quality" | ADR-0074 *Sources* |
| 12 | SBE là encoding "complimentary to other FIX standards for session protocol"; iLink 3 = SBE **trên FIXP**; không tìm thấy venue nào chở SBE trong session FIXT tag=value; Artio tách FIX và iLink 3 thành codec và connection riêng | ADR-0078, ADR-0079 *Sources* |
| 13 | B8: fixbolt `standard` app p50 chậm hơn nanofix +1 719 / +1 113 ns; admin đổi dấu; arm fixbolt **không tái hiện** | `measured-costs.md` *B8* |
| 14 | Item 13, 21, 22, 24, 76 đã ghi **CLOSED** trong chính hàng của nó, kèm PR/ADR; 21 và 76 chưa có gạch `~~`; 22 và 24 còn một câu "chưa đo" ở cuối | `STATUS.md` dòng 4988, 4994, 5006, 5012, 5053 |
| 15 | ADR-0025 (PRD §6 hàng 7) cố ý để `Proposed` vì số 4 phiên/engine dựa trên "epoll-class wakeup 2–5 µs **từ tài liệu**, chưa đo ở đây" — ADR-0014 câu hỏi 1 | ADR-0025 *Context* |
| 16 | ADR cao nhất trước plan này là 0069; plan này viết **0070–0079** | `ls docs/decisions` |

Tra cứu **không** tìm thấy: (a) so sánh độ trễ kTLS/userspace cho bản ghi ~200 byte trên kernel
gần đây ngoài PR #71 của repo này; (b) hồi quy hiệu năng nào của rustc liên quan (toolchain
không đổi nên không cần); (c) engine nào công bố điểm corpus "mirrored".

## Cách làm

Một ADR cho mỗi quyết định, một bước xây cho mỗi test, một dòng đo cho mỗi việc cần bàn §9.
Gom thành bốn PR, docs trước, code sau, và một gói *boot C* cho manager.

### Từng item — cách đóng và lý do

| Item | Cách đóng | Quyết định / việc | ADR |
|---|---|---|---|
| **84** kTLS chậm hơn userspace | **ADR** + needs-desk (số phụ) | Quyết định 2 của ADR-0005 **giữ**, nhưng lý do chỉ còn là *bảo đảm không cấp phát + parse tại chỗ*; độ trễ trở thành **giá phải trả** ghi rõ trong `best-practices-hft.md` §9; điều kiện lật quyết định là một NIC có `tls-hw-tx-offload`. Đo thêm hai arm "lệch" (engine kTLS / client userspace và ngược lại) để tách phần của engine — không đổi quyết định | ADR-0070 |
| **40** stamp thiếu trên I211 | **ADR** + test + needs-desk | Run thiếu stamp là run **thiếu mẫu**, không phải run hỏng: FAIL chỉ khi thiếu > 0,1 % (20/20 000) hoặc thiếu RX; in số thiếu cạnh mọi percentile; bảng của Mac trên tập đủ và tập có stamp phải khớp band ADR-0031. Sweep interval 0/10/20/30/50 µs là A/B ở bàn | ADR-0071 |
| **86** gate không cần tracer | **ADR** + test | `w2w` đọc `voluntary_ctxt_switches` của engine tid trước/sau cửa sổ; `hft` phải là 0; script mới `check-no-kernel-sleep-by-ctxt.sh` có đảo chiều `standard` > 0 tích hợp | ADR-0072 |
| **87** `standard` không stamp được | **ADR** | Đường đúng là BPF sock_ops (tiền đề đã xác minh trên source `v6.16`); **không xây trong plan này**, xếp vào boot §9 đầu tiên của phase 2; hình dạng (C + `clang -target bpf`, loader `aya`, feature tắt mặc định) chốt sẵn | ADR-0073 |
| **14** kernel bypass, **22** (câu `io_uring`), **24** (câu Logon hop) | **ADR** | Bypass giữ ngoài phạm vi, thứ tự Onload → `ef_vi` → không DPDK, thêm sự thật Onload chạy trên AF_XDP; `io_uring`/`recvmmsg` chỉ thử sau khi có số NIC ở interval 0; Logon hop không đo, §8 nói vì sao | ADR-0074 |
| **55** nợ indexing | **ADR** | Ratchet **là** cách đóng; trả nợ chỉ khi file mở vì việc khác; sàn (crc32, const fn) ghi nhận | ADR-0075 |
| **36** trần corpus mirrored | **ADR** + test | Rút trần 45; trần = số file `Reachable` theo bảng phân loại có test (`Unclassified` rỗng; đếm khớp ceiling; `34=0`/`123=Y` kiểm lại trên file thật) | ADR-0076 |
| **PRD 1** positioning | **ADR** | Acceptor-first giữ; bỏ chữ "fastest" khỏi tiêu đề; từ so sánh chỉ được đứng cạnh một cặp tái hiện (ADR-0068) | ADR-0077 |
| **PRD 3** SBE / FIXP | **ADR** | Phase 2 = SBE **encoding không session** + FIXT/FIX 5.0; FIXP là phase 3, ADR riêng khi có venue và oracle | ADR-0078 |
| **PRD 4** một hay nhiều view | **ADR** | **Nhiều view, một trait `Encoding`** (associated types, static dispatch); `MessageView` và API `codec` **không đổi**; gate đầu tiên của phase 2 là các bench tag=value vẫn trong band | ADR-0079 |
| **PRD 7** engine tự cấu hình | **needs-desk** (không ADR mới) | ADR-0025 đã trả lời; thứ nó thiếu là số "epoll-class wakeup" đo ở đây. Bước 4.2 viết `crates/engine/benches/wakeup.rs`; số đo ở boot C; rồi manager chuyển ADR-0025 sang *Accepted* (ADR `Proposed` được sửa tại chỗ, `CLAUDE.md` §5) | — |
| **85** lệch giữa hai procedure | **test** + needs-desk | Nửa guard đã có (ADR-0068). Còn thiếu: (a) script so hai summary và in *reproduced / not* từng percentile — test thuần như `check-w2w-baseline-summary.sh`; (b) header ghi thêm nhiệt độ và tần số core engine để lần sau có ứng viên đọc được; (c) ở bàn: một cặp procedure trong đó procedure 1 chạy **ngay sau** một build release và procedure 2 sau 10 phút nghỉ — một biến | — |
| **91** timing suite chậm 3–8 % | **needs-desk**, có danh sách ứng viên sẵn | Tách máy khỏi code bằng **một** A/B: checkout commit của các dòng 2026-09-05 trong worktree, build, chạy `bench.sh` cho `serialize density validate` (n = 20), cùng boot với HEAD. Nếu commit cũ cũng đọc số hôm nay → kernel `-30 → -31` là ứng viên duy nhất còn lại (sự thật 1–4); nếu không → bisect từ `588b350` cho `validate`, nhánh TLS cho `density` | — |
| **49** ~2 770 ns chưa quy được | **test** (case bench thiếu) + needs-desk | Bước 4.1: thêm case `serialize Heartbeat (session)` — số hạng duy nhất chưa trừ. Ở bàn: đo **tại chỗ** thay vì qua slope — `perf trace -s`/`strace -T` trên engine tid ở `hft`, admin và app, lấy trung vị thời gian `sendto` và `recvfrom` từng path; chênh của hai syscall là số kernel thật của payload lớn hơn. Nếu tổng vẫn hụt, item đóng bằng câu "phần còn lại là kernel, đo tại chỗ = N ns" — đó là con số, không phải nguyên nhân | — |
| **45** bản đồ thứ tự | **docs** | Chuyển thứ tự còn lại (boot C → boot D → wave D → ADR encoding phase 2) thành 6 dòng trong `STATUS.md` *Where the work is*; gạch hàng 45 | — |
| **13, 21, 76** | **đã có số** | Gạch `~~` trọn hàng (13 đã gạch; 21, 76 chưa); không việc gì khác | — |
| **22, 24** | **đã có số** + ADR-0074 | Câu cuối của mỗi hàng được ADR-0074 trả lời; gạch trọn hàng | — |

### Gom thành PR

| PR | Nhánh | Nội dung | Đụng code? |
|---|---|---|---|
| **PR 1 — docs** | `plan/closing-the-open-items-docs` | plan này; ADR-0070–0079; các sửa `STATUS.md`, `DESIGN.md`, `PRD.md`, `README.md`, `best-practices-hft.md`, `CLAUDE.md` liệt kê ở *Tài liệu phải cập nhật*; gạch 13/14/21/22/24/45/55/76/84/87 và PRD 1/3/4 | không |
| **PR 2 — scripts + w2w** | `plan/closing-the-open-items-gates` | bước 2.1 (ctxt gate, item 86), 2.2 (missing-stamp rule, item 40), 2.3 (compare script + header, item 85) | `tools/w2w`, `scripts/`, `.github/workflows/ci.yml` |
| **PR 3 — conformance** | `plan/closing-the-open-items-mirror` | bước 3.1 (`mirror.rs` + tests, item 36), 3.2 (điền số vào ADR-0076) | `crates/conformance` |
| **PR 4 — benches** | `plan/closing-the-open-items-benches` | bước 4.1 (Heartbeat serialise case, item 49), 4.2 (`wakeup.rs`, PRD 7) — cả hai **không có baseline** cho tới boot C | `crates/session/benches`, `crates/engine/benches` |
| **Boot C — gói đo** | không phải PR; manager gom vào plan boot C | C-40, C-49, C-84, C-85, C-91, C-PRD7 ở bảng dưới | — |

PR 2, 3, 4 độc lập nhau (file rời), có thể chạy song song sau khi PR 1 được duyệt (các ADR
phải `Accepted` trước khi code của chúng vào — ADR-0071, 0072, 0076).

## Bất biến bị đụng tới

| Bất biến | Bước | Giữ bằng cách nào |
|---|---|---|
| 4 — `hft` không ngủ / `standard` phải ngủ | 2.1 | gate mới có đảo chiều tích hợp (`standard` phải > 0); gate `strace` giữ nguyên; senior review vì đây là machine check của bất biến |
| 1 — không cấp phát hot path | 2.1 | đọc `/proc` trên **main thread**, ngoài cửa sổ đo; `allocs` của `w2w` vẫn 0 — quote trong report |
| 3 — 59 định nghĩa | 3.1 | không đụng `crates/session`; nếu một file `Reachable` đỏ, đó là item mới, không sửa trong bước này |
| 10 — không số nào không có bench/máy/§9 | 4.1, 4.2 | case mới in `NO BASELINE` tới boot C; không số nào được trích |
| 7 — không unwrap | 2.1, 3.1, 4.x | clippy `-D warnings` |

PR 1 không đụng `codec`/`session`/`engine`/`transport`: **không**.

## Chia việc

Mỗi dòng đủ để viết brief theo `CLAUDE.md` §12 (*The brief is the spec*). Cột *Đọc trước* là
đúng chỗ, không phải cả file.

| Bước | Item | Kết quả | File đụng (không đụng gì khác) | Gate (lệnh) | Tier | Phụ thuộc |
|---|---|---|---|---|---|---|
| **1.1** | tất cả docs | Manager áp các sửa ở *Tài liệu phải cập nhật*, gạch hàng, mở PR 1 draft | `STATUS.md`, `DESIGN.md`, `PRD.md`, `README.md`, `docs/best-practices-hft.md`, `CLAUDE.md` | `python3 scripts/check-links.py` (hoặc script link hiện có) xanh; `grep -c fastest README.md docs/DESIGN.md docs/PRD.md` = 0 ở dòng tiêu đề | manager (haiku cho link check) | ADR 0070–0079 được duyệt |
| **2.1** | 86 | `w2w` in `engine-ctxt voluntary <n> involuntary <m>` (đọc `/proc/self/task/<tid>/status` trước và sau cửa sổ, trên main thread); `--mode hft` **fail** nếu voluntary ≠ 0. Script `scripts/check-no-kernel-sleep-by-ctxt.sh`: chạy `hft` → 0, chạy `standard` → > 0, in cả hai; header nói nó không gọi tên syscall. `w2w-baseline.sh` ghi hai số vào mỗi run. CI: một step mới trong job mode hiện có. Đọc trước: ADR-0072 *Decision*; `scripts/check-no-kernel-sleep.sh` header và vòng `for red in`; `tools/w2w/src/main.rs:1791-1800` (`print_engine_tid`) và `:2033-2045` (`ARMED`/`allocs`) | `tools/w2w/src/main.rs`, `scripts/check-no-kernel-sleep-by-ctxt.sh` (mới), `scripts/w2w-baseline.sh` (chỉ dòng in), `.github/workflows/ci.yml`, `CLAUDE.md` §2 hàng 4 (manager) | `scripts/check-no-kernel-sleep-by-ctxt.sh` in `hft voluntary 0 … standard voluntary N>0 … PASS`; **đảo chiều**: chạy `w2w --mode hft` với `--yield` (hoặc mode `yield` script cũ dùng) phải đỏ ở assertion `voluntary`; `cargo clippy --all-targets -- -D warnings` | sonnet xây → **opus review** (bất biến 4) | ADR-0072 duyệt |
| **2.2** | 40 | `w2w-baseline.sh` chế độ `WIRE_NIC`: FAIL khi `hw-tx-missing > 0.1 %` hoặc `hw-rx-missing > 0`; in số thiếu cạnh p50/p99/p99.9 mỗi run và trong summary; đọc `ethtool -S <nic> \| grep tx_hwtstamp_skipped` trước/sau, in chênh; `w2w --listen --wire-timestamps` in bảng Mac cho **cả tập** và **tập có stamp**. Đọc trước: ADR-0071 quyết định 1–3; `scripts/w2w-baseline.sh` phần `WIRE_NIC` (grep `hw-tx-missing`); `tools/w2w/src/main.rs` `print_figures` (`:1824`) | `scripts/w2w-baseline.sh`, `scripts/check-w2w-baseline-summary.sh` (thêm case: 19 thiếu → PASS, 21 thiếu → FAIL, RX thiếu 1 → FAIL), `tools/w2w/src/main.rs` (chỉ phần in) | `scripts/check-w2w-baseline-summary.sh` xanh với ba case mới; `cargo test -p fixbolt-w2w` (unit test `standard_is_refused…` vẫn xanh); clippy | sonnet | ADR-0071 duyệt |
| **2.3** | 85 | `scripts/compare-w2w-procedures.sh <summary1> <summary2>`: in từng arm × percentile, `diff %`, verdict `reproduced`/`not reproduced` theo ADR-0068 quyết định 2 (≤ 5 % của số nhỏ hơn); hàm so sánh **thuần**, test bằng số của 2026-09-14 (20 774 ‖ 17 473 → not; các arm 0,2–2,5 % → reproduced). Header `w2w-baseline.sh` in thêm `/sys/class/thermal/thermal_zone*/temp` và `cpuinfo_cur_freq` của core engine (đọc một lần, không đụng engine). Đọc trước: ADR-0068 quyết định 1, 2, 5; `scripts/check-w2w-baseline-summary.sh` toàn bộ (mẫu test thuần) | `scripts/compare-w2w-procedures.sh` (mới), `scripts/check-w2w-compare.sh` (mới), `scripts/w2w-baseline.sh` (header) | `scripts/check-w2w-compare.sh` in `ok … pass N fail 0`; đảo chiều: đổi 5 → 4 trong test phải đỏ ở arm 4,7 % của B2 | sonnet | — |
| **3.1** | 36 | `crates/conformance/src/mirror.rs`: enum `MirrorClass` với rustdoc trỏ ADR; bảng 50 hàng; ba test theo ADR-0076 quyết định 2; score test đọc ceiling từ bảng. Người xây **phân loại 34 file còn lại** bằng cách đọc từng `.def` và API initiator công khai (`crates/engine/src/lib.rs` các `Sender`/`Outbox` door, D15). Đọc trước: ADR-0076; ADR-0006 *Context*; `crates/conformance/src/script.rs::mirrors`; `crates/session/tests/goodbye.rs` (mẫu hai defect 2026-09-02) | `crates/conformance/src/mirror.rs` (mới), `crates/conformance/src/lib.rs` (một dòng `mod`), test score mirrored hiện có (chỉ đổi nguồn ceiling) | `cargo test -p fixbolt-conformance` xanh; `cargo test -p fixbolt-session --test score` vẫn 59/59; đảo chiều: đổi một file `Reachable` thành `Unclassified` → test `unclassified_is_empty` đỏ | **opus** (phán đoán 34 file) | ADR-0076 duyệt |
| **3.2** | 36 | Điền đoạn *Measured* vào ADR-0076: số `Reachable`, số `Needs*` từng loại, commit, CI run id; nếu bước 3.1 tìm ra defect session → item mới, không sửa ở đây | `docs/decisions/ADR-0076-…md`, `STATUS.md` hàng 36 (manager) | quote output test | manager | 3.1 |
| **4.1** | 49 | Case `serialize Heartbeat (session)` trong `crates/session/benches/validate.rs` hoặc file bench session phù hợp: session sinh `35=0` trả lời `TestRequest` bằng đúng đường `w2w` admin dùng; in `NO BASELINE` tới khi ghi ở bàn. Đọc trước: `DESIGN.md` §8 *The 3 898 ns* hàng "the session's own Heartbeat serialise"; `crates/codec/benches/serialize.rs` case `encode ExecutionReport (template)` (mẫu) | một file bench session, `benches/baselines.tsv` **không** (số ở boot C) | `cargo bench -p fixbolt-session --bench <file> -- --test` chạy; `scripts/check-bench-alignment.sh` xanh | sonnet | — |
| **4.2** | PRD 7 | `crates/engine/benches/wakeup.rs`: hai thread trên hai core (tham số core như `density.rs`), A ghi 1 byte vào socket/pipe, B ngủ trong `epoll_wait` (và một arm `poll`), đo wake-to-return bằng `Instant` một chiều (cùng máy, cùng clock), n = 20 000, in p50/p99/p99.9; đây là số ADR-0014 câu hỏi 1 và ADR-0025 cần. Đọc trước: ADR-0025 *Context* bảng crossover; `crates/engine/benches/turn.rs` (mẫu harness) | `crates/engine/benches/wakeup.rs` (mới), `crates/engine/Cargo.toml` (`[[bench]]`) | `cargo bench -p fixbolt-engine --bench wakeup -- --test` chạy; alignment script xanh | sonnet | — |
| **C-40** | 40 | Bàn §9, boot C, sau 2.2: `WIRE_NIC` `hft` admin+app interval 0, **hai procedure** (ADR-0068), số thiếu in cạnh; sweep A/B interval 0/10/20/30/50 µs, 10 run/arm, admin | — | `scripts/w2w-baseline.sh` + `compare-w2w-procedures.sh`; `tx_hwtstamp_skipped` chênh = `hw-tx-missing` | manager + haiku chạy | 2.2, 2.3 |
| **C-49** | 49 | Boot C: `sudo -n perf trace -s -t <engine tid>` (hoặc `strace -T -p`, chấp nhận slowdown, chỉ lấy **chênh** giữa hai path) trong 20 000 round trip `hft` admin rồi app; trung vị `sendto`, `recvfrom` từng path; ghi vào §8 bảng *added back* dòng mới "kernel, đo tại chỗ"; ghi baseline cho case 4.1 | — | hai bảng syscall, cùng boot, cùng core | manager | 4.1 |
| **C-84** | 84 | Boot C: hai arm lệch `hft` admin loopback — engine `--tls ktls` / client userspace, engine userspace / client kTLS (cần `w2w` cho phép hai mode khác nhau hai đầu — nếu chưa có, một cờ `--client-tls`, sonnet, cùng PR 2) — hai procedure; ghi vào ADR-0070 quyết định 5 và `best-practices-hft.md` §9 | `tools/w2w/src/main.rs` (cờ, nếu cần) | `w2w-baseline.sh` với `ARMS` mới | manager | 2.x |
| **C-85** | 85 | Boot C, một biến: procedure 1 bắt đầu **≤ 2 phút** sau `cargo build --release`, procedure 2 sau **≥ 10 phút** máy rảnh, cùng arm `hft` admin `off`; header in nhiệt độ/tần số (2.3). Kết quả vào `a-tight-spread-…md` như *candidate tested* | — | `compare-w2w-procedures.sh` | manager | 2.3 |
| **C-91** | 91 | Boot C: `git worktree add ../fb-0905 <commit của dòng 2026-09-05 trong baselines.tsv>`; build; `scripts/bench.sh` cho `serialize density validate` n = 20 ở cả hai worktree, xen kẽ; so với band. Kết luận **máy** hay **code**; nếu code → bisect nêu ở *Cách làm* | — | `scripts/bench.sh --strict` hai lần, quote | manager | — |
| **C-PRD7** | PRD 7 | Boot C: chạy `wakeup.rs` n ≥ 20 run, ghi baseline; nếu p50 nằm trong 2–5 µs, ADR-0025 giữ số 4 và chuyển *Accepted*; nếu không, sửa số theo `floor(wake_p50 / turn)` ngay trong ADR-0025 (còn `Proposed`, sửa tại chỗ được) rồi *Accepted* | `docs/decisions/ADR-0025-…md`, `benches/baselines.tsv` | quote p50/p99/p99.9 và `check-machine.sh` | manager | 4.2 |

## Cách kiểm chứng

- **PR 1**: link check xanh; `grep` xác nhận không còn "fastest" ở tiêu đề `README.md`,
  `DESIGN.md` §1, `PRD.md` §1, `CLAUDE.md` đoạn đầu; mỗi hàng gạch trong `STATUS.md` trỏ đúng ADR.
- **2.1**: output script quote nguyên văn cả hai mode; đảo chiều đỏ đúng ở assertion `voluntary`
  (câu FAIL dự kiến viết trước: `hft: engine thread made N voluntary context switches, expected 0`).
- **2.2**: ba case mới của `check-w2w-baseline-summary.sh`; câu FAIL dự kiến: `hw-tx-missing 21 of
  20000 exceeds 0.1%`.
- **2.3**: test thuần với đúng 8 arm ngày 2026-09-14; đảo chiều 5 → 4.
- **3.1**: ba test; đảo chiều `Unclassified`; 59/59 không đổi.
- **4.x**: bench chạy ở chế độ `--test`; **không số nào được trích** cho tới boot C.
- **Boot C**: mọi số theo ADR-0068 (hai procedure), `check-machine.sh` trong header, và
  `DESIGN.md` §9 ghi đầy đủ; hai số ctxt-switch của 2.1 xuất hiện trong mỗi run.

## Tài liệu phải cập nhật

Manager sửa; kiến trúc sư **không** đụng ba file đầu. Số dòng đọc ngày 2026-09-18 trên
`plan/the-second-linux-desk-c`.

- [ ] `STATUS.md` *Open items* (từ dòng 4934):
  - dòng 4994 (21) và 5053 (76): thêm `~~` bao trọn ô đầu;
  - dòng 4988 (22): thêm sau câu "still open here is `recvmmsg`/`io_uring`…" → *"— answered by
    ADR-0074 decision 2, 2026-09-18"*, gạch trọn;
  - dòng 5006 (24): sau "What is NOT measured and is said so…" → *"— by decision, ADR-0074
    decision 3"*, gạch trọn;
  - dòng 5015 (14, hàng kernel bypass): gạch, trỏ ADR-0074 quyết định 1 (thêm sự thật Onload/AF_XDP);
  - dòng 5021 (36): mở đầu *"closes by ADR-0076 + PR 3 step 3.1"*, gạch sau 3.2;
  - dòng 5024 (40): *"decision taken: ADR-0071; row met when C-40 publishes the interval-0 pair"*;
  - dòng 5031 (49): *"case 4.1 + C-49 close it; the remainder is published as an in-situ kernel
    number"*;
  - dòng 5037 (55): gạch, *"closed by ADR-0075"*;
  - dòng 5062 (84): gạch, *"ADR-0070; two mixed arms owed at C-84"*;
  - dòng 5063 (85): *"comparator 2.3; cause probe C-85"*;
  - dòng 5064 (86): gạch sau PR 2, *"ADR-0072, `check-no-kernel-sleep-by-ctxt.sh`"*;
  - dòng 5065 (87): gạch, *"ADR-0073: deferred to phase 2's first §9 boot, precondition verified"*;
  - dòng 5069 (91): *"C-91 decides machine vs code"*;
  - dòng 5075 (45): gạch; thứ tự chuyển sang *Where the work is* (dòng 3532) thành 6 dòng:
    boot C (gói này) → boot D → wave D (`const-templates-in-dict`, `deterministic-simulation`)
    → phase 2 mở bằng ADR-0078/0079 → BPF `standard` stamp (ADR-0073) ở boot §9 đầu của phase 2.
  - *Not proven*: thêm "PRD 7's wakeup number (C-PRD7)", "item 49's in-situ kernel number (C-49)".
- [ ] `DESIGN.md`:
  - dòng 13 *Positioning*: bỏ "The fastest FIX acceptor that can be built" → câu ADR-0077 quyết định 2;
  - dòng 451 (`Cost of a wakeup`): thêm *"measured here by `benches/wakeup.rs` at boot C"*;
  - dòng 922 (§6 hàng `hft` never sleeps): thêm gate thứ hai `check-no-kernel-sleep-by-ctxt.sh` (ADR-0072);
  - dòng 950 (§6 NIC row): quy tắc stamp thiếu (ADR-0071), *"`standard`: ADR-0073, phase 2"*;
  - §8 *The 3 898 ns*: một dòng "kernel, in situ" chờ C-49; §8 thêm câu Logon hop (ADR-0074 quyết định 3);
  - §4 D11: một câu trỏ ADR-0070 (lý do kTLS là bảo đảm, không phải độ trễ).
- [ ] `PRD.md` §6: hàng 1 → *Answered by ADR-0077*; hàng 3 → *ADR-0078*; hàng 4 → *ADR-0079*;
  hàng 7 → *"ADR-0025 accepted once C-PRD7 records the number"*; §1 và §2 *Phase 2*: theo
  ADR-0078 quyết định 4 (encoding only, không FIXP/FAST/FIXML); dòng 266 bỏ "fastest".
- [ ] `README.md` dòng 3: câu positioning mới (ADR-0077).
- [ ] `CLAUDE.md`: đoạn đầu ("positioned as the fastest acceptor…") theo ADR-0077 — **chủ sở
  hữu sửa**, nói to trong phiên; §2 *Machine checks* hàng 4: thêm script ctxt (sau PR 2).
- [ ] `docs/best-practices-hft.md` §9 (dòng 170–195): giữ `TlsRequireKernel=Y`, thêm đoạn giá
  và điều kiện userspace theo ADR-0070 quyết định 2.
- [ ] `docs/hft-playbook.md` §6: gate ctxt chạy được trên NIC thật, không cần `sudo`.
- [ ] `docs/reference/a-transmit-timestamp-wakes-a-blocking-engine.md`: mục *What would let
  `standard` be stamped*: đổi "inferred… not verified" → "verified 2026-09-18, ADR-0073, quoted".
- [ ] `docs/reference/measured-costs.md`: sau boot C, các số C-40/C-49/C-84/C-85/C-91/C-PRD7.
- [ ] `docs/CONFORMANCE.md`: sau 3.2, điểm mirrored và trần mới, kèm CI run id.
- [ ] `CHANGELOG.md`: `w2w` in ctxt switches; `w2w-baseline.sh` quy tắc stamp thiếu;
  `compare-w2w-procedures.sh`.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Đọc `/proc/self/task/<tid>/status` **trên engine thread** sẽ thêm syscall vào chính thread đang canh | 2.1 đọc trên main thread; gate `strace` cũ vẫn chạy và tập syscall engine không đổi |
| `standard` với cửa sổ quá ngắn có thể đọc 0 voluntary → đảo chiều giả đỏ | script dùng `--hold-ms` như script cũ; assertion `> 0` nêu số đọc được |
| Ngưỡng 0,1 % tính trên **số request trong cửa sổ** (sau warmup), không trên tổng | case test 19/20 000 và 21/20 000 |
| `tx_hwtstamp_skipped` là bộ đếm **của cả card**, cộng dồn từ boot | đọc trước/sau, in chênh; chênh ≠ `hw-tx-missing` → run *marked* |
| Phân loại 34 file bằng cách đọc *tên* file thay vì dòng `I` | test thứ ba của 3.1 kiểm lại `34=0`/`123=Y` trên nội dung file |
| Điền số vào ADR-0076 từ output local thay vì CI | 3.2 yêu cầu CI run id |
| Bench `wakeup.rs` đo hai chiều cộng lại thay vì một chiều | A ghi `Instant` vào bộ nhớ chia sẻ trước khi write; B đọc sau khi `epoll_wait` trả về; cùng clock |
| C-91 so hai worktree ở hai **boot** khác nhau | cả hai chạy xen kẽ trong một boot, cùng `check-machine.sh` header |
| Sửa `CLAUDE.md` giữa phiên không có hiệu lực cho phiên đó | manager nói to dòng đã đổi (`CLAUDE.md` đoạn mở đầu) |
| ADR `Proposed` bị coi là đã quyết | không bước code nào của PR 2/3 bắt đầu trước khi ADR tương ứng *Accepted* |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Chủ sở hữu không đồng ý bỏ chữ "fastest" (ADR-0077) | Trung | ADR là `Proposed`; nếu bác, hàng PRD 1 giữ mở với lý do ghi rõ, plan không chờ |
| Bước 3.1 tìm thêm defect session (như 0 → 10 ngày 2026-09-02) | Trung | item mới, bất biến 3, plan riêng; trần ADR-0076 vẫn điền được (file đó là `Reachable` và đỏ) |
| Sweep C-40 cho thấy interval 0 luôn vượt 0,1 % | Thấp | ngưỡng là quyết định trong ADR-0071; số sweep là bằng chứng để lật nó bằng ADR mới, không phải để nới tay |
| `w2w` chưa cho hai đầu hai mode TLS khác nhau (C-84) | Thấp | cờ `--client-tls`, sonnet, trong PR 2 |
| Kernel `7.0.0-31` là nguyên nhân item 91 nhưng không quay lại `-30` được | Trung | kết luận "máy" vẫn là kết luận; ghi lại baseline theo Q6 của boot B; ADR-0016 cho phép |
| Phase 2 theo ADR-0078/0079 bị hiểu là "đã hỗ trợ iLink 3" | Trung | `PRD.md` §2 dùng đúng chữ *encoding only, no session* |
| Boot C quá dài: C-40 (2 procedure + sweep) + C-84 (4 arm × 2) + C-91 + C-85 + C-49 + C-PRD7 | Cao | thứ tự cắt nếu hết giờ: **C-40 → C-91 → C-PRD7 → C-49 → C-85 → C-84**; phần chưa chạy sang boot D, ghi ở *Not proven* |

## Ngoài phạm vi

- Item **89** (listener cadence) và nhánh mitigations của **51** — đang đo; plan này không đọc
  kết quả của chúng.
- **Xây** BPF sock_ops cho `standard` (ADR-0073 quyết định 2) — phase 2.
- **Trả nợ** indexing (ADR-0075) — không có plan riêng.
- Bất kỳ thay đổi nào ở `crates/session`, `crates/codec` — kể cả khi 3.1 tìm ra defect.
- Đo `hft` đối đầu với engine khác (ADR-0077 quyết định 4).
- Mua NIC `igc` (ADR-0071 quyết định 4) — nêu tên, không mua.
- Bench `wakeup.rs` không thay số 4 trong ADR-0025 cho tới khi có số (C-PRD7).

## Nhật ký giao hàng

*(trống — điền khi đóng từng PR: nhánh, commit, gate quote, CI run id, cái gì chưa chứng minh)*
