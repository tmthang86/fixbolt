# Phần dư không cần bàn đo: bốn việc còn mở mà máy §9 không liên quan

> **Loại:** Plan · **Ngày:** 2026-09-20 · **Trạng thái:** Chờ duyệt
> **Phạm vi:** đóng mọi việc còn mở trong `STATUS.md` **không** cần máy đo `DESIGN.md` §9 —
> item 94, item 32 (a), hai gạch đầu dòng *Not proven* của ngày 2026-09-20, và hai hàng
> (36, 89) đã đóng mà chưa gạch.

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Bàn đo §9 (Ryzen 3700X) đang được một kế hoạch khác chiếm — *boot D*,
`docs/plans/2026-09-20-boot-d.md`, viết song song với kế hoạch này. Mọi việc **cần** bàn đo
(dải số cho `validate` sau khi thêm đếm group lồng, A/B `listener_every`, số latency FIXT) đi
theo boot D. Kế hoạch này gom **phần còn lại**: những việc chỉ cần một máy Linux bất kỳ, một
trình biên dịch, và đọc output.

Bốn việc, bốn lý do khác nhau vì sao chúng còn mở:

1. **Item 94 — thông điệp trong bench sai checksum của chính nó, lần thứ tư.** Một chuỗi
   `NewOrderSingle` viết tay mang `10=098` trong khi checksum thật là 097, dán vào bốn file
   bench. Sửa ngày 2026-09-05 chạm một file; ngày 2026-09-19 tìm lại thấy ở file thứ hai và
   *ghi nhận* thay vì sửa, vì fixture đó nuôi gate của bất biến 1 và chủ dự án đang đi vắng.
   Hai trang `docs/reference/` đã nói "assert đầu vào trước khi đo" — đúng, và vẫn tái diễn,
   vì kỷ luật chỉ với tới file người ta đang mở. Lần này phải là **một gate**, không phải một
   lời nhắc.
2. **Item 32 (a) — triển khai chia shard chưa khôi phục được phiên.** `serve_with_recovery` và
   `serve_hft_with_recovery` hỏi `Recovery` rồi nối tiếp số thứ tự; `serve_sharded_hft` thì
   không có cửa đó. ADR-0034 quyết định 3 ghi lý do hoãn: `shard.rs` chỉ chạy trên Linux và
   tác giả lúc đó ngồi Mac. Máy này là Linux.
3. **Not proven (a) — test socket FIXT đỏ khi máy bận.** `[measured 2026-09-20]` 0/40 chạy tuần
   tự, **11/50** khi chạy mười bản đồng thời, **8/50** cùng cách trên `main` không mang thay đổi
   nào của branch. Engine phát thêm một message không ai yêu cầu, và mọi `34=` sau đó lệch một.
   Đây là việc khó nhất và phần *Những gì đã biết chắc* dưới đây nói cơ chế thật sự nằm ở đâu.
4. **Not proven (b) — `die()` trong `build.rs` khi `<message>` thiếu `msgcat` chưa bao giờ
   được nhìn thấy đỏ.** Máy xây hôm đó có `vendor/` chỉ đọc. `CLAUDE.md` §10 đòi một lần đảo
   ngược có quan sát.

Cộng thêm hai hàng `STATUS.md` (36, 89) đã đóng bằng code và ADR nhưng chưa gạch — việc của
tài liệu, đi cùng bước tài liệu.

## Những gì đã biết chắc

**Item 94**

- `grep -rn '10=09[78]' crates/` ngày 2026-09-20: `codec/benches/parse.rs:25` mang `10=097`
  (đã sửa 09-05); `codec/benches/alloc.rs:91`, `engine/benches/dispatch.rs:77`,
  `engine/benches/ring_full.rs:73` mang `10=098`. Chuỗi `167=BOO\x0110=` xuất hiện **đúng bốn
  file này** và không ở đâu khác trong `crates/*/benches/`.
- `codec/benches/alloc.rs:114-124` hiện assert `warm_parse == Err(ParseError::BadCheckSum)` —
  nghĩa là gate của bất biến 1 đang assert *một lời từ chối* làm bằng chứng đường dẫn sống.
  `docs/reference/a-bench-message-that-fails-its-own-checksum.md` ghi vì sao.
- `crates/codec/Cargo.toml:14` đặt `autobenches = false`, và `benches/harness.rs` là một module
  không phải target, được ba bench trong `codec` include bằng `#[path = "harness.rs"]` và các
  crate khác include bằng `#[path = "../../codec/benches/harness.rs"]` (`harness.rs:85-86`).
  Tiền lệ cho một file fixture dùng chung nằm sẵn ở đó. `crates/engine/Cargo.toml` **không**
  tắt autobenches — một file `.rs` mới trong `engine/benches/` sẽ thành target.
- `scripts/bench.sh` coi `alloc` và `ring_full` là INVARIANT (panic → exit ≠ 0), `dispatch`
  là TIMING (kết quả chỉ báo). `cargo test` không xây bench `harness = false`, nên một assert
  trong bench chỉ chạy ở job `bench` của CI.

**Not proven (a) — cơ chế thật**

- Runner của corpus (`crates/conformance/src/runner.rs:441-447`): khi tới một dòng `E` mà
  `pending` (những gì session đã phát và chưa dòng `E` nào nhận) **rỗng**, runner đẩy đồng hồ
  lên **một `HeartBtInt` trọn** (`heart_bt_ms`, 30 000 ms cho `108=30`) rồi feed `Input::Tick`,
  tối đa `WAITS = 3` lần. Với session thuần (`crates/session`), "pending rỗng" đúng nghĩa là
  "session không có gì để nói" — session trả lời đồng bộ.
- Harness socket (`crates/engine/tests/wire_fixt.rs:160-185`, `wire.rs:142` cùng hình):
  `Wire::pump` dừng khi **không có gì chuyển động trong 1 ms wall-clock** (`STEP_QUIET`) hoặc
  hết `STEP_DEADLINE = 50 ms`. Cả hai là thời gian thật. Đồng hồ engine là `ManualClock`, chỉ
  đổi khi `Input::Tick` tới (`wire_fixt.rs:263-265`).
- Ghép hai điều trên: khi máy bận, tiến trình bị tước CPU, byte trên loopback chưa được engine
  đọc khi `pump` bỏ cuộc, `pending` rỗng **vì câu trả lời tới muộn**, runner tick 30 s, engine
  thấy 30 s im lặng và phát heartbeat — đúng luật. Output lỗi trong
  `the-same-commit-went-red-and-green-in-the-same-minute.md` khớp: message thừa có **8 field**
  (`8,9,35,34,49,52,56,10` — đúng hình một `35=0`), rồi `35=5|34=5` là Logout hợp lệ bị đẩy
  lệch. **Điều này là suy luận từ code và output, chưa quan sát trực tiếp `35=` của message
  thừa** — bước 4 phải in ra trước khi sửa (xem *Cách kiểm chứng*).
- Hook đếm có sẵn: `MessageLog::record(Direction::In, …)` gọi tại `crates/engine/src/conn.rs:448`
  cho **mỗi frame đọc khỏi socket, trước khi session phán xét — kể cả rác** (rustdoc của
  `Direction::In`, `msglog.rs:95`); `Direction::Out` tại `conn.rs:840` khi message vào hàng đợi
  gửi. `Engine::with_log` (`lib.rs:309`) nhận một `L2: MessageLog` bất kỳ; `record` không được
  cấp phát hay chặn (`msglog.rs:146`). Cả hai Wire hiện dùng `NoLog`.
- Corpus `fix50sp2` có 290 dòng `I`; một nhúm (cỡ 8) cố ý mang `9=` sai hoặc tag rác
  (`3c_GarbledMessage`-kiểu) mà framer của chính harness (`next_message`) không đóng khung
  được.
- **Runner của QuickFIX và QuickFIX/J không tha thứ message ngoài kịch bản** — kết quả tìm
  trên internet 2026-09-20, trích trong
  [ADR-0087](../decisions/ADR-0087-a-socket-harness-settles-on-counted-records-and-the-clock-waits-for-the-engine.md)
  *What the search found*: QuickFIX C++ `ReflectorClient.rb` đọc message kế tiếp khỏi socket
  (blocking) và so ngay, không lọc loại nào; QuickFIX/J `ExpectMessageStep` `readMessage(…,
  TIMEOUT_IN_MS = 10000)` từ một queue nhận **mọi** message, chỉ có `heartBeatOverride` bỏ so
  *giá trị* field 108; quickfixgo `_test/` là cùng bộ Ruby. Họ **tránh** kích timer bằng thời
  gian thật và chờ tới 10 s — không ai mô phỏng đồng hồ. Ta mô phỏng để gate in-process xác
  định; harness socket thừa kế mô phỏng mà không thừa kế tiền điều kiện "engine đang rỗi".

**Item 32 (a)**

- `crates/engine/src/shard.rs:449-560`: `serve_sharded_hft_with` chạy pre-session trên thread
  gọi, `set.settled()` → `shards.hand(p)` gửi `Pending<TcpTransport, PRE>` qua `mpsc`; thread
  shard `try_recv` rồi `engine.add(t, cfg, prefix)` (`shard.rs:270-283`).
- `Shardable` (`shard.rs:60-100`) có `add`/`turn`/`idle`; impl blanket đòi `J: SessionJournal
  + Default` và gọi `Engine::add_with_prefix_and_config` — tức luôn `J::default()`. Đây đúng là
  ràng buộc ADR-0039 đã gỡ khỏi `pump`.
- `pump` (`lib.rs:3182-3260`) là mẫu: `recovery.recover(&cfg)` trên thread acceptor, rồi
  `engine.add_with_prefix_config_and_journal(t, cfg, prefix, state, || recovery.fresh(&cfg))`
  (`lib.rs:608`). `Recovery<J>` (`recovery.rs:125-156`) có `recover` và `fresh`, cả hai được
  phép chặn vì chạy trên thread acceptor (ADR-0020).
- `Shards<PRE>` được dùng ở `tests/shard.rs` (7 chỗ), `tests/shard_wire.rs` (2 chỗ), luôn dạng
  `Shards::<PRE>::start`. `serve_sharded_hft` được dùng ở `tests/shard_hft.rs` và nhắc trong
  `docs/GUIDE.md:52-53,156,159,202,857,1488,1578`, `docs/PRD.md:198`, `docs/CONFORMANCE.md:408`.
- `tests/shard_hft.rs::serve_sharded_hft_serves_a_session` assert `|34=1|` cho phiên không ai
  khôi phục — đây là bằng chứng "đường cũ không đổi" nếu nó **xanh mà không sửa**.
- Bẫy đã trả tiền: `tests/shard_hft.rs:16-35` — file test phải `#![cfg]` cả
  `feature = "standard"`, vì `serve_sharded_hft` nằm sau `#[cfg(feature = "standard")]`
  (`shard.rs:439`), và `cargo test -p fixbolt-engine --no-default-features --features affinity`
  từng đỏ vì thiếu điều đó.

**Not proven (b)**

- `crates/dict/build.rs:106-112` đọc đường dẫn XML từ biến môi trường **`NANOFIX_FIX44_XML`**
  (mặc định `../../vendor/quickfix/spec/FIX44.xml`), và `build.rs:73` khai
  `cargo:rerun-if-env-changed`. Không cần ghi vào `vendor/`.
- Ba nhánh `die` liên quan trong `emit` (`build.rs:792-808`) và sau đó (`build.rs:1208`), với
  câu chữ đúng: `has msgcat="…"; the only categories`, `has no msgcat attribute.`, và
  `not one <message> carries msgcat='admin'.`
- `FIX44.xml` có 93 dòng `<message …>`, thuộc tính viết dấu nháy đơn:
  `<message name='Heartbeat' msgtype='0' msgcat='admin'>` (dòng 36).
- `scripts/check-scratch-fixtures.sh` phạt script nào `cd` vào hay trỏ `cargo` vào một thư mục
  ngoài cây; đặt file XML tạm dưới `target/` thì không chạm luật đó. `/tmp` trên bàn là tmpfs
  (ghi nhớ của dự án) — thêm một lý do dùng `target/`.

**Hàng 36 và 89**

- Hàng 36 đòi "phân loại từng file" cho corpus mirror; `ADR-0076` (**Accepted 2026-09-18**) và
  `crates/conformance/src/mirror.rs` là chính bảng phân loại đó. Đã đóng.
- Hàng 89: nhịp nghe listener đã xây (`Limits::listener_every`, `presession.rs:426-526`);
  `ADR-0069` **Accepted 2026-09-18**, `DEFAULT_LISTENER_EVERY = 16` (`presession.rs:442`). Cái
  còn mở trong hàng là **một phép đo**, thuộc boot D — không thuộc kế hoạch này.

## Cách làm

Ba ADR viết sẵn ở bước 0, mỗi cái một quyết định; đây chỉ tóm phương án được chọn.

**Item 94 —
[ADR-0089](../decisions/ADR-0089-a-shared-bench-fixture-has-one-source-included-by-path-and-a-test-that-parses-it.md).**
Tạo `crates/codec/benches/fixture.rs`: `pub const NEW_ORDER_SINGLE: &[u8]` với `10=097` và
`pub fn assert_valid()` (parse `Validation::ALL`, đòi `Ok(Parsed::Complete { consumed })` với
`consumed == len`). Bốn bench bỏ literal, include bằng `#[path]`, gọi `assert_valid()` trước
khi đo — cả `dispatch.rs` và `ring_full.rs` dù không parse. `alloc.rs` đổi assert liveness sang
`Ok(Parsed::Complete { .. })`, giữ assert `get(55) == Some(b"INTC")`. Tạo
`crates/codec/tests/bench_fixture.rs` với hai test: một parse fixture, một duyệt
`crates/*/benches/*.rs` và đỏ nếu `167=BOO\x0110=` xuất hiện ngoài `fixture.rs`.

**Not proven (a) —
[ADR-0087](../decisions/ADR-0087-a-socket-harness-settles-on-counted-records-and-the-clock-waits-for-the-engine.md).**
Không sửa runner, không sửa comparator, không sửa engine. Trong `tests/wire_fixt.rs` và
`tests/wire.rs`: một `CountingLog: MessageLog` đếm `In`/`Out` bằng `Arc<AtomicUsize>`; Wire
cài bằng `with_log`. Một bước *settled* khi (1) số record `In` ≥ số dòng `I` đóng khung được
đã ghi trên connection đó, và (2) số message harness đã đọc ≥ số record `Out`; sau đó mới
tới 1 ms quiet. `Input::Tick` chờ (1) rồi mới đặt `ManualClock`. `STEP_DEADLINE` lên 5 s và
khi chạm thì `eprintln!` file, dòng, fact chưa đạt — dây cứu sinh, không phải settle. Tạo
`scripts/check-socket-corpus-under-contention.sh <rounds> <copies>` chạy binary test đã xây
`rounds × copies` lần, `copies` bản cùng lúc, in `N red in M`, exit ≠ 0 nếu N > 0; thêm một
step vào job `gates` của `ci.yml`.

**Item 32 (a) —
[ADR-0088](../decisions/ADR-0088-recovery-reaches-the-sharded-runtime-and-the-journal-crosses-the-channel-with-the-connection.md).**
`recovery.rs` thêm `pub enum Start<J> { Fresh(J), Resumed(Resumed<J>) }`. Thread acceptor gọi
`recover` rồi `fresh` (đều được chặn), gửi `(Pending, Start<J>)` qua channel. `Shardable` thêm
`add_started(transport, cfg, prefix, start)`; `add` cũ giữ, cài bằng
`add_started(…, Start::Fresh(J::default()))`, ràng buộc `J: Default` dời từ header impl xuống
riêng `add`. `Shards<PRE>` → `Shards<PRE, J = Store>`. Hai cửa mới
`serve_sharded_hft_with_recovery` / `_with_recovery_with<N, RX, TX, APP, …>`; hai cửa cũ gọi
qua `NoRecovery` — **một vòng lặp**. Test mới `tests/shard_recovery.rs`. Shutdown có trật tự
cho sharded **không** làm — nói rõ trong hàng 32.

**Not proven (b).** Script `scripts/check-dict-refuses-a-message-without-msgcat.sh`: sao
`vendor/quickfix/spec/FIX44.xml` vào `target/check-msgcat/`, ba biến thể bằng `sed`
(xoá ` msgcat='admin'` ở dòng `Heartbeat`; đổi thành `msgcat='other'`; đổi mọi `admin` thành
`app`), mỗi biến thể chạy `NANOFIX_FIX44_XML=<file> cargo build -p fixbolt-dict` và đòi
exit ≠ 0 **và** stderr chứa câu `die` tương ứng; nhánh 0 với file gốc phải build xanh (chứng
minh harness phân biệt được). Header nói script không thấy gì (nhánh FIXT dùng cùng một
`match`, không chạy riêng). Thêm một step vào job `gates`.

**Hàng 36, 89, 94, 32 và hai gạch đầu dòng *Not proven*** — bước tài liệu, câu chữ ở
*Cách kiểm chứng*.

## Bất biến bị đụng tới

| Bất biến | Đụng thế nào | Giữ bằng gì |
|---|---|---|
| 1 — không cấp phát trên hot path | `codec/benches/alloc.rs` là gate; fixture đổi một byte và assert liveness đổi chiều. `Start<J>` đi qua cùng channel `Pending` đã đi (kết nối tới, không phải mỗi message) | `scripts/bench.sh`: mọi case `alloc` đọc **0**; `engine --bench alloc` không đổi |
| 3 — 59 định nghĩa là gate của session | Hai harness socket đổi cách settle; runner và comparator **không** đổi | `cargo test -p fixbolt-engine --test wire` 59/59 cả hai mode; `--features fix50sp2 --test wire_fixt` 60/60; `cargo test -p fixbolt-session --test score` 59/59 không sửa |
| 4 — thread engine `hft` không ngủ | `recover`/`fresh` chỉ chạy trên thread acceptor; thread shard vẫn `try_recv` | Đọc code (`shard.rs` thread shard không thêm lời gọi nào chặn); `scripts/check-no-kernel-sleep.sh` vẫn trace `w2w`, không trace shard — khoảng trống **như hiện nay**, ghi lại |
| 6 — feature gate trên `mod`/item | Cửa mới mang `#[cfg(feature = "standard")]` như cửa cũ; file test mới mang `#![cfg]` đủ ba điều kiện | `cargo test -p fixbolt-engine --no-default-features --features affinity`; `cargo clippy --all-targets --features affinity -- -D warnings` |
| 7 — không `unwrap` trong lib | `shard.rs`, `recovery.rs` sửa | `cargo clippy --all-targets -- -D warnings`; `scripts/check-indexing-debt.sh` không tăng |
| 10 — không số đo thiếu ba thứ | Không con số nào công bố. Số "0 red in 50" là **số đếm đúng/sai**, không phụ thuộc OS tuning (chính `docs/CONFORMANCE.md` §9 nói vậy) | Ghi kèm lệnh và máy, gắn nhãn là số đếm |

Không đụng `codec/src`, `session/src`, `transport`.

## Chia việc

Mọi bước chạm `crates/engine/src` hoặc gate của §2 → **senior developer (opus)**. Không bước
nào commit; manager chạy lại gate và commit. **Hai pull request**, ranh giới sau bước 3.

| Bước | Ai | Kết quả | Được sửa | Không được sửa | §2 | Test viết trước, câu FAIL chờ đợi | Gate đóng bước | Phụ thuộc |
|---|---|---|---|---|---|---|---|---|
| **0** | architect (fable) | Kế hoạch này; ADR-0087, 0088, 0089 | `docs/plans/`, `docs/decisions/` | mọi thứ khác | — | — | `python3 scripts/check-links.py`; `scripts/check-adr-numbers.sh` | — |
| **1** | senior dev (opus) | Item 94 theo ADR-0089 | `crates/codec/benches/fixture.rs` (mới), `crates/codec/tests/bench_fixture.rs` (mới), `crates/codec/benches/alloc.rs`, `crates/codec/benches/parse.rs`, `crates/engine/benches/dispatch.rs`, `crates/engine/benches/ring_full.rs` | `crates/*/src/`, `Cargo.toml` nào, mọi bench khác | 1, 10 | `bench_fixture.rs::the_shared_bench_message_parses_clean_under_full_validation` với fixture **còn** `10=098` (chép y từ `alloc.rs`): FAIL `left: Err(BadCheckSum)`. `no_bench_carries_its_own_copy_of_the_shared_message` khi bốn bench còn literal: FAIL nêu tên bốn file | `cargo test -p fixbolt-codec --test bench_fixture`; `cargo bench -q -p fixbolt-codec --bench alloc` và `-p fixbolt-engine --bench ring_full` in `0` mọi case; `cargo bench -q -p fixbolt-engine --bench dispatch` chạy hết; `cargo clippy --all-targets -- -D warnings` | 0 |
| **2** | developer (sonnet) | Not proven (b) | `scripts/check-dict-refuses-a-message-without-msgcat.sh` (mới), `.github/workflows/ci.yml` (một step trong job `gates`, sau `cargo test --all`) | `crates/`, `vendor/`, script khác | 6 | Reversal của gate: tạm đổi `None => die(…)` (`build.rs:802`) thành coi như `app` → script FAIL nhánh 1: `expected the build to refuse: no msgcat`. Hoàn lại | Script in `ok` cho nhánh 0 và ba nhánh đỏ; `scripts/check-scratch-fixtures.sh` xanh; `cargo build -p fixbolt-dict` sau đó xanh | 0. **Song song với 1** (file rời) |
| **3** | developer (sonnet) | Tài liệu PR A | `STATUS.md` (hàng 36, 89, 94; gạch *Not proven* (b)), `docs/reference/a-bench-message-that-fails-its-own-checksum.md` (đoạn `[2026-09-2x]`), `docs/internals/codec.md`, `docs/internals/dict.md` | `crates/`, `CLAUDE.md`, `DESIGN.md` | — | — | `python3 scripts/check-links.py` sạch | 1, 2. **Ranh giới PR A / PR B** |
| **4** | senior dev (opus) | Not proven (a) theo ADR-0087 | `crates/engine/tests/wire_fixt.rs`, `crates/engine/tests/wire.rs`, `scripts/check-socket-corpus-under-contention.sh` (mới), `.github/workflows/ci.yml` (một step trong `gates`) | `crates/conformance/`, `crates/engine/src/`, `crates/session/`, `tests/shard_wire.rs` | 3, 4 | **Đỏ trước, trên cây chưa sửa**: `scripts/check-socket-corpus-under-contention.sh 5 10` cho `wire_fixt` → `N red in 50` với N ≥ 1, **và in `35=` của message thừa** (sửa tạm harness để in `unexpected output` đủ 8 field). Chờ `35=0`. Nếu không phải `35=0`: **dừng, báo manager, về architect** | `cargo test -p fixbolt-engine --features fix50sp2 --test wire_fixt` 60/60; `cargo test -p fixbolt-engine --test wire` 59/59 **cả hai** test; script `5 10` → `0 red in 50` cho `wire_fixt` và cho `wire`; chạy tuần tự một lần in `lifeline hit: 0` | 0. **Song song với 5** (file rời) |
| **5** | senior dev (opus) | Item 32 (a) theo ADR-0088 | `crates/engine/src/shard.rs`, `crates/engine/src/recovery.rs`, `crates/engine/src/lib.rs` (chỉ re-export nếu cần), `crates/engine/tests/shard_recovery.rs` (mới), `CHANGELOG.md`; `tests/shard.rs`/`tests/shard_wire.rs` **chỉ** nếu default type param không cứu được — nêu tên nếu phải | `crates/engine/tests/wire*.rs`, `crates/session/`, `.github/`, `scripts/` | 1, 4, 6, 7 | `shard_recovery.rs::a_sharded_acceptor_resumes_the_numbers_recovery_hands_it`: `Recovery` trả `Resumed { next_out: 5, next_in: 3, … }`; client gửi Logon `34=3`; đòi Logon trả lời `\|34=5\|`. FAIL chờ đợi: `error[E0425]: cannot find function serve_sharded_hft_with_recovery`. Thêm `fn assert_send<T: Send>()` cho `Start<FileJournal>` | `cargo test -p fixbolt-engine --features affinity --test shard_recovery --test shard_hft --test shard --test shard_wire` (`serve_sharded_hft_serves_a_session` xanh **không sửa**); `cargo test -p fixbolt-engine --no-default-features --features affinity`; `cargo clippy --all-targets --features affinity -- -D warnings`; `cargo test --all` | 0. **Song song với 4** |
| **6** | developer (sonnet) | Tài liệu PR B | `STATUS.md` (hàng 32, gạch *Not proven* (a)), `docs/GUIDE.md` (§1a đoạn shard; dòng 855-858; 1577-1578), `docs/CONFORMANCE.md` §9 *Through a real socket* và *What is not proven here*, `docs/reference/the-same-commit-went-red-and-green-in-the-same-minute.md` (đoạn nguyên nhân), `docs/internals/engine.md`, `docs/DESIGN.md` (nơi nhắc `serve_with_recovery` — grep — thêm cửa sharded), `CHANGELOG.md` nếu bước 5 chưa xong | `crates/` | — | — | `python3 scripts/check-links.py` sạch | 4, 5 |
| **7** | manager | Gate toàn bộ (*Cách kiểm chứng*), review senior một lần mỗi PR, commit từng bước xanh, PR nháp từ commit đầu, CI run id | — | — | — | — | tất cả | 3 (PR A), 6 (PR B) |

Chạy song song: **1 ‖ 2**, rồi 3. **4 ‖ 5**, rồi 6. Bước 4 và 5 cùng ở `crates/engine` nhưng
file rời; nếu chạy cùng một `target/`, cargo khoá tuần tự — chấp nhận, hoặc mỗi bên một
worktree.

## Cách kiểm chứng

**Bước 1.** Viết test trước với fixture chép nguyên `10=098`; chạy
`cargo test -p fixbolt-codec --test bench_fixture` — chờ:

```
the_shared_bench_message_parses_clean_under_full_validation ... FAILED
  left: Err(BadCheckSum)
no_bench_carries_its_own_copy_of_the_shared_message ... FAILED
  crates/codec/benches/alloc.rs, crates/codec/benches/parse.rs, crates/engine/benches/dispatch.rs, crates/engine/benches/ring_full.rs
```

Sửa `10=097`, đổi bốn bench sang include, chạy lại → hai test xanh. Rồi
`cargo bench -q -p fixbolt-codec --bench alloc` và `-p fixbolt-engine --bench ring_full`: mọi
case in `0`. Đảo ngược (§10): đặt lại `098` trong `fixture.rs` → test đầu đỏ **và**
`cargo bench -p fixbolt-codec --bench alloc` panic ở `assert_valid`; hoàn lại. Dán literal vào
một bench → test thứ hai đỏ nêu tên file; hoàn lại.

**Bước 2.** `scripts/check-dict-refuses-a-message-without-msgcat.sh` in bốn dòng:

```
ok  arm 0  the untouched FIX44.xml builds
ok  arm 1  no msgcat            -> "has no msgcat attribute."
ok  arm 2  msgcat='other'       -> "has msgcat=\"other\"; the only categories"
ok  arm 3  no admin at all      -> "not one <message> carries msgcat='admin'."
```

Đảo ngược: `None => die(…)` (`build.rs:802`) tạm thành `None => {}` → script `FAIL arm 1:
build succeeded, expected refusal`; hoàn lại → `ok`. Cả hai lần trích output.

**Bước 4.** Thứ tự bắt buộc:

1. Xây binary: `cargo test -p fixbolt-engine --features fix50sp2 --test wire_fixt --no-run`.
2. Trên cây **chưa sửa**, `scripts/check-socket-corpus-under-contention.sh 5 10` → chờ
   `N red in 50`, N ≥ 1, và dòng `unexpected output: 8=FIXT.1.1|9=…|35=0|34=…` (message thừa
   in đủ). Trích. Nếu `35=` không phải `0` — dừng.
3. Sửa harness. Chạy tuần tự: `cargo test … --test wire_fixt` 60/60 và dòng `lifeline hit: 0`
   trong output (`--nocapture`). Chạy script `5 10` → `0 red in 50`. Lặp cho `--test wire`
   (59/59 cả `the_fifty_nine_definitions_pass_through_a_real_socket` và
   `…_pass_in_standard_mode_too`), script → `0 red in 50`.
4. Bước 2 **là** phép đảo ngược của bước 3 — không cần lặp lại; ghi cả hai con số vào
   *Nhật ký giao hàng* kèm máy (tên host, số lõi, lệnh).

**Bước 5.** Viết `shard_recovery.rs` trước → `cargo test -p fixbolt-engine --features affinity
--test shard_recovery` FAIL biên dịch `E0425 … serve_sharded_hft_with_recovery`. Xây xong → test
xanh với `|34=5|` trong Logon trả lời. `serve_sharded_hft_serves_a_session` xanh **không sửa
một dòng** — đó là bằng chứng đường `NoRecovery` không đổi. Đảo ngược: trong thread shard tạm
bỏ `Start::Resumed` và luôn `Fresh` → test mới đỏ `expected |34=5|, got |34=1|`; hoàn lại.

**Bước 3 và 6 — câu chữ cho `STATUS.md`:**

- Hàng 36: gạch toàn hàng, mở đầu `~~…~~ — **CLOSED 2026-09-18 by ADR-0076 +
  crates/conformance/src/mirror.rs, struck 2026-09-2x**: the classification the row asked for
  is that file.`
- Hàng 89: gạch phần đã xây, thêm: `**Built 2026-09-18** (`Limits::listener_every`, ADR-0069
  Accepted, default 16). **What stays open is a measurement, and it belongs to boot D
  (docs/plans/2026-09-20-boot-d.md), not to the-desk-free-residue.**`
- Hàng 94: gạch, `**CLOSED by ADR-0089**: one source `codec/benches/fixture.rs`, guarded by
  `codec/tests/bench_fixture.rs` on every commit.`
- Hàng 32: bỏ "(a)" khỏi câu còn mở; ghi `**(a) recovery CLOSED by ADR-0088**
  (`serve_sharded_hft_with_recovery`). **Still open: the sharded runtime cannot be stopped** —
  needs a design of its own.`
- *Not proven* (a): gạch, thêm số đếm trước/sau và lệnh. (b): gạch, tên script.

Bước 7 (manager) chạy lại **toàn bộ** các lệnh trên tại commit đóng mỗi PR, thêm
`cargo test --all`, `cargo test --no-default-features`, `cargo fmt --all -- --check`,
`scripts/bench.sh` (đọc mọi dòng `alloc`, `ring_full` = 0), và đối chiếu CI run id.

## Tài liệu phải cập nhật

Theo `CLAUDE.md` §4, đi từng hàng:

- [ ] Public API `engine` đổi (`Shardable::add_started`, `Shards<PRE, J>`, `Start<J>`, hai cửa
      mới) → `DESIGN.md` (nơi nhắc `serve_with_recovery`), rustdoc, `CHANGELOG.md` (bước 5, 6)
- [ ] Ràng buộc người dùng phải giữ: `J: Send` cho journal qua channel; `recover`/`fresh` chặn
      thread acceptor → `docs/GUIDE.md` §1a và §6b (bước 6)
- [ ] Kết quả gate conformance: số đếm dưới tải, lệnh, máy → `docs/CONFORMANCE.md` §9 (bước 6)
- [ ] Bẫy: harness mô phỏng đồng hồ hỏi engine quá sớm → đoạn mới trong
      `the-same-commit-went-red-and-green-in-the-same-minute.md` (bước 6); fixture một nguồn →
      đoạn mới trong `a-bench-message-that-fails-its-own-checksum.md` (bước 3)
- [ ] `docs/internals/codec.md` (test `bench_fixture.rs`, module `fixture.rs`),
      `docs/internals/dict.md` (script msgcat), `docs/internals/engine.md`
      (`tests/shard_recovery.rs`; `wire*.rs` settle theo fact) (bước 3, 6)
- [ ] `STATUS.md`: hàng 32, 36, 89, 94; hai gạch *Not proven*; handoff *Start here* do manager
      viết (bước 3, 6, 7)
- [ ] ADR: 0087, 0088, 0089 (bước 0) — không sửa ADR đã Accepted nào; ADR-0034 quyết định 3
      và ADR-0039 *Bad* được **nhắc tới**, không sửa
- [ ] `CLAUDE.md`: **không sửa** — không luật nào đổi. Nếu bước 1 thấy bảng máy-kiểm của
      bất biến 1 nên nêu `fixture.rs`, báo manager, không tự sửa
- [ ] `docs/PRD.md:198` giữ nguyên (shutdown sharded vẫn chưa có)

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Sửa fixture ở một file, ba file còn lại giữ bản cũ (lần 3 và 4 của họ này) | `bench_fixture.rs::no_bench_carries_its_own_copy_of_the_shared_message` |
| Fixture đúng nhưng không ai parse nó dưới `cargo test` (bench `harness = false` không chạy) | `bench_fixture.rs::the_shared_bench_message_parses_clean_under_full_validation` |
| Đặt `fixture.rs` vào `engine/benches/` → cargo autodiscovery biến nó thành target không có `main` | `scripts/bench.sh` đỏ ồn ào; kế hoạch đặt file trong `codec/benches/` (autobenches tắt) |
| Assert liveness cũ `Err(BadCheckSum)` còn sót trong `alloc.rs` | `cargo bench -p fixbolt-codec --bench alloc` panic ngay dòng đó (INVARIANT → fatal) |
| Script msgcat `cd` hay trỏ `cargo` vào thư mục tạm ngoài cây | `scripts/check-scratch-fixtures.sh`; file tạm dưới `target/check-msgcat/` |
| Sau khi script đổi `NANOFIX_FIX44_XML`, build kế tiếp rebuild `dict` một lần — nhìn như "dirty" | Không phải lỗi; `rerun-if-env-changed` là hành vi mong đợi; ghi trong header script |
| Chạy 40 lần tuần tự và kết luận "không tái hiện" | Bước 4 chỉ nhận số đếm từ script chạy **đồng thời**; số tuần tự không được dùng làm bằng chứng |
| Message thừa không phải `35=0` — cơ chế khác với suy luận | Bước 4 mục 2 in `35=` trước khi sửa; khác `0` → dừng, về architect |
| Dòng `I` không đóng khung được (rác cố ý) không có fact `In` → chờ tới dây cứu sinh | Harness coi dòng `next_message` trả `None` là "không có fact", settle bằng quiet; lần chạy tuần tự phải in `lifeline hit: 0` |
| Nâng `STEP_DEADLINE` lên 5 s bị đọc là "tăng timeout cho qua" | Dây cứu sinh không quyết định pass; chạm là in dòng nêu file/dòng/fact; lần chạy tuần tự in 0 lần chạm |
| Mode `standard` (`Block`) trong `wire.rs` chờ trong `idle_with` — vòng chờ fact phải vẫn gọi `idle_with` khi không chuyển động | `the_fifty_nine_definitions_pass_in_standard_mode_too` 59/59; `scripts/check-standard-gives-the-core-back.sh` không đổi |
| `Shards<PRE>` mất suy diễn kiểu ở `tests/shard.rs` sau khi thêm `J` | Default type param `J = Store`; `cargo test -p fixbolt-engine --features affinity --test shard` biên dịch không sửa |
| `Start<J>` qua `mpsc` cần `Send`; `FileJournal` không `Send` | `assert_send::<Start<FileJournal>>()` trong `shard_recovery.rs` — lỗi biên dịch là câu trả lời |
| File test mới thiếu `feature = "standard"` trong `#![cfg]` (bẫy `shard_hft.rs:16-35`) | `cargo test -p fixbolt-engine --no-default-features --features affinity` |
| `recover`/`fresh` bị gọi trên thread shard | Đọc diff `shard.rs`: hai lời gọi nằm **trước** `shards.hand`; review senior nêu dòng |
| Hai kiến trúc sư song song lấy cùng số ADR (boot D) | `scripts/check-adr-numbers.sh` bắt trong một cây; manager chạy vòng `git branch -r` của `two-branches-can-take-the-same-adr-number-without-a-conflict.md` trước khi merge. **Đã xảy ra trong lúc viết**: hai bên đổi số chéo nhau một lượt; chốt: kế hoạch này giữ **0087–0089**, boot D giữ **0090**. Manager chạy `scripts/check-adr-numbers.sh` trước khi commit |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Sau bước 4 vẫn còn cửa sổ giữa "engine ghi reply" và "engine đóng socket" (`eDISCONNECT`) chờ theo wall-time | Thấp | Script đếm sẽ lộ; output nêu file. Nếu xuất hiện: `CountingLog` đếm thêm record `Direction` đóng kết nối (nếu có) hoặc chờ `Ok(0)` theo fact — việc nhỏ, cùng bước, cùng reviewer |
| Step CI mới kéo dài job `gates` ~1–2 phút | Thấp | Kích cỡ `2 5` trên runner 2 vCPU; ghi thời gian thật vào *Nhật ký* |
| Một engine hỏng thật giờ đỏ chậm (5 s/bước) thay vì 50 ms | Thấp | Chấp nhận trong ADR-0087; dòng lifeline nêu ngay fact nào hụt |
| `FileJournal` không `Send` | Trung | ADR-0088 quyết định 4; nếu đỏ biên dịch → thêm `Send` cho `FileJournal` là việc của bước 5 nếu chỉ là bound, còn nếu cần đổi cấu trúc → về architect |
| Đổi `Shardable` phá test `tests/shard.rs` (7 chỗ) ngoài dự kiến | Trung | Bước 5 được phép sửa **chỉ** khi default param không cứu được, và phải nêu tên từng chỗ |
| Số ADR va với boot D | Trung | Va thật 2026-09-20 trong cùng cây, hai lượt đổi số chéo nhau; chốt như hàng trên: 0087–0089 ở đây, 0090 cho boot D (`ADR-0090-a-measurement-boot-…`). Manager kiểm `scripts/check-adr-numbers.sh` trước khi commit và vòng `git branch -r` trước khi merge |

## Ngoài phạm vi

- **Shutdown có trật tự cho `serve_sharded_hft*`** — vẫn mở trong hàng 32, cần thiết kế riêng.
- **`tests/shard_wire.rs`** giữ settle theo wall-time (chính nó ghi có "floor" của máy) — có
  thể nhận `CountingLog` ở kế hoạch sau; không phải test đang đỏ.
- **Sửa `crates/conformance`** (runner, comparator) — cố ý không; đó là gate của bất biến 3.
- **Các literal khác** trong `session/benches/validate.rs`, `engine/benches/density.rs`… — có
  assert riêng, không thuộc họ `167=BOO`.
- **Tính hợp lệ theo dictionary** của fixture (`52=00000000-…`) — fixture chỉ cần hợp lệ ở
  tầng codec, như bốn bench đang dùng.
- **Mọi phép đo**: dải `validate` sau đếm group lồng, A/B `listener_every`, latency FIXT — boot D.
- **Nhánh FIXT của `die()` msgcat** — cùng một `match` trong `emit`; không chạy riêng, nói rõ
  trong header script.

## Nhật ký giao hàng

*Điền khi đóng từng bước: commit, gate xanh (trích), CI run id, cái gì chưa làm và vì sao.*

| Bước | Commit | Gate và output (trích) | Chưa làm |
|---|---|---|---|
| 0 | — | ADR-0087/0088/0089 viết; `check-links.py`, `check-adr-numbers.sh` — xem báo cáo của architect | — |
