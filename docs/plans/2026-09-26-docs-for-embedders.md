# Tài liệu cho người nhúng fixbolt, và từ điển tuỳ biến (custom tag / từ điển riêng của sàn)

> **Loại:** Plan · **Ngày:** 2026-09-26 · **Trạng thái:** Đã duyệt (2026-09-26)
> **Phạm vi:** hai việc trong một plan, theo quyết định của chủ dự án: (A) viết lại bộ tài liệu để
> fixbolt đọc như một framework FIX mà lập trình viên công ty khác nhúng vào sản phẩm của họ;
> (B) tính năng mới — tag tuỳ biến và từ điển riêng của sàn (venue dictionary). Thiết kế ở hai ADR
> đề xuất cùng plan này:
> [ADR-0206](../decisions/ADR-0206-the-documentation-is-an-mdbook-over-docs-in-place-split-by-diataxis-and-no-cited-file-moves.md)
> (kiến trúc tài liệu) và
> [ADR-0207](../decisions/ADR-0207-a-custom-dictionary-is-an-overlay-generated-in-the-users-build-into-the-users-own-type.md)
> (cơ chế từ điển tuỳ biến). Tư liệu nghiên cứu:
> [reference/prior-art-for-embedders.md](../reference/prior-art-for-embedders.md).
> **Phase 5** mới trong `PRD.md` §2, sau phase 4 và trước kernel bypass (chủ dự án quyết định
> 2026-09-26; bước 1a). Câu trả lời của chủ dự án cho Q1–Q6: mục *Quyết định của chủ dự án*.

## Bối cảnh

Chủ dự án muốn fixbolt được các công ty khác dùng: lập trình viên Rust bên ngoài dựng acceptor
hoặc initiator trên nó, và sau này có thể mua giấy phép thương mại. Hôm nay tài liệu được viết cho
chính những người xây nó. Khoảng 8 400 dòng trong `docs/`, cộng 131 ADR, 131 trang reference, 92
plan; `STATUS.md` 6 518 dòng. Người ngoài không có trang đầu nói nên đọc gì trước, không có file nói
"đừng phá những điều này" cho người đóng góp, không có `CONTRIBUTING.md`, `SECURITY.md`, không có
trang web — và vì ADR-0161 (0.1.0 là git tag, không lên crates.io) nên cũng không có docs.rs.

Việc thứ hai là tính năng mà người dùng QuickFIX mặc nhiên chờ đợi: thêm field riêng của sàn vào
từ điển. Với QuickFIX, họ sửa một bản sao file XML rồi trỏ `DataDictionary=` vào nó lúc chạy. Với
fixbolt hôm nay, đường duy nhất là thay **nguyên** file FIX 4.4 lúc build bằng biến môi trường
`NANOFIX_FIX44_XML` — áp cho cả chương trình, và kiểu vẫn mang tên `Fix44` dù không còn là FIX 4.4.

Người mới vào sau sáu tháng cần biết: plan này **không chuyển chỗ** file tài liệu nào đang được
trích dẫn, mà thêm trang mới và dựng một "cuốn sách" (mdBook) làm mục lục lên trên; và từ điển tuỳ
biến được **sinh thành code lúc build trong crate của người dùng**, không nạp lúc chạy.

## Những gì đã biết chắc

**Về tài liệu hiện có** (đếm trên cây làm việc ở `6c2192d`, 2026-09-26):

- `docs/DESIGN.md` 2 094 dòng, `GUIDE.md` 2 045, `CONFORMANCE.md` 1 131, `SESSION-BEHAVIOUR.md` 701,
  `CONFIGURATION.md` 547, `hft-playbook.md` 545, `PRD.md` 389, `best-practices-hft.md` 295,
  `GETTING-STARTED.md` 268, `TUTORIAL.md` 248, `best-practices-standard.md` 221,
  `INTRODUCTION.md` 130; `docs/internals/` 11 trang (34–134 dòng).
- `DESIGN.md` được **288** file nhắc tên; số mục của nó ("`DESIGN.md` §8", "D3") xuất hiện dạng
  chữ khoảng **776** lần. `GUIDE.md` được 143 file nhắc, `CONFORMANCE.md` 55 (có cả
  `.github/workflows/ci.yml` và `scripts/stranger-check.sh`).
- Chỉ **24** link markdown có `#anchor` trỏ vào một tài liệu cấp cao; phần lớn trích dẫn là chữ,
  không phải link — **không redirect được**. `scripts/check-links.py` không kiểm anchor (đầu file
  nói rõ).
- **95** link từ `docs/` trỏ vào `../crates/…`; lên web sẽ thành 404.
- `scripts/stranger-check.sh` chép nguyên hai khối code trong `GETTING-STARTED.md` (đánh dấu bằng
  comment `<!-- stranger-check: … -->`) vào một crate ngoài cây và chạy Logon/Logout thật. Đây là
  mẫu có sẵn để "code trong tài liệu phải biên dịch được".
- Chưa có `ARCHITECTURE.md`, `CONTRIBUTING.md`, `SECURITY.md`.

**Về mdBook** (nguồn ở trang prior-art §4): bản mới nhất v0.5.4 (2026-07-06). Chỉ chương có trong
`SUMMARY.md` mới được render — file không liệt kê thì không được đọc (issue #702, còn mở). Link
tương đối đuôi `.md` được đổi thành `.html`; `README.md` thành `index.html` (có lỗi đã báo với
README trong thư mục con, #984, #1920). Id của heading: chữ thường, khoảng trắng thành gạch nối —
**chưa biết** có trùng với cách GitHub làm cho heading có gạch dài, backtick, số mục hay không.
`[output.html.redirect]` có sẵn cho trang bị dời. `mdbook test` cần `-L` tới mọi dependency.
GitHub có workflow mẫu deploy mdBook bằng `actions/upload-pages-artifact` + `actions/deploy-pages`.

**Về giấy phép** (prior-art §3): code đã phát hành theo MIT/Apache-2.0 không thể đơn phương đổi
giấy phép — cần từng người giữ bản quyền đồng ý. DCO giữ nguyên giấy phép hiện tại cho mỗi đóng
góp (muốn đổi sau phải hỏi lại từng người); CLA cấp cho dự án quyền đủ rộng để đổi giấy phép và bán
bản thương mại, nhưng làm một số người ngại đóng góp. **Phải chọn trước khi merge đóng góp ngoài đầu
tiên.**

**Về cách các engine khác cho thêm field** (prior-art §2): QuickFIX C++ đọc XML **lúc chạy**, mỗi
session một file (`UseDataDictionary` mặc định `Y`, `DataDictionary`, `TransportDataDictionary`,
`AppDataDictionary`, `ValidateUserDefinedFields` mặc định `Y`); trang cấu hình C++ không có
`AllowUnknownMsgFields`. QuickFIX/J: XML lúc chạy **và** sinh class Java lúc build (plugin Maven
`quickfixj-codegenerator`, ví dụ thêm tag 5000). Fix8: trình biên dịch `f8c` sinh C++ lúc build —
"update the schema and recompile" — và nhiều biến thể FIX trong một chương trình. Chronicle: sinh
code, nhiều schema trong một engine. OnixS, B2BITS: lúc chạy; B2BITS đọc thẳng XML định dạng
QuickFIX. `fefix` 0.7.0: cả hai (`codegen` hoặc `Dictionary::from_quickfix_spec`). IronFix: lúc
chạy, `Dictionary::from_quickfix_xml`. **Định dạng XML của QuickFIX là thứ ai cũng nhận.**

**Về code của fixbolt** (đọc 2026-09-26):

- `crates/codec/src/dict.rs`: `trait Dictionary` — `is_header`, `data_length_tag`,
  `group_delimiter`, `group_members`, `group_order`; mọi hàm là associated function, không
  receiver, không `dyn`.
- `crates/dict/src/tables.rs`: `trait Tables: Dictionary` — chín câu hỏi session hỏi, cố ý không
  có default method (ADR-0084).
- `Session` và `Engine` đã generic theo encoding `E`, với `E::Dict: Tables` (ADR-0080 quyết định
  1: registry chọn giữa các encoding **đã biên dịch**, không bao giờ chọn từ điển lúc chạy).
- **Bất ngờ 1:** crate mặt tiền `fixbolt` thì **không** generic: `crates/library/src/app.rs:231`
  parse bằng `Fix44`, `crates/library/src/reply.rs:379-380` sắp thứ tự và mã hoá bằng `Fix44`; mọi
  cửa `serve*`/`connect_and_serve*` dựng engine với encoding mặc định `TagValue<Fix44, N>`. Tức là
  hôm nay một từ điển khác không đi tới được ứng dụng viết trên mặt tiền.
- **Bất ngờ 2:** bộ sinh bảng là `crates/dict/build.rs`, 1 595 dòng, có
  `#![allow(clippy::indexing_slicing)]` cho cả file, báo lỗi bằng `die()` = `process::exit(1)`, và
  code nó sinh ra ghi `crate::FieldType` — không `include!` được vào crate khác. Nó đã biết gộp hai
  file XML có kiểm tra khớp (`merge_fields`, `merge_components`, cho cặp FIXT 1.1 + FIX 5.0 SP2).
- `crates/sbe-gen` là tiền lệ đúng hình dạng cần: một bộ sinh mà `build.rs` của người dùng gọi,
  file `generator.rs` được nạp hai lần (module thường, và `build.rs` `#[path]` vào) — ADR-0081.
- `ValidateUserDefinedFields=N` (`DictionaryChecks::skipping_user_defined_fields`,
  `FIRST_USER_DEFINED_TAG = 5000`, `crates/session/src/lib.rs`) đã cho một tag ≥ 5000 đi qua. Cái
  **chưa** làm được: group tuỳ biến (parser cần delimiter/members từ từ điển), giá trị enum mới
  cho field chuẩn (`373=5`), message type mới (`373=11`), field sàn bắt buộc, tag tuỳ biến < 5000.
- **Bất ngờ 3:** bảng `ALLOWED` là bitset trên `0..=max_tag`. FIX 4.4 dừng ở tag 956 → 15 word mỗi
  message type, 93 message type ≈ 11 KB. Chỉ một tag tuỳ biến 20000 → 313 word ≈ 233 KB. Tĩnh,
  không cấp phát, nhưng tốn binary và cache (reference
  `a-bitset-keyed-by-tag-scales-with-the-highest-tag-not-the-field-count.md` đã đo cùng hiện tượng
  với FIX 5.0 SP2).
- `crates/engine/src/settings.rs`: `Problem` có `#[non_exhaustive]` — thêm biến thể không phá API.
  `DataDictionary`/`UseDataDictionary` trong file cấu hình hôm nay là *unknown key*.
- `scripts/bench.sh` liệt kê bench bằng tay (`TARGETS`), và một bench không có trong danh sách
  `is_invariant` là lỗi cứng.
- ADR-0104 quyết định 7: tên "QuickFIX" chỉ xuất hiện như một sự thật, "never as an endorsement, a
  compatibility badge or a comparison in marketing copy". Trang "vì sao fixbolt" vì thế là giải
  thích kỹ thuật về cơ chế của fixbolt, không phải bài so sánh (chủ dự án, Q2).
- Cargo: lỗi cũ `[env]` không làm chạy lại build script (#10358, #14350) **đã đóng** — không phải
  lý do để từ chối phương án biến môi trường.

## Cách làm

### A. Tài liệu (ADR-0206)

**Nguyên tắc giữ link: không dời file nào đang bị trích dẫn, không cắt mục nào ra khỏi file của
nó.** Chọn cách này vì 288 + 143 + 55 file và ~776 trích dẫn dạng chữ trỏ vào đường dẫn và số mục
hiện tại; trích dẫn dạng chữ không redirect được, và link checker không kiểm anchor. Tách một file
lớn thành nhiều file (kể cả có trang redirect) sẽ làm gãy âm thầm nhiều hơn hẳn so với để nguyên
đường dẫn và chỉ thêm `SUMMARY.md` làm mục lục. File lớn không bị chia; trang mới được **thêm** và
link *vào* mục cũ.

**Dựng sách:** `book.toml` ở gốc repo, `[book] src = "docs"`, thư mục build dưới `target/`. Mục lục
`docs/SUMMARY.md` theo Diátaxis (bản nháp dưới đây; phần ADR/reference/internals do script sinh):

```text
fixbolt → index.md
# Tutorials
- Getting started → GETTING-STARTED.md
- Tutorial: a working acceptor → TUTORIAL.md
# How-to guides
- Task index → how-to/index.md
- Add a custom tag → how-to/add-a-custom-tag.md                 (PR 6)
- Use a venue dictionary → how-to/use-a-venue-dictionary.md     (PR 6)
- Move a QuickFIX configuration → how-to/migrate-from-quickfix.md (PR 6)
- Embedding guide: the constraints → GUIDE.md
- Operating in standard mode → best-practices-standard.md
- Operating in hft mode → best-practices-hft.md
- Tuning a Linux host → hft-playbook.md
# Reference
- Configuration → CONFIGURATION.md
- Session behaviour → SESSION-BEHAVIOUR.md
- Conformance evidence → CONFORMANCE.md
- API → api.md
- Protocol facts, measured costs and traps → (no page)     ← sinh từ docs/reference/
# Explanation
- FIX 4.4 and the acceptor role → INTRODUCTION.md
- Why fixbolt → explanation/why-fixbolt.md
- Why Rust, and what it costs → explanation/why-rust.md
- The design in ten decisions → explanation/design-rationale.md
- Design document → DESIGN.md
- Scope and phases → PRD.md
# Contributing and project records
- Contributing → contributing.md
- Crate internals → internals/README.md            ← các trang con sinh từ docs/internals/
- Decision records → (no page)                              ← sinh từ docs/decisions/
```

**Link ra ngoài `docs/`** (tới `../crates/…`, `../scripts/…`, `STATUS.md`, `CLAUDE.md`, các file ở
gốc, và tới `docs/plans/` vì plans không vào sách) được **viết lại lúc build** bởi preprocessor
`scripts/mdbook-repo-links.py` (Python chuẩn, không dependency) thành URL GitHub ở đúng commit đang
build. Mã nguồn giữ link tương đối — luật của `check-links.py` không đổi.

**Bảng di trú** — mọi file hiện có, không file nào bị bỏ:

| File hiện tại | Làm gì | Nằm ở đâu trong sách | Sửa gì trong file |
|---|---|---|---|
| `README.md` (gốc) | Giữ đường dẫn, viết lại phần đầu | Ngoài sách; `index.md` tóm và link tới | Định vị cho người nhúng; câu headline giữ đúng ADR-0099; phần *Licence* chỉ nêu sự thật hiện tại, trung lập; bảng *Where it stands* giữ nguyên số liệu; thêm link tới sách |
| `GETTING-STARTED.md` | Giữ | Tutorials | Một dòng đầu "bạn sẽ dựng gì"; **không đụng** hai khối có marker `stranger-check` |
| `TUTORIAL.md` | Giữ | Tutorials | Đánh dấu khối code bằng marker `sample:` trỏ tới file ví dụ đã biên dịch |
| `INTRODUCTION.md` | Giữ | Explanation | §4 *Prior art in Rust* và §5 *How fixbolt is positioned*: thêm link sang `why-fixbolt.md`, không chép nội dung |
| `GUIDE.md` | Giữ nguyên, **không chia** | How-to (một chương) | Chỉ thêm một đoạn ở §3a trỏ tới how-to từ điển (PR 6) |
| `CONFIGURATION.md` | Giữ | Reference | Hàng mới cho feature `codegen`, bốn key bị từ chối, §5 overlay (PR 4–6) |
| `SESSION-BEHAVIOUR.md` | Giữ | Reference | §3: tag do từ điển tuỳ biến định nghĩa là "defined" (PR 6) |
| `CONFORMANCE.md` | Giữ | Reference | Không đổi (trừ khi số gate đổi) |
| `DESIGN.md` | Giữ, số mục bất biến | Explanation (bản sâu) | §3 hàng `dict`, `library`, crate ví dụ mới; §4 D3 thêm đoạn về từ điển người dùng (PR 4–6) |
| `PRD.md` | Giữ | Explanation | §2 phase 5 mới, bypass dời sau nó (bước 1a); §3 thêm hàng "Custom fields / venue dictionary" (PR 6) |
| `best-practices-standard.md`, `best-practices-hft.md`, `hft-playbook.md` | Giữ | How-to | Không đổi |
| `docs/internals/*` | Giữ | Contributing | `dict.md` (PR 4–5), `library.md`, `engine.md`, trang mới cho crate ví dụ (PR 6) |
| `docs/reference/*` | Giữ | Reference (mục sinh tự động) | Trang mới cho mỗi bất ngờ gặp phải |
| `docs/decisions/*` | Giữ | Contributing (mục sinh tự động) | ADR-0206/0207 → Accepted khi chủ dự án duyệt |
| `docs/plans/*` | Giữ | **Không vào sách** (tiếng Việt, nội bộ) | — |
| `STATUS.md`, `CLAUDE.md`, `CHANGELOG.md`, `RELEASING.md` | Giữ ở gốc | Ngoài sách, link ra GitHub | `CLAUDE.md` §4 hai bảng (bước 6) |

**File mới** (phần A): `book.toml`; `docs/SUMMARY.md`; `docs/index.md`; `docs/api.md` (chạy
`cargo doc --open -p fixbolt`, và danh sách kiểu vào cửa: `serve`, `Handler`, `Reply`, `Settings`,
`Table`, … — link về mã nguồn); `docs/contributing.md` (trang ngắn link ra ba file ở gốc);
`docs/how-to/index.md` (danh mục việc → mục cụ thể của `GUIDE.md`/best-practices/playbook);
`docs/explanation/why-fixbolt.md`, `why-rust.md`, `design-rationale.md`; `ARCHITECTURE.md`,
`CONTRIBUTING.md`, `SECURITY.md` ở gốc; script `scripts/gen-book-summary.py`,
`scripts/mdbook-repo-links.py`, `scripts/check-doc-samples.sh`, `scripts/check-doc-claims.sh`,
`scripts/check-cited-headings.sh`; chế độ `--rendered` cho `scripts/check-links.py`; job CI `book`
và workflow `pages.yml`.

**Nội dung ba trang explanation:**

- `design-rationale.md`: D1–D10 kể cho người ngoài. Mỗi quyết định: một câu nói nó là gì, vì sao,
  giá phải trả, link `DESIGN.md` §4 Dn và ADR gốc. Không chép số đo — link tới chỗ ghi số đo.
- `why-fixbolt.md`: **giải thích kỹ thuật vì sao fixbolt nhanh** — cơ chế nào, và nó tránh được chi
  phí nào — không phải bài so sánh (chủ dự án, Q2). Mỗi mục: một cơ chế của fixbolt, chi phí nó
  tránh, và chỗ số đo được ghi (link, không chép số — non-negotiable 10). Các cơ chế: parse tại chỗ
  thành view mượn buffer, không map sở hữu (D2, ADR-0003); không cấp phát trên đường nóng, chứng
  minh bằng bộ đếm (D9, `benches/alloc.rs`); từ điển là bảng sinh lúc build, dispatch tĩnh, không
  tra cứu cấu trúc dựng lúc chạy (D3, ADR-0080); message gửi đi là template được vá, không dựng lại
  (D9); session thuần, không socket, không đồng hồ (D1); dispatch inline trên luồng engine (D4);
  tách `standard`/`hft` và một session mỗi luồng poll trong `hft` (D8, ADR-0012). Kèm phần "cái giá"
  (build lại khi đổi phương ngữ, một core bị đốt trong `hft`) và "chưa có" từ `PRD.md` §3 (độ phủ
  phiên bản, zero track record). **Engine khác chỉ xuất hiện khi cần để giải thích**, dưới dạng sự
  thật thiết kế có nguồn từ tài liệu hoặc mã nguồn của chính họ (ví dụ "QuickFIX đọc từ điển XML lúc
  chạy, theo trang cấu hình của nó"); **không xếp hạng, không "nhanh hơn X", không con số latency
  của engine khác** — số của họ chỉ ở trang reference prior-art, ghi "their claim".
  **Vì sao trang nằm trong ADR-0104 quyết định 7:** tên QuickFIX chỉ xuất hiện như sự thật có nguồn
  (điều quyết định 7 cho phép), không có câu tán thành, huy hiệu tương thích hay so sánh hơn kém.
  Giữ bằng máy: `check-doc-claims.sh` (bước 14) đỏ khi một dòng có tên engine khác (QuickFIX,
  Fix8, Chronicle, OnixS, B2BITS, FerrumFIX, IronFix) cùng một từ so sánh (`faster`, `slower`,
  `better`, `outperform` — bỏ `than` ở bước 14: đo được 8 dòng trúng, 6 là tiếng Anh bình thường như "rather than"; ADR-0206 ghi lần sửa); và bằng tay: senior review bước 16 đọc từng câu có tên engine
  khác.
- `why-rust.md`: mức cược của micro giây (Aquilina–Budish–O'Neill, QJE 2022 — chỉ cho luận điểm
  này), cái Rust cho dự án (bằng chứng của chính repo: bộ đếm cấp phát, lint cấm `panic`, `unsafe`
  phải có bằng chứng), và nhược điểm thật (matklad *Why Not Rust*, Databento). **Cấm** các câu Jane
  Street / Jump / Tower / "98.7 %" — `check-doc-claims.sh` bắt.

**`ARCHITECTURE.md`** theo kiểu matklad: ngắn (mục tiêu dưới 200 dòng), ít đổi; bản đồ code bằng
**tên để grep** (crate, module, kiểu), không bằng link dễ mục; các khối *Architecture Invariant*,
kể cả những thứ **cố ý không có**: không async runtime; không cấp phát trên đường nóng; session
không socket, không đồng hồ; không chọn từ điển lúc chạy; codec không dependency; không log trên
đường nóng; không mã nguồn QuickFIX; mặt tiền cố ý không re-export `Engine`/`Dispatch`/`Transport`.
Mỗi invariant ghi mục `CLAUDE.md` §2 tương ứng và cái máy nào kiểm nó — **link tới luật, không
chép luật** (một luật, một chỗ).

**`CONTRIBUTING.md`** — viết cho **lập trình viên trong nhóm dự án**, không cho công chúng (chủ
dự án, Q1): build, `scripts/fetch-quickfix-assets.sh` và `fetch-sbe-assets.sh`, lệnh gate
(link tới bảng `CLAUDE.md` §7 và tên job CI, không chép), quy trình đề xuất thay đổi: issue → plan
(`docs/plans/_template.md`) → ADR nếu là quyết định → PR nháp sớm → gate xanh. **Luật ghi trong
file:** đóng góp từ ngoài nhóm **chưa được nhận**, cho tới khi một ADR về giấy phép quyết định CLA
hay DCO. Câu hỏi CLA/DCO là việc **hoãn**, gắn với mốc "trước khi nhận đóng góp ngoài đầu tiên",
không chặn plan này.

**`SECURITY.md`**: báo lỗ hổng **chỉ** qua *private vulnerability reporting* của GitHub (chủ dự án
bật, Q4); không có email liên hệ. Phiên bản được hỗ trợ (tag mới nhất), phạm vi, không có bounty.

### B. Từ điển tuỳ biến (ADR-0207)

1. **Một bộ sinh, ba nơi gọi.** Bộ sinh chuyển từ `crates/dict/build.rs` vào
   `crates/dict/src/codegen/` (`mod.rs`, `parse.rs`, `merge.rs`, `model.rs`, `emit.rs`, `error.rs`),
   sạch lint của thư viện (lỗi là giá trị `GenError`, không `panic`/`unwrap`/`expect`, không index
   gây panic). `build.rs` còn lại mỏng, `#[path]` vào module này, sinh `Fix44` (và cặp FIXT khi bật
   `fix50sp2`) **y hệt hôm nay** — chứng minh bằng hash file sinh ra trùng commit cha, trước khi
   có một dòng overlay nào. Feature mới `codegen`, **mặc định tắt**, khai báo `pub mod codegen`;
   `roxmltree` (đã là build-dependency ghim `=0.20.0`) thành dependency tuỳ chọn dưới `codegen`.
   Không tạo crate mới (ADR-0207 nói vì sao).
2. **Đầu vào là XML định dạng QuickFIX**, hai dạng: *overlay* chồng lên `spec/FIX44.xml` (thêm
   field, thêm `<value>` vào field có sẵn, thêm message/component/group, thêm field vào message có
   sẵn kể cả `required="Y"`, thêm field header; trùng số khác tên hoặc trùng tên khác số hoặc đổi
   kiểu → build lỗi nêu tên cả hai); hoặc *nguyên file* FIX 4.4 của người dùng. File trong `spec/`
   chỉ được đọc, nội dung gốc tới bộ sinh bằng `include_str!` bên trong package `fixbolt-dict`.
3. **Đầu ra là kiểu của chính người dùng**: một file Rust để họ `include!`, gồm bảng, một struct
   rỗng tên do họ chọn, `impl codec::Dictionary` và `impl dict::Tables`. Đường dẫn trong code sinh
   ra đi qua mặt tiền (`::fixbolt::dict::…`) theo mặc định, hoặc thẳng tới `fixbolt_dict`/
   `fixbolt_codec` (tuỳ chọn `Paths::direct()`, cho test trong `crates/dict` và người dùng mức
   engine). File sinh ra có một kiểm tra lúc biên dịch: phiên bản định dạng của bộ sinh phải bằng
   của crate lúc chạy.
4. **Mặt tiền generic thêm, không phá**: module `fixbolt::dict` (re-export `Dictionary`, `Tables`,
   `FieldType`, `Fix44`, `TagValue`); `App`, `Handler`, `Reply`, `Incoming` thêm tham số kiểu cuối
   `D = Fix44`; ba cửa mới `serve_over`, `serve_hft_over`, `connect_and_serve_over` generic theo
   `E`. **Cửa cũ trở thành lời gọi vào cửa mới với `E = TagValue<Fix44, N>`** — một thân hàm, không
   hai bản sao.
5. **Nhiều sàn trong một process = nhiều engine**, mỗi engine một kiểu từ điển.
6. **Cấu hình**: `UseDataDictionary`, `DataDictionary`, `TransportDataDictionary`,
   `AppDataDictionary` bị từ chối có tên (`Problem::DictionaryIsBuildTime`), câu báo lỗi chỉ tới
   how-to.
7. **Crate ví dụ mới** `examples/custom-dictionary` (thành viên workspace, `publish = false`): `build.rs`
   sinh `Venue` từ `venue.xml` (một phương ngữ **tự bịa**, ghi rõ ở đầu file — repo này không được
   chứa đặc tả sàn nào) và `Plain` từ overlay rỗng; `src/main.rs` là acceptor dùng `serve_over`; code
   của how-to là code của crate này.

## Bất biến bị đụng tới

- **1 — không cấp phát trên đường nóng.** Bảng sinh ra là `static`; tra cứu y như `Fix44`. Giữ bằng
  `examples/custom-dictionary/benches/alloc.rs` với ba case `venue-parse-group`,
  `venue-validate`, `venue-reply`, mỗi case khẳng định đường của nó chạy thật và đọc 0; thêm vào
  `TARGETS` và `is_invariant` của `scripts/bench.sh`. `crates/library/benches/alloc.rs` hiện có phải
  giữ 0 sau khi thêm tham số `D`.
- **2 — session thuần.** Không sửa `crates/session`. Session đã generic theo `E::Dict: Tables`.
- **3 — 59 định nghĩa.** Mọi PR đụng `dict`/`engine`/`library` chạy `cargo test -p fixbolt-session
  --test score` (59/59) và bộ FIXT với `--features fix50sp2`; thêm `an_empty_overlay_scores_59_of_59`
  chạy bộ 59 trên kiểu `Plain` sinh bởi đường overlay.
- **4 — theo mode.** Cửa mới dùng chung thân với cửa cũ; không đổi chiến lược chờ. Test
  `serve_over_with_fix44_answers_like_serve` chứng minh cùng byte trên dây.
- **5 — thứ tự field từ bảng sinh.** Kiểu của người dùng lấy thứ tự từ bảng của chính nó; test group
  tuỳ biến ghi theo thứ tự khai báo, và field header tuỳ biến nằm trong header.
- **6 — feature gate chính `mod`.** `#[cfg(feature = "codegen")] pub mod codegen;`; job
  `no-default-features`, `scripts/check-no-optional-deps.sh`, `cargo hack … --feature-powerset`.
- **7 — không panic trong thư viện.** Bộ sinh chuyển vào `src/` nên chịu lint và ratchet
  `check-indexing-debt.sh` (con số không được tăng).
- **9 — ba file QuickFIX giữ nguyên byte.** Không ghi vào `spec/`; `scripts/check-dict-spec-pin.sh`
  xanh ở mọi commit.
- **10 — số hiệu năng.** Plan không công bố số latency nào; trang tài liệu chỉ link tới chỗ số đã
  ghi. Kích thước bảng (KB) là số đếm, ghi kèm lệnh đo.
- 8 (`unsafe`): không có `unsafe` mới.

## Chia việc

Cột *Người làm* theo `CLAUDE.md` §12. Hàng đụng `dict`, `engine`, `library` (và do đó chạm đường
nóng hoặc cổng session) đi **senior developer (opus)**; không hàng nào sửa mã nguồn `codec` hay
`session`. Manager chạy lại gate và commit sau mỗi hàng xanh; developer không commit. Mỗi PR mở
nháp ở commit đầu, kết thúc bằng một senior review (context mới) và CI xanh có run id.

| Bước | Kết quả | Người làm | File được sửa / không được sửa | Gate | Phụ thuộc |
|---|---|---|---|---|---|
| **PR 0** | **Plan này + ADR-0206, ADR-0207 + trang prior-art** (nhánh `plan/docs-for-embedders`) | architect | chỉ ba file đó và plan | `scripts/check-links.py`, `scripts/check-adr-numbers.sh` | — |
| 0 | Chủ dự án duyệt plan (Q1–Q6 đã trả lời 2026-09-26) | chủ dự án | — | — | PR 0 |
| **PR 1** | **Khung sách** — nhánh `docs/book-skeleton` | | | | 0 |
| 1 | `book.toml`, `docs/SUMMARY.md` (phần viết tay, chỉ file đang có), `docs/index.md`, `docs/api.md`, `docs/contributing.md` (link ra ba file gốc, tạm là stub cho tới PR 2) | developer (sonnet) | chỉ các file này; không sửa file `docs/` đang có | `mdbook build` thoát 0 | 0 |
| 1a | `PRD.md` §2: mục mới *Phase 5: dependable by an embedder* đặt sau phase 4 (phạm vi = plan này, tiêu chí thoát = gate của PR 1–6, trỏ ADR-0206/0207); hàng kernel bypass ở *Later phases* ghi thêm "comes after phase 5". **Không sửa nội dung ADR-0204/0205** — xem mục *Quyết định của chủ dự án*, Q3 | developer (sonnet) | chỉ `PRD.md` §2 | `check-links.py` | 0 |
| 2 | `scripts/gen-book-summary.py` sinh phần ADR / reference / internals của `SUMMARY.md`; `--check` đỏ khi thiếu hoặc thừa. Đảo ngược: xoá một dòng ADR khỏi `SUMMARY.md` → đỏ với câu nêu tên file | developer (sonnet) | script + `SUMMARY.md` | `scripts/gen-book-summary.py --check`, và câu đỏ khi đảo ngược | 1 |
| 3 | `scripts/mdbook-repo-links.py` (preprocessor): link ra ngoài `docs/` hoặc tới trang không trong `SUMMARY.md` → URL GitHub ở commit đang build. Test trong job `script-logic`: `../crates/codec/src/dict.rs` → `…/blob/<sha>/crates/codec/src/dict.rs`; link nội bộ trong sách giữ nguyên | developer (sonnet) | script + test của nó + `book.toml` | test script xanh; đảo ngược (tắt preprocessor) → bước 4 đỏ | 1 |
| 4 | `scripts/check-links.py --rendered target/book`: đọc HTML bằng thư viện chuẩn, kiểm mọi `href` nội bộ **và anchor**; ghi ra danh sách anchor lệch giữa GitHub và mdBook (đo, không đoán) | developer (sonnet) | `scripts/check-links.py` (thêm chế độ, không đổi chế độ cũ) | chạy trên sách đã build: 0 lỗi, hoặc danh sách lỗi được sửa ở nguồn | 2, 3 |
| 5 | `scripts/check-doc-samples.sh` (marker `<!-- sample: <path> -->`, khối code phải trùng byte với file đã biên dịch); `scripts/check-cited-headings.sh` (một heading `## N.` có trên `main` của `DESIGN.md`, `GUIDE.md`, `SESSION-BEHAVIOUR.md`, `CONFIGURATION.md`, `CONFORMANCE.md` mà biến mất → đỏ). Mỗi script chứng minh bằng đảo ngược | developer (sonnet) | hai script | câu đỏ khi đảo ngược, xanh khi khôi phục | 1 |
| 6 | CI: job `book` (mdBook v0.5.4 bản nhị phân ghim sha256, `gen-book-summary.py --check`, `mdbook build`, grep cảnh báo của mdBook trong log, bước 4, bước 5); `.github/workflows/pages.yml` deploy chỉ khi push `main`. `CLAUDE.md` §4 hai bảng: thêm hàng `ARCHITECTURE.md`, `CONTRIBUTING.md`, `SECURITY.md`, `docs/SUMMARY.md` + `book.toml`, `docs/how-to/`, `docs/explanation/`; hàng đồng bộ "thêm trang dưới `docs/` → `SUMMARY.md`", "đổi ranh giới crate hoặc một invariant → `ARCHITECTURE.md`". Sửa `CLAUDE.md` dùng skill `mattpocock-skills:writing-for-agents`; manager nói rõ luật nào đổi | developer (sonnet) | `.github/workflows/`, `CLAUDE.md` §4, `README.md` mục *Layout* | job `book` xanh trên PR | 2–5 |
| 7 | Senior review + CI; chủ dự án (hoặc manager qua API nếu token có quyền) bật Pages nguồn "GitHub Actions", URL mặc định (Q4) | senior developer (opus); manager | — | CI xanh, run id | 6 |
| **PR 2** | **Bộ cho người đóng góp** — nhánh `docs/contributor-set` | | | | PR 1 |
| 8 | `ARCHITECTURE.md` theo kiểu matklad, như mục *Cách làm* | architect (opus) | chỉ file này | `check-links.py`; bảng tay: mỗi invariant ↔ mục §2 ↔ máy kiểm | PR 1 |
| 9 | `CONTRIBUTING.md`, `SECURITY.md`; `docs/contributing.md` hết là stub | developer (sonnet) | ba file | `check-links.py`, job `book` | 8 |
| 10 | Senior review + CI; chủ dự án bật private vulnerability reporting | senior developer (opus); manager | — | CI xanh, run id | 9 |
| **PR 3** | **Trang cho người nhúng** — nhánh `docs/embedder-pages` | | | | PR 1 |
| 11 | `docs/explanation/design-rationale.md` | architect (opus) | chỉ file này + `SUMMARY.md` | `check-links.py`, job `book` | PR 1 |
| 12 | `docs/explanation/why-fixbolt.md` (giải thích cơ chế, theo mục *Cách làm*) | architect (opus) | như trên | như trên + bước 14 | PR 1 |
| 13 | `docs/explanation/why-rust.md` | architect (opus) | như trên | như trên + bước 14 | PR 1 |
| 14 | `scripts/check-doc-claims.sh`: grep `docs/`, `README.md`, `ARCHITECTURE.md` tìm các câu cấm (Jane Street, Jump Trading, Tower Research, "98.7"), từ ngữ giấy phép tương lai ("commercial licen", "enterprise edition", "pricing"), và dòng có tên engine khác cùng từ so sánh (xem `why-fixbolt.md` ở *Cách làm*); trang reference prior-art được miễn phần so sánh vì nó ghi lời của nhà cung cấp. Đảo ngược bằng một dòng mồi mỗi loại | developer (sonnet) | script + job CI `links` | câu đỏ khi đảo ngược | 11 |
| 15 | `README.md` viết lại phần đầu; `INTRODUCTION.md` §4/§5 thêm link; `docs/how-to/index.md`; dòng mở đầu cho `GETTING-STARTED.md`/`TUTORIAL.md`; marker `sample:` cho khối code của `TUTORIAL.md` | developer (sonnet) | các file đó; **không** đụng khối `stranger-check` | `check-links.py`, `check-doc-samples.sh`, job `stranger-git`, job `book` | 11–13 |
| 16 | Senior review, trọng tâm: mọi con số có nguồn, số của bên khác ghi "their claim", trung lập về giấy phép | senior developer (opus) | — | CI xanh, run id | 15 |
| **PR 4** | **Bộ sinh thành thư viện, không đổi hành vi** — nhánh `dict/generator-library`; chạy song song được với PR 1–3 | | | | 0 |
| 17 | Khung `crates/dict/src/codegen/` sau feature `codegen`, hàm trả `Err(GenError::Unsupported)`; test đỏ ở khẳng định: `crates/dict/tests/gen_matches_build.rs` — `generated_fix44_equals_the_build_output`, `generated_pair_equals_the_build_output` (dưới `fix50sp2`) so với `include_str!(concat!(env!("OUT_DIR"), …))` | senior developer (opus) | `crates/dict/{Cargo.toml,src/lib.rs,src/codegen/,tests/gen_matches_build.rs}`; **không** sửa `spec/`, `crates/codec`, `crates/session` | output đỏ quote lại | PR 0 |
| 18 | Chuyển bộ sinh vào `src/codegen/`, sạch lint; `build.rs` mỏng. Giữ **nguyên câu** của ba nhánh `die` mà `check-dict-refuses-a-message-without-msgcat.sh` đọc | senior developer (opus) | `crates/dict/build.rs`, `src/codegen/` | sha256 của `fix44.rs` và `fixt11_fix50sp2.rs` trùng commit cha (ghi vào thân commit); bước 17 xanh; `cargo test -p fixbolt-dict --tests` (có `vendor/`); `scripts/check-dict-refuses-a-message-without-msgcat.sh`; `scripts/check-indexing-debt.sh`; `cargo clippy --all-targets --all-features -- -D warnings` | 17 |
| 19 | Gate feature: `--no-default-features`, `--features codegen`, `scripts/check-no-optional-deps.sh`, `scripts/check-feature-gated-tests-ran.sh` cho các test `codegen`, job `package` (`.crate` có `spec/**`); tài liệu cùng commit: `docs/internals/dict.md` (số dòng `build.rs` mà trang đang trích sẽ đổi), `DESIGN.md` §3 hàng `dict`, `CONFIGURATION.md` §4, `CHANGELOG.md` | senior developer (opus) code; developer (sonnet) tài liệu | CI yml nếu cần thêm tổ hợp feature | 59/59 `--test score`; FIXT `--features fix50sp2`; `check-dict-spec-pin.sh` | 18 |
| 20 | Senior review + CI | senior developer (opus), context mới | — | CI xanh, run id | 19 |
| **PR 5** | **Overlay** — nhánh `dict/overlay` | | | | PR 4 |
| 21 | Fixture tự bịa `crates/dict/tests/fixtures/overlay-invented.xml` (đầu file ghi: tự bịa, không phải đặc tả sàn nào). Test đỏ `crates/dict/tests/overlay.rs` (feature `codegen`), hỏi trên mô hình đã gộp: `an_empty_overlay_emits_byte_identical_fix44`, `an_added_field_is_defined_and_typed`, `an_added_enum_value_is_allowed_and_the_old_ones_still_are`, `an_added_group_has_its_delimiter_members_and_declared_order`, `a_group_reused_in_two_messages_keeps_each_delimiter`, `an_added_message_type_is_a_message_type_and_not_admin`, `a_field_added_to_a_message_as_required_is_required`, `an_added_header_field_is_a_header_field`, `a_number_reused_with_another_name_fails_naming_both`, `a_name_reused_with_another_number_fails_naming_both`, `a_retyped_field_fails`, `a_data_field_added_without_its_length_field_fails`, `a_whole_file_generates_as_is`, `a_high_tag_reports_the_table_size` | senior developer (opus) | `crates/dict/tests/`, `src/codegen/` (stub) | output đỏ quote lại | PR 4 |
| 22 | Cài overlay + nguyên file + `Paths` + kiểm tra phiên bản định dạng (một doctest `compile_fail` chứng minh lệch phiên bản không biên dịch) + `cargo:warning` kích thước bảng | senior developer (opus) | `crates/dict/src/codegen/` | bước 21 xanh; bước 18–19 vẫn xanh (hash `fix44.rs` không đổi) | 21 |
| 23 | Tài liệu cùng commit: `docs/internals/dict.md`, `CONFIGURATION.md` §5 (overlay khác gì `NANOFIX_FIX44_XML`), `DESIGN.md` §4 D3 (bảng của người dùng cũng theo D3), trang `docs/reference/` cho mỗi bất ngờ, `CHANGELOG.md` | developer (sonnet) | chỉ `docs/`, `CHANGELOG.md` | `check-links.py`, job `book` | 22 |
| 24 | Senior review + CI | senior developer (opus) | — | CI xanh, run id | 23 |
| **PR 6** | **Từ điển tuỳ biến tới tận ứng dụng** — nhánh `library/custom-dictionary` | | | | PR 1, PR 5 |
| 25 | Test đỏ: `crates/library/tests/dictionary_param.rs` — `a_reply_over_a_dialect_orders_its_custom_group_by_the_dialect`, `an_app_over_the_default_is_fix44`; `crates/engine/tests/serve_over.rs` — `serve_over_with_fix44_answers_like_serve`; test settings `a_data_dictionary_key_is_refused_by_name` (bốn key) | senior developer (opus) | test mới; **không** sửa test có sẵn | output đỏ quote lại | PR 5 |
| 26 | Cài: `D` trên `App`/`Handler`/`Reply`/`Incoming`; `fixbolt::dict`; ba cửa `_over` với cửa cũ gọi vào chúng; `Problem::DictionaryIsBuildTime` | senior developer (opus) | `crates/library/src/`, `crates/engine/src/lib.rs`, `crates/engine/src/settings.rs`; **không** `crates/session`, `crates/codec` | bước 25 xanh; `grep -n 'Fix44' crates/library/src` chỉ còn default và re-export; `crates/library/benches/alloc.rs` đọc 0; `scripts/check-semver-against-tag.sh` (thêm vào phải là minor — nếu không, dừng, về architect) | 25 |
| 27 | Crate `examples/custom-dictionary` (thêm vào `members`): `build.rs`, `venue.xml`, `src/main.rs`; test qua socket thật `tests/venue.rs` — `a_custom_tag_reaches_the_handler`, `a_custom_enum_value_is_not_rejected`, `a_custom_group_is_read_and_echoed_in_declared_order`, `a_custom_header_field_is_written_in_the_header`, `a_custom_message_type_is_delivered`, `a_missing_venue_required_field_is_rejected_373_1`, `an_undefined_tag_is_still_rejected_373_0`, `an_undefined_user_tag_passes_when_user_defined_fields_are_skipped`; `tests/plain_scores.rs` — `an_empty_overlay_scores_59_of_59`; `benches/alloc.rs` ba case, thêm vào `scripts/bench.sh` | senior developer (opus) | crate mới, `Cargo.toml` gốc, `scripts/bench.sh` | các test trên xanh; job `bench` in ra ba case với 0 | 26 |
| 28 | `scripts/check-custom-dictionary-packaged.sh`: dựng crate ví dụ **ngoài workspace** trên `target/package/` (bản `.crate`), chứng minh `codegen` đọc được `spec/` từ package và đường `::fixbolt::dict` đúng; `cargo tree -e normal` của nó không có `roxmltree`. Gắn vào job `package` | developer (sonnet) | script + CI yml | script thoát 0; đảo ngược (bỏ `spec/**` khỏi `include`) → đỏ | 27 |
| 29 | Tài liệu cùng commit: `docs/how-to/add-a-custom-tag.md` (hai đường: bỏ qua bằng `ValidateUserDefinedFields=N`, hay định nghĩa nó), `use-a-venue-dictionary.md` (overlay, nguyên file, nhiều sàn = nhiều engine, phải build lại khi đổi, nghĩa vụ `NOTICE`, kích thước bảng với tag cao), `migrate-from-quickfix.md` (bảng key QuickFIX → fixbolt, bốn key bị từ chối, `ValidateFieldsOutOfOrder` không hỗ trợ); marker `sample:` trỏ vào file của crate ví dụ; `CONFIGURATION.md` §1/§4; `GUIDE.md` §3a một đoạn; `SESSION-BEHAVIOUR.md` §3; `DESIGN.md` §3 (crate mới, hàng `library`, `engine`); `PRD.md` §3 hàng mới; `docs/internals/` (trang mới + `library.md`, `engine.md`, `README.md`); `README.md` *Layout*; `SUMMARY.md`; `CHANGELOG.md` | developer (sonnet) | chỉ tài liệu | `check-links.py`, `check-doc-samples.sh`, job `book` | 27 |
| 30 | Senior review + CI; `STATUS.md` (*Start here*, *Not proven*); `PRD.md` §2 ghi phase 5 xong; ADR-0206/0207 → Accepted (manager ghi dòng trạng thái); *Nhật ký giao hàng* | senior developer (opus); manager | `STATUS.md`, `PRD.md` §2, hai ADR | CI xanh trên commit đóng, run id | 29 |

**Ranh giới PR:** PR 0 (plan) → PR 1 (khung sách) → PR 2 và PR 3 song song. PR 4 → PR 5 chạy
song song với PR 1–3 (không đụng file chung ngoài `CHANGELOG.md`, `DESIGN.md` — mỗi lúc một người
viết). PR 6 cần cả PR 1 (sách, kiểm mẫu code) lẫn PR 5.

## Cách kiểm chứng

**Phần A.** Trên commit đóng mỗi PR, manager chạy lại và quote dòng kết quả:

- `scripts/check-links.py` (nguồn) và `scripts/check-links.py --rendered target/book` (sách đã
  build): "0 dead links". Chạy sau `mdbook build`, đọc **mã thoát và** grep log tìm `WARN`/`ERROR`
  của mdBook (mdBook có cảnh báo không làm build thất bại).
- `scripts/gen-book-summary.py --check`: in số ADR / reference / internals đã khớp (131 / 132 / 11
  tại thời điểm plan, cộng những file PR đó thêm).
- `scripts/check-doc-samples.sh`, `scripts/check-doc-claims.sh`, `scripts/check-cited-headings.sh`:
  mỗi cái có một lần đảo ngược ghi trong thân commit, với câu FAIL viết ra **trước** khi chạy
  (`CLAUDE.md` §7).
- Job `stranger-git` và `package` xanh: marker `stranger-check` trong `GETTING-STARTED.md` không bị
  đụng.
- Sau khi PR 1 merge: mở URL Pages, chụp màn hình trang chủ và một trang có link ra `../crates/`,
  bấm link đó thấy file trên GitHub đúng commit. Bằng chứng này vào *Nhật ký giao hàng*.

**Phần B.**

- PR 4: sha256 của `$OUT_DIR/fix44.rs` và `fixt11_fix50sp2.rs` trước/sau (lệnh `find target -name
  fix44.rs -path '*fixbolt-dict*' | xargs shasum -a 256`), phải trùng. `cargo test -p fixbolt-dict
  --tests --no-fail-fast` (có `vendor/`: `interop_quickfix_*` so với C++ sinh bởi QuickFIX, chưa
  từng là bảng của chính mình). `cargo test -p fixbolt-session --test score` → 59/59. FIXT:
  `cargo test -p fixbolt-session --tests --features fix50sp2` với điểm như `CONFORMANCE.md` §9.
- PR 5: các test ở bước 21, cộng `an_empty_overlay_emits_byte_identical_fix44` — đây là sợi dây nối
  đường overlay với đúng bảng mà 59 định nghĩa đang kiểm.
- PR 6: test socket ở bước 27 là **bản ghi thật đi qua socket kernel thật**, không phải gọi hàm;
  `an_empty_overlay_scores_59_of_59` chạy cả bộ định nghĩa QuickFIX trên kiểu sinh qua overlay;
  `scripts/bench.sh` (job `bench`) in ba case alloc mới với 0 — đọc dòng output, không đọc mã thoát;
  `scripts/check-custom-dictionary-packaged.sh` với dòng `Compiling fixbolt-dict … (…/target/package/…)`.
- Mọi PR: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
  `cargo test --all`, `cargo test --no-default-features`, CI xanh có run id cho commit đóng.

**Không có dữ liệu sàn thật nào** được dùng hay commit; phương ngữ trong test là tự bịa. Vì vậy
cái PR 6 chứng minh là "cơ chế đúng với một phương ngữ tự bịa", **không** phải "chạy được với sàn
X" — ghi vào `STATUS.md` *Not proven*.

## Tài liệu phải cập nhật

Theo bảng đồng bộ ở `CLAUDE.md` §4 (đi từng hàng):

- [ ] Thêm/đổi crate (`examples/custom-dictionary`): `DESIGN.md` §3 + `README.md` *Layout* +
      `Cargo.toml` `members` + trang `docs/internals/` (bước 27, 29)
- [ ] API công khai (`fixbolt::dict`, tham số `D`, ba cửa `_over`, feature `codegen`, `GenError`):
      `DESIGN.md`, rustdoc, `CHANGELOG.md` (bước 19, 22, 26, 29)
- [ ] Ràng buộc người dùng phải giữ mà compiler không kiểm (build lại khi đổi phương ngữ; build-dep
      và runtime cùng tag; nghĩa vụ `NOTICE`): `GUIDE.md` §3a (bước 29)
- [ ] Key cấu hình / feature / hằng số người dùng thấy: `CONFIGURATION.md` §1, §4, §5 (bước 19, 23, 29)
- [ ] Hành vi biên session (tag định nghĩa bởi phương ngữ): `SESSION-BEHAVIOUR.md` §3, nêu test canh
      (bước 29)
- [ ] Hành vi codec/dispatch (bảng người dùng theo D3): `DESIGN.md` §4 D3, và đi lại §2 (bước 23)
- [ ] Bẫy / bất ngờ: `docs/reference/` — trang prior-art (PR 0) và mỗi bất ngờ gặp khi làm
- [ ] Quyết định mới: ADR-0206, ADR-0207 (PR 0); ADR giấy phép / CLA hay DCO — hoãn tới trước đóng góp ngoài đầu tiên (ngoài plan này)
- [ ] Chuyển việc vào phase: `PRD.md` §2 phase 5 (bước 1a, đóng ở bước 30), ADR-0206 quyết định 9; `PRD.md` §3 hàng mới (bước 29)
- [ ] Chứng minh điều đang ghi "chưa chứng minh": gạch dòng tương ứng trong `STATUS.md` *Not proven*
- [ ] `CLAUDE.md` §4 hai bảng (bước 6); `STATUS.md` (bước 30)

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| mdBook chỉ render file có trong `SUMMARY.md`; một ADR mới quên thêm là vô hình trên web | `gen-book-summary.py --check` trong job `book` |
| Link từ `docs/` ra `../crates/…` thành 404 trên web | preprocessor bước 3 + `check-links.py --rendered` bước 4 |
| Id heading của mdBook khác GitHub (gạch dài, backtick, `§`, số mục) → anchor chạy trên GitHub, gãy trên web | bước 4 kiểm anchor trên HTML đã render và in danh sách lệch |
| `internals/README.md` trong thư mục con → lỗi đã biết của mdBook với README | bước 4 phủ trang đó |
| Ai đó "dọn" một heading có số mục đang bị trích dẫn | `check-cited-headings.sh` bước 5 |
| Khối code trong how-to trôi khỏi code thật | `check-doc-samples.sh` bước 5 |
| Sửa nhầm khối `stranger-check` trong `GETTING-STARTED.md` | job `stranger-git`, job `package` |
| Câu không có nguồn (Jane Street…) hoặc lời hứa giấy phép lọt vào trang | `check-doc-claims.sh` bước 14 |
| Refactor bộ sinh đổi âm thầm một bảng | hash `fix44.rs`/`fixt11_fix50sp2.rs` trùng commit cha; `interop_quickfix_*` |
| Câu lỗi của ba nhánh `die` đổi → script msgcat mất khả năng thấy chúng | `scripts/check-dict-refuses-a-message-without-msgcat.sh` bước 18 |
| Bộ sinh vào `src/` kéo index gây panic / `unwrap` theo | clippy `-D warnings`, `check-indexing-debt.sh` |
| Test sau feature `codegen` không bao giờ chạy trong CI (reference `a-feature-gated-test-is-a-test-ci-never-runs`) | `check-feature-gated-tests-ran.sh` nêu tên test `codegen` |
| `codegen` chạy trong cây nhưng hỏng từ `.crate` (thiếu `spec/` trong `include`) | `check-custom-dictionary-packaged.sh` bước 28, có đảo ngược |
| Code sinh ra ghi `crate::FieldType` → không biên dịch trong crate người dùng | `Paths` + crate ví dụ biên dịch (bước 27, 28) |
| Build-dependency và runtime khác tag → bảng sinh theo một định dạng, đọc theo định dạng khác | kiểm tra phiên bản lúc biên dịch + doctest `compile_fail` (bước 22) |
| Field DATA thêm vào mà không có field độ dài (D3: `89`/`93`) | `a_data_field_added_without_its_length_field_fails` |
| Delimiter group theo `(msg_type, counter)`, không theo counter | `a_group_reused_in_two_messages_keeps_each_delimiter` |
| Field header tuỳ biến bị xếp vào body khi ghi (non-negotiable 5) | `an_added_header_field_is_a_header_field`, `a_custom_header_field_is_written_in_the_header` |
| Một tag cao làm bảng bitset phình (233 KB với tag 20000) | `a_high_tag_reports_the_table_size`; how-to ghi rõ |
| Feature của build-dep lan sang bản target (`roxmltree` vào binary) — reference `feature-flags-unify-across-a-workspace` | `cargo tree -e normal` trong bước 28 |
| Bench alloc mới không nằm trong `TARGETS` của `bench.sh` nên không bao giờ chạy | bước 27 thêm vào; đọc dòng output của job `bench` |
| Cửa `_over` viết thành bản sao của cửa cũ → hai thân hàm trôi khác nhau | cửa cũ gọi cửa mới; `serve_over_with_fix44_answers_like_serve` |
| Tham số `D` bị `cargo semver-checks` coi là phá API | `check-semver-against-tag.sh` bước 26; đỏ → dừng, về architect |
| Tag 5000 trong fixture làm `14a_BadField.def` đổi nghĩa nếu chạy bộ 59 trên `Venue` | bộ 59 chỉ chạy trên `Plain`; fixture ghi chú điều này |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Viết lại 1 595 dòng bộ sinh cho sạch lint khó hơn dự kiến, không đạt hash trùng | Cao | PR 4 là PR riêng, không chặn phần A; không đạt hash trùng thì dừng ở bước 18, báo lại, không merge |
| Thêm tham số `D` vào trait `Handler` bị semver gate coi là major | Trung bình | Dừng ở bước 26, về architect; phương án dự phòng (một trait mới cho phương ngữ) cần sửa ADR-0207 và duyệt lại |
| Người dùng mong đổi phương ngữ không cần build lại, như QuickFIX | Trung bình | Chủ dự án đã chấp nhận (Q6), ghi trong ADR-0207 *Consequences*; how-to và `why-fixbolt.md` nói thẳng |
| Trang web công khai từ lần deploy đầu; sai ở đó là sai trước công chúng | Trung bình | Chủ dự án đã cho bật (Q4); mọi trang qua senior review như repo trước khi merge vào `main` |
| Phase 5 xếp sau phase 4, nhưng phase 4 hàng 7 còn chờ một lần boot máy §9. Nếu PR của plan này mở trước khi phase 4 đóng → xung đột `DESIGN.md`, `STATUS.md`, `CHANGELOG.md` | Thấp | Thứ tự phase không nói hai việc có được chạy chồng không: manager hỏi chủ dự án một câu khi bắt đầu PR 1 nếu phase 4 chưa đóng. Một người viết mỗi file mỗi lúc; rebase trước khi mở PR (PR xung đột không có CI) |
| Sách rất lớn (131 ADR, 132 reference) → build chậm, index tìm kiếm nặng | Thấp | Đo ở PR 1, ghi số vào *Nhật ký giao hàng* |

### Quyết định của chủ dự án (2026-09-26)

Sáu câu hỏi của bản nháp đã được trả lời; không còn câu hỏi mở nào chặn plan.

| # | Câu hỏi | Câu trả lời | Plan ghi ở đâu |
|---|---|---|---|
| Q1 | Ai đóng góp; CLA hay DCO? | Người đóng góp là lập trình viên trong nhóm dự án, không phải công chúng. Đóng góp từ ngoài **chưa được nhận** cho tới khi một ADR giấy phép quyết định CLA hay DCO. **Việc hoãn**, hạn: trước khi nhận đóng góp ngoài đầu tiên; không chặn plan này | *Cách làm* `CONTRIBUTING.md`; *Tài liệu phải cập nhật* |
| Q2 | Trang "why fixbolt" là gì? | Giải thích kỹ thuật vì sao fixbolt nhanh (cơ chế, chi phí tránh được), không phải so sánh marketing. Engine khác chỉ là sự thật thiết kế có nguồn khi cần; không xếp hạng, không "nhanh hơn X" | *Cách làm* `why-fixbolt.md` (kèm lý do nằm trong ADR-0104 quyết định 7); bước 12, 14, 16; ADR-0206 quyết định 8 |
| Q3 | Việc này ở phase nào? | **Phase mới (5), sau phase 4 và trước kernel bypass.** ADR-0204 không gán số phase cho bypass ("no phase number until the owner scopes a phase that includes it") nên câu đó vẫn đúng; câu "phase 5 is not scoped" trong ADR-0204 *Context* là sự thật tại ngày viết. ADR-0141 "FIXP not before phase 5" vẫn đúng, vì phase 5 không chứa FIXP. **Không sửa nội dung ADR đã Accepted, không cần ADR thay thế**: việc xếp phase ghi ở `PRD.md` §2 (bước 1a) và ADR-0206 quyết định 9 | bước 1a, 30; ADR-0206 quyết định 9 |
| Q4 | Pages, báo lỗ hổng? | Bật GitHub Pages (URL mặc định) và private vulnerability reporting. Không email: `SECURITY.md` chỉ trỏ tới private vulnerability reporting | *Cách làm* `SECURITY.md`; bước 7, 10 |
| Q5 | Overlay cho FIXT / SP2? | Chỉ FIX 4.4 ở bản đầu; overlay FIXT 1.1 / FIX 5.0 SP2 ngoài phạm vi | *Ngoài phạm vi*; ADR-0207 quyết định 8 |
| Q6 | Chấp nhận build lại khi đổi phương ngữ? | Chấp nhận | ADR-0207 *Consequences* (ghi là chủ dự án chấp nhận) |

## Ngoài phạm vi

- Mô hình giấy phép thương mại, giá, bản "enterprise" — ADR riêng sau này (quyết định của chủ dự án).
- Benchmark đối đầu với engine khác. Số của họ chỉ được trích như lời họ nói.
- Từ điển nạp lúc chạy (ADR-0207 phương án A, bị loại).
- Overlay lên cặp FIXT 1.1 + FIX 5.0 SP2 — chủ dự án quyết định bản đầu chỉ FIX 4.4 (Q5); cửa
  `_over` cho các cửa sharded và TLS.
- Quyết định CLA hay DCO, và nhận đóng góp từ ngoài nhóm — ADR giấy phép sau, trước đóng góp
  ngoài đầu tiên (Q1).
- Chia nhỏ `GUIDE.md` hay `DESIGN.md`; dời bất kỳ file đang bị trích dẫn (ADR-0206 quyết định 3).
- Đưa `docs/plans/` vào sách; dịch tài liệu.
- Xuất bản rustdoc lên Pages, lên crates.io, docs.rs (ADR-0161 giữ nguyên).
- Class message có kiểu sinh cho từng message (ADR-0003 giữ `MessageView`).
- Che (redact) tag bí mật của sàn trong message log — lỗ hổng đã ghi ở ADR-0110, không đổi ở đây.
- Nhận đầu vào FIX Orchestra; công cụ sửa từ điển.

## Nhật ký giao hàng

### PR 0 — plan (#116, merge `4699704`, CI run 36242130265 xanh)

Chủ dự án duyệt 2026-09-26, chạy song song với lần boot của phase 4.
**Sửa sau khi duyệt (chỉ đổi tên):** feature và module `gen` đổi thành `codegen` ở bước 17 —
`gen` là từ khoá dành riêng của Rust edition 2024, `pub mod gen` không biên dịch. Thiết kế không
đổi; ADR-0207 ghi lần sửa tại chỗ.
Chủ dự án yêu cầu (2026-09-26): mọi bước viết tài liệu chạy trên Opus thay cho Sonnet.

### PR 1 — khung sách (#118, nhánh `docs/book-skeleton`)

- Bước 1–6 xong; senior review (bước 7) ra 1 lỗi trung bình + 4 lỗi nhẹ, đã sửa ở `02f6ea9`.
- **Lệch so với plan:** test của preprocessor chạy trong job `book`, không phải `script-logic`
  như hàng 3 ghi — repo chưa có job `script-logic` cho script Python.
- **Bất ngờ đã đo:** đưa đủ 282 trang ADR/reference/internals vào sách làm chỉ mục tìm kiếm
  18 MB (quá ngưỡng cảnh báo 10 MB của mdBook); bỏ `decisions/` và `reference/` khỏi tìm kiếm
  (vẫn trong sidebar) còn 4 MB. Ghi vào ADR-0206.
- **Chưa chứng minh:** anchor của link đã viết lại thành URL GitHub không được kiểm (ghi trong
  phần "cannot see" của hai script). Pages cần chủ dự án đặt nguồn "GitHub Actions".
- CI xanh cho commit đóng: `b6512e7`, run 36245351372 (22/22); merge `b840f9d`.

### PR 4 — bộ sinh thành thư viện (#119, nhánh `dict/generator-library`)

- Bước 17–19 xong; senior review (bước 20) ra 1 lỗi trung bình + 4 nhẹ, sửa ở `e7dc2b2`.
- **Đổi tên:** feature/module là `codegen` (xem PR 0).
- **Byte identity:** `fix44.rs` `323eafd4…`, `fixt11_fix50sp2.rs` `deeb5016…` trước và sau; nay được
  ghim bằng `tests/generated_is_pinned.rs` (review phát hiện `gen_matches_build` so bộ sinh với
  chính nó sau khi `build.rs` nạp cùng code qua `#[path]`).
- **Bẫy mới, mỗi cái có test canh:** script cần bash 4 báo ok khi chạy bằng bash 3.2 mà không
  kiểm gì (`check-old-bash-is-refused.sh`); `gen` là từ khoá của edition 2024 (trình biên dịch);
  bộ đếm index không thấy thư mục con — đã sửa, số vẫn 176 (đảo ngược: 176 → 177 đỏ).
- **Chưa chứng minh trên máy này:** clippy toàn workspace `--all-features` (`ktls-core` chỉ
  Linux) — CI chạy.
- CI xanh cho commit đóng: `7487171`, run 36246973797 (22/22); merge `b3799df`.

### PR 2 — bộ cho người đóng góp (#120, merge `414b640`, run 36254765790 trên `29443cf`, 22/22)

- `ARCHITECTURE.md` 188 dòng, 15 Architecture Invariant, mỗi cái trỏ về mục §2 và máy kiểm.
- Review: 1 trung bình (CONTRIBUTING chép lệnh gate thay vì link) + 3 nhẹ, đã sửa.
- **Bất ngờ:** ba subagent viết tài liệu tự dừng vì tưởng chủ dự án từ chối quyền; thực ra là
  hook GateGuard. Chủ dự án xác nhận không chặn gì.
- Chủ dự án (2026-09-26): "dự án có team phát triển (hiện tại chỉ mình tôi)".

### PR 3 — trang cho người nhúng (#121, nhánh `docs/embedder-pages`)

- Ba trang giải thích, README mới, TUTORIAL mà code là file ví dụ đã biên dịch (4 marker
  `sample:`), `check-doc-claims.sh`.
- **Sửa lỗi cũ tìm ra trên đường:** TUTORIAL gọi `shutdown` ngay sau khi spawn; mẫu wire sai
  `9=`/`10=`; `standard` "chạy mọi OS" (thật ra chỉ Unix); số ca cấp phát 24/33 đã cũ (đúng là
  8/17/35); PRD nói chưa từng TLS với engine khác (CI run 35892604235 đã chạy 7/7); DESIGN §1 trích
  số của fix8 ngoài trang prior-art.

