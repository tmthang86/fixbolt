# Đọc `52=` ở mọi độ chính xác, và cái bẫy là `.1` không phải một phần nghìn

> **Loại:** Plan · **Ngày:** 2026-09-09 · **Trạng thái:** Xong
> **Phạm vi:** `STATUS.md` item 59. Chạm `session` (`clock.rs`), `dict` (`field_type.rs`),
> `scripts/interop.sh`, docs. **Không chạm** `codec`, `engine`, `transport`.
> Quyết định gate: **ADR-0058** (`Proposed`, viết cùng ngày, kèm plan này).
> **Không viết một dòng code nào trước khi ADR-0058 được duyệt.**

## Bối cảnh

Item 59 nói thẳng vấn đề: một đầu QuickFIX C++ chỉnh `TimestampPrecision` thành 1, 2, 4, 5, 7
hoặc 8 sẽ gửi một `52=` mà engine này **không đọc được**, và trước `Logon` thì cái không đọc được
đó biến thành **cúp máy, không gửi byte nào**. Đúng loại lỗi mà nửa A của
[timestamp-micros](2026-09-04-timestamp-micros.md) sinh ra để đóng — chỉ khác con số độ rộng.

Điều làm nó đáng làm ngay, chứ không phải một hàng trong danh sách nợ: **lỗi này nằm trong tầm với
của chính file cấu hình của oracle.** Không cần venue thật, không cần capture. Sửa một dòng trong
`initiator-*.cfg` là dựng lại được.

## Những gì đã biết chắc

Mọi dòng dưới đây đo hoặc đọc trong ngày 2026-09-09, không dòng nào chép lại từ tài liệu cũ.

| Sự thật | Nguồn |
|---|---|
| Hai reader hiện nhận đúng 4 độ rộng: **17, 21, 24, 27**; sáu độ rộng 19, 20, 22, 23, 25, 26 đều trả `None` / `false` | đo trực tiếp bằng một crate thử ngoài repo, bảng đầy đủ trong ADR-0058 |
| 18 byte (`20260909-10:00:00.`) bị **cả hai** reader từ chối hôm nay | cùng phép đo trên |
| `parse_utc` chọn số chữ số phần lẻ bằng `match s.len()` trên 4 hằng số | `crates/session/src/clock.rs:65-75` |
| `dict` chọn bằng `matches!(v.len(), 8 \| 12 \| 15 \| 18)` trong `fn time` | `crates/dict/src/field_type.rs:236` |
| `fn time` **dùng chung** cho `UtcTimeOnly` lẫn `UtcTimestamp` | `crates/dict/src/field_type.rs:161-165` |
| Trong `AwaitingLogon`, `time_ok == false` → `Refusal::BadSendingTime` → cúp máy im lặng | `crates/session/src/lib.rs:3003, 3016-3018` |
| QuickFIX C++ parse: từ chối `length < 17 \|\| length > 27`, rồi bắt `.` và mọi ký tự còn lại phải là chữ số — **18 byte được nhận**, `fraction = 0` | `386ce46e:src/C++/FieldConvertors.h`, `UtcTimeStampConvertor::convert(const std::string&)`, đọc bằng `git cat-file` từ object store của `vendor/quickfix` |
| QuickFIX C++ serialise: `precision` bị `clamp` về `0..=9`, ghi `17 + 1 + precision` byte | cùng file |
| `PRECISION_FACTOR = {1000000000, 100000000, 10000000, 1000000, …}` và `convertToNanos` nhân `fraction` với `PRECISION_FACTOR[precision]` | `386ce46e:src/C++/FieldTypes.h:56, 347-393` |
| ⇒ **`.1` = 100 ms**, `.12` = 120 ms. Phần lẻ là phân số thập phân, đọc từ trái, đệm 0 sang phải | suy ra trực tiếp từ hai dòng trên, không phải phỏng đoán |
| `scripts/interop.sh` §4h đã là một scenario phán xét **byte** chứ không phán xét dòng step, và đã biết cách đếm cái *không* có mặt | `scripts/interop.sh:898-1057` |
| Trần `check-indexing-debt.sh` hiện là **184**, chỉ được giảm | `CLAUDE.md` §2 |
| 0 trong 59 definition mang stamp rộng hơn 21 byte | `crates/session/src/clock.rs:33-36` |

## Cách làm

Một luật thay cho hai bảng bốn hằng số. Độ dài hợp lệ là **17** (không phần lẻ) và **19..=30**
(dấu `.` cộng 1–12 chữ số). **18 bị từ chối** — ADR-0058 quyết định 2.

`[sửa 2026-09-09, sau khảo sát]` Cận trên là **30, không phải 27**. QuickFIX/J nhận 30 byte —
pico giây — và Technical Addendum của FIX mở rộng kiểu timestamp tới pico giây. Engine này từ
chối 30 im lặng y hệt cách nó từ chối 19, nên bỏ nó ra ngoài là đóng sáu ca rồi để nguyên ca thứ
bảy. **Item 59 đếm sáu; thật ra là bảy.**

1. **`crates/session/src/clock.rs`** — `parse_utc`: thay `match s.len()` bằng phép suy ra số chữ
   số phần lẻ từ độ dài, rồi đọc **tối đa 3 chữ số đầu và đệm 0 sang phải** để ra mili giây; các
   chữ số còn lại vẫn phải là chữ số nhưng bị bỏ. Không cấp phát, không `format!`, không index
   trần — dùng `get`/slice pattern như nửa A đã làm.
2. **`crates/dict/src/field_type.rs`** — `fn time`: thay `matches!(v.len(), 8 | 12 | 15 | 18)`
   bằng `v.len() == 8 || (10..=21).contains(&v.len())`, giữ nguyên phần kiểm `.` và chữ số.
   (`time` nhận `&value[9..]`, nên tổng 17 → 8, tổng 19 → 10, tổng 30 → **21**.) Đây là nửa thứ
   hai và **phải đi cùng commit** với bước 1.
3. **Test cho cả hai reader, và một test bắt chúng phải đồng ý** — bảng 0..=12 chạy qua cả
   `parse_utc` lẫn `FieldType::UtcTimestamp::accepts`, khẳng định hai bên cho cùng một câu trả lời
   ở cả mười ba độ chính xác cộng với 18 byte.
4. **`scripts/interop.sh` scenario 4i** — đầu C++ ở `TimestampPrecision=2`, đầu này để mặc định.
   Khẳng định: session **logon được**, không có `35=3` nào nhắc tag 52, và có ít nhất một `52=`
   20 byte đi từ libquickfix sang. Dựng theo đúng khuôn 4h, kể cả `--dump-tape` và việc đếm cái
   không có mặt.
5. **Docs**, cùng commit: `docs/SESSION-BEHAVIOUR.md` (độ rộng nào nhận, 18 byte là khác biệt cố
   ý), `docs/CONFORMANCE.md` (kết quả scenario mới, kèm lệnh + máy + CI run id), `CHANGELOG.md`,
   `STATUS.md` (gạch item 59 và rà mục *Not proven*).

File tạo mới: không có. File sửa: 2 file `src`, 2 file test, 1 script, 4 doc.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **1 — không cấp phát** | `parse_utc` nằm trên đường nhận của mọi message | `benches/alloc.rs` chạy lại; không có `String`, không `format!`, không `Vec` trong diff |
| **2 — session thuần khiết** | chỉ sửa một hàm thuần trong `session` | không thêm socket, clock hay lỗi có trường |
| **3 — 59 definition là gate** | đổi cách đọc `52=` | chạy lại corpus, phải là **59 / 59**; và ghi rõ rằng 59/59 ở đây chỉ nói *không hỏng gì*, không nói *cái mới chạy* |
| **7 — không panic/unwrap** | thêm nhánh trên độ dài | `clippy -D warnings`; `check-indexing-debt.sh` phải **≤ 184**, không được tăng |
| 5 — thứ tự field | không đụng | — |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 0 | ADR-0058 được duyệt | — |
| 1 | Test đỏ trước: bảng 13 độ chính xác + 18 byte, chạy trên code hôm nay, **đỏ ở 7 dòng** (sáu độ rộng dưới 27, cộng 30) | 0 |
| 2 | `parse_utc` nhận 17 và 19..=30, đệm trái; test bước 1 xanh nửa session | 1 |
| 3 | `dict::time` nhận 8 và 10..=21; test đồng-ý-giữa-hai-reader xanh | 2 |
| 4 | `scripts/interop.sh` 4i, chạy đỏ trước trên code cũ (đảo ngược), xanh sau | 3 |
| 5 | Docs + `STATUS.md` + `CHANGELOG.md`, cùng commit với code tương ứng | 2–4 |

## Cách kiểm chứng

- **Bước 1 phải đỏ trước.** Chạy `cargo test -p fixbolt-session -p fixbolt-dict` trên code chưa
  sửa và **trích nguyên output**, không tóm tắt. Sáu dòng phải đỏ, và phải đỏ **đúng ở khẳng định
  về độ rộng**, không phải vì test viết sai.
- **Đảo ngược cho bước 4** — đây là gate thật của item 59. Trả `parse_utc` về 4 độ rộng cũ, chạy
  `scripts/interop.sh`, scenario 4i phải **đỏ ở dòng *never logged on***. Nếu nó đỏ ở dòng khác
  thì gate đang canh nhầm thứ.
- `cargo test --all` và `cargo test --all --no-default-features` — **đọc output, không đọc exit
  code**, và không đi qua `tail` (bài học `[measured 2026-09-08]`: exit status của pipeline là của
  lệnh cuối, `tail` báo 0 trong khi `cargo` chưa từng chạy).
- 59 definition: **59 / 59**.
- `cargo bench -p fixbolt-session` phần parse, trước và sau, cùng máy — ADR-0058 phần *Bad* nợ con
  số này. Máy này **không phải máy §9** nên số ghi kèm tên máy và không thêm hàng
  `benches/baselines.tsv`.
- `scripts/check-indexing-debt.sh`, `scripts/check-no-optional-deps.sh`, `scripts/check-links.py`,
  `cargo fmt`, `cargo clippy --all-targets -- -D warnings`.
- **CI xanh trên đúng commit đóng plan**, nêu run id — §9 hộp cuối.

## Tài liệu phải cập nhật

- [ ] `docs/SESSION-BEHAVIOUR.md` — độ rộng `52=` được nhận, nêu test canh nó; 18 byte là khác biệt cố ý so với QuickFIX
- [ ] `docs/CONFORMANCE.md` — scenario 4i, kèm lệnh, máy, CI run id
- [ ] `CHANGELOG.md` — hành vi nhận rộng hơn
- [ ] `STATUS.md` — gạch item 59; rà lại mục *Not proven* theo hàng cuối bảng §4
- [ ] `docs/decisions/ADR-0058-*.md` — `Proposed` → `Accepted` khi duyệt
- [ ] `docs/reference/one-field-two-readers.md` — thêm một câu: luật một-dòng đã thay bảng bốn hằng số, nên hình dạng gây ra lỗi đó không còn tồn tại
- [ ] `docs/CONFIGURATION.md` — **chỉ khi** `TimestampPrecision` đổi giá trị hợp lệ. Plan này **không** đổi.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| **`.1` đọc thành 1 ms thay vì 100 ms.** Sai 99 ms, và **không gate nào ở đây nhìn thấy** vì mọi skew đo với ngưỡng 120 000 ms | `a_single_fractional_digit_is_a_tenth_of_a_second` — `.1` → 100, `.12` → 120, `.1239` → 123 |
| 18 byte lọt vào vì viết `17..=27` cho gọn | `a_dot_with_no_digits_is_not_a_timestamp`, chạy trên **cả hai** reader |
| Chỉ sửa một reader — đúng lỗi ngày 2026-09-08 | `both_readers_agree_on_every_precision`, bảng 0..=9 + 18 byte, một test hỏi cả hai |
| `fn time` dùng chung với `UtcTimeOnly`, nới rộng lan sang type khác mà không ai nói | `a_utc_time_only_takes_the_same_widths` — nêu rõ đây là chủ ý, ADR-0058 phần *Good* |
| Chữ số phần lẻ quá 3 không còn bị kiểm là chữ số | `20260909-10:00:00.123abcd` phải là `None` ở mọi độ dài |
| **Quên 30 byte** vì item 59 chỉ đếm sáu | bảng test chạy tới độ chính xác **12**, không dừng ở 9 |
| `59/59` bị đọc như bằng chứng cái mới chạy | plan này nói trước: corpus không mang stamp > 21 byte, gate thật là 4i |
| Số indexing tăng vì nhánh mới | `check-indexing-debt.sh` ≤ 184 |
| Scenario 4i xanh vì lý do khác | đảo ngược ở phần *Cách kiểm chứng*, và phải đỏ **đúng dòng** |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Nhánh mới trên đường nhận làm chậm parse | Thấp | đo trước/sau cùng máy; nếu tệ thì giữ nhánh nhanh cho 17/21/24/27 và nhánh chậm cho phần còn lại — nhưng **chỉ khi số đo bắt phải làm vậy** |
| `libquickfix` trong CI không nhận `TimestampPrecision=2` như đọc trong source | Thấp | scenario 4i đếm stamp 20 byte đi ra từ nó; nếu đếm 0 thì gate báo *oracle chưa được cấu hình*, không báo pass |
| Nới rộng làm hỏng một `.def` đang dựa vào việc từ chối | Rất thấp | corpus không có stamp > 21 byte; 59/59 sẽ nói ngay |

## Ngoài phạm vi

- **Không** đổi phía gửi. `TimestampPrecision` vẫn nhận 3, 6, 9 — ADR-0057 quyết định 1.
- **Không** nhận 18 byte, dù QuickFIX nhận.
- **Không** giữ lại phần lẻ dưới mili giây ở phía nhận. D13 nói `Tick` là mili giây; giữ lại là
  một ADR khác.
- **Không** đo trên máy §9.

## Nhật ký giao hàng

**2026-09-09 — đóng, cả năm bước.** ADR-0058 `Accepted` sau khi chủ dự án yêu cầu khảo sát các
engine khác thay vì duyệt bản đầu, và **khảo sát đó đổi quyết định hai lần**: cận trên đi từ 9 lên
12 chữ số, và hoá ra họ QuickFIX không thống nhất với nhau (C++ và .NET lấy khoảng; J và Go lấy
bảng liệt kê; nanofix không đọc field này bao giờ). Ghi ở
[prior-art.md](../reference/prior-art.md).

**Gate, trích nguyên chứ không tóm tắt:**

- Bước 1 **đỏ trước**, đúng khẳng định về độ rộng: `parse_utc refused 1 fractional digits:
  20260909-10:00:00.1 (19 bytes)`, và `.1` mong đợi `Some(63956167200100)` nhận về `None`.
  Bốn test còn lại trong file **xanh ngay từ đầu** — trong đó có `both_readers_agree_on_every_width`,
  và việc nó xanh trên code cũ là một dữ kiện chứ không phải may: hai reader lúc đó **đồng ý với
  nhau trên một tập quá hẹp**. Một test nhất-quán không phải test đúng-sai.
- `cargo test --all` **613 → 619, 0 failed**; `--no-default-features` **608 → 614, 0 failed**.
  Đọc từ file output, không đi qua `tail`.
- 59 / 59 (`a_session_that_answers_correctly_scores_fifty_nine`).
- `scripts/interop.sh`: **7/7 + 8/8 + 6/6 + 6/6 + 6/6 + 9/9 + 5/5 + 3/3**, `the run added nothing
  git can see`, không dòng lỗi shell nào trong toàn bộ run.
- §4i: `20-byte 52= from libquickfix — 10`, `35=3 naming tag 52 — 0`, `PASS 3/3`.
- **Đảo ngược**: trả `parse_utc` về bốn độ rộng, để `dict` nguyên → đỏ **đúng một dòng**,
  `the odd-precision session never logged on — a valid 52= was refused before the Logon, in
  silence`; hai khẳng định kia vẫn xanh, và số stamp **10 → 97** vì đầu C++ nối lại liên tục.
- `fmt`, `clippy -D warnings`, `check-indexing-debt.sh` **184 / trần 184**,
  `check-no-optional-deps.sh`, `check-links.py`, `benches/alloc.rs` 17 case đều 0.

**Ba test có sẵn bị sửa, nói rõ chứ không giấu.** Sáu khẳng định trong
`crates/dict/tests/field_types.rs` và unit test của `clock.rs` nói 22, 23, 25, 26, 28, 29 byte
**không** phải timestamp. Mỗi cái là phát biểu trực tiếp của chính sách mà ADR-0058 đảo — không
phải fixture bị bẻ cho code mới chạy. Mỗi chỗ sửa mang ghi chú `[amended 2026-09-09, ADR-0058]`
nói nó từng khẳng định gì và vì sao đổi, và **cái chúng thật sự canh vẫn còn được canh**: biên 18
byte, biên 31 byte, và đuôi không phải chữ số.

**Một comment cũ được sửa nhân tiện, và nó sai từ trước việc này.** `clock.rs` vẫn viết *"This
engine still writes 21 bytes; the send half is ADR-0057 and is not built"* — nửa B đã dựng nó hôm
trước.

**Chưa làm, nói thẳng:**

- **Không có số §9.** ADR-0058 phần *Bad* nợ một phép đo trước/sau cho nhánh mới trên đường nhận;
  máy này còn hai lần reboot nữa mới về §9, nên không có hàng nào thêm vào
  `benches/baselines.tsv`.
- **`cmake` không có trên desk này**, nên `scripts/interop.sh` chạy với một cmake 3.31.6 tải vào
  scratchpad, ngoài repo. CI vẫn là nơi có thẩm quyền cho job đó.
- **CI chưa được đọc cho commit này** — §9 hộp cuối còn mở cho tới lúc đó.
