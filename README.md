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

The game itself never goes online: only the launcher downloads these two lists.

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
