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
| **`isolated` liệt kê cả core không online.** Sau khi §9 tắt SMT: `present 0-15`, `online 0-7`, `offline 8-15`, `isolated 6-7,14-15`. Phải giao với `online` | `[đo 2026-08-31]` — xem nhật ký bước 1 |
| ~~`cpu6` ↔ `cpu14`, `cpu7` ↔ `cpu15` là cặp SMT sibling~~ — **đúng lúc SMT còn bật**. `[đo 2026-08-31]` §9 tắt SMT, nên trên máy đã tune mọi CPU online có `thread_siblings_list` **một phần tử** và luật SMT không bao giờ nổ được ở đây | `[đo 2026-08-30]`, sửa `[đo 2026-08-31]` |
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
- `docs/decisions/ADR-0015-…` — mới, bước 1
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
| 1 | **ADR-0015** — mô hình thread và affinity: id tường minh, ghim từ trong thread, đọc lại xác nhận, từ chối khi sai, ai sở hữu việc gán shard. Chốt hình dạng API | ADR-0012 được ký |
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

- [x] `docs/decisions/ADR-0015-…` — bước 1, **xong 2026-08-31**
- [x] `docs/DESIGN.md` D8 (câu "pinned to an isolated core" thành có thật), §3 (crate/mod mới) — **bước 2, 2026-08-31**
- [ ] `docs/DESIGN.md` §8 — số của `Engine::turn` thật thay cho sàn từ chương trình C
- [~] `docs/GUIDE.md` §1a — **nửa chừng 2026-08-31**: mục "Pinning" đã đổi; shard, từ chối core sai và affinity cho thread phụ vẫn là của người dùng
- [ ] `docs/PRD.md` — `density` có hình dạng cụ thể
- [x] `CHANGELOG.md` — public API đổi — **bước 2, 2026-08-31**
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
| `libc` là dependency ngoài **đầu tiên** của `engine` | Trung bình | Chỉ trong feature `affinity`; `no-default-features` không kéo nó. `libc` không có transitive dep. Ghi lý do trong ADR-0015 |
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

### Bước 1 — ADR-0015, và hai luật đổi vì đọc máy thật. 2026-08-31.

**Xong.** [ADR-0015](../decisions/ADR-0015-explicit-cores-pinned-from-inside-and-read-back.md)
chốt: id core tường minh do người gọi nêu; ghim **từ trong thread**, việc đầu tiên, rồi
`sched_getaffinity` **đọc lại và so**; hỏng thì dừng ở khởi động; lỗi **có trường** (không phải
hot path, và `NotIsolated(CoreId(3))` nói được điều `NotIsolated` không nói); `validate()` chạy
trước khi tạo thread nào; gán session vào shard là của người gọi; mọi thread crate tạo đều có
chỗ; feature `affinity` gate chính `mod`, dùng lại `libc` mà `standard` đã làm optional; **đúng
một khối `unsafe`**.

**Sửa 1 — số ADR.** Plan này viết "ADR-0013" ở bốn chỗ. Nó được viết trước khi `standard-mode`
lấy 0013 và 0014, và §5 cấm dùng lại số. **Số đúng là 0015**, đã được giữ chỗ trong header của
ADR-0018. Bốn chỗ đó sửa trong cùng commit này; dòng ở đầu plan **giữ nguyên** vì nó trỏ đúng
tới ADR-0013 thật (hai chế độ).

**Sửa 2 — hai luật từ chối đổi, vì đọc máy chứ không suy.** `[đo 2026-08-31]` trên máy §9 đã
tune, `check-machine.sh` đọc `pass 10 fail 0`:

```
present   0-15        online    0-7         offline   8-15
isolated  6-7,14-15   nohz_full 6-7,14-15   smt/control off
cpu6 thread_siblings_list  6     cpu7 thread_siblings_list  7
```

- **`isolated` gọi tên core không chạy được.** `isolcpus=6,7,14,15` đến từ dòng lệnh kernel;
  §9 sau đó tắt SMT, đưa 8–15 offline. File `isolated` vẫn liệt kê 14 và 15. Một validator chỉ
  đọc `isolated` sẽ nhận core 14 và ghim thread lên một CPU không tồn tại với scheduler.
  **Phải giao `isolated` với `online`, và `online` thắng.** Plan chỉ nói "đọc
  `/sys/devices/system/cpu/isolated`" — thế là chưa đủ. Thêm luôn `NoSuchCore` (đọc `present`)
  tách khỏi `NotOnline` (đọc `online`): trên máy này chúng là hai trạng thái khác nhau và
  `NotOnline` mới là cái thật sự gặp.
- **Luật SMT sibling không bao giờ nổ được trên máy đúng chuẩn.** §9 bắt tắt SMT, nên mọi CPU
  online có `thread_siblings_list` một phần tử. Đó **không** phải lý do bỏ luật — nó nổ trên máy
  *chưa* đúng chuẩn, tức là đúng chỗ người ta mắc lỗi. Nhưng nó có nghĩa là **cái đọc được test,
  còn cái thật thì không**, và ADR ghi thẳng đó là một lỗ chứ không phải thủ tục.

**Thêm một biến thể lỗi ngoài phác thảo của plan:** `DuplicateCore` — hai shard nêu cùng một id
là đúng lời nói dối mà `SmtSiblingOf` chặn, và là lỗi dễ mắc hơn.

**Bốn câu hỏi mở** được ghi trong ADR thay vì để chúng thành bất ngờ ở bước sau; câu đáng chú ý
nhất: `check-no-kernel-sleep.sh` quy syscall theo tid và lấy `engine-tid` **đầu tiên** nó thấy —
với M shard nó chỉ kiểm một trong M. Đó là việc của bước 4 và đã được gọi tên.

**Bước 2 chưa bắt đầu.**

### Bước 2 — `affinity.rs`: ghim, rồi hỏi lại kernel. 2026-08-31.

**Xong.** `crates/engine/src/affinity.rs` sau `#[cfg(all(feature = "affinity", target_os =
"linux"))]`, feature `affinity` **tắt mặc định**, dùng lại đúng `libc` optional mà `standard`
đã có. API: `CoreId`, `AffinityError`, `pin_current_thread`, `current_mask`, `running_on`.

**Test đỏ trước, trên code chưa viết:**

```
error[E0583]: file not found for module `affinity`
  --> crates/engine/src/lib.rs:23:1
```

Rồi 5 test xanh. Nhưng xanh chưa chứng minh gì, nên **hai lần đảo ngược**:

1. Bỏ lời gọi `sched_setaffinity` → 2/5 đỏ. Nhưng chúng đỏ ở **guard read-back**
   (`ReadbackMismatch`), không phải ở khẳng định về cư trú. Đúng theo nghĩa "guard bắt được",
   nhưng chưa chứng minh được khẳng định kia có giá trị.
2. Bỏ **cả** read-back → khẳng định cư trú mới là thứ phải bắt, và nó bắt:

```
assertion `left == right` failed: a pinned thread must be observed on exactly one core, CoreId(0)
  left: [CoreId(0), CoreId(4), CoreId(5)]
 right: [CoreId(0)]
```

Đây là điều đáng giá nhất của bước này: **không ghim thì thread thật sự bị scheduler dời**, ba
core khác nhau trong một lần chạy. Test không rỗng, và việc ghim đúng là thứ chặn nó lại. Nếu
chỉ làm đảo ngược thứ nhất thì vẫn không biết điều đó.

**Ba chỗ ADR-0015 sai, và viết code là thứ tìm ra:**
[ADR-0019](../decisions/ADR-0019-two-unsafe-blocks-and-an-error-the-enum-can-hold.md) sửa hai
quyết định, phần còn lại của ADR-0015 giữ nguyên.

1. **"Đúng một khối `unsafe`" mâu thuẫn với chính quyết định 2 của nó.** Read-back là
   `sched_getaffinity` — một lời gọi FFI thứ hai, tức là một khối thứ hai. Luật thật:
   *không có `unsafe` ngoài `affinity.rs`, mỗi lời gọi FFI một khối, mỗi khối nêu tên test
   chứng minh nó.* Hôm nay là **hai**. Đếm số khối là luật dễ lách bằng cách gộp khối lại — tệ
   hơn mà vẫn từng ấy `unsafe`.
2. **Khối thứ ba có sẵn và đã từ chối.** `sched_getcpu()` trả lời "thread đang ở core nào";
   `/proc/thread-self/stat` trường `processor` trả lời y hệt, **không FFI**, và là câu trả lời
   *tốt hơn* vì scheduler viết nó chứ không phải crate này. `[đo 2026-08-31]` chỉ số trường được
   kiểm bằng `taskset -c 3` chứ không đếm theo tài liệu.
3. **`NotSupported` không dựng được.** ADR-0015 cho nó nghĩa "không phải Linux, hoặc feature
   tắt" — nhưng `mod` bị gate đúng bằng hai điều kiện đó, nên nếu một trong hai sai thì enum
   không tồn tại để chứa variant. Bỏ. Thêm `Failed(i32)` và `Unreadable(&'static str)`: không có
   chúng, một `errno` ngoài dự kiến buộc phải bị xếp vào một nguyên nhân đã mô hình hoá — đúng
   hình dạng lỗi repo này đã trả giá một lần ở `check-ktls-available.sh`.

**Cổng đã chạy và đọc output, không suy ra:**

```
cargo test -p fixbolt-engine --features affinity --test affinity   5 passed
cargo test -p fixbolt-engine --no-default-features --features affinity  ok  (tổ hợp CI không chạy)
cargo test --all                                   52 target, 0 fail
cargo fmt --all -- --check                         clean
cargo clippy --all-targets --all-features          clean
scripts/check-no-optional-deps.sh                  ok, per crate
scripts/check-no-kernel-sleep.sh                   GREEN ok / RED ok (7 poll)
scripts/check-standard-gives-the-core-back.sh      GREEN ok / RED ok (yield 95.52% CPU)
scripts/check-links.py                             389 link, không link chết
```

`--no-default-features` xanh **chính là** bằng chứng cho bất biến 6: nếu `mod affinity` không có
`#[cfg]`, file đó sẽ được biên dịch, nó dùng `libc`, và `libc` không có trong build đó — hỏng
biên dịch. Không phải suy luận, là kết quả của một lệnh đã chạy.

**Bước 3 chưa bắt đầu.** Item 21 **vẫn mở**: ghim đã có, nhưng các phép từ chối (core offline,
không cô lập, SMT sibling) và affinity cho writer/consumer chưa có, nên câu của D8 mới đúng một
nửa và `DESIGN.md` nói đúng như vậy.

### Bước 3 — các phép từ chối, và một cái bẫy chỉ máy thật mới chỉ ra. 2026-08-31.

**Xong.** `Topology` + `ShardPlan::validate()`, chạy **trước khi tạo thread nào** (ADR-0015
quyết định 6). Năm phép từ chối, mỗi cái gọi tên core: `NoSuchCore`, `NotOnline`,
`DuplicateCore`, `SmtSiblingOf`, `NotIsolated`, cộng `EmptyPlan`.

**Test đỏ trước:**

```
error[E0432]: unresolved imports `fixbolt_engine::affinity::ShardPlan`,
              `fixbolt_engine::affinity::Topology`
error[E0599]: no variant ... named `EmptyPlan`
```

Rồi 18 test xanh, và **đảo ngược cả ba luật cùng lúc** — bỏ kiểm `online`, bỏ kiểm cô lập, bỏ
kiểm SMT sibling. Đúng 5 test đỏ và đúng 5 cái phải đỏ:

```
a_core_outside_isolcpus_is_refused_by_default            FAILED
allow_unisolated_lifts_exactly_one_rule_and_no_other     FAILED
an_isolated_core_that_is_offline_is_still_refused        FAILED
a_support_thread_on_an_smt_sibling_of_a_shard_is_refused FAILED
two_shards_on_smt_siblings_are_refused                   FAILED
13 passed
```

Quan trọng không kém: **các test "chấp nhận" vẫn xanh** (`an_isolated_online_core_is_accepted`,
`a_support_thread_need_not_be_isolated`). Nếu chúng cũng đỏ thì bộ test chỉ đang từ chối mọi thứ.

**`Topology::from_sysfs` là public, và đó là một quyết định chứ không phải tiện tay.** ADR-0015
đã nói: §9 bắt tắt SMT, nên trên máy đúng chuẩn luật SMT sibling **không bao giờ nổ được**. Cách
duy nhất để kiểm nó là dựng topology giả. Hai fixture được commit, cả hai là số đọc thật của
chính máy này:

- `tuned_desktop()` — `present 0-15`, `online 0-7`, `isolated 6-7,14-15`, siblings đơn.
  **Đây là cái bẫy**: `isolated` gọi tên cpu14 và cpu15, mà cả hai đang offline. Validator chỉ
  đọc `isolated` sẽ nhận một core kernel không xếp lịch lên được.
- `desktop_with_smt_on()` — cùng máy trước khi §9 tắt SMT, `[đo 2026-08-30]`, cặp 6↔14 và 7↔15.

**Hai lựa chọn thiết kế, nói rõ vì sao:**

1. **Cô lập chỉ bắt buộc với core của shard.** Bắt writer của journal hay consumer của ring phải
   nằm trong `isolcpus` là đẩy chúng lên đúng những core mà thiết kế này đang cố giữ sạch. Chúng
   vẫn bị kiểm tồn tại, online, trùng lặp và **SMT sibling với một shard** — vì chia lõi vật lý
   với engine là đúng cái hại cần chặn.
2. **`allow_unisolated()` nới đúng một luật.** Core không tồn tại hoặc offline vẫn bị từ chối,
   và có một test nói đúng điều đó — một cửa thoát lặng lẽ biến thành "cho qua tất" còn tệ hơn
   không có luật.

**Thêm một variant:** `EmptyPlan`. ADR-0019 quyết định 5 đã lường trước chuyện này khi làm enum
`#[non_exhaustive]`, nên không cần ADR mới.

**Cổng đã chạy và đọc output:**

```
cargo test -p fixbolt-engine --features affinity --test affinity   18 passed
cargo test --all                                                   0 failures
cargo test -p fixbolt-engine --no-default-features --features affinity   ok
cargo clippy --all-targets --features affinity -- -D warnings      clean
cargo fmt --all -- --check                                         clean
scripts/check-links.py                                             394 link, không link chết
```

CI xanh trên commit của bước 2: `407d72c`, run
[`33387225861`](https://github.com/tmthang86/fixbolt/actions/runs/33387225861) — **và đó là lần
đầu bước CI mới thật sự biên dịch `--features affinity`**; trước khi thêm nó, cả module lẫn 5
test của bước 2 là một cổng không có gì chạy.

**Bước 4 chưa bắt đầu.** Item 21 vẫn mở: `ShardPlan` **nói được** writer và consumer nằm đâu,
nhưng chưa có gì đặt chúng vào đó, và engine vẫn chưa shard.

### Trước bước 4 — kênh chuyển socket không được chặn, và đã đo. 2026-08-31.

Bước 4 phải chuyển một socket vừa accept từ luồng acceptor sang luồng engine sẽ sở hữu nó. Vật
mang hiển nhiên là `std::sync::mpsc`, và nỗi lo hiển nhiên là `try_recv` lấy khoá — trên luồng
engine thì đó là một `futex`, tức là vi phạm bất biến 4.

`[đo 2026-08-31]` Ryzen 7 3700X, Linux 7.0.0-30-generic, rustc 1.98.0, `--release`. Một luồng
spin `try_recv` **2 000 000 lần** trong khi luồng kia gửi 5 giá trị; `strace -f`, quy syscall
theo tid, giữa hai mốc do chính nó in ra: **không một syscall nào**. Cùng luồng đó trên **toàn
bộ** lần chạy có 19 syscall — tất cả đều là khởi tạo/kết thúc thread và đều nằm ngoài vùng đo.

Con số thứ hai mới là thứ làm con số thứ nhất có nghĩa: "không có sự kiện nào" và "máy đo không
chạy" in ra giống hệt nhau. Ghi ở
[measured-costs.md](../reference/measured-costs.md), kèm dấu `[to testing-skills]`.

**Kết luận cho bước 4:** dùng `std::sync::mpsc` + `try_recv` trên luồng engine là được. Phía
**gửi** chạy trên luồng acceptor và được phép chặn — luồng đó không phải luồng engine, nên nó
nên dùng `accept` chặn thay vì spin, và như thế không tốn thêm một core cho việc ngồi chờ.
