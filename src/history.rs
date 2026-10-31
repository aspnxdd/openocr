use std::{
    fs::{self, File},
    io::{ErrorKind, Write},
    path::Path,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScreenshotData {
    pub screenshot_path: String,
    pub created_at: u64,
    pub response: String,
}

pub fn save_screenshot(png_bytes: &[u8], response: &str) -> Result<ScreenshotData> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    save_screenshot_at(&home.join(".openocr"), png_bytes, response)
}

fn save_screenshot_at(root: &Path, png_bytes: &[u8], response: &str) -> Result<ScreenshotData> {
    fs::create_dir_all(root).context("Could not create history directory")?;
    // Lock a separate file because history.json is replaced on each save.
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join("history.lock"))
        .context("Could not open history lock")?;
    lock.lock().context("Could not lock history")?;
    let mut history = get_history_at(root)?;

    let screenshot_path = root
        .join("screenshots")
        .join(format!("{}.png", uuid::Uuid::new_v4()));
    let entry = ScreenshotData {
        screenshot_path: screenshot_path.to_string_lossy().into_owned(),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("System clock is before UNIX epoch")?
            .as_secs(),
        response: response.to_string(),
    };
    history.insert(0, entry.clone());

    fs::create_dir_all(root.join("screenshots"))?;
    let mut image = create_private_file(&screenshot_path)?;
    let result = (|| {
        image.write_all(png_bytes)?;
        image.sync_all()?;
        drop(image);
        write_history(&root.join("history.json"), &history)?;
        Ok(entry)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&screenshot_path);
    }
    result
}

fn create_private_file(path: &Path) -> std::io::Result<File> {
    let mut options = File::options();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn write_history(path: &Path, history: &[ScreenshotData]) -> Result<()> {
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut file = create_private_file(&temporary)?;
    let result = (|| {
        serde_json::to_writer_pretty(&mut file, history)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path).context("Could not replace history.json")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn get_history() -> Result<Vec<ScreenshotData>> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    get_history_at(&home.join(".openocr"))
}

fn get_history_at(root: &Path) -> Result<Vec<ScreenshotData>> {
    let data = match fs::read_to_string(root.join("history.json")) {
        Ok(data) => data,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).context("Could not read history.json"),
    };
    serde_json::from_str(&data)
        .context("Could not parse history.json; existing history was not changed")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(std::path::PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("openocr-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn missing_history_is_empty_and_saves_round_trip_newest_first() {
        let root = TestDirectory::new();
        assert!(get_history_at(&root.0).unwrap().is_empty());
        let first = save_screenshot_at(&root.0, b"first png", "first text").unwrap();
        let second = save_screenshot_at(&root.0, b"second png", "second text").unwrap();
        let history = get_history_at(&root.0).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].response, second.response);
        assert_eq!(history[1].response, first.response);
        assert_eq!(
            fs::read(&history[0].screenshot_path).unwrap(),
            b"second png"
        );
        assert_eq!(fs::read(&history[1].screenshot_path).unwrap(), b"first png");
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 3);
    }

    #[test]
    fn invalid_or_unreadable_history_is_not_overwritten() {
        let root = TestDirectory::new();
        let path = root.0.join("history.json");
        for data in ["", "not json", "{}", "[{\"response\":\"incomplete\"}]"] {
            fs::write(&path, data).unwrap();
            assert!(get_history_at(&root.0).is_err());
            assert!(save_screenshot_at(&root.0, b"png", "text").is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), data);
            assert!(!root.0.join("screenshots").exists());
        }
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(get_history_at(&root.0).is_err());
        assert!(save_screenshot_at(&root.0, b"png", "text").is_err());
        assert!(path.is_dir());
    }

    #[test]
    fn failed_screenshot_write_keeps_existing_history() {
        let root = TestDirectory::new();
        fs::write(root.0.join("history.json"), "[]").unwrap();
        fs::write(root.0.join("screenshots"), "not a directory").unwrap();
        assert!(save_screenshot_at(&root.0, b"png", "text").is_err());
        assert_eq!(
            fs::read_to_string(root.0.join("history.json")).unwrap(),
            "[]"
        );
    }

    #[test]
    fn failed_history_commit_removes_temporary_file() {
        let root = TestDirectory::new();
        let path = root.0.join("history.json");
        fs::create_dir(&path).unwrap();
        assert!(write_history(&path, &[]).is_err());
        assert!(path.is_dir());
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn saves_and_temporary_files_keep_sensitive_data_private() {
        use std::os::unix::fs::PermissionsExt;

        let root = TestDirectory::new();
        let temporary = root.0.join("temporary");
        let file = create_private_file(&temporary).unwrap();
        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        let path = root.0.join("history.json");
        fs::write(&path, "[]").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let entry = save_screenshot_at(&root.0, b"private image", "private text").unwrap();
        for path in [&path, Path::new(&entry.screenshot_path)] {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn concurrent_writers_keep_every_capture() {
        let root = TestDirectory::new();
        std::thread::scope(|scope| {
            for index in 0..8 {
                let root = &root.0;
                scope.spawn(move || {
                    save_screenshot_at(root, b"png", &index.to_string()).unwrap();
                });
            }
        });
        let mut responses = get_history_at(&root.0)
            .unwrap()
            .into_iter()
            .map(|entry| {
                assert!(Path::new(&entry.screenshot_path).exists());
                entry.response
            })
            .collect::<Vec<_>>();
        responses.sort();
        assert_eq!(
            responses,
            (0..8).map(|index| index.to_string()).collect::<Vec<_>>()
        );
    }
}
