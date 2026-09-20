# Phase 2: trait `Encoding`, session FIXT 1.1 / FIX 5.0 SP2, rồi SBE

> **Loại:** Plan · **Ngày:** 2026-09-19 · **Trạng thái:** Chờ duyệt
> **Phạm vi:** phase 2 của `PRD.md` §2 — trục encoding và trục phiên bản; **không** FIXP,
> không FAST, không FIXML (ADR-0078 quyết định 4)
>
> **Sửa 2026-09-19 — chủ đã duyệt 2026-09-19:** cột *Phụ thuộc* của bảng *Chia việc* sửa cho
> đúng với file từng bước đụng (B1, B3, C1, C4, C5, C7, D1 đổi); thêm mục *Chạy song song* để
> ba phiên (bàn, cloud Linux, Mac) làm cùng lúc mà không giẫm file nhau; chỗ kế hoạch này
> **lệch `CLAUDE.md` §1** (hai crate mới trong một kế hoạch) chủ quyết theo **cách 1**: `sbe` +
> `sbe-gen` là một cặp, một bước §7 (bước 9), ghi vào `DESIGN.md` ở A4. Nội dung các bước,
> gate và tier **không đổi**.
>
> **Sửa 2026-09-19 (lần 2) — chờ chủ duyệt:** bản phác thảo mã ở *Cách làm* → *PR A* viết
> lại cho **đúng với trait đã xây** (commit `44df719`, `crates/codec/src/encoding.rs`). Bản
> phác cũ lệch mã ở ba chỗ, mỗi chỗ do một luật hoặc một file có sẵn ép buộc, nêu tên ngay
> dưới bản mới. **Hàng A1 của *Chia việc* — kết quả, file đụng, bốn gate, tier — không đổi
> một chữ và đã đạt đủ**, nên việc tiếp tục sang A2 theo `CLAUDE.md` §1 (kế hoạch sửa, chờ
> duyệt lại, không lệch lặng lẽ). Kèm theo: hàng A2 và mục *PR B* điểm 1 ghi rõ ràng buộc
> `where E::Dict: Tables` đặt ở `Session`, không đặt trên trait (trước đây viết
> `Encoding::Dict: Tables`, không thể được vì `codec` không phụ thuộc `dict`); hàng C4 ghi
> `SbeTables<S>` phải impl cả hai trait. Chủ chỉ cần duyệt bản phác; không có quyết định mới.
>
> **Sửa 2026-09-19 (lần 3) — chờ chủ duyệt, có một quyết định mới (ADR-0082):** A2 phát hiện
> không viết được `Session<E>` chỉ với `E: Encoding` + `E::Dict: Tables`: session đi bộ message
> theo thứ tự wire (`field_at`/`len`/`find_from`, `session/src/lib.rs:3775-3863`), dựng bảy
> khung session bằng `TemplateBuilder` (`out.rs`, `Outbound::new`), và match
> `ParseError::BadTag` — trait A1 không cho ba việc đó, và **đúng là không nên cho**. A2 đã
> viết năm đẳng thức kiểu trên `impl` (`View = MessageView<N>`, `Scratch = FieldIndex<N>`,
> `Template<24,320> = Template<24,320>`, `Field = u32`, `ParseError = ParseError`). Kiến trúc sư
> xác minh trên mã và phán: **đây là một quyết định chưa ai lấy, không phải lỗi của A1** —
> ADR-0079 quyết định 3 nói session "chỉ cần các trường session qua trait", nhưng mã và chính
> spec FIX Session Layer (§3.1.4: "valid FIX message" là chuỗi **tagvalue**; §4.5 bắt validate
> tagvalue + checksum tagvalue) nói khác. Ghi thành
> **[ADR-0082](../decisions/ADR-0082-the-session-is-generic-over-tag-value-encodings-and-the-boundary-to-sbe-is-the-session-not-the-trait.md)
> (Proposed)**: `Session<E>` generic trên **encoding tag=value**, năm đẳng thức là hợp đồng,
> ranh giới với SBE nằm ở `where` của `Session` chứ không ở trait; phương án mở rộng trait (đi
> bộ có thứ tự + builder + probe lỗi) bị loại và vì sao; phase 3 mất gì. Hệ quả trong kế hoạch:
> **A4** viết D16 theo ADR-0082, không chép nguyên câu của ADR-0079 quyết định 3; **C4 chốt**:
> `SbeTables<S>` chỉ impl `codec::Dictionary`, **không** impl `dict::Tables`, C4 không còn chờ
> A2; **C7** thêm doctest `compile_fail` ghi `Session<Sbe<S>>` bị từ chối. Mã A1, A2 **không
> đổi một chữ**; PR B không bị ảnh hưởng. Chủ phải **quyết** ADR-0082 (chấp nhận, hay muốn
> trait mở rộng để session không có đẳng thức — khi đó A1 và A2 làm lại và A-desk đo lại); nếu
> chấp nhận, manager đổi Status → Accepted trong commit A4.

## Bối cảnh

Phase 1 xong: một engine FIX 4.4 tag=value, hai vai, 59 / 59, số đo có máy có lệnh. Lệnh của
chủ dự án ngày 2026-09-18 gồm việc dựng phase 2. Hai ADR đã mở đường và đã được duyệt:

- **ADR-0078**: SBE vào phase 2 **chỉ là một encoding**, không có session riêng; FIXP để
  phase 3. Vậy phase 2 là việc của `codec` và một bộ bảng, không phải một state machine mới.
- **ADR-0079**: **mỗi encoding một kiểu view**, một trait `Encoding` phủ lên trên, dispatch
  tĩnh; `MessageView` giữ nguyên tên, kích thước và API. Bước đầu tiên của phase 2 là trait
  đó, và phải chứng minh nó không làm đường tag=value chậm đi (band ADR-0031, trên bàn §9)
  trước khi viết dòng SBE nào.

Kế hoạch này chia phase 2 thành ba khối theo thứ tự rẻ-trước: (A) trait `Encoding` với
`MessageView` giữ nguyên; (B) session FIXT 1.1 / FIX 5.0 SP2 — có sẵn oracle 180 `.def`
trong `vendor/`, là mục rẻ nhất phase 2 (`PRD.md` §2 bảng *Phase 2 starts with…*); (C) SBE
dưới trait, với oracle là hex dump trong spec và `sbe-tool` của Real Logic chạy qua script.
Cuối cùng (D) là `docs/internals`, phần C của the-doc-set bị hoãn sang "ADR đầu tiên của
phase 2" (STATUS item 33).

Hai quyết định mà 0078/0079 chưa cho, viết thành hai ADR đi kèm kế hoạch này, đều `Proposed`:

- **ADR-0080**: dictionary là associated type của encoding (`Encoding::Dict`), không phải
  tham số thứ hai của `Session`; session FIXT dùng **một bảng sinh từ hai file XML**
  (`FIXT11.xml` + `FIX50SP2.xml`) sau feature `fix50sp2`; quy tắc `1137`/`1128`.
- **ADR-0081**: bảng SBE do generator **của repo này** sinh (mẫu `dict/build.rs`), crate
  `sbe` runtime zero-dep `no_std`; oracle là ba hex dump trong spec SBE 1.0 RC4 §7 (không cần
  toolchain) cộng `sbe-tool` (Java, Apache-2.0) chạy qua `scripts/sbe-interop.sh` và một job
  CI riêng, như `interop.sh` với `libquickfix`.

## Những gì đã biết chắc

**Trong repo (đọc 2026-09-19, nhánh `plan/the-second-linux-desk-c`):**

- `Session<R: Role, const N: usize, const APP: usize>` (`crates/session/src/lib.rs:1289`)
  gọi thẳng `Fix44` 17 lần: `parse_into::<Fix44, N>` (`:2985`), `Fix44::is_msg_type`
  (`:3150`), và toàn bộ pass validate `:3735-3826` (`is_header`, `is_defined_tag`,
  `field_type`, `allows`, `enum_allows`, `group_delimiter`, `group_members`,
  `required_header`, `required`). Đó là **hàm inherent của struct sinh ra**, không phải
  trait — nên chưa generic được. `out.rs` thêm 8 chỗ.
- `codec::Dictionary` (`crates/codec/src/dict.rs:14`) chỉ có `is_header`, `data_length_tag`,
  `group_delimiter`, `group_members`, `group_order`. `MessageView<'a, N>` 24 byte, `Copy`,
  có `const _: () = assert!` trong `index.rs` (ADR-0003 guard 2).
- `dict/build.rs` đọc **một** file (`NANOFIX_FIX44_XML`, mặc định
  `vendor/quickfix/spec/FIX44.xml`) bằng `roxmltree = "=0.20.0"` build-dependency, sinh
  `fix44.rs` rồi `include!`. `dict` chưa có feature nào.
- Chỗ hardcode `FIX.4.4` ngoài session: chỉ ví dụ/rustdoc và test settings
  (`engine/src/settings.rs:19, :2545-2576`, `engine/src/block.rs:71`,
  `codec/src/template.rs:174`). `presession::is_logon` đọc `35=` bằng quét trường, không
  phụ thuộc phiên bản.
- `conformance::script::definitions_dir()` trỏ cứng `…/server/fix44`, và `load` **từ chối
  số file khác 59** (`script.rs:228-245`). `crates/session/tests/score.rs` dựng engine tối
  thiểu quanh `Session<Acceptor, …>`; `crates/engine/tests/wire.rs` chạy qua socket.
- Alloc bench có ở bốn crate: `codec`, `session`, `engine`, `library` (`benches/alloc.rs`),
  chạy bởi `scripts/bench.sh`, band theo `benches/baselines.tsv` (ADR-0016, ADR-0031).
- CI đã có tiền lệ toolchain ngoài chạy qua script: job `interop` build `libquickfix` bằng
  cmake/g++ (`.github/workflows/ci.yml:719-732`), không `build.rs` nào gọi nó.
- STATUS item mới nhất là **91**; item 33 ghi `docs/internals` hoãn sang phase 2; dòng
  `STATUS.md:2989` ghi phase 2 = "one opening ADR: the encoding trait, a dictionary chosen
  at `Logon` by the registry, and `docs/internals`".

**Trong `vendor/` (đo 2026-09-19):**

- `vendor/quickfix/spec/` có `FIXT11.xml`, `FIX50.xml`, `FIX50SP1.xml`, `FIX50SP2.xml`.
  `FIXT11.xml`: header 29 field + group `NoHops` (có `ApplVerID 1128`, `ApplExtID 1156`,
  `CstmApplVerID 1129`), 8 message admin (`0 1 2 3 4 5 A n`), field `1137 DefaultApplVerID`
  (bắt buộc trên Logon, dòng 90), `1130 RefApplVerID`. `FIX50SP2.xml`: **`<header />` và
  `<trailer />` rỗng** (dòng 2, 4674), 156 message, 15 267 dòng `<field`. Hai file cùng định
  nghĩa 1128/1129/1130/1137/1156.
- `vendor/quickfix/test/definitions/server/{fix50,fix50sp1,fix50sp2}/`: **60 file mỗi thư
  mục = 180**. `diff -r` cho thấy ba thư mục chỉ khác `TW50`/`TW50SP1`/`TW50SP2` và
  `1137=7`/`8`/`9`. So với `fix44` (59 file) chỉ **thêm một file**
  `1d_InvalidLogonNoDefaultApplVerID.def`: Logon `FIXT.1.1` không có `1137` → `eDISCONNECT`,
  không gửi gì. Mọi Logon `E` echo `1137=<giá trị>`. **Không file nào chứa `1128=`.** Các
  message ứng dụng trong corpus (`35=D`, `8`, `d`, …) dùng tag như FIX 4.4; text reject giữ
  nguyên (`Required tag missing`, `Incorrect BeginString`, 373 codes).
- Thư mục `future/` có 2 file (`14j`, `14k`), không thuộc phạm vi.

**Đặc tả và prior art (web, đọc 2026-09-19; nguồn đầy đủ ở *Sources* của ADR-0080/0081):**

- FIXT 1.1 so với FIX 4.4 session: OnixS ghi thay đổi "rất nhỏ": thêm `1128/1156/1129` vào
  header, `1137` bắt buộc trên Logon ("The Session Default Application Version must be
  specified at Logon time"), thứ tự ưu tiên *explicit (1128) > message-type default >
  session default (1137)*, và "the use of the Explicit Application Version fields is not
  permitted on FIX Session Level Messages". Seq num, resend, gap fill, reject: không đổi.
- QuickFIX C++ `Session.cpp`: `nextLogon()` lấy `1137` bằng `FIELD_GET_REF` khi
  `isFIXT()` (thiếu → ném, Logon hỏng → disconnect, khớp `.def`); `next()` chọn app
  dictionary từ `1128` của message, fallback `m_targetDefaultApplVerID`; `generateLogon()`
  đặt `1137=m_senderDefaultApplVerID`. QuickFIX/J: `DefaultApplVerID` "required only for
  FIXT 1.1", `TransportDataDictionary` + `AppDataDictionary`.
- Artio: `CodecGenerationTool` nhận "both the transport and data files" và sinh **một**
  codec cho FIXT (wiki *Codecs*).
- SBE 1.0 RC4: header composite 4 × `uint16` (`blockLength, templateId, schemaId, version`)
  = 8 byte (§2); root block rồi group rồi varData (§3.5); `groupSizeEncoding` =
  `blockLength u16, numInGroup u16` (§3.4.5); không padding trừ khi schema khai `offset`;
  template lạ → nhảy bằng `blockLength` (§3.6). `07Examples.md` có **ba hex dump byte-exact
  kèm offset**: `NewOrderSingle` (blockLength 54, template 99, schema 100), `ExecutionReport`
  có group `FillsGrp` (42, 98), `BusinessMessageReject` có varData (9, 100).
- `sbe-tool` (Real Logic/Aeron, **Apache-2.0**): có target Rust
  (`generation/rust/RustGenerator.java` …), README: output "100% safe rust crates … do not
  have any dependencies on any libraries"; chạy `java -Dsbe.target.language=Rust -jar
  sbe-all-<v>.jar schema.xml`; Maven `uk.co.real-logic:sbe-tool`. Ví dụ chuẩn
  `sbe-samples/src/main/resources/example-schema.xml` (`Car`, `byteOrder=littleEndian`,
  composite, enum, set, group lồng, 3 varData). Là **Java** — không được gọi từ `build.rs`
  (bất biến 6).
- crates.io: `sbe`, `sbe_gen` (sinh code trên `zerocopy` — một dependency runtime),
  `sbe-schema`. Không cái nào zero-dep với view mượn; **không dùng**.
- **Tìm 2026-09-19 không thấy** venue nào chở SBE trong session FIXT tag=value (ADR-0078) và
  không thấy corpus `.def` nào cho `1128` — nên hai quy tắc ở ADR-0080 quyết định 3 là
  quyết định, không phải sự thật đo được.

## Cách làm

Bốn PR, **gộp theo thứ tự A → B → C → D**, vì mỗi PR sau xây trên kiểu của PR trước. Nhưng
không phải bước nào cũng phải chờ: cột *Phụ thuộc* nói bước nào chờ bước nào thật sự, và mục
*Chạy song song* nói phiên nào làm phần nào cùng lúc. Mọi bước trong `crates/` đều nêu file,
test, gate và tier; tier `opus` cho bước đụng đường nóng hoặc bất biến.

### PR A — trait `Encoding`, `MessageView` không đổi (ADR-0079 quyết định 1–3, 5)

`crates/codec/src/encoding.rs` — **đã xây, commit `44df719`**; đoạn dưới chép đúng chữ ký
trong file đó (bản phác cũ nằm ngay sau, kèm lý do từng chỗ lệch):

```rust
pub trait Encoding {
    /// Kiểu view mượn; `MessageView<'a, N>` cho tag=value. `Copy`, 24 byte (assert ở lib.rs).
    type View<'a>: Copy;
    /// Định danh trường: `u32` (tag) cho tag=value; id trường cho SBE.
    type Field: Copy;
    /// Bảng tra (ADR-0080). Ràng buộc là `codec::Dictionary` — KHÔNG phải `dict::Tables`,
    /// vì `codec` không phụ thuộc `dict`; `Session` tự thêm `where E::Dict: Tables` (A2).
    type Dict: Dictionary;
    /// Vùng nhớ chủ gọi giữ cho một kết nối: `FieldIndex<N>` cho tag=value.
    type Scratch: Default;
    /// Khung gửi đi có lỗ (D9); P, S là sức chứa do chủ gọi chọn, y như `Template<P, S>`.
    type Template<const P: usize, const S: usize>;
    type ParseError: Copy;
    type EncodeError: Copy;

    fn parse(buf: &[u8], scratch: &mut Self::Scratch, v: Validation)
        -> Result<Parsed, Self::ParseError>;
    /// Mượn `buf` qua `scratch` — tách khỏi `parse`, gọi được cả sau `Err` (xem lệch 1).
    fn view<'a>(scratch: &'a Self::Scratch, buf: &'a [u8]) -> Self::View<'a>;
    fn field<'a>(view: Self::View<'a>, f: Self::Field) -> Option<&'a [u8]>;
    /// Các trường session đọc: `8`, `35`, `34`, `49`, `56`, `52`, `43`, `122`; SBE trả `None`.
    fn session_fields<'a>(view: Self::View<'a>) -> Option<SessionFields<'a>>;
    /// Ghi một message: byte cố định của template, `slots` lấp lỗ. Trả về KHOẢNG `out` bị
    /// chiếm, không phải độ dài — prefix canh phải nên message không bắt đầu ở `out[0]`.
    fn encode<const P: usize, const S: usize>(
        t: &Self::Template<P, S>, out: &mut [u8], slots: &[(Self::Field, &[u8])],
    ) -> Result<Range<usize>, Self::EncodeError>;
}

pub struct TagValue<D, const N: usize>(PhantomData<D>);
pub type Fix44TagValue = TagValue<fixbolt_dict::Fix44, 64>;   // alias sống ở `dict` (A2)
```

`impl<D: Dictionary, const N: usize> Encoding for TagValue<D, N>` chỉ **gọi lại** hàm có sẵn,
mọi hàm `#[inline]`: `parse` → `parse_into::<D, N>`, `view` → `FieldIndex::view`, `field` →
`MessageView::get`, `session_fields` → tám lần `get`, `encode` → `Template::encode_with::<D>`
với group rỗng. `View = MessageView<'a, N>`, `Field = u32`, `Scratch = FieldIndex<N>`,
`Template<P, S> = Template<P, S>`, hai kiểu lỗi = `ParseError`, `EncodeError` của `codec`.
`Session` đổi thành `Session<E: Encoding, R: Role, const APP: usize>` (N nằm trong `E`),
engine/library thêm alias `AcceptorFix44 = Session<Fix44TagValue, Acceptor>` để chữ ký `serve*`
đọc như cũ. `parse_into`, `MessageView`, `FieldIndex`, `Parsed` **không đổi một ký tự** (test
`api_unchanged` trong `crates/codec/tests/encoding.rs` khẳng định bằng `size_of` và bằng chữ ký
gọi cũ compile). Nhóm lặp (repeating group) **cố ý không có** trên trait: bốn thao tác của
ADR-0079 không gồm ghi group, và `GroupData` là hình dạng tag=value, không có nghĩa dưới SBE —
ai ghi group thì dùng thẳng `Template::encode_with`.

**Ba chỗ bản phác cũ lệch mã** (bản phác là phác thảo; hàng A1 mới là spec, và hàng A1 đòi
`parse_into`, `MessageView`, `FieldIndex` không đổi, `TagValue` chỉ *gọi lại* chúng):

1. *Phác cũ:* `parse` trả `Parsed<Self::View<'a>>` — view nằm trong kết quả `Ok`.
   *Mã:* `parse` trả `Parsed` **có sẵn** (enum `Complete { consumed } | Incomplete`,
   `parse.rs:36`), và `view(scratch, buf)` là hàm riêng. *Vì sao:* làm `Parsed` generic là đổi
   kiểu trả về của `parse_into` — public API của `codec`, hàng A1 cấm. Và tách ra là cần
   thật: sau `ParseError::BadTag`, index vẫn giữ mọi trường đọc *trước* tag hỏng, session
   dựng view trên đó để đọc `34=`/`35=` mà trả lời (`session/src/lib.rs:2994-3010`, định
   nghĩa `14a_BadField`). View chỉ có trong nhánh `Ok` không làm được việc đó.
2. *Phác cũ:* `patch(t: &mut Template, f, value)` rồi `encode(t, out) -> usize`.
   *Mã:* một hàm `encode(t: &Template<P, S>, out, slots) -> Range<usize>`. *Vì sao:*
   `Template<P, S>` **bất biến** theo D9 — dựng một lần, lỗ lấp bằng `slots` lúc gửi;
   `template.rs` không có `patch` (chỉ `new/field/slot/group/build/encode/encode_with`). Chữ
   ký này khớp 1:1 call site A2 sẽ thay: `template.encode(buf, &slots[..n])` ở
   `session/src/lib.rs:2788`, và trả `Range` chứ không phải `usize` vì đó là hợp đồng của
   `Template::encode` (prefix canh phải, `emit(&buf[range])`).
3. *Phác cũ:* một `type Error: Copy` chung. *Mã:* `ParseError` và `EncodeError` riêng, bằng
   đúng hai kiểu `codec` đang trả. *Vì sao:* `match` của session trên `ParseError::BadTag`,
   `Incomplete`… (`:2988-2994`) giữ nguyên ở A2, không phải bọc/mở thêm một lớp.

Hai chi tiết nhỏ cùng loại: `type View<'a>: Copy` bỏ `where Self: 'a` (marker không có
lifetime, không cần); `type Template` thành GAT có tham số const `<P, S>` để `out.rs` gọi
`E::Template<24, 320>` (`Skeleton`) mà không mất sức chứa do chủ gọi chọn.

**Hệ quả cho A2 và C4 (sửa lần 3, ADR-0082):** `Encoding::Dict` chỉ đòi `codec::Dictionary`,
nên `Session` tự viết `where E::Dict: Tables` để gọi pass validate qua `<E::Dict as Tables>::…`.
Nhưng `Tables` **không phải ràng buộc duy nhất** `Session` đặt thêm: session cần đi bộ view
theo thứ tự wire, dựng bảy khung tag=value, và match `ParseError::BadTag`, nên `impl Session`
mang **năm đẳng thức** (`View = MessageView<N>`, `Scratch = FieldIndex<N>`,
`Template<24,320> = Template<24,320>`, `Field = u32`, `ParseError = ParseError`) cộng
`E::Dict: Tables`. Đọc gọn: **`Session<E>` generic trên encoding tag=value** — mọi
`TagValue<D, N>` thoả, FIXT 1.1 ở PR B thoả; `Sbe<S>` không thoả năm đẳng thức nên
`Session<Sbe<S>>` không compile *trước khi* hỏi tới `Tables`. Vì vậy `SbeTables<S>` (C4)
**chỉ** impl `codec::Dictionary`; impl `dict::Tables` là thoả một ràng buộc không bao giờ tới
được, và kéo `dict` vào `sbe` vô cớ. Trait **không** mở rộng để xoá đẳng thức — lý do ở ADR-0082
*The alternative, not chosen*.

Gate đóng PR A là band ADR-0031 trên bàn §9: `benches/parse.rs`, `serialize.rs`, `alloc.rs`
của `codec`, `validate.rs` của `session`, `density.rs` của `engine` — cùng commit, cùng boot,
trước SBE. **Chỉ chạy khi bàn không còn phiên bisect C-91b** (memory: một phiên khác đang đo
thì ở đây không `cargo`).

### PR B — FIXT 1.1 / FIX 5.0 SP2 (ADR-0080)

1. `dict::Tables` trait (`crates/dict/src/tables.rs`, mới) gom đúng các hàm validate session
   gọi; `Fix44: Tables` uỷ quyền cho hàm sinh sẵn. Ràng buộc nằm ở **`Session`**
   (`where E::Dict: Tables`, A2), **không** nằm trên trait — `Encoding::Dict` chỉ đòi
   `codec::Dictionary` vì `codec` không phụ thuộc `dict` (sửa 2026-09-19 lần 2).
2. `dict/build.rs` nhận **cặp** XML sau feature `fix50sp2`: header/trailer/admin từ
   `FIXT11.xml`, field/component/group/message từ `FIX50SP2.xml`; trùng field phải giống
   nhau về số–tên–kiểu, khác → `die`. Sinh `fixt11_fix50sp2.rs`, `include!` sau
   `#[cfg(feature = "fix50sp2")]`. Biến môi trường ghi đè `NANOFIX_FIXT11_XML`,
   `NANOFIX_FIX50SP2_XML` theo mẫu `NANOFIX_FIX44_XML`.
3. `conformance::script::load` nhận `Corpus { dir, expected, comp_id, default_appl_ver_id }`;
   `definitions_dir()` giữ nguyên cho 59; thêm `fixt_corpora()` trả ba corpus 60 file.
4. `session`: `Config` thêm `default_appl_ver_id: Option<Name<…>>`; khi `begin_string ==
   b"FIXT.1.1"`: Logon thiếu `1137` → `DropReason::LogonWithoutDefaultApplVerID`; Logon gửi đi
   có `1137`; lưu `1137` của đối tác; `1128` ngoài họ `7/8/9` → `Reject 373=5 371=1128`.
   `session/tests/score_fixt.rs`: **180 / 180** trong process (3 corpus × 60);
   `engine/tests/wire_fixt.rs`: 60 / 60 qua socket cho `fix50sp2`.
5. `engine::settings`: key `DefaultApplVerID` (bắt buộc khi `BeginString=FIXT.1.1`, bị từ chối
   khi không), `Problem` mới kèm số dòng.
6. Alloc bench: case `parse NewOrderSingle (FIXT)`, `validate NewOrderSingle (FIXT tables)`,
   `encode Logon with 1137` đọc 0; timing bench `validate NewOrderSingle (FIXT tables)` in
   `NO BASELINE` tới khi ghi ở bàn.
7. Interop: `scripts/interop.sh` thêm arm `FIXT` (cấu hình `libquickfix`
   `BeginString=FIXT.1.1`, `DefaultApplVerID=FIX.5.0SP2`, `TransportDataDictionary`,
   `AppDataDictionary`) cho **acceptor** trước, 7 / 7 cùng kịch bản.

#### PR B — ba hàng thêm 2026-09-19 sau senior review, chờ duyệt (ADR-0085)

> **Vì sao có mục này.** Senior review của PR B tìm ra một lỗi mà cách sửa là một quyết định
> thiết kế, nên theo `CLAUDE.md` §1 (*kế hoạch sai giữa chừng → dừng, sửa kế hoạch, duyệt lại*)
> PR B **dừng ở đây** cho tới khi
> [ADR-0085](../decisions/ADR-0085-a-member-waits-for-a-counter-that-came-before-it-and-the-array-is-only-a-cache.md)
> được duyệt (chủ, hoặc manager theo uỷ quyền 2026-09-19 như đã làm với ADR-0084).
>
> **Lỗi, nói bằng lời thường.** Hàng B4b (ADR-0084 quyết định 2) làm cho giá trị của một trường
> *nằm trong nhóm lặp* được hỏi **sau** khi đã kiểm "thiếu trường bắt buộc" (`373=1`) và "đếm
> nhóm sai" (`373=16`). Để biết một trường có nằm trong nhóm hay không, máy quét ghi lại các
> trường đếm (`NoXXX`) nó **đã đi qua** vào một mảng 32 ô trên stack (`SeenCounters`). Khi
> mảng đầy, mã hiện tại chuyển sang hỏi một hàm khác (`in_a_group`) — hàm này nhìn **cả
> message**, kể cả những trường đếm nằm **sau** trường đang xét. Hai câu hỏi ấy khác nhau,
> nên cùng một message có thể nhận hai mã lý do Reject khác nhau tuỳ mảng đầy hay chưa.
> FIX 4.4 không bao giờ làm đầy mảng (nhiều nhất 23 trường đếm trên một loại message);
> FIXT/SP2 thì có (`TradeCaptureReport` khai 393, riêng cấp trên cùng đã 48). **Không test
> nào trong repo chạm tới nhánh "mảng đầy"** — đặt `panic!` vào đó, chạy hết test, không nổ.
>
> **Quyết định của ADR-0085, một câu.** Một trường được hoãn hỏi giá trị **khi và chỉ khi**
> trường đếm của nhóm nó thuộc về đã xuất hiện **trước nó trên dây**; khi mảng đầy, hàm thay
> thế phải hỏi **đúng câu ấy** (chỉ nhìn `0..i`), nên mảng chỉ còn là bộ nhớ đệm và 32 chỉ
> là con số về chi phí. Mảng giữ nguyên 32. Nhánh `373=13` **không** đổi.

| Bước | Kết quả | File đụng (không đụng gì khác) | Gate (lệnh) | Tier | Phụ thuộc |
|---|---|---|---|---|---|
| **B4c** | Sửa hàm thay thế khi mảng đầy theo ADR-0085 quyết định 1–2: thêm `in_a_group_before(view, msg_type, tag, upto)` (quét `0..upto`), `SeenCounters::defers` khi `full` gọi hàm này với `upto` = chỉ số trường đang xét (trong `scan_fields`; còn trong `scan_group_members` gọi với `upto = view.len()`, vì ở pass bốn mảng đã chứa **mọi** trường đếm của message nên hàm thay thế cũng phải nhìn mọi trường đếm — hai chế độ cùng hỏi một tập, ADR-0085 quyết định 2 và 4); `in_a_group` giữ nguyên cho nhánh `373=13`. Viết lại rustdoc của `SeenCounters` (`lib.rs:3999-4026`): bỏ câu *"the answer never depends on the capacity"* và câu *"no `alloc.rs` case sends a populated group"* (đã cũ — B6 đã thêm case `validate NewOrderSingle (populated group)`), thay bằng luật của ADR-0085 quyết định 1 và tên hai test dưới đây. **Test 1** (unit, trong mô-đun `#[cfg(test)] mod tests` của `lib.rs` — **hiện chưa có, tạo mới**, đặt cuối file; test này sau `#[cfg(feature = "fix50sp2")]`; nằm trong lib crate nên chịu đủ lint của bất biến 7: không `unwrap`/`expect`/`panic!`, không `a[i]`, chỉ `assert!`/`assert_eq!` và `match`; lý do phải là unit test: `SeenCounters::full` là private): parse một `35=AE` trên bảng `Fixt11Fix50Sp2Tables` gồm 33 nhóm cấp trên cùng khác nhau, mỗi nhóm `NoX=1\|<delimiter>=<giá trị hợp lệ>\|` — lấy 32 nhóm có delimiter *không* liệt kê giá trị (bảng 48 nhóm trong ADR-0085 *Sources*) cộng `552=1\|54=1\|`; gọi `scan_fields`, assert `seen.full == true`. **Test 2** (integration, `crates/session/tests/fixt.rs`, dùng `fixt_acceptor()` và `msg()` sẵn có ở `:26-46`): cùng 33 nhóm ấy, trong đó `1907=2\|1903=X\|` (khai 2, gửi 1 → `373=16`), rồi **sau nhóm thứ 33** đặt `447=ZZ` (thành viên của `453`, giá trị bảng từ chối), rồi `552=1\|54=1\|453=1\|448=A\|447=D\|452=1\|`; kỳ vọng đúng **một** Reject `373=5` và `371=447`. **Đảo chiều, câu FAIL viết trước khi chạy**: đặt lại `in_a_group` vào nhánh `full` → Test 2 đỏ với `expected 373=5 371=447, engine sent 373=16 371=1907`; khôi phục → xanh. **Test 3** (song sinh, cùng file): message của Test 2 với `1907=1` và `447=D` thay `447=ZZ` → không Reject, `Link::Up`, `next_in()` tăng — chứng minh hình dạng message tự nó hợp lệ, không phải xanh vì lỗi khác. **Test 4** (unit, cạnh Test 1, không cần feature): gấp `fixbolt_dict::GROUP_KEYS` theo `msg_type`, assert số trường đếm lớn nhất của `Fix44` `<= SeenCounters::SEEN` (in ra con số, kỳ vọng 23); sau feature, in con số của `fixt11_fix50sp2::GROUP_KEYS` (kỳ vọng 393) và assert nó `> SEEN` — để ghi bằng test rằng nhánh thay thế **sống** trên bảng ấy. Đọc trước: ADR-0085 *Decision* 1–2, 4–5 và *Sources* (bảng 48 nhóm); `lib.rs:3999-4089`, `:4158-4196`, `:4217-4247`, `:4301-4316`; `tests/fixt.rs:17-60`; `tests/group_member_values.rs:222-300` (mẫu assert) | `crates/session/src/lib.rs` (chỉ `SeenCounters`, `defers`, hàm mới, rustdoc, mô-đun test), `crates/session/tests/fixt.rs`. **Không đụng** `scan_group_members` ngoài đối số `upto`, không đụng nhánh `373=13`, không đụng `crates/dict/`, không đụng test nào đang có | `cargo test -p fixbolt-session --features fix50sp2` xanh, bốn test mới có tên trong output; `cargo test -p fixbolt-session --test score` **59 / 59**; `--features fix50sp2 --test score_fixt` in `fix50 59/60 fix50sp1 60/60 fix50sp2 60/60` không đổi (assert lệch `21` vẫn đúng nội dung); `--test group_member_values` xanh **không sửa**; đảo chiều đúng câu FAIL đã viết; clippy `-D warnings`; `scripts/check-indexing-debt.sh` không tăng; `cargo doc -p fixbolt-session` không warning | **opus** (senior developer — bất biến 1, 2, 3; đường validate) | **ADR-0085 duyệt** |
| **B4d** | Bench: `crates/session/benches/alloc.rs` case `validate TradeCaptureReport (33 groups)` sau `#[cfg(feature = "fix50sp2")]`, cùng bytes với Test 3 của B4c (message hợp lệ), đọc **0**; ba assert sống theo mẫu case `validate NewOrderSingle (populated group)` (`alloc.rs:660-760`): (1) `view.group::<Fixt11Fix50Sp2Tables>(b"AE", 1907).count() == 1` và cùng thế với `552`, (2) `validate(...) == None` trên cả message, (3) bản hỏng `447=ZZ` ở vị trí sau nhóm 33 trả `Some(ValueIsIncorrect)` — cái (3) là cái chứng minh nhánh `full` đã chạy trong bench chứ không chỉ trong test. `crates/session/benches/validate.rs` thêm case timing cùng tên, in `NO BASELINE` trên máy cloud (không ghi baseline; bàn §9 ghi sau, không chặn PR). Đọc trước: `alloc.rs:660-760`, `:840-860` (chỗ in tên case); `benches/validate.rs` case `validate NewOrderSingle (FIXT tables)`; `scripts/check-bench-alignment.sh` | `crates/session/benches/alloc.rs`, `crates/session/benches/validate.rs`, `scripts/check-bench-alignment.sh` (chỉ nếu script liệt kê tên case) | `scripts/bench.sh` với feature: case mới đọc `0`, `invariant failures 0`; `scripts/check-bench-alignment.sh` xanh; tập binary build **trùng** tập read-back (so bằng `comm`, như B6) | sonnet | B4c |
| **B4e** | Docs: `docs/SESSION-BEHAVIOUR.md` — B4b **chưa từng viết** dòng hành vi của nó (kiểm 2026-09-19: bảng chỉ có dòng ADR-0084 quyết định 1), nên viết **một** dòng cho cả hai: *giá trị `373=5`/`373=6` của thành viên nhóm được hỏi sau `373=1` và `373=16`, "thành viên" nghĩa là trường đếm đã xuất hiện trước nó; message làm đầy mảng 32 nhận cùng câu trả lời* — canh bởi `tests/group_member_values.rs` và Test 2 của B4c, trỏ ADR-0084 quyết định 2 và ADR-0085; `CHANGELOG.md` sửa mục *A group member's value is checked after its counter agrees* (`:37-39`) thêm nửa câu "counter đứng trước" và tên ADR-0085; ADR-0085 *Status* → Accepted kèm số đo của B4c/B4d (câu FAIL đảo chiều, con số 23 / 393 Test 4 in ra, số `0` của case alloc); ADR-0084 **chỉ** thêm vào dòng *Status* chữ "amended by ADR-0085" (§5 cho phép đổi trạng thái, không đổi nội dung); `docs/reference/a-fallback-that-answers-a-different-question.md` điền tên test hồi quy đúng như B4c đặt; `STATUS.md` mục *Not proven*: gạch "nhánh mảng đầy chưa test nào chạm" nếu có ghi, và thêm mục mở "chi phí message > 32 trường đếm chưa đo trên bàn §9". Đọc trước: ADR-0085 *Consequences* (đoạn "what a counterparty observes" là nguồn của dòng SESSION-BEHAVIOUR); `SESSION-BEHAVIOUR.md:485-500`; `CHANGELOG.md:30-40` | các file trên; `docs/reference/a-fallback-that-answers-a-different-question.md` (một dòng tên test) | `python3 scripts/check-links.py` không link chết; bảng đồng bộ §4 đi từng dòng và nói ra đã đi | manager | B4d, CI xanh trên commit đóng B4d |

Ba hàng này đứng **sau** B8 trong thứ tự giao và **trước** senior review lại + gộp: B8 đã giao
docs của PR B, nhưng dòng SESSION-BEHAVIOUR của B4b thiếu — B4e trả nợ đó luôn. Không hàng nào
cần bàn §9.

### PR C — SBE dưới trait (ADR-0079 quyết định 1, 4; ADR-0081)

1. `crates/sbe` (mới, L1, `#![no_std]`, zero-dep): `header.rs` (decode 8 byte, hai byte
   order), `view.rs` (`SbeView` 24 byte, `Copy`, `const _: () = assert!`), `group.rs`
   (cursor `groupSizeEncoding`, lồng nhau), `vardata.rs`, `schema.rs` (trait `Schema` đọc
   bảng `&'static`), `encoding.rs` (`Sbe<S>: Encoding`, `session_fields` trả `None`).
2. `crates/sbe-gen` (mới): `generate(xml) -> Result<String, Error>`, `roxmltree`; sinh bảng
   theo ADR-0081 quyết định 2; hỗ trợ phạm vi quyết định 5; ngoài phạm vi → `Error::Unsupported`
   có tên phần tử, không bao giờ sinh bảng sai lặng lẽ.
3. `scripts/fetch-sbe-assets.sh`: clone `fix-simple-binary-encoding` và
   `simple-binary-encoding` ở SHA ghim vào `vendor/sbe-spec/`, `vendor/sbe-ref/` (gitignore).
   `crates/sbe/build.rs` (dev-only, sau `[dev-dependencies] sbe-gen`) sinh bảng cho schema ví
   dụ của spec và `example-schema.xml` vào `OUT_DIR` — chỉ cho test, không vào lib.
4. `crates/sbe/tests/spec_examples.rs`: ba hex dump §7 decode từng trường, encode lại byte
   một; `tests/car_roundtrip.rs`: `Car` mọi kiểu trường, group lồng, ba varData, round trip.
5. `scripts/sbe-interop.sh` + job CI `sbe-interop` (Temurin 17): tải `sbe-all-<v>.jar` từ
   Maven Central (SHA-256 ghim), sinh crate Rust của Real Logic cho `example-schema.xml`
   vào `target/sbe-ref/`, `tools/sbe-interop` encode `Car` bằng của họ → decode bằng mình và
   ngược lại, so byte. Là "second implementation" theo ADR-0042.
6. `crates/sbe/benches/alloc.rs` (parse, field, group walk, encode: 0) và `benches/sbe.rs`
   (timing, `NO BASELINE` tới bàn §9).
7. `library`: alias `AcceptorSbe<S>` **không** có — SBE không có session (ADR-0078), nên cửa
   `serve*` không nhận `Sbe<S>`; `GUIDE.md` nói rõ SBE là codec để dùng với transport của
   người dùng, kèm ví dụ `examples/sbe_decode.rs`.

### PR D — `docs/internals`

Bản đồ mã cho người đóng góp (phần C của the-doc-set, STATUS item 33): một trang mỗi crate,
theo mục `DESIGN.md` §3 và bảng *What `engine` contains*, thêm `sbe`/`sbe-gen`. Không mô tả
hành vi (đã có ở DESIGN/GUIDE), chỉ "file nào giữ gì, đọc theo thứ tự nào".

## Bất biến bị đụng tới

Đụng `codec`, `session`, `engine`, `library`, thêm `sbe` — walk cả mười:

| # | Đụng? | Giữ bằng cách nào |
|---|---|---|
| 1 zero alloc | **có** — trait mới trên đường parse/serialize; bảng FIXT; toàn bộ `sbe` | case alloc mới cho **mỗi** đường nóng mới (A1, B6, C6); `bench.sh` trong job `bench` |
| 2 session thuần | **có** — `Session<E>` | không thêm I/O, clock, `format!`; `DropReason` mới fieldless; 59/59 + 180/180 chạy trên máy thuần |
| 3 59 .def | **có** | 59/59 trên `Fix44TagValue` là gate của A2; 180/180 là gate của B4 |
| 4 mode | không đụng wait strategy | A3 chỉ thêm tham số kiểu cho `serve*`; hai script mode chạy lại ở gate PR A |
| 5 thứ tự từ bảng sinh | **có** — bảng FIXT mới, bảng SBE mới | B2 test thứ tự header FIXT so với `FIXT11.xml`; C2 sinh offset từ schema, không call site |
| 6 feature gate `mod` | **có** — `fix50sp2` | `#[cfg]` trên `include!`; `build.rs` không đọc XML thứ hai khi feature tắt; `check-no-optional-deps.sh` thêm crate `sbe` |
| 7 no unwrap/panic | có — crate mới | lint workspace; `check-lint-config.sh`; `sbe` không có `unsafe` |
| 8 unsafe | không — `sbe` cấm `unsafe` | `#![forbid(unsafe_code)]` trong `sbe` |
| 9 không copy QuickFIX | có — thêm asset ngoài | `vendor/sbe-*` gitignore, fetch bằng script, ghim SHA; hex dump spec chép thành số trong test (sự thật, không phải văn bản) |
| 10 số đo có máy | **có** | band ADR-0031 ở gate PR A trên bàn §9; SBE baseline ghi ở bàn, tới lúc đó `NO BASELINE` |

## Chia việc

Mỗi dòng đủ để viết brief theo `CLAUDE.md` §12. Cột *Đọc trước* là đúng chỗ. Dòng
**needs-desk** cần bàn §9 và **không chạy khi phiên bisect C-91b còn đo**.

| Bước | Kết quả | File đụng (không đụng gì khác) | Gate (lệnh) | Tier | Phụ thuộc |
|---|---|---|---|---|---|
| **A1** | `codec::encoding`: trait `Encoding`, `SessionFields<'a>`, `TagValue<D, N>` impl gọi lại API cũ, mọi hàm `#[inline]`; `const _: () = assert!(size_of::<MessageView<64>>() == 24)` giữ nguyên; test `tests/encoding.rs`: `field` qua trait bằng `MessageView::get` trên 5 message thật của `tests/common`; `api_unchanged` compile chữ ký cũ. Alloc case `parse via Encoding` = 0. Đọc trước: ADR-0079 quyết định 1–2; ADR-0080 quyết định 1; `codec/src/index.rs` (`MessageView`, `get`), `parse.rs::parse_into`, `template.rs` (`patch`, `encode_with`) | `crates/codec/src/encoding.rs` (mới), `lib.rs` (một `pub mod` + `pub use`), `crates/codec/tests/encoding.rs` (mới), `crates/codec/benches/alloc.rs` (một case) | `cargo test -p fixbolt-codec` xanh; `cargo bench -p fixbolt-codec --bench alloc` in `parse via Encoding 0`; clippy `-D warnings`; `cargo doc -p fixbolt-codec` không warning | **opus** (đường nóng, bất biến 1) | ADR-0080 duyệt |
| **A2** | `Session<E: Encoding, R, APP>` **với `where E::Dict: Tables` trên `Session`** (trait chỉ đòi `codec::Dictionary`); 17 chỗ `Fix44::` → `<E::Dict as Tables>::`; `parse_into::<Fix44, N>` → `E::parse`, và `self.idx.view(bytes)` → `E::view(&self.idx, bytes)` (cả nhánh `BadTag` `:2994`); `out.rs` `Skeleton` = `E::Template<24, 320>`, `template.encode(buf, &slots[..n])` (`:2788`) → `E::encode(template, buf, &slots[..n])`; hai `match` trên `ParseError` giữ nguyên (`E::ParseError = ParseError`). `dict::Tables` trait + `impl Tables for Fix44` (uỷ quyền). Alias `Fix44TagValue` ở `dict`. Đọc trước: ADR-0080 quyết định 1; `session/src/lib.rs:2985`, `:3150`, `:3735-3826`, `out.rs` các chỗ `Fix44`; `score.rs:1-60` | `crates/session/src/lib.rs`, `out.rs`; `crates/dict/src/tables.rs` (mới), `lib.rs` (`mod`, alias); mọi test trong `crates/session/tests/` **chỉ đổi kiểu**, không đổi kỳ vọng | `cargo test -p fixbolt-session --test score` **59 / 59**; `cargo test -p fixbolt-session` xanh không sửa fixture; `cargo bench -p fixbolt-session --bench alloc` mọi case 0 | **opus** (bất biến 2, 3) | A1 |
| **A3** | `engine`, `library`, `conformance`, `tools/w2w`, `tools/interop`, `tools/jrnl` compile với `Session<E>`; alias `AcceptorFix44`, `InitiatorFix44` trong `engine`; `serve*` thêm `E` với default alias sao cho `examples/acceptor.rs` **không đổi**. Đọc trước: ADR-0079 *Consequences* dòng "Generic engines mean generic front doors"; `engine/src/lib.rs` các `serve*`; `library/src/app.rs`, `reply.rs` chỗ `Fix44` | `crates/engine/src/lib.rs`, `shard.rs`, `recovery.rs`, `reconnect.rs`, `block.rs`; `crates/library/src/*.rs`; `crates/conformance/src/echo.rs`; `tools/*/src/main.rs` (chỉ kiểu) | `cargo test --all` xanh; `cargo test --no-default-features` xanh; `scripts/check-no-kernel-sleep.sh` và `check-standard-gives-the-core-back.sh` xanh (bất biến 4 walk lại); `cargo test -p fixbolt-engine --test wire` 59 / 59; `scripts/interop.sh` 7 / 7 hai vai | sonnet xây → **opus review** | A2 |
| **A4** | Docs PR A: `DESIGN.md` §4 thêm **D16 — Encoding là trait, mỗi encoding một view, và `Session` generic trên tag=value** (nội dung = ADR-0079 quyết định 1–2, 5 + chữ ký A1 + **ADR-0082 quyết định 1–4**: nêu đủ năm đẳng thức `View`/`Scratch`/`Template<24,320>`/`Field`/`ParseError` và `E::Dict: Tables`, mỗi cái vì sao — chép từ rustdoc trên `impl Session` ở `crates/session/src/lib.rs` mục *What the session needs of an `Encoding`, beyond the trait* —; `N` bị ghim bởi đẳng thức chứ không phải tham số của `Session`; và câu **"`Session` is generic over tag=value encodings; `Session<Sbe<S>>` does not compile (ADR-0082)"** bằng đúng chữ ấy. **Không** chép câu "needs from a view only the session fields" của ADR-0079 quyết định 3 — ADR-0082 đã rút câu đó); ADR-0082 Status → Accepted cùng commit, sau khi chủ duyệt; §3 bảng `codec` thêm `encoding`; `GUIDE.md` mục alias; `CHANGELOG.md` *Unreleased* dòng public API `Session<E>`; `README.md` layout không đổi. **Thêm `DESIGN.md` §7 bước 9** (ngoại lệ §1, chủ duyệt 2026-09-19 — xem *Chạy song song*), nguyên văn tiếng Anh, chèn sau bước 8 và trước đoạn "TLS (D11) has no step here": `9. **\`sbe\` + \`sbe-gen\`**: the SBE codec under the \`Encoding\` trait (D16) and the generator that emits its layout tables from a schema. One step, like step 1, because the runtime is only usable with generated tables and the generator is only testable against the runtime. Under the phase 2 plan (chỗ này đặt một link chữ `plan` trỏ file này, đường dẫn tương đối từ `docs/` như bước 1), the one exception to "one crate per plan" in \`CLAUDE.md\` §1, decided by the owner on 2026-09-19. \`tools/sbe-interop\` sits beside it as \`tools/interop\` sits beside step 5.` Câu "All eight are complete as of 2026-09-02" đổi thành "Steps 1–8 are complete as of 2026-09-02; step 9 is in flight." | `docs/DESIGN.md` (§3, §4 D16, **§7 bước 9**), `docs/GUIDE.md`, `CHANGELOG.md`, `docs/GETTING-STARTED.md` (nếu chữ ký ví dụ đổi) | `python3 scripts/check-links.py` xanh; `grep -n '^9\. \*\*`sbe` + `sbe-gen`\*\*' docs/DESIGN.md` in đúng một dòng | manager (haiku link check) | A3 |
| **A-desk** *(needs-desk)* | Band ADR-0031 trên bàn §9, cùng boot, trước/sau commit A3: `scripts/bench.sh --strict` cho `parse serialize alloc validate density`, n = 20 mỗi bên, xen kẽ hai worktree (mẫu C-91); mọi case trong band → PR A đóng; lệch → **dừng**, sửa A1/A2, không ghi baseline mới | `docs/reference/measured-costs.md` (mục mới), `STATUS.md` (manager) | `scripts/bench.sh --strict` hai lần, quote; `scripts/check-machine.sh` `pass … fail 0` trong header | manager + haiku runner | A3; **bàn rảnh (không bisect)** |
| **B1** | `dict/build.rs`: hàm `generate_pair(transport, app)`; feature `fix50sp2`; `include!` sau `#[cfg]`; trùng field khác nhau → `die` có tên field; `Fixt11Fix50Sp2Tables: Dictionary + Tables`; test `crates/dict/tests/fixt.rs`: header đúng 29 field + `NoHops`, `is_msg_type(b"A")` và `is_msg_type(b"D")` đều true, `required(b"A")` chứa 1137, `is_defined_tag(1128)`, số message = 156 + 8, `field_type(1156) == Int`. Đọc trước: ADR-0080 quyết định 2; `dict/build.rs:24-60` (override, `die`), `:88` (`generate`), `:753` (`collect_header`); `FIXT11.xml:1-40, 85-95`; `FIX50SP2.xml:1-5` | `crates/dict/build.rs`, `Cargo.toml` (feature), `src/lib.rs` (`cfg` include), `crates/dict/tests/fixt.rs` (mới) | `cargo test -p fixbolt-dict --features fix50sp2` xanh; `cargo test -p fixbolt-dict` (feature tắt) xanh và **không đọc** `FIXT11.xml` (đổi tên file tạm → vẫn build); `scripts/check-no-optional-deps.sh` xanh; đo và ghi thời gian build hai trạng thái | sonnet | **Không chờ A** cho `build.rs`, feature, `include!` và test trên `Dictionary` (`dict` không dùng `Session`); riêng dòng `Fixt11Fix50Sp2Tables: Tables` cần trait `Tables` mà A2 tạo (`crates/dict/src/tables.rs`) → làm sau khi PR A gộp, lúc rebase. Cùng đụng `dict/src/lib.rs` với A2 — hai hunk khác chỗ, xem *Chạy song song* |
| **B2** | Thứ tự trường FIXT so với QuickFIX: nếu `vendor/quickfix-src` (do `interop.sh` clone) có `src/C++/fix50sp2/` sinh sẵn, mở rộng `crates/dict/tests/interop_quickfix_order.rs` theo mẫu 730/730 cho SP2; nếu không có, test so **header order** với thứ tự khai trong `FIXT11.xml` và ghi rõ giới hạn trong test doc. Đọc trước: `DESIGN.md` D3 đoạn "checked against QuickFIX's generated C++"; `crates/dict/tests/interop_quickfix_order.rs` | `crates/dict/tests/interop_quickfix_order.rs` hoặc `tests/fixt_order.rs` (mới) | `cargo test -p fixbolt-dict --features fix50sp2 order` xanh; đảo chiều: hoán vị hai member kề nhau trong một group SP2 → đỏ | sonnet | B1 |
| **B3** | `conformance::script`: `Corpus`, `load_corpus(&Corpus)`; `LoadError::WrongCount { expected, found }`; `fixt_corpora()` = 3 × `{dir, 60, comp_id, 1137}`; `echo.rs` generic `E`. Test `crates/conformance/tests/fixt_corpus.rs`: 180 file parse được, đúng 60 mỗi dir, mọi Logon `I` có `1137`, đúng một file mỗi dir thiếu `1137`. Đọc trước: `script.rs:215-260`; ADR-0080 *Context* đoạn oracle | `crates/conformance/src/script.rs`, `echo.rs`, `crates/conformance/tests/fixt_corpus.rs` (mới) | `cargo test -p fixbolt-conformance` xanh; `cargo test -p fixbolt-session --test score` vẫn 59 / 59 | sonnet | **Không chờ A** cho `script.rs`, `Corpus`, `fixt_corpora()` và test corpus (`conformance` chỉ phụ thuộc `codec` + `dict`, không có `Session`); riêng `echo.rs` generic `E` cần A1 và **A3 cũng sửa `echo.rs`** → phần đó làm sau khi PR A gộp |
| **B4** | Session FIXT (ADR-0080 quyết định 3): `Config::default_appl_ver_id`, `Config::acceptor_fixt(...)`; `DropReason::LogonWithoutDefaultApplVerID`; Logon gửi đi có `1137` (ordered by tables, không call site); `1128` policy; text.rs không thêm text mới. `tests/score_fixt.rs` (feature `fix50sp2`): **180 / 180**, mỗi corpus in số riêng; `tests/fixt.rs`: 4 unit test: thiếu 1137 → drop reason đúng và **không gửi byte nào**; `1137` khác → vẫn logged on; `1128=9` → validate thường; `1128=4` → `Reject 373=5 371=1128`; session `FIX.4.4` **không** emit 1137. Đọc trước: ADR-0080 quyết định 3–4; `1d_InvalidLogonNoDefaultApplVerID.def`; `session/src/lib.rs` chỗ `WrongBeginString` (`:1053`) và Logon reply; `score.rs` wrapper | `crates/session/src/lib.rs`, `out.rs`, `crates/session/tests/score_fixt.rs` (mới), `tests/fixt.rs` (mới), `Cargo.toml` (feature pass-through) | `cargo test -p fixbolt-session --features fix50sp2 --test score_fixt` in `fix50 60/60 fix50sp1 60/60 fix50sp2 60/60`; `--test score` 59 / 59 không đổi; đảo chiều: bỏ check `1137` → `1d_…NoDefaultApplVerID` đỏ, câu FAIL dự kiến `expected DISCONNECT, engine sent Logon`; `cargo bench -p fixbolt-session --bench alloc --features fix50sp2` 0 | **opus** (bất biến 2, 3) | B1, B3 |
| **B5** | `engine::settings`: key `DefaultApplVerID`; `Problem::DefaultApplVerIdRequired { line }` khi `BeginString=FIXT.1.1` mà thiếu, `Problem::DefaultApplVerIdWithoutFixt { line }` khi có mà BeginString là `FIX.4.x`; `Config` từ settings điền `default_appl_ver_id`. `engine/tests/wire_fixt.rs`: 60 / 60 `fix50sp2` qua socket (mẫu `wire.rs`). Đọc trước: ADR-0080 quyết định 4; ADR-0040; `settings.rs:90-130` (`BeginString`), test block `:2540-2580`; `engine/tests/wire.rs` phần dựng | `crates/engine/src/settings.rs`, `crates/engine/tests/wire_fixt.rs` (mới), `crates/engine/Cargo.toml` (feature) | `cargo test -p fixbolt-engine --features fix50sp2` xanh, `wire_fixt` in `60 / 60`; hai test `Problem` mới đỏ khi bỏ check | sonnet | B4 |
| **B6** | Bench: `codec/benches/alloc.rs` case `parse NewOrderSingle (FIXT)`; `session/benches/alloc.rs` case `validate NewOrderSingle (FIXT tables)`, `encode Logon (FIXT, 1137)`; `session/benches/validate.rs` case timing `validate NewOrderSingle (FIXT tables)` in `NO BASELINE`. Đọc trước: `session/benches/validate.rs` case `validate NewOrderSingle`; `benches/baselines.tsv` header | ba file bench, `scripts/check-bench-alignment.sh` (nếu liệt kê case) | `scripts/bench.sh` với feature: ba case 0; alignment xanh | sonnet | B4 |
| **B7** | `scripts/interop.sh` arm `FIXT`: cfg `libquickfix` `BeginString=FIXT.1.1 DefaultApplVerID=FIX.5.0SP2 TransportDataDictionary=…/FIXT11.xml AppDataDictionary=…/FIX50SP2.xml`; `tools/interop` cấu hình engine `FIXT.1.1`/`1137=9`; acceptor 7 / 7 trước, initiator 7 / 7 nếu không đụng D15; CI job `interop` chạy thêm arm. Đọc trước: `scripts/interop.sh:100-140`; `tools/interop/src/main.rs` phần cfg; ADR-0042 | `scripts/interop.sh`, `tools/interop/src/main.rs`, `.github/workflows/ci.yml` (job `interop`) | `scripts/interop.sh fixt` in `7 / 7` mỗi vai; quote | sonnet | B5 |
| **B8** | Docs PR B: `CONFIGURATION.md` key `DefaultApplVerID` + feature `fix50sp2`; `SESSION-BEHAVIOUR.md` mục FIXT (4 hành vi, `.def`/test canh, hai hành vi "không có oracle" gọi tên ADR-0080); `CONFORMANCE.md` bảng 180 / 180 + wire 60 / 60 kèm CI run id; `DESIGN.md` §3 `dict` thêm bảng thứ hai, D16 đoạn `Dict`; `PRD.md` §2 hàng FIX 5.0 → `[measured]`; `CHANGELOG.md` | các file trên | link check xanh | manager | B7, CI xanh |
| **C1** | `crates/sbe` runtime (ADR-0081 quyết định 1, 4, 5): `header.rs`, `view.rs` (`SbeView` 24 byte + `const _` assert), `group.rs`, `vardata.rs`, `schema.rs` (`trait Schema { fn message(template_id) -> Option<&'static MessageLayout>; … }`), `error.rs` (fieldless `SbeError`). `#![no_std]`, `#![forbid(unsafe_code)]`, zero dep. Unit test với bảng **viết tay** cho `NewOrderSingle` của spec §7 và hex dump của nó. Đọc trước: ADR-0081 quyết định 1–2, 4–5; SBE 1.0 RC4 §2, §3.3–3.6 (file `vendor/sbe-spec/v1-0-RC4/doc/03MessageStructure.md`); `codec/src/index.rs` (mẫu view + assert) | `crates/sbe/**` (mới), `Cargo.toml` workspace `members`, `scripts/check-no-optional-deps.sh` (thêm crate), `scripts/fetch-sbe-assets.sh` (mới, ghim SHA hai repo) | `cargo test -p fixbolt-sbe` xanh; `cargo test -p fixbolt-sbe --no-default-features`; clippy; `cargo build -p fixbolt-sbe --target thumbv7em-none-eabi` **hoặc** test `no_std` bằng `#![no_std]` + `cargo check` không `std` (chọn cái CI có sẵn, nêu rõ) | **opus** (crate mới trên đường nóng) | **Chỉ ADR-0081 duyệt** — C1 không gọi trait `Encoding` (đó là C4), nên không chờ A1. Đụng file chung: `Cargo.toml` `members`, `Cargo.lock`, `check-no-optional-deps.sh` — xem *Chạy song song* |
| **C2** | `crates/sbe-gen`: `generate(xml) -> Result<String, Error>`; sinh `impl Schema` + bảng `&'static`; phạm vi ADR-0081 quyết định 5, ngoài phạm vi → `Error::Unsupported(&'static str)`. Test: schema spec §7 và `example-schema.xml` sinh ra compile (test dùng `trybuild`-free: ghi vào `OUT_DIR` rồi `include!` trong `crates/sbe/build.rs` dev-only); offset từng trường của `Car` bằng số tính tay từ schema (ít nhất 8 trường, 2 group, 3 varData). Đọc trước: ADR-0081 quyết định 2; `dict/build.rs:88-330` (mẫu emit bảng); `vendor/sbe-ref/sbe-samples/src/main/resources/example-schema.xml` | `crates/sbe-gen/**` (mới), `crates/sbe/build.rs` (mới, chỉ khi `cfg(test)`-style qua dev-dep), `crates/sbe/tests/schemas/` (chỉ **đường dẫn** vào vendor, không copy XML) | `cargo test -p fixbolt-sbe-gen` xanh; `cargo test -p fixbolt-sbe` compile bảng sinh | sonnet | C1 |
| **C3** | `crates/sbe/tests/spec_examples.rs`: ba hex dump §7 decode → từng trường bằng số spec ghi → encode lại **byte một**; `tests/car_roundtrip.rs`: `Car` mọi kiểu, nested group, 3 varData; `tests/versioning.rs`: `sinceVersion` > header → absent; template lạ → skip đúng `blockLength`; message cụt → `Err`, không panic (fuzz nhỏ bằng cắt từng byte). Đọc trước: `07Examples.md` ba mục; ADR-0081 quyết định 3a, 4 | `crates/sbe/tests/*.rs` (mới) | `cargo test -p fixbolt-sbe` xanh; đảo chiều: đổi một byte trong hex `ExecutionReport` → test group đỏ ở đúng trường | sonnet | C2 |
| **C4** | `impl Encoding for Sbe<S: Schema>` (`crates/sbe/src/encoding.rs`): `View = SbeView`, `Field = FieldId(u16)`, `Dict = SbeTables<S>` — impl **chỉ** `codec::Dictionary` (ADR-0082 quyết định 4: `dict::Tables` là ràng buộc của `Session`, mà `Session<Sbe<S>>` đã không compile trên năm đẳng thức trước khi hỏi tới `Tables`; `sbe` **không** phụ thuộc `dict`), `session_fields` → `None`, `Template<P, S>` = root block + group builder + varData append (GAT có tham số const, bỏ qua `P, S` nếu không dùng), `encode(t, out, slots)` ghi theo offset (**không có `patch`** — xem *PR A* lệch 2), trả `Range<usize>`; hai kiểu lỗi riêng. Test: `field` qua trait bằng số của spec §7; encode `NewOrderSingle` qua `Template` == hex spec. Đọc trước: ADR-0079 quyết định 2, 4; ADR-0082 quyết định 3–4; A1 (`codec/src/encoding.rs`); ADR-0080 quyết định 1 | `crates/sbe/src/encoding.rs`, `tables.rs` (mới), `crates/sbe/tests/encoding.rs` (mới) | `cargo test -p fixbolt-sbe` xanh; `cargo bench -p fixbolt-sbe --bench alloc` (C6) | **opus** (trait trên đường nóng) | C3, **và PR A đã gộp** (A1 cho `Encoding`; **không còn cần A2** — sửa lần 3) |
| **C5** | `scripts/sbe-interop.sh` + `tools/sbe-interop` + job CI `sbe-interop` (ADR-0081 quyết định 3b): tải `sbe-all-<v>.jar` (SHA-256 ghim trong script), Temurin 17 cài trong job, sinh crate Rust Real Logic cho `example-schema.xml` vào `target/sbe-ref/`, binary encode bằng họ → decode mình, encode mình → decode họ, so byte hai chiều cho `Car` với group lồng và varData; in `sbe-interop: 2 / 2`. Đọc trước: `scripts/interop.sh:30-110` (fetch/ghim/build); `.github/workflows/ci.yml:719-800` (job `interop`); README SBE đoạn lệnh chạy | `scripts/sbe-interop.sh` (mới), `tools/sbe-interop/**` (mới), `.github/workflows/ci.yml` (job mới), `Cargo.toml` members | `scripts/sbe-interop.sh` in `2 / 2`; job xanh trong CI, run id ghi lại | sonnet | C4 (encode "mình" đi qua `Template` của C4). Đụng `ci.yml` và `Cargo.toml` `members` cùng B7/C1 — cùng một phiên sửa, xem *Chạy song song* |
| **C6** | `crates/sbe/benches/alloc.rs`: `parse Car`, `field`, `walk fuelFigures`, `encode NewOrderSingle` = 0, mỗi case chứng minh bằng injection (mẫu `codec/benches/alloc.rs:230-246`); `benches/sbe.rs` timing 4 case `NO BASELINE`; `scripts/bench.sh` và `check-bench-alignment.sh` biết crate mới. Đọc trước: `codec/benches/alloc.rs`, `harness.rs`; `scripts/bench.sh` phần INVARIANT/TIMING | `crates/sbe/benches/*.rs` (mới), `Cargo.toml` `[[bench]]`, `scripts/bench.sh`, `scripts/check-bench-alignment.sh` | `scripts/bench.sh` in bốn `0` và bốn `NO BASELINE`; alignment xanh | sonnet | C4 |
| **C7** | `library/examples/sbe_decode.rs` + `tests/sbe_example.rs`: decode một `Car` từ bytes cố định qua `fixbolt::sbe` re-export, không socket (ADR-0078: không session). `library` re-export `sbe` sau feature `sbe` (gate `mod`/`pub use`); một doctest `compile_fail` trên re-export ghi rằng `Session<Sbe<S>>` bị từ chối, đặt ở `library` vì đó là crate duy nhất thấy cả `session` lẫn `sbe` (ADR-0082 quyết định 4). Đọc trước: ADR-0082 quyết định 4; ADR-0078 *Consequences* "cannot log on anywhere"; `library/src/lib.rs` re-export list | `crates/library/src/lib.rs`, `Cargo.toml`, `examples/sbe_decode.rs`, `tests/sbe_example.rs` | `cargo test -p fixbolt --features sbe` xanh; `--no-default-features` không kéo `sbe` (`check-no-optional-deps.sh`) | sonnet | C4, **và PR A đã gộp** (A3 sửa `crates/library/src/*.rs`) |
| **C8** | Docs PR C: `DESIGN.md` §3 hai crate mới, §7 bước 9 chỉ đổi "in flight" → "complete as of <ngày>" (**không có bước 10** — cặp `sbe` + `sbe-gen` là một bước, A4 đã ghi), D16 đoạn SBE, §6 gate `sbe-interop`; `GUIDE.md` mục "SBE là codec, bạn mang transport"; `GETTING-STARTED.md` không đổi (FIX 4.4); `CONFIGURATION.md` feature `sbe`; `README.md` layout; `CHANGELOG.md`; `PRD.md` §2 hàng SBE `[measured]`, bảng *Phase 2 starts…* hàng SBE → trỏ ADR-0081; `docs/reference/sbe-spec-facts.md` (header, order, group dims, §3.6 — sự thật đã dùng) | các file trên | link check xanh | manager | C7, CI xanh |
| **C-desk** *(needs-desk)* | Baseline SBE trên bàn §9: `scripts/bench.sh` n = 20, ghi `benches/baselines.tsv` bốn case `sbe`; so `parse Car` với `parse NewOrderSingle` tag=value cùng boot; kết quả vào `measured-costs.md`, `DESIGN.md` §8 **chỉ** nếu có dòng budget đổi | `benches/baselines.tsv`, `docs/reference/measured-costs.md` | `scripts/bench.sh --strict` xanh sau khi ghi; `check-machine.sh` header | manager + haiku | C6; bàn rảnh |
| **D1** | `docs/internals/README.md` + một trang mỗi crate (`codec.md`, `dict.md`, `session.md`, `engine.md`, `library.md`, `conformance.md`, `sbe.md`, `tools.md`): file → giữ gì → đọc theo thứ tự nào → test canh; mỗi trang ≤ 80 dòng; trỏ DESIGN/ADR thay vì kể lại. Đọc trước: `DESIGN.md` §3 và bảng *What `engine` contains*; STATUS item 33 đoạn "engine-has-no-map" | `docs/internals/**` (mới), `README.md` (một link), `CLAUDE.md` §4 bảng (một hàng, manager) | link check xanh; `grep -c` mỗi trang ≤ 80 dòng | sonnet | **Tách hai đợt**: D1a (`README.md` của internals + `codec.md`, `dict.md`, `session.md`, `engine.md`, `library.md`, `conformance.md`, `tools.md`) viết ngay theo mã trên `main`, không chờ gì; D1b (`sbe.md`, và một lượt rà lại bảy trang kia cho `encoding.rs`, `tables.rs`, FIXT) sau C8 |
| **D2** | Đóng phase 2: `STATUS.md` (item 33 gạch phần internals; item mới cho hai hành vi FIXT không oracle và cho "no `1128` on session messages"; hàng Phase 2 `:2989`); `PRD.md` §2 *Phase 2* thêm bảng exit criteria đã đạt (180/180, 60/60 wire, spec 3/3, sbe-interop 2/2, band A-desk, alloc 0 × N); ADR-0080/0081 → *Accepted* với số đo | `STATUS.md`, `docs/PRD.md`, hai ADR | mọi CI run id nêu tên | manager | D1 |

## Chạy song song

*(Thêm 2026-09-19, chờ duyệt.)* Ba phiên chạy cùng lúc, mỗi phiên một nhánh, một PR, một
bộ file. Ranh giới rút từ file thật mỗi bước đụng, không từ tên PR.

**Điều đã kiểm:** `conformance` chỉ phụ thuộc `codec` + `dict` (`crates/conformance/Cargo.toml`),
không có `Session`; `dict` không có `Session`; C1–C3 chỉ tạo file mới trong `crates/sbe`,
`crates/sbe-gen`. Nhưng `dict::Tables` là **của A2** (`crates/dict/src/tables.rs`), và cả B1
(`Fixt11Fix50Sp2Tables: Tables`) lẫn C4 (`SbeTables: Tables`) đều cần nó; `echo.rs` bị A3 và
B3 cùng sửa; `library` bị A3 và C7 cùng sửa. Vì thế B và C mỗi bên chỉ chạy được **nửa đầu**
trước khi PR A gộp. `Cargo.lock` **có commit** (`git ls-files`) và CI dùng `--locked` (5 chỗ
trong `ci.yml`), nên nhánh nào thêm crate cũng phải commit lock đúng.

| Phiên | Máy | Bước làm được **trước** khi A gộp | Bước làm **sau** khi A gộp (rebase) | Nhánh / PR |
|---|---|---|---|---|
| **Bàn** | desk §9, Linux | A1 → A2 → A3 → A4 → A-desk | gộp A; rồi review + gộp B, C; D2; C-desk | `feat/phase-2-a`, PR A |
| **Cloud** | GitHub cloud, Linux | B1 (trừ `impl Tables`), B2, B3 (trừ `echo.rs`), D1a | B1 `impl Tables`, B3 `echo.rs`, B4 → B5 → B6 → B7 → B8 | `feat/phase-2-b`, PR B (draft từ commit đầu) |
| **Mac** | macOS, không có §9 | C1 → C2 → C3 | C4, C6, C7, C8, D1b; **C5 chuyển sang cloud** (cần Linux + Java, và `ci.yml`) | `feat/phase-2-c`, PR C (draft từ commit đầu) |

Nếu chỉ có hai phiên (bàn + cloud): cloud làm B trước, C sau, cùng một thứ tự; bảng file
chung không đổi.

**File chung — đúng một phiên được sửa, phiên khác chờ rồi rebase:**

| File | Ai sửa | Phiên khác làm gì |
|---|---|---|
| `Cargo.toml` `[workspace] members` | Mac (C1 thêm `crates/sbe`, `crates/sbe-gen`; C5 thêm `tools/sbe-interop`) | không đụng; PR B không thêm crate |
| `Cargo.lock` | phiên nào thêm crate hay dep (C1, C2, C5); B không thêm dep (`roxmltree` đã có trong lock) | khi rebase gặp xung đột: lấy bản của `main`, chạy `cargo check --workspace` (không `--locked`) để cargo bổ sung entry thiếu, commit lock đó; **không** chạy `cargo update` |
| `crates/dict/src/lib.rs` | A2 (`mod tables; pub use`) **và** B1 (`#[cfg] include!`) | hai hunk khác chỗ; B rebase sau A, xung đột nếu có tự giải quyết, không chờ |
| `crates/conformance/src/echo.rs` | A3 | B3 sửa phần generic sau khi A gộp |
| `crates/library/**` | A3 | C7 sau khi A gộp |
| `scripts/bench.sh`, `scripts/check-bench-alignment.sh` | B6 (case FIXT) rồi C6 (crate `sbe`) | C6 rebase lên B nếu B gộp trước; nếu C gộp trước thì ngược lại — ai gộp sau rebase |
| `scripts/check-no-optional-deps.sh` | C1 (thêm crate `sbe`) | B1 chỉ **chạy**, không sửa |
| `.github/workflows/ci.yml` | cloud: B7 (arm `interop`) và C5 (job `sbe-interop`) — C5 vì thế chuyển sang cloud | Mac không đụng `ci.yml` |
| `docs/DESIGN.md`, `docs/GUIDE.md`, `CHANGELOG.md`, `docs/PRD.md`, `docs/CONFIGURATION.md` | bước docs của **PR đang gộp** (A4, B8, C8), ngay trước khi gộp, đã rebase lên `main` | trước lúc đó mỗi phiên **không** sửa các file này; ghi những dòng cần thêm vào *Nhật ký giao hàng* của PR mình để bước docs chép sang |
| `STATUS.md`, `CLAUDE.md` | **chỉ bàn** (manager) | cloud/Mac không đụng; handoff của họ nằm trong PR body và *Nhật ký giao hàng* |
| `docs/plans/2026-09-19-phase-2-fixt-and-sbe.md` *Nhật ký giao hàng* | mỗi phiên **một mục con** riêng (`### PR A`, `### PR B`, `### PR C`) | không sửa mục của phiên khác |

**Thứ tự gộp:** A → B → C → D (D1a có thể gộp như một PR docs riêng bất cứ lúc nào). B và C
mỗi bên rebase lên `main` ngay sau khi A gộp, chạy lại gate của các bước đã làm (59/59 cho B,
`cargo test -p fixbolt-sbe` cho C) rồi mới tiếp nửa sau. PR nào gộp sau rebase lần nữa.

**Gate Mac không chạy được** (phải chờ CI hoặc cloud, và ghi "chưa chạy ở máy này" trong
báo cáo): `engine/tests/wire*.rs`, `scripts/check-no-kernel-sleep*.sh`,
`check-standard-gives-the-core-back.sh`, `tools/w2w`, `scripts/interop.sh`, `sbe-interop.sh`
(Java + Linux job), mọi số thời gian. Mac chạy được: `cargo test -p fixbolt-sbe`,
`-p fixbolt-sbe-gen`, `-p fixbolt-dict`, `-p fixbolt-conformance`, clippy, `cargo doc`,
alloc bench của `sbe` (đếm alloc, không đo giờ). Bench alloc của C6 xanh trên Mac vẫn phải
xanh lại trong job `bench` của CI trước khi C đóng.

**Ngoại lệ `CLAUDE.md` §1 — chủ đã quyết 2026-09-19.** §1 nói crate thêm vào workspace
**"một crate một lần, theo thứ tự `DESIGN.md` §7, mỗi crate một kế hoạch"**. Kế hoạch này
thêm **hai** crate (`sbe` ở C1, `sbe-gen` ở C2) dưới **một** kế hoạch. Chủ quyết: coi `sbe` +
`sbe-gen` là **một cặp, một bước** — bước 9 của `DESIGN.md` §7 — giống hệt bước 1 là
`codec` + `dict`. Lý do cặp là một đơn vị: C1 test được một mình bằng bảng viết tay, nhưng
`sbe` chỉ dùng được thật khi có bảng sinh, và C3–C4 test trên bảng do C2 sinh; tách ra thì
nửa nào cũng không đóng được. Để giữ ý "gate của bước có trước bước", **A4 ghi bước 9 vào §7
trước khi C1 bắt đầu** (câu chữ nằm trong hàng A4), C8 chỉ đổi trạng thái bước 9 thành xong;
**không có bước 10**. `tools/sbe-interop` (C5) không phải crate thư viện, cùng loại
`tools/interop` đứng cạnh bước 5, không cần bước riêng. Ngoại lệ này ghi cả ở *Nhật ký giao
hàng* để manager sau thấy; `CLAUDE.md` §1 **không sửa** — đây là một ngoại lệ có ghi, không
phải luật mới. Phiên Mac được bắt đầu C1 sau khi PR A gộp với A4 đã ghi bước 9, hoặc sớm
hơn trên nhánh riêng nếu chủ chấp nhận §7 đi sau vài ngày (chủ chưa nói — xem *Rủi ro*).

## Cách kiểm chứng

- **PR A** đóng bằng ba thứ, đủ cả ba: 59 / 59 in-process và qua socket **không sửa fixture**;
  hai script mode xanh (bất biến 4 walk lại vì `serve*` đổi chữ ký); **A-desk trong band
  ADR-0031** trên bàn §9, quote `bench.sh --strict` hai worktree. Lệch band là dừng, không
  phải ghi chú.
- **PR B** *(sửa 2026-09-19 theo ADR-0084 — viết lại, **không** hạ)*: `score_fixt` in
  `fix50 59/60 fix50sp1 60/60 fix50sp2 60/60` **và** khẳng định bằng nội dung rằng file duy nhất
  lệch là `21_RepeatingGroupSpecifierWithValueOfZero.def:17`, engine trả `Reject 373=5 371=336`
  (ADR-0084 quyết định 3); câu FAIL đảo chiều `1d_…` giữ nguyên. Số cũ là ba `60/60`; nó không
  đạt được vì `336` có 0 giá trị enum trong `FIX50.xml` và 7 trong `FIX50SP2.xml`, nên corpus
  `fix50` chấm bằng bảng SP2 hỏng đúng một file. `wire_fixt` 60 / 60 trên Linux CI. Interop
  `fixt` 7 / 7 — đây là ý kiến độc lập duy nhất (ADR-0042); `.def` và interop cùng xanh mới
  đóng. `dict` build có feature tắt **không mở** `FIXT11.xml` (đổi tên file rồi build).
  **Thêm:** CI phải thật sự chạy test sau feature — `grep fix50sp2 .github/workflows/ci.yml`
  hiện **không khớp gì**, nên bốn binary test FIXT chưa từng chạy trong CI (B7, và trang
  `a-feature-gated-test-is-a-test-ci-never-runs.md`).
- **PR C**: ba hex dump spec round trip byte một; `sbe-interop` 2 / 2 trong CI với Java do job
  cài; alloc `0` bốn case, chứng minh bằng injection; `no_std` chứng minh bằng build không
  `std` (C1 nêu cách CI có). Đảo chiều C3: đổi một byte trong hex group → đỏ đúng trường.
- **Mọi số thời gian** chỉ có sau A-desk/C-desk, trên bàn §9, header `check-machine.sh`, hai
  procedure theo ADR-0068 nếu công bố. Trước đó bench in `NO BASELINE`.

## Tài liệu phải cập nhật

Theo `CLAUDE.md` §4; đây là danh sách **manager sửa**, dòng chính xác:

- [ ] `docs/PRD.md` §2 *Phase 2* (dòng 94–108): bảng 3 hàng thêm cột *Plan / PR* trỏ kế hoạch
      này (A/B/C); sau D2, thêm bảng *Phase 2 exit criteria* dưới bảng *Phase 1 exit
      criteria* (sau dòng 148).
- [ ] `docs/PRD.md` §2 *Phase 2 starts with an architectural decision* (dòng 150–167): hàng
      **SBE** `[unproven]` → "decided: ADR-0078, ADR-0081"; hàng **FIX 5.0 / FIXT 1.1**
      `[unproven]` → "`[measured 2026-09-19]` 180 = 60 × 3, differ only in `1137` and CompID;
      ADR-0080"; hàng FAST/FIXML giữ, thêm "out of phase 2 (ADR-0078 d.4)".
- [ ] `docs/PRD.md` §6: hàng mới **11** "Where does the dictionary live once the session is
      generic? — Proposed answer ADR-0080"; **12** "Who generates SBE layouts? — Proposed
      answer ADR-0081".
- [ ] `docs/DESIGN.md` §3 (dòng 97–112): hàng `codec` thêm "`encoding`: the `Encoding` trait
      (D16)"; hàng `dict` thêm "second table `Fixt11Fix50Sp2Tables` behind `fix50sp2`"; hai
      hàng mới `sbe` (L1) và `sbe-gen` (build) sau `library`.
- [ ] `docs/DESIGN.md` §4: **D16** mới sau D15 (dòng 779–846): trait, alias, `Dict`, SBE view,
      static dispatch, **năm đẳng thức của `Session` và câu "generic over tag=value" (ADR-0082,
      xem hàng A4)**; §4 D2 thêm một câu "`SbeView` is the second 24-byte view (D16)".
- [ ] `docs/decisions/ADR-0082-…md`: Status Proposed → Accepted khi chủ duyệt, cùng commit A4.
- [ ] `docs/DESIGN.md` §6 *Correctness* (dòng 877–912): hàng 180 `.def` FIXT và hàng
      `sbe-interop`; §6 *Allocation*: các case mới.
- [ ] `docs/DESIGN.md` §7 (dòng 1098–1124): "All eight are complete" → thêm bước 9 `sbe`, 10
      `sbe-gen`, mỗi bước trỏ PR C.
- [ ] `STATUS.md`: *Start here* mới khi mở PR A; hàng Phase 2 (dòng 2989) → trỏ kế hoạch này;
      item 33 (dòng 5074) gạch phần `docs/internals` khi D1 xong; item mới ≥ 92 cho ba hành vi
      FIXT không oracle (ADR-0080 quyết định 3).
- [ ] `docs/CONFIGURATION.md` (dòng 81 vùng `BeginString`): key `DefaultApplVerID`; feature
      `fix50sp2`, `sbe`.
- [ ] `docs/SESSION-BEHAVIOUR.md`: mục FIXT 1.1, bốn hành vi, `.def`/test canh từng cái.
- [ ] `docs/CONFORMANCE.md`: 180 / 180, 60 / 60 wire, interop `fixt` 7 / 7, spec 3 / 3,
      sbe-interop 2 / 2 — lệnh, máy, CI run id.
- [ ] `docs/GUIDE.md`: alias `Fix44TagValue`/`AcceptorFix44`; mục SBE "codec only".
- [ ] `README.md` layout (dòng 132–140): hai crate mới; `CHANGELOG.md` *Unreleased*.
- [ ] `docs/reference/sbe-spec-facts.md` (mới, C8) và mục mới trong `measured-costs.md`
      (A-desk, C-desk).
- [ ] `CLAUDE.md` §2 bảng *Machine checks* hàng 3: thêm "180 FIXT `.def` behind `fix50sp2`";
      §7 bảng "Any session-layer change" → "59 + 180". (Manager, nói to rule đổi.)

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| GAT `type View<'a>` + `#[inline]` làm hàm generic không inline qua ranh giới crate → tag=value chậm | A-desk band; `benches/parse.rs` là số đầu tiên đọc |
| `Session<E>` monomorphise hai lần (Fix44 + FIXT) làm thời gian build `session` tăng; `dict` với hai bảng | B1 ghi thời gian build hai trạng thái; feature tắt mặc định |
| Field trùng giữa `FIXT11.xml` và `FIX50SP2.xml` khác enum → bảng sai lặng lẽ | B1: khác → `die`; test cố ý sửa một enum trong bản copy XML → build đỏ |
| Corpus loader vẫn từ chối ≠ 59 | B3 `LoadError::WrongCount`; test 60 mỗi dir |
| `1137` phải nằm đúng thứ tự trong Logon gửi đi (comparator positional, D3) | B4 `score_fixt` `1a_ValidLogonWithCorrectMsgSeqNum` là file đầu tiên đỏ nếu sai |
| `is_admin` cho FIXT lấy từ `msgcat` của `FIXT11.xml`, không từ `ADMIN` const trong session | B1 `is_admin(b"n")` (XMLnonFIX) true; B4 `8_OnlyAdminMessages` |
| SBE `blockLength` trong header **lớn hơn** tổng field (schema thêm padding) → đọc group sai offset | C3 test với bảng viết tay `blockLength` = tổng + 4; §3.3.1 |
| SBE `numInGroup` × `blockLength` vượt buffer → panic index | C3 fuzz cắt byte; `indexing_slicing = deny` |
| Byte order: schema `bigEndian` với header cùng byte order (§2.2) | C3 test `NewOrderSingle` hex spec là big-endian? — người xây **đọc hex** và khẳng định trong test doc, không đoán |
| `sbe-tool` phiên bản mới đổi output/CLI | C5 ghim version + SHA-256; job đỏ nói rõ "pin" |
| `build.rs` của `sbe` (dev-only) chạy khi user build crate → kéo `roxmltree` vào runtime | C2: `build.rs` chỉ sau `cfg`/env `FIXBOLT_SBE_GEN_TESTS`; `check-no-optional-deps.sh` |
| Java thiếu trên máy dev → `sbe-interop.sh` đỏ mập mờ | C5 script kiểm `java -version` đầu tiên, in hướng dẫn, exit 2 riêng |
| `1128` trên message session (cấm theo spec) không bị từ chối | ghi STATUS item, không xây; test doc B4 nói rõ |
| Bàn §9 đang bisect C-91b → số A-desk vô nghĩa | A-desk chỉ chạy khi `STATUS.md` *Start here* nói bisect xong; memory rule |

**Hàng `is_admin` ở trên: đóng 2026-09-20.**
[the-group-count-pass-and-is-admin](2026-09-20-the-group-count-pass-and-is-admin.md) xây nó —
`Tables::is_admin`, sinh từ `msgcat` — và
[ADR-0086](../decisions/ADR-0086-a-group-count-is-asked-at-every-depth-admin-is-two-questions-with-two-names-and-xmlnonfix-is-not-asked-the-appl-ver-id-rule.md)
quyết định 2 ghi rõ vì sao nó trả lời một câu hỏi khác với danh sách định tuyến của session
(nay là `SESSION_OWNED`).

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| A-desk lệch band → trait phải thiết kế lại (ví dụ bỏ GAT, dùng `Encoding<'a>`) | cao | dừng ở A; kiến trúc sư sửa ADR-0079 (còn được sửa? — **không**, đã Accepted → ADR mới); PR B/C không mở |
| `Session<E>` kéo theo `library` API đổi nhìn thấy được (`Handler` generic) | vừa | A3 dùng alias mặc định để `examples/acceptor.rs` không đổi; nếu không được → ghi CHANGELOG, GUIDE |
| Generator SBE gặp schema venue dùng `ref`/`offset` phức tạp | vừa | `Error::Unsupported` có tên; phạm vi ADR-0081 d.5; venue là phase 3 |
| Hai hành vi FIXT không oracle sai với một venue thật | thấp | ghi rõ ở SESSION-BEHAVIOUR + STATUS item; đổi bằng ADR mới |
| `interop` FIXT: `libquickfix` cần `DataDictionary` path tuyệt đối, cfg khác 4.4 | thấp | B7 copy cfg mẫu QuickFIX/J; nếu initiator đụng D15 → chỉ acceptor, nói rõ |
| Build time CI tăng (hai bảng dict + hai crate + Java job) | thấp | đo ở B1/C5, ghi STATUS; job `sbe-interop` không chặn nếu > 5 phút → ADR |
| Mac bắt đầu C1 **trước** khi A4 ghi §7 bước 9 lên `main` (chủ chưa nói rõ): crate có trước dòng §7 vài ngày trên một nhánh draft | thấp | nếu chủ cho phép: chấp nhận, vì bước 9 đã có trong kế hoạch được duyệt và A4 gộp trước C; nếu không: Mac chờ PR A gộp — mất song song của C1–C3 |

## Ngoài phạm vi

- **FIXP, SOFH, FAST, FIXML** (ADR-0078 quyết định 2, 4). Không có session cho SBE; `serve*`
  không nhận `Sbe<S>`.
- Bảng `FIX50`, `FIX50SP1` riêng; chọn app dictionary theo `1128` từng message (ADR-0080).
- Cấm `1128/1156/1129` trên message session — item, không xây.
- Flyweight/accessor có kiểu cho SBE (`fn cl_ord_id()`); SBE 2.0 RC.
- Schema của bất kỳ venue nào (iLink 3, B3, MOEX).
- `no_std` cho `codec` — vẫn là mục tiêu; `sbe` đi trước làm mẫu.
- Số thời gian SBE công bố (chỉ baseline nội bộ ở C-desk; công bố cần hai procedure).

## Nhật ký giao hàng

- **2026-09-19 — ngoại lệ `CLAUDE.md` §1, chủ duyệt:** `sbe` + `sbe-gen` là một cặp, một bước
  (`DESIGN.md` §7 bước 9, ghi ở A4), dưới kế hoạch này thay vì "một crate một lần, mỗi crate
  một kế hoạch"; lý do và câu chữ ở mục *Chạy song song*. Cùng ngày chủ duyệt bản sửa cột
  *Phụ thuộc* và mục *Chạy song song*.

*(mỗi PR một mục con `### PR A` / `### PR B` / `### PR C` / `### PR D` — điền khi đóng: commit,
CI run id, gate quote, cái gì chưa làm và vì sao)*

### PR A

- **2026-09-19 — A1 giao, commit `44df719`** (nhánh `plan/phase-2-a`, phiên bàn): bốn file
  đúng như hàng A1 (`crates/codec/src/encoding.rs` mới, `lib.rs` +2 dòng, `tests/encoding.rs`
  mới, `benches/alloc.rs` một case). Bốn gate xanh theo báo cáo của manager (manager chạy lại
  trên commit này trước khi gộp): `cargo test -p fixbolt-codec`; `cargo bench -p fixbolt-codec
  --bench alloc` in `parse via Encoding 0`; clippy `-D warnings`; `cargo doc -p fixbolt-codec`
  không warning. CI run id: chưa có (PR draft mở khi nào thì ghi khi ấy). Trait xây ra khác
  bản phác *PR A* ở ba chỗ — kế hoạch đã sửa cùng ngày (mục *Sửa … lần 2* đầu file, **chờ chủ
  duyệt**); hàng A1 không đổi. A-desk chưa chạy: số band ADR-0031 chỉ có sau A3.
- **2026-09-19 — A2 nêu design finding, kiến trúc sư phán (ADR-0082, Proposed):** `Session<E>`
  không viết được với `E: Encoding` + `E::Dict: Tables` mà thôi; A2 đặt năm đẳng thức kiểu
  trên `impl` (`session/src/lib.rs`, `out.rs`). Phán: **quyết định chưa lấy, không phải lỗi
  A1** — ADR-0079 quyết định 3 hứa nhiều hơn mã, và spec FIX Session Layer §3.1.4/§4.5 ghim
  session vào tagvalue. Ghi ADR-0082; sửa kế hoạch ở mục *Sửa … lần 3*, đoạn *Hệ quả cho A2
  và C4*, hàng A4 (D16), C4 (`SbeTables` chỉ `Dictionary`, bỏ phụ thuộc A2), C7 (doctest
  `compile_fail`), *Tài liệu phải cập nhật*. Mã A1/A2 không đổi; **chờ chủ quyết ADR-0082**.
- **2026-09-19 — A4 viết xong, một việc để lại:** `DESIGN.md` §3 (`codec` thêm `encoding`,
  `dict` thêm `tables`), §4 D16 (+ một câu ở D2), §6 case `parse via Encoding`, §7 bước 9 và
  câu "Steps 1–8 … step 9 is in flight"; `GUIDE.md` mục 3a (alias, `serve*` không đổi);
  `CHANGELOG.md` *Unreleased* (Changed: `Session<E>`; Added: trait, `Tables`, alias);
  `GETTING-STARTED.md`, `README.md` không đổi (ví dụ không đổi chữ ký). **Chưa làm: ADR-0082
  Status vẫn `Proposed`** — hàng A4 ghi "→ Accepted cùng commit, *sau khi chủ duyệt*", chủ
  đang vắng và chưa duyệt, nên D16 và `GUIDE.md` trích ADR-0082 là *Proposed*. Khi chủ duyệt:
  đổi Status trong ADR-0082, bỏ hai chữ "Proposed" ở D16 và `GUIDE.md` 3a, và dòng CHANGELOG.

### PR B

Nhánh `feat/phase-2-b`, base `plan/phase-2-a` (kế hoạch và ADR-0080 nằm ở đó, không ở `main`).
PR draft [#82](https://github.com/tmthang86/fixbolt/pull/82). Máy: cloud Linux, **không phải bàn
§9** — không có số thời gian nào trong PR này.

**Đã giao**

| Bước | Commit | Gate đã chạy và đọc output |
|---|---|---|
| B3 (phần không cần A) | `27051f6` | `cargo test -p fixbolt-conformance` mọi suite `ok`, `fixt_corpus` 4/4; `--test score` in `step_six_b_replays_what_it_sent_and_scores_fifty_nine ... ok` (59/59, không sửa fixture); `cargo test --all` và `--no-default-features` 0 `FAILED`; clippy `-D warnings` sạch; `fmt --check` sạch |
| D1a | `ca15066` | `check-links.py`: `2321 internal links checked`, `no dead internal links`; `wc -l` tám trang: 41/39/35/31/63/31/39/38, đều ≤ 80 |

Đảo chiều (§7, câu FAIL viết trước khi chạy, cả ba đỏ đúng câu đã đoán, khôi phục → xanh):
`1137` 9→8 → `Logon 1137 should equal the corpus's declared value`; count 60→59 →
`expected 59 definitions in this corpus, found 60`; comp_id → `TW50` →
`Logon 49 should equal the corpus's declared CompID`.

Kiểm tay ngoài gate: 85 đường dẫn file mà tám trang `internals/` nêu bằng văn xuôi đều tồn tại
trong `crates/` và `tools/` — `check-links.py` không xét tên file viết trong prose.

**Năm phát hiện chặn B1/B2** (đọc thẳng hai XML ở pin `386ce46e…`, mỗi cái tái lập bằng parser):

1. `FIX50SP2.xml` dùng **10 tên kiểu** `field_type.rs::from_xml` không biết — `XID` 34 field,
   `XIDREF` 29, `LOCALMKTTIME` 45, `MULTIPLECHARVALUE` 8, `XMLDATA` 8, `TZTIMEONLY` 6,
   `MULTIPLESTRINGVALUE` 4, `TZTIMESTAMP` 1, `LANGUAGE` 1, `TAGNUM` 1. `build.rs` **cố ý `die`**
   khi gặp kiểu lạ. Mỗi ánh xạ quyết định `SessionRejectReason 6`, là quyết định hành vi.
2. `XmlData(213)` là `DATA` ở `FIXT11.xml` nhưng `XMLDATA` ở `FIX50SP2.xml`. 71/71 field FIXT11
   đều có trong SP2, 70 khớp số–tên–kiểu, **đúng field này lệch**. ADR-0080 quyết định 2 bảo lệch
   thì `die` → luật như đã viết làm hỏng build vì chính QuickFIX viết hai kiểu cho một field.
3. Oracle của B2 **có tồn tại**: quickfix ở đúng pin ship **160 header sinh sẵn** ở
   `src/C++/fix50sp2/`, nhưng `fetch-quickfix-assets.sh` chỉ sparse-checkout `/src/C++/fix44/`,
   còn `vendor/quickfix-src` chỉ có sau khi `interop.sh` cmake-build libquickfix — việc mà không
   test `dict` nào và không job `test` nào của CI làm.
4. `<component name='MsgTypeGrp' />` **rỗng** ở file transport, có `NoMsgTypes(384)` sáu thành
   viên ở file app, và Logon tham chiếu nó → merge sai chiều làm **mất im lặng** một repeating
   group khỏi Logon (đúng kiểu hỏng của §2 điều 5). **Không `.def` nào trong 180 file có `384=`**
   nên corpus không thấy. `HopGrp` giống hệt hai bên, không xung đột.
5. Câu "đúng một file mỗi dir thiếu `1137`" của hàng B3: mỗi dir có **hai** file không có
   `1137=` — `1d_InvalidLogonNoDefaultApplVerID.def` (Logon thiếu 1137, đúng ý hàng) và
   `1e_NotLogonMessage.def` (**không có Logon nào**). Đã siết chính xác trong test, không cần sửa
   kế hoạch. Cùng kiểu: `49=` khớp CompID khai báo trên mọi Logon trừ `1c_InvalidSenderCompID.def`
   (`49=WT`, cố ý). `[đo 2026-09-19]` 198 dòng Logon `I`, 195 có `1137`.

1–4 đang ở kiến trúc sư → **ADR-0083** + trang `docs/reference/` cho bẫy `XmlData`.

**Lệch phạm vi chủ cần biết:** sửa phát hiện 1 phải động `crates/dict/src/field_type.rs`, mà cột
*File đụng (không đụng gì khác)* của hàng B1 **không liệt kê** file đó — và gate của chính hàng B1
không thể xanh nếu thiếu. Ghi ra đây chứ không làm lặng.

**Sau khi chủ uỷ quyền duyệt (2026-09-19, "uỷ quyền cho bạn duyệt thay tôi")**

ADR-0083 → **Accepted** (`d6134c0`). Dòng *Deciders* ghi rõ chủ **không đọc** ADR: kiến trúc sư
đề xuất, manager tự đo lại sáu sự thật, không ai khác đọc. Số ADR đổi 0082 → 0083 (`c22bb57`)
vì phiên bàn lấy 0082 cùng buổi chiều — hai file khác slug nên **git merge cả hai không báo xung
đột**; trang `two-branches-can-take-the-same-adr-number-without-a-conflict.md` ghi lệnh khảo sát
chéo nhánh nên chạy trước khi viết ADR. Merge `plan/phase-2-a` (`958e71d`) gỡ xung đột duy nhất —
đúng *Nhật ký giao hàng*, hai bên thuần cộng thêm, giữ cả hai theo thứ tự A → B.

| Bước | Commit | Gate manager tự chạy lại trên commit đó |
|---|---|---|
| **B1** | `a787e3a` | `--features fix50sp2` fixt 9/9, field_types 8/8; feature tắt xanh; `--all` và `--no-default-features` 0 `FAILED`; clippy hai chiều sạch; `RUSTDOCFLAGS="-D warnings" cargo doc` hai chiều exit 0; `check-no-optional-deps.sh` ok; `check-indexing-debt.sh` 178/178; `score` 59/59 |
| **B2** | `0a881d2` | `fetch-quickfix-assets.sh` 160 header SP2, corpus 59/539/244 không đổi; `fixt_order` 2/2; `interop_quickfix_order` vẫn 730/730; `git status --porcelain \| grep vendor` **0 dòng** (§2 rule 9) |

**Đảo chiều manager tự làm** (câu FAIL viết trước): cho khai báo component **rỗng** bên transport
thắng → **đoán** `the_logon_carries_the_msg_type_group` đỏ, **thực tế** build chết sớm hơn ở tầng
chặn component rỗng. Đoán sai tầng nào cắn, và phòng thủ hoá ra **xếp lớp**: phải phá cả luật merge
lẫn hai tầng chặn mới tạo được mất mát im lặng.

**Hai phát hiện lớn, đều tự kiểm từ nguồn gốc**

1. **Chi phí bảng thứ hai không phải "double"** như ADR-0080 viết: **25.6×** byte sinh
   (4 008 198 / 156 397) và **9.4×** build nguội (5.25 s / 0.56 s), ba lần mỗi bên, máy cloud.
   Nguyên nhân: `ALLOWED` là bitset trên `0..=max_tag`, max_tag SP2 = **50002** so với 956 →
   782 word/message thay vì 15, trong khi chỉ nhiều hơn 6.6× số field. **Đường nóng không chậm đi,
   không thêm allocation**; feature tắt mặc định nên không sửa gì. Trang
   `a-bitset-keyed-by-tag-scales-with-the-highest-tag-not-the-field-count.md`.
2. **Oracle 730/730 không chuyển được sang SP2.** Hai mệnh đề (delimiter, subsequence) đúng trên
   **cả 25 927** nhóm, giữ hard assert. Mệnh đề 3 ("tag thừa đều là group counter") **sai**:
   1 307 tag / 64 750 lần. Manager tự khai triển đệ quy `FIX50SP2.xml` ngoài `build.rs` và ngoài
   parser của test → `NoSides(552)` trong `AE` ra **162** tag gồm `OrderQty(38)`, đúng bằng `G402`;
   còn `TradeCaptureReport.h` **tự mâu thuẫn**: `FIELD_SET(*this, FIX::OrderQty)` dòng 6417 nhưng
   `message_order(552,…)` chỉ 144 tag, không có 38. Cùng hình thù ở `LegSecurityXML(1872)`.
   Nguyên nhân: `<component>` lồng ≥ 2 tầng trong `<group>`, hình thù FIX 4.4 gần như không có.
   **Bỏ field đi cho khớp QuickFIX mới là sai** (§2 điều 5, D3). Test chốt bốn số bằng `assert_eq!`
   (25 927 / 231 / 1 307 / 64 750). Trang
   `quickfix-drops-deeply-nested-fields-from-its-own-message-order.md`.

**Lệch phạm vi đã ghi, không làm lặng:** `crates/dict/src/field_type.rs` (ADR-0083 quyết định 1)
và `scripts/fetch-quickfix-assets.sh` (quyết định 3) đều **không** có trong cột *File đụng* của
hàng B1/B2. Thêm: `scripts/check-indexing-debt.sh` hạ trần 181 → 178 vì refactor `UtcTimestamp`
bỏ được ba subscript panic — chính script đó yêu cầu hạ trần trong cùng commit.

**Chi phí fetch cho B8/D2 chép sang:** `src/C++/fix50sp2/` = 26 413 279 byte / 160 file, khớp dự
đoán 26.4 MB của ADR-0083; `vendor/quickfix` 14M → 39M. Ba fetch nguội mỗi bên: 3.70/3.37/3.45 s
trước, 2.23/3.32/3.30 s sau — **thời gian thêm không phân biệt được với nhiễu mạng** trên máy này.
ADR yêu cầu CI cho số riêng, vẫn giữ.

**ADR-0084 (Accepted) — B4 dừng ở 173/180 và ba hàng nó đẻ ra**

`score_fixt` đỏ **không phải lỗi B4**. Bảy file hỏng là ba câu hỏi ADR-0080 quyết định 2 chưa trả
lời, manager tự đo lại từng cái: `14a`×3, `14i`×3, `21`×1. Ba hàng follow-on, **chưa làm**:

| Hàng | Việc | Tier |
|---|---|---|
| **B4a** | `Tables::is_defined_tag_for(msg_type, tag)` + `TRANSPORT_DEFINED_TAGS` trong `generate_pair`; `scan_fields` đổi một lời gọi. Một message được validate theo tập tag của **tầng định nghĩa nó** — cả hai engine QuickFIX làm vậy | developer |
| **B4b** | Hoãn `373=5/6` trên thành viên group tới sau `373=1` và `373=16` (chỗ QuickFIX/J đặt), **gộp cùng** luật per-token `enum_allows` của ADR-0083 — cùng một arm, cùng kiểu test tay, **đổi hành vi FIX 4.4**, không có `.def` nào canh | **senior developer** |
| **B4c** | `score_fixt` đổi assertion sang `(179, 180)` + tuple ghim `21_…def:17` với `373=5 371=336`; sửa dòng *Cách kiểm chứng* (đã làm ở trên) | developer |

**Số đo đáng nhớ:** enum giữa các service pack **trôi hai chiều**, không phải superset như
ADR-0080 quyết định 3 viết. FIX50→SP2: 4 field free→enum, 5 mất giá trị, **1 enum→free** —
`DeskOrderHandlingInst(1035)` **24 giá trị → 0**. SP1→SP2: 3 / 3 / **2** (thêm `1395` 3→0).
Nghĩa là chấm `fix50` bằng bảng SP2 sai được **cả hai chiều**; assertion chỉ ghim được chiều chặt,
và điều đó **chấp nhận được** vì **0/180** file `.def` mang `1035=` hoặc `1395=` (đã grep).

**Một hàng kế hoạch chưa bao giờ được xây:** bảng traps nói B1 sinh `is_admin` từ `msgcat` và
`is_admin(b"n")` phải true. Đo: **0** lần `is_admin` trong bảng sinh, **0** lần `build.rs` đọc
`msgcat`, và `ADMIN` là const 7 phần tử **viết tay** ở `session/src/lib.rs:291`, **không có**
`b"n"`. Nợ của B1 hay bị bỏ lặng — cần xác định, chưa xử lý.

**Lỗ hổng CI, nghiêm trọng:** `grep fix50sp2 .github/workflows/ci.yml` **không khớp gì**. Bốn
binary test FIXT (`dict/tests/fixt.rs`, `dict/tests/fixt_order.rs`, `session/tests/fixt.rs`,
`session/tests/score_fixt.rs`) **chưa từng chạy trong CI**. Mọi số FIXT của PR này là lời khai
của máy cloud. Bịt ở **B7**, kèm phép thử ngược: cố ý làm một test sau feature đỏ và xác nhận CI
đỏ theo. Trang `a-feature-gated-test-is-a-test-ci-never-runs.md`.

**Chưa làm, và vì sao**

- `echo.rs` generic `E` (B3): cần A1, và A3 đang sửa chính `echo.rs`.
- B4–B7: sau khi PR A gộp → `git fetch && git rebase origin/main`, chạy lại gate rồi mới tiếp.
- Một hàng `CLAUDE.md` §4 trỏ `docs/internals/` (hàng D1 yêu cầu): `CLAUDE.md` là file **chỉ bàn**
  theo *Chạy song song* → **giao lại cho phiên bàn**.
- Dòng cho `DESIGN.md` / `GUIDE.md` / `CHANGELOG.md` / `CONFIGURATION.md`: thuộc B8.
- **Luật `enum_allows` per-token cho bảng `Fix44`**: `enum_allows(18, b"2 A") == Some(false)` →
  `373=5` cho một `ExecInst` hợp lệ (`session/src/lib.rs:3765`); cả hai engine QuickFIX tách theo
  dấu cách trước. 0/239 `.def` gửi field multi-value nên 59/59 không thấy. B1 dựng bảng **FIXT**
  per-token và **không** đụng `Fix44` — sửa FIX 4.4 là thay đổi biên session không có oracle,
  **cần hàng kế hoạch riêng** (ADR-0083 quyết định 1, luật thứ hai).
- Gác trùng số ADR: một dòng shell, nhưng `ci.yml` thuộc bước khác, và script không job nào chạy
  là check không ai đọc (§10). Ghi trong trang reference cho phiên sở hữu file đó.

**CI xanh — `35430710585`, 14/14 job, trên `33c85d8`** (§9). Job `feature-sets` đỏ **sáu head
liên tiếp** vì `serve_with` ở `crates/engine/src/lib.rs:134` của PR A (link hỏng khi không có
feature `standard`), đỏ cả trên base; `crates/engine` ngoài bộ file nhánh này nên patch một dòng
**đã kiểm chứng rồi hoàn nguyên** nằm trong comment PR #82, không push. Bàn sửa ở `c2df98f` —
đúng bản patch đó — và `33c85d8` merge base về, gỡ CI. Bốn lệnh rustdoc từng đỏ chạy lại trước khi
push: cả bốn `OK`.

**Chưa chứng minh:** không có số nào từ bàn §9.
Sáu variant `FieldType` mới **không có corpus nào đỡ** — luật `accepts` rút từ chữ của spec, và
arm FIXT của `interop.sh` (B7) là đối tác thật đầu tiên có thể phản bác. ADR-0083 trích FIX
Orchestra *FIX Latest EP312*, **không** phải PDF SP2 Volume 1 (proxy chặn `fixtrading.org`,
`onixs.biz`) và nói rõ chỗ đó.

#### PR B — đã gộp

**`[2026-09-19]`** PR [#82](https://github.com/tmthang86/fixbolt/pull/82) gộp vào `main` tại
**`e673e8f`**, no-ff. **CI xanh trên chính commit merge**, run
[`35456988038`](https://github.com/tmthang86/fixbolt/actions/runs/35456988038), **14 / 14 job** —
đếm, không liếc. Commit đóng nhánh `24e0e6e` cũng 14 / 14, run
[`35456425546`](https://github.com/tmthang86/fixbolt/actions/runs/35456425546).

Base của PR **đổi từ `plan/phase-2-a` sang `main`** trước khi gộp: nhánh đó đã nằm trong `main`,
nên gộp theo cấu hình cũ sẽ đưa cả track vào một nhánh không dẫn đi đâu mà vẫn báo thành công.
`main` được gộp vào trước (`24e0e6e`), bốn xung đột tài liệu giải bằng tay.

Sau khi gộp, `scripts/bench.sh` đọc **20 / 20** target: hai crate SBE của PR C đi vào mà không
phải sửa dòng nào, vì B6 đã cho script lấy feature set và danh sách binary **từ `cargo metadata`**
thay vì danh sách package viết cứng.

Còn nợ, đã ghi ở `STATUS.md` mục *Not proven*: ba băng cần bàn §9 (A-desk, `validate` của B4b,
chi phí quét vị trí); `bad_group_count` bỏ cả lượt `373=16` khi gặp nhóm lồng (cần hàng riêng);
`is_admin` chưa dựng; hàng `CLAUDE.md` §2 và §7; gác trùng số ADR; doctest sau feature.

#### PR B — các bước B5 đến B8

| Bước | Commit | Bằng chứng manager tự chạy lại trên đúng commit đó |
|---|---|---|
| **B5** | `fbdf1aa` | `wire_fixt` **60 / 60** đọc bằng `--nocapture` (có `assert_eq!`, không chỉ in); `--test wire` 59/59 với `wire.rs` **không sửa**; `doc_table` 13 passed (trước 11 passed 2 FAILED); `--all` / `--no-default-features` 0 failed; clippy + fmt + rustdoc hai chiều sạch; `check-no-kernel-sleep.sh` GREEN + RED ok; `check-standard-gives-the-core-back.sh` GREEN CPU 0% ngủ 20/20, RED ok trên `hft` và `yield` |
| **B6** | `29cf3c3` | `scripts/bench.sh` 18/18 target, `invariant failures 0`, `OK`; bốn case mới đọc **0**; tập 18 binary `bench.sh` build **trùng khít** tập read-back chứng nhận (so bằng `comm`/`diff`, không bằng exit code) |
| **B7** | `9ca0608` | `scripts/interop.sh` rc=0, FIXT **7 / 7 acceptor + 7 / 7 initiator**; vòng lặp bốn crate của CI chạy tại chỗ, `check-feature-gated-tests-ran.sh` ok cả bốn; `grep -c fix50sp2 ci.yml` 12 (trước 0); `git status --porcelain \| grep vendor` **rỗng** sau một lần chạy interop đầy đủ (§2 rule 9) |
| **B8** | (commit này) | `check-links.py` không link chết; bảng đồng bộ §4 đi từng dòng |

**Ba lần agent sửa manager, và cả ba lần agent đúng.** Ghi lại vì đây là bằng chứng quy trình
§12 hoạt động, không phải để tự trách:

1. **B4b** — manager đề nghị gate việc hoãn `373=5/6` bằng `has_groups(msg_type)`, lý lẽ là
   Heartbeat không khai báo group nào. Agent **từ chối và chứng minh sai**: `NoHops(627)` nằm
   trong `<header>` của `FIX44.xml` nên **mọi** message type đều có group. Gate ấy nếu dựng thật
   thì **59/59 và 179/180 đều vẫn xanh** — không `.def` nào gửi `NoHops` có nội dung.
2. **B5** — brief của manager bắt hai `Problem` mới mang `{ line }`. Agent từ chối: cả 20+
   variant hiện có đều không có trường, số dòng đã đi trên `SettingsError`. Manager đọc lại enum
   và constructor: agent đúng.
3. **B6** — manager chẩn đoán sai nguyên nhân lỗ hổng đọc-ngược căn chỉnh, và cách sửa một dòng
   của manager sẽ đưa từ **1 sai thành 17 sai**. Nguyên nhân thật là `--workspace` hợp nhất
   feature khác `-p`. Ghi ở
   [a-workspace-build-and-a-per-package-build-are-different-artifacts](../reference/a-workspace-build-and-a-per-package-build-are-different-artifacts.md).

**Còn nợ, nêu tên chứ không để trôi:**

- **Hai băng chưa đo, cần bàn §9**: band ADR-0031 của PR A (A-desk), và band `validate` của B4b
  (`validate NewOrderSingle` 877.5 → 906.2 ns, `validate Heartbeat` 140.7 → 166.2 ns). Máy cloud
  là Xeon, `benches/baselines.tsv` chỉ có Ryzen, nên mọi lần chạy in `NO BASELINE` và hộp này
  **không phân giải nổi** hiệu ứng cỡ đó — cùng mã không đổi đo được 158.4 rồi 166.2 ns.
- **`is_admin` chưa bao giờ được dựng.** Bảng *Bẫy* của kế hoạch chờ B1 sinh nó từ `msgcat` với
  `is_admin(b"n")` đúng. Đo: 0 lần xuất hiện trong bảng sinh ra, `build.rs` không đọc `msgcat`,
  và `ADMIN` là const viết tay 7 phần tử ở `session/src/lib.rs:291`, **thiếu `b"n"`**. Là nợ B1
  hay đã bị bỏ im lặng thì chưa rõ, và ghi lại là chưa rõ.
- **`CLAUDE.md` §2 bảng *Machine checks* hàng 3 và §7 hàng "Any session-layer change"** vẫn nói
  59. Nay còn 179/180 sau feature. Kế hoạch giao dòng này cho manager, nhưng chủ đã liệt
  `CLAUDE.md` vào danh sách không được đụng của PR B, nên **để nguyên và ghi nợ** thay vì tự ý
  sửa một file chủ đã rào.
- **Doctest sau feature vẫn không chạy ở đâu cả**: `check-feature-gated-tests-ran.sh` nhận
  `--tests`. Hôm nay chưa có doctest `fix50sp2` nào. Ghi trong `ci.yml`.
- **Gác trùng số ADR** vẫn chưa có (hai nhánh lấy cùng một số mà git không xung đột).

### PR C

- **2026-09-19 — ADR-0081 Accepted** (`980338a`, nhánh `feat/phase-2-c`, PR
  [#83](https://github.com/tmthang86/fixbolt/pull/83), phiên Mac): chủ duyệt đích danh, và uỷ
  quyền cho phiên này mọi quyết định thiết kế và gộp sau đó của PR C.
- **2026-09-19 — C1 giao, commit `4390256`:** `crates/sbe` (`no_std`, `forbid(unsafe_code)`,
  không dependency), `scripts/fetch-sbe-assets.sh` ghim spec `418a8f6` và Real Logic `05b076c`
  (1.40.2). 22 + 1 test xanh, clippy sạch, bốn đảo chiều đỏ đúng chỗ. Bẫy ghi ở
  `docs/reference/the-sbe-rc4-example-dumps-disagree-with-their-own-tables.md`: dump §7 và
  bảng giải thích của chính nó không khớp ở bốn chỗ — test tin byte, không tin bảng.
- **2026-09-19 — Sửa kế hoạch lần 4 (PR C), duyệt theo uỷ quyền của chủ:** hàng C2/C3 như
  viết không xây được, vì `build.rs` không dùng được dev-dependency — để `crates/sbe/build.rs`
  gọi `sbe-gen` thì `sbe` phải có build-dependency kéo `roxmltree`, trái ADR-0081 quyết định 1
  ("not a dependency of `sbe`"). Sửa:
  1. Bảng sinh cho test do **`crates/sbe-gen/build.rs`** tạo (nạp mã generator bằng
     `#[path]`, `roxmltree` là build-dependency của `sbe-gen`); `sbe-gen` có dev-dependency
     `fixbolt-sbe`. Không có `crates/sbe/build.rs`.
  2. Test của C3 (`spec_examples.rs`, `car_roundtrip.rs`, `versioning.rs`) nằm ở
     **`crates/sbe-gen/tests/`** thay vì `crates/sbe/tests/`; gate C3 là
     `cargo test -p fixbolt-sbe-gen`.
  3. CI chỉ chạy `scripts/fetch-quickfix-assets.sh`, và Mac không đụng `ci.yml`; nên script
     ấy gọi `scripts/fetch-sbe-assets.sh` ở dòng cuối. Thiếu `vendor/sbe-*` thì `build.rs` vẫn
     cho lib build (người dùng `sbe-gen` không cần vendor), nhưng file sinh ra là
     `compile_error!` nên test **đỏ**, không bao giờ xanh lặng lẽ.
  4. `example-schema.xml` dùng `xi:include`; `generate(xml)` giữ nguyên chữ ký và trả
     `Error::Unsupported` khi gặp include; thêm `generate_with_includes(xml, resolve)`.
- **2026-09-19 — C2 `163c845`, C3 `7d7b723`, C4 `2429d5c`, C7 `5fe1214`, C6 `8982d36`, C8
  `ecbbb01`.** C3 và C4 chạy song song, rồi C6 và C7 song song (bộ file tách rời). Hai lệch
  so với bảng, quyết theo uỷ quyền: (a) **encode lại từng byte** chuyển từ C3 sang C4, vì chỉ
  C4 xây encoder — C3 chỉ còn phía đọc; (b) `sbe` phụ thuộc `fixbolt-codec` **chỉ** sau feature
  `encoding` (mặc định bật, gate `mod`), nên `--no-default-features` vẫn không dependency.
  `Encoding::encode` chỉ ghi root block; group và `varData` đi qua `MessageWriter`. Bench C6
  dùng bảng viết tay (một message "nested group + varData" thay cho `Car`, gọi đúng tên), vì
  `sbe` không được phụ thuộc `sbe-gen`. Bẫy mới:
  `docs/reference/an-encoding-that-ignores-a-const-parameter-makes-every-caller-name-it.md`.
- **2026-09-19 — review senior (Opus, context mới): 10 finding, cả 10 manager tái hiện bằng
  probe và đối chiếu spec**, cả 10 xác nhận, không cái nào bị bác. Một blocker: composite có
  `offset` (padding) làm generator đặt field sau chồng lên nó. Sáu nên sửa: một message 14 byte
  bắt reader đi 2e8 entry (2,57 s); `GroupWriter::entry` lỗi mà không lùi con trỏ; ba luật
  schema của spec bị nhận lặng lẽ; optional non-char array và `sinceVersion` trong composite
  sinh ra bảng hỏng thay vì `Unsupported`. Ba nhỏ: `byteOrder` lạ thành little, f64 quá lớn
  thành `inf`, test big-endian thiếu độ rộng. Sửa cả 10, mỗi cái có test đỏ trước (test big-endian
  xanh ngay từ đầu — chỉ là thêm phủ, không phải lỗi). Ghi ở `docs/reference/sbe-spec-facts.md`
  mục cuối.
- **Chưa làm ở PR C, và vì sao:** C5 (`sbe-interop`: Linux + Java + `ci.yml`) thuộc phiên
  cloud; C-desk (baseline thời gian) cần bàn §9; D1b cần `docs/internals/` của D1a, hiện chỉ
  nằm trên `feat/phase-2-b`. `sinceVersion` chưa được thử qua bảng **sinh ra** (không schema
  mẫu nào khai báo nó); `check-no-crate-root-allow.sh` không chạy được trên Mac (bash 3.2), CI
  chạy nó.
