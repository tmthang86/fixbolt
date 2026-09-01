# Release profile: LTO và codegen-units đáng bao nhiêu, và tốn bao nhiêu

> **Loại:** Plan · **Ngày:** 2026-09-01 · **Trạng thái:** Chờ duyệt
> **Phạm vi:** open item 13. Không đụng dòng code thư viện nào — chỉ `Cargo.toml`

## Bối cảnh

`STATUS.md` item 13: *"Release profile là mặc định. Không `lto`, không `codegen-units = 1`,
không PGO, không `#[cold]` trên đường lỗi. Rẻ, nhưng mỗi cái là một con số phải đo trước và
sau, không phải một thiết lập để mặc định là đúng."*

`[đo 2026-09-01]` `Cargo.toml` gốc **không có mục `[profile.release]`**, nên mọi số trong
`benches/baselines.tsv` được đo ở mặc định của cargo: `opt-level = 3`, `lto = false`,
`codegen-units = 16`.

Đây là đòn bẩy duy nhất còn lại trên máy này **không cần reboot nào**.

## Những gì đã biết chắc

Nền so sánh là `benches/baselines.tsv`, 20+ lượt hợp lệ mỗi dòng, trên dòng §9 của ADR-0021
với mitigation bật, `check-machine.sh` đọc `pass 12 fail 0 unknown 1`.

Mười sáu case, chia làm hai nhóm mà **bài đo mitigation vừa chứng minh là tách bạch**:

| Nhóm | Case | Đặc điểm |
|---|---|---|
| **Thuần user space** | `parse` ×3, `encode` ×2, `walk` ×2, `group_members`, `SendingTime`, `inline deliver`, `ring` ×2, `presession, read and route` | 13 case, không vào kernel |
| **Chạm syscall** | `recv on a quiet socket`, `engine turn` ×3, `presession sweep` ×2 | 6 case, `[đo 2026-09-01]` **59–63%** thời gian nằm trong kernel |

## Cách làm

Bốn nhánh, **không nhánh nào cần reboot** — chỉ sửa `Cargo.toml` và build lại:

| Nhánh | `[profile.release]` |
|---|---|
| **0** | không có mục nào (hiện tại, và là nguồn của mọi baseline) |
| **A** | `lto = "thin"` |
| **B** | `lto = "fat"` |
| **C** | `codegen-units = 1` |
| **D** | `lto = "fat"` + `codegen-units = 1` |

Mỗi nhánh: **10 lượt `scripts/bench.sh`** (không `--strict`, vì một profile chậm hơn sẽ vượt
baseline và đó là dữ liệu chứ không phải lỗi), lấy trung vị, so với `baselines.tsv`.

**Và đo cả cái giá**, vì repo này luôn muốn cả hai vế: thời gian `cargo build --release
--benches` từ sạch, cho mỗi nhánh.

PGO **ngoài phạm vi** — nó cần một vòng sinh profile, một tập tải đại diện, và một bước gộp;
đó là một plan riêng, và nó chỉ đáng viết nếu bốn nhánh trên cho thấy tối ưu hoá toàn chương
trình có tác dụng ở đây.

`#[cold]` trên đường lỗi cũng ngoài phạm vi: nó là thay đổi **code**, không phải profile.

## Bất biến bị đụng tới

Không đụng `codec`, `session`, `engine`, `transport`. Hai điều liên quan:

- **Điều 10** — máy phải ở §9 và `check-machine.sh` phải đọc `pass 12 fail 0` cho mọi lượt
  tính vào trung vị. Không reboot nên thiết lập không đổi giữa các nhánh, nhưng vẫn phải
  **đọc** chứ không giả định.
- **Điều 1** — `benches/alloc.rs` phải vẫn ra 0 ở mọi nhánh. LTO đổi cách nội tuyến; nếu một
  cấp phát xuất hiện thì đó là phát hiện, không phải nhiễu.

## Dự đoán, ghi trước khi chạy

LTO và `codegen-units = 1` mua **nội tuyến xuyên crate**. Cho nên:

| Nhóm | Dự đoán |
|---|---|
| 13 case **thuần user space** | **cải thiện**, và đây là nơi hiệu ứng nằm — `parse` và `encode` gọi xuyên `codec`/`dict`, `ring` xuyên `engine` |
| 6 case **chạm syscall** | **gần như không đổi**, vì 59–63% thời gian của chúng ở trong kernel và không tối ưu hoá user space nào chạm tới |
| thời gian build | **tăng**, và nhánh D tăng nhiều nhất |

**Đây là nhóm đối chứng, và nó đảo ngược so với bài đo mitigation:** lần đó case user space
là đối chứng, lần này case syscall là đối chứng. **Cái bác bỏ phép đo:** cả hai nhóm cùng
cải thiện một lượng như nhau — khi đó cái đổi không phải profile mà là máy trôi.

**Cái sẽ làm tôi ngạc nhiên và phải viết ra:** không nhóm nào nhúc nhích. Với 4 crate và một
đường nóng đi xuyên `codec` → `session` → `engine`, LTO *nên* làm được gì đó; nếu không thì
điều đáng ghi là **vì sao không**, chứ không phải "thử rồi, không ăn thua".

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | ✅ Ghi dự đoán (mục trên), đo lại nhánh 0 để có nền cùng phiên | — |
| 2 | ✅ Chạy bốn nhánh A–D, mỗi nhánh 10 lượt + một lần build sạch có bấm giờ | 1 |
| 3 | ✅ Chọn, hoặc không chọn. Nếu đổi profile ⇒ **ADR**, và **ghi lại toàn bộ baseline** vì mọi số cũ được đo ở nhánh 0 | 2 |
| 4 | ✅ Viết `measured-costs.md`; `STATUS.md` item 13 | 3 |

## Cách kiểm chứng

- **Mỗi lượt phải đọc `pass 12 fail 0`**; lượt nào không thì bỏ khỏi trung vị.
- **`benches/alloc.rs` phải ra 0 ở mọi nhánh**, và nó chạy trong chính `bench.sh`.
- **Nếu chọn một profile mới thì `baselines.tsv` phải ghi lại toàn bộ** — 20 lượt, theo
  ADR-0016 — vì mọi dòng hiện có mô tả một cấu hình build không còn tồn tại. Không chọn thì
  không ghi lại gì.
- **Test phải xanh ở profile được chọn**: `cargo test --all --release`.

## Tài liệu phải cập nhật

- [ ] `docs/reference/measured-costs.md`
- [ ] `STATUS.md` item 13
- [ ] `Cargo.toml` + `docs/DESIGN.md` §6, và `benches/baselines.tsv` — **chỉ nếu đổi**
- [ ] `docs/decisions/ADR-00NN` — chỉ nếu đổi

## Bẫy đã lường trước

| Bẫy | Cái canh nó |
|---|---|
| Máy trôi giữa nhánh đầu và nhánh cuối, và trôi bị đọc thành hiệu ứng | Nhóm đối chứng syscall; và nhánh 0 được đo **lại trong cùng phiên**, không chỉ lấy từ `baselines.tsv` |
| Một profile nhanh hơn ở một case, chậm hơn ở case khác, và chỉ case nhanh được kể | Bảng in **cả 16 case**, không lọc |
| LTO làm xuất hiện một cấp phát và không ai nhìn | `benches/alloc.rs` chạy trong mỗi lượt |
| Đổi profile rồi quên ghi lại baseline | Bước 3 nêu thẳng; và `bench.sh --strict` sẽ đỏ nếu profile mới chậm hơn ở đâu đó, còn nếu nhanh hơn thì nó **xanh và sai** — baseline là trần |
| Kết luận từ một lượt | 10 lượt mỗi nhánh, trung vị |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Thời gian build tăng gấp nhiều lần, làm chậm mọi vòng lặp sau này | Trung bình | Đo và đưa vào quyết định; CI cũng trả cái giá đó |
| Kết quả nhỏ và không đáng đổi | Trung bình | Vẫn là câu trả lời, và nó gỡ item 13 khỏi trạng thái "chưa ai đo" |
| ~2 giờ máy | Chắc chắn | Không cần reboot, nên không tốn thao tác của chủ máy |

## Ngoài phạm vi

- **PGO** — cần vòng sinh profile và một tập tải đại diện; plan riêng, và chỉ nếu bốn nhánh
  này cho thấy tối ưu hoá toàn chương trình có tác dụng.
- **`#[cold]`** — thay đổi code, không phải profile.
- **`panic = "abort"`** — đổi ngữ nghĩa, không phải tốc độ, và §2 điều 7 đã cấm `panic!` trong
  crate thư viện rồi.

## Nhật ký giao hàng

**2026-09-01 — xong, và câu trả lời là *giữ nguyên mặc định*.**

Bốn nhánh, 10 lượt mỗi nhánh, không reboot nào. `lto="fat"`: **−2.9% đến −5.6%** trên nhóm
chạm syscall, tới **−31%** trên một hàm thuần. `codegen-units=1`: −0.1% đến −16.6%. Build sạch
**5.2 s → ~16 s**. Dự đoán ghi trước đúng hướng và **nói nhẹ phần syscall** — 3–6% chứ không
phải "gần như không đổi".

**Rồi quyết định đi ngược lại bảng số**, vì hai điều bảng không cho thấy:

1. **Cargo chỉ đọc `[profile.*]` từ package cấp cao nhất đang được build.** Profile ở đây chạm
   tới benchmark của chính workspace này và `tools/w2w`, **không** chạm tới ai phụ thuộc vào
   các crate này. Đặt nó vào là làm số công bố đẹp hơn mà không làm chương trình của ai nhanh
   hơn — đúng hình dạng mà non-negotiable 10 sinh ra để chặn. (Đây là hành vi có tài liệu của
   cargo, **không phải phép đo ở đây**, và ADR dán nhãn đúng như vậy.)
2. **Một phần mức lợi là hiện vật của việc đo.** Benchmark là một crate riêng, nên LTO nội
   tuyến ruột thư viện vào **vòng lặp benchmark**. `presession, read and route` giảm 83.4 →
   57.7 ns, nhưng ở production `Shards::hand` gọi `identity_of` trong cùng crate, vốn đã nội
   tuyến được. `recv` giảm 2.9% trên một case 94% thời gian nằm trong kernel — 12 ns trên
   khoảng 25 ns công việc user space.

[ADR-0024](../decisions/ADR-0024-the-workspace-keeps-the-default-release-profile.md) giữ mặc
định và đưa dải số vào `GUIDE.md`, nơi profile của **người đọc** thực sự có tác dụng.

**Và bài này là thứ tìm ra open item 25.** `inline deliver + reply` đọc 1.3 ns ở ba nhánh và
7.4–8.6 ở hai nhánh; đọc thô thì đó là *"`codegen-units=1` làm chậm dispatch 6 lần"* — hợp lý,
đáng báo động, và hoàn toàn sai. **1.3 ns không copy nổi 163 byte** (125 GB/s từ một lõi), và
phép chia đó là thứ tìm ra nó, chứ không phải cổng nào cả.

**Bẫy "một profile nhanh hơn ở case này, chậm hơn ở case khác" đã xảy ra thật:**
`SendingTime from the cache` **+12.2%** dưới fat LTO. Bảng in cả 18 case, không lọc.

**Gate:** `fmt` sạch · `clippy` sạch · `cargo test --all` **272/0** · `bench.sh --strict`
**OK** · `check-machine.sh` `pass 12 fail 0 unknown 1` · `benches/alloc.rs` ra 0 ở mọi nhánh.
