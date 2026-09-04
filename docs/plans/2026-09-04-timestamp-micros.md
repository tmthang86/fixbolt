# Timestamp micro giây, hai chiều

> **Loại:** Plan · **Ngày:** 2026-09-04 · **Trạng thái:** Draft
> **Phạm vi:** `STATUS.md` item 45, đợt B, plan thứ ba. Chạm `codec` (`TimestampCache`,
> **hot path**), `session` (`clock::parse_utc`, `Config`), `engine` (`settings`, `clock`),
> `benches`. **Không chạm** `dict`, `transport`.
>
> **Draft viết 2026-09-04.** Khi đến lượt: xác minh lại `clock.rs` và `timestamp.rs` (số dòng
> dưới là của `main` hôm nay), và **số baseline** trong `benches/baselines.tsv` — plan này đổi
> một hàng của §8 nên phải có máy §9 hoặc đợi đợt C. Sửa rồi mới *Chờ duyệt*.
>
> **Máy chạy:** viết và test trên macOS; **số cuối cùng cần máy §9** (gộp vào đợt C nếu chưa
> có). **Thời lượng dự kiến:** 1 ngày code, cộng một lần đo.

## Bối cảnh

`52=SendingTime` của FIX 4.4 là `YYYYMMDD-HH:MM:SS` hoặc `.sss`. Từ FIX 5.0 SP2 EP và trong thực
tế nhiều venue từ 2018, `.ssssss` (micro) và `.sssssssss` (nano) là hợp lệ và thường **bắt
buộc** — MiFID II RTS 25 yêu cầu đồng hồ đồng bộ tới micro giây cho HFT, và các venue châu Âu
gửi/đòi `52=` micro.

Hôm nay: `clock::parse_utc` **từ chối mọi độ dài khác 17 và 21** (`crates/session/src/clock.rs:40`).
Một đối tác gửi `52=20260904-10:32:07.123456` → `None`. Chuyện gì xảy ra sau đó là **điều đầu
tiên plan này phải xác minh** (nhiều khả năng: coi như thiếu `52=` → Reject hoặc từ chối skew);
dù là gì, đó là một session không lên được với một venue rất bình thường, và không có test nào
nói vậy. Chiều ra: `TimestampCache` chỉ biết `SS.sss` (`crates/codec/src/timestamp.rs:12, 57`).

## Những gì đã biết chắc (2026-09-04 — xác minh lại khi làm)

| Sự thật | Nguồn |
|---|---|
| `parse_utc`: chỉ 17 hoặc 21 byte; `s[17] == '.'`, 3 chữ số | `crates/session/src/clock.rs:39–60` |
| `TimestampCache::format(millis) -> &[u8; 21]`, prefix cache theo phút, `SS.sss` mỗi lần; `[measured 2026-08-31]` 4.9 ns | `crates/codec/src/timestamp.rs`, `benches/baselines.tsv` |
| `Tick` là ms từ năm 0; skew là `abs_diff` trên ms | D13 |
| `52=` là một `Slot` trong `Template`, patch tại chỗ; **độ dài slot cố định** khi build | D9, `crates/codec/src/template.rs` |
| Engine đọc clock bằng `SystemTime::now()` → ms | `crates/engine/src/clock.rs:33` |
| `122=OrigSendingTime` đi qua cùng `parse_utc` | `lib.rs:1861` |
| QuickFIX: `TimestampPrecision=SECONDS\|MILLIS\|MICROS\|NANOS`, mặc định MILLIS | `prior-art.md` 2026-09-03 |
| Corpus: 17 byte trên dòng `I`, 21 trên `E`; không có 24/27 | `clock.rs:27` |

## Cách làm — hình dạng dự kiến

**Nhận (bắt buộc, không knob):** `parse_utc` nhận 17, 21, 24, 27 byte; phần lẻ giây được cắt
về ms cho skew (D13 giữ ms) — **không** làm `Tick` thành micro, vì mọi thứ khác (schedule,
heartbeat, skew) đều ở ms và không có lý do đổi. Test: bốn độ dài, và một cái 24 byte với `.`
sai chỗ → `None`.

**Gửi (knob):** `Config::timestamp_precision: Precision { Millis, Micros, Nanos }`, mặc định
`Millis` (QuickFIX cũng vậy; một venue chỉ nhận 21 byte sẽ Reject 24). Hai thay đổi:

1. `TimestampCache` thành `TimestampCache<const FRAC: usize>` (3/6/9) hoặc một `Precision` lúc
   tạo — chọn khi làm, tiêu chí: **không branch mỗi message** trên hot path; const generic gần
   như chắc chắn. Nguồn thời gian: engine `clock` đọc `SystemTime::now()` đã có nano; đưa
   `micros`/`nanos` xuống thay vì ms. **`Tick` vẫn ms** — cache nhận một `u64` micro hoặc nano
   riêng cho việc format.
2. `Template` slot `52=` được build với **độ dài theo precision** — build một lần, không đổi
   sau.

**Giá phải đo:** `SendingTime from the cache` 4.9 ns là ba chữ số; sáu chữ số là một phép chia
nữa. Kỳ vọng < 10 ns; **đó là một hàng §6/§8 mới và cần máy §9** — nếu chưa có máy, plan đóng
với nhãn *unmeasured* và đợt C đo.

Key file: `TimestampPrecision=MILLIS|MICROS|NANOS`.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **1 — không cấp phát, hot path codec** | `TimestampCache` đổi | `crates/codec/benches/alloc.rs` 0; `benches/serialize.rs` arm cho mỗi precision |
| 2 — session thuần | `parse_utc` rộng hơn | không alloc; skew vẫn `abs_diff` |
| 3 — 59 định nghĩa | `52=` là một trong năm tag `fields.fmt` khớp **theo hình dạng** | corpus không thấy độ dài `52=`; **test riêng** kiểm byte-level 24 byte ra khi bật |
| 10 — không số nào thiếu benchmark/máy | hàng mới | đo trên §9 hoặc nhãn *unmeasured* |
| **`no_std` của codec là mục tiêu** | `TimestampCache` không được kéo `std` | giữ nguyên hình dạng |

## Chia việc (dự kiến)

| Bước | Kết quả |
|---|---|
| 1 | **Xác minh** hôm nay session làm gì với `52=` 24 byte: test đỏ-hoặc-xanh `crates/session/tests/logon.rs::a_microsecond_sending_time_is_accepted` — ghi kết quả đầu tiên vào nhật ký trước khi sửa |
| 2 | `parse_utc` bốn độ dài; test; `122=` cùng đường |
| 3 | `TimestampCache` theo precision; `codec` bench arm ×3; alloc 0 |
| 4 | `Config::timestamp_precision`, slot `52=` theo precision; test byte-level; `Settings` key |
| 5 | Interop: QuickFIX `TimestampPrecision=MICROS` một lần mỗi chiều — bước `logon` phải ok và `52=` 24 byte thấy trong log thô |
| 6 | Docs: `CONFIGURATION.md`, `SESSION-BEHAVIOUR.md` §1 (skew với micro — nêu test), `DESIGN.md` §6 hàng cache và §8 nếu có số, `CHANGELOG.md`, `STATUS.md` |

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Slot `52=` build 21 byte, patch 24 byte → ghi đè `56=` phía sau | slot theo precision lúc build; test kiểm toàn bộ message ra qua `parse_into` + checksum |
| `TimestampCache` prefix cache theo phút vẫn đúng với micro | test qua biên phút với micro |
| Skew: `Tick` ms, `52=` micro → cắt, không làm tròn lên | test biên `.999999` → ms `999` |
| Nano trên macOS: `SystemTime` có nano nhưng độ phân giải thật thấp hơn | không test giá trị, chỉ test định dạng |
| Số 4.9 ns bị thay bằng số laptop | `bench.sh --strict` trên §9 hoặc nhãn *unmeasured* |

## Ngoài phạm vi

`Tick` micro; `TransactTime(60)` của application (là của handler); `DateTime` precision trong `dict` types.

## Nhật ký giao hàng

*(draft — chưa duyệt, chưa bắt đầu)*
