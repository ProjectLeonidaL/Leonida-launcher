//! Updates from GitHub releases: the launcher's own, and Leonida Legacy's ready-made builds.
//! Nothing is installed unless the player asks. The launcher puts a new .exe in place of its own
//! (Windows lets a running .exe be renamed) and starts it; a ready-made build is unpacked under
//! %LOCALAPPDATA%\leonida-launcher\builds and leonida.exe is pointed at it. Downloads use Windows'
//! own curl.exe, tar.exe and certutil.exe, like remote.rs.

use std::cmp::Ordering;
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{env, fs, thread};

use serde::{Deserialize, Serialize};

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
pub fn is_newer(theirs: &str, ours: &str) -> bool {
    match (version(theirs), version(ours)) {
        (Some(theirs), Some(ours)) => compare(&theirs, &ours) == Ordering::Greater,
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
}

pub fn build_offer(known: &Known) -> Option<BuildOffer> {
    let Known::Release(release) = known else {
        return None;
    };
    Some(BuildOffer {
        number: build_number(&release.tag)?,
        page: release.page.clone(),
        zip: release.asset(LEONIDA_ZIP)?.clone(),
    })
}

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
    NoReleases,
    Unknown,
}

pub fn game_state(have: Have, newest: Newest) -> Game {
    match (have, newest) {
        (_, Newest::Unknown) | (Have::Unknown, _) => Game::Unknown,
        (_, Newest::NoneYet) => Game::NoReleases,
        (Have::Missing, Newest::Build(n)) => Game::Download(n),
        (Have::Build(have), Newest::Build(n)) if have < n => Game::Behind { have, newest: n },
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

/// `url` with curl.exe and the arguments in `extra`: the body (empty with --output), or why not.
fn curl(url: &str, extra: &[&OsStr], seconds: u64) -> Result<String, Miss> {
    let local = url.starts_with("file:");
    let agent = format!("leonida-launcher/{VERSION}");
    let run = |best_effort: bool| {
        let mut command = Command::new("curl.exe");
        command
            .args(["--silent", "--location", "--max-time"])
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
    known(curl(&feed(repository), &accept, 15))
}

/// Asks GitHub for both newest releases side by side, keeping the last answer for one that
/// didn't come, and saves the result. Blocks for up to 15 s without network.
pub fn check(mut saved: Saved) -> (Saved, Option<Miss>) {
    let (launcher, leonida) = thread::scope(|scope| {
        let launcher = scope.spawn(|| fetch(LAUNCHER));
        let leonida = fetch(LEONIDA);
        (launcher.join().unwrap_or(Err(Miss::Other(-1))), leonida)
    });
    let mut miss = None;
    match launcher {
        Ok(known) => saved.launcher = known,
        Err(error) => miss = Some(error),
    }
    match leonida {
        Ok(known) => saved.leonida = known,
        Err(error) => miss = miss.or(Some(error)),
    }
    saved.checked = now();
    if miss.is_some() {
        saved.checked = saved.checked.saturating_sub(RETRY_SOONER);
    }
    saved.save();
    (saved, miss)
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

fn download(url: &str, to: &Path, seconds: u64) -> Result<(), String> {
    let _ = fs::remove_file(to);
    let result = curl(url, &[OsStr::new("--output"), to.as_os_str()], seconds);
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
    let output = Command::new("certutil.exe")
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
            "The download doesn't match its checksum, so it wasn't used. Try again \
                    later; if it happens again, tell the project on GitHub."
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

fn expected_sha256(offer: &LauncherOffer) -> Result<String, String> {
    if !offer.exe.sha256.is_empty() {
        return Ok(offer.exe.sha256.clone());
    }
    let none = "This release has no checksum, so the launcher won't install it. Get it from the \
                release page instead.";
    let Some(file) = offer
        .checksum
        .as_ref()
        .filter(|f| allowed(&f.url, LAUNCHER))
    else {
        return Err(none.into());
    };
    let text = curl(&file.url, &[], 30).map_err(|miss| {
        format!(
            "The checksum didn't download. {} Try again later.",
            reason(miss)
        )
    })?;
    find_sha256(&text).ok_or_else(|| none.into())
}

pub enum Fetched {
    /// The new launcher is in place of this .exe, which is now `<name>.old`.
    Swapped(PathBuf),
    /// This folder can't be written to: the checked download is here for the player.
    ByHand(PathBuf),
}

/// Downloads the new launcher beside this one, checks it and puts it in place of this .exe.
/// When this folder can't be written to, the checked download goes to %LOCALAPPDATA% instead.
/// Any failure leaves the running .exe where it was.
pub fn fetch_launcher(offer: &LauncherOffer) -> Result<Fetched, String> {
    if !allowed(&offer.exe.url, LAUNCHER) {
        return Err(NOT_OURS.into());
    }
    let sha256 = expected_sha256(offer)?;
    let exe = env::current_exe()
        .map_err(|error| format!("Windows didn't say where the launcher is ({error})."))?;
    let beside = with_suffix(&exe, ".new");
    let (target, by_hand) = if can_write(&beside) {
        (beside, false)
    } else {
        let dir = data_dir()
            .ok_or("Windows didn't say where %LOCALAPPDATA% is.")?
            .join("update");
        fs::create_dir_all(&dir)
            .map_err(|error| format!("Couldn't make {} ({error}).", dir.display()))?;
        (dir.join(LAUNCHER_EXE), true)
    };
    download(&offer.exe.url, &target, 600)?;
    let checked = verify(&target, offer.exe.size, &sha256).and_then(|()| is_program(&target));
    if let Err(error) = checked {
        let _ = fs::remove_file(&target);
        return Err(error);
    }
    if by_hand {
        return Ok(Fetched::ByHand(target));
    }
    swap(&exe, &target)?;
    Ok(Fetched::Swapped(exe))
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

fn swap(exe: &Path, new: &Path) -> Result<(), String> {
    let old = with_suffix(exe, ".old");
    let _ = fs::remove_file(&old);
    if let Err(error) = fs::rename(exe, &old) {
        let _ = fs::remove_file(new);
        return Err(format!(
            "Windows didn't let the launcher set its own .exe aside ({error}). Nothing changed."
        ));
    }
    if let Err(error) = fs::rename(new, exe) {
        let _ = fs::rename(&old, exe);
        let _ = fs::remove_file(new);
        return Err(format!(
            "Windows didn't let the new launcher take the old one's place ({error}). Nothing \
             changed."
        ));
    }
    Ok(())
}

/// Puts the old launcher back after the new one didn't start.
pub fn undo_swap(exe: &Path) {
    let _ = fs::remove_file(exe);
    let _ = fs::rename(with_suffix(exe, ".old"), exe);
}

/// Deletes what an update left beside this .exe: the old one once its process has ended, and a
/// download that didn't finish. In the background, for up to 30 s.
pub fn tidy() {
    let Ok(exe) = env::current_exe() else { return };
    thread::spawn(move || {
        let _ = fs::remove_file(with_suffix(&exe, ".new"));
        let old = with_suffix(&exe, ".old");
        for _ in 0..60 {
            if !old.exists() || fs::remove_file(&old).is_ok() {
                break;
            }
            thread::sleep(Duration::from_millis(500));
        }
    });
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

/// leonida.exe in an unpacked build: in its leonida folder, as the release zip has it, or at the
/// top.
fn exe_in(dir: &Path) -> Option<PathBuf> {
    [
        dir.join("leonida").join("leonida.exe"),
        dir.join("leonida.exe"),
    ]
    .into_iter()
    .find(|exe| exe.is_file())
}

/// Downloads build `offer`, unpacks it to builds\build-N and returns its leonida.exe, which
/// said it is build N. A build there already is used as it is.
pub fn install_build(offer: &BuildOffer) -> Result<PathBuf, String> {
    if !allowed(&offer.zip.url, LEONIDA) {
        return Err(NOT_OURS.into());
    }
    let dir = builds_dir().ok_or("Windows didn't say where %LOCALAPPDATA% is.")?;
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Couldn't make {} ({error}).", dir.display()))?;
    let n = offer.number;
    let done = dir.join(format!("build-{n}"));
    if let Some(exe) = exe_in(&done)
        && installed(&exe) == Have::Build(n)
    {
        return Ok(exe);
    }
    let zip = dir.join(format!("build-{n}.zip"));
    download(&offer.zip.url, &zip, 600)?;
    let checked = if offer.zip.sha256.is_empty() {
        verify_size(&zip, offer.zip.size)
    } else {
        verify(&zip, offer.zip.size, &offer.zip.sha256)
    };
    if let Err(error) = checked {
        let _ = fs::remove_file(&zip);
        return Err(error);
    }
    let part = dir.join(format!("build-{n}.part"));
    let _ = fs::remove_dir_all(&part);
    let unpacked = fs::create_dir_all(&part).and_then(|()| {
        Command::new("tar.exe")
            .arg("-xf")
            .arg(&zip)
            .arg("-C")
            .arg(&part)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status()
    });
    let _ = fs::remove_file(&zip);
    let failed = |text: String| {
        let _ = fs::remove_dir_all(&part);
        Err(text)
    };
    match unpacked {
        Ok(status) if status.success() => {}
        Ok(_) => return failed("The download couldn't be unpacked. Try again.".into()),
        Err(error) => return failed(format!("Windows' tar.exe couldn't unpack it ({error}).")),
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
    let _ = fs::remove_dir_all(&done);
    // Antivirus may still be reading the new files for a moment.
    let mut moved = fs::rename(&part, &done);
    for _ in 0..20 {
        if moved.is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(250));
        moved = fs::rename(&part, &done);
    }
    match moved {
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

/// Opens a PowerShell window in `checkout` that pulls the newest code (only as a fast-forward,
/// and only when no tracked file was edited) and builds leonida.exe. The window stays open.
pub fn update_checkout(checkout: &Path) -> Result<(), String> {
    is_checkout(checkout)?;
    let script = script_argument(CHECKOUT_SCRIPT);
    Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NoExit",
            "-ExecutionPolicy",
            "Bypass",
        ])
        .args(["-EncodedCommand", &script])
        .current_dir(checkout)
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .map(drop)
        .map_err(|error| format!("Couldn't open PowerShell ({error})."))
}

/// A script for -EncodedCommand: UTF-16LE in base64, so nothing in it needs quoting.
fn script_argument(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64(&bytes)
}

fn base64(bytes: &[u8]) -> String {
    const LETTERS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let byte = |i: usize| u32::from(chunk.get(i).copied().unwrap_or(0));
        let n = (byte(0) << 16) | (byte(1) << 8) | byte(2);
        for i in 0..4 {
            if i <= chunk.len() {
                text.push(char::from(LETTERS[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                text.push('=');
            }
        }
    }
    text
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
    /// A new build is in use and its shaders aren't warmed up yet.
    pub warm: bool,
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
            previous: if settings.previous_leonida.is_empty() || !previous.is_file() {
                String::new()
            } else {
                installed_build(previous).map_or_else(
                    || {
                        let folder = crate::leonida::folder(previous);
                        format!("Your own build in {}", folder.display())
                    },
                    |n| format!("Build {n}"),
                )
            },
            checked: self.saved.checked,
            note: self.miss.map(check_note).unwrap_or_default(),
            checking: self.checking,
            busy: self.busy,
            warm: self.warm,
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
    /// "download", "behind", "own", "old", "current", "none" (no ready-made builds yet) or
    /// "unknown".
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
    /// When GitHub was last asked, in seconds since 1970, or 0.
    pub checked: u64,
    pub note: String,
    pub checking: bool,
    pub busy: &'static str,
    pub warm: bool,
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
        assert_eq!(game_state(Have::Build(121), n), Game::Current);
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
    fn scripts_are_encoded_for_powershell() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b""), "");
        assert_eq!(script_argument("dir"), "ZABpAHIA");
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
        ] {
            assert!(!text.contains(LONG_DASH), "{text}");
        }
    }
}
