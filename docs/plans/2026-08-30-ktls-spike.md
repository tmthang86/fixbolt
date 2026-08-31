# `ktls-core` có lái được từ một socket non-blocking trần không?

> **Loại:** Plan · **Ngày:** 2026-08-30 · **Trạng thái:** **Xong — đóng 2026-08-31**
> **Phạm vi:** open item 10 — một spike, kết quả là một phát hiện chứ không phải code

## Bối cảnh

[ADR-0005](../decisions/ADR-0005-tls.md) đã được duyệt và làm TLS thành một transport, với bảo
đảm hot-path phát biểu riêng cho từng chế độ. Nó được duyệt **trên lập luận, không trên đo**,
và câu hỏi mở số 1 của chính nó vẫn chưa có câu trả lời — đó là open item 10:

> `ktls-core` có lái được từ một socket non-blocking trần, không async runtime, không?

Cách dùng được ghi trong tài liệu của nó có hình dạng `tokio-rustls`. Nếu câu trả lời là
**không**, thì luận điểm trung tâm của ADR-0005 sụp xuống thành "chỉ có rustls ở userspace", và
bảo đảm hot-path đi theo — vì lúc đó mọi byte TLS phải đi qua một tầng mã hoá trong tiến trình
thay vì để kernel làm.

`DESIGN.md` D5 nói mọi thứ tuỳ chọn phải nằm sau feature flag, và `CLAUDE.md` §6 nói một phụ
thuộc kéo theo async runtime **cần một ADR**. Nên câu trả lời cho item 10 quyết định TLS là
một transport nhỏ hay là một cuộc thương lượng lại kiến trúc.

Item 10 trước đây ghi là "không kiểm được ở đây — cần máy Linux của item 6". **Điều đó không
còn đúng.** Phiên này chạy trên Linux 6.18 x86_64. Kernel TLS là một tính năng kernel, không
phải một đặc tính hiệu năng — nên nó **kiểm được ngay ở đây**, dù container này không đủ tư
cách công bố số latency.

## Những gì đã biết chắc

- ADR-0005 đã Accepted, và câu hỏi mở số 1 của nó chưa được trả lời.
- **Chưa có plan nào cho TLS.** `STATUS.md` ghi TLS bị chặn ở đúng item 10.
- `crates/engine/src/transport.rs` đã có `Transport` là một trait, với `Io::Idle` cho
  `WouldBlock` — tức là hình dạng để cắm một transport thứ hai đã sẵn.
- `codec` có **zero** phụ thuộc runtime, và đó là một quy tắc cứng. Bất cứ thứ gì TLS kéo vào
  đều phải nằm ngoài `codec`.
- Máy đang chạy: Linux 6.18.44 x86_64. `DESIGN.md` §9 mô tả một máy khác hẳn — không ghim
  core, không `isolcpus` ở đây.

## Cách làm

Đây là một **spike**: mục tiêu là một câu trả lời có bằng chứng, không phải một tính năng.
Kết thúc bằng một trang trong `docs/reference/`, và có thể là một ADR sửa ADR-0005.

1. **Kiểm kernel có `tls` không.** `modprobe tls`, `/proc/net/tls_stat`, và setsockopt
   `TCP_ULP = "tls"` trên một socket thật. Nếu container không cho, nói thẳng và dừng — một
   spike không kết luận được vẫn là một kết quả, miễn là nó nói rõ nó không kết luận được.
2. **Viết chương trình nhỏ nhất có thể**: một socket TCP non-blocking, bắt tay TLS bằng
   `rustls` ở userspace, rồi **giao khoá cho kernel** qua `TCP_ULP` + `TLS_TX`/`TLS_RX`, rồi
   `read`/`write` bình thường. Không tokio, không async, không runtime.
3. **Đo cái quan trọng:** sau khi giao khoá, đường dữ liệu có còn là `read`/`write` trần
   không, và `WouldBlock` có còn cư xử như cũ không. Đó chính xác là cái mà `Transport` cần.
4. **Trả lời câu hỏi bằng một trong ba kết luận**, không được lấp lửng:
   - *Được* → ADR-0005 giữ nguyên, và một plan TLS thật mở ra sau.
   - *Được nhưng có điều kiện* → ghi điều kiện, và ADR-0005 cần một ADR bổ sung.
   - *Không được* → ADR-0005 phải bị supersede, và `DESIGN.md` D11 phải sửa.

## Bất biến bị đụng tới

- **Số 4** (*luồng engine không bao giờ ngủ trong kernel*). Cả điểm của kTLS là giữ đường dữ
  liệu vẫn là `read`/`write` non-blocking. Nếu bắt tay hoặc gia hạn khoá kéo theo một lệnh
  chặn, đó là phát hiện chính của spike và phải ghi lại.
- **Số 6** (feature flag gate cả `mod`). Nếu spike sinh ra code, nó nằm sau feature, và job
  `--no-default-features` phải xanh trên máy không cài gì.
- **Số 8** (`unsafe` cần plan và một comment nêu cái chứng minh nó đúng). `setsockopt` với
  struct khoá gần như chắc chắn cần `unsafe`. Spike phải nói rõ chỗ nào, và cái gì chứng minh.
- **Số 10.** Spike này **không công bố số hiệu năng nào** — container này không đủ tư cách.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | Xác nhận kernel ở đây có kTLS, hoặc xác nhận là không và dừng có ghi chép | — |
| 2 | Chương trình nhỏ nhất: rustls bắt tay + giao khoá cho kernel, socket non-blocking, không runtime | 1 |
| 3 | Kết luận về `WouldBlock`, về `unsafe` cần thiết, và về mọi lệnh chặn gặp phải | 2 |
| 4 | `docs/reference/ktls-on-a-plain-socket.md` — câu trả lời và cách kiểm lại | 3 |
| 5 | ADR bổ sung hoặc supersede ADR-0005, **chỉ nếu** kết luận đòi hỏi | 4 |

## Cách kiểm chứng

- **Bằng chứng phải là byte đi qua**, không phải là một lời gọi API trả `Ok`. Gửi dữ liệu qua
  và đọc lại ở đầu kia; và **kiểm bằng một công cụ ngoài tiến trình** rằng dữ liệu trên dây đã
  được mã hoá — nếu không thì "TLS đang chạy" chỉ là suy đoán từ một exit code.
- **Đảo ngược:** bỏ bước giao khoá cho kernel và xem đầu kia không đọc được nữa. Nếu bỏ mà vẫn
  chạy thì kTLS chưa hề bật, và mọi kết luận trước đó vô nghĩa.
- **Đọc `/proc/net/tls_stat` trước và sau**, và khẳng định nó đổi. Đây là quan sát độc lập với
  code của chính mình.
- `strace` để trả lời câu hỏi về lệnh chặn — cùng công cụ mà plan `w2w-and-linux-numbers` dựng.

## Tài liệu phải cập nhật

- [ ] `docs/reference/ktls-on-a-plain-socket.md` — mới, **ưu tiên cao nhất** theo §4
- [ ] `STATUS.md` — đóng item 10, và gỡ chú thích "cần máy Linux của item 6" vì nó đã sai
- [ ] `docs/decisions/` — ADR bổ sung/supersede, **chỉ nếu** kết luận đòi hỏi
- [ ] `DESIGN.md` D11 — nếu kết luận là "không được"
- [ ] `PRD.md` — nếu TLS đổi phase

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| `setsockopt` trả `Ok` nhưng kTLS không bật | Đọc `/proc/net/tls_stat`; và đảo ngược bằng cách bỏ giao khoá |
| Dữ liệu đi được nhưng thật ra vẫn qua rustls userspace | Bắt gói ngoài tiến trình, hoặc so `tls_stat` trước/sau |
| Container cho `TCP_ULP` nhưng máy thật cấu hình khác | Ghi rõ kernel version và cấu hình đã kiểm; kết luận có phạm vi |
| Spike lặng lẽ phình thành một triển khai TLS | "Ngoài phạm vi" bên dưới, và không có code TLS nào được merge từ plan này |
| Kết luận "chắc là được" | Bắt buộc chọn đúng một trong ba kết luận ở bước 4 |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Container chặn `TCP_ULP` hoặc không có module `tls` | Trung bình | Dừng và ghi lại là chưa kết luận được. **Không** suy ra câu trả lời từ tài liệu |
| Câu trả lời là "không" và ADR-0005 phải bị lật | Trung bình | Đó là lý do spike tồn tại. ADR mới supersede, không sửa ADR đã accept |
| Cần `unsafe` nhiều hơn dự kiến | Trung bình | Điều 8: mỗi chỗ `unsafe` có comment nêu cái chứng minh nó đúng, hoặc không làm |

## Ngoài phạm vi

- **Không** triển khai TLS transport. Spike này chỉ trả lời một câu hỏi.
- **Không** đo hiệu năng TLS — container này không đủ tư cách (item 6).
- **Không** đụng `codec` (zero dependency), session, hay engine.
- **Không** viết plan TLS thật ở đây — nó là việc sau, và phụ thuộc vào kết luận này.

## Nhật ký giao hàng

**Duyệt 2026-08-30.** Chủ dự án duyệt cả sáu plan cùng lúc, kèm một uỷ quyền ghi rõ:
*trong quá trình làm, nếu plan sai thì được sửa plan theo tình hình thực tế.* Điều đó nới
`CLAUDE.md` §1 — chỗ bảo "dừng lại, sửa plan, xin duyệt lại" — thành "sửa plan, **ghi lại
vào đây**, đi tiếp". Mỗi lần sửa plan phải có một mục dưới đây nói rõ **sửa gì và vì sao**,
nếu không thì uỷ quyền này biến thành giấy phép đi chệch trong im lặng.

---

### Bước 1 — dừng ở đây, có ghi chép. 2026-08-30.

**Kết quả: không kết luận được trên máy này, và đó là nhánh mà chính bước 1 đã lường trước.**

`[measured 2026-08-30]` Linux 6.18.44-fc-v22:

```
config:    # CONFIG_TLS is not set
tls_stat:  absent
setsockopt(TCP_ULP, "tls"): REFUSED errno=2 (ENOENT)
```

`setsockopt` được gọi trên **socket TCP thật, đã kết nối**, ở cả hai đầu — trên socket chưa
kết nối nó hỏng vì lý do khác và kết quả đó chẳng nói gì. `ENOENT` nghĩa là kernel **không có**
ULP tên `tls` chút nào: module không tồn tại, chứ không phải chưa nạp hay bị policy chặn.

**Đọc dòng config là chưa đủ**, và đó là lý do phép kiểm gọi syscall chứ không `grep`: một
kernel config nói cái gì đã được biên dịch, nó không nói một container được phép làm gì. Lời
gọi syscall nói cả hai.

**Sửa plan — hai lần, và cả hai đều về chỗ item 10 bị chặn.** Plan này viết rằng item 10 bị
ghi sai là "cần máy Linux của item 6", và điều đó đúng: kTLS là **tính năng kernel**, không
phải đặc tính hiệu năng, nên nó không cần máy §9 nào cả. **Nhưng plan cũng sai**: nó kết luận
"nên chạy được ngay ở đây". Không phải "một máy Linux" là đủ — phải là **kernel build với
`CONFIG_TLS`**, một thứ hẹp hơn hẳn và không suy ra được từ việc đang chạy Linux.

**Không kết luận gì về `ktls-core` hay ADR-0005.** Thí nghiệm chưa đi tới được thư viện. Ghi
"chắc là được" hay "chắc là hỏng" ở đây đúng là loại phát biểu `CLAUDE.md` §10 cấm.

**Cái thu được, và nó có giá trị thật:** yêu cầu để làm tiếp giờ đã đúng và kiểm được bằng một
lệnh. `scripts/check-ktls-available.sh` trả lời "máy này bắt đầu được chưa?" và thoát khác 0
kèm lý do khi chưa. Bước 2–5 của plan giữ nguyên.

**Trạng thái: Tạm dừng.** Chặn ở một kernel có `CONFIG_TLS` — **không phải** ở máy §9.

### Bước 1b — chặn được gỡ, và cái chặn là chính phép kiểm. 2026-08-30.

**Trạng thái: Hết tạm dừng.** Bước 2–5 chạy được.

`[đo 2026-08-30]` trên desktop Linux của chủ dự án — AMD Ryzen 7 3700X, Linux 7.0.0-30-generic:

```
config: CONFIG_TLS=m
module: loaded=yes on_disk=yes
setsockopt(TCP_ULP, "tls"): ACCEPTED
READY — This machine can answer STATUS.md open item 10.       EXIT=0
```

**Máy này vốn đã đủ điều kiện từ đầu. Cái nói ngược lại là `scripts/check-ktls-available.sh`.**
Lần chạy đầu tiên trên đúng máy này in ra `config: CONFIG_TLS=m`, rồi bốn dòng sau in *"the
kernel has no `tls` ULP at all: it was built without CONFIG_TLS"*. Hai câu, một lần chạy, mâu
thuẫn nhau. Nguyên nhân: script in **một đoạn văn ENOENT cố định cho mọi `OSError`**, và không
bao giờ đọc lại dòng config mà chính nó vừa in.

Điều `ENOENT` từ `TCP_ULP` thực sự nói là **không có ULP nào đăng ký dưới tên `tls`**. Đăng ký
khác với biên dịch: kernel chỉ tự `request_module` cho ULP khi tiến trình gọi có
`CAP_NET_ADMIN`, nên một tiến trình thường thấy `ENOENT` trong khi `tls.ko` nằm ngay trên đĩa.
Đó chính là trạng thái của máy này.

**Bẫy thứ hai, phát hiện trong lúc sửa bẫy thứ nhất.** Bản sửa dò module bằng
`lsmod | grep -q '^tls '` và báo `loaded=no` trên máy đang nạp module. Dưới `set -o pipefail`
của chính script, `grep -q` thoát ngay khi khớp, `lsmod` chết vì `SIGPIPE` với mã 141, và
**pipeline báo thất bại đúng vào lần tìm thấy thứ cần tìm**. `scripts/check-machine.sh` có y
hệt cấu trúc đó ở hàng kTLS; ở đó nó bị `|| modinfo tls` che, nhưng trên kernel `CONFIG_TLS=y`
thì module là built-in, `modinfo` không tìm thấy gì, và hàng đó sẽ báo *"no tls module"* trên
đúng loại máy chạy kTLS tốt nhất. Cả hai giờ hỏi `/sys/module/tls`.

**Sửa plan.** Bước 1 của plan này coi `check-ktls-available.sh` là kết quả giao được. Nó không
phải: nó là một cổng mà **không có gì canh nó**, nên nó sai suốt một ngày mà không ai biết.
Bổ sung vào bước 1: `scripts/check-ktls-classify.sh` — 8 tổ hợp (syscall, config, loaded,
on_disk), khẳng định token cho từng cái, không cần kernel, không cần root, chạy trong CI ở job
`script-logic`. Đối chiếu với logic cũ nó **trượt 5/8**, gồm cả chính trường hợp của desktop
này; 3 cái nó qua là trường hợp container và trường hợp accepted — logic cũ đúng ở đó thật.

**Chứng minh bằng đảo ngược, trên script thật chứ không chỉ trên hàm:** gỡ module ra để tái
tạo đúng trạng thái đã cho câu trả lời sai → script mới báo `LOADABLE`, `EXIT=2`, kèm đúng
lệnh cần chạy. Nạp lại → `READY`, `EXIT=0`.

**Vẫn không kết luận gì về `ktls-core` hay ADR-0005.** Thí nghiệm vẫn chưa chạm tới thư viện.
Cái thay đổi duy nhất là bây giờ nó chạm được.

### Bước 2–5 — trả lời được, và câu trả lời là "được, kèm điều kiện". 2026-08-31.

`[đo 2026-08-31]` trên desktop của chủ dự án — AMD Ryzen 7 3700X, Linux 7.0.0-30-generic,
`CONFIG_TLS=m` đã nạp. Thư viện: `ktls-core` 0.0.5, `rustls` 0.23.43 (provider `ring`),
`rcgen` 0.14.10. TLS 1.3, một bộ mã duy nhất `TLS13_AES_128_GCM_SHA256`.

Chương trình: [`spikes/ktls`](../../spikes/ktls). Cổng chạy nó:
`scripts/check-ktls-on-a-plain-socket.sh`. **15 khẳng định, `fail 0`.**

**Kết luận, chọn đúng một trong ba nhánh bước 4 bắt buộc: *được nhưng có điều kiện*.**
Nên ADR-0005 **không** bị lật; nó được bổ sung bằng
[ADR-0018](../decisions/ADR-0018-ktls-on-a-plain-socket-answers-adr-0005.md).
`DESIGN.md` D11 giữ nguyên hình dạng, chỉ đổi từ "chưa kiểm được" sang "đã đo".

**Cái làm ADR-0005 tưởng là khó, hoá ra là nhầm crate.** ADR-0005 dẫn ba crate cùng lúc.
`ktls` 6.0.2 (dưới tổ chức rustls) đúng là chỉ dùng được với `tokio-rustls`; `ktls-stream`
mặc định bật feature `tokio`. Nhưng `ktls-core` — đúng cái mà câu hỏi mở số 1 gọi tên — phụ
thuộc `bitfield-struct`, `libc`, `nix`, `zeroize`, **không có feature async nào cả**, và mọi
điểm vào đều đồng bộ và generic trên `AsFd`. `Context::handle_io_error` chính là hình dạng một
vòng spin cần: gọi `read`, hỏng thì đưa lỗi lại cho session rồi thử lại.

**Đường dữ liệu không có lệnh chặn nào.** `strace -f`, quy syscall về đúng luồng đã in mốc, qua
1000 lượt đi-về: `recvfrom` 3033, `sendto` 1000, **không gì khác**. Nhánh đỏ — cùng vòng lặp đó
với `poll(2)` đặt trước `read` — cho `poll` 1000, và đó là thứ chứng minh nhánh xanh **có thể**
đỏ. Tiện thể đo được luôn cái giá: spin tốn ~3.0 `recvfrom` mỗi bản tin, chặn tốn 1.0.

**Bốn điều kiện, mỗi cái đều đo chứ không suy:**

| # | Điều kiện | Bỏ qua thì |
|---|---|---|
| 1 | Mọi lỗi `read` phải đưa cho `ktls_core::Context` | `[đo]` 1 `EIO` mỗi kết nối với số session ticket mặc định của rustls. Vòng đọc coi lỗi là chết sẽ giết session ngay sau bắt tay |
| 2 | **Không bao giờ tự đọc socket ngoài đường offload** | `[đo]` `EBADMSG` (errno 74) ở bản ghi kế tiếp, và `/proc/net/tls_stat` `TlsDecryptError` 0 → 1 |
| 3 | Lúc giao khoá, buffer userspace phải trống | Số byte còn lại là ciphertext kernel sẽ không bao giờ thấy |
| 4 | `setup_ulp` cần socket đang `ESTABLISHED` | `ENOTCONN` (errno 107), đọc lên như lỗi kTLS mà không phải |

**Sửa 1 — bằng chứng "ngoài tiến trình" làm theo cách khác plan viết.** Plan nói *"bắt gói ngoài
tiến trình"*. `tcpdump` trên `lo` cần `CAP_NET_RAW`, tức là một hộp thoại pkexec trên máy chủ dự
án, cho một thứ không cần đến quyền đó. Thay bằng hai quan sát độc lập với code của chính spike:
(a) đầu nhận **không** bật kTLS và đọc socket thô — đó vẫn là byte trên dây, chỉ đọc ở đầu kia;
(b) `/proc/net/tls_stat`, con số của kernel. Mục tiêu của plan — "không suy ra từ một exit code"
— giữ nguyên; công cụ thì không.

**Sửa 2 — code nằm ở `spikes/ktls`, ngoài workspace.** Plan không nói để đâu. Đặt ngoài
workspace vì `CLAUDE.md` §2 điều 6: job `--no-default-features` phải build trên máy không cài gì,
mà cargo hợp nhất feature trong một lần gọi — repo này **đã** dính đúng bẫy đó một lần
(`docs/reference/feature-flags-unify-across-a-workspace.md`). `Cargo.lock` của spike **được
commit**, khác `fuzz/`: câu trả lời gắn với phiên bản cụ thể, chạy lại mà tự giải ra phiên bản
khác là trả lời một câu hỏi khác.

**Bẫy tự mình dính, và nó là thứ đáng giá nhất trong buổi làm.** Phép kiểm dây lúc đầu khẳng
định *"5 byte đầu là header application-data của TLS 1.3"*. Nó **xanh trong khi đọc nhầm một bản
ghi session ticket mà bài test chưa hề gửi** — việc giao khoá của bên gửi đã hỏng hẳn, và
ticket có header y hệt. Hai thứ sai cùng lúc, transcript in ra một dòng `PASS`.

Sửa bằng cách khẳng định **kích thước mà payload bắt buộc phải có** thay vì *hình dạng*: 35 byte
plaintext + 1 byte content-type + 16 byte tag = bản ghi phải khai đúng 52. Chứng minh bằng đảo
ngược: bật lại ticket thì phép kiểm mới đọc `record declares 341, needle+type+tag is 52` và
**đỏ**, đúng chỗ phép kiểm cũ xanh. Đáng chú ý là cái **không** cứu được: khẳng định đi kèm
*"không có plaintext trong đám byte này"* xanh suốt, vì một payload chưa từng gửi thì hiển
nhiên là không có. **Một khẳng định phủ định không phát hiện được việc thứ cần đo chưa hề xảy
ra.**

Bẫy thứ hai: bản đầu dùng `?` trên nửa chạy ở luồng chính, nên khi luồng phụ hỏng thì lỗi của nó
bị bỏ đi *và* socket nó giữ bị đóng — rồi nửa chính báo `ENOTCONN` từ một `setsockopt` hoàn toàn
lành. Transcript gọi tên triệu chứng và giấu nguyên nhân. Cả hai đều đã ghi vào
`docs/reference/ktls-on-a-plain-socket.md` kèm dấu `[to testing-skills]`.

**Không kết luận gì về:** số latency (spike cố ý không công bố cái nào, `DESIGN.md` §8 hàng TLS
vẫn trống), key update dưới kTLS, TLS 1.2, mutual TLS, sàn kernel/bộ mã, và cổng trả lời "chế độ
nào đang chạy". Bốn thứ đầu là câu hỏi mở 2, 4, 5, 6 của ADR-0005; cái cuối là câu hỏi 3.

**Plan đóng.** Item 10 đóng theo. Việc tiếp theo về TLS là một plan riêng và chưa được viết —
đúng như phần "Ngoài phạm vi" của plan này yêu cầu.
