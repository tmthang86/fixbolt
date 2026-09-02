# Tắt máy có thứ tự

> **Loại:** Plan · **Ngày:** 2026-09-02 · **Trạng thái:** Đã duyệt
> *(tự viết, tự duyệt theo uỷ quyền thường trực 2026-09-01.)*
>
> **Phạm vi:** `STATUS.md` item 30 (a) — **mục cuối cùng của item 30**. Chạm `session` (một
> cách chào tạm biệt mới) và `engine` (`run`, `serve`, `pump`). Không chạm `codec`, `dict`,
> `transport`.
>
> **Máy chạy:** đóng trọn vẹn trên macOS. `shard.rs` chỉ chạy trên Linux và **nằm ngoài phạm
> vi** — nói thẳng ra, như ADR-0034 đã làm với `Recovery`.

## Bối cảnh

**Hôm nay không có cách nào dừng engine cho tử tế.** `Engine::run()` trả `-> !`; `serve` và
`serve_hft` trả `Result<Infallible, ServeError>`. Cách duy nhất để dừng là giết tiến trình.

Điều đó có ba hậu quả, và không cái nào là lý thuyết:

| Chuyện gì xảy ra | Vì sao nó tệ |
|---|---|
| Đối tác không nhận `Logout` | Với họ đây là **đường truyền chết**, không phải một lần đóng cửa có kế hoạch. Họ sẽ thử kết nối lại, có khi hàng giờ |
| Byte đã đánh số nhưng chưa ra khỏi `tx` thì mất | Đã tiêu số thứ tự. Lần sau lên, đối tác thấy gap và đòi resend thứ chưa từng lên dây |
| File journal có thể có đuôi rách | `[2026-09-02]` bây giờ ta **thấy được** điều đó (`torn_tail_bytes`), và thấy được nghĩa là phải bớt đi |

Và có một cái bẫy đã cắn một lần: `[2026-08-30]` thả engine trong khi luồng khác còn giữ
`WakeHandle` thì self-pipe đóng đầu đọc, `libc::write` vào đầu ghi và **`SIGPIPE` giết tiến
trình**. Nên đường tắt máy ở đây **thiếu thiết kế**, không đơn thuần là thiếu tính năng.

## Những gì đã biết chắc

| Sự thật | Nguồn |
|---|---|
| `logout_now` gửi `Logout` rồi trả **`Link::Dropped` ngay** — nó là đường của D10, không phải đường chào tạm biệt | `crates/session/src/lib.rs` |
| Trạng thái `AwaitingLogout` đã tồn tại, và `Logout` của đối tác cho `DropReason::PeerLogout` | ADR-0035 |
| Kênh lệnh từ luồng khác đã có, và **không mất lệnh** | [ADR-0036](../decisions/ADR-0036-one-mechanism-two-capabilities.md) |
| `serve`/`serve_hft`/`pump` chạy được trên macOS (`cfg(all(feature = "standard", unix))`); `shard.rs` thì không (`target_os = "linux"`) | `crates/engine/src/lib.rs` dòng 36 |
| `.run()` **không có caller nào trong repo** — `tools/w2w` không gọi | grep 2026-09-02 |
| Thả engine khi còn `WakeHandle` sống = `SIGPIPE` | `STATUS.md` mục *Proven* |

## Quyết định trung tâm

**Tắt máy là một lệnh, không phải một tín hiệu.** Nó đi qua đúng kênh `Admin` mà ADR-0036 đã
dựng — cùng một `Arc`, cùng quy tắc **lệnh không được mất**. Không thêm cờ toàn cục, không thêm
cơ chế thứ hai. Người vận hành gọi `Admin::shutdown()`.

**`Session` học một cách chào tạm biệt mới.** `begin_logout(text, emit)` gửi `Logout` và trả
**`Link::Up`** — link còn sống, session vào `AwaitingLogout`, và engine tiếp tục quay cho tới
khi `Logout` của đối tác về hoặc hết hạn. `logout_now` **giữ nguyên**: nó là đường của D10, nơi
việc cần làm đúng là cắt ngay, và trộn hai thứ vào một hàm là cách để cả hai cùng sai.

**Có hạn chót, và nó là của người gọi.** Không đợi vô hạn: một đối tác đã chết không bao giờ trả
lời. `Admin::shutdown()` nhận một khoảng thời gian tính bằng mili giây trên đồng hồ của engine.

**`run()` đổi từ `-> !` sang trả về một báo cáo.** Đây là API break, và blast radius **bằng
không** — không caller nào trong repo. Khác hẳn tình huống ADR-0034 phải né. `serve`/`serve_hft`
đổi theo; **`serve_sharded_hft` không đụng tới** và điều đó được ghi vào *Not proven*.

**Báo cáo nói ra cái nó không làm được.** `Shutdown { sessions, said_goodbye, acked, timed_out }`
— vì "tắt xong" và "tắt xong mà hai đối tác không kịp trả lời" là hai chuyện khác nhau, và người
vận hành phải phân biệt được trước khi khởi động lại.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **2 — session thuần** | thêm `begin_logout` | Không clock, không socket, không alloc — hạn chót đến qua `Input::Tick` như mọi thứ khác |
| **1 — không cấp phát** | vòng tắt máy | `Shutdown` là struct số. Case mới `benches/alloc.rs`: một lần tắt máy đầy đủ, đọc 0 |
| **4 — luồng engine không ngủ** | chờ ack | **Chờ bằng cách quay tiếp**, không phải bằng `sleep`. Cùng `turn()` cũ |
| **3 — 59 định nghĩa** | đường session đổi | 59/59 cả hai mode |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | **Test đặc tả, đỏ ở assertion.** Engine chạy, một phiên đang logon; dừng engine — đối tác phải **nhận được `Logout`**. Hôm nay không nhận được gì | — |
| 2 | `Session::begin_logout`, giữ link `Up`. Test riêng, kể cả khi đối tác im lặng | 1 |
| 3 | `Admin::shutdown(deadline_ms)`, vòng tắt máy trong `turn()`, `Shutdown`; `run`/`serve`/`serve_hft` trả về | 2 |

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 1 | `cargo test -p fixbolt-engine --test shutdown` | **đỏ ở assertion** |
| 2 | `cargo test -p fixbolt-session` | xanh; link còn `Up` sau khi gửi `Logout` |
| 3 | `cargo test -p fixbolt-engine --test shutdown` | xanh; **đối tác im lặng thì vẫn tắt được**, và báo cáo nói ra |
| 3 | `cargo bench -p fixbolt-engine --bench alloc` | `shutdown 0` |
| mọi bước | `--test wire` 59/59 cả hai mode; `cargo test --all`; `check-no-optional-deps.sh`; clippy; fmt; links | xanh |

**Đảo ngược, bắt buộc:**

1. `begin_logout` trả `Link::Dropped` như `logout_now` → phải có test đỏ, vì `Logout` sẽ không
   bao giờ ra khỏi `tx`.
2. Bỏ hạn chót, đợi vô hạn → test "đối tác im lặng" phải **treo rồi hỏng**, không phải xanh.
3. Tắt máy không đợi `tx` trôi hết → test đỏ, vì byte đã đánh số bị mất.

**Bẫy đã lường trước:**

| Bẫy | Test canh |
|---|---|
| `Logout` được **tạo** nhưng chưa ra socket khi engine dừng | Đọc từ **socket phía đối tác**, không phải từ snapshot |
| Đối tác chết → treo vô hạn | Một test không đọc gì cả, và phải kết thúc |
| Hạn chót đo bằng đồng hồ thật → test giòn | `ManualClock`, hạn chót là số mili giây, test tự đẩy đồng hồ |
| Thả engine khi còn `WakeHandle` → `SIGPIPE` | Một test tắt máy **rồi thả**, có `WakeHandle` sống |

## Tài liệu phải cập nhật

- [ ] ADR mới — tắt máy là một lệnh; `begin_logout` tách khỏi `logout_now`; báo cáo nói ra cái nó không làm được
- [ ] `DESIGN.md` §3; `CHANGELOG.md`; `GUIDE.md`; `STATUS.md` item 30 **(đóng hẳn)**; `PRD.md`
- [ ] Đi lại bảng §4, đọc lại *Not proven*

## Ngoài phạm vi

- **`serve_sharded_hft`** — Linux-only, không chạy được ở máy này. Ghi vào *Not proven*.
- **Chờ ứng dụng xử lý xong hàng đợi** — đó là `Dispatch`, và D10 đã có chính sách riêng.
- **Bắt tín hiệu (`SIGTERM`)** — engine là thư viện; bắt tín hiệu là việc của tiến trình.
