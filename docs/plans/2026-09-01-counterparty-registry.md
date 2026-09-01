# Nhiều counterparty trên một acceptor — registry trong `presession`

> **Loại:** Plan · **Ngày:** 2026-09-01 · **Trạng thái:** Đã duyệt
> **Phạm vi:** `STATUS.md` open item 28. Chạm `engine` (`presession`, `shard`, các entry point)
> và **API công khai** — không chạm `codec`, không chạm `session` trừ một chỗ đọc `Config`.

> **Máy chạy:** plan này **cố ý chọn được toàn bộ trên macOS.** Mọi gate của nó là test, không
> phải phép đo. Hai thứ **không** làm được trên Mac và plan nói rõ ở bước 6.

## Bối cảnh

`[verified 2026-09-01]` **Engine này phục vụ đúng một counterparty.** `Config` ghim
`target_comp_id` (`crates/session/src/lib.rs:259`); logon đòi `49=` vào phải khớp nó và `56=`
khớp của ta (`:1154`–`:1157`); `serve`, `serve_hft` và `serve_sharded_hft` mỗi cái nhận **một**
`Config`, và `serve_sharded_hft` đưa **cùng một cái** cho mọi shard
(`crates/engine/src/shard.rs:410`, `:431`).

Một FIX gateway của broker theo định nghĩa là multi-counterparty. Thiếu cái này thì đây là **một
đường point-to-point, không phải một acceptor** — và đó là khoảng cách lớn nhất trong `PRD.md` §3.

**Trớ trêu là bộ máy cho điều ngược lại đã dựng xong và không có chỗ để gửi đi đâu.**
`presession::identity_of` đọc `(49, 56)` khỏi `Logon` và `HashRoute` rải các danh tính khác nhau
ra các shard — mà mỗi shard từ chối mọi danh tính trừ một. **Định tuyến theo danh tính hôm nay là
chọn giữa những engine đều nói không.**

## Những gì đã biết chắc

- **[ADR-0026](../decisions/ADR-0026-a-counterparty-registry-in-the-pre-session-stage.md)
  `Accepted`** quyết định hình dạng: registry nằm trong `presession`, là **trait**, `lookup` **đồng
  bộ**, xác thực là `lookup` trả `None`, không có mặc định, và một `Engine` vẫn mang một `Config`.
- **[ADR-0020](../decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md)** đã đặt
  quyết định *"shard nào"* vào `presession`. *"Config nào"* là cùng một quyết định, sớm hơn một
  trường.
- `[documented 2026-09-01]` cả QuickFIX, QuickFIX/J (`DynamicAcceptorSessionProvider`) và Artio
  (`AuthenticationStrategy`) đều quyết định **tại `Logon`, trong tầng đang accept**, qua **provider**
  chứ không phải bảng cố định — [prior-art.md](../reference/prior-art.md).
- `[verified]` `presession::Identity` hiện là `(49, 56)` **thôi**. Artio có `SessionIdStrategy`
  (có thể gồm SubID, LocationID), QuickFIX có `SessionQualifier`. **Counterparty phân biệt bằng
  `50=`/`57=` hôm nay không phục vụ được.**
- `[measured 2026-09-01]` nền xanh để so: `cargo test --all` → **272 passed, 0 failed**, 56 binary.
- `[measured 2026-09-01]` giá của tầng pre-session hiện tại: sweep **426.2 ns/socket**, đọc hai
  comp ID và chọn shard **84.0 ns**, một lần mỗi connection — `crates/engine/benches/presession.rs`.

## Cách làm

**Một trait, một implementation mặc định, và mọi entry point đổi chữ ký.**

```rust
// crates/engine/src/presession.rs
pub struct Entry { cfg: Config, /* journal handle, policy sau này */ }

pub trait Registry {
    fn lookup(&self, id: Identity<'_>) -> Option<&Entry>;
}

/// Implementation mặc định: một bảng dựng lúc khởi động. Rỗng thì từ chối tất cả.
pub struct Table { /* Vec<(Key, Entry)> — tuyến tính, xem bước 5 */ }
```

`PendingSet` giữ socket tới khi có `Logon` như hiện nay; khi có, nó gọi `lookup`. `None` → xử lý
**y hệt** một danh tính không hợp lệ hôm nay: bỏ socket, đếm vào `Refused`, không trả lời gì.
`Some(entry)` → `Shards::hand` định tuyến như cũ, và engine nhận được `Config` từ `entry`.

File đụng tới: `crates/engine/src/presession.rs` (chính), `crates/engine/src/shard.rs`
(`serve_sharded_hft` nhận registry thay cho `cfg`), `crates/engine/src/lib.rs` (`serve`,
`serve_hft`), `crates/engine/tests/presession.rs`, `tests/shard_wire.rs`, và một file test mới.

**`Engine` vẫn mang một `Config`.** Registry quyết định *engine nào*, không làm một engine thành
đa danh tính — đó là điều giữ cho luật single-logon còn trả lời được, và giữ cho trần 4 của
ADR-0025 còn nghĩa.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **1 — không cấp phát trên hot path** | `lookup` nằm trên đường **connection**, không phải đường message — nhưng `benches/alloc.rs` đang chứng minh ba case pre-session là 0 | Thêm case `lookup` vào `benches/alloc.rs`. **`Table` cấp phát lúc dựng, không cấp phát lúc tra.** Đây là bất biến dễ vi phạm nhất của plan này |
| **3 — 59 định nghĩa là cổng của session layer** | `shard_wire.rs` đang chạy corpus qua hai shard | Phải vẫn **59/59 qua một shard và qua hai**, và số socket bị loại phải vẫn đúng hai, đúng tên (ADR-0022) |
| **5 — thứ tự trường từ bảng sinh** | không đụng | — |
| **7 — không `unwrap`/`expect`/`panic`** | `Registry` là API công khai, lỗi phải là kiểu | `lookup` trả `Option`, không `Result` — không có gì để `unwrap` |
| **2 — session thuần** | `Entry` giữ `Config`, `session` không biết gì về registry | `session` không đổi một dòng nào |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | **Test đặc tả, đỏ trước.** Hai counterparty, hai `Config`, một acceptor: cả hai logon được, mỗi bên thấy đúng comp ID của mình. Đỏ vì hôm nay không dựng nổi | — |
| 2 | `Entry`, `Registry`, `Table`. `lookup` gọi từ `PendingSet`. Bước 1 xanh | 1 |
| 3 | `Identity` mang thêm `50=`/`57=` khi có; khoá do implementation của `Registry` quyết định | 2 |
| 4 | Mọi entry point đổi: `serve`, `serve_hft`, `serve_sharded_hft` nhận registry. `CHANGELOG.md` | 2 |
| 5 | **Giá của `lookup`**: case mới trong `benches/presession.rs` và `benches/alloc.rs` | 2 |
| 6 | Đóng plan: docs theo §4, và **con số của bước 5 chỉ công bố từ máy §9** | 1–5 |

## Cách kiểm chứng

**Bước 1 phải đỏ trước, và đỏ vì đúng lý do.** Không phải "không compile" — mà là compile được và
**một trong hai counterparty bị từ chối**, vì `Config` chỉ mang được một `target_comp_id`. Ghi lại
output đỏ đó vào nhật ký.

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 1 | `cargo test -p fixbolt-engine --test registry` | **đỏ**, và thông điệp nói đúng *counterparty thứ hai bị từ chối* |
| 2 | như trên | xanh |
| 2 | `cargo test --all` | **≥ 272 passed, 0 failed** — nền đã đo 2026-09-01 |
| 2 | `cargo test -p fixbolt-engine --features affinity --test shard_wire` | **59/59 qua một shard và qua hai**, số socket bị loại vẫn đúng **2**, đúng tên |
| 3 | test riêng: hai counterparty **cùng `(49,56)`, khác `57=`** | cả hai logon được, và với `Table` chỉ khoá theo comp ID thì cái thứ hai bị từ chối — **hai hành vi, cùng một code, do implementation quyết định** |
| 4 | `cargo test --all --no-default-features` + `scripts/check-no-optional-deps.sh` | xanh |
| 5 | `scripts/bench.sh` | `lookup` **0 cấp phát**; ba case pre-session cũ vẫn 0 |

**Đảo ngược, bắt buộc:** cho `lookup` luôn trả `Some(entry_đầu_tiên)` → test bước 3 phải đỏ. Nếu
nó vẫn xanh thì test đang không đo cái nó nói.

**Registry rỗng phải từ chối mọi thứ**, và có test riêng — một acceptor nhận một danh tính không ai
cấu hình là một cổng mở.

## Tài liệu phải cập nhật

- [ ] `DESIGN.md` §3 — dòng `presession` mô tả `Registry`, `Entry`, `Table`
- [ ] `DESIGN.md` §6 — nếu shard corpus gate đổi cách chạy
- [ ] `GUIDE.md` §1a — **viết lại**: hôm nay nó dạy người đọc dựng acceptor một counterparty
- [ ] `GUIDE.md` §9 — bỏ mục "không phục vụ nhiều counterparty" nếu có, thêm ràng buộc registry rỗng
- [ ] `CHANGELOG.md` — API công khai đổi ở cả ba entry point
- [ ] `PRD.md` §3 — bốn dòng gap: many counterparties, logon auth (một phần), config file (chưa)
- [ ] `STATUS.md` — item 28
- [ ] Đi lại bảng §4 **từng dòng**, và **đọc lại mục "Not proven" từng dòng** — item 27, và
      `[measured 2026-09-01]` mục đó đã có **chín** bullet sai

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| `lookup` cấp phát → hỏng bất biến 1 trên đường connection | case mới trong `benches/alloc.rs`, đảo ngược bằng cách cho nó `to_vec()` |
| Test xanh vì cả hai counterparty trùng shard | test bước 3 khẳng định **shard id khác nhau**, không chỉ khẳng định cả hai logon được |
| 59/59 giữ nguyên nhưng một socket bị tầng pre-session vứt lặng lẽ | `shard_wire.rs` đã đếm mọi lần loại bỏ và đòi đúng 2, đúng tên — ADR-0022. **Cái thứ ba là lỗi mới đội lốt 59/59** |
| `Table` tuyến tính chậm khi nhiều counterparty | bước 5 đo; **không tối ưu trước khi đo**, và 40 entry quét tuyến tính có thể đã đủ |
| Registry rỗng vô tình cho qua | test riêng, và nó phải đỏ nếu ai đổi `None` thành mặc định |
| Đổi chữ ký entry point làm hỏng `tools/w2w` lặng lẽ | `cargo test --all` gồm `w2w`; và `scripts/check-no-kernel-sleep.sh` chạy nó thật |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Chữ ký công khai đổi ở cả ba entry point | **Cao**, nhưng chưa publish gì | `CHANGELOG.md`, và sửa hết call site trong repo — đó là toàn bộ cái giá hôm nay |
| Một journal mỗi counterparty = 40 mmap, 40 writer thread | Trung bình | **Ngoài phạm vi**: `Entry` giữ `Config` thôi ở plan này. Câu hỏi mở 2 của ADR-0026 |
| `Identity` rộng ra làm mọi consumer thêm nhánh | Thấp | Trường tuỳ chọn, và bước 3 có test cho cả hai trạng thái |
| Session schedule cũng thuộc về `Entry` | Thấp | Câu hỏi mở 3 của ADR-0026 — **để plan session-schedule quyết**, không đoán ở đây |

## Ngoài phạm vi

- **Xác thực bằng `553`/`554` và IP allowlist.** ADR-0026 nói chúng thuộc về `Entry`; plan này chỉ
  dựng chỗ cho chúng. `lookup` trả `None` đã là một authenticator.
- **Journal riêng cho mỗi counterparty** — câu hỏi mở 2 của ADR-0026.
- **Hot reload.** Trait giữ cánh cửa mở; plan này không mở nó.
- **File cấu hình.** `Table` dựng bằng code. Định dạng file là gap riêng trong `PRD.md` §3.
- **Session schedule** — plan riêng.
- **Mọi thứ trong open item 30** (vận hành) — plan riêng.

## Nhật ký giao hàng

> Điền khi đóng từng bước: dựng gì, gate nào xanh, cái gì chưa làm và vì sao.

**Trước khi bắt đầu, trên máy mới:** `scripts/fetch-quickfix-assets.sh` — `vendor/` bị gitignore
và 59 định nghĩa nằm trong đó, không có nó thì cổng quyết định mọi thay đổi session layer không
chạy được.

**Về máy Mac, và đây là ràng buộc thật của plan này.** Mọi gate ở mục *Cách kiểm chứng* là **test**
và chạy được trên macOS. **Hai thứ thì không:**

1. **Con số của bước 5.** `scripts/bench.sh` chạy được ở đâu cũng được và **số đếm cấp phát là
   độc-lập-máy** — nên phần *"`lookup` cấp phát 0 lần"* đóng được trên Mac. Nhưng **con số nanosecond
   thì không**: `DESIGN.md` §9 và bất biến 10 đòi máy bare-metal đã chỉnh, và `benches/baselines.tsv`
   khoá theo CPU model. Một CPU lạ đọc ra `NO BASELINE`. **Đừng công bố ns nào từ Mac** — đó đúng là
   cái `CLAUDE.md` §10 gọi tên: *"một con số trích từ laptop như thể nó đến từ máy Linux"*.
2. **`scripts/check-no-kernel-sleep.sh` và `check-standard-gives-the-core-back.sh`** — cả hai
   Linux-only (`strace`, `/proc`). CI chạy chúng; Mac thì không. **Đừng coi việc chúng không chạy là
   xanh.**

Nên: đóng bước 1–4 trên Mac bằng test, đóng nửa *cấp phát* của bước 5 trên Mac, và **để nửa
nanosecond của bước 5 lại cho một buổi ở desktop §9**. Nói rõ điều đó khi đóng plan, thay vì để
người sau tưởng con số đã có.

**Và bất biến cuối, từ §9:** plan chỉ đóng khi **một CI run xanh được gọi tên bằng id, cho đúng
commit đang đóng.** Laptop nói gate xanh *cho anh*; chỉ CI nói nó xanh *cho commit*.
