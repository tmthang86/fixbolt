# Nhiều counterparty trên một acceptor — registry trong `presession`

> **Loại:** Plan · **Ngày:** 2026-09-01 · **Trạng thái:** **ĐÃ ĐÓNG 2026-09-01**
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

---

### Bước 1 — test đặc tả, đỏ trước · 2026-09-01

**Dựng gì.** `crates/engine/tests/registry.rs`, 4 test. Hai cái là *đặc tả* và phải đỏ; hai
cái là *chốt chặn* và xanh ngay hôm nay — chúng tồn tại để cái đỏ kia không đổ tội nhầm chỗ.

| Test | Hôm nay | Nó khẳng định điều gì |
|---|---|---|
| `two_counterparties_log_on_to_one_acceptor` | **đỏ** | một acceptor, hai counterparty, mỗi bên thấy comp ID của chính mình |
| `the_second_configured_counterparty_is_served_when_it_connects_alone` | **đỏ** | cùng chuyện đó, nhưng `BETA` kết nối **một mình** — luật single-logon bị loại khỏi bức tranh |
| `the_corpus_counterparty_still_logs_on` | xanh | counterparty mà acceptor **có** cấu hình vẫn logon được |
| `relabelling_to_the_same_sender_reproduces_the_corpus_bytes` | xanh | bộ đổi nhãn `49=` là byte-exact, gồm cả `9=` và `10=` |

`BETA` không có trong corpus — không file `.def` nào cho hai counterparty nói chuyện với một
acceptor. Nên nó là `Logon` thật của corpus (`8=FIX.4.4|9=59|35=A|34=1|49=TW44|52=…|56=ISLD|98=0|
108=30|10=116|`) với đúng **một trường** bị viết lại, `9=`/`10=` tính lại — dẫn xuất từ bản thật
chứ không bịa (§7). Bộ đổi nhãn đó tự nó có test đảo ngược: `TW44` → `TW44` phải trả về đúng từng
byte của corpus.

**Output đỏ, nguyên văn:**

```
running 4 tests
test relabelling_to_the_same_sender_reproduces_the_corpus_bytes ... ok
test the_corpus_counterparty_still_logs_on ... ok
test the_second_configured_counterparty_is_served_when_it_connects_alone ... FAILED
test two_counterparties_log_on_to_one_acceptor ... FAILED

failures:

---- the_second_configured_counterparty_is_served_when_it_connects_alone stdout ----

thread 'the_second_configured_counterparty_is_served_when_it_connects_alone' (1212180) panicked at crates/engine/tests/registry.rs:441:5:
BETA was refused in silence: the acceptor sent it nothing.
An acceptor holds one `Config` and therefore one `target_comp_id`, so it can serve one counterparty. This is what ADR-0026's registry is for.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- two_counterparties_log_on_to_one_acceptor stdout ----

thread 'two_counterparties_log_on_to_one_acceptor' (1212181) panicked at crates/engine/tests/registry.rs:417:5:
BETA was refused in silence: the acceptor sent it nothing.
An acceptor holds one `Config` and therefore one `target_comp_id`, so it can serve one counterparty. This is what ADR-0026's registry is for.


failures:
    the_second_configured_counterparty_is_served_when_it_connects_alone
    two_counterparties_log_on_to_one_acceptor

test result: FAILED. 2 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

**Đỏ vì đúng lý do — và bản nháp đầu KHÔNG đỏ vì đúng lý do.** `[measured 2026-09-01]` lần chạy
đầu tiên đỏ ở **`TW44`**, cái mà acceptor hôm nay phải phục vụ được. Nguyên nhân: `N = 8` trong
chữ ký `Engine`, mà một `Logon` mang chín trường — parse hỏng, session từ chối trong im lặng, và
thông điệp lỗi vẫn đổ tội cho registry còn thiếu. Một cái đỏ gọi sai nguyên nhân tệ hơn không có
test. Sửa `N = 256` như `tests/wire.rs`, và **`the_corpus_counterparty_still_logs_on` được thêm
vào để lần sau máy nói ra điều đó thay vì tôi**. Ghi lại thành
[silence-before-a-logon-has-many-causes.md](../reference/silence-before-a-logon-has-many-causes.md)
— §4 nói dòng `docs/reference/` là ưu tiên cao nhất, và mọi bẫy đã ghi phải có test canh.

**Nền không xê dịch.** `cargo test --all --no-fail-fast` → **57 binary, 274 passed, 2 failed**.
Nền 2026-09-01 là 56 / 272 / 0 — và `[measured 2026-09-01]` nó được **chạy lại trên chiếc Mac
này trước khi viết dòng code nào** và ra đúng `56 binary, 272 passed, 0 failed`. Chênh lệch là
đúng file test mới: +1 binary, +2 xanh (hai chốt chặn), +2 đỏ (hai đặc tả). **Không một test cũ
nào bị sửa.**

**Gate đã chạy:** `cargo fmt --all -- --check` sạch; `cargo clippy --all-targets -- -D warnings`
sạch.

**Chưa làm, và vì sao.** Chưa đụng `presession.rs` — đó là bước 2. Hai script Linux-only chưa
chạy được ở đây; **chúng không chạy ≠ chúng xanh**. Chưa có con số nanosecond nào, và sẽ không
có từ máy này.

---

### Bước 2 — `Entry`, `Registry`, `Table`, và `lookup` từ `PendingSet` · 2026-09-01

**Dựng gì.** `presession::Entry`, `Registry` (trait), `Table` (mặc định, quét tuyến tính,
**rỗng từ chối tất cả**) và `One` (registry đúng một counterparty). `PendingSet<T, PRE>` →
`PendingSet<T, R, PRE>`; `Progress` thêm `unknown`; `Pending` thêm `config()`. Trên
`session`: `Config` thêm `serves`, `inbound_sender_matches`, `inbound_target_matches` — và
**chỗ kiểm tra `Logon` của session gọi đúng hai predicate đó**, nên phép so comp ID chỉ có
một nhà. `serve_sharded_hft` giữ nguyên chữ ký và tự dựng `One` bên trong; nâng registry lên
chữ ký công khai là bước 4.

**Hai test đặc tả của bước 1 xanh mà không bị sửa một dòng nào** — chỉ `gateway()` đổi. Đó
là hình dạng bước 1 được viết ra để chứng minh được điều này.

**Gate đã chạy, trên máy Mac:**

| Lệnh | Kết quả |
|---|---|
| `cargo test --all --no-fail-fast` | **57 binary, 280 passed, 0 failed** (nền 272 + 8 test mới) |
| `cargo test --all --no-default-features --no-fail-fast` | **280 passed, 0 failed** |
| `cargo bench --bench alloc` | `pending-idle 0 pending-busy 0 pending-cycle 0` — vẫn 0 với `lookup` trên đường |
| `scripts/check-no-optional-deps.sh` | ok, cả hai khẳng định |
| `cargo fmt`, `cargo clippy --all-targets --all-features -D warnings` | sạch |

**Hai phép đảo ngược, mỗi cái đỏ đúng chỗ định chứng minh:**

- `Table::lookup` bỏ qua identity, trả `entries.first()` → 4 test đỏ, gồm cả hai test đặc
  tả. `an_empty_registry_refuses_every_connection` vẫn xanh — **đúng**, vì bảng rỗng không
  có phần tử đầu; nó cần phép đảo riêng.
- `Step::Unknown` giữ socket thay vì bỏ → `an_empty_registry_refuses_every_connection` đỏ
  đúng dòng `"the socket was let go of, not held"`.

Khôi phục, 8/8 xanh.

#### Chỗ plan sai, và nó tốn hai vòng CI để thấy

Plan viết gate bước 2 là *"số socket bị loại vẫn đúng **2**, đúng tên"*. **Sai.** Registry
từ chối identity lạ ở tầng pre-session, sớm hơn một tầng so với session, nên
`1c_InvalidSenderCompID.def` (`49=WT`) và `1c_InvalidTargetCompID.def` (`56=DLSI`) chuyển
sang bị loại ở đây. Con số đúng là **4**, và đó là
[ADR-0029](../decisions/ADR-0029-the-pre-session-stage-enforces-four-definitions.md), sửa
số của ADR-0022.

**Vòng CI thứ nhất nói ngược lại, và nó không phải bằng chứng.** `[measured 2026-09-01]` run
[33509748294](https://github.com/tmthang86/fixbolt/actions/runs/33509748294) **xanh** trên
Linux, có `--features affinity`, tức `shard_wire.rs` thật sự chạy. Nhưng `pump()` đọc bốn
trường của `Progress` và registry vừa thêm trường thứ năm — **hai kết nối biến mất vào một ô
đếm không ai đọc**, trong khi `not_logon == 1`, `gone == 1`, `[timed_out, unrouted] == [0,0]`
và 59/59 đều vẫn đúng. Comment ngay dưới đó viết *"a THIRD connection disappearing here
would be a new defect wearing the same green"*: đúng về hình dạng, mù về thực thể, vì cái
bẫy phụ thuộc vào **có người nhớ nới nó ra**.

Sửa bằng máy chứ không bằng thói quen — `pump()` giờ phá cấu trúc `Progress` từng trường,
**không có `..`**, nên lý do loại bỏ tiếp theo sẽ làm gãy build tại đó. Ghi lại thành
[a-counter-that-must-be-remembered-is-not-a-counter.md](../reference/a-counter-that-must-be-remembered-is-not-a-counter.md).

**Vòng CI thứ hai là phép đo.** `[measured 2026-09-01]` run
[33512983304](https://github.com/tmthang86/fixbolt/actions/runs/33512983304), Linux,
`cargo test -p fixbolt-engine --features affinity`:

```
test one_shard_passes_all_fifty_nine_at_any_settle_bound ... ok
test two_shards_pass_all_fifty_nine_because_identity_decides_the_shard ... ok
```

`unknown == 2`, và **corpus vẫn 59 qua một shard và qua hai**. Quan sát được từ dây không
đổi; chỉ tầng sinh ra nó dời đi.

**Vì sao `shard_wire` không chạy được ở đây.** Nó là
`#![cfg(all(feature = "affinity", target_os = "linux"))]`. Trên Mac nó không compile, không
chạy, và clippy không đọc nó. Plan nói mọi gate của nó chạy được trên macOS — **điều đó sai
với gate này**, và đó là lý do hai commit được đẩy lên đỏ/chưa-chắc có chủ đích thay vì đoán.

**Tài liệu đã cập nhật trong cùng dải commit này:** `DESIGN.md` §3 (dòng `presession` part
three), `CHANGELOG.md` (mục BREAKING), ADR-0029, ADR-0022 (header trỏ tới ADR-0029),
`reference/a-counter-that-must-be-remembered-is-not-a-counter.md`.

**Chưa làm.** `GUIDE.md` §1a, `PRD.md` §3, `STATUS.md` item 28 — chờ bước 4 chốt chữ ký
entry point, rồi đóng ở bước 6. Chưa có con số ns nào. Hai script Linux-only vẫn chưa chạy
được ở đây.

---

### Bước 3–6 — sub-ID, entry point, giá của `lookup`, và đóng plan · 2026-09-01

#### Bước 3 — `Identity` mang `50=`/`57=`

`Identity` thêm `sender_sub`/`target_sub`, `identity_of` đọc khi có, và `Identity::comp_ids`
cho người dựng tay. **`HashRoute` cố ý KHÔNG băm chúng**: hai kết nối của cùng một
counterparty khác nhau ở `50=` vẫn phải về cùng một shard — đó chính là lỗi ADR-0020 sinh ra
để sửa. `Table` cũng bỏ qua, vì `Config` không có chỗ chứa; ai cần thì viết `Registry` riêng,
và `tests/registry.rs` có một cái tám dòng (`ByDesk`).

**Corpus cho không một cái bẫy:** `2r_UnregisteredMsgType.def` mang `150=0` (ExecType). Một
phép quét `50=` khớp bất kỳ đâu sẽ đọc ra SenderSubID từ một định nghĩa thật. Cái chặn là
phép quét theo đầu-trường của `field_value`, và test là
`a_tag_ending_in_fifty_is_not_a_sender_sub_id` — **không phải bịa ra**.

#### Bước 4 — và đây là chỗ ADR-0026 quyết định 5 sụp

Quyết định 5 nói *một `Engine` mang một `Config`, registry chọn **engine nào***. Không dựng
được: entry point phải dựng engine **trước khi** có kết nối nào, mà `Registry` là trait —
không liệt kê được, nên `serve()` không biết dựng bao nhiêu engine. Và sharding **đã** chọn
engine rồi, nên engine sẽ phải là một-cho-mỗi-*(shard × counterparty)* mà không có gì bắt hai
quyết định đó khớp nhau.

`1b_DuplicateIdentity.def` giải quyết bằng chính dòng đầu của nó: *"If two logons with the
**same SenderCompID/TargetCompID combination** logon the second one must be disconnected"* —
**theo identity**. Engine này cài nó thành *"có kết nối nào khác đang logon không"*, chỉ đúng
khi một engine giữ một identity. Và **không định nghĩa nào bắt được**, vì cả `1b` lẫn
`AlreadyLoggedOn` đều nối hai lần **cùng một counterparty**.

Bảng prior-art của chính ADR-0026 đã chỉ đúng hướng và bị đọc cho câu hỏi khác: QuickFIX,
QuickFIX/J và Artio đều giữ nhiều session trong **một** tiến trình accept. Không cái nào dựng
một engine cho một counterparty. → [ADR-0030](../decisions/ADR-0030-one-engine-holds-many-counterparties.md).

#### Bước 5 — giá của `lookup`

**Nửa cấp phát, đóng ở đây.** `benches/alloc.rs` thêm case `registry-lookup`: một `Table` 41
mục, mục được phục vụ nằm **cuối** để phép quét chạy hết chiều dài.

```
allocations: idle 0 send 0 recv 0 frame 0 turn 0 shard-turn 0 busy 0 ring 0
             interests 0 pending-idle 0 pending-busy 0 pending-cycle 0 registry-lookup 0
```

Đảo ngược (`Table::lookup` dựng khoá bằng `to_vec()`): đọc ra **100000** và assertion nổ. Bẫy
này bắt được thật.

**Nửa nanosecond, KHÔNG đóng ở đây.** `benches/presession.rs` thêm hai case
(`registry lookup of 1`, `registry lookup of 40`). Chúng chạy trên Mac và harness tự nói:

```
presession, registry lookup of 40      45.2 ns/op   NO BASELINE for 'Apple M5'
```

`benches/baselines.tsv` khoá theo CPU model, và một CPU lạ đọc ra `NO BASELINE`. **Không con
số ns nào ở đây được công bố** — chúng chờ một buổi ở desktop §9. Bất biến 10, và đúng cái
`CLAUDE.md` §10 gọi tên.

#### CI bắt được thứ tôi bỏ sót

`[measured 2026-09-01]` run [33520447994](https://github.com/tmthang86/fixbolt/actions/runs/33520447994)
**đỏ**: `pump` bị tôi gắn `#[cfg(feature = "standard")]`, mà `serve_hft` là entry point của
`hft` và tồn tại không cần feature đó — `--no-default-features` không tìm thấy `pump`. **Tôi
đã không chạy `--no-default-features` sau bước 4**, dù §7 bảo chạy mỗi bước. Đã sửa; giờ cả ba
cấu hình đều sạch:

| Cấu hình | `cargo test --all` | clippy `-D warnings` |
|---|---|---|
| mặc định | 57 binary, **284 passed, 0 failed** | sạch |
| `--all-features` | — | sạch |
| `--no-default-features` | 57 binary, **284 passed, 0 failed** | sạch |

`scripts/check-no-optional-deps.sh` ok cả hai khẳng định. `check-links.py` 653 link, không
link chết.

#### Bước 6 — tài liệu, và cái gì chưa đóng

Đã cập nhật cùng dải commit: `DESIGN.md` §3 (hai dòng `presession` part three và part four),
`CHANGELOG.md` (hai mục BREAKING), `GUIDE.md` **§1a0 mới** (viết lại cách dựng acceptor nhiều
counterparty, và ba thứ là *quyết định* chứ không phải mặc định), `PRD.md` §2 (dòng *many
counterparties* → DONE, và nói rõ **chưa có file cấu hình**), ADR-0026 (header trỏ tới
ADR-0030), ADR-0022 (header trỏ tới ADR-0029), ADR-0029, ADR-0030, và hai ghi chép
`docs/reference/`.

**Chưa đóng, và không nhận là đã đóng:**

1. **Không con số ns nào từ máy này.** Câu hỏi mở 1 của ADR-0030 là món nợ mới: phép so
   identity giờ là O(n²) theo số kết nối trên đường `turn`, nhiều nhất mười hai phép so dưới
   trần 4 của `hft`, **không chặn** dưới `standard`, và `benches/turn.rs` chưa chạy lại trên
   máy §9.
2. **`check-no-kernel-sleep.sh` và `check-standard-gives-the-core-back.sh`** Linux-only, không
   chạy được ở đây. CI chạy cả hai và chúng xanh ở đó.
3. **File cấu hình** vẫn chưa có — `Table` dựng bằng code. Là gap riêng trong `PRD.md`.

---

### Plan đóng · 2026-09-01

**CI xanh, gọi tên bằng id, cho đúng commit đang đóng** — `CLAUDE.md` §9:
run [`33521010564`](https://github.com/tmthang86/fixbolt/actions/runs/33521010564), commit
`61e5cd7`, **success**. Nó chạy cả phần Linux-only mà máy này không compile được:

```
test one_shard_passes_all_fifty_nine_at_any_settle_bound ... ok
test two_shards_pass_all_fifty_nine_because_identity_decides_the_shard ... ok
test the_same_identity_always_lands_on_the_same_shard ... ok
```

**Sáu bước, tất cả đóng, trừ hai thứ được nói rõ là không đóng:**

| Bước | Trạng thái |
|---|---|
| 1 test đặc tả đỏ trước | đóng — CI 33508641705 đỏ đúng hai test, trên Linux |
| 2 `Entry`/`Registry`/`Table` | đóng — và sinh ra ADR-0029 |
| 3 `Identity` mang `50=`/`57=` | đóng |
| 4 entry point đổi chữ ký | đóng — và sinh ra ADR-0030, siêu việt ADR-0026 quyết định 5 |
| 5 giá của `lookup` | **nửa đóng**: 0 cấp phát, có đảo ngược. Nửa ns để lại cho máy §9 |
| 6 đóng plan, docs §4 | đóng |

**Ba lần CI phản bác cái laptop tin:** (1) bước 1 đỏ trên Linux đúng như trên Mac; (2) run
33509748294 xanh **và không phải bằng chứng**, vì ô đếm mù; (3) run 33520447994 đỏ vì `pump`
bị gắn cờ `standard` mà `serve_hft` không cần cờ đó — lỗi của tôi, do không chạy
`--no-default-features` sau bước 4.

**Món nợ mới, ghi vào mục *Not proven* của STATUS.md:** phép so identity là O(n²) theo số kết
nối trên đường `turn` (câu hỏi mở 1 của ADR-0030), và chưa có counterparty nào được thêm vào
một acceptor đang chạy hay đọc từ file.
