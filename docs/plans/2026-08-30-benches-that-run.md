# Bench chạy thật, và số đọc được

> **Loại:** Plan · **Ngày:** 2026-08-30 · **Trạng thái:** Đã duyệt
> **Phạm vi:** open item 20 — gate `DESIGN.md` §6, CI

## Bối cảnh

Open item 20 nói: `benches/dispatch.rs` đặt trần theo máy M5, đỏ trên Linux, và **không có
job CI nào chạy `cargo bench`**, nên một assertion không ai thực thi chỉ là một dòng chú thích.

Chủ dự án chọn phương án "chạy `cargo bench` trong CI, advisory-only". Trước khi viết, tôi đo
lại trên chính container này. **Kết quả rộng hơn item 20 mô tả, và ở một chỗ thì ngược lại.**

Ba điều item 20 chưa nói:

1. **`codec/benches/groups.rs` cũng đỏ**, và chưa từng được ghi nhận ở đâu.
2. **`dispatch` không đỏ ổn định.** Nó xanh 4/5 lần ở đây. Cái đỏ là một case khác.
3. **Ba bench `alloc.rs` cũng chưa từng chạy trong CI.** Đó là bằng chứng máy mà `CLAUDE.md`
   §2 nêu tên cho bất biến 1 — "không cấp phát trên hot path". Nó chưa bao giờ chạy tự động.

Điều 3 là điều nghiêm trọng nhất và item 20 không hề nhắc tới.

## Những gì đã biết chắc

`[measured 2026-08-30]` Linux 6.18.44 x86_64, container 4 vCPU chia sẻ, `cargo 1.98.0`,
`cargo bench` (release). Chạy từng bench target riêng, 5 lần với hai target hay dao động.

**`cargo test --all` chạy 43 binary, không cái nào là bench target.** Kiểm bằng cách lọc
dòng `Running` — không có đường dẫn `benches/` nào xuất hiện. Không job CI nào gọi
`cargo bench`. Vậy 9 bench target chưa bao giờ chạy tự động.

> **Sửa ngày 2026-08-30, sau khi đo lại 5 lần mỗi case.** Bảng đầu tiên tôi viết ở đây
> nói `walk 4 levels` "đỏ ổn định 4/4". **Sai.** Bốn lần đầu rơi vào lúc máy đang chậm; đo
> 5 lần liên tiếp thì nó vượt trần 3/5 và lần thấp nhất là 285.0 ns — dưới trần. Không case
> nào đỏ ổn định cả. Đúng theo `CLAUDE.md` §10: một lần chạy không phải một phép đo.

Best-of-7 × 200 000 vòng mỗi case, 5 lần chạy độc lập:

| Case | Trần | min | max | Dao động | Kết luận |
|---|---|---|---|---|---|
| `parse` validated | 150 | 102.0 | 107.5 | 5% | Không bao giờ vượt |
| `parse` no checks | 145 | 97.3 | 102.0 | 5% | Không bao giờ vượt |
| `parse` Heartbeat | 70 | 52.4 | 54.4 | 4% | Không bao giờ vượt |
| `encode ExecutionReport` | 190 | 177.6 | 199.4 | 12% | **Chớp — vượt 2/5** |
| `SendingTime` từ cache | 5 | 3.4 | 3.7 | 9% | Không bao giờ vượt |
| `walk 1 group` | 60 | 50.8 | 56.8 | 12% | Không vượt (nhưng từng thấy 65.5) |
| `walk 4 levels` | 300 | 285.0 | 314.8 | 10% | **Chớp — vượt 3/5** |
| `group_members contains` | 12 | 8.9 | 10.1 | 13% | Không bao giờ vượt |
| `encode 1 group` | 75 | 72.8 | 88.5 | 22% | **Chớp — vượt 4/5** |
| `inline deliver + reply` | 15 | 3.4 | 11.3 | **232%** | Không vượt trong 5 lần; từng thấy 17.8 |
| `ring, one way` | 260 | 188.5 | 233.2 | 24% | Không vượt. Item 20 ghi 332.5 — một khoảnh khắc, không phải tính chất |
| `ring, round trip` | 500 | 339.4 | 447.3 | 32% | Không vượt |
| `alloc` × 3, `ring_full` | — | — | — | 0% | **Không phụ thuộc máy** |

**Không case nào vượt trần ở cả 5 lần.** Nghĩa là ở đây không có bằng chứng của một hồi quy
nào cả — chỉ có những cái trần đặt quá sát so với nhiễu của máy chia sẻ. Đó chính là lý do
nhóm thời gian không được phép chặn CI, và lý do sửa trần phải đợi máy §9.

**Hai khuyết tật khác nhau, item 20 gộp làm một:**

- **Một gate đỏ thật chưa ai từng thấy** — `groups` walk 4 levels, vượt trần 4/4 lần.
- **Vài trần nằm lọt trong nhiễu đo** — `inline` đặt trần 15 ns trên baseline 3.7 ns, và một
  lần bị scheduler chen vào đã cho 17.8 ns. Chính `harness.rs` đã tự cảnh báo điều này trong
  doc comment của nó: *"Baseline 2.5–4.9 ns across runs. The spread is the measurement's, not
  the code's."* Cảnh báo đã viết ra, trần vẫn đặt ở 15.

**`harness.rs` dừng ngay ở case hỏng đầu tiên.** Hệ quả đo được: lần chạy có `inline` = 17.8 ns
thì **hai số ring không bao giờ được in ra**; lần `groups` hỏng ở case đầu thì ba case sau mất.
Với CI thì đây là hỏng cấu trúc — ta chạy bench để lấy số, mà một case dao động lại nuốt mất
đúng những số cần lấy.

`crates/codec/benches/harness.rs` được cả `engine` dùng qua `#[path]`, nên sửa một chỗ là
mọi bench thời gian đổi theo.

## Cách làm

**Tách theo cái quyết định được kết quả, chứ không theo crate.**

| Nhóm | Bench | Phụ thuộc máy? | CI |
|---|---|---|---|
| Bất biến | `alloc` × 3, `ring_full` | Không — đếm cấp phát, đếm message | **Chặn (blocking)** |
| Thời gian | `parse`, `serialize`, `groups`, `dispatch` | Có | **Advisory, không bao giờ chặn** |

Nhóm bất biến chặn được vì kết quả của nó giống nhau trên mọi máy. Đây là chỗ vá lỗ hổng
bất biến 1. Nhóm thời gian không bao giờ chặn, vì trần được chỉnh theo M5.

File tạo/sửa:

- `scripts/bench.sh` — mới. Chạy **mọi** bench target, không dừng ở cái hỏng đầu tiên, in
  một bảng và thông tin máy. Cùng một script chủ dự án chạy được trên desktop Linux mới.
- `.github/workflows/ci.yml` — thêm hai job: `bench-invariants` (chặn), `bench-timings`
  (advisory, đổ số vào `$GITHUB_STEP_SUMMARY` để **có người đọc**).
- `crates/codec/benches/harness.rs` — báo cáo hết mọi case rồi mới assert ở cuối.
- `STATUS.md` — viết lại item 20 theo cái đã đo.
- `docs/DESIGN.md` §6 — ghi nhận số Linux và cách đo mới.
- `docs/reference/measured-costs.md` — số Linux đầu tiên cho các bench này.

## Bất biến bị đụng tới

- **Bất biến 1 (không cấp phát trên hot path).** Việc này **củng cố** chứ không đụng: lần đầu
  tiên `benches/alloc.rs` chạy tự động. Giữ nguyên bằng cách để nhóm bất biến chặn.
- **Bất biến 10 (không có số nào thiếu benchmark, máy, thiết lập §9).** `scripts/bench.sh`
  in máy và thiết lập cùng với số, nên một số chép ra khỏi đó vẫn mang theo nguồn gốc.
- **Bất biến 6 (feature flag).** Không đụng — không thêm feature nào.

Không đụng code `codec`, `session`, `engine`, `transport`. Chỉ đụng cách đo.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | `harness.rs` chạy hết mọi case rồi mới assert | — |
| 2 | `scripts/bench.sh` chạy mọi target, có liveness check | 1 |
| 3 | Hai job CI | 2 |
| 4 | `STATUS.md`, `DESIGN.md` §6, `measured-costs.md` | 1–3 |

## Cách kiểm chứng

- **Bước 1, phản chứng:** hạ một trần xuống dưới giá trị đo được → bench phải đỏ. Rồi hạ trần
  của case **đầu tiên** → các case sau vẫn phải in ra số. Đó chính là thứ hôm nay không có.
- **Bước 2, phản chứng:** đổi tên một bench target → script phải đỏ vì đếm thiếu, chứ không
  âm thầm chạy ít hơn rồi báo xanh.
- **Bước 3:** đọc log CI thật, không đọc màu. Bảng số phải xuất hiện trong step summary.
- Không bước nào coi là xong nếu chỉ có exit code — phải đọc output.

## Tài liệu phải cập nhật

- [x] `DESIGN.md` §6 — số Linux, và cách đo đổi
- [x] `STATUS.md` — item 20 viết lại (**không** thêm item mới: cùng một nguyên nhân,
      hai item sẽ mâu thuẫn nhau về sau), và item 11 thêm con số Linux
- [x] `docs/reference/measured-costs.md` — số Linux đầu tiên
- [x] `CLAUDE.md` §2 — danh sách "machine-checked today" phải nói đúng bất biến nào có CI canh

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Job advisory không ai đọc → đúng lại khuyết tật đang sửa | Số đổ vào `$GITHUB_STEP_SUMMARY`, hiện ngay trang run |
| Script báo xanh vì **không chạy bench nào** (đổi tên target, lỗi filter) | Script đếm target từ `cargo metadata` và đỏ nếu chạy ít hơn số đếm được |
| Sửa `harness.rs` thành in hết nhưng **quên assert** | Phản chứng bước 1: hạ trần phải làm bench đỏ |
| Ai đó thêm `continue-on-error` vào job bất biến → bất biến 1 lại hở | Không có máy canh. **Kiểm bằng tay**, ghi ở đây |
| Trần nằm trong nhiễu → gate chớp đỏ ngẫu nhiên → bị tắt đi | Không sửa trần trong plan này. Chỉ ghi lại. Sửa trần cần máy §9 |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| CI chậm thêm vì bench chạy release | Thấp | Hai job chạy song song với các job khác |
| `bench-invariants` chớp đỏ vì lý do không phải cấp phát | Trung bình | Nếu xảy ra thì đọc log rồi mới quyết, không tắt job |

## Ngoài phạm vi

- **Không sửa trần nào.** Trần sai cần máy §9 để đặt lại; đặt lại từ container chia sẻ chỉ là
  đổi một con số sai lấy một con số sai khác.
- **Không tối ưu `groups`.** Nó đỏ thật, nhưng sửa tốc độ trước khi đo trên máy thật đúng là
  cái `measured-costs.md` tồn tại để ngăn.
- **Không đổi sang Criterion.**

## Nhật ký giao hàng

**2026-08-30 — cả 4 bước xong.** Linux 6.18.44 x86_64, Xeon 2.10GHz, 4 vCPU chia sẻ,
`rustc 1.98.0`, governor và no_turbo không đọc được trong container. **Đây không phải máy §9.**

Ba chỗ plan sai, sửa theo quyền chỉnh plan giữa chừng:

1. **"Đỏ ổn định" là kết luận sai** — xem hộp trích ở mục *Những gì đã biết chắc*. Đo lại
   5 lần thì không case nào vượt trần ở cả 5.
2. **Hai job CI gộp còn một.** Script đã mang đúng ngữ nghĩa exit rồi (đỏ khi bất biến hỏng
   hoặc khi có target không đo gì; chỉ báo cáo khi thời gian vượt trần). Hai job sẽ build
   release hai lần và mở ra đúng cái bẫy `continue-on-error` mà plan đã ghi tên.
3. **Guard đếm target đầu tiên tôi viết không bao giờ đỏ được** — `ran` tăng vô điều kiện nên
   luôn bằng `EXPECTED`. Sửa thành: một target chỉ được tính khi nó **in ra phép đo**
   (`ns/op` với nhóm thời gian, output khác rỗng với nhóm bất biến).

Hai thứ tìm được mà plan không lường:

- **`benches/harness.rs` bị Cargo nhận nhầm thành bench target.** Nó không có case nào:
  `cargo bench --bench harness` in `0 measured` và exit 0 — một target luôn xanh, đo không gì.
  Tắt bằng `autobenches = false`, và nó trở thành vật liệu phản chứng cho guard ở trên.
- **`encode 1 group, 2 entries` vượt trần (79.8 / 75) và chưa ai từng thấy nó**, vì bản
  harness cũ chết ở case thứ hai của `groups` nên case thứ tư không bao giờ chạy.

Phản chứng đã chạy, cả hai đều xác nhận có injection bằng `grep` trước khi đọc kết quả:

| Phản chứng | Kết quả |
|---|---|
| Bật lại `autobenches` → phantom target quay lại | `EXIT=1`, `targets silent 1 nanofix-codec/harness` |
| Hạ trần case **đầu tiên** của `parse` xuống 50 | `EXIT=101`, và **case 2 và 3 vẫn in ra số** — bản cũ sẽ mất chúng |

Gate chạy tại chỗ, đọc output chứ không đọc exit status:

```
210 passed / 0 failed   cargo test --all
210 passed / 0 failed   cargo test --all --no-default-features
FMT OK                  cargo fmt --all -- --check
CLIPPY EXIT=0           cargo clippy --all-targets -- -D warnings
GREEN ok / RED ok       scripts/check-no-kernel-sleep.sh
8 of 8 targets measuring scripts/bench.sh — 0 invariant failures
links OK                scripts/check-links.py
```

**Chưa chứng minh:** CI thật chưa chạy lần nào với job mới. Không đóng plan cho tới khi
đọc được log CI của commit này — `CLAUDE.md` §9 ô cuối.
