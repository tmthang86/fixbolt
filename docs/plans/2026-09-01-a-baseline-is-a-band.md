# Một baseline là một dải, không phải một trần

> **Loại:** Plan · **Ngày:** 2026-09-01 · **Trạng thái:** **ĐÃ ĐÓNG 2026-09-01**
> *(tự viết và tự duyệt theo uỷ quyền thường trực ghi ở STATUS.md "Start here", 2026-08-30, và
> chỉ thị của chủ sở hữu ngày 2026-09-01.)*
>
> **Phạm vi:** `STATUS.md` open item 25. Chạm `crates/codec/benches/harness.rs`,
> `benches/baselines.tsv` (chỉ phần header), `scripts/bench.sh`. **Không chạm một dòng code
> engine nào.**
>
> **Máy chạy:** chọn được toàn bộ trên macOS. Mọi gate của plan này là *hành vi của bộ so
> sánh*, không phải một phép đo thời gian — và nó được chứng minh bằng cách nạp baseline giả
> vào bộ so sánh, không bằng cách chạy benchmark thật.

## Bối cảnh

`[measured 2026-09-01]` `inline deliver + reply` công bố **1.3 ns** suốt một ngày trong khi
việc thật là **8.5 ns**: `out` được ghi mỗi vòng và không ai đọc, nên trình tối ưu xoá luôn
một phép chép 163 byte. Nó bị bắt bằng **số học trong một thí nghiệm không liên quan** — 163
byte trong 1.3 ns là 125 GB/s từ một lõi — chứ không bằng một cổng nào.

**Vì sao không cổng nào bắt được.** `benches/baselines.tsv` so với `baseline × margin`, tức
**một cái trần**. Một case bắt đầu đo-không-gì đọc ra số **thấp hơn nhiều** so với giới hạn và
**đi qua — mãi mãi, và mỗi ngày một thoải mái hơn.**

**Đây là hình dạng thứ ba của cùng một lỗi**, và ba lần trong ba hình dạng là lý do nó có item
riêng:

| Lần | Cái gì nhanh lên | Cổng nói gì |
|---|---|---|
| Một cài đặt máy ngoài §9 ([ADR-0021](../decisions/ADR-0021-nohz-full-leaves-section-9.md), [ADR-0023](../decisions/ADR-0023-section-9-records-the-cpu-mitigations.md)) | mọi con số | xanh |
| Một benchmark thôi không đo nữa ([a-benchmark-can-delete-its-own-work](../reference/a-benchmark-can-delete-its-own-work.md)) | một con số | xanh |
| Một guard cấp phát đo cửa sổ không chứa thao tác ([the-guard-measured-a-window…](../reference/the-guard-measured-a-window-that-excluded-the-thing.md)) | 0 cấp phát | xanh |

Hai cái đầu đã đóng. Cái này là cổng thời gian, và nó là cái khó nhất — vì **một sàn ngây thơ
biến mọi tối ưu hoá thật thành cổng đỏ.**

## Cái chốt của thiết kế: "dưới sàn" KHÔNG phải là đỏ

Nếu sàn làm gate đỏ thì mọi lần tối ưu thành công đều phải sửa gate trước khi merge, và người
ta sẽ học cách nới margin. Đó là cách hỏng một cổng chắc chắn nhất.

Nhưng **hai nguyên nhân của "dưới sàn" cần đúng một hành động giống nhau**:

| Nguyên nhân | Hành động đúng |
|---|---|
| Tối ưu hoá thật | **Ghi lại baseline.** Nếu không, cái trần cũ rộng hơn thực tế và không còn canh gì |
| Benchmark thôi không đo | **Sửa benchmark** |

Cả hai đều cần một con người, và **không cái nào được im lặng đi qua.** Nên:

> **Dưới sàn được BÁO CÁO, đếm riêng một dòng, và `bench.sh --strict` làm nó chí mạng** —
> đúng cách `NO BASELINE` đang được xử lý hôm nay.

Không có `--strict`, CI trên pool dùng chung vẫn không đỏ vì một máy lạ. Có `--strict`, tức là
một buổi đo cố ý trên máy §9, nó phải được giải quyết.

**Lợi ích phụ, và nó lớn hơn cái chính.** ADR-0016 tự ghi trong Consequences: *"Baselines go
stale. A real speed-up leaves the baseline generous until somebody re-records."* Cái sàn
**biến việc ghi lại baseline thành tự động được nhắc**, thay vì phụ thuộc vào trí nhớ.

## Sàn đặt ở đâu

`floor = baseline / margin`, **dùng chính margin đang có.**

Vì sao đối xứng là đủ và không cần đo lại: `margin` là `max/median` trên `n` lần chạy — độ
tản của case đó. Một số đo rơi **dưới** `median / margin` là ra ngoài độ tản đã ghi của chính
nó theo hướng còn lại.

**Chỗ này thiên lệch, và plan nói ra thay vì giấu:** giá trị đo là `best` (min qua các vòng),
còn baseline là *median* qua `n` lần chạy — nên phân bố dưới median có thể rộng hơn trên. Do
đó sàn `baseline / margin` sẽ **báo động nhầm đôi khi**. Điều đó chấp nhận được **chỉ vì dưới
sàn là báo cáo chứ không phải đỏ**, và cách giải quyết một báo động nhầm là ghi lại baseline —
đúng việc phải làm. Một cột `low_margin` riêng, đo từ cùng `n` lần chạy, là bản v2 và nó cần
một buổi đo trên máy §9; **plan này cố ý không giả vờ có dữ liệu đó.**

## Chia việc

| Bước | Kết quả | Máy |
|---|---|---|
| 1 | **Test đặc tả, đỏ trước**: bộ so sánh, nạp baseline giả, phải phân biệt *trong dải* / *trên trần* / *dưới sàn*. Đỏ vì hôm nay không có khái niệm "dưới sàn" | Mac |
| 2 | Tách logic so sánh thành hàm thuần `verdict(best, baseline, margin) -> Verdict`, ba nhánh. Bước 1 xanh | Mac |
| 3 | `harness.rs` in `UNDER BASELINE`, đếm riêng như `missing`; **không** assert | Mac |
| 4 | `scripts/bench.sh` đọc dòng đếm mới; `--strict` làm nó chí mạng | Mac |
| 5 | ADR-0031 sửa mô hình của ADR-0016; header `baselines.tsv`; `DESIGN.md` §6 | Mac |

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 1 | `cargo test -p fixbolt-codec --lib` (hoặc test của harness) | **đỏ**, và thông điệp nói *không có nhánh dưới sàn* |
| 2–3 | như trên | xanh, ba nhánh, mỗi nhánh một test |
| 3 | `cargo bench` | mọi case hiện có **không** đọc `UNDER BASELINE` trên một máy có baseline |
| 4 | `scripts/bench.sh` | chạy được; dòng đếm mới xuất hiện khi có case dưới sàn |
| mọi bước | `cargo test --all`, `--no-default-features`, clippy `-D warnings`, `fmt` | xanh |

**Đảo ngược, bắt buộc:**

1. Cho `verdict` luôn trả `InBand` → cả ba test nhánh phải đỏ.
2. Đặt một baseline giả cao gấp mười cho một case thật → `cargo bench` phải in `UNDER
   BASELINE` cho đúng case đó và **không cho case nào khác**, rồi khôi phục.
3. `bench.sh --strict` với case đó → phải thoát khác 0.

**Cái bẫy lớn nhất của chính plan này:** một test của bộ so sánh mà không có case *trong dải*
sẽ xanh với một `verdict` trả bừa. Mỗi nhánh phải có case của nó, và **case trong dải là chốt
chặn** — đúng bài học của
[silence-before-a-logon-has-many-causes](../reference/silence-before-a-logon-has-many-causes.md).

## Tài liệu phải cập nhật

- [ ] ADR-0031 — baseline là một dải; sửa mô hình quyết định 1 của ADR-0016
- [ ] `benches/baselines.tsv` header — nói margin giờ dùng cho cả hai hướng
- [ ] `DESIGN.md` §6 — hàng nói cổng thời gian được đo thế nào
- [ ] `STATUS.md` item 25
- [ ] Đi lại bảng §4 từng dòng, và đọc lại *Not proven* từng dòng

## Ngoài phạm vi

- **Cột `low_margin` đo riêng** — v2, cần `n ≥ 20` lần chạy trên máy §9.
- **Bắt benchmark phải tiêu thụ kết quả bằng kiểu** (`bench` nhận closure trả giá trị) — sửa
  *nguyên nhân* chứ không phải *lớp*, đáng làm, và là plan riêng vì nó đụng mọi file bench.
- **Ghi lại mọi baseline** — cần máy §9.

## Nhật ký giao hàng

> Điền khi đóng từng bước.

### Đóng · 2026-09-01

**Cả năm bước xong, và plan sai một chỗ về máy — sẽ nói ở dưới.**

**Bước 1, đỏ trước, output nguyên văn:**

```
test a_figure_inside_the_band_is_a_pass ... ok
test a_figure_over_the_ceiling_is_a_regression ... ok
test a_figure_under_the_floor_is_neither_a_pass_nor_a_regression ... FAILED
test every_figure_lands_in_exactly_one_branch ... FAILED

assertion `left == right` failed: 1.3 ns/op against a baseline of 8.5 is the real
defect from open item 25 ...
  left: InBand
 right: Under
```

Hai control xanh ngay từ đầu — **đó là điều làm cái đỏ có nghĩa.**

**Bước 2–3.** `verdict` thêm nhánh `Under`; 4/4 xanh. `harness.rs` in dải `[floor, ceiling]`,
đánh dấu `UNDER BASELINE`, đếm riêng một dòng, và **không** đẩy vào `over` (thứ mà `finish`
assert).

**Chứng minh qua harness thật, không chỉ qua hàm thuần** — nạp ba baseline giả cho CPU này:

```
parse NewOrderSingle (validated)   70.3 ns/op  baseline 10.0 x1.10 = [9.1, 11.0]    OVER BASELINE
parse NewOrderSingle (no checks)   67.2 ns/op  baseline 300.0 x1.10 = [272.7, 330.0] UNDER BASELINE
parse Heartbeat (validated)        32.8 ns/op  baseline 31.0 x1.10 = [28.2, 34.1]
cases under their baseline: 1  parse NewOrderSingle (no checks): 67.2 ns/op is below 272.7 ns ...
thread 'main' panicked: 1 of 3 case(s) over the machine baseline
```

Trần vẫn làm panic, **sàn thì không** — đúng thiết kế.

**Bước 4, qua `scripts/bench.sh`, đảo ngược đầu-cuối:**

```
cases under the band 1  fixbolt-codec/parse
1 case(s) came in under their baseline: either re-record it,
OK                                       ← non-strict exit=0, đúng: đây là báo cáo
```

Khôi phục: `cases under the band 0`.

#### Plan sai chỗ nào: `bench.sh` chưa bao giờ chạy được trên Mac

Plan viết *"chọn được toàn bộ trên macOS"*. Bước 4 suýt không kiểm chứng được:
`[measured 2026-09-01]` `scripts/bench.sh` dùng `mapfile`, là bash 4+, mà macOS ship bash
**3.2** — script chết ở dòng 58 với `mapfile: command not found` **trước khi đo bất cứ thứ
gì**. Nó là **cổng Linux-only thứ tư** mà không ai biết.

Đã sửa thành vòng `while read` trong cùng commit, vì không có nó thì bước 4 không có bằng
chứng. Kết quả: **`bench.sh` chạy trên máy phát triển lần đầu tiên** — `targets measuring 10 of
10`, `targets silent 0`, `invariant failures 0`. **Con số ns của nó vẫn vô giá trị ở đây**
(`check-machine.sh` nói thẳng trên dòng của nó); *hành vi* của nó thì độc lập máy và đã không
kiểm chứng được vì một lý do chẳng ra gì.

#### Chưa chứng minh, và không nhận là đã

- **Nửa chí mạng của quyết định này chưa chạy.** `bench.sh --strict` trên máy này thoát 1 ở
  cổng §9 **trước khi** tới nhánh dưới-sàn (`FAIL: --strict, and this machine is not set up to
  DESIGN.md §9`, exit 1). Nhánh `--strict` cho `under_baseline` cần một máy §9 Linux. CI cũng
  không chạy `--strict` — câu hỏi mở 3 của ADR-0031.
- **Không con số ns nào công bố.** 21 case vẫn `NO BASELINE` trên CPU này.
- Cột `low_margin` đo riêng: ngoài phạm vi, cần `n ≥ 20` lần chạy trên máy §9.

**Gate đã chạy:** `cargo test --all` **58 binary, 288 passed, 0 failed**; `--no-default-features`
**288 passed, 0 failed**; `fmt` sạch; clippy `-D warnings` sạch ở cả ba cấu hình feature;
`check-links.py` 682 link, không link chết; `bench.sh` OK.
