// A minimal FIX 4.4 initiator built on libquickfix, for one purpose: to be the
// judge of this repository's **acceptor**, which is the product it is
// positioned on and which no other implementation had ever logged on to.
//
// See tools/interop/src/main.rs `--role acceptor` for the other end and
// docs/plans/2026-09-03-acceptor-interop.md for why this direction exists at
// all. ADR-0042 says a second implementation is the only independent opinion;
// until this file, that sentence applied to half the engine, and to the half
// that is not the differentiator.
//
// THIS FILE IS THIS PROJECT'S OWN CODE. Nothing here is copied from QuickFIX;
// it calls QuickFIX's public API, which is what CLAUDE.md §2 rule 9 permits and
// vendor/ being gitignored enforces. It is compiled by scripts/interop.sh and
// by the `interop` CI job, and by nothing else — no Cargo.toml mentions C++.
//
// ---------------------------------------------------------------------------
// WHY EVERY STEP IS SCORED ON THE RAW WIRE STRING
// ---------------------------------------------------------------------------
//
// QuickFIX's session layer does not hand a PossDup replay of a sequence number
// it has already seen to `fromApp` — it treats it as a duplicate and drops it
// before the application. Step 5 asks for exactly such a replay. A judge
// written on the `fromApp` / `fromAdmin` callbacks would therefore report the
// acceptor's CORRECT answer as a missing message.
//
// So the judge is a `FIX::Log`: `onIncoming` sees every byte that arrived,
// before any of that. Everything below matches on those strings.
//
// ---------------------------------------------------------------------------
// READING THE RESULT
// ---------------------------------------------------------------------------
//
// One line per step, `interop-acceptor: <step> ok|FAIL  <what was seen>`, and
// a last line `interop-acceptor: PASS n/7`. scripts/interop.sh greps for each
// step name AND for the PASS line, because a binary that dies before printing
// and a binary that prints seven failures both exit non-zero.

#include <quickfix/Application.h>
#include <quickfix/FileStore.h>
#include <quickfix/Log.h>
#include <quickfix/Message.h>
#include <quickfix/Session.h>
#include <quickfix/SessionID.h>
#include <quickfix/SessionSettings.h>
#include <quickfix/SocketInitiator.h>
#include <quickfix/fix44/NewOrderSingle.h>
#include <quickfix/fix44/ResendRequest.h>
#include <quickfix/fix44/TestRequest.h>

#include <chrono>
#include <cstdlib>
#include <iomanip>
#include <iostream>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

namespace {

using Clock = std::chrono::steady_clock;

std::string readable(const std::string &raw) {
  // A leading and trailing '|' so a match on "|35=A|" cannot hit the middle of
  // another field's value. The same shape as `readable` on the Rust side.
  std::string out = "|";
  for (char c : raw) {
    out += (c == '\001') ? '|' : c;
  }
  return out;
}

/// Every inbound message, in order, as it arrived — and, separately, a full
/// two-way transcript printed only when a step fails.
///
/// Scoring reads the inbound tape alone. The transcript exists because an
/// acceptor that HANGS and an acceptor that REFUSES produce the same silence
/// at a deadline, and the only thing that tells them apart is what was on the
/// wire before it went quiet.
class Tape {
public:
  void add(const std::string &raw) {
    FIX::Locker lock(m_mutex);
    m_in.push_back(readable(raw));
    m_all.push_back("  in  " + readable(raw));
  }

  void sent(const std::string &raw) {
    FIX::Locker lock(m_mutex);
    m_all.push_back("  out " + readable(raw));
  }

  void note(const std::string &what) {
    FIX::Locker lock(m_mutex);
    m_all.push_back("  --  " + what);
  }

  std::vector<std::string> snapshot() const {
    FIX::Locker lock(m_mutex);
    return m_in;
  }

  void dump() const {
    FIX::Locker lock(m_mutex);
    std::cerr << "---- what this initiator saw on the wire ----" << std::endl;
    for (const std::string &line : m_all) {
      std::cerr << line << std::endl;
    }
  }

private:
  mutable FIX::Mutex m_mutex;
  std::vector<std::string> m_in;
  std::vector<std::string> m_all;
};

Tape g_tape;

/// The judge's eyes. **Not** an `Application` callback — see the header.
class RawLog : public FIX::Log {
public:
  void clear() override {}
  void backup() override {}
  void onIncoming(const std::string &raw) override { g_tape.add(raw); }
  void onOutgoing(const std::string &raw) override { g_tape.sent(raw); }
  void onEvent(const std::string &what) override { g_tape.note(what); }
};

class RawLogFactory : public FIX::LogFactory {
public:
  FIX::Log *create() override { return new RawLog(); }
  FIX::Log *create(const FIX::SessionID &) override { return new RawLog(); }
  void destroy(FIX::Log *log) override { delete log; }
};

/// Answers nothing. The scoring is on the tape; this exists because
/// `SocketInitiator` requires one.
class Quiet : public FIX::Application {
public:
  void onCreate(const FIX::SessionID &) override {}
  void onLogon(const FIX::SessionID &id) override {
    std::cout << "interop-acceptor: logged on to " << id.toString() << std::endl;
  }
  void onLogout(const FIX::SessionID &) override {}
  void toAdmin(FIX::Message &, const FIX::SessionID &) override {}
  void toApp(FIX::Message &, const FIX::SessionID &) EXCEPT(FIX::DoNotSend) override {}
  void fromAdmin(const FIX::Message &, const FIX::SessionID &)
      EXCEPT(FIX::FieldNotFound, FIX::IncorrectDataFormat, FIX::IncorrectTagValue,
             FIX::RejectLogon) override {}
  void fromApp(const FIX::Message &, const FIX::SessionID &)
      EXCEPT(FIX::FieldNotFound, FIX::IncorrectDataFormat,
             FIX::UnsupportedMessageType) override {}
};

// ---- the little language every step is written in --------------------------

bool has(const std::string &m, const std::string &what) {
  return m.find(what) != std::string::npos;
}

/// The value of `<tag>=` in a pipe-separated message, or "" when absent.
std::string field(const std::string &m, const std::string &tag) {
  const std::string key = "|" + tag + "=";
  const std::size_t at = m.find(key);
  if (at == std::string::npos) {
    return "";
  }
  const std::size_t from = at + key.size();
  const std::size_t to = m.find('|', from);
  return (to == std::string::npos) ? "" : m.substr(from, to - from);
}

/// Poll the tape until `pred` says yes or `ms` have gone by.
///
/// **Every step has its own deadline and every step prints what it saw.** An
/// acceptor that hangs and an acceptor that refuses look identical to a judge
/// that only waits.
template <typename Pred> bool within(int ms, Pred pred) {
  const auto deadline = Clock::now() + std::chrono::milliseconds(ms);
  for (;;) {
    if (pred()) {
      return true;
    }
    if (Clock::now() >= deadline) {
      return false;
    }
    FIX::process_sleep(0.01);
  }
}

/// The first inbound message matching all of `parts`, or "".
std::string first(const std::vector<std::string> &parts) {
  for (const std::string &m : g_tape.snapshot()) {
    bool all = true;
    for (const std::string &p : parts) {
      if (!has(m, p)) {
        all = false;
        break;
      }
    }
    if (all) {
      return m;
    }
  }
  return "";
}

class Score {
public:
  void step(const std::string &name, bool ok, const std::string &saw) {
    std::cout << "interop-acceptor: " << std::left << std::setw(12) << name
              << (ok ? " ok  " : " FAIL") << "  " << saw << std::endl;
    ++m_total;
    if (ok) {
      ++m_passed;
    }
  }

  bool finish() const {
    const bool all = m_passed == m_total && m_total > 0;
    std::cout << "interop-acceptor: " << (all ? "PASS " : "FAIL ") << m_passed << "/"
              << m_total << std::endl;
    return all;
  }

private:
  int m_passed = 0;
  int m_total = 0;
};

/// Seven steps against this repository's acceptor.
void run(const FIX::SessionID &id, Score &score, bool invert_resend) {
  // What arrives from the other end carries our target in `49=` and us in
  // `56=`. Read from the SessionID rather than written as a literal: a
  // hard-coded `49=` in the sibling role was green for two weeks by
  // coincidence and red the first time it met a second counterparty
  // (docs/reference/a-green-fraction-over-a-scenario-that-never-ran.md).
  const std::string from_them = "|49=" + id.getTargetCompID().getValue() + "|";
  const std::string to_us = "|56=" + id.getSenderCompID().getValue() + "|";

  // ---- 1. Logon ------------------------------------------------------------
  //
  // `ResetOnLogon=Y` on this side, so our Logon carries `141=Y` and a correct
  // acceptor echoes it. Without that assertion the step would pass against an
  // acceptor that silently ignored the reset and kept counting.
  std::string logon;
  const bool logged_on = within(5000, [&] {
    logon = first({"|35=A|", from_them, to_us});
    return !logon.empty();
  });
  const bool reset_echoed = logged_on && has(logon, "|141=Y|");
  score.step("logon", reset_echoed,
             logged_on ? ("35=A " + from_them + " " + to_us + " 141=" +
                          (has(logon, "|141=Y|") ? "Y" : "MISSING"))
                       : "no 35=A within 5 s");

  // The heartbeat interval the ACCEPTOR agreed to, off the wire. Step 3's
  // deadline comes from this and not from this process's own config file: an
  // acceptor throws the counterparty's `108=` back, so the two can differ and
  // a deadline read from the wrong one is a flake waiting for a slow machine.
  const std::string beat = field(logon, "108");
  const int beat_s = beat.empty() ? 30 : std::atoi(beat.c_str());

  // ---- 2. Two orders, and the reports that pair with them -------------------
  //
  // `11=` is echoed by the acceptor's handler and is how a report is paired
  // with the order that asked for it. Their `34=` is what step 5 asks back for.
  for (int i = 1; i <= 2; ++i) {
    FIX44::NewOrderSingle order(FIX::ClOrdID("QF-ORD-" + std::to_string(i)),
                                FIX::Side(FIX::Side_BUY), FIX::TransactTime(),
                                FIX::OrdType(FIX::OrdType_LIMIT));
    order.set(FIX::Symbol("FIXBOLT"));
    order.set(FIX::OrderQty(100));
    order.set(FIX::Price(10.0));
    FIX::Session::sendToTarget(order, id);
  }
  std::string r1;
  std::string r2;
  const bool both = within(5000, [&] {
    r1 = first({"|35=8|", "|11=QF-ORD-1|"});
    r2 = first({"|35=8|", "|11=QF-ORD-2|"});
    return !r1.empty() && !r2.empty();
  });
  const std::string seq1 = field(r1, "34");
  const std::string seq2 = field(r2, "34");
  score.step("order", both && !seq1.empty() && !seq2.empty(),
             both ? ("35=8 at 34=" + seq1 + " and 34=" + seq2 + ", 11= matched")
                  : "35=8 for QF-ORD-1: " + std::string(r1.empty() ? "no" : "yes") +
                        ", for QF-ORD-2: " + std::string(r2.empty() ? "no" : "yes"));

  // ---- 3. A heartbeat nobody asked for --------------------------------------
  //
  // `35=0` with no `112=`. A `35=0` carrying a `112=` it was never given is the
  // silently-wrong shape: a strict counterparty rejects it, a lenient one
  // ignores it, and a test looking only for "a 35=0 arrived" passes on both.
  const int beat_deadline_ms = beat_s * 2000 + 1000;
  const bool beat_seen = within(beat_deadline_ms, [&] {
    for (const std::string &m : g_tape.snapshot()) {
      if (has(m, "|35=0|") && !has(m, "|112=")) {
        return true;
      }
    }
    return false;
  });
  score.step("heartbeat", beat_seen,
             beat_seen ? ("35=0 without 112= within " + std::to_string(beat_deadline_ms / 1000) +
                          " s (108=" + std::to_string(beat_s) + ")")
                       : "no unprompted 35=0 in " + std::to_string(beat_deadline_ms / 1000) + " s");

  // ---- 4. A TestRequest with our own 112= -----------------------------------
  FIX44::TestRequest tr1(FIX::TestReqID("QF-TR-1"));
  FIX::Session::sendToTarget(tr1, id);
  const bool echoed = within(5000, [&] { return !first({"|35=0|", "|112=QF-TR-1|"}).empty(); });
  score.step("testrequest", echoed,
             echoed ? "35=0 112=QF-TR-1" : "no 35=0 with 112=QF-TR-1 within 5 s");

  // ---- 5. ResendRequest, and WHICH messages come back -----------------------
  //
  // **The assertion is the two numbered reports, not "something with 43=Y".**
  // `[measured 2026-09-02]` the sibling role's first version of this step asked
  // only for a `43=Y` and a deliberate reversal — swapping `7=` and `16=` —
  // left it green: the inverted range was answered with a SequenceReset gap
  // fill, which also carries `43=Y`. A legal answer to a question nobody asked
  // passed a test named for the question.
  // docs/reference/a-resend-answer-has-two-legal-shapes.md.
  //
  // `--invert-resend` exists for exactly one reason: to run that reversal
  // against this direction too. It is never passed by a passing run.
  const long a = std::atol(seq1.empty() ? "0" : seq1.c_str());
  const long b = std::atol(seq2.empty() ? "0" : seq2.c_str());
  const long from = invert_resend ? b : a;
  const long to = invert_resend ? a : b;
  FIX44::ResendRequest rr(FIX::BeginSeqNo(static_cast<FIX::SEQNUM>(from)),
                          FIX::EndSeqNo(static_cast<FIX::SEQNUM>(to)));
  FIX::Session::sendToTarget(rr, id);
  const std::string want1 = "|34=" + seq1 + "|";
  const std::string want2 = "|34=" + seq2 + "|";
  std::string got;
  const bool replayed = within(5000, [&] {
    const bool one = !first({"|35=8|", "|43=Y|", "|122=", want1}).empty();
    const bool two = !first({"|35=8|", "|43=Y|", "|122=", want2}).empty();
    got = std::string(one ? seq1 : "") + (one && two ? ", " : "") + (two ? seq2 : "");
    return one && two;
  });
  score.step("resend", replayed,
             "35=8 43=Y replayed at 34=[" + got + "], wanted [" + seq1 + ", " + seq2 + "]");

  // ---- 6. A gap this end opens, and what the acceptor does about it ---------
  //
  // Move our own outbound number forward without saying so, then speak. The
  // acceptor sees a number it did not expect and must ask for what it missed.
  //
  // **The check is that the session survives it**, not that a `35=2` arrived: a
  // gap fill the acceptor mishandled would leave the link up for one more
  // message and then drop it.
  //
  // `[measured 2026-09-04]` **and survival is proven by a message sent AFTER
  // the gap fill, never by the gap-causing one.** The first version of this
  // step asked for an answer to `112=QF-TR-2`, the TestRequest that opened the
  // gap, and read `FAIL 6/7` against an acceptor that was behaving correctly.
  // The wire says why:
  //
  //     out 35=1 34=10 112=QF-TR-2           the gap-causing TestRequest
  //     in  35=2 34=6  7=7 16=0              the acceptor asks, correctly
  //     out 35=4 34=7 43=Y 36=11 123=Y       QuickFIX fills 7 THROUGH 10
  //
  // QuickFIX's own gap fill covers sequence 10, which is the TestRequest
  // itself: it told the acceptor to skip the very message this step was
  // waiting for an answer to. Discarding it is the correct thing for the
  // acceptor to do. So the question moved to one the counterparty cannot
  // retract — a fresh TestRequest, after the reset.
  // docs/reference/a-gap-fill-can-swallow-the-question.md.
  FIX::Session *session = FIX::Session::lookupSession(id);
  bool asked = false;
  bool survived = false;
  if (session != nullptr) {
    session->setNextSenderMsgSeqNum(session->getExpectedSenderNum() + 3);
    FIX44::TestRequest tr2(FIX::TestReqID("QF-TR-2"));
    FIX::Session::sendToTarget(tr2, id);
    asked = within(8000, [&] { return !first({"|35=2|", "|16=0|"}).empty(); });
    FIX44::TestRequest tr3(FIX::TestReqID("QF-TR-3"));
    FIX::Session::sendToTarget(tr3, id);
    survived = within(8000, [&] { return !first({"|35=0|", "|112=QF-TR-3|"}).empty(); });
  }
  score.step("gapfill", asked && survived,
             "35=2 7=" + field(first({"|35=2|"}), "7") + " 16=0 in: " +
                 (asked ? "yes" : "no") + ", then 35=0 112=QF-TR-3: " +
                 (survived ? "yes" : "no"));

  // ---- 7. Logout ------------------------------------------------------------
  if (session != nullptr) {
    session->logout("interop done");
  }
  const bool bye = within(5000, [&] { return !first({"|35=5|", from_them}).empty(); });
  score.step("logout", bye, bye ? "35=5" : "no 35=5 within 5 s");
}

} // namespace

int main(int argc, char **argv) {
  if (argc < 2) {
    std::cerr << "usage: initiator <config file> [--invert-resend]" << std::endl;
    return 2;
  }
  bool invert_resend = false;
  for (int i = 2; i < argc; ++i) {
    if (std::string(argv[i]) == "--invert-resend") {
      invert_resend = true;
    }
  }

  try {
    FIX::SessionSettings settings(argv[1]);
    const std::set<FIX::SessionID> sessions = settings.getSessions();
    if (sessions.empty()) {
      std::cerr << "interop-acceptor: the config names no session" << std::endl;
      return 2;
    }
    const FIX::SessionID id = *sessions.begin();

    Quiet app;
    FIX::FileStoreFactory store(settings);
    RawLogFactory log;
    FIX::SocketInitiator initiator(app, store, settings, log);

    Score score;
    initiator.start();
    run(id, score, invert_resend);
    const bool ok = score.finish();
    if (!ok) {
      g_tape.dump();
    }
    initiator.stop();
    return ok ? 0 : 1;
  } catch (std::exception &e) {
    std::cerr << "interop-acceptor: " << e.what() << std::endl;
    return 1;
  }
}
