# Cổng phải nói đúng, và nói ở chỗ có người đọc

> **Loại:** Plan · **Ngày:** 2026-08-30 · **Trạng thái:** Chờ duyệt
> **Phạm vi:** open item 7, 17, 18, 19 — hạ tầng kiểm chứng, không phải giao thức

## Bối cảnh

Ngày 30-08-2026, chạy lại toàn bộ suite trên một máy Linux (không phải chiếc M5 đã dùng để
đóng plan `engine`) thì **cổng quan trọng nhất của dự án đỏ**: `cargo test -p nanofix-engine
--test wire` ra 39/59 thay vì 59/59. Đi tìm nguyên nhân thì lòi ra ba chuyện lớn hơn cái lỗi
ban đầu:

1. Điểm số của cổng đó **chạy theo một hằng số timeout**, không theo giao thức.
2. **CI đã đỏ đúng chuyện đó trên `main` từ lúc `engine` merge** — và không ai đọc. Suốt một
   ngày, `STATUS.md`, `README.md`, `DESIGN.md` §6 và `PRD.md` đều mang con số của cái laptop.
3. CI còn đỏ vì một lý do thứ hai chẳng liên quan gì: một lint clippy **ra đời sau khi code
   được viết**.

Ba chuyện đó cùng một hình dạng: **một cái cổng nói điều nó không thật sự kiểm được.** Đó
đúng là cái bẫy `CLAUDE.md` §10 tự đặt tên — *một check không chứng minh gì cho tới khi có
người đọc nó*. Plan này sửa hạ tầng ấy trước, vì `tools/w2w` (DESIGN §7 bước 7) dựng ngay lên
trên cái cổng đang đỏ, và mọi con số latency sau này sẽ được công bố qua chính CI này.

Item 7 (script tải QuickFIX bám nhánh `master` trôi nổi) đi kèm ở đây vì cùng một bệnh: kết
quả không tái lập được, chỉ khác là nguồn trôi thay vì toolchain trôi.

## Những gì đã biết chắc

Tất cả đều đã chạy và đọc output, không suy đoán:

- **Cổng wire chạy theo ngưỡng chờ.** Máy Linux 6.18 x86_64, 4 vCPU, `cargo 1.94.1`, cây làm
  việc không đổi. Chỉ đổi hằng số `quiet` trong `Wire::pump`
  (`crates/engine/tests/wire.rs:145`) — số lần `Engine::turn` liên tiếp không nhúc nhích trước
  khi harness coi là đã lắng:

  | `quiet` | Điểm | Thời gian |
  |---|---|---|
  | 200 (đang commit) | 39/59 | 0,7 s |
  | 2 000 | 43/59 | 4,3 s |
  | 20 000 | 59/59 | 41,3 s |

- **Bản thân session đúng.** `cargo test -p nanofix-session --test score` trên chính máy đó ra
  **59/59**. Sai lệch nằm ở thời điểm đến của câu trả lời, không ở nội dung.
- **Các diff báo lỗi nói đúng như vậy:** `FieldCount { expected: 9, actual: 8 }` ở một dòng và
  `expected: 8, actual: 9` bốn dòng sau là *một* câu trả lời đến sau cái bước lẽ ra phải đọc
  nó, làm lệch mọi so sánh phía sau.
- **CI đã đỏ trên `main`.** Run `33291318638`, commit `9986890`: job *Builds with nothing
  optional installed* fail đúng `the_fifty_nine_definitions_pass_through_a_real_socket` với
  `left: 39, right: 59`. Runner của GitHub ra **cùng con số 39** với máy ở đây.
- **Lint là lint mới.** `clippy::byte_char_slices` báo đỏ tại
  `crates/dict/tests/interop_quickfix_fields.rs:133`. URL trợ giúp của runner ghi
  `rust-1.98.0`. `clippy 0.1.94` ở đây và `1.95.0` trên M5 **đều cho file đó đi qua**. Repo
  không có `rust-toolchain.toml`, nên `-D warnings` = cấm mọi lint mà clippy tương lai nghĩ ra.
- **`scripts/fetch-quickfix-assets.sh` dòng 11:** `REF="${QUICKFIX_REF:-master}"`, clone
  `--depth 1 --branch "${REF}"`. Không ghim commit, không kiểm checksum. Mọi con số đếm được
  từ corpus (539 dòng, 247 dòng có `9=`, 244 dòng có `10=`, 59 file) có thể đổi mà không ai hay.
- **`scripts/check-links.py` chỉ đi qua Markdown.** Nó báo "26 markdown files, 162 internal
  links checked". Nó **không đọc rustdoc**, và hiện có hai link chết trong doc comment trỏ tới
  `https://github.com/nanofixengine/...` — một org không tồn tại:
  `crates/engine/src/dispatch.rs:8` và `crates/session/src/journal.rs:18`.

## Cách làm

Bốn việc độc lập, làm theo thứ tự rủi ro giảm dần. Mỗi việc kết thúc bằng một commit riêng
xanh trên **cả** máy Linux ở đây **và** CI — không có "xanh trên laptop" nào được tính.

### 17 — tiêu chí lắng phải gọi tên cái sự kiện nó chờ

Bỏ đếm vòng quay. Harness sẽ **chờ đúng số message mà bước đó kỳ vọng**, vì corpus đã biết con
số ấy: mỗi bước trong `.def` có bao nhiêu dòng `E` là biết phải nhận bao nhiêu message.

Việc này cần `nanofix_conformance::runner` nói cho `SessionUnderTest` biết bước hiện tại kỳ
vọng bao nhiêu output — hôm nay `step()` không nhận thông tin đó. Đây là **đổi API công khai
của một crate**, nên đi kèm một ADR mới (ADR-0009).

Quy tắc mới trong `Wire::pump`:

- Bước **kỳ vọng N message (N > 0)**: quay engine cho tới khi đã đọc đủ N message từ socket,
  hoặc hết một deadline theo *đồng hồ tường* rộng rãi (đề xuất 2 giây). Hết deadline mà chưa
  đủ N thì đó là **thất bại thật**, để nguyên cho comparator báo.
- Bước **kỳ vọng 0 message**: đây chính là cái bẫy "vacuous wait" — chờ-không-có-gì luôn
  thành công kể cả khi chẳng có gì xảy ra. Nên phải ghép với bằng chứng là hành động *đã*
  xảy ra: chờ một cửa sổ im lặng cố định theo đồng hồ tường (đề xuất 50 ms) **và** khẳng định
  kết nối vẫn đúng trạng thái mong đợi (còn sống, hoặc đã đóng nếu là dòng `e`).
- Bước **kỳ vọng ngắt kết nối**: chờ tới khi `read` trả `Ok(0)`, hoặc hết deadline.

Điểm mấu chốt: cả ba nhánh đều **chờ một sự kiện có tên**, deadline chỉ là lưới an toàn để
không treo — chứ không phải là tiêu chí.

### 19 — ghim toolchain, và đừng cấm cả tương lai

- Thêm `rust-toolchain.toml` ghim một bản stable cụ thể. Đề xuất **1.95.0** (bản trên M5, mới
  hơn 1.94.1 ở đây; rustup sẽ tự tải).
- Sửa cái lint đang đỏ: `.chain([b'*', b'?', b'!'])` → `.chain(*b"*?!")`.
- Thêm **một job CI mới, không chặn merge**, chạy `clippy` trên `stable` mới nhất. Cái này
  biến "repo đỏ vì một bản release" thành một cảnh báo đọc được trước, chứ không phải một
  buổi sáng đi tìm mình đã làm hỏng gì.

### 18 — bằng chứng đóng plan phải nêu một CI run

Đây là sửa **quy trình**, không phải sửa code, và nó là cái duy nhất trong plan này ngăn
chuyện tương tự lặp lại:

- `CLAUDE.md` §9 thêm một ô: *đã nêu tên CI run xanh cho đúng commit đó*.
- `CLAUDE.md` §10, mục "Failures no gate can see", thêm một dòng: *một plan đóng bằng output
  trên laptop trong khi CI đỏ*.
- Sửa hai link rustdoc chết, và **mở rộng `scripts/check-links.py` để đọc cả link trong file
  `.rs`** — vì cái gate hiện tại chỉ soi đúng chỗ đã sạch.

### 7 — ghim nguồn corpus

- `scripts/fetch-quickfix-assets.sh` mặc định ghim **một commit SHA cụ thể** thay vì `master`
  (`QUICKFIX_REF` vẫn cho phép ghi đè để nâng có chủ đích).
- Sau khi tải, script tự kiểm và in ra các con số đã ghi trong tài liệu (59 file `.def`, 539
  dòng message), và **thoát khác 0 nếu lệch**. Corpus đổi thì phải nổ ra ngay chỗ tải, chứ
  không phải nổ ở một test cách đó ba lớp.

## Bất biến bị đụng tới

Trong mười điều ở `CLAUDE.md` §2:

- **Số 3** (59 định nghĩa là cổng của session). Plan này không đổi hành vi session; nó làm cho
  con số 59/59 qua socket trở nên **tái lập được**. Sau khi xong, cổng phải là 59/59 trên cả
  hai máy và trên runner — nếu chỉ xanh ở một chỗ thì coi như chưa xong.
- **Số 6** (feature flag gate `mod`). Job `--no-default-features` phải xanh trở lại; hiện nó
  đang đỏ vì cổng wire, không phải vì feature.
- **Số 7** (không `unwrap`/`expect`/`panic` trong crate thư viện). Sửa lint không được đụng
  vào cấu hình đó; `scripts/check-lint-config.sh` vẫn phải xanh cả hai chiều.
- **Số 10** (không có số đo nào thiếu benchmark, máy, và cấu hình §9). Plan này không công bố
  số đo hiệu năng nào.

Không đụng `codec`, `session`, `engine` hay `transport` về mặt hành vi. Chỉ `crates/dict` có
một dòng test đổi cách viết, không đổi giá trị.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | **19 xong.** `rust-toolchain.toml` ghim 1.95.0, lint sửa, job `clippy-latest-stable` không chặn merge. `fmt · clippy · test` xanh trên CI | — |
| 2 | **ADR-0009** đề xuất: đổi API `SessionUnderTest::step` để bước biết số output kỳ vọng. Chờ duyệt trước khi viết code | — |
| 3 | **17 xong.** `Wire::pump` chờ sự kiện có tên; cổng wire 59/59 trên máy Linux ở đây, và **không đổi điểm** khi vặn deadline lên/xuống 10× | 2 |
| 4 | **17 xác nhận.** Cổng wire 59/59 trên CI runner. Job `--no-default-features` xanh | 3 |
| 5 | **7 xong.** Script ghim SHA, tự kiểm số lượng, đỏ khi lệch | — |
| 6 | **18 xong.** `CLAUDE.md` §9 và §10 thêm điều kiện CI; hai link rustdoc sửa; `check-links.py` đọc cả `.rs` | 4 |

## Cách kiểm chứng

**Bước 1.** Chạy `cargo clippy --all-targets -- -D warnings` sau khi ghim toolchain — không
chứng minh được ở máy này vì `0.1.94` không có lint đó, nên **bằng chứng duy nhất là runner
xanh**. Nói rõ điều đó chứ không lấp liếm. `scripts/check-lint-config.sh` chạy lại, đọc cả hai
nửa RED và GREEN.

**Bước 3 — đây là bước phải chứng minh cẩn thận nhất, vì nó sửa chính cái công cụ đo.**

- *Trước khi sửa*: chạy lại ba mốc `quiet` 200 / 2 000 / 20 000, chép lại 39 / 43 / 59. Đó là
  cái đỏ ban đầu, phải nhìn thấy nó đỏ trước.
- *Sau khi sửa*: **59/59**, và điểm **không đổi** khi deadline đổi từ 2 s xuống 0,5 s và lên
  20 s. Một tiêu chí đúng thì điểm phải phẳng theo deadline; nếu vẫn dốc thì chưa sửa được gì,
  chỉ đổi hằng số.
- *Chứng minh bằng đảo ngược*: chặn một câu trả lời của engine (bỏ một nhánh xử lý) và xem
  cổng đỏ đúng ở file đó, chứ không phải đỏ lan man. Rồi khôi phục.
- *Bẫy phải tránh*: một `pump` chờ **đúng N message** có thể xanh vì nó dừng ngay khi đủ N,
  bỏ qua message thứ N+1 sai lè. Nên sau khi đủ N, vẫn phải chờ hết cửa sổ im lặng và khẳng
  định **không có gì đến thêm**.

**Bước 4.** Đọc log của CI run, không đọc dấu tích. Chép lại `test result:` của cả hai job.

**Bước 5.** Xoá `vendor/`, chạy lại script, đọc số nó in. Rồi **đảo ngược**: sửa tay một file
`.def` cho lệch số đếm và xem script thoát khác 0.

**Bước 6.** `scripts/check-links.py` chạy trên cây hiện tại phải **đỏ** (vì hai link rustdoc
chết vẫn còn), sửa link, rồi xanh. Đúng thứ tự đó — thấy đỏ trước.

**Mọi bước** đều chạy `cargo test --all` và `cargo test --no-default-features`.

## Tài liệu phải cập nhật

Theo bảng đồng bộ `CLAUDE.md` §4:

- [ ] `DESIGN.md` §6 — ô cổng wire bỏ ghi chú "chưa đạt", ghi lại cách đo mới (đổi *cách một
      cổng được đo* → bắt buộc theo §4)
- [ ] `DESIGN.md` §6 — thêm dòng cho job `clippy-latest-stable` và cho việc ghim toolchain
- [ ] `PRD.md` §2 — tiêu chí thoát phase 1 số 1 ghép lại làm một khi cổng tái lập được
- [ ] `README.md` — bỏ đoạn cảnh báo "single-machine" khi nó không còn đúng
- [ ] `STATUS.md` — đóng item 7, 17, 18, 19; ghi số đo mới
- [ ] `docs/reference/measured-costs.md` — ghi kết quả "điểm phẳng theo deadline", vì đó là
      bằng chứng cho thấy đã sửa đúng bệnh
- [ ] `CHANGELOG.md` — API `SessionUnderTest` đổi (crate `conformance`)
- [ ] `docs/decisions/ADR-0009-*.md` — mới
- [ ] `CLAUDE.md` §9 và §10 — điều kiện CI (bước 6). **Nói to ra rule nào đã đổi.**

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Sửa xong nhưng điểm vẫn dốc theo deadline → chỉ đổi hằng số, chưa đổi tiêu chí | Bước 3: chạy ba deadline, khẳng định điểm phẳng |
| `pump` dừng ngay khi đủ N, nuốt mất message thứ N+1 sai | Bước 3: sau khi đủ N vẫn chờ hết cửa sổ im lặng, khẳng định không có gì thêm |
| Bước kỳ vọng 0 message xanh vì chẳng có gì xảy ra cả (vacuous wait) | Ghép với khẳng định trạng thái kết nối; đảo ngược bằng cách ngắt kết nối và xem nó đỏ |
| Ghim toolchain xong lại quên `clippy-latest-stable`, nửa năm sau nâng một phát ăn 40 lint | Job không chặn merge chạy mỗi lần push |
| Ghim SHA corpus xong nhưng không ai biết nâng thế nào | Script in rõ cách ghi đè bằng `QUICKFIX_REF` và số commit đang ghim |
| `check-links.py` mở rộng sang `.rs` rồi báo false positive tràn lan | Chạy trên cây hiện tại phải ra **đúng 2** link chết, không nhiều hơn |
| Sửa lint clippy bằng cách thêm `#[allow]` | Không dùng `#[allow]`; sửa đúng dòng code |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Đổi API `SessionUnderTest` làm hỏng cổng in-process 59/59 | Cao | Bước 3 chạy **cả hai** cổng; `--test score` phải giữ nguyên 59/59, không được sửa một dòng nào trong `score.rs` để nó qua |
| Ghim toolchain 1.95.0 mà máy này chỉ có 1.94.1 | Thấp | rustup tự tải; nếu tải hỏng thì hạ xuống bản có sẵn và ghi rõ |
| Deadline 2 giây làm cổng wire chạy chậm hẳn trên CI | Trung bình | Deadline chỉ tiêu tốn thời gian ở bước *thất bại*; bước thành công về ngay khi đủ N. Đo lại thời gian chạy, ghi vào nhật ký |
| Cổng wire vẫn không 59/59 trên CI dù xanh ở đây | Trung bình | Đó là kết quả thật và phải báo là **chưa xong**, không phải lý do để nới deadline |
| Sửa `CLAUDE.md` §9 làm quy trình nặng thêm mà không ai theo | Trung bình | Chỉ thêm đúng một ô, và nó chỉ áp cho lúc **đóng plan**, không áp cho từng commit |

## Ngoài phạm vi

- **Không** dựng `tools/w2w` (DESIGN §7 bước 7) — plan riêng.
- **Không** đụng vào tốc độ serialise (item 11) hay profile release (item 13).
- **Không** sửa DATA field (item 8, 9), journal đọc lại (item 16), chính sách ring đầy (item 5).
- **Không** đổi hành vi session hay engine. Nếu bước 3 làm lộ ra một lỗi giao thức thật, thì
  **dừng lại, sửa plan, xin duyệt lại** — không nhét vào đây.
- **Không** đổi tên project (item 1).

## Nhật ký giao hàng

*(chưa bắt đầu — plan đang chờ duyệt)*
