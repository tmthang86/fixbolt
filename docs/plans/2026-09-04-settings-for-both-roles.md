# File cấu hình cho cả hai vai, và những knob một FIX desk mong có sẵn

> **Loại:** Plan · **Ngày:** 2026-09-04 · **Trạng thái:** **Đã duyệt 2026-09-05** (xác minh lại
> 2026-09-04, và **lần hai 2026-09-05** sau khi items 46, 48, 47 đóng — xem *Sửa 2*)
> **Phạm vi:** `STATUS.md` item 45, đợt B, plan thứ nhất. Chạm `engine` (`settings`, `presession`,
> `reconnect`, entry point), `session` (`Config`, vài rule reset), `library` (`Reply`), docs.
> **Không chạm** `codec`, `dict`, `transport`.
>
> **Máy chạy:** macOS đủ. **Thời lượng dự kiến:** 2–3 ngày.

> ## Sửa 1 `[2026-09-04]` — đọc lại toàn bộ *Những gì đã biết chắc* theo code sau đợt A
>
> Draft này viết **trước** khi đợt A chạy và tự đặt điều kiện: đọc lại từng dòng theo code lúc
> đến lượt, sửa, rồi mới đổi trạng thái. Đã làm. **Sáu dòng sai, hai phát hiện mới**, và một
> trong hai phát hiện không nằm trong phạm vi mà draft tưởng tượng.
>
> | Draft nói | Code hôm nay | |
> |---|---|---|
> | "Mười key hiện có" | **Mười một** — `FileLogPath`, `[DEFAULT]` **only**, engine-wide (một engine một file, `conn=`/`shard=` phân biệt bên trong) | sửa |
> | `Config` sáu trường | **Bảy** — thêm `resend_batch: u16`, mặc định 8 (ADR-0046) | sửa |
> | `connect_and_serve(addr, cfg, app, policy, recovery)` tại `lib.rs:1088` | **sáu tham số**, thêm `log: L`, tại **`lib.rs:1298`** | sửa |
> | `identity_of` đọc `49/56/35` (+`50/57`) | đọc **`49/56/50/57`**, **không** đọc `35` | sửa |
> | "30 test hiện có" cho settings | **40** — `settings.rs` 35 + `settings_wire.rs` 5 | sửa |
> | "phải đọc `frame.rs`" — ngụ ý `crates/codec/src/frame.rs` | file là **`crates/engine/src/frame.rs`**; và câu hỏi của nó **đã có câu trả lời**, xem *Phát hiện 1* | sửa |
>
> **Bốn dòng đứng nguyên, đã kiểm từng cái chứ không tin theo:** `Policy` (`reconnect.rs:47`,
> sáu trường, **không jitter**); `Registry::lookup(Identity<'_>) -> Option<&Entry>`
> (`presession.rs:213`, số dòng đúng nguyên văn); `Limits::new(pending, logon_ms)`
> (`presession.rs:420`); `Validation { body_length, check_sum }` (`parse.rs:86`, số dòng đúng
> nguyên văn). `Reply` vẫn là `message`/`field`/`send`, **cộng thêm** `group` và
> `send_with_groups`.

> ## Sửa 2 `[2026-09-05]` — xác minh lần hai, sau khi items 46, 48 và 47 đóng
>
> `STATUS.md` item 45 (c) nói rõ vì sao lần xác minh này bị giữ lại: *"`ResetOn*` là chuyện đánh
> số, mà item 48 làm cho bền; và entry point của nó là mười chữ ký item 47 đổi — xác minh trước
> thì phải làm hai lần"*. Cả ba đã đóng và đã lên `main` (`52a2895`, `edb0121`, `3000334`), nên
> đây là lúc. Đọc lại **từng dòng** của *Những gì đã biết chắc* theo code hôm nay chứ không tin
> theo lần trước.
>
> **Bảy dòng sai. Một bước gần như biến mất, một bước mất hẳn nửa đầu.**
>
> | Plan nói | Code hôm nay (`3000334`) | |
> |---|---|---|
> | `connect_and_serve(addr, cfg, app, policy, recovery, log)` tại `lib.rs:1298` | **bảy tham số**, thêm `handles: crate::observe::Handles` ở cuối (ADR-0054), tại **`lib.rs:1644`** | sửa |
> | "Sáu entry point" (*Phát hiện 2*) | **Mười hai.** ADR-0047 cho mỗi cái một bản sinh đôi `_with` nhận bốn hằng buffer. **Mười cái trong `lib.rs` nhận `Handles`; hai cái trong `shard.rs` thì không** | sửa |
> | "40 test settings hiện có" | **39** — `settings.rs` **34**, không phải 35. Con số 35 đến từ `grep -c '#\[test\]'`, và một dòng văn xuôi ở đầu file (`settings.rs:27`, *"A `#[test]` edited to go green…"*) bị đếm là một test | sửa |
> | `Reply` "chi phí 766 ns (item 34)" | **804,1 ns**, `library, reply only`, máy §9 (Ryzen 7 3700X), median của 20 lần, `benches/baselines.tsv:196`. 766 ns là số của một VM cũ **không kèm máy** — đúng thứ §2 điều 10 gọi là claim của người khác. Và tỉ lệ đúng là **3,4×** chứ không phải 24× (ADR-0051) | sửa |
> | `Limits::new` `presession.rs:412–441` | **420**. *Sửa 1* đã ghi 420 rồi mà bảng dữ kiện vẫn giữ 412 — **một plan tự mâu thuẫn với chính bản sửa của nó** | sửa |
> | `141=Y` tại `session/src/lib.rs:1124, 2099` | **1241 và 2325**; `Session::new` **809**, `resume` **868**, `resume_at` **893** | sửa |
> | `Reply` tại `library/src/reply.rs:169–240` | **212–286** (`message` 212, `field` 256, `group` 269, `send` 276, `send_with_groups` 286) | sửa |
>
> **Mười dòng đứng nguyên, kiểm từng cái:** 11 key (`settings.rs`, enum `Key` không thêm biến
> thể nào); `Settings::load/parse/log/configs/into_table` vẫn ở **366 / 379 / 480 / 485 / 491**;
> `Config` **bảy trường** và vẫn `Copy` (`session/src/lib.rs:337–338`); `Policy` sáu trường,
> **không jitter** (`reconnect.rs:46`); `Registry::lookup` `presession.rs:213`; `identity_of`
> đọc `49/56/50/57` và **không** đọc `35` (`presession.rs:122`); `Validation { body_length,
> check_sum }` `parse.rs:86`; `Cut::Garbage` `frame.rs:44` và pre-session vẫn trả `Step::Gone`
> **im lặng** (`presession.rs:690`, gộp chung với `p.gone` của một socket vừa đóng);
> `grep -rni "reset_on\|ResetOnLogon\|ResetPolicy" crates/` vẫn **rỗng**; `serve_sharded_hft`
> vẫn là entry point duy nhất **không nhận `Recovery`**.
>
> ### Việc phải làm đổi ba chỗ
>
> **1. Bước 0 chỉ còn cái ADR.** Nửa sau của nó — *"Sửa `prior-art.md:143`"* — **đã xong**, ngày
> 2026-09-04, ngay trong lần xác minh thứ nhất: dòng đó bây giờ đọc `[corrected 2026-09-04]` và
> giải thích tag 383. Bước 0 không được đòi công đã trả.
>
> **2. Bước 6 mất nửa đầu, và mất vì một người khác đã làm đúng hơn.** Plan đòi
> `const _: () = assert!(PRE == RX);` ở `lib.rs:1635` và `shard.rs:471`. **Hai hằng đó không còn
> tồn tại.** ADR-0047 không đặt assert lên hai bản sao của một bất biến — nó làm cho hai bản sao
> không viết khác nhau được: buffer pre-session **chính là** `RX` của engine, một tham số kiểu,
> và `Shards<const PRE>` mặc định 4096 nhận `RX` từ người gọi. Comment tại `lib.rs:2147` nói
> nguyên văn: *"The pre-session buffer IS the engine's `RX`, and that is now a type rather than
> a promise."* **Một `assert!` là cách kiểm một lời hứa; một tham số kiểu là cách không cần hứa.**
> Bước 6 còn lại ba việc: tên cho cái đóng vì frame dài hơn buffer, `Reply::business_reject`, và
> alloc case `reject`.
>
> **3. Bước 4 phải nói với mười hai entry point chứ không sáu, nhưng kết luận không đổi.** Hình
> dạng vẫn đúng hai loại: mười cái nhận `log: L` (và `handles`), hai cái nhận
> `log_path: Option<&Path>`. `Settings::log()` trả `Option<&Path>`, tức vẫn hợp với loại thứ hai
> và vẫn để người gọi tự dựng `FileLog` cho loại thứ nhất. Ba câu hỏi của *Phát hiện 2* đứng
> nguyên văn.
>
> ### Một điều kiện của ADR-0054 đi ngang qua plan này, và không bị chạm
>
> ADR-0054 hoãn `Serve` builder kèm điều kiện mở lại: **"lần đầu tiên có người muốn tham số thứ
> mười một"**. Bốn entry point đã ở 8 tham số với một `#[allow(clippy::too_many_arguments)]`.
> **Plan này không thêm tham số nào cho entry point** — mọi key mới đi vào *file*, và `Settings`
> trả về `Config` / `Limits` / `Policy` mà các chữ ký đó đã nhận sẵn. Ghi ở đây để người sau
> không phải tự suy ra rằng điều kiện ấy đã hay chưa được kích hoạt: **chưa**.

## Hai phát hiện mới, và chúng đổi việc phải làm

### Phát hiện 1 — `MaxMessageSize` không phải config key ở engine nào, và `RX` là câu trả lời

Draft ghi `MaxMessageSize` là "phải đọc `frame.rs` trước". Đã đọc, **và đã khảo sát ba engine
khác**. Kết quả lật ngược một dòng của chính repo này.

**`docs/reference/prior-art.md:143` sai.** Nó ghi `MaxMessageSize` là "per-session knob" của
QuickFIX. `vendor/quickfix-src/src/C++/SessionSettings.h` có **113 config key** và không có key
này. Nó nằm ở `FixFieldNumbers.h:61` — `const int MaxMessageSize = 383;` — tức là **tag 383, một
field tuỳ chọn trong Logon** (`spec/FIX44.xml:284`). Hai bên *nói cho nhau biết* giới hạn của
mình. Chuyện giao thức, không phải chuyện file. QuickFIX/J cũng không có setting này.

| Engine | Buffer đọc | Trần | Đặt ở đâu | Quá dài thì sao |
|---|---|---|---|---|
| QuickFIX C++ | `std::string`, append, mọc trên heap | **không có** | — | chờ mãi, buffer mọc mãi — một DoS surface |
| QuickFIX/J | không có setting giới hạn message | **không có** | — | — |
| **Artio** | `ByteBuffer` cố định, **16 KiB** | 16 KiB | `receiverBufferSize(int)` lúc dựng engine | ghi lại + **disconnect**, *có nêu lý do* |
| **fixbolt** | `[u8; RX]` cố định, **4 KiB** | 4 KiB | tham số kiểu, compile time | `Cut::Garbage` → session quyết; pre-session đóng **im lặng** |

QuickFIX C++ `Parser::readFixMessage`: `int length`, chỉ kiểm `< 0`, không có trần trên;
`9=2000000000` làm `m_buffer` mọc trên mỗi lần đọc. **Đây là chỗ fixbolt tốt hơn, không phải chỗ
nó thiếu.**

Artio giống fixbolt nhất và khác đúng hai chỗ: (1) Artio đợi buffer đầy thật
(`offset == 0 && byteBuffer.remaining() == 0`), fixbolt biết từ `9=` — **fixbolt nhanh hơn và
đúng hơn**; (2) Artio *nêu lý do* — `"Unable to frame message, receiver buffer too small"` —
còn fixbolt trả `Step::Gone`, đóng socket không tên, không sự kiện.

**Kết luận: không có key `MaxMessageSize`.** Không engine nào có; Artio đặt trần ở chỗ dựng
engine, đúng chỗ fixbolt đặt nó. Một key trong file sẽ là thứ fixbolt tự nghĩ ra.

**Nhưng hai việc thật, cả hai tốt hơn một config key:**

1. **Tag 383 là đường giao thức đã có sẵn.** fixbolt gửi `383=<RX>` trong Logon và đọc `383=`
   của đối tác. Việc của `session`, nhỏ, **một mục riêng** — không thuộc plan này.
2. **Đóng vì frame quá dài phải có tên.** `conn.rs:348` đã có comment *"Named, not merely
   closed"* cho `DuplicateIdentity`; lý do đó áp dụng y hệt ở đây và chưa được áp dụng. **Thuộc
   plan này, bước 6.**

### `RX = 4096` — đo được gì từ máy này, và cái gì phải đợi máy §9

`[đo 2026-09-04, macOS, `size_of` là dữ kiện compile-time nên đúng ở mọi máy]`

```
Connection RX=4096  : 23 752 bytes
Connection RX=16384 : 36 040 bytes      (+12 288, đúng bằng buffer, không padding phát sinh)
```

**Nhưng con số đó không phải chi phí thật của một connection.** `Store = MemJournal<4096, 512>`
là `Box<[Slot<512>]>`, `4096 × (512+8) ≈ 2 MiB mỗi session` (`journal.rs:45-48`):

| | inline | heap (journal) | tổng |
|---|---|---|---|
| RX=4096 | 23 752 | 2 129 920 | **2,05 MiB** |
| RX=16384 | 36 040 | 2 129 920 | **2,07 MiB** |

**Nâng RX gấp 4 là +0,57% bộ nhớ mỗi connection.** Bộ nhớ không phải lý do để từ chối.

**Và RX to hơn không mua tốc độ** — đã tìm cơ chế, không có: `cut()` và `take()` chạy trên
`self.len` chứ không trên `N` (`frame.rs:133-180`), và vòng quét chạm đúng một cache line mỗi
connection ở cả hai kích thước, nên TLB reach không đổi. RX to hơn mua **năng lực**, không mua ns.

**Ba rủi ro thật, không cái nào là latency:**

1. **`PRE` phải đi theo `RX`, và chỉ có comment giữ điều đó.** `const PRE: usize = 4096;` viết
   cứng ở **hai chỗ** — `lib.rs:1635` và `shard.rs:471` — mỗi chỗ kèm comment *"Matches the
   engine's RX"*. Không gì kiểm. **Đây đúng là "prose does not hold a constraint"**, và
   `const _: () = assert!(PRE == RX);` giải quyết xong. **Bước 6, và nó không phụ thuộc việc có
   nâng RX hay không.**
2. **`Connection::new` dựng trên stack rồi mới `push`** (`lib.rs:317-322`, và 373, 505). Release
   thường elide, không bảo đảm. `Engine::add` vốn đã cấp phát ~2 MiB trên engine thread.
3. **Trần đi lên một chiều.** `SLOT_LEN = 512` là message dài nhất ring resend giữ được, khoá
   với `TX` qua `resend_batch × SLOT_LEN < TX` (ADR-0046). Nhận được 16 KiB nhưng resend chỉ
   512 byte — không mất im lặng (`puts_refused`, `EventKind::JournalRefused`) nhưng bất đối xứng.
   **Bốn hằng đi cùng nhau, không phải một.**

**Quyết định: không nâng RX trong plan này**, và lý do **không** phải bộ nhớ — chủ dự án đã nói
rõ RAM không phải vấn đề trên PROD. Lý do là `CLAUDE.md` §2 điều 10: đổi một hằng hot-path phải
đo trên máy §9, và phép đo là `benches/turn.rs` ở hai giá trị RX. **Thuộc đợt C.**

### Phát hiện 2 — sáu entry point, và cái thứ sáu nhận log bằng một kiểu khác

Draft (và `STATUS.md`) nói **năm** entry point nhận message log. Đếm lại: **sáu**.

| Entry point | Tham số log |
|---|---|
| `serve` (`lib.rs:1234`) | `log: L` where `L: MessageLog` |
| `connect_and_serve` (`lib.rs:1298`) | `log: L` |
| `serve_with_recovery` (`lib.rs:1440`) | `log: L` |
| `serve_hft` (`lib.rs:1477`) | `log: L` |
| `serve_hft_with_recovery` (`lib.rs:1509`) | `log: L` |
| **`serve_sharded_hft` (`shard.rs:440`)** | **`log_path: Option<&Path>`** |

Cái thứ sáu bất đối xứng có lý do — mỗi shard là một thread và phải mở file của riêng nó, nên
nó nhận *đường dẫn* rồi tự dựng log, chứ không nhận một `L` đã dựng sẵn. **Nhưng nó là chỗ
`ConnectionType` sẽ va vào**: `Settings` phải nói được với cả hai hình dạng. `Settings::log()`
hôm nay trả `Option<&Path>`, tức là nó **đã** hợp với cái thứ sáu và **chưa** hợp với năm cái
kia — người gọi tự dựng `FileLog` từ path.

Hệ quả cho plan: bước 4 không chỉ là "`into_initiator()`". Nó phải trả lời **ba câu**, và câu
thứ ba là câu draft không biết là có:

1. `ConnectionType=acceptor` + không shard → `into_table()` + `log()`, như hôm nay.
2. `ConnectionType=initiator` → `into_initiator()` → `(Config, addr, Policy)`.
3. `ConnectionType=acceptor` + `ShardPlan` → `into_table()` + `log()`, và **`serve_sharded_hft`
   là entry point duy nhất không nhận `Recovery`** — một file khai initiator-với-shard phải là
   lỗi có số dòng, không phải một panic ở runtime.

## Bối cảnh

Người dùng đầu tiên của fixbolt gần như chắc chắn đã dùng QuickFIX. Họ mở file cấu hình và tìm
mười thứ; hôm nay tìm thấy **mười một key, tất cả cho acceptor**. Không có cách nào khai một
initiator từ file (host, port, reconnect), không có `ResetOnLogon`, không có `LogonTimeout`, và
không có chỗ nào để nói "đối tác này gửi field ngoài dictionary, cho qua". Mỗi thứ đó hôm nay là
một dòng Rust, và ADR-0040 đã quyết định file là cách người vận hành nói với engine.

Kèm theo hai việc nhỏ cùng vùng: registry chỉ nhìn thấy `Identity` (49/56/50/57), không thấy
`553=`/`554=`/`96=` nên không kiểm được credential dù ADR-0026 nói `lookup` là auth hook; và
`library` không có cách nói `35=j` BusinessMessageReject mà không tự tay xếp field.

## Những gì đã biết chắc (xác minh lại **2026-09-05**, số dòng của `3000334`)

| Sự thật | Nguồn |
|---|---|
| **11 key**, `[DEFAULT]`/`[SESSION]`, key lạ là lỗi có số dòng, không `[SESSION]` là lỗi. `FileLogPath` là `[DEFAULT]` **only** | `settings.rs:78–111`, ADR-0040 |
| `Settings` công khai: `load`, `parse`, `log() -> Option<&Path>`, `configs() -> &[Config]`, `into_table() -> Table` | `settings.rs:366, 379, 480, 485, 491` |
| `Config` **bảy trường**: `begin_string`, `sender_comp_id`, `target_comp_id`, `max_skew_ms`, `heart_bt_int`, `schedule`, `resend_batch` — **không có** flag reset nào | `session/src/lib.rs:270–288` |
| `connect_and_serve(addr, cfg, app, policy, recovery, log, handles)` — **bảy tham số**; `Policy { first_ms, ceiling_ms, schedule, attempt, not_before_ms, stopped }`, **không jitter** | `engine/src/lib.rs:1644`, `reconnect.rs:46–76`, ADR-0043, ADR-0054 |
| **Mười hai entry point**: mười trong `lib.rs` nhận `log: L` + `handles`, hai trong `shard.rs` nhận `log_path: Option<&Path>` và **không** `handles` | *Phát hiện 2*, sửa bởi *Sửa 2* |
| `Session::new` reset về 1 mỗi lần `connect`; `Session::resume` giữ số; `141=Y` inbound reset cả hai chiều **trước khi** judge chính nó | ADR-0010, `session/src/lib.rs:809, 868, 1241, 2325` |
| **Không có `ResetOn*` ở bất cứ đâu trong `crates/`** — `grep -rni "reset_on\|ResetOnLogon\|ResetPolicy"` trả về rỗng | đã chạy 2026-09-04 |
| `Registry::lookup(Identity<'_>) -> Option<&Entry>`; `impl Registry for &R`; `Table` rỗng từ chối tất cả | `presession.rs:213–228`, ADR-0026 |
| `identity_of` đọc `49/56` bắt buộc, `50/57` tuỳ chọn, bằng quét byte, không parse. **Không đọc `35`** | `presession.rs:122–130` |
| `Limits::new(pending, logon_ms)` — đã có `LogonTimeout` cho **acceptor**, chưa có cho initiator; `Shutdown` có deadline của caller, chưa có `LogoutTimeout` theo phiên | `presession.rs:372, 420`, ADR-0020, ADR-0038 |
| `Validation` của codec chỉ có `body_length`, `check_sum`; dictionary pass chạy trong session mỗi message, **chưa có knob và chưa được đo** (item 39) | `codec/src/parse.rs:86–110`, `STATUS.md` item 39 |
| Frame dài hơn `RX` → `Cut::Garbage`, giao session một lần, **không im lặng**; nhưng ở **pre-session** thì `Cut::Garbage` → `Step::Gone`, đóng socket **không tên** | `engine/src/frame.rs:38–47`, `presession.rs:690`, `lib.rs:1439` |
| `Reply`: `message(msg_type)` → `field`, `group`, `send`, `send_with_groups`; `library, reply only` **804,1 ns** trên máy §9, tỉ lệ **3,4×** so với `encode ExecutionReport (template)` | `library/src/reply.rs:212–286`, `benches/baselines.tsv:196`, ADR-0051 |
| **39** test settings hiện có phải xanh nguyên | `engine/tests/settings.rs` (**34**), `settings_wire.rs` (5) |
| QuickFIX đặt tên: `ConnectionType`, `SocketConnectHost/Port`, `ReconnectInterval`, `ResetOnLogon/Logout/Disconnect`, `LogonTimeout`, `LogoutTimeout`, `ValidateFieldsOutOfOrder`, `ValidateUserDefinedFields`, `MaxMessageSize` | `docs/reference/prior-art.md:141–143` |

## Cách làm

**Key mới trong file** (tên của QuickFIX khi nghĩa giống hệt; khác nghĩa thì đặt tên khác và nói
rõ trong `CONFIGURATION.md`):

| Key | Vào đâu | Ghi chú |
|---|---|---|
| `ConnectionType=acceptor\|initiator` | `Settings` → chọn entry point | mặc định `acceptor`; một file **không** trộn hai vai |
| `SocketConnectHost`, `SocketConnectPort` | `connect_and_serve` | bắt buộc khi initiator; lỗi có số dòng nếu có mà là acceptor |
| `ReconnectInterval` (giây), `ReconnectCeiling` (giây, **không có ở QuickFIX**) | `Policy::new(first_ms, ceiling_ms)` | ceiling mặc định = 16 × first; parse **nhân 1000** |
| `ResetOnLogon`, `ResetOnLogout`, `ResetOnDisconnect` = `Y\|N` | `Config` trường thứ **tám**: `reset: ResetPolicy` | **không dùng `Session::new`/`resume` để biểu diễn** — đó là chuyện journal có gì; đây là chuyện session *muốn* gì |
| `LogonTimeout` (giây) | initiator: từ `connect` tới `Logon` về; acceptor: **ghi đè** `Limits.logon_ms` | `Session` thuần: deadline đo bằng `tick` |
| `LogoutTimeout` (giây) | `begin_logout` → không có `Logout` về → `disconnect_with(DropReason::LogoutTimedOut)` | `DropReason` thêm biến thể, ADR-0035 kiểu không trường |
| `AllowUnknownMsgFields=Y\|N`, `ValidateUserDefinedFields=Y\|N` | `Config::validation: DictionaryChecks` | `ValidateFieldsOutOfOrder` **không hỗ trợ**: index phẳng không có khái niệm thứ tự header/body (D2) — nêu rõ *vì sao* trong `CONFIGURATION.md` |
| ~~`MaxMessageSize`~~ | — | **bỏ khỏi phạm vi, thay bằng một ADR** — *Phát hiện 1* |
| `FileLogPath` | đã có | — |

**Credential hook:** giữ `lookup` và thêm `fn admit(&self, id: Identity<'_>, logon: &[u8]) ->
Option<&Entry>` với default gọi `lookup` — **default method, không đổi chữ ký cũ**, nên 40 test
settings và mọi `impl Registry` hiện có không phải sửa một dòng. Ràng buộc: `identity_of` vẫn quét
byte; `Table` mặc định **bỏ qua** credential (không có mặc định nào là "chấp nhận mật khẩu rỗng");
engine **không** lưu mật khẩu; một `impl` tự viết là cách duy nhất kiểm 553/554.

**`Reply::business_reject(ref_seq, ref_msg_type, reason, text)`** trong `library`: viết `35=j` với
`45=`, `372=`, `380=`, `58=` qua cùng `TemplateBuilder`; không đụng session.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| 2 — session thuần | `ResetPolicy`, `LogoutTimeout` là trạng thái + so sánh với `tick` | không clock, không alloc, không `format!`; `benches/alloc.rs` đường `logon_out`, `clock` giữ 0 |
| 3 — 59 định nghĩa | rule reset mới có thể đổi cách trả `141=` | **59 / 59**, và `SessionReset.def` đọc lại bằng tay; mirror giữ ≥ 10 / 50 |
| 1 — không cấp phát | `Settings` parse ở startup (được phép); `Reply::business_reject` trên đường trả lời | thêm case `reject` vào alloc bench của `library` |
| 5 — thứ tự field từ bảng | `35=j` | qua `TemplateBuilder::build::<Fix44>()` như mọi reply |
| 6 — feature gate | `connect_and_serve` sau `standard` như hôm nay | `#[cfg]` trên item; `cargo test --no-default-features` |
| 7 — không `unwrap`/`expect`/`panic` | mọi API mới | clippy workspace, `-D warnings` |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 0 | **ADR nói ba điều** (*Phát hiện 1*): không có key `MaxMessageSize` — không engine nào có; tag 383 là đường giao thức, một mục riêng; `RX = 4096` chưa kiểm chứng, kèm bộ số 23 752 / 36 040 / +0,57%. ~~Sửa `prior-art.md:143`~~ — **đã xong 2026-09-04**, *Sửa 2*. Không code | — |
| 1 | `ResetPolicy` trong `session` + ba rule; test đỏ trước trong `crates/session/tests/logon.rs`; **59/59** | — |
| 2 | `LogonTimeout` (initiator) và `LogoutTimeout`; `DropReason::LogoutTimedOut`; test bằng `tick`, **mỗi test có deadline riêng** | 1 |
| 3 | `DictionaryChecks` trong `Config`; hai knob; test với một tag 5000+ và một tag không có trong FIX44 | 1 |
| 4 | `Settings`: `ConnectionType`, host/port, reconnect, ba reset, hai timeout, hai knob. `into_initiator() -> (Config, SocketAddr, Policy)`. **Ba câu hỏi của *Phát hiện 2* đều có test**, kể cả initiator-với-shard là lỗi có số dòng. **39 test hiện có xanh nguyên** | 2, 3 |
| 5 | `Registry::admit` default method; `Table` không đổi hành vi; một test với registry tự viết từ chối `554=` sai | 4 |
| 6 | ~~`const _: () = assert!(PRE == RX);`~~ — **ADR-0047 đã làm, bằng kiểu chứ không bằng assert**, *Sửa 2*. Còn lại: `DropReason` có tên cho frame dài hơn buffer, + sự kiện ở pre-session (hôm nay `Step::Gone` im lặng); `Reply::business_reject`; alloc case `reject` đọc 0 | — |
| 7 | Docs: `CONFIGURATION.md` (mọi key mới), `GUIDE.md` §1a0/§8c, `SESSION-BEHAVIOUR.md` §1/§4 (**nêu test**), `CHANGELOG.md`, `STATUS.md`, ADR bước 0 | 1–6 |

## Cách kiểm chứng

- `cargo test -p fixbolt-session --test score --test mirror --test logon`
- `cargo test -p fixbolt-engine --test settings --test settings_wire --test reconnect --test reconnect_wire --test shard --test shard_wire`
- `cargo test --all`, `cargo test --all --no-default-features`, `benches/alloc.rs`
- `scripts/interop.sh` **cả hai chiều**, một lần với `ResetOnLogon=Y` ở cả hai đầu và một lần với
  `N` — **chiều nào không đổi kết quả thì knob đó chưa được kiểm**
- `scripts/bench.sh` invariant (không phải `--strict` — cổng đó đang đỏ trên máy §9, item 41,
  và plan này không sửa nó)

**Reversal tối thiểu, mỗi cái phải đỏ trước:**

| Đảo | Phải thấy |
|---|---|
| bỏ `ResetOnLogout` khỏi `end` | test reset đỏ |
| `LogoutTimeout` không đếm | test **đỏ vì assertion**, không phải treo — bài học [a-reversal-can-fail-by-hanging](../reference/a-reversal-can-fail-by-hanging.md); mỗi test có deadline riêng |
| `admit` default gọi `lookup` bị đổi thành `None` | test credential đỏ, và **40 test settings vẫn xanh** — nếu chúng cũng đỏ thì default method đã không phải default |
| `ConnectionType=initiator` đưa vào `into_table()` | lỗi có số dòng, không phải panic |

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| `ResetOnLogon=Y` ở acceptor + `Session::resume` từ journal: số nào thắng? | resume ở 500, đối tác `141=Y` → cả hai về 1, journal `highest` **không đổi** (không xoá) |
| `ReconnectInterval` giây ở QuickFIX, ms trong `Policy` | `ReconnectInterval=2` → `first_ms == 2000` |
| Một file khai initiator nhưng gọi `serve` | `into_table()` từ chối, thông báo chỉ sang `into_initiator()` |
| **Một file khai initiator + `ShardPlan`** (*Phát hiện 2*) | lỗi có số dòng; `serve_sharded_hft` không nhận `Recovery` nên không có đường im lặng |
| `AllowUnknownMsgFields=Y` làm corpus `14a_BadField.def` đổi kết quả | knob mặc định `N`; corpus chạy với mặc định; test riêng bật `Y` |
| Thêm trường thứ tám vào `Config` làm `Config` hết `Copy` | `Config` đang được truyền by-value khắp nơi (`serve` dòng 1242, `dial`); `ResetPolicy` phải là kiểu `Copy` |
| Đặt `MaxMessageSize` thành key rồi để nó không làm gì | bước 0 là ADR, và bảng key ghi rõ **không có key này** |

## Ngoài phạm vi

`RefreshOnLogon`, `SendRedundantResendRequests`, store DB, `DefaultApplVerID` (phase 2), nhiều vai
trong một file, `MaxMessageSize` như một key (bước 0 giải thích), item 41 (`bench.sh --strict` đỏ
trên máy §9), item 39, item 34, item 46.

**Và bốn hằng, với lý do — không phải vì đắt, mà vì chưa đo.** `[chủ dự án nói 2026-09-04]` trên
PROD server RAM và disk **không phải vấn đề**, và khi phải đánh đổi thì chọn performance. Nguyên
tắc đó **mở ra** bốn hằng dưới đây chứ không đóng chúng — cái đóng chúng là §2 điều 10, vì cả
bốn đều nằm trên hot path và máy này không phải máy §9:

| Hằng | Hôm nay | Cái nó mua nếu lớn hơn | Cái giá |
|---|---|---|---|
| `SLOTS` | 4096 (~2 MiB/session) | resend xa hơn; mỗi message rơi ra ngoài ring là một gap fill, tức **dữ liệu mất thật** (`EventKind::ResendBeyondJournal`) | tuyến tính, 8 MiB/session ở 16 384 slot |
| `SLOT_LEN` | 512 | reply dài hơn 512 byte hiện **không bao giờ resend được** (`puts_refused`) | kéo theo `TX` qua `resend_batch × SLOT_LEN < TX` |
| `TX` | 8192 | đi kèm `SLOT_LEN` | — |
| `RingDispatch::DEFAULT_CAPACITY` | 4 MiB | ứng dụng chậm có thêm thời gian trước `Logout(58=slow application)` (ADR-0011) | tuyến tính |

**Đây xứng đáng một ADR riêng: chọn mặc định cho một PROD server hiện đại thay vì cho một
laptop.** Nó đi cùng đợt C, khi đã có máy §9 — cùng chuyến với phép đo `RX`.

## Việc tách ra, không thuộc plan này

**14 trên 23 link `#Lnnn` trong `docs/` trỏ sai dòng.** Tìm ra khi kiểm `CONFIGURATION.md` để
viết plan này. Cả **11 link vào `settings.rs` lệch đúng 4 dòng** — `settings.rs:95` được quảng
cáo là `BeginString`, thật ra là `impl Key {`; `settings.rs:590` trỏ vào một dòng `///` trống.
Nguyên nhân là doc comment của `FileLogPath` đẩy enum xuống, và **không có gì đọc những link
này**, nên chúng hỏng im lặng.

Đây là đúng hình dạng `CLAUDE.md` §4 gọi là *"prose does not hold a constraint"*, và nó **kiểm
được bằng máy**. Nhưng nó không phải việc của plan này: nó là một script, một entry
`docs/reference/`, và một dấu **`[to testing-skills]`**. **Đề xuất: một plan riêng, nhỏ**, sau
khi plan này đóng — nếu gộp vào đây thì bước 7 sẽ vừa sửa link vừa dựng gate cho chính nó.

## Nhật ký giao hàng

| Bước | Ngày | Kết quả |
|---|---|---|
| Xác minh lại draft | 2026-09-04 | **Xong.** Sáu dòng sai đã sửa, bốn dòng kiểm lại đứng nguyên, hai phát hiện mới (`MaxMessageSize` là `RX`; sáu entry point chứ không năm). Trạng thái Draft → **Chờ duyệt** |
| Xác minh lại lần hai | 2026-09-05 | **Xong** (*Sửa 2*). Bảy dòng sai; bước 0 mất nửa sau, bước 6 mất nửa đầu — cả hai vì việc **đã được làm rồi**, một lần trong chính lần xác minh trước và một lần bởi ADR-0047. Mười dòng kiểm lại đứng nguyên. Baseline `cargo test --all` **524, 0 failed** `[đo 2026-09-05]`. Trạng thái → **Đã duyệt** |
| 0–7 | — | Chưa bắt đầu |
