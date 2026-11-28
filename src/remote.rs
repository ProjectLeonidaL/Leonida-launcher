//! The switch list (from this launcher's repository) and the profile catalog (from Leonida's),
//! read from GitHub so the launcher doesn't need a new release for every new switch. The last download is kept in
//! %LOCALAPPDATA%\leonida-launcher and used offline; the copy built in is the last resort.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::{env, fs, thread};

use serde::{Deserialize, Serialize};

use crate::clock;

const RAW: &str = "https://raw.githubusercontent.com/ProjectLeonidaL";
pub const SWITCHES_REPOSITORY: &str = "Leonida-launcher/main";
const PROFILES_REPOSITORY: &str = "leonida/main";
pub const SWITCHES: &str = "launcher-switches.json";
pub const PROFILES: &str = "gpu-profiles.json";
const BUILT_IN_SWITCHES: &str = include_str!("../launcher-switches.json");
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Serialize, Deserialize)]
pub struct Switch {
    pub name: String,
    pub values: Vec<String>,
    /// The switch's name in plain words, when the list has one.
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub about: String,
    /// "safe", "experimental" or "may crash" when the list says, otherwise empty.
    #[serde(default)]
    pub risk: String,
    #[serde(default)]
    pub build: String,
    /// What Leonida does when the switch isn't set, when that is one of `values`.
    #[serde(default)]
    pub default: String,
    /// "speed" or "crash": which guided test suits the switch. Without it, it isn't offered.
    #[serde(default)]
    pub test: String,
}

/// A test the maintainers want from cards that fit `matcher`.
#[derive(Clone, Serialize, Deserialize)]
pub struct Wanted {
    /// A switch name, or empty when `profile` is set.
    #[serde(default)]
    pub switch: String,
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub value: String,
    #[serde(default, rename = "match")]
    pub matcher: crate::leonida::Matcher,
    #[serde(default)]
    pub why: String,
}

/// A profile from gpu-profiles.json in plain words.
#[derive(Clone, Serialize, Deserialize)]
pub struct ProfileAbout {
    pub name: String,
    pub about: String,
}

#[derive(Deserialize)]
pub struct SwitchList {
    pub switches: Vec<Switch>,
    #[serde(default)]
    pub wanted: Vec<Wanted>,
    #[serde(default)]
    pub profiles: Vec<ProfileAbout>,
}

/// Where a list came from, for the window.
#[derive(Clone, Copy, PartialEq)]
pub enum Source {
    GitHub,
    LastDownload,
    BuiltIn,
}

/// Why a download gave nothing usable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Miss {
    /// GitHub answered with this status.
    Http(u16),
    NoConnection,
    TimedOut,
    Secure,
    /// GitHub sent the file, but it didn't parse.
    Unreadable,
    NoCurl,
    /// Another curl exit code.
    Other(i32),
}

/// A list's source, why the download didn't count, and the date of the saved copy in use.
#[derive(Clone)]
pub struct Origin {
    pub source: Source,
    pub miss: Option<Miss>,
    /// "26 Nov" for a saved copy.
    pub saved: String,
    /// "github 2026-11-27", "saved 2026-11-26" or "built-in", for test reports.
    pub tag: String,
}

impl Origin {
    pub fn built_in(miss: Option<Miss>) -> Self {
        Self {
            source: Source::BuiltIn,
            miss,
            saved: String::new(),
            tag: "built-in".into(),
        }
    }

    pub fn label(&self) -> String {
        match self.source {
            Source::GitHub => "Newest list from GitHub".into(),
            Source::LastDownload => format!("Saved copy from {}", self.saved),
            Source::BuiltIn => "Built-in list".into(),
        }
    }

    /// What happened, in words that don't alarm: the list in use works either way.
    pub fn detail(&self, what: &str) -> String {
        let using = match self.source {
            Source::GitHub => return format!("Refreshed the {what}."),
            Source::LastDownload => format!("the copy saved on {}", self.saved),
            Source::BuiltIn => "the built-in list".into(),
        };
        match self.miss {
            Some(miss) => cause(
                miss,
                what,
                &using,
                "New switches arrive with the next launcher update.",
            ),
            None => format!("Using {using}, which works fine."),
        }
    }
}

/// The sentence for a download that didn't count, then what is used instead. `later` closes a
/// 404, which won't go away by itself: where newer copies come from instead.
pub fn cause(miss: Miss, what: &str, using: &str, later: &str) -> String {
    let fine = format!("Using {using}, which works fine.");
    match miss {
        Miss::Http(404) => format!(
            "GitHub answered, but the {what} isn't public there (HTTP 404, not found). {fine} \
             {later}"
        )
        .trim_end()
        .to_string(),
        Miss::Http(code @ (403 | 429)) => format!(
            "GitHub is limiting downloads right now (HTTP {code}). {fine} Try Refresh in an hour."
        ),
        Miss::Http(code) => {
            format!("GitHub answered with an error (HTTP {code}). {fine} Try Refresh later.")
        }
        Miss::NoConnection => format!(
            "No internet connection, or GitHub can't be reached. {fine} The game itself never \
             needs the internet."
        ),
        Miss::TimedOut => format!("GitHub took too long to answer. {fine}"),
        Miss::Secure => format!(
            "The secure connection to GitHub failed. A wrong clock, a proxy or antivirus \
             checking connections can cause this. {fine}"
        ),
        Miss::Unreadable => format!(
            "GitHub sent the {what}, but the launcher couldn't read it (maybe a change in \
             progress). {fine}"
        ),
        Miss::NoCurl => {
            format!("Windows' curl.exe wasn't found, so the launcher couldn't ask GitHub. {fine}")
        }
        Miss::Other(code) => format!("The download stopped with curl error {code}. {fine}"),
    }
}

pub struct Lists {
    pub switches: Vec<Switch>,
    pub wanted: Vec<Wanted>,
    pub profile_abouts: Vec<ProfileAbout>,
    pub switches_from: Origin,
    /// gpu-profiles.json from GitHub or its last download, when there is one.
    pub profiles: Option<String>,
    pub profiles_from: Origin,
}

fn cache(file: &str) -> Option<PathBuf> {
    let base = PathBuf::from(env::var_os("LOCALAPPDATA")?);
    Some(base.join("leonida-launcher").join("cache").join(file))
}

/// What curl's exit code and its output (the body, a line break and the HTTP status from
/// `--write-out`) mean.
pub fn classify(exit: Option<i32>, stdout: &[u8]) -> Result<String, Miss> {
    match exit {
        Some(0) => {}
        // 52, 55, 56: an empty reply or a dropped send or receive, as on bad Wi-Fi or with
        // antivirus checking connections.
        Some(5..=7 | 52 | 55 | 56) => return Err(Miss::NoConnection),
        Some(28) => return Err(Miss::TimedOut),
        Some(35 | 51 | 53 | 54 | 58 | 59 | 60 | 66 | 77 | 80 | 82 | 83 | 90 | 91) => {
            return Err(Miss::Secure);
        }
        Some(code) => return Err(Miss::Other(code)),
        None => return Err(Miss::Other(-1)),
    }
    let text = String::from_utf8_lossy(stdout);
    let (body, status) = text.rsplit_once('\n').unwrap_or(("", &text));
    match status.trim().parse::<u16>() {
        Ok(200) => Ok(body.to_string()),
        Ok(0) => Err(Miss::NoConnection),
        Ok(code) => Err(Miss::Http(code)),
        Err(_) => Err(Miss::Unreadable),
    }
}

/// `file` from `repository` on GitHub with Windows' own curl.exe.
fn download(repository: &str, file: &str) -> Result<String, Miss> {
    let curl = |best_effort: bool| {
        let mut command = Command::new(crate::pc::windows_tool("curl.exe"));
        // --disable, which must come first, keeps a .curlrc from adding headers, a proxy or
        // --insecure.
        command.args(["--disable", "--silent", "--location", "--max-time", "10"]);
        if best_effort {
            // Offline, Windows can't check the certificate's revocation and curl stops with 35.
            command.arg("--ssl-revoke-best-effort");
        }
        command
            .args(["--write-out", "\n%{http_code}"])
            .arg(format!("{RAW}/{repository}/{file}"))
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|_| Miss::NoCurl)
    };
    let mut output = curl(true)?;
    // Exit 2: a curl older than 7.70 (early Windows 10) doesn't know the option.
    if output.status.code() == Some(2) {
        output = curl(false)?;
    }
    classify(output.status.code(), &output.stdout)
}

/// The newest copy of `file` that `valid` accepts: downloaded (and then kept), otherwise the
/// last download, and where it came from.
fn fetch(repository: &str, file: &str, valid: impl Fn(&str) -> bool) -> (Option<String>, Origin) {
    let cached = cache(file);
    let miss = match download(repository, file) {
        Ok(text) if valid(&text) => {
            if let Some(path) = &cached
                && let Some(dir) = path.parent()
                && fs::create_dir_all(dir).is_ok()
            {
                let _ = fs::write(path, &text);
            }
            let origin = Origin {
                source: Source::GitHub,
                miss: None,
                saved: String::new(),
                tag: format!("github {}", clock::iso_date(std::time::SystemTime::now())),
            };
            return (Some(text), origin);
        }
        Ok(_) => Miss::Unreadable,
        Err(miss) => miss,
    };
    let saved = cached.and_then(|path| {
        let text = fs::read_to_string(&path).ok().filter(|text| valid(text))?;
        let when = fs::metadata(&path).and_then(|m| m.modified()).ok()?;
        Some((text, when))
    });
    match saved {
        Some((text, when)) => (
            Some(text),
            Origin {
                source: Source::LastDownload,
                miss: Some(miss),
                saved: clock::day_month(when),
                tag: format!("saved {}", clock::iso_date(when)),
            },
        ),
        None => (None, Origin::built_in(Some(miss))),
    }
}

fn parse_list(text: &str) -> Option<SwitchList> {
    let mut list: SwitchList = serde_json::from_str(text).ok()?;
    list.switches
        .retain(|s| !s.name.is_empty() && !s.values.is_empty());
    Some(list)
}

pub fn built_in() -> SwitchList {
    parse_list(BUILT_IN_SWITCHES).unwrap_or(SwitchList {
        switches: Vec::new(),
        wanted: Vec::new(),
        profiles: Vec::new(),
    })
}

/// Downloads both lists side by side. Blocks for up to 10 s without network, so call it off
/// the UI thread.
pub fn load() -> Lists {
    let ((text, mut switches_from), (profiles, profiles_from)) = thread::scope(|scope| {
        let switches = scope.spawn(|| {
            fetch(SWITCHES_REPOSITORY, SWITCHES, |text| {
                parse_list(text).is_some()
            })
        });
        let profiles = fetch(PROFILES_REPOSITORY, PROFILES, |text| {
            serde_json::from_str::<serde_json::Value>(text).is_ok_and(|v| v["profiles"].is_array())
        });
        let switches = switches
            .join()
            .unwrap_or_else(|_| (None, Origin::built_in(Some(Miss::Other(-1)))));
        (switches, profiles)
    });
    let list = match text.as_deref().and_then(parse_list) {
        Some(list) => list,
        None => {
            switches_from.source = Source::BuiltIn;
            switches_from.tag = "built-in".into();
            built_in()
        }
    };
    Lists {
        switches: list.switches,
        wanted: list.wanted,
        profile_abouts: list.profiles,
        switches_from,
        profiles,
        profiles_from,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_list_parses() {
        assert!(!built_in().switches.is_empty());
    }

    #[test]
    fn built_in_switches_have_plain_names_and_known_risks() {
        let SwitchList {
            switches,
            wanted,
            profiles,
        } = built_in();
        for profile in &profiles {
            assert!(
                !profile.name.is_empty() && !profile.about.is_empty(),
                "a profile entry needs a name and an about"
            );
        }
        for switch in &switches {
            assert!(!switch.title.is_empty(), "{} has no title", switch.name);
            assert!(
                ["", "safe", "experimental", "may crash"].contains(&switch.risk.as_str()),
                "{} has risk {:?}",
                switch.name,
                switch.risk
            );
            assert!(
                ["", "speed", "crash"].contains(&switch.test.as_str()),
                "{} has test {:?}",
                switch.name,
                switch.test
            );
        }
        for want in &wanted {
            let named = [&want.switch, &want.profile]
                .iter()
                .filter(|n| !n.is_empty())
                .count();
            assert_eq!(named, 1, "a wanted test names one switch or profile");
            assert!(
                want.switch.is_empty() || switches.iter().any(|s| s.name == want.switch),
                "wanted switch {} is not in the list",
                want.switch
            );
        }
    }

    #[test]
    fn curl_answers_become_reasons() {
        assert_eq!(classify(Some(0), b"{}\n200"), Ok("{}".into()));
        assert_eq!(
            classify(Some(0), b"404: Not Found\n404"),
            Err(Miss::Http(404))
        );
        assert_eq!(classify(Some(0), b"\n503"), Err(Miss::Http(503)));
        assert_eq!(classify(Some(0), b"\n000"), Err(Miss::NoConnection));
        assert_eq!(classify(Some(6), b"\n000"), Err(Miss::NoConnection));
        assert_eq!(classify(Some(7), b""), Err(Miss::NoConnection));
        assert_eq!(classify(Some(56), b""), Err(Miss::NoConnection));
        assert_eq!(classify(Some(28), b""), Err(Miss::TimedOut));
        assert_eq!(classify(Some(60), b""), Err(Miss::Secure));
        assert_eq!(classify(Some(3), b""), Err(Miss::Other(3)));
    }

    #[test]
    fn a_404_says_the_file_is_not_public_and_that_all_is_well() {
        let origin = Origin::built_in(Some(Miss::Http(404)));
        let text = origin.detail("switch list");
        assert!(text.contains("isn't public"), "{text}");
        assert!(text.contains("404"), "{text}");
        assert!(text.contains("built-in list, which works fine"), "{text}");
        assert!(!text.contains("Couldn't reach"), "{text}");
        assert_eq!(origin.label(), "Built-in list");
    }

    #[test]
    fn no_connection_is_told_apart_from_an_answer() {
        let saved = Origin {
            source: Source::LastDownload,
            miss: Some(Miss::NoConnection),
            saved: "26 Nov".into(),
            tag: "saved 2026-11-26".into(),
        };
        let text = saved.detail("switch list");
        assert!(text.starts_with("No internet connection"), "{text}");
        assert!(text.contains("the copy saved on 26 Nov"), "{text}");
        assert_eq!(saved.label(), "Saved copy from 26 Nov");
        let limited = cause(Miss::Http(429), "switch list", "the built-in list", "");
        assert!(
            limited.contains("limiting") && limited.contains("429"),
            "{limited}"
        );
        let profiles = cause(Miss::Http(404), "graphics profile list", "x", "");
        assert!(profiles.ends_with("which works fine."), "{profiles}");
    }

    #[test]
    fn no_text_has_a_long_dash() {
        let misses = [
            Miss::Http(404),
            Miss::Http(429),
            Miss::Http(500),
            Miss::NoConnection,
            Miss::TimedOut,
            Miss::Secure,
            Miss::Unreadable,
            Miss::NoCurl,
            Miss::Other(3),
        ];
        for miss in misses {
            assert!(!cause(miss, "switch list", "x", "y").contains('\u{2014}'));
        }
        assert!(!BUILT_IN_SWITCHES.contains('\u{2014}'));
    }
}
