//! Guided setting tests: what to test on this PC, the runs (leonida.exe started by the
//! launcher with its log piped in and reduced to figures), and the report the player shares
//! themselves. Test runs use Recommended plus the one tested setting, so reports from
//! different PCs compare the same thing.

use std::collections::BTreeMap;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{self, BufRead, BufReader};
use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{env, fs, thread};

use serde::{Deserialize, Serialize};

use crate::figures::{self, Seen};
use crate::leonida::{Card, Profile};
use crate::pc::{self, Plan};
use crate::remote::{Switch, Wanted};
use crate::report::{self, Answers, Facts, Run, Setting};

/// Seconds of play before the beep that says "close the game now": 3 minutes asked, half a
/// minute of slack for getting settled.
pub const SPEED_PLAY_S: f64 = 210.0;
/// A crash test plays until it stops or this long.
pub const CRASH_PLAY_S: f64 = 600.0;
const DISCORD_SERVER: &str = "1574773017114841229";
const DISCORD_CHANNEL: &str = "1575267731857739916";
pub const DISCORD_INVITE: &str = "https://discord.gg/dxMD2XqyEJ";

pub type Shared = Arc<Mutex<Trial>>;

pub fn lock(trial: &Mutex<Trial>) -> MutexGuard<'_, Trial> {
    trial.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the launcher knows when a test is set up.
pub struct Context {
    pub exe: PathBuf,
    pub game_dir: String,
    pub switches: Vec<Switch>,
    pub wanted: Vec<Wanted>,
    pub profiles: Vec<Profile>,
    pub list: String,
}

/// Everything a test needs from the PC, gathered once.
struct Ready {
    exe: PathBuf,
    game_dir: String,
    facts: Facts,
    /// The profiles Leonida picks for this card by itself, base included.
    card_profiles: Vec<String>,
    /// The names from the shipped or downloaded catalog: only these appear in reports.
    catalog: Vec<String>,
    other: Vec<String>,
    /// What Leonida reported in effect without the launcher's variables: leonida.toml and the
    /// card's profiles.
    settings: BTreeMap<String, String>,
    data_dir: Option<PathBuf>,
    warmed: bool,
    list: String,
    choices: Vec<Choice>,
    /// A test from an earlier launcher session with runs but no report, same build and card.
    unfinished: Option<Test>,
}

#[derive(Default)]
pub struct Trial {
    ready: Option<Ready>,
    test: Option<Test>,
    live: Option<Live>,
}

/// One thing a player can test.
#[derive(Clone, Serialize)]
pub struct Choice {
    /// "switch:NAME", "profile:name" or "none".
    pub key: String,
    pub kind: String,
    pub name: String,
    pub title: String,
    pub about: String,
    /// "speed", "crash" or "baseline".
    pub test: String,
    pub values: Vec<String>,
    pub value: String,
    pub risk: String,
    /// The branch a switch needs, empty for main.
    pub build: String,
    /// What Leonida uses when the switch isn't set, when the list says.
    pub default: String,
    /// For a profile: what was measured with it, from gpu-profiles.json.
    pub measured: String,
    /// Why the maintainers want it, for the suggested test.
    pub why: String,
    /// The date this PC last tested it, if it did.
    pub tested: String,
}

#[derive(Serialize)]
pub struct SetupView {
    pub card: String,
    pub release: bool,
    pub leonida: String,
    pub warm_needed: bool,
    pub suggested: Choice,
    pub choices: Vec<Choice>,
    pub branch: Vec<Choice>,
    /// Said when Leonida didn't report the card.
    pub problem: String,
    /// A test closed half-way, which can go on.
    pub unfinished: Option<Unfinished>,
    /// A test of this launcher session, still open.
    pub open: bool,
}

#[derive(Serialize)]
pub struct Unfinished {
    title: String,
    value_label: String,
    done: usize,
    of: usize,
    created: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct Test {
    id: String,
    created: String,
    kind: String,
    setting: Setting,
    /// Whether each run of the pair has the setting, in the order they run.
    order: Vec<bool>,
    pair: u32,
    runs: Vec<Run>,
    warmup: String,
    before: String,
    where_: String,
    stops: (u32, u32),
    earlier: String,
    play_s: f64,
    profiles: Vec<String>,
    other: Vec<String>,
    /// Set in the run as usual: Leonida's default for the tested switch, so leonida.toml or a
    /// profile can't make both runs the same.
    #[serde(default)]
    reset: Option<String>,
    /// The tested switch as the run as usual kept it, when its default isn't known.
    #[serde(default)]
    kept: String,
    list: String,
    facts: Facts,
    folder: String,
    report: String,
}

struct Live {
    warm: bool,
    with: bool,
    started: Instant,
    /// Seconds from the spawn to the end, once ended.
    ended_s: Option<f64>,
    started_at: SystemTime,
    seen: Seen,
    stopping: bool,
    /// Stopped because the launcher closes: the run is dropped, not counted as a freeze.
    leaving: bool,
    beeped: bool,
    ended: bool,
    warm_ok: bool,
    battery: Option<bool>,
    play_s: f64,
}

/// The running or ended run, without saying whether it has the setting.
#[derive(Serialize)]
pub struct LiveView {
    warm: bool,
    number: usize,
    of: usize,
    elapsed_s: f64,
    played_s: f64,
    frames: bool,
    beeped: bool,
    stopping: bool,
    ended: bool,
    warm_ok: bool,
    play_s: f64,
}

#[derive(Serialize)]
pub struct RunView {
    ending: String,
    words: String,
    stopped: bool,
    short: bool,
    figures: bool,
    played_s: f64,
    run_s: f64,
}

#[derive(Serialize)]
pub struct TestView {
    id: String,
    kind: String,
    title: String,
    value_label: String,
    pair: u32,
    done: usize,
    of: usize,
    last: Option<RunView>,
    warmup: String,
    where_: String,
    before: String,
    stops: (u32, u32),
    report: String,
    chars: usize,
}

#[derive(Serialize)]
pub struct Status {
    live: Option<LiveView>,
    test: Option<TestView>,
}

fn random() -> u64 {
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u128(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    hasher.finish()
}

fn tests_dir() -> Option<PathBuf> {
    let base = PathBuf::from(env::var_os("LOCALAPPDATA")?);
    Some(base.join("leonida-launcher").join("tests"))
}

/// A switch value as the window shows it.
pub fn shown(value: &str) -> String {
    match value {
        "" => "Default".into(),
        "0" => "Off (0)".into(),
        "1" => "On (1)".into(),
        other => other.into(),
    }
}

#[derive(Serialize, Deserialize)]
struct WarmMark {
    size: u64,
    modified: u64,
    build: String,
}

fn warm_mark(exe: &Path, build: &str) -> Option<WarmMark> {
    let meta = fs::metadata(exe).ok()?;
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(WarmMark {
        size: meta.len(),
        modified: modified.as_secs(),
        build: build.into(),
    })
}

fn warmed(exe: &Path, build: &str) -> bool {
    let (Some(now), Some(dir)) = (warm_mark(exe, build), tests_dir()) else {
        return false;
    };
    fs::read_to_string(dir.join("warm.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<WarmMark>(&text).ok())
        .is_some_and(|saved| {
            (saved.size, saved.modified, saved.build) == (now.size, now.modified, now.build)
        })
}

/// Which settings this PC tested, and when: key and value to date.
fn tested() -> BTreeMap<String, String> {
    let Some(dir) = tests_dir() else {
        return BTreeMap::new();
    };
    fs::read_to_string(dir.join("index.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn base_env(game_dir: &str) -> Vec<(String, String)> {
    vec![("LEONIDA_GAME_DIR".into(), game_dir.into())]
}

/// Switches in effect besides Leonida's defaults (code and the base profile) and the tested
/// one, as reports may show them. The launcher's own variables are never listed.
fn other_switches(plan: &Plan, switches: &[Switch]) -> Vec<String> {
    plan.settings
        .iter()
        .filter(|(name, _)| {
            plan.sources.get(*name).map(String::as_str) != Some("profile:base")
                && ![
                    "LEONIDA_GAME_DIR",
                    "LEONIDA_FRAME_LOG",
                    "LEONIDA_LOG",
                    "LEONIDA_PROFILE",
                ]
                .contains(&name.as_str())
        })
        .map(|(name, value)| {
            let known: Vec<String> = switches
                .iter()
                .find(|s| format!("LEONIDA_{}", s.name) == *name)
                .map(|s| s.values.clone())
                .unwrap_or_default();
            report::safe_switch(name, value, &known)
        })
        .collect()
}

fn switch_choice(s: &Switch, tested: &BTreeMap<String, String>) -> Option<Choice> {
    let values: Vec<String> = s
        .values
        .iter()
        .filter(|v| **v != s.default)
        .cloned()
        .collect();
    let value = values.first()?.clone();
    let key = format!("switch:{}", s.name);
    Some(Choice {
        tested: tested.get(&key).cloned().unwrap_or_default(),
        key,
        kind: "switch".into(),
        name: s.name.clone(),
        title: if s.title.is_empty() {
            s.name.clone()
        } else {
            s.title.clone()
        },
        about: s.about.clone(),
        test: s.test.clone(),
        values,
        value,
        risk: s.risk.clone(),
        build: s.build.clone(),
        default: s.default.clone(),
        measured: String::new(),
        why: String::new(),
    })
}

fn profile_choice(p: &Profile, tested: &BTreeMap<String, String>) -> Choice {
    let key = format!("profile:{}", p.name);
    Choice {
        tested: tested.get(&key).cloned().unwrap_or_default(),
        key,
        kind: "profile".into(),
        name: p.name.clone(),
        title: format!("Settings measured on {}", p.matcher.describe()),
        about: "A set of settings that helped on a card like that. Leonida doesn't turn it on \
                by itself yet: it needs figures from more cards first."
            .into(),
        test: "speed".into(),
        values: vec!["on".into()],
        value: "on".into(),
        risk: String::new(),
        build: String::new(),
        default: String::new(),
        measured: p.measured.clone(),
        why: String::new(),
    }
}

fn baseline() -> Choice {
    Choice {
        key: "none".into(),
        kind: "none".into(),
        name: String::new(),
        title: "Check how well your PC runs it".into(),
        about: "One run with nothing changed. Figures from cards nobody has measured yet are \
                what the graphics profiles need most."
            .into(),
        test: "baseline".into(),
        values: Vec::new(),
        value: String::new(),
        risk: String::new(),
        build: String::new(),
        default: String::new(),
        measured: String::new(),
        why: String::new(),
        tested: String::new(),
    }
}

/// The choices for this card: the suggested test first, then the main build's, then the
/// branch builds' (only for a build that isn't a release). Profiles in `active` (Leonida turns
/// them on already) aren't offered: both runs would be the same.
fn choices(
    ctx: &Context,
    card: Option<&Card>,
    release: bool,
    tested: &BTreeMap<String, String>,
    active: &[String],
) -> (Choice, Vec<Choice>, Vec<Choice>) {
    let fits = |m: &crate::leonida::Matcher| card.map_or(m.is_empty(), |c| m.fits(c));
    let offered = |s: &&Switch| !s.test.is_empty() && (s.build.is_empty() || !release);
    let off = |p: &Profile| !p.local && !active.contains(&p.name);
    let mut main: Vec<Choice> = ctx
        .switches
        .iter()
        .filter(|s| offered(s) && s.build.is_empty())
        .filter_map(|s| switch_choice(s, tested))
        .collect();
    main.extend(
        ctx.profiles
            .iter()
            .filter(|p| off(p) && p.opt_in && !p.matcher.is_empty() && fits(&p.matcher))
            .map(|p| profile_choice(p, tested)),
    );
    let branch: Vec<Choice> = ctx
        .switches
        .iter()
        .filter(|s| offered(s) && !s.build.is_empty())
        .filter_map(|s| switch_choice(s, tested))
        .collect();
    let suggested = ctx
        .wanted
        .iter()
        .filter(|w| fits(&w.matcher))
        .find_map(|w| {
            let mut choice = if w.profile.is_empty() {
                let s = ctx
                    .switches
                    .iter()
                    .filter(offered)
                    .find(|s| s.name == w.switch)?;
                switch_choice(s, tested)?
            } else {
                let p = ctx
                    .profiles
                    .iter()
                    .find(|p| off(p) && p.name == w.profile)?;
                profile_choice(p, tested)
            };
            if !w.value.is_empty() && choice.values.contains(&w.value) {
                choice.value = w.value.clone();
            }
            choice.why = w.why.clone();
            Some(choice)
        })
        .unwrap_or_else(baseline);
    main.retain(|c| c.key != suggested.key);
    if suggested.key != "none" {
        main.push(baseline());
    }
    (suggested, main, branch)
}

/// Whether this leonida.exe knows the profile. A catalog downloaded from GitHub can be newer,
/// and Leonida stops at once with "unknown graphics profile" on a name it doesn't know.
fn profile_known(exe: &Path, env: &[(String, String)], name: &str) -> bool {
    let mut env = env.to_vec();
    env.push(("LEONIDA_PROFILE".into(), name.into()));
    pc::plan(exe, &env).is_some()
}

fn same_build(a: &Facts, b: &Facts) -> bool {
    (&a.version, &a.build, &a.commit, &a.gpu) == (&b.version, &b.build, &b.commit, &b.gpu)
}

/// The newest saved test with runs but no report, made with this build on this card.
fn unfinished(facts: &Facts) -> Option<Test> {
    let dir = tests_dir()?;
    let mut folders: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    folders.sort();
    let newest = folders.pop()?;
    let text = fs::read_to_string(newest.join("test.json")).ok()?;
    let test: Test = serde_json::from_str(&text).ok()?;
    (!test.runs.is_empty() && test.report.is_empty() && same_build(&test.facts, facts))
        .then_some(test)
}

/// A folder name the launcher made itself, "20261127-180512-7f3a9c", so a hand-edited
/// test.json can't point a delete elsewhere.
fn own_folder(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}

/// Looks at the PC and leonida.exe (`version`, `config --graphics`, `config`; never `run` or
/// `translate`). Takes a few seconds, so call it off the UI thread.
pub fn setup(trial: &Mutex<Trial>, ctx: Context) -> SetupView {
    let version = pc::version(&ctx.exe);
    let env = base_env(&ctx.game_dir);
    let plan = pc::plan(&ctx.exe, &env);
    let card = plan.as_ref().and_then(|p| p.gpu.clone());
    let adapters = pc::adapters();
    let card_name = card.as_ref().map_or("", |c| c.name.as_str());
    let facts = Facts {
        gpu: card_name.to_string(),
        vram_mb: card.as_ref().map_or(0, |c| c.vram_mb),
        adapters: adapters.len(),
        driver: pc::driver(&adapters, card_name),
        cpu: pc::cpu(),
        ram_gb: pc::ram_gb(),
        windows: pc::windows(),
        screen: pc::screen(),
        launcher: env!("CARGO_PKG_VERSION").into(),
        version: version.version.clone(),
        build: version.build.clone(),
        commit: version.commit.clone(),
        release: version.release,
    };
    let tested = tested();
    let active = plan
        .as_ref()
        .map(|p| p.profiles.clone())
        .unwrap_or_default();
    let (mut suggested, mut main, branch) =
        choices(&ctx, card.as_ref(), version.release, &tested, &active);
    let known = |c: &Choice| c.kind != "profile" || profile_known(&ctx.exe, &env, &c.name);
    main.retain(known);
    if !known(&suggested) {
        suggested = baseline();
        main.retain(|c| c.key != "none");
    }
    let dirs = pc::dirs(&ctx.exe, &env);
    let warmed = warmed(&ctx.exe, &version.build) && pc::shaders_cached(dirs.cache.as_deref());
    let catalog: Vec<String> = ctx
        .profiles
        .iter()
        .filter(|p| !p.local)
        .map(|p| p.name.clone())
        .chain(["base".to_string()])
        .collect();
    let leonida = if version.release {
        format!("Leonida Legacy build {}", version.build)
    } else if version.version.is_empty() {
        "Leonida Legacy, version unknown (not a release build)".into()
    } else {
        format!("Leonida Legacy {} (not a release build)", version.version)
    };
    let mut trial = lock(trial);
    let running = trial.live.as_ref().is_some_and(|l| !l.ended);
    // A test of this session made with another leonida.exe or card can't go on: its runs
    // would compare two builds. It stays saved.
    if !running
        && trial
            .test
            .as_ref()
            .is_some_and(|t| !same_build(&t.facts, &facts))
    {
        trial.test = None;
        trial.live = None;
    }
    let unfinished =
        unfinished(&facts).filter(|u| trial.test.as_ref().is_none_or(|t| t.folder != u.folder));
    let view = SetupView {
        card: card.as_ref().map_or_else(
            || "unknown".into(),
            |c| format!("{}, {} MB", c.name, c.vram_mb),
        ),
        release: version.release,
        leonida,
        warm_needed: !warmed,
        unfinished: unfinished.as_ref().map(|t| Unfinished {
            title: t.setting.title.clone(),
            value_label: t.setting.value_label.clone(),
            done: t.runs.len(),
            of: t.order.len(),
            created: t.created.clone(),
        }),
        open: trial.test.is_some(),
        suggested: suggested.clone(),
        choices: main.clone(),
        branch: branch.clone(),
        problem: if plan.is_none() {
            "Leonida didn't say which graphics card it uses, so nothing is suggested for your \
             card. You can still test."
                .into()
        } else {
            String::new()
        },
    };
    let ready = Ready {
        data_dir: dirs.data,
        warmed,
        other: plan
            .as_ref()
            .map(|p| other_switches(p, &ctx.switches))
            .unwrap_or_default(),
        settings: plan
            .as_ref()
            .map(|p| p.settings.clone())
            .unwrap_or_default(),
        card_profiles: active,
        exe: ctx.exe,
        game_dir: ctx.game_dir,
        facts,
        catalog,
        list: ctx.list,
        choices: [suggested].into_iter().chain(main).chain(branch).collect(),
        unfinished,
    };
    // Kept while a run goes: it started with the old one and ends against it.
    if !running {
        trial.ready = Some(ready);
    }
    view
}

/// Goes on with the unfinished test (`keep`), or deletes it.
pub fn resume(trial: &Mutex<Trial>, keep: bool) -> Result<Option<TestView>, String> {
    let mut trial = lock(trial);
    if trial.live.as_ref().is_some_and(|l| !l.ended) {
        return Err("A test run is going. Finish it first.".into());
    }
    let test = trial
        .ready
        .as_mut()
        .and_then(|r| r.unfinished.take())
        .ok_or("There's no unfinished test any more.")?;
    if !keep {
        if let Some(dir) = tests_dir().filter(|_| own_folder(&test.folder)) {
            let _ = fs::remove_dir_all(dir.join(&test.folder));
        }
        return Ok(None);
    }
    trial.live = None;
    trial.test = Some(test);
    Ok(view(&trial).test)
}

#[derive(Deserialize)]
pub struct Begin {
    key: String,
    value: String,
    /// For a crash test: "keeps stopping" or "sometimes".
    #[serde(default)]
    before: String,
}

fn coin() -> bool {
    random() & 1 == 1
}

fn order(kind: &str, before: &str) -> Vec<bool> {
    match kind {
        "baseline" => vec![false],
        "crash" if before == "keeps stopping" => vec![true],
        _ if coin() => vec![true, false],
        _ => vec![false, true],
    }
}

pub fn begin(trial: &Mutex<Trial>, begin: Begin) -> Result<TestView, String> {
    let mut trial = lock(trial);
    if trial.live.as_ref().is_some_and(|l| !l.ended) {
        return Err("A test run is going. Finish it first.".into());
    }
    let Some(ready) = &trial.ready else {
        return Err("The launcher hasn't looked at your PC yet. Open the test again.".into());
    };
    let Some(choice) = ready.choices.iter().find(|c| c.key == begin.key) else {
        return Err("That setting isn't in the list any more. Pick another.".into());
    };
    let value = if choice.values.contains(&begin.value) {
        begin.value
    } else {
        choice.value.clone()
    };
    let value_label = match (choice.kind.as_str(), choice.test.as_str()) {
        ("switch", "crash") if value == "0" => "turned off".into(),
        ("switch", "crash") => format!("set to {}", shown(&value)),
        ("switch", _) => shown(&value),
        ("profile", _) => "on".into(),
        _ => String::new(),
    };
    let profiles = ready
        .card_profiles
        .iter()
        .map(|p| {
            if ready.catalog.contains(p) {
                p.clone()
            } else {
                "<own profile>".into()
            }
        })
        .collect();
    let other = ready
        .other
        .iter()
        .filter(|o| choice.kind != "switch" || o.split('=').next() != Some(choice.name.as_str()))
        .cloned()
        .collect();
    // leonida.toml or a profile may set the tested switch too. With Leonida's default known,
    // the run as usual sets it back; otherwise the report says what that run kept.
    let (reset, kept) = match ready.settings.get(&format!("LEONIDA_{}", choice.name)) {
        _ if choice.kind != "switch" => (None, String::new()),
        _ if !choice.default.is_empty() => (Some(choice.default.clone()), String::new()),
        Some(set) => {
            let name = format!("LEONIDA_{}", choice.name);
            (None, report::safe_switch(&name, set, &choice.values))
        }
        None => (None, String::new()),
    };
    let now = SystemTime::now();
    let id = format!("{:06x}", random() & 0xFF_FFFF);
    let test = Test {
        folder: format!("{}-{id}", crate::clock::stamp(now)),
        id,
        created: crate::clock::iso_date(now),
        kind: choice.test.clone(),
        setting: Setting {
            kind: choice.kind.clone(),
            name: choice.name.clone(),
            title: choice.title.clone(),
            value,
            value_label,
        },
        order: order(&choice.test, &begin.before),
        pair: 1,
        runs: Vec::new(),
        warmup: if ready.warmed {
            "done earlier (same leonida.exe)".into()
        } else {
            "skipped".into()
        },
        before: match begin.before.as_str() {
            "keeps stopping" => "the game kept stopping".into(),
            "sometimes" => "the game stopped sometimes".into(),
            _ => String::new(),
        },
        where_: String::new(),
        stops: (0, 0),
        earlier: String::new(),
        play_s: if choice.test == "crash" {
            CRASH_PLAY_S
        } else {
            SPEED_PLAY_S
        },
        profiles,
        other,
        reset,
        kept,
        list: ready.list.clone(),
        facts: ready.facts.clone(),
        report: String::new(),
    };
    if let Some(ready) = trial.ready.as_mut() {
        ready.unfinished = None;
    }
    trial.live = None;
    trial.test = Some(test);
    Ok(view(&trial).test.expect("the test was just set"))
}

fn view(trial: &Trial) -> Status {
    let live = trial.live.as_ref().map(|l| {
        let elapsed = l
            .ended_s
            .unwrap_or_else(|| l.started.elapsed().as_secs_f64());
        let done = trial.test.as_ref().map_or(0, |t| t.runs.len());
        LiveView {
            warm: l.warm,
            number: if l.ended { done } else { done + 1 },
            of: trial.test.as_ref().map_or(1, |t| t.order.len()),
            elapsed_s: elapsed,
            played_s: l.seen.play_start().map_or(0.0, |s| (elapsed - s).max(0.0)),
            frames: !l.seen.readings.is_empty() || !l.seen.samples.is_empty(),
            beeped: l.beeped,
            stopping: l.stopping,
            ended: l.ended,
            warm_ok: l.warm_ok,
            play_s: l.play_s,
        }
    });
    let test = trial.test.as_ref().map(|t| TestView {
        id: t.id.clone(),
        kind: t.kind.clone(),
        title: t.setting.title.clone(),
        value_label: t.setting.value_label.clone(),
        pair: t.pair,
        done: t.runs.len(),
        of: t.order.len(),
        last: t.runs.last().map(|r| RunView {
            ending: r.ending.key().into(),
            words: r.ending.words().into(),
            stopped: r.ending.stopped(),
            short: r.short(),
            figures: r.window.is_some(),
            played_s: r.played_s,
            run_s: r.run_s,
        }),
        warmup: t.warmup.clone(),
        where_: t.where_.clone(),
        before: t.before.clone(),
        // Only once the pair is done: it goes up after a run with the setting, which would
        // tell the player which run had it before they say how it felt.
        stops: if t.runs.len() >= t.order.len() {
            t.stops
        } else {
            (0, 0)
        },
        chars: t.report.chars().count(),
        report: t.report.clone(),
    });
    Status { live, test }
}

pub fn status(trial: &Mutex<Trial>) -> Status {
    view(&lock(trial))
}

/// Whether a run is going, for the close button and Play.
pub fn running(trial: &Mutex<Trial>) -> bool {
    lock(trial).live.as_ref().is_some_and(|l| !l.ended)
}

fn env_for(ready: &Ready, test: &Test, with: bool) -> Vec<(String, String)> {
    let mut env = base_env(&ready.game_dir);
    env.push(("LEONIDA_FRAME_LOG".into(), "1".into()));
    env.push(("LEONIDA_LOG".into(), "info".into()));
    if with {
        let s = &test.setting;
        match s.kind.as_str() {
            "switch" => env.push((format!("LEONIDA_{}", s.name), s.value.clone())),
            "profile" => {
                let mut names: Vec<&str> = ready
                    .card_profiles
                    .iter()
                    .map(String::as_str)
                    .filter(|n| *n != "base" && *n != s.name)
                    .collect();
                names.push(&s.name);
                env.push(("LEONIDA_PROFILE".into(), names.join(",")));
            }
            _ => {}
        }
    } else if let ("switch", Some(default)) = (test.setting.kind.as_str(), &test.reset) {
        env.push((format!("LEONIDA_{}", test.setting.name), default.clone()));
    }
    env
}

/// Starts the warm-up (`warm`) or the next run of the pair, without a console window. `exe`
/// and `game_dir` are the launcher's current ones: a test set up with others can't go on.
pub fn start(
    shared: &Shared,
    warm: bool,
    exe: &Path,
    game_dir: &str,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let mut trial = lock(shared);
    if trial.live.as_ref().is_some_and(|l| !l.ended) {
        return Err("A test run is going already.".into());
    }
    let (Some(ready), Some(test)) = (&trial.ready, &trial.test) else {
        return Err("Pick what to test first.".into());
    };
    if ready.exe != exe || ready.game_dir != game_dir {
        return Err(
            "You picked another leonida.exe or game folder since this test was set up. Close \
             Test a setting and open it again."
                .into(),
        );
    }
    if !Path::new(&ready.game_dir).is_dir() {
        return Err(
            "Leonida can't find the game folder any more. Pick it again on the home screen, \
             then start this run again. The runs so far are kept."
                .into(),
        );
    }
    if !warm && test.runs.len() >= test.order.len() {
        return Err("Both runs are done. See the report.".into());
    }
    let with = !warm && test.order[test.runs.len()];
    let (args, env): (&[&str], _) = if warm {
        (&["translate", "--warm", "all"], base_env(&ready.game_dir))
    } else {
        (&["run", "--execute"], env_for(ready, test, with))
    };
    let mut command = pc::leonida(&ready.exe, &env);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = command
        .spawn()
        .map_err(|error| format!("Couldn't start leonida.exe: {error}"))?;
    let data_dir = ready.data_dir.clone();
    let exe = ready.exe.clone();
    let build = ready.facts.build.clone();
    let play_s = test.play_s;
    let started = Instant::now();
    trial.live = Some(Live {
        warm,
        with,
        started,
        ended_s: None,
        started_at: SystemTime::now(),
        seen: Seen::default(),
        stopping: false,
        leaving: false,
        beeped: false,
        ended: false,
        warm_ok: false,
        battery: pc::on_battery(),
        play_s,
    });
    drop(trial);
    watch(shared.clone(), child, started, data_dir, exe, build, app);
    Ok(())
}

/// The threads of a run: stdout drained and dropped, stderr lines queued raw by one thread
/// and parsed by another (so the game's log writes never wait on the parsing), and one that
/// waits for the end.
fn watch(
    shared: Shared,
    mut child: std::process::Child,
    started: Instant,
    data_dir: Option<PathBuf>,
    exe: PathBuf,
    build: String,
    app: tauri::AppHandle,
) {
    if let Some(mut out) = child.stdout.take() {
        thread::spawn(move || io::copy(&mut out, &mut io::sink()));
    }
    let (lines, queue) = mpsc::channel::<(f64, Vec<u8>)>();
    if let Some(err) = child.stderr.take() {
        thread::spawn(move || {
            let mut reader = BufReader::with_capacity(64 * 1024, err);
            let mut line = Vec::new();
            let mut parsing = true;
            loop {
                line.clear();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    // Without a parser the lines are still read and dropped: a full pipe
                    // would stop the game at its next log line.
                    Ok(_) if parsing => {
                        let at = started.elapsed().as_secs_f64();
                        parsing = lines.send((at, line.clone())).is_ok();
                    }
                    Ok(_) => {}
                }
            }
        });
    }
    let (parsed, drained) = mpsc::channel::<()>();
    let parser = shared.clone();
    thread::spawn(move || {
        for (at, raw) in queue {
            let text = String::from_utf8_lossy(&raw);
            if !figures::worth(&text) {
                continue;
            }
            let Some(line) = figures::parse(&text) else {
                continue;
            };
            let mut trial = lock(&parser);
            let Some(live) = trial.live.as_mut() else {
                continue;
            };
            live.seen.take(at, line);
            let play = live.seen.play_start().map(|s| at - s);
            if !live.warm && !live.beeped && play.is_some_and(|p| p >= live.play_s) {
                live.beeped = true;
                thread::spawn(|| {
                    pc::beep();
                    thread::sleep(Duration::from_millis(450));
                    pc::beep();
                });
            }
        }
        let _ = parsed.send(());
    });
    thread::spawn(move || {
        let mut killed = false;
        let code = loop {
            if !killed && lock(&shared).live.as_ref().is_some_and(|l| l.stopping) {
                killed = child.kill().is_ok();
            }
            match child.try_wait() {
                Ok(Some(status)) => break status.code(),
                Ok(None) => thread::sleep(Duration::from_millis(250)),
                Err(_) => break None,
            }
        };
        // The last lines are still being parsed; a helper the game started could keep the
        // pipe open, so this waits only a moment.
        let _ = drained.recv_timeout(Duration::from_secs(3));
        finish(&shared, code, killed, data_dir.as_deref(), &exe, &build);
        bring_forward(&app);
    });
}

fn bring_forward(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.set_focus();
        let _ = window.request_user_attention(Some(tauri::UserAttentionType::Informational));
    }
}

fn finish(
    shared: &Mutex<Trial>,
    code: Option<i32>,
    killed: bool,
    data_dir: Option<&Path>,
    exe: &Path,
    build: &str,
) {
    let started_at = lock(shared).live.as_ref().map(|l| l.started_at);
    let crash = started_at.map_or("unknown", |since| pc::crash_written(data_dir, since));
    let battery = pc::on_battery();
    let mut guard = lock(shared);
    let trial = &mut *guard;
    let Some(live) = trial.live.as_mut() else {
        return;
    };
    let end = live.started.elapsed().as_secs_f64();
    live.ended = true;
    live.ended_s = Some(end);
    if live.warm {
        live.warm_ok = code == Some(0) && !killed;
        if live.warm_ok {
            if let (Some(mark), Some(dir)) = (warm_mark(exe, build), tests_dir())
                && fs::create_dir_all(&dir).is_ok()
                && let Ok(text) = serde_json::to_string(&mark)
            {
                let _ = fs::write(dir.join("warm.json"), text);
            }
            if let Some(test) = trial.test.as_mut() {
                test.warmup = "done before testing".into();
            }
            if let Some(ready) = trial.ready.as_mut() {
                ready.warmed = true;
            }
        }
        return;
    }
    if live.leaving {
        return;
    }
    let seen = &live.seen;
    let ending = figures::ending(code, killed, seen.gpu_failure, seen.device_removed);
    let run = Run {
        with: live.with,
        window: seen.summary(end, figures::WINDOW_S),
        last60: seen.summary(end, figures::WINDOW_SHORT_S),
        whole: seen.whole(),
        played_s: seen.played(),
        run_s: end,
        ending,
        exit: code,
        device_removed: seen.device_removed,
        crash_report: crash.into(),
        battery: match (live.battery, battery) {
            (Some(a), Some(b)) => Some(a || b),
            (a, b) => a.or(b),
        },
        said: String::new(),
    };
    if let Some(test) = trial.test.as_mut() {
        if run.with {
            test.stops.1 += 1;
            if ending.stopped() {
                test.stops.0 += 1;
            }
        }
        test.runs.push(run);
        save(test);
    }
}

/// Asks a frozen run to stop; the launcher owns it, so this ends it.
pub fn stop(trial: &Mutex<Trial>) -> Result<(), String> {
    let mut trial = lock(trial);
    match trial.live.as_mut() {
        Some(live) if !live.ended => {
            live.stopping = true;
            Ok(())
        }
        _ => Err("No test run is going.".into()),
    }
}

/// Forgets the last run, to play it again. The stop count keeps it.
pub fn retry(trial: &Mutex<Trial>) -> Result<TestView, String> {
    let mut trial = lock(trial);
    if trial.live.as_ref().is_some_and(|l| !l.ended) {
        return Err("A test run is going.".into());
    }
    let test = trial.test.as_mut().ok_or("There's no test to repeat.")?;
    test.runs.pop();
    test.report.clear();
    trial.live = None;
    Ok(view(&trial).test.expect("the test is there"))
}

/// The player's answers as the window sends them.
#[derive(Deserialize)]
pub struct AnswersIn {
    #[serde(default)]
    where_: String,
    #[serde(default)]
    felt: String,
    #[serde(default)]
    looked: String,
    #[serde(default)]
    black: String,
    #[serde(default)]
    other: String,
    /// How the player says each run ended, when they corrected it ("closed", "froze", ...).
    #[serde(default)]
    said: Vec<String>,
}

#[derive(Serialize)]
pub struct ReportView {
    text: String,
    chars: usize,
    /// Longer than a Discord message even after cutting.
    over: bool,
    left_out: Vec<String>,
}

fn said_words(key: &str) -> &'static str {
    match key {
        "closed" => "I closed it",
        "stopped" => "it closed by itself",
        "froze" => "it froze",
        _ => "",
    }
}

/// Keeps where the player was after a run, cleaned, for the next run's reminder.
pub fn set_where(trial: &Mutex<Trial>, text: &str) -> String {
    let names = pc::names();
    let mut trial = lock(trial);
    let cleaned = report::clean(text, &names, report::WHERE_MAX);
    if let Some(test) = trial.test.as_mut() {
        test.where_ = cleaned.clone().unwrap_or_default();
    }
    if cleaned.is_none() && !text.trim().is_empty() {
        "That was left out because it had your user name, a file path or a log line in it. \
         Please write it again without that."
            .into()
    } else {
        String::new()
    }
}

pub fn make_report(trial: &Mutex<Trial>, answers: AnswersIn) -> Result<ReportView, String> {
    let names = pc::names();
    let mut trial = lock(trial);
    let test = trial.test.as_mut().ok_or("There's no test to report.")?;
    if test.runs.is_empty() {
        return Err("No run has ended yet.".into());
    }
    let mut left_out = Vec::new();
    let mut keep = |label: &str, text: &str, max: usize| {
        if text.trim().is_empty() {
            return String::new();
        }
        report::clean(text, &names, max).unwrap_or_else(|| {
            left_out.push(label.to_string());
            String::new()
        })
    };
    let where_ = if answers.where_.trim().is_empty() {
        test.where_.clone()
    } else {
        keep("Where", &answers.where_, report::WHERE_MAX)
    };
    let other = keep("Anything else", &answers.other, report::ELSE_MAX);
    let pick = |value: &str, allowed: &[&str]| {
        if allowed.contains(&value) {
            value.to_string()
        } else {
            String::new()
        }
    };
    // `felt` compares the second run with the first; the report needs it about the setting.
    let second_has_it = test.runs.get(1).is_some_and(|r| r.with);
    let felt = pick(
        &answers.felt,
        &["smoother", "about the same", "worse", "can't tell"],
    );
    let filled = Answers {
        where_,
        felt: report::felt_with(&felt, second_has_it),
        looked: pick(&answers.looked, &["no", "yes", "didn't look"]),
        black: pick(&answers.black, &["no", "yes, in one run", "yes, in both"]),
        other,
        before: test.before.clone(),
    };
    for (run, said) in test.runs.iter_mut().zip(&answers.said) {
        let words = said_words(said);
        let saw = if run.ending.stopped() {
            "stopped"
        } else {
            "closed"
        };
        run.said = if words.is_empty()
            || said == saw
            || (said == "froze" && run.ending == figures::Ending::Froze)
        {
            String::new()
        } else {
            words.into()
        };
    }
    let fitted = report::fit(&report::Input {
        id: &test.id,
        pair: test.pair,
        kind: &test.kind,
        setting: &test.setting,
        runs: &test.runs,
        facts: &test.facts,
        profiles: &test.profiles,
        other: &test.other,
        kept: &test.kept,
        warmup: &test.warmup,
        list: &test.list,
        answers: &filled,
        stops: test.stops,
        earlier: &test.earlier,
    });
    let text = fitted.text;
    test.where_ = filled.where_;
    test.report = text.clone();
    save(test);
    remember(test);
    Ok(ReportView {
        chars: text.chars().count(),
        text,
        over: fitted.over,
        left_out,
    })
}

/// A second pair of runs with the same setting, in a new random order.
pub fn again(trial: &Mutex<Trial>) -> Result<TestView, String> {
    let mut trial = lock(trial);
    if trial.live.as_ref().is_some_and(|l| !l.ended) {
        return Err("A test run is going. Stop it first.".into());
    }
    let test = trial.test.as_mut().ok_or("There's no test to repeat.")?;
    test.earlier = test
        .report
        .lines()
        .find_map(|l| l.strip_prefix("Result: "))
        .unwrap_or("")
        .to_string();
    test.pair += 1;
    test.runs.clear();
    test.report.clear();
    test.order = order(
        &test.kind,
        if test.order.len() == 1 {
            "keeps stopping"
        } else {
            ""
        },
    );
    trial.live = None;
    Ok(view(&trial).test.expect("the test is there"))
}

pub fn cancel(trial: &Mutex<Trial>) -> Result<(), String> {
    let mut trial = lock(trial);
    if trial.live.as_ref().is_some_and(|l| !l.ended) {
        return Err("A test run is going. Stop it first.".into());
    }
    // A first pair given up before its report: nothing in it to keep or to continue later.
    if let Some(test) = trial.test.take()
        && test.pair == 1
        && test.report.is_empty()
        && own_folder(&test.folder)
        && let Some(dir) = tests_dir()
    {
        let _ = fs::remove_dir_all(dir.join(&test.folder));
    }
    trial.live = None;
    Ok(())
}

/// Stops a run for the close button and waits up to about 3 s for the game to end, so the
/// launcher doesn't leave it running with nobody reading its log.
pub fn stop_now(shared: &Shared) {
    {
        let mut trial = lock(shared);
        match trial.live.as_mut() {
            Some(live) if !live.ended => {
                live.stopping = true;
                live.leaving = true;
            }
            _ => return,
        }
    }
    for _ in 0..30 {
        thread::sleep(Duration::from_millis(100));
        if !running(shared) {
            return;
        }
    }
}

/// test.json and the report beside it, in %LOCALAPPDATA%\leonida-launcher\tests.
fn save(test: &Test) {
    let Some(dir) = tests_dir().map(|d| d.join(&test.folder)) else {
        return;
    };
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(text) = serde_json::to_string_pretty(test) {
        let _ = fs::write(dir.join("test.json"), text);
    }
    if !test.report.is_empty() {
        let _ = fs::write(
            dir.join(format!("report-pair{}.txt", test.pair)),
            &test.report,
        );
    }
}

fn remember(test: &Test) {
    let Some(dir) = tests_dir() else { return };
    let mut index = tested();
    let key = match test.setting.kind.as_str() {
        "switch" | "profile" => format!("{}:{}", test.setting.kind, test.setting.name),
        _ => "none".into(),
    };
    index.insert(key, test.created.clone());
    if let Ok(text) = serde_json::to_string_pretty(&index) {
        let _ = fs::write(dir.join("index.json"), text);
    }
}

/// Opens #test-results in the Discord app when it is installed, otherwise in the browser.
pub fn open_discord() -> Result<(), String> {
    let app = Command::new(pc::windows_tool("reg.exe"))
        .args(["query", r"HKCR\discord"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(pc::CREATE_NO_WINDOW)
        .status()
        .is_ok_and(|s| s.success());
    let url = if app {
        format!("discord://-/channels/{DISCORD_SERVER}/{DISCORD_CHANNEL}")
    } else {
        format!("https://discord.com/channels/{DISCORD_SERVER}/{DISCORD_CHANNEL}")
    };
    open(&url)
}

pub fn open_invite() -> Result<(), String> {
    open(DISCORD_INVITE)
}

pub fn open_tests() -> Result<(), String> {
    let dir = tests_dir().ok_or("There's no tests folder.")?;
    fs::create_dir_all(&dir).map_err(|e| format!("Couldn't make the tests folder ({e})."))?;
    Command::new(pc::windows_tool("explorer.exe"))
        .arg(dir)
        .spawn()
        .map(drop)
        .map_err(|e| format!("Couldn't open the folder ({e})."))
}

fn open(url: &str) -> Result<(), String> {
    Command::new(pc::windows_tool("explorer.exe"))
        .arg(url)
        .spawn()
        .map(drop)
        .map_err(|e| format!("Couldn't open {url} ({e})."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::leonida::Matcher;

    fn switch(name: &str, values: &[&str], default: &str, test: &str, build: &str) -> Switch {
        Switch {
            name: name.into(),
            values: values.iter().map(|v| v.to_string()).collect(),
            title: name.into(),
            about: String::new(),
            risk: String::new(),
            build: build.into(),
            default: default.into(),
            test: test.into(),
        }
    }

    fn context() -> Context {
        let eight: Matcher = serde_json::from_str(
            r#"{"vendor": "nvidia", "min_vram_mb": 7000, "max_vram_mb": 8600}"#,
        )
        .unwrap();
        Context {
            exe: PathBuf::new(),
            game_dir: String::new(),
            switches: vec![
                switch(
                    "TEXTURE_STAGE_MB",
                    &["192", "512", "1024"],
                    "192",
                    "speed",
                    "",
                ),
                switch("ASYNC_SUBMIT", &["0"], "", "crash", ""),
                switch("WAVE_MORE_SKIPS", &["1"], "", "", "fix/x"),
                switch("THIN", &["1"], "", "speed", "perf/thin"),
            ],
            wanted: vec![Wanted {
                switch: String::new(),
                profile: "nvidia-8gb".into(),
                value: String::new(),
                matcher: eight.clone(),
                why: "Opt-in until measured on other 8 GB cards.".into(),
            }],
            profiles: vec![Profile {
                name: "nvidia-8gb".into(),
                opt_in: true,
                measured: String::new(),
                local: false,
                matcher: eight,
            }],
            list: "built-in".into(),
        }
    }

    fn card(vendor: &str, vram_mb: u64) -> Card {
        Card {
            name: "A card".into(),
            vendor: vendor.into(),
            vram_mb,
        }
    }

    #[test]
    fn the_suggestion_fits_the_card_and_defaults_are_not_offered() {
        let ctx = context();
        let none = BTreeMap::new();
        let eight = card("nvidia", 8192);
        let (suggested, main, branch) = choices(&ctx, Some(&eight), true, &none, &[]);
        assert_eq!(suggested.key, "profile:nvidia-8gb");
        assert_eq!(suggested.title, "Settings measured on 8 GB NVIDIA cards");
        assert!(
            main.iter().all(|c| c.key != suggested.key),
            "not offered twice"
        );
        let texture = main.iter().find(|c| c.name == "TEXTURE_STAGE_MB").unwrap();
        assert_eq!(texture.values, ["512", "1024"], "192 is the default");
        assert!(
            main.iter().all(|c| c.name != "WAVE_MORE_SKIPS"),
            "no test field"
        );
        assert!(branch.is_empty(), "a release build gets no branch switches");
        assert_eq!(
            main.last().unwrap().key,
            "none",
            "the PC check stays on offer"
        );

        let (suggested, main, branch) = choices(&ctx, Some(&card("amd", 16368)), false, &none, &[]);
        assert_eq!(suggested.key, "none", "nothing wanted: the PC check");
        assert!(main.iter().all(|c| c.kind != "profile"));
        assert_eq!(branch.len(), 1);
        assert_eq!(branch[0].name, "THIN");
    }

    #[test]
    fn a_profile_already_on_is_not_offered() {
        let ctx = context();
        let active = ["nvidia-8gb".to_string()];
        let eight = card("nvidia", 8192);
        let (suggested, main, _) = choices(&ctx, Some(&eight), true, &BTreeMap::new(), &active);
        assert_ne!(
            suggested.key, "profile:nvidia-8gb",
            "both runs would be the same"
        );
        assert!(main.iter().all(|c| c.kind != "profile"));
    }

    #[test]
    fn only_launcher_folders_are_deleted() {
        assert!(own_folder("20261127-180512-7f3a9c"));
        assert!(!own_folder(""));
        assert!(!own_folder(".."));
        assert!(!own_folder(r"..\..\Documents"));
        assert!(!own_folder("C:/Users"));
    }

    #[test]
    fn orders_cover_both_sides() {
        assert_eq!(order("baseline", ""), [false]);
        assert_eq!(order("crash", "keeps stopping"), [true]);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..64 {
            let o = order("speed", "");
            assert_eq!(o.len(), 2);
            assert_ne!(o[0], o[1]);
            seen.insert(o[0]);
        }
        assert_eq!(seen.len(), 2, "the order is picked at random");
    }

    #[test]
    fn other_switches_leave_out_defaults_and_paths() {
        let plan = Plan {
            gpu: None,
            profiles: vec!["base".into()],
            settings: [
                ("LEONIDA_ASYNC_SUBMIT", "1"),
                ("LEONIDA_TEXTURE_STAGE_MB", "512"),
                ("LEONIDA_GAME_DIR", r"D:\Games"),
                ("LEONIDA_SHOT", r"C:\shots"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            sources: [
                ("LEONIDA_ASYNC_SUBMIT", "profile:base"),
                ("LEONIDA_TEXTURE_STAGE_MB", "config"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        };
        let other = other_switches(&plan, &context().switches);
        assert_eq!(other, ["SHOT=<set>", "TEXTURE_STAGE_MB=512"]);
    }
}
