//! Just enough of a WebSocket client (RFC 6455) to hear ComfyUI's `status` broadcast (2026-10-14, phase 2: following any
//! ComfyUI). ws:// or wss:// over the same connection http.rs makes for the base address, so TLS, a path prefix and the
//! Authorization header come along; no extensions. Server frames arrive unmasked; the only frames sent are pongs, masked as
//! the spec requires.

use crate::http::{self, base64};
use std::io::{ErrorKind, Read, Write};
use std::time::Duration;

const MAX_FRAME: usize = 64 << 20;

pub enum Msg {
    Text(String),
    Binary,
}

pub struct Ws {
    s: Box<dyn http::Stream>,
    buf: Vec<u8>,
    /// a fragmented message being assembled: (its opcode, the payload so far)
    frag: Option<(u8, Vec<u8>)>,
}

fn random_bytes() -> [u8; 16] {
    let (a, b) = (crate::comfy::random_seed(), crate::comfy::random_seed());
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&a.to_le_bytes());
    out[8..].copy_from_slice(&b.to_le_bytes());
    out
}

/// One whole frame at the start of `b`: (fin, opcode, payload, bytes used); None while it is still incomplete.
pub fn parse_frame(b: &[u8]) -> Result<Option<(bool, u8, Vec<u8>, usize)>, String> {
    if b.len() < 2 {
        return Ok(None);
    }
    let (fin, op, masked) = (b[0] & 0x80 != 0, b[0] & 0x0F, b[1] & 0x80 != 0);
    let (len, mut at) = match b[1] & 0x7F {
        126 if b.len() >= 4 => (u16::from_be_bytes([b[2], b[3]]) as usize, 4),
        127 if b.len() >= 10 => (u64::from_be_bytes(b[2..10].try_into().unwrap()) as usize, 10),
        126 | 127 => return Ok(None),
        n => (n as usize, 2),
    };
    if len > MAX_FRAME {
        return Err(format!("a {} MB websocket frame", len >> 20));
    }
    let mask = if masked {
        if b.len() < at + 4 {
            return Ok(None);
        }
        at += 4;
        Some([b[at - 4], b[at - 3], b[at - 2], b[at - 1]])
    } else {
        None
    };
    if b.len() < at + len {
        return Ok(None);
    }
    let mut payload = b[at..at + len].to_vec();
    if let Some(m) = mask {
        payload.iter_mut().enumerate().for_each(|(i, x)| *x ^= m[i % 4]);
    }
    Ok(Some((fin, op, payload, at + len)))
}

/// A masked client frame (only control frames are sent, so the payload is under 126 bytes).
pub fn client_frame(op: u8, payload: &[u8]) -> Vec<u8> {
    let p = &payload[..payload.len().min(125)];
    let m = random_bytes();
    let mut f = vec![0x80 | op, 0x80 | p.len() as u8, m[0], m[1], m[2], m[3]];
    f.extend(p.iter().enumerate().map(|(i, x)| x ^ m[i % 4]));
    f
}

impl Ws {
    /// Opens `path` (e.g. "/ws?clientId=...") on the server at the http:// or https:// `base`.
    pub fn connect(base: &str, path: &str, timeout: Duration) -> Result<Ws, String> {
        let t = http::parse(base)?;
        let mut s = http::connect(&t, timeout)?;
        let _ = s.set_read_timeout(Some(timeout));
        let auth = t.authorization().map(|a| format!("Authorization: {}\r\n", a)).unwrap_or_default();
        let req = format!(
            "GET {}{} HTTP/1.1\r\nHost: {}\r\nUser-Agent: BlinkLoupe\r\n{}Upgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: {}\r\nSec-WebSocket-Version: 13\r\n\r\n",
            t.prefix,
            path,
            t.host_header(),
            auth,
            base64(&random_bytes())
        );
        s.write_all(req.as_bytes()).map_err(|e| format!("send: {}", e))?;
        let mut head = Vec::new();
        let mut tmp = [0u8; 4096];
        let end = loop {
            let n = s.read(&mut tmp).map_err(|e| format!("handshake: {}", e))?;
            if n == 0 {
                return Err("the server closed the connection during the handshake".into());
            }
            head.extend_from_slice(&tmp[..n]);
            if let Some(i) = head.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
            if head.len() > 16 << 10 {
                return Err("no end to the handshake reply".into());
            }
        };
        let status = String::from_utf8_lossy(&head[..end]).lines().next().unwrap_or("").to_string();
        if !status.contains(" 101") {
            return Err(format!("no websocket here ({})", status.trim()));
        }
        Ok(Ws { s, buf: head[end..].to_vec(), frag: None })
    }

    /// The next whole message, Ok(None) when `wait` passes without one, Err when the connection is gone.
    pub fn next(&mut self, wait: Duration) -> Result<Option<Msg>, String> {
        let _ = self.s.set_read_timeout(Some(wait.max(Duration::from_millis(10))));
        loop {
            while let Some((fin, op, payload, used)) = parse_frame(&self.buf)? {
                self.buf.drain(..used);
                match op {
                    0x9 => {
                        self.s.write_all(&client_frame(0xA, &payload)).map_err(|e| format!("pong: {}", e))?;
                    }
                    0xA => {}
                    0x8 => return Err("the server closed the websocket".into()),
                    0x1 | 0x2 if fin => return Ok(Some(message(op, payload))),
                    0x1 | 0x2 => self.frag = Some((op, payload)),
                    0x0 => {
                        let (fop, mut acc) = self.frag.take().ok_or("a continuation frame with nothing to continue")?;
                        acc.extend(payload);
                        if acc.len() > MAX_FRAME {
                            return Err("an oversized websocket message".into());
                        }
                        if fin {
                            return Ok(Some(message(fop, acc)));
                        }
                        self.frag = Some((fop, acc));
                    }
                    _ => return Err(format!("websocket opcode {}", op)),
                }
            }
            let mut tmp = [0u8; 16384];
            match self.s.read(&mut tmp) {
                Ok(0) => return Err("the websocket closed".into()),
                Ok(n) => self.buf.extend_from_slice(&tmp[..n]),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => return Ok(None),
                Err(e) => return Err(format!("websocket: {}", e)),
            }
        }
    }
}

fn message(op: u8, payload: Vec<u8>) -> Msg {
    if op == 0x1 {
        Msg::Text(String::from_utf8_lossy(&payload).into_owned())
    } else {
        Msg::Binary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0u8; 16]).len(), 24);
    }

    #[test]
    fn frames() {
        // a short unmasked text frame, then part of the next one
        let mut b = vec![0x81, 5];
        b.extend(b"hello");
        b.extend([0x81, 126]);
        let (fin, op, p, used) = parse_frame(&b).unwrap().unwrap();
        assert!(fin && op == 1 && p == b"hello" && used == 7);
        assert!(parse_frame(&b[used..]).unwrap().is_none(), "the 16-bit length has not arrived");
        // a 300-byte frame with a 16-bit length
        let mut big = vec![0x82, 126, 1, 44];
        big.extend(vec![7u8; 300]);
        let (_, op, p, used) = parse_frame(&big).unwrap().unwrap();
        assert!(op == 2 && p.len() == 300 && used == 304);
        assert!(parse_frame(&big[..200]).unwrap().is_none());
        // what we send unmasks back to what we meant
        let f = client_frame(0xA, b"ping-data");
        let (fin, op, p, _) = parse_frame(&f).unwrap().unwrap();
        assert!(fin && op == 0xA && p == b"ping-data");
        assert!(f[1] & 0x80 != 0, "client frames are masked");
        // an absurd length is refused rather than waited for
        let mut huge = vec![0x82, 127];
        huge.extend((1u64 << 40).to_be_bytes());
        assert!(parse_frame(&huge).is_err());
    }

    /// A stand-in server: the handshake carries the path prefix and the token, a status message comes through, a quiet
    /// socket gives Ok(None), and a close is an error.
    #[test]
    fn handshake_and_messages() {
        use std::net::TcpListener;
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let srv = std::thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            let mut b = [0u8; 2048];
            let n = c.read(&mut b).unwrap();
            let req = String::from_utf8_lossy(&b[..n]).to_string();
            let msg = br#"{"type": "status", "data": {"status": {"exec_info": {"queue_remaining": 0}}}}"#;
            let mut out = b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n".to_vec();
            out.extend([0x81, msg.len() as u8]);
            out.extend(msg);
            c.write_all(&out).unwrap();
            std::thread::sleep(Duration::from_millis(400));
            c.write_all(&[0x88, 0]).unwrap(); // close
            std::thread::sleep(Duration::from_millis(200));
            req
        });
        let _lock = http::TEST_TOKEN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        http::set_token("wstok");
        let mut w = Ws::connect(&format!("http://127.0.0.1:{}/comfy", port), "/ws?clientId=t", Duration::from_secs(3)).unwrap();
        http::set_token("");
        match w.next(Duration::from_secs(2)).unwrap() {
            Some(Msg::Text(t)) => assert!(t.contains("queue_remaining")),
            _ => panic!("no status message"),
        }
        let quiet = w.next(Duration::from_millis(100));
        assert!(matches!(quiet, Ok(None)), "quiet socket: {:?}", quiet.err());
        assert!(w.next(Duration::from_secs(2)).is_err(), "closed by the server");
        let req = srv.join().unwrap();
        assert!(req.starts_with("GET /comfy/ws?clientId=t HTTP/1.1\r\n"), "{}", req);
        assert!(req.contains("Authorization: Bearer wstok\r\n") && req.contains("Upgrade: websocket\r\n"), "{}", req);
    }
}
