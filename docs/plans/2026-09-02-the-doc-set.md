# Bộ tài liệu: người dùng, người contribute, và người làm HFT

> **Loại:** Plan · **Ngày:** 2026-09-02 · **Trạng thái:** Chờ duyệt
> **Phạm vi:** tài liệu. Không đụng code sản phẩm.

> **Sửa 1 — 2026-09-02, trước khi bắt đầu.** Bản đầu được soạn trên `73e48c6`. Kéo source về
> thấy **73 commit mới** (tới `89c785b`), và một phần chẩn đoán của bản đầu **đã sai**:
> `docs/GUIDE.md` từ 501 lên **1054 dòng**, từ 2 lên **14 khối `rust`**, và đã hấp thụ registry,
> file cấu hình, session schedule, journal trên đĩa, quan sát engine đang chạy, và shutdown có
> thứ tự. Hai trong ba tuyên bố cũ đã được sửa. Nên bản này **bỏ `OPERATIONS.md`**, **thu hẹp
> `CONFIGURATION.md`** xuống đúng phần `GUIDE.md` không có, và đổi số open item 26 → **33**.
> Ghi lại theo `CLAUDE.md` §1: plan sai giữa đường thì sửa plan, không âm thầm đi lệch.

## Bối cảnh

Repo có tài liệu **rất dày cho người xây engine**: `DESIGN.md` 1176 dòng, **39 ADR**,
20 file `docs/reference/`, 35 plan, `STATUS.md` 1426 dòng. Và `docs/GUIDE.md` — sau đợt việc
vừa rồi — đã là một developer guide tốt thật cho **người nhúng engine**.

Còn thiếu là hai loại người đọc khác, cộng cửa trước.

| Người đọc | Có gì hôm nay | Thiếu gì |
|---|---|---|
| **Người dùng** — nhúng engine | `GUIDE.md` 1054 dòng, 14 ví dụ code | cửa trước; bảng tra cứu key cấu hình; kết quả conformance công bố |
| **Người contribute** — sửa codebase | `DESIGN.md` nói **đã quyết định gì**; 39 ADR nói **giá phải trả** | **không có gì nói code hôm nay chạy ra sao** — 19 module trong `engine` mà không có bản đồ |
| **Người làm HFT** — muốn đạt chuẩn µs | `DESIGN.md` §9 (bảng OS có gate máy), `GUIDE.md` §1/§7 | **không có quy trình theo thứ tự**: phần cứng, BIOS, NIC, IRQ, config app, cách nghiệm thu |

Đo được, trên `89c785b`:

| Thứ | Con số |
|---|---|
| Thư mục `examples/` trong workspace | **0**, `find` không ra cái nào |
| `serve` / `serve_hft` / `serve_with_recovery` / `serve_hft_with_recovery` được gọi trong test, bench hay tool | **0 lần**. Bốn cửa vào chính thức, không gate nào chạy qua |
| Khối ```` ```rust ```` trong `README.md` | **0** |
| File nguồn có doc code fence | **5 / 42** |
| Mục *Installation* / *Getting Started* / *Usage* / *Quickstart* trong README + `docs/` | **0**, `grep` không ra dòng nào |
| Key mà `settings.rs` nhận | **10** — và `EndDay`, `MaxSkewMillis` **không được nêu tên ở đâu trong `GUIDE.md`** |
| Bảng tra cứu `Setting / Valid values / Default` | **không có** |
| Trang công bố kết quả conformance | **không có** |
| Fence hỏng trong `GUIDE.md` | **1**, đã dời xuống dòng **331** |
| Tuyên bố user-facing còn cũ | **1** — `README.md:23` |

Kết quả muốn đạt: người mới có cửa vào; người contribute có bản đồ code; người làm HFT có quy
trình. Và **không ai bị hứa hẹn thứ chưa tồn tại**.

## Những gì đã biết chắc

Mọi mục kèm nguồn. Phỏng đoán nằm ở mục Rủi ro.

### Ràng buộc từ repo

**1. `library` (`DESIGN.md` §7 bước 8) chưa tồn tại và chưa có plan.** Hôm nay người dùng viết
thẳng vào `fixbolt-engine`. **Chủ repo đã chọn: chờ `library` rồi mới viết tutorial/quickstart.**
Đó là lý do phase E tồn tại và bị chặn.

**2. Chưa crate nào publish.** `version = "0.0.0"`, `publish = false`.
`CHANGELOG.md`: *"Nothing has been released."* **Không được viết `cargo add fixbolt`.**

**3. Bootstrap bắt buộc, README không nói.** `scripts/fetch-quickfix-assets.sh` kéo `vendor/`
(gitignored); `crates/dict/build.rs` sinh bảng từ `vendor/quickfix/spec/FIX44.xml`. Không chạy
thì **không build được**. `STATUS.md` nói; `README.md` không.

**4. `README.md:23` còn cũ:** *"`standard` is decided and not yet built"* — `standard` đã dựng
và **là mặc định**. Hai chỗ cũ khác của bản plan đầu (GUIDE §9 về ADR-0010 và về ghim thread)
**đã được sửa trong 73 commit vừa rồi**; không còn việc ở đó.

**5. Fence hỏng `docs/GUIDE.md:331`** — dòng đóng ```` ``` ```` có chữ `` `allow_unisolated()` waives ``
đi kèm, nên đoạn văn sau bị nuốt vào code block.

**6. `GUIDE.md` bị nhiều file trỏ tới** và `scripts/check-links.py` **là job CI**. Tách nó ra
nhiều file mà không sửa kèm thì CI đỏ. Bản này **không tách**.

**7. `CLAUDE.md` §4 định charter `GUIDE.md` rất hẹp** — *"Every constraint it names is one the
type system cannot enforce"*. Thêm tài liệu mới **là sửa luật §4**, và `CLAUDE.md` mở đầu yêu cầu
**nói rõ đã sửa luật nào**.

**8. `engine` có 19 module** — `affinity, backpressure, block, clock, conn, dispatch, frame,
journal, observe, poll, presession, recovery, ring, settings, shard, transport, wait, waker`
cộng `lib.rs`. **Không có tài liệu nào nói chúng ghép với nhau ra sao.**

**9. Const generic bị chôn trong alias.** `TcpAcceptorEngine<A, W, J = journal::Store>` cố định
`N/RX/TX`. Muốn khác phải viết `Engine<...>` 9 tham số — không tài liệu nào nói.

**10. Số liệu phải kèm ba thứ** (§2 điều 10: benchmark đã commit, máy, cài đặt §9), **cộng mode**
(điều 4) và **số session** (`PRD.md` §1). `STATUS.md` "Not proven": mọi dòng `DESIGN.md` §8 là số
từ tài liệu người khác; `standard` chưa có số nào đáng trích; engine này **chưa từng gửi cho một
peer FIX thật**.

**11. Nguyên liệu sẵn có, chưa dùng làm tài liệu:** `crates/conformance/src/echo.rs` là một
`impl Application` thật dùng `TemplateBuilder`; `crates/engine/tests/` là example trên thực tế;
`crates/engine/tests/registry.rs` có một `Registry` tự viết; 59 file `.def` mỗi file là **một
hành vi session có test đứng sau**.

### Khảo sát bên ngoài

**QuickFIX/C++** — 4 chương: Installation · Getting Started · Working With Messages ·
**Testing (Unit + Acceptance)**. Bản site mới **mất hẳn** chương Threading / Store / Logging.
**QuickFIX/Go** 10 trang và được coi là đủ. **QuickFIX/n** 10 trang phẳng.
**QuickFIX/J** 20 trang, 4 nhóm, có trang **Use Cases**; tutorial là **ba bước** —
config → application → bootstrap.

**OnixS** (thương mại, 22 chương) — cái open-source không có: **Best Practices tách Low Latency /
High Throughput**; **Understanding Send Latency** và **Receive Latency** là hai trang riêng;
Threading Models, System Requirements, Deployment là chương.

**Rust:** **tokio** ba artefact ba việc, **không lặp lại** (Tutorial tuần tự · Topics · rustdoc
chỉ là tour). **quinn** mdBook 6 trang, **hai chương giao thức trước** khi có chương thư viện.
**rustls** đặt sách prose **trong rustdoc** (`rustls::manual`), `_04_features` = cái gì có/cố ý
bỏ, `_05_defaults` = vì sao mỗi mặc định như vậy. **glommio/monoio** giải thích thread-per-core
bằng ba nhịp, **luôn trước ví dụ đầu tiên**: lời hứa → **ràng buộc dưới heading riêng** →
**điều kiện môi trường thành mục riêng**. **hftbacktest** đặt "Debugging Discrepancies" làm
**chương hạng nhất**.

**Không gian FIX Rust — phần quan trọng nhất.** ≈20 crate, tài liệu trống rỗng:
- `quickfix` (binding C++) **~345k lượt tải, gấp ~3,4× tổng mọi engine Rust thuần cộng lại** —
  vì nó là crate duy nhất trả lời được *"làm sao chạy một acceptor?"*
- `fefix` **101 866 lượt tải, không ra bản nào từ 2021**, docs.rs của nó vẫn là thứ đầu tiên
  người mới đọc. Kế nhiệm `rustyfix` có 2 124, README **3 heading**.
- `hotfix` là bộ lành mạnh nhất và là crate **duy nhất** có mục **Prior Art**.
  `forgefix` có mục **Terminology** — vẫn là văn bản khái niệm hay nhất trong toàn không gian.

**Lãnh thổ chưa ai chiếm:** bảng tra cứu cấu hình session · getting-started cho **acceptor** ·
kết quả conformance công bố · số hiệu năng kèm phương pháp · trang *"tầng session thực sự làm
gì ở biên"*. **Mọi bộ tài liệu FIX ở mọi ngôn ngữ đều có bảng Configuration; không bộ nào có
trang cuối** — và mọi khiếu nại người dùng tìm được đều rơi đúng vào đó (quickfixn #206/#80/#151,
quickfixj #912, quickfixgo #345).

**Kết luận định vị:** bộ tài liệu FIX **tốt thì nhỏ**. Đối thủ không phải OnixS 22 chương — mà là
việc không crate Rust nào có nổi một bảng config.

## Cách làm

**Markdown trong `docs/`, không thêm toolchain.** Không mdBook, không job build site.
`scripts/check-links.py` đã là gate và sẽ phủ file mới.

**Nguyên tắc: mỗi file một charter, không chồng lấn** — §4 "một luật, một chỗ".
`GUIDE.md` **giữ nguyên tên và charter**; `DESIGN.md` §9 vẫn là bảng OS chuẩn duy nhất. Cái mới
nằm cạnh, không thay thế.

### A0 — đăng ký vào open item, và nó đi trước mọi thứ

`STATUS.md` là nơi dự án theo dõi việc còn nợ; việc không nằm trong đó là việc không ai đọc lại.
Bước A0 viết **item 33** (32 là số cao nhất đang dùng; §5 cấm dùng lại số), thêm một dòng vào
bảng *Plan → Closes*, và cập nhật *Where the work is* + ô *Plan in flight*.

Nội dung item 33, viết theo giọng các item khác — nêu cái đã đo, không nêu ý định:

> **33 — Ba loại người đọc; hai không có tài liệu và cửa trước thì trống.**
> `[measured 2026-09-02]` workspace có **0 thư mục `examples/`**; **bốn cửa vào `serve*` không
> được gọi ở bất kỳ test, bench hay tool nào**; `README.md` có **0 khối `rust`** và `grep` không
> tìm được mục *Installation*, *Getting Started*, *Usage* hay *Quickstart* nào trong README +
> `docs/`; `settings.rs` nhận **10 key** mà `EndDay` và `MaxSkewMillis` không được nêu tên ở đâu,
> và **không có bảng tra cứu `Setting / Valid values / Default`**; không có trang công bố kết quả
> conformance; `engine` có **19 module** không có bản đồ nào; `README.md:23` còn nói `standard`
> *"not yet built"*; `GUIDE.md:331` có một fence hỏng. Khảo sát ngoài: **không crate FIX Rust nào
> có bảng cấu hình, hướng dẫn acceptor, kết quả conformance, hay số hiệu năng kèm phương pháp** —
> binding C++ `quickfix` có ~345k lượt tải, gấp ~3,4× tổng mọi engine Rust thuần.
> Đóng bởi [the-doc-set](2026-09-02-the-doc-set.md), 5 phase; **chỉ phase E chờ `library`**.

**Khiếm khuyết bốn `serve*` không có test được ghi trong item 33 nhưng KHÔNG do plan này sửa** —
đó là code, cần plan riêng (Rule Zero). Ghi để nó không mất, đúng §10.

### Phase A — sửa cái đang sai

- `README.md:23` — `standard` đã dựng và là mặc định; sửa theo hiện trạng, ghi ngày.
- `docs/GUIDE.md:331` — đóng fence, đưa câu văn ra ngoài.
- `README.md` — thêm **thứ tự đọc** và **bước bootstrap** `fetch-quickfix-assets.sh`.

A merge được ngay: nó chỉ sửa cái sai.

### Phase B — cửa trước và ba trang người dùng còn thiếu

**Đã bỏ `OPERATIONS.md`** — `GUIDE.md` §8a (*Watching a running engine*, *Health*,
*Why a connection ended*, *The 3 a.m. phone call*, *Stopping without lying to the counterparty*)
đã làm đúng việc đó. Viết thêm là tạo bản mô tả thứ hai để trôi.

| File | Charter | Không chứa |
|---|---|---|
| `docs/INTRODUCTION.md` | FIX 4.4 là gì; session vs connection; sequence number, resend, gap fill, admin vs application — **từ vựng trước, kiểu quinn và `forgefix`**. Vì sao **phía acceptor** khó. Mục **Prior Art** rút gọn trỏ `reference/prior-art.md` — vì `fefix` đã chết mà người mới không có cách nào biết | số đo; API |
| `docs/CONFIGURATION.md` | **Bảng tra cứu duy nhất, và nó bổ sung `GUIDE.md` §1a0 chứ không kể lại.** Cột `Setting / Meaning / Valid values / Default / Where set`. Phủ **cả 10 key** của `settings.rs` — gồm `EndDay` và `MaxSkewMillis` mà hôm nay không được nêu tên ở đâu — cộng thứ chỉ đặt được trong code: `Limits` (**không có mặc định, cố ý** — ADR-0020), const generic `N/RX/TX` và alias che chúng, `ring::DEFAULT_CAPACITY`, timeout `block`, `journal::Durability`, feature flag. Mỗi hàng ghi **đường dẫn:dòng** nguồn | vì sao (→ ADR); ràng buộc runtime (→ `GUIDE.md`) |
| `docs/SESSION-BEHAVIOUR.md` | **Tầng session làm gì ở biên**, dạng bảng: logon/logout, reset vs resume, resend và gap fill, `141=Y`, PossDup/PossResend, cửa sổ lệch đồng hồ, 12 mã `373`, ba ngưỡng heartbeat, bảy loại admin engine tự trả lời, và `DropReason`. **Mỗi hàng trỏ file `.def` hoặc test giữ nó** | số đo; cấu hình |
| `docs/CONFORMANCE.md` | **Kết quả công bố kèm bằng chứng** — 59/59 in-process và qua socket; 93 group; 730/730 thứ tự trường khớp C++ do QuickFIX sinh; 912 tag, 93 message type, 12 524 cặp (msg,tag), 23 field type, 1 708 enum value; 0 allocation. Mỗi hàng kèm **lệnh, máy, run id CI**. Và mục **cái gì CHƯA được chứng minh** nằm **cùng trang** | lời hứa; số chưa đo |

Kèm **củng cố gate tài liệu**, rẻ và máy kiểm được:
- Job CI `cargo doc --workspace --no-deps`, với `#![deny(rustdoc::broken_intra_doc_links)]`.
- Mở rộng `scripts/check-links.py`: báo lỗi **link trống path** (`https://github.com/`) — đúng
  loại placeholder rustdoc đang mang mà rule hiện tại không bắt.

### Phase C — tuyến contribute, `docs/internals/`

**Đây là chỗ dễ vỡ "một luật, một chỗ" nhất.** `DESIGN.md` §4 nói **đã quyết định gì và vì sao**;
ADR nói **giá phải trả**; `docs/internals/` nói **code hôm nay chạy ra sao**. Mỗi trang mở đầu
bằng một dòng cố định — *"D-n / ADR-nnnn là quyết định; trang này là cách nó được hiện thực hôm
nay"* — và **trích `file:dòng`** thay vì kể lại.

| File | Nội dung |
|---|---|
| `internals/README.md` | Bản đồ + thứ tự đọc. Sáu tầng, và **đường đi một message end-to-end**: byte vào socket → `presession` nhận diện → `frame::Framer` cắt → `codec::parse_into` → `session::received_with` → `Application::on_message` → `Template::encode` → `backpressure` → write. Kèm chuỗi lời gọi thật |
| `01-codec.md` | Parse tại chỗ; `FieldIndex<N>` tách khỏi `MessageView` (D2, ADR-0003) và vì sao view 24 byte `Copy`; checksum và `BodyLength`; `GroupIter` trên index phẳng; `Template`/`TemplateBuilder` — parts list sắp sẵn, **vá chứ không dựng** (D9); `TimestampCache`; `no_std` |
| `02-dict.md` | `build.rs` sinh code từ XML; bốn bảng validation; **thứ tự trường từ bảng sinh** (D3, §2 điều 5); `Fix44` là ZST nên không có receiver trên đường parse; vì sao XML là **dữ liệu** chứ không phải code chép về (ADR-0001) |
| `03-session.md` | Máy trạng thái thuần (D1); hình dạng `received/tick/connect` + `emit: FnMut(&[u8])`; `Role` sealed nên nhánh biến mất lúc biên dịch (ADR-0004); `Journal` trait và vì sao `highest`/`highest_in` **không có default** (ADR-0008, ADR-0017); `resume` vs `new` (ADR-0010); `DropReason`; schedule; `out.rs`, `text.rs`; thời gian là **ms từ 0000-01-01** (D13) |
| `04-engine.md` | Trang lớn nhất, viết **theo thứ tự `Engine::turn` gọi**. 19 module: `conn` và bố cục cache-line; `dispatch` inline vs ring (D4, ADR-0002) và `RingApp::pump`; `backpressure` (D10) và ring đầy thì rớt session (D10b, ADR-0011); `journal`, `recovery` (ADR-0034, ADR-0039); `presession` giữ socket tới `Logon` (ADR-0020, ADR-0022), `Table`/`Registry` (ADR-0026, ADR-0030), `settings` (ADR-0040); `observe` (ADR-0035); shutdown có thứ tự (ADR-0038); `shard`; `wait`/`block`/`poll`/`waker` và **chỗ hai mode rẽ nhánh** (D8, ADR-0013, ADR-0014); `affinity` và hai khối `unsafe` (ADR-0015, ADR-0019) |
| `05-patterns.md` | **Kỹ thuật lặp lại, gom một chỗ** — const generic làm sức chứa; trait sealed; ZST làm dictionary; **policy bằng trait** (`Transport`/`Dispatch`/`Journal`/`Waiting`/`Registry`/`Recovery`) và cái giá mỗi cái; lỗi fieldless trên hot path, `thiserror` ngoài nó; kỷ luật không cấp phát và cách nó được **chứng minh**; template patching; view mượn buffer của caller; **feature gate ở `mod` chứ không chỉ ở `Cargo.toml`** (§2 điều 6). **Chỉ nhận kỹ thuật xuất hiện ≥2 nơi** |
| `06-testing.md` | Bộ gate hoạt động ra sao: 59 `.def` in-process và qua socket; `benches/alloc.rs` với allocator đếm, **mỗi case tự chứng minh đường của nó còn sống**; `baselines.tsv` per-machine (ADR-0016); các script máy; `tools/jrnl`, `tools/w2w`; **chứng minh bằng đảo ngược** — vì sao mọi gate ở đây có một nhánh đỏ bắt buộc, và ba cách một đảo ngược tự nó thất bại (`reference/a-reversal-can-fail-by-hanging.md`, `a-reversal-that-must-not-compile.md`) |
| `07-contributing.md` | Rule Zero và `docs/plans/_template.md`; khi nào cần ADR; §2 mười điều; CI chạy gì; bootstrap `vendor/`; `fmt`/`clippy -D warnings`/`--no-default-features`; §9 Definition of Done |

### Phase D — best practice theo mode, và HFT playbook

**Tách theo mode là bắt buộc** — §2 điều 4: một tuyên bố không nêu mode là tuyên bố chưa đầy đủ,
và **cả hai nửa đều là luật**.

| File | Nội dung | Không chứa |
|---|---|---|
| `docs/best-practices-standard.md` | Mode mặc định. Sizing `capacity`; chọn timeout `block` và nó đánh đổi cái gì; **nhiều session trên một thread là bình thường ở mode này**; chọn `Durability`; handler nên và không nên làm gì; khi nào chuyển `RingDispatch`; container và máy dùng chung; **cái gì chưa được đo ở mode này — và đó là hầu hết** | số của `hft` |
| `docs/best-practices-hft.md` | Mode opt-in. Một session một thread và số học đằng sau (ADR-0012); vì sao `InlineDispatch` là mặc định; ghim core từ bên trong và **đọc lại** (ADR-0015); `ShardPlan` từ chối cái gì và vì sao **SMT sibling** là lỗi thường gặp; ring sizing theo **thời gian đứng hình dài nhất** chứ không theo throughput; vì sao `wait::Yield` hỏng **cả hai** gate | hướng dẫn OS (→ playbook) |
| `docs/hft-playbook.md` | **Quy trình theo thứ tự.** 7 mục dưới | bảng OS row-by-row (→ `DESIGN.md` §9) |

**Phân vai `hft-playbook.md` — bổ sung §9, không thay §9.** §9 giữ **bảng row chuẩn**;
`scripts/check-machine.sh` giữ **gate**. Playbook làm phần §9 không làm:

1. **Phần cứng** — CPU (clock đơn luồng hơn số nhân), NIC, NUMA, RAM. Nêu rõ cái gì là **yêu cầu
   đã đo** và cái gì là **khuyến nghị chưa đo ở đây**.
2. **BIOS/firmware** — C-state, turbo, SMT, power profile: thứ OS không đặt được.
3. **Kernel và boot** — trỏ §9 cho danh sách row; playbook nêu **thứ tự áp dụng** và cách xác
   nhận từng cái đã ăn. Ghi rõ hai kết quả ngược trực giác **đã đo ở đây**:
   **`nohz_full` KHÔNG được khuyến nghị** (ADR-0021 — 160 ns mỗi lần vào kernel, trăm ăn một) và
   **mitigations phải BẬT** (ADR-0023 — tắt đi rẻ hơn 59–63% nhưng máy khi đó **không so sánh
   được**; đó **không phải lời khuyên tắt**).
4. **NIC và IRQ** — queue nào về core nào, tách khỏi core engine, `busy_poll`.
5. **Cấu hình app** — bản đồ core → shard → session; `capacity`; ring; `Durability`; feature
   `affinity`; và **profile build**: giữ mặc định, LTO đáng 3–6% và phải do **người tiêu thụ** bật
   (ADR-0024).
6. **Đo cho ra số dùng được** — `check-machine.sh` sạch, `bench.sh --strict`, `tools/w2w`, và
   **năm cái bẫy đo** đã trả giá ở đây (`GUIDE.md` §8) — **trỏ, không chép**.
7. **Tiêu chí nghiệm thu và cái chưa ai chứng minh** — sàn 10–20 µs wire-to-wire của kernel TCP;
   mọi dòng `DESIGN.md` §8 hiện là số từ tài liệu người khác; `standard` chưa có số nào đáng
   trích; engine này **chưa từng gửi cho một peer FIX thật**.

### Phase E — chỉ mở khi `library` tồn tại

`docs/GETTING-STARTED.md` (config → application → bootstrap, hình dạng QuickFIX/J),
`docs/TUTORIAL.md`, `examples/` với ít nhất một acceptor chạy được. **Phụ thuộc cứng:**
`DESIGN.md` §7 bước 8, chưa có plan. Plan này **không viết chúng** và không hứa nội dung.

**Chỉ E bị chặn.** C và D mô tả code đang có, nên chạy song song được với A và B.

### Sửa luật — phải nói rõ

`CLAUDE.md` §4 nhận thêm **9 dòng bảng directory** (4 file người dùng + `docs/internals/` +
2 best-practice + playbook + thư mục tutorial của phase E) và **8 dòng bảng sync**:

| Khi đổi… | Phải cập nhật |
|---|---|
| Một hằng số, mặc định, hay key file cấu hình người dùng thấy được | `CONFIGURATION.md` |
| Hành vi tầng session ở biên (reset, resend, gap fill, từ chối, `DropReason`) | `SESSION-BEHAVIOUR.md`, **kèm `.def` hoặc test giữ nó** |
| Một con số conformance, hoặc một gate đổi kết quả | `CONFORMANCE.md`, **kèm lệnh, máy và run id CI** |
| Cơ chế bên trong một module | trang `docs/internals/` của module đó, **cùng commit** |
| Một kỹ thuật mới, hoặc bỏ một kỹ thuật đang dùng | `internals/05-patterns.md` |
| Một gate mới, hoặc đổi cách một gate chứng minh | `internals/06-testing.md` |
| Một khuyến nghị vận hành theo mode | `best-practices-<mode>.md` — **và nêu rõ mode** |
| Một dòng phần cứng / BIOS / kernel / NIC | `hft-playbook.md`; nếu là row OS thì **`DESIGN.md` §9 trước**, playbook chỉ trỏ |

`CLAUDE.md` §3 thêm `docs/internals/` làm lối vào cho người sửa code.
Charter `GUIDE.md` **không đổi một chữ**; §9 vẫn là bảng OS chuẩn duy nhất.

## Bất biến bị đụng tới

Việc này không đụng `codec`, `session`, `engine` hay `transport`. Nhưng ba điều của §2 bị đụng
vì tài liệu **trích lại** thứ chúng canh:

- **Điều 10 — số phải kèm benchmark, máy, cài đặt §9.** `CONFORMANCE.md` và `hft-playbook.md` là
  hai file rủi ro nhất. Giữ bằng: mỗi hàng có ba thứ đó; **mục "chưa chứng minh" nằm cùng trang**;
  và **không con số nào mới được sinh ra ở đây** — mọi số chép từ `STATUS.md` "Proven" kèm nguồn.
- **Điều 4 — mọi tuyên bố phải nêu mode.** `CONFIGURATION.md`, hai file best-practice và playbook
  chạm trực tiếp. Giữ bằng: mỗi bảng có cột hoặc nhãn mode; **dòng không nêu mode là dòng chưa xong**.
- **Điều 5 — thứ tự trường từ bảng sinh, không từ nơi gọi.** `SESSION-BEHAVIOUR.md` và
  `internals/01-codec.md` phải nói điều đó và **không được trưng một message xếp tay**.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| **A0** | `STATUS.md`: **item 33**, dòng *Plan → Closes*, *Where the work is*, ô *Plan in flight* | — |
| **A1** | `README.md:23` + fence `GUIDE.md:331` | A0 |
| **A2** | `README.md`: thứ tự đọc, bước bootstrap | A1 |
| **B1** | `CLAUDE.md` §4 (9 + 8 dòng) và §3. **Nói rõ đã sửa luật nào.** Một commit riêng | A2 |
| **B2** | `docs/INTRODUCTION.md` | B1 |
| **B3** | `docs/CONFIGURATION.md` — cả 10 key, mỗi hàng ghi `file:dòng` | B1 |
| **B4** | `docs/SESSION-BEHAVIOUR.md` — mỗi hàng trỏ `.def` hoặc test | B1 |
| **B5** | `docs/CONFORMANCE.md` — chỉ số có trong `STATUS.md` "Proven" | B1 |
| **B6** | Job CI `cargo doc`, `deny(rustdoc::broken_intra_doc_links)`, mở rộng `check-links.py` | B2–B5 |
| **C1** | `internals/README.md` — bản đồ + đường đi end-to-end | B1 |
| **C2** | `01-codec.md`, `02-dict.md` | C1 |
| **C3** | `03-session.md` | C1 |
| **C4** | `04-engine.md` — theo thứ tự `Engine::turn` gọi | C1 |
| **C5** | `05-patterns.md` — chỉ kỹ thuật ≥2 nơi | C2, C3, C4 |
| **C6** | `06-testing.md`, `07-contributing.md` | C5 |
| **D1** | `best-practices-standard.md` | B3 |
| **D2** | `best-practices-hft.md` | B3 |
| **D3** | `hft-playbook.md` — 7 mục, trỏ §9 | D2 |
| **E** | `GETTING-STARTED.md`, `TUTORIAL.md`, `examples/` | **Chặn: `library`** |

**Mỗi bước C và D là một commit riêng.** Một trang internals sai còn tệ hơn không có, nên chúng
qua gate từng cái chứ không gộp.

## Cách kiểm chứng

**"Đã viết xong" không phải bằng chứng.**

- **A1** — `grep -n 'not yet built' README.md` không ra dòng nào. Fence: script đếm fence
  (`docs/GUIDE.md` từ `suspicious: [331]` thành rỗng), **và mắt đọc lại đoạn render**.
- **A2, B2–B5** — `scripts/check-links.py` xanh, **kèm đảo ngược**: thêm một link chết vào file
  mới, thấy đỏ, gỡ ra, thấy xanh. Không đảo ngược thì không biết gate có đọc file mới.
- **B3** — đối chiếu **từng hàng** với code, không đọc từ trí nhớ. Mười key của `settings.rs`;
  `DEFAULT_HEART_BT_INT`, `DEFAULT_MAX_SKEW_MS`, `ring::DEFAULT_CAPACITY`,
  `block::DEFAULT_TIMEOUT_MS`/`MIN_TIMEOUT_MS`, `journal::SLOTS`/`SLOT_LEN`, alias
  `TcpAcceptorEngine`, và `Limits` **không có `Default`**. Mỗi hàng ghi `file:dòng`.
  Thêm một kiểm nữa: **`grep` mỗi key trong bảng ra được ở `settings.rs`** — key trong tài liệu
  mà code không nhận là lỗi ngược chiều mà chỉ có bước này bắt.
- **B4** — mỗi hàng nêu tên `.def` hoặc tên test. **Hàng không có thứ giữ nó thì xoá hàng**,
  không đổi thành văn xuôi.
- **B5** — mỗi số đối chiếu `STATUS.md` "Proven". Số không truy được nguồn ở đó thì **không lên
  trang**, và tên nó chuyển sang mục "chưa được chứng minh".
- **B6** — `cargo doc --workspace --no-deps` sạch; đảo ngược bằng một intra-doc link sai, thấy
  build đỏ. `check-links.py`: cho nó một `https://github.com/` trống path, thấy đỏ.
- **C1–C6** — mỗi khẳng định về cơ chế **trích `file:dòng`**, và người viết **mở đúng dòng đó ra
  đọc**. Quy tắc đóng bước: **một câu không truy được về code là một câu bị xoá.** Thêm:
  - `05-patterns.md` — mỗi kỹ thuật liệt kê **≥2 nơi dùng thật**; một chỗ thì chưa phải pattern.
  - `06-testing.md` — mỗi gate nêu **lệnh chạy** và **nhánh đỏ**. Gate nào không nêu được cách làm
    nó đỏ thì **ghi thẳng là chưa có**, thay vì mô tả như đã có.
  - `04-engine.md` — đọc `Engine::turn` một lượt và xác nhận **thứ tự trong trang khớp thứ tự
    lời gọi**; lệch thứ tự là thấy được, và đó là mục đích của cách viết đó.
- **D1, D2** — rà từng dòng theo §2 điều 4: **dòng nào không nêu mode là dòng chưa xong.** Số
  `hft` không được xuất hiện trong trang `standard` và ngược lại. Trang `standard` phải nói thẳng
  phần lớn của nó **chưa được đo**.
- **D3** — đối chiếu ngược `DESIGN.md` §9: playbook **không được** chứa bảng row đó. `grep`
  `nohz_full` và `mitigations` trong playbook, **đọc lại tay**, xác nhận đúng chiều đã đo —
  **`nohz_full` không khuyến nghị**, **mitigations bật**. Đây là hai chỗ một trang HFT viết theo
  thói quen ngành sẽ nói **ngược** sự thật đo được ở repo này.
- **Toàn C/D** — `check-links.py` xanh, kèm **một lần đảo ngược cho `docs/internals/`** để chứng
  minh gate có đi vào thư mục mới.
- **Đóng plan** — `cargo test --all`, `cargo test --all --no-default-features`,
  `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, và **một run CI xanh
  nêu đích danh bằng id cho commit được đóng** (§9 ô cuối).

## Tài liệu phải cập nhật

- [ ] `STATUS.md` — **A0**: item 33, *Plan → Closes*, *Where the work is*, *Plan in flight*.
      Cập nhật lại mỗi lần đóng một phase; đánh dấu item 33 đóng khi D3 xong (**E ở lại, gắn với
      `library`**)
- [ ] `CLAUDE.md` §4 (hai bảng) và §3 — **nói rõ đã sửa luật nào**; một commit riêng, trước mọi
      bước viết
- [ ] `README.md` — thứ tự đọc, bootstrap, dòng 23
- [ ] `docs/GUIDE.md` — fence dòng 331
- [ ] `docs/plans/2026-09-02-the-doc-set.md` — chính file này, **Nhật ký giao hàng** điền mỗi phase
- [ ] `docs/PRD.md` — **không đổi**: plan này không dời việc giữa các phase
- [ ] `CHANGELOG.md` — **không đổi**: không API nào đổi

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Plan viết trên nền cũ, repo đã đi trước — **đã xảy ra một lần, Sửa 1** | Trước mỗi phase: `git pull`, rồi **đo lại** các con số ở mục Bối cảnh của phase đó trước khi viết |
| Tài liệu mới mâu thuẫn `GUIDE.md`/`DESIGN.md` — "một luật, một chỗ" vỡ | Bảng charter ở mục Cách làm; đọc chéo tay từng file trước khi đóng bước |
| Chép §9 vào playbook rồi hai bên trôi khác nhau | Charter cấm chép; D3 đối chiếu ngược; `check-machine.sh` vẫn là gate duy nhất |
| Số trong `CONFORMANCE.md` thiếu máy/lệnh/run id → vi phạm §2.10 | B5: số không truy về `STATUS.md` "Proven" thì không lên trang |
| Tuyên bố không nêu mode → vi phạm §2.4 | Nhãn mode mỗi bảng; rà tay khi đóng B3, D1, D2 |
| `check-links.py` không thực sự đọc file mới → gate xanh giả | **Đảo ngược bắt buộc** ở A2/B2–B5 và một lần cho `internals/` |
| Bảng config chép sai mặc định, **hoặc nêu key code không nhận** | B3: hai chiều — `file:dòng` cho mỗi hàng, **và `grep` key ngược lại `settings.rs`** |
| Tài liệu hứa `cargo add fixbolt` khi chưa publish | Không trang nào có lệnh cài từ registry |
| Trưng snippet `serve*` như thể đã kiểm, khi không gate nào chạy qua | Ví dụ chạy được hoãn sang E; phase B/C **chỉ trích code có test đứng sau**, và nói rõ khi không có |
| `docs/internals/` kể lại `DESIGN.md` → hai bản mô tả trôi | Dòng charter cố định đầu mỗi trang; quy tắc "trích `file:dòng`, không kể lại"; rà chéo khi đóng C6 |
| Trang internals mô tả code **đã đổi** — loại tài liệu chết nhanh nhất | Dòng sync mới ở §4 là cơ chế duy nhất; `file:dòng` khiến sai **thấy được**; C4 theo thứ tự lời gọi nên lệch là thấy được |
| Playbook nói theo thói quen ngành, ngược cái repo này đo được | D3 kiểm riêng `nohz_full` và mitigations; cả hai đã có ADR (0021, 0023) |
| `05-patterns.md` phong thánh một dòng code thành "pattern" | Quy tắc ≥2 nơi dùng thật, kiểm khi đóng C5 |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| **17 file mới trở thành 17 file cũ** — §4: "tài liệu cũ tệ hơn không có" | **Cao nhất** | Tám dòng bảng sync là cơ chế duy nhất giữ chúng sống; đó là lý do B1 đứng trước mọi bước viết. `file:dòng` khiến trang sai **thấy được**, không sai âm thầm |
| Repo đi nhanh hơn tài liệu — 73 commit trong ~1 ngày | **Cao** | Phase nhỏ, mỗi bước một commit, đo lại trước mỗi phase. Trang mô tả **cơ chế** già chậm hơn trang mô tả **API** |
| Phạm vi phình: làm một lượt thì không cái nào xong tử tế | Cao | 5 phase độc lập; A merge ngay; **dừng sau bất kỳ phase nào mà phần đã làm vẫn đứng vững** |
| Phase E chặn vô hạn vì `library` chưa có plan | Trung bình | A–D độc lập và có giá trị ngay; E ghi rõ phụ thuộc, không viết trước |
| Trang internals thành bản dịch của code — đọc code còn nhanh hơn | Trung bình | Charter là **cơ chế và vì sao**, không phải chú giải từng dòng. `internals/README.md` (đường đi end-to-end) và `05-patterns.md` là hai trang mang giá trị mà đọc code không có |
| `hft-playbook.md` bị đọc như lời hứa hiệu năng | Trung bình | Mục 7 nằm **trong cùng trang**, cùng khuôn `CONFORMANCE.md` |
| `SESSION-BEHAVIOUR.md` viết hành vi mà 59 `.def` **không** kiểm, tưởng là có | Trung bình | Quy tắc B4: hàng không nêu được `.def`/test thì xoá. `reference/a-conformance-corpus-is-not-an-adversarial-one.md` đã ghi corpus không phải bộ đối kháng |
| README phình thành tài liệu thứ hai | Thấp | README chỉ thêm thứ tự đọc và bootstrap |
| Lộ thứ không được lộ (§0) | Thấp | Không file nào chứa capture, cấu hình counterparty, hay nội dung `vendor/` |

## Ngoài phạm vi

Cố ý **không** làm:

- **mdBook, GitHub Pages, bất kỳ site nào** — đã chọn Markdown trong `docs/`.
- **`docs/OPERATIONS.md`** — `GUIDE.md` §8a đã làm; viết thêm là tạo bản thứ hai để trôi.
- **Tutorial, getting-started, `examples/`** — chờ `library` (phase E).
- **Tách `docs/GUIDE.md`** — nhiều nơi trỏ vào nó; lợi ích không bù rủi ro.
- **Sửa việc bốn `serve*` không có test.** **Khiếm khuyết code, không phải tài liệu**; cần plan
  riêng (Rule Zero). Plan này chỉ **ghi nhận** ở item 33.
- **Đo mới.** Mọi dòng phần cứng/BIOS/NIC chưa đo ở đây được **dán nhãn là chưa đo**. Biến một
  dòng thành số là plan khác, cần máy §9.
- **Sửa `DESIGN.md` §9.** Playbook trỏ vào nó; §9 thiếu row thì đó là thay đổi của §9, đi riêng.
- **Doctest / `# Examples` trong rustdoc** — thuộc E, cần API ổn định.
- **Tài liệu TLS, SBE, FIX 5.0, FAST, FIXML** — phase 2 của `PRD.md`, chưa có code.
- **Dịch tài liệu sang tiếng Việt** — §6: mô tả hệ thống thì tiếng Anh.
- **Tài liệu initiator ngoài phần đã đúng hôm nay** — §7 bước 5 đang tạm dừng.

## Nhật ký giao hàng

*(điền khi đóng từng phase)*
