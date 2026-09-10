# Ba cửa trước của `hft` chưa từng có ai đi qua

> **Loại:** Plan · **Ngày:** 2026-09-10 · **Trạng thái:** **Đã duyệt 2026-09-10**
> **Phạm vi:** phát hiện của [the-doc-set](2026-09-02-the-doc-set.md), `STATUS.md`
> dòng 2085. Chạm `crates/engine/tests/`, docs. **Không chạm** `codec`, `session`, và không đổi
> một dòng nào trong `crates/*/src`.
>
> **Một nửa chạy được trên Mac, một nửa không.** `serve_hft` và `serve_hft_with_recovery` build
> và chạy trên darwin. `serve_sharded_hft` nằm sau `cfg(all(feature = "affinity",
> target_os = "linux"))` và **không compile trên máy này** — nó được viết ở đây và chứng minh
> bằng CI.

## Bối cảnh

Dự án này đặt claim của nó lên mode `hft`. Ba cửa trước của mode đó — `serve_hft`,
`serve_hft_with_recovery`, `serve_sharded_hft` — **không test nào, bench nào, tool nào gọi**.
`the-doc-set` ghi lại điều này 2026-09-02 và nói thẳng đây là lỗi **code**, cần plan riêng.

Điều làm nó tệ hơn một lỗ hổng coverage thường: **`hft` *có* được chứng minh, chỉ là không qua
cửa của nó**. `tools/w2w` tự dựng `Engine` rồi tự gọi `pump` với `Spin` (`tools/w2w/src/main.rs:122,
647-664`); `scripts/check-no-kernel-sleep.sh` trace đúng cái binary đó. Nên mọi con số `hft`
đã công bố nói về **một engine dựng bằng tay**, không phải về hàm mà người dùng thư viện gọi.
Ba hàm đó lại nằm trong API công khai của crate `fixbolt` (`crates/library/src/lib.rs:27, 36, 64`).

Đây là đúng lớp lỗi `a-test-that-cannot-fail-reads-as-coverage.md` nói tới, ở quy mô một mode.

## Những gì đã biết chắc

| Sự thật | Nguồn |
|---|---|
| Sáu hàm `*_hft*`: `serve_hft`, `serve_hft_with`, `serve_hft_with_recovery`, `serve_hft_with_recovery_with`, `serve_sharded_hft`, `serve_sharded_hft_with` | `crates/engine/src/lib.rs:1962, 1980, 2028, 2061`; `crates/engine/src/shard.rs:440, 469` |
| **Không nơi nào trong `crates/*/tests`, `crates/*/benches`, `tools/`, `benches/` gọi chúng.** Ba lần xuất hiện duy nhất là **comment** | `grep` 2026-09-10 trên `settings_roles.rs:19`, `registry.rs:11`, `tools/interop/src/main.rs:50` |
| Cả ba đều là API công khai của crate `fixbolt` | `crates/library/src/lib.rs:27, 36, 64` |
| `serve_hft_with` dựng `HftAcceptorEngine` với `crate::wait::Spin` rồi gọi `pump` | `crates/engine/src/lib.rs:1980-2012` |
| `tools/w2w` **không** gọi `serve_hft`; nó tự `Engine::new` + `pump` và tự chọn `Spin`/`Yield` | `tools/w2w/src/main.rs:122, 644-664` |
| `pub mod shard` nằm sau `#[cfg(all(feature = "affinity", target_os = "linux"))]` | `crates/engine/src/lib.rs:40-41` |
| `crates/engine/tests/shard_wire.rs` đã có hình dạng cần bắt chước: `#![cfg(all(feature = "affinity", target_os = "linux"))]`, đồng hồ chia sẻ qua `AtomicU64`, settle theo wall time | `crates/engine/tests/shard_wire.rs:28-40` |
| CI có bước `--features affinity` trên Linux — nó là thứ đã tìm ra một `#![allow]` bị sót 2026-09-08 | `crates/engine/tests/shard_wire.rs:33-38` |
| Hai kịch bản kiểm mode đã tồn tại và **cả hai đều trỏ vào `tools/w2w`**, không vào ba hàm này | `scripts/check-no-kernel-sleep.sh`, `scripts/check-standard-gives-the-core-back.sh` |
| `serve_sharded_hft` không có biến thể `_with_recovery` và không dừng được — item 32 (a), vẫn mở | `STATUS.md:1953` |

## Cách làm

Một test file mới cho mỗi nửa. **Không sửa một dòng nào trong `crates/*/src`** — nếu một cửa
trước hoá ra hỏng, đó là phát hiện của plan này, ghi lại, và sửa nó là plan kế.

1. **`crates/engine/tests/hft_wire.rs`** — `serve_hft` và `serve_hft_with_recovery` bị lái qua
   một socket thật: bind, một client nối vào, `Logon`, một application message, một
   `Logout`, rồi dừng qua `Handles`. Bắt chước hình dạng `settings_wire.rs` / `shutdown.rs`,
   là những test đã lái `serve` đúng kiểu đó.
2. **`serve_hft_with_recovery` được cho một session đã resume**, để nhánh recovery thật sự
   chạy chứ không chỉ compile. `crates/engine/tests/engine_recovery.rs` và `on_disk.rs` đã có
   khuôn.
3. **`crates/engine/tests/shard_hft.rs`**, sau `#![cfg(all(feature = "affinity",
   target_os = "linux"))]`, cho `serve_sharded_hft`. **Không chạy được trên máy này**; nó xanh
   hay đỏ do CI nói.
4. **Mỗi test khẳng định nó thật sự chạy `hft`**, không chỉ khẳng định là không crash. Cách rẻ
   nhất mà không cần Linux: `events()`/`Handles` cho thấy session lên và xuống trong một
   khoảng thời gian nhỏ hơn nhiều so với bất kỳ poll timeout nào — cùng lập luận với assertion
   thứ tư của `check-standard-gives-the-core-back.sh`, cái duy nhất nhìn thấy một engine
   ngồi chờ hết timeout. **Ba assertion đầu của script đó bị ba engine hỏng khác nhau lừa được**,
   và đó là lý do đừng chỉ đếm CPU.
5. **Đảo chiều từng test**: đổi `serve_hft` thành `serve`, xác nhận test vẫn xanh (nó **phải**
   xanh — cả hai đều phục vụ đúng), rồi ghi lại rằng đó chính là giới hạn của gate này: nó
   canh *cửa trước tồn tại và hoạt động*, **không** canh *cửa đó không ngủ*. Cái sau cần Linux
   và là bước 6.
6. **`scripts/check-no-kernel-sleep.sh` được trỏ thêm vào một binary gọi `serve_hft`.** Đây là
   nửa duy nhất thật sự đóng lỗ hổng bất biến 4 cho cửa trước. **Cần Linux**, và nó là phần đầu
   tiên bị cắt nếu phạm vi phải co lại — cắt thì nói rõ, đừng im.

File tạo: `crates/engine/tests/hft_wire.rs`, `crates/engine/tests/shard_hft.rs`.
File sửa: `scripts/check-no-kernel-sleep.sh` (bước 6), docs.

## Bất biến bị đụng tới

- **Bất biến 4** — đây là bất biến plan này tồn tại vì nó. **Cả hai nửa đều là luật**: mọi khẳng
  định phải gọi tên mode nó nói về. Test ở bước 1-5 nói về *cửa trước chạy được*, **không** nói
  về *engine thread không ngủ*; viết docs cho nó như thể ngược lại là đúng loại lỗi ADR-0013
  cấm. Bước 6 là nửa còn lại và nó cần Linux.
- **Bất biến 1** — `benches/alloc.rs` không nhìn thấy được test; plan này không thêm case alloc
  nào và không tuyên bố gì về allocation.
- **Bất biến 6** — `shard_hft.rs` chỉ compile sau `--features affinity`, và một file bị feature
  che là **vô hình với clippy chạy dưới feature mặc định**. Đã cháy một lần
  (`shard_wire.rs:33-38`). Chạy clippy dưới cả hai bộ feature.
- **Bất biến 7** — test không phải `crates/*/src`, nên `#![allow(clippy::unwrap_used, …)]` ở
  đầu file là đúng luật, giống mọi test khác. `scripts/check-indexing-debt.sh` không đếm gì
  ngoài `crates/*/src`, nên ceiling 181 không được đổi.
- **Bất biến 10** — plan này **không** công bố số nào. `serve_hft` chạy trong test là hành vi,
  không phải latency.

## Chia việc

| Bước | Kết quả | Phụ thuộc | Máy |
|---|---|---|---|
| 1 | `hft_wire.rs`: `serve_hft` lên, phục vụ, dừng qua `Handles` | — | Mac |
| 2 | `serve_hft_with_recovery` với một session resume thật | 1 | Mac |
| 3 | `shard_hft.rs` cho `serve_sharded_hft`, sau cfg `affinity`+linux | — | **CI** |
| 4 | Mỗi test khẳng định mình đã chạy, không chỉ khẳng định không crash | 1, 2 | Mac |
| 5 | Đảo chiều, và **ghi lại rõ gate này không canh cái gì** | 4 | Mac |
| 6 | `check-no-kernel-sleep.sh` trace một binary gọi `serve_hft` | 1 | **Linux** |

## Cách kiểm chứng

- `cargo test --all` và `cargo test --all --no-default-features`, **đọc output**, không đọc exit
  code, không qua pipe nuốt mất status.
- `cargo clippy --all-targets -- -D warnings`, chạy **hai lần**: feature mặc định, và
  `--features affinity`. Lần thứ hai là lần duy nhất nhìn thấy `shard_hft.rs`.
- 59/59 định nghĩa — plan này không sửa session, nên đây là kiểm tra hồi quy.
- **CI cho bước 3.** Không có cách nào chứng minh nó từ Mac, và nói "chắc là xanh" là đúng thứ
  `CLAUDE.md` §10 gọi là kết quả suy ra chứ không phải kết quả quan sát.
- Một CI run xanh, gọi tên bằng id, cho đúng commit được đóng.

## Tài liệu phải cập nhật

- [ ] `STATUS.md` — lỗ hổng ở dòng 2085 được đóng tới đâu, **và phần nào vẫn hở** (bước 6 nếu bị cắt)
- [ ] `docs/best-practices-hft.md` — gọi tên mode, theo bảng §4
- [ ] `docs/CONFORMANCE.md` nếu bước 6 chạy: lệnh, máy, CI run id
- [ ] `docs/reference/` nếu một cửa trước hoá ra hỏng — ưu tiên cao nhất

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Test compile, chạy, xanh, mà session chưa từng lên — coverage giả | Bước 4: khẳng định trên event/`Handles`, không phải trên "không panic" |
| `serve_hft` spin, test không dừng được, CI treo | Dừng qua `Handles::admin()` như `shutdown.rs`; timeout cứng trong test |
| Viết docs như thể test này chứng minh engine thread không ngủ | Bước 5 ghi thẳng giới hạn vào doc comment của file; bất biến 4 |
| `shard_hft.rs` bị clippy feature-mặc-định bỏ qua | Chạy clippy `--features affinity`; đã cháy 2026-09-08 |
| Một core bị đốt suốt cả suite trên laptop | Test ngắn, dừng dứt khoát; đo thời gian suite trước và sau |
| Bước 6 bị bỏ im lặng, và STATUS đọc như thể bất biến 4 đã kín | §9: hộp nào chưa tick thì báo **chưa xong**, và nói rõ cái nào |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Một trong ba cửa trước thật sự hỏng — chưa ai chạy chúng bao giờ | **Trung bình** | Đó là phát hiện, không phải thất bại. Ghi vào `docs/reference/`, mở item, sửa ở plan kế. Không sửa `src` trong plan này |
| `serve_sharded_hft` không dừng được (item 32 a), nên test không kết thúc sạch | **Trung bình** | Đã biết trước. Nếu chặn, bước 3 dừng lại và nói rõ nó bị chặn bởi item 32 (a) |
| Test spin làm CI chậm hoặc flaky | Thấp | Ngắn, có timeout, đo thời gian suite trước/sau |
| Bước 6 không làm được vì không có bàn Linux hôm nay | **Cao** | Cắt bước 6, đóng plan với năm bước, **nói rõ bất biến 4 vẫn hở ở cửa trước** |

## Ngoài phạm vi

- Sửa bất kỳ dòng nào trong `crates/*/src`, kể cả khi một cửa trước hỏng.
- Item 21 (`serve_hft` không pin core gì) và item 32 (a) (`serve_sharded_hft` không dừng được).
- Bất kỳ con số latency nào. Đây là gate hành vi.
- `docs/hft-playbook.md` — không có hàng hardware/BIOS/kernel nào đổi.

## Nhật ký giao hàng

*(chưa bắt đầu)*
