# Khôi phục chạm tới được đĩa

> **Loại:** Plan · **Ngày:** 2026-09-02 · **Trạng thái:** Đã duyệt
> *(tự viết, tự duyệt theo uỷ quyền thường trực 2026-09-01.)*
>
> **Phạm vi:** `STATUS.md` item 32 **(b)** và **(c)**. Chạm `session` (trait `Journal`) và
> `engine` (`journal`, `recovery`, `pump`). Không chạm `codec`, `dict`, `transport`.
>
> **Máy chạy:** đóng trọn vẹn trên macOS. **(a)** — `serve_sharded_hft` — chỉ chạy trên
> Linux và **nằm ngoài phạm vi**, y như ADR-0034 và ADR-0038 đã làm.

## Bối cảnh

Hai nửa của cùng một lỗ hổng, và **từng nửa một mình thì vô dụng**:

| | |
|---|---|
| **(b)** `pump` dựng một engine cụ thể, nên `Recovery<J>` là generic mà vòng phục vụ chỉ trả lời được bằng `journal::Store` — **không deployment nào dùng `FileJournal` qua `serve_with_recovery` được** | ADR-0034, *Bad, and named* |
| **(c)** không có gì lưu `Session::last_active_ms()`, nên việc đặt lại số ở ranh giới phiên (ADR-0033) chỉ sống sót qua restart nếu người gọi tự giữ cái mốc thời gian ấy ở đâu đó | ADR-0034, quyết định 5 |

Lưu `last_active_ms` vào một `FileJournal` **không có ý nghĩa gì** chừng nào vòng phục vụ chưa
dùng được `FileJournal`. Và một `FileJournal` chạy được qua vòng phục vụ vẫn **không trả lời
được câu hỏi ranh giới** nếu nó không nhớ lần cuối phiên còn sống. Nên đây là **một việc**.

`[verified 2026-09-02]` cụ thể chỗ chặn của (b) chỉ là **một ràng buộc**: engine gọi
`J::default()` khi `Recovery` trả `None`, và `FileJournal` không có `Default` — nó cần một
đường dẫn. Một `Default` cho `FileJournal` sẽ là lời nói dối.

## Những gì đã biết chắc

| Sự thật | Nguồn |
|---|---|
| Bản ghi journal: `[seq u32-le][len u32-le][bytes]`; `len == 0` là dấu inbound | `crates/engine/src/journal.rs` |
| **`34=0` không bao giờ là số hợp lệ trong FIX** — đối xứng với `len == 0`, và đó là chỗ trống duy nhất còn lại trong định dạng | FIX 4.4 |
| `Reader` đọc được toàn bộ file và **báo đuôi rách** | [ADR-0037](../decisions/ADR-0037-reading-a-journal-is-not-recovering-from-one.md) |
| `add_with_prefix_config_and_state` đã nhận `Option<Resumed<J>>`, và chỉ cần `J: Default` ở **nhánh `None`** | `crates/engine/src/lib.rs` |
| `Session::resume_at` nhận `last_active_ms`; ranh giới được kiểm ở đầu `tick` | [ADR-0033](../decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md) |
| `serve`/`serve_hft`/`pump` chạy được trên macOS; `shard.rs` thì không | `crates/engine/src/lib.rs` |

## Quyết định trung tâm

**(b) — `Recovery` tự dựng cuốn journal trắng, engine thôi đoán.** Thêm `Recovery::fresh(cfg)`,
có thân mặc định `where J: Default`. Ai dùng `Store` không phải viết gì thêm; ai dùng
`FileJournal` **ghi đè nó** và mở đúng file cho đối tác ấy. Ràng buộc `J: Default` biến khỏi
vòng phục vụ, và **không có `Default` giả nào được viết ra**.

Alias `TcpAcceptorEngine<A, W, J = journal::Store>` nhận thêm tham số **có giá trị mặc định**,
nên `shard.rs` và `tools/w2w` biên dịch y nguyên — quan trọng vì `shard.rs` là Linux-only.

**(c) — `seq == 0` là dấu thời gian.** Bản ghi `[0][8][mili-giây LE]`. `34=0` không bao giờ hợp
lệ, nên nó không thể lẫn với message, đúng cách `len == 0` đã dùng cho dấu inbound. **Định dạng
không đổi, đầu đọc dài thêm một nhánh** — chính lý lẽ mà `INBOUND_MARK` đã ghi trong code.

**Ghi khi nào là quyết định, không phải chi tiết.** **Không ghi mỗi message** — đó là hot path.
Ghi ở hai thời điểm: khi phiên **logon**, và khi **tắt máy có thứ tự**. Cái thứ hai là cái đáng
kể: nó trả lời đúng câu hỏi *"phiên còn sống lần cuối lúc nào"* cho một lần khởi động lại có kế
hoạch, và ADR-0038 vừa làm cho thời điểm ấy tồn tại.

**Người gọi vẫn quyết định.** ADR-0010 nói engine không đoán. `Journal::last_active()` chỉ là
một sự thật đọc được; biến nó thành `Resumed::last_active_ms` vẫn là việc của `Recovery`.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **2 — session thuần** | trait `Journal` thêm hai hàm | Thân mặc định **rỗng**; không clock, không alloc. Session không tự gọi chúng — engine gọi |
| **1 — không cấp phát** | ghi dấu thời gian | Bản ghi 16 byte trên stack. Case `benches/alloc.rs` hiện có phải vẫn 0 |
| **4 — luồng engine không ngủ** | `Fsync` ghi đồng bộ | **Đã là sự thật cũ** và người dùng mua nó có chủ đích (D7). Dấu thời gian không thêm tần suất nào ngoài logon và tắt máy |
| **3 — 59 định nghĩa** | trait đổi | 59/59 cả hai mode |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | **Test đặc tả, đỏ ở assertion.** Một `FileJournal` đi qua `serve_with_recovery`, và mốc thời gian sống sót qua restart. Hôm nay không làm được cả hai | — |
| 2 | `Journal::mark_active` / `last_active` (mặc định rỗng); `FileJournal` cài bằng bản ghi `seq == 0`; `Reader` sinh `Record::ActivityMark` | 1 |
| 3 | `Recovery::fresh`; `pump` và `serve_with_recovery` generic theo `J`; engine ghi dấu lúc logon và lúc tắt máy | 2 |

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 1 | `cargo test -p fixbolt-engine --test on_disk` | **đỏ ở assertion** |
| 2 | `cargo test -p fixbolt-engine --test journal_reader` | xanh; file cũ **không có** dấu thời gian vẫn đọc được |
| 3 | `cargo test -p fixbolt-engine --test on_disk` | xanh; `FileJournal` qua **socket thật** |
| 3 | `cargo bench -p fixbolt-engine --bench alloc` | 20 case cũ vẫn **0** |
| mọi bước | `--test wire` 59/59 cả hai mode; `cargo test --all`; `check-no-optional-deps.sh`; clippy; fmt; links | xanh |

**Đảo ngược, bắt buộc:**

1. `mark_active` không ghi gì → test "mốc sống sót qua restart" đỏ.
2. `Reader` đọc bản ghi `seq == 0` thành một message bình thường → test đỏ, **và** một file
   không có dấu nào vẫn phải xanh (không được sửa bằng cách đòi dấu).
3. `Recovery::fresh` bị lờ đi, engine vẫn `J::default()` → **không biên dịch được** với
   `FileJournal`. Nếu nó vẫn biên dịch được thì ràng buộc chưa thật sự biến mất.

**Bẫy đã lường trước:**

| Bẫy | Test canh |
|---|---|
| Test đọc file khi journal chưa thả | Thả trước khi đọc — kỷ luật của `tests/recovery.rs` |
| Dùng `Fsync` cho tốc độ chậm không cần | `Async` + drop, vì [the-strongest-knob-is-not-the-settle-point](../reference/the-strongest-knob-is-not-the-settle-point.md) |
| File **cũ**, chưa từng có dấu thời gian, đọc thành hỏng | Một test ghi bằng đường cũ rồi đọc bằng đường mới |
| `last_active` trả về thời điểm *mở file* thay vì thời điểm ghi | Đẩy đồng hồ giữa hai lần ghi và so sánh |

## Tài liệu phải cập nhật

- [x] [ADR-0039](../decisions/ADR-0039-a-fresh-journal-is-the-deployments-to-build.md) — `seq == 0` là dấu thời gian; `Recovery::fresh` thay cho `J: Default`
- [x] `DESIGN.md` §3; `CHANGELOG.md`; `GUIDE.md` §6b; `STATUS.md` item 32; `PRD.md`
- [x] `docs/reference/` — [a-reversal-that-must-not-compile](../reference/a-reversal-that-must-not-compile.md) mới, gắn `[to testing-skills]`
- [x] Đi lại bảng §4, đọc lại *Not proven* — ba mục mới: dấu thời gian **không** ghi theo chu kỳ, không có khoá file, và file không tự nói thang thời gian của nó là gì

## Ngoài phạm vi

- **(a) `serve_sharded_hft`** — Linux-only. Ở lại *Not proven*.
- **Ghi dấu thời gian theo chu kỳ** — cần một chính sách về tần suất, và tần suất là chuyện đo đạc.
- **Chống va chạm khi hai tiến trình mở cùng một file** — đó là một plan về khoá file.

## Nhật ký giao hàng

### Bước 1 — test đặc tả, đỏ ở assertion

`crates/engine/tests/on_disk.rs::a_journal_on_disk_remembers_when_its_session_was_last_alive`.

**Bản viết đầu đỏ ở compiler** — nó gọi `mark_active`, một hàm chưa tồn tại. Đó là lần thứ tư
trong hai ngày mắc đúng lỗi này, và nó không chứng minh gì: đỏ vì không biên dịch được chỉ nói
rằng mình chưa viết code, không nói hệ thống hôm nay làm gì.

Viết lại để hỏi bằng đúng thứ hôm nay có: **chính các byte của file**. Sau tất cả những gì một
session có thể ghi được — `put`, `mark_in` — tám byte của thời điểm ấy có nằm đâu đó trong file
không? Không. Đó là đặc tả, và nó đỏ ở assertion.

Test đối chứng `the_sequence_numbers_already_survive_a_restart` xanh ngay từ đầu: các **con số**
vốn đã sống sót qua restart. Nên thứ đang thiếu là *thời điểm*, không phải là *file*.

### Bước 2 — `Reader`, và `seq == 0` là dấu thời gian

`ACTIVITY_MARK = 0` dùng được vì `34=` của FIX bắt đầu từ 1, nên số 0 chưa từng là một bản ghi
hợp lệ. Format **không đổi**: một file cũ không có dấu nào đọc y như trước, và
`a_file_with_no_activity_mark_reads_as_it_always_did` khẳng định điều đó thay vì giả định.

### Bước 3 — `Recovery::fresh`, và engine ghi dấu ở hai thời điểm

`fn fresh(&mut self, cfg: &Config) -> J` **không có thân mặc định**. Bản viết đầu là
`fn fresh(...) -> J where J: Default { J::default() }`, và nó **không giải quyết được gì**:
mệnh đề `where` trên một thân mặc định rơi xuống **phía người gọi**, nên vòng lặp phục vụ vẫn cần
`J: Default` để gọi được — ràng buộc chỉ đơn giản là chuyển chỗ. Bắt buộc phải cài đặt method là
cách duy nhất đặt ràng buộc lên đúng nơi cần nó.

Engine ghi dấu ở **lúc logon** và **lúc tắt máy có trật tự**, không ghi theo chu kỳ — xem
*Ngoài phạm vi*, và nó nằm trong *Not proven* của `STATUS.md`.

Commit `33d1793`. `cargo test --all` **383 passed, 0 failed**; `--test wire` **59/59** ở cả
default lẫn `--no-default-features --features standard`; `cargo bench --bench alloc` **20 case,
tất cả 0**; clippy `--all-targets --all-features` sạch; `check-no-optional-deps.sh` sạch;
`check-links.py` sạch. Máy: Apple M5, macOS 15 — **không có con số nanosecond nào từ máy này**.

### Đảo ngược, đã chạy, output nguyên văn

**1 — `mark_active` không ghi gì** (`let _ = at_ms;`, không đặt field, không ghi file):

```
test result: FAILED. 3 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.99s

---- a_journal_on_disk_remembers_when_its_session_was_last_alive stdout ----
assertion `left == right` failed: and a Recovery must be able to ask for it without parsing the file
  left: None
 right: Some(63849600000000)

---- serving::a_file_journal_runs_through_the_serving_loop_and_records_when_it_lived stdout ----
the file must say when this session was alive; without it a restart cannot tell whether a trading
day ended in between — saw None
```

Ba test đỏ, và một trong ba đi qua **socket thật** — nên cái được canh là đường đi trọn vẹn, không
phải chỉ là một field trong bộ nhớ.

**2 — bản ghi `seq == 0` đọc thành một message bình thường.** Có **hai** bộ giải mã, và chỉ khi
lật cả hai thì đảo ngược mới đầy đủ. Lật riêng cái lười (`Records::next`):

```
test result: FAILED. 5 passed; 1 failed
    serving::a_file_journal_runs_through_the_serving_loop_and_records_when_it_lived
```

Lật cả hai (thêm vòng quét lúc `open`):

```
test result: FAILED. 3 passed; 3 failed
    a_journal_on_disk_remembers_when_its_session_was_last_alive
    serving::a_file_journal_runs_through_the_serving_loop_and_records_when_it_lived
    the_latest_activity_mark_is_the_one_that_answers
```

**Và `a_file_with_no_activity_mark_reads_as_it_always_did` xanh trong cả hai lần** — đó là nửa
thứ hai của yêu cầu: không được sửa bằng cách bắt mọi file phải có dấu.

Điều rút ra, và nó không có trong bảng bẫy: **một hằng số phân biệt được đọc ở hai chỗ thì phải
đảo ngược ở cả hai chỗ.** Lật một chỗ đọc ra "1 test đỏ" và nghe như đã đủ.

**3 — `pump` quay về `J::default()`:**

```
error[E0599]: no associated function or constant named `default` found for type parameter `J`
    --> crates/engine/src/lib.rs:1264:23
error: could not compile `fixbolt-engine` (lib) due to 1 previous error
```

**Đảo ngược này đỏ ở compiler, và lần này đó chính là điều cần chứng minh** — ngược hẳn với bước
1. Câu hỏi là *"ràng buộc `J: Default` có còn không?"*, và câu trả lời duy nhất thuyết phục là
trình biên dịch nói không. Một test chạy được sẽ không phân biệt nổi hai phiên bản.
