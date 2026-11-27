//! Finding leonida.exe and the profiles of the checkout it was built in.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
pub struct Profile {
    pub name: String,
    pub opt_in: bool,
    pub measured: String,
    pub local: bool,
    #[serde(rename = "match")]
    pub matcher: Matcher,
}

/// The card a profile is meant for, in gpu-profiles.json's form. Empty fits every card.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Matcher {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_vram_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_vram_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The graphics card Leonida reports (`config --graphics`).
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Card {
    pub name: String,
    pub vendor: String,
    pub vram_mb: u64,
}

impl Matcher {
    pub fn is_empty(&self) -> bool {
        self.vendor.is_none()
            && self.min_vram_mb.is_none()
            && self.max_vram_mb.is_none()
            && self.name.is_none()
    }

    /// Whether `card` fits, the way Leonida's graphics.rs decides it.
    pub fn fits(&self, card: &Card) -> bool {
        self.vendor
            .as_ref()
            .is_none_or(|vendor| vendor.eq_ignore_ascii_case(&card.vendor))
            && self.min_vram_mb.is_none_or(|min| card.vram_mb >= min)
            && self.max_vram_mb.is_none_or(|max| card.vram_mb <= max)
            && self
                .name
                .as_ref()
                .is_none_or(|pattern| wildcard(pattern, &card.name))
    }

    /// "8 GB NVIDIA cards", for someone who doesn't know the profile names. It says which
    /// cards the profile is for, all of them, since it matches every card that fits.
    pub fn describe(&self) -> String {
        let vendor = match self
            .vendor
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("nvidia") => "NVIDIA ",
            Some("amd") => "AMD ",
            Some("intel") => "Intel ",
            _ => "",
        };
        let gb = |mb: u64| (mb + 512) / 1024;
        match (self.min_vram_mb, self.max_vram_mb) {
            (Some(min), Some(max)) if gb(min) == gb(max) || max - min < 2048 => {
                format!("{} GB {vendor}cards", gb(max))
            }
            (Some(min), Some(max)) => {
                format!("{vendor}cards with {} to {} GB", gb(min), gb(max))
            }
            (Some(min), None) => format!("{vendor}cards with {} GB or more", gb(min)),
            (None, Some(max)) => format!("{vendor}cards with up to {} GB", gb(max)),
            (None, None) if vendor.is_empty() => "cards like yours".into(),
            (None, None) => format!("{vendor}cards"),
        }
    }
}

/// `*` for any run of characters, `?` for one, ignoring case.
fn wildcard(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// leonida.exe beside this launcher (or in its target\release), otherwise the newest
/// target\release\leonida.exe in a folder of the user's Downloads.
pub fn find() -> Option<PathBuf> {
    let here = env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let beside = here.into_iter().flat_map(|dir| {
        [
            dir.join("leonida.exe"),
            dir.join(r"target\release\leonida.exe"),
        ]
    });
    if let Some(exe) = beside.into_iter().find(|exe| exe.is_file()) {
        return Some(exe);
    }
    let downloads = PathBuf::from(env::var_os("USERPROFILE")?).join("Downloads");
    fs::read_dir(downloads)
        .ok()?
        .flatten()
        .map(|entry| entry.path().join(r"target\release\leonida.exe"))
        .filter_map(|exe| Some((fs::metadata(&exe).ok()?.modified().ok()?, exe)))
        .max()
        .map(|(_, exe)| exe)
}

/// The folder leonida.exe runs in: the checkout for a target\<profile>\leonida.exe, otherwise
/// the exe's own folder. leonida.toml and gpu-profiles.local.json are read from there.
pub fn folder(exe: &Path) -> PathBuf {
    let Some(dir) = exe.parent() else {
        return PathBuf::new();
    };
    let named = |p: Option<&Path>, names: &[&str]| {
        p.and_then(Path::file_name)
            .is_some_and(|n| names.iter().any(|name| n.eq_ignore_ascii_case(name)))
    };
    if named(Some(dir), &["release", "diag", "debug"])
        && named(dir.parent(), &["target"])
        && let Some(root) = dir.parent().and_then(Path::parent)
    {
        return root.to_path_buf();
    }
    dir.to_path_buf()
}

/// Whether a leonida.exe is running already; two at once fight over the shader cache.
pub fn running() -> bool {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    Command::new("tasklist.exe")
        .args(["/FI", "IMAGENAME eq leonida.exe", "/NH", "/FO", "CSV"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains("\"leonida.exe\""))
}

#[derive(Deserialize)]
struct Catalog {
    #[serde(default)]
    profiles: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    name: String,
    #[serde(default)]
    opt_in: bool,
    #[serde(default)]
    measured: String,
    #[serde(default, rename = "match")]
    matcher: Matcher,
}

/// The profiles in gpu-profiles.json (the checkout's, otherwise `downloaded`) and
/// gpu-profiles.local.json in `folder`, base left out because Leonida always applies it.
pub fn profiles(folder: &Path, downloaded: Option<&str>) -> Vec<Profile> {
    let read = |file: &str| fs::read_to_string(folder.join(file)).ok();
    let catalog = read("gpu-profiles.json").or_else(|| downloaded.map(str::to_string));
    let mut profiles: Vec<Profile> = Vec::new();
    for (text, local) in [(catalog, false), (read("gpu-profiles.local.json"), true)] {
        let Some(text) = text else { continue };
        let Ok(catalog) = serde_json::from_str::<Catalog>(&text) else {
            continue;
        };
        for entry in catalog.profiles {
            if entry.name == "base" || profiles.iter().any(|p| p.name == entry.name) {
                continue;
            }
            let Entry {
                name,
                opt_in,
                measured,
                matcher,
            } = entry;
            profiles.push(Profile {
                name,
                opt_in,
                measured,
                local,
                matcher,
            });
        }
    }
    profiles
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, vendor: &str, vram_mb: u64) -> Card {
        Card {
            name: name.into(),
            vendor: vendor.into(),
            vram_mb,
        }
    }

    #[test]
    fn profiles_fit_cards_like_leonida_decides() {
        let eight: Matcher = serde_json::from_str(
            r#"{"vendor": "nvidia", "min_vram_mb": 7000, "max_vram_mb": 8600}"#,
        )
        .unwrap();
        assert!(eight.fits(&card("NVIDIA GeForce RTX 3060 Ti", "nvidia", 8192)));
        assert!(!eight.fits(&card("NVIDIA GeForce RTX 3080", "nvidia", 12288)));
        assert!(!eight.fits(&card("AMD Radeon RX 6600", "amd", 8176)));
        assert_eq!(eight.describe(), "8 GB NVIDIA cards");
        let nvidia: Matcher = serde_json::from_str(r#"{"vendor": "nvidia"}"#).unwrap();
        assert_eq!(nvidia.describe(), "NVIDIA cards");
        let named: Matcher = serde_json::from_str(r#"{"name": "*RTX 40*"}"#).unwrap();
        assert!(named.fits(&card("NVIDIA GeForce RTX 4070", "nvidia", 12282)));
        assert!(!named.fits(&card("NVIDIA GeForce RTX 3070", "nvidia", 8192)));
        assert!(Matcher::default().is_empty());
        assert_eq!(Matcher::default().describe(), "cards like yours");
    }
}
