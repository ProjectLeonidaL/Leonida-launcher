# Leonida Launcher

A small Windows window for starting [Leonida](https://github.com/ProjectLeonidaL/leonida) with
the profiles and `LEONIDA_*` switches you want to test, without typing PowerShell.

- **Game folder:** find it automatically (looks through this PC's drives for the folder with
  `MicrosoftGame.config`, up to a minute, same rules as Leonida's `scripts/game-dir.ps1`), or
  pick the folder or the game's .exe yourself.
- **leonida.exe:** found beside the launcher or in a `target\release` under Downloads, or picked.
- **Play / Warm up shaders:** start `leonida.exe run --execute` or `translate --warm all` in a
  new console window that stays open with the log.
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

- **The launcher itself:** when a newer release of this repository is out, a row *LAUNCHER x.y.z
  IS OUT* shows under START. *Update now* downloads `leonida-launcher.exe`, checks it against the
  release's SHA-256, puts it in place of the old one and starts it. Your settings stay. If the
  launcher's folder can't be written to (for example under Program Files), it saves the checked
  file in `%LOCALAPPDATA%\leonida-launcher\update` and opens the release page: close the launcher
  and copy that file over the old one.
- **Leonida Legacy:** every change merged into Leonida's `main` is published as a ready-made
  build (*Build N*, a small zip with `leonida.exe`). The launcher asks `leonida.exe version` which
  build you have and compares it with the newest one:
  - No leonida.exe yet: *Download Leonida* gets the newest build. Finding or picking your own
    build still works.
  - An older ready-made build: a row *LEONIDA BUILD N IS OUT* offers the update. The build you
    had stays, so *Use the previous build* goes back to it; older ones are deleted.
  - Your own build from a checkout: the Leonida row says so and names the newest ready-made
    build. *Use ready-made builds* switches to them; *Update my checkout* asks first, then opens a
    window that runs `git pull --ff-only` and `cargo build --release -p leonida` in your checkout.
    It stops and changes nothing if you edited files there.

  Builds go to `%LOCALAPPDATA%\leonida-launcher\builds`. Your game folder reaches every build the
  same way, so nothing has to be set again. A new build translates the shaders again: warm them
  up once before you play.

For updates the launcher asks only GitHub's release pages of these two repositories, and sends
nothing about you or your PC. The last answer is kept in `%LOCALAPPDATA%\leonida-launcher\cache`.

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
