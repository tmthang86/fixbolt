# Phase 4, hàng 3 (và quy trình đo của hàng 4): store SQLite

> **Loại:** Plan · **Ngày:** 2026-09-24 · **Trạng thái:** Đề xuất
> **Phạm vi:** phase 4, hàng 3 của bảng *Chia việc* trong
> [2026-09-23-phase-4-scope.md](2026-09-23-phase-4-scope.md), cộng quy trình đo của hàng 4 (hàng 4
> chạy ở một PR sau, xem mục *Hàng 4*); hạng mục 4 và vạch bỏ của
> [ADR-0098](../decisions/ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md);
> thiết kế ở ba ADR đề xuất cùng plan này:
> [ADR-0180](../decisions/ADR-0180-the-sqlite-store-is-the-async-journal-with-a-database-for-a-file-one-database-per-session-and-no-synchronous-mode.md)
> (hình dạng store),
> [ADR-0181](../decisions/ADR-0181-a-journal-outside-the-engine-joins-the-engines-writer-bookkeeping-through-three-public-handles.md)
> (ba "tay cầm" công khai engine phải mở ra),
> [ADR-0182](../decisions/ADR-0182-the-sqlite-store-is-born-publish-shaped-behind-a-default-feature-and-joins-the-lockstep-release-only-when-its-kill-line-passes.md)
> (feature, publish, lockstep).

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Anh chọn SQLite làm store dạng database cho phase 4 (câu hỏi 4 của plan phạm vi), và yêu cầu
vạch bỏ được viết trước code (câu hỏi 1). Vạch đó là: **luồng engine cấp phát 0 lần; độ trễ
wire p50 nằm trong band so với `FileJournal` `Async`; ghi 50 000 bản tin/giây suốt 60 giây mà
không rớt bản ghi nào.** Không đạt thì crate ở lại trong repo nhưng **không publish**.

Hôm nay engine có hai cách giữ bản tin đã gửi: `MemJournal` (chỉ trong RAM) và `FileJournal`
(RAM + file). Người dùng muốn tra lịch sử bằng SQL thì chưa có gì. Store SQLite là **`FileJournal`
`Async` đổi file thành database**: luồng engine làm y hệt như cũ (chép vào vòng nhớ, đẩy một bản
ghi vào hàng đợi cấp sẵn), còn một luồng ghi riêng gom nhiều bản ghi thành một transaction.

Kể từ khi ADR-0098 được viết, năm ADR đã đặt luật cho mọi journal có luồng ghi riêng: không lộ
mật khẩu xuống đĩa (ADR-0110), bộ đệm luồng ghi và tín hiệu dừng (ADR-0150), rời kết nối thì
"cho nghỉ" luồng ghi chứ không chờ (ADR-0153), một file chỉ một người ghi (ADR-0154), và
recovery hỏi chính luồng ghi xem đã buông file chưa (ADR-0155). Store mới phải giữ đủ năm luật
đó. Ba cơ chế trong số đó hiện là đồ riêng của crate engine — việc đầu tiên của plan này là mở
chúng ra (ADR-0181).

## Những gì đã biết chắc

**Đọc code ở `main` `094bfc3`, 2026-09-24:**

- Trait `Journal` (`crates/session/src/journal.rs:22-218`) có 12 hàm. `mark_in`, `mark_out` là
  **mốc cao nhất** — `MemJournal` giữ `max` (`crates/engine/src/journal.rs:247-263`) — nên gộp
  mỗi lô thành một giá trị lớn nhất là ghi đúng cái mà file journal đọc lại.
- `FileJournal::put` nhánh `Async` (`crates/engine/src/journal.rs:1186-1246`): chép vào
  `MemJournal`, đẩy `seq ‖ len ‖ bytes` vào `ring::Producer`; ring đầy thì `unwritten += 1` và
  `put` vẫn trả `true` (ADR-0154 quyết định 4). Ring của nó 1 MiB (`journal.rs:772`).
- `retire` (`journal.rs:1271-1291`) tăng biến đếm toàn tiến trình `RETIRED_WRITERS` **trước**
  khi báo cho luồng ghi; `wait_for_retired_writers` (`journal.rs:956-968`) chờ biến đó về 0 và
  được mọi `serve*` gọi sau vòng phục vụ. Biến đếm này, cờ `Released` (`journal.rs:1005`, trường
  riêng) và luật "nhàn thì ngủ" `ring::Idle` (`crates/engine/src/ring.rs:147-170`, `pub(crate)`)
  đều **không dùng được từ crate khác**.
- `ring::{pair, Producer, Consumer}`, `ring::DEFAULT_CAPACITY` (4 MiB) và
  `redact::{carries_secret, mask}` đã công khai.
- `Recovery::recover` chạy **trên luồng engine** trong các vòng `serve*` một engine
  (`crates/engine/src/recovery.rs:170-181`) — chi phí đã biết, ghi trong ADR-0154.
- `tools/w2w` đếm cấp phát trên **mọi luồng** trong cửa sổ đo (`tools/w2w/src/main.rs:302-308`),
  và có `--journal mem|file-async`. `scripts/w2w-baseline.sh` nhận `W2W_EXTRA` (dòng 143);
  `scripts/compare-w2w-procedures.sh` so hai bản tóm tắt theo ngưỡng 5 % của ADR-0068 quyết định 2.
- `scripts/bench.sh` chạy mọi bench target; target tên `alloc` được coi là bất biến
  (`bench.sh:53-57`), target khác phải có baseline — nên bài chạy 60 giây **không** được là một
  bench target.
- Ba script phát hành ghi cứng danh sách sáu crate (`check-release-versions.sh:50-57`,
  `check-packaged-build.sh:76`, `check-package-contents.sh:63`); luật 4 của
  `check-release-versions.sh` đòi mọi member khác là `publish = false`.

**Tra cứu trên mạng, 2026-09-24** (bảng đầy đủ ở ADR-0180 *Research*):

- SQLite: *"Transactions are durable across application crashes regardless of the synchronous
  setting"*; WAL + `synchronous=NORMAL` luôn nhất quán nhưng *"might roll back following a power
  loss or system crash"* — <https://sqlite.org/pragma.html#pragma_synchronous>. Tức là cùng hạng
  với `FileJournal` `Async`: sống qua tiến trình chết, không sống qua mất điện.
- `locking_mode=EXCLUSIVE`: lần ghi đầu lấy khoá ghi và **giữ tới khi đóng kết nối**; WAL chạy
  được ở chế độ này không cần shared memory — <https://sqlite.org/pragma.html#pragma_locking_mode>.
- **Bẫy của chính SQLite**: một `close()` bất kỳ trên file database trong cùng tiến trình xoá
  mọi khoá POSIX của SQLite, dẫn tới hai người cùng ghi và hỏng file; hai bản SQLite trong một
  tiến trình cũng vậy — <https://sqlite.org/howtocorrupt.html> §2.2, §2.3.
- WAL phình mãi nếu checkpoint không chạy được vì tranh khoá —
  <https://phiresky.github.io/blog/2020/sqlite-performance-tuning/>.
- rusqlite 0.40.2 (2026-08-08, MIT): feature `bundled` biên dịch SQLite 3.53.2 bằng `cc`; không
  khai `rust-version`, README nói MSRV là *"bản stable mới nhất lúc phát hành"*;
  `libsqlite3-sys` khai `links = "sqlite3"` — <https://github.com/rusqlite/rusqlite>, crates.io.
  `Connection` là `Send` không `Sync` — docs.rs.
- Chèn theo lô với prepared statement dùng lại: 100 triệu dòng trong 34,3 giây, **khi đã tắt
  bền vững** — <https://avi.im/blag/2021/fast-sqlite-inserts/>; không so được với cấu hình của mình.
- QuickFIX/J: bảng `sessions` (khoá 8 phần của session id, `incoming_seqnum`,
  `outgoing_seqnum`) và `messages` (session id + `msgseqnum`, `message`); QuickFIX C++
  `MySQLStore.cpp`: mỗi bản tin một `INSERT`, mỗi lần đổi số một `UPDATE sessions`, SQL ghép
  chuỗi. QFJ-119 / issue 357: hai lần I/O mỗi bản tin, không transaction — người dùng tự gom lô.
- **Tìm mà không thấy**: engine FIX nào lưu journal bằng SQLite; số đo công khai nào cho SQLite
  WAL + NORMAL với blob cỡ bản tin FIX.

**Đo thử, 2026-09-24** — chỉ để biết cỡ, **chưa phải con số** (crate thử không commit, máy
bàn đang ở dòng grub desktop, không phải §9): máy `tmt-B450-I-AORUS-PRO-WIFI`, kernel
`7.0.0-31-generic`, NVMe ext4; WAL + NORMAL + EXCLUSIVE, blob 200 byte, 512 dòng mỗi transaction,
một prepared statement giữ suốt: 3 000 000 dòng trong **5,640 s và 5,485 s** (~530 000
dòng/giây), database 648 MB. Kết nối thứ hai vào cùng file, `busy_timeout` 0 → `database is
locked` ngay lập tức. Crate thử build được trên Rust 1.88.0 và 1.98.0. Cây phụ thuộc: `bitflags`,
`fallible-iterator`, `fallible-streaming-iterator`, `smallvec`, `libsqlite3-sys` (+ `cc`,
`pkg-config`, `vcpkg`, `shlex`, `find-msvc-tools` lúc build) — không runtime async.

## Cách làm

Chi tiết và lý do ở ADR-0180/0181/0182; đây là phương án được chọn, tóm lại.

**A. Engine mở ba "tay cầm" (ADR-0181).** Trong `crates/engine/src/journal.rs`:
`WriterTicket` (vé của luồng ghi: `new`, `retire(stop_pushed) -> bool` trên luồng engine chỉ đụng
atomic, `state`, `finish` — hạ biến đếm đúng một lần), `Releaser` + `Released::pair()`. Trong
`ring.rs`: `Idle`, `IDLE_SPINS`, `IDLE_SLEEP` thành `pub`. `FileJournal` chuyển sang dùng chính
ba thứ này, để hai journal chạy chung một đoạn code. Chỉ thêm, không đổi chữ ký nào.

**B. Crate mới `crates/store-sqlite` (`fixbolt-store-sqlite`).**

- **Feature `sqlite`, bật mặc định**, chặn chính các `mod` và dependency `rusqlite`
  (`default-features = false, features = ["bundled"]`). `--no-default-features` → thư viện rỗng,
  **không biên dịch C** (bất biến 6).
- `SqliteJournal<const N, const LEN>` (+ `SqliteStore` = kích thước mặc định) cài đủ trait
  `Journal` như `FileJournal`. `SqliteJournal::open(path, &Config, SqliteOptions)` →
  `io::Result`. `SqliteOptions`: `synchronous` (`Normal` mặc định, `Full`), `ring_bytes` (mặc định
  4 MiB), `batch_max` (mặc định 4 096), `max_db_pages` (tuỳ chọn, trần dung lượng).
- **Mỗi session một file database.** Bảng `messages(seq INTEGER PRIMARY KEY, body BLOB)` ghi bằng
  `INSERT OR REPLACE`; bảng `session` một dòng: định danh (`begin_string`, `sender`, `target`),
  `highest_in`, `highest_out`, `last_active_ms`; `PRAGMA user_version = 1`. Mở file của session
  khác, hoặc phiên bản schema lạ → từ chối (`InvalidData`).
- **Luồng engine làm y hệt `FileJournal` `Async`**, và struct không chứa kiểu nào của `rusqlite`
  — `Connection` được mở trên luồng gọi `open` rồi **chuyển hẳn** vào luồng ghi.
- **Luồng ghi `fixbolt-sqlite`**: bộ đệm `8 + LEN` cấp một lần trước vòng lặp; gặp bản ghi đầu thì
  mở transaction; mỗi bản tin một `INSERT OR REPLACE` bằng statement giữ suốt; mark gộp thành giá
  trị lớn nhất (và `mark_active` lấy giá trị cuối); commit khi ring cạn hoặc đủ `batch_max`; cạn
  thì chờ theo `ring::Idle`. Bản tin mang bí mật (`redact::carries_secret`) → **không** chèn,
  chỉ nâng `highest_out` (ADR-0110 quyết định 4). Lô lỗi → rollback, cộng số bản ghi vào bộ đếm
  chung mà `unwritten()` cộng thêm. Dừng khi gặp `STOP`: commit, `wal_checkpoint(TRUNCATE)`,
  **đóng Connection**, rồi `Releaser::release`, rồi `WriterTicket::finish`.
- **Một người ghi = khoá của chính SQLite**: `open` đặt `journal_mode=WAL`, `synchronous`,
  `locking_mode=EXCLUSIVE`, `busy_timeout=0`, chạy một `BEGIN IMMEDIATE … COMMIT`; bận →
  `WouldBlock` ngay. Không chỗ nào trong crate mở file database ngoài SQLite.
- **Mở lại** nạp dòng `session` và `N` bản tin mới nhất vào `MemJournal`.
- `released()` (cho `Recovery::ready`), `progress()` (số bản ghi đã commit, số outbound cao nhất
  đã commit), `close()`.
- Manifest đủ khuôn publish của ADR-0160 nhưng **`publish = false`** tới khi hàng 4 cho kết quả
  đạt (ADR-0182).

**C. Công cụ đo cho hàng 4, dựng luôn ở hàng này**: `examples/soak.rs` (chạy 50 000 msg/s × N
giây, tự kiểm từng dòng) và nhánh `--journal sqlite-async` trong `tools/w2w` sau feature `sqlite`.

**File sẽ tạo hoặc sửa:** `Cargo.toml` (members); `crates/engine/src/{journal.rs, ring.rs}`,
`crates/engine/tests/writer_hooks.rs` (mới); `crates/store-sqlite/{Cargo.toml, README.md,
LICENSE-MIT, LICENSE-APACHE, src/lib.rs, src/journal.rs, src/writer.rs, src/schema.rs,
tests/store.rs, tests/crash.rs, tests/serve.rs, benches/alloc.rs, examples/soak.rs}` (mới hết);
`tools/w2w/{Cargo.toml, src/main.rs}`; `scripts/check-no-optional-deps.sh`;
`.github/workflows/ci.yml` (một bước soak ngắn); tài liệu ở mục *Tài liệu phải cập nhật*.

## Bất biến bị đụng tới

- **1 (không cấp phát trên hot path)**: đường của luồng engine là `MemJournal::put` + một lần
  đẩy ring — cùng đường `FileJournal` `Async` đã được `benches/alloc.rs` chứng minh 0. Store có
  `benches/alloc.rs` riêng, **đếm theo luồng** (chỉ luồng engine), năm case, mỗi case tự chứng
  minh đường của nó có chạy. Bộ đếm Rust không nhìn thấy heap C của SQLite — không sao, vì luồng
  engine không bao giờ gọi vào SQLite (struct không chứa kiểu `rusqlite`; reviewer kiểm bằng tay).
- **2 (session thuần)**: không đụng `crates/session`.
- **3 (59 / 59)**: không đụng session; `cargo test --all` vẫn chạy corpus.
- **4 (ngủ/spin theo mode)**: luồng ghi không phải luồng engine; nó ngủ theo `ring::Idle` ở mọi
  mode, luồng engine không bao giờ đánh thức nó. `retire` trên luồng engine chỉ đụng atomic.
  Ba script mode chạy với `W2W_EXTRA="--journal sqlite-async"`, và vẫn phải đỏ khi đổi sai mode.
- **5**: không đổi.
- **6 (feature chặn `mod`)**: feature `sqlite` chặn chính `mod`; `--no-default-features` không
  biên dịch C; `check-no-optional-deps.sh` thêm `fixbolt-store-sqlite:rusqlite`,
  `fixbolt-store-sqlite:libsqlite3-sys`, `fixbolt-w2w:rusqlite`.
- **7 (không panic)**: crate thư viện, lint workspace; nợ `indexing_slicing` bắt đầu ở 0.
- **8 (`unsafe`)**: crate không có `unsafe` (`#![forbid(unsafe_code)]` ở gốc); bench alloc có
  `unsafe impl GlobalAlloc` như bốn bench alloc đã có, chứng minh bằng đảo ngược.
- **9**: không đụng.
- **10 (số đo)**: số của hàng 4 kèm lệnh, máy, output `check-machine.sh`; số đo thử ở trên đã
  gắn nhãn "chưa phải con số".

## Chia việc

Bước 1–6 do **một** senior developer (opus) làm liên tục (bước sau gửi tiếp bằng `SendMessage`
cho cùng agent) vì chúng dính engine và dữ liệu trên đĩa. Bước 7 (tài liệu) nằm **cùng commit**
với code (`CLAUDE.md` §4). Manager chạy lại gate và commit sau mỗi bước xanh; developer không
commit. Cột *Gate* là lệnh manager chạy lại.

| Bước | Kết quả | Người làm | File được sửa / không được sửa | Gate | Phụ thuộc |
|---|---|---|---|---|---|
| 1 | **Ba tay cầm của engine (ADR-0181).** Test đỏ trước: `crates/engine/tests/writer_hooks.rs` — `a_ticket_retired_twice_is_counted_once`, `finish_without_retire_changes_no_count`, `wait_for_retired_writers_waits_for_a_ticket_finished_on_another_thread` (luồng phụ `finish` sau 50 ms: `wait` trả `true`, đã trôi ≥ 50 ms), `released_turns_true_only_when_its_releaser_releases`, `idle_is_reachable_from_outside_the_crate`; các test trong file tự tuần tự hoá bằng một `Mutex` tĩnh (biến đếm là toàn tiến trình). Đỏ = lỗi biên dịch, trích nguyên văn. Rồi code: `WriterTicket`, `Releaser`, `Released::pair`, `ring::Idle` công khai; `FileJournal` chuyển sang dùng chúng | senior developer (opus) | Sửa: `crates/engine/src/{journal.rs, ring.rs}`, `crates/engine/tests/writer_hooks.rs` (mới), `CHANGELOG.md`. **Không** sửa: file nào khác trong `crates/engine/src`, mọi test/bench có sẵn, `crates/session/` | `cargo test -p fixbolt-engine --test writer_hooks`; **không sửa mà vẫn xanh**: `cargo test -p fixbolt-engine --test one_appender --test after_serving --test retire --test writer_idle --test journal --test on_disk --test engine_recovery --test secrets_stay_off_disk`; `cargo bench -p fixbolt-engine --bench alloc` (case `retire` 0 và có chạy); `cargo test --all`; `cargo test --no-default-features`; `cargo clippy --all-targets -- -D warnings`; `cargo fmt --check`; `cargo semver-checks -p fixbolt-engine --baseline-rev origin/main` (không break); `scripts/check-indexing-debt.sh`; `scripts/check-no-crate-root-allow.sh`. Đảo ngược: R1 `finish` hạ đếm không qua CAS → đỏ ở `a_ticket_retired_twice_is_counted_once`/`finish_without_retire…`; R2 `retire` không tăng đếm → đỏ ở `wait_for_retired_writers_waits…` | duyệt plan + ba ADR |
| 2 | **Khung crate + test đỏ.** Manifest đủ khuôn ADR-0160 với `publish = false` và feature `sqlite` (ADR-0182); `src/lib.rs` có kiểu và hàm công khai, thân hàm trả lỗi `Unsupported` (không `panic`, không `todo!`) để test biên dịch được và đỏ ở khẳng định. Test (mọi file mở đầu `#![cfg(feature = "sqlite")]`): `tests/store.rs` — `a_reopened_store_answers_what_it_was_told`, `a_reused_number_keeps_the_newest_bytes`, `reopen_loads_only_the_last_n_into_the_ring`, `a_second_open_of_a_held_database_is_refused_at_once` (`WouldBlock`, < 100 ms), `a_database_of_another_session_is_refused`, `a_retired_writer_commits_everything_then_releases_then_is_counted_done`, `a_message_carrying_a_secret_leaves_only_its_number` (đọc bytes của `.db` **và** `-wal` sau khi đóng; tiền đề dương: bản tin `35=8` có trong file), `a_record_larger_than_the_ring_is_counted_and_put_still_answers_true`, `a_failed_commit_is_counted_in_unwritten` (dùng `max_db_pages` nhỏ → `SQLITE_FULL`), `the_writer_sleeps_when_idle` (theo cách `crates/engine/tests/writer_idle.rs` làm); `tests/crash.rs` — `a_killed_process_resumes_from_what_it_committed` (chạy lại chính binary test làm tiến trình con qua biến môi trường, con in `committed <n>` mỗi khi `progress()` tăng, cha `Child::kill()` (SIGKILL) khi ≥ 5 000, mở lại: `highest_out` ≥ số cuối đã in, mọi `seq` từ 1 tới `highest_out` có mặt và đúng từng byte, `PRAGMA integrity_check` = `ok`, `Resumed::from_journal` cho `next_out = highest_out + 1`); `tests/serve.rs` — qua socket với `serve_with_recovery`, chép khuôn `crates/engine/tests/on_disk.rs:470-520` và `secrets_stay_off_disk.rs:430-470`: `a_session_resumed_from_the_database_replays_what_it_sent` (lần chạy hai: `ResendRequest` nhận lại `43=Y` đúng byte), `secrets_through_a_real_logon_stay_out_of_the_database`, `a_reconnect_while_the_writer_flushes_is_parked_then_admitted` (khuôn `one_appender.rs:362`, `ready` trả lời bằng `released()`) | senior developer (opus), cùng agent | Tạo: `crates/store-sqlite/{Cargo.toml, README.md, LICENSE-MIT, LICENSE-APACHE, src/lib.rs, tests/store.rs, tests/crash.rs, tests/serve.rs}`; sửa: `Cargo.toml` (members), `scripts/check-no-optional-deps.sh`. **Không** sửa: `crates/engine/`, `crates/session/`, `.github/` | `cargo test -p fixbolt-store-sqlite` **đỏ**, trích nguyên văn, câu FAIL mong đợi viết trước (mục *Cách kiểm chứng* #1); `cargo build -p fixbolt-store-sqlite --no-default-features` xanh; `scripts/check-no-optional-deps.sh` xanh; `cargo test --all --no-default-features` không biên dịch `libsqlite3-sys` (grep output `Compiling libsqlite3-sys` → không có) | 1 |
| 3 | **Code store (ADR-0180).** `src/journal.rs` (`SqliteJournal`, cài `Journal`, `open`, `released`, `progress`, `close`, `Drop`), `src/writer.rs` (vòng ghi, gom lô, mark gộp, bí mật, lỗi lô, dừng/đóng/release/finish), `src/schema.rs` (pragma, tạo/kiểm schema và định danh, nạp lại). Struct `SqliteJournal` không chứa kiểu `rusqlite`. Đảo ngược, mỗi cái viết câu FAIL trước: R1 bỏ nhánh bí mật trong writer → đỏ ở `a_message_carrying_a_secret…` và `secrets_through_a_real_logon…`; R2 `busy_timeout` 5000 → đỏ ở `a_second_open…` (thời gian); R3 bỏ kiểm định danh → đỏ ở `a_database_of_another_session…`; R4 `INSERT` thay `INSERT OR REPLACE` → đỏ ở `a_reused_number…`; R5 `retire` không lấy vé → đỏ ở `a_retired_writer_commits_everything…` | senior developer (opus), cùng agent | Tạo/sửa: `crates/store-sqlite/src/{lib.rs, journal.rs, writer.rs, schema.rs}`. **Không** sửa test của bước 2 (sửa test cho xanh là thất bại), `crates/engine/`, `crates/session/` | `cargo test -p fixbolt-store-sqlite` xanh, chạy 3 lần liền (crash và serve không được chập chờn); `cargo test --all`; `cargo test --no-default-features`; `cargo clippy --all-targets -- -D warnings`; `cargo clippy -p fixbolt-store-sqlite --no-default-features --all-targets -- -D warnings`; `cargo fmt --check`; `scripts/check-indexing-debt.sh`; `scripts/check-no-crate-root-allow.sh`; `scripts/check-lint-config.sh`; `scripts/check-no-optional-deps.sh`; R1–R5 đỏ đúng chỗ rồi xanh lại | 2 |
| 4 | **Case alloc.** `crates/store-sqlite/benches/alloc.rs` (`harness = false`): bộ cấp phát đếm **theo luồng** (cờ `thread_local!` khởi tạo `const`, chỉ luồng đang đo được đếm) — tự kiểm ngay trong bench: cấp phát trên luồng khác trong cửa sổ → 0, trên luồng này → 1. Năm case: `sqlite-put`, `sqlite-mark-in`, `sqlite-mark-out`, `sqlite-mark-active`, `sqlite-retire`; mỗi case có khẳng định "đường có chạy" (dòng đã vào database / `writers_retired()` tăng). In một dòng `allocations: sqlite-put 0 …`. Đảo ngược R6: `std::hint::black_box(Vec::<u8>::with_capacity(1))` trong `put` → `sqlite-put` > 0 | senior developer (opus), cùng agent | Tạo: `crates/store-sqlite/benches/alloc.rs`; sửa: `crates/store-sqlite/Cargo.toml` (`[[bench]]`). Không sửa gì khác | `cargo bench -p fixbolt-store-sqlite --bench alloc` (năm số 0, trích dòng `allocations:`); `scripts/bench.sh` liệt kê và chạy target mới (trích dòng của nó); R6 đỏ rồi xanh | 3 |
| 5 | **Công cụ soak (thước của vạch 50 000 × 60).** `crates/store-sqlite/examples/soak.rs` (`required-features = ["sqlite"]`): `--rate --seconds --dir --synchronous normal\|full --body-bytes 200`; **từ chối thư mục trên tmpfs** (đọc `/proc/self/mounts`); một luồng "giả luồng engine" giữ nhịp bằng spin theo `Instant` (không ngủ), mỗi bản tin `mark_in(k)` + `put(k, body(k))`; một luồng theo dõi đọc `progress()` mỗi 10 ms; cuối: `close()`, mở lại bằng `rusqlite`, đếm và so **từng dòng** với bộ sinh. In một dòng: `soak: synchronous … rate … seconds … messages … records … unwritten … rows … mismatched … behind_max_ms … max_batch … wal_bytes_max … db_bytes … pace_missed …`; thoát khác 0 nếu `unwritten > 0`, `rows ≠ messages`, `mismatched > 0` hoặc luồng giả engine hụt nhịp quá 1 %. Thêm một bước trong job `gates` của CI: `--rate 50000 --seconds 2 --dir target/soak`, ghi rõ là **đếm, không phải số đo** | senior developer (opus), cùng agent | Tạo: `crates/store-sqlite/examples/soak.rs`; sửa: `crates/store-sqlite/Cargo.toml` (`[[example]]`), `.github/workflows/ci.yml` (một bước). Không sửa gì khác | `cargo run --release -p fixbolt-store-sqlite --example soak -- --rate 50000 --seconds 5 --dir target/soak` thoát 0, trích dòng `soak:`; cùng lệnh với `--dir /tmp/x` bị từ chối; đảo ngược R7: bỏ một dòng khi kiểm (bỏ qua `seq` 1000 lúc so) → `mismatched`/`rows` bắt được, thoát ≠ 0 | 4 |
| 6 | **Nhánh w2w + ba script mode.** `tools/w2w`: feature `sqlite = ["dep:fixbolt-store-sqlite"]` (không trong `default`), `--journal sqlite-async` sau `#[cfg(feature = "sqlite")]`, mở `SqliteStore` như `--journal file-async` mở `FileStore`. Ở nhánh này cửa sổ đếm cấp phát chỉ đếm các luồng tự đăng ký (engine, client, observer), nhãn dòng `allocs` nói rõ và nói heap C của SQLite không được đếm; các nhánh cũ **giữ nguyên** cách đếm | senior developer (opus), cùng agent | Sửa: `tools/w2w/{Cargo.toml, src/main.rs}`, `scripts/check-no-optional-deps.sh` (dòng `fixbolt-w2w:rusqlite`). **Không** sửa ba script mode, `crates/` | `cargo build --release -p fixbolt-w2w --features sqlite`; `target/release/w2w --mode hft --journal sqlite-async` và `--mode standard …` (loopback) in dòng allocs 0; `W2W_EXTRA="--journal sqlite-async" scripts/check-no-kernel-sleep.sh`, `… scripts/check-standard-gives-the-core-back.sh`, `… scripts/check-no-kernel-sleep-by-ctxt.sh` — mỗi script xanh ở mode đúng **và** đỏ ở mode sai như nó vốn làm; `cargo test --all --no-default-features` vẫn không biên dịch `libsqlite3-sys`; `scripts/check-no-optional-deps.sh` | 5 |
| 7 | **Tài liệu**, cùng commit với code: danh sách ở mục *Tài liệu phải cập nhật*; trang `docs/reference/` cho mỗi bất ngờ bước 1–6 báo về; ba ADR → `Accepted` (manager ghi dòng trạng thái) | developer (sonnet) | Chỉ các file `docs/`, `README.md`, `CHANGELOG.md`, `crates/store-sqlite/README.md` được liệt kê. **Không** sửa `crates/*/src`, `STATUS.md` (manager viết) | `python3 scripts/check-links.py` xanh | 6 |
| 8 | **Review + CI**: một senior review, context mới, được đưa plan này, ba ADR và các gate; manager kiểm từng phát hiện theo `CLAUDE.md` §12; PR nháp mở từ commit đầu; CI xanh trên commit đóng, run id ghi vào *Nhật ký giao hàng*; `STATUS.md` ghi hàng 3 xong, hàng 4 đang chờ | senior developer (opus) review; manager | — | CI xanh trên commit đóng, run id ghi lại | 7 |

## Hàng 4 — quy trình đo (PR sau, không thuộc PR này)

Làm ở một PR riêng, nhánh `measure/p4-sqlite-store-verdict`. **Manager chạy, runner (haiku)
trích output, developer (sonnet) sửa manifest/script khi lật cờ.** Mọi lần chạy kèm output
`scripts/check-machine.sh` ngay trước đó; không có LM Studio/llama-server; lần chạy đầu sau
reboot bỏ đi.

**4a. 50 000 msg/s × 60 s — trên máy bàn, dòng grub nào cũng được, ghi rõ dòng nào.**

```
scripts/check-machine.sh
cargo build --release -p fixbolt-store-sqlite --example soak
target/release/examples/soak --rate 50000 --seconds 60 --synchronous normal --dir target/soak   # lần 1
target/release/examples/soak --rate 50000 --seconds 60 --synchronous normal --dir target/soak   # lần 2
target/release/examples/soak --rate 50000 --seconds 60 --synchronous full   --dir target/soak   # chỉ ghi lại
```

Đạt khi **cả hai** lần `normal` thoát 0 với `unwritten 0`, `rows 3000000`, `mismatched 0`,
`pace_missed` ≤ 1 %. Lần `full` chỉ được ghi vào `measured-costs.md`, không phải vạch. Mỗi lần
ghi ~650 MB; xoá `target/soak` sau mỗi lần (ổ đang đầy 88 %).

**4b. Cặp `w2w` — trong một boot §9, đi cùng boot của hàng 7** (build sẵn cả hai nhánh trước
boot theo ADR-0090, binary `--features sqlite`). Đường `app` (chỉ bản tin ứng dụng mới vào
journal), cả `hft` lẫn `standard`, NIC có hardware timestamp như con số công bố, hai procedure
(ADR-0068):

```
W2W_EXTRA="--journal file-async"   ARMS="hft:app standard:app" scripts/w2w-baseline.sh   # procedure 1, nhánh đối chứng
W2W_EXTRA="--journal sqlite-async" ARMS="hft:app standard:app" scripts/w2w-baseline.sh   # procedure 1, nhánh store
# lặp lại cho procedure 2
scripts/compare-w2w-procedures.sh <summary file-async> <summary sqlite-async>
```

Đạt khi, ở **cả hai mode và cả hai procedure**, `wire p50` của nhánh store lệch không quá 5 %
so với nhánh `file-async` (ngưỡng ADR-0068 quyết định 2); dòng allocs của luồng engine bằng 0.
p99 và p99.9 được ghi lại, không phải vạch (ADR-0098 chỉ đặt vạch ở p50).

**4c. Áp phán quyết.** Đạt cả 4a, 4b và bước 4 của plan này (alloc 0) → ADR-0182 quyết định 3:
bỏ `publish = false`, thêm tên vào ba danh sách script, thêm case build `sqlite` vào
`check-packaged-build.sh`, ghi số vào `measured-costs.md`, ADR-0180 ghi kết quả. Hụt bất kỳ vạch
nào → ADR-0182 quyết định 4: giữ `publish = false`, cặp số vào `measured-costs.md`, ADR-0180
ghi kết quả. Cả hai đều là "xong" (câu hỏi 1).

## Cách kiểm chứng

| # | Tiêu chí | Lệnh | Coi là đạt |
|---|---|---|---|
| 1 | Đỏ trên code chưa viết | bước 1: `cargo test -p fixbolt-engine --test writer_hooks`; bước 2: `cargo test -p fixbolt-store-sqlite` | bước 1: lỗi biên dịch `cannot find type WriterTicket`; bước 2: mọi test FAIL ở `open` với lỗi `Unsupported` — trừ các khẳng định phủ định chạy trước `open` (không có); câu FAIL ghi trước khi chạy |
| 2 | Store nhớ đúng, khôi phục đúng | `cargo test -p fixbolt-store-sqlite` | xanh 3 lần liền; `crash` cho thấy `highest_out` ≥ số đã commit và mọi dòng đúng byte |
| 3 | Mật khẩu không xuống đĩa | `a_message_carrying_a_secret…`, `secrets_through_a_real_logon…` | không byte bí mật nào trong `.db`/`-wal`; tiền đề dương xanh; gửi lại trong tiến trình vẫn có `554=` nguyên văn |
| 4 | Một người ghi, nghỉ không chờ, recovery hỏi luồng ghi | `a_second_open…`, `a_retired_writer…`, `a_reconnect_while_the_writer_flushes…` | `WouldBlock` < 100 ms; `wait_for_retired_writers` `true`, `is_released()` `true`, mở lại được; kết nối bị giữ rồi được nhận |
| 5 | Luồng engine không cấp phát | `cargo bench -p fixbolt-store-sqlite --bench alloc` | `allocations: sqlite-put 0 sqlite-mark-in 0 sqlite-mark-out 0 sqlite-mark-active 0 sqlite-retire 0`; R6 làm `sqlite-put` > 0 |
| 6 | Mặc định không build thêm gì | `cargo build --workspace --no-default-features`; `scripts/check-no-optional-deps.sh` | xanh; không dòng `Compiling libsqlite3-sys` |
| 7 | Mode không bị phá | ba script mode với `W2W_EXTRA="--journal sqlite-async"` | xanh ở mode đúng, đỏ ở mode sai |
| 8 | Engine không đổi hành vi | tám binary test engine ở bước 1, không sửa; `cargo semver-checks` | xanh; không break |
| 9 | Không phá gì đã có | `cargo test --all`, `cargo test --no-default-features` | xanh; 59 / 59 vẫn nằm trong `cargo test --all` |
| 10 | Soak tự bắt lỗi | bước 5, R7 | thoát ≠ 0 khi thiếu một dòng |

"Test pass" chưa đủ ở đây vì vạch bỏ là số đo: hàng 4 mới là nơi ba vạch được áp.

## Tài liệu phải cập nhật

Theo bảng đồng bộ ở `CLAUDE.md` §4, đi từng dòng:

- [ ] `DESIGN.md` §3 (crate mới), D7 (thêm dòng `SqliteJournal` vào bảng chính sách, và đoạn
      "một database mỗi session, không chế độ đồng bộ")
- [ ] `README.md` layout; `Cargo.toml` members; `docs/internals/store-sqlite.md` (mới: file
      nào giữ gì, thứ tự đọc, test canh từng phần); `docs/internals/engine.md` (ba tay cầm)
- [ ] `docs/GUIDE.md` — store không có chế độ "ghi xong mới gửi"; `Normal` không sống qua mất
      điện; mỗi session một file; **không mở file database bằng `std::fs` khi store đang chạy**
      (sao lưu bằng backup API của SQLite hoặc sau `close`); một bản SQLite mỗi tiến trình;
      `fresh()` phải dùng đường dẫn chưa có lịch sử; journal tự viết nên dùng `WriterTicket`
- [ ] `docs/CONFIGURATION.md` — feature `sqlite`, `SqliteOptions` (`synchronous`, `ring_bytes`,
      `batch_max`, `max_db_pages`), `--journal sqlite-async` của w2w
- [ ] `docs/SESSION-BEHAVIOUR.md` — không đổi hành vi session; ghi một dòng "sau khởi động lại,
      bản tin mang bí mật được lấp khoảng trống", trỏ test
- [ ] `docs/best-practices-standard.md`, `docs/best-practices-hft.md` — store chỉ thêm việc cho
      luồng ghi; ghim lõi cho luồng ghi là ngoài phạm vi
- [ ] `CHANGELOG.md` — bốn item công khai mới của engine; crate mới (chưa publish)
- [ ] `docs/reference/` — mỗi bất ngờ gặp trong lúc dựng, kèm test canh
- [ ] `docs/reference/measured-costs.md` — **hàng 4**, không phải hàng này
- [ ] `STATUS.md` — manager viết khi hàng đóng

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| `serve*` trả về trong khi luồng ghi SQLite còn giữ bản ghi chưa commit → tắt bình thường vẫn mất dữ liệu | `a_retired_writer_commits_everything…` qua `wait_for_retired_writers`; R5 |
| Hạ biến đếm hai lần, hoặc hạ khi chưa tăng → `wait_for_retired_writers` chạy tới timeout mỗi lần tắt | `a_ticket_retired_twice…`, `finish_without_retire…`; R1 |
| Một `close()` trên file database trong cùng tiến trình xoá khoá của SQLite (howtocorrupt §2.2) → hai người ghi, hỏng file | không có code nào trong crate mở file ngoài SQLite (reviewer grep `File::open`/`fs::read` trong `src/`); test đọc bytes file chỉ **sau** `close`; `GUIDE.md` |
| Hai bản SQLite trong một tiến trình (§2.3), hoặc `links = "sqlite3"` đụng một `libsqlite3-sys` khác | không test được bằng code; `GUIDE.md` và ADR-0180 *Consequences* |
| Luồng engine gọi vào SQLite (heap C, bộ đếm Rust không thấy) | struct `SqliteJournal` không chứa kiểu `rusqlite` — reviewer kiểm; case alloc chỉ đo các lời gọi của luồng engine |
| Bộ đếm cấp phát toàn tiến trình đếm luôn luồng ghi → đỏ giả | bench đếm theo luồng, tự kiểm "luồng khác → 0, luồng này → 1"; nhánh w2w đếm luồng đăng ký |
| Checkpoint WAL (fsync) làm luồng ghi đứng lâu → ring đầy → rớt bản ghi | ring mặc định 4 MiB; soak in `behind_max_ms`, `unwritten`; vạch 4a |
| WAL phình mãi | một kết nối, không người đọc → auto-checkpoint luôn chạy; soak in `wal_bytes_max`; `TRUNCATE` khi đóng |
| Số bị dùng lại sau `SetNextOut` → bản cũ thắng | `a_reused_number_keeps_the_newest_bytes`; R4 |
| Mở nhầm database của session khác → nối tiếp sai số | `a_database_of_another_session_is_refused`; R3 |
| Mở lần hai chờ khoá thay vì từ chối → luồng engine ngủ trong `recover` | `a_second_open…` (< 100 ms); R2 |
| Bí mật lọt xuống `.db` hoặc `-wal` | hai test bí mật đọc cả hai file; R1 |
| Test crash chập chờn (tiến trình con, thời điểm kill) | chạy 3 lần liền ở bước 3; khẳng định chỉ dùng số con đã in, không dùng thời gian |
| `--no-default-features` làm test biên dịch thành rỗng rồi "xanh" | `cargo test --all` (feature bật) chạy chúng; bước 2 trích tên từng test đã chạy |
| C bị biên dịch trong job `no-default-features` | `check-no-optional-deps.sh` ba dòng mới; grep `Compiling libsqlite3-sys` |
| MSRV của rusqlite tăng lặng lẽ khi `cargo update` | chưa publish thì vô hại; khi lật cờ, job `package` build trên `rust-version` (ADR-0182 quyết định 3) |
| Soak chạy trên tmpfs (`/tmp` của máy bàn là tmpfs) → số vô nghĩa | soak từ chối tmpfs; R-kiểm ở bước 5 |
| Build lại giữa boot đo làm layout đổi | hàng 4b build sẵn trước boot (ADR-0090) |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| 50 000 × 60 hụt vì checkpoint đứng quá lâu trên máy bàn | thấp (đo thử ~530 000 dòng/giây, gấp ~10 lần) | tăng `ring_bytes` là một setting, không phải thiết kế lại; nếu vẫn hụt → vạch bỏ áp dụng |
| wire p50 lệch > 5 % dù luồng engine làm y hệt | thấp | cặp đo cho biết; nếu lệch thì đó là tranh chấp cache/bộ nhớ của luồng ghi — ghi vào `measured-costs.md`, vạch áp dụng |
| Đổi `FileJournal` sang ba tay cầm làm vỡ một hành vi mà test không thấy | trung bình | tám binary test không sửa + case `retire` + review senior; ADR-0181 *Consequences* |
| Thêm C vào mọi build mặc định làm CI chậm hơn | chắc chắn, nhỏ | chấp nhận (ADR-0182 *Consequences*) |
| Hàng 4b phải chờ boot §9 của hàng 7 | chắc chắn | crate ở trạng thái `publish = false` tới lúc đó; không chặn hàng nào khác |

## Ngoài phạm vi

- Postgres, MySQL, ODBC — anh chọn SQLite, không mở ADR Postgres.
- Một database dùng chung cho nhiều session (ADR-0180 *Options not taken*).
- Chế độ "ghi xong database mới gửi" — không bao giờ (bất biến 4).
- Dọn dòng cũ, giới hạn thời gian giữ lịch sử.
- Ghim lõi cho luồng ghi SQLite (`FileJournal::open_pinned` có; store chưa).
- Recovery dựng sẵn trong crate: người dùng tự viết như với `FileJournal`; test và rustdoc cho
  mẫu.
- Feature link SQLite của hệ thống, SQLCipher.
- Đưa store vào crate `fixbolt` (facade).
- Đo hàng 4 — plan này chỉ dựng thước và viết quy trình.

## Nhật ký giao hàng

*(trống — điền khi từng bước đóng)*
