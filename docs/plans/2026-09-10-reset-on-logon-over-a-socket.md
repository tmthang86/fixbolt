# `ResetOnLogon` được xử ngoài dây, và lý do hoãn nó đã hết hiệu lực

> **Loại:** Plan · **Ngày:** 2026-09-10 · **Trạng thái:** **Đã duyệt 2026-09-10**
> **Phạm vi:** `STATUS.md` item 53. Chạm `tools/interop`, `scripts/interop.sh`, docs.
> **Không chạm** `codec`, `session`, `engine`.
>
> **Chạy được trên Mac.** Bằng chứng cuối cùng vẫn là job `interop` trên CI Linux, nhưng
> `scripts/interop.sh` build `libquickfix` và chạy được tại chỗ, nên vòng lặp phát triển
> không cần bàn Linux.

> **Sửa 1 — 2026-09-10, duyệt lại giữa lúc build, theo Rule Zero.** Bước 2 của plan nói "chạy
> initiator lần thứ hai" mà không nhận ra `tools/interop/initiator.cpp` là **một kịch bản 7 bước
> cố định**, viết cho một phiên mới có `141=Y`. Chạy lần hai vào một phiên đã resume thì sáu bước
> còn lại vô nghĩa: `[đo 2026-09-10]` cả hai arm đều đọc `logon FAIL 141=MISSING`, `order FAIL`,
> và `heartbeat FAIL no unprompted 35=0 in 61 s`. Kịch bản vẫn in `PASS 4/4` bên cạnh những dòng
> đó — đúng hình dạng
> [a-green-fraction-over-a-scenario-that-never-ran](../reference/a-green-fraction-over-a-scenario-that-never-ran.md).
>
> **Phạm vi mở rộng, có duyệt:** thêm cờ `--logon-only` cho `initiator.cpp` — nối, logon, in tape,
> thoát. Nửa sau của §4j dùng nó, nên nó làm đúng việc nó cần và không gì khác: ~1 giây thay vì
> ~60, không dòng FAIL nào.
>
> **`141=MISSING` đã bị điều tra và KHÔNG phải defect.** Đọc `Session.cpp` của oracle thay vì suy
> luận: overload acceptor (dòng 701) chỉ set `141=Y` khi `m_state.receivedReset()` — tức là
> **echo**, y hệt tầng session ở đây (`crates/session/src/lib.rs:3291`). Nhánh `shouldSendReset()`
> ở dòng 687 nằm trong overload **initiator**, phía nói trước. Hai engine hành xử giống nhau; dòng
> FAIL đó là assertion của kịch bản C++, không phải của giao thức. **Ghi lại vì lần đọc đầu tiên
> đã gần như mở một item defect** — và cái chặn nó lại là đọc source của oracle, không phải đọc
> thêm code ở đây.
>
> Kèm theo: `2>/dev/null` cho vòng lặp readiness. `[đo 2026-09-10]` nó in
> `grep: .../fixbolt2.log: No such file or directory` một lần trong một lần chạy **đang pass** —
> race giữa shell và tiến trình vừa spawn, đúng lớp lỗi PR #49.

## Bối cảnh

`ResetOnLogon` được chứng minh ở mọi tầng **trừ** ngoài dây. Danh sách kiểm chứng của đợt B
plan 1 đòi chạy `scripts/interop.sh` hai lần, `Y` và `N` ở cả hai đầu, với lập luận: *một
hướng mà kết quả không đổi thì chưa test cái knob nào cả*. Việc đó đã không được làm.

Lý do ghi trong item 53 là **cấu trúc, không phải đi tắt**: acceptor chỉ đi vào nhánh
`ResetOnLogon` khi session của nó **được resume**, nên kịch bản cần một recovery entry point
nằm trong interop fixture — *"một thay đổi fixture lớn hơn chính cái knob nó định test"*.

**Lý do đó đã hết hiệu lực, và đây là điều duy nhất khiến plan này đáng làm bây giờ.** Item 53
được viết 2026-09-05. Cùng ngày đó, commit `d31db5e` thêm `tools/interop/src/reconnect.rs`,
mang theo toàn bộ đường ống recovery: một `FileJournal` trên đĩa, một `impl Recovery`, và một
công tắc đảo chiều `--no-recovery`. Đường ống đó dựng cho phía **initiator**. Phía acceptor
dùng lại được gần như nguyên vẹn.

## Những gì đã biết chắc

| Sự thật | Nguồn |
|---|---|
| `--role acceptor` gọi `fixbolt::serve(...)` — **không có** recovery, nên session không bao giờ được resume và nhánh `ResetOnLogon` không bao giờ chạy | `tools/interop/src/main.rs:460` |
| `--role acceptor` **đã** đọc file cấu hình qua `--cfg`, `Settings::load` → `into_table` | `tools/interop/src/main.rs:419-433` |
| `ResetOnLogon` **đã** là một key của `settings`, đọc thành flag và đổ vào `Config` | `crates/engine/src/settings.rs:108, 166, 198, 441, 875` |
| `serve_with_recovery(addr, table, app, capacity, limits, recovery, log, handles)` tồn tại, 8 tham số, `#[cfg(all(feature = "standard", unix))]` | `crates/engine/src/lib.rs:1884-1902` |
| Đường ống recovery đã có sẵn trong fixture: `type Disk = FileJournal<64, 1024>`, `struct OnDisk`, `impl Recovery<Disk> for OnDisk`, `OnDisk::probe(path, Durability::Async)` | `tools/interop/src/reconnect.rs:49, 60, 151-189, 297` |
| `--no-recovery` là công tắc đảo chiều đã có, dùng `NoRecovery` | `tools/interop/src/reconnect.rs:255-272` |
| `reconnect.rs` được thêm 2026-09-05, `d31db5e` — **cùng ngày item 53 được viết** | `git log --diff-filter=A -- tools/interop/src/reconnect.rs` |
| Kịch bản reconnect đã dựng C++ acceptor **hai lần** và đã đặt cả ba `ResetOn*` là `N` một cách có chủ đích, kèm lý do: dưới `Y` thì cả hai đầu restart về 1 và **một engine hỏng vẫn pass** | `scripts/interop.sh:393-400` |
| Chữ cái kịch bản cuối cùng đang dùng là **§4i** | `scripts/interop.sh:64, 143` |
| Điểm số hiện tại: `7/7 + 8/8 + 6/6 + 6/6 + 6/6 + 9/9 + 5/5 + 3/3` | `STATUS.md`, run `34372299308` |

## Cách làm

**§4j: fixbolt là acceptor, bị giết và dựng lại, `ResetOnLogon` chạy hai giá trị.**

1. **`--role acceptor` nhận `--journal <path>`.** Có cờ thì gọi `serve_with_recovery` với
   `OnDisk` + `Disk` (kéo hai kiểu này lên khỏi `reconnect.rs` vào một module dùng chung, hoặc
   `pub(crate)` từ `reconnect.rs` — chọn cái ít di chuyển code hơn khi viết). Không có cờ thì
   giữ nguyên `fixbolt::serve` như hôm nay, để **không kịch bản nào đang xanh bị đổi đường đi**.
2. **Kịch bản §4j trong `scripts/interop.sh`.** libquickfix làm initiator; fixbolt acceptor
   chạy với `--journal`. Trình tự: logon, vài application message, giết fixbolt acceptor, dựng
   lại **cùng journal path**, cho initiator nối lại.
3. **Chạy hai lần, `ResetOnLogon=Y` rồi `N`, chỉ đổi một dòng trong file `--cfg`.**
   - `N` → phiên tiếp tục: `34=` sau khi nối lại phải **lớn hơn** số cuối trước khi chết.
   - `Y` → cả hai đếm lại: `34=1` trên `Logon` sau khi nối lại.
   Hai kết quả **phải khác nhau**. Giống nhau nghĩa là knob chưa được test — đúng lập luận đã
   mở item 53.
4. **Đảo chiều:** chạy §4j với `Y` nhưng khẳng định như `N`, xác nhận nó đỏ, rồi trả lại.
5. **Docs**, xem mục dưới.

File chạm: `tools/interop/src/main.rs`, `tools/interop/src/reconnect.rs` (chỉ mở visibility),
`scripts/interop.sh`, `docs/SESSION-BEHAVIOUR.md`, `docs/CONFORMANCE.md`, `STATUS.md`.

## Bất biến bị đụng tới

Không đụng `codec`, `session`, `engine` lẫn `transport` — chỉ `tools/` và `scripts/`. Ba điều
vẫn phải đi qua tay:

- **Bất biến 3** — 59 định nghĩa vẫn phải 59/59; plan này không sửa session nên đây là kiểm tra
  chứ không phải rủi ro.
- **Bất biến 6** — `tools/interop` phụ thuộc `fixbolt-engine` với feature mặc định, và đó
  chính là hình dạng đã làm CI xanh về một build chưa từng xảy ra (`CLAUDE.md` §2, mục 6).
  `scripts/check-no-optional-deps.sh` là gate, hỏi từng crate một.
- **Bất biến 10** — plan này không sinh số hiệu năng nào. Nếu có, nó phải kèm benchmark, máy,
  và cấu hình §9.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | `--journal` trên `--role acceptor`, đi qua `serve_with_recovery`; không cờ thì đường đi cũ nguyên vẹn | — |
| 2 | §4j trong `interop.sh`: giết và dựng lại fixbolt acceptor, cùng journal | 1 |
| 3 | §4j chạy hai giá trị `ResetOnLogon`, khẳng định hai kết quả **khác nhau** | 2 |
| 4 | Đảo chiều bước 3, thấy đỏ, trả lại, thấy xanh | 3 |
| 5 | Docs theo bảng §4, gạch item 53 khỏi *Not proven* nếu nó có mặt ở đó | 4 |

## Cách kiểm chứng

- `scripts/interop.sh` chạy tại chỗ, đọc **từ dòng đầu đến dòng cuối**, không đọc dòng PASS.
  Tám điểm số cũ phải nguyên vẹn, cộng thêm §4j.
- **Transcript ngoài dây là bằng chứng, không phải step line.** Chép ra `34=` của `Logon` sau
  khi nối lại, ở cả hai giá trị knob, và đặt cạnh nhau trong nhật ký giao hàng.
- **Không có dòng lỗi shell nào trong output.** §4h thêm ~170 dòng shell và
  `reading-the-output-you-grepped-for.md` là về đúng lớp lỗi này; §4j thêm nữa.
- `cargo test --all` và `cargo test --all --no-default-features`, cả hai đọc output chứ không
  đọc exit code — `[measured 2026-09-08]` một pipeline `| tail -60` đã báo `exit code 0` cho
  một suite không hề compile.
- Một CI run xanh, gọi tên bằng id, **cho đúng commit được đóng** (§9 ô cuối).

## Tài liệu phải cập nhật

- [ ] `docs/SESSION-BEHAVIOUR.md` — hàng `ResetOnLogon`, **gọi tên §4j** là thứ canh nó ngoài dây
- [ ] `docs/CONFORMANCE.md` — điểm số interop mới, kèm lệnh, máy, CI run id
- [ ] `STATUS.md` — item 53 đóng; *Not proven* đọc lại từng dòng (`CLAUDE.md` §4, hàng cuối)
- [ ] `docs/reference/` nếu bước 3 hoặc 4 tìm ra bẫy nào — ưu tiên cao nhất

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Hai lần chạy cho **cùng** kết quả và vẫn PASS — knob chưa được test | Bước 3 khẳng định hai kết quả khác nhau; giống nhau là đỏ |
| Journal path bị dọn giữa hai lần dựng, nên `N` cũng restart về 1 và trông như `Y` | Kiểm file journal còn tồn tại và khác rỗng trước khi dựng lại; khẳng định riêng |
| Initiator C++ tự có `ResetOnLogon` của nó, che mất hành vi phía fixbolt | File cfg của libquickfix đặt cả ba `ResetOn*` là `N`, như `interop.sh:393-400` đã làm |
| Lỗi shell trong heredoc, job vẫn PASS | Grep output cho `command not found` / `: not found`; đã cháy một lần ở PR #49 |
| `--journal` mặc định bật, làm đổi đường đi của tám kịch bản đang xanh | Không cờ = `fixbolt::serve` như cũ; tám điểm số cũ là bằng chứng |
| Kịch bản exit sớm, in ít step hơn, vẫn để lại PASS | `interop.sh:720` đã ghi lớp lỗi này; đếm step lines cho §4j |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Giết và dựng lại acceptor có tính thời gian, kịch bản thành flaky trên CI | **Trung bình** | Dùng đúng cơ chế settle của các kịch bản reconnect đã có (`interop.sh:422-470`), không phát minh cái mới |
| `OnDisk`/`Disk` kéo lên module dùng chung làm vỡ `--role reconnect` | Thấp | Ba kịch bản reconnect (6/6 ×3) là gate; chúng phải giữ nguyên điểm |
| Acceptor resume cần thêm thứ `reconnect.rs` không cần, phát hiện lúc viết | Thấp | Rule Zero: dừng, sửa plan, xin duyệt lại — không âm thầm đi lệch |

## Ngoài phạm vi

- `ResetOnLogout` và `ResetOnDisconnect` — cùng họ, khác nhánh, plan khác.
- Sửa `serve_with_recovery` hay bất cứ thứ gì trong `crates/`.
- Đo đạc hiệu năng, bàn Linux §9, ba job CI `interop`/`bench`/`deny` chạy tại chỗ.
- Item 57, 58 và hàng đợi `[to testing-skills]` — `CLAUDE.md` §11 nói PR ngược lên upstream mở
  khi dự án được triển khai, không phải khi một plan đóng.

## Nhật ký giao hàng

**2026-09-10 — cả năm bước xong, trên `plan/reset-on-logon-over-a-socket`.** Chưa merge, chưa có
CI run nào được gọi tên: §9 ô cuối còn mở.

**Bước 1.** `--role acceptor` nhận `--journal <path>`; có cờ thì đi qua
`fixbolt::serve_with_recovery`, không cờ thì `fixbolt::serve` như cũ. `Disk` và `OnDisk` trong
`reconnect.rs` thành `pub(crate)` và nhận thêm một field `tag` cho tiền tố in ra — nếu không, một
dòng `interop-reconnect: resuming` phát ra từ acceptor sẽ gọi sai tên kịch bản trong một transcript
mà script này grep.

**Bước 2-3.** §4j trong `scripts/interop.sh`, cổng thứ sáu (`PORT6`, 15649). Chạy hai arm, `N` rồi
`Y`, mỗi arm dựng acceptor hai lần trên cùng journal.

`[đo 2026-09-10]` kết quả, đọc từ frame **đến** phía đối tác chứ không từ log của chính engine này:

| `ResetOnLogon` | Logon lần 1 | Logon lần 2 (phiên resume) |
|---|---|---|
| `N` | `34=1` | **`34=11`** — numbering tiếp tục |
| `Y` | `34=1` | **`34=1`** — đếm lại |

`interop-reset: PASS 4/4`. Tám kịch bản cũ nguyên điểm: `7/7 + 8/8 + 6/6 + 6/6 + 6/6 + 9/9 + 5/5 +
3/3`. Không dòng lỗi shell nào trong toàn bộ output (`grep -E 'command not found|: not found|No such
file'` → rỗng).

**Ba thứ đi chệch trong lúc build, cả ba ghi lại.**

1. **`declare -A` là bash 4, macOS chạy bash 3.2.** Lần chạy đầu của §4j chết ở
   `declare: -A: invalid option` **sau khi** tám kịch bản trên đã pass. Thay bằng hai biến thường.
   Một gate chỉ chạy được trên máy CI là một gate người viết code không dùng được.
2. **Kịch bản in `PASS 4/4` cạnh một tape đầy dòng `FAIL`** — xem *Sửa 1* và
   [a-driver-reused-outside-its-scenario](../reference/a-driver-reused-outside-its-scenario.md).
   Sửa bằng `--logon-only`.
3. **`grep` thiếu `2>/dev/null`** in `No such file or directory` một lần trong một lần chạy đang
   pass — race giữa shell và tiến trình vừa spawn.

**Bước 4 — đảo chiều.** Không cần dựng riêng: **arm `Y` chính là phép đảo chiều của arm `N`**, cùng
một kịch bản, khác một dòng cfg, và assertion *hai arm không được giống nhau* là thứ đỏ nếu knob
không được test. Đây là hình dạng
[a-reversal-needs-an-input-where-the-answers-differ](../reference/a-reversal-needs-an-input-where-the-answers-differ.md)
đòi, và nó nằm sẵn trong gate chứ không phải một lần chạy tay.

**Bước 5 — docs.** `SESSION-BEHAVIOUR.md` (hàng `ResetPolicy`, gọi tên §4j **và** hai overload của
QuickFIX kèm số dòng), `CONFORMANCE.md` (hàng `interop-reset:` mới, **cộng một caveat cũ được gạch**:
*"the reconnect scenarios do not cover a fixbolt process that restarts"* — §4j làm đúng việc đó),
`docs/reference/a-driver-reused-outside-its-scenario.md` mới, `STATUS.md` (*Start here* + item 53
đóng, nguyên văn cũ giữ lại).

**Gate đã chạy, đọc output chứ không đọc exit code:** `cargo test --all` **623 passed / 0 failed**
(621 → 623 là hai test của plan 2 trên cùng branch); `--no-default-features` **618 / 0**;
59 định nghĩa xanh **cả hai mode** (`the_fifty_nine_definitions_pass_through_a_real_socket`,
`..._in_standard_mode_too`); `cargo fmt --check` sạch; `cargo clippy --all-targets -D warnings`
sạch; `check-indexing-debt.sh` **181, ceiling 181**; `check-links.py` sạch.

**Chưa làm:** không có CI run nào được gọi tên cho commit này (§9 ô cuối). Không đo hiệu năng, không
chạm `crates/*/src`. `ResetOnLogout` và `ResetOnDisconnect` vẫn chỉ được chứng minh ở tầng session.
