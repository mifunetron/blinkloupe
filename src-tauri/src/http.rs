//! A minimal blocking HTTP/1.1 client for talking to ComfyUI, one request per connection (`Connection: close`).
//! http:// and https:// (2026-10-14: TLS through native-tls, i.e. Windows' own SChannel and certificate store, so a cloud
//! server's ordinary certificate just works). An address may carry `user:password@` (sent as Basic auth, for password
//! proxies) and a path the server lives under (`https://host/comfy`); otherwise the access token from Settings goes as
//! `Authorization: Bearer` (ComfyUI-Login, token proxies). No redirects.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Mutex;
use std::time::Duration;

pub struct Resp {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Resp {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// A server address, taken apart.
#[derive(Debug, PartialEq, Clone)]
pub struct Target {
    pub tls: bool,
    pub host: String,
    pub port: u16,
    /// the path ComfyUI is served under behind a proxy, "" or "/comfy"
    pub prefix: String,
    /// "user:password" from the address, base64 for a Basic header
    pub basic: Option<String>,
}

impl Target {
    /// The Host header: the port only when it is not the scheme's own; IPv6 in brackets.
    pub fn host_header(&self) -> String {
        let h = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        if self.port == if self.tls { 443 } else { 80 } { h } else { format!("{}:{}", h, self.port) }
    }

    /// The Authorization header's value, if any: the address's user:password, else the Settings token.
    pub fn authorization(&self) -> Option<String> {
        if let Some(b) = &self.basic {
            return Some(format!("Basic {}", b));
        }
        let t = TOKEN.lock().unwrap();
        (!t.is_empty()).then(|| format!("Bearer {}", t))
    }
}

/// Settings > ComfyUI > Access token, set whenever settings load or save. Never logged.
static TOKEN: Mutex<String> = Mutex::new(String::new());

/// Tests that set the token hold this, since tests run in parallel and the token is shared.
#[cfg(test)]
pub static TEST_TOKEN_LOCK: Mutex<()> = Mutex::new(());

pub fn set_token(t: &str) {
    *TOKEN.lock().unwrap() = t.trim().to_string();
}

pub fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            out.push(if i <= c.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

fn unpercent(s: &str) -> String {
    let b = s.as_bytes();
    let hex = |c: u8| (c as char).to_digit(16);
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(hi), Some(lo)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// "https://user:pw@host:8443/comfy" -> its parts. Only http:// and https://.
pub fn parse(base: &str) -> Result<Target, String> {
    let b = base.trim();
    let (tls, rest) = if let Some(r) = b.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = b.strip_prefix("http://") {
        (false, r)
    } else {
        return Err(format!("the address must start with http:// or https:// ({})", redact(base)));
    };
    let (authority, path) = match rest.find(['/', '?', '#']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let (userinfo, hostport) = match authority.rfind('@') {
        Some(i) => (Some(&authority[..i]), &authority[i + 1..]),
        None => (None, authority),
    };
    let (host, port) = if let Some(r) = hostport.strip_prefix('[') {
        let (h, after) = r.split_once(']').ok_or_else(|| format!("bad IPv6 address in {}", redact(base)))?;
        (h.to_string(), after.strip_prefix(':').map(|p| p.to_string()))
    } else {
        match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), Some(p.to_string())),
            None => (hostport.to_string(), None),
        }
    };
    if host.is_empty() {
        return Err(format!("no host in {}", redact(base)));
    }
    let port = match port.filter(|p| !p.is_empty()) {
        Some(p) => p.parse::<u16>().map_err(|_| format!("bad port in {}", redact(base)))?,
        None => if tls { 443 } else { 80 },
    };
    let path = path.split(['?', '#']).next().unwrap_or("");
    Ok(Target {
        tls,
        host,
        port,
        prefix: path.trim_end_matches('/').to_string(),
        basic: userinfo.filter(|u| !u.is_empty()).map(|u| base64(unpercent(u).as_bytes())),
    })
}

/// (host, port) of an address.
pub fn host_port(base: &str) -> Result<(String, u16), String> {
    parse(base).map(|t| (t.host, t.port))
}

/// The address with any user:password@ taken out, for the screen and the log.
pub fn redact(base: &str) -> String {
    let b = base.trim();
    let Some(i) = b.find("://") else { return b.to_string() };
    let rest = &b[i + 3..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    match rest[..end].rfind('@') {
        Some(at) => format!("{}{}", &b[..i + 3], &rest[at + 1..]),
        None => b.to_string(),
    }
}

/// A connection that reads and writes, plain or TLS, and whose read timeout can change (the status socket waits a second
/// at a time). The timeout goes to the socket itself: on Windows one set through a try_clone() handle does not reach it.
pub trait Stream: Read + Write + Send {
    fn set_read_timeout(&self, d: Option<Duration>) -> std::io::Result<()>;
}

impl Stream for TcpStream {
    fn set_read_timeout(&self, d: Option<Duration>) -> std::io::Result<()> {
        TcpStream::set_read_timeout(self, d)
    }
}

impl Stream for native_tls::TlsStream<TcpStream> {
    fn set_read_timeout(&self, d: Option<Duration>) -> std::io::Result<()> {
        self.get_ref().set_read_timeout(d)
    }
}

/// Connects to the target: TCP, then TLS for https.
pub fn connect(t: &Target, timeout: Duration) -> Result<Box<dyn Stream>, String> {
    let addr = (t.host.as_str(), t.port)
        .to_socket_addrs()
        .map_err(|e| format!("resolve {}: {}", t.host, e))?
        .next()
        .ok_or_else(|| format!("no address for {}", t.host))?;
    let s = TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("connect {}: {}", t.host_header(), e))?;
    let _ = s.set_read_timeout(Some(timeout.max(Duration::from_secs(2))));
    let _ = s.set_write_timeout(Some(timeout.max(Duration::from_secs(2))));
    if !t.tls {
        return Ok(Box::new(s));
    }
    let c = native_tls::TlsConnector::new().map_err(|e| format!("TLS: {}", e))?;
    let tls = c.connect(&t.host, s).map_err(|e| format!("TLS with {}: {}", t.host, e))?;
    Ok(Box::new(tls))
}

/// "https://host:port/a/b?q" -> ("https://host:port", "/a/b?q"), the query kept (download links are signed in it).
pub fn split_url(url: &str) -> Result<(String, String), String> {
    let u = url.trim();
    let i = u.find("://").ok_or_else(|| format!("not a web address: {}", redact(u)))? + 3;
    let end = u[i..].find(['/', '?', '#']).map(|e| i + e).unwrap_or(u.len());
    let mut path = u[end..].split('#').next().unwrap_or("").to_string();
    if !path.starts_with('/') {
        path.insert(0, '/');
    }
    Ok((u[..end].to_string(), path))
}

/// Where a redirect goes: an absolute Location, or one relative to the current address's host.
pub fn resolve_location(current: &str, location: &str) -> Result<String, String> {
    if location.contains("://") {
        return Ok(location.to_string());
    }
    let (base, path) = split_url(current)?;
    if location.starts_with('/') {
        return Ok(format!("{}{}", base, location));
    }
    let dir = path.split('?').next().unwrap_or("/").rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    Ok(format!("{}{}/{}", base, dir, location))
}

/// GET a public address (the update check and its download): no access token or Basic auth is ever sent, and up to six
/// redirects are followed (GitHub's release links redirect twice).
pub fn fetch(url: &str, timeout: Duration) -> Result<Resp, String> {
    let mut url = url.trim().to_string();
    for _ in 0..7 {
        let (base, path) = split_url(&url)?;
        let mut t = parse(&base)?;
        t.basic = None;
        t.prefix.clear();
        let mut s = connect(&t, timeout)?;
        let head = format!("GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: BlinkLoupe\r\nAccept: */*\r\nConnection: close\r\n\r\n", path, t.host_header());
        s.write_all(head.as_bytes()).map_err(|e| format!("send: {}", e))?;
        let mut raw = Vec::new();
        let mut buf = [0u8; 65536];
        loop {
            match s.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    raw.extend_from_slice(&buf[..n]);
                    if complete(&raw) {
                        break;
                    }
                }
                Err(_) if complete(&raw) => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(format!("read: {}", e)),
            }
        }
        let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("incomplete response")?;
        let head = String::from_utf8_lossy(&raw[..split]).into_owned();
        let status = head.split_whitespace().nth(1).and_then(|s| s.parse::<u16>().ok()).unwrap_or(0);
        if [301, 302, 303, 307, 308].contains(&status) {
            let loc = head
                .lines()
                .find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case("location")).map(|(_, v)| v.trim().to_string()))
                .ok_or("a redirect with no Location")?;
            url = resolve_location(&url, &loc)?;
            continue;
        }
        return parse_response(&raw);
    }
    Err("too many redirects".into())
}

pub fn request(base: &str, method: &str, path: &str, body: Option<&[u8]>, timeout: Duration) -> Result<Resp, String> {
    request_typed(base, method, path, body, "application/json", timeout)
}

/// A multipart/form-data body: text fields, then one file field. Returns (content type, body).
pub fn multipart(fields: &[(&str, &str)], file_field: &str, file_name: &str, file_mime: &str, file: &[u8]) -> (String, Vec<u8>) {
    let boundary = format!("----loupe{:016x}", crate::comfy::random_seed());
    let mut b = Vec::with_capacity(file.len() + 1024);
    for (k, v) in fields {
        b.extend(format!("--{}\r\nContent-Disposition: form-data; name=\"{}\"\r\n\r\n{}\r\n", boundary, k, v).as_bytes());
    }
    b.extend(
        format!(
            "--{}\r\nContent-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
            boundary, file_field, file_name, file_mime
        )
        .as_bytes(),
    );
    b.extend(file);
    b.extend(format!("\r\n--{}--\r\n", boundary).as_bytes());
    (format!("multipart/form-data; boundary={}", boundary), b)
}

/// The request head: method, the prefixed path, Host, auth, and the body's type and length for anything but GET.
pub fn head(t: &Target, method: &str, path: &str, content_type: &str, body_len: usize) -> String {
    let mut h = format!(
        "{} {}{} HTTP/1.1\r\nHost: {}\r\nUser-Agent: BlinkLoupe\r\nAccept: */*\r\nConnection: close\r\n",
        method,
        t.prefix,
        path,
        t.host_header()
    );
    if let Some(a) = t.authorization() {
        h.push_str(&format!("Authorization: {}\r\n", a));
    }
    if method != "GET" {
        h.push_str(&format!("Content-Type: {}\r\nContent-Length: {}\r\n", content_type, body_len));
    }
    h.push_str("\r\n");
    h
}

pub fn request_typed(base: &str, method: &str, path: &str, body: Option<&[u8]>, content_type: &str, timeout: Duration) -> Result<Resp, String> {
    let t = parse(base)?;
    let mut s = connect(&t, timeout)?;
    let body = body.unwrap_or(&[]);
    s.write_all(head(&t, method, path, content_type, body.len()).as_bytes()).map_err(|e| format!("send: {}", e))?;
    if !body.is_empty() {
        s.write_all(body).map_err(|e| format!("send body: {}", e))?;
    }
    let _ = s.flush();
    let mut raw = Vec::new();
    let mut buf = [0u8; 65536];
    loop {
        match s.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if complete(&raw) {
                    break;
                }
            }
            // some TLS servers drop the connection without saying goodbye; a whole response is still a response
            Err(_) if complete(&raw) => break,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("read: {}", e)),
        }
    }
    parse_response(&raw)
}

/// Whether `raw` already holds the whole response: its Content-Length has arrived, or the last chunk has.
pub fn complete(raw: &[u8]) -> bool {
    let Some(split) = raw.windows(4).position(|w| w == b"\r\n\r\n") else { return false };
    let head = String::from_utf8_lossy(&raw[..split]).to_ascii_lowercase();
    let body = &raw[split + 4..];
    if head.contains("transfer-encoding: chunked") {
        return chunks_done(body);
    }
    head.lines()
        .find_map(|l| l.strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok()))
        .map(|n| body.len() >= n)
        .unwrap_or(false)
}

/// Whether a chunked body has reached its last (zero) chunk, walking the chunk sizes.
fn chunks_done(mut b: &[u8]) -> bool {
    loop {
        let Some(eol) = b.windows(2).position(|w| w == b"\r\n") else { return false };
        let Ok(size) = usize::from_str_radix(String::from_utf8_lossy(&b[..eol]).split(';').next().unwrap_or("").trim(), 16) else {
            return false;
        };
        b = &b[eol + 2..];
        if size == 0 {
            return true;
        }
        if b.len() < size + 2 {
            return false;
        }
        b = &b[size + 2..];
    }
}

pub fn parse_response(raw: &[u8]) -> Result<Resp, String> {
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("incomplete response")?;
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| format!("bad status line: {}", status_line))?;
    let mut chunked = false;
    let mut length: Option<usize> = None;
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            let k = k.trim().to_ascii_lowercase();
            let v = v.trim();
            if k == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked") {
                chunked = true;
            } else if k == "content-length" {
                length = v.parse().ok();
            }
        }
    }
    let rest = &raw[split + 4..];
    let body = if chunked {
        dechunk(rest)?
    } else if let Some(n) = length {
        rest[..n.min(rest.len())].to_vec()
    } else {
        rest.to_vec()
    };
    Ok(Resp { status, body })
}

fn dechunk(mut b: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let eol = b.windows(2).position(|w| w == b"\r\n").ok_or("bad chunk header")?;
        let size_s = String::from_utf8_lossy(&b[..eol]);
        let size = usize::from_str_radix(size_s.split(';').next().unwrap_or("").trim(), 16).map_err(|_| "bad chunk size")?;
        b = &b[eol + 2..];
        if size == 0 {
            return Ok(out);
        }
        if b.len() < size {
            out.extend_from_slice(b);
            return Ok(out);
        }
        out.extend_from_slice(&b[..size]);
        b = if b.len() >= size + 2 { &b[size + 2..] } else { &[] };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_and_redirect_targets() {
        assert_eq!(split_url("https://github.com/a/b/releases/latest/download/latest.json").unwrap(), ("https://github.com".into(), "/a/b/releases/latest/download/latest.json".into()));
        assert_eq!(split_url("https://h:8443?x=1#f").unwrap(), ("https://h:8443".into(), "/?x=1".into()));
        assert_eq!(split_url("http://h").unwrap().1, "/");
        let cur = "https://github.com/o/r/releases/latest/download/latest.json";
        assert_eq!(resolve_location(cur, "https://objects.example/x?sig=1").unwrap(), "https://objects.example/x?sig=1");
        assert_eq!(resolve_location(cur, "/o/r/releases/download/v1/latest.json").unwrap(), "https://github.com/o/r/releases/download/v1/latest.json");
        assert_eq!(resolve_location(cur, "other.json").unwrap(), "https://github.com/o/r/releases/latest/download/other.json");
    }

    #[test]
    fn fetch_follows_redirects_and_never_sends_the_token() {
        use std::net::TcpListener;
        let _g = TEST_TOKEN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_token("secret-comfy-token");
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let srv = std::thread::spawn(move || {
            let mut heads = Vec::new();
            for i in 0..2 {
                let (mut c, _) = l.accept().unwrap();
                let mut buf = [0u8; 4096];
                let n = c.read(&mut buf).unwrap();
                heads.push(String::from_utf8_lossy(&buf[..n]).into_owned());
                let reply = if i == 0 {
                    "HTTP/1.1 302 Found\r\nLocation: /final.json?sig=abc\r\nContent-Length: 0\r\n\r\n".to_string()
                } else {
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".to_string()
                };
                c.write_all(reply.as_bytes()).unwrap();
            }
            heads
        });
        let r = fetch(&format!("http://me:pw@127.0.0.1:{}/latest.json", port), Duration::from_secs(5)).unwrap();
        set_token("");
        let heads = srv.join().unwrap();
        assert_eq!((r.status, r.text().as_str()), (200, "ok"));
        assert!(heads[1].starts_with("GET /final.json?sig=abc HTTP/1.1"), "{}", heads[1]);
        assert!(heads.iter().all(|h| !h.to_ascii_lowercase().contains("authorization")), "{:?}", heads);
    }

    #[test]
    fn hosts() {
        assert_eq!(host_port("http://127.0.0.1:8188").unwrap(), ("127.0.0.1".into(), 8188));
        assert_eq!(host_port("http://localhost:8000/").unwrap(), ("localhost".into(), 8000));
        assert_eq!(host_port("http://box").unwrap(), ("box".into(), 80));
        assert_eq!(host_port("https://x:1").unwrap(), ("x".into(), 1));
        assert!(host_port("ftp://x").is_err() && host_port("127.0.0.1:8188").is_err() && host_port("http://:80").is_err());
    }

    #[test]
    fn addresses() {
        let t = parse("https://pod-8188.proxy.example.net").unwrap();
        assert_eq!((t.tls, t.port, t.prefix.as_str(), t.host_header()), (true, 443, "", "pod-8188.proxy.example.net".to_string()));
        let t = parse("https://me:p%40ss@box.example:8443/comfy/").unwrap();
        assert_eq!((t.host.as_str(), t.port, t.prefix.as_str()), ("box.example", 8443, "/comfy"));
        assert_eq!(t.basic.as_deref(), Some(base64(b"me:p@ss").as_str()), "user:password, percent-decoded");
        assert_eq!(t.host_header(), "box.example:8443");
        let t = parse("http://[::1]:8188").unwrap();
        assert_eq!((t.host.as_str(), t.port, t.host_header()), ("::1", 8188, "[::1]:8188".to_string()));
        assert_eq!(parse("http://h:80/").unwrap().host_header(), "h");
        assert_eq!(redact("https://me:secret@box:8443/comfy"), "https://box:8443/comfy");
        assert_eq!(redact("http://127.0.0.1:8188"), "http://127.0.0.1:8188");
        assert_eq!(redact("http://h/a@b"), "http://h/a@b", "an @ in the path is not a password");
        assert_eq!(unpercent("a%2Fb%zz%"), "a/b%zz%");
    }

    #[test]
    fn auth_headers() {
        let plain = parse("http://h:1").unwrap();
        let with_pw = parse("http://u:p@h:1").unwrap();
        let _lock = TEST_TOKEN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_token("");
        assert_eq!(plain.authorization(), None);
        set_token(" tok123 ");
        assert_eq!(plain.authorization().as_deref(), Some("Bearer tok123"));
        assert_eq!(with_pw.authorization(), Some(format!("Basic {}", base64(b"u:p"))), "the address's password wins");
        let h = head(&parse("https://h/comfy").unwrap(), "POST", "/prompt", "application/json", 7);
        assert!(h.starts_with("POST /comfy/prompt HTTP/1.1\r\nHost: h\r\n"), "{}", h);
        assert!(h.contains("Authorization: Bearer tok123\r\n") && h.contains("Content-Length: 7\r\n"));
        set_token("");
    }

    #[test]
    fn knows_when_a_response_is_whole() {
        assert!(!complete(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nab"));
        assert!(complete(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nabcd"));
        assert!(complete(b"HTTP/1.1 204 No\r\nContent-Length: 0\r\n\r\n"));
        assert!(!complete(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n0\r\n\r\n"), "data that looks like the end");
        assert!(complete(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n0\r\n\r\n\r\n0\r\n\r\n"));
        assert!(!complete(b"HTTP/1.0 200 OK\r\n\r\nuntil the close"));
    }

    /// One request against a stand-in server: the Bearer token, the path prefix and Host arrive, and a response that stops
    /// at its Content-Length is read without waiting for the connection to close.
    #[test]
    fn token_reaches_the_server() {
        use std::net::TcpListener;
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let srv = std::thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            let mut b = [0u8; 2048];
            let n = c.read(&mut b).unwrap();
            c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok").unwrap();
            std::thread::sleep(Duration::from_millis(1500)); // keeps the socket open: the client must not wait for it
            String::from_utf8_lossy(&b[..n]).to_string()
        });
        let _lock = TEST_TOKEN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_token("s3cret");
        let t0 = std::time::Instant::now();
        let r = request(&format!("http://127.0.0.1:{}/sub", port), "GET", "/system_stats", None, Duration::from_secs(5)).unwrap();
        set_token("");
        assert_eq!((r.status, r.text().as_str()), (200, "ok"));
        assert!(t0.elapsed() < Duration::from_millis(1200), "stopped at Content-Length");
        let req = srv.join().unwrap();
        assert!(req.starts_with("GET /sub/system_stats HTTP/1.1"), "{}", req);
        assert!(req.contains("Authorization: Bearer s3cret\r\n") && req.contains(&format!("Host: 127.0.0.1:{}\r\n", port)), "{}", req);
    }

    /// HTTPS against a real certificate (needs the internet): cargo test --release -- --ignored tls_
    #[test]
    #[ignore]
    fn tls_round_trip() {
        let r = request("https://example.com", "GET", "/", None, Duration::from_secs(10)).unwrap();
        assert_eq!(r.status, 200);
        assert!(r.text().contains("Example Domain"));
    }

    #[test]
    fn responses() {
        let r = parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi").unwrap();
        assert_eq!((r.status, r.text().as_str()), (200, "hi"));
        let r = parse_response(b"HTTP/1.1 400 Bad\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n").unwrap();
        assert_eq!((r.status, r.text().as_str()), (400, "abcde"));
    }
}
