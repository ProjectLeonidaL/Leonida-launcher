//! Finding leonida.exe and the profiles of the checkout it was built in.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use serde::Deserialize;

pub struct Profile {
    pub name: String,
    pub opt_in: bool,
    pub measured: String,
    pub local: bool,
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
            } = entry;
            profiles.push(Profile {
                name,
                opt_in,
                measured,
                local,
            });
        }
    }
    profiles
}
