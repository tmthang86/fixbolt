# Khởi động lại mà phiên vẫn tiếp tục

> **Loại:** Plan · **Ngày:** 2026-08-30 · **Trạng thái:** Đang chờ ADR-0010 được duyệt (2026-08-30)
> **Phạm vi:** open item 16 — `session` và `engine`

## Bối cảnh

Journal hiện **chỉ ghi, không bao giờ đọc lại lúc khởi động**. Ba tầng D7 đã dựng xong
(`NoJournal`, `MemJournal`, `FileJournal` với `Durability::{Async, Fsync}`), và session đọc
journal khi trả lời `ResendRequest` trong cùng một lần chạy. Nhưng **không có gì khôi phục số
thứ tự gửi ra hay các message chưa được xác nhận từ log khi tiến trình khởi động lại**.

Hệ quả thực tế: `Fsync` hôm nay là một **dấu vết kiểm toán**, không phải một cơ chế khôi phục.
Nó tốn tiền của một cơ chế khôi phục mà không cho cái lợi của nó — và đó là kiểu chi phí tệ
nhất, vì nó **trông giống như** đã có bảo đảm.

Corpus không nhìn thấy chuyện này: không file `.def` nào yêu cầu số thứ tự sống qua một lần
khởi động lại. `STATUS.md` đã ghi rõ ở mục "Not proven" rằng số thứ tự **reset mỗi lần
connect**, và không có gì chứng minh cái reset đó đúng cho một triển khai thật.

## Những gì đã biết chắc

- `fixbolt_session::journal::Journal` là một trait với đúng hai phương thức: `put(seq, bytes)`
  và `get(seq) -> Option<&[u8]>`. Session không giữ byte nào nó không tự sinh ra.
- [ADR-0008](../decisions/ADR-0008-journal-is-a-trait.md) giải thích vì sao là trait chứ không
  phải `Action::Store` như D1 phác — **một resend phải đọc, mà một action thì không trả lời
  được**.
- Ba tầng D7 đã có: `NoJournal`, `MemJournal<N, LEN>`, `FileJournal<N, LEN>` với
  `FileJournal::open(path, how)` và `close()`.
- **Điểm conformance phụ thuộc vào journal, và đã chứng minh bằng đảo ngược:** làm
  `MemJournal::put` không giữ gì thì 4 trong 7 test của `tests/journal.rs` đỏ **và** `--test
  score` tụt dưới 59.
- **Chuyện về gap thứ hai corpus không nhìn thấy được.** Mọi file mở một gap đều kết thúc
  trước khi mở gap thứ hai, và gap sâu nhất chỉ giữ hai message. Việc đóng một gap đã lấp,
  phát lại theo thứ tự, và bỏ cái không còn chỗ — tất cả chỉ được canh bởi
  `crates/session/tests/resend.rs`.
- **Trait không có phương thức nào để hỏi "log này đang có gì".** Không có `last_seq`, không
  có cách duyệt. Đó chính là mảnh thiếu.
- `crates/session/src/journal.rs:18` có một link rustdoc chết trỏ tới
  `https://github.com/fixbolt/...` — sửa trong plan `gates-that-can-be-trusted`.

## Cách làm

Session phải **dựng được *từ* một journal**, chứ không chỉ *ghi vào* một journal.

1. **Mở rộng trait `Journal`** thêm đúng cái tối thiểu để khôi phục: số thứ tự cao nhất đã ghi.
   Không thêm API duyệt — session không cần duyệt, nó cần biết đếm tiếp từ đâu.
2. **Thêm một đường dựng session từ trạng thái đã lưu**: số thứ tự gửi ra tiếp theo lấy từ
   journal thay vì từ 1.
3. **Số thứ tự nhận vào cũng phải sống sót**, và đây là nửa khó hơn: journal hiện chỉ giữ
   message *gửi ra*. Cần một chỗ ghi bền cho `next_in`, và nó phải được cập nhật **trước khi**
   ứng dụng nhìn thấy message — nếu không, một lần chết đúng lúc sẽ xử lý lại một message đã
   xử lý.
4. **Quyết định điều gì xảy ra khi journal và counterparty bất đồng** lúc Logon. Đây là một
   quyết định giao thức có giá, nên nó cần **một ADR riêng (ADR-0010)**, không quyết trong
   plan.

Bước 4 là bước phải xin duyệt trước khi viết code, vì nó chạm vào tầng session — chỗ mà
`CLAUDE.md` §2 điều 3 nói rõ: một thay đổi session chưa chạy đủ 59/59 thì chưa xong.

## Bất biến bị đụng tới

- **Số 2** (tầng session thuần khiết: không socket, không đồng hồ, không cấp phát, không
  `format!`). **Đây là bất biến chịu áp lực lớn nhất ở plan này.** Khôi phục nghe rất giống
  "đọc file", và đọc file thì không được nằm trong session. Cách giữ: session vẫn chỉ nói
  chuyện với trait; **mọi thứ chạm đĩa nằm trong `engine`**, đúng như ADR-0008 đã đặt ra.
- **Số 3** (59 định nghĩa là cổng). Đây là thay đổi tầng session → **bắt buộc chạy 59/59**, cả
  in-process lẫn qua socket.
- **Số 1** (không cấp phát trên hot path). Khôi phục xảy ra lúc khởi động, không phải trên hot
  path — nhưng đường `put`/`get` thì có, và không được đổi.
- **Số 7** (không `unwrap`/`expect`/`panic` trong crate thư viện).

## Chia việc

| Bước | Kết quả | Phụ thuộc |
|---|---|---|
| 1 | Test đỏ trước: khởi động lại một session và khẳng định số thứ tự gửi ra tiếp tục thay vì reset. Hiện phải đỏ | — |
| 2 | **ADR-0010** đề xuất: journal và counterparty bất đồng lúc Logon thì xử sao. Chờ duyệt | — |
| 3 | Trait `Journal` thêm cách hỏi số thứ tự cao nhất; ba tầng cài đủ | 1 |
| 4 | Session dựng được từ journal; số thứ tự gửi ra sống qua khởi động lại | 3, 2 |
| 5 | `next_in` bền, cập nhật trước khi ứng dụng thấy message | 4 |
| 6 | **59/59 in-process và qua socket**, không sửa một dòng nào trong `score.rs` hay `wire.rs` | 5 |

## Cách kiểm chứng

- **Test khôi phục thật, không giả lập:** dựng `FileJournal` trên một file tạm, chạy một phiên,
  **thả nó đi**, mở lại từ cùng file, và khẳng định phiên tiếp tục ở đúng số. Một test dùng
  `MemJournal` cho bước này chứng minh được số 0 — bộ nhớ thì đằng nào cũng còn đó.
- **Đảo ngược từng cái:** bỏ đọc lại số gửi ra → test đỏ. Bỏ bền hoá `next_in` → test đỏ. Mỗi
  lần đảo phải xác nhận cái sửa đã vào file thật.
- **Cắt ngang giữa chừng.** Kịch bản đáng sợ là chết *giữa* `put` và lúc ứng dụng thấy message.
  Ít nhất phải có một test cắt file journal ở giữa một bản ghi và khẳng định mở lại vẫn cho ra
  một trạng thái nhất quán — không panic, không đọc rác.
- **`Fsync` và `Async` phải cho kết quả khôi phục khác nhau**, nếu không thì cái đắt hơn đang
  không mua được gì và ADR-0008 cần sửa.
- Mỗi bước: `cargo test --all`, `--no-default-features`, và **59 định nghĩa** (điều 3).

## Tài liệu phải cập nhật

- [ ] `DESIGN.md` §4 D1 và D7 — hành vi session và journal đổi
- [ ] `CHANGELOG.md` — trait `Journal` là API công khai và đang đổi
- [ ] rustdoc của `Journal`, ba tầng, và đường dựng mới
- [ ] `docs/decisions/ADR-0010-*.md` — mới; ADR-0008 có thể cần một dòng "được bổ sung bởi"
- [ ] `STATUS.md` — đóng item 16; sửa mục "Not proven" về việc reset số thứ tự
- [ ] `PRD.md` — nếu chạm tiêu chí thoát phase 1

## Bẫy đã lường trước

| Bẫy | Test canh |
|---|---|
| Test khôi phục dùng `MemJournal` → chứng minh số 0 | Test bắt buộc dùng `FileJournal` và thả instance đi |
| Ghi `next_in` **sau** khi ứng dụng thấy message → xử lý lại sau khi chết | Test cắt ngang, khẳng định không có message nào thấy hai lần |
| Đọc lại làm chậm đường `put` trên hot path | `benches/alloc.rs` và bench journal chạy lại, so trước/sau |
| Journal hỏng làm session panic thay vì từ chối | Test với file cắt cụt và file rác; phải trả lỗi, không panic (điều 7) |
| Sửa `score.rs` hoặc `wire.rs` cho 59/59 qua | Hai file đó **không được đổi**; kiểm bằng `git diff` trước khi commit |
| `Fsync` và `Async` khôi phục như nhau → cái đắt vô dụng | Test riêng so hai chế độ sau khi cắt ngang |

## Rủi ro

| Rủi ro | Mức | Cách xử lý |
|---|---|---|
| Khôi phục kéo I/O vào tầng session, phá bất biến 2 | Cao | Mọi thứ chạm đĩa ở `engine`; session chỉ thấy trait. Đọc lại §2 bằng tay trước mỗi commit |
| ADR-0010 khó quyết vì không có counterparty thật để thử | Cao | Ghi rõ là quyết trên lập luận, không trên đo — như ADR-0004 và -0005 đã làm |
| Corpus không kiểm được gì ở đây, nên test là do mình bịa | Cao | Nói thẳng trong `STATUS.md` mục "Not proven". Bịa một test đồng ý với một quy tắc bịa là một phỏng đoán viết hai lần — bài học đã trả giá ở bước 4 plan session |
| 59/59 tụt sau khi đổi session | Trung bình | Đó là tín hiệu dừng, không phải tín hiệu sửa test |

## Ngoài phạm vi

- **Không** làm replication hay journal chia sẻ giữa nhiều tiến trình.
- **Không** đổi ba tầng D7 thành thứ khác.
- **Không** đụng `tools/w2w`, DATA field, hay chính sách ring đầy.
- **Không** tự quyết chuyện Logon bất đồng — đó là ADR-0010.

## Nhật ký giao hàng

**Duyệt 2026-08-30.** Chủ dự án duyệt cả sáu plan cùng lúc, kèm một uỷ quyền ghi rõ:
*trong quá trình làm, nếu plan sai thì được sửa plan theo tình hình thực tế.* Điều đó nới
`CLAUDE.md` §1 — chỗ bảo "dừng lại, sửa plan, xin duyệt lại" — thành "sửa plan, **ghi lại
vào đây**, đi tiếp". Mỗi lần sửa plan phải có một mục dưới đây nói rõ **sửa gì và vì sao**,
nếu không thì uỷ quyền này biến thành giấy phép đi chệch trong im lặng.

---

### Bước 1–3 xong 2026-08-30. Dừng trước bước 4, đúng như plan.

**Journal đọc lại được. Trước đó nó là *chỉ-ghi*, và đó không phải chuyện nhỏ.**

**Sửa plan — một khuyết tật plan không lường trước.** Format trên đĩa là
`seq(4 byte) || message`, **không có độ dài**, nên **không tách được bản ghi khi đọc**. File có
thể ghi thêm mãi mà không bao giờ phân tích được. Tách bằng cách re-frame FIX thì chạy được,
nhưng gắn journal vào codec và vẫn để cái đuôi bị cắt dở ở trạng thái mơ hồ. Đổi thành
`seq(4) || len(4) || bytes` — crate chưa publish nên không có ràng buộc tương thích.

**Đã làm:**

- `Journal::highest() -> Option<u32>` — **cố ý không có default**. Một default trả `None` sẽ
  để một journal *đang giữ* message báo là không giữ gì, và session resume từ nó lặng lẽ bắt
  đầu lại từ 1.
- `FileJournal::open` **đọc file trước khi append**, nạp lại vào ring bộ nhớ. Đuôi bị cắt dở
  thì **bỏ, không đọc nửa vời** — phát lại byte chưa từng lên dây tệ hơn phát lại không gì cả,
  vì gap fill là câu trả lời hợp lệ còn một message hỏng thì không.

**Đã chạy, đọc output:**

```
4 / 4    cargo test -p fixbolt-engine --test recovery
59 / 59  cargo test -p fixbolt-session --test score      (bất di bất dịch 3)
59 / 59  cargo test -p fixbolt-engine  --test wire
210 passed / 0 failed   cargo test --all · --no-default-features
0 allocations           benches/alloc.rs
clean                   clippy -D warnings · fmt --check
git diff score.rs wire.rs   rỗng — không sửa test nào để đi qua
```

**Đảo ngược ba lần, và lần thứ ba đáng ghi lại.** Bỏ đọc file lúc open → 3/4 đỏ. Chấp nhận
đuôi cắt dở → 1/4 đỏ. Lần thứ ba — đổi `max()` thành `min()` — **báo XANH, và nó là một
reversal rỗng**: chuỗi thay thế không khớp vì `cargo fmt` đã gộp dòng, nên **không có gì được
tiêm vào cả**. `grep` đếm ra `0` và đó là thứ bắt được. Tiêm lại cho đúng thì đỏ đúng như phải
thế (`left: Some(7), right: Some(8)`). Đây chính xác là bẫy `false-greens.md` §5 mà tôi đã
trích dẫn suốt phiên này — **và tôi vẫn dính**. Bài học không phải "cẩn thận hơn" mà là
**luôn `grep` xác nhận cái tiêm đã vào file trước khi đọc kết quả**.

**Dừng trước bước 4 — và lý do khác plan.** Plan nói ADR-0010 là về *journal bất đồng với
counterparty lúc Logon*. Câu hỏi thật sự đứng trước nó và lớn hơn:

`[measured 2026-08-30]` **ba file trong corpus reconnect** — `2i` (2 lần), `2k` (3),
`2o` (2) — và **cả bảy lần connect đều kỳ vọng `34=1` trả về**, **không Logon nào mang
`141=Y`**. Cả corpus chỉ một file nhắc tới `141=`.

Nhưng **corpus là cái harness, không phải giao thức.** FIX 4.4 đánh số một *session*, không
phải một *connection*; QuickFIX khi triển khai thật thì giữ số qua reconnect và reset theo giờ
trong ngày. Harness của nó bắt đầu mỗi `iCONNECT` từ store sạch — đó là tính chất của cách chạy
test, không phải phát biểu về nghĩa vụ của một acceptor.

**Nên corpus và một triển khai thật đòi hai hành vi ngược nhau, và code hiện không phân biệt
được nó đang ở đâu**, vì `connect` là lối vào duy nhất và được gọi trong cả hai trường hợp.

[ADR-0010](../decisions/ADR-0010-a-reconnect-is-not-a-restart.md) đặt đúng câu hỏi đó, trạng
thái **Proposed**. Bước 4 và 5 chờ chữ ký.
