# Phase 4, hàng 1: xem engine qua Prometheus và Grafana mà engine không phải trả giá

> **Loại:** Plan · **Ngày:** 2026-09-24 · **Trạng thái:** Đã duyệt (manager duyệt 2026-09-24 theo uỷ quyền 2026-09-18)
> **Phạm vi:** phase 4, hàng 1 của bảng *Chia việc* trong
> [2026-09-23-phase-4-scope.md](2026-09-23-phase-4-scope.md), cộng **nửa tài liệu** của hàng 2
> (`tools/grafana/fixbolt.json` và docs). Cặp đo `w2w` scrape bật/tắt trên máy bàn **vẫn là hàng
> 2**, không nằm trong plan này. Khung đã duyệt: [ADR-0098](../decisions/ADR-0098-phase-4-is-the-owners-five-items-each-entering-behind-a-measurement-that-can-kill-it.md)
> mục 5. Quyết định mới: [ADR-0170](../decisions/ADR-0170-the-metrics-exporter-holds-an-observer-and-nothing-else-allocates-nothing-per-scrape-and-publishes-only-after-its-kill-line.md)
> và [ADR-0171](../decisions/ADR-0171-a-series-name-is-public-api-promtool-is-the-format-oracle-and-the-dashboard-names-no-data-source.md), cả hai *Proposed*.

> Tên file luôn tiếng Anh: `docs/plans/YYYY-MM-DD-<topic>.md`.
> Nội dung viết tiếng Việt, ngôn ngữ dễ hiểu — xem `CLAUDE.md` §6.
> Tên định danh (file, hàm, package, tag FIX, lệnh chạy) giữ nguyên tiếng Anh.

## Bối cảnh

Hôm nay muốn biết engine đang ra sao, người dùng phải tự viết code gọi `Observer::request()` rồi
tự in ra. Người lạ mà phase 3 mời vào quen một cách khác: trỏ Prometheus vào một cổng `/metrics`,
mở Grafana, thấy biểu đồ. ADR-0098 mục 5 đã chốt khung: một crate riêng `fixbolt-metrics`, chạy
trên **luồng của nó**, chỉ đọc cái engine đã có sẵn cho việc quan sát, không runtime async, không
thêm dependency; và một **vạch bỏ**: nếu scrape 10 lần một giây làm p50/p99 của `w2w` ra khỏi band,
hoặc luồng engine phải cấp phát, thì exporter không được ship.

Đọc code thì thấy khung đó còn bảy chỗ chưa trả lời (chi tiết ở ADR-0170 *Context*). Ba chỗ quan
trọng nhất, nói nôm na:

- **Hỏi snapshot thì nhận cái cũ.** `Observer::request()` trả snapshot *lần trước*, rồi mới xin
  cái mới. Prometheus scrape mỗi 15 giây thì mỗi lần sẽ thấy số liệu cũ 15 giây.
- **Ai scrape càng dày thì engine càng phải làm.** Mỗi lần xin là luồng engine dựng một snapshot.
  Hai Prometheus chạy cặp, hoặc một vòng `curl`, có thể bắt engine làm việc đó bao nhiêu lần tuỳ ý.
- **Đọc event là lấy mất event.** `Observer::events()` rút event ra khỏi hàng; exporter mà đọc thì
  ứng dụng của người dùng không còn thấy.

Plan này dựng crate đó, thêm vào `Snapshot` hai con số `PRD.md` §3 còn thiếu (độ đầy của ring gửi
sang ứng dụng, và số chỗ đang dùng ở tầng chờ Logon), viết file dashboard Grafana, và chuẩn bị
đủ đồ để hàng 2 đo vạch bỏ trên máy bàn.

## Những gì đã biết chắc

Đọc code ở worktree `fb-p4r1`, commit `094bfc3`, ngày 2026-09-24 (chỉ đọc, không chạy cargo):

- **`Observer::request()`** (`crates/engine/src/observe.rs:497-503`) bật cờ `wanted` rồi trả bản
  đang nằm trong ô (`Mutex<Snapshot>`), `None` nếu engine chưa công bố lần nào. Không có cách nào
  đọc ô mà **không** bật cờ.
- **Engine chỉ dựng snapshot khi có người xin**: đầu mỗi `Engine::turn`, một lần đọc atomic
  `wanted`; nếu bật thì dựng `Snapshot` trên stack (mảng cố định 64 phiên) và `try_lock` để chép
  vào ô — lock bận thì bỏ qua, lượt sau làm (`crates/engine/src/lib.rs:1204-1216`,
  `observe.rs:400-406`). Luồng engine không bao giờ chờ lock.
- **Engine `standard` đang ngủ trong `poll` sẽ không thấy cờ** cho tới khi có gì đánh thức nó;
  `request()` không đánh thức engine.
- **Đã có hai case alloc cho quan sát**: `observe-idle`, `observe-asked`
  (`crates/engine/benches/alloc.rs:620-700`), đều dùng `InlineDispatch`, nên đường đọc ring chưa
  bao giờ được đếm.
- **Độ đầy ring đọc được sẵn**: `ring::Producer::free()` (`crates/engine/src/ring.rs:195-200`),
  hai lần đọc atomic. Chỉ `RingDispatch` giữ `Producer` (`crates/engine/src/dispatch.rs:168-176`),
  và engine chỉ thấy nó qua trait `Dispatch`. Chưa có hàm trả dung lượng ring.
- **Số socket đang chờ Logon** là `PendingSet::len()` cộng số kết nối đang "đỗ" chờ writer của
  journal (ADR-0154), trần là `Limits::pending()` (`lib.rs:3500`). Tầng chờ Logon thuộc về vòng
  phục vụ, không thuộc engine; con số `unframeable_prelogon` đi đúng đường này — vòng phục vụ gọi
  `Engine::note_unframeable` mỗi vòng (`lib.rs:836-838`, `:3515`).
- **`serve_sharded_hft` không đưa `Handles`** (`crates/engine/src/shard.rs` không có `adopt`):
  runtime chia shard hôm nay không quan sát được qua `Observer`. Plan này không sửa chuyện đó.
- **`tools/w2w` đếm cấp phát trên mọi luồng** trong cửa sổ đo và assert bằng 0
  (`tools/w2w/src/main.rs:294-343`). Các script mode (`check-standard-gives-the-core-back.sh`,
  `check-no-kernel-sleep.sh`, `check-no-kernel-sleep-by-ctxt.sh`) chạy `w2w` và nhận thêm cờ qua
  biến `W2W_EXTRA`.
- **`scripts/bench.sh` tự tìm mọi bench target qua `cargo metadata`**; bench tên `alloc` được coi
  là bất biến (chặn CI), không phải số đo thời gian (`scripts/bench.sh:50-58`).
- **Khuôn publish** (ADR-0160): `version.workspace = true`, dependency nội bộ ghim `=0.1.0`,
  `include` là danh sách cho phép, hai file licence chép vào crate, `rust-version` thừa kế (hiện
  1.89). `scripts/check-release-versions.sh` giữ sáu crate; mọi member khác phải `publish = false`.
- **Phase 3 chưa đóng** lúc viết plan (`STATUS.md` *Start here* 2026-09-24). Anh quyết ngày
  2026-09-24: **không publish lên crates.io**; phase 3 đóng bằng tag `v0.1.0` (ADR-0161, đang
  viết). Bước 0 của phase 4 là kiểm chuyện đó.
- **`DropReason`** là enum không có field, `#[non_exhaustive]`, khoảng 24 biến thể
  (`crates/session/src/lib.rs:1235`).

Ngoài repo (tra 2026-09-24; lời người khác, chưa kiểm ở đây — link đầy đủ ở ADR-0170 và ADR-0171
mục *Research*):

- Định dạng text 0.0.4 của Prometheus: `HELP`/`TYPE` trước mẫu, mỗi metric một nhóm, escape
  `\\ \" \n` trong giá trị label, dòng cuối phải có `\n`. **Từ Prometheus 3.0, thiếu hoặc sai
  `Content-Type` là scrape hỏng** — <https://prometheus.io/docs/instrumenting/exposition_formats/>.
- Chuỗi `text/plain; version=0.0.4, charset=utf-8` (dấu phẩy thay dấu chấm phẩy) làm Prometheus 3
  từ chối — <https://github.com/prometheus/prometheus/issues/15777>.
- Server không khớp `Accept` thì *"MUST use PrometheusText0.0.4"* —
  <https://prometheus.io/docs/instrumenting/content_negotiation/>.
- `metrics-exporter-prometheus` mặc định kéo Tokio và hyper; `prometheus-client` và `prometheus`
  (tikv) không có HTTP server, kéo `parking_lot` và vài crate khác — Cargo.toml trên docs.rs.
- Aeron (và Artio dựa trên nó) để counter là atomic trong một file map bộ nhớ, công cụ ngoài đọc
  mà không đụng luồng nóng — <https://github.com/aeron-io/aeron/wiki/Monitoring-and-Debugging>.
- Seqlock trong Rust hiện là data race (UB) nếu dữ liệu không phải atomic, cho tới khi có
  "atomic memcpy" — <https://github.com/rust-lang/rfcs/pull/3301>. Vậy giữ ô `try_lock` hiện có.
- Đặt tên metric: đơn vị cơ bản (giây, byte), counter có `_total`, không để label có vô số giá
  trị — <https://prometheus.io/docs/practices/naming/>.
- `promtool check metrics` đọc text từ stdin và lint —
  <https://prometheus.io/docs/prometheus/latest/command-line/promtool/>.
- Dashboard xuất kiểu "chia sẻ" có `__inputs` / `${DS_PROMETHEUS}` thì **hỏng khi nạp bằng file
  provisioning** — <https://github.com/grafana/grafana/issues/10786>.

**Tìm mà không thấy:** engine FIX mã nguồn mở nào tự ship exporter Prometheus; số đo nào về ảnh
hưởng của exporter lên một luồng độ trễ thấp chạy cùng máy.

## Cách làm

Lý do và phương án bị loại ở ADR-0170, ADR-0171. Ở đây chỉ phương án được chọn.

### A. Phần engine (nhỏ, đều là thêm mới, không đổi hành vi cũ)

1. **Kiểu mới `observe::Occupancy { used, capacity }`** (`Copy`, hai `usize`, có hàm đọc).
2. **`Snapshot::ring_to_app() -> Option<Occupancy>`** — tính bằng **byte**. Nguồn: phương thức
   mới có sẵn mặc định trên trait, `Dispatch::ring_to_app(&self) -> Option<Occupancy>` trả
   `None`; `RingDispatch` trả từ `Producer::free()` và hàm mới `Producer::capacity()`. Chỉ đọc lúc
   dựng snapshot. Chỉ chiều engine → ứng dụng (chiều gây ngắt kết nối theo D10b).
3. **`Snapshot::presession_slots() -> Option<Occupancy>`** — số chỗ đang dùng ở tầng chờ Logon
   (đang chờ + đang đỗ) trên trần `Limits::pending()`. Vòng phục vụ báo cho engine bằng hàm mới
   `Engine::note_presession_slots(used, capacity)`, gọi **ngay cạnh mọi chỗ đang gọi
   `note_unframeable`**. Một lệnh ghi mỗi vòng.
4. **`None` nghĩa là "không ai báo"** (`InlineDispatch`, initiator, engine dựng tay, runtime chia
   shard) — khác với "rỗng". Exporter khi đó **bỏ hẳn series**, không xuất số 0.
5. **`Observer::latest() -> Option<Snapshot>`**: đọc ô mà **không** bật cờ.
6. Hàm `pub(crate)` `set_counters` và `Engine::snapshot` nhận thêm tham số; không đổi API công khai
   nào đang có. `cargo-semver-checks` phải thấy toàn bộ là thay đổi thêm.

### B. Crate mới `crates/metrics` (`fixbolt-metrics`)

- **Dependency duy nhất**: `fixbolt-engine = { version = "=0.1.0", path = "../engine",
  default-features = false }`. Dev-dependency: `fixbolt-engine` có `standard`, `fixbolt-conformance`
  (lấy fixture Logon), cả hai chỉ path. Không feature nào.
- **Đúng khuôn gia đình lockstep ngay từ đầu, nhưng `publish = false`** (ADR-0170 quyết định
  10): thừa kế version, ghim `=`, `include = ["src/**", "README.md", "LICENSE-MIT",
  "LICENSE-APACHE"]`, hai file licence chép vào crate, `README.md` riêng. Hàng 2 đưa crate **vào
  gia đình release theo tag** (thêm vào danh sách của `check-release-versions.sh`, cờ `publish`
  theo đúng cách ADR-0161 để cho sáu crate kia) **cùng commit** với cặp đo qua vạch. **Publish lên
  crates.io không nằm trong kế hoạch** (ADR-0161).
- **API** (tên chốt ở bước 3, hình dạng chốt ở đây):
  `Exporter::builder(addr: SocketAddr)` → `.engine(name: &'static str, observer: Observer)` (gọi
  nhiều lần được) → `.min_request_interval(Duration)` (mặc định 100 ms) → `.fresh_wait(Duration)`
  (50 ms) → `.tick(Duration)` (100 ms) → `.read_timeout(Duration)` (1 s) →
  `.with_events(FnMut(&Event) + Send + 'static)` (tuỳ chọn) → `.spawn() -> Result<Exporter,
  ExportError>`. `Exporter::local_addr()`, `Exporter::stop()` (dừng và join; `Drop` cũng dừng).
  Lỗi là enum `thiserror`-kiểu nhưng viết tay (không thêm dependency): bind hỏng, spawn hỏng.
- **Luồng exporter** tên `fixbolt-metrics`: mỗi `tick` thức dậy — rút event nếu được giao, nhận
  mọi kết nối đang chờ (listener non-blocking), trả lời từng cái trên socket blocking có
  `read_timeout`, đọc tối đa 4 KiB request — rồi ngủ. Không bao giờ spin.
- **Hỏi snapshot tối đa một lần mỗi `min_request_interval`**, dù scrape dày bao nhiêu. Khi được
  hỏi: nhớ `published()`, gọi `request()`, ngủ-từng-1-ms tối đa `fresh_wait` chờ `published()`
  tăng, rồi `latest()`. Xuất `fixbolt_snapshot_age_seconds` = thời gian từ lần cuối thấy
  `published()` tăng. **Không đánh thức engine.**
- **Không cấp phát sau khi khởi động**: bộ đệm trả lời cấp một lần theo trường hợp xấu nhất (mọi
  engine × 64 phiên × mọi series × giá trị dài nhất), bộ đệm request là mảng cố định, bộ đệm event
  đặt trước `EVENT_CAPACITY`. Số viết bằng `core::fmt` vào buffer đã đặt trước; độ lệch đồng hồ
  (ms, `i64`) in thành giây với ba chữ số thập phân, không dùng float.
- **HTTP tối thiểu**: `GET`/`HEAD /metrics` → 200, header **đúng từng byte**
  `Content-Type: text/plain; version=0.0.4; charset=utf-8`, có `Content-Length`,
  `Connection: close`. `/healthz` → 200 nếu mọi engine `Snapshot::healthy()`, ngược lại 503.
  Đường khác 404, method khác 405, request hỏng 400. Không keep-alive, không nén, không TLS.
- **Event chỉ khi bật `with_events`**: đếm `fixbolt_events_total{kind}` và
  `fixbolt_session_ends_total{reason}` trên tập nhãn cố định; biến thể `DropReason` lạ trong tương
  lai → `reason="other"`; mỗi event đưa cho closure của người dùng trên luồng exporter.
- **Module**: `src/lib.rs` (API, builder), `src/series.rs` (bảng tên/loại/HELP — nguồn duy nhất),
  `src/encode.rs` (bộ encode 0.0.4 trên một `View` nội bộ, để test dựng được 64 phiên mà không cần
  engine thật), `src/http.rs` (đọc request, viết header), `src/thread.rs` (vòng lặp).
- **Example**: `examples/acceptor_with_metrics.rs` (người dùng đọc: `serve` + exporter ở
  `127.0.0.1:9464`) và `examples/scrape_fixture.rs` (in đúng một lần scrape của fixture đủ ba
  kiểu triển khai ra stdout — đầu vào của `promtool`). Không nằm trong `.crate`.

### C. Series — đây là API công khai (ADR-0171)

Mọi series có label `engine`. Series phiên có thêm `conn` (`ConnId`). Tên dưới đây là đề xuất;
đổi thì đổi ở đây trước khi code.

| Series | Loại | Nguồn | Ghi chú |
|---|---|---|---|
| `fixbolt_snapshot_available` | gauge | đã có snapshot chưa | 0 cho tới lần công bố đầu |
| `fixbolt_snapshot_age_seconds` | gauge | đồng hồ của exporter | lớn dần khi engine `standard` ngủ |
| `fixbolt_snapshots_published_total` | counter | `Observer::published()` | đứng yên = engine không quay |
| `fixbolt_healthy` | gauge | `Snapshot::healthy()` | 0/1 |
| `fixbolt_connections` | gauge | `connections()` | |
| `fixbolt_sessions_logged_on` | gauge | đếm `logged_on` | |
| `fixbolt_snapshot_truncated` | gauge | `truncated()` | 0/1 |
| `fixbolt_refused_connections_total` | counter | `refused_connections()` | ADR-0011 |
| `fixbolt_unframeable_prelogon_total` | counter | `unframeable_prelogon()` | |
| `fixbolt_sources_missing_total` | counter | `sources_missing()` | |
| `fixbolt_message_log_lost_total` | counter | `log_lost()` | |
| `fixbolt_events_lost_total` | counter | `Observer::events_lost()` | luôn có, kể cả không bật event |
| `fixbolt_ring_to_app_used_bytes`, `fixbolt_ring_to_app_capacity_bytes` | gauge | `ring_to_app()` | chỉ khi `RingDispatch` |
| `fixbolt_presession_slots_used`, `fixbolt_presession_slots_capacity` | gauge | `presession_slots()` | chỉ sau cửa `serve*` |
| `fixbolt_session_logged_on` | gauge | phiên | `conn` |
| `fixbolt_session_next_out_seq_num`, `fixbolt_session_next_in_seq_num` | gauge | phiên | `conn` |
| `fixbolt_session_clock_skew_seconds` | gauge | `last_skew_ms()` | bỏ khi `None` |
| `fixbolt_session_pending_output` | gauge | phiên | 0/1 |
| `fixbolt_session_journal_refused_total` | counter | `puts_refused()` | |
| `fixbolt_session_resend_beyond_journal_total` | counter | `resend_beyond_journal()` | |
| `fixbolt_events_total{kind}` | counter | event | chỉ khi `with_events` |
| `fixbolt_session_ends_total{reason}` | counter | `EventKind::Ended` | chỉ khi `with_events` |
| `fixbolt_exporter_scrapes_total`, `fixbolt_exporter_bad_requests_total` | counter | exporter | không có `engine` |

### D. Dashboard và công cụ kiểm

- `tools/grafana/fixbolt.json`: một biến template kiểu `datasource` (query `prometheus`) và một
  biến `engine`; mọi panel dùng `${datasource}`; **không có `__inputs`**. Các hàng: *Sức khoẻ*
  (`healthy`, tuổi snapshot, phiên đã logon/kết nối), *Mất mát — phải bằng 0*
  (`rate()` của các counter `_total` mất/từ chối), *Áp lực* (ring và tầng chờ Logon theo %),
  *Phiên* (seq in/out, lệch đồng hồ, theo `conn`), *Vì sao phiên kết thúc* (cần `with_events`).
  Mô tả panel nói rõ panel nào cần kiểu triển khai nào.
- `scripts/check-grafana-dashboard.py`: JSON hợp lệ; không `__inputs`, không `${DS_`; mọi data
  source của panel là biến; mọi target có `expr`; id panel không trùng.
- `scripts/check-metrics-format.sh`: tải Prometheus bản ghim (version + SHA-256 viết trong script),
  chạy `cargo run -p fixbolt-metrics --example scrape_fixture | promtool check metrics`. Thiếu
  `promtool` thì in `SKIPPED, NOT PASSED` và thoát khác 0, trừ khi có `--allow-skip`.
- `scripts/scrape-loop.sh <addr> <hz> <seconds>`: vòng `curl` đơn giản, dùng cho script mode ở
  bước 5 và cho hàng 2 trên máy bàn.
- `tools/w2w` thêm `--metrics <addr>` (chỉ ở chạy gộp và `--listen`; `--connect` từ chối): dựng
  exporter từ luồng main **trước** khi luồng engine tự ghim lõi, in dòng `metrics: <addr>`.

**File sẽ tạo:** `crates/metrics/{Cargo.toml, README.md, LICENSE-MIT, LICENSE-APACHE}`,
`crates/metrics/src/{lib.rs, series.rs, encode.rs, http.rs, thread.rs}`,
`crates/metrics/tests/{exporter.rs, series_names.rs, dashboard.rs}`,
`crates/metrics/benches/alloc.rs`, `crates/metrics/examples/{acceptor_with_metrics.rs,
scrape_fixture.rs}`, `crates/engine/tests/observe_occupancy.rs`, `tools/grafana/fixbolt.json`,
`scripts/{check-grafana-dashboard.py, check-metrics-format.sh, scrape-loop.sh}`,
`docs/internals/metrics.md`, một trang `docs/reference/` (xem *Bẫy*).
**File sẽ sửa:** `crates/engine/src/{observe.rs, dispatch.rs, ring.rs, lib.rs}`,
`crates/engine/benches/alloc.rs`, `Cargo.toml` (members), `tools/w2w/src/main.rs`,
`scripts/{check-release-versions.sh, check-no-optional-deps.sh}`, `.github/workflows/ci.yml`
(bước `promtool` và bước kiểm dashboard trong job `gates`), tài liệu ở mục dưới.

## Bất biến bị đụng tới

- **1 — không cấp phát trên hot path.** Luồng engine: dựng snapshot thêm hai lần đọc atomic (ring)
  và chép thêm hai `Option<Occupancy>`; vòng phục vụ thêm một lệnh ghi. Chứng minh: case mới
  `observe-asked-ring` trong `crates/engine/benches/alloc.rs` (engine có `RingDispatch`, tự kiểm
  `ring_to_app()` là `Some` và `used > 0` — đường chạy là thật). Luồng exporter: không bắt buộc
  bởi bất biến 1 nhưng bắt buộc bởi ADR-0170 quyết định 4, chứng minh bằng
  `crates/metrics/benches/alloc.rs` hai case `metrics-idle` và `metrics-scraped`, đếm **toàn tiến
  trình** (engine + exporter + client scrape viết không cấp phát), mỗi case tự kiểm đường chạy là
  thật (scrape trả 200, thân có `fixbolt_sessions_logged_on{engine="a"} 1`, `published()` tăng).
- **2 — session thuần.** Không đụng `crates/session`.
- **3 — 59/59.** Không đụng session; `cargo test --all` vẫn chạy 59/59.
- **4 — mode.** Luồng exporter không phải luồng engine: nó ngủ giữa các `tick` và không đánh thức
  engine. Chứng minh: ba script mode chạy với `W2W_EXTRA="--metrics 127.0.0.1:19464"` **và**
  `scripts/scrape-loop.sh 127.0.0.1:19464 10 …` chạy song song; `hft` xanh, `standard` xanh, và
  mỗi script vẫn đỏ khi đổi sai mode như hôm nay. Bẫy riêng: exporter thừa hưởng affinity của
  luồng tạo ra nó — `w2w` tạo nó trước khi ghim; `GUIDE.md` ghi rõ.
- **6 — feature chặn `mod`.** Crate mới không có feature; lấy engine với
  `default-features = false`; `scripts/check-no-optional-deps.sh` thêm `fixbolt-metrics:libc`.
- **7 — không panic.** Crate thư viện mới theo lint workspace; `scripts/check-indexing-debt.sh`
  không được tăng (file mới dùng `get`); `scripts/check-no-crate-root-allow.sh` xanh.
- **8 — `unsafe`.** Không có `unsafe` trong `src/`. Bench `alloc.rs` dùng lại allocator đếm như
  các bench khác (đã có lý do an toàn ghi sẵn).
- **10 — số đo.** Plan này **không công bố số đo độ trễ nào**. Chi phí một lệnh ghi mỗi vòng ghi
  `[unmeasured]` (ADR-0170 *Consequences*). Cặp `w2w` là của hàng 2.
- **5, 9**: không đụng.

## Chia việc

| Bước | Kết quả | Người làm | File được sửa / không được sửa | Gate | Phụ thuộc |
|---|---|---|---|---|---|
| 0 | Phase 3 đóng bằng tag `v0.1.0` (ADR-0161); plan này đã duyệt (manager, 2026-09-24); ADR-0170, ADR-0171 được duyệt | manager | — | tag `v0.1.0` có trên `origin`; dòng *Start here* mới nhất của `STATUS.md` ghi phase 3 đóng | ADR-0161 |
| 1 | **Engine, test đỏ trước rồi code.** `crates/engine/tests/observe_occupancy.rs`: `a_ring_dispatch_reports_how_full_the_ring_to_the_application_is` (app không rút, `used` > 0, `capacity` = kích thước đã cấp), `an_inline_dispatch_reports_no_ring`, `the_front_door_reports_its_presession_slots` (qua `serve`, hai socket chưa gửi Logon → `used = 2`, `capacity = Limits::pending()`), `a_hand_built_engine_reports_no_presession_slots`, `latest_reads_without_asking` (`latest()` rồi nhiều `turn()` → `published()` không đổi). Đỏ = lỗi biên dịch vì API chưa có, trích nguyên văn. Rồi code mục A; thêm case `observe-asked-ring` vào `benches/alloc.rs` và vào dòng `allocations:` | senior developer (opus) | Sửa: `crates/engine/src/{observe.rs, dispatch.rs, ring.rs, lib.rs}`, `crates/engine/benches/alloc.rs`; tạo `crates/engine/tests/observe_occupancy.rs`. **Không** sửa: file test có sẵn, `crates/session/`, `crates/codec/`, `conn.rs`, `journal.rs`, `shard.rs` | `cargo test -p fixbolt-engine --test observe_occupancy`; test có sẵn **không sửa** vẫn xanh: `cargo test -p fixbolt-engine --test observe --test events --test admin --test dispatch`; `cargo test --all`; `cargo test --no-default-features`; `cargo bench -p fixbolt-engine --bench alloc` (`observe-asked-ring 0`, mọi case cũ 0); `grep -c note_unframeable` = `grep -c note_presession_slots` trong `crates/engine/src/lib.rs` (trích cả hai số); `cargo clippy --all-targets -- -D warnings`; `cargo fmt --check`; `scripts/check-indexing-debt.sh` | 0 |
| 2 | **Đảo ngược phần engine**, từng cái, viết câu FAIL trước: R1 `RingDispatch` không override `ring_to_app`; R2 bỏ lời gọi `note_presession_slots` trong `pump_loop`; R3 `latest()` bật cờ `wanted`; R4 tiêm `std::hint::black_box(Vec::<u8>::with_capacity(1))` vào `RingDispatch::ring_to_app` | senior developer (opus), cùng agent | Như bước 1; `git diff` sau bước 2 giống hệt sau bước 1 | R1 đỏ ở `a_ring_dispatch_reports…`; R2 đỏ ở `the_front_door_reports…`; R3 đỏ ở `latest_reads_without_asking`; R4 `observe-asked-ring` > 0 | 1 |
| 3 | **Crate `fixbolt-metrics`**, test trước: `tests/series_names.rs` (danh sách mục C viết tay, so với `series.rs`), `tests/exporter.rs` gồm `the_content_type_is_exactly_the_one_prometheus_3_parses`, `a_scrape_before_the_first_snapshot_says_so`, `a_scrape_storm_builds_at_most_one_snapshot_per_interval` (500 scrape trong 1 s, `published()` tăng ≤ 11), `the_snapshot_age_grows_while_the_engine_sleeps`, `healthz_follows_snapshot_healthy`, `a_request_larger_than_4_kib_is_refused_not_buffered`, `a_client_that_sends_nothing_is_dropped_after_the_read_timeout`, `without_events_the_exporter_leaves_the_stream_alone`, `with_events_every_drop_reason_today_has_its_own_label`, `a_series_with_no_source_is_omitted_not_zeroed`, `stop_joins_the_thread`; unit test trong `encode.rs`: `a_full_snapshot_fits_the_reserved_buffer` (64 phiên, giá trị dài nhất, dung lượng buffer không đổi), `label_values_are_escaped`, `skew_is_printed_as_seconds_without_a_float`. Rồi code mục B; `benches/alloc.rs` hai case; khuôn publish; `Cargo.toml` members; `check-release-versions.sh` thêm danh sách "đủ khuôn, chưa publish" kiểm luật 2 (trừ `publish`), 3, 5 cho `fixbolt-metrics`; `check-no-optional-deps.sh` thêm case | senior developer (opus) | Tạo mọi file dưới `crates/metrics/` trừ `tests/dashboard.rs` và `examples/scrape_fixture.rs`; sửa `Cargo.toml`, `scripts/check-release-versions.sh`, `scripts/check-no-optional-deps.sh`. **Không** sửa `crates/engine/`, `crates/session/`, `tools/`, `.github/` | `cargo test -p fixbolt-metrics`; `cargo bench -p fixbolt-metrics --bench alloc` (`metrics-idle 0 metrics-scraped 0`); `scripts/check-release-versions.sh`; `scripts/check-no-optional-deps.sh`; `scripts/check-every-crate-is-licensed.sh`; `cargo package -p fixbolt-metrics --list --allow-dirty` (chỉ file trong `include`); `cargo test --all`; `cargo test --no-default-features`; clippy, fmt, `scripts/check-indexing-debt.sh`, `scripts/check-no-crate-root-allow.sh` | 1 |
| 4 | **Đảo ngược phần crate**: R5 bỏ giới hạn `min_request_interval`; R6 tiêm `black_box(Vec::with_capacity(1))` vào encode; R7 `Content-Type` với dấu phẩy; R8 exporter rút event khi chưa bật `with_events`; R9 đổi tên một series trong `series.rs`; R10 bỏ dấu `=` ở pin engine; R11 lấy engine với feature mặc định | senior developer (opus), cùng agent bước 3 | Như bước 3; khôi phục hết | R5 đỏ ở `a_scrape_storm…`; R6 `metrics-scraped` > 0; R7 đỏ ở `the_content_type…`; R8 đỏ ở `without_events…`; R9 đỏ ở `series_names`; R10 `check-release-versions.sh` FAIL; R11 `check-no-optional-deps.sh` FAIL ở `fixbolt-metrics:libc` | 3 |
| 5 | **Công cụ đo và mode.** `tools/w2w --metrics <addr>`; `scripts/scrape-loop.sh`; chạy ba script mode với exporter gắn và scrape 10 Hz | senior developer (opus) — `w2w` là thước đo của vạch bỏ | Sửa `tools/w2w/src/main.rs`, `tools/w2w/Cargo.toml`; tạo `scripts/scrape-loop.sh`. **Không** sửa `crates/`, các script mode | Trên Linux: `W2W_EXTRA="--metrics 127.0.0.1:19464"` + `scripts/scrape-loop.sh 127.0.0.1:19464 10 <s>` song song với `scripts/check-standard-gives-the-core-back.sh`, `scripts/check-no-kernel-sleep.sh`, `scripts/check-no-kernel-sleep-by-ctxt.sh` — xanh, và nửa "sai mode" của mỗi script vẫn đỏ; output `w2w` có `metrics:` và `allocs 0`; `ps -L -o tid,psr,comm` cho thấy `fixbolt-metrics` không nằm trên lõi engine khi có `--engine-core` | 3 |
| 6 | **Dashboard + kiểm định dạng** (nửa tài liệu của hàng 2): `tools/grafana/fixbolt.json`, `scripts/check-grafana-dashboard.py`, `scripts/check-metrics-format.sh`, `crates/metrics/examples/scrape_fixture.rs`, `crates/metrics/tests/dashboard.rs` (`every_series_the_dashboard_queries_is_scraped` — fixture: một engine qua `serve`, một engine dựng tay với `RingDispatch`, `with_events` bật); hai bước mới trong job `gates` | developer (sonnet) | Chỉ các file nêu ở ô này và `.github/workflows/ci.yml` (job `gates`). **Không** sửa `crates/metrics/src/`, `crates/engine/` | `cargo test -p fixbolt-metrics --test dashboard`; `python3 scripts/check-grafana-dashboard.py tools/grafana/fixbolt.json`; `scripts/check-metrics-format.sh` có dòng `SUCCESS` của `promtool`; đảo ngược R12 thêm `${DS_PROMETHEUS}` vào một panel → script Python FAIL; R13 dashboard hỏi một series không tồn tại → test `dashboard` FAIL; R14 encode `TYPE` hai lần → `promtool` FAIL | 4 |
| 7 | **Chạy thật với Prometheus + Grafana** (nếu máy có container runtime): `examples/acceptor_with_metrics.rs` + một client Logon; Prometheus bản ghim scrape 1 s; Grafana bản ghim nạp `fixbolt.json` **bằng file provisioning**; mọi panel có dữ liệu trừ panel cần kiểu triển khai khác (ghi ra) | manager (lệnh cụ thể trong brief cho runner haiku) | Không sửa file nào; bằng chứng: trang *Targets* của Prometheus `up == 1`, trích `curl -s localhost:3000/api/dashboards/uid/<uid>` và `api/ds/query` của một panel | Không có container runtime → ghi vào *Chưa chứng minh*, không coi là đạt | 6 |
| 8 | **Tài liệu**, cùng commit với code: danh sách ở mục dưới; trang `docs/reference/` mới; ADR-0170/0171 → `Accepted` (manager ghi dòng trạng thái) | developer (sonnet) | Chỉ file `docs/`, `README.md`, `CHANGELOG.md`, `crates/metrics/README.md` được liệt kê. **Không** sửa `crates/*/src`, `STATUS.md` | `python3 scripts/check-links.py` xanh; `scripts/check-adr-numbers.sh` xanh | 6 |
| 9 | **Review + CI**: một senior review, context mới, được đưa plan này, ADR-0170, ADR-0171 và các gate; manager kiểm từng phát hiện theo `CLAUDE.md` §12; PR nháp từ commit đầu, CI xanh trên commit đóng, ghi run id | senior developer (opus) review; manager | — | CI xanh trên commit đóng, run id ghi vào *Nhật ký giao hàng* | 8 |

Bước 1 và 3 không chạy song song: bước 3 cần API của bước 1. Bước 5 và 6 chạy song song được (file
rời nhau). Hàng 2 (cặp `w2w` scrape bật/tắt trên máy bàn §9, hai procedure, và nếu qua
vạch thì đưa crate vào gia đình release theo tag — không publish crates.io, ADR-0161) là PR sau,
dùng `w2w --metrics` và `scrape-loop.sh` của plan này.

## Cách kiểm chứng

| # | Tiêu chí | Lệnh | Đạt khi |
|---|---|---|---|
| 1 | Đỏ trên code chưa viết | `cargo test -p fixbolt-engine --test observe_occupancy` ở bước 1; `cargo test -p fixbolt-metrics` trước code bước 3 | lỗi biên dịch nêu đúng tên API mới (`ring_to_app`, `presession_slots`, `latest`, `Exporter`), trích nguyên văn |
| 2 | Hai số mới đúng, và `None` đúng chỗ | test bước 1 | xanh; R1, R2 đỏ đúng test |
| 3 | Engine không làm thêm khi scrape dồn dập | `a_scrape_storm_builds_at_most_one_snapshot_per_interval` | `published()` tăng ≤ 11 trong 1 s dù 500 scrape; R5 đỏ |
| 4 | Không cấp phát — engine | `cargo bench -p fixbolt-engine --bench alloc` | `observe-asked-ring 0`; R4 > 0 |
| 5 | Không cấp phát — cả tiến trình khi scrape | `cargo bench -p fixbolt-metrics --bench alloc` | `metrics-idle 0 metrics-scraped 0`; R6 > 0 |
| 6 | Prometheus đọc được | `scripts/check-metrics-format.sh`; test `Content-Type` | `promtool` không báo gì; header đúng từng byte; R7, R14 đỏ |
| 7 | Dashboard khớp exporter | `cargo test -p fixbolt-metrics --test dashboard`; `scripts/check-grafana-dashboard.py` | xanh; R12, R13 đỏ |
| 8 | Tên series được giữ | `cargo test -p fixbolt-metrics --test series_names` | xanh; R9 đỏ với thông báo nhắc `CHANGELOG.md` |
| 9 | Mode không bị phá | ba script mode với exporter + scrape 10 Hz (bước 5) | cả hai mode xanh; nửa sai mode vẫn đỏ |
| 10 | Khuôn publish | `scripts/check-release-versions.sh`, `scripts/check-no-optional-deps.sh`, `cargo package -p fixbolt-metrics --list --allow-dirty` | xanh; R10, R11 đỏ |
| 11 | Chạy với Prometheus/Grafana thật | bước 7 | `up == 1`, panel có dữ liệu — hoặc ghi rõ là chưa chứng minh |
| 12 | Không phá gì đã có | các lệnh ở bước 1, 3 | test có sẵn xanh **không sửa**; 59/59 nằm trong `cargo test --all`; job `semver` chỉ báo thay đổi thêm |

"Test pass" chưa đủ ở đây theo nghĩa của ADR-0098: plan này xong **không** có nghĩa exporter được
giữ. Nó được giữ khi hàng 2 đo cặp `w2w` trên máy bàn §9 và cặp đó nằm trong band.

## Tài liệu phải cập nhật

Theo bảng `CLAUDE.md` §4, đi từng dòng:

- [ ] `DESIGN.md` §3 — dòng crate `metrics`; dòng `observe` trong *What `engine` contains*
      (`Occupancy`, `latest`, hai số mới); D4 — phương thức `Dispatch::ring_to_app`; §6
      *Allocation* — case `observe-asked-ring` và hai case của `fixbolt-metrics`
- [ ] `README.md` layout; `Cargo.toml` members (bước 3)
- [ ] `docs/internals/metrics.md` (mới) — file nào giữ gì, đọc theo thứ tự nào, test nào canh;
      `docs/internals/engine.md` — dòng `observe`; `docs/internals/tools.md` — `w2w --metrics`,
      `tools/grafana/`
- [ ] `CHANGELOG.md` — API thêm của `fixbolt-engine`; crate mới (chưa vào gia đình release); **danh sách tên
      series là API công khai**
- [ ] `docs/GUIDE.md` §8a — gắn exporter; bẫy affinity (tạo exporter trước khi ghim luồng
      engine); bật event là lấy event của người khác; bind loopback hoặc mạng riêng, không TLS,
      không xác thực; tuổi snapshot ở `standard`
- [ ] `docs/CONFIGURATION.md` §2 — `min_request_interval`, `fresh_wait`, `tick`,
      `read_timeout` và mặc định của chúng
- [ ] `docs/best-practices-standard.md`, `docs/best-practices-hft.md` — đặt luồng exporter ở đâu,
      mỗi mode một câu
- [ ] `PRD.md` §3 — bỏ *"Still missing: ring depth and pending-set occupancy"*; dòng exporter
      ghi *built, not yet kept* cho tới hàng 2
- [ ] `docs/reference/` — trang mới về ba bẫy đã trả giá khi đọc: `request()` trả bản cũ;
      `Content-Type` của Prometheus 3; `__inputs` của Grafana hỏng khi provisioning
- [ ] `docs/decisions/` — ADR-0170, ADR-0171
- [ ] `STATUS.md` — manager viết khi đóng
- Không đổi: `SESSION-BEHAVIOUR.md`, `CONFORMANCE.md`, `hft-playbook.md`, `DESIGN.md` §8/§9

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Scrape thấy snapshot cũ một chu kỳ | `the_snapshot_age_grows_while_the_engine_sleeps`; thiết kế `request` → chờ → `latest` |
| Scrape dồn dập bắt engine dựng snapshot liên tục | `a_scrape_storm_builds_at_most_one_snapshot_per_interval`; R5 |
| Đọc `latest()` lại vô tình bật cờ, engine làm thêm | `latest_reads_without_asking`; R3 |
| Exporter rút mất event của ứng dụng | `without_events_the_exporter_leaves_the_stream_alone`; R8 |
| Xuất số 0 cho thứ không ai báo → trông như "rỗng, khoẻ" | `a_series_with_no_source_is_omitted_not_zeroed`; `an_inline_dispatch_reports_no_ring`; `a_hand_built_engine_reports_no_presession_slots` |
| Quên gọi `note_presession_slots` ở một cửa `serve*` | so số lần `grep -c` hai hàm ở bước 1; `the_front_door_reports…`; R2 |
| Luồng exporter cấp phát → `w2w` (đếm mọi luồng) đỏ vì lý do không phải engine | `metrics-scraped` đếm toàn tiến trình; `a_full_snapshot_fits_the_reserved_buffer`; R6 |
| Buffer trả lời không đủ khi 64 phiên → `Vec` phình → cấp phát | `a_full_snapshot_fits_the_reserved_buffer` |
| `Content-Type` sai một ký tự → Prometheus 3 bỏ scrape | `the_content_type_is_exactly…`; R7 |
| Encode sai định dạng mà test tự viết không thấy | `promtool check metrics` trong CI; R14 |
| Đổi tên series làm hỏng dashboard người dùng mà không báo | `series_names`; `dashboard`; R9, R13 |
| Dashboard dùng `${DS_PROMETHEUS}` → hỏng khi provisioning | `check-grafana-dashboard.py`; R12 |
| Client chậm hoặc request khổng lồ giữ luồng exporter | `a_client_that_sends_nothing_is_dropped…`; `a_request_larger_than_4_kib…` |
| Exporter thừa hưởng lõi đã ghim của luồng engine | `w2w` tạo exporter trước khi ghim, kiểm bằng `ps -L` ở bước 5; `GUIDE.md` |
| Exporter spin khi rảnh, làm hỏng script `standard` | script `standard` chạy với exporter + scrape ở bước 5 |
| Biến thể `DropReason` mới thành `reason="other"` mà không ai để ý | `with_events_every_drop_reason_today_has_its_own_label` liệt kê tay các biến thể hôm nay; ghi trong `docs/internals/metrics.md` |
| Crate mới lọt vào bản release theo tag trước khi có phán quyết | `publish = false` + `check-release-versions.sh` luật 4 |
| Kéo `libc` vào build `hft` của người dùng qua crate mới | `check-no-optional-deps.sh` case `fixbolt-metrics:libc`; R11 |
| Case alloc xanh vì không chạy gì | mỗi case tự kiểm: scrape trả 200, thân có đúng giá trị, `published()` tăng |
| `promtool` vắng trên máy → tưởng là đạt | script in `SKIPPED, NOT PASSED` và thoát khác 0 |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Một hàm `std` nào đó cấp phát trên đường scrape (accept, timeout) làm `metrics-scraped` > 0 | Trung bình | dừng, báo manager; plan sai thì sửa plan (phương án dự phòng: `w2w` đếm theo luồng) — không tự đổi thiết kế |
| Một lệnh ghi mỗi vòng phục vụ làm chậm đường nóng | Thấp | `[unmeasured]`; lần `--strict` kế trên máy §9 đọc `turn.rs`, nêu đây là nghi phạm nếu số dịch |
| Tên phương thức `ring_to_app` trùng với phương thức người dùng tự thêm vào `Dispatch` của họ | Thấp | tên cụ thể; ghi trong `CHANGELOG.md` |
| Tải Prometheus trong CI hỏng (mạng, bản bị gỡ) | Thấp | ghim version + SHA-256; job đỏ rõ lý do |
| Series theo `conn` sinh series mới mỗi lần kết nối lại | Thấp | chấp nhận, ghi ở ADR-0171; trần 64 phiên sống |
| Test socket chập chờn trên CI | Trung bình | chờ có hạn theo đồng hồ, không `sleep` cố định; cổng `127.0.0.1:0` |
| Hàng 2 đo trượt vạch | Trung bình | crate đã `publish = false`; thiết kế lại theo ADR-0098, không ship |

## Ngoài phạm vi

- Cặp `w2w` scrape bật/tắt trên máy bàn và việc đưa crate vào gia đình release theo tag —
  **hàng 2**. Publish lên crates.io: không có trong kế hoạch (ADR-0161).
- Quan sát runtime chia shard (`serve_sharded_hft` không có `Handles`) — việc riêng, cần plan.
- Tên đối tác (SenderCompID/TargetCompID) làm label — `SessionSnapshot` chưa mang; cần plan riêng.
- Độ đầy ring chiều ứng dụng → engine.
- Re-export từ crate `fixbolt` (facade) — thêm sau được mà không phá gì.
- OpenMetrics, protobuf, nén, keep-alive, TLS, xác thực cho `/metrics`.
- Web UI riêng của dự án (vẫn là non-goal, ADR-0098).
- Đánh thức engine `standard` để có snapshot mới hơn.

## Anh cần quyết

Không có câu hỏi mới. Mọi quyết định ở đây nằm trong khung ADR-0098 anh đã duyệt; hai ADR mới chỉ
chốt những chỗ khung đó để ngỏ. Nếu anh muốn đổi, ba chỗ đáng xem nhất: bảng tên series ở mục C
(đây là API công khai), việc crate đúng khuôn gia đình lockstep nhưng chỉ vào gia đình release theo tag ở hàng 2,
và việc exporter mặc định **không** đọc event.

## Nhật ký giao hàng

(trống — điền khi đóng từng bước)
