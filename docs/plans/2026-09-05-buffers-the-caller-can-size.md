# Ba hằng mà tài liệu bảo bạn tự chọn, và không ai chọn được

> **Loại:** Plan · **Ngày:** 2026-09-05 · **Trạng thái:** **Đã duyệt (2026-09-05), đang làm**
> **Phạm vi:** `engine` (`lib.rs` type alias + `pump` + entry point, `shard.rs` một dòng),
> **`session` (`Outbound::app`, `Session` — Sửa 2)**, docs. **Không chạm** `codec`,
> `library` logic — chỉ thêm re-export.
> **Không đổi một mặc định nào.** Mọi caller hiện tại biên dịch và chạy y nguyên.
>
> **Máy chạy:** macOS đủ. **Thời lượng:** 1 ngày.
> **Đứng trước** [settings-for-both-roles](2026-09-04-settings-for-both-roles.md): plan đó viết
> code chọn giữa các entry point, nên sửa chữ ký của chúng sau đó là làm hai lần.

## Bối cảnh — một hằng giấu sau một type alias vẫn là hằng giấu

`CLAUDE.md` §6: *"**`FieldIndex<const N>`** — the caller picks `N`. Aliases for the common
sizes; **no hidden constant**."* `docs/CONFIGURATION.md` nói với người đọc cách đổi `N`/`RX`/`TX`:
*"Instantiate `Engine<..., N, RX, TX>` directly."*

**Cả hai đều không đúng với người dùng thật.**

| | |
|---|---|
| Sáu entry point (`serve`, `serve_hft`, `connect_and_serve`, hai bản `_with_recovery`, `serve_sharded_hft`) | **không cái nào có tham số `N`/`RX`/`TX`** — tất cả đi qua alias khoá cứng `256, 4096, 8192` (`lib.rs:1169-1205`) |
| Người dùng crate `fixbolt` | **`Engine` không được re-export** — `crates/library/src/lib.rs` re-export 9 thứ từ `fixbolt-engine`, không có `Engine` |
| Muốn đổi thật thì phải làm gì | thêm `fixbolt-engine` làm dependency riêng **và viết lại vòng `pump`** (~60 dòng, gồm vòng pre-session và `PRE`) |

Nên câu trong `CONFIGURATION.md` **không làm theo được** bởi đúng đối tượng nó viết cho. Một
người dùng gặp đối tác gửi message dài hơn 4 KiB hôm nay không có đường đi nào ngoài fork.

**Đây không phải chuyện nâng mặc định.** Mặc định nào cũng là phỏng đoán: nếu một venue gửi
64 KiB thì 16 KiB hỏng y như 4 KiB. Cái hỏng là **không ai đổi được nó**. Nâng mặc định là việc
của đợt C, cần máy §9 (`STATUS.md` item 41 vùng lân cận, và ADR mặc định-cho-PROD).

## Những gì đã biết chắc (xác minh 2026-09-05)

| Sự thật | Nguồn |
|---|---|
| `Framer<const N>` giữ `[u8; N]`; `Connection<T, R, J, N, RX, TX>` giữ `Framer<RX>` và `[u8; TX]` | `frame.rs:54`, `conn.rs:33-74` |
| `pump` nhận `TcpAcceptorEngine<A, W, J, L>` (alias đã khoá) và khai `const PRE: usize = 4096` bên trong | `lib.rs:1620-1637` |
| Bốn type alias khoá `256, 4096, 8192` | `lib.rs:1169, 1184, 1188, 1204` |
| `serve_sharded_hft` khai `const PRE: usize = 4096` riêng | `shard.rs:471` |
| **`Shards<const PRE: usize = 4096>` ĐÃ có const generic kèm default** — phía shard đã tham số hoá sẵn, chỉ entry point ghim nó | `shard.rs:204` |
| `PRE` phải bằng `RX`, và **chỉ có comment giữ điều đó**, ở hai chỗ | `lib.rs:1633`, `shard.rs:469` |
| `[đo 2026-09-04]` `Connection` RX=4096 → 23 752 B; RX=16384 → 36 040 B; journal `Store` thêm ~2 MiB/session trên heap, nên RX×4 là **+0,57%** | `size_of`, `journal.rs:45-48` |

## Hình dạng bị compiler ép, không phải bị chọn

`[đo 2026-09-05, rustc stable, bốn probe]`

| Thử | Kết quả |
|---|---|
| `serve_with::<256, 16384, 8192, _, _>(...)` — const đứng trước, type suy ra | **được** |
| `serve_with::<256, 16384>(...)` — bỏ `_` | **không**: *"function takes 5 generic arguments but 2 were supplied"* |
| `fn serve<..., const RX: usize = 4096>` — thêm default vào `serve` sẵn có | **không**: *"defaults for generic parameters are not allowed here"* |
| `trait Sizes { const RX; }` + `[u8; S::RX]` — gói ba hằng thành một tham số cho call site đẹp hơn | **không**: *"generic parameters may not be used in const operations"* — cần `generic_const_exprs`, unstable |
| `type Alias<A, const N: usize = 256> = ...` — default trên **type alias** | **được** |

Hai hệ quả, và cả hai là của ngôn ngữ chứ không phải lựa chọn thiết kế:

1. **`serve` không thể tự nhận thêm tham số** — phải có hàm thứ hai. Đổi lại: chữ ký `serve` không
   đổi một ký tự, nên **không caller nào phải sửa**.
2. **Call site phải có `_, _`.** `serve_with::<256, 16384, 8192, _, _>(...)`. Xấu, và là cái giá
   rẻ nhất có thể trên stable. `CONFIGURATION.md` phải in đúng dòng này chứ không mô tả nó.

## Cách làm

```rust
// giữ nguyên tuyệt đối — mặc định của hôm nay, không caller nào phải sửa
pub fn serve<A: Application, L: MessageLog>(...) -> Result<Shutdown, ServeError> {
    serve_with::<256, 4096, 8192, A, L>(addr, table, app, capacity, limits, log)
}

// mới
pub fn serve_with<
    const N: usize, const RX: usize, const TX: usize,
    A: Application, L: MessageLog,
>(...) -> Result<Shutdown, ServeError>
```

Alias nhận const default nên bản thân chúng vẫn viết ngắn được:

```rust
pub type TcpAcceptorEngine<
    A, W, J = Store, L = NoLog,
    const N: usize = 256, const RX: usize = 4096, const TX: usize = 8192,
> = Engine<TcpTransport, Acceptor, InlineDispatch<A>, SystemClock, W, J, N, RX, TX, L>;
```

`pump` nhận ba const và **`PRE` biến mất, thay bằng `RX`** — đó là chỗ comment trở thành kiểu.

## Chia việc

| Bước | Kết quả |
|---|---|
| 1 | Test đỏ trước, **hai loại đỏ, và [a-reversal-that-must-not-compile](../reference/a-reversal-that-must-not-compile.md) nói vì sao phải là hai**: (a) claim *"RX giờ là lựa chọn của caller"* là claim về **type system**, nên reversal đúng của nó là **lỗi compiler**, trích nguyên văn; (b) claim *"message 6 KiB tới được session"* là hành vi, nên nó đỏ **ở assertion**. Một mình (a) chỉ chứng minh code chưa viết |
| 2 | Bốn type alias nhận `const N/RX/TX` kèm default. `cargo test --all` phải xanh **không sửa một call site nào** — nếu phải sửa thì default chưa đúng chỗ |
| 3 | `pump` nhận ba const; **xoá `const PRE`**, dùng `RX`. Cùng việc đó ở `shard.rs:471` → `Shards<RX>` (đã có sẵn tham số) |
| 4 | Năm hàm `*_with` cho năm entry point ở `lib.rs`; bản cũ gọi bản mới với `256, 4096, 8192`. `serve_sharded_hft_with` cho cái thứ sáu |
| 5 | `library`: re-export `serve_with` và bạn bè; **không** re-export `Engine` (giữ vòng `pump` là đường chính thức) |
| 6 | Docs: `CONFIGURATION.md` bảng ba hằng — **in đúng dòng gọi**, không mô tả; `GUIDE.md` §6 đoạn arithmetic bộ nhớ kèm số đo 2026-09-04; `CHANGELOG.md`; `STATUS.md` |

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **6 — feature gate** | `serve_sharded_hft` sau `affinity`/`hft`; các `_with` phải mang **đúng** `#[cfg]` của bản gốc | `cargo test --all --no-default-features`, và `scripts/check-no-optional-deps.sh` hỏi từng crate |
| 1 — không cấp phát | không đổi đường nào; `Framer::new()` vẫn zero lúc dựng | `benches/alloc.rs` **24 case đọc 0 y như trước** |
| 2 — session thuần | không chạm `session` | — |
| 10 — không con số nào thiếu benchmark | **plan này không công bố con số hiệu năng nào** và không đổi mặc định, nên không cần máy §9 | nói rõ trong `CHANGELOG` |

## Cách kiểm chứng

- `cargo test --all` và `--no-default-features` — **492 test hiện có phải xanh không sửa một dòng
  test nào.** Đây là gate chính: một plan "không đổi hành vi" mà phải sửa test là một plan đã đổi
  hành vi.
- `benches/alloc.rs` 24 case đọc 0.
- `scripts/interop.sh` cả hai chiều — mặc định không đổi nên **kết quả phải giống hệt 7/7 + 7/7**.
- `cargo doc --workspace --no-deps` dưới `-D rustdoc::broken_intra_doc_links` (job `docs`).

**Reversal, và cái thứ hai là cái thật:**

| Đảo | Phải thấy |
|---|---|
| Test bước 1 chạy trên `serve` thường (RX=4096) thay vì `serve_with::<_, 16384, _>` | **đỏ** — message 6 KiB thành `Cut::Garbage`. Nếu nó xanh thì test không đo cái nó tưởng |
| Đổi `pump` cho `PRE = 4096` cứng trong khi `RX = 16384` — **ở CẢ HAI chỗ**, `lib.rs:1635` và `shard.rs:471` | **đỏ** — pre-session cắt prefix ngắn hơn buffer của connection. Nếu **không** đỏ thì `PRE == RX` chưa bao giờ là ràng buộc thật, và cả hai comment ở `lib.rs:1633`/`shard.rs:469` sai. **Lật một chỗ là một reversal dở dang** và cho một màu đỏ nghe hợp lý nhưng nói dối về việc suite đang giữ cái gì — `a-reversal-that-must-not-compile.md` mục *near-miss*; `grep` hằng đó là cả kỹ thuật |

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Thêm default vào alias làm một call site im lặng nhận số khác | bước 2 đòi 492 test xanh **không sửa call site**; và `size_of` không đổi |
| `_, _` trong turbofish suy ra nhầm `L` thành `NoLog` khi caller truyền `FileLog` | một test dùng `serve_with` **có** `FileLog`, đọc lại file |
| Năm hàm `_with` chép nhầm một `#[cfg]` | `--no-default-features` + build từng crate; §2 điều 6 |
| Nhân đôi số hàm public rồi tài liệu chỉ nói về một nửa | bước 6: mỗi `_with` có rustdoc trỏ về bản gốc và ngược lại; job `docs` bắt link gãy |
| **Sửa một test cũ để bước nào đó đi qua** | `git diff --stat` trên `crates/*/tests/` phải **chỉ có dòng thêm**, không dòng sửa. Kiểm bằng tay trước khi commit — `CLAUDE.md` §10 |

## Ngoài phạm vi

- **Không đổi một mặc định nào** — `256/4096/8192` giữ nguyên. Nâng chúng là đợt C, cần máy §9,
  cùng ADR mặc định-cho-PROD với `SLOTS`/`SLOT_LEN`/`RingDispatch`.
- Không re-export `Engine` từ `library` — nếu `serve_with` vẫn không đủ cho ai đó thì đó là một
  ADR riêng, có bằng chứng.
- Không đụng tag 383, `DropReason` cho frame quá dài (thuộc `settings-for-both-roles` bước 6),
  item 41, 39, 34, 46.

## Sửa 1 `[2026-09-05]` — plan thiếu một hằng, và đoán sai chiều của một invariant

**Rule Zero: plan sai giữa chừng thì dừng, sửa, duyệt lại.** Hai chỗ, cả hai tìm ra khi chạy chứ
không khi đọc, ghi ở [a-ceiling-has-more-than-one-floor](../reference/a-ceiling-has-more-than-one-floor.md).

**1. Có hằng giấu thứ tư, và nó chặt hơn `RX`.** `crates/session/src/out.rs:44`:

```rust
pub(crate) app: [u8; 1024],   // nơi Application viết reply
```

Plan nói *"ba hằng"*. Có **bốn**. Và cái thứ tư là cái cắn trước: một acceptor hôm nay **nhận**
được 4 KiB nhưng **trả lời** không quá ~1 KiB, vì một app không xếp nổi reply trả `None` — im
lặng hợp lệ, không lỗi ở đâu cả. Sweep kích thước cho thấy ngưỡng thật nằm giữa **200 và 1000
byte**, không gần 4096:

```
size=  200  echo=true   reply_len=223
size= 1000  echo=false  reply_len=0
size= 5000  echo=false  reply_len=0
```

**`[Sửa 2, 2026-09-05, chủ dự án chọn]` kéo vào plan này** — xem Sửa 2.

**2. Invariant là `PRE ≤ RX`, không phải `PRE == RX`.** Plan viết reversal là *"ghim `PRE = 4096`
trong khi `RX = 16384`, phải đỏ"*. Chạy: **xanh**. Lật cả hai chỗ như *near-miss* dặn: **vẫn
xanh**. Cả hai lần đều đúng, vì `4096 < 16384` là phía an toàn — guard thật là
`if prefix.len() > RX { return Err(PrefixTooLong { .. }) }` (`lib.rs:487`). Lật đúng chiều thì nó
phân biệt ngay:

```
REVERSAL PRE>RX: logon answered = false
```

Comment cũ nói *"matches"*, và chính chữ đó đẩy hai lần reversal đi sai hướng. **Một bất đẳng
thức có hai chiều; đọc guard trước khi chọn chiều.**

**3. Observable của test bước 1 đã đổi.** *"Echo có về không"* bắc qua ba hằng và tên test chỉ nêu
một — nó đo `min` của cả ba. Thay bằng *"session còn đúng sequence không"*: gửi message lớn, rồi
một `TestRequest` nhỏ ngay sau; message không frame được là garbage và **kéo theo mọi thứ đệm phía
sau nó**, nên `TestRequest` biến mất cùng. Một ranh giới, một nguyên nhân.

## Sửa 2 `[2026-09-05]` — hằng thứ tư vào phạm vi, và `session` vào theo

Chủ dự án chọn kéo `Outbound::app` vào plan này thay vì mở item riêng. Phạm vi mở sang `session`.

**Hình dạng, và nó bị ép giống hệt ba hằng kia:**

```rust
pub(crate) struct Outbound<const APP: usize> { …, app: [u8; APP] }
pub struct Session<R: Role, const N: usize, const APP: usize = 1024> { … }
```

`Connection<T, R, J, N, RX, TX, APP>` → `Engine<…, N, RX, TX, APP, L>` → `serve_with` nhận hằng
thứ tư. Gọi thành `serve_with::<256, 16384, 8192, 8192, _, _>(…)`.

**Mặc định `APP = 1024` giữ nguyên**, nên gate của bước 2 không đổi: mọi test hiện có phải xanh
không sửa một call site nào.

**Vì sao `APP` là hằng riêng chứ không dùng lại `TX`.** Reply phải lọt vào hàng đợi ghi, nên
`APP > TX` là vô nghĩa và có vẻ nên gộp. Nhưng `TX` là hàng đợi cho **nhiều** message đang chờ
socket, còn `APP` là chỗ xếp **một** message; gộp chúng buộc ai muốn reply 8 KiB phải nuôi một
hàng đợi 8 KiB và ngược lại. Hai câu hỏi khác nhau, hai hằng. `docs/CONFIGURATION.md` phải nói
ràng buộc `APP ≤ TX` ra lời.

**Test bổ sung ở bước 1b:** reply 5 KiB đi được qua `APP = 8192` và không đi được qua mặc định
1024 — cùng hình dạng hai nửa như test `RX`, và lần này *"echo có về không"* **là** observable
đúng, vì nó chính là hằng đang đo.

## Nhật ký giao hàng

| Bước | Ngày | Kết quả |
|---|---|---|
| 1 | 2026-09-05 | **Xong, hai màu đỏ như plan đòi.** Type-system: `error[E0425]: cannot find function 'serve_with' in crate 'fixbolt_engine'`, cô lập còn đúng một lỗi. Hành vi: `a 5 KiB order did not reach the session through a 16 KiB buffer`. Observable phải viết lại một lần — xem Sửa 1 mục 3 |
| 2 | 2026-09-05 | **Xong.** Bốn alias nhận `const N/RX/TX` kèm default. Gate: **492 test xanh, `git diff` chỉ có `lib.rs`** — không call site, không test nào bị sửa |
| 3 | 2026-09-05 | **Xong.** `pump` nhận ba const; **cả hai `const PRE: usize = 4096` bị xoá** (`lib.rs`, `shard.rs`) và thay bằng `RX`. Invariant giờ là một biến, nên `PrefixTooLong` thành **không với tới được** — xem Sửa 1 mục 2 |
| 4 | 2026-09-05 | **Xong.** Sáu `*_with`; sáu bản cũ uỷ quyền với `256, 4096, 8192`. Chữ ký cũ không đổi một ký tự |
| 5 | 2026-09-05 | **Xong.** `library` re-export `serve_with`, `serve_hft_with`, hai bản `_recovery_with`. `Engine` vẫn không re-export, đúng *Ngoài phạm vi* |
| 1b | 2026-09-05 | **Xong.** Đỏ trước: `error[E0107]: function takes 5 generic arguments but 6 were supplied`. `Outbound<APP>`, `Session<R, N, APP>`, `Connection<…, APP>`, `Engine<…, L, APP>`, sáu `*_with` nhận hằng thứ tư. `DEFAULT_APP_SCRATCH = 1024` |
| 6 | 2026-09-05 | **Xong.** [ADR-0047](../decisions/ADR-0047-the-four-buffer-sizes-are-the-callers-through-a-second-function.md). `CONFIGURATION.md` bốn hằng, dòng gọi in nguyên văn, ràng buộc `APP ≤ TX`, số bộ nhớ đo được; `GUIDE.md` §6 thêm hàng *"không thấy gì cả"* cho `APP`; `CHANGELOG.md`; `STATUS.md` *Not proven* — **bốn mặc định chưa từng đối chiếu với đối tác thật**. `cargo doc --workspace --no-deps` sạch |

### Ba chỗ sai giữa chừng, cả ba do compiler bắt chứ không do đọc

1. **Thứ tự tham số làm hỏng mọi cách viết theo vị trí.** Chèn `APP` **trước** `L` khiến
   `Engine<…, N, RX, TX, L>` ở năm file test hiểu `L` là `APP` — bốn test target không biên dịch.
   `APP` chuyển xuống **sau** `L`, và gate "không sửa file test nào" trở lại đúng. **Một tham số
   mới thêm vào giữa là một thay đổi phá vỡ tương thích, kể cả khi nó có default.**
2. **`Connection` thiếu default cho `APP`** — `= 1024`, cùng lý do.
3. **Một sweep nữa, một trần nữa, và lần này nó thuộc *dụng cụ đo*.** Nâng `APP` lên 8 KiB
   không đẩy tường lên 8 KiB mà lên khoảng 3–5 KiB: `fixbolt_conformance::echo` xếp reply trong
   `TemplateBuilder::<128, 4096>` của riêng nó. Test dùng 3 KiB và **nói ra vì sao**.

### Gate bị vi phạm, và nó được báo chứ không được giấu

Plan viết: *"492 test hiện có phải xanh **không sửa một dòng test nào**"*. **Vi phạm ở đúng một
file**: `crates/session/src/out.rs`, hai unit test, thêm turbofish
`Outbound::<{ crate::DEFAULT_APP_SCRATCH }>::new(…)`. Không né được — **một const parameter
không được suy ra ở vị trí biểu thức, kể cả khi struct có default**; đã thử default trước, vẫn
`E0284`. Hai dòng đó chỉ thêm chú thích kiểu, **không đụng assertion nào**, nên ý nghĩa của test
không đổi. Mọi file trong `crates/*/tests/` và `tools/` **không bị chạm** — `git diff --stat`
trên chúng rỗng.

## Sửa 3 `[2026-09-05]` — hai điều tôi báo sai về alloc bench, CI bắt được

**1. Tên case sai.** Tôi báo *"case 21 `log-busy`"*. Đếm lại danh sách 24 case, **index 21 là
`log-record`**. Tên đó là tôi đoán từ trí nhớ chứ không đọc từ output — đúng dạng sai mà
`CLAUDE.md` §10 gọi là *một kết quả được suy ra chứ không được quan sát*. Commit `f3fca4c` đã
push mang tên sai này trong thông điệp; không sửa lại lịch sử đã push, đính chính ở đây.

**2. "Có sẵn, cùng giá trị" là kết luận may hơn là đúng.** Tôi so **một mẫu** trên nhánh với
**một mẫu** trên `main`, thấy cả hai đọc `2`, và kết luận nguyên nhân. Chạy ba lần liên tiếp trên
macOS:

```
lần 1: [… index 21 = 1 …]
lần 2: [… index 21 = 2 …]
lần 3: [… index 21 = 2 …]
```

**Nó không tất định.** Hai mẫu trùng nhau của một đại lượng dao động không chứng minh gì về
nguyên nhân — đúng cái bẫy `CLAUDE.md` §10 tên là *"một nguyên nhân được chấp nhận vì có một
knob nhúc nhích cùng nó"*. Kết luận *"không phải do thay đổi này"* vẫn đứng, nhưng thứ chứng minh
nó là **CI xanh trên chính commit này**, không phải phép so sánh kia.

**3. Và trên máy chạy gate thật thì nó xanh.** CI run `33891879514`, job *"Benchmarks run, and the
machine-independent ones must pass"*: cả 24 case đọc **0**, `log-record` trong đó. Nên đây là
**đỏ chỉ trên macOS**, hình dạng đã biết —
[a-benchmark-measured-its-own-fixture](../reference/a-benchmark-measured-its-own-fixture.md) ghi
rằng ba case log mất bốn lần đo mới ra 0 vì hai lần đầu đo harness và writer thread. Bộ đếm là
một atomic `Relaxed` dùng chung với writer thread; trên macOS cấp phát của writer đôi khi rơi vào
cửa sổ đo. **Nó là một flake của phép đo, không phải một cấp phát trên hot path** — và
`STATUS.md` item 23 đã có hình dạng ngược lại (*xanh ở CI, đỏ ở máy đang làm*).

**Gate đã chạy `[2026-09-05, macOS, không phải máy §9 — plan này không công bố số hiệu năng nào]`:**

| Gate | Kết quả |
|---|---|
| `cargo test --all` | **495 passed, 0 failed** (492 + 3 test mới) |
| `cargo test --no-default-features`, **từng crate** | codec 69, session 94, engine 209, fixbolt 5 — **0 failed** |
| `cargo clippy --all-targets -- -D warnings` | sạch |
| `cargo check --all-features --target x86_64-unknown-linux-gnu` | **Finished** — `shard.rs` biên dịch, và nó không bao giờ biên dịch trong một `cargo check` local |
| `benches/alloc.rs` | **ĐỎ trên macOS — xem Sửa 3, hai điều tôi báo sai.** **Xanh trên CI Linux**, cả 24 case đọc 0 |
