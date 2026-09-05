# Phần còn lại của round trip, và một message chạm vào bao nhiêu

> **Loại:** Plan · **Ngày:** 2026-09-05 · **Trạng thái:** Đã duyệt (2026-09-05)
> **Phạm vi:** hai open item cuối còn cần máy §9 và đã sẵn sàng đo — item 49 và nửa còn mở của item 14

## Bối cảnh

Hôm nay đóng ba item (41 → 39 → 34) trên máy §9. Còn đúng hai chỗ vừa **cần** cấu hình §9
vừa **đo được ngay** (item 40 chờ dây mạng, Wave C chờ code chưa có):

**Item 49 — 2 804 ns không ai nhận.** `tools/w2w --path app` đắt hơn `--path admin`
**3 898 ns** ở p50. Các benchmark đã commit giải thích được **~1 094 ns, 28%**:
parse vào +60, **dictionary pass +679**, dispatch +9, application tự parse lại
(`Validation::NONE`) +114, encode template +233. Phần còn lại **~2 804 ns, 72%**, hiện chỉ có
bốn ứng viên **được kể tên chứ chưa cái nào được đo**. Item 39 là lời cảnh báo: ứng viên
*lớn nhất* được nêu tên hoá ra chỉ chiếm một phần sáu. Nên việc ở đây là **định giá**, không
phải suy luận thêm.

**Item 14 — một message chạm vào bao nhiêu trong 53.3 KiB.** `size_of::<Connection<…>>()` là
**54 600 byte** và đã đo. Cái *chưa* đo là mỗi message thật sự **đụng** vào bao nhiêu trong đó,
và con số ấy quyết định tường cache nằm ở đâu: **N ≈ 9** nếu chạm hết, **N ≈ 128** nếu chỉ
chạm ~4 KiB. Hai đáp án cách nhau **14 lần** và cả `GUIDE.md` §1a lẫn mọi lời khuyên về mật độ
session đều đang đứng trên khoảng trống đó.

Muốn người sau sáu tháng hiểu động cơ: đây là hai dòng cuối cùng của bảng *Open items* mà cái
máy này trả lời được. Sau plan này, phần còn lại chờ phần cứng hoặc chờ code khác.

## Những gì đã biết chắc

Không có phỏng đoán trong mục này.

**Về item 49**

- `[measured 2026-09-05]` chênh lệch `--path app` − `--path admin` = **3 898 ns** p50, máy §9.
- Đã trừ được, mỗi số có benchmark đã commit: +60 / +679 / +9 / +114 / +233 ns = **1 095 ns**.
- Bốn ứng viên còn lại, chưa cái nào có case: (1) hai lần kernel copy payload lớn hơn mỗi
  chiều — 149 byte vào và ~200 byte ra, so với 79 và ~70; (2) framing và quản lý read buffer
  của engine, không benchmark nào tách ra; (3) `Journal::put` cất `ExecutionReport` vào ring
  trong bộ nhớ — đường admin **không** làm, vì `Heartbeat` không được giữ để resend;
  (4) `read` blocking của client trả về trên message lớn hơn.
- `MemJournal::put` là một index, một `copy_from_slice` độ dài message, và một `high_water`
  ([crates/engine/src/journal.rs:136](../../crates/engine/src/journal.rs#L136)). Ring
  `MemJournal<64,512>` là 33 288 byte — slot đích gần như chắc chắn lạnh.
- `benches/turn.rs` đã có case `recv on a quiet socket` = **418.5 ns**, tức chi phí syscall
  trên máy này đã có mốc, nhưng **không có case nào so hai kích thước payload**.

**Về item 14**

- Bảng độ trễ theo working set đã đo trên chính máy này (`measured-costs.md`): 16–32 KiB →
  1.05 ns, 256 KiB → 3.11 ns, 512 KiB → 5.53 ns, 4–8 MiB → 11.5–12.0 ns, 32–64 MiB → 68–79 ns.
  **L1 → RAM là 75×.** L1d là 32 KiB.
- Trong 54 600 byte: `Session<Acceptor,64>` 8 960 B, `MemJournal<64,512>` 33 288 B. Tức
  **ring journal chiếm 61%** kích thước struct.
- `transport::Loopback` tồn tại, không có syscall, và là transport của bộ acceptance corpus —
  `crates/engine/tests/*.rs` đã có sẵn nhiều chỗ dựng session đăng nhập trên nó.
- `benches/turn.rs` sweep N = 1, 4, 16 với session **rỗi** trên TCP thật và cho ra
  481.0 / 474.0 / 481.0 ns mỗi session — **phẳng**. Một turn rỗi không phải một turn có message,
  nên con số này không trả lời câu hỏi; nhưng nó là mốc đối chứng.
- `[measured 2026-09-05, item 39]` **thêm case vào một bench binary đã có sẽ làm dịch những
  case cũ trong chính binary đó** — hai baseline `validate` dịch −2.3% và +3.6% ngược chiều
  nhau chỉ vì thêm hai case. Đây là lỗ hổng ADR-0049 chưa bịt.

## Cách làm

Hai nửa độc lập. Mỗi nửa **một bench target MỚI**, không thêm case vào target đã có — lý do ở
ngay dòng cuối mục trên: thêm case vào `turn.rs` sẽ mở lại bốn baseline của `turn.rs` và biến
một phép đo thành hai.

### Nửa A — định giá phần còn lại của item 49

**`crates/engine/benches/payload.rs`** (mới, `harness = false`) — ứng viên (1) và (4) cùng lúc.
Một cặp socket TCP loopback, một luồng, mỗi vòng lặp: gửi `in_len` byte một chiều, đọc hết,
gửi `out_len` byte chiều kia, đọc hết. Hai case với **đúng** kích thước hai đường:

- `socket round trip, 79 in 70 out` — đường admin.
- `socket round trip, 149 in 200 out` — đường app.

**Hiệu số** của hai case chính là số hạng "payload lớn hơn tốn thêm bao nhiêu ở kernel", và đó
là thứ được trừ vào 2 804 ns. Con số tuyệt đối của từng case **không** được trừ — nó chứa cả
phần đường admin đã trả rồi.

**`crates/engine/benches/journal.rs`** (mới, `harness = false`) — ứng viên (3).

- `journal put, 200-byte ExecutionReport` — `MemJournal<64,512>::put` với seq tăng dần, để mỗi
  vòng rơi vào một slot khác (đúng như engine chạy thật), không phải ghi đi ghi lại một slot
  nóng.
- `journal put, 70-byte Heartbeat` — đối chứng, và là số mà đường admin **không** trả.

Ứng viên (2) — framing và read buffer — **không** có case riêng trong plan này. Nó là **phần
dư**: sau khi trừ (1)+(3)+(4), cái còn lại được ghi lại kèm tên, không kèm phỏng đoán.

### Nửa B — một message chạm vào bao nhiêu (item 14)

**`crates/engine/benches/density.rs`** (mới, `harness = false`).

Câu hỏi là *bao nhiêu byte bị chạm*, nhưng cái quyết định thực sự là *tường cache ở đâu*. Nên
đo thẳng cái tường, đừng đo footprint rồi suy ra tường qua mô hình 78.5 ns.

- N session `Loopback` đã đăng nhập, mỗi session **có một `NewOrderSingle` chờ sẵn** mỗi turn.
- Sweep N ∈ {1, 2, 4, 8, 16, 32, 64, 128}, báo **ns mỗi message** (chia cho N).
- `Loopback` chứ không phải TCP, **cố ý và ngược với `turn.rs`**: syscall 418 ns sẽ nhấn chìm
  một hiệu ứng cache vài chục ns. Ở đây cần số hạng cache, không cần syscall.

**Thí nghiệm phân biệt — cùng một lượng việc, khác kích thước vùng nhớ.** Chạy lại nguyên sweep
với ring journal thu nhỏ: `MemJournal<8,512>` ≈ 4.2 KiB thay vì 33.3 KiB, tức `Connection` còn
~25 KiB thay vì 53.3 KiB. Message làm **y hệt** một lượng việc; chỉ vùng nhớ nhỏ đi.

| Nếu | Thì |
|---|---|
| Tường dịch sang N lớn hơn đúng theo tỉ lệ kích thước | message chạm **gần hết** struct → N ≈ 9 là đáp án |
| Tường **không** dịch | phần bị chạm là phần sống, không phải cả ring → N ≈ 128 là đáp án |
| Không thấy tường nào tới N = 128 | phần bị chạm nhỏ hơn cả cận dưới đang ghi; ghi lại đúng như thấy |

Đây là **chứng minh bằng đảo ngược** đặt vào miền cache: đổi *kích thước* một vùng mà không đổi
*việc* chạm vào nó, rồi xem con số có nghe theo không.

## Bất biến bị đụng tới

Không đụng `codec`, `session`, `engine` hay `transport` **về mặt code hot path** — plan này chỉ
thêm bench target và tài liệu. Nhưng ba điều vẫn phải đi qua:

- **#1 không cấp phát trên hot path.** `benches/density.rs` chạy engine thật; `Loopback` dùng
  `VecDeque<u8>` nên **bản thân harness có cấp phát** khi pipe lớn lên. Xử lý: mồi pipe cho đủ
  capacity trước khi tính giờ, và `benches/alloc.rs` vẫn là chỗ khẳng định hot path — không
  dùng bench mới này để nói gì về cấp phát.
- **#10 không có số nào không kèm benchmark, máy, và cấu hình §9.** Mọi con số plan này sinh ra
  đi kèm tên case, `AMD Ryzen 7 3700X`, và `pass 12 fail 0 unknown 1`.
- **#4 mode-scoped.** Các case này không nói gì về mode; câu chữ khi công bố phải nói rõ đây là
  chi phí mỗi message, không phải một tuyên bố về `hft` hay `standard`.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| A1 | `crates/engine/benches/payload.rs` — hai case round trip, khẳng định số byte đọc được đúng bằng số byte gửi trước khi tính giờ | — |
| A2 | `crates/engine/benches/journal.rs` — hai case `put`, khẳng định `get(seq)` trả đúng bytes trước khi tính giờ | — |
| A3 | Ghi baseline trên máy §9, làm phép trừ **ra giấy**, cập nhật `DESIGN.md` §8 + `measured-costs.md`, viết lại dòng item 49 với phần dư mới | A1, A2 |
| B1 | Harness trong `density.rs`: N session `Loopback` đăng nhập xong, mỗi turn mỗi session một `NewOrderSingle`, khẳng định N reply quay ra mỗi vòng | — |
| B2 | Sweep N ∈ {1,2,4,8,16,32,64,128}, ns mỗi message | B1 |
| B3 | Chạy lại sweep với `MemJournal<8,512>` — thí nghiệm phân biệt | B2 |
| B4 | Trả lời N ≈ 9 hay N ≈ 128 bằng một con số, cập nhật `measured-costs.md` Term 2, `DESIGN.md`, `GUIDE.md` §1a nếu số học dịch, đóng nửa mở của item 14 | B3 |
| C | ADR nếu có quyết định kiến trúc (dự kiến có ít nhất một: phần dư của item 49 đóng lại thế nào), `CHANGELOG.md`, `STATUS.md`, đi từng dòng bảng §4 | A3, B4 |

## Cách kiểm chứng

**Mỗi bước:** `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all`,
`cargo test --all --no-default-features`, `scripts/check-links.py`.
`check-links.py` **có trong danh sách này lần này** — lần trước nó không có trong gate của plan
và CI đỏ vì một link ADR tuyệt đối.

**Đo:** `scripts/check-machine.sh` phải in `pass 12 fail 0 unknown 1` **trước** mỗi loạt.
`scripts/bench.sh --strict`. Baseline mới ghi vào `benches/baselines.tsv` theo ADR-0016/0031:
n ≥ 20 lần chạy sạch, margin lấy nấc nhỏ nhất trên thang 1.10…1.35 ≥ max/median.
**Bỏ run đầu tiên sau khi build** — nó chưa ổn định, đã bị lần trước.

**Không chỉ "test pass":**

- A1 phải in ra số byte thực đọc được ở cả hai chiều, và số đó phải khớp `in_len`/`out_len`.
- A2 phải `get(seq)` lại và so bytes — một `put` ghi nhầm slot vẫn nhanh y hệt.
- B1 phải khẳng định **N** reply ra khỏi engine mỗi vòng lặp. Một sweep mà session chưa đăng
  nhập sẽ phẳng, nhanh, và sai.
- B3 chỉ có nghĩa nếu **lượng việc không đổi** — cùng message, cùng số turn, chỉ khác `N` const
  của journal. Ghi rõ hai binary khác nhau ở đúng một tham số.

**Chứng minh bằng đảo ngược:** với `payload.rs`, đặt `in_len` = `out_len` cho cả hai case và
hiệu số phải về ~0; nếu không thì cái đang đo không phải kích thước payload.

## Tài liệu phải cập nhật

- [ ] `docs/DESIGN.md` §6 — dòng gate cho mỗi case mới, kèm target
- [ ] `docs/DESIGN.md` §8 — bảng cộng lại của 3 898 ns, và tổng user-space nếu nó dịch
- [ ] `docs/reference/measured-costs.md` — mục cho nửa A, và **Term 2 của mục kernel bypass**
      cho nửa B (chỗ đang ghi "what is not known")
- [ ] `docs/GUIDE.md` §1a — số học mật độ session, **nếu** nửa B làm nó dịch
- [ ] `benches/baselines.tsv` — mọi case mới, kèm ghi chú header nếu có gì bất ngờ
- [ ] `docs/decisions/` — ADR cho cách item 49 được đóng hoặc thu hẹp
- [ ] `CHANGELOG.md` — nếu có API công khai nào phải mở ra để đo được (item 39 đã phải)
- [ ] `STATUS.md` — item 49 và item 14, và mục *Not proven* nếu có bullet nào hết đúng
- [ ] `docs/reference/` với dấu `[to testing-skills]` nếu bẫy tìm được là bẫy về **testing**

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Thêm case vào bench target cũ làm dịch baseline cũ trong cùng binary (item 39, −2.3%/+3.6%) | Ba target **mới**, không sửa `turn.rs`, `dispatch.rs`, `alloc.rs`; `bench.sh --strict` sẽ đỏ nếu case cũ dịch |
| Sweep của B phẳng vì session chưa đăng nhập, không phải vì không có tường | Khẳng định đếm reply mỗi vòng lặp, và `engine.connections() == N` |
| `Loopback` `VecDeque` cấp phát trong vòng tính giờ, đo bộ nhớ chứ không đo cache | Mồi capacity trước khi tính giờ; `benches/alloc.rs` không đổi và vẫn là nguồn duy nhất nói về cấp phát |
| Đọc "tường cache" từ một đường cong vốn dĩ O(N) | Chuẩn hoá ns **mỗi message**; và B3 giữ N cố định, chỉ đổi kích thước struct |
| `payload.rs` đo TCP loopback rồi bị đọc như một số về NIC | Tên case và module doc nói thẳng: loopback, không driver, không dây — item 40 mới là NIC |
| `journal put` ghi đi ghi lại một slot nóng, rẻ hơn engine thật | seq tăng dần để mỗi vòng một slot khác; `get` lại và so bytes |
| Bench build không thực sự ghim alignment (ADR-0049) | `scripts/check-bench-alignment.sh`, chạy bởi `bench.sh` |
| Máy không sạch (LM Studio, `code`, run đầu sau build) | `check-machine.sh` trước mỗi loạt, kiểm tra tải, bỏ run-01 |
| Link ADR tuyệt đối trong rustdoc làm CI đỏ | `scripts/check-links.py` nằm trong gate của **từng** bước |
| Phần dư của item 49 vẫn lớn và bị làm tròn thành "đã hiểu" | Phần dư được ghi bằng **số**, và item 49 chỉ đóng nếu nó được nhận hoặc được đặt tên lại kèm số |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Trừ xong phần dư của item 49 vẫn > 1 µs | Cao | Đó **là** một kết quả. Dòng item 49 được viết lại với phần dư nhỏ hơn và ứng viên còn lại; không đóng bằng lời |
| Không thấy tường nào tới N = 128 trong nửa B | Trung bình | Cũng là câu trả lời — nó bác cận trên N ≈ 9 và đóng được nửa mở của item 14 theo hướng ~4 KiB |
| Sweep tới N = 128 quá chậm / quá nhiều bộ nhớ để chạy 20 lần | Thấp | 128 × 53.3 KiB = 6.8 MiB, không đáng ngại; nếu thời gian chạy quá dài thì cắt N = 128 khỏi `bench.sh` mặc định và chạy riêng, **nói rõ** case nào không có trong gate |
| Hiệu số của `payload.rs` nằm trong nhiễu | Trung bình | Margin 1.10 trên hai case ~vài trăm ns cho phép thấy hiệu số vài chục ns; nếu không thì báo "không đo được ở độ phân giải này" thay vì báo một số |
| Nửa B cần sửa `engine` để dựng được harness | Trung bình | Nếu phải mở API mới → **dừng, sửa plan, xin duyệt lại** (đúng như item 39 đã phải làm với `pub fn validate`) |

## Ngoài phạm vi

- **Item 40** — NIC to NIC. Chờ dây Ethernet và máy thứ hai. Không đụng.
- **Kernel bypass** — Term 1 và Term 3 của mục bypass không được đo lại; plan này chỉ lấp
  khoảng trống Term 2.
- **Sharding** — N ở đây là session trên **một** engine. Câu hỏi M shard là của `tools/w2w`.
- **Wave C** — `w2w --interval`, `SO_BUSY_POLL`, `mlockall`, case cho `FileJournal`/`FileLog`,
  `turn.rs` ở hai giá trị `RX`, đối đầu với `matthart1983/nanofix`.
- **Tối ưu.** Plan này **đo**, không sửa cho nhanh hơn. Nếu tìm ra chỗ đắt, nó thành open item
  mới kèm số, không thành một commit tối ưu lén trong cùng nhánh.

## Nhật ký giao hàng

Chưa có. Điền khi đóng từng bước.
