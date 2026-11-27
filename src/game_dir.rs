//! Finding the folder of the player's copy of the game, the one with MicrosoftGame.config in
//! it. Same rules as scripts/game-dir.ps1 in the Leonida repository.

use std::collections::{HashSet, VecDeque};
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{env, fs};

const CONFIG: &str = "MicrosoftGame.config";
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

pub struct Found {
    pub dir: PathBuf,
    pub name: String,
}

pub struct Search {
    pub games: Vec<Found>,
    pub timed_out: bool,
}

pub fn has_config(dir: &Path) -> bool {
    !dir.as_os_str().is_empty() && dir.join(CONFIG).is_file()
}

/// The game folder for a picked path: that folder, the folder of a file in it, a folder up to
/// three levels above, or its Content subfolder (where the Xbox app installs games).
pub fn resolve(path: &Path) -> Option<PathBuf> {
    let mut dir = if path.is_file() {
        path.parent()?.to_path_buf()
    } else {
        path.to_path_buf()
    };
    let content = dir.join("Content");
    if has_config(&content) {
        return Some(content);
    }
    for _ in 0..4 {
        if has_config(&dir) {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

/// The name a game folder's MicrosoftGame.config shows, or the folder's own name.
pub fn game_name(dir: &Path) -> String {
    if dir.as_os_str().is_empty() {
        return String::new();
    }
    if let Ok(text) = fs::read_to_string(dir.join(CONFIG)) {
        let shown = attribute(&text, "ShellVisuals", "DefaultDisplayName")
            .filter(|name| !name.is_empty() && !name.starts_with("ms-resource:"));
        let identity = || attribute(&text, "Identity", "Name").filter(|name| !name.is_empty());
        if let Some(name) = shown.or_else(identity) {
            return name;
        }
    }
    let leaf = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned());
    match leaf(dir) {
        Some(name) if name == "Content" => dir.parent().and_then(leaf).unwrap_or(name),
        other => other.unwrap_or_default(),
    }
}

/// The value of `name="..."` on the first `<tag ...>` element.
fn attribute(text: &str, tag: &str, name: &str) -> Option<String> {
    let rest = &text[text.find(&format!("<{tag}"))?..];
    let element = &rest[..rest.find('>')?];
    let key = format!("{name}=\"");
    let mut from = 0;
    while let Some(at) = element[from..].find(&key).map(|i| from + i) {
        if element[..at].ends_with(char::is_whitespace) {
            let value = &element[at + key.len()..];
            return Some(value[..value.find('"')?].to_string());
        }
        from = at + key.len();
    }
    None
}

/// Folders with a MicrosoftGame.config on this PC's own drives: up to four levels under each
/// drive's root (Windows' own folders skipped), and deeper in the user's Downloads, Desktop and
/// Documents and the usual game libraries. Gives up after `limit`. Games named GTA first.
pub fn find(limit: Duration) -> Search {
    const SKIP: [&str; 12] = [
        "Windows",
        "Program Files",
        "Program Files (x86)",
        "ProgramData",
        "AppData",
        "$Recycle.Bin",
        "System Volume Information",
        "Recovery",
        "PerfLogs",
        "$WinREAgent",
        "node_modules",
        ".git",
    ];
    // The deeper places go first: a folder is looked in once, at the depth it was first reached.
    let mut roots: Vec<(PathBuf, usize)> = Vec::new();
    if let Some(home) = env::var_os("USERPROFILE").map(PathBuf::from) {
        for name in ["Downloads", "Desktop", "Documents"] {
            roots.push((home.join(name), 3));
        }
    }
    let drives = fixed_drives();
    for drive in &drives {
        for library in [
            r"Program Files\ModifiableWindowsApps",
            r"Program Files (x86)\Steam\steamapps\common",
            r"Program Files\Epic Games",
        ] {
            roots.push((drive.join(library), 2));
        }
    }
    roots.extend(drives.into_iter().map(|drive| (drive, 4)));

    let clock = Instant::now();
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    'roots: for (root, max_depth) in roots {
        if !root.is_dir() {
            continue;
        }
        let mut queue = VecDeque::from([(root, 0)]);
        while let Some((dir, depth)) = queue.pop_front() {
            if clock.elapsed() >= limit {
                break 'roots;
            }
            if !seen.insert(dir.to_string_lossy().to_lowercase()) {
                continue;
            }
            if has_config(&dir) {
                found.push(dir);
                continue;
            }
            if depth >= max_depth {
                continue;
            }
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(meta) = entry.metadata() else { continue };
                // Junctions such as "Documents and Settings" lead back into folders already seen.
                if !meta.is_dir() || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    continue;
                }
                if SKIP
                    .iter()
                    .any(|skip| entry.file_name().eq_ignore_ascii_case(skip))
                {
                    continue;
                }
                queue.push_back((entry.path(), depth + 1));
            }
        }
    }

    let mut games: Vec<Found> = found
        .into_iter()
        .map(|dir| Found {
            name: game_name(&dir),
            dir,
        })
        .collect();
    games.sort_by_cached_key(|game| {
        let text = format!("{} {}", game.name, game.dir.display()).to_lowercase();
        let gta = text.contains("grand theft auto") || text.contains("gta");
        (!gta, game.dir.to_string_lossy().to_lowercase())
    });
    Search {
        games,
        timed_out: clock.elapsed() >= limit,
    }
}

fn fixed_drives() -> Vec<PathBuf> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDriveTypeW(root: *const u16) -> u32;
    }
    const DRIVE_FIXED: u32 = 3;
    (b'A'..=b'Z')
        .filter_map(|letter| {
            let root = format!("{}:\\", letter as char);
            let wide: Vec<u16> = root.encode_utf16().chain([0]).collect();
            // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the call.
            let kind = unsafe { GetDriveTypeW(wide.as_ptr()) };
            (kind == DRIVE_FIXED).then(|| PathBuf::from(root))
        })
        .collect()
}
