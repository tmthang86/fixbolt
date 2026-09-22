# Cái máy dò và điều kiện trước chiến dịch: ba lỗi gate và hai quyết định không cần bàn đo

> **Loại:** Plan · **Ngày:** 2026-09-22 · **Trạng thái:** Đã duyệt (chủ dự án, 2026-09-22)
> **Phạm vi:** phần dư của boot D mà máy §9 không liên quan — item **97** (nửa *parser*, không
> phải nửa *chậm*), item **98** (a) gate cho bẫy `sudo`, (b) hàng timer trong `check-machine.sh`,
> và chỗ để driver chiến dịch; item **96** chỉ *thiết kế* thí nghiệm, không chạy.

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Boot D đóng (PR #90, `main` tại `60ea1a2`), bàn đo đã tắt và quay về dòng grub desktop. Mọi việc
còn mở mà **cần** bàn đo chờ chiến dịch sau. Kế hoạch này gom **phần còn lại**: ba chỗ gate nói
sai hoặc không nói gì, và hai quyết định thuộc về kiến trúc sư.

1. **Item 97, nửa parser.** Trong boot D, bench binary của `main` *panic ở mọi vòng* (20/20,
   21/21, 21/21) vì tám median vượt đường baseline đã ghi — đó là **máy dò làm đúng việc**. Nhưng
   `scripts/ab-rotation.sh` (a) không đọc exit status, nên ghi "round N complete" sáu mươi hai
   lần; (b) lấy dòng báo cáo của harness (`… ns/op exceeds …`) làm hàng đo, nên `--summary` có
   thêm hàng ma; (c) không có cột nào nói một arm đang vượt baseline của chính nó. Bản viết đầu
   của boot D vì thế gọi báo cáo thật của harness là "lỗi parser". Ba điều này sửa được ở bất kỳ
   máy Linux nào. **Cái chậm thì không sửa ở đây** — xem *Ngoài phạm vi*.
2. **Item 98 (a).** `sudo -n perf record … -- cargo bench` chạy trong một giây, ghi file
   `.data` chỉ có header, exit 0; năm lần "thành công" trước khi ai đó đọc log. `sudo` không dùng
   `PATH` của người gọi mà dùng `secure_path`, và `~/.cargo/bin` không có trong đó. Trang
   `docs/reference/` đã ghi; `CLAUDE.md` §4 nói mỗi bẫy phải có test canh — chưa có.
3. **Item 98 (b).** `apt-daily-upgrade.timer` nổ lúc 06:51, mất vòng 13–20. Hàng *machine is
   quiet* đo một giây, *bây giờ*, không nói gì về bốn tiếng nữa. Cần một hàng đọc
   `systemctl list-timers`; cần quyết định cửa sổ bao nhiêu và pending timer là `fail` hay
   `unknown`.
4. **Item 98, nửa script.** `run-d2.sh`, `run-d3.sh`, `run-d5.sh` nằm trong `target/`
   (gitignored) và **đã mất** cùng bàn đo. Không khôi phục được — nên quyết định *luật*, không
   phải nội dung ba file.
5. **Item 96.** Thí nghiệm đo giá "descent" của ADR-0086 đọc −10,83 % khi bỏ descent, nhưng ba
   trong bốn case đối chứng (không có group, không bao giờ vào descent) lệch 2,9–5,8 % trong khi
   thiết kế đòi ≤ 2 %. Độ lớn biết rồi, quy trách nhiệm thì chưa; lý do là layout của một binary
   thứ hai. Cần một thiết kế **không cần binary thứ hai** — cho boot §9 sau, không chạy ở đây.

Ba ADR viết ở bước 0 giữ các quyết định: [ADR-0092](../decisions/ADR-0092-the-rotation-driver-reads-a-row-by-its-shape-and-a-panicking-finish-is-a-verdict-not-a-lost-round.md)
(item 97), [ADR-0093](../decisions/ADR-0093-a-campaign-driver-is-committed-sudo-in-a-committed-script-names-what-root-can-find-and-a-timer-due-inside-the-window-is-a-fail-row.md)
(item 98 cả ba phần), [ADR-0094](../decisions/ADR-0094-the-descent-is-priced-by-attribution-inside-one-binary-not-by-a-second-binary.md)
(item 96).

## Những gì đã biết chắc

Số dòng đọc tại `main` `0629111`.

**Harness và driver (item 97)**

- **Dòng báo cáo của harness có hình hàng đo.** `crates/codec/benches/harness.rs:329-333`
  in mỗi case một hàng `{name:<34} {best:>8.1} ns/op   baseline {b:.1} x{m:.2} = [{floor:.1},
  {ceiling:.1}]{mark}` (mark là rỗng, `  OVER BASELINE` hoặc `  UNDER BASELINE`), hoặc
  `:361` `{name:<34} {best:>8.1} ns/op   NO BASELINE for '{cpu}'`. Nhưng `:344-348` cũng đẩy
  vào `over` chuỗi `"{name}: {best:.1} ns/op exceeds {ceiling:.1} ns (baseline …)"`, `:353-358`
  đẩy vào `under` chuỗi `"{name}: {best:.1} ns/op is below …"`. `finish()` (`:398-402`) in
  `cases under their baseline: N  a | b` một dòng, rồi `:406-411` `assert!` với danh sách `over`
  nối bằng xuống dòng. **Mọi chuỗi đó đều chứa ` ns/op`.** `[measured 2026-09-22, manager, cloud
  container]`
- **Extractor của driver không phân biệt được.** `scripts/ab-rotation.sh:460-469`:
  `awk -F' ns/op' '/ ns\/op/ {…}'`, lấy `$1`, tách theo dấu cách, token cuối là số, phần trước
  là tên case. Cho ăn đúng hình output thật → **5 hàng từ 2 phép đo**: `[measured 2026-09-22,
  manager, cloud container]`

  ```text
  w1s	7	validate NewOrderSingle	1023.5
  w1s	7	validate Heartbeat	195.6
  w1s	7	cases under their baseline: 1 encode Heartbeat (template):	12.0
  w1s	7	validate NewOrderSingle:	1023.5
  w1s	7	validate Heartbeat:	195.6
  ```

  Tên hàng ma khác tên thật (thêm `:`), nên median của case thật **không bị** trộn — mọi median
  boot D đều tái hiện.
- **Driver không đọc exit status.** `:105` `set -uo pipefail` (không `-e`); `:465`
  `out=$("$bin" 2>&1)`; không có `$?` ở đâu cả. Panic của Rust là exit 101.
  `[measured 2026-09-22, manager, cloud container]`
- **`ab_summary()` (`:134-195`) in `arm / case / median / min-med / max-med / n / diff%`**, không
  có cột nào về baseline. `[measured 2026-09-22, manager]`
- **Harness đo hết rồi mới fail**: `harness.rs:56-63`, `[measured 2026-08-30]` — mọi case in
  xong trước khi `finish` assert. Nên một run exit 101 do vượt band có **đủ** số liệu, và số liệu
  tốt y như run exit 0.
- **`scripts/bench.sh` không mắc lỗi này** và không sửa: `:151-200` dùng `ns/op` chỉ để biết
  "có đo", đọc hai dòng đếm theo prefix riêng, đọc `code=$?`.
- **`scripts/check-ab-rotation.sh` tồn tại, test `ab_summary`/`ab_median`/`ab_complete_rounds`
  bằng fixture — và KHÔNG chạy trong CI.** `grep check-ab-rotation .github/workflows/ci.yml` →
  không có; ba script cùng họ (`check-machine-verdicts.sh`, `check-w2w-baseline-summary.sh`,
  `check-w2w-compare.sh`) nằm trong job `script-logic` (`ci.yml:107-118`).
  `[measured 2026-09-22, architect, cloud container]`
- **Arm của một rotation là binary xây trước boot** từ worktree cũ (ADR-0090 quyết định 2;
  `MANIFEST.txt`). Sửa `harness.rs` chỉ bảo vệ binary xây *sau* khi sửa. Đây là mấu chốt quyết
  định 1 của ADR-0092: sửa **parser**, không sửa harness.
- **`benches/baselines.tsv` đọc lúc chạy, không compile vào binary** (ADR-0067). Thêm một dòng
  tạm vào file đó là đủ ép một case thành `OVER`/`UNDER` mà không xây lại — cách chụp fixture
  thật ở bước 1. Dòng dữ liệu sai làm harness exit ≠ 0 ở **mọi** mode (`harness.rs:read_baselines`).
- **Định dạng dòng tsv**: harness in sẵn dòng để dán, 7 cột tab:
  `{cpu}\t{name}\t{best:.1}\t<margin>\t<n>\t<date>\t<verdict>` (`harness.rs:368-372`).
- **`--summary <runs.txt>`** đọc `$(dirname runs.txt)/timeline.txt`, cần `CONTROL`
  (`ab-rotation.sh:200-211`). Raw stdout mỗi run được giữ nguyên ở `raw/<round>-<arm>-<suite>.txt`
  (header, mục *EVIDENCE*) — nên bằng chứng boot D (`target/boot-d-evidence/d4m/`, trên bàn đo)
  đọc lại được mà không cần boot.

**`sudo` (item 98a) và chỗ để driver (98, nửa script)**

- **`secure_path` áp dụng dù `env_reset` bật hay tắt — `sudo -E` không cứu được `cargo`.**
  `plugins/sudoers/env.c`, `rebuild_env()`, *sau* khối `if (def_env_reset || …)`:
  `if (def_secure_path && !user_is_exempt(ctx)) { CHECK_SETENV2("PATH", def_secure_path, …) }`
  (fetch 2026-09-22 từ `github.com/sudo-project/sudo`). `sudoers(5)` mục `env_reset`: *"If the
  secure_path setting is enabled, its value will be used for the PATH environment variable"*.
- **Giá trị `secure_path` chỉ đọc được lúc chạy khi là root**: `sudo -V` (root) in
  `Value to override user's $PATH with: /usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/snap/bin`;
  `/etc/sudoers` là `0440 root`. `cargo` ở đây là `/root/.cargo/bin/cargo` — không nằm trong
  danh sách. `[measured 2026-09-22, architect, cloud container, sudo 1.9.15p5]` Gate chạy trên
  CI runner (không root, không `perf`) nên **phải mang danh sách cố định**, không hỏi máy.
- **Không có một lời gọi `sudo` thật nào trong `scripts/` hôm nay.** Regex vị-trí-lệnh của
  `check-scratch-fixtures.sh:185` (thay `cp` bằng `sudo`) + lọc comment `is_live()` trên mọi
  `scripts/*.sh` cho **6** dòng, tất cả là chuỗi lời khuyên đưa vào `row()` của
  `check-machine.sh` (`:391, :394, :414, :425, :620, :669`): `… | sudo tee …`,
  `sudo systemctl stop irqbalance && sudo systemctl disable irqbalance`. Các chỗ `sudo` còn lại
  (`check-machine.sh:381,440,450,563,566,651,652,696`, `check-ktls-available.sh:60`,
  `check-no-kernel-sleep.sh:73`, `check-standard-gives-the-core-back.sh:80`) đứng sau dấu nháy
  hoặc sau một từ thường. `[measured 2026-09-22, manager + architect, cloud container]` **Hệ
  quả**: gate neo vào *vị trí* `sudo` thì đỏ trên `main` từ lúc sinh với 0 phát hiện thật; gate
  neo vào **lệnh mà `sudo` chạy** thì xanh ngay (`tee`, `systemctl`, `sysctl`, `cpupower`,
  `modprobe`, `ethtool` đều nằm trên `secure_path`). Và gate giới hạn ở `scripts/` chỉ có giá trị
  nếu driver nằm trong `scripts/` — B phụ thuộc C, ADR-0093 quyết định 1 chốt C trước.
- **Bẫy nằm ngay trong ô kế hoạch**: `docs/plans/2026-09-20-boot-d.md:309` viết
  `sudo -n perf record … -- cargo bench -q -p fixbolt-engine --bench density`; driver chép từ đó.
  Trang tham chiếu ghi cách sửa tay: chạy binary đã pin trong manifest bằng đường dẫn tuyệt đối.
- **Từ lệnh thật của bẫy là `perf`** — nằm trên `secure_path`; cái không tìm thấy là workload
  `cargo` sau `--`. Một gate chỉ đọc từ đầu tiên sau `sudo` **bỏ qua đúng bẫy này**.
- **ShellCheck không có luật nào về `sudo` và `PATH`**; tìm 2026-09-22 không thấy dự án nào gate
  việc này trong CI. Tiền lệ trong repo: `check-scratch-fixtures.sh` — gate hình grep, nêu rõ
  khoảng trống (ADR-0061).
- **ADR-0090 mục *Good* đã nói**: *"the rotation driver is a committed procedure; the next boot
  does not rewrite `c91.sh` by hand"*. `ab-rotation.sh` ở `scripts/`. §2 bất biến 10: không con
  số nào thiếu *benchmark đã commit* sinh ra nó.

**Timer (item 98b)**

- **`systemctl list-timers` có dạng JSON, khoá cố định.** `src/systemctl/systemctl-list-units.c`,
  `output_timers_list`: `table_new("next", "left", "last", "passed", "unit", "activates")`;
  `src/systemctl/systemctl-util.c`, `output_table()`: `if (OUTPUT_MODE_IS_JSON(arg_output))
  r = table_print_json(…)`; `src/shared/format-table.c`: ô timestamp → số nguyên **micro giây
  từ epoch**, hoặc **`null`** khi `USEC_INFINITY` (timer inactive dưới `--all`). (Fetch
  2026-09-22 từ `github.com/systemd/systemd`.)
- **systemd 255 nhận `--output=json` cho `list-timers`**: `systemctl list-timers --output=bogus`
  bị từ chối ngay lúc đọc tham số (`Unknown output 'bogus'.`), `--output=json` đi tiếp tới bus
  (ở container này: `System has not been booted with systemd as init system`).
  `[measured 2026-09-22, architect, cloud container, systemd 255.4-1ubuntu8.14]` Phiên bản
  systemd nào *thêm* tính năng này: tìm trong `NEWS` không ra — bước 4 ghi lại phiên bản trên
  bàn đo khi chạy thật.
- **`check-machine.sh` không dùng `jq`** (`grep -c jq` → 0); `ab-rotation.sh` và `bench.sh` có.
- **Hợp đồng `check-machine.sh`**: `row PASS|FAIL|UNKNOWN name value fixcmd` (`:40-48`); exit 1
  khi `fail > 0` hoặc `unknown > 1` (`:704-710`); header `:13-15`: *`unknown` is NOT a pass*.
  `ab-rotation.sh` chỉ đọc hàng *machine is quiet* (`quiet_status`, `:440-450`), `|| true`.
  `bench.sh` bỏ qua exit code trừ `--strict`. Một desktop thường đã đỏ trên hàng chục hàng.
- **Độ dài chiến dịch**: boot D D4 chạy 23:36 → 11:46 (12 h 10 min); boot C ~6 h
  (trang tham chiếu). Hàm kiểm tra chạy *một thời điểm* và không biết chiến dịch dài bao lâu.
- `next` của `list-timers` là thời điểm systemd **đã xếp lịch**, gồm cả `RandomizedDelaySec`
  (trang tham chiếu: *"prints the next one outright"*); `systemd-analyze calendar` thì không.

**Descent (item 96)**

- `bad_nested_count<'a, D: Tables, const N: usize>` (`crates/session/src/lib.rs:4437`) **đệ
  quy** với `depth + 1` (ADR-0086 quyết định 1), không có `#[inline]`. Hàm đệ quy giữ symbol
  riêng dù lời gọi ngoài cùng có bị inline vào `bad_group_count` (`:4376`).
- **`Cargo.toml` không có mục `[profile.*]`** (`grep -n '^\[profile'` → rỗng) → bench binary
  dùng mặc định cargo: `strip = "none"`, có symbol, không debuginfo. `[measured 2026-09-22]`
- Case đo có sẵn: `crates/session/benches/validate.rs:226` `validate TradeCaptureReport (33
  groups)` sau `fix50sp2`; vòng đo 10 000 warm-up + 7 × 200 000 lần (`harness.rs:305-318`). Bốn
  case không group cùng binary: `:104, :108, :149, :153`.
- Số đo boot D: 82 071,1 → 73 182,8 ns (−10,83 %), đối chứng lệch +5,78 / +4,35 / +2,86 %
  (`STATUS.md` hàng 96; ADR-0086 ghi chú `[measured 2026-09-22]`). Cả hai arm đều có cờ
  ADR-0049.
- `ab/validate-no-descent` "deliberately broken" (`STATUS.md` *Do not*).

**Máy xây kế hoạch này**: container cloud 4 vCPU Intel Xeon, clone mới, `vendor/` chưa fetch
(`scripts/fetch-quickfix-assets.sh`), không phải bàn đo §9, không có PID 1 systemd. Mọi gate
đóng bước phải chạy được ở đây.

## Cách làm

Ba ADR ở bước 0 giữ lý do và phương án bị loại; đây chỉ tóm cái được chọn.

**Item 97 — [ADR-0092](../decisions/ADR-0092-the-rotation-driver-reads-a-row-by-its-shape-and-a-panicking-finish-is-a-verdict-not-a-lost-round.md).**
(1) Extractor thành hàm thuần `ab_extract <arm> <round>` (stdin → TSV 5 cột
`arm round case ns verdict`), **neo vào hình hàng đo**: sau ` ns/op` phải là ba dấu cách rồi
`baseline …` hoặc `NO BASELINE for '`; verdict đọc từ mark (`in|over|under|none`). Dòng
`… ns/op exceeds …`, `… is below …`, dòng `cases under their baseline:`, thân panic — không
khớp. **`harness.rs` không sửa** (arm là binary cũ). Hình hàng được ghim bằng **fixture chụp
thật** từ một bench binary: bốn case, **ba dòng tạm** trong `baselines.tsv` — một ép `OVER`, một
ép `UNDER`, một đặt case thứ ba **in-band** (số đọc từ lần chạy đầu, margin rộng) — case thứ tư
để `NO BASELINE`; một lần chạy cho đủ **cả bốn** giá trị verdict `over/under/in/none`; rồi hoàn
lại byte-identical (*Sửa 1* nói vì sao là ba dòng chứ không phải hai).
(2) `run_suite` đọc `code=$?`, đếm hàng, đọc dòng `<k> of <m> case(s) over the machine baseline`
của panic; ghi vào `timeline.txt` một dòng mỗi suite:
`round N arm X suite S exit E rows R over O under U nobase M  ok|OVER|FAILED`. `OVER` (exit ≠ 0
*và* có dòng verdict *và* `m == R`) → **vòng vẫn `complete`**; `FAILED` (mọi trường hợp khác,
kể cả `rows 0` với exit 0) → `round N incomplete` như máy bận. Dòng `busy … ok|DISQUALIFIED`
và `ab_complete_rounds` **không đổi**.
(3) `runs.txt` thêm cột 5; `ab_summary` thêm cột `over` = `k/n` (`?` khi file 4 cột) và dòng
chân `over baseline: <P> (arm, case) pairs` hoặc `over baseline: none`. Mode mới
`--reextract <evidence-dir>` dựng `<dir>/runs.reextracted.txt` từ `raw/*.txt`, **không bao giờ
ghi đè `runs.txt`**.
(4) `scripts/check-ab-rotation.sh` vào job `script-logic`. **Không** đụng
`benches/baselines.tsv` ngoài ba dòng tạm ở bước 1 (hoàn lại trong cùng bước).

**Item 98 — [ADR-0093](../decisions/ADR-0093-a-campaign-driver-is-committed-sudo-in-a-committed-script-names-what-root-can-find-and-a-timer-due-inside-the-window-is-a-fail-row.md).**
*Chỗ để driver (quyết định 1)*: driver chiến dịch nằm trong `scripts/`, trên nhánh của kế hoạch,
**trước** reboot, được nêu tên trong *Chia việc*; `target/` chỉ chứa bằng chứng. Ba file chết
không dựng lại. *Gate `sudo` (quyết định 2)*: `scripts/check-sudo-names-what-root-can-find.sh`
quét mọi file shell trong `scripts/` (và đường dẫn truyền vào), dòng sống, nối dòng có `\` cuối;
với **mọi** chỗ có từ `sudo` (kể cả trong chuỗi lời khuyên): **R1** từ lệnh sau tuỳ chọn của
sudo phải chứa `/`, hoặc bắt đầu bằng `$`, hoặc nằm trong ALLOW cố định (`apt apt-get bash cat
chrt cpupower dmesg env ethtool ip journalctl kill modprobe nft nice perf pkill setcap sh sysctl
systemctl taskset tee update-grub`); **R2** bất kỳ token trần nào sau `sudo` là `cargo`,
`cargo-*`, `rustc`, `rustup`, `rustdoc`, `w2w` → FAIL; **R3** lệnh `perf` có `--` → token sau `--`
qua R1. 0 script quét = FAIL. Hàm thuần được `scripts/check-sudo-verdicts.sh` test bằng
fixture (dòng thật của boot D, sáu dòng lời khuyên, dạng đường dẫn tuyệt đối, dạng `-E`). Cả hai
vào CI — gate đứng **ngay sau step `check-scratch-fixtures.sh`** (cùng job với tiền lệ của nó,
hôm nay tên là `lint-config`), verdict test đứng **cạnh `check-machine-verdicts.sh`** (job
`script-logic`), cả hai script vào danh sách `shellcheck -S info` của step đó (*Sửa 2*). *Hàng
timer (quyết định 3)*:
`check-machine.sh` thêm hàng `no timer due`: `systemctl list-timers --all --output=json` → `jq`
→ hàm thuần `timers_verdict <now_usec> <window_sec> <json>`; **FAIL** khi có `next ≤ now +
window` (kể cả `next` đã qua), nêu tên unit + giờ nổ **theo UTC** + còn bao lâu (`in 12h00m` /
`overdue 5m00s`; UTC chứ không phải giờ địa phương — hàm nhận `now` làm tham số, không gọi `date`,
nên test ghim được bằng một epoch cố định; xem ADR-0093 quyết định 3, sửa lần hai), fix
`sudo -n systemctl stop <unit>` (*stop, không disable*); **UNKNOWN** khi không có `systemctl`,
không tới PID 1, hoặc thiếu `jq`; cửa sổ `FIXBOLT_TIMER_WINDOW` (giờ), **mặc định 12**, in ra ở
mọi verdict. `ab-rotation.sh` preflight **từ chối chạy** khi hàng này FAIL (hình như từ chối
`/tmp`). `DESIGN.md` §9 thêm hàng; `docs/hft-playbook.md` §6 thêm mục.

**Item 96 — [ADR-0094](../decisions/ADR-0094-the-descent-is-priced-by-attribution-inside-one-binary-not-by-a-second-binary.md).**
Chỉ thiết kế: `perf record` trên **một** binary `validate` của `main` (đã pin manifest, đường
dẫn tuyệt đối), ≥ 5 lần; giá descent = phần trăm `--children` của symbol `bad_nested_count` ×
median case, kèm `self%`; ba điều kiện C1–C3 (symbol có thật qua `nm` trước boot; phần trăm
không vượt phần của chính case; nếu lời gọi ngoài bị inline thì là **cận dưới**); nói rõ cái
không thấy (hiệu ứng bậc hai lên caller). `ab/validate-no-descent` nghỉ hưu. Không có bước nào
trong kế hoạch này chạy nó — hàng 7 chờ boot sau.

## Bất biến bị đụng tới

**Không** — kế hoạch này không sửa `codec`, `session`, `engine`, `transport`; mọi file sửa là
`scripts/`, `.github/`, `docs/`, `STATUS.md`. Hai điểm cần nói vì đứng gần:

| Chỗ gần bất biến | Đụng thế nào | Giữ bằng gì |
|---|---|---|
| 10 — không số đo thiếu ba thứ | Kế hoạch không công bố con số nào. Rehearsal ở bước 2 chạy bench trên container: số in ra là **để đọc plumbing của driver**, không ghi vào đâu | Nhật ký ghi rõ "container, không phải §9, không phải số"; `--summary` chỉ chạy trên fixture và `target/ab-rehearsal/` |
| `benches/baselines.tsv` | Bước 1 và 2 **thêm dòng tạm** để ép `OVER`/`UNDER`/lỗi định dạng | Mỗi bước kết thúc bằng `git checkout benches/baselines.tsv && git diff --exit-code benches/baselines.tsv`, quote output; một dòng sót lại làm harness exit ≠ 0 ở mọi mode |

## Chia việc

Không bước nào commit; manager chạy lại gate và commit. Một pull request. `ci.yml` **một người
sửa** (bước 3) — bước 1 không đụng `ci.yml`.

| Bước | Ai | Kết quả | Được sửa | Không được sửa | §2 | Test viết trước, câu FAIL chờ đợi | Gate đóng bước | Phụ thuộc |
|---|---|---|---|---|---|---|---|---|
| **0** | architect (fable) | Kế hoạch này; ADR-0092, 0093, 0094 | `docs/plans/`, `docs/decisions/` | mọi thứ khác | — | — | `python3 scripts/check-links.py`; `scripts/check-adr-numbers.sh` | — |
| **1** | developer (sonnet) | Item 97 (1)+(4a): `ab_extract` thuần, fixture chụp thật, test | `scripts/ab-rotation.sh` (thêm `ab_extract`, `run_suite` gọi nó — **không** đổi gì khác), `scripts/check-ab-rotation.sh` | `crates/`, `ci.yml`, `benches/baselines.tsv` (chỉ ba dòng tạm, hoàn lại trong bước) | — | Mục `=== ab_extract` với fixture thật (cách chụp ở *Cách kiểm chứng*). `same "4" "$(… \| wc -l)"` — FAIL chờ: `FAIL  want [4] got [6]  REVERSAL TARGET (rows): the harness's OVER/UNDER report lines and the panic body are not measurement rows` (con số `got` là con số awk cũ in ra, đọc chứ không đoán). Thêm: không tên case nào kết thúc bằng `:`; cột verdict đúng `over/under/in/none` — **đủ bốn giá trị** (*Sửa 1*) | `scripts/check-ab-rotation.sh` → `pass N   fail 0`; `shellcheck -S info scripts/ab-rotation.sh scripts/check-ab-rotation.sh` (quote; nếu có cảnh báo **có sẵn** ở dòng không sửa → báo, không sửa) | 0 |
| **2** | developer (sonnet) | Item 97 (2)+(3): exit status, dòng suite, `FAILED`, cột `over`, chân, `--reextract` | `scripts/ab-rotation.sh`, `scripts/check-ab-rotation.sh` | `crates/`, `ci.yml`, `check-machine.sh` | — | Fixture timeline có dòng suite `FAILED` và `round 3 incomplete`: `same "1\n2"` — FAIL chờ: `FAIL  want [1 2] got [1 2 3]  REVERSAL TARGET: a suite that exited non-zero without the harness's verdict line drops the round`. runs.txt 5 cột → cột `over` `2/2`; 4 cột → `?` — FAIL chờ: `FAIL  want [2/2] got [?]  REVERSAL TARGET (over): the verdict column reaches the summary`. `--reextract` trên `raw/` giả → 4 hàng | `scripts/check-ab-rotation.sh` xanh; **rehearsal thật** `ROUNDS=1` trên container (ba lần: sạch → `ok`; dòng tạm ép OVER → `exit 101 … OVER`, `round 1 complete`, chân `over baseline: 1 …`; dòng tsv **sai định dạng** → `rows 0 … FAILED`, `round 1 incomplete`); `git diff --exit-code benches/baselines.tsv` | 1 |
| **3** | developer (sonnet) | Item 98a: gate `sudo` + verdict test + CI | `scripts/check-sudo-names-what-root-can-find.sh` (mới), `scripts/check-sudo-verdicts.sh` (mới), `.github/workflows/ci.yml` (gate: một step **ngay sau step `check-scratch-fixtures.sh`**, job nào chứa step đó thì vào job đó — hôm nay là `lint-config`, *Sửa 2*; verdict test: cạnh step `check-machine-verdicts.sh` — job `script-logic` — cùng với `check-ab-rotation.sh`; danh sách `shellcheck -S info` của step cạnh `check-scratch-fixtures.sh`: hai script mới) | `crates/`, `check-machine.sh`, `ab-rotation.sh`, mọi script khác | — | `check-sudo-verdicts.sh` với dòng boot D `sudo -n perf record -e cycles -F 4999 -o d.data -- cargo bench -q -p fixbolt-engine --bench density` — FAIL chờ khi R2 chưa viết: `FAIL  want [FAIL R2 cargo] got [ok]  REVERSAL TARGET: boot D's own line — perf passes R1, the workload after -- is what root cannot find`. Sáu dòng lời khuyên → `ok`. `sudo mytool` → `FAIL R1`. `sudo -E cargo bench` → FAIL. `sudo -n "$BIN"` → `ok` (G1) | `scripts/check-sudo-names-what-root-can-find.sh` trên cây → `ok — <N> scripts scanned, <M> sudo lines read, 0 findings` (đọc N, M); đảo chiều bằng file dưới `target/check-sudo/` (*Cách kiểm chứng*) → exit 1 đúng câu; `scripts/check-sudo-verdicts.sh` → `pass N fail 0`; `scripts/check-scratch-fixtures.sh` vẫn xanh; `shellcheck -S info` hai file mới sạch | 0. **Song song với 1** (file rời) |
| **4** | developer (sonnet) | Item 98b: hàng `no timer due`, `timers_verdict`, preflight của driver, hàng §9 | `scripts/check-machine.sh`, `scripts/check-machine-verdicts.sh`, `scripts/ab-rotation.sh` (**chỉ** preflight), `docs/DESIGN.md` (**chỉ** một hàng §9, chữ ở *Cách kiểm chứng*) | `crates/`, `ci.yml`, phần khác của `DESIGN.md` | — | `check-machine-verdicts.sh` mục `=== timers_verdict`: JSON 4 timer (`next` = now+1 h; now+13 h; `null`; now−5 min), window 12 h → `FAIL` nêu **hai** unit — FAIL chờ: `FAIL  want [FAIL apt-daily-upgrade.timer …] got [PASS]  REVERSAL TARGET: a timer due in 1h inside a 12h window`; cùng JSON, window 0,5 h → `FAIL` nêu **một** (unit đã qua giờ); `[]` → `PASS`; JSON hỏng → `UNKNOWN` | `scripts/check-machine-verdicts.sh` → `pass N fail 0`; `scripts/check-machine.sh` trên container in hàng `? ? ?  no timer due  cannot reach PID 1 (…) [window 12h]` (quote); `AB_ROTATION` dry-run vẫn chạy; `scripts/check-sudo-names-what-root-can-find.sh` xanh (fix line dùng `systemctl`) | 2, 3 |
| **5** | developer (sonnet) | Tài liệu | `STATUS.md` (hàng 96, 97, 98; *Not proven*), hai trang `docs/reference/` (đoạn *Guarded by*), `docs/DESIGN.md` §6 đoạn *How the benchmarks are run* (ba câu về exit status/verdict/`--reextract`), `docs/hft-playbook.md` §6 (mục mới: driver trong `scripts/`, hàng timer, gate `sudo`) | `crates/`, `CLAUDE.md`, ADR đã Accepted | — | — | `python3 scripts/check-links.py` → `no dead internal links`; `scripts/check-adr-numbers.sh` | 1–4 |
| **6** | manager | Gate toàn bộ tại commit đóng, senior review một lần, CI run id, merge, handoff *Start here* | — | — | — | — | tất cả | 5 |
| **7** | **chờ bàn đo** (dòng grub bất kỳ cho 7a; boot §9 cho 7b) | 7a: `scripts/ab-rotation.sh --reextract target/boot-d-evidence/d4m` rồi `--summary` — cột `over` phải nêu đúng tám median của item 97; ghi vào `measured-costs.md` *Boot D … over its recorded baselines*. 7b: thí nghiệm ADR-0094 | — | — | — | 7a: nếu cột `over` không nêu đúng 5 dòng baseline / 8 median đã công bố → dừng, báo architect | — | 6; bàn đo |

Song song: **1 ‖ 3**, rồi 2, rồi 4, rồi 5.

## Cách kiểm chứng

**Bước 1 — chụp fixture thật (bắt buộc, không gõ tay).** Trên container:

1. `scripts/fetch-quickfix-assets.sh`; `RUSTFLAGS="$(scripts/check-bench-alignment.sh --flags)"
   cargo bench -p fixbolt-session --bench validate --no-run` (không feature — bốn case là đủ);
   lấy đường dẫn binary từ output (hoặc `--message-format=json` như `preflight()` làm).
2. Chạy binary một lần: mọi hàng `NO BASELINE for '<cpu>'` — đọc `<cpu>` **và đọc số
   `<X>` ns/op của `validate TestRequest, w2w bytes`** (case sẽ đặt in-band).
3. Thêm **ba** dòng vào cuối `benches/baselines.tsv` (tab) — bốn case, mỗi giá trị verdict một
   case, case thứ tư để trống:
   `<cpu>	validate NewOrderSingle	1.0	1.10	1	2026-09-22	fixture` (ép OVER),
   `<cpu>	validate Heartbeat	999999.0	1.10	1	2026-09-22	fixture` (ép UNDER),
   `<cpu>	validate TestRequest, w2w bytes	<X>	1.35	1	2026-09-22	fixture` (**in-band**: số
   vừa đọc, margin 1,35 là đỉnh thang của file — đủ rộng để lần chạy thứ hai rơi vào
   `[X/1.35, X×1.35]` trên máy này; nếu không rơi vào, đọc lại số và chạy lại, không nới thêm).
   `validate NewOrderSingle, w2w bytes` **không** có dòng → `NO BASELINE`.
4. `"$BIN" > fixture.txt 2>&1; echo "exit=$?"` — chờ `exit=101`; file có: một hàng
   `  OVER BASELINE`, một hàng `  UNDER BASELINE`, **một hàng in-band** (` ns/op   baseline … =
   [floor, ceiling]` và **không có mark** sau `]`), **một** hàng `NO BASELINE for`, dòng
   `cases without a baseline: 1 …`, dòng `cases under their baseline: 1  validate Heartbeat: … ns/op is
   below …`, thân panic `1 of 4 case(s) over the machine baseline:` + `validate NewOrderSingle: … ns/op
   exceeds …`. Cột verdict mà `ab_extract` phải đọc ra: `over`, `under`, `in`, `none` — **đủ bốn**.
   Dán **nguyên văn** vào heredoc của `check-ab-rotation.sh`, kèm dòng đầu ghi lệnh, ngày,
   `<cpu>`, sha binary.
5. `git checkout benches/baselines.tsv && git diff --exit-code benches/baselines.tsv` — quote.

Viết test trước khi viết `ab_extract`: chạy `check-ab-rotation.sh` với `ab_extract` tạm là thân
awk cũ → phải thấy đúng dòng FAIL ở bảng. Viết `ab_extract` → xanh. Đảo ngược lần nữa: **làm
hỏng mỏ neo** (đổi ba dấu cách sau ` ns/op` trong regex thành đúng hai) — kỳ vọng nêu bằng **cơ
chế, không bằng con số**: mọi hàng mang ` ns/op   baseline` lẫn ` ns/op   NO BASELINE` đều
không còn khớp, nên (a) số hàng đọc ra **nhỏ hơn 4** và (b) tên `validate TestRequest, w2w
bytes` (hàng in-band) **vắng** trong danh sách tên — test `same` về hàng in-band phải đỏ. Con số
`got` là con số regex hỏng in ra, đọc và quote; nó phụ thuộc vào cách làm hỏng (trên fixture của
`854fbbb`, không có hàng in-band, manager đọc `got [1]`). Hoàn lại.

**Bước 2 — rehearsal.** `RUSTFLAGS=… cargo bench -p fixbolt-session --bench validate --no-run
--features "$(scripts/check-bench-alignment.sh --features-map | awk -F'\t' '$1=="fixbolt-session"{print $2}')"`
(driver đọc feature map của cây; xây đúng feature để `fresh=true`). Rồi:

```sh
ROUNDS=1 ARMS="c=.:fixbolt-session/validate" CONTROL=c EVIDENCE=target/ab-rehearsal-<k> scripts/ab-rotation.sh
```

k = 1 sạch → `timeline.txt`: `round 1 arm c suite fixbolt-session/validate exit 0 rows 6 over 0 under 0 nobase 6  ok`,
`round 1 complete`; summary chân `over baseline: none`. k = 2 với dòng tạm ép OVER (như bước 1,
`<cpu>` của container) → `exit 101 rows 6 over 1 … OVER`, `round 1 complete`, cột `over 1/1`,
chân `over baseline: 1 (arm, case) pairs`. k = 3 với một dòng tsv **thiếu cột** → harness exit 1
trước khi đo → `exit 1 rows 0 … FAILED`, `round 1 incomplete`, summary `(no complete rounds yet)`.
Mỗi k: quote `timeline.txt` và summary; kết thúc `git diff --exit-code benches/baselines.tsv`.
Nếu hàng *machine is quiet* của container DISQUALIFIED: chạy lại; rehearsal **không phải số đo**.

**Bước 3 — đảo chiều gate `sudo`.**

```sh
mkdir -p target/check-sudo
printf 'sudo -n perf record -e cycles -F 4999 -o d.data -- cargo bench -q -p fixbolt-engine --bench density\n' > target/check-sudo/reversal.sh
scripts/check-sudo-names-what-root-can-find.sh target/check-sudo/reversal.sh; echo "exit=$?"
```

Chờ: `check-sudo: FAIL — target/check-sudo/reversal.sh:1: sudo hands 'cargo' to root by name;
root's secure_path has no ~/.cargo/bin (R2) — give an absolute path`, `exit=1`. Đổi `cargo bench`
thành `/abs/path/density-abc` → `ok — 1 scripts scanned, 1 sudo lines read, 0 findings`, exit 0.
Thêm dòng `sudo mytool --flag` → `FAIL … sudo runs 'mytool' by name, and this script cannot say
root's secure_path finds it (R1) — add it to ALLOW with dpkg -S evidence, or give a path`.
Trên cây thật: `ok — <N> scripts scanned, <M> sudo lines read, 0 findings` với M ≥ 17 (số dòng
`sudo` đếm được hôm nay, xem *Những gì đã biết chắc*) — **đọc** N, M và quote.

**Bước 4.** JSON fixture: `[{"next":<now+3600e6>,"unit":"apt-daily-upgrade.timer",…},
{"next":<now+13*3600e6>,"unit":"fstrim.timer",…},{"next":null,"unit":"man-db.timer",…},
{"next":<now-300e6>,"unit":"systemd-tmpfiles-clean.timer",…}]` với `now` cố định trong test
(không gọi `date`). Kỳ vọng ở bảng. Chữ hàng `DESIGN.md` §9 (dán nguyên, sau hàng *Nothing else
is running on the machine*):

> | **No systemd timer due inside the campaign window** | `[measured 2026-09-22]` boot D lost rounds 13–20 to `apt-daily-upgrade.timer` at 06:51 while the quiet row read green before and after; a quiet check reads one second and says nothing about what systemd has already scheduled. `check-machine.sh` reads `systemctl list-timers --all --output=json` and **FAILs** on any timer due inside `FIXBOLT_TIMER_WINDOW` hours (default 12, the longest campaign on record), naming the units; the fix is `systemctl stop`, not `disable`, so the next boot restores them. `ab-rotation.sh` refuses to start on that FAIL ([ADR-0093](../decisions/ADR-0093-a-campaign-driver-is-committed-sudo-in-a-committed-script-names-what-root-can-find-and-a-timer-due-inside-the-window-is-a-fail-row.md)) |

(Khi dán vào `DESIGN.md`, đường dẫn link là `decisions/ADR-0093-…` — bớt `../` — vì `DESIGN.md`
nằm ở `docs/`, còn file kế hoạch này nằm ở `docs/plans/`.)

Preflight của driver: `--dry-run` **không** đọc hàng này (dry-run không đụng máy); run thật đọc
trước round 1; trên container hàng là UNKNOWN → driver **chạy** (chỉ FAIL mới từ chối) — quote
dòng driver in ra khi UNKNOWN (`timers: unknown — <lý do>; not refusing`).

**Bước 5 — câu chữ `STATUS.md`:**

- Hàng 97: giữ nguyên phần số; thêm đầu hàng `**Detector record fixed 2026-09-2x by ADR-0092**
  (`ab_extract` anchors on the row, exit status read, `OVER` is its own state, `--summary` has an
  `over` column and `--reextract`). **The slowdown is untouched and the baselines are not moved**
  — ADR-0090 decision 4 still holds. Open until 7a re-reads `d4m/` on the desk and item 93 concludes.`
- Hàng 98: `**(a) CLOSED by ADR-0093 decision 2** — `scripts/check-sudo-names-what-root-can-find.sh`,
  red on boot D's exact line in `check-sudo-verdicts.sh`. **(b) CLOSED by decision 3** — the `no
  timer due` row, `FIXBOLT_TIMER_WINDOW` default 12 h, `FAIL`. **Drivers: decision 1** — in
  `scripts/` before the boot; the three dead files stay dead.` Gạch cả hàng nếu cả ba đóng.
- Hàng 96: thêm `**Experiment designed: ADR-0094** — attribution inside one binary, C1 (`nm`
  shows `bad_nested_count`) checked pre-boot. Waits for the next §9 boot; `ab/validate-no-descent`
  retired.` Không gạch.
- *Not proven*: bullet item 96 đổi thành `**What the ADR-0086 descent costs** — the experiment is
  designed (ADR-0094), not run.` Item 95 giữ.

Hai trang `docs/reference/`: thêm đoạn `## Guarded by` — trang `sudo`: tên gate, tên test, dòng
fixture; trang timer: tên hàng, biến cửa sổ, hàm thuần, và câu *the row prints the window it used,
so a 14-hour campaign under a 12-hour window is still unseen*.

Bước 6 (manager) chạy lại **mọi** lệnh trên tại commit đóng, thêm `cargo test --all`,
`cargo test --no-default-features`, `cargo fmt --all -- --check`, `cargo clippy --all-targets --
-D warnings` (không đổi `crates/` nhưng vẫn là gate mỗi commit), `scripts/check-scratch-fixtures.sh`,
`scripts/check-links.py`, `scripts/check-adr-numbers.sh`; đối chiếu CI run id.

## Tài liệu phải cập nhật

Theo `CLAUDE.md` §4, đi từng hàng:

- [ ] *Gate: cách đo đổi* → `DESIGN.md` §6 *How the benchmarks are run* (bước 5): ba câu về
      exit status, `OVER` là state riêng, `--reextract`.
- [ ] *Hàng OS mới* → `DESIGN.md` §9 **trước**, rồi `docs/hft-playbook.md` §6 (bước 4, 5).
- [ ] *Khuyến nghị vận hành* → `docs/hft-playbook.md` §6 (mục: driver trong `scripts/`, đọc hàng
      timer, gate `sudo`); `docs/best-practices-hft.md` **không sửa** — không có khuyến nghị theo
      mode nào đổi.
- [ ] *Bẫy đã ghi có test canh* → hai trang `docs/reference/` thêm `## Guarded by` (bước 5, cùng
      commit với gate là bước 3/4 — manager gộp khi commit, hoặc bước 5 đi cùng commit đóng).
- [ ] *ADR* → 0092, 0093, 0094 (bước 0); ADR-0086/0090 **nhắc**, không sửa.
- [ ] *Bẫy mới, đã trả giá ở bước 1* → `docs/reference/wc-l-counts-an-empty-capture-as-one-line.md`
      (architect, *Sửa 1*), canh bởi `scripts/check-ab-rotation.sh` mục `=== ab_extract` từ
      `85262e4`; bước 5 **không** cần thêm gì cho trang này.
- [ ] `STATUS.md` hàng 96, 97, 98; *Not proven*; handoff *Start here* của manager (bước 5, 6).
- [ ] `docs/CONFORMANCE.md`: **không** — không có kết quả conformance nào; số `N scripts, M sudo
      lines` là số đếm của gate, ghi ở *Nhật ký*.
- [ ] `docs/internals/`: **không** — không trang nào mô tả `scripts/`; `tools.md` chỉ nhắc script
      theo crate. Nếu reviewer muốn một trang `docs/internals/scripts.md` → việc riêng.
- [ ] `CLAUDE.md`: **không** — không luật nào đổi; gate mới không thuộc bảng §2 (không phải bất
      biến 1–10). `CHANGELOG.md`: **không** — không API công khai.
- [ ] `docs/reference/measured-costs.md`: **không** ở kế hoạch này; hàng 7a ghi khi đọc lại `d4m/`.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Fixture gõ từ trí nhớ thay vì chụp (bẫy `STATUS.md` 2026-09-22 nêu hai lần) | Bước 1 mục 4–5: heredoc mang dòng đầu ghi lệnh + sha binary; `git diff --exit-code baselines.tsv` |
| Regex hàng đo quá chặt: tên dài hơn 34 ký tự → đệm còn một dấu cách | Fixture có `validate NewOrderSingle, w2w bytes` (34+ ký tự) — nếu chưa có, thêm case tên dài vào test bằng một hàng chụp thật khác |
| `{best:>8.1}` với số ≥ 8 ký tự (`82071.1`) — vẫn một dấu cách trước | Test có hàng ≥ 5 chữ số phần nguyên (chụp từ case 33 groups nếu xây `fix50sp2`, hoặc hàng thật của bước 2) |
| `runs.txt` cũ 4 cột làm `ab_summary` crash (`set -u`) | Test 4 cột → `?`; `--summary` chạy trên fixture 4 cột không lỗi |
| `OVER` bị đọc là `ok` hoặc là `DISQUALIFIED` | Test timeline: `OVER` → vòng `complete`; `FAILED` → `incomplete`; ADR-0092 quyết định 2 |
| Hai người cùng sửa `ci.yml` | Chỉ bước 3 sửa `ci.yml` |
| Gate `sudo` đỏ trên `main` từ lúc sinh | Bước 3 chạy trên cây trước khi thêm vào CI: `0 findings`, quote N, M |
| Gate `sudo` bỏ qua bẫy thật (từ lệnh là `perf`) | Fixture dòng boot D → phải FAIL R2; đảo chiều bằng file `target/check-sudo/reversal.sh` |
| Dòng nối `\` giấu workload sang dòng sau | Fixture hai dòng nối → FAIL; script nối dòng trước khi quét |
| `check-scratch-fixtures.sh` phạt script mới vì `mktemp` | Script mới không `cd` vào scratch; `check-scratch-fixtures.sh` chạy lại ở bước 3 |
| `timers_verdict` gọi `date` bên trong (không thuần, test phập phù) | Hàm nhận `now_usec` làm tham số; test dùng `now` cố định |
| `next` là µs, code so với giây | Fixture `now+3600e6` và window tính bằng giây × 1 000 000 trong hàm |
| Hàng timer thêm một `unknown` làm CI runner/laptop đổi verdict | `bench.sh` bỏ qua exit; verdict `unknown > 1` đã đúng trên các máy đó; bước 4 quote block trên container |
| Driver từ chối chạy vì hàng timer UNKNOWN (container, macOS) | Chỉ FAIL mới từ chối; quote dòng `not refusing` ở bước 4 |
| Dòng tạm trong `baselines.tsv` sót lại | Mỗi bước 1, 2 kết thúc bằng `git diff --exit-code`; harness từ chối dòng sai ở mọi mode |
| Hai nhánh cùng lấy số ADR | `git branch -r` quét 2026-09-22: không nhánh nào có 0092+; manager chạy `scripts/check-adr-numbers.sh` trước commit |
| Shellcheck có cảnh báo **sẵn** trong `ab-rotation.sh` khi thêm vào danh sách | Bước 1 chỉ chạy shellcheck và quote; bước 3 chỉ thêm hai file mới vào danh sách CI; `ab-rotation.sh` vào danh sách khi sạch — nếu không, báo |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Container không xây được bench `validate` (`vendor/`, thời gian) | Thấp | `fetch-quickfix-assets.sh` có sẵn; bench `validate` xây vài phút; nếu không được → fixture chụp trên laptop dev **có ghi máy**, vẫn là chụp thật |
| Hàng *machine is quiet* của container DISQUALIFIED mọi lần → rehearsal bước 2 không chạy được | Trung | Chạy lại lúc rỗi; nếu vẫn không → rehearsal chuyển sang laptop dev, ghi máy; test thuần vẫn đóng bước |
| Hình hàng của harness đổi sau này mà không ai đổi fixture | Trung | Chấp nhận trong ADR-0092 *Bad*: `rows 0` → `FAILED` ở run thật đầu tiên, ồn, không sai số |
| ALLOW thiếu một tên hợp lệ → gate đỏ khi có script mới | Thấp | Đúng hướng over-match; thêm tên kèm bằng chứng `dpkg -S` trong header |
| Cửa sổ 12 h không đủ cho chiến dịch dài hơn | Thấp | Hàng in cửa sổ ở mọi verdict; chiến dịch đặt `FIXBOLT_TIMER_WINDOW` |
| 7a trên bàn đo cho cột `over` khác tám median đã công bố | Trung | Dừng, báo architect — hoặc extractor sai, hoặc số công bố sai; cả hai đều phải ra ánh sáng |

## Ngoài phạm vi

- **Ghi lại `benches/baselines.tsv`** — ADR-0090 quyết định 4 giữ cho commit sau khi item 93 có
  đủ verdict; dời đường bây giờ là viết cái chậm vào máy dò. `bench.sh --strict` **vẫn đỏ** trên
  bàn đo, cố ý.
- **Cái chậm** (item 93, 95) — ba góc nhìn của một việc; ở đây chỉ sửa máy dò.
- **Sửa `harness.rs`** (bỏ ` ns/op` khỏi chuỗi báo cáo, hoặc in JSON) — ADR-0092 *Alternatives*;
  arm là binary cũ.
- **Dựng lại `run-d2.sh`, `run-d3.sh`, `run-d5.sh`** — mất rồi, và không phải bằng chứng.
- **Chạy thí nghiệm ADR-0094** — cần boot §9; hàng 7b.
- **Đọc lại `d4m/`** (7a) — cần bàn đo bật (dòng grub nào cũng được).
- **Gate `sudo` đọc prose** (ô kế hoạch) — G3 của ADR-0093; luật là driver vào `scripts/`.
- **Đổi `quiet_status` của driver** theo hàng timer từng vòng — hàng quiet mỗi vòng đã đủ (boot D
  chứng minh); preflight một lần là đủ.
- Mọi phép đo và mọi con số.

## Sửa 1 — 2026-09-22, sau khi bước 1 chụp fixture và một phép đảo ngược không ra số như dự đoán

**Cái gì sai.** Bản đầu của kế hoạch này tự mâu thuẫn: *Cách làm* điểm (1) đòi fixture có
`OVER` + `UNDER` + `NO BASELINE` + **in-band**, nhưng *Cách kiểm chứng* bước 1 chỉ bảo thêm
**hai** dòng tạm — và bench `validate` không feature có đúng **bốn** case, nên hai dòng ép OVER
và UNDER để lại hai case `NO BASELINE`, **không thể** có hàng in-band. Fixture chụp ở `854fbbb`
đọc cột verdict `over / under / none / none`: giá trị `in` — hàng **thường gặp nhất** trong mọi
rotation thật, hàng mang mọi phép đo bình thường của boot D, hình ` ns/op   baseline … = [floor,
ceiling]` **không mark** — không được ghim bởi gì cả, dù ADR-0092 quyết định 1 kể nó là một
trong bốn giá trị.

**Cái gì bắt được.** Không phải test xanh: `check-ab-rotation.sh` đọc `pass 15 fail 0`. Là
**phép đảo ngược thứ hai ra số khác dự đoán**: kế hoạch viết `want [4] got [2]` với lý do "hàng
in-band biến mất"; chạy thật cho `got [1]`, vì không có hàng in-band nào để mất. **Và con số
`1` đó cũng không phải số hàng**: phép đếm là `printf '%s\n' "$extracted" | wc -l`, mà `printf`
tự thêm dấu xuống dòng cho một chuỗi rỗng, nên **0 hàng đếm thành 1** — `ab_extract` thực ra
không đọc ra hàng nào cả (manager tái hiện: `printf '%s\n' "" | wc -l` → 1; `printf '%s' "" |
grep -c .` → 0). Từ `85262e4` phép đếm là `printf '%s' "$extracted" | grep -c .` và phép đảo
ngược đọc `got [0]`; bẫy này có trang riêng:
[wc-l-counts-an-empty-capture-as-one-line](../reference/wc-l-counts-an-empty-capture-as-one-line.md).
Người xây báo con số thật thay vì ép cho khớp — đúng `CLAUDE.md` §10, *read the output, not
the exit status*. Manager tái hiện bằng tay rồi mới gửi về đây.

**Sửa gì.** Ba chỗ trong file này, cùng nhau: *Cách làm* (1) nói **ba dòng tạm**, mỗi giá trị
verdict một case; *Cách kiểm chứng* bước 1 mục 2–4 ghi cách đặt case thứ ba in-band (số đọc từ
lần chạy đầu, margin 1,35 — đỉnh thang, không nới thêm) và nội dung file chờ đợi là **một** OVER,
**một** UNDER, **một** in-band, **một** `NO BASELINE`; phép đảo ngược thứ hai nêu kỳ vọng bằng
**cơ chế** (hàng in-band vắng tên, số hàng < 4) thay vì một con số, vì con số phụ thuộc vào cách
làm hỏng regex và vào fixture. Fixture trong `scripts/check-ab-rotation.sh` được senior developer
chụp lại theo mục 2–4 mới — việc đó **không** thuộc kiến trúc sư và không nằm trong file này.

**Vì sao ghi ở đây, không ở `docs/reference/`.** Hình dạng đáng nhớ là: *một công thức kiểm chứng
không sinh ra được đúng cái case mà phần thiết kế của nó đòi, và test xanh không thấy — chỉ một
phép đảo ngược được **đọc** chứ không được coi là xanh mới thấy.* `docs/reference/` đã có trang
cho hai nửa của nó (*reading-the-output-you-grepped-for*, *a-green-fraction-over-a-scenario-that-
never-ran*); cái mới ở đây là ở **cấp kế hoạch** — hai mục của một kế hoạch nói hai điều khác
nhau về cùng một fixture — và cái canh nó là luật đã có: viết câu FAIL trước, chạy, **so**. Không
thêm trang; thêm một hàng vào *Bẫy đã lường trước* của mọi kế hoạch sau là việc của
`_template.md`, và đó là quyết định của chủ dự án, không của bản sửa này. Manager ghi vào
*Nhật ký* hàng 1 con số hai phép đảo ngược đã đọc.

## Sửa 2 — 2026-09-22, sau khi bước 3 hạ cánh: một tên job sai, và một khoảng trống gate tự tạo

**(a) Kế hoạch gọi sai tên job CI.** Hàng 3 của *Chia việc* (và brief chép từ đó) bảo đặt gate
vào job **`gates`**, "một step sau `check-scratch-fixtures.sh`". Đọc `ci.yml`:
`check-scratch-fixtures.sh` chạy trong job **`lint-config`** (`:76`); job tên `gates` có thật
(`:376`) và không chạy script đó. Người xây **báo lên** thay vì tự giải, chọn job có tiền lệ thật;
gate nay ở `lint-config` (`:85`), hai script vào danh sách `shellcheck -S info` của job đó
(`:101`), `check-sudo-verdicts.sh` và `check-ab-rotation.sh` vào `script-logic` (`:129`). Đặt thế
là đúng. Hàng 3 và *Cách làm* Item 98 nay **gọi tên step, không gọi tên job** — "ngay sau step
`check-scratch-fixtures.sh`", "cạnh `check-machine-verdicts.sh`" — để đổi tên job không làm mục
văn bản này mục nát. Không hàng nào khác của kế hoạch nhắc tên job.

**(b) Gate loại chính file test của nó khỏi phạm vi, và ADR-0093 chưa nói.**
`check-sudo-names-what-root-can-find.sh:79` loại **đúng một** file khỏi phạm vi mặc định:
`scripts/check-sudo-verdicts.sh`, vì fixture của nó cố tình byte-identical với dòng xấu thật
(dòng boot D trong đó), đưa vào `sudo_verdict()` dưới dạng chuỗi, không bao giờ chạy; lần quét
đầu thấy 9 "phát hiện", toàn là fixture đọc lại chính mình. Loại là thật thà (tiền lệ:
`check-scratch-fixtures.sh` gọi tên `check-links.py` ra khỏi phạm vi của nó). Hệ quả không được
nói: **một dòng `sudo` xấu thật thêm vào `check-sudo-verdicts.sh` sẽ không bao giờ bị bắt.**
ADR-0093 sửa tại chỗ (còn `Proposed`, `CLAUDE.md` §5): thêm **G5**, và làm mềm **G2** — R2-trong-
`sh -c`/`env` được viết đúng spec nhưng **không có fixture nào**, vì không dòng thật nào trong cây
đi qua đó và hàng 3 không gọi tên một cái.

**Cách đóng G5, rẻ, và thêm fixture cho G2 — bước 3b** (developer, sonnet; chạy khi
`scripts/check-sudo-*.sh` không còn ai viết):

- *Được sửa*: `scripts/check-sudo-names-what-root-can-find.sh`, `scripts/check-sudo-verdicts.sh`.
  *Không được sửa*: `ci.yml` (không đổi step), mọi script khác. §2: không.
- *Cơ chế*: bỏ việc loại theo tên file; thay bằng một **dấu trên dòng** — comment đuôi
  `# check-sudo: fixture` — mà scanner bỏ qua **chỉ trong `scripts/check-sudo-verdicts.sh`**
  (dấu ở file khác không có tác dụng — không mở một cửa thoát chung). Mỗi dòng fixture xấu trong
  file test mang dấu; một dòng xấu **không** mang dấu trong chính file đó bị bắt như mọi file
  khác. Dòng `ok` của gate in thêm `K fixture lines skipped by marker` để số đó lộ trong review.
- *Fixture G2 thêm vào verdict test*: `sudo sh -c 'cargo bench -q'` → `FAIL R2 cargo`;
  `sudo env PATH=$PATH cargo bench` → `FAIL R2 cargo`; `sudo sh -c '/abs/bin --flag'` → `ok`
  (R1 không áp cho thân `sh -c` — G2 vẫn là G2, nay có fixture nói đúng nó che gì).
- *Test viết trước, câu FAIL chờ đợi*: trong `check-sudo-verdicts.sh`, một `same` cho hàm
  `is_marked_fixture`/tương đương với dòng có dấu → `skip`; chưa viết → `FAIL  want [skip] got []`.
  *Đảo ngược thật*: thêm tạm dòng `sudo -n perf record -o x.data -- cargo bench` **không dấu** vào
  `check-sudo-verdicts.sh` → `scripts/check-sudo-names-what-root-can-find.sh` phải in
  `check-sudo: FAIL — scripts/check-sudo-verdicts.sh:<n>: sudo hands 'cargo' to root by name; …
  (R2) …`, exit 1; bỏ dòng → `ok — <N> scripts scanned, <M> sudo lines read, K fixture lines
  skipped by marker, 0 findings` với N **tăng 1** so với trước (file test nay trong phạm vi).
- *Gate đóng bước*: hai script xanh; `shellcheck -S info` sạch; `check-scratch-fixtures.sh` xanh.
- ADR-0093 G5 ghi phương án này là *đề xuất*, không phải đã đóng; bước 3b đóng nó. Nếu bước 3b
  không chạy trong PR này, G5 ở lại trong ADR đúng như đang viết và `STATUS.md` hàng 98 nói rõ.
  **Đã đóng tại `9e33aa9`** — cả hai chiều được đọc (xem *Nhật ký*); ADR-0093 G5 ghi "closed".
- **Sau 3b, câu "G2 untested" của *Sửa 2* hoá ra sai**: R2 **không** tới thân `sh -c` khi dấu nháy
  dính vào từ (`sudo sh -c 'cargo bench -q'` → ok), và R1 gắn cờ dòng sạch (`sudo 'tee' /sys/x` →
  FAIL R1) — một lỗi tokenise chung cho cả ba luật, sửa tại `67e2898`. ADR-0093 sửa lần hai trong
  cùng khối *Revised*: G2 nửa đầu **thay**, không làm mềm; thêm định nghĩa token; **G6** (từ dính
  `&` `;` `|`, header script lúc đó gọi là `G2b` — đổi tên trong script cho khớp **đã làm tại
  `f537e4d`**, `grep -rn G2b scripts/` nay rỗng);
  quyết định 3 nói **UTC**.

## Nhật ký giao hàng

*Điền khi đóng từng bước: commit, gate xanh (trích), CI run id, cái gì chưa làm và vì sao.*

| Bước | Commit | Gate và output (trích) | Chưa làm |
|---|---|---|---|
| 0 | `b93bbf7` | ADR-0092/0093/0094 viết. Manager chạy lại gate **tại chính commit đóng**, đọc output: `python3 scripts/check-links.py` → `no dead internal links`, 2632 link, EXIT=0; `scripts/check-adr-numbers.sh` → `ok - 92 files seen, 92 ADRs, 92 distinct numbers, 92 H1s checked`, EXIT=0. Mốc cây sạch: `cargo test --all` → **exit 0 của `cargo`**, 127 suite `ok`, **839 passed**, 0 `FAILED`; `cargo test --no-default-features` → exit 0, **834 passed**. **CI xanh 14 job / 14 trên `b93bbf7`: run [`35696241608`](https://github.com/tmthang86/fixbolt/actions/runs/35696241608)** — `interop`, `bench`, `deny` trong số đó; không job nào khác `success`. PR [#91](https://github.com/tmthang86/fixbolt/pull/91) draft. Bốn sự kiện manager tự đo (hình dạng dòng báo cáo của harness; `set -uo pipefail` không đọc `$?`; sáu dòng `sudo` trong `scripts/` đều là chuỗi lời khuyên; bẫy sinh ra ở ô D2 của plan boot D) nằm ở *Những gì đã biết chắc* | Kế hoạch **chưa được duyệt** — không bước nào sau 0 được xây. Một bẫy manager tự sập chưa có trang `docs/reference/`: `cargo test --all \| tail -60` rồi đọc `$?` (status của `tail`) — §7 đã gọi tên sẵn, vẫn xảy ra; **không** thuộc phạm vi kế hoạch này, cần chủ dự án quyết có mở việc riêng không |
| 1 | `854fbbb` | `check-ab-rotation.sh` → `pass 15 fail 0`; `shellcheck -S info` hai file → không output, exit 0; `git diff --exit-code benches/baselines.tsv` → sạch. Manager chạy tay trên fixture chụp thật (binary `validate-56b06784da3bc3fc`, sha `4593d071…`, CPU `Intel(R) Xeon(R) Processor @ 2.80GHz`): awk cũ → **6 hàng từ 4 phép đo**, hai hàng ma `cases under their baseline: 1 validate Heartbeat:` 246.1 và `validate NewOrderSingle:` 1219.3; `ab_extract` → 4 hàng `over/under/none/none`. Đảo ngược 1 `want [4] got [6]` **đúng dự đoán**; đảo ngược 2 đọc **`got [1]` trong khi dự đoán `[2]`** — người xây báo số thật | Fixture **không có hàng in-band** — hai mục của kế hoạch nói hai điều (→ *Sửa 1*, `44254b8`) |
| *Sửa 1* | `44254b8` | `check-links.py`, `check-adr-numbers.sh` xanh | Chỉ sửa kế hoạch; fixture chụp lại ở hàng dưới |
| 1 (chụp lại) | `85262e4` | `check-ab-rotation.sh` → `pass 17 fail 0`; `git diff --exit-code scripts/ab-rotation.sh` → sạch — **lỗ phủ, không phải bug**: `ab_extract` đọc đúng hàng in-band thật `validate TestRequest, w2w bytes  311.4 ns/op   baseline 309.6 x1.35 = [229.3, 418.0]`; cột verdict `over/under/in/none`. Đảo ngược 2 nay `want [4] got [0]`, `pass 13 fail 4`. **Con số `1` của đảo ngược trước là `wc -l` đếm chuỗi rỗng thành 1 — số thật là 0** (`printf '%s\n' "" \| wc -l` → 1; `printf '%s' "" \| grep -c .` → 0). Hai probe cho thấy `got [2]` **không thể** đạt bằng bất kỳ cách nới mỏ neo nào (nới hai dấu cách → 4; đòi mark → 3) | Trang tham chiếu + mệnh đề trong *Sửa 1* → `ee26862` |
| *Bẫy `wc -l`* | `ee26862` | `check-links.py`, `check-adr-numbers.sh` xanh; manager đọc lại ba site tải trọng (`w2w-baseline.sh:578` — sống nhờ `else` của `-z`; `check-no-optional-deps.sh:162` — nhờ `head -1 … \|\| continue`; `check-sudo-verdicts.sh:101,121` — pipe thẳng) | Không script nào đổi |
| 3 | `ebe0525` | Gate → `ok — 31 scripts scanned, 17 sudo lines read, 0 findings` (M = 17 đúng kiểm kê: 6 + 8 + 1 + 1 + 1); `check-sudo-verdicts.sh` → `pass 16 fail 0`; `check-scratch-fixtures.sh` → `ok — 32 scripts`; `shellcheck` sạch; `ci.yml` YAML OK; `check-ab-rotation.sh` → `pass 17` **nay chạy trong CI**. Đỏ trước xanh: R2/R3 stub → `want [FAIL R2 cargo] got [ok]`; file `target/check-sudo/` cho đủ ba câu FAIL nguyên văn. Hai bất ngờ người xây tự bắt: dấu gạch nối là ranh giới từ giả (`check-sudo` đọc thành `sudo`), gate đọc chính câu FAIL của nó | Kế hoạch gọi **sai job CI** (`gates` ≠ `lint-config`, → *Sửa 2*); **G2 không có fixture**; một file bị loại khỏi phạm vi (G5, → 3b) |
| *Sửa 2* | `ba6bf04` | `check-links.py`, `check-adr-numbers.sh` xanh | ADR-0093 sửa tại chỗ: G5 thêm, G2 "untested" — **câu đó sai, xem `67e2898`** |
| 3b | `9e33aa9` | Gate → `ok — 32 scripts scanned, 25 sudo lines read, 11 fixture lines skipped by marker, 0 findings` (N 31 → 32); `check-sudo-verdicts.sh` → `pass 20 fail 0`; `check-scratch-fixtures.sh` ok; `shellcheck` sạch. Hai chiều: dòng xấu **không dấu** thêm vào file test → `FAIL — scripts/check-sudo-verdicts.sh:182: sudo hands 'cargo' …`; helper dấu đỏ trước khi có: `want [skip] got []` | **Mở, manager tái hiện**: `sudo sh -c 'cargo bench -q'` → `ok` (false negative); người xây **từ chối** đổi fixture sang cách viết có dấu cách đầu để qua — đúng §10 → `67e2898` |
| 2 | `c320b7a` | `check-ab-rotation.sh` → `pass 31 fail 0`; `shellcheck` sạch; `baselines.tsv` sạch. Manager tự chạy đảo ngược (`ab_suite_verdict` → `echo ok`) → `pass 25 fail 6`, đúng `want [1 2] got [1 2 3]` + năm assert đơn. Ba rehearsal thật khớp *Cách kiểm chứng*: k=1 `exit 0 rows 6 … ok`, `over baseline: none`; k=2 `exit 101 rows 6 over 1 nobase 5  OVER`, vòng `complete`, cột `1/1`; k=3 `exit 1 rows 0 … FAILED`, `round 1 incomplete`. Dự đoán `got [?]` — **thật `got [+9.5%]`** (trước khi có cột, `$NF` đọc `diff%`) | Không `wc -l` nào còn trong file. Container dùng chung: 27–33 % busy loại arm hai lần ở k=3 — ngưỡng 3 % làm việc của nó |
| 4 | `99e8564` | `check-machine-verdicts.sh` → `pass 41 fail 0`; `check-links.py` xanh; hàng trên container nguyên văn: `? ? ?  no timer due   cannot reach PID 1 (…) [window 12h]`; driver: `timers: unknown — … not refusing`; `unknown` 5 → 6, verdict không đổi. Dự đoán `got [PASS]` — **thật `got []`** ×4 (hàm chưa tồn tại). **UTC, không phải giờ địa phương** — người xây giữ, ADR-0093 sửa theo | Nhánh FAIL chỉ chứng minh bằng fixture (không có PID 1 ở đây); hai cảnh báo shellcheck **có sẵn** (SC2317, SC1091), báo không sửa |
| 3c (tokeniser) | `67e2898` | `check-sudo-verdicts.sh` → `pass 34 fail 0`; gate → `ok — 32 scripts, 32 sudo lines, 19 fixture lines skipped, 0 findings`; `check-scratch-fixtures.sh` ok; `shellcheck` năm file sạch. Trước sửa, manager đo: `sudo sh -c 'cargo bench -q'` → ok, `sudo nice -n -20 'cargo' bench` → ok (**false negative**), `sudo 'tee' /sys/x` → FAIL R1 (**false positive**, senior dev tìm ra). Một nguyên nhân: `read -ra` để dấu nháy dính vào từ. Sửa: `unfuse_quotes` (thay bằng dấu cách, không xoá) ở cả ba chỗ tokenise; bốn đảo ngược, manager tự chạy một: `pass 33 fail 1` đúng `want [ok] got [FAIL R1]`. Dự đoán **mười** đỏ — **thật bảy** (ba fixture cũ không phụ thuộc) | **G6** mở theo quyết định: từ dính `&` `;` `\|` không đọc (`'true;cargo bench'` → ok), vì tách `\|` làm `grep -E "cargo\|rustc"` thành finding; ghim bằng fixture chờ `ok`. ADR-0093 sửa lần hai |
| 5 | `f537e4d` | `STATUS.md` hàng 96 (thí nghiệm đã thiết kế, chưa chạy), 97 (máy dò sửa; cái chậm **không** đụng, baseline **không** dời — chờ 7a), 98 **CLOSED** (cả hai bẫy có gate; hàng tự nêu G6 kèm ví dụ đọc `ok`); `DESIGN.md` §6 ba câu, §9 một hàng; `hft-playbook.md` §6; hai trang reference thêm `## Guarded by`; scripts đổi `G2b` → `G6` cùng commit. Manager chạy lại **mọi** gate: `cargo fmt --check` exit 0; `clippy -D warnings` exit 0; `cargo test --all` 839 passed, `--no-default-features` 834 — **đúng hai con số lúc nhánh bắt đầu**, bằng chứng "test cũ xanh không sửa"; `check-sudo-verdicts.sh` `pass 34 fail 0`; gate `ok — 32 scripts, 32 sudo lines, 19 fixture lines skipped, 0 findings`; `check-scratch-fixtures.sh` ok; `check-ab-rotation.sh` `pass 31 fail 0`; `check-machine-verdicts.sh` `pass 41 fail 0`; `shellcheck` năm file sạch; `check-links.py`, `check-adr-numbers.sh` xanh. **CI run [`35704184232`](https://github.com/tmthang86/fixbolt/actions/runs/35704184232) xanh** | Trong chính commit này, lượt sửa ADR-0093 của architect **làm mất bullet R3** (một `re.sub` với lambda ghi `\1` và `\n` nguyên văn thay vì nhóm bắt được) — review senior bắt (F1); không gate nào đọc Markdown tìm vết `sed` |
| Review senior | *(chờ commit của manager)* | Năm finding xác nhận, toàn chữ, toàn file của architect: **F1** ADR-0093 mất R3 + vết `\1`/`\n` (**blocking**); **F3** `STATUS.md` *Do not* nói "parser is the defect" ở thì hiện tại sau khi parser đã sửa; **F5** trang reference `perf-record…` chỉ vào `run-d2.sh` — file đã chết, chỗ đã cấm; **F6** ADR-0092 nói "hai dòng tạm" — chính điều *Sửa 1* ghi là không thể; **F7** ADR-0093 hai lần nói header script gọi `G2b` sau khi cùng commit đã đổi thành `G6`. Thêm: hàng 5 của bảng này trống dù `f537e4d` là commit đóng bước 5; trang `wc -l` trích `check-sudo-verdicts.sh:101,121` đã trôi thành `:216,236`; `DESIGN.md` §6 gọi `FAILED` là "exit không mang hàng nào" — thiếu hai trường hợp (`exit 0` với 0 hàng; exit ≠ 0 với số hàng lệch `<m>` của panic). Tất cả sửa bởi architect, chỉ trong `docs/` và `STATUS.md`; manager tái hiện từng finding trước khi gửi | Hai câu chờ senior developer: "never calls `date`" ở quyết định 3 (code gọi `date -u -d`, GNU-only, đỏ trên macOS) và G2 có nêu `chrt`/`taskset` hay không |

**Hình dạng của cả kế hoạch này, đọc từ bảng trên** — bốn điều đáng giữ lại:

1. **Năm con số dự đoán, bốn sai, và người xây báo số thật cả năm lần**: `got [6]` (đúng);
   `got [1]` thay vì `[2]` (và chính `1` cũng là `wc -l` nói dối, thật là 0); `got [+9.5%]` thay vì
   `[?]`; `got []` thay vì `[PASS]`; **bảy** đỏ thay vì mười. Ba trong năm dẫn tới một lỗi thật
   (hàng in-band thiếu, rồi `wc -l`; `[+9.5%]` và `[]` chỉ là dự đoán sai, không phải lỗi). Câu FAIL
   viết trước rồi **so** là thứ bắt được chúng, không phải test xanh.
2. **Kế hoạch tự mâu thuẫn hai lần**: hai mục nói hai điều về một fixture (*Sửa 1*); lệnh CI gọi
   một job không chạy step nó nêu (*Sửa 2*). Cả hai do người xây phát hiện khi **làm theo đúng
   chữ**, và báo lên thay vì tự sửa.
3. **G5 do manager tìm ra khi kiểm chứng bước 3**, không phải do review.
4. **False positive của R1 do senior developer tìm ra khi sửa false negative của R2** — cùng một
   nguyên nhân, hai dấu; nửa nguy hiểm hơn là nửa không ai báo.
