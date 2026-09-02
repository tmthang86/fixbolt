# Số thứ tự lúc ba giờ sáng

> **Loại:** Plan · **Ngày:** 2026-09-02 · **Trạng thái:** Đã duyệt
> *(tự viết, tự duyệt theo uỷ quyền thường trực 2026-09-01.)*
>
> **Phạm vi:** `STATUS.md` item 30 (c). Chạm `session` (hai hàm đặt số, thuần) và `engine`
> (mở rộng `observe` thêm chiều ngược lại). Không chạm `codec`, `dict`, `transport`.
>
> **Máy chạy:** đóng trọn vẹn trên macOS.

## Bối cảnh

**Thao tác mà mọi người vận hành FIX đều phải làm — và engine này không có đường nào để làm.**
Đối tác gọi điện lúc 3 giờ sáng: *"số của chúng tôi là 4 812, của anh là gì?"* Trong QuickFIX,
người trực gọi `setNextSenderMsgSeqNum`. Ở đây, `Session::resume` là **hàm khởi tạo** — muốn đổi
một con số thì phải dựng lại session, tức là **dừng engine**.

`[verified 2026-09-02]` `Engine` không có một hàm public nào chạm tới số thứ tự của một phiên
đang chạy. `conns` là private; `Session::next_out`/`next_in` chỉ đọc.

Và mọi phiên đang sống trên **luồng engine**, luồng mà non-negotiable 4 cấm chặn. Nên đây không
phải chuyện thêm một setter — nó là chuyện **đưa một mệnh lệnh từ luồng khác vào luồng engine mà
không làm luồng đó ngủ**.

## Những gì đã biết chắc

| Sự thật | Nguồn |
|---|---|
| Cơ chế đi qua ranh giới luồng đã trả giá xong một lần: `Arc<Shared>`, `try_lock` không bao giờ `lock`, cấu trúc cố định | [ADR-0032](../decisions/ADR-0032-observation-is-a-snapshot-taken-on-request.md) |
| Chiều engine → người vận hành đã có, kể cả **sự kiện đẩy** và bộ đếm mất mát | [ADR-0035](../decisions/ADR-0035-an-event-is-pushed-and-a-loss-is-counted.md) |
| Hai cơ chế song song là hai thứ sẽ bất đồng | ADR-0035 quyết định 3 |
| `Session::resume`/`resume_at` đặt được cả hai số — nhưng chỉ lúc dựng | `crates/session/src/lib.rs` |
| Đổi số **gửi đi** mà không báo đối tác là nói dối; cách trung thực duy nhất là `SequenceReset` (35=4, 123=N) | [session-lifecycle-prior-art](../reference/session-lifecycle-prior-art.md) |
| QuickFIX `setNextSenderMsgSeqNum` **chỉ đổi cục bộ**, không gửi gì | như trên |
| Ranh giới lịch phiên đã tự đặt lại cả hai số | [ADR-0033](../decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md) |

## Quyết định trung tâm

**Một cơ chế, hai quyền.** `Commands` nằm **trong chính `observe::Shared`** đã có, sau cùng một
`Arc`. Nhưng `Engine` phát ra **hai tay cầm khác nhau**: `Observer` (chỉ đọc) và `Admin` (ghi).
Người vận hành nào cũng xem được; chỉ nơi nào được trao `Admin` mới đặt lại được số thứ tự.

Điều này **không** mâu thuẫn với ADR-0035 quyết định 3 — vẫn đúng một cơ chế, một `try_lock`, một
kiểu vòng đệm cố định. Cái tách ra là **quyền**, không phải cơ chế.

**Lệnh được áp trên luồng engine, ở đầu `turn()`, trước khi bất kỳ message nào được đánh số.**
Áp sau thì con số vừa đặt đã bị dùng mất rồi.

**Kết quả của lệnh đi ra bằng luồng sự kiện.** ADR-0035 đã dựng đường ấy; một lệnh không nói được
là nó có tác dụng hay không thì tệ hơn không có lệnh. Và như thế **cùng một dòng ghi lại cả
chuyện tự xảy ra lẫn chuyện mình gây ra** — đó chính là audit trail mà item 30 hỏi.

**Ba lệnh, và mỗi lệnh nói rõ nó có nói với đối tác hay không:**

| Lệnh | Có gửi gì ra dây không |
|---|---|
| `SetNextIn { id, n }` | **Không.** Số mình *mong đợi* là chuyện của mình |
| `SetNextOut { id, n }` | **Không** — và đây là một lời nói dối cho đến khi đối tác được báo. Đặt tên và ghi tài liệu đúng như thế, giống QuickFIX |
| `SendSequenceReset { id, n }` | **Có**: `35=4`, `123=N`, `36=n`. Cách trung thực duy nhất để đổi số gửi đi |

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **2 — session thuần** | thêm hai setter | Không clock, không socket, không alloc. `SendSequenceReset` phát byte qua closure `emit` như mọi thứ khác |
| **1 — không cấp phát** | hàng lệnh | Vòng đệm cố định trong `Shared`. Case mới trong `benches/alloc.rs`: có lệnh và không có lệnh, cả hai đọc 0 |
| **4 — luồng engine không ngủ** | đọc hàng lệnh mỗi turn | `try_lock`, không bao giờ `lock`. Không lấy được thì để lệnh lại cho turn sau — **khác** sự kiện, vì lệnh không mất được |
| **3 — 59 định nghĩa** | đường session đổi | 59/59 cả hai mode |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | **Test đặc tả, đỏ ở assertion.** Một engine đang chạy, một phiên đang logon; từ luồng khác đổi số — hôm nay không có đường nào, nên test phải đỏ bằng thứ quan sát được hôm nay | — |
| 2 | `Session::set_next_out` / `set_next_in` + `send_sequence_reset`. Thuần, có test riêng | 1 |
| 3 | `observe::Command`, `Admin`, `Engine::admin()`, áp ở đầu `turn()`; kết quả ra luồng sự kiện | 2 |

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 1 | `cargo test -p fixbolt-engine --test admin` | **đỏ ở assertion** |
| 2 | `cargo test -p fixbolt-session` | xanh, mỗi setter một test |
| 3 | `cargo test -p fixbolt-engine --test admin` | xanh; lệnh gửi **từ luồng khác** trong khi engine quay |
| 3 | `cargo bench -p fixbolt-engine --bench alloc` | `admin-idle 0`, `admin-busy 0` |
| mọi bước | `--test wire` 59/59 cả hai mode; `cargo test --all`; `check-no-optional-deps.sh`; clippy; fmt; links | xanh |

**Đảo ngược, bắt buộc:**

1. Áp lệnh **sau** khi đánh số thay vì trước → phải có test đỏ. Nếu không, thứ tự chẳng ai canh.
2. `Admin::submit` luôn trả `true` nhưng vứt lệnh đi → test đỏ, và **59/59 vẫn xanh**.
3. Bỏ kiểm tra `ConnId` không tồn tại → lệnh trôi vào hư không, phải có test đỏ.

**Bẫy đã lường trước:**

| Bẫy | Test canh |
|---|---|
| Đặt lại số trong khi ring còn byte **đã đánh số** chờ gửi | Một test có output tồn đọng rồi mới đặt lại |
| `try_lock` hỏng → lệnh **mất** (khác sự kiện: sự kiện mất thì đếm, lệnh mất là sai) | Lệnh ở lại hàng cho turn sau; test ép hỏng khoá |
| Test đọc kết quả sau khi engine dừng | Gửi lệnh **trong khi** engine quay, như `tests/events.rs` |

## Tài liệu phải cập nhật

- [ ] ADR mới — một cơ chế hai quyền; lệnh áp trước khi đánh số; ba lệnh và lệnh nào nói với đối tác
- [ ] `DESIGN.md` §3; `CHANGELOG.md`; `GUIDE.md` §8a; `STATUS.md` item 30; `PRD.md`
- [ ] Đi lại bảng §4, đọc lại *Not proven*

## Ngoài phạm vi

- **Đặt lại số cho một đối tác chưa kết nối** — chưa có `ConnId` nào để chỉ tới. Đó là việc của `Recovery` (ADR-0034).
- **Tự động `ResendRequest`** — session đã tự làm khi thấy gap.
- **Xác thực ai được cầm `Admin`** — engine không biết gì về danh tính người vận hành.
