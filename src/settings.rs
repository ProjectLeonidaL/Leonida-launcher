//! The launcher's choices, kept in %LOCALAPPDATA%\leonida-launcher\settings.json.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
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
    /// Whether the profiles, switches and extra lines are sent (My own choices) or kept but
    /// left out (Recommended). None in settings saved before the choice existed.
    pub advanced: Option<bool>,
    /// The launcher version the player answered Later to; a newer one shows again.
    pub later_launcher: String,
    /// The same for Leonida's ready-made builds.
    pub later_build: u64,
    /// The leonida.exe in use before the last ready-made build was installed, for going back.
    pub previous_leonida: String,
    /// A newly installed leonida.exe whose shaders aren't warmed up yet, or empty.
    pub warm_pending: String,
}

impl Settings {
    fn path() -> Option<PathBuf> {
        let base = PathBuf::from(env::var_os("LOCALAPPDATA")?);
        Some(base.join("leonida-launcher").join("settings.json"))
    }

    /// The saved settings, and a message for the user when the file couldn't be read.
    pub fn load() -> (Self, Option<String>) {
        match Self::path() {
            Some(path) => Self::load_from(&path),
            None => (Self::default(), None),
        }
    }

    /// A file that doesn't parse is moved aside to settings.json.bad, so the next save doesn't
    /// write over what the user had.
    fn load_from(path: &Path) -> (Self, Option<String>) {
        let Ok(text) = fs::read_to_string(path) else {
            return (Self::default(), None);
        };
        let error = match serde_json::from_str(&text) {
            Ok(settings) => return (settings, None),
            Err(error) => error,
        };
        let bad = path.with_extension("json.bad");
        let kept = match fs::rename(path, &bad) {
            Ok(()) => format!("The old file is kept as {}.", bad.display()),
            Err(rename) => format!("It couldn't be moved aside either ({rename})."),
        };
        let notice = format!(
            "Your saved launcher settings couldn't be read ({error}), so the launcher started \
             with none. {kept}"
        );
        (Self::default(), Some(notice))
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

    /// Whether a run sends the user's own choices. Settings from before the choice existed
    /// always sent their picks, so they keep doing that until the user picks Recommended.
    pub fn advanced(&self) -> bool {
        self.advanced.unwrap_or_else(|| {
            !self.profiles.is_empty() || !self.switches.is_empty() || !self.extra.trim().is_empty()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_keep_sending_their_picks() {
        let old: Settings = serde_json::from_str(r#"{"profiles":["texture-keep"]}"#).unwrap();
        assert!(old.advanced());
        let empty: Settings = serde_json::from_str("{}").unwrap();
        assert!(!empty.advanced());
        let chosen: Settings =
            serde_json::from_str(r#"{"profiles":["texture-keep"],"advanced":false}"#).unwrap();
        assert!(!chosen.advanced());
    }

    #[test]
    fn a_broken_file_is_kept_aside() {
        let dir = env::temp_dir().join("leonida-launcher-settings-test");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let bad = dir.join("settings.json.bad");
        let _ = fs::remove_file(&bad);

        fs::write(&path, r#"{"game_dir":"D:\\Games"}"#).unwrap();
        let (good, notice) = Settings::load_from(&path);
        assert_eq!(good.game_dir, r"D:\Games");
        assert!(notice.is_none());

        fs::write(&path, r#"{"game_dir": "D:\Ga"#).unwrap();
        let (fresh, notice) = Settings::load_from(&path);
        assert!(fresh.game_dir.is_empty());
        assert!(notice.unwrap().contains("settings.json.bad"));
        assert!(!path.exists());
        assert_eq!(fs::read_to_string(&bad).unwrap(), r#"{"game_dir": "D:\Ga"#);

        let (missing, notice) = Settings::load_from(&path);
        assert!(missing.game_dir.is_empty() && notice.is_none());
        fs::remove_dir_all(&dir).unwrap();
    }
}
