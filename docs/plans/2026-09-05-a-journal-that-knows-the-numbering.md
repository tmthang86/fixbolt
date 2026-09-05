# Một journal biết mình đã đếm tới đâu

> **Loại:** Plan · **Ngày:** 2026-09-05 · **Trạng thái:** **Đã duyệt 2026-09-05**, đang làm
> **Phạm vi:** `STATUS.md` item 48. Chạm `session` (trait `Journal`, hai chỗ tiêu số thứ tự),
> `engine` (`MemJournal`, `FileJournal`, `Reader`, `Resumed`, hai chỗ gọi `mark_active`),
> `tools/interop` + `scripts/interop.sh`, tài liệu, một ADR mới. **Không chạm** `codec`, `dict`,
> `library`, `transport`.
>
> **Máy chạy:** macOS hoặc Linux đủ cho toàn bộ test. Gate quyết định là job `interop` trong CI.
> **Không cần máy §9** — đây là gate về tính đúng. Có một số đo phải làm trên Linux (bước 5).
> **Thời lượng dự kiến:** 1–2 ngày.

> **Vì sao plan này KHÔNG chờ item 47**, dù `STATUS.md` item 48 viết *"không đóng được trước
> item 47"*. `[quyết định 2026-09-05]` Câu đó đúng với một cách sửa — đọc `next_out` từ
> `Observer` — và cách sửa đó **sai từ gốc**: `Observer` chỉ biết con số lúc ai đó hỏi, nên một
> Heartbeat gửi ra giữa lần hỏi cuối và lúc tiến trình chết vẫn làm số bị hụt. Nguồn duy nhất
> vừa *bền* vừa *có mặt đúng lúc `recover`* là chính journal. Nên cách sửa là **dạy journal ghi
> lại con số**, không phải mở thêm đường tới một con số đang sống. Sửa như vậy thì item 48 không
> cần bất cứ thứ gì của item 47, và đứng trước item 47 vì nó nhỏ hơn và đang ghim một gate đỏ.
>
> **Vì sao đứng trước đợt B** — cùng lý do item 45 (a) đưa item 46 lên trước: một initiator
> không sống qua được một lần logout sạch là lỗ hổng **sản phẩm**, còn đợt B là knob. Và
> `settings-for-both-roles` có `ResetOnLogon/Logout/Disconnect` — ba knob nói về **cách đánh số
> lại** — nên viết chúng lên một journal chưa biết đánh số là viết hai lần. Item 45 được sửa cùng
> commit với plan này, mục (c).

## Sửa, ghi trước khi code dịch chuyển

**Sửa 1 — ADR là 0053, không phải 0052.** `[2026-09-05]` Plan viết lúc `ADR-0052` còn trống.
Công việc của item 49 và 52 đã lấy số đó (`ADR-0052-two-candidates-are-retired-with-numbers-…`)
và đã merge. `CLAUDE.md` §5: số ADR không bao giờ dùng lại. Mọi chỗ trong plan này đọc *ADR-0052*
nghĩa là **ADR-0053**.

**Sửa 2 — `send_as` KHÔNG nhận journal; journal được kể *mốc cao nhất*, không phải từng số.**
`[đo 2026-09-05]` Rủi ro *"`send_as` luồn journal xuống làm nhiều chữ ký nội bộ đổi"* đã xảy ra,
và nó lớn hơn ngưỡng 6 hàm plan tự đặt — **đếm được 19 hàm**, trong đó có `tick`, `received`,
`send_heartbeat`, `send_test_request`, `send_resend_request`, `connect`, `disconnect`,
`disconnect_with`, `begin_logout`, `logout_now`, `send_sequence_reset` (public) và `send`,
`send_reject`, `logout_with`, `too_high`, `fill`, `tick_inner`, `judge`, `drain` (private).
Luồn journal xuống đó **phá chính API không-journal** mà 59 định nghĩa và phần lớn test đang
dùng — tức là trả giá bất biến 3 để mua một chi tiết cài đặt.

Nên đổi hình, theo đúng lối thoát plan đã cho phép:

- `Session` không giữ thêm state. Số chiều ra cao nhất đã tiêu **đã có sẵn**: `next_out - 1`.
- Một helper private `tell_journal(&self, journal)` gọi `journal.mark_out(self.next_out - 1)`
  khi `next_out > 1`. Gọi từ những chỗ **đã** cầm journal: `tick_with`, `received_with`,
  `send_application`.
- `mark_out` là **mốc cao nhất (high-water mark), đơn điệu tăng**, không phải sự kiện một-số-một-lần.
  Gọi lại với số cũ là no-op. `put(seq)` thành công cũng nâng `highest_out`, nên `mark_out(seq)`
  ngay sau đó **không ghi gì** — đúng cái bẫy *"ghi số hai lần"* plan đã lường, nay được chặn
  bằng cấu trúc chứ không bằng điều kiện ở chỗ gọi.
- Câu bất biến đổi theo: từ *"mỗi số tiêu ra được journal nghe đúng một lần"* thành
  **"journal luôn biết số chiều ra cao nhất đã tiêu"**. Yếu hơn về hình, đủ mạnh về việc: cái
  `recover` cần là một con số, không phải một danh sách.

**Sửa 3 — ba thứ đọc code hôm nay mới thấy, cả ba đổi việc phải làm.**

1. **`connect` và `disconnect*` không tiêu số.** `Session::connect` có `let _ = emit;` — nó chỉ
   đặt state, Logon của initiator đi ở `tick` kế tiếp (`crates/session/src/lib.rs:1206`);
   `disconnect_with` cũng vậy (`:1275`). Nên chúng **không** cần bản `_with`. Plan viết
   *"`logout_now` là ngoại lệ"* như thể chỉ có một; danh sách thật là **ba**, và đó là ba chỗ
   engine tiêu số mà không cầm journal: `logout_now` (`conn.rs:477`, `:644`), `begin_logout`
   (`conn.rs:596`), `send_sequence_reset` (`conn.rs:549`). Cả ba mọc bản `_with`.
2. **`tick_with` đang nhận `journal: &J`, phải thành `&mut J`.** Một thay đổi **breaking** trên
   API public mà plan không gọi tên. `CHANGELOG.md` phải ghi.
3. **Chỗ xả không được đặt "ở cuối `received_with`".** Hàm này `return` sớm khi `judge` trả
   `Link::Dropped` (`:1465`) — và **đó đúng là đường logout sạch**: đối phương gửi `35=5`,
   session này trả lời `35=5` (tiêu một số) rồi drop. Xả ở cuối là bỏ sót đúng cái ca đang sửa.
   Nên thân cũ của `received_with` và `tick_with` lùi thành `*_inner`, và bản public gọi inner →
   `tell_journal` → trả kết quả. Không đường nào ra mà không đi qua chỗ xả.

**Sửa 4 — chiều VÀO có đúng cái lỗ đó, trên đúng dòng đó, và gate interop tìm ra chứ không phải
test của repo này.** `[đo 2026-09-05]` Sau khi nửa chiều ra xanh, kịch bản logout sạch đi đủ xa
để đọc tới assertion `no_resend` và đọc được **`35=2: 1`** — engine này gửi một `ResendRequest`
cho message nó đã có.

Nguyên nhân: `received_with` `return` sớm khi `judge` trả `Link::Dropped`, và **`35=5` của đối
phương được phán đúng kiểu đó**, nên `journal.mark_in` nằm sau chỗ đó không bao giờ chạy. Số
`34=` mà `35=5` mang đã bị tiêu và không được ghi; session hồi phục chờ lại đúng số ấy, message
kế tiếp của đối phương cao hơn một, mở gap, và engine hỏi lại.

Là **ảnh gương chính xác** của lỗi item 48: cùng một dòng `return`, cùng một message, chiều
ngược lại. Nên `mark_in` chuyển ra `received_with` nằm cạnh `tell_journal` — vẫn **sau** khi giao
cho application (ADR-0017 giữ nguyên, chỉ muộn hơn), và `mark_in` lấy `max` nên những đường ra
không tiêu gì chỉ nhắc lại một số journal đã biết.

Test đỏ trước: `the_logout_that_ends_the_session_is_still_a_message_that_was_consumed`, đỏ ở
`left: Some(1)` chống `right: Some(2)`. **Bước 5 sinh ra một mục của bước 2**, và đó là lý do
gate ý-kiến-thứ-hai tồn tại.

## Bối cảnh

Một session FIX đánh số **mọi** message nó gửi — `Logon`, `Heartbeat`, `Logout` đều tiêu một số
`34=`. Nhưng journal của engine này chỉ giữ **application message**, vì journal là kho để trả
lời `ResendRequest`, và message hành chính không bao giờ được phát lại, chỉ được gap-fill.

Hệ quả: mọi ví dụ trong repo suy `next_out` bằng `journal.highest() + 1`, và con số đó **hụt
đúng bằng số message hành chính gửi sau application message cuối cùng**. Một lần logout sạch là
đủ để hụt một: trả lời `35=5` của đối phương tiêu một số mà journal không biết.

`[đo 2026-09-05]` Một `libquickfix` thật nói bằng lời của nó khi engine này quay lại sau
`SIGTERM`: `MsgSeqNum too low, expecting 4 but received 3`. Kịch bản `SIGKILL` **chỉ xanh vì
`HeartBtInt=30`** được chọn để không có Heartbeat nào lọt vào cửa sổ vài giây của kịch bản
(`tools/interop/src/reconnect.rs`, chú thích *"The plan's trap 4"*) — tức là nó xanh nhờ điều
kiện thí nghiệm, không nhờ engine. Đầy đủ ở
[a-journal-holds-messages-not-numbering](../reference/a-journal-holds-messages-not-numbering.md).

Điều đáng nói: journal **đã** biết đếm chiều vào. `Journal::mark_in(seq)` được gọi sau mỗi
message nhận (ADR-0017), `highest_in()` đọc lại được, và `next_in` của một session hồi phục là
`highest_in() + 1` — đúng, không hụt. Chiều ra là chỗ **bất đối xứng**: có `put` cho bytes, không
có gì cho *con số*. Plan này lấp đúng chỗ đó.

## Những gì đã biết chắc (đọc code 2026-09-05)

| Sự thật | Nguồn |
|---|---|
| Trait `Journal` có `put / get / highest / oldest / mark_in / highest_in / mark_active / last_active`. **Không có gì ghi số thứ tự chiều ra ngoài `put`** | `crates/session/src/journal.rs:22–132` |
| `mark_in` được gọi **mỗi message nhận**; rustdoc ADR-0017 nói rõ giá dưới `Fsync` là một `sync_data` mỗi message — tiền lệ cho một ghi-nhận-mỗi-message trên journal | `crates/session/src/journal.rs:83–100`; `crates/engine/src/journal.rs:779–802` |
| `mark_active` **không** gọi mỗi message: engine gọi lúc logon và lúc ordered shutdown nói goodbye | `crates/engine/src/lib.rs:752`, `:957` |
| Message hành chính đi qua `send_as`: `next_out += 1` khi `at.is_none()`; **hàm này không cầm journal** | `crates/session/src/lib.rs:1775–1827` |
| `logout_now(text, emit)` là API public, **không nhận journal** — và là đúng đường ordered shutdown và slow-consumer đi | `crates/session/src/lib.rs:1687`; `crates/engine/src/conn.rs:477, :644` |
| Đường application: `journal.put(seq_out, …)` rồi `next_out += 1`; **một `put` bị từ chối vẫn tiêu số** (`puts_refused += 1`) — nên `highest()` hụt cả trong trường hợp này | `crates/session/src/lib.rs:2464–2496` |
| `send_application` (ADR-0048) cùng hình: `put` ở `:1871`, tiêu số ở `:1876` | `crates/session/src/lib.rs:1846–1876` |
| Phát lại (`at = Some`) **không** tiêu số | `crates/session/src/lib.rs:1823–1825` |
| `Session::next_out()` là getter `const`; `set_next_out` tồn tại cho `Admin` | `crates/session/src/lib.rs:988, :1050` |
| Định dạng file v1: `seq(4) ‖ len(4) ‖ bytes ‖ crc(4)`; `len == 0` là inbound mark, `seq == 0 && len == 8` là activity mark; `Reader` phân loại đúng bằng hai điều kiện đó | `crates/engine/src/journal.rs:367–380, :529`; `DESIGN.md` D7 |
| `Reader`/`Record` public: `Message`, `InboundMark`, `ActivityMark` — **ba hình, không có hình cho số chiều ra** | `crates/engine/src/journal.rs:815–848` |
| `FileJournal` ghi hai tầng: `Async` đẩy vào writer thread, `Fsync` ghi + `sync_data` trên engine thread | `crates/engine/src/journal.rs:685–802` |
| `Resumed { journal, next_out, next_in, last_active_ms }`; rustdoc `next_out` nói *"Usually `journal.highest() + 1`"* | `crates/engine/src/recovery.rs:50–75` |
| Hai chỗ suy sai cùng một kiểu: `crates/engine/tests/on_disk.rs:288` và `tools/interop/src/reconnect.rs:195` | đọc 2026-09-05 |
| `dial` hỏi `recovery.recover(&cfg)` **mỗi lần** nối lại (ADR-0043) | `crates/engine/src/lib.rs:1690–1710` |
| `scripts/interop.sh` bước `known_gap` ghim `expecting N but received N-1` và **cố ý đỏ khi item 48 được sửa** | `scripts/interop.sh:526–547` |
| Kịch bản `SIGKILL` 5/5 chạy với `HeartBtInt=30` để không có Heartbeat trong cửa sổ | `tools/interop/src/reconnect.rs:232–236`; `scripts/interop.sh:372` |
| `GUIDE.md` §8c điểm 5 dặn người dùng *"tự giữ bộ đếm chiều ra bên cạnh journal"* — một ràng buộc đẩy sang người dùng vì engine chưa làm | `docs/GUIDE.md:1127–1139` |
| `cargo test --all` hôm nay **506** | `STATUS.md` 2026-09-05 |
| QuickFIX `FileStore` giữ `seqnums` riêng khỏi `body`, cập nhật **mỗi** message gửi kể cả hành chính — prior art nói cùng một điều | `docs/reference/prior-art.md`; `vendor/` |

## Cách làm

**Journal ghi thêm một câu trả lời: *"số chiều ra cao nhất đã tiêu"*.** Không đổi câu trả lời
cũ, không đổi ý nghĩa của `highest()` (vẫn là *message cao nhất còn giữ để phát lại*).

### Trait `Journal` mọc thêm hai method, đối xứng với chiều vào

```rust
/// Số chiều ra cao nhất đã tiêu tính đến lúc này — kể cả những message
/// KHÔNG nằm trong journal (hành chính, hoặc application mà `put` từ chối).
/// Đơn điệu tăng: một số nhỏ hơn cái đã biết không làm gì.
fn mark_out(&mut self, seq: u32);
/// Số chiều ra cao nhất đã tiêu, tính cả `put` lẫn `mark_out`. `None` nếu chưa gửi gì.
fn highest_out(&self) -> Option<u32>;
```

- **`mark_out` là mốc, không phải sự kiện** (Sửa 2). Gọi nó với một số journal đã biết là no-op,
  và `put(seq)` thành công cũng nâng `highest_out`, nên gọi `mark_out` sau một `put` giữ được
  **không ghi thêm gì lên đĩa**. Quy tắc một câu: *journal luôn biết số chiều ra cao nhất đã tiêu.*
- **`highest_out` không có thân mặc định**, cùng lý do `highest` và `highest_in` không có: một
  journal có giữ trạng thái không được phép nói mình không giữ. `NoJournal` trả `None`.
- **`mark_out` có thân mặc định rỗng**, như `mark_active`: một journal không sống qua restart
  không phải giả vờ.

### Session kể cho journal ở những chỗ nó đã cầm journal

Số cao nhất đã tiêu **đã có sẵn** trong session: `next_out - 1`. Không thêm state.

```rust
fn tell_journal<J: Journal>(&self, journal: &mut J) {
    if self.next_out > 1 { journal.mark_out(self.next_out - 1); }
}
```

- `received_with` và `tick_with`: thân cũ lùi thành `received_inner` / `tick_after_inner`, bản
  public gọi inner → `tell_journal` → trả kết quả, nên **không đường nào ra mà không xả** —
  kể cả đường `return` sớm khi `judge` trả `Dropped`, vốn là chính đường logout sạch (Sửa 3.3).
  `tick_with` đổi `&J` thành `&mut J` (breaking, Sửa 3.2).
- `send_application`: gọi sau `next_out += 1`. Phủ cả ca `put` bị từ chối, không cần nhánh riêng.
- **Ba entry point public engine dùng để tiêu số mà không cầm journal** mọc bản `_with`
  (Sửa 3.1), giữ nguyên bản cũ cho caller không có gì để ghi:
  `logout_now_with`, `begin_logout_with`, `send_sequence_reset_with`. `Conn` đổi sang bản `_with`
  ở bốn chỗ gọi (`conn.rs:477`, `:549`, `:596`, `:644`). Bỏ sót `logout_now_with` thì gate
  `known_gap` vẫn đỏ; bỏ sót `begin_logout_with` thì kịch bản `SIGTERM` vẫn đỏ.

### `MemJournal` và `FileJournal`

- `MemJournal`: một `Option<u32>` `highest_out`, cập nhật trong cả `put` (khi giữ) lẫn
  `mark_out`. Không cấp phát.
- `FileJournal`: một **record mới trên đĩa**, hình `seq == 0 && len == 4`, payload là số đã tiêu
  (little-endian). Phân biệt được với activity mark (`len == 8`) và inbound mark (`len == 0`)
  bằng cùng cách `Reader` đang dùng. Hai tầng `Async`/`Fsync` y hệt `mark_in`. **Không nâng
  version**: `34=0` vẫn không phải số FIX có, và D7 đã ghi hai mark trước không tốn đổi format
  vì cùng lý do. Lúc mở file, `highest_out` = max của mọi `Message.seq` và mọi outbound mark.
- `Reader`/`Record` mọc `Record::OutboundMark { seq }`. Một binary **cũ** đọc file mới sẽ thấy
  nó là `Message { seq: 0, bytes: 4 byte }` — chấp nhận được vì chưa có gì được publish, và ADR
  ghi rõ đây là lần cuối một hình record mới được thêm mà không nâng version.

### `Resumed` học cách tự suy, để không còn ví dụ nào suy tay

```rust
impl<J: Journal> Resumed<J> {
    /// `next_out = highest_out() + 1`, `next_in = highest_in() + 1`, `last_active_ms = last_active()`.
    /// `None` nếu journal không biết gì cả.
    pub fn from_journal(journal: J) -> Option<Self>;
}
```

`on_disk.rs` và `tools/interop` đổi sang gọi nó. Rustdoc của `next_out` bỏ chữ *"usually"* và
nói: *"`highest_out() + 1`, và `from_journal` tính hộ"*. Engine vẫn **không** tự quyết định
resume hay restart — ADR-0010 giữ nguyên, chỉ là con số đúng nay có sẵn để lấy.

### Gate interop đảo chiều

- `known_gap` **bị xoá**, thay bằng `continued`: kịch bản `SIGTERM` quay về **5/5**, cùng năm
  assertion với kịch bản `SIGKILL` (`dropped` đổi thành `goodbye`).
- Kịch bản `SIGKILL` **mất điều kiện `HeartBtInt=30`**: chạy thêm một lượt `--heart-bt-int 1`
  với `sleep 2.5` trước khi kill, để **chắc chắn có ít nhất một Heartbeat sau `35=B`**. Lượt này
  là thứ chứng minh sửa đúng cho *mọi* message hành chính chứ không chỉ cho `35=5`. Assertion
  `next_out` vẫn quan hệ (một-quá-số-cuối-đã-gửi), nên nó đọc được số nào Heartbeat tiêu.

### File sẽ tạo hoặc sửa

`crates/session/src/journal.rs` · `crates/session/src/lib.rs` (`send_as`, hai đường application,
`logout_now_with`) · `crates/engine/src/journal.rs` (`MemJournal`, `FileJournal`, `Reader`,
`Record`) · `crates/engine/src/recovery.rs` (`Resumed::from_journal`) · `crates/engine/src/conn.rs`
(ba chỗ `logout_now`) · `crates/engine/tests/on_disk.rs`, `recovery.rs`, `journal_reader.rs`,
`engine_recovery.rs` · `crates/session/tests/` (một file test mới cho `mark_out`) ·
`tools/interop/src/reconnect.rs` · `scripts/interop.sh` · `docs/decisions/ADR-0052-*.md` (mới).

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **2 — session thuần** | thêm một lời gọi trait trên đường gửi hành chính | `mark_out` là method trait, không socket, không clock, không cấp phát; `--test score` **59/59** là bằng chứng, và `NoJournal` giữ thân rỗng nên corpus chạy y như cũ |
| **1 — không cấp phát trên hot path** | `MemJournal::mark_out` chạy mỗi Heartbeat; `FileJournal` `Async` đẩy vào writer | hai case mới trong `crates/engine/benches/alloc.rs`: `mark-out-mem`, `mark-out-file-async`; đảo bằng một `to_vec()` trong `mark_out`, phải đọc ra số khác 0 |
| **4 — engine thread không ngủ (`hft`)** | `FileJournal` `Fsync` giờ `sync_data` mỗi message hành chính | **đây là giá ADR-0017 đã trả cho chiều vào**, không phải giá mới; ADR mới nói rõ và `CONFIGURATION.md` hàng `Durability` ghi thêm; `Async` không chạm engine thread, `scripts/check-no-kernel-sleep.sh` chạy lại ở `hft` với `Async` phải sạch |
| **3 — 59 định nghĩa** | session đổi | chạy, đọc số: 59/59 |
| **10 — số nào cũng có benchmark** | `benches/turn.rs` có thêm một lời gọi trait per admin message | đo trước/sau trên cùng máy; lệch band thì ghi số, không nới band |
| 5, 6, 7, 8, 9 | không | — |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | **ADR-0052 `Proposed`**: journal trả lời hai câu hỏi tách bạch — *phát lại được gì* và *đã đếm tới đâu*; vì sao `mark_out` chứ không phải ghi `put` cho cả hành chính (hành chính không được phát lại, ghi bytes là dối kho resend); vì sao không nâng version; vì sao **không** lấy số từ `Observer`; giá dưới `Fsync` | — |
| 2 | Trait `Journal` + `NoJournal` + `MemJournal`; session gọi `mark_out` ở `send_as`, hai đường application khi `put` từ chối, `logout_now_with`. **Test đỏ trước**: `crates/session/tests/numbering.rs` — logon, một Heartbeat, một Logout: `highest()` là `None`, `highest_out()` phải là `Some(3)` | 1 |
| 3 | `FileJournal` ghi/đọc outbound mark, `Reader::OutboundMark`; `on_disk.rs` mở lại file sau logon + Heartbeat + Logout và đọc đúng số. Test file v1 **cũ** (không có mark) vẫn đọc như trước — fixture giữ nguyên | 2 |
| 4 | `Resumed::from_journal`; `on_disk.rs:288` và `tools/interop/src/reconnect.rs:195` đổi sang dùng nó; `grep -rn 'highest().*+ 1'` trong repo phải **trống** ngoài chính `from_journal` | 3 |
| 5 | `scripts/interop.sh`: `known_gap` → `continued`, `SIGTERM` 5/5; lượt `SIGKILL` có Heartbeat. Alloc cases, `benches/turn.rs`, `check-no-kernel-sleep.sh` trên Linux. ADR `Accepted`, bảng §4 | 4 |

## Cách kiểm chứng

- **Gate quyết định là bước 5, ý kiến của một implementation khác.** `scripts/interop.sh` phải in
  `7 / 7 + 7 / 7 + 5 / 5 + 5 / 5` — và **cột thứ tư là 5/5 chứ không còn 3/3**. Thêm lượt
  `SIGKILL`-có-Heartbeat; script grep tên bước, nên đổi tên `known_gap` mà quên sửa danh sách
  bước ở `scripts/interop.sh:586` phải làm script thoát 1 — đó là reversal miễn phí đầu tiên.
- **Đảo ngược, mỗi cái phải đỏ đúng chỗ:**
  (a) bỏ lời gọi `mark_out` trong `send_as` → `numbering.rs` đỏ, `continued` đỏ với đúng chữ
  `expecting 4 but received 3`, và **`--test score` vẫn 59/59** — ghi lại, vì nó nói corpus mù
  với chuyện này (đã biết từ trước, nay có bằng chứng cụ thể hơn);
  (b) giữ `send_as` nhưng để `Conn` gọi `logout_now` cũ thay vì `logout_now_with` → **chỉ**
  kịch bản `SIGTERM` đỏ, `SIGKILL` cả hai lượt vẫn xanh — đây là reversal chứng minh ngoại lệ
  `logout_now` là thật;
  (c) `FileJournal::open` bỏ qua outbound mark khi đọc → `on_disk.rs` đỏ trong khi test
  `MemJournal` vẫn xanh;
  (d) `--no-recovery` của `tools/interop` vẫn phải đỏ ở `next_out` như hôm nay — sửa xong không
  được làm reversal cũ mất tác dụng.
- `cargo test --all` và `cargo test --all --no-default-features`, **đọc số test**: 506 phải tăng,
  tăng bao nhiêu thì nói ra.
- `cargo clippy --all-targets -- -D warnings`; `scripts/check-lint-config.sh`;
  `scripts/check-no-optional-deps.sh`.
- `scripts/bench.sh`: hai case alloc mới; `turn` so với band của máy.
- Linux: `scripts/check-no-kernel-sleep.sh` cả hai lượt.
- **Một CI run xanh, gọi tên bằng id, cho đúng commit đóng plan.**

## Tài liệu phải cập nhật

- [ ] `docs/decisions/ADR-0052-*.md` — mới
- [ ] `DESIGN.md` §4 D7 — journal trả lời *hai* câu; đoạn *"The journal is not a message log"*
      sửa: nó vẫn không giữ bytes hành chính, nhưng **giữ số** của chúng; định dạng có mark thứ ba
- [ ] `DESIGN.md` §8 nếu `benches/turn.rs` đổi hàng
- [ ] `docs/GUIDE.md` §8c điểm 5 — viết lại: `Resumed::from_journal` là cách đúng, bỏ câu *"tự
      giữ bộ đếm"*; §6b thêm một câu về giá `Fsync` mỗi message hành chính
- [ ] `docs/SESSION-BEHAVIOUR.md` §4 — *"mỗi số tiêu ra được journal nghe đúng một lần"*, **gọi
      tên `numbering.rs`**
- [ ] `docs/CONFIGURATION.md` hàng `Durability` — `Fsync` giờ đồng bộ cả message hành chính
- [ ] `docs/CONFORMANCE.md` — cột interop thành `5 / 5 + 5 / 5`, gọi tên CI run
- [ ] Rustdoc `Journal`, `Resumed::next_out`, `Record`; `crates/library/README.md` nếu có ví dụ
      `Recovery`
- [ ] `CHANGELOG.md` — trait `Journal` thêm hai method (**breaking** cho ai tự implement journal),
      `Resumed::from_journal`, `Record::OutboundMark`, `logout_now_with`
- [ ] `docs/reference/a-journal-holds-messages-not-numbering.md` — mục *What was done about it*;
      **giữ marker `[to testing-skills]`** nếu có
- [ ] `STATUS.md` — gạch item 48; item 45 (c); **đi qua *Not proven***; `known_gap` không còn tồn
      tại nên mọi chỗ nhắc nó phải đổi

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Sửa `send_as` mà quên `logout_now` — gate `SIGKILL` xanh, `SIGTERM` vẫn đỏ | reversal (b); kịch bản `SIGTERM` trong CI |
| Sửa cho `35=5` mà không sửa cho Heartbeat — kịch bản cũ xanh nhờ `HeartBtInt=30` | lượt `SIGKILL`-có-Heartbeat, mới |
| Ghi số hai lần (cả `put` lẫn `mark_out`) — không sai kết quả, nhưng nhân đôi ghi đĩa mỗi application message | `journal_reader.rs` đếm record: N application message ⇒ đúng N `Message`, 0 `OutboundMark` |
| `put` bị từ chối (message dài hơn slot) không được `mark_out` → hụt số ở đúng session có message to | test trong `crates/engine/tests/journal.rs`: một reply dài hơn `SLOT_LEN`, `highest_out()` vẫn tiến |
| `Reader` cũ hiểu mark mới là `Message { seq: 0 }` | `journal_reader.rs` thêm case; ADR ghi rõ; không có binary nào đã publish |
| File v1 cũ không có mark → `highest_out()` bằng `highest()` và **vẫn hụt** như trước, âm thầm | `from_journal` trả `next_out` từ `highest_out()`; test mở fixture cũ và khẳng định kết quả **giống hôm nay** — không tốt hơn, không tệ hơn, và rustdoc nói file cũ không được sửa số |
| Một Heartbeat mỗi giây thành một `sync_data` mỗi giây dưới `Fsync` | `CONFIGURATION.md` nói; `Async` là mặc định; không đổi mặc định |
| `--test score` không thấy gì hết | ghi vào ADR như một sự thật, không coi là gate |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| ADR kết luận phải nâng format lên v2 | thấp | plan sửa bước 3, ghi Sửa 1; `FileJournal` đã có máy phân biệt v0/v1 nên v2 là thêm một nhánh |
| `send_as` luồn journal xuống làm nhiều chữ ký nội bộ đổi | trung bình | tất cả là `fn` private của `Session`; nếu vượt quá 6 hàm thì cân nhắc session tự giữ `unmarked: Option<u32>` và xả một lần cuối `step` — ghi thành Sửa nếu đổi |
| Trait `Journal` đổi phá journal tự viết của người dùng | thấp | chưa publish; `CHANGELOG.md` gọi là breaking |

## Ngoài phạm vi

Đọc `next_out` từ `Observer` qua front door (item 47 — plan riêng). `ResetOnLogon/Logout/
Disconnect` (đợt B). Ghi số vào `MessageLog`. Lưu `next_in` theo cách khác. `RefreshOnLogon`.
Một tiến trình fixbolt bị giết rồi hồi phục trước một `libquickfix` — vẫn là *Not done* của
reconnect-interop, và plan này không đóng nó.

## Nhật ký giao hàng

**2026-09-05 — cả năm bước xong.** Bốn Sửa, cả bốn ghi trước khi code dịch chuyển.

| Bước | Kết quả |
|---|---|
| 1 | [ADR-0053](../decisions/ADR-0053-the-journal-answers-two-questions-and-the-second-is-a-number.md), `Accepted`. Số 0052 đã bị công việc item 49/52 lấy mất — Sửa 1 |
| 2 | `Journal::mark_out` + `highest_out`; `tell_journal` gọi từ `received_with`, `tick_with`, `send_application`; ba bản `_with`; `Conn` đổi ở bốn chỗ. `crates/session/tests/numbering.rs` **đỏ trước**: `left: None` chống `right: Some(3)` |
| 3 | `FileJournal` ghi/đọc outbound mark, `Record::OutboundMark`; sáu test mới trong `on_disk.rs`, gồm `Fsync`, `put` bị từ chối, và file cũ đọc y như cũ |
| 4 | `Resumed::from_journal`; `on_disk.rs`, `tools/interop`, `engine_recovery.rs`, `recovery.rs` đổi sang dùng nó — `grep` cho `highest().{expect,map_or}` trong `crates/` và `tools/` **trống** |
| 5 | `scripts/interop.sh`: `known_gap` xoá, `SIGTERM` về **5/5**, thêm kịch bản `interop-reconnect-beat` ở `HeartBtInt=1`. Hai case alloc mới. Tài liệu |

**Số đo, đọc từng dòng chứ không đọc mã thoát:**

- `cargo test --all` **507 → 519**, 0 fail. `--no-default-features` 514, 0 fail.
- `cargo clippy --all-targets -- -D warnings` sạch; `cargo fmt` sạch.
- 59 định nghĩa: **59/59**, không đổi — và đó chính là điều đáng nói, corpus mù với chuyện này.
- `scripts/check-no-optional-deps.sh` ok; `scripts/check-lint-config.sh` ok, chứng minh bằng đảo ngược.
- `benches/alloc.rs`: `mark-out-mem 0`, `mark-out-file-async 0`, 29/29 case bằng 0. **Đảo ngược**
  (`vec![seq]` trong `mark_out`) đọc `mark-out-mem 10000 mark-out-file-async 10000` — và cũng làm
  đỏ tám case khác (`observe-idle`, `events-idle`, `admin-idle`, `origin-idle`…), tức là
  `tell_journal` thật sự nằm trên vòng lặp turn chứ không chỉ trên đường test.
- **`scripts/interop.sh`: `7 / 7 + 7 / 7 + 5 / 5 + 5 / 5 + 5 / 5`**, 29 assertion, mỗi dòng đọc
  riêng. `interop-reconnect-logout: next_out ok sent up to 34=3 before the kill, came back at
  34=4, wanted 34=4` — chỗ trước đây là `known_gap`.

**Bước 5 sinh ra một mục của bước 2 — Sửa 4.** Lần chạy interop đầu tiên sau khi nửa chiều ra
xanh đọc `no_resend FAIL 35=2: 1`: chiều **vào** có đúng cái lỗ đó, trên đúng dòng `return` đó.
Không test nào của repo này thấy được, vì chúng đều lái message để link còn sống, còn corpus so
byte và mọi byte đều đúng.

**Chưa làm, nói rõ ra:**

- **`scripts/check-no-kernel-sleep.sh` chưa chạy.** Máy này là container không có `bpftrace`;
  ADR-0053 khẳng định `Async` không chạm engine thread bằng cấu trúc (đẩy vào ring, y hệt
  `mark_in`), không bằng số đo.
- **`benches/turn.rs` chưa đo lại.** Mỗi turn nay thêm một lời gọi trait; đây không phải máy §9
  nên một con số ở đây sẽ là số của máy khác. **Đó là một mục mở, không phải một mục đã xong.**
- **Chưa có CI run xanh gọi tên cho commit đóng plan** — ô cuối §9. `docs/CONFORMANCE.md` nói
  thẳng rằng 5/5+5/5+5/5 hiện là lời của một máy dev.
