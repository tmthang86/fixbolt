import io.aeron.CommonContext;
import io.aeron.archive.Archive;
import io.aeron.archive.ArchiveThreadingMode;
import io.aeron.archive.ArchivingMediaDriver;
import io.aeron.driver.MediaDriver;
import io.aeron.driver.ThreadingMode;
import org.agrona.IoUtil;
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

import java.io.File;
import java.util.Collections;

/**
 * The FIXP spike's referee (ADR-0140 decision 2, docs/plans/2026-09-23-p3-fixp-spike.md row 1).
 *
 * Our own code, Artio's public API only: an in-process Binary EntryPoint acceptor built the way
 * {@code artio-samples/.../FixPExchangeApplication.java} and Artio's own
 * {@code AbstractBinaryEntryPointSystemTest} build one — an {@link ArchivingMediaDriver}, a
 * {@link FixEngine} configured with {@code acceptFixPProtocol(BINARY_ENTRYPOINT)}, and a
 * {@link FixLibrary} in sole-library mode connected over {@link CommonContext#IPC_CHANNEL}.
 *
 * <p>Row 1 only: start headless, print a line once the acceptor is OBSERVED to be listening, run
 * until a deadline this process holds itself, then stop and print that it stopped. No client
 * drives it yet — that is row 3, which also adds per-field comparison against values passed on
 * the command line (ADR-0140 decision 2); the authentication strategy below accepts
 * unconditionally because there is nothing yet to compare a field against.
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
        try
        {
            driver = ArchivingMediaDriver.launch(driverCtx, archiveCtx);

            final EngineConfiguration engineConfig = new EngineConfiguration()
                .bindTo(DEFAULT_HOST, a.port)
                .libraryAeronChannel(CommonContext.IPC_CHANNEL)
                .logFileDir(new File(a.aeronDir, "engine-logs").getPath())
                .deleteLogFileDirOnStart(true)
                .scheduler(new LowResourceEngineScheduler())
                .acceptFixPProtocol(FixPProtocolType.BINARY_ENTRYPOINT)
                // Row 1 has no client yet, so nothing to compare a field against: accept
                // whatever a future arm sends. Row 3 replaces this with the per-field judge
                // ADR-0140 decision 2 describes, fed by values passed on the command line.
                .fixPAuthenticationStrategy((context, authProxy) -> authProxy.accept())
                .errorHandlerFactory(errorBuffer -> Throwable::printStackTrace);
            engineConfig.aeronContext().aeronDirectoryName(a.aeronDir);
            engineConfig.aeronArchiveContext()
                .controlRequestChannel(archiveCtx.controlChannel())
                .controlResponseChannel("aeron:udp?endpoint=" + DEFAULT_HOST + ":" + a.archiveResponsePort);

            engine = FixEngine.launch(engineConfig);
            System.out.println("referee: listening on " + DEFAULT_HOST + ":" + a.port);

            final LibraryConfiguration libraryConfig = new LibraryConfiguration()
                .libraryAeronChannels(Collections.singletonList(CommonContext.IPC_CHANNEL))
                .fixPConnectionExistsHandler((lib, surrogateSessionId, protocol, context) ->
                {
                    // Row 1 has no client, so this is never invoked; row 3's arms are what
                    // exercise it and finish the reply this call starts.
                    lib.requestSession(
                        surrogateSessionId, FixLibrary.NO_MESSAGE_REPLAY, FixLibrary.NO_MESSAGE_REPLAY, 5_000);
                    return io.aeron.logbuffer.ControlledFragmentHandler.Action.CONTINUE;
                })
                .fixPConnectionAcquiredHandler(connection -> NO_OP_HANDLER);
            libraryConfig.aeronContext().aeronDirectoryName(a.aeronDir);

            library = FixLibrary.connect(libraryConfig);

            final FixLibrary polledLibrary = library;
            final long deadlineNs = System.nanoTime() + a.deadlineSeconds * 1_000_000_000L;
            while (System.nanoTime() < deadlineNs)
            {
                final int worked = polledLibrary.poll(10);
                if (worked == 0)
                {
                    Thread.sleep(10);
                }
            }
        }
        finally
        {
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

    private static final FixPConnectionHandler NO_OP_HANDLER = new FixPConnectionHandler()
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
            return io.aeron.logbuffer.ControlledFragmentHandler.Action.CONTINUE;
        }

        public io.aeron.logbuffer.ControlledFragmentHandler.Action onDisconnect(
            final FixPConnection connection, final DisconnectReason reason)
        {
            return io.aeron.logbuffer.ControlledFragmentHandler.Action.CONTINUE;
        }
    };

    private static final class Args
    {
        int port = 15660;
        int archiveControlPort = 10010;
        int archiveResponsePort = 10020;
        int deadlineSeconds = 5;
        String aeronDir;

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
                    default:
                        throw new IllegalArgumentException("unknown argument: " + arg);
                }
            }
            if (a.aeronDir == null)
            {
                throw new IllegalArgumentException("--aeron-dir is required");
            }
            return a;
        }
    }
}
