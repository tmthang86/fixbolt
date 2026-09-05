# Vòng nối lại, trước một `libquickfix` thật

> **Loại:** Plan · **Ngày:** 2026-09-05 · **Trạng thái:** **XONG 2026-09-05**, cả sáu bước, sửa ba lần
> **Phạm vi:** `STATUS.md` item 38. Chạm `tools/interop`, `scripts/interop.sh`,
> `.github/workflows/ci.yml`, và tài liệu. **Không chạm** `codec`, `dict`, `session`, `engine`,
> `library` — nếu gate mới tìm ra lỗi ở đó, lỗi đó có plan sửa riêng (xem *Bẫy* cuối bảng).
>
> **Máy chạy:** macOS hoặc Linux có `cmake`, `g++` là đủ để viết và chạy thử. Gate chính thức là
> CI job `interop` trên `ubuntu-latest`. **Không cần máy §9** — đây là gate về tính đúng, không
> phải về nano giây.
>
> **Thời lượng dự kiến:** nửa ngày cho bước 1–4, thêm nửa ngày cho bước 5 và tài liệu.

## Sửa đổi `[2026-09-05]` — hai điều mục *Đã biết chắc* nói sai, cả hai đo được

**Cả hai đều được phát hiện bằng cách chạy, không phải bằng cách đọc lại.** Bước 1 build xong,
chạy tay vào acceptor C++, và transcript nói ngược lại plan ở hai chỗ.

### Sửa 1 — `seq` trong `Application::on_message` là số **gửi ra**, không phải số nhận vào

Điểm 9 của *Những gì đã biết chắc* viết: *"`Application::on_message` nhận `seq`, nên tool in được
`34=` của từng application message đã giao tới ứng dụng"*. Sai. Rustdoc của chính trait đó nói:
*"the session emits it and spends `seq`"* — đó là số của **câu trả lời**, không phải của message
đến. `[đo 2026-09-05]` acceptor gửi News ở `34=2` và `34=3`; tool in:

```
interop-reconnect: delivered 34=2 35=B
interop-reconnect: delivered 34=2 35=B
```

Hai lần cùng `34=2`, vì fixbolt mới gửi Logon ở `34=1` nên `next_out` là 2, và `Watch` trả `None`
nên không tiêu số nào. **Một dòng log sai theo kiểu vẫn đọc được**: nó có đúng dạng, đúng số
lượng, và nói sai điều nó tự nhận là đang nói.

**Sửa:** đọc `34=` ra khỏi chính message, qua `Incoming::seq()` — cùng kỷ luật với bước `logon` của
vai initiator đọc `49=` khỏi dây thay vì so hằng số.

### Sửa 2 — journal chỉ giữ application message, nên `next_out` không suy ra được từ nó

Điểm 4 của *Những gì đã biết chắc* chép `next_out = highest() + 1` từ `on_disk.rs`. Trong test đó
session **có** gửi application message; ở đây `Watch` không gửi gì, nên `journal.highest()` là
`None` và `next_out` ra **1**. `session.put` chỉ được gọi trên đường application
(`crates/session/src/lib.rs:1871` và `:2481`).

`[đo 2026-09-05]` kịch bản 4d chạy với `Watch` không tự phát:

```
interop-reconnect: resuming next_out=1 next_in=4
acceptor: out 35=5 34=4 58=MsgSeqNum too low, expecting 2 but received 1
```

Lặp lại mười hai lần theo đúng thang backoff. **Đây là bằng chứng ngược đáng giá và nên đọc kỹ:**
vòng `connect_and_serve` chạy đúng — nó quay lại, nó leo thang, nó hỏi `Recovery` mỗi lần — còn thứ
sai là con số `Recovery` trả về. Và đối phương nói ra bằng lời của chính nó.

**Sửa:** `Watch` trở thành một `fixbolt::Handler` và tự phát **một** `35=B` trong `on_logon` — cửa
[ADR-0048](../decisions/ADR-0048-an-engine-that-can-speak-first-has-two-doors.md) vừa mở tuần này —
rồi gói bằng `fixbolt::app(...)`. Journal khi đó có `34=2`, `next_out` ra 3, và đó là số QuickFIX
đang chờ. Viết `35=B` bằng `Reply` chứ không bằng tay: bất biến 5, thứ tự field đến từ dictionary.

### Điều Sửa 2 để lộ, và nó lớn hơn plan này

**`highest() + 1` sai đúng bằng số message quản trị đã gửi sau application message cuối cùng.**
Rustdoc của `Resumed::next_out` đã lường trước — *"usually `journal.highest() + 1`, and usually is
why the engine does not compute it"* — nhưng không tài liệu nào nói người triển khai lấy con số
đúng ở đâu, và ví dụ duy nhất trong repo này (`on_disk.rs`) làm cách ngây thơ.

**Và cách duy nhất biết con số thật là `Observer` trên session đang sống, thứ `connect_and_serve`
không phát ra** — `STATUS.md` item 47. Nên item 38 và item 47 chạm nhau ở đây, có bằng chứng.

Hệ quả cho plan: kịch bản 4d sắp đặt sao cho cách ngây thơ **đúng** — `HeartBtInt=30`, và giết
acceptor ngay sau khi News được giao, nên không message quản trị nào chen vào giữa. Điều đó phải
nằm trong comment của code chứ không phải trong đầu người viết. **Kịch bản 4e thì không sắp đặt
được như thế**: fixbolt trả lời `35=5` cho lời chào của QuickFIX, tiêu một số mà journal không
ghi. Khẳng định `next_out` của 4e vì vậy **bị bỏ**, thay bằng những khẳng định 4e thật sự chứng
minh được; bảng bước 3 bên dưới đã mang bản sửa này. Phát hiện đi vào `docs/reference/` và một
open item mới.

## Sửa đổi 3 `[2026-09-05]` — 4e khẳng định ba điều nó chứng minh được, không phải năm điều plan mong

Sửa 2 dự đoán 4e không giữ được số thứ tự. **Chạy rồi mới biết dự đoán đúng, và đúng đến từng
con số:**

```
acceptor: out 35=5 34=4                      QuickFIX chào
acceptor: in  35=5 34=3                      fixbolt đáp — tiêu 34=3
acceptor: out 35=5 58=MsgSeqNum too low, expecting 4 but received 3
```

Thiếu **đúng một**, đúng bằng một message quản trị gửi sau application message cuối. Ghi ở
[a-journal-holds-messages-not-numbering](../reference/a-journal-holds-messages-not-numbering.md),
và thành `STATUS.md` item 48.

**4e đổi từ năm khẳng định xuống ba**, mỗi cái là điều nó thật sự chứng minh:

| Tên | Khẳng định |
|---|---|
| `goodbye` | `A1` có `out 35=5` **và** `in 35=5` — lời chào được đáp |
| `redialled` | một Logon tới được acceptor thứ hai sau lời chào sạch — ADR-0043 quyết định 5, do một engine chưa từng nghe tới ADR đó xác nhận |
| `known_gap` | `expecting N but received N-1` — **ghim đúng kích thước lỗ hổng**, kèm số item |

**`known_gap` cố tình ghim một khuyết điểm đã biết.** Ghim kích thước chứ không chỉ ghim "có
lỗi": một lỗ hổng kích thước khác nghĩa là lời giải thích sai. Và ngày item 48 được sửa, dòng này
đỏ — buộc người sửa phải quay lại đọc. Một giới hạn đã biết mà không có gì đọc nó thì mục *Not
proven* của repo này đã cho thấy nó mục theo hướng nào.

### Và `back` của 4e đọc sai chỗ, đây là bài học thứ hai

Bản đầu tìm dòng `acceptor: in` chứa `35=A` trong `A2`. **Nó `FAIL` với chữ *"nothing reached the
second acceptor"* trong khi Logon đã tới hai mươi hai lần.** QuickFIX từ chối message đó ở tầng
session, nên `fromAdmin` không bao giờ chạy và transcript dựng trên callback ứng dụng **không
nhìn thấy message bị từ chối** — đúng loại message mà một cổng về thất bại đang muốn thấy.
Sửa bằng cách hợp hai điểm quan sát: *được nhận* hoặc *bị từ chối*. Ghi ở
[a-refused-message-never-reaches-the-callback](../reference/a-refused-message-never-reaches-the-callback.md),
**`[to testing-skills]`**.

## Bối cảnh

Item 35 đóng ngày 2026-09-02: `fixbolt_engine::connect_and_serve` là vòng lặp cho một initiator
mất kết nối — rớt thì hỏi `reconnect::Policy` khi nào nối lại, nối lại thì hỏi `Recovery` số thứ
tự tiếp tục từ đâu ([ADR-0043](../decisions/ADR-0043-backoff-without-jitter-and-a-reconnect-asks-recovery-every-time.md)).
Nó có tám test thuần, hai test qua socket thật, một case đếm cấp phát. **Và toàn bộ số test đó là
cách đọc của chính dự án này.** ADR-0043 nói thẳng trong *Consequences*:

> Every test of this is invented. No corpus covers reconnect. A rule everybody would agree with
> but nobody wrote down here would pass, and only an interop scenario driving a real counterparty
> through a disconnect would close that — which `scripts/interop.sh` could grow and today does not.

Đó là item 38. Plan [acceptor-interop](2026-09-03-acceptor-interop.md) có bước 5 tuỳ chọn dành cho
nó và đã bỏ qua khi đóng. Plan này là bước 5 đó, viết lại thành plan riêng vì khi đọc kỹ code thì
nó **không phải** "thêm một bước thứ tám" như item 38 mô tả — xem điểm 1 của mục dưới.

Kết quả muốn có: một kịch bản trong `scripts/interop.sh` mà ở đó **acceptor `libquickfix` chết và
sống lại**, initiator của engine này tự nối lại, và **transcript của phía C++** — không phải
lời của fixbolt — cho thấy phiên tiếp tục đúng số thứ tự ở cả hai chiều. Chạy chặn trong CI như
hai chiều đã có.

## Những gì đã biết chắc

1. **Vai `--role initiator` hiện có không đi qua `connect_and_serve`.** `tools/interop/src/main.rs`
   dựng `Session<Initiator, 256>` và lái nó trực tiếp trên một `TcpStream` blocking; rustdoc đầu
   file ghi rõ *"not under test: the engine's polling loop"*. Nếu "bước thứ tám" thêm vào vai này,
   thứ được kiểm là vòng nối lại **của tool**, còn vòng lặp của engine vẫn chưa ai ngoài dự án
   nhìn thấy. Vậy phải là một vai mới.
2. **`connect_and_serve` là `standard`-only, và không trả về tay cầm nào.** Chữ ký ở
   `crates/engine/src/lib.rs:1589`; vòng `dial` chỉ trả về khi `Policy` nói `Stop` hoặc khi
   `shutdown_finished()` — mà `Policy` được truyền theo giá trị và không có `Observer`, `Admin`
   hay `Shutdown` handle nào ra ngoài (`STATUS.md` item 47). Nên tool **không tự dừng được**;
   script phải `kill` nó, y như đang làm với vai acceptor.
3. **`Recovery::recover` được hỏi ở mọi lần thử, và `NoRecovery` quay về `34=1`.**
   `lib.rs:1700` gọi `recovery.recover(&cfg)` ngay sau `connect(addr)` thành công;
   `GUIDE.md` §8c điểm 1 gọi đây là *"the easiest mistake to make here"*. Muốn số tiếp tục thì
   phải đưa một `Recovery` đọc từ journal trên đĩa.
4. **Hình dạng `Recovery` trên `FileJournal` đã có sẵn để chép:** `OnDisk` trong
   `crates/engine/tests/on_disk.rs:258–302`, mở `FileJournal::open(path, Durability::Async)` rồi
   suy `next_out` từ `highest()`. `FileJournal::open` đọc lại cả `highest` lẫn `highest_in` từ
   file — `on_disk.rs:104–109` khẳng định điều đó.
5. **`next_in` là `highest_in + 1`**, theo rustdoc của trait
   (`crates/session/src/journal.rs:125–128`: *"A resumed session's `next_in` is this plus one"*).
   `OnDisk` ở `on_disk.rs:288` viết `next_in = journal.highest_in().unwrap_or(1)` — **không cộng
   một**. Chưa biết test đó có lối nào nhìn thấy sai lệch này không; plan này **không chép dòng
   đó**, và ghi nó vào *Bẫy* để kiểm chứng riêng.
6. **`tools/interop/acceptor.cpp` đã có mọi thứ kịch bản cần, không phải sửa dòng nào:** bắt
   `SIGINT`/`SIGTERM` rồi gọi `acceptor.stop()` (Logout sạch), in `acceptor: ready` khi cổng đã
   nghe, in **mọi** message vào ra qua `toAdmin`/`toApp`/`fromAdmin`/`fromApp`, gửi hai `35=B` trong
   `onLogon`, và dùng `FileStoreFactory` — nghĩa là số thứ tự của nó nằm trên đĩa và sống qua một
   lần chết tiến trình.
7. **File cấu hình acceptor hiện tại làm QuickFIX quên số:** `ResetOnLogon=Y`, `ResetOnLogout=Y`,
   `ResetOnDisconnect=Y` (`scripts/interop.sh`, mục 2). Với `ResetOnLogon=Y` một acceptor QuickFIX
   đặt lại số về 1 khi **nhận** Logon. Kịch bản này cần file cấu hình riêng với cả ba là `N`, nếu
   không thì "tiếp tục đúng số" là câu hỏi không có nghĩa. Cổng và thư mục `FileStorePath` cũng
   phải riêng, vì hai chiều đã có dùng `15644` và `15645`.
8. **Hai cách "sàn chết" khác nhau về giao thức, và cả hai đều có ý nghĩa.** `kill -9` tiến trình
   acceptor: kernel đóng socket, initiator thấy EOF, **không có** `35=5` nào. `SIGTERM`:
   `acceptor.stop()` gửi `35=5`, initiator trả lời `35=5` rồi rớt. ADR-0043 quyết định 5 nói mọi
   lần kết thúc đều leo thang backoff, **kể cả logout sạch** — nên kịch bản chạy cả hai.
9. **`Application::on_message` nhận `seq`**, nên tool in được `34=` của từng application message
   đã **giao tới ứng dụng** — khác với "đã tới trên dây" (`docs/reference/a-message-on-the-wire-is-not-a-message-delivered.md`).
   Đây là bằng chứng phía fixbolt về chiều vào: nếu `next_in` quay về 1 thì `35=B` ở `34=5` không
   được giao mà bị hỏi lại bằng `35=2`.
10. **`crates/engine/tests/reconnect_wire.rs` chứng minh vòng lặp quay lại, không chứng minh giao
    thức:** acceptor là một `TcpListener` viết tay chỉ trả Logon, `Policy::new(50, 200)`,
    `NoRecovery`. Chính file đó nói việc kiểm giao thức là của `scripts/interop.sh`.
11. **`tools/interop` chỉ phụ thuộc `fixbolt` và `fixbolt-session`**, và `fixbolt` **không**
    re-export `connect_and_serve` (`crates/library/src/lib.rs` không có nó; `GUIDE.md` §8c gọi thẳng
    `fixbolt_engine::connect_and_serve`). Nên tool phải thêm `fixbolt-engine`, và feature
    `standard` của tool phải chuyển tiếp sang **cả hai** crate — bẫy hợp nhất feature
    (`docs/reference/feature-flags-unify-across-a-workspace.md`) đã trả giá một lần.
12. **CI job `interop` đã đọc từng bước bằng `grep` và đăng cả bốn transcript lên trang run**
    (`ci.yml` mục *Publish both transcripts*). Kịch bản mới nối vào cùng cơ chế: thêm dòng grep, thêm
    transcript.

## Cách làm

### 1. Vai mới `--role reconnect` trong `tools/interop`

```text
interop --role reconnect --connect 127.0.0.1:15646 --journal <dir>/FIXBOLT.journal
        [--first-ms 200] [--ceiling-ms 2000] [--no-recovery]
```

- `Config::initiator(b"FIX.4.4", b"FIXBOLT", b"QFACC").with_heart_bt_int(30)` — `30` để không
  heartbeat nào chen vào trong cửa sổ vài giây của kịch bản (xem *Bẫy* 4).
- `Application` tên `Watch`: mỗi application message giao tới thì in
  `interop-reconnect: delivered 34=<seq> 35=<type>`, không trả lời gì. Đây là toàn bộ đầu ra
  "có nghĩa" của tool; nó **không chấm điểm** — chấm điểm là việc của script trên transcript C++.
- `Recovery` tên `OnDisk` trên `FileJournal<64, 1024>`, `Durability::Async`: `recover` mở file,
  `next_out = highest() + 1`, **`next_in = highest_in() + 1`**, `last_active` từ journal; không có
  gì thì trả `None`. Kích cỡ ring không quan trọng ở đây vì kịch bản không replay gì.
- `--no-recovery` thay bằng `fixbolt_engine::recovery::NoRecovery`. Đây là **công tắc reversal**,
  cùng vai với `--invert-resend` của plan trước: một cờ để chứng minh gate nhìn thấy số thứ tự.
- In `interop-reconnect: dialing <addr>` rồi gọi `connect_and_serve`; hàm này không trả về cho
  tới khi bị `kill`. Cả vai nằm sau `#[cfg(all(feature = "standard", unix))]`, nhánh còn lại in
  `FAIL` như vai acceptor đang làm.
- `Cargo.toml`: thêm `fixbolt-engine = { path = ..., default-features = false }`;
  `standard = ["fixbolt/standard", "fixbolt-engine/standard"]`.

### 2. Kịch bản 4d trong `scripts/interop.sh` — sàn chết đột ngột

Cổng `PORT3` (`15646`), `store3/`, file `acceptor-reconnect.cfg` giống `acceptor.cfg` nhưng ba
`ResetOn*=N`. Trình tự, mỗi lần chờ đều có hạn 20 giây và in cả hai log khi hết hạn:

| # | Làm | Chờ thấy |
|---|---|---|
| a | chạy acceptor C++ → `A1.log` | `acceptor: ready` |
| b | chạy `interop --role reconnect` → `R.log` | `delivered 34=2` **và** `34=3` trong `R.log` |
| c | `kill -9` acceptor, `wait` | tiến trình đã chết |
| d | chạy lại acceptor **cùng cfg, cùng store** → `A2.log` | `acceptor: ready` |
| e | không làm gì | `delivered 34=5` **và** `34=6` trong `R.log` |
| f | `kill` cả hai | — |

Bước b là điều làm cho số thứ tự **xác định**: khi hai `35=B` đã được giao thì journal đã
`mark_in(3)`, và fixbolt mới gửi đúng một message (`35=A`, `34=1`). Bước e là bằng chứng chiều
vào: acceptor sống lại gửi Logon ở `34=4` và hai News ở `34=5`, `34=6`; chúng chỉ được **giao**
nếu `next_in` của fixbolt là 4.

Rồi script chấm, in từng dòng `interop-reconnect: <tên> ok|FAIL <đã thấy gì>` và một dòng
`interop-reconnect: PASS n/n`, cùng khuôn với hai chiều đã có:

| Tên | Khẳng định | Đọc từ đâu |
|---|---|---|
| `dropped` | `A1.log` **không** có `35=5` (chết đột ngột thì không ai chào) | `A1.log` |
| `back` | `A2.log` có một `in … 35=A` từ `49=FIXBOLT` | `A2.log` |
| `next_out` | `34=` của Logon đó bằng **`34=` cuối cùng fixbolt gửi trong `A1.log`, cộng một** | `A1.log`, `A2.log` |
| `next_in` | `R.log` có `delivered 34=5` và `34=6` | `R.log` |
| `no_resend` | sau Logon thứ hai, `A2.log` **không** có `35=2` ở chiều nào, **không** có `141=Y`, **không** có `35=5` | `A2.log` |

Khẳng định `next_out` là **tương đối**, đọc số từ chính transcript, chứ không ghi cứng `34=2` —
để một heartbeat chen ngang trên runner chậm làm sai kỳ vọng chứ không làm sai gate.

**Vì sao giết tiến trình chứ không `stop()`/`start()` `SocketAcceptor` trong process như item 38
gợi ý:** giết tiến trình là kịch bản triển khai thật (sàn khởi động lại), nó buộc `FileStore`
phải là thứ giữ số qua lần chết — đúng cái item 38 đòi — và **không cần sửa một dòng C++ nào**.
Dừng-mở trong process là kịch bản yếu hơn với nhiều code hơn.

### 3. Kịch bản 4e — sàn chào rồi mới đi

Cùng trình tự, khác hai chỗ: bước c là `kill -TERM`, và thêm khẳng định `goodbye`: `A1.log` có
`out … 35=5` **và** `in … 35=5` từ fixbolt. Khẳng định `next_out` vẫn là "cuối cùng cộng một",
lúc này số cuối là câu trả lời `35=5` của fixbolt. Kịch bản này là ADR-0043 quyết định 5 nhìn từ
phía một engine khác: sau một logout sạch, initiator **vẫn** quay lại, và quay lại với số tiếp
theo chứ không phải số 1.

### 4. CI

`ci.yml` job `interop`: thêm `R.log`, `A1.log`, `A2.log` của cả hai kịch bản vào bước *Publish*.
Không thêm job, không thêm `continue-on-error`.

### File sẽ tạo hoặc sửa

| File | Việc |
|---|---|
| `tools/interop/src/main.rs` | vai `reconnect`: `Watch`, `OnDisk`, cờ `--no-recovery` |
| `tools/interop/Cargo.toml` | phụ thuộc `fixbolt-engine`, feature chuyển tiếp |
| `scripts/interop.sh` | mục 4d, 4e; `PORT3`; grep từng khẳng định; dòng tổng kết cuối |
| `.github/workflows/ci.yml` | đăng thêm transcript |
| `docs/CONFORMANCE.md` §7 · `docs/DESIGN.md` §6 · `docs/GUIDE.md` §8c · `STATUS.md` | theo bảng dưới |

## Bất biến bị đụng tới

Không file nào dưới `crates/` đổi, nên bất biến 1–5, 7, 8 không bị chạm. Hai cái còn lại:

- **6 — feature gate.** Vai mới nằm sau `#[cfg(all(feature = "standard", unix))]` trên item, và
  feature của tool chuyển tiếp sang cả `fixbolt` lẫn `fixbolt-engine`. Kiểm bằng
  `scripts/check-no-optional-deps.sh` và `cargo test --all --no-default-features` vẫn build
  `fixbolt-interop`.
- **9 — không chép QuickFIX.** Mọi thứ script tải hoặc build nằm dưới `vendor/`; mục 5 của script
  đã so `git status` trước–sau và sẽ chạy như cũ.

Bất biến 4 không liên quan: vai này chạy `standard`, và `connect_and_serve` chỉ tồn tại ở mode
đó (ADR-0043, *Bad*, gạch đầu dòng 3).

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | `tools/interop --role reconnect` build được, chạy tay vào acceptor C++ (khởi động thủ công) in `dialing` rồi `delivered 34=2`, `34=3` | — |
| 2 | Mục 4d trong script: sàn chết đột ngột, năm khẳng định, `interop-reconnect: PASS 5/5` | 1 |
| 3 | Ba reversal của mục *Cách kiểm chứng* chạy và **đỏ đúng chỗ**, rồi khôi phục, xanh lại | 2 |
| 4 | Mục 4e: sàn chào rồi đi, `interop-reconnect-logout: PASS 6/6` | 2 |
| 5 | CI: transcript mới trên trang run; một run xanh **được nêu id** trên commit đóng plan | 2, 4 |
| 6 | Tài liệu theo bảng dưới; `STATUS.md` *Not proven* đọc từng dòng | 5 |

## Cách kiểm chứng

**Lệnh:** `scripts/interop.sh`. Đạt khi log có đủ cả bốn dòng tổng kết — `interop: PASS 7/7`,
`interop-acceptor: PASS 7/7`, `interop-reconnect: PASS 5/5`, `interop-reconnect-logout: PASS 6/6`
— **và** từng tên khẳng định được grep riêng, **và** `==> the run added nothing git can see`.
Đọc log, không đọc mã thoát (`docs/reference/a-green-fraction-over-a-scenario-that-never-ran.md`).

**Ba reversal, mỗi cái phải đỏ ở đúng một khẳng định:**

| Reversal | Đỏ ở đâu | Chứng minh gì |
|---|---|---|
| A — chạy với `--no-recovery` | `next_out FAIL` (Logon thứ hai ở `34=1`), và dự kiến `no_resend FAIL` vì QuickFIX trả `35=5` *MsgSeqNum too low* — **đọc chữ thật từ `A2.log`, không đoán** | gate nhìn thấy số thứ tự, không chỉ thấy "có nối lại" |
| B — bỏ bước d, không khởi động lại acceptor | `back FAIL` sau hạn 20 giây, **không treo** | hạn chờ là khẳng định; vòng lặp đang được kiểm là của engine, vì tool không có vòng nào của riêng nó |
| C — đổi tên một khẳng định trong danh sách grep | script fail dù transcript đầy đủ | grep từng bước là load-bearing, như reversal C của plan trước |

Sau mỗi reversal: `git diff` cho thấy đúng thay đổi đã hoàn tác, chạy lại xanh.

**Bằng chứng khi đóng:** trích nguyên văn khối `interop-reconnect:` và
`interop-reconnect-logout:` từ log của **CI job**, kèm run id và commit; và ba dòng đỏ của ba
reversal, nguyên văn.

## Tài liệu phải cập nhật

- [ ] `STATUS.md`: item 38 gạch, nêu run id; bảng plan; row item 35 thêm một câu *đã có ý kiến thứ
      hai*; **đọc lại từng dòng *Not proven*** — có bullet nào về reconnect hoặc `NoRecovery` thì
      xử lý theo `CLAUDE.md` §4 dòng cuối
- [ ] `docs/CONFORMANCE.md` §7: hai dòng mới trong bảng, mỗi dòng nêu vai, đối tác, điểm, run id
- [ ] `docs/DESIGN.md` §6: dòng *An initiator comes back after its counterparty hangs up* thêm
      `scripts/interop.sh` 4d/4e bên cạnh `reconnect_wire.rs`
- [ ] `docs/GUIDE.md` §8c: một câu chỉ vào kịch bản, để điểm 1 (*`NoRecovery` restarts your
      numbers*) có bằng chứng từ một engine khác chứ không chỉ lời dặn
- [ ] `docs/reference/`: **chỉ nếu** có bẫy mới lộ ra — ứng viên là *Bẫy* 3 và 5 dưới đây
- [ ] `CHANGELOG.md`: **không** — không API công khai nào đổi, tool không publish
- [ ] ADR-0043: **không sửa** (đã Accepted); câu *"today does not"* trong *Consequences* trở thành
      sai một cách đúng đắn, và `STATUS.md` là nơi nói điều đó
- [ ] Nếu *Bẫy* 3 xác nhận `on_disk.rs:288` sai: ghi `docs/reference/` + open item mới, **không
      sửa trong branch này**

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| 1. Thêm bước vào vai `initiator` cũ, kiểm nhầm vòng lặp của tool | vai mới bắt buộc gọi `connect_and_serve`; `grep -c connect_and_serve tools/interop/src/main.rs` ≥ 1; reversal B |
| 2. `ResetOn*=Y` trong cfg cũ làm QuickFIX quên số, gate xanh vì hai bên **cùng** về 1 | cfg riêng; khẳng định `next_out` đòi "cuối cộng một" nên `34=1` là FAIL; reversal A |
| 3. `next_in` thiếu `+1` (chép `on_disk.rs:288` không đọc rustdoc trait) — fixbolt hỏi `35=2` cho Logon thứ hai của acceptor | `no_resend` cấm `35=2`; `next_in` đòi `34=5`, `34=6` được **giao** |
| 4. Heartbeat chen vào làm `34=` không cố định trên runner chậm | `HeartBtInt=30`; khẳng định tương đối, đọc số từ `A1.log` |
| 5. `Durability::Async` chưa ghi xong khi `recover` mở lại file — `recover` đọc số cũ, Logon sai số | `next_out` bắt được; nếu dính, chuyển `Fsync` cho tool và ghi `docs/reference/` |
| 6. Cổng chưa nhả sau `kill -9`, acceptor thứ hai không bind được — trông như initiator không quay lại | `SocketReuseAddress=Y`; chờ `acceptor: ready` có hạn; in `A2.log` khi hết hạn |
| 7. Gate xanh vì không đọc gì: tool chết trước khi in, hoặc script in PASS mà thiếu khẳng định | grep từng tên; mỗi dòng in *đã thấy gì*; reversal C |
| 8. Reversal đỏ bằng cách treo | mọi `wait` có hạn 20 giây (`docs/reference/a-reversal-can-fail-by-hanging.md`) |
| 9. Feature unification: `fixbolt-engine/standard` bật cho cả workspace | `scripts/check-no-optional-deps.sh`; `cargo test --all --no-default-features` |
| 10. Cám dỗ "sửa luôn" item 47 để tool tự dừng được | script `kill`; item 47 cần ADR riêng, không ở đây |
| 11. Gate tìm ra lỗi trong `engine` hoặc `session` và cám dỗ sửa trong branch này | branch chỉ chứa tool, script, CI, docs; lỗi đi vào `docs/reference/` + open item + plan riêng |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| QuickFIX sống lại gửi `35=2` một cách **hợp lệ** vì fixbolt đã gửi thứ gì đó acceptor chưa kịp thấy trước khi bị giết | Thấp | `HeartBtInt=30` và giết ngay sau khi hai News được giao thu hẹp cửa sổ. Nếu vẫn xảy ra thì đó là phát hiện thật về cửa sổ mất message, ghi lại, không nới gate |
| Runner chậm làm hạn 20 giây không đủ | Thấp | Hạn là biến môi trường `INTEROP_DEADLINE`; mặc định 20, tăng chỉ khi log cho thấy tiến trình đang tiến chứ không đứng |
| `on_disk.rs:288` sai thật, và test đó vẫn xanh | Trung bình | Đó là kết quả có giá trị hơn kịch bản: một test fixture đọc sai rustdoc của trait mình implement. Ghi `docs/reference/`, open item, plan riêng |
| Kịch bản 4e: `stop()` của QuickFIX chờ Logout trả lời với timeout riêng, khiến bước c lâu hơn dự kiến | Thấp | `wait` có hạn; nếu quá 20 giây thì đọc `A1.log` xem phía nào chờ ai |

## Ngoài phạm vi

- **Khởi động lại tiến trình fixbolt** (recovery từ đĩa sau khi *mình* chết): đó là item 31/32 và
  `crates/engine/tests/on_disk.rs`, không phải item 38.
- **Initiator `hft`**: không tồn tại (ADR-0043), không tạo ở đây.
- **Jitter**, **`Schedule`** trong kịch bản: ADR-0043 quyết định 1 và 3, có test thuần riêng.
- **Item 47** (tay cầm từ `serve`/`connect_and_serve`): cần ADR về hình dạng.
- **Sửa `acceptor.cpp`**: không cần, và không làm.
- **Bộ soi gương** (item 36): không liên quan.

## Nhật ký giao hàng

**Đóng 2026-09-05, cả sáu bước. Không file nào dưới `crates/` đổi.**

### Kết quả

```
scripts/interop.sh
interop: PASS 7/7
interop-acceptor: PASS 7/7
interop-reconnect: dropped     ok    no 35=5 in the first transcript (saw 0)
interop-reconnect: back        ok    acceptor: in  ...|35=A|34=3|49=FIXBOLT|...
interop-reconnect: next_out    ok    sent up to 34=2 before the kill, came back at 34=3, wanted 34=3
interop-reconnect: next_in     ok    35=B at 34=5 6 sent after the restart, each delivered to the application
interop-reconnect: no_resend   ok    35=2: 0, 141=Y: 0, 'MsgSeqNum too low': 0
interop-reconnect: PASS 5/5
interop-reconnect-logout: goodbye     ok    35=5 out: 1, answered by this engine: 1
interop-reconnect-logout: redialled   ok    a Logon reached the acceptor after a clean goodbye (22 lines say so)
interop-reconnect-logout: known_gap   ok    expecting 4 but received 3 — short by exactly the one 35=5 ... (item 48)
interop-reconnect-logout: PASS 3/3
==> the run added nothing git can see
interop: 7 / 7 + 7 / 7 + 5 / 5 + 3 / 3 against libquickfix @ 386ce46e...
```

**Vòng nối lại đúng ngay lần chạy đầu sau khi Sửa 2 được áp** — kể cả chỗ dễ sai âm thầm nhất:
`34=` của Logon thứ hai, và hai News sau khi sàn sống lại được **giao tới ứng dụng** chứ không bị
hỏi lại bằng `35=2`.

### Ba reversal, mỗi cái cắn một chỗ khác nhau

| Reversal | Thấy gì |
|---|---|
| A — `--no-recovery` | `next_out FAIL  came back at 34=none, wanted 34=3`; `no_resend FAIL  'MsgSeqNum too low': 22`; `known_gap FAIL  expecting 4 but received 1`. **`dropped` vẫn `ok`** — reversal chạm đúng thứ nó nhắm, không đỏ cả bảng. Script thoát 1 |
| B — chính sách 600 000 ms, engine không kịp quay lại trong hạn | `back FAIL  nothing reached the second acceptor` **sau hạn, không treo**. Và `no_resend` vẫn **`ok`** (0 refusal) — **đó là chỗ B khác A**: "không quay lại" và "quay lại sai số" có hai chữ ký khác nhau trên transcript |
| C — đổi tên một bước trong danh sách grep (`next_out` → `next_ou`) | scenario vẫn in `interop-reconnect: PASS 5/5` mà script thoát 1 với `MISSING OR FAILED ASSERTION: interop-reconnect next_ou`. Grep từng bước là load-bearing |

Sau khôi phục: `scripts/interop.sh` giống hệt bản trước reversal, chạy lại xanh cả bốn.

### Gate

| Lệnh | Kết quả |
|---|---|
| `scripts/interop.sh` | **7 / 7 + 7 / 7 + 5 / 5 + 3 / 3**, thoát 0 |
| `cargo test --all` | **506 passed, 0 failed**, thoát 0 — trong đó corpus 59 định nghĩa |
| `cargo clippy --all-targets --all-features -D warnings` | sạch |
| `cargo fmt --all --check` | sạch |
| `cargo doc --workspace --no-deps` dưới `-D rustdoc::broken_intra_doc_links` | sạch — **đỏ một lần** vì doc link trỏ `Application::on_message` sau khi `Watch` đổi sang `Handler` |
| `scripts/check-no-optional-deps.sh` | ok, gồm `fixbolt-interop` với `fixbolt-engine` mới thêm |
| `scripts/check-lint-config.sh` | `RED ok` / `GREEN ok` |
| `scripts/check-links.py` | 1 419 link, 0 chết |

### Ba lần sửa plan, tất cả ghi trước khi code đổi

Xem ba mục *Sửa đổi* đầu file. Ngắn gọn: `seq` không phải số nhận vào (Sửa 1); journal không giữ
message quản trị nên `next_out` ra 1 và bị từ chối (Sửa 2, thành item 48); 4e còn ba khẳng định và
`back` của nó đọc sai tầng (Sửa 3).

### Hai thứ tìm ra, không cái nào là lỗi của session layer

| Tìm ra | Ghi ở |
|---|---|
| Journal giữ message, không giữ cách đánh số; và số đúng thì không với tới được qua `connect_and_serve` | STATUS item **48**, [a-journal-holds-messages-not-numbering](../reference/a-journal-holds-messages-not-numbering.md) |
| Một message bị từ chối ở tầng session không bao giờ tới callback ứng dụng, nên transcript dựng trên callback mù đúng chỗ cần nhìn | [a-refused-message-never-reaches-the-callback](../reference/a-refused-message-never-reaches-the-callback.md), **`[to testing-skills]`** |

### Chưa làm, và nói rõ

- **Không kịch bản nào giết tiến trình fixbolt.** Chỉ sàn chết. Recovery khi *phía này* chết vẫn là
  `crates/engine/tests/on_disk.rs`, tức dự án tự đọc mình.
- **Item 48 mở, và plan này cố ý không sửa** — nó cần ADR của item 47 trước. `CLAUDE.md` §1.
- **`crates/engine/tests/on_disk.rs:288` vẫn thiếu `+ 1`** (Bẫy 3). Vai `reconnect` không chép dòng
  đó; test kia có nhìn thấy sai lệch hay không thì **chưa kiểm**, vì kiểm nó là sửa file dưới
  `crates/`, ngoài phạm vi. Đi kèm item 48.
