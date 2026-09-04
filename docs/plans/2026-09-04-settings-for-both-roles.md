# File cấu hình cho cả hai vai, và những knob một FIX desk mong có sẵn

> **Loại:** Plan · **Ngày:** 2026-09-04 · **Trạng thái:** Draft
> **Phạm vi:** `STATUS.md` item 45, đợt B, plan thứ nhất. Chạm `engine` (`settings`, `presession`,
> `reconnect`, entry point), `session` (`Config`, vài rule reset), `library` (`Reply`), docs.
> **Không chạm** `codec`, `dict`, `transport`.
>
> **Draft viết 2026-09-04, trước khi đợt A chạy.** Khi đến lượt: đọc lại **toàn bộ** mục *Những
> gì đã biết chắc* theo code lúc đó (đợt A đổi `journal.rs`, `observe`, `settings`), sửa plan, rồi
> mới đổi sang *Chờ duyệt*. Số dòng trích dẫn ở đây là của `main` ngày 2026-09-04.
>
> **Máy chạy:** macOS đủ. **Thời lượng dự kiến:** 2–3 ngày.

## Bối cảnh

Người dùng đầu tiên của fixbolt gần như chắc chắn đã dùng QuickFIX. Họ mở file cấu hình và
tìm mười thứ; hôm nay tìm thấy mười key, **tất cả cho acceptor**. Không có cách nào khai một
initiator từ file (host, port, reconnect), không có `ResetOnLogon`, không có `LogonTimeout`, và
không có chỗ nào để nói "đối tác này gửi field ngoài dictionary, cho qua". Mỗi thứ đó hôm nay
là một dòng Rust, và ADR-0040 đã quyết định file là cách người vận hành nói với engine.

Kèm theo ba việc nhỏ cùng vùng: registry chỉ nhìn thấy `Identity` (49/56/50/57), không thấy
`553=`/`554=`/`96=` nên không kiểm được credential dù ADR-0026 nói `lookup` là auth hook; không
có `MaxMessageSize` (một frame dài hơn `RX` là gì? — phải đọc `frame.rs`); và `library` không có
cách nói `35=j` BusinessMessageReject mà không tự tay xếp field.

## Những gì đã biết chắc (2026-09-04 — xác minh lại khi làm)

| Sự thật | Nguồn |
|---|---|
| Mười key hiện có, `[DEFAULT]`/`[SESSION]`, key lạ là lỗi có số dòng, không `[SESSION]` là lỗi | `crates/engine/src/settings.rs`, ADR-0040, `docs/CONFIGURATION.md` §1 |
| `Config` của session: `begin_string`, `sender_comp_id`, `target_comp_id`, `max_skew_ms`, `heart_bt_int`, `schedule` — **không có** flag reset nào | `crates/session/src/lib.rs:271–297` |
| Initiator: `connect_and_serve(addr, cfg, app, Policy, recovery)`; `Policy { first_ms, ceiling_ms, schedule, .. }` không jitter | `crates/engine/src/lib.rs:1088`, `reconnect.rs:47`, ADR-0043 |
| `Session::new` reset về 1 mỗi lần `connect`; `Session::resume` giữ số; `141=Y` inbound reset cả hai chiều trước khi judge | ADR-0010, `lib.rs:1878` |
| Registry: `lookup(Identity<'_>) -> Option<&Entry>`; `Entry { cfg }`; `identity_of` đọc `49/56/35` (+`50/57`) bằng quét byte, không parse | `presession.rs:162, 213`, ADR-0020, ADR-0026 quyết định 3 |
| `Limits::new(pending, logon_ms)` là deadline tới `Logon` ở pre-session — tức đã có `LogonTimeout` cho **acceptor**, chưa có cho initiator; `Shutdown` có deadline của caller — chưa có `LogoutTimeout` theo phiên | ADR-0020, ADR-0038 |
| `Validation` của codec chỉ có `body_length`, `check_sum`; dictionary pass (required, type, enum, unknown tag, group) chạy trong session mỗi message, **chưa có knob và chưa được đo** (item 39) | `crates/codec/src/parse.rs:86`, `STATUS.md` item 39 |
| `Reply::message(msg_type)` → `TemplateBuilder`, `field(tag, value)`, `send()`; chi phí 766 ns (item 34) | `crates/library/src/reply.rs:169–244` |
| QuickFIX đặt tên: `ConnectionType`, `SocketConnectHost/Port`, `ReconnectInterval`, `ResetOnLogon/Logout/Disconnect`, `RefreshOnLogon`, `LogonTimeout`, `LogoutTimeout`, `ValidateFieldsOutOfOrder`, `ValidateUserDefinedFields`, `AllowUnknownMsgFields`, `MaxMessageSize` | `docs/reference/prior-art.md` mục 2026-09-03 |

## Cách làm — hình dạng dự kiến

**Key mới trong file** (tên của QuickFIX khi nghĩa giống hệt; khác nghĩa thì đặt tên khác và
nói rõ trong `CONFIGURATION.md`):

| Key | Vào đâu | Ghi chú |
|---|---|---|
| `ConnectionType=acceptor\|initiator` | `Settings` → chọn entry point | mặc định `acceptor`; một file **không** trộn hai vai (một engine, một vai — ADR-0004 là một core, không phải một process) |
| `SocketConnectHost`, `SocketConnectPort` | `connect_and_serve` | bắt buộc khi initiator; lỗi nếu có mà là acceptor |
| `ReconnectInterval` (giây), `ReconnectCeiling` (giây, **không có ở QuickFIX**) | `Policy::new(first, ceiling)` | ceiling mặc định = 16 × first |
| `ResetOnLogon`, `ResetOnLogout`, `ResetOnDisconnect` = `Y\|N` | `Config` mới: `reset: ResetPolicy` | **không dùng `Session::new`/`resume` để biểu diễn** — đó là chuyện journal có gì; đây là chuyện session *muốn* gì. Initiator `ResetOnLogon=Y` gửi `141=Y`; acceptor `ResetOnLogon=Y` chấp nhận `141=Y` và tự reset; `Logout`/`Disconnect` reset ở `end`/`disconnect` |
| `LogonTimeout` (giây) | initiator: từ `connect` tới `Logon` về; acceptor: đã có `Limits.logon_ms` → key này **ghi đè** giá trị đó | `Session` thuần: deadline đo bằng `tick` |
| `LogoutTimeout` (giây) | `begin_logout` → nếu không có `Logout` về trong ngần ấy → `disconnect_with(DropReason::LogoutTimedOut)` | `DropReason` thêm biến thể, ADR-0035 kiểu không trường |
| `AllowUnknownMsgFields=Y\|N`, `ValidateUserDefinedFields=Y\|N` | `Config::validation: DictionaryChecks` | knob cho dictionary pass; **`ValidateFieldsOutOfOrder` không có ý nghĩa** ở đây vì index phẳng không có khái niệm thứ tự header/body (D2) — nêu rõ trong `CONFIGURATION.md` là *không hỗ trợ, vì sao* |
| `MaxMessageSize` | `Limits` / `Framer` | phải đọc `frame.rs` trước: hôm nay frame dài hơn `RX` là gì? Nếu là drop im lặng → item riêng |
| `FileLogPath` | đã có sau plan message-log | — |

**Credential hook:** `Registry::lookup(id: Identity, logon: MessageView<'_, N>)` — hay giữ chữ
ký cũ và thêm `fn admit(&self, id, logon) -> Option<&Entry>` với default gọi `lookup`? Quyết
định khi làm; ràng buộc: `identity_of` vẫn quét byte, `Table` mặc định **bỏ qua** credential
(không có mặc định nào là "chấp nhận mật khẩu rỗng"), và một implementation tự viết là cách
duy nhất để kiểm 553/554 — engine **không** lưu mật khẩu.

**`Reply::business_reject(ref_seq, ref_msg_type, reason, text)`** trong `library`: viết
`35=j` với `45=`, `372=`, `380=`, `58=` qua cùng `TemplateBuilder`; không đụng session.

## Bất biến bị đụng tới

| Điều | Ảnh hưởng | Giữ bằng cách nào |
|---|---|---|
| 2 — session thuần | `ResetPolicy`, `LogoutTimeout` là trạng thái + so sánh với `tick` | không clock, không alloc; `benches/alloc.rs` session đường `logon_out`, `clock` giữ 0 |
| 3 — 59 định nghĩa | rule reset mới có thể đổi cách trả `141=` | 59 / 59 **và** `SessionReset.def` được đọc lại bằng tay; mirror 10 / 50 |
| 1 — không cấp phát | `Settings` parse ở startup — được phép; `Reply::business_reject` trên đường trả lời | alloc case `library` thêm `reject` |
| 5 — thứ tự field từ bảng | `35=j` | qua `TemplateBuilder::build::<Fix44>()` như mọi reply |
| 6 — feature gate | `connect_and_serve` sau `standard` như hôm nay | `#[cfg]` trên item |

## Chia việc (dự kiến, xếp lại khi làm)

| Bước | Kết quả |
|---|---|
| 1 | `ResetPolicy` trong `session` + ba rule, test đỏ trước trong `crates/session/tests/logon.rs`; 59/59 |
| 2 | `LogonTimeout` (initiator) và `LogoutTimeout` trong `session`; `DropReason::LogoutTimedOut`; test bằng `tick` |
| 3 | `DictionaryChecks` trong `Config`; `AllowUnknownMsgFields`, `ValidateUserDefinedFields`; test với một tag 5000+ và một tag không có trong FIX44 |
| 4 | `Settings`: `ConnectionType`, host/port, reconnect, ba reset, hai timeout, hai knob validation; `Settings::into_initiator()` → `(Config, addr, Policy)`; mọi key sai vai là lỗi có số dòng; **30 test hiện có phải xanh nguyên** |
| 5 | Credential hook trên `Registry`; `Table` không đổi hành vi; một test với registry tự viết từ chối `554=` sai |
| 6 | `MaxMessageSize` sau khi đọc `frame.rs`; `Reply::business_reject` |
| 7 | Docs: `CONFIGURATION.md` (mọi key mới, mỗi hàng `file:line`), `GUIDE.md` §1a0/§8c, `SESSION-BEHAVIOUR.md` §1/§4 (reset, timeout — **nêu test**), `CHANGELOG.md`, `STATUS.md` |

## Cách kiểm chứng (dự kiến)

`cargo test -p fixbolt-session --test score/mirror/logon`, `-p fixbolt-engine --test settings
--test settings_wire --test reconnect --test reconnect_wire`, `scripts/interop.sh` **cả hai
chiều** với `ResetOnLogon=Y` ở cả hai đầu và một lần với `N` — chiều nào không đổi kết quả thì
knob đó chưa được kiểm; `scripts/bench.sh` invariant.

Reversal tối thiểu: bỏ `ResetOnLogout` → test reset đỏ; `LogoutTimeout` không đếm → test treo
theo deadline của harness (bài học [a-reversal-can-fail-by-hanging](../reference/a-reversal-can-fail-by-hanging.md)) — test phải có deadline riêng.

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| `ResetOnLogon=Y` ở acceptor + `Session::resume` từ journal: số nào thắng? | test: resume ở 500, đối tác `141=Y` → cả hai về 1, journal `highest` không đổi (không xoá) |
| `ReconnectInterval` bằng giây ở QuickFIX, ms trong `Policy` | parse nhân 1000; test `ReconnectInterval=2` → `first_ms == 2000` |
| Một file khai initiator nhưng gọi `serve` | `Settings::into_table()` từ chối khi `ConnectionType=initiator` với thông báo chỉ sang `into_initiator()` |
| `AllowUnknownMsgFields=Y` làm corpus `14a_BadField.def` đổi kết quả | knob mặc định `N`; corpus chạy với mặc định; test riêng bật `Y` |

## Ngoài phạm vi

`RefreshOnLogon`, `SendRedundantResendRequests`, store DB, `DefaultApplVerID` (phase 2),
`SessionQualifier` mới (đã có), nhiều vai trong một file.

## Nhật ký giao hàng

*(draft — chưa duyệt, chưa bắt đầu)*
