//! Facts about this PC and the leonida.exe in use, for test reports: the hardware, Windows and
//! version names only. Read through reg.exe and a few kernel32 and user32 calls, so no new
//! crate is needed.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::SystemTime;
use std::{env, fs};

use serde::Deserialize;

use crate::leonida::{self, Card};

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// One of Windows' own programs by its full path: `name` in System32, or explorer.exe in the
/// Windows folder. Started by bare name, a file of the same name beside the launcher (in Downloads,
/// say) would run instead.
pub fn windows_tool(name: &str) -> PathBuf {
    let root =
        env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    if name.eq_ignore_ascii_case("explorer.exe") {
        root.join(name)
    } else {
        root.join("System32").join(name)
    }
}

/// leonida.exe in its folder with only `env` as LEONIDA_* variables: the launcher's own
/// environment may carry some, and test runs must not depend on them.
pub fn leonida(exe: &Path, env: &[(String, String)]) -> Command {
    let mut command = Command::new(exe);
    command
        .current_dir(leonida::folder(exe))
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    for (name, _) in env::vars_os() {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("LEONIDA_")
        {
            command.env_remove(&name);
        }
    }
    command.envs(
        env.iter()
            .map(|(n, v)| (OsString::from(n), OsString::from(v))),
    );
    command
}

fn output(mut command: Command) -> Option<Output> {
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()
}

/// What `leonida.exe config --graphics` prints.
#[derive(Clone, Default, Deserialize)]
pub struct Plan {
    pub gpu: Option<Card>,
    #[serde(default)]
    pub profiles: Vec<String>,
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    #[serde(default)]
    pub sources: BTreeMap<String, String>,
}

pub fn plan(exe: &Path, env: &[(String, String)]) -> Option<Plan> {
    let mut command = leonida(exe, env);
    command.args(["config", "--graphics"]);
    let out = output(command)?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

/// Leonida's folders from `leonida.exe config`, used only to look at what is in them; never
/// shown.
#[derive(Default)]
pub struct Dirs {
    /// Holds the shader cache, `shaders`.
    pub cache: Option<PathBuf>,
    /// Holds crashes.log.
    pub data: Option<PathBuf>,
}

/// The folders in `config`'s lines, "cache_dir   : C:\...".
pub fn parse_dirs(text: &str) -> Dirs {
    let dir = |key: &str| {
        let line = text.lines().find(|l| l.starts_with(key))?;
        let (_, dir) = line.split_once(':')?;
        Some(PathBuf::from(dir.trim())).filter(|d| !d.as_os_str().is_empty())
    };
    Dirs {
        cache: dir("cache_dir"),
        data: dir("data_dir"),
    }
}

pub fn dirs(exe: &Path, env: &[(String, String)]) -> Dirs {
    let mut command = leonida(exe, env);
    command.arg("config");
    output(command).map_or_else(Dirs::default, |out| {
        parse_dirs(&String::from_utf8_lossy(&out.stdout))
    })
}

/// Whether the shader cache has anything in it; true when the folder isn't known, since then
/// nothing can be said.
pub fn shaders_cached(cache: Option<&Path>) -> bool {
    cache.is_none_or(|dir| {
        fs::read_dir(dir.join("shaders")).is_ok_and(|mut entries| entries.next().is_some())
    })
}

/// "yes" when a crashes.log under `dir` changed since `since`, "no", or "unknown". Only the
/// time is read, never the file: it can quote the game.
pub fn crash_written(dir: Option<&Path>, since: SystemTime) -> &'static str {
    fn walk(dir: &Path, since: SystemTime, depth: u32) -> bool {
        let Ok(entries) = fs::read_dir(dir) else {
            return false;
        };
        entries.flatten().any(|entry| {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                return false;
            };
            if kind.is_dir() {
                return depth < 4 && walk(&path, since, depth + 1);
            }
            path.file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("crashes.log"))
                && entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .is_ok_and(|t| t >= since)
        })
    }
    match dir {
        None => "unknown",
        Some(dir) if !dir.is_dir() => "no",
        Some(dir) if walk(dir, since, 0) => "yes",
        Some(_) => "no",
    }
}

/// leonida.exe's version: `version` on release builds (feat/releases), `--version` before.
#[derive(Clone, Default)]
pub struct Version {
    pub version: String,
    pub build: String,
    pub commit: String,
    /// Built by the Release workflow: `build=` is a number.
    pub release: bool,
}

pub fn parse_version(version_out: Option<&str>, dash_version_out: Option<&str>) -> Version {
    let mut v = Version::default();
    if let Some(text) = version_out {
        for line in text.lines() {
            match line.trim().split_once('=') {
                Some(("version", x)) => v.version = x.trim().into(),
                Some(("build", x)) => v.build = x.trim().into(),
                Some(("commit", x)) => v.commit = x.trim().into(),
                _ => {}
            }
        }
    }
    if v.version.is_empty()
        && let Some(text) = dash_version_out
    {
        v.version = text.split_whitespace().last().unwrap_or("").into();
    }
    v.release = !v.build.is_empty() && v.build.bytes().all(|c| c.is_ascii_digit());
    v
}

pub fn version(exe: &Path) -> Version {
    let run = |args: &[&str]| {
        let mut command = leonida(exe, &[]);
        command.args(args);
        output(command)
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let first = run(&["version"]);
    let second = if first.is_none() {
        run(&["--version"])
    } else {
        None
    };
    parse_version(first.as_deref(), second.as_deref())
}

/// Names the player's text must not carry: the user name, the computer's name and the profile
/// folder's name, which can differ from the user name.
pub fn names() -> Vec<String> {
    let folder = env::var_os("USERPROFILE").and_then(|p| {
        Path::new(&p)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
    });
    let mut names: Vec<String> = [
        env::var("USERNAME").ok(),
        env::var("COMPUTERNAME").ok(),
        folder,
    ]
    .into_iter()
    .flatten()
    .filter(|n| !n.trim().is_empty())
    .collect();
    names.dedup();
    names
}

fn reg(args: &[&str]) -> String {
    let mut command = Command::new(windows_tool("reg.exe"));
    command
        .arg("query")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW);
    output(command)
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default()
}

/// A `reg query` value line: "    Name    REG_SZ    value".
fn reg_value(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    let at = line.find("    REG_")?;
    let rest = line[at..].trim_start();
    let kind_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    Some((line[..at].trim(), rest[kind_end..].trim()))
}

/// A display adapter from the registry.
#[derive(Default)]
pub struct Adapter {
    pub desc: String,
    pub version: String,
    pub device: String,
    pub radeon: String,
}

const DISPLAY_CLASS: &str =
    r"HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

/// An adapter's own key: the class key's four-digit subkeys (0000, 0001, ...).
fn adapter_key(line: &str) -> bool {
    let last = line.trim_end().rsplit('\\').next().unwrap_or("");
    line.starts_with("HKEY_") && last.len() == 4 && last.bytes().all(|c| c.is_ascii_digit())
}

/// The adapters in `reg query` output of their keys, Microsoft's own basic driver left out.
pub fn parse_adapters(text: &str) -> Vec<Adapter> {
    let mut list: Vec<Adapter> = Vec::new();
    let mut current: Option<Adapter> = None;
    for line in text.lines() {
        if line.starts_with("HKEY_") {
            list.extend(current.take());
            if adapter_key(line) {
                current = Some(Adapter::default());
            }
            continue;
        }
        let (Some(adapter), Some((name, value))) = (current.as_mut(), reg_value(line)) else {
            continue;
        };
        match name {
            "DriverDesc" => adapter.desc = value.into(),
            "DriverVersion" => adapter.version = value.into(),
            "MatchingDeviceId" => adapter.device = value.to_ascii_lowercase(),
            "RadeonSoftwareVersion" => adapter.radeon = value.into(),
            _ => {}
        }
    }
    list.extend(current);
    list.retain(|a| !a.desc.is_empty() && !a.desc.starts_with("Microsoft "));
    list
}

/// The driver of the card named `name`, in the vendor's numbering where it has one (the rules
/// of test-report.ps1).
pub fn driver(adapters: &[Adapter], name: &str) -> String {
    let Some(a) = adapters.iter().find(|a| a.desc == name) else {
        return "unknown".into();
    };
    if a.device.contains("ven_10de") {
        // NVIDIA's number is the last five digits of the Windows one: 32.0.16.2124 is 621.24.
        let digits: String = a.version.chars().filter(char::is_ascii_digit).collect();
        if digits.len() >= 5 {
            let d = &digits[digits.len() - 5..];
            return format!("{}.{} ({})", &d[..3], &d[3..], a.version);
        }
    }
    if !a.radeon.is_empty() {
        return format!("Adrenalin {} ({})", a.radeon, a.version);
    }
    a.version.clone()
}

/// Each adapter's key read on its own: `reg query /s` on the class walks every subkey below
/// the adapters too and took 29 s on one PC, against about a second this way.
pub fn adapters() -> Vec<Adapter> {
    let text: String = reg(&[DISPLAY_CLASS])
        .lines()
        .map(str::trim)
        .filter(|line| adapter_key(line))
        .map(|key| reg(&[key]))
        .collect();
    parse_adapters(&text)
}

fn reg_values(key: &str) -> BTreeMap<String, String> {
    reg(&[key])
        .lines()
        .filter_map(reg_value)
        .map(|(n, v)| (n.to_string(), v.to_string()))
        .collect()
}

pub fn cpu() -> String {
    reg_values(r"HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\0")
        .remove("ProcessorNameString")
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| "unknown".into())
}

/// "Windows 11 Pro 25H2 26200.1234": ProductName still says Windows 10 on Windows 11.
pub fn windows_name(values: &BTreeMap<String, String>) -> String {
    let get = |k: &str| values.get(k).map_or("", String::as_str);
    let build: u32 = get("CurrentBuild").parse().unwrap_or(0);
    let ubr = u32::from_str_radix(get("UBR").trim_start_matches("0x"), 16).unwrap_or(0);
    let mut product = get("ProductName").to_string();
    if build >= 22_000 {
        product = product.replace("Windows 10", "Windows 11");
    }
    format!("{product} {} {build}.{ubr}", get("DisplayVersion"))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn windows() -> String {
    windows_name(&reg_values(
        r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
    ))
}

// Win32 layouts: every field is there for the size, not read.
#[allow(dead_code)]
#[repr(C)]
struct MemoryStatus {
    length: u32,
    load: u32,
    total_phys: u64,
    avail_phys: u64,
    total_page: u64,
    avail_page: u64,
    total_virtual: u64,
    avail_virtual: u64,
    avail_extended: u64,
}

// Win32 layouts: every field is there for the size, not read.
#[allow(dead_code)]
#[repr(C)]
struct PowerStatus {
    ac_line: u8,
    battery_flag: u8,
    battery_percent: u8,
    saver: u8,
    life_time: u32,
    full_life_time: u32,
}

/// DEVMODEW, only the display fields named.
#[allow(dead_code)]
#[repr(C)]
struct DevMode {
    device_name: [u16; 32],
    spec_version: u16,
    driver_version: u16,
    size: u16,
    driver_extra: u16,
    fields: u32,
    position: [u32; 4],
    color: i16,
    duplex: i16,
    y_resolution: i16,
    tt_option: i16,
    collate: i16,
    form_name: [u16; 32],
    log_pixels: u16,
    bits_per_pel: u32,
    width: u32,
    height: u32,
    display_flags: u32,
    frequency: u32,
    rest: [u32; 8],
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalMemoryStatusEx(status: *mut MemoryStatus) -> i32;
    fn GetSystemPowerStatus(status: *mut PowerStatus) -> i32;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn EnumDisplaySettingsW(device: *const u16, mode: u32, devmode: *mut DevMode) -> i32;
    fn MessageBeep(kind: u32) -> i32;
}

/// The installed memory, rounded to whole GB like test-report.ps1.
pub fn ram_gb() -> u64 {
    let mut status = MemoryStatus {
        length: size_of::<MemoryStatus>() as u32,
        load: 0,
        total_phys: 0,
        avail_phys: 0,
        total_page: 0,
        avail_page: 0,
        total_virtual: 0,
        avail_virtual: 0,
        avail_extended: 0,
    };
    // SAFETY: `status` is a MEMORYSTATUSEX with its length set, alive for the call.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) } != 0;
    if ok {
        (status.total_phys + (1 << 29)) >> 30
    } else {
        0
    }
}

/// Whether the PC runs on its battery, None when Windows doesn't know.
pub fn on_battery() -> Option<bool> {
    let mut status = PowerStatus {
        ac_line: 255,
        battery_flag: 255,
        battery_percent: 255,
        saver: 0,
        life_time: 0,
        full_life_time: 0,
    };
    // SAFETY: `status` is a SYSTEM_POWER_STATUS, alive for the call.
    let ok = unsafe { GetSystemPowerStatus(&mut status) } != 0;
    match (ok, status.ac_line) {
        (true, 0) => Some(true),
        (true, 1) => Some(false),
        _ => None,
    }
}

/// The main screen's desktop mode, "2560x1440@144". The game may run at another size.
pub fn screen() -> String {
    // SAFETY: DEVMODEW is plain data; all zeroes is a valid value before the call fills it.
    let mut mode: DevMode = unsafe { std::mem::zeroed() };
    mode.size = size_of::<DevMode>() as u16;
    const ENUM_CURRENT_SETTINGS: u32 = u32::MAX;
    // SAFETY: a null device names the main screen; `mode` is a DEVMODEW with its size set.
    let ok = unsafe { EnumDisplaySettingsW(std::ptr::null(), ENUM_CURRENT_SETTINGS, &mut mode) };
    if ok == 0 {
        return "unknown".into();
    }
    format!("{}x{}@{}", mode.width, mode.height, mode.frequency)
}

/// Windows' information sound, the signal to close the game.
pub fn beep() {
    const MB_ICONASTERISK: u32 = 0x40;
    // SAFETY: MessageBeep takes a sound type and nothing else.
    unsafe {
        MessageBeep(MB_ICONASTERISK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win32_structs_have_their_sizes() {
        assert_eq!(size_of::<DevMode>(), 220);
        assert_eq!(size_of::<MemoryStatus>(), 64);
        assert_eq!(size_of::<PowerStatus>(), 12);
    }

    #[test]
    fn drivers_from_the_registry() {
        let text = r"
HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0000
    DriverDesc    REG_SZ    NVIDIA GeForce RTX 3060 Ti
    DriverVersion    REG_SZ    32.0.16.2140
    MatchingDeviceId    REG_SZ    PCI\VEN_10DE&DEV_2489

HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0000\Settings
    Device Description    REG_SZ    something else

HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0001
    DriverDesc    REG_SZ    AMD Radeon(TM) Graphics
    DriverVersion    REG_SZ    31.0.21912.14
    RadeonSoftwareVersion    REG_SZ    24.3.1

HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0002
    DriverDesc    REG_SZ    Microsoft Basic Display Adapter
";
        let list = parse_adapters(text);
        assert_eq!(list.len(), 2);
        assert_eq!(
            driver(&list, "NVIDIA GeForce RTX 3060 Ti"),
            "621.40 (32.0.16.2140)"
        );
        assert_eq!(
            driver(&list, "AMD Radeon(TM) Graphics"),
            "Adrenalin 24.3.1 (31.0.21912.14)"
        );
        assert_eq!(driver(&list, "Other"), "unknown");
    }

    #[test]
    fn windows_11_is_named_so() {
        let values: BTreeMap<String, String> = [
            ("ProductName", "Windows 10 Pro"),
            ("DisplayVersion", "25H2"),
            ("CurrentBuild", "26200"),
            ("UBR", "0x4d2"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        assert_eq!(windows_name(&values), "Windows 11 Pro 25H2 26200.1234");
        assert_eq!(
            reg_value("    ProcessorNameString    REG_SZ    AMD Ryzen 5 5600 6-Core Processor"),
            Some(("ProcessorNameString", "AMD Ryzen 5 5600 6-Core Processor"))
        );
    }

    #[test]
    fn folders_from_config() {
        let text = "config file : (none)\ngame_dir    : (not set)\ncache_dir   : C:\\x\\cache\n\
                    data_dir    : C:\\x\\data\n";
        let dirs = parse_dirs(text);
        assert_eq!(dirs.cache.as_deref(), Some(Path::new(r"C:\x\cache")));
        assert_eq!(dirs.data.as_deref(), Some(Path::new(r"C:\x\data")));
        assert!(parse_dirs("").cache.is_none());
        let empty = std::env::temp_dir().join("leonida-launcher-shaders-test");
        std::fs::create_dir_all(empty.join("shaders")).unwrap();
        assert!(
            !shaders_cached(Some(&empty)),
            "an empty cache needs a warm-up"
        );
        std::fs::write(empty.join("shaders").join("a"), "").unwrap();
        assert!(shaders_cached(Some(&empty)));
        assert!(shaders_cached(None));
        std::fs::remove_dir_all(&empty).unwrap();
    }

    #[test]
    fn release_builds_are_told_apart() {
        let release = parse_version(Some("version=0.1.0\nbuild=42\ncommit=4b28ba1\n"), None);
        assert!(release.release);
        assert_eq!(
            (release.build.as_str(), release.commit.as_str()),
            ("42", "4b28ba1")
        );
        let local = parse_version(Some("version=0.1.0\nbuild=local\ncommit=local\n"), None);
        assert!(!local.release);
        let old = parse_version(None, Some("leonida 0.1.0\n"));
        assert!(!old.release);
        assert_eq!(old.version, "0.1.0");
    }
}
