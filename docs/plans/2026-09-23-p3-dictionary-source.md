# Phase 3, bước 1–2: từ điển cho crate đã publish — đo Orchestra trước, rồi mới đổi build

> **Loại:** Plan · **Ngày:** 2026-09-23 · **Trạng thái:** Đã duyệt lại — *Sửa 1* (manager, 2026-09-23, theo lựa chọn C của owner và mandate thường trực)
> **Phạm vi:** phase 3, hàng 1 và hàng 2 của *Chia việc* trong
> [2026-09-23-phase-3-scope.md](2026-09-23-phase-3-scope.md). Quyết định và lý lẽ nằm ở
> [ADR-0101](../decisions/ADR-0101-the-shipped-dictionary-is-chosen-by-a-rule-written-before-the-orchestra-diff-is-measured.md)
> (luật chọn nguồn, và kết quả đo) và
> [ADR-0104](../decisions/ADR-0104-the-published-dictionary-is-quickfixs-xml-shipped-with-a-notice.md)
> (ship XML của QuickFIX kèm `NOTICE`).

> **Sửa 1 (2026-09-23).** Bản đầu được manager duyệt cùng ngày. Hàng 1 đã chạy (commit
> `4eaeb53`) và ra **kết quả C**; anh chọn nhánh C là **ship XML của QuickFIX kèm `NOTICE`**.
> Vì vậy hàng 2 của bản đầu (ship Orchestra) **bỏ**, thay bằng hàng 2a–2e dưới đây. Hàng 1
> giữ nguyên để làm hồ sơ. Plan cần duyệt lại vì hàng 2 đổi hẳn nội dung, và vì nó đề xuất sửa
> hai câu trong `CLAUDE.md` (mục *Câu thay trong `CLAUDE.md`*).

## Bối cảnh

Hôm nay `crates/dict/build.rs` sinh bảng từ điển FIX 4.4 từ file XML của QuickFIX nằm trong
`vendor/` — thư mục bị gitignore. Ai tải `fixbolt-dict` từ crates.io sẽ không có `vendor/`, nên
build hỏng, kéo theo mọi crate phía trên. Không sửa chỗ này thì không publish được gì.

Anh đã chọn (ADR-0097, câu Q2): thử file Orchestra (Apache-2.0) trước, giữ XML của QuickFIX làm
trọng tài, và **chỉ chốt sau khi đo**. Đã đo: hai file lệch **687 chỗ, 97 chỗ đụng tới gate**
(kể cả ba group lệch thứ tự thành viên) — theo luật viết sẵn là kết quả C. Anh chọn: **đưa ba
file XML của QuickFIX vào crate, kèm `NOTICE`**.

Hàng 2 bây giờ đơn giản hơn nhiều so với bản đầu: bảng sinh ra **không đổi một byte** — chỉ đổi
chỗ `build.rs` đọc file (từ `vendor/` sang `crates/dict/spec/` trong crate), cộng phần giấy phép
đi kèm.

## Những gì đã biết chắc

Trong repo (đọc ngày 2026-09-23):

- `crates/dict/build.rs` dòng 36–47: đường dẫn mặc định là `../../vendor/quickfix/spec/FIX44.xml`
  (cùng `FIXT11.xml`, `FIX50SP2.xml` cho feature `fix50sp2`); biến môi trường
  `NANOFIX_FIX44_XML` ghi đè được. Build-dependency duy nhất là `roxmltree = "=0.20.0"`.
- `.gitignore` dòng 28 loại `/vendor/`. Cargo cũng bỏ file bị gitignore khi đóng gói.
- Bảng đang được trọng tài kiểm (so với C++ do QuickFIX tự sinh, `docs/CONFORMANCE.md` §2–§3):
  912 / 912 tag, 12 524 / 12 524 cặp (message, tag), 1 708 / 1 708 giá trị enum, 93 loại message,
  thứ tự trong group 730 / 730 (731 tính cả `NoHops(627)` ở header).
- Các test trọng tài đã có: `crates/dict/tests/interop_quickfix_fields.rs`,
  `interop_quickfix_messages.rs`, `interop_quickfix_order.rs`, `enums.rs`. File
  `crates/dict/tests/common/mod.rs` ghi rõ luật: **thiếu file trọng tài là đỏ, không bao giờ là
  bỏ qua**.
- `scripts/check-feature-gated-tests-ran.sh PACKAGE FEATURE LOG` đã chứng minh được "mọi test
  build ra đều đã chạy, không cái nào bị ignore" (R1–R3) — hiện chỉ nhận một feature cụ thể.
- Crate `fixbolt` (library) **không** mở feature `fix50sp2`; `fixbolt-engine`, `-session`,
  `-codec` thì có (chuyển tiếp sang `fixbolt-dict/fix50sp2`).
- Workspace có giấy phép `MIT OR Apache-2.0`.

Ngoài repo (tra ngày 2026-09-23 — nguồn đầy đủ ở ADR-0101 *Research*):

- Repo `FIXTradingCommunity/orchestrations` giấy phép **Apache-2.0**, **không có file `NOTICE`**.
  `OrchestraFIX44.xml` nặng 1 517 010 byte, nén còn ~150 KB — bằng 1,5 % giới hạn 10 MB của
  crates.io. Bản mới nhất ở commit `cd24169a2abd8daba7c360987c7a46ca11873a12` (2026-09-08),
  sha256 `a36262895e90bbcad2948e0c98072173a571a253ba63107a89617441c67a9f86`.
- **Không có file Orchestra cho FIX 5.0 SP2.** Chỉ có FIX 4.2, 4.4, FIX Latest và hai file session.
- Đặc tả Orchestra nói thẳng: **thứ tự field trong file Orchestra không bảo đảm trùng thứ tự
  trên dây**. Mà bộ so của 59 file `.def` so theo vị trí (bất biến 5). Nên thứ tự trong group là
  mặt phải so kỹ nhất.
- QuickFIX/J sinh FIX Latest và FIXT 1.1 từ Orchestra, nhưng **không dùng** file Orchestra FIX
  4.4 — FIX 4.4 của họ vẫn là `FIX44.xml` riêng.
- Một công cụ bên thứ ba (`fixaudit`, gói `fixorchestra` trên PyPI) từng so một bản Orchestra
  FIX 4.4 **cũ** với FIX Repository 2010: khớp 912 field, 93 message, lệch **một** chỗ — Logon
  (35=A) thiếu group `NoMsgTypes`. Bản hiện tại còn lệch không thì spike sẽ nói.
- XML của QuickFIX cũng không phải đặc tả: QuickFIX/J ghi nhận `StipulationValue(234)` bị giới
  hạn chặt hơn văn bản FIX 4.4 (QFJ-757). Trọng tài cuối cùng khi hai file cãi nhau là văn bản
  *FIX 4.4 with Errata 20030618*.
- Metadata trong file Orchestra ghi *"Copyright (c) FIX Protocol Ltd. All Rights Reserved."*,
  trong khi repo và trang fixtrading.org nói Orchestra dùng Apache-2.0. Không tìm thấy văn bản nào
  của FIX Trading Community giải thích hai dòng này đi cùng nhau thế nào.

Kiến trúc sư đã **đếm thẳng các phần tử** trong file Orchestra (912 field, 93 message, 92
group, 247 codeSet, 1 728 code) để thiết kế bộ đọc — nhưng **chưa so với QuickFIX**, có chủ ý:
luật ở dưới phải được viết khi chưa ai biết kết quả.

Thêm sau khi hàng 1 chạy (2026-09-23 — nguồn đầy đủ ở ADR-0101 *Result* và ADR-0104 *Research*):

- **Kết quả spike** (`python3 scripts/dict-diff.py`, commit `4eaeb53`): 687 dòng lệch, 97
  gate-visible, 590 quiet → **C**. Chi tiết và ba cái bẫy:
  [reference/orchestra-fix44-vs-quickfix-fix44.md](../reference/orchestra-fix44-vs-quickfix-fix44.md).
- **Ba file sẽ ship**, ở pin `386ce46e` mà `scripts/fetch-quickfix-assets.sh` đã dùng:
  `FIX44.xml` 315 399 byte, `FIXT11.xml` 11 927 byte, `FIX50SP2.xml` 1 471 310 byte — tổng
  1,8 MB, nén còn khoảng 196 KB, tức ~2 % giới hạn 10 MB của crates.io.
- **Giấy phép QuickFIX** (bản ở pin giống hệt `master`): năm điều kiện. Phân phối source phải giữ
  dòng bản quyền, danh sách điều kiện và phần miễn trừ (1); phân phối binary phải in lại chúng
  trong tài liệu kèm theo (2); tài liệu cho người dùng cuối phải có câu *"This product includes
  software developed by quickfixengine.org (http://www.quickfixengine.org/)."* — hoặc câu đó
  nằm trong chính phần mềm (3); không dùng tên "QuickFIX" để quảng bá (4) hay đặt tên sản phẩm
  (5).
- **Không có mã SPDX cho giấy phép QuickFIX.** crates.io đọc trường `license` bằng crate `spdx`,
  crate này chấp nhận `LicenseRef-…` — đọc từ mã nguồn, **chưa chứng minh bằng một lần publish**.
- **Người khác đã làm vậy**: QuickFIX/J và QuickFIX/Go đều ship các file XML này; crate
  `quickfix-msg44` trên crates.io ship `FIX44.xml` với `license = "MIT OR Apache-1.1"`; `fefix`
  0.7.0 ship chúng mà **không** kèm giấy phép QuickFIX.

## Cách làm

**Hàng 1 — đo, không xây.**

- `scripts/fetch-orchestra-assets.sh` (mới): tải `OrchestraFIX44.xml` ở đúng commit ghim, vào
  `vendor/orchestra/`, kiểm sha256, sai là dừng và nói rõ vì sao. Cùng khuôn với
  `scripts/fetch-quickfix-assets.sh`.
- `scripts/dict-diff.py` (mới, Python 3, chỉ thư viện chuẩn): đọc hai file, trải phẳng cả hai
  theo đúng cách `build.rs` làm (mở component, lồng group, tách header / trailer), so chín mặt
  (bảng ở ADR-0101 quyết định 1: T tag, Y kiểu, M loại message, P cặp message–tag, R cặp bắt
  buộc, E enum, H header/trailer, L DATA → length, G group: có hay không, delimiter, **thứ tự
  thành viên trùng khít**). Mỗi chỗ lệch là một dòng, gọi tên bằng nội dung. Chương trình tự xếp
  mỗi dòng vào loại *gate-visible* hay *quiet* và in ra dòng kết luận `OUTCOME: A|B|C` theo luật.
  Ghi ra `target/dict-diff/report.md` và `report.json`.
- Chương trình phải **tự chứng minh** trước khi được tin: so QuickFIX với chính nó phải ra 0
  dòng lệch và đúng 912 / 93 / 12 524 / 1 708 / 731 / 30 header / 16 DATA; một bản Orchestra bị
  sửa cố ý (đổi chỗ hai thành viên cạnh nhau trong một group, bỏ một giá trị enum) phải ra
  **đúng hai dòng**, ở G và E.
- Với outcome B, mỗi dòng gate-visible cần kiến trúc sư tra văn bản FIX 4.4 Errata 20030618 và
  ghi trang — đó là việc của kiến trúc sư, không của chương trình.

**Luật quyết định** (ADR-0101 quyết định 3, nhắc lại cho dễ đọc):

| Kết quả | Khi nào | Ship gì |
|---|---|---|
| **A** | 0 dòng gate-visible **và** ≤ 25 dòng quiet | file Orchestra nguyên vẹn; mỗi dòng quiet thành một mục miễn trừ có tên trong test trọng tài |
| **B** | tổng ≤ 150 dòng, **mọi** dòng gate-visible đều được văn bản FIX 4.4 Errata phân xử, và số dòng văn bản đứng về phía QuickFIX ≤ 25 | Orchestra + một bảng sửa (overlay) đúng những dòng đó, mỗi dòng ghi trang văn bản |
| **C** | mọi trường hợp còn lại, kể cả 59 / 59 đỏ ở hàng 2 | quay về phương án (b): bảng sinh từ QuickFIX + `NOTICE` — **dừng và hỏi anh** trước khi xây |

*Gate-visible* = mọi dòng ở M, H, L, G; và dòng ở T, Y, P, R, E nếu tag hoặc loại message của nó
xuất hiện trong 59 file `.def`, hoặc là một trong bảy message session `0 1 2 3 4 5 A`.

Một dòng overlay **không được** có lý do "vì QuickFIX nói vậy" — đó là dữ liệu lấy từ QuickFIX,
sẽ kéo theo `NOTICE` (ADR-0001 quyết định 5). Chỉ văn bản đặc tả FIX mới được làm lý do.

**Kết quả hàng 1:** C. Hàng 2 của bản đầu (ship Orchestra, overlay, `fix50sp2` do người dùng tự
đưa XML) **không làm**. Thay bằng dưới đây.

**Hàng 2 — nhánh C: ship XML của QuickFIX kèm `NOTICE`** (ADR-0104).

- **Ba file** `FIX44.xml`, `FIXT11.xml`, `FIX50SP2.xml` chép **nguyên từng byte** từ
  `vendor/quickfix/spec/` ở pin `386ce46e` vào `crates/dict/spec/`. `.gitattributes` đánh dấu
  `-text` để git không đổi đuôi dòng. Không file QuickFIX nào khác vào repo.
- **`scripts/check-dict-spec-pin.sh`** (mới) kiểm: sha256 của ba file đúng giá trị ghi trong
  script; pin trong script trùng `PINNED_SHA` của `fetch-quickfix-assets.sh`; hai bản `NOTICE`
  giống hệt nhau; và khi có `vendor/` thì `cmp` từng file với `vendor/quickfix/spec/`.
- **`NOTICE`** ở gốc repo và `crates/dict/NOTICE` (chỉ file trong thư mục crate mới vào gói
  `.crate`), cùng một nội dung — văn bản ở ADR-0104 quyết định 4: ba file nào, lấy từ commit
  nào, câu ghi công bắt buộc, "fixbolt không phải QuickFIX", rồi nguyên văn giấy phép QuickFIX
  ở pin.
- **`build.rs`** đổi ba đường dẫn mặc định sang `spec/…` (tính từ thư mục crate). Không mạng,
  không toolchain ngoài. Ba biến `NANOFIX_*_XML` vẫn ghi đè được. Phần sinh bảng **không đổi**.
- **`crates/dict/Cargo.toml`**: `license = "(MIT OR Apache-2.0) AND LicenseRef-QuickFIX-1.0"`.
  Crate khác giữ `MIT OR Apache-2.0` (không chứa dữ liệu QuickFIX).
- **API**: `fixbolt_dict::NOTICE` (`include_str!("../NOTICE")`), re-export `fixbolt::NOTICE` — để
  ứng dụng in được câu ghi công "trong chính phần mềm" (điều kiện 3).
- **Trọng tài bây giờ là gì.** XML ship và XML trong `vendor/` giờ là cùng một file, nên "so với
  XML của QuickFIX" thành so một file với chính nó. Còn lại ba lớp:
  (i) script pin — byte ship ra đúng là byte của QuickFIX;
  (ii) ở commit chuyển nguồn: hash của `fix44.rs` và `fixt11_fix50sp2.rs` sinh ra **trùng**
  commit cha — bảng không đổi;
  (iii) các test so bảng với **C++ do QuickFIX tự sinh** (`interop_quickfix_fields.rs`,
  `interop_quickfix_messages.rs`, `interop_quickfix_order.rs`, `fixt_order.rs`, nửa `FixValues.h`
  của `enums.rs`) — chương trình khác, vẫn đọc `vendor/quickfix/src/C++`. Chỗ nào các test đọc
  XML thì chuyển sang đọc `crates/dict/spec/`, để test kiểm đúng thứ được ship. Thiếu `vendor/`
  vẫn là đỏ, không bỏ qua. CI chứng minh các test này thật sự chạy bằng
  `check-feature-gated-tests-ran.sh fixbolt-dict - LOG`.
- **CI**: một job **không có `vendor/`** build `fixbolt-dict` (có và không `fix50sp2`),
  `fixbolt --no-default-features`, `fixbolt-engine --features fix50sp2`, và chạy script pin.
- `scripts/dict-diff.py` và `fetch-orchestra-assets.sh` ở lại trong repo như công cụ đã tạo ra
  kết quả của ADR-0101, **không** là gate.

## Bất biến bị đụng tới

- **3 (59 / 59)**: bảng sinh ra không đổi (hash chứng minh), nhưng vẫn chạy lại 59 / 59 trong
  process và qua socket, FIXT theo `docs/CONFORMANCE.md` §9 — vì đây là commit đổi nguồn của bảng
  mà session dùng.
- **5 (thứ tự field từ bảng sinh)**: generator không đổi; `interop_quickfix_order.rs` và
  `fixt_order.rs` vẫn so với C++ của QuickFIX.
- **6 (build.rs không gọi toolchain ngoài)**: `build.rs` chỉ đọc file trong crate. Job không
  `vendor/` là bằng chứng. `fix50sp2` vẫn gate `mod` như cũ.
- **7 (không panic trong crate thư viện)**: `NOTICE` là một `const &str`; `build.rs` giữ lối
  `die()`.
- **9 (không copy QuickFIX)**: **bị đổi có chủ ý** — ba file dữ liệu vào repo, kèm `NOTICE`
  (ADR-0001 quyết định 5, ADR-0104). Không dòng source QuickFIX nào vào. Câu chữ của bất biến 9
  phải sửa — xem *Câu thay trong `CLAUDE.md`*.
- **1, 2, 4, 8, 10**: không đụng — bảng sinh ở build time, không đổi; không hot path nào đổi.
- **§6 Dependencies**: không thêm dependency.

## Chia việc

Hàng 1 là một pull request (đã build 1a, 1b; 1c viết trong *Sửa 1*). Hàng 2 là pull request
thứ hai. Không bước nào tự commit — manager chạy lại gate và commit. Cuối PR 2: một senior review
(opus), đọc riêng phần nghĩa vụ giấy phép và đi lại danh sách §2.

| Bước | Kết quả | Người làm | File được sửa / **không** được sửa | Gate — xong khi | Test đỏ trước | Phụ thuộc |
|---|---|---|---|---|---|---|
| 1a | **Xong — `4eaeb53`.** Script tải Orchestra ghim commit + sha256 | developer (sonnet) | Sửa: `scripts/fetch-orchestra-assets.sh` (mới). **Không**: `crates/`, `.gitignore`, `.github/`, `scripts/fetch-quickfix-assets.sh` | `shellcheck -S info scripts/fetch-orchestra-assets.sh` sạch; chạy hai lần liền đều exit 0; `sha256sum vendor/orchestra/OrchestraFIX44.xml` đúng giá trị ghim; `git status --short` không có gì dưới `vendor/` | sửa tạm sha256 mong đợi → script exit khác 0, in câu `sha256 mismatch` (viết câu này ra trước khi chạy), rồi trả lại | plan duyệt |
| 1b | **Xong — `4eaeb53`, kết quả C.** Chương trình so chín mặt + tự kiểm + kết luận theo luật | **senior developer (opus)** — logic trải phẳng phải khớp `build.rs`, sai là quyết định sai | Sửa: `scripts/dict-diff.py` (mới). **Không**: `crates/` (kể cả `build.rs`), `docs/` | `python3 scripts/dict-diff.py --self-check` in 0 dòng lệch và đúng 912 / 93 / 12 524 / 1 708 / 731 / 30 / 16; `--mutation-check` in đúng 2 dòng (G, E); `python3 scripts/dict-diff.py` ra `target/dict-diff/report.{md,json}` và một dòng `OUTCOME:` | `--self-check` chạy trước khi viết phần trải phẳng group phải đỏ vì thiếu 731; ghi output đỏ | 1a |
| 1c | **Xong — *Sửa 1*.** Ghi kết quả vào ADR-0101 *Result*; bảng lệch vào `docs/reference/orchestra-fix44-vs-quickfix-fix44.md`; với B, tra trang văn bản FIX 4.4 Errata cho từng dòng gate-visible | architect (opus) | Sửa: ADR-0101 mục *Result* (chỉ mục đó), `docs/reference/orchestra-fix44-vs-quickfix-fix44.md` (mới). **Không**: `crates/`, `scripts/`, mọi mục khác của ADR | `python3 scripts/check-links.py` sạch; outcome ghi trong ADR trùng dòng `OUTCOME:` của 1b | — | 1b |
| 2a | Ba file XML vào `crates/dict/spec/`, hai bản `NOTICE`, script pin, bước CI | developer (sonnet) | Sửa: `crates/dict/spec/FIX44.xml`, `FIXT11.xml`, `FIX50SP2.xml` (chép từ `vendor/quickfix/spec/`), `crates/dict/NOTICE`, `NOTICE`, `.gitattributes`, `scripts/check-dict-spec-pin.sh` (mới), `scripts/fetch-quickfix-assets.sh` (chỉ comment đầu file, dòng 4–6: "NEVER committed" → trừ ba file này), `.github/workflows/ci.yml` (một bước chạy script pin trong job đang chạy các `scripts/check-*.sh`). **Không**: `build.rs`, mọi `Cargo.toml`, `crates/*/src`, `crates/*/tests` | `scripts/check-dict-spec-pin.sh` exit 0 khi có `vendor/`, và exit 0 với `FIXBOLT_VENDOR=/nonexistent` (in "vendor absent: sha256 only"); `cmp NOTICE crates/dict/NOTICE` im lặng; `shellcheck -S info` sạch; giấy phép trong `NOTICE` trùng từng dòng với `LICENSE` của QuickFIX ở pin | chạy script trước khi chép file → đỏ `missing crates/dict/spec/FIX44.xml`; lật một byte trong bản chép tạm → đỏ `sha256 mismatch`; sửa một bản `NOTICE` → đỏ `NOTICE copies differ` (viết ba câu này ra trước khi chạy), rồi trả lại | plan duyệt lại |
| 2b | `build.rs` đọc `spec/`; `license`; test đọc XML ship | developer (sonnet) — bảng không đổi, hash là bằng chứng; senior review ở cuối PR | Sửa: `crates/dict/build.rs` (ba hằng `DEFAULT`, comment của chúng, câu báo lỗi thiếu file, doc đầu file), `crates/dict/Cargo.toml` (`license`, `description`), `crates/dict/src/lib.rs` (chỉ rustdoc dòng 1 và 31), `crates/dict/tests/common/mod.rs` và `crates/dict/tests/enums.rs` (chỗ `read("spec/…")` đọc XML → đọc `crates/dict/spec/`; chỗ đọc `src/C++` giữ ở `vendor/`). **Không**: logic sinh bảng trong `build.rs`, `crates/session/`, `crates/codec/`, `crates/engine/`, file `.def` | hash `fix44.rs` và `fixt11_fix50sp2.rs` trong `out_dir` (lấy từ `cargo build -p fixbolt-dict --features fix50sp2 --message-format=json`, dòng `build-script-executed`) **trùng** commit cha; `cargo test -p fixbolt-dict`; `cargo test -p fixbolt-dict --features fix50sp2`; `cargo test -p fixbolt-session --test score` 59 / 59; `cargo test -p fixbolt-engine --test wire` 59 / 59; `cargo test -p fixbolt-session --features fix50sp2 --test score_fixt` và `cargo test -p fixbolt-engine --features fix50sp2 --test wire_fixt` đúng số `CONFORMANCE.md` §9; `cargo test --all`; `cargo test --no-default-features`; `cargo clippy --all-targets -- -D warnings` | trên một `git worktree` của nhánh **không** fetch `vendor/`: `cargo build -p fixbolt-dict` đỏ trước khi sửa (trích câu `die` gọi tên script fetch), xanh sau khi sửa | 2a |
| 2c | `fixbolt_dict::NOTICE`, `fixbolt::NOTICE` | developer (sonnet) | Sửa: `crates/dict/src/lib.rs` (một `pub const` + rustdoc), `crates/library/src/lib.rs` (một `pub use` + rustdoc), `crates/dict/tests/notice.rs` (mới). **Không**: `build.rs`, crate khác | `cargo test -p fixbolt-dict --test notice`: `NOTICE` chứa nguyên văn câu ghi công của điều kiện 3, chứa câu của điều kiện 5, chứa pin `386ce46e917ae494ab6e90b1be90fd421cdbe3f9`; `cargo doc -p fixbolt --no-deps` không cảnh báo; `cargo test --all`; clippy | `notice.rs` viết trước khi có `const` → không biên dịch được, trích lỗi; rồi xoá câu ghi công khỏi một bản tạm của `NOTICE` → test đỏ đúng assertion câu ghi công | 2a |
| 2d | Job CI không `vendor/`; chứng minh trọng tài đã chạy | developer (sonnet) | Sửa: `.github/workflows/ci.yml`, `scripts/check-feature-gated-tests-ran.sh` (nhận `-` = feature mặc định). **Không**: `crates/` | trên PR, job mới xanh: `test ! -e vendor`, `cargo build -p fixbolt-dict`, `cargo build -p fixbolt-dict --features fix50sp2`, `cargo build -p fixbolt --no-default-features`, `cargo build -p fixbolt-engine --features fix50sp2`, `scripts/check-dict-spec-pin.sh` đều exit 0; ở job có `vendor/`: `check-feature-gated-tests-ran.sh fixbolt-dict - LOG` và `… fixbolt-dict fix50sp2 LOG` exit 0; `shellcheck` sạch; manager ghi run id | chạy `cargo test -p fixbolt-dict --tests -- --skip the_only_group_quickfix_has_no_message_for_is_the_header_one` vào LOG → script đỏ ở R1 (viết câu FAIL trước: `FAIL R1 — the_only_group_quickfix_has_no_message_for_is_the_header_one is in the fixbolt-dict (default features) build (listed by crates/dict/tests/interop_quickfix_order.rs, 1×) and the log records it running 0×`). **Sửa sau senior review**: `--skip interop` lọc theo tên hàm test, không theo tên file/binary — không có hàm nào tên chứa "interop", nên câu lệnh cũ không bỏ qua test nào và không bao giờ đỏ; tên hàm thật ở trên đã đỏ đúng chỗ, trích nguyên văn ở trên | 2b |
| 2e | Tài liệu theo `CLAUDE.md` §4 | developer (sonnet) | Sửa: `README.md` (mục giấy phép nhắc `NOTICE`), `docs/GUIDE.md` (ai phát hành binary có fixbolt phải làm điều kiện 2 và 3; dùng `fixbolt::NOTICE`), `docs/CONFIGURATION.md` (mặc định của ba biến `NANOFIX_*_XML`), `docs/internals/dict.md`, `docs/DESIGN.md` §3 (nguồn của `dict`) và §4 D3 (một câu về nguồn), `docs/CONFORMANCE.md` §2 (bảng sinh từ `crates/dict/spec/` ở pin), `CHANGELOG.md` (`NOTICE`, `license`, `fixbolt::NOTICE`). **Không**: `CLAUDE.md` (manager sửa sau khi anh xem), `STATUS.md` (manager), `crates/` | `python3 scripts/check-links.py` sạch; `scripts/check-adr-numbers.sh` sạch; không câu nào dùng "QuickFIX" để quảng bá (điều kiện 4) — manager đọc lại | — | 2b, 2c, 2d |

## Cách kiểm chứng

- **Hàng 1** (đã chạy): `--self-check` 0 dòng và bảy con số; `--mutation-check` đúng 2 dòng; lần
  chạy thật ra bảng và `OUTCOME: C` — trích trong ADR-0101 *Result*.
- **Hàng 2**: hash bảng sinh trùng commit cha (2b) — đây là bằng chứng chính rằng sản phẩm không
  đổi; 59 / 59 hai đường và FIXT đúng §9; job không `vendor/` xanh trên CI, nêu run id; script pin
  đỏ khi lật một byte; script R1–R3 đỏ khi bỏ một test.
- Dữ liệu thật: đúng ba file của QuickFIX ở pin, và 59 file `.def` gốc.
- Đóng mỗi PR: một run CI xanh, nêu id, cho đúng commit đóng (`CLAUDE.md` §9).

## Tài liệu phải cập nhật

- [x] ADR-0101 mục *Result* (1c, *Sửa 1*)
- [x] `docs/reference/orchestra-fix44-vs-quickfix-fix44.md` — bảng lệch và ba bẫy (1c)
- [x] ADR-0104 (Proposed) và dòng trạng thái của ADR-0001 (*Sửa 1*)
- [ ] `README.md`, `docs/GUIDE.md`, `docs/CONFIGURATION.md`, `docs/internals/dict.md`,
      `docs/DESIGN.md` §3 và §4 D3, `docs/CONFORMANCE.md` §2, `CHANGELOG.md` (2e)
- [ ] `CLAUDE.md` §2 mục 9, bảng *Machine checks*, §8 — **manager** áp dụng câu thay dưới đây,
      sau khi anh đã xem
- [ ] `STATUS.md` — manager, khi mỗi PR đóng

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| File ship bị đổi một byte (editor, git đổi đuôi dòng) → không còn là file của QuickFIX ở pin, `NOTICE` nói sai | `scripts/check-dict-spec-pin.sh` trong CI; `.gitattributes` `-text` |
| Nâng pin QuickFIX trong `fetch-quickfix-assets.sh` mà quên ba file ship (hoặc ngược lại) → test so với oracle khác thứ đang ship | script pin so pin của nó với `PINNED_SHA`, và `cmp` với `vendor/` khi có |
| Hai bản `NOTICE` trôi khác nhau | script pin `cmp` hai bản |
| `NOTICE` chép giấy phép từ `master` thay vì từ pin | gate 2a: giấy phép trong `NOTICE` trùng `LICENSE` ở pin |
| Test vẫn đọc XML trong `vendor/` → kiểm một file khác thứ đang ship | 2b chuyển các chỗ đọc XML sang `crates/dict/spec/`; script pin chứng minh hai bên trùng |
| Crate build xanh trong repo vì `vendor/` có sẵn, hỏng trên crates.io | job không `vendor/` (2d); test đỏ trước của 2b chạy trên worktree không `vendor/` |
| Đổi đường dẫn mà bảng sinh ra đổi theo (vd. đọc nhầm file) | hash `out_dir` trùng commit cha (2b) |
| Trọng tài thiếu mà vẫn xanh | `common/mod.rs` biến thiếu file thành đỏ; `check-feature-gated-tests-ran.sh fixbolt-dict -` (R1–R3) |
| crates.io từ chối `LicenseRef-QuickFIX-1.0` lúc publish thật | chưa có test nào chạm được — ghi ở *Rủi ro*; phương án dự phòng `license-file = "NOTICE"` |
| Chữ "QuickFIX" dùng như lời quảng bá trong README / mô tả crate (điều kiện 4) | kiểm tay ở 2e và ở senior review; không có máy nào kiểm được |
| Người lạ phát hành binary mà không biết mình mang bảng từ QuickFIX | `docs/GUIDE.md` (2e); `fixbolt::NOTICE` (2c) |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| crates.io không nhận `LicenseRef-QuickFIX-1.0` | Thấp–trung bình | lần publish đầu sẽ biết; dự phòng `license-file = "NOTICE"` (mất phần máy đọc được) |
| Điều khoản ghi công đi vào mọi binary người dùng phát hành | Chắc chắn, đã chấp nhận | anh đã chọn; `GUIDE.md` và `fixbolt::NOTICE` làm cho việc tuân thủ rẻ |
| Lỗi từ điển của QuickFIX (QFJ-757, ba group thiếu thành viên, tên `HaltReasonChar`) giờ là hành vi của fixbolt | Thấp | đã ghi trong trang reference; người cần đúng đặc tả dùng biến `NANOFIX_FIX44_XML` |
| Bảng sinh có phải "sản phẩm dẫn xuất" hay không là cách mình đọc, chưa ai phán | Thấp | đọc theo hướng thận trọng: coi là dẫn xuất |
| Nâng pin QuickFIX từ nay là thay đổi sản phẩm | Thấp | script pin buộc làm có chủ ý; cần đủ gate như một thay đổi codec |

## Ngoài phạm vi

- Bỏ `publish = false`, metadata crate, `cargo publish --dry-run`, `cargo package --list` — hàng 6
  của phase 3.
- Ship Orchestra, overlay, bộ đọc Orchestra — bỏ theo kết quả C.
- Sửa lỗi từ điển của QuickFIX trong file ship — file phải nguyên từng byte.
- Ship file QuickFIX nào khác ba file trên (`.def`, C++, bản FIX khác).
- Sửa `STATUS.md`, `PRD.md`, `CLAUDE.md` (câu thay dưới đây do manager áp dụng).

## Câu thay trong `CLAUDE.md`

Manager áp dụng **sau khi anh đã xem**, trong cùng commit với 2a (commit đưa file QuickFIX vào
repo) — và nói rõ ra là đã đổi luật nào.

**§2 mục 9** — hiện tại:

> 9. **No QuickFIX source is copied.** Its XML and `.def` files are data and a test oracle, fetched
>    into gitignored `vendor/`. If that ever changes, `NOTICE` becomes mandatory. (ADR-0001)

thay bằng:

> 9. **No QuickFIX source is copied, and exactly three QuickFIX files ship.**
>    `crates/dict/spec/FIX44.xml`, `FIXT11.xml` and `FIX50SP2.xml` are committed byte-identical to
>    the pin in `scripts/fetch-quickfix-assets.sh`, under `NOTICE`; the `.def` corpus, the
>    generated C++ and all source stay a test oracle in gitignored `vendor/`. (ADR-0001, ADR-0104)

**§2 bảng *Machine checks*** — thêm một dòng:

> | 9 | `scripts/check-dict-spec-pin.sh`: the three shipped files match their pinned sha256, the pin matches `fetch-quickfix-assets.sh`'s `PINNED_SHA`, and the two `NOTICE` copies are identical | it cannot see a QuickFIX file committed under another name or path — `git add` is still the control |

**§8** — hiện tại:

> - `vendor/` is gitignored. **Never commit its contents** — that pulls QuickFIX's attribution
>   clause into this repository.

thay bằng:

> - `vendor/` is gitignored. **Never commit its contents.** The only QuickFIX files in the tree are
>   the three under `crates/dict/spec/`, held to the pinned bytes by
>   `scripts/check-dict-spec-pin.sh`; committing any other needs a new ADR first (ADR-0104).

## Điểm manager cần duyệt

1. **ADR-0104** (Proposed): ship **ba** file, không chỉ `FIX44.xml` — `NOTICE` đằng nào cũng phải
   trả, và ba file chỉ tốn ~196 KB nén; đổi lại `fix50sp2` build được từ crates.io.
2. **`license = "(MIT OR Apache-2.0) AND LicenseRef-QuickFIX-1.0"`** chỉ cho `fixbolt-dict` —
   chưa chứng minh trên crates.io.
3. **API mới `fixbolt::NOTICE`** — phần công khai, vào `CHANGELOG`.
4. **Ba câu thay trong `CLAUDE.md`** ở trên — anh xem trước khi áp dụng.
5. **Thiếu trọng tài là đỏ, không bỏ qua** — giữ từ bản đầu.

## Nhật ký giao hàng

*(Chưa có mục đóng phase. Hàng 1: 1a và 1b build ở commit `4eaeb53`, kết quả C; 1c viết trong
*Sửa 1*.)*

**Hàng 2 (2a–2e): build ở commit `66d6990`** (PR #99). Ba file XML QuickFIX chép nguyên byte vào
`crates/dict/spec/`, hai bản `NOTICE` giống hệt, `scripts/check-dict-spec-pin.sh` mới, `build.rs`
đọc `spec/` thay vì `vendor/`, `license` của `fixbolt-dict` đổi thành
`(MIT OR Apache-2.0) AND LicenseRef-QuickFIX-1.0`, `fixbolt_dict::NOTICE` + `fixbolt::NOTICE`,
job CI `dict-no-vendor`, `scripts/check-feature-gated-tests-ran.sh` nhận `FEATURE = -`, tài liệu
sửa theo §4. Gate đã chạy: `cargo test --all` và `cargo test --no-default-features` đều 129
`test result: ok`, 0 `FAILED`; 59/59 cả hai đường (`score`, `wire`); FIXT `score_fixt` /
`wire_fixt` đúng số `CONFORMANCE.md` §9 (179/180, 60/60); hash `fix44.rs` và
`fixt11_fix50sp2.rs` trùng commit cha; build không `vendor/` (rsync ra ngoài cây, xoá sau) xanh
cho `fixbolt-dict`, `--features fix50sp2`, `fixbolt --no-default-features`,
`fixbolt-engine --features fix50sp2`; `cargo clippy --all-targets -- -D warnings` sạch;
`cargo fmt --check` sạch; `check-no-optional-deps.sh`, `check-links.py`, `check-adr-numbers.sh`
sạch.

**Senior review sau `66d6990`**: không có điểm chặn merge; manager xác nhận tay các mục 2, 4, 5
bằng `grep`. Sửa trong cùng worktree `fb-p3r1`, chưa commit (manager gộp ở đợt kế tiếp):
- Hàng 2d ở trên: câu lệnh đảo ngược cũ `--skip interop` không lọc được test nào — `--skip` khớp
  theo **tên hàm test**, không theo tên file/binary, và không hàm nào ở đây tên chứa "interop".
  Thay bằng tên hàm thật `the_only_group_quickfix_has_no_message_for_is_the_header_one`
  (`crates/dict/tests/interop_quickfix_order.rs`); đã chạy lại và trích đúng câu `FAIL R1` ở ô
  trên.
- `scripts/check-dict-spec-pin.sh` từng trỏ tới một script đảo ngược chưa hề tồn tại
  (`check-dict-spec-pin-reversal.sh`) — script đó nay được viết thật: bốn nhánh (thiếu file, lật
  một byte, hai bản `NOTICE` lệch, một file lạ trong `spec/`), mỗi nhánh phục hồi bằng `trap` kể
  cả khi đỏ, chạy trong job CI `lint-config` cạnh script chính.
- `scripts/check-dict-spec-pin.sh` thêm một phép kiểm: `crates/dict/spec/` phải có **đúng ba
  file**, không hơn — một file lạ (vd. `FIX42.xml`) giờ là đỏ, khớp câu ở đầu script và
  `CLAUDE.md` §2 mục 9.
- Sửa câu chữ (không đổi hành vi): `crates/dict/src/lib.rs` dòng 4–5 (XML giờ được commit dưới
  `NOTICE`, không còn "never copied"); `README.md` dòng 64 (script fetch cần cho test và trọng
  tài, không cần để **build**); `docs/GUIDE.md` gần dòng 1719 (in `NOTICE` lúc chạy thoả điều
  kiện 3; điều kiện 2 cần văn bản đi kèm tài liệu/vật liệu phát hành cùng binary — nói cả hai,
  đúng như ADR-0104 quyết định 5); `.github/workflows/ci.yml` gần dòng 82 (phép so byte với
  `vendor/` chạy ở job `gates`, không phải `dict-no-vendor`) và dòng 463 (thêm `--locked`);
  `docs/decisions/ADR-0001-*.md` dòng trạng thái (`Proposed …` → `Accepted 2026-09-23`, chỉ dòng
  trạng thái).
