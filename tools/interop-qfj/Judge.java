// Judge.java — this repository's own interop counterparty and referee against
// QuickFIX/J, for ADR-0097 exit criterion 6 / ADR-0130.
//
// scripts/interop-qfj.sh compiles this with `javac` against five pinned jars
// (never Maven, never Gradle — CLAUDE.md §2 rule 6 kept for the JVM too) and
// runs it once per arm: `qfj-acceptor-plain`, `qfj-acceptor-tls`,
// `qfj-initiator-plain`, `qfj-initiator-tls`. This file calls only QuickFIX/J's
// public API (quickfix.*, quickfix.fix44.MessageFactory) — non-negotiable 9,
// no QuickFIX source is copied, only its jars are fetched at test time into
// gitignored vendor/.
//
// Usage:
//
//     java -cp <jars>:<classes> Judge <initiator|acceptor> <file.cfg> <label> [--invert-resend]
//
// `<initiator|acceptor>` is QuickFIX/J's own role in this run. The seven steps
// below are the same `interop-acceptor:` steps scripts/interop.sh already runs
// against libquickfix (logon, order, heartbeat, testrequest, resend, gapfill,
// logout), and this file plays them the same way regardless of which side of
// the socket it sits on — Judge always drives, fixbolt's `desk::Desk` always
// answers.
//
// EVERY assertion below reads the raw wire bytes this file's own
// `quickfix.Log` recorded (`RawLog`), never `Application.fromApp` /
// `fromAdmin`. That is deliberate and mirrors the C++ side's own `Log`: a
// message-type-aware callback can swallow or reshape a frame before an
// assertion ever sees it, and CLAUDE.md §10 is about exactly that gap between
// "the check saw green" and "the wire agreed".
//
// Nothing here is a claim until scripts/interop-qfj.sh reads it: this process
// prints one line per step (`<label>: <step> ok|FAIL <what was seen>`) and one
// summary line (`<label>: PASS n/7` or `<label>: FAIL n/7`), and the script
// greps every one of them rather than trusting the exit code.
import quickfix.ConfigError;
import quickfix.Connector;
import quickfix.DoNotSend;
import quickfix.FieldNotFound;
import quickfix.FileStoreFactory;
import quickfix.IncorrectDataFormat;
import quickfix.IncorrectTagValue;
import quickfix.Log;
import quickfix.LogFactory;
import quickfix.Message;
import quickfix.MessageStoreFactory;
import quickfix.RejectLogon;
import quickfix.RuntimeError;
import quickfix.Session;
import quickfix.SessionID;
import quickfix.SessionNotFound;
import quickfix.SessionSettings;
import quickfix.SocketAcceptor;
import quickfix.SocketInitiator;
import quickfix.UnsupportedMessageType;
import quickfix.Application;

import java.io.IOException;
import java.net.Socket;
import java.time.Instant;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.function.Predicate;

public final class Judge {

    /** The seven steps, in the order every arm's summary line counts them. */
    private static final String[] STEP_NAMES = {
        "logon", "order", "heartbeat", "testrequest", "resend", "gapfill", "logout",
    };

    private Judge() {
    }

    public static void main(String[] args) {
        if (args.length < 3) {
            System.out.println(
                "qfj: FAIL usage: Judge <initiator|acceptor> <file.cfg> <label> [--invert-resend]");
            System.exit(1);
            return;
        }
        final String role = args[0];
        final String cfgPath = args[1];
        final String label = args[2];
        final boolean invertResend = args.length > 3 && "--invert-resend".equals(args[3]);

        final RawLog rawLog = new RawLog();
        final JudgeApp app = new JudgeApp();

        Connector connector;
        SessionSettings settings;
        try {
            settings = new SessionSettings(cfgPath);
            final MessageStoreFactory storeFactory = new FileStoreFactory(settings);
            final LogFactory logFactory = new RawLogFactory(rawLog);
            // The FIX 4.4 message factory from quickfixj-messages-fix44, referenced by
            // its full name so the wildcard-free imports above never have to choose
            // between it and the `quickfix.MessageFactory` interface it implements.
            final quickfix.MessageFactory messageFactory = new quickfix.fix44.MessageFactory();

            if ("initiator".equals(role)) {
                connector = new SocketInitiator(app, storeFactory, settings, logFactory, messageFactory);
            } else if ("acceptor".equals(role)) {
                connector = new SocketAcceptor(app, storeFactory, settings, logFactory, messageFactory);
            } else {
                System.out.println("qfj: FAIL unknown role " + role + " (initiator | acceptor)");
                System.exit(1);
                return;
            }
            connector.start();
        } catch (ConfigError | RuntimeError e) {
            System.out.println("qfj: FAIL could not start " + role + " from " + cfgPath + ": " + e);
            System.exit(1);
            return;
        }

        // The acceptor's readiness line, printed by a thread that CONNECTED to the
        // port — an observation, not a claim made before the bind even runs. Mirrors
        // tools/interop/src/main.rs::announce_when_listening.
        if ("acceptor".equals(role)) {
            long port = -1;
            try {
                port = settings.getLong("SocketAcceptPort");
            } catch (ConfigError | quickfix.FieldConvertError e) {
                System.out.println("qfj: FAIL could not read SocketAcceptPort from " + cfgPath + ": " + e);
            }
            if (port > 0) {
                announceWhenListening((int) port);
            }
        }

        final Scorer score = new Scorer(label);
        try {
            final SessionID sid = awaitSessionId(app, 5_000);
            if (sid == null) {
                for (String name : STEP_NAMES) {
                    score.step(name, false, "no quickfix.Session was created from " + cfgPath);
                }
            } else {
                runSteps(role, sid, rawLog, score, invertResend);
            }
        } finally {
            try {
                connector.stop();
            } catch (RuntimeException e) {
                System.out.println("qfj: connector.stop() raised " + e);
            }
        }

        final boolean ok = score.finish();
        System.exit(ok ? 0 : 1);
    }

    /** The seven steps, run in order, each printed whether or not the ones before it passed. */
    private static void runSteps(
        String role, SessionID sid, RawLog rawLog, Scorer score, boolean invertResend) {
        final Cursor cur = new Cursor(rawLog);

        final StepResult logon = stepLogon(cur, sid, role);
        score.step("logon", logon.ok, logon.saw);

        final int[] seqs = new int[2];
        final StepResult order = stepOrder(cur, sid, seqs);
        score.step("order", order.ok, order.saw);

        final StepResult heartbeat = stepHeartbeat(cur, heartBtIntOf(logon.saw, 2));
        score.step("heartbeat", heartbeat.ok, heartbeat.saw);

        final StepResult testreq = stepTestRequest(cur, sid);
        score.step("testrequest", testreq.ok, testreq.saw);

        final StepResult resend = order.ok
            ? stepResend(cur, sid, seqs[0], seqs[1], invertResend)
            : new StepResult(false, "no order acknowledgements were recorded to resend");
        score.step("resend", resend.ok, resend.saw);

        final Session session = Session.lookupSession(sid);
        final StepResult gapfill = session != null
            ? stepGapfill(cur, session, sid)
            : new StepResult(false, "no quickfix.Session object for " + sid);
        score.step("gapfill", gapfill.ok, gapfill.saw);

        final StepResult logout = stepLogout(cur, sid);
        score.step("logout", logout.ok, logout.saw);
    }

    // ---- Step 1: logon -----------------------------------------------------
    //
    // The QuickFIX/J side of `qfj-acceptor.cfg` / `qfj-initiator.cfg` carries
    // `ResetOnLogon=Y`, so when Judge plays the initiator its own outbound Logon
    // sets 141=Y — and a counterparty that resets in step echoes it back on its
    // reply. That is checked only in that direction: a Judge acceptor's
    // reply to a fixbolt-initiated Logon carries whatever fixbolt's own
    // settings ask for, not Judge's.
    private static StepResult stepLogon(Cursor cur, SessionID sid, String role) {
        final String them = sid.getTargetCompID();
        final String us = sid.getSenderCompID();
        final String line = cur.await(5_000, l -> l.startsWith("in ") && l.contains("|35=A|"));
        if (line == null) {
            return new StepResult(false, "no 35=A from " + them + " within 5 s");
        }
        final boolean idsOk = line.contains("|49=" + them + "|") && line.contains("|56=" + us + "|");
        final boolean resetOk = !"initiator".equals(role) || line.contains("|141=Y|");
        return new StepResult(
            idsOk && resetOk,
            "35=A 49=" + tag(line, 49) + " 56=" + tag(line, 56)
                + " 141=" + tag(line, 141) + " 108=" + tag(line, 108));
    }

    // ---- Step 2: order ------------------------------------------------------
    //
    // Two NewOrderSingle, ClOrdID QFJ-ORD-1 / QFJ-ORD-2. fixbolt's `desk::Desk`
    // fills both and echoes `11=`; this is what pairs a reply with the order
    // that asked for it, and what step 5 replays by sequence number.
    private static StepResult stepOrder(Cursor cur, SessionID sid, int[] outSeqs) {
        try {
            sendOrder(sid, "QFJ-ORD-1");
            sendOrder(sid, "QFJ-ORD-2");
        } catch (SessionNotFound e) {
            return new StepResult(false, "could not send: " + e);
        }
        final String r1 = cur.await(5_000,
            l -> l.startsWith("in ") && l.contains("|35=8|") && l.contains("|11=QFJ-ORD-1|"));
        final String r2 = cur.await(5_000,
            l -> l.startsWith("in ") && l.contains("|35=8|") && l.contains("|11=QFJ-ORD-2|"));
        final Integer s1 = r1 == null ? null : seqOf(r1);
        final Integer s2 = r2 == null ? null : seqOf(r2);
        if (s1 != null) {
            outSeqs[0] = s1;
        }
        if (s2 != null) {
            outSeqs[1] = s2;
        }
        final boolean ok = s1 != null && s2 != null;
        return new StepResult(ok, "35=8 at 34=[" + s1 + ", " + s2 + "], 11= matched");
    }

    // ---- Step 3: heartbeat ---------------------------------------------------
    //
    // Passive: fixbolt's own periodic Heartbeat, unprompted, inside
    // `2 * HeartBtInt + 1` seconds — `HeartBtInt` read off the wire's own `108=`
    // from step 1, never off the settings file. A `112=` on this message would
    // mean it is answering something rather than firing on its own clock.
    private static StepResult stepHeartbeat(Cursor cur, int heartBtIntSeconds) {
        final long deadlineMs = (2L * heartBtIntSeconds + 1) * 1_000;
        final String line = cur.await(
            deadlineMs, l -> l.startsWith("in ") && l.contains("|35=0|") && !l.contains("|112="));
        return new StepResult(
            line != null,
            (line != null ? "35=0 without 112= within " : "no unsolicited 35=0 within ")
                + (deadlineMs / 1000) + " s");
    }

    // ---- Step 4: testrequest --------------------------------------------------
    private static StepResult stepTestRequest(Cursor cur, SessionID sid) {
        try {
            sendTestRequest(sid, "QFJ-TR-1");
        } catch (SessionNotFound e) {
            return new StepResult(false, "could not send: " + e);
        }
        final String line = cur.await(
            5_000, l -> l.startsWith("in ") && l.contains("|35=0|") && l.contains("|112=QFJ-TR-1|"));
        return new StepResult(line != null, "35=0 with 112=QFJ-TR-1: " + (line != null));
    }

    // ---- Step 5: resend -------------------------------------------------------
    //
    // Asks for exactly the two order acknowledgements step 2 recorded, by
    // sequence number. `--invert-resend` swaps 7=/16= into a backwards range —
    // reversal B — which this assertion must fail rather than accept a
    // different-shaped legal answer for (docs/reference/a-resend-answer-has-two-legal-shapes.md
    // documents the analogous C++ trap).
    private static StepResult stepResend(Cursor cur, SessionID sid, int a, int b, boolean invert) {
        final int begin = invert ? b : a;
        final int end = invert ? a : b;
        try {
            sendResendRequest(sid, begin, end);
        } catch (SessionNotFound e) {
            return new StepResult(false, "could not send: " + e);
        }
        final List<String> replies = cur.collect(
            5_000, l -> l.startsWith("in ") && l.contains("|35=8|") && l.contains("|43=Y|"), 2);
        final List<Integer> seqs = new ArrayList<>();
        for (String l : replies) {
            final Integer s = seqOf(l);
            if (s != null) {
                seqs.add(s);
            }
        }
        Collections.sort(seqs);
        final List<Integer> want = new ArrayList<>(Arrays.asList(a, b));
        Collections.sort(want);
        return new StepResult(
            seqs.equals(want), "35=8 43=Y replayed at 34=" + seqs + ", wanted " + want);
    }

    // ---- Step 6: gapfill -------------------------------------------------------
    //
    // Judge jumps its own outbound sequence number forward by 3 without telling
    // anybody (`Session.setNextSenderMsgSeqNum`, exactly as the plan names it),
    // then speaks. fixbolt sees a number it did not expect and asks for what it
    // missed (`35=2`); QuickFIX/J's own session engine answers that on its own —
    // Judge sends nothing back for the gap — with a SequenceReset-GapFill,
    // because the range was never actually sent. The real assertion is that the
    // session survives: a gap fill the counterparty refused would drop the link
    // rather than answer the TestRequest that follows.
    //
    // **A race lives between here and QuickFIX/J's own engine, and this method
    // used to lose it.** `[measured 2026-09-23]` QuickFIX/J generates and sends
    // its SequenceReset-GapFill on its own session-processing thread, as a
    // direct reaction to the inbound ResendRequest above; this method sends
    // QFJ-TR-3 on Judge's own thread, independently. Nothing serialised the two
    // writes to the same socket, so occasionally QFJ-TR-3 (a real, higher
    // sequence number) reached the wire BEFORE the gap fill that was supposed
    // to precede it — and fixbolt, correctly holding an out-of-order message
    // rather than answering it, never replied within the deadline. The fix is
    // to wait for evidence the gap fill was actually sent — an outbound
    // `35=4` `123=Y` line in this file's own RawLog — before sending anything
    // else; QuickFIX/J's send happens inside the same call stack that handles
    // the ResendRequest, so it is available within milliseconds.
    private static StepResult stepGapfill(Cursor cur, Session session, SessionID sid) {
        try {
            final int n = session.getExpectedSenderNum();
            session.setNextSenderMsgSeqNum(n + 3);
        } catch (IOException e) {
            return new StepResult(false, "could not bump the outbound sequence number: " + e);
        }
        try {
            sendTestRequest(sid, "QFJ-TR-2");
        } catch (SessionNotFound e) {
            return new StepResult(false, "could not send: " + e);
        }
        final boolean sawResend =
            cur.await(8_000, l -> l.startsWith("in ") && l.contains("|35=2|")) != null;
        final boolean sawGapFillSent = !sawResend || cur.await(
            3_000, l -> l.startsWith("out ") && l.contains("|35=4|") && l.contains("|123=Y|")) != null;
        try {
            sendTestRequest(sid, "QFJ-TR-3");
        } catch (SessionNotFound e) {
            return new StepResult(false, "could not send: " + e);
        }
        final boolean survived = cur.await(
            8_000, l -> l.startsWith("in ") && l.contains("|35=0|") && l.contains("|112=QFJ-TR-3|")) != null;
        return new StepResult(
            sawResend && sawGapFillSent && survived,
            "35=2 in: " + (sawResend ? "yes" : "no")
                + ", gap fill sent: " + (sawGapFillSent ? "yes" : "no")
                + ", then 35=0 112=QFJ-TR-3: " + (survived ? "yes" : "no"));
    }

    // ---- Step 7: logout ---------------------------------------------------------
    private static StepResult stepLogout(Cursor cur, SessionID sid) {
        try {
            sendLogout(sid);
        } catch (SessionNotFound e) {
            return new StepResult(false, "could not send: " + e);
        }
        final boolean acked = cur.await(5_000, l -> l.startsWith("in ") && l.contains("|35=5|")) != null;
        return new StepResult(acked, "35=5 from fixbolt: " + acked);
    }

    // ---- Sending a message with raw tag numbers, on purpose --------------------
    //
    // No `quickfix.field.*` import anywhere in this file: every field is set by
    // its numeric tag through `FieldMap.setString`, which both `Message` and its
    // `Header` inherit. That keeps this file independent of exactly which jar
    // ships which generated field class across QuickFIX/J versions, and it is
    // still calling nothing but public API — `Session.sendToTarget` fills in
    // BeginString, the comp IDs, `34=` and `52=` the same way it would for a
    // typed message.

    private static void sendOrder(SessionID sid, String clOrdId) throws SessionNotFound {
        final Message m = new Message();
        m.getHeader().setString(35, "D"); // NewOrderSingle
        m.setString(11, clOrdId); // ClOrdID
        m.setString(21, "1"); // HandlInst — automated, no intervention
        m.setString(55, "EUR/USD"); // Symbol
        m.setString(54, "1"); // Side — Buy
        m.setString(60, nowUtc()); // TransactTime
        m.setString(40, "1"); // OrdType — Market
        m.setString(38, "100"); // OrderQty
        Session.sendToTarget(m, sid);
    }

    private static void sendTestRequest(SessionID sid, String id) throws SessionNotFound {
        final Message m = new Message();
        m.getHeader().setString(35, "1"); // TestRequest
        m.setString(112, id); // TestReqID
        Session.sendToTarget(m, sid);
    }

    private static void sendResendRequest(SessionID sid, int begin, int end) throws SessionNotFound {
        final Message m = new Message();
        m.getHeader().setString(35, "2"); // ResendRequest
        m.setString(7, String.valueOf(begin)); // BeginSeqNo
        m.setString(16, String.valueOf(end)); // EndSeqNo
        Session.sendToTarget(m, sid);
    }

    private static void sendLogout(SessionID sid) throws SessionNotFound {
        final Message m = new Message();
        m.getHeader().setString(35, "5"); // Logout
        m.setString(58, "interop-qfj done"); // Text
        Session.sendToTarget(m, sid);
    }

    private static String nowUtc() {
        return DateTimeFormatter.ofPattern("yyyyMMdd-HH:mm:ss")
            .withZone(ZoneOffset.UTC)
            .format(Instant.now());
    }

    // ---- Reading the raw, pipe-delimited transcript ----------------------------

    /** The value of `|tag=...|` in a readable line, or `null` if it is not there. */
    private static String tag(String line, int t) {
        final String key = "|" + t + "=";
        final int i = line.indexOf(key);
        if (i < 0) {
            return null;
        }
        final int start = i + key.length();
        final int end = line.indexOf('|', start);
        return end < 0 ? null : line.substring(start, end);
    }

    /** The `34=` on a readable line, or `null` if it has none or it does not parse. */
    private static Integer seqOf(String line) {
        final String v = tag(line, 34);
        if (v == null) {
            return null;
        }
        try {
            return Integer.parseInt(v);
        } catch (NumberFormatException e) {
            return null;
        }
    }

    /** `108=` off the step-1 logon line, or `fallback` if it is missing or malformed. */
    private static int heartBtIntOf(String logonLine, int fallback) {
        if (logonLine == null) {
            return fallback;
        }
        final String v = tag(logonLine, 108);
        if (v == null) {
            return fallback;
        }
        try {
            return Integer.parseInt(v);
        } catch (NumberFormatException e) {
            return fallback;
        }
    }

    private static SessionID awaitSessionId(JudgeApp app, long deadlineMs) {
        final long stop = System.currentTimeMillis() + deadlineMs;
        while (System.currentTimeMillis() < stop) {
            final SessionID id = app.sessionId();
            if (id != null) {
                return id;
            }
            sleep(10);
        }
        return null;
    }

    /** `interop-qfj: ready` once something accepts a real TCP connection on `port`. */
    private static void announceWhenListening(int port) {
        final Thread t = new Thread(() -> {
            for (int i = 0; i < 2_000; i++) {
                try (Socket probe = new Socket("127.0.0.1", port)) {
                    System.out.println("interop-qfj: ready");
                    return;
                } catch (IOException e) {
                    sleep(10);
                }
            }
            System.out.println("interop-qfj: FAIL nothing accepted a connection on 127.0.0.1:" + port);
        });
        t.setDaemon(true);
        t.start();
    }

    private static void sleep(long ms) {
        try {
            Thread.sleep(ms);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
    }

    /** One step's outcome: whether it passed, and what to print beside that. */
    private static final class StepResult {
        final boolean ok;
        final String saw;

        StepResult(boolean ok, String saw) {
            this.ok = ok;
            this.saw = saw;
        }
    }

    /** Prints every step as it lands, and the one summary line the script greps. */
    private static final class Scorer {
        private final String label;
        private int total;
        private int passed;

        Scorer(String label) {
            this.label = label;
        }

        void step(String name, boolean ok, String saw) {
            total++;
            if (ok) {
                passed++;
            }
            System.out.printf("%s: %-11s %s  %s%n", label, name, ok ? "ok  " : "FAIL", saw);
        }

        boolean finish() {
            if (passed == total && total > 0) {
                System.out.println(label + ": PASS " + passed + "/" + total);
                return true;
            }
            System.out.println(label + ": FAIL " + passed + "/" + total);
            return false;
        }
    }

    /**
     * A one-way, growing view of {@link RawLog}'s lines, read forward only.
     *
     * <p>Each step gets a fresh {@link #await} / {@link #collect} call starting
     * from where the last one left off, the same shape
     * {@code tools/interop/src/main.rs}'s {@code read_until} uses over the wire
     * directly — so an old line from a previous step is never re-matched by a
     * later one that happens to share a pattern.
     */
    private static final class Cursor {
        private final RawLog log;
        private int at;

        Cursor(RawLog log) {
            this.log = log;
            this.at = log.snapshot().size();
        }

        /** Block until a line satisfying {@code want} appears, or {@code deadlineMs} passes. */
        String await(long deadlineMs, Predicate<String> want) {
            final long stop = System.currentTimeMillis() + deadlineMs;
            while (System.currentTimeMillis() < stop) {
                final List<String> all = log.snapshot();
                while (at < all.size()) {
                    final String line = all.get(at++);
                    if (want.test(line)) {
                        return line;
                    }
                }
                sleep(10);
            }
            return null;
        }

        /** Up to `n` lines satisfying {@code want} within {@code deadlineMs}, in arrival order. */
        List<String> collect(long deadlineMs, Predicate<String> want, int n) {
            final long stop = System.currentTimeMillis() + deadlineMs;
            final List<String> found = new ArrayList<>();
            while (System.currentTimeMillis() < stop && found.size() < n) {
                final List<String> all = log.snapshot();
                while (at < all.size()) {
                    final String line = all.get(at++);
                    if (want.test(line)) {
                        found.add(line);
                    }
                }
                if (found.size() < n) {
                    sleep(10);
                }
            }
            return found;
        }
    }

    /**
     * QuickFIX/J's {@link Log}, kept as this repository's own: every raw string
     * {@code onIncoming} / {@code onOutgoing} sees is what every step above
     * judges on, and nothing here reads a parsed field off a typed message.
     *
     * <p>One instance is shared by every {@link SessionID} this process ever
     * opens — always exactly one, since each arm dials or listens for a single
     * counterparty — so {@link RawLogFactory} hands back the same object
     * regardless of which session QuickFIX/J asks a log for.
     */
    private static final class RawLog implements Log {
        private final List<String> lines = new ArrayList<>();
        private final Object lock = new Object();

        @Override
        public void clear() {
            // Never called by anything this file relies on; a step's Cursor tracks
            // its own position rather than trusting the log to stay put.
        }

        @Override
        public void onIncoming(String message) {
            record("in", message);
        }

        @Override
        public void onOutgoing(String message) {
            record("out", message);
        }

        @Override
        public void onEvent(String text) {
            System.out.println("qfj: event " + text);
        }

        @Override
        public void onErrorEvent(String text) {
            System.out.println("qfj: error " + text);
        }

        private void record(String direction, String raw) {
            final String readable = readable(raw);
            final String line = direction + " " + readable;
            synchronized (lock) {
                lines.add(line);
            }
            System.out.println("qfj: " + direction + "  " + readable);
        }

        List<String> snapshot() {
            synchronized (lock) {
                return new ArrayList<>(lines);
            }
        }

        private static String readable(String raw) {
            return "|" + raw.replace('\u0001', '|');
        }
    }

    private static final class RawLogFactory implements LogFactory {
        private final RawLog log;

        RawLogFactory(RawLog log) {
            this.log = log;
        }

        @Override
        public Log create(SessionID sessionID) {
            return log;
        }
    }

    /**
     * The counterparty QuickFIX/J drives on our behalf. Every judgement in this
     * file reads {@link RawLog} instead, so this class exists only to learn the
     * {@link SessionID} QuickFIX/J assigned — available from {@link #onCreate},
     * before any connection is even attempted.
     */
    private static final class JudgeApp implements Application {
        private volatile SessionID sessionId;

        @Override
        public void onCreate(SessionID sessionID) {
            sessionId = sessionID;
        }

        @Override
        public void onLogon(SessionID sessionID) {
            sessionId = sessionID;
        }

        @Override
        public void onLogout(SessionID sessionID) {
            // Judged on the wire (step 7), not here.
        }

        @Override
        public void toAdmin(Message message, SessionID sessionID) {
        }

        @Override
        public void fromAdmin(Message message, SessionID sessionID)
            throws FieldNotFound, IncorrectDataFormat, IncorrectTagValue, RejectLogon {
        }

        @Override
        public void toApp(Message message, SessionID sessionID) throws DoNotSend {
        }

        @Override
        public void fromApp(Message message, SessionID sessionID)
            throws FieldNotFound, IncorrectDataFormat, IncorrectTagValue, UnsupportedMessageType {
        }

        SessionID sessionId() {
            return sessionId;
        }
    }
}
