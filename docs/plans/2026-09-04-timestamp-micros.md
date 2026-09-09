# Timestamp micro giây, hai chiều

> **Loại:** Plan · **Ngày:** 2026-09-04 · **Sửa lại:** 2026-09-08 (Sửa 1) · **Trạng thái:** ĐÓNG 2026-09-09 — nửa A 09-08, nửa B 09-09
> **Phạm vi:** `STATUS.md` item 45, đợt B, plan thứ ba. Chạm `codec` (`TimestampCache`,
> **hot path**), `session` (`clock::parse_utc`, `Config`, `Session::stamp`), `engine`
> (`settings`, `clock`), `conformance`, `benches`. **Không chạm** `dict`, `transport`.
>
> **Máy chạy:** nửa A không cần máy §9. **Nửa B cần máy §9 và một ADR trước khi viết dòng code
> nào.** **Thời lượng dự kiến:** nửa A một ngày; nửa B một ngày cộng một lần đo, sau ADR.

---

## Draft này đã được xác minh lại và nó sai một chỗ lớn

`[2026-09-08]` Item 45 đòi: khi tới lượt, `Những gì đã biết chắc` phải đối chiếu lại với code
của ngày hôm đó rồi mới chuyển sang *Chờ duyệt*. Đã làm, và **năm chỗ lệch**, một trong đó đổi
hình dạng của cả plan:

**1 — Hậu quả của một `52=` micro giây giờ đã biết chắc, không còn là phỏng đoán.** Draft viết
*"nhiều khả năng: coi như thiếu `52=` → Reject hoặc từ chối skew"*. Đọc thẳng đường đi
(`crates/session/src/lib.rs:2830` → `:2840` → `:2854`) thì cụ thể hơn và tệ hơn:

```
52= 24 byte  →  parse_utc = None  →  time_ok = false
             →  state == AwaitingLogon  →  Err(Refusal::BadSendingTime)
```

`AwaitingLogon` **cắt kết nối trong im lặng** — comment ngay trên đó nói rõ vì sao: trước
`Logon` chưa có session để trả lời. Nghĩa là một venue châu Âu gửi `52=` micro giây **không lên
được session và không nhận được một byte giải thích nào**. Đó là hành vi dành cho timestamp
*sai*, đang áp cho một timestamp *hợp lệ*.

**2 — Và đây là chỗ draft sai: session KHÔNG THỂ gửi micro giây thật, dù có sửa `TimestampCache`.**
Draft viết *"engine `clock` đọc `SystemTime::now()` đã có nano; đưa `micros`/`nanos` xuống thay
vì ms"*. Nhưng `TimestampCache` **nằm trong `Session`** (`crates/session/src/lib.rs:1179`), và
nó được cho ăn `self.now_ms` (`:2489`), thứ chỉ được gán ở đúng một chỗ:

```
tick_inner(now_ms) → self.now_ms = now_ms          crates/session/src/lib.rs:1921
```

`now_ms` đến từ `Input::Tick`, đơn vị mili giây (D13). **Session là thuần (bất biến 2): thời
gian chỉ vào được bằng `Tick`.** Nên không có đường nào đưa micro giây xuống cache mà không đổi
đơn vị thời gian của session — và `SystemClock::now_ms` phía engine cũng cắt sẵn bằng
`d.as_millis()` (`crates/engine/src/clock.rs:35`) nên nguồn cũng không có sẵn.

Ba lựa chọn, **và không lựa chọn nào là hiển nhiên**, nên nó thuộc về một ADR chứ không thuộc
về plan này:

| | Cách | Giá |
|---|---|---|
| (a) | `Tick` mang micro giây | Đổi D13. Mọi thứ khác — schedule, heartbeat, skew, `last_skew_ms`, journal — đang là ms và **không có lý do gì để đổi**. Rủi ro tràn ra rất rộng cho một tính năng hẹp |
| (b) | Một đầu vào thứ hai mang phần lẻ dưới ms, chỉ dùng để format `52=` | Session vẫn thuần, `Tick` vẫn ms. Nhưng thêm một khái niệm thời gian thứ hai vào một lớp mà cả thiết kế dựa trên chỗ nó chỉ có một |
| (c) | Format 24 byte nhưng ba chữ số cuối luôn `000` | Rẻ nhất, chạy được ngay, **và là một lời nói dối về độ phân giải**. MiFID II RTS 25 đòi đồng hồ tới micro giây; gửi `.123000` cho một venue đang đo là tệ hơn gửi `.123` |

**3 — Con số 4.9 ns đã là số máy §9 rồi.** `benches/baselines.tsv:141`: `AMD Ryzen 7 3700X`,
`SendingTime from the cache`, **4.9 ns**, n = 20, `[2026-09-05]`, `pass 12 fail 0 unknown 1`.
Draft lo *"số 4.9 ns bị thay bằng số laptop"* — cái nền thì không còn rủi ro đó nữa; rủi ro nằm
ở **số mới** phải đo cùng máy cùng cách thì mới so được.

**4 — `TimestampCache` có bốn nơi dùng, không phải một.** Ngoài session còn
`crates/conformance/src/script.rs:523`, `crates/engine/src/msglog.rs:448` và `:459`. Một const
generic trên kiểu sẽ **chạm cả bốn**, và message log không có lý do gì phải đổi độ chính xác
theo session. Bước 3 vì thế rộng hơn draft ghi.

**5 — Số dòng đã lệch.** `clock.rs` nhận thêm `#![allow(clippy::indexing_slicing)]` ở đầu file
`[2026-09-08]` (item 55), nên `parse_utc` giờ ở `:46` chứ không phải `:40`. `122=` đi qua
`lib.rs:2917`, không phải `:1861`.

**Vì vậy plan này tách làm hai nửa, có một cửa ở giữa.** Nửa A đóng khoảng trống sản phẩm và
không cần quyết định kiến trúc nào. Nửa B không được bắt đầu trước khi có ADR-0057.

---

## Bối cảnh

`52=SendingTime` của FIX 4.4 là `YYYYMMDD-HH:MM:SS` hoặc `.sss`. Từ FIX 5.0 SP2 EP và trong
thực tế nhiều venue từ 2018, `.ssssss` (micro) và `.sssssssss` (nano) là hợp lệ và thường **bắt
buộc** — MiFID II RTS 25 yêu cầu đồng hồ đồng bộ tới micro giây cho HFT, và các venue châu Âu
gửi lẫn đòi `52=` micro.

Hôm nay engine này **từ chối cả hai chiều**: nhận thì cắt kết nối im lặng (mục 1 ở trên), gửi
thì `TimestampCache` cứng 21 byte (`crates/codec/src/timestamp.rs:19`).

Không có test nào nói vậy. Đó là điều bước 1 sửa trước tiên.

## Những gì đã biết chắc (xác minh lại 2026-09-08)

| Sự thật | Nguồn |
|---|---|
| `parse_utc`: chỉ 17 hoặc 21 byte; `s[17] == '.'`, 3 chữ số | `crates/session/src/clock.rs:46–89` |
| Một `52=` không đọc được → `time_ok=false` → `AwaitingLogon` → `Refusal::BadSendingTime`, **im lặng** | `crates/session/src/lib.rs:2830, 2840, 2854` |
| `TimestampCache` nằm trong `Session`, ăn `self.now_ms` | `crates/session/src/lib.rs:1179, 2489` |
| `self.now_ms` chỉ được gán từ `Input::Tick`, đơn vị ms | `crates/session/src/lib.rs:1921`; D13 |
| `SystemClock::now_ms` cắt bằng `as_millis()` | `crates/engine/src/clock.rs:35` |
| `TimestampCache::format(millis) -> &[u8; 21]`, prefix cache theo phút | `crates/codec/src/timestamp.rs:19, 50–70` |
| Bốn nơi dùng `TimestampCache` | `session/src/lib.rs`, `conformance/src/script.rs:523`, `engine/src/msglog.rs:448, 459` |
| `SendingTime from the cache` **4.9 ns**, máy §9, n = 20 | `benches/baselines.tsv:141` |
| `122=OrigSendingTime` đi qua cùng `parse_utc` | `crates/session/src/lib.rs:2917` |
| `settings.rs` có **25** key hôm nay | `crates/engine/src/settings.rs:139–165` |
| QuickFIX: `TimestampPrecision=SECONDS\|MILLIS\|MICROS\|NANOS`, mặc định MILLIS | `docs/reference/prior-art.md` |
| Corpus: 17 byte trên dòng `I`, 21 trên `E`; không có 24/27 | `crates/session/src/clock.rs:34` |
| `skew.rs` đã có sẵn `good_logon()` và `skew_at()` để dựng test | `crates/session/tests/skew.rs:34, 60` |

## Cách làm

### Nửa A — nhận (không cần ADR, không cần máy §9)

**Bắt buộc, không knob.** Một timestamp hợp lệ thì đọc được; không ai muốn một key để bật
"chấp nhận thứ mà spec nói là hợp lệ".

`parse_utc` nhận **17, 21, 24, 27** byte. Phần lẻ giây cắt (truncate, không làm tròn) về ms cho
skew — **`Tick` vẫn ms, D13 không đổi**, vì skew, schedule và heartbeat đều ở ms và không có
gì đòi đổi. `122=` đi cùng đường nên được miễn phí.

File: `crates/session/src/clock.rs`, `crates/session/tests/skew.rs`, một test mới cho `122=`.

### Nửa B — gửi (cửa: ADR-0057 trước, rồi mới code)

**ADR-0057 quyết một câu**: session lấy phần lẻ dưới mili giây ở đâu, giữa (a), (b) và (c) ở
bảng trên. Plan này **không quyết thay** — nó chỉ nói rõ rằng không quyết thì không viết được,
và rằng (c) rẻ nhất nhưng là lời nói dối về độ phân giải.

Sau khi ADR được duyệt, hình dạng dự kiến:

1. `TimestampCache<const FRAC: usize>` (3/6/9) — const generic để **không branch mỗi message**
   trên hot path. Bốn nơi dùng đều phải nêu tên độ chính xác của mình; `msglog` giữ `3`.
2. `Config::timestamp_precision`, mặc định `Millis` (QuickFIX cũng vậy; một venue chỉ nhận 21
   byte sẽ Reject 24).
3. `Template` slot `52=` build **theo precision**, một lần lúc build, không đổi sau (D9).
4. Key thứ 26: `TimestampPrecision=MILLIS|MICROS|NANOS`.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **1 — không cấp phát, hot path** | `TimestampCache` đổi (nửa B) | `crates/codec/benches/alloc.rs` 0; `benches/serialize.rs` một arm mỗi precision |
| **2 — session thuần** | `parse_utc` rộng hơn (A); **nguồn thời gian dưới ms (B) là chỗ dễ vi phạm nhất** | A: không alloc, skew vẫn `abs_diff`. B: **ADR-0057 phải nói rõ thời gian vào bằng đường nào; không có `SystemTime` trong `session`** |
| 3 — 59 định nghĩa | corpus không có 24/27 byte nên **nó không thể thấy gì cả** | chạy đủ 59 ở cả hai nửa như một chốt chặn không-lùi, và **nói rõ rằng nó không phải bằng chứng cho việc này** |
| 5 — thứ tự field từ bảng sinh | slot `52=` đổi độ dài (B) | slot theo precision lúc build; test byte-level qua `parse_into` + checksum |
| 7 — không `panic`/`unwrap` | `clock.rs` đang mang `#![allow(clippy::indexing_slicing)]` cả file (item 55) | **code mới trong file này dùng `get`, không dùng `s[i]`** — và nếu dọn được cả file thì `check-indexing-debt.sh` phải hạ trần trong cùng commit |
| 10 — mọi số có benchmark + máy | hàng §6/§8 mới (B) | đo trên máy §9 cùng cách với 4.9 ns, hoặc đóng với nhãn *unmeasured* và để đợt C đo |
| `no_std` của `codec` là mục tiêu | `TimestampCache` không được kéo `std` | giữ nguyên hình dạng, không nhận `SystemTime` |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| **A1** | **Test đỏ trước:** `crates/session/tests/skew.rs::a_microsecond_sending_time_is_read_and_the_link_survives` dựng trên `good_logon()` sẵn có, khẳng định hành vi **mong muốn** — link sống, skew đo được, có byte trả lời. Đỏ hôm nay. Chạy, **chép output đỏ vào nhật ký trước khi sửa bất cứ gì** (Sửa 1) | — |
| **A2** | `parse_utc` nhận 17/21/24/27; cắt về ms. Test: bốn độ dài; `.` sai chỗ ở 24 byte → `None`; biên `.999999` → ms `999` (cắt, không làm tròn) | A1 |
| **A3** | `122=OrigSendingTime` cùng đường — một test riêng, vì nó là một `Refusal` khác | A2 |
| **A4** | A1 xanh, **không sửa một assertion nào**. `cargo test --all`, `--no-default-features`, **59/59** | A2, A3 |
| **A5** | Docs nửa A: `SESSION-BEHAVIOUR.md` (nêu tên test canh), `CHANGELOG.md`, `STATUS.md`, và **`docs/reference/`**: một venue hợp lệ bị cắt kết nối im lặng vì một độ dài field — đúng loại bẫy §4 đòi ghi lại, kèm nhãn `[to testing-skills]` nếu bài học là về test | A4 |
| — | **CỬA: ADR-0057 viết, duyệt.** Không bước nào dưới đây bắt đầu trước | A5 |
| **B1** | `TimestampCache<const FRAC>`; bốn nơi dùng nêu tên precision; `codec` bench arm ×3; alloc 0 | ADR-0057 |
| **B2** | `Config::timestamp_precision` + slot `52=` theo precision; test byte-level toàn message | B1 |
| **B3** | Key `TimestampPrecision`, key thứ 26 | B2 |
| **B4** | Interop: QuickFIX `TimestampPrecision=MICROS` một lần mỗi chiều; `52=` 24 byte phải thấy trong log thô | B3 |
| **B5** | Đo trên máy §9, hoặc nhãn *unmeasured*. Docs: `CONFIGURATION.md`, `DESIGN.md` §6/§8, `CHANGELOG.md`, `STATUS.md` | B4 |

## Cách kiểm chứng

**Nửa A không được đóng bằng "test pass".** Bằng chứng là:

1. **A1 đỏ trước**, output chép nguyên văn — đó là câu duy nhất chứng minh khoảng trống có thật.
2. **A4 xanh sau**, cùng một test, không sửa assertion. Nếu phải sửa assertion để nó xanh thì
   đó là fixture bị uốn cho code, đúng thứ §10 bảo tự canh.
3. **Đảo chiều A2**: bỏ 24 khỏi danh sách độ dài → test 24 byte phải đỏ, test 17/21 vẫn xanh.
   Xác nhận nó đỏ **đúng ở assertion mình định**, không phải đỏ vì compile lỗi.
4. **59/59 ở cả A1 và A4**, và ghi rõ: **corpus không có 24 byte nên nó không chứng minh gì cho
   việc này** — nó chỉ chứng minh không làm hỏng thứ đang chạy.
5. `cargo test --all` **và** `cargo test --all --no-default-features`, cộng
   `scripts/check-no-optional-deps.sh` vì `--no-default-features` một mình đã từng xanh về một
   build không xảy ra.

Nửa B thêm: `benches/alloc.rs` 0 trên mọi case, `scripts/bench.sh --strict` đọc từng dòng chứ
không đọc exit code, và **CI xanh trên đúng commit đóng plan**, nêu id.

## Tài liệu phải cập nhật

- [x] `docs/SESSION-BEHAVIOUR.md` — hành vi biên với `52=`, nêu tên test canh (A5)
- [x] [`docs/reference/a-valid-field-refused-for-its-width.md`](../reference/a-valid-field-refused-for-its-width.md) — một field hợp lệ bị cắt kết nối im lặng vì độ dài (A5)
- [x] `CHANGELOG.md` — nửa A xong; `codec` chờ nửa B — API công khai của `session`, rồi `codec` (A5, B5)
- [x] `STATUS.md` — nửa A xong; đã đọc *Not proven* từng dòng và thêm một dòng mới — mục *Start here*, item 45, **và đọc từng dòng mục *Not proven*** (A5, B5)
- [x] `docs/decisions/ADR-0057-*.md` — nguồn thời gian dưới mili giây (cửa) — `Accepted` 2026-09-09
- [x] `docs/CONFIGURATION.md` — key `TimestampPrecision` (B5)
- [x] `docs/DESIGN.md` D9 — hàng cache mới kèm số đo (B5). **§6/§8 không đụng**: không hàng nào của ngân sách độ trễ đổi, và một hàng thêm vào đó mà không có số máy §9 thì là một lời hứa
- [x] `docs/GUIDE.md` §5b — ba ràng buộc kiểu không giữ được (B5)
- [x] `docs/SESSION-BEHAVIOUR.md` §1b — ba độ rộng đi ra (B5)
- [x] `docs/CONFORMANCE.md` — hàng `interop-micros:` 5 / 5 (B5)
- [x] [`docs/reference/one-field-two-readers.md`](../reference/one-field-two-readers.md) — một field, hai bộ đọc (B4)

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| **A1 xanh ngay từ đầu** — nghĩa là mình hiểu sai đường đi, không phải là không có lỗi | Bắt buộc chép output A1 vào nhật ký **trước** khi sửa code. Xanh thì dừng plan lại, không sửa tiếp |
| `parse_utc` nhận 24 byte nhưng `.` sai chỗ vẫn lọt | Test `20260908-10:32:07:123456` → `None` |
| Cắt phần lẻ thành làm tròn lên → skew lệch 1 ms | Test biên `.999999` → ms `999` |
| Corpus 59/59 bị đọc như bằng chứng cho micro giây | Ghi thẳng vào nhật ký rằng corpus không có 24 byte; §7 "real captures over invented messages" ở đây **không có capture nào**, và điều đó phải nói ra |
| Slot `52=` build 21 byte, patch 24 byte → ghi đè `56=` phía sau | Slot theo precision lúc build; test kiểm toàn message qua `parse_into` + checksum |
| Prefix cache theo phút sai với micro | Test đi qua biên phút ở mỗi precision |
| `msglog` bị đổi độ chính xác theo session một cách vô tình | `msglog` nêu `3` tường minh; test log thô vẫn 21 byte |
| Code mới trong `clock.rs` thêm nợ indexing mà `#![allow]` cả file nuốt mất | Dùng `get`; `scripts/check-indexing-debt.sh` chạy ở cả hai nửa (bài học item 58: một `#![allow]` ở đầu file làm câm cả crate) |
| Số mới đo trên máy khác máy đã cho ra 4.9 ns | Cùng máy §9, cùng `n = 20`, cùng build đã ghim alignment (ADR-0049); không đủ thì nhãn *unmeasured* |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| ADR-0057 chọn (a) và D13 phải đổi | Cao | Đó chính là lý do có cửa. Nửa A giao được độc lập, nên nếu ADR bế tắc thì vẫn có sản phẩm |
| Máy §9 cần hai lần reboot (grub, `[2026-09-05]`) | Trung bình | Nửa A không cần máy. Nửa B đóng với nhãn *unmeasured* và dồn vào đợt C nếu chưa reboot |
| Không có capture thật của venue gửi `52=` micro | Trung bình | Nói thẳng trong nhật ký. QuickFIX ở bước B4 là ý kiến thứ hai duy nhất có thật |
| Nửa B phình sang `Template`, `dict` types | Thấp | `dict` nằm trong *Ngoài phạm vi*; `Template` chỉ đổi độ dài slot |

## Ngoài phạm vi

`Tick` micro giây (thuộc ADR-0057, không thuộc plan); `TransactTime(60)` của application (của
handler); `DateTime` precision trong `dict` types; `SECONDS` precision khi gửi (QuickFIX có,
đây chưa ai hỏi).

## Nhật ký giao hàng

`[2026-09-09]` **NỬA B ĐÓNG, đủ năm bước B1–B5.** ADR-0057 được duyệt trong ngày và chuyển
`Accepted`. Kết quả: `TimestampPrecision` là key thứ 26, `52=` đi ra ở 21/24/27 byte, và
`scripts/interop.sh` có một kịch bản mới xử theo **byte** chứ không theo dòng bước.

**Cái đắt nhất của cả plan không nằm trong plan.** Bước B4 chạy lần đầu: session logon xong, rồi
**mọi message sau Logon bị `35=3 … 371=52 373=6`**. Nửa A đã mở rộng `parse_utc` — nhưng `52=`
có **hai** bộ đọc, và bộ thứ hai, `dict::FieldType::UtcTimestamp`, vẫn chỉ nhận 8 hoặc 12 byte
giờ. Không một gate nào trong repo này thấy được: test nửa A chạy trên một `Logon`, corpus không
có stamp nào rộng hơn 21 byte, và test của `dict` liệt kê đúng những giá trị `dict` đã nhận.
**Thứ tìm ra nó là engine của người khác.**
[one-field-two-readers](../reference/one-field-two-readers.md), có nhãn `[to testing-skills]`.

**Bằng chứng, theo thứ tự nó xảy ra.**

**B1 đỏ trước, ở assertion chứ không phải ở compile:**

```
assertion `left == right` failed: a cache that cannot write more than
milliseconds cannot serve a venue that requires microseconds
  left: 21
 right: 24
```

Nói thẳng một chỗ plan không lường: **assertion đó không bao giờ xanh được**, vì mặc định phải
ở lại mili giây. Nó chứng minh khoảng trống có thật, không phải chứng minh hành vi đích — hành
vi đích cần một kiểu chưa tồn tại lúc chạy đỏ. Mẫu *"cùng một test, không sửa assertion"* của
nửa A **không chuyển được** sang một bước *thêm API*. Test thay thế nêu tên chuyện này.

**Đảo chiều cái guard quan trọng nhất.** Đổi `Precision::coarser` cho nó chọn cái mịn hơn:

```
assertion `left == right` failed: a millisecond tick must not be padded out to
Micros: 20260828-12:00:00.000000
  left: 24
 right: 21
```

`20260828-12:00:00.000000` — đúng lời nói dối phương án (c) bị bác. Guard biến việc ADR bác bỏ
(c) thành tính chất của kiểu chứ không phải một câu trong tài liệu.

**B4, gate interop, đọc trên transcript của chính QuickFIX:**

```
interop-micros: 24-byte 52= — 12 from fixbolt, 10 from libquickfix
interop-micros: 21-byte 52= — 0 (must be 0)
interop-micros: 35=3 naming tag 52 — 0 (must be 0)
interop-micros: PASS 5/5
```

Đảo chiều: đặt đầu này về `TimestampPrecision=3` → `0 from fixbolt`, `12` stamp mili giây, gate
đỏ. **Và phải sửa `initiator.cpp` mới có bằng chứng**: nó chỉ in transcript khi thất bại, nên
một lần chạy *thành công* không có bằng chứng nào cả — thêm `--dump-tape`. Bản đầu của §4h còn
**chết im lặng**: nó glob một file log QuickFIX không bao giờ ghi, và dưới `set -o pipefail` thì
script tắt mà không in một dòng.

**B5, đo trên máy này chứ không phải máy §9**, ba lần mỗi arm, một biến một lần:

| | ns/op |
|---|---|
| trước thay đổi, cùng máy | **4.7** |
| trả về slice thay cho `&[u8; 21]`, chưa có branch | **4.8** |
| như đã ship, mặc định `Millis` | **5.4** |
| `Micros` | **10.5** |
| `Nanos` | **13.0** |

**+0.7 ns ở mặc định, trong đó branch runtime 0.6 và slice return 0.1.** Đó là giá của việc chọn
trường runtime thay cho const generic, và là con số ADR-0057 quyết định 5 hỏi. **Số 4.9 ns không
được đo lại** — nó là số máy §9 và máy này là VM chia sẻ.

**Một regression bị bắt và bị sửa giữa đường.** Bản đầu gộp cả ba precision vào một vòng lặp:
**4.7 → 6.4 ns**, +36% ở mặc định cho những chữ số mặc định không viết. Tách nhánh mili giây
thành thẳng hàng đưa về 5.4. Và **ratchet indexing bắt bản đầu**: 184 → 186 vì một index tính
được và một slice tính được; viết lại bằng slice pattern đưa về 184. Nhân đó biết thêm một điều
về chính cái ratchet: `clippy::indexing_slicing` **không** báo với index hằng vào mảng cỡ cố
định — nó đếm cái *không chứng minh được*, và đó mới là loại panic.

**Cái nửa B KHÔNG làm:** không đo trên máy §9 (cần hai lần reboot, `[2026-09-05]`), nên mọi số
ở trên mang nhãn máy của chúng; `benches/baselines.tsv` **không thêm dòng nào**. `SECONDS`
precision vẫn ngoài phạm vi. Và `STATUS.md` item 59 **vẫn mở**: `dict` giờ khớp bốn độ rộng của
`parse_utc`, nhưng oracle nhận 17–27, nên sáu precision nó gửi được vẫn bị đây từ chối.


`[2026-09-09, sau khi duyệt]` **Sửa 2, viết trước khi code chạm vào bất cứ file nào.**
ADR-0057 được duyệt trong ngày, `Accepted`. Đọc lại code để mở nửa B thì **ba dòng của plan này
sai**, và một trong ba là một cái bẫy plan tự dựng lên cho một mối nguy **không tồn tại trong
codebase này**.

**Sửa 2a — B1 bỏ `const FRAC`, dùng trường runtime.** Plan viết
`TimestampCache<const FRAC: usize>` "để không branch mỗi message". Nhưng **B2 lại đặt precision
vào `Config`**, thứ đọc từ file cấu hình lúc chạy. Hai cái đó không đi được với nhau: một const
generic bắt `Session<N, APP, FRAC>`, tức là engine phải `match` một giá trị runtime thành **ba
bản monomorphise** của cả session lẫn engine, chạm cả mười chữ ký `serve*`. Đó là cái giá rất
lớn cho một branch mà bộ dự đoán nhánh đoán đúng mọi lần (cùng một giá trị suốt đời session).
ADR-0057 quyết định 5 để chuyện này cho plan, **với đúng một ràng buộc**: đo lại với
`benches/baselines.tsv:141` — `SendingTime from the cache`, **4.9 ns**, AMD Ryzen 7 3700X,
n = 20 — **cùng máy, cùng cách**. Nếu không có máy §9 thì đóng với nhãn *unmeasured* và nói rõ.

**Sửa 2b — bẫy "slot `52=` build 21 byte, patch 24 byte → ghi đè `56=`" KHÔNG CÓ THẬT ở đây.**
`Template::encode_with` viết `Part::Slot(tag)` bằng **đúng giá trị được đưa vào**, dài bao nhiêu
cũng được, rồi tính `9=` từ vị trí ghi thực tế (`crates/codec/src/template.rs:585–590`,
`body_len = w - k`). Không có slot cố định độ rộng, và không có đường patch tại chỗ: mỗi message
được encode lại từ đầu. Đường application cũng vậy — `rebuild` gọi
`b.field(tag::SENDING_TIME, now)` với một slice (`crates/session/src/lib.rs:3652`). **Test
byte-level toàn message ở B2 vẫn giữ**, nhưng nó canh số học `9=`/`10=` chứ không canh một vụ
ghi đè. *Một bẫy được canh nhầm chỗ vẫn tốn tiền như một bẫy thật.*

**Sửa 2c — nửa B phải mở một cửa tick thứ hai, và plan không lường.** ADR-0057 quyết định 1 đòi
phần lẻ đi **cùng một lời gọi** với `now_ms`. `tick`/`tick_with` hôm nay có **102 nơi gọi**
(`grep`, 2026-09-09), trong đó **100 là test và bench**; đổi chữ ký là 100 lần sửa cơ học cho
một tính năng mà 100 chỗ đó không hề chạm tới. Nên:

- `tick_at(now_ms, sub_ms_nanos, emit)` và `tick_at_with(...)` — cửa mang cả hai số, một lời
  gọi, một lần đọc đồng hồ. Đây là cửa engine dùng.
- `tick`/`tick_with` **giữ nguyên chữ ký và đổi nghĩa cho rõ**: *một mili giây, và không có độ
  phân giải nào mịn hơn*.

**Và điều đó tự mở một lỗ mà plan phải bịt ngay**: một session cấu hình `MICROS` nhưng được
đánh nhịp qua cửa mili giây sẽ in `.123000` — **đúng lời nói dối của phương án (c), vào bằng
cửa sau**. Nên session ghi lại **độ phân giải nó được đưa**, và format ở `min(cấu hình, được
đưa)`. Một session `MICROS` bị `tick()` in **21 byte**, không phải `.xxx000`. Có test canh, và
đó là test biến việc ADR bác bỏ (c) thành một tính chất của kiểu chứ không phải một câu trong
tài liệu.

**Sửa 2d — cách viết giá trị của key.** `TimestampPrecision` nhận **số nguyên**, để một `.cfg`
dùng chung với đầu QuickFIX C++ đọc được ở cả hai bên (ADR-0057, bảng oracle). Nhận `3`, `6`,
`9`; **mọi giá trị khác là lỗi cấu hình lúc nạp, không phải clamp im lặng** — ADR-0057 câu hỏi
mở 3 nói rõ hơn nào là trung thực. `0` (SECONDS) vẫn nằm trong *Ngoài phạm vi* của chính plan
này.

`[2026-09-09]` **Cửa đã có trang, chưa có chữ ký.**
[ADR-0057](../decisions/ADR-0057-sub-millisecond-time-arrives-beside-the-tick.md) viết xong,
trạng thái `Proposed`. Nó chọn **(b)** — phần lẻ dưới mili giây đi **cạnh** `now_ms`, **trong
cùng một lời gọi, từ cùng một lần đọc đồng hồ** — và **không đảo D13**. Nửa B vẫn **chưa bắt
đầu**: bước B1 chỉ chạy sau khi ADR được duyệt.

**Hai điều viết ADR mới tìm ra, và lần xác minh lại 2026-09-08 đã không thấy:**

1. **Engine đọc đồng hồ một lần mỗi lượt** (`crates/engine/src/lib.rs:950`; `conn.rs:292` nói
   thẳng điều đó), và `Session::received` **không nhận thời gian nào cả** (`lib.rs:2063`). Nên
   `52=` đang đặt tên cho **thời điểm lượt bắt đầu**, không phải thời điểm `send`. Ở mili giây
   không ai phải quan tâm; ở micro giây thì phải, nếu không sáu chữ số kia là một tuyên bố về độ
   phân giải mà engine không có. Thành quyết định 3 của ADR và một câu trong `GUIDE.md`.
2. **Dòng `TimestampPrecision=SECONDS|MILLIS|MICROS|NANOS` trong bảng *Những gì đã biết chắc* dẫn
   nguồn `prior-art.md`, và tài liệu đó chưa bao giờ có cách viết ấy.** Đó là của
   QuickFIX/**J**. Đọc thẳng source QuickFIX **C++** ở đúng SHA đã pin: key nhận **số nguyên
   0–9**, mặc định **3**, và bộ phân tích nhận **mọi độ dài 17–27**. Hệ quả tốt:
   `scripts/interop.sh` **làm oracle được cho cả hai chiều** — thứ `369` không bao giờ có. Hệ
   quả xấu: một dòng nhìn như có nguồn đã **sống sót qua chính lần xác minh lại** này. Xác minh
   *dữ kiện* không phải là xác minh *xuất xứ*.

Bảng *Những gì đã biết chắc* **chưa sửa** — nó là bản ghi của ngày 2026-09-08 và ADR-0057 là chỗ
ghi cái đúng. Nếu ADR được duyệt, Sửa 2 sẽ sửa dòng đó cùng lúc với việc mở nửa B.

`[2026-09-08]` **NỬA A ĐÓNG, đủ năm bước A1–A5.** Chủ dự án duyệt trong ngày. Nửa B **chưa bắt
đầu** và vẫn nằm sau cửa ADR-0057.

**Bằng chứng, theo thứ tự nó xảy ra.**

**A1 đỏ trước khi sửa một dòng code nào**, và đỏ đúng ở assertion đã định:

```
thread 'a_microsecond_sending_time_is_read_and_the_link_survives' panicked at
crates/session/tests/skew.rs:197:5:
assertion `left == right` failed: a valid microsecond SendingTime is not a reason to hang up
  left: Dropped
 right: Up
```

**A2** — `parse_utc` nhận 17/21/24/27, cắt về ms. Ba test đơn vị mới trong `clock.rs`. **Và
bốn chỗ `s[i]` của file bị dọn luôn**, nên `#![allow(clippy::indexing_slicing)]` ở đầu
`clock.rs` biến mất và `scripts/check-indexing-debt.sh` **đỏ ngay lần chạy đầu**: *"the debt
went DOWN, 188 -> 184, and the ceiling still says 188"*. Trần hạ xuống 184 trong cùng commit,
đúng như bảng *Bất biến bị đụng tới* đòi. `CLAUDE.md` §2 chép con số đó trong văn xuôi nên
cũng sửa theo, và điều đó được nói thẳng ra ở `STATUS.md`.

**A3** — và fixture đầu tiên **sai theo cách thứ hai**. `122=` để `.123456` trong khi `52=`
vẫn `.000` thì `122=` trỏ tới một thời điểm **sau** chính message chở nó: đó là
`2f_PossDupOrigSendingTimeTooHigh`, `373=10`, Logout. Test đỏ ở assertion cái link chứ không
phải ở assertion nó được viết ra để kiểm. Đổi sang `.000456` — cắt về đúng mili giây của
`52=` — nên bây giờ test **còn đỏ thêm** nếu phần dưới mili giây bị giữ lại hay bị làm tròn
lên.

**Đảo chiều hai lần, không phải một.** Bỏ `24`: bốn test đỏ, mỗi cái ở assertion của riêng nó,
và `application.rs` in ra nguyên văn cái Reject của hành vi cũ:

```
8=FIX.4.4|9=100|35=3|34=2|49=ISLD|52=20260828-12:00:00.000|56=TW44|45=2|
58=Required tag missing|371=122|372=0|373=1|10=238|
```

Bỏ `27` riêng: hai assertion nano đỏ, phần còn lại xanh. Một lần đảo chiều bỏ cả hai độ dài
thì không phân biệt được nhánh nano sống với nhánh nano chết.

**A4 — gate, đọc từng dòng chứ không đọc exit code** (bài học `[measured 2026-09-08]` của
chính `STATUS.md`: một `| tail` từng biến `build failed` thành `exit 0`):

| Gate | Kết quả |
|---|---|
| `cargo test --all` | **594 passed, 0 failed** (nền 589) |
| `cargo test --all --no-default-features` | **589 passed, 0 failed** (nền 584) |
| 59 định nghĩa | **59 / 59**, `score.rs::step_six_b_replays_what_it_sent_and_scores_fifty_nine` |
| `cargo fmt --all --check` | sạch (chạy `--check` riêng, ra file, 0 byte) |
| `cargo clippy --all-targets -- -D warnings` | 0 warning, 0 error |
| `scripts/check-indexing-debt.sh` | 184 / trần 184, ok |
| `scripts/check-no-optional-deps.sh` | ok cả ba crate |
| `benches/alloc.rs` (engine) | 0 trên đủ **30** case |
| `benches/alloc.rs` (codec) | 0 trên đủ 6 case |
| `cargo doc -D rustdoc::broken_intra_doc_links` | 0 warning |

**Chưa chạy, và nói thẳng chứ không để ngầm hiểu:** `scripts/interop.sh` — máy này không có
`cmake`. Nó cũng sẽ không nói được gì về việc này: QuickFIX mặc định gửi mili giây, còn key
micro giây thuộc về nửa B chưa xây. `scripts/bench.sh --strict` cũng chưa chạy: `parse_utc`
nằm trên đường nhận, nhưng thay đổi là một `match` trên `s.len()` thay cho hai phép so sánh và
`benches/alloc.rs` là gate mà §7 đòi cho một thay đổi không cấp phát; con số Criterion nếu đo ở
đây cũng là số của container, không phải số máy §9, nên nó sẽ là một dòng không được phép trích
theo bất biến 10.

**A5** — `docs/reference/a-valid-field-refused-for-its-width.md` (`[to testing-skills]`),
`SESSION-BEHAVIOUR.md` §1a + §5, `CHANGELOG.md`, `STATUS.md` (*Start here*, item 45, item 55,
*Where the work is*, và một dòng mới trong *Not proven*: **không có capture thật nào của một
venue gửi `52=` micro giây** — cái này dựng từ spec và từ MiFID II, không dựng từ bằng chứng).

**Cái nửa A KHÔNG làm:** engine vẫn **gửi** 21 byte ở mọi precision, không có key nào đổi được,
nên một venue đòi `52=` micro giây **từ** engine này vẫn chưa được phục vụ. Đó là nửa B, và cửa
ADR-0057 vẫn đóng.

`[2026-09-08]` **Sửa 1, viết trước khi code chạm vào bất cứ file nào.** Plan tự mâu thuẫn ở
đúng một chỗ và bản sửa này chọn nửa chặt hơn. *Chia việc* đặt tên bước A1 là
`a_microsecond_sending_time_is_refused_today` — một test khẳng định hành vi **hôm nay**, tức là
**xanh** ngay từ đầu — rồi A4 bảo *"đảo chiều"* nó. Nhưng *Cách kiểm chứng* mục 1 và 2 đòi
**"A1 đỏ trước"** và **"A4 xanh sau, cùng một test, không sửa assertion"**, còn bảng *Bẫy đã
lường trước* nói thẳng: *"A1 xanh ngay từ đầu → dừng plan lại"*. Hai câu sau không thể đúng
cùng lúc với cái tên kia. Chọn theo *Cách kiểm chứng*: A1 khẳng định hành vi **mong muốn**, đỏ
hôm nay, xanh sau A2, **assertion không đổi một chữ**. Lý do không phải hình thức — một test
được viết để xanh rồi sau đó bị lật đúng là hình dạng "fixture bị uốn cho code" mà `CLAUDE.md`
§10 bảo tự canh, và nó sẽ là chính tay mình uốn.

`[2026-09-08]` **Draft 2026-09-04 được xác minh lại và sửa, chuyển sang *Chờ duyệt*.** Năm chỗ
lệch, ghi ở đầu file. Chỗ lớn nhất: draft giả định engine đưa micro giây xuống cache được, mà
session là thuần và `Tick` là ms — nên nửa gửi cần một ADR chứ không cần thêm code. Plan tách
đôi vì thế. **Chưa có dòng code nào.**
