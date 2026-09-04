# Một engine biết nói trước

> **Loại:** Plan · **Ngày:** 2026-09-05 · **Trạng thái:** **Xong** (2026-09-05) — xem *Nhật ký giao hàng*
> **Phạm vi:** `STATUS.md` item 46. Chạm `engine` (`dispatch`, `observe`, `conn`, entry point),
> `library` (`Handler`, `Reply`), **`session` — ba chỗ cộng thêm, xem Sửa 1**, docs, một ADR mới.
> **Không chạm** `codec`, `dict`.
>
> **Máy chạy:** macOS đủ cho toàn bộ test; gate quyết định là job `interop` trong CI.
> **Thời lượng dự kiến:** 2–3 ngày.

> ## Sửa 1 `[2026-09-05]` — cửa 1 không thể mở mà không chạm `session`, và đây là ba chỗ
>
> **Ghi trước khi viết dòng code nào.** Dòng phạm vi ở trên nói *"không chạm `session`"* và nêu
> lý do: `Session::send_application` đã có sẵn. Lý do đó vẫn đúng — **không một dòng logic nào
> của session machine đổi** — nhưng nó trả lời sai câu hỏi. Câu hỏi là *engine gọi tới ứng dụng
> bằng đường nào*, và đường đó đi qua trait `Application`, trait này **ở trong `crates/session`**.
>
> Đọc code 2026-09-05 mới thấy:
>
> | Chỗ | Sự thật | Vì sao chặn cửa 1 |
> |---|---|---|
> | `serve_with` (`engine/src/lib.rs:1336`) | nhận `A: Application` rồi **tự** bọc `InlineDispatch::new(app)` | `library` không cấy được một `Dispatch` của riêng nó vào; sáu entry point `*_with` vừa chốt xong sẽ phải đổi hết nếu muốn |
> | `InlineDispatch<H>` (`engine/src/dispatch.rs:114`) | `impl<H: Application> Dispatch for InlineDispatch<H>` | muốn `Dispatch::on_logon` chạy tới `App` thì `Application` phải mang được method đó; Rust không có specialization để cấy một mặc định cho mọi `H` |
> | `Config` (`session/src/lib.rs:285`) | có `inbound_sender_matches` / `serves` / `same_identity_as` — **toàn predicate, không có getter** | `Counterparty` mà `on_logon` nhận cần đọc `begin_string`, `sender_comp_id`, `target_comp_id` |
>
> **Ba thứ được cộng vào `crates/session`, tất cả đều cộng thêm và trơ:**
>
> 1. `Application::on_logon(...) -> Option<Range<usize>>` với **thân mặc định trả `None`** — mọi
>    `impl Application` đang tồn tại vẫn biên dịch không sửa một chữ.
> 2. Ba getter trên `Config`: `begin_string()`, `sender_comp_id()`, `target_comp_id()`.
> 3. Không có gì nữa.
>
> **`Session` không gọi `on_logon`. Engine gọi.** Nên bất di bất dịch 2 không bị đụng: không
> socket, không clock, không cấp phát, không `format!` nào được thêm vào session layer, và
> `--test score` 59/59 là thứ chứng minh chuyện đó chứ không phải đoạn văn này.
>
> **Ba phương án đã loại, để lần sau khỏi mở lại:** (a) đổi sáu entry point thành generic trên
> `D: Dispatch` — phá API vừa chốt hôm qua, đắt hơn nhiều lần thứ nó mua; (b) specialization —
> không có trên stable; (c) bỏ cửa 1, để `tools/interop` dùng `Sender` từ thread khác sau khi
> đọc `EventKind::LoggedOn` — **chạy được**, không chạm `session` dòng nào, nhưng bắt mọi người
> dùng phải dựng một thread quan sát chỉ để nói câu đầu tiên, và biến thứ acceptor C++ làm bằng
> ba dòng `onLogon` thành một bài tập về đồng bộ.
>
> **Chủ repo đảo được quyết định này bằng cách chọn (c)**: bỏ bước 2, giữ nguyên bước 3, và cửa 1
> biến mất khỏi plan. Nếu vậy thì hai bước interop đỏ vẫn xanh được, chỉ là xanh theo cách khác.

> ## Sửa 2 `[2026-09-05]` — `Outbox` không tồn tại; `Reply::originate` và `Peer` thay chỗ nó
>
> Mục *Cách làm* dưới đây mô tả một type `Outbox` mượn, không bị nuốt sau một message, và một
> `Counterparty`. **Dựng ra thì cả hai đều thừa**, và cái thay chúng nhỏ hơn:
>
> - **`Outbox` → `Reply::originate`.** `Reply` đã làm đúng mọi thứ `Outbox` cần làm; điểm khác
>   duy nhất là một origination **không có** `34=`/`52=` để mang. Nên `Reply` giữ `seq:
>   Option<u32>` và `message()` chỉ ghi hai tag đó khi có. Một type mới cho một `Option` là một
>   type không đáng.
> - **Engine sở hữu vòng lặp, không phải ứng dụng.** Plan viết `on_logon(..., out: &mut Outbox)`
>   ghi N message. N message dài không biết trước cần một buffer cỡ tệ-nhất — một hằng số không
>   ai chọn được — hoặc một cấp phát, mà bất di bất dịch 1 cấm. Nên engine hỏi lại: `nth = 0, 1,
>   2, …` cho tới khi ứng dụng trả `silent()`, mỗi lần một message, một buffer dùng chung, chặn
>   trên bằng `MAX_ON_LOGON`.
> - **`Counterparty` → `fixbolt_session::Peer`.** Plan quên `begin_string`, và `App` không lấy
>   được nó từ đâu khác vì `on_logon` không có message vào. Ba `&[u8]` liền nhau là đúng thứ
>   `JournalHealth` đã tồn tại để tránh, nên gom thành struct và để nó ở `session` cạnh trait
>   dùng nó.

> **Vì sao plan này đứng trước đợt B**, dù item 45 xếp nó *trong* đợt B cạnh
> `settings-for-both-roles`. `[quyết định 2026-09-05]` Một acceptor chỉ biết trả lời thì không
> phải acceptor một desk chạy được — đây là lỗ hổng **sản phẩm**, không phải lỗ hổng tiện nghi,
> và ba plan còn lại của đợt B đều là knob. `settings-for-both-roles` mang sẵn helper `35=j` trên
> `Reply`, mà `35=j` ngoài luồng chính là **một trong bốn thứ item 46 nói engine không gửi được**
> — nên làm ngược thứ tự thì helper đó ra đời không có cửa để đi ra. Item 45 được sửa cùng commit
> với plan này.

## Bối cảnh

`fixbolt::serve` không cho ứng dụng **tự khởi xướng** một message. Mọi application message engine
này gửi được đều là *câu trả lời*, trả về từ `Handler::on_message` cho một message vừa tới.

Hệ quả, nguyên văn item 46: không có `ExecutionReport` cho một fill về sau lệnh một giây, không
có luồng quote, không có `35=j` ngoài luồng, và **không có gì để nói với một counterparty đang
kết nối và im lặng**.

Chuyện này lọt qua mọi gate vì một lý do có thể phát biểu thành câu:
**một thành phần mà mọi test đều lái từ bên ngoài thì không thể lộ ra cái nó không tự bắt đầu
được.** 59 file `.def`, `end_to_end.rs`, `interop.sh` và `w2w` đều là *kích thích → phản hồi*, nên
một năng lực không cần kích thích nằm ngoài hệ toạ độ của chúng. Đầy đủ ở
[an-acceptor-that-can-only-answer](../reference/an-acceptor-that-can-only-answer.md).

Nó được tìm ra bằng cách **chĩa hai vai của chính repo này vào nhau** trước khi có counterparty
C++: acceptor fixbolt không gửi được hai `35=B` không ai hỏi mà vai initiator chờ ở bước 2 và 5.

## Những gì đã biết chắc (đọc code 2026-09-05)

| Sự thật | Nguồn |
|---|---|
| `Handler::on_message(&mut self, msg, reply) -> Answer` — một message, trả lời một lần | `crates/library/src/app.rs:117` |
| `Reply::message()` **nuốt `self`**, `send()` trả `Answer` — nên một `Reply` là đúng một message | `crates/library/src/reply.rs:169,230` |
| `Command` có **ba** biến thể, cả ba chỉ dời số thứ tự | `crates/engine/src/observe.rs:772` |
| `Conn::send_application(&mut self, msg, at_ms, log)` **đã tồn tại**: session quyết số thứ tự, `SendingTime`, link còn sống hay không; flush ngay trong hàm | `crates/engine/src/conn.rs:206–245` |
| `Session::send_application` là hàm thuần, nhận journal và một closure ghi bytes | `crates/session/src/lib.rs:1764` |
| `Dispatch::collect` + `const OUT_OF_BAND: bool` — engine gom cái thread khác sinh ra rồi đẩy qua `send_application`; `false` cho `InlineDispatch` nên cả khối biến mất khi biên dịch | `crates/engine/src/dispatch.rs:43,65,114`; `crates/engine/src/lib.rs:971–988` |
| `Admin` là `Arc<Shared>`, `Send + Sync + Clone`; `submit` trả `bool` — **hàng đợi đầy thì từ chối tại chỗ gọi, không bao giờ im lặng**; `drains()` đếm số lần engine với tay tới lock | `crates/engine/src/observe.rs:942–1000` |
| `Events::push` dùng `try_lock` **không bao giờ `lock`** — bất di bất dịch 4; từ chối được đếm là mất | `crates/engine/src/observe.rs:685` |
| `EVENT_CAPACITY = 256`, ring cố định, `[Option<Event>; N]` | `crates/engine/src/observe.rs:526` |
| `benches/alloc.rs` của `engine` đang có **24 path**, mỗi case tự khẳng định path của nó còn sống | `DESIGN.md` §6 |
| `tools/interop` bước `news` chờ **hai** `|35=B|`, bước `resend` chờ chúng phát lại ở `34=[2,3]`; hôm nay hai bước này **đỏ theo thiết kế** khi acceptor là fixbolt | `tools/interop/src/main.rs:462–547` |
| `scripts/interop.sh` chạy bảy bước cho mỗi vai: `logon news heartbeat testrequest resend gapfill logout` | `scripts/interop.sh:154` |
| `cargo test --all` hiện **495** | `STATUS.md` 2026-09-05 |

## Cách làm

**Một nguyên thuỷ, hai cánh cửa.** Nguyên thuỷ đã có — `Conn::send_application`. Việc của plan
này là mở hai cửa tới nó và không mở cửa nào khác.

### Nguyên thuỷ: `Outbox`

Một handle **mượn**, sống trên engine thread, trỏ vào đúng một connection, ghi được **N** message
application, mỗi cái đi qua `Conn::send_application`. Nó dựng message bằng chính bộ dựng của
`Reply` (`message` / `field` / `group` / `send`), để ứng dụng viết một message giống hệt nhau ở cả
hai cửa. Khác `Reply` đúng một điểm: `Outbox` **không** bị nuốt sau một message.

`Outbox` không giữ số thứ tự, không giữ `SendingTime`, không chạm socket. Ứng dụng đưa thân
message; session gắn header. Một ứng dụng đưa `34=` hay `52=` sai không làm hỏng được luồng, vì
nó không bao giờ được hỏi hai thứ đó.

### Cửa 1 — `Handler::on_logon`, trên engine thread, không hàng đợi

```rust
fn on_logon(&mut self, _who: &Counterparty<'_>, _out: &mut Outbox<'_, P, S>) {}
```

Thân mặc định rỗng, nên **mọi `Handler` đang tồn tại vẫn biên dịch**. Gọi đúng một lần cho mỗi
connection, ngay sau khi session lên. Đây là cửa mà ba dòng `onLogon` của acceptor C++ đang dùng,
và là cửa làm hai bước interop đỏ chuyển xanh.

### Cửa 2 — `Sender`, từ thread khác

`Send + Sync + Clone`, cùng `Arc<Shared>` mà `Admin` và `Observer` đang cưỡi, cùng phép chia năng
lực: một `Observer` không gửi được, một `Sender` gửi được.

```rust
pub fn send(&self, id: ConnId, msg: &[u8]) -> bool;   // false = hàng đợi đầy, KHÔNG lấy gì
```

Hàng đợi: slot cố định, cấp phát một lần lúc dựng, hình dáng của `MemJournal` (`SLOTS` slot ×
`LEN` byte) chứ không phải `Vec`. Engine rút ở **đầu turn**, cùng chỗ `Command` được áp, bằng
`try_lock` — **không bao giờ `lock`** — và mỗi lần rút được đẩy qua `Conn::send_application` y
như khối `OUT_OF_BAND` đang làm. Một `Sender` chỉ về connection đã chết thì message bị bỏ, cố ý,
đúng lý do khối `OUT_OF_BAND` đang nêu: session sở hữu số thứ tự của nó đã đi cùng nó.

Engine chưa ai xin `Sender` phải trả **đúng một `Option` check** mỗi turn, không phải một lần với
tay tới lock. Giữ được điều đó bằng bộ đếm `drains()`, y hệt `Admin`.

### Thứ tự trong một turn, phát biểu chứ không để tự suy

Rút `Sender` **trước** khi đọc socket, cùng chỗ với `Command`. Lý do: một message khởi xướng và
một câu trả lời cho message vừa đọc thì cái khởi xướng đã chờ sẵn từ turn trước, nên nó đi trước.
Có test canh, vì đây là thứ không đọc code mà đoán ra được.

### File sẽ tạo hoặc sửa

`crates/engine/src/origin.rs` (mới) · `crates/engine/src/observe.rs` (`Shared`, `Sender`) ·
`crates/engine/src/lib.rs` (rút ở đầu turn, `Engine::sender()`) · `crates/engine/src/conn.rs` ·
`crates/library/src/app.rs` (`Handler::on_logon`, `Counterparty`) · `crates/library/src/reply.rs`
(`Outbox` dùng chung bộ dựng) · `crates/library/src/lib.rs` (re-export) ·
`tools/interop/src/main.rs` (vai acceptor gửi hai `35=B` on logon) ·
`docs/decisions/ADR-0048-*.md` (mới).

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **1 — không cấp phát trên hot path** | việc rút chạy trên engine thread mỗi turn | hai case mới trong `crates/engine/benches/alloc.rs`: `origin-idle` và `origin-busy`, mỗi case tự khẳng định path còn sống; đảo bằng cách nhét một `format!` vào việc rút, phải đọc ra số khác 0 |
| **4 — engine thread không ngủ (`hft`)** | hàng đợi mới có lock | `try_lock`, không bao giờ `lock`; `scripts/check-no-kernel-sleep.sh` chạy lại và **không được** thấy `futex` trên engine thread |
| **2 — session thuần** | không đụng | `Session::send_application` dùng nguyên trạng, không thêm tham số nào |
| **5 — thứ tự field từ bảng sinh** | `Outbox` dựng message | dùng chung đường của `Reply`, đi qua `Template::encode_with::<D>` |
| **10 — số nào cũng có benchmark, máy, §9** | thêm việc vào turn | `benches/turn.rs` chạy lại; nếu `engine turn, 1 idle session` lệch ra ngoài band thì đó là kết quả, ghi vào `baselines.tsv` chứ không nới band |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | ADR-0048 `Proposed`: vì sao hai cửa chứ không một, vì sao `Sender` không phải là biến thể thứ tư của `Command` (`Command` là `Copy`, thân message thì không), giá phải trả | — |
| 2 | `Outbox` + `Handler::on_logon`, thân mặc định rỗng. Test: một handler im lặng vẫn xanh; một handler gửi hai message thì cả hai lên dây, đúng số thứ tự session cấp | 1 |
| 3 | `Sender`, hàng đợi, rút ở đầu turn, `Engine::sender()`. Test: gửi từ thread khác; hàng đợi đầy trả `false`; connection đã chết thì bỏ; `drains()` chứng minh engine không ai dùng không với tay tới lock | 2 |
| 4 | `tools/interop` vai acceptor gửi hai `35=B` on logon; bước `news` và `resend` chuyển từ **đỏ theo thiết kế** sang xanh, cho cả hai vai | 2 |
| 5 | Hai case `benches/alloc.rs`, `benches/turn.rs` chạy lại, ADR `Accepted`, toàn bộ bảng đồng bộ §4 | 3, 4 |

## Cách kiểm chứng

- **Gate quyết định là bước 4, và nó là ý kiến của một implementation khác.** `scripts/interop.sh`
  chạy cả hai vai; `news` và `resend` của vai acceptor hôm nay đỏ **theo thiết kế** và tự khai
  điều đó trong self-check bước 1. Sau plan này chúng phải xanh, **và `tools/interop/src/main.rs`
  phải bỏ dòng self-check nói chúng được phép đỏ** — nếu quên bỏ, gate vẫn xanh mà không chứng
  minh gì, nên việc bỏ dòng đó là một mục riêng chứ không phải dọn dẹp.
- **Đảo ngược, mỗi cái phải đỏ đúng chỗ định:** (a) `on_logon` để thân rỗng → `news` đỏ, `resend`
  đỏ, năm bước kia xanh; (b) `Outbox` lấy `34=` từ bytes ứng dụng đưa thay vì từ session → test
  số thứ tự đỏ trong khi `--test score` **vẫn 59/59**, và điều đó được ghi lại, vì nó nói corpus
  mù với chuyện này; (c) nhét `format!` vào việc rút → `origin-busy` đọc ra số khác 0.
- `cargo test --all` và `cargo test --all --no-default-features`, đọc **số test**, không đọc exit
  code — 495 phải tăng, và tăng bao nhiêu thì nói ra.
- `cargo clippy --all-targets -- -D warnings`; `scripts/check-lint-config.sh`;
  `scripts/check-no-optional-deps.sh`.
- `scripts/bench.sh` — hai case alloc mới, `turn` so với band của máy đang chạy.
- Trên Linux: `scripts/check-no-kernel-sleep.sh` cả hai lượt (`hft` phải sạch, `standard` phải
  sập bẫy).
- **Một CI run xanh, gọi tên bằng id, cho đúng commit đóng plan** — §9 hộp cuối.

## Tài liệu phải cập nhật

- [ ] `docs/decisions/ADR-0048-*.md` — mới
- [ ] `DESIGN.md` §4 — D4 mọc thêm cửa thứ hai, hoặc một D mới đứng cạnh nó; rồi **đi lại §2**
- [ ] `DESIGN.md` §8 nếu `benches/turn.rs` đổi hàng
- [ ] `docs/GUIDE.md` — ràng buộc compiler không canh được: `on_logon` chạy **trên engine thread**,
      chặn ở đó là chặn session layer; và một `Sender` đầy là mất message nếu người gọi không đọc
      `false`
- [ ] `docs/SESSION-BEHAVIOUR.md` — một message khởi xướng lấy số thứ tự và `52=` từ session, **gọi
      tên test canh nó**
- [ ] `docs/CONFIGURATION.md` — sức chứa hàng đợi `Sender` và mặc định của nó
- [ ] `crates/library/README.md` + rustdoc của `Handler`
- [ ] `CHANGELOG.md` — đổi public API của hai crate
- [ ] `docs/reference/an-acceptor-that-can-only-answer.md` — mục *What was done here*; **giữ nguyên
      marker `[to testing-skills]`**, nó chỉ được thay bằng link PR khi PR upstream mở (§11)
- [ ] `STATUS.md` — gạch item 46, sửa item 45 (thứ tự đợt B), và **đi qua mục *Not proven*** theo
      hàng cuối bảng §4

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Test lái từ bên ngoài không thấy được năng lực không cần kích thích — **chính cái bẫy đẻ ra item 46** | gate là `interop.sh`, do một implementation khác lái, không phải runner của repo này |
| `on_logon` trở thành cửa để chặn engine thread | rustdoc + `GUIDE.md`; ADR-0002 đã có sẵn câu chữ |
| Ứng dụng tự đặt `34=`/`52=` | đảo ngược (b) ở trên |
| Message khởi xướng dùng nhờ buffer của reply, đè lên một reply đang dở | scratch riêng; test gửi một reply và một message khởi xướng **trong cùng một turn**, đọc cả hai trên dây |
| Rút hàng đợi cấp phát | `origin-idle` / `origin-busy` |
| Rút hàng đợi chặn engine thread | `try_lock` + `drains()`, bản sao của `an_engine_nobody_is_administering_does_not_reach_for_the_lock` |
| Hàng đợi đầy mất message trong im lặng | `send` trả `false`; một `EventKind` cho phần bị bỏ; test đổ đầy rồi đọc |
| Backpressure: message khởi xướng đẩy `TX` qua ngưỡng D10 | đi qua `Conn::send_application`, vốn đã gọi `slow_consumer`; test một `Sender` bơm vào socket không ai đọc |
| Bỏ quên dòng self-check "hai bước này được phép đỏ" trong `tools/interop` | mục riêng ở bước 4 |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| `benches/turn.rs` xấu đi vì thêm một `Option` check mỗi turn | thấp | đo trước và sau trên cùng máy; nếu lệch band thì ghi số mới, không nới band |
| `Handler` là trait public, thêm method dù có thân mặc định vẫn là thay đổi API | trung bình | thân mặc định rỗng nên không phá build ai; `CHANGELOG.md` và ADR nói rõ |
| Hai cửa hoá ra là một cửa quá nhiều | trung bình | bước 1 là ADR chính vì thế; nếu ADR kết luận chỉ cần `Sender`, plan cắt bước 2 và nói ra |

## Ngoài phạm vi

`on_tick` — một callback mỗi turn cho counterparty im lặng: `Sender` đã trả lời nhu cầu đó từ
thread khác, và một callback trên hot path cần số đo riêng của nó. Helper `35=j` (thuộc
`settings-for-both-roles`). Sinh message theo lịch. Ứng dụng khởi xướng khi session **chưa** lên.

## Nhật ký giao hàng

### `[2026-09-05]` Cả năm bước xong. Gate quyết định **PASS 7/7**.

**Dựng gì, ở đâu**

| Bước | Kết quả | File |
|---|---|---|
| 1 | ADR-0048 `Proposed` | `docs/decisions/ADR-0048-*.md` |
| 2 | `Application::on_logon` (thân mặc định `None`) + `Peer` + ba getter `Config`; `Dispatch::on_logon`; `Engine::speak_first` + `MAX_ON_LOGON`; `Reply::originate`; `Handler::on_logon` | `session/src/lib.rs`, `engine/src/{lib,dispatch}.rs`, `library/src/{app,reply,lib}.rs` |
| 3 | `Sender`, hàng đợi cố định 64 × 512 B, rút đầu turn, `Engine::sender()` | `engine/src/origin.rs` (mới), `engine/src/{lib,observe}.rs` |
| 4 | `Desk::on_logon` gửi hai `35=B`; **bỏ dòng miễn trừ** trong `tools/interop` | `tools/interop/src/{desk,main}.rs` |
| 5 | Ba case alloc, D15, và toàn bộ bảng §4 | `benches/alloc.rs`, `DESIGN.md`, `GUIDE.md`, `CONFIGURATION.md`, `SESSION-BEHAVIOUR.md`, `CHANGELOG.md`, `STATUS.md` |

**Gate nào xanh** — máy: Apple M5, macOS, `cargo 1.95.0`. **Không phải máy §9**, nên ở đây không
có số thời gian nào cả, chỉ có đếm và pass/fail.

```
cargo test --all                     506 passed  0 failed   (495 trước đó, +11)
cargo test --all --no-default-features 502 passed  0 failed
cargo clippy --all-targets -D warnings  sạch
scripts/check-lint-config.sh         RED ok, GREEN ok
scripts/check-no-optional-deps.sh    ok
scripts/check-links.py               1385 link, 0 chết
cargo bench -p fixbolt-engine --bench alloc
    27 case, tất cả 0, gồm origin-idle origin-busy logon-first
interop, hai vai chĩa vào nhau       PASS 7/7   (5/7 trước đó)
```

**Đảo ngược, cái nào đỏ ở đâu** — mỗi cái chạy rồi khôi phục:

| Đảo | Kết quả |
|---|---|
| bịt `speak_first` trong `turn` | 3/5 test cửa 1 đỏ, **`--test score` vẫn 59/59** |
| bịt `originate` trong `turn` | 5/6 test cửa 2 đỏ, hai test cửa 1 vẫn xanh |
| bỏ relaxed-load trước `try_lock` | **đúng một** test đỏ: `drains()` đọc 20 thay vì 0 |
| tiêm `format!` vào hai đường gửi | `origin-busy` 2000, `logon-first` 16 |

**Ba thứ hoá ra không đúng như plan viết, đã ghi lại chứ không lặng lẽ đổi:**

1. **Sửa 1** ở đầu file: cửa 1 không mở được nếu không chạm `crates/session`. Ba thứ cộng thêm,
   đều trơ, và ba phương án đã loại được nêu tên.
2. **`Peer` thay ba tham số rời.** Plan viết `on_logon(nth, sender, target, out)`; thiếu
   `begin_string`, mà `App` không lấy được từ đâu khác vì không có message vào. Năm tham số vị
   trí là thứ `JournalHealth` đã tồn tại để tránh, nên gom thành struct.
3. **`logon-first` đo 16 lần gọi trên một session, không phải 500 session.** Hai lý do, cả hai
   tìm ra bằng cách làm sai trước: `Engine::add` cấp phát một lần mỗi connection (là setup, và
   không case nào khác trong file này đo nó), và **một identity chỉ được một connection**, nên
   500 session cùng `Config` để lại 499 bị loại — bộ đếm đọc `1 sends over 500 sessions`. Thả
   đầu `Loopback` cũng không giải phóng slot vì ống trong bộ nhớ không báo EOF.

**Cái tìm được mà plan không lường:** `35=B` chỉ có `148=` **lên tới dây, replay đúng ở resend,
và vẫn bị counterparty từ chối** — `FIX44.xml:294` bắt buộc cả `LinesOfTextGrp`. Hai bước của
cùng một gate nhìn cùng hai message và trả lời ngược nhau, vì một bước khớp byte còn một bước
đếm delivery. Ghi ở
[a-message-on-the-wire-is-not-a-message-delivered](../reference/a-message-on-the-wire-is-not-a-message-delivered.md),
có marker `[to testing-skills]`.

**Cái gì chưa làm, và vì sao**

- **`serve` không phát `Sender`** — cũng không phát `Observer` hay `Admin`, và chuyện đó có
  trước plan này. Nên qua cửa trước chỉ với tới được cửa 1. **Mở thành item 47**, không sửa ở
  đây: bước 3 của plan nói `Engine::sender()` và chỉ thế, mở rộng tại chỗ là đúng thứ Rule Zero
  ngăn.
- **Chưa chạy `scripts/interop.sh` với `libquickfix` thật** — cần build QuickFIX; job `interop`
  trong CI là nơi nó chạy, và **CI trên đúng commit này là hộp cuối của §9 chưa tick.**
- **Chưa có số nào trên máy §9.** Không đụng hot path của byte, nhưng `benches/turn.rs` có thêm
  một `Option` check mỗi turn và **chưa ai đo lại nó trên máy có baseline**.
