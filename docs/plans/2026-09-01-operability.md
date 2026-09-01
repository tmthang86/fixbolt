# Vận hành: đọc được engine từ một luồng khác, mà không chạm hot path

> **Loại:** Plan · **Ngày:** 2026-09-01 · **Trạng thái:** BƯỚC 1–2 ĐÓNG 2026-09-01; bước 3–6 chưa bắt đầu
> *(tự viết và tự duyệt theo uỷ quyền thường trực, STATUS.md "Start here" 2026-08-30, và chỉ
> thị của chủ sở hữu 2026-09-01.)*
>
> **Phạm vi:** `STATUS.md` open item 30. Chạm `engine` (module mới `observe`, `Engine`,
> `conn`), một accessor trên `session`, và **API công khai**. Không chạm `codec`, không chạm
> `dict`.
>
> **Máy chạy:** bước 1–4 chọn được trên macOS bằng test. Bước 5 có một nửa chỉ đóng được ở
> máy §9 và plan nói rõ chỗ nào.

## Bối cảnh

`[verified 2026-09-01]` **Toàn bộ bề mặt quan sát được của `Engine` là `connections() -> usize`**,
cộng hai bộ đếm `sources_missing()` và `refused_connections()`. Không trạng thái session, không
`next_out`/`next_in`, không độ sâu ring, không lý do ngắt kết nối. `grep` cho `shutdown`,
`drain` hay xử lý tín hiệu trong `crates/*/src` trả về **không gì cả**.

Nghĩa là: engine này chạy được, và **không vận hành được**. Sáu mảnh, và item 30 cố ý gộp
chúng làm một vì **chúng chia đúng một ràng buộc** — mọi thứ ở đây phải đọc được từ một luồng
khác mà không chạm hot path, và giải quyết cái đó một lần là phần lớn công việc.

| | Mảnh | Bước |
|---|---|---|
| (a) | Ordered shutdown — `Logout`, flush journal, chờ ack hoặc hết giờ | 4 |
| (b) | Operator snapshot — trạng thái, hai số thứ tự, ms từ inbound cuối, độ sâu ring, refusal, pending-set, **và độ lệch đồng hồ đo được** | **1–2** |
| (c) | Sửa số thứ tự trên engine đang chạy | 3 |
| (d) | Luồng sự kiện có cấu trúc — logon, logout, gap, resend, reject, disconnect **kèm lý do** | 5 |
| (e) | Trình đọc journal ngoại tuyến | 6 |
| (f) | Health probe — listener đã bind, session đã logon, journal ghi được | 2 |

**PRD open decision 9 đã được trả lời** bởi
[ADR-0027](../decisions/ADR-0027-the-engine-owes-a-byte-stream-not-an-archive.md): engine nợ
một bản sao byte trung thực ở một ranh giới, **không bao giờ là một kho lưu trữ**. Điều đó
chốt phạm vi của (d) và (e) và là lý do plan này không bị chặn nữa.

## Quyết định trung tâm: hỏi thì mới trả lời

**Không xuất bản snapshot mỗi turn.** Một engine ghi trạng thái mỗi vòng là một engine trả giá
cho một người quan sát không có mặt — và D8 nói hot path không làm việc thừa.

**Theo yêu cầu**, và giá khi không ai hỏi là **một lần `load(Relaxed)`**:

```rust
// crates/engine/src/observe.rs
pub struct Observer(Arc<Shared>);   // luồng vận hành giữ cái này

impl Observer {
    /// Xin một ảnh chụp. Trả về ảnh chụp mới nhất engine đã xuất bản.
    pub fn request(&self) -> Option<Snapshot>;
}

// Engine, ở đầu turn():
//   if shared.wanted.swap(false, Acquire) { self.publish(&shared); }
```

**Vì sao không dùng ring của D10.** Ring là đường ứng dụng và ADR-0011 nói ring đầy thì ngắt
kết nối. Một người vận hành hỏi trạng thái **không được** có khả năng làm rớt một session. Hai
cơ chế, hai mục đích, và cái này không được chia rủi ro với cái kia.

**Vì sao ảnh chụp có kích thước cố định.** Không cấp phát trên hot path (bất biến 1). Mảng
`[SessionSnapshot; MAX]` cộng một cờ `truncated`. `hft` có trần 4 session
([ADR-0025](../decisions/ADR-0025-hft-has-a-hard-session-ceiling-and-the-engine-advises-rather-than-applies.md));
`standard` không có trần, nên **`truncated` là một sự thật phải nói ra, không phải một lỗi**.

**Độ lệch đồng hồ là mảnh dễ bị bỏ quên nhất và là mảnh cứu người trực đêm.** `max_skew_ms`
âm thầm từ chối message khi NTP trôi, và hôm nay **không gì nói tại sao**. Ảnh chụp mang
`last_skew_ms` — hiệu giữa `52=` vào gần nhất và đồng hồ của engine — nên câu hỏi *"vì sao
counterparty bị từ chối"* có một chỗ để nhìn.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | **Test đặc tả, đỏ trước.** Một luồng khác đọc được trạng thái, hai số thứ tự và độ lệch đồng hồ của mỗi session. Đỏ vì hôm nay `Engine` chỉ nói được `connections()` | — |
| 2 | `observe`: `Observer`, `Snapshot`, `SessionSnapshot`. `Engine::observer()`. Bước 1 xanh. Health probe là một hàm thuần trên `Snapshot` | 1 |
| 3 | Sửa số thứ tự trên engine đang chạy: một kênh lệnh dùng lại cùng cơ chế | 2 |
| 4 | Ordered shutdown: `Logout`, flush, chờ ack hoặc hết giờ | 2 |
| 5 | Luồng sự kiện có cấu trúc, kèm **lý do** ngắt kết nối | 2 |
| 6 | Trình đọc journal ngoại tuyến | — |

**Bước 1–2 là plan này.** Bước 3–6 mỗi cái đóng riêng, và **nếu buổi làm việc kết thúc sau
bước 2 thì đó là một kết quả trọn vẹn**, không phải một plan dở dang: cái khó là cơ chế, và nó
đứng một mình.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **1 — không cấp phát trên hot path** | `publish` chạy trong `turn` | `Arc` cấp phát **một lần** ở `observer()`; ảnh chụp là mảng cố định. Case mới trong `benches/alloc.rs`, cả khi *có* và *không có* người hỏi |
| **4 — luồng engine không ngủ trong kernel** | `publish` không được khoá | Không mutex. Hai bộ đệm và một bộ đếm thế hệ; người đọc thử lại. Không có gì chặn luồng engine |
| **2 — session thuần** | cần một accessor đọc | `session` chỉ thêm hàm đọc; không đổi hành vi. 59 định nghĩa là cổng |
| **7 — không `unwrap`/`expect`/`panic`** | API công khai | `request()` trả `Option`; không có gì để `unwrap` |

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 1 | `cargo test -p fixbolt-engine --test observe` | **đỏ**, và thông điệp nói *engine không nói được trạng thái session* |
| 2 | như trên | xanh; một **luồng thật** đọc ảnh chụp trong khi engine đang chạy |
| 2 | `cargo test -p fixbolt-engine --test wire` | **59/59, cả hai mode** |
| 2 | `cargo bench --bench alloc` | `observe-idle 0` và `observe-asked 0` |
| mọi bước | `cargo test --all`, `--no-default-features`, clippy `-D warnings`, `fmt`, `check-links.py` | xanh |

**Đảo ngược, bắt buộc:**

1. Cho `publish` không ghi `next_in` → test số thứ tự phải đỏ, và **chỉ nó**.
2. Cho `publish` chạy vô điều kiện mỗi turn → case `observe-idle` trong `benches/alloc.rs`
   vẫn 0 (nó không cấp phát), nhưng `benches/turn.rs` phải **chậm đi đo được**. Đây là phép
   đảo ngược duy nhất của plan này cần máy §9, và nó **không đóng ở đây**.
3. Xoá cờ `wanted`, luôn xuất bản → không test nào đỏ. **Đó là lỗ hổng**, và bước 2 phải thêm
   một test đếm số lần `publish` chạy, nếu không "theo yêu cầu" là một lời hứa không ai canh.

**Cái bẫy lớn nhất:** một test đọc ảnh chụp *sau khi* engine dừng sẽ xanh với một cơ chế không
an toàn giữa các luồng. Ảnh chụp phải được đọc **trong khi** engine đang quay, từ một luồng
khác, hoặc test không đo cái nó nói.

## Tài liệu phải cập nhật

- [x] ADR mới — cơ chế quan sát: theo yêu cầu, kích thước cố định, không dùng ring của D10
- [x] `DESIGN.md` §3 — module `observe`
- [x] `CHANGELOG.md` — API công khai
- [x] `GUIDE.md` — mục vận hành, thứ hôm nay **không tồn tại**
- [x] `STATUS.md` item 30 — thu hẹp, không đóng
- [x] Đi lại bảng §4 từng dòng, và đọc lại *Not proven* từng dòng

## Ngoài phạm vi

- **Bước 3–6** — mỗi cái một lần đóng riêng.
- **Audit tap** — ADR-0027 nói nó là tính năng riêng, không chia store với journal.
- **Định dạng số liệu (Prometheus, v.v.)** — `Snapshot` là dữ liệu; ai muốn export thì viết.
- **Xử lý tín hiệu** — thuộc bước 4 và thuộc `library`, không thuộc `engine`.

## Nhật ký giao hàng

> Điền khi đóng từng bước.

### Bước 1 — test đặc tả, đỏ trước (2026-09-01)

`crates/engine/tests/observe.rs` chạy engine trên **một luồng riêng** và đọc từ luồng test
trong lúc nó đang quay. Đỏ đúng chỗ:

```
timed out waiting for the session to log on; last snapshot: Some(Snapshot { sessions: [...],
len: 0, truncated: false, connections: 1, refused_connections: 0, sources_missing: 0 })
```

`connections: 1` chứng minh cơ chế theo-yêu-cầu đã chạy; `len: 0` là đúng cái lỗ hổng bước 2
lấp. Test anh em xanh ngay từ bước 1, vì nó nói về cơ chế chứ không nói về nội dung:

```
test the_engine_publishes_nothing_until_it_is_asked ... ok
```

### Bước 2 — nội dung ảnh chụp (2026-09-01)

`Engine::snapshot` đọc `self.conns`: `id`, `is_logged_on`, `next_out`, `next_in`,
`last_skew_ms`, `has_pending_output`. `Session` có thêm trường `last_skew_ms: Option<i64>`,
**ghi trước khi phán quyết `SendingTime`**, không phải sau. `Snapshot::healthy()` là mảnh (f).

```
test an_operator_sees_session_state_from_another_thread ... ok
test the_engine_publishes_nothing_until_it_is_asked ... ok
test result: ok. 2 passed; 0 failed
```

**Ba phép đảo ngược của plan, cộng hai cái nữa mà việc này đòi thêm.**

1. `snapshot` không ghi `next_in` → **đỏ đúng một test, đúng một assertion**:
   `assertion left == right failed: the Logon was 34=1, so the next inbound expected is 2:
   SessionSnapshot { id: 0, logged_on: true, next_out: 2, next_in: 0, last_skew_ms: Some(0),
   has_pending_output: false }`. Không test nào khác đổi.
2. `publish` chạy vô điều kiện mỗi turn → **cần máy §9, KHÔNG đóng ở đây.** Nửa cấp phát thì
   đóng được và đã đóng: `observe-idle 0`, `observe-asked 0`. Nửa nanosecond thì không, và
   `STATUS.md` *Not proven* có một bullet mới nói đúng điều đó.
3. Xoá cờ `wanted` → `the_engine_publishes_nothing_until_it_is_asked` đỏ:
   `left: 84555, right: 0`. Trong 50 ms engine đã dựng 84 555 ảnh chụp mà không ai hỏi. Lỗ
   hổng plan cảnh báo giờ có người canh.
4. **Đảo dấu độ lệch đồng hồ** (`now >= t` thành `now < t`) → `tests/observe.rs` **vẫn xanh**,
   vì corpus và engine dùng cùng một mốc thời gian nên `Some(0)` đọc như nhau theo cả hai
   chiều. Ba test trong `crates/session/tests/skew.rs` đỏ. Đây là lý do file đó tồn tại.
5. **Ghi độ lệch chỉ khi message được chấp nhận** → `a_message_refused_for_skew_still_records_
   the_skew_that_refused_it` đỏ, `left: None, right: Some(200000)`. Đúng cái trường hợp trường
   này sinh ra để giải thích.

**Cổng, chạy trên macOS 2026-09-01:**

| Lệnh | Kết quả |
|---|---|
| `cargo test --all` | **292 passed, 0 failed** |
| `cargo test --all --no-default-features` | **292 passed, 0 failed** |
| `cargo test -p fixbolt-engine --test wire` | `the_fifty_nine_definitions_pass_through_a_real_socket` **ok**, `..._in_standard_mode_too` **ok** (mỗi cái assert `report.passed == 59`) |
| `cargo test -p fixbolt-conformance --test fix44` | `a_session_that_answers_correctly_scores_fifty_nine` **ok** |
| `cargo test -p fixbolt-session --test score` | `step_six_b_replays_what_it_sent_and_scores_fifty_nine` **ok** |
| `cargo bench --bench alloc` | `observe-idle 0 observe-asked 0`, và cả 15 case đều 0 |
| `cargo clippy --all-targets --all-features -D warnings` | sạch |
| `cargo fmt --all` | sạch |
| `scripts/check-links.py` | 702 liên kết, 0 chết |

**Không chạy được ở đây, và không phải là xanh:** `shard_wire.rs`, `check-no-kernel-sleep.sh`,
`check-standard-gives-the-core-back.sh` (Linux-only), và mọi con số nanosecond —
`benches/baselines.tsv` khoá theo CPU model.

**Tài liệu đã đi từng dòng:** ADR-0032 mới; `DESIGN.md` §3 có module `observe`;
`CHANGELOG.md` có API công khai của cả `engine` và `session`; `GUIDE.md` §8a là mục vận hành
trước đây không tồn tại; `STATUS.md` item 30 thu hẹp còn (a)(c)(d)(e) chứ **không đóng**;
`PRD.md` §2 dòng *Operator visibility* đổi từ *gap* sang *narrowed*; ba bullet mới trong
*Not proven*.

**Trạng thái plan: bước 1–2 ĐÓNG. Bước 3–6 chưa bắt đầu** — và plan đã nói trước rằng dừng ở
đây là một kết quả trọn vẹn.
