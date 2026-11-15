//! Finder "Open With" input. Only OS-supplied local .sql URLs enter this queue;
//! the webview cannot ask this command to read an arbitrary filesystem path.
use serde::Serialize;
use std::collections::VecDeque;
#[cfg(test)]
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PENDING_FILES: usize = 16;
pub const OPENED_EVENT: &str = "tiq:sql-files-ready";

#[derive(Clone, Debug, Serialize)]
pub struct OpenedSqlFile {
    pub id: u64,
    pub name: String,
    pub sql: Option<String>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct OpenedSqlFiles {
    pending: Mutex<VecDeque<OpenedSqlFile>>,
    sequence: AtomicU64,
}

impl OpenedSqlFiles {
    pub fn receive(&self, urls: Vec<tauri::Url>) {
        let mut pending = crate::lk(&self.pending);
        for url in urls {
            // Keep a failure visible rather than silently evicting a previously opened file.
            // The last slot is reserved for the overflow message.
            if pending.len() >= MAX_PENDING_FILES - 1 {
                if pending.len() < MAX_PENDING_FILES {
                    pending.push_back(OpenedSqlFile {
                        id: self.next_id(),
                        name: "SQL files".into(),
                        sql: None,
                        error: Some("Too many files opened together. Open the remaining files again after these are loaded.".into()),
                    });
                }
                break;
            }
            let name = url
                .to_file_path()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "SQL file".into());
            let result = read_sql_url(&url);
            let (sql, error) = match result {
                Ok(sql) => (Some(sql), None),
                Err(error) => (None, Some(error)),
            };
            pending.push_back(OpenedSqlFile {
                id: self.next_id(),
                name,
                sql,
                error,
            });
        }
    }

    fn next_id(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn take(&self) -> Vec<OpenedSqlFile> {
        crate::lk(&self.pending).drain(..).collect()
    }
}

fn read_sql_url(url: &tauri::Url) -> Result<String, String> {
    if url.scheme() != "file" {
        return Err("Only local .sql files can be opened.".into());
    }
    let path = url
        .to_file_path()
        .map_err(|_| "Could not read this local file URL.".to_string())?;
    read_sql(&path)
}

fn read_sql(path: &Path) -> Result<String, String> {
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("sql"))
    {
        return Err("Choose a file with the .sql extension.".into());
    }
    // Reject special files before opening: opening a named pipe can otherwise
    // block forever before the byte cap or handle metadata can be checked.
    let path_metadata = std::fs::metadata(path)
        .map_err(|_| "The SQL file could not be found or inspected.".to_string())?;
    if !path_metadata.is_file() {
        return Err("Choose a regular .sql file, not a folder or special file.".into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // O_NONBLOCK also covers replacement with a FIFO between metadata/open.
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|_| {
        "The file could not be opened. Check that it exists and you have permission to read it."
            .to_string()
    })?;
    let metadata = file
        .metadata()
        .map_err(|_| "Could not read the file's details.".to_string())?;
    if !metadata.is_file() {
        return Err("Choose a regular .sql file, not a folder or special file.".into());
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(
            "This SQL file is larger than 4 MiB. Open a smaller script in the editor.".into(),
        );
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "The SQL file could not be read.".to_string())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(
            "This SQL file is larger than 4 MiB. Open a smaller script in the editor.".into(),
        );
    }
    let text = String::from_utf8(bytes).map_err(|_| {
        "This SQL file is not UTF-8 text. Save it as UTF-8 and open it again.".to_string()
    })?;
    Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned())
}

#[tauri::command]
pub fn take_opened_sql_files(state: tauri::State<'_, OpenedSqlFiles>) -> Vec<OpenedSqlFile> {
    state.take()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_start_keeps_multiple_files_until_drained_once() {
        let dir = tempfile::tempdir().unwrap();
        let one = dir.path().join("one script.sql");
        let two = dir.path().join("two.SQL");
        std::fs::write(&one, "\u{feff}DROP TABLE not_executed;\n").unwrap();
        std::fs::write(&two, "SELECT 2;").unwrap();
        let queue = OpenedSqlFiles::default();
        queue.receive(vec![
            tauri::Url::from_file_path(&one).unwrap(),
            tauri::Url::from_file_path(&two).unwrap(),
        ]);
        let files = queue.take();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].name, "one script.sql");
        assert_eq!(files[0].sql.as_deref(), Some("DROP TABLE not_executed;\n"));
        assert_eq!(files[1].sql.as_deref(), Some("SELECT 2;"));
        assert!(files[0].id < files[1].id);
        assert!(queue.take().is_empty());
    }

    #[test]
    fn rejects_non_sql_missing_non_file_and_non_utf8_inputs() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("other.txt");
        std::fs::write(&other, "secret").unwrap();
        assert!(read_sql(&other).unwrap_err().contains(".sql extension"));
        assert!(read_sql(&dir.path().join("missing.sql")).is_err());
        let folder = dir.path().join("folder.sql");
        std::fs::create_dir(&folder).unwrap();
        assert!(read_sql(&folder).unwrap_err().contains("regular .sql file"));
        let invalid = dir.path().join("binary.sql");
        std::fs::write(&invalid, [0xff, 0xfe]).unwrap();
        assert!(read_sql(&invalid).unwrap_err().contains("UTF-8"));
        assert!(
            read_sql_url(&tauri::Url::parse("https://example.com/script.sql").unwrap()).is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_named_pipe_without_waiting_for_a_writer() {
        use std::time::Duration;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pipe.sql");
        assert!(std::process::Command::new("mkfifo").arg(&path).status().unwrap().success());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || { let _ = tx.send(read_sql(&path)); });
        let result = rx.recv_timeout(Duration::from_secs(1))
            .expect("opening a special file must not block");
        assert!(result.unwrap_err().contains("regular .sql file"));
    }

    #[test]
    fn rejects_large_files_without_reading_them_into_the_editor() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("huge.sql");
        File::create(&path)
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        assert!(read_sql(&path).unwrap_err().contains("4 MiB"));
    }

    #[test]
    fn overflow_keeps_first_files_and_reports_the_limit() {
        let queue = OpenedSqlFiles::default();
        queue.receive(
            (0..20)
                .map(|_| tauri::Url::parse("file:///missing.sql").unwrap())
                .collect(),
        );
        let files = queue.take();
        assert_eq!(files.len(), MAX_PENDING_FILES);
        assert!(files
            .last()
            .unwrap()
            .error
            .as_deref()
            .unwrap()
            .contains("Too many files"));
    }
}
