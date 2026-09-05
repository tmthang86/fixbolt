# Cửa trước trả về tay cầm

> **Loại:** Plan · **Ngày:** 2026-09-05 · **Trạng thái:** ĐÓNG 2026-09-05
> **Phạm vi:** `STATUS.md` item 47. Chạm `engine` (`Engine`, `observe`, `origin`, mười entry
> point `serve*` / `connect_and_serve*`, vòng `dial`), `library` (re-export, `README`),
> `tools/interop`, tài liệu, một ADR mới. **Không chạm** `codec`, `dict`, `session`, `transport`,
> `shard`.
>
> **Máy chạy:** macOS đủ cho toàn bộ test; gate quyết định là job `interop` trong CI.
> **Không cần máy §9.** **Thời lượng dự kiến:** 2 ngày.
>
> **Thứ tự:** sau [a-journal-that-knows-the-numbering](2026-09-05-a-journal-that-knows-the-numbering.md)
> (item 48), trước đợt B — item 45 (c). Plan này **không** cần plan kia, nhưng plan kia nhỏ hơn
> và đang ghim một gate đỏ, nên đi trước.

## Sửa, ghi trước khi code dịch chuyển

**Sửa 1 — số ADR là 0054, không phải 0053.** `[2026-09-05]` Plan này viết *"ADR-0053 mới"* vào
buổi sáng, và plan item 48 đóng vào buổi chiều cùng ngày, lấy đúng số đó
([ADR-0053](../decisions/ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md),
`Accepted`). §5 không cho dùng lại số. Mọi chỗ trong plan này đọc *ADR-0053* nghĩa là
**ADR-0054**; các mục tài liệu ở dưới đã sửa tên file.

**Sửa 2 — con số nền của `cargo test --all` không còn là 506.** Item 48 thêm test; số nền đo
lại ngay trước khi bắt đầu bước 2 và ghi vào nhật ký. Điều kiện *"phải tăng"* không đổi, chỉ có
mốc so sánh đổi.

**Sửa 3 — `GUIDE.md` §8c điểm 5 đã được item 48 sửa rồi.** Bảng *Tài liệu* dưới đây viết điều
kiện *"cùng plan item 48 nếu plan đó chưa đóng"*; plan đó **đã đóng**, nên việc còn lại của plan
này ở đoạn ấy chỉ là **đọc lại để hai plan không nói trái nhau**, không phải sửa lần nữa.

**Sửa 4 — cú "dừng" của `tools/interop` đến từ stdin, không phải `SIGTERM`.** Plan viết *"cài
`SIGTERM` → `admin.shutdown(2_000)`"*. Bắt tín hiệu trong Rust ở đây cần **hoặc** `libc` cộng một
`unsafe extern "C"` — §2 rule 8 đòi một plan và một câu chứng minh tính đúng cho chỗ ấy — **hoặc**
một dependency mới cho một tool. Cả hai đều không phải điều gate này nói tới. Thứ đang được kiểm
là *"`serve` quay về **vì** `Admin::shutdown` được gọi"*; **ai bóp cò không phải chủ đề**. Nên vai
acceptor đọc một dòng (hoặc EOF) trên stdin và gọi `admin.shutdown`, còn `scripts/interop.sh` giữ
đầu ghi của một fifo và viết một dòng vào đó. `Admin::shutdown` là hai atomic store, không cần
async-signal-safe gì cả.

**Sửa 5 — test *"`serve` trả về vì `Admin::shutdown`"* nằm ở `crates/library/tests/end_to_end.rs`,
không phải `crates/engine/tests/shutdown.rs`.** `shutdown.rs` lái một `Engine` trên `Loopback`,
không có socket và không có `#![cfg(standard, unix)]`; `end_to_end.rs` đã có cả hai và đã gọi
`fixbolt::serve` qua socket thật. Đặt test ở đó cũng đúng khán giả hơn: câu sai trong
`GETTING-STARTED.md` là câu nói với **người dùng thư viện**.

## Bối cảnh

`fixbolt::serve` và năm entry point cùng nhà **tự dựng `Engine` bên trong và chỉ trả về một
`Shutdown`** — sau khi mọi thứ đã kết thúc. `Engine::observer()`, `admin()`, `sender()` đều cần
một `&mut Engine`, mà người gọi cửa trước không bao giờ cầm. Hệ quả, đọc thẳng từ code:

- **Không nhìn được**: không snapshot, không event, không `events_lost`. Toàn bộ item 30 (quan
  sát engine đang chạy) chỉ dùng được bởi ai tự dựng `Engine`.
- **Không quản trị được**: cú điện thoại 3 giờ sáng (`Admin::SetNextOut`) không gọi tới được.
- **Không dừng sạch được.** `serve` chỉ trả về khi `shutdown_finished()` có kết quả, mà thứ duy
  nhất khởi động nó là `Admin::shutdown(grace)`. Nên `docs/GETTING-STARTED.md:186` — *"`serve`
  returns when an operator stops the engine through `Admin::shutdown`"* — là câu **không ai
  làm được** qua chính API trang đó dạy. `tools/interop` cũng vậy: vai acceptor chạy tới khi bị
  `kill`.
- **Cửa 2 của ADR-0048 không có ở cửa trước.** `Sender` là handle lấy từ `Engine::sender()`;
  qua `serve` chỉ tới được cửa 1 (`on_logon`). Một fill về sau lệnh một giây vẫn không gửi được
  bằng thư viện.

Lỗ hổng này **cũ hơn item 46** và được tìm ra khi sửa item 46: mọi test của `observe` lái một
`Engine` trực tiếp, nên không test nào hỏi *"qua `serve` thì sao"* — cùng hình với item 16
(ADR-0034: *một tầng xong xuôi, tầng trên nó chưa bao giờ được hỏi*).

## Những gì đã biết chắc (đọc code 2026-09-05)

| Sự thật | Nguồn |
|---|---|
| `Engine.observe: Option<Arc<observe::Shared>>`, tạo **lười** ở lần gọi đầu của một trong ba handle; ba handle là ba `Arc` lên **cùng một** cell | `crates/engine/src/lib.rs:172, :565–620` |
| `Shared::new()` là `pub(crate)` | `crates/engine/src/observe.rs:330` |
| `Observer(Arc<Shared>)`, `Admin(Arc<Shared>)`, `Sender(Arc<Shared>)` — đều là newtype public với field `pub(crate)` | `observe.rs:381, :978`; `origin.rs:206` |
| Sáu entry point acceptor + hai initiator + hai `_with` mỗi loại = **mười** hàm trong `lib.rs`, cùng hình: dựng `Engine::new(...)`, `.with_log(log)`, gọi `pump`/`dial`, trả `Result<Shutdown, ServeError>` | `lib.rs:1480, :1518, :1589, :1618, :1768, :1794, :1835, :1852, :1891, :1916` |
| `shard::serve_sharded_hft(_with)` là hai hàm nữa, **không có** `observer/admin/sender` nào trong `shard.rs` | `crates/engine/src/shard.rs:440, :469` |
| `with_log` chuyển `observe` nguyên trạng sang engine mới | `lib.rs:227–243` |
| `dial` **tự** gọi `engine.observer()` và **xả** event ring để bắt `LoggedOn` cho `Policy::logged_on()` | `lib.rs:1650–1730` |
| `Observer::events` **xả và xoá** khỏi ring — hai reader trên một cell chia nhau event, không nhân đôi | `observe.rs:414–416` |
| `Observer::request` chỉ đặt cờ `wanted` và đọc cell; engine chỉ dựng snapshot khi có người hỏi — **một relaxed load mỗi turn** | `observe.rs:394–400`; `lib.rs:565` |
| `Admin::shutdown(grace_ms)` là thứ duy nhất làm `shutdown_finished()` trả `Some` | `observe.rs:1020`; `lib.rs:772` |
| `Sender::send(id: ConnId, msg)` cần `ConnId`; `Snapshot::sessions()` liệt kê id | `origin.rs:240`; `observe.rs:208` |
| `fixbolt` re-export `Observer, Admin, Sender, Snapshot, Event, …` nhưng **không** re-export `Engine` | `crates/library/src/lib.rs:27–77` |
| ADR-0047 chốt: hàm gốc giữ nguyên chữ ký, `_with` nhận bốn const; **chưa có gì được publish** | `docs/decisions/ADR-0047-*.md`; `CLAUDE.md` đầu trang |
| Sáu ví dụ gọi cửa trước trong tài liệu: `GETTING-STARTED.md:160`, `TUTORIAL.md:170`, `library/README.md:50`, `GUIDE.md:312, :396, :668`, `GUIDE.md:1102` (initiator) | grep 2026-09-05 |
| `GUIDE.md` §8a dạy `engine.observer()`, §8c *3 a.m.* dạy `engine.admin()` — cả hai giả định người đọc cầm `Engine` | `docs/GUIDE.md:950, :1154` |
| `benches/alloc.rs` có `admin-idle/busy`, `origin-idle/busy`: turn của engine có handle mà không ai dùng tốn **một relaxed load** | `crates/engine/src/lib.rs:625–640`; `DESIGN.md` §6 |
| `cargo test --all` hôm nay **506** | `STATUS.md` |

## Cách làm

**Tay cầm ra đời trước engine, và engine nhận nuôi nó.** Không callback, không hàm sinh đôi.

### `fixbolt::Handles` — một cell, ba năng lực, dựng trước

```rust
pub struct Handles(Arc<observe::Shared>);        // Send + Sync + Clone
impl Handles {
    pub fn new() -> Self;                         // MỘT cấp phát, ở đây, không bao giờ trên turn
    pub fn observer(&self) -> Observer;           // nhìn
    pub fn admin(&self) -> Admin;                 // đổi
    pub fn sender(&self) -> Sender;               // nói
}
```

Đây chính là `Arc<Shared>` mà `Engine::observer()` đang tạo lười — chỉ khác là **người gọi tạo
nó trước** và đưa vào. `Engine` mọc `pub fn adopt(&mut self, h: &Handles)`: đặt `self.observe =
Some(h.0.clone())`, từ chối (trả `false`) nếu đã có cell — hai cell trên một engine là hai sự
thật, và ba method cũ vẫn hoạt động y nguyên cho ai lái `Engine` trực tiếp.

Nghĩa của nó với người dùng:

```rust
let handles = fixbolt::Handles::new();
let admin = handles.admin();
ctrlc::set_handler(move || admin.shutdown(5_000))?;     // thứ GETTING-STARTED hứa mà chưa làm được
let shutdown = fixbolt::serve(addr, table, app(Desk), 64, limits, NoLog, handles)?;
```

Tay cầm có sẵn **trước** khi thread nào chạy, nên không cần channel, không cần chờ callback, và
`Sender` đi sang thread fill bằng một `clone()` thường.

### Mười entry point nhận `handles: Handles` làm tham số cuối

Tất cả mười hàm trong `lib.rs` — kể cả các hàm gốc — thêm **một** tham số. Đây là chỗ đi ngược
ADR-0047 quyết định 2 (*"hàm gốc giữ nguyên chữ ký"*), và cố ý: chưa có gì publish, và lựa chọn
khác là sáu hàm sinh đôi nữa (**mười sáu** hàm cho một điều) hay callback trên `Application`
(bị tầng chặn — `session` không gọi tên được `engine::Handles`, xem ADR). Bên trong: `engine.adopt(&handles)`
ngay sau `Engine::new`, trước `pump`/`dial`.

**Không có `Option<Handles>`.** Một `Handles::new()` là một cấp phát lúc khởi động; buộc người
dùng cầm nó là buộc họ có cách dừng engine — và tài liệu đã hứa điều đó từ 2026-09-03.

### `dial` thôi xả event ring của người dùng

`dial` đang tự lấy `observer()` rồi `events()` để thấy `LoggedOn`. Với cell của người dùng, hai
reader **chia nhau** event: cái `dial` xả thì `Observer` của họ không thấy. Sửa: `Engine` mọc
`pub fn logons(&self) -> u64` — bộ đếm tăng ở đúng chỗ `LoggedOn` được đẩy — và `dial` so số cũ/mới
thay vì đọc ring. Không đổi gì trên turn: bộ đếm là một `u64` tăng ở chỗ vốn đã ghi event.

### `tools/interop` dùng cửa trước để dừng, và để nói

Vai acceptor: cầm `Handles`, cài `SIGTERM` → `admin.shutdown(2_000)`, in `Shutdown` ra transcript.
Vai reconnect: cầm `Observer`, in `next_out` từ snapshot **cạnh** số `from_journal` trả về mỗi lần
nối lại — hai nguồn phải bằng nhau, và đó là một assertion mới của `scripts/interop.sh`
(`two_sources`), thứ mà item 48 nói là *"unreachable"* hôm nay.

### File sẽ tạo hoặc sửa

`crates/engine/src/observe.rs` (`Handles`) · `crates/engine/src/lib.rs` (`adopt`, `logons`, mười
chữ ký, `dial`) · `crates/library/src/lib.rs` (re-export) · `crates/library/README.md` ·
`crates/engine/tests/observe.rs`, `admin.rs`, `originate.rs`, `reconnect.rs`, `shutdown.rs`,
`wire.rs` (mọi test gọi `serve*`) · `crates/library/tests/end_to_end.rs` · `tools/interop/src/*.rs`
· `tools/w2w` nếu gọi `serve` · `scripts/interop.sh` · `docs/decisions/ADR-0054-*.md` (mới).

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **1 — không cấp phát trên hot path** | `adopt` không thêm gì lên turn; `logons` là một `u64` | `benches/alloc.rs` chạy lại: `admin-idle`, `origin-idle` đọc **0** như cũ; case mới `serve-adopted-idle` dựng engine qua `adopt` rồi turn rỗng, đọc 0 |
| **4 — engine thread không ngủ (`hft`)** | không thêm lock nào | `scripts/check-no-kernel-sleep.sh` chạy lại (`tools/w2w` là thứ nó đo, và `w2w` đổi sang cầm `Handles`) |
| **2, 3 — session thuần, 59 định nghĩa** | không chạm `session` | chạy `--test score`, đọc 59/59 |
| **6 — feature gate `mod`** | `Handles` sống trong `observe`, không feature | `cargo test --no-default-features` |
| **10** | không có số mới | — |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | **ADR-0054 `Proposed`**: cell dựng trước thay vì callback hay sinh đôi; vì sao đi ngược ADR-0047 quyết định 2 và ADR-0047 **không** bị supersede (bốn const vẫn đúng, chỉ có *"chữ ký gốc bất biến"* rơi); vì sao không `Option`; builder `Serve` được ghi là phương án *hoãn* với điều kiện mở lại | — |
| 2 | `Handles`, `Engine::adopt`, `Engine::logons`. **Test đỏ trước**: `crates/engine/tests/observe.rs::a_handle_made_before_the_engine_sees_its_first_logon` — dựng `Handles`, dựng `Engine`, `adopt`, logon qua transport giả, `observer.request()` phải thấy một session; `adopt` lần hai trả `false` | 1 |
| 3 | Mười chữ ký + `dial` không xả ring. Test: `reconnect.rs` — `Observer` của người gọi **vẫn nhận** `LoggedOn` sau khi `dial` đã thấy nó; `shutdown.rs` — `admin.shutdown` từ thread khác làm `serve` **trả về** với `Shutdown::clean()` | 2 |
| 4 | `library` re-export, `README`, sáu ví dụ tài liệu; `tools/interop` dừng bằng `Admin`, `two_sources` trong `scripts/interop.sh`; `tools/w2w` | 3 |
| 5 | Alloc case, `check-no-kernel-sleep.sh` trên Linux, ADR `Accepted`, bảng §4, `GETTING-STARTED.md:186` nay **đúng** | 4 |

## Cách kiểm chứng

- **Gate quyết định là bước 3 và 4.** (i) `serve` trả về vì `Admin::shutdown` — lần đầu tiên
  trong repo này một entry point kết thúc mà không có `kill`; transcript interop in
  `acceptor: stopped: Shutdown { sessions: 1, acked: 1, .. }`. (ii) `two_sources`:
  `interop-reconnect: resuming next_out=N` và `observer next_out=N` cùng một số, ba lần nối lại.
- **Đảo ngược, mỗi cái đỏ đúng chỗ:** (a) `adopt` bỏ qua cell đưa vào (tự tạo cell mới) →
  `a_handle_made_before_the_engine_sees_its_first_logon` đỏ, `two_sources` đỏ với `observer
  next_out=none`; (b) `dial` quay lại xả ring → test `reconnect.rs` mới đỏ, mọi test khác xanh
  — chứng minh assertion đọc đúng seam; (c) bỏ `admin.shutdown` khỏi `tools/interop` → job
  `interop` treo tới timeout của script, và script phải **có** timeout đó (nếu chưa có, thêm là
  một mục của bước 4).
- `cargo test --all`, `--no-default-features`, **đọc số**: 506 phải tăng.
- `cargo clippy --all-targets -- -D warnings`; `check-lint-config.sh`; `check-no-optional-deps.sh`;
  `cargo doc --workspace --no-deps` dưới `-D rustdoc::broken_intra_doc_links` (job `docs`).
- `scripts/bench.sh`; Linux: `check-no-kernel-sleep.sh` cả hai lượt.
- **Một CI run xanh, gọi tên bằng id, cho đúng commit đóng plan.**

## Tài liệu phải cập nhật

- [ ] `docs/decisions/ADR-0054-*.md` — mới; ADR-0047 **không** sửa (đã Accepted), ADR mới nói
      rõ điều gì của nó còn đứng
- [ ] `DESIGN.md` §3 (public API của `library`), §4 — D15 mọc đoạn *cửa 2 tới được qua cửa trước*;
      ADR-0036 *một cơ chế hai năng lực* nay là **ba năng lực, một cell dựng trước**; đi lại §2
- [ ] `docs/GETTING-STARTED.md` — bước bootstrap có `Handles`, câu ở dòng 186 nay đúng
- [ ] `docs/TUTORIAL.md`, `crates/library/README.md`, `docs/GUIDE.md` §1b, §1c, §6b, §8a, §8c,
      *3 a.m.* — mọi ví dụ `serve*`/`connect_and_serve*` thêm tham số; §8a/§8c bỏ giả định cầm
      `Engine`; §8c điểm 5 sửa (**cùng plan item 48 nếu plan đó chưa đóng — hai plan không được
      để câu này trái nhau**)
- [ ] `docs/CONFIGURATION.md` — không có knob mới; kiểm tra bảng entry point nếu có
- [ ] `docs/best-practices-standard.md`, `docs/best-practices-hft.md` — cách dừng sạch, **gọi
      tên mode**
- [ ] `CHANGELOG.md` — mười chữ ký đổi (**breaking**), `Handles`, `Engine::adopt`, `Engine::logons`
- [ ] `docs/reference/` — nếu bước 3 tìm ra điều gì đo được về hai reader trên một ring
- [ ] `STATUS.md` — gạch item 47; item 30 ghi chú *nay tới được qua cửa trước*; **đi qua *Not
      proven***

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Sửa `Engine` mà seam trên nó không được hỏi — chính cái bẫy đẻ ra item 47 và item 16 | mọi test mới đi qua `serve*`/`connect_and_serve*`, **không** qua `Engine` |
| `dial` và người dùng chia nhau event | test `reconnect.rs` mới + reversal (b) |
| Cell đưa vào bị thay bằng cell tự tạo (một `get_or_insert_with` còn sót) | reversal (a); `grep -n get_or_insert_with lib.rs` phải chỉ còn trong ba method cũ, và ba method đó **dùng** cell đã adopt |
| `Sender` qua cửa trước cần `ConnId` mà người dùng không biết | rustdoc `Handles::sender` chỉ sang `Snapshot::sessions()`; test `originate.rs` lấy id từ snapshot rồi gửi |
| `Handles` bị drop trước engine → `Arc` vẫn sống, không sao; nhưng người dùng tưởng drop là "thôi quan sát" và engine vẫn trả một relaxed load mỗi turn | rustdoc nói rõ; `admin-idle` chứng minh giá là một load |
| `serve` treo mãi trong CI nếu `shutdown` không tới | timeout trong `scripts/interop.sh`, reversal (c) |
| Quên `tools/w2w` → build đỏ ở workspace | `cargo test --all` |
| Hai plan (47, 48) cùng sửa `GUIDE.md` §8c điểm 5 | mục ở *Tài liệu* ở trên; plan đóng sau đọc lại đoạn đó |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| ADR-0054 kết luận builder `Serve` thay vì tham số thứ bảy | trung bình | plan sửa bước 3 thành *builder + mười hàm thành shim*, ghi Sửa 1; các bước 2, 4, 5 không đổi |
| Mười chữ ký đổi làm mọi test/tool/doc gọi `serve` đổi theo | chắc chắn | đó là bước 3–4, đếm trước bằng grep và ghi số vào nhật ký |
| `shard` không có handle và plan này không chạm nó | thấp | ghi rõ ở *Ngoài phạm vi*; một `Handles` cho N shard là câu hỏi `ConnId` theo shard — ADR nêu là câu hỏi mở |

## Ngoài phạm vi

`serve_sharded_hft(_with)` — N engine, N cell, và `ConnId` chỉ duy nhất trong một shard; cần
thiết kế riêng. Builder `Serve` (ghi trong ADR là phương án hoãn). Một `on_ready` callback trên
`Handler` (bị tầng chặn — ADR ghi vì sao). Sửa số chiều ra (item 48, plan riêng). `RingDispatch`.

## Nhật ký giao hàng

**Đóng 2026-09-05, cả năm bước.**
[ADR-0054](../decisions/ADR-0054-the-handles-are-made-before-the-engine-and-the-engine-adopts-them.md)
`Accepted`. `STATUS.md` item 47 gạch.

### Làm được gì

| Bước | Kết quả |
|---|---|
| 1 | ADR-0054, số sửa từ 0053 (Sửa 1). Chốt: cell dựng trước, `adopt`, **không** callback (bị tầng chặn), **không** sinh đôi; builder `Serve` ghi là *hoãn* kèm điều kiện mở lại |
| 2 | `observe::Handles` (`new`/`observer`/`admin`/`sender`), `Engine::adopt` (từ chối cell thứ hai), `Engine::logons` |
| 3 | Mười chữ ký nhận `handles: Handles`; `dial` thôi xả event ring, đọc `logons()` |
| 4 | `library` re-export, `README`, `examples/acceptor.rs`, `GETTING-STARTED`, `TUTORIAL`, `GUIDE` §1b/§6b/§8a/§8c, `CONFIGURATION`, `PRD`; `tools/interop` dừng bằng `Admin`; `scripts/interop.sh` thêm `shutdown` và `two_sources` |
| 5 | `benches/alloc.rs` case `adopt-idle`; `DESIGN.md` §3/§4 D15/§6; `CONFORMANCE.md`; `CHANGELOG.md`; `best-practices-standard.md` §9 và `best-practices-hft.md` §8; đi qua *Not proven* |

### Số đo

- `cargo test --all` **519 → 524**, 0 failed. `--no-default-features` **519**, sạch.
- `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `check-lint-config.sh` (đảo ngược),
  `check-no-optional-deps.sh`, `check-links.py` (1 556 link, 0 chết), `cargo doc` dưới
  `-D rustdoc::broken_intra_doc_links` — tất cả sạch.
- `benches/alloc.rs`: **0 trên 30 / 30 case**, có `adopt-idle` mới.
- `scripts/interop.sh`: **7 / 7 + 8 / 8 + 6 / 6 + 6 / 6 + 6 / 6** = 33 assertion, exit 0.
  `interop-acceptor: shutdown ok Shutdown { … }` — lần đầu một tiến trình trong repo này được
  **hỏi** cho dừng thay vì bị `kill`.

### Đảo ngược, mỗi cái đỏ đúng chỗ

| Đảo | Đọc được gì |
|---|---|
| `adopt` bỏ cell được đưa, tự tạo cell mới | `the pre-made handle never saw the session log on; last snapshot: None` — hai test mới đỏ, hai test cũ vẫn xanh |
| `dial` quay lại xả ring | `the caller's Observer saw 0 LoggedOn event(s) of two` — chỉ test mới đỏ |
| `serve` không `adopt` | không có `35=5` nào quay lại, test đọc timeout ở đúng chỗ chờ lời chào |
| `stop_on_stdin` không gọi `admin.shutdown` | gate **đỏ trong 29 giây**: *"did not return from serve within 10s"* — có chặn thời gian nên đảo ngược không treo |
| một cấp phát trong nhánh `observe` của `turn` | `adopt-idle 10000`, còn `idle` và `turn` (engine không ai quan sát) vẫn 0 |
| bỏ `mark_out` (đảo ngược của item 48) | `interop-reconnect-beat: two_sources BEHIND (resumed 3, already seen live 5)` — assertion mới có răng thật |

### Không làm, và nói rõ

- **`shard::serve_sharded_hft(_with)` không nhận `Handles`.** Ngoài phạm vi từ đầu, và nay bất
  đối xứng ấy **nhìn thấy được** trong API công khai: mười hàm có, hai hàm không. ADR-0054 ghi
  là câu hỏi mở, không phải sót.
- **`benches/turn.rs` vẫn không có con số giá.** Nó chạy nhưng rơi vào `cases w/o a baseline`.
  Plan này đóng trên container, không phải desktop §9.
- **Assertion `two_sources` là bất đẳng thức, không phải đẳng thức** — xem *Điều tìm ra* dưới.

### Điều tìm ra khi làm, plan không nói

**Một — `two_sources` không thể là đẳng thức, và lý do đã nằm sẵn trong ADR-0053.** Plan viết
*"hai nguồn phải bằng nhau"*. `[đo 2026-09-05]` bản đầu đòi `live == resumed + 1` và đọc
`resumed 4, live 6`: ứng dụng còn nói trước lúc logon, nên hằng số cũng sai — và một gate dựa
trên hằng số như thế sẽ vỡ khi **ứng dụng** đổi, chứ không phải khi giao thức đổi. Lý do sâu hơn
là chính lập luận của ADR-0053, quay lại áp lên plan đi sau nó: observer biết con số **lúc có
người hỏi**, nên một message gửi giữa lần hỏi cuối và lúc kết thúc là đã tiêu, đã bền, và **vô
hình** ở phía ấy — và với một logout sạch thì **luôn luôn** như vậy, vì trả lời `35=5` và rớt
link nằm trong cùng một turn. Nên assertion là **chiều**: thứ người vận hành thấy đã tiêu thì
journal biết. Lấy mẫu chỉ có thể làm số live **thấp đi**, nên bất đẳng thức an toàn ở chỗ đẳng
thức là một cuộc đua.

**Hai — bốn entry point vượt ngưỡng `clippy::too_many_arguments` (8 / 7).** Chúng nhận `#[allow]`
kèm comment trỏ về ADR-0054 — nơi builder `Serve` đã được ghi là *hoãn* với điều kiện mở lại.
Lint chính là điều kiện ấy đến sớm và bị từ chối **một lần**, có chủ ý; ghi lại chứ không bịt,
để tham số tiếp theo gặp một sự thật chứ không phải một thói quen.

**Ba — `DESIGN.md` §6 vẫn mang dòng reconnect của trước ADR-0053.** Nó còn đọc *"**3 / 3** sau
`SIGTERM` … refused by **exactly one**, which is the known gap item 48 names"*, trong khi
`STATUS.md` và `CONFORMANCE.md` đều đã sửa. **Bảng đồng bộ §4 đi bằng tay, và một bàn tay bỏ sót
đúng một dòng của nó.** Sửa trong plan này.
