# Phase 3: sửa năm lỗi mà chính phase 3 tìm ra

> **Loại:** Plan · **Ngày:** 2026-09-23 · **Trạng thái:** Đã duyệt (manager, 2026-09-23, theo mandate thường trực của owner)
> **Phạm vi:** bốn lỗi (D1–D4) do reviewer hoặc builder của phase 3 tìm ra, mỗi cái có cách tái
> hiện, và D5 do architect tìm thấy khi đọc cho plan này (manager yêu cầu thêm) — **chưa nằm
> trong plan nào**. Mandate thường trực của owner: mọi mục mở phải đóng.
> Thiết kế: [ADR-0150](../decisions/ADR-0150-the-journal-writer-holds-the-largest-record-the-slot-allows-and-stops-only-on-a-record-no-message-can-be.md)
> (D1, D5) và [ADR-0151](../decisions/ADR-0151-a-tls-handshake-this-end-refuses-sends-its-alert-and-is-counted-and-a-peer-that-leaves-is-not.md)
> (D3), cả hai **Proposed** — duyệt plan này là duyệt luôn hai ADR đó. D2 và D4 không cần ADR:
> D2 là lỗi rõ ràng, D4 do đặc tả FIX quyết chứ không phải một lựa chọn.

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Trong lúc làm phase 3, bốn chỗ hỏng lộ ra (thêm D5, thấy khi lập plan này). Không cái nào do phase 3 gây ra; phase 3 chỉ là lần
đầu có ai đi qua đúng chỗ đó:

- **D1 — mất dữ liệu.** Journal ghi file ở chế độ `Async` âm thầm **ngừng ghi vĩnh viễn** khi
  gặp một message dài hơn 4 088 byte. Mọi message sau đó cũng không xuống đĩa. Sau khi khởi động
  lại, engine không nhớ gì về chúng.
- **D2 — tắt chậm.** Một initiator đang chờ để quay số lại (reconnect) không nghe thấy lệnh
  `Admin::shutdown` cho tới khi hết giờ chờ — đo được **+26 giây** với `ReconnectInterval=30`.
- **D3 — im lặng khi TLS hỏng.** Khi hai bên không có bộ mã hoá (cipher suite) nào chung,
  acceptor của fixbolt đóng kết nối **không gửi TLS alert** và **không phát event nào**. Bên kia
  chỉ thấy "đứt kết nối", người vận hành bên này không thấy gì.
- **D4 — đọc số sai luật.** Hàm công khai `as_i64` của `codec` nhận dấu `+` ở đầu (`+200` →
  200), trong khi kiểu `int` của FIX không có dấu `+`, và phần còn lại của engine đã từ chối nó.
- **D5 — thread ghi đốt một core khi rảnh.** Thread ghi của journal `Async` quay tít khi không có
  gì để ghi, và thread ghi của message log thì `yield_now` (ADR-0013 nói đó **không** phải là trả
  core) — ở cả `standard`, nơi engine hứa trả core khi rảnh. Không gate nào thấy, vì các script
  chế độ chỉ nhìn engine thread.

Plan này sửa cả năm, mỗi cái bắt đầu bằng một test đỏ trên code hôm nay.

## Những gì đã biết chắc

Đọc ngày 2026-09-23, worktree `fb-p3fix`, `main` = `2f0a0dc`. Mỗi lỗi đã được **xác nhận bằng
cách đọc code** ở đúng chỗ ghi dưới đây.

### D1 — journal `Async` dừng ở message dài

- `FileJournal::put` dưới `Async` đẩy vào ring một bản ghi: 4 byte số thứ tự + 4 byte độ dài +
  message (`crates/engine/src/journal.rs:854-855`).
- Thread ghi (`write_loop`, `journal.rs:792-835`) đọc ring vào **bộ đệm cố định
  `[0u8; 4096]`** (`journal.rs:793`).
- `ring::Consumer::pop` gặp bản ghi dài hơn bộ đệm thì **bỏ nó và trả `Some(0)`**
  (`crates/engine/src/ring.rs:183-222`).
- `write_loop` coi `Some(0)` là **tín hiệu dừng** (`journal.rs:798-801`) — tín hiệu mà `close()`
  gửi bằng một bản ghi rỗng (`journal.rs:770-781`). Hai nghĩa dùng chung một giá trị → message
  dài làm thread ghi thoát.
- `MemJournal::put` chỉ từ chối `bytes.len() > LEN` (`journal.rs:153`), nên message 4 200 byte
  với `LEN = 8192` được nhận vào bộ nhớ, `put` trả `true`, nhưng không bao giờ xuống file.
- Cách tái hiện của reviewer: `FileJournal<8, 8192>` `Async`, `put` 4 200 byte rồi một message
  nhỏ, mở lại → `get(1)=None`, `get(2)=None`, `highest_out=None`.
- Mặc định `SLOT_LEN = 512` (`journal.rs:63`), nên chỉ deployment nào tự tăng `LEN` lên trên
  4 088 mới gặp. `Durability::Fsync` ghi thẳng, không bị.
- **Lỗi thứ hai cùng chỗ, tìm thấy khi đọc (chưa ai tái hiện):** `MemJournal::put` lưu độ dài
  bằng `u16::try_from(len).unwrap_or(0)` (`journal.rs:173`) và `get` chỉ trả khi `len > 0`
  (`journal.rs:198`). Với `LEN > 65 535`, message dài hơn 65 535 byte được báo là "đã giữ" rồi
  lại trả `None` — một lần từ chối không được đếm (ADR-0046 đòi phải đếm).
- Message log đã giải đúng bài này: bộ đệm cỡ bản ghi lớn nhất (`msglog.rs:85`), cấp phát một
  lần trên thread ghi, tín hiệu dừng là bản ghi **1 byte `STOP`** (`msglog.rs:94`, `:408-460`).

### D2 — vòng quay số không nghe `shutdown` khi đang chờ

- `dial` (`crates/engine/src/lib.rs:2558-2688`), nhánh `Next::At(_)` (`lib.rs:2609-2616`): gọi
  `engine.idle_with(&[])` rồi `continue` về đầu vòng.
- Vì `continue`, hai việc bị bỏ qua: `engine.turn()` (`lib.rs:2679`) — nơi
  `begin_shutdown_if_asked` phát hiện có người đòi dừng (`lib.rs:998`, gọi ở `lib.rs:1171`) — và
  `engine.shutdown_finished()` (`lib.rs:2694`). Chỉ khi policy trả `Next::Now` thì vòng mới đi
  qua hai chỗ đó.
- `Admin::shutdown` chỉ bật một cờ atomic (`crates/engine/src/observe.rs:1193-1201`), không đánh
  thức engine. Engine `standard` (`Block`) tự thức sau tối đa `DEFAULT_TIMEOUT_MS = 100` ms
  (`crates/engine/src/block.rs:33`).
- `dial` chỉ tồn tại dưới `feature = "standard"` và chỉ được gọi với `Block`
  (`lib.rs:2271-2286`, `lib.rs:2454`). **Không có vòng quay số nào ở chế độ `hft`.**
- Đo được: `Shutdown {` in ra ở **+26 s** sau lệnh dừng, `ReconnectInterval=30` (trang
  reference trên nhánh `plan/p3-quickfixj-interop`,
  `docs/reference/a-dial-in-its-reconnect-wait-hears-shutdown-only-when-the-timer-fires.md`).
- Mẫu test đo CPU của thread đã có:
  `crates/engine/tests/tls_initiator_wire.rs:839-930`
  (`the_dial_loop_sleeps_rather_than_spins_while_the_handshake_waits`).

### D3 — TLS không có suite chung: không alert, không event

- fixbolt chỉ cho phép `TLS13_AES_128_GCM_SHA256` (`crates/engine/src/tls.rs:821-825`).
- Bộ điều khiển handshake `Handshake::pump` trả `Step::Failed(InvalidData)` ngay khi
  `process_tls_records` báo lỗi (`tls.rs:502`).
- Đọc mã nguồn `rustls` 0.23.45 (bản trong `Cargo.lock`): khi không có suite chung, rustls
  **xếp hàng** alert `handshake_failure` (`server/hs.rs:471`, `common_state.rs:562-572`) rồi trả
  lỗi. Với API unbuffered, alert đó chỉ được đưa ra ở **lần gọi `process_tls_records` tiếp theo**
  (`conn/unbuffered.rs:42-175`: nhánh lấy `sendable_tls` đứng trước nhánh kiểm tra lỗi). fixbolt
  không gọi lần tiếp theo → alert không bao giờ lên dây.
- Phía event: transport hỏng trả `Io::Failed` (`tls.rs:1421-1423`); tầng pre-session gom mọi
  `Closed`/`Failed` thành `gone` (`crates/engine/src/presession.rs:797`, đếm ở `:776`); vòng
  acceptor chỉ chuyển tiếp `unframeable` cho engine (`lib.rs:3285-3289`). `gone` không đi đâu cả.
- Initiator dùng chung `pump` (generic theo `Side`), nên cũng mất alert; `Admission::Failed` chỉ
  gọi `policy.dropped` (`lib.rs:2673`).
- Câu hỏi mở số 3 của trang reference ("`TlsRequireKernel=N` có khác không?") trả lời được bằng
  đọc: handshake luôn chạy trong rustls trước khi xét offload (`tls.rs:1250-1262`) → **giống hệt**.
- `EventKind` là `#[non_exhaustive]` (`observe.rs`), thêm variant không phá ai. Mẫu event không có
  `ConnId`: `OriginationUndeliverable { count: u64 }` phát dưới `ConnId::MAX` (`lib.rs:982-986`).
- `Step` (`tls.rs:326`) và `Progress` (`presession.rs:566-567`) **không** `#[non_exhaustive]`.
- Shard acceptor không có cửa TLS (`crates/engine/src/shard.rs` không nhắc tới TLS).
- CI chạy test TLS bằng `cargo test -p fixbolt-engine --tests --features tls --no-fail-fast`
  rồi `scripts/check-feature-gated-tests-ran.sh` (`.github/workflows/ci.yml:789-830`).

### D4 — `as_i64` nhận `+`

- `as_i64` (`crates/codec/src/index.rs:240-267`) có nhánh `Some((b'+', rest)) => (false, rest)`.
- Session **đã đúng**: kiểm tra kiểu `int` dùng `signed_int` của `dict`, chỉ cho `-`
  (`crates/dict/src/field_type.rs:182`, `:237-240`). Kiểu float dùng `signed_number`, cũng từ chối
  `+` vì `14f_IncorrectDataFormat.def` gửi `38=+200.00` và đòi `373=6` (`field_type.rs:242-261`).
- **Không ai trong repo gọi `as_i64`** ngoài re-export: session dùng `as_u32`
  (`crates/session/src/lib.rs:3257-3652`); `crates/library/src/lib.rs:16` re-export cho người
  dùng. Không test nào khẳng định `as_i64` nhận `+`; không fuzz target, không bench dùng nó.
- `as_decimal` (phase 3 bước 4) đã từ chối `+`, có test `a_leading_plus_is_refused`
  (`crates/codec/tests/decimal.rs:158-163`), và comment ở đó ghi thẳng "`as_i64` takes a `+`".
- `codec` có `fixbolt-dict` làm dev-dependency, nên test có thể so với `FieldType::Int.accepts`.

### D5 — thread ghi quay tít khi rảnh

- `write_loop` của journal: ring rỗng → `std::hint::spin_loop()` rồi hỏi lại
  (`crates/engine/src/journal.rs:832`), mọi chế độ.
- `write_loop` của message log: ring rỗng → flush nếu cần rồi `std::thread::yield_now()`
  (`crates/engine/src/msglog.rs:422-429`). ADR-0013 mục 2: `yield_now` không phải chế độ
  `standard` — trên máy rảnh thread được chạy lại ngay.
- Điều bất di bất dịch 4 chỉ nói về engine thread; `scripts/check-no-kernel-sleep.sh` lọc syscall
  theo tid engine thread (header dòng 14), script `standard` đo CPU engine thread. **Không gate nào
  nhìn thread ghi.** `STATUS.md` (*Not proven*, quanh dòng 1547): các script chế độ chưa bao giờ
  chạy với `--journal`/`--log`.
- Không thread ghi nào biết chế độ engine: `FileJournal::open(path, how)` (`journal.rs:490`),
  `FileLog::open(path)` / `open_pinned` (`msglog.rs:193`, `:217`); w2w dùng cùng `--journal file-async` cho cả
  hai chế độ (`tools/w2w/src/main.rs:1253-1260`).
- Ring journal 1 MiB (`journal.rs:711`); ring message log `DEFAULT_CAPACITY` 4 MiB hoặc do người
  gọi chọn (`msglog.rs`). Thread ghi đã ghim có tên `fixbolt-journal` / `fixbolt-msglog`
  (`affinity::spawn_pinned`); thread **không ghim** của cả hai thì không có tên (`journal.rs:723`,
  `msglog.rs:314`, `std::thread::spawn`).
- Cả ba script chế độ nhận `W2W_EXTRA`; w2w nhận `--journal file-async` và `--log file`.

### Trên internet (đọc ngày 2026-09-23)

| Nguồn | Một dòng |
|---|---|
| [FIX 5.0 SP2 datatypes (fiximate)](https://fiximate.fixtrading.org/legacy/en/FIX.5.0SP2/fix_datatypes.html), [FIX 4.3 data types (btobits)](https://btobits.com/fixopaedia/fixdic43/data_types.html) | `int`: *"Sequence of digits without commas or decimals and optional sign character (ASCII characters "-" and "0" - "9")"* — dấu chỉ có `-`. |
| [RFC 8446 §6.2](https://datatracker.ietf.org/doc/html/rfc8446#section-6.2) | Gặp lỗi nghiêm trọng thì nên gửi alert fatal phù hợp rồi mới đóng. |
| [docs.rs `UnbufferedStatus`](https://docs.rs/rustls/0.23.45/rustls/unbuffered/struct.UnbufferedStatus.html), [docs.rs `rustls::unbuffered`](https://docs.rs/rustls/latest/rustls/unbuffered/index.html) | Tài liệu **không nói** gì về alert sau khi `state` là `Err` — hành vi chỉ đọc được từ mã nguồn (ghi ở mục Rủi ro). |
| [QuickFIX/J `FileStore.java`](https://github.com/quickfix-j/quickfixj/blob/master/quickfixj-core/src/main/java/quickfix/FileStore.java) | Ghi nguyên message xuống file, không có bộ đệm cố định hay giới hạn cỡ. |
| [QuickFIX/J QFJ-949](https://www.quickfixj.org/jira/browse/QFJ-949) | QuickFIX/J báo handshake hỏng bằng `SSLHandshakeException` trong log — engine khác có nói ra. |

## Cách làm

### D1 (ADR-0150)

1. `write_loop` nhận thêm tham số độ dài bản ghi lớn nhất và đọc ring vào bộ đệm
   `vec![0u8; RECORD_HEADER + LEN]`, cấp phát **một lần trên thread ghi, trước vòng lặp**.
2. `close()` gửi bản ghi **1 byte `STOP`** thay cho bản ghi rỗng; `write_loop` dừng khi gặp đúng
   bản ghi 1 byte đó. `Some(0)` từ `pop` → `continue`, không bao giờ dừng.
3. `MemJournal::put` trả `false` khi `bytes.len() > u16::MAX as usize` (trước khi đụng slot).

File sửa: `crates/engine/src/journal.rs`. Test: `crates/engine/tests/journal.rs` và một
`#[cfg(test)] mod` mới trong `journal.rs` (vì `write_loop` là hàm riêng).

### D2

Nhánh `Next::At(_)` **bỏ `idle_with` và `continue`**, rơi xuống phần chung cuối vòng
(`turn` → kiểm tra `shutdown_finished` → idle). Phần idle cuối vòng thêm nhánh thứ ba: không có
kết nối, không có handshake đang chờ → `engine.idle_with(&[])`. Kết quả: mỗi vòng chờ vẫn ngủ
tối đa một timeout của `Block` (100 ms) như hôm nay, nhưng mỗi vòng đều hỏi "có ai đòi dừng
không". Độ trễ tắt khi đang chờ quay số: từ "tới khi hết `ReconnectInterval`" xuống "≤ một
timeout của `Block` + một turn".

File sửa: `crates/engine/src/lib.rs` (chỉ thân `dial`). Test: `crates/engine/tests/reconnect_wire.rs`.

### D3 (ADR-0151)

1. `Handshake::pump`: khi `process_tls_records` trả `Err`, áp `discard`, gọi lại
   `process_tls_records` **tối đa 4 lần**, mã hoá mọi `EncodeTlsData` vào bộ đệm ra sẵn có,
   `TransmitTlsData` coi như flush; rồi flush **một lần, không chặn**; rồi trả `Step::Refused`
   (variant mới). `Step::Failed(kind)` giữ nghĩa "socket hỏng / peer bỏ đi".
2. `TlsTransport` nhớ đã `Refused`; trait `Transport` thêm
   `fn handshake_refused(&self) -> bool { false }`, `TlsTransport` trả `true` sau `Refused`.
3. `presession::Progress` thêm `tls_refused: usize`: socket kết thúc mà transport trả
   `handshake_refused()` thì đếm vào đây thay vì `gone`.
4. Engine thêm `note_tls_refused(n)` (cùng dạng `note_unframeable`, `lib.rs:821`), phát
   `EventKind::TlsHandshakeRefused { count: u64 }` dưới `ConnId::MAX` khi `n > 0`. Vòng acceptor
   (`lib.rs:3285-3289`) gọi nó; nhánh `Admission::Failed` của `dial` gọi nó với 1 khi transport
   trả `handshake_refused()`.
5. Peer kết nối rồi bỏ đi giữa handshake: vẫn là `gone`, không event.

File sửa: `crates/engine/src/tls.rs`, `crates/engine/src/transport.rs`,
`crates/engine/src/presession.rs`, `crates/engine/src/observe.rs`, `crates/engine/src/lib.rs`
(chỉ `note_tls_refused`, vòng acceptor ở `:3285-3289`, và arm `Admission::Failed` của `dial`).
Test: `crates/engine/tests/tls.rs`, `tls_wire.rs`, `tls_initiator_wire.rs`.

### D5 (ADR-0150 quyết định 4, sửa tại chỗ)

Một luật chờ chung cho cả hai thread ghi, đặt trong `ring.rs` cạnh hàng đợi: ring rỗng → đếm;
dưới **1 024 lần rỗng liên tiếp** thì `spin_loop` (bắt kịp một đợt dồn mà không syscall); từ đó
trở đi `std::thread::sleep(1 ms)`; có bản ghi thì đếm lại từ 0. **Engine thread không đánh thức
thread ghi** (không `unpark`, không futex phía đẩy) → đường của engine thread không đổi một byte,
ở cả hai chế độ. Không cần tham số chế độ mới. Thread journal không ghim được đặt tên
`fixbolt-journal` (qua `std::thread::Builder`) để test và người vận hành tìm được.

File sửa: `crates/engine/src/ring.rs` (thêm luật chờ), `crates/engine/src/journal.rs`
(`write_loop` + đặt tên thread), `crates/engine/src/msglog.rs` (`write_loop` + đặt tên thread `fixbolt-msglog`). Test mới:
`crates/engine/tests/writer_idle.rs` (file riêng — một tiến trình test riêng, để không lẫn
thread ghi của test khác).

### D4

Xoá nhánh `b'+'` trong `as_i64`; sửa rustdoc: *"An optional leading `-`, then digits. A `+` is
refused: FIX `int` has none."* File sửa: `crates/codec/src/index.rs`. Test mới:
`crates/codec/tests/int.rs`.

## Bất biến bị đụng tới

| # | Điều | D1 | D2 | D3 | D4 | Giữ bằng cách nào |
|---|---|---|---|---|---|---|
| 1 | Không cấp phát trên hot path | có | có | có | có | D5: `sleep` không cấp phát, engine thread không đổi. D1: cấp phát mới nằm trên **thread ghi**, không phải engine thread (ADR-0037); `put` trên engine thread không đổi. D2: vòng chờ không có message. D3: rustls cấp phát trong handshake là ngoài hot path (đã vậy từ ADR-0005); `emit` chỉ đẩy một `Event` `Copy` vào ring (`observe.rs:382-384`), chỉ khi `n > 0`. D4: `as_i64` không cấp phát. Chứng minh: `cargo bench -p fixbolt-engine --bench alloc` và `cargo bench -p fixbolt-codec --bench alloc` xanh, **không sửa case nào** |
| 4 | `hft` không ngủ, `standard` phải ngủ | không | **có** | có (gián tiếp) | không | **D5 (cột ngoài bảng): có** — thread ghi không phải engine thread, nên điều 4 không trực tiếp áp, nhưng D5 là lời hứa "`standard` trả core" cho cả tiến trình; chứng minh **cả hai chế độ**: engine thread `hft` vẫn không ngủ **khi có thread ghi đang ngủ cạnh nó** (`W2W_EXTRA="--journal file-async --log file"` với `check-no-kernel-sleep.sh` và `-by-ctxt.sh`), engine thread `standard` vẫn trả core với cùng `W2W_EXTRA` (`check-standard-gives-the-core-back.sh`), và thread ghi trả core (test mới). D2 đổi cách chờ của `dial` → chứng minh **cả hai chế độ** (ADR-0013, `CLAUDE.md` §7): `standard` bằng test CPU mới (bước D2) + `scripts/check-standard-gives-the-core-back.sh`; `hft` không có vòng quay số (xem *Những gì đã biết chắc*), nên chứng minh bằng `scripts/check-no-kernel-sleep.sh` và `scripts/check-no-kernel-sleep-by-ctxt.sh` vẫn xanh trên cùng commit (w2w build lại cùng `lib.rs`). D1 không đụng: thread ghi không phải engine thread và cách nó chờ không đổi. D3 không đổi cách chờ nhưng đụng transport → chạy script `standard` (có nhánh `--tls ktls`) |
| 6 | Feature gate ở `mod` | — | — | có | — | `Step::Refused` nằm trong `mod tls` (đã sau `#[cfg(feature = "tls")]`); `handshake_refused` trên trait lõi **không** theo feature (như `tls_mode`). Chứng minh: `cargo test --no-default-features -p fixbolt-engine` và `scripts/check-no-optional-deps.sh` |
| 7 | Không `unwrap`/`expect`/`panic`, không index panic | có | có | có | có | lint workspace; `scripts/check-indexing-debt.sh` không được tăng |
| 3 | 59 acceptance definitions | — | — | — | gián tiếp | session không gọi `as_i64`, nhưng codec đổi hành vi công khai → chạy `cargo test -p fixbolt-conformance` ở hàng đóng |

Không có `unsafe` mới (điều 8). Không số hiệu năng nào được công bố (điều 10).

## Chia việc

**Về chạy song song.** Brief ban đầu coi bốn lỗi là bốn nhóm file rời nhau. **Đọc code thì
không hẳn:** D2 và D3 **cùng sửa `crates/engine/src/lib.rs`** (D2: thân `dial`; D3: nhánh
`Admission::Failed` trong `dial`, vòng acceptor, và một hàm `note_`). Theo luật "một file, một
người viết", D3 chạy **sau khi D2 đã commit**. **D5 cũng sửa `journal.rs` như D1**, nên D5 chạy
**sau khi D1 đã commit, do chính worker của D1** (tiếp tục bằng `SendMessage`, giữ context). D1, D2, D4 rời nhau về code và chạy song song,
**mỗi worker một `git worktree`** tách từ nhánh plan. File tài liệu dùng chung (`CHANGELOG.md`,
`DESIGN.md`) thì mỗi worktree tự thêm phần của mình; manager đưa commit vào nhánh theo thứ tự
D4 → D1 → D2 → D3 và gỡ xung đột (chỉ là thêm dòng) khi đó.

| Hàng | Kết quả | Người làm | Chạm vào (code + test) | Không chạm | Phụ thuộc |
|---|---|---|---|---|---|
| 0 | ADR-0150, ADR-0151 được chấp nhận (duyệt plan này) | manager | — | — | — |
| D4 | `as_i64` từ chối `+` | senior developer (`opus`) — `codec`, hành vi công khai | `crates/codec/src/index.rs` (chỉ `as_i64` + rustdoc), `crates/codec/tests/int.rs` (mới) | mọi crate khác; `decimal.rs`; `crates/dict/` | 0 |
| D1 | journal `Async` giữ message dài và không dừng vì dữ liệu | senior developer (`opus`) — `engine`, mất dữ liệu | `crates/engine/src/journal.rs`, `crates/engine/tests/journal.rs` | `ring.rs`, `msglog.rs`, `dispatch.rs`, `lib.rs` | 0 |
| D5 | hai thread ghi ngủ khi rảnh, mọi chế độ | senior developer (`opus`) — **cùng worker với D1** | `crates/engine/src/ring.rs` (chỉ thêm luật chờ), `journal.rs` (`write_loop`, đặt tên thread), `msglog.rs` (`write_loop`), `crates/engine/tests/writer_idle.rs` (mới), `crates/engine/Cargo.toml` chỉ nếu test cần khai báo | `lib.rs`, `dispatch.rs`, `block.rs`, `wait.rs` | **D1 đã commit** |
| D2 | `dial` nghe `shutdown` trong lúc chờ quay số, vẫn ngủ | senior developer (`opus`) — engine thread, điều 4 | `crates/engine/src/lib.rs` (chỉ thân `fn dial`, `:2558-2688`), `crates/engine/tests/reconnect_wire.rs` | `reconnect.rs`, `block.rs`, `observe.rs`, `tls.rs`, `presession.rs` | 0 |
| D3 | handshake bị từ chối: gửi alert + phát event | senior developer (`opus`) — `transport`, API công khai | `crates/engine/src/tls.rs`, `transport.rs`, `presession.rs`, `observe.rs`, `lib.rs` (chỉ ba chỗ ở *Cách làm* D3), `tests/tls.rs`, `tests/tls_wire.rs`, `tests/tls_initiator_wire.rs` | `journal.rs`, `ring.rs`, `shard.rs`, `crates/session/` | 0, **D2 đã commit** |
| R | Review một lần, senior developer **mới** (context sạch), đưa plan + gate, không đưa lập luận của worker | senior developer (`opus`, fresh) | — | — | D1–D4 |
| C | Gate đóng plan (mục *Cách kiểm chứng*), `STATUS.md`, CI run id | manager | `STATUS.md` | — | R |

Mỗi hàng: tài liệu của hàng đó sửa **cùng commit** với code (mục *Tài liệu phải cập nhật*),
do chính worker đó viết. Worker không commit; manager chạy lại gate và commit.

### Hàng D4 — `as_i64` từ chối `+`

- **Test đỏ trước** (`crates/codec/tests/int.rs`, bọc trong `mod int { … }` để filter bắt được):
  - `a_leading_plus_is_refused`: `as_i64(b"+200") == Err(NotANumber)`, `as_i64(b"+0")`,
    `as_i64(b"+")` cũng vậy. **Đỏ hôm nay**: trả `Ok(200)`.
  - `as_i64_agrees_with_the_dictionarys_int_rule`: với một tập chuỗi cố định (chữ số, `-`, `+`,
    `.`, rỗng, dài 1–20, sinh bằng xorshift seed cố định như `tests/decimal.rs`),
    `as_i64(s).is_ok() == FieldType::Int.accepts(s)` trừ khi `as_i64` trả `Overflow`. **Đỏ hôm
    nay** trên mọi chuỗi bắt đầu bằng `+`.
  - `a_minus_and_leading_zeros_still_parse` (khoá phần không đổi): `-5`, `-0`, `00023` → `-5`, `0`,
    `23`. Xanh cả trước lẫn sau.
- **Đảo ngược:** thêm lại nhánh `b'+'` → đỏ ở `a_leading_plus_is_refused` với câu
  *"`+200` must be NotANumber, FIX int has no plus"* và ở test so với `dict`; bỏ đi → xanh. Ghi câu
  FAIL mong đợi trước khi chạy.
- **Gate:**
  `cargo test -p fixbolt-codec --test int`,
  `cargo test -p fixbolt-codec decimal`,
  `cargo test --no-default-features -p fixbolt-codec`,
  `cargo test -p fixbolt` (facade re-export),
  `cargo clippy -p fixbolt-codec --all-targets -- -D warnings`,
  `cargo bench -p fixbolt-codec --bench alloc`.
- **Xong khi:** ba test xanh, đảo ngược đã chạy và trích, `tests/decimal.rs` không sửa, mọi gate
  trích nguyên văn.
- **Tài liệu:** `CHANGELOG.md` *Changed* — ghi rõ **thay đổi phá vỡ**: `as_i64(b"+5")` giờ là
  `Err(NotANumber)`, lý do là đặc tả FIX `int`; `docs/internals/codec.md` (dòng `index.rs` nếu có
  nhắc bộ đọc số). Comment ở `crates/codec/tests/decimal.rs:161` sẽ lỗi thời nhưng **không** sửa
  trong hàng này (xem *Bẫy*).

### Hàng D1 — journal `Async`

- **Test đỏ trước:**
  - `tests/journal.rs::an_async_journal_keeps_a_message_longer_than_four_kilobytes_and_all_that_follow`:
    `FileJournal<8, 8192>` `Async`, `put(1, <4 200 byte>)`, `put(2, <nhỏ>)`, `close`, mở lại cùng
    file → `get(1)` dài 4 200, `get(2)` có, `highest_out() == Some(2)`. **Đỏ hôm nay**
    (`None`/`None`/`None`), câu FAIL: *"the writer stopped at the 4 200-byte record and nothing
    after it reached the file"*.
  - `tests/journal.rs::a_message_longer_than_a_u16_is_refused_not_kept_empty`:
    `MemJournal<2, 70_000>`, `put(1, <66 000 byte>)` → `false`, `get(1) == None`. **Đỏ hôm nay**:
    `put` trả `true`.
  - `journal.rs` `#[cfg(test)] mod writer_tests::a_record_the_writer_cannot_hold_does_not_stop_it`:
    gọi thẳng `write_loop` với bộ đệm nhỏ cố ý (tham số mới), ring chứa: một bản ghi quá dài, một
    bản ghi hợp lệ, `STOP` → file chứa bản ghi hợp lệ. Test này canh quyết định 2 của ADR-0150
    (tín hiệu dừng), vì sau quyết định 1 thì test đầu không còn chạm được `Some(0)`. Theo mẫu
    `lib.rs:3528` (không `unwrap` trong crate thư viện).
- **Đảo ngược:** (a) đặt lại bộ đệm 4 096 → test đầu đỏ; (b) cho `Some(0)` dừng lại → test
  `writer_tests` đỏ; (c) bỏ kiểm tra `u16` → test thứ hai đỏ. Ghi câu FAIL trước mỗi lần.
- **Gate:**
  `cargo test -p fixbolt-engine --test journal`,
  `cargo test -p fixbolt-engine --lib writer_tests`,
  `cargo test -p fixbolt-engine --test on_disk --test journal_reader --test secrets_stay_off_disk --test recovery --test engine_recovery`,
  `cargo test --no-default-features -p fixbolt-engine`,
  `cargo clippy -p fixbolt-engine --all-targets -- -D warnings`,
  `scripts/check-indexing-debt.sh`,
  `cargo bench -p fixbolt-engine --bench alloc`.
- **Xong khi:** ba test xanh, ba đảo ngược đã chạy, test cũ xanh **không sửa**.
- **Tài liệu:** `docs/reference/an-async-journal-record-longer-than-its-writers-buffer-stopped-the-writer.md`
  (mới — bẫy, ưu tiên cao nhất, nêu test canh); `DESIGN.md` §4 D7 (một câu: bộ đệm thread ghi
  theo `LEN`, tín hiệu dừng là `STOP`); `docs/CONFIGURATION.md` dòng `SLOT_LEN` (message dài hơn
  65 535 byte không bao giờ được giữ); `docs/GUIDE.md` (ràng buộc compiler không kiểm được: đặt
  `LEN` > 65 535 không giúp giữ message dài hơn); `CHANGELOG.md` *Fixed*; ADR-0150 → *Accepted*.

### Hàng D5 — thread ghi ngủ khi rảnh

- **Test đỏ trước** (`crates/engine/tests/writer_idle.rs`, `#[cfg(target_os = "linux")]`, đọc
  `/proc/self/task/<tid>/{comm,stat}` như `tls_initiator_wire.rs:839-930`):
  - `an_idle_async_journal_writer_gives_its_core_back`: mở `FileJournal<8, SLOT_LEN>` `Async`,
    `put` một message (để chắc thread đã chạy), chờ 200 ms, tìm tid có `comm` = `fixbolt-journal`,
    đo `utime+stime` trong 1 s → **< 50 ms** CPU. **Đỏ hôm nay** vì hai lẽ: thread không ghim
    không có tên (câu FAIL *"no thread named fixbolt-journal"*) — nên bước đầu worker **chỉ đặt
    tên thread**, chạy lại, và phải thấy đỏ thật: *"the journal writer used ~1000 ms of CPU in
    1 s while idle"*.
  - `an_idle_message_log_writer_gives_its_core_back`: cùng cách với `FileLog` (`comm` =
    `fixbolt-msglog`; thread không ghim cũng chưa có tên, nên đặt tên như trên). **Đỏ hôm
    nay** (`yield_now` ≈ 100 % trên máy rảnh; trên máy bận có thể thấp hơn — ghi con số đo được).
  - `an_async_journal_still_writes_a_burst_after_it_slept`: để rảnh 50 ms (thread đã ngủ), `put`
    1 000 message, `close`, đọc file → đủ 1 000. Xanh cả trước lẫn sau — canh việc thức dậy.
- **Đảo ngược:** (a) trả `spin_loop` vô hạn cho journal → test 1 đỏ; (b) trả `yield_now` cho
  message log → test 2 đỏ; (c) cho luật chờ không bao giờ đếm lại về 0 → test 3 vẫn phải xanh
  (chỉ chậm hơn) — ghi rõ đây là đảo ngược **không** đỏ, và vì sao.
- **Gate:**
  `cargo test -p fixbolt-engine --test writer_idle`,
  `cargo test -p fixbolt-engine --test journal --test msglog --test on_disk --test secrets_stay_off_disk --test journal_reader`,
  `cargo test -p fixbolt-engine --features affinity --test affinity` (thread ghim vẫn đúng tên,
  đúng core),
  `cargo test --no-default-features -p fixbolt-engine`,
  `cargo clippy -p fixbolt-engine --all-targets -- -D warnings`, `scripts/check-indexing-debt.sh`,
  `cargo bench -p fixbolt-engine --bench alloc`, `cargo bench -p fixbolt-engine --bench journal`
  (chạy để thấy không vỡ; không công bố số),
  **trên Linux, cả hai chế độ, có thread ghi:**
  `W2W_EXTRA="--journal file-async --log file" scripts/check-no-kernel-sleep.sh`,
  `W2W_EXTRA="--journal file-async --log file" scripts/check-no-kernel-sleep-by-ctxt.sh`,
  `W2W_EXTRA="--journal file-async --log file" scripts/check-standard-gives-the-core-back.sh`
  (build w2w với `--release` trước; nếu một script không chấp nhận hai cờ đó thì dừng và báo, không
  sửa script).
- **Xong khi:** ba test xanh, đảo ngược (a)(b) đỏ đúng câu, ba script xanh **với** thread ghi và
  nửa đỏ của mỗi script vẫn đỏ.
- **Tài liệu:** `DESIGN.md` §4 D7 và D14 (thread ghi ngủ 1 ms sau 1 024 lần rỗng; engine không
  đánh thức nó); `docs/best-practices-standard.md` và `docs/best-practices-hft.md` (thread ghi giờ
  trả core; với `hft` ghim thread ghi vẫn nên làm, ADR-0015 quyết định 8); `docs/GUIDE.md` (dữ liệu
  `Async` có thể trễ thêm ~1 ms trước khi xuống đĩa); `docs/reference/a-writer-thread-no-gate-watched-spun-a-core.md`
  (mới — bẫy: gate chỉ nhìn engine thread); `STATUS.md` *Not proven*: gạch phần "mode scripts never
  ran with a journal writer or a log" (manager, hàng C, cùng commit); `CHANGELOG.md` *Fixed*.

### Hàng D2 — `dial` nghe `shutdown`

- **Test đỏ trước** (`crates/engine/tests/reconnect_wire.rs`, `#[cfg(target_os = "linux")]` cho
  test đo CPU):
  - `a_dial_waiting_to_reconnect_stops_when_asked_not_when_the_timer_fires`: bind rồi thả một
    cổng (không ai nghe → quay số bị từ chối → vào nhánh chờ), `Policy::new(20_000, 20_000)`,
    `connect_and_serve` trên thread riêng, chờ 300 ms, `admin.shutdown(0)`, nhận kết quả qua
    `mpsc` với `recv_timeout(5 s)`. Khẳng định: trả về trong 5 s, `Shutdown.sessions == 0`.
    **Đỏ hôm nay**: timeout 5 s, câu FAIL *"a dial in its reconnect wait did not hear
    Admin::shutdown within 5 s; it hears it only when the 20 s timer fires"*. Thread bị bỏ lại như
    `an_initiator_whose_counterparty_hangs_up_comes_back` đã làm.
  - `the_dial_loop_sleeps_rather_than_spins_while_it_waits_to_reconnect`: cùng dựng như trên
    (không gọi `shutdown`), đo CPU và trạng thái `S` của thread quay số trong một cửa sổ, theo
    đúng cách `tls_initiator_wire.rs:839-930` (chép helper `task_cpu_and_state` và hằng số sang,
    ghi nguồn). Khẳng định CPU dưới cùng ngưỡng, thread sống, phần lớn mẫu là `S`. **Xanh cả hôm
    nay** — đây là phần canh cho "standard phải ngủ", không phải phần chứng minh lỗi.
- **Đảo ngược:** (a) đặt lại `continue` trong nhánh `At` → test đầu đỏ với câu trên; (b) xoá
  nhánh idle thứ ba mới → test CPU đỏ (dự kiến ~100 % một core, như lần đo 2026-09-13 của test
  handshake: 0,00 % → 99,90 %). Ghi câu FAIL trước.
- **Gate:**
  `cargo test -p fixbolt-engine --test reconnect_wire --test reconnect --test shutdown --test originate`,
  `cargo test -p fixbolt-engine --tests --features tls --test tls_initiator_wire` (test handshake
  vẫn xanh),
  `cargo test --no-default-features -p fixbolt-engine`,
  `cargo clippy -p fixbolt-engine --all-targets -- -D warnings`,
  `cargo bench -p fixbolt-engine --bench alloc`, `cargo bench -p fixbolt-engine --bench dispatch`,
  **trên Linux, cả hai chế độ:** `scripts/check-standard-gives-the-core-back.sh`,
  `scripts/check-no-kernel-sleep.sh`, `scripts/check-no-kernel-sleep-by-ctxt.sh`.
- **Xong khi:** hai test xanh, hai đảo ngược đã chạy, ba script trích nguyên văn và mỗi script tự
  báo nửa đỏ của nó (sai chế độ phải làm nó đỏ).
- **Tài liệu:** `DESIGN.md` §4 D8 hoặc dòng `reconnect` ở §3 (một câu: vòng chờ quay số hỏi
  `shutdown` mỗi lần thức, trễ ≤ một timeout `Block`); `docs/GUIDE.md` (người gọi `Admin::shutdown`
  trên initiator đang chờ quay số: trả về trong ~100 ms); `CHANGELOG.md` *Fixed*;
  `docs/reference/a-dial-in-its-reconnect-wait-hears-shutdown-only-when-the-timer-fires.md` — **nếu
  trang đó đã có trên `main`** khi hàng này commit, thêm mục `[fixed <ngày>]` nêu test canh; nếu
  chưa, manager ghi việc đó vào *Open items* của `STATUS.md`.

### Hàng D3 — TLS bị từ chối: alert + event

- **Test đỏ trước:**
  - `tests/tls.rs::a_client_with_no_suite_in_common_is_sent_a_handshake_failure_alert`: acceptor
    `Handshake` như `a_peer_that_hangs_up_mid_handshake_is_a_failure_and_not_a_hang`
    (`tests/tls.rs:262-296`); client `rustls::ClientConnection` với provider chỉ còn
    `TLS13_AES_256_GCM_SHA384`. Bơm acceptor tới khi xong; client đọc socket →
    `process_new_packets()` phải là `Err(rustls::Error::AlertReceived(AlertDescription::HandshakeFailure))`.
    **Đỏ hôm nay**: client chỉ thấy EOF, câu FAIL *"the acceptor closed without a TLS alert"*.
    Khẳng định thêm: bước cuối của acceptor là `Step::Refused`.
  - `tests/tls.rs::a_peer_that_leaves_mid_handshake_is_not_a_refusal`: như test hang-up có sẵn,
    thêm khẳng định `transport.handshake_refused() == false`; và test trên có
    `handshake_refused() == true` ở mức `TlsTransport`.
  - `tests/tls_wire.rs::a_refused_handshake_is_an_event_not_silence`: `serve_tls` + `Handles`,
    client như trên, `wait_for_event(… TlsHandshakeRefused { count: 1 } …)` (helper
    `tls_wire.rs:194`). **Đỏ hôm nay** — để đỏ ở lúc chạy chứ không phải lỗi biên dịch, worker
    thêm variant `EventKind::TlsHandshakeRefused` **trước**, chưa phát ở đâu, rồi chạy test.
  - `tests/tls_initiator_wire.rs::an_initiator_refused_by_its_venue_says_so`: server rustls chỉ
    có `TLS13_AES_256_GCM_SHA384`, `connect_and_serve_tls` với `Handles`, chờ event
    `TlsHandshakeRefused`. **Đỏ hôm nay** cùng cách.
- **Đảo ngược:** (a) bỏ vòng gọi lại sau `Err` → test alert đỏ; (b) đếm vào `gone` thay vì
  `tls_refused` → test event đỏ; (c) cho mọi `Failed` trả `handshake_refused() = true` → test
  "peer bỏ đi" đỏ. Ghi câu FAIL trước.
- **Gate:**
  `cargo test -p fixbolt-engine --tests --features tls --no-fail-fast 2>&1 | tee target/tls-tests.log`
  rồi `scripts/check-feature-gated-tests-ran.sh fixbolt-engine tls target/tls-tests.log`,
  `cargo test -p fixbolt-engine --test presession`,
  `cargo test --no-default-features -p fixbolt-engine`, `scripts/check-no-optional-deps.sh`,
  `cargo clippy --all-targets --features tls -- -D warnings`,
  `cargo bench -p fixbolt-engine --bench alloc`,
  trên Linux: `scripts/check-standard-gives-the-core-back.sh` (có nhánh `--tls ktls`).
- **Xong khi:** bốn test xanh, ba đảo ngược đã chạy, test TLS cũ xanh **không sửa**, script
  feature-gated báo mọi test TLS đã chạy.
- **Tài liệu:** `DESIGN.md` §4 D11 (alert + event khi từ chối handshake) và phần API công khai
  (`Transport::handshake_refused`, `Step::Refused`, `Progress::tls_refused`,
  `EventKind::TlsHandshakeRefused`); rustdoc bốn thứ đó; `CHANGELOG.md` *Added* (bốn thứ, ghi rõ
  `Progress` và `Step` không `#[non_exhaustive]` nên struct literal / `match` đủ nhánh của người
  dùng sẽ vỡ) + *Fixed* (alert); `docs/GUIDE.md` (event mới; health check không bị đếm);
  `docs/internals/engine.md` (dòng `tls.rs`/`presession.rs` nếu có liệt kê kết quả); trang reference
  `a-tls-handshake-with-no-common-suite-closes-without-a-word.md` — xử lý như trang của D2;
  ADR-0151 → *Accepted*.

## Cách kiểm chứng

Mỗi hàng: test đỏ trích nguyên văn trước khi sửa, xanh sau khi sửa, đảo ngược đã chạy, gate
trích nguyên văn. Manager chạy lại gate của hàng trên commit của hàng đó.

**Hàng đóng (C)**, trên commit cuối, trích nguyên văn:

- `cargo test --all` và `cargo test --no-default-features` (`CLAUDE.md` §7, mọi commit);
- `cargo test -p fixbolt-conformance` — 59/59, và với `--features fix50sp2` theo
  `docs/CONFORMANCE.md` §9 (codec đổi hành vi công khai);
- `cargo clippy --all-targets -- -D warnings` và với `--features tls`;
- `scripts/bench.sh` (không `--strict`: không có case timing mới, không dòng baseline mới);
- trên Linux, cả hai chế độ: `scripts/check-no-kernel-sleep.sh`,
  `scripts/check-no-kernel-sleep-by-ctxt.sh`, `scripts/check-standard-gives-the-core-back.sh` —
  mỗi script hai lần, không và có `W2W_EXTRA="--journal file-async --log file"` (D5);
- `python3 scripts/check-links.py`;
- CI xanh trên commit đóng, **ghi run id**.

**Ngoài test, chạy thật:** nếu nhánh `plan/p3-quickfixj-interop` đã vào `main`, chạy lại
`INTEROP_QFJ_ARMS=acceptor-tls INTEROP_QFJ_CIPHER=TLS_AES_256_GCM_SHA384 scripts/interop-qfj.sh`
và trích: log QuickFIX/J phải nêu alert `handshake_failure` thay cho `END_OF_STREAM`, log fixbolt
phải có dòng event `TlsHandshakeRefused`. Nếu chưa vào, ghi rõ là **chưa kiểm với client thật
ngoài rustls** ở *Nhật ký giao hàng*.

## Tài liệu phải cập nhật

Đi theo bảng `CLAUDE.md` §4, từng dòng:

- [ ] API công khai đổi (D3: bốn thứ mới; D4: hành vi `as_i64`) → `DESIGN.md`, rustdoc,
      `CHANGELOG.md`
- [ ] Ràng buộc người dùng phải giữ mà compiler không kiểm (D1: giới hạn 65 535; D3: health check
      không bị đếm) → `docs/GUIDE.md`
- [ ] Hằng số / giới hạn người dùng thấy (D1; D5: 1 024 lần / 1 ms là hằng số trong code, ghi
      trong `CONFIGURATION.md` như `ORIGIN_LEN` — "not configurable") → `docs/CONFIGURATION.md`
- [ ] Khuyến nghị vận hành theo chế độ (D5) → `docs/best-practices-standard.md`,
      `docs/best-practices-hft.md`
- [ ] Chứng minh một mục *Not proven* (D5: script chế độ chạy có thread ghi) → gạch trong
      `STATUS.md`, cùng commit
- [ ] Hành vi transport / engine (D2, D3) → `DESIGN.md` §4, và đi lại §2
- [ ] Bẫy, giả định sai (D1; D3 là hành vi rustls không có trong tài liệu) → `docs/reference/`
- [ ] Quyết định kỹ thuật → ADR-0150, ADR-0151 (chuyển *Accepted* khi plan được duyệt)
- [ ] `STATUS.md` — manager viết ở hàng C, không worker nào sửa
- [ ] Hành vi ranh giới session (`SESSION-BEHAVIOUR.md`): **không** — không lỗi nào đổi reject,
      resend, gap fill hay `DropReason`
- [ ] Số hiệu năng / latency budget: **không** — không số mới nào được công bố

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| D1: sửa bộ đệm xong thì `Some(0)` không còn chạm được, nên nhánh dừng sai có thể quay lại mà không test nào thấy | `writer_tests::a_record_the_writer_cannot_hold_does_not_stop_it` (gọi thẳng `write_loop` với bộ đệm nhỏ) |
| D1: `close()` quay vòng `while !p.push(..)` — nếu thread ghi đã chết và ring đầy, `close` treo mãi (`docs/reference/a-reversal-can-fail-by-hanging.md`) | test đầu của D1 đóng journal sau khi ghi; khi đảo ngược, chạy với `timeout 120` để một lần treo là đỏ có câu, không phải treo CI |
| D1: bản ghi `STOP` 1 byte bị nhầm là bản ghi thật | bản ghi thật luôn ≥ 8 byte; `writer_tests` có một bản ghi hợp lệ sau bản ghi quá dài |
| D2: thêm `turn()` vào lúc chờ làm message gửi trong lúc mất kết nối bị báo `OriginationUndeliverable` sớm hơn trước | `cargo test --test originate` xanh **không sửa** |
| D2: sửa cho nghe `shutdown` nhưng vô tình làm vòng chờ quay tít (`standard` spin) | `the_dial_loop_sleeps_rather_than_spins_while_it_waits_to_reconnect` + đảo ngược (b) |
| D2: test đỏ bằng cách treo | `recv_timeout(5 s)` — đỏ có câu sau 5 s |
| D3: gọi lại `process_tls_records` mà không áp `discard` trước (rustls bắt buộc) | test alert; rustls sẽ trả lỗi khác hoặc alert sai |
| D3: vòng gọi lại không có giới hạn → quay tít trên socket hỏng | giới hạn 4 lần trong code; `a_peer_that_hangs_up_mid_handshake_is_a_failure_and_not_a_hang` vẫn xanh |
| D3: health check (kết nối rồi đóng) bị đếm là từ chối | `a_peer_that_leaves_mid_handshake_is_not_a_refusal` |
| D3: test TLS mới nằm trong file không ai liệt kê → CI không chạy | `scripts/check-feature-gated-tests-ran.sh` trong gate |
| D5: test tìm nhầm thread ghi của test khác chạy song song | file test riêng `writer_idle.rs` (một tiến trình), và mỗi test trong đó chạy tuần tự (một `Mutex` tĩnh hoặc một test duy nhất gồm các phần) |
| D5: "CPU thấp" qua được khi thread ghi đã **chết** (thread chết không tốn CPU) | test đọc được `stat` của tid trong suốt cửa sổ (`alive > 0`), như test handshake; test 3 chứng minh thread còn ghi |
| D5: ngủ lâu làm ring đầy trong một đợt dồn → message không xuống đĩa | giới hạn 1 ms ↔ ring 1 MiB (ADR-0150 quyết định 4, `[derived]`); test 3 dồn 1 000 message ngay sau khi ngủ |
| D4: test mới không khớp filter → `cargo test` thoát 0 mà không chạy gì | dùng `--test int`, và trích dòng `running N tests` với N ≥ 3 |
| D4: comment `tests/decimal.rs:161` ("`as_i64` takes a `+`") thành sai | worker **không** sửa file test đó trong bước sửa code (test cũ không được đụng); manager giao một sửa comment riêng, chỉ comment, sau khi D4 xanh |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| D3 dựa trên hành vi rustls **không có trong tài liệu** (gọi lại sau `Err` thì nhận alert) | trung bình | test alert là người gác; nó chạy trong job `tls` của CI mỗi lần nâng rustls. Nếu test không đỏ/xanh như dự tính → dừng, trả về architect |
| D4 phá người dùng đang dựa vào `+` | thấp | không ai trong repo dựa vào; `CHANGELOG.md` ghi là thay đổi phá vỡ; đặc tả FIX không cho `+` |
| D3 phá người dùng dựng `Progress` bằng struct literal hoặc `match` đủ nhánh trên `Step` | thấp | ghi trong `CHANGELOG.md`; không `#[non_exhaustive]` hoá hai kiểu đó trong plan này (đó là quyết định API riêng) |
| D2: thêm `turn()` mỗi 100 ms khi đang chờ làm engine tốn thêm CPU | thấp | vẫn là một turn rỗng mỗi timeout, như hôm nay lúc đang handshake; test CPU canh |
| Máy desk đang bị phiên khác đo → không chạy được script chế độ trên Linux | trung bình | theo luật dừng của `CLAUDE.md` §12: chờ máy rảnh, không chạy chồng |
| D5: hằng số 1 024 lần / 1 ms là suy ra, chưa đo; một deployment `hft` có thể muốn thread ghi quay tít | thấp | ADR-0150 *Options not taken* ghi điều kiện mở lại (một phép đo cho thấy thiệt); không công bố số nào |

## Ngoài phạm vi

- Thread `RingDispatch` phía ứng dụng: nó do ứng dụng chạy (`pump` "never blocks", ứng dụng tự
  chọn cách chờ, `dispatch.rs:375-378`), không phải thread của crate này quay tít.
- Đổi `ring::Consumer::pop` sang enum ba trạng thái (ADR-0150 *Options not taken*).
- Đếm message không xuống đĩa vì ring đầy (`journal.rs:850-855`) — một đánh đổi đã ghi từ trước,
  không phải lỗi này.
- Lý do cụ thể của từng lần từ chối TLS trong event; alert cho lỗi **sau** handshake (đường
  userspace `Traffic`); cửa TLS cho shard acceptor.
- Trả lại thời gian chờ 10 s cho `scripts/interop-qfj.sh` (`run_initiator` đang chờ 40 s vì D2) —
  file đó nằm trên nhánh khác; khi cả hai đã vào `main`, đó là một sửa nhỏ riêng.
- `#[non_exhaustive]` cho `Progress` và `Step`.

## Sửa 2 — script `check-no-kernel-sleep.sh` đỏ khi có thread ghi (2026-09-23)

**Chuyện gì xảy ra.** Builder của D5 (commit `bc98fcb`, nhánh `fix/d1-journal`) chạy ba script
chế độ **có** thread ghi (`W2W_EXTRA="--journal file-async --log file"`), đúng như hàng D5 đòi.
`check-no-kernel-sleep-by-ctxt.sh` xanh (0 lần tự nhường CPU), `check-standard-gives-the-core-back.sh`
xanh, nhưng `check-no-kernel-sleep.sh` **đỏ**: `FAIL: the engine thread slept in the kernel:  2 futex`
(cả với `--tls ktls`). Hai lần `futex` đó nằm **sau** lần gọi socket cuối cùng của engine thread,
mỗi lần theo sau là `munmap` đúng cỡ một ring (1 052 672 B của journal, 4 198 400 B của message
log): engine thread đang **dọn dẹp lúc tắt** — huỷ `FileJournal`/`FileLog`, và `close()` của chúng
chờ thread ghi xong (`join`). Binary trước D5 (`016f2a5`) cũng đọc đúng `2 futex` 5/5 lần →
**có từ trước**, chỉ chưa ai thấy vì chưa script nào chạy với thread ghi.

**Vì sao hai script cho hai kết quả** (đọc header của cả hai): `check-no-kernel-sleep.sh` đếm
**mọi** syscall engine thread từng làm, từ lúc sinh tới lúc chết (hàm `engine_syscalls`, dòng
129-155) — cả khởi động, phục vụ, lẫn dọn dẹp. `-by-ctxt.sh` thì **đã có cửa sổ** từ đầu: w2w
đọc bộ đếm ngay trước và ngay sau vòng đo (ADR-0072 quyết định 1).

**Quyết định** — phương án (c) + (a), ghi trong
[ADR-0152](../decisions/ADR-0152-non-negotiable-4-judges-the-engine-threads-serving-window-and-its-teardown-may-wait-for-its-writers.md)
(**Proposed**, vì nó nói rõ điều 4 bao phủ tới đâu):

1. Điều 4 ("không ngủ trong kernel **trên hot path**") áp cho engine thread **từ lúc vào vòng
   phục vụ tới lúc vòng đó trả về**. Khởi động trước đó và dọn dẹp sau đó được phép chặn; dọn
   dẹp **phải** chờ thread ghi xả hết, nếu không message đã nhận sẽ không xuống đĩa.
2. `tools/w2w` đánh dấu cửa sổ bằng hai syscall không gì khác làm: tra metadata của hai đường dẫn
   không tồn tại, `fixbolt-w2w-serve-open` ngay trước vòng phục vụ và `fixbolt-w2w-serve-close`
   ngay sau khi vòng trả về, **trước khi huỷ bất cứ thứ gì** (`drop` tường minh sau dấu thứ hai).
3. Script chỉ đếm `SLEEPERS` và lời gọi socket **giữa hai dấu** trên tid engine; khớp theo
   **chuỗi đường dẫn**, không theo tên syscall (`statx` hay `newfstatat` tuỳ libc).
4. Sleeper ngoài cửa sổ được **in ra**, không giấu: *"outside the serving window, not judged: …"*.
5. Thiếu dấu hoặc dấu xuất hiện hai lần → **đỏ**: *"FAIL: the serving window is not marked exactly
   once — nothing can be judged"*.
6. **Không** cắt ở lần gọi socket cuối: cửa sổ định nghĩa bằng chính thứ nó đo sẽ bỏ sót một lần
   ngủ nằm trong vòng nhưng sau message cuối.

Loại: (b) tách rời thread ghi — mất message đã nhận khi tiến trình thoát; (b) join ở thread khác —
chỉ dời chỗ chờ, thêm một thread thư viện phải giữ, không được gì.

**Hàng mới: W (cửa sổ phục vụ)**

| Hàng | Kết quả | Người làm | Chạm vào | Không chạm | Phụ thuộc |
|---|---|---|---|---|---|
| W | `check-no-kernel-sleep.sh` chỉ phán trong cửa sổ phục vụ, in phần ngoài cửa sổ; w2w đánh dấu cửa sổ ở mọi chế độ và nhánh TLS | senior developer (`opus`) — gate của điều 4; sai thì gate nói dối | `tools/w2w/src/main.rs` (hai dấu + `drop` tường minh, ở mọi đường engine thread chạy vòng phục vụ), `scripts/check-no-kernel-sleep.sh` | `crates/`, `-by-ctxt.sh`, `check-standard-gives-the-core-back.sh`, CI | 0; **chạy song song với D5 được** (file rời). D5 chỉ đóng sau khi W đã commit |

- **Đỏ trước:** trên commit hiện tại, `W2W_EXTRA="--journal file-async --log file" scripts/check-no-kernel-sleep.sh`
  đỏ với `2 futex` (đã có, trích lại nguyên văn).
- **Gate:** `cargo build -p fixbolt-w2w --release --features tls`; script chạy **ba lần** và trích
  nguyên văn: không `W2W_EXTRA` (xanh, như CI), với `W2W_EXTRA="--journal file-async --log file"`
  (xanh, và **phải in** dòng `outside the serving window, not judged:` có `futex`), và nửa đỏ
  `--mode standard` vẫn đỏ **trong** cửa sổ ở cả hai lần; `bash -n` và
  `shellcheck -S info scripts/check-no-kernel-sleep.sh`; `cargo clippy -p fixbolt-w2w --all-targets --features tls -- -D warnings`;
  `scripts/check-no-kernel-sleep-by-ctxt.sh` và `scripts/check-standard-gives-the-core-back.sh`
  vẫn xanh (w2w đổi).
- **Đảo ngược** (ghi câu FAIL trước): (1) dời dấu `serve-close` xuống **sau** `drop` → đỏ
  `2 futex` — chứng minh đúng hai lần chờ lúc dọn dẹp, và chỉ chúng, bị loại khỏi phán xét;
  (2) xoá dấu `serve-open` → đỏ *"not marked exactly once"*; (3) nửa đỏ `--mode standard` có sẵn
  là đảo ngược cho "sleeper trong cửa sổ vẫn bị bắt".
- **Xong khi:** ba lần chạy và ba đảo ngược trích nguyên văn; D5 chạy lại gate chế độ của nó trên
  commit có W và xanh cả ba script.
- **Tài liệu (cùng commit):** header của script (một đoạn `[2026-09-23]` nói cửa sổ là gì, và nó
  không thấy gì: khởi động không còn bị phán); `tools/w2w` rustdoc đầu file (hai dấu); `DESIGN.md`
  §4 D8 (điều 4 phủ vòng phục vụ; dọn dẹp chờ thread ghi); `docs/reference/a-teardown-join-read-as-a-hot-path-sleep.md`
  (mới — bẫy: gate đếm cả vòng đời thread); ADR-0152 → *Accepted*. **`CLAUDE.md` §2 bảng Machine
  checks, dòng điều 4**: thêm ghi chú "the strace check judges the serving window; teardown is
  printed, not judged" — file luật, **manager sửa**, nói rõ đã sửa luật nào.

**Thay đổi ở hàng C:** các lệnh chế độ có `W2W_EXTRA` chỉ được coi là xanh khi đã có W.

## Sửa 3 — engine thread ngủ **giữa lúc phục vụ** khi một phiên có journal kết thúc (2026-09-23)

**Chuyện gì xảy ra.** Hàng W đã dựng đúng ADR-0152 (worktree `fb-w`, nhánh
`fix/w-serving-window`, chưa commit; các đảo ngược đỏ như dự tính). Nhưng với thread ghi, script
đỏ **2/8** lần, và **3/12** lần trace tay đọc `inside=1 outside=1`. Chỉ `futex` của message log là
lúc dọn dẹp. `futex` của **journal** xảy ra **khi một kết nối đóng, bên trong `turn()`**:
`self.conns.swap_remove(i)` (`crates/engine/src/lib.rs:1351`) huỷ `FileJournal` của kết nối đó →
`close()` → `join` thread ghi (`journal.rs:770-788`). Thứ tự trên tid engine: `recvfrom = 0`,
`close(7)`, `futex`, `munmap(…, 1052672)`, rồi mới tới dấu `serve-close`. Rơi trong hay ngoài cửa
sổ là do EOF hay lệnh dừng tới trước.

**Nghĩa là:** engine `hft` **thật sự ngủ trong kernel giữa lúc phục vụ** mỗi khi một phiên có
`FileJournal` kết thúc — vi phạm điều 4 thật, có từ trước D5 (D5 có thể làm nó dài thêm tới
~1 ms vì thread ghi có thể đang ngủ). Ở `standard`, đó là một lần kẹt tới ~1 ms cho mọi phiên khác
trên cùng thread. **Tiền đề của ADR-0152 quyết định 1 sai** ("join chỉ xảy ra lúc dọn dẹp"); cơ chế
cửa sổ thì đúng — chính nó bắt được lỗi này.

**Quyết định** — [ADR-0153](../decisions/ADR-0153-a-connections-journal-is-retired-without-waiting-and-its-writer-is-awaited-only-after-serving.md)
(**Proposed**; khi được chấp nhận thì thay phần "join chỉ lúc dọn dẹp" của ADR-0152, giữ nguyên
cơ chế cửa sổ). ADR-0152 đã *Accepted* nên không sửa nội dung, chỉ thêm một dòng trạng thái trỏ
sang ADR-0153 (`CLAUDE.md` §5).

1. **Engine không bao giờ chờ thread ghi của journal trong lúc phục vụ, ở cả hai chế độ.** Journal
   của kết nối rời đi được **cho nghỉ** (`retire`), không đóng.
2. Trait `fixbolt_session::journal::Journal` thêm `fn retire(&mut self) {}` (mặc định không làm
   gì — session vẫn thuần). `Connection` (`crates/engine/src/conn.rs:49`) có `Drop` gọi
   `self.journal.retire()` → **mọi** đường huỷ kết nối (`swap_remove`, `clear` ở `lib.rs:1065`,
   huỷ engine) đều cho nghỉ trước, không chỗ nào quên được.
3. `FileJournal::retire`: không syscall, không quay vòng — đẩy `STOP` **một lần**; ring đầy thì bật
   cờ dừng mà thread ghi đọc khi ring cạn; lấy `JoinHandle` ra và thả (tách rời); tăng bộ đếm toàn
   tiến trình "thread ghi đã cho nghỉ, chưa xong". Thread ghi, sau lần flush cuối, giảm bộ đếm như
   việc cuối cùng. `Drop` sau đó không còn handle để `join`.
4. **Chỗ chờ dời ra sau vòng phục vụ:** `fixbolt_engine::journal::wait_for_retired_writers(timeout)
   -> bool` — ngủ 1 ms giữa các lần nhìn bộ đếm tới khi về 0 hoặc hết giờ. Mọi hàm chạy vòng phục
   vụ (`serve*`, `connect_and_serve*`, serve của shard) gọi nó **sau khi vòng trả về** (dọn dẹp,
   ADR-0152 cho phép), timeout = thời gian ân hạn lúc tắt. Ai tự lái `Engine` phải tự gọi —
   `GUIDE.md` ghi.
5. `FileJournal::close()` và `Drop` của journal **chưa** cho nghỉ vẫn `join` như cũ (test, tool,
   người gọi tự đóng journal của mình).

Loại (chi tiết ở ADR-0153): đánh thức thread ghi ngay khi `STOP` (vẫn là một lần `futex`, ngắn
không phải là không); thread dọn dẹp nhận qua channel (`mpsc` cấp phát và `futex` wake); tách rời
không có chỗ chờ (mất dữ liệu khi thoát sạch); cho mọi `Drop` không chặn (đổi nghĩa `Drop` với mọi
chủ sở hữu, test hiện có đọc file ngay sau `drop`). Nguồn: Chronicle Core
`BackgroundResourceReleaser`, Aeron `FREE_LOG_BUFFER`, tài liệu `JoinHandle` của Rust — trích ở
ADR-0153 *Context* §3.

**Hàng mới: J (cho nghỉ journal, không chờ)**

| Hàng | Kết quả | Người làm | Chạm vào | Không chạm | Phụ thuộc |
|---|---|---|---|---|---|
| J | engine không `futex` khi một phiên có `FileJournal` kết thúc giữa lúc phục vụ; dữ liệu vẫn xuống đĩa trước khi serve trả về | senior developer (`opus`) — engine thread, điều 1/2/3/4, **cùng worker D1/D5** nếu còn (giữ context `journal.rs`) | `crates/session/src/journal.rs` (chỉ thêm `retire` mặc định + rustdoc), `crates/engine/src/journal.rs`, `crates/engine/src/conn.rs` (`Drop`), `crates/engine/src/lib.rs` (chỉ lời gọi `wait_for_retired_writers` sau vòng của các hàm serve), `crates/engine/src/shard.rs` (cùng lời gọi), `crates/engine/tests/retire.rs` (mới) | `tools/`, `scripts/` (của W), `ring.rs`, `msglog.rs`, `crates/codec/` | nền là commit D5 (`bc98fcb`); `lib.rs` **sau khi D2 và D3 đã commit** (một file, một người viết) |

- **Test đỏ trước** (`crates/engine/tests/retire.rs`, `#[cfg(target_os = "linux")]`):
  - `a_session_with_a_file_journal_ends_without_the_engine_thread_waiting`: engine `hft`
    (`wait::Spin`) trên `transport::Loopback`, 20 kết nối lần lượt, mỗi cái có `FileJournal`
    `Async` riêng, logon rồi phía kia đóng; đọc `voluntary_ctxt_switches` của **chính thread chạy
    turn** (`/proc/thread-self/status`) trước và sau đoạn các phiên kết thúc; khẳng định **0**.
    **Đỏ hôm nay**: mỗi `join` là một lần tự nhường, câu FAIL *"the engine thread made N voluntary
    switches while sessions with a FileJournal ended — a writer join on the serving path"*. Khẳng
    định kèm: số kết nối về 0 (đoạn đo thật sự có phiên kết thúc).
  - Cùng test với engine `standard` (`Block`), khẳng định số lần tự nhường **không tăng thêm** so
    với cùng kịch bản dùng `MemJournal` (đo cả hai trong test) — `standard` được ngủ khi rảnh,
    nhưng không được chờ thread ghi.
  - `a_retired_journal_reaches_the_disk_before_the_wait_returns`: `put` 10 000 message,
    `retire`, `wait_for_retired_writers(5 s) == true`, mở lại file → đủ 10 000.
  - `a_journal_closed_by_its_owner_still_joins`: `close()` trên journal chưa cho nghỉ vẫn chờ xong
    (test `async_reaches_the_disk_once_the_writer_has_caught_up` hiện có cũng canh — không sửa).
- **Đảo ngược** (ghi câu FAIL trước): (1) `FileJournal::retire` để trống (dùng mặc định) → test 1
  đỏ; (2) `wait_for_retired_writers` trả `true` ngay → test 3 đỏ (thiếu message); (3) bỏ `Drop`
  của `Connection` → test 1 đỏ.
- **Gate:** `cargo test -p fixbolt-engine --test retire --test journal --test on_disk --test secrets_stay_off_disk --test shutdown --test engine_recovery --test shard_recovery`;
  `cargo test -p fixbolt-session` và `cargo test -p fixbolt-conformance` (59/59 — trait session
  đổi, điều 3); `cargo test --no-default-features`; `cargo clippy --all-targets -- -D warnings`
  và `--features tls`; `cargo bench -p fixbolt-engine --bench alloc` (điều 1: `retire` không cấp
  phát); **cả hai chế độ, trên Linux, trên commit có cả W và J**:
  `W2W_EXTRA="--journal file-async --log file" scripts/check-no-kernel-sleep.sh` **10/10 lần
  xanh** (trích từng lần; dòng *outside the serving window* chỉ còn `futex` của message log),
  cùng `W2W_EXTRA` với `scripts/check-no-kernel-sleep-by-ctxt.sh` và
  `scripts/check-standard-gives-the-core-back.sh`.
- **Xong khi:** bốn test xanh, ba đảo ngược đỏ đúng câu, 10/10 script, test cũ xanh **không sửa**.
- **Tài liệu (cùng commit):** `DESIGN.md` §4 D7 (journal của phiên rời đi được cho nghỉ, chờ ở
  dọn dẹp) và D8 (điều 4 giữ khi phiên kết thúc); `docs/GUIDE.md` (ai tự lái `Engine` phải gọi
  `wait_for_retired_writers` trước khi thoát); `CHANGELOG.md` *Added* (`Journal::retire`,
  `wait_for_retired_writers`) + *Fixed*; `docs/reference/a-connection-end-joined-its-journal-writer-on-the-engine-thread.md`
  (mới — bẫy: ADR-0152 tưởng join chỉ lúc dọn dẹp; cửa sổ bắt được); `docs/internals/engine.md`
  (`conn.rs` có `Drop`); ADR-0153 → *Accepted*.

**Thứ tự đóng:** W **chỉ đóng sau J** — gate 10/10 của W là gate của J. D5 vẫn đóng sau W. Hàng C
chạy lại mọi script chế độ với `W2W_EXTRA` trên commit cuối.

## Sửa 4 — 2026-09-23

- Sau vòng phục vụ, engine chờ các thread ghi journal đã cho nghỉ, tối đa bằng thời gian ân hạn lúc tắt nhưng **không dưới 1 giây** (`RETIRED_WRITERS_FLOOR_MS`, `crates/engine/src/lib.rs`).
- Lý do: `Admin::shutdown(0)` chỉ muốn cắt phía đối tác ngay, chứ không muốn journal của chính mình bị cắt ngang. Shard và các đường thoát vì lỗi thì không có thời gian ân hạn nào. Chờ 0 giây sẽ mất dữ liệu `Async` khi thoát sạch — đúng cái ADR-0153 đã loại.
- Hàm chờ trả về ngay khi các thread ghi xong, nên mức sàn không tốn gì khi chúng nhanh. Con số 1 giây chưa đo (`[unmeasured]`); manager chấp nhận ngày 2026-09-24.

## Sửa 5 — review cấp cao PR #103 (HEAD `0f1f8a5`), 2026-09-24

Một phát hiện **chặn** (thiết kế của ADR-0153 thiếu một trường hợp), hai **nên sửa**, bốn **ghi
chú**. Quyết định nằm ở
[ADR-0154](../decisions/ADR-0154-a-journal-file-has-one-appender-a-reconnect-waits-for-it-by-parking-and-what-the-file-missed-is-counted.md)
(**Proposed**; bổ sung ADR-0153, không đổi quyết định nào của nó).

### 1. (Chặn) Kết nối lại mở lại file khi thread ghi cũ chưa xong

**Đã xác nhận bằng đọc code + probe của reviewer:** thread ghi đã cho nghỉ còn đang flush (~1 ms,
vì có thể đang ngủ) thì một `FileJournal::open` mới trên cùng file đọc `highest_out` cũ
(*"reopen saw highest_out=Some(1), wanted Some(2)"*, 50/50) và hai thread ghi cùng nối vào một
file — bản ghi và CRC là hai lần `write_all` (`journal.rs:988`, `:994`) nên có thể xen nhau. Đối
tác logon lại trong ~1 ms sẽ nhận `MsgSeqNum` đã thấy rồi. Trước J, `join` lúc `drop` chặn được
chuyện này — bằng cách bắt engine thread chờ.

**Chỗ mở lại file:** trong `Recovery` do deployment viết. Engine gọi nó ở `pump` (`serve*` một
thread) **ngay trong vòng quay engine** (`lib.rs:3446`) — tức là engine thread, giữa lúc phục vụ,
dù comment ở đó và rustdoc của `Recovery::recover` nói "acceptor thread"; ở `dial` (`lib.rs:2714`,
chỉ `standard`, lúc engine không giữ kết nối nào); và ở shard, trên acceptor thread (được chặn,
ADR-0088). Nên cách sửa **không được chờ** trên engine thread.

**Quyết định (ADR-0154 quyết định 1–3):**

- `FileJournal::open` lấy khoá độc quyền `File::try_lock` (`flock` `LOCK_EX|LOCK_NB`) **trước khi
  đọc file**; khoá sống cùng `File` — của thread ghi (`Async`, nhả khi nó đóng file sau lần flush
  cuối) hoặc của journal (`Fsync`). Đang bị giữ → `open` **trả lỗi ngay** `WouldBlock`, không chờ.
  Chặn luôn cả hai **tiến trình** cùng ghi một file (hiểm hoạ có sẵn). `rust-version` 1.85 → 1.89.
- `Recovery` thêm `fn ready(&mut self, cfg) -> bool { true }`; `journal::file_busy(path)` trả lời
  mà không chặn. Engine hỏi `ready` trước `recover`.
- Chưa sẵn sàng → kết nối được **để chờ tại chỗ** (trong tập pre-session ở `pump`, trong ô
  handshake ở `dial`, cùng luật ở shard), hỏi lại **tối đa mỗi 1 ms** theo đồng hồ engine; không
  tính là "có tiến triển" nên `standard` vẫn ngủ khi rảnh, `hft` vẫn không ngủ. Quá
  `LogonTimeout` → bỏ, đếm là timed out.
- Loại: `open` chờ khoá (ngủ trên engine thread); danh sách đường dẫn trong tiến trình (không thấy
  tiến trình khác, cần `Mutex`); trao journal cũ cho lần mở sau (cấp phát + xoá kiểu trên engine
  thread); khoá theo từng bản ghi kiểu Chronicle Queue (khoá cả vòng đời file đã đủ). Nguồn:
  tài liệu `File::try_lock` của Rust, Chronicle Queue `TableStoreWriteLock` — ở ADR-0154 §4.

**Hàng K (một người ghi mỗi file)** — senior developer (`opus`), **cùng worker J** nếu còn.

| Chạm vào | Không chạm | Phụ thuộc |
|---|---|---|
| `crates/engine/src/journal.rs` (khoá, `file_busy`), `crates/engine/src/recovery.rs` (`ready` + sửa rustdoc "acceptor thread"), `crates/engine/src/lib.rs` (`pump`, `dial`: để chờ + hỏi lại theo nhịp; sửa comment `:3440-3445`), `crates/engine/src/shard.rs` (cùng luật), `tools/interop/src/reconnect.rs` (cài `ready`), `Cargo.toml` (`rust-version`), `crates/engine/tests/one_appender.rs` (mới) | `crates/session/`, `ring.rs`, `msglog.rs`, `scripts/` | commit J (`4bb69c9`) |

- **Test đỏ trước** (`tests/one_appender.rs`):
  - `a_journal_file_has_one_appender`: `FileJournal` `Async`, `put`, `mark_out(2)`, `retire`,
    `drop`, mở lại ngay → **hôm nay `Ok`** (đỏ: *"a second appender opened the file while the
    first had not finished"*); sau sửa: `Err(WouldBlock)`; sau `wait_for_retired_writers` thì mở
    được và `highest_out() == Some(2)`.
  - `a_reconnect_resumes_from_the_finished_file`: `serve_with_recovery` với recovery dùng
    `FileJournal` + `ready` qua `file_busy`; 50 lần: phiên gửi, phía kia ngắt, logon lại **ngay**
    → `MsgSeqNum` đầu tiên của phiên mới luôn = số cuối + 1. **Hôm nay đỏ** (probe: 50/50 sai).
  - `a_parked_reconnect_costs_the_engine_thread_no_wait`: engine `hft` trên `Loopback`, một phiên
    khác đang chạy, một kết nối bị để chờ → `voluntary_ctxt_switches` của thread đó = **0**; phiên
    đang chạy vẫn được trả lời; kết nối để chờ quá `LogonTimeout` thì bị bỏ.
  - `a_second_process_cannot_append`: tiến trình con (`std::process::Command` chạy chính binary test
    với biến môi trường) mở cùng file → `WouldBlock`.
- **Đảo ngược:** (1) bỏ `try_lock` → test 1 và 4 đỏ; (2) `ready` luôn `true` → test 2 đỏ; (3) để
  chờ bằng `sleep` thay vì để tại chỗ → test 3 đỏ.
- **Gate:** `cargo test -p fixbolt-engine --test one_appender --test retire --test journal --test on_disk --test reconnect_wire --test shard_recovery --test engine_recovery`;
  `cargo test -p fixbolt-interop` (nếu có test); `cargo clippy --all-targets -- -D warnings` (bắt
  `incompatible_msrv`) và `--features tls`; `cargo test --no-default-features`;
  `cargo bench -p fixbolt-engine --bench alloc`; **cả hai chế độ** (cách chờ của `pump`/`dial` đổi):
  `W2W_EXTRA="--journal file-async --log file"` với `check-no-kernel-sleep.sh` (10/10),
  `check-no-kernel-sleep-by-ctxt.sh`, `check-standard-gives-the-core-back.sh`; nếu nhánh interop
  đã vào `main`, chạy arm reconnect của `scripts/interop-qfj.sh`.
- **Tài liệu:** `DESIGN.md` §4 D7; `docs/GUIDE.md` (recovery mở `FileJournal` phải cài `ready`;
  `WouldBlock` không có nghĩa "không có lịch sử"); `CHANGELOG.md` (`Recovery::ready`,
  `journal::file_busy`, lỗi mới của `open`, MSRV 1.89); `docs/reference/a-reconnect-reopened-a-journal-its-retired-writer-still-owned.md`
  (mới); ADR-0154 → *Accepted*.

### 2. (Nên sửa) Chưa test nào chứng minh các hàm serve chờ thread ghi sau vòng phục vụ

`after_serving` (`lib.rs:3370-3373`) làm thành không làm gì thì `cargo test -p fixbolt-engine
--features standard` vẫn 372/0. **Hàng M** — senior developer (`opus`), **song song** với K (chỉ
file test mới).

- Chạm vào: `crates/engine/tests/after_serving.rs` (mới). Không chạm: `crates/engine/src/`.
- **Test** (mỗi họ hàm serve một test, recovery với `fresh` mở `FileJournal` `Async`, ứng dụng
  echo 2 000 lệnh để journal có việc, rồi `Admin::shutdown(0)`):
  `serve_with_recovery_returns_after_its_writers_finished`,
  `connect_and_serve_with_recovery_returns_after_its_writers_finished`,
  `shard_serve_returns_after_its_writers_finished`. Ngay khi hàm trả về, **không** gọi gì thêm:
  `journal::wait_for_retired_writers(Duration::ZERO) == true` và file có đủ 2 000 bản ghi. Lặp 5
  lần mỗi test để lần đỏ không phụ thuộc may rủi.
- Hôm nay xanh (J đã có `after_serving`) — đây là **đảo ngược bắt buộc**: `after_serving` thành
  rỗng → cả ba đỏ, câu *"serve returned while N retired writers were still writing"*. Trích lần đỏ
  đó rồi trả lại.
- **Gate:** `cargo test -p fixbolt-engine --test after_serving` (trích `running 3 tests`),
  clippy.

### 3. (Nên sửa) Ring đầy → `put` vẫn trả `true`, mất im lặng

**Đã xác nhận bằng đọc code** (`journal.rs:~1031`, `let _ = p.push(…)`); probe: 40 × 60 KB, 28/40
xuống đĩa. **Hợp đồng của session** (`crates/session/src/lib.rs:3083`, `:3890`;
`crates/session/src/journal.rs:22-40`): `false` = "không giữ, mọi `ResendRequest` sau này sẽ gap
fill", đếm vào `puts_refused`. Ở đây bộ nhớ **vẫn giữ** message (`get` trả lời; resend trong lúc
chạy vẫn replay đúng) — chỉ file thiếu, chỉ hại sau khi khởi động lại.

**Quyết định (ADR-0154 quyết định 4):** `put` **vẫn trả `true`**; `FileJournal` đếm lần đẩy ring
bị từ chối; trait `Journal` thêm `fn unwritten(&self) -> u64 { 0 }`; engine báo phần tăng bằng
`EventKind::JournalUnwritten { count }`, cùng cách báo `JournalRefused` (`lib.rs:1306`).

**Hàng L** — senior developer (`opus`), **sau K, cùng worker** (chung `journal.rs`, `lib.rs`).

- Chạm vào: `crates/session/src/journal.rs` (chỉ `unwritten` mặc định + rustdoc),
  `crates/engine/src/journal.rs`, `crates/engine/src/observe.rs` (variant),
  `crates/engine/src/lib.rs` (phát event, cạnh `JournalRefused`), `crates/engine/tests/journal.rs`.
- **Test đỏ trước:** `a_full_ring_is_counted_not_silent`: thread ghi đang ngủ, 40 × 60 KB `put` →
  mọi `put` vẫn `true`, `unwritten() == 40 − (số trên đĩa)` và > 0. Để đỏ lúc chạy chứ không phải
  lỗi biên dịch: thêm method trả 0 trước, chạy → đỏ. Test engine
  `a_full_journal_ring_is_an_event`: event `JournalUnwritten` với đúng số đó.
- **Đảo ngược:** không tăng bộ đếm → hai test đỏ.
- **Gate:** `cargo test -p fixbolt-engine --test journal --test observe`; `cargo test -p fixbolt-session`
  và `cargo test -p fixbolt-conformance` (59/59 — trait session đổi); `cargo bench -p fixbolt-engine --bench alloc`
  (đếm không cấp phát); clippy; `--no-default-features`.
- **Tài liệu:** `DESIGN.md` §4 D7; `docs/GUIDE.md` §6a (con số tăng nghĩa là gì: đĩa chậm hoặc ring
  1 MiB nhỏ so với đợt dồn); `CHANGELOG.md`; `docs/SESSION-BEHAVIOUR.md` **không** (resend không
  đổi).

### 4. Ghi chú — cái nào làm ngay, cái nào thành mục mở

| Ghi chú | Quyết định |
|---|---|
| (a) Đường từ chối prefix quá dài (`lib.rs:668-673`) huỷ journal `Resumed` chưa từng thành `Connection` → `join` trên engine thread | **Làm ngay, trong hàng L** (cùng worker, cùng `lib.rs`): cho nghỉ trước khi huỷ (ADR-0154 quyết định 5). Test `a_refused_prefix_retires_its_journal`: prefix > `RX` với journal `Resumed` `FileJournal` → 0 lần tự nhường trên thread đó, `wait_for_retired_writers(1 s) == true`. Đảo ngược: bỏ lời cho nghỉ → đỏ. Kèm: `PRE <= RX` ở `shard.rs:76` thành `const` assert (hôm nay chỉ là lời hứa trong comment) |
| (a) Panic tháo ngăn xếp qua `after_serving` → thread ghi không được chờ, mất dữ liệu `Async` khi thoát | **Mục mở `STATUS.md`** + một câu trong `GUIDE.md`: thư viện không panic (điều 7), chỉ callback ứng dụng có thể; bắt panic là quyết định API riêng |
| (b) D2: `shutdown` trong lúc `connect()` chặn chỉ được nghe khi SYN hết giờ (~2 phút), `lib.rs:3603` | **Mục mở `STATUS.md`**: có từ trước; sửa đúng là `connect` không chặn, tức đổi transport — cần plan riêng và chứng minh hai chế độ. Ghi vào trang reference của D2 nếu nó đã ở `main` |
| (c) D3: chưa có test phía acceptor cho lần từ chối **do alert của peer** | **Làm ngay — hàng T**, senior developer (`opus`), song song, chỉ `crates/engine/tests/tls.rs` + `tls_wire.rs`. Quyết định: alert của peer (vd. client không tin chứng chỉ của ta) **cũng** là handshake bị từ chối và **được đếm** — người vận hành cần biết client không tin chứng chỉ mình. Test `a_peer_that_refuses_our_certificate_is_a_refusal`: client với root store không chứa CA của ta → acceptor `handshake_refused() == true`, event `TlsHandshakeRefused { count: 1 }`. Nếu hôm nay đỏ thì sửa `tls.rs` trong cùng hàng; đảo ngược: coi `AlertReceived` là `Failed` → đỏ. Gate: lệnh TLS + `check-feature-gated-tests-ran.sh` như hàng D3. Tài liệu: ADR-0151 không đổi nội dung; `GUIDE.md` một câu |
| Có từ trước, lộ ra khi đọc cho mục 1: `recover` đọc file **trên engine thread** trong `pump` (hft) | **Mục mở `STATUS.md`** (ADR-0154 *Consequences*): chuyển recovery khỏi engine thread ở `serve*` một thread là thay đổi kiến trúc (ADR-0088 đã làm cho shard) |

**Thứ tự:** K → L (cùng worker, chung file); M và T song song với K. Hàng C chạy lại mọi gate
trên commit cuối, cả hai chế độ, với `W2W_EXTRA`. Manager viết ba mục mở vào `STATUS.md` ở hàng C.

## Sửa 6 — `file_busy` trên engine thread, và một test đếm lần tự nhường bị chập chờn (2026-09-24)

**Chuyện gì xảy ra.** Hàng K+L đã dựng xong ở worktree `fb-k` (chưa commit, mọi gate xanh), còn một
rủi ro: `a_parked_reconnect_costs_the_engine_thread_no_wait` (engine `hft`, 0 lần tự nhường CPU
trong lúc một kết nối bị để chờ) đọc **1** lần và **7** lần trong ~300 lần chạy khi máy đang build
song song; ~200 lần chạy dưới tracer `sched_switch` không bắt được lần nào. Nghi phạm (chưa chứng
minh): `journal::file_busy` — `open` + `try_lock` + unlock + `close` — mà `Recovery::ready` gọi
trên engine thread trong `pump`, tối đa mỗi 1 ms khi có kết nối để chờ.

**(a) Syscall hệ thống file mỗi ms trên engine thread `hft` có chấp nhận được không? — Không.**
Điều 4 cấm engine thread `hft` ngủ trong kernel trên hot path, và lấy `read` chặn làm ví dụ.
`open(2)` phải dò đường dẫn: giữ khoá thư mục/inode, và có thể chờ I/O khi metadata chưa có trong
cache — **có thể ngủ**, và có ngủ hay không tuỳ tải của máy, đúng như độ chập chờn đã thấy. Kết nối
để chờ nằm cạnh các phiên đang chạy trong `pump`, nên đây là hot path. Việc `recover` đọc file trên
engine thread trong `pump` là chuyện có từ trước (đã ghi cho `STATUS.md`), không phải lý do để thêm
một lần đọc định kỳ.

**Quyết định** — [ADR-0155](../decisions/ADR-0155-a-recovery-learns-its-writer-let-go-from-the-writer-not-from-the-filesystem.md)
(**Proposed**; thay **nửa sau của quyết định 2** trong ADR-0154 — "`file_busy` trả lời `ready`".
ADR-0154 đã *Accepted* nên không sửa nội dung, chỉ thêm một câu ở dòng trạng thái, `CLAUDE.md` §5):

1. `Recovery::ready` **phải trả lời không cần syscall** — rustdoc nói rõ, vì engine hỏi nó trên
   engine thread trong `pump`.
2. **Chính thread ghi báo khi nó đã nhả file.** `FileJournal::released(&self) -> Released`: một
   handle `Clone` bọc `Arc<AtomicBool>`, cấp phát **lúc `open`**. Thread ghi đặt `true` (Release)
   **sau khi đã đóng `File`** (tức khoá của ADR-0154 đã nhả), trước khi giảm bộ đếm thread ghi đã
   cho nghỉ. Với `Fsync` và với `close()` có `join`: đặt khi `File` của journal bị huỷ.
   `is_released()` là một lần load Acquire.
3. **Recovery giữ handle của journal nó đã trao** cho mỗi đối tác; `ready()` = `is_released()`
   (chưa có handle → sẵn sàng). Không thêm sổ đăng ký toàn tiến trình theo đường dẫn — tra cứu nó
   cần khoá hoặc cấp phát trên engine thread, còn recovery vốn đã biết đường dẫn của mình.
4. `file_busy` giữ lại nhưng **không dùng trên engine thread** (cho tool và acceptor thread của
   shard; rustdoc nói vậy). Tiến trình **khác** giữ file thì `open` vẫn từ chối bằng `try_lock` của
   chính nó (ADR-0154 quyết định 1) — đó là lỗi triển khai, phải báo to, không để chờ.

**(b) Test phải khẳng định gì để không chập chờn mà vẫn cắn.** Lần tự nhường có thể đến từ page
fault lớn hay thu hồi bộ nhớ khi máy build song song — thứ code đang test không gây ra. `== 0` trên
máy bận là khẳng định về cái máy. ADR-0072 giữ phép đo đó cho một lần chạy riêng trên máy yên, không
cho `cargo test`. Nên tách làm hai:

- **Test 1 (tất định, chứng minh "không đụng hệ thống file"):**
  `ready_is_answered_by_the_writer_not_the_filesystem` — `FileJournal` `Async`, `put`, `retire`,
  lấy `released()` **trước**; **xoá file journal** ngay khi thread ghi còn chưa xong → `is_released()`
  vẫn `false`; sau `wait_for_retired_writers` → `true` (file vẫn đã bị xoá). Câu trả lời không thể
  đến từ hệ thống file. **Đảo ngược:** trả lời bằng `!file_busy(path)` → đỏ ngay ở khẳng định đầu
  (file đã xoá thì `file_busy` nói "rảnh" khi thread ghi còn sống): *"ready said released while the
  writer was still writing — it asked the filesystem"*.
- **Test 2 (engine, thay test chập chờn):** `a_parked_reconnect_does_not_slow_the_engine_thread` —
  engine `hft` trên `Loopback`, một phiên đang chạy, một kết nối bị để chờ bằng recovery thử có
  `ready()` đọc một `AtomicBool` do test giữ. Khẳng định: (i) trong 50 ms để chờ, số turn của engine
  **≥ 10 000** (spin thì cỡ µs/turn; ngủ 1 ms mỗi lần hỏi thì ≤ ~50) và phiên kia vẫn được trả lời;
  (ii) `ready` được hỏi **≤ số ms đã trôi + 1** lần và ≥ 1 lần; (iii) bật cờ → kết nối được nhận
  **trong turn kế tiếp** lần hỏi sau đó. **Bỏ** khẳng định 0 lần tự nhường. **Đảo ngược:** để chờ
  bằng `thread::sleep(1 ms)` → (i) đỏ; hỏi `ready` mỗi turn thay vì mỗi ms → (ii) đỏ.
- Điều "engine thread không ngủ khi để chờ" giờ dựa vào cơ chế (một lần load) + test 1; script
  strace có cửa sổ **không** chạy qua kịch bản để chờ — ghi rõ ở *Nhật ký giao hàng*, không giấu.

**Hàng P** — senior developer (`opus`), **cùng worker K+L**, trong worktree `fb-k`, trước khi K+L
được commit (P sửa đúng những file đó).

| Chạm vào | Không chạm | Phụ thuộc |
|---|---|---|
| `crates/engine/src/journal.rs` (`Released`, `released()`, đặt cờ sau khi đóng `File` ở cả ba đường; rustdoc `file_busy`), `crates/engine/src/recovery.rs` (rustdoc `ready`: không syscall, kèm mẫu), `tools/interop/src/reconnect.rs` (giữ handle, `ready` = `is_released()`), `crates/engine/tests/one_appender.rs` (test 1 mới; thay test chập chờn bằng test 2) | `crates/engine/src/lib.rs` (cách để chờ không đổi), `crates/session/`, `scripts/` | K+L trong `fb-k` |

- **Đỏ trước:** test 1 viết trước, chạy với `ready` hiện tại (qua `file_busy`) → đỏ với câu trên.
  Test 2 chạy trên code hiện tại phải **xanh** (cơ chế để chờ không đổi) — hai đảo ngược của nó là
  bằng chứng nó cắn.
- **Gate:** `cargo test -p fixbolt-engine --test one_appender` **chạy 200 lần liên tiếp trong lúc
  máy build song song** (`for i in $(seq 200); do … || break; done`, trích số lần đạt) — 200/200;
  `cargo test -p fixbolt-engine --test retire --test journal --test reconnect_wire`; interop build
  + arm reconnect nếu có; `cargo clippy --all-targets -- -D warnings`; `cargo test --no-default-features`;
  `cargo bench -p fixbolt-engine --bench alloc` (`is_released` không cấp phát); cả hai chế độ:
  `W2W_EXTRA="--journal file-async --log file"` với ba script chế độ (engine thread và cách chờ không
  đổi, nhưng thread ghi đổi thứ tự đóng file).
- **Xong khi:** test 1 đỏ-rồi-xanh, hai đảo ngược của test 2 đỏ đúng câu, 200/200, gate trích nguyên văn.
- **Tài liệu:** `docs/GUIDE.md` (recovery giữ `Released` cho mỗi đối tác; `ready` không được
  syscall; `file_busy` không dùng trên engine thread); `CHANGELOG.md` (`FileJournal::released`,
  `Released`); `DESIGN.md` §4 D7 một câu; `docs/reference/a-readiness-probe-that-opened-a-file-on-the-engine-thread.md`
  (mới — bẫy: probe đúng về logic nhưng là syscall có thể ngủ; test đếm tự nhường chập chờn vì máy);
  ADR-0155 → *Accepted*.

## Sửa 7 — 2026-09-24

Test 2 của hàng P không đếm "≥ 10 000 lượt trong 50 ms" nữa: đếm lượt đo bộ lập lịch của máy
(khoảng 8 400 khi máy rảnh, 8 khi máy đang build). Thay bằng: engine thread ở trạng thái chạy được
(đọc `/proc/self/task/<tid>/schedstat`) ít nhất một nửa cửa sổ 50 ms khi có kết nối đang đỗ. Đảo
ngược bằng `sleep(1 ms)` đọc 455 µs trên 50 ms, vẫn đỏ. Manager quyết theo mandate thường trực.

## Nhật ký giao hàng

Điền vào mỗi khi đóng một phase: đã dựng gì, ở đâu, gate nào xanh, cái gì chưa làm và vì sao.

**Đây là phần sống sót qua nén context** — phiên sau đọc mục này trước tiên.

## Sửa 1 — 2026-09-23

- Khi làm D4, test so với `dict` tìm thêm một lỗi cũ: `as_i64` và `as_u32` dừng ngay ở chữ số làm tràn số, nên `9410947898048986560 ` (dấu cách ở cuối) bị báo `Overflow` trong khi `dict` coi đó là sai định dạng.
- Manager quyết: gộp vào D4. Cả hai hàm phải báo `NotANumber` khi có byte không phải chữ số ở bất kỳ đâu, trước `Overflow` — cùng luật với `as_decimal` (ADR-0120: lỗi cú pháp thắng tràn số).
- Test so với `dict` bỏ ngoại lệ `Overflow`, thành khớp đúng hai chiều; thêm test `a_syntax_fault_wins_over_overflow`; `CHANGELOG.md` ghi đây cũng là thay đổi phá vỡ.
