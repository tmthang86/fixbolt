# Phase 3, bước 6–8: đóng gói, tài liệu cho người lạ, và bản `0.1.0` đầu tiên

> **Loại:** Plan · **Ngày:** 2026-09-23 · **Trạng thái:** Đã duyệt (manager, 2026-09-23, theo mandate thường trực của owner)
> **Phạm vi:** phase 3, hàng 6, 7 và 8 của *Chia việc* trong
> [2026-09-23-phase-3-scope.md](2026-09-23-phase-3-scope.md) — tiêu chí thoát 3, 7, 8 của
> [ADR-0097](../decisions/ADR-0097-phase-3-makes-the-engine-dependable-by-a-stranger-and-fixp-waits-on-a-running-oracle.md).
> Quyết định mới nằm ở
> [ADR-0160](../decisions/ADR-0160-six-crates-release-in-lockstep-and-the-packaged-sources-are-the-stranger-before-crates-io-is.md)
> (*Proposed*, duyệt cùng plan này).

## Bối cảnh

Hàng 1–5 của phase 3 đã hoặc sắp xong: từ điển build không cần `vendor/`, mật khẩu không xuống
đĩa, có `Decimal`, interop QuickFIX/J. Còn lại là **đưa được code lên crates.io để một người
lạ `cargo add fixbolt` và chạy được**.

Hôm nay cả tám crate đều `version = "0.0.0"` và `publish = false`, và `cargo package` từ chối
ngay vì các dependency nội bộ chỉ có `path`, không có `version`. Anh đã quyết (ADR-0097): bản
đầu là `0.1.0` (Q3); **anh tự chạy `cargo publish`** vì publish không xoá được, chỉ yank được
(Q5); publish sáu crate `fixbolt-codec`, `fixbolt-dict`, `fixbolt-session`, `fixbolt-engine`,
`fixbolt-sbe`, `fixbolt` — không publish `fixbolt-conformance`, `fixbolt-sbe-gen` (Q8).

Plan này làm mọi thứ **đến sát nút bấm**, dừng lại để anh bấm, rồi chuẩn bị sẵn việc kiểm tra
ngay hôm sau. Ý chính: trước khi publish, CI đã build **đúng các byte sẽ được upload** (các file
`.crate` giải nén), với mọi tổ hợp feature công khai, trên hai phiên bản Rust, và đã chạy một
Logon / Logout qua đúng đoạn code trong `docs/GETTING-STARTED.md`. Sau publish chỉ còn đổi
nguồn từ "bản đóng gói" sang "crates.io thật".

## Những gì đã biết chắc

Đo ngày 2026-09-23 trên máy desk (`tmt-B450-I-AORUS-PRO-WIFI`, cargo 1.98.0), trong một bản
chép của `main` `a6c6026` đặt ngoài repo, **không có `vendor/`**. Chi tiết và nguồn: ADR-0160
*Context* và *Research*.

1. **Lỗi đã biết:** `cargo package -p fixbolt-dict --no-verify` báo `all dependencies must have a
   version requirement specified when packaging. dependency 'fixbolt-codec' does not specify a
   version`.
2. **Chỉ cần sửa rất ít là đóng gói được cả sáu.** Thêm `version = "0.1.0"` ở
   `[workspace.package]`, `version.workspace = true` và bỏ `publish = false` ở sáu crate, thêm
   `version = "=0.1.0"` cạnh `path` cho mọi dependency nội bộ **thường** (dev-dependency để
   nguyên) → `cargo publish --workspace --dry-run` đóng gói và build kiểm cả sáu theo đúng thứ
   tự phụ thuộc, bỏ qua `fixbolt-conformance`, `fixbolt-sbe-gen` và `tools/*`, không một cảnh báo
   nào ngoài `aborting upload due to dry run`. Kích thước nén: codec 87,1 KiB, dict 253,5 KiB,
   session 188,6 KiB, engine 504,7 KiB, sbe 42,2 KiB, fixbolt 32,7 KiB (giới hạn 10 MB).
3. **Dev-dependency chỉ có `path` bị cargo cắt bỏ khi đóng gói** (Cargo book, *Multiple
   locations*). Nhờ vậy vòng phụ thuộc `codec` ↔ `dict`, `session` ↔ `engine` và
   `fixbolt-conformance` (không publish) không cản gì. Hệ quả phụ: feature `fix50sp2` của
   `fixbolt-codec` bị cargo viết lại thành `fix50sp2 = []` trong bản đóng gói — với người dùng nó
   không làm gì.
4. **Một crate ngoài repo** phụ thuộc `fixbolt` (feature `sbe`) và `fixbolt-engine` (feature
   `tls`, `affinity`, `fix50sp2`), trỏ `[patch.crates-io]` vào `target/package/<crate>-0.1.0/`
   (mã nguồn đã đóng gói mà dry run để lại) → build và chạy được trên Rust 1.98.0.
5. **`rust-version = "1.85"` mà mọi manifest đang khai là sai.** Cùng crate đó trên Rust 1.85.0:
   năm lỗi `error[E0658]: 'let' expressions in this position are unstable` trong
   `fixbolt-codec`, và build script của `fixbolt-dict` hỏng cùng kiểu. Trên 1.88.0: build được,
   mọi feature ở trên đều bật.
6. **crates.io chấp nhận `LicenseRef-`.** `slint` 1.18.1, publish 2026-09-21, có `license =
   "GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0"` (đọc từ
   API crates.io). Mã của crates.io dùng crate `spdx`, và bộ lexer của crate đó nhận
   `LicenseRef-…` **trước khi** xét `allow_unknown`. Chưa tìm thấy crate nào dùng `LicenseRef-`
   với `AND` và ngoặc như `fixbolt-dict`; test của crates.io có parse `MIT OR (Apache-2.0 AND
   MIT)`. Dry run không gửi lên server, nên bằng chứng cuối cùng vẫn là lần upload của anh.
7. **Sáu tên crate hôm nay còn trống**: `https://crates.io/api/v1/crates/<tên>` trả 404 cho cả sáu.
8. **`cargo publish --workspace` ổn định từ Rust 1.90**; bỏ qua crate `publish = false` (cargo
   PR #15525); **không nguyên tử** — nếu upload hỏng giữa chừng thì các crate đã lên vẫn nằm đó,
   chạy lại cho phần còn thiếu.
9. **Xoá crate**: trong 72 giờ đầu chủ crate tự xoá được, nhưng **chỉ khi không crate nào khác
   phụ thuộc vào nó** (RFC 3660) — sáu crate phụ thuộc nhau nên phải xoá ngược thứ tự. Yank
   (`cargo yank --version 0.1.0`) không xoá gì, chỉ chặn lockfile mới chọn bản đó.
10. **docs.rs** build bằng nightly, trên `x86_64-unknown-linux-gnu`, **không có mạng**, đặt
    `--cfg docsrs`. `doc_auto_cfg` đã bị gộp vào `doc_cfg` (10/2025): viết
    `#![cfg_attr(docsrs, feature(doc_auto_cfg))]` bây giờ làm hỏng build docs.rs.
11. **`cargo-semver-checks`** mới nhất 0.50.0 (2026-08-01, tự nó cần Rust 1.93). Chọn mốc so
    sánh bằng `--baseline-version` (crates.io), `--baseline-rev` (git) hoặc `--baseline-root`.
    Với `0.y.z`, Cargo coi đổi `y` là bản lớn (được phá API), đổi `z` là bản nhỏ.
12. **`docs/GUIDE.md`** (mục SBE) bảo người dùng thêm `fixbolt-sbe-gen` làm build-dependency —
    crate mà Q8 không publish.
13. **Đoạn code bước 3 trong `docs/GETTING-STARTED.md` khác file ví dụ nó nói là nguồn**: trang
    gọi `admin.shutdown(5_000)` ngay trong thread vừa tạo (engine dừng ngay), còn
    `crates/library/examples/acceptor.rs` chờ một dòng trên stdin. Trang còn bảo người đọc chạy
    `scripts/fetch-quickfix-assets.sh` trước — người dùng crate không cần việc đó.
14. **Crate tạm đặt ngoài repo không thấy `rust-toolchain.toml`** và dùng toolchain mặc định của
    máy — bẫy đã trả giá, có gate `scripts/check-scratch-fixtures.sh`
    ([a-scratch-fixture-inherits-the-machine](../reference/a-scratch-fixture-inherits-the-machine.md)).
15. **Cargo gộp feature trong một lần gọi**: một crate tạm bật mọi feature không chứng minh được
    từng feature riêng lẻ build được
    ([feature-flags-unify-across-a-workspace](../reference/feature-flags-unify-across-a-workspace.md)).

## Cách làm

Theo ADR-0160, tóm tắt:

- **Một phiên bản chung cho cả sáu crate**, khai một chỗ ở `[workspace.package] version`.
  Dependency nội bộ thường ghim đúng phiên bản (`version = "=0.1.0", path = "../codec"`), để
  người lạ không bao giờ trộn được hai bản mà CI chưa từng build chung. Dev-dependency nội bộ giữ
  chỉ `path`.
- **`fixbolt-conformance`, `fixbolt-sbe-gen`, `tools/*` giữ `publish = false`** (Q8 không đổi).
  `GUIDE.md` hướng dẫn lấy `fixbolt-sbe-gen` qua git, ghim tag `v0.1.0`.
- **`rust-version = "1.88"`**, và CI build bản đóng gói trên đúng 1.88.0.
- **Mỗi `.crate` chỉ chứa danh sách file được liệt kê** (`include`): mã nguồn, `build.rs`,
  `spec/` và `NOTICE` cho dict, `examples/` cho `fixbolt`, `README.md`, hai file giấy phép (bản
  chép trong từng crate, giữ giống hệt bản ở gốc repo). Không ship test và bench — chúng không
  build được khi dev-dependency đã bị cắt.
- **Metadata**: `description` (đã có), `license` (dict giữ biểu thức của ADR-0104),
  `repository`, `homepage` (= repository), `readme`, `keywords` (≤ 5), `categories` (chỉ các
  slug có thật: `finance`, `network-programming`, `parser-implementations`, `encoding`,
  `no-std` cho `codec` và `sbe`).
- **docs.rs** build theo danh sách feature đặt tên, không dùng `all-features`; mỗi crate thêm
  `#![cfg_attr(docsrs, feature(doc_cfg))]`.
- **Trước publish, "người lạ" là bản đóng gói**: CI dry run trên runner không `vendor/`, rồi build
  một crate ngoài repo trỏ `[patch.crates-io]` vào `target/package/…`, từng tổ hợp feature một,
  và chạy Logon / Logout qua code dán nguyên văn từ `GETTING-STARTED.md`. **Sau publish**: cùng
  script, bỏ patch, `cargo add fixbolt@0.1.0` từ crates.io thật.
- **`cargo-semver-checks` 0.50.0**: trước publish chạy không chặn, so với `origin/main`; sau
  publish so với crates.io và chặn merge.

File tạo mới: `scripts/check-release-versions.sh`, `scripts/check-package-contents.sh`,
`scripts/check-packaged-build.sh`, `scripts/stranger-check.sh`, `scripts/stranger-logon.py`,
`RELEASING.md`, `crates/{codec,dict,session,engine,sbe}/README.md`,
`crates/*/LICENSE-MIT`, `crates/*/LICENSE-APACHE` (sáu crate publish),
`docs/reference/publishing-a-workspace-to-crates-io.md`. File sửa: `Cargo.toml` gốc, sáu
`crates/*/Cargo.toml`, `Cargo.lock`, sáu `crates/*/src/lib.rs` (chỉ rustdoc và một dòng
`cfg_attr`), `.github/workflows/ci.yml`, `README.md`, `docs/GETTING-STARTED.md`, `docs/GUIDE.md`
(mục SBE), `CHANGELOG.md`, `crates/library/examples/acceptor.rs` (nếu cần cho khớp trang).

## Bất biến bị đụng tới

- **6 (feature gate chính `mod`; CI build `--no-default-features`)**: đổi manifest, không đổi
  `[features]` nào (trừ khi dry run bắt buộc). `scripts/check-no-optional-deps.sh` chạy lại ở
  6a. Job mới build **từng** tổ hợp feature trên bản đóng gói, mỗi tổ hợp một crate tạm riêng —
  vì cargo gộp feature trong một lần gọi.
- **7 (không `panic!`/`unwrap()`/`expect()` trong crate thư viện)**: 6a thêm một dòng
  `cfg_attr` ở gốc crate — không phải `allow`/`expect`, nên `check-no-crate-root-allow.sh` vẫn
  xanh; chạy lại để chắc. Script mới là bash/python, không phải crate thư viện.
- **9 (không copy source QuickFIX)**: không đổi. Danh sách `include` bảo đảm không file nào từ
  `vendor/` hay `tests/` lọt vào `.crate`; `check-package-contents.sh` kiểm điều đó.
- **1, 2, 3, 4, 5, 8, 10**: không đụng — không đổi dòng code chạy nào. 59 / 59 vẫn chạy lại ở
  6a vì `Cargo.lock` và phiên bản đổi.
- **§6 Dependencies**: không thêm dependency nào vào crate. `cargo-semver-checks` chỉ cài trong CI.

## Chia việc

Một pull request cho 6a–7a (và 8a), một bước dừng (7b), một pull request nhỏ sau publish (8b).
Không bước nào tự commit — manager chạy lại gate và commit. Thứ tự: **6a → 6b → 6c → 8a → 7a →
7b (dừng) → 8b**; 8a đi trước 7a vì script người lạ chính là bài test đỏ trước của trang
`GETTING-STARTED`. Cuối PR: một senior review (opus), đọc riêng `RELEASING.md` từng lệnh.

| Bước | Kết quả | Người làm | File được sửa / **không** được sửa | Gate — xong khi | Test đỏ trước / đảo ngược | Phụ thuộc |
|---|---|---|---|---|---|---|
| 6a | Metadata sáu crate; phiên bản `0.1.0` chung; `=0.1.0` cho dependency nội bộ thường; `include`; hai file giấy phép trong từng crate; `rust-version = "1.88"`; `[package.metadata.docs.rs]`; `cfg_attr(docsrs, feature(doc_cfg))`; script `check-release-versions.sh`; trang reference | **senior developer (opus)** — đụng manifest của cả `codec`, `session`, `engine`, bất biến 6 trải nhiều crate; trang reference do architect viết từ số đo sẵn có | Sửa: `Cargo.toml` gốc (`[workspace.package] version`, `rust-version`, `homepage`), sáu `crates/{codec,dict,session,engine,sbe,library}/Cargo.toml` (`[package]`, `[dependencies]`, `[package.metadata.docs.rs]`; **không** `[features]`, **không** `[dev-dependencies]`), `Cargo.lock`, sáu `crates/*/src/lib.rs` (**chỉ** thêm dòng `cfg_attr`), `crates/*/LICENSE-MIT`, `crates/*/LICENSE-APACHE` (chép), `scripts/check-release-versions.sh` (mới), `docs/reference/publishing-a-workspace-to-crates-io.md` (mới, architect). **Không**: `crates/conformance/`, `crates/sbe-gen/`, `tools/`, mọi `src/` ngoài dòng `cfg_attr`, `.github/`, `deny.toml` | Trên một `git worktree` **không có `vendor/`** (`test ! -e vendor`): `cargo publish --workspace --dry-run --allow-dirty` exit 0, liệt kê đúng sáu `Packaging`; `cargo package --list -p <crate>` cho từng crate chỉ gồm file trong `include`; `scripts/check-release-versions.sh` exit 0; `shellcheck -S info` sạch. Trên cây có `vendor/`: `cargo test --all`, `cargo test --no-default-features`, `cargo clippy --all-targets -- -D warnings`, `scripts/check-no-optional-deps.sh`, `scripts/check-no-crate-root-allow.sh`, `cargo deny check`, `scripts/check-every-crate-is-licensed.sh`, `cargo test -p fixbolt-session --test score` 59 / 59, `cargo test -p fixbolt-engine --test wire` 59 / 59. docs.rs mô phỏng: `RUSTDOCFLAGS="--cfg docsrs" cargo +nightly doc --no-deps -p fixbolt-engine --features standard,affinity,tls,fix50sp2` và tương tự cho năm crate kia, không lỗi | Chạy `check-release-versions.sh` **trước** khi sửa manifest → đỏ, câu viết trước: `FAIL fixbolt-dict: dependency fixbolt-codec has no version requirement`. Đảo ngược sau khi xanh: đổi một `=0.1.0` thành `0.1.0` → đỏ `… is not "=0.1.0"`; sửa một byte trong `crates/codec/LICENSE-MIT` → đỏ `LICENSE-MIT copies differ`; trả lại, xanh | plan duyệt |
| 6b | Job CI `package`: dry run không `vendor/`, kiểm nội dung `.crate`, build bản đóng gói theo từng tổ hợp feature trên 1.98.0 và 1.88.0 | developer (sonnet) | Sửa: `.github/workflows/ci.yml` (một job mới, runner riêng, như `dict-no-vendor`), `scripts/check-package-contents.sh` (mới), `scripts/check-packaged-build.sh` (mới). **Không**: `crates/`, mọi job khác | Trên PR, job xanh và manager ghi run id: `test ! -e vendor`; `cargo publish --workspace --dry-run` (checkout sạch, **không** `--allow-dirty`); `check-package-contents.sh` exit 0 — dict có `spec/FIX44.xml`, `spec/FIXT11.xml`, `spec/FIX50SP2.xml`, `NOTICE`; mọi crate có `README.md`, `LICENSE-MIT`, `LICENSE-APACHE`; không crate nào có `tests/`, `benches/`, `vendor`, `.def`; đúng sáu crate. `check-packaged-build.sh` build **mỗi tổ hợp một crate tạm** ở `target/packaged-build/`, chép `rust-toolchain.toml` vào: `fixbolt` mặc định; `fixbolt` `default-features = false`; `fixbolt` `sbe`; `fixbolt-engine` `default-features = false`; `fixbolt-engine` `tls`; `fixbolt-engine` `affinity`; `fixbolt-engine` `fix50sp2`; `fixbolt-sbe` `default-features = false`; `fixbolt-codec` một mình — rồi trên `+1.88.0`: tổ hợp gộp tất cả và `fixbolt` `default-features = false`. `scripts/check-scratch-fixtures.sh` xanh với hai script mới | Viết câu FAIL trước rồi mới chạy: xoá `NOTICE` khỏi `include` của dict → `FAIL fixbolt-dict: NOTICE missing from package`; thêm `tests/**` vào `include` của codec → `FAIL fixbolt-codec: tests/ shipped`; đặt tạm `rust-version = "1.85"` và build trên `+1.85.0` → đỏ `E0658` trong `fixbolt-codec` (đây chính là số đo 5). Trả lại, xanh | 6a |
| 6c | Job CI `semver`, **chưa chặn** | developer (sonnet) | Sửa: `.github/workflows/ci.yml` (job mới, `continue-on-error: true`, `fetch-depth: 0`, `cargo install --locked cargo-semver-checks@0.50.0`, `cargo semver-checks --workspace --baseline-rev origin/main`). **Không**: `crates/`, job khác | Job chạy trên PR; log có một dòng kiểm cho **mỗi** crate trong sáu, và **không** có `fixbolt-conformance`, `fixbolt-sbe-gen`, `tools/*` (nếu có: thêm `--exclude` từng tên, ghi vào trang reference); manager ghi run id. Lần chạy đầu có nghĩa là PR **sau** khi 6a vào `main` (trước đó `main` còn `publish = false`) — ghi rõ trong comment của job | Trên nhánh tạm (không merge): đổi tên một `pub fn` trong `fixbolt-codec` → log job gọi tên lint `function_missing` cho `fixbolt-codec`. Viết câu mong đợi trước. Xoá nhánh tạm | 6a |
| 8a | `scripts/stranger-check.sh` + `scripts/stranger-logon.py`, **chạy được ngay** ở chế độ `--from packaged`; bước chặn trong job `package` | developer (sonnet) | Sửa: `scripts/stranger-check.sh` (mới), `scripts/stranger-logon.py` (mới, chỉ thư viện chuẩn python — một bên đối tác không dùng dòng code nào của fixbolt), `.github/workflows/ci.yml` (một bước cuối job `package`). **Không**: `crates/`, `docs/` | Script: tạo crate ở `target/stranger/` (không phải `/tmp`), chép `rust-toolchain.toml`; `--from packaged` trỏ patch vào `target/package/…`, `--from registry --version X` dùng `cargo add fixbolt@X` không patch; lấy **nguyên văn** các khối code có dấu `<!-- stranger-check: main.rs -->` và `<!-- stranger-check: acceptor.cfg -->` trong `docs/GETTING-STARTED.md`; build; chạy binary; `stranger-logon.py` gửi Logon (`49=TW44`, `56=ISLD`, `98=0`, `108=30`), chờ `35=A`, gửi Logout, chờ `35=5`, in `LOGON OK` / `LOGOUT OK`; ghi một dòng vào stdin của acceptor, chờ nó thoát 0 trong 10 giây và in `stopped:`. `shellcheck -S info` sạch; `scripts/check-scratch-fixtures.sh` xanh | **Đỏ trước trên trang hiện tại** (đây là test đỏ của 7a): chạy `--from packaged` → đỏ, câu viết trước: `FAIL: no block marked stranger-check: main.rs in docs/GETTING-STARTED.md`. Đảo ngược của client: gửi `56=WRONG` → acceptor đóng socket không trả lời → `FAIL: no Logon answer within 5 s` | 6b |
| 7a | Tài liệu cho người lạ | developer (sonnet) | Sửa: `docs/GETTING-STARTED.md` (cài bằng `cargo add fixbolt@0.1.0`, một dòng báo "chưa lên crates.io — tạm dùng `git = …`" mà 8b sẽ xoá; bỏ bước fetch `vendor/`; bước 2 + bước 3 ghép lại thành **một** `main.rs` biên dịch được, có dấu `stranger-check`, dừng bằng một dòng stdin như file ví dụ, đọc file cfg từ đối số); `crates/library/examples/acceptor.rs` và `examples/shared/order_handler.rs` **chỉ nếu** cần để trang và ví dụ giống nhau; rustdoc đầu `lib.rs` của sáu crate (một đoạn "crate này là gì", "phần lớn người dùng chỉ cần `fixbolt`", bảng feature, dòng giấy phép — dict nhắc `NOTICE`); `crates/{codec,dict,session,engine,sbe}/README.md` (mới, ngắn, cùng nội dung đó); `README.md` (mục *Getting started* thành cài đặt + mức hỗ trợ của dự án một người); `docs/GUIDE.md` (mục SBE: `fixbolt-sbe-gen` qua `git`, `tag = "v0.1.0"`); `CHANGELOG.md` (các mục *Unreleased* chuyển sang `## [0.1.0] — ngày ghi khi publish`, thêm mục *Điều kiện lên 1.0*: một triển khai mà repo này không viết, được báo công khai, và một bản minor không miễn trừ semver-checks — ADR-0097 quyết định 3); `RELEASING.md` (mới, gốc repo — xem *Trình tự phát hành* dưới). **Không**: mọi `src/` ngoài rustdoc, `Cargo.toml`, `STATUS.md`, `docs/TUTORIAL.md` (chỉ sửa link nếu gãy) | `scripts/stranger-check.sh --from packaged` xanh (trích `LOGON OK`, `LOGOUT OK`, `stopped:`); `cargo test --doc -p fixbolt`; `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p <crate>` cho sáu crate; `cargo test -p fixbolt --test end_to_end`; `python3 scripts/check-links.py`; `scripts/check-adr-numbers.sh`; manager đọc lại: không câu nào dùng "QuickFIX" để quảng bá (ADR-0104 quyết định 7) | Đỏ trước = đỏ của 8a trên trang cũ. Đảo ngược: xoá dòng stdin khỏi khối `main.rs` trên trang (quay về `shutdown` ngay) → `FAIL: no Logon answer within 5 s` hoặc `acceptor exited before Logon`; trả lại, xanh | 8a |
| 7b | **DỪNG. Anh chạy `cargo publish`.** Manager không publish, không `cargo login`, không đụng token | **anh** | Không file nào. Manager chỉ báo: commit phát hành trên `main`, CI run id xanh của đúng commit đó, tiêu chí 1–6 của ADR-0097 xanh, job `package` xanh | Anh làm theo `RELEASING.md` đến hết; báo lại bằng output của `cargo publish` | — | 6a–7a vào `main`; tiêu chí 1–6 xanh |
| 8b | Sau publish: người lạ thật, semver-checks chặn | developer (sonnet); manager chạy script trên desk | Sửa: `.github/workflows/ci.yml` (job `semver` bỏ `continue-on-error`, bỏ `--baseline-rev`, dùng mốc crates.io — `--baseline-version 0.1.0` như ADR-0097 tiêu chí 8; thêm job `stranger-registry` chạy `stranger-check.sh --from registry --version 0.1.0` trên `push` vào `main`), `docs/GETTING-STARTED.md` (xoá dòng "chưa lên crates.io"), `CHANGELOG.md` (ngày), `docs/CONFORMANCE.md` (tiêu chí 7 kèm lệnh, máy, run id). **Không**: `crates/` | `scripts/stranger-check.sh --from registry --version 0.1.0` exit 0 trên desk **và** trong CI, trích `LOGON OK`, `LOGOUT OK`, và dòng `Downloaded fixbolt v0.1.0` (chứng tỏ lấy từ crates.io, không từ path); job `semver` xanh, không `continue-on-error`; manager ghi run id | Chạy `--from registry --version 0.1.0` **trước** khi anh publish → đỏ `failed to select a version for the requirement fixbolt = "^0.1.0"` (hoặc `could not find fixbolt in registry`) — câu đó là bằng chứng script thật sự hỏi crates.io. Đảo ngược semver: nhánh tạm đổi tên một `pub fn` → job đỏ, chặn merge | 7b |

## Trình tự phát hành (nội dung `RELEASING.md`, anh chạy)

`RELEASING.md` viết bằng tiếng Anh (tài liệu mô tả hệ thống, `CLAUDE.md` §6). Tóm tắt các lệnh:

1. **Trên `main`, cây sạch**, ở đúng commit mà CI xanh (ghi run id). `git status --short` rỗng;
   `test ! -e vendor` không bắt buộc nhưng nên làm trên một clone mới.
2. **Token**: tạo token crates.io giới hạn quyền `publish-new` + `publish-update`, phạm vi
   `fixbolt*`, hết hạn sau vài ngày; `cargo login` trong terminal của anh. Token không bao giờ
   dán vào hội thoại.
3. **`cargo publish --workspace --dry-run`** (không `--allow-dirty`) → đọc đủ sáu dòng
   `Packaging` / `Verifying`, không cảnh báo mới.
4. **`cargo publish --workspace`** — cargo tự đi theo thứ tự codec → dict, sbe → session →
   engine → fixbolt, và chờ index trước khi upload crate phụ thuộc.
5. **Sau mỗi crate**: trang `https://crates.io/crates/<tên>/0.1.0` hiện đúng license (dict:
   `(MIT OR Apache-2.0) AND LicenseRef-QuickFIX-1.0`); vài phút sau `https://docs.rs/<tên>/0.1.0`
   build xanh.
6. **Nếu hỏng giữa chừng** (không nguyên tử): xem crate nào đã lên (`cargo info <tên>@0.1.0`),
   sửa nguyên nhân, `cargo publish -p <tên còn thiếu> …`. Nếu server **từ chối license** của
   dict: đổi dict sang `license-file = "NOTICE"` (phương án dự phòng của ADR-0104), commit, publish
   lại dict và các crate phía trên — vẫn là `0.1.0` vì chúng chưa từng lên.
7. **Gắn tag**: `git tag -a v0.1.0 -m 'fixbolt 0.1.0'`, `git push origin v0.1.0`; tạo GitHub release
   từ mục `0.1.0` của `CHANGELOG.md`.
8. **Nếu bản đã lên có lỗi**: `cargo yank --version 0.1.0 <tên>` cho từng crate, bắt đầu từ
   `fixbolt` đi ngược xuống `fixbolt-codec`; `--undo` để gỡ yank. Xoá hẳn chỉ trong 72 giờ đầu,
   qua trang *Settings* của crate, **theo thứ tự ngược phụ thuộc** (fixbolt trước, codec cuối),
   vì crates.io không cho xoá crate đang có crate khác phụ thuộc. Lộ bí mật thì yank không cứu
   được — đổi bí mật ngay.
9. Báo manager: manager chạy 8b.

## Cách kiểm chứng

| Tiêu chí ADR-0097 | Lệnh | Khi nào |
|---|---|---|
| 3 — đóng gói như khi publish | job `package`: `test ! -e vendor`, `cargo publish --workspace --dry-run`, `check-package-contents.sh`, `check-packaged-build.sh` | mỗi PR từ 6b |
| 7 — người lạ dùng được | `scripts/stranger-check.sh --from packaged` (trước publish); `--from registry --version 0.1.0` (sau publish) | 8a trở đi; 8b |
| 8 — API được canh | job `semver`, không chặn → chặn | 6c → 8b |
| MSRV khai báo là thật | `check-packaged-build.sh` trên `+1.88.0` | mỗi PR từ 6b |
| Phase 1, 2 vẫn giữ | `cargo test --all`, 59 / 59 hai đường, FIXT theo `CONFORMANCE.md` §9, alloc 0 | cuối PR 6a–7a |

"Test pass" một mình chưa đủ: tiêu chí 7 là một binary thật, build từ byte đóng gói (hoặc từ
crates.io), nói chuyện qua socket thật với một client không dùng code của fixbolt.

## Tài liệu phải cập nhật

Theo bảng `CLAUDE.md` §4:

- [ ] `README.md` — cài đặt, mức hỗ trợ (7a)
- [ ] `docs/GETTING-STARTED.md` — `cargo add`, code dán được (7a); xoá dòng "chưa publish" (8b)
- [ ] `docs/GUIDE.md` — `fixbolt-sbe-gen` qua git (7a); ràng buộc người dùng phải biết
- [ ] `CHANGELOG.md` — mục `0.1.0`, điều kiện `1.0` (7a); ngày (8b)
- [ ] rustdoc sáu crate (7a)
- [ ] `docs/reference/publishing-a-workspace-to-crates-io.md` — ba bẫy đã đo: dependency path
      không version, dev-dependency bị cắt kéo theo feature rỗng, `rust-version` khai sai (6a)
- [ ] `docs/CONFORMANCE.md` — tiêu chí 7 với lệnh, máy, run id (8b)
- [ ] `docs/DESIGN.md` §3 — cột/ghi chú "publish hay không" cho từng crate (6a, một câu)
- [ ] ADR-0160 — *Proposed* → *Accepted* khi plan được duyệt
- [ ] `STATUS.md` — manager, khi từng bước đóng (plan này không sửa)

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Dependency nội bộ chỉ có `path` → `cargo package` từ chối | `check-release-versions.sh`, đỏ trước ở 6a |
| Hai crate fixbolt khác phiên bản trộn nhau ở máy người lạ | `=0.1.0` + `check-release-versions.sh` |
| Build xanh vì runner có `vendor/` | `test ! -e vendor` đầu job `package` |
| `--allow-dirty` giấu file chưa commit / đưa file rác vào `.crate` | CI và anh chạy **không** `--allow-dirty`; `include` là danh sách cho phép; `check-package-contents.sh` |
| Một crate tạm bật mọi feature che mất một feature hỏng khi đứng riêng | mỗi tổ hợp một crate tạm (6b) |
| Crate tạm dùng toolchain của máy thay vì bản ghim | chép `rust-toolchain.toml`; `check-scratch-fixtures.sh` |
| `rust-version` khai mà không ai build | build trên `+1.88.0`; đảo ngược về 1.85 ra `E0658` |
| File giấy phép trong crate lệch bản gốc | `check-release-versions.sh` so byte |
| `doc_auto_cfg` làm hỏng build docs.rs | dùng `doc_cfg`; mô phỏng `--cfg docsrs` trên nightly ở 6a |
| docs.rs không có mạng | `build.rs` của dict chỉ đọc `spec/` (ADR-0104); mô phỏng docs.rs ở 6a |
| Trang `GETTING-STARTED` lệch file ví dụ (đã lệch — số đo 13) | `stranger-check.sh` biên dịch **chính trang đó** |
| `cargo-semver-checks` không đọc được rustdoc JSON của 1.98 | lần chạy đầu của 6c; nếu hỏng, ghim bản semver-checks mới hơn, ghi vào trang reference |
| Publish không nguyên tử, hỏng giữa chừng | `RELEASING.md` bước 6 |
| Server từ chối `LicenseRef` dạng `AND` | `RELEASING.md` bước 6, phương án `license-file` của ADR-0104 |
| Token lộ trong hội thoại / transcript | chỉ anh `cargo login`; `RELEASING.md` bước 2 |
| Script người lạ "xanh" mà thật ra lấy từ path | 8b đỏ trước khi publish; trích dòng `Downloaded fixbolt v0.1.0` |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Có người chiếm tên `fixbolt*` trên crates.io trước khi anh publish | Thấp | chỉ publish mới giữ được tên; đừng để khoảng giữa 7a và 7b kéo dài |
| crates.io từ chối biểu thức license của dict | Thấp (slint được nhận 2026-09-21) | `license-file = "NOTICE"`, publish lại dict và phía trên |
| Lockstep: sửa một dòng codec phải publish lại cả sáu | Trung bình | chấp nhận (ADR-0160 *Consequences*); `RELEASING.md` chỉ có một đường |
| semver-checks trước publish báo nhiều lỗi "phá API" so với `main` — gây nhiễu | Trung bình | không chặn đến 8b; đọc như thông tin |
| Người dùng SBE khó chịu vì `sbe-gen` phải lấy qua git | Thấp | publish `sbe-gen` sau là việc thêm, không phá semver (ADR-0160 quyết định 2) |
| Người lạ chạy test từ `.crate` (kiểu Debian) và không được | Thấp | nói rõ trong `README.md`: test chạy từ tag git |

## Ngoài phạm vi

- **Publish tự động từ CI** (trusted publishing, `release-plz`, `cargo-release`) — Q5: anh bấm.
- **Publish `fixbolt-sbe-gen`, `fixbolt-conformance`, `tools/*`** — Q8.
- **`1.0`** — ADR-0097 quyết định 3.
- **Build docs.rs cho target khác Linux x86_64**; bảo đảm Windows.
- **Hạ MSRV xuống dưới 1.88** bằng cách viết lại `let` chain.
- **Chạy test từ trong `.crate`**.

## Điểm manager cần duyệt

1. **ADR-0160** (*Proposed*): một phiên bản chung, ghim `=`; `rust-version` 1.85 → **1.88**
   (bản khai hiện tại sai, đã đo); không ship test/bench; Q8 giữ nguyên, `sbe-gen` qua git.
2. **Thứ tự 8a trước 7a**: script người lạ được xây trước để làm bài test đỏ cho trang
   `GETTING-STARTED`.
3. **6a giao senior developer (opus)**, các bước khác sonnet.
4. **Điểm dừng 7b**: sau khi 6a–7a (và 8a) vào `main` với CI xanh, manager dừng, báo anh, và
   **không** tự publish.

## Sửa 1 — 2026-09-23

Manager duyệt theo uỷ quyền thường trực, trong lúc làm 6a.

1. **Nâng `rust-version` lên 1.88 bật thêm ba lint của clippy.** Clippy coi `rust-version` là
   phiên bản Rust thấp nhất phải hỗ trợ, và giấu những lint mà cách sửa cần compiler mới hơn.
   Đo trên desk, clippy 0.1.98, chỉ đổi đúng một dòng: `rust-version = "1.85"` → 0 cảnh báo;
   `"1.88"` → **15 cảnh báo ở 9 file** (`collapsible_if`, `manual_is_multiple_of`,
   `chunks_exact_to_as_chunks`), nằm cả trong `crates/session/src`, `crates/engine/src`,
   `crates/conformance/src` — những file bảng *Chia việc* cấm 6a đụng. Nên thêm **bước 6a'**
   (senior developer): chạy `cargo clippy --fix` áp đúng 15 gợi ý của clippy (let chain,
   `is_multiple_of`, `as_chunks`), không đổi hành vi; rồi chạy lại các gate canh những crate
   đó: clippy `-D warnings` ba kiểu (mặc định, `--all-features`, `--no-default-features`),
   59 / 59 qua `score` và `wire` (cả hai mode), bộ FIXT (`score_fixt`, `wire_fixt` với
   `--features fix50sp2`), bench `alloc` của session và engine như `scripts/bench.sh` chạy (tất
   cả 0), `cargo test --all`, `--no-default-features`, `check-indexing-debt.sh` (không tăng).
2. **README của từng crate chuyển từ 7a sang 6a'.** Gate của 6b (`check-package-contents.sh`)
   đòi mỗi crate có `README.md`, mà 6b chạy trước 7a. Nên 6a' viết một README ngắn, đúng sự
   thật cho năm crate còn thiếu (crate là gì, lệnh cài một dòng, link về README gốc và
   `GETTING-STARTED`, dòng giấy phép — dict nhắc `NOTICE`), thêm mục cài đặt và giấy phép vào
   README của `fixbolt`, và đặt `readme = "README.md"` cho cả sáu. 7a chỉ còn trau chuốt nội
   dung. `scripts/check-links.py` mở rộng miễn trừ "phải dùng đường dẫn tương đối" — vốn chỉ
   dành cho `crates/library/README.md` — sang cả sáu README được publish, cùng lý do (link
   tương đối vào `docs/` gãy trên crates.io); kiểm "file phải tồn tại" vẫn giữ, đã đảo ngược.
3. **Ba chỗ lệch so với *Cách làm*, đã được chấp nhận:**
   - **Không có `homepage`.** Cargo nightly 1.100 cảnh báo ở từng crate: `package.homepage is
     redundant with package.repository`.
   - **`readme`**: lúc đầu bỏ khoá này vì README chưa có (cargo từ chối `readme` trỏ vào file
     không tồn tại); nay đã có README nên khoá được đặt lại theo mục 2.
   - **Sửa một comment** trong `[dev-dependencies]` của `crates/engine/Cargo.toml`, vì nó vẫn
     ghi phiên bản khai báo là 1.85. Chỉ sửa comment, không đổi dependency nào.

## Sửa 2 — 2026-09-24: không publish, gắn tag `v0.1.0`

**Vì sao.** Ngày 2026-09-24 anh quyết định **"Không publish"**: anh chỉ dùng fixbolt cho dự án
của mình, nên `0.1.0` **không** lên crates.io. Bản phát hành là tag git `v0.1.0`; người dùng lấy
fixbolt qua git: `fixbolt = { git = "https://github.com/tmthang86/fixbolt", tag = "v0.1.0" }`.
Quyết định được ghi ở
[ADR-0161](../decisions/ADR-0161-0-1-0-is-a-git-tag-not-a-crates-io-upload-and-the-stranger-and-the-semver-gate-read-the-tag.md)
(*Accepted* 2026-09-24, manager chấp nhận theo quyết định của anh). ADR đó thay ADR-0097 Q5 (anh
bấm `cargo publish`), bước (iv) *the first publish* và (v) *the post-publish check* trong quyết định
2 của ADR-0097, và tiêu chí thoát 7, 8 (cả hai đọc từ crates.io); ở ADR-0160 nó chỉ thay câu thứ ba
của quyết định 6 và vế sau của quyết định 7 (hai chỗ nói "sau khi publish"). **Phần còn lại của
ADR-0160 giữ nguyên**: sáu crate vẫn ở dạng publish được, job `package` vẫn chạy, nên sau này muốn publish thì vẫn chỉ là một
lệnh `cargo publish --workspace` theo `RELEASING.md`.

Phần trên của plan này không sửa lại. Những gì dưới đây **thay** hàng 7b và 8b của bảng *Chia việc*,
hai dòng 7 và 8 của bảng *Cách kiểm chứng*, và mục *Trình tự phát hành* (phần đó giờ là "nếu có
ngày publish").

### Đã đo trước khi viết (desk, cargo 1.98.0, cargo-semver-checks 0.50.0, không `vendor/`)

Chi tiết ở ADR-0161 *Context*. Tóm tắt:

1. `cargo add --git https://github.com/tmthang86/fixbolt --rev 094bfc3` **không có tên** → lỗi
   `multiple packages found` (repo có 15 package). **Có tên** `fixbolt` → `Adding fixbolt (git)`,
   cargo tự thêm `version = "0.1.0"`. `cargo build` → `Compiling fixbolt v0.1.0
   (https://github.com/tmthang86/fixbolt?rev=094bfc3#094bfc3e)`, `Finished`. Phần trong ngoặc cho
   biết lấy từ đâu: dependency path thì in đường dẫn thư mục, crates.io thì không in gì.
2. `Cargo.lock` ghi đủ 40 ký tự sha: `source = "git+https://github.com/tmthang86/fixbolt?rev=094bfc3#094bfc3e159a294341f6af65e547f47ab3841e0b"`.
3. Tag không tồn tại → `cargo add` exit 101 (trích ở hàng 8b).
4. `cargo semver-checks --workspace --baseline-rev HEAD` → đúng sáu crate, mỗi crate
   `v0.1.0 -> v0.1.0 (no change; assume minor)`, `196 checks: 196 pass, 58 skip`, exit 0.
5. **Bẫy:** khi phiên bản nhảy kiểu "major" (`0.1` → `0.2`), cargo-semver-checks **bỏ qua mọi
   kiểm tra mà vẫn exit 0** (`0 checks: 0 pass, 254 skip`, đã thấy ở 0.0.0 → 0.1.0; đọc mã nguồn
   0.50.0 xác nhận). Nên job chặn phải kiểm là **đã có kiểm tra chạy**, không chỉ đọc exit status.

### Thứ tự mới

**PR của ADR-0161 và Sửa 2 này (chỉ tài liệu) → merge → 7b (gắn tag lên đúng commit merge đó) →
8b.** Tag phải có trước 8b, vì cả hai gate của 8b đọc tag. Hệ quả chấp nhận được: cây mã tại
`v0.1.0` còn dòng "chưa lên crates.io, dùng `rev`" và `CHANGELOG.md` chưa có ngày — người lạ đọc
`main`, và ghi chú phát hành trên GitHub viết từ `CHANGELOG.md` của `main` sau 8b (ADR-0161
*Consequences*).

### Hàng 7b và 8b mới (thay hàng cũ trong *Chia việc*)

| Bước | Kết quả | Người làm | File được sửa / **không** được sửa | Gate — xong khi | Test đỏ trước / đảo ngược | Phụ thuộc |
|---|---|---|---|---|---|---|
| 7b | **Gắn tag `v0.1.0`** lên một commit của `main` mà CI xanh. **Không publish lên crates.io**, không `cargo login`, không token (ADR-0161 quyết định 1) | **manager** | Không file nào trong repo. Commit được gắn tag: commit merge PR #107 (ADR-0161) trên `main`. Commit đó **không** giống mã nguồn `094bfc3`: `main` đã có PR #106 (`Drop` của `Shards` join các thread, sửa `crates/engine/src/shard.rs`; `git diff --stat 094bfc3 3323e41` = 8 file), và #106 được ghi trong `CHANGELOG.md` mục `[0.1.0]` *Fixed* — nên `v0.1.0` mang cả #106 | Trên checkout `main` sạch: `git status --short` rỗng; `git log -1 --format=%H` đúng commit của CI run xanh (ghi run id, đọc từng job); `git tag -a v0.1.0 -m 'fixbolt 0.1.0'`; `git push origin v0.1.0`; `git ls-remote --tags origin 'v0.1.0^{}'` trả về đúng sha đó (trích nguyên văn). Nếu token của phiên có quyền admin: tạo ruleset tag cho `v*` (cấm xoá, cấm cập nhật, cấm force-push) qua `gh api`, rồi đọc lại bằng `gh api repos/tmthang86/fixbolt/rulesets/<id>` và trích `enforcement` cùng danh sách rule; không có quyền thì báo anh | **Đỏ trước:** trong một crate tạm ngoài repo, **trước** khi push tag, `cargo add fixbolt --git https://github.com/tmthang86/fixbolt --tag v0.1.0` → đỏ `failed to find tag 'v0.1.0'`; sau khi push → `Adding fixbolt (git) to dependencies`. **Không đảo ngược ruleset bằng cách thử dời hay xoá tag thật:** nếu ruleset không có hiệu lực, phép thử đó chính là phá bản phát hành; bằng chứng là đọc lại ruleset | PR #107 đã merge; **CI run của chính commit merge đó** (trigger `push` vào `main`) xanh ở mọi job, gồm các gate của tiêu chí 1–6 của ADR-0097 — ADR-0097 quyết định 7 đòi "on the closing commit", nên bằng chứng cũ trong `STATUS.md` (của commit trước #106) không đủ; manager ghi run id đó trước khi gắn tag |
| 8b | Người lạ lấy fixbolt **từ GitHub theo tag**; semver-checks **chặn**, so với tag; tài liệu nói đúng cách cài | **developer (sonnet)**; manager chạy script trên desk, ghi run id, commit | Sửa: `scripts/stranger-check.sh` — chế độ mới `--from git --tag <tag> [--url <url>]` (url mặc định `https://github.com/tmthang86/fixbolt`): crate tạm ở `target/stranger/` như hai chế độ cũ, `cargo add fixbolt --git <url> --tag <tag> --manifest-path …` (luôn có tên `fixbolt`), không `[patch]`; sau build phải thấy trong log dòng `Compiling fixbolt v<ver> (<url>?tag=<tag>#<sha8>)`, và `Cargo.lock` của crate tạm có `source = "git+<url>?tag=<tag>#<sha40>"` với `<sha40>` bằng `git rev-parse <tag>^{commit}` của checkout (tag không có trong checkout → exit 2); `docs/GETTING-STARTED.md` phải chứa `tag = "<tag>"`; phần *WHAT IT CANNOT SEE* ở đầu file thêm: tag bị dời trên GitHub mà checkout cũng dời theo thì script không thấy. `--from packaged`, `--from registry` giữ nguyên. `scripts/check-semver-against-tag.sh` (mới): chạy `cargo semver-checks --workspace --baseline-rev <tag>`, in lại output, **đỏ** nếu thiếu một trong sáu crate hoặc có crate nào `0 checks`; chỉ cho qua `0 checks` khi phiên bản workspace khác phiên bản của tag (cố ý nhảy `0.2.0`), kèm một dòng `NOTE` nói rõ. Hai phiên bản đọc **từ manifest**, như `scripts/check-release-versions.sh` đang đọc: của tag từ `git show <tag>:Cargo.toml`, của cây hiện tại từ `Cargo.toml`, cả hai là `[workspace.package] version` đọc bằng `tomllib` — không suy từ chữ `(major change)` trong output của cargo-semver-checks (đó là cách in, không phải cam kết); tag không có trong checkout → exit 2. `.github/workflows/ci.yml` — job `semver`: bỏ `continue-on-error`, đổi tên thành "… against the v0.1.0 tag (blocking, ADR-0161)", giữ `fetch-depth: 0`, gọi `scripts/check-semver-against-tag.sh v0.1.0`; job mới `stranger-git`: `fetch-depth: 0`, `scripts/stranger-check.sh --from git --tag v0.1.0`, chạy trên cả `pull_request` và `push` vào `main` (theo trigger chung của file, không thêm trigger); thêm hai script vào bước `shellcheck` của job `package`. Tài liệu (nội dung từng file ở *Tài liệu phải cập nhật (Sửa 2)* dưới): `RELEASING.md`, `docs/GETTING-STARTED.md`, `README.md`, `CHANGELOG.md`, `docs/PRD.md` §2, `docs/DESIGN.md` (dòng ~101 và hai dòng §6 ~1462–1463), `docs/GUIDE.md` (~657), `docs/internals/dict.md` (~10), `docs/reference/publishing-a-workspace-to-crates-io.md`, `docs/CONFORMANCE.md` (manager điền run id). **Không**: `crates/`, `Cargo.toml`, `Cargo.lock`, mọi job CI khác, mọi ADR, `STATUS.md` (manager) | Trên desk: `shellcheck -S info scripts/stranger-check.sh scripts/check-semver-against-tag.sh` sạch; `scripts/check-scratch-fixtures.sh` xanh; `scripts/stranger-check.sh --from git --tag v0.1.0` exit 0, trích nguyên văn: `Updating git repository`, `Compiling fixbolt v0.1.0 (https://github.com/tmthang86/fixbolt?tag=v0.1.0#…)`, dòng `source = "git+…"` của `Cargo.lock`, `LOGON OK`, `LOGOUT OK`, `stopped:`; `scripts/stranger-check.sh --from packaged` vẫn xanh; `scripts/check-semver-against-tag.sh v0.1.0` exit 0 với sáu dòng `(no change; assume minor)` và sáu dòng `Checked … N checks` (N > 0); `python3 scripts/check-links.py`; `scripts/check-adr-numbers.sh`. Trên PR: job `semver` và `stranger-git` xanh, **không** `continue-on-error`; manager ghi run id của commit đóng, và run id của lần `push` vào `main` sau merge | **Đỏ trước khi viết chế độ mới** (mã hiện tại; manager đã chạy lại và thấy đúng như vậy): `scripts/stranger-check.sh --from git --tag v0.1.0` → exit 2, `stranger-check: FAIL — unknown argument: --tag` (vòng đọc đối số từ chối `--tag` trước khi tới kiểm tra `--from`). **Đỏ của tag sai, câu viết trước:** `scripts/stranger-check.sh --from git --tag v9.9.9` → exit 1; cargo in (đã đo 2026-09-24): `Updating git repository` `` `https://github.com/tmthang86/fixbolt` ``, `error: failed to load source for dependency` `` `fixbolt` ``, `unable to update https://github.com/tmthang86/fixbolt?tag=v9.9.9`, `failed to find tag` `` `v9.9.9` ``; script thêm `stranger-check: FAIL — could not add fixbolt from https://github.com/tmthang86/fixbolt at tag v9.9.9`. `scripts/check-semver-against-tag.sh v9.9.9` → khác 0, cargo-semver-checks in `couldn't parse revision: "v9.9.9^{tree}"` và `The ref partially named "v9.9.9" could not be found` (đã đo). **Đảo ngược**, mỗi cái viết câu FAIL trước, trả lại, xanh: (a) tạm thêm vào manifest crate tạm `[patch."https://github.com/tmthang86/fixbolt"] fixbolt = { path = "<ROOT>/crates/library" }` → đỏ `stranger-check: FAIL — fixbolt was compiled from <path>, not from https://github.com/tmthang86/fixbolt?tag=v0.1.0` (đây đúng là lỗi "xanh mà thật ra lấy từ path"); (b) tạm đổi dòng cài trên trang thành `tag = "v0.0.9"` → đỏ `FAIL — docs/GETTING-STARTED.md names tag v0.0.9, this run checks v0.1.0`; (c) tạm thêm `--release-type major` vào lệnh trong wrapper → đỏ `FAIL semver: fixbolt-codec ran 0 checks against v0.1.0` (bẫy số 5 ở trên); (d) **manager làm, không phải developer** (vì nó sửa `crates/codec` và cần đẩy một nhánh): trong một `git worktree` tạm, đổi tên `fixbolt_codec::checksum::checksum`, chạy `scripts/check-semver-against-tag.sh v0.1.0` tại chỗ → đỏ với `failure function_missing`, exit khác 0; bỏ worktree, không commit, không push. Bằng chứng "PR bị chặn trên CI" là job `semver` không còn `continue-on-error` trong diff của `ci.yml`, đọc tay | 7b (tag đã có trên GitHub) |

**Tầng:** 8b là developer (sonnet): chỉ script bash, `ci.yml` và tài liệu, không đụng `crates/`,
brief nêu đủ từng câu FAIL. Nếu manager muốn chạy song song, phần tài liệu có thể giao một
developer (sonnet) thứ hai vì file tách rời hẳn; `docs/CONFORMANCE.md` thì manager điền run id sau
khi CI xong.

**Lệch so với brief của manager, có lý do:** job `stranger-git` chạy cả trên `pull_request`, không
chỉ `push` vào `main`. Lý do: `CLAUDE.md` §9 đòi một run id xanh **cho chính commit đóng**, mà commit
đóng của 8b nằm trên PR; và nó bắt được PR làm trang `GETTING-STARTED` dùng API mà `v0.1.0` chưa có.
Tag đã có trước 8b nên không bị vòng lặp "cần tag để xanh, cần xanh để gắn tag".

### *Cách kiểm chứng* — dòng 7 và 8 mới

| Tiêu chí ADR-0097 (theo ADR-0161) | Lệnh | Khi nào |
|---|---|---|
| 7 — người lạ dùng được | `scripts/stranger-check.sh --from packaged` (mỗi PR, không đổi); `scripts/stranger-check.sh --from git --tag v0.1.0`, có dòng `Compiling fixbolt v0.1.0 (https://github.com/tmthang86/fixbolt?tag=v0.1.0#…)` | job `package`; job `stranger-git` từ 8b |
| 8 — API được canh | `scripts/check-semver-against-tag.sh v0.1.0`, chặn, và phải có kiểm tra thật sự chạy | job `semver` từ 8b |

### Bẫy mới

| Bẫy | Test canh |
|---|---|
| Script "xanh" mà fixbolt lấy từ path chứ không từ GitHub | dòng `Compiling fixbolt … (https://…?tag=v0.1.0#…)` + sha trong `Cargo.lock`; đảo ngược (a) |
| semver-checks bỏ qua mọi kiểm tra mà vẫn exit 0 khi phiên bản nhảy "major" | wrapper đếm `N checks` > 0 cho từng crate; đảo ngược (c) |
| Trang `GETTING-STARTED` nói một tag, gate kiểm tag khác | script so `tag = "<tag>"` trên trang; đảo ngược (b) |
| `cargo add --git` không có tên package → lỗi vì repo có 15 package | script luôn truyền `fixbolt`; tài liệu luôn ghi tên |
| Tag bị dời hay bị xoá trên GitHub | ruleset tag `v*` (7b); sha trong `Cargo.lock` phải bằng `git rev-parse` — script không thấy nếu checkout cũng dời theo (ghi trong header) |
| GitHub sập làm job `stranger-git` đỏ dù mã không sai | đọc log: lỗi mạng của `Updating git repository` khác lỗi build; chạy lại job, không sửa mã |
| Lần tag sau: trang, job `stranger-git` và baseline semver vẫn trỏ `v0.1.0` | `RELEASING.md` (bước tag): PR ngay sau mỗi tag đổi cả ba cùng lúc |

### Tài liệu phải cập nhật (Sửa 2, theo bảng `CLAUDE.md` §4)

- [x] `docs/decisions/ADR-0097-…md` — **một dòng** *Superseded in part* đầu file: Q5, bước (iv)
      và (v) của quyết định 2, tiêu chí thoát 7–8, bởi ADR-0161. Không sửa nội dung (ADR đã
      Accepted). Đã có trong PR #107.
- [x] `docs/decisions/ADR-0160-…md` — **một dòng** *Superseded in part* đầu file: câu thứ ba của
      quyết định 6 và vế sau của quyết định 7, bởi ADR-0161. Đã có trong PR #107.
- [ ] `RELEASING.md` (8b) — thành "cách phát hành": các bước **tag** lên đầu (commit `main` xanh có
      run id, `git tag -a`, `git push`, `git ls-remote` đọc lại, ruleset `v*`, PR theo sau đổi tag ở
      trang + job `stranger-git` + baseline semver, ghi chú phát hành GitHub từ `### Summary`); các
      bước crates.io hiện có chuyển xuống mục "Publishing to crates.io, if ever", nội dung giữ
      nguyên, mở đầu bằng "cần một ADR mới thay ADR-0161 quyết định 1"; bước 9 cũ ("báo manager
      chạy `--from registry`") viết lại cho đúng.
- [ ] `docs/GETTING-STARTED.md` (8b) — dòng cài: `cargo add fixbolt --git
      https://github.com/tmthang86/fixbolt --tag v0.1.0` và dòng TOML tương ứng; xoá ghi chú "Not
      on crates.io yet"; nói một câu: không có trên crates.io theo quyết định, tài liệu API là
      `cargo doc --open`; đoạn ở dòng ~253 nói `--from git --tag v0.1.0` thay cho `--from registry`.
- [ ] `README.md` (8b) — mục cài đặt (dòng ~63–77): như trên; bỏ "Not on crates.io yet … until
      both are true"; một câu về mức hỗ trợ không đổi.
- [ ] `CHANGELOG.md` (8b) — `## [0.1.0] — <ngày gắn tag>`, một dòng "released as the git tag
      `v0.1.0`; not published to crates.io (ADR-0161)"; bỏ "date filled in when the owner
      publishes"; mục điều kiện `1.0` giữ nguyên.
- [ ] `docs/PRD.md` §2 *Phase 3* (8b) — câu "the owner runs `cargo publish`" và mục *First publish,
      `0.1.0`* đổi thành tag git; hai dòng tiêu chí 7 và 8 của bảng exit criteria đổi theo ADR-0161.
- [ ] `docs/CONFORMANCE.md` (8b, manager điền run id) — tiêu chí 7 và 8: lệnh, máy, run id của
      commit đóng và của lần `push` vào `main`.
- [ ] `docs/DESIGN.md` (8b) — dòng ~101 ("Six are published to crates.io") thành "publish-shaped,
      released as git tags, not uploaded (ADR-0161)"; hai dòng của bảng §6 (hiện ở dòng ~1462–1463:
      người lạ `--from registry` sau publish; semver "not yet blocking", `--baseline-rev
      origin/main`, `continue-on-error`) đổi theo gate mới — `CLAUDE.md` §4 đòi sửa §6 **cùng
      commit** với gate.
- [ ] `docs/GUIDE.md` (8b) — dòng ~657 ("`v0.1.0` exists once `RELEASING.md` has run; before that,
      pin a commit instead") thành: `v0.1.0` là tag git có sẵn.
- [ ] `docs/internals/dict.md` (8b) — dòng ~10 ("`cargo add fixbolt` builds with nothing but
      crates.io") thành: build chỉ từ những gì có trong repo (git) hoặc trong `.crate`, không cần
      `vendor/`.
- [ ] `docs/reference/publishing-a-workspace-to-crates-io.md` (8b) — hai bẫy đã đo: `cargo add
      --git` cần tên package trong repo nhiều crate; cargo-semver-checks bỏ qua mọi kiểm tra khi
      nhảy "major" mà vẫn exit 0.
- [ ] `STATUS.md` (manager, khi đóng) — *Start here* mới; *Do not* "chạy `cargo publish`" giữ; dòng
      *Not proven* về `LicenseRef-QuickFIX-1.0` đổi thành "chỉ khi có ngày publish".

### Ngoài phạm vi (thêm)

- **Publish lên crates.io**, kể cả một bản "giữ chỗ" cho tên `fixbolt`: chính sách của crates.io
  coi đó là chiếm tên (ADR-0161 *Research*).
- **Một trang tài liệu API thay docs.rs** (ví dụ GitHub Pages).
- Thêm trigger `push: tags` cho CI — `CLAUDE.md` §8 giữ nguyên hai trigger.

### Câu hỏi cho anh

1. **Ruleset khoá tag `v*`** (cấm xoá, cấm dời) trên repo GitHub: đồng ý để manager tạo không, nếu
   token của phiên có quyền admin? Không có ruleset thì "tag không bao giờ dời" chỉ là lời hứa.

## Nhật ký giao hàng

| Bước | Commit | Bằng chứng |
|---|---|---|
| 6a | *(manager commit)* | `check-release-versions.sh` đỏ trước khi sửa manifest: 37 dòng FAIL, trong đó có câu viết trước `FAIL fixbolt-dict: dependency fixbolt-codec has no version requirement`; sau đó xanh `OK — 6 crates at 0.1.0 …`. Hai lần đảo ngược: `"0.1.0"` → `… is not "=0.1.0"`; sửa một byte `crates/codec/LICENSE-MIT` → `LICENSE-MIT copies differ`. Trên bản chép không có `vendor/`: `cargo publish --workspace --dry-run --allow-dirty` exit 0, sáu `Packaging` và sáu `Verifying`. `cargo +1.88.0 check -p fixbolt --all-features` và `-p fixbolt-engine --all-features` đều `Finished`. Mô phỏng docs.rs (`--cfg docsrs -D warnings`, nightly) cho sáu crate đều exit 0 |
| 6a' | *(manager commit)* | Tìm ra nguyên nhân bằng cách chỉ đổi một biến: 1.85 → 0 cảnh báo, 1.88 → 15. Sau `clippy --fix`: clippy ba kiểu exit 0; `score` 4 passed, `wire` 2 passed (cả hai mode), `score_fixt` 2 passed, `wire_fixt` 1 passed; `alloc` của session và engine toàn 0; `check-indexing-debt` 176, trần 176. Đo lại dry run có README, xem báo cáo của bước |
