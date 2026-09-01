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
| 2 | Chủ máy boot vào nhánh A (`mitigations=off`) | 1 |
| 3 | Đo: `measure-isolation-cost.sh`, `bench.sh` (không `--strict`), `--jitter` | 2 |
| 4 | Nếu vượt ngưỡng: chủ máy boot vào nhánh B (`vmscape=off`), đo lại | 3 |
| 5 | Chủ máy khôi phục dòng §9; `check-machine.sh` và `bench.sh --strict` phải đọc lại đúng như trước | 3 hoặc 4 |
| 6 | Viết `measured-costs.md`; §9 và ADR nếu cần; đóng item 22 | 5 |

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

- [ ] `docs/reference/measured-costs.md`
- [ ] `docs/DESIGN.md` §9 — một dòng cho mitigation, kể cả khi câu trả lời là "không đáng đổi"
- [ ] `docs/decisions/ADR-00NN` — chỉ nếu §9 đổi khuyến nghị
- [ ] `STATUS.md` item 22
- [ ] `[to testing-skills]` nếu có bài học chung

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

Chưa mở.
