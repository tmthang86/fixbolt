# Vì sao một kết nối kết thúc

> **Loại:** Plan · **Ngày:** 2026-09-02 · **Trạng thái:** Đã duyệt
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

- [ ] ADR mới — sự kiện đẩy, mất mát phải đếm, dùng lại cơ chế ADR-0032
- [ ] `DESIGN.md` §3; `CHANGELOG.md`; `GUIDE.md` §8a; `STATUS.md` item 30; `PRD.md`
- [ ] Đi lại bảng §4, đọc lại *Not proven*

## Ngoài phạm vi

- **Audit tap** — ADR-0027 nói nó là tính năng riêng.
- **Định dạng xuất** (JSON, Prometheus) — `Event` là dữ liệu.
- **Sự kiện mức message** (mỗi message vào/ra) — đó là hot path và D8 cấm.

## Nhật ký giao hàng

> Điền khi đóng từng bước.
