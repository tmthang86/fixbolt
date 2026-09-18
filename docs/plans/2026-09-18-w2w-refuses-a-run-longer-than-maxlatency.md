# w2w từ chối một lần chạy paced dài hơn MaxLatency

> **Loại:** Plan · **Ngày:** 2026-09-18 · **Trạng thái:** Đã duyệt 2026-09-18 (owner: theo đề xuất cho cả hai câu hỏi)
> **Phạm vi:** `tools/w2w` — đóng open item 90 của `STATUS.md`

## Bối cảnh

`[measured 2026-09-15]` ở boot B bước B4, lệnh `tools/w2w --interval 1000000` với `WARMUP=5
MESSAGES=120` chết giữa chừng: engine trả `35=3 373=10` *SendingTime accuracy problem* cho
sequence 122, `w2w` thoát 101. Engine **đúng**: `w2w` in dấu thời gian `52=` cho **toàn bộ**
message trước khi đồng hồ chạy, nên ở nhịp 1 s thì message thứ 121 mang một `52=` đã già 120 s,
vượt `max_skew_ms` của phiên. Lần chạy được làm lại với 100 message và xanh.

Sự cố đã được ghi ở `docs/reference/a-paced-run-outlived-the-sessions-maxlatency.md`, nhưng
**chưa có gì canh**: không test hồi quy, không refusal. Đó là một nghĩa vụ `CLAUDE.md` §4 còn
treo. Việc này thêm một refusal thuần tính toán trong `w2w` — chạy sai tham số thì bị từ chối
ngay trước khi tốn một lần chạy trên máy §9 — kèm test.

Đây là thay đổi nhỏ, khoảng một giờ.

## Những gì đã biết chắc

- **Cả warmup lẫn cửa sổ đo đều được pace.** `tools/w2w/src/main.rs:1816-1836`: `Pacer::new` được
  tạo trước vòng warmup, warmup gọi `pacer.wait()`/`pacer.sent_now()`, rồi vòng đo dùng cùng
  `pacer`. Câu "unpaced warmup" ở `main.rs:958` chỉ nói về trường hợp **không** có `--interval`
  (khi đó `Pacer` không đọc đồng hồ). Vậy công thức của `STATUS.md` dùng `warmup + messages` là
  đúng về số message được pace.
- **Mọi message được render trước khi đồng hồ chạy**, `52=` kèm theo (`main.rs:1802-1812`, comment
  "Every message is rendered before the clock starts"; `stamp()` ở `main.rs:3986`). Message đầu
  tiên gửi ngay sau khi render nên tuổi ≈ 0; message thứ `i` (0-based) gửi khoảng `i × interval`
  sau đó, và `Pacer::sent` đặt mốc kế tiếp từ **lần gửi trước**, nên độ trễ chỉ dồn thêm chứ không
  bị bù lại. Tuổi lớn nhất là của message cuối: `(warmup + messages − 1) × interval`.
- **Khớp với quan sát boot B**: seq 122 tương ứng `i = 120` (`test_request(2 + i, i)`,
  `main.rs:3953`) → tuổi ≈ 120 s, vừa vượt ngưỡng; seq 121 (`i = 119`, ≈ 119 s) vẫn qua. Con số
  105 message của lần chạy lại (`i` cao nhất = 104) nằm an toàn dưới ngưỡng.
- **Ngưỡng là 120 000 ms**: `crates/session/src/lib.rs:403`
  `pub const DEFAULT_MAX_SKEW_MS: u64 = 120_000;` — mặc định của `Config`
  (`lib.rs:661`), và `w2w` dựng phiên bằng `Config::acceptor(b"FIX.4.4", b"ISLD", b"W2W")`
  (`main.rs:2345`) mà **không** gọi `with_max_skew_ms`, nên nửa engine của `w2w` đúng là 120 s.
- **Phép so sánh là `<=`**: `lib.rs:3102` `stamp.abs_diff(self.now_ms) <= cfg.max_skew_ms` — từ
  chối khi tuổi **vượt quá** 120 000 ms.
- **`stamp()` chỉ có độ phân giải giây** (`main.rs:3986-3999`, `as_secs()`), luôn làm tròn xuống,
  nên tuổi thật mà engine tính có thể lớn hơn tuổi lý thuyết tới **1 s**.
- **QuickFIX**: `CheckLatency` mặc định bật và `MaxLatency` mặc định **120 giây**; khi lệch quá thì
  peer trả `35=3` với `373=10`. Nguồn: tài liệu cấu hình QuickFIX-Go
  (<https://pkg.go.dev/github.com/alpacahq/quickfix/config>), và các thư SendingTime accuracy trên
  mailing list QuickFIX (<https://sourceforge.net/p/quickfix/mailman/message/19572205/>). Ý nghĩa
  đúng như code repo này: so `|now − SendingTime|` với `MaxLatency`, không phải một cửa sổ một
  chiều.
- **`tools/w2w` build và chạy test được trên macOS.** `cargo test -p fixbolt-w2w` →
  `test result: ok. 15 passed; 0 failed`. Các phần Linux-only nằm sau `cfg(target_os = "linux")` và
  không cản test đơn vị.
- **`--listen` đã từ chối sẵn `--interval`, `--messages`, `--hold-ms`** (`main.rs:711-730`,
  `half_of`), nên nửa engine không có gì để canh: nó không biết nhịp và không tự gửi.

## Cách làm

Thêm một **hàm thuần** trong `tools/w2w/src/main.rs`, cạnh `interval_of`:

```rust
/// Refuse a paced run whose last message's `52=` would be older than the
/// counterparty's MaxLatency by the time it is sent.
fn paced_run_fits(interval_us: u64, warmup: usize, n: usize, max_skew_ms: u64) -> Result<(), String>
```

Luật (chỉ áp dụng khi `interval_us > 0` và `warmup + n >= 1`):

```
age_ms = interval_us × (warmup + n − 1) / 1000
refuse khi  age_ms + MARGIN_MS >= max_skew_ms
```

- `MARGIN_MS = 1000`. Một giây này trả đúng cho phần làm tròn xuống của `stamp()`; phần trôi do
  round trip chậm hơn nhịp thì không bù được bằng một hằng số, nên refusal nói rõ nó chỉ là biên
  tối thiểu.
- `max_skew_ms` mặc định là `fixbolt_session::DEFAULT_MAX_SKEW_MS` — **đọc hằng số, không viết
  `120_000` lần thứ hai** (một luật, một chỗ). `w2w` đã phụ thuộc `fixbolt-session`.
- Thêm cờ `--max-skew-ms <ms>` để khai báo `MaxLatency` của đối tác khi chạy `--connect` tới một
  acceptor khác, đọc qua `value_of` (cờ có mặt mà thiếu giá trị thì bị từ chối, như mọi cờ A3a).
  Không có cờ "tắt kiểm tra": ai cần rộng hơn thì khai đúng con số của đối tác.
- Nội dung refusal nêu cả hai vế để người chạy biết phải rút cái nào, ví dụ:
  `w2w: --interval 1000000 us x (warmup 5 + messages 120 - 1) = 124000 ms; with 1000 ms of margin
  that is not under the counterparty's MaxLatency 120000 ms. Lower --interval, or send at most 118
  messages including --warmup. See docs/reference/a-paced-run-outlived-the-sessions-maxlatency.md`
- Gọi ở `main`, ngay sau khi `warmup`, `n`, `interval_us` đã đọc xong và **trước** khi engine hay
  socket được dựng, cho `Half::Both` và `Half::Connect`. `Half::Listen` không gọi (không có nhịp,
  không có số message — `half_of` đã từ chối các cờ đó).

File đụng tới: `tools/w2w/src/main.rs` (duy nhất), cộng các tài liệu ở mục dưới.

## Bất biến bị đụng tới

**Không có bất biến nào của `CLAUDE.md` §2 bị đụng.** Việc này không sửa `codec`, `session`,
`engine` hay `transport`; nó thêm một phép tính số nguyên chạy **một lần lúc khởi động**, trước
khi `ARMED` được bật, nên không nằm trên hot path và không vào cửa sổ đếm allocation. Chuỗi thông
báo lỗi dùng `format!` là chuyện của đường khởi động — cùng khuôn với các refusal sẵn có
(`half_of`, `--allow-unisolated`). Rule 7 (không `unwrap`/`expect`) vẫn giữ: hàm trả `Result`.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | **developer (sonnet)** — trong `tools/w2w/src/main.rs`: thêm `MARGIN_MS`, `max_skew_of(args)` đọc `--max-skew-ms` (mặc định `fixbolt_session::DEFAULT_MAX_SKEW_MS`), hàm thuần `paced_run_fits`, và gọi nó trong `main` cho `Half::Both`/`Half::Connect`. Thêm vào `mod tests` (quanh `main.rs:4284`): `a_paced_run_past_maxlatency_is_refused`, `a_paced_run_inside_maxlatency_is_allowed`, `an_unpaced_run_is_never_refused`, `a_declared_maxlatency_moves_the_bound` | — |
| 2 | **developer (sonnet)** — cập nhật module doc phần *Two halves, and pacing* (`main.rs:136-143`) nói về refusal, cập nhật `docs/reference/a-paced-run-outlived-the-sessions-maxlatency.md` mục *What guards it* (trỏ tên test + hàm), `docs/hft-playbook.md` dòng mô tả `--interval` (mục 110/135), và STATUS item 90 | 1 |
| 3 | **manager** — chạy lại gate, làm reversal chứng minh test đỏ đúng chỗ, commit | 1, 2 |

## Cách kiểm chứng

Lệnh gate (macOS đủ, không cần máy Linux):

```
cargo test -p fixbolt-w2w
cargo fmt --check && cargo clippy --all-targets -- -D warnings
cargo test --no-default-features -p fixbolt-w2w
```

Đạt khi `cargo test -p fixbolt-w2w` in `test result: ok. 19 passed; 0 failed` (15 test hiện có +
4 test mới) và clippy im lặng.

**Chứng minh bằng đảo ngược** (bước 3, ghi output vào nhật ký giao hàng): đổi `>=` trong
`paced_run_fits` thành `>` **và** bỏ `MARGIN_MS`, rồi chạy lại. Câu FAIL chờ đợi, viết ra trước khi
chạy:

```
---- tests::a_paced_run_past_maxlatency_is_refused stdout ----
assertion failed: paced_run_fits(1_000_000, 5, 120, 120_000).is_err()
```

Khôi phục, chạy lại, thấy xanh.

**Kiểm chứng bằng tham số thật, không chỉ test đơn vị**: chạy tay
`cargo run -p fixbolt-w2w -- --connect 127.0.0.1:9 --interval 1000000 --warmup 5 --messages 120`
trên laptop và dán stderr: nó phải in refusal và thoát khác 0 **mà không mở socket nào** (địa chỉ
port 9 cố tình không có ai nghe — nếu thấy lỗi connect nghĩa là refusal đặt sai chỗ). Rồi chạy lại
với `--messages 100` và xem nó đi tới bước connect.

## Tài liệu phải cập nhật

- [ ] `docs/reference/a-paced-run-outlived-the-sessions-maxlatency.md` — mục *What guards it*: nêu
      tên hàm và tên test thay cho "Nothing yet"
- [ ] `STATUS.md` item 90 — đóng, trỏ commit và CI run id
- [ ] `docs/hft-playbook.md` — dòng `--interval` (mục 110 và 135): nêu trần và cờ `--max-skew-ms`
- [ ] `tools/w2w/src/main.rs` module doc, phần *Two halves, and pacing* (cùng commit)
- [ ] `CHANGELOG.md` — một dòng, mục tooling

Các hàng khác của bảng §4 **không** áp dụng: không đổi API công khai của crate nào, không đổi hành
vi biên của phiên, không có số hiệu năng mới, không dời hàng nào của `DESIGN.md` §8 (các con số
paced đã công bố vẫn nằm dưới trần này — 105 message ở nhịp 1 s).

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Tính `warmup + n` thay vì `warmup + n − 1`, chặn nhầm một lần chạy hợp lệ | `a_paced_run_inside_maxlatency_is_allowed` với `(1_000_000, 5, 100)` — boot B đã chạy xanh thật |
| `interval_us × total` tràn `u64` với giá trị vô lý | dùng `u128` hoặc `checked_mul`; test `a_huge_interval_is_refused_not_overflowed` nếu developer thấy cần |
| Viết cứng `120_000` thay vì đọc `DEFAULT_MAX_SKEW_MS` — hai luật sẽ lệch nhau | `a_declared_maxlatency_moves_the_bound`, và mắt người đọc diff |
| Refusal đặt sau khi socket/engine đã dựng, tốn một lần chạy máy §9 | bước kiểm chứng tay với `--connect 127.0.0.1:9` |
| Chặn nhầm lần chạy **không** pace (`--interval 0`, mặc định 20 000 message) | `an_unpaced_run_is_never_refused` |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Biên 1 s không đủ khi round trip chậm hơn nhịp và độ trễ dồn lại | Thấp | Refusal nói rõ đây là biên tối thiểu; `w2w` vẫn in số send "late" và engine vẫn từ chối đúng nếu vượt |
| `--connect` tới đối tác có `MaxLatency` khác 120 s | Thấp | Cờ `--max-skew-ms` khai đúng con số của đối tác |

## Ngoài phạm vi

- **Không** đổi cách `w2w` render `52=` (ví dụ đóng dấu lại lúc gửi). Đó là một quyết định thiết kế
  khác: nó sẽ đưa việc định dạng vào vòng đo, đúng thứ mà `docs/reference/measured-costs.md` cấm.
- **Không** đụng `crates/session`: ngưỡng 120 s và phép so sánh `<=` là hành vi đúng.
- **Không** thêm gate CI mới; test đơn vị chạy sẵn trong `cargo test --all`.

## Câu hỏi để owner quyết

1. **Refusal nên nằm ở `w2w` hay ở engine?** Kế hoạch này đặt ở `w2w` vì đây là lỗi tham số của
   benchmark, còn engine đang hành xử đúng theo spec. Nhưng nếu sau này có công cụ khác cũng render
   trước rồi gửi theo nhịp, luật sẽ phải viết lại lần nữa. Nếu owner muốn luật ở chỗ dùng chung,
   cần một ADR và kế hoạch riêng — không làm ở đây.
2. **Cờ `--max-skew-ms` có đáng thêm không**, hay cứ cứng theo `DEFAULT_MAX_SKEW_MS` cho gọn? Bỏ cờ
   thì `--connect` tới đối tác có `MaxLatency` khác sẽ bị chặn sai.

## Bẫy phát hiện thêm khi lập kế hoạch

`crates/session/src/lib.rs:382-401` — doc comment *"QuickFIX's default `MaxLatency`, in
milliseconds"* đang dính vào `DEFAULT_APP_SCRATCH` (hai khối doc nối liền nhau), còn
`DEFAULT_MAX_SKEW_MS` ở dòng 403 **không có doc nào**. Rustdoc của `fixbolt-session` vì thế đang mô
tả sai một hằng số công khai. Sửa là một dòng, nhưng nó nằm trong `crates/session` nên **không gộp
vào kế hoạch này**: đề nghị owner cho một commit `docs:` riêng.

## Nhật ký giao hàng

Chưa có.
