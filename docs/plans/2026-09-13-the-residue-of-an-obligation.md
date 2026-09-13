# Phần dư của một nghĩa vụ — đóng item 67, 71, 72, 73, 74 và 21; nói rõ vì sao 55 không ở đây

> **Loại:** Plan · **Ngày:** 2026-09-13 · **Trạng thái:** Đã duyệt 2026-09-13, theo đề xuất (Q1 có, Q2 (b), Q3 đóng, Q4 trong plan này, Q5 tách) — **đang làm**, nhánh `plan/the-residue-of-an-obligation`, draft PR mở ở commit đầu theo `CLAUDE.md` §8
> **Phạm vi:** sáu open item không cần máy §9, một pull request, một phiên. `mod doc_table` và
> `number()` trong `crates/engine/src/settings.rs`; `scripts/check-links.py`; bốn tài liệu mang con
> số *eighteen*; một cửa vào `hft` có ghim lõi trong `crates/engine/src/lib.rs`.

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

PR [#65](https://github.com/tmthang86/fixbolt/pull/65) đóng bốn item cùng một họ — *một nghĩa vụ
được ghi ra mà không có gì đứng canh* — và **mở bốn item mới** trong lúc làm: 71, 72, 73, 74. Cả
bốn là **phần dư** của chính những guard vừa dựng: một con số đúng lúc viết rồi không ai đọc lại
(71), một gate bắt đúng hai link sai nhưng bắt nhầm một trích dẫn ngoài (72), một parser lỏng hơn
tài liệu ở bảy khoá (73), một tìm kiếm có biên mà biên của nó đã được đo rõ (74). Thêm hai item cũ
hơn cùng tính chất *không cần máy §9*: 67 (gate hai chiều tài liệu ↔ code còn bỏ trống ô nào) và
21 (D8 nói thread engine được ghim, nhưng cửa `serve_hft` không ghim và không từ chối gì).

Chủ sở hữu yêu cầu **đóng mọi open item**. Plan này gom những item đóng được trên bàn làm việc
hoặc trên CI, không cần reboot, không cần `isolcpus`. Mỗi mục có guard mới hoặc quyết định được ghi
thành văn bản có địa chỉ, mỗi guard được chứng minh bằng reversal có câu FAIL viết sẵn. Những chỗ
cần chủ sở hữu quyết thì **để thành câu hỏi riêng, có đề xuất** — xem mục *Câu hỏi cho chủ sở hữu*
ngay dưới *Cách làm*.

## Những gì đã biết chắc

### Đo tại chỗ hôm nay `[measured 2026-09-13]`, máy này (desk, `tmt-B450-I-AORUS-PRO-WIFI`), cây sạch ở `010e7bc`

Mọi phép đo dưới đây là `grep`/`sed` đọc cây hiện tại, **không sửa file nào, không chạy cargo**.

**1. Con số *eighteen* (item 71) nằm ở sáu chỗ, không phải bốn.** Item 71 kể bốn: ADR-0035
(dòng 15, và dòng 102 dùng theo nghĩa khác — *"eighteen different faults"*), ADR-0059 (dòng 45,
bảng so sánh), `docs/reference/prior-art.md:407` (cùng bảng), `crates/session/tests/drop_reason.rs`
(dòng 4 header **và** dòng 169 doc của `every_pre_session_fault_names_itself`). Chỗ thứ sáu item
71 **không kể**: rustdoc của `DropReason` ở `crates/session/src/lib.rs:1089` — *"returns it from
**eighteen** places"* — là thứ người dùng thư viện đọc trên docs.rs.

Đếm lại `Link::Dropped` trong `crates/session/src/lib.rs`, ba cách, ba con số:

```
grep -c 'Link::Dropped'                                              29   (mọi dòng, kể cả comment)
… | grep -v '^\s*//'                                                 25   (bỏ comment)
grep -cE 'return (Ok\()?Link::Dropped|=> Link::Dropped|^\s*Link::Dropped\s*$'   23   (điểm trả về)
```

Bốn dòng là comment/rustdoc (1089, 2339, 2357, 2563); hai dòng là phép so sánh `link ==`/`!=`
(2385, 2401); còn lại **23 điểm trả về**. Con số 29 của item 71 là con số thứ nhất, đếm cả chữ
trong comment. Không con số nào trong ba là *eighteen*. `prior-art.md` sửa lần cuối `3c42635`
2026-09-10; hai ADR đều `Status: Accepted`.

**2. `scripts/check-links.py` (item 72), 153 dòng, chạy ở job `links` (`ci.yml:113-118`) và có hàng
§6 ở `DESIGN.md:869`.** `names_a_repo_file` (dòng 72-85) so đuôi URL từ phải sang trái, chấp nhận
đuôi **một đoạn** miễn `parts[-1]` có dấu chấm và file tồn tại ở gốc repo. Hai link sai thật nó đã
bắt (docstring dòng 13-24) đều là đuôi **nhiều đoạn** `docs/decisions/…`. Remote của repo:
`https://github.com/tmthang86/fixbolt.git`. Ngoại lệ duy nhất hôm nay: `crates/library/README.md`
(dòng 104-109) được phép dùng URL tuyệt đối vì `include_str!` lên docs.rs.

**3. `number()` (item 73) ở `settings.rs:812-819`, là `str::parse` nguyên bản, được gọi ở 8 chỗ:**
`HeartBtInt` (:1221), `MaxSkewMillis` (:1224), `LogonTimeout` (:1251), `LogoutTimeout` (:1255),
`TimestampPrecision` (:1287), `SocketConnectPort` (:1337), `ReconnectInterval` (:1341),
`ReconnectCeiling` (:1345). **Bảy** khoá đầu đi qua `number()` *trần*; khoá thứ tám đã có
`spelled_exactly_as_digits` (:832-836) kiểm **sau** `number()` (:1299), trả `UnsupportedPrecision`,
được canh bởi `crates/engine/tests/settings_roles.rs:344
timestamp_precision_is_refused_unless_spelled_exactly`. Thông điệp của `Problem::NotANumber` hôm
nay: `"expected a number"` (:378). Plan trước đã đo `rustc` 1.98.0: `"+3"` → `Ok(3)`, `"03"` →
`Ok(3)`, `"-3"` → `Err`.

**Cùng họ, chưa ai nêu:** `time_of_day` (:1371-1389) đọc `HH:MM:SS` bằng `h.len() == 2` rồi
`h.parse::<u32>()`. `"+1"` dài 2 và parse ra `1`, nên theo code **`StartTime=+1:00:00` hôm nay được
nhận là 01:00:00**. Đây là suy ra từ code cộng phép đo `"+3"` của hôm qua, chưa chạy — bước 3 xác
nhận bằng test đỏ-trước; nếu test *xanh* ngay thì câu này sai và phải ghi lại.

**4. Probe 3 (item 74) ở `settings.rs:2209-2403`.** Vũ trụ = 66 + 66² = 4 422 chuỗi dài 1–2 trên
`ALPHABET` (:2216) cộng 9 lân cận mỗi literal (:2223-2243); zero-guard `MIN_UNIVERSE = 4400`;
`FLOOR` 10 (11 với `tls`). Hai bộ đọc literal dạng `match` chuỗi: `flag` (:799, `match value {` ở
:800, arm `"Y"`/`"N"`) và `ConnectionType` (`let what = match value {` ở :944, arm `"acceptor"`,
`"initiator"`, nhánh `other =>` không có literal). Helper `arm_literals(opens)` (:1611-1645) đã đọc
arm của `Key::name`/`Key::parse` bằng cách tìm **một dòng trim bằng đúng** `opens` rồi lấy từ trong
ngoặc kép đầu tiên của mỗi dòng có `=>`; nó dừng ở dòng `}` có indent ≤ dòng mở — **không dừng ở
`};`**, là cách `match` của `ConnectionType` đóng. Plan trước đã đo 49 000 lượt parse hết 0,32 s.

**5. Bốn probe của `mod doc_table` (item 67) đọc ô theo tên cột** (`doc_rows`, :1714). Ô *Default*
bị probe 2 bỏ qua hôm nay là **15**, và đọc bảng §1 thấy chúng thuộc bốn dạng: `required` trần
(`BeginString`, `SenderCompID`), `required per [SESSION]` (`TargetCompID`), `required when
ConnectionType=initiator; refused otherwise` (`SocketConnectHost`, `SocketConnectPort`), `required
when SocketUseSSL=Y; refused otherwise` (`ServerCertificateFile`, `ServerCertificateKeyFile`), và
văn xuôi thật (`none…`, `all seven days`, `16 × ReconnectInterval`). **Bảy ô là một mệnh đề kiểm
được** — *"thiếu thì `MissingKey`, thừa sai vai thì từ chối"*. `Problem` có đúng các biến thể để
phân biệt: `MissingKey`, `WrongRole`, `NeedsFeature`, `NeedsTlsDoor`, `DefaultOnly`. `Sample`
(:1863) là hai chuỗi tĩnh `default_block` + `sessions`, nên *bỏ một dòng* là một phép thay chuỗi.
**`DESIGN.md` §6 không có hàng nào cho gate `doc_table`** — `grep doc_table docs/DESIGN.md` chỉ
trúng dòng 866 (đếm test của job `tls`).

**6. Item 21 còn mở đúng như hàng STATUS viết.** `serve_hft` (`lib.rs:2339`) nhận 7 tham số, không
có `CoreId`, không `cfg`; `serve_sharded_hft` (`shard.rs:442`) nhận `&ShardPlan` và ghim từng
thread. `ShardPlan::validate()` (`affinity.rs:568`) = `Topology::read()?.validate(self)`, từ chối
lõi không có / offline / trùng / anh em SMT / ngoài `isolcpus` (trừ `allow_unisolated()`).
`pin_current_thread` (:134) đọc mask lại; `running_on()` (:231) đọc `/proc/thread-self/stat`.
`ServeError` (`lib.rs:2484`) có `NoCounterparties`, `Io`, `LogPath`, và biến thể TLS — **không có
biến thể affinity**. `best-practices-hft.md:57` và `GUIDE.md:1474` đã viết thẳng *"`serve_hft` pins
nothing… thread is yours to pin"*; `DESIGN.md` D8 bảng dòng 452 vẫn viết cột `hft` là *"the polling
thread is pinned to an isolated core"* — không nói cửa nào. `clippy::too_many_arguments` trần 7;
ADR-0054 *Consequences* đã nhận `#[allow]` cho bốn hàm 8 tham số và nói điều kiện mở builder là
*tham số thứ mười một*. CI job `gates` (`ci.yml:289-293`) chạy `cargo test -p fixbolt-engine
--features affinity` — test ghim chạy trên runner GitHub, `crates/engine/tests/affinity.rs` chọn lõi
bằng `a_core_we_may_use()` (mask của chính thread) thay vì `CoreId(0)`.

**7. Item 55:** `scripts/check-indexing-debt.sh` hôm nay in `181 indexing/slicing sites in
crates/*/src, ceiling 181` / `ok`. 22 file mang `allow` có phạm vi; nặng nhất `crates/session/src/lib.rs`
(13 dòng allow) và `crates/codec/src/template.rs`.

### Tra cứu trên internet `[researched 2026-09-13]`

- **QuickFIX C++ `IntTConvertor::convert`**, `src/C++/FieldConvertors.h` trên `master`
  ([raw.githubusercontent.com/quickfix/quickfix/master/src/C++/FieldConvertors.h](https://raw.githubusercontent.com/quickfix/quickfix/master/src/C++/FieldConvertors.h)):
  chuỗi rỗng → `false`; chỉ xét dấu `-` (và từ chối nó cho kiểu không dấu); rồi vòng
  `const unsigned char c = *str - '0'; if (c > 9) return false;` cho **mọi** ký tự còn lại. Nên
  **`+30` bị từ chối** (`'+' - '0'` tràn thành 251), **`030` được nhận** (vòng lặp nhân 10 cộng
  dồn, không kiểm số 0 đầu). `Dictionary::getInt`
  ([…/src/C++/Dictionary.cpp](https://raw.githubusercontent.com/quickfix/quickfix/master/src/C++/Dictionary.cpp))
  gọi `IntConvertor::convert(getString(key))` và bọc lỗi thành `ConfigError("Illegal value … for
  …")`. Khớp với điều hàng STATUS 73 đoán: *từ chối `+`, nhận số 0 dẫn đầu*.
- **QuickFIX/J `Dictionary.getLong`**: hai lần fetch mã nguồn trên GitHub trả 404 (đường dẫn
  `quickfixj-core/src/main/java/quickfix/Dictionary.java` trên `master`); kết quả tìm kiếm
  ([javadoc 2.3.0](https://www.quickfixj.org/javadoc/2.3.0/quickfix/Dictionary.html), bản chép
  `SessionSettings.java` trên javatips) nói `getLong` dùng `Long.parseLong` và bọc
  `NumberFormatException` thành `FieldConvertError`. `Long.parseLong("+30")` là `30` theo Javadoc
  của `java.lang.Long` (dấu `+` được nhận từ Java 7) và `"030"` là `30`. **Không đo ở đây** — ghi là
  theo tài liệu. Kết luận dùng được: hai engine tham chiếu **bất đồng về `+`** và **đồng ý nhận số 0
  dẫn đầu**; không engine nào "đọc đúng như viết".
- **Gate tài liệu ↔ code ở dự án Rust khác.** `ruff` sinh trang *Settings* từ struct `Options` bằng
  `cargo dev generate-all` và CI chạy lại ở chế độ kiểm để phát hiện tài liệu lệch code
  ([docs.astral.sh/ruff/contributing](https://docs.astral.sh/ruff/contributing/)) — chiều **code →
  tài liệu**, tài liệu là sản phẩm sinh ra. `rust-lang/rust` giữ `bootstrap.example.toml` với mọi
  khoá ở dạng comment kèm giá trị mặc định
  ([github.com/rust-lang/rust/blob/main/bootstrap.example.toml](https://github.com/rust-lang/rust/blob/main/bootstrap.example.toml));
  tìm kiếm thấy file, **không thấy** test nào đọc nó. Không tìm thấy dự án nào làm chiều của
  `doc_table` — đọc ô bảng viết tay rồi *lái parser* bằng nó. Nghĩa là §C và §D dưới đây không có
  prior art để chép, chỉ có nguyên tắc sẵn của repo: một ô là *giá trị* thì probe được, là *văn xuôi*
  thì đếm `skipped` và in ra.
- **Link checker và URL tuyệt đối về nhà.** `lychee` dùng `--remap` — *"remaps are applied
  textually"*, một regex trên **toàn bộ URL** đổi thành `file://…` để kiểm file cục bộ
  ([lychee.cli.rs/recipes/local-folder](https://lychee.cli.rs/recipes/local-folder/)); **không có
  heuristic nào trên tên file trần**. Quy tắc (a) của §B — *URL bắt đầu bằng repo của mình thì là
  link về nhà* — là đúng cách lychee làm; quy tắc (b) — *đuôi nhiều đoạn khớp file trong repo, bất
  kể host* — là của riêng repo này và phải tự nói giới hạn.

## Cách làm

### A. Item 71 — con số sống ở **một** chỗ, có test canh; hai ADR nhận một dòng erratum

**Quyết định:** *eighteen* không còn đúng và không con số nào sẽ đúng lâu; thứ sống lâu là **định
nghĩa của phép đếm**. Con số được giữ ở **đúng một chỗ sống** — hàng `fixbolt` của bảng trong
`docs/reference/prior-art.md:407` — viết kèm *lệnh đếm* và ngày, và một test trong
`crates/session/tests/drop_reason.rs` đọc cả hai file bằng `include_str!` và so. Ba chỗ sống khác
**bỏ con số**, trỏ về bảng đó. Hai ADR Accepted **không sửa nội dung** (`CLAUDE.md` §5): mỗi ADR
nhận **một dòng** ngay dưới `Status`, dạng
`- **Erratum `[2026-09-13]`:** *eighteen* was the count on 2026-09-02; the living count and the command that produces it are in [prior-art.md](../reference/prior-art.md) — ADR body unchanged.`
Đây là câu hỏi Q1 cho chủ sở hữu.

Phương án bị loại: *(i)* ADR thay thế — một ADR để sửa một con số là tiếng ồn, và không có quyết định
nào bị đảo; *(ii)* sửa thẳng chữ *eighteen* trong ADR — §5 cấm; *(iii)* chỉ sửa tài liệu, không
test — đúng lỗi item 71 mô tả, lần thứ hai.

Việc làm, một bước (**sonnet**):

1. `docs/reference/prior-art.md:407`, ô cuối: `` **yes** — `DropReason`; `[measured 2026-09-13]` 23 return sites of `Link::Dropped` in `crates/session/src/lib.rs` (`grep -cE 'return (Ok\()?Link::Dropped|=> Link::Dropped|^\s*Link::Dropped\s*$'`), guarded by `drop_reason.rs::the_site_count_prior_art_quotes_is_the_one_in_the_source` ``. Developer **đếm lại** và điền số thật.
2. `crates/session/tests/drop_reason.rs`: header dòng 4 và doc dòng 169 bỏ *eighteen* (*"from every
   refusal path — the living count is in `docs/reference/prior-art.md`"*). Test mới
   `the_site_count_prior_art_quotes_is_the_one_in_the_source`: `include_str!("../src/lib.rs")` đếm
   dòng khớp ba dạng trên (không dùng regex crate — ba `str` check: `trim_start().starts_with("return
   Link::Dropped")` / `starts_with("return Ok(Link::Dropped)")` / `contains("=> Link::Dropped")` /
   `trim() == "Link::Dropped"`), `include_str!("../../../docs/reference/prior-art.md")` tìm hàng bắt
   đầu `| **fixbolt** |` và đọc số ngay trước `return sites`; `assert_eq!` với câu
   `prior-art.md quotes {doc} return sites of Link::Dropped; crates/session/src/lib.rs has {src} — update the table, and the date beside it`.
   Zero-guard: `src >= 10` (`the counter read nothing — the pattern no longer matches how lib.rs returns Link::Dropped`).
3. `crates/session/src/lib.rs:1089-1090`, **chỉ rustdoc**: *"`Link::Dropped` is one bit and this
   session returns it from every refusal path — a wrong `BeginString`…"*. Không đổi một dòng code.
4. Nếu Q1 = có: một dòng erratum trong `ADR-0035` (dưới dòng 3) và `ADR-0059` (dưới dòng 3). Dòng 102
   của ADR-0035 (*"eighteen different faults"*) **để nguyên** — là mô tả một phép đo ngày 2026-09-02
   về `disconnect()`, không phải con số sống.

### B. Item 72 — hai quy tắc thay một: *URL về nhà* theo tên repo, *đuôi trùng* phải nhiều đoạn

**Quyết định:** `names_a_repo_file` giữ cách so từ phải, thêm hai điều kiện:

- **(a)** URL có host+path bắt đầu bằng `github.com/tmthang86/fixbolt` (hằng `OWN_REPO` trong
  script, có docstring nói đổi tên repo thì đổi ở đây) → **mọi** đuôi khớp file đều bị báo, kể cả một
  đoạn. Đây là lychee `--remap` viết tay.
- **(b)** host khác → chỉ báo khi đuôi khớp có **ít nhất một dấu `/`** (hai đoạn trở lên). Một tên
  file trần (`CHANGELOG.md`, `README.md`, `LICENSE`) **không phải bằng chứng** file thuộc repo nào.
- Lớp bị bỏ qua được **đếm và in**: dòng tổng kết thêm `, N foreign URLs sharing only a filename
  with this repository (not judged)`. Skip-counted, như `doc_table`.

**Giới hạn nói thẳng** (docstring + `DESIGN.md` §6:869 + reference mới): một URL tuyệt đối tới file
**gốc** của repo này dưới **tổ chức sai** (`https://github.com/fixbolt/CHANGELOG.md`) nay đi qua.
Hai link sai thật đã bắt đều là `docs/decisions/…` — nhiều đoạn — nên vẫn bắt (R72-2 chứng minh).

Phương án bị loại: *(i)* tắt quy tắc host-agnostic — bỏ đúng thứ đã bắt hai link sai; *(ii)* chỉ
(b) không (a) — mất khả năng bắt `github.com/tmthang86/fixbolt/blob/main/CHANGELOG.md`, là link mà
một ngày đổi tên nhánh sẽ chết; *(iii)* fetch mạng — docstring dòng 10-11 đã nói vì sao không.

Việc làm, một bước (**sonnet**): `scripts/check-links.py` (hàm `names_a_repo_file` nhận thêm
`url` đầy đủ, hằng `OWN_REPO`, đếm `ignored_bare`), docstring đoạn 20-24 thêm hai câu; `DESIGN.md`
§6 dòng 869 cột *Proven by* thêm `[measured 2026-09-13]` hai quy tắc và giới hạn; reference mới
`docs/reference/a-bare-filename-is-not-evidence-of-a-repository.md` `[to testing-skills]` — phép đo
của item 72 (trích dẫn `cargo-deny`'s `CHANGELOG.md` bị đọc là của mình), hai quy tắc, giới hạn, và
câu tổng quát: *a matcher that compares from the right needs enough segments to name an owner; one
segment names a convention*.

### C. Item 73 — số nguyên **đọc đúng như viết**, một quy tắc ở một chỗ; `HH:MM:SS` cùng lúc

**Quyết định (Q2 cho chủ sở hữu, đề xuất (b)):** `number()` kiểm `spelled_exactly_as_digits`
**trước** `parse`, sai chính tả → `Problem::NotANumber`; thông điệp đổi thành
`"expected a number written as digits only — no sign, no leading zero"`. Một quy tắc, một chỗ, tám
khoá. Nhánh `TimestampPrecision` (:1287-1305) **đảo thứ tự**: kiểm chính tả trước rồi mới
`number()`, để `03` vẫn là `UnsupportedPrecision` như tài liệu và test hôm qua nói —
`timestamp_precision_is_refused_unless_spelled_exactly` **không đổi một chữ** và là control.
`time_of_day`: thêm `h.bytes().all(|b| b.is_ascii_digit())` cho ba phần trước `parse` (số 0 dẫn đầu
ở đây là *định dạng*, không phải lỗi) — `+1:00:00` → `BadTime`.

Vì sao (b) chứ không (a) "bằng QuickFIX C++" (từ chối `+`, nhận `030`): hai engine tham chiếu đã
bất đồng về `+`, nên "bằng QuickFIX" không phải một điểm mà là hai; `030` → 30 là nhầm lẫn kinh điển
với bát phân và không tài liệu nào của repo đưa ra cách viết đó; và `TimestampPrecision` đã ship quy
tắc *đọc như viết* hôm qua — (a) sẽ là hai quy tắc trong một file. (c) "để nguyên, ghi tài liệu" là
đúng lỗ item 73 mô tả.

Việc làm, **trong bước 3 cùng §D** (cùng file, một người viết, **sonnet**; senior review đọc kỹ):

1. `settings.rs`: `number()` như trên; thông điệp `:378`; nhánh `TimestampPrecision` đảo thứ tự;
   `time_of_day` thêm kiểm chữ số. **Không đụng** `Key`, `doc_rows`, `Sample`.
2. Test mới trong `crates/engine/tests/settings.rs` (nơi `NotANumber` đã có test):
   `an_integer_key_is_read_as_written` — bảng 7 khoá × (`+7` → `NotANumber` tại đúng dòng, `07` →
   `NotANumber`, `7` → *không phải* `NotANumber`, có thể là lỗi khác như `ImpossiblePolicy` cho
   `ReconnectCeiling`); `a_time_of_day_is_digits_only` — `+1:00:00`, `1+:00:00`, `01:+0:00` →
   `BadTime`; `01:00:00` → ok. **Chạy trước khi sửa parser, trích câu đỏ** (§10).
3. Probe 6 trong `mod doc_table`, `an_integer_values_cell_is_read_as_written`: hàng nào có ô
   *Values* chứa từ `integer` hoặc dạng `` `\d+`–`\d+` `` → với mẫu `sample(group(key))`, viết
   `{name}=+7`, `{name}=07` → `NotANumber`; `{name}=7` → không `NotANumber`. `FLOOR = 7` (developer
   đo; `SocketConnectPort` đi qua nhóm initiator). In `probe 6 — integer Values cells: N probed, M skipped`.
4. `docs/CONFIGURATION.md` §1, ngay sau đoạn *Validation is strict* (dòng 17-19): một đoạn
   `[measured 2026-09-13]`: *every integer value is read as written — ASCII digits, no `+`, no
   leading zero; `HeartBtInt=+30` and `SocketConnectPort=08080` are refused as `NotANumber`. QuickFIX
   C++ `IntConvertor` refuses `+` and accepts `030`; QuickFIX/J `Long.parseLong` accepts both. This
   engine is stricter than either, for the reason the `TimestampPrecision` note gives.* Ô *Values*
   của 7 hàng **không đổi** (đã ghi `integer`). `CHANGELOG.md` *Unreleased → Changed*: một dòng.

### D. Item 74 — chiều ngược có hai chân: tìm kiếm có biên **và** đọc arm của `match`

**Quyết định:** probe 3 giữ nguyên tìm kiếm có biên (nó bắt *parser lỏng*: `+3`, `03`), và thêm
**chân thứ hai đọc code** cho những khoá mà bộ đọc là một `match` trên literal chuỗi: tập arm **phải
bằng** tập literal tài liệu liệt kê. Chân này thấy `"acc"` ở mọi độ dài vì nó không *thử* gì cả — nó
*đọc*. Plan trước loại "đọc code" vì đứng **một mình** nó sai với `TimestampPrecision` (đi qua số);
đứng **cạnh** tìm kiếm thì mỗi chân che đúng chỗ chân kia mù. Vũ trụ tìm kiếm thêm **mọi chuỗi 3
chữ số** `000`–`999` (1 000 chuỗi, ~7 ms/hàng theo tốc độ đã đo) để chân số không còn phụ thuộc
lân cận.

Cơ chế: `enum Reader { Literals(&'static str), Numeric, Prose }` và `const fn reader(key: Key) ->
Reader` **khớp đủ, không `_`** — khoá mới phải khai báo. `Literals(opener)` với `opener` là dòng mở
`match` **duy nhất** trong file: `flag` → `"match value {"` **không duy nhất** (có ít nhất hai), nên
helper mới `literals_of_match(opener, inside_fn)` nhận thêm dòng mở hàm để khoanh vùng:
`("fn flag((line, value): (usize, &str), key: Key) -> Result<bool, SettingsError> {", "match value {")`
và `(None, "let what = match value {")`; dừng ở dòng trim **bắt đầu bằng** `}` có indent ≤ dòng mở
(để `};` cũng đóng). Assert đúng một dòng khớp mỗi opener.

Ba kết cục cũ giữ nguyên câu. Kết cục mới, một câu: `docs/CONFIGURATION.md §1: {name} lists
{listed:?} but the parser's match arms read {arms:?} — a literal of three or more characters is
outside the bounded search, and this is the leg that sees it`.

Việc làm, trong bước 3 (**sonnet**): `settings.rs` `mod doc_table` — `Reader`, `reader`,
`literals_of_match`, `candidates` thêm 3 chữ số, `MIN_UNIVERSE` → `5400`, doc của probe 3
(:2270-2306) viết lại đoạn *Reverse*; `STATUS.md` để manager.

### E. Item 67 — probe 5 cho ô *required*, rồi đóng bằng cách nói giới hạn ở nơi người ta đọc

**Quyết định (Q3 cho chủ sở hữu, đề xuất đóng):** thêm probe 5 cho bảy ô *Default* dạng `required…`
— thứ cuối cùng trong §1 *là một mệnh đề* mà chưa probe — rồi **đóng item 67**, với phần dư (*Meaning*,
mọi ô ghi chú) ghi là **giới hạn cố định** ở ba nơi người ta thật sự đọc: docstring của `mod
doc_table` (đã có, viết lại cho đủ sáu probe), **hàng §6 mới** trong `DESIGN.md` cho gate này (hôm
nay không có), và `docs/CONFIGURATION.md` §1 một câu *"six probes read this table; the Meaning and
notes cells are prose and are read by a person"*. Một item không bao giờ đóng được là một tuyên bố,
không phải việc — và `docs/reference/a-known-limitations-list-rots-in-one-direction.md` đã nói danh
sách giới hạn rỉ theo chiều nào.

Probe 5, `a_required_default_cell_is_what_the_parser_demands`, bốn dạng ô:

| Ô *Default* bắt đầu bằng | Phép thử | Kỳ vọng |
|---|---|---|
| `required` (trần) | bỏ dòng `{name}=` khỏi `default_block` của mẫu acceptor | `MissingKey` |
| `required per [SESSION]` | bỏ dòng khỏi `sessions` | `MissingKey` |
| `required when ConnectionType=initiator; refused otherwise` | bỏ khỏi mẫu initiator → ; thêm vào mẫu acceptor → | `MissingKey` ; `WrongRole` |
| `required when SocketUseSSL=Y; refused otherwise` | (với `tls`) bỏ khỏi mẫu tls → ; thêm vào mẫu acceptor không `SocketUseSSL` → | `MissingKey` ; một trong `{WrongRole, NeedsFeature, NeedsTlsDoor, DefaultOnly}` — developer **đo và ghi đúng biến thể** vào assertion, không để tập |

Helper `Sample::without(key_name) -> String` (xoá dòng bắt đầu `{name}=`; assert đã xoá đúng một
dòng — nếu không, probe đang thử một mẫu không chứa khoá và xanh vì lý do sai). `FLOOR`: developer
đo, kỳ vọng **5** không `tls`, **7** với `tls`. In `probe 5 — required Default cells: N probed, M skipped`.

### F. Item 21 — `serve_hft_pinned`: một cửa `hft` đơn engine **từ chối trước, ghim rồi, mới bind**

**Quyết định (Q4 cho chủ sở hữu, đề xuất làm trong plan này):** thêm **một** hàm
`serve_hft_pinned(pin: &affinity::CorePin, addr, table, app, capacity, limits, log, handles)` trong
`crates/engine/src/lib.rs` cạnh `serve_hft`, `#[cfg(all(feature = "affinity", target_os =
"linux"))]`, thứ tự bắt buộc: `pin.validate()?` → `affinity::pin_current_thread(pin.core())?`
(đọc mask lại, ADR-0015 quyết định 2) → `serve_hft_with::<256, 4096, 8192, 1024, …>(…)`. `CorePin`
là struct mới trong `affinity.rs`: `CorePin::to(core: CoreId)`, `.allow_unisolated()`,
`.core()`, `.validate()` = `ShardPlan::new(vec![core])` (+ allow) `.validate()` — **mọi quy tắc từ
chối là của `Topology::validate` sẵn có**, không viết lại quy tắc nào. `ServeError::Affinity(AffinityError)`
biến thể mới, rustdoc nói *its own variant for the reason `LogPath` is*. 8 tham số →
`#[allow(clippy::too_many_arguments)]` với comment trỏ ADR-0054 *Consequences* đúng cách bốn hàm
kia làm (đây là tham số thứ 8, chưa tới điều kiện *thứ mười một*). Không `serve_hft_pinned_with_recovery`:
ai cần recovery + ghim đã có `serve_sharded_hft` một shard.

Vì sao không đổi chữ ký `serve_hft`: `hft_wire.rs`, `best-practices-hft.md` và `GUIDE.md` đang mô
tả đúng cửa không ghim và nói rõ *"thread is yours to pin"* — đó là một lựa chọn hợp lệ (`taskset`
quanh process). D8 chỉ thiếu **cửa có từ chối**; thêm cửa, không bỏ cửa. Vì sao không nhận
`&ShardPlan`: một plan nhiều shard đưa vào cửa một engine là lỗi mà kiểu phải chặn, không phải
runtime.

Việc làm, một bước (**opus** — `engine`, public API, cửa vào hot path):

1. `crates/engine/src/affinity.rs`: `CorePin` (+ rustdoc, không `unsafe` mới — bất biến 8).
2. `crates/engine/src/lib.rs`: `ServeError::Affinity`; `serve_hft_pinned` với rustdoc nói thứ tự
   *validate → pin → bind* và vì sao (một socket đã bind rồi mới biết lõi sai là một cổng bị chiếm
   trong lúc operator đọc lỗi).
3. Test mới `crates/engine/tests/hft_pinned.rs`, `#![cfg(all(feature = "affinity", feature =
   "standard", target_os = "linux"))]` (cần poller pre-session như `hft_wire.rs:64` nói), chép khung
   `serve_hft_serves_a_session_and_stops` (`hft_wire.rs:178-245`):
   - `serve_hft_pinned_refuses_a_core_the_machine_does_not_have`: `CorePin::to(CoreId(4096))`,
     `addr = "127.0.0.1:1"` (bind sẽ lỗi nếu tới được) → `Err(ServeError::Affinity(AffinityError::NoSuchCore(CoreId(4096))))`,
     **không phải `Io`** — chứng minh thứ tự.
   - `serve_hft_pinned_refuses_a_core_outside_isolcpus_unless_told`: lõi online đầu tiên **không**
     trong `Topology::read()?.isolated()` (luôn tồn tại — `isolcpus` không bao giờ gồm mọi lõi),
     không `allow_unisolated` → `Err(Affinity(NotIsolated(core)))`.
   - `serve_hft_pinned_serves_on_the_core_it_was_given`: `a_core_we_may_use()` (chép từ
     `affinity.rs` test), `.allow_unisolated()`; trong callback của `Application` (chạy trên thread
     engine) ghi `affinity::running_on()` vào `Arc<Mutex<Option<CoreId>>>`; sau `Shutdown` assert bằng
     lõi đã ghim — **đọc từ `/proc/thread-self/stat`**, không từ giá trị trả về (ADR-0015 quyết định 2).
     Chạy trên thread spawn riêng, vì ghim thread harness làm hỏng test khác (`affinity.rs:9-12`).
4. Tài liệu: `DESIGN.md` §4 D8 bảng dòng 452 cột `hft`: *"`serve_sharded_hft` and `serve_hft_pinned`
   validate the core and pin from inside; `serve_hft` pins nothing and says so"*; §3 dòng 120 hàng
   `affinity` thêm `CorePin`; `best-practices-hft.md:57-59` bullet thêm câu về cửa mới; `GUIDE.md:1474`
   cùng câu; `CHANGELOG.md` *Unreleased → Added*; rustdoc.

### G. Item 55 — **không** ở plan này, và vì sao

Item 55 **đã có gate** (`check-indexing-debt.sh`, trần 181, đỏ hai chiều) — nó không thuộc họ *nghĩa
vụ không ai kiểm*; nó là nợ có sổ. Trả nợ nghĩa là sửa `crates/session/src/lib.rs` và
`crates/codec/src/template.rs` — hai file hot path, đụng bất biến 1 (alloc) và 3 (59/59), và §1
`CLAUDE.md` gọi đó là *codec change / session-layer change* → **plan riêng**, có `benches/alloc.rs`
và corpus chạy trước-sau. Trộn vào đây là trộn một PR đọc tài liệu với một PR đổi hot path, và senior
review sẽ phải đọc hai thứ bằng một con mắt. Đề xuất (Q5): item 55 giữ là **ratchet đứng**, đóng
theo cách chính script viết (*"when only sites like those two are left, the ceiling stops moving and
item 55 closes by saying so"*), và một plan *indexing-debt wave* riêng cho `session` + `codec` khi
chủ sở hữu muốn.

### Câu hỏi cho chủ sở hữu

| # | Câu hỏi | Đề xuất | Nếu chọn khác |
|---|---|---|---|
| Q1 | Item 71: thêm **một dòng erratum** dưới `Status` của ADR-0035 và ADR-0059 (không đụng thân)? | **Có.** §5 cấm sửa *nội dung*; một dòng ghi ngày, nói con số đúng lúc viết và trỏ chỗ sống, là chú thích biên tập — ADR-0061 đã nhận thêm một bullet *Consequences* trong ngày theo cùng tinh thần | Không → hai ADR giữ nguyên chữ *eighteen*; test canh `prior-art.md` vẫn làm; reference mới ghi rằng ADR là bản ghi ngày 2026-09-02 |
| Q2 | Item 73: (a) bằng QuickFIX C++ — từ chối `+`, **nhận** `030`; (b) đọc đúng như viết — từ chối cả hai | **(b).** Một quy tắc đã ship cho `TimestampPrecision`; hai engine tham chiếu bất đồng nên (a) không phải một điểm; `030` là bẫy bát phân | (a) → `spelled_exactly_as_digits` không vào `number()`; `number()` chỉ kiểm `+`; bỏ phép thử `07` khỏi probe 6 và test |
| Q3 | Item 67: **đóng** sau probe 5 (giới hạn *Meaning*/ghi chú ghi ở docstring, §6, CONFIGURATION.md), hay để mở vô hạn? | **Đóng.** Phần còn lại không có phép kiểm nào không biến tài liệu thành fixture toàn phần; một item không thể đóng là tuyên bố, không phải việc | Mở → vẫn làm probe 5; hàng STATUS thu hẹp thêm một lần |
| Q4 | Item 21: làm `serve_hft_pinned` **trong plan này** (một bước opus, chạy trên CI) hay tách plan riêng? | **Trong plan này.** Không cần máy §9 — test ghim đã chạy trên runner GitHub từ 2026-08-31; ~150 dòng; D8 sai từ 2026-08-30 | Tách → bước 5 bỏ; STATUS 21 giữ mở; D8 dòng 452 vẫn phải sửa chữ trong plan này để không nói sai |
| Q5 | Item 55: giữ là ratchet đứng, plan riêng cho `session`/`codec`? | **Giữ.** Lý do ở §G | Gộp → plan này phải thêm bước opus đụng hot path và gate alloc + 59/59; phạm vi đổi loại |

## Bất biến bị đụng tới

| # | Bất biến | Bị đụng bởi | Giữ bằng cách |
|---|---|---|---|
| 1 | không heap alloc trên hot path | §F: `CorePin::validate` và `pin_current_thread` chạy **trước** `serve_hft_with`, một lần, trước socket đầu tiên | không đụng `Engine::turn`; `scripts/bench.sh` dòng `alloc` đọc `0` — control, không phải bằng chứng mới |
| 2 | session thuần | §A chỉ sửa **rustdoc** của `crates/session/src/lib.rs` | `git diff --stat` của file này chỉ có dòng `///`; senior review kiểm |
| 3 | 59 định nghĩa | không đụng | `cargo test -p fixbolt-session --test score` 59/59 chạy ở bước đóng — control |
| 4 | `hft` không ngủ trong kernel | §F: `sched_setaffinity` + đọc `/proc` là syscall **trước khi phục vụ**, không phải trên đường nóng | rustdoc nói rõ thứ tự; `check-no-kernel-sleep.sh` vẫn trace `tools/w2w`, không phải cửa này — **không chứng minh gì thêm về cửa này**, nói thẳng trong CONFORMANCE nếu senior review hỏi |
| 7 | không `panic`/`unwrap`/`expect` trong lib | §C, §D, §E, §F là code lib trong `engine` | `cargo clippy --all-targets -- -D warnings`; `scripts/check-indexing-debt.sh` vẫn `181` (không `[i]` mới — `CorePin` không index gì) |
| 8 | `unsafe` có plan | §F **không thêm** `unsafe`; hai block có sẵn trong `affinity.rs` là của ADR-0019 | `grep -c unsafe crates/engine/src/affinity.rs` trước và sau bằng nhau, trích |
| 10 | không số hiệu năng thiếu bench/máy/§9 | §D ghi thời gian test `doc_table` | ghi là *thời gian test debug trên bàn*, chỉ vào nhật ký |
| — | §A, §B không đụng `codec`/`session` (ngoài rustdoc)/`engine`/`transport`; §C–§F đụng `engine` ở đường cấu hình và cửa vào | | senior review cuối PR đọc §C–§F |

Bất biến 5, 6, 9: không đụng. (6: `serve_hft_pinned` nằm sau `cfg` cùng tổ hợp với `mod affinity`,
và `--no-default-features` vẫn build — job `no-default-features` là gate.)

## Chia việc

| Bước | Mục | Kết quả | Được đụng | Không được đụng | Gate đóng bước | Tier | Song song |
|---|---|---|---|---|---|---|---|
| 1 | §A item 71 | con số ở một chỗ + test canh; ba chỗ bỏ số; (Q1) hai dòng erratum | `docs/reference/prior-art.md` (hàng 407); `crates/session/tests/drop_reason.rs`; `crates/session/src/lib.rs` **rustdoc :1089-1090 only**; `docs/decisions/ADR-0035…md`, `ADR-0059…md` (một dòng mỗi file, chỉ nếu Q1 = có) | mọi dòng code của `session`; `STATUS.md` | `cargo test -p fixbolt-session --test drop_reason` xanh, trích tên test mới; R71-1, R71-2; `git diff --stat crates/session/src/lib.rs` chỉ rustdoc | sonnet | có — worktree riêng, cùng 3 và 5 |
| 3 | §C item 73 + §D item 74 | `number()` đọc như viết; `time_of_day` chữ số; probe 6; probe 3 hai chân; CONFIGURATION.md §1 đoạn mới | `crates/engine/src/settings.rs` (`number`, `:378`, nhánh `TimestampPrecision`, `time_of_day`, `mod doc_table`); `crates/engine/tests/settings.rs` (test mới); `docs/CONFIGURATION.md` §1 (một đoạn sau dòng 19); `CHANGELOG.md` *Changed* | `Key`, `doc_rows`, `Sample`; `crates/codec`; `settings_roles.rs` (control, không sửa); `STATUS.md` | test đỏ-trước trích; `cargo test -p fixbolt-engine --test settings`; `cargo test -p fixbolt-engine --lib doc_table` và `--features tls` xanh, trích ba dòng `probe 3/6`; `cargo clippy --all-targets -- -D warnings`; R73-1..3, R74-1..3 | sonnet | có — worktree riêng |
| 5 | §F item 21 | `CorePin`, `ServeError::Affinity`, `serve_hft_pinned`, ba test, D8/§3/best-practices/GUIDE/CHANGELOG | `crates/engine/src/affinity.rs`; `crates/engine/src/lib.rs` (enum + hàm mới + `#[allow]`); `crates/engine/tests/hft_pinned.rs` (mới); `docs/DESIGN.md` (**chỉ** dòng 120 và 452); `docs/best-practices-hft.md:57-59`; `docs/GUIDE.md:1474`; `CHANGELOG.md` *Added* | `serve_hft*` có sẵn; `shard.rs`; `hft_wire.rs`; `STATUS.md` | `cargo test -p fixbolt-engine --features affinity --test hft_pinned` 3/3; `cargo test -p fixbolt-engine --features affinity`; `cargo clippy --all-targets --features affinity -- -D warnings`; `cargo build -p fixbolt-engine --no-default-features`; R21-1..3; `grep -c unsafe affinity.rs` không đổi | **opus** | có — worktree riêng |
| 4 | §E item 67 | probe 5; `Sample::without`; docstring `mod doc_table` sáu probe; hàng §6 mới; câu trong CONFIGURATION.md §1 | `crates/engine/src/settings.rs` (`mod doc_table` only); `docs/DESIGN.md` §6 (một hàng mới, cuối bảng gate đầu); `docs/CONFIGURATION.md` §1 (một câu) | `number`, `settle`, mọi thứ ngoài `mod doc_table`; `STATUS.md` | `cargo test -p fixbolt-engine --lib doc_table` và `--features tls` xanh, trích dòng `probe 5`; R67-1, R67-2; thời gian `test result:` | sonnet | **sau 3 và 5** (cùng `settings.rs` với 3, cùng `DESIGN.md` với 5) |
| 2 | §B item 72 | hai quy tắc, đếm lớp bỏ qua, docstring, §6:869, reference mới | `scripts/check-links.py`; `docs/DESIGN.md` (**chỉ** hàng 869); `docs/reference/a-bare-filename-is-not-evidence-of-a-repository.md` (mới) | `crates/library/README.md` và ngoại lệ của nó; `STATUS.md` | `scripts/check-links.py` in dòng tổng kết mới và `no dead internal links`; R72-1..4 (+ control); `python3 -m py_compile` | sonnet | **sau 4** (cùng `DESIGN.md`) |
| 6 | toàn PR | senior review theo §12 bước 2: plan + gate, không kèm lý luận của manager; đọc kỹ §C (parser, 8 khoá), §D (`literals_of_match` khoanh vùng đúng không), §F (thứ tự validate→pin→bind, không `unsafe` mới) | tuỳ finding, cùng branch | thiết kế (về architect qua manager) | mọi gate bước 1-5 chạy lại trên commit sau sửa | opus | sau 1-5 |
| 7 | đóng | `STATUS.md` 67/71/72/73/74/21 đóng, 55 ghi quyết định Q5, *Not proven* rà, nhật ký, CI run id | `STATUS.md`; *Nhật ký giao hàng* của plan này | — | `cargo test --all`; `cargo test --all --no-default-features`; `scripts/check-links.py`; `scripts/check-indexing-debt.sh`; CI xanh **theo id** cho commit đóng | manager | cuối |

Ba file có nhiều ứng viên viết: `crates/engine/src/settings.rs` — bước 3 rồi 4; `docs/DESIGN.md` —
bước 5 (dòng 120, 452), rồi 4 (hàng §6 mới), rồi 2 (hàng 869); `CHANGELOG.md` — bước 3 (*Changed*)
và 5 (*Added*) ở hai worktree, hai mục khác nhau, manager merge. `docs/CONFIGURATION.md` — bước 3
(đoạn số nguyên) rồi 4 (câu về sáu probe). `STATUS.md` chỉ manager.

## Cách kiểm chứng

**Gate của cả plan**, trên commit đóng: `cargo fmt --check`; `cargo clippy --all-targets --
-D warnings`; `cargo clippy --all-targets --features affinity -- -D warnings`; `cargo test --all`;
`cargo test --all --no-default-features`; `cargo test -p fixbolt-engine --features affinity`;
`cargo test -p fixbolt-engine --lib doc_table --features tls`; `scripts/check-links.py`;
`scripts/check-indexing-debt.sh`; `scripts/check-no-crate-root-allow.sh`; và CI xanh theo run id.
Output trích nguyên văn, đọc nội dung không đọc exit code.

**Reversal — mỗi cái ghi sẵn: sửa gì, câu đỏ nào, rồi khôi phục thấy xanh.** Kiểm `git diff` có hunk
trước khi tin đỏ (hai reversal hôm 2026-09-12 là no-op).

| Mã | Sửa tạm | Lệnh | Câu đỏ phải thấy | Khôi phục |
|---|---|---|---|---|
| R71-1 | `prior-art.md:407`: số `23` → `24` | `cargo test -p fixbolt-session --test drop_reason the_site_count` | `prior-art.md quotes 24 return sites of Link::Dropped; crates/session/src/lib.rs has 23 — update the table, and the date beside it` | `git checkout docs/reference/prior-art.md` |
| R71-2 | trong test, pattern `return Link::Dropped` → `return Link::Droppedx` | như trên | zero-guard: `the counter read nothing — the pattern no longer matches…` (số đọc được < 10) | khôi phục |
| R72-1 | tạo `docs/zz-probe.md` chứa một link markdown (nhãn `x`) trỏ tới `https://github.com/EmbarkStudios/cargo-deny/blob/main/CHANGELOG.md` | `scripts/check-links.py` | **không đỏ**; dòng tổng kết đếm `1 foreign URLs sharing only a filename…` — đây là lỗ item 72 đóng, và lớp bỏ qua được nhìn thấy | xoá file |
| R72-2 | cùng file, link tới `https://github.com/fixbolt/docs/decisions/ADR-0001-relationship-to-quickfix.md` | như trên | `FAIL: 1 link(s) name a repository file by absolute URL` … `this repository has docs/decisions/ADR-0001-relationship-to-quickfix.md` — hai link sai thật vẫn bị bắt | xoá file |
| R72-3 | cùng file, link tới `https://github.com/tmthang86/fixbolt/blob/main/CHANGELOG.md` | như trên | `FAIL` … `this repository has CHANGELOG.md` — quy tắc (a) bắt file gốc của **chính mình** | xoá file |
| R72-4 | **control, giới hạn đã nói**: link tới `https://github.com/fixbolt/CHANGELOG.md` | như trên | **không đỏ**, đếm vào lớp bỏ qua — chính là giới hạn docstring và §6 ghi; nếu đỏ, quy tắc (b) sai | xoá file |

`[measured 2026-09-13]` **bốn URL trên, khi bản nháp đầu của plan này viết chúng bằng cú pháp link
markdown thật, làm `scripts/check-links.py` đỏ cả bốn** trên chính file này — kể cả R72-1 và
R72-4, hai cái mà quy tắc mới phải cho qua. Đó là item 72 tái hiện trên một file tài liệu, không cần
dựng gì; và nó nói thêm một điều về gate: **nó đọc cả trong backtick và trong bảng**, nên một plan
muốn *trích dẫn* một URL làm ví dụ phải viết URL trần, không viết thành link. Bốn hàng trên đã được
viết lại như vậy; sau bước 2, chạy lại trên plan này phải đọc `2 absolute URLs…` → **0**, vì bốn URL
không còn là link. Developer bước 2 dùng `docs/zz-probe.md` đúng như bảng nói, không dùng file này.
| R73-1 | **là bước đỏ-trước**: test mới chạy trên parser cũ | `cargo test -p fixbolt-engine --test settings an_integer_key_is_read_as_written a_time_of_day_is_digits_only` | đỏ: `HeartBtInt=+7` được nhận (`Ok`), `StartTime=+1:00:00` được nhận — **trích**; nếu `a_time_of_day_is_digits_only` xanh ngay thì suy luận mục 3 sai, ghi lại | sửa parser → xanh |
| R73-2 | bỏ `spelled_exactly_as_digits` khỏi `number()` | `cargo test -p fixbolt-engine --lib doc_table` | probe 6: `… {name}=+7 was accepted; an integer value is read as written` cho 7 hàng; **và** `settings_roles.rs::timestamp_precision_is_refused_unless_spelled_exactly` vẫn **xanh** (kiểm chính tả của nó đứng trước) — control | khôi phục |
| R73-3 | ô *Values* của `HeartBtInt` đổi `positive integer` → `positive number` | như trên | probe 6 `probed` giảm 7 → 6 dưới `FLOOR` — `probe 6 reached 6 rows, below its floor of 7` | `git checkout docs/CONFIGURATION.md` |
| R74-1 | thêm arm `"acc" => ConnectionType::Acceptor,` ở `:945` (**đúng phép đo item 74 từng xanh**) | `cargo test -p fixbolt-engine --lib doc_table` | `ConnectionType lists ["acceptor", "initiator"] but the parser's match arms read ["acc", "acceptor", "initiator"] — a literal of three or more characters is outside the bounded search, and this is the leg that sees it` | bỏ arm |
| R74-2 | trong `flag`, thêm `"yes" => Ok(true),` | như trên | cùng câu cho hàng `` `Y` or `N` `` đầu tiên (`ResetOnLogon`) với `arms` chứa `yes` | bỏ arm |
| R74-3 | `reader(Key::ConnectionType)` → `Prose` | như trên | **xanh** — và đó là lỗ: một khoá khai sai `Reader` là khoá không được đọc. Vì thế thêm assertion: mọi hàng `enumerated` phải có `reader` ≠ `Prose`, câu `{name} lists literals but declares Reader::Prose — say which match reads it`; sau assertion này R74-3 đỏ đúng câu | khôi phục |
| R67-1 | `settle`: bỏ kiểm `MissingKey` cho `SenderCompID` (tạm cho mặc định rỗng) | như trên | probe 5: `docs/CONFIGURATION.md §1: SenderCompID says required but a file without it parses` | khôi phục |
| R67-2 | `CONFIGURATION.md`: ô *Default* của `SocketConnectHost` bỏ `; refused otherwise` | như trên | `probed` giảm dưới `FLOOR` **hoặc** — tốt hơn — probe đọc ô theo hai nửa và nửa `refused otherwise` là một phép thử riêng; developer chọn cách đọc và câu đỏ phải nêu tên khoá | `git checkout` |
| R21-1 | `serve_hft_pinned`: đảo `bind` lên trước `validate` | `cargo test -p fixbolt-engine --features affinity --test hft_pinned refuses_a_core_the_machine_does_not_have` | đỏ: nhận `Err(Io(…))` thay vì `Err(Affinity(NoSuchCore(CoreId(4096))))` — thứ tự là thứ test canh | khôi phục |
| R21-2 | bỏ lời gọi `pin_current_thread` (giữ validate) | `… serves_on_the_core_it_was_given` | đỏ tại assertion `running_on`: `left: Some(CoreId(k)), right: Some(CoreId(j))` với k ≠ j **ít nhất một lần trong 3 lần chạy** — thread không ghim *có thể* tình cờ ở đúng lõi; chạy 3 lần, trích lần đỏ; nếu 3 lần đều xanh trên máy có ≥ 4 lõi, báo | khôi phục |
| R21-3 | `CorePin::validate` bỏ `allow_unisolated` (luôn đòi isolcpus) | `… refuses_a_core_outside_isolcpus_unless_told` **vẫn xanh**, nhưng `serves_on_the_core_it_was_given` **đỏ** với `Affinity(NotIsolated(_))` — chứng minh hai test nhìn hai nửa khác nhau của một quy tắc | khôi phục |

Cho bước 3 và 4, developer trích thêm các dòng `probe N — …: X probed, Y skipped` và `test result:`
với thời gian; trần **10 s** debug cho `doc_table` giữ như plan trước (ước lượng thêm ~0,1 s).

## Tài liệu phải cập nhật

Theo bảng §4 của `CLAUDE.md`, đi từng hàng:

- [ ] **Public API của crate** (§F): rustdoc `CorePin`, `ServeError::Affinity`, `serve_hft_pinned`;
      `docs/DESIGN.md` §3 dòng 120; `CHANGELOG.md` *Unreleased → Added*.
- [ ] **Ràng buộc người dùng phải giữ mà compiler không kiểm** (§F): `docs/GUIDE.md:1474` — cửa nào
      ghim, cửa nào không.
- [ ] **Hằng số / khoá cấu hình người dùng thấy** (§C): `docs/CONFIGURATION.md` §1 đoạn *read as
      written*; `CHANGELOG.md` *Changed* — `+30`/`030`/`+1:00:00` nay bị từ chối.
- [ ] **Hành vi biên của session layer**: không đổi — §A chỉ sửa rustdoc.
- [ ] **Gate: mục tiêu hoặc cách đo** (§B, §E): `docs/DESIGN.md` §6 dòng 869 (`check-links.py` hai
      quy tắc + giới hạn); **hàng §6 mới** cho `mod doc_table` — *`docs/CONFIGURATION.md` §1 says
      what the parser does* | *six probes; Meaning and notes cells are prose, read by a person* |
      `cargo test -p fixbolt-engine --lib doc_table`, và với `--features tls`.
- [ ] **Codec/session/dispatch/transport behaviour**: không đổi.
- [ ] **Hardware/OS / §9**: không đổi. `hft-playbook.md` không đụng — `taskset` và `pin_current_thread`
      đã có ở đó; nếu senior review muốn một câu về `serve_hft_pinned`, là finding.
- [ ] **Bẫy / giả định sai / bất ngờ đã đo** (ưu tiên cao nhất): mới
      `docs/reference/a-bare-filename-is-not-evidence-of-a-repository.md` `[to testing-skills]` (§B).
      Item 71 **không** cần reference riêng: `a-known-limitations-list-rots-in-one-direction.md` đã
      là bài học đó — bước 1 thêm **một đoạn** `[measured 2026-09-13]` vào file ấy (con số *eighteen*
      ở sáu chỗ, 23 thật, cách đóng: một chỗ sống + test). `+1:00:00` nếu xác nhận đỏ: một đoạn trong
      `docs/CONFIGURATION.md` là đủ — đó là hành vi, không phải bẫy kiểm thử.
- [ ] **Chọn dependency / đổi kỹ thuật / đảo quyết định**: **không ADR mới.** §F là cửa thêm theo
      ADR-0015 (caller nêu lõi) và ADR-0054 (`#[allow]` ghi chứ không im); §C là quy tắc đã có cho một
      khoá mở rộng cho tám — rẻ để đảo, không tranh cãi, ghi ở CONFIGURATION.md. Nếu chủ sở hữu chọn
      Q2 = (a), vẫn không cần ADR.
- [ ] **`CLAUDE.md` §2**: không đổi rule nào. (`check-links.py` không nằm trong §2.)
- [ ] **`STATUS.md`**: 67, 71, 72, 73, 74, 21 đóng với ngày và bằng chứng; 55 ghi Q5; hàng 2640 (bảng
      cũ của item 21) gạch; mục *Not proven* (dòng 3682) — `grep` hôm nay không bullet nào nhắc sáu
      item này; ghi rõ đã rà.
- [ ] **`docs/CONFORMANCE.md`**: không đổi số; dòng 369-376 về `serve_hft` và `check-no-kernel-sleep`
      **vẫn đúng** cho cửa mới (script trace `w2w`, không trace cửa nào) — thêm nửa câu tên
      `serve_hft_pinned` vào dòng 371 nếu bước 5 thấy cần, không bắt buộc.
- [ ] Không thêm/bớt crate; không đụng `README.md`, `PRD.md`, `best-practices-standard.md`,
      `SESSION-BEHAVIOUR.md`.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Bước 1 đếm bằng regex khác bước 1 ghi vào tài liệu → con số "23" và test nhìn hai phép đếm khác nhau | test **là** phép đếm; tài liệu ghi lệnh `grep` tương đương; R71-1 đỏ đúng hai số |
| `include_str!("../../../docs/reference/prior-art.md")` làm `cargo publish` của `fixbolt-session` hỏng (file ngoài crate) | chỉ trong `tests/`, không trong `src/`; `cargo package -p fixbolt-session --list` không liệt kê test — developer chạy và trích, hoặc nếu liệt kê thì đọc bằng `std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), …))` |
| `check-links.py` quy tắc (a) khớp `github.com/tmthang86/fixbolt-other/…` vì so tiền tố chuỗi | so theo **đoạn**: `parts[:3] == ["github.com","tmthang86","fixbolt"]`; R72 thêm một link `…/fixbolt-other/CHANGELOG.md` không đỏ (control phụ, developer trích) |
| Sửa `number()` đổi luôn lỗi của `TimestampPrecision=03` từ `UnsupportedPrecision` thành `NotANumber` → test hôm qua đỏ và bị "sửa cho xanh" | thứ tự trong nhánh đảo **trước** khi chạy; `settings_roles.rs` **cấm đụng**; R73-2 nói rõ test đó phải xanh |
| `literals_of_match` đọc nhầm `match` khác cùng tên dòng mở | assert **đúng một** dòng khớp (và trong hàm được nêu); nếu 0 hoặc ≥ 2 → FAIL nêu số lần |
| `Reader::Prose` khai sai làm một khoá liệt kê không được đọc (R74-3 xanh) | assertion *enumerated ⇒ không Prose*; R74-3 đỏ sau đó |
| Vũ trụ thêm 3 chữ số `000`–`999` chạm `0…` là lân cận đã có → trùng | `BTreeSet` khử trùng; `MIN_UNIVERSE` 5400 đếm **sau** khử; R70-4 (bảng chữ rỗng) của plan trước vẫn đỏ |
| Probe 5 xanh vì `Sample::without` không xoá gì (tên khoá không có trong mẫu) | `without` assert xoá đúng một dòng; nếu 0 → FAIL `sample does not contain {name}; the probe would test nothing` |
| Probe 5 với `tls` đọc biến thể khác dự đoán (`NeedsFeature` vs `NeedsTlsDoor`) | developer **đo** rồi ghi đúng biến thể vào assertion; không dùng tập "một trong" trong code cuối |
| Ghim thread harness trong `hft_pinned.rs` làm test khác cùng binary bị hẹp lõi | mỗi test spawn thread riêng (`affinity.rs:9-12`); file test riêng, không chung binary với `hft_wire` |
| R21-2 xanh vì thread không ghim tình cờ ở đúng lõi | chạy 3 lần; trích lần đỏ; nếu không đỏ → báo, không tuyên bố |
| `a_core_we_may_use()` trả lõi trong `isolcpus` trên bàn §9 → test *outside isolcpus* không tìm được lõi | test chọn lõi **đầu tiên online không isolated** từ `Topology::read()`, không dùng `a_core_we_may_use()` |
| CI `no-default-features` đỏ vì `serve_hft_pinned` hoặc `CorePin` lộ ra ngoài `cfg` | `cfg(all(feature = "affinity", target_os = "linux"))` trên **từng** item mới; job `no-default-features` là gate; `scripts/check-no-optional-deps.sh` |
| Ba worktree cùng sửa `CHANGELOG.md`/`DESIGN.md` → merge conflict | hunks ở mục khác nhau; manager merge và chạy lại gate của cả ba trên commit merge |
| Test `doc_table` phình thời gian | trần 10 s debug; quá → báo, không thu nhỏ |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Chủ sở hữu chọn Q2 = (a): `030` vẫn được nhận và tài liệu phải nói *"leading zeros are read as decimal, as QuickFIX does"* | thấp | §C có sẵn nhánh (a) trong bảng câu hỏi; probe 6 bỏ phép thử `07` |
| `+1:00:00` không được nhận hôm nay (suy luận mục 3 sai, ví dụ có kiểm khác tôi không thấy) | thấp | R73-1 là test đỏ-trước; nếu xanh, bỏ sửa `time_of_day`, ghi lại trong nhật ký — không phải lỗi của plan, là phép đo |
| Probe 5 thấy một ô *required* **sai** hôm nay (ví dụ `TargetCompID` thiếu không ra `MissingKey`) | trung bình, **là mục đích** | đó là finding thật về parser hoặc tài liệu; báo manager, sửa bên sai, ghi nhật ký; không sửa probe |
| Runner GitHub `Topology::read()` lỗi vì `/sys/devices/system/cpu/isolated` không có | thấp | `shard_wire.rs` đã chạy trên runner từ 2026-08-31 (run `33394373832`); `parse_cpu_list("")` trả rỗng theo thiết kế |
| `include_str!` một file ngoài crate bị `cargo package` hoặc `publish` từ chối | thấp | bẫy trên; fallback `CARGO_MANIFEST_DIR` |
| Hai quy tắc link mới để lọt một URL tuyệt đối sai nào đó hôm nay đang có trong cây | thấp | bước 2 chạy script trên cây: dòng tổng kết phải đọc `0 absolute URLs naming…` **và** con số lớp bỏ qua được trích — nếu > 1, developer liệt kê từng URL để manager đọc |
| Thời gian: năm bước xây, ba song song, một senior review | — | 5 (opus) dài nhất, chạy đầu; 4 và 2 ngắn, chạy cuối |

## Ngoài phạm vi

- **Item 55** — §G, Q5. Ratchet đứng; plan riêng cho `session`/`codec`.
- **Item 40, 49, 51, 52** — cần máy §9 và reboot. Plan khác.
- **`serve_hft_pinned_with_recovery` và các biến thể `_with`** — một cửa là đủ để D8 đúng; ai cần
  thêm dùng `serve_sharded_hft` một shard. Nếu có người xin, đó là tham số thứ 9–10 và ADR-0054 đã
  nói điều kiện của builder.
- **Probe cho ô *Meaning* và ghi chú** — là văn xuôi; Q3 nói vì sao không.
- **Chiều ngược cho *Values* văn xuôi** (`StartDay`, `Weekdays`, `HH:MM:SS`) — vẫn skip-counted;
  `time_of_day` được sửa ở §C vì **hành vi**, không vì probe.
- **Anchor `#section` trong `check-links.py`** — docstring dòng 26-28 đã nói vì sao không.
- **Fetch mạng trong link checker** — không, cùng lý do cũ.
- **Dọn `Link::Dropped` về ít điểm trả về hơn** — không; con số không phải vấn đề, việc nó *không ai
  đọc lại* mới là.

## Nhật ký giao hàng

### 2026-09-13 — bắt đầu

Nhánh `plan/the-residue-of-an-obligation` từ `main` `949401c` (sau PR #66 và #67). **Draft PR mở
ngay ở commit này**, vì từ PR #67 CI chỉ chạy trên `pull_request` — một nhánh chưa có PR là nhánh
không có CI.

Bước 1, 3, 5 chạy song song, **mỗi bước một worktree do manager tự dựng** trước khi giao
(`.claude/worktrees/step{1,3,5}`, `vendor/` symlink) — lý do chia là *reversal của bước này làm đỏ
gate của bước kia*, không chỉ là trùng file. Bước 4 rồi 2 chạy sau, trên nhánh đã gộp 1+3+5.

### 2026-09-13 — bước 1 XONG (item 71)

Con số sống ở **một** chỗ: hàng `fixbolt` của `docs/reference/prior-art.md:407` ghi **23** điểm trả
về `Link::Dropped`, kèm lệnh `grep` và tên test. Test
`the_site_count_prior_art_quotes_is_the_one_in_the_source` đếm lại từ source. Ba chỗ khác bỏ số;
`crates/session/src/lib.rs` chỉ đổi rustdoc (manager kiểm: mọi dòng đổi đều là `///`). ADR-0035 và
ADR-0059 mỗi file một dòng erratum dưới `Status`, thân không đổi (Q1).

**Ba điều plan viết sai, đo mới thấy:**

1. **`starts_with("return Link::Dropped")` đếm ra 21, không phải 23.** Hai điểm trả về là arm của
   `match` (`… => return Link::Dropped,`), `return` không đứng đầu dòng. Developer đổi sang
   `contains` — khớp `grep` 23, và là cách duy nhất để R71-1 đỏ đúng câu plan dự đoán. Manager kiểm
   thêm: không dòng comment nào trong `lib.rs` khớp mẫu, nên `contains` và `grep` vẫn là một phép đo.
2. **`prior-art.md` có ba hàng `| **fixbolt** |`**, không phải một; bộ tìm hàng lần đầu lấy nhầm dòng
   118. Test giờ đòi hàng đó chứa cả `return sites`.
3. **Cột *Khôi phục* của R71-1 ghi `git checkout`, và nó xoá luôn bản sửa.** Checkout trả file về
   commit gần nhất, mà bản sửa chưa commit — lần chạy xanh sau khôi phục là thứ duy nhất thấy. Ghi
   thành `docs/reference/a-restore-by-checkout-reverted-the-fix-too.md` `[to testing-skills]`, và
   **manager báo ngay cho bước 3 đang chạy**, vì R73-3 có đúng cột khôi phục đó trên
   `docs/CONFIGURATION.md` — file mà bước 3 cũng đang thêm một đoạn.

**`include_str!` đổi thành `read_to_string(CARGO_MANIFEST_DIR…)`** theo đúng hàng bẫy của plan:
`cargo package -p fixbolt-session --list` có liệt kê `tests/drop_reason.rs`. (`cargo package` đầy đủ
vẫn hỏng vì một lý do có sẵn, không liên quan: dependency đường dẫn `fixbolt-codec` không có
`version`.)

**Manager sửa rustdoc của test trước khi commit**: bản giao nói test *"fails … including the date
beside it"* — test không đọc ngày. Câu đó giờ nói thẳng là không đọc.

**Gate, manager chạy lại trong checkout chính:**

```
cargo test -p fixbolt-session --test drop_reason     9 passed; 0 failed
cargo clippy -p fixbolt-session --all-targets -D warnings   Finished, 0 warning
cargo fmt --check                                     fmt-exit=0
scripts/check-links.py (trong worktree step1, không có worktree lồng)
                                                      371 files, 1987 links, no dead internal links
R71-1 (23 → 24, khôi phục bằng bản sao)               đỏ: prior-art.md quotes 24 return sites of
                                                      Link::Dropped; crates/session/src/lib.rs has 23
                                                      — update the table, and the date beside it
                                                      → khôi phục → 1 passed
```

`check-links.py` chạy ở gốc checkout chính đọc **1485** file và báo 9 URL — cả 9 là
`crates/library/README.md` **bên trong ba worktree**, nơi ngoại lệ khoá theo đường dẫn không khớp.
Không phải lỗi của cây; là lý do memory ghi *xoá worktree trước khi chạy script toàn repo*.

### 2026-09-13 — bước 5 XONG (item 21)

`affinity::CorePin` (`to`, `allow_unisolated`, `core`, `validate` — `validate` là `ShardPlan::new(vec![core])`
nên **không có quy tắc từ chối mới**), `ServeError::Affinity`, và `serve_hft_pinned` theo thứ tự
*validate → pin → serve*. `ServeError` đã là `#[non_exhaustive]`, nên biến thể mới không phải
breaking change. `serve_hft` không đổi. Senior developer (opus) làm, trong worktree riêng.

**Bốn chỗ lệch plan trong test, cả bốn làm test mạnh hơn:**

1. **Test 1 và 2 dùng một port test đang giữ**, không phải `127.0.0.1:1`: chạy bằng root hoặc hạ
   `ip_unprivileged_port_start` thì bind port 1 thành công, và R21-1 **treo** thay vì đỏ.
2. **Test 3 thêm assertion mask** (`sched_getaffinity` trong `on_logon`) sau assertion `running_on`.
   `[measured 2026-09-13]` bỏ lời gọi pin, thread không ghim vẫn nằm đúng `cpu0` **10/16 lần**.
   Plan đã lường bẫy này và dặn *chạy 3 lần* — ba lần xanh cùng lúc sẽ xảy ra khoảng một phần tư số
   lần. Ghi thành trường hợp thứ tư của
   `docs/reference/a-reversal-needs-an-input-where-the-answers-differ.md`: *assert thứ việc ghim
   thay đổi, không phải thứ việc ghim làm cho có khả năng xảy ra*.
3. **Test 3 in giá trị trả về nếu cửa thoát trước khi bind** — lần đỏ đầu của R21-3 là 5 giây
   *never bound*, mất hẳn lý do.
4. **Test 3 đọc mask thêm một lần sau khi cửa trả về**, vì rustdoc và `GUIDE.md` nói thread vẫn bị
   ghim sau lời gọi, và §4 đòi câu đó có test đứng sau. Có reversal riêng.

**Một phát hiện về gate, chưa sửa:** rustdoc của biến thể mới lúc đầu link `Self::Tls`, gãy khi
build **chỉ** `--features affinity`. Job `docs` của CI chạy mặc định và `--all-features` — **không
tổ hợp nào trong hai cái đó thấy nó**. Developer tự bắt và sửa; lỗ ở CI vẫn còn. Ứng viên open
item ở bước đóng.

**Để senior review (bước 6) quyết, không sửa ở bước này:**
- `crates/library/src/lib.rs:27` re-export `serve_hft` nhưng **không** re-export `serve_hft_pinned`.
- `CorePin` không có accessor kiểu `ShardPlan::is_unisolated_allowed()`, trong khi ADR-0015 quyết
  định 5 nói việc miễn `isolcpus` phải nhìn thấy được.
- Thứ tự validate → pin → serve nghĩa là `NoCounterparties` (bảng rỗng) báo **sau** khi đã ghim,
  và thread vẫn bị ghim lúc lỗi trả về. Rustdoc có ghi.
- Có sẵn từ trước, không gate nào build: `unused import crate::msglog::MaybeLog` ở `shard.rs:43`
  dưới `--no-default-features --features affinity`.

**Gate, manager chạy lại trong checkout chính:**

```
cargo test -p fixbolt-engine --features affinity --test hft_pinned   ×3: 3 passed; 0 failed (2.01s)
cargo test -p fixbolt-engine --features affinity                     44 binaries, 352 passed, 0 failed
cargo clippy --all-targets --features affinity -- -D warnings         clean
cargo clippy --all-targets -- -D warnings                             clean
cargo build -p fixbolt-engine --no-default-features                   Finished
RUSTDOCFLAGS=-D broken_intra_doc_links cargo doc -p fixbolt-engine --no-deps --features affinity
                                                                      Finished, Generated
scripts/check-indexing-debt.sh                                        ok (181)
grep -c unsafe crates/engine/src/affinity.rs                          6 (trước: 6)
cargo fmt --check                                                     exit 0
scripts/check-links.py (trong worktree step5)                         372 files, no dead internal links
R21-1 (bind trước validate, khôi phục bằng bản sao, byte-identical)
  đỏ: expected Err(Affinity(NoSuchCore(CoreId(4096)))) before any bind;
      got Err(Io(Os { code: 98, kind: AddrInUse, message: "Address already in use" }))
  → khôi phục → 3 passed
```

R21-2 và R21-3 do developer chạy và trích (R21-2 đỏ 3/3 sau khi thêm mask; R21-3 đỏ đúng câu
`Err(Affinity(NotIsolated(CoreId(0))))` sau khi test in giá trị trả về).
