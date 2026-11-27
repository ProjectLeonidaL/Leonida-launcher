//! The switch list (from this launcher's repository) and the profile catalog (from Leonida's),
//! read from GitHub so the launcher doesn't need a new release for every new switch. The last download is kept in
//! %LOCALAPPDATA%\leonida-launcher and used offline; the copy built in is the last resort.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};

use serde::Deserialize;

const RAW: &str = "https://raw.githubusercontent.com/ProjectLeonidaL";
pub const SWITCHES_REPOSITORY: &str = "Leonida-launcher/main";
const PROFILES_REPOSITORY: &str = "leonida/main";
pub const SWITCHES: &str = "launcher-switches.json";
pub const PROFILES: &str = "gpu-profiles.json";
const BUILT_IN_SWITCHES: &str = include_str!("../launcher-switches.json");
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Deserialize)]
pub struct Switch {
    pub name: String,
    pub values: Vec<String>,
    #[serde(default)]
    pub about: String,
    #[serde(default)]
    pub build: String,
}

#[derive(Deserialize)]
struct SwitchList {
    switches: Vec<Switch>,
}

/// Where a list came from, for the window.
#[derive(Clone, Copy, PartialEq)]
pub enum Source {
    GitHub,
    LastDownload,
    BuiltIn,
}

impl Source {
    pub fn describe(self) -> &'static str {
        match self {
            Source::GitHub => "newest list from GitHub",
            Source::LastDownload => "saved list, GitHub didn't answer",
            Source::BuiltIn => "built-in list",
        }
    }

    pub fn explain(self) -> String {
        match self {
            Source::GitHub => format!("{RAW}/{SWITCHES_REPOSITORY}/{SWITCHES}"),
            Source::LastDownload => {
                "The last list downloaded from GitHub. Refresh to try again.".into()
            }
            Source::BuiltIn => format!(
                "{SWITCHES} couldn't be downloaded from the launcher repository's main branch \
                 (no connection?), so the list built into this launcher is used."
            ),
        }
    }
}

pub struct Lists {
    pub switches: Vec<Switch>,
    pub switches_from: Source,
    /// gpu-profiles.json from GitHub or its last download, when there is one.
    pub profiles: Option<String>,
}

fn cache(file: &str) -> Option<PathBuf> {
    let base = PathBuf::from(env::var_os("LOCALAPPDATA")?);
    Some(base.join("leonida-launcher").join("cache").join(file))
}

/// `file` from `repository` on GitHub with Windows' own curl.exe, or None without an
/// answer.
fn download(repository: &str, file: &str) -> Option<String> {
    let output = Command::new("curl.exe")
        .args(["--fail", "--silent", "--location", "--max-time", "10"])
        .arg(format!("{RAW}/{repository}/{file}"))
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())
        .flatten()
}

/// The newest copy of `file` that `valid` accepts: downloaded (and then kept), otherwise the
/// last download.
fn fetch(repository: &str, file: &str, valid: impl Fn(&str) -> bool) -> Option<(String, Source)> {
    let cached = cache(file);
    if let Some(text) = download(repository, file).filter(|text| valid(text)) {
        if let Some(path) = &cached
            && let Some(dir) = path.parent()
            && fs::create_dir_all(dir).is_ok()
        {
            let _ = fs::write(path, &text);
        }
        return Some((text, Source::GitHub));
    }
    let text = fs::read_to_string(cached?)
        .ok()
        .filter(|text| valid(text))?;
    Some((text, Source::LastDownload))
}

fn parse_switches(text: &str) -> Option<Vec<Switch>> {
    let list: SwitchList = serde_json::from_str(text).ok()?;
    Some(
        list.switches
            .into_iter()
            .filter(|s| !s.name.is_empty() && !s.values.is_empty())
            .collect(),
    )
}

pub fn built_in_switches() -> Vec<Switch> {
    parse_switches(BUILT_IN_SWITCHES).unwrap_or_default()
}

/// Downloads both lists. Blocks for up to 20 s without network, so call it off the UI thread.
pub fn load() -> Lists {
    let (switches, switches_from) = fetch(SWITCHES_REPOSITORY, SWITCHES, |text| {
        parse_switches(text).is_some()
    })
    .and_then(|(text, from)| Some((parse_switches(&text)?, from)))
    .unwrap_or_else(|| (built_in_switches(), Source::BuiltIn));
    let profiles = fetch(PROFILES_REPOSITORY, PROFILES, |text| {
        serde_json::from_str::<serde_json::Value>(text).is_ok_and(|v| v["profiles"].is_array())
    })
    .map(|(text, _)| text);
    Lists {
        switches,
        switches_from,
        profiles,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_list_parses() {
        assert!(!built_in_switches().is_empty());
    }
}
