# Tầng library — API cho ứng dụng, và ví dụ chạy được đầu tiên

> **Loại:** Plan · **Ngày:** 2026-09-02 · **Trạng thái:** Đã duyệt 2026-09-02
> **Phạm vi:** `DESIGN.md` §7 bước 8 — ô cuối cùng chưa dựng của cây phase 1 trong `PRD.md` §2

## Bối cảnh

Trong cây phase 1 ở `PRD.md` §2 còn đúng một ô ghi **`not started`**: `library`. `DESIGN.md`
§7 bước 8 gọi nó là *"the public API and the first end-to-end example"*, `DESIGN.md` §3 xếp nó
ở tầng L4 và cho nó phụ thuộc `engine`. `README.md` nói thẳng: *"The application-facing
`library` does not exist."*

Hôm nay engine **đã chạy được**: `serve()` mở cổng, registry chọn counterparty, session chạy
59/59 qua socket thật. Cái còn thiếu không phải năng lực mà là **chỗ đứng của người viết ứng
dụng**. Cụ thể, một người muốn trả lời một `NewOrderSingle` hôm nay phải:

1. Biết `Config` nằm ở `fixbolt_session`, `Table`/`Limits` ở `fixbolt_engine::presession`,
   `Settings` ở `fixbolt_engine::settings`, `Observer`/`Admin` ở `fixbolt_engine::observe`,
   `serve` ở gốc `fixbolt_engine` — **năm đường dẫn cho một việc**, và phải khai hai crate
   trong `Cargo.toml` của mình.
2. Tự `parse_into` lại đống byte engine vừa đưa cho, vì `Application::on_message` đưa
   `msg: &[u8]` thô.
3. Tự dựng `TemplateBuilder`, tự nhớ ghi `34`, `49`, `56`, `52` — trong đó `49`/`56` **phải
   đảo chiều** và `52` **phải là dấu thời gian của session chứ không phải của mình**.
4. Trả về một `Range<usize>` và nhớ rằng bản tin **không bắt đầu ở `out[0]`**.

Bốn việc đó, `crates/conformance/src/echo.rs` làm hết trong 60 dòng, và ba trong bốn cái bẫy
của nó được ghi ngay trong comment đầu file: `52` phải 21 byte có mili-giây, `60` phải chép
nguyên xi 17 byte, và danh sách `REGENERATED` sai một tag thì hỏng một file `.def` khác nhau.
**Một người viết ứng dụng không nên phải học lại ba cái bẫy đó.** Đó là việc của tầng L4.

Việc này **không mở rộng năng lực engine**. Nó chỉ dựng một mặt tiền, và mặt tiền ấy phải trả
giá bằng một lần parse thêm và một lần dựng template mỗi bản tin — cái giá đó là quyết định
đắt và khó đảo, nên có ADR riêng.

## Những gì đã biết chắc

- **`DESIGN.md` §3** xếp `library` là L4, phụ thuộc `engine`. **§7 bước 8** nói nội dung là
  *"the public API and the first end-to-end example"*.
- **`fixbolt_session::Application`** (`crates/session/src/lib.rs:102`) là seam có sẵn:
  `on_message(&mut self, msg: &[u8], seq: u32, stamp: &[u8], out: &mut [u8]) -> Option<Range<usize>>`.
  `fixbolt_engine::serve` nhận `A: Application`. Nên tầng mới **không cần engine sửa một dòng
  nào** — nó chỉ cần một kiểu cài `Application`.
- **`session` gọi ứng dụng ở `crates/session/src/lib.rs:2028`**, sau khi đã tự parse bản tin
  vào `self.idx`. Chỉ số ấy **không** được đưa ra ngoài, nên tầng mới phải parse lại. Đây là
  chi phí, không phải giả định — xem ADR dưới.
- **`fixbolt_codec::TemplateBuilder<P, S>`** sắp thứ tự trường **lúc build, không phải lúc
  gửi** (`crates/codec/src/template.rs`, dòng chú thích *"Ordering happens at build time"*),
  và `encode_with::<Fix44>` viết `8`, `9`, `10` cho mình. Đây chính là bất biến 5, đã có sẵn.
- **`TemplateBuilder::new` khởi tạo `scratch: [0; S]`** (`template.rs:171`). Nên mỗi lần dựng
  một template là một lần zero `S` byte trên stack. Không phải cấp phát heap, nhưng không
  miễn phí.
- **`echo.rs` đã chứng minh mẫu này chạy được**: nó dựng lại bản tin qua `TemplateBuilder`,
  ghi `34`/`49`/`56`/`52` từ session, và `15_HeaderAndBodyFieldsOrderedDifferently.def` —
  file gửi cùng một `NewOrderSingle` hai lần với thứ tự trường khác nhau và đòi **cùng một
  dãy byte** trả về — là file chứng minh việc sắp thứ tự thật sự xảy ra.
- **`tools/jrnl/Cargo.toml` là mẫu crate không có `[features]`**, và comment ở đó nói rõ vì
  sao: *"nothing here branches on one, and declaring one would be a lie the compiler cannot
  catch"*. Nó cũng lấy `engine` với `default-features = false`, để
  `scripts/check-no-optional-deps.sh` không bị crate anh em bật lại cờ —
  `docs/reference/feature-flags-unify-across-a-workspace.md`.
- **CI đã có job chạy được cho crate mới không cần sửa gì**: job `fmt · clippy · test` chạy
  `cargo test --all`, job `no-default-features` chạy `cargo test --all --no-default-features`
  **và** `scripts/check-no-optional-deps.sh`, job `bench` chạy `scripts/bench.sh`. Cả ba đọc
  `cargo metadata` để đếm crate, nên thêm một crate là thêm số, không phải thêm job.
- `[đo 2026-09-02, trên container này]` nền xanh trước khi bắt đầu: `cargo test --all` đọc
  **417 passed, 0 failed, 75 binaries**. Đây là con số phải so lại ở mọi bước.
- **Máy làm việc phiên này KHÔNG đạt `DESIGN.md` §9**: một VM 4 vCPU dùng chung,
  `Linux 6.18.44`, không `isolcpus`, không `nohz_full`, không khoá tần số. Mọi con số nano-giây
  từ đây đều mang nhãn ấy.

## Cách làm

Một crate mới: thư mục **`crates/library`**, tên package **`fixbolt`** — tên thư mục theo
`DESIGN.md` §3, tên package là cái người dùng gõ vào `Cargo.toml` của họ.

### 1. Một chỗ để khai báo, thay vì năm

`crates/library/src/lib.rs` xuất lại đúng những gì một ứng dụng cần, **và không xuất lại thứ
gì khác** — một mặt tiền xuất lại mọi thứ thì không phải mặt tiền:

| Từ đâu | Xuất lại |
|---|---|
| `fixbolt_engine` | `serve`, `serve_hft`, `serve_with_recovery`, `serve_hft_with_recovery`, `ServeError`, `Shutdown` |
| `fixbolt_engine::presession` | `Table`, `Limits`, `Entry`, `Identity`, `Registry` |
| `fixbolt_engine::settings` | `Settings`, `SettingsError`, `Problem` |
| `fixbolt_engine::observe` | `Observer`, `Admin`, `Snapshot`, `SessionSnapshot`, `Event`, `EventKind`, `Command` |
| `fixbolt_engine::recovery` | `Recovery`, `Resumed`, `NoRecovery`, `FromFn` |
| `fixbolt_engine::journal` | `FileJournal`, `Store`, `Reader`, `Record` |
| `fixbolt_session` | `Config`, `Role`, `Link`, `DropReason`, `Schedule`, `Weekday`, `Weekdays`, `MAX_BEGIN_STRING_LEN`, `MAX_COMP_ID_LEN` |
| `fixbolt_codec` | `MessageView`, `GroupIter`, `as_u32`, `as_i64`, `as_char` |

Cố ý **không** xuất lại: `Engine`, `Dispatch`, `Transport`, `wait`, `shard`, `affinity`,
`frame`, `ring`. Ai cần chúng thì khai `fixbolt-engine` trực tiếp — và việc phải khai thêm
một dòng chính là chỗ để người ấy dừng lại một nhịp.

### 2. `Handler` — API của ứng dụng

`crates/library/src/app.rs`:

```rust
pub trait Handler<const N: usize = 256> {
    /// Một bản tin ứng dụng. Chỉ nêu các trường thân bài; phần đầu là của session.
    fn on_message(&mut self, msg: &Incoming<'_, N>, reply: Reply<'_>) -> Answer;
}
```

- **`Incoming<'a, N>`** — bản tin **đã parse sẵn**: `msg_type()`, `get(tag)`, `seq()`,
  `sender()`, `target()`, `groups(counter)`, `view()`. Mượn vào bộ đệm đọc của engine, `Copy`,
  không sở hữu gì.
- **`Reply<'a>`** — người viết ứng dụng gọi `.message(b"8").field(37, id).field(150, b"0")`
  rồi `.send()`. Thư viện tự ghi `8`, `9`, `34`, `49`, `56`, `52`, `10`; **`49`/`56` lấy từ
  bản tin đến và đảo chiều**, `34` và `52` lấy từ session. Thứ tự trường do bảng sinh ra
  quyết định, vì `Reply` đi qua `TemplateBuilder::build::<Fix44>()` chứ không tự xếp.
- **`Answer`** — `Answer::sent(range)` hoặc `Answer::silent()`. Kiểu riêng chứ không phải
  `Option<Range<usize>>` trần, để một hàm trả `None` vì quên còn khác một hàm nói "im lặng".

`Reply::send()` trả `Result<Answer, ReplyError>`; `ReplyError` bọc `EncodeError`. Một ứng
dụng **không** phải nhìn thấy `Range<usize>` nào cả.

### 3. `App<H>` — cái nối `Handler` vào `Application`

```rust
pub struct App<H, const N: usize = 256, const P: usize = 128, const S: usize = 4096> { inner: H }
pub fn app<H: Handler>(h: H) -> App<H>;
```

`impl Application for App<H, N, P, S>`: parse `msg` vào `FieldIndex<N>` với
`Validation::NONE` (khung đã được kiểm khi bản tin được nhận — đúng lý do `echo.rs` nêu),
dựng `Incoming`, dựng `Reply`, gọi `handler.on_message`, trả `Option<Range<usize>>` cho
session. Một bản tin không parse được thì **im lặng** và tăng một bộ đếm đọc được
(`App::unparsable()`) — im lặng không đếm được là im lặng không giải thích được, đúng bài học
`docs/reference/silence-before-a-logon-has-many-causes.md`.

### 4. Ví dụ end-to-end đầu tiên

- `crates/library/examples/acceptor.rs` — một `main` thật: đọc `acceptor.cfg` bằng
  `Settings::load`, gọi `serve`, phục vụ tới khi bị dừng.
- `crates/library/examples/acceptor.cfg` — file cấu hình đi kèm.
- `crates/library/examples/shared/order_handler.rs` — **cái `Handler` thật**, để riêng một
  file trong thư mục con (thư mục con không có `main.rs` **không** là một target của cargo,
  nên nó không thành ví dụ thứ hai).

`crates/library/tests/end_to_end.rs` nạp **chính file ấy** bằng
`#[path = "../examples/shared/order_handler.rs"]`, rồi lái nó qua một socket thật:
logon → `NewOrderSingle` → `ExecutionReport` → logout. **Một bản, hai người đọc** — hai bản
là hai oracle rồi sẽ bất đồng, và đó là lý do `echo.rs` được gom về một chỗ ngày 2026-08-31.

Cái duy nhất ví dụ có mà test không chạy tới là `main` đọc tham số dòng lệnh. Ghi rõ ra, không
giấu.

### 5. ADR

**ADR-0041 — tầng library đổi một lần parse và một lần dựng template lấy một API.** Quyết
định đắt vì nó nằm trên hot path của con đường mà **đa số người dùng sẽ đi**, và khó đảo vì
nó là API công khai. Nội dung: nêu cái giá, nêu vì sao không sửa `Application` để đưa
`MessageView` xuống (đó là thay đổi tầng session, ngoài phạm vi, và phải chạy lại 59 định
nghĩa), và nêu lối thoát — ai cần từng nano-giây thì cài `Application` trực tiếp, đường ấy
không bị lấy đi.

### 6. Con số `tools/w2w` trên máy này

Chủ dự án chọn: chạy ở đây, **dán nhãn không phải §9**. Chạy `tools/w2w` cả hai chế độ, chép
nguyên văn output (chính binary ấy tự in ra câu cảnh báo), ghi vào `STATUS.md` phần
**Not proven** kèm tên máy — **và open item 6 không đóng**. Tiêu chí thoát số 6 của phase 1
vẫn là chưa đạt.

## Bất biến bị đụng tới

Crate mới nằm ở L4, trên `engine`, nhưng nó **có mặt trên hot path** vì `App::on_message`
chạy trên thread engine.

| # | Đụng thế nào | Giữ bằng gì |
|---|---|---|
| **1** — không cấp phát trên hot path | `App::on_message` chạy trên thread engine mỗi bản tin ứng dụng | `crates/library/benches/alloc.rs`, case `handler-reply` phải đọc **0**, kèm một control tiêm `to_vec()` phải đọc khác 0 trong **cùng cửa sổ đếm** (quy tắc 3 của ngày 2026-09-02) |
| **2** — session thuần | Không sửa `session` một dòng nào | `git diff --stat crates/session` phải rỗng ở PR này |
| **3** — 59 định nghĩa là cổng của session | Không sửa session, nhưng vẫn chạy lại | `cargo test -p fixbolt-session --test score` và `-p fixbolt-engine --test wire` phải vẫn **59/59** |
| **4** — hai chế độ | Không đụng chiến lược chờ | Không đổi; hai script check chạy như cũ trong CI |
| **5** — thứ tự trường từ bảng sinh | **Đây là cái `Reply` tồn tại để bảo vệ** | `Reply` đi qua `TemplateBuilder::build::<Fix44>()`; test khẳng định **đúng dãy byte** khi handler nêu trường sai thứ tự |
| **6** — cờ tính năng gác chính `mod` | Crate mới **không khai `[features]`** và lấy `engine` với `default-features = false`, như `tools/jrnl` | `cargo test --all --no-default-features` **và** `scripts/check-no-optional-deps.sh` |
| **7** — không `panic`/`unwrap`/`expect` trong crate thư viện | Crate mới **là** crate thư viện | `[lints] workspace = true` trong manifest, `cargo clippy --all-targets -- -D warnings` |
| **8** — `unsafe` cần kế hoạch | Không có `unsafe` nào, trừ allocator đếm trong bench (giống ba bench kia) | `unsafe_code = "warn"` ở workspace |
| **9** — không chép nguồn QuickFIX | Không đụng tới | — |
| **10** — không có số nào thiếu benchmark/máy/§9 | Con số `w2w` phiên này | Ghi kèm tên máy và câu "không đạt §9", vào **Not proven** |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | Khung crate `crates/library` vào workspace + **một test ĐỎ ở assertion**: `Reply` phải ghi phần đầu mà handler không nêu | — |
| 2 | `Reply` — phần đầu từ session, thân bài sắp theo từ điển. Bước 1 xanh. Đảo ngược: bỏ `52` → phải đỏ | 1 |
| 3 | `Incoming`, `Handler`, `Answer`, `App<H>` + `fixbolt::app()`; xuất lại mặt tiền §1 | 2 |
| 4 | `examples/shared/order_handler.rs`, `examples/acceptor.rs`, `acceptor.cfg`, và `tests/end_to_end.rs` lái chính handler ấy qua socket thật | 3 |
| 5 | `crates/library/benches/alloc.rs` — `handler-reply` = **0**, có control tiêm | 3 |
| 6 | ADR-0041 + đồng bộ tài liệu §4 + chạy `tools/w2w` ghi vào **Not proven** | 1–5 |

## Cách kiểm chứng

| Bước | Lệnh | Đạt khi |
|---|---|---|
| 1 | `cargo test -p fixbolt --test reply` | **ĐỎ ở một assertion**, không phải ở trình biên dịch — thông báo là "byte khác nhau", có in ra hai dãy |
| 2 | như trên | Xanh. Rồi **đảo ngược**: xoá dòng ghi `52` trong `Reply` → phải đỏ ở đúng assertion ấy |
| 3 | `cargo test -p fixbolt` | Tất cả xanh; `App` trả `None` cho bản tin không parse được và `unparsable()` đọc 1 |
| 4 | `cargo test -p fixbolt --test end_to_end` | Client thật nhận đúng một `ExecutionReport` với `34`, `49`, `56`, `52` do thư viện ghi. **Đảo ngược**: cho handler nêu `49` của chính nó → thư viện phải bỏ qua, byte không đổi |
| 5 | `cargo bench -p fixbolt --bench alloc` | `handler-reply 0`; control tiêm đọc **khác 0** trong cùng lần chạy |
| mọi bước | `cargo test -p fixbolt-session --test score` | vẫn **59 / 59** |
| mọi bước | `cargo test -p fixbolt-engine --test wire` | vẫn **59 / 59** |
| mọi bước | `cargo test --all` | ≥ **417 passed, 0 failed** (nền 2026-09-02) |
| mọi bước | `cargo test --all --no-default-features`, `scripts/check-no-optional-deps.sh` | rc = 0 |
| mọi bước | `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings` | rc = 0 |
| đóng plan | Một lần chạy CI **xanh, gọi tên bằng id, trên đúng commit đóng** | `CLAUDE.md` §9 ô cuối |

**Chạy chứ không suy ra.** Mọi dòng "đạt" ở trên là output đọc bằng mắt, không phải exit code.

## Tài liệu phải cập nhật

Đi từng dòng bảng đồng bộ `CLAUDE.md` §4:

- [ ] `docs/PRD.md` §2 — ô `library` trong cây phase 1 chuyển khỏi `not started`
- [ ] `docs/DESIGN.md` §3 — hàng `library` được điền thật; §7 bước 8 ghi đã dựng
- [ ] `README.md` — mục Layout thêm `crates/library`, và **xoá câu "The application-facing
      `library` does not exist"**
- [ ] `docs/GUIDE.md` — mục mới: viết một ứng dụng bằng `Handler`, và **cái giá của nó**
- [ ] `Cargo.toml` — thêm member
- [ ] `CHANGELOG.md` — API công khai của crate mới
- [ ] `docs/decisions/ADR-0041-…` — quyết định về cái giá
- [ ] `STATUS.md` — plan đóng, con số `w2w` vào **Not proven** kèm tên máy
- [ ] `docs/reference/` — nếu có bẫy nào phải trả giá mới thì viết vào đây **trước tiên**

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| `Reply` ghi `49`/`56` **không đảo chiều** — bản tin gửi về chính mình | `tests/reply.rs`: khẳng định đúng dãy byte, `49` là target của bản tin đến |
| `52` do handler tự sinh thay vì lấy của session → lệch `9=` bốn byte (đúng bẫy `9=101` của `echo.rs`) | `tests/reply.rs` khẳng định đủ dãy byte, kể cả `9=` và `10=` |
| Handler nêu một trong `8`, `9`, `10`, `34`, `49`, `52`, `56` → phải **bị bỏ qua**, không phải ghi hai lần | `tests/reply.rs`, một case riêng cho mỗi tag |
| `Reply` cấp phát (một `Vec` trong đường lỗi, một `format!` trong `ReplyError`) | `benches/alloc.rs` case `handler-reply` = 0 **kèm control**; `ReplyError` là enum không trường |
| Ví dụ và test dùng **hai bản** handler rồi lệch nhau | Một file, nạp bằng `#[path]` từ cả hai phía |
| Crate mới bật lại `libc` cho cả workspace qua hợp nhất cờ tính năng | `scripts/check-no-optional-deps.sh`, hỏi **từng crate một** |
| Thêm `[features]` vào manifest mà không có `#[cfg]` nào — lời nói dối trình biên dịch không bắt được | Crate **không khai `[features]`** |
| Test end-to-end "xanh" vì socket im lặng chứ vì đúng | Test khẳng định **nội dung bản tin nhận được**, và có một case đối chứng handler trả `silent()` — phải **không** có bản tin nào về |
| Số `w2w` từ máy này bị trích dẫn sau đó như số §9 | Ghi vào **Not proven**, kèm tên máy, kèm câu binary tự in |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| `const N: usize = 256` mặc định trên trait không biên dịch được trên `1.98.0` | Trung bình | Thử ngay ở bước 1. Không được thì bỏ mặc định, `Handler<256>` viết tay — không đổi thiết kế |
| `TemplateBuilder::new` zero `S = 4096` byte mỗi bản tin, ăn vào ngân sách §8 | Cao | **Đo, đừng đoán.** Bước 5 đo cấp phát; chi phí memset ghi vào ADR-0041 và GUIDE như một hệ quả có tên, kèm lối thoát là cài `Application` trực tiếp |
| Parse hai lần (session một, library một) | Cao | Chính là ADR-0041. Không giấu: GUIDE nói thẳng, và đường thô vẫn còn |
| Mặt tiền xuất lại thiếu một kiểu người dùng cần, phát hiện sau khi API đã công khai | Trung bình | Ví dụ end-to-end là bài kiểm tra: nếu nó phải `use fixbolt_engine::…` một lần nào thì mặt tiền thiếu, và đó là một test đọc được bằng mắt |
| Crate mới làm `cargo test --all` chậm thêm | Thấp | Chấp nhận; ghi số trước/sau |

## Ngoài phạm vi

- **Sửa `fixbolt_session::Application` để đưa `MessageView` xuống thẳng.** Đó là thay đổi tầng
  session, cần plan riêng, và ADR-0041 sẽ ghi nó là lối đi đã cân nhắc.
- **Tiêu chí thoát số 4 của phase 1** — interop initiator với `libquickfix` trong CI. Chủ dự
  án đã chọn để ngoài phiên này.
- **Tiêu chí thoát số 6** — con số §9. Chạy `w2w` ở đây **không** đóng nó.
- **`RingDispatch` qua mặt tiền mới.** `serve` dùng `InlineDispatch`; ai cần ring thì dựng
  `Engine` tay, như hôm nay.
- **Publish lên crates.io.** Không có `publish = true` nào ở PR này.
- **TLS**, **reload cấu hình khi đang chạy**, **credentials** — đều đã có lý do riêng ở nơi khác.

## Nhật ký giao hàng

*(điền khi đóng từng bước)*
