use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScreenshotData {
    pub screenshot_path: String,
    pub created_at: u64,
    pub response: String,
}

pub fn save_screenshot(image: &image::DynamicImage, response: &str) -> Result<()> {
    let entry_id = uuid::Uuid::new_v4().to_string();

    let home = dirs::home_dir().context("Could not determine home directory")?;

    let screenshot_path_path_buf = home
        .join(".openocr")
        .join("screenshots")
        .join(format!("{}.png", entry_id));

    let screenshot_path = screenshot_path_path_buf.to_string_lossy().to_string();

    let entry = ScreenshotData {
        screenshot_path: screenshot_path.clone(),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("System clock is before UNIX epoch")?
            .as_secs(),
        response: response.to_string(),
    };

    let db_path = home.join(".openocr").join("history.json");

    let mut history = {
        let data = std::fs::read_to_string(&db_path).unwrap_or_default();
        serde_json::from_str::<Vec<ScreenshotData>>(&data).unwrap_or_default()
    };

    history.insert(0, entry);

    std::fs::create_dir_all(
        db_path
            .parent()
            .context("history.json path has no parent directory")?,
    )?;
    std::fs::write(&db_path, serde_json::to_string_pretty(&history)?)?;

    std::fs::create_dir_all(
        screenshot_path_path_buf
            .parent()
            .context("screenshot path has no parent directory")?,
    )?;
    image.save(&screenshot_path_path_buf)?;
    Ok(())
}

pub fn get_history() -> Result<Vec<ScreenshotData>> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    let db_path = home.join(".openocr").join("history.json");
    let data = std::fs::read_to_string(&db_path).unwrap_or_default();
    serde_json::from_str::<Vec<ScreenshotData>>(&data).map_err(|e| anyhow::anyhow!(e))
}
