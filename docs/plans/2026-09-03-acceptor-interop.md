# Acceptor trước một `libquickfix` thật

> **Loại:** Plan · **Ngày:** 2026-09-03 · **Trạng thái:** **ĐÓNG 2026-09-04** (bước 1–4 / 5), đã sửa hai lần
> **Phạm vi:** `STATUS.md` item 42. Chạm `tools/interop`, `scripts/interop.sh`,
> `.github/workflows/ci.yml`. **Không chạm** `codec`, `dict`, `session`, `engine`, `library` —
> trừ khi gate mới tìm ra lỗi, khi đó lỗi đó có plan sửa riêng (xem *Bẫy* cuối bảng).
>
> **Máy chạy:** macOS đủ để viết và chạy thử (cần `cmake`, `g++`). Gate chính thức là CI job
> `interop` trên `ubuntu-latest`. Không cần máy §9.
>
> **Thời lượng dự kiến:** 1 ngày cho bước 1–4. Bước 5 là tuỳ chọn, thêm nửa ngày.

## Sửa đổi `[2026-09-04]` — self-check không thể ra 7/7, và lý do đáng ghi

**Bản đầu của bước 1 yêu cầu self-check fixbolt-với-fixbolt phải in `interop: PASS 7/7`. Nó
không thể.** Chạy thử ngay khi vai acceptor build xong:

```
interop: fixbolt acceptor on 127.0.0.1:15699, 1 counterparties
interop: listening on 127.0.0.1:15699
interop: logon        FAIL  |8=FIX.4.4|9=67|35=A|34=1|49=FIXACC|52=…|56=FIXINI|98=0|108=30|10=123|
interop: FAIL 0/1
```

Hai nguyên nhân, khác hẳn nhau:

1. **Nhỏ, sửa được:** vai initiator so `|49=QFACC|` cứng trong code thay vì đọc `--target`. Sửa
   một dòng, và bản thân nó là một gate đọc sai thứ nó tưởng đang đọc.
2. **Không sửa được trong plan này:** bước `news` và `resend` cần counterparty **tự gửi** hai
   `35=B` khi logon. `Handler::on_message` chỉ *trả lời* một message (`crates/library/src/app.rs:117`),
   và `Admin::Command` chỉ có `SetNextOut`, `SetNextIn`, `SendSequenceReset`
   (`crates/engine/src/observe.rs:649`). **Không có đường nào cho application chủ động gửi.**
   Một acceptor chỉ biết trả lời không gửi được `35=8` khi lệnh khớp muộn, không gửi được
   quote, không gửi được `35=j`. Đó là lỗ hổng sản phẩm, lớn hơn cái self-check này, và nó đi
   ra `docs/reference/` + một open item mới — **không sửa ở đây**.

**Bước 1 đổi thành:** self-check đòi năm bước `logon`, `heartbeat`, `testrequest`, `gapfill`,
`logout` in `ok`; `news` và `resend` **được phép đỏ**, và lý do chúng đỏ được ghi lại. Đây vẫn
là kiểm tra lắp ráp, không phải gate. Bảng *Chia việc* và mục *Cách kiểm chứng* bên dưới đã
mang bản sửa này.

## Sửa đổi 2 `[2026-09-04]` — bước 6 hỏi một câu mà gap fill được phép xoá

**Bản đầu của bước 6 đòi `35=0 112=QF-TR-2` quay về, và nó không thể.** Lần chạy **đầu tiên** của
`initiator.cpp` ra `FAIL 6/7`, và bước đỏ là hành vi **đúng ở cả hai phía**:

```
out 35=1 34=10 112=QF-TR-2               TestRequest gây gap
in  35=2 34=6  7=7 16=0                  fixbolt hỏi lại từ 7 — đúng
out 35=4 34=7 43=Y 36=11 123=Y           QuickFIX gap fill 7 ĐẾN 10
```

`36=11` phủ luôn số 10, tức chính cái `TestRequest` đó. QuickFIX tự bảo đối phương bỏ qua câu
hỏi mà bài test đang chờ trả lời; fixbolt vứt message đã queue ở 34=10 là điều **duy nhất đúng**.
Transcript sau đó cho thấy session vẫn sống: heartbeat hai chiều thêm 8 giây, logout sạch.

**Bước 6 đổi thành:** message gây gap là `112=QF-TR-2` và **coi như đã tiêu**; sống sót được
chứng minh bằng một `TestRequest` **mới**, `112=QF-TR-3`, gửi *sau* khi gap fill xong. Hai khẳng
định, không cái nào bị recovery thu hồi được. Dòng kỳ vọng của bước 6 ở mục *Cách kiểm chứng* đổi
theo. Ghi lại ở [a-gap-fill-can-swallow-the-question](../reference/a-gap-fill-can-swallow-the-question.md).

## Bối cảnh

fixbolt định vị là **acceptor** nhanh nhất trên kernel TCP. Thế nhưng tính đến 2026-09-03, thứ
duy nhất từng nói acceptor này đúng là 59 file `.def` của QuickFIX — **do chính runner của repo
này diễn giải**. Chưa có một implementation FIX nào khác từng kết nối vào acceptor này và nói
"đúng".

Chiều ngược lại thì đã có: `scripts/interop.sh` lái **initiator** của fixbolt vào một acceptor
`libquickfix` thật, 7 / 7, chặn CI (ADR-0042). Và lần chạy **đầu tiên** của gate đó tìm ra một
lỗi mà sáu gate xanh không thấy — initiator trả lời `Logon` bằng một `Logon`
([a-role-can-be-wrong-in-a-direction-no-gate-runs](../reference/a-role-can-be-wrong-in-a-direction-no-gate-runs.md)).
Cùng một lý lẽ áp cho acceptor: **59 / 59 là đồng ý với một oracle mà mình tự đọc; một
implementation thứ hai là ý kiến độc lập duy nhất** (ADR-0042 nói đúng câu này, và hiện chỉ áp
cho một nửa engine).

Kết quả muốn đạt: một script và một CI job lái `libquickfix` **initiator** vào acceptor của
fixbolt qua kernel TCP, đi 7 bước giống chiều kia, đọc transcript chứ không đọc exit code, và
chặn CI.

## Những gì đã biết chắc

| Sự thật | Nguồn |
|---|---|
| Chiều initiator đã có: `tools/interop/src/main.rs` (450 dòng, 7 bước, in `interop: <bước> ok/FAIL` và `interop: PASS n/n`), `tools/interop/acceptor.cpp` (124 dòng, QuickFIX `SocketAcceptor`, gửi 2 `35=B` khi logon, in mọi message), `scripts/interop.sh` (175 dòng: build QuickFIX tại `PINNED_SHA`, build C++, chạy, grep từng bước) | các file đó |
| CI job `interop` là **blocking**, `ubuntu-latest`, publish cả hai transcript lên run page | `.github/workflows/ci.yml:298–338` |
| Script đọc **dòng output**, không đọc `$?`; kiểm tra `git status` không đổi sau khi chạy (không để gì của QuickFIX lọt vào repo) | `scripts/interop.sh` mục 4 và 5 |
| Cổng vào acceptor của thư viện: `fixbolt::serve(addr, table, app, capacity, limits)` — mode `standard`, chỉ tồn tại sau feature `standard` trên unix | `crates/engine/src/lib.rs:1031`, `crates/library/src/lib.rs:38` |
| Ví dụ acceptor hoàn chỉnh một trang, đọc `Settings::load(cfg).into_table()` | `crates/library/examples/acceptor.rs` |
| `Handler::on_message(&mut self, msg: &Incoming<N>, reply: Reply<P, S>) -> Answer`; `Reply` tự sắp thứ tự field theo dictionary (bất biến 5) | `crates/library/src/app.rs:112` |
| Key file cấu hình của fixbolt: `BeginString`, `SenderCompID`, `TargetCompID`, `HeartBtInt`, `MaxSkewMillis`, `StartTime`, `EndTime`, `StartDay`, `EndDay`, `Weekdays`. Key lạ là lỗi | `docs/CONFIGURATION.md` §1, ADR-0040 |
| Acceptor tự gửi `ResendRequest` với `16=0` khi thấy gap | `crates/session/src/lib.rs:1340` |
| Acceptor trả lời `ResendRequest` bằng replay từ journal, còn lại gap fill; `Store` mặc định giữ **8** message | `crates/session/src/lib.rs:2097–2130`, `crates/engine/src/journal.rs:40` |
| **Feature không tách theo crate trong một lần `cargo`**: `tools/w2w` từng bật lại `libc` cho cả workspace. Mỗi tool có `[features]` riêng, và `scripts/check-no-optional-deps.sh` kiểm tra từng crate | [feature-flags-unify-across-a-workspace](../reference/feature-flags-unify-across-a-workspace.md) |
| `tools/interop` hiện chỉ phụ thuộc `fixbolt-session`, **không có** `[features]` | `tools/interop/Cargo.toml` |
| QuickFIX **không đưa** message `43=Y` có số thứ tự đã thấy vào `fromApp` — nó coi là bản trùng và bỏ qua trong session layer | hành vi `Session::verify` của QuickFIX; xem *Bẫy* 1 |
| Một reversal của gate cũ từng **vô hiệu** vì bước resend chỉ kiểm "có `43=Y`" thay vì kiểm đúng số `34=` | [a-resend-answer-has-two-legal-shapes](../reference/a-resend-answer-has-two-legal-shapes.md) |

## Cách làm

**Phía Rust: `tools/interop` có thêm vai acceptor.** Không dùng `examples/acceptor.rs` của
`library` làm bên bị test, vì tool không được phụ thuộc vào hình dạng một ví dụ. Thêm vào
`tools/interop`:

- `Cargo.toml`: phụ thuộc thêm `fixbolt = { path = "../../crates/library", default-features = false }`
  và một khối `[features]` riêng: `standard = ["fixbolt/standard"]`, `default = ["standard"]`.
  Lý do: `serve` chỉ có sau `standard`, và một tool bật feature của thư viện mà không tự khai
  báo là bẫy `w2w` đã trả giá.
- `src/main.rs`: cờ `--role initiator | acceptor`. Vai `initiator` là code hiện có, không đổi
  một dòng. Vai `acceptor` nằm sau `#[cfg(feature = "standard")]` (bất biến 6: `cfg` trên item),
  nhận `--listen 127.0.0.1:PORT --cfg <file>`, gọi `fixbolt::serve(addr, table, fixbolt::app(Desk), 4, Limits::new(4, 10_000)?)`.
- `src/desk.rs`: handler `Desk` cho vai acceptor. Nhận `35=D` NewOrderSingle, trả `35=8`
  ExecutionReport với đủ field bắt buộc của FIX44.xml: `37` OrderID, `17` ExecID, `150=0`,
  `39=0`, `55`, `54`, `151` LeavesQty, `14=0`, `6=0`, và **echo `11=` ClOrdID** để bên kia ghép
  được câu hỏi với câu trả lời. Mọi message khác: `reply.silent()`.
- Acceptor **không tự dừng**: script kill nó bằng `SIGTERM` sau khi initiator thoát. Journal là
  `Store` trong bộ nhớ, không có gì phải đóng gọn. (Ordered shutdown từ tool là ngoài phạm vi.)

**Phía C++: `tools/interop/initiator.cpp`.** QuickFIX `SocketInitiator`, cùng phong cách
`acceptor.cpp`: code của repo này, chỉ gọi API public của QuickFIX (bất biến 9). Hai phần:

1. **Một `FIX::Log` tự viết** (`RawLog`) ghi **chuỗi thô** của `onIncoming` / `onOutgoing` vào
   một `std::vector<std::string>` có mutex. Mọi bước **chấm điểm trên chuỗi thô**, không trên
   `fromApp` / `fromAdmin`, vì QuickFIX nuốt bản `43=Y` trước khi tới application (bẫy 1).
2. **Bảy bước, mỗi bước có deadline**, poll vector mỗi 10 ms, in
   `interop-acceptor: <bước> ok|FAIL  <đã thấy gì>` rồi cuối cùng `interop-acceptor: PASS n/7`:

| # | Bước | Initiator làm gì | Đạt khi thấy gì trên dây (thô) | Deadline |
|---|---|---|---|---|
| 1 | `logon` | `SocketInitiator::start()`; `ResetOnLogon=Y` nên Logon mang `141=Y` | một `35=A` đến, có `49=FIXBOLT`, `56=QFINI`, `141=Y` | 5 s |
| 2 | `order` | gửi 2 `35=D` với `11=QF-ORD-1`, `QF-ORD-2` | 2 `35=8` đến, mỗi cái đúng `11=`; **ghi lại `34=` của từng cái** cho bước 5 | 5 s |
| 3 | `heartbeat` | không làm gì | một `35=0` đến **không có `112=`**, trước khi hết `2 × HeartBtInt + 1` giây. `HeartBtInt` lấy từ **`108=` trong `35=A` fixbolt trả về ở bước 1**, không lấy từ file cấu hình của initiator (bẫy 2) | tính từ 108= |
| 4 | `testrequest` | `Session::sendToTarget(TestRequest 112=QF-TR-1)` | `35=0` đến, có `112=QF-TR-1` | 5 s |
| 5 | `resend` | `Session::sendToTarget(ResendRequest 7=a 16=b)` với `a`, `b` là hai số `34=` ghi ở bước 2 | **đúng hai** `35=8` đến với `43=Y`, `122=`, và `34=a`, `34=b`; không phải "có gì đó `43=Y`" | 5 s |
| 6 | `gapfill` | `Session::lookupSession(id)->setNextSenderMsgSeqNum(n + 3)` rồi gửi TestRequest `112=QF-TR-2`, và **sau khi thấy `35=2`, gửi tiếp `112=QF-TR-3`** `[sửa 2026-09-04]` | fixbolt gửi `35=2` với `7=<số nó chờ>` và `16=0`; QuickFIX tự trả `35=4 123=Y` — **và gap fill đó nuốt luôn `QF-TR-2`**, nên sống sót đo bằng `35=0 112=QF-TR-3`, một câu recovery không thu hồi được | 8 s |
| 7 | `logout` | `Session::logout()` | `35=5` đến | 5 s |

Cấu hình initiator (script sinh ra trong `vendor/interop-run/`): `ConnectionType=initiator`,
`SocketConnectHost=127.0.0.1`, `SocketConnectPort=${PORT2}`, `HeartBtInt=2`,
`ReconnectInterval=1`, `ResetOnLogon=Y`, `UseDataDictionary=Y`, `DataDictionary=<spec>/FIX44.xml`,
`FileStorePath`, `SenderCompID=QFINI`, `TargetCompID=FIXBOLT`, `BeginString=FIX.4.4`.
Cấu hình fixbolt: `[DEFAULT] BeginString=FIX.4.4  SenderCompID=FIXBOLT  HeartBtInt=2`,
`[SESSION] TargetCompID=QFINI`.

**Script: mở rộng `scripts/interop.sh`, không viết script thứ hai.** Build `libquickfix` một
lần rồi chạy **cả hai chiều** trong cùng một lần gọi; hai script là hai `PINNED_SHA` sẽ trôi.
Phần thêm vào sau mục 4 hiện có:

- mục 4b: build `initiator.cpp`; `cargo build -p fixbolt-interop`; chạy
  `target/debug/interop --role acceptor --listen 127.0.0.1:${PORT2} --cfg …` nền, chờ dòng
  `interop: listening` (không `sleep`); chạy `${WORK}/initiator … | tee interop-acceptor.log`;
  kill acceptor.
- mục 4c: grep **từng** dòng `^interop-acceptor: <bước> +ok` cho 7 tên, **và** dòng
  `^interop-acceptor: PASS 7/7`. Thiếu một là fail, in cả hai log ra stderr.
- Cổng: `PORT2=${INTEROP_PORT2:-15645}`, khác cổng chiều kia để hai acceptor không giẫm nhau.
- Dòng tổng kết cuối: `interop: 7 / 7 + 7 / 7 against libquickfix @ ${PINNED_SHA}`.

**CI:** job `interop` giữ tên, đổi `name:` thành *Both roles, against a real libquickfix*, thêm
transcript thứ ba và thứ tư lên `$GITHUB_STEP_SUMMARY` (*This engine's acceptor* / *What the
initiator saw*). Vẫn blocking.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **6 — feature gate trên `mod`, không toolchain ngoài** | `tools/interop` bật `fixbolt/standard` | `[features]` riêng của tool; vai acceptor sau `#[cfg(feature = "standard")]`; `scripts/check-no-optional-deps.sh` phải xanh **từng crate**; C++ vẫn chỉ ở `scripts/` và CI, không `Cargo.toml` nào nhắc tới |
| **9 — không copy QuickFIX** | thêm một file C++ gọi API QuickFIX | cùng header comment như `acceptor.cpp`; mục 5 của script kiểm `git status` không đổi |
| 1, 2, 4, 5, 7 | **không đụng** — plan này không sửa crate nào | nếu gate tìm ra lỗi trong `session`/`engine`, lỗi đó **có plan riêng** và đi qua 59/59 + `benches/alloc.rs` như mọi thay đổi session. Không sửa "tiện tay" trong branch này |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | `tools/interop --role acceptor` chạy được, `Desk` trả `35=8` đúng field. **Tự kiểm bằng chính fixbolt** `[sửa 2026-09-04]`: chạy `--role acceptor` rồi `--role initiator --connect` vào nó, phải thấy `logon`, `heartbeat`, `testrequest`, `gapfill`, `logout` **ok**; `news` và `resend` đỏ vì library không gửi chủ động được (xem *Sửa đổi* trên). Kiểm tra lắp ráp, **không phải** gate. `check-no-optional-deps.sh` xanh, có case mới cho `fixbolt-interop` | — |
| 2 | `initiator.cpp` với `RawLog` và 7 bước; build tay bằng `g++` như mục 2 của script; chạy tay vào acceptor bước 1 và đọc 7 dòng | 1 |
| 3 | `scripts/interop.sh` chạy cả hai chiều, grep 14 dòng bước + 2 dòng PASS; **ba reversal** ở mục *Cách kiểm chứng* được chạy và output dán vào nhật ký giao hàng | 2 |
| 4 | CI job cập nhật, **CI xanh trên commit đóng plan, ghi run id**; docs theo bảng dưới; `STATUS.md` item 42 gạch, mục *Not proven* đọc lại từng dòng | 3 |
| 5 *(tuỳ chọn)* | Kịch bản `reconnect` cho **chiều initiator** (item 38): `--role initiator --scenario reconnect` dùng `connect_and_serve` với `Policy`, script kill và chạy lại acceptor C++ sau logon, chấm điểm: một `35=A` thứ hai trong `acceptor.log` và số `34=` **tiếp tục** (acceptor C++ chạy với `ResetOnLogon=N` cho kịch bản này). Tách ra plan riêng nếu bước 1–4 tìm ra lỗi cần sửa | 4 |

## Cách kiểm chứng

Lệnh và dòng output coi là đạt:

```
scripts/interop.sh
# … phải in, theo thứ tự:
interop: PASS 7/7
interop-acceptor: logon        ok  35=A 49=FIXBOLT 56=QFINI 141=Y
interop-acceptor: order        ok  35=8 at 34=2 and 34=3, 11= matched
interop-acceptor: heartbeat    ok  35=0 without 112= within 5 s
interop-acceptor: testrequest  ok  35=0 112=QF-TR-1
interop-acceptor: resend       ok  35=8 43=Y replayed at 34=[2, 3], wanted [2, 3]
interop-acceptor: gapfill      ok  35=2 7=… 16=0 in: yes, then 35=0 112=QF-TR-3: yes
interop-acceptor: logout       ok  35=5
interop-acceptor: PASS 7/7
==> the run added nothing git can see
interop: 7 / 7 + 7 / 7 against libquickfix @ 386ce46e…
```

**Ba reversal, mỗi cái phải đỏ đúng bước, rồi khôi phục và xanh lại. Dán output cả hai lần.**

| Reversal | Sửa gì (tạm, trong working tree) | Phải thấy |
|---|---|---|
| A | Trong `Desk`, bỏ echo `11=` khỏi ExecutionReport | `order FAIL`, và **chỉ** `order` (bước 5 có thể FAIL theo vì không có số `34=` để hỏi — ghi nhận, không tính là lỗi reversal) |
| B | Trong script, đổi `7=a 16=b` thành `7=b 16=a` (range ngược) như reversal của gate cũ | `resend FAIL` với dòng "replayed at 34=[], wanted [2, 3]" — QuickFIX **không** gửi range ngược, hoặc fixbolt trả gap fill; cả hai đều phải đỏ. Đây là reversal đã từng vô hiệu ở chiều kia, nên bắt buộc chạy |
| C | Trong script, đổi tên bước grep `gapfill` thành `gapfil` | script fail với `MISSING OR FAILED STEP: gapfil` dù binary in `PASS 7/7` — chứng minh grep từng bước là load-bearing chứ không chỉ dòng PASS |

Sau khi khôi phục: `git diff --stat` **rỗng** ngoài các file plan này chạm, rồi chạy lại xanh.

Gate thường lệ vẫn chạy vì `Cargo.toml` của workspace đổi: `cargo test --all`,
`cargo test --all --no-default-features`, `scripts/check-no-optional-deps.sh`, `cargo clippy
--all-targets -- -D warnings`, `cargo fmt --check`.

## Tài liệu phải cập nhật

Theo `CLAUDE.md` §4:

- [ ] `docs/CONFORMANCE.md` §1 hoặc mục mới: kết quả 7 / 7 chiều acceptor, **lệnh, máy (`ubuntu-latest`), CI run id**; §6 xoá/ sửa dòng "the initiator's independent check is narrow" thành phát biểu cho **cả hai vai**, vẫn nói rõ 7 case không phải 59
- [ ] `docs/DESIGN.md` §6: thêm hàng gate *The acceptor, against a real `libquickfix`*, 7 / 7, `scripts/interop.sh`; hàng cũ đổi tên cho rõ là chiều initiator
- [ ] `docs/PRD.md` §2 đoạn *What criterion 4 did NOT buy* (dòng ~167): cập nhật — criterion 4 giờ có cả hai chiều; giữ nguyên câu "7 case, không phải corpus thứ hai"
- [ ] `docs/decisions/ADR-0042`: **không sửa** (Accepted). Nếu cần ghi hệ quả mới, một ghi chú `[2026-09-xx]` ở cuối, không đổi nội dung quyết định
- [ ] `STATUS.md`: item 42 gạch; bảng plan; *Not proven* đọc lại từng dòng; nếu bước 5 làm, item 38
- [ ] `docs/reference/`: **chỉ khi** gate tìm ra lỗi hoặc một false green — mỗi cái một file, đánh dấu `[to testing-skills]` nếu là bài học về testing
- [ ] `tools/interop/src/main.rs` rustdoc đầu file: mô tả cả hai vai
- [ ] `README.md`: nếu có dòng nêu interop, thêm "both roles"

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| 1. QuickFIX **không đưa** bản replay `43=Y` (số đã thấy) vào `fromApp`; chấm điểm qua callback sẽ đỏ dù fixbolt đúng | `RawLog` chấm trên `onIncoming` thô. Reversal B chứng minh bước 5 nhìn thấy số `34=` |
| 2. Heartbeat: acceptor dùng `108=` trong Logon **của initiator**, còn deadline tính từ file cấu hình initiator sẽ lệch nếu hai bên khác nhau | bước 3 đọc `108=` từ `35=A` fixbolt trả về. Kiểm tay: đặt `HeartBtInt=2` initiator, `HeartBtInt=30` trong file fixbolt, bước 3 vẫn ok trong 5 s |
| 3. Feature unification: `fixbolt/standard` bật lên cho cả workspace qua `tools/interop` | `scripts/check-no-optional-deps.sh` từng crate; `cargo test --all --no-default-features` không kéo `libc` |
| 4. Một acceptor **treo** và một acceptor **từ chối** trông giống nhau khi chỉ chờ dòng output | mỗi bước có deadline riêng và in *đã thấy gì*; script in cả `acceptor.log` khi fail |
| 5. Gate xanh vì đo không có gì: binary chết trước khi in, hoặc in `PASS 7/7` mà thiếu bước | grep **từng** bước + dòng PASS (reversal C) |
| 6. Hai acceptor cùng cổng khi chạy hai chiều trong một script | `PORT2` riêng; `SocketReuseAddress=Y` phía QuickFIX; fixbolt bind lỗi thì in lỗi và script dừng ngay |
| 7. `Store` giữ 8 message: nếu heartbeat chen vào trước bước 5, hai ExecutionReport vẫn trong 8 slot gần nhất | bước 5 hỏi đúng số `34=` đã ghi; nếu FAIL vì gap fill, đó là **item 43** lộ ra qua gate này — ghi vào nhật ký, không sửa ở đây |
| 8. Gate tìm ra lỗi trong `session` và cám dỗ sửa ngay trong branch này | Quy tắc plan: lỗi có plan riêng, đi qua 59/59 và `benches/alloc.rs`. Branch này chỉ chứa tool, script, CI, docs |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Bước heartbeat flaky trên runner CI chậm | trung bình | deadline `2 × 108 + 1` s là hằng từ dây, không từ máy; nếu vẫn flaky, **đo** số lần trên 20 run rồi mới nới, và ghi số đó |
| Viết judge bằng C++ tốn hơn dự kiến | trung bình | giữ `initiator.cpp` dưới ~250 dòng; judge chỉ là "chuỗi chứa"; không parse FIX bằng tay ở C++ |
| Gate tìm ra lỗi thật trong acceptor | **mong muốn** | dừng, ghi `docs/reference/`, mở plan sửa; plan này vẫn đóng khi gate đỏ đúng chỗ và được ghi là đỏ. Một gate mới có quyền đỏ trên `main` một thời gian ngắn **có tên item** — không được đóng plan bằng cách nới gate |
| Runner mất thêm ~1 phút build C++ thứ hai | thấp | cùng job, `libquickfix.a` dùng chung; chỉ thêm một `g++` |

## Ngoài phạm vi

- `hft` mode: `serve_hft` quay 100% một core trên runner chia sẻ. Ba entry point `hft` không có
  gate nào đi qua vẫn là nợ (ghi ở `STATUS.md` *Where the work is*), không trả ở đây.
- TLS, shard, nhiều counterparty, session schedule: mỗi cái là một kịch bản riêng sau này.
- Mirror 59 `.def` qua QuickFIX: không — 7 bước là "ý kiến thứ hai", không phải corpus thứ hai
  (PRD §2 đã nói rõ).
- Ordered shutdown từ tool (`Admin::shutdown`): kill là đủ cho một tool test.
- Sửa lỗi tìm được: plan riêng.

## Nhật ký giao hàng

**Đóng 2026-09-04, bước 1–4. Bước 5 tuỳ chọn: KHÔNG làm, nên item 38 vẫn mở.**
Không file nào dưới `crates/` đổi.

### Kết quả

```
scripts/interop.sh
interop: PASS 7/7
interop-acceptor: logon        ok  35=A |49=FIXBOLT| |56=QFINI| 141=Y
interop-acceptor: order        ok  35=8 at 34=2 and 34=3, 11= matched
interop-acceptor: heartbeat    ok  35=0 without 112= within 5 s (108=2)
interop-acceptor: testrequest  ok  35=0 112=QF-TR-1
interop-acceptor: resend       ok  35=8 43=Y replayed at 34=[2, 3], wanted [2, 3]
interop-acceptor: gapfill      ok  35=2 7=7 16=0 in: yes, then 35=0 112=QF-TR-3: yes
interop-acceptor: logout       ok  35=5
interop-acceptor: PASS 7/7
==> the run added nothing git can see
interop: 7 / 7 + 7 / 7 against libquickfix @ 386ce46e...
```

**Acceptor đúng cả bảy bước ngay lần đầu**, kể cả hai chỗ dễ sai âm thầm: echo `141=Y`, và
replay **đúng hai số** thay vì gap fill đè lên.

### Ba reversal

| Reversal | Thấy gì |
|---|---|
| A — bỏ echo `11=` trong `Desk` | `order FAIL  35=8 for QF-ORD-1: no, for QF-ORD-2: no`; `resend FAIL` theo sau đúng như plan lường trước. **Chiều initiator vẫn `PASS 7/7`**, tức reversal chỉ chạm đúng thứ nó nhắm |
| B — đảo range resend (`--invert-resend`) | `resend FAIL  replayed at 34=[], wanted [2, 3]`. **Đây là reversal từng vô hiệu ở chiều kia**; ở đây nó cắn, vì bước 5 gọi tên hai số |
| C — đổi tên bước grep `gapfill` → `gapfil` | binary in `interop-acceptor: PASS 7/7` mà script vẫn fail `MISSING OR FAILED STEP: gapfil`. Grep từng bước là load-bearing |

Sau khôi phục: `desk.rs` giống hệt file đã commit, danh sách bước về `gapfill`, chạy lại xanh.

### Gate

| Lệnh | Kết quả |
|---|---|
| `scripts/interop.sh` | 7 / 7 + 7 / 7 |
| `scripts/check-links.py` | 1 231 link, 0 chết |
| `scripts/check-no-optional-deps.sh` | 3 crate (thêm `fixbolt-interop`), 6 check, ok |
| `cargo fmt --all --check` | sạch |
| `cargo clippy --all-targets --all-features -D warnings` | sạch |
| `cargo doc --workspace --no-deps` | sạch |
| `cargo test --all` | 446 passed, 0 failed |
| `cargo test --all --no-default-features` | 83 suite, 0 failed |

**CI xanh trên commit đóng plan**: run
[`33833427382`](https://github.com/tmthang86/fixbolt/actions/runs/33833427382), job
[`100900997589`](https://github.com/tmthang86/fixbolt/actions/runs/33833427382/job/100900997589),
commit `f94e36e`, **11 job / 11**, `ubuntu-latest`, cmake 3.31.6, g++ 13.3.0 — và **log của
chính job đó được đọc lại**, không phải kết luận của nó.

### Ba thứ tìm ra, không cái nào là lỗi của session layer

| Tìm ra | Ghi ở |
|---|---|
| Engine không gửi chủ động được message nào | STATUS item **46**, [an-acceptor-that-can-only-answer](../reference/an-acceptor-that-can-only-answer.md) |
| Gap fill có quyền nuốt chính câu hỏi mà test đang chờ | [a-gap-fill-can-swallow-the-question](../reference/a-gap-fill-can-swallow-the-question.md) |
| `interop` in `PASS 1/1` trên một kịch bản chạy 1/7 bước; và bước logon so `49=QFACC` cứng trong khi session dựng từ `--target` | [a-green-fraction-over-a-scenario-that-never-ran](../reference/a-green-fraction-over-a-scenario-that-never-ran.md) |

Cả ba mang dấu `[to testing-skills]`.

### Không làm, nói rõ

- **Bước 5** (kịch bản reconnect, item 38): tuỳ chọn, không làm.
- **`hft` mode**: `serve_hft` quay 100% một core; runner chia sẻ là chỗ sai để chạy. Ba entry
  point `hft` vẫn không có gate nào đi qua.
- **Sửa item 46**: đổi public API hai crate, cần ADR riêng, thuộc wave B.
