//! The text a tester shares: a few lines for people, then one fenced block with one
//! `key=value` per line for a script. Figures and words only: never a path, a log line, a
//! user name or anything else from the PC beyond its hardware and versions.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::figures::{self, Compare, Ending, Summary};

/// Discord cuts a message at 2,000 characters for accounts without Nitro (from memory, not
/// checked); this leaves room.
pub const LIMIT: usize = 1900;
pub const WHERE_MAX: usize = 300;
pub const ELSE_MAX: usize = 600;

/// The PC and the build, gathered once per test.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Facts {
    pub gpu: String,
    pub vram_mb: u64,
    pub adapters: usize,
    pub driver: String,
    pub cpu: String,
    pub ram_gb: u64,
    pub windows: String,
    /// "2560x1440@144", the desktop's mode on the main screen.
    pub screen: String,
    pub launcher: String,
    pub version: String,
    pub build: String,
    pub commit: String,
    pub release: bool,
}

/// What is tested.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Setting {
    /// "switch", "profile" or "none".
    pub kind: String,
    pub name: String,
    pub title: String,
    pub value: String,
    /// The value in words: "512", "Off (0)", "on".
    pub value_label: String,
}

/// One run, as the report needs it.
#[derive(Clone, Serialize, Deserialize)]
pub struct Run {
    /// Whether the run had the setting.
    pub with: bool,
    pub window: Option<Summary>,
    pub last60: Option<Summary>,
    pub whole: Option<Summary>,
    pub played_s: f64,
    pub run_s: f64,
    pub ending: Ending,
    pub exit: Option<i32>,
    pub device_removed: bool,
    /// "yes", "no" or "unknown".
    pub crash_report: String,
    pub battery: Option<bool>,
    /// How the player said it ended, when it differs from what the launcher saw.
    pub said: String,
}

impl Run {
    pub fn short(&self) -> bool {
        self.played_s < figures::SHORT_S
    }
}

/// The player's answers, already cleaned.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Answers {
    pub where_: String,
    pub felt: String,
    pub looked: String,
    pub black: String,
    pub other: String,
    /// For a crash test: what the player said before it ("keeps stopping", "sometimes").
    pub before: String,
}

pub struct Input<'a> {
    pub id: &'a str,
    pub pair: u32,
    pub kind: &'a str,
    pub setting: &'a Setting,
    /// The runs in the order they ran.
    pub runs: &'a [Run],
    pub facts: &'a Facts,
    pub profiles: &'a [String],
    pub other: &'a [String],
    /// The tested switch as the run as usual still had it, when Leonida's default isn't known
    /// so the launcher couldn't put it back: "NAME=value", or empty.
    pub kept: &'a str,
    pub warmup: &'a str,
    pub list: &'a str,
    pub answers: &'a Answers,
    /// Runs with the setting that stopped, over all tries: (stopped, tries).
    pub stops: (u32, u32),
    /// The result of the pair before, for a second pair.
    pub earlier: &'a str,
}

fn num(v: f64, digits: usize) -> String {
    format!("{v:.digits$}")
}

fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn or_none(text: &str) -> &str {
    if text.is_empty() { "none" } else { text }
}

/// "21.3 fps, median 47 ms, 99% under 120 ms, 12 over 50 ms"
fn figures_line(s: &Summary) -> String {
    if s.from_samples {
        return format!(
            "{} fps, median {} ms (from the once-a-few-seconds samples, no frame log)",
            num(s.fps, 1),
            num(f64::from(s.median_ms), 0)
        );
    }
    format!(
        "{} fps, median {} ms, 99% under {} ms, {} over 50 ms",
        num(s.fps, 1),
        num(f64::from(s.median_ms), 0),
        num(f64::from(s.p99_ms), 0),
        s.over50
    )
}

/// For a run without figures: frames that were too few to measure, or none at all.
fn unmeasured(run: &Run) -> String {
    if run.played_s > 0.0 {
        format!("too little play to measure ({} s)", num(run.played_s, 0))
    } else {
        "no frame times (the game closed before its first frames?)".into()
    }
}

fn run_line(run: &Run) -> String {
    let mut line = match &run.window {
        Some(s) => figures_line(s),
        None => unmeasured(run),
    };
    if run.short() && run.window.is_some() {
        let _ = write!(line, " (short: {} s of play)", num(run.played_s, 0));
    }
    let _ = write!(line, ". Ended: {}", run.ending.report());
    if run.ending.stopped() {
        let _ = write!(line, " after {}", clock(run.run_s));
    }
    if !run.said.is_empty() {
        let _ = write!(line, " (player: {})", run.said);
    }
    line.push('.');
    line
}

/// One run's figures for scripts. `compact` leaves out the extras that only compare with older
/// reports, for a report that would not fit otherwise.
fn run_block(run: &Run, compact: bool) -> String {
    let mut parts = Vec::new();
    let mut add = |key: &str, value: String| parts.push(format!("{key}:{value}"));
    if let Some(s) = &run.window {
        add("fps", num(s.fps, 1));
        add("med", num(f64::from(s.median_ms), 0));
        if !s.from_samples {
            add("p99", num(f64::from(s.p99_ms), 0));
            add("max", num(f64::from(s.longest_ms), 0));
            add("o50", s.over50.to_string());
            add("o100", s.over100.to_string());
            add("steady", num(s.steady_pct, 0));
        } else {
            add("src", "samples".into());
        }
        add("win", num(s.seconds, 0));
        add(
            "draws",
            s.draws.map_or("-".into(), |d| num(f64::from(d), 0)),
        );
    }
    if let Some(s) = run.last60.as_ref().filter(|_| !compact) {
        add("fps60", num(s.fps, 1));
        add("med60", num(f64::from(s.median_ms), 0));
    }
    if let Some(d) = run
        .whole
        .as_ref()
        .and_then(|s| s.draws)
        .filter(|_| !compact)
    {
        add("draws_all", num(f64::from(d), 0));
    }
    add("played", num(run.played_s, 0));
    add("run", num(run.run_s, 0));
    add("end", run.ending.key().into());
    add("exit", figures::hex(run.exit));
    add("removed", yes(run.device_removed).into());
    add("crash", run.crash_report.clone());
    add("battery", run.battery.map_or("unknown", |b| yes(b)).into());
    add("short", yes(run.short()).into());
    parts.join(" ")
}

fn yes(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

/// The setting line: "Textures sent to the card each frame (TEXTURE_STAGE_MB), Default against 512".
fn setting_line(input: &Input) -> String {
    let s = input.setting;
    match s.kind.as_str() {
        "switch" if input.kind == "crash" => {
            format!("{} ({}), {}", s.title, s.name, s.value_label)
        }
        "switch" => format!(
            "{} ({}), Default against {}",
            s.title, s.name, s.value_label
        ),
        "profile" => format!("profile {}, off against on", s.name),
        _ => "none (baseline)".into(),
    }
}

/// "16 s and 120 s": the seconds each run's figures cover, in the order they ran.
fn seconds_list(values: impl Iterator<Item = f64>) -> String {
    values
        .map(|s| format!("{} s", num(s, 0)))
        .collect::<Vec<_>>()
        .join(" and ")
}

/// The label of a run: what it had, from the value, so a switch turned off doesn't read
/// "With it".
fn run_label(input: &Input, run: &Run) -> String {
    let s = input.setting;
    match (input.kind, run.with, s.kind.as_str()) {
        ("baseline", _, _) => "The run".into(),
        (_, false, _) => "As usual".into(),
        (_, true, "switch") => match s.value.as_str() {
            "0" => format!("With {} off", s.name),
            "1" => format!("With {} on", s.name),
            value => format!("With {}={value}", s.name),
        },
        (_, true, "profile") => format!("With profile {}", s.name),
        _ => "With it".into(),
    }
}

fn result_line(input: &Input) -> String {
    let with = input.runs.iter().find(|r| r.with);
    let without = input.runs.iter().find(|r| !r.with);
    let mut noisy = input.kind == "speed";
    let mut text = match (input.kind, without, with) {
        ("crash", _, Some(w)) if w.ending.stopped() => format!(
            "stopped after {} anyway: {} ({})",
            clock(w.run_s),
            w.ending.report(),
            figures::hex(w.exit)
        ),
        ("crash", _, Some(w)) => format!("ran {} without stopping", clock(w.run_s)),
        ("baseline", Some(run), _) => match &run.window {
            Some(s) => format!(
                "{} fps, median frame {} ms over the last {} of play",
                num(s.fps, 1),
                num(f64::from(s.median_ms), 0),
                clock(s.seconds)
            ),
            None => unmeasured(run),
        },
        (_, Some(_), Some(b)) if b.ending.stopped() => format!(
            "stopped with the setting: {} ({})",
            b.ending.report(),
            figures::hex(b.exit)
        ),
        (_, Some(a), Some(_)) if a.ending.stopped() => {
            let words = a.ending.report();
            format!("stopped without the setting: {words}, no comparison")
        }
        (_, Some(_), Some(_)) if input.runs.iter().any(Run::short) => {
            noisy = false;
            format!(
                "runs too short to compare ({} of play, about 3 minutes each needed)",
                seconds_list(input.runs.iter().map(|r| r.played_s))
            )
        }
        (_, Some(a), Some(b)) => match (&a.window, &b.window) {
            (Some(sa), Some(sb)) => {
                let (ma, mb) = (sa.median_ms, sb.median_ms);
                let pair = format!(
                    "median frame {} ms with it against {} ms as usual",
                    num(f64::from(mb), 0),
                    num(f64::from(ma), 0)
                );
                let mut line = match figures::compare(ma, mb) {
                    Compare::Same => format!("about the same, {pair}"),
                    Compare::Faster(p) => {
                        format!("faster with it, {pair} ({}% shorter)", num(p, 0))
                    }
                    Compare::Slower(p) => format!("slower with it, {pair} ({}% longer)", num(p, 0)),
                };
                if let (Some(da), Some(db)) = (sa.draws, sb.draws)
                    && (da - db).abs() > da.max(db) * 0.2
                {
                    line.push_str("; the two runs may show different places");
                }
                if sa.steady_pct > 90.0 && sb.steady_pct > 90.0 && !sa.from_samples {
                    line.push_str(
                        "; most frames are the same length, so the frame rate may be capped",
                    );
                }
                line
            }
            _ => "no frame times in one of the runs, no comparison".into(),
        },
        _ => "not measured".into(),
    };
    if noisy {
        text.push_str(". One pair of runs, can vary");
    }
    text
}

/// The report, with the player's own words as they are.
pub fn build(input: &Input) -> String {
    render(input, false)
}

fn render(input: &Input, compact: bool) -> String {
    let f = input.facts;
    let a = input.answers;
    let title = match input.kind {
        "crash" => "crash test",
        "baseline" => "PC check",
        _ => "setting test",
    };
    let mut out = format!("**Leonida Legacy {title}**");
    if input.pair > 1 {
        let _ = write!(out, " (pair {})", input.pair);
    }
    if !f.release {
        out.push_str(" (not a release build)");
    }
    out.push('\n');
    let _ = writeln!(out, "Setting: {}", setting_line(input));
    let _ = writeln!(out, "Result: {}.", result_line(input));
    if !input.earlier.is_empty() {
        let _ = writeln!(out, "Pair {} said: {}", input.pair - 1, input.earlier);
    }
    let _ = writeln!(out, "Card: {}, {} MB", or_none(&f.gpu), f.vram_mb);
    for run in input.runs {
        let _ = writeln!(out, "{}: {}", run_label(input, run), run_line(run));
    }
    if input.runs.len() > 1 {
        let first = if input.runs[0].with {
            "with"
        } else {
            "without"
        };
        let _ = writeln!(
            out,
            "Order: the launcher picked it, {first} the setting first."
        );
    }
    if !input.kept.is_empty() {
        let _ = writeln!(
            out,
            "Note: the run as usual still had {} from the player's own settings.",
            input.kept
        );
    }
    if input.kind == "crash" && !a.before.is_empty() {
        let _ = writeln!(out, "Before the test: {}.", a.before);
    }
    if input.kind == "speed" && input.runs.len() > 1 {
        let _ = writeln!(
            out,
            "With the setting it felt: {}. Looked different: {}. Black or missing parts: {}.",
            or_none(&a.felt),
            or_none(&a.looked),
            or_none(&a.black)
        );
    } else if !a.black.is_empty() {
        let _ = writeln!(out, "Black or missing parts: {}.", a.black);
    }
    if !a.where_.is_empty() {
        let _ = writeln!(out, "Where: {}", a.where_);
    }
    let _ = writeln!(out, "Anything else: {}", or_none(&a.other));
    let measured: Vec<f64> = input
        .runs
        .iter()
        .filter_map(|r| r.window.as_ref().map(|s| s.seconds))
        .collect();
    if measured.is_empty() && input.runs.iter().any(|r| r.played_s > 0.0) {
        out.push_str("Figures: none, too little play to measure.\n");
    } else if measured.is_empty() {
        out.push_str("Figures: none, the game logged no frame times.\n");
    } else {
        let _ = writeln!(
            out,
            "Figures: from {} of play (the last {} min of a longer run), frame log on.",
            seconds_list(measured.into_iter()),
            num(figures::WINDOW_S / 60.0, 0)
        );
    }

    out.push_str("```leonida-test v1\n");
    let mut kv = |key: &str, value: &str| {
        let _ = writeln!(out, "{key}={value}");
    };
    kv("test", input.id);
    kv("pair", &input.pair.to_string());
    kv("kind", input.kind);
    let setting = match input.setting.kind.as_str() {
        "switch" | "profile" => format!("{}:{}", input.setting.kind, input.setting.name),
        _ => "none".into(),
    };
    kv("setting", &setting);
    if input.setting.kind != "none" {
        kv("value", &input.setting.value);
    }
    let order: String = input
        .runs
        .iter()
        .map(|r| if r.with { 'b' } else { 'a' })
        .collect();
    kv("order", &order);
    kv("release", yes(f.release));
    kv("version", or_none(&f.version));
    kv("build", or_none(&f.build));
    kv("commit", or_none(&f.commit));
    kv("launcher", &f.launcher);
    kv("list", input.list);
    kv("gpu", or_none(&f.gpu));
    kv("vram_mb", &f.vram_mb.to_string());
    kv("adapters", &f.adapters.to_string());
    kv("driver", or_none(&f.driver));
    kv("cpu", or_none(&f.cpu));
    kv("ram_gb", &f.ram_gb.to_string());
    kv("windows", or_none(&f.windows));
    kv("screen", or_none(&f.screen));
    kv("profiles", &input.profiles.join(","));
    let other = input.other.join(",");
    kv("other", or_none(&other));
    if !input.kept.is_empty() {
        kv("kept", input.kept);
    }
    kv("warmup", input.warmup);
    kv("window_s", &num(figures::WINDOW_S, 0));
    kv("stops", &format!("{}/{}", input.stops.0, input.stops.1));
    if input.kind == "speed" {
        kv("felt", or_none(&a.felt));
        kv("looked", or_none(&a.looked));
    }
    kv("black", or_none(&a.black));
    for run in input.runs {
        kv(if run.with { "b" } else { "a" }, &run_block(run, compact));
    }
    out.push_str("```");
    out
}

pub struct Fitted {
    pub text: String,
    /// Still longer than `LIMIT`: Discord sends it as a file instead of a message.
    pub over: bool,
}

/// The report cut until it fits `LIMIT`: the player's words first ("Anything else", then
/// "Where"), then the figures that only compare with older reports.
pub fn fit(input: &Input) -> Fitted {
    let mut answers = input.answers.clone();
    for step in 0..2 {
        let text = build(&Input {
            answers: &answers,
            ..*input
        });
        let over = text.chars().count().saturating_sub(LIMIT);
        if over == 0 {
            return Fitted { text, over: false };
        }
        let field = if step == 0 {
            &mut answers.other
        } else {
            &mut answers.where_
        };
        let keep = field.chars().count().saturating_sub(over + 3);
        *field = cut(field, keep);
    }
    let input = Input {
        answers: &answers,
        ..*input
    };
    let text = build(&input);
    if text.chars().count() <= LIMIT {
        return Fitted { text, over: false };
    }
    let text = render(&input, true);
    let over = text.chars().count() > LIMIT;
    Fitted { text, over }
}

/// What the player says about the second run against the first, turned around when the first
/// run was the one with the setting: the report says how the setting felt. The order is hidden
/// from the player, so they can't answer about the setting directly.
pub fn felt_with(second_against_first: &str, second_has_it: bool) -> String {
    match (second_against_first, second_has_it) {
        ("smoother", false) => "worse".into(),
        ("worse", false) => "smoother".into(),
        (answer, _) => answer.into(),
    }
}

fn cut(text: &str, keep: usize) -> String {
    if text.chars().count() <= keep {
        return text.to_string();
    }
    if keep == 0 {
        return String::new();
    }
    let mut out: String = text.chars().take(keep).collect();
    out.push_str("...");
    out
}

/// The player's text on one line, cut to `max`, without what could break the report or ping
/// people. Text that looks like a file path, a log line, an email address or one of `names`
/// (the user name, the computer's name) is left out whole: `None` then.
pub fn clean(text: &str, names: &[String], max: usize) -> Option<String> {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    // Checked before `@` goes, or me@mail.com would pass as memail.com.
    if private(&line, names) {
        return None;
    }
    let line: String = line.chars().filter(|c| !matches!(c, '`' | '@')).collect();
    Some(cut(&line, max))
}

fn email(text: &str) -> bool {
    text.split_whitespace().any(|word| {
        word.split_once('@').is_some_and(|(name, host)| {
            !name.is_empty() && host.trim_end_matches('.').contains('.')
        })
    })
}

/// `%NAME%`, an environment variable in a path.
fn variable(text: &str) -> bool {
    let parts: Vec<&str> = text.split('%').collect();
    parts.len() >= 3
        && parts[1..parts.len() - 1].iter().any(|name| {
            !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        })
}

fn private(text: &str, names: &[String]) -> bool {
    let lower = text.to_lowercase();
    let b = lower.as_bytes();
    let drive = b
        .windows(3)
        .any(|w| w[0].is_ascii_alphabetic() && w[1] == b':' && (w[2] == b'\\' || w[2] == b'/'));
    let share = b
        .windows(3)
        .any(|w| w[0] == b'\\' && w[1] == b'\\' && w[2].is_ascii_alphanumeric());
    let users = lower.contains("\\users\\") || lower.contains("/users/");
    let places = ["appdata", "~\\", "~/", ".log", "+0x"]
        .iter()
        .any(|p| lower.contains(p));
    let named = names.iter().any(|name| {
        let name = name.trim().to_lowercase();
        name.chars().count() >= 3 && has_word(&lower, &name)
    });
    drive || share || users || places || named || email(text) || variable(text) || log_line(text)
}

fn has_word(text: &str, word: &str) -> bool {
    let edge = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
    text.match_indices(word).any(|(at, _)| {
        edge(text[..at].chars().next_back()) && edge(text[at + word.len()..].chars().next())
    })
}

/// A time of day and a log level, or words only Leonida's log has.
fn log_line(text: &str) -> bool {
    let b = text.as_bytes();
    let time = b.windows(8).any(|w| {
        w[0].is_ascii_digit()
            && w[1].is_ascii_digit()
            && w[2] == b':'
            && w[3].is_ascii_digit()
            && w[4].is_ascii_digit()
            && w[5] == b':'
            && w[6].is_ascii_digit()
            && w[7].is_ascii_digit()
    });
    let level = ["TRACE", "DEBUG", "INFO", "WARN", "ERROR"]
        .iter()
        .any(|level| has_word(text, level));
    let words = [
        "frame times:",
        "draws/frame",
        "ms/frame",
        "leonida_",
        "title said:",
    ]
    .iter()
    .any(|w| text.contains(w));
    (time && level) || words
}

/// A switch as a report may show it, like leonida-settings.ps1's Format-LeonidaGraphicsSwitch
/// -Short: an unknown name or a value that isn't simple (a path, say) is hidden.
pub fn safe_switch(name: &str, value: &str, known: &[String]) -> String {
    let Some(short) = name.strip_prefix("LEONIDA_") else {
        return "<unknown switch>=<set>".into();
    };
    if short.is_empty()
        || !short
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
    {
        return "<unknown switch>=<set>".into();
    }
    let digits = |s: &str, most: usize| {
        !s.is_empty() && s.len() <= most && s.bytes().all(|c| c.is_ascii_hexdigit())
    };
    let simple =
        (value.len() <= 10 && !value.is_empty() && value.bytes().all(|c| c.is_ascii_digit()))
            || value
                .strip_prefix("0x")
                .is_some_and(|h| digits(h, 8) && h.bytes().all(|c| !c.is_ascii_uppercase()))
            || matches!(value, "alt" | "on" | "off")
            || known.iter().any(|k| k == value);
    let value = if simple {
        value
    } else if value.is_empty() {
        "<empty>"
    } else {
        "<set>"
    };
    if value == "1" {
        short.to_string()
    } else {
        format!("{short}={value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(median: f32, fps: f64) -> Summary {
        Summary {
            seconds: 120.0,
            count: (fps * 120.0) as usize,
            fps,
            median_ms: median,
            p99_ms: median * 2.5,
            longest_ms: median * 4.0,
            over50: 12,
            over100: 3,
            steady_pct: 40.0,
            draws: Some(4200.0),
            from_samples: false,
        }
    }

    fn run(with: bool, median: f32, ending: Ending) -> Run {
        Run {
            with,
            window: Some(summary(median, 1000.0 / f64::from(median))),
            last60: Some(summary(median, 1000.0 / f64::from(median))),
            whole: Some(summary(median + 10.0, 15.0)),
            played_s: 190.0,
            run_s: 400.0,
            ending,
            exit: Some(if ending == Ending::Closed {
                0
            } else {
                -1_073_741_819
            }),
            device_removed: false,
            crash_report: "no".into(),
            battery: Some(false),
            said: String::new(),
        }
    }

    fn facts() -> Facts {
        Facts {
            gpu: "NVIDIA GeForce RTX 3060 Ti".into(),
            vram_mb: 8192,
            adapters: 1,
            driver: "621.40 (32.0.16.2140)".into(),
            cpu: "AMD Ryzen 5 5600 6-Core Processor".into(),
            ram_gb: 16,
            windows: "Windows 11 Pro 25H2 26200.1234".into(),
            screen: "2560x1440@144".into(),
            launcher: "0.1.0".into(),
            version: "0.1.0".into(),
            build: "42".into(),
            commit: "4b28ba1".into(),
            release: true,
        }
    }

    fn texture() -> Setting {
        Setting {
            kind: "switch".into(),
            name: "TEXTURE_STAGE_MB".into(),
            title: "Textures sent to the card each frame".into(),
            value: "512".into(),
            value_label: "512".into(),
        }
    }

    fn input<'a>(
        kind: &'a str,
        setting: &'a Setting,
        runs: &'a [Run],
        facts: &'a Facts,
        answers: &'a Answers,
    ) -> Input<'a> {
        Input {
            id: "7f3a9c",
            pair: 1,
            kind,
            setting,
            runs,
            facts,
            profiles: &[],
            other: &[],
            kept: "",
            warmup: "done before testing",
            list: "built-in",
            answers,
            stops: (0, 1),
            earlier: "",
        }
    }

    #[test]
    fn a_speed_report_reads_well_and_parses() {
        let (setting, facts) = (texture(), facts());
        let runs = [
            run(true, 41.0, Ending::Closed),
            run(false, 47.0, Ending::Closed),
        ];
        let answers = Answers {
            felt: "smoother".into(),
            looked: "no".into(),
            black: "no".into(),
            where_: "standing at the start".into(),
            ..Answers::default()
        };
        let fitted = fit(&input("speed", &setting, &runs, &facts, &answers));
        assert!(!fitted.over);
        let text = fitted.text;
        assert!(
            text.starts_with("**Leonida Legacy setting test**\n"),
            "{text}"
        );
        assert!(text.contains(
            "Setting: Textures sent to the card each frame (TEXTURE_STAGE_MB), Default against 512"
        ));
        assert!(
            text.contains("Result: faster with it, median frame 41 ms with it against 47 ms as usual (13% shorter). One pair of runs, can vary."),
            "{text}"
        );
        assert!(
            text.contains("\nWith TEXTURE_STAGE_MB=512: 24.4 fps"),
            "{text}"
        );
        assert!(
            text.contains("Ended: the game closed normally.\nAs usual: 21.3 fps"),
            "{text}"
        );
        assert!(text.contains("Order: the launcher picked it, with the setting first."));
        assert!(
            text.contains("With the setting it felt: smoother."),
            "{text}"
        );
        assert!(
            text.contains("Figures: from 120 s and 120 s of play (the last 2 min of a longer run)"),
            "{text}"
        );
        assert!(
            text.chars().count() <= LIMIT,
            "{} characters",
            text.chars().count()
        );
        assert!(!text.contains('\u{2014}'));
        let block = text.split("```leonida-test v1\n").nth(1).unwrap();
        let block = block.strip_suffix("```").unwrap();
        let keys: Vec<&str> = block
            .lines()
            .map(|l| l.split_once('=').unwrap().0)
            .collect();
        assert_eq!(&keys[..4], ["test", "pair", "kind", "setting"]);
        assert!(block.contains("\norder=ba\n"));
        assert!(block.contains("\nwindow_s=120\n"));
        assert!(block.contains("\nb=fps:24.4 med:41 p99:102 max:164 o50:12 o100:3 steady:40 "));
    }

    #[test]
    fn long_words_are_cut_to_fit() {
        let (setting, facts) = (texture(), facts());
        let runs = [
            run(false, 47.0, Ending::Closed),
            run(true, 41.0, Ending::Closed),
        ];
        let answers = Answers {
            where_: "w".repeat(WHERE_MAX),
            other: "x".repeat(ELSE_MAX),
            ..Answers::default()
        };
        let text = fit(&input("speed", &setting, &runs, &facts, &answers)).text;
        assert!(
            text.chars().count() <= LIMIT,
            "{} characters",
            text.chars().count()
        );
        assert!(
            text.contains(
                "
Where: www"
            ),
            "where is cut after anything else: {text}"
        );
        assert!(text.ends_with("```"));
    }

    #[test]
    fn a_report_too_long_for_discord_says_so() {
        let (setting, mut facts) = (texture(), facts());
        facts.cpu = "c".repeat(900);
        let runs = [
            run(false, 47.0, Ending::Closed),
            run(true, 41.0, Ending::Closed),
        ];
        let answers = Answers::default();
        let fitted = fit(&input("speed", &setting, &runs, &facts, &answers));
        assert!(fitted.over, "{} characters", fitted.text.chars().count());
        assert!(!fitted.text.contains("fps60"), "the extras go first");
        facts.cpu = String::new();
        let compact = render(&input("speed", &setting, &runs, &facts, &answers), true);
        facts.cpu = "c".repeat(LIMIT - compact.chars().count());
        let fitted = fit(&input("speed", &setting, &runs, &facts, &answers));
        assert!(
            !fitted.over && !fitted.text.contains("fps60"),
            "fits without the extras"
        );
    }

    #[test]
    fn short_runs_give_no_result_and_say_how_long_they_were() {
        let (setting, facts) = (texture(), facts());
        let mut a = run(false, 47.0, Ending::Closed);
        let mut b = run(true, 40.0, Ending::Closed);
        for (r, played) in [(&mut a, 21.0), (&mut b, 22.4)] {
            r.played_s = played;
            r.window.as_mut().unwrap().seconds = 16.0;
        }
        let text = build(&input(
            "speed",
            &setting,
            &[a, b],
            &facts,
            &Answers::default(),
        ));
        assert!(
            text.contains(
                "Result: runs too short to compare (21 s and 22 s of play, about 3 minutes each \
                 needed).\n"
            ),
            "{text}"
        );
        assert!(
            text.contains("Figures: from 16 s and 16 s of play"),
            "{text}"
        );
    }

    #[test]
    fn a_few_frames_are_not_called_none() {
        let (setting, facts) = (texture(), facts());
        let mut a = run(false, 8.0, Ending::Closed);
        let mut b = run(true, 8.0, Ending::Closed);
        for r in [&mut a, &mut b] {
            r.played_s = 4.2;
            r.window = None;
        }
        let text = build(&input(
            "speed",
            &setting,
            &[a, b],
            &facts,
            &Answers::default(),
        ));
        assert!(
            text.contains("too little play to measure (4 s). Ended"),
            "{text}"
        );
        assert!(
            text.contains("Figures: none, too little play to measure.\n"),
            "{text}"
        );
        assert!(!text.contains("no frame times"), "{text}");
    }

    #[test]
    fn the_felt_answer_is_about_the_setting() {
        assert_eq!(felt_with("smoother", true), "smoother");
        assert_eq!(felt_with("smoother", false), "worse");
        assert_eq!(felt_with("worse", false), "smoother");
        assert_eq!(felt_with("about the same", false), "about the same");
        assert_eq!(felt_with("", false), "");
    }

    #[test]
    fn crash_and_baseline_reports() {
        let facts = facts();
        let off = Setting {
            kind: "switch".into(),
            name: "ASYNC_SUBMIT".into(),
            title: "Send frames from a helper thread".into(),
            value: "0".into(),
            value_label: "turned off".into(),
        };
        let mut stopped = run(true, 40.0, Ending::Crashed);
        stopped.run_s = 221.0;
        let answers = Answers {
            before: "the game kept stopping".into(),
            ..Answers::default()
        };
        let text = build(&input("crash", &off, &[stopped], &facts, &answers));
        assert!(text.starts_with("**Leonida Legacy crash test**"));
        assert!(
            text.contains("Setting: Send frames from a helper thread (ASYNC_SUBMIT), turned off")
        );
        assert!(
            text.contains("Result: stopped after 3:41 anyway: the game crashed (0xC0000005)."),
            "{text}"
        );
        assert!(text.contains("\nWith ASYNC_SUBMIT off: "), "{text}");
        let mut reset = run(true, 40.0, Ending::GpuReset);
        reset.run_s = 10.0;
        let text = build(&input("crash", &off, &[reset], &facts, &answers));
        assert!(
            text.contains("Ended: the graphics driver was reset after 0:10.")
                && text.contains("anyway: the graphics driver was reset (0x")
                && !text.contains("card is fine"),
            "{text}"
        );
        let mut fine = run(true, 40.0, Ending::Closed);
        fine.run_s = 600.0;
        let text = build(&input("crash", &off, &[fine], &facts, &answers));
        assert!(
            text.contains("Result: ran 10:00 without stopping."),
            "{text}"
        );

        let none = Setting {
            kind: "none".into(),
            ..Setting::default()
        };
        let mut dev = facts.clone();
        dev.release = false;
        let text = build(&input(
            "baseline",
            &none,
            &[run(false, 47.0, Ending::Closed)],
            &dev,
            &Answers::default(),
        ));
        assert!(text.starts_with("**Leonida Legacy PC check** (not a release build)"));
        assert!(text.contains("Setting: none (baseline)"));
        assert!(text.contains("\nsetting=none\n") && text.contains("\nrelease=no\n"));
    }

    #[test]
    fn a_stopped_run_and_different_places_are_said() {
        let (setting, facts) = (texture(), facts());
        let mut b = run(true, 41.0, Ending::Closed);
        b.window.as_mut().unwrap().draws = Some(1000.0);
        let runs = [run(false, 47.0, Ending::Closed), b];
        let text = build(&input(
            "speed",
            &setting,
            &runs,
            &facts,
            &Answers::default(),
        ));
        assert!(text.contains("may show different places"), "{text}");
        let runs = [
            run(false, 47.0, Ending::Closed),
            run(true, 41.0, Ending::Crashed),
        ];
        let text = build(&input(
            "speed",
            &setting,
            &runs,
            &facts,
            &Answers::default(),
        ));
        assert!(
            text.contains("Result: stopped with the setting: the game crashed"),
            "{text}"
        );
    }

    #[test]
    fn private_text_is_left_out() {
        let user = &["Robin".to_string(), "DESKTOP-AB12".to_string()][..];
        for text in [
            r"it crashed, see C:\Games\leonida\crashes.log",
            r"in d:/stuff",
            r"\\nas\share\game",
            "my name is robin and it froze",
            "2026-11-27T18:00:01Z  INFO leonida_gpu: something",
            "12:01:33 ERROR it broke",
            "frame times: 5 frames in 1.0 s",
            "write me at me@mail.com",
            r"look in %LOCALAPPDATA%\leonida",
            r"AppData\Local\leonida\cache",
            r"~\leonida\target",
            "see crashes.log",
            "it stopped at module+0x1a2b",
            "on desktop-ab12 it froze",
        ] {
            assert_eq!(clean(text, user, 600), None, "{text}");
        }
        assert_eq!(
            clean("  the first\nstreet,  standing still ", user, 600).as_deref(),
            Some("the first street, standing still")
        );
        assert_eq!(
            clean("@everyone `look`", user, 600).as_deref(),
            Some("everyone look")
        );
        assert_eq!(
            clean("robinsville is a place", user, 600).as_deref(),
            Some("robinsville is a place")
        );
        assert_eq!(clean("abcdef", user, 3).as_deref(), Some("abc..."));
        assert_eq!(
            clean("At 3:30 it was fine", user, 600).as_deref(),
            Some("At 3:30 it was fine")
        );
        assert_eq!(
            clean("50% to 60% of the time, 100%", user, 600).as_deref(),
            Some("50% to 60% of the time, 100%")
        );
    }

    #[test]
    fn switches_show_only_simple_values() {
        let known = vec!["quality".to_string()];
        assert_eq!(safe_switch("LEONIDA_QUEUE2", "1", &known), "QUEUE2");
        assert_eq!(
            safe_switch("LEONIDA_TEXTURE_STAGE_MB", "512", &known),
            "TEXTURE_STAGE_MB=512"
        );
        assert_eq!(
            safe_switch("LEONIDA_DLSS", "quality", &known),
            "DLSS=quality"
        );
        assert_eq!(
            safe_switch("LEONIDA_SHOT", r"C:\shots", &known),
            "SHOT=<set>"
        );
        assert_eq!(safe_switch("LEONIDA_X", "", &known), "X=<empty>");
        assert_eq!(safe_switch("LEONIDA_X", "0x1f", &known), "X=0x1f");
        assert_eq!(safe_switch("HOME", "x", &known), "<unknown switch>=<set>");
        assert_eq!(
            safe_switch("LEONIDA_bad", "1", &known),
            "<unknown switch>=<set>"
        );
    }
}
