# Nhật ký message hai chiều

> **Loại:** Plan · **Ngày:** 2026-09-03 · **Trạng thái:** **Đã duyệt** `[2026-09-04]`
> — bước 0 đã đóng cùng ngày, kết luận **giữ hai đường ghi**, không đổi thiết kế:
> [why-the-message-log-is-not-the-journal](../reference/why-the-message-log-is-not-the-journal.md).
> **Phạm vi:** `STATUS.md` item 44. Chạm `engine` (module mới `msglog`, hai điểm móc trong
> `conn.rs`, tham số generic mới trên `Engine`, **mọi** entry point, `shard.rs`, `settings`,
> `observe`, `journal.rs` ở bước 4), `library` (re-export), `tools/jrnl` (bước 4), docs.
> **Không chạm** `codec`, `dict`, `session`, `transport`.
>
> **Máy chạy:** macOS đủ. `crates/engine/benches/alloc.rs` trên CI là gate; không có số latency mới.
>
> **Thời lượng dự kiến:** 2–3 ngày cho bước 1–3b; thêm 1 ngày cho bước 4.
>
> **Làm sau** [resend-from-the-journal](2026-09-03-resend-from-the-journal.md): cả hai chạm
> `journal.rs` và cùng dùng mẫu ring → writer thread; hai branch song song ở đây là conflict chắc chắn.

> **Sửa 1 `[2026-09-04]` — sau `/plan-eng-review`.** Bảy chỗ đổi, tất cả do review tìm ra và
> người duyệt chọn. Ghi lại ở đây vì bản gốc đã được đọc:
>
> 1. **Mọi entry point nhận `L`**, không chỉ hai `*_with_recovery`. Bản gốc để `serve`/
>    `serve_hft`/`connect_and_serve` giữ `NoLog` — mà `serve` chính là đường của
>    `GETTING-STARTED.md`, nên `FileLogPath` đặt vào file cấu hình sẽ **không làm gì và không
>    báo gì**. (D1 → B)
> 2. **Đường sharded được làm luôn**: một `FileLog` mỗi shard, `path.<shard>`, và `shard=`
>    trong mỗi dòng. Bản gốc không nhắc `shard.rs`; N engine trên N thread cùng ghi một path,
>    và `conn=1` ở shard 0 không phải `conn=1` ở shard 1. (D2 → B)
> 3. **`OUT` chỉ nghĩa là "đã vào hàng gửi"**, và cái mất khi hàng gửi chết **được đếm**.
>    `conn.rs` vứt mọi thứ còn trong `tx` khi socket chết, nên log cũ khai đã gửi một message
>    chưa bao giờ ra khỏi máy. (D4 → A)
> 4. Buffer của writer phải `>= RX + 21`, và `pop` trả `Some(0)` **được đếm vào `lost`** —
>    `ring.rs` bỏ record dài hơn buffer của consumer, và `lost` bản gốc chỉ đếm phía producer.
> 5. `Out` mang `ConnId`; `impl` block ở `lib.rs:144` phải viết `L` bằng tay — **default type
>    param không áp dụng cho `impl` block**, chỉ cho type alias. Bẫy 7 bản gốc nói sai chỗ này.
> 6. Mất log thành `EventKind`, không chỉ counter — cùng khuôn ADR-0046 vừa dựng hôm qua.
> 7. Bước 4 **ở lại plan này**, và ADR-0008 được sửa kèm ghi chú ngày. Reversal "ghi thẳng
>    thay vì push" đổi sang canh **trực tiếp**, chạy được trên macOS.
>
> **Sửa 2 `[2026-09-04]` — sau ý kiến độc lập (subagent; Codex hết quota tới 11/09).** Bốn chỗ
> nữa, ba cái đã kiểm vào source trước khi tin:
>
> 8. **Log phải sống sót được một cú `kill -9`.** `FileJournal` đọc lại file lúc `open` và đếm
>    đuôi rách (`journal.rs:396–424`, `torn_tail_bytes()`); `FileLog` bản gốc chỉ "append".
>    Một process chết giữa `write_all` để lại dòng cụt, dòng sau nối thẳng vào, hai message
>    thành một dòng méo — vĩnh viễn, không counter nào biết. Thêm: `close(&mut self)` chứ không
>    `close(self)`, **và một `impl Drop` gọi nó**. `FileJournal` làm đúng như vậy —
>    `close(&mut self)` ở `journal.rs:486`, `impl … Drop for FileJournal` ở `journal.rs:501` —
>    và `&mut self` chính là để `Drop` gọi được. `close(self)` nhận theo giá trị thì `Drop`
>    không gọi được nếu không có `Option`/`ManuallyDrop`. Bản gốc ghi "như `FileJournal`" nhưng
>    chữ ký lại khác nó. (E1 → A)
> 9. **`peer=` trên mọi dòng.** `ConnId` là số đếm trong process, reset mỗi lần khởi động lại,
>    không map sang `CompID` ở đâu; với frame rác hoặc bị từ chối trước `Logon` — đúng ca đầu
>    bài — trong bytes có thể không có `49=`/`56=`. (E2 → B; xem cách làm để biết vì sao nó
>    **không** tốn thêm byte nào trên engine thread)
> 10. **Ba im lặng nhỏ**: mọi dòng `OUT` trong cùng một `turn` mang cùng một mili giây (`Out`
>    không có trường thời gian, cả batch resend lẫn mọi reply dùng chung `now_ms`); escape
>    không thoát `\` nên round-trip nhập nhằng; `FileLog::open` lỗi không có đường đi vì
>    `ServeError` chỉ có hai biến thể. (E4 → A)
> 11. **Bước 0 mới, và nó chặn mọi thứ**: outside voice hỏi vì sao hai đường ghi song song
>    thay vì mở rộng `journal.rs` — thứ vừa được làm cứng tuần này. Người duyệt chọn **điều tra
>    trước khi quyết** (E3 → B). Không viết code cho tới khi bước 0 đóng.

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
| `Consumer::pop` **bỏ** record dài hơn `out` và trả `Some(0)` để phân biệt với hàng rỗng — queue vẫn tiến | `crates/engine/src/ring.rs:178–183` |
| Khi socket chết, `tx` còn bao nhiêu byte cũng bị vứt: `if self.dead { disconnect; return Turn::Gone }` | `crates/engine/src/conn.rs:339–345` |
| `Out` được dựng ở **ba** chỗ trong `conn.rs` (tick, vòng đọc, `slow_application`) và **không có `ConnId`** | `conn.rs:245, 300, 365` |
| `Engine` có **một** `impl` block lớn, ở `lib.rs:144`. Default type param **không** áp dụng cho `impl` block — chỉ cho type alias như `TcpAcceptorEngine` | `crates/engine/src/lib.rs:71, 144, 970` |
| `serve_sharded_hft` dựng N engine trên N thread; mỗi engine đánh `ConnId` riêng từ 0 | `crates/engine/src/shard.rs:438` |
| `benches/alloc.rs` của `engine` in **21** case và assert cả 21 bằng 0 | `crates/engine/benches/alloc.rs:976–1000` |

## Quyết định trung tâm

**Một trait trong `engine`, không phải trong `session`.** Session không biết byte nào bị pre-session
từ chối và không được biết đến file (D1). Điểm móc nằm ở `conn.rs`, nơi đã có byte và có `now_ms`.

**Hai đường ghi, không phải một — và đây là lý do, đã viết ra và đã bị phản biện.**
`[đóng bước 0, 2026-09-04]` Journal không giữ được thứ log tồn tại để giữ, vì **khoá của nó là
`seq`**: cả tám phương thức của trait `Journal` (`crates/session/src/journal.rs`) nhận hoặc trả
`seq: u32`, và `MemJournal` địa chỉ hoá `slots[(seq as usize) % N]` (`journal.rs:149`). Ba thứ
log phải giữ — frame vào chưa được phán, frame rác, frame bị từ chối trước session — **không có
`seq` nào cả**. Định dạng trên đĩa cũng đã tiêu hết hai giá trị khoá dự phòng vào sentinel
(`len == 0` inbound mark `:288`, `seq == 0` activity mark `:298`). Thêm nữa, `Journal` là trait
của **session**, mà bytes bị từ chối theo định nghĩa là bytes session không thấy — gộp là bắt
session biết thứ D1 cấm nó biết. Và `Durability::Fsync` chặn engine thread có chủ đích, còn log
thì không bao giờ được `fsync`: một file không phục vụ được hai chính sách durability mà không
rẽ nhánh theo loại record ngay trên hot path. Chi tiết và số đo:
[why-the-message-log-is-not-the-journal](../reference/why-the-message-log-is-not-the-journal.md).

**Log là "đã đưa vào buffer gửi", không phải "đã lên dây" — và khoảng cách giữa hai điều đó
được đếm.** `Out::push` thành công thì ghi; push bị từ chối thì không ghi, vì `SlowConsumer` đã
là sự kiện có tên cho chuyện đó (ADR-0035). Chiều vào: ghi **trước** `refuse` và trước session —
mọi frame đã cắt được, kể cả garbage.

`[sửa 2026-09-04]` `SlowConsumer` che *từ chối nhận vào buffer*, **không** che *chết sau khi đã
nhận*: `conn.rs:339–345` vứt mọi thứ còn trong `tx` khi socket chết, và log đã ghi `OUT` cho
những byte đó rồi. Với một file người ta mở ra để cãi nhau, sai theo hướng "khai đã gửi cái
chưa gửi" là hướng bất lợi nhất. Nên khi `Turn::Gone` vì `dead`, số byte bị vứt thành
`EventKind::MessageLogUnsent { bytes }` — không sửa được dòng đã ghi, nhưng người đọc log biết
có một đuôi không tin được và biết nó dài bao nhiêu.

**Text, một dòng một message, SOH giữ nguyên.** Đọc bằng `grep`, không cần tool. Hai byte
`0x0A`/`0x0D` bên trong (chỉ có thể ở DATA field) được writer đổi thành `\n`/`\r` để một dòng
là một message; ghi trong rustdoc và `GUIDE.md`.

**Ring đầy thì bỏ và đếm, không bao giờ chờ** — cùng câu ADR-0011 nói cho ring dispatch, nhưng
ngược chiều: log không được làm rớt session, nên mất log là điều được chấp nhận và **được đếm**.

## Bước 0 — một đường ghi hay hai? (chặn mọi thứ)

`[thêm 2026-09-04, E3 → B]` Ý kiến độc lập hỏi một câu plan chưa trả lời: `journal.rs` vừa được
mở rộng tuần này (ADR-0046) và sắp được làm cứng thêm ở bước 4. Sao lại dựng **đường thứ hai**
— ring riêng, thread riêng, hằng dung lượng riêng, định dạng escape riêng — thay vì cho
`journal.rs` nhận thêm chiều vào và frame bị từ chối?

**Lập luận giữ hai đường (chưa được viết ra, nên chưa được phản biện):** journal là trạng thái
**một chiều ra của một session**, khoá bằng `seq`, tồn tại để trả `ResendRequest`. Log phải bắt
byte **vào**, byte **rác**, và byte **bị từ chối trước khi có `seq`** — ba thứ journal theo định
nghĩa không thấy và không có khoá để chứa. Gộp lại là làm journal mất khoá của nó.

**Điều tra phải trả lời, bằng văn bản, trước khi viết code:**

1. Một `journal.rs` mở rộng sẽ đánh khoá gì cho một frame rác chưa có `seq`? Nếu câu trả lời là
   "một khoá thứ hai" thì nó đã là hai cấu trúc trong một file, và lợi ích gộp là gì.
2. `FileJournal` đang `Fsync` được (ADR-0008). Log **không** muốn `fsync`. Một đường ghi phục vụ
   hai chính sách durability thì chính sách nào thắng, và ai đọc được điều đó từ code.
3. Chi phí bảo trì thật: hai đường là ~1 module 300 dòng dùng lại `ring.rs` sẵn có; một đường là
   một định dạng file phải phục vụ hai mục đích và một `Reader` phải phân biệt chúng. Đếm ra.
4. Bước 4 (CRC) đặt đúng file chưa — outside voice #4 nói file text mới là file người ta mở ra
   lúc cãi nhau, mà nó lại không được CRC. Nếu bước 0 kết luận gộp, câu hỏi này tự tan.

**Kết quả:** một ghi chú trong `docs/reference/`, và **một ADR nếu kết luận là gộp** — đảo một
quyết định kiến trúc thì §5 đòi ADR. Nếu kết luận là giữ hai đường, lập luận trên vào phần
*Quyết định trung tâm* của plan này và không cần ADR (mẫu ring→writer đã có ADR-0007/0008).

**Không viết dòng code nào trước khi bước 0 được duyệt.** Đây là Rule Zero áp cho chính plan này.

### Đóng `[2026-09-04]` — giữ hai đường, không cần ADR

[why-the-message-log-is-not-the-journal](../reference/why-the-message-log-is-not-the-journal.md).
Bốn câu hỏi trả lời bằng code, không bằng ý định:

1. **Khoá.** Không có khoá nào cho một frame không có `seq`. Trait `Journal` khoá bằng `seq` ở cả
   tám phương thức; `MemJournal` địa chỉ hoá `seq % N`; hai giá trị khoá dự phòng đã bị hai
   sentinel tiêu hết. Gộp là hai cấu trúc trong một file.
2. **Ranh giới.** `Journal` là trait của `session`; frame bị từ chối là frame `conn.rs` chặn
   **trước** `session.received_with`. Gộp thì hoặc session biết thứ D1 cấm, hoặc engine ghi lén
   vào trait mà session sở hữu.
3. **Durability.** `Fsync` chặn engine thread có chủ đích (`journal.rs:555, 599, 628`); log
   không bao giờ được `fsync`. Một file thì phải chọn một, hoặc rẽ nhánh theo loại record ngay
   trên vòng lặp không được phép rẽ nhánh.
4. **Chi phí.** Hai đường: một module dùng lại `ring.rs` (212 dòng, đã chia với `RingDispatch`,
   đã đo 0 alloc). Một đường: hằng số định dạng bị chạm ở **31 chỗ chỉ trong `journal.rs`**, cộng
   `Reader`/`Record`/`Records`/`tools/jrnl` — sáu loại record thay vì ba, và exit code của `jrnl`
   là hợp đồng đã công bố. **Gộp đắt hơn, không rẻ hơn.**

Nửa đúng của outside voice #4 được nhận: hai file có kiểu hỏng khác nhau nên cần hai câu trả lời
khác nhau — journal sợ byte lật (CRC, bước 4), log sợ `kill -9` cắt dòng (vá đuôi rách, bước 1).
**Bước 4 ở nguyên chỗ cũ.**

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
    pub fn open(path) -> io::Result<Self>;                     // append, tạo nếu chưa có, VÁ ĐUÔI RÁCH
    pub fn with_capacity(path, bytes) -> io::Result<Self>;     // mặc định 4 MiB (= ring::DEFAULT_CAPACITY)
    #[cfg(all(feature = "affinity", target_os = "linux"))]
    pub fn open_pinned(path, core) -> Result<Self, AffinityError>;
    pub fn close(&mut self);                                   // join writer
    pub fn torn_tail_bytes(&self) -> usize;                    // đuôi tìm thấy lúc open
}
impl Drop for FileLog { fn drop(&mut self) { self.close(); } }
```

**Đuôi rách được vá lúc `open`, không được bỏ qua** — `[sửa 2026-09-04, E1]`. `FileJournal`
đọc lại file lúc mở và đếm đuôi cụt (`journal.rs:396–424`); `FileLog` phải làm điều tương đương,
rẻ hơn nhiều vì định dạng là text: seek tới cuối, đọc byte cuối, nếu **không** phải `\n` thì ghi
một `\n` rồi một dòng `# torn tail, N bytes, reopened at <ts>`. Không có nó, một process bị
`kill -9` giữa `write_all` để lại dòng cụt và dòng kế nối thẳng vào — hai message thành một dòng
`grep` đọc là một, vĩnh viễn, không ai biết. `torn_tail_bytes()` phơi số đó ra như journal.

**`close` nhận `&mut self`, và có `Drop`.** `[sửa 2026-09-04, E1]` Bản gốc viết `close(self)`
nhận theo giá trị **và** ghi "như `FileJournal`" — hai điều đó mâu thuẫn nhau.
`FileJournal::close` là `&mut self` (`journal.rs:486`) **chính vì** `impl … Drop for FileJournal`
(`journal.rs:501`) gọi nó; một `close(self)` thì `Drop` không gọi được nếu không có
`Option`/`ManuallyDrop`, và khi ấy thứ còn trong ring lúc process kết thúc bình thường **không
bao giờ được ghi và không bao giờ được đếm** — `lost()` chỉ đếm `push` bị từ chối. `FileLog` đi
đúng đường `FileJournal` đã đi: `close(&mut self)` + `Drop`. `FileJournal` **không cần sửa gì**.

Record đẩy vào ring bằng **một** `push(&[&[dir], &at_ms.to_le_bytes(), &id.to_le_bytes(), &len.to_le_bytes(), bytes])`
— không alloc, không format trên engine thread. Writer thread: pop record, format
`YYYYMMDD-HH:MM:SS.sss IN  shard=0 conn=12 8=FIX.4.4␁9=…` bằng `TimestampCache` sau khi trừ
`MILLIS_YEAR_ZERO_TO_EPOCH`, escape `\n`/`\r`, `write_all`, và `flush` mỗi khi ring rỗng (không
`fsync`; đây là log, không phải journal). Writer **được phép cấp phát**, rustdoc nói rõ như ADR-0037.

**`peer=` trên mọi dòng, và nó không tốn một byte nào trên engine thread** —
`[sửa 2026-09-04, E2]`. Yêu cầu là mỗi dòng tự đủ để trả lời "ai", vì `ConnId` reset mỗi lần
khởi động và một frame rác có thể không có `49=`/`56=`. Cách **không** làm: đẩy chuỗi địa chỉ
qua ring mỗi record — đó là ~22 byte × 1.7 ns mỗi message, trên đúng cái thread đang đếm từng
ns. Cách làm: khi một connection được nhận, đẩy **một** record `Direction::Open` mang
`(conn_id, shard, peer_addr)`; writer giữ một `HashMap<(u16, ConnId), String>` (writer được
phép cấp phát, ADR-0037) và in `peer=` vào mọi dòng sau đó. Một record mỗi connection, không
phải mỗi message. Writer cũng ghi record đó ra file dưới dạng `# conn=1 shard=0 peer=…` để một
file bị cắt giữa chừng vẫn tra được.

**Escape thoát cả dấu `\`** — `[sửa 2026-09-04, E4]`. `0x0A` → `\n`, `0x0D` → `\r`, **và
`0x5C` → `\\`**. Không có luật thứ ba thì một DATA field chứa đúng hai byte `\` `n` không phân
biệt được với một newline đã escape, và dòng log thôi là bản ghi trung thực. Test canh là
**round-trip** (unescape ra đúng bytes gốc), không phải đếm dòng.

**Buffer của writer là `RX + 21` byte, không ít hơn** — `[sửa 2026-09-04]`. `Consumer::pop`
(`ring.rs:178–183`) **bỏ** record dài hơn `out` và trả `Some(0)`; `RX` là 4096 nên một frame có
thể tới 4096 byte, cộng 21 byte header của record. Buffer nhỏ hơn thì message biến mất **trong
im lặng** và `lost()` — vốn chỉ đếm `push` trả `false` — vẫn bằng 0. Nên `Some(0)` cũng cộng vào
`lost`, và buffer được dựng từ `RX` bằng const, không bằng một số viết tay.

**Móc vào engine**

- `Engine` thêm tham số **cuối** `L: MessageLog = NoLog`; trường `log: L`; nhận qua tham số của
  `new` để không có trạng thái "engine chưa có log" nửa chừng.
  **Default chỉ cứu type alias, không cứu `impl`:** `TcpAcceptorEngine`/`HftAcceptorEngine`/
  `TcpInitiatorEngine`/`StandardAcceptorEngine` compile nguyên, nhưng `impl` block duy nhất ở
  `lib.rs:144` phải liệt kê `L` bằng tay. `[sửa 2026-09-04]` bản gốc nói default làm mọi thứ
  compile nguyên — sai với `impl`.
- `Connection::turn(now_ms, app, refuse, log: &mut L)`: sau `rx.cut()` →
  `log.record(In, now_ms, self.id, self.rx.bytes(taken))` **trước** `refuse`.
- `Out` thêm **hai** trường: `log: &'a mut L` và `id: ConnId` — `Out` hôm nay không mang
  `ConnId`, và nó được dựng ở **ba** chỗ (`conn.rs:245` tick, `:300` vòng đọc, `:365`
  `slow_application`), cả ba phải truyền cùng một cặp. Trong `push`, sau khi copy vào `tx`
  thành công → `log.record(Out, at_ms, self.id, bytes)`.
- **Khi socket chết, phần chưa gửi được đếm**: ở nhánh `if self.dead` (`conn.rs:339`), trước
  `return Turn::Gone`, phát `EventKind::MessageLogUnsent { bytes: tx_len }` nếu `tx_len > 0`.
  Nhánh `closed` (peer hang up) đã đợi `tx_len == 0` nên không cần.
- `NoLog` thân rỗng: `#[inline]`, không branch, không trường — cùng cách `Dispatch::OUT_OF_BAND`
  gập đi. `MessageLogUnsent` cũng gập đi cùng nó (`L::LOGS` const false).
- **Mọi entry point nhận `log: L`** `[sửa 2026-09-04]`: `serve`, `serve_hft`,
  `connect_and_serve`, `serve_with_recovery`, `serve_hft_with_recovery`. Bản gốc chỉ cho hai
  cái `*_with_recovery`, nhưng `serve` là đường của `GETTING-STARTED.md` và `library` re-export
  nó (`crates/library/src/lib.rs:27, 38`) — `FileLogPath` đặt vào file cấu hình rồi gọi `serve`
  sẽ không làm gì và không báo gì. Không có đường nào "không mang log được" thì không có chỗ
  nào im lặng được.
- **Sharded**: `serve_sharded_hft` nhận `F: Fn(usize) -> Option<FileLog>` bên cạnh `make_app`,
  hoặc một `PathBuf` gốc và tự mở `path.<shard>` cho từng shard. Mỗi shard **một file, một
  writer thread** — không hai thread nào ghi chung một descriptor. Dòng log mang `shard=<i>`
  bên cạnh `conn=`, vì `ConnId` được đánh lại từ 0 trong mỗi engine (`shard.rs:438`).
- `observe::Snapshot` thêm `log_lost: u64` (đọc `Arc<AtomicU64>` relaxed khi snapshot được yêu
  cầu), và `EventKind` thêm **hai** biến thể — `[sửa 2026-09-04]`, cùng khuôn ADR-0046 vừa dựng:
  một counter trong snapshot là thứ phải đi hỏi, một event là thứ tự nó đến.
  - `MessageLogLost { count: u32 }` — bao nhiêu record bị bỏ ở lượt này, do ring đầy hoặc do
    `pop` trả `Some(0)`.
  - `MessageLogUnsent { bytes: usize }` — bao nhiêu byte đã ghi `OUT` mà không bao giờ lên dây.
- `settings.rs`: key `FileLogPath` (tên của QuickFIX, để người đọc file cũ hiểu ngay), chỉ ở
  `[DEFAULT]`; `Settings::log()` → `Option<PathBuf>`. Hai counterparty không có hai file: một
  engine, một log, `conn=` phân biệt. Đây là key thứ **11** (`settings.rs:95–104` có 10).
- **`ServeError` thêm `LogPath(std::io::Error)`** — `[sửa 2026-09-04, E4]`. `FileLog::open` trả
  `io::Result`, mà `ServeError` hôm nay có **đúng hai** biến thể (`NoCounterparties`, `Io`) và
  `Io` là "bind listener hỏng". Một `FileLogPath` gõ sai thư mục hoặc không có quyền ghi phải
  chết lúc khởi động với tên riêng của nó — non-negotiable 7 không cho `unwrap`, và một biến
  thể dùng chung làm người vận hành đi tìm sai chỗ.
- **Thời gian: mọi dòng `OUT` trong cùng một `turn` mang cùng một mili giây** —
  `[sửa 2026-09-04, E4]`. `Out` không có trường thời gian; cả batch replay từ `tick_with` lẫn
  mọi reply sinh trong vòng đọc đều dùng chung `now_ms` của turn đó. Thứ tự tương đối đọc từ
  **vị trí dòng trong file**, không từ cột thời gian. Ghi vào rustdoc của `MessageLog::record`
  và `GUIDE.md` §6a. **Không** đọc clock lần hai trên hot path để chữa việc này — cái giá không
  đáng, và bẫy 6 đã nói cùng một điều cho chiều vào.
- `library`: re-export `MessageLog`, `NoLog`, `FileLog`, `Direction`.

**Bước 4 — CRC32 cho journal**

`[quyết định 2026-09-04]` **ở lại plan này**, và định dạng file được ghi vào **ADR-0008 kèm ghi
chú ngày sửa** (`[sửa 2026-09-04]`), không phải một ADR mới. Người duyệt chọn vậy sau khi review
nêu rằng CLAUDE.md §5 cấm sửa nội dung một ADR đã `Accepted`: ghi chú có ngày, đặt ở cuối, không
viết lại một câu nào của phần đã chấp nhận — cùng hình dạng ADR-0002 dùng.

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
| **7 — không unwrap** | writer thread: lỗi ghi được đếm, không panic; `FileLog::open` lỗi thành `ServeError::LogPath`, không `expect` | clippy, `a_bad_file_log_path_is_a_named_startup_error` |
| **10 — số nào cũng có bench, máy, §9** | ~340 ns/message/**chiều** là **ước lượng**, không phải đo | nhãn `[unproven]` ở `GUIDE.md` và `DESIGN.md` §8 cho tới đợt C |
| 5, 8, 9 | không đụng | — |

**Chi phí trên engine thread phải nêu, không giấu:** mỗi message được log là một lần copy
byte-một vào ring, ~1.7 ns/byte — với message 200 byte là ~340 ns, gấp đôi parse. Đó là giá của
ADR-0007 (không `unsafe`), và là số **đợt C** phải đo trên máy §9 rồi ghi vào `DESIGN.md` §8
như một hàng "nếu bật log". Plan này chỉ **ghi** con số ước lượng đó vào `GUIDE.md` với nhãn
`[unproven]`.

`[sửa 2026-09-04]` **và nó là mỗi message mỗi chiều.** Một cặp request/reply trả **hai** lần,
~680 ns, chồng lên 270 ns mà `RingDispatch` đã thu của chính message vào đó (`DESIGN.md` §6).
Hàng §8 phải nói *per message per direction*; viết "~340 ns nếu bật log" bên cạnh một con số
round-trip là lạc quan đúng 2×, và đó là loại sai số CLAUDE.md §2 rule 10 tồn tại để chặn.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| **0** | **Điều tra một-đường-hay-hai** (mục *Bước 0* ở trên): ghi chú `docs/reference/` trả lời bốn câu hỏi, và một ADR nếu kết luận là gộp. **Chặn mọi bước dưới.** | — |
| 1 | `msglog.rs`: trait, `NoLog`, `FileLog`, writer, escape (**thoát cả `\`**), vá đuôi rách lúc `open`, `close(&mut self)` + `Drop` (theo đúng mẫu `FileJournal` đã có). Buffer writer `= RX + 21`, `pop → Some(0)` cộng vào `lost`. Test đỏ trước `crates/engine/tests/msglog.rs`: `::a_record_becomes_one_line_with_direction_time_and_connection`, `::a_data_field_with_a_newline_stays_on_one_line`, `::a_full_ring_drops_and_counts_and_never_blocks` (capacity 256 byte, 100 record trước khi writer chạy — `FileLog::with_capacity` + một `Consumer` giữ lại trong test → `lost() == 100 - k`, và `record` trả về trong < 1 ms mỗi lần), **`::record_touches_no_file_until_the_writer_runs`** (canh trực tiếp cho "ghi thẳng", xem bảng reversal), **`::a_record_longer_than_the_writer_buffer_is_counted_not_silently_dropped`**, **`::a_backslash_in_a_data_field_round_trips`** (unescape ra đúng bytes gốc, không chỉ đếm dòng), **`::a_torn_last_line_is_marked_not_merged_with_the_next`** (ghi nửa dòng, `open` lại, dòng mới không dính vào), **`::dropping_a_file_log_without_close_still_writes_what_was_queued`** | 0 |
| 2 | Móc vào `Connection`/`Out`(+`ConnId`, ba chỗ dựng)/`Engine<…, L>`; **mọi** entry point; `Settings` `FileLogPath`; snapshot + `EventKind::MessageLogLost`. Test đỏ trước: `msglog.rs::a_refused_logon_is_in_the_log_even_though_the_session_never_saw_it` (qua socket thật, `56=` sai → `Logout`/drop; log có dòng `IN` với byte gốc và dòng `OUT` với `35=5`), `::everything_the_session_emits_is_logged_including_heartbeats` (tick qua `HeartBtInt`, thấy `OUT … 35=0`), **`::file_log_path_reaches_serve_not_only_serve_with_recovery`** (đường `GETTING-STARTED`, `serve` + `FileLogPath` → file có dòng), `settings.rs::file_log_path_is_read_and_an_unknown_key_beside_it_is_still_an_error`, `settings_wire.rs`: một counterparty logon với `FileLogPath` đặt → file có ≥ 2 dòng, **`::a_bad_file_log_path_is_a_named_startup_error`** (thư mục không tồn tại → `ServeError::LogPath`, không panic, không im lặng), **`::every_line_carries_the_peer_address`** | 1 |
| 3 | **`EventKind::MessageLogUnsent`**: đếm byte còn trong `tx` khi `dead`. Test đỏ trước `msglog.rs::bytes_still_queued_when_the_socket_dies_are_counted_not_claimed_as_sent` — giết socket với bytes đã push, thấy dòng `OUT` **và** event với `bytes > 0` | 2 |
| 3b | **Sharded**: một `FileLog` mỗi shard, `path.<shard>`, `shard=` trong dòng. Test đỏ trước `shard.rs`/`msglog.rs::two_shards_write_two_files_and_conn_ids_do_not_collide` — hai shard, hai connection cùng `ConnId` 0, hai file, `shard=` khác nhau | 2 |
| 3c | `crates/engine/benches/alloc.rs` hai case (`log-idle`, `log-busy`); `library` re-export; docs bảng dưới; `STATUS.md` item 44; CI xanh, run id | 3, 3b |
| 4 | Header + CRC32 v1, `open` xử lý corrupt như torn, `jrnl` exit 2, **ghi chú `[sửa 2026-09-04]` cuối ADR-0008**. Test đỏ trước `on_disk.rs::a_flipped_byte_stops_the_read_at_the_record_before_it`, `::a_file_without_the_header_reads_exactly_as_before` (fixture v0 sinh bằng code hôm nay, commit như bytes), `journal_reader.rs` tương ứng | 3c |

## Cách kiểm chứng

```
cargo test -p fixbolt-engine --test msglog --test settings --test settings_wire --test on_disk --test journal_reader
cargo test --all && cargo test --all --no-default-features
scripts/check-no-optional-deps.sh
scripts/bench.sh            # log-idle 0, log-busy 0, 21 case cũ không đổi
cargo run -p fixbolt --example acceptor -- <cfg có FileLogPath> 127.0.0.1:9876   # rồi grep file
grep -c '^#' <log>          # dòng chú thích: mở connection, đuôi rách vá lúc open
grep -v '^#' <log> | wc -l  # đúng số message hai chiều
```

Dòng coi là đạt trong file log (ví dụ):

```
# conn=1 shard=0 peer=10.4.2.9:51422 opened at 20260903-10:32:07.118
20260903-10:32:07.120 IN  shard=0 conn=1 peer=10.4.2.9:51422 8=FIX.4.4␁9=…␁35=A␁49=QFINI␁…
20260903-10:32:07.120 OUT shard=0 conn=1 peer=10.4.2.9:51422 8=FIX.4.4␁9=…␁35=A␁49=FIXBOLT␁…
```

`shard=` luôn có mặt, cả khi không sharded (`shard=0`) — một định dạng, không hai, để `awk` của
người vận hành không phải đoán. Dòng bắt đầu bằng `#` là chú thích của writer (mở connection,
đuôi rách vá lúc `open`); `grep -v '^#'` cho ra đúng các message.

**Reversal bắt buộc, ghi output:**

| Reversal | Phải thấy |
|---|---|
| Móc `In` chuyển xuống **sau** `refuse` | `a_refused_logon_is_in_the_log…` đỏ: thiếu dòng `IN` |
| `record` gọi `write_all` trực tiếp thay vì `push` | **`record_touches_no_file_until_the_writer_runs` đỏ.** `[sửa 2026-09-04]` bản gốc để đây là canh *gián tiếp* qua `lost` và đẩy bằng chứng thật sang đợt C. Có canh **trực tiếp**, chạy trên macOS: dựng `FileLog` với writer **chưa** chạy (`Consumer` giữ trong test), gọi `record` một lần, rồi `metadata(path).len() == 0`. Bản ghi thẳng có bytes trong file khi không writer nào chạy → đỏ ngay ở assertion đó. Alloc bench **không** thấy syscall và không bao giờ thấy — đó là lý do cần test này, không phải lý do để hoãn |
| `record` bỏ qua `pop → Some(0)` khi cộng `lost` | `a_record_longer_than_the_writer_buffer_is_counted_not_silently_dropped` đỏ: `lost() == 0` trong khi file thiếu dòng |
| Bỏ escape `\n` | `a_data_field_with_a_newline_stays_on_one_line` đỏ (đếm dòng = 2) |
| Bỏ `MessageLogUnsent` ở nhánh `dead` | `bytes_still_queued_when_the_socket_dies_are_counted_not_claimed_as_sent` đỏ: có dòng `OUT`, không có event |
| Hai shard dùng chung một path | `two_shards_write_two_files_and_conn_ids_do_not_collide` đỏ: một file, và hai dòng `conn=0` không phân biệt được |
| Bỏ vá đuôi rách lúc `open` | `a_torn_last_line_is_marked_not_merged_with_the_next` đỏ: dòng mới dính vào dòng cụt, đếm dòng thiếu 1 |
| Bỏ `impl Drop for FileLog` | `dropping_a_file_log_without_close_still_writes_what_was_queued` đỏ: file rỗng |
| Escape không thoát `\` | `a_backslash_in_a_data_field_round_trips` đỏ: unescape ra khác bytes gốc |
| `FileLog::open` lỗi bị nuốt bằng `Option` | `a_bad_file_log_path_is_a_named_startup_error` đỏ: `serve` chạy bình thường với path sai |
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
- [ ] `docs/decisions/`: bước 1–3c **không cần ADR** — mẫu ring→writer đã có ADR-0007/0008; **bước 4 ghi chú `[sửa 2026-09-04]` cuối ADR-0008**, không sửa một câu nào của phần Accepted (quyết định của người duyệt, D3). **Bước 0 cần ADR nếu nó kết luận gộp một đường** — đảo một quyết định kiến trúc thì §5 đòi thế
- [ ] `docs/CONFIGURATION.md` §1 là bảng **11 key** sau thay đổi này, không phải 10; §2 thêm dung lượng ring log và buffer writer (`RX + 21`)
- [ ] `docs/reference/`: ghi chú của **bước 0**, và một mục cho bài học "một artefact pháp lý cần cơ chế phát hiện hỏng của riêng nó" — kèm dấu **`[to testing-skills]`** (§11): shape là *một check đọc file để chứng minh, nhưng chính file đó không có cách nào biết mình bị cắt*

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
| 10. `kill -9` giữa `write_all` → hai message dính thành một dòng, `grep` đọc là một | `a_torn_last_line_is_marked_not_merged_with_the_next` |
| 11. `close(self)` nhận theo giá trị → `Drop` không gọi được → quên `close()` là ring còn gì mất nấy, im lặng | `dropping_a_file_log_without_close_still_writes_what_was_queued` |
| 12. `conn=3` sáu tuần sau không tra được là ai | `every_line_carries_the_peer_address`, và dòng `#` lúc mở connection |
| 13. `\` trong DATA field làm bản ghi hết trung thực | `a_backslash_in_a_data_field_round_trips` — round-trip, **không** đếm dòng |
| 14. Đẩy chuỗi `peer=` qua ring mỗi message → ~22 byte × 1.7 ns trên engine thread | một record `Open` mỗi connection, writer giữ map; `log-busy` bench là canh gián tiếp, `record` count trong test là canh trực tiếp |
| 15. Thời gian: mọi dòng `OUT` một turn cùng mili giây, người đọc tưởng là thứ tự thật | rustdoc + `GUIDE.md` §6a nói rõ thứ tự đọc từ vị trí dòng |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Chi phí ring copy trên `hft` (~340 ns/message ước lượng) làm người dùng `hft` tắt log | trung bình | mặc định `NoLog`; số thật đo ở đợt C; nếu quá đắt, ADR-0007 có sẵn ngã rẽ `unsafe` đã nêu |
| Hai entry point đổi chữ ký | thấp | chưa publish; `CHANGELOG.md` |
| Bước 4 đổi định dạng file trong khi plan resend cũng chạm `journal.rs` | trung bình | plan này làm **sau** plan resend; bước 4 có thể tách |
| Đĩa đầy → writer lỗi → im lặng | thấp | writer đếm lỗi ghi vào cùng `lost`, và dừng thread với một dòng stderr; snapshot thấy `log_lost` tăng, **và `EventKind::MessageLogLost` tới nơi mà không cần ai hỏi** |
| **Bước 0 kết luận "gộp một đường" → plan này phải viết lại phần lớn** | trung bình | đó chính là lý do bước 0 chặn mọi thứ. Chi phí viết lại một plan rẻ hơn nhiều so với chi phí bỏ một module đã có test |
| **Diện tích plan phình sau review**: 5 entry point, sharded, hai `EventKind`, `Drop` cho cả `FileJournal` | trung bình | bước 0 → 1 → 2 → 3 → 3b → 3c là sáu điểm dừng có gate riêng; PR tách theo bước, không gộp một cục |

## Ngoài phạm vi

- Rotation, nén, retention: của người vận hành (`logrotate` + restart, hoặc copytruncate).
- Log kỹ thuật (`tracing`): khác mục đích, khác plan.
- Chọn định dạng nhị phân: không — `grep` là yêu cầu.
- Log ở pre-session **trước** khi frame cắt được (byte rác chưa thành frame): `Framer` chỉ giao khi `Cut`; byte chưa thành frame khi socket đóng là mất — ghi vào `GUIDE.md`, không xử lý.
- Ghi log từ `tools/w2w`: đợt C.
- **Chứng minh trên Linux rằng log không thêm syscall nào lên engine thread**: `check-no-kernel-sleep.sh` chạy với log bật, **đợt C**. Trên macOS canh là `record_touches_no_file_until_the_writer_runs` — trực tiếp, nhưng không phải syscall-level.
- **CRC cho file log** (outside voice #4): bước 0 trả lời trước; nếu vẫn hai đường thì đây là một plan sau, không phải bước của plan này.
- **Rotation theo dung lượng bên trong engine**: vẫn là của `logrotate` + restart. Vá đuôi rách lúc `open` làm `copytruncate` an toàn hơn, nhưng không biến engine thành thứ tự xoay file.

## Nhật ký giao hàng

| Bước | Ngày | Kết quả |
|---|---|---|
| 0 | 2026-09-04 | **Đóng.** Giữ hai đường ghi, không cần ADR — [why-the-message-log-is-not-the-journal](../reference/why-the-message-log-is-not-the-journal.md) |
| 1 | 2026-09-04 | **Xong.** `crates/engine/src/msglog.rs`, `crates/engine/tests/msglog.rs` 12 test. `cargo test --all` 463 → **475**; `--no-default-features` 471. `fmt`/`clippy -D warnings` sạch; `check-no-optional-deps.sh` 6/6; `scripts/bench.sh` — 21 case alloc cũ vẫn **0**, không đổi. **Sáu reversal chạy, sáu lần đỏ, đỏ đúng assertion.** Máy: Apple M5, macOS 25.6.0. **CI xanh 11/11 trên `2ff6a2a`, run [`33854304710`](https://github.com/tmthang86/fixbolt/actions/runs/33854304710)** |

> **`[2026-09-04]` Bước 1 ĐÓNG. CI đứng hình nửa tiếng, và lý do không phải code.** GitHub
> Actions từ chối chạy: cả 11 job của run
> [`33854207867`](https://github.com/tmthang86/fixbolt/actions/runs/33854207867) đều *not
> started*, annotation *"recent account payments have failed or your spending limit needs to be
> increased"*. Run trước đó,
> [`33853041235`](https://github.com/tmthang86/fixbolt/actions/runs/33853041235), hỏng y hệt
> trên một commit **chỉ có docs** — nên không phải workflow, không phải code.
>
> **Nguyên nhân: repo private trên tài khoản Free, hết 2.000 phút Actions của tháng.** Dấu vết
> thời gian nói vậy: 07:51 UTC run xanh 11/11, 08:18 trở đi mọi job không khởi động — một cửa sổ
> 27 phút, dạng hết hạn mức giữa ngày chứ không phải thẻ hỏng từ trước. Cả 11 job đều
> `ubuntu-latest`, hệ số ×1, nên không có job nào ngốn bất thường.
>
> **Gỡ bằng cách đổi repo sang public `[2026-09-04]`** — Actions miễn phí không giới hạn cho
> public repo, và đó vốn là ý định đã ghi ở đầu `CLAUDE.md`. Run tiếp theo xanh 11/11.
>
> **Đoạn này giữ lại nguyên văn**, kể cả câu "bước 1 chưa đóng" đã sai sau đó nửa tiếng, vì nó
> là ví dụ sống của §9: mọi gate trên laptop đều xanh trong khi commit **không có CI**, và nếu
> không ai đọc trang Actions thì bảng trên đã đọc là "xong".

**Reversal đã chạy `[2026-09-04]`**, output đọc chứ không suy:

| Reversal | Thấy |
|---|---|
| bỏ nhánh `\n` trong `escape_into` | `a_data_field_with_a_newline_stays_on_one_line` đỏ, `left: 3` (một message thành hai dòng) |
| bỏ nhánh `\\` | `a_backslash_in_a_data_field_round_trips` đỏ — `96=a\nb` giải mã ra newline thật `10` thay vì hai byte `92, 110` |
| `record` `write_all` thẳng vào file | `record_touches_no_file_until_the_writer_runs` đỏ, `left: 22 right: 0` |
| `open` không vá đuôi rách | `a_torn_last_line_is_marked_not_merged_with_the_next` đỏ |
| bỏ `impl Drop for FileLog` | **lần đầu XANH** — false green, xem dưới. Sau khi sửa test: đỏ, `left: 2 right: 1` |
| `pop → Some(0)` không cộng `lost` | `a_record_longer_than_the_writer_buffer…` đỏ, `left: 0 right: 1` |

**Hai thứ bước 1 tìm ra mà plan không đặt tên**, cả hai đã vào
[a-background-thread-wins-the-race-your-test-was-measuring](../reference/a-background-thread-wins-the-race-your-test-was-measuring.md)
với dấu `[to testing-skills]`:

1. **Test `Drop` đầu tiên là false green.** Nó assert dòng có trong file — mà writer thread
   detached vẫn drain và flush dù `Drop` bị xoá, nên nó đo một cuộc đua chứ không đo `Drop`.
   Thứ chỉ `close()` làm được là **kết thúc writer**, nên assertion đổi sang
   `Arc::strong_count` của counter chia sẻ: 3 khi log còn sống, 1 sau khi drop. Không còn timing.
   `FileLog::counter()` sinh ra từ đó, và bước 3 cần đúng cái `Arc` ấy cho `Snapshot`.
2. **Reversal đầu tiên là no-op.** Regex xoá luật escape khớp nhầm hàm `unescape` cách đó vài
   trăm dòng. Test xanh, và ghi lại thành "reversal xác nhận" thì đã là một lời nói dối do một
   pattern sai sinh ra. Mọi reversal giờ **assert patch của chính nó đã áp** trước khi chạy test.

**Khác plan một chỗ, có lý do:** record trong ring là `dir(1) ‖ at_ms(8) ‖ shard(2) ‖ conn(8)`
= **19 byte**, không phải 21 — `ring::push` đã mang `len` trong header 4 byte của nó rồi, nên
plan đếm thừa một `len` nữa. Buffer writer là `19 + MAX_RECORD`. Và **tín hiệu dừng là một tag
`0xFF`, không phải record rỗng**: `FileJournal` dùng `push(&[])` được vì record của nó không bao
giờ vượt buffer, còn ở đây `pop` báo *"bỏ vì quá dài"* cũng bằng `Some(0)` — hai sự kiện khác
nhau không được mang cùng một giá trị.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | — |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | — | — |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | ISSUES_OPEN | 15 issues, 0 critical gaps, 14 folded into the plan |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | not applicable, no UI |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | — |
| Outside Voice | subagent | Independent plan challenge | 1 | ISSUES_FOUND | 8 findings, 4 spot-checked against source, 6 folded |

- **OUTSIDE VOICE:** Codex was `ready` but returned a usage-limit error (resets 2026-09-11), so
  a Claude subagent ran instead — fresh context, same model family, **not** a cross-model read.
  Weigh its agreement accordingly. Four of its eight findings were verified against source
  before being accepted (`journal.rs:396–424` torn-tail read-back; `journal.rs:486`
  `close(&mut self)` **called by `impl … Drop for FileJournal` at `journal.rs:501`**, which is
  exactly why the plan's `close(self)` by value was wrong; `lib.rs` `ServeError`'s two variants;
  `conn.rs:566–577` `Out` carrying no time field). One of those checks was botched twice on the
  way — an `impl Drop` grep that could not match `impl<const N: usize, …> Drop for …` — and the
  correction is recorded here because the wrong version briefly reached this plan's text.
- **CROSS-MODEL:** not available — no second model ran. The one disagreement is between this
  review and the same-family subagent: the review accepted reusing `ring.rs` + a writer thread
  as correct reuse; the subagent asked why a **second** write path exists at all beside the
  journal being hardened in the same window. Left open as step 0.
- **VERDICT:** ENG REVIEW COMPLETE, PLAN CONDITIONALLY APPROVED — implementation is blocked on
  step 0. Fourteen findings are folded in; one is an open investigation.

**Step 0 closed the same day** — `docs/reference/why-the-message-log-is-not-the-journal.md`.
The answer is *keep two*, so no ADR was needed and the design is unchanged: the journal's key is
`seq`, and the three things the log exists for have no `seq`; `Journal` is a session-layer trait
while refused frames are stopped before the session sees them; `Durability::Fsync` blocks the
engine thread deliberately and the log must never fsync; and the merge touches the record
constants in 31 places plus `Reader`/`Record`/`Records`/`tools/jrnl`, so it is the more expensive
option. Half of outside voice #4 was accepted: the two files have different failure modes, so the
journal keeps CRC (step 4) and the log gets a marked torn tail (step 1) instead.

NO UNRESOLVED DECISIONS
