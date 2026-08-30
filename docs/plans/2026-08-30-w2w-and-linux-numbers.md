# `tools/w2w`, và con số đầu tiên đo trên Linux

> **Loại:** Plan · **Ngày:** 2026-08-30 · **Trạng thái:** Đã duyệt (2026-08-30)
> **Phạm vi:** open item 6, 11, 12, 13, 15 — `DESIGN.md` §7 bước 7

## Bối cảnh

`DESIGN.md` §8 có một bảng ngân sách latency đầy đủ. **Không một dòng nào trong đó được đo ở
dự án này** — tất cả là số từ tài liệu của người khác. `DESIGN.md` §9 tự nói rằng một con số
latency lấy từ laptop macOS không phải là một con số. Và `CLAUDE.md` §2 điều 10 nói không có
số hiệu năng nào được phép tồn tại nếu thiếu benchmark, máy, và cấu hình §9.

Nói cách khác: **dự án hiện không có quyền công bố bất kỳ con số latency nào.** `tools/w2w` là
thứ đổi chuyện đó, và nó cũng là thứ duy nhất đóng được hai open item khác:

- **item 15** — bất di bất dịch số 4 (*luồng engine không bao giờ ngủ trong kernel*) chưa có
  máy kiểm nào. Hai lần thử đều thất bại và đã ghi lại: `dtruss` bị macOS SIP từ chối, còn
  đọc symbol từ rlib thì **xanh kể cả khi nhét `thread::sleep` vào vòng lặp**, vì `Engine` và
  `serve` là generic nên không bao giờ được sinh mã vào thư viện. Câu trả lời đúng là trace
  syscall của **một binary cụ thể trên Linux** — và `w2w` chính là binary đó.
- **item 6** — "cần một máy Linux". Phiên này *đang chạy trên* Linux 6.18 x86_64. Nhưng đó là
  container 4 vCPU, **không** có `isolcpus`, không ghim luồng, không tắt C-state — nên nó
  **không phải** cái máy §9 mô tả. Nó đủ để chạy `w2w` và lấy syscall trace; nó **không** đủ
  để công bố một con số latency. Plan này tách hai chuyện đó ra thay vì gộp.

Item 11 (serialise 93,8 ns so với cổng 60 ns) và item 13 (profile release để mặc định) đi kèm
vì cả hai chỉ có nghĩa khi đo trên Linux, và cả hai đã được cố ý hoãn tới đúng bước này.

Item 12 (SIMD/SWAR) **không** nằm trong phạm vi làm, chỉ nằm trong phạm vi *quyết định*: sau
bước đo, hoặc nó bị đóng vĩnh viễn, hoặc nó có dữ liệu để mở một plan riêng.

## Những gì đã biết chắc

- `DESIGN.md` §7 bước 7 là `tools/w2w`; bước 8 là `library`. Bước 6 (`engine`) đã đóng.
- **`Engine::turn` là một lượt không chặn**, và `crates/engine/tests/wire.rs` đã lái nó bằng
  tay qua socket thật — nên `w2w` không phải phát minh cách chạy engine, chỉ phải thêm vòng
  lặp và đo.
- `nanofix_engine::wait::Spin` đã tồn tại và có ghi chú là dành cho `w2w`; test dùng `Park`.
- **Chi phí đo được của một lần nhảy luồng:** inline 2,7 ns, ring một chiều 128,0 ns, khứ hồi
  242,5 ns, trên `NewOrderSingle` 163 byte, Apple M5, không ghim core.
  `crates/engine/benches/dispatch.rs` có assert.
- **Serialise trượt cổng:** 93,8 ns so với 60 ns công bố. Nguyên nhân đã xác định — 
  `Template::encode` tìm mỗi slot bằng cách quét tuyến tính danh sách của caller, nên chi phí
  là slots × parts. Hai hướng sửa đã ghi trong item 11: đánh index slot theo tag lúc build
  template, hoặc bắt caller đưa slot theo đúng thứ tự part.
- **Profile release đang là mặc định.** `Cargo.toml` không có `[profile.release]`: không
  `lto`, không `codegen-units = 1`, không PGO, không `#[cold]` trên nhánh lỗi.
- **Criterion đang bị hoãn** (ghi trong `STATUS.md`): benchmark hiện dùng harness 24 dòng
  không phụ thuộc, vì benchmark phải **assert**, còn Criterion chỉ đo. Cái giá là mất phát
  hiện outlier và khoảng tin cậy.
- **Số đo trên M5 dao động thật.** Cùng một binary, inline chạy từ 2,5 tới 4,9 ns giữa các
  lần. Trần đặt ở 15 ns chứ không phải 2×, vì *một trần chặt hơn độ tản của chính phép đo là
  một cổng đỏ ngẫu nhiên*.

## Cách làm

Chia làm hai nửa dứt khoát, vì chúng cần hai loại máy khác nhau và cho hai loại kết luận
khác nhau.

### Nửa A — những gì container này làm được (item 15, và cấu trúc của `w2w`)

`tools/w2w` là một **binary**, không phải thư viện: một acceptor thật, một initiator thật, nối
qua loopback, đo từ lúc byte rời tiến trình gửi tới lúc byte về tới tiến trình nhận.

1. Dựng binary với `wait::Spin` và `InlineDispatch`.
2. **Trace syscall bằng `strace -f -c` trên chính binary đó**, ở trạng thái ổn định. Đây là
   máy kiểm mà bất di bất dịch số 4 chưa từng có: nếu luồng engine ngủ trong kernel thì
   `epoll_wait`, `futex`, hay `nanosleep` sẽ hiện ra trong bảng đếm. Biến nó thành một script
   trong `scripts/`, chạy được lặp lại, chứ không phải một lần chạy tay.
3. **Chứng minh máy kiểm đó bằng đảo ngược** — nhét một `thread::sleep(0)` vào vòng lặp engine
   và xem script đỏ. Đây là bước bắt buộc: hai lần thử trước đã **xanh** với `sleep` bên trong,
   và đó chính là lý do item 15 vẫn mở.

Nửa A cho ra một **cổng bật/tắt** (có ngủ / không ngủ), không cho ra con số nào. Kết luận của
nó không phụ thuộc vào việc máy nhanh hay chậm, nên container này đủ tư cách.

### Nửa B — những gì cần một máy thật (item 6, 11, 13, và quyết định item 12)

Nửa này **không được bắt đầu** cho tới khi có một máy đúng `DESIGN.md` §9. Trên máy đó:

1. Đo baseline wire-to-wire, ghi đủ máy + cấu hình §9 + lệnh chạy.
2. **Item 13** — bật `lto = "fat"`, `codegen-units = 1`, đo **trước và sau từng cái một**. Một
   cái không cải thiện thì bỏ, không giữ vì "chắc là tốt".
3. **Item 11** — đo lại serialise trên Linux trước khi sửa. Có thể nó đã đạt 60 ns mà không cần
   làm gì; có thể nó tệ hơn. Chỉ sửa sau khi biết. Nếu phải sửa thì dùng hướng "index slot theo
   tag lúc build template", vì nó không đẩy gánh nặng sang caller.
4. **Item 12** — nhìn `benches/parse.rs` trên Linux. Nếu parse **không** nằm trên đường găng
   thì đóng item 12 vĩnh viễn với dữ liệu, chứ không đóng bằng ý kiến.
5. Thay bảng `DESIGN.md` §8 bằng số đo thật, từng dòng một, dòng nào chưa đo thì ghi rõ là
   chưa đo.

## Bất biến bị đụng tới

- **Số 4** (*luồng engine không bao giờ ngủ trong kernel*) — đây là plan biến điều này từ
  kiểm-bằng-tay thành kiểm-bằng-máy. Cho tới khi nửa A xong, nó vẫn là kiểm tay.
- **Số 1** (không cấp phát trên hot path). `w2w` là binary đo; bản thân nó không được cấp phát
  trong vòng đo. `benches/alloc.rs` không che được binary, nên phải có assert riêng.
- **Số 10** (không có số nào thiếu benchmark, máy, cấu hình §9). **Đây là điều luật trung tâm
  của plan này.** Mọi con số ra từ nửa A đều bị dán nhãn "container, không §9"; chỉ nửa B mới
  được công bố.
- **Số 6** (feature flag gate `mod`). `tools/w2w` phải không phá job `--no-default-features`.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | `tools/w2w` dựng và chạy: acceptor + initiator qua loopback, `Spin`, `InlineDispatch` | Cổng wire xanh (plan `gates-that-can-be-trusted`) |
| 2 | `scripts/check-no-kernel-sleep.sh` — `strace -f -c` trên binary, đỏ khi thấy syscall ngủ | 1 |
| 3 | **Item 15 đóng.** Đảo ngược: thêm `sleep` → script đỏ; bỏ ra → xanh | 2 |
| 4 | Job CI mới chạy bước 2 trên runner Linux | 3 |
| 5 | **CHẶN — cần máy §9.** Baseline w2w, ghi đủ máy và cấu hình | máy thật |
| 6 | **Item 13.** Đo từng thiết lập profile một, giữ cái nào có tác dụng | 5 |
| 7 | **Item 11.** Đo lại serialise trên Linux; sửa chỉ khi vẫn trượt | 5 |
| 8 | **Item 12 quyết định.** Đóng bằng dữ liệu, hoặc mở plan riêng | 5 |
| 9 | `DESIGN.md` §8 thay bằng số đo thật; **item 6 đóng** | 5–8 |

Bước 1–4 làm được ngay. Bước 5–9 **chặn ở phần cứng**, và đó là chỗ plan này sẽ dừng lại và
báo là chưa xong, chứ không hạ tiêu chuẩn để đóng.

## Cách kiểm chứng

- **Bước 3 là bước quan trọng nhất và cũng là bước dễ tự lừa nhất.** Hai công cụ trước đã thất
  bại theo đúng kiểu "xanh mà chẳng kiểm gì". Nên: chạy script trên binary có `sleep` **trước**,
  thấy đỏ, chép output; bỏ `sleep`, thấy xanh, chép output. Và **xác nhận cái `sleep` đã thật
  sự nằm trong file đã biên dịch** — không chỉ tin là mình đã sửa.
- Bảng `strace -c` phải được **đọc**, không chỉ đọc exit code. Ghi lại danh sách syscall thật
  sự thấy trong vòng ổn định.
- **Số đo nào cũng phải chạy nhiều lần và ghi độ tản**, vì bài học ở `benches/dispatch.rs` là
  một trần chặt hơn độ tản là một cổng đỏ ngẫu nhiên.
- Mỗi bước: `cargo test --all`, `cargo test --no-default-features`, `benches/alloc.rs`.

## Tài liệu phải cập nhật

- [ ] `DESIGN.md` §3 + `README.md` + `Cargo.toml` members — thêm `tools/w2w` (thêm crate)
- [ ] `DESIGN.md` §6 — dòng cho cổng "không ngủ trong kernel"
- [ ] `DESIGN.md` §8 — **chỉ ở bước 9**, và chỉ với số đo kèm máy
- [ ] `CLAUDE.md` §2 — bảng "machine-checked today" thêm điều 4 (sau bước 3). **Nói to ra.**
- [ ] `docs/reference/measured-costs.md` — mọi số đo, và cách đo
- [ ] `STATUS.md` — item 15 (bước 3), item 6/11/12/13 (bước 9)
- [ ] `PRD.md` §2 — tiêu chí thoát phase 1 số 6

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Script trace xanh vì trace nhầm tiến trình, hoặc nhầm luồng | Đảo ngược bằng `sleep` ở bước 3; và khẳng định trace thấy được **ít nhất một** syscall đã biết là có (ví dụ `sendto`) |
| Trace bắt cả giai đoạn khởi động, nơi ngủ là hợp lệ | Chỉ đếm trong cửa sổ ổn định, sau khi phiên đã lên |
| Công bố số từ container này như thể từ máy §9 | Mọi số ở nửa A dán nhãn ngay trong output của chính script |
| `w2w` tự cấp phát trong vòng đo, và đổ lỗi cho engine | Assert cấp phát riêng cho binary, giống bài học "benchmark đo một kết nối đã rớt" |
| Bật `lto` + `codegen-units` cùng lúc rồi không biết cái nào có tác dụng | Bước 6 đo **từng cái một** |
| Sửa serialise trước khi đo trên Linux | Bước 7 bắt buộc đo trước; nguyên nhân đã biết không phải là lý do để sửa mù |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Không có máy §9 nào trong tầm tay | Cao | Nửa A vẫn về đích và đóng được item 15. Nửa B **báo là chưa xong**, không nới tiêu chuẩn |
| `strace` không có hoặc bị chặn trong container | Trung bình | Thử `strace`; nếu bị chặn thì `perf trace` hoặc `/proc/<pid>/stack`. Nếu không cái nào chạy được thì item 15 vẫn mở và phải nói thẳng |
| Container 4 vCPU làm `Spin` chiếm hết máy | Trung bình | Ghim số luồng, và nhớ rằng nửa A chỉ cần kết luận có/không, không cần số |
| Đo xong thấy ngân sách §8 sai xa | Trung bình | Đó là kết quả. Sửa `DESIGN.md` §8 theo số đo, và mở ADR nếu nó lật một quyết định |

## Ngoài phạm vi

- **Không** dựng `library` (DESIGN §7 bước 8).
- **Không** làm SIMD/SWAR — item 12 ở đây chỉ được *quyết định*, không được *làm*.
- **Không** đụng kernel bypass (item 14) — phase 3, và cần phần cứng không có.
- **Không** đưa Criterion vào. Nếu số đo cho thấy thiếu khoảng tin cậy là vấn đề thật thì đó
  là một ADR, không phải một nhánh rẽ giữa plan.
- **Không** đụng TLS (item 10 — plan riêng).

## Nhật ký giao hàng

**Duyệt 2026-08-30.** Chủ dự án duyệt cả sáu plan cùng lúc, kèm một uỷ quyền ghi rõ:
*trong quá trình làm, nếu plan sai thì được sửa plan theo tình hình thực tế.* Điều đó nới
`CLAUDE.md` §1 — chỗ bảo "dừng lại, sửa plan, xin duyệt lại" — thành "sửa plan, **ghi lại
vào đây**, đi tiếp". Mỗi lần sửa plan phải có một mục dưới đây nói rõ **sửa gì và vì sao**,
nếu không thì uỷ quyền này biến thành giấy phép đi chệch trong im lặng.
