# Các mitigation CPU lấy mất bao nhiêu của một syscall

> **Loại:** Plan · **Ngày:** 2026-09-01 · **Trạng thái:** Chờ duyệt
> **Phạm vi:** open item 22 — đòn bẩy cuối cùng còn lại của nó

## Bối cảnh

`STATUS.md` item 22 liệt kê các đòn bẩy theo thứ tự đã đo, và một cái vẫn mang nhãn
**`[unproven]`** từ 2026-08-30: *"`mitigations=off` — full mitigations đang bật, riêng
`vmscape` làm một IBPB ở mỗi lần trả về từ syscall, cần reboot và là một quyết định bảo mật"*.

Nó chưa bao giờ được đo. Và bài học của
[which-isolation-flag-costs](2026-08-31-which-isolation-flag-costs.md) áp thẳng vào đây:
`nohz_full` được §9 khuyến nghị vì lý do đúng, tốn 36% ở thao tác thống trị, và **không ai đặt
đồng hồ lên hai thứ đó cùng lúc trong một ngày**. Mitigation là cùng hình dạng — một thiết lập
toàn hệ thống, nằm đúng trên đường vào/ra kernel, chưa từng được đo so với thao tác nó thay đổi.

**Và §9 hoàn toàn không nhắc tới mitigation.** Hai máy cùng đọc `pass 11 fail 0` có thể chênh
nhau đúng bằng con số này, và checklist sẽ không nói gì.

## Những gì đã biết chắc

`[đo 2026-09-01]` đọc thẳng `/sys/devices/system/cpu/vulnerabilities/` trên máy §9:

| Lỗ hổng | Mitigation đang chạy |
|---|---|
| `vmscape` | **IBPB before exit to userspace** |
| `retbleed` | untrained return thunk; SMT disabled |
| `spectre_v2` | Retpolines; IBPB: conditional; STIBP: always-on; RSB filling |
| `spec_rstack_overflow` | Safe RET |
| `spec_store_bypass` | disabled via prctl |
| `spectre_v1` | usercopy/swapgs barriers |

`vmscape` là cái nằm trên đường đo: một IBPB ở **mỗi** lần ra khỏi kernel.

Nền để so, tất cả trên dòng §9 của [ADR-0021](../decisions/ADR-0021-nohz-full-leaves-section-9.md),
`check-machine.sh` đọc `pass 11 fail 0 unknown 1`:

| Case | ns |
|---|---|
| `user_loop` (không bao giờ vào kernel) | 1.0577 ns/iter |
| `getpid` trần | **154.5 ns/call** |
| `recv on a quiet socket` | 420.5 ns |
| `engine turn, 1 idle sessions` | 448.9 ns |
| `presession sweep, 16 quiet sockets` | 6819.5 ns |

Công cụ đã có và đã commit: `scripts/measure-isolation-cost.{c,sh}` (hai vòng lặp + chế độ
`--jitter`), `crates/engine/benches/turn.rs`, `crates/engine/benches/presession.rs`,
`scripts/bench.sh --strict`.

## Cách làm

**Khác bài `nohz_full` ở một điểm quan trọng: mitigation là toàn hệ thống, không gán được
cho từng CPU.** Nên đây bắt buộc là A/B **giữa hai lần boot**, và bài học của chính repo này
nói A/B giữa hai lần boot là chỗ dễ sai. Cái chống lại điều đó là `user_loop`.

Hai nhánh, làm tuần tự và có điều kiện:

| Nhánh | Dòng lệnh thêm vào | Trả lời |
|---|---|---|
| **A** | `mitigations=off` | **Tổng** giá của tất cả mitigation |
| **B** | `vmscape=off` | Riêng cái cơ chế được nêu tên |

**Nhánh B chỉ chạy nếu nhánh A cho ra chênh lệch đáng kể.** Nếu tổng đã nhỏ thì không có gì
để quy trách nhiệm, và một lần reboot nữa là lãng phí. Ngưỡng ghi trước ở mục Dự đoán.

Dòng lệnh nhánh A, thêm vào dòng §9 hiện tại:

```
quiet splash isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1 mitigations=off
```

**`mitigations=off` là tạm thời và chỉ để đo.** Nó bị gỡ ở bước cuối. Con số nói mitigation
tốn bao nhiêu; **chạy sản phẩm ở trạng thái đó là một quyết định khác**, thuộc về chủ máy, và
plan này không đưa ra khuyến nghị nào về việc đó.

File sẽ tạo hoặc sửa: `docs/reference/measured-costs.md`, `docs/DESIGN.md` §9 (nếu cần một
dòng mới), `STATUS.md` item 22, và một ADR nếu §9 đổi.

## Bất biến bị đụng tới

Không có dòng code thư viện nào thay đổi. Hai điều vẫn liên quan:

- **Điều 10** — mọi số phải nêu benchmark, máy, và thiết lập §9. Trong nhánh A máy **vẫn**
  đọc `pass 11 fail 0`, vì §9 không có dòng nào về mitigation. **Đó chính là một phát hiện,
  không phải một chi tiết:** checklist không phân biệt được hai cấu hình chênh nhau đúng cái
  đang đo. Mọi số của nhánh A phải mang nhãn *`mitigations=off`* bằng chữ, chứ không dựa vào
  verdict của `check-machine.sh`.
- **Điều 4** — kết quả này thuộc về `hft`. `standard` chặn trong kernel nên số syscall mỗi
  giây thấp hơn hàng bậc, và cái giá này không nằm trên đường tới hạn theo cùng cách.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | Ghi **dự đoán** và ngưỡng quyết định chạy nhánh B, trước khi reboot | — |
| 2 | ✅ Chủ máy boot vào nhánh A (`mitigations=off`) | 1 |
| 3 | ✅ Đo: `measure-isolation-cost.sh`, `bench.sh` (không `--strict`), `--jitter` | 2 |
| 4 | ✅ Nhánh B (`vmscape=off`) — **bác bỏ cơ chế được nêu tên** | 3 |
| 5 | ✅ Chủ máy khôi phục dòng §9; `check-machine.sh` và `bench.sh --strict` phải đọc lại đúng như trước | 3 hoặc 4 |
| 6 | ✅ Viết `measured-costs.md`; §9 và ADR nếu cần; đóng item 22 | 5 |

## Dự đoán, ghi trước bước 2

| Đại lượng | Dự đoán |
|---|---|
| `user_loop` | **không đổi**, trong 0.5% của 1.0577 ns/iter |
| `getpid` trần | **giảm**, và đây là nơi hiệu ứng lớn nhất |
| `recv`, `turn`, `presession sweep` | giảm cùng một lượng tuyệt đối mỗi lần vào kernel |
| `parse`, `encode`, `ring`, `groups` | **không đổi** — chúng không vào kernel |

**Ngưỡng cho nhánh B:** chạy nó nếu `getpid` giảm **≥ 20 ns**. Dưới mức đó thì không có gì
đủ lớn để đáng một lần reboot nữa để quy trách nhiệm.

**Cái bác bỏ phép đo, chứ không phải bác bỏ dự đoán:** `user_loop` lệch quá 0.5%. Khi đó hai
lần boot không so được với nhau và **không con số nào trong lần chạy đó dùng được** — đúng như
lần trước, khi `scaling_cur_freq` đưa ra một lời giải thích gọn ghẽ và sai.

**Cái sẽ làm tôi ngạc nhiên và phải viết ra:** `parse` hoặc `ring` cũng giảm. Chúng thuần
user space; nếu chúng động thì cái đổi giữa hai lần boot không phải mitigation.

## Cách kiểm chứng

- **Bước 3 đạt khi `user_loop` khớp và ít nhất một case syscall đổi.** Cả hai đều đổi thì
  phép đo hỏng; không cái nào đổi thì mitigation không tốn gì và đó cũng là một câu trả lời.
- **Các case thuần user space là nhóm đối chứng.** 12 case trong `bench.sh` không chạm syscall;
  chúng phải nằm trong biên độ của chính chúng. Đây đúng là phép thử đã dùng cho `nohz_full`
  và nó bác bỏ được chính nó.
- **Bước 5 đạt khi `check-machine.sh` đọc lại `pass 11 fail 0 unknown 1` VÀ `bench.sh --strict`
  xanh với đúng `baselines.tsv` hiện có.** Baseline được ghi với mitigation **bật**; nếu chúng
  vẫn xanh sau khi khôi phục thì máy thực sự về nguyên trạng, và điều đó phải được **đọc**.

## Tài liệu phải cập nhật

- [x] `docs/reference/measured-costs.md` — ba nhánh, nhóm đối chứng, và bài học chung
- [x] `docs/DESIGN.md` §9 — dòng mới, **yêu cầu mitigation BẬT**
- [x] `docs/decisions/ADR-0023-section-9-records-the-cpu-mitigations.md`
- [x] `scripts/check-machine.sh` — dòng gate, cả ba nhánh đã chứng minh
- [x] `docs/GUIDE.md` — người triển khai cần biết con số này tồn tại
- [x] `benches/baselines.tsv` — ghi chú vì sao các dòng cũ giữ `pass 11`
- [x] `STATUS.md` item 22 — **đóng**
- [x] `[to testing-skills]` — một case, trong `measured-costs.md`

## Bẫy đã lường trước

| Bẫy | Cái canh nó |
|---|---|
| A/B giữa hai lần boot, và một thứ khác cũng đổi | `user_loop` là mỏ neo; 12 case user space là nhóm đối chứng |
| `check-machine.sh` vẫn đọc `pass 11 fail 0` ở nhánh A, và số bị coi là công bố được | Mọi số nhánh A dán nhãn `mitigations=off` bằng chữ. Chính lỗ này là một phát hiện phải viết ra |
| SMT bật lại sau reboot và không ai để ý | `fixbolt-machine on` + `smtoff` rồi **đọc** `check-machine.sh`, không giả định |
| Lượt `bench.sh` đầu sau reboot tự loại | Đã biết: gnome-shell còn khởi động. Bỏ lượt đầu, chạy N+1 |
| Quên gỡ `mitigations=off` và ngày mai đo trên máy đã hạ bảo mật | Bước 5, và backup `/etc/default/grub.fixbolt-s9` |
| Đọc kết quả thành một khuyến nghị bảo mật | Plan này đo một cái giá. Nó **không** khuyến nghị chạy sản phẩm với mitigation tắt |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Máy chạy tạm thời với mitigation tắt | Trung bình | Chỉ trong lúc đo, máy cá nhân, không dịch vụ công khai; gỡ ở bước 5 |
| Ba lần reboot | Chắc chắn | Nhánh B có điều kiện, nên có thể chỉ còn hai |
| Kết quả là "không đáng kể" | Thấp | Vẫn là câu trả lời, và nó gỡ nhãn `[unproven]` khỏi item 22 |

## Ngoài phạm vi

- **Khuyến nghị bảo mật.** Không có.
- **Item 13** (release profile) và **item 12** (SIMD) — A/B cùng máy, không cần reboot, việc khác.
- **Item 6** (`tools/w2w`) — cần máy phát tải riêng, không làm một mình được.

## Nhật ký giao hàng

**2026-09-01 — nhánh A xong. Con số lớn hơn mọi thứ đã đo trong repo này.**

`mitigations=off`, `check-machine.sh` đọc **`pass 11 fail 0 unknown 1`** — đúng như plan ghi
trước: checklist không phân biệt được. Mọi số dưới đây mang nhãn `mitigations=off`.

**Mỏ neo giữ:** `user_loop` 1.0566–1.0569 ns/iter so với 1.0577–1.0581 khi mitigation bật —
lệch **0.1%**, trong ngưỡng 0.5%. Hai lần boot so được với nhau.

| Case | mitigation BẬT | mitigation TẮT | đổi |
|---|---|---|---|
| `getpid` trần | 154.5 ns | **59.45 ns** | **−61.5%** |
| `recv on a quiet socket` | 420.5 | **156.9** | **−62.7%** |
| `engine turn, 1 idle sessions` | 448.9 | **175.2** | **−61.0%** |
| `engine turn, 4 idle sessions` | 1807.1 | 712.3 | −60.6% |
| `engine turn, 16 idle sessions` | 7333.5 | 2985.6 | −59.3% |
| `presession sweep, 1 quiet sockets` | 435.9 | 165.4 | −62.0% |
| `presession sweep, 16 quiet sockets` | 6819.5 | 2610.3 | −61.7% |

**Nhóm đối chứng — 13 case thuần user space:** −4.1% đến +4.1%, **không theo hướng nào**.
`parse NewOrderSingle` +2.0%, `ring, one way` +0.3%, `encode ExecutionReport` −0.3%,
`SendingTime` và `inline deliver` đúng bằng 0.0%. Kể cả
`presession, read and route an identity` — thuần byte, không syscall — đứng yên ở −0.2%.
Đây là điều bác bỏ được lời giải thích, và nó không bị bác bỏ.

**Đuôi cũng đi theo.** `--jitter`, 5 triệu lần gọi: p50 **216 → 80 ns**, và **p99.99 đi từ
2848 xuống 88** — phẳng hết ra tới p99.99, vì cái từng chi phối đuôi giờ nhỏ hơn tick.

**Và một điều chưa giải thích được, nên nó thành lý do chạy nhánh B.** Tiết kiệm tuyệt đối
**không bằng nhau giữa các syscall**: `getpid` bớt **95 ns**, `recv` bớt **264 ns**. Nếu cái
giá chỉ là một IBPB cố định mỗi lần ra khỏi kernel thì cả hai phải bớt như nhau. Giả thuyết có
tên: `getpid` là syscall lá, gần như không có nhánh gián tiếp; `recv` đi qua
`sock->ops->recvmsg` và chuỗi dispatch của giao thức, nên nó trả thêm **retpoline** theo số
nhánh gián tiếp của chính nó. **Đó là giả thuyết, không phải phép đo**, và nhánh B tách được:
`vmscape=off` một mình để lại retpoline bật.

Ngưỡng ghi trước là ≥20 ns; đo được 95. **Nhánh B chạy.**

### Dự đoán cho nhánh B, ghi trước khi chạy

Trạng thái nhánh B, đọc từ `/sys`: `vmscape: Vulnerable`, `spectre_v2: Retpolines; IBPB:
conditional; STIBP: always-on; RSB filling`, `retbleed: untrained return thunk; SMT disabled`,
`spec_rstack_overflow: Safe RET`. Chỉ đúng một thứ bị tắt.

Nếu giả thuyết đúng — IBPB là **chi phí cố định mỗi lần ra khỏi kernel**, còn phần dư của
`recv` là retpoline theo số nhánh gián tiếp của chính nó — thì:

| Đại lượng | Dự đoán |
|---|---|
| `getpid` | thu về **~95 ns**, tức về gần **59** ns của nhánh A |
| `recv` | thu về **cùng ~95 ns tuyệt đối**, tức **~325** ns, **không** phải 157 |
| `engine turn, 1` | ~354 ns |
| 13 case user space | không đổi, như nhánh A |

**Cái bác bỏ giả thuyết:** `recv` về thẳng ~157 ns. Khi đó `vmscape` giải thích toàn bộ và
"retpoline theo số nhánh gián tiếp" là sai — cái chênh giữa `getpid` và `recv` phải tìm lời
giải khác.

**Cái làm hỏng phép đo:** `user_loop` lệch quá 0.5% so với 1.0577.

**2026-09-01 — nhánh B xong, và nó bác bỏ cơ chế mà item 22 nêu tên từ 2026-08-30.**

`vmscape=off` một mình. `/sys` xác nhận đúng một thứ bị tắt: `vmscape: Vulnerable`, còn
`spectre_v2: Retpolines; IBPB: conditional; STIBP: always-on; RSB filling`,
`retbleed: untrained return thunk; SMT disabled`, `spec_rstack_overflow: Safe RET` vẫn bật.
`user_loop` 1.0577–1.0585, khớp baseline.

| Case | mitigation đầy đủ | `vmscape=off` | đổi |
|---|---|---|---|
| `getpid` trần | 154.5 ns | **154.5** | **0.0%** |
| `recv on a quiet socket` | 420.5 | 418.4 | −0.5% |
| `engine turn, 1 idle sessions` | 448.9 | 443.8 | −1.1% |
| `engine turn, 16 idle sessions` | 7333.5 | 7264.4 | −0.9% |
| `presession sweep, 16 quiet sockets` | 6819.5 | 6767.8 | −0.8% |

**`vmscape` thu về số không.** `getpid` đọc đúng 154.5 ns, không đổi một chữ số.

`STATUS.md` item 22 viết từ 2026-08-30: *"riêng `vmscape` làm một IBPB ở mỗi lần trả về từ
syscall"*. Câu đó có một cơ chế, có tên, đọc rất hợp lý — **và nó sai**. 61% nằm ở chỗ khác
trong bộ mitigation.

`[to testing-skills]` — **vặn đúng cái núm được nêu tên và không có gì nhúc nhích là cách rẻ
nhất để biết cơ chế mình đặt tên là sai.** Repo này đã ghi mặt kia của đồng xu — *một nguyên
nhân được chấp nhận vì có cái núm nhúc nhích cùng nó*. Đây là mặt còn lại, và nó rẻ hơn: một
lần boot, một phép đo, và một giả thuyết sống suốt hai ngày bị giết.

### Giả thuyết mới, và dự đoán cho nhánh C — ghi trước khi chạy

Còn lại trên Zen 2, theo thứ tự khả năng: `retbleed` (**untrained return thunk**) và
`spec_rstack_overflow` (**Safe RET**). Cả hai thêm việc vào **mọi lần return trong kernel**,
chứ không phải một lần cố định mỗi syscall — và **đó chính là thứ giải thích được cái mà
"IBPB cố định" không giải thích được**: `recv` chạy nhiều code kernel hơn `getpid`, nên nó
trả nhiều hơn. Tiết kiệm không đều (95 so với 264) là **bằng chứng ủng hộ họ return-thunk**
chứ không phải ủng hộ IBPB.

Nhánh C: `retbleed=off spec_rstack_overflow=off`, mọi thứ khác giữ nguyên.

| Đại lượng | Dự đoán |
|---|---|
| `getpid` | về gần **59–90 ns**, tức thu về phần lớn của 95 ns |
| `recv` | thu về **nhiều hơn** `getpid` tính bằng ns tuyệt đối |
| 13 case user space | không đổi |

**Cái bác bỏ nó:** `getpid` vẫn ~154. Khi đó cái tốn tiền là `spectre_v2` (retpoline / STIBP /
RSB filling) và phải có nhánh D.

**2026-09-01 — nhánh C xong. Giả thuyết mới đúng, và đúng toàn phần.**

`retbleed=off spec_rstack_overflow=off`. `/sys` xác nhận: `retbleed: Vulnerable`,
`spec_rstack_overflow: Vulnerable`, **`vmscape: Mitigation: IBPB before exit to userspace`
bật lại**, `spectre_v2: Retpolines; IBPB: conditional; RSB filling` vẫn nguyên.
`user_loop` 1.0563–1.0571, khớp baseline.

| Case | mitigation đầy đủ | A: tắt hết | **C: chỉ return-thunk tắt** | C so với đầy đủ |
|---|---|---|---|---|
| `getpid` trần | 154.5 | 59.45 | **59.46** | **−61.5%** |
| `recv on a quiet socket` | 420.5 | 156.9 | **158.2** | −62.4% |
| `engine turn, 1 idle sessions` | 448.9 | 175.2 | **176.0** | −60.8% |
| `engine turn, 4 idle sessions` | 1807.1 | 712.3 | 713.1 | −60.5% |
| `engine turn, 16 idle sessions` | 7333.5 | 2985.6 | 3001.8 | −59.1% |
| `presession sweep, 1 quiet sockets` | 435.9 | 165.4 | 165.3 | −62.1% |
| `presession sweep, 16 quiet sockets` | 6819.5 | 2610.3 | 2602.2 | −61.8% |

**Nhánh C thu về trong 0.5% của nhánh A ở mọi dòng.** Nghĩa là **toàn bộ 61% là họ
return-thunk của AMD** — `retbleed` và `spec_rstack_overflow` — với `vmscape` vẫn làm IBPB
mỗi lần ra khỏi kernel và retpoline vẫn bật. Hai thứ đó cộng lại tốn **dưới 1%**.

Nhóm đối chứng: 13 case user space, −3.1% đến +1.4%, không hướng.

**Và nó giải thích được cái nhánh A không giải thích nổi.** Tiết kiệm không đều — `getpid`
95 ns, `recv` 264 ns — vì đây **không** phải chi phí cố định mỗi syscall: return thunk và
Safe RET thêm việc vào **mọi lần return trong kernel**, nên nó tỉ lệ với lượng code kernel
mà syscall đó chạy. `getpid` là syscall lá; `recv` đi qua cả chuỗi dispatch.

**Không tách được `retbleed` khỏi `spec_rstack_overflow`, và nói rõ vì sao không làm:** chúng
là **cùng một lớp cơ chế** — cả hai viết lại đường return của kernel — nên tách ra cho hai con
số nhỏ hơn chứ không cho một kết luận khác. Hai lần reboot nữa không mua thêm được quyết định
nào.

**Một thứ nhánh C tắt nhiều hơn tên gọi, ghi lại thay vì giấu:** `retbleed=off` cũng bỏ luôn
`STIBP: always-on` khỏi dòng `spectre_v2` (nhánh B còn, nhánh C không). STIBP bảo vệ giữa hai
luồng của một lõi, và **SMT đang tắt** theo §9, nên gần như chắc chắn nó không tốn gì ở đây —
nhưng "gần như chắc chắn" không phải phép đo, nên nó nằm ở đây.

**2026-09-01 — bước 5 và 6 xong, plan đóng.**

**Bước 5, và nó được *đọc* chứ không giả định.** Máy boot lại vào dòng §9 của ADR-0021, mọi
mitigation bật lại (`/sys` xác nhận `vmscape: IBPB`, `spec_rstack_overflow: Safe RET`,
`retbleed: untrained return thunk; SMT disabled`). Baseline vốn ghi khi mitigation **bật**, nên
chúng là phép thử máy đã về nguyên trạng hay chưa: `recv on a quiet socket` **419.6** so với
baseline 420.5, `engine turn, 1` **443.7** so với 448.9, `presession, read and route an
identity` **84.0** so với 84.0. `bench.sh --strict` **OK**.

**Bước 6.** [ADR-0023](../decisions/ADR-0023-section-9-records-the-cpu-mitigations.md) thêm một
dòng vào §9 và vào `check-machine.sh`. Dòng đó **PASS khi mitigation BẬT** — hướng này là chủ ý
và **không phải khuyến nghị bảo mật**: baseline được ghi ở trạng thái đó, và một máy tắt
mitigation đọc **thấp hơn** mọi baseline chạm syscall khoảng 60%, tức là **qua**, vì baseline là
trần. Cổng bench không nhìn thấy được; phải có thứ khác nhìn.

Nó đọc `/sys` chứ không đọc `/proc/cmdline`, vì `[đo]` hai cái khác nhau — `retbleed=off` còn
bỏ luôn `STIBP: always-on` khỏi dòng `spectre_v2`, điều mà không cách đọc command line nào thấy.

Cả ba nhánh của dòng gate đã chứng minh bằng sysfs giả: `mitigated → PASS`,
`vulnerable → FAIL disabled: retbleed spec_rstack_overflow`, `missing → UNKNOWN`. Máy hiện đọc
**`pass 12 fail 0 unknown 1`**.

**Item 22 đóng.** Còn lại của nó là `recvmmsg`/`io_uring` với `SQPOLL` và item 14 — tức là
chính cái syscall, giờ đã biết phụ phí mitigation của nó.
