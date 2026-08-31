# Serialise: tìm ra 145 ns đi đâu, trước khi sửa bất cứ thứ gì

> **Loại:** Plan · **Ngày:** 2026-08-31 · **Trạng thái:** **Đã duyệt 2026-08-31**
> **Phạm vi:** `codec` — `template.rs`, `benches/serialize.rs`. Không đụng `session`,
> `engine`, `transport`.
>
> **Duyệt bằng uỷ quyền.** `[2026-08-30]` chủ dự án uỷ quyền việc viết plan và duyệt plan cho
> agent làm việc tại đây. Không ai đọc plan này thay mặt chủ dự án. `CLAUDE.md` §10 không
> đổi: việc bỏ một chữ ký không bỏ bất kỳ bằng chứng nào.

## Bối cảnh

`DESIGN.md` §6 có một dòng gate ghi thẳng là **chưa đạt**:

> Serialise `ExecutionReport` (template, D9) — ≤ **60 ns** *published*.
> `[measured]` **93.8 ns — the target is NOT met**.

Đây là open item 11. Nó đã nằm đó qua nhiều phiên, và mỗi lần đo lại trên một máy mới thì con
số càng xa hơn chứ không gần lại. **Không máy nào từng tới gần 60 ns.**

Item 11 cũng đã nêu sẵn một nguyên nhân — `Template::encode` quét tuyến tính danh sách slot của
người gọi — và hai hướng sửa. Plan này **không bắt đầu bằng việc sửa**, và lý do là chuyện đã
xảy ra hôm nay.

**`[đo 2026-08-31]` một thí nghiệm sai được ghi lại trước khi plan này bắt đầu.** Để chứng minh
cái quét đó đắt, tôi đảo ngược thứ tự 14 slot mà người gọi đưa vào và đo lại: **145.0 → 153.5
ns**, chênh 6%, trông như "quét không đáng kể". **Kết luận đó sai vì thí nghiệm sai.** Xuôi thì
part thứ *i* khớp ở vị trí *i*, tổng 1+2+…+14 = **105** phép so. Ngược thì khớp ở 14−*i*, tổng
14+13+…+1 = **cũng 105**. Đảo thứ tự **không thay đổi biến mà tôi tưởng mình đang thay đổi**.
6% kia là branch prediction, không phải độ dài quét.

Đó là lý do plan này có hình dạng như dưới đây: **bước 1 là đo xem 145 ns đi đâu, không phải
sửa.** `measured-costs.md` đã có sẵn bài học này dưới tên *"characterise before you attribute"*
— tám giả thuyết được nêu, và cái đúng không nằm trong tám cái đó. Việc sửa mà không đo trước
sẽ tạo ra một thay đổi không ai chứng minh được là có tác dụng.

**Plan này không hứa đạt 60 ns.** Nó hứa trả lời được: 145 ns gồm những gì, cắt cái quét đi thì
còn bao nhiêu, và **60 ns có còn là một con số đúng để giữ trong `DESIGN.md` §6 hay không.**
Cả hai câu trả lời đều là kết quả.

## Những gì đã biết chắc

Không có phỏng đoán ở mục này.

| Sự thật | Nguồn |
|---|---|
| Target công bố **≤ 60 ns**; ceiling mà bench thật sự chặn là **190 ns** — hai con số khác nhau | `DESIGN.md` §6, `crates/codec/benches/serialize.rs` |
| **Chưa máy nào tới gần 60**: 93.8 (Apple M5) · 177.6–199.4 (Linux x86_64, 5 lần) · **240.0** (desktop §9, `bench.sh --strict`) · 241.4 (desktop, median 15 lần) | `DESIGN.md` §6; STATUS mục **Proven** và open item 20 |
| `[đo 2026-08-31]` container này (Xeon 2.10GHz, 4 core, docker): **154.3 / 185.0 / 156.6 ns** — spread **20%** trên ba lần chạy liên tiếp | `cargo bench --bench serialize`, `git 371aef3` |
| Container này `check-machine.sh` = **`pass 2 fail 6 unknown 3`**, là guest dưới docker | `scripts/check-machine.sh`, 2026-08-31 |
| Vòng lặp encode gọi `slots.iter().find(...)` **cho từng part**, ở cả `Part::Slot` và `Part::DataLen` | đọc `crates/codec/src/template.rs:562` và `:573` |
| Template của bench: 3 field tĩnh + **14 slot**; người gọi đưa 14 slot đúng thứ tự template → **105** phép so tag mỗi lần encode | đọc `crates/codec/benches/serialize.rs` |
| `[đo 2026-08-31]` đảo thứ tự slot: 145.0/146.2 xuôi vs 153.5/149.6 ngược. **Thí nghiệm này vô giá trị** — cả hai đều 105 phép so | mục Bối cảnh ở trên |
| `Template` giữ `parts: [Part; P]`, `len: u8`. **Không có index theo tag** | đọc `template.rs:143–152` |
| `codec` có **zero** runtime dependency, và đó là luật | `CLAUDE.md` §6 |
| `[đo 2026-08-30]` cùng ba case này vượt ceiling **15/15 lần ở cả hai trạng thái máy** trên desktop — `encode ExecutionReport` 241.4 vs 190, `encode 1 group` 104.7 vs 75, `walk 4 levels` 347.6 vs 300 | STATUS open item 20 |
| `[đo 2026-08-30]` chỉnh máy theo §9 chỉ dịch median bench **dưới 2%**; tải cạnh tranh dịch **71%** | STATUS open item 20; `DESIGN.md` §9 |

**Hệ quả của hai dòng cuối, và nó quyết định cách đo trong plan này:** ba case đó vượt ceiling
là **thật**, không phải nhiễu máy — chúng vượt ở mọi trạng thái máy đã đo. Nhưng spread 20% trên
container này lớn hơn nhiều so với thứ ta muốn thấy, nên **một lần chạy trước và một lần chạy
sau không phải là bằng chứng gì cả.**

## Cách làm

Ba giai đoạn, và giai đoạn sau chỉ bắt đầu khi giai đoạn trước đã có số.

**A. Đo xem 145 ns gồm những gì** — quét một biến, là **số slot**.

Dựng cùng một template với *N* slot, *N* = 1, 2, 4, 8, 14, và encode với đúng *N* slot đó. Nếu
chi phí quét là O(parts × slots) thì thời gian phải cong theo *N²*; nếu nó phẳng hoặc tuyến
tính thì cái quét **không phải** thứ đang chi phối và item 11 nêu sai nguyên nhân.

Đây là điểm mà plan có thể chứng minh mình sai, và nó được đặt ở bước đầu vì lý do đó.

Thêm hai mốc để chia phần còn lại: cùng template nhưng **không slot nào** (chỉ field tĩnh), và
chi phí thuần của `put` cho 14 field khi tag đã biết sẵn.

**B. Sửa, nếu và chỉ nếu A chỉ ra cái quét đáng kể.**

Cách sửa được chọn là **con trỏ tiến (forward cursor) có đường lui**, không đổi API:

giữ một chỉ số `c` vào `slots`. Với `Part::Slot(tag)`: thử `slots[c]` trước; khớp thì dùng và
`c += 1`. Không khớp thì rơi về đúng `find` như hiện nay, và đặt `c` = vị trí khớp + 1.

Vì sao chọn cách này thay vì hai hướng item 11 nêu:

- **Không đổi chữ ký, không đổi ngữ nghĩa.** Người gọi vẫn đưa slot theo thứ tự bất kỳ; thứ tự
  khớp template chỉ là *đường nhanh*. Ràng buộc "người gọi phải đưa đúng thứ tự" sẽ là một
  constraint mà compiler không kiểm được — `GUIDE.md` tồn tại cho loại đó, và không thêm một
  cái nữa thì tốt hơn.
- **Không cấp phát, không dependency.** Index theo tag dựng lúc build template không dùng được:
  slot đến từ **người gọi lúc encode**, không từ template.
- **Trường hợp xấu nhất bằng đúng hôm nay**, không tệ hơn.

**C. Trả lời câu 60 ns.**

Sau B, con số trên máy §9 sẽ nói `DESIGN.md` §6 phải làm gì. Ba kết cục, và **cả ba đều là kết
quả hợp lệ**:

1. Đạt ≤ 60 → sửa `[measured]` trong §6, đóng item 11.
2. Còn cách xa nhưng A đã chỉ ra phần còn lại nằm ở đâu → §6 giữ target, ghi số mới và **nêu tên
   thứ đang chiếm phần lớn**, item 11 thu hẹp lại quanh thứ đó.
3. A cho thấy 60 ns không đạt được với hình dạng `Part` hiện tại → **60 là con số sai**, và nó
   cần một ADR để đổi, không phải một lần sửa lặng lẽ. `DESIGN.md` §6 nói target neo vào một
   phép đo trên Apple M5 ngày 2026-08-27; một target không máy nào đạt là một target đã ngừng
   nói điều gì — đúng chữ mà open item 20 đã dùng cho các ceiling.

File sẽ đụng: `crates/codec/src/template.rs`, `crates/codec/benches/serialize.rs`,
`crates/codec/tests/` (test mới cho thứ tự slot), `docs/DESIGN.md` §6, `docs/reference/`,
`STATUS.md`, `CHANGELOG.md`.

## Bất biến bị đụng tới

| # | Điều | Giữ bằng cách nào |
|---|---|---|
| 1 | **Không cấp phát trên hot path** | Con trỏ là một `usize` trên stack. `crates/codec/benches/alloc.rs` phải vẫn **0**, chạy lại ở mỗi bước |
| 5 | **Thứ tự field đến từ bảng sinh ra, không từ call site** | Đây là điểm nguy hiểm nhất của plan này. Con trỏ **không được** làm output phụ thuộc thứ tự người gọi đưa slot. Test: encode cùng một tin với slot xuôi, ngược và xáo trộn phải cho **byte y hệt nhau** |
| 7 | **Không `panic!` / `unwrap()` / `expect()` trong crate thư viện** | `slots[c]` phải là `slots.get(c)`. Lint workspace chặn, `scripts/check-lint-config.sh` chứng minh lint đó thật |
| 10 | **Không có số nào không kèm bench, máy và cấu hình §9** | Container này là guest, `pass 2 fail 6`. Mọi số ở đây là **A/B cùng máy**, không công bố được. Số công bố **chỉ đến từ desktop §9** với `bench.sh --strict` |

Không đụng 2, 3, 4, 6, 8, 9.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | **Quét theo số slot** trong một bench tạm, *N* = 1, 2, 4, 8, 14, mỗi *N* nhiều lần. Bảng số + hình dạng đường cong. **Kết luận: cái quét có đáng kể không** | — |
| 2 | Hai mốc chia phần: template không slot; `put` 14 field khi tag đã biết. Ra được **145 ns gồm mấy phần** | 1 |
| 3 | *Chỉ khi bước 1 nói đáng kể:* con trỏ tiến có đường lui trong `encode_with`. Test thứ tự xuôi/ngược/xáo trộn cho byte y hệt | 1, 2 |
| 4 | A/B trên container này, **≥ 15 lần mỗi nhánh**, so median chứ không so một lần chạy | 3 |
| 5 | Đo lại trên **desktop §9** sau `fixbolt-machine on`, `bench.sh --strict`, kèm khối machine | 4, desktop |
| 6 | Quyết định `DESIGN.md` §6 theo một trong ba kết cục ở mục C. Nếu là kết cục 3 thì viết **ADR-0016** | 5 |

**Bước 1 có quyền huỷ bước 3.** Nếu đường cong phẳng theo *N* thì con trỏ không đáng làm, plan
dừng ở bước 2, và **cái được giao là câu trả lời đúng về nguyên nhân** cộng một bản sửa cho item
11 vốn đang nêu sai. Đó không phải thất bại.

## Cách kiểm chứng

Đọc output, không đọc exit code.

- **Bước 1 — đường cong phải phân biệt được hai giả thuyết.** O(N²) và O(N) khác nhau rõ ở *N* =
  14. Ghi cả số thô, không chỉ kết luận. Mỗi *N* chạy ≥ 15 lần vì `[đo 2026-08-31]` container này
  có spread 20%.
- **Bước 3 — thứ tự người gọi không đổi output.** Test encode cùng nội dung với slot xuôi, ngược,
  và một hoán vị cố định; **so `assert_eq!` trên bytes**, không so độ dài. **Đảo ngược**: bỏ
  đường lui `find` đi, test xáo trộn phải **đỏ**; khôi phục, xanh lại. Kiểm rằng nó đỏ **vì sai
  bytes**, không vì panic.
- **Bước 3 — slot không được cấp vẫn bị bỏ qua đúng.** Template khai 14 slot, người gọi đưa 10.
  Con trỏ không được vì thế mà lệch. Test riêng, và một ca DATA/DataLen riêng vì nhánh đó tra
  `data_tag` chứ không tra `tag`.
- **Bước 3 — `benches/alloc.rs` vẫn 0**, và case của nó vẫn khẳng định đường của mình còn sống.
- **Bước 3 — nhóm lặp vẫn đúng byte.** `cargo test -p fixbolt-codec` toàn bộ, đặc biệt
  `group_roundtrip.rs` (357 vị trí) và `groups.rs` (731 vị trí).
- **Bước 3 — cổng 59 vẫn xanh**, cả hai chế độ: `-p fixbolt-session --test score` và
  `-p fixbolt-engine --test wire`.
- **Bước 4 — so median 15 lần, không so một lần.** Ghi cả hai phân bố. Một thay đổi đáng vài
  chục ns nằm **trong** spread 20% của máy này nếu chỉ chạy một lần mỗi nhánh.
- **Bước 5 — số kèm khối machine của `check-machine.sh`**, và `pass 10 fail 0` trước khi đo.
- **Mọi bước — CI xanh, nêu tên run theo id** (`CLAUDE.md` §9).

## Tài liệu phải cập nhật

- [ ] `docs/DESIGN.md` §6 — dòng serialise: số mới, và target theo kết cục ở mục C
- [ ] `docs/DESIGN.md` §8 — chỉ nếu bước 5 dịch một dòng của latency budget
- [ ] `docs/reference/measured-costs.md` — bảng của bước 1 và 2, đây là chỗ nó thuộc về
- [ ] `docs/reference/` — thí nghiệm vô giá trị ở mục Bối cảnh, kèm `[to testing-skills]`
- [ ] `STATUS.md` — item 11: sửa nguyên nhân nếu bước 1 bác bỏ nó; đóng nếu đạt
- [ ] `CHANGELOG.md` — chỉ nếu public API đổi (kế hoạch hiện tại là **không** đổi)
- [ ] `docs/decisions/ADR-0016-…` — **chỉ** ở kết cục 3
- [ ] rustdoc của `Template::encode_with` — nếu có đường nhanh thì nói rõ nó là *đường nhanh*,
      không phải yêu cầu, và **nêu tên test chứng minh** (`CLAUDE.md` §4)

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| **Thí nghiệm không thay đổi biến mình tưởng** — đã xảy ra một lần hôm nay | Bước 1 quét *N*, và *N* thay đổi thật số phép so. Ghi số phép so kỳ vọng bên cạnh số đo |
| Con trỏ làm output phụ thuộc thứ tự người gọi | Test xuôi/ngược/xáo trộn cho byte y hệt, chứng minh bằng đảo ngược |
| Con trỏ lệch khi một slot không được cấp | Test riêng: khai 14, đưa 10 |
| Nhánh `DataLen` tra `data_tag` chứ không `tag` — con trỏ không áp dụng được | Test DATA riêng; nhánh đó **giữ nguyên `find`** trừ khi bước 1 nói khác |
| So một lần chạy trước với một lần chạy sau trên máy spread 20% | Bước 4 bắt buộc ≥ 15 lần mỗi nhánh, so median |
| Bench xanh (dưới 190) đọc thành "đạt" trong khi target 60 vẫn trượt | `DESIGN.md` §6 phải in **cả hai** số. Bench in ceiling, không in target — ghi vào bước 6 |
| Tối ưu một bench không đại diện cho đường dùng thật | Trước bước 3, đọc `session` gọi `encode` với bao nhiêu slot thật. Nếu thật sự là 3–4 slot thì bức tranh khác hẳn |
| Số từ container này bị trích như số latency | Mọi bảng kèm dòng `pass 2 fail 6 unknown 3`. Số công bố chỉ từ bước 5 |
| `slots[c]` panic | `slots.get(c)`. Lint chặn, và `check-lint-config.sh` chứng minh lint đó thật |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| **Bước 1 bác bỏ nguyên nhân item 11 nêu** | **Trung bình, và đã lường** | Đó là kết quả, không phải hỏng. Plan dừng ở bước 2, item 11 được sửa lại cho đúng, và thứ chiếm phần lớn được nêu tên |
| Con trỏ giúp ít vì 105 phép so trên dữ liệu nóng L1 rẻ hơn tưởng | Cao | Chính xác là thứ bước 1 đo. Không viết code trước khi có số |
| Đạt 60 ns là bất khả với hình dạng `Part` hiện tại | Trung bình | Kết cục 3: ADR-0016 đổi target, chứ không lặng lẽ sửa số |
| Desktop không rảnh để chạy bước 5 | Trung bình | Bước 1–4 không cần nó. Bước 5–6 chờ, và plan **không đóng** khi thiếu — đúng như `standard-mode` đã làm với phép đo wakeup |
| Sửa `encode` làm hỏng nhóm lặp | Thấp | 357 + 731 vị trí đã có test byte-identical, chạy ở bước 3 |

## Ngoài phạm vi

- **Không SIMD/SWAR.** Đó là item 12, ước tính 20–40 ns, và item 22 đã xếp nó sau việc gỡ syscall.
- **Không đụng `walk 4 levels` hay `encode 1 group`.** Chúng cũng vượt ceiling 15/15 nhưng là
  đường khác; gộp vào là mở rộng phạm vi.
- **Không đổi ceiling 190** của bench. Ceiling là chống hồi quy; target 60 là lời hứa. Hai thứ
  khác nhau và bước 6 chỉ động vào cái thứ hai.
- **Không đổi API công khai** theo kế hoạch hiện tại. Nếu bước 3 hoá ra cần đổi thì **dừng và
  sửa plan**, theo `CLAUDE.md` §1.

## Nhật ký giao hàng

**Bước 1 — xong 2026-08-31. Nó bác bỏ cách item 11 đóng khung vấn đề, đúng như plan cho phép.**
Quét theo *N* = 0, 1, 2, 4, 8, 14, năm lần mỗi mức, median: **30.8 / 46.9 / 55.2 / 77.5 / 111.3
/ 152.5 ns**. Đường **thẳng theo số slot**, không cong theo số phép so. Phép đo riêng cho cái
quét — chèn *k* slot không khớp vào **đầu** danh sách, output byte y hệt ở cả bốn nhánh (169
byte, cùng tổng byte) — cho **~0.4 ns mỗi phép so tag**, tức 105 phép so đáng **~42 ns trên
~152**, khoảng **24%**. Thật, nhưng không phải phần lớn.

**Bước 2 — xong 2026-08-31.** 152.5 ns tách thành: **~31 ns cố định** trả trước khi ghi field
biến đầu tiên (prefix, 3 field tĩnh, render body length, trailer) — **51% của cả cái target 60
ns**, trên một message không mang gì; cộng **~8.7 ns mỗi slot**, trong đó `put` ~7 ns và quét
~3 ns.

**`checksum` bị nghi rồi được loại.** Nó chạy trên toàn message nên "chi phí cố định" không
thật sự cố định — 43 byte ở *N*=0, 169 byte ở *N*=14. Đo riêng: **2.3 ns** và **3.2 ns**. Đã
vector hoá, gần như phẳng, **~0.9 ns trong chênh lệch 122 ns**. Ghi lại như một nghi vấn *đã
loại*, không để treo.

**Hệ quả cho bước 3 và bước 6.** Gỡ sạch cái quét còn **~116 ns** so với target **60**. Nên:
- bước 3 (con trỏ tiến) vẫn đáng làm — **~24%, không đổi API** — nhưng nó **không** đóng được
  item 11, và plan không được để nó trông như đóng;
- **kết cục 3 ở mục C là kết cục nhiều khả năng nhất**: 60 ns không đạt được chỉ bằng việc sửa
  cái quét, và ba đòn bẩy theo thứ tự đo được là **chi phí cố định → `put` → quét**. Đổi target
  cần **ADR**, không phải sửa số lặng lẽ.

**Bước 3 — làm xong, đo, và ĐẢO NGƯỢC 2026-08-31. Đây là kết quả, không phải hỏng.**
Con trỏ tiến có đường lui đã được viết, qua **mọi** cổng đúng đắn — 6/6 test thứ tự, 59/59 cả
hai chế độ, `alloc` vẫn 0, nhóm lặp byte y hệt, fmt/clippy sạch. Rồi đo A/B cùng máy, **30 lần
mỗi nhánh**: baseline median **154.6**, cursor **159.8** — **chậm hơn 5.2 ns (+3.4%)**. Máy này
lưỡng mode nên so **trong từng mode**: **+4.5** và **+4.4 ns**, khớp nhau tới 0.1 ns và cùng dấu
với median gộp. Hồi quy lặp lại được, không phải nhiễu. **Đã revert.**

**Và nó sửa chính con số của bước 1.** 0.4 ns mỗi phép so được đo trên scan **78 phần tử** rồi
ngoại suy xuống 14 — ngoại suy đó không đúng. Scan ngắn trên dữ liệu nóng, đoán nhánh chuẩn, rẻ
hơn nhiều mỗi phần tử; con trỏ thay nó bằng một nhánh phụ thuộc dữ liệu và một `usize` mang qua
vòng lặp, trình tối ưu có ít thứ để làm hơn chứ không nhiều hơn. **Nên "~42 ns là cái quét" là
ước tính cao**, và phép đo này là thứ nói ra điều đó. Kết luận của bước 1 **vẫn đứng và mạnh
hơn**: cái quét không phải chỗ mất 60 ns.

**Giữ lại từ lần thử này:** `crates/codec/tests/slot_order.rs`, 6 ca giữ cho thứ tự người gọi
không bao giờ ra tới dây. Đường body **trước đây không có guard nào** cho điều 5 — chỉ nhóm lặp
có, trong `group_roundtrip.rs`. Chứng minh bằng đảo ngược **hai lần**: lấy slot theo thứ tự
người gọi → 4/6 đỏ vì **sai bytes**; xoá đường lui của con trỏ → cũng 4 ca đó đỏ vì **mất
field**. Guard sống lâu hơn cái thay đổi đã sinh ra nó.

**Bước 4 do đó cũng đóng, và nó huỷ bước 3** — đúng thẩm quyền plan đã trao cho phép đo.

Ghi chép: [measured-costs.md](../reference/measured-costs.md). Bench tạm đã xoá, cây làm việc
sạch. Số đo trên container `pass 2 fail 6 unknown 3` — **tỷ lệ dùng được, con số tuyệt đối
không công bố được**, và bước 5 trên desktop §9 vẫn còn nguyên đó.

**Bước 5 — xong 2026-08-31, trên desktop §9.** `check-machine.sh` = **`pass 10 fail 0
unknown 1`**, §9 thoả mãn. `encode ExecutionReport (template)` = **236.2 ns**, median của **27
lần chạy đạt chuẩn** (3 lần bị loại vì máy đọc 7–9% busy, trên ngưỡng 3%). Con số này tái lập
240.0 ns đo ngày 2026-08-30 trong vòng **1.6%**. So với target 60 ns: **trượt 3.9×**.

**Bước 6 — xong 2026-08-31. Kết cục là số 3, đúng như bước 2 đã dự đoán, và nó lớn hơn dòng
serialise.** Chủ dự án chọn hướng trực tiếp: *"hạ mục tiêu xuống mức với tới được, theo baseline
từng máy"*, phạm vi **cả bảng §6** chứ không riêng dòng này, và **bỏ hẳn cột target tuyệt đối**
chứ không chỉ hạ nó.

Việc đó được làm ở một plan riêng —
[per-machine-baselines](2026-08-31-per-machine-baselines.md) — vì nó chạm cả 12 case timing chứ
không một case, và nó sinh ra
**[ADR-0016](../decisions/ADR-0016-per-machine-baselines-replace-absolute-targets.md)**.

**Thứ tìm được ở bước 6 mà plan này không đi tìm:** gốc của 60 ns. `DESIGN.md` §4 D9 nói ra
bằng chính lời nó — *"This is how the fastest commercial engines reach tens of nanoseconds per
serialise, and it is why the published serialise target in §6 is 60 ns, not 150."* **60 ns là
một con số đọc được về phần mềm của người khác, chưa bao giờ là phép đo của engine này.** Khác
hẳn 150 ns của parse, vốn neo vào 139 ns đo tại đây. Suốt bốn tháng §6 để hai loại số đó dưới
cùng một cột, và chỉ loại thứ hai mới gate được cái gì.

**Plan này đóng.** Sáu bước, sáu kết quả: cái quét không phải thủ phạm (bước 1), 152.5 ns tách
được thành ~31 ns cố định + ~8.7 ns mỗi slot (bước 2), bản sửa được viết và bị phép đo huỷ
(bước 3–4), số §9 là 236.2 ns (bước 5), và target 60 ns bị rút (bước 6). **Thứ ở lại lâu nhất
có lẽ là `crates/codec/tests/slot_order.rs`** — guard cho điều 5 mà đường body chưa từng có,
sinh ra từ một thay đổi đã bị revert.
