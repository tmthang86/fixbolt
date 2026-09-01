# Đọc `Logon` trước, rồi mới giao socket cho shard

> **Loại:** Plan · **Ngày:** 2026-08-31 · **Trạng thái:** **Xong 2026-09-01** — cả sáu bước
> **Phạm vi:** `engine` — một tầng mới giữa acceptor và shard. Không đụng `codec`, `session`.
>
> **Chủ dự án chọn cách A ngày 2026-08-31**, sau khi được nêu hai đường: tầng pre-session, hay
> một sổ đăng ký dùng chung mà `Engine` tra. Lý do chọn: *"theo cách các engine thật làm"*.

## Bối cảnh

`STATUS.md` open item 24. `[đo 2026-08-31]` chạy 59 định nghĩa qua `fixbolt_engine::shard`:
**59 với một shard, 57 với hai**, ở cả hai settle bound nên không phải timing. Hỏng đúng
`1b_DuplicateIdentity.def` và `AlreadyLoggedOn.def`.

**Luật vốn đúng; shard làm sai tiền đề của nó.** Một `Engine` mang đúng một `Config`, tức phục
vụ đúng một identity FIX, nên nó trả lời được *"identity này đã logon chưa"* bằng cách đếm những
kết nối **nó đang giữ** mà đang logon (`crates/engine/src/lib.rs`, `others_on`). Chia các kết
nối đó ra nhiều engine thì không còn gì để đếm, và cả hai `Logon` đều được nhận.

**`Assign` không sửa được, và không phải vì nó thiếu tính năng.** Nó bị hỏi lúc `accept`, khi
`Logon` — thứ duy nhất nói identity là gì — chưa tới. Không có gì ở thời điểm đó biết socket này
thuộc về ai.

Nên câu hỏi không phải *"gán thông minh hơn thế nào"* mà *"ai giữ socket trong lúc chưa biết nó
là ai"*. Đó là một tầng, và các engine FIX thật đều có nó.

## Những gì đã biết chắc

| Sự thật | Nguồn |
|---|---|
| 59 với một shard, **57 với hai**, hỏng đúng hai file | `[đo 2026-08-31]` `crates/engine/tests/shard_wire.rs` |
| Luật nằm ở `Engine::turn`, biến `others_on`, hỏi **trước** khi session xử lý message | đọc `crates/engine/src/lib.rs` |
| `Connection::turn` nhận một closure `refuse` được hỏi cho **từng message trọn vẹn**, trả `true` là ngắt kết nối không trả lời | đọc `crates/engine/src/conn.rs:181` |
| `1b_DuplicateIdentity.def` và `AlreadyLoggedOn.def` đều chờ **không có hồi đáp nào** trên kết nối thứ hai | đọc corpus |
| `Config` là `Copy`, mang `begin_string`, `sender`, `target` | `crates/session/src/lib.rs:252` |
| `Shards::hand(transport)` là điểm duy nhất socket đi vào một shard | đọc `crates/engine/src/shard.rs` |
| `msg_type_is_logon` đọc thẳng byte, không cần từ điển | `crates/engine/src/lib.rs` |
| `Acceptor::accept_blocking` đã có, và luồng acceptor **được phép chặn** | `[2026-08-31]` bước 4 của plan trước |
| `mpsc::try_recv` không tốn syscall nào (2 triệu lần), không cấp phát | `[đo 2026-08-31]` `reference/measured-costs.md`, `benches/alloc.rs` |

## Cách làm

**Một tầng, ba trách nhiệm, và trách nhiệm thứ ba là thứ dễ quên nhất.**

**1. Giữ socket cho tới khi `Logon` tới.** Một `Pending` sở hữu `TcpTransport` và một buffer nhỏ,
đọc không chặn, và **không dựng session nào**. Nó chỉ tìm một message trọn vẹn đầu tiên.

**2. Đọc identity ra khỏi message đó, không phải parse cả message.** `49=` (SenderCompID) và
`56=` (TargetCompID) đọc thẳng khỏi byte, giống `msg_type_is_logon` đã làm. **Không** kéo từ điển
vào tầng này.

**3. Vứt socket đi khi nó không chịu nói mình là ai.** Đây là trách nhiệm hay bị quên và là chỗ
một acceptor công khai bị đánh gục: một client mở kết nối rồi im lặng chiếm một chỗ mãi mãi.
Nên tầng này có **hai giới hạn cứng**, cả hai do người gọi nêu:

| Giới hạn | Vì sao |
|---|---|
| **Thời gian tới `Logon`** | Kết nối im lặng bị đóng. FIX gọi cái này là logon timeout; không có nó thì đây là một lỗ DoS |
| **Số `Pending` cùng lúc** | Bảng có trần. Đầy thì kết nối mới bị từ chối **ngay**, không phải xếp hàng |

Message đầu tiên **không phải** `35=A` cũng là vứt, không trả lời — cùng hình dạng với cái mà
`Connection::turn`'s `refuse` đang làm.

**4. Định tuyến bằng identity, và người gọi vẫn là người quyết.** `Assign` được thay bằng một
trait nhận identity:

```rust
pub struct Identity<'a> { pub sender: &'a [u8], pub target: &'a [u8] }

pub trait Route: Send {
    /// Shard nào cho identity này. Ngoài khoảng `0..shards` là bị từ chối, không lấy dư.
    fn shard_for(&mut self, id: Identity<'_>, shards: usize) -> usize;
}
```

Mặc định: **băm ổn định trên `(sender, target)`**, không phải round-robin. Đó là điều làm luật
single-logon đúng trở lại: **cùng một identity luôn về cùng một shard**, kể cả sau khi kết nối
lại. HFT thật chia theo đối tác; băm là mặc định hợp lý, không phải câu trả lời cuối cùng.

**5. `RoundRobin` bị bỏ, không phải giữ lại cho tương thích.** Nó là chính sách tạo ra khiếm
khuyết. Giữ nó lại sau khi biết điều đó là để sẵn một cái bẫy có tài liệu.

**File sẽ tạo hoặc sửa:**

- `crates/engine/src/presession.rs` — mới: `Pending`, `PendingSet`, `Identity`, đọc `49`/`56`
- `crates/engine/src/shard.rs` — `Route` thay `Assign`; `Shards::hand` nhận identity;
  `serve_sharded_hft` chạy tầng mới trên luồng acceptor
- `crates/engine/tests/presession.rs` — mới
- `crates/engine/tests/shard.rs`, `shard_wire.rs` — theo API mới
- `docs/decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md` — bước 1
- `docs/DESIGN.md` §3 và D8 · `docs/GUIDE.md` §1a · `CHANGELOG.md` · `STATUS.md` item 24

## Bất biến bị đụng tới

| # | Đụng thế nào | Giữ bằng cách nào |
|---|---|---|
| **1** — không cấp phát trên hot path | `PendingSet` có bảng và buffer | Cấp phát **một lần lúc khởi động**, trần cố định do người gọi nêu. `benches/alloc.rs` thêm case: một lượt quét `PendingSet` rỗng phải ra **0** |
| **2** — session layer thuần | Tầng này **không** dựng session, không gọi vào `session` | Nó chỉ đọc byte và định tuyến. Nếu nó phải hỏi `session` bất cứ điều gì thì thiết kế sai |
| **3** — 59 định nghĩa là cổng | Đây chính là thứ đang hỏng | `shard_wire.rs` phải lên **59 với hai shard**, và test đặc tả khiếm khuyết hiện tại **phải đỏ** rồi bị viết lại — đó là mục đích nó tồn tại |
| **4** — luồng engine không ngủ | Tầng này chạy trên **luồng acceptor**, luồng được phép chặn | `check-no-kernel-sleep.sh` quy theo tid; chạy lại và **đọc output** |
| **7** — không `panic`/`unwrap` | Code mới đọc byte từ mạng | Mọi đường trả `Result`/`Option`. Lint workspace canh |

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | ✅ [ADR-0020](../decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md) — tầng pre-session: ai sở hữu socket trước `Logon`, hai giới hạn cứng, `Route` thay `Assign`, băm ổn định là mặc định, `RoundRobin` bị bỏ | — |
| 2 | ✅ `presession.rs`: đọc `49`/`56` khỏi message trọn vẹn đầu tiên. Test: message đủ, message thiếu, message không phải `35=A`, message rác | 1 |
| 3 | ✅ `PendingSet`: trần số lượng, logon timeout, vứt kết nối im lặng. Mỗi giới hạn một ca hỏng, so **biến thể lỗi** chứ không `is_err()` | 2 |
| 4 | ✅ `Route` + băm ổn định; `Shards::hand` nhận identity; `serve_sharded_hft` nối tầng mới | 3 |
| 5 | ✅ **`shard_wire.rs` lên 59 với hai shard**, và test đặc tả cũ được viết lại | 4 |
| 6 | ✅ Đo: `Logon` mất thêm bao lâu vì đi qua tầng này, kèm `N` và khối machine của `check-machine.sh` | 5, máy §9 |

## Cách kiểm chứng

- **Bước 2 — đọc identity đúng, và sai thì nói sai.** Test trên byte thật lấy từ corpus, không
  phải message tự bịa. **Đảo ngược:** bẻ chỉ số trường, test phải đỏ.
- **Bước 3 — từng giới hạn có một ca hỏng.** Kết nối im lặng quá hạn **bị đóng**, chứng minh
  bằng cách đọc phía client thấy EOF; bảng đầy thì kết nối thứ `n+1` bị từ chối ngay. **Đảo
  ngược:** bỏ timeout, ca đó phải đỏ.
- **Bước 5 — cổng thật.** `shard_wire.rs` **59/59 với hai shard**. Và
  `two_shards_break_the_single_logon_rule_and_this_records_it` **phải đỏ trước khi bị xoá** —
  nếu nó vẫn xanh thì khiếm khuyết chưa được sửa, chỉ bị đi vòng.
- **Bước 5 — hai cổng 59 cũ vẫn xanh, không sửa fixture nào.** `-p fixbolt-session --test score`
  và `-p fixbolt-engine --test wire`.
- **Bước 5 — `benches/alloc.rs` vẫn toàn 0**, thêm case quét `PendingSet` rỗng.
- **Bước 5 — `check-no-kernel-sleep.sh` xanh**, nửa đỏ của nó vẫn phải trượt.
- **Bước 6 — số kèm `N`** và khối machine, theo ADR-0012 quyết định 4.

## Tài liệu phải cập nhật

- [x] [ADR-0020](../decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md) — bước 1, **Accepted 2026-09-01**
- [ ] `docs/DESIGN.md` §3 (mod mới), D8 (luật single-logon ở đâu)
- [ ] `docs/GUIDE.md` §1a — *"shard nào là việc của anh"* thành *"identity quyết định, đây là
      cách thay chính sách"*, cộng hai giới hạn cứng mà người gọi phải nêu
- [ ] `CHANGELOG.md` — `Assign` bị bỏ, `Route` thay thế: **đây là breaking change**
- [ ] `STATUS.md` — item 24 đóng, item 21 cập nhật

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Đọc `49`/`56` khi message chưa trọn vẹn | Test cắt message ở mọi vị trí; phải trả "chưa đủ", không phải một identity sai |
| Băm không ổn định giữa các lần chạy | Tự viết hàm băm, không dùng `DefaultHasher` (nó có seed ngẫu nhiên mỗi tiến trình). Test khẳng định một identity cụ thể ra một shard cụ thể |
| Kết nối im lặng chiếm chỗ | Bước 3, và một ca hỏng riêng |
| Tầng này lặng lẽ thành một session layer thứ hai | Bất biến 2: nó không được `use fixbolt_session::` gì ngoài `Config`. Kiểm bằng `grep` khi đóng plan |
| Sửa khiếm khuyết bằng cách đổi test | Bước 5: test đặc tả **phải đỏ trước** |
| `Logon` đầu tiên bị đọc mất, session không thấy nó | Đây là bẫy nguy hiểm nhất: tầng này đọc byte rồi phải **giao lại nguyên vẹn** cho `Engine::add`. Test: shard nhận đúng số byte đó và session trả lời `Logon` bình thường |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Tầng mới đọc mất `Logon` | **Cao** | `Engine::add` cần một đường nhận "socket + byte đã đọc sẵn". Đó là thay đổi API và nằm trong ADR-0020 quyết định 3: `Engine::add_with_prefix`, dựng trên `Framer::spare()`/`filled()` đã có sẵn, và **từ chối** prefix dài hơn `RX` chứ không cắt bớt |
| `Route` là breaking change | Trung bình | Không có người dùng ngoài; `Assign` mới ra đời cùng ngày. Ghi ở `CHANGELOG.md` |
| Logon timeout thành một cái đồng hồ nữa | Trung bình | Dùng chính `Clock` của engine, không dựng nguồn thời gian thứ hai |
| Hai giới hạn cứng có mặc định "hợp lý" rồi không ai nêu | Trung bình | **Không có mặc định.** Người gọi phải nêu, như `ShardPlan` bắt nêu core |

## Ngoài phạm vi

- **Không** đụng luật single-logon bên trong `Engine`. Nó đúng; cái sai là chỗ kết nối được đưa
  tới. `others_on` giữ nguyên.
- **Không** làm TLS. Bắt tay TLS cũng cần một tầng giữ socket trước session, và hai thứ đó sẽ
  gặp nhau — nhưng ADR-0018 nói TLS chưa có plan, và gộp hai việc chưa có plan là cách chắc chắn
  nhất để hỏng cả hai.
- **Không** cân bằng lại shard lúc chạy.
- **Không** định tuyến theo gì khác ngoài `(sender, target)`. SNI, cổng, subnet là việc khác.

## Nhật ký giao hàng

*(duyệt 2026-08-31 theo uỷ quyền thường trực; chủ dự án chọn cách A cùng ngày.)*

**2026-09-01 — bước 1 xong.**
[ADR-0020](../decisions/ADR-0020-a-pre-session-stage-owns-the-socket-until-logon.md),
`Accepted`, mười quyết định. Ba trong số đó không có trong plan và đến từ việc đọc code
trước khi viết:

- **Quyết định 3 có đường giải rồi, không cần API mới ở tầng dưới.** `Framer` đã có
  `spare()` và `filled(n)` — đúng nghĩa *"đây là byte đã tới trước khi anh nhìn"*. Nên
  `Engine::add_with_prefix` chỉ là một lớp mỏng, và nó **từ chối** prefix dài hơn `RX` chứ
  không cắt: cắt thì session nhận nửa message và framer sẽ báo `Garbage` về những byte vốn
  lành lặn — một khiếm khuyết mà bằng chứng đã bị chính đoạn code gây ra nó xoá mất.
- **Quyết định 5, và nó là hệ quả plan chưa nói ra.** Hai giới hạn cứng ở bước 3 vô nghĩa
  nếu luồng acceptor đỗ trong `accept_blocking`: một luồng đang đỗ ở đó **không hết hạn
  được** kết nối im lặng, nên logon timeout chỉ nổ khi tình cờ có người khác kết nối. Đó
  đúng là kiểu hành vi-phụ-thuộc-tải mà `CLAUDE.md` §10 gọi tên. `Poller::wait` có timeout
  và đã tồn tại; `serve_sharded_hft` thôi gọi `accept_blocking`.
- **Quyết định 7 nêu rõ vì sao `DefaultHasher` bị cấm** ở đây: nó có seed theo tiến trình,
  nên hai lần chạy cùng một binary sẽ định tuyến cùng một đối tác đi hai nơi — luật
  single-logon đúng trong một lần chạy và sai sau khi restart. Test khẳng định **một
  identity cụ thể ra một shard cụ thể**, không phải chỉ "ổn định trong tiến trình này".

**2026-09-01 — bước 2 xong.** `crates/engine/src/presession.rs` (`Identity`, `identity_of`,
`is_logon`) và `crates/engine/tests/presession.rs`, 8 test. Test viết trước và **đỏ đúng chỗ**:
`unresolved import fixbolt_engine::presession`. `Engine` bỏ hàm `msg_type_is_logon` riêng và
gọi sang đây, nên luật `35=A` chỉ còn một chỗ. Không nhân đôi luật khung: `Framer` vẫn là nơi
duy nhất cắt stream.

**Corpus bác bỏ giả định của chính test, và đó là điều tốt.** Bản đầu khẳng định mọi `Logon`
trong corpus là `49=TW44`/`56=ISLD`; nó đỏ trên byte thật, vì corpus **cố tình** gửi `49=WT`
(`1c_InvalidSenderCompID.def`) và `56=DLSI` (`2k_CompIDDoesNotMatchProfile.def`), cùng một
`56=` **rỗng**. Test giờ khẳng định phân bố thật: 289 message gửi đi, đúng **5** cái không đọc
được identity, và đúng ba file — `14b_RequiredFieldMissing`, `2d_` và `3c_GarbledMessage`.

**Ba reversal, cả ba đỏ đúng assertion:**

| Bẻ cái gì | Kết quả |
|---|---|
| khớp tag ở **bất kỳ đâu trong một field** | 1/8 đỏ — `a_field_value_that_looks_like_an_identity_is_not_one` |
| bỏ hẳn ranh giới field, quét cả message | 1/8 đỏ — cùng test đó |
| `is_logon` luôn trả `true` | 2/8 đỏ — hai test đếm |

**Và đây là phát hiện đáng giá hơn code:** hai reversal đầu để **289/289 message thật của
corpus xanh**. Chỉ một message *tự dựng* bắt được, và nó chỉ chèn một field `58=49=EVIL` vào
một `Logon` thật. Corpus conformance mã hoá *cái mà spec nói về lỗi* — hỏng cấu trúc; nó không
mã hoá *cái mà một bên không đáng tin sẽ chọn* — một message hoàn toàn hợp lệ mà **giá trị**
được chọn để bị đọc sai. Viết ở
[a-conformance-corpus-is-not-an-adversarial-one.md](../reference/a-conformance-corpus-is-not-an-adversarial-one.md),
đánh dấu `[to testing-skills]`. Nó **không** làm yếu §7: corpus thật vẫn là cổng chính, và
chính nó bắt được giả định sai của test.

**Gate cho bước 2:** `fmt` sạch · `clippy --all-targets -D warnings` sạch · `cargo test --all`
**258 pass 0 fail**, `--no-default-features` **258 pass 0 fail**, `--features affinity`
**107 pass 0 fail** · `bench.sh --strict` **OK**, mọi dòng `allocations:` đều 0 (đường
`Engine::turn` có đổi, nên bất biến 1 được chạy lại chứ không suy).

**2026-09-01 — bước 3 xong.** `Limits`, `LimitError`, `PendingSet`, `Pending`, `Refused`,
`Progress` trong `presession.rs`; `crates/engine/tests/pending.rs`, **12 test**. `Limits` là
struct có tên chứ không phải hai tham số vị trí: cả hai đều là số, và `(30_000, 8)` — bảng ba
mươi nghìn chỗ hết hạn sau tám mili giây — sẽ biên dịch trót lọt. Số 0 bị từ chối ở cả hai
chiều.

**Bốn reversal, mỗi cái đỏ đúng ca đặt tên nó:**

| Bẻ cái gì | Đỏ |
|---|---|
| bỏ logon timeout | 4/12 |
| bỏ trần số lượng | 3/12 |
| chỉ giao lại **message**, không giao mọi byte đã đọc | **1/12** |
| cho message đầu không phải `Logon` đi qua | 1/12 |

**Reversal thứ ba là lý do phải thêm một test trước khi đảo ngược.** Khi viết xong 11 test tôi
nhận ra không cái nào bắt được nó: mọi ca đều gửi đúng một message, nên "cả buffer" và "message
đó" là cùng một slice. `whatever_arrives_behind_the_logon_is_handed_on_with_it` gửi `Logon` cộng
một message nữa trong một lần `send`, và nó là **test duy nhất** đỏ khi bẻ. Đúng cái bẫy mà plan
gọi là nguy hiểm nhất, và nó suýt không có gì canh.

**`Loopback` không mô hình hoá đóng-khi-drop**, nên ba assertion `Io::Closed` đầu tiên đỏ trong
khi code đúng. Không sửa `Loopback` — nó là transport mà 59 định nghĩa chạy trên đó, và sửa một
test double đang gánh cổng để test mới xanh đúng là hình dạng §10 cảnh báo. Thay vào đó: đếm và
thời gian đo trên `Loopback` (tất định), còn **EOF đo một lần trên socket thật**, nơi nó là sự
thật về thứ sẽ ship chứ không phải về một cái double.

**Và một false green của chính tôi, đã viết lại:** hai case cấp phát đầu tiên đọc 0 và **vẫn đọc
0** khi tôi bỏ `Vec::with_capacity` để mỗi `admit` phải cấp phát lại — vì `admit` nằm **ngoài**
cửa sổ `count()`. Case thứ hai được thêm vào đúng để chống chuyện đó, và nó thừa hưởng luôn cái
lỗ nó sinh ra để bịt. Case thứ ba bọc cả chu trình `admit`→`turn`→`take`: xanh 0, và **đỏ ở 7**
khi bỏ đặt chỗ một lần. Viết ở
[the-guard-measured-a-window-that-excluded-the-thing.md](../reference/the-guard-measured-a-window-that-excluded-the-thing.md),
`[to testing-skills]`.

**Cố tình chưa làm, nói rõ:** `GUIDE.md` chưa nói gì về hai giới hạn này, vì `serve_sharded_hft`
chưa nhận `Limits` — người dùng chưa với tới được. Nó đi cùng phần viết lại §1a ở bước 4/5.

**Gate cho bước 3:** `fmt` sạch · `clippy --all-targets -D warnings` sạch · `cargo test --all`
**270 pass 0 fail**, `--no-default-features` **270**, `--features affinity` **119** ·
`bench.sh --strict` **OK**, mười hai dòng `allocations:` đều 0 · `check-machine.sh`
`pass 11 fail 0`.

**2026-09-01 — bước 4 và 5 xong. `shard_wire.rs` đọc 59 với hai shard.**

`Engine::add_with_prefix` + `Connection::prime` + `Framer::all` giao lại **mọi byte đã đọc**;
`Route`/`HashRoute` thay `Assign`/`RoundRobin`; `Shards<PRE>` mang `Pending` qua kênh (mảng
di chuyển, không cấp phát); `serve_sharded_hft` chạy `PendingSet` + `Poller` trên luồng
acceptor, chờ **đúng tới deadline sớm nhất** chứ không theo một chu kỳ ai đó tự chọn.

**Test đặc tả đỏ trước, đúng như plan bắt:** `59 != 57`. Nếu nó còn xanh thì khiếm khuyết mới
bị đi vòng chứ chưa sửa.

**Băm được ghim vào giá trị cụ thể**, không chỉ "tất định trong tiến trình này":
`(TW44,ISLD)` → shard 1 của 2 và shard 3 của 8; `(WT,DLSI)` → shard 1 của 8. Một `DefaultHasher`
có seed theo tiến trình không thể tái lập bốn hằng số đó — đấy là điều test này mua được, và
`a_route_outside_the_range_is_refused_and_not_clamped` canh việc không lấy dư.

**Và 59/59 không được nhận nguyên xi.** `1b_DuplicateIdentity.def` với `AlreadyLoggedOn.def`
đều chờ **không có hồi đáp nào** ở kết nối thứ hai — mà một socket bị tầng mới vứt đi cũng cho
ra đúng thế. Thêm bộ đếm mọi cách tầng này thải socket, khẳng định bằng 0, và `[đo 2026-09-01]`
**nó đỏ ở `[0, 1, 1, 0]`**: hai kết nối chưa từng tới engine nào.

Cả hai hợp lệ, và cả hai giờ được ghim **theo tên và theo số**:

| File | Comment của chính nó | Tầng mới làm gì |
|---|---|---|
| `1e_NotLogonMessage.def` | *"if first message is not a Logon, we must disconnect"* | message trọn vẹn đầu tiên là `35=0` → vứt, không trả lời |
| `1d_InvalidLogonLengthInvalid.def` | *"if the length of a logon message is invalid, we must disconnect"* | `9=40` nói dối; `Framer` tin `9=` → `Garbage` → vứt |

Một cái **thứ ba** biến mất ở đây sẽ là khiếm khuyết mới đội cùng màu xanh 59/59.

Việc đó đẩy luật garbage sang **nhà thứ hai**, đúng cái mà `frame.rs` đã viết ra là sẽ không
xảy ra. Sửa comment trong cùng commit, và quyết định ở
[ADR-0022](../decisions/ADR-0022-the-pre-session-stage-enforces-two-definitions.md) thay vì để
người sau đọc `frame.rs` tự phát hiện.

**Gate cho bước 4–5:** `fmt` sạch · `clippy --all-targets` và `--features affinity` đều sạch ·
`cargo test --all` **272 pass 0 fail**, `--no-default-features` **272**, `--features affinity`
**122**, `--no-default-features --features affinity` **100** · hai cổng 59 cũ xanh và
`git diff main -- crates/conformance crates/session tests/wire.rs` **rỗng** — không sửa fixture
nào · `bench.sh --strict` **OK**, mười hai dòng `allocations:` đều 0 ·
`check-no-kernel-sleep.sh`, `check-standard-gives-the-core-back.sh`, `check-lint-config.sh`,
`check-no-optional-deps.sh` đều exit 0 với nửa đỏ của chúng vẫn trượt.

**Còn lại: bước 6** — đo `Logon` mất thêm bao lâu vì đi qua tầng này, kèm `N` và khối machine.

**2026-09-01 — bước 6 xong, plan đóng.**

`crates/engine/benches/presession.rs`, ba case, baseline từ **20 lượt `bench.sh` hợp lệ** trên
dòng §9 của ADR-0021, `check-machine.sh` đọc `pass 11 fail 0 unknown 1`, max/median 1.006–1.011:

| Case | ns/op | mỗi socket |
|---|---|---|
| `presession sweep, 1 quiet sockets` | 435.9 | 435.9 |
| `presession sweep, 16 quiet sockets` | 6819.5 | **426.2** |
| `presession, read and route an identity` | **84.0** | một lần cho cả đời kết nối |

Quét của tầng này **rẻ hơn** một lượt `Engine::turn` cho mỗi socket (426.2 so với 458.3) — đúng
hình dạng phải có, vì nó không có session machine, không journal, không dispatch. Phần việc
riêng của nó trên `recv` là **~15 ns**, so với ~28 ns của engine. Quyết định định tuyến tốn
**84 ns, một lần** — một phần năm của một `recv`.

`HashRoute`/`Route` được **chuyển từ `shard.rs` sang `presession.rs`** trong bước này, vì
`bench.sh` chạy `cargo bench` **không kèm feature** và `shard` nằm sau `affinity`. Đó cũng là
chỗ đúng hơn: định tuyến theo identity chẳng liên quan gì tới ghim lõi. `shard` re-export.

**Cái KHÔNG đo được, và nói thẳng ra:** độ trễ thực tế mà một `Logon` phải trả thêm. Bảng trên
là giá của *công việc*; đường kết nối còn thêm một chặng kênh và một lần **bàn giao qua luồng**
— socket đọc ở luồng acceptor, phục vụ ở luồng shard — và không dòng nào ở đây nói gì về nó.
Một bench hình dạng này không đo được: setup mỗi vòng lặp sẽ phải mở socket mới **bên trong**
cửa sổ đo, tức là đo `TcpStream::connect`. Thứ đo được nó là `tools/w2w`, tức **open item 6**,
và nó chưa từng chạy.

**Gate đóng plan:** `fmt` sạch · `clippy` sạch ở cả hai feature set · `cargo test --all`
**272/0**, `--no-default-features` **272/0**, `--features affinity` **122/0** ·
`bench.sh --strict` **OK** với 12 target, 0 vượt baseline, 0 thiếu baseline ·
`check-machine.sh` `pass 11 fail 0 unknown 1`.
