// A minimal FIX 4.4 acceptor built on libquickfix, for one purpose: to be the
// counterparty this repository does not get to have an opinion about.
//
// Phase 1 exit criterion 4. See tools/interop/src/main.rs for what the Rust
// side does and docs/decisions/ADR-0004-bidirectional-engine.md decision 5 for
// why this exists at all.
//
// THIS FILE IS THIS PROJECT'S OWN CODE. Nothing here is copied from QuickFIX;
// it calls QuickFIX's public API, which is what CLAUDE.md §2 rule 9 permits and
// vendor/ being gitignored enforces. It is compiled by scripts/interop.sh and
// by the `interop` CI job, and by nothing else — no Cargo.toml mentions C++.
//
// What it does that a bare acceptor would not:
//
//   * sends two News (35=B) on logon, so the initiator's ResendRequest has real
//     messages to ask back for rather than only a gap to fill;
//   * prints every message it sends and receives, so a failure on the Rust side
//     can be read against what the C++ side actually saw.

#include <quickfix/Application.h>
#include <quickfix/FileStore.h>
#include <quickfix/Message.h>
#include <quickfix/Session.h>
#include <quickfix/SessionSettings.h>
#include <quickfix/SocketAcceptor.h>
#include <quickfix/fix44/News.h>

#include <csignal>
#include <cstdlib>
#include <iostream>
#include <string>

namespace {

volatile std::sig_atomic_t g_stop = 0;

void on_signal(int) { g_stop = 1; }

std::string readable(const std::string &raw) {
  std::string out = raw;
  for (char &c : out) {
    if (c == '\001') {
      c = '|';
    }
  }
  return out;
}

class Counterparty : public FIX::Application {
public:
  void onCreate(const FIX::SessionID &id) override {
    std::cout << "acceptor: created " << id.toString() << std::endl;
  }

  void onLogon(const FIX::SessionID &id) override {
    std::cout << "acceptor: logon " << id.toString() << std::endl;
    // Two application messages, so the initiator has something real to ask for
    // back. A News needs a Headline and one line of text; both are required by
    // FIX44.xml, and the acceptor validates its own outgoing messages against
    // it, so a wrong one would be refused here rather than on the wire.
    for (int i = 1; i <= 2; ++i) {
      FIX44::News news;
      news.set(FIX::Headline("interop-" + std::to_string(i)));
      FIX44::News::NoLinesOfText line;
      line.set(FIX::Text("line " + std::to_string(i)));
      news.addGroup(line);
      FIX::Session::sendToTarget(news, id);
    }
  }

  void onLogout(const FIX::SessionID &id) override {
    std::cout << "acceptor: logout " << id.toString() << std::endl;
  }

  void toAdmin(FIX::Message &m, const FIX::SessionID &) override {
    std::cout << "acceptor: out " << readable(m.toString()) << std::endl;
  }

  void toApp(FIX::Message &m, const FIX::SessionID &) EXCEPT(FIX::DoNotSend) override {
    std::cout << "acceptor: out " << readable(m.toString()) << std::endl;
  }

  void fromAdmin(const FIX::Message &m, const FIX::SessionID &)
      EXCEPT(FIX::FieldNotFound, FIX::IncorrectDataFormat, FIX::IncorrectTagValue,
             FIX::RejectLogon) override {
    std::cout << "acceptor: in  " << readable(m.toString()) << std::endl;
  }

  void fromApp(const FIX::Message &m, const FIX::SessionID &)
      EXCEPT(FIX::FieldNotFound, FIX::IncorrectDataFormat, FIX::UnsupportedMessageType) override {
    std::cout << "acceptor: in  " << readable(m.toString()) << std::endl;
  }
};

} // namespace

int main(int argc, char **argv) {
  if (argc < 2) {
    std::cerr << "usage: acceptor <config file>" << std::endl;
    return 2;
  }
  std::signal(SIGINT, on_signal);
  std::signal(SIGTERM, on_signal);
  try {
    FIX::SessionSettings settings(argv[1]);
    Counterparty app;
    FIX::FileStoreFactory store(settings);
    FIX::SocketAcceptor acceptor(app, store, settings);
    acceptor.start();
    // Printed once the port is listening, so the script waits on a line rather
    // than on a sleep. A sleep long enough to be safe is a slow gate, and a
    // sleep short enough to be quick is a flaky one.
    std::cout << "acceptor: ready" << std::endl;
    while (g_stop == 0) {
      FIX::process_sleep(0.1);
    }
    acceptor.stop();
    std::cout << "acceptor: stopped" << std::endl;
    return 0;
  } catch (std::exception &e) {
    std::cerr << "acceptor: " << e.what() << std::endl;
    return 1;
  }
}
