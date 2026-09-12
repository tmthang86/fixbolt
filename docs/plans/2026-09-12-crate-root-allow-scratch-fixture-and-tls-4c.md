# Hai lớp lỗi của gate chưa có máy canh, và bước 4c của plan TLS

> **Loại:** Plan · **Ngày:** 2026-09-12 · **Trạng thái:** **Đã duyệt 2026-09-12**, đang làm — nhánh `plan/crate-root-allow-and-scratch-fixtures`
> **Phạm vi:** `STATUS.md` item **58** (một `#![allow]` ở gốc crate tắt lint cả crate) và item
> **23** (fixture tạm thừa hưởng máy thay vì repo) — hai *lớp* lỗi đã đóng *một trường hợp* mà
> chưa có gì canh trường hợp tiếp theo; cộng bước **4c** của
> [plan TLS](2026-09-04-tls.md) (năm key cấu hình), sau khi xác minh lại theo code hôm nay.
> Chạm `scripts/`, `.github/workflows/ci.yml`, `crates/engine/src/settings.rs`,
> `crates/engine/src/tls.rs`, `crates/engine/tests/`, docs. **Không chạm** `codec`, `session`,
> `transport.rs`, `lib.rs` của `engine`.
>
> **Duyệt 2026-09-12.** Chủ sở hữu duyệt cả plan này và *Sửa 5* của plan TLS trong cùng một lượt,
> và **quyết định luôn câu hỏi tên key mà Sửa 5 để mở**: lấy **tên của QuickFIX C++** ở chỗ
> QuickFIX C++ có tên — `SocketUseSSL`, `ServerCertificateFile`, `ServerCertificateKeyFile`,
> `TlsRequireKernel`. Lý do là luật đặt tên `docs/CONFIGURATION.md:69-77` tự viết: người đến từ
> QuickFIX gõ đúng ngay lần đầu. `SocketUseSSL` giữ vì đó là tên QuickFIX/J và C++ không có
> tương đương (C++ chọn SSL bằng lớp `SSLSocketAcceptor`). `TlsCaFile` **bỏ** — không có gì đọc.
>
> **Một PR, một nhánh, ba việc.** Thứ tự bước được xếp để nếu phải cắt thì cắt từ cuối: item 58 và
> 23 đóng trước, 4c sau — vì 4c còn chờ thêm một quyết định ở *Sửa 5* của plan TLS.

## Bối cảnh

Hai lần trong hai tuần, một gate của repo này **đọc đúng số trong khi nửa canh gác của nó đã tắt**:

1. `[measured 2026-09-08]` `clippy::indexing_slicing = "deny"` không có tác dụng ở `engine`,
   `session` và `dict` trong suốt một lần build, vì ba chú thích nợ được viết
   `#![allow(clippy::indexing_slicing)]` ở đầu `lib.rs`. `#!` là *inner attribute*; đặt ở gốc
   crate thì nó tắt lint cho **cả crate**, kể cả module sạch. `scripts/check-indexing-debt.sh`
   vẫn đếm đúng — nó đếm bằng `--force-warn`, thứ đè lên mọi `allow` — nên **con số ai cũng nhìn
   thì đúng, còn cái chặn code mới thì tắt**. Tìm ra bằng cách dán một `v[0]` vào module sạch và
   nhận exit 0. Đã sửa: 15 hàm mang `#[allow]` riêng. **Chưa có gì máy-canh** để lần sau một
   `#![allow]` không lại rơi vào gốc crate. (`STATUS.md` item 58.)
2. `[measured 2026-08-31]` `scripts/check-lint-config.sh` dựng crate tạm trong `mktemp -d`, nơi
   `rust-toolchain.toml` không với tới. Trên máy không có `rustup default`, nó exit 1 và nói
   *"the workspace lints do not deny: unwrap_used expect_used panic"* — **đỏ giả về hệ thống đang
   được kiểm**, trong khi `cargo clippy` chưa hề chạy. Mặt trái lặng hơn: trên máy *có* default,
   gate kiểm config lint của workspace bằng **một clippy khác** với clippy workspace đã ghim. Đã
   sửa bằng cách copy `rust-toolchain.toml` vào crate tạm. **Trường hợp đóng, lớp còn mở**: không
   gì ngăn fixture tiếp theo được viết y hệt. (`STATUS.md` item 23.)

Cả hai cùng canh **bất biến 7** (`CLAUDE.md` §2: không `panic`/`unwrap`/`expect` trong crate thư
viện) — một cái là lint bị tắt, một cái là gate của lint chạy trên sai compiler.

Việc thứ ba là **bước 4c của plan TLS**: năm key cấu hình cho TLS. Plan đó tự đặt luật *xác minh
lại trước khi nhận việc*; lần xác minh này tìm ra bước 4c **không dựng được đúng như Sửa 3 viết**,
vì bốn lý do ghi ở *Sửa 5* của plan đó và tóm ở mục dưới. Bước 4c vì thế đứng trong bảng này với
điều kiện: **Sửa 5 được duyệt trước**.

## Những gì đã biết chắc

### Về item 58 — `#![allow]` ở gốc crate

| Sự thật | Nguồn |
|---|---|
| Hôm nay **không** `lib.rs`/`main.rs` nào dưới `crates/` mang `#![allow(...)]`. Các inner attribute ở gốc crate là: `codec` `#![no_std]`; `library` `#![doc = …]`, `#![warn(missing_docs)]`; `session` `#![forbid(unsafe_code)]` — đều là siết, không phải nới | `grep -rn '^#!\[' crates/*/src tools/*/src`, chạy 2026-09-12 |
| **Hai binary dưới `tools/` vẫn mang `#![allow]` ở gốc crate, có chủ ý và có comment**: `tools/w2w/src/main.rs:65` `#![allow(unsafe_code)]` và `:69` `#![allow(clippy::indexing_slicing)]`; `tools/interop/src/main.rs:72` `#![allow(clippy::indexing_slicing)]`. Comment tại chỗ: *"Not a library crate's source: non-negotiable 7 is about `crates/*/src`"* | hai file đó |
| 17 module (không phải gốc crate) mang `#![allow(clippy::indexing_slicing)]` ở đầu file — đó là nợ của item 55, phạm vi đúng một module, và ngoài phạm vi plan này | cùng lệnh grep |
| Cả **9 thành viên workspace** đều `[lints] workspace = true`, không có bảng `[lints.*]` riêng | `grep -n -A2 '^\[lints\]' crates/*/Cargo.toml tools/*/Cargo.toml` |
| Workspace deny bốn lint: `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`; `unsafe_code = "warn"` | `Cargo.toml:34-67` |
| `check-indexing-debt.sh` chỉ đếm `crates/*/src` (regex `^crates/…`), chạy `--workspace` với `--force-warn`, và **từ chối số 0** | `scripts/check-indexing-debt.sh:101, 116-120` |
| Gốc crate thật sự là: mọi target `lib`/`bin` (`src_path`) mà `cargo metadata` liệt kê — không phải "file tên `lib.rs`". `crates/dict` có `build.rs` (một gốc crate nữa, nhưng là build script, không phải thư viện) | `cargo metadata --no-deps`; `ls crates/*/build.rs` |
| **Luật ngôn ngữ**: `--force-warn` *"forces a lint to warning level, and takes precedence over attributes and all other CLI flags"*; **attribute trong source thắng cờ dòng lệnh** (*"Attributes within the source code take precedence over CLI flags, except for `-F`/`--forbid`"*); `forbid` không hạ được; `#[expect]` chỉ báo khi lint **không** nổ, không nói gì về phạm vi | [rustc book, Lint levels](https://doc.rust-lang.org/rustc/lints/levels.html), đọc 2026-09-12 |
| Attribute lint đè lên attribute ở *cấp cao hơn trong cây cú pháp* — nên `#![allow]` ở gốc crate là cấp cao nhất, mọi module con thừa hưởng | [Rust Reference, Diagnostics](https://doc.rust-lang.org/reference/attributes/diagnostics.html) |
| **Hệ quả cho thiết kế**: `[lints]` trong `Cargo.toml` được cargo truyền cho rustc dưới dạng **cờ**, nên một crate bỏ `workspace = true` và tự viết `[lints.clippy] indexing_slicing = "allow"` cũng tắt được lint cho cả crate — cùng lớp lỗi, ở tầng manifest. Tài liệu cargo mục `[lints]` chỉ nói cargo áp `[lints]` cho package hiện tại và dùng `--cap-lints` cho dependency; không nói gì thêm | [Cargo reference, `[lints]`](https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section) — **tìm kiếm không cho thêm gì** về ưu tiên `[lints]` so với attribute |
| **Tìm kiếm không thấy** dự án Rust nào máy-canh "không có `#![allow]` ở gốc crate" bằng công cụ có sẵn: không có lint clippy cho việc đó, `clippy.toml` không có khoá tương ứng, `#[expect]` không phát hiện được phạm vi. Cách duy nhất tìm thấy là kiểm **văn bản** — grep gốc crate — hoặc kiểm **hiệu ứng**: dán một vi phạm vào module sạch rồi xem clippy có từ chối không (đúng cách đã tìm ra lỗi) | tìm kiếm web 2026-09-12, hai truy vấn; [clippy #12716](https://github.com/rust-lang/rust-clippy/issues/12716) là thứ gần nhất và nói về việc ngược lại |

### Về item 23 — fixture tạm thừa hưởng máy

| Sự thật | Nguồn |
|---|---|
| Sáu script dùng `mktemp`: `check-lint-config.sh:19` (`mktemp -d`, **`cd` vào**, đã copy `rust-toolchain.toml` ở `:52`), `check-standard-gives-the-core-back.sh:43`, `check-no-kernel-sleep.sh:31`, `check-ktls-on-a-plain-socket.sh:22` (ba cái này chỉ ghi file output; `check-ktls…` chạy `cargo build` **trong** `spikes/ktls`, tức trong cây), `check-indexing-debt.sh:58` (`mktemp` một file log), `measure-isolation-cost.sh:40` (`mktemp -t` một file binary) | `grep -n mktemp scripts/*.sh` |
| **Artefact ghim ở gốc repo hôm nay chỉ có `rust-toolchain.toml`** (`channel = "1.98.0"`). Không có `clippy.toml`, `.clippy.toml`, `.cargo/config.toml`, `rustfmt.toml`. `Cargo.lock` và `deny.toml` có nhưng không ảnh hưởng crate tạm không dependency | `ls -a` gốc repo |
| **rustup tìm `rust-toolchain.toml` từ thư mục hiện tại đi lên**, không từ `--manifest-path`; thứ tự ưu tiên: `+toolchain` > `RUSTUP_TOOLCHAIN` > directory override > `rust-toolchain.toml` > default. Nên **`cd` vào thư mục tạm là hành vi châm ngòi**, còn `cargo --manifest-path /tmp/x/Cargo.toml` chạy từ trong cây vẫn dùng toolchain của repo | [rustup book, Overrides](https://rust-lang.github.io/rustup/overrides.html) |
| Clippy tìm `clippy.toml` từ `CLIPPY_CONF_DIR`, rồi `CARGO_MANIFEST_DIR`, rồi thư mục hiện tại, và **đi lên cha**. Nếu repo này một ngày có `clippy.toml`, crate tạm ngoài cây sẽ không thấy nó — cùng lớp lỗi, artefact khác | [Clippy book, Configuration](https://doc.rust-lang.org/clippy/configuration.html) |
| Không test Rust nào trong `crates/`, `tools/` gọi `Command::new("cargo")`/`"rustc"`. Các `std::env::temp_dir()` trong test là file journal/log output | `grep -rn 'Command::new("cargo")\|temp_dir' crates tools` |
| CI cài toolchain bằng `rustup component add clippy` trong cây, nên **CI là đúng môi trường mà lỗi này không bao giờ hiện** — item 23 chỉ tìm ra vì §9 bắt chạy gate ở máy làm việc | `.github/workflows/ci.yml:40, 237`; `docs/reference/a-scratch-fixture-inherits-the-machine.md` |
| **Tìm kiếm không thấy** dự án nào máy-canh lớp này; lời khuyên phổ biến là *copy artefact ghim vào fixture* hoặc *dựng fixture trong cây* — đúng hai cách reference đã ghi | tìm kiếm web 2026-09-12 |
| Một đảo chiều đỏ chưa chứng minh assertion mình viết nó cho; phải **dự đoán assertion nào đỏ trước khi chạy**, rồi so | `docs/reference/a-red-reversal-does-not-prove-the-assertion-you-wrote-it-for.md` |

### Về bước 4c — xác minh lại `Sửa 3` theo code hôm nay

| Sự thật | Nguồn |
|---|---|
| `Key` có **26 biến thể**, không `_` arm; `parse` (`:146`), `name` (`:180`), `Block::set` (`:425`), `Block::over` (`:467`) đều match **vét cạn** — thêm biến thể mà quên một chỗ thì không compile. `settings.rs` **không có `#[cfg(feature)]` nào** | `crates/engine/src/settings.rs:78-141, 146-217, 395-508` |
| Key `[DEFAULT]`-only được xử lý **trước** khi vào `Block` (`FileLogPath`, `ConnectionType`, `:643-672`); key theo vai bị từ chối bằng `Problem::WrongRole` nêu đúng dòng (`dialling_key`, `:500-509`, `:721-732`). `Problem` là `#[non_exhaustive]` | cùng file |
| `Settings` giữ `log: Option<PathBuf>` và `log()` trả path; rustdoc ở `:757-762` nói thẳng: *một key mà lặng lẽ không làm gì là failure mode `CLAUDE.md` §10 nêu* | `settings.rs:571-585, 757-766` |
| **Không có "kiểm hai chiều" nào giữa `docs/CONFIGURATION.md` và `Key`.** Không test, script hay job nào đọc `CONFIGURATION.md` để đối chiếu key; `check-links.py` chỉ kiểm link. Bằng chứng nó không tồn tại: `CONFIGURATION.md:21` nói **"Twenty-three keys"** trong khi bảng có **26 hàng** và enum có 26 biến thể | `grep -rn CONFIGURATION crates scripts .github` → chỉ `buffer_size.rs` (về const generic); `grep -c '^| \`[A-Za-z]*\` |' docs/CONFIGURATION.md` |
| `serve_tls` nhận `certs: Vec<CertificateDer<'static>>` + `key: PrivateKeyDer<'static>` — **DER đã nạp, không phải đường dẫn** (`:1789-1799`); `serve_tls_requiring` (`:1820-1830`) thêm `require_kernel: bool`; `serve_tls_with_offload` (`:1873`) là cửa đầy đủ. Cả ba `cfg(all(feature = "tls", feature = "standard", target_os = "linux"))` | `crates/engine/src/lib.rs` |
| **Engine chưa có bộ đọc PEM.** Test dựng chứng chỉ bằng `rcgen` trong bộ nhớ (dev-dep). `rustls-pki-types 1.15.1` (đã trong `Cargo.lock`) có `pem::PemObject` với `from_pem_file` sau `#[cfg(feature = "std")]`, và `rustls` 0.23.43 feature `std` bật `pki-types/std` — **loader có sẵn trong cây, không cần dependency mới** | `~/.cargo/registry/src/*/rustls-pki-types-1.15.1/src/pem.rs:8-37`; `rustls-0.23.43/Cargo.toml:92-96`; `crates/engine/tests/tls_wire.rs:129-137` |
| `tls::server_config(certs, key)` gọi **`with_no_client_auth()`** — không có tham số CA. Một key `TlsCaFile` hôm nay **không có gì đọc** | `crates/engine/src/tls.rs:550-561` |
| `TlsRequireKernel` từ chối ở **hai chỗ** (ADR-0060 quyết định 1): lúc khởi động, `serve_tls_with_offload` dò kernel một lần và trả `ServeError::Tls("TlsRequireKernel=Y, and this kernel cannot offload TLS…")` **trước khi bind** (`lib.rs:1897-1910`); mỗi connection, handshake rơi về `Userspace` thì `EventKind::TlsFellBackToUserspace` được phát và, nếu `require_kernel`, connection bị kết thúc với `DropReason::RefusedByDeployment` (`lib.rs:599-612`). Cờ đi vào bằng `engine.require_kernel(bool)` (`:644`) | `crates/engine/src/lib.rs`; ADR-0060 |
| **QuickFIX C++ tại SHA đã ghim `386ce46e` không có key `SocketUseSSL`** — chọn SSL bằng lớp `SSLSocketAcceptor`; key của nó là `ServerCertificateFile`, `ServerCertificateKeyFile`, `CertificationAuthoritiesFile`, `CertificateVerifyLevel`, `SSLProtocol`, `SSLCipherSuite`, `TLSCipherSuites` (acceptor), `ClientCertificateFile`/`ClientCertificateKeyFile` (initiator), đều ở `[DEFAULT]`. `SocketUseSSL` là tên của **QuickFIX/J** | `grep -rn SocketUseSSL vendor/quickfix-src/src` rỗng; `vendor/quickfix-src/README.SSL`; [SessionSettings.h](https://raw.githubusercontent.com/quickfix/quickfix/master/src/C++/SessionSettings.h) |
| Luật đặt tên key của repo này, viết ở một chỗ: **lấy tên QuickFIX C++ khi C++ có**, vì `interop.sh` chạy với C++; C++ không có thì lấy tên J/n (`EnableLastMsgSeqNumProcessed`); không ai có thì tự đặt (`ReconnectCeiling`) | `docs/CONFIGURATION.md:69-77`; `settings.rs:121-133` |
| `DESIGN.md` D11 dòng *"Still not built"* vẫn liệt kê `TlsRequireKernel` và event fallback — **đã cũ** từ khi 4b merge (PR #61) | `docs/DESIGN.md:652-653` |
| `CONFIGURATION.md` §4 (bảng Cargo feature) **chưa có hàng `tls`** dù feature tồn tại từ bước 1 | `docs/CONFIGURATION.md:289-292` |
| Job `tls` trong CI chạy `cargo clippy --all-targets --features tls` và `cargo test -p fixbolt-engine --features tls --no-fail-fast` trên runner có kTLS | `.github/workflows/ci.yml:415-470` |

## Cách làm

### A. Item 58 — một script mới, kiểm chung cho mọi lint, không chỉ `indexing_slicing`

**Script mới `scripts/check-no-crate-root-allow.sh`, không phải assertion thêm vào
`check-indexing-debt.sh`.** Ba lý do:

1. `check-indexing-debt.sh` là **cái thước** của một lint; nó chạy clippy hai lần, cần cả
   workspace compile, và **sẽ bị xoá** khi trần về sàn (item 55). Cái canh gốc crate phải sống
   lâu hơn cái thước.
2. Lớp lỗi rộng hơn một lint: `#![allow(clippy::unwrap_used)]` ở gốc crate tắt bất biến 7 y
   hệt, và `check-lint-config.sh` — thứ chứng minh workspace deny `unwrap` — **không nhìn thấy**,
   vì nó kiểm `Cargo.toml` chứ không kiểm source. Một check theo tên lint là một check phải sửa
   mỗi lần thêm lint.
3. Kiểm văn bản chạy trong một giây, không build, nên đặt được vào job `lint-config` cạnh
   `check-lint-config.sh` — hai check về cùng một câu: *config lint có với tới mọi module không*.

**Kiểm chung, không theo lint**: bất kỳ inner attribute nào ở gốc crate **nới** lint đều đỏ. Cái
được phép là siết (`forbid`, `deny`), `no_std`, `doc`, `warn` với lint workspace *không* deny.
`expect` ở gốc crate bị coi như `allow` — nó tắt lint cả crate y hệt, chỉ khác là kêu khi không
có site nào.

**Phạm vi: mọi target `lib` và `bin` của package dưới `crates/`**, lấy từ `cargo metadata` chứ
không từ tên file — để một `crates/x/src/bin/foo.rs` tương lai không lọt. **Không bao gồm
`tools/`**, và đây là chọn có chủ ý: bất biến 7 nói về *crate thư viện*; `check-indexing-debt.sh`
cũng cố ý không đếm `tools/`; hai `#![allow]` ở `w2w` và `interop` là có chủ ý, có comment, và một
check phải mang allow-list ngay ngày đầu là một check có hai lỗ được đặt tên sẵn. Nếu ngày nào
`tools/` cần canh, mở rộng bằng cách bỏ bộ lọc `crates/` và scope hai allow đó — ghi ở *Ngoài
phạm vi*.

**Ba assertion**, mỗi cái một câu FAIL riêng, để đảo chiều đỏ được đúng câu:

| # | Kiểm gì | Câu FAIL |
|---|---|---|
| A1 | Mỗi `src_path` của target `lib`/`bin` dưới `crates/`: không dòng nào khớp `^\s*#!\[\s*(cfg_attr\([^]]*,\s*)?(allow|expect)\b` | `check-no-crate-root-allow: FAIL — crate-root allow at <file>:<line>: <dòng>` |
| A2 | Cùng các file: không dòng nào khớp `^\s*#!\[\s*warn\(` mà nêu một lint workspace đang `deny` (danh sách lấy bằng grep `= "deny"` trong `Cargo.toml`, không viết cứng) | `check-no-crate-root-allow: FAIL — crate-root warn lowers a workspace deny at <file>:<line>` |
| A3 | Mỗi `manifest_path` thành viên workspace: khối `[lints]` là đúng một dòng `workspace = true`, và không có `[lints.` nào khác | `check-no-crate-root-allow: FAIL — <Cargo.toml> does not inherit the workspace lints` |
| A0 | **Số 0 không phải pass**: nếu `cargo metadata` trả 0 gốc crate dưới `crates/`, exit 1 nói *"0 crate roots found — broken invocation, not a clean workspace"* | như `check-indexing-debt.sh:116` |

Kết thúc: `check-no-crate-root-allow: ok — <N> crate roots, <M> manifests`. Cần `jq` (đã dùng
trong `bench.sh` và CI). Chạy trong cây, nên `rust-toolchain.toml` với tới `cargo metadata`.

**Cái check này không thấy, nói trước**: một `#[allow]` *outer* trên `mod foo;` (tắt một module,
không phải cả crate — đó là phạm vi item 55 và trần đếm được); `RUSTFLAGS=-A …` từ môi trường
(CI cố ý không đặt `RUSTFLAGS`, `ci.yml:25-29`); `--cap-lints` từ một lệnh ngoài repo; và một
`allow` được sinh ra bởi `include!` (item 58 đã gặp — attribute phải do generator phát, và nó
phát *lên hàm*, không lên gốc crate, nên không lọt A1).

### B. Item 23 — một script mới, luật theo *hành vi* chứ không theo *tên script*

**Script mới `scripts/check-scratch-fixtures.sh`.** Câu hỏi của brief: *grep `mktemp -d` rồi
`cargo`, với allow-list ba script chỉ ghi output* — **không, allow-list theo tên là sai hình
dạng**. Một script trong danh sách được miễn mãi mãi; ngày nó mọc thêm một `cargo build` trong
`$TMP` thì không ai kiểm — đúng cái lỗ mà plan này tồn tại để bịt. Luật đúng là luật rustup thật
sự dùng: **cái châm ngòi là `cd` vào thư mục ngoài cây**, không phải `mktemp`. Ba script kia không
`cd` vào `$TMP`, nên chúng qua mà không cần được nêu tên.

**Bốn assertion:**

| # | Kiểm gì | Câu FAIL |
|---|---|---|
| B0 | Tập artefact ghim = những file **đang tồn tại** ở gốc trong danh sách `rust-toolchain.toml`, `rust-toolchain`, `clippy.toml`, `.clippy.toml`, `.cargo/config.toml`, `rustfmt.toml`, `.rustfmt.toml`. Hôm nay là một. **Nếu tập rỗng → exit 1**: check thành vô nghĩa, và một `rust-toolchain.toml` bị xoá là chuyện phải nghe thấy | `check-scratch-fixtures: FAIL — no pinning artefact at the repo root; nothing to copy means nothing to check` |
| B1 | Với mỗi `scripts/*.sh`: thu **biến tạm** — tên gán từ `mktemp`, `$TMPDIR`, `/tmp/`, rồi lặp tới bất động: tên gán từ `$<biến tạm>`. Với `check-lint-config.sh` đó là `TMP` rồi `CRATE` | — |
| B2 | Dòng **vào** thư mục tạm: `cd`/`pushd` với đối số chứa `$<biến tạm>`, hoặc `--manifest-path`/`-C` trỏ vào nó. Mỗi dòng vào **phải** đi kèm, trong cùng script, một dòng `cp` mang tên **từng** artefact của B0 và `$<biến tạm>` (hoặc biến dẫn xuất) | `check-scratch-fixtures: FAIL — scripts/<x>.sh:<line> enters a scratch dir ($VAR, from mktemp at :<line>) and never copies <artefact> into it` |
| B3 | `grep -rn 'Command::new("cargo")\|Command::new("rustc")' crates tools` phải rỗng. Một test Rust sinh toolchain trong `temp_dir()` mắc đúng lớp này, và check này chưa biết đọc Rust — nên nó **từ chối và bảo đến sửa check trước** | `check-scratch-fixtures: FAIL — <file>:<line> spawns the toolchain from Rust; teach this check where it runs before adding one` |
| B4 | Số 0 không phải pass: 0 script quét → exit 1 | tương tự A0 |

Kết thúc: `check-scratch-fixtures: ok — <N> scripts, <M> enter a scratch dir, <P> pins`.

**Cái check này không thấy, nói trước**: `cd` qua hàm hoặc `eval`; `cd -- "$(dirname "$X")"`;
fixture viết trong một bước `run:` của `ci.yml` dưới `$RUNNER_TEMP`; script Python; artefact ghim
nằm ở **máy** (`$CARGO_HOME/config.toml`) — thứ không repo nào ghim được; và hướng lỗi thứ hai của
reference (copy đúng file nhưng toolchain đó chưa cài) — rustup tự cài, và ở máy làm việc §9 bắt
đọc output. Với mỗi cái, câu trả lời là *khi nó xuất hiện, mở rộng check trong cùng commit*, và
comment đầu script liệt kê danh sách này.

**Không dời crate tạm của `check-lint-config.sh` vào trong cây** (`target/scratch/…`), dù reference
gọi đó là *"cách rẻ hơn"*: một crate nằm dưới thư mục workspace mà không phải thành viên bị cargo
từ chối nếu thiếu `[workspace]` rỗng — một bẫy mới để đổi một bẫy đã đóng, và gate đó đang xanh
đã chứng minh bằng đảo chiều. Ghi ở *Ngoài phạm vi*.

### C. Bước 4c — dựng theo *Sửa 5* của plan TLS, sau khi được duyệt

Sửa 3 viết: *"Năm key vào `Key` enum (`SocketUseSSL`, `TlsCertificate`, `TlsPrivateKey`,
`TlsCaFile`, `TlsRequireKernel`) + `docs/CONFIGURATION.md`, kiểm hai chiều như 26 key kia. Máy:
bất kỳ."* **Không dựng được đúng như thế**, và mỗi lý do đã có nguồn ở bảng trên:

1. **"Kiểm hai chiều như 26 key kia" — không tồn tại.** 26 key kia được compiler ép vét cạn ở bốn
   `match` trong code, còn phía `CONFIGURATION.md` chưa ai kiểm; câu *"Twenty-three keys"* nằm
   cạnh 26 hàng là bằng chứng. Gate phải có **trước** bước (`CLAUDE.md` §1), nên 4c bắt đầu bằng
   việc dựng nó.
2. **`serve_tls` nhận DER, không nhận đường dẫn**, và engine chưa đọc được PEM. Một key chứa
   đường dẫn cần một loader, và loader đó là code sau feature `tls` — không phải *"máy: bất kỳ"*
   cho toàn bước.
3. **`TlsCaFile` không có gì đọc** (`with_no_client_auth()`), và nhật ký giao hàng của chính plan
   TLS nói *thêm key mà chưa gì đọc là một lời hứa trong tài liệu, tệ hơn là chưa thêm*. Bỏ khỏi
   4c; quay lại cùng cơ chế client-auth (ADR-0005 câu 4).
4. **Tên key trái luật đặt tên của repo**: C++ có `ServerCertificateFile`/`ServerCertificateKeyFile`;
   Sửa 3 đặt `TlsCertificate`/`TlsPrivateKey`. `SocketUseSSL` thì đúng luật (C++ không có key
   bật/tắt — chọn bằng lớp — nên lấy tên J, tiền lệ `EnableLastMsgSeqNumProcessed`).
   `TlsRequireKernel` tự đặt, đúng luật (tiền lệ `ReconnectCeiling`). **Chủ sở hữu chọn** — ghi ở
   Sửa 5; brief cho developer chặn ở quyết định này.

Hình dạng đề xuất (chi tiết ở Sửa 5): **bốn key**, tất cả `[DEFAULT]`-only (một listener, một
chứng chỉ — SNI đã ở ngoài phạm vi), chỉ acceptor (initiator TLS là bước 5, với
`ClientCertificateFile`); `Settings::tls() -> Option<&TlsSettings { certificate: PathBuf,
private_key: PathBuf, require_kernel: bool }>`; **`SocketUseSSL=Y` trên build không có feature
`tls` bị từ chối ngay lúc parse** bằng `Problem::NeedsFeature` — một `#[cfg(not(feature = "tls"))]`
duy nhất trong `settings.rs`, và đó là câu trả lời cho bất biến 6: *refused cleanly*, không phải
*parses*; `into_table()` **từ chối** file có `SocketUseSSL=Y` (`Problem::NeedsTlsDoor`, cùng
hình với `WrongRole` — cái sai đắt là một file xin TLS được rót vào `serve` thường và phục vụ
plaintext mà không ai thấy); `into_tls_table() -> (Table, TlsSettings)` là cửa đúng;
`tls::load_pem(&TlsSettings)` sau feature `tls` trả `(Vec<CertificateDer>, PrivateKeyDer)` bằng
`PemObject::from_pem_file` — không dependency mới.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **7 — không `unwrap`/`expect`/`panic` trong crate thư viện** | A và B là hai máy canh **mới** cho bất biến này: A giữ cho `deny` với tới mọi module, B giữ cho gate của `deny` chạy trên đúng compiler. Code 4c thêm vào `settings.rs`/`tls.rs` đi qua clippy như mọi code | `check-no-crate-root-allow.sh`, `check-scratch-fixtures.sh`, cả hai chứng minh bằng đảo chiều; `cargo clippy --all-targets -- -D warnings` có và không `--features tls` |
| **6 — feature gate `mod`, `--no-default-features` build được** | 4c thêm key mà `settings.rs` không có feature. **Quyết định: parse từ chối `SocketUseSSL=Y` khi không có feature `tls`**, không lặng lẽ nhận. `load_pem` nằm trong `mod tls`, đã sau `#[cfg]` (`lib.rs:55`) | test `socket_use_ssl_is_refused_without_the_tls_feature` chạy trong `cargo test --no-default-features`; `check-no-optional-deps.sh` không đổi vì không có dependency mới |
| 1 — không cấp phát hot path | không đụng: settings và loader chạy lúc khởi động, trước khi có connection | `benches/alloc.rs` không đổi, vẫn chạy trong job `bench` |
| 2, 3 — session thuần, 59 `.def` | không đụng `crates/session` | — |
| 4 — mode | không đụng engine thread | — |

Không đụng `codec`, `session`, `transport.rs`. Đụng `engine` (`settings.rs`, `tls.rs`, tests).

## Chia việc

Tier theo `CLAUDE.md` §12: mọi bước chạm `crates/` là **senior developer (opus)** tối thiểu;
script với spec viết sẵn từng assertion là **developer (sonnet)**; docs đồng bộ là sonnet vì phải
đọc và chọn chữ. Không bước nào là runner (haiku): không bước nào có đúng một câu trả lời.

| Bước | Kết quả | Phụ thuộc | Tier | Máy |
|---|---|---|---|---|
| 1 | `scripts/check-no-crate-root-allow.sh` với A0–A3 đúng câu FAIL ở mục A; gọi trong job `lint-config` của `ci.yml` ngay sau `check-lint-config.sh`; ba đảo chiều R-A1, R-A2, R-A3 ở *Cách kiểm chứng* chạy và **trích output**, mỗi cái đỏ đúng câu dự đoán | — | sonnet | bất kỳ |
| 2 | `scripts/check-scratch-fixtures.sh` với B0–B4; cùng job CI; ba đảo chiều R-B1, R-B2, R-B3 chạy và trích output. Files: chỉ script mới và `ci.yml` (dòng thêm cách xa dòng của bước 1 — nếu manager muốn song song thì bước 2 nhận `ci.yml` sau bước 1) | — | sonnet | bất kỳ |
| 3 | Đảo chiều **hiệu ứng** R-A4 (dán `v[0]` vào module sạch khi có/không có `#![allow]` giả) — nối kiểm văn bản với lỗi thật, chạy **một lần** và ghi vào plan; rồi docs của A+B: `CLAUDE.md` §2 đoạn *Machine-checked today* cho bất biến 7, `DESIGN.md` §6 bảng *Mode and machine* hai hàng, `STATUS.md` item 58 và 23 đóng (kèm câu FAIL đã quan sát), hai file `docs/reference/` thêm một dòng *"canh bởi `scripts/…`"* | 1, 2 | sonnet | có cargo |
| 4 | **Gate của 4c, trước 4c**: test unit trong `settings.rs` (mod `tests`, `#[cfg(test)]`) đọc `include_str!("settings.rs")` lấy mọi literal trong `fn name` — match vét cạn nên danh sách **đầy đủ do compiler** — và `std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/CONFIGURATION.md"))`; assert **hai chiều**: mỗi tên có hàng `| \`Tên\` |` trong §1 và mỗi hàng §1 `Key::parse` được; cộng `parse(name(k)) == Some(k)`. Đỏ trước: sửa `CONFIGURATION.md:21` "Twenty-three" → "Twenty-six" trong cùng commit (test không kiểm câu đó — nói ra để không ai tưởng) | — | opus | bất kỳ |
| 5 | **4c-1, chỉ khi Sửa 5 đã duyệt và tên key đã chọn**: bốn key vào `Key`, `Block`, `Settings::tls()`, `into_tls_table()`, `Problem::{NeedsFeature, NeedsTlsDoor}`, các từ chối ở mục C; test trong `crates/engine/tests/settings.rs` (tên ở *Cách kiểm chứng*); test bước 4 phải **đỏ trước** khi thêm hàng doc, xanh sau | 4, Sửa 5 | opus | bất kỳ, **cả hai feature set** |
| 6 | **4c-2**: `tls::load_pem` + test Linux `crates/engine/tests/tls_settings_wire.rs`: một `.cfg` thật (PEM do `rcgen` ghi ra `temp_dir()`) → `Settings::parse` → `into_tls_table` → `load_pem` → `serve_tls_requiring` → client `rustls` logon. **Bước 5 không merge nếu thiếu bước 6** — key không ai đọc là lời hứa | 5 | opus | Linux, `--features tls` |
| 7 | Docs 4c: `CONFIGURATION.md` bốn hàng §1 + hàng `tls` ở §4 (thiếu từ bước 1 của plan TLS); `GUIDE.md` mục TLS (`:1473`) nói file cấu hình; `DESIGN.md` D11 *"Still not built"* sửa cho đúng sau 4b và 4c; `CHANGELOG.md` (API mới: `Settings::tls`, `into_tls_table`, `tls::load_pem`, hai `Problem`); nhật ký giao hàng plan TLS | 5, 6 | sonnet | bất kỳ |

**Nếu phải cắt:** sau bước 3 thì item 58 và 23 đóng, 4c chưa chạm — PR vẫn có nghĩa. Sau bước 4
thì có thêm gate doc↔key (tự nó đã sửa một câu sai). **Không được dừng giữa 5 và 6.**

## Cách kiểm chứng

Mọi đảo chiều **ghi assertion dự đoán trước**, chạy, so câu FAIL thật với câu dự đoán, rồi khôi
phục và thấy `ok`. Đỏ ở câu khác là một phát hiện, không phải pass.

**A — `scripts/check-no-crate-root-allow.sh`**

| # | Làm gì | Dự đoán đỏ ở |
|---|---|---|
| R-A1 | Thêm `#![allow(clippy::indexing_slicing)]` làm dòng 1 của `crates/dict/src/lib.rs` | A1: `crate-root allow at crates/dict/src/lib.rs:1` |
| R-A2 | Thêm `#![warn(clippy::unwrap_used)]` vào dòng 1 của `crates/dict/src/lib.rs` | A2: `crate-root warn lowers a workspace deny at crates/dict/src/lib.rs:1` |
| R-A3 | Trong `crates/dict/Cargo.toml` thay `workspace = true` bằng `[lints.clippy]` + `indexing_slicing = "allow"` | A3: `crates/dict/Cargo.toml does not inherit the workspace lints` |
| R-A4 (hiệu ứng, chạy một lần ở bước 3) | Với R-A1 tại chỗ: thêm `fn probe(v: &[u8]) -> u8 { v[0] }` vào một module sạch của `dict`, chạy `cargo clippy -p fixbolt-dict --lib`; rồi bỏ R-A1, chạy lại | Lần 1 **exit 0** (lỗi item 58 tái hiện); lần 2 **đỏ** `indexing may panic` tại `probe`. Đây là bằng chứng kiểm văn bản đỏ **đúng lúc** hiệu ứng có thật |

Sau mỗi cái: khôi phục, chạy lại, thấy `check-no-crate-root-allow: ok — 6 crate roots, 9 manifests`
(số 6 = 6 target lib dưới `crates/`; developer đọc số thật từ `cargo metadata` và ghi vào comment).

**B — `scripts/check-scratch-fixtures.sh`**

| # | Làm gì | Dự đoán đỏ ở |
|---|---|---|
| R-B1 (trường hợp cũ) | Xoá dòng `cp "$ROOT/rust-toolchain.toml" …` trong `check-lint-config.sh` | B2: `scripts/check-lint-config.sh:<dòng cd "$CRATE"> enters a scratch dir ($CRATE, from mktemp at :19) and never copies rust-toolchain.toml` |
| R-B2 (**lớp**, không phải trường hợp) | Thêm `( cd "${TMP}" && cargo build )` vào cuối `check-no-kernel-sleep.sh` | B2 nêu `check-no-kernel-sleep.sh` và dòng mới — script này **không** nằm trong allow-list nào, vì không có allow-list |
| R-B3 (artefact mới) | `touch clippy.toml` ở gốc | B2 nêu `check-lint-config.sh` thiếu `clippy.toml` — danh sách ghim **suy ra từ cái đang có**, không viết cứng |
| R-B4 | Thêm `let _ = std::process::Command::new("cargo");` vào một test bất kỳ | B3 |

Không chạy lại thí nghiệm "máy không có `rustup default`" — reference đã đo 2026-08-31 và số đó
được trích, không dựng lại.

**C — 4c**

- Bước 4: test đỏ trước bằng cách thêm một hàng giả `| \`NoSuchKey\` |` vào doc → đỏ ở
  assertion *"doc row has no Key"*; xoá → xanh. Rồi tạm bỏ hàng `TimestampPrecision` → đỏ ở
  *"Key has no doc row"*, khôi phục.
- Bước 5, `cargo test -p fixbolt-engine --test settings` **và** cùng lệnh với
  `--no-default-features`; tên test: `tls_keys_parse_into_tls_settings`,
  `socket_use_ssl_without_a_certificate_is_missing_key`,
  `a_certificate_without_socket_use_ssl_is_refused`, `tls_keys_in_a_session_block_are_default_only`,
  `tls_keys_on_an_initiator_file_are_wrong_role`, `into_table_refuses_a_file_that_asks_for_tls`,
  `socket_use_ssl_is_refused_without_the_tls_feature` (`#[cfg(not(feature = "tls"))]`) và bản
  đối ngẫu `#[cfg(feature = "tls")]`. Đảo chiều: bỏ `#[cfg(not(feature = "tls"))]` arm → test
  `…without_the_tls_feature` đỏ ở assertion `Problem::NeedsFeature`, không ở chỗ khác.
- Bước 6, Linux: `cargo test -p fixbolt-engine --features tls --test tls_settings_wire`. Đảo
  chiều: hoán đổi đường dẫn cert và key trong `.cfg` → dự đoán đỏ **ở `Err(ServeError::Tls)`
  của `load_pem`**, trước khi bind — không phải ở socket (nếu đỏ ở socket thì loader đã nạp sai
  thứ mà không kêu, và đó là phát hiện).
- Đóng plan: `cargo test --all`, `cargo test --all --no-default-features`, clippy hai lần,
  `check-indexing-debt.sh` (trần không đổi — code mới dùng `get`), `check-no-optional-deps.sh`,
  bốn script `check-*` mới và cũ của job `lint-config`, và **một run CI xanh nêu id** cho commit
  đóng.

## Tài liệu phải cập nhật

Theo bảng `CLAUDE.md` §4, đi từng hàng:

- [ ] **Thêm gate** → `CLAUDE.md` §2 đoạn *Machine-checked today*, bất biến 7: thêm hai script và
      nói mỗi cái canh gì; `DESIGN.md` §6 bảng *Mode and machine*: hai hàng *Gate / Target /
      Proven by* cạnh hàng `check-lint-config.sh` (`:853`); `.github/workflows/ci.yml` job
      `lint-config` (đổi tên job cho đúng: nó canh *ba* thứ)
- [ ] **Chứng minh thứ đã ghi là chưa chứng minh** → `STATUS.md` item 58 và 23 đóng, nêu câu FAIL
      quan sát được và R-A4; mục *Not proven* không có bullet về hai lớp này (đã grep 2026-09-12),
      nên không có gì để gạch
- [ ] **Bẫy đã trả giá** → hai file `docs/reference/` hiện có (`an-allow-at-the-top-of-a-file…`,
      `a-scratch-fixture-inherits-the-machine.md`) thêm một dòng *regression test* nêu script —
      luật *"mỗi bẫy ghi lại đều có test canh"*. Giữ nguyên `[to testing-skills]`
- [ ] **Key cấu hình mới** → `docs/CONFIGURATION.md`: bốn hàng §1 **trong cùng commit với bước 5**,
      hàng `tls` ở bảng §4, câu đếm key; `GUIDE.md` mục TLS
- [ ] **Public API `engine`** → `CHANGELOG.md`; rustdoc; `DESIGN.md` D11 *"Still not built"*
- [ ] **Plan TLS** → nhật ký giao hàng ghi 4c xong theo Sửa 5, và *Việc dở dang* cập nhật
- [ ] **ADR**: không cần. Hai script không phải quyết định đắt hay khó đảo; tên key theo luật đã
      viết ở `CONFIGURATION.md:69-77`, một chỗ. Nếu chủ sở hữu chọn tên **khác** luật đó thì luật
      phải sửa ở đúng chỗ đó, không thêm chỗ thứ hai
- [ ] **Không cần**: `PRD.md`, `SESSION-BEHAVIOUR.md` (không đụng session), `CONFORMANCE.md`
      (không có số conformance), `best-practices-*.md`, `hft-playbook.md`

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Check A/B xanh vì **không quét gì** (cargo metadata hỏng, glob rỗng, `CARGO_TERM_COLOR` bọc output) | A0, B0, B4: số 0 là exit 1; script in số đếm ở dòng `ok` |
| Đảo chiều đỏ ở **assertion khác** assertion dự đoán | Mỗi đảo chiều ghi câu FAIL dự đoán trước; developer trích output, manager so |
| Regex A1 bỏ sót `#![cfg_attr(…, allow(…))]` hoặc `# ! [ allow` có khoảng trắng | Regex cho phép `\s*` ở mọi khe; R-A1 chạy thêm một biến thể `#![cfg_attr(test, allow(clippy::indexing_slicing))]` |
| B1 không theo được chuỗi biến hai bậc (`TMP` → `CRATE`) | R-B1 dùng đúng `check-lint-config.sh`, nơi chuỗi là hai bậc |
| Test bước 4 đọc doc bằng đường dẫn tương đối và **xanh khi file không có** | `read_to_string(...).expect` bị cấm — test dùng `match` và **fail** khi không đọc được, với message nêu đường dẫn |
| `include_str!("settings.rs")` bắt cả literal ngoài `fn name` | Test cắt đúng khối `fn name` (từ `const fn name` đến dấu `}` đóng match) rồi mới lấy literal; assert số literal == số arm `parse` (đếm `=> Some(Self::`) |
| Bước 5 xanh ở feature này, đỏ ở feature kia | Cả hai lệnh test chạy ở bước 5, output trích riêng; CI có sẵn job `no-default-features` và job `tls` |
| `into_table()` cũ vẫn nhận file `SocketUseSSL=Y` (plaintext lặng lẽ) | `into_table_refuses_a_file_that_asks_for_tls` |
| Hoán đổi cert/key trong `.cfg` mà loader **không kêu**, lỗi hiện ở socket | Đảo chiều bước 6 dự đoán `ServeError::Tls`; đỏ ở chỗ khác = mở phát hiện |
| Hai bước song song cùng sửa `ci.yml` | Bước 2 nhận `ci.yml` sau bước 1, hoặc manager gộp hai dòng vào một lần sửa |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Kiểm văn bản (A, B) chỉ thấy hình dạng nó biết; hình dạng mới lọt | Vừa, **chấp nhận** | Mỗi script liệt kê cái nó không thấy ở comment đầu; R-A4 nối một lần với hiệu ứng thật; item 58/23 đóng với câu *"canh hình dạng đã gặp, không phải mọi hình dạng"* |
| Chủ sở hữu chọn tên key khác đề xuất, brief bước 5 phải viết lại | Thấp | Bước 5 chặn ở Sửa 5; bước 1–4 không phụ thuộc |
| Test bước 4 mong manh với định dạng bảng Markdown | Thấp | Chỉ khớp `^\| \`Tên\` \|` ở đầu dòng, đúng cái `grep -c` ở bảng *Đã biết chắc* đã dùng |
| `load_pem` nhận key PKCS#1/SEC1/PKCS#8 khác nhau; `rcgen` sinh PKCS#8 | Thấp | `PrivateKeyDer::from_pem_file` nhận cả ba (pki-types `pem.rs`); test chỉ chứng minh PKCS#8 — nói rõ trong rustdoc là *chưa đo* hai loại kia |
| `tools/` vẫn có `#![allow]` ở gốc crate sau plan này | Thấp, **có chủ ý** | Ghi ở *Ngoài phạm vi* và trong comment của script A |

## Ngoài phạm vi

- **`tools/*` không được check A quét.** Hai `#![allow]` ở `w2w` và `interop` giữ nguyên với
  comment của chúng. Mở rộng khi có lý do — và khi đó phải scope `#![allow(unsafe_code)]` của
  `w2w` xuống từng `unsafe`, một việc riêng.
- **Không dời crate tạm của `check-lint-config.sh` vào trong cây.**
- **Không kiểm `ci.yml` `run:` blocks, Python, hay Makefile** cho lớp item 23.
- **`TlsCaFile`/`CertificationAuthoritiesFile`** — chờ cơ chế client-auth (ADR-0005 câu 4).
- **Initiator TLS và `ClientCertificateFile`** — bước 5 của plan TLS.
- **Hạ trần `check-indexing-debt.sh`** — item 55, không đụng.
- **`DESIGN.md` §8 số TLS, `w2w --tls`** — bước 6 plan TLS.

## Nhật ký giao hàng

### Bước 1 và 2 — hai script, xong 2026-09-12, commit `2bdbf3f`

`scripts/check-no-crate-root-allow.sh` (A0–A3) và `scripts/check-scratch-fixtures.sh` (B0–B4),
cả hai đã nối vào job `lint-config` của `ci.yml`, ngay cạnh `check-lint-config.sh`.

Số hai script đọc được ở cây sạch:

```
check-no-crate-root-allow: ok — 6 crate roots, 9 manifests
check-scratch-fixtures: ok — 19 scripts, 1 enter a scratch dir, 1 pins
```

Bảy đảo chiều, mỗi cái **viết câu FAIL dự đoán trước rồi mới chạy**, và mỗi cái đỏ đúng câu đã
dự đoán. Manager tự chạy lại R-A2 và R-B2:

```
check-no-crate-root-allow: FAIL — crate-root warn lowers a workspace deny at crates/dict/src/lib.rs:1
exit=1
check-scratch-fixtures: FAIL — scripts/check-no-kernel-sleep.sh:102 enters a scratch dir ($TMP, from mktemp at :31) and never copies rust-toolchain.toml into it
```

**Phát hiện đáng giá hơn cả hai script: A2 lần đầu viết ra đã *xanh* trên chính đảo chiều của
nó.** Không phải đỏ ở assertion khác — không đỏ gì cả. Bộ khớp của nó là
`(^|[^A-Za-z0-9_:])${lint}\b`, tức là loại dấu `:` ra khỏi những ký tự được phép đứng trước tên
lint, nên nó **không bao giờ khớp được một lint viết sau `::`** — mà mọi lint clippy thật đều
viết như thế. Cái assertion sinh ra để bắt `#![warn(clippy::unwrap_used)]` thì mù với
`clippy::`. Sửa thành `\b${lint}\b` và chạy lại cả ba đảo chiều A.

Đây là một nấc xa hơn item 58: ở đó **thứ được canh** bị tắt còn con số vẫn đọc đúng; ở đây
**chính cái canh** bị tắt, và chỉ một đảo chiều có câu FAIL viết sẵn từ trước mới nhìn thấy. Nếu
chỉ đọc *đỏ hay không đỏ* thì lần chạy đó được ghi là pass. Viết thành
[a-matcher-excluded-the-separator-every-real-name-uses.md](../reference/a-matcher-excluded-the-separator-every-real-name-uses.md),
có `[to testing-skills]`.

### Bước 3 — đảo chiều hiệu ứng R-A4, rồi tài liệu. Xong 2026-09-12

**R-A4 chạy đúng một lần, dự đoán viết trước khi chạm vào cây.**

Chọn module nào của `dict`: crate này chỉ có hai file nguồn — `lib.rs` và `field_type.rs` —
và `field_type.rs:20` đã mang `#![allow(clippy::indexing_slicing)]` của riêng nó, nên nó
**không sạch**. Kiểm bằng `grep -rn '^#!\[' crates/*/src` và `ls crates/dict/src`. Module sạch
duy nhất là chính gốc crate. Nên thí nghiệm tạo thêm một **file module mới, sạch**,
`crates/dict/src/probe_scratch.rs`, khai báo bằng `pub mod probe_scratch;` trong `lib.rs` — đúng
hình dạng của item 58 (một submodule sạch bị `#![allow]` ở gốc crate tắt hộ), chứ không phải
cùng một file tự tắt chính nó.

Dự đoán, viết trước:

- **Lần 1**, có `#![allow(clippy::indexing_slicing)]` ở dòng 1 của `crates/dict/src/lib.rs`:
  `cargo clippy -p fixbolt-dict --lib` **exit 0**, không có chữ `indexing may panic` nào.
- **Lần 2**, bỏ đúng dòng đó, mọi thứ khác giữ nguyên: **exit khác 0**, có
  `error: indexing may panic` chỉ vào `v[0]` trong `probe`.

Lần 1, output nguyên văn và exit tách riêng:

```
EXIT_STATUS_RUN1=0
   Compiling fixbolt-dict v0.0.0 (/home/tmt/Projects/nanofixengine/crates/dict)
    Checking fixbolt-codec v0.0.0 (/home/tmt/Projects/nanofixengine/crates/codec)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.95s
```

`grep -c 'indexing may panic'` trên output đó: **0**.

Lần 2:

```
EXIT_STATUS_RUN2=101
    Checking fixbolt-dict v0.0.0 (/home/tmt/Projects/nanofixengine/crates/dict)
error: indexing may panic
 --> crates/dict/src/probe_scratch.rs:2:5
  |
2 |     v[0]
  |     ^^^^
  |
  = help: consider using `.get(n)` or `.get_mut(n)` instead
  = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.98.0/index.html#indexing_slicing
  = note: requested on the command line with `-D clippy::indexing-slicing`

error: could not compile `fixbolt-dict` (lib) due to 1 previous error
```

**Cả hai dự đoán đúng.** Đây là thứ nối kiểm **văn bản** của A1 với **hiệu ứng** thật của trình
biên dịch: A1 đỏ đúng lúc lint thật sự bị tắt, chứ không phải đỏ vì một luật văn bản tự đặt ra.

Cây được khôi phục ngay sau đó: xoá `probe_scratch.rs`, `git checkout -- crates/dict/src/lib.rs`.
`git status --porcelain` và `git diff -- crates/` đều rỗng trước khi viết tài liệu.

**Tài liệu — đi từng hàng bảng `CLAUDE.md` §4:**

- `docs/reference/a-matcher-excluded-the-separator-every-real-name-uses.md` — **mới**, là bẫy đắt
  nhất của bước 1 (hàng *bẫy đã trả giá*, ưu tiên cao nhất). Có `[to testing-skills]`; đoạn tổng
  quát hoá không nhắc FIX, engine hay bất cứ thứ gì của lĩnh vực này.
- `CLAUDE.md` §2, đoạn *Machine-checked today*: thêm hai script vào phần bất biến 7, giữ nguyên
  chữ cũ. `check-scratch-fixtures.sh` được ghi là **bất biến 7 ở một tầng lùi — nó canh cái
  gate, không canh cái luật**: nó không chứng minh rằng không có `unwrap` trong crate thư viện,
  nó chứng minh rằng thứ đi chứng minh điều đó chạy trên đúng compiler của dự án.
- `DESIGN.md` §6, bảng *Mode and machine*: hai hàng mới ngay dưới hàng `check-lint-config.sh`.
  Bảng trên đĩa đúng như plan giả định.
- `STATUS.md` item **58** và **23** đóng. 23 đóng **theo lớp**, và ghi rõ cả hai vế: cái gì giờ
  đã được giữ, và cái gì vẫn không — `cd` qua hàm hay `eval`, `cd -- "$(dirname "$X")"`, fixture
  trong một bước `run:` của `ci.yml`, script Python, và artefact ghim ở mức **máy**
  (`$CARGO_HOME/config.toml`) mà không file nào trong repo ghim được.
- `STATUS.md` mục *Not proven*: **grep lại 2026-09-12, không có bullet nào về gốc crate, fixture
  tạm, hay chuyện cấu hình lint có với tới mọi module — nên không có gì để gạch.** Đúng như hàng
  tương ứng trong mục *Tài liệu phải cập nhật* của plan này đã nói.
- `STATUS.md` mục *Not proven*: **thêm** một bullet — chưa script mới nào đi qua `shellcheck`; nó
  không có trên máy này và không job CI nào chạy nó trên `scripts/`.
- Hai file `docs/reference/` cũ (`an-allow-at-the-top-of-a-file-silenced-the-whole-crate.md`,
  `a-scratch-fixture-inherits-the-machine.md`) mỗi file thêm một đoạn *"Guarded since 2026-09-12
  by `scripts/…`"* — luật *"prose does not hold a constraint"*.

Cổng chạy lại sau khi sửa tài liệu: `python3 scripts/check-links.py`,
`bash scripts/check-no-crate-root-allow.sh`, `bash scripts/check-scratch-fixtures.sh` — output
trích trong báo cáo của bước.

### Việc dở dang sau bước 3

- Bước **4–7** (gate doc↔key, rồi 4c) **chưa bắt đầu**. Nếu cắt ở đây thì PR vẫn có nghĩa: item
  58 và 23 đóng, 4c chưa chạm — đúng như mục *Nếu phải cắt* của plan.
- **Chưa có id run CI** cho commit đóng. §9 hộp cuối chưa đánh dấu được.
- **`shellcheck` chưa chạy** trên hai script mới — ghi ở *Not proven* của `STATUS.md`.

### Bước 4 — gate hai chiều doc↔key, xong 2026-09-12, commit `348b9a8`

Bước 4c (`Sửa 3` của plan TLS) tự nhận là "kiểm hai chiều như 26 key kia", nhưng **gate đó chưa
từng tồn tại**: không test, script hay job CI nào đọc `docs/CONFIGURATION.md` để đối chiếu với
`Key`. `#[cfg(test)] mod doc_table` trong `settings.rs` đọc `include_str!("settings.rs")`, lấy
mọi literal chữ trong các nhánh của `Key::name` (`match` không có nhánh `_`, nên danh sách này
**đầy đủ do trình biên dịch giữ**, không phải do tay), rồi so với từng hàng `| \`Tên\` |` của §1
— cả hai chiều — cộng `Key::parse(name(k)) == Some(k)` cho mọi key. Hai chốt chống-câm thêm vào
ngoài yêu cầu của plan, rút từ chính bài học buổi sáng rằng một guard không khớp được thì không
có gì để quan sát: một nhân chứng thứ hai gạo `Key::parse` độc lập rồi so hai tập hợp, và một sàn
26 chỉ được tăng.

**Tiền đề của bước không đúng, và đó chính là phát hiện.** Plan dự đoán test đỏ ngay khi viết, vì
`CONFIGURATION.md:21` nói *"Twenty-three keys"* cạnh 26 hàng. Nó **xanh**: bảng §1 và enum `Key`
đã khớp đúng nhau, chỉ câu văn xuôi ở dòng 21 sai — và test không đọc câu văn xuôi, một doc
comment phía trên module nói rõ điều đó để sau này không ai tưởng câu đó được canh. Sửa dòng 21
thành "Twenty-six"; con số được một `awk` độc lập xác nhận, không phải bởi test.

Bốn đảo chiều thay vào đó, mỗi cái viết câu dự đoán trước. Manager tự chạy lại cái thứ hai:

```
docs/CONFIGURATION.md §1: Key has no doc row: `TimestampPrecision`
test result: FAILED. 2 passed; 1 failed
```

Hai đảo chiều ngoài kịch bản của plan (R3, R4) là lý do hai chốt chống-câm tồn tại: phá dòng đánh
dấu `Key::name` làm cả ba test đỏ ở câu *"is now reading nothing"* thay vì lướt qua một lần quét
rỗng và coi như xanh; đổi tên heading `## 1.` cũng cho kết quả tương tự ở phía tài liệu.

Hai giới hạn nói ra trước, chứ không để bị phát hiện sau: gate đọc dòng bắt đầu bằng `|`, nó
**không parse markdown** — xoá dòng `|---|---|` của một bảng vẫn để gate xanh, tìm ra vì gõ sai
một đảo chiều; và chú thích `[changed 2026-09-05, was eleven]` ở dòng 21 tự nó đã cũ — để nguyên,
không thuộc phạm vi bước này.

**Bất biến 7 áp dụng cho cả code `#[cfg(test)]`, và điều đó được kiểm bằng đảo chiều chứ không
mặc định.** Không có `clippy.toml`, nên `unwrap()` trong module mới là lỗi biên dịch
(`error: used \`unwrap()\` on a \`Result\` value`); module dùng `unwrap_or_default`,
`strip_prefix`, `split_once` và `assert!` thay vào đó.

Cổng: `cargo test --all` 626 qua, 0 hỏng; `--no-default-features` 0 hỏng; `cargo clippy
--all-targets -- -D warnings` sạch; `cargo fmt --check` sạch; hai script của bước 1-2 vẫn `ok`;
`check-links.py` 1865 link, không link chết. CI xanh trên commit cha `41bdd99`, run
`34682470085`, 14/14 job.

**Đỏ có chủ đích, chưa đóng plan**: bốn key mới của bước 5 chưa có hàng tài liệu —

```
docs/CONFIGURATION.md §1: Key has no doc row: `SocketUseSSL`
test result: FAILED. 2 passed; 1 failed
```

Bước 6 và bước 7 đóng cả hai — plan TLS tự nói 4c-1 không merge nếu thiếu 4c-2.

### Bước 5 (4c-1) — bốn key TLS vào `Settings`, xong 2026-09-12, commit `b39b52e`

TLS đã tới cửa trước từ 4a và từ chối triển khai đòi kernel từ 4b, nhưng chưa file cấu hình nào
bật được gì cả. Bốn key, đặt tên theo luật `docs/CONFIGURATION.md:69-77` tự viết — tên của
QuickFIX C++ ở chỗ C++ có tên: `SocketUseSSL` (C++ không có key bật/tắt, đây là tên QuickFIX/J),
`ServerCertificateFile`, `ServerCertificateKeyFile`, `TlsRequireKernel` (không kỹ sư nào khảo sát
có key này; tiền lệ là `ReconnectCeiling`). `TlsCaFile` **không có**: `tls.rs:561` gọi
`with_no_client_auth()`, và một key không ai đọc là một lời hứa trong tài liệu.

`Settings::tls() -> Option<&TlsSettings>` mượn; `into_tls_table()` trả về bản sở hữu, vì bên gọi
giữ chứng chỉ là bên đang dựng listener. `TlsBlock` tách khỏi `Block` có chủ đích: `Block` gộp
theo từng `[SESSION]`, còn bốn key này không bao giờ gộp như vậy — nên *"một key TLS trong
`[SESSION]` là lỗi"* là tính chất của kiểu dữ liệu, không phải luật phải nhớ.

**Bất biến 6 là nửa quan trọng, và nó là *từ chối sạch sẽ*, không phải *đọc rồi lơ đi*.** Build
không có feature `tls` từ chối `SocketUseSSL=Y` ngay lúc parse (`Problem::NeedsFeature`). Đúng
một `#[cfg(not(feature = "tls"))]` trong cả crate, ở `settings.rs:707`, đặt trên **một câu lệnh**
chứ không phải một khối — dạng khối làm phần còn lại của hàm thành `unreachable_code` và `-D
warnings` đỏ. Nó nằm **cuối cùng** trong `settle` có chủ đích, để năm trong bảy test mới chạy
được ở cả hai bộ feature, không chỉ bộ có TLS.

`into_table()` từ chối file mang `SocketUseSSL=Y` bằng `Problem::NeedsTlsDoor` — cái sai đắt chỉ
có một chiều: file xin TLS rót vào `serve` thường, phục vụ plaintext mà không ai thấy.

**Đảo chiều, và lần thử đầu của manager là một no-op đáng ghi lại.** Thay `#[cfg(not(feature =
"tls"))]` bằng một dòng comment khiến câu lệnh được canh vẫn biên dịch **vô điều kiện** — build
không feature vẫn từ chối đúng, test vẫn xanh. **Đó là một đảo chiều không đổi gì cả, và nếu đọc
theo đúng nghĩa đen sẽ tưởng nó vừa "chứng minh" cái guard.** Xoá hẳn cả khối từ chối mới là đảo
chiều có ý nghĩa:

```
thread 'socket_use_ssl_is_refused_without_the_tls_feature' panicked at crates/engine/tests/settings.rs:1065:5:
assertion `left == right` failed: a build without the `tls` feature must refuse SocketUseSSL=Y at parse time rather than serve the port in plaintext; instead: Ok(true)
  left: None
 right: Some(NeedsFeature)
test result: FAILED. 38 passed; 1 failed
```

Người viết test lại vấp đúng lớp đó theo hướng khác: cách viết test tự nhiên nhất làm đảo chiều
đỏ **bên trong** một hàm phụ, ở câu *"this should not have parsed"* — đúng
`a-red-reversal-does-not-prove-the-assertion-you-wrote-it-for` một lần nữa. Assertion bây giờ so
sánh một `Option<&Problem>` tồn tại bất kể file có parse được hay không.

Ba bộ lệnh, không phải hai như brief tưởng: mặc định, `--no-default-features` và `--features
tls` — vì `default = ["standard"]`, không có `tls`, nên hai bộ đầu là một.

```
default                  test result: ok. 39 passed; 0 failed
--no-default-features    test result: ok. 39 passed; 0 failed
--features tls           test result: ok. 41 passed; 0 failed
```

`cargo clippy --all-targets -- -D warnings` sạch cả hai; `check-indexing-debt.sh` 181/181,
không đổi.

**ĐỎ CÓ CHỦ ĐÍCH, nhánh này không được merge ở trạng thái này.** Gate hai chiều của bước 4 đỏ vì
bốn key mới chưa có hàng tài liệu:

```
docs/CONFIGURATION.md §1: Key has no doc row: `SocketUseSSL`
test result: FAILED. 2 passed; 1 failed
```

Hai nhân chứng còn lại của gate vẫn xanh, nên bản quét còn nguyên, chỉ thiếu hàng.
`scripts/check-no-optional-deps.sh` thoát 1 đúng một test đó, không gì khác. Bước 6 (`load_pem`
và test wire) và bước 7 (tài liệu) đóng cả hai.

### Bước 6 (4c-2) — `load_pem` và một `.cfg` thật đưa phiên TLS lên, xong 2026-09-12, commit `0a49778`

Bước 5 thêm bốn key và không ai đọc chúng; plan TLS tự nói 4c-1 không merge nếu thiếu bước này.
Nó cũng đóng một khoảng trống hình dạng: `TlsSettings` giữ đường dẫn, mọi `serve_tls*` nhận DER,
và chưa gì trong engine đọc PEM.

`tls::load_pem(&TlsSettings)` nằm sau đúng cổng `#[cfg(all(feature = "tls", target_os =
"linux"))]` như `server_config`, dùng `pem_file_iter`/`from_pem_file` của `rustls-pki-types`
1.15.1 — đã có sẵn trong cây qua feature `std` của `rustls`, không thêm dependency,
`Cargo.toml` không đổi. Bốn lỗi thao tác viên khác nhau được bốn câu khác nhau, mỗi câu nêu tên
key cấu hình và đường dẫn: file không mở được, file không có mục `CERTIFICATE`, hai câu tương tự
cho key. `ServeError::Tls` mang chúng; không `unwrap`, `expect`, `panic!` hay indexing —
`check-indexing-debt` không đổi, 181/181.

`crates/engine/tests/tls_settings_wire.rs` chạy toàn bộ đường đi trên một socket thật: `rcgen`
ghi PEM ra thư mục tạm, một `.cfg` thật đặt tên hai đường dẫn đó cộng `SocketUseSSL=Y`, rồi
`Settings::parse` → `into_tls_table` → `load_pem` → `serve_tls_requiring` → một client `rustls`
bình thường bắt tay và logon.

**Đảo chiều là trọng tâm của bước, và manager đã tự chạy lại.** Hoán đổi **nội dung byte** của
hai file PEM:

```
load_pem refused the PEM written by rcgen: building the TLS server configuration: ServerCertificateFile /tmp/fixbolt-tls-settings-wire-up-88891/server.crt holds no CERTIFICATE section
test result: FAILED. 2 passed; 1 failed; finished in 0.00s
```

Đỏ ở `load_pem`, trước khi listener bind, đúng như dự đoán. `0.00s` tự nó là bằng chứng — lần
chạy xanh mất 2.19s vì nó mở socket, còn lần này chưa tới đó.

Hoán đổi **đường dẫn** thay vì nội dung — đúng chữ plan yêu cầu — **không** tới được `load_pem`:
một assertion trước đó bắt được trước. Cùng lớp với
`a-matcher-excluded-the-separator-every-real-name-uses`: đảo chiều chạy, đỏ, nhưng chứng minh một
thứ khác với thứ nó được viết ra để chứng minh. Ghi lại trong doc của chính module test, không
phải một file `docs/reference/` mới. Một đảo chiều thứ hai — xoá chứng chỉ sau khi `.cfg` đã đặt
tên nó — rơi đúng chỗ đó với câu khác, và đó là cái xác nhận bốn câu thông báo thật sự khác nhau
nhau: soạn nó bắt được một tiền tố lặp (`could not be opened: I/O error: …`) mà không test đứng
sẵn nào thấy được, vì chúng chỉ kiểm tên key và đường dẫn, không kiểm câu chữ.

`cargo test -p fixbolt-engine --features tls --no-fail-fast`, chạy **ba lần**, cả 24 target xanh
mỗi lần, chỉ còn đỏ tài liệu đã biết. Lặp lại có lý do: chính cái flake mà PR #61 đóng sáng nay
được tìm ra nhờ chạy binary TLS ba lần, một lần xanh không nói lên điều gì.

**CI SẼ KHÔNG CHẠY FILE NÀY nếu không sửa.** Job `tls` của `.github/workflows/ci.yml` nêu tên
binary test tường minh — `--test tls --test tls_wire --test tls_mode` — và file mới không nằm
trong đó, nên job sẽ xanh mà chưa từng biên dịch nó. Đúng hình dạng của `STATUS.md` item 62 một
lớp sâu hơn, và đúng thứ mà con số *"TLS tests that ran"* của job đó tồn tại để bắt. Đã thêm
`--test tls_settings_wire`, kèm comment rằng một file test `--features tls` mới phải được thêm ở
đó trong cùng commit.

Trần indexing của `CLAUDE.md` §2 đọc **184** trong khi script đã đọc 181 từ khi item 60 lên
`main` 2026-09-09. Luật không đổi — trần chỉ được giảm — chỉ con số cũ, và nay ghi cả ngày nó
dịch chuyển lẫn ngày nó được sửa. Một luật, một chỗ.

**ĐỎ CÓ CHỦ ĐÍCH, không đổi từ commit trước, vẫn là đỏ duy nhất:**

```
docs/CONFIGURATION.md §1: Key has no doc row: `SocketUseSSL`
```

`scripts/check-no-optional-deps.sh` thoát 1 đúng một test đó. `cargo clippy --all-targets
--features tls -- -D warnings` sạch, `cargo fmt --check` sạch, `check-links.py` 1867 link không
chết, hai script của bước 1-2 vẫn `ok`. Bước 7 đóng tài liệu.

### Bước 7 — tài liệu 4c, xong 2026-09-12 (commit đóng bước này)

Viết bởi developer (sonnet), theo đúng brief bước 7: bốn hàng `docs/CONFIGURATION.md` §1 (đọc
hành vi thật từ `settings.rs` và bảy test của `crates/engine/tests/settings.rs`, không đoán),
một hàng `tls` ở §4 (thiếu từ bước 1 của plan TLS), mục TLS của `GUIDE.md` (`:1473`) thêm điều
người dùng phải biết mà trình biên dịch không giữ được, `DESIGN.md` D11 đoạn *"Still not built"*
sửa cho đúng sau 4b **và** 4c — bao gồm sửa một câu đã sai từ trước bước này: câu hỏi mở 3 của
ADR-0005 (*cái gì khẳng định mode nào đang chạy*) đã được trả lời từ bước 4b (`e728c16`, sáng nay)
nhưng D11 vẫn còn ghi *"TlsMode exists and nothing reads it"* — sửa luôn, không thuộc phạm vi
brief nhưng là tài liệu tôi được giao sửa. `CHANGELOG.md` thêm API mới (`Settings::tls`,
`into_tls_table`, `TlsSettings`, `tls::load_pem`, hai `Problem` mới), nhật ký giao hàng của cả
hai plan.

**Không đụng `crates/engine/src/settings.rs` hay bất cứ gì dưới `crates/`** — gate đỏ được canh
bởi chính test đó, và nó là thứ đang phán xét bước này, đúng như brief nói.

Cổng, trích nguyên văn:

```
cargo test -p fixbolt-engine --lib settings
test settings::doc_table::every_key_name_parses_back_to_its_key ... ok
test settings::doc_table::configuration_md_section_1_lists_exactly_the_keys ... ok
test settings::doc_table::the_key_scrape_is_not_silently_short ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out
```

`cargo test --all` và `cargo test --all --no-default-features`: mọi binary `0 failed` (thoát đọc
từ `${PIPESTATUS[0]}`, không qua `tail`). `bash scripts/check-no-optional-deps.sh`: `ok` cả chín
kiểm. `python3 scripts/check-links.py`: `1882 internal links checked … no dead internal links` —
tăng từ 1867, mọi liên kết mới (`ADR-0060`, `ADR-0005`, `tls.rs`, `settings.rs`) đều trỏ đúng chỗ.
`bash scripts/check-no-crate-root-allow.sh` và `bash scripts/check-scratch-fixtures.sh`: vẫn
`ok`, không đổi vì bước này không chạm code. `git status --porcelain`: đúng năm file được giao
chạm, cộng nhật ký giao hàng của hai plan.

**Không tạo file `docs/reference/` mới**, dù bước 5 để lại một phát hiện chưa có nơi ghi (đảo
chiều no-op của manager, xem *Bước 5* ở trên) — việc đó nằm ngoài phạm vi bước 7 (chỉ docs được
liệt kê trong brief), nên chỉ nêu tên trong nhật ký này và trong `STATUS.md`, không tự ý thêm
file.

### Việc dở dang sau bước 7

- ~~**Chưa có id run CI**~~ — **đã có, 2026-09-12.** Commit đóng **`a8a5824`** xanh, run
  [`34688952076`](https://github.com/tmthang86/fixbolt/actions/runs/34688952076), **14 job / 14**;
  `41bdd99` giữa nhánh cũng xanh, run
  [`34682470085`](https://github.com/tmthang86/fixbolt/actions/runs/34682470085), 14 / 14.
  **Dòng đáng đọc không phải dấu tick mà là con số**: job `tls` in `TLS tests that ran: 14`, trước
  nhánh này là **11** — ba cái chênh đúng là `tls_settings_wire.rs`, nên file đó thật sự được
  compile và chạy trên runner chứ không chỉ ở bàn này. Nếu thiếu một dòng sửa `ci.yml` ở `0a49778`
  thì con số vẫn đọc 11 bên cạnh một dấu tick xanh. Và `tls.rs repetitions failed: 0 of 3`, trên
  chính binary còn flaky ở runner này mười hai tiếng trước. **Commit merge thì chưa xanh — nó chưa
  tồn tại.**
- **`shellcheck` chưa chạy** trên bất kỳ script nào của repo này — ghi ở *Not proven* của
  `STATUS.md`.
- **Không có gì nói về luồng engine dưới TLS.** `scripts/check-no-kernel-sleep.sh` chưa có arm
  TLS, `DESIGN.md` §8 hàng TLS vẫn trống.
- **Ba đảo chiều gần-trượt của cả nhánh, một điểm chung**: bộ khớp không khớp `clippy::` (bước
  1, đã có file), đảo chiều no-op của manager ở bước 5 (chưa có file), hoán đường dẫn của bước 6
  không tới được `load_pem` (ghi trong doc của test, không phải file riêng). Hai trong ba đã có
  chỗ ghi bền; cái còn lại được nêu tên ở đây và ở `STATUS.md` để không mất.
- **Plan TLS bước 5, 6, 7 (đánh số riêng của plan đó) chưa bắt đầu**: initiator, `w2w --tls`, arm
  TLS của `check-no-kernel-sleep.sh`.
- ~~Việc còn lại của manager~~ — **xong 2026-09-12**: manager chạy lại toàn bộ cổng trên chính
  commit đóng, đặt tên run CI ở trên, viết
  [a-reversal-that-removed-the-guard-s-label-not-the-guard](../reference/a-reversal-that-removed-the-guard-s-label-not-the-guard.md)
  cho đảo chiều no-op của chính mình (cái thứ ba, bước 7 đã nêu tên mà không được phép tạo file),
  nối chéo cả ba mục, và **sửa một ô sai trong bảng mới**: ghi chú của `TlsRequireKernel` viết là
  *"does nothing without `SocketUseSSL=Y`"* trong khi `settings.rs:668-680` **từ chối** nó — cổng
  hai chiều kiểm *có hàng*, không kiểm *hàng đúng*. PR [#63](https://github.com/tmthang86/fixbolt/pull/63).

**Plan ĐÓNG 2026-09-12**, cả 7 bước. Chưa merge.
