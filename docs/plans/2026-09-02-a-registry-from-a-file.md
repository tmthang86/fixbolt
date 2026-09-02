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

## Sửa plan giữa chừng, ghi lại tại đây

**Thêm hai hằng số công khai vào `fixbolt-session`:** `MAX_BEGIN_STRING_LEN` và
`MAX_COMP_ID_LEN`. Plan viết rằng parser phải từ chối một giá trị quá dài, nhưng không nói
parser **lấy giới hạn ở đâu**. Viết `32` trong `engine` là một luật thứ hai, và cái sai sẽ là
cái quyết định một đối tác có được phục vụ hay không. `Config` khai báo các trường theo hai hằng
số này, nên chỉ có một chỗ định nghĩa. Đây là thay đổi API công khai của một crate, nên nó phải
đi kèm `DESIGN.md` và `CHANGELOG.md` — đã làm.

## Nhật ký giao hàng

### Bước 1 — test đặc tả, đỏ ở assertion

`crates/engine/tests/settings.rs`, chạy trước khi có `settings.rs`:

```
---- two_counterparties_named_only_in_a_file_are_both_served stdout ----
assertion `left == right` failed: the file names two counterparties and the acceptor must serve
exactly those two — adding one is an operator's edit, not a release
  left: 0
 right: 2
```

`table_from_file` là **cái mối nối**, và ở bước 1 nó trả `Table::new()`. Đó không phải một hàm
giả thay cho hàm còn thiếu: nó là **câu trả lời đúng của hôm nay** — crate không dựng được gì từ
file — và ADR-0026 quyết định 6 làm cho câu trả lời đó chính xác: bảng rỗng từ chối mọi kết nối.

Hai tiền đề được **khẳng định** chứ không giả định, vì cả hai đều có thể làm test đỏ vì lý do
khác: đổi tên `TW44` thành `TW44` phải cho lại đúng từng byte của corpus, và file với corpus phải
đồng ý acceptor này là `ISLD`. Sai `SenderCompID` và không biết đối tác bị từ chối **giống hệt
nhau**.

### Bước 2 — parser, và những gì nó từ chối

18 test. Ba luật khác QuickFIX, mỗi luật một test, và mỗi luật một lý do cụ thể chứ không phải
"chặt chẽ hơn cho chắc".

### Bước 3 — giờ giao dịch từ file

30 test. **Các test của bước này viết sau code, không phải đỏ trước** — nói thẳng ra ở đây, và
thứ thay thế cho red-first là đảo ngược số 4 dưới đây.

Assertion so sánh **cả `Config`** với một `Schedule` dựng bằng tay, chứ không dò qua `contains`:
số học là của ADR-0033 và đã có test riêng; việc của parser là truyền đúng hai con số vào đó.
Mỗi test giờ giấc kèm một `assert_ne!` với chính `Config` đó nhưng không lịch — đúng thứ mà một
`StartTime` bị bỏ qua sẽ để lại.

### Bước 4 — một file thật, một socket thật

3 test qua `serve`: hai đối tác chỉ có tên trong file cùng logon; một danh tính file không nêu
không nhận được gì; một đối tác có cửa sổ giao dịch **đã đóng hai tiếng trước** bị từ chối trong
khi một đối tác đang mở được phục vụ. Cửa sổ được **tính từ đồng hồ** chứ không viết cứng, vì
vòng lặp phục vụ dùng đồng hồ thật và một hằng số sẽ xanh buổi sáng, đỏ lúc sáu giờ chiều.

### Đảo ngược, đã chạy, output nguyên văn

| # | Phá cái gì | Kết quả |
|---|---|---|
| 1 | khoá lạ bị bỏ qua thay vì báo lỗi | **17 passed; 1 failed** — đúng `a_mistyped_key_is_refused_and_not_ignored` |
| 2 | `[DEFAULT]` không thừa kế xuống `[SESSION]` | **9 passed; 9 failed** |
| 2b | `[DEFAULT]` **thắng** `[SESSION]` | **17 passed; 1 failed** — đúng `a_session_overrides_the_default_block` |
| 3 | file không có `[SESSION]` trả bảng rỗng | **16 passed; 2 failed** |
| 4 | lịch phân tích xong bị vứt đi | **25 passed; 5 failed**, và `a_file_with_no_hours_leaves_the_neutral_schedule` **vẫn xanh** |

**Đảo ngược 2 không phân biệt được như plan đã dự đoán.** Plan viết rằng test ghi đè `HeartBtInt`
sẽ vẫn xanh vì nó "không dựa vào thừa kế". Sai: mọi khối `[SESSION]` trong mọi fixture đều lấy
`BeginString` và `SenderCompID` từ `[DEFAULT]`, nên bỏ thừa kế làm gần như tất cả đỏ. Nó chứng
minh **thừa kế có tồn tại** và **không nói gì về thứ tự ưu tiên**. Đảo ngược 2b là nửa còn thiếu,
và nó mới là cái tách được đúng một test.

### Bẫy thứ năm cùng hình dạng, và lần này hai nguyên nhân không nằm cùng một tầng

Một đảo ngược **không có trong plan**, chạy như một phép thử vu vơ — chỉ giữ lại đối tác **đầu
tiên** khi đổ `Settings` vào `Table` — làm cả ba test bước 4 **vẫn xanh**.

Đương nhiên. Một danh tính registry không phục vụ bị từ chối trong im lặng (ADR-0026 quyết định
3), và một đối tác ngoài giờ cũng bị từ chối trong im lặng (ADR-0033). Test đặt tên cho nguyên
nhân thứ hai và đang đo nguyên nhân thứ nhất.

**Bản vá đầu tiên cũng không vá được gì**: khẳng định rằng `Settings` đã phân tích có đủ hai đối
tác — chạy lại, **vẫn xanh**, vì chỗ mất mát nằm ở bước sau, trong `into_table`. Một khẳng định
đặt ở *đầu vào* của một đường ống hai chặng không nói gì về *đầu ra* của nó.

Khẳng định đúng là trên chính vật được trao cho acceptor: độ dài của `Table`, kiểm ngay tại thời
điểm truyền vào. Với cùng đảo ngược đó, **hai trong ba test đỏ**.

Viết đầy đủ ở [two-time-rules-share-one-observable](../reference/two-time-rules-share-one-observable.md),
mục thứ tư, gắn `[to testing-skills]`.
