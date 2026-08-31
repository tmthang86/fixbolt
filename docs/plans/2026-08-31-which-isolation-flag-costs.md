# Cái nào trong ba tuỳ chọn cô lập lấy mất 36%

> **Loại:** Plan · **Ngày:** 2026-08-31 · **Trạng thái:** **Xong** — ADR-0021 `Accepted`, máy ở dòng §9 mới, `bench.sh --strict` OK · **Sửa 2026-08-31** — thêm bước 4b (jitter), chủ dự án duyệt sau khi bước 4 ra kết quả
> **Phạm vi:** open item 22 — phần còn lại sau khi `threads-and-affinity` đóng

## Bối cảnh

`DESIGN.md` §9 bảo dành cho engine một lõi cô lập, và đưa ra ba thứ cùng lúc:
`isolcpus`, `nohz_full`, `rcu_nocbs`. Ngày 2026-08-31
[measured-costs.md](../reference/measured-costs.md) đo được lõi cô lập **chậm hơn 36%**
ở đúng thao tác mà §8 nói là tốn nhất — một `recv` không chặn.

Bài đo đó dừng lại ở chỗ trung thực nhất có thể lúc ấy: **ba tuỳ chọn được một dòng
kernel command line áp lên cùng một tập CPU, nên không tách được cái nào gây ra.**
Cơ chế được nêu tên — `nohz_full` bật context tracking chạy ở **mọi** lần vào và ra
kernel — nhưng nó được dán nhãn *giả thuyết*, không phải phép đo.

Việc này biến giả thuyết đó thành một con số, hoặc bác bỏ nó. Kết quả quyết định
§9 sẽ khuyến nghị gì: nếu chỉ một trong ba tốn tiền, hai cái kia vẫn dùng được miễn phí.

## Những gì đã biết chắc

Đo trên chính máy §9 hôm nay, **không cần reboot nào**, bằng
`scratchpad/two_loops.c` — hai vòng lặp trên cùng một lõi, một cái không bao giờ vào
kernel, một cái không làm gì khác ngoài vào kernel:

| Lõi | `isolcpus`? | `user_loop` | `syscall_loop` |
|---|---|---|---|
| `cpu0` | không | 1.0581 ns/iter | 198.99 ns/call |
| `cpu5` | không | 1.0579 ns/iter | 198.36 ns/call |
| `cpu6` | **có** | 1.0546 ns/iter | **361.36 ns/call** |
| `cpu7` | **có** | 1.0546 ns/iter | **361.26 ns/call** |

12 lần đọc xen kẽ `cpu5, cpu6, cpu5, cpu6` × 3 vòng: `cpu5` nằm trong 198.35–199.02,
`cpu6` trong 359.41–361.52. Không trôi, không phụ thuộc thứ tự.

Ba sự thật rút ra, và cả ba đều đã đo chứ không suy:

1. **Tốc độ user space giống hệt nhau** — chênh 0.3%, và lõi cô lập nhanh hơn một
   chút chứ không chậm hơn. Cái giá không nằm ở xung nhịp.
2. **Cái giá nằm nguyên ở lần vào/ra kernel**: +162 ns cho một `getpid` trần,
   **+82%**. Con số 36% của `Engine::turn` nhỏ hơn chỉ vì `recv` là một syscall
   béo hơn, cùng một lượng phụ phí chia cho một mẫu số lớn hơn.
3. **`scaling_cur_freq` nói dối trên lõi `nohz_full`.** Sysfs đọc `cpu6` = 2 240 000 kHz
   (đúng bằng `scaling_min_freq`) trong khi nó đang chạy 100% tải, còn `cpu5` đọc
   3 792 929. Nếu tin nó thì kết luận sẽ là "lõi cô lập bị hạ xung 1.69×" — sai,
   vì `user_loop` chứng minh hai lõi chạy cùng một tốc độ. Driver là `amd-pstate-epp`
   ở chế độ `active`; giá trị đó do một đường cập nhật gắn với tick, mà tick thì
   `nohz_full` đã tắt.

Kernel này có sẵn cơ chế được nêu tên — `/boot/config-7.0.0-30-generic`:
`CONFIG_CONTEXT_TRACKING_USER=y`, `CONFIG_VIRT_CPU_ACCOUNTING_GEN=y`,
`CONFIG_NO_HZ_FULL=y`, `CONFIG_RCU_NOCB_CPU=y`, `CONFIG_CPU_ISOLATION=y`.

Topology cần cho thiết kế bên dưới: SMT **off**, `cpu4`–`cpu7` cùng một miền L3
(`cache/index3/shared_cpu_list` = `4-7`), mỗi lõi một thread.

Dòng lệnh kernel hiện tại, trong `/etc/default/grub` dòng 10:

```
quiet splash isolcpus=6,7,14,15 nohz_full=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1
```

## Cách làm

**Tách ba tuỳ chọn bằng cách đưa chúng cho ba CPU khác nhau trong cùng một lần boot**,
chứ không phải ba lần boot mỗi lần một tuỳ chọn. Cách này tốt hơn ở hai điểm: chỉ cần
**một** lần reboot thay vì ba, và các nhánh chia chung mọi thứ còn lại — cùng nhiệt độ,
cùng kernel, cùng tải, cùng phiên — nên phép trừ giữa chúng sạch hơn phép trừ giữa hai
lần boot.

Dòng lệnh cho lần boot thí nghiệm:

```
quiet splash isolcpus=4,6,12,14 rcu_nocbs=7,15 nohz_full=4,12 processor.max_cstate=1
```

Bốn lõi, bốn trạng thái, một biến đổi mỗi lần:

| Lõi | Được áp | Vai |
|---|---|---|
| `cpu5` | không gì | đối chứng |
| `cpu6` | `isolcpus` | một mình `isolcpus` |
| `cpu7` | `rcu_nocbs` | một mình `rcu_nocbs` |
| `cpu4` | `isolcpus` + `nohz_full` (kernel tự thêm `rcu_nocbs`) | đủ bộ §9 |

`nohz_full` không thể đứng một mình: kernel tự bật `rcu_nocbs` cho CPU nào có
`nohz_full`. Vì thế `cpu4` mang cả ba, và `nohz_full` được suy ra bằng phép trừ —
`cpu4` trừ `cpu6` (`isolcpus`) trừ `cpu7` (`rcu_nocbs`). Đó là lý do cần đủ bốn nhánh
chứ không phải hai.

`cpu4` cũng lấy `isolcpus` chứ không chỉ `nohz_full`, và đây là chủ ý: `nohz_full` chỉ
thực sự dừng tick khi CPU có **không quá một** tác vụ chạy được. Không có `isolcpus`,
scheduler vẫn đẩy việc lên `cpu4`, adaptive tick bật tắt thất thường, và nhánh đó có
thể đọc ra "miễn phí" vì cơ chế không chạy chứ không phải vì nó không tốn — một
**false green** đúng nghĩa. `isolcpus` giữ cho điều kiện đó đúng suốt phép đo.

File sẽ tạo hoặc sửa:

- `scripts/measure-isolation-cost.sh` — mới. Chạy một lệnh ra cả bảng, đọc
  `/proc/cmdline` và `/sys/devices/system/cpu/isolated` rồi **in ra trạng thái thật
  của từng lõi** thay vì tin vào cái tên nhánh.
- `scripts/measure-isolation-cost.c` — mới, chính là `two_loops.c` ở trên, đưa vào repo
  vì `CLAUDE.md` §2 điều 10: không có số nào mà không có benchmark đã commit sinh ra nó.
- `docs/reference/measured-costs.md` — mục "The isolated core is 36% slower" được nối
  thêm phần trả lời; không sửa phần cũ, vì nó đã đúng ở thời điểm viết.
- `docs/DESIGN.md` §9 — dòng khuyến nghị cô lập, theo kết quả.
- `docs/decisions/ADR-00NN` — nếu §9 đổi khuyến nghị.
- `STATUS.md` — item 22.

## Bất biến bị đụng tới

Không đụng `codec`, `session`, `engine`, `transport` — không có dòng code thư viện nào
thay đổi. Hai điều vẫn liên quan:

- **Điều 10** (không có số nào không có benchmark, máy, và thiết lập §9). Chính vì điều
  này mà `two_loops.c` phải vào repo dưới dạng script có tên, chứ không nằm lại trong
  scratchpad. Và mọi số ở đây **không phải số latency công bố được**: trong lần boot thí
  nghiệm máy **không** ở trạng thái §9 (thiếu `nohz_full` trên `cpu6`, `check-machine.sh`
  sẽ FAIL đúng như vậy). Chúng là **A/B của máy với chính nó**, và phải được dán nhãn đó.
- **Điều 4** (mode-scoped). Phát hiện này thuộc về `hft` — `standard` block trong kernel
  nên không quét, và một lần chặn thì phụ phí vào/ra kernel không nằm trên đường tới hạn
  theo cùng cách. Bài viết phải nói rõ nó nói về mode nào.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | `scripts/measure-isolation-cost.{c,sh}` vào repo; chạy trên boot §9 hiện tại và **tái lập** đúng bảng ở mục "đã biết chắc" | — |
| 2 | Ghi **dự đoán** vào plan này trước khi reboot, kèm cái gì sẽ bác bỏ nó | 1 |
| 3 | Chủ máy sửa `/etc/default/grub`, `update-grub`, reboot vào dòng thí nghiệm | 2 |
| 4 | Chạy script, đọc bảng bốn nhánh | 3 |
| 4b | Đo **đuôi** trên `cpu4` (`nohz_full`) và `cpu6` (chỉ `isolcpus`) — cả hai đều `isolcpus`, khác nhau đúng một biến. Thêm chế độ `--jitter` vào `measure-isolation-cost.c` | 4 |
| 5 | Chủ máy khôi phục dòng §9, reboot, `check-machine.sh` đọc lại `pass … fail 0` | 4b |
| 6 | Viết vào `measured-costs.md`; sửa §9 và ADR nếu khuyến nghị đổi; đóng item 22 | 4, 5 |

## Dự đoán, ghi trước bước 3

Ghi ở đây trước khi reboot, để kết quả có thể bác bỏ nó thay vì được đọc cho khớp với nó.

| Lõi | Được áp | Dự đoán `syscall_loop` |
|---|---|---|
| `cpu5` | không gì | ~199 ns |
| `cpu6` | `isolcpus` | **~199 ns** — `isolcpus` chỉ gỡ CPU khỏi scheduler domain, không thêm việc gì vào đường vào/ra kernel |
| `cpu7` | `rcu_nocbs` | **~199 ns** — nó dời callback RCU sang thread khác, cũng không nằm trên đường đó |
| `cpu4` | `isolcpus` + `nohz_full` | **~360 ns** |

**Cái bác bỏ dự đoán này:** `cpu6` hoặc `cpu7` đọc ~360. Khi đó cơ chế được nêu tên là sai
và bài viết trong `measured-costs.md` phải nói vậy bằng đúng số chữ đã dùng để nêu nó.

**Cái làm hỏng phép đo chứ không bác bỏ nó:** `cpu4` đọc ~199 **kèm** `ticks(LOC)` hàng nghìn —
nghĩa là tick không dừng và `nohz_full` chưa từng chạy.

## Dự đoán cho bước 4b, ghi trước khi chạy

`nohz_full` được mua để đổi lấy jitter, nên câu hỏi là: nó cắt được cái đuôi nào, và cái
đuôi đó có đáng 155 ns trung vị không?

`cpu6` chỉ có `isolcpus`: không tác vụ nào khác chạy lên đó, nhưng **tick vẫn chạy**
— `[đo]` ~3745 lần/giây. `cpu4` có `nohz_full`: `[đo]` 2–4 tick trong cùng cửa sổ.

Dự đoán: `cpu6` sẽ có những lần vọt hình dạng-tick ở đuôi xa mà `cpu4` không có, vào
khoảng **p99.85** (3745 tick trên ~5 triệu lần gọi ≈ 0.075%). **Tôi KHÔNG dự đoán được
chúng có đủ lớn để bù 155 ns trung vị hay không** — đó chính là lý do phải đo, và nếu tôi
đoán được thì không cần đo.

**Cái làm hỏng phép đo:** p50 của một trong hai lõi không khớp với bảng ở bước 4 cộng
thêm phụ phí đọc đồng hồ. Khi đó cái được đo không phải cái tưởng là đang đo.

## Cách kiểm chứng

**Bước 1 tự nó là một phép kiểm chứng:** script mới, chạy trên boot hiện tại, phải ra
lại 199 / 361 ns. Nếu không thì lỗi nằm ở script chứ không ở kernel, và biết điều đó
**trước** khi reboot rẻ hơn nhiều so với sau.

**Bước 4 đạt khi bảng có một hình dạng đọc được**, nghĩa là ít nhất một nhánh ~199 và
`cpu4` ~361. Nếu **cả bốn** cùng ~199 thì chính hiệu ứng đã biến mất và bài đo hỏng, không
phải kết luận "cô lập miễn phí" — khi đó nghi ngờ đầu tiên là `nohz_full` trên `cpu4`
không thực sự bật, kiểm bằng `/sys/devices/system/cpu/nohz_full` và bằng số ngắt timer
cục bộ trên `cpu4` trong `/proc/interrupts` (dòng `LOC`) trước và sau vòng lặp: một lõi
đã dừng tick tăng vài đơn vị, một lõi còn tick tăng hàng nghìn.

**`user_loop` là gate đọc-được-thì-tin-được của mỗi nhánh.** Nó phải bằng nhau trên cả
bốn lõi ở mọi lần boot. Nếu một nhánh lệch, xung nhịp hoặc nhiệt đã xen vào và nhánh đó
bị loại — chứ không được giải thích thành kết luận.

**Bước 5 đạt khi `scripts/check-machine.sh` đọc lại đúng số PASS như trước bước 3.**
Máy phải được trả về nguyên trạng, và điều đó phải được *đọc*, không được *cho là*.

## Tài liệu phải cập nhật

- [ ] `docs/reference/measured-costs.md` — phần trả lời, nối vào mục 36%, và phần đuôi từ bước 4b
- [x] `docs/DESIGN.md` §9 — dòng cô lập tách làm hai: `isolcpus`+`rcu_nocbs` giữ, `nohz_full` bị gỡ và được **định giá**
- [x] `docs/DESIGN.md` §8 — bốn dòng ngân sách bỏ hậu tố "isolated"; 675 ns giờ gắn với `nohz_full` chứ không với "lõi cô lập"
- [x] `docs/DESIGN.md` §1 và §2 — hai câu mở đầu mang con số 675 ns
- [x] `docs/GUIDE.md` §1 — dòng khuyến nghị cho người triển khai, kèm ngưỡng p99.99
- [x] `docs/decisions/ADR-0021-nohz-full-leaves-section-9.md` — **0020 đã bị plan `pre-session-routing` đặt trước**, nên lấy 0021 (§5: số không dùng lại)
- [x] `scripts/check-machine.sh` — dòng gate **đảo chiều**, chứng minh cả bốn nhánh
- [x] `STATUS.md` — item 22, thu hẹp chứ không đóng
- [ ] ~~`CHANGELOG.md`~~ — **không.** Phạm vi tự khai của file đó là *"thay đổi public API và hành vi quan sát được của crate đã phát hành"*; việc này không đụng dòng code thư viện nào. Ô này trong plan là thừa và ghi lại ở đây thay vì lặng lẽ bỏ
- [x] `[to testing-skills]` — hai case, cả hai trong `measured-costs.md`

## Bẫy đã lường trước

| Bẫy | Cái canh nó |
|---|---|
| Tin `scaling_cur_freq` và kết luận "hạ xung". **Đã suýt xảy ra hôm nay** — sysfs đọc 2.24 GHz trên một lõi đang chạy hết công suất | `user_loop` trong cùng chương trình: hai lõi cùng tốc độ thì cái đọc được là sai, không phải cái chạy |
| Đặt tên nhánh theo cái mình *định* boot vào, rồi đọc kết quả theo cái tên đó | Script in `/proc/cmdline` và `/sys/devices/system/cpu/{isolated,nohz_full}` cho mỗi lần chạy, và bảng ghi cái **đọc được**, không ghi cái đặt tên |
| Nhánh `nohz_full` đọc ra miễn phí vì tick không thực sự dừng | `isolcpus` trên cùng lõi giữ điều kiện ≤1 tác vụ; và cột `ticks(LOC)` của script, `[đo 2026-08-31]` đọc **3761 / 3740** trên lõi thường và **4 / 4** trên lõi cô lập |
| Cái canh ngay trên đây tự nó là false green. Bản đầu dùng `/^LOC:/`, mà `/proc/interrupts` căn phải cột đầu nên dòng bắt đầu bằng dấu cách: nó in delta **0 cho cả bốn lõi** trên đúng lần boot mà hai lõi tick ba triệu lần | Chính con số phải khác nhau giữa các lõi. Một cột hằng số 0 không phải "tất cả đều tickless", nó là "không đọc được gì" — và hai cái đó nhìn giống hệt nhau |
| Nhận một nguyên nhân vì có một cái núm nhúc nhích cùng nó — `CLAUDE.md` §10 | Bốn nhánh, một biến đổi mỗi lần, trong **cùng một** lần boot |
| Quên trả máy về §9 và ngày mai đo một con số latency trên máy đã hỏng thiết lập | Bước 5, và `check-machine.sh` được đọc chứ không được giả định |
| Số đo trong lần boot thí nghiệm bị dùng như số công bố | Máy FAIL `check-machine.sh` trong lần boot đó, và bài viết dán nhãn A/B |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Máy không boot được với dòng mới | Thấp | GRUB giữ entry cũ; dòng chỉ thêm/bớt tham số cô lập, không đụng `root=` |
| Chủ máy phải reboot hai lần | Chắc chắn | Thiết kế đã ép xuống còn hai; ba lần boot riêng từng cờ đã bị loại |
| VSCode và các tiến trình khác làm nhiễu (`check-machine.sh` hôm nay FAIL "machine is quiet", `code` chiếm 21% một lõi) | Trung bình | Các nhánh chạy trên lõi cô lập hoặc `taskset` cố định, và `user_loop` bắt được nhiễu; nhưng phải **đóng bớt** trước bước 4 và đọc lại `check-machine.sh` |
| Kết quả là "cả ba đều tốn" và §9 không có gì rẻ để khuyến nghị | Thấp | Vẫn là câu trả lời; §9 khi đó nói rõ cái giá thay vì im lặng như hiện nay |

## Ngoài phạm vi

- ~~**Đo jitter mà cô lập mua được.**~~ **Chuyển VÀO phạm vi, 2026-08-31, sau bước 4.**
  Lý do của việc sửa plan: bước 4 cho ra kết quả `nohz_full` là toàn bộ cái giá, và câu
  hỏi tiếp theo lập tức là *có nên bỏ nó khỏi §9 không*. Đảo một dòng §9 chỉ dựa trên
  trung vị là **đúng cái sai mà bài viết cũ đã cảnh báo** — nó viết rằng một cái đuôi mà
  cô lập cắt được "hoàn toàn có thể đáng giá 175 ns trung vị". Quyết định cần cả hai nửa.
  Và lần boot này là **cấu hình duy nhất** có `cpu4` với `cpu6` chỉ khác nhau đúng một
  biến `nohz_full`, cả hai đều `isolcpus`: đo sau này tốn thêm hai lần reboot nữa.
- **`mitigations=off`.** Cũng cần reboot, cũng ảnh hưởng đúng đường vào/ra kernel này,
  và là một **quyết định bảo mật** chứ không phải một phép đo. Nó ở item 22 từ đầu và
  ở nguyên đó.
- **`tools/w2w`.** Số wire-to-wire là item 6.
- **Sửa `Engine` hay `affinity`.** Không có dòng code thư viện nào trong plan này.

## Nhật ký giao hàng

**2026-08-31 — xong bước 1, 2, 4, 4b. Còn bước 5 (chủ máy reboot) và duyệt ADR-0021.**

**Bước 1–2, không tốn reboot** (`7eb9a53`). `scripts/measure-isolation-cost.{c,sh}` vào repo và
tái lập đúng bảng: 199 ns trên lõi thường, 361 trên lõi cô lập, `user_loop` bằng nhau. Ba điều
chốt được ngay ở đây: **không phải xung nhịp**, **toàn bộ nằm ở vào/ra kernel** (+81% trên một
`getpid` trần), **không phải ngắt** — lõi cô lập nhận ít hơn 3757 ngắt timer mà vẫn chậm hơn.
Dự đoán được ghi vào plan trước khi reboot.

**Bước 4, một lần reboot** (`d0c5634`). `isolcpus=4,6,12,14 rcu_nocbs=7,15 nohz_full=4,12`.
`cpu5` không gì 501.8 ns · `cpu6` `isolcpus` **494.8** · `cpu7` `rcu_nocbs` 498.2 ·
`cpu4` +`nohz_full` **670.7**. Dự đoán **trúng cả bốn nhánh**. `bench.sh` gate tự bắt được:
bốn case `turn` đỏ `OVER BASELINE` trên `cpu4`, xanh trên ba lõi kia, mà không được cho biết gì
về cô lập.

**Bước 4b — plan được sửa và duyệt lại giữa chừng**, vì bước 4 làm nảy ra câu hỏi *có nên bỏ
`nohz_full` khỏi §9 không*, và trả lời câu đó chỉ bằng trung vị là đúng cái sai bài viết cũ đã
cảnh báo. `nohz_full` **thua ở p50, p99 và cả p99.9**, chỉ thắng từ p99.99. `over_1us` khớp
`LOC` một-một: 1130/1283, 1078/1281, 1120/1281, **2/2**.

**Hai lần suýt sai, và chúng đáng hơn kết quả.** `scaling_cur_freq` đọc 2.24 GHz cho một lõi
đang chạy hết công suất — một lời giải thích gọn gàng và sai — bị `user_loop` bác bỏ. Và **cái
canh chống false green tự nó là false green**: `awk '/^LOC:/'` không khớp gì vì `/proc/interrupts`
căn phải cột đầu, in delta 0 cho cả bốn lõi trên đúng lần boot mà hai lõi tick ba triệu lần. Cả
hai thành `[to testing-skills]` trong `measured-costs.md`.

**Bước 5 — và nó không còn là "khôi phục".** Chủ dự án duyệt ADR-0021, nên máy **không** quay
về dòng cũ: nó boot vào dòng §9 **mới**, đúng cái ADR vừa quyết định.

```
quiet splash isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1
```

Bản lưu dòng cũ (có `nohz_full`) vẫn nằm ở `/etc/default/grub.fixbolt-s9`.

**Bước 5 xong, và nó tìm ra thêm một thứ không ai đi tìm.** Sau reboot,
`check-machine.sh` đọc **`pass 11 fail 0 unknown 1`** và `bench.sh --strict` **OK** —
9/9 target, 0 vượt baseline, 0 thiếu baseline. Nhưng:

| Lần boot | `nohz_full` | `cpu5` `getpid` | `cpu5` `Engine::turn` |
|---|---|---|---|
| §9 cũ | `6,7,14,15` | 198.36–199.04 ns | ~501 ns |
| thí nghiệm | `4,12` | 198.87 ns | 501.8 ns |
| §9 mới (ADR-0021) | **không có** | **154.62 ns** | **455.7 ns** |

**`cpu5` không mang cờ nào trong cả ba lần boot.** `nohz_full` có một cái giá **toàn cục**,
không chỉ trên lõi mang nó: ~44 ns mỗi lần vào kernel cho **mọi** CPU, cộng thêm ~155 ns nữa
cho những CPU thực sự mang nó. Ba phép đo độc lập cho cùng một hằng số: `getpid` +44,
`recv` +47, `turn` +46 — hình dạng của *một chi phí cố định mỗi lần vào kernel*, không phải
hình dạng của khối lượng công việc. `user_loop` không đổi (1.0577 so với 1.0578), nên lại
một lần nữa không phải xung nhịp.

Cơ chế đã biết: `NO_HZ_FULL` bật context tracking và `VIRT_CPU_ACCOUNTING_GEN` ở mức **toàn
hệ thống** khi có bất kỳ CPU nào dùng nó. **Cơ chế đó không được kiểm chứng ở đây** — nó là
lời giải thích, con số mới là phép đo.

### Bước 6b — ghi lại baseline, và dự đoán ghi trước

`benches/baselines.tsv` được ghi trên máy **có** `nohz_full`, cột verdict của nó nói
`pass 10 fail 0 unknown 1` — một checklist không còn tồn tại. `DESIGN.md` §8 mang 505 ns,
máy giờ chạy 456. Baseline là **trần** nên không gate nào đỏ, nhưng con số thì đã cũ.

**Dự đoán, ghi trước khi chạy 25 lượt:** chỉ những case **chạm syscall** giảm (~9%) —
`recv on a quiet socket` và ba dòng `engine turn`. Những case thuần user space —
`parse`, `encode`, `ring`, `groups`, `deliver` — **không được nhúc nhích** ngoài biên độ
sẵn có của chúng.

**Nếu `parse` cũng giảm 9% thì lời giải thích trên là sai** và cái đổi không phải
`nohz_full`. Đó là chỗ dự đoán này bác bỏ được chính nó.

**Bước 6b xong, và dự đoán trúng theo cách bác bỏ được nó.** 24/25 lượt hợp lệ — lượt duy nhất
bị loại là **lượt đầu tiên sau reboot**, gnome-shell còn 29% một lõi. Mười hai case thuần user
space nhúc nhích −1.7% đến +4.8% **không theo hướng nào** (`parse` −0.7%, `ring, one way` −1.7%,
`encode 1 group` +4.8% — sai dấu cho bất kỳ hiệu ứng hệ thống nào và chỉ vừa ra khỏi biên độ
3.8% của chính nó). Bốn case chạm syscall giảm cùng nhau: **470.9→420.5, 500.3→448.9,
2002.9→1807.1, 8139.4→7333.5**. Nếu `parse` cũng giảm 10% thì lời giải thích đã sai.

`benches/baselines.tsv` lấy bốn số mới, n=24, margin 1.10 (max/median 1.007–1.013), verdict
`pass 11 fail 0 unknown 1`; **chứng minh bằng reversal** — đặt một dòng về 400.0 thì đúng
**một trong bốn** case đỏ, đúng case và đúng giới hạn của nó. Mười hai dòng kia giữ nguyên số
và giữ verdict `pass 10` cũ, và file ghi rõ vì sao.

`DESIGN.md` §8 dòng chủ đạo **505 → 449 ns**; §9, `GUIDE.md`, `PRD.md` và ngưỡng N của D8
(thắng ở N=1, thua từ **N=11**, trước là N=8) đi theo. Con số **703 ns** của 2026-08-30 giờ
giải thích được: 449 + ~45 (thuế toàn cục) + ~155 (thuế trên lõi) + chênh giữa hai chương
trình — **chính phần tinh chỉnh của §9 chiếm khoảng một phần ba con số mà thiết kế này lấy
làm ngân sách.**

**CI xanh, gọi tên theo §9 hộp cuối.** Run
[`33404799598`](https://github.com/tmthang86/fixbolt/actions/runs/33404799598), commit
`a750592`, **9/9 job success**, tổng 2 phút 31 giây (14:48:35Z → 14:51:06Z, đọc từ timestamp
của API chứ không suy từ hoạt động của chính tôi — bài học đã trả giá ngày 2026-08-31).
Hai run trên cùng SHA, cả hai success.

**Chưa xong, nói rõ ra:**

- **~~Chưa reboot vào dòng mới.~~** `check-machine.sh` phải đọc lại `pass 11 fail 0` sau đó, và
  `bench.sh --strict` phải xanh với đúng `baselines.tsv` hiện có — chúng vốn được ghi trên
  những lõi **không** `nohz_full`, nên nếu ADR đúng thì các số cũ không đổi nghĩa. Đó là phép
  kiểm chứng cuối cùng và nó **chưa chạy**.
- **`isolcpus` dưới tải** không đo. Giữ vì nó miễn phí, không phải vì đo được lợi ích.
- **Item 22 thu hẹp, không đóng**: `mitigations=off`, `recvmmsg`/`io_uring` vẫn nguyên.
