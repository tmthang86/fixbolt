# Lần thứ hai ở bàn Linux: NIC thật, cache lạnh, và những con số còn thiếu

> **Loại:** Plan · **Ngày:** 2026-09-04 · **Trạng thái:** Draft
> **Phạm vi:** `STATUS.md` item 45, đợt C — **một plan cho một lần ngồi ở máy §9**, gom mọi
> việc cần máy: item 40, 39, 41, `SO_BUSY_POLL`, cache lạnh, `mlockall`, hai case alloc chưa có.
> Chạm `tools/w2w`, `scripts/`, `benches/`, `engine` (vài socket option và `mlockall` sau feature),
> docs `DESIGN.md` §6 §8 §9, `hft-playbook.md`.
>
> **Draft viết 2026-09-04.** Khi đến lượt: chạy `scripts/check-machine.sh` trước, đọc lại
> `benches/baselines.tsv` (đợt A và B có thể đã đổi vài hàng), và **chuẩn bị máy thứ hai + NIC
> từ trước** — bước 1 không làm được trong ngày nếu phần cứng chưa có. Sửa rồi mới *Chờ duyệt*.
>
> **Máy chạy:** máy §9 (Ryzen 7 3700X hiện tại) **và một máy thứ hai** nối bằng NIC 10/25G qua
> cáp trực tiếp hoặc một switch có tên. **Thời lượng dự kiến:** 2 ngày tại máy, 1 ngày chuẩn bị
> code trước đó trên macOS.

## Bối cảnh

Mọi số wire-to-wire fixbolt công bố là **loopback**, back-to-back, cache nóng, N = 1. Ba trong
bốn tính từ đó là best case mà tài liệu đã nói rõ, nhưng nói rõ không phải là đo. Lần đầu ở bàn
Linux (`w2w-and-linux-numbers`, `fixbolt-and-the-linux-desk`) đóng phase 1; lần này trả nốt
những gì phase 1 không hỏi, và đo giá của những gì đợt A thêm vào (`FileLog`, ring 4096).

## Những gì đã biết chắc (2026-09-04 — xác minh lại khi làm)

| Sự thật | Nguồn |
|---|---|
| Loopback §9 desktop: `hft` p50 16 010 / p99 20 589 / p99.9 22 127 ns admin; app 19 908 / 24 657 / 26 150; `standard` +3 437 ns | `DESIGN.md` §8, `measured-costs.md` |
| `check-machine.sh` `pass 12 fail 0 unknown 1` — unknown là NIC IRQ affinity | `STATUS.md` 2026-09-02 |
| `w2w`: `--messages`, `--warmup`, `--hold-ms` (một lần chờ, **không phải** pacing), `--mode`, `--path`, `--engine-core`, `--client-core`, `--allow-unisolated`; đếm alloc hai thread; in p50/p99/p99.9 | `tools/w2w/src/main.rs:327–389` |
| Không có `SO_TIMESTAMPING`, không HdrHistogram trong `w2w` | grep 2026-09-04 |
| Không có `SO_BUSY_POLL`, `TCP_QUICKACK`, `mlockall` ở đâu trong `crates/` | grep 2026-09-04 |
| `isolcpus` đáng 11× ở p99.9 (26 300 với, 266 887 không) và không đáng gì ở p50 | `DESIGN.md` §9 |
| Mitigations là term lớn nhất: `turn` 448.9 → 175.2 ns khi tắt; **§9 yêu cầu bật** | ADR-0023 |
| Item 39: dictionary pass chưa được đo; app round trip cao hơn admin 3 898 ns, bench giải thích ~320 | `STATUS.md` item 39, ADR-0045 |
| Item 41: `bench.sh --strict` đỏ trên §9 desktop trước mọi thay đổi; một case +140% là `presession, read and route` | `STATUS.md` item 41 |
| `FileJournal` `Async` push qua ring 1 MiB — **không có case alloc**; `FileLog` (sau plan message-log) cùng hình | plan 2026-09-03-message-log |
| NAPI busy poll chỉ có ý nghĩa với NIC thật; loopback không có NAPI | `prior-art.md` 2026-09-03 |
| Đợt B `timestamp-micros` để lại một hàng *unmeasured* nếu chưa có máy | plan đó |
| `matthart1983/nanofix` HEAD 2026-07-05: thread-per-connection (`server.rs:117`), `recv` chặn với `set_read_timeout(100ms)` và `TCP_NODELAY` bật (`transport_tcp.rs:23–37`), **không có** code pin core | đọc source 2026-09-05 |
| `nanofix` `build.rs` gọi cmake/Aeron vô điều kiện, có đường dẫn `/Users/matt/...` (`build.rs:81`); `src/lib.rs` **0** `#[cfg(feature)]`; `ResendRequest` luôn trả GapFill (`engine.rs:612–629`) | đọc source 2026-09-05 |

## Cách làm — thứ tự trong hai ngày

**Trước khi tới máy (macOS, 1 ngày):**

- `w2w --interval <us>`: gửi một message mỗi `interval` micro giây, chờ bằng spin trên client
  thread (không `sleep`, client không phải engine thread nhưng jitter của `sleep` sẽ vào số đo);
  in cùng ba percentile; `--interval 0` là hôm nay.
- `w2w --busy-poll <us>`: đặt `SO_BUSY_POLL` (và `SO_PREFER_BUSY_POLL` nếu kernel có) trên
  socket engine — **là một `Transport` option** sau `#[cfg(target_os = "linux")]`, không phải cờ
  tool; tool chỉ truyền xuống.
- `engine::memory::lock_all()` sau feature `affinity` (cùng `libc`): `mlockall(MCL_CURRENT |
  MCL_FUTURE)`, pre-fault `RX`/`TX`/ring/journal bằng cách ghi một byte mỗi trang, rồi **đọc lại**
  `VmLck` từ `/proc/self/status` và trả về số KiB — người gọi so với kỳ vọng. `w2w --mlock` gọi
  nó và in `VmLck`.
- Hai case `benches/alloc.rs`: `journal-async-busy` (`FileJournal` `Async`, một message qua,
  đọc file sau `close`) và `log-busy` nếu plan message-log chưa thêm.
- `benches/dict_pass.rs` (item 39): thời gian của dictionary pass trên `NewOrderSingle` tách
  khỏi `parse_into` — đo cùng khung, cùng máy với `benches/parse.rs`.
- `w2w --wire-timestamps`: `SO_TIMESTAMPING` (`SOF_TIMESTAMPING_RX_HARDWARE|TX_HARDWARE|RAW_HARDWARE`)
  trên client, đọc `cmsg`; nếu NIC không hỗ trợ, in `unsupported` và **không** in số đó — không
  fallback sang software timestamp mà ghi cùng cột.

**Ngày 1 tại máy — loopback, đóng những gì loopback trả lời được:**

1. `check-machine.sh` → phải `pass`; `bench.sh --strict` → đọc item 41 **trước** khi chạm gì:
   lấy lại baseline cho `presession, read and route` nếu suite đã đổi thành phần (item 41 nói
   thế), ghi vào `baselines.tsv` với N ≥ 20.
2. `w2w --interval 0 / 1 000 / 10 000 / 1 000 000` (back-to-back, 1 kHz, 100 Hz, 1 Hz) — cả
   hai mode, cả hai path. **Kỳ vọng**: p50 tăng khi thưa (cache, branch predictor, C-state dù đã
   tắt); số này là *"latency đối tác thấy lúc 3 giờ sáng"* và vào §8 như một hàng riêng.
3. `w2w --mlock`: `VmLck` đọc lại; so p99.9 có/không.
4. `benches/dict_pass.rs`: điền item 39; nếu pass > 1 µs, mở item cho một knob (đợt B đã có
   `DictionaryChecks`).
5. `journal-async-busy`, `log-busy` — 0; và **giá**: `w2w` với `FileJournal` `Async` và với
   `FileLog` bật, so với không — điền hàng §8 "if a journal/log is on" mà plan message-log để
   *unmeasured*.
6. Nếu đợt B để lại hàng `SendingTime` micro *unmeasured*: `bench.sh` arm đó.
7. **Đối chứng với `matthart1983/nanofix`, cùng máy, cùng harness, cùng §9** — xem mục dưới.

**Ngày 2 tại máy — NIC thật (item 40):**

7. Máy thứ hai chạy client `w2w --connect <ip>`; IRQ của queue NIC pin về core **không phải**
   engine core (`/proc/irq/*/smp_affinity_list`), `check-machine.sh` hàng IRQ phải chuyển từ
   `unknown` sang `pass` — **sửa script để đọc được**.
8. `w2w` NIC-to-NIC, hai mode, hai path, `--interval 0` và `1 000 000`; với và không
   `--busy-poll 50`; với và không `--wire-timestamps`. Ghi **topology**: cáp trực tiếp hay
   switch, model NIC, driver, `ethtool -c` (coalescing **tắt** — thêm hàng §9), MTU.
9. `check-no-kernel-sleep.sh` một lần với `--busy-poll`: `SO_BUSY_POLL` **không** đưa thêm
   syscall nào lên engine thread (nó thay đổi việc kernel làm trong `recv`, không thêm call).

### Bước 7 — đối chứng với `matthart1983/nanofix`

`[thêm 2026-09-05]` **Mọi số fixbolt công bố đều là fixbolt tự đo.** Con số duy nhất của một
engine Rust khác mà repo này đang mang là README của `nanofix` — Apple Silicon, Criterion, máy
khác, phương pháp khác — và `prior-art.md` gọi đúng nó là *claim của người khác*. Một lần ngồi ở
máy §9 là dịp rẻ nhất để biến nó thành **một phép đo**, vì harness, máy và các hàng §9 đã sẵn.

**Cách:** `tools/w2w` ở vai client, đối diện là `FixServer` của `nanofix` thay cho engine fixbolt;
cùng máy, cùng `check-machine.sh` output, cùng shape message, cùng số vòng, medians of 20 runs.
Ghi commit của `nanofix` đã dùng.

**So với `standard`, không so với `hft`, và lý do là điều 4.** `nanofix` là thread-per-connection
với `recv` chặn, `set_read_timeout(100ms)`, `TCP_NODELAY` bật (`src/server.rs:117`,
`src/transport_tcp.rs:23–37`) — hình dáng đó là `standard`, không phải `hft`. Một phép so `hft`
với nó là phép so hai mode, mà điều 4 nói thẳng là không đầy đủ. Nếu vẫn muốn in số `hft`, in
thành **hàng riêng** kèm câu nói rõ nó so cái gì với cái gì.

**Ba điều phải nói ra cạnh mọi con số, nếu không con số là bẫy:**

| Điều | Vì sao |
|---|---|
| `build.rs` của `nanofix` gọi cmake/Aeron **vô điều kiện** và có đường dẫn máy tác giả (`build.rs:81`); `src/lib.rs` có **0** `#[cfg(feature)]` | phải vá mới link được — nên số đo là của **một bản đã vá**, và bản vá phải được ghi lại. `measured-costs.md` §1 đã làm đúng việc này một lần |
| `nanofix` trả lời `ResendRequest` bằng GapFill **luôn luôn**, journal mmap không nối vào engine (`src/engine.rs:612–629`) | arm resend **không so được**. Đo arm nào thì nói arm đó |
| Journal và message log của fixbolt bật hay tắt | hai engine phải làm cùng lượng việc, hoặc phải nói ra là không |

**Kết quả đi đâu:** `measured-costs.md` — đây là số của repo này, đo bằng tay repo này. Hàng
claim trong `prior-art.md` **giữ nguyên là claim** và trỏ sang; không được sửa nó thành số đo.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| **4 — hai nửa** | socket option mới trên engine socket | `check-no-kernel-sleep.sh` với option bật; `check-standard-gives-the-core-back.sh` không đổi (busy poll là trong `recv`, `standard` vẫn block trên `poll`) |
| **10 — số có benchmark, máy, §9** | mọi số mới | mỗi hàng ghi lệnh, `check-machine.sh` output, topology NIC |
| 1 — không cấp phát | `mlockall`, pre-fault là startup | ngoài cửa sổ đếm, nói rõ |
| 6 — feature gate | `lock_all`, busy-poll sau `cfg`/feature | `check-no-optional-deps.sh` |

## Tài liệu phải cập nhật

`DESIGN.md` §6 (hàng NIC-to-NIC **MET** với topology; hàng cache lạnh mới; item 39 hàng dictionary
pass; hàng journal/log), §8 (bốn hàng), §9 (hàng IRQ từ `unknown`; hàng coalescing mới; hàng
`SO_BUSY_POLL` với số; hàng `mlockall` giờ là code), `hft-playbook.md` (bước IRQ và coalescing
với lệnh), `best-practices-hft.md` (nêu mode), `measured-costs.md` (mọi số), `CONFORMANCE.md` §6
(dòng "no latency figure lives here" không đổi), `STATUS.md` (40, 39, 41 và *Not proven*),
`benches/baselines.tsv`, `prior-art.md` (hàng claim của `nanofix` **giữ nguyên là claim**, thêm
đường trỏ sang số đo ở `measured-costs.md` — bước 7).

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| `--interval` bằng `sleep` → jitter của scheduler vào số đo | client spin; kiểm bằng `strace` client không `nanosleep` trong cửa sổ đo |
| So sánh NIC với loopback như thể cùng thang | §8 hai bảng riêng, không bao giờ trừ cho nhau |
| `SO_BUSY_POLL` "không thay đổi gì" vì `net.core.busy_poll` sysctl không đặt | `check-machine.sh` hàng mới đọc sysctl; in cả hai |
| Hardware timestamp không có nhưng software timestamp im lặng thay vào | cột riêng, `unsupported` là chữ, không phải số |
| Đo `mlockall` khi máy còn RAM trống → không thấy gì | ghi là "không thấy khác biệt trên máy rảnh", giữ hàng §9 vì lý do khác (page fault không xác định) |
| Một buổi đo lại chấm baseline cho một case mà suite đã đổi thành phần (item 41) | baseline mới ghi kèm hash của danh sách case |
| Đem `hft` so với một engine chặn rồi công bố tỉ số — điều 4 gọi đó là phép so không đầy đủ | bước 7: arm chính là `standard`; số `hft` nếu in thì thành hàng riêng, tự khai so cái gì |
| Số của một bản `nanofix` đã vá được đọc như số của `nanofix` | bản vá ghi lại nguyên văn cạnh con số, như `measured-costs.md` §1 đã làm |

## Ngoài phạm vi

Kernel bypass (item 14); `io_uring` (ADR riêng sau khi có số NIC); NIC 100G; đo trên máy cloud.
Sửa hay gửi PR ngược cho `nanofix`: bước 7 chỉ đo, và bản vá để link được là của riêng buổi đo.

## Nhật ký giao hàng

*(draft — chưa duyệt, chưa bắt đầu)*
