#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod art;
mod game_dir;
mod leonida;
mod remote;
mod settings;
mod theme;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::Duration;

use eframe::egui::{
    self, Align, Color32, Frame, Label, Layout, Margin, Rect, RichText, Sense, Stroke, Ui,
    UiBuilder, pos2, vec2,
};

use game_dir::{Found, Search};
use leonida::Profile;
use remote::{Lists, Source, Switch};
use settings::Settings;
use theme::{
    DRIFTWOOD, FLAMINGO, INSET, LINE, NIGHT, NIGHT_TOP, PLATE, PLATE_ALT, SAND, SUNSET, TEAL,
};

const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
const SEARCH_LIMIT: Duration = Duration::from_secs(60);
const PLAY: &str = "run --execute";
const WARM_UP: &str = "translate --warm all";
/// Space added between plates on top of the item spacing.
const PLATE_GAP: f32 = 6.0;
const FLASH: f64 = 1.2;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Leonida Launcher")
            .with_inner_size([780.0, 880.0])
            .with_min_inner_size([600.0, 520.0])
            .with_icon(Arc::new(theme::icon())),
        ..Default::default()
    };
    eframe::run_native(
        "Leonida Launcher",
        options,
        Box::new(|cc| {
            theme::install(&cc.egui_ctx);
            Ok(Box::new(Launcher::new(&cc.egui_ctx)))
        }),
    )
}

/// Runs `work` on its own thread and wakes the window when it is done.
fn background<T: Send + 'static>(
    ctx: &egui::Context,
    work: impl FnOnce() -> T + Send + 'static,
) -> Receiver<T> {
    let (send, receive) = mpsc::channel();
    let ctx = ctx.clone();
    thread::spawn(move || {
        let _ = send.send(work());
        ctx.request_repaint();
    });
    receive
}

/// The result of a background job once it is there; None while it runs or after it died.
fn finished<T>(job: &mut Option<Receiver<T>>) -> Option<T> {
    let result = match job.as_ref()?.try_recv() {
        Ok(result) => Some(result),
        Err(TryRecvError::Empty) => return None,
        Err(TryRecvError::Disconnected) => None,
    };
    *job = None;
    result
}

/// How the run bar shows a status message.
#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Info,
    Ok,
    Warn,
}

/// The two steps Play needs.
#[derive(Clone, Copy, PartialEq)]
enum Step {
    Game,
    Leonida,
}

struct Launcher {
    settings: Settings,
    game_name: String,
    switches: Vec<Switch>,
    switches_from: Source,
    downloaded_profiles: Option<String>,
    profiles: Vec<Profile>,
    lists: Option<Receiver<Lists>>,
    search: Option<Receiver<Search>>,
    found: Vec<Found>,
    status: String,
    status_tone: Tone,
    /// The step a click on the unready Play button asked to be shown.
    reveal: Option<Step>,
    /// The plate that flashes after a reveal, and until when.
    flash: Option<(Step, f64)>,
    dirty: bool,
}

impl Launcher {
    fn new(ctx: &egui::Context) -> Self {
        let mut settings = Settings::load();
        if !Path::new(&settings.leonida_exe).is_file()
            && let Some(exe) = leonida::find()
        {
            settings.leonida_exe = exe.display().to_string();
        }
        let game_name = game_dir::game_name(Path::new(&settings.game_dir));
        let mut launcher = Self {
            settings,
            game_name,
            switches: remote::built_in_switches(),
            switches_from: Source::BuiltIn,
            downloaded_profiles: None,
            profiles: Vec::new(),
            lists: Some(background(ctx, remote::load)),
            search: None,
            found: Vec::new(),
            status: String::new(),
            status_tone: Tone::Info,
            reveal: None,
            flash: None,
            dirty: true,
        };
        launcher.reload_profiles();
        launcher
    }

    fn exe(&self) -> PathBuf {
        PathBuf::from(&self.settings.leonida_exe)
    }

    fn has_exe(&self) -> bool {
        !self.settings.leonida_exe.is_empty() && self.exe().is_file()
    }

    fn has_game(&self) -> bool {
        game_dir::has_config(Path::new(&self.settings.game_dir))
    }

    fn set_status(&mut self, text: impl Into<String>, tone: Tone) {
        self.status = text.into();
        self.status_tone = tone;
    }

    fn reload_profiles(&mut self) {
        let folder = leonida::folder(&self.exe());
        self.profiles = leonida::profiles(&folder, self.downloaded_profiles.as_deref());
    }

    fn set_game_dir(&mut self, dir: PathBuf) {
        self.game_name = game_dir::game_name(&dir);
        self.settings.game_dir = dir.display().to_string();
        self.dirty = true;
    }

    fn set_leonida(&mut self, exe: PathBuf) {
        self.settings.leonida_exe = exe.display().to_string();
        self.reload_profiles();
        self.dirty = true;
    }

    fn poll(&mut self) {
        if let Some(lists) = finished(&mut self.lists) {
            self.switches = lists.switches;
            self.switches_from = lists.switches_from;
            self.downloaded_profiles = lists.profiles;
            self.reload_profiles();
        }
        let searching = self.search.is_some();
        let Some(search) = finished(&mut self.search) else {
            if searching && self.search.is_none() {
                self.set_status(
                    "The search stopped unexpectedly. Pick the folder yourself.",
                    Tone::Warn,
                );
            }
            return;
        };
        match search.games.len() {
            0 => self.set_status("No game folder found. Pick it yourself.", Tone::Warn),
            1 => self.set_status(format!("Found {}.", search.games[0].name), Tone::Ok),
            n => self.set_status(
                format!("Found {n} games. Click your copy of the game."),
                Tone::Info,
            ),
        }
        if search.timed_out {
            self.status += " Stopped looking after 60 s.";
        }
        if let [only] = search.games.as_slice()
            && !self.has_game()
        {
            self.set_game_dir(only.dir.clone());
        }
        self.found = search.games;
    }

    fn pick_game(&mut self, folder: bool) {
        let dialog = rfd::FileDialog::new().set_title("Your copy of the game");
        let picked = if folder {
            dialog.pick_folder()
        } else {
            dialog.add_filter("The game's .exe", &["exe"]).pick_file()
        };
        let Some(path) = picked else { return };
        match game_dir::resolve(&path) {
            Some(dir) => {
                self.set_game_dir(dir);
                self.status.clear();
            }
            None => {
                let text = format!(
                    "No MicrosoftGame.config in {} or the folders around it. Pick the folder \
                     the game was installed to.",
                    path.display()
                );
                self.set_status(text, Tone::Warn);
            }
        }
    }

    /// The ticked profiles that are in the list; the others are not sent.
    fn ticked_profiles(&self) -> Vec<&str> {
        self.settings
            .profiles
            .iter()
            .filter(|name| self.profiles.iter().any(|p| &p.name == *name))
            .map(String::as_str)
            .collect()
    }

    /// How many listed switches are set to something other than default.
    fn changed_switches(&self) -> usize {
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
    fn extra_lines(&self) -> Vec<(String, String)> {
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

    /// The environment of a run: the game folder, the ticked profiles and the chosen switches.
    fn env(&self) -> Vec<(String, String)> {
        let mut env = vec![(
            "LEONIDA_GAME_DIR".to_string(),
            self.settings.game_dir.clone(),
        )];
        let profiles = self.ticked_profiles();
        if !profiles.is_empty() {
            env.push(("LEONIDA_PROFILE".into(), profiles.join(",")));
        }
        for switch in &self.switches {
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
    fn powershell(&self, args: &str) -> String {
        let quote = |text: &str| format!("'{}'", text.replace('\'', "''"));
        let mut text = String::new();
        for (name, value) in self.env() {
            let _ = writeln!(text, "$env:{name} = {}", quote(&value));
        }
        let folder = leonida::folder(&self.exe());
        let _ = writeln!(
            text,
            "Set-Location -LiteralPath {}",
            quote(&folder.to_string_lossy())
        );
        let _ = write!(text, "& {} {args}", quote(&self.settings.leonida_exe));
        text
    }

    fn launch(&mut self, args: &str) {
        if !self.has_exe() {
            self.set_status("Pick leonida.exe first.", Tone::Warn);
            return;
        }
        if !self.has_game() {
            self.set_status("Pick the game folder first.", Tone::Warn);
            return;
        }
        if leonida::running() {
            self.set_status(
                "leonida.exe is already running. Close it first.",
                Tone::Warn,
            );
            return;
        }
        let exe = self.exe();
        // cmd /k keeps the window, and leonida.exe's log in it, open after the run ends.
        let started = Command::new("cmd.exe")
            .raw_arg(format!("/k \"\"{}\" {args}\"", exe.display()))
            .current_dir(leonida::folder(&exe))
            .envs(self.env())
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn();
        match (started, args) {
            (Ok(_), PLAY) => {
                self.set_status("Started the game. Its log is in the new window.", Tone::Ok)
            }
            (Ok(_), _) => self.set_status(
                "Warming up the shaders in a new window. Play when it is done.",
                Tone::Ok,
            ),
            (Err(error), _) => {
                self.set_status(format!("Couldn't start leonida.exe: {error}"), Tone::Warn)
            }
        }
    }

    /// Whether `step`'s plate flashes now; starts the flash when Play asked to show it.
    fn flashing(&mut self, ui: &Ui, step: Step) -> (bool, bool) {
        let now = ui.input(|i| i.time);
        let reveal = self.reveal == Some(step);
        if reveal {
            self.reveal = None;
            self.flash = Some((step, now + FLASH));
            ui.ctx()
                .request_repaint_after(Duration::from_secs_f64(FLASH));
        }
        let flash = self
            .flash
            .is_some_and(|(s, until)| s == step && now < until);
        (flash, reveal)
    }

    fn step_plate(&mut self, ui: &mut Ui, step: Step, body: impl FnOnce(&mut Self, &mut Ui)) {
        let (flash, reveal) = self.flashing(ui, step);
        let ready = match step {
            Step::Game => self.has_game(),
            Step::Leonida => self.has_exe(),
        };
        let strip = if ready { TEAL } else { SUNSET };
        let rect = theme::plate(ui, strip, flash, |ui| body(self, ui));
        if reveal {
            ui.scroll_to_rect(rect, Some(Align::Center));
        }
    }

    fn game_plate(&mut self, ui: &mut Ui) {
        self.step_plate(ui, Step::Game, Self::game_body);
    }

    fn game_body(&mut self, ui: &mut Ui) {
        let ready = self.has_game();
        theme::plate_header(ui, Some((1, ready)), "Game", |ui| ready_pill(ui, ready));
        if self.settings.game_dir.is_empty() {
            theme::hint(
                ui,
                "Find your copy of the game, or pick the folder it was installed to.",
            );
        } else if ready {
            ui.label(
                RichText::new(&self.game_name)
                    .text_style(theme::strong())
                    .color(SAND),
            );
            path_label(ui, &self.settings.game_dir);
        } else {
            ui.horizontal_top(|ui| {
                theme::marker(ui, "!", SUNSET, NIGHT);
                let text = format!("No MicrosoftGame.config in {}", self.settings.game_dir);
                ui.add(Label::new(RichText::new(text).color(SUNSET)).wrap());
            });
        }
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            let searching = self.search.is_some();
            let label = "Find it automatically";
            let find = if ready {
                egui::Button::new(label)
            } else {
                theme::primary(label)
            };
            let find = ui
                .add_enabled(!searching, find)
                .on_hover_text("Looks through this PC's drives, up to a minute");
            if find.clicked() {
                self.search = Some(background(ui.ctx(), || game_dir::find(SEARCH_LIMIT)));
                self.found.clear();
                self.status.clear();
            }
            if ui.button("Pick folder").clicked() {
                self.pick_game(true);
            }
            if ui.button("Pick the game's .exe").clicked() {
                self.pick_game(false);
            }
        });
        if self.search.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                theme::hint(ui, "Looking through your drives, up to a minute...");
            });
        }
        let mut chosen = None;
        for game in &self.found {
            let selected = Path::new(&self.settings.game_dir) == game.dir;
            let path = game.dir.display().to_string();
            if theme::found_row(ui, &game.name, &path, selected).clicked() {
                chosen = Some(game.dir.clone());
            }
        }
        if let Some(dir) = chosen {
            self.set_game_dir(dir);
        }
    }

    fn leonida_plate(&mut self, ui: &mut Ui) {
        self.step_plate(ui, Step::Leonida, Self::leonida_body);
    }

    fn leonida_body(&mut self, ui: &mut Ui) {
        let ready = self.has_exe();
        theme::plate_header(ui, Some((2, ready)), "Leonida", |ui| ready_pill(ui, ready));
        if ready {
            path_label(ui, &self.settings.leonida_exe);
        } else {
            theme::hint(ui, "Build leonida.exe, then find or pick it.");
            theme::code_chip(ui, "cargo build --release");
        }
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            let find = if ready {
                egui::Button::new("Find it")
            } else {
                theme::primary("Find it")
            };
            if ui.add(find).clicked() {
                match leonida::find() {
                    Some(exe) => self.set_leonida(exe),
                    None => self.set_status("No leonida.exe found. Pick it yourself.", Tone::Warn),
                }
            }
            if ui.button("Pick leonida.exe").clicked()
                && let Some(exe) = rfd::FileDialog::new()
                    .set_title("leonida.exe")
                    .add_filter("leonida.exe", &["exe"])
                    .pick_file()
            {
                self.set_leonida(exe);
            }
        });
    }

    fn profiles_plate(&mut self, ui: &mut Ui) {
        theme::plate(ui, LINE, false, |ui| self.profiles_body(ui));
    }

    fn profiles_body(&mut self, ui: &mut Ui) {
        let ticked = self.ticked_profiles().len();
        let listed = !self.profiles.is_empty();
        theme::plate_header(ui, None, "Profiles", |ui| {
            if ticked > 0 {
                theme::tag(ui, &format!("Your picks ({ticked})"), FLAMINGO, SAND);
            } else if listed {
                theme::hint(ui, "Picks by card");
            }
        });
        if !listed {
            ui.horizontal_top(|ui| {
                theme::marker(ui, "i", DRIFTWOOD, NIGHT);
                let text = "No profile list: no checkout beside leonida.exe and GitHub didn't \
                            answer. You can still name one under Extra switches as \
                            LEONIDA_PROFILE=name.";
                ui.add(Label::new(RichText::new(text).small().color(DRIFTWOOD)).wrap());
            });
            return;
        }
        if ticked == 0 {
            theme::hint(
                ui,
                "Leonida picks by your graphics card. Tick one to choose yourself.",
            );
        } else {
            ui.horizontal_wrapped(|ui| {
                theme::hint(
                    ui,
                    "Your ticks replace the card's picks. Tick those too to keep them.",
                );
                if ui.small_button("Use card picks").clicked() {
                    self.settings.profiles.clear();
                    self.dirty = true;
                }
            });
        }
        let profiles = &self.profiles;
        let picks = &mut self.settings.profiles;
        let mut changed = false;
        let groups: [ProfileGroup; 2] = [
            ("Picked by your card", |p| !p.local && !p.opt_in),
            ("Opt-in", |p| !p.local && p.opt_in),
        ];
        for (name, keep) in groups {
            if profiles.iter().any(keep) {
                ui.add_space(2.0);
                ui.label(theme::caption(name));
                changed |= profile_chips(ui, profiles, picks, keep);
            }
        }
        let own = profiles.iter().filter(|p| p.local).count();
        if own > 0 {
            let own_ticked = profiles
                .iter()
                .filter(|p| p.local && picks.contains(&p.name))
                .count();
            let text = if own_ticked > 0 {
                format!("Your own ({own}), {own_ticked} ticked")
            } else {
                format!("Your own ({own})")
            };
            ui.add_space(2.0);
            egui::CollapsingHeader::new(theme::caption(&text))
                .id_salt("own-profiles")
                .default_open(false)
                .show(ui, |ui| {
                    changed |= profile_chips(ui, profiles, picks, |p| p.local);
                });
        }
        if changed {
            self.dirty = true;
        }
    }

    fn switches_plate(&mut self, ui: &mut Ui) {
        theme::plate(ui, LINE, false, |ui| self.switches_body(ui));
    }

    fn switches_body(&mut self, ui: &mut Ui) {
        let loading = self.lists.is_some();
        let mut refresh = false;
        let from = self.switches_from;
        theme::plate_header(ui, None, "Switches", |ui| {
            if loading {
                theme::hint(ui, "Checking GitHub...");
                ui.spinner();
                return;
            }
            refresh = ui.small_button("Refresh").clicked();
            let text = RichText::new(from.describe()).small().color(DRIFTWOOD);
            ui.add(Label::new(text).truncate().show_tooltip_when_elided(false))
                .on_hover_text(from.explain());
            let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
            let color = match from {
                Source::GitHub => TEAL,
                Source::LastDownload => SUNSET,
                Source::BuiltIn => DRIFTWOOD,
            };
            ui.painter().circle_filled(dot.center(), 4.0, color);
        });
        if refresh {
            self.lists = Some(background(ui.ctx(), remote::load));
        }
        theme::hint(
            ui,
            "All start with LEONIDA_. default keeps what the profiles set.",
        );
        let changed = self.changed_switches();
        if changed > 0 {
            ui.horizontal(|ui| {
                theme::pill(ui, &format!("{changed} changed"), SUNSET);
                if ui.small_button("Reset all").clicked() {
                    self.settings.switches.clear();
                    self.dirty = true;
                }
            });
        }
        ui.add_space(2.0);
        let switches = &self.switches;
        let values = &mut self.settings.switches;
        let mut dirty = false;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for (row, switch) in switches.iter().filter(|s| s.build.is_empty()).enumerate() {
                dirty |= switch_row(ui, switch, values, row % 2 == 1);
            }
        });
        let tests: Vec<&Switch> = switches.iter().filter(|s| !s.build.is_empty()).collect();
        if !tests.is_empty() {
            let changed = tests
                .iter()
                .filter(|s| values.get(&s.name).is_some_and(|v| !v.is_empty()))
                .count();
            let count = tests.len();
            let text = if changed > 0 {
                format!("Test builds ({count}), {changed} changed")
            } else {
                format!("Test builds ({count})")
            };
            ui.add_space(4.0);
            egui::CollapsingHeader::new(theme::caption(&text))
                .id_salt("test-builds")
                .default_open(changed > 0)
                .show(ui, |ui| {
                    theme::hint(ui, "Need a branch build. Main builds ignore them.");
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for (row, switch) in tests.iter().enumerate() {
                        dirty |= switch_row(ui, switch, values, row % 2 == 1);
                    }
                });
        }
        if dirty {
            self.dirty = true;
        }
    }

    fn extra_plate(&mut self, ui: &mut Ui) {
        theme::plate(ui, LINE, false, |ui| self.extra_body(ui));
    }

    fn extra_body(&mut self, ui: &mut Ui) {
        theme::plate_header(ui, None, "Extra switches", |_| {});
        theme::hint(
            ui,
            "One NAME=value per line for switches not in the list. # starts a comment.",
        );
        let extra = egui::TextEdit::multiline(&mut self.settings.extra)
            .desired_rows(3)
            .desired_width(f32::INFINITY)
            .hint_text("LEONIDA_EXAMPLE=1")
            .font(egui::TextStyle::Monospace)
            .frame(Frame::NONE);
        if well(ui, |ui| ui.add(extra)).changed() {
            self.dirty = true;
        }
        ui.add_space(2.0);
        egui::CollapsingHeader::new(theme::caption("Run command"))
            .id_salt("run-command")
            .show(ui, |ui| {
                theme::hint(
                    ui,
                    "What Play runs, as PowerShell. Copy it into a test report.",
                );
                let mut script = self.powershell(PLAY);
                let mut shown_script = script.as_str();
                well(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut shown_script)
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace)
                            .frame(Frame::NONE),
                    )
                });
                if ui.button("Copy run command").clicked() {
                    ui.ctx().copy_text(std::mem::take(&mut script));
                    self.set_status("Copied the run command.", Tone::Ok);
                }
            });
    }

    /// The plates, in two columns when there is room for them.
    fn board(&mut self, ui: &mut Ui) {
        let area = ui.max_rect();
        ui.painter()
            .add(theme::vgradient(area, &[(0.0, NIGHT_TOP), (1.0, NIGHT)]));
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let available = ui.available_width();
                let content = (available - 48.0).min(1180.0);
                let margin = ((available - content) / 2.0).max(24.0);
                ui.add_space(18.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(margin);
                    if content >= 700.0 {
                        let left = (content * 0.45).floor();
                        let right = content - left - 16.0;
                        ui.spacing_mut().item_spacing.x = 16.0;
                        column(ui, left, |ui| {
                            self.game_plate(ui);
                            ui.add_space(PLATE_GAP);
                            self.leonida_plate(ui);
                            ui.add_space(PLATE_GAP);
                            self.profiles_plate(ui);
                        });
                        column(ui, right, |ui| {
                            self.switches_plate(ui);
                            ui.add_space(PLATE_GAP);
                            self.extra_plate(ui);
                        });
                    } else {
                        column(ui, content, |ui| {
                            self.game_plate(ui);
                            ui.add_space(PLATE_GAP);
                            self.leonida_plate(ui);
                            ui.add_space(PLATE_GAP);
                            self.profiles_plate(ui);
                            ui.add_space(PLATE_GAP);
                            self.switches_plate(ui);
                            ui.add_space(PLATE_GAP);
                            self.extra_plate(ui);
                        });
                    }
                });
                ui.add_space(28.0);
            });
        let fade = Rect::from_min_max(pos2(area.left(), area.bottom() - 24.0), area.max);
        let stops = [(0.0, Color32::TRANSPARENT), (1.0, NIGHT)];
        ui.painter().add(theme::vgradient(fade, &stops));
    }

    /// What a run will use, in a few words.
    fn summary(&self) -> String {
        let ticked = self.ticked_profiles();
        let mut text = if ticked.is_empty() {
            "Card picks".to_string()
        } else {
            ticked.join(", ")
        };
        match self.changed_switches() {
            0 => text += ", no switch changes",
            1 => text += ", 1 switch changed",
            n => {
                let _ = write!(text, ", {n} switches changed");
            }
        }
        match self.extra_lines().len() {
            0 => {}
            1 => text += ", 1 extra line",
            n => {
                let _ = write!(text, ", {n} extra lines");
            }
        }
        text
    }

    fn run_bar(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        let painter = ui.painter();
        painter.add(theme::vgradient(rect, &[(0.0, NIGHT_TOP), (1.0, NIGHT)]));
        theme::neon_line(painter, rect);
        let (has_game, has_exe) = (self.has_game(), self.has_exe());
        let ready = has_game && has_exe;
        let inner = rect.shrink2(vec2(24.0, 12.0));
        let layout = Layout::right_to_left(Align::Center);
        ui.scope_builder(UiBuilder::new().max_rect(inner).layout(layout), |ui| {
            let play = theme::play_button(ui, ready);
            let play = if ready {
                play.on_hover_text("Starts the game with the choices above")
            } else if !has_game {
                play.on_hover_text("Pick the game folder first. Click to show where.")
            } else {
                play.on_hover_text("Pick leonida.exe first. Click to show where.")
            };
            if play.clicked() {
                if ready {
                    self.launch(PLAY);
                } else {
                    self.reveal = Some(if has_game { Step::Leonida } else { Step::Game });
                }
            }
            ui.add_space(4.0);
            let warm = theme::outline_pill(ui, "Warm up shaders", ready);
            let warm = warm.on_hover_text(
                "Translates the game's shaders before playing, a few minutes. Do it once \
                 after each new build and after changing a shader switch.",
            );
            if warm.clicked() {
                self.launch(WARM_UP);
            }
            ui.add_space(8.0);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                self.status_block(ui, has_game, has_exe);
            });
        });
    }

    fn status_block(&self, ui: &mut Ui, has_game: bool, has_exe: bool) {
        let (text, tone) = if !self.status.is_empty() {
            (self.status.as_str(), self.status_tone)
        } else {
            match (has_game, has_exe) {
                (true, true) => ("Ready to play.", Tone::Ok),
                (false, true) => ("Pick the game folder to play.", Tone::Warn),
                (true, false) => ("Pick leonida.exe to play.", Tone::Warn),
                (false, false) => ("Pick the game folder and leonida.exe to play.", Tone::Warn),
            }
        };
        let (dot, color) = match tone {
            Tone::Ok => (TEAL, TEAL),
            Tone::Warn => (SUNSET, SUNSET),
            Tone::Info => (DRIFTWOOD, SAND),
        };
        let width = ui.available_width();
        let font = egui::TextStyle::Body.resolve(ui.style());
        let galley = ui
            .painter()
            .layout(text.to_string(), font, color, (width - 17.0).max(40.0));
        let one_line = galley.rows.len() <= 1;
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let (spot, _) =
                    ui.allocate_exact_size(vec2(9.0, galley.rows[0].height()), Sense::hover());
                ui.painter().circle_filled(spot.center(), 4.5, dot);
                ui.add(Label::new(galley));
            });
            if one_line {
                let summary = RichText::new(self.summary()).small().color(DRIFTWOOD);
                ui.horizontal(|ui| {
                    ui.add_space(17.0);
                    ui.add(Label::new(summary).truncate());
                });
            }
        });
    }
}

/// A Ready or Needed pill for a step plate's header.
fn ready_pill(ui: &mut Ui, ready: bool) {
    if ready {
        theme::pill(ui, "Ready", TEAL);
    } else {
        theme::pill(ui, "Needed", SUNSET);
    }
}

/// A path on one line, cut short when it doesn't fit; the whole path shows on hover.
fn path_label(ui: &mut Ui, path: &str) {
    let text = RichText::new(path).small().color(DRIFTWOOD);
    ui.add(Label::new(text).truncate());
}

/// A dark well for text, the extra switches and the run command.
fn well<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(INSET)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(8)
        .inner_margin(Margin::same(8))
        .show(ui, add)
        .inner
}

/// A column of the board, `width` wide.
fn column(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.x = 8.0;
        add(ui);
    });
}

/// A caption and the profiles under it.
type ProfileGroup = (&'static str, fn(&Profile) -> bool);

/// Chips for the profiles `keep` accepts. Returns whether a tick changed.
fn profile_chips(
    ui: &mut Ui,
    profiles: &[Profile],
    picks: &mut Vec<String>,
    keep: impl Fn(&Profile) -> bool,
) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for profile in profiles.iter().filter(|p| keep(p)) {
            let mut on = picks.contains(&profile.name);
            let response = theme::chip(ui, &profile.name, &mut on);
            let response = if profile.measured.is_empty() {
                response
            } else {
                response.on_hover_text(&profile.measured)
            };
            if response.changed() {
                picks.retain(|name| name != &profile.name);
                if on {
                    picks.push(profile.name.clone());
                }
                changed = true;
            }
        }
    });
    changed
}

/// One switch: its name and a drop-down of its values. Returns whether the value changed.
fn switch_row(
    ui: &mut Ui,
    switch: &Switch,
    values: &mut BTreeMap<String, String>,
    odd: bool,
) -> bool {
    let current = values.get(&switch.name).cloned().unwrap_or_default();
    let mut value = current.clone();
    let color = if current.is_empty() { SAND } else { SUNSET };
    let fill = if odd { PLATE_ALT } else { PLATE };
    let row = Frame::new()
        .fill(fill)
        .corner_radius(6)
        .inner_margin(Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let name = ui
                    .vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let name = RichText::new(&switch.name).monospace().color(color);
                        let name = ui.label(name);
                        if !switch.build.is_empty() {
                            let build = format!("needs {}", switch.build);
                            theme::tag(ui, &build, LINE, DRIFTWOOD);
                        }
                        name
                    })
                    .inner;
                let about = if switch.about.is_empty() {
                    format!("LEONIDA_{}", switch.name)
                } else {
                    format!("LEONIDA_{}\n{}", switch.name, switch.about)
                };
                name.on_hover_text(about);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    egui::ComboBox::from_id_salt(&switch.name)
                        .width(130.0)
                        .selected_text(RichText::new(shown(&value)).color(color))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut value, String::new(), "default");
                            for choice in &switch.values {
                                ui.selectable_value(&mut value, choice.clone(), shown(choice));
                            }
                        });
                });
            });
        });
    if !current.is_empty() {
        let rect = row.response.rect;
        let bar = Rect::from_min_size(rect.min, vec2(3.0, rect.height()));
        ui.painter().rect_filled(bar, 1.5, SUNSET);
    }
    if value == current {
        return false;
    }
    if value.is_empty() {
        values.remove(&switch.name);
    } else {
        values.insert(switch.name.clone(), value);
    }
    true
}

fn shown(value: &str) -> String {
    match value {
        "" => "default".into(),
        "0" => "0 (off)".into(),
        "1" => "1 (on)".into(),
        other => other.into(),
    }
}

impl eframe::App for Launcher {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll();
        let full = ui.ctx().content_rect();
        let header = if full.height() < 700.0 {
            124.0
        } else {
            (full.width() * 0.24).clamp(160.0, 210.0)
        };
        let ready = self.has_exe() && self.has_game();
        egui::Panel::top("scene")
            .exact_size(header)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::NONE)
            .show(ui, |ui| {
                let rect = ui.max_rect();
                ui.allocate_rect(rect, Sense::hover());
                art::scene(&ui.painter_at(rect), rect, ready);
            });
        egui::Panel::bottom("run")
            .exact_size(76.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::NONE)
            .show(ui, |ui| self.run_bar(ui));
        egui::CentralPanel::default()
            .frame(Frame::NONE)
            .show(ui, |ui| self.board(ui));
        if self.dirty {
            self.dirty = false;
            if let Err(error) = self.settings.save() {
                self.set_status(format!("Couldn't save the settings: {error}"), Tone::Warn);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkout_folder_for_a_built_exe() {
        let root = leonida::folder(Path::new(r"C:\x\leonida\target\release\leonida.exe"));
        assert_eq!(root, Path::new(r"C:\x\leonida"));
        let beside = leonida::folder(Path::new(r"C:\games\leonida\leonida.exe"));
        assert_eq!(beside, Path::new(r"C:\games\leonida"));
    }

    #[test]
    fn game_folder_from_a_file_inside_it() {
        let dir = std::env::temp_dir()
            .join("leonida-launcher-test")
            .join("Content");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("MicrosoftGame.config"), "<Game/>").unwrap();
        std::fs::write(dir.join("sub").join("game.exe"), "").unwrap();
        assert_eq!(
            game_dir::resolve(&dir.join("sub").join("game.exe")),
            Some(dir.clone())
        );
        assert_eq!(game_dir::resolve(dir.parent().unwrap()), Some(dir.clone()));
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }
}
