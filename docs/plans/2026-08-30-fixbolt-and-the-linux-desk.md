# Đổi tên sang `fixbolt`, và mở đường cho máy Linux

> **Loại:** Plan · **Ngày:** 2026-08-30 · **Trạng thái:** Đã duyệt
> **Phạm vi:** open item 1; và mở khoá item 6, 10, 11, 12, 13

## Bối cảnh

Chủ dự án vừa quyết hai việc trong cùng một câu: **tên là `fixbolt`**, và **desktop Linux đã
dựng xong**. Hai việc không liên quan nhau về kỹ thuật nhưng cùng một lý do để làm ngay: người
làm sắp chuyển máy, và cần repo ở trạng thái mạch lạc để tiếp tục ở đó.

Phần đổi tên không còn là chuyện thẩm mỹ. `DESIGN.md:21` **đã ghi sẵn từ trước** rằng placeholder
phải đổi *"to clear a collision with `matthart1983/nanofix`"* — và `matthart1983/nanofix` chính là
reference project mà repo này tự đối chiếu (item 12, `CLAUDE.md` §2 rule 7). Tên hiện tại cách nó
đúng một từ.

Phần máy Linux thì ngược lại: §9 có **bảng thiết lập** nhưng **không có quy trình**. Không có
lệnh nào để áp, không có cách nào để kiểm xem đã áp chưa. Bất biến 10 đòi mọi con số phải kèm
"the §9 settings in force" — hiện tại `scripts/bench.sh` in ra `governor unknown`, và trên máy
thật thì nó phải đọc được thật.

## Những gì đã biết chắc

- **97 file** chứa `fixbolt`, hơn 300 lần xuất hiện: 204 chỗ `fixbolt_` (định danh trong code),
  107 chỗ `fixbolt-` (tên crate), 17 file chứa `fixbolt` (tên repo).
- **Không thư mục nào cần đổi tên.** Thư mục là `crates/codec`, `crates/dict`, … không mang tên
  `fixbolt`.
- **`fixbolt` trống trên cả crates.io và GitHub** — tra ngày 2026-08-30, `404` và `0` kết quả.
- **Mọi `fixbolt` trần trong `.md` đều là `matthart1983/nanofix`** — đã liệt kê từng dòng: 15 chỗ
  trong `STATUS.md`, `CHANGELOG.md`, `prior-art.md`, `measured-costs.md`, `DESIGN.md`, ADR-0001,
  ADR-0003. `STATUS.md` item 1 còn nhắc `LMAX-Exchange/nanofix`.
- `scripts/bench.sh` trên container này in `governor unknown`, `no_turbo unknown`,
  `isolcpus 0 setting(s)` — vì container không cho đọc `/sys/devices/system/cpu/cpufreq/`.
- Item **6** cần máy §9; item **10** chỉ cần kernel có `CONFIG_TLS`; item **11, 12, 13** là so
  sánh A/B trên cùng một máy.

## Cách làm

### Phần A — đổi tên

Thay theo thứ tự dài trước ngắn sau, **sau khi che hai chuỗi được bảo vệ**:

| Từ | Thành |
|---|---|
| `matthart1983/nanofix`, `LMAX-Exchange/nanofix` | **giữ nguyên** — che trước khi thay, bỏ che sau |
| `fixbolt` | `fixbolt` |
| `fixbolt-` | `fixbolt-` |
| `fixbolt_` | `fixbolt_` |

Plan và ADR cũ **cũng đổi**, vì thứ bị đổi là **định danh** chứ không phải luận điểm: một plan
ghi `cargo test -p fixbolt-session` sẽ thành một câu lệnh chạy không được. Nội dung lập luận của
ADR không bị đụng — `CLAUDE.md` §5 cấm sửa *substance*, không cấm sửa tên crate.

**Việc đổi tên repo trên GitHub là của chủ dự án, không phải của tôi**, và phải làm **sau** khi
mọi thứ đã push xong — nếu đổi giữa chừng thì remote của phiên này gãy.

### Phần B — máy Linux

- `scripts/check-machine.sh` — mới. Đọc từng dòng của bảng §9 và in **PASS / FAIL / unknown**,
  kèm lệnh để sửa nếu FAIL. `bench.sh` gọi nó thay cho khối `=== machine` tự chế hiện tại, nên
  mọi con số tự động mang theo thiết lập đã được **kiểm**, không phải được **khẳng định**.
- `DESIGN.md` §9 — thêm quy trình: lệnh áp từng thiết lập, và lệnh kiểm.
- `STATUS.md` — item 1 đóng; item 6, 10, 11, 12, 13 mỗi cái ghi rõ **chạy lệnh gì trước**.

## Bất biến bị đụng tới

- **Bất biến 10** (không số nào thiếu máy + thiết lập §9). Phần B **củng cố**: thiết lập chuyển
  từ lời khẳng định sang kết quả đọc được.
- **Bất biến 3** (59 định nghĩa là cổng của session). Đổi tên chạm vào `fixbolt-session`, nên
  phải chạy lại `--test score` và `--test wire` sau khi đổi.
- Không đổi một dòng logic nào. Mọi thay đổi là định danh.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | Đổi tên toàn repo, hai chuỗi ngoài được bảo vệ | — |
| 2 | Toàn bộ gate xanh lại sau khi đổi | 1 |
| 3 | `scripts/check-machine.sh`, `bench.sh` gọi nó | — |
| 4 | `DESIGN.md` §9 quy trình; `STATUS.md` item 1 đóng, 6/10/11/12/13 có lệnh | 1, 3 |

## Cách kiểm chứng

- **Phản chứng cho việc bảo vệ chuỗi ngoài:** sau khi đổi, `grep -rn "matthart1983/fixbolt"` phải
  ra **0 kết quả**, và `grep -rn "matthart1983/nanofix"` phải ra **đúng 20 dòng như trước**.
- **Không còn sót:** `grep -rn "fixbolt"` chỉ còn lại các dòng `matthart1983/` và `LMAX-Exchange/`.
- Gate: `cargo test --all`, `--no-default-features`, `fmt`, `clippy`, `--test score`,
  `--test wire`, `scripts/bench.sh`, `check-no-kernel-sleep.sh`, `check-links.py`.
- **`check-machine.sh` phải FAIL được.** Trên container này nó phải báo FAIL/unknown cho
  governor và isolcpus — nếu nó in PASS ở đây thì nó không đọc gì cả.
- CI xanh, ghi lại run id (§9 ô cuối).

## Tài liệu phải cập nhật

- [x] `DESIGN.md` §3 (tên crate) và §9 (quy trình) — bảng §4 dòng "rename a crate"
- [x] `README.md` layout
- [x] `Cargo.toml` members
- [x] `CHANGELOG.md`
- [x] `STATUS.md` item 1 đóng; 6/10/11/12/13 có lệnh chạy
- [x] `CLAUDE.md` — tên repo trong phần đầu

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| `sed` mù biến `matthart1983/nanofix` thành `matthart1983/fixbolt` — **hỏng một trích dẫn ngoài** | `grep` đếm đúng 20 dòng trước và sau |
| Đổi `fixbolt` trước `fixbolt` → ra `fixboltengine` | Thay chuỗi dài trước |
| Sót trong file ẩn / script / yml | `grep -rn "fixbolt"` cuối cùng, loại trừ hai chuỗi ngoài |
| `check-machine.sh` in PASS ở mọi nơi vì đọc nhầm đường dẫn | Bắt buộc nó FAIL trên container này |
| Đổi tên repo GitHub giữa chừng làm gãy remote | Chỉ đổi **sau** khi push xong; ghi rõ cho chủ dự án |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Đổi 300 chỗ làm hỏng một chỗ không có test canh | Trung bình | Toàn bộ là định danh — trình biên dịch bắt được. `cargo test --all` là lưới |
| Link GitHub cũ 404 tới khi chủ dự án đổi tên repo | Thấp | GitHub tự chuyển hướng sau khi đổi |

## Ngoài phạm vi

- **Không tự đổi tên repo trên GitHub.** Không phải việc của tôi và sẽ làm gãy remote.
- **Không publish lên crates.io.**
- **Không áp thiết lập §9 hộ.** Script chỉ **đọc và báo cáo**; áp là lệnh root, tuỳ máy, và
  thuộc về người ngồi trước máy đó.
- **Không đặt lại trần bench.** Đó là item 20, cần chính máy này và cần plan riêng.

## Nhật ký giao hàng

**2026-08-30 — cả 4 bước xong.**

**Phần A.** 97 file được viết lại. Hai chuỗi ngoài được che trước khi thay và đếm lại sau:
`matthart1983/nanofix` **20 dòng trước, 20 dòng sau**; `LMAX-Exchange/nanofix` **3 và 3**.
Không thư mục nào đổi tên. Sáu crate giờ là `fixbolt-*`.

**Guard bắt được đúng thứ nó được viết ra để bắt — nhưng là một dương tính giả.**
`grep "matthart1983/fixbolt"` ra 2 kết quả, và cả hai nằm trong **chính file plan này**, nơi tôi
viết chuỗi đó ra làm dấu hiệu. Bài học nhỏ: một guard tìm dấu hiệu hỏng sẽ khớp với tài liệu
đặt tên cho dấu hiệu đó. Loại trừ file plan thì đếm được **0**.

**Cái một lệnh đổi tên máy móc luôn làm hỏng: các câu nói *về* cái tên.** Đổi định danh thì
trình biên dịch canh được; đổi `nanofixengine` → `fixbolt` bên trong câu *"X is a placeholder
name"* biến sáu câu đúng thành sáu câu sai, ở `CLAUDE.md`, `README.md`, `DESIGN.md`,
`CHANGELOG.md` (2 chỗ) và `PRD.md`. Không gate nào bắt được. Tìm bằng cách grep
`placeholder|shortlist|collision with` rồi đọc từng dòng.

**Phần B.** `scripts/check-machine.sh` đọc từng dòng §9 và in PASS/FAIL/unknown kèm lệnh sửa.
`unknown` **không** tính là pass — một container không đọc được `/sys` không được phép trông
giống một máy đã tinh chỉnh. `bench.sh` gọi nó thay cho khối machine tự chế, nên mọi con số
mang theo thiết lập **đã đọc được**, không phải **được khẳng định**.

Phản chứng đã chạy:

| Phản chứng | Kết quả |
|---|---|
| `check-machine.sh` trên container này | `pass 1  fail 5  unknown 3`, `EXIT=1` — đúng như plan đòi |
| `bench.sh --strict` trên container này | `EXIT=1`, từ chối trước khi nhìn tới bất kỳ cái trần nào |
| `bench.sh` không `--strict` | `EXIT=0`, 8/8 target đo được — số đếm vẫn dùng được |

Gate tại chỗ, đọc output:

```
210 passed / 0 failed   cargo test --all  ·  --no-default-features
FMT OK / CLIPPY OK      fmt --check, clippy --all-targets -D warnings
4 passed / 1 passed     -p fixbolt-session --test score, -p fixbolt-engine --test wire
GREEN ok / RED ok       scripts/check-no-kernel-sleep.sh
8 of 8 measuring        scripts/bench.sh, 0 invariant failures
links OK                scripts/check-links.py
```

**Chưa chứng minh:** CI chưa chạy với tên mới. Không đóng plan cho tới khi đọc log CI.

**Việc còn lại của chủ dự án, một bước:** đổi tên repo trên GitHub thành `fixbolt` — **sau** khi
nhánh này đã push. GitHub tự chuyển hướng URL cũ nên PR và nhánh không gãy.
