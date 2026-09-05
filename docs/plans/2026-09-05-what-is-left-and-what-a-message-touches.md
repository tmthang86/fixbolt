# Phần còn lại của round trip, và một message chạm vào bao nhiêu

> **Loại:** Plan · **Ngày:** 2026-09-05 · **Trạng thái:** Xong (2026-09-05)
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

## Sửa 1 (2026-09-05, trước bước A1) — bốn kích thước payload đều phải đo lại, và hai điểm là quá ít

**Cái gì sai.** Mục *Những gì đã biết chắc* chép lại bốn con số từ dòng item 49 của `STATUS.md`
— "149 byte vào và ~200 byte ra, so với 79 và ~70". Ba trong bốn con số đó sai. Chúng chưa bao
giờ được đo; dấu `~` là lời thú nhận, và tôi đã suýt dựng hai case benchmark trên chúng.

**Đo thế nào.** `strace -f -e trace=sendto` lên chính `./target/release/w2w`, đúng cờ mặc định
(`--messages 20000 --warmup 2000`), trên máy §9, rồi lấy 2 000 lần gửi cuối của mỗi chiều.
Ở trạng thái ổn định cả 2 000 mẫu **giống hệt nhau**, không phải một phân phối:

| Đường | Vào (client → engine) | Ra (engine → client) |
|---|---|---|
| `--path admin` (`35=1` → `35=0`) | **83** | **87** |
| `--path app` (`35=D` → `35=8`) | **149** | **191** |

Đối chiếu dòng item 49: 149 **đúng**; `~200` thật ra là **191**; `79` thật ra là **83**;
`~70` thật ra là **87** — lệch 17 byte, và lệch về phía **làm cho giả thuyết payload yếu đi**,
vì chiều ra của đường admin lớn hơn tưởng.

Kích thước **trôi theo số chữ số của `34=` và của `11=`/`112=`**: cùng hai đường này ở
`--messages 20` đọc ra 77/81 và 143/179. Nên "đúng byte của w2w" chỉ có nghĩa khi kèm số message
của lần chạy — bảng trên là ở cờ mặc định, và đó là cờ mọi con số w2w đã công bố dùng.

Chênh lệch app − admin: **+66 byte vào, +104 byte ra**. Mỗi byte bị copy hai lần (vào kernel,
ra khỏi kernel), nên một round trip app copy thêm **340 byte** so với admin.

**Đổi cách làm.** Hai case là quá ít. Chênh lệch cần đọc nằm cỡ vài chục ns trên hai con số cỡ
vài µs — dưới 2%, tức sát mức nhiễu của chính harness, và một hiệu số hai điểm thì **không có
cách nào tự nói nó là thật hay là nhiễu**. `payload.rs` vì vậy có **bốn** case, không phải hai:

| Case | Vì sao |
|---|---|
| `socket round trip, 8 in 8 out` | Sàn. Gần như không có payload, nên nó là chi phí bốn syscall gần như thuần |
| `socket round trip, 83 in 87 out` | Đường admin, đúng byte đã đo |
| `socket round trip, 149 in 191 out` | Đường app, đúng byte đã đo |
| `socket round trip, 1024 in 1024 out` | Đòn bẩy. Có nó thì ra được **ns mỗi byte**, và số hạng payload trở thành một độ dốc chứ không phải một phép trừ |

Số đem trừ vào 2 804 ns vẫn là hiệu **app − admin**; hai case còn lại là thứ cho biết hiệu đó
có đáng tin không. Nếu độ dốc từ (8, 1024) dự đoán được hiệu (83/87, 149/191) thì hiệu đó là
thật; nếu không thì cái đang đo không phải kích thước payload, và điều đó phải được nói ra chứ
không được trừ.

**Hệ quả cho tài liệu.** Dòng item 49 phải sửa bốn con số của chính nó, dù kết quả đo có ra sao
— đó là một sai sót độc lập với việc benchmark trả lời gì.

## Sửa 2 (2026-09-05, trước bước A2) — 53.3 KiB không còn tồn tại, và ring journal đã ra khỏi struct

**Cái gì sai.** Mục *Những gì đã biết chắc* chép từ `measured-costs.md`:
`size_of::<Connection<…, MemJournal<64,512>, 64, 4096, 8192>>()` = 54 600 byte, trong đó
`Session<Acceptor,64>` 8 960 B và `MemJournal<64,512>` 33 288 B, kèm câu chốt **"L1d máy này là
32 KiB — một connection không lọt L1"**. Cả bốn con số đó đo ngày 2026-08-30 và **đã hết đúng
từ 2026-09-04**.

**Đo lại, hôm nay, bằng `size_of` chạy thật:**

| | 2026-08-30 ghi | Đo 2026-09-05 |
|---|---|---|
| `MemJournal<64,512>` | 33 288 B | **32 B** |
| `Session<Acceptor,64>` | 8 960 B | **9 064 B** |
| `Connection<…, MemJournal<64,512>, 64, 4096, 8192>` | 54 600 B (53.3 KiB) | **21 456 B (20.95 KiB)** |
| `Connection` đúng hình dạng w2w (`Store`, 256, 4096, 8192) | — | **23 760 B (23.2 KiB)** |

**Vì sao.** [ADR-0046](../decisions/ADR-0046-the-ring-is-the-resend-store-and-a-replay-goes-in-batches.md), commit
`6d02f3a` ngày **2026-09-04**, đổi ring thành `slots: Box<[Slot<LEN>]>` và nâng mặc định lên
4 096 slot. Ring **rời khỏi** struct: `size_of` của journal giờ là một con trỏ béo cộng
`high_water`, còn 2 MiB thật nằm trên heap ở một allocation khác.

**Ba hệ quả, và không cái nào nhỏ.**

1. **Câu "một connection không lọt L1" giờ sai.** 20.95 KiB lọt thoải mái vào L1d 32 KiB. Câu
   ấy đang nằm in đậm trong `measured-costs.md` và là tiền đề của cả mục kernel bypass.
2. **Hai cận "N ≈ 9 hoặc N ≈ 128" được tính từ 53.3 KiB**, một con số không còn tồn tại. Câu
   hỏi của item 14 vẫn nguyên giá trị — *một message chạm vào bao nhiêu* — nhưng số học phải
   làm lại từ đầu.
3. **Và bộ nhớ không biến mất, nó chuyển chỗ.** `Store` mặc định — thứ `tools/w2w` dùng — là
   `MemJournal<4096,512>` = **2 MiB mỗi connection**, trên heap. `put` địa chỉ hoá bằng
   `seq % 4096`, nên message liên tiếp đi vào slot liên tiếp và **quét tuyến tính hết 2 MiB rồi
   quay vòng**. L3 của máy này là 32 MiB, L2 là 512 KiB. Mỗi `put` chạm 512 byte = 8 dòng
   cache, gần như chắc chắn đã bị đẩy ra từ lâu. **Đây là ứng viên (3) của item 49, và nó vừa
   to hơn nhiều so với lúc được kể tên.**

**Đổi cách làm — nửa A.** `journal.rs` có **ba** case, không phải hai. Case thứ ba là phép đảo
ngược trong miền cache, và nó tách "ghi 191 byte" khỏi "ghi vào một slot lạnh":

| Case | Vì sao |
|---|---|
| `journal put, 191 bytes, next slot` | Đúng cái engine làm: seq tăng một, slot kế tiếp, quét hết 2 MiB |
| `journal put, 87 bytes, next slot` | Đối chứng kích thước — đường admin không trả khoản này, nhưng nếu chi phí là *copy* thì hai case phải lệch theo byte |
| `journal put, 191 bytes, one slot` | `seq += 4096`, luôn rơi vào slot 0. **Cùng một lượng việc, cache nóng.** Hiệu số với case đầu chính là cái ring 2 MiB tốn |

Kích thước lấy theo Sửa 1: **191** byte là `ExecutionReport` thật, **87** là `Heartbeat` thật.

**Đổi cách làm — nửa B.** Thí nghiệm phân biệt đã mô tả (`MemJournal<8,512>` so với
`MemJournal<64,512>`) **không còn đo được cái gì**: cả hai giờ là 32 byte trong struct, khác
nhau chỉ ở kích thước allocation trên heap. Nó vẫn là một thí nghiệm hợp lệ — nhưng nó đo
**ring**, không đo `Connection`. Nên nửa B tách làm hai câu hỏi thay vì một:

- **B-i:** trong 20.95 KiB của struct, một message chạm bao nhiêu → sweep N với ring **cố định
  nhỏ** (`MemJournal<8,512>` = 4 KiB heap), để ring không tham gia.
- **B-ii:** ring tốn bao nhiêu → giữ N = 1, đổi số slot của ring qua 8 / 64 / 512 / 4096.

Bước B3 cũ trở thành B-ii. Chi tiết viết lại ở bảng *Chia việc*.

**Hệ quả cho tài liệu.** `measured-costs.md` phải sửa bốn con số và một câu in đậm, **độc lập
với kết quả đo của plan này**. Và đây là một dòng *Not proven* kiểu mới: không phải một tuyên
bố sai, mà một **phép đo đúng vào ngày nó được đo** rồi bị một refactor một tuần sau âm thầm
làm hỏng, mà không tài liệu nào chỉ vào nó. `[to testing-skills]`.

## Sửa 3 (2026-09-05, trước bước B1) — `Loopback` không dùng được cho phép đo này

**Cái gì sai.** Mục *Cách làm* nửa B viết "N session `Loopback` đã đăng nhập". Đọc code thì
`transport::Loopback` giữ byte trong `std::collections::VecDeque<u8>`. Ba hệ quả, và cái thứ ba
mới là cái giết phép đo:

1. **Nó cấp phát.** `VecDeque` lớn lên bằng cách nhân đôi. `benches/alloc.rs` đã ghi lại đúng
   bẫy này ở dòng chú thích của nó — một case từng báo "1 allocation" và con số ấy là hàng đợi
   của cái fake đang phình, không phải của engine.
2. **Nó tính tiền theo từng byte.** Đẩy và lấy 149 byte qua một deque byte-một là công việc của
   harness nằm trong vùng tính giờ.
3. **Nó thêm bộ nhớ heap cho mỗi connection, và bộ nhớ đó vào cache.** Phép đo này *là* một
   phép đo cache. Dùng `Loopback` nghĩa là mỗi connection kéo theo một vùng đệm của đồ giả, rồi
   ta đo cái đồ giả đó và gọi nó là `Connection`.

**Đổi cách làm.** `density.rs` định nghĩa transport riêng của nó, `Feed`, ngay trong file bench:

- `recv` chép **một** message vào buffer của caller và trả `Io::Ready(len)`. Không cấp phát,
  không deque, chi phí là đúng một `memcpy` — cùng loại việc mà một `read()` thật làm, trừ
  syscall.
- `send` đếm byte rồi vứt. Không có hàng đợi để phình.
- Message đầu là `Logon`; từ đó trở đi là `NewOrderSingle`, **vá tại chỗ**: `34=` viết ở bề rộng
  cố định 8 chữ số, tăng có nhớ, và checksum cập nhật tăng dần thay vì tính lại. Không
  `format!`, không cấp phát, chi phí không đổi theo N.

Vá `34=` là bắt buộc chứ không phải tối ưu: `benches/alloc.rs` đã ghi `[measured 2026-08-30]`
rằng gửi lại **cùng một** message làm session từ chối số thứ tự đã dùng và bỏ link, và từ vòng
thứ ba trở đi bench đo một engine không còn connection nào.

Trạng thái thêm vào mỗi connection: một template ~200 byte và bốn số. So với `VecDeque` thì đây
là thứ nhỏ nhất có thể mà vẫn nuôi được một session thật.

**Không đổi:** vẫn là `Engine` thật, `Connection` thật, `Session` thật, parse thật, journal thật.
Cái bị thay chỉ là ống dẫn byte, và nó bị thay vì ống cũ nặng hơn thứ cần đo.

**Tên case và dải N.**

| Case | Nửa |
|---|---|
| `engine turn, {1,2,4,8,16,32,64} busy sessions` — ring 8 slot | B-i |
| `engine turn, 1 busy, ring {64,512,4096}` | B-ii |

Dải N dừng ở 64 vì số học đã đổi theo Sửa 2: `Connection` ~21 KiB cộng ring, nên L1d 32 KiB
đến ở N≈1, mép L2 512 KiB ở **N≈20**, và L3 32 MiB còn cách rất xa ở N≈1 300. Điểm 128 là
~3.2 MiB — vẫn nằm gọn trong L3, và nó là điểm cuối còn nói được điều gì.

## Sửa 4 (2026-09-05, sau lần chạy đặc tính đầu tiên) — dải N dừng ở 64, và cái đó tốn gì

**Vì sao.** Sweep tốn O(N): một lần chạy `density` mất ~7 phút, gần hết nằm ở hai điểm cuối.
Baseline cần 20 lần chạy sạch, nên giữ N=128 là **~3.5 giờ** chiến dịch, bỏ nó còn **~1.2 giờ**.
Chủ sở hữu chọn bỏ.

**Cái bị mất, nói thẳng.** N=128 là điểm gần bão hoà nhất — chỗ gần như mọi dòng cache bị chạm
đều đã bị đẩy khỏi L2 giữa hai lần thăm. Ước lượng *số byte bị chạm* dựa vào đó là vững nhất.
Không có nó, phép quy đổi từ "đắt thêm bao nhiêu ns" sang "bao nhiêu KiB" phải làm ở N=64, nơi
working set 1.63 MiB **chưa** bão hoà so với L2 512 KiB, nên tỉ lệ trượt phải ước lượng thay vì
đọc thẳng. Sai số rộng ra: khoảng **2–4 KiB** thay vì một con số.

**Cái KHÔNG bị mất, và nó mới là câu trả lời của item 14.** Cận trên "message chạm gần hết
struct → mép L2 ở N ≈ 20" bị bác **không cần điểm bão hoà nào**: nếu đúng thế thì phải có một
**bậc** ở quanh N=16–32, và đường cong ở đó chỉ nhích 2.8% rồi 5.6%, đều và không bậc. Một dốc
đều là hình dạng của tập bị chạm nhỏ nằm rải trong vùng cấp phát lớn. Cận N≈9 chết bằng hình
dạng, không bằng số học.

**Quy tắc áp dụng, không có ngoại lệ.** Lần chạy đặc tính đầu tiên *có* đo N=128. Con số đó
**không được trích dẫn ở bất kỳ đâu** sau khi case bị gỡ: bất di bất dịch số 10 đòi benchmark đã
commit sinh ra nó, và một case không còn trong repository thì không phải benchmark đã commit.
Nó ở lại trong lần chạy đã sinh ra nó và không đi đâu cả.

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
| B2 | **B-i** — sweep N ∈ {1,2,4,8,16,32,64} với ring cố định nhỏ (`MemJournal<8,512>`), ns mỗi message. Ring đứng ngoài, nên tường nào thấy được là tường của struct | B1, Sửa 2 |
| B3 | **B-ii** — N = 1, ring qua 8 / 64 / 512 / 4096 slot. Cùng một lượng việc, chỉ vùng nhớ khác. Đây là chỗ 2 MiB của `Store` bị định giá | B1, Sửa 2 |
| B4 | Trả lời câu hỏi của item 14 bằng một con số, **sửa bốn con số và một câu in đậm đã hỏng trong `measured-costs.md`**, cập nhật `DESIGN.md`, `GUIDE.md` §1a nếu số học dịch, đóng nửa mở của item 14 | B2, B3 |
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

**A1 — `crates/engine/benches/payload.rs`.** Bốn case. Trước khi viết một dòng code, `strace`
lên chính release binary cho ra bốn kích thước thật (83/87 admin, 149/191 app) và **ba trong bốn
con số của dòng item 49 là sai** → Sửa 1. Bản đầu dùng `read` blocking, đọc 12.5 µs cho bốn
syscall; mồi sẵn hàng đợi không đổi gì, và đào tiếp thì ra chuyện lớn hơn ở A-ngoài-lề dưới đây.
Hiệu hai case thật **không được trừ** — ba lần lặp cho −4, +13, +46 ns. Số dùng là độ dốc trên
đòn bẩy 8 → 8192: **0.1443 ns/byte → 24.5 ns**.

**A2 — `crates/engine/benches/journal.rs`.** Ba case. `size_of` chạy thật cho thấy
`MemJournal<64,512>` là **32 byte** chứ không phải 33 288 và `Connection` là **21 456** chứ không
phải 54 600 → Sửa 2, và ring 2 MiB nằm trên heap. `put` = **8.9 ns**. Case `one slot` là phép đảo
ngược trong miền cache; module doc nói thẳng 8.9 là **sàn**, không phải con số, vì vòng lặp chặt
với stride 512 byte là thứ prefetcher thích nhất — và giao việc thu hẹp cho B-ii.

**A-ngoài-lề.** `[measured 2026-09-05]` một `write` TCP loopback 8 byte tốn **5 450 ns** trên máy
này: 32 lần `getppid` (170.5), 7 lần một pipe (778.9), 2.8 lần một UNIX socketpair (1 924.9).
Không phải `read` chờ (0.00 `EAGAIN`/op), không phải scheduler (`taskset` khớp 0.1%), không phải
code của dự án. Netfilter (Tailscale + Docker) và mitigations của Zen 2 là ứng viên, **không cái
nào được kiểm chứng nên không cái nào được nhận**. Không ảnh hưởng kết quả A: hằng số triệt tiêu
trong phép trừ, và `strace` cho thấy hai đường gọi **44 002 `sendto` mỗi bên**. → item 51,
`docs/reference/a-loopback-write-costs-thirty-two-syscalls.md`, `[to testing-skills]`.

**B1–B3 — `crates/engine/benches/density.rs`.** `Loopback` bị thay bằng transport riêng `Feed`
(Sửa 3): `VecDeque<u8>` cấp phát, tính tiền theo byte, và cho mỗi connection một vùng đệm heap
riêng — tức đo đồ giả trong chính một phép đo cache. Ba lỗi harness, cả ba do assertion bắt:
template phải bắt đầu ở seq 1; checksum FIX dừng **trước** `10=`; và **`Engine` chặn hai session
cùng identity** — thêm hai connection thì `connections()` đọc 1, nên mỗi session phải có
counterparty riêng qua `add_with_prefix_and_config`. `benches/turn.rs` chưa bao giờ gặp cái thứ
ba vì session của nó rỗi. N=128 đo một lần rồi gỡ theo quyết định của chủ sở hữu (Sửa 4); con số
ấy **không được trích ở đâu cả**.

**Kết quả.** Item 14: dốc chứ không phải vách, **không có bậc ở N ≈ 20**, nên cận N≈9 chết bằng
hình dạng; tập bị chạm **~2–4 KiB**; ở N=64 cache thêm **13.9%** mỗi message. Ring 2 MiB tốn
**không gì đo được** (1.5%, không đơn điệu). Item 49: hai ứng viên chết với con số, phần dư
2 804 → **2 770 ns**.

**Chiến dịch baseline.** 22 lần chạy, **20 dùng được**; hai bị loại bằng verdict của chính chúng
(`gnome-shell` 31% rồi 58% một core). Trước đó năm run nữa bị hỏng vì lỗi của tôi: `pgrep -x
collect.sh` không thấy driver cũ (tên tiến trình của script bash là `bash`), nên hai chiến dịch
chạy song song và một binary `payload` mồ côi quay TCP loopback ở 74% một core suốt 28 phút.
`check-machine.sh` nêu đích danh nó trong mọi run nó chạm tới. **Không con số nhiễm nào lọt vào
`baselines.tsv`.**

**Và lần `--strict` đầu tiên đỏ**, ở `journal put, 191 bytes, one slot`: 8.2 ghi từ 20 run, rồi
6.4. `baselines.tsv` được `include!` vào `harness.rs`, nên **ghi baseline làm đổi binary mà
baseline được đo từ đó** — 23% cho một case nhỏ, alignment đã ghim mà vẫn không chặn được. Ghi
lại **6.3 / 1.35 / n = 8**, cả ba trường đều cố ý khác hàng xóm. → item 52,
`docs/reference/recording-a-baseline-changed-the-baseline.md`, `[to testing-skills]`.
`bench.sh --strict` xanh: 16/16 target, 0 silent, 0 over, 0 under, 0 thiếu baseline.

**Cái không làm.** Không tối ưu gì (đúng mục *Ngoài phạm vi*). Không đuổi theo bất thường
loopback — nó cần đổi cấu hình máy của chủ sở hữu. Không đóng item 49.
