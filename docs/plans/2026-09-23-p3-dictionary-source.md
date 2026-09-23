# Phase 3, bước 1–2: từ điển cho crate đã publish — đo Orchestra trước, rồi mới đổi build

> **Loại:** Plan · **Ngày:** 2026-09-23 · **Trạng thái:** Đã duyệt (manager, 2026-09-23, theo mandate thường trực của owner)
> **Phạm vi:** phase 3, hàng 1 và hàng 2 của *Chia việc* trong
> [2026-09-23-phase-3-scope.md](2026-09-23-phase-3-scope.md). Quyết định và lý lẽ nằm ở
> [ADR-0101](../decisions/ADR-0101-the-shipped-dictionary-is-chosen-by-a-rule-written-before-the-orchestra-diff-is-measured.md).

## Bối cảnh

Hôm nay `crates/dict/build.rs` sinh bảng từ điển FIX 4.4 từ file XML của QuickFIX nằm trong
`vendor/` — thư mục bị gitignore. Ai tải `fixbolt-dict` từ crates.io sẽ không có `vendor/`, nên
build hỏng, kéo theo mọi crate phía trên. Không sửa chỗ này thì không publish được gì.

Anh đã chọn (ADR-0097, câu Q2): dùng file `OrchestraFIX44.xml` của FIX Trading Community (giấy
phép Apache-2.0) làm nguồn kèm theo crate, giữ XML của QuickFIX làm **trọng tài** — nhưng **chỉ
chốt sau khi đo** hai file lệch nhau bao nhiêu.

Plan này làm đúng thứ tự đó:

1. **Đo trước** (hàng 1): một chương trình nhỏ đọc cả hai file, so trên chín mặt, in ra bảng
   chỗ lệch. Không đụng `build.rs`, không đụng crate nào. File Orchestra chỉ được tải vào
   `vendor/`, chưa commit.
2. **Luật quyết định viết sẵn từ bây giờ** (ADR-0101 quyết định 3), trước khi có số — để không
   ai chỉnh luật cho vừa kết quả.
3. **Rồi mới xây** (hàng 2), và chỉ xây nếu luật ra kết quả A hoặc B. Ra C thì dừng, quay lại
   hỏi anh.

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

**Hàng 2 — xây, chỉ khi A hoặc B.**

- File đi kèm crate: `crates/dict/spec/OrchestraFIX44.xml` (giữ nguyên từng byte),
  `crates/dict/spec/LICENSE-orchestrations` (bản `LICENSE` gốc), `crates/dict/spec/README.md`
  (nguồn, commit, sha256, "không sửa"). `.gitattributes` đánh dấu file XML là `-text` để git
  không đổi đuôi dòng. Một bước CI kiểm sha256.
- `crates/dict/Cargo.toml`: `license = "(MIT OR Apache-2.0) AND Apache-2.0"`.
- `crates/dict/build.rs`: trước hết tách phần *đọc từ điển* ra khỏi phần *sinh bảng* qua một
  mô hình trung gian (message → field / component / group, kèm bắt buộc hay không), chứng minh
  bằng file sinh ra **giống từng byte** trước và sau. Sau đó thêm bộ đọc Orchestra vào mô hình
  đó, và đổi nguồn FIX 4.4 mặc định sang file trong `spec/`. `NANOFIX_FIX44_XML` vẫn ghi đè
  được; phần tử gốc của file (`fixr:repository` hay `fix`) quyết định dùng bộ đọc nào. Không
  thêm dependency nào.
- Trọng tài: bốn test có sẵn giữ nguyên chỗ đọc `vendor/quickfix/`, mỗi test thêm danh sách
  miễn trừ kiểm hai chiều (miễn trừ mà không còn lệch thì đỏ). Test mới
  `crates/dict/tests/referee_quickfix_xml.rs` phủ ba mặt các test cũ chưa phủ: cặp bắt buộc,
  header / trailer, DATA → length. Cả bộ này là "test so khớp" mà tiêu chí thoát 2 của ADR-0097
  gọi tên.
- CI: một job **không có `vendor/`** chạy `cargo build -p fixbolt-dict` và
  `cargo build -p fixbolt --no-default-features`, và khẳng định `--features fix50sp2` hỏng với
  thông báo gọi tên `NANOFIX_FIXT11_XML` và `NANOFIX_FIX50SP2_XML`. Script
  `check-feature-gated-tests-ran.sh` nhận thêm `-` nghĩa là "feature mặc định", để job có
  `vendor/` chứng minh mọi test trọng tài của `fixbolt-dict` đã thật sự chạy.
- `fix50sp2` trên crates.io: **người dùng tự đưa XML** (không có Orchestra SP2). Crate `fixbolt`
  không mở feature này nên `cargo add fixbolt` không bị ảnh hưởng.

## Bất biến bị đụng tới

- **3 (59 / 59)**: đổi nguồn FIX 4.4 đổi bảng mà session dùng. Hàng 2c chạy lại 59 / 59 trong
  process và qua socket, FIXT 179 / 180 (`docs/CONFORMANCE.md` §9), không sửa một dòng nào của
  `crates/session/src`.
- **5 (thứ tự field từ bảng sinh)**: mặt G của spike và `interop_quickfix_order.rs` canh; bảng
  vẫn sinh ở build time, không call site nào tự xếp field.
- **6 (feature flag, build.rs không gọi toolchain ngoài)**: `build.rs` chỉ đọc file trong crate,
  không mạng, không toolchain; `fix50sp2` vẫn gate `mod` như cũ. Job không `vendor/` là bằng
  chứng.
- **7 (không panic trong crate thư viện)**: `build.rs` không phải `src/`, giữ lối `die()` hiện
  có; `crates/dict/src` không đổi ngoài bảng sinh.
- **9 (không copy QuickFIX)**: file QuickFIX vẫn chỉ ở `vendor/`. Overlay chỉ được lấy lý do từ
  văn bản FIX. Nếu outcome C thì bất biến này buộc `NOTICE` — và đó là lúc hỏi anh.
- **1, 2, 4, 8, 10**: không đụng. Bảng sinh ở build time; hot path không đổi. Hàng 2c vẫn chạy
  lại `benches/alloc.rs` của codec vì codec đọc bảng group.
- **§6 Dependencies**: không thêm dependency; `roxmltree` đã có.

## Chia việc

Hàng 1 là một pull request. Hàng 2 là pull request thứ hai, **chỉ mở khi ADR-0101 ra A hoặc B**.
Không bước nào được commit — manager chạy lại gate và commit.

| Bước | Kết quả | Người làm | File được sửa / **không** được sửa | Gate — xong khi | Test đỏ trước | Phụ thuộc |
|---|---|---|---|---|---|---|
| 1a | Script tải Orchestra ghim commit + sha256 | developer (sonnet) | Sửa: `scripts/fetch-orchestra-assets.sh` (mới). **Không**: `crates/`, `.gitignore`, `.github/`, `scripts/fetch-quickfix-assets.sh` | `shellcheck -S info scripts/fetch-orchestra-assets.sh` sạch; chạy hai lần liền đều exit 0; `sha256sum vendor/orchestra/OrchestraFIX44.xml` đúng giá trị ghim; `git status --short` không có gì dưới `vendor/` | sửa tạm sha256 mong đợi → script exit khác 0, in câu `sha256 mismatch` (viết câu này ra trước khi chạy), rồi trả lại | plan duyệt |
| 1b | Chương trình so chín mặt + tự kiểm + kết luận theo luật | **senior developer (opus)** — logic trải phẳng phải khớp `build.rs`, sai là quyết định sai | Sửa: `scripts/dict-diff.py` (mới). **Không**: `crates/` (kể cả `build.rs`), `docs/` | `python3 scripts/dict-diff.py --self-check` in 0 dòng lệch và đúng 912 / 93 / 12 524 / 1 708 / 731 / 30 / 16; `--mutation-check` in đúng 2 dòng (G, E); `python3 scripts/dict-diff.py` ra `target/dict-diff/report.{md,json}` và một dòng `OUTCOME:` | `--self-check` chạy trước khi viết phần trải phẳng group phải đỏ vì thiếu 731; ghi output đỏ | 1a |
| 1c | Ghi kết quả vào ADR-0101 *Result*; bảng lệch vào `docs/reference/orchestra-fix44-vs-quickfix-fix44.md`; với B, tra trang văn bản FIX 4.4 Errata cho từng dòng gate-visible | architect (opus) | Sửa: ADR-0101 mục *Result* (chỉ mục đó), `docs/reference/orchestra-fix44-vs-quickfix-fix44.md` (mới). **Không**: `crates/`, `scripts/`, mọi mục khác của ADR | `python3 scripts/check-links.py` sạch; outcome ghi trong ADR trùng dòng `OUTCOME:` của 1b | — | 1b |
| 2a | `build.rs` tách "đọc" khỏi "sinh bảng" qua mô hình trung gian, vẫn đọc QuickFIX | **senior developer (opus)** — generator mà cả session dựa vào | Sửa: `crates/dict/build.rs`. **Không**: `crates/dict/src/`, `crates/dict/tests/`, crate khác | sha256 của `fix44.rs` và (với `--features fix50sp2`) `fixt11_fix50sp2.rs` trong `OUT_DIR` **trùng từng byte** trước/sau; `cargo test -p fixbolt-dict`; `cargo test -p fixbolt-dict --features fix50sp2`; `cargo clippy -p fixbolt-dict --all-targets -- -D warnings` | là refactor: bằng chứng là hai hash trước/sau; đảo ngược: đổi thứ tự hai thành viên trong mô hình → hash khác và `interop_quickfix_order.rs` đỏ | ADR-0101 = A/B |
| 2b | File Orchestra đi kèm crate, giấy phép, kiểm sha256 | developer (sonnet) | Sửa: `crates/dict/spec/OrchestraFIX44.xml`, `crates/dict/spec/LICENSE-orchestrations`, `crates/dict/spec/README.md` (đều mới), `crates/dict/Cargo.toml` (chỉ dòng `license`), `.gitattributes`, `scripts/check-orchestra-pin.sh` (mới), `.github/workflows/ci.yml` (một bước). **Không**: `build.rs`, `publish = false`, crate khác | `scripts/check-orchestra-pin.sh` exit 0; `cmp crates/dict/spec/OrchestraFIX44.xml vendor/orchestra/OrchestraFIX44.xml` im lặng; `cargo metadata` đọc được `license` mới | sửa một byte của file trong bản tạm → script đỏ với câu viết sẵn, rồi trả lại | 2a (song song được, file khác nhau) |
| 2c | Bộ đọc Orchestra; nguồn FIX 4.4 mặc định chuyển sang `spec/`; miễn trừ có tên; overlay (nếu B); `referee_quickfix_xml.rs` | **senior developer (opus)** — đụng bảng của session, bất biến 3 và 5 | Sửa: `crates/dict/build.rs`, `crates/dict/tests/*.rs`, `crates/dict/tests/common/mod.rs`. **Không**: `crates/session/`, `crates/codec/src/`, `crates/engine/`, file `.def` | `cargo test -p fixbolt-dict`; `cargo test -p fixbolt-session --test score` 59 / 59; `cargo test -p fixbolt-engine --test wire` 59 / 59; `cargo test -p fixbolt-session --features fix50sp2 --test score_fixt` đúng con số của `CONFORMANCE.md` §9; `cargo test -p fixbolt-engine --features fix50sp2 --test wire_fixt`; `cargo test -p fixbolt-codec --test group_roundtrip`; `cargo bench -p fixbolt-codec --bench alloc`; `cargo test --all` và `cargo test --no-default-features`; clippy `-D warnings` | `referee_quickfix_xml.rs` viết trước, xanh trên bảng QuickFIX; đổi nguồn sang Orchestra trước khi thêm miễn trừ → đỏ, và **danh sách đỏ phải trùng bảng của spike** (hai chương trình độc lập khớp nhau) | 2a, 2b |
| 2d | Job CI không `vendor/`; chứng minh trọng tài đã chạy | developer (sonnet) | Sửa: `.github/workflows/ci.yml`, `scripts/check-feature-gated-tests-ran.sh` (nhận `-`). **Không**: `crates/` | job mới xanh trên PR: hai lệnh build exit 0 trên checkout không có `vendor/`, `fix50sp2` hỏng với thông báo gọi tên hai biến; `check-feature-gated-tests-ran.sh fixbolt-dict - LOG` exit 0; `shellcheck` sạch | chạy test với `--skip referee` → script đỏ ở R1, ghi câu FAIL trước khi chạy | 2c |
| 2e | Tài liệu theo `CLAUDE.md` §4 | developer (sonnet) | Sửa: `docs/DESIGN.md` §4 D3 (một đoạn về nguồn), `docs/internals/dict.md`, `docs/CONFORMANCE.md` §2, `docs/CONFIGURATION.md` (`NANOFIX_FIX44_XML` nhận hai định dạng), `docs/GUIDE.md` (`fix50sp2` phải tự đưa XML), `docs/reference/fix44-dictionary-traps.md`, `CHANGELOG.md`. **Không**: `STATUS.md` (manager viết), `crates/` | `python3 scripts/check-links.py` sạch; mọi con số trích có lệnh và commit | — | 2c, 2d |

Nếu ADR-0101 ra **C**: hàng 2 ở trên **không chạy**. Manager báo anh, và phương án (b) cần một
plan riêng.

## Cách kiểm chứng

- **Hàng 1**: ba lệnh của 1b, output trích nguyên văn — `--self-check` (0 dòng, bảy con số),
  `--mutation-check` (đúng 2 dòng), lần chạy thật (bảng + `OUTCOME:`). Con số QuickFIX phía
  spike phải trùng con số các test Rust đang assert — nếu không, lỗi ở chương trình, không ở dữ
  liệu.
- **Hàng 2**: hai hash trùng ở 2a; danh sách đỏ ở 2c trùng bảng spike; 59 / 59 hai đường và FIXT
  đúng số §9; job không `vendor/` xanh trên CI, nêu run id; script R1–R3 xanh, và đỏ khi cố ý bỏ
  một test.
- Đây là dữ liệu thật: hai file từ điển gốc và 59 file `.def` gốc — không file nào tự bịa.
- Đóng mỗi PR: một run CI xanh, nêu id, cho đúng commit đóng (`CLAUDE.md` §9).

## Tài liệu phải cập nhật

- [ ] ADR-0101 mục *Result* (1c), rồi trạng thái *Accepted* khi manager duyệt kết quả
- [ ] `docs/reference/orchestra-fix44-vs-quickfix-fix44.md` — bảng lệch (1c)
- [ ] `docs/DESIGN.md` §4 D3 — nguồn của bảng (2e)
- [ ] `docs/internals/dict.md` — hai bộ đọc, mô hình trung gian, `spec/` (2e)
- [ ] `docs/CONFORMANCE.md` §2 — con số trọng tài sau khi đổi nguồn, kèm lệnh và run id (2e)
- [ ] `docs/CONFIGURATION.md` — `NANOFIX_FIX44_XML` (2e)
- [ ] `docs/GUIDE.md` — `fix50sp2` cần XML người dùng tự đưa (2e)
- [ ] `docs/reference/fix44-dictionary-traps.md` — bẫy gặp phải (2e)
- [ ] `CHANGELOG.md` (2e)
- [ ] `STATUS.md` — manager, khi mỗi PR đóng

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Orchestra không bảo đảm thứ tự trên dây — group có thể đúng thành viên mà sai thứ tự | mặt G so trùng khít; `interop_quickfix_order.rs`; `group_roundtrip.rs` |
| Group trong Orchestra là phần tử riêng (`groupRef`), còn QuickFIX để group nằm trong component (vd. `Parties`) — trải phẳng sai là lệch khoá (message, counter) | `--self-check` phải ra 731; `--mutation-check`; ở 2c danh sách đỏ trùng bảng spike |
| Kiểu field nằm gián tiếp qua `…CodeSet` | mặt Y; `interop_quickfix_fields.rs` (`every_field_type_agrees_with_quickfix_or_is_a_named_exemption`) |
| Logon thiếu group `NoMsgTypes` (từng thấy ở bản cũ) — Logon là message session, 59 file `.def` đều đi qua nó | mặt P / G, gate-visible; 59 / 59 ở 2c |
| Component bắt buộc không làm field bên trong thành bắt buộc; Orchestra ghi `presence` trên `componentRef` / `groupRef` | mặt R; `tables.rs` `a_required_component_does_not_make_its_fields_required`; `referee_quickfix_xml.rs` |
| Group `NoHops(627)` nằm trong header — lấy thiếu thì bốn tag rơi xuống body | mặt H (30); `tables.rs` `header_group_and_its_members_are_header_fields` |
| 68 phần tử `updated="FIX.Latest"` — sửa lỗi của bản sau lọt vào file 4.4 | spike liệt kê riêng; mỗi cái thành dòng lệch có tên nếu đổi bảng |
| Thuộc tính gõ sai (`deprecated="FIIX.4.4"`) làm hỏng bộ đọc nếu bộ đọc tin vào nó | bộ đọc chỉ đọc thuộc tính nó cần; spike báo cáo; `cargo test -p fixbolt-dict` |
| Chương trình Python trải phẳng khác `build.rs` → đo sai mà tưởng đúng | `--self-check` phải trùng số các test Rust; ở 2c danh sách đỏ trùng bảng spike |
| File ship bị đổi một byte (editor, git đổi đuôi dòng) → không còn "nguyên vẹn", mất lý do không cần notice | `scripts/check-orchestra-pin.sh` trong CI; `.gitattributes` `-text` |
| File để trong `vendor/` hoặc bị gitignore → không được đóng gói | file nằm ở `crates/dict/spec/`; job không `vendor/` ở 2d |
| Trọng tài thiếu mà vẫn xanh | `common/mod.rs` biến thiếu file thành đỏ; `check-feature-gated-tests-ran.sh fixbolt-dict -` (R1–R3) |
| Một dòng overlay mượn lý do từ QuickFIX → thành dữ liệu QuickFIX, kéo `NOTICE` | mỗi dòng overlay ghi trang văn bản FIX 4.4 Errata; `build.rs` kiểm overlay hai chiều như `SP2_LENGTH_EXCEPTIONS`; senior review đọc từng dòng |
| Refactor 2a đổi bảng sinh mà không ai thấy | hash `OUT_DIR` trùng từng byte trước/sau |
| `fix50sp2` trên crates.io âm thầm hỏng | job không `vendor/` khẳng định thông báo lỗi gọi tên hai biến môi trường |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Lệch nhiều → outcome C, quay về QuickFIX + `NOTICE` | Trung bình | luật viết sẵn; C dừng lại hỏi anh, không tự xây |
| Dòng "All Rights Reserved" trong file mâu thuẫn Apache-2.0 | Thấp–trung bình | đọc như dòng bản quyền mà Apache §4(c) bắt giữ lại; nếu FIX Trading Community nói khác → C. Anh có thể muốn hỏi thẳng họ qua issue |
| Ngưỡng 25 / 150 là phán đoán | Thấp | viết trước khi đo; ADR ghi rõ là phán đoán |
| Upstream sửa file 4.4 (đã có 10 commit từ 2023) | Thấp | ghim commit + sha256; nâng cấp là việc có chủ ý: ghim lại, chạy lại spike |
| Refactor `build.rs` làm hỏng bảng mà test không bắt | Trung bình | hash trùng từng byte ở 2a; senior developer làm |
| Thời gian build tăng vì file 1,5 MB nhiều phần tài liệu | Thấp | 2c ghi thời gian `cargo build -p fixbolt-dict` trước/sau |

## Ngoài phạm vi

- Bỏ `publish = false`, metadata crate, `cargo publish --dry-run` — hàng 6 của phase 3.
- Sinh FIX 5.0 SP2 từ `OrchestraFIXLatest.xml` — nếu muốn, là một spike riêng.
- Đọc `FIX44Session.xml` hay `FIXTSession.xml` — FIX 4.4 session đã có sẵn trong
  `OrchestraFIX44.xml`.
- Dùng các thông tin phong phú hơn của Orchestra (scenario, rule, workflow) — file FIX 4.4 không
  có chúng.
- Phương án (b) hoặc (c) — chỉ khi ADR-0101 ra C, bằng plan riêng.
- Sửa `STATUS.md`, `PRD.md`.

## Điểm manager cần duyệt

1. **ADR-0101**, và luật A / B / C với ngưỡng 25 / 150 — duyệt trước khi 1b chạy.
2. **`fix50sp2` trên crates.io là "người dùng tự đưa XML"** — thu hẹp dòng bẫy *"dry-run chạy …
   từng feature công khai"* của plan phase 3: feature này được build trong CI có `vendor/`, còn ở
   job không `vendor/` thì khẳng định nó hỏng đúng cách.
3. **Thiếu trọng tài là đỏ, không bỏ qua** — khác với brief ban đầu ("skip kèm lý do"), vì
   `common/mod.rs` đã đặt luật đó và không ai chạy test của dependency lấy từ crates.io; job không
   `vendor/` chỉ build.

## Nhật ký giao hàng

*(Chưa có — plan chờ duyệt.)*
