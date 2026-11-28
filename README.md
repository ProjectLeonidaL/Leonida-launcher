# Leonida Launcher

A small Windows window for starting [Leonida](https://github.com/ProjectLeonidaL/leonida) with
the profiles and `LEONIDA_*` switches you want to test, without typing PowerShell.

- **Game folder:** find it automatically (looks through this PC's drives for the folder with
  `MicrosoftGame.config`, up to a minute, same rules as Leonida's `scripts/game-dir.ps1`), or
  pick the folder or the game's .exe yourself.
- **leonida.exe:** found beside the launcher or in a `target\release` under Downloads, or picked.
- **Play / Warm up shaders:** start `leonida.exe run --execute` or `translate --warm all` in a
  new console window. It closes when the run ends well and stays open with the log after an
  error.
- **Picture and speed:** *Recommended* lets Leonida pick the profiles for your graphics card and
  sets nothing else. *My own choices* opens the profiles, the switches and extra `NAME=value`
  lines: ticked profiles go into `LEONIDA_PROFILE`, each switch has a drop-down where `default`
  sets nothing. Settings saved by an older launcher with picks in them start on *My own choices*,
  so nothing you picked is dropped.
- **Run command:** the same run as PowerShell, with a Copy button, to paste into test reports.

The launcher only reads Leonida's files and the game folder; it never changes either. Its own
choices live in `%LOCALAPPDATA%\leonida-launcher\settings.json`.

## Lists from GitHub

The switch list is [`launcher-switches.json`](launcher-switches.json) on this repository's
`main` branch, so a new switch shows up for everyone after a commit here, without a new
launcher build. The profile list is Leonida's `gpu-profiles.json` (the checkout's own copy when
leonida.exe was built from one). Both are downloaded with Windows' `curl.exe`, saved, and the
saved copy is used offline; the list built into the .exe is the last resort.

Each switch may have two optional fields: `title`, a plain name shown above the variable name,
and `risk`, one of `safe`, `experimental` or `may crash` (leave it out when unsure). Older
launchers ignore both.

The game itself never goes online: only the launcher downloads these two lists and the updates
below.

## Updates

The launcher looks on GitHub for news when it opens, and again when you come back to its window
after an hour or more. *Check for updates now* under About asks at once. Nothing is installed
unless you click it, and *Later* hides an update until a newer one comes out.

When both a new launcher and a new Leonida build are out, one row *2 UPDATES ARE OUT* holds both.

- **The launcher itself:** when a newer release of this repository is out, a row *LAUNCHER x.y.z
  IS OUT* shows under START. *Update now* downloads `leonida-launcher.exe`, checks that it is the
  release's own file (by its SHA-256), puts it in place of the old one and starts it. Your
  settings stay. The old launcher is deleted only once the new one's window has opened; if the
  new one doesn't start, the old one comes back. If Windows doesn't let the launcher replace its
  own file (for example under Program Files), it saves the checked file in
  `%LOCALAPPDATA%\leonida-launcher\update` and shows it in Explorer: close the launcher and copy
  that file over the old one. About repeats these steps until it's done.

  **Launcher 0.2.0 has no updater.** If you have it, download the newest `leonida-launcher.exe`
  from the [releases page](https://github.com/ProjectLeonidaL/Leonida-launcher/releases) once
  and put it in place of the old one. Every launcher from then on updates itself.
- **Leonida Legacy:** every change merged into Leonida's `main` is published as a ready-made
  build (*Build N*, a small zip with `leonida.exe`). The launcher asks `leonida.exe version` which
  build you have and compares it with the newest one:
  - No leonida.exe yet: *Download Leonida* gets the newest build. Finding or picking your own
    build still works. Without internet, *Try again* asks GitHub once you're online.
  - An older ready-made build: a row *LEONIDA BUILD N IS OUT* offers the update. The build you
    had stays, so *Go back to build M* on the Leonida row returns to it, and the update isn't
    offered again until a newer one comes out. Older builds are deleted.
  - Your own build: the Leonida row says so and names the newest ready-made build. *Use
    ready-made builds* switches to them. When it was built in a git checkout, *Update my
    checkout* asks first (naming the branch), then opens a window that runs `git pull --ff-only`
    and `cargo build --release -p leonida` there. It stops and changes nothing if you edited
    files, and warns when the checkout isn't on `main`. The window closes by itself when it
    worked and waits for Enter after a problem. Each check also runs `git fetch` in the checkout
    (hidden, never asking for a password), and a row *N NEW COMMITS FOR YOUR CHECKOUT* shows up
    when the branch it pulls from has newer code.

  Builds go to `%LOCALAPPDATA%\leonida-launcher\builds`; nothing is installed in Windows. The
  launcher installs a build only when the release gives its SHA-256 and the zip holds nothing
  outside its `leonida` folder. Your game folder reaches every build the same way, so nothing has
  to be set again. A new build translates the shaders again: warm them up once before you play.

For updates the launcher asks only GitHub's release pages of these two repositories, and sends
nothing about you or your PC. The last answer is kept in `%LOCALAPPDATA%\leonida-launcher\cache`.
GitHub answers 60 questions an hour from one internet address; when that runs out, the launcher
waits an hour before it asks again.

## Build

The window is [Tauri 2](https://tauri.app) with a plain HTML, CSS and JavaScript page in `ui/`.
There is no npm or bundler step; Cargo does everything:

```powershell
cargo build --release
.\target\release\leonida-launcher.exe
```

The page runs in Microsoft Edge WebView2, which is part of Windows 11. On Windows 10, install the
[WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) if the launcher
says it is missing.
