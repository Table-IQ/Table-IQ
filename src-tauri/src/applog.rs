//! The application log: one file under `app_dir()/logs`, written with std only.
//!
//! Deliberately not a logging crate. The app writes a few dozen lines a day — connect
//! failures, refused host keys, panics, front-end errors — and a framework (plus its
//! dependency tree) would be more machinery than the job needs.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, Once, OnceLock};

use crate::lk;

const LOG_NAME: &str = "tableiq.log";
/// Rotate past 5 MB, keeping exactly one previous file (`tableiq.log.1`).
const MAX_BYTES: u64 = 5 << 20;

static LOG: OnceLock<Mutex<File>> = OnceLock::new();
static INIT: Once = Once::new();

/// Open `<dir>/tableiq.log` for appending, first rotating the current file to
/// `tableiq.log.1` when it has grown past the cap.
fn open_log(dir: &Path) -> std::io::Result<File> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(LOG_NAME);
    if std::fs::metadata(&path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        std::fs::rename(&path, dir.join(format!("{LOG_NAME}.1")))?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

/// Open the log file, install the panic hook and the SQL log's sqlx capture
/// (`sql_log::init_tracing`). Idempotent; call it from every entry point.
pub fn init() {
    INIT.call_once(|| {
        crate::sql_log::init_tracing();
        match open_log(&crate::appdir::app_dir().join("logs")) {
            Ok(f) => {
                let _ = LOG.set(Mutex::new(f));
            }
            // Nowhere to log the fact that we cannot log.
            Err(e) => eprintln!("[tableiq] cannot open the log file: {e}"),
        }
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let payload = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "panic".to_string());
            let at = info
                .location()
                .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                .unwrap_or_else(|| "unknown location".to_string());
            log("panic", &format!("{payload} at {at}"));
            previous(info);
        }));
    });
}

/// Append one timestamped line. Before `init` (tests, the bridge's early startup) it goes to
/// stderr instead, so a message is never simply dropped.
pub fn log(level: &str, msg: &str) {
    let text = line(level, msg, now_secs());
    match LOG.get() {
        Some(f) => {
            let _ = lk(f).write_all(text.as_bytes());
        }
        None => eprint!("{text}"),
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// One log line, newline included. Pure so the format is unit-testable.
fn line(level: &str, msg: &str, unix_secs: i64) -> String {
    format!("{} {level} {}\n", utc_stamp(unix_secs), msg.replace('\n', " "))
}

/// `YYYY-MM-DDTHH:MM:SSZ` from unix seconds — Howard Hinnant's civil-from-days. std has no
/// date formatting and the app takes no date crate, so the ~10 lines live here.
fn utc_stamp(unix_secs: i64) -> String {
    let (days, secs) = (unix_secs.div_euclid(86_400), unix_secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        (secs / 60) % 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_is_rfc3339_utc() {
        assert_eq!(utc_stamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc_stamp(1_758_585_600), "2025-09-23T00:00:00Z");
        assert_eq!(utc_stamp(1_758_585_600 + 3661), "2025-09-23T01:01:01Z");
    }

    #[test]
    fn line_is_timestamped_labelled_and_single_line() {
        assert_eq!(line("error", "boom", 0), "1970-01-01T00:00:00Z error boom\n");
        // A multi-line message must not break the one-record-per-line shape.
        assert_eq!(line("info", "a\nb", 0), "1970-01-01T00:00:00Z info a b\n");
    }

    #[test]
    fn opens_for_append_and_creates_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs");
        writeln!(open_log(&logs).unwrap(), "one").unwrap();
        writeln!(open_log(&logs).unwrap(), "two").unwrap();
        let body = std::fs::read_to_string(logs.join(LOG_NAME)).unwrap();
        assert_eq!(body, "one\ntwo\n", "the second open must append, not truncate");
    }

    #[test]
    fn rotates_once_past_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(LOG_NAME), vec![b'x'; (MAX_BYTES + 1) as usize]).unwrap();
        let mut f = open_log(dir.path()).unwrap();
        writeln!(f, "fresh").unwrap();
        assert_eq!(
            std::fs::metadata(dir.path().join("tableiq.log.1")).unwrap().len(),
            MAX_BYTES + 1,
            "the oversized log must be kept as .1"
        );
        assert_eq!(std::fs::read_to_string(dir.path().join(LOG_NAME)).unwrap(), "fresh\n");
    }
}
