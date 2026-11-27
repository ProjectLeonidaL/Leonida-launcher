//! The launcher's choices, kept in %LOCALAPPDATA%\leonida-launcher\settings.json.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::{env, fs, io};

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub game_dir: String,
    pub leonida_exe: String,
    pub profiles: Vec<String>,
    /// Switch name without the LEONIDA_ prefix, and its value.
    pub switches: BTreeMap<String, String>,
    pub extra: String,
}

impl Settings {
    fn path() -> Option<PathBuf> {
        let base = PathBuf::from(env::var_os("LOCALAPPDATA")?);
        Some(base.join("leonida-launcher").join("settings.json"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(
            path,
            serde_json::to_string_pretty(self).map_err(io::Error::other)?,
        )
    }
}
