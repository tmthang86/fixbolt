import io.aeron.CommonContext;
import io.aeron.archive.Archive;
import io.aeron.archive.ArchiveThreadingMode;
import io.aeron.archive.ArchivingMediaDriver;
import io.aeron.driver.MediaDriver;
import io.aeron.driver.ThreadingMode;
import b3.entrypoint.fixp.sbe.EstablishDecoder;
import b3.entrypoint.fixp.sbe.MessageHeaderDecoder;
import b3.entrypoint.fixp.sbe.NegotiateDecoder;
import b3.entrypoint.fixp.sbe.TerminateDecoder;
import org.agrona.IoUtil;
import org.agrona.concurrent.UnsafeBuffer;
import org.agrona.concurrent.YieldingIdleStrategy;
import uk.co.real_logic.artio.engine.EngineConfiguration;
import uk.co.real_logic.artio.engine.FixEngine;
import uk.co.real_logic.artio.engine.LowResourceEngineScheduler;
import uk.co.real_logic.artio.fixp.FixPConnection;
import uk.co.real_logic.artio.fixp.FixPConnectionHandler;
import uk.co.real_logic.artio.fixp.FixPMessageHeader;
import uk.co.real_logic.artio.library.FixLibrary;
import uk.co.real_logic.artio.library.LibraryConfiguration;
import uk.co.real_logic.artio.library.NotAppliedResponse;
import uk.co.real_logic.artio.messages.DisconnectReason;
import uk.co.real_logic.artio.messages.FixPProtocolType;
import uk.co.real_logic.artio.messages.SessionReplyStatus;
import uk.co.real_logic.artio.Reply;
import uk.co.real_logic.artio.binary_entrypoint.BinaryEntryPointContext;
import uk.co.real_logic.artio.fixp.FixPContext;
import uk.co.real_logic.artio.validation.FixPAuthenticationProxy;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.util.Arrays;
import java.util.Collections;
import java.util.Objects;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * The FIXP spike's referee (ADR-0140 decision 2, docs/plans/2026-09-23-p3-fixp-spike.md row 1).
 *
 * Our own code, Artio's public API only: an in-process Binary EntryPoint acceptor built the way
 * {@code artio-samples/.../FixPExchangeApplication.java} and Artio's own
 * {@code AbstractBinaryEntryPointSystemTest} build one — an {@link ArchivingMediaDriver}, a
 * {@link FixEngine} configured with {@code acceptFixPProtocol(BINARY_ENTRYPOINT)}, and a
 * {@link FixLibrary} in sole-library mode connected over {@link CommonContext#IPC_CHANNEL}.
 *
 * <p>Start headless, print a line once the acceptor is OBSERVED to be listening, run until the
 * script's stop file appears or a deadline this process holds itself, then stop and print that it
 * stopped.
 *
 * <p><strong>The judge (row 3, ADR-0140 decision 2).</strong> The authentication strategy compares
 * all seven fields of the {@link BinaryEntryPointContext} the Negotiate produced — {@code sessionID},
 * {@code sessionVerID}, {@code enteringFirm} and the four {@code varData}: {@code credentials},
 * {@code clientIP}, {@code clientAppName}, {@code clientAppVersion} — with the {@code --expect-*}
 * values on the command line. It prints one line per field, {@code referee: field <name> ok <value>}
 * or {@code referee: field <name> MISMATCH got <value> want <value>}, and <strong>rejects</strong>
 * on any mismatch; Artio then answers with {@code NegotiateReject} code {@code CREDENTIALS}. The
 * value is printed as received so {@code scripts/fixp-spike.sh} can hold it against what the probe
 * was told to send. With no {@code --expect-*} values at all (the {@code referee-only} arm) it
 * refuses every connection: a referee with nothing to compare against must not say yes.
 *
 * <p><strong>The wire tap (review of PR #102).</strong> The context above carries neither
 * {@code onbehalfFirm}, nor the header's {@code blockLength}, nor anything of {@code Establish} or
 * {@code Terminate} — and a width mistake in our encoder hides exactly there: with {@code Firm}
 * encoded as {@code uint16} and small test values the seven context fields still read back right
 * (measured by the reviewer: block length 26 against 28, green). So with {@code --artio-port} the
 * referee listens on {@code --port} itself, relays every byte to Artio on {@code --artio-port},
 * and decodes each client frame on the way through with Real Logic's generated decoders from the
 * pinned codecs jar ({@code NegotiateDecoder}, {@code EstablishDecoder}, {@code TerminateDecoder}),
 * printing {@code referee: wire <Message>.<field> ok <value>} or {@code ... MISMATCH got <value>
 * want <value>} for every field, the header's {@code blockLength}, {@code schemaId} and
 * {@code version}, and the frame's length against the decoder's own end of message. A mismatch in
 * a Negotiate makes the authentication strategy refuse; a mismatch in an Establish or Terminate
 * closes the connection. Timestamps have no expected value — the tap prints them as received
 * ({@code referee: wire Negotiate.timestamp <n>}) and the script holds them against what the probe
 * printed it sent.
 *
 * <p>The limits Artio judges a timestamp and a keep-alive interval by are set explicitly and
 * printed ({@code referee: limits ...}), so a reject the probe sees can be read against them.
 * Artio 0.184's defaults ({@code CommonConfiguration}): sending-time window 2 minutes, counterparty
 * keep-alive 1 ms to 1 minute, the acceptor's own keep-alive 30 s. The timestamp window is checked
 * by the library's connection ({@code InternalBinaryEntryPointConnection.isInvalidTimestamp}), after
 * this strategy accepted — so the {@code reject-timestamp} arm prints seven {@code ok} lines and is
 * still refused.
 *
 * <p><strong>The "listening" line is printed only after {@code FixEngine.launch} has returned</strong>
 * rather than before the bind is attempted — CLAUDE.md §10, an observation rather than a claim.
 * {@code FixEngine.launch} binds the acceptor's {@code ServerSocketChannel} synchronously on the
 * calling thread (verified here: pre-occupying the port makes it throw {@code BindException}
 * straight out of {@code launch}, not asynchronously on a framer thread), so a normal return is
 * itself the observation. A first version of this method instead opened and immediately closed a
 * bare loopback socket to confirm the bind — and that bare connect, carrying no FIXP framing, is
 * itself an ill-formed client: Artio's framer read it as a corrupt Simple Open Framing Header and
 * logged {@code IllegalArgumentException: Unsupported Encoding Type} through the error handler
 * below. Recorded as a trap for row 3, whose arms are real FIXP clients and do not hit it.
 */
public final class Referee
{
    private static final String DEFAULT_HOST = "127.0.0.1";

    public static void main(final String[] args) throws Exception
    {
        final Args a = Args.parse(args);

        System.out.println("referee: aeron-dir " + a.aeronDir);
        System.out.println("referee: archive control port " + a.archiveControlPort
            + ", response port " + a.archiveResponsePort);

        final File aeronDirFile = new File(a.aeronDir);
        // Never left over from a previous run: the trap this row is specifically pinned against
        // (docs/plans row 1's own trap, "Thư mục Aeron / archive còn sót từ lần chạy trước").
        IoUtil.delete(aeronDirFile, true);

        final MediaDriver.Context driverCtx = new MediaDriver.Context()
            .aeronDirectoryName(a.aeronDir)
            .threadingMode(ThreadingMode.SHARED)
            .sharedIdleStrategy(new YieldingIdleStrategy())
            .dirDeleteOnStart(true)
            .warnIfDirectoryExists(false);

        final Archive.Context archiveCtx = new Archive.Context()
            .archiveDir(new File(a.aeronDir, "archive"))
            .controlChannel("aeron:udp?endpoint=" + DEFAULT_HOST + ":" + a.archiveControlPort)
            .replicationChannel("aeron:udp?endpoint=" + DEFAULT_HOST + ":0")
            .threadingMode(ArchiveThreadingMode.SHARED)
            .idleStrategySupplier(YieldingIdleStrategy::new)
            .deleteArchiveOnStart(true);

        ArchivingMediaDriver driver = null;
        FixEngine engine = null;
        FixLibrary library = null;
        Tap tap = null;
        try
        {
            driver = ArchivingMediaDriver.launch(driverCtx, archiveCtx);

            final EngineConfiguration engineConfig = new EngineConfiguration()
                .bindTo(DEFAULT_HOST, a.artioPort > 0 ? a.artioPort : a.port)
                .libraryAeronChannel(CommonContext.IPC_CHANNEL)
                .logFileDir(new File(a.aeronDir, "engine-logs").getPath())
                .deleteLogFileDirOnStart(true)
                .scheduler(new LowResourceEngineScheduler())
                .acceptFixPProtocol(FixPProtocolType.BINARY_ENTRYPOINT)
                .fixPAuthenticationStrategy((context, authProxy) -> judge(a, context, authProxy))
                .errorHandlerFactory(errorBuffer -> Throwable::printStackTrace);
            engineConfig.aeronContext().aeronDirectoryName(a.aeronDir);
            engineConfig.aeronArchiveContext()
                .controlRequestChannel(archiveCtx.controlChannel())
                .controlResponseChannel("aeron:udp?endpoint=" + DEFAULT_HOST + ":" + a.archiveResponsePort);

            engine = FixEngine.launch(engineConfig);
            if (a.artioPort > 0)
            {
                tap = new Tap(a);
                System.out.println("referee: artio on " + DEFAULT_HOST + ":" + a.artioPort);
            }
            System.out.println("referee: listening on " + DEFAULT_HOST + ":" + a.port);

            final LibraryConfiguration libraryConfig = new LibraryConfiguration();
            libraryConfig
                .libraryAeronChannels(Collections.singletonList(CommonContext.IPC_CHANNEL))
                .fixPConnectionExistsHandler((lib, surrogateSessionId, protocol, context) ->
                {
                    // Sole-library mode: the engine offers each accepted connection here, and
                    // this library acquires it; the Negotiate itself is then answered by the
                    // library's connection (InternalBinaryEntryPointConnection.onNegotiate).
                    lib.requestSession(
                        surrogateSessionId, FixLibrary.NO_MESSAGE_REPLAY, FixLibrary.NO_MESSAGE_REPLAY, 5_000);
                    return io.aeron.logbuffer.ControlledFragmentHandler.Action.CONTINUE;
                })
                .fixPConnectionAcquiredHandler(connection ->
                {
                    System.out.println("referee: connection acquired");
                    return LOGGING_HANDLER;
                });
            // Explicit, and printed, rather than inherited: these are what Artio judges the
            // probe's timestamps and keep-alive by (the plan's keep-alive trap).
            libraryConfig
                .sendingTimeWindowInMs(a.sendingTimeWindowMs)
                .minFixPKeepaliveTimeoutInMs(a.minKeepaliveMs)
                .maxFixPKeepaliveTimeoutInMs(a.maxKeepaliveMs)
                .acceptorFixPKeepaliveTimeoutInMs(a.acceptorKeepaliveMs);
            libraryConfig.aeronContext().aeronDirectoryName(a.aeronDir);
            System.out.println("referee: limits sending-time-window=" + a.sendingTimeWindowMs
                + "ms keepalive-min=" + a.minKeepaliveMs + "ms keepalive-max=" + a.maxKeepaliveMs
                + "ms acceptor-keepalive=" + a.acceptorKeepaliveMs + "ms");

            library = FixLibrary.connect(libraryConfig);
            // A client is only served once a library is there to acquire its connection, so the
            // script waits for this line, not for "listening", before it starts the probe.
            System.out.println("referee: library connected");

            final FixLibrary polledLibrary = library;
            final File stopFile = a.stopFile == null ? null : new File(a.stopFile);
            final long deadlineNs = System.nanoTime() + a.deadlineSeconds * 1_000_000_000L;
            while (System.nanoTime() < deadlineNs)
            {
                if (stopFile != null && stopFile.exists())
                {
                    System.out.println("referee: stop file seen");
                    break;
                }
                final int worked = polledLibrary.poll(10);
                if (worked == 0)
                {
                    Thread.sleep(10);
                }
            }
        }
        finally
        {
            closeQuietly(tap);
            // Engine first, then library — the order Artio's own
            // AbstractBinaryEntryPointSystemTest#closeArtio uses.
            closeQuietly(engine);
            closeQuietly(library);
            if (driver != null)
            {
                final String aeronDirectoryName = driver.mediaDriver().aeronDirectoryName();
                closeQuietly(driver::close);
                final File dir = new File(aeronDirectoryName);
                if (dir.exists())
                {
                    IoUtil.delete(dir, false);
                }
            }
        }

        System.out.println("referee: shutdown ok");
    }

    /**
     * The authentication strategy: seven fields compared, one line each, reject on any mismatch
     * (ADR-0140 decision 2). Runs on the engine's thread.
     */
    private static void judge(final Args a, final FixPContext context, final FixPAuthenticationProxy authProxy)
    {
        if (!(context instanceof BinaryEntryPointContext))
        {
            System.out.println("referee: refused a " + context.getClass().getName() + ", not Binary EntryPoint");
            authProxy.reject();
            return;
        }
        final BinaryEntryPointContext c = (BinaryEntryPointContext)context;
        if (!a.hasExpectations())
        {
            System.out.println("referee: no --expect-* values given; refusing " + c);
            authProxy.reject();
            return;
        }

        boolean ok = true;
        if (a.artioPort > 0 && !WIRE_NEGOTIATE_OK.get())
        {
            System.out.println("referee: the wire tap found a Negotiate field wrong; refusing");
            ok = false;
        }
        ok &= field("sessionID", Long.toString(c.sessionID()), a.expectSessionId);
        ok &= field("sessionVerID", Long.toString(c.sessionVerID()), a.expectSessionVerId);
        ok &= field("enteringFirm", Long.toString(c.enteringFirm()), a.expectEnteringFirm);
        ok &= field("credentials", c.credentials(), a.expectCredentials);
        ok &= field("clientIP", c.clientIP(), a.expectClientIp);
        ok &= field("clientAppName", c.clientAppName(), a.expectClientAppName);
        ok &= field("clientAppVersion", c.clientAppVersion(), a.expectClientAppVersion);

        if (ok)
        {
            System.out.println("referee: authentication accepted");
            authProxy.accept();
        }
        else
        {
            System.out.println("referee: authentication rejected");
            authProxy.reject();
        }
    }

    private static boolean field(final String name, final String got, final String want)
    {
        if (Objects.equals(got, want))
        {
            System.out.println("referee: field " + name + " ok " + got);
            return true;
        }
        System.out.println("referee: field " + name + " MISMATCH got " + got + " want " + want);
        return false;
    }

    /** Whether the last Negotiate the tap decoded had every field as expected. */
    private static final AtomicBoolean WIRE_NEGOTIATE_OK = new AtomicBoolean(false);

    private static boolean wire(final String name, final Object got, final Object want)
    {
        final String g = String.valueOf(got);
        final String w = String.valueOf(want);
        if (g.equals(w))
        {
            System.out.println("referee: wire " + name + " ok " + g);
            return true;
        }
        System.out.println("referee: wire " + name + " MISMATCH got " + g + " want " + w);
        return false;
    }

    /**
     * A relay in front of Artio that decodes every client frame with Real Logic's generated
     * decoders and judges it (see the class comment). One accept loop thread; per connection one
     * thread each way, blocking sockets. When one side ends its output, the other side's output is
     * ended too, so the probe still sees Artio's close as EOF.
     */
    private static final class Tap implements AutoCloseable
    {
        private final Args a;
        private final ServerSocket server;
        private final Thread acceptor;

        Tap(final Args a) throws IOException
        {
            this.a = a;
            server = new ServerSocket();
            server.bind(new InetSocketAddress(InetAddress.getByName(DEFAULT_HOST), a.port));
            acceptor = new Thread(this::acceptLoop, "referee-tap");
            acceptor.setDaemon(true);
            acceptor.start();
        }

        private void acceptLoop()
        {
            while (!server.isClosed())
            {
                try
                {
                    final Socket client = server.accept();
                    final Socket artio = new Socket(DEFAULT_HOST, a.artioPort);
                    client.setTcpNoDelay(true);
                    artio.setTcpNoDelay(true);
                    daemon(() -> clientToArtio(client, artio), "referee-tap-in");
                    daemon(() -> artioToClient(artio, client), "referee-tap-out");
                }
                catch (final IOException e)
                {
                    if (!server.isClosed())
                    {
                        System.out.println("referee: tap accept failed " + e);
                    }
                }
            }
        }

        private static void daemon(final Runnable r, final String name)
        {
            final Thread t = new Thread(r, name);
            t.setDaemon(true);
            t.start();
        }

        private static void artioToClient(final Socket artio, final Socket client)
        {
            try (InputStream in = artio.getInputStream())
            {
                final OutputStream out = client.getOutputStream();
                final byte[] buf = new byte[4096];
                int n;
                while ((n = in.read(buf)) >= 0)
                {
                    out.write(buf, 0, n);
                    out.flush();
                }
                client.shutdownOutput();
            }
            catch (final IOException e)
            {
                closeBoth(artio, client);
            }
        }

        private void clientToArtio(final Socket client, final Socket artio)
        {
            try (InputStream in = client.getInputStream())
            {
                final OutputStream out = artio.getOutputStream();
                byte[] pending = new byte[0];
                final byte[] buf = new byte[4096];
                int n;
                while ((n = in.read(buf)) >= 0)
                {
                    pending = concat(pending, buf, n);
                    while (pending.length >= FRAME_LEN)
                    {
                        final int size = (pending[0] & 0xFF) | ((pending[1] & 0xFF) << 8);
                        if (size < FRAME_LEN || pending.length < size)
                        {
                            if (size < FRAME_LEN)
                            {
                                // Not a frame this tap can delimit: pass it on as it is and let
                                // Artio say what it thinks of it.
                                out.write(pending);
                                pending = new byte[0];
                            }
                            break;
                        }
                        final byte[] frame = Arrays.copyOf(pending, size);
                        pending = Arrays.copyOfRange(pending, size, pending.length);
                        final boolean ok = judgeFrame(frame);
                        out.write(frame);
                        out.flush();
                        if (!ok && templateOf(frame) != NegotiateDecoder.TEMPLATE_ID)
                        {
                            // Artio does not authenticate these; the referee refuses by closing.
                            System.out.println("referee: the wire tap found a field wrong; closing");
                            closeBoth(client, artio);
                            return;
                        }
                    }
                }
                artio.shutdownOutput();
            }
            catch (final IOException e)
            {
                closeBoth(client, artio);
            }
        }

        private static byte[] concat(final byte[] a, final byte[] b, final int n)
        {
            final byte[] c = Arrays.copyOf(a, a.length + n);
            System.arraycopy(b, 0, c, a.length, n);
            return c;
        }

        private static int templateOf(final byte[] frame)
        {
            if (frame.length < FRAME_LEN + MessageHeaderDecoder.ENCODED_LENGTH)
            {
                return -1;
            }
            return new MessageHeaderDecoder().wrap(new UnsafeBuffer(frame), FRAME_LEN).templateId();
        }

        /** Decodes one frame with Real Logic's generated code and judges every field. */
        private boolean judgeFrame(final byte[] frame)
        {
            final UnsafeBuffer buffer = new UnsafeBuffer(frame);
            final int encodingType = ((frame[2] & 0xFF) | ((frame[3] & 0xFF) << 8));
            if (encodingType != 0xEB50)
            {
                System.out.println("referee: wire frame encoding type MISMATCH got "
                    + Integer.toHexString(encodingType) + " want eb50");
                return false;
            }
            if (frame.length < FRAME_LEN + MessageHeaderDecoder.ENCODED_LENGTH)
            {
                System.out.println("referee: wire frame of " + frame.length + " bytes has no SBE header");
                return false;
            }
            final MessageHeaderDecoder header = new MessageHeaderDecoder().wrap(buffer, FRAME_LEN);
            final int body = FRAME_LEN + MessageHeaderDecoder.ENCODED_LENGTH;
            final int blockLength = header.blockLength();
            final int version = header.version();
            boolean ok = true;
            switch (header.templateId())
            {
                case NegotiateDecoder.TEMPLATE_ID:
                {
                    final String m = "Negotiate.";
                    ok &= wire(m + "blockLength", blockLength, NegotiateDecoder.BLOCK_LENGTH);
                    ok &= wire(m + "schemaId", header.schemaId(), NegotiateDecoder.SCHEMA_ID);
                    ok &= wire(m + "version", version, NegotiateDecoder.SCHEMA_VERSION);
                    final NegotiateDecoder d = new NegotiateDecoder().wrap(buffer, body, blockLength, version);
                    ok &= wire(m + "sessionID", d.sessionID(), a.expectSessionId);
                    ok &= wire(m + "sessionVerID", Long.toUnsignedString(d.sessionVerID()), a.expectSessionVerId);
                    System.out.println("referee: wire " + m + "timestamp " + Long.toUnsignedString(d.timestamp().time()));
                    ok &= wire(m + "enteringFirm", d.enteringFirm(), a.expectEnteringFirm);
                    ok &= wire(m + "onbehalfFirm", d.onbehalfFirm(), a.expectOnbehalfFirm);
                    ok &= wire(m + "credentials", d.credentials(), a.expectCredentials);
                    ok &= wire(m + "clientIP", d.clientIP(), a.expectClientIp);
                    ok &= wire(m + "clientAppName", d.clientAppName(), a.expectClientAppName);
                    ok &= wire(m + "clientAppVersion", d.clientAppVersion(), a.expectClientAppVersion);
                    ok &= wire(m + "length", frame.length, d.limit());
                    WIRE_NEGOTIATE_OK.set(ok);
                    break;
                }
                case EstablishDecoder.TEMPLATE_ID:
                {
                    final String m = "Establish.";
                    ok &= wire(m + "blockLength", blockLength, EstablishDecoder.BLOCK_LENGTH);
                    ok &= wire(m + "schemaId", header.schemaId(), EstablishDecoder.SCHEMA_ID);
                    ok &= wire(m + "version", version, EstablishDecoder.SCHEMA_VERSION);
                    final EstablishDecoder d = new EstablishDecoder().wrap(buffer, body, blockLength, version);
                    ok &= wire(m + "sessionID", d.sessionID(), a.expectSessionId);
                    ok &= wire(m + "sessionVerID", Long.toUnsignedString(d.sessionVerID()), a.expectSessionVerId);
                    System.out.println("referee: wire " + m + "timestamp " + Long.toUnsignedString(d.timestamp().time()));
                    ok &= wire(m + "keepAliveInterval", Long.toUnsignedString(d.keepAliveInterval().time()), a.expectKeepaliveMs);
                    ok &= wire(m + "nextSeqNo", d.nextSeqNo(), a.expectNextSeqNo);
                    ok &= wire(m + "cancelOnDisconnectType", d.cancelOnDisconnectTypeRaw(), a.expectCodType);
                    ok &= wire(m + "codTimeoutWindow", Long.toUnsignedString(d.codTimeoutWindow().time()), a.expectCodTimeoutMs);
                    ok &= wire(m + "credentials", d.credentials(), a.expectCredentials);
                    ok &= wire(m + "length", frame.length, d.limit());
                    break;
                }
                case TerminateDecoder.TEMPLATE_ID:
                {
                    final String m = "Terminate.";
                    ok &= wire(m + "blockLength", blockLength, TerminateDecoder.BLOCK_LENGTH);
                    ok &= wire(m + "schemaId", header.schemaId(), TerminateDecoder.SCHEMA_ID);
                    ok &= wire(m + "version", version, TerminateDecoder.SCHEMA_VERSION);
                    final TerminateDecoder d = new TerminateDecoder().wrap(buffer, body, blockLength, version);
                    ok &= wire(m + "sessionID", d.sessionID(), a.expectSessionId);
                    ok &= wire(m + "sessionVerID", Long.toUnsignedString(d.sessionVerID()), a.expectSessionVerId);
                    ok &= wire(m + "terminationCode", d.terminationCodeRaw(), a.expectTerminationCode);
                    ok &= wire(m + "length", frame.length, d.limit());
                    break;
                }
                default:
                    System.out.println("referee: wire template " + header.templateId() + " is not one this spike sends");
                    ok = false;
            }
            return ok;
        }

        private static void closeBoth(final Socket x, final Socket y)
        {
            try
            {
                x.close();
            }
            catch (final IOException ignored)
            {
            }
            try
            {
                y.close();
            }
            catch (final IOException ignored)
            {
            }
        }

        public void close() throws IOException
        {
            server.close();
        }
    }

    /** B3's framing header: u16 LE length including itself, u16 LE 0xEB50. */
    private static final int FRAME_LEN = 4;

    private static void closeQuietly(final AutoCloseable closeable)
    {
        if (closeable == null)
        {
            return;
        }
        try
        {
            closeable.close();
        }
        catch (final Exception e)
        {
            e.printStackTrace();
        }
    }

    /** Prints what the library's connection reports; the probe judges the wire itself. */
    private static final FixPConnectionHandler LOGGING_HANDLER = new FixPConnectionHandler()
    {
        public io.aeron.logbuffer.ControlledFragmentHandler.Action onBusinessMessage(
            final FixPConnection connection, final int templateId, final org.agrona.DirectBuffer buffer,
            final int offset, final int blockLength, final int version, final boolean possRetrans,
            final FixPMessageHeader messageHeader)
        {
            return io.aeron.logbuffer.ControlledFragmentHandler.Action.CONTINUE;
        }

        public io.aeron.logbuffer.ControlledFragmentHandler.Action onNotApplied(
            final FixPConnection connection, final long fromSequenceNumber, final long msgCount,
            final NotAppliedResponse response)
        {
            return io.aeron.logbuffer.ControlledFragmentHandler.Action.CONTINUE;
        }

        public io.aeron.logbuffer.ControlledFragmentHandler.Action onRetransmitReject(
            final FixPConnection connection, final String reason, final long requestTimestamp,
            final int errorCodes)
        {
            return io.aeron.logbuffer.ControlledFragmentHandler.Action.CONTINUE;
        }

        public io.aeron.logbuffer.ControlledFragmentHandler.Action onRetransmitTimeout(
            final FixPConnection connection)
        {
            return io.aeron.logbuffer.ControlledFragmentHandler.Action.CONTINUE;
        }

        public io.aeron.logbuffer.ControlledFragmentHandler.Action onSequence(
            final FixPConnection connection, final long nextSeqNo)
        {
            return io.aeron.logbuffer.ControlledFragmentHandler.Action.CONTINUE;
        }

        public io.aeron.logbuffer.ControlledFragmentHandler.Action onError(
            final FixPConnection connection, final Exception ex)
        {
            System.out.println("referee: connection error " + ex);
            return io.aeron.logbuffer.ControlledFragmentHandler.Action.CONTINUE;
        }

        public io.aeron.logbuffer.ControlledFragmentHandler.Action onDisconnect(
            final FixPConnection connection, final DisconnectReason reason)
        {
            System.out.println("referee: disconnected " + reason);
            return io.aeron.logbuffer.ControlledFragmentHandler.Action.CONTINUE;
        }
    };

    private static final class Args
    {
        // No default: scripts/fixp-spike.sh always passes a port the kernel handed out fresh
        // (pick_port there). A hardcoded fallback here previously read 15660, which collides with
        // scripts/interop-qfj.sh's fixed PORT_ACCEPTOR_PLAIN (15660-15663) — a parallel run of
        // this referee and that script's judge could bind the same port. The only default this
        // spike has for the referee's own port is "the caller must say" (see --aeron-dir below).
        int port = -1;
        int archiveControlPort = 10010;
        int archiveResponsePort = 10020;
        int deadlineSeconds = 5;
        String aeronDir;
        String stopFile;
        long sendingTimeWindowMs = 120_000;
        long minKeepaliveMs = 1;
        long maxKeepaliveMs = 60_000;
        long acceptorKeepaliveMs = 30_000;
        String expectSessionId;
        String expectSessionVerId;
        String expectEnteringFirm;
        String expectCredentials;
        String expectClientIp;
        String expectClientAppName;
        String expectClientAppVersion;
        /** Artio's own port when the wire tap sits in front of it on {@link #port}; 0: no tap. */
        int artioPort;
        String expectOnbehalfFirm;
        String expectKeepaliveMs;
        String expectNextSeqNo;
        String expectCodType;
        String expectCodTimeoutMs;
        String expectTerminationCode;

        /** All seven or none: a partial set is a script error, refused at parse time. */
        boolean hasExpectations()
        {
            return expectSessionId != null;
        }

        static Args parse(final String[] argv)
        {
            final Args a = new Args();
            for (int i = 0; i < argv.length; i++)
            {
                final String arg = argv[i];
                switch (arg)
                {
                    case "--port":
                        a.port = Integer.parseInt(argv[++i]);
                        break;
                    case "--archive-control-port":
                        a.archiveControlPort = Integer.parseInt(argv[++i]);
                        break;
                    case "--archive-response-port":
                        a.archiveResponsePort = Integer.parseInt(argv[++i]);
                        break;
                    case "--deadline-seconds":
                        a.deadlineSeconds = Integer.parseInt(argv[++i]);
                        break;
                    case "--aeron-dir":
                        a.aeronDir = argv[++i];
                        break;
                    case "--stop-file":
                        a.stopFile = argv[++i];
                        break;
                    case "--sending-time-window-ms":
                        a.sendingTimeWindowMs = Long.parseLong(argv[++i]);
                        break;
                    case "--keepalive-min-ms":
                        a.minKeepaliveMs = Long.parseLong(argv[++i]);
                        break;
                    case "--keepalive-max-ms":
                        a.maxKeepaliveMs = Long.parseLong(argv[++i]);
                        break;
                    case "--acceptor-keepalive-ms":
                        a.acceptorKeepaliveMs = Long.parseLong(argv[++i]);
                        break;
                    case "--expect-session-id":
                        a.expectSessionId = argv[++i];
                        break;
                    case "--expect-session-ver-id":
                        a.expectSessionVerId = argv[++i];
                        break;
                    case "--expect-entering-firm":
                        a.expectEnteringFirm = argv[++i];
                        break;
                    case "--expect-credentials":
                        a.expectCredentials = argv[++i];
                        break;
                    case "--expect-client-ip":
                        a.expectClientIp = argv[++i];
                        break;
                    case "--expect-client-app-name":
                        a.expectClientAppName = argv[++i];
                        break;
                    case "--expect-client-app-version":
                        a.expectClientAppVersion = argv[++i];
                        break;
                    case "--artio-port":
                        a.artioPort = Integer.parseInt(argv[++i]);
                        break;
                    case "--expect-onbehalf-firm":
                        a.expectOnbehalfFirm = argv[++i];
                        break;
                    case "--expect-keepalive-ms":
                        a.expectKeepaliveMs = argv[++i];
                        break;
                    case "--expect-next-seq-no":
                        a.expectNextSeqNo = argv[++i];
                        break;
                    case "--expect-cod-type":
                        a.expectCodType = argv[++i];
                        break;
                    case "--expect-cod-timeout-ms":
                        a.expectCodTimeoutMs = argv[++i];
                        break;
                    case "--expect-termination-code":
                        a.expectTerminationCode = argv[++i];
                        break;
                    default:
                        throw new IllegalArgumentException("unknown argument: " + arg);
                }
            }
            if (a.port <= 0)
            {
                throw new IllegalArgumentException("--port is required");
            }
            if (a.aeronDir == null)
            {
                throw new IllegalArgumentException("--aeron-dir is required");
            }
            final String[] expectations = {
                a.expectSessionId, a.expectSessionVerId, a.expectEnteringFirm, a.expectCredentials,
                a.expectClientIp, a.expectClientAppName, a.expectClientAppVersion };
            int given = 0;
            for (final String e : expectations)
            {
                if (e != null)
                {
                    given++;
                }
            }
            if (given != 0 && given != expectations.length)
            {
                throw new IllegalArgumentException(
                    "--expect-* values come as all seven or none; got " + given);
            }
            if (a.artioPort > 0)
            {
                final String[] wireExpectations = {
                    a.expectOnbehalfFirm, a.expectKeepaliveMs, a.expectNextSeqNo, a.expectCodType,
                    a.expectCodTimeoutMs, a.expectTerminationCode };
                if (given == 0 || Arrays.asList(wireExpectations).contains(null))
                {
                    throw new IllegalArgumentException(
                        "--artio-port (the wire tap) needs all seven --expect-* values and all six wire ones");
                }
            }
            return a;
        }
    }
}
