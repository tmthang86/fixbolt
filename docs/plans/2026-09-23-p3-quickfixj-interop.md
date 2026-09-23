# Phase 3, bước 5: interop với QuickFIX/J, hai vai, plaintext và TLS

> **Loại:** Plan · **Ngày:** 2026-09-23 · **Trạng thái:** Đã duyệt (manager, 2026-09-23, theo mandate thường trực của owner)
> **Phạm vi:** phase 3, hàng 5 của *Chia việc* trong [phase-3-scope](2026-09-23-phase-3-scope.md);
> tiêu chí thoát 6 của [ADR-0097](../decisions/ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md);
> quyết định kỹ thuật ở [ADR-0130](../decisions/ADR-0130-a-jvm-enters-ci-as-a-second-oracle-quickfixj-by-pinned-jar-and-our-own-judge.md) (Proposed)

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Hiện nay chỉ có **một** engine khác từng nói chuyện với fixbolt qua socket thật: `libquickfix`
(C++), trong job CI `interop`. Job đó chạy plaintext thôi (`libquickfix` được build với
`-DHAVE_SSL=OFF`). Phần TLS của fixbolt (rustls bắt tay, rồi giao khoá cho kernel — kTLS) chưa
bao giờ gặp một bên TLS nào ngoài chính nó: mọi test trong `crates/engine/tests/tls*.rs` đều là
rustls ở cả hai đầu.

ADR-0097 tiêu chí 6 đòi: **QuickFIX/J, hai vai, 7 / 7, plaintext và TLS, là job CI chặn merge.**
Đây là "ý kiến độc lập" theo nghĩa của ADR-0042: một bản cài đặt khác đọc byte của mình và nói
đồng ý hay không. Người lạ chạy fixbolt sẽ gặp QuickFIX/J ở đầu bên kia thường hơn gặp bất kỳ
engine nào khác trong thế giới Java.

Kết quả mong muốn: một script `scripts/interop-qfj.sh`, một job CI `interop-qfj` chặn merge, in
ra bốn dòng `PASS 7/7` — fixbolt làm acceptor (plaintext, TLS) và làm initiator (plaintext, TLS) —
và chứng minh TLS thật sự chạy trong kernel, không lặng lẽ rơi về userspace.

## Những gì đã biết chắc

**Về khuôn có sẵn (C++ `libquickfix`):**

- `scripts/interop.sh` §1–§5: build `libquickfix` ở commit ghim, chạy hai đầu, **đọc output
  chứ không đọc mã thoát** — grep từng dòng `^interop-acceptor: <bước> +ok` cho bảy tên bước
  *và* dòng `PASS 7/7`; chờ dòng `acceptor: ready` / `interop: listening` thay vì `sleep`; mỗi lần
  chờ có hạn (`DEADLINE=20`); dừng acceptor của fixbolt bằng một dòng `stop` vào stdin
  (`Admin::shutdown`), rồi kiểm dòng `interop: acceptor stopped: Shutdown {`; cuối cùng so ảnh
  `git status` trước/sau: **"the run added nothing git can see"**.
- Bảy bước của chiều "C++ initiator vào acceptor fixbolt" (plan
  [2026-09-03-acceptor-interop](2026-09-03-acceptor-interop.md) *Cách làm*):
  `logon`, `order`, `heartbeat`, `testrequest`, `resend`, `gapfill`, `logout`. Bên C++ vừa là đối
  tác vừa là **trọng tài**, chấm trên **chuỗi thô** ghi bằng một `Log` tự viết (vì QuickFIX nuốt
  bản `43=Y` trước khi tới application).
- Chiều ngược lại (fixbolt initiator) dùng **session thuần** trên `TcpStream`, không qua engine
  (ADR-0042 quyết định 4). Đường đó không có TLS.
- `tools/interop --role acceptor` gọi `fixbolt::serve` với `desk::Desk`: gửi hai `35=B` khi logon,
  trả `35=8` cho mỗi `35=D` và **echo `11=`** (`tools/interop/src/desk.rs:65-117`).

**Về TLS của fixbolt** (`crates/engine/src/tls.rs`, `docs/CONFIGURATION.md` §1 bảng TLS):

- Chỉ TLS 1.3, chỉ `TLS13_AES_128_GCM_SHA256` (`server_config`, `offloadable_provider`).
- Acceptor không đòi chứng chỉ client. Initiator luôn kiểm chứng chỉ server, chỉ với
  `CertificationAuthoritiesFile`; địa chỉ IP cần IP SAN.
- Khoá cấu hình: acceptor `SocketUseSSL`, `ServerCertificateFile`, `ServerCertificateKeyFile`,
  `TlsRequireKernel`; initiator `SocketUseSSL`, `CertificationAuthoritiesFile`, `TlsRequireKernel`.
  `Settings::into_table` từ chối file có `SocketUseSSL=Y` (`NeedsTlsDoor`); cửa TLS là
  `into_tls_table` + `tls::load_pem` + `serve_tls_requiring`, và phía initiator
  `into_tls_initiator` + `tls::load_client_pem` + `connect_and_serve_tls`
  (`crates/engine/src/lib.rs:1982`, `:2341`).
- Rơi về userspace thì engine phát `EventKind::TlsFellBackToUserspace`
  (`crates/engine/src/observe.rs:678`), và với `TlsRequireKernel=Y` thì cắt kết nối (ADR-0060).
- Job CI `tls` kiểm trước là runner có kTLS (`scripts/check-ktls-available.sh` phải in `READY`),
  báo lỗi "môi trường" bằng lời riêng. **Máy desk hôm nay in `READY`**, kernel
  `7.0.0-31-generic`, `/proc/net/tls_stat` có mặt (đọc 2026-09-23).

**Về QuickFIX/J** (nguồn đầy đủ ở ADR-0130 *Research*):

- Bản mới nhất **3.0.2**, lên Maven Central 2026-08-04
  (<https://repo1.maven.org/maven2/org/quickfixj/quickfixj-core/maven-metadata.xml>).
- Cần năm jar: `quickfixj-core`, `quickfixj-base`, `quickfixj-messages-fix44` (có sẵn
  `FIX44.xml` bên trong), `mina-core` 2.2.9, `slf4j-api` 2.0.18; bytecode Java 8
  (<https://repo1.maven.org/maven2/org/quickfixj/quickfixj-parent/3.0.2/quickfixj-parent-3.0.2.pom>).
- Khoá SSL: `SocketUseSSL`, `SocketKeyStore`, `SocketKeyStorePassword`, `KeyStoreType`,
  `SocketTrustStore`, `SocketTrustStorePassword`, `TrustStoreType`, `NeedClientAuth`,
  `EnabledProtocols`, `CipherSuites`, `EndpointIdentificationAlgorithm`
  (<https://quickfixj.org/docs/configuration/>). Mặc định store là `JKS`, không kiểm tên máy
  (<https://github.com/quickfix-j/quickfixj/blob/master/quickfixj-core/src/main/java/quickfix/mina/ssl/SSLSupport.java>).
  Initiator QFJ **luôn** kiểm chứng chỉ acceptor → cần trust store chứa CA của mình.
- Mặc định đáng để ý: `CheckLatency=Y` (`MaxLatency=120` giây), `TimeStampPrecision=MILLIS`,
  `ValidateFieldsOutOfOrder=Y`, `AllowUnknownMsgFields=N`, `ValidateUserDefinedFields=Y`,
  `ResetOnLogon=N`.
- Giấy phép: *The QuickFIX Software License 1.0*; nghĩa vụ ghi công chỉ phát sinh khi **phân
  phối lại** (<https://raw.githubusercontent.com/quickfix-j/quickfixj/master/LICENSE>). Mình chỉ
  tải jar vào `vendor/` lúc test, không phân phối → theo ADR-0001 quyết định 5, **không cần
  `NOTICE`**.
- **Máy desk chưa có JDK** (`java: command not found`, 2026-09-23). Gói có sẵn:
  `openjdk-21-jdk-headless` 21.0.12. `openssl` có sẵn ở `/usr/bin/openssl`; `keytool` đi kèm JDK.
- Tìm mà **không thấy gì**: báo cáo QuickFIX/J chạy với engine Rust nào; báo cáo JSSE nói chuyện
  với một đầu kTLS; lỗi interop riêng của QFJ về `SendingTime` / `CheckLatency`.

## Cách làm

### Tổng thể

```text
scripts/interop-qfj.sh
  §1  tải 5 jar đã ghim (SHA-256) vào vendor/quickfixj/, javac Judge.java
  §2  sinh chứng chỉ cho lần chạy này (openssl + keytool) vào vendor/interop-qfj-run/pki/
  §3  bốn lần chạy, mỗi lần: một bên fixbolt + một bên Java (Judge), đọc output
        qfj-acceptor-plain   Judge làm initiator  → fixbolt acceptor (serve)
        qfj-acceptor-tls     Judge làm initiator  → fixbolt acceptor (serve_tls_requiring)
        qfj-initiator-plain  fixbolt initiator (connect_and_serve)     → Judge làm acceptor
        qfj-initiator-tls    fixbolt initiator (connect_and_serve_tls) → Judge làm acceptor
  §4  grep từng bước, từng khẳng định phụ; đọc /proc/net/tls_stat trước/sau mỗi lần TLS
  §5  git status không có gì mới; dòng tổng kết
```

Tên dòng output theo **vai của fixbolt**, giống khuôn cũ (`interop-acceptor:` nghĩa là fixbolt
làm acceptor).

### Bên Java: `tools/interop-qfj/Judge.java` — code của mình, một file

- Chỉ gọi API công khai của QFJ, không chép một dòng nguồn QFJ (bất biến 9). Giấy phép theo
  workspace. Biên dịch bằng `javac` với classpath là năm jar; **không Maven, không Gradle,
  không `pom.xml`**.
- Cách gọi: `java -cp <jars>:<classes> Judge <initiator|acceptor> <file.cfg> <nhãn> [--invert-resend]`.
- Một `quickfix.LogFactory` + `quickfix.Log` tự viết (`RawLog`): ghi mọi chuỗi thô
  `onIncoming` / `onOutgoing` vào danh sách có khoá, in ra stdout dạng `qfj: in|out <chuỗi, SOH→|>`,
  và in `qfj: error <...>` cho `onErrorEvent`. **Mọi bước chấm trên chuỗi thô này.**
- Vai `acceptor`: `SocketAcceptor.start()`, rồi tự mở một kết nối TCP thử tới cổng của mình,
  thấy được nhận mới in `interop-qfj: ready` (dòng sẵn sàng là quan sát, không phải lời hứa —
  `CLAUDE.md` §10).
- Bảy bước, mỗi bước có hạn, poll danh sách mỗi 10 ms, in
  `<nhãn>: <bước> ok|FAIL  <đã thấy gì>`, cuối cùng `<nhãn>: PASS n/7`:

| # | Bước | Judge làm gì | Đạt khi thấy trên dây (chuỗi thô) | Hạn |
|---|---|---|---|---|
| 1 | `logon` | vai initiator: `start()` với `ResetOnLogon=Y`; vai acceptor: chờ `onLogon` | có `35=A` **từ** fixbolt, đúng `49=`/`56=`; vai initiator thì thêm `141=Y` trong câu trả lời; ghi lại `108=` | 5 s |
| 2 | `order` | gửi hai `35=D`, `11=QFJ-ORD-1`, `QFJ-ORD-2` (vai acceptor cũng gửi — FIX không cấm hướng) | đúng hai `35=8`, mỗi cái đúng `11=`; **ghi lại `34=` của từng cái** | 5 s |
| 3 | `heartbeat` | không làm gì | một `35=0` **không có `112=`** trong `2 × 108= + 1` giây, `108=` đọc từ `35=A` của fixbolt ở bước 1 | tính từ `108=` |
| 4 | `testrequest` | gửi `35=1 112=QFJ-TR-1` | `35=0 112=QFJ-TR-1` | 5 s |
| 5 | `resend` | gửi `35=2 7=a 16=b`, `a`,`b` là hai số ở bước 2 (cờ `--invert-resend` đảo thành `7=b 16=a`) | **đúng hai** `35=8` với `43=Y`, `122=`, ở `34=a` và `34=b` | 5 s |
| 6 | `gapfill` | `Session.setNextSenderMsgSeqNum(n + 3)`, gửi `35=1 112=QFJ-TR-2`; thấy `35=2` rồi gửi `112=QFJ-TR-3` | fixbolt gửi `35=2` với `7=<số nó chờ>`; QFJ trả `35=4 123=Y`; sống sót đo bằng `35=0 112=QFJ-TR-3` | 8 s |
| 7 | `logout` | `Session.logout()` | `35=5` từ fixbolt | 5 s |

Đây đúng là bảy bước của `interop-acceptor:` bên C++, **giống nhau ở cả hai vai**. Khác khuôn
cũ một chỗ, có chủ ý: khi fixbolt làm initiator, trọng tài vẫn là bên Java (làm acceptor), còn
fixbolt chạy **cửa initiator thật của engine** (`connect_and_serve` / `connect_and_serve_tls`) với
`Desk`, chứ không chạy session thuần. Lý do (chi tiết ở ADR-0130 *Options*): TLS phía initiator
chỉ tồn tại trong engine; plaintext và TLS phải là cùng một kịch bản chỉ khác một biến thì một
lỗi chỉ-TLS mới quy được về TLS.

### Bên Rust: `tools/interop`

- **Vai mới `--role dial`** (file mới `tools/interop/src/dial.rs`): đọc file cấu hình initiator
  bằng `Settings::load(..).into_initiator()` (plaintext) hoặc `into_tls_initiator()` +
  `tls::load_client_pem` (khi `SocketUseSSL=Y`), gọi `connect_and_serve` /
  `connect_and_serve_tls` với `fixbolt::app(desk::Desk::default())`, `NoRecovery`, `Handles` mới;
  dừng bằng `stop_on_stdin` giống vai acceptor, in `interop: dial stopped: Shutdown {…}`.
- **Vai `acceptor` biết cửa TLS**: nếu file có `SocketUseSSL=Y` thì đi `into_tls_table` +
  `load_pem` + `serve_tls_requiring(.., require_kernel)`; không có thì **đường cũ không đổi một
  dòng** (tám kịch bản `libquickfix` đang xanh qua đó).
- **In sự kiện**: một luồng phụ đọc `Observer::events` mỗi 50 ms, in `interop: event <kind>` —
  script đếm `TlsFellBackToUserspace` phải bằng 0.
- **Feature**: `tls = ["standard", "fixbolt-engine/tls"]` trong `tools/interop/Cargo.toml`, không
  vào `default`. Mọi item TLS nằm sau `#[cfg(all(feature = "tls", target_os = "linux"))]` trên
  chính item đó (bất biến 6); build không có `tls` mà gặp file `SocketUseSSL=Y` thì in
  `interop: FAIL … needs --features tls`.

### Cấu hình

- fixbolt: `HeartBtInt=2`; initiator thêm `SocketConnectHost=127.0.0.1`, `SocketConnectPort`,
  `ReconnectInterval=30` (để không quay số lại trong lúc Judge đang tắt sau `logout`). TLS: thêm
  `SocketUseSSL=Y`, **`TlsRequireKernel=Y`**, và cặp chứng chỉ/CA.
- QFJ: `UseDataDictionary=Y` với `FIX44.xml` **của chính QFJ** (trong jar, không phải file của
  `libquickfix`); `FileStorePath` dưới `vendor/interop-qfj-run/`; `ResetOnLogon=Y`,
  `ResetOnLogout=Y`, `ResetOnDisconnect=Y`; `StartTime=EndTime=00:00:00`; initiator
  `ReconnectInterval=1`. TLS: `SocketUseSSL=Y`, `EnabledProtocols=TLSv1.3`,
  `CipherSuites=TLS_AES_128_GCM_SHA256`, `KeyStoreType=PKCS12`, `TrustStoreType=PKCS12`;
  initiator QFJ có `SocketTrustStore` (chứa CA) và `EndpointIdentificationAlgorithm=HTTPS`;
  acceptor QFJ có `SocketKeyStore` (khoá + chuỗi chứng chỉ), `NeedClientAuth=N`. Để chắc, cả hai
  vai QFJ đều được cho `SocketKeyStore` của riêng nó (tránh QFJ đi tìm `quickfixj.keystore` mặc
  định).
- **Chứng chỉ sinh mỗi lần chạy**, dưới `vendor/interop-qfj-run/pki/`: một CA P-256, hai lá
  (`fixbolt-acceptor`, `qfj-acceptor`) có SAN `IP:127.0.0.1`, bằng `openssl`; PKCS12 cho QFJ bằng
  `openssl pkcs12 -export` (keystore) và `keytool -importcert` (truststore). Mật khẩu là một
  chuỗi cố định trong script — nó không bảo vệ gì, chứng chỉ sống vài giây và không bao giờ vào
  git.

### Script chấm gì, ngoài bảy bước

Mỗi lần chạy còn ba khẳng định phụ, in cùng khuôn `<nhãn>: <tên> ok|FAIL <đã thấy gì>`:

- `shutdown` — fixbolt trả về qua `Admin::shutdown` và in `Shutdown {`.
- `clean` — không có `35=3` và không có `35=j` ở **chiều nào** trong log của Judge (một Reject do
  từ điển QFJ khác từ điển mình là dạng lỗi hay lẩn nhất).
- `kernel` (chỉ hai lần TLS) — `TlsTxSw` **và** `TlsRxSw` trong `/proc/net/tls_stat` tăng ít nhất
  1 so với trước lần chạy; fixbolt in **0** dòng `interop: event TlsFellBackToUserspace`; file
  cấu hình có `TlsRequireKernel=Y`. Số đếm của kernel là thứ engine không tự viết được.

Script grep **từng tên** (7 bước + 2 hoặc 3 khẳng định phụ) cho **từng nhãn**, và dòng
`<nhãn>: PASS 7/7`. Thiếu một là đỏ, in cả log hai đầu ra stderr. Biến `INTEROP_QFJ_ARMS` cho
chạy một phần khi đang phát triển (ví dụ `acceptor-plain`), nhưng **dòng tổng kết cuối chỉ in
khi đủ bốn lần chạy**, và CI grep đúng dòng đó:

```text
interop-qfj: 7 / 7 acceptor plain + 7 / 7 acceptor TLS + 7 / 7 initiator plain + 7 / 7 initiator TLS
  (+ shutdown 4 / 4, clean 4 / 4, kernel 2 / 2) against QuickFIX/J 3.0.2 on <java -version dòng đầu>
```

### Chống chập chờn

- Không `sleep` để "đợi cho chắc": chờ dòng `interop-qfj: ready` / `interop: listening`, chờ
  `onLogon`, chờ tiến trình thoát — mỗi lần chờ có hạn `DEADLINE` (20 s), hết hạn là **đỏ, không
  treo**.
- Mỗi lần chạy có cổng riêng (`15660`–`15663`, đè được bằng biến môi trường), store riêng, log
  riêng: va cổng trông y hệt lỗi giao thức.
- Bước `heartbeat` đọc `108=` từ dây, không từ file cấu hình.
- Bước `resend` đòi **đúng** hai số, không phải "có gì đó `43=Y`".

### CI: job mới `interop-qfj`, chặn merge

```yaml
interop-qfj:
  name: Both roles, plaintext and TLS, against QuickFIX/J
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v5
    - uses: actions/setup-java@v6     # Temurin 21
    - name: This runner must be able to offload TLS   # chép nguyên bước của job `tls`
    - run: scripts/fetch-quickfix-assets.sh            # tools/interop cần vendor/ để build
    - name: Fetch QuickFIX/J, run four arms, read the output
      run: set -o pipefail; scripts/interop-qfj.sh 2>&1 | tee "$RUNNER_TEMP/interop-qfj.log"
    - name: Assert the summary line                   # grep dòng tổng kết, không tin mã thoát
    - if: always()
      name: Publish every transcript on the run page   # 4 × (log fixbolt + log Judge)
```

Không `continue-on-error`. Không đụng job `interop` cũ. Tốn **ước tính** 4–7 phút một runner
cho mỗi pull request (chủ yếu là build `tools/interop` với `rustls`/`ring`) — **chưa đo**; số đo
thật ghi vào `docs/CONFORMANCE.md` §10 khi có CI run.

### Chế độ engine

**`standard`** (mặc định) ở cả bốn lần chạy. `hft` không được gate này chạm tới, giống gate
`libquickfix`; ghi rõ trong `docs/CONFORMANCE.md` §10 là *chưa chứng minh bằng interop*.

### File sẽ tạo hoặc sửa

| File | Việc |
|---|---|
| `tools/interop-qfj/Judge.java` | **mới** — trọng tài Java, hai vai |
| `scripts/interop-qfj.sh` | **mới** — ghim + tải jar, sinh chứng chỉ, bốn lần chạy, chấm |
| `tools/interop/src/dial.rs` | **mới** — vai `--role dial` |
| `tools/interop/src/main.rs` | nhận `--role dial`; nhánh TLS của vai `acceptor`; luồng in sự kiện |
| `tools/interop/Cargo.toml` | feature `tls` |
| `.github/workflows/ci.yml` | job `interop-qfj` (chỉ thêm, không sửa job khác) |
| docs | xem *Tài liệu phải cập nhật* |

**Không đụng**: `crates/**`, `scripts/interop.sh`, `tools/interop/acceptor.cpp`,
`tools/interop/initiator.cpp`, `tools/interop/src/desk.rs` (trừ reversal tạm), `Cargo.toml` của
workspace (thư mục `tools/interop-qfj` **không** phải crate, không vào `members`), `STATUS.md`
(manager viết).

## Bất biến bị đụng tới

Việc này không sửa `codec`, `session`, `engine`, `transport`. Nó **gọi** cửa TLS của engine từ
một tool, nên đi lại danh sách §2:

- **1 (không cấp phát trên hot path)** — không đổi code hot path. `tools/interop` không phải hot
  path. Không cần chạy `benches/alloc.rs` cho bước này; không có số hiệu năng nào được công bố.
- **4 (theo chế độ)** — chạy `standard`; `standard` phải ngủ khi rảnh, và các cửa được gọi đã
  có check riêng (`the_dial_loop_sleeps_rather_than_spins_while_the_handshake_waits`). Mọi dòng
  kết quả ghi chế độ `standard`.
- **6 (feature gate chính `mod`/item)** — feature `tls` mới của `tools/interop` gắn `#[cfg]` trên
  item; `scripts/check-no-optional-deps.sh` và `cargo test --all --no-default-features` phải xanh
  **không đổi**; job `no-default-features` không thấy JVM nào.
- **7 (không `unwrap`/`expect`/`panic!`)** — `tools/interop` theo lint workspace; clippy `-D warnings`
  cả khi bật `--features tls`.
- **9 (không chép nguồn QuickFIX)** — `Judge.java` là code của mình gọi API công khai; jar nằm
  trong `vendor/` (gitignored); khẳng định cuối script bắt mọi file mới mà git thấy.
- **10 (số hiệu năng)** — gate này không in số hiệu năng nào; số phút CI là chi phí, không phải
  độ trễ, và được ghi là ước tính cho tới khi đo.

## Chia việc

Mỗi hàng một commit xanh trên nhánh `plan/p3-quickfixj-interop`; manager chạy lại gate và commit.

| Bước | Kết quả — file tạo / sửa (và **không** đụng) | Người làm | Gate (chạy được trên desk) | Xong khi | Reversal | Phụ thuộc |
|---|---|---|---|---|---|---|
| 0 | Desk có JDK: `sudo -n apt-get install -y openjdk-21-jdk-headless`; quote `java -version`, `javac -version`, `keytool -help 2>&1 \| head -1`. Không file nào | runner (haiku) | ba lệnh trên | ba dòng phiên bản được quote nguyên văn | — | anh/manager duyệt plan |
| 1 | **Trọng tài + ghim + lần chạy `acceptor-plain`**. Tạo `tools/interop-qfj/Judge.java` (cả hai vai Java), `scripts/interop-qfj.sh` §1 (ghim 5 jar bằng SHA-256: tải, so với `.sha1` của Maven Central, rồi ghi `sha256sum` vào script), §3–§5 cho nhãn `qfj-acceptor-plain`. Sửa `docs/internals/tools.md` (hàng mới). **Không đụng** `tools/interop/**`, `crates/**`, `scripts/interop.sh`, `.github/**` | developer (sonnet) | `INTEROP_QFJ_ARMS=acceptor-plain scripts/interop-qfj.sh` | log có `qfj-acceptor-plain: PASS 7/7`, `shutdown ok`, `clean ok`, `==> the run added nothing git can see`; sửa một byte trong một SHA-256 ghim → script dừng với `CHECKSUM MISMATCH` trước khi chạy gì | **B**: `--invert-resend` → `qfj-acceptor-plain: resend FAIL` (ghi nguyên văn QFJ gửi gì / fixbolt trả gì). **C**: đổi `gapfill` thành `gapfil` trong danh sách grep → `MISSING OR FAILED STEP: gapfil` dù Judge in `PASS 7/7` | 0 |
| 2 | **Vai `--role dial` + lần chạy `initiator-plain`**. Tạo `tools/interop/src/dial.rs`; sửa `tools/interop/src/main.rs` (định tuyến `--role dial`, luồng in sự kiện), `scripts/interop-qfj.sh` (nhãn `qfj-initiator-plain`), `docs/internals/tools.md`. **Không đụng** `desk.rs`, vai `initiator`/`acceptor`/`reconnect` hiện có, `crates/**`, `scripts/interop.sh` | developer (sonnet) | `INTEROP_QFJ_ARMS=acceptor-plain,initiator-plain scripts/interop-qfj.sh`; `scripts/interop.sh` (libquickfix, phải còn nguyên dòng tổng kết cũ); `cargo test --all`; `cargo test --all --no-default-features`; `cargo clippy --all-targets -- -D warnings`; `cargo fmt --all --check`; `scripts/check-no-optional-deps.sh` | hai nhãn `PASS 7/7` + `shutdown ok` + `clean ok`; `scripts/interop.sh` in lại đúng dòng `interop: 7 / 7 + 8 / 8 + …` | **A**: tạm bỏ `.field(11, cl_ord_id)` trong `desk.rs` → `order FAIL` ở **cả hai** nhãn QFJ (và `interop-acceptor: order FAIL` bên `libquickfix` — ghi nhận, đó là cùng một `Desk`) | 1 |
| 3 | **TLS, hai lần chạy, và chứng cứ kernel**. Sửa `tools/interop/Cargo.toml` (feature `tls`), `tools/interop/src/main.rs` (nhánh TLS vai `acceptor`), `tools/interop/src/dial.rs` (nhánh TLS), `scripts/interop-qfj.sh` (§2 sinh chứng chỉ; nhãn `qfj-acceptor-tls`, `qfj-initiator-tls`; khẳng định `kernel`; đọc `tls_stat` trước/sau từng lần). **Không đụng** `crates/**` — nếu cửa TLS của engine thiếu gì, **dừng và báo**, không sửa engine | **senior developer (opus)** — giữ một bất biến trải qua settings, cửa TLS của engine, sự kiện và bộ đếm kernel; sai thì công bố một câu "TLS interop" không đúng | toàn bộ `scripts/interop-qfj.sh` (bốn nhãn); `cargo build -p fixbolt-interop --features tls`; `cargo clippy -p fixbolt-interop --all-targets --features tls -- -D warnings`; các gate của bước 2 | dòng tổng kết bốn lần 7 / 7, `kernel 2 / 2` | **D** (chỉ trên desk, khi không phiên nào khác đang đo): `sudo -n modprobe -r tls` → hai nhãn TLS đỏ, fixbolt in lời từ chối của `TlsRequireKernel` (quote chữ thật, không đoán); `sudo -n modprobe tls` → xanh lại. **E**: `CipherSuites=TLS_AES_256_GCM_SHA384` phía QFJ → `qfj-*-tls: logon FAIL` với lỗi bắt tay của JSSE trong `qfj: error` | 2 |
| 4 | **Job CI `interop-qfj`, chặn merge**. Sửa `.github/workflows/ci.yml` (chỉ thêm job; bước kTLS chép từ job `tls`; transcript lên `$GITHUB_STEP_SUMMARY`). Sửa `README.md` (layout: `tools/interop-qfj/`; dòng interop), `CHANGELOG.md` (`[Unreleased]` → *Added*), `docs/DESIGN.md` §3 (hàng `tools/interop-qfj`) và §6 (hàng gate mới). **Không đụng** job khác | developer (sonnet) | ba lần chạy liền `scripts/interop-qfj.sh` trên desk, quote ba dòng tổng kết; `python3 scripts/check-links.py`; push, PR draft, CI | job `interop-qfj` xanh trên commit đó, log job có dòng tổng kết; run id ghi lại | **G**: tạm đổi dòng grep tổng kết trong job thành `7 / 7 acceptor plain + 7 / 7 acceptor TLS + 6 / 7` → job đỏ ở bước *Assert the summary line* (chạy trên nhánh, rồi revert) | 3 |
| 5 | **Công bố kết quả**. `docs/CONFORMANCE.md` §10 mới *Interop against QuickFIX/J* (lệnh, máy, JDK, CI run id, bốn khối output nguyên văn, cái chưa chứng minh); `docs/reference/` cho mọi bẫy đã gặp (mỗi cái có test canh); ADR-0130 → Accepted nếu manager duyệt | developer (sonnet); manager viết `STATUS.md` | `python3 scripts/check-links.py` | §10 trích nguyên văn log **của CI**, có run id của commit đóng | — | 4 |

Bước 1 và bước 0 là tuần tự; các bước còn lại tuần tự vì cùng sửa `scripts/interop-qfj.sh`
(một file, một người viết).

## Cách kiểm chứng

**Lệnh:** `scripts/interop-qfj.sh`. Đạt khi log có, theo thứ tự:

```text
==> quickfixj 3.0.2: 5 jars, every SHA-256 as pinned
qfj-acceptor-plain: logon        ok  35=A 49=FIXBOLT 56=QFJINI 141=Y 108=2
qfj-acceptor-plain: order        ok  35=8 at 34=[4, 5], 11= matched
qfj-acceptor-plain: heartbeat    ok  35=0 without 112= within 5 s
qfj-acceptor-plain: testrequest  ok  35=0 112=QFJ-TR-1
qfj-acceptor-plain: resend       ok  35=8 43=Y replayed at 34=[4, 5], wanted [4, 5]
qfj-acceptor-plain: gapfill      ok  35=2 7=… in: yes, then 35=0 112=QFJ-TR-3: yes
qfj-acceptor-plain: logout       ok  35=5
qfj-acceptor-plain: PASS 7/7
qfj-acceptor-plain: shutdown     ok  Shutdown { … }
qfj-acceptor-plain: clean        ok  no 35=3, no 35=j
… (ba nhãn còn lại cùng khuôn; hai nhãn TLS có thêm)
qfj-acceptor-tls: kernel         ok  TlsTxSw +n, TlsRxSw +m, 0 TlsFellBackToUserspace, TlsRequireKernel=Y
==> the run added nothing git can see
interop-qfj: 7 / 7 acceptor plain + 7 / 7 acceptor TLS + 7 / 7 initiator plain + 7 / 7 initiator TLS …
```

(Số `34=` ở trên là ví dụ — `Desk` gửi hai `35=B` khi logon nên `35=8` không ở `34=2`; script đọc
số từ dây, không ghi cứng.)

**Đọc log, không đọc mã thoát** — grep từng bước, từng nhãn, từng khẳng định phụ, rồi dòng tổng
kết. Một nhãn thiếu hẳn (Judge chết trước khi in) phải đỏ với `MISSING OR FAILED STEP`.

**Reversal** (bảng *Chia việc*): A (`Desk` bỏ echo `11=`), B (`--invert-resend`), C (grep sai tên
bước), D (gỡ module `tls` — chỉ desk), E (QFJ đòi cipher suite fixbolt không có), G (CI grep sai
dòng tổng kết). Mỗi cái: **viết câu FAIL mong đợi ra trước**, chạy, quote dòng đỏ nguyên văn,
khôi phục, chạy lại xanh, `git diff` sạch ngoài file của bước.

**Chạy thật chứ không phải test**: đây là hai tiến trình của hai engine khác nhau qua socket
kernel thật, TLS thật (JSSE ↔ rustls + kTLS). Ba lần chạy liền trên desk ở bước 4 để thấy không
chập chờn; CI chạy một lần mỗi PR.

**Bằng chứng khi đóng**: bốn khối output nguyên văn từ log **của job CI**, kèm run id và commit;
dòng đỏ của từng reversal, nguyên văn.

## Tài liệu phải cập nhật

Đi theo bảng `CLAUDE.md` §4, từng hàng:

- [ ] `docs/CONFORMANCE.md` — §10 mới: lệnh, máy (runner CI + desk), JDK, QFJ 3.0.2, CI run id,
      output nguyên văn, "chưa chứng minh" (`hft`; userspace TLS; client certificate)
- [ ] `README.md` — layout có `tools/interop-qfj/`; dòng interop nói hai đối tác
- [ ] `CHANGELOG.md` — `[Unreleased]` *Added*: interop QuickFIX/J hai vai, plaintext + TLS
- [ ] `docs/DESIGN.md` §3 (hàng `tools/interop-qfj`, không phải crate) và §6 (gate mới, cách đo)
- [ ] `docs/internals/tools.md` — `Judge.java`, `dial.rs`, thứ tự đọc, gate canh
- [ ] `docs/reference/` — mọi bẫy gặp phải (ưu tiên cao nhất), mỗi bẫy một test canh
- [ ] ADR-0130 — Proposed → Accepted khi duyệt
- [ ] `STATUS.md` — manager, khi đóng; gạch dòng *Not proven* liên quan nếu có
- Không đổi: `docs/CONFIGURATION.md` (không khoá mới), `docs/GUIDE.md` (không ràng buộc mới cho
  người nhúng), `PRD.md` (phase không đổi).

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Jar hay chứng chỉ lọt vào commit (kéo nghĩa vụ giấy phép QFJ vào repo) | §5 của script: so `git status` trước/sau, đỏ nếu có file mới git thấy; mọi thứ nằm dưới `vendor/` |
| Jar bị thay trên mạng, hoặc tải nhầm phiên bản | SHA-256 ghim trong script, lệch là dừng trước khi chạy; kiểm bằng cách sửa một byte (bước 1) |
| TLS xanh nhưng thực ra chạy userspace (bẫy của phase-3 scope) | khẳng định `kernel`: `tls_stat` tăng, 0 sự kiện `TlsFellBackToUserspace`, `TlsRequireKernel=Y`; reversal D |
| Runner không có kTLS → đỏ trông như lỗi engine | bước "This runner must be able to offload TLS" chép từ job `tls`, báo lỗi môi trường bằng lời riêng |
| QFJ nuốt bản `43=Y` / message admin trước application → chấm sai | chấm trên chuỗi thô của `RawLog`, không trên `fromApp`/`fromAdmin` |
| `FIX44.xml` của QFJ khác của `libquickfix` → QFJ trả `35=3`/`35=j` và bước sau đỏ lẫn lộn | khẳng định `clean`; `order` đọc `58=` của Reject nếu có |
| QFJ initiator gửi Logon trước khi bắt tay TLS xong (báo cáo NPE trên 3.0.2 / JDK 17, **chưa kiểm chứng**) | `logon` có hạn 5 s và in `qfj: error …`; nếu gặp: ghi vào `docs/reference/`, không nới hạn |
| JSSE server gửi `NewSessionTicket` sau bắt tay TLS 1.3; initiator fixbolt trên kTLS phải bỏ qua | lần chạy `qfj-initiator-tls` (`kernel` + bảy bước); test sẵn có `tests/tls_key_update.rs` (ADR-0063) |
| `close_notify` của QFJ sau Logout tới kernel như một bản ghi không phải dữ liệu | `shutdown ok` và không có `interop: event EndedWithoutReason` trong log fixbolt |
| Initiator fixbolt quay số lại sau `logout` rồi logon lần hai, làm bẩn log | `ReconnectInterval=30` trong cấu hình fixbolt; Judge chấm xong trước khi dừng |
| Va cổng giữa các lần chạy trông như lỗi giao thức | bốn cổng riêng `15660`–`15663`, store riêng |
| Một lần chạy chết sớm mà script vẫn xanh vì chỉ grep dòng `PASS` | grep từng bước + reversal C |
| Reversal treo thay vì đỏ | mọi lần chờ có `DEADLINE`; reversal D phải đỏ trong hạn |
| `KeyStoreType` mặc định `JKS` của QFJ không đọc PKCS12 | đặt `KeyStoreType=PKCS12`, `TrustStoreType=PKCS12` tường minh; lần chạy TLS đầu tiên canh |
| Bật feature `tls` của tool làm `cargo test --all --no-default-features` âm thầm build `rustls` | `scripts/check-no-optional-deps.sh`, job `no-default-features` không đổi |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Cửa initiator TLS của engine hoặc cách settings dựng nó thiếu một mảnh mà tool cần | Trung bình | bước 3 dừng và báo; việc sửa `crates/engine` là plan riêng, không lén làm ở đây |
| QFJ và fixbolt thật sự bất đồng ở một bước (không phải lỗi harness) | Trung bình | đó là mục đích của gate: ghi bằng chứng hai đầu, phân loại theo §12 (*confirmed* / *refuted* / *design*); không nới khẳng định để xanh |
| QFJ là "anh em" của QuickFIX: cách đọc FIX giống nhau thì cùng sai mà vẫn xanh | Thấp–trung bình | ghi rõ ở ADR-0130 *Consequences* và `CONFORMANCE.md` §10; không nói "độc lập hoàn toàn" |
| CI chậm thêm vài phút mỗi PR | Thấp | chạy song song với các job khác; số phút thật ghi ở §10 |
| Ảnh runner bỏ module `tls` | Thấp | job đỏ bằng lời "môi trường", giống job `tls`; không `continue-on-error` |
| Phiên khác đang đo trên desk khi bước 3/4 cần chạy | Thấp | kiểm trước; reversal D chỉ chạy khi desk rảnh; các bước khác không cần máy §9 |

## Ngoài phạm vi

- Chế độ `hft`; TLS userspace (cố ý bị cấm bằng `TlsRequireKernel=Y`); chứng chỉ client (mTLS);
  FIXT 1.1 / FIX 5.0 SP2 với QFJ; kịch bản reconnect / `789=` / độ chính xác `52=` với QFJ — các
  kịch bản đó có với `libquickfix` và không phải tiêu chí 6.
- Chạy session thuần (`--role initiator`) với QFJ — `libquickfix` đã canh đường đó.
- Mở rộng bộ cipher suite hay phiên bản TLS của fixbolt — gate này ghi lại cái đã hỗ trợ, không
  đổi nó.
- macOS: TLS của fixbolt chỉ có trên Linux.
- Sửa bất cứ gì trong `crates/`.

## Sửa 1 — 2026-09-23

Ba điều đọc code hôm nay mới thấy, cả ba xảy ra khi làm bước 3 và bước 4/5; không cái nào đổi
hình bước 1–2 đã giao.

**1. Lá chứng chỉ `fixbolt-acceptor` mang thêm `DNS:localhost`, lệch chữ "IP SAN" của ADR-0130
quyết định 6 — có chủ đích, có bằng chứng `javap`.** Quyết định 6 viết "an IP SAN of
`127.0.0.1`" cho cả hai lá. Chạy thật, lá chỉ có `IP:127.0.0.1` làm nhãn `qfj-acceptor-tls` đỏ ở
bước `logon` với lỗi bắt tay JSSE nhắc tới `localhost`, không phải `127.0.0.1`. `javap -p -c`
trên `quickfix.mina.ssl.InitiatorSslFilter` (trong `quickfixj-core-3.0.2.jar`) cho thấy
`createEngine` gọi `InetSocketAddress.getHostName()` rồi đưa chuỗi đó vào
`SSLContext.createSSLEngine(String, int)`:

```
8: aload_2
9: invokevirtual #4   // Method java/net/InetSocketAddress.getHostName:()Ljava/lang/String;
...
16: invokevirtual #6  // Method javax/net/ssl/SSLContext.createSSLEngine:(Ljava/lang/String;I)Ljavax/net/ssl/SSLEngine;
```

`getHostName()` gọi trên một `InetSocketAddress` dựng từ chuỗi IP sẽ **phân giải ngược** —
trên máy nào có `/etc/hosts` trả `127.0.0.1` về `localhost` (mọi desk và mọi runner CI bình
thường), chuỗi đưa vào là `"localhost"`, và `EndpointIdentificationAlgorithm=HTTPS` kiểm chứng
chỉ theo đúng chuỗi đó chứ không theo địa chỉ đã quay số. Manager duyệt: `qfj-acceptor` (fixbolt
làm initiator kiểm) giữ nguyên chữ quyết định 6, chỉ `fixbolt-acceptor` (QFJ làm initiator kiểm)
thêm `DNS:localhost` bên cạnh `IP:127.0.0.1` — cả hai bên đều được thoả bằng một chứng chỉ.
Viết đầy đủ ở
[docs/reference/quickfixj-verifies-the-host-name-not-the-dial-address.md](../reference/quickfixj-verifies-the-host-name-not-the-dial-address.md).

**2. Lời từ chối `NeedsFeature` — chữ thật, không phải diễn giải.** Khi file cấu hình có
`SocketUseSSL=Y` mà build không bật feature `tls`, `Settings::load` trả `Problem::NeedsFeature`,
và `Display` của nó (`crates/engine/src/settings.rs:466`) in nguyên văn:

> `this engine was built without the feature this key needs`

`tools/interop/src/dial.rs` không tự viết lời riêng cho ca này — nó chỉ in `interop: FAIL
settings <file>: <e>`, và `<e>` là chuỗi trên. Ghi lại vì `--role dial` là chỗ đầu tiên một
người ngoài đọc thấy câu này qua một công cụ dòng lệnh thay vì qua `SettingsError` được test đọc
trực tiếp.

**3. `tls_stat` là bộ đếm toàn máy — cần chứ không đủ, và `docs/CONFORMANCE.md` §10 nói rõ.**
`/proc/net/tls_stat`'s `TlsTxSw`/`TlsRxSw` không phân biệt theo tiến trình hay socket; khẳng
định `kernel` đọc chúng tăng là bằng chứng **cần** rằng kernel có nhận khoá trong lần chạy đó,
không phải bằng chứng **đủ** rằng chính khoá của lần chạy đó — trên một máy đang chạy việc khác
cũng dựng kTLS cùng lúc, số đó có thể tăng vì lý do khác. Trên desk lúc chạy gate này không có
phiên nào khác đang đo, và trên runner CI cũng không có gì khác dựng kTLS cùng lúc, nên rủi ro
không thành hiện thực trong ba lần chạy — nhưng câu chữ ở `docs/CONFORMANCE.md` §10 ghi giới
hạn thật của phép đo chứ không nói "bằng chứng kín".

## Nhật ký giao hàng

- **2026-09-23 — bước 0, 1, 2 giao** (commit `38eef9d`, nhánh `plan/p3-quickfixj-interop`):
  JDK cài qua `sudo -n apt-get install -y openjdk-21-jdk-headless`; `tools/interop-qfj/Judge.java`
  (cả hai vai), `scripts/interop-qfj.sh` §1/§3–§5 hai nhãn `qfj-acceptor-plain` và
  `qfj-initiator-plain`, `tools/interop/src/dial.rs` (vai `--role dial`, plaintext). Gate:
  `INTEROP_QFJ_ARMS=acceptor-plain,initiator-plain scripts/interop-qfj.sh` xanh
  (`qfj-acceptor-plain: PASS 7/7`, `qfj-initiator-plain: PASS 7/7`, `the run added nothing git
  can see`); `scripts/interop.sh` (libquickfix) vẫn xanh nguyên dòng tổng kết cũ. Reversal A
  (bỏ echo `11=` trong `desk.rs`) → `order FAIL` ở cả hai nhãn QFJ **và**
  `interop-acceptor: order FAIL` bên libquickfix (cùng một `Desk`); B (`--invert-resend`) →
  `resend FAIL`; C (grep sai tên bước) → `MISSING OR FAILED STEP`; sửa một hex trong SHA-256
  ghim → `CHECKSUM MISMATCH` trước khi chạy gì. Bẫy tìm thấy: judge đua với luồng gap-fill riêng
  của QFJ (sửa bằng cách chờ `35=4 123=Y` trên dây trước khi gửi tiếp) — viết ở
  [docs/reference/a-judges-verdict-can-race-the-oracle-it-judges.md](../reference/a-judges-verdict-can-race-the-oracle-it-judges.md).
  Việc để lại: docs (internals/tools.md, CONFORMANCE.md, reference) — dồn vào bước 4–5.
- **2026-09-23 — bước 3 giao** (commit `010b42b`): `tools/interop/Cargo.toml` thêm feature
  `tls`; `tools/interop/src/main.rs` nhánh TLS vai `acceptor`; `tools/interop/src/dial.rs` nhánh
  TLS; `scripts/interop-qfj.sh` §2 sinh chứng chỉ, hai nhãn `qfj-acceptor-tls` và
  `qfj-initiator-tls`, khẳng định `kernel`. Gate: bốn nhãn `scripts/interop-qfj.sh` xanh (dòng
  tổng kết `7 / 7 × 4 + shutdown 4/4 + clean 4/4 + kernel 2/2`); `cargo build -p fixbolt-interop
  --features tls`; `cargo clippy -p fixbolt-interop --all-targets --features tls -- -D
  warnings`. Reversal D (`sudo -n modprobe -r tls` → hai nhãn TLS đỏ với lời từ chối
  `TlsRequireKernel=Y`; `modprobe tls` → xanh lại); E (`CipherSuites=TLS_AES_256_GCM_SHA384`
  phía QFJ → `logon FAIL`, JSSE báo không có cipher chung). Việc để lại, ghi rõ chứ không giấu:
  docs vẫn chưa cập nhật (đúng như thông báo trong commit), và hai phát hiện mới — (b) lá chứng
  chỉ cần `DNS:localhost` (Sửa 1 mục 1), (c) bắt tay không có cipher chung khiến acceptor của
  fixbolt đóng im lặng, không alert không event.
- **2026-09-23 — bước 4 giao, do phiên này build** (chưa commit — manager chạy gate và commit):
  job CI `interop-qfj` (`.github/workflows/ci.yml`), chặn merge, chép bước kTLS từ job `tls`,
  Temurin 21 qua `actions/setup-java@v6`, đọc dòng tổng kết ở bước riêng chứ không tin mã thoát;
  `scripts/check-no-optional-deps.sh` thêm `fixbolt-interop:rustls` và
  `fixbolt-interop:ktls-core` (cùng hình `fixbolt-engine`/`fixbolt-w2w` đã có); `README.md`,
  `CHANGELOG.md`, `docs/DESIGN.md` §3 và §6, `docs/internals/tools.md`. Gate: ba lần chạy liền
  `scripts/interop-qfj.sh` trên desk, cả ba dòng tổng kết giống hệt nhau (quote ở
  `docs/CONFORMANCE.md` §10); `python3 -c 'import yaml,sys;yaml.safe_load(open(".github/workflows/ci.yml"))'`
  qua; reversal G (đổi dòng grep tổng kết trong job thành thiếu `initiator TLS` → bước *Assert
  the summary line* đỏ, rồi khôi phục) — quote ở báo cáo của bước này. CI run id: `<pending>` —
  manager điền khi PR mở và job chạy.
- **2026-09-23 — bước 5 giao cùng phiên** (chưa commit): `docs/CONFORMANCE.md` §10 (lệnh, máy,
  JDK, bốn khối output nguyên văn từ desk, CI run id còn `<pending>`); bốn trang
  `docs/reference/` cho bốn bẫy (a)–(d), mỗi trang nêu rõ có test canh hay không; Sửa 1 này;
  một dòng trạng thái thêm vào ADR-0130 trỏ tới Sửa 1.
