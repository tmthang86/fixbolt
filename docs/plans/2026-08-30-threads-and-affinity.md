# Engine tự quản thread và ghim core, theo nếp HFT

> **Loại:** Plan · **Ngày:** 2026-08-30 · **Trạng thái:** **Đã duyệt 2026-08-30**
> **Phạm vi:** `engine` — public API, mô hình thread. Không đụng `codec`, `session`.
>
> **Sửa 2026-08-30, ghi tại chỗ vì plan vừa được duyệt cùng ngày:** mọi thứ trong plan này
> thuộc về **`hft` mode** của [ADR-0013](../decisions/ADR-0013-two-modes-standard-and-hft.md),
> không phải mặc định của engine. `standard` mode — chặn trên readiness, chạy mọi nền tảng,
> không ghim core — là một plan riêng và là **mặc định mới**. Việc ghim core do đó là
> **opt-in**, và các phép từ chối ở nguyên tắc 4 chỉ áp trong `hft`.

## Bối cảnh

[ADR-0012](../decisions/ADR-0012-latency-first-and-one-session-per-polling-thread.md) đã chốt:
latency thắng session density, hình dạng mặc định là **một session một polling thread**. Nhưng
engine hiện **không thực hiện được điều nó tuyên bố**:

- `DESIGN.md` D8 viết *"the engine thread is pinned to an isolated core"*. `[đo 2026-08-30]`
  `grep` `sched_setaffinity` / `affinity` / `core_affinity` / `libc` trong `crates/` và `tools/`
  trả về **rỗng**. Không có gì ghim. Đây là open item 21 — một khẳng định trong tài liệu mà code
  không thực hiện.
- Engine không shard. `Engine` giữ `Vec<Connection>` phẳng, `turn()` quét hết, `run()` là
  `loop { turn() }`. `GUIDE.md` §1a hiện phải nói với người dùng rằng **họ tự lo** việc chia
  shard, chuyển socket qua thread, ghim core và cô lập core.

Chủ dự án quyết định 2026-08-30: **engine phải cho người dùng chọn số thread và số core để ghim.**
Đây là plan cho việc đó.

**Điều plan này KHÔNG làm là quan trọng ngang việc nó làm**: nó không hứa engine sẽ nhanh hơn.
Ghim core không gỡ được 703 ns mỗi syscall. Nó làm cho *một* thứ trở thành sự thật — rằng một
polling thread có một core cho riêng nó — và làm cho việc đó **kiểm chứng được** thay vì được
giả định.

## Những gì đã biết chắc

Số đo, đọc từ code, và ràng buộc. Không có phỏng đoán ở mục này.

| Sự thật | Nguồn |
|---|---|
| Một lượt quét rỗi = `N × 703 ns`, phẳng từ N=1 tới N=256 | `[đo 2026-08-30]` [measured-costs.md](../reference/measured-costs.md) |
| 353.8 ns trong đó là vào/ra kernel không làm gì | như trên |
| `size_of::<Connection<…,64,4096,8192>>()` = **54 600 B (53.3 KiB)**; `L1d` = 32 KiB | như trên |
| Chi phí một lần chạm bộ nhớ: **1.05 ns** (L1) → **78.5 ns** (RAM), 75× | như trên |
| Head-of-line: `(k−1) × ~465 ns` với `k` session cùng có tin | như trên |
| `Engine` giữ `Vec<Connection>`; `turn()` quét hết; thread duy nhất crate tạo là writer của journal | đọc `crates/engine/src/lib.rs`, `journal.rs:224` |
| `Acceptor` tách rời `Engine`; `Engine::add(transport) -> ConnId` | `crates/engine/src/lib.rs` |
| `engine` **không có dependency ngoài nào** hôm nay | `crates/engine/Cargo.toml` |
| Máy §9 đang cô lập `6,7,14,15`; `/sys/devices/system/cpu/isolated` đọc được không cần quyền | `[đo 2026-08-30]` |
| `cpu6` ↔ `cpu14`, `cpu7` ↔ `cpu15` là cặp SMT sibling; đọc được ở `topology/thread_siblings_list` | `[đo 2026-08-30]` |
| `scaling_cur_freq` **đóng băng** trên lõi `nohz_full` — không dùng để kiểm chứng ghim | `[đo 2026-08-30]` |
| `scripts/check-no-kernel-sleep.sh` quy syscall theo **tid**, nên thread phụ không làm nó đỏ nhầm | đọc script |

## Cách làm

Bốn nguyên tắc, lấy từ nếp HFT, và mỗi cái đều kiểm chứng được chứ không phải khẩu hiệu.

**1. Người dùng nêu **id core cụ thể**, engine không bao giờ tự chọn.**
Ý niệm "core đang rảnh" của OS là sai trong bối cảnh này: nó không biết `isolcpus`, không biết
IRQ của NIC nằm đâu, không biết hai id là hai luồng của cùng một lõi vật lý. Tự chọn là cách
tạo ra một hệ thống trông như đã ghim.

**2. Ghim **từ bên trong thread**, ngay khi nó bắt đầu, rồi **đọc lại để xác nhận**.**
`sched_setaffinity(0, …)` gọi bởi chính thread đó, trước khi làm bất cứ việc gì, rồi
`sched_getaffinity` đọc lại và so. Một lời gọi trả `Ok` mà mask không đúng là đúng loại lỗi
`CLAUDE.md` §10 gọi tên: *"một kết quả xanh được suy ra chứ không phải quan sát được thì không
phải kết quả"*.

**3. Hỏng thì **dừng ở khởi động**, không bao giờ chạy tiếp mà không ghim.**
Một tiến trình "low latency" chạy không ghim tệ hơn một tiến trình báo lỗi, vì nó trông vẫn ổn.
Lỗi là enum có kiểu, trả về từ hàm dựng — **không panic** (§2 rule 7).

**4. Engine **từ chối** cấu hình mà chính nó biết là sai**, không chỉ cảnh báo:

| Từ chối | Vì sao | Đọc ở đâu |
|---|---|---|
| Core không tồn tại / không online | ghim vào hư vô | `/sys/devices/system/cpu/online` |
| Hai shard trên hai SMT sibling của cùng lõi vật lý | chúng chia nhau một lõi, "một core mỗi thread" thành dối trá | `topology/thread_siblings_list` |
| Core không nằm trong `isolcpus` | scheduler sẽ đặt việc khác lên đó; §9 dòng `isolcpus` tồn tại vì lý do này | `/sys/devices/system/cpu/isolated` |

Điều thứ ba **có thể bỏ qua bằng một cờ tường minh** (`allow_unisolated`), vì môi trường dev
không có `isolcpus` và bắt buộc thì không chạy test được. Nhưng mặc định là **từ chối**, và cờ
đó xuất hiện trong bất kỳ thứ gì engine báo cáo về chính nó.

**Thread nào cũng phải có chỗ, kể cả thread không phải engine.** Ghim engine vào lõi cô lập rồi
để writer của journal trôi nổi là tự phá sự cô lập — nó có thể rơi đúng lên lõi đó. Nên
`JournalConfig` và thread tiêu thụ của `RingDispatch` cũng nhận affinity, và tài liệu nói rằng
để trống là một lựa chọn chứ không phải mặc định vô hại.

**Hình dạng API** (phác thảo, sẽ chốt ở bước 1):

```rust
pub struct CoreId(pub usize);

pub enum AffinityError {
    NotSupported,               // không phải Linux, hoặc feature tắt
    NoSuchCore(CoreId),
    NotOnline(CoreId),
    NotIsolated(CoreId),        // bỏ qua được bằng allow_unisolated
    SmtSiblingOf(CoreId, CoreId),
    Denied,                     // setsockopt trả EPERM
    ReadbackMismatch,           // đặt xong đọc lại không khớp
}

pub struct ShardPlan {
    shards: Vec<CoreId>,        // một shard một core; số shard = độ dài
    journal_core: Option<CoreId>,
    consumer_cores: Vec<CoreId>,
    allow_unisolated: bool,
}

impl ShardPlan {
    pub fn validate(&self) -> Result<(), AffinityError>;  // trước khi tạo thread nào
}
```

Gán session vào shard là **của người gọi**: một trait với bản mặc định round-robin. HFT thật
chia theo đối tác chứ không chia đều, và engine không biết đối tác nào quan trọng.

**File sẽ tạo hoặc sửa:**

- `crates/engine/src/affinity.rs` — mới, sau `#[cfg(feature = "affinity")]`
- `crates/engine/src/shard.rs` — mới, runtime nhiều engine
- `crates/engine/src/lib.rs` — khai báo mod có `#[cfg]`, export
- `crates/engine/src/journal.rs` — affinity cho writer thread
- `crates/engine/Cargo.toml` — feature `affinity`, dep `libc` chỉ trong feature đó
- `crates/engine/tests/affinity.rs` — mới
- `docs/decisions/ADR-0013-…` — mới, bước 1
- `docs/DESIGN.md` D8, §3 · `docs/GUIDE.md` §1a · `docs/PRD.md` · `STATUS.md` item 21
- `CHANGELOG.md` — public API đổi

## Bất biến bị đụng tới

| # | Đụng thế nào | Giữ bằng cách nào |
|---|---|---|
| **1** — không cấp phát trên hot path | `ShardPlan` dùng `Vec`, và việc tạo thread cấp phát | Toàn bộ nằm ở **khởi động**, không phải hot path. `benches/alloc.rs` thêm case chạy `turn()` trên engine đã shard và phải vẫn ra **0** |
| **4** — engine thread không ngủ trong kernel | Tạo thread gọi `clone`; `sched_setaffinity` là syscall | Cả hai chỉ ở khởi động. `check-no-kernel-sleep.sh` quy syscall **theo tid** nên thread phụ không làm nó đỏ nhầm — nhưng phải **chạy lại và đọc output**, không suy ra |
| **6** — feature phải gate chính `mod` | `affinity` là feature mới | `#[cfg(feature = "affinity")] mod affinity;` trong `lib.rs`, **không chỉ trong `Cargo.toml`**. Job CI `no-default-features` là thứ canh. Đây là bẫy `CLAUDE.md` §10 liệt kê sẵn |
| **7** — không `panic`/`unwrap`/`expect` | Code mới đọc `/sys`, parse số, gọi syscall | Mọi đường trả `Result` với enum không trường. Lint workspace là thứ canh |
| **8** — `unsafe` cần kế hoạch và bình luận nêu thứ chứng minh nó đúng | `sched_setaffinity` qua `libc` là `unsafe` | Một khối `unsafe` duy nhất, bao quanh đúng lời gọi, kèm bình luận nêu tên test đọc lại mask. Không có `unsafe` nào khác |
| **3** — 59 định nghĩa là cổng của session layer | Không đụng `session` | Vẫn phải chạy 59/59, in-process và qua socket |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | **ADR-0013** — mô hình thread và affinity: id tường minh, ghim từ trong thread, đọc lại xác nhận, từ chối khi sai, ai sở hữu việc gán shard. Chốt hình dạng API | ADR-0012 được ký |
| 2 | `affinity.rs` sau feature: `CoreId`, `AffinityError`, đặt + **đọc lại**, `libc` chỉ trong feature. Test: đặt rồi đọc lại khớp; core sai trả `Err` chứ không panic | 1 |
| 3 | Các phép từ chối: không online, SMT sibling, không cô lập (+ `allow_unisolated`). `ShardPlan::validate()` chạy **trước khi tạo thread nào** | 2 |
| 4 | `shard.rs`: M engine, M thread, mỗi thread ghim rồi `run()`. Gán session mặc định round-robin, thay thế được. Accept ở một chỗ, socket chuyển qua channel | 3 |
| 5 | Affinity cho writer của journal và thread tiêu thụ `RingDispatch` — thread nào cũng có chỗ | 4 |
| 6 | Đo: thời gian một lượt quét theo số session **mỗi shard**, và tổng theo số shard. Đối chiếu với `N × 703 ns`. Cập nhật `DESIGN.md` §8 bằng số đo được của `Engine::turn` thật, thay cho sàn từ chương trình C | 4, máy §9 |

## Cách kiểm chứng

Từng bước, và **đọc output chứ không đọc exit code**.

- **Bước 2 — ghim thật sự xảy ra.** Test đặt affinity rồi gọi `sched_getaffinity` đọc lại và so
  mask. **Đảo ngược**: bỏ lời gọi đặt, test phải đỏ. Không dùng `scaling_cur_freq` để kiểm —
  `[đo 2026-08-30]` nó đóng băng trên lõi `nohz_full` và sẽ nói dối.
- **Bước 2 — thread thật sự ở lại core đó.** Đọc trường `processor` trong
  `/proc/self/task/<tid>/stat` nhiều lần trong lúc chạy; phải luôn là core đã ghim. Đây là quan
  sát, khác với việc tin lời gọi trả `Ok`.
- **Bước 3 — từng phép từ chối được chứng minh bằng một ca hỏng.** Core không tồn tại, hai shard
  trên sibling, core không cô lập. Mỗi ca một test, và **mỗi ca phải đỏ vì đúng lý do đó**, kiểm
  bằng cách so biến thể lỗi chứ không chỉ so `is_err()`.
- **Bước 4 — cổng 59 vẫn xanh.** `cargo test -p fixbolt-session --test score` và
  `-p fixbolt-engine --test wire`, cả hai. Thêm một lần chạy `wire` **qua shard runtime** —
  cùng 59 định nghĩa, đi qua đường mới.
- **Bước 4 — `benches/alloc.rs` vẫn 0.** Thêm case `turn()` trên engine đã shard.
- **Bước 4 — `check-no-kernel-sleep.sh` vẫn xanh**, chạy lại trên Linux, đọc output. Nửa đỏ của
  chính script (`wait::Park`) vẫn phải trượt.
- **Bước 6 — con số đi kèm `N` của nó**, theo ADR-0012 quyết định 4, và kèm khối machine của
  `check-machine.sh`.

## Tài liệu phải cập nhật

- [ ] `docs/decisions/ADR-0013-…` — bước 1
- [ ] `docs/DESIGN.md` D8 (câu "pinned to an isolated core" thành có thật), §3 (crate/mod mới)
- [ ] `docs/DESIGN.md` §8 — số của `Engine::turn` thật thay cho sàn từ chương trình C
- [ ] `docs/GUIDE.md` §1a — từ "anh tự lo" thành "engine làm, đây là cách khai báo"
- [ ] `docs/PRD.md` — `density` có hình dạng cụ thể
- [ ] `CHANGELOG.md` — public API đổi
- [ ] `STATUS.md` — item 21 đóng, item 22 cập nhật
- [ ] `crates/engine` rustdoc

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Ghim nhầm **thread cha** thay vì thread mới | Test đọc `/proc/self/task/<tid>/stat` của đúng thread đó |
| Lời gọi trả `Ok` nhưng mask không đúng | Đọc lại bằng `sched_getaffinity` và so; đảo ngược bằng cách bỏ lời gọi đặt |
| Feature có trong `Cargo.toml` mà `mod` không có `#[cfg]` | Job CI `no-default-features`. Bẫy này `CLAUDE.md` §10 đã liệt kê sẵn |
| Ghim engine nhưng writer của journal trôi lên đúng lõi cô lập | Bước 5. Test khai báo affinity cho writer rồi đọc lại tid của nó |
| `unwrap` lọt vào code đọc `/sys` | Lint workspace, và `scripts/check-lint-config.sh` chứng minh lint đó thật sự chặn |
| Dùng `scaling_cur_freq` để "chứng minh" đã ghim | Không dùng. Ghi thẳng trong plan vì `[đo 2026-08-30]` nó đóng băng trên lõi `nohz_full` |
| Đo trên máy không cô lập rồi kết luận ghim không giúp gì | `check-machine.sh` phải `pass 10 fail 0`, và số phải kèm khối machine |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| `libc` là dependency ngoài **đầu tiên** của `engine` | Trung bình | Chỉ trong feature `affinity`; `no-default-features` không kéo nó. `libc` không có transitive dep. Ghi lý do trong ADR-0013 |
| `unsafe` đầu tiên trong crate | Trung bình | Một khối duy nhất, quanh đúng một lời gọi, kèm bình luận nêu tên test. `unsafe_code = "warn"` sẽ kêu và điều đó là đúng |
| API shard làm hỏng người dùng hiện tại của `serve()` | Thấp | `serve()` giữ nguyên, không đổi chữ ký. Shard là đường thứ hai |
| Từ chối core không cô lập làm CI không chạy được | Trung bình | `allow_unisolated` mặc định **bật** trong test, **tắt** trong ví dụ và tài liệu |
| Ghim rồi mà latency không tốt hơn | **Cao, và đã lường** | `[đo 2026-08-30]` ghim vào lõi cô lập **không** đổi median lẫn mode 324 ns. Plan này không hứa nhanh hơn — nó biến một khẳng định trong tài liệu thành sự thật kiểm được. Nếu bước 6 cho thấy không đổi gì, **đó là kết quả và ghi đúng như vậy** |

## Ngoài phạm vi

- **Không dùng `io_uring`, `recvmmsg`, hay kernel bypass.** Đó là cách gỡ 703 ns; plan này chỉ
  quyết định thread chạy ở đâu. Item 14 và 22 giữ nguyên.
- **Không NUMA.** Máy này một node, và `[đo 2026-08-30]` đặt luồng khác miền L3 không có tác
  dụng đo được. Xem lại nếu máy đo thành nhiều socket.
- **Không cân bằng lại shard lúc chạy.** Session vào shard nào thì ở đó. Di chuyển giữa thread
  là một quyết định khác và cần ADR riêng.
- **Không tự chọn core.** Nguyên tắc 1 nói vì sao.
- **Không đo phần bị chạm trong 53.3 KiB** của `Connection`. Nó đáng đo và không thuộc plan này.

## Nhật ký giao hàng

*(được duyệt 2026-08-30 cùng lúc với ADR-0010, 0011 và 0012. Chưa bắt đầu bước nào.)*
