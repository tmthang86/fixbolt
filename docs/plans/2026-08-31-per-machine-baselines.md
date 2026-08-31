# Bỏ target tuyệt đối trong §6, thay bằng baseline theo từng máy

> **Loại:** Plan · **Ngày:** 2026-08-31 · **Trạng thái:** **Đã duyệt 2026-08-31**
> **Phạm vi:** `DESIGN.md` §6, `crates/codec/benches/harness.rs` và bốn bench target dùng nó,
> `scripts/bench.sh`. Không đụng `codec` runtime, `session`, `engine`, `transport`.
>
> **Duyệt bằng uỷ quyền.** `[2026-08-30]` chủ dự án uỷ quyền việc viết plan và duyệt plan cho
> agent làm việc tại đây. Không ai đọc plan này thay mặt chủ dự án. `CLAUDE.md` §10 không đổi.
>
> **Hướng do chủ dự án chọn, `[2026-08-31]`:** *"hạ mục tiêu xuống mức với tới được, theo
> baseline từng máy"*, phạm vi **cả bảng §6**, và **bỏ hẳn cột target tuyệt đối** cho các dòng
> timing. Plan này thực hiện đúng hai câu đó.

## Sửa 1 — `[2026-08-31]` một `MARGIN` chung là bất khả, và phép đo nói vì sao

**Bước 1 đã chạy và nó bác bỏ mục 3 của *Cách làm*.** Plan viết: *"`MARGIN` được chọn bằng cách
chạy suite N ≥ 20 lần và lấy con số nhỏ nhất mà số lần nhấp nháy là 0/N"*, với giả định ngầm là
tồn tại một con số như vậy. **Không tồn tại**, và cái chặn không phải nhiễu.

`[đo 2026-08-31]` 21 lần chạy, máy §9, `check-machine.sh` = **`pass 10 fail 0 unknown 1`**,
tải theo *từng* lần chạy 0–2% trừ một lần 4%. Tỷ lệ max/median của 12 case:

| Case | med | min | max | max/med |
|---|---|---|---|---|
| inline deliver + reply | 6.3 | 6.3 | 8.3 | **1.317** |
| ring, one way | 270.0 | 267.2 | 335.7 | **1.243** |
| ring, round trip | 522.4 | 514.9 | 579.2 | **1.109** |
| encode ExecutionReport (template) | 241.6 | 235.2 | 259.9 | 1.076 |
| encode 1 group, 2 entries | 105.0 | 98.1 | 110.5 | 1.052 |
| group_members contains, 61 tags | 9.4 | 9.1 | 9.8 | 1.043 |
| parse Heartbeat (validated) | 56.1 | 54.8 | 58.2 | 1.037 |
| walk 4 levels, 61-tag member list | 350.5 | 339.2 | 359.5 | 1.026 |
| walk 1 group, 2 entries, 2 members | 58.3 | 58.2 | 59.7 | 1.024 |
| parse NewOrderSingle (no checks) | 115.3 | 113.7 | 117.8 | 1.022 |
| parse NewOrderSingle (validated) | 122.5 | 120.0 | 124.9 | 1.020 |
| SendingTime from the cache | 4.9 | 4.8 | 4.9 | 1.000 |

Một `MARGIN` chung phải ≥ **1.35** để 0/21. Áp 1.35 cho `encode ExecutionReport` nghĩa là nó
được phép đi từ 241.6 lên **326 ns** mà không ai đỏ — trong khi case đó tán thật chỉ 7.6%. **Một
biên chung đủ rộng cho case hẹp nhất là một biên vô dụng cho chín case còn lại.**

**Và nhìn vào phân bố thì rõ đây không phải nhiễu, mà là hai mode rời rạc chọn một lần cho mỗi
tiến trình:**

- `inline deliver + reply` — 21 giá trị rơi vào **ba** cụm rời: `6.3–6.4` (13 lần), `7.4`
  (4 lần), `8.1–8.3` (4 lần). Không có giá trị nào ở giữa. Trên một case 6.3 ns, lượng tử 2 ns.
- `ring, one way` — **19/21 nằm trong 267.2–272.8** (tán 2.1%), rồi **2 lần nhảy thẳng sang
  334.5 và 335.7**. Đây đúng là mode thứ hai ~324 ns mà open item 20 đã loại **năm** giả thuyết
  (L3, SMT, governor/boost, nhiệt, tải) mà vẫn không giải thích được. Tần suất 2/21 = **9.5%**,
  khớp con số *5–10% trên máy yên* item 20 ghi.

**Hệ quả kỹ thuật, và nó là lý do không nống biên lên 1.35:** harness lấy `best` của 7 mẫu
**bên trong một tiến trình**. Một mode được chọn **cho cả tiến trình** thì bảy mẫu đều nằm trong
cùng mode đó, nên tăng số mẫu không dập được nó. Đây là thứ mới, và nó thuộc về
`docs/reference/measured-costs.md`.

**Thay đổi:** `MARGIN` **không** còn là một hằng số chung trong `harness.rs`. Nó thành **một cột
của `benches/baselines.tsv`, theo từng (máy, case)**, và được suy ra bằng một quy tắc ghi rõ:
*bậc nhỏ nhất trong thang `1.10 · 1.15 · 1.20 · 1.25 · 1.30 · 1.35` mà ≥ max/median đo được*,
sàn là 1.10. Ba case cần biên rộng hơn thì mang biên rộng hơn **kèm lý do đã đo**, chín case còn
lại giữ 1.10.

Vì sao vẫn an toàn trước cái bẫy *"nống biên cho tới khi hết đỏ"*: biên nằm trong file dữ liệu
cạnh `n`, ngày và verdict của lần đo sinh ra nó, nên sửa biên mà không kèm phép đo mới **là một
dòng diff nhìn thấy được**, và một biên lệch khỏi thang là lệch rõ ràng chứ không lẫn vào đâu.

**Bước 1 dừng ở 21 lần chứ không 30, và lý do được ghi ra đây thay vì làm tròn thành 30:** lần
chạy thứ 22 gặp `harness.rs` đã bị sửa dưới chân nó nên không biên dịch. 21 ≥ 20, đủ theo chính
plan này. Mẫu 30 lần **với cơ chế thật** là bước 4, và baseline ghi vào TSV lấy từ mẫu đó, không
từ mẫu 21 lần này — mẫu này chỉ dùng để **chọn biên**.

## Bối cảnh

`DESIGN.md` §6 công bố các target timing tuyệt đối tính bằng nano giây. Hai trong số đó không
còn nói lên điều gì:

- **Serialise ≤ 60 ns.** Chưa máy nào tới gần: 93.8 (Apple M5) · 177.6–199.4 (Xeon container)
  · **240.5** (Ryzen 7 3700X §9, đo lại hôm nay). Plan
  [serialise-and-the-60ns-target](2026-08-31-serialise-and-the-60ns-target.md) đã đo xem
  145–240 ns đó gồm những gì và kết luận ở bước 2: gỡ **sạch** cái quét slot vẫn còn **~116 ns**
  so với 60. Bản sửa mà open item 11 đề xuất đã được viết, đo, và **đảo ngược** — dự đoán
  −36 ns, đo được **+5.2 ns**.
- **Các regression ceiling.** Open item 20 đo trên ba máy và kết luận nguyên văn: *"a
  per-machine baseline is viable, keyed on the CPU model that `scripts/check-machine.sh` now
  prints with every figure; a single absolute ceiling across the pool is not."* Ceiling
  `ring, one way` = 260 ns hiện **nằm dưới máy nhanh nhất trong ba máy** — một ceiling không
  máy nào qua là một ceiling đã ngừng nói điều gì.

**Gốc của 60 ns là điều mà plan này ghi lại vì nó đổi bản chất vấn đề.** `DESIGN.md` §4 D9
dòng 543 viết: *"This is how the fastest commercial engines reach tens of nanoseconds per
serialise, and it is why the published serialise target in §6 is 60 ns, not 150."* **60 ns là
một con số đọc được về engine khác, chưa bao giờ là phép đo của engine này.** Nó khác hẳn 150 ns
của parse, vốn §6 nói rõ là neo vào 139 ns đo thật trên Apple M5 ngày 2026-08-27.

Nói cách khác: §6 đang trộn hai loại số dưới cùng một cột. Một loại là *phép đo của mình cộng
biên*; loại kia là *lời hứa mượn của người khác*. Chỉ loại đầu mới gate được.

## Những gì đã biết chắc

Không có phỏng đoán ở mục này.

| Sự thật | Nguồn |
|---|---|
| `[đo 2026-08-31]` Máy §9 (Ryzen 7 3700X), `check-machine.sh` = **`pass 9 fail 1 unknown 1`**, hàng đỏ duy nhất là **`machine is quiet` 4–6%** (ngưỡng 3%) | `scripts/check-machine.sh`, phiên này |
| `[đo 2026-08-31]` `bench.sh` trên máy đó: **5 trong 12** case timing vượt ceiling — `walk 4 levels` 344.3/300 · `encode 1 group` 106.8/75 · `encode ExecutionReport` **240.5**/190 · `ring, one way` 272.7/260 · `ring, round trip` 526.5/500 | `$S/bench-run0.txt`, phiên này |
| `[đo 2026-08-31]` serialise **240.5 ns** khớp **240.0** đo ngày 2026-08-30 trong vòng **0.2%** | như trên; STATUS mục **Proven** |
| **Parse đạt target trên chính máy này**: 121.6 / 150, 113.3 / 145, 56.3 / 70 | như trên |
| `[đo 2026-08-30]` Ba máy đọc `ring, one way` = **260.9** (Ryzen) · **270.7–272.9** (EPYC 9V74) · **327.2–331.1** (EPYC 7763). Chênh giữa hai EPYC **21%**; trong cùng một máy **0.8–1.2%** | STATUS open item 20 |
| `[đo 2026-08-30]` Case đơn luồng giữ trong **~3%**; case qua ring mới là cái tán rộng | STATUS open item 20 |
| `[đo 2026-08-30]` Chỉnh máy theo §9 dịch median **dưới 2%**; tải cạnh tranh dịch **71%** | STATUS open item 20; `DESIGN.md` §9 |
| `[đo 2026-08-30]` Trên máy yên, còn **5–10%** số lần chạy rơi vào mode thứ hai ~324 ns **chưa giải thích được**. Năm giả thuyết đã bị đo và loại: L3, SMT, governor/boost, nhiệt, tải | STATUS open item 20 |
| Harness lấy **`best` của 7 mẫu**, mỗi mẫu 200 000 vòng, sau 10 000 vòng khởi động | đọc `crates/codec/benches/harness.rs:56–68` |
| Ceiling hiện là **hằng số viết cứng trong từng file bench**, `CEILING_*` | đọc `parse.rs`, `groups.rs`, `serialize.rs`, `dispatch.rs` |
| `harness.rs` nằm ở `crates/codec/benches/` và **được `engine` include qua đường dẫn** | `crates/engine/benches/dispatch.rs` |
| `codec` có **zero** runtime dependency, và đó là luật | `CLAUDE.md` §6 |
| `README.md`, `docs/PRD.md`, `docs/GUIDE.md` **không** trích các con số target này | `grep '150 ns\|60 ns'`, phiên này |
| `bench.sh` tách INVARIANT khỏi TIMING và `--strict` làm timing thành chí mạng | đọc `scripts/bench.sh` |

## Cách làm

**Một cơ chế, áp cho cả 12 case timing.** Không có case nào giữ hằng số viết cứng sau plan này.

### 1. Một file baseline, commit vào repo

`benches/baselines.tsv` ở gốc workspace — TSV thuần, phân tách bằng tab, `#` là chú thích:

```
# cpu model <TAB> case <TAB> baseline ns <TAB> ngày <TAB> verdict của check-machine.sh
AMD Ryzen 7 3700X 8-Core Processor	encode ExecutionReport (template)	240.5	2026-08-31	pass 10 fail 0 unknown 1
```

**Khoá là đúng chuỗi model CPU** mà `check-machine.sh` in ra — item 20 đã chỉ định chính nó, và
dùng lại nó nghĩa là khối machine đi kèm mọi figure vốn đã in sẵn khoá.

Harness đọc file bằng `include_str!`, nên **không có I/O lúc chạy** và file thiếu là lỗi biên
dịch chứ không phải một lần chạy âm thầm bỏ qua.

### 2. Harness chọn baseline theo máy đang chạy

`harness.rs` thêm:

- `cpu_model()` — Linux đọc `model name` trong `/proc/cpuinfo`; macOS đọc
  `sysctl -n machdep.cpu.brand_string`; không nhận ra thì `None`.
- `Suite::bench(name, f)` — **bỏ tham số `ceiling_ns`**. Ceiling tính ra từ bảng:
  `baseline × MARGIN`.

Ba trạng thái, và **trạng thái thứ ba không được đọc thành đạt**:

| Trạng thái | In ra | `finish()` |
|---|---|---|
| Có baseline, dưới `baseline × MARGIN` | `240.5 ns/op   baseline 240.5  x1.25 = 300` | xanh |
| Có baseline, trên | `... OVER BASELINE` | đỏ |
| **Không có baseline cho máy này** | `... NO BASELINE for 'AMD …' — thêm dòng vào benches/baselines.tsv:` kèm đúng dòng TSV để dán | **không đỏ**, nhưng đếm riêng và `bench.sh` in `cases without a baseline: N`; `--strict` coi `N > 0` là chí mạng |

Trạng thái thứ ba là thứ nguy hiểm nhất của plan này: nó là con đường để mọi thứ xanh trên mọi
máy lạ. Nó được xử lý bằng ba thứ cùng lúc — in ra rõ ràng, đếm riêng trong summary, và `--strict`
đỏ. Cộng một phép đảo ngược ở bước kiểm chứng.

### 3. `MARGIN` chọn từ phép đo, không từ khẩu vị

Item 20 đã đo sẵn thứ quyết định con số này: case đơn luồng giữ trong ~3%, nhưng hai case ring
có **mode thứ hai ~324 ns trên nền ~262**, tức **+24%**, xuất hiện 5–10% số lần chạy trên máy
yên và **chưa ai giải thích được**. Một `MARGIN` = 1.15 sẽ nhấp nháy đúng ở hai case đó.

Nên **`MARGIN` được chọn bằng cách chạy suite N ≥ 20 lần trên máy §9 và lấy con số nhỏ nhất mà
số lần nhấp nháy là 0/N**, chứ không đặt trước rồi hy vọng. Con số đó vào `harness.rs` như một
hằng số có comment nêu tên phép đo đã chọn nó.

### 4. `DESIGN.md` §6 đổi hình

Cột **Target** của các dòng timing bỏ số tuyệt đối, thay bằng *"không hồi quy quá baseline của
máy đang chạy × MARGIN"*, và bảng baseline theo máy nằm ngay dưới. Tham vọng không biến mất —
nó chuyển sang một dòng **Stretch, ghi rõ KHÔNG phải gate**, mang theo con số sàn đã đo được
(~116 ns cho serialise với hình dạng `Part` hiện tại) thay vì 60 ns mượn của người khác.

### 5. ADR-0016

Quyết định này đắt và khó đảo: nó gỡ bỏ các con số công bố mà bốn tài liệu đang trích. `CLAUDE.md`
§5 → ADR. Nội dung: vì sao target tuyệt đối ngừng nói lên điều gì, ba lựa chọn đã cân nhắc, cái
giá phải trả (bên dưới), và nó supersede phần nào của §6.

File sẽ tạo hoặc sửa: `benches/baselines.tsv` (mới), `crates/codec/benches/harness.rs`,
`parse.rs`, `groups.rs`, `serialize.rs`, `crates/engine/benches/dispatch.rs`,
`scripts/bench.sh`, `docs/DESIGN.md` §6, `docs/decisions/ADR-0016-…` (mới), `STATUS.md`,
`CHANGELOG.md`.

## Bất biến bị đụng tới

| # | Điều | Giữ bằng cách nào |
|---|---|---|
| 10 | **Không số nào không kèm bench, máy và cấu hình §9** | Đây là điều plan này phục vụ trực tiếp. Mỗi dòng `baselines.tsv` **bắt buộc** mang ngày và verdict `check-machine.sh` của lần đo. Dòng nào ghi verdict khác `pass 10 fail 0` thì §6 đánh dấu là **chưa công bố được** |
| 1 | **Không cấp phát trên hot path** | Không đụng code runtime. `benches/alloc.rs` × 3 vẫn phải 0, chạy lại ở mọi bước |
| 7 | **Không `panic!` / `unwrap()` / `expect()` trong crate thư viện** | `harness.rs` là bench, không phải crate thư viện — nhưng `finish()` vẫn `assert!` như hôm nay, đó là cách nó gate. Parser TSV dùng `split` + `filter_map`, không index thô |

Không đụng 2, 3, 4, 5, 6, 8, 9.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | **Đặc trưng hoá**: chạy `bench.sh` **N ≥ 20 lần** trên máy §9 yên, ghi median và tán của cả 12 case. Ra được `MARGIN` nhỏ nhất cho 0/N nhấp nháy | máy yên |
| 2 | `benches/baselines.tsv` + `cpu_model()` + `Suite::bench` không còn tham số ceiling. Bốn file bench bỏ hằng `CEILING_*` | 1 |
| 3 | `bench.sh` đếm và in `cases without a baseline`; `--strict` đỏ khi > 0 | 2 |
| 4 | **Đảo ngược** cả ba trạng thái (xem *Cách kiểm chứng*) | 3 |
| 5 | `DESIGN.md` §6 đổi hình + bảng baseline; ADR-0016; STATUS items 11 và 20; CHANGELOG | 4 |

**Bước 1 có quyền đổi hình bước 2.** Nếu tán trên máy yên hoá ra không cho `MARGIN` nào đạt
0/N, thì kết luận là *harness lấy `best` của 7 chưa đủ để dập mode thứ hai*, và bước 2 phải sửa
cách lấy mẫu chứ không phải nống `MARGIN` lên tới lúc nó xanh. **Nống biên cho tới khi hết đỏ là
đúng cái bệnh mà plan này sinh ra để chữa** — ghi ở đây để nó không xảy ra một cách vô tình.

## Cách kiểm chứng

- **Bước 1** — số kèm khối `check-machine.sh` mỗi lần chạy, và **`pass 10 fail 0`** trước khi
  con số nào được ghi vào `baselines.tsv`. Verdict khác thì con số vào nhật ký, không vào file.
- **Bước 4 — đảo ngược, ba lần, mỗi lần một trạng thái**, và phải đỏ vì đúng lý do:
  1. Hạ một baseline trong TSV xuống 1 ns → case đó `OVER BASELINE`, các case khác vẫn in đủ số.
  2. Xoá một dòng TSV → case đó `NO BASELINE`, `bench.sh` in `cases without a baseline: 1`,
     `--strict` đỏ, **không `--strict` thì không đỏ**.
  3. Sửa `cpu_model()` trả về một chuỗi rác → **cả 12 case** thành `NO BASELINE`, không phải
     xanh. Đây là phép đảo ngược quan trọng nhất: nó chứng minh một máy lạ không im lặng qua cửa.
- **Mọi bước** — `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all`,
  `cargo test --no-default-features`, `benches/alloc.rs` × 3 vẫn 0.
- **Trước khi đóng** — CI xanh, **nêu tên run theo id** (`CLAUDE.md` §9).

## Tài liệu phải cập nhật

- [ ] `docs/DESIGN.md` §6 — bỏ cột target tuyệt đối ở các dòng timing, thêm bảng baseline theo
      máy, thêm dòng Stretch ghi rõ không phải gate
- [ ] `docs/DESIGN.md` §4 D9 dòng 543 — câu *"why the published serialise target in §6 is
      60 ns"* trỏ tới một target không còn tồn tại. Sửa cùng commit, nếu không nó thành link chết
      về mặt nội dung
- [ ] `docs/DESIGN.md` §8 — chỉ nếu bước 1 dịch một dòng của latency budget
- [ ] `docs/decisions/ADR-0016-…` — **bắt buộc**
- [ ] `STATUS.md` — **item 11 đóng** (câu hỏi của nó được ADR-0016 trả lời); **item 20 đóng**
      (kết luận của nó chính là thứ được thi hành); mục *Start here* bỏ câu hỏi 60 ns
- [ ] `docs/plans/2026-08-31-serialise-and-the-60ns-target.md` — bước 5 và 6 đóng, ghi kết cục 3
- [ ] `CHANGELOG.md` — con số 93.8 vs 60 đang nằm ở dòng 358; ghi thay đổi cơ chế
- [ ] `docs/reference/measured-costs.md` — bảng đặc trưng hoá của bước 1 thuộc về đây
- [ ] `CLAUDE.md` §2 điều 10 — **không sửa.** Plan này thi hành điều đó chứ không nới nó

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| **Máy lạ im lặng qua cửa** vì không có baseline | Đảo ngược 3 ở bước 4: `cpu_model()` trả rác → 12/12 `NO BASELINE`, `--strict` đỏ |
| **Nống `MARGIN` cho tới khi hết đỏ** | `MARGIN` chọn ở bước 1 **trước khi** viết code bước 2, từ 0/N nhấp nháy. Nếu không có con số nào đạt thì sửa cách lấy mẫu, không nống biên — ghi sẵn ở *Chia việc* |
| Baseline ghi từ một máy chưa yên rồi thành số công bố | `baselines.tsv` mang cột verdict; §6 đánh dấu dòng nào chưa `pass 10 fail 0`. Bước 1 không ghi file khi máy chưa yên |
| Một baseline ghi từ **một** lần chạy trên máy tán 20% | Bước 1 bắt buộc N ≥ 20, lấy **median**, và ghi cả tán bên cạnh |
| `include_str!` đường dẫn sai khi `engine` include `harness.rs` qua path | `dispatch.rs` chạy trong bước 4 chứ không chỉ `codec`. Đường dẫn `include_str!` tính theo file chứa nó, nên cùng một chuỗi cho cả hai — kiểm bằng chính lần chạy |
| Bỏ target tuyệt đối đọc thành **bỏ tham vọng** | Dòng Stretch trong §6 mang con số sàn đã đo (~116 ns), và ADR-0016 nói thẳng đây là cái giá |
| Case mới thêm về sau không có baseline và không ai nhận ra | Chính trạng thái `NO BASELINE` + `--strict` là cơ chế đó. CI chạy `--strict`? **Không** — CI dùng máy chung; `--strict` là cho máy §9. Nên bước 3 phải in `cases without a baseline` **cả khi không strict**, và đó là dòng người đọc CI nhìn thấy |
| Sửa `DESIGN.md` §6 mà quên dòng 543 của D9 | Nằm sẵn trong checklist tài liệu, và `check-links.py` **không** bắt được loại này — nó chỉ kiểm link |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| **Máy §9 không yên được** trong phiên này | **Cao — đang xảy ra**, `machine is quiet` 4–6% | Bước 2–4 không cần máy yên; chúng là code và đảo ngược. **Bước 1 và bước 5 chờ**, và plan **không đóng** khi thiếu — đúng như `standard-mode` đã làm với phép đo wakeup |
| Mode thứ hai ~324 ns làm không `MARGIN` nào đạt 0/N | Trung bình | Đó là kết quả, không phải hỏng: nó nói harness cần đổi cách lấy mẫu. Bước 1 dừng ở đó và plan được sửa lại theo `CLAUDE.md` §1 |
| Bảng baseline chỉ có một máy nên §6 nghèo đi | Trung bình | M5 và hai EPYC đã có số trong STATUS item 20 — nhưng chúng đo bằng ceiling cũ, không phải quy trình này. Ghi vào TSV kèm verdict thật của chúng, hoặc để trống. **Không suy ra**, `CLAUDE.md` §10 |
| Bỏ ceiling cứng làm mất bảo vệ hồi quy trong lúc chuyển | Thấp | Bước 2 thay chứ không gỡ; giữa hai bước không có commit nào không có gate |

## Ngoài phạm vi

- **Không tối ưu gì cả.** Không đụng `template.rs`, `groups`, ring. Năm case vượt ceiling là
  vấn đề riêng của chúng và plan này chỉ đổi *cách nói về* chúng.
- **Không đụng các dòng INVARIANT** — `alloc` × 3 và `ring_full`. Đáp số của chúng giống nhau
  trên mọi máy, nên baseline theo máy không áp dụng và ceiling của chúng không phải ceiling.
- **Không đổi `check-machine.sh`.**
- **Không đổi ngưỡng 3% của hàng `machine is quiet`.** Nó đến từ phép đo của item 20.
- **Không chuyển sang Criterion.** Lý do harness tự viết vẫn còn nguyên: Criterion đo mà không
  assert.

## Nhật ký giao hàng

*(điền khi đóng từng bước)*
