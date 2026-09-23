# Phase 3 bước 9: spike trọng tài FIXP (B3 Binary EntryPoint, Artio) và ADR FIXP

> **Loại:** Plan · **Ngày:** 2026-09-23 · **Trạng thái:** Đã duyệt (manager, 2026-09-23, theo mandate thường trực của owner)
> **Phạm vi:** phase 3, hàng 9 của [plan phạm vi phase 3](2026-09-23-phase-3-scope.md) —
> [ADR-0097](../decisions/ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md)
> quyết định 5 và câu Q4; cách dựng spike do
> [ADR-0140](../decisions/ADR-0140-the-fixp-spike-speaks-the-referees-own-schema-through-a-detached-probe-and-its-ci-job-blocks.md)
> (Proposed) quyết định. Nhánh `plan/p3-fixp-spike`, từ `main` `2f0a0dc`.

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

FIXP là tầng session của các giao thức nhị phân (B3 Binary EntryPoint, CME iLink 3): thay cho
Logon / ResendRequest của FIX tag=value là Negotiate → Establish → … → Terminate, và thân
message mã hoá bằng SBE. Phase 2 đã có bộ mã hoá SBE (`crates/sbe`, `crates/sbe-gen`) nhưng
**không có session nào cho nó** — [ADR-0078](../decisions/ADR-0078-sbe-enters-as-an-encoding-without-a-session-and-fixp-is-its-own-phase.md)
quyết định 2 hoãn FIXP cho tới khi có một sàn mục tiêu **và** một "trọng tài" (một bản cài
đặt khác, chạy được, để kiểm mình).

Anh đã chọn (ADR-0097 Q4): sàn mục tiêu là B3 Binary EntryPoint, và phase 3 **chỉ làm spike**:
chứng minh trọng tài chạy được headless trong CI, rồi viết ADR FIXP từ những gì spike thấy.
**Không xây session FIXP trong phase 3.**

Câu hỏi thật mà spike phải trả lời: *bộ SBE của mình có nói chuyện được với một peer FIXP có
thật không?* — tức là bảng sinh từ schema của sàn, bộ mã hoá của mình, và bộ giải mã của mình,
đặt đối diện một engine khác qua socket. Nếu câu này là "không", mọi thiết kế session phía trên
đều vô nghĩa.

## Những gì đã biết chắc

**Về trọng tài (Artio):**

- Artio hỗ trợ hai giao thức FIXP: iLink3 **chỉ vai initiator**, Binary EntryPoint **chỉ vai
  acceptor** — <https://github.com/artiofix/artio/wiki/FIXP-Support>. Tức là Artio chỉ kiểm
  được một **initiator** của fixbolt.
- Cấu hình acceptor: `configuration.acceptFixPProtocol(FixPProtocolType.BINARY_ENTRYPOINT)` và
  `fixPAuthenticationStrategy((context, authProxy) -> authProxy.accept())`; thư viện cần
  `FixPConnectionExistsHandler` và `FixPConnectionAcquiredHandler`; *"Acceptor only, doesn't
  support Engine owned sessions, sole library mode"* —
  <https://github.com/artiofix/artio/wiki/Binary-EntryPoint-Support>.
- Mẫu chính thức `FixPExchangeApplication` chạy **media driver của Aeron ngay trong tiến trình**
  (`ArchivingMediaDriver.launch`), không cần tiến trình driver riêng; Aeron Archive dùng hai cổng
  UDP localhost (`10010`, `10020`) — <https://github.com/artiofix/artio>, file
  `artio-samples/src/main/java/uk/co/real_logic/artio/example_fixp_exchange/FixPExchangeApplication.java`.
- `BinaryEntryPointContext` (thứ trọng tài nhận khi xác thực) có `sessionID()`,
  `sessionVerID()`, `enteringFirm()`, `credentials()`, `clientIP()`, `clientAppName()`,
  `clientAppVersion()` — trọng tài đọc lại được **mọi** trường mình gửi, kể cả bốn trường
  `varData`.
- Artio tự từ chối Negotiate có timestamp lệch khỏi cửa sổ (`INVALID_TIMESTAMP`), từ chối
  Establish sai timestamp hoặc `nextSeqNo`, và trả Terminate khi client gửi Terminate — file
  `artio-binary-entrypoint-impl/.../InternalBinaryEntryPointConnection.java`.
- Khung (framing) của B3: 4 byte — độ dài `uint16` **little-endian** (tính cả khung), rồi
  `0xEB50` little-endian — `artio-codecs/.../fixp/SimpleOpenFramingHeader.java` dòng 25–62. Khác
  SOFH chuẩn của FIXP (độ dài 4 byte, big-endian).
- CI của chính Artio build và chạy cả `artio-binary-entrypoint-system-tests` trên runner
  GitHub `ubuntu-24.04`, Java 17 và 25, mỗi lần khoảng 8 phút —
  <https://github.com/artiofix/artio/blob/master/.github/workflows/ci.yml> và
  `gh run list -R artiofix/artio` (đọc 2026-09-23). Tức là một acceptor Binary EntryPoint đã chạy
  được trên runner GitHub.
- Licence Artio: Apache-2.0 (GitHub API, đọc 2026-09-23). Artio yêu cầu Java ≥ 17
  (`build.gradle`: `sourceCompatibility = JavaVersion.VERSION_17`) và các cờ
  `--add-opens java.base/sun.nio.ch=ALL-UNNAMED`, `java.base/jdk.internal.misc`,
  `java.base/java.util.zip` (dòng 174–176). Desk có `openjdk 21.0.12`.

**Jar cần ghim** `[đo 2026-09-23]` — Maven Central, bản mới nhất `0.184` (cập nhật
2026-09-18); danh sách lấy đúng từ các file `.pom`; mỗi file đã so khớp với `.sha1` của Maven
Central; tổng 5,2 MB:

| Jar | SHA-256 |
|---|---|
| `uk.co.real-logic:artio-binary-entrypoint-impl:0.184` | `38f26226fc09e0b99f7a5a0108e04e9ca8fb1396b1817f94f38d42a408cc1c02` |
| `uk.co.real-logic:artio-binary-entrypoint-codecs:0.184` | `570c093e41a94ec2a1ff12631634c300a242f42cdb7d43b5d693ac736aba1931` |
| `uk.co.real-logic:artio-core:0.184` | `8deebebb80a19887e74e2272a53b6a19717d183c5ebbe26aa69010a041382d7c` |
| `uk.co.real-logic:artio-codecs:0.184` | `ceb176667681ccc78ad9ee5a6655627d4ea32c6faa069269248ee87780bcd85f` |
| `io.aeron:aeron-client:1.53.2` | `1386ce46b2bb89b4694a63c76b19e452808b6ee446380b54f2374b72a65e4ff8` |
| `io.aeron:aeron-driver:1.53.2` | `ab102b05f064fc394c661e0fba23c09817a47775d26a3b87fb331597b8a4ac28` |
| `io.aeron:aeron-archive:1.53.2` | `ca8f988ba875b7936fd01df1b77ff01fa168f49a21e24560c2acdf40b07193c1` |
| `io.aeron:aeron-annotations:1.53.2` | `e2e336ab3865e9fd6b08d6ca8202bda2364a8af30b48ec06cc2b1643bc3c0bc9` |
| `org.agrona:agrona:2.6.1` | `84c07eb02695c06bfda3d6c3a20c1984e3ff1bbd617b2f8e678140baf973fa28` |
| `uk.co.real-logic:sbe-tool:1.40.2` | `df2556a61a199383952030a6cd7fd2e3a82c3eb835bf4d5bc6fe3ca11295d058` |
| `org.hdrhistogram:HdrHistogram:2.2.2` | `22d1d4316c4ec13a68b559e98c8256d69071593731da96136640f864fa14fad8` |

**Về schema:**

- Schema mà trọng tài thật sự dùng nằm trong `artio-binary-entrypoint-codecs-0.184.jar`, đường
  dẫn `uk/co/real_logic/artio/entrypoint/binary_entrypoint.xml`: `id="1" version="5"
  semanticVersion="5.6"`, little-endian, ngày 2022-08-24, SHA-256
  `c31fcd6228e613fa6ee3af441832393a4a35c30263bb5cd80929f16529af4a71` — giống từng byte với bản
  trên `master` của Artio `[đo 2026-09-23]`.
- B3 hiện công bố `b3-entrypoint-messages-8.4.2.xml` (`version="6" semanticVersion="8.4.2"`) —
  <https://www.b3.com.br/en_us/solutions/platforms/puma-trading-system/for-developers-and-vendors/entrypoint/>.
  **Trang đó không ghi điều khoản hay licence nào cho file XML.** Vậy trọng tài chậm hơn sàn
  ba bản lớn.
- Trong schema: Negotiate (id 1), NegotiateResponse (2), NegotiateReject (3), Establish (4),
  EstablishAck (5), EstablishReject (6), Terminate (7), NotApplied (8), Sequence (9),
  FinishedSending / FinishedReceiving (10, 11), RetransmitRequest / Retransmission /
  RetransmitReject (12–14). `clientFlow` là hằng `IDEMPOTENT`, `serverFlow` là hằng
  `RECOVERABLE`.

**Về chuẩn FIXP:** bản 1.0 là Technical Standard; các message session chia theo giai đoạn
(khởi tạo: Negotiate…; gắn kết: Establish…; truyền: Sequence, RetransmitRequest…; tháo: Terminate;
kết thúc: FinishedSending / FinishedReceiving) —
<https://github.com/FIXTradingCommunity/fixp-specification>, file
`v1-0-STANDARD/doc/06SummaryOfSessionMessages.md`. *Recoverable* = đúng một lần, mất thì xin
gửi lại; *Idempotent* = nhiều nhất một lần, mất thì báo (NotApplied) — cùng repo,
`03CommonFeatures.md`. Licence của spec: CC BY-ND 4.0. Bản cài đặt tham chiếu Silverflash (Java,
Apache-2.0) không có commit nào từ 2020-10-13 — <https://github.com/FIXTradingCommunity/silverflash>.

**Về bộ sinh code của mình** `[đo 2026-09-23, desk tmt-B450-I-AORUS-PRO-WIFI, một crate nháp
ngoài repo gọi fixbolt_sbe_gen::generate ở commit 2f0a0dc]`:

```text
binary_entrypoint.xml: ERR schema error: constant '' is not an unsigned integer: cannot parse integer from empty string
b3-8.4.2.xml:          ERR schema error: constant '' is not an unsigned integer: cannot parse integer from empty string
```

Nguyên nhân: B3 đặt `presence="constant" valueRef="TimeUnit.NANOSECOND"` trên một `<type>`
**bên trong composite** (`UTCTimestampNanos.unit`), còn `sbe-gen` chỉ hiểu `valueRef` trên
`<field>`. Chuẩn SBE 1.0 tự mâu thuẫn ở đây: bảng thuộc tính (`04MessageSchema.md:449-464`) và
`sbe.xsd:272,368` chỉ cho `valueRef` trên field, nhưng chính ví dụ của chuẩn
(`02FieldEncoding.md:884`) đặt nó trên `<type>`; `sbe-tool` của Real Logic chấp nhận
(`EncodedDataType.java:81-130`, <https://github.com/aeron-io/simple-binary-encoding>). Thay ba chỗ
đó bằng hằng số trong bản nháp thì schema của Artio **sinh được trọn** (`OK, 121134 bytes
generated`); schema 8.4.2 vấp tiếp lỗi thứ hai: `presence` trên field có kiểu composite (12
field, trong đó có `NegotiateResponse.semanticVersion`).

**Code có sẵn:** `crates/sbe/src/encode.rs` có `MessageWriter` (ghi field, group, `var_data`)
và `crates/sbe/src/view.rs` có `SbeView::decode` — đủ cho bốn message ra, bốn loại vào. Mẫu
"trọng tài Java của mình + script ghim jar" là `scripts/interop-qfj.sh` và
`tools/interop-qfj/Judge.java` (phase 3 hàng 5, ADR-0130). Mẫu "spike tách khỏi workspace" là
`spikes/ktls` ([ADR-0018](../decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md),
`Cargo.toml` `exclude`).

**Tìm mà không thấy:** client Binary EntryPoint nào của Artio được phát hành để dùng (chỉ có
`BinaryEntryPointClient`, một lớp phụ trợ JUnit); điều khoản cho file schema của B3; bộ test
conformance FIXP công khai. Bản cài đặt FIXP mã nguồn mở khác: `fefixp` 0.1.0 và `rustyfixp`
0.7.4 (Rust, tài liệu phủ 5,56 %, chỉ có struct message) — <https://docs.rs/fefixp>,
<https://docs.rs/rustyfixp>; B3EntryPointClient (C#, MIT, 0 sao, schema 8.4.2, có một "test
peer" trong tiến trình) — <https://github.com/pedrosakuma/B3EntryPointClient>. Không thấy bản Go
hay C++ nào. CME iLink 3 cũng là FIXP nhưng ký Negotiate / Establish bằng HMAC và chỉ kiểm được
qua môi trường chứng nhận của CME —
<https://cmegroupclientsite.atlassian.net/wiki/spaces/EPICSANDBOX/pages/714145834/iLink+Binary+Order+Entry+-+Session+Layer>.

## Cách làm

Chỉ phương án đã chọn; các phương án bị loại nằm trong ADR-0140.

1. **Sửa một chỗ trong `sbe-gen`**: hiểu `valueRef` trên `<type>` trong composite; lỗi khi
   `valueRef` không giải được phải nói "valueRef", không nói "constant ''". Test bằng một schema
   nhỏ **tự viết** (không chép schema của B3 hay của chuẩn).
2. **Script `scripts/fixp-spike.sh`**: tải 11 jar trên, so SHA-256, rút `binary_entrypoint.xml`
   ra `vendor/fixp/` và so SHA-256 của nó; biên dịch trọng tài; chạy từng "arm"; đọc **dòng in
   ra**, không đọc exit code; cuối cùng kiểm `git status` không thêm gì.
3. **Trọng tài `spikes/fixp-probe/referee/Referee.java`** — code của mình, chỉ dùng API công
   khai của Artio: acceptor Binary EntryPoint, media driver trong tiến trình, xác thực bằng cách
   so từng trường của `BinaryEntryPointContext` với giá trị mong đợi nhận qua dòng lệnh, in một
   dòng mỗi trường, **từ chối** nếu lệch.
4. **Probe `spikes/fixp-probe`** — crate Rust tách khỏi workspace (bảng `[workspace]` rỗng, thêm
   vào `exclude` ở `Cargo.toml` gốc), `build.rs` sinh bảng từ `vendor/fixp/binary_entrypoint.xml`
   bằng `fixbolt-sbe-gen`, mã hoá bằng `fixbolt-sbe`. Một chương trình chạy thẳng, **không phải
   session**: Negotiate → đọc NegotiateResponse → Establish → đọc EstablishAck → Terminate → đọc
   Terminate → đọc EOF; mỗi bước so từng trường và in `<bước> ok` hoặc `<bước> FAIL: <vì sao>`.
   Mọi lần đọc có hạn chót. Khung 4 byte của B3 do probe tự ghi.
5. **Ba arm**: `accept` — năm bước tên `negotiate` (gửi Negotiate, nhận NegotiateResponse đúng
   `sessionID`, `sessionVerID`, `enteringFirm`, `requestTimestamp`), `establish` (gửi Establish,
   nhận EstablishAck đúng `nextSeqNo`, `lastIncomingSeqNo`), `terminate` (gửi Terminate),
   `echo` (nhận Terminate cùng `terminationCode`), `eof` (trọng tài đóng socket), tất cả `ok`; `reject-timestamp` (Negotiate với timestamp
   lệch 1 giờ → phải nhận `NegotiateReject` mã `INVALID_TIMESTAMP`); `reject-credentials`
   (credentials khác giá trị trọng tài chờ → phải nhận `NegotiateReject`). Dòng tổng kết:
   `fixp-spike: accept PASS 5/5, reject-timestamp PASS, reject-credentials PASS`.
6. **Job CI `fixp-spike`, chặn merge** (lý do: ADR-0140 quyết định 5).
7. **ADR FIXP** viết từ kết quả spike, trả lời các câu ở hàng 5 dưới đây.

File tạo: `scripts/fixp-spike.sh`, `spikes/fixp-probe/{Cargo.toml,Cargo.lock,build.rs,src/main.rs,README.md}`,
`spikes/fixp-probe/referee/Referee.java`, một test mới trong `crates/sbe-gen/tests/`, một trang
mới trong `docs/reference/`, ADR FIXP. File sửa: `crates/sbe-gen/src/generator.rs`,
`Cargo.toml` gốc (chỉ dòng `exclude`), `.github/workflows/ci.yml` (chỉ thêm job), `README.md`
(layout), `docs/internals/tools.md`, `docs/internals/sbe.md`, `docs/CONFORMANCE.md`,
`CHANGELOG.md`, `STATUS.md` (manager).

## Bất biến bị đụng tới

Việc này đụng `sbe-gen` (sinh bảng cho codec SBE), không đụng `codec`, `session`, `engine`,
`transport`.

- **5 — thứ tự field từ bảng sinh ra**: probe ghi mọi field qua layout sinh từ schema, không có
  offset viết tay; ngoại lệ duy nhất là khung 4 byte của B3, vốn không phải message, và có dẫn
  nguồn. Sửa `sbe-gen` giữ nguyên mọi bảng đang sinh: toàn bộ test `fixbolt-sbe` và
  `fixbolt-sbe-gen` (gồm ba dump của spec và ví dụ `Car`) phải xanh không sửa.
- **7 — không `unwrap`/`expect`/`panic!` trong crate thư viện**: áp cho phần sửa `sbe-gen`.
  Probe là spike, không phải thư viện, nhưng chép bảng lint của workspace để clippy thấy.
- **6 — cờ tính năng / không phụ thuộc tuỳ chọn**: probe nằm ngoài workspace, nên `cargo test
  --all`, `--no-default-features` và `scripts/check-no-optional-deps.sh` không thấy nó; gate
  chạy lại để chứng minh.
- **9 (tinh thần) — không chép mã/tài liệu bên thứ ba vào repo**: trọng tài là code của mình;
  schema của B3 chỉ nằm trong `vendor/` (đã gitignore), không bao giờ `git add`.
- **1**: `crates/sbe` không đổi; nếu hàng 2 buộc phải đổi `crates/sbe`, bench `alloc.rs` của nó
  phải chạy lại và đọc 0.
- **10**: spike không công bố con số hiệu năng nào. Thời gian chạy job chỉ để biết chi phí CI,
  ghi kèm run id, không phải số đo latency.
- 2, 3, 4, 8: không đụng (không session, không engine, không `unsafe`).

## Chia việc

Mỗi hàng một commit xanh trên `plan/p3-fixp-spike`; manager chạy lại gate và commit. Hàng 1 và
2 song song được (file rời nhau); 3 sau cả hai; 4 sau 3; 5 sau 4.

| Bước | Kết quả — file tạo / sửa (và **không** đụng) | Người làm | Gate | Xong khi | Reversal | Phụ thuộc |
|---|---|---|---|---|---|---|
| 1 | **Ghim + trọng tài chạy một mình.** Tạo `scripts/fixp-spike.sh` §1 (11 jar, SHA-256 ở bảng trên; rút schema, so `c31fcd62…4a71`), §2 (biên dịch `Referee.java`), arm `referee-only` (trọng tài khởi động, in `referee: listening on 127.0.0.1:<port>`, tự tắt sau hạn chót, in `referee: shutdown ok`). Tạo `spikes/fixp-probe/referee/Referee.java`. **Không đụng** `crates/**`, `tools/**`, `.github/**`, `Cargo.toml` | developer (sonnet) | `FIXP_SPIKE_ARMS=referee-only scripts/fixp-spike.sh`, chạy **hai lần liền** | hai lần đều có `listening`, `shutdown ok`, `==> the run added nothing git can see`; ghi thời gian mỗi lần | Sửa một chữ số trong SHA-256 ghim của `artio-core` → `CHECKSUM MISMATCH: artio-core-0.184.jar` trước khi chạy gì; sửa SHA-256 schema → `CHECKSUM MISMATCH: binary_entrypoint.xml` | plan duyệt |
| 2 | **`sbe-gen` hiểu `valueRef` trên `<type>` trong composite.** Test đỏ trước: `crates/sbe-gen/tests/value_ref_on_composite_member.rs` với schema tự viết (một enum `TimeUnit`, một composite có `unit` là hằng `valueRef`), khẳng định bảng sinh ra có hằng đúng giá trị và 0 byte trên dây; một test nữa cho `valueRef` trỏ tới enum không có → lỗi có chữ `valueRef`. Sửa `crates/sbe-gen/src/generator.rs`. Trang mới `docs/reference/sbe-valueref-on-a-composite-member.md` (chuẩn tự mâu thuẫn, dẫn các dòng ở trên, test canh). Sửa `docs/internals/sbe.md`, `CHANGELOG.md`. **Không đụng** `crates/sbe/**`, mọi test đang có | **senior developer (opus)** — bảng sinh ra là D3, sai là sai bố cục mọi schema | `cargo test -p fixbolt-sbe-gen`; `cargo test -p fixbolt-sbe`; `cargo test --all`; `cargo test --all --no-default-features`; `cargo clippy --all-targets -- -D warnings`; `cargo fmt --all --check`; và (sau hàng 1) crate nháp ngoài repo gọi `generate` trên `vendor/fixp/binary_entrypoint.xml` | test mới xanh, mọi test cũ xanh không sửa; crate nháp in `OK` | Bỏ nhánh `valueRef` vừa thêm → test mới đỏ với đúng câu `constant '' is not an unsigned integer` (viết câu này ra trước khi chạy) | plan duyệt |
| 3 | **Probe + ba arm.** Tạo `spikes/fixp-probe/{Cargo.toml,Cargo.lock,build.rs,src/main.rs,README.md}`; sửa `Cargo.toml` gốc (thêm `"spikes/fixp-probe"` vào `exclude`, kèm chú thích như `spikes/ktls`); thêm §3–§5 vào `scripts/fixp-spike.sh` (arm `accept`, `reject-timestamp`, `reject-credentials`, dòng tổng kết). Trọng tài in lại từng trường nhận được; script so với giá trị probe đã gửi. **Không đụng** `crates/**` — nếu `fixbolt-sbe` thiếu gì, **dừng và báo** | **senior developer (opus)** — lần đầu mã hoá một schema sàn thật đối diện engine khác; kết luận của ADR FIXP đứng trên hàng này | `scripts/fixp-spike.sh` (đủ arm), hai lần liền; `cargo clippy --manifest-path spikes/fixp-probe/Cargo.toml --all-targets -- -D warnings`; `cargo fmt --manifest-path spikes/fixp-probe/Cargo.toml --check`; `cargo test --all`; `cargo test --all --no-default-features`; `scripts/check-no-optional-deps.sh` | dòng `fixp-spike: accept PASS 5/5, reject-timestamp PASS, reject-credentials PASS` cả hai lần; bảy dòng `referee: field <tên> ok`; `the run added nothing git can see` | **A**: ghi `0xEB51` thay `0xEB50` trong khung → `accept: negotiate FAIL` (ghi nguyên văn trọng tài làm gì — đóng socket hay im lặng). **B**: đổi một byte `clientAppVersion` probe gửi (giá trị mong đợi của trọng tài giữ nguyên) → `referee: field clientAppVersion MISMATCH` và `accept: negotiate FAIL: NegotiateReject`. **C**: probe chờ `EstablishAck.nextSeqNo == 2` → `accept: establish FAIL: nextSeqNo 1 != 2` (chứng minh probe thật sự đọc). Viết ba câu mong đợi ra trước, rồi khôi phục và xanh lại | 1, 2 |
| 4 | **Job CI `fixp-spike`, chặn merge.** Sửa `.github/workflows/ci.yml` (chỉ thêm job: `setup-java` temurin 21, chạy script, grep dòng tổng kết, transcript lên `$GITHUB_STEP_SUMMARY`, `timeout-minutes` có đặt). Sửa `README.md` (layout: `spikes/fixp-probe/`), `docs/internals/tools.md` (hàng mới), `CHANGELOG.md`. **Không đụng** job khác | developer (sonnet) | `python3 scripts/check-links.py`; push, PR nháp, CI | job `fixp-spike` xanh trên chính commit đó; ghi run id và thời gian job | **G**: đổi grep tổng kết trong job thành `accept PASS 6/5` → job đỏ ở bước kiểm dòng tổng kết (chạy trên nhánh rồi revert) | 3 |
| 5 | **Công bố + ADR FIXP.** `docs/CONFORMANCE.md` mục mới *FIXP spike against Artio* (lệnh, máy, JDK, run id, output nguyên văn của CI, cái chưa chứng minh); trang `docs/reference/b3-binary-entrypoint-facts.md` (khung LE, độ lệch schema 5.6 / 8.4.2, cửa sổ timestamp và giới hạn keep-alive của Artio, mỗi điều kèm arm canh nó); **ADR FIXP** (số trống kế tiếp ≥ 0141) trả lời: (i) cái gì thay non-negotiable 3 cho session không có corpus; (ii) vai nào trước — initiator (có trọng tài) hay acceptor (đúng định vị ADR-0077, chưa có trọng tài đủ tin); (iii) nhắm schema nào — 5.6 của trọng tài hay 8.4.2 của sàn (cần sửa `sbe-gen` lần hai, mở rộng ADR-0081 quyết định 5); (iv) khung SOFH nằm ở tầng nào; (v) máy trạng thái thuần có hợp với idempotent/recoverable không (D1, `Input::Tick`); (vi) số phận job `fixp-spike`; (vii) session FIXP thuộc phase nào — đề xuất, anh quyết | developer (sonnet) cho hai trang docs; **architect** cho ADR FIXP; manager viết `STATUS.md` | `python3 scripts/check-links.py` | mục CONFORMANCE trích log **của CI** kèm run id của commit đóng; ADR FIXP ở trạng thái Proposed, chờ anh | — | 4 |

## Cách kiểm chứng

- **Hàng 1**: trọng tài khởi động thật trên desk, hai lần liền (lần hai chứng minh không còn rác
  Aeron từ lần một). Checksum đỏ trước khi có gì chạy.
- **Hàng 2**: test đỏ trước với đúng câu lỗi hiện nay, rồi xanh; mọi test SBE cũ xanh không sửa;
  schema thật của trọng tài sinh được bảng.
- **Hàng 3**: đây là bằng chứng chính. Hai chiều: trọng tài (giải mã bằng code Real Logic sinh)
  đọc lại đúng bảy trường mình gửi; probe (giải mã bằng bảng của mình) đọc đúng các trường Artio
  trả. Hai arm từ chối chứng minh trọng tài biết nói "không". Ba reversal chứng minh mỗi chiều
  thật sự được kiểm.
- **Hàng 4**: CI xanh trên commit đó, có run id; reversal G đỏ trên nhánh.
- "Test pass" một mình không đủ: mọi kết luận trích **dòng in ra** của script, nguyên văn.
  Không có bản ghi sàn thật nào — spike này không thay chứng nhận của B3, và ADR FIXP phải nói
  rõ điều đó.

## Tài liệu phải cập nhật

Theo bảng đồng bộ ở `CLAUDE.md` §4.

- [ ] `README.md` layout — `spikes/fixp-probe/` (hàng 4)
- [ ] `docs/internals/tools.md` — hàng cho spike (hàng 4); `docs/internals/sbe.md` — `valueRef`
      trên `<type>` (hàng 2)
- [ ] `CHANGELOG.md` — sửa `sbe-gen` (hàng 2), job mới (hàng 4)
- [ ] `docs/reference/` — hai trang mới, mỗi bẫy kèm test hoặc arm canh (hàng 2, 5)
- [ ] `docs/CONFORMANCE.md` — kết quả spike với lệnh, máy, run id (hàng 5)
- [ ] `docs/decisions/` — ADR-0140 (Accepted khi duyệt plan này); ADR FIXP (hàng 5)
- [ ] `STATUS.md` — manager, khi đóng plan; `PRD.md` §2 *Phase 3* chỉ đổi nếu ADR FIXP chuyển
      việc giữa các phase
- [ ] `DESIGN.md` — không đổi (không crate mới trong workspace, không đổi hành vi codec hay
      session); nếu ADR FIXP được duyệt thì plan sau mới sửa

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| `sbe-gen` không hiểu `valueRef` trên `<type>` — đã đo, lỗi gây hiểu lầm (`constant ''`) | test mới ở hàng 2, reversal hàng 2 |
| Khung B3 là little-endian, độ dài 2 byte — không phải SOFH chuẩn | reversal A hàng 3 |
| Schema lệch: trọng tài 5.6 (`version 5`), sàn 8.4.2 (`version 6`) — lấy nhầm bản là lệch bố cục | SHA-256 schema ghim trong script (hàng 1) |
| Schema của B3 lọt vào git | `vendor/` đã gitignore; kiểm `the run added nothing git can see` cuối script |
| Artio từ chối timestamp ngoài cửa sổ | arm `reject-timestamp`; arm `accept` dùng đồng hồ thật lúc gửi |
| `keepAliveInterval` ngoài giới hạn min/max của Artio → EstablishReject | trọng tài in giới hạn đang dùng; arm `accept` |
| Thư mục Aeron / archive còn sót từ lần chạy trước | chạy hai lần liền ở hàng 1 và 3; `dirDeleteOnStart(true)`, thư mục riêng cho mỗi lần |
| Cổng UDP 10010 / 10020 của Archive bị chiếm | cổng đọc từ biến môi trường, script in ra cổng đã dùng |
| Thiếu cờ `--add-opens` trên Java ≥ 17 | arm `referee-only` |
| Probe đọc không hạn chót → treo CI | mọi lần đọc có deadline; job có `timeout-minutes` |
| Crate probe lọt vào workspace (hợp nhất feature, `vendor/` bắt buộc cho `cargo test --all`) | `[workspace]` rỗng + `exclude`; gate hàng 3 chạy `cargo test --all` và `--no-default-features` ở gốc |
| Lint của crate tách rời không ai thấy | `cargo clippy --manifest-path …` trong gate hàng 3 và trong job |
| Đọc exit code thay vì dòng in ra | script chỉ tin dòng tổng kết; reversal G |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Aeron trên runner GitHub cần `/dev/shm` và hai cổng UDP; có thể chập chờn | Trung bình | CI của Artio chạy được trên `ubuntu-24.04`; nếu chập chờn, ghi nguyên văn vào `docs/reference/` và báo — không nới gate |
| Job chặn merge phụ thuộc Maven Central | Thấp | jar ghim SHA-256; cùng chi phí đã nhận cho QuickFIX/J (ADR-0130) |
| Xanh chỉ nghĩa là "nói được Binary EntryPoint của Artio", không phải của B3 hôm nay | Cao | chấp nhận; ADR FIXP phải nói rõ, và nêu điều kiện để nhắm 8.4.2 |
| Spike phình thành session (thêm Sequence, retransmit, máy trạng thái) | Trung bình | probe chạy thẳng, không kiểu trạng thái; mọi thứ ngoài năm bước là "Ngoài phạm vi" |
| Hàng 2 lộ thêm chỗ `sbe-gen` không hiểu trong schema 5.6 (bản nháp chỉ thay đúng ba chỗ) | Thấp | đã đo: sau ba chỗ đó sinh được trọn; nếu khác, dừng và báo — không sửa thêm trong plan này |
| `sbe-tool` hay jar nào đó hoá ra không cần | Thấp | vẫn ghim đúng bộ mà `.pom` khai — không đoán bớt |

## Ngoài phạm vi

- Session FIXP (máy trạng thái thuần, Sequence / keep-alive, RetransmitRequest, NotApplied,
  FinishedSending / FinishedReceiving) — ADR-0097 quyết định 5.
- Message nghiệp vụ (SimpleNewOrder, ExecutionReport…).
- Schema 8.4.2 và lỗi `sbe-gen` thứ hai (`presence` trên field composite) — đầu vào cho ADR FIXP.
- Đưa khung SOFH vào `crates/sbe` hay `transport`.
- Vai acceptor của fixbolt trong FIXP; iLink 3; TLS; macOS.
- Bất kỳ con số hiệu năng nào.
- Trọng tài thứ hai (B3EntryPointClient C#, `fefixp`, `rustyfixp`) — chỉ ghi nhận cho ADR FIXP.

## Nhật ký giao hàng

*(Chưa có — plan chờ duyệt.)*
