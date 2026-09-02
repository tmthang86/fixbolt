# Vì sao một kết nối kết thúc

> **Loại:** Plan · **Ngày:** 2026-09-02 · **Trạng thái:** Xong
> *(tự viết, tự duyệt theo uỷ quyền thường trực 2026-09-01.)*
>
> **Phạm vi:** `STATUS.md` item 30 (d). Chạm `session` (một accessor đọc, không đổi hành vi)
> và `engine` (mở rộng `observe`). Không chạm `codec`, `dict`, `transport`.
>
> **Máy chạy:** đóng trọn vẹn trên macOS.

## Bối cảnh

`Link::Dropped` là **một bit**. `[verified 2026-09-02]` session trả về nó ở 18 chỗ khác nhau —
sai BeginString, sai danh tính, `SendingTime` lệch quá xa, số thứ tự quá thấp, không phải
`Logon`, ngoài giờ giao dịch, `Logout` của đối tác, hết giờ heartbeat — và **ở đầu bên kia
không có gì phân biệt được chúng**. Engine thêm vào đó vài lý do của riêng nó: ring đầy
(ADR-0011), consumer chậm (D10), socket chết.

Cái giá đã trả **hai lần chỉ trong phiên làm việc này**, và cả hai lần đều tốn hàng giờ:

| | |
|---|---|
| Một test lịch phiên **xanh** vì `max_skew_ms` từ chối, không phải vì lịch. Hai quy tắc thời gian, một observable im lặng | [two-time-rules-share-one-observable](../reference/two-time-rules-share-one-observable.md) |
| Một `Logon` bị từ chối trong im lặng vì `FieldIndex` quá nhỏ, trong khi thông điệp lỗi đổ cho một registry chưa tồn tại | [silence-before-a-logon-has-many-causes](../reference/silence-before-a-logon-has-many-causes.md) |

Cả hai write-up đều kết luận cùng một câu: **cách phòng thủ rẻ nhất là làm cho lý do nhìn thấy
được.** Plan này làm đúng thế.

## Những gì đã biết chắc

| Sự thật | Nguồn |
|---|---|
| D8 cấm log trên hot path; `tracing` sau feature flag không phải audit trail | `DESIGN.md` §4 |
| **Không dùng ring của D10**: ring đầy thì ngắt kết nối, và người quan sát không được làm rớt session | [ADR-0011](../decisions/ADR-0011-a-full-ring-disconnects.md), [ADR-0032](../decisions/ADR-0032-observation-is-a-snapshot-taken-on-request.md) |
| Cơ chế quan sát đã có: `Arc<Shared>`, `try_lock` không bao giờ `lock`, mảng cố định | ADR-0032 |
| Mẫu "session ghi một sự thật, engine đọc lại" đã dùng rồi và thuần: `Session::last_skew_ms` | ADR-0032 quyết định 5 |
| `Link` chỉ có `Up`/`Dropped` và 18 chỗ trả `Dropped` | `crates/session/src/lib.rs` |

## Quyết định trung tâm

**Session ghi lý do vào một trường; engine đọc lại.** Giống hệt `last_skew_ms`, và vì cùng lý
do: không đổi chữ ký `Link` (một API break lan khắp nơi), không thêm allocation, không đổi
hành vi. `DropReason` là enum **không trường**, D1 giữ nguyên.

**Sự kiện đi cùng cơ chế `observe` đã có, không phải một cơ chế thứ hai.** ADR-0032 đã trả giá
cho `try_lock`/mảng cố định/`Arc` một lần; hai cơ chế song song là hai thứ sẽ bất đồng.

**Sự kiện bị mất thì phải nói ra.** Buffer cố định, và một bộ đếm `dropped`. Một luồng sự kiện
âm thầm mất mát tệ hơn không có luồng nào — đó chính là bài học của hai write-up ở trên.

**Khác snapshot ở một điểm:** snapshot là *theo yêu cầu*; sự kiện thì **engine đẩy khi có**,
vì một sự kiện không hỏi đúng lúc là một sự kiện mất. Giá phải trả: một `try_lock` mỗi sự
kiện — và sự kiện là chuyện hiếm (logon, logout, gap, disconnect), **không phải mỗi message**.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **2 — session thuần** | thêm một trường + accessor | Enum không trường, không clock, không alloc. 59 định nghĩa là cổng |
| **1 — không cấp phát** | engine ghi sự kiện | Mảng cố định trong `Shared`. Case mới trong `benches/alloc.rs`: một session logon rồi rớt, **có** observer, phải đọc 0 |
| **4 — luồng engine không ngủ** | ghi cần khoá | `try_lock`, không bao giờ `lock`. Thất bại thì tăng `dropped` |
| **3 — 59 định nghĩa** | đường session đổi | 59/59 cả hai mode |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | **Test đặc tả, đỏ ở assertion.** Hai kết nối rớt vì hai lý do khác nhau; test đòi phân biệt được. Hôm nay cả hai chỉ là `Dropped` | — |
| 2 | `DropReason` + `Session::last_drop_reason()`. Ghi ở **mọi** chỗ trả `Refusal` và mọi chỗ đặt `Disconnected` | 1 |
| 3 | `observe::Event`, `EventKind`, buffer + `dropped`; `Observer::events(&mut [Event]) -> usize`. Engine phát khi session lên/xuống | 2 |

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 1 | `cargo test -p fixbolt-session --test drop_reason` | **đỏ ở assertion**, hai lý do đọc ra như nhau |
| 2 | như trên | xanh, và mỗi lý do có một test riêng |
| 3 | `cargo test -p fixbolt-engine --test events` | xanh; sự kiện đọc được từ **luồng khác** |
| 3 | `cargo bench -p fixbolt-engine --bench alloc` | `events-idle 0`, `events-busy 0` |
| mọi bước | `--test wire` 59/59 cả hai mode; `cargo test --all`; `check-no-optional-deps.sh`; clippy; fmt; links | xanh |

**`scripts/check-no-optional-deps.sh` nằm trong danh sách vì hôm nay nó đã bắt được một lỗi mà
`cargo test --all --no-default-features` không bắt được.** Không lặp lại.

**Đảo ngược, bắt buộc:**

1. `last_drop_reason` luôn trả một hằng số → test phân biệt đỏ, **59/59 vẫn xanh** (corpus
   không nhìn thấy lý do).
2. Bỏ bộ đếm `dropped`, để buffer đầy thì ghi đè im lặng → phải có test đỏ. Nếu không thì
   "mất mát được báo" là lời hứa không ai canh.
3. `publish` sự kiện dùng `lock` thay vì `try_lock` → không test nào đỏ. **Đó là lỗ hổng**, và
   nó không đóng được bằng test — ghi vào *Not proven*.

**Bẫy đã lường trước:**

| Bẫy | Test canh |
|---|---|
| Ghi lý do **sau** khi state đã đổi → đọc ra lý do của lần trước | Một test hai lần rớt liên tiếp vì hai lý do khác nhau |
| Chỉ ghi ở đường `Refusal`, quên đường `tick` (hết giờ, ngoài lịch) | Một test cho mỗi nhóm; và enum khớp `match` không có `_` |
| Sự kiện đọc sau khi engine dừng → xanh với cơ chế không an toàn | Đọc **trong khi** engine quay, từ luồng khác — như `tests/observe.rs` |

## Tài liệu phải cập nhật

- [x] ADR mới — [ADR-0035](../decisions/ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md)
- [x] `DESIGN.md` §3; `CHANGELOG.md`; `GUIDE.md` §8a; `STATUS.md` item 30; `PRD.md`
- [x] Đi lại bảng §4, đọc lại *Not proven* — thêm ba mục mới, sửa hai mục cũ đã hơi lệch
- [x] `docs/reference/` — [a-benchmark-measured-its-own-fixture](../reference/a-benchmark-measured-its-own-fixture.md), gắn `[to testing-skills]`

## Ngoài phạm vi

- **Audit tap** — ADR-0027 nói nó là tính năng riêng.
- **Định dạng xuất** (JSON, Prometheus) — `Event` là dữ liệu.
- **Sự kiện mức message** (mỗi message vào/ra) — đó là hot path và D8 cấm.

## Nhật ký giao hàng

### Bước 1 — test đặc tả, đỏ ở assertion

`crates/session/tests/drop_reason.rs`, chạy trước khi có `DropReason`. Đỏ ở **assertion**, không
phải ở compiler — dùng đúng API hôm nay (`(Link, Vec<String>)`: link và các byte đi ra, tức là
**toàn bộ** những gì quan sát được):

```
---- two_connections_that_end_for_different_reasons_are_distinguishable stdout ----
assertion `left != right` failed: and they are different faults with different fixes, so
nothing that reports on this session may describe them identically
  left: (Dropped, [])
 right: (Dropped, [])
```

Hai lỗi hoàn toàn khác nhau — sai phiên bản FIX và trỏ nhầm đối tác — đọc ra **giống hệt nhau**.
Đó là đặc tả.

**Lần viết đầu bước 1 đã sai và phải viết lại.** Nó gọi `last_drop_reason()`, một hàm chưa tồn
tại → đỏ ở compiler. Đỏ vì không biên dịch được thì không chứng minh gì cả: nó không nói lỗi hiện
tại là gì, chỉ nói mình chưa viết code. Đây là lần thứ ba trong ngày mắc đúng lỗi này.

### Bước 2 — mỗi lý do tự xưng tên

`DropReason` (enum không trường, `#[non_exhaustive]`), `Session::last_drop_reason()`, và một hàm
`end(why)` duy nhất — mọi chỗ đặt `Disconnected` đều đi qua nó. `From<Refusal>` liệt kê đủ, **không
có nhánh `_`**: thêm một `Refusal` mà quên đặt tên thì **không biên dịch được**.

Xanh: 8 test trong `drop_reason.rs`, `cargo test --all` 329 passed. Commit `61689e6`.

**Hai bẫy trong bảng đã bắt được thật:**

| Bẫy | Đã xảy ra |
|---|---|
| Chỉ ghi ở đường `Refusal`, quên `tick` | `a_timeout_and_a_peer_logout_are_named_too` — hai lý do này đến từ chỗ khác hẳn |
| Ghi **sau** khi state đổi → đọc ra lý do cũ | `a_second_fault_replaces_the_first` |

Một bẫy **không** lường trước: một `Logon` khoác `35=5` vẫn mang `98=`/`108=`, hai tag không hợp lệ
cho `Logout` → nhận Reject chứ không phải lời chào tạm biệt. Phải gỡ hai tag đó đi.

### Bước 3 — sự kiện rời khỏi luồng engine

`observe::Event` / `EventKind` / `EVENT_CAPACITY` / `Observer::events` / `events_lost`, phát trong
`turn()`. Xanh: 4 test trong `crates/engine/tests/events.rs`, đọc **trong khi** engine đang quay.
Commit `ac9d220`.

**Ba lý do engine tự biết mà session không thể biết** phải thêm vào — và việc phát hiện ra chúng là
do một test **hỏng vì lý do không liên quan đến tên nó**:

- `DuplicateIdentity` — luật một-logon của ADR-0030 từ chối socket **trước khi** session xét bất cứ
  điều gì, và engine báo `TransportClosed`. Nó **đổ lỗi cho mạng vì một quyết định của chính nó**,
  và người trực đêm sẽ đi soi nhầm tầng.
- `SlowApplication` / `SlowConsumer` — đường backpressure của D10 tự gửi `Logout`, nên dùng
  `note_drop_reason` chứ không đi qua phễu disconnect.

Và `disconnect()` **không còn ghi đè lên một lý do đã biết** — chỉ riêng chỗ đó đã che mọi lý do
sau chữ `TransportClosed`.

**Đảo ngược đã chạy:**

| # | Kết quả |
|---|---|
| 1 — `last_drop_reason` trả hằng số | test phân biệt **đỏ**, `--test wire` **59/59 vẫn xanh**. Đúng như dự đoán: corpus không nhìn thấy lý do |
| 2 — bỏ bộ đếm `lost`, ring đầy thì ghi đè im lặng | `a_reader_that_falls_behind_is_told_how_much_it_missed` **đỏ** |
| 3 — `lock` thay `try_lock` | **không test nào đỏ.** Đúng như plan đã lường. Vào *Not proven*, không nhận là đã chứng minh |

### Bẫy thứ tư, không có trong bảng: benchmark đo chính đồ nghề của nó

Case `events-busy` đọc **30 000 → 6 000 → 2 000 → 0**, và **không con số sai nào đến từ đoạn code
đang được đo**. Cả ba đều là fixture: `Loopback::pair()` gọi trong cửa sổ đếm, rồi vẫn tạo pair
trong cửa sổ, rồi `VecDeque` của mỗi pair mới cấp phát ở lần `send` **đầu tiên** — nên "dựng ngoài
cửa sổ" và "làm nóng ngoài cửa sổ" là hai lời khẳng định khác nhau, và chỉ cái thứ hai mới đúng.

Giữa chừng còn có một **false green của chính công cụ chẩn đoán**: tách cửa sổ làm hai vòng, vòng
sau đọc 0 — nhưng chỉ vì vòng trước đã lấy hết `Option` ra khỏi vector, nên vòng sau `continue` mọi
lần lặp. **Một guard không thể đỏ thì luôn báo xanh.**

Cách sửa, và phần đáng mang đi chỗ khác: dựng **và làm nóng** fixture ngoài cửa sổ, cấp đủ sức chứa
cho engine, và **khẳng định ngay trong cửa sổ rằng đường code cần đo có chạy thật** — ở đây là
stream phải ghi được nhiều hơn 0 sự kiện. Chỉ điều thứ ba biến số 0 từ *"không cấp phát"* thành
*"không cấp phát trong lúc chuyện đó xảy ra"*.

Viết đầy đủ ở [a-benchmark-measured-its-own-fixture](../reference/a-benchmark-measured-its-own-fixture.md),
có gắn `[to testing-skills]`.

**Và một bài học về quy trình, lần thứ hai trong ngày:** `git checkout <file>` để bỏ một sửa đổi
nháp đã **xoá luôn phần việc chưa commit** trong cùng file đó — lần này là toàn bộ case benchmark
vừa nói. **Commit trước khi chạy bất kỳ đảo ngược nào**, nếu không thì vòng lặp đảo ngược và lệnh
undo dùng chung một mục tiêu.

### Cổng đã chạy — Apple M5, macOS 15 (máy phát triển)

| Lệnh | Kết quả |
|---|---|
| `cargo test --all` | **333 passed, 0 failed** |
| `cargo test -p fixbolt-engine --test wire` | **59/59**, mặc định và `--no-default-features --features standard` |
| `cargo bench -p fixbolt-engine --bench alloc` | `events-idle 0`, `events-busy 0`, và 15 case cũ vẫn 0 |
| `cargo clippy --all-targets --all-features -D warnings` | sạch |
| `scripts/check-no-optional-deps.sh` | sạch |
| `python3 scripts/check-links.py` | 781 liên kết, 0 hỏng |

**Không công bố con số nanosecond nào từ máy này** — `benches/baselines.tsv` khoá theo CPU model và
CPU này không có dòng nào. **Hai check chỉ chạy trên Linux** (`check-no-kernel-sleep.sh`,
`check-standard-gives-the-core-back.sh`) **không chạy được ở đây, và không-chạy-được ≠ xanh** — CI
là thứ duy nhất phán xử chúng.
