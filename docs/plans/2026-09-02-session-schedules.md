# Lịch phiên: khi nào session mở, khi nào số thứ tự về 1

> **Loại:** Plan · **Ngày:** 2026-09-02 · **Trạng thái:** Đã duyệt
> *(tự viết và tự duyệt theo uỷ quyền thường trực của chủ sở hữu, 2026-09-01.)*
>
> **Phạm vi:** `PRD.md` §2 dòng *Session schedules* — một lỗ hổng Phase 1 **được gọi tên ba
> lần và chưa bao giờ có plan**. Chạm `session` (một kiểu mới, thuần) và `engine` (nơi đồng hồ
> thật đi vào). Không chạm `codec`, không chạm `dict`, không chạm `transport`.
>
> **Máy chạy:** đóng trọn vẹn trên macOS. Toàn bộ việc này là số học trên trục thời gian và
> máy trạng thái thuần — không có cổng nào cần Linux, không có con số nanosecond nào được
> công bố.

## Bối cảnh

Một phiên FIX **không chạy mãi mãi**. Nó mở lúc 08:00, đóng lúc 17:00, và **sáng hôm sau cả
hai bên bắt đầu lại từ `34=1`**. Đó không phải chuyện vận hành phụ — đó là giao thức: hai bên
phải **đồng ý** khi nào số thứ tự về 1, và bên nào hiểu sai thì sáng hôm sau tranh chấp số
thứ tự với đối tác.

Hôm nay engine này **không có khái niệm đó**. `Session::new` reset, `Session::resume` không
reset ([ADR-0010](../decisions/ADR-0010-a-reconnect-is-not-a-restart.md)), và *chọn cái nào*
là việc của người nhúng — nhưng không có gì nói cho họ biết **khi nào** phải chọn. `GUIDE.md`
§9 nói thẳng: *"It has no session schedule."*

Ba chỗ trong repo này đã gọi tên nó là lỗ hổng: `PRD.md` §2 (`P1, gap`), `PRD.md` §1 bản đồ
Phase 1 (*"Named a gap three times, never planned"*), và `PRD.md` §219 (*"Reconnect, backoff,
session schedules: untested"*). Plan này là lần đầu nó được lập kế hoạch.

## Những gì đã biết chắc

**Về giao thức và về prior art:**

| Sự thật | Nguồn |
|---|---|
| QuickFIX quyết định reset bằng `isSameSession(t1, t2)` — *hai mốc thời gian này có rơi vào cùng một khoảng phiên không* — chứ **không phải** bằng "đồng hồ vừa điểm 00:00" | `DefaultSessionSchedule.java`, đọc 2026-09-02 |
| Khoá cấu hình của QuickFIX: `StartTime`, `EndTime`, `StartDay`, `EndDay`, `Weekdays`, `TimeZone`, `NonStopSession`. Không đặt `StartDay`/`EndDay` → phiên theo **ngày**; đặt → phiên theo **tuần** | như trên |
| QuickFIX **không xử lý DST một cách tường minh**. Mỗi mốc giờ mang theo một `TimeZone` và lớp đó đọc giờ/phút/giây theo zone khi dựng khoảng | như trên |
| **Artio không có lịch phiên trong lõi.** `SessionScheduler` nằm ở `artio-samples/`, là một ví dụ, không phải một thành phần. Artio phơi ra `resetSequenceNumber()` và để *khi nào* cho người nhúng | wiki Artio + cây thư mục, đọc 2026-09-02 |

**Về repo này:**

| Sự thật | Nguồn |
|---|---|
| `Input::Tick` mang `u64` mili-giây kể từ **0000-01-01**, không phải 1970 — để mọi timestamp FIX viết được đều là `u64` không âm | `crates/session/src/clock.rs`, D13 |
| `MILLIS_YEAR_ZERO_TO_EPOCH` và `DAYS_YEAR_ZERO_TO_EPOCH` đã có, và có test **suy ra** hằng số chứ không nhớ nó | `clock.rs::the_epoch_offset_is_derived_not_recalled` |
| Session thuần: không đồng hồ, không socket, không cấp phát. Thời gian vào qua `tick` và **chỉ qua đó** | D1, bất biến 2 |
| `141=Y` trên một `Logon` đã reset cả hai chiều, **trước khi** chính `Logon` đó được đánh số | `crates/session/src/lib.rs:1348` |
| `Session::config()` và `Config` là `Copy`, 128 byte, đã có sẵn | `crates/session/src/lib.rs` |
| **Corpus không có một định nghĩa nào về lịch phiên.** 59 định nghĩa chạy trong một khoảng thời gian duy nhất | `PRD.md` §219 |

## Quyết định trung tâm: cắt đúng chỗ múi giờ

**`Schedule` là số học thuần trên trục mili-giây, biểu diễn bằng UTC. Không tên múi giờ,
không cơ sở dữ liệu IANA, không DST.**

Vì sao: một `Schedule` biết "17:00 America/New_York" cần cơ sở dữ liệu múi giờ. Cơ sở dữ liệu
đó là một dependency, nó cấp phát, và nó phải sống trong lớp mà bất biến 2 nói là **thuần**.
Đưa nó vào là phá D1 để lấy một tiện nghi.

**Cắt như sau:**

- `session` nhận một `Schedule` đã quy về UTC: *giây kể từ nửa đêm UTC* cho mốc mở và mốc
  đóng, cộng một tập ngày trong tuần. Số học thuần, `const fn` ở đâu làm được.
- **Việc đổi "17:00 New York" thành UTC không thuộc workspace này.** Ai cần thì dùng crate
  múi giờ của họ, dựng `Schedule`, và dựng lại khi DST đảo. `GUIDE.md` phải nói to điều này,
  vì đây chính là chỗ người ta làm sai.
- Cho trường hợp phổ biến không có DST, có `Schedule::with_utc_offset_ms` để không ai phải
  tự trừ tay.

**Đây là lựa chọn giữa hai prior art, không phải một sáng tạo:** QuickFIX đặt múi giờ *vào
trong* engine và tự nhận là không xử lý DST tường minh; Artio đặt cả cái lịch *ra ngoài*
engine. Plan này đứng giữa: **hình dạng phiên ở trong, lịch dương ở ngoài** — vì `same_session`
là quy tắc giao thức và cần được test, còn "múi giờ nào" là dữ liệu triển khai.

**Reset được quyết bằng `same_session(a, b)`, không bằng chuông báo.** Một engine ngủ qua đêm,
hoặc một tiến trình khởi động lại lúc 06:00, đều phải ra cùng một câu trả lời: *mốc cuối tôi
còn nhớ và bây giờ có cùng một phiên không*. Một cái chuông "reset lúc 00:00" bỏ lỡ mọi lần
tiến trình không chạy vào đúng lúc đó — và đó là lúc reset **quan trọng nhất**.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **2 — session thuần** | `Schedule` sống trong `session` | Không dependency, không cấp phát, không `format!`. Toàn bộ là số học trên `u64`/`u32`. Thời gian vẫn chỉ vào qua `tick` |
| **1 — không cấp phát hot path** | `tick` gọi `Schedule` mỗi lần | `Schedule` là `Copy`, khoảng 24 byte. Case mới trong `benches/alloc.rs` — phiên **trong** giờ và phiên **ngoài** giờ, vì hai nhánh khác nhau |
| **3 — 59 định nghĩa là cổng** | `tick` đổi hành vi | `Schedule::always()` là mặc định và phải **không đổi gì**. 59/59 cả hai mode là điều kiện đóng mỗi bước |
| **7 — không `unwrap`/`expect`/`panic`** | API công khai mới | Hàm dựng trả `Option`. Giờ 25:00, ngày rỗng, mở trùng đóng — tất cả trả `None`, không panic |
| **5 — thứ tự trường từ bảng sinh** | `Logout` khi hết giờ | Dùng đúng đường `send(Which::Logout, …)` đang có, không dựng tay |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | **Test đặc tả, đỏ trước.** `crates/session/tests/schedule.rs`: hai mốc cách nhau qua nửa đêm phải **khác phiên**; hai mốc trong cùng ngày phải **cùng phiên**; một `Logon` đến ngoài giờ phải bị từ chối. Đỏ vì hôm nay không có kiểu nào để hỏi | — |
| 2 | `Schedule`: `daily`, `weekly`, `always`, `with_utc_offset_ms`; `contains(t)`, `same_session(a, b)`. Thuần, `Copy`, `Option` ở mọi hàm dựng. **Chưa nối vào `Session`.** Bước 1 xanh phần số học | 1 |
| 3 | Nối vào `Session`: `Config::with_schedule`. `tick` ngoài giờ → `Logout` rồi `Disconnected`; `Logon` đến ngoài giờ → từ chối im lặng; sang phiên mới → **reset cả hai số trước** `Logon` kế tiếp. 59/59 không đổi | 2 |
| 4 | `Engine`: mốc "phiên lần cuối" đi vào journal cạnh số thứ tự, để một tiến trình khởi động lại vẫn trả lời được `same_session`. Không có nó thì bước 3 chỉ đúng khi tiến trình không bao giờ chết | 3 |
| 5 | `benches/alloc.rs` hai case; `GUIDE.md` mục múi giờ; ADR | 3 |

**Bước 1–3 là plan này.** Bước 4 chạm định dạng journal và **có thể tách thành plan riêng nếu
nó lớn hơn dự tính** — quy tắc §1 của `CLAUDE.md`: plan sai giữa chừng thì dừng và sửa plan.

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 1 | `cargo test -p fixbolt-session --test schedule` | **đỏ**, và đỏ vì *không có `Schedule`*, không phải vì không compile |
| 2 | như trên | phần số học xanh |
| 3 | như trên, và `cargo test -p fixbolt-session --test score` | `step_six_b_replays_what_it_sent_and_scores_fifty_nine` **ok** |
| 3 | `cargo test -p fixbolt-engine --test wire` | **59/59, cả hai mode** |
| 5 | `cargo bench --bench alloc` | `schedule-in 0` và `schedule-out 0` |
| mọi bước | `cargo test --all`, `--no-default-features`, clippy `-D warnings`, `fmt`, `check-links.py` | xanh |

**Đảo ngược, bắt buộc:**

1. Cho `same_session` luôn trả `true` → test qua-nửa-đêm phải đỏ, và **59/59 phải vẫn xanh**.
   Nếu 59 đỏ theo thì `Schedule::always()` đã không thực sự trung tính và mặc định đang thay
   đổi hành vi của mọi người dùng hiện có.
2. Cho `contains` luôn trả `true` → test từ-chối-ngoài-giờ đỏ, và **chỉ nó**.
3. Bỏ nhánh reset ở bước 3, giữ nguyên `Logout` → phải có một test đỏ về **số thứ tự**, không
   phải một test đỏ về kết nối. Nếu chỉ test kết nối đỏ thì bộ test này đang đo việc ngắt
   phiên chứ không đo việc reset, và **đó mới là thứ giao thức quan tâm**.

**Bẫy đã lường trước:**

| Bẫy | Test canh |
|---|---|
| **Ngày trong tuần tính từ epoch 0000-01-01 chứ không phải 1970.** Lệch một ngày là lịch tuần sai toàn bộ | Một test **suy ra** thứ của 0000-01-01 từ một ngày đã biết thứ, theo đúng lối `the_epoch_offset_is_derived_not_recalled` — không nhớ hằng số |
| **Phiên vắt qua nửa đêm** (mở 22:00, đóng 06:00). Nếu `contains` là `start <= t && t < end` thì nó luôn sai | Một case riêng cho phiên vắt đêm, cả `contains` lẫn `same_session` |
| **`always()` không thật sự trung tính** → 59/59 rơi, và người ta sẽ đổ cho việc nối dây chứ không cho mặc định | Đảo ngược 1 ở trên bắt đúng cái này |
| **Reset xảy ra nhưng sai thứ tự** — reset *sau* khi `Logon` đã được đánh số thì `Logon` mang số cũ | Test khẳng định `34=` của `Logon` đầu phiên mới là **1**, không phải chỉ khẳng định `next_out` đã về 1 |
| **Corpus không thấy gì ở đây cả** — mọi thứ bước này thêm vào đều vô hình với 59 định nghĩa | Một dòng mới trong `STATUS.md` *Not proven*, viết ngay khi bước 3 đóng chứ không để cuối |

## Tài liệu phải cập nhật

- [ ] ADR mới — lịch phiên là số học UTC thuần; múi giờ ở ngoài workspace; reset quyết bằng
      `same_session` chứ không bằng chuông
- [ ] `DESIGN.md` §4 — hành vi session đổi
- [ ] `CHANGELOG.md` — API công khai (`Schedule`, `Config::with_schedule`)
- [ ] `GUIDE.md` — mục múi giờ và DST, **viết như một cảnh báo chứ không như một tính năng**
- [ ] `GUIDE.md` §9 — bỏ dòng *"It has no session schedule"*
- [ ] `PRD.md` §2 dòng *Session schedules*, và bản đồ Phase 1 ở §1
- [ ] `STATUS.md` — item mới cho phần chưa làm, và một dòng *Not proven*
- [ ] Đi lại bảng §4 từng dòng, và đọc lại *Not proven* từng dòng

## Ngoài phạm vi

- **Cơ sở dữ liệu múi giờ IANA.** Quyết định trung tâm ở trên là để không cần nó.
- **`ResetSeqTime` riêng biệt** (QuickFIX cho reset ở một giờ khác giờ đóng phiên). Thêm được
  sau; hôm nay reset gắn với ranh giới phiên.
- **Initiator tự đăng nhập theo lịch.** Đó là ADR-0004 và plan initiator đang **tạm dừng**.
- **Lịch theo từng counterparty.** Registry đã trả về `Config`, nên khi `Config` mang
  `Schedule` thì điều này thành ra miễn phí — nhưng nó không được test ở plan này.

## Nhật ký giao hàng

> Điền khi đóng từng bước.
