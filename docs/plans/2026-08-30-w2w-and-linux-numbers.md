# `tools/w2w`, và con số đầu tiên đo trên Linux

> **Loại:** Plan · **Ngày:** 2026-08-30 · **Trạng thái:** **Xong** — item 15 đóng 2026-08-30, item 11 2026-08-31, item 13 2026-09-01, item 6 2026-09-02; *Nhật ký giao hàng* nói **không còn gì của plan này**
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
- `fixbolt_engine::wait::Spin` đã tồn tại và có ghi chú là dành cho `w2w`; test dùng `Park`.
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

---

### Nửa A — bước 1 tới 4, xong 2026-08-30. Item 15 đóng.

**`tools/w2w` chạy được**, là một binary thật trong workspace, hình dạng đúng như một triển
khai: `wait::Spin`, `InlineDispatch`, `SystemClock`, luồng engine riêng.

**Sửa plan — phạm vi đo hẹp hơn plan mô tả, và nói rõ ra.** Bản này đo **vòng khứ hồi hành
chính: `TestRequest` đi, `Heartbeat` về**. Không có ứng dụng nào tham gia — session sở hữu
`35=1`, nên `Never::on_message` không bao giờ được gọi (và nếu bị gọi thì nó tự kêu lên, chứ
không lặng lẽ trả `None`). Chọn thế vì nó **không cần một echo application và không cần
corpus**, nên con số không bị nhiễm bởi chính công cụ. Echo ứng dụng — thứ mà `DESIGN.md` §8
thật sự mô tả — đi cùng nửa B.

**Item 15 đóng, và cổng tự mang theo phép đảo ngược của nó.**
`scripts/check-no-kernel-sleep.sh` chạy `strace -f` trên binary và quy syscall về **đúng luồng
engine theo tid** — client ở luồng chính chặn *có chủ đích* và sẽ che hết nếu đếm theo tiến
trình.

`[measured 2026-08-30]` Linux 6.18 x86_64, luồng engine:

```
   3111 accept4      3111 recvfrom      351 sendto
      0 epoll_wait / poll / select / futex / nanosleep / sched_yield
```

**Cổng chạy binary lần thứ hai với `--park`** (`wait::Park`, tức `sched_yield`) và **bắt buộc
lần đó phải làm cổng đỏ**:

```
GREEN ok — engine thread made no blocking call; it did make socket calls
RED   ok — --park trips it:  1749 sched_yield
```

Đó là điểm mấu chốt. Non-negotiable 4 đã có **hai** máy kiểm trước đây và **cả hai đều xanh
trong khi có `sleep` nằm bên trong** — `dtruss` bị SIP từ chối nên không chạy gì cả, còn đọc
symbol từ rlib thì không thấy được code generic. Một cổng chỉ từng được nhìn thấy xanh thì
chưa được biết là hoạt động, nên cổng này mang nửa RED bên trong chính nó, chạy mỗi lần.

Cũng theo bài học cũ: **số 0 chỉ có nghĩa khi có thứ khác chứng minh đường đó đã chạy.** Script
đòi luồng engine phải có `recvfrom`/`sendto` khác 0 trước khi chấp nhận con số 0 kia.

`--park` là một công tắc "làm cho engine ngủ" nằm trong một công cụ, và nó tồn tại **chỉ để**
chứng minh cổng biết đỏ. Điều đó được ghi ngay trong doc comment của `main.rs`.

**Một cái sửa nhỏ đáng ghi:** bản đầu dùng `.expect()` khi spawn luồng, và clippy chặn đúng
theo non-negotiable 7. Đổi sang `?`. **Một công cụ không được miễn một rule mà workspace ép
bằng lint** — nếu miễn thì rule đó chỉ còn là lời khuyên.

**Số đo, và tại sao chúng không được công bố.** `[measured 2026-08-30]` container 4 vCPU:
min 14 967 ns, p50 29 745 ns, p99 67 943 ns trên 5 000 mẫu. Chính binary tự in ra rằng đây
**không phải** con số latency để công bố, mỗi lần chạy — vì máy này không khớp `DESIGN.md` §9
(không cô lập core, không tắt tần số động, không ghim luồng). Không có dòng nào của §8 được
sửa dựa trên nó.

**Gate:**

```
200 passed / 0 failed   cargo test --all
200 passed / 0 failed   cargo test --all --no-default-features
clean                   cargo clippy --all-targets -- -D warnings
clean                   cargo fmt --check
GREEN ok / RED ok       scripts/check-no-kernel-sleep.sh
no dead internal links  scripts/check-links.py
```

**Bước 4 — CI.** Run **`33295397667`** trên commit `7e00e25`, `success`. Job `no-kernel-sleep`
chạy trên runner GitHub (cài `strace` qua apt) và xanh ở đó, không chỉ ở máy này.

**Còn lại:** nửa B (item 6, 11, 13, và quyết định 12) — **chặn ở một máy đúng §9**, và plan
dừng ở đó chứ không hạ tiêu chuẩn để đóng.

---

### Nửa B — bước 5, 8, 9, xong 2026-09-02. **Item 6, 11, 13 và quyết định 12 đóng. Phase 1 đóng.**

**Máy:** AMD Ryzen 7 3700X, Linux 7.0.0-30-generic, **máy thật, không phải máy ảo**.
`scripts/check-machine.sh` đọc **`pass 12  fail 0  unknown 1`** — dòng `unknown` là NIC IRQ
affinity, và phép đo này chạy qua loopback nên **không có NIC nào để lái**. Dòng lệnh kernel:
`isolcpus=6,7,14,15 rcu_nocbs=6,7,14,15 processor.max_cstate=1`, **không `nohz_full`**
(ADR-0021), **mitigations bật** (ADR-0023). Sáu dòng bật lúc chạy bằng
`sudo -n fixbolt-machine on` và `smtoff`.

**Sửa plan — ba chỗ, và cả ba là chuyện của *dụng cụ đo*, không phải của engine.** Bước 5 nói
"đo baseline wire-to-wire" như thể chỉ cần chạy. Không phải:

1. **`tools/w2w` không ghim luồng nào.** §9 đòi *pinned threads*, và chính lời từ chối mà
   `w2w` in ra mỗi lần chạy cũng gọi tên nó — nhưng binary không có cách nào để ghim. Nghĩa là
   **một lần chạy trên cái máy hoàn hảo vẫn không phải một lần chạy §9**, và điều đó đúng suốt
   ba ngày mà không ai thấy. Nay có `--engine-core` / `--client-core`, đi qua
   `fixbolt_engine::affinity` (ADR-0015: ghim từ bên trong luồng, đọc lại từ scheduler), và
   **từ chối một core mà `isolcpus` không gọi tên** trừ khi được bảo `--allow-unisolated`.
   `[đo 2026-09-02]` ghim vào `cpu2` không cô lập **thành công** và in `engine-core: cpu2` —
   đọc lên như một lần chạy §9, mà không phải.
2. **`w2w` in p50, p99, `max` — không in p99.9**, đúng cái phân vị mà tiêu chí 6 gọi tên.
3. **Chỉ đo vòng hành chính.** Chỗ này plan đã tự ghi từ 2026-08-30 rằng echo ứng dụng thuộc
   nửa B, nên đây là làm đúng plan chứ không phải sửa nó. `--path app` lái
   **`NewOrderSingle` → `ExecutionReport`** qua một application giữ `Template` dựng **một lần**
   lúc khởi động — hình dạng D9 mô tả. Nó **không phải** `crates/library/examples/shared/order_handler.rs`
   (bản `Desk` duy nhất), vì bản đó viết trên API `Handler`/`Reply` của tầng library, và
   template mỗi message của tầng đó **chính là open item 34** — đo qua nó là đo item 34, không
   phải đo §8.

**Bước 5 — con số.** Trung vị của **20 lần chạy nguyên** mỗi arm, mỗi lần 20 000 vòng khứ hồi
sau 2 000 warmup, `scripts/w2w-baseline.sh` (script mới, cam kết vào repo — rule 10 đòi
benchmark đi kèm con số, và một trung vị 20 lần cần một cái runner chứ không cần một đoạn văn
kể lại ai đã gõ gì):

| Mode | Path | min | p50 | p99 | p99.9 | run hợp lệ | spread |
|---|---|---|---|---|---|---|---|
| `hft` | admin | 15 810 | **16 010** | **20 589** | **22 127** | 20 / 20 | 1.006 |
| `hft` | app | 17 288 | **19 908** | **24 657** | **26 150** | 20 / 20 | 1.005 |
| `standard` | admin | 16 020 | **19 447** | **24 106** | **25 609** | 16 / 20 | 1.003 |
| `standard` | app | 17 624 | **20 920** | **25 618** | **27 092** | 19 / 20 | 1.005 |

**Năm lần chạy bị loại, và bị loại bởi đúng cái guard tồn tại để loại chúng**: Chrome thức dậy
trên các core housekeeping, `w2w-baseline.sh` đọc lại CPU busy **trước từng lần chạy**. Sau đó
`standard / admin` được đo lại 10 lần, **0 bị loại**, đọc **19 451** so với 19 447 — lệch 0,02%.
Đây là quy tắc *"một lần chạy không phải một phép đo"* áp cho chính phép đo này.

**Bốn phát hiện.** Chi tiết ở [measured-costs.md](../reference/measured-costs.md):

1. **Thiết kế này sở hữu 2,9% vòng khứ hồi**, và lần đầu tiên **cả hai nửa của phép chia đều
   được đo**. §8 nói "dưới 5%" từ ngày nó được viết, trên một mẫu số đi vay.
2. **`hft` hơn `standard` 3 437 ns — 17,7%** trên đúng cùng một đường. Đó là toàn bộ lý lẽ của
   D8, và nó cũng định giá dòng wakeup của §8 ở **~3,9 µs** (hiệu số cộng với vòng quét 449 ns
   mà nó thay thế) — nằm trong khoảng 2–5 µs mà §8 vay từ tài liệu.
3. **Ghim vào core cô lập không mua được gì ở p50 và mua 11× ở p99.9.** Một biến, hai arm cùng
   một CCD: p50 **19 968 so với 19 407** — core cô lập **chậm hơn 2,9%** — còn p99.9
   **26 300 so với 266 887**, và **293 749** khi không ghim gì. §9 từng ghi rằng lợi ích của
   `isolcpus` là **chưa đo được**; thứ không thấy được nó là **cái dụng cụ đo**. Một benchmark
   500 ns không có p99.9 nào đọc được, mà cái stall nó ngăn dài 250 µs. **Ngược hẳn với
   `nohz_full`**, cái tệ hơn ở p50, p99 *và* p99.9 và chỉ thắng từ p99.99.
4. **Dòng parse của §8 không mô tả việc engine làm với một message vào.** Vòng app cao hơn vòng
   admin **3 898 ns**, và tất cả benchmark đã cam kết cộng lại chỉ giải thích được **~320 ns**.
   Ứng viên lớn nhất: **lượt kiểm tra dictionary của session** — mỗi field bị hỏi
   `is_defined_tag`, `field_type`, `allows`, `enum_allows`, cộng `view.get` một lần cho mỗi tag
   bắt buộc — và **không benchmark nào đo nó**, vì `benches/parse.rs` parse với `NoDict`. **Ghi
   là một khoảng trống, không suy thành nguyên nhân**: `[đo 2026-08-30]` dự án này đã từng công
   bố một nguyên nhân sai suốt một ngày trên đúng dạng số học này. Open item **39**.

**Bước 8 — item 12 đóng bằng dữ liệu, và đó là [ADR-0045](../decisions/ADR-0045-parse-is-under-one-percent-of-the-wire-and-simd-is-declined.md).**
Điều kiện của chính item 12 là *"chỉ làm khi `benches/parse.rs` trên máy Linux cho thấy parse
nằm trên đường găng"*. `[đo 2026-09-02]` parse là **0,62%** vòng app và **0,36%** vòng admin;
mức lợi 20–40 ns mà item tự ước lượng là **0,10–0,20%**, **nhỏ hơn spread 0,5% của chính cái
dụng cụ phải nhìn thấy nó**. ADR nói rõ **con số 20–40 ns cũng không phải một phép đo** — không
ai viết SWAR nào — chỉ mẫu số là được đo. Và ADR gọi tên **một** điều mở lại nó: một transport
bỏ được số hạng kernel (item 14).

**Bước 9 — tài liệu.** `DESIGN.md` §6 tách thành **hai** dòng wire-to-wire: dòng loopback,
**đạt**, và dòng NIC-to-NIC, **mở** (open item **40**) — loopback không có driver, không có
interrupt, không có dây. §8 có bảng khứ hồi đo được ở đầu mục và dòng parse nói rõ nó **không**
bao gồm lượt dictionary. §9 dòng `isolcpus` thay câu "lợi ích chưa đo được" bằng phép đo. `PRD.md`
§2 tiêu chí 6 **đạt**. `GUIDE.md` §1 và §7 có ràng buộc mới (*ghim vào core cô lập, và trình
biên dịch không kiểm được*) — **và bảng §1 được sửa: nó vẫn đang dựng trên 703 ns**, con số mà
ADR-0021 đã thay bằng 449 từ 2026-08-31, cùng với câu *"core cô lập là cái đắt, 36%"* mà thực ra
là của `nohz_full`. Đó là hai đoạn cũ, không phải do lần này gây ra, và sửa vì `CLAUDE.md` §4:
tài liệu cũ tệ hơn không có tài liệu.

**Cổng, chạy và đọc chứ không suy:**

```
445 passed / 0 failed   cargo test --all
442 passed / 0 failed   cargo test --all --no-default-features
clean                   cargo clippy --all-targets -- -D warnings
clean                   cargo clippy --all-targets --features affinity -- -D warnings
clean                   cargo fmt --all --check
GREEN + RED ok          scripts/check-no-kernel-sleep.sh          (standard trips it: 7 poll)
GREEN + 2 RED ok        scripts/check-standard-gives-the-core-back.sh  (hft 99.59%, yield 100.04%)
ok                      scripts/check-no-optional-deps.sh
RED + GREEN ok          scripts/check-lint-config.sh
no dead internal links  scripts/check-links.py
RED                     scripts/bench.sh --strict   <-- xem dưới
```

**`bench.sh --strict` ĐỎ, và phải nói ra chứ không được im.** Nó đỏ **trước** phiên này:
`git diff origin/main -- crates/` **rỗng** cho nhánh đóng phase 1. Hai case vượt band và
**lặp lại 6 trên 6 lần chạy** trên máy đọc 0–1% busy: `encode ExecutionReport (template)`
**274,2 · 279,6 · 275,5 · 283,3 · 275,0 · 279,4 ns** so với trần 263,0 (**+16%**), và
`presession, read and route an identity` **201,3 · 197,9 · 201,6 · 202,2 · 209,3 · 205,7 ns**
so với 92,4 (**+140%**). Năm case **không có baseline cho CPU này**, và đó là thứ làm `--strict`
thoát khác 0. **Mọi case còn lại trong band**, kể cả `parse NewOrderSingle (validated)` ở
119,8–121,7 so với 122,6 — chính là con số ADR-0045 dựa vào. **Không sửa ở đây**: Rule Zero, một
bản sửa cần plan riêng. Open item **41**.

**Một lỗi cùng loại, tìm ra sau khi commit đầu đã push, và sửa ngay.** `--mode` không hợp lệ
**thoát 0**: nó in lời phàn nàn ra stderr rồi `return Ok(())`, nên `w2w --mode standrad` báo
thành công và không đo gì. Đúng hình dạng của cái bug `--mode standard` mà comment trong
`Cargo.toml` được viết ra vì nó, và đúng hình dạng của một `--engine-core` ghim-mà-không-ghim.
Nay là `Err`. Và `w2w-baseline.sh` **đọc lại `mode:` và `path:` từ chính output của binary**,
giống việc `check-no-kernel-sleep.sh` đã học làm — một lỗi chính tả trong `ARMS` không được
lặng lẽ sinh ra một cột số cho arm khác. **Đảo ngược, chạy thật:** cho `Mode::Standard::name()`
trả về `"hft"` → runner đỏ với `ran a mode other than 'standard'`; bỏ ra → xanh.

**Và mẻ đo chính đã chạy *trước khi* cổng đọc-lại đó tồn tại**, nên bốn arm được đo lại 3 lần
mỗi arm với cổng bật, để chứng minh chúng đúng là arm chúng khai. Mọi p50 khớp trung vị đã
công bố trong **0,35%**: 16 020 / 19 978 / 19 477 / 20 940 so với 16 010 / 19 908 / 19 447 /
20 920. Ba bằng chứng độc lập nữa nói cùng điều: hai mode lệch 17,7% nên chúng chạy code khác
nhau; `check-standard-gives-the-core-back.sh` đọc `standard` ở 0,31% CPU và `hft` ở 99,59%; và
ba assertion trong binary (`35=8`, `ClOrdID`, `150=F`) chỉ xanh trên đường app.

**CI xanh cho đúng commit được đóng** — `31623fe`, PR
[#31](https://github.com/tmthang86/fixbolt/pull/31), runs [`33644573232`](https://github.com/tmthang86/fixbolt/actions/runs/33644573232) and [`33644576288`](https://github.com/tmthang86/fixbolt/actions/runs/33644576288), **20 check trên 20**.
Đây là hộp cuối của §9, và nó tồn tại vì một cái laptop chỉ nói cổng xanh *với mình*, còn CI
mới nói cổng xanh *với commit*.

**Còn lại:** không còn gì của plan này. **Phase 1 hết tiêu chí mở, hết cấu phần chưa dựng, hết
quyết định treo.** Những gì mở trong `STATUS.md` là việc phase 1 chưa từng đòi: item 39, 40, 41,
34, 36, 38.
