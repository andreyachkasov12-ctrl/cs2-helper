use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAPS: [&str; 10] = [
    "Mirage", "Dust 2", "Inferno", "Nuke", "Ancient", "Anubis", "Overpass", "Vertigo", "Train", "Другая",
];

/// (название для людей, внутренний код)
pub const KINDS: [(&str, &str); 4] = [
    ("Смок", "smoke"),
    ("Флешка", "flashbang"),
    ("Граната", "hegrenade"),
    ("Молотов", "fire"),
];

pub const BUTTONS: [&str; 3] = ["ЛКМ", "ПКМ", "ЛКМ + ПКМ"];
pub const MOVES: [&str; 3] = ["Стоя", "Шаг (shift)", "Бег"];

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub map: String,
    pub kind: String,
    pub title: String,
    pub from: String,
    pub to: String,
    pub button: String,
    pub jump: bool,
    pub crouch: bool,
    pub movement: String,
    pub notes: String,
    pub setpos: String,
    /// Имена файлов картинок в папке images.
    pub images: Vec<String>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Library {
    pub entries: Vec<Entry>,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn unique() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Папка с данными программы: %APPDATA%\CS2Helper на Windows.
pub fn data_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("CS2Helper")
}

pub fn images_dir() -> PathBuf {
    data_dir().join("images")
}

pub fn image_path(name: &str) -> PathBuf {
    images_dir().join(name)
}

fn library_file() -> PathBuf {
    data_dir().join("library.json")
}

pub fn load() -> Library {
    fs::read_to_string(library_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

impl Library {
    pub fn save(&self) -> Result<(), String> {
        fs::create_dir_all(data_dir()).map_err(|e| format!("Не могу создать папку данных: {e}"))?;
        let json = serde_json::to_string_pretty(self).map_err(|e| format!("Ошибка сохранения: {e}"))?;
        fs::write(library_file(), json).map_err(|e| format!("Не могу записать библиотеку: {e}"))
    }
}

/// Копирует выбранную картинку в папку программы и возвращает её имя.
pub fn add_image_from_file(src: &Path) -> Result<String, String> {
    let ext = src
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| "png".to_string());
    let name = format!("img_{}.{}", unique(), ext);
    fs::create_dir_all(images_dir()).map_err(|e| format!("Не могу создать папку картинок: {e}"))?;
    fs::copy(src, images_dir().join(&name)).map_err(|e| format!("Не удалось скопировать картинку: {e}"))?;
    Ok(name)
}

/// Берёт картинку из буфера обмена (например, после Win+Shift+S) и сохраняет как PNG.
pub fn add_image_from_clipboard() -> Result<String, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Буфер обмена недоступен: {e}"))?;
    let img = clipboard.get_image().map_err(|_| {
        "В буфере обмена нет картинки. Сделай скриншот (Win+Shift+S) и нажми кнопку ещё раз.".to_string()
    })?;
    let rgba = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned())
        .ok_or_else(|| "Не удалось прочитать картинку из буфера.".to_string())?;
    let name = format!("img_{}.png", unique());
    fs::create_dir_all(images_dir()).map_err(|e| format!("Не могу создать папку картинок: {e}"))?;
    rgba.save(images_dir().join(&name))
        .map_err(|e| format!("Не удалось сохранить картинку: {e}"))?;
    Ok(name)
}
