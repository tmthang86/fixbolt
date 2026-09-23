# Phase 3, bước 3: mật khẩu không được xuống đĩa

> **Loại:** Plan · **Ngày:** 2026-09-23 · **Trạng thái:** Đã duyệt (manager, 2026-09-23, theo mandate thường trực của owner)
> **Phạm vi:** phase 3, dòng 3 của bảng *Chia việc* trong
> [2026-09-23-phase-3-scope.md](2026-09-23-phase-3-scope.md); tiêu chí thoát số 4 của
> [ADR-0097](../decisions/ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md);
> quyết định thiết kế ở [ADR-0110](../decisions/ADR-0110-a-secret-is-masked-in-the-message-log-and-leaves-only-its-number-in-the-journal-file.md) (Proposed)

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Khi đối tác đăng nhập, bản tin `Logon` của họ có thể mang `554=` (mật khẩu) và `96=` (dữ liệu xác
thực thô). Engine có hai chỗ ghi ra đĩa:

- **message log** (`FileLog`) — ghi mọi bản tin hai chiều, để trả lời câu hỏi "lúc 10:32 ta nhận
  gì, gửi gì";
- **journal** (`FileJournal`) — ghi bản tin ứng dụng ta đã gửi, để gửi lại khi đối tác xin
  (`ResendRequest`) và để khôi phục sau khi khởi động lại.

Plan phạm vi phase 3 ghi: *grep không thấy chỗ nào che mật khẩu, chưa chạy thử.* Một người lạ đem
engine này đi chạy sẽ có mật khẩu của đối tác nằm trơ trong file log. Đó là lỗi loại CWE-532, và
các engine cùng nhóm đã xử lý nó (Artio, OnixS). Việc này làm cho tiêu chí 4 của ADR-0097 thành
một lệnh chạy được: *không mật khẩu nào xuống đĩa, chứng minh bằng test đỏ khi bỏ che.*

## Những gì đã biết chắc

Đọc code ở worktree này, commit `213e73c`, ngày 2026-09-23:

- **Message log ghi nguyên văn cả hai chiều.** `Conn` gọi `MessageLog::record` với mọi frame đọc
  được, trước khi session xét (`crates/engine/src/conn.rs:448`), và với mọi frame đưa vào hàng gửi
  (`conn.rs:840`). Luồng ghi `msglog::write_loop` (`crates/engine/src/msglog.rs:404-497`) chỉ
  escape `\`, `\n`, `\r`; mọi byte khác ghi như nhận. Luồng ghi này **không phải luồng engine**
  và được phép cấp phát (ADR-0037), nhưng có bộ đệm `buf` riêng, sửa tại chỗ được.
- **Journal chỉ giữ bản tin ứng dụng ta gửi đi.** Session gọi `Journal::put` ở
  `crates/session/src/lib.rs:3083` và `:3890`; chú thích ở đó: *chỉ bản tin ứng dụng được giữ,
  QuickFIX không bao giờ gửi lại bản tin quản trị mà lấp khoảng trống.* `Logon` không bao giờ vào
  journal. **Vậy `554=` trong `Logon` hôm nay không thể vào journal.**
- **Nhưng bản tin ứng dụng có thể mang mật khẩu.** Theo từ điển QuickFIX trong `vendor/`
  (`FIX44.xml`, `FIXT11.xml`, `FIX50SP2.xml`), `UserRequest` (`35=BE`) mang `554` Password,
  `925` NewPassword, `96` RawData; bản FIX 5.0 SP2 thêm `1402` EncryptedPassword và `1404`
  EncryptedNewPassword. Nếu ứng dụng gửi một `UserRequest`, journal ghi nguyên văn
  (`crates/engine/src/journal.rs:755-785` luồng ghi `Async`, `:787-826` nhánh `Fsync`).
- **`96` RawData không phải lúc nào cũng là bí mật.** Nó có trong `Logon` (`A`), `UserRequest`
  (`BE`), `News` (`B`), `Email` (`C`). Ở `A`, định nghĩa FIX 4.4 ghi *"Required for some
  authentication methods"*; ở `B`, `C` nó là nội dung bản tin.
- **Kiểu và số tag** (từ `vendor/`): `554` STRING; `925` STRING; `95` LENGTH → `96` DATA;
  `1401` LENGTH → `1402` DATA; `1403` LENGTH → `1404` DATA. Một trường DATA **được phép chứa
  byte SOH** (D3), nên không thể dò hết giá trị của nó bằng cách tìm SOH kế tiếp.
- **Journal là nguồn gửi lại.** Khi mở, `FileJournal::open_with` (`journal.rs:507-640`) đọc lại
  file vào vòng nhớ. Cái gì ghi vào file thay cho mật khẩu thì sau khi khởi động lại sẽ được gửi
  cho đối tác.
- **Journal đã có sẵn một loại bản ghi "chỉ có số, không có bytes"**: dấu outbound của ADR-0053,
  ghi bởi `mark_out` (`journal.rs:909-948`). Khi khôi phục, số đó được tính là đã dùng; xin gửi
  lại số đó thì session lấp khoảng trống — đúng như với bản tin quản trị.
- **Hook xác thực đã thấy mật khẩu trong bộ nhớ**: `Registry::admit`
  (`crates/engine/src/presession.rs:224-244`). Việc che chỉ đụng bản ghi ra đĩa, không đụng cái
  hook này thấy.
- **Đã có hạ tầng test chạy thật qua socket với `FileJournal`**: module trong
  `crates/engine/tests/on_disk.rs` (khoảng dòng 400-600: `OnDisk`, `EchoApp`, `logon_now`,
  `serve_with_recovery`), và với `FileLog`:
  `crates/engine/tests/settings_wire.rs:285-370`.
- **CI đã chạy `cargo test -p fixbolt-engine --tests --features fix50sp2`** kèm script chứng
  minh test đã chạy (`.github/workflows/ci.yml:512-545`). Một test gắn `cfg(feature =
  "fix50sp2")` trong `crates/engine/tests/` được chạy mà không phải sửa CI.

Ngoài repo (tra 2026-09-23; lời người khác, chưa kiểm ở đây — chi tiết và link đầy đủ ở ADR-0110
mục *Research*):

- Artio che mật khẩu trong bản tin nhận ở vai acceptor, thay bằng `***` —
  <https://github.com/artiofix/artio/wiki/Frequently-Asked-Questions>; lớp `PasswordCleaner`
  che `554` và `925`, sửa lại `9=`, không đụng `10=`, không che `96`.
- OnixS có `ScrambledLogonFields`: *"tags for the Logon(A) message fields to scramble in the
  session storage"* — <https://ref.onixs.biz/net-core-fix-engine-guide/api/OnixS.Fix.EngineSettings.html>.
- QuickFIX/J issue #330 (log mật khẩu khi logon lỗi) được đóng bằng một **cờ tắt log**, không
  phải che trường — <https://github.com/quickfix-j/quickfixj/issues/330>,
  <https://github.com/quickfix-j/quickfixj/pull/352>.
- QuickFIX C++/n: tự viết `Log` riêng để bỏ trường —
  <https://quickfix-developers.narkive.com/Zk4a3yRw/username-password-in-logon-msg>.
- QuickFIX/J cho ứng dụng từ chối gửi lại một bản tin (`DoNotSend`), khi đó *"a sequence reset
  will be sent in place of the message"* —
  <https://www.quickfixj.org/usermanual/2.3.0/usage/application.html>.
- Loại lỗi: CWE-532 — <https://cwe.mitre.org/data/definitions/532.html>.

**Tìm mà không thấy:** QuickFIX/Go có che mật khẩu trong log hay store không; bất kỳ CVE hay
advisory nào về một FIX engine ghi mật khẩu ra log.

## Cách làm

Chi tiết lý do và các phương án đã loại ở ADR-0110. Ở đây chỉ phương án được chọn.

1. **Danh sách bí mật, một chỗ duy nhất**: hằng `fixbolt_engine::redact::MASKED`.
   - `554`, `925` (chuỗi) — che trong **mọi** bản tin;
   - `1402` (độ dài ở `1401`), `1404` (độ dài ở `1403`) (DATA) — che trong mọi bản tin;
   - `96` (độ dài ở `95`) — **chỉ** khi `35=` là `A` hoặc `BE`, hoặc khi không đọc được `35=`
     (frame rác).

   Không che: `553` Username (tranh chấp cần biết ai), các trường độ dài, `1400`, `91`, `96`
   trong `News`/`Email`.
2. **Module mới `crates/engine/src/redact.rs`**, hai hàm thuần, nhận slice mượn, không cấp phát,
   không panic, không index trực tiếp (chỉ `get`):
   - `pub fn mask(bytes: &mut [u8]) -> usize` — ghi đè từng byte giá trị của trường bí mật bằng
     `*`, **giữ nguyên độ dài**; trả về số trường đã che;
   - `pub fn carries_secret(bytes: &[u8]) -> bool`.

   Cách dò: tách theo SOH, không cần từ điển. Giá trị chuỗi không chứa SOH, nên mọi trường
   `554=`/`925=` thật đều bắt đầu ngay sau một SOH và **không bao giờ bị sót**; sai sót duy nhất
   có thể là đọc nhầm bytes bên trong một trường DATA khác thành một trường, và hậu quả chỉ là
   che thừa. Trường DATA bí mật được che trên **độ dài lớn hơn** giữa độ dài khai báo (giá trị
   `95=`/`1401=`/`1403=` gần nhất trong bản tin) và đoạn tới SOH kế tiếp, kẹp trong biên của
   buffer. Nguyên tắc: **che thừa được, che thiếu không được.**
3. **Message log** (`msglog.rs`, trong `write_loop`): với bản ghi `In`/`Out` (không phải `Open`),
   gọi `redact::mask` trên `buf[REC_HEADER..n]` **trước** `escape_into`. Làm trên luồng ghi, luồng
   engine không đổi gì. `9=`, `95=`, `10=` giữ như nhận: dòng vẫn tách được thành bản tin, nhưng
   cố ý **không còn khớp checksum** — đó là dấu hiệu dòng đã bị sửa.
4. **Journal** (`journal.rs`): bản tin mang bí mật **không bao giờ ghi vào file**, kể cả dạng đã
   che. Thay vào đó file nhận **dấu outbound ADR-0053 cho đúng số `seq` đó** — đúng những byte
   `mark_out(seq)` ghi (kèm CRC nếu file V1). Vòng nhớ vẫn giữ nguyên văn, nên trong cùng tiến
   trình, `ResendRequest` vẫn nhận lại đúng bản tin (có `43=Y`). Sau khi khởi động lại, số đó
   không có bytes → session lấp khoảng trống, như với bản tin quản trị.
   - `Durability::Async`: quyết định trên luồng ghi, trong `write_loop`, với bản ghi có
     `seq != 0` và độ dài > 0;
   - `Durability::Fsync`: quyết định trong `put`, trên luồng engine (nhánh này vốn đã ghi và
     `fsync` trên luồng engine), trước khi ghi.

   Vì sao không ghi bytes đã che: sau khởi động lại engine sẽ gửi `554=********` cho đối tác —
   một mật khẩu sai trên dây. Vì sao không từ chối `put`: sẽ lấp khoảng trống cả khi chưa khởi
   động lại, và làm bộ đếm `puts_refused` (ADR-0046: "vòng nhớ không đủ chỗ") mang hai nghĩa.
5. **Luôn bật, không có khoá cấu hình.** Công tắc tắt việc che mật khẩu là cái bẫy trong một file
   cấu hình người ta chép từ mạng. Ai thật sự cần bytes gốc thì tự viết `MessageLog` (trait công
   khai) và chịu trách nhiệm trong code. `NoLog`, `MemJournal` không đổi.
6. **`pub mod redact;`** trong `crates/engine/src/lib.rs` — công khai để bench `alloc.rs` và
   người dùng tự viết `MessageLog` gọi được. Không có `#[cfg]` feature: module không kéo thêm
   dependency nào.

**File sẽ tạo:** `crates/engine/src/redact.rs`; `crates/engine/tests/redact.rs`;
`crates/engine/tests/secrets_stay_off_disk.rs`; một trang `docs/reference/` (bẫy ở mục dưới).
**File sẽ sửa:** `crates/engine/src/lib.rs` (một dòng `pub mod`), `crates/engine/src/msglog.rs`
(`write_loop`), `crates/engine/src/journal.rs` (`write_loop`, `put` nhánh `Fsync`),
`crates/engine/benches/alloc.rs` (hai case), các tài liệu ở mục *Tài liệu phải cập nhật*.

## Bất biến bị đụng tới

- **1 — không cấp phát trên hot path.** Luồng engine không đổi dưới `NoLog`, `FileLog` và
  `Durability::Async`. Dưới `Fsync`, `put` thêm một lần gọi `carries_secret` — hàm không cấp
  phát. Chứng minh bằng hai case mới trong `crates/engine/benches/alloc.rs`: `redact-mask` và
  `redact-scan` đọc 0, mỗi case tự kiểm đường chạy là thật (`mask` trả về > 0), và đỏ khi tiêm
  một `Vec`. Luồng ghi thì được phép cấp phát (ADR-0037) nhưng hai hàm này không cấp phát, và
  `tools/w2w --log file --journal file-async` (đếm cấp phát cả hai luồng) giữ 0.
- **2 — session thuần.** Không đụng `crates/session`. Việc che nằm ở tầng engine.
- **5 — thứ tự field từ bảng sinh.** Không xếp lại field nào; `mask` chỉ ghi đè bytes giá trị tại
  chỗ, độ dài giữ nguyên.
- **7 — không `panic!`/`unwrap`/`expect`, không index gây panic.** `redact.rs` là file mới:
  `scripts/check-indexing-debt.sh` phải đọc **không tăng**; dùng `get`/`get_mut`, cộng trừ có
  kiểm tra (`checked_add`/`saturating_*`) khi tính vị trí từ độ dài khai báo — độ dài đó đến từ
  mạng.
- **4 — mode.** Không đổi cách chờ nào; không có vòng lặp mới trên luồng engine.
- **10 — không con số hiệu năng nào.** Plan này không công bố số đo. Chi phí quét dưới `Fsync`
  ghi là `[unmeasured]` trong ADR-0110.
- **3, 6, 8, 9**: không đụng. Không `unsafe`, không feature mới, không chép code QuickFIX.

## Chia việc

Bước 1–3 do **một** senior developer (opus) làm liên tục (bước sau gửi tiếp bằng `SendMessage`
cho cùng agent), vì chúng cùng các file `engine`. Bước 4 (tài liệu) chạy **sau** bước 3 và phải
nằm **cùng commit** với code (`CLAUDE.md` §4, §8). Manager chạy lại gate và commit sau bước 4.

| Bước | Kết quả | Người làm | File được sửa / không được sửa | Gate | Phụ thuộc |
|---|---|---|---|---|---|
| 1 | **Test đỏ trước.** `crates/engine/tests/secrets_stay_off_disk.rs`, chỉ dùng API đã có: (a) qua socket, `serve_with_recovery` + `FileLog` + `FileJournal<_,_>` `Async` trong thư mục tạm; client gửi `Logon` có `553`, `554=<S1>`, `95`, `96=<S2 chứa một SOH>`, rồi một `UserRequest` `35=BE` có `554=<S3>`, `925=<S4>`, `95`, `96=<S5>`, và một `35=D`; app trong test trả lời `BE` bằng một `BE` mang đúng các bí mật đó và trả lời `D` bằng một `35=8`; (b) `FileLog` đứng riêng, `record` một `Logon` có bí mật; (c) `FileJournal` `Async` và `Fsync` đứng riêng, `put` một `BE` có bí mật. Mỗi test đọc file dạng **bytes** và khẳng định không có byte-string bí mật nào. Kèm tiền đề dương: client **có** nhận `554=<S3>` trên dây; file journal **có** chứa bản tin `35=8`; file log **có** dòng `IN` với `35=A` | senior developer (opus) | Chỉ tạo `crates/engine/tests/secrets_stay_off_disk.rs`. Không đụng `crates/*/src`, `crates/session/`, `scripts/`, CI | `cargo test -p fixbolt-engine --test secrets_stay_off_disk` **đỏ**, trích nguyên văn; câu FAIL mong đợi viết ra trước khi chạy (xem *Cách kiểm chứng*) | anh / manager duyệt plan này và ADR-0110 |
| 2 | **Code.** `redact.rs` (hằng `MASKED`, `mask`, `carries_secret`); `pub mod redact;` trong `lib.rs`; gọi `mask` trong `msglog::write_loop`; trong `journal.rs` nhánh `Async` (`write_loop`) và `Fsync` (`put`) ghi dấu outbound thay cho bản ghi mang bí mật. `crates/engine/tests/redact.rs`: các test đơn vị ở mục *Bẫy*, cộng test ghim cặp độ dài→DATA với `fixbolt_dict` (`Fix44` mặc định; `1401→1402`, `1403→1404` trong test `#[cfg(feature = "fix50sp2")]`). Hai case `redact-mask`, `redact-scan` trong `benches/alloc.rs`, thêm tên vào dòng `allocations:` và mảng assert | senior developer (opus), cùng agent | Sửa: `crates/engine/src/{redact.rs (mới), lib.rs, msglog.rs, journal.rs}`, `crates/engine/tests/redact.rs` (mới), `crates/engine/benches/alloc.rs`. **Không** sửa: `conn.rs`, `crates/session/`, `crates/codec/`, `crates/dict/`, file test có sẵn, `scripts/`, `.github/` | `cargo test -p fixbolt-engine --test secrets_stay_off_disk --test redact`; `cargo test -p fixbolt-engine --features fix50sp2 --test redact`; test có sẵn **không sửa** vẫn xanh: `cargo test -p fixbolt-engine --test msglog --test journal --test on_disk --test journal_reader --test engine_recovery --test recovery --test shard_recovery --test settings_wire`; `cargo test --all`; `cargo test --no-default-features`; `cargo bench -p fixbolt-engine --bench alloc` (dòng `allocations:` có `redact-mask 0 redact-scan 0`, mọi case cũ vẫn 0); `cargo clippy --all-targets -- -D warnings`; `cargo fmt --check`; `scripts/check-indexing-debt.sh`, `scripts/check-no-crate-root-allow.sh`, `scripts/check-lint-config.sh` | 1 |
| 3 | **Đảo ngược, từng cái một**, mỗi cái viết câu FAIL mong đợi trước, chạy, trích đỏ, khôi phục, trích xanh: R1 bỏ lời gọi `mask` trong `msglog::write_loop`; R2 bỏ nhánh bí mật trong `journal::write_loop` (`Async`); R3 bỏ nhánh bí mật trong `put` (`Fsync`); R4 che DATA chỉ tới SOH kế tiếp (bỏ độ dài khai báo); R5 tiêm `let _v: Vec<u8> = std::hint::black_box(Vec::with_capacity(1));` vào `mask` (phải có `black_box`, xem *Sửa 1*); R6 bỏ điều kiện `35=A`/`BE` cho `96` | senior developer (opus), cùng agent | Như bước 2; mọi đảo ngược được khôi phục, `git diff` cuối cùng giống hệt trước bước 3 | R1 đỏ ở test log của `secrets_stay_off_disk`; R2 đỏ ở test journal `Async` và test qua socket; R3 đỏ ở test journal `Fsync`; R4 đỏ ở test "nửa sau SOH của `96`"; R5 `redact-mask` > 0; R6 đỏ ở test "`96` trong `News` giữ nguyên" | 2 |
| 4 | **Tài liệu**, cùng commit với code: danh sách ở mục *Tài liệu phải cập nhật*; trang `docs/reference/` mới; ADR-0110 → `Accepted` (manager ghi dòng trạng thái) | developer (sonnet) | Chỉ các file `docs/`, `CHANGELOG.md` được liệt kê. **Không** sửa `crates/`, `STATUS.md` (manager viết) | `python3 scripts/check-links.py` xanh | 3 |
| 5 | **Review + CI**: một senior review, context mới, được đưa plan này, ADR-0110 và các gate; manager kiểm từng phát hiện theo `CLAUDE.md` §12; PR nháp, CI xanh trên commit đóng, ghi run id | senior developer (opus) review; manager | — | CI xanh trên commit đóng, run id ghi vào *Nhật ký giao hàng* | 4 |

**Kiểm cả hai luồng không cấp phát (Linux, máy nào cũng được, không phải số đo độ trễ):**
`cargo run --release -p fixbolt-w2w -- --log file --journal file-async` — trích dòng đếm cấp phát
của cả hai luồng, phải 0. Không cần máy §9: đây là đếm, không phải thời gian.

## Cách kiểm chứng

| # | Tiêu chí | Lệnh | Đạt khi |
|---|---|---|---|
| 1 | Đỏ trên code chưa viết | `cargo test -p fixbolt-engine --test secrets_stay_off_disk` ở bước 1 | FAIL ở test log với câu dạng *"the message log holds the secret `<S1>`"*; test journal `Async`/`Fsync` FAIL với *"the journal file holds the secret `<S3>`"*. Riêng nửa `Logon` của khẳng định journal **xanh ngay từ đầu** (Logon không vào journal — đúng thiết kế) và được giữ làm test canh hồi quy |
| 2 | Không bí mật nào xuống đĩa | cùng lệnh, sau bước 2 | xanh; tiền đề dương cũng xanh (bí mật có trên dây; journal có `35=8`; log có `IN 35=A` với `554=` theo sau là dãy `*` cùng độ dài) |
| 3 | Gửi lại trong cùng tiến trình vẫn đúng | test trong `secrets_stay_off_disk.rs`: client gửi `ResendRequest` cho số của `BE` | nhận lại `BE` có `43=Y` và `554=<S3>` nguyên văn |
| 4 | Sau khởi động lại thì lấp khoảng trống | test mở lại `FileJournal` từ file | `get(seq_BE)` là `None`, `highest_out()` ≥ `seq_BE`; `journal::Reader` thấy `Record::OutboundMark { seq: seq_BE }` và không có `Record::Message` cho số đó |
| 5 | Không cấp phát | `cargo bench -p fixbolt-engine --bench alloc` | `redact-mask 0 redact-scan 0`; R5 (dạng `std::hint::black_box(Vec::with_capacity(1))`) làm nó > 0 |
| 6 | Đảo ngược | bước 3, R1–R6 | từng cái đỏ đúng test đã ghi trước, rồi xanh lại |
| 7 | Không phá gì đã có | các lệnh ở bước 2 | test có sẵn xanh **không sửa**; 59 / 59 (`cargo test -p fixbolt-session --test score`) vẫn nằm trong `cargo test --all` |

"Bản ghi thật": test qua socket chạy đúng đường mà `docs/GETTING-STARTED.md` dạy (kernel socket,
`serve_with_recovery`, `Logon` thật có checksum thật). Không có capture của đối tác nào được dùng
hay commit.

## Tài liệu phải cập nhật

Theo bảng đồng bộ ở `CLAUDE.md` §4, đi từng dòng:

- [ ] `docs/DESIGN.md` — §3 bảng module của `engine`: thêm dòng `redact`; D7 (đoạn *The journal
  is not a message log*): bản tin mang bí mật chỉ để lại số trong file; D14: log che bí mật trên
  luồng ghi; §6 dòng *Allocations on the hot path, engine*: thêm `redact-mask`, `redact-scan`
  và sửa số đường chạy
- [ ] `docs/GUIDE.md` §6b và §6c — cái gì bị che, dòng log không còn khớp `10=`, `UserRequest`
  bị lấp khoảng trống sau khởi động lại, file cũ vẫn chứa mật khẩu (đổi mật khẩu, xoá hoặc siết
  quyền file cũ), tag bí mật riêng của sàn (`5000+`) **không** được che
- [ ] `docs/CONFIGURATION.md` §2 — hằng `redact::MASKED` là giá trị người dùng thấy được; ghi rõ
  **không có khoá** bật/tắt và vì sao
- [ ] `docs/SESSION-BEHAVIOUR.md` §4 — sau khởi động lại, bản tin ứng dụng mang bí mật được lấp
  khoảng trống, không gửi lại; nêu tên test canh
- [ ] `CHANGELOG.md` `[Unreleased]` — *Added* module `redact`; *Changed* hành vi ghi của
  `FileLog`, `FileJournal`
- [ ] `docs/internals/engine.md` — bảng file thêm `redact.rs`, thứ tự đọc, test canh
- [ ] `docs/reference/` — trang mới về bẫy *"một trường DATA có thể chứa SOH, nên che tới SOH
  kế tiếp là che thiếu"*, kèm tên test canh
- [ ] `docs/decisions/ADR-0110` — `Proposed` → `Accepted` khi plan được duyệt
- [ ] `STATUS.md` — manager viết (mục *Not proven*, handoff), không phải developer
- Không đổi: `PRD.md` (phase 3 đã ghi việc này), `README.md` (không thêm crate),
  `docs/hft-playbook.md`, `docs/best-practices-*.md`, `DESIGN.md` §8 (không dòng ngân sách nào
  di chuyển), `DESIGN.md` §9, `docs/CONFORMANCE.md` (không đổi con số conformance)

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| `96` chứa SOH; che tới SOH kế tiếp thì nửa sau lộ ra | `secrets_stay_off_disk` với `<S2>` chứa SOH; `redact.rs::a_raw_data_holding_an_soh_is_masked_whole`; đảo ngược R4 |
| Độ dài khai báo lớn hơn phần còn lại của buffer (frame rác, frame cụt) → cộng tràn hoặc index ngoài biên | `redact.rs::a_declared_length_past_the_end_is_clamped_not_a_panic`; `redact.rs::every_prefix_of_a_logon_is_scanned_without_a_panic` (lặp qua mọi tiền tố của fixture) |
| Khớp nhầm `1554=` hay `5540=` thành `554=` | `redact.rs::only_the_whole_tag_554_is_masked` |
| Che `96` trong `News`/`Email` — làm hỏng nội dung và làm `News` bị lấp khoảng trống sau khởi động lại | `redact.rs::raw_data_on_news_is_left_alone`; đảo ngược R6 |
| Frame rác không có `35=` mà vẫn mang `96=` | `redact.rs::raw_data_on_a_frame_with_no_msg_type_is_masked` |
| `554=` rỗng, hoặc ở cuối buffer không có SOH đóng | `redact.rs::an_empty_or_unterminated_password_is_handled` |
| Che làm đổi độ dài → `9=` sai, dòng log không tách được nữa | `redact.rs::masking_keeps_the_length`; test log so `len` trước/sau |
| Journal ghi bytes đã che → sau khởi động lại gửi mật khẩu sai cho đối tác | tiêu chí 4 (mở lại journal: `get` là `None`, có `OutboundMark`) |
| Chỉ sửa nhánh `Async`, quên `Fsync` (hoặc ngược lại) | test journal chạy **cả hai** `Durability`; đảo ngược R2, R3 |
| Che trong vòng nhớ luôn → gửi lại trong tiến trình mang mật khẩu sai | tiêu chí 3 |
| Test xanh vì chẳng có gì được ghi (file rỗng, writer chưa chạy xong) | tiền đề dương: file journal có `35=8`, log có `IN 35=A`; chờ có giới hạn thời gian như `settings_wire.rs:337-347`, không `sleep` cố định |
| Test xanh vì bí mật không hề đi qua (app không gửi `BE`) | tiền đề dương: client nhận `554=<S3>` trên dây |
| Cặp độ dài→DATA viết tay lệch khỏi từ điển khi phase 3 bước 2 đổi nguồn từ điển | `redact.rs::the_length_pairs_match_the_dictionary` (+ bản `fix50sp2`) |
| Che mà lại cấp phát | `redact-mask`, `redact-scan` trong `benches/alloc.rs`; đảo ngược R5; `fixbolt-w2w --log file --journal file-async` |
| Thêm index trực tiếp vào file mới làm tăng nợ `indexing_slicing` | `scripts/check-indexing-debt.sh` |
| Tên file tạm trùng nhau giữa test chạy song song | tên file theo `process::id()` và `thread::current().id()`, như `on_disk.rs::tmp` |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Độ dài mật khẩu lộ trong log (dãy `*` cùng độ dài) | Thấp | chấp nhận, ghi ở ADR-0110 *Consequences*; quyền file là lớp bảo vệ còn lại |
| Công cụ đọc log có kiểm checksum sẽ từ chối dòng đã che | Thấp | ghi trong `GUIDE.md` §6c |
| Đối tác trông chờ `UserRequest` được gửi lại sau khi ta khởi động lại | Thấp | hành vi giống QuickFIX/J khi app ném `DoNotSend`; ghi trong `SESSION-BEHAVIOUR.md` §4 |
| Che thừa bên trong một trường DATA khác (ví dụ `213` XmlData chứa `\x01554=`) | Thấp | chấp nhận có chủ đích (che thừa hơn che thiếu); ghi ở ADR-0110 |
| `Fsync` chậm thêm một lần quét | Thấp | `[unmeasured]`; không công bố số; nếu cần, đo bằng `benches/journal.rs` trên máy §9 ở plan khác |
| Test qua socket chập chờn trên CI | Trung bình | dùng đúng mẫu `on_disk.rs` / `settings_wire.rs` đã ổn định; chờ có hạn, không `sleep` cố định |
| Test ghim cặp `fix50sp2` không chạy trên CI | Thấp | job `gates` đã chạy `fixbolt-engine --features fix50sp2` và script chứng minh test đã chạy |

## Ngoài phạm vi

- **Bí mật trong bộ nhớ**: buffer nhận, vòng nhớ journal, vòng nhớ log, core dump. Không xoá
  sạch bộ nhớ.
- **Viết lại file cũ** đã chứa mật khẩu. Chỉ hướng dẫn trong `GUIDE.md`.
- **Danh sách tag bí mật do người dùng thêm** (tag riêng của sàn). Mở lại khi có triển khai đầu
  tiên nêu tên một tag như vậy (ADR-0110).
- **Khoá cấu hình bật/tắt** việc che.
- **`91` SecureData, `553` Username, `1400`** — không che.
- **Initiator gửi `554=` trong `Logon` của chính mình**: hôm nay session không có đường nào để
  thêm `554` vào `Logon` đi (`crates/session/src/lib.rs:2453-2477` chỉ có `98`, `108`, `789`,
  `1137`). Nếu sau này có, bản ghi `Out` của log đã được che bởi cùng một đường, và test
  `FileLog` đứng riêng ở bước 1 đã canh chiều `Out`.
- **Log của `tracing`**: engine không log bytes bản tin qua `tracing` (grep `crates/*/src`
  2026-09-23 không thấy); nếu sau này có, nó phải đi qua `redact::mask`.
- Đo độ trễ, `DESIGN.md` §8, máy §9.

## Nhật ký giao hàng

*(Manager ghi vào đây khi đóng từng bước: commit, gate đã chạy, CI run id, cái gì chưa chứng
minh.)*

- **2026-09-23 — bước 1–3 xong, chưa commit** (worktree `fb-p3r3`, nhánh `plan/p3-redact-secrets`).
  Bước 1 đỏ đúng như dự đoán: 4/4 test của `secrets_stay_off_disk` đỏ ở câu *"… holds the
  secret …"*, còn nửa `Logon` của journal thì xanh ngay từ đầu. Sau bước 2 thì xanh. R1–R6 đỏ
  đúng test đã ghi trước, rồi khôi phục (sha256 khớp bản trước bước 3). Có hai chỗ phải sửa
  plan, ghi ở *Sửa 1*. Commit, CI run id: manager ghi khi đóng.

## Sửa 1 — 2026-09-23

**Plan tự mâu thuẫn ở một chỗ.** *Cách làm* mục 1 (và ADR-0110 quyết định 1) nói: bản tin không
đọc được `35=` mà có `96=` thì che `96`. Nhưng bước 2 lại đòi `--test msglog` phải xanh **mà không
sửa file test**. Hai test có sẵn trong `crates/engine/tests/msglog.rs` dùng đúng loại bản tin đó
(không có `35=`, có `96=`), nên code mới che `96` và chúng đỏ:

```text
thread 'a_data_field_with_a_newline_stays_on_one_line' panicked at crates/engine/tests/msglog.rs:154:5:
left: "20260903-10:32:07.120 IN  shard=0 conn=1 8=FIX.4.4\u{1}95=3\u{1}96=***\u{1}10=000\u{1}"
thread 'a_backslash_in_a_data_field_round_trips' panicked at crates/engine/tests/msglog.rs:174:5:
assertion `left == right` failed: the escaped line must decode back to the exact bytes that arrived
```

**Quyết định: thêm `35=B` (News) vào hai bản tin mẫu đó, không đổi câu kiểm nào.** Hai test ấy
kiểm việc *escape* (xuống dòng, dấu `\`) trong một trường DATA, không kiểm chuyện bí mật. Chúng
viết ra lúc log còn ghi nguyên văn mọi byte, và ADR-0110 cố ý bỏ hành vi đó cho bản tin không có
`35=`. Có `35=B` thì `96` là nội dung, không bị che, nên test vẫn kiểm đúng điều nó định kiểm.
Nửa còn lại (bản tin **không có** `35=` thì `96` **bị** che trong file log) giờ có test riêng:
`secrets_stay_off_disk.rs::raw_data_on_a_frame_with_no_msg_type_is_masked_in_the_log`. Test
này đã được đảo ngược thử: đổi luật thành "không có `35=` thì không che" thì nó đỏ. Phương án
bị loại: bỏ luật che `96` khi không có `35=`. Làm vậy trái ADR-0110 đã chấp nhận, và frame rác
sẽ bị che thiếu.

**Sửa R5.** Viết đúng như bảng cũ (`let _v: Vec<u8> = Vec::with_capacity(1);`) thì bench vẫn in
`redact-mask 0`: ở chế độ build của bench, trình biên dịch xoá luôn cặp cấp phát/giải phóng không
ai dùng, nên phép đảo ngược không chứng minh gì. Viết `std::hint::black_box(Vec::with_capacity(1))`
thì bench in `redact-mask 1000` và đỏ ở câu *"non-negotiable 1: the engine allocates nothing on
the byte path"*. Bảng *Chia việc* và *Cách kiểm chứng* đã sửa theo dạng này. Bẫy này đã từng gặp
một lần (`docs/reference/measured-costs.md`, mục *An injected allocation the optimiser can delete
proves nothing*); lần này ghi thêm vào đó.

## Sửa 2 — 2026-09-23

Senior review của PR #98 (commit `6899701`) tìm ra ba điều.

**Phát hiện 1 — đã sửa.** Một frame hỏng có `9=` quá lớn làm `Framer::cut` trả `Cut::Garbage`
cho cả buffer (`frame.rs:17`, `:166`), và engine ghi cả khối đó vào log thành **một** bản ghi
`In` (`conn.rs:448`). Code cũ chỉ nhìn `35=` **đầu tiên** trong bản ghi. Nếu đó là `35=0`
(Heartbeat) thì `96` RawData của một `BE`/`A` nằm phía sau trong cùng khối bị ghi nguyên văn
(`554` thì vẫn được che). Dòng thử của người review:

```text
Framer<4096> fed `8=FIX.4.4|9=5000|35=0|34=5|10=000|` + a real BE with `95=9|96=RAWSECRET|554=PW|`,
then FileLog::record(Direction::In, …) → `log file holds RAWSECRET=true 554=PW=false`
```

Hai test mới đỏ trước khi sửa:

```text
the message log holds the secret `rawUSERREQp3Wd` — S5 (96 on UserRequest)
the RawData of the Logon inside the garbage survives: 8=FIX.4.4|9=5000|35=0|34=5|10=000|8=FIX.4.4|9=60|35=A|34=6|95=14|96=raw1|raw2-tail|98=0|554=*******|10=000|
```

**Cách sửa:** `96` bị che khi **bất kỳ** `35=` nào trong bản ghi là `A` hoặc `BE`, hoặc khi không
có `35=` nào. Một `35=` khác đứng trước không còn miễn che, và một `35=` đứng sau không gỡ che được.
Vẫn quét theo SOH, không cấp phát. Test canh:
`secrets_stay_off_disk.rs::raw_data_behind_a_non_sign_on_msg_type_in_a_garbage_cut_is_masked_in_the_log`,
`redact.rs::raw_data_of_a_logon_behind_another_msg_type_is_masked`,
`redact.rs::a_later_msg_type_does_not_unmask_raw_data`. Không có test cũ nào đổi nghĩa.

**Phát hiện 2 — hệ quả đã biết, không sửa.** Một trường DATA bí mật mà trường độ dài của nó bị
thiếu hoặc đứng **sau** nó (`1402` không có `1401`, `96` đứng trước `95`) thì không có độ dài khai
báo để dựa vào, nên chỉ được che tới SOH kế tiếp. Nếu giá trị của nó chứa SOH thì phần sau lọt vào
log. Frame như vậy là frame sai (D3: trường độ dài phải đứng ngay trước trường DATA). Câu "không bao
giờ che thiếu" giờ chỉ đúng với cặp độ dài viết đúng thứ tự. Đã sửa lời khẳng định ở `redact.rs`
(doc module), `DESIGN.md` §3 (dòng `redact`) và
`docs/reference/masking-a-data-field-to-the-next-soh-under-masks.md`. ADR-0110 đã Accepted nên
không sửa nội dung; chỉ thêm một dòng dưới *Status* trỏ về mục này. `CHANGELOG.md`, `GUIDE.md`,
`CONFIGURATION.md` sửa theo luật "bất kỳ `35=`".

**Phát hiện 3 — ngoài phạm vi, không sửa ở đây** (manager mở một mục trong STATUS). Lỗi có sẵn
trên `main`: luồng ghi của journal `Async` dừng hẳn khi gặp một bản ghi dài hơn 4096 byte
(`journal.rs` `write_loop` dùng `buf [0u8;4096]`, và `ring::pop` trả `Some(0)` cho bản ghi quá cỡ
thì bị hiểu là tín hiệu STOP; `journal.rs:793/798` ở nhánh này, `756/761` trên `main`). Dòng thử
của người review: `FileJournal<8,8192>` `Async`, `put` 4200 byte rồi một bản tin nhỏ, mở lại →
`get(1)=None get(2)=None highest_out=None`.
