# Phase 3: để một người lạ dùng được fixbolt

> **Loại:** Plan · **Ngày:** 2026-09-23 · **Trạng thái:** Đã duyệt (anh duyệt 2026-09-23, cả tám khuyến nghị)
> **Phạm vi:** phạm vi phase 3 — đề xuất bởi [ADR-0097](../decisions/ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md), `PRD.md` §2 *Phase 3*

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Phase 1 (engine FIX 4.4 chạy được, cả hai vai, 59 / 59) và phase 2 (`Encoding` trait, SBE
không kèm session, FIX 5.0 SP2 / FIXT 1.1) đã giao xong. `PRD.md` §2 đến giờ chỉ ghi
"Phase 3: not scoped" kèm một danh sách ứng viên (kernel bypass, SIMD, cluster, HA, replication)
— để ai muốn nhét thêm việc thì phải cãi với tài liệu.

Anh yêu cầu một đề xuất có phạm vi rõ và có lý lẽ, để anh duyệt hoặc bác. File này là đề xuất
đó, viết cho anh đọc. Lý do chi tiết, các phương án bị loại và cái giá phải trả nằm trong
ADR-0097; ở đây chỉ nói **làm gì, vì sao, theo thứ tự nào, và anh cần quyết những gì**.

Tóm một câu: **phase 3 không làm engine nhanh hơn — nó làm engine *tải về được và tin được*.**
Khoảng cách lớn nhất giữa fixbolt và QuickFIX (`PRD.md` §3) là *chưa ai chạy nó ngoài đời*, và
khoảng cách đó chỉ đóng lại khi có người khác dùng. Hôm nay không ai dùng được, vì không có gì
để tải.

## Những gì đã biết chắc

Trong repo (đã đọc hoặc đã chạy ngày 2026-09-23):

- **Không crate nào publish được.** Mọi crate đều `version = "0.0.0"`, `publish = false`;
  `docs/GETTING-STARTED.md` ghi *"Not on crates.io yet"*.
- **Kể cả bật cờ publish lên cũng không build được.** `crates/dict/build.rs` (dòng 1–45) sinh
  bảng từ điển từ `../../vendor/quickfix/spec/FIX44.xml`, mà `vendor/` bị gitignore (ADR-0001).
  Crate tải từ crates.io không có `vendor/` → build hỏng. Muốn sửa thì hoặc phải kèm file
  của QuickFIX vào gói (kéo theo điều khoản ghi công, `NOTICE` — bất biến 9, ADR-0001 quyết
  định 5), hoặc sinh từ một nguồn khác có giấy phép riêng.
- **Hook xác thực đã có.** `Registry::admit` (`crates/engine/src/presession.rs:228-244`) nhìn
  thấy `553` / `554` / `96`, có test `a_registry_that_sees_the_logon_can_refuse_a_wrong_password`.
  Dòng trong `PRD.md` §3 ghi "nothing beyond identity" đã cũ ở điểm này.
- **Có thể mật khẩu đang bị ghi ra đĩa.** Grep `crates/engine/src` và `crates/library/src`
  không thấy chỗ nào che `554=` hay `96=`. Message log (D14) ghi cả hai chiều, nên Logon của
  đối tác — kèm mật khẩu — **có thể** nằm nguyên trong file log. *Mới grep, chưa chạy thử*; bước
  4 bên dưới sẽ chứng minh bằng test.
- **`Decimal` đã quyết nhưng chưa xây**: ADR-0028 accepted 2026-09-01; hôm nay ứng dụng nhận giá
  dưới dạng bytes.
- Toolchain ghim `1.98.0` (`rust-toolchain.toml`), `rust-version = "1.85"` (`Cargo.toml`).

Ngoài repo (tra cứu ngày 2026-09-23, là lời của người khác, chưa kiểm chứng ở đây):

- **Tên `fixbolt` còn trống trên crates.io** — API crates.io trả về *"crate `fixbolt` does not
  exist"*.
- **Khoảng trống "chưa có acceptor Rust" đang bị người khác lấp.** `ironfix-engine` 0.4.1
  (2026-09-18) đã có `Acceptor`, nhưng tự ghi không có TLS, không kiểm từ điển, store chỉ trong
  RAM — <https://docs.rs/ironfix-engine/latest/ironfix_engine/>. `hotfix` 0.13.0 vẫn chỉ là
  initiator — <https://crates.io/crates/hotfix>. `fefix` không ra bản mới từ 2021 —
  <https://docs.rs/fefix>.
- **Có nguồn từ điển giấy phép Apache-2.0**: FIX Trading Community công bố
  `OrchestraFIX44.xml`, `OrchestraFIXLatest.xml` trong
  <https://github.com/FIXTradingCommunity/orchestrations> (thư mục `FIX Standard`). Nó có khớp
  với XML của QuickFIX trên 912 tag / 12 524 cặp / 1 708 enum hay không thì **chưa ai đo**.
- **Giấy phép QuickFIX**: phát hành lại dạng source hay binary đều phải kèm thông báo bản quyền
  — <https://raw.githubusercontent.com/quickfix/quickfix/master/LICENSE>.
- **Người ta chọn FIX engine theo gì**: độ trễ ở tải thật, hỗ trợ dialect của sàn, store thay
  được, giám sát, hỗ trợ, giao thức nhị phân, bộ test chấp nhận —
  <https://www.onixs.biz/insights/the-complete-guide-to-fix-engine-selection-in-2026>.
- **HA ở các engine khác là sản phẩm cluster, không phải tính năng engine.** Chronicle FIX
  replicate hàng đợi store sang acceptor dự phòng; chế độ mặc định *"sacrifices consistency"*,
  chế độ nhất quán thì chỉ gửi khi bản sao đã ack — tức là thêm một vòng mạng vào mỗi lần gửi —
  <https://chronicle.software/tech-hub/technical-information/chronicle-fix/failover>. Artio đứng
  trước Aeron Cluster, và chính tài liệu Aeron nói khi leader chết có thể mất message —
  <https://aeron.io/docs/aeron-cluster/cluster-gateway-patterns/>. OnixS "failover" là initiator
  nối sang server dự phòng — <https://ref.onixs.biz/cpp-fix-engine-guide/group__failover.html>.
- **Store dạng database** có ở QuickFIX/J (`JdbcStore`, JMX, dynamic session) —
  <https://quickfixj.org/docs/architecture/> — và QuickFIX/Go (SQL, MongoDB) —
  <https://pkg.go.dev/github.com/quickfixgo/quickfix/store/mongo>; Fix8 dùng Redis/BerkeleyDB
  tuỳ chọn — <https://github.com/fix8/fix8>.
- **FIXP**: bản 1.0 là Technical Standard (2021, điều kiện là "hai bản cài đặt tương thích
  nhau"), bản 1.1 vẫn Draft (2019); **không có bộ test conformance công khai** —
  <https://github.com/FIXTradingCommunity/fixp-specification>. CME iLink 3 dùng FIXP —
  <https://cmegroupclientsite.atlassian.net/wiki/spaces/EPICSANDBOX/pages/714145834/iLink+Binary+Order+Entry+-+Session+Layer>.
  B3 Binary EntryPoint dùng FIXP + SBE và công bố schema —
  <https://www.b3.com.br/en_us/solutions/platforms/puma-trading-system/for-developers-and-vendors/entrypoint/>.
  **Artio cài Binary EntryPoint ở vai acceptor** (iLink 3 thì chỉ initiator) —
  <https://github.com/artiofix/artio>, trang wiki *FIXP Support*. Có một bộ test bên thứ ba bằng
  C# (MIT, 0 sao) — <https://github.com/pedrosakuma/B3EntryPointClient>.
- **Kernel bypass**: Onload chạy được trên card không phải Solarflare qua AF_XDP, hỗ trợ cộng
  đồng — <https://github.com/Xilinx-CNS/onload/blob/master/README.md>; ef_vi trên phần cứng
  Solarflare đo 1.866 µs RTT trung vị — <https://github.com/ASherjil/ABTRDA3>. Không đổi gì so
  với ADR-0074.
- **Công cụ publish**: `cargo publish --workspace` ổn định từ Rust 1.90, dry-run build cả bộ như
  thể đã publish, nhưng publish thật **không nguyên tử** —
  <https://blog.rust-lang.org/2025/09/18/Rust-1.90.0/>. `cargo-semver-checks` bắt thay đổi phá
  API — <https://github.com/obi1kenobi/cargo-semver-checks>.

**Tìm mà không thấy:** bộ test conformance FIXP công khai kiểu 59 file `.def`; sàn nào chạy SBE
bên trong session FIXT tag=value; con số độ trễ đã tái lập cho bất kỳ acceptor Rust nào; engine
mã nguồn mở nào có HA là tính năng của chính engine (thay vì dựa vào Chronicle Queue / Aeron
Cluster).

## Cách làm

Phase 3 gồm bốn mảng, theo thứ tự:

1. **Từ điển mà crate đã publish build được.** Trước hết một ADR riêng chọn nguồn (khuyến nghị:
   file Orchestra Apache-2.0, giữ XML QuickFIX làm "trọng tài" so khớp). Rồi đổi
   `crates/dict/build.rs` theo quyết định đó, cộng một test so khớp hai nguồn.
2. **Ba việc song song, mỗi việc một crate riêng, không đụng nhau:**
   - che `554=` / `96=` trước khi ghi message log và journal (`crates/engine`);
   - xây `Decimal` theo ADR-0028 (`crates/codec`);
   - interop với QuickFIX/J — engine thứ hai khác họ với `libquickfix` — cả plaintext và TLS
     (`tools/interop`, `scripts/interop.sh`, CI).
3. **Đóng gói**: metadata cho từng crate được publish, job CI `cargo publish --workspace
   --dry-run` trên máy không có `vendor/`, job `cargo-semver-checks`.
4. **Tài liệu cho người lạ, rồi publish `0.1.0`**: `GETTING-STARTED.md` chuyển sang `cargo add`,
   rustdoc cho crate công khai, `CHANGELOG.md` ghi điều kiện lên `1.0`. Sau khi publish: một
   script tạo crate mới ngoài repo, lấy `fixbolt` từ crates.io, dán nguyên code của
   `GETTING-STARTED.md`, chạy một Logon / Logout.

Cuối cùng, **có điều kiện**: một spike FIXP — chỉ để chứng minh "trọng tài" (acceptor Binary
EntryPoint của Artio) chạy được headless trong CI, rồi viết ADR FIXP từ kết quả đó. **Không xây
session FIXP trong phase 3.** Nếu anh không chọn sàn mục tiêu, spike không chạy.

File sẽ tạo/sửa (theo từng PR, chi tiết ở plan của từng bước): `crates/dict/build.rs` và test
so khớp; `crates/engine/src` (đường ghi message log / journal) và một test trong
`crates/engine/tests/`; `crates/codec/src` (module `Decimal`) và `crates/codec/benches/alloc.rs`;
`tools/interop`, `scripts/interop.sh`, `scripts/fetch-*` cho QuickFIX/J; `.github/workflows/ci.yml`;
`crates/*/Cargo.toml` (metadata, `publish`); `docs/GETTING-STARTED.md`, `README.md`,
`CHANGELOG.md`, `docs/CONFIGURATION.md`, `docs/GUIDE.md`; một ADR cho nguồn từ điển, một ADR
cho FIXP nếu spike chạy.

## Bất biến bị đụng tới

- **1 (không cấp phát trên hot path)**: `Decimal` nằm trên đường ứng dụng đọc giá → case mới
  trong `crates/codec/benches/alloc.rs`, chứng minh bằng tiêm lỗi. Việc che mật khẩu nằm trên
  đường ghi log → không được cấp phát; test đếm allocator như các case log hiện có.
- **2 (session thuần)**: không việc nào đụng `crates/session`. Che mật khẩu làm ở tầng engine,
  không ở session.
- **3 (59 / 59)**: đổi nguồn từ điển có thể đổi bảng mà session dùng → 59 / 59 và FIXT 179 / 180
  phải chạy lại ở bước 2 của bảng dưới. FIXP (nếu có về sau) cần một gate thay thế — đó chính
  là câu hỏi ADR FIXP phải trả lời.
- **5 (thứ tự field từ bảng sinh)**: nguồn từ điển mới vẫn sinh bảng; không được có call site
  tự xếp field.
- **6 (feature flag)**: crate đã publish sẽ được build với tổ hợp feature do *người dùng* chọn →
  dry-run publish phải chạy cả `--no-default-features`.
- **7 (không panic)**: `Decimal` parse lỗi trả về enum lỗi không field.
- **9 (không copy QuickFIX)**: đây là bất biến bị ép mạnh nhất. Jar QuickFIX/J chỉ được fetch
  vào `vendor/`; nếu chọn phương án kèm bảng sinh từ QuickFIX thì `NOTICE` bắt buộc.
- **10 (số đo)**: README và docs.rs chỉ được trích số có benchmark, máy và thiết lập §9 đi kèm.
- 4 và 8: không đụng.

## Chia việc

Mỗi hàng là một pull request, vừa một phiên.

| Bước | Kết quả | Người làm (đề xuất) | Phụ thuộc |
|---|---|---|---|
| 1 | ADR nguồn từ điển + spike đo độ khớp Orchestra ↔ QuickFIX (chỉ đo, chưa đổi build) | architect, rồi senior developer cho spike | anh duyệt plan này |
| 2 | `fixbolt-dict` build không cần `vendor/`; test so khớp giữ QuickFIX làm trọng tài; 59 / 59 và 179 / 180 chạy lại | senior developer (đụng bảng mà session dùng) | 1 |
| 3 | Che `554=` / `96=` trong message log và journal; test grep bí mật, chứng minh đỏ khi bỏ che | senior developer (`engine`) | anh duyệt plan này |
| 4 | `Decimal` theo ADR-0028; case alloc = 0 | senior developer (`codec`, hot path) | anh duyệt plan này |
| 5 | Interop QuickFIX/J hai vai, plaintext + TLS, job CI chặn merge | developer (sonnet) | anh duyệt plan này |
| 6 | Metadata các crate, job `cargo publish --workspace --dry-run` không `vendor/`, job `cargo-semver-checks` (chưa chặn) | developer (sonnet) | 2 |
| 7 | Tài liệu cho người lạ (`GETTING-STARTED`, rustdoc, `CHANGELOG` với điều kiện `1.0`); **anh** chạy `cargo publish` `0.1.0` | developer (sonnet) viết; publish do anh | 3, 4, 5, 6 |
| 8 | Script "người lạ": crate mới ngoài repo lấy `fixbolt@0.1.0` từ crates.io, chạy Logon / Logout; `cargo-semver-checks` chuyển sang chặn | developer (sonnet) | 7 |
| 9 | *Có điều kiện*: spike trọng tài FIXP (Artio Binary EntryPoint acceptor chạy headless trong CI) và ADR FIXP | architect + senior developer | anh chọn sàn (câu hỏi 4) |

Bước 3, 4, 5 chạy song song được (crate khác nhau). Bước 1 cũng song song với chúng.

## Cách kiểm chứng

Mỗi tiêu chí thoát là một lệnh, pass hoặc fail, trên commit đóng, kèm CI run id:

| # | Tiêu chí | Lệnh |
|---|---|---|
| 1 | Build không cần `vendor/` | trên checkout đã xoá `vendor/`: `cargo build -p fixbolt-dict` và `cargo build -p fixbolt --no-default-features` |
| 2 | Bảng vẫn khớp trọng tài | test so khớp: 912 / 912 tag, 12 524 / 12 524 cặp, 1 708 / 1 708 enum, hoặc từng chỗ lệch được gọi tên theo nội dung; `cargo test -p fixbolt-session --test score` vẫn 59 / 59 |
| 3 | Đóng gói như khi publish | `cargo publish --workspace --dry-run` trong CI, không có `vendor/` |
| 4 | Không bí mật nào xuống đĩa | test ghi Logon có `554=` / `96=` qua message log và `FileJournal`, rồi tìm bí mật trong cả hai file; đỏ khi bỏ che |
| 5 | `Decimal` | `cargo test -p fixbolt-codec decimal`; case alloc đọc 0, đỏ khi tiêm `format!` |
| 6 | Engine họ khác đồng ý | `scripts/interop.sh` với QuickFIX/J, hai vai, 7 / 7, plaintext **và** TLS |
| 7 | Người lạ dùng được | script crate mới lấy `fixbolt@0.1.0` từ crates.io, dán code `GETTING-STARTED.md`, một Logon / Logout, exit 0 |
| 8 | API được canh | `cargo semver-checks --baseline-version 0.1.0` chặn trong CI |
| — | Phase 1, 2 vẫn giữ | 59 / 59 trong process và qua socket; FIXT 179 / 180; interop `libquickfix` 7 / 7; alloc 0 |

"Test pass" một mình chưa đủ: tiêu chí 6 là chạy thật với một engine khác qua socket thật;
tiêu chí 7 là chạy thật với gói tải từ crates.io, không phải từ đường dẫn trong repo.

## Tài liệu phải cập nhật

Theo bảng đồng bộ `CLAUDE.md` §4, khi từng bước giao:

- [ ] `PRD.md` §2 (đã sửa trong đề xuất này, trạng thái *Proposed*), §3 dòng xác thực (đã cũ) và
      dòng track record khi publish
- [ ] `DESIGN.md` §3 nếu thêm/đổi crate; D3 nếu nguồn từ điển đổi
- [ ] `CHANGELOG.md` và rustdoc — API công khai (`Decimal`), phiên bản `0.1.0`
- [ ] `docs/GUIDE.md` — người nhúng phải biết gì về credential và log
- [ ] `docs/CONFIGURATION.md` — nếu có khoá mới
- [ ] `docs/CONFORMANCE.md` — kết quả interop QuickFIX/J, kèm lệnh, máy, CI run id
- [ ] `docs/GETTING-STARTED.md`, `README.md` — `cargo add`
- [ ] `docs/internals/dict.md`, `codec.md`, `engine.md`, `tools.md`
- [ ] `docs/reference/` — mọi chỗ lệch Orchestra ↔ QuickFIX, mọi bẫy khi publish
- [ ] ADR mới: nguồn từ điển (bước 1); FIXP (bước 9, nếu chạy)
- [ ] `STATUS.md` — khi mỗi bước đóng

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Crate build xanh trong repo vì `vendor/` có sẵn, nhưng hỏng trên crates.io | tiêu chí 1 và 3 chạy trên máy **không** có `vendor/` |
| Từ điển Orchestra khác QuickFIX, còn 59 file `.def` viết theo QuickFIX → gate và sản phẩm đọc hai từ điển khác nhau | test so khớp ở bước 2 gọi tên từng chỗ lệch; 59 / 59 chạy lại |
| Mật khẩu nằm trong message log, hoặc trong journal (bản ghi Logon gửi đi / resend) | tiêu chí 4 grep cả hai file, chứng minh bằng đảo ngược |
| Che mật khẩu mà lại cấp phát (`String`, `format!`) trên đường ghi log | case alloc cho đường log, tiêm lỗi |
| `cargo publish` không nguyên tử: hỏng giữa chừng để lại nửa bộ crate trên crates.io | dry-run xanh trước; publish theo thứ tự phụ thuộc; ghi lại cách xử lý nếu hỏng giữa chừng |
| Tổ hợp feature người dùng chọn mà CI chưa build | dry-run chạy cả `--no-default-features` và từng feature công khai |
| Jar QuickFIX/J lọt vào commit | jar chỉ ở `vendor/`; kiểm `git status` trước `git add` |
| Interop TLS xanh nhưng thực ra rơi về userspace TLS | kiểm `Engine::tls_mode` / `TlsRequireKernel` trong bài interop |
| Một bản `0.1.0` bị publish nhầm thì không xoá được (chỉ yank) | publish do anh chạy, sau khi tiêu chí 1–6 xanh |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Orchestra lệch QuickFIX nhiều hơn mức gọi tên được | Trung bình | bước 1 chỉ đo; nếu lệch lớn, ADR chọn phương án kèm `NOTICE` và quay lại hỏi anh |
| Publish xong là cam kết API; tốc độ đổi API hiện nay (ADR-0048, 0054, 0088 đều thêm method bắt buộc) phải chậm lại | Trung bình | `0.1`, không `1.0`; semver-checks gọi tên từng lần phá |
| Người lạ gửi issue, hỏi hỗ trợ — dự án một người | Trung bình | README nói rõ mức hỗ trợ; không có trong tiêu chí |
| CI thêm JVM: chậm hơn, thêm chuỗi cung ứng | Thấp | fetch jar có checksum, như `vendor/` hiện nay |
| Phase 3 xong mà track record vẫn là 0 | Cao | chấp nhận: "có triển khai thứ hai" không đo bằng lệnh được; ghi vào `CHANGELOG` như điều kiện `1.0` |
| Người khác publish acceptor Rust có TLS và kiểm từ điển trước | Trung bình | lý do để bước 1–7 đi trước mọi thứ khác |

## Ngoài phạm vi

Cố tình **không** làm trong phase 3:

- **HA, replication, cluster, hot standby** — `PRD.md` §5 giữ nguyên. Muốn nhất quán tuyệt đối
  thì mỗi lần gửi phải chờ bản sao ack (một vòng mạng trong mọi con số độ trễ), hoặc chấp nhận
  mất message như chế độ mặc định của Chronicle. Cả hai đều ngược với định vị của ADR-0077.
- **Kernel bypass, Onload, `ef_vi`, AF_XDP, DPDK** — định vị là *on kernel TCP*; muốn làm phải
  thay ADR-0077 trước. Kể cả bài thử Onload chế độ copy (ADR-0074 cho phép) cũng không lên
  lịch — nó không phục vụ người dùng nào.
- **`io_uring` / `recvmmsg`** — giữ điều kiện của ADR-0074 quyết định 2.
- **SIMD** — ADR-0045.
- **FAST, FIXML, SBE bên trong FIXT** — ADR-0078.
- **Session FIXP** — chỉ spike trọng tài (bước 9), không xây session.
- **Store database** (kiểu `JdbcStore`) — `Journal` là trait; đó là crate của người dùng.
- **Metrics exporter, dashboard, web UI, binding ngôn ngữ khác** — `PRD.md` §5.
- **Bản `1.0`**.
- **Tắt có thứ tự cho bản sharded** (`STATUS.md` item 32) — vẫn là open item, cần thiết kế riêng.
- **Việc đo bench** (item 101 và tương tự) — vẫn là open item, không thuộc phase.

## Anh cần quyết

> **Đã quyết 2026-09-23** (anh trả lời trong hội thoại): **cả tám câu theo đúng khuyến nghị.**
> Q1 có; Q2 từ điển lấy từ FIX Orchestra, XML QuickFIX làm trọng tài — chỉ chốt hẳn sau spike đo
> độ lệch ở bước 1; Q3 `0.1.0`; Q4 FIXP chỉ spike trọng tài, ở bước cuối, sàn mục tiêu B3 Binary
> EntryPoint; Q5 anh tự chạy `cargo publish`; Q6 không thử Onload; Q7 "triển khai thứ hai" là
> điều kiện lên `1.0`, không phải tiêu chí thoát; Q8 publish `fixbolt-codec`, `fixbolt-dict`,
> `fixbolt-session`, `fixbolt-engine`, `fixbolt-sbe`, `fixbolt` — không publish
> `fixbolt-conformance`, `fixbolt-sbe-gen`. ADR-0097 chuyển sang *Accepted* cùng ngày.

1. **Chủ đề phase 3 là "người lạ dùng được" (publish + bảo mật tối thiểu + `Decimal` + interop
   engine thứ hai), thay vì FIXP, HA hay kernel bypass?**
   *Khuyến nghị: có.* Đây là thứ duy nhất đụng tới khoảng cách lớn nhất trong `PRD.md` §3, và
   mọi hạng mục đều đã có trọng tài để kiểm.
2. **Crate publish lấy từ điển từ đâu?** (a) file Orchestra Apache-2.0, giữ XML QuickFIX làm
   trọng tài; (b) kèm bảng sinh từ QuickFIX và thêm `NOTICE`; (c) bắt người dùng tự đưa XML.
   *Khuyến nghị: (a), nhưng chỉ chốt sau khi bước 1 đo độ lệch.* (c) làm hỏng trải nghiệm
   `cargo add`; (b) hợp lệ nhưng kéo điều khoản ghi công của QuickFIX vào mọi binary người dùng
   phát hành.
3. **Bản đầu là `0.1.0` hay `1.0`?** *Khuyến nghị: `0.1.0`.* `1.0` là lời hứa về một API chưa
   người lạ nào dùng. Điều kiện lên `1.0` ghi vào `CHANGELOG`: một triển khai bên ngoài, công
   khai, và một bản minor không phá API.
4. **Có mở nhánh FIXP có điều kiện không, và sàn mục tiêu là B3 Binary EntryPoint?**
   *Khuyến nghị: có, nhưng chỉ spike trọng tài ở bước cuối.* B3 công bố schema và Artio có
   acceptor chạy được — nhưng acceptor đó chỉ kiểm được **initiator** của fixbolt, lệch với định
   vị acceptor-first; và vẫn không có bộ test conformance. Nếu anh muốn giữ phase gọn, trả lời
   "không" — phase 3 vẫn đóng được đủ.
5. **Ai bấm `cargo publish`?** *Khuyến nghị: anh.* Token crates.io là của anh, và publish không
   xoá được. Manager chuẩn bị mọi thứ đến dry-run xanh rồi dừng.
6. **Bài thử Onload chế độ copy (ADR-0074 quyết định 1 nói nó thuộc phase 3) có đưa vào không?**
   *Khuyến nghị: không.* Không ra số, không phục vụ ai; để ADR-0074 giữ nó ở dạng "được phép,
   chưa lên lịch".
7. **"Có một triển khai thứ hai thật" có là tiêu chí thoát không?** *Khuyến nghị: không* —
   không lệnh nào chứng minh được, và nó nằm ngoài tầm tay dự án; nó là điều kiện `1.0`.
8. **Crate nào được publish?** *Khuyến nghị:* `fixbolt` (library) và các crate nó phụ thuộc —
   `fixbolt-codec`, `fixbolt-dict`, `fixbolt-session`, `fixbolt-engine`, `fixbolt-sbe`;
   **không** publish `fixbolt-conformance` (cần `vendor/`, là công cụ test) và `fixbolt-sbe-gen`
   (công cụ sinh code) trừ khi bước 6 cho thấy phải có.

## Nhật ký giao hàng

*(Chưa có — plan đã duyệt 2026-09-23, chưa bắt đầu xây.)*
