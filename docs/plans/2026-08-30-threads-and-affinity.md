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

### Bước 4 — runtime chạy được, và corpus tìm ra một khiếm khuyết thiết kế. 2026-08-31.

**Xong phần code.** `crates/engine/src/shard.rs`: `Shardable`, `Assign`, `RoundRobin`,
`ShardError`, `Shards`, `serve_sharded_hft`. Một thread mỗi shard, mỗi thread **tự ghim trước
tiên**, xác nhận, rồi mới dựng engine — nên buffer của một kết nối được cấp phát bởi đúng core
sẽ chạm vào nó. Luồng acceptor dùng `accept` **chặn** (`Acceptor::bind_blocking`,
`accept_blocking`), vì nó không phải luồng engine và spin ở đó là đốt một core để ngồi chờ.

**Nhưng bước 4 KHÔNG đóng**, và lý do là điều đáng giá nhất trong cả plan này.

#### Corpus qua shard: 59 với một shard, **57 với hai**

`[đo 2026-08-31]` `crates/engine/tests/shard_wire.rs`. Hỏng đúng hai file —
`1b_DuplicateIdentity.def` và `AlreadyLoggedOn.def` — và **hỏng ở cả hai settle bound (1 ms và
20 ms)**, nên không phải timing. Một shard thì 59/59, cũng ở cả hai bound: đường đi của runtime
(channel, clock chia sẻ, thread đã ghim, settle theo wall time, không có `turn` nào do test gọi)
là đúng.

**Vì sao.** Một `Engine` mang **đúng một `Config`**, tức phục vụ **một identity FIX**. Nhờ thế
nó trả lời được câu *"identity này đã logon chưa"* bằng cách đếm những kết nối **nó đang giữ**
mà đang logon. Chia các kết nối đó ra nhiều engine thì không còn gì để đếm, và cả hai `Logon`
đều được nhận. **Luật vốn đúng; shard làm sai tiền đề của nó.**

**`Assign` không sửa được.** Nó được hỏi lúc accept, mà `Logon` — thứ nói identity là gì — chưa
tới. Một acceptor thật đọc `Logon` trước rồi mới định tuyến, tức là cần một tầng "pre-session"
giữ socket cho tới lúc đó. Đó là một quyết định, không phải một bản vá, và chưa có ADR.

**Điều cố ý KHÔNG làm:** cho test một chính sách gán giữ cả hai kết nối trên một shard. Nó sẽ
xanh và **chứng minh không gì cả** — đúng cái nước đi `CLAUDE.md` §10 gọi tên. Thay vào đó
`two_shards_break_the_single_logon_rule_and_this_records_it` ghim khiếm khuyết cùng hai tên file,
và **sẽ đỏ khi khiếm khuyết được sửa** — đó là mục đích của nó. Ghi ở `STATUS.md` open item 24,
và nói thẳng trong rustdoc của `Shards`, trong `serve_sharded_hft`, `GUIDE.md` §1a và
`DESIGN.md` §3.

#### Gộp một fixture bị trùng, trước khi nó thành ba bản

`EchoApp` được viết y hệt ở `crates/session/tests/score.rs` và `crates/engine/tests/wire.rs`, và
bước này sắp làm nó thành ba. Chuyển về một chỗ: `fixbolt_conformance::echo::Echo`. Cố ý
**không** phải một impl `Application` — trait đó thuộc `fixbolt_session`, và `DESIGN.md` §3 cho
`conformance` chỉ phụ thuộc `codec` và `dict`; một fixture dùng chung không phải lý do để đổi đồ
thị phụ thuộc của một crate. Mỗi nơi gọi viết năm dòng chuyển tiếp.

**Cả hai cổng 59 chạy lại và không đổi** — `wire` 2/2, `score` 4/4. Đó là thứ làm việc này thành
refactor chứ không phải sửa fixture cho code mới đi qua.

#### Cổng đã chạy và đọc output

```
cargo test -p fixbolt-engine --features affinity        18+6+2 ... tất cả xanh
  shard.rs         6 test   shard_wire.rs   2 test (một shard 59/59 ở 1ms và 20ms)
cargo test --all                                        0 failure
cargo test -p fixbolt-engine --test wire                2 passed   (59/59, hai chế độ)
cargo test -p fixbolt-session --test score              4 passed   (59/59)
cargo bench -p fixbolt-engine --bench alloc
  allocations: idle 0 send 0 recv 0 frame 0 turn 0 shard-turn 0 busy 0 ring 0 interests 0
cargo clippy --all-targets --features affinity -D warnings   clean
scripts/check-no-kernel-sleep.sh                        GREEN ok / RED ok (6 poll)
scripts/check-standard-gives-the-core-back.sh           GREEN ok / RED ok
scripts/check-links.py                                  402 link, không link chết
```

**Đảo ngược, ba lần, và lần thứ ba mới là lần đáng kể:**

1. Bỏ lời gọi ghim → `every_shard_thread_confirms_the_core_it_was_given` đỏ đúng ở khẳng định
   *"the threads are not on the cores the plan named"*.
2. Bỏ xử lý `Disconnected` → `dropping_the_runtime_ends_every_thread` đỏ.
3. Bỏ `plan.validate()` **mà giữ nguyên phần ghim** → `a_plan_the_machine_refuses_starts_nothing_at_all`
   **vẫn xanh**. Nó chưa bao giờ kiểm điều tên nó nói: `pin_current_thread(CoreId(9999))` hỏng
   ngay trong thread, cùng một `NoSuchCore` quay về, và không engine nào được dựng — cả hai
   khẳng định của nó đúng dù ADR-0015 quyết định 6 có được tôn trọng hay không. Thêm
   `validation_is_what_refuses_and_not_the_pin_behind_it`: dùng một core **online** (nên ghim sẽ
   thành công) nhưng **ngoài `isolcpus`** (nên chỉ `validate()` từ chối). Bỏ `validate()` thì
   đúng test đó đỏ và test cũ vẫn xanh.

**Bước 5 và 6 chưa bắt đầu.** Item 21 vẫn mở, và giờ có thêm item 24 — bước 4 không đóng được
cho tới khi luật single-logon có chỗ ở qua nhiều shard.

### Bước 5 — thread nào cũng có chỗ, và CI làm nổ luật mà máy §9 không thể. 2026-08-31.

**Xong.** `affinity::spawn_pinned(name, core, work)` khởi thread, ghim **từ bên trong** trước
khi làm gì, và **không trả về cho tới khi thread đó xác nhận** — nên một lần ghim hỏng đi tới
đúng luồng có thể dừng khởi động, thay vì chết lặng trên thread mới. Nó trả về core **quan sát
được**, không phải core đã yêu cầu.

`FileJournal::open_pinned(path, Durability::Async, core)` + `writer_core()`. Chỉ `Async` có
writer thread; xin ghim một journal `Fsync` bị **từ chối** chứ không nhận rồi lờ đi — một hàm
dựng lặng lẽ bỏ qua một tham số là cách một triển khai tin rằng nó đã ghim thứ gì đó.

**Consumer của `RingDispatch` vẫn là thread của người dùng** — nó là bất kỳ luồng nào gọi
`RingApp::pump`, và crate này không bao giờ tạo một luồng như vậy. `[đọc code]` cả crate chỉ
`spawn` đúng **một** thread: writer của journal. Nên `with_consumer_cores` **kiểm** chứ không
ghim, và điều đó được nói thẳng trong rustdoc: cái nó mua được là một consumer chia lõi vật lý
với một shard sẽ bị từ chối **trước khi** khởi động.

**Đảo ngược:** bỏ lời gọi ghim trong `spawn_pinned` → đúng 3 test đỏ, mỗi cái ở khẳng định của
nó (*"the thread reported a core it was not asked for"*, *"the writer thread is not on the core
the caller named"*). Chọn core cố ý **khác** core hiện tại của luồng test, vì một thread mới hay
bắt đầu trên core của cha nó — nếu chọn cùng core thì bản đảo ngược sẽ xanh do trùng hợp.

#### Luật SMT nổ thật, ở lần chạy CI đầu tiên

`[đo 2026-08-31]` CI đỏ trên commit bước 4, và **không phải flaky**: runner của GitHub báo
`cpu0` và `cpu1` là **hai luồng của một lõi vật lý**, nên `ShardPlan::new(vec![cpu0, cpu1])` bị
từ chối:

```
a plan this machine accepts: Affinity(SmtSiblingOf(CoreId(0), CoreId(1)))
```

ADR-0015 đã viết rằng luật này **không bao giờ nổ được** trên máy đúng chuẩn §9 (SMT tắt), rằng
nó nổ trên máy *chưa* đúng chuẩn, và rằng vì thế *"cái đọc được test, còn cái thật thì không"*.
Lần chạy CI đầu tiên biến "cái thật" thành đã test. Fixture `desktop_with_smt_on()` là dựng
tay; runner là thật, và cả hai cho cùng một câu trả lời.

**Sửa: test tự chọn một id mỗi lõi vật lý**, bằng `Topology::siblings_of` — nay là public, vì
engine không bao giờ tự chọn core (quyết định 1) nên người gọi phải chọn, và `cpu0, cpu1` là
phỏng đoán tự nhiên và sai. `GUIDE.md` §1a có đoạn code đúng.

**Cổng đã chạy và đọc output:**

```
cargo test -p fixbolt-engine --features affinity   22 + 6 + 2 ... tất cả xanh
cargo test --all                                   0 failure
cargo build -p fixbolt-engine --no-default-features   ok (WriterCore alias giữ một thân hàm)
cargo clippy --all-targets --features affinity -D warnings   clean
scripts/check-links.py                             405 link, không link chết
```

**Bước 6 chưa bắt đầu**, và bước 4 vẫn không đóng được vì item 24.

### Bước 6 — đo, và phép đo lật ngược một dòng lời khuyên của chính §9. 2026-08-31.

`crates/engine/benches/turn.rs`, bốn case, qua socket TCP thật (`Loopback` không có kernel
trong nó và sẽ đo cái sweep mà thiếu đúng thứ chi phối nó).

**Con số thật của `Engine::turn`**, đo trong cùng một binary một lần chạy nên phép trừ không
bắc qua hai chương trình:

```
recv on a quiet socket                474.6 ns
engine turn, 1 idle session           505.2 ns
engine turn, 4 idle sessions         2012.0 ns   (503.0 mỗi session)
engine turn, 16 idle sessions        8162.3 ns   (510.1 mỗi session)
```

**Engine tự nó tốn ~30 ns mỗi session mỗi lượt; syscall là 94% còn lại.** Phẳng từ 1 tới 16
trong vòng 2% — đúng tính chất mà 703 ns từng được công bố kèm, giờ đúng cho engine chứ không
phải cho một cái sàn.

#### Và rồi cùng benchmark đó dưới `taskset` lật ngược một dòng của §9

| Core | `isolcpus`? | miền L3 | turn, 1 session | mỗi session ở N=16 |
|---|---|---|---|---|
| `cpu0` | không | 0 | **498.5 ns** | 509.0 ns |
| `cpu5` | không | 1 | **497.4 ns** | 505.2 ns |
| `cpu6` | **có** | 1 | **680.1 ns** | 679.7 ns |
| `cpu7` | **có** | 1 | **671.6 ns** | 672.0 ns |

**Lõi cô lập chậm hơn 36%** ở đúng cái syscall mà §8 nói là chi phí lớn nhất. Và **không phải
miền L3**: `cpu5` với `cpu6` cùng miền mà lệch 36.7%; `cpu0` với `cpu5` khác miền mà lệch 0.2%.
Ba tuỳ chọn cô lập (`isolcpus`, `nohz_full`, `rcu_nocbs`) do cùng một dòng lệnh kernel áp lên
cùng một tập CPU nên **chưa tách được cái nào**; cơ chế có tên là `nohz_full` — context tracking
chạy ở **mọi** lần vào/ra kernel, mà workload này không là gì khác ngoài vào/ra kernel. Đó là
một giả thuyết có cơ chế, và được ghi đúng như vậy.

Con số 703 ns cũ đo bằng chương trình C **ghim trên lõi cô lập** — chính nó ghi thế. Khớp chỗ
đặt lại thì hai số đồng ý trong 4%.

**Điều phép đo này KHÔNG nói:** cô lập mua được bao nhiêu jitter. Một cái đuôi bị nó cắt đi rất
có thể đáng hơn 175 ns median. Cái nó bỏ đi là giả định rằng cô lập là miễn phí. `DESIGN.md` §8
và §9, `GUIDE.md` §1a và item 22 đều nói lại điều đó.

`[to testing-skills]` — **một cấu hình được nhận vì hiệu năng, chưa bao giờ đo với chính thao
tác mà nó thay đổi.** Cả lời khuyên lẫn thao tác nóng đều nằm trong cùng một tài liệu ở đây,
suốt một ngày, và không ai bấm giờ hai thứ cùng nhau. Ghi ở `reference/measured-costs.md`.

#### Cái không đo ở đây

**Tổng theo số shard.** `N` ở trên là số session trên **một** engine, tức là những gì một shard
giữ; tổng cho M shard là M thread mỗi cái làm đúng thế, và đó là câu hỏi wire-to-wire của
`tools/w2w` chứ không phải của một microbench — mà `w2w` chưa shard. Phép tính mà `GUIDE.md`
§1a phát biểu (8 shard × 13 session thay vì 1 × 104) chính là phép tính lấy những con số này
làm đầu vào.

#### Baseline cho bốn case mới, theo đúng thủ tục ADR-0016

`[đo 2026-08-31]` **26 lần chạy `bench.sh` trọn vẹn, 21 lần đọc `pass 10 fail 0`**, cách nhau
8 giây, đo **qua `bench.sh`** chứ không phải chạy thẳng target — đúng cái bẫy `baselines.tsv`
ghi ở đầu file.

| case | median | max/median | margin |
|---|---|---|---|
| `recv on a quiet socket` | 470.9 ns | 1.016 | 1.10 |
| `engine turn, 1 idle sessions` | 500.3 ns | 1.012 | 1.10 |
| `engine turn, 4 idle sessions` | 2002.9 ns | 1.021 | 1.10 |
| `engine turn, 16 idle sessions` | 8139.4 ns | 1.017 | 1.10 |

Đây là những case **chặt nhất** trong file — max/median 1.011–1.021, so với 1.30 của
`ring, one way` — nên cả bốn đều lấy bậc thấp nhất của thang.

**Năm lần bị loại, và nguồn lớn nhất là chính tôi.** Mỗi lần tôi hỏi thăm tiến độ bằng `ps`,
`grep`, `python3` là một lần thêm tải lên đúng cái máy đang đo. `check-machine.sh` bắt được:
`FAIL machine is quiet — 15% CPU busy over 1s`. Ghi lại vì nó là một biến thể của điều đã có
trong `desktop-load-invalidates-benchmarks`: **người quan sát phép đo là một phần của phép đo**.

```
scripts/bench.sh --strict
  targets measuring    9 of 9
  timing over baseline 0
  cases w/o a baseline 0
  OK                                   EXIT=0
```

#### Cổng corpus qua shard có một sàn, và sàn đó là của máy

`[đo 2026-08-31]` CI đỏ hai lần liên tiếp ở `quiet = 1 ms`, và lần thứ hai không phải do hai
test giẫm chân nhau — đã sửa rồi. Runner của GitHub có **hai vCPU là hai luồng của một lõi vật
lý**, nên luồng engine và luồng test chia nhau một lõi và một khoảng trống *bên trong* chuỗi
trả lời dài hơn 1 ms sẽ kết thúc bước sớm và mất phần còn lại: **58/59**.

Trên desktop tham chiếu (8 lõi vật lý) thì 59 ở 1, 2, 4, 8 và 20 ms — 18 lần chạy ở ba mức đầu.

**Vậy `quiet` có một sàn do khả năng lập lịch của máy quyết định, không phải do giao thức.** Hai
mức mới là **10 ms và 50 ms**, cách nhau 5×, đặt hẳn trên sàn đó.

**Đây không phải nước đi mà `tests/wire.rs` cảnh báo.** Cảnh báo đó nói về việc **nâng bound cho
tới khi một lỗi giao thức biến mất** — `[đo 2026-08-30]` một cổng wire có điểm đi 39 → 43 → 59
theo timeout của chính nó thật ra đang hỏng vì Nagle. Giao thức ở đây đã có cổng **tất định**
ngay bên cạnh: `tests/wire.rs`, 59/59, hai chế độ, **không có bound nào cả**. Cái tránh được ở
đây là một test báo cáo về bộ lập lịch của runner rồi gọi đó là FIX.

#### Một chẩn đoán sai của tôi, và nó là phần đáng học hơn

`[2026-08-31]` Tôi kết luận cổng corpus qua shard **treo trên CI 35 phút** và đã (a) huỷ một
lần chạy, (b) bắt test đòi hai lõi vật lý và bỏ qua ở nơi không có, (c) định thêm `timeout` vào
job CI. **Cả ba dựa trên một quan sát sai.**

Sự thật: job đó **xong sau 69 giây**, bước affinity mất **19 giây**, và CI **xanh 9/9**. Cái tôi
đọc là trường `status` của GitHub API, và nó trả `in_progress` **gần một tiếng sau khi job đã
hoàn tất**. Tôi đọc lại nó bảy lần và bảy lần tin nó.

Đã lùi: bỏ yêu cầu hai lõi vật lý, bỏ `timeout`. **Cái còn lại là con số 58/59 ở 1 ms**, vì nó
đến từ **log của một lần chạy hỏng thật**, không phải từ một trường trạng thái.

`[to testing-skills]` — **một trường trạng thái không phải một quan sát.** Cùng một API, cùng
một câu hỏi, hai câu trả lời khác nhau: `status: in_progress` sai suốt một tiếng, trong khi
`steps[].completed_at` của chính job đó nói chính xác 12:53:52. Log thì luôn đúng. Quy tắc rút
ra: **khi một hệ thống cho cả một trạng thái tóm tắt lẫn dữ liệu thô, tin dữ liệu thô** — và khi
định làm một hành động không lùi được (huỷ một lần chạy, tắt một test), hãy lấy xác nhận từ
nguồn thứ hai trước. Ở đây nguồn thứ hai nằm trong cùng một lời gọi API và tôi không hỏi nó.

Đây là biến thể của cái đã ghi ở `reference/ktls-on-a-plain-socket.md`: **một chẩn đoán mâu
thuẫn với chính dữ liệu của nó, và không có gì kiểm chẩn đoán đó.**
