//! Updates from GitHub releases: the launcher's own, and Leonida Legacy's ready-made builds.
//! Nothing is installed unless the player asks. The launcher puts a new .exe in place of its own
//! (Windows lets a running .exe be renamed) and starts it; a ready-made build is unpacked under
//! %LOCALAPPDATA%\leonida-launcher\builds and leonida.exe is pointed at it. Downloads use Windows'
//! own curl.exe, tar.exe and certutil.exe, like remote.rs.

use std::cmp::Ordering;
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::os::windows::fs::MetadataExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Once;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{env, fs, io, thread};

use serde::{Deserialize, Serialize};

use crate::pc::windows_tool;
use crate::remote::{self, Miss};
use crate::settings::Settings;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const LAUNCHER: &str = "Leonida-launcher";
pub const LEONIDA: &str = "leonida";
const LAUNCHER_EXE: &str = "leonida-launcher.exe";
const LEONIDA_ZIP: &str = "leonida-windows-x64.zip";
const PROJECT: &str = "https://github.com/ProjectLeonidaL/";
const CHECK_EVERY: u64 = 3600;
/// After a check that didn't reach GitHub, the next one comes this much sooner than an hour.
const RETRY_SOONER: u64 = 3000;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// "v1.2.3" or "1.2.3" as numbers. Anything else, a pre-release suffix too, is None.
pub fn version(text: &str) -> Option<Vec<u64>> {
    let text = text.trim();
    let text = text.strip_prefix(['v', 'V']).unwrap_or(text);
    let parts: Option<Vec<u64>> = text
        .split('.')
        .map(|part| {
            let digits = !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
            digits.then(|| part.parse().ok()).flatten()
        })
        .collect();
    parts.filter(|parts| !parts.is_empty())
}

/// Compares part by part; 1.2 is the same as 1.2.0.
pub fn compare(a: &[u64], b: &[u64]) -> Ordering {
    (0..a.len().max(b.len()))
        .map(|i| a.get(i).unwrap_or(&0).cmp(b.get(i).unwrap_or(&0)))
        .find(|order| order.is_ne())
        .unwrap_or(Ordering::Equal)
}

/// Whether version `theirs` is newer than `ours`. A tag that isn't a plain version never is.
/// `ours` may carry a pre-release suffix ("0.3.0-rc1"): the release 0.3.0 is newer than that.
pub fn is_newer(theirs: &str, ours: &str) -> bool {
    let ours = ours.trim().split('+').next().unwrap_or_default();
    let (core, pre) = ours
        .split_once('-')
        .map_or((ours, false), |(core, _)| (core, true));
    match (version(theirs), version(core)) {
        (Some(theirs), Some(ours)) => match compare(&theirs, &ours) {
            Ordering::Greater => true,
            Ordering::Equal => pre,
            Ordering::Less => false,
        },
        _ => false,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub size: u64,
    /// From the API's `digest`, lowercase, or empty when GitHub didn't give one.
    #[serde(default)]
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Release {
    pub tag: String,
    #[serde(default)]
    pub page: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

impl Release {
    fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets
            .iter()
            .find(|asset| asset.name.eq_ignore_ascii_case(name))
    }
}

/// The parts of GitHub's releases/latest answer the launcher uses, or None when the text isn't
/// one.
pub fn parse_release(text: &str) -> Option<Release> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let tag = value["tag_name"].as_str().filter(|tag| !tag.is_empty())?;
    if value["draft"].as_bool() == Some(true) {
        return None;
    }
    let assets = value["assets"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|asset| {
                    Some(Asset {
                        name: asset["name"].as_str()?.to_string(),
                        url: asset["browser_download_url"].as_str()?.to_string(),
                        size: asset["size"].as_u64().unwrap_or(0),
                        sha256: asset["digest"]
                            .as_str()
                            .and_then(|digest| digest.strip_prefix("sha256:"))
                            .and_then(find_sha256)
                            .unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Release {
        tag: tag.to_string(),
        page: value["html_url"].as_str().unwrap_or_default().to_string(),
        assets,
    })
}

/// What the launcher knows about a repository's newest release.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum Known {
    #[default]
    Unknown,
    /// GitHub answered 404: nothing published yet, or the repository isn't public.
    NoneYet,
    Release(Release),
}

/// What GitHub's answer to releases/latest says. A 404 is no release yet, not a problem.
pub fn known(answer: Result<String, Miss>) -> Result<Known, Miss> {
    match answer {
        Ok(text) => parse_release(&text)
            .map(Known::Release)
            .ok_or(Miss::Unreadable),
        Err(Miss::Http(404)) => Ok(Known::NoneYet),
        Err(miss) => Err(miss),
    }
}

/// A newer launcher than this one, with its .exe.
#[derive(Clone, Debug, PartialEq)]
pub struct LauncherOffer {
    pub version: String,
    pub page: String,
    pub exe: Asset,
    /// leonida-launcher.exe.sha256, for when the API gives no digest.
    pub checksum: Option<Asset>,
}

pub fn launcher_offer(known: &Known, ours: &str) -> Option<LauncherOffer> {
    let Known::Release(release) = known else {
        return None;
    };
    if !is_newer(&release.tag, ours) {
        return None;
    }
    Some(LauncherOffer {
        version: release
            .tag
            .trim()
            .trim_start_matches(['v', 'V'])
            .to_string(),
        page: release.page.clone(),
        exe: release.asset(LAUNCHER_EXE)?.clone(),
        checksum: release.asset(&format!("{LAUNCHER_EXE}.sha256")).cloned(),
    })
}

/// Whether the player answered Later to launcher version `new`: `later` is the version they
/// answered it to, empty when they never did, and a newer version asks again.
fn answered_later(new: &str, later: &str) -> bool {
    version(new).is_some() && version(later).is_some() && !is_newer(new, later)
}

/// "build-120" is 120.
pub fn build_number(tag: &str) -> Option<u64> {
    tag.trim().strip_prefix("build-")?.parse().ok()
}

/// The newest ready-made Leonida build, with its zip.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildOffer {
    pub number: u64,
    pub page: String,
    pub zip: Asset,
    /// leonida-windows-x64.zip.sha256, for when the API gives no digest.
    pub checksum: Option<Asset>,
}

pub fn build_offer(known: &Known) -> Option<BuildOffer> {
    let Known::Release(release) = known else {
        return None;
    };
    Some(BuildOffer {
        number: build_number(&release.tag)?,
        page: release.page.clone(),
        zip: release.asset(LEONIDA_ZIP)?.clone(),
        checksum: release.asset(&format!("{LEONIDA_ZIP}.sha256")).cloned(),
    })
}

/// Why the launcher can't install `file` itself, said before anything is downloaded: there's no
/// SHA-256 to check it with. The player gets the release page instead.
pub fn cannot_check(file: &Asset, checksum: Option<&Asset>) -> Option<&'static str> {
    (file.sha256.is_empty() && checksum.is_none()).then_some(NO_CHECK)
}

const NO_CHECK: &str = "This release has no way to check that the download is the real file from \
                        the project, so the launcher won't install it. The release page is open \
                        in your browser instead.";

/// Why a build can't be unpacked here: Windows' tar.exe came with Windows 10 version 1803.
pub fn cannot_unpack() -> Option<&'static str> {
    (!windows_tool("tar.exe").is_file()).then_some(NO_TAR)
}

const NO_TAR: &str = "Unpacking a build needs Windows' tar.exe, which comes with Windows 10 version \
                      1803 and newer. Update Windows, or get the build from the release page, now \
                      open in your browser.";

/// The SHA-256 in a .sha256 file ("<hex>  name", or the hex alone), in certutil's output (with
/// spaces between the bytes on older Windows) or after the API's "sha256:". Lowercase.
pub fn find_sha256(text: &str) -> Option<String> {
    let is_hex = |s: &str| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit());
    text.lines()
        .map(|line| line.trim_start_matches(BOM))
        .find_map(|line| {
            let first = line.split_whitespace().next().unwrap_or_default();
            let joined: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            [first.to_string(), joined].into_iter().find(|s| is_hex(s))
        })
        .map(|hex| hex.to_ascii_lowercase())
}

const BOM: char = '\u{feff}';

/// Which leonida.exe the player has.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Have {
    Missing,
    /// A ready-made build from a release.
    Build(u64),
    /// Built in the player's own checkout (`build=local`).
    Local,
    /// From before `leonida.exe version` existed.
    NoNumber,
    /// It didn't start, or said something unexpected.
    #[default]
    Unknown,
}

/// What `leonida.exe version` printed; `ok` is false when it stopped with an error, as an exe
/// without the command does.
pub fn parse_version(ok: bool, text: &str) -> Have {
    if !ok {
        return Have::NoNumber;
    }
    let build = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("build="))
        .map(str::trim);
    match build {
        Some("local") => Have::Local,
        Some(number) => number.parse().map_or(Have::Unknown, Have::Build),
        None => Have::NoNumber,
    }
}

/// The newest ready-made build, as far as the launcher knows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Newest {
    Build(u64),
    NoneYet,
    Unknown,
}

pub fn newest(known: &Known) -> Newest {
    match known {
        Known::Unknown => Newest::Unknown,
        // A release without the zip (or with another kind of tag) has nothing to install.
        Known::NoneYet | Known::Release(_) => {
            build_offer(known).map_or(Newest::NoneYet, |offer| Newest::Build(offer.number))
        }
    }
}

/// What the Play tab offers about Leonida's builds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Game {
    /// No leonida.exe: the ready-made build is the main fix.
    Download(u64),
    Behind {
        have: u64,
        newest: u64,
    },
    /// The player's own checkout build.
    Own(u64),
    /// A build from before build numbers.
    Old(u64),
    Current,
    /// A higher number than the newest release, as after the release numbers started again.
    Ahead(u64),
    NoReleases,
    Unknown,
}

pub fn game_state(have: Have, newest: Newest) -> Game {
    match (have, newest) {
        (_, Newest::Unknown) | (Have::Unknown, _) => Game::Unknown,
        (_, Newest::NoneYet) => Game::NoReleases,
        (Have::Missing, Newest::Build(n)) => Game::Download(n),
        (Have::Build(have), Newest::Build(n)) if have < n => Game::Behind { have, newest: n },
        (Have::Build(have), Newest::Build(n)) if have > n => Game::Ahead(n),
        (Have::Build(_), Newest::Build(_)) => Game::Current,
        (Have::Local, Newest::Build(n)) => Game::Own(n),
        (Have::NoNumber, Newest::Build(n)) => Game::Old(n),
    }
}

/// Whether a check is due: once an hour, or now when the player asks.
pub fn due(checked: u64, now: u64, asked: bool) -> bool {
    asked || now < checked || now - checked >= CHECK_EVERY
}

fn data_dir() -> Option<PathBuf> {
    Some(PathBuf::from(env::var_os("LOCALAPPDATA")?).join("leonida-launcher"))
}

pub fn builds_dir() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("builds"))
}

/// The last answers from GitHub, kept in %LOCALAPPDATA%\leonida-launcher\cache\releases.json so a
/// start within the hour asks nothing and an offline start still knows what was out.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    /// When GitHub was last asked, in seconds since 1970.
    pub checked: u64,
    pub launcher: Known,
    pub leonida: Known,
}

impl Saved {
    fn path() -> Option<PathBuf> {
        data_dir().map(|dir| dir.join("cache").join("releases.json"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(dir) = path.parent()
            && fs::create_dir_all(dir).is_ok()
            && let Ok(text) = serde_json::to_string_pretty(self)
        {
            let _ = fs::write(path, text);
        }
    }
}

/// Where a repository's newest release is asked for. A debug build may read the launcher's from
/// LEONIDA_LAUNCHER_FEED and Leonida's from LEONIDA_BUILDS_FEED (file:// URLs of a saved answer),
/// to try an update end to end without publishing one.
fn feed(repository: &str) -> String {
    #[cfg(debug_assertions)]
    {
        let name = if repository == LAUNCHER {
            "LEONIDA_LAUNCHER_FEED"
        } else {
            "LEONIDA_BUILDS_FEED"
        };
        if let Some(url) = env::var(name).ok().filter(|url| url.starts_with("file:")) {
            return url;
        }
    }
    format!("https://api.github.com/repos/ProjectLeonidaL/{repository}/releases/latest")
}

/// Whether a file may be downloaded from `url`: only the project's own release files (and a test
/// feed's local files in a debug build).
fn allowed(url: &str, repository: &str) -> bool {
    let ours = format!("{PROJECT}{repository}/releases/download/").to_ascii_lowercase();
    url.to_ascii_lowercase().starts_with(&ours)
        || (cfg!(debug_assertions)
            && url.starts_with("file:")
            && feed(repository).starts_with("file:"))
}

/// A release page the launcher may open: only this project's, otherwise its release list.
pub fn page_or_list(page: &str, repository: &str) -> String {
    if page
        .to_ascii_lowercase()
        .starts_with(&PROJECT.to_ascii_lowercase())
    {
        page.to_string()
    } else {
        format!("{PROJECT}{repository}/releases")
    }
}

/// `url` with curl.exe and the arguments in `extra`, run in `dir` when given: the body (empty
/// with --output), or why not.
fn curl(url: &str, extra: &[&OsStr], seconds: u64, dir: Option<&Path>) -> Result<String, Miss> {
    let local = url.starts_with("file:");
    let agent = format!("leonida-launcher/{VERSION}");
    let run = |best_effort: bool| {
        let mut command = Command::new(windows_tool("curl.exe"));
        if let Some(dir) = dir {
            command.current_dir(dir);
        }
        // --disable, which must come first, keeps a .curlrc from adding headers, a proxy or
        // --insecure.
        command
            .args(["--disable", "--silent", "--location", "--max-time"])
            .arg(seconds.to_string())
            .args(["--user-agent", &agent]);
        if !local {
            command.args(["--proto", "=https", "--proto-redir", "=https"]);
        }
        if best_effort {
            // Offline, Windows can't check the certificate's revocation and curl stops with 35.
            command.arg("--ssl-revoke-best-effort");
        }
        command
            .args(extra)
            .args(["--write-out", "\n%{http_code}"])
            .arg(url)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|_| Miss::NoCurl)
    };
    let mut output = run(true)?;
    // Exit 2: a curl older than 7.70 (early Windows 10) doesn't know the option.
    if output.status.code() == Some(2) {
        output = run(false)?;
    }
    if local && output.status.code() == Some(0) {
        // A file:// URL has no HTTP status.
        let text = String::from_utf8_lossy(&output.stdout);
        return Ok(text
            .rsplit_once('\n')
            .map_or("", |(body, _)| body)
            .to_string());
    }
    remote::classify(output.status.code(), &output.stdout)
}

fn fetch(repository: &str) -> Result<Known, Miss> {
    let accept = [
        OsStr::new("--header"),
        OsStr::new("Accept: application/vnd.github+json"),
    ];
    known(curl(&feed(repository), &accept, 15, None))
}

/// Asks GitHub for both newest releases side by side, keeping the last answer for one that
/// didn't come, and saves the result. Blocks for up to 15 s without network.
pub fn check(mut saved: Saved) -> (Saved, Option<Miss>) {
    let (launcher, leonida) = thread::scope(|scope| {
        let launcher = scope.spawn(|| fetch(LAUNCHER));
        let leonida = fetch(LEONIDA);
        (launcher.join().unwrap_or(Err(Miss::Other(-1))), leonida)
    });
    let mut misses = Vec::new();
    match launcher {
        Ok(known) => saved.launcher = known,
        Err(error) => misses.push(error),
    }
    match leonida {
        Ok(known) => saved.leonida = known,
        Err(error) => misses.push(error),
    }
    saved.checked = retry_from(now(), &misses);
    saved.save();
    (saved, misses.first().copied())
}

/// When the check counts as done: now, or earlier after a check that didn't get through, so the
/// next comes sooner. Not after a rate limit: GitHub allows 60 questions an hour from one address,
/// which players behind one shared address use up together.
fn retry_from(now: u64, misses: &[Miss]) -> u64 {
    let limited = misses
        .iter()
        .any(|miss| matches!(miss, Miss::Http(403 | 429)));
    if misses.is_empty() || limited {
        now
    } else {
        now.saturating_sub(RETRY_SOONER)
    }
}

/// Why a check or a download didn't work, in one sentence.
fn reason(miss: Miss) -> String {
    match miss {
        Miss::Http(code @ (403 | 429)) => {
            format!("GitHub is limiting downloads right now (HTTP {code}).")
        }
        Miss::Http(code) => format!("GitHub answered with an error (HTTP {code})."),
        Miss::NoConnection => "No internet connection, or GitHub can't be reached.".into(),
        Miss::TimedOut => "GitHub took too long to answer.".into(),
        Miss::Secure => "The secure connection to GitHub failed. A wrong clock, a proxy or \
                         antivirus checking connections can cause this."
            .into(),
        Miss::Unreadable => "GitHub sent something the launcher couldn't read.".into(),
        Miss::NoCurl => "Windows' curl.exe wasn't found, so the launcher can't ask GitHub.".into(),
        Miss::Other(code) => format!("The download stopped with curl error {code}."),
    }
}

/// The quiet line for a check that didn't reach GitHub: what is shown still holds.
pub fn check_note(miss: Miss) -> String {
    format!(
        "Couldn't check for updates. {} The launcher tries again later; nothing else is \
         affected.",
        reason(miss)
    )
}

/// Downloads `url` to `to`. curl runs in its folder and gets only the file name, so a folder name
/// outside the ANSI code page doesn't matter.
fn download(url: &str, to: &Path, seconds: u64) -> Result<(), String> {
    let _ = fs::remove_file(to);
    let (Some(dir), Some(name)) = (to.parent(), to.file_name()) else {
        return Err(format!("{} isn't a file name.", to.display()));
    };
    let result = curl(url, &[OsStr::new("--output"), name], seconds, Some(dir));
    match result {
        Ok(_) if to.is_file() => Ok(()),
        Ok(_) => Err("The download came back empty. Try again later.".into()),
        Err(miss) => {
            let _ = fs::remove_file(to);
            Err(format!(
                "The download didn't finish. {} Try again later.",
                reason(miss)
            ))
        }
    }
}

fn sha256_of(file: &Path) -> Result<String, String> {
    let output = Command::new(windows_tool("certutil.exe"))
        .arg("-hashfile")
        .arg(file)
        .arg("SHA256")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| format!("Windows' certutil.exe couldn't check the download ({error})."))?;
    let text = String::from_utf8_lossy(&output.stdout);
    output
        .status
        .success()
        .then(|| find_sha256(&text))
        .flatten()
        .ok_or_else(|| "Windows' certutil.exe couldn't check the download.".into())
}

fn verify_size(file: &Path, size: u64) -> Result<(), String> {
    let got = fs::metadata(file).map_or(0, |meta| meta.len());
    if size > 0 && got != size {
        return Err(format!(
            "The download has {got} bytes instead of {size}: it broke off. Try again."
        ));
    }
    Ok(())
}

/// The download's size and SHA-256 against what the release says.
fn verify(file: &Path, size: u64, sha256: &str) -> Result<(), String> {
    verify_size(file, size)?;
    if !sha256_of(file)?.eq_ignore_ascii_case(sha256) {
        return Err(
            "The download came out different from the release (it may be damaged), so it \
             wasn't used. Try again; if it happens again, tell the project on GitHub."
                .into(),
        );
    }
    Ok(())
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(path.as_os_str());
    name.push(suffix);
    PathBuf::from(name)
}

fn can_write(file: &Path) -> bool {
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(file)
        .is_ok()
}

/// The SHA-256 the release gives for `file`: the API's digest, otherwise the one in its .sha256
/// file. Without either, nothing is installed.
fn expected_sha256(
    file: &Asset,
    checksum: Option<&Asset>,
    repository: &str,
) -> Result<String, String> {
    if !file.sha256.is_empty() {
        return Ok(file.sha256.clone());
    }
    let Some(sums) = checksum.filter(|sums| allowed(&sums.url, repository)) else {
        return Err(NO_CHECK.into());
    };
    let text = curl(&sums.url, &[], 30, None).map_err(|miss| {
        format!(
            "The file that checks the download didn't come. {} Try again later.",
            reason(miss)
        )
    })?;
    find_sha256(&text).ok_or_else(|| {
        "The file that checks the download couldn't be read, so nothing was installed. Try \
         again later; if it happens again, tell the project on GitHub."
            .into()
    })
}

const NO_LOCALAPPDATA: &str = "Windows didn't say where %LOCALAPPDATA% is.";

pub enum Fetched {
    /// The new launcher is in place of this .exe, which is now `<name>.old`.
    Swapped(PathBuf),
    /// The launcher couldn't replace itself: the checked download is here for the player.
    ByHand(PathBuf),
}

/// Where a checked new launcher waits when it can't replace the running one.
fn by_hand_dir() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("update"))
}

/// Downloads the new launcher beside this one, checks it and puts it in place of this .exe.
/// When this folder can't be written to, or Windows doesn't let the .exe be replaced, the checked
/// download goes to %LOCALAPPDATA% instead. Any failure leaves the running .exe where it was.
pub fn fetch_launcher(offer: &LauncherOffer) -> Result<Fetched, String> {
    if !allowed(&offer.exe.url, LAUNCHER) {
        return Err(NOT_OURS.into());
    }
    let sha256 = expected_sha256(&offer.exe, offer.checksum.as_ref(), LAUNCHER)?;
    let exe = env::current_exe()
        .map_err(|error| format!("Windows didn't say where the launcher is ({error})."))?;
    let beside = with_suffix(&exe, ".new");
    let target = if can_write(&beside) {
        beside.clone()
    } else {
        let dir = by_hand_dir().ok_or(NO_LOCALAPPDATA)?;
        fs::create_dir_all(&dir)
            .map_err(|error| format!("Couldn't make {} ({error}).", dir.display()))?;
        dir.join(LAUNCHER_EXE)
    };
    download(&offer.exe.url, &target, 600)?;
    let checked = verify(&target, offer.exe.size, &sha256).and_then(|()| is_program(&target));
    if let Err(error) = checked {
        let _ = fs::remove_file(&target);
        return Err(error);
    }
    if target != beside {
        return hand_over(&target, &offer.version).map(Fetched::ByHand);
    }
    swap(&exe, &target, &offer.version)
}

const NOT_OURS: &str = "The release points somewhere other than this project's GitHub page, so \
                        the launcher won't download it.";

fn is_program(file: &Path) -> Result<(), String> {
    let mut head = [0u8; 2];
    let read = fs::File::open(file).and_then(|mut f| f.read_exact(&mut head));
    match read {
        Ok(()) if &head == b"MZ" => Ok(()),
        _ => Err("The download isn't a Windows program, so it wasn't used.".into()),
    }
}

/// Renames `from` to `to`, trying again for 5 s: antivirus often holds a file it has just seen
/// written open for a moment.
fn rename_retry(from: &Path, to: &Path) -> io::Result<()> {
    let mut result = fs::rename(from, to);
    for _ in 0..20 {
        if result.is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(250));
        result = fs::rename(from, to);
    }
    result
}

/// Moves a checked new launcher to the update folder for the player to copy by hand, with its
/// version beside it, so that a later start knows when it is no longer needed.
fn hand_over(new: &Path, version: &str) -> Result<PathBuf, String> {
    let dir = by_hand_dir().ok_or(NO_LOCALAPPDATA)?;
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Couldn't make {} ({error}).", dir.display()))?;
    let target = dir.join(LAUNCHER_EXE);
    if new != target {
        let _ = fs::remove_file(&target);
        // A rename can't cross drives; a copy can.
        if fs::rename(new, &target).is_err() {
            fs::copy(new, &target)
                .map_err(|error| format!("Couldn't keep the new launcher ({error})."))?;
            let _ = fs::remove_file(new);
        }
    }
    let _ = fs::write(dir.join("version.txt"), version);
    Ok(target)
}

/// The checked new launcher waiting in the update folder for the player to copy, while it is
/// newer than this one.
pub fn waiting_by_hand() -> Option<PathBuf> {
    let dir = by_hand_dir()?;
    let version = fs::read_to_string(dir.join("version.txt")).ok()?;
    let exe = dir.join(LAUNCHER_EXE);
    (is_newer(&version, VERSION) && exe.is_file()).then_some(exe)
}

/// Puts the checked `new` in place of the running `exe`, which becomes `<exe>.old`. When Windows
/// doesn't allow that, the old launcher stays where it was and the download goes to the update
/// folder for the player.
fn swap(exe: &Path, new: &Path, version: &str) -> Result<Fetched, String> {
    let old = with_suffix(exe, ".old");
    let _ = fs::remove_file(&old);
    if rename_retry(exe, &old).is_err() {
        return hand_over(new, version).map(Fetched::ByHand);
    }
    if rename_retry(new, exe).is_ok() {
        return Ok(Fetched::Swapped(exe.to_path_buf()));
    }
    if rename_retry(&old, exe).is_err() {
        return Err(format!(
            "Windows didn't let the new launcher take the old one's place, nor put the old one \
             back. {}",
            rename_back(exe)
        ));
    }
    hand_over(new, version).map(Fetched::ByHand)
}

/// What the player does to get the old launcher back by hand.
fn rename_back(exe: &Path) -> String {
    let name = exe.file_name().unwrap_or_default().to_string_lossy();
    format!(
        "To use the old launcher, delete {name} if it is there, then rename {} to {name}.",
        with_suffix(exe, ".old").display()
    )
}

/// Puts `<exe>.old` back in place of the new launcher, which becomes `<exe>.new` (a running .exe
/// can be renamed, not deleted) for the next start to delete. True when the old one is back.
fn undo_swap(exe: &Path) -> bool {
    let old = with_suffix(exe, ".old");
    if !old.is_file() {
        return false;
    }
    let aside = with_suffix(exe, ".new");
    let _ = fs::remove_file(&aside);
    if exe.exists() && rename_retry(exe, &aside).is_err() {
        return false;
    }
    if rename_retry(&old, exe).is_ok() {
        return true;
    }
    let _ = rename_retry(&aside, exe);
    false
}

/// Starts the new launcher that took this one's place and waits, up to 30 s, until its window
/// has loaded (it leaves `<exe>.started`). When it ends before that, the old launcher is put
/// back. Ok means this launcher may close.
pub fn start_new(exe: &Path) -> Result<(), String> {
    let signal = with_suffix(exe, ".started");
    let _ = fs::remove_file(&signal);
    let mut child = match Command::new(exe).spawn() {
        Ok(child) => child,
        Err(error) => {
            return Err(put_back(
                exe,
                &format!("The new launcher didn't start ({error})"),
            ));
        }
    };
    let since = Instant::now();
    while since.elapsed() < Duration::from_secs(30) {
        if signal.exists() {
            return Ok(());
        }
        if let Ok(Some(_)) = child.try_wait() {
            // Its window opened and was closed, or it put this launcher back itself.
            if signal.exists() || !with_suffix(exe, ".old").exists() {
                return Ok(());
            }
            return Err(put_back(
                exe,
                "The new launcher closed before its window opened",
            ));
        }
        thread::sleep(Duration::from_millis(200));
    }
    // Still starting. If its window can't open, it offers to go back to this one itself.
    Ok(())
}

/// For a new launcher that isn't running (it never started or has ended).
fn put_back(exe: &Path, what: &str) -> String {
    if undo_swap(exe) {
        let _ = fs::remove_file(with_suffix(exe, ".new"));
        format!(
            "{what}, so this launcher stays. Try again later; if it happens again, tell the \
             project on GitHub."
        )
    } else {
        format!(
            "{what}, and Windows didn't let this launcher take its place back. {}",
            rename_back(exe)
        )
    }
}

/// Whether this launcher replaced an older one that is still beside it.
pub fn can_go_back() -> bool {
    env::current_exe().is_ok_and(|exe| with_suffix(&exe, ".old").is_file())
}

/// For a new launcher whose window couldn't open: puts the previous one back and starts it.
pub fn go_back() -> Result<(), String> {
    let exe = env::current_exe()
        .map_err(|error| format!("Windows didn't say where the launcher is ({error})."))?;
    if !undo_swap(&exe) {
        return Err(format!(
            "Windows didn't let the previous launcher back in its place. {}",
            rename_back(&exe)
        ));
    }
    Command::new(&exe).spawn().map(drop).map_err(|error| {
        format!(
            "The previous launcher is back as {}, but it didn't start ({error}). Double-click \
             it.",
            exe.display()
        )
    })
}

/// Called once the window has loaded. After an update, it tells the launcher that started this
/// one that it may close, and only then is the old .exe deleted: until here, a new launcher that
/// can't open its window can still go back to it.
pub fn started() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        thread::spawn(|| {
            let Ok(exe) = env::current_exe() else { return };
            if with_suffix(&exe, ".old").exists() {
                let _ = fs::write(with_suffix(&exe, ".started"), b"");
            }
            tidy(&exe);
        });
    });
}

/// Deletes what an update left: a download beside `exe` that didn't finish, the old .exe once
/// its process has ended (trying for up to 30 s) and the copy-by-hand download once this
/// launcher is at least that version.
fn tidy(exe: &Path) {
    let _ = fs::remove_file(with_suffix(exe, ".new"));
    if let Some(dir) = by_hand_dir()
        && waiting_by_hand().is_none()
    {
        let _ = fs::remove_file(dir.join(LAUNCHER_EXE));
        let _ = fs::remove_file(dir.join("version.txt"));
    }
    let old = with_suffix(exe, ".old");
    for _ in 0..60 {
        if !old.exists() || fs::remove_file(&old).is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(500));
    }
    if !old.exists() {
        let _ = fs::remove_file(with_suffix(exe, ".started"));
    }
}

/// Runs `leonida.exe version` without a window, for up to 10 s. It prints three lines and exits;
/// an older exe without the command stops with an error. The game never starts from it.
pub fn installed(exe: &Path) -> Have {
    if !exe.is_file() {
        return Have::Missing;
    }
    let mut command = Command::new(exe);
    command
        .arg("version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    if let Some(dir) = exe.parent() {
        command.current_dir(dir);
    }
    let Ok(mut child) = command.spawn() else {
        return Have::Unknown;
    };
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < Duration::from_secs(10) => {
                thread::sleep(Duration::from_millis(40));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Have::Unknown;
            }
        }
    };
    let mut text = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut text);
    }
    parse_version(status.success(), &text)
}

/// The file and its write time, to tell whether `installed` must ask again.
pub fn stamp(exe: &Path) -> (PathBuf, Option<SystemTime>) {
    let written = fs::metadata(exe).and_then(|meta| meta.modified()).ok();
    (exe.to_path_buf(), written)
}

fn inside(path: &Path, dir: &Path) -> bool {
    let lower = |p: &Path| p.to_string_lossy().to_lowercase().replace('/', "\\");
    let (path, dir) = (lower(path), lower(dir));
    path.strip_prefix(dir.trim_end_matches('\\'))
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('\\'))
}

/// The number of a ready-made build the launcher installed, from its folder's name.
fn installed_build(exe: &Path) -> Option<u64> {
    let builds = builds_dir()?;
    if !inside(exe, &builds) {
        return None;
    }
    exe.ancestors()
        .filter_map(|dir| dir.file_name()?.to_str())
        .find_map(build_number)
}

/// leonida.exe in an unpacked build, in its leonida folder as the release zip has it.
fn exe_in(dir: &Path) -> Option<PathBuf> {
    Some(dir.join("leonida").join("leonida.exe")).filter(|exe| exe.is_file())
}

/// Whether a name `tar -tf` lists stays inside the zip's leonida folder: not absolute, no drive
/// or other `:`, no `..`.
fn safe_entry(name: &str) -> bool {
    if name.starts_with(['/', '\\']) || name.contains(':') {
        return false;
    }
    let mut parts = name.split(['/', '\\']);
    parts.next() == Some("leonida") && parts.all(|part| part != "..")
}

/// Whether anything under `dir` is a link (a symbolic link or junction) or can't be looked at.
fn has_link(dir: &Path) -> bool {
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    let Ok(mut entries) = fs::read_dir(dir) else {
        return true;
    };
    entries.any(|entry| {
        let Ok(entry) = entry else { return true };
        match entry.path().symlink_metadata() {
            Ok(meta) if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 => true,
            Ok(meta) if meta.is_dir() => has_link(&entry.path()),
            Ok(_) => false,
            Err(_) => true,
        }
    })
}

/// `tar.exe` with `args`, in the builds folder so it gets only plain names.
fn tar(dir: &Path, args: &[&str]) -> io::Result<std::process::Output> {
    Command::new(windows_tool("tar.exe"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
}

/// Downloads build `offer`, checks its SHA-256 and what it holds, unpacks it to builds\build-N
/// and returns its leonida.exe, which said it is build N. A build there already is used as it is.
pub fn install_build(offer: &BuildOffer) -> Result<PathBuf, String> {
    if !allowed(&offer.zip.url, LEONIDA) {
        return Err(NOT_OURS.into());
    }
    let sha256 = expected_sha256(&offer.zip, offer.checksum.as_ref(), LEONIDA)?;
    let dir = builds_dir().ok_or(NO_LOCALAPPDATA)?;
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Couldn't make {} ({error}).", dir.display()))?;
    let n = offer.number;
    let done = dir.join(format!("build-{n}"));
    if let Some(exe) = exe_in(&done)
        && installed(&exe) == Have::Build(n)
    {
        return Ok(exe);
    }
    let zip_name = format!("build-{n}.zip");
    let zip = dir.join(&zip_name);
    download(&offer.zip.url, &zip, 600)?;
    if let Err(error) = verify(&zip, offer.zip.size, &sha256) {
        let _ = fs::remove_file(&zip);
        return Err(error);
    }
    let listed = match tar(&dir, &["-tf", &zip_name]) {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            let names: Vec<&str> = text.lines().map(str::trim_end).collect();
            if names.iter().all(|name| name.is_empty() || safe_entry(name)) {
                Ok(())
            } else {
                Err(
                    "The download holds files outside its leonida folder, so it wasn't used. \
                     Tell the project on GitHub."
                        .to_string(),
                )
            }
        }
        Ok(_) => Err("The download couldn't be read as a zip. Try again.".to_string()),
        Err(error) => Err(format!(
            "Windows' tar.exe couldn't read the download ({error})."
        )),
    };
    if let Err(error) = listed {
        let _ = fs::remove_file(&zip);
        return Err(error);
    }
    let part_name = format!("build-{n}.part");
    let part = dir.join(&part_name);
    let _ = fs::remove_dir_all(&part);
    let unpacked =
        fs::create_dir_all(&part).and_then(|()| tar(&dir, &["-xf", &zip_name, "-C", &part_name]));
    let _ = fs::remove_file(&zip);
    let failed = |text: String| {
        let _ = fs::remove_dir_all(&part);
        Err(text)
    };
    match unpacked {
        Ok(out) if out.status.success() => {}
        Ok(_) => return failed("The download couldn't be unpacked. Try again.".into()),
        Err(error) => return failed(format!("Windows' tar.exe couldn't unpack it ({error}).")),
    }
    if has_link(&part) {
        return failed(
            "The download holds a link to somewhere else on this PC, so it wasn't used. Tell \
             the project on GitHub."
                .into(),
        );
    }
    let Some(exe) = exe_in(&part) else {
        return failed("There's no leonida.exe in the download.".into());
    };
    if installed(&exe) != Have::Build(n) {
        return failed(format!(
            "The leonida.exe in the download doesn't say it is build {n}, so it wasn't used."
        ));
    }
    let Ok(relative) = exe.strip_prefix(&part).map(Path::to_path_buf) else {
        return failed("The build isn't where it was unpacked.".into());
    };
    // The download can take minutes: the game may have been started from a build meanwhile.
    if crate::leonida::running() {
        return failed(
            "Leonida was started while the build downloaded. Close the game, then \
                       update again."
                .into(),
        );
    }
    let _ = fs::remove_dir_all(&done);
    // Antivirus may still be reading the new files for a moment.
    match rename_retry(&part, &done) {
        Ok(()) => Ok(done.join(relative)),
        Err(error) => failed(format!("Couldn't put the build in place ({error}).")),
    }
}

/// Deletes the builds and downloads in builds\ that hold none of `keep`.
pub fn tidy_builds(keep: &[&Path]) {
    let Some(dir) = builds_dir() else { return };
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if !name.starts_with("build-") || keep.iter().any(|exe| inside(exe, &path)) {
            continue;
        }
        if path.is_dir() {
            let _ = fs::remove_dir_all(&path);
        } else {
            let _ = fs::remove_file(&path);
        }
    }
}

const CHECKOUT_SCRIPT: &str = r#"$Host.UI.RawUI.WindowTitle = 'Updating Leonida Legacy'
Write-Host "Updating Leonida Legacy in $((Get-Location).Path)"
Write-Host ''
foreach ($tool in 'git', 'cargo') {
  if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
    Write-Host "$tool isn't installed, so this can't update. The build steps say how to get it." -ForegroundColor Yellow
    return
  }
}
$branch = git branch --show-current
Write-Host "Branch: $branch"
if ($branch -ne 'main') {
  Write-Host "This checkout is on $branch, not main: git pull gets the newest code of $branch, which may not be the newest Leonida Legacy." -ForegroundColor Yellow
}
git update-index -q --refresh
git diff-index --quiet HEAD --
if ($LASTEXITCODE -ne 0) {
  Write-Host 'Some files in this folder were edited, so nothing was changed.' -ForegroundColor Yellow
  Write-Host 'git status lists them. Keep or undo your edits, then try again.'
  return
}
git pull --ff-only
if ($LASTEXITCODE -ne 0) {
  Write-Host 'git pull stopped and changed nothing. The lines above say why.' -ForegroundColor Yellow
  return
}
cargo build --release -p leonida
if ($LASTEXITCODE -ne 0) {
  Write-Host 'The build stopped. The lines above say why.' -ForegroundColor Yellow
  return
}
Write-Host ''
Write-Host 'Done. Warm up the shaders in the launcher once before you play: a new build translates them again.' -ForegroundColor Green
"#;

pub fn is_checkout(folder: &Path) -> Result<(), String> {
    if folder.join(".git").exists() {
        return Ok(());
    }
    Err(format!(
        "{} isn't a Leonida Legacy checkout (a folder made with git clone), so there's nothing \
         to pull. Use the ready-made builds instead.",
        folder.display()
    ))
}

/// The branch a checkout is on, read from its HEAD file (a worktree's too) without running git.
pub fn branch(checkout: &Path) -> Option<String> {
    let git = checkout.join(".git");
    let dir = if git.is_dir() {
        git
    } else {
        let text = fs::read_to_string(&git).ok()?;
        checkout.join(text.trim().strip_prefix("gitdir:")?.trim())
    };
    let head = fs::read_to_string(dir.join("HEAD")).ok()?;
    head.trim()
        .strip_prefix("ref: refs/heads/")
        .map(str::to_string)
}

/// Opens a PowerShell window in `checkout` that pulls the newest code (only as a fast-forward,
/// and only when no tracked file was edited) and builds leonida.exe. The window stays open. The
/// script runs from a file: antivirus watches for encoded command lines.
pub fn update_checkout(checkout: &Path) -> Result<(), String> {
    is_checkout(checkout)?;
    let script = data_dir()
        .ok_or(NO_LOCALAPPDATA)?
        .join("update-checkout.ps1");
    if let Some(dir) = script.parent() {
        let _ = fs::create_dir_all(dir);
    }
    // With a byte order mark, Windows PowerShell 5.1 reads the file as UTF-8.
    let text = format!("{BOM}{}", CHECKOUT_SCRIPT.replace('\n', "\r\n"));
    fs::write(&script, text)
        .map_err(|error| format!("Couldn't write {} ({error}).", script.display()))?;
    Command::new(windows_tool(r"WindowsPowerShell\v1.0\powershell.exe"))
        .args(["-NoLogo", "-NoProfile", "-NoExit"])
        .args(["-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .current_dir(checkout)
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .map(drop)
        .map_err(|error| format!("Couldn't open PowerShell ({error})."))
}

/// What the launcher knows about updates while it runs.
#[derive(Default)]
pub struct Updates {
    pub saved: Saved,
    /// Why the last check didn't reach GitHub.
    pub miss: Option<Miss>,
    pub have: Have,
    /// The leonida.exe and write time `have` belongs to.
    pub have_of: Option<(PathBuf, Option<SystemTime>)>,
    pub checking: bool,
    /// "launcher" or "leonida" while that is being downloaded.
    pub busy: &'static str,
}

impl Updates {
    pub fn new() -> Self {
        Self {
            saved: Saved::load(),
            ..Self::default()
        }
    }

    /// Whether `have` is about `exe` as it is now.
    pub fn knows(&self, exe: &Path) -> bool {
        self.have_of.as_ref() == Some(&stamp(exe))
    }

    pub fn view(&self, settings: &Settings, has_exe: bool) -> UpdatesView {
        let launcher_new = launcher_offer(&self.saved.launcher, VERSION)
            .map(|offer| offer.version)
            .unwrap_or_default();
        let have = if has_exe { self.have } else { Have::Missing };
        let build = build_offer(&self.saved.leonida);
        let state = game_state(have, newest(&self.saved.leonida));
        let newest = build.as_ref().map_or(0, |offer| offer.number);
        let previous = Path::new(&settings.previous_leonida);
        let has_previous = !settings.previous_leonida.is_empty() && previous.is_file();
        let exe = Path::new(&settings.leonida_exe);
        UpdatesView {
            launcher: VERSION,
            launcher_later: answered_later(&launcher_new, &settings.later_launcher),
            launcher_new,
            game: match state {
                Game::Download(_) => "download",
                Game::Behind { .. } => "behind",
                Game::Own(_) => "own",
                Game::Old(_) => "old",
                Game::Current => "current",
                Game::Ahead(_) => "ahead",
                Game::NoReleases => "none",
                Game::Unknown => "unknown",
            },
            newest,
            newest_size: build.as_ref().map_or_else(String::new, |offer| {
                format!("about {} MB", offer.zip.size.div_ceil(1 << 20).max(1))
            }),
            have: match have {
                Have::Missing => "Not found".into(),
                Have::Build(n) => format!("Build {n}"),
                Have::Local => "Your own build".into(),
                Have::NoNumber => "An older build without a number".into(),
                Have::Unknown if self.have_of.is_none() => "Checking...".into(),
                Have::Unknown => "Couldn't tell which build".into(),
            },
            have_build: match have {
                Have::Build(n) => n,
                _ => 0,
            },
            game_later: newest > 0 && settings.later_build >= newest,
            previous: if has_previous {
                installed_build(previous).map_or_else(
                    || {
                        let folder = crate::leonida::folder(previous);
                        format!("Your own build in {}", folder.display())
                    },
                    |n| format!("Build {n}"),
                )
            } else {
                String::new()
            },
            previous_build: if has_previous {
                installed_build(previous).unwrap_or(0)
            } else {
                0
            },
            checkout: has_exe && is_checkout(&crate::leonida::folder(exe)).is_ok(),
            checked: self.saved.checked,
            note: self.miss.map(check_note).unwrap_or_default(),
            checking: self.checking,
            busy: self.busy,
            warm: has_exe && settings.warm_pending == settings.leonida_exe,
            by_hand: waiting_by_hand().map_or_else(String::new, |file| file.display().to_string()),
            launcher_exe: env::current_exe()
                .map_or_else(|_| String::new(), |exe| exe.display().to_string()),
        }
    }
}

/// The update part of the window's view.
#[derive(Serialize)]
pub struct UpdatesView {
    pub launcher: &'static str,
    /// A newer launcher's version, or empty.
    pub launcher_new: String,
    /// The player answered Later to that version.
    pub launcher_later: bool,
    /// "download", "behind", "own", "old", "current", "ahead" (a higher number than the newest
    /// release), "none" (no ready-made builds yet) or "unknown".
    pub game: &'static str,
    /// The newest ready-made build, or 0.
    pub newest: u64,
    pub newest_size: String,
    /// The build in use, in words.
    pub have: String,
    pub have_build: u64,
    pub game_later: bool,
    /// The leonida.exe to go back to, in words, or empty.
    pub previous: String,
    /// Its build number, or 0 for the player's own build.
    pub previous_build: u64,
    /// Whether the leonida.exe in use was built in a git checkout, which can be updated.
    pub checkout: bool,
    /// When GitHub was last asked, in seconds since 1970, or 0.
    pub checked: u64,
    pub note: String,
    pub checking: bool,
    pub busy: &'static str,
    /// The build in use is new and its shaders aren't warmed up yet.
    pub warm: bool,
    /// A checked new launcher waiting for the player to copy it by hand, or empty.
    pub by_hand: String,
    /// The running launcher's .exe, which that copy replaces.
    pub launcher_exe: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
    const LONG_DASH: char = '\u{2014}';

    fn release_json(tag: &str, assets: &str) -> String {
        format!(
            r#"{{"tag_name": "{tag}", "draft": false, "prerelease": false,
                "html_url": "https://github.com/ProjectLeonidaL/leonida/releases/tag/{tag}",
                "assets": [{assets}]}}"#
        )
    }

    fn asset_json(name: &str, digest: Option<&str>) -> String {
        let digest = digest.map_or("null".to_string(), |d| format!("\"sha256:{d}\""));
        let url = format!("https://github.com/ProjectLeonidaL/leonida/releases/download/x/{name}");
        format!(
            r#"{{"name": "{name}", "size": 4194304, "digest": {digest},
                "browser_download_url": "{url}"}}"#
        )
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert_eq!(version("v0.10.2"), Some(vec![0, 10, 2]));
        assert_eq!(version(" 1.2 "), Some(vec![1, 2]));
        assert_eq!(version("v1.2.0-beta"), None);
        assert_eq!(version("build-12"), None);
        assert_eq!(version(""), None);
        assert_eq!(version("1..2"), None);
        assert!(is_newer("v0.10.0", "0.9.9"), "10 is more than 9");
        assert!(is_newer("v0.2.1", "0.2.0"));
        assert!(is_newer("1.0", "0.99.99"));
        assert!(!is_newer("v0.2.0", "0.2.0"));
        assert!(!is_newer("v0.2", "0.2.0"));
        assert!(!is_newer("v0.1.9", "0.2.0"));
        assert!(!is_newer("nightly", "0.2.0"));
        assert!(!is_newer("0.3.0", ""), "nothing answered Later yet");
        assert!(
            is_newer("v0.3.0", "0.3.0-rc1"),
            "the release after its candidate"
        );
        assert!(is_newer("v0.3.1", "0.3.0-rc1"));
        assert!(!is_newer("v0.2.9", "0.3.0-rc1"));
        assert!(
            !is_newer("v0.3.0", "0.3.0+local"),
            "build metadata isn't a pre-release"
        );
    }

    #[test]
    fn a_release_answer_is_read() {
        let assets = [
            asset_json(LEONIDA_ZIP, Some(SHA)),
            asset_json("notes.txt", None),
        ];
        let release = parse_release(&release_json("build-120", &assets.join(","))).unwrap();
        assert_eq!(release.tag, "build-120");
        assert!(release.page.ends_with("/tag/build-120"));
        assert_eq!(release.assets.len(), 2);
        assert_eq!(release.assets[0].sha256, SHA);
        assert_eq!(release.assets[0].size, 4_194_304);
        assert_eq!(release.assets[1].sha256, "");
        let offer = build_offer(&Known::Release(release)).unwrap();
        assert_eq!(offer.number, 120);
        assert!(allowed(&offer.zip.url, LEONIDA));
        assert!(!allowed(&offer.zip.url, LAUNCHER));
        assert!(!allowed("https://example.com/leonida.zip", LEONIDA));
    }

    #[test]
    fn a_404_is_no_release_yet_and_not_a_problem() {
        assert_eq!(known(Err(Miss::Http(404))), Ok(Known::NoneYet));
        assert_eq!(known(Err(Miss::NoConnection)), Err(Miss::NoConnection));
        assert_eq!(known(Ok(String::new())), Err(Miss::Unreadable));
        assert_eq!(known(Ok("{}".into())), Err(Miss::Unreadable));
        assert_eq!(known(Ok("[]".into())), Err(Miss::Unreadable));
        let draft = r#"{"tag_name": "v1.0.0", "draft": true, "assets": []}"#;
        assert_eq!(known(Ok(draft.into())), Err(Miss::Unreadable));
        assert_eq!(newest(&Known::NoneYet), Newest::NoneYet);
        assert_eq!(newest(&Known::Unknown), Newest::Unknown);
        let empty = parse_release(&release_json("build-7", "")).unwrap();
        assert!(empty.assets.is_empty());
        assert_eq!(
            newest(&Known::Release(empty)),
            Newest::NoneYet,
            "no zip yet"
        );
    }

    #[test]
    fn the_launcher_offers_only_a_newer_exe() {
        let assets = [
            asset_json(LAUNCHER_EXE, None),
            asset_json("leonida-launcher.exe.sha256", None),
        ];
        let text = release_json("v0.3.0", &assets.join(","));
        let newer = Known::Release(parse_release(&text).unwrap());
        let offer = launcher_offer(&newer, "0.2.0").unwrap();
        assert_eq!(offer.version, "0.3.0");
        assert!(offer.checksum.is_some());
        assert_eq!(launcher_offer(&newer, "0.3.0"), None);
        let no_exe = Known::Release(parse_release(&release_json("v0.3.0", "")).unwrap());
        assert_eq!(launcher_offer(&no_exe, "0.2.0"), None);
        assert_eq!(launcher_offer(&Known::NoneYet, "0.2.0"), None);
    }

    #[test]
    fn later_hides_only_the_version_it_was_answered_to() {
        assert!(!answered_later("0.3.0", ""), "never answered");
        assert!(answered_later("0.3.0", "0.3.0"));
        assert!(!answered_later("0.4.0", "0.3.0"), "a newer one asks again");
        assert!(!answered_later("", "0.3.0"), "nothing is out");
    }

    #[test]
    fn checksums_are_found_in_every_form() {
        let named = format!("{SHA}  leonida-launcher.exe\n");
        assert_eq!(find_sha256(&named), Some(SHA.into()));
        let binary = format!("{SHA} *leonida-launcher.exe");
        assert_eq!(find_sha256(&binary), Some(SHA.into()));
        let upper = format!("{BOM}{}\r\n", SHA.to_uppercase());
        assert_eq!(find_sha256(&upper), Some(SHA.into()));
        let certutil = format!(
            "SHA256 hash of C:\\x\\leonida-launcher.exe.new:\r\n{SHA}\r\nCertUtil: -hashfile \
             command completed successfully.\r\n"
        );
        assert_eq!(find_sha256(&certutil), Some(SHA.into()));
        let spaced: Vec<&str> = (0..32).map(|i| &SHA[i * 2..i * 2 + 2]).collect();
        let old = format!("SHA256 hash of file x:\r\n{}\r\n", spaced.join(" "));
        assert_eq!(find_sha256(&old), Some(SHA.into()));
        assert_eq!(find_sha256("not a hash"), None);
        assert_eq!(find_sha256(&SHA[1..]), None);
        assert_eq!(find_sha256(""), None);
    }

    #[test]
    fn the_version_command_says_which_build() {
        let release = "version=0.1.0\nbuild=120\ncommit=abc\n";
        assert_eq!(parse_version(true, release), Have::Build(120));
        let local = "version=0.1.0\r\nbuild=local\r\ncommit=local\r\n";
        assert_eq!(parse_version(true, local), Have::Local);
        assert_eq!(
            parse_version(false, ""),
            Have::NoNumber,
            "no version command"
        );
        assert_eq!(parse_version(true, "leonida 0.1.0"), Have::NoNumber);
        assert_eq!(parse_version(true, "build=soon"), Have::Unknown);
    }

    #[test]
    fn which_state_applies() {
        let n = Newest::Build(120);
        assert_eq!(game_state(Have::Missing, n), Game::Download(120));
        let behind = Game::Behind {
            have: 118,
            newest: 120,
        };
        assert_eq!(game_state(Have::Build(118), n), behind);
        assert_eq!(game_state(Have::Build(120), n), Game::Current);
        assert_eq!(
            game_state(Have::Build(617), n),
            Game::Ahead(120),
            "release numbers started again"
        );
        assert_eq!(game_state(Have::Local, n), Game::Own(120));
        assert_eq!(game_state(Have::NoNumber, n), Game::Old(120));
        assert_eq!(game_state(Have::Unknown, n), Game::Unknown);
        assert_eq!(game_state(Have::Missing, Newest::NoneYet), Game::NoReleases);
        assert_eq!(game_state(Have::Local, Newest::NoneYet), Game::NoReleases);
        assert_eq!(game_state(Have::Build(3), Newest::Unknown), Game::Unknown);
    }

    #[test]
    fn checks_come_at_most_once_an_hour() {
        assert!(due(0, 10_000, false), "never checked");
        assert!(!due(10_000, 10_000 + 3599, false));
        assert!(due(10_000, 10_000 + 3600, false));
        assert!(due(10_000, 10_005, true), "asked for");
        assert!(due(10_000, 9_000, false), "the clock went back");
    }

    #[test]
    fn saved_answers_read_back() {
        let saved = Saved {
            checked: 5,
            launcher: Known::NoneYet,
            leonida: Known::Release(parse_release(&release_json("build-9", "")).unwrap()),
        };
        let text = serde_json::to_string(&saved).unwrap();
        let back: Saved = serde_json::from_str(&text).unwrap();
        assert_eq!(back.checked, 5);
        assert_eq!(back.launcher, Known::NoneYet);
        assert_eq!(back.leonida, saved.leonida);
        let old: Saved = serde_json::from_str("{}").unwrap();
        assert_eq!(old.leonida, Known::Unknown);
    }

    #[test]
    fn paths_inside_a_folder() {
        let dir = Path::new(r"C:\Users\a\AppData\Local\leonida-launcher\builds\build-9");
        assert!(inside(&dir.join(r"leonida\leonida.exe"), dir));
        let other_case = r"c:\users\A\appdata\local\leonida-launcher\builds\build-9";
        assert!(inside(Path::new(other_case), dir));
        let longer = r"C:\Users\a\AppData\Local\leonida-launcher\builds\build-90\x.exe";
        assert!(!inside(Path::new(longer), dir));
        assert_eq!(
            with_suffix(Path::new(r"C:\x\leonida-launcher.exe"), ".old"),
            Path::new(r"C:\x\leonida-launcher.exe.old")
        );
    }

    #[test]
    fn zip_entries_stay_in_the_leonida_folder() {
        for good in [
            "leonida/",
            "leonida/leonida.exe",
            "leonida/docs/./a.txt",
            "leonida",
        ] {
            assert!(safe_entry(good), "{good}");
        }
        for bad in [
            "/leonida/leonida.exe",
            "\\leonida\\x",
            "C:/leonida/x",
            "leonida/../x.exe",
            "leonida\\..\\x.exe",
            "other/leonida.exe",
            "leonida.exe",
            "leonida/x:stream",
        ] {
            assert!(!safe_entry(bad), "{bad}");
        }
    }

    #[test]
    fn a_rate_limit_waits_the_full_hour() {
        assert_eq!(retry_from(10_000, &[]), 10_000);
        assert_eq!(
            retry_from(10_000, &[Miss::NoConnection]),
            10_000 - RETRY_SOONER
        );
        assert_eq!(retry_from(10_000, &[Miss::Http(403)]), 10_000);
        assert_eq!(
            retry_from(10_000, &[Miss::TimedOut, Miss::Http(429)]),
            10_000
        );
    }

    #[test]
    fn the_branch_of_a_checkout_and_a_worktree() {
        let root = env::temp_dir().join("leonida-launcher-branch-test");
        let _ = fs::remove_dir_all(&root);
        let checkout = root.join("main");
        fs::create_dir_all(checkout.join(".git")).unwrap();
        fs::write(checkout.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(branch(&checkout).as_deref(), Some("main"));
        let tree = root.join("tree");
        let gitdir = root
            .join("main")
            .join(".git")
            .join("worktrees")
            .join("tree");
        fs::create_dir_all(&tree).unwrap();
        fs::create_dir_all(&gitdir).unwrap();
        fs::write(gitdir.join("HEAD"), "ref: refs/heads/feat/x\n").unwrap();
        fs::write(tree.join(".git"), format!("gitdir: {}\n", gitdir.display())).unwrap();
        assert_eq!(branch(&tree).as_deref(), Some("feat/x"));
        assert!(is_checkout(&tree).is_ok());
        fs::write(checkout.join(".git").join("HEAD"), "0123abc\n").unwrap();
        assert_eq!(branch(&checkout), None, "detached");
        assert!(is_checkout(&root).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn pages_and_texts() {
        let page = "https://github.com/ProjectLeonidaL/leonida/releases/tag/build-9";
        assert_eq!(page_or_list(page, LEONIDA), page);
        assert_eq!(
            page_or_list("https://example.com", LEONIDA),
            "https://github.com/ProjectLeonidaL/leonida/releases"
        );
        let note = check_note(Miss::NoConnection);
        assert!(note.starts_with("Couldn't check for updates."), "{note}");
        for text in [
            note,
            check_note(Miss::Http(403)),
            CHECKOUT_SCRIPT.to_string(),
            NO_CHECK.to_string(),
            NOT_OURS.to_string(),
            NO_LOCALAPPDATA.to_string(),
            NO_TAR.to_string(),
        ] {
            assert!(!text.contains(LONG_DASH), "{text}");
        }
    }
}
