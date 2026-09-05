# Ba con số trên máy §9: cổng đang đỏ, lượt kiểm chưa ai đo, và template mỗi message

> **Loại:** Plan · **Ngày:** 2026-09-02 · **Trạng thái:** Đã duyệt (2026-09-02), **đang làm**
> **Phạm vi:** open item 41 → 39 → 34, đúng thứ tự chủ dự án chọn
> **Máy chạy:** **bắt buộc máy §9** — `benches/baselines.tsv` khoá theo CPU model, không máy nào khác phát biểu được.

> **Sửa 1 `[2026-09-04]` — plan này sống trên một nhánh riêng hai ngày, và đó là vấn đề.**
> `plan/baselines-and-the-untimed-pass` là plan **duy nhất** của repo không nhìn thấy từ `main`,
> trong khi bảy draft wave B/C/D chưa ai bắt đầu thì nằm sẵn đó. Hệ quả không phải là bất tiện:
> vòng đi bảng đồng bộ §4 không với tới nó, `grep -rn '\[to testing-skills\]' docs/` không thấy
> nó, và `STATUS.md` là thứ duy nhất đứng giữa công việc này với việc bị quên. Merge rồi
> (`ffe7a31`); plan **vẫn mở**, chỉ là nhìn thấy được.
>
> **Bước 1 (chẩn đoán) đã xong** — commit `0e845f5`, và kết luận của nó vẫn đúng, đã kiểm lại
> hôm nay chứ không tin theo:
>
> | Kiểm lại `[2026-09-04]` | |
> |---|---|
> | `crates/engine/src/presession.rs` | **không đổi** kể từ `0e845f5^` — nên `identity_of` vẫn quét bốn lần `field_value` |
> | `crates/engine/benches/presession.rs` | **không đổi**, hai case `registry lookup` vẫn ở đúng vị trí cũ trong suite |
> | `benches/baselines.tsv` | **không đổi** — năm case vẫn thiếu baseline cho CPU này |
>
> Nghĩa là **bước 1a chạy lại được nguyên văn**, không phải viết lại.
>
> **Cái đã đổi quanh nó, và không cái nào chạm vào giả thuyết của plan:** `SLOTS` 8 → 4096 với
> `MemJournal` đóng hộp (ADR-0046), `Journal::put -> bool` và `oldest`, module `msglog` với
> tham số kiểu thứ mười trên `Engine`, key thứ 11 trong `settings`. Hai cái đầu **có** chạm
> `benches/alloc.rs`, là bench *đếm cấp phát*, không phải bench *đo thời gian*, nên `--strict`
> không nhìn vào chúng.
>
> **Một hệ quả mới đáng ghi vào bước 1d:** `benches/alloc.rs` của `engine` giờ là **24 case**,
> không phải 21, và ba case mới (`log-record`, `log-idle`, `log-busy`) mất bốn lần đo mới ra 0 —
> hai lần đầu là harness và writer thread chứ không phải engine
> ([a-benchmark-measured-its-own-fixture](../reference/a-benchmark-measured-its-own-fixture.md)).
> Bước 1d ghi baseline cho **năm case đo thời gian**, và cùng một cách tự lừa áp dụng: một con số
> ổn định không có nghĩa là nó đo đúng thứ mình nghĩ.

> **Sửa 2 `[2026-09-05]` — bước 1a sai giả thuyết, bước 1c sai commit, và một dữ kiện của plan
> này sai hẳn. Ghi trước khi đổi một dòng nào.**
>
> **1a phủ định.** Bỏ hai case `registry lookup` ra khỏi suite: `read and route` đi từ **191.8
> xuống 186.8 ns**, không về 84. Chiều thứ hai mà *Cách kiểm chứng* đòi cũng đã chạy — đưa hai
> case lên **đầu** suite đọc 190.8. Vậy con số đi theo **sự có mặt** (−5 ns, −2.6%) chứ không
> theo **vị trí** (<1 ns), tức là theo đúng cái phân biệt plan này viết ra thì nguyên nhân
> **không phải thành phần suite**.
>
> **1b không cần bisect, vì phép đảo ngược mạnh hơn bisect và đã chạy lại.** Thay hai lời gọi
> `field_value` mới bằng `None`: **83.5 · 84.1 · 84.7 · 84.3 · 84.0**, median **84.1** so với
> baseline **84.0**. Kết luận của bước 1 đứng vững: ADR-0026 nới `Identity` thêm `50=`/`57=`,
> `identity_of` quét bốn lần thay vì hai, và baseline chưa ai ghi lại.
>
> **Một dữ kiện của *Những gì đã biết chắc* sai, và chính nó đẻ ra giả thuyết 1a.** Gạch đầu
> dòng 2 nói `presession.rs` *"chỉ đổi đúng một lần"* kể từ `f15c82d` và lần đó là `93675c2`.
> `git log` nói nó đổi **ba lần**, và một trong ba là **`0cfa904`** — đúng commit ADR-0026 nới
> `Identity`. Đọc diff `identity_of` ở `f15c82d` so với hôm nay thì thấy ngay hai lời gọi mới.
> Dữ kiện sai ấy loại đúng thủ phạm ra khỏi danh sách, nên phần còn lại phải đi tìm nguyên nhân
> ở chỗ khác — và đó là cả giả thuyết 1a.
>
> **1c: bisect một commit là đủ hẹp, và nó chỉ sai commit.** ADR-0044 (`576f924`) có thật nhưng
> nhỏ: 267.5 → 279.4, **+4.5%**. Case đã **vượt trần trước nó**. Bisect rộng ra:
>
> | commit | là gì | median |
> |---|---|---|
> | `bf798ea` | ghi baseline 239.1 | **240.0** — máy và toolchain vô can |
> | `54eebe9` | sửa một bench khác | 240.6 |
> | **`4396d6d`** | **a baseline is a band — chỉ harness** | **268.0** |
> | `576f924` | ADR-0044 | 279.4 |
> | `HEAD` | hôm nay | 280.4 |
>
> **`4396d6d` không chạm một dòng `crates/codec/src/` nào** (`git show 4396d6d -- crates/codec/src/`
> rỗng). Nó `include!("verdict.rs")` vào binary bench, và +11.4%. Và ở đây plan này sai lần thứ
> hai: *Những gì đã biết chắc* gọi `4396d6d` là **dữ kiện phủ định** *"dụng cụ đo không đổi"* —
> kết luận rút ra bằng **đọc diff**. Điều 10 nói thẳng review diff bắt được gần như không gì.
>
> **Nguyên nhân là layout của binary, và nó được chứng minh hai lần bằng hai đường khác nhau:**
> ép `-C llvm-args=-align-all-functions=6` kéo `HEAD` từ 278.9 về **233.0**; và — vì một cái núm
> nhúc nhích không phải là một nguyên nhân — chèn **code trơ** (N hàm encoder không bao giờ gọi)
> làm con số đi **236.5 → 292.4** qua bốn layout, không đổi một dòng code bị đo. Cùng phép nhiễu
> đó dưới căn lề ép: 229.6 · 238.9 · 235.8 · 230.9, spread **4.0%** so với **14.6%**.
> [a-benchmark-that-measures-where-the-compiler-put-it](../reference/a-benchmark-that-measures-where-the-compiler-put-it.md),
> **`[to testing-skills]`**.
>
> **Chủ dự án chọn *cả hai* (2026-09-05): ghim căn lề cho bench, VÀ nới margin case này lên
> 1.15.** ADR-0049. Ba việc kèm theo, và việc thứ hai là bắt buộc chứ không phải trang trí:
>
> 1. `scripts/bench.sh` xuất `RUSTFLAGS` ghim căn lề — phạm vi đúng bằng cái script mà quy trình
>    baseline đã bắt buộc dùng, không phải `.cargo/config.toml` (cái đó sẽ đổi cả bản ship).
> 2. **Một gate đọc lại rằng cờ thật sự có tác dụng.** Một cờ LLVM im lặng ngừng ăn là đúng hình
>    dạng false-green repo này gặp ba lần. Không có gate này thì quyết định 1 là một lời hứa.
> 3. Baseline của **cả năm case thiếu và hai case đỏ** được ghi **dưới build đã ghim**, ≥ 20 lần
>    chạy — 20 lần chạy chưa ghim thu lúc 09:30 bị **vứt bỏ**, không dùng.
>
> **Bước 1d đổi phạm vi**: từ *"5 case thiếu baseline"* thành *"5 case thiếu + `encode
> ExecutionReport (template)` + `presession, read and route an identity`"*, vì hai case sau giờ
> đã có kết luận và cả hai kết luận đều là *ghi lại baseline*, không phải *sửa code*.

## Bối cảnh

Phase 1 vừa đóng, và buổi đo đóng nó để lại ba việc **trên cùng một cái máy**. Cả ba đều là
chuyện *con số*, và cả ba chỉ trả lời được ở đây vì `benches/baselines.tsv` khoá theo **CPU
model** — không máy nào khác có quyền phát biểu.

**Item 41 đi trước, và nó không phải "dọn dẹp".** `scripts/bench.sh --strict` là cái cổng mà
bất di bất dịch số 10 tựa vào: *không con số hiệu năng nào tồn tại nếu thiếu benchmark, máy, và
cấu hình §9*. Cổng đó **đang đỏ trên chính máy §9**. Trong lúc nó đỏ, mọi con số §6 của dự án
này không có cổng nào canh — kể cả bốn con số wire-to-wire vừa công bố.

**Item 39 đi thứ hai vì nó cùng một buổi đo.** Vòng app cao hơn vòng admin **3 898 ns**, và
mọi benchmark đã cam kết cộng lại chỉ giải thích **~320 ns**. Ứng viên lớn nhất là lượt kiểm
dictionary của session, và **không benchmark nào đo nó** — nên `DESIGN.md` §8 có một dòng
`Parse (D2)` đo bằng `NoDict` trong khi engine parse bằng `Fix44`.

**Item 34 đi thứ ba** vì nó là item duy nhất trong ba cái mà *phần sửa code* làm ở đâu cũng
được; chỉ *con số* mới cần máy này. Mọi figure của tầng library hiện là **từ một VM**.

## Những gì đã biết chắc

Tất cả đo/đọc trong phiên 2026-09-02 trên máy §9, `check-machine.sh` `pass 12 fail 0 unknown 1`.

**Về item 41 — hai case vượt band, lặp 6/6 lần chạy, máy đọc 0–1% busy:**

| Case | Đọc được | Baseline × 1.10 | Lệch |
|---|---|---|---|
| `encode ExecutionReport (template)` | 274,2 · 279,6 · 275,5 · 283,3 · 275,0 · 279,4 | 239,1 → trần 263,0 | **+16%** |
| `presession, read and route an identity` | 201,3 · 197,9 · 201,6 · 202,2 · 209,3 · 205,7 | 84,0 → trần 92,4 | **+140%** |

**Năm case không có baseline cho CPU này** — và đó là thứ làm `--strict` thoát khác 0:
`library, parse only` 142,0 · `library, reply only` 797,0 · `library, on_message` 995,6 ·
`presession, registry lookup of 1` 10,8 · `presession, registry lookup of 40` 102,6.

**Mọi case còn lại trong band**, kể cả `parse NewOrderSingle (validated)` 119,8–121,7 so với
122,6 — chính là con số ADR-0045 dựa vào.

**Ba dữ kiện từ `git log`, và chúng thu hẹp cả hai nghi vấn xuống gần một commit:**

1. **`crates/codec/src/template.rs` chỉ đổi đúng một lần** kể từ khi baseline 239,1 được ghi
   (2026-08-31): **`576f924 perf(codec): a builder that is not moved once per field`** —
   ADR-0044. Đây là một phép bisect **một commit**.
2. **`crates/engine/src/presession.rs` chỉ đổi đúng một lần** kể từ khi baseline 84,0 được ghi
   (2026-09-01), và thay đổi đó là **thêm `Display` + `std::error::Error` cho `LimitError`** —
   `93675c2`. **Không thể** ảnh hưởng `identity_of`. Nên nguyên nhân **không nằm trong source
   của case đó**.
3. **Hai case `registry lookup of` được thêm bởi `61e5cd7`, tức là SAU khi baseline của
   `read and route` được ghi.** Suite đổi thành phần. Đó là nghi vấn dẫn đầu, và nó kiểm được
   bằng một bước.

**Và một dữ kiện phủ định, quan trọng vì nó loại một nghi vấn hiển nhiên:**
`4396d6d feat(bench)!: a baseline is a band` có sửa `crates/codec/benches/harness.rs`
(+59/−10), nhưng đọc diff thì nó **chỉ đổi logic phán quyết và phần in ra** — `best` được đo y
nguyên. **Dụng cụ đo không đổi.** Nên không thể giải thích cả hai case bằng "cái thước đã
khác".

**Về item 39 — lượt kiểm dictionary nằm ở đâu:** `crates/session/src/lib.rs` quanh dòng
2245–2340. Mỗi field bị hỏi `Fix44::is_header`, `is_defined_tag`, `field_type`,
`allows(msg_type, tag)`, `enum_allows`, `field_type().accepts()`, cộng kiểm group delimiter và
member; rồi hai vòng `required_header()` và `required(msg_type)`, mỗi tag bắt buộc gọi
**`view.get(tag)` — một lần quét tuyến tính danh sách field**. `NewOrderSingle` mang 14 field
và ~13 tag bắt buộc; `Heartbeat` mang 6 và ~8. **Các hàm này ở trong `fn` private**, nên bước
đầu của item 39 là câu hỏi *đo được từ đâu*, không phải câu hỏi *đo bao nhiêu*.

**Về item 34:** ADR-0044 đã bỏ nửa chi phí (`TemplateBuilder` copy `self` mỗi field).
`library, reply only` đọc **797,0 ns** ở đây so với **~956 ns** ghi từ VM, và `on_message`
**995,6** so với ~2 100 → 956 trước/sau ADR-0044 trên VM. **Không có baseline nào cho CPU
này**, nên ba case này chính là ba trong năm case làm `--strict` đỏ. Nghĩa là **item 34 và item
41 gặp nhau ở đúng chỗ đó**: ghi baseline cho `fixbolt/cost` là việc của 41, còn *làm cho con
số nhỏ đi* là việc của 34.

## Cách làm

Ba bước tách bạch, và **bước 1 không được sửa code nào của thư viện** — nó là bước *chẩn đoán*,
và nếu nó kết luận cần sửa code thì đó là một plan mới.

### Bước 1 — item 41: hỏi "cái gì đổi", không hỏi "làm sao cho xanh"

**Cấm tuyệt đối một việc: ghi lại baseline để cổng xanh.** Đó là dạng thất bại `CLAUDE.md` §10
gọi tên — *"một fixture bị sửa để việc mới đi qua được"* — và ở đây nó dễ hơn mọi chỗ khác vì
`baselines.tsv` là một file text.

1. **`presession, read and route an identity` trước**, vì nó lệch 140% và vì nghi vấn của nó
   kiểm được bằng **một biến**: tạm **bỏ hai case `registry lookup of`** ra khỏi
   `benches/presession.rs`, chạy lại, đọc `read and route`. Nếu nó về ~84 ns thì nguyên nhân là
   *thành phần của suite*, không phải code — và câu trả lời đúng là **ADR về việc một case chỉ
   có nghĩa trong suite nó được ghi baseline cùng**, chứ không phải sửa `identity_of`.
   Nếu nó **vẫn** ~200 ns thì nghi vấn sai và bước 1b chạy.
1b. **Nếu vẫn ~200 ns**: bisect. `f15c82d` (nơi ghi baseline) → `HEAD`, chạy case đó ở mỗi
   commit ứng viên. Danh sách ứng viên là `git log f15c82d..HEAD -- crates/engine/`.
2. **`encode ExecutionReport (template)`**: bisect **một commit** — dựng ở `576f924^` và ở
   `576f924`, chạy case đó ≥ 5 lần mỗi bên, so trung vị. ADR-0044 đổi `TemplateBuilder`, còn
   bench dựng template **ngoài** vòng đo, nên đường duy nhất nó chạm tới `encode` là **layout
   của `Template`**. Nếu bisect chỉ vào đó thì đọc `struct Template` trước/sau và nói ra cái gì
   đổi kích thước.
3. **Năm case thiếu baseline**: ghi baseline cho CPU này theo **đúng quy trình đã có** —
   ≥ 20 lần chạy `bench.sh` nguyên trên máy đọc `fail 0`, trung vị, margin là bậc nhỏ nhất của
   thang 1.10/1.15/1.20/1.25/1.30/1.35 mà ≥ max/median, kèm `n` và ngày và verdict.
   **Ba case `fixbolt/cost` được ghi baseline ở đây, không phải bị sửa cho nhanh hơn** — cái đó
   là bước 3.
4. Kết quả bước 1: `bench.sh --strict` **thoát 0**, hoặc một câu nói rõ case nào vẫn đỏ và vì
   sao, kèm ADR nếu có quyết định.

### Bước 2 — item 39: đo cái lượt kiểm, rồi để §8 nói thật

1. **Trước hết là câu hỏi đo-từ-đâu.** Ba lựa chọn, chọn bằng cách đọc code chứ không đoán:
   (a) một case trong `crates/session/benches/` gọi qua đường công khai đã có, nếu có đường nào
   chạm được lượt kiểm mà không chạm phần khác; (b) mở một API `pub(crate)` + một bench trong
   cùng crate; (c) đo **hiệu số** — cùng một message, một lần qua `Session` với dictionary và
   một lần với một dictionary rỗng — nếu `Session` cho phép chọn. **(c) là hình dạng ưa hơn**
   vì nó không thêm public API, nhưng chỉ dùng được nếu nó thật sự chỉ đổi một biến.
2. Đo trên **`NewOrderSingle` và `Heartbeat`**, vì cả item 39 nói chi phí là *theo field* và
   *theo tag bắt buộc*, và hai message đó là hai đầu của khoảng đó (14 field / ~13 tag so với
   6 / ~8).
3. Ghi baseline, ≥ 20 lần chạy, đúng quy trình bước 1.3.
4. **`DESIGN.md` §8**: dòng `Parse (D2)` hiện đã ghi rõ nó **không** gồm lượt này; nay nó gồm
   một dòng riêng có số. Và **3 898 ns của vòng app được cộng lại**: nếu lượt kiểm giải thích
   phần lớn, nói ra bao nhiêu; nếu không, **nói ra là vẫn không giải thích được** và giữ item
   39 mở với con số mới. **Không được suy nguyên nhân từ việc con số lớn.**

### Bước 3 — item 34: con số ở máy này, rồi mới nói chuyện sửa

1. Ba case `fixbolt/cost` đã có baseline từ bước 1.3, nên lần đầu tiên tầng library có figure
   **§9** thay vì figure VM. Nói rõ tỉ lệ giữa `library, reply only` và
   `encode ExecutionReport (template)` — ADR-0041 dựa vào tỉ lệ đó.
2. **Rồi mới quyết định có sửa hay không**, và nếu sửa thì **đó là một plan riêng**: bỏ việc
   materialise `Template` mỗi message là thay đổi public API của `fixbolt::Message`, tức là
   Rule Zero.
3. Nếu con số §9 cho thấy khoảng cách nhỏ hơn ADR-0041 tưởng, thì kết quả của bước này là
   **một ADR nói item 34 nhỏ hơn nó từng được ghi**, chứ không phải một bản sửa.

## Bất biến bị đụng tới

- **Số 10** (không con số nào thiếu benchmark, máy, cấu hình §9) — **đây là điều luật trung tâm
  của plan này**, và bước 1 là nó đang đỏ.
- **Số 1** (không cấp phát trên hot path) — bước 2 có thể mở API để đo; một API mở ra để đo
  không được cấp phát, và `benches/alloc.rs` phải vẫn đọc 0.
- **Số 5** (thứ tự field từ bảng sinh ra) — bước 3 chạm `Template`; không được để một call site
  nào quyết định thứ tự.
- **Số 7** (không `panic!`/`unwrap()`/`expect()` trong library crate) — mọi API mới ở bước 2.

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1a | Một biến: bỏ hai case `registry lookup`, đọc `read and route`. Kết luận *suite* hay *code* | — |
| 1b | Nếu 1a phủ định: bisect `f15c82d..HEAD` trên `crates/engine/` | 1a |
| 1c | Bisect một commit cho `encode ExecutionReport`: `576f924^` so với `576f924` | — |
| 1d | Baseline cho 5 case thiếu, ≥ 20 lần chạy, đúng quy trình | 1a–1c xong (để không ghi baseline lên một hồi quy) |
| 1e | `bench.sh --strict` thoát 0, **hoặc** một câu nói rõ cái gì còn đỏ. ADR nếu có quyết định. **Item 41 đóng** | 1a–1d |
| 2a | Trả lời "đo lượt kiểm từ đâu" bằng cách đọc code, chọn (a)/(b)/(c) | — |
| 2b | Bench cho lượt kiểm, `NewOrderSingle` và `Heartbeat`, đỏ trước | 2a |
| 2c | Baseline ≥ 20 lần chạy | 2b, 1e |
| 2d | `DESIGN.md` §8 có dòng riêng; 3 898 ns được cộng lại hoặc **vẫn ghi là chưa giải thích được**. **Item 39 đóng hoặc hẹp lại** | 2c |
| 3a | Figure §9 cho ba case `fixbolt/cost`, và tỉ lệ mà ADR-0041 dựa vào | 1d |
| 3b | ADR: hoặc item 34 nhỏ hơn nó từng được ghi, hoặc một plan riêng để sửa | 3a |

## Cách kiểm chứng

- **Bước 1a là bước dễ tự lừa nhất trong cả plan.** Nếu bỏ hai case đi mà `read and route` về
  84 ns thì **cám dỗ là kết luận ngay**. Phải làm tiếp một chiều nữa: **thêm lại hai case ở một
  vị trí khác trong suite** (trước/sau) và xem con số đi theo vị trí. Một con số đi theo *vị
  trí* thì nguyên nhân là suite; một con số chỉ đi theo *sự có mặt* thì là chuyện khác.
- **Mọi con số ở đây là trung vị của ≥ 5 lần chạy**, không phải một lần — quy tắc của chính dự
  án này, và hôm nay nó vừa cứu một kết luận sai (200 mẫu so với 2 000 mẫu đọc p99.9 khác nhau
  hoàn toàn).
- **Bench mới ở bước 2 phải đỏ trước.** Một bench mới xanh ngay từ đầu chưa chứng minh nó đo gì.
  Đảo ngược: cho lượt kiểm trả về ngay `None` và xem case rơi xuống.
- **`bench.sh --strict` được đọc, không đọc exit code** — đọc từng dòng, vì đúng cổng này đã
  từng xanh trong khi không đo gì (`inline deliver + reply` 1,3 ns suốt một ngày).
- Mỗi bước: `cargo test --all`, `cargo test --all --no-default-features`, `benches/alloc.rs`.
- **Máy phải im trước mỗi lần đo**: `ps -eo pcpu,comm --sort=-pcpu | head`, và Chrome là thứ
  hôm nay làm rơi 5 lần chạy trên 20.

## Tài liệu phải cập nhật

- [ ] `benches/baselines.tsv` — 5 dòng mới (bước 1d), + 1–2 dòng (bước 2c), mỗi dòng kèm `n`,
      ngày, verdict
- [ ] `docs/DESIGN.md` §6 — nếu một ceiling hay cách đo nào đổi
- [ ] `docs/DESIGN.md` §8 — dòng cho lượt kiểm dictionary; và **cộng lại 3 898 ns**
- [ ] `docs/reference/measured-costs.md` — **ưu tiên cao nhất**: mọi phát hiện của bước 1, kể
      cả nếu nó là "một case chỉ có nghĩa trong suite nó được ghi baseline cùng"
- [ ] `docs/decisions/` — ADR mới nếu bước 1e hoặc 3b ra một quyết định
- [ ] `STATUS.md` — item 41, 39, 34
- [ ] `docs/GUIDE.md` §8 — nếu bước 1 tìm ra một cách benchmark tự lừa mình mới

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| **Ghi lại baseline cho hai case đỏ để cổng xanh** | Bước 1d **phụ thuộc** 1a–1c; và không dòng nào của hai case đó được sửa trước khi có kết luận |
| Bỏ hai case `registry lookup` rồi kết luận ngay | Chiều thứ hai ở "Cách kiểm chứng": đổi **vị trí** hai case, xem số có đi theo |
| Bisect một commit rồi tin luôn, không đọc code | Nếu bisect chỉ vào `576f924`, phải đọc `struct Template` hai bên và **nói ra cái gì đổi kích thước** |
| Bench mới ở bước 2 đo cả việc parse, rồi gọi đó là lượt kiểm | Đo **hiệu số** nếu được (lựa chọn c); nếu không thì phải nói rõ nó gồm những gì |
| Kết luận lượt kiểm là nguyên nhân của 3 898 ns vì nó lớn | Bước 2d bắt buộc **cộng lại** và nói ra phần dư. Đây là bẫy đã làm dự án này công bố sai một ngày |
| Mở public API chỉ để đo, rồi để nó ở đó | Lựa chọn (b) phải là `pub(crate)`, và nếu buộc phải `pub` thì cần ADR |
| Đo trong lúc Chrome chạy | `ps` trước mỗi lần đo; và `bench.sh` tự đọc dòng quiet |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Bước 1a phủ định và bisect ra một commit không ai ngờ | Trung bình | Đó là kết quả. Ghi vào `measured-costs.md`, và nếu cần sửa code thì **plan mới** |
| `encode` chậm 16% là thật và ADR-0044 là nguyên nhân | Trung bình | ADR-0044 mua nửa chi phí của tầng library; nếu nó bán 16% của `encode` thì **đó là một trao đổi phải nói ra**, có thể là một ADR đảo lại |
| Lượt kiểm không đo được mà không mở public API | Trung bình | Lựa chọn (c) trước; nếu cả ba không được thì item 39 **hẹp lại thành "đo bằng hiệu số wire-to-wire"** và nói rõ đó là cận trên |
| 3 898 ns vẫn không giải thích được sau bước 2 | Trung bình | Đó là kết quả, và item 39 giữ mở với một con số mới. **Không đoán tiếp** |
| Đo 20 lần × nhiều case tốn cả buổi ở máy | Cao | Là cái giá của bất di bất dịch 10. Chạy nền, đọc sau |

## Ngoài phạm vi

- **Không** sửa `Template` để `encode` nhanh hơn — bước 1c chỉ *tìm ra*, không *sửa*.
- **Không** bỏ việc materialise `Template` mỗi message ở tầng library — đó là plan riêng của
  item 34 nếu bước 3 kết luận nên làm.
- **Không** đụng item 40 (NIC-to-NIC) — cần một máy thứ hai trên cùng switch, xem câu trả lời
  đã ghi trong `STATUS.md` item 40.
- **Không** đụng item 21, 32(a), 36, 38.
- **Không** làm SIMD/SWAR — ADR-0045 đã đóng item 12.

## Nhật ký giao hàng

| Bước | Ngày | Kết quả |
|---|---|---|
| 1 (chẩn đoán) | 2026-09-02 | **Xong**, commit `0e845f5`. `read and route` +140% **không phải hồi quy**: ADR-0026 quyết định 2 nới `Identity` thêm `50=`/`57=`, nên `identity_of` quét bốn lần `field_value` thay vì hai, và `field_value` quét hết message trước khi trả `None` — đúng việc hai lời gọi mới làm, vì `Logon` của corpus không mang cả hai tag (đã đếm: 0 lần xuất hiện). Reversal một biến: thay hai lời gọi bằng `None` đọc **83.2 / 83.1 / 83.1 ns** so với baseline **84.0**, ba lần chạy. Kết luận: **baseline chưa ai ghi lại**, không phải code chậm đi, và không revert ADR-0026. `encode ExecutionReport (template)` +16% **chưa chẩn đoán** — hai giả thuyết trong *Cách làm*, cả hai chưa thử |
| 1a | 2026-09-05 | **Xong, và giả thuyết bị phủ định.** Bỏ hai case `registry lookup`: `read and route` 191.8 → **186.8**, không về 84. Chiều thứ hai cũng chạy — đưa hai case lên đầu suite đọc **190.8**. Số đi theo **sự có mặt** (−5 ns), không theo **vị trí** (<1 ns). Nguyên nhân **không phải thành phần suite** |
| 1b | 2026-09-05 | **Không bisect, và lý do mạnh hơn bisect.** Đảo ngược một biến — thay hai lời gọi `field_value` của ADR-0026 bằng `None` — đọc **83.5 · 84.1 · 84.7 · 84.3 · 84.0**, median **84.1** so với baseline **84.0**. Đọc `git log` xác nhận `presession.rs` đổi **ba** lần từ `f15c82d`, một trong đó là `0cfa904` (ADR-0026) — dữ kiện *"chỉ đổi đúng một lần"* của plan sai, xem Sửa 2 |
| 1c | 2026-09-05 | **Xong, và bisect một commit chỉ ra sai commit.** `bf798ea` (nơi ghi baseline) hôm nay vẫn đọc **240.0** → máy và toolchain vô can. Bisect: `54eebe9` 240.6 → **`4396d6d` 268.0** → `576f924` 279.4 → HEAD 280.4. `4396d6d` **không chạm `crates/codec/src/`**. Chứng minh hai đường: căn lề ép đưa HEAD 278.9 → **233.0**; chèn **code trơ** đưa con số **236.5 → 292.4** qua bốn layout (pinned: 4.0%, unpinned: 14.6%) |
| 1d | 2026-09-05 | **Xong, phạm vi rộng hơn plan viết.** ADR-0049 đổi *cách đo*, nên **cả 19 dòng cũ được ghi lại** dưới build đã ghim, cộng **5 dòng mới** = 24, `n = 20`, `pass 12 fail 0 unknown 1`. Không margin nào bị **thu hẹp** dù mẫu mới cho phép: `ring, one way` giữ 1.30, `ring, round trip` 1.20, `parse (no checks)` 1.15 — các mode chúng che chưa xuất hiện trong 20 lần này và không có bằng chứng chúng đã biến mất. `encode ExecutionReport (template)` lên **1.15** theo ADR-0049, dù thang từ 20 lần chạy chỉ đòi 1.10 — đúng chỗ cái lỗ của thang lộ ra |
| 1e | 2026-09-05 | **Xong. `bench.sh --strict` thoát 0, ba lần liên tiếp**, đọc từng dòng: `timing over baseline 0 · cases w/o a baseline 0 · cases under the band 0`. [ADR-0049](../decisions/ADR-0049-bench-builds-pin-function-alignment-and-the-flag-is-read-back.md), `scripts/check-bench-alignment.sh` (23/23 pinned, 5/23 unpinned, reversal đỏ 2/11), [write-up](../reference/a-benchmark-that-measures-where-the-compiler-put-it.md) mang `[to testing-skills]`. **Item 41 đóng** |
| 2, 3 | — | Chưa bắt đầu — item 39 (lượt kiểm dictionary) và item 34 |

**Trạng thái một câu `[2026-09-05]`:** **item 41 đóng** — `bench.sh --strict` xanh trên máy §9,
nên mọi con số §6 lại có cổng canh; và cái đắt nhất tìm được không phải một hồi quy mà là
**một benchmark đo chỗ trình biên dịch đặt nó**, thứ mà chính plan này đã loại trừ bằng cách đọc
diff. Bước 2 (item 39) và bước 3 (item 34) chưa bắt đầu.
