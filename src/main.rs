#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clock;
mod figures;
mod game_dir;
mod leonida;
mod pc;
mod remote;
mod report;
mod run;
mod settings;
mod trial;
mod update;

use std::collections::BTreeMap;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::async_runtime::spawn_blocking;
use tauri::{Manager, State, WindowEvent};

use leonida::Profile;
use remote::{Origin, Source, Switch, Wanted};
use run::{PLAY, Run, WARM_UP};
use settings::Settings;

const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
const SEARCH_LIMIT: Duration = Duration::from_secs(60);
/// How long after a spawn another start is refused without looking at the task list.
const START_GAP: Duration = Duration::from_secs(8);
const BUILD_STEPS: &str = "https://github.com/ProjectLeonidaL/leonida#getting-started";
const FORKED_FROM: &str = "https://github.com/fxceless/victory-gta6";
const VICTORY: &str = "https://github.com/Marvisak/VIctory";

fn main() {
    update::tidy();
    let started = tauri::Builder::default()
        .manage(Mutex::new(Launcher::new()))
        .manage(trial::Shared::default())
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event
                && trial::running(&window.state::<trial::Shared>())
            {
                api.prevent_close();
                if leave_a_run(window) {
                    let shared = window.state::<trial::Shared>().inner().clone();
                    let app = window.app_handle().clone();
                    std::thread::spawn(move || {
                        trial::stop_now(&shared);
                        app.exit(0);
                    });
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            view,
            save_choices,
            find_game,
            choose_game,
            pick_game,
            find_leonida,
            pick_leonida,
            open_build_steps,
            open_credit,
            refresh_lists,
            is_running,
            launch,
            test_setup,
            test_begin,
            test_resume,
            test_start,
            test_status,
            test_stop,
            test_retry,
            test_where,
            test_report,
            test_again,
            test_cancel,
            open_discord,
            open_invite,
            open_tests,
            check_updates,
            update_launcher,
            update_later,
            install_build,
            use_previous,
            update_checkout,
            open_release,
        ])
        .run(tauri::generate_context!());
    if let Err(error) = started {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Leonida Launcher")
            .set_description(format!(
                "The launcher window couldn't open: {error}\n\nIt needs Microsoft Edge WebView2, \
                 which is part of Windows 11. On Windows 10, install the WebView2 Runtime from \
                 Microsoft, then start the launcher again."
            ))
            .show();
    }
}

struct Launcher {
    settings: Settings,
    game_name: String,
    switches: Vec<Switch>,
    wanted: Vec<Wanted>,
    switches_from: Origin,
    downloaded_profiles: Option<String>,
    profiles_from: Origin,
    profiles: Vec<Profile>,
    /// A problem from loading the settings, told once in the first view.
    notice: Option<String>,
    starts: Starts,
    updates: update::Updates,
}

type Shared = Mutex<Launcher>;

/// Keeps a second Play or Warm up from starting while the first is checked or has just been
/// spawned: leonida.exe isn't in the task list yet then, and two runs at once fight over the
/// shader cache.
#[derive(Default)]
struct Starts {
    checking: bool,
    spawned: Option<Instant>,
}

impl Starts {
    fn claim(&mut self) -> Result<(), String> {
        if self.checking || self.spawned.is_some_and(|at| at.elapsed() < START_GAP) {
            return Err("Leonida is starting already. Its window opens in a moment.".into());
        }
        self.checking = true;
        Ok(())
    }

    fn end(&mut self, spawned: bool) {
        self.checking = false;
        if spawned {
            self.spawned = Some(Instant::now());
        }
    }
}

fn lock<'a>(state: &'a State<'_, Shared>) -> MutexGuard<'a, Launcher> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Launcher {
    fn new() -> Self {
        let (mut settings, notice) = Settings::load();
        if !Path::new(&settings.leonida_exe).is_file()
            && let Some(exe) = leonida::find()
        {
            settings.leonida_exe = exe.display().to_string();
        }
        let game_name = game_dir::game_name(Path::new(&settings.game_dir));
        let (switches, wanted) = remote::built_in();
        let mut launcher = Self {
            settings,
            game_name,
            switches,
            wanted,
            switches_from: Origin::built_in(None),
            downloaded_profiles: None,
            profiles_from: Origin::built_in(None),
            profiles: Vec::new(),
            notice,
            starts: Starts::default(),
            updates: update::Updates::new(),
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

    fn save(&self) -> Result<(), String> {
        self.settings
            .save()
            .map_err(|error| format!("Couldn't save the settings: {error}"))
    }

    fn set_game_dir(&mut self, dir: &Path) -> Result<(), String> {
        self.game_name = game_dir::game_name(dir);
        self.settings.game_dir = dir.display().to_string();
        self.save()
    }

    fn set_leonida(&mut self, exe: &Path) -> Result<(), String> {
        self.settings.leonida_exe = exe.display().to_string();
        self.reload_profiles();
        self.save()
    }

    fn run(&self) -> Run<'_> {
        Run {
            settings: &self.settings,
            switches: &self.switches,
            profiles: &self.profiles,
        }
    }

    fn view(&self) -> View {
        let run = self.run();
        let source = match self.switches_from.source {
            Source::GitHub => "github",
            Source::LastDownload => "saved",
            Source::BuiltIn => "builtin",
        };
        View {
            game_dir: self.settings.game_dir.clone(),
            game_name: self.game_name.clone(),
            has_game: self.has_game(),
            leonida_exe: self.settings.leonida_exe.clone(),
            checkout: leonida::folder(&self.exe()).display().to_string(),
            has_exe: self.has_exe(),
            advanced: self.settings.advanced(),
            picked: self.settings.profiles.clone(),
            values: self.settings.switches.clone(),
            extra: self.settings.extra.clone(),
            profiles: self.profiles.clone(),
            switches: self.switches.clone(),
            source,
            source_label: self.switches_from.label(),
            source_detail: self.switches_from.detail("switch list"),
            profiles_detail: match (self.profiles_from.source, self.profiles_from.miss) {
                (Source::BuiltIn, Some(miss)) => remote::cause(
                    miss,
                    "graphics profile list",
                    "the profiles built into leonida.exe",
                    "",
                ),
                _ => String::new(),
            },
            testing: false,
            changes: run.changes(),
            uses: run.uses(),
            command: run.powershell(PLAY),
            notice: None,
            updates: self.updates.view(&self.settings, self.has_exe()),
        }
    }
}

/// Everything the window shows, sent after every change.
#[derive(Serialize)]
struct View {
    game_dir: String,
    game_name: String,
    has_game: bool,
    leonida_exe: String,
    checkout: String,
    has_exe: bool,
    advanced: bool,
    /// The ticked profiles, also those no longer in the list.
    picked: Vec<String>,
    values: BTreeMap<String, String>,
    extra: String,
    profiles: Vec<Profile>,
    switches: Vec<Switch>,
    source: &'static str,
    source_label: String,
    /// What happened to the download, in plain words; also the Refresh toast.
    source_detail: String,
    /// Why there is no downloaded profile list, when there's none.
    profiles_detail: String,
    /// Whether a test run is going.
    testing: bool,
    changes: usize,
    uses: String,
    command: String,
    notice: Option<String>,
    updates: update::UpdatesView,
}

/// The user's own choices, as the window edits them.
#[derive(Deserialize)]
struct Choices {
    advanced: bool,
    profiles: Vec<String>,
    switches: BTreeMap<String, String>,
    extra: String,
}

#[derive(Serialize)]
struct Game {
    name: String,
    dir: String,
}

#[derive(Serialize)]
struct Searched {
    games: Vec<Game>,
    timed_out: bool,
    view: View,
}

#[derive(Serialize)]
struct Picked {
    /// False when the dialog was closed without a pick.
    picked: bool,
    view: View,
}

fn stopped(error: impl std::fmt::Display) -> String {
    format!("That stopped unexpectedly ({error}). Try again.")
}

#[tauri::command]
fn view(state: State<'_, Shared>, trials: State<'_, trial::Shared>) -> View {
    let mut launcher = lock(&state);
    let mut view = launcher.view();
    view.notice = launcher.notice.take();
    view.testing = trial::running(&trials);
    view
}

/// Asks before the window closes during a test run.
fn leave_a_run(window: &tauri::Window) -> bool {
    let answer = rfd::MessageDialog::new()
        .set_parent(window)
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Leonida Launcher")
        .set_description(
            "A test run is going. Closing the launcher stops the game, and this run isn't \
             measured.\n\nStop the game and close the launcher?",
        )
        .set_buttons(rfd::MessageButtons::YesNo)
        .show();
    answer == rfd::MessageDialogResult::Yes
}

#[tauri::command]
fn save_choices(state: State<'_, Shared>, choices: Choices) -> Result<View, String> {
    let mut launcher = lock(&state);
    let settings = &mut launcher.settings;
    settings.advanced = Some(choices.advanced);
    settings.profiles = choices.profiles;
    settings.switches = choices.switches;
    settings.switches.retain(|_, value| !value.is_empty());
    settings.extra = choices.extra;
    launcher.save()?;
    Ok(launcher.view())
}

/// Looks through this PC's drives for the game, up to a minute. The one game found is chosen
/// when no game folder is set yet.
#[tauri::command]
async fn find_game(state: State<'_, Shared>) -> Result<Searched, String> {
    let search = spawn_blocking(|| game_dir::find(SEARCH_LIMIT))
        .await
        .map_err(stopped)?;
    let mut launcher = lock(&state);
    if let [only] = search.games.as_slice()
        && !launcher.has_game()
    {
        launcher.set_game_dir(&only.dir)?;
    }
    let games = search
        .games
        .into_iter()
        .map(|game| Game {
            name: game.name,
            dir: game.dir.display().to_string(),
        })
        .collect();
    Ok(Searched {
        games,
        timed_out: search.timed_out,
        view: launcher.view(),
    })
}

#[tauri::command]
fn choose_game(state: State<'_, Shared>, dir: String) -> Result<View, String> {
    let dir = PathBuf::from(dir);
    if !game_dir::has_config(&dir) {
        return Err(format!(
            "There's no MicrosoftGame.config in {} any more. Look again.",
            dir.display()
        ));
    }
    let mut launcher = lock(&state);
    launcher.set_game_dir(&dir)?;
    Ok(launcher.view())
}

/// The game folder from a picked folder, or from the game's .exe when `exe` is true.
#[tauri::command]
async fn pick_game(state: State<'_, Shared>, exe: bool) -> Result<Picked, String> {
    let picked = spawn_blocking(move || {
        let dialog = rfd::FileDialog::new().set_title("Your copy of the game");
        if exe {
            dialog.add_filter("The game's .exe", &["exe"]).pick_file()
        } else {
            dialog.pick_folder()
        }
    })
    .await
    .map_err(stopped)?;
    let mut launcher = lock(&state);
    let Some(path) = picked else {
        return Ok(Picked {
            picked: false,
            view: launcher.view(),
        });
    };
    let Some(dir) = game_dir::resolve(&path) else {
        return Err(format!(
            "There's no MicrosoftGame.config in {} or the folders around it. Pick the folder \
             the game was installed to.",
            path.display()
        ));
    };
    launcher.set_game_dir(&dir)?;
    Ok(Picked {
        picked: true,
        view: launcher.view(),
    })
}

/// Looks beside the launcher and through Downloads, which can be slow on a synced folder.
#[tauri::command]
async fn find_leonida(state: State<'_, Shared>) -> Result<View, String> {
    let found = spawn_blocking(leonida::find).await.map_err(stopped)?;
    let Some(exe) = found else {
        return Err(
            "There's no leonida.exe beside the launcher or in a target\\release folder \
                    under Downloads. Pick it yourself."
                .into(),
        );
    };
    let mut launcher = lock(&state);
    launcher.set_leonida(&exe)?;
    Ok(launcher.view())
}

#[tauri::command]
async fn pick_leonida(state: State<'_, Shared>) -> Result<Picked, String> {
    let picked = spawn_blocking(|| {
        rfd::FileDialog::new()
            .set_title("leonida.exe")
            .add_filter("leonida.exe", &["exe"])
            .pick_file()
    })
    .await
    .map_err(stopped)?;
    let mut launcher = lock(&state);
    if let Some(exe) = &picked {
        launcher.set_leonida(exe)?;
    }
    Ok(Picked {
        picked: picked.is_some(),
        view: launcher.view(),
    })
}

/// Opens Leonida Legacy's build steps in the browser.
#[tauri::command]
fn open_build_steps() -> Result<(), String> {
    Command::new("explorer.exe")
        .arg(BUILD_STEPS)
        .spawn()
        .map(drop)
        .map_err(|error| {
            format!("Couldn't open the browser ({error}). The steps are at {BUILD_STEPS}")
        })
}

/// Opens one of the projects named in About's credits. Only these two fixed pages, never a URL
/// from the page.
#[tauri::command]
fn open_credit(which: String) -> Result<(), String> {
    let url = match which.as_str() {
        "fork" => FORKED_FROM,
        "victory" => VICTORY,
        _ => return Err(format!("There's no credit called {which}.")),
    };
    Command::new("explorer.exe")
        .arg(url)
        .spawn()
        .map(drop)
        .map_err(|error| format!("Couldn't open the browser ({error}). The page is {url}"))
}

/// Downloads the switch list and the profile catalog, up to 20 s without network.
#[tauri::command]
async fn refresh_lists(state: State<'_, Shared>) -> Result<View, String> {
    let lists = spawn_blocking(remote::load).await.map_err(stopped)?;
    let mut launcher = lock(&state);
    launcher.switches = lists.switches;
    launcher.wanted = lists.wanted;
    launcher.switches_from = lists.switches_from;
    launcher.downloaded_profiles = lists.profiles;
    launcher.profiles_from = lists.profiles_from;
    launcher.reload_profiles();
    Ok(launcher.view())
}

#[tauri::command]
async fn is_running() -> bool {
    spawn_blocking(leonida::running).await.unwrap_or(false)
}

/// Starts leonida.exe to play (`what` = "play") or to warm up the shaders ("warm") in a new
/// console window, which stays open with the log after the run ends.
#[tauri::command]
async fn launch(
    state: State<'_, Shared>,
    trials: State<'_, trial::Shared>,
    what: String,
) -> Result<String, String> {
    let (args, done) = match what.as_str() {
        "play" => (PLAY, "Started the game in a new window"),
        "warm" => (WARM_UP, "Started the warm-up in a new window"),
        _ => return Err(format!("Unknown action {what}")),
    };
    if trial::running(&trials) {
        return Err("A test run is going. Finish it in Test a setting first.".into());
    }
    {
        let mut launcher = lock(&state);
        if !launcher.has_game() {
            return Err("Pick the game folder first.".into());
        }
        if !launcher.has_exe() {
            return Err("Pick leonida.exe first.".into());
        }
        launcher.starts.claim()?;
    }
    let running = spawn_blocking(leonida::running).await.unwrap_or(false);
    let mut launcher = lock(&state);
    if running {
        launcher.starts.end(false);
        return Err("The game is already running. Close it before you start it again.".into());
    }
    let exe = launcher.exe();
    // cmd /k keeps the window, and leonida.exe's log in it, open after the run ends.
    let spawned = Command::new("cmd.exe")
        .raw_arg(format!("/k \"\"{}\" {args}\"", exe.display()))
        .current_dir(leonida::folder(&exe))
        .envs(launcher.run().env())
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn();
    launcher.starts.end(spawned.is_ok());
    spawned.map_err(|error| format!("Couldn't start leonida.exe: {error}"))?;
    if what == "warm" {
        launcher.updates.warm = false;
    }
    Ok(done.into())
}

/// Looks at the PC and leonida.exe for the test panel. Never starts the game.
#[tauri::command]
async fn test_setup(
    state: State<'_, Shared>,
    trials: State<'_, trial::Shared>,
) -> Result<trial::SetupView, String> {
    let context = {
        let launcher = lock(&state);
        if !launcher.has_game() {
            return Err("Pick the game folder first.".into());
        }
        if !launcher.has_exe() {
            return Err("Pick leonida.exe first.".into());
        }
        trial::Context {
            exe: launcher.exe(),
            game_dir: launcher.settings.game_dir.clone(),
            switches: launcher.switches.clone(),
            wanted: launcher.wanted.clone(),
            profiles: launcher.profiles.clone(),
            list: launcher.switches_from.tag.clone(),
        }
    };
    let shared = trials.inner().clone();
    spawn_blocking(move || trial::setup(&shared, context))
        .await
        .map_err(stopped)
}

#[tauri::command]
fn test_begin(
    trials: State<'_, trial::Shared>,
    begin: trial::Begin,
) -> Result<trial::TestView, String> {
    trial::begin(&trials, begin)
}

/// Starts the warm-up (`warm` true) or the next test run, after the same checks as Play.
#[tauri::command]
async fn test_start(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
    trials: State<'_, trial::Shared>,
    warm: bool,
) -> Result<(), String> {
    {
        let mut launcher = lock(&state);
        if !launcher.has_exe() {
            return Err("Leonida can't find leonida.exe any more. Pick it again.".into());
        }
        launcher.starts.claim()?;
    }
    let running = spawn_blocking(leonida::running).await.unwrap_or(false);
    let mut launcher = lock(&state);
    if running {
        launcher.starts.end(false);
        return Err("The game is already running. Close it before you start a test run.".into());
    }
    let exe = launcher.exe();
    let started = trial::start(&trials, warm, &exe, &launcher.settings.game_dir, app);
    launcher.starts.end(started.is_ok());
    started
}

#[tauri::command]
fn test_status(trials: State<'_, trial::Shared>) -> trial::Status {
    trial::status(&trials)
}

#[tauri::command]
fn test_stop(trials: State<'_, trial::Shared>) -> Result<(), String> {
    trial::stop(&trials)
}

#[tauri::command]
fn test_retry(trials: State<'_, trial::Shared>) -> Result<trial::TestView, String> {
    trial::retry(&trials)
}

/// Continues the unfinished test (`keep`) or deletes it.
#[tauri::command]
fn test_resume(
    trials: State<'_, trial::Shared>,
    keep: bool,
) -> Result<Option<trial::TestView>, String> {
    trial::resume(&trials, keep)
}

/// Reads the user's name from Windows, so off the UI thread.
#[tauri::command]
async fn test_where(trials: State<'_, trial::Shared>, text: String) -> Result<String, String> {
    let shared = trials.inner().clone();
    spawn_blocking(move || trial::set_where(&shared, &text))
        .await
        .map_err(stopped)
}

#[tauri::command]
async fn test_report(
    trials: State<'_, trial::Shared>,
    answers: trial::AnswersIn,
) -> Result<trial::ReportView, String> {
    let shared = trials.inner().clone();
    spawn_blocking(move || trial::make_report(&shared, answers))
        .await
        .map_err(stopped)?
}

#[tauri::command]
fn test_again(trials: State<'_, trial::Shared>) -> Result<trial::TestView, String> {
    trial::again(&trials)
}

#[tauri::command]
fn test_cancel(trials: State<'_, trial::Shared>) -> Result<(), String> {
    trial::cancel(&trials)
}

/// Asks the registry and starts Discord or the browser, so off the UI thread.
#[tauri::command]
async fn open_discord() -> Result<(), String> {
    spawn_blocking(trial::open_discord).await.map_err(stopped)?
}

#[tauri::command]
fn open_invite() -> Result<(), String> {
    trial::open_invite()
}

#[tauri::command]
fn open_tests() -> Result<(), String> {
    trial::open_tests()
}

/// Asks GitHub for the newest launcher and Leonida build (at most once an hour unless `now`) and
/// leonida.exe which build it is. Off the UI thread: up to 15 s without network.
#[tauri::command]
async fn check_updates(state: State<'_, Shared>, now: bool) -> Result<View, String> {
    let (saved, exe, look) = {
        let mut launcher = lock(&state);
        let exe = launcher.exe();
        let due = update::due(launcher.updates.saved.checked, update::now(), now);
        let look = launcher.has_exe() && !launcher.updates.knows(&exe);
        if launcher.updates.checking || !(due || look) {
            return Ok(launcher.view());
        }
        launcher.updates.checking = true;
        (due.then(|| launcher.updates.saved.clone()), exe, look)
    };
    let found = spawn_blocking(move || {
        let have = look.then(|| (update::stamp(&exe), update::installed(&exe)));
        (have, saved.map(update::check))
    })
    .await;
    let mut launcher = lock(&state);
    launcher.updates.checking = false;
    let (have, checked) = found.map_err(stopped)?;
    if let Some((of, have)) = have {
        launcher.updates.have = have;
        launcher.updates.have_of = Some(of);
    }
    if let Some((saved, miss)) = checked {
        launcher.updates.saved = saved;
        launcher.updates.miss = miss;
    }
    Ok(launcher.view())
}

/// Downloads the newer launcher, checks it, puts it in place of this one and starts it.
#[tauri::command]
async fn update_launcher(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
    trials: State<'_, trial::Shared>,
) -> Result<String, String> {
    if trial::running(&trials) {
        return Err("A test run is going. Finish it in Test a setting first.".into());
    }
    let offer = {
        let mut launcher = lock(&state);
        if !launcher.updates.busy.is_empty() {
            return Err("An update is downloading already.".into());
        }
        let offer = update::launcher_offer(&launcher.updates.saved.launcher, update::VERSION)
            .ok_or("This launcher is the newest one.")?;
        launcher.updates.busy = "launcher";
        offer
    };
    let page = update::page_or_list(&offer.page, update::LAUNCHER);
    let fetched = spawn_blocking(move || update::fetch_launcher(&offer)).await;
    lock(&state).updates.busy = "";
    match fetched.map_err(stopped)?? {
        update::Fetched::Swapped(exe) => {
            if let Err(error) = Command::new(&exe).spawn() {
                update::undo_swap(&exe);
                return Err(format!(
                    "The new launcher didn't start ({error}), so this one stays. Try again later."
                ));
            }
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(400));
                app.exit(0);
            });
            Ok("Starting the new launcher...".into())
        }
        update::Fetched::ByHand(file) => {
            let _ = Command::new("explorer.exe").arg(&page).spawn();
            Ok(format!(
                "The launcher can't replace itself: Windows doesn't let it write to its own \
                 folder. The new launcher is downloaded and checked as {}. Close this launcher \
                 and copy that file over the old one. The release page is open in your browser \
                 too.",
                file.display()
            ))
        }
    }
}

/// Hides the offer of the newest launcher ("launcher") or Leonida build ("leonida") until a
/// newer one comes out.
#[tauri::command]
fn update_later(state: State<'_, Shared>, what: String) -> Result<View, String> {
    let mut launcher = lock(&state);
    match what.as_str() {
        "launcher" => {
            let offer = update::launcher_offer(&launcher.updates.saved.launcher, update::VERSION);
            launcher.settings.later_launcher = offer.map(|o| o.version).unwrap_or_default();
        }
        "leonida" => {
            let offer = update::build_offer(&launcher.updates.saved.leonida);
            launcher.settings.later_build = offer.map_or(0, |o| o.number);
        }
        _ => return Err(format!("There's no update called {what}.")),
    }
    launcher.save()?;
    Ok(launcher.view())
}

/// Downloads the newest ready-made build, unpacks it under %LOCALAPPDATA%\leonida-launcher and
/// uses it. The leonida.exe used before stays for going back; older builds are deleted.
#[tauri::command]
async fn install_build(
    state: State<'_, Shared>,
    trials: State<'_, trial::Shared>,
) -> Result<View, String> {
    if trial::running(&trials) {
        return Err("A test run is going. Finish it in Test a setting first.".into());
    }
    let (offer, before) = {
        let mut launcher = lock(&state);
        if !launcher.updates.busy.is_empty() {
            return Err("An update is downloading already.".into());
        }
        let offer = update::build_offer(&launcher.updates.saved.leonida).ok_or(
            "There's no ready-made build to download right now. Check for updates under About.",
        )?;
        launcher.updates.busy = "leonida";
        let before = if launcher.has_exe() {
            launcher.exe()
        } else {
            PathBuf::from(&launcher.settings.previous_leonida)
        };
        (offer, before)
    };
    let number = offer.number;
    let installed = spawn_blocking(move || {
        if leonida::running() {
            return Err("Leonida is running. Close the game, then update.".to_string());
        }
        let exe = update::install_build(&offer)?;
        update::tidy_builds(&[&exe, &before]);
        Ok(exe)
    })
    .await;
    let mut launcher = lock(&state);
    launcher.updates.busy = "";
    let exe = installed.map_err(stopped)??;
    if launcher.has_exe() && launcher.exe() != exe {
        launcher.settings.previous_leonida = launcher.settings.leonida_exe.clone();
    }
    launcher.updates.have = update::Have::Build(number);
    launcher.updates.have_of = Some(update::stamp(&exe));
    launcher.updates.warm = true;
    launcher.set_leonida(&exe)?;
    Ok(launcher.view())
}

/// Goes back to the leonida.exe used before the last ready-made build, and keeps that one for
/// going forward again.
#[tauri::command]
fn use_previous(state: State<'_, Shared>) -> Result<View, String> {
    let mut launcher = lock(&state);
    let previous = PathBuf::from(&launcher.settings.previous_leonida);
    if launcher.settings.previous_leonida.is_empty() || !previous.is_file() {
        return Err("The previous leonida.exe isn't there any more.".into());
    }
    launcher.settings.previous_leonida = launcher.settings.leonida_exe.clone();
    launcher.set_leonida(&previous)?;
    Ok(launcher.view())
}

/// Asks first, then opens a window in the player's own checkout that pulls the newest code and
/// builds leonida.exe. An empty answer means the player said no.
#[tauri::command]
async fn update_checkout(
    window: tauri::Window,
    state: State<'_, Shared>,
) -> Result<String, String> {
    let checkout = {
        let launcher = lock(&state);
        if !launcher.has_exe() {
            return Err("Pick leonida.exe first.".into());
        }
        leonida::folder(&launcher.exe())
    };
    spawn_blocking(move || {
        if leonida::running() {
            return Err(
                "Leonida is running. Close the game first: the build can't replace \
                        leonida.exe while it runs."
                    .to_string(),
            );
        }
        update::is_checkout(&checkout)?;
        let answer = rfd::MessageDialog::new()
            .set_parent(&window)
            .set_level(rfd::MessageLevel::Info)
            .set_title("Leonida Launcher")
            .set_description(format!(
                "Update your own build in {}?\n\nA window opens that gets the newest code with \
                 git pull and builds leonida.exe with cargo build --release -p leonida, which \
                 takes a few minutes. If you edited files there, it stops and changes nothing.",
                checkout.display()
            ))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if answer != rfd::MessageDialogResult::Yes {
            return Ok(String::new());
        }
        update::update_checkout(&checkout)?;
        Ok("Updating your checkout in a new window".to_string())
    })
    .await
    .map_err(stopped)?
}

/// Opens the release page of the newest launcher ("launcher") or Leonida build ("leonida").
/// Only this project's pages, never a URL from the window.
#[tauri::command]
fn open_release(state: State<'_, Shared>, which: String) -> Result<(), String> {
    let url = {
        let launcher = lock(&state);
        let saved = &launcher.updates.saved;
        let (known, repository) = match which.as_str() {
            "launcher" => (&saved.launcher, update::LAUNCHER),
            "leonida" => (&saved.leonida, update::LEONIDA),
            _ => return Err(format!("There's no release called {which}.")),
        };
        let page = match known {
            update::Known::Release(release) => release.page.as_str(),
            _ => "",
        };
        update::page_or_list(page, repository)
    };
    Command::new("explorer.exe")
        .arg(&url)
        .spawn()
        .map(drop)
        .map_err(|error| format!("Couldn't open the browser ({error}). The page is {url}"))
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
    fn a_second_start_waits_for_the_first() {
        let mut starts = Starts::default();
        assert!(starts.claim().is_ok());
        assert!(starts.claim().is_err(), "while the first is checked");
        starts.end(false);
        assert!(starts.claim().is_ok(), "after a start that didn't spawn");
        starts.end(true);
        assert!(starts.claim().is_err(), "right after a spawn");
        starts.spawned = Some(Instant::now() - START_GAP);
        assert!(starts.claim().is_ok(), "once the gap is over");
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
