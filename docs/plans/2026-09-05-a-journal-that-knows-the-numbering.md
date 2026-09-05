# Một journal biết mình đã đếm tới đâu

> **Loại:** Plan · **Ngày:** 2026-09-05 · **Trạng thái:** Chờ duyệt
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
/// Số `34=` này vừa được tiêu bởi một message KHÔNG nằm trong journal —
/// hành chính, hoặc application mà `put` từ chối.
fn mark_out(&mut self, seq: u32);
/// Số chiều ra cao nhất đã tiêu, tính cả `put` lẫn `mark_out`. `None` nếu chưa gửi gì.
fn highest_out(&self) -> Option<u32>;
```

- **`mark_out` chỉ gọi khi bytes không vào journal.** Một application message được `put` giữ
  thì `put` đã là bằng chứng nó tiêu số; gọi thêm `mark_out` là ghi hai lần một sự thật.
  Quy tắc một câu: *mỗi số tiêu ra được journal nghe đúng một lần, qua `put` nếu giữ được,
  qua `mark_out` nếu không.*
- **`highest_out` không có thân mặc định**, cùng lý do `highest` và `highest_in` không có: một
  journal có giữ trạng thái không được phép nói mình không giữ. `NoJournal` trả `None`.
- **`mark_out` có thân mặc định rỗng**, như `mark_active`: một journal không sống qua restart
  không phải giả vờ.

### Session gọi `mark_out` ở đúng chỗ tiêu số, và cầm journal ở chỗ cần

- `send_as` (`lib.rs:1775`) nhận thêm `journal: &mut J`; sau `next_out += 1` gọi
  `journal.mark_out(seq)`. Mọi caller nội bộ của `send_as` đều nằm trong `step`/`tick_with`/
  `send_application`, vốn đã cầm journal — chỉ là luồn tham số xuống.
- Đường application (`:2481`, `:1871`): khi `put` trả `false`, gọi `mark_out(seq_out)`.
- **`logout_now` là ngoại lệ, và nó là đúng trường hợp `SIGTERM`.** Hàm này không có journal và
  là API public. Chọn: **thêm `logout_now_with(text, journal, emit)`**, giữ `logout_now` cũ
  nguyên nghĩa (không journal, cho caller không có gì để ghi). `Conn` đổi sang bản có journal
  ở cả ba chỗ gọi. Đây là chỗ nhỏ nhất mà nếu bỏ sót thì gate `known_gap` vẫn đỏ — nên nó là một
  mục riêng, không phải dọn dẹp.

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

*(trống — chưa bắt đầu)*
