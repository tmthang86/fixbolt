# Đóng phase 2: bốn việc còn treo, đóng bằng số đếm lệnh và bằng quyết định, không cần boot §9

> **Loại:** Plan · **Ngày:** 2026-09-23 · **Trạng thái:** Đã duyệt (manager, 2026-09-23, theo mandate thường trực của owner)
> **Phạm vi:** `STATUS.md` item 101; phần dư 1.12 của item 99; ghi chú về lý do "≥" của ADR-0094 (item 96); câu hỏi "commit nào trong PR B mang bước chậm của item 95". Nhánh `plan/closing-phase-2`, worktree `../fb-p2close`, từ `main` `213e73c`.

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Chủ dự án muốn **đóng phase 2**. Phần xây của phase 2 đã xong; còn bốn việc treo trong
`STATUS.md` (*Start here — 2026-09-23 (boot F)*, mục *What the boot found*, *Next*, *Not proven*,
và hàng 101 của bảng *Open items*):

- **(a) Item 101.** Bản sửa item 99 (commit `6b2833b`: đọc file baseline vào một buffer cố định
  1 MiB) làm case `walk nested group + varData` của `fixbolt-sbe` chậm đi ~10 % (153.6 … 160.3 →
  174.2 … 176.1 ns), dù không ai sửa `crates/sbe`. Chưa biết cơ chế: vị trí heap sau khối 1 MiB,
  hay bố cục code của chính harness.
- **(b) Phần dư của item 99.** Sau bản sửa, quét độ dài file cho `journal put, 191 bytes, one
  slot` đọc 7.4 … 8.3 ns — max/min 1.12, trong khi plan boot F đặt ngưỡng 1.10.
- **(c) ADR-0094 nói sai lý do cho dấu "≥".** ADR-0094 nói giá của "descent" là cận dưới nếu tầng
  đầu bị inline. Boot F thấy tầng đầu **không** bị inline (một điểm gọi `c7046 → c9680`); thứ bị
  inline vào `validate_with` là vòng lặp của `bad_group_count`. ADR đã Accepted thì không sửa nội
  dung (`CLAUDE.md` §5) — cần một ADR mới.
- **(d) Commit nào trong PR B gây bước chậm +14 % của item 95.** Boot E chỉ chia tới mức merge:
  cả bước nằm trong PR B.

**Ràng buộc cứng:** phiên này không boot được dòng §9 (reboot là mất phiên; cuối phiên chủ sẽ tắt
máy). Máy đang ở dòng grub desktop (không `isolcpus`). Nên plan chia hai phần: (1) những gì quyết
định, xây và chứng minh được ngay trên dòng desktop; (2) những gì thật sự cần §9 — viết thành việc
đầu tiên của lần boot đo sau, **không** chặn việc đóng phase 2.

Ý chính của plan: **số lệnh CPU đã chạy (`instructions:u`) không phụ thuộc bố cục bộ nhớ**, và
harness chạy mỗi case đúng 10 000 + 7 × 200 000 lần, nên số đếm của một binary bench là cố định.
Hai binary cùng số lệnh mà khác thời gian ⇒ khác nhau ở bố cục, không ở lượng việc. Số này đo được
trên dòng desktop, không cần cô lập lõi. Một công cụ đó trả lời được (a), (d), và làm quy tắc cho
lần sau.

## Những gì đã biết chắc

**Từ repo và các lần boot:**

- `harness::suite` nhận cả thân bench là một closure; mọi `Suite::bench::<F>` bị inline vào đó.
  Trong binary `sbe` chỉ có **một** symbol chứa mọi vòng lặp đo:
  `sbe::harness::suite::<sbe::main::{closure#0}>`. `6b2833b` tách phần so baseline ra
  `Suite::figure` (compiler để nó ngoài dòng): symbol đó từ **16 696 byte** (binary trước
  `6b2833b`, sha256 `89bcc880…`, `/home/tmt/Projects/nanofixengine/target/release/deps/sbe-ce2236dc240bf0f7`)
  còn **12 276 byte** + `Suite::figure` 2 293 byte (binary sau, sha256 `1719ddc4…`,
  `/home/tmt/Projects/fb-strict/target/release/deps/sbe-ce2236dc240bf0f7`). Đọc bằng
  `nm -C -S --defined-only`. ADR-0049 chỉ ghim **đầu hàm** vào mốc 64 byte, không ghim gì bên trong
  hàm — nên sửa một phần harness mà không case nào đo cũng làm mọi vòng lặp đo xê dịch.
- Case walk không đụng heap: message là mảng trên stack `nested_buf: [0u8; 128]`
  (`crates/sbe/benches/sbe.rs`), bảng schema là static, và `crates/sbe/benches/alloc.rs` khẳng định
  đường này không cấp phát. Ngưỡng mmap của glibc đã bị loại ở boot F
  (`MALLOC_MMAP_THRESHOLD_=131072` vẫn đọc 174.7 … 177.4).
- `Suite::bench` chạy cố định 10 000 lần khởi động + 7 × 200 000 lần đo mỗi case
  (`crates/codec/benches/harness.rs`). `harness.rs` không đổi giữa `6fbe851` và `e673e8f`; mọi case
  mới của `crates/session/benches/validate.rs` trong khoảng đó nằm sau `#[cfg(feature = "fix50sp2")]`
  (`git diff 6fbe851 e673e8f -- crates/session/benches/validate.rs`), nên bản build mặc định chỉ có
  bốn case FIX 4.4 ở mọi commit.
- Những commit của nhánh PR B (`e673e8f^2`) có chứa `6fbe851`: `31507b5` (merge `main` vào nhánh) và
  mọi commit sau nó — `4992966`, `179ab51`, `a331971`, `fbdf1aa`, `29cf3c3`, `064d90a`, `d7be83d`,
  `eb8e7ae`, `24e0e6e`; cha thứ nhất của `e673e8f` là `29be3bd`. Năm commit đầu của PR B
  (`27051f6` … `0f94bd5`) vào `main`-cộng-PR-B qua merge `31507b5`. (`git merge-base --is-ancestor`,
  `git log --first-parent`.)
- Bước chậm của item 95, boot E S1 (`target/boot-e-evidence/s1m/summary.txt`, n = 21 mỗi arm):
  `wa` → `b1`: Heartbeat 166.8 → 190.8, NewOrderSingle 950.3 → 1012.9, TestRequest w2w 220.6 →
  239.5, NewOrderSingle w2w 950.3 → 1008.6 — tổng **163.8 ns** mỗi vòng cho bốn case.
- Item 96 (boot F, `target/boot-f-evidence/d96f-analysis.txt`): `bad_nested_count` children 8.49 /
  9.68 / 8.28 %; `validate_with::<…, 256>` self **7.51 / 7.28 / 7.48 %**; ns/op 82 666.9 / 82 704.7 /
  82 884.6; hai điểm gọi `c7046` (ngoài) và `c98eb` (đệ quy).
- `baselines.tsv` cấm đổi margin mà không có `n` mới và ngày mới (header, dòng 18–21).
- `scripts/check-sudo-names-what-root-can-find.sh` (ADR-0093) đọc mọi chữ `sudo` trong script đã
  commit; `perf stat` có thể exit 0 khi workload không chạy
  ([perf-record-exits-zero-when-sudo-cannot-find-the-workload](../reference/perf-record-exits-zero-when-sudo-cannot-find-the-workload.md)).
- Bàn: `kernel.perf_event_paranoid = 4` (cần `sudo -n perf`); `perf` 7.0.14, `strace`, `gdb`,
  `setarch` có; **`valgrind` và `ltrace` chưa cài**; `vendor/` không có trong worktree này (có ở
  `/home/tmt/Projects/nanofixengine/vendor`).
- Binary bench đọc `benches/baselines.tsv` theo **đường dẫn tuyệt đối biên dịch sẵn**: binary sau
  `6b2833b` đọc `/home/tmt/Projects/fb-strict/crates/sbe/../../benches/baselines.tsv`
  (`strings`). Xoá worktree `fb-strict` thì binary đó thoát lỗi.

**Probe của architect, 2026-09-23** `[đo trên bàn ở dòng grub desktop, không isolcpus,
mitigations bật, chưa chạy check-machine, taskset -c 6 — chẩn đoán, không phải con số công bố]`:

- Ba cặp xen kẽ `sudo -n perf stat -x, -e instructions:u,cycles:u -- taskset -c 6 <bin>` trên hai
  binary `sbe` ở trên: `instructions:u` trước 4 408 882 945 / …023 / …884, sau 4 407 487 817 / …898 /
  …582 — **chênh −1 395 128 (−0.032 %)**, dao động trong cùng arm 139 và 316 lệnh; `cycles:u` 1.414 …
  1.417 G → 1.508 … 1.566 G; `walk nested group + varData` 151.6 / 158.3 / 157.5 → 175.5 / 176.8 /
  174.9 ns. **Bước +10 % của item 101 tái hiện được trên dòng desktop**, và binary mới làm *ít* việc
  hơn chứ không nhiều hơn.
- Tắt ASLR (`setarch x86_64 -R`) và thêm một biến môi trường `k` byte, `k ∈ {0, 16, 32, 48, 64, 128,
  256, 512, 1024, 2048}` (đòn bẩy của Mytkowicz, dịch vị trí stack): trước 151.8 … 160.4, sau 174.2 …
  178.4 ở **mọi** `k` — vị trí stack không làm arm nào xê dịch.
- `journal` sau sửa (sha256 `22558546…`), 15 run có ASLR và 15 run không: `one slot` 7.4 … 7.7. Hai
  điểm 8.2/8.3 của F8 không xuất hiện; điều này **không** nói gì về nguyên nhân của chúng.

**Tìm trên mạng, 2026-09-23:**

- Mytkowicz và cộng sự, *Producing wrong data without doing anything obviously wrong!*, ASPLOS 2009
  — thứ tự link và kích thước biến môi trường làm đổi hiệu năng đo được vì môi trường nằm trên stack:
  <https://dl.acm.org/doi/10.1145/1508244.1508275>.
- Curtsinger & Berger, *STABILIZER*, ASPLOS 2013 — "một binary chỉ là một mẫu trong không gian bố
  cục, dù chạy bao nhiêu lần"; công cụ ngẫu nhiên hoá lại code/stack/heap lúc chạy, cho C/C++ qua
  LLVM: <https://people.cs.umass.edu/~emery/pubs/stabilizer-asplos13.pdf>,
  <https://github.com/ccurtsinger/stabilizer>.
- iai-callgrind (nay là Gungraun) — bench Rust bằng số lệnh của Callgrind, ổn định kể cả trong CI
  ảo hoá: <https://github.com/iai-callgrind/iai-callgrind>.
- Weaver & McKee — bộ đếm lệnh retired "về lý thuyết là tất định"; nguồn dao động là ASLR, kích thước
  môi trường, errata; sai số hạ xuống dưới 0.002 %:
  <https://web.eece.maine.edu/~vweaver/projects/deterministic/deterministic_counters.pdf>.
- Các cờ căn lề của LLVM (`-align-all-functions`, `-align-all-nofallthru-blocks`,
  `-align-all-blocks`) và cái giá của chúng (thêm `nop`, binary to ra):
  <https://easyperf.net/blog/2018/01/25/Code_alignment_options_in_llvm>; rustc đang ổn định hoá
  `-Cmin-function-alignment`: <https://github.com/rust-lang/rust/pull/142824>.
- `perf report` tính mẫu của hàm bị inline cho hàm chứa nó:
  <https://www.kdab.com/improved-handling-inlined-frames-linux-perf-report/>.
- **Không tìm thấy** harness bench Rust nào tách vòng lặp đo của từng case thành hàm riêng để các case
  không kéo bố cục của nhau; ADR-0102 quyết định 4 là suy luận, không phải vay mượn.

## Cách làm

Hai ADR mới, cả hai `Proposed`, thành `Accepted` khi merge:

- [ADR-0102](../decisions/ADR-0102-a-line-that-moves-while-its-instruction-count-does-not-is-a-layout-move-and-the-count-is-read-off-the-desk.md)
  — số lệnh là "người đi kèm" không phụ thuộc bố cục của mọi case; một dòng dịch chuyển mà số lệnh
  không đổi là **dịch chuyển do bố cục**, một nguyên nhân có tên để re-record; đóng (a), (b), (d)
  với luật phán quyết viết **trước** khi chạy.
- [ADR-0103](../decisions/ADR-0103-the-descents-price-is-exact-for-its-symbol-and-bracketed-for-its-driver-correcting-adr-0094-c3.md)
  — sửa lý do "≥" của ADR-0094: giá 7 021.6 ns là **chính xác** cho symbol `bad_nested_count`; tính
  cả vòng lặp dẫn vào descent thì nằm trong khoảng **[7 021.6, 13 232.8] ns**, đọc từ dữ liệu boot F
  có sẵn. Đóng (c).

Mỗi việc đóng thế nào:

| Việc | Đóng bằng | Cần bàn §9? |
|---|---|---|
| (a) item 101 | **số** (P1: số lệnh bằng nhau, bước thời gian tái hiện, `nm -S`) + **ADR-0102** q.3 gọi tên lớp nguyên nhân: bố cục của chính binary, bị `6b2833b` dời | không |
| (b) phần dư 1.12 của item 99 | **test tất định** (P4: dưới `setarch -R`, địa chỉ mọi lần cấp phát giống hệt nhau ở mọi `k`; bản đảo ngược ở `6b2833b^` phải khác) + **ADR-0102** q.5 chấp nhận độ tản của một run lẻ, nói rõ giá | không |
| (c) lý do "≥" của ADR-0094 | **ADR-0103**, số từ dữ liệu boot F đã có | không |
| (d) commit nào trong PR B | **số** (P3: đếm lệnh từng commit so với cha) theo luật ADR-0102 q.6 | không |

Không việc nào cần §9 để đóng. Dòng baseline nào cũng không bị dời bởi plan này.

**Harness không bị sửa.** Bản sửa tận gốc — mỗi case có hàm đo `#[inline(never)]` riêng
(ADR-0102 q.4) — dời **mọi** dòng của CPU Ryzen 3700X một lần, nên phải re-record toàn bộ ở một
boot §9. Nó được xây bởi plan đầu tiên đằng nào cũng phải re-record toàn bộ (nâng toolchain, sửa
harness, hoặc boot đo đầu tiên của phase 4), trong cùng boot đó.

File tạo hoặc sửa (cả plan):

- Tạo: `scripts/bench-instructions.sh`, `scripts/check-bench-instructions.sh`.
- Sửa: `.github/workflows/ci.yml` (job `gates`, một bước), `docs/reference/measured-costs.md`
  (mục mới), `docs/reference/recording-a-baseline-changed-the-baseline.md` (lần thứ ba),
  `docs/DESIGN.md` §6 (một đoạn), `CHANGELOG.md` (*Unreleased*), ADR-0102 (*Outcome*), ADR-0094
  (một dòng status trỏ sang ADR-0103), `STATUS.md` (manager, bước cuối).
- Evidence (gitignored): `/home/tmt/Projects/fb-p2close/target/p2close-evidence/`,
  `/home/tmt/Projects/nanofixengine/target/baseline-bins/`.
- **Không** đụng `crates/`, `benches/baselines.tsv`, `scripts/bench.sh`, `tools/`.

## Bất biến bị đụng tới

Không đụng `codec`, `session`, `engine`, `transport`, cũng không đụng harness bench. Điều 10 (không
con số hiệu năng nào thiếu benchmark, máy, thiết lập §9): mọi số thời gian của plan này đo trên dòng
desktop và được dán nhãn **chẩn đoán, không công bố**, kèm lệnh và trạng thái máy; số lệnh
(`instructions:u`) là số đếm, không phải số latency, và cũng ghi lệnh, máy, dòng grub. Không dòng
nào của `baselines.tsv` được ghi từ plan này. Walk §2 bằng tay ở bước P6.

## Chia việc

Mọi bước chạy trên bàn ở **dòng desktop**. Bước đo thời gian (chỉ P1) không chạy song song với bước
build. Evidence vào `target/`, không `/tmp` (tmpfs).

| Bước | Vai trò / model | Kết quả | File đụng (không đụng gì khác) | Cần bàn §9 | Gate (lệnh) | Xong khi | Quote về | Phụ thuộc |
|---|---|---|---|---|---|---|---|---|
| **P0** | manager | PR draft cho `plan/closing-phase-2`; commit đầu = plan + ADR-0102 + ADR-0103 (`Proposed`); `mkdir -p target/p2close-evidence`; `uptime` và `pgrep -a llama-server` (phải rỗng) | docs trên | không | `python3 scripts/check-links.py`; `scripts/check-adr-numbers.sh` | hai lệnh xanh, PR draft có số | output hai lệnh nguyên văn | — |
| **P1** | runner (haiku), brief tự chứa — **chạy trước khi ai xoá worktree `fb-strict`** | (1) `sha256sum` hai binary `sbe` (phải bắt đầu `89bcc880` và `1719ddc4`); (2) `nm -C -S --defined-only <bin> \| grep -E 'harness::suite\|Suite>::figure'` cho cả hai; (3) 5 cặp xen kẽ `sudo -n perf stat -x, -e instructions:u,cycles:u -o <file> -- taskset -c 6 <bin>` với stdout (mọi case ns/op) lưu riêng; (4) quét `k` như probe dưới `setarch x86_64 -R`; (5) chép 20 binary bench của `fb-strict/target/release/deps/` mà `f8-strict-3.txt` đã chạy vào `/home/tmt/Projects/nanofixengine/target/baseline-bins/` kèm `SHA256SUMS` | `target/p2close-evidence/p1-*.txt`, `nanofixengine/target/baseline-bins/` | không | `grep -c instructions:u` = 10; mọi số khác 0 | bảng 5 cặp: instructions, cycles, ns/op **mọi** case `sbe`; bảng quét | các bảng nguyên văn; `SHA256SUMS` | P0 |
| **P2** | developer (sonnet) | `scripts/bench-instructions.sh [-n N] [-c CPU] A B` đúng ADR-0102 q.1 (verdict `same-work` / `work-changed` / `unstable`; exit 0 / 3 / 2 cho lỗi; đọc `${PERF:-perf}`; không chữ `sudo` nào; từ chối khi counter thiếu, bằng 0, `<not counted>`, hoặc workload exit ≠ 0); `scripts/check-bench-instructions.sh`: stub `perf` in CSV dựng sẵn — chênh 0.05 % ⇒ `same-work`, 0.2 % ⇒ `work-changed`, dao động 0.02 % ⇒ `unstable`, thiếu counter ⇒ exit 2, workload exit 1 ⇒ exit 2; thêm một bước vào job `gates` của `ci.yml` cạnh `check-w2w-compare.sh` | `scripts/bench-instructions.sh`, `scripts/check-bench-instructions.sh`, `.github/workflows/ci.yml` | không | `scripts/check-bench-instructions.sh`; **đảo ngược**: đổi ngưỡng 0.1 % → 1 % thì ca 0.2 % phải đỏ đúng assertion đó, khôi phục thì xanh; `scripts/check-sudo-names-what-root-can-find.sh`; `bash -n` cả hai; chạy thật `PERF="sudo -n perf" scripts/bench-instructions.sh <sbe cũ> <sbe mới>` | self-test `pass N fail 0`; đảo ngược đỏ rồi xanh; chạy thật in `same-work` | diff từng file; output mọi lệnh nguyên văn, kể cả câu FAIL của lần đảo ngược (viết câu đó ra **trước** khi chạy) | P1 |
| **P3** | developer (sonnet); dừng và báo nếu commit cũ không build | Item 95: tại `6fbe851`, `31507b5`, `4992966`, `179ab51`, `a331971`, `fbdf1aa`, `29cf3c3`, `064d90a`, `d7be83d`, `eb8e7ae`, `24e0e6e`, `29be3bd`, `e673e8f`: `git worktree add --detach /home/tmt/Projects/fb-p2close-bisect/<sha>`, `ln -s /home/tmt/Projects/nanofixengine/vendor vendor`, `RUSTFLAGS="-C llvm-args=-align-all-functions=6" nice cargo bench -p fixbolt-session --bench validate --no-run` (feature mặc định), ghi đường dẫn + sha256 binary; rồi `PERF="sudo -n perf" scripts/bench-instructions.sh -n 3 <cha> <con>` cho từng cặp (cha thứ nhất; `e673e8f` so với `29be3bd` **và** với `24e0e6e`; `31507b5` so với `6fbe851`); thêm `cycles:u` của `6fbe851` để có IPC; bảng ΔI và ΔI/1 410 000; phán quyết theo ADR-0102 q.6; xoá mọi worktree tạm | `target/p2close-evidence/p3-*.txt` (không file nào đã commit) | không | mỗi cặp in một verdict; `git worktree list` sau cùng không còn `fb-p2close-bisect` | bảng 13 arm × (I min/max, ΔI so với cha, verdict); một câu phán quyết: *commit X mang bước*, *bố cục*, hoặc *trộn* | bảng nguyên văn; lệnh build một arm; câu phán quyết kèm phép tính | P2 |
| **P4** | developer (sonnet) | Item 99 phần dư: `sudo -n apt-get install -y ltrace`; build `journal` (`cargo bench -p fixbolt-engine --bench journal --no-run`, cùng `RUSTFLAGS`) ở worktree này và ở worktree tạm `/home/tmt/Projects/fb-p2close-rev` tại `6b2833b^`; với `k ∈ {0, 16, 624, 640, 784, 800, 816, 1024}`: đệm `benches/baselines.tsv` **của đúng cây mà binary đó đọc** bằng dòng `# pad x…` dài `k` byte, chạy `setarch x86_64 -R ltrace -e malloc+calloc+realloc+free+posix_memalign+aligned_alloc -o p4-<tree>-<k>.txt <bin>`, khôi phục bằng `git checkout -- benches/baselines.tsv`; `diff` mỗi `k` với `k = 0`. Nếu `ltrace` không bắt được lời gọi nào: dừng và báo (không tự đổi công cụ) | `target/p2close-evidence/p4-*.txt`; `benches/baselines.tsv` **tạm thời** | không | `git diff --exit-code -- benches/baselines.tsv` ở cả hai cây sau cùng; `git worktree remove` cây tạm | cây này: 7 `diff` rỗng; cây `6b2833b^`: ít nhất một `diff` khác rỗng (đảo ngược) | số dòng mỗi trace; 14 kết quả `diff` (rỗng / số dòng khác); ba dòng khác đầu tiên của bản đảo ngược | P0 (song song được với P2) |
| **P5** | senior developer (opus), context mới | Đọc P1, P3, P4; áp luật ADR-0102 q.3, q.5, q.6 **như đã viết**; viết *Outcome* của ADR-0102; mục mới `## Desk-free, 2026-09-2x: item 101 named by count, item 99's residue by trace, PR B's step counted` trong `measured-costs.md` (mọi số kèm lệnh, máy, dòng grub, nhãn "diagnostic" cho số thời gian); mục `## The third time, in the harness's own code` trong `recording-a-baseline-changed-the-baseline.md`; một đoạn trong `DESIGN.md` §6 (cách xác định nguyên nhân *layout* khi re-record, trỏ ADR-0102); dòng `CHANGELOG.md` *Unreleased*; thêm dòng ghi chú ADR-0103 vào *Boot F, Item 96*. Nếu một luật cho kết quả "không đóng" (P1 `work-changed`, P4 trace khác nhau) — **dừng, báo manager**, không viết lại luật | các file docs nêu ở cột Kết quả; **không** `STATUS.md` | không | `python3 scripts/check-links.py`; `scripts/check-adr-numbers.sh` | mọi số trong docs truy về một file `p*-*.txt` | diff từng file; ba phán quyết, mỗi cái một dòng kèm số | P1, P3, P4 |
| **P6** | manager; **senior review** (opus, context mới, một lần cho PR) trước merge | Kiểm chứng từng finding theo `CLAUDE.md` §12; tự chạy lại `scripts/check-bench-instructions.sh` và **một** cặp của P3 trên commit đóng; walk §2 và bảng §4; `STATUS.md`: hàng 101 đóng, *Not proven* gạch bốn gạch (item 99 bound, item 101 cơ chế, "≥" của item 96, commit PR B) hoặc ghi phán quyết, *Start here* mới với việc đầu tiên cho lần boot sau (mục dưới); ADR-0102/0103 → `Accepted`, dòng trỏ vào ADR-0094; commit đóng; CI xanh trên **đúng** commit đó; merge | `STATUS.md`, status ADR-0094/0102/0103 | không | CI run id xanh 14/14 (hoặc bao nhiêu job CI có); `check-links.py` | run id ghi vào *Start here* | run id; output `check-bench-instructions.sh` và một cặp P3 nguyên văn | P5 |

Ước lượng: P0 5′ · P1 10′ · P2 45–60′ · P3 60–90′ (13 build) · P4 30–45′ · P5 45′ · P6 30′ + CI.
P2 và P4 chạy song song được (file rời nhau, P4 chỉ ghi evidence); P3 sau P2; P1 trước mọi build.

### Việc cho lần boot đo §9 sau — không chặn đóng phase 2

Không có việc nào **bắt buộc**. Hai điều viết vào *Start here* để lần boot sau làm trước tiên:

1. Nếu `--strict` đầu tiên của boot có dòng `OVER`/`UNDER`: **trước mọi thứ khác**, chạy
   `PERF="sudo -n perf" scripts/bench-instructions.sh <binary trong target/baseline-bins/> <binary vừa build>`
   cho bench đó (ADR-0102 q.2). `same-work` ⇒ re-record với nguyên nhân *layout*; `work-changed` ⇒
   tìm trong code. Binary trong `baseline-bins/` đọc file baseline theo đường dẫn `fb-strict`; nếu
   worktree đó đã xoá, tạo lại đúng đường dẫn bằng `git show main:benches/baselines.tsv`.
2. Mỗi lần re-record: chép binary đã đo vào `target/baseline-bins/<bench>-<sha16>` và ghi sha256
   vào thân commit (ADR-0102 q.2).

Bản sửa tách vòng lặp đo (ADR-0102 q.4) **không** thuộc lần boot sau trừ khi plan của boot đó
đằng nào cũng re-record toàn bộ dòng Ryzen.

## Cách kiểm chứng

- **P1**: bảng 5 cặp — hai arm có `instructions:u` chênh ≤ 0.1 % và dao động trong arm ≤ 0.01 %;
  `walk nested group + varData` lệch ~+10 % như boot F; không case `sbe` nào khác lệch **ngược chiều**
  quá band của nó (điều kiện chống "bù trừ" của ADR-0102 q.2). Manager tự chạy lại một cặp.
- **P2**: self-test xanh; đảo ngược đỏ **đúng** assertion đã viết trước ("ca chênh 0.2 % mong đợi
  `work-changed`, nhận `same-work`"), khôi phục xanh; job `gates` của CI chạy nó trên commit đóng
  (đọc log CI, không chỉ màu).
- **P3**: mỗi cặp một verdict, bảng ΔI; câu phán quyết tính ra từ luật q.6 với số trong bảng, không
  đọc bằng mắt. Manager chạy lại cặp mang ΔI lớn nhất.
- **P4**: trace giống hệt nhau ở mọi `k` trên harness hiện tại, khác ở `6b2833b^` — đây là đảo
  ngược, không phải phụ lục.
- **ADR-0103**: không có lần chạy; manager đối chiếu năm số trong bảng của ADR với
  `target/boot-f-evidence/d96f-analysis.txt` dòng 76–122.

## Tài liệu phải cập nhật

Theo bảng §4, từng hàng:

- [ ] Một mẹo/bẫy đo lường → `docs/reference/recording-a-baseline-changed-the-baseline.md` (lần thứ
  ba: sửa harness dời mọi case trong cùng file bench) và `measured-costs.md` (số của P1, P3, P4).
- [ ] Cách một gate được đo → `DESIGN.md` §6 (nguyên nhân *layout* khi re-record, ADR-0102).
- [ ] Kỹ thuật mới / quyết định đảo ngược → ADR-0102, ADR-0103 (đã viết).
- [ ] Chứng minh được một mục *Not proven* → gạch trong `STATUS.md`, cùng commit.
- [ ] `CHANGELOG.md` *Unreleased* — hai script mới.
- Không đổi: `PRD.md`, `GUIDE.md`, `CONFIGURATION.md`, `SESSION-BEHAVIOUR.md`, `CONFORMANCE.md`,
  best-practices, `hft-playbook.md`, `DESIGN.md` §8/§9 — không hàng nào của §4 chạm tới chúng.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| `perf stat` exit 0 khi workload không chạy được dưới `sudo` (tên tương đối, `secure_path`) | script từ chối counter 0 / `<not counted>` / workload exit ≠ 0 — ca stub trong `check-bench-instructions.sh`; `check-sudo-names-what-root-can-find.sh` |
| Binary cũ đọc `baselines.tsv` theo đường dẫn biên dịch sẵn; xoá `fb-strict` là binary sau `6b2833b` thoát lỗi | P1 chạy trước khi dọn worktree; cách tạo lại đường dẫn ghi trong *Việc cho lần boot sau* |
| Số lệnh là của cả process — một case +X và case khác −X cộng lại bằng 0 | P1 đọc ns/op **mọi** case; ADR-0102 q.2 đòi không case nào lệch ngược chiều |
| Commit cũ của PR B không build (thiếu `vendor/`, asset SBE, lockfile) | symlink `vendor`; build lỗi ⇒ dừng và báo, không sửa commit cũ |
| Bisect thời gian bị bố cục làm nhiễu ±10 % | không bisect bằng thời gian; chỉ số lệnh (ADR-0102 lựa chọn bị loại) |
| Đệm file sai cây: binary đọc file của **cây nó được build**, không phải cwd | P4 đệm đúng cây theo `strings <bin> \| grep baselines.tsv`; `git diff --exit-code` ở cả hai cây |
| Worktree tạm lồng trong worktree làm `check-links.py` quét nhầm | worktree tạm ở `/home/tmt/Projects/fb-p2close-bisect/` và `fb-p2close-rev`, không trong `fb-p2close/` |
| Một session khác đang đo trên bàn | P0 kiểm tra `uptime`, `pgrep llama-server`; hỏi manager trước khi build nếu có |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| P1 đọc `work-changed` | thấp (probe đã đọc −0.032 %) | item 101 **không** đóng; `6b2833b` có lỗi thêm việc vào vòng đo; dừng, plan sai, sửa plan |
| P4: trace khác nhau theo `k` trên harness hiện tại | thấp | ADR-0102 q.5 bị gạch; phần dư ở lại *Not proven* kèm trace; ứng viên sửa là (b) của ADR-0096 q.2; dừng, báo |
| P4: `ltrace` không móc được vào binary PIE của Rust | trung bình | dừng, báo; ứng viên thay thế (gdb `dprintf` trên `malloc`) do manager chọn, không do developer |
| P3: 13 build lâu hơn ước lượng, hoặc commit giữa chừng không build | trung bình | commit không build được ghi là "không build"; ΔI tính qua cha gần nhất build được, nói rõ khoảng |
| Ngưỡng 0.1 % / 0.01 % sai với một cặp thật | thấp | cảnh báo sai theo hướng an toàn (`work-changed` ⇒ đi xem code); ghi vào *Outcome* |

## Ngoài phạm vi

- Xây bản sửa tách vòng lặp đo (ADR-0102 q.4) — chờ lần re-record toàn bộ.
- Mua lại chi phí của PR B (item 95) — plan riêng, sau khi P3 chỉ ra commit.
- Chia nhỏ cận trên 13 232.8 ns của ADR-0103 bằng `perf annotate`.
- Mọi đo thời gian trên dòng §9, mọi re-record `baselines.tsv`.
- Items 1 và 14 (không có plan, theo quyết định cũ).

## Nhật ký giao hàng

*(Điền khi đóng từng bước.)*
