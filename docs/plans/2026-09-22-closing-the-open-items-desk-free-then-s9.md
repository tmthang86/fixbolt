# Đóng các item còn mở: phần không cần bàn đo trước, rồi reboot vào dòng §9 đóng phần còn lại

> **Loại:** Plan · **Ngày:** 2026-09-22 · **Trạng thái:** Đã duyệt 2026-09-22 (manager, theo uỷ quyền 2026-09-18)
> **Phạm vi:** item 93, 95, 96, 97 của `STATUS.md`; ba gạch đầu dòng *Not proven* không cần §9
> (Mac mini ×2, hàng `no timer due`); ghi lại baseline theo ADR-0090 quyết định 4.

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Chủ dự án ra lệnh ngày 2026-09-22: *"đóng mọi item còn mở không cần máy §9; rồi reboot vào dòng
grub §9 và đóng phần còn lại."* Kế hoạch này là **một** kế hoạch, **hai pha**, ranh giới cứng là
một lần reboot. Reboot giết phiên làm việc, và manager tiếp theo chỉ có `STATUS.md` *Start here*
— nên **mọi thứ pha 2 cần (worktree, binary, manifest, sha256) được dựng xong ở pha 1**, trước
khi reboot, và pha 2 **không biên dịch gì** (ADR-0090 quyết định 2).

Bốn item mở là bốn góc nhìn của **một** cái chậm: engine turn tăng ~4–9% qua năm đoạn bisect từ
2026-09-05 (item 93), `validate Heartbeat` tăng 12,32% trong span PR B/C/#85/#86 (item 95), tám
median trên `main` đã vượt đường baseline đã ghi trên bàn đo (item 97), và thí nghiệm định giá
descent của ADR-0086 chưa chạy (item 96). ADR-0090 quyết định 4 giữ baseline **không dời** cho tới
khi item 93 có đủ verdict — nên thứ tự bắt buộc là: gọi tên từng đoạn (pha 1, không cần bàn đo vì
profile đã ghi), sửa cái sửa được, **rồi** đo trên §9 và dời baseline **một lần**.

## Những gì đã biết chắc

### Về máy và bằng chứng — đọc trực tiếp 2026-09-22, 20:45–20:55 (+07)

- Bàn đo là host `tmt-B450-I-AORUS-PRO-WIFI`, đang ở **dòng grub desktop**: `/proc/cmdline`
  không có `isolcpus`. Bản sao dòng §9 ở `/etc/default/grub.fixbolt-backup-20260919-bootc`, đọc
  được: `GRUB_CMDLINE_LINUX_DEFAULT="quiet splash isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15
  processor.max_cstate=1"`. `sudo -n true` exit 0. Helper `/usr/local/sbin/fixbolt-machine` có.
- **`scripts/check-machine.sh` nay có 17 hàng, không phải 16**: chạy hôm nay trên dòng desktop
  đọc `pass 6   fail 10   unknown 1` (= 17), vì PR #91 thêm hàng `no timer due`. **Kỳ vọng ở §9
  là `pass 17 fail 0 unknown 0`.** Mọi tài liệu cũ ghi `pass 16` là số của trước PR #91.
- Hàng `no timer due` **đã hiện hình `FAIL` live** trên máy systemd thật này (điều *Not proven*
  nói chưa ai thấy): `FAIL   no timer due   sysstat-collect.timer next 2026-09-22T13:50Z (in
  0m51s), … apt-daily-upgrade.timer next 2026-09-22T23:18Z (in 9h29m) [window 12h]`, kèm dòng
  `fix:` liệt kê 13 lệnh `sudo -n systemctl stop <timer>`. Hình `PASS` chưa thấy — bước P4.
  `systemctl is-system-running` → `degraded` (systemd thật, PID 1 có).
- `target/boot-d-evidence/` còn nguyên (921 MB): 14 file `d2-<sha>-{1,2}.data` (~64 MB, ~1,68 M
  mẫu mỗi file, ví dụ `d2-0149b26-1.data` `1678921 samples`), 7 file `.stat`; **file thuộc
  `tmt`, mode 600, đọc được không cần sudo**. `perf 7.0.14` ở `/usr/bin/perf`.
- **`perf buildid-list` phân giải được binary**: `d2-0149b26-1.data` trỏ tới
  `/home/tmt/Projects/fb-boot-d/e1/target/release/deps/density-afb0b72f28974c88`, file còn trên
  đĩa; bảy worktree `e1`…`e7` vẫn checkout đúng sha (`git worktree list`).
- **`perf diff` chạy được trên hai bản ghi của hai binary khác nhau, 2,0 s một cặp**, khớp ký
  hiệu theo **tên**, và tên Rust đã demangle **không có hash `::h…`** (ví dụ
  `fixbolt_session::scan_fields::<64>`, `fixbolt_session::clock::parse_utc`). Thử `e1-1` ↔ `e2-1`
  với `-c delta-abs -s symbol -o 1`: `scan_fields` 35,38 % → −1,77 %, `__memcmp_avx2_movbe`
  9,87 % → +1,31 %, `Session::judge` 7,43 % → +0,61 %, `FieldType::accepts` 4,11 % → +0,51 %.
  **`--percent-limit` không phải tuỳ chọn của `perf diff`** (`Error: unknown option
  'percent-limit'`); nó là của `perf report`. Ký hiệu kernel in `[k] 0xffff…` không phân giải
  (không cần cho việc này).
- Cột `Delta Abs` là **hiệu số phần trăm mẫu**, không phải nano-giây. Muốn ra ns phải nhân tỷ lệ
  với `ns/op` của chính run đó (dòng `engine turn, 1 busy sessions` trong `d2-<sha>-<k>.out`).
- `d4/` và `d4b/` **đã có `runs.reextracted.txt`** (20:47 hôm nay, runner của manager chạy
  `--reextract`): `d4` 1248 hàng ở vòng ≤ 12, `d4b` 832 hàng ở vòng 1–8. `d4m/` không có `raw/`
  (là bản ghép của hai thư mục kia, vòng của `d4b` đánh lại 13–20; `d4m/timeline.txt` có 20 dòng
  `complete`). `--summary` đọc `timeline.txt` **cùng thư mục** với file runs được đưa vào
  (`ab-rotation.sh:324`). Đếm thẳng trên hai file reextract: `w1s validate NewOrderSingle` over
  12 + 8 = 20, `w1 engine turn, 1 busy, ring 64` 12 + 8 = 20 (hàng 13 của `d4` thuộc vòng
  incomplete), `w1 validate TestRequest, w2w bytes` 9 + 8 = 17.
- **C1 của ADR-0094 đúng hôm nay**: `nm -C ../fb-boot-d/w1s/target/release/deps/validate-ea1ac0706c5fc955
  | grep -c bad_nested_count` → **3** (`<Fixt11Fix50Sp2Tables, 256>`, `<…, 64>`, `<Fix44, 64>`);
  `bad_group_count` → 0 ký hiệu (đã inline — C3 của ADR-0094 áp dụng theo hướng ngược lại: số
  đo là cận dưới nếu tầng đầu inline).
- Span item 95 theo first-parent `6fbe851..76e53cb`: `29be3bd` (#83, PR C), `e673e8f` (#82,
  PR B), `71a1dc7` (#84 docs), `64ea6c2` (#85), `3f84a81` (#86), `c5ab56d` (docs), `a451831`
  (#87), `76e53cb` (#88). **Chỉ ba bước đụng `crates/session`, `crates/dict`, `crates/codec`**
  (`git diff --stat <prev> <c> -- crates/session crates/dict crates/codec`): `e673e8f` (20 file),
  `3f84a81` (9 file), `a451831` (4 file). Bench `validate` là `fixbolt-session` — không link
  `engine` — nên bisect chỉ cần **ba worktree mới**.
- Từ `c5ab56d` (cha của arm `1825d8c`) tới `main` `d32f8c5`: `crates/session/src/clock.rs` và
  `crates/dict/src/field_type.rs` **không đổi** (diff rỗng) → arm rebase lên `main` không xung
  đột ở file sửa. Arm `ab/parse-utc-fast-path` = `1825d8c`, đổi `clock.rs` (+109/−37) và thêm
  `tests/parse_utc_equivalence.rs` (327 dòng); worktree `../fb-ab-parse` sạch.
- Mac mini: `ssh thangtran@192.168.77.2` bằng key, `Darwin 25.6.0 arm64`, checkout
  `~/Projects/nanofixengine` ở **`ece17e7`** (cũ, PR #78), sạch; `cargo`, `rustc` có; **bash
  3.2.57**; `net.link.loopback.sched_model: 0`. Script contention có ở cả `bd6be07` (cây trước
  `3233032`) lẫn `main`; cách gọi `scripts/check-socket-corpus-under-contention.sh <rounds>
  <copies> [wire|wire_fixt|both]`. `scripts/check-machine-verdicts.sh` trên desk hôm nay:
  `pass 63   fail 0`.
- Đĩa: `/` còn 155 G; `../fb-boot-d` 1,1 G. `scripts/check-bench-alignment.sh --flags` →
  `-C llvm-args=-align-all-functions=6`.

### Về quy tắc đã có

- ADR-0090 quyết định 4: mỗi đoạn của item 93 được **một** trong ba verdict — *fix* (arm dựng
  trước boot, thắng thì thành PR có senior review), *accept, named*, *accept, unnamed* (IPC delta
  thay tên hàm); baseline `engine turn *` và `validate *` **dời đúng một lần**, sau verdict cuối,
  từ median 20 vòng của arm `main` **có feature** của **chính boot đó**.
- ADR-0090 quyết định 2, 3, 5; ADR-0092 quyết định 3 (`--reextract`, cột `over`); ADR-0093
  quyết định 1 (driver chiến dịch nằm trong `scripts/`, `sudo` chỉ gọi đường dẫn tuyệt đối,
  không có `cargo` sau `sudo` — `secure_path` không có cargo); ADR-0094 quyết định 1–4; ADR-0031
  (band `[b/m, b·m]`, `Under` là báo cáo, n ≥ 20 mới là baseline); ADR-0091 quyết định 2 (hai đếm
  trên Mac: ≥ 1 đỏ trong 50 ở cây trước `3233032` với cặp `FieldCount { expected: 14, actual: 8 }`,
  rồi `0 red in 50` và `lifeline hit: 0` ở cây sau).
- Boot D ([measured-costs](../reference/measured-costs.md) *Boot D*): `d4` mất 8 vòng vì
  `apt-daily-upgrade` + `packagekit`; timer phải **stop, không disable**, sau khi boot (stop
  không sống qua reboot). Run bench đầu tiên sau reboot **bỏ**. Segment (3): `w1` → `w2`
  −33,1 ns trên `engine turn, 1 busy sessions`, luật ≥ 25 ns xác nhận đã tuyên bố trước.
- C-91b (*Boot C*): bisect với judge median 3 run, đường 5 % ở 1 743 ns; D2 đọc dốc +4,1 %
  (+68 ns) chứ không phải +9,5 %; hai thủ tục khác nhau, không cái nào bác cái nào.

### Từ internet — tra 2026-09-22, mỗi dòng một nguồn

- **`perf diff` khớp theo tên ký hiệu, file đầu là baseline, mặc định `delta-abs`**: *"As the
  perf.data files could come from different binaries, the symbols addresses could vary. So perf
  diff is based on the comparison of the files and symbols name"*; *"The baseline perf.data file
  is iterated for samples. All other perf.data files … are searched for the baseline sample
  pair"*; `-c delta, delta-abs, ratio, wdiff, cycles (default delta-abs)`; `-S` lọc ký hiệu;
  `-o` sắp theo cột tính — [perf-diff(1)](https://man7.org/linux/man-pages/man1/perf-diff.1.html),
  và patch gốc [perf diff: Support for different binaries](https://lore.kernel.org/lkml/1416585348-14762-1-git-send-email-kan.liang@intel.com/).
  Kiểm chứng tại chỗ ở mục trên (2,0 s, tên khớp).
- **Quy đổi hồi quy nhỏ (2–3 %) từ profile**: differential profiling so **hai profile của hai
  lần chạy**, và một hàm chỉ tăng 0,5 % → 0,6 % vẫn có thể là thủ phạm nếu nằm trên đường nóng —
  [Practical Differential Profiling, Schulz & de Supinski](https://www.osti.gov/servlets/purl/914615);
  [Differential flame graphs (Bezemer et al.)](https://www.researchgate.net/publication/282681970_Understanding_software_performance_regressions_using_differential_flame_graphs).
  Không nguồn nào nói cách đổi *tỷ lệ mẫu* thành *ns* — kế hoạch tự làm: `ns = tỷ lệ × ns/op
  của chính run`, và **cặp cùng-binary (`<sha>-1` ↔ `<sha>-2`) là sàn nhiễu** của tỷ lệ mỗi
  ký hiệu. Đó là thiết kế của kế hoạch này, không phải trích dẫn.
- **Bisect với judge ồn**: rustc-perf coi kết quả *significant* khi vượt `Q3 + 3 × IQR` của
  lịch sử **từng benchmark**, và chỉ báo khi "definitely relevant" —
  [rustc-perf comparison-analysis](https://github.com/rust-lang/rustc-perf/blob/master/docs/comparison-analysis.md).
  MongoDB dùng change-point (E-Divisive) trên chuỗi thời gian thay vì ngưỡng cố định —
  [Change Point Detection in Software Performance Testing](https://arxiv.org/pdf/2003.00584),
  [Hunter](https://arxiv.org/pdf/2301.03034). Bài học lấy về: **ngưỡng phải đọc từ phân tán của
  chính case đó**, không phải một số cố định — ở đây phân tán đã đo: `max/median` 1,008–1,035
  trên bốn case `validate` của `w1` (boot D). Kế hoạch dùng luật: một bước được gán cho một đoạn
  khi hiệu median của đoạn ≥ 3 × (max/median − 1) của case đó ở hai arm hai đầu — [ADR-0095](../decisions/ADR-0095-a-drift-is-read-at-re-record-time-not-by-a-wider-band-a-segment-is-named-by-a-rule-and-the-desk-runs-strict-at-every-boot.md) quyết định 3.
  Tìm "median-of-n bisect judge" chỉ ra bài viết tổng quát, không có thủ tục nào đáng trích hơn
  C-91b của chính repo.

## Cách làm

### Pha 1 — trên dòng desktop, boot hiện tại, không số nào công bố

1. **7a sửa lại** (P1): `d4m/` không có `raw/`, nên `--reextract` chạy trên `d4` và `d4b` (đã
   chạy), rồi **ghép** thành `d4m/runs.reextracted.txt` bằng đúng luật đã ghép `d4m/runs.txt`:
   hàng của `d4` ở vòng ≤ 12 giữ nguyên, hàng của `d4b` cộng 12 vào cột vòng. `--summary` với
   `CONTROL=w1s` trên file ghép (timeline của `d4m` đã đánh lại số vòng). **Cách đọc tám median
   của item 97**: cột `median` của tám cặp (arm, case) trong bảng *`main` is over its recorded
   baselines* của measured-costs phải bằng số ở đó (sai lệch làm tròn một chữ số), và cột `over`
   của mỗi cặp phải là `k/20` với **k ≥ 10** (median vượt trần ⇒ ít nhất nửa số hàng vượt); footer
   `over baseline: N (arm, case) pairs` với N ≥ 8. Không cặp nào của `w0` trong footer.
2. **Bốn đoạn còn lại của item 93** (P2): cho mỗi đoạn (1) `e1→e2`, (2) `e2→e3`, (4) `e5→e6`,
   (5) `e6→e7`: năm `perf diff` — bốn cặp chéo (`a-1↔b-1`, `a-2↔b-2`, `a-1↔b-2`, `a-2↔b-1`) và
   một cặp cùng binary mỗi đầu (`a-1↔a-2`, `b-1↔b-2`) làm **sàn nhiễu**. Mỗi ký hiệu: `ns_a =
   share_a × turn_a`, `ns_b = share_b × turn_b` (turn đọc từ `.out` của run đó), `Δns`. Verdict
   theo ADR-0095 quyết định 3: **named** khi ký hiệu có |Δns| lớn nhất cùng dấu ở cả bốn cặp
   chéo và |Δshare| ≥ 2 × |Δshare| của nó ở cặp cùng-binary; **fix** khi named và người phân
   tích nêu được một thay đổi code cụ thể có thể dựng thành arm trước reboot; ngược lại
   **accept, unnamed** với IPC từ hai file `.stat`. Với `Engine::turn` inline, `perf annotate
   --stdio -s <symbol> -i <data>` là bước cuối, thủ công.
3. **Segment (3) thành PR sửa thật** (P3): rebase `ab/parse-utc-fast-path` lên `main` thành
   `fix/parse-utc-fast-path`, senior developer xây, senior review, merge trước reboot để arm
   `main` của pha 2 mang nó. Không đổi hành vi: `tests/parse_utc_equivalence.rs` giữ hai nhánh
   cùng đáp án với luật ADR-0058 ở mọi độ dài tới 40 và 40 000 input nhiễu.
4. **Hàng `no timer due` hai hình** (P4): quote hình `FAIL` hiện tại, chạy đúng dòng `fix:` của
   hàng, chạy lại → hình `PASS`, rồi `systemctl start` lại các timer vừa stop (máy đang là
   desktop, không được để timer chết).
5. **Mac mini hai món nợ** (P5): đưa checkout về `main`, fetch `vendor/`, chạy
   `check-machine-verdicts.sh` (hình `date` BSD của F8); rồi ADR-0091 quyết định 2 đúng như ADR
   viết (cây `bd6be07` với script từ `main`, `5 10 both`, rồi `main`).
6. **Dựng mọi thứ pha 2 cần** (P6): worktree `../fb-s9e/{b1,b2,b3,m,ms}` (+ `f<k>` nếu P2 ra
   arm fix), bench `--no-run` với `RUSTFLAGS` của `bench.sh`, `MANIFEST.txt` sha256; **cây
   `main` chính** cũng `cargo bench --no-run` mọi bench để `bench.sh --strict` ở pha 2 không biên
   dịch; `nm` C1 trên binary `validate` của `ms`.
7. **Handoff + ranh giới reboot** (P7, R): `STATUS.md` *Start here* là brief cho manager pha 2;
   commit, push, CI xanh; rồi khôi phục grub §9 và reboot.

### Pha 2 — dòng §9, không biên dịch

- **S0**: vào §9, `check-machine.sh` → `pass 17 fail 0 unknown 0`, run bench đầu bỏ.
- **S1 — một vòng xoay phục vụ ba việc** (ADR-0090 quyết định 3): arms `wa`, `b1`, `b2`, `b3`,
  `w1` (validate, không feature — bisect item 95); `m` (validate + density, không feature — cầu
  nối với `w1`, đo fix segment (3) đã merge vào `main`); `ms` (validate + density, `fix50sp2`,
  **CONTROL**, ứng viên baseline duy nhất — ADR-0090 quyết định 5); `f<k>` nếu có. `ROUNDS=20`,
  `--summary` tại vòng 10 và 20.
- **S2 — item 96**: ADR-0094 quyết định 1 nguyên văn trên binary `validate` của `ms`, k = 1…5,
  đường dẫn tuyệt đối, sha256 khớp manifest trước mỗi run.
- **S3 — `bench.sh --strict` trên `main` trước khi dời baseline**: kỳ vọng **đỏ** (item 97 nói
  "would be red" — giờ là *observed*, không phải *inferred*), quote các dòng `OVER BASELINE`.
- **S4 — kết luận item 93 và dời baseline một lần**: điều kiện — cả năm đoạn có verdict (P2 +
  segment (3) đã merge). `benches/baselines.tsv`: dòng `engine turn *` và `validate *` ghi từ
  median n = 20 của `ms`, `n=20`, ngày; các dòng khác không đụng. `bench.sh --strict` chạy lại →
  xanh. Commit trên desk (tsv đọc lúc chạy, ADR-0067 — không build).
- **S5 — handoff**, và về dòng desktop là **quyết định của chủ dự án**, lệnh ghi sẵn.

## Bất biến bị đụng tới

| §2 | Bước | Giữ bằng |
|---|---|---|
| 1 — không cấp phát hot path | P3 | `cargo bench -p fixbolt-session --bench alloc` đọc 0 ở mọi case, quote |
| 2 — session thuần | P3 | diff chỉ trong `parse_utc`, không `format!`, không clock; clippy `-D warnings` |
| 3 — 59 định nghĩa | P3 | `cargo test -p fixbolt-conformance` in `59 / 59`; **không sửa fixture**; corpus FIXT với `--features fix50sp2` |
| 7 — không unwrap | P3 | clippy workspace lints; `unwrap_or` trong closure của nhánh chung là code cũ giữ nguyên |
| 10 — không số không nguồn | P2, S1–S4 | mọi số kèm sha, feature, `n`, dòng máy; số pha 1 **không công bố** (dòng desktop) |
| 4 — hai chế độ | không | không đổi wait strategy, không đổi `pump` |

P2 không đụng code (chỉ đọc profile). P4, P5, P6 không đụng `crates/`. Thay đổi `scripts/`: không
có — nếu P2 hay P6 cần script chạy pha 2 (ADR-0093 quyết định 1), đó là bước riêng cho developer,
**không** viết tay trong boot.

## Chia việc

### Pha 1 — dòng desktop, trước reboot

| Bước | Vai trò / model | Kết quả | File đụng (không đụng gì khác) | §2 | Gate (lệnh) | Xong khi | Quote về | Phụ thuộc |
|---|---|---|---|---|---|---|---|---|
| **P0** | manager | Nhánh `plan/closing-the-open-items` từ `main` `d32f8c5`; PR draft; ADR-0095 `Proposed` đã có trên nhánh | `docs/plans/…`, `docs/decisions/ADR-0095-…` | — | `python3 scripts/check-links.py`; `scripts/check-adr-numbers.sh` | cả hai xanh | hai output nguyên văn | — |
| **P1** | runner (haiku), brief tự chứa | 7a sửa lại: `E=target/boot-d-evidence; awk -F'\t' 'BEGIN{OFS="\t"} $2<=12' $E/d4/runs.reextracted.txt > $E/d4m/runs.reextracted.txt; awk -F'\t' 'BEGIN{OFS="\t"} {$2=$2+12; print}' $E/d4b/runs.reextracted.txt >> $E/d4m/runs.reextracted.txt; wc -l $E/d4m/runs.reextracted.txt` (kỳ vọng **2080**); `CONTROL=w1s scripts/ab-rotation.sh --summary $E/d4m/runs.reextracted.txt > $E/d4m/summary.reextracted.txt; cat $E/d4m/summary.reextracted.txt` | chỉ `target/boot-d-evidence/d4m/` (file mới; **không** mở `runs.txt` để ghi) | 10 | lệnh trên | `wc -l` = 2080; tám dòng (arm, case) của bảng *`main` is over its recorded baselines* có median khớp và `over` ≥ 10/20; footer `over baseline: N` với N ≥ 8; không dòng `w0` nào có `over` ≠ 0/20 | `wc -l`, tám dòng summary nguyên văn, footer | P0 |
| **P2** | senior developer (opus) — phân tích, không sửa code | Bốn đoạn có verdict. Cho mỗi đoạn `(a,b)` ∈ {(e1,e2),(e2,e3),(e5,e6),(e6,e7)}: `perf diff -c delta-abs -s symbol -o 1 $E/d2-<a>-<i>.data $E/d2-<b>-<j>.data` cho 4 cặp chéo + `a-1↔a-2` + `b-1↔b-2` (**không** dùng `--percent-limit`); bảng `symbol, share_a, share_b, turn_a, turn_b, ns_a, ns_b, Δns` cho mọi ký hiệu có \|Δshare\| ≥ 0,3 ở cặp `1↔1`; ngưỡng nhiễu = \|Δshare\| cùng-binary; IPC hai đầu từ `.stat`; verdict theo ADR-0095 quyết định 3; nếu *fix*: mô tả thay đổi code (file, hàm, một câu) — **không viết code ở bước này** | ghi `target/boot-d-evidence/d2-analysis/seg<k>.txt` (bảng + lệnh) và mục mới `### Item 93 — five segments named` trong `docs/reference/measured-costs.md` ngay sau mục *Item 93 — the bisect, profiled…*; **không** đụng `STATUS.md`, `crates/` | 10 | `python3 scripts/check-links.py` | bốn verdict, mỗi cái một câu + tên hàm hoặc "unnamed, IPC a→b" | bốn verdict; bảng top-5 Δns mỗi đoạn; lệnh nguyên văn của một cặp; điều gì mơ hồ — dừng và nói | P0 |
| **P3** | senior developer (opus); rồi **senior review** (opus, context mới) | Nhánh `fix/parse-utc-fast-path` = `ab/parse-utc-fast-path` rebase lên `main`; commit message `perf(session): a straight-line 21-byte path in parse_utc (item 93 segment 3)`; `CHANGELOG.md` một dòng; `docs/internals/session.md` một câu nếu trang có mục `clock.rs` | `crates/session/src/clock.rs`, `crates/session/tests/parse_utc_equivalence.rs`, `CHANGELOG.md`, có thể `docs/internals/session.md`; **không** sửa `timestamp_widths.rs`, `timestamp_precision.rs`, `crates/dict/` | 1, 2, 3, 7 | `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `cargo test -p fixbolt-session -p fixbolt-dict -p fixbolt-conformance`; `for p in fixbolt-codec fixbolt-dict fixbolt-session fixbolt-engine; do cargo test -p $p --tests --features fix50sp2; done` (*Sửa 1*: `fixbolt-conformance` không có feature `fix50sp2`; đây là vòng lặp CI dùng)

> **Sửa 2 (2026-09-23, manager, trong lúc chạy S3):** S4 viết "`--strict` → xanh" nhưng `--strict` đã đỏ về cấu trúc từ 2026-09-18 vì hai case `wakeup` p50 không có cơ chế baseline (item 100), và 6 case mới (4 `sbe`, 2 `validate` FIXT) chưa có dòng nào cho CPU này. Quyết định: ghi 6 dòng đó từ dữ liệu n ≥ 20 của chính boot này (`ms` n = 21 cho FIXT; một rotation riêng `s1c` n = 24 cho `sbe`), **không** đụng `wakeup`, và S4 "xong" đọc là "mọi dòng đã ghi nằm trong band, đỏ chỉ còn ở `wakeup`" — nói rõ trong STATUS.; `cargo test --no-default-features`; `cargo bench -p fixbolt-session --bench alloc`; `git diff --exit-code main -- crates/session/tests/timestamp_widths.rs crates/session/tests/timestamp_precision.rs` | `59 / 59`; alloc 0 mọi case; mọi gate exit 0; PR riêng (không phải PR của plan) xanh CI, senior review không finding mở; **merge vào `main`** (theo uỷ quyền 2026-09-18) | `59 / 59` dòng nguyên văn; alloc rows; diff stat; CI run id | P0 |
| **P4** | runner (haiku), brief tự chứa | Hai hình của hàng `no timer due` trên máy systemd thật: `FIXBOLT_NIC=enp9s0 scripts/check-machine.sh \| grep -A1 'no timer due'` (quote, phải bắt đầu `FAIL`); chạy **nguyên văn** dòng `fix:` vừa in (chuỗi `sudo -n systemctl stop …`); chạy lại lệnh grep (quote, phải bắt đầu `PASS   no timer due`); rồi `sudo -n systemctl start` **cùng danh sách** timer; `systemctl list-timers --all \| grep -c '\.timer'` trước và sau bằng nhau | không file repo; manager ghi `STATUS.md` *Not proven* (gạch bullet) và một dòng `[seen live 2026-09-22]` trong `docs/reference/a-quiet-machine-check-cannot-see-a-timer-that-has-not-fired.md` mục *Guarded by* | — | lệnh trên | hai hình quote được, timer đã start lại | ba output nguyên văn | P0 |
| **P5** | developer (sonnet) | Mac mini, mọi lệnh qua `ssh thangtran@192.168.77.2`: (a) `cd ~/Projects/nanofixengine && git fetch origin && git checkout main && git pull --ff-only && git rev-parse --short HEAD` (= sha `main` sau P3 merge); `scripts/fetch-quickfix-assets.sh`; `bash scripts/check-machine-verdicts.sh \| tail -1` → `pass 63   fail 0` (hoặc số của `main` lúc đó — quote); (b) ADR-0091 quyết định 2: `git worktree add ../fb-pre3233032 bd6be07`; trong đó `cargo test -p fixbolt-engine --features fix50sp2 --test wire --test wire_fixt --no-run`; `cp ~/Projects/nanofixengine/scripts/check-socket-corpus-under-contention.sh scripts/`; `scripts/check-socket-corpus-under-contention.sh 5 10 both` → kỳ vọng `N red in 50`, N ≥ 1, có cặp `FieldCount { expected: 14, actual: 8 }`; (c) trên `main`: build cùng lệnh, chạy `5 10 both` → `0 red in 50`, `lifeline hit: 0`; quote `uname -a`, `sysctl net.link.loopback.sched_model` | `docs/CONFORMANCE.md` §9 (hai đếm + dòng máy, cạnh dòng 60 / 60); **không** sửa ADR-0091 (Accepted); `STATUS.md` do manager | — | lệnh trên; `python3 scripts/check-links.py` | ba output đúng hình; nếu (b) đọc `0 red in 50` → **dừng**, đó là ADR-0091 bị bác đến lượt nó, về architect | output (a)(b)(c) nguyên văn, sha Mac | P3 (để Mac ở `main` đã có fix — không bắt buộc, nhưng tránh chạy hai lần) |
| **P6** | runner (haiku), brief liệt kê từng lệnh | Dựng pha 2: `M=$(git rev-parse --short main)`; `git worktree add ../fb-s9e/b1 e673e8f`, `b2 3f84a81`, `b3 a451831`, `m $M`, `ms $M`; `F="$(scripts/check-bench-alignment.sh --flags)"`; trong `b1 b2 b3 m`: `RUSTFLAGS="$F" cargo bench --no-run -p fixbolt-session --bench validate`; trong `m` thêm `… -p fixbolt-engine --bench density`; trong `ms`: `RUSTFLAGS="$F" cargo bench --no-run -p fixbolt-session --bench validate --bench alloc --features fix50sp2` và `… -p fixbolt-engine --bench density --bench alloc --features fix50sp2`; **cây `main` chính**: `RUSTFLAGS="$F" cargo bench --no-run --workspace --features fix50sp2` (để `bench.sh --strict` không biên dịch — hoặc đúng lệnh `bench.sh` gọi, đọc `scripts/bench.sh` để lấy); mỗi cây `scripts/check-bench-alignment.sh` xanh; `../fb-s9e/MANIFEST.txt` cùng cột như `../fb-boot-d/MANIFEST.txt` (`name sha package bench features rustflags binary_path sha256`); `nm -C <ms validate bin> \| grep -c bad_nested_count` ≥ 1 (C1); `df -h /` | ngoài repo (`../fb-s9e/`); nếu P2 ra arm fix: developer (sonnet) dựng nhánh `ab/seg<k>-…` trước, rồi P6 thêm `f<k>` (density, không feature) | 10 | `check-bench-alignment.sh` từng cây; `sha256sum` khớp manifest | manifest ≥ 9 dòng (b1,b2,b3 validate; m validate+density; ms validate+density+2 alloc), alignment xanh mọi cây, C1 ≥ 1, `cargo bench --no-run` trong `main` lần hai in **không** dòng `Compiling` | manifest nguyên văn; C1; `df -h` | P2 (arm fix, nếu có), P3 (merge) |
| **P7** | manager | Handoff: `STATUS.md` *Start here 2026-09-22 (pha 1 đóng)* — cái gì đóng, sha `main`, manifest `../fb-s9e/MANIFEST.txt`, **lệnh S0–S5 nguyên văn** (chép từ bảng dưới), thứ tự cắt, do-not list; delivery log pha 1; `STATUS.md` hàng 93 (4 verdict), 97 (7a đọc xong), *Not proven* (Mac, timer gạch) | `STATUS.md`, plan này | — | `check-links.py`; `check-adr-numbers.sh`; CI của PR xanh **trên commit handoff** | CI run id ghi vào *Start here* | run id | P1–P6 |

### Ranh giới — bước R, manager chạy tay, mỗi lệnh quote

Điều kiện vào: P7 đã push, CI xanh trên commit handoff (đợi run kết thúc — push mới huỷ run
cũ), không phiên nào khác đang đo trên bàn (memory: *parallel session bench campaign*).

```sh
# 1. Lưu dòng desktop hiện tại, khôi phục dòng §9 (isolcpus, không nohz_full, không mitigations)
sudo -n cp /etc/default/grub /etc/default/grub.fixbolt-desktop-20260922
sudo -n cp /etc/default/grub.fixbolt-backup-20260919-bootc /etc/default/grub
sudo -n grep CMDLINE /etc/default/grub      # phải có isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1
sudo -n update-grub
# 2. Cây sạch, không worktree nào bẩn
git status --porcelain                        # rỗng
for d in ../fb-s9e/*/ ../fb-boot-d/*/; do git -C "$d" status --porcelain | grep -v '^?? vendor' ; done   # rỗng
# 3. Reboot — phiên này kết thúc ở đây
sudo -n reboot
```

Sau reboot, **phiên mới**, manager mới đọc `STATUS.md` *Start here* và chạy S0:

```sh
cat /proc/cmdline                                             # có isolcpus=6,7,14,15
sudo -n /usr/local/sbin/fixbolt-machine on && sudo -n /usr/local/sbin/fixbolt-machine status
sudo -n ethtool -C enp9s0 rx-usecs 0
for i in $(grep enp9s0 /proc/interrupts | cut -d: -f1); do echo 4 | sudo -n tee /proc/irq/$i/smp_affinity_list; done
sudo -n ethtool --set-eee enp9s0 eee off
# timers: stop (không disable) ĐÚNG danh sách hàng `no timer due` in ra — chạy check trước, chép dòng fix:
FIXBOLT_NIC=enp9s0 scripts/check-machine.sh | grep -A1 'no timer due'
#   → chạy nguyên văn dòng `fix:` (sudo -n systemctl stop … cho từng timer), rồi:
FIXBOLT_NIC=enp9s0 scripts/check-machine.sh                   # kỳ vọng: pass 17 fail 0 unknown 0
ps -eo pcpu,comm --sort=-pcpu | head -5                       # không llama-server, không chrome
head -3 ../fb-s9e/MANIFEST.txt
# run đầu tiên sau reboot — BỎ, không ghi
../fb-s9e/ms/target/release/deps/density-<hash> >/dev/null
```

Nếu `check-machine.sh` khác `pass 17 fail 0 unknown 0`: sửa theo dòng `fix:` của hàng đỏ, chạy
lại; nếu vẫn đỏ → không đo, ghi handoff, dừng.

### Pha 2 — dòng §9, thứ tự chạy = thứ tự giá trị mỗi phút

| Bước | Item | Vai trò | Lệnh (nguyên văn) | Loại run khi | Xong khi | Quote về | Ghi vào | Ước lượng |
|---|---|---|---|---|---|---|---|---|
| **S0** | — | manager | khối trên | `check-machine.sh` ≠ `pass 17 fail 0 unknown 0` | quote đủ | `/proc/cmdline`, toàn bộ `check-machine.sh`, manifest head | measured-costs *Boot E — Settings in force* | 20 phút |
| **S1** | 95, 93(3) đã merge, baseline | manager chạy driver (một lệnh); haiku quote summary | `ROUNDS=20 CONTROL=ms EVIDENCE=target/boot-e-evidence/s1 ARMS="wa=../fb-boot-d/wa:fixbolt-session/validate@nofeat b1=../fb-s9e/b1:fixbolt-session/validate@nofeat b2=../fb-s9e/b2:fixbolt-session/validate@nofeat b3=../fb-s9e/b3:fixbolt-session/validate@nofeat w1=../fb-boot-d/w1:fixbolt-session/validate@nofeat m=../fb-s9e/m:fixbolt-session/validate@nofeat,fixbolt-engine/density@nofeat ms=../fb-s9e/ms:fixbolt-session/validate,fixbolt-engine/density" scripts/ab-rotation.sh` (+ `f<k>=../fb-s9e/f<k>:fixbolt-engine/density@nofeat` nếu có); trước đó một lần `alloc` của `ms` (hai binary, đọc 0); `CONTROL=ms scripts/ab-rotation.sh --summary target/boot-e-evidence/s1/runs.txt` ở vòng 10 và 20 | driver tự loại arm busy > 3 %; `FAILED` bất kỳ → vòng incomplete | n = 20 mọi arm; sha256 không đổi (driver tự kiểm) | summary vòng 10 và 20; đầu/cuối `timeline.txt`; footer `over baseline` | measured-costs *Boot E*: bảng bisect 95 (`wa→b1→b2→b3→w1`, bốn case validate), bảng `w1→m` (fix (3) trên `main`, ba case như boot D), bảng `ms` (ứng viên baseline, median/min/max/n); STATUS 95, 93 | 5–6 giờ (9 suite/vòng; boot D 15 suite/vòng ≈ 30 phút/vòng) |
| **S2** | 96 | manager chạy (sudo); opus phân tích sau boot | `B=$(awk -F'\t' '$1=="ms" && $4=="validate"{print $7}' ../fb-s9e/MANIFEST.txt); sha256sum "$B"` (khớp manifest); `for k in 1 2 3 4 5; do sudo -n perf record -e cycles -F 4999 -o target/boot-e-evidence/d96-$k.data -- "$B" \| tee target/boot-e-evidence/d96-$k.out; sleep 8; done`; `sudo -n chown tmt target/boot-e-evidence/d96-*`; sau boot: `perf report -i d96-<k>.data --children -s sym --percent-limit 0 \| grep bad_nested_count` và `--no-children` | busy > 3 %; `lost` > 1 % | 5 file, C2 (share ≤ share của case TCR) đọc được | `ls -la d96-*`, hai dòng `perf report` mỗi k, median `validate TradeCaptureReport (33 groups)` | measured-costs *Boot E — the descent, priced inside one binary*; ADR-0086 ghi chú có ngày (chỉ dated note, không sửa substance); STATUS 96 | 30 phút |
| **S3** | 97 | manager | `scripts/bench.sh --strict` trong cây `main` (đã build ở P6; nếu in `Compiling` → dừng, ghi, không dùng số) | — | kết thúc đỏ với các dòng `OVER BASELINE` | dòng verdict + mọi `OVER BASELINE` + dòng `FAIL:` cuối | measured-costs *Boot E — `--strict` observed red*; STATUS 97 | 15 phút |
| **S4** | 93 kết, ADR-0090 q.4 | manager (viết tsv), haiku chạy lại strict | Điều kiện: 5 verdict + S1 n = 20. Từ summary của `ms`: cập nhật **chỉ** các dòng `engine turn *` và `validate *` của CPU này trong `benches/baselines.tsv` (median, `n=20`, margin = `max/median` của `ms` với sàn 1,10 như dòng cũ, ngày); `scripts/bench.sh --strict` → xanh; `git diff benches/baselines.tsv` chỉ những dòng đó | — | `--strict` xanh, `pass 17 fail 0 unknown 0` trong header | diff tsv; verdict `--strict` | `benches/baselines.tsv`; `DESIGN.md` §8 hàng engine-turn; ADR-0095 *Consequences* ghi số dời; STATUS 93, 95, 97 **CLOSED**; `docs/CONFORMANCE.md` §4/§5 nếu số trích ở đó | 30 phút |
| **S5** | — | manager | `git status --porcelain` các worktree; `date`; `DONE` vào `timeline.txt`; commit + push trên nhánh plan (CI xanh trước khi merge); handoff *Start here*. Về dòng desktop là **của chủ dự án**: `sudo -n cp /etc/default/grub.fixbolt-desktop-20260922 /etc/default/grub && sudo -n update-grub`, reboot, `sudo -n fixbolt-machine off` | — | — | timeline nguyên văn | STATUS *Start here* | 20 phút |

Tổng: **~7–8 giờ** kể cả S0. Timers đã stop sống lại sau reboot về desktop — không cần start.

### Danh sách cắt theo thời gian — cắt từ dưới lên

| Cắt thứ | Bỏ | Mất gì | Vì sao bỏ trước |
|---|---|---|---|
| 1 | **S2** (item 96) | giá descent | ADR-0086 đã Accepted với ghi chú; không chặn baseline |
| 2 | **S1 vòng 11–20** | band chính thức (n = 20) → **S4 không chạy** (ADR-0031: baseline n ≥ 20) | tại n = 10 bisect 95 đã đọc được *provisional, n = 10*; baseline **không dời** — nói thẳng, item 93/97 vẫn mở |
| 3 | **S3** | "observed red" của 97 | 97 đã có 20/20 panic từ boot D; S3 là hình thức |
| 4 | **S1 vòng 1–10** | mọi phép so | chỉ ghi *readings*, không verdict |
| — | **S0** không bao giờ cắt | — | không S0 thì không số nào |

Nếu S4 không chạy vì cắt 2: handoff ghi rõ "baseline chưa dời, cần một boot nữa với n = 20 của
`ms`", và worktree `../fb-s9e/` **giữ nguyên** cho boot đó.

### Đo nhưng KHÔNG công bố

| Số | Vì sao |
|---|---|
| Mọi số pha 1 | dòng desktop, không §9 |
| `wa`, `b1`–`b3`, `w1`, `m`, `f<k>` | binary không phải cái `bench.sh --strict` đo (feature khác / không phải `main` có feature); chỉ hiệu số so `ms`/nhau, dán nhãn A/B |
| `ms` tại n < 20 | provisional |
| S2 `validate` dưới `perf record` | có tracer, chỉ lấy tỷ lệ; median lấy từ `ms` ở S1 |

## Cách kiểm chứng

- **P1**: số 2080 là tổng hai `wc -l` đã đếm hôm nay (1248 + 832); tám cặp so với bảng đã công
  bố — nếu **khác** → dừng, về architect (hoặc extractor sai hoặc bảng sai; cả hai phải ra ánh
  sáng — rủi ro đã ghi ở plan trước).
- **P2**: một cặp cùng-binary phải cho mọi |Δshare| nhỏ (kỳ vọng < 0,5 điểm phần trăm cho ký hiệu
  ≥ 5 %); nếu cặp cùng-binary ồn ngang cặp chéo → đoạn đó *accept, unnamed*, không ép tên.
  Tổng Δns của mọi ký hiệu phải cùng dấu và cùng bậc với `turn_b − turn_a` (kiểm tra tổng).
- **P3**: đảo ngược đã có trong `parse_utc_equivalence.rs` của arm (giữ hai nhánh cùng đáp
  án); senior review chạy `git diff main..fix/parse-utc-fast-path -- crates/` và đọc `59 / 59`
  **tự mình**, không tin báo cáo.
- **P4**: hình `PASS` phải đọc `PASS   no timer due` — nếu hàng **biến mất** thay vì PASS, đó là
  bẫy đã ghi (`a-shell-error-inside-an-if…`), dừng.
- **P5**: ADR-0091 đã viết sẵn kỳ vọng hai chiều; `0 red in 50` ở cây cũ là kết quả hợp lệ và
  **là** tin xấu — ghi đúng thế.
- **P6**: `cargo bench --no-run` lần hai không in `Compiling`; sha256 chép vào manifest bằng
  `sha256sum`, không gõ tay.
- **S1**: driver tự kiểm sha256 và busy; manager đọc `timeline.txt` đầu/cuối và footer.
- **S4**: `bench.sh --strict` xanh **sau** khi sửa tsv, trên cùng boot, không rebuild.

## Tài liệu phải cập nhật

- [ ] `STATUS.md`: hàng 93 (bốn verdict pha 1; kết ở S4), 95 (bisect), 96 (giá), 97 (7a đọc;
      `--strict` observed; đóng ở S4); *Not proven*: Mac ×2, timer hai hình — gạch cùng commit;
      *Start here* hai lần (P7 handoff, S5 handoff)
- [ ] `docs/reference/measured-costs.md`: *Item 93 — five segments named* (P2); *Boot E* (S0–S4)
- [ ] `docs/CONFORMANCE.md` §9: hai đếm macOS (P5); §4/§5 nếu số đổi (S4)
- [ ] `docs/reference/a-quiet-machine-check-cannot-see-a-timer-that-has-not-fired.md`: `[seen live]` (P4)
- [ ] `benches/baselines.tsv` + `DESIGN.md` §8 hàng engine-turn (S4, cùng commit)
- [ ] `CHANGELOG.md` (P3)
- [ ] ADR-0095 `Proposed` → `Accepted` khi merge; *Consequences* ghi số dời thật (S4)
- [ ] ADR-0086: ghi chú có ngày sau S2 (không sửa substance)
- [ ] Bẫy mới gặp → `docs/reference/`, cùng commit với bước gặp

## Bẫy đã lường trước

| Bẫy | Câu FAIL kỳ vọng / test canh |
|---|---|
| `perf diff --percent-limit` | `Error: unknown option 'percent-limit'` — đã đo; brief P2 cấm dùng |
| Ký hiệu kernel `[k] 0xffff…` không tên | bỏ qua; density không vào kernel trên hot path (`ManualClock`, `Yield`) |
| Δshare đọc như Δns | kiểm tra tổng: Σ Δns ≈ turn_b − turn_a; lệch dấu → bảng sai |
| `sudo` không có `cargo` (`secure_path`) | S2 gọi binary tuyệt đối; `scripts/check-sudo-names-what-root-can-find.sh` xanh trên mọi script; lệnh S2 không có `cargo` sau `sudo` |
| `perf record` exit 0 khi không tìm thấy workload | `.out` phải có dòng `ns/op` — [perf-record-exits-zero…](../reference/perf-record-exits-zero-when-sudo-cannot-find-the-workload.md) |
| Cây `main` build lại trong boot (`bench.sh --strict` gọi cargo) | P6 kiểm `cargo bench --no-run` lần hai không `Compiling`; S3 dừng nếu thấy `Compiling` |
| Kỳ vọng `pass 16` cũ | plan này nói **17**; nếu check in tổng ≠ 17 → hàng biến mất, dừng |
| Timer stop bị mất sau reboot về desktop | đúng ý; ở pha 1 (P4) phải `start` lại vì máy vẫn là desktop |
| Mac bash 3.2 (`mapfile`) | script contention đã chạy trên Mac (ADR-0091 probe); `check-machine-verdicts.sh` chưa — nếu chết vì bash 3.2 → bug thật, quote dòng lỗi, về developer |
| `d4m/runs.reextracted.txt` ghi đè `runs.txt` | lệnh P1 chỉ ghi file mới; `ls -la d4m/runs.txt` mtime không đổi |
| Ghép `d4` vòng 13 (incomplete) trùng `d4b` vòng 1+12 | lọc `$2<=12` ở `d4`; `wc -l` = 2080 canh |
| Fix arm được "tìm ra" nhưng không dựng kịp trước reboot | ADR-0095 q.3: quá ranh giới reboot → verdict *accept, named*, không phải *fix* |
| Handoff chưa push mà đã reboot | R kiểm `git status` rỗng và CI id có trong *Start here* |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Profile flat không tách được gì trong `turn` inline (ADR-0090 *Bad*) | Cao | *accept, unnamed* là verdict hợp lệ; baseline vẫn dời ở S4 |
| P3 review có finding mở → không merge kịp trước reboot | Trung | pha 2 vẫn chạy; `m`/`ms` dựng từ `main` không có fix; `w2` (`../fb-boot-d/w2`) thêm làm arm để giữ số (3) trong cùng boot |
| P5 đọc `0 red in 50` ở cây cũ | Trung | ghi *not reproducible on either platform*, ADR-0091 bị bác — về architect, không sửa gì trong boot |
| Bisect 95 không tách được (ba bước đều ~4 %) | Trung | công bố ba hiệu số với dispersion, item 95 thành *accept, named per PR*; không mở thêm boot |
| Boot dài hơn ước lượng | Trung | danh sách cắt |
| Timer khác ngoài 13 cái hôm nay lên lịch trong cửa sổ | Thấp | hàng `no timer due` đọc lại tại S0 với cửa sổ 12 h; chiến dịch ~8 h |

## Ngoài phạm vi

- Item 13, 21, 22, 24, 51, 76, 85 và mọi item mở khác ngoài 93/95/96/97: không nằm trong lệnh
  "đóng phần không cần §9" ở mức có thể brief được hôm nay; G6/G7 mở **theo quyết định**.
- `standard`, TLS, số trên dây: không đo.
- Thay `bench.sh`/harness để có band tích luỹ: ADR-0095 quyết định **không** làm.
- D1 của boot D (item 51, flush arm): vẫn cần chủ dự án rời Tailscale — không đưa vào.

## Nhật ký giao hàng

*Điền khi đóng từng bước: commit, gate xanh (trích), CI run id, cái gì chưa làm và vì sao.*

| Bước | Commit | Gate và output (trích) | Chưa làm |
|---|---|---|---|
| P0 | `9308317` | `check-links.py` → `no dead internal links`; `check-adr-numbers.sh` → `ok - 93 files seen, 93 ADRs, 93 distinct numbers, 93 H1s checked`. PR [#93](https://github.com/tmthang86/fixbolt/pull/93) draft | — |
| P1 | *(không file repo)* | `wc -l` → `2080 target/boot-d-evidence/d4m/runs.reextracted.txt`; `CONTROL=w1s --summary` → tám cặp của item 97 đọc bằng máy: `w1s validate NewOrderSingle 1023.5 … 20/20`, `w1s validate Heartbeat 195.6 … 20/20`, `w1s validate TestRequest, w2w bytes 248.1 … 20/20`, `w1 validate NewOrderSingle 1008.9 … 20/20`, `w1 engine turn, 1 busy, ring 64 1822.2 … 20/20`, `w1 engine turn, 1 busy, ring 4096 1860.3 … 20/20`, `w1 validate Heartbeat 187.8 … 15/20`, `w1 validate TestRequest, w2w bytes 242.4 … 15/20`; `w0` không hàng nào ≠ `0/20`; footer `over baseline: 30 (arm, case) pairs` | Bản `d4m/runs.txt` gốc không mở để ghi, đúng brief. **Plan cũ viết 7a là `--reextract d4m`** — `d4m/` không có `raw/`; sửa thành ghép `d4` + `d4b` |
| P2 | `f49b8c9` | Bốn verdict, đều *accept*: (1) unnamed, IPC 2.856→2.983; (2) unnamed, turn +0,7 ns; (4) named `scan_fields::<64>` +13,1 ns, 4,4× sàn nhiễu, `Engine::turn` −7,1 ns cùng commit — hình của inlining dịch; (5) named `__memcmp_avx2_movbe` +13,4 ns, 9× sàn, IPC giảm duy nhất. `check-links.py` xanh | **Bản ghi D2 không có call-graph** (`sample_type = IP\|TID\|TIME\|PERIOD`) nên caller của memcmp không giải được từ đĩa — mọi `perf record` pha 2 muốn đặt tên caller phải có `-g`. Không có arm fix mới cho P6 |
| P3 | PR [#94](https://github.com/tmthang86/fixbolt/pull/94), `cfe7ece` + `70695d3`, merge `badc144` | Trong worktree `../fb-fix-parse`: fmt, clippy `-D warnings`, `cargo test -p fixbolt-session` (mọi suite `ok`, 0 `FAILED`), `--no-default-features` ok, alloc `… clock 0 …` mọi bộ đếm 0, `check-indexing-debt.sh` `176 … ceiling 176 / ok`, `timestamp_widths.rs`/`timestamp_precision.rs` không đổi so với `main`. 59/59 đọc bằng đảo ngược (đòi 60 → `59 / 59 left: 59 right: 60`). **CI run [`35739307994`](https://github.com/tmthang86/fixbolt/actions/runs/35739307994) 14/14 xanh trên `70695d3`.** Review senior: một finding xác nhận — ràng buộc "21 nằm trong luật ADR-0058" chỉ ở prose → hai `const` assert, đảo ngược `LEN_MILLIS = 18` và `= 22` đều `error[E0080]`; bốn phá hoại `clock.rs` (nhân sai, bỏ guard digit, nhận byte lạ ở `.`, xoá arm) — ba đỏ, xoá arm vẫn xanh: file test là lưới tương đương, không phải bản chép | Chưa đo lại trên `main` — S1. **Plan gọi sai gate**: `fixbolt-conformance` không có feature `fix50sp2` (→ *Sửa 1*, `47d9a98`). **`vendor/` cây chính thiếu SP2** — trang [a-vendor-tree-fetched-before-sp2…](../reference/a-vendor-tree-fetched-before-sp2-fails-the-sp2-gates-silently.md), đã fetch lại |
| P4 | `f49b8c9` | `FAIL   no timer due   sysstat-collect.timer next 2026-09-22T14:10Z (in 7m59s), … [window 12h]` → chạy nguyên văn dòng `fix:` (12 `stop`) → `PASS   no timer due   no timer due inside the window [window 12h]` → `start` lại 12 timer, `list-timers --all` đếm 20 trước và sau | — |
| P5 | `fb0230b`, `a2011ca` | Mac mini `d32f8c5`, Darwin 25.6.0 arm64, `sched_model 0`: `bd6be07` → `wire: 27 red in 50`, `wire_fixt: 12 red in 50`, có `FieldCount { expected: 14, actual: 8 }`; `main` → `0 red in 50` cả hai, `lifeline hit: 0`. `docs/CONFORMANCE.md` §9 ghi. **`check-machine-verdicts.sh` trên macOS đọc `pass 62 fail 1`** — `nic_irqs` dùng `find -printf` (GNU); sửa bằng vòng glob + `basename`, đảo ngược (stub `find` từ chối `-printf`) tái hiện đúng dòng FAIL của Mac, sau sửa `pass 63 fail 0` trên cả desk lẫn Mac | Mac để lại worktree `~/Projects/fb-pre3233032`; Mac ở `d32f8c5`, thiếu một commit (#94) — không ảnh hưởng hai phép đếm |
| P6 | *(ngoài repo)* | `../fb-s9e/{b1,b2,b3,m,ms}` dựng, `check-bench-alignment.sh` → `OK: 20 bench binaries, alignment pinned and read back` cả năm cây; `MANIFEST.txt` 29 hàng (9 worktree + 20 cây chính), `sha256sum -c` 29/29 OK; `nm -C … validate-ea1ac0706c5fc955 \| grep -c bad_nested_count` → `3` (C1); cây chính chạy vòng `--no-run` lần hai → `Compiling` = 0; `df -h /` 147G trống | **Ba lỗi của runner, manager sửa tay**: `binary_path` tương đối (mất tên worktree) → tuyệt đối; hàng cây chính đặt tên `m` trùng worktree → `main-tree`; `-p fixbolt-cost` không tồn tại nên `cost` chưa build → build với `-p fixbolt`. **Nhánh plan tách trước #94** nên `git diff main -- crates/` không rỗng → merge `main` vào (`9374820`) rồi build lại cây chính |
| P7 | `877629b` | `STATUS.md` *Start here 2026-09-22, night*; `check-links.py` → `no dead internal links`; **CI run [`35742268276`](https://github.com/tmthang86/fixbolt/actions/runs/35742268276) 14/14 xanh trên `877629b`** | — |
| R | *(commit này)* | `sudo -n grep CMDLINE /etc/default/grub` → `isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1`; `update-grub` → `done`; `git status --porcelain` rỗng ở cây chính và 0 dòng bẩn trong `../fb-s9e/*` và `../fb-boot-d/*`; `ps` đầu bảng `whoopsie 5.1%`, không llama-server | Reboot ngay sau commit này; S0 là việc của phiên kế |
| S0 | *(phiên mới, 2026-09-22 22:00 +07)* | `/proc/cmdline` có `isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1`; `fixbolt-machine on` → `§9 tuning ON`; rx-usecs 0, IRQ → core 4, EEE `disabled`; hàng `no timer due` FAIL → chạy nguyên văn dòng `fix:` (13 timer) → `check-machine.sh` → **`pass 17 fail 0 unknown 0`**; hai `alloc` của `ms` mọi bộ đếm 0; run đầu (`density` của `ms`) bỏ | Timer thứ 14 (`man-db`) trôi vào cửa sổ lúc 03:05 +07 và driver **từ chối** S1b đúng như ADR-0093 — dừng thêm, chạy lại |
| S1 | *(ngoài repo)* | `ROUNDS=20` 7 arm, 15:09–20:05 UTC, 4 arm-vòng bị loại (`busy` 4 %, 11 %, 4 %, 5 % — **tool call của chính manager** là tải), n = 17; **S1b** 4 vòng (20:06–21:07 UTC, 0 loại) ghép offset +20 → `target/boot-e-evidence/s1m/`, `wc -l runs.txt` 1219, **n = 21 mọi (arm, case)**; summary vòng 10 (n = 8) và 20 (n = 17) giữ ở `s1-summary-round{10,20}.txt`. Item 95: `validate Heartbeat` `wa` 166.8 → `b1` 190.8 → 188.1 → 188.0 → 189.0 → `m` 188.7 — cả bước nằm trong PR B. Item 93 (3) trên `main`: `engine turn, 1 busy sessions` `m` 1747.8 (boot D `w1` 1799.1) | Không đặt tên commit trong PR B (một merge). Manager phải **im lặng** khi rotation chạy — mỗi tool call làm node hook bận 30 % trong 1 s và driver loại arm |
| S3 | *(ngoài repo)* | `scripts/bench.sh --strict` 21:07–21:17 UTC, `Compiling` 0, header `pass 17 fail 0 unknown 0`: **đỏ như quan sát** — `validate NewOrderSingle 1024.2 vs 882.1`, `Heartbeat 193.5 vs 169.5`, `TestRequest, w2w bytes 247.3 vs 218.4` `OVER BASELINE`; **không** engine-turn case nào over (fix #94 đã kéo về); `FAIL: --strict, and 8 case(s) had no baseline for this CPU` | **Plan không lường 8 case không baseline**: 2 `wakeup` p50 (bench không có cơ chế tsv — item 100), 4 `sbe`, 2 `validate` FIXT. → *Sửa 2*: S1c 24 vòng `fixbolt-sbe/sbe` từ cây chính (21:29–21:33 UTC, n = 24) để ghi 4 dòng; 2 dòng FIXT từ `ms` n = 21; `wakeup` để lại cho architect |
| S2 | *(ngoài repo; `d96-analysis.txt`)* | 5 record `-g` (thêm `-g` so với plan, vì D2 đã mất caller), sha khớp manifest, `Total Lost Samples: 0`, ~615 K mẫu/lần; `bad_nested_count` children 2.94 % / self 2.93 % (median 5 k) → **2 455,6 ns = 3,0 % của TCR**, cận dưới; C1 3 ký hiệu, C3 giữ (`bad_group_count` inline), C2 tầm thường (mọi case cùng 1 410 000 lời gọi) | **Inclusive share không đo được**: build release không frame pointer → callchain rác, `children ≡ self`. Item 96 **vẫn mở**, về architect: record `--call-graph dwarf` với `debug = 1` trong bench profile. Hai bẫy → trang reference mới |
| S4 | *(commit này)* | `benches/baselines.tsv`: 15 dòng dời từ `ms` n = 21 (ladder: 1.10, TCR 1.15), 2 dòng FIXT thêm, 4 dòng `sbe` thêm (n = 24); `bench.sh --strict` lần hai (`s4-strict.txt`, 21:34–21:44 UTC, `Compiling` 0, header `pass 17 fail 0 unknown 0`): **mọi dòng đã ghi trong band**; đỏ còn lại `2 case(s) had no baseline` (`wakeup`, item 100) và **một bất ngờ**: `journal put, 191 bytes, one slot` 7.4 (S3) → 12.4 ns, lặp lại 4 lần kể cả `taskset -c 6`, hai case `walking` không đổi → item 99, dòng **không** dời | `--strict` không xanh — theo *Sửa 2*, đọc là "đỏ chỉ ở `wakeup`" **cộng** item 99. Ledger ADR-0095 ghi trong *Consequences*; `DESIGN.md` §8 một hàng |
| S5 | *(commit này)* | `git status --porcelain` các worktree rỗng; handoff *Start here 2026-09-23*; review senior PR; CI; merge; **shutdown** theo lệnh chủ dự án 2026-09-22 ("khi nào xong rồi thì shutdown") — dòng grub §9 **giữ nguyên** (về desktop là của chủ dự án), timer đã stop sẽ tự về khi boot | — |
