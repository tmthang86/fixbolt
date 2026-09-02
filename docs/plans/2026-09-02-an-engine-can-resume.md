# Engine phải mở lại được một phiên đã có lịch sử

> **Loại:** Plan · **Ngày:** 2026-09-02 · **Trạng thái:** Đã duyệt
> *(tự viết và tự duyệt theo uỷ quyền thường trực của chủ sở hữu, 2026-09-01.)*
>
> **Phạm vi:** `STATUS.md` open item 31. Chạm `engine` (API công khai, `presession`). Không
> chạm `session` — mọi thứ ở lớp đó **đã có sẵn và đã được test**. Không chạm `codec`, `dict`.
>
> **Máy chạy:** đóng trọn vẹn trên macOS. Toàn bộ là test, không công bố con số nanosecond nào.

## Bối cảnh

`[verified 2026-09-02]` **`Engine` không mở lại được một phiên nào cả.** Cả hai đường vào —
`add_with_journal` và `add_with_prefix_and_config` — đều dựng `Session::new(cfg)`, mà cái đó
reset. `conns` là private, không hàm nào trả về `&mut Connection`, và `Connection::new` tuy
`pub` nhưng không có gì công khai chạm tới nó.

Nghĩa là mọi thứ dưới đây **có thật, đã test, và không với tới được qua API của engine**:

| Đã có | Từ |
|---|---|
| `Journal::highest` / `highest_in` — đọc lại số thứ tự hai chiều | item 16, đóng 2026-08-31 |
| `Session::resume(cfg, out, in)` | [ADR-0010](../decisions/ADR-0010-a-reconnect-is-not-a-restart.md) |
| `Session::resume_at(cfg, out, in, last_active_ms)` | [ADR-0033](../decisions/ADR-0033-a-schedule-is-utc-arithmetic-and-the-calendar-stays-outside.md) |
| `Session::last_active_ms()` — thứ cần lưu | như trên |
| `Durability::Fsync` | D7 |

**Vì sao lọt:** item 16 đóng một cách trung thực — journal *đọc lại được* và `Session::resume`
*chạy đúng* — và `crates/engine/tests/recovery.rs` chứng minh cả hai với **không một chữ
`Engine` nào trong file**. Một tầng được làm xong và cái khớp nối phía trên nó không ai hỏi
tới, bởi một plan mà mọi tiêu chí đóng đều thoả được ở tầng dưới.

**Hệ quả thực tế:** `Durability::Fsync` hôm nay mua một sổ ghi chép, không mua một cơ chế phục
hồi — đúng cái câu mà rustdoc của `Journal::highest` đã cảnh báo, chỉ là ở một tầng cao hơn.

## Những gì đã biết chắc

| Sự thật | Nguồn |
|---|---|
| `add_with_prefix_and_config` dựng `J::default()` cho mỗi connection, nên **journal cũng chưa nối được theo counterparty** | `crates/engine/src/lib.rs:284` |
| Registry chọn `Config` **sau** khi `Logon` tới, nên danh tính chỉ biết được ở thời điểm đó | [ADR-0026](../decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md), [ADR-0030](../decisions/ADR-0030-one-engine-holds-many-counterparties.md) |
| `Registry` là một **trait**, đồng bộ, trả `Option`, và trả `None` chính là chỗ từ chối | ADR-0026 |
| Artio phơi `resetSequenceNumber()` và để *khi nào* cho người nhúng; engine không đoán | wiki Artio, đọc 2026-09-02 |
| ADR-0010 nói thẳng: engine **không bao giờ đoán** nên reset hay không | ADR-0010 |

## Quyết định trung tâm: engine hỏi, không đoán — và hỏi đúng lúc biết danh tính

Hai tầng, và **tầng 1 tự nó đã đóng được lỗ hổng "không với tới được"**.

**Tầng 1 — một đường vào nguyên thuỷ.**

```rust
impl Engine {
    /// Nhận một connection và mở lại phiên từ số thứ tự người gọi đưa.
    pub fn add_resumed(&mut self, transport: T, cfg: Config, journal: J,
                       next_out: u32, next_in: u32,
                       last_active_ms: Option<u64>) -> ConnId;
}
```

Người nhúng đọc journal của họ, tự quyết, rồi trao. Đúng tinh thần ADR-0010 và đúng hình dạng
Artio. Không có chính sách nào bị nhét vào engine.

**Tầng 2 — một trait `Recovery`, đối xứng với `Registry`.**

```rust
pub trait Recovery<J> {
    /// Counterparty này để lại gì? `None` = không gì cả, dùng `Session::new`.
    fn recover(&mut self, cfg: &Config) -> Option<Resumed<J>>;
}
pub struct Resumed<J> { pub journal: J, pub next_out: u32, pub next_in: u32,
                        pub last_active_ms: Option<u64> }
```

Hỏi **sau** khi registry chọn `Config`, vì trước đó chưa biết danh tính. `NoRecovery` là mặc
định và **phải trung tính tuyệt đối** — 59 định nghĩa chạy dưới nó.

**Vì sao là trait chứ không phải một map:** cùng lý do ADR-0026 đưa ra cho `Registry`. Một
triển khai đọc file, một triển khai đọc `FileJournal` trên đĩa, một triển khai test — engine
không cần biết cái nào.

**Vì sao `Resumed` mang cả `journal`:** hôm nay mỗi connection nhận `J::default()`, nên ngay
cả khi số thứ tự đúng thì journal vẫn rỗng và `ResendRequest` đầu tiên trả về gap fill. Trả
số mà không trả journal là mở lại một nửa.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **1 — không cấp phát hot path** | `recover` gọi trên đường connection, không phải mỗi turn | Case mới trong `benches/alloc.rs`: một vòng admit→settle→add có `NoRecovery`, phải đọc 0 |
| **3 — 59 định nghĩa là cổng** | đường `add` đổi | `NoRecovery` mặc định, và 59/59 cả hai mode là điều kiện đóng mỗi bước |
| **7 — không `unwrap`/`expect`/`panic`** | API công khai | `recover` trả `Option`; không có gì để `unwrap` |
| **2 — session thuần** | không đụng | Plan này **không sửa `session`**. Nếu thấy cần sửa thì dừng lại và hỏi vì sao |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | **Test đặc tả, đỏ ở assertion.** `crates/engine/tests/engine_recovery.rs`: dựng journal có lịch sử, đưa cho **`Engine`**, và khẳng định phiên tiếp tục đếm. Đỏ vì engine reset | — |
| 2 | `Engine::add_resumed`. Bước 1 xanh. **Lỗ hổng "không với tới được" đóng ở đây** | 1 |
| 3 | `Recovery`, `Resumed`, `NoRecovery`; `pump` hỏi sau khi registry chọn `Config`. `serve`/`serve_hft` nhận nó | 2 |
| 4 | Lịch phiên gặp phục hồi: một phiên mở lại qua ranh giới **phải** về `34=1`; cùng phiên thì không | 3 |

**Bước 1–2 đóng được lỗ hổng.** Bước 3–4 là đường triển khai. Nếu buổi làm dừng sau bước 2 thì
item 31 **thu hẹp**, không đóng, và `STATUS.md` phải nói đúng như vậy.

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 1 | `cargo test -p fixbolt-engine --test engine_recovery` | **đỏ ở một assertion** về số thứ tự |
| 2 | như trên | xanh; và một control chứng minh `add` thường vẫn reset |
| 3 | như trên | `NoRecovery` không đổi gì; một `Recovery` giả trả đúng cho đúng danh tính |
| 4 | như trên | qua ranh giới về 1; trong cùng phiên giữ nguyên |
| mọi bước | `--test wire` 59/59 cả hai mode; `cargo test --all`; `--no-default-features`; clippy; fmt; links | xanh |

**Đảo ngược, bắt buộc:**

1. `add_resumed` bỏ qua `next_out` → test đếm phải đỏ, và **59/59 vẫn xanh**.
2. `NoRecovery::recover` trả `Some` với số bịa → phải có test đỏ. Nếu không có thì mặc định
   không được canh và bước 3 chưa xong.
3. Bỏ `last_active_ms`, luôn truyền `None` → test bước 4 đỏ, **và chỉ nó**.

**Bẫy đã lường trước:**

| Bẫy | Test canh |
|---|---|
| **Một test "phục hồi" không có `Engine` trong file** — đúng cái đã để lọt item 31 | File mới **bắt buộc** dựng `Engine` và đi qua `add*`; một khẳng định ở `grep` là không đủ, nên test dùng `Engine` để lấy kết quả |
| `NoRecovery` không trung tính → 59 rơi và sẽ bị đổ cho việc nối dây | Đảo ngược 1 |
| Số đúng nhưng journal rỗng → `ResendRequest` trả gap fill, và "phục hồi" chỉ đúng một nửa | Một test gửi `ResendRequest` sau khi mở lại và đòi **replay**, không phải gap fill |
| Mở lại rồi nhưng `Logon` đầu tiên vẫn mang số cũ vì reset đến sau | Khẳng định `34=` trên byte đi ra, không chỉ khẳng định `next_out` |

## Tài liệu phải cập nhật

- [ ] ADR mới — engine hỏi ai đó chứ không đoán; `Recovery` đối xứng với `Registry`
- [ ] `DESIGN.md` §3 và §4 D7
- [ ] `CHANGELOG.md` — API công khai
- [ ] `GUIDE.md` — mục phục hồi, và sửa §5a lẫn §6
- [ ] `STATUS.md` item 31, và item 16 phải nói rõ phạm vi đóng của nó hẹp hơn nó đọc ra
- [ ] `PRD.md` §2
- [ ] Đi lại bảng §4 từng dòng, và đọc lại *Not proven* từng dòng

## Ngoài phạm vi

- **Định dạng journal không đổi.** `last_active_ms` được người nhúng lưu ở đâu là việc của họ
  cho tới khi có bằng chứng nói khác.
- **Initiator tự kết nối lại.** Plan initiator đang tạm dừng.
- **Reset theo lệnh vận hành** — đó là item 30 (c).

## Nhật ký giao hàng

> Điền khi đóng từng bước.
