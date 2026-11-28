//! What a run sends to leonida.exe, as an environment and as the same run in PowerShell.

use std::fmt::Write as _;
use std::path::Path;

use crate::leonida::{self, Profile};
use crate::remote::Switch;
use crate::settings::Settings;

pub const PLAY: &str = "run --execute";
pub const WARM_UP: &str = "translate --warm all";

/// The user's settings and the lists they are checked against.
pub struct Run<'a> {
    pub settings: &'a Settings,
    pub switches: &'a [Switch],
    pub profiles: &'a [Profile],
}

impl Run<'_> {
    /// The ticked profiles that are in the list; the others are not sent.
    pub fn ticked(&self) -> Vec<&str> {
        self.settings
            .profiles
            .iter()
            .filter(|name| self.profiles.iter().any(|p| &p.name == *name))
            .map(String::as_str)
            .collect()
    }

    /// How many listed switches are set to something other than default.
    pub fn changed_switches(&self) -> usize {
        self.switches
            .iter()
            .filter(|s| {
                self.settings
                    .switches
                    .get(&s.name)
                    .is_some_and(|v| !v.is_empty())
            })
            .count()
    }

    /// The NAME=value lines of the extra switches, comments left out.
    pub fn extra_lines(&self) -> Vec<(String, String)> {
        let mut lines = Vec::new();
        for line in self.settings.extra.lines().map(str::trim) {
            if line.starts_with('#') {
                continue;
            }
            if let Some((name, value)) = line.split_once('=')
                && !name.trim().is_empty()
            {
                lines.push((name.trim().to_string(), value.trim().to_string()));
            }
        }
        lines
    }

    /// How many of the user's own choices differ from Recommended, sent or not.
    pub fn changes(&self) -> usize {
        self.ticked().len() + self.changed_switches() + self.extra_lines().len()
    }

    /// The environment of a run: the game folder, and with the user's own choices on, the
    /// ticked profiles, the chosen switches and the extra lines.
    pub fn env(&self) -> Vec<(String, String)> {
        let mut env = vec![(
            "LEONIDA_GAME_DIR".to_string(),
            self.settings.game_dir.clone(),
        )];
        if !self.settings.advanced() {
            return env;
        }
        let profiles = self.ticked();
        if !profiles.is_empty() {
            env.push(("LEONIDA_PROFILE".into(), profiles.join(",")));
        }
        for switch in self.switches {
            if let Some(value) = self
                .settings
                .switches
                .get(&switch.name)
                .filter(|v| !v.is_empty())
            {
                env.push((format!("LEONIDA_{}", switch.name), value.clone()));
            }
        }
        env.extend(self.extra_lines());
        env
    }

    /// The same run as PowerShell commands, for test reports.
    pub fn powershell(&self, args: &str) -> String {
        let quote = |text: &str| format!("'{}'", text.replace('\'', "''"));
        let mut text = String::new();
        for (name, value) in self.env() {
            let _ = writeln!(text, "$env:{name} = {}", quote(&value));
        }
        let folder = leonida::folder(Path::new(&self.settings.leonida_exe));
        let _ = writeln!(
            text,
            "Set-Location -LiteralPath {}",
            quote(&folder.to_string_lossy())
        );
        let _ = write!(text, "& {} {args}", quote(&self.settings.leonida_exe));
        text
    }

    /// What Play sends, in a few words, for the line under it.
    pub fn uses(&self) -> String {
        if !self.settings.advanced() {
            return "Recommended for your graphics card".into();
        }
        let count = |n: usize, one: &str, many: &str| match n {
            0 => None,
            1 => Some(format!("1 {one}")),
            n => Some(format!("{n} {many}")),
        };
        let ticked = self.ticked();
        let profiles = match ticked.len() {
            0 => "your card's profiles".to_string(),
            1 | 2 => ticked.join(", "),
            n => format!("{n} profiles"),
        };
        let mut parts = vec![profiles];
        parts.extend(count(self.changed_switches(), "switch", "switches"));
        parts.extend(count(self.extra_lines().len(), "extra line", "extra lines"));
        match parts.as_slice() {
            [only] if ticked.is_empty() => format!("{only}, nothing changed"),
            [only] => only.clone(),
            [first @ .., last] => format!("{} and {last}", first.join(", ")),
            [] => unreachable!("the profiles part is always there"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn switch(name: &str) -> Switch {
        Switch {
            name: name.into(),
            values: vec!["0".into()],
            title: String::new(),
            about: String::new(),
            risk: String::new(),
            build: String::new(),
            default: String::new(),
            test: String::new(),
        }
    }

    fn profile(name: &str) -> Profile {
        Profile {
            name: name.into(),
            opt_in: true,
            measured: String::new(),
            about: String::new(),
            local: false,
            matcher: Default::default(),
        }
    }

    fn settings(advanced: bool) -> Settings {
        Settings {
            game_dir: r"D:\Games\Test\Content".into(),
            leonida_exe: r"C:\it's\leonida\target\release\leonida.exe".into(),
            profiles: vec!["texture-keep".into(), "gone".into()],
            switches: [("QUEUE2".to_string(), "0".to_string())].into(),
            extra: "# a note\nLEONIDA_HEAP_WORKER = 1\nnot a line".into(),
            advanced: Some(advanced),
            ..Settings::default()
        }
    }

    #[test]
    fn recommended_sends_only_the_game_folder_and_keeps_the_choices() {
        let settings = settings(false);
        let (switches, profiles) = ([switch("QUEUE2")], [profile("texture-keep")]);
        let run = Run {
            settings: &settings,
            switches: &switches,
            profiles: &profiles,
        };
        assert_eq!(run.env().len(), 1);
        assert_eq!(run.changes(), 3);
        assert_eq!(run.uses(), "Recommended for your graphics card");
    }

    #[test]
    fn own_choices_send_listed_profiles_switches_and_extra_lines() {
        let settings = settings(true);
        let (switches, profiles) = ([switch("QUEUE2")], [profile("texture-keep")]);
        let run = Run {
            settings: &settings,
            switches: &switches,
            profiles: &profiles,
        };
        let env = run.env();
        let names: Vec<&str> = env.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            [
                "LEONIDA_GAME_DIR",
                "LEONIDA_PROFILE",
                "LEONIDA_QUEUE2",
                "LEONIDA_HEAP_WORKER"
            ]
        );
        assert_eq!(env[1].1, "texture-keep");
        assert_eq!(run.uses(), "texture-keep, 1 switch and 1 extra line");
    }

    #[test]
    fn own_choices_without_changes_say_so() {
        let settings = Settings {
            advanced: Some(true),
            ..Settings::default()
        };
        let run = Run {
            settings: &settings,
            switches: &[],
            profiles: &[],
        };
        assert_eq!(run.uses(), "your card's profiles, nothing changed");
    }

    #[test]
    fn powershell_quotes_paths_and_runs_in_the_checkout() {
        let settings = settings(false);
        let run = Run {
            settings: &settings,
            switches: &[],
            profiles: &[],
        };
        let text = run.powershell(PLAY);
        assert!(text.contains(r"Set-Location -LiteralPath 'C:\it''s\leonida'"));
        assert!(text.ends_with(r"& 'C:\it''s\leonida\target\release\leonida.exe' run --execute"));
    }
}
