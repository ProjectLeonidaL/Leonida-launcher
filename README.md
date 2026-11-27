# Leonida Launcher

A small Windows window for starting [Leonida](https://github.com/ProjectLeonidaL/leonida) with
the profiles and `LEONIDA_*` switches you want to test, without typing PowerShell.

- **Game folder:** find it automatically (looks through this PC's drives for the folder with
  `MicrosoftGame.config`, up to a minute, same rules as Leonida's `scripts/game-dir.ps1`), or
  pick the folder or the game's .exe yourself.
- **leonida.exe:** found beside the launcher or in a `target\release` under Downloads, or picked.
- **Profiles and switches:** ticked profiles go into `LEONIDA_PROFILE`; each switch has a
  drop-down, `default` sets nothing. Extra `NAME=value` lines cover anything not listed.
- **Play / Warm up shaders:** start `leonida.exe run --execute` or `translate --warm all` in a
  new console window that stays open with the log.
- **Run command:** the same run as PowerShell, to paste into test reports.

The launcher only reads Leonida's files and the game folder; it never changes either. Its own
choices live in `%LOCALAPPDATA%\leonida-launcher\settings.json`.

## Lists from GitHub

The switch list is [`launcher-switches.json`](launcher-switches.json) on this repository's
`main` branch, so a new switch shows up for everyone after a commit here, without a new
launcher build. The profile list is Leonida's `gpu-profiles.json` (the checkout's own copy when
leonida.exe was built from one). Both are downloaded with Windows' `curl.exe`, saved, and the
saved copy is used offline; the list built into the .exe is the last resort.

The game itself never goes online: only the launcher downloads these two lists.

## Build

```powershell
cargo build --release
.\target\release\leonida-launcher.exe
```
