//! The least HTTP/1.1 that Prometheus needs — ADR-0170 decision 6.
//!
//! One request per connection, read into a fixed 4 KiB array; one response,
//! then `Connection: close`. No keep-alive, no chunking, no compression, no
//! TLS, no authentication: the listener binds where the caller says, and
//! `GUIDE.md` says loopback or a private interface.
//!
//! **Nothing here allocates.** The request is parsed in place, and a response
//! header is written into a stack array.

use std::io::{self, Read, Write};
use std::net::TcpStream;

/// The largest request this exporter reads. A request that has not ended its
/// headers within this many bytes is refused, not buffered.
pub(crate) const REQUEST_CAP: usize = 4096;

/// Exactly what Prometheus 3 parses as the classic text format, byte for byte.
/// A near miss — a comma for a semicolon — and the scrape falls back or fails.
pub(crate) const METRICS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

const PLAIN: &str = "text/plain; charset=utf-8";

/// Which endpoint a request asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Route {
    Metrics,
    Healthz,
}

/// A request, judged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Request {
    /// `GET` or `HEAD` (`head: true`) of a known path.
    Serve {
        route: Route,
        head: bool,
    },
    NotFound,
    MethodNotAllowed,
    Bad,
}

/// What reading a request came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Received {
    /// The headers ended; the request line is in the first `n` bytes.
    Complete(usize),
    /// [`REQUEST_CAP`] bytes and still no end of headers.
    TooLarge,
    /// The peer closed, or said nothing before the read timeout, or the socket
    /// failed. There is nobody to answer.
    Gone,
}

/// Read until the blank line that ends the headers, at most [`REQUEST_CAP`]
/// bytes. The socket's read timeout bounds how long a silent client holds the
/// exporter's one thread.
pub(crate) fn read_request(s: &mut TcpStream, buf: &mut [u8; REQUEST_CAP]) -> Received {
    let mut n = 0;
    loop {
        let Some(room) = buf.get_mut(n..) else {
            return Received::TooLarge;
        };
        if room.is_empty() {
            return Received::TooLarge;
        }
        match s.read(room) {
            Ok(0) | Err(_) => return Received::Gone,
            Ok(k) => n += k,
        }
        if buf
            .get(..n)
            .is_some_and(|b| b.windows(4).any(|w| w == b"\r\n\r\n"))
        {
            return Received::Complete(n);
        }
    }
}

/// Judge a request from its request line: `<method> <target> HTTP/1.x`.
pub(crate) fn parse(req: &[u8]) -> Request {
    let line_end = req
        .windows(2)
        .position(|w| w == b"\r\n")
        .unwrap_or(req.len());
    let line = req.get(..line_end).unwrap_or(&[]);
    let mut parts = line.split(|b| *b == b' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Request::Bad;
    };
    if !version.starts_with(b"HTTP/1.") || !target.starts_with(b"/") || method.is_empty() {
        return Request::Bad;
    }
    let path = target.split(|b| *b == b'?').next().unwrap_or(target);
    let route = match path {
        b"/metrics" => Route::Metrics,
        b"/healthz" => Route::Healthz,
        _ => return Request::NotFound,
    };
    match method {
        b"GET" => Request::Serve { route, head: false },
        b"HEAD" => Request::Serve { route, head: true },
        _ => Request::MethodNotAllowed,
    }
}

/// A status this exporter answers with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Ok,
    BadRequest,
    NotFound,
    MethodNotAllowed,
    InternalError,
    Unavailable,
}

impl Status {
    const fn line(self) -> &'static [u8] {
        match self {
            Self::Ok => b"HTTP/1.1 200 OK\r\n",
            Self::BadRequest => b"HTTP/1.1 400 Bad Request\r\n",
            Self::NotFound => b"HTTP/1.1 404 Not Found\r\n",
            Self::MethodNotAllowed => b"HTTP/1.1 405 Method Not Allowed\r\n",
            Self::InternalError => b"HTTP/1.1 500 Internal Server Error\r\n",
            Self::Unavailable => b"HTTP/1.1 503 Service Unavailable\r\n",
        }
    }

    /// The short body a status that is not a scrape carries.
    pub(crate) const fn body(self) -> &'static [u8] {
        match self {
            Self::Ok => b"ok\n",
            Self::BadRequest => b"bad request\n",
            Self::NotFound => b"not found: try /metrics or /healthz\n",
            Self::MethodNotAllowed => b"method not allowed: GET or HEAD\n",
            Self::InternalError => b"internal error\n",
            Self::Unavailable => b"unhealthy\n",
        }
    }
}

/// Write one whole response. `metrics` picks the scrape's `Content-Type`;
/// `head` sends the headers — with the length the body would have — and no
/// body.
pub(crate) fn respond(
    s: &mut TcpStream,
    status: Status,
    metrics: bool,
    body: &[u8],
    head: bool,
) -> io::Result<()> {
    let mut hdr = Header::default();
    hdr.put(status.line());
    hdr.put(b"Content-Type: ");
    hdr.put(if metrics { METRICS_CONTENT_TYPE } else { PLAIN }.as_bytes());
    hdr.put(b"\r\nContent-Length: ");
    hdr.uint(body.len());
    hdr.put(b"\r\n");
    if status == Status::MethodNotAllowed {
        hdr.put(b"Allow: GET, HEAD\r\n");
    }
    hdr.put(b"Connection: close\r\n\r\n");
    s.write_all(hdr.bytes())?;
    if !head {
        s.write_all(body)?;
    }
    s.flush()
}

/// A response header on the stack. 256 bytes holds the longest this module
/// writes several times over; a write past the end is dropped, never grown.
struct Header {
    buf: [u8; 256],
    len: usize,
}

impl Default for Header {
    fn default() -> Self {
        Self {
            buf: [0; 256],
            len: 0,
        }
    }
}

impl Header {
    fn put(&mut self, bytes: &[u8]) {
        let end = self.len.saturating_add(bytes.len());
        if let Some(dst) = self.buf.get_mut(self.len..end) {
            dst.copy_from_slice(bytes);
            self.len = end;
        }
    }

    fn uint(&mut self, mut n: usize) {
        let mut digits = [0u8; 20];
        let mut at = digits.len();
        loop {
            at -= 1;
            if let Some(d) = digits.get_mut(at) {
                *d = b'0' + (n % 10) as u8;
            }
            n /= 10;
            if n == 0 || at == 0 {
                break;
            }
        }
        self.put(digits.get(at..).unwrap_or(&[]));
    }

    fn bytes(&self) -> &[u8] {
        self.buf.get(..self.len).unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_line_is_judged_by_method_then_path() {
        let cases: [(&[u8], Request); 9] = [
            (
                b"GET /metrics HTTP/1.1\r\nHost: x\r\n\r\n",
                Request::Serve {
                    route: Route::Metrics,
                    head: false,
                },
            ),
            (
                b"HEAD /metrics HTTP/1.0\r\n\r\n",
                Request::Serve {
                    route: Route::Metrics,
                    head: true,
                },
            ),
            (
                b"GET /metrics?x=1 HTTP/1.1\r\n\r\n",
                Request::Serve {
                    route: Route::Metrics,
                    head: false,
                },
            ),
            (
                b"GET /healthz HTTP/1.1\r\n\r\n",
                Request::Serve {
                    route: Route::Healthz,
                    head: false,
                },
            ),
            (b"GET / HTTP/1.1\r\n\r\n", Request::NotFound),
            (b"POST /metrics HTTP/1.1\r\n\r\n", Request::MethodNotAllowed),
            (b"GET /metrics\r\n\r\n", Request::Bad),
            (b"GET /metrics HTTP/2\r\n\r\n", Request::Bad),
            (b"nonsense\r\n\r\n", Request::Bad),
        ];
        for (req, want) in cases {
            assert_eq!(parse(req), want, "{}", String::from_utf8_lossy(req));
        }
    }
}
