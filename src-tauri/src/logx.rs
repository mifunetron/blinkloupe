//! A tiny append-only log in %LOCALAPPDATA%\BlinkLoupe\blinkloupe.log (rotated at 512 KB to .1). UTC timestamps.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOCK: Mutex<()> = Mutex::new(());
const MAX: u64 = 512 * 1024;

pub fn dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("BlinkLoupe")
}

fn path() -> PathBuf {
    dir().join("blinkloupe.log")
}

/// UTC "YYYY-MM-DD HH:MM:SS" from a unix time (civil-from-days, no dependencies).
pub fn utc_stamp(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, rem / 3600, (rem % 3600) / 60, rem % 60)
}

pub fn log(msg: &str) {
    let _g = LOCK.lock();
    let p = path();
    let _ = fs::create_dir_all(dir());
    if fs::metadata(&p).map(|m| m.len() > MAX).unwrap_or(false) {
        let _ = fs::rename(&p, dir().join("loupe.log.1"));
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{}Z {}", utc_stamp(now), msg);
    }
}

#[macro_export]
macro_rules! logf {
    ($($arg:tt)*) => { $crate::logx::log(&format!($($arg)*)) };
}

#[cfg(test)]
mod tests {
    #[test]
    fn stamp() {
        assert_eq!(super::utc_stamp(0), "1970-01-01 00:00:00");
        assert_eq!(super::utc_stamp(1_790_918_891), "2026-10-02 05:28:11");
    }
}
