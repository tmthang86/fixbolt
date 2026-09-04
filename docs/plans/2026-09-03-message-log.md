# Nhật ký message hai chiều

> **Loại:** Plan · **Ngày:** 2026-09-03 · **Trạng thái:** Chờ duyệt
> **Phạm vi:** `STATUS.md` item 44. Chạm `engine` (module mới `msglog`, hai điểm móc trong
> `conn.rs`, tham số generic mới có default trên `Engine`, hai entry point `*_with_recovery`,
> `settings`, `observe`), `library` (re-export), `tools/jrnl` (bước 4), docs. **Không chạm**
> `codec`, `dict`, `session`, `transport`.
>
> **Máy chạy:** macOS đủ. `benches/alloc.rs` trên CI là gate; không có số latency mới.
> Bước 4 (CRC cho journal) **tách được** thành plan riêng nếu bước 1–3 đã đủ một PR.
>
> **Thời lượng dự kiến:** 1–2 ngày cho bước 1–3; thêm 1 ngày cho bước 4.
>
> **Làm sau** [resend-from-the-journal](2026-09-03-resend-from-the-journal.md): cả hai chạm
> `journal.rs` và cùng dùng mẫu ring → writer thread; hai branch song song ở đây là conflict chắc chắn.

## Bối cảnh

Câu hỏi vận hành đầu tiên mà mọi FIX desk hỏi khi có tranh chấp: *"10:32:07 chúng tôi nhận được
gì, và đã gửi gì?"* QuickFIX trả lời bằng `messages.log`. fixbolt hôm nay **không trả lời được**:

- Journal chỉ giữ **application message chiều ra**, để resend (D7). Admin message chiều ra không
  có ở đâu.
- Chiều vào chỉ có `mark_in(seq)` — một con số, không có byte (ADR-0017).
- Message bị **từ chối** (sai `56=`, `SendingTime` lệch, garbage) là thứ quan trọng nhất khi cãi
  nhau, và chúng biến mất ngay khi session trả lời.
- `tracing` **không có trong cây** (grep 0 kết quả ở `crates/*/src`), nên cũng không có log kỹ
  thuật nào thay thế.

Kết quả muốn đạt: một file text, một dòng một message, **cả hai chiều, kể cả message bị từ
chối**, ghi bởi một thread không phải engine thread, không cấp phát và không blocking trên hot
path, bật bằng một key trong file cấu hình, và **đếm được** khi nó không ghi kịp.

## Những gì đã biết chắc

| Sự thật | Nguồn |
|---|---|
| Mẫu đã có và đã đo 0 alloc: `ring::pair(cap)` → `Producer::push(&[&[u8], ..]) -> bool` một lần cho cả record; writer thread `write_loop`; `spawn_pinned` nếu có `affinity` | `crates/engine/src/journal.rs:356–372, 440–455`, `ring.rs:117–183`, ADR-0007 |
| Ring `AtomicU8` copy ~1.7 ns/byte trên engine thread; 163 byte ≈ 270 ns | `DESIGN.md` §6 hàng dispatch, ADR-0007 |
| Điểm chiều vào: `Connection::turn` cắt frame bằng `rx.cut()` → `Cut::Message(n)` hoặc `Cut::Garbage(n)`, rồi `refuse(bytes)` (pre-session), rồi session | `crates/engine/src/conn.rs:262–300` |
| Điểm chiều ra: **mọi** byte session emit đi qua `Out::push(bytes)` — admin, replay, application; từ chối khi không vừa `TX` | `conn.rs:575–600` |
| `now_ms` trong `turn` là mili giây **từ năm 0** (D13); `codec::TimestampCache` nhận Unix millis; hằng chuyển đổi `clock::MILLIS_YEAR_ZERO_TO_EPOCH` ở `session` | `DESIGN.md` §4 D13 |
| `Engine<T, R, D, C, W, J, N, RX, TX>`; alias `TcpAcceptorEngine<A, W, J = Store>` có default cho `J` để `shard.rs` không phải đổi | `lib.rs:71, 970` |
| Entry point: `serve`, `serve_hft` (không journal), `serve_with_recovery`, `serve_hft_with_recovery` (generic `J`, `V: Recovery<J>`), `connect_and_serve` | `lib.rs:1031–1290` |
| `Settings`: key lạ là lỗi có số dòng; mười key hiện có | ADR-0040, `docs/CONFIGURATION.md` §1 |
| `Observer`/`Snapshot`: đọc theo yêu cầu, một relaxed load khi không ai xem | ADR-0032 |
| `journal::Reader` cấp phát **cố ý** và nói vậy trong rustdoc, vì không chạy trên engine thread | ADR-0037 |
| Record journal trên đĩa: `seq(4) ‖ len(4) ‖ bytes`; `len == 0` là inbound mark, `seq == 0` là activity mark; tail rách được đếm, `tools/jrnl` exit 2 | D7 *As built*, ADR-0017, ADR-0039 |
| Một DATA field hợp lệ có thể chứa `0x01`, `0x0A`, `0x0D` | `DESIGN.md` §4 D3, `crates/codec/tests/data_encode.rs` |

## Quyết định trung tâm

**Một trait trong `engine`, không phải trong `session`.** Session không biết byte nào bị pre-session
từ chối và không được biết đến file (D1). Điểm móc nằm ở `conn.rs`, nơi đã có byte và có `now_ms`.

**Log là "đã đưa vào buffer gửi", không phải "đã lên dây".** `Out::push` thành công thì ghi;
push bị từ chối thì không ghi, vì `SlowConsumer` đã là sự kiện có tên cho chuyện đó (ADR-0035).
Chiều vào: ghi **trước** `refuse` và trước session — mọi frame đã cắt được, kể cả garbage.

**Text, một dòng một message, SOH giữ nguyên.** Đọc bằng `grep`, không cần tool. Hai byte
`0x0A`/`0x0D` bên trong (chỉ có thể ở DATA field) được writer đổi thành `\n`/`\r` để một dòng
là một message; ghi trong rustdoc và `GUIDE.md`.

**Ring đầy thì bỏ và đếm, không bao giờ chờ** — cùng câu ADR-0011 nói cho ring dispatch, nhưng
ngược chiều: log không được làm rớt session, nên mất log là điều được chấp nhận và **được đếm**.

## Cách làm

**`crates/engine/src/msglog.rs` (mới)**

```rust
pub enum Direction { In, Out }
pub trait MessageLog {
    fn record(&mut self, dir: Direction, at_ms: u64, id: ConnId, bytes: &[u8]);
    fn lost(&self) -> u64 { 0 }
}
pub struct NoLog;            // thân rỗng, gập đi ở InlineDispatch-style
pub struct FileLog { to_writer: Producer, writer: Option<JoinHandle<()>>, lost: Arc<AtomicU64>, .. }
impl FileLog {
    pub fn open(path) -> io::Result<Self>;                     // append, tạo nếu chưa có
    pub fn with_capacity(path, bytes) -> io::Result<Self>;     // mặc định 4 MiB (= ring::DEFAULT_CAPACITY)
    #[cfg(all(feature = "affinity", target_os = "linux"))]
    pub fn open_pinned(path, core) -> Result<Self, AffinityError>;
    pub fn close(self);                                        // join writer, như FileJournal
}
```

Record đẩy vào ring bằng **một** `push(&[&[dir], &at_ms.to_le_bytes(), &id.to_le_bytes(), &len.to_le_bytes(), bytes])`
— không alloc, không format trên engine thread. Writer thread: pop record, format
`YYYYMMDD-HH:MM:SS.sss IN  conn=12 8=FIX.4.4␁9=…` bằng `TimestampCache` sau khi trừ
`MILLIS_YEAR_ZERO_TO_EPOCH`, escape `\n`/`\r`, `write_all`, và `flush` mỗi khi ring rỗng (không
`fsync`; đây là log, không phải journal). Writer **được phép cấp phát**, rustdoc nói rõ như ADR-0037.

**Móc vào engine**

- `Engine` thêm tham số `L: MessageLog = NoLog` (default, để mọi alias và `shard.rs` compile
  nguyên); trường `log: L`; `Engine::with_log(self, log)` hoặc tham số của `new` — chọn tham số
  `new` để không có trạng thái "engine chưa có log" nửa chừng.
- `Connection::turn(now_ms, app, refuse, log: &mut L)`: sau `rx.cut()` →
  `log.record(In, now_ms, self.id, self.rx.bytes(taken))` **trước** `refuse`.
  `Out` giữ `log: &mut L`; trong `push` sau khi copy vào `tx` thành công →
  `log.record(Out, at_ms, id, bytes)`.
- `NoLog` thân rỗng: `#[inline]`, không branch, không trường — cùng cách `Dispatch::OUT_OF_BAND`
  gập đi.
- `serve_with_recovery` và `serve_hft_with_recovery` nhận thêm `log: L`; `serve`/`serve_hft`/
  `connect_and_serve` giữ nguyên chữ ký và truyền `NoLog` — hai entry point "cấp triển khai" là
  nơi có log, đúng như chúng là nơi có journal.
- `observe::Snapshot` thêm `log_lost: u64` (đọc `Arc<AtomicU64>` relaxed khi snapshot được yêu cầu).
- `settings.rs`: key `FileLogPath` (tên của QuickFIX, để người đọc file cũ hiểu ngay), chỉ ở
  `[DEFAULT]`; `Settings::log()` → `Option<PathBuf>`. Hai counterparty không có hai file: một
  engine, một log, `conn=` phân biệt.
- `library`: re-export `MessageLog`, `NoLog`, `FileLog`, `Direction`.

**Bước 4 — CRC32 cho journal (tách được)**

- File header mới: `b"FXBJ\x01"` (5 byte). File **không** có header đọc như v0 (định dạng
  hôm nay), không đổi một byte nào của cách đọc cũ.
- Record v1: `seq(4) ‖ len(4) ‖ bytes ‖ crc(4)`, CRC32 (IEEE, bảng 256 entry tự tính lúc build
  bằng `const fn`, **zero dependency**) trên `seq ‖ len ‖ bytes`. Hai sentinel (`len == 0`,
  `seq == 0`) cũng có CRC.
- `Async`: writer thread tính CRC. `Fsync`: engine thread tính (~100 ns cho 200 byte, trên một
  đường đã trả `sync_data` — nêu trong ADR-0046 *Consequences* hoặc ghi chú ADR-0008).
- `FileJournal::open`: record CRC sai được xử lý **như tail rách**: dừng tại đó, đếm vào
  `corrupt_records`, phần trước vẫn dùng. `tools/jrnl` in cảnh báo và **exit 2** như tail rách.
- Reversal: lật một byte giữa file bằng test → `open` dừng đúng chỗ, `jrnl` exit 2; với file v0
  cùng byte lật → đọc như hôm nay (không phát hiện) — **đó là điều bước này mua**.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **1 — không cấp phát** | hai điểm móc trên hot path | `benches/alloc.rs` engine: case `log-idle` (FileLog gắn, không message) và `log-busy` (một message vào, một ra, **cả hai được ghi trong cửa sổ đếm**, kiểm bằng cách đọc file sau khi `close`) → **0**; `NoLog` không đổi số của 21 case cũ |
| **4 — không ngủ trong kernel (`hft`)** | writer thread là thread khác; engine chỉ `push` | không `write` mới trên engine thread; `check-no-kernel-sleep.sh` không đổi (chạy `w2w` với `FileLog` bật ở đợt C để chứng minh trên Linux) |
| **4 — `standard` phải block** | writer thread không ảnh hưởng | không đổi |
| **2 — session thuần** | không đụng | trait ở `engine` |
| **6 — feature gate** | `open_pinned` sau `affinity` như `FileJournal` | cùng `#[cfg]` |
| **7 — không unwrap** | writer thread: lỗi ghi được đếm, không panic | clippy |
| 5, 8, 9, 10 | không đụng | — |

**Chi phí trên engine thread phải nêu, không giấu:** mỗi message được log là một lần copy
byte-một vào ring, ~1.7 ns/byte — với message 200 byte là ~340 ns, gấp đôi parse. Đó là giá của
ADR-0007 (không `unsafe`), và là số **đợt C** phải đo trên máy §9 rồi ghi vào `DESIGN.md` §8
như một hàng "nếu bật log". Plan này chỉ **ghi** con số ước lượng đó vào `GUIDE.md` với nhãn
`[unproven]`.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | `msglog.rs`: trait, `NoLog`, `FileLog`, writer, escape. Test đỏ trước `crates/engine/tests/msglog.rs`: `::a_record_becomes_one_line_with_direction_time_and_connection`, `::a_data_field_with_a_newline_stays_on_one_line`, `::a_full_ring_drops_and_counts_and_never_blocks` (capacity 256 byte, 100 record trước khi writer chạy — `FileLog::with_capacity` + một `Consumer` giữ lại trong test → `lost() == 100 - k`, và `record` trả về trong < 1 ms mỗi lần) | — |
| 2 | Móc vào `Connection`/`Out`/`Engine<…, L>`; `serve_*_with_recovery`; `Settings` `FileLogPath`; snapshot. Test đỏ trước: `msglog.rs::a_refused_logon_is_in_the_log_even_though_the_session_never_saw_it` (qua socket thật, `56=` sai → `Logout`/drop; log có dòng `IN` với byte gốc và dòng `OUT` với `35=5`), `::everything_the_session_emits_is_logged_including_heartbeats` (tick qua `HeartBtInt`, thấy `OUT … 35=0`), `settings.rs::file_log_path_is_read_and_an_unknown_key_beside_it_is_still_an_error`, `settings_wire.rs`: một counterparty logon với `FileLogPath` đặt → file có ≥ 2 dòng | 1 |
| 3 | `benches/alloc.rs` hai case; `library` re-export; docs bảng dưới; `STATUS.md` item 44; CI xanh, run id | 2 |
| 4 *(tách được)* | Header + CRC32 v1, `open` xử lý corrupt như torn, `jrnl` exit 2. Test đỏ trước `on_disk.rs::a_flipped_byte_stops_the_read_at_the_record_before_it`, `::a_file_without_the_header_reads_exactly_as_before` (fixture v0 sinh bằng code hôm nay, commit như bytes), `journal_reader.rs` tương ứng | 3 |

## Cách kiểm chứng

```
cargo test -p fixbolt-engine --test msglog --test settings --test settings_wire --test on_disk --test journal_reader
cargo test --all && cargo test --all --no-default-features
scripts/check-no-optional-deps.sh
scripts/bench.sh            # log-idle 0, log-busy 0, 21 case cũ không đổi
cargo run -p fixbolt --example acceptor -- <cfg có FileLogPath> 127.0.0.1:9876   # rồi grep file
```

Dòng coi là đạt trong file log (ví dụ):

```
20260903-10:32:07.120 IN  conn=1 8=FIX.4.4␁9=…␁35=A␁49=QFINI␁56=FIXBOLT␁…
20260903-10:32:07.120 OUT conn=1 8=FIX.4.4␁9=…␁35=A␁49=FIXBOLT␁56=QFINI␁…
```

**Reversal bắt buộc, ghi output:**

| Reversal | Phải thấy |
|---|---|
| Móc `In` chuyển xuống **sau** `refuse` | `a_refused_logon_is_in_the_log…` đỏ: thiếu dòng `IN` |
| `record` gọi `write_all` trực tiếp thay vì `push` | `log-busy` alloc vẫn 0 (đây là bẫy: alloc bench **không** thấy syscall) → phải có test thứ hai: `strace`-shape không có trên macOS, nên `a_full_ring_drops_and_counts_and_never_blocks` là cái canh — bản reversal không có ring nên không có `lost`, test đỏ ở `lost() == …`. Ghi rõ là canh gián tiếp; đợt C chạy `check-no-kernel-sleep.sh` với log bật |
| Bỏ escape `\n` | `a_data_field_with_a_newline_stays_on_one_line` đỏ (đếm dòng = 2) |
| Bước 4: bỏ kiểm CRC khi đọc | `a_flipped_byte_stops_the_read…` đỏ: đọc qua record hỏng, `corrupt_records == 0` |

## Tài liệu phải cập nhật

- [ ] `docs/DESIGN.md` §4: mục mới **D14 — the message log is a second file, written by the journal's pattern, and it records refusals** (ngắn, tiếng Anh, nêu chi phí ring copy `[unproven]` cho đến đợt C); §3 hàng `engine` một dòng; §6 hai case alloc; §8 một hàng "if `FileLog` is on" với nhãn *unmeasured*
- [ ] `docs/CONFIGURATION.md` §1: `FileLogPath`; §2: dung lượng ring log
- [ ] `docs/GUIDE.md` §6a mới: bật log, đọc log, `lost` nghĩa là gì, escape, rotation là của bạn (restart)
- [ ] `docs/best-practices-standard.md` / `-hft.md`: một dòng mỗi file, **nêu mode**: `hft` — writer thread cần core riêng không phải engine core (`open_pinned`); `standard` — bật mặc định là hợp lý
- [ ] `docs/SESSION-BEHAVIOUR.md`: không đổi hành vi session — **không sửa**, chỉ thêm một câu ở §6 chỉ sang log để "đọc code" không còn là cách duy nhất
- [ ] `CHANGELOG.md`: `Engine<…, L>`, hai entry point đổi chữ ký, `FileLogPath`, (bước 4) định dạng journal v1
- [ ] `tools/jrnl` rustdoc + `README.md` layout nếu thêm binary (không dự kiến)
- [ ] `STATUS.md`: item 44; *Not proven* đọc lại
- [ ] `docs/decisions/`: bước 1–3 **không cần ADR** — mẫu ring→writer đã có ADR-0007/0008; bước 4 ghi chú `[2026-09-xx]` cuối ADR-0008 (không sửa nội dung Accepted) hoặc ADR mới nếu người duyệt muốn định dạng file là quyết định riêng

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| 1. Log chiều vào đặt sau session → message bị từ chối biến mất, đúng cái cần nhất | `a_refused_logon_is_in_the_log…` |
| 2. `format!` hoặc `String` lọt lên engine thread khi ghép dòng | `log-busy` = 0; format chỉ ở writer |
| 3. Ring đầy → chờ → engine thread treo theo tốc độ đĩa | `a_full_ring_drops_and_counts_and_never_blocks` |
| 4. Một `FileLog` **mỗi connection** → N writer thread, N file | API chỉ nhận log ở `Engine`, không ở `Connection`; `settings` chỉ đọc ở `[DEFAULT]` |
| 5. `\n` trong DATA field cắt một message thành hai dòng | test escape |
| 6. Timestamp là `now_ms` của turn (tick trước read), không phải thời điểm byte tới socket | ghi trong rustdoc và `GUIDE.md`; **không** đọc clock lần hai trên hot path |
| 7. `Engine` thêm generic làm `shard.rs`, `benches`, `tests` không compile | default `L = NoLog` trên struct **và** trên mọi alias; `cargo test --all` là canh |
| 8. Alloc bench đo `NoLog` và tưởng là `FileLog` | `log-busy` dựng `FileLog` thật, `close()` rồi **đọc file** kiểm 2 dòng — case mà không thể fail là case chưa có (bài học `pending-*`) |
| 9. Bước 4: fixture v0 "sinh lại" bằng code mới thì test v0 vô nghĩa | fixture là **bytes commit sẵn**, sinh bởi code trước bước 4, kèm SHA trong comment |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Chi phí ring copy trên `hft` (~340 ns/message ước lượng) làm người dùng `hft` tắt log | trung bình | mặc định `NoLog`; số thật đo ở đợt C; nếu quá đắt, ADR-0007 có sẵn ngã rẽ `unsafe` đã nêu |
| Hai entry point đổi chữ ký | thấp | chưa publish; `CHANGELOG.md` |
| Bước 4 đổi định dạng file trong khi plan resend cũng chạm `journal.rs` | trung bình | plan này làm **sau** plan resend; bước 4 có thể tách |
| Đĩa đầy → writer lỗi → im lặng | thấp | writer đếm lỗi ghi vào cùng `lost`, và dừng thread với một dòng stderr; snapshot thấy `log_lost` tăng |

## Ngoài phạm vi

- Rotation, nén, retention: của người vận hành (`logrotate` + restart, hoặc copytruncate).
- Log kỹ thuật (`tracing`): khác mục đích, khác plan.
- Chọn định dạng nhị phân: không — `grep` là yêu cầu.
- Log ở pre-session **trước** khi frame cắt được (byte rác chưa thành frame): `Framer` chỉ giao khi `Cut`; byte chưa thành frame khi socket đóng là mất — ghi vào `GUIDE.md`, không xử lý.
- Ghi log từ `tools/w2w`: đợt C.

## Nhật ký giao hàng

*(trống — chưa bắt đầu)*
