//! A test run's log reduced to figures: the frame log's "frame times" readings, the
//! once-a-few-seconds frame-rate samples and how the run ended. Lines are matched and dropped;
//! only numbers are kept, never the text (logs can quote game data).

use serde::{Deserialize, Serialize};

/// The measured part of a run: its last two minutes of play.
pub const WINDOW_S: f64 = 120.0;
/// The window of the older test-report.ps1 reports, given too so both can be compared.
pub const WINDOW_SHORT_S: f64 = 60.0;
/// Readings this close to the exit are left out: closing the game makes long frames.
pub const TAIL_S: f64 = 5.0;
/// Under this many seconds of play a run is too short to compare.
pub const SHORT_S: f64 = 100.0;
/// Two medians closer than this share of the slower one count as the same. A guess, to be
/// tuned once reports come in.
pub const SAME_SHARE: f64 = 0.10;

/// One "frame times" line: when it arrived (seconds since the spawn), the seconds it covers
/// and every frame's length in ms.
pub struct Reading {
    pub at: f64,
    pub seconds: f64,
    pub frames: Vec<f32>,
}

/// One "fps, ms/frame" line.
pub struct Sample {
    pub at: f64,
    pub fps: f32,
    pub draws: Option<f32>,
}

/// What one run's log showed.
#[derive(Default)]
pub struct Seen {
    pub readings: Vec<Reading>,
    pub samples: Vec<Sample>,
    pub gpu_failure: bool,
    pub device_removed: bool,
}

/// A line worth looking at, parsed outside any lock.
pub enum Line {
    Frames(f64, Vec<f32>),
    Sample(f32, Option<f32>),
    GpuFailure { device_removed: bool },
}

/// Removes the colour codes tracing writes (`ESC [ ... m`).
pub fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Whether a raw line can hold a figure, checked before anything else is done with it.
pub fn worth(line: &str) -> bool {
    line.contains("frame times: ")
        || line.contains(" fps, ")
        || line.contains("host GPU failure")
        || line.contains("device removed:")
}

fn number_before(text: &str, end: usize) -> Option<f32> {
    text[..end].rsplit(' ').next()?.parse().ok()
}

pub fn parse(line: &str) -> Option<Line> {
    let line = strip_ansi(line);
    if line.contains("host GPU failure") || line.contains("device removed:") {
        return Some(Line::GpuFailure {
            device_removed: line.contains("device removed:"),
        });
    }
    if let Some(at) = line.find("frame times: ") {
        // "frame times: 5 frames in 1.0 s: shortest ...; each: 140 150 160"
        let rest = &line[at + "frame times: ".len()..];
        let mut words = rest.split_whitespace();
        let count: usize = words.next()?.parse().ok()?;
        if (words.next(), words.next()) != (Some("frames"), Some("in")) {
            return None;
        }
        let seconds: f64 = words.next()?.parse().ok()?;
        let (_, each) = rest.rsplit_once("; each: ")?;
        let frames: Vec<f32> = each
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        return (frames.len() == count && seconds > 0.0).then_some(Line::Frames(seconds, frames));
    }
    // "21.3 fps, 47.0 ms/frame (...), 4200 draws/frame, ..."
    let at = line.find(" fps, ")?;
    let after = &line[at + " fps, ".len()..];
    if !after.split(' ').nth(1).is_some_and(|w| w == "ms/frame") {
        return None;
    }
    let fps = number_before(&line, at)?;
    let draws = line
        .find(" draws/frame")
        .and_then(|end| number_before(&line, end));
    Some(Line::Sample(fps, draws))
}

/// The figures of a window of frames.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Summary {
    pub seconds: f64,
    pub count: usize,
    pub fps: f64,
    pub median_ms: f32,
    pub p99_ms: f32,
    pub longest_ms: f32,
    pub over50: usize,
    pub over100: usize,
    /// Share of frames within 1 ms of the median, in percent. Near 100 means a frame cap.
    pub steady_pct: f64,
    /// The median draws per frame in the window, from the samples.
    pub draws: Option<f32>,
    /// Made from the fps samples because the frame log was missing.
    pub from_samples: bool,
}

fn median(values: &mut [f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    Some(values[(values.len() - 1) / 2])
}

fn summarize(seconds: f64, frames: &mut [f32]) -> Option<Summary> {
    if frames.is_empty() || seconds <= 0.0 {
        return None;
    }
    frames.sort_by(f32::total_cmp);
    let n = frames.len();
    let median_ms = frames[(n - 1) / 2];
    let p99 = frames[((n as f64 * 0.99).ceil() as usize).clamp(1, n) - 1];
    let steady = frames
        .iter()
        .filter(|f| (**f - median_ms).abs() <= 1.0)
        .count();
    Some(Summary {
        seconds,
        count: n,
        fps: n as f64 / seconds,
        median_ms,
        p99_ms: p99,
        longest_ms: frames[n - 1],
        over50: frames.iter().filter(|f| **f > 50.0).count(),
        over100: frames.iter().filter(|f| **f > 100.0).count(),
        steady_pct: steady as f64 * 100.0 / n as f64,
        draws: None,
        from_samples: false,
    })
}

impl Seen {
    pub fn take(&mut self, at: f64, line: Line) {
        match line {
            Line::Frames(seconds, frames) => self.readings.push(Reading {
                at,
                seconds,
                frames,
            }),
            Line::Sample(fps, draws) => self.samples.push(Sample { at, fps, draws }),
            Line::GpuFailure { device_removed } => {
                self.gpu_failure = true;
                self.device_removed |= device_removed;
            }
        }
    }

    /// When play began: the first sample whose draws per frame rise above half the run's
    /// highest (loading screens and menus draw little), otherwise the first frames. A guess
    /// that real runs have to confirm.
    pub fn play_start(&self) -> Option<f64> {
        let most = self
            .samples
            .iter()
            .filter_map(|s| s.draws)
            .fold(0.0f32, f32::max);
        if most > 0.0
            && let Some(first) = self
                .samples
                .iter()
                .find(|s| s.draws.is_some_and(|d| d > most / 2.0))
        {
            return Some(first.at);
        }
        if let Some(first) = self.readings.first() {
            return Some((first.at - first.seconds).max(0.0));
        }
        self.samples.first().map(|s| s.at)
    }

    /// Seconds of play the readings cover, loading left out.
    pub fn played(&self) -> f64 {
        let Some(start) = self.play_start() else {
            return 0.0;
        };
        let covered: f64 = self
            .readings
            .iter()
            .filter(|r| r.at - r.seconds >= start - 0.5)
            .map(|r| r.seconds)
            .sum();
        if covered > 0.0 {
            return covered;
        }
        // Without the frame log, the samples' span.
        self.samples.last().map_or(0.0, |last| last.at - start)
    }

    /// The last `window` seconds of play before `end` (seconds since the spawn), the last
    /// few seconds before the exit left out.
    pub fn summary(&self, end: f64, window: f64) -> Option<Summary> {
        let start = self.play_start()?;
        let cut = end - TAIL_S;
        let mut seconds = 0.0;
        let mut frames = Vec::new();
        let mut from = cut;
        for r in self.readings.iter().rev() {
            if r.at > cut {
                continue;
            }
            if r.at - r.seconds < start - 0.5 {
                break;
            }
            seconds += r.seconds;
            frames.extend_from_slice(&r.frames);
            from = r.at - r.seconds;
            if seconds >= window {
                break;
            }
        }
        let mut summary = match summarize(seconds, &mut frames) {
            Some(summary) => summary,
            None => self.sampled(start.max(cut - window), cut)?,
        };
        let mut draws: Vec<f32> = self
            .samples
            .iter()
            .filter(|s| s.at >= from && s.at <= cut)
            .filter_map(|s| s.draws)
            .collect();
        summary.draws = median(&mut draws);
        Some(summary)
    }

    /// Every reading, loading and menus included.
    pub fn whole(&self) -> Option<Summary> {
        let seconds = self.readings.iter().map(|r| r.seconds).sum();
        let mut frames: Vec<f32> = self
            .readings
            .iter()
            .flat_map(|r| r.frames.iter().copied())
            .collect();
        let mut summary = summarize(seconds, &mut frames)?;
        let mut draws: Vec<f32> = self.samples.iter().filter_map(|s| s.draws).collect();
        summary.draws = median(&mut draws);
        Some(summary)
    }

    /// A rougher summary from the fps samples, for a build whose frame log is missing.
    fn sampled(&self, from: f64, to: f64) -> Option<Summary> {
        let mut fps: Vec<f32> = self
            .samples
            .iter()
            .filter(|s| s.at >= from && s.at <= to && s.fps > 0.0)
            .map(|s| s.fps)
            .collect();
        let fps = median(&mut fps)?;
        Some(Summary {
            seconds: to - from,
            fps: f64::from(fps),
            median_ms: 1000.0 / fps,
            from_samples: true,
            ..Summary::default()
        })
    }
}

/// How a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Ending {
    Closed,
    /// The player said it froze and the launcher stopped it.
    Froze,
    GpuReset,
    GpuError,
    LeonidaError,
    Crashed,
    Other,
}

/// DXGI_ERROR_DEVICE_REMOVED, the exit code of a lost device.
pub const DEVICE_REMOVED: u32 = 0x887A_0005;

pub fn ending(code: Option<i32>, stopped_here: bool, gpu_failure: bool, removed: bool) -> Ending {
    if stopped_here {
        return Ending::Froze;
    }
    let code = code.map(|c| c as u32);
    if gpu_failure || code == Some(DEVICE_REMOVED) {
        return if removed {
            Ending::GpuReset
        } else {
            Ending::GpuError
        };
    }
    match code {
        Some(0) => Ending::Closed,
        Some(1) => Ending::LeonidaError,
        Some(c) if c & 0xF000_0000 == 0xC000_0000 => Ending::Crashed,
        _ => Ending::Other,
    }
}

impl Ending {
    /// For the player, right after the run. Exit 0 is also a game that quit by itself, so the
    /// questions let them say which.
    pub fn words(self) -> &'static str {
        match self {
            Ending::Closed => "The game closed normally",
            Ending::Froze => "The game froze and the launcher stopped it",
            Ending::GpuReset => {
                "The game's picture stopped (Windows reset the graphics driver). Your graphics \
                 card is fine"
            }
            Ending::GpuError => {
                "The game stopped with a graphics driver error. Your graphics card is fine"
            }
            Ending::LeonidaError => "Leonida Legacy stopped with an error",
            Ending::Crashed => "The game crashed",
            Ending::Other => "The game ended unexpectedly",
        }
    }

    /// For the report's sentences: short, without the reassurance meant for the player.
    pub fn report(self) -> &'static str {
        match self {
            Ending::Closed => "the game closed normally",
            Ending::Froze => "it froze and the launcher stopped it",
            Ending::GpuReset => "the graphics driver was reset",
            Ending::GpuError => "it stopped with a graphics driver error",
            Ending::LeonidaError => "Leonida Legacy stopped with an error",
            Ending::Crashed => "the game crashed",
            Ending::Other => "the game ended unexpectedly",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Ending::Closed => "closed",
            Ending::Froze => "froze",
            Ending::GpuReset => "gpu-reset",
            Ending::GpuError => "gpu-error",
            Ending::LeonidaError => "error",
            Ending::Crashed => "crash",
            Ending::Other => "other",
        }
    }

    pub fn stopped(self) -> bool {
        self != Ending::Closed
    }
}

pub fn hex(code: Option<i32>) -> String {
    code.map_or_else(|| "none".into(), |c| format!("0x{:08X}", c as u32))
}

/// The result of a pair: positive when the setting made the median frame shorter.
#[derive(Debug, PartialEq)]
pub enum Compare {
    Same,
    Faster(f64),
    Slower(f64),
}

pub fn compare(without: f32, with: f32) -> Compare {
    let (without, with) = (f64::from(without), f64::from(with));
    let band = without.max(with) * SAME_SHARE;
    let change = (without - with) / without * 100.0;
    if (without - with).abs() < band || without <= 0.0 {
        Compare::Same
    } else if with < without {
        Compare::Faster(change)
    } else {
        Compare::Slower(-change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames_line(each: &[u32]) -> String {
        let list: Vec<String> = each.iter().map(u32::to_string).collect();
        format!(
            "\u{1b}[2m2026-11-27T18:00:00Z\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m leonida_gpu::framelog: \
             frame times: {} frames in 1.0 s: shortest 1 ms, median 2, longest 3; 0 long (over \
             40 ms, the usual frame of the last 60 being 2); each: {}",
            each.len(),
            list.join(" ")
        )
    }

    fn sample_line(fps: f32, draws: u32) -> String {
        format!(
            " INFO leonida_gpu::host: {fps:.1} fps, {:.1} ms/frame (12.0 preparing: copies \
             1.0 ms 2.0 MB), {draws} draws/frame, dropped 0 without a host pipeline",
            1000.0 / fps
        )
    }

    fn seen(lines: &[(f64, String)]) -> Seen {
        let mut seen = Seen::default();
        for (at, line) in lines {
            assert!(worth(line));
            if let Some(parsed) = parse(line) {
                seen.take(*at, parsed);
            }
        }
        seen
    }

    #[test]
    fn lines_become_figures() {
        let Some(Line::Frames(seconds, frames)) = parse(&frames_line(&[30, 31, 45])) else {
            panic!("frame times not read");
        };
        assert_eq!((seconds, frames), (1.0, vec![30.0, 31.0, 45.0]));
        let Some(Line::Sample(fps, draws)) = parse(&sample_line(21.3, 4200)) else {
            panic!("sample not read");
        };
        assert_eq!((fps, draws), (21.3, Some(4200.0)));
        let removed = parse("host GPU failure: present; device removed: 0x887A0006");
        assert!(matches!(
            removed,
            Some(Line::GpuFailure {
                device_removed: true
            })
        ));
        assert!(!worth(
            "copied whole from 0x1+0x2: 1.0x/frame, 2.00 ms/frame"
        ));
        assert!(parse("frame times: 3 frames in 1.0 s; each: 1 2").is_none());
    }

    #[test]
    fn the_window_skips_loading_and_the_exit() {
        let mut lines = Vec::new();
        // 60 s of loading with few draws, then 200 s of play, one reading a second.
        for s in 1..=260 {
            let at = f64::from(s);
            let play = s > 60;
            let ms = if play { 40 } else { 200 };
            lines.push((at, frames_line(&vec![ms; 1000 / ms as usize])));
            if s % 5 == 0 {
                let draws = if play { 4000 } else { 300 };
                lines.push((at, sample_line(1000.0 / ms as f32, draws)));
            }
        }
        // The last seconds before the exit: a long closing frame.
        lines.push((259.5, frames_line(&[900])));
        let seen = seen(&lines);
        assert_eq!(seen.play_start(), Some(65.0));
        let window = seen.summary(260.0, WINDOW_S).unwrap();
        assert_eq!(window.median_ms, 40.0);
        assert_eq!(window.longest_ms, 40.0, "the exit's frame is left out");
        assert!((window.seconds - 120.0).abs() < 0.01);
        assert_eq!(window.draws, Some(4000.0));
        assert_eq!(window.steady_pct, 100.0);
        let short = seen.summary(260.0, WINDOW_SHORT_S).unwrap();
        assert!((short.seconds - 60.0).abs() < 0.01);
        assert!(seen.played() > 190.0 && seen.played() < 200.0);
        let whole = seen.whole().unwrap();
        assert_eq!(whole.longest_ms, 900.0);
    }

    #[test]
    fn a_run_quit_soon_after_loading_is_short() {
        let mut lines = Vec::new();
        for s in 1..=200 {
            let play = s > 170;
            lines.push((f64::from(s), frames_line(&[if play { 30 } else { 100 }])));
            if s % 5 == 0 {
                lines.push((
                    f64::from(s),
                    sample_line(30.0, if play { 5000 } else { 100 }),
                ));
            }
        }
        let seen = seen(&lines);
        assert!(seen.played() < SHORT_S, "{}", seen.played());
    }

    #[test]
    fn stutter_and_cap_figures() {
        let mut frames: Vec<f32> = vec![33.0; 95];
        frames.extend([60.0, 70.0, 120.0, 34.0, 32.0]);
        let s = summarize(5.0, &mut frames).unwrap();
        assert_eq!(s.median_ms, 33.0);
        assert_eq!(s.p99_ms, 70.0);
        assert_eq!(s.longest_ms, 120.0);
        assert_eq!((s.over50, s.over100), (3, 1));
        assert_eq!(s.steady_pct, 97.0);
        assert_eq!(s.fps, 20.0);
    }

    #[test]
    fn samples_stand_in_without_a_frame_log() {
        let lines: Vec<(f64, String)> = (1..=40)
            .map(|i| (f64::from(i * 5), sample_line(25.0, 3000)))
            .collect();
        let s = seen(&lines).summary(200.0, WINDOW_S).unwrap();
        assert!(s.from_samples);
        assert_eq!(s.median_ms, 40.0);
    }

    #[test]
    fn endings_in_plain_words() {
        assert_eq!(ending(Some(0), false, false, false), Ending::Closed);
        assert_eq!(
            ending(Some(DEVICE_REMOVED as i32), false, false, true),
            Ending::GpuReset
        );
        assert_eq!(
            ending(Some(DEVICE_REMOVED as i32), false, false, false),
            Ending::GpuError
        );
        assert_eq!(ending(Some(1), false, false, false), Ending::LeonidaError);
        assert_eq!(
            ending(Some(0xC000_0005_u32 as i32), false, false, false),
            Ending::Crashed
        );
        assert_eq!(ending(Some(1), true, false, false), Ending::Froze);
        assert_eq!(hex(Some(DEVICE_REMOVED as i32)), "0x887A0005");
        assert!(
            Ending::GpuReset
                .words()
                .contains("Your graphics card is fine")
        );
        assert!(!Ending::GpuReset.words().contains("lost"));
        assert!(!Ending::GpuReset.report().contains("fine"));
    }

    #[test]
    fn the_same_band_grows_with_the_frame_time() {
        assert_eq!(compare(47.0, 45.0), Compare::Same);
        assert!(matches!(compare(47.0, 41.0), Compare::Faster(p) if (p - 12.8).abs() < 0.1));
        // At 60 fps a 3 ms change is real; a fixed 5 ms band would call it noise.
        assert!(matches!(compare(16.0, 13.0), Compare::Faster(_)));
        assert!(matches!(compare(30.0, 40.0), Compare::Slower(p) if (p - 33.3).abs() < 0.1));
    }
}
