#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod game_dir;
mod leonida;
mod remote;
mod settings;
mod theme;

use std::fmt::Write as _;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::Duration;

use eframe::egui::{self, Frame, Margin, RichText};

use game_dir::{Found, Search};
use leonida::Profile;
use remote::{Lists, Source, Switch};
use settings::Settings;
use theme::{FOAM, MUTED, ORANGE};

const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
const SEARCH_LIMIT: Duration = Duration::from_secs(60);
const PLAY: &str = "run --execute";
const WARM_UP: &str = "translate --warm all";

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
                self.status = "The search stopped unexpectedly. Pick the folder yourself.".into();
            }
            return;
        };
        self.status = match search.games.len() {
            0 => "No game folder found. Pick it yourself.".into(),
            1 => format!("Found {}.", search.games[0].name),
            n => format!("Found {n} games. Click your copy of the game."),
        };
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
                self.status = format!(
                    "No MicrosoftGame.config in {} or the folders around it. Pick the folder \
                     the game was installed to.",
                    path.display()
                )
            }
        }
    }

    /// The environment of a run: the game folder, the ticked profiles and the chosen switches.
    fn env(&self) -> Vec<(String, String)> {
        let mut env = vec![(
            "LEONIDA_GAME_DIR".to_string(),
            self.settings.game_dir.clone(),
        )];
        let profiles: Vec<&str> = self
            .settings
            .profiles
            .iter()
            .filter(|name| self.profiles.iter().any(|p| &p.name == *name))
            .map(String::as_str)
            .collect();
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
        for line in self.settings.extra.lines().map(str::trim) {
            if line.starts_with('#') {
                continue;
            }
            if let Some((name, value)) = line.split_once('=')
                && !name.trim().is_empty()
            {
                env.push((name.trim().to_string(), value.trim().to_string()));
            }
        }
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
            self.status = "Pick leonida.exe first.".into();
            return;
        }
        if !self.has_game() {
            self.status = "Pick the game folder first.".into();
            return;
        }
        if leonida::running() {
            self.status = "leonida.exe is already running. Close it first.".into();
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
        self.status = match (started, args) {
            (Ok(_), PLAY) => "Started the game. Its log is in the new window.".into(),
            (Ok(_), _) => "Warming up the shaders in a new window. Play when it is done.".into(),
            (Err(error), _) => format!("Couldn't start leonida.exe: {error}"),
        };
    }

    fn game_section(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Game", Some(self.has_game()));
        if self.settings.game_dir.is_empty() {
            theme::hint(
                ui,
                "Find your copy of the game, or pick the folder it was installed to.",
            );
        } else if self.has_game() {
            ui.label(RichText::new(&self.game_name).color(FOAM));
            theme::hint(ui, &self.settings.game_dir);
        } else {
            let text = format!("No MicrosoftGame.config in {}", self.settings.game_dir);
            ui.label(RichText::new(text).color(ORANGE));
        }
        ui.horizontal_wrapped(|ui| {
            let searching = self.search.is_some();
            let find = egui::Button::new("Find it automatically");
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
                theme::hint(ui, "Looking through your drives...");
            });
        }
        let mut chosen = None;
        for game in &self.found {
            let selected = Path::new(&self.settings.game_dir) == game.dir;
            let text = format!("{}    {}", game.name, game.dir.display());
            if ui.selectable_label(selected, text).clicked() {
                chosen = Some(game.dir.clone());
            }
        }
        if let Some(dir) = chosen {
            self.set_game_dir(dir);
        }
    }

    fn leonida_section(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Leonida", Some(self.has_exe()));
        if self.has_exe() {
            theme::hint(ui, &self.settings.leonida_exe);
        } else {
            theme::hint(
                ui,
                "Build leonida.exe (cargo build --release), then find or pick it.",
            );
        }
        ui.horizontal(|ui| {
            if ui.button("Find it").clicked() {
                match leonida::find() {
                    Some(exe) => self.set_leonida(exe),
                    None => self.status = "No leonida.exe found. Pick it yourself.".into(),
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

    fn profiles_section(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Profiles", None);
        if self.profiles.is_empty() {
            theme::hint(
                ui,
                "No profile list: no checkout beside leonida.exe and GitHub didn't answer. You \
                 can still name one under Extra switches as LEONIDA_PROFILE=name.",
            );
            return;
        }
        theme::hint(
            ui,
            "Leave these unticked and Leonida picks by your graphics card. Ticking any \
             replaces the card's picks, so tick those too to keep them.",
        );
        for profile in &self.profiles {
            let mut on = self.settings.profiles.contains(&profile.name);
            let kind = match (profile.local, profile.opt_in) {
                (true, _) => "your own",
                (false, true) => "opt-in",
                (false, false) => "picked by card",
            };
            let response = ui
                .horizontal(|ui| {
                    let response = ui.checkbox(&mut on, &profile.name);
                    ui.label(RichText::new(kind).small().color(MUTED));
                    response
                })
                .inner;
            let response = if profile.measured.is_empty() {
                response
            } else {
                response.on_hover_text(&profile.measured)
            };
            if response.changed() {
                self.settings.profiles.retain(|name| name != &profile.name);
                if on {
                    self.settings.profiles.push(profile.name.clone());
                }
                self.dirty = true;
            }
        }
    }

    fn switches_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            theme::section_title(ui, "Switches");
            ui.add_space(6.0);
            if self.lists.is_some() {
                ui.spinner();
                theme::hint(ui, "checking GitHub for the newest list");
            } else {
                let from = ui.label(
                    RichText::new(self.switches_from.describe())
                        .small()
                        .color(MUTED),
                );
                from.on_hover_text(self.switches_from.explain());
                if ui.small_button("Refresh").clicked() {
                    self.lists = Some(background(ui.ctx(), remote::load));
                }
            }
        });
        theme::hint(
            ui,
            "default keeps what the profiles set. Changed ones are orange. A build without a \
             switch ignores it. Hover a name to see what it does.",
        );
        ui.add_space(2.0);
        egui::Grid::new("switches")
            .num_columns(3)
            .striped(true)
            .spacing([16.0, 6.0])
            .show(ui, |ui| {
                for switch in &self.switches {
                    let current = self
                        .settings
                        .switches
                        .get(&switch.name)
                        .cloned()
                        .unwrap_or_default();
                    let mut value = current.clone();
                    let color = if current.is_empty() { FOAM } else { ORANGE };
                    let name = RichText::new(format!("LEONIDA_{}", switch.name)).color(color);
                    let name = ui.label(name);
                    if !switch.about.is_empty() {
                        name.on_hover_text(&switch.about);
                    }
                    egui::ComboBox::from_id_salt(&switch.name)
                        .selected_text(RichText::new(shown(&value)).color(color))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut value, String::new(), "default");
                            for choice in &switch.values {
                                ui.selectable_value(&mut value, choice.clone(), shown(choice));
                            }
                        });
                    if switch.build.is_empty() {
                        ui.label("");
                    } else {
                        theme::hint(ui, &format!("needs the {} build", switch.build));
                    }
                    ui.end_row();
                    if value != current {
                        if value.is_empty() {
                            self.settings.switches.remove(&switch.name);
                        } else {
                            self.settings.switches.insert(switch.name.clone(), value);
                        }
                        self.dirty = true;
                    }
                }
            });

        theme::section(ui, "Extra switches", None);
        theme::hint(
            ui,
            "One NAME=value per line for switches not in the list. # starts a comment.",
        );
        let extra = egui::TextEdit::multiline(&mut self.settings.extra)
            .desired_rows(3)
            .desired_width(f32::INFINITY)
            .hint_text("LEONIDA_EXAMPLE=1")
            .font(egui::TextStyle::Monospace);
        if ui.add(extra).changed() {
            self.dirty = true;
        }

        ui.add_space(10.0);
        egui::CollapsingHeader::new("Run command").show(ui, |ui| {
            theme::hint(
                ui,
                "What Play runs, as PowerShell. Copy it into a test report.",
            );
            let mut script = self.powershell(PLAY);
            let mut shown_script = script.as_str();
            ui.add(
                egui::TextEdit::multiline(&mut shown_script)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
            if ui.button("Copy run command").clicked() {
                ui.ctx().copy_text(std::mem::take(&mut script));
                self.status = "Copied the run command.".into();
            }
        });
        ui.add_space(12.0);
    }

    fn run_bar(&mut self, ui: &mut egui::Ui) {
        let ready = self.has_exe() && self.has_game();
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let play = theme::play_button(ui, ready);
                let play = if ready {
                    play.on_hover_text("Starts the game with the choices above")
                } else {
                    play.on_hover_text("Pick the game folder and leonida.exe first")
                };
                if play.clicked() {
                    self.launch(PLAY);
                }
                let warm = ui.add_enabled(ready, egui::Button::new("Warm up shaders"));
                let warm = warm.on_hover_text(
                    "Translates the game's shaders before playing, a few minutes. Do it once \
                     after each new build and after changing a shader switch.",
                );
                if warm.clicked() {
                    self.launch(WARM_UP);
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let text = if !self.status.is_empty() {
                        self.status.clone()
                    } else if ready {
                        "Ready to play.".into()
                    } else {
                        "Pick the game folder and leonida.exe to play.".into()
                    };
                    ui.add(egui::Label::new(RichText::new(text).color(MUTED)).wrap());
                });
            });
        });
    }
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
        egui::Panel::bottom("run")
            .frame(
                Frame::new()
                    .fill(theme::BAY)
                    .inner_margin(Margin::symmetric(24, 14)),
            )
            .show(ui, |ui| self.run_bar(ui));
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(theme::NIGHT)
                    .inner_margin(Margin::symmetric(24, 18)),
            )
            .show(ui, |ui| {
                ui.heading("Leonida Launcher");
                theme::hint(ui, "Pick your game, choose switches, play.");
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        ui.set_max_width(720.0);
                        self.game_section(ui);
                        self.leonida_section(ui);
                        self.profiles_section(ui);
                        self.switches_section(ui);
                    });
            });
        if self.dirty {
            self.dirty = false;
            if let Err(error) = self.settings.save() {
                self.status = format!("Couldn't save the settings: {error}");
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
