# Danh sách đối tác đến từ một file, không từ code

> **Loại:** Plan · **Ngày:** 2026-09-02 · **Trạng thái:** Đã duyệt
> **Phạm vi:** Phase 1 — `many counterparties`, phần còn thiếu

## Bối cảnh

`presession::Table` phục vụ được nhiều đối tác từ 2026-09-01, nhưng **cách duy nhất để đưa một
đối tác vào là viết Rust rồi biên dịch lại**:

```rust
Table::with_capacity(3)
    .serving(Config::acceptor(b"FIX.4.4", b"ISLD", b"TW44"))
```

`docs/PRD.md` gọi thẳng chỗ này là *"Still missing: a CONFIG FILE (built in code today)"*, và nó
là dòng cuối cùng của nhánh `many counterparties` còn để trống.

**Vì sao đây là một khiếm khuyết thật, không phải sự tiện tay.** Thêm một đối tác vào một acceptor
đang chạy production là việc của người vận hành, thường vào buổi tối trước ngày đối tác đó lên
UAT. Bắt việc đó phải đi qua một lần biên dịch nghĩa là:

- người vận hành phải có toolchain Rust và source, tức là phải có quyền vào repo;
- mọi thay đổi cấu hình đều là một lần release binary, nên **thay một `HeartBtInt` và sửa một lỗi
  hot path là cùng một loại rủi ro** trong mắt quy trình;
- không có cách nào so sánh cấu hình của hai môi trường ngoài việc đọc hai bản Rust.

Mọi engine FIX từng được triển khai thật đều có file cấu hình, và không phải vì tiện.

## Những gì đã biết chắc

- `Config` (`crates/session/src/lib.rs:255`) mang đúng năm thứ: `begin_string`, `sender_comp_id`,
  `target_comp_id`, `max_skew_ms`, `heart_bt_int`, cộng `schedule`. Không có gì khác.
- `Schedule` ([ADR-0033](../decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md))
  có `daily(start, end)`, `weekly(...)`, `always()`, `with_weekdays`, `with_utc_offset_ms`, **tất
  cả trả `Option`** — một khoảng giờ vô lý đã bị từ chối ở tầng dưới, plan này không kiểm tra lại.
- `Table::serving(cfg)` là builder, và khoá tra cứu **chính là `Config::serves`** — không có khoá
  riêng nào có thể lệch khỏi `Config`.
- [ADR-0026](../decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md) quyết định 6:
  **bảng rỗng từ chối tất cả**, không có mẫu ký tự đại diện, không có `ANY_SESSION`.
- `Registry::lookup` **không được cấp phát** và `benches/alloc.rs` case `registry-lookup` đang đọc
  **0**. Việc đọc file xảy ra lúc khởi động, không đụng vào đó.
- QuickFIX (C++ và J) dùng file INI: `[DEFAULT]` và nhiều khối `[SESSION]`, khoá `BeginString`,
  `SenderCompID`, `TargetCompID`, `StartTime`, `EndTime`, `HeartBtInt`. Đó là **dữ liệu và một
  hình dạng**, không phải source — [ADR-0001](../decisions/ADR-0001-relationship-to-quickfix.md)
  cho phép; `NOTICE` không phát sinh.

## Cách làm

Module mới `crates/engine/src/settings.rs`, **không thêm dependency nào**. Không `serde`, không
`toml`: format này là *tên khoá = giá trị*, và một parser 150 dòng rẻ hơn hai crate cộng lại một
cây phụ thuộc mà `codec` đã đứng ngoài.

### Format — hình dạng của QuickFIX, tập khoá của repo này

```ini
[DEFAULT]
BeginString=FIX.4.4
SenderCompID=ISLD
StartTime=08:00:00
EndTime=17:00:00

[SESSION]
TargetCompID=TW44

[SESSION]
TargetCompID=BANZAI
HeartBtInt=60
StartTime=00:00:00
EndTime=00:00:00
```

`[DEFAULT]` đặt giá trị cho mọi khối `[SESSION]` sau nó; mỗi `[SESSION]` ghi đè phần của nó. Người
vận hành FIX nào cũng đọc được ngay, và đó là toàn bộ lý do chọn hình dạng này.

Khoá được nhận: `BeginString`, `SenderCompID`, `TargetCompID`, `HeartBtInt`, `MaxSkewMillis`,
`StartTime`, `EndTime`, `StartDay`, `EndDay`, `Weekdays`.

### Ba quyết định trong parser, mỗi cái có lý do

1. **Khoá lạ là lỗi, không phải bỏ qua.** QuickFIX bỏ qua khoá nó không biết. Ở đây một khoá gõ
   sai — `TargetCompId`, `Starttime` — sẽ **im lặng rơi về mặc định**, và mặc định của lịch là
   `always()`: một phiên đáng ra đóng lúc 5 giờ chiều trở thành mở suốt ngày. Đây đúng là hình
   dạng ADR-0026 quyết định 6 đã từ chối một lần rồi.
2. **File rỗng, hoặc không có `[SESSION]` nào, là lỗi.** Một `Table` rỗng từ chối mọi kết nối, nên
   một file gõ sai đường dẫn sẽ biểu hiện y hệt một firewall chặn — hai nguyên nhân, một hiện
   tượng, và đây là bài học `two-time-rules-share-one-observable` đã trả tiền ba lần.
3. **Lỗi mang theo số dòng.** Cấu hình được sửa bởi người không đọc Rust; *"dòng 14: khoá không
   biết `Starttime`"* là thứ dùng được, `ParseError` thì không.

### File sẽ tạo hoặc sửa

| File | Việc |
|---|---|
| `crates/engine/src/settings.rs` | **mới** — `Settings::from_str`, `Settings::load`, `into_table()`, `SettingsError` |
| `crates/engine/src/lib.rs` | khai báo `mod settings;` |
| `crates/engine/tests/settings.rs` | **mới** — test đặc tả và các case lỗi |
| `crates/engine/tests/settings_wire.rs` | **mới** — một file thật, một socket thật, hai đối tác |

## Bất biến bị đụng tới

| Non-negotiable | Ảnh hưởng |
|---|---|
| 1 — không cấp phát trên hot path | **Có cấp phát, và đúng chỗ**: parser chạy lúc khởi động, `Table` đã cấp phát ở `with_capacity`. `benches/alloc.rs` case `registry-lookup` phải vẫn **0** |
| 2 — session thuần khiết | Không đụng: parser nằm ở `engine`, `session` không biết đến file |
| 6 — feature gate cho dependency ngoài | **Không thêm dependency nào**, nên không phát sinh feature. `check-no-optional-deps.sh` phải vẫn sạch |
| 7 — không `unwrap`/`expect`/`panic` trong crate thư viện | Mọi lỗi là `SettingsError`, có số dòng |
| 9 — không copy source QuickFIX | Chỉ mượn **hình dạng file**, tự viết parser |

## Chia việc

| Bước | Nội dung | Số test |
|---|---|---|
| 1 | Test đặc tả: một file hai đối tác dựng được `Table` phục vụ cả hai — **đỏ ở assertion** | 1 |
| 2 | Parser: `[DEFAULT]` + `[SESSION]`, năm khoá bắt buộc, lỗi có số dòng | 8 |
| 3 | Lịch từ file: `StartTime`/`EndTime`/`StartDay`/`EndDay`/`Weekdays` | 4 |
| 4 | Một file thật qua socket thật: hai đối tác vào cùng một engine | 1 |

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 1 | `cargo test -p fixbolt-engine --test settings` | **đỏ ở assertion** |
| 2–3 | `cargo test -p fixbolt-engine --test settings` | xanh |
| 4 | `cargo test -p fixbolt-engine --test settings_wire` | xanh, qua socket thật |
| mọi bước | `--test wire` 59/59 cả hai mode; `cargo test --all`; `cargo bench --bench alloc` 20 case vẫn 0; `check-no-optional-deps.sh`; clippy; fmt; links | xanh |

**Đảo ngược, bắt buộc:**

1. Khoá lạ được bỏ qua thay vì báo lỗi → test "khoá gõ sai không được im lặng" **đỏ**.
2. `[DEFAULT]` không được thừa kế xuống `[SESSION]` → test hai đối tác **đỏ**, và test đối tác
   ghi đè `HeartBtInt` **vẫn xanh** (nó không dựa vào thừa kế) — hai kết quả khác nhau là thứ nói
   rằng bộ test phân biệt được.
3. File không có `[SESSION]` nào trả về `Table` rỗng thay vì lỗi → test **đỏ**. Đây là đảo ngược
   quan trọng nhất: nó là hình dạng *"một nguyên nhân đọc ra giống nguyên nhân khác"*.

**Bẫy đã lường trước:**

| Bẫy | Test canh |
|---|---|
| `Name<32>` cắt bớt một CompID quá dài mà không ai biết | Một test đưa CompID 40 byte và đòi **lỗi**, không phải cắt |
| `StartTime=17:00:00`, `EndTime=08:00:00` — phiên vắt qua nửa đêm | `Schedule::daily` đã xử lý; test khẳng định file **truyền đúng** hai số vào nó |
| Hai `[SESSION]` trùng hệt nhau → `Table` có hai dòng, tra cứu ra dòng đầu | Test đòi **lỗi**: cấu hình trùng là lỗi người viết, không phải một luật ưu tiên |
| CRLF từ máy Windows làm giá trị mang theo `\r` | Test đọc một file có CRLF |
| Test đọc file trong `/tmp` dùng chung tên → hai test đá nhau | Cùng kỷ luật `tmp(name)` của `tests/on_disk.rs`: pid + thread id |

## Tài liệu phải cập nhật

- [ ] ADR mới — vì sao INI viết tay chứ không phải TOML/serde, và vì sao khoá lạ là lỗi
- [ ] `DESIGN.md` §3 (module mới) + §6 (bảng chứng cứ); `CHANGELOG.md`; `GUIDE.md`; `PRD.md`
- [ ] `README.md` nếu file cấu hình xuất hiện trong ví dụ khởi động
- [ ] Đi lại bảng §4, đọc lại *Not proven*

## Ngoài phạm vi

- **Credential / entitlement.** ADR-0026 quyết định 3 nói `lookup` trả `None` **chính là** chỗ xác
  thực, và không có `AuthStrategy` thứ hai. Một trường mật khẩu trong file này sẽ là hook thứ hai.
- **Đường dẫn journal cho từng đối tác.** Đọc được từ file thì dễ, nhưng nó thuộc `Recovery` chứ
  không thuộc `Registry` — `Entry` chỉ mang `Config`. Là một plan riêng, sau ADR-0039.
- **Nạp lại cấu hình khi đang chạy.** Bảng là read-only sau khởi động, và đổi điều đó là một
  quyết định về đồng bộ hoá trên đường kết nối, không phải về format file.
- **`50=` / `57=` trong file.** `Config` không có chỗ chứa; ADR-0026 đã nói một deployment cần
  chúng thì viết `Registry` của riêng nó.
