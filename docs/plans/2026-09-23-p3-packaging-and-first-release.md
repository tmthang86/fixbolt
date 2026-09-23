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

## Nhật ký giao hàng

*(chưa có)*
