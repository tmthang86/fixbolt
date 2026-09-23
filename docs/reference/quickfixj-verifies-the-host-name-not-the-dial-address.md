# QuickFIX/J's initiator verifies the reverse-resolved host name, not the dial address

`[measured 2026-09-23]` found while building the `qfj-acceptor-tls` arm of
`scripts/interop-qfj.sh`,
[ADR-0130](../decisions/ADR-0130-a-jvm-enters-ci-as-a-second-oracle-quickfixj-by-pinned-jar-and-our-own-judge.md)
decision 6. Recorded as **Sửa 1** of
[docs/plans/2026-09-23-p3-quickfixj-interop.md](../plans/2026-09-23-p3-quickfixj-interop.md), a
deviation from that decision's own wording ("an IP SAN of `127.0.0.1`"), accepted by the
manager.

**With `EndpointIdentificationAlgorithm=HTTPS`, QuickFIX/J's initiator checks the certificate
against the *reverse-resolved host name* of the address it dialled, not against the literal
address string it was configured with.** `SocketConnectHost=127.0.0.1` is not what gets checked
— `localhost` is, on any machine where `/etc/hosts` (or the resolver) maps `127.0.0.1` back to
that name, which is the ordinary case.

## The evidence

`javap -p -c` on `quickfix.mina.ssl.InitiatorSslFilter` from `quickfixj-core-3.0.2.jar`
(`vendor/quickfixj/quickfixj-core.jar`, fetched and pinned by `scripts/interop-qfj.sh`):

```
protected javax.net.ssl.SSLEngine createEngine(org.apache.mina.core.session.IoSession, java.net.InetSocketAddress);
    Code:
       0: aload_2
       1: ifnull        23
       4: aload_0
       5: getfield      #3   // Field sslContext:Ljavax/net/ssl/SSLContext;
       8: aload_2
       9: invokevirtual #4   // Method java/net/InetSocketAddress.getHostName:()Ljava/lang/String;
      12: aload_2
      13: invokevirtual #5   // Method java/net/InetSocketAddress.getPort:()I
      16: invokevirtual #6   // Method javax/net/ssl/SSLContext.createSSLEngine:(Ljava/lang/String;I)Ljavax/net/ssl/SSLEngine;
      19: astore_3
      20: goto          31
      ...
```

`SSLContext.createSSLEngine(String, int)` is the two-argument overload the JSSE endpoint
identification algorithm reads its peer host from — the string it is given here is
`InetSocketAddress.getHostName()`. When an `InetSocketAddress` is constructed by IP literal and
port and later asked for its host name, the JDK resolves the address (a reverse DNS / hosts-file
lookup) rather than returning the literal it was built from. On a machine whose resolver answers
`127.0.0.1` with `localhost` — every ordinary Linux desk and CI runner — `getHostName()` returns
`"localhost"`, and that is the string `EndpointIdentificationAlgorithm=HTTPS` checks the
certificate's SAN list against.

## What was seen before the fix

A leaf carrying only `IP:127.0.0.1` (ADR-0130 decision 6's literal wording) failed the
`qfj-acceptor-tls` arm's `logon` step with a JSSE handshake exception naming `localhost`, not
`127.0.0.1` — the certificate was correct for the address dialled and wrong for the name
actually checked.

## The fix

`scripts/interop-qfj.sh`'s `make_pki` gives the `fixbolt-acceptor` leaf **two** SAN entries,
`IP:127.0.0.1,DNS:localhost`, so QuickFIX/J's initiator (checking `localhost`) and fixbolt's own
initiator (which verifies the venue's certificate through `ServerName::IpAddress` against the
literal `127.0.0.1`, `crates/engine/src/tls.rs`) are both satisfied by the same certificate. The
`qfj-acceptor` leaf, which only fixbolt's initiator ever verifies, keeps the IP SAN alone.

## The generalisation

`[to testing-skills]` — **"verify the host" is ambiguous between the string a caller typed and
the string the platform resolves it to, and a Java client using
`InetSocketAddress.getHostName()` after construction from a literal picks the second.** A
certificate generated to match the configuration file's own value can be wrong for exactly this
reason, on a machine where nothing else about the setup is unusual. Any TLS interop harness that
dials `127.0.0.1` (or any other literal) against a JSSE-based initiator with endpoint
identification on should check what the client's `getHostName()` actually returns before
shaping the certificate to the config file's spelling.

## What guards it

The `qfj-acceptor-tls` and `qfj-initiator-tls` arms of `scripts/interop-qfj.sh`, three
consecutive green runs on the desk (`docs/CONFORMANCE.md` §10); a leaf regenerated with only
`IP:127.0.0.1` reproduces the `logon FAIL` this page describes (not re-run as a committed
reversal, since it requires editing the certificate generation in place — the fix itself is the
positive evidence).
