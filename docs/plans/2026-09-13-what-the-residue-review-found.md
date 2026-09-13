# Bốn lỗi cũ mà bản review PR #68 tìm ra — đóng items 75, 77, 78, 79

> **Loại:** Plan · **Ngày:** 2026-09-13 · **Trạng thái:** Đã duyệt (owner, 2026-09-13, cả năm đề nghị)
> **Phạm vi:** `crates/engine` (cửa `serve_sharded_hft`, bộ probe `doc_table`), CI (`.github/workflows/ci.yml`), `DESIGN.md` §3 và §6, hai ADR mới

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Bản review cấp senior của PR [#68](https://github.com/tmthang86/fixbolt/pull/68) tìm ra năm
lỗi **đã có sẵn từ trước** PR đó, ghi thành items 75–79 trong `STATUS.md`. Item 76 đã đóng
trên nhánh này bằng một dòng tài liệu (commit `8f4c03f`) và còn nợ một bài `docs/reference/`
— bài đó viết kèm plan này. Bốn item còn lại chưa ai đụng, và mỗi cái mới **đo được đúng một
lần, trên một máy, không có test nào canh**.

Bốn cái đó là:

- **75** — cửa `serve_sharded_hft` mở socket (bind cổng) *trước* khi kiểm tra kế hoạch chia
  core (`ShardPlan::validate`). Nếu cổng đang bị giữ **và** kế hoạch nói sai core, người vận
  hành đọc được lỗi cổng (`AddrInUse`) chứ không đọc được lỗi core — cái họ sửa được trong
  cấu hình thì bị cái họ không sửa được che mất.
- **77** — bộ probe `doc_table` (kiểm tra `docs/CONFIGURATION.md` §1 nói đúng những gì parser
  làm) có một bảng `reader(key)` do người viết tay, nói "khóa này được đọc bởi `match` kia".
  Probe tin bảng đó. Nếu ai chuyển một khóa sang đọc bằng một hàm *khác* có thêm một cách
  viết (ví dụ nhận cả `yes`), probe vẫn đọc `match` cũ và không thấy gì.
- **78** — build `cargo check -p fixbolt-engine --no-default-features --features affinity` in
  ra `warning: unused import: crate::msglog::MaybeLog` ở `crates/engine/src/shard.rs:43`.
  Cảnh báo, không phải lỗi, nên job CI chạy đúng tổ hợp này (`cargo test`) vẫn xanh.
- **79** — job CI `docs` chỉ build rustdoc cho bộ feature mặc định và `--all-features`. Một
  link trong doc đúng dưới `--all-features` nhưng hỏng dưới `--features affinity` một mình;
  không job nào thấy, một developer bắt bằng tay.

**78 và 79 là một họ**: *một tổ hợp feature mà không cổng kiểm nào build*. Đây là lần thứ hai
họ này xuất hiện — lần đầu là item 61, đóng bằng cách thêm đúng một tổ hợp thiếu vào CI. Cách
đó không giữ được: cứ mỗi lỗi lại thêm một tổ hợp bằng tay là vòng lặp không có điểm dừng.
Plan này phải nói **build tổ hợp nào và vì sao dừng ở đó** — số tổ hợp là 2ⁿ, không vét hết
được, nên ranh giới cần một *lý do*, không phải một *con số*.

Kết quả muốn có: bốn item đóng, mỗi cái có một test hoặc một job CI canh, và bài học của item
76 nằm trong `docs/reference/` với test hồi quy của nó.

## Những gì đã biết chắc

**Item 75 — đọc code, không phỏng đoán.**

- `crates/engine/src/shard.rs:497-507`: `serve_sharded_hft_with` kiểm tra bảng đối tác rỗng
  (`NoCounterparties`) rồi **`crate::Acceptor::bind(addr)` ở dòng 507**. Mở file log từng
  shard ở 523–535. `Shards::start(plan, …)` ở dòng 536, và `plan.validate()` nằm *bên trong*
  đó, dòng 231, với comment *"ADR-0015 decision 6: before a single thread exists"*. Vậy thứ
  tự hiện tại là: bảng → **bind** → mở file log → **validate** → sinh thread.
- `crates/engine/src/lib.rs:2838-2839`: `serve_hft_pinned` làm `pin.validate()` →
  `pin_current_thread` → rồi mới bind. Test canh thứ tự đó:
  `crates/engine/tests/hft_pinned.rs:217`, `serve_hft_pinned_refuses_a_core_the_machine_does_not_have`
  — giữ sẵn một cổng, đưa `CoreId(4096)`, đòi lỗi phải là `Affinity(NoSuchCore(4096))`. Doc
  của test nói rõ: *"a refusal is only evidence of order if the bind would have failed too"*.
- `ShardError` (`shard.rs:116-130`) có `Affinity(AffinityError)`, `Io(std::io::Error)`,
  `ThreadGone`, `BadRoute`, `NoCounterparties`; `impl From<AffinityError> for ShardError` ở
  dòng 162, nên `plan.validate()?` dùng được trực tiếp.
- ADR-0015 decision 6 (`docs/decisions/ADR-0015-…md:119-121`): chữ nói *thread*, lý do nói
  *"leaves threads to join and sockets to close"* — tức là cả socket. `CLAUDE.md` §5 cấm sửa
  nội dung ADR đã Accepted, nên mở rộng chữ phải bằng ADR mới: **ADR-0064**, viết kèm plan này.
- **Tiền lệ bên ngoài, đọc chứ không chép (bất biến 9)** — `[researched 2026-09-13]`:
  QuickFIX (C++) tạo mọi session từ settings trong `Acceptor::initialize()` (constructor) và
  chỉ bind trong `onInitialize()` khi `start()`/`block()`; lỗi cấu hình ném ra trước khi có
  socket nào. QuickFIX/J `AbstractSocketAcceptor.startAcceptingConnections()` gọi
  `createSessions(getSettings(), continueInitOnError)` và `startSessionTimer()` **trước** vòng
  lặp `ioAcceptor.bind(...)`. QuickFIX/n `ThreadedSocketAcceptor` tạo session từ settings
  trong constructor, bật listener trong `Start()`. Cả ba: kiểm cấu hình trước, bind sau.
- **Trên Mac không build được `shard.rs`**: `lib.rs:40-41` gate `mod shard` bằng
  `cfg(all(feature = "affinity", target_os = "linux"))`. Test cho item 75 vì thế chỉ chạy
  trên CI (job *The affinity feature builds…*) — cách kiểm từ bàn làm việc là
  `cargo check --tests -p fixbolt-engine --features affinity --target x86_64-unknown-linux-gnu`
  (target này đã cài: `rustup target list --installed` liệt kê `x86_64-unknown-linux-gnu`).

**Item 77 — đọc code.**

- `crates/engine/src/settings.rs:2100-2135`: `const fn reader(key: Key) -> Reader` là bảng
  viết tay, khớp hết mọi biến thể `Key` (không có nhánh `_`). Nó khai báo: `ConnectionType`
  → `Literals(None, "let what = match value {")`; chín khóa cờ → `Literals(Some(FLAG_FN),
  "match value {")`; tám khóa số → `Numeric`; mười lăm khóa còn lại → `Prose`.
- Nhánh đọc-arm của probe 3 (`settings.rs:3023-3031`) lấy `reader(key)`, đọc literal trong
  arm của đúng `match` mà bảng khai báo, và so với ô *Values*. **Nó không hề nhìn xem parser
  có thực sự gọi hàm đó cho khóa đó không.**
- Chỗ parser *thực sự* đọc từng khóa là các dòng gọi hàm mang `Key::X` làm đối số — đếm được
  **25 dòng** hôm nay: `flag(v, Key::…)` × 9 (dòng 751, 780, 1533, 1538, 1543, 1562, 1572,
  1582, 1585), `number(v, Key::…)` × 7 (1522, 1525, 1552, 1556, 1629, 1633, 1637),
  `integer_as_written(…, Key::TimestampPrecision, …)` × 1 (1593), `fitting(…, Key::…)` × 3
  (1517–1519), `time_of_day(…, Key::…)` × 2 (1757–1758), `one_day(…, Key::…)` × 3 (1790, 1792,
  1823). Mỗi dòng đều để tên hàm và `Key::X` **trên cùng một dòng**. `ConnectionType` là
  ngoại lệ duy nhất: đọc bằng `match value` tại chỗ (dòng 1148), không qua hàm.
- `mod doc_table` bắt đầu ở dòng 1899; `SRC = include_str!("settings.rs")` ở 1904 — probe đọc
  chính file nguồn của nó, kể cả phần test. Nhánh mới phải **dừng quét ở dòng `mod doc_table {`**
  để không đọc chính mình.
- Ô *Values* của tám khóa số hôm nay (`docs/CONFIGURATION.md` §1): `HeartBtInt` *non-negative
  integer … zero means no heartbeats*; `LogonTimeout`/`LogoutTimeout` *integer … `0` is off*;
  `MaxSkewMillis`, `ReconnectInterval`, `ReconnectCeiling` *integer*; `SocketConnectPort`
  `` `0`–`65535` ``; `TimestampPrecision` `` `3`, `6` or `9` `` (probe 3 đã lo ô này). Không ô
  nào còn chữ *positive*. Probe 6 (`settings.rs:3098`) có sẵn `with_value`, `sample`, `group`,
  `is_about_the_value` để dùng lại.

**Item 78 — đo lại 2026-09-13, trên bàn này.**

- `cargo check -p fixbolt-engine --no-default-features --features affinity` trên macOS:
  `Finished … in 2.27s`, **không cảnh báo** — kể cả sau `touch shard.rs`. Vì `mod shard`
  không compile trên darwin (xem trên), đây là *không nhìn thấy*, không phải *không có*.
- Cùng lệnh với `--target x86_64-unknown-linux-gnu`: `warning: unused import:
  crate::msglog::MaybeLog --> crates/engine/src/shard.rs:43:5` … `fixbolt-engine (lib)
  generated 1 warning`. **Tái hiện đúng như STATUS ghi.** Với `--features affinity` (có
  `standard`): sạch.
- Nguyên nhân: `use crate::msglog::MaybeLog;` ở dòng 43 không có `#[cfg]`, còn hai chỗ dùng
  nó (dòng 536, 556) nằm trong `serve_sharded_hft_with`, hàm này `#[cfg(feature = "standard")]`
  (dòng 470).
- **Một câu trong STATUS row 78 sai một nửa**: nó viết *"CI builds `--no-default-features`
  and `--features affinity` separately, never the two together"*. Thực ra `ci.yml:349` có
  `cargo test -p fixbolt-engine --no-default-features --features affinity` — **CI có build
  đúng tổ hợp này**, nhưng bằng `cargo test`, mà `cargo test` để cảnh báo đi qua. Lỗ hổng
  không phải "thiếu tổ hợp", mà là "tổ hợp đó không chạy qua `-D warnings`". Khi gạch row 78
  phải sửa câu đó.

**Item 79 — đọc CI.**

- `ci.yml:165-183`: job `docs` chạy `cargo doc --workspace --no-deps` với
  `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::redundant_explicit_links"`
  hai lần — mặc định và `--all-features`. Comment trong job ghi đúng bài học item 61.
- `ci.yml:345-349`: job affinity chạy `clippy --all-targets --features affinity -- -D warnings`
  (có `standard`), rồi hai lệnh `cargo test`. Không có `clippy -D warnings` cho tổ hợp
  `--no-default-features --features affinity`.
- Feature toàn workspace (đọc từng `Cargo.toml`): `fixbolt-engine` có `standard` (mặc định),
  `affinity`, `tls` → 8 tổ hợp; `tools/w2w` cũng 3 → 8; `crates/library` và `tools/interop`
  mỗi cái 1 (`standard`) → 2; các crate khác không có feature. **Với độ sâu 2, hôm nay vét
  được toàn bộ** (3 feature thì "mọi cặp + tất cả" chính là 8/8). `[sai — độ sâu 2 không có
  "tất cả"; xem Sửa đổi giữa lúc dựng, 2026-09-13, review D2]`

**Tiền lệ cho 78/79** — `[researched 2026-09-13]`:

- [`cargo-hack`](https://github.com/taiki-e/cargo-hack): `--each-feature` (từng feature một),
  `--feature-powerset` (mọi tổ hợp), `--depth N` (tối đa N feature cùng lúc; `--depth 1` bằng
  `--each-feature`). Cả hai chế độ đều tự thêm `--no-default-features`, bộ mặc định và
  `--all-features`; tự gộp các tổ hợp tương đương. Cài trong GitHub Actions bằng
  `taiki-e/install-action@cargo-hack`. Là binary chạy trong CI, **không phải dependency của
  crate nào** — `Cargo.lock` và `deny.toml` không đổi.
- [tokio](https://github.com/tokio-rs/tokio), file `.github/workflows/ci.yml` của nó:
  `cargo hack check --all --feature-powerset --depth 2 --keep-going` và `cargo hack test
  --each-feature`; rustdoc chạy với `RUSTDOCFLAGS: --cfg docsrs … -Dwarnings`. hyper và
  futures dùng cùng dạng `--feature-powerset --depth 2`.
- [serde](https://github.com/serde-rs/serde), cùng file:
  liệt kê tay sáu bộ feature qua ba job — cách làm được khi danh sách feature cố định và nhỏ.
- [Rust Project Primer — Crate Features](https://rustprojectprimer.com/checks/features.html):
  khuyên `--feature-powerset --depth 2` cho check biên dịch, `--each-feature` cho test; vét
  hết powerset là không thực tế ở số feature thật.
- Về link rustdoc dưới `cfg(feature)`: `doc_auto_cfg` đã bị gỡ khỏi nightly, `#[doc(cfg)]`
  vẫn là unstable; cách stable duy nhất là **build doc dưới từng bộ feature** hoặc gate chính
  dòng doc bằng `#[cfg_attr(feature = "…", doc = "…")]`. Không có công cụ nào kiểm link
  "cho mọi cfg" trong một lần chạy — tìm không ra.

**Item 76 — nguồn cho bài `docs/reference/`.** FIX 4.4 định nghĩa `108=0` là *không gửi
heartbeat* ([B2BITS tag 108](https://www.b2bits.com/fixopaedia/fixdic44/tag_108_HeartBtInt_.html),
[OnixS msgtype 0](https://www.onixs.biz/fix-dictionary/4.4/msgtype_0_0.html)).
`crates/session/src/lib.rs:1322` và `:2262-2263` đã cài và ghi chú đúng như vậy;
`crates/engine/src/settings.rs:1522` nhận mọi `u32`, không kiểm khoảng. Ô
`docs/CONFIGURATION.md:77` nay đã đúng.

## Cách làm

Chỉ phương án được chọn. Phương án loại nằm trong hai ADR.

**Item 75** (ADR-0064): trong `serve_sharded_hft_with`, chuyển `plan.validate()?` lên **ngay
sau** kiểm tra `NoCounterparties` và **trước** `Acceptor::bind`. Thứ tự mới: kiểm tra chỉ đọc
đối số → kiểm tra đọc máy (`/sys`) → chiếm tài nguyên (socket, file, thread). `Shards::start`
**giữ** `validate()` của nó — nó là API public riêng, có test riêng; đọc `/sys` hai lần lúc khởi
động không nằm trên đường nóng. Thứ tự *giữa các tài nguyên* (socket rồi file rồi thread)
không đổi — xem *Ngoài phạm vi*. Test mới trong `crates/engine/tests/shard_hft.rs` (file đã
gate `affinity + standard + linux`, đã có `cfg()`, `free_addr()`, `one_shard()` dùng lại được):
giữ một cổng, đưa `ShardPlan::new(vec![CoreId(4096)])`, đòi `Err(ShardError::Affinity(
AffinityError::NoSuchCore(CoreId(4096))))`. Đây là **thay đổi hành vi quan sát được** trên
cửa public — với hai lỗi cùng lúc, caller thấy `Affinity` thay cho `Io` — nên ghi
`CHANGELOG.md` mục *Changed*.

**Item 78**: `#[cfg(feature = "standard")]` đặt lên dòng `use crate::msglog::MaybeLog;`
(`shard.rs:43`). Một dòng, cùng file với item 75, cùng người làm, cùng bước.

**Item 77**: thêm một nhánh thứ ba cho probe 3 trong `mod doc_table` —
`the_reader_table_matches_the_call_sites` — **đọc parser để kiểm bảng, thay vì tin bảng**:

1. Quét `SRC` từ đầu đến dòng `mod doc_table {` (không đọc chính mình). Mỗi dòng có dạng
   `<tên hàm>(…, Key::<Biến thể>` — `Key::` đứng ở vị trí đối số thứ hai trở đi, sau một dấu
   phẩy, trong ngoặc mở bởi một định danh — là **một call site**: ghi (biến thể → tên hàm).
   Dòng `Key::X =>` hay `Key::X |` (arm của `match key`) không tính. Dòng comment không tính.
2. Bảng hàm đã biết: `flag` ↔ `Literals(Some(FLAG_FN), _)`; `number` và `integer_as_written`
   ↔ `Numeric`; `fitting`, `time_of_day`, `one_day` ↔ `Prose`. `ConnectionType` không có call
   site và phải khai báo `Literals(None, CONNECTION_TYPE_MATCH_OPEN)`.
3. Ba khẳng định, mỗi cái một câu FAIL nêu tên khóa và tên hàm:
   - mọi call site gọi **một hàm trong bảng đã biết** — *"`flag_or_yes` reads `ResetOnLogon`,
     and this leg does not know what literals it accepts"*;
   - với mỗi khóa, hàm ở call site **khớp** với `reader(key)` — *"`ResetOnLogon` is declared
     `Literals(flag)` but the parser reads it through `number`"*;
   - mỗi khóa khai báo `Literals` hay `Numeric` có **ít nhất một** call site — *"`X` is
     declared read by `flag` but no call site reads it"*.
4. Sàn: **25 call site**, không hạ (`FLOOR`, kiểu probe 6). Một lời gọi bị rustfmt bẻ xuống dòng
   sẽ *mất* call site → sàn đỏ, thay vì lặng lẽ bỏ qua.

**Item 76, test hồi quy** (cùng file, cùng người, bước sau): probe 7 —
`a_values_cell_that_names_a_bound_is_a_bound_the_parser_holds`. Với mỗi hàng mà `reader(key)`
là `Numeric`, đọc ô *Values*: chứa chữ `positive` → parse `0` **phải bị từ chối** (lý do về
giá trị, theo `is_about_the_value`); chứa `non-negative`, hoặc `` `0` `` → `0` **phải được
nhận**; dạng `` `a`–`b` `` → `b+1` **phải bị từ chối**, và `a` phải được nhận. Ô không có dấu
hiệu nào → đếm `skipped`, không đếm là đạt. Sàn: **4 hàng** `[sửa 2026-09-13, xem *Sửa đổi
giữa lúc dựng* — bản đầu ghi 7, lấy nhầm từ số của probe 6]`.
Đảo chiều: đổi ô `HeartBtInt` về *positive integer, seconds* → đỏ với câu
*"docs/CONFIGURATION.md §1: HeartBtInt says positive but the parser accepts 0 — read FIX 4.4
before changing either side"*. **Trong phạm vi vì**: bài `docs/reference/` viết trong plan này,
`CLAUDE.md` §4 nói mỗi bẫy ghi lại phải có test; cùng module, cùng người đang mở file cho item
77, ~40 dòng. Nếu tách ra plan khác, nó sẽ là một bullet *Not proven* nữa.

**Items 78 + 79 — một job CI mới** (ADR-0065): job `feature-sets` trong `ci.yml`, sau
`scripts/fetch-quickfix-assets.sh` và `taiki-e/install-action@cargo-hack`, chạy hai lệnh:

```sh
cargo hack clippy --workspace --all-targets --feature-powerset --depth 2 --keep-going -- -D warnings
RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::redundant_explicit_links" \
  cargo hack doc --workspace --no-deps --feature-powerset --depth 2 --keep-going
```

**Ranh giới là độ sâu 2, và lý do**: mọi lỗi của họ này gặp ở đây tới nay đều cần đúng *một
feature bật và một feature tắt* — `tls` tắt (item 61); `standard` tắt, `affinity` bật (78,
79). Cặp là bộ nhỏ nhất mà "bật" và "tắt" cùng có tên: bộ một-feature nói feature đó cần gì,
bộ hai-feature nói hai feature dùng chung gì (ở đây là một `libc` optional). Lỗi cần đúng ba
feature bật cùng lúc chưa từng thấy, và mỗi mức sâu thêm nhân số lần build với số feature.
Ranh giới **giữ ở 2 khi số feature tăng**; ngày tìm ra lỗi ba-feature, plan sửa nó quyết định
nâng sâu hay thêm bộ đó bằng tên, và ghi ADR mới. Hôm nay sâu 2 tình cờ vét hết (8/8 cho
engine) `[sai — xem Sửa đổi giữa lúc dựng, 2026-09-13, review D2]` — nói rõ để ngày nó không còn vét hết thì đó là quyết định có hồ sơ, không phải con số
ai đó chọn. **Không chạy test theo tổ hợp** — lý do ở ADR-0065 quyết định 3.

Hai dòng `cargo doc` của job `docs` **bị gộp vào** (mặc định và `--all-features` đều nằm trong
powerset `[sai — --all-features không nằm trong đó; xem Sửa đổi giữa lúc dựng, 2026-09-13,
review D2]`) và bỏ đi; comment ghi item 61 chuyển sang job mới. Hai dòng `cargo test` của job
affinity giữ nguyên.

**Đảo chiều cho job mới — viết trước khi dựng, câu FAIL phải in ra:**

- **R78-1**: bỏ `#[cfg]` vừa thêm ở `shard.rs:43` → lệnh clippy đỏ, log có dòng cargo-hack
  `info: running cargo clippy --no-default-features --features affinity on fixbolt-engine`
  theo sau là `error: unused import: crate::msglog::MaybeLog`. Cùng lúc, hai lệnh clippy *cũ*
  của CI (`--features affinity` có `standard`) **vẫn xanh** trên cùng cây — chứng minh đảo
  chiều đổi được thứ mà cổng cũ không thấy.
- **R79-1**: thêm vào doc của một item *không* gate feature (ví dụ đầu `crates/engine/src/lib.rs`)
  một link `` [`tls::TlsTransport`] `` → lệnh doc đỏ với `error: unresolved link to
  `tls::TlsTransport`` dưới một bộ không có `tls`, và **`cargo doc --workspace --no-deps
  --all-features` cũ vẫn xanh** trên cùng cây.

Cả hai đảo chiều chạy được **từ bàn làm việc** với `--target x86_64-unknown-linux-gnu` (đã cài),
trước khi push.

**Tài liệu**: ADR-0064 và ADR-0065 đã viết ở trạng thái `Proposed`; bài
`docs/reference/a-documentation-cell-is-not-a-specification.md` đã viết. `DESIGN.md` §6 thêm một
hàng *Correctness* cho job mới, nêu độ sâu; `DESIGN.md` §3 hàng `shard` (dòng 121) thêm "kiểm
kế hoạch trước khi bind"; `docs/GUIDE.md:50` thêm nửa câu cùng ý; `CHANGELOG.md` *Changed*.

## Bất biến bị đụng tới

Việc này đụng `engine` (`shard.rs`, `settings.rs` phần test), nên đi hết danh sách:

- **1 (không cấp phát trên đường nóng)** — không đụng. `validate()` chạy một lần lúc khởi động,
  trước khi có kết nối nào; thêm `#[cfg]` lên một `use` không sinh mã. `benches/alloc.rs`
  không cần chạy lại, và plan nói rõ như vậy thay vì chạy cho yên tâm (§7).
- **4 (hft không ngủ / standard phải ngủ)** — không đụng. Thứ tự khởi động không nằm trên
  đường nóng; không đổi wait strategy, không đổi readiness.
- **6 (feature gate cả `mod`)** — **được siết chặt hơn**: job mới build mọi bộ feature tới sâu
  2, tức chính là cổng kiểm cho bất biến này ở mức "biên dịch được và doc không hỏng".
  `scripts/check-no-optional-deps.sh` và job `--no-default-features` vẫn nguyên.
- **7 (không `unwrap`/`expect`/`panic` trong crate thư viện)** — test mới nằm trong
  `tests/shard_hft.rs` (đã có `allow` ở đầu file, không phải `crates/*/src`) và trong
  `mod doc_table` (`#[cfg(test)]`). Code thư viện chỉ thêm một `?` đã có `From`.
- **10 (không số hiệu năng thiếu nguồn)** — plan không công bố con số hiệu năng nào. Thời gian
  chạy job CI được đo ở bước 0 và ghi là thời gian job, không phải hiệu năng engine.
- 2, 3, 5, 8, 9 — không đụng `session`, không đụng bảng thứ tự trường, không `unsafe`, không
  chép QuickFIX (ba tiền lệ ở trên là *đọc thứ tự*, ghi bằng lời, không chép dòng nào).

## Chia việc

Vai và model theo `CLAUDE.md` §12. Mọi bước đụng `crates/engine` do **senior developer
(opus)** làm. Các bước đụng cùng một file chạy **nối tiếp**, không song song.

| Bước | Kết quả | File đụng | Vai · model | §2 | Lệnh đóng bước | Phụ thuộc |
|---|---|---|---|---|---|---|
| 0 | **Đo trước khi dựng.** Cài `cargo-hack` trên bàn (`cargo install cargo-hack --locked`), chạy hai lệnh của job mới với `--target x86_64-unknown-linux-gnu`; trích **danh sách tổ hợp** cargo-hack in ra, **cái nào đỏ** (78 phải đỏ; nếu `--no-default-features --features tls` hay bộ nào khác cũng đỏ thì ghi lại — đó là lỗi mới, xem *Rủi ro*), và **thời gian chạy** | không sửa file | runner · haiku | — | hai lệnh ở *Cách làm*, output nguyên văn | `vendor/` đã fetch |
| 1 | **Item 77**: nhánh `the_reader_table_matches_the_call_sites` trong `mod doc_table`, sàn 25; đảo chiều R77-1 (xem *Bẫy*) chạy và trích | `crates/engine/src/settings.rs` (chỉ trong `mod doc_table`) | senior dev · opus | 7 | `cargo test -p fixbolt-engine --lib doc_table` xanh, in số call site; `cargo clippy --all-targets -- -D warnings` | — |
| 2 | **Item 76, test hồi quy**: probe 7 `a_values_cell_that_names_a_bound_is_a_bound_the_parser_holds`, sàn **4** `[sửa 2026-09-13]`; đảo chiều R76-1 chạy và trích | `crates/engine/src/settings.rs` (cùng module) | senior dev · opus | 7 | như bước 1, và `cargo test -p fixbolt-engine --lib doc_table --features tls` (hàng §6 hiện có chạy cả hai) | 1 (cùng file) |
| 3 | **Items 75 + 78**: `plan.validate()?` lên trước `bind` (ADR-0064); `#[cfg(feature = "standard")]` lên `use MaybeLog`; test `serve_sharded_hft_refuses_the_plan_before_it_binds` trong `shard_hft.rs`; đảo chiều R75-1 và R78-1 chạy **với target Linux** và trích; `CHANGELOG.md` *Changed*; `DESIGN.md:121`; `GUIDE.md:50` | `crates/engine/src/shard.rs`, `crates/engine/tests/shard_hft.rs`, `CHANGELOG.md`, `docs/DESIGN.md` §3, `docs/GUIDE.md` | senior dev · opus | 1, 4, 7 (đi qua, không đụng) | `cargo check --tests -p fixbolt-engine --features affinity --target x86_64-unknown-linux-gnu` sạch **0 warning**; cùng lệnh với `--no-default-features` sạch; `cargo test --all`; `cargo clippy --all-targets -- -D warnings`. **Test 75 chỉ chạy trên CI** — manager đọc log job *The affinity feature builds…* và trích dòng `serve_sharded_hft_refuses_the_plan_before_it_binds … ok` | — (file khác bước 1–2) |
| 4 | **Items 78 + 79, cổng**: job `feature-sets` trong `ci.yml` (ADR-0065); gộp hai dòng `cargo doc` của job `docs`; chuyển comment item 61; `DESIGN.md` §6 hàng mới nêu **độ sâu 2 và lý do**; đảo chiều R79-1 chạy từ bàn và trích | `.github/workflows/ci.yml`, `docs/DESIGN.md` §6 | developer · sonnet | — (không đụng `crates/`) | `actionlint` hoặc `yamllint` nếu có trên máy (nếu không, nói rõ là không chạy); job mới **xanh trên CI** cho commit này, run id ghi lại; `scripts/check-links.py` chạy **trong worktree** | 3 (cổng phải xanh sau khi 78 đã sửa; R78-1 làm ở bước 3 là bằng chứng đỏ) |
| 5 | **Senior review**, context mới, đưa plan + gate, không đưa lý luận của manager; một lần cho cả PR (§12) | đọc, sửa nếu có finding | senior dev · opus | tất cả | các lệnh ở trên chạy lại sau sửa | 1–4 |
| 6 | **Đóng**: ADR-0064, ADR-0065 → `Accepted`; `STATUS.md` gạch 75, 77, 78, 79 (**sửa câu sai của row 78** — xem *Những gì đã biết chắc*), sửa mục *Items 75–79: what is left*, gạch bullet *Not proven* "Items 75–79 are measured once each…", viết *Start here* mới làm handoff; *Nhật ký giao hàng* của plan này; run id CI xanh của commit đóng | `STATUS.md`, hai ADR, plan này | manager | — | CI xanh trên commit đóng, **run id nêu tên** | 5 |

## Cách kiểm chứng

Từng bước một. "Xanh" là output trích nguyên văn, không phải mã thoát.

- **Bước 0** — output cargo-hack phải liệt kê cho `fixbolt-engine` đủ 8 bộ: `--no-default-features`,
  `standard`, `affinity`, `tls`, `standard,affinity`, `standard,tls`, `affinity,tls`,
  `--all-features` (cargo-hack có thể gộp bộ tương đương; số nhỏ hơn 8 phải có lời giải thích
  của nó trong log). `[đo 2026-09-13]` **Thực tế là 10, không phải 8**: cargo-hack coi `default`
  là một feature có tên, nên thêm `default,affinity` và `default,tls` — xem *Sửa đổi giữa lúc
  dựng*. Bộ `--no-default-features --features affinity` **phải đỏ** với `unused
  import: crate::msglog::MaybeLog`. Không đỏ = cổng chưa nhìn thấy điều nó được dựng để thấy →
  dừng, hỏi lại.
- **Bước 1** — trước khi viết code: viết R77-1 (sao `flag` thành `flag_or_yes` nhận `"Y" |
  "yes"`, chuyển `Key::ResetOnLogon` sang nó, **không** sửa `reader`), chạy `cargo test -p
  fixbolt-engine --lib doc_table` **hiện tại** → phải **xanh** (đó là item 77, chứng minh
  bằng chạy chứ không bằng đọc). Viết nhánh mới → đỏ trên đúng câu *"`flag_or_yes` reads
  `ResetOnLogon`…"*. Hoàn lại → xanh, in `25 call sites`.
- **Bước 2** — R76-1: đổi ô `HeartBtInt` về *positive integer, seconds* → probe 7 đỏ trên
  câu nêu `HeartBtInt` và `0`. Hoàn lại → xanh, `4 probed, 4 skipped, 25 not Numeric` `[sửa
  2026-09-13; bản đầu ghi 7]`. Kiểm thêm rằng probe 3 và 6 **không đổi kết quả** dưới R76-1 (đó
  là lỗ hổng cũ, cho thấy bằng chạy).
- **Bước 3** — R75-1: đặt `bind` lại trên `validate` → test mới đỏ với `expected
  Err(Affinity(NoSuchCore(CoreId(4096)))) before any bind; got Err(Io(… AddrInUse …))`. Chỉ
  chạy được trên CI (Linux); trên bàn, `cargo check --tests … --target x86_64-unknown-linux-gnu`
  chứng minh test **biên dịch** trong bộ `affinity` — không hơn. Manager trích dòng `ok` từ log
  CI. R78-1: bỏ `#[cfg]` → `cargo check … --no-default-features --features affinity --target
  x86_64-unknown-linux-gnu` in lại `warning: unused import`; đặt lại → `0 warning`.
- **Bước 4** — R79-1 từ bàn với cargo-hack + target Linux: lệnh doc đỏ với `unresolved link to
  `tls::TlsTransport``, và lệnh doc **cũ** `--all-features` xanh trên cùng cây. Hoàn lại → cả
  hai xanh. Trên CI: job mới xanh, **và** log của nó có ≥ 10 `[sửa 2026-09-13; bản đầu 8]` dòng `info: running … on
  fixbolt-engine` (đếm, không suy).
- **Không chạy** `benches/alloc.rs`, bộ Criterion, `w2w`, hai script bất biến 4 — không bước
  nào đụng đường nóng, wait strategy hay readiness (§7: mở rộng phạm vi là nêu thêm case, không
  phải "chạy hết cho chắc").

## Tài liệu phải cập nhật

Đi từng hàng bảng `CLAUDE.md` §4:

- [ ] *Public API của crate* → `CHANGELOG.md` *Changed*: `serve_sharded_hft`/`_with` kiểm kế
      hoạch trước khi bind; hai lỗi cùng lúc nay trả `Affinity` thay `Io`. `DESIGN.md` §3
      hàng `shard` (dòng 121). Rustdoc của hàm (`shard.rs:436-439`, mục *Errors*) thêm một
      câu về thứ tự.
- [ ] *Ràng buộc người dùng phải giữ mà compiler không kiểm* → `docs/GUIDE.md:50`: cửa kiểm
      kế hoạch **trước khi chiếm cổng**, nên một lỗi cổng đọc được là lỗi cổng thật.
- [ ] *Mục tiêu hay cách đo của một cổng* → `DESIGN.md` §6 *Correctness*: hàng mới cho job
      `feature-sets`, nêu độ sâu 2, lý do dừng, và `[measured]` run id; hàng
      *"`docs/CONFIGURATION.md` §1 says what the parser does"* (dòng 41) sửa *six probes* →
      *seven probes* và thêm nhánh call-site. **Cùng commit với `ci.yml`.**
- [ ] *Quyết định kỹ thuật* → ADR-0064 (75), ADR-0065 (78/79) — đã viết, `Proposed` → `Accepted` ở bước 6.
- [ ] *Bẫy / giả định sai* → `docs/reference/a-documentation-cell-is-not-a-specification.md`
      — đã viết (item 76). Không có bẫy mới nào khác đã đo trong plan này; nếu bước 0 tìm ra bộ
      feature đỏ ngoài 78, bài thứ hai viết ở bước 6.
- [ ] *Chứng minh được điều đã ghi là chưa chứng minh* → `STATUS.md` bullet *Not proven* "Items
      75–79 are measured once each by the review, on this desk, with no test guarding any of
      them" — gạch. Rows 75, 77, 78, 79 gạch, row 78 **sửa câu sai** về CI. Mục *Items 75–79: what
      is left, and what each is blocked on* cập nhật.
- [ ] *Hành vi biên session* → `SESSION-BEHAVIOUR.md`: **không** — không đổi logon/resend/reject.
- [ ] *Hằng số, mặc định, khóa cấu hình* → `CONFIGURATION.md`: **không đổi thêm** — ô `HeartBtInt`
      đã sửa ở `8f4c03f`; probe 7 giờ canh nó.
- [ ] *Con số conformance* → `CONFORMANCE.md`: **không** — không con số nào đổi; tiền lệ item 61
      (cổng rustdoc) cũng không ghi vào đó.
- [ ] *Best practices theo mode*, *hft-playbook*, `DESIGN.md` §8/§9: **không** — không đụng mode,
      OS hay ngân sách độ trễ.
- [ ] `PRD.md`: **không** — không đổi phase.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| **Test item 75 xanh vì bind không hề thất bại** (cổng không bị giữ thật) — khi đó thứ tự nào cũng trả `Affinity` | test giữ cổng bằng `TcpListener::bind` trong cùng process và **giữ handle sống** đến hết assert (kiểu `a_held_addr()` trong `hft_pinned.rs:127`); R75-1 phải đọc `Io(AddrInUse)`, không phải một lỗi khác |
| **Test item 75 chạy trên bàn mà không biết mình không compile** — `shard_hft.rs` gate `linux` nên trên Mac nó biến mất, không đỏ | manager đếm dòng `ok` của test trong log job *The affinity feature builds…*; file đã ghi *"if both ever read zero, this file is dead"* (dòng 64–65) — đọc hai con số `passed` cạnh nhau như file dặn |
| **Nhánh 77 đọc luôn chính nó** — `SRC` gồm cả `mod doc_table`, và code mới có chữ `Key::` | quét dừng ở dòng `mod doc_table {`; assert dòng đó tồn tại đúng một lần (kiểu `opener_hits == 1` ở `settings.rs:2054`) |
| **Một call site bị rustfmt bẻ xuống dòng → nhánh 77 mất nó và lặng lẽ đếm thiếu** | sàn 25 call site, và mỗi khóa `Literals`/`Numeric` phải có ≥ 1 — thiếu là đỏ có tên khóa |
| **R77-1 không phải đảo chiều thật** — nếu probe 3 hiện tại đã đỏ với `flag_or_yes` thì item 77 không tồn tại | chạy probe 3 **trước** khi viết nhánh mới, dưới R77-1, và trích dòng `passed` (bước 1, dấu đầu tiên) |
| **Probe 7 đọc "positive" trong một ô không phải khóa số** (ví dụ chữ *positive* trong ô prose) | probe 7 chỉ đọc hàng có `reader(key) == Numeric`; ô khác đếm `skipped` và in ra |
| **`cargo hack doc` không nhận `RUSTDOCFLAGS`** hay cargo-hack không chuyển được subcommand `doc` | bước 0 đo trước; R79-1 phải đỏ **qua chính lệnh của job**, không qua `cargo doc` gọi tay. Nếu không đỏ: dừng, báo, fallback là vòng `for` shell trên danh sách bộ do `cargo metadata` cho — quyết định lại với owner |
| **Job mới xanh vì cargo-hack gộp mất bộ cần thiết** (dedup tổ hợp tương đương) | đếm dòng `info: running … on fixbolt-engine` trong log ≥ 10 `[sửa 2026-09-13]`; R78-1 và R79-1 chứng minh hai bộ cần thiết *thực sự* chạy |
| **Gộp hai dòng `cargo doc` của job `docs` làm mất cổng cũ nếu job mới bị tắt/skip** | job mới **không** có `continue-on-error`, không có `if:`; comment item 61 chuyển kèm để lý do không bị mất |
| **Gộp cả `--features tls` vào powerset kéo theo yêu cầu kernel** — job `tls` hiện chạy trên runner riêng vì test cần kTLS | job mới chỉ `clippy` và `doc`, **không chạy test**; `rustls`/`ktls-core` chỉ cần biên dịch. Bước 0 xác nhận bộ `tls` build được trên bàn với target Linux |
| **Sửa nhầm ADR-0015** thay vì viết ADR mới (§5) | `git diff --stat` của bước 3 và 6 không được có `ADR-0015`; reviewer bước 5 kiểm |
| **Test 75 rò một thread quay** như test `serve_sharded_hft_serves_a_session` | không: cửa bị từ chối **trước** khi sinh thread (đó chính là điều đang kiểm); assert thêm rằng lỗi trả về trong < 5 s để một cửa "treo" không đọc thành "đạt" |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Bước 0 phát hiện một bộ feature khác cũng đỏ (khả năng nhất: `--no-default-features --features tls`, chưa ai build) | Trung bình | Đó là **lỗi mới do cổng tìm ra**, không phải lỗi của plan. Nếu sửa ≤ 10 dòng và không đổi hành vi: sửa trong bước 3 bởi senior dev, ghi `[measured]` vào ADR-0065 và một bài `docs/reference/` ngắn. Nếu lớn hơn: mở item STATUS mới, job dùng `--exclude-features` **có ghi lý do và ngày** cho đúng bộ đó, plan nêu rõ là để lại |
| Thời gian job `feature-sets` trên `ubuntu-latest` vượt 15 phút | Thấp | bước 0 đo trên bàn (ước lượng trên); nếu CI vượt, thử `--depth 1` cho `doc` trước, giữ `2` cho `clippy`, sửa ADR-0065 tại chỗ khi còn `Proposed` |
| `taiki-e/install-action` — thêm một action bên thứ ba, pin theo tag | Thấp | cùng mức tin cậy với `EmbarkStudios/cargo-deny-action@v2` đã dùng; pin theo hash cho cả hai là quyết định riêng, ghi trong ADR-0065 *Bad* |
| Thay đổi thứ tự lỗi của `serve_sharded_hft` làm caller bên ngoài vỡ | Rất thấp | chưa có bản phát hành nào; `CHANGELOG.md` ghi *Changed* để người đọc lịch sử thấy |
| Nhánh 77 là một regex nữa trên mã nguồn — cùng họ với bài học ADR-0061 (regex không thắng được kẻ tấn công) | Trung bình | kẻ "tấn công" ở đây là rustfmt và một developer vô tình, không phải người cố ý; sàn + yêu cầu ≥ 1 call site mỗi khóa biến "không khớp" thành đỏ thay vì lặng im. Nêu rõ giới hạn này trong doc của nhánh |
| Hai bước 1–2 và 3 do cùng model làm nhưng là hai file khác — chạy song song được; nếu manager gộp thành một subagent thì context phình | Thấp | hai brief riêng, hai subagent, song song; bước 4 chờ 3 |

## Ngoài phạm vi

- **Thứ tự giữa các tài nguyên** trong `serve_sharded_hft_with` (socket → file log → thread).
  Mở file là *chiếm*, không phải *kiểm*; ADR-0064 nói về ranh giới kiểm/chiếm và dừng ở đó.
- **Chạy test theo từng bộ feature** (`cargo hack test --each-feature`) — ADR-0065 quyết định 3.
- **Nâng độ sâu quá 2**, hoặc liệt kê bộ bằng tay khi số feature tăng — chờ lỗi ba-feature
  đầu tiên, và ADR mới khi đó.
- **`hft` dưới TLS vẫn không có cổng nào kiểm** (`CLAUDE.md` §2 mục 4 đã ghi) — không liên quan.
- **Item 32 (a)**: `serve_sharded_hft` không dừng được — test mới không cần dừng vì bị từ chối
  trước khi chạy.
- **Sửa `CLAUDE.md`** — manager đang viết lại trên nhánh này; plan không đụng file đó.
- **Marker `[to testing-skills]`** trong bài `docs/reference/` — owner đang bỏ quy tắc đó; bài
  viết không mang marker.
- **Các cửa khác** (`serve`, `serve_hft`, `serve_tls`, initiator) — không có kiểm tra đọc-máy
  nào để sắp lại; ADR-0064 quyết định 1 áp cho chúng bằng lời, không bằng code trong plan này.

## Sửa đổi giữa lúc dựng

`CLAUDE.md` §1, hàng ba: plan sai giữa lúc dựng thì sửa plan, và sửa sao cho đọc thấy được.
Mỗi mục dưới đây ghi *plan viết gì*, *đo được gì*, *quyết định gì*.

### 2026-09-13 — Sàn của probe 7 là 4, không phải 7 (bước 2; kiến trúc sư quyết)

**Plan viết**: sàn 7 hàng, "đúng bằng probe 6 hôm nay". **Đo được** (senior dev, probe 7 viết đúng
như plan, chạy trên `docs/CONFIGURATION.md` §1 hiện tại): `probe 7 — bounded Values cells: 4
probed, 4 skipped, 25 not Numeric`. Bốn hàng được đọc: `HeartBtInt` (*non-negative*),
`LogonTimeout` và `LogoutTimeout` (`` `0` is off ``), `SocketConnectPort` (`` `0`–`65535` ``). Bốn
hàng bỏ qua vì ô chỉ ghi *integer*, không nêu bound: `MaxSkewMillis`, `TimestampPrecision`,
`ReconnectInterval`, `ReconnectCeiling`. R76-1 hoạt động đúng: đổi ô `HeartBtInt` về *positive
integer, seconds* → đỏ trên câu nêu `HeartBtInt`, probe 3 và 6 vẫn xanh.

**Số 7 là lỗi của plan**: nó là số hàng probe 6 đọc (mọi ô có chữ *integer*), không phải số hàng
*nêu bound* — hai câu hỏi khác nhau, và plan đã chép số của câu này sang câu kia.

**Quyết định: sàn 4. Không thêm bound vào bốn ô để đủ 7.** Lý do:

1. Sàn chỉ có một việc: phát hiện probe lặng lẽ không còn khớp gì nữa (probe 6 dùng sàn đúng
   như vậy). Sàn 4 làm được việc đó. Sàn không phải mục tiêu để tài liệu vươn tới.
2. Viết bound vào ô để một test đạt sàn là **chính cái bẫy của item 76, theo chiều ngược lại**:
   bài `a-documentation-cell-is-not-a-specification.md` nói ô mô tả không phải đặc tả; thêm bốn
   câu mô tả nữa là thêm bốn chỗ có thể sai — và ít nhất hai trong bốn *không thể* nhận bound mà
   probe 7 đọc được một cách trung thực: `TimestampPrecision` là một *liệt kê* (`` `3`, `6` or `9` ``,
   việc của probe 3, `settings.rs:1593` đọc bằng `integer_as_written` với `UnsupportedPrecision`),
   và `MaxSkewMillis` không có bound nào ngoài kiểu `u64` (`settings.rs:1525`,
   `with_max_skew_ms(mut self, ms: u64)` ở `session/src/lib.rs:916`) — ghi *non-negative* chỉ là
   nhắc lại rằng dấu trừ bị từ chối, điều probe 6 đã chứng minh bằng `+7`.
3. **Hai ô còn lại thì đúng là thiếu**, nhưng sửa chúng không thuộc bước 2 — xem mục *Phát hiện*
   ngay dưới. Mở rộng một bước đã đo xong để kéo một con số về là đổi brief giữa lúc dựng.

**Phát hiện kèm theo, ghi lại làm item mới ở bước 6, không làm trong bước 2**:
`ReconnectInterval=0` **bị parser từ chối** — `settings.rs:1641-1653` gọi
`crate::reconnect::Policy::new(first_ms, ceiling_ms)`, và `reconnect.rs:63-64` trả
`PolicyError::FirstIsZero`, thành `Problem::ImpossiblePolicy` (`settings.rs:328-330`: *"a zero
first delay, or a ceiling below it"*). `ReconnectCeiling=0` cũng bị từ chối, qua
`CeilingBelowFirst` (`reconnect.rs:67`), vì `first ≥ 1` luôn. Hai ô ở `docs/CONFIGURATION.md:93-94`
chỉ ghi *integer, seconds* — **ô nói ít hơn điều parser làm**, chiều ngược của item 76 (ô nói
nhiều hơn). Đóng nó cần hai việc, và việc thứ hai là lý do nó không nằm ở bước 2:
(a) ô `ReconnectInterval` → *positive integer, **seconds** — `0` is refused as `ImpossiblePolicy`,
a ladder needs a first step*; ô `ReconnectCeiling` → *positive integer, **seconds**, not below
`ReconnectInterval`*; (b) `is_about_the_value` (`settings.rs:2812-2820`) hiện **không** coi
`ImpossiblePolicy` là lỗi-về-giá-trị, nên probe 7 với ô *positive* sẽ đỏ *sai* — phải thêm nhánh
đó vào helper dùng chung của probe 3, và kiểm rằng probe 3 không đổi kết quả (không khóa liệt kê
nào sinh `ImpossiblePolicy`). Khi làm, sàn probe 7 lên **6**, và đây là cách sàn được nâng: vì tài
liệu nói thêm một sự thật, không phải vì test cần một con số.

### 2026-09-13 — Ba chỗ bước 0 và bước 1 lệch so với plan, ghi làm sự thật

- **Bước 1, `required(…)`**: plan liệt kê 25 call site dạng `hàm(…, Key::X)`. Trong parser còn
  **8 call site** `required(…, Key::X, …)` mà plan không đếm; hàm này chỉ kiểm *có mặt* hay không,
  không đọc giá trị, nên không có gì để so với `reader(key)`. Nhánh 77 **đếm riêng** nhóm này và
  không đối chiếu với khai báo nào; sàn 25 **không gồm** 8 dòng đó. Plan giữ 25.
- **Bước 1, turbofish**: cách quét của plan ("định danh ngay trước dấu ngoặc") không thấy được
  `number::<u8>(v, Key::X)` — tên hàm cách ngoặc bởi `::<u8>`. Nhánh 77 **chuẩn hoá** bằng cách bỏ
  phần turbofish ở đuôi trước khi đọc tên (`settings.rs:3098-3102`, có `[measured 2026-09-13]` ghi
  điều gì xảy ra nếu không làm). Plan đã bỏ sót dạng gọi này khi đếm.
- **Bước 0, số bộ feature**: plan nói 8 bộ cho `fixbolt-engine`. cargo-hack coi `default` là một
  feature có tên, nên liệt kê **10**: thêm `default,affinity` và `default,tls` (trùng nội dung với
  `standard,affinity` và `standard,tls`, nhưng cargo-hack không gộp vì tên khác). Số đếm dòng
  `info: running` trong *Cách kiểm chứng* và *Bẫy* đã sửa từ 8 thành 10. Không đổi ranh giới sâu 2.
- **Bước 0, 14 link hỏng**: lần chạy đầu của `cargo hack doc --feature-powerset --depth 2` đỏ ở
  **mọi bộ không có `standard`** — 14 intra-doc link trỏ tới `serve`, `serve_with`,
  `serve_with_recovery`, `connect_and_serve`, `StandardAcceptorEngine`, `poll::Poller`, là những
  thứ chỉ tồn tại khi `standard` bật. Đúng rủi ro hàng đầu của bảng *Rủi ro* ("bộ khác cũng đỏ"),
  ở nhánh "sửa nhỏ, không đổi hành vi": sửa trong commit `3404114`, chỉ đụng doc comment. Đây là
  cổng mới tìm ra lỗi trước cả khi lên CI — bằng chứng nó nhìn thấy đúng lớp nó được dựng để thấy.

### 2026-09-13 — Độ sâu 2 chưa bao giờ build "tất cả feature" (review D2)

**Plan viết**: độ sâu 2 gồm "mọi cặp + tất cả", hôm nay "vét hết (8/8 cho engine)", và vì thế hai
dòng `cargo doc` của job `docs` (mặc định và `--all-features`) được gộp vào rồi bỏ đi.

**Đo được**: log CI run 34750085195 liệt kê đúng 10 bộ cho `fixbolt-engine` — không feature,
`default`, `standard`, `affinity`, `tls`, `affinity+default`, `affinity+standard`, `affinity+tls`,
`default+tls`, `standard+tls`. **Không có bộ `standard,affinity,tls`.** Trên máy bàn,
`cargo hack check --workspace --feature-powerset --depth 2 --print-command-list` in ra đúng 10 bộ
đó cho `fixbolt-engine` và cho `tools/w2w`. Lý do: cargo-hack đếm độ sâu theo số feature có tên, và
"tất cả" của ba feature là ba tên — quá độ sâu 2 một bậc. Hệ quả: bỏ job `docs` là bỏ luôn lần build
rustdoc duy nhất dưới `--all-features` (chính bộ của item 61), trong khi mọi tài liệu nói nó vẫn còn.

**Quyết định**: giữ ranh giới sâu 2 (ADR-0065 quyết định 2 không đổi), và thêm hai bước riêng vào
cuối job `feature-sets`: `cargo clippy --workspace --all-targets --all-features -- -D warnings` và
`cargo doc --workspace --no-deps --all-features` với cùng `RUSTDOCFLAGS`. Sửa theo: comment
`ci.yml`, hàng `DESIGN.md` §6, ADR-0065 (quyết định 1, 5, mục *Good*, câu cuối *Context*, kèm một
mục *Revision* ghi ngày). Chưa chạy được trên máy bàn: các bộ có `tls` không build trên macOS, nên
hai bước mới chỉ quan sát được trên CI.

## Nhật ký giao hàng

*(trống — điền khi đóng từng bước: đã dựng gì, commit nào, gate nào xanh với output trích,
run id CI, cái gì chưa làm và vì sao)*
