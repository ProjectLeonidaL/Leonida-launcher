"use strict";

const core = window.__TAURI__ && window.__TAURI__.core;
const invoke = (command, args) =>
  core
    ? core.invoke(command, args)
    : Promise.reject("This page needs the launcher behind it. Start leonida-launcher.exe instead.");

const $ = (id) => document.getElementById(id);
const esc = (text) =>
  String(text).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
const plural = (n, one, many) => `${n} ${n === 1 ? one : many}`;
const leaf = (path) => String(path).replace(/[\\/]+$/, "").split(/[\\/]/).pop();
const reduced = matchMedia("(prefers-reduced-motion: reduce)");

/** The last view from the back end: settings, lists and what a run sends. */
let V = null;
/** What only the window knows. */
const ui = {
  hero: null, // "game" or "leonida" while the user is changing one of them
  searching: false,
  found: [],
  problem: "",
  findingExe: false,
  running: false,
  checking: false,
  starting: "", // "play" or "warm" while that is being started
  listsLoading: false,
  tube: "",
  open: {},
  panel: "", // the open tab: "" for Play, "panel", "testPanel" or "about"
  hl: "playTile", // the Play row the highlight bar is on
  startSeen: false, // Esc put the menu back while a start was still being shown
  busy: "", // "check", "launcher", "leonida" or "checkout" while that update step runs
};

const ICON = {
  check: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5l4.2 4L19 7" fill="none" stroke="currentColor" stroke-width="3.4" stroke-linecap="square"/></svg>',
  close: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18" stroke="currentColor" stroke-width="2.6" stroke-linecap="square"/></svg>',
  chev: '<svg class="chev" viewBox="0 0 16 16" aria-hidden="true"><path d="M2.5 5.5 8 11l5.5-5.5" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="square"/></svg>',
  left: '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M10.5 2.5 5 8l5.5 5.5" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="square"/></svg>',
  right: '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5.5 2.5 11 8l-5.5 5.5" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="square"/></svg>',
};

/* ---------- Scene ---------- */

const NS = "http://www.w3.org/2000/svg";
function shape(tag, attrs, parent) {
  const el = document.createElementNS(NS, tag);
  for (const k in attrs) el.setAttribute(k, attrs[k]);
  parent.appendChild(el);
  return el;
}
function seeded(seed) {
  return () => (seed = (seed * 16807) % 2147483647) / 2147483647;
}

function buildScene() {
  const bridge = $("causeway");
  const glints = $("glints");
  const x0 = 520, x1 = 1640;
  const deck = (u) => 462 + 38 * u;
  const thick = (u) => 1.6 + 9 * u;
  // The deck: a thin band that comes closer toward the right.
  const top = [], bottom = [];
  for (let i = 0; i <= 20; i++) {
    const u = i / 20, x = x0 + (x1 - x0) * u;
    top.push(`${x.toFixed(1)},${deck(u).toFixed(1)}`);
    bottom.unshift(`${x.toFixed(1)},${(deck(u) + thick(u)).toFixed(1)}`);
  }
  shape("polygon", { points: top.concat(bottom).join(" "), fill: "#140b20" }, bridge);
  const N = 46;
  for (let i = 0; i <= N; i++) {
    const u = Math.pow(i / N, 1.55);
    const x = x0 + (x1 - x0) * u;
    const y = deck(u) + thick(u);
    const h = 4 + 20 * u;
    shape("rect", { x: x.toFixed(1), y: y.toFixed(1), width: (1 + 3.4 * u).toFixed(1), height: h.toFixed(1), fill: "#140b20" }, bridge);
    if (i % 3 === 1) {
      const pole = 7 + 30 * u;
      const py = deck(u) - pole;
      shape("rect", { x: x.toFixed(1), y: py.toFixed(1), width: (0.7 + 1.6 * u).toFixed(1), height: pole.toFixed(1), fill: "#140b20" }, bridge);
      shape("circle", { cx: (x + 1).toFixed(1), cy: py.toFixed(1), r: (1 + 2.2 * u).toFixed(1), fill: "#ffd890" }, bridge);
      shape("rect", {
        x: x.toFixed(1), y: (y + h + 2).toFixed(1), width: (0.8 + 2 * u).toFixed(1),
        height: (8 + 46 * u).toFixed(1), opacity: (0.16 + 0.18 * u).toFixed(2),
      }, glints);
    }
  }
  // The sun's path on the water.
  const rnd = seeded(7);
  for (let k = 0; k < 22; k++) {
    const y = 474 + k * 4 + k * k * 0.55;
    const w = Math.max(10, 150 - k * 5.5) * (0.6 + rnd() * 0.5);
    const x = 1220 - w / 2 + (rnd() - 0.5) * 30;
    shape("rect", {
      x: x.toFixed(1), y: y.toFixed(1), width: w.toFixed(1), height: (1.4 + k * 0.22).toFixed(1),
      opacity: Math.max(0.05, 0.55 - k * 0.024).toFixed(2),
    }, glints);
  }
  const palms = $("palms");
  palm(palms, { bx: 1640, by: 920, cx: 1585, cy: 330, lean: -50, base: 22, top: 9, size: 0.9 });
  palm(palms, { bx: 1720, by: 920, cx: 1680, cy: 430, lean: 30, base: 18, top: 8, size: 0.7 });
  palm(palms, { bx: 1196, by: 492, cx: 1182, cy: 404, lean: 6, base: 4, top: 2.2, size: 0.3 });
  palm(palms, { bx: 1236, by: 494, cx: 1246, cy: 420, lean: -4, base: 3.4, top: 2, size: 0.24 });
  palm(palms, { bx: -60, by: 920, cx: 70, cy: 380, lean: 50, base: 22, top: 9, size: 0.95 });
}

function palm(g, p) {
  const mx = (p.bx + p.cx) / 2 + p.lean, my = (p.by + p.cy) / 2;
  shape("path", {
    d: `M${p.bx - p.base} ${p.by}Q${mx - p.base * 0.6} ${my} ${p.cx - p.top} ${p.cy}` +
       `L${p.cx + p.top} ${p.cy}Q${mx + p.base * 0.6} ${my} ${p.bx + p.base} ${p.by}Z`,
  }, g);
  const fronds = [
    [-168, 200, 70], [-140, 215, 50], [-114, 190, 40], [-88, 150, 30], [-62, 185, 40],
    [-34, 215, 60], [-8, 210, 80], [18, 170, 90], [160, 175, 90], [192, 160, 80],
  ];
  for (const [deg, len, droop] of fronds) {
    frond(g, p.cx, p.cy, (deg * Math.PI) / 180, len * p.size, droop * p.size, 26 * p.size);
  }
  shape("ellipse", { cx: p.cx, cy: p.cy + 4 * p.size, rx: 16 * p.size, ry: 12 * p.size }, g);
}

function frond(g, cx, cy, ang, len, droop, width) {
  const dx = Math.cos(ang), dy = Math.sin(ang);
  const N = 26, upper = [], lower = [];
  for (let i = 0; i <= N; i++) {
    const t = i / N;
    const px = cx + dx * len * t, py = cy + dy * len * t + droop * t * t;
    let tx = dx * len, ty = dy * len + 2 * droop * t;
    const tl = Math.hypot(tx, ty);
    tx /= tl;
    ty /= tl;
    const nx = -ty, ny = tx;
    const w = width * Math.pow(Math.sin(Math.PI * Math.min(t * 1.08, 1)), 0.7) * (1 - 0.3 * t);
    const tooth = i % 2 ? 1 : 0.35;
    const back = w * 0.55 * tooth;
    upper.push([px + nx * w * tooth - tx * back, py + ny * w * tooth - ty * back]);
    lower.push([px - nx * w * tooth * 0.85 - tx * back, py - ny * w * tooth * 0.85 - ty * back + w * 0.25 * tooth]);
  }
  const pts = upper.concat(lower.reverse()).map(([x, y]) => `${x.toFixed(1)},${y.toFixed(1)}`);
  shape("polygon", { points: `${cx},${cy} ` + pts.join(" ") }, g);
}

/* ---------- State ---------- */

function blocker() {
  if (!V) return null;
  if (!V.has_game) {
    return { tile: "gameTile", text: "Leonida needs the folder you installed the game to.", fix: "Find my game", act: "search" };
  }
  if (!V.has_exe) {
    if (V.updates.game === "download") {
      return { tile: "leonidaTile", text: "Leonida Legacy isn't on this PC yet.", fix: downloadLabel(), act: "installBuild" };
    }
    return { tile: "leonidaTile", text: "Leonida Legacy isn't built yet, or the launcher can't find it.", fix: "Find leonida.exe", act: "findExe" };
  }
  if (testing()) {
    return { tile: null, label: "A test is running.", text: "Finish the run in Test a setting first." };
  }
  if (ui.running) {
    return { tile: null, label: "Already running.", text: "Close the game before you start it again." };
  }
  return null;
}

/* ---------- Updates ---------- */

const U = () => V.updates;
/** A newer launcher the player hasn't answered Later to. */
const launcherNews = () => !!(V && U().launcher_new && !U().launcher_later);
/** A newer ready-made build than the one in use, not answered Later to. */
const buildNews = () => !!(V && U().game === "behind" && !U().game_later);
const downloadLabel = () => `Download Leonida (build ${U().newest}, ${U().newest_size})`;

/** "Today at 21:40" or "26 Nov at 09:12" for a time in seconds since 1970. */
function when(secs) {
  const d = new Date(secs * 1000);
  const time = d.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
  if (d.toDateString() === new Date().toDateString()) return `Today at ${time}`;
  return `${d.toLocaleDateString("en-GB", { day: "numeric", month: "short" })} at ${time}`;
}

/** What a check found, in a sentence or two, for the toast after Check for updates now. */
function updateSummary() {
  const u = U();
  if (u.note) return u.note;
  const launcher = u.launcher_new ? `Launcher ${u.launcher_new} is out.` : "The launcher is up to date.";
  const game = {
    behind: `Leonida build ${u.newest} is out.`,
    download: `Leonida build ${u.newest} is ready to download.`,
    current: `You have build ${u.newest}, the newest Leonida build.`,
    own: `The newest ready-made Leonida build is ${u.newest}.`,
    old: `The newest ready-made Leonida build is ${u.newest}.`,
    none: "No ready-made Leonida builds are published yet.",
  }[u.game] || "";
  return `${launcher} ${game}`.trim();
}

/** The ways to get a newer Leonida for the build in use, as buttons for the detail pane. */
function buildActions(withPick) {
  const u = U();
  const busy = ui.busy ? " disabled" : "";
  const getting = ui.busy === "leonida" ? "Downloading..." : "";
  let html = "";
  if (u.game === "behind") html += btn("installBuild", getting || `Update to build ${u.newest}`, "warm", busy);
  if (u.game === "own" || u.game === "old") {
    html += btn("installBuild", getting || "Use ready-made builds", "warm", busy) +
      btn("updateCheckout", ui.busy === "checkout" ? "Asking..." : "Update my checkout", "ghost", busy);
  }
  if (withPick) html += btn("pickLeonida", "Pick another leonida.exe");
  if (u.previous) html += link("usePrevious", `Use the previous build (${esc(u.previous)})`);
  return html;
}

/** The update group under About: versions, what is out, and the last check. */
function renderAbout() {
  if (!V) return;
  const u = U();
  const a = (act, text) => `<button class="link-btn" type="button" data-act="${act}" data-key="about-${act}">${text}</button>`;
  let launcher = esc(u.launcher);
  if (u.launcher_new) {
    launcher += `. Version ${esc(u.launcher_new)} is out: ` +
      a("updateLauncher", ui.busy === "launcher" ? "Updating..." : "Update now") + " " + a("launcherPage", "What's new");
  } else if (u.checked && !u.note) {
    launcher += ", the newest";
  }
  setHTML($("aboutLauncher"), launcher);

  let have = esc(V.has_exe ? u.have : "None yet");
  if (u.game === "behind") have += `. Build ${u.newest} is out: ` + a("installBuild", ui.busy === "leonida" ? "Downloading..." : "Update");
  if (u.game === "own" || u.game === "old") {
    have += ". " + a("installBuild", "Use ready-made builds") + " or " + a("updateCheckout", "Update my checkout");
  }
  if (u.previous) have += `<span class="arow-note">${a("usePrevious", `Use the previous build (${esc(u.previous)})`)}</span>`;
  if (u.warm) have += `<span class="arow-note">New build: warm up the shaders once before you play.</span>`;
  setHTML($("aboutHave"), have);

  const ready = u.game === "none" ? "No ready-made builds published yet"
    : u.newest ? `Build ${u.newest} is the newest, ${esc(u.newest_size)}. ` + a("buildPage", "What's new")
    : u.checking ? "Checking..." : "Not known yet. Check below.";
  setHTML($("aboutReady"), ready);

  const checking = u.checking || ui.busy === "check";
  setHTML($("aboutChecked"),
    (checking ? "Checking GitHub..." : u.checked ? esc(when(u.checked)) : "Not yet") +
    ". " + a("checkNow", "Check for updates now") +
    (u.note ? `<span class="arow-note">${esc(u.note)}</span>` : ""));
}

function heroState() {
  if (!V) return "loading";
  if (ui.hero === "leonida") return "leonida";
  if (ui.hero === "game") return ui.found.length ? "pick" : "game";
  // An update row explains itself in the pane while the highlight bar is on it.
  if ((ui.hl === "launcherNews" && launcherNews()) || (ui.hl === "buildNews" && buildNews())) return ui.hl;
  if (!V.has_game) return ui.found.length ? "pick" : "game";
  if (!V.has_exe) return "leonida";
  if (ui.running) return "running";
  // Otherwise the detail pane explains the row the highlight bar is on, as a pause menu does.
  const about = { gameTile: "gameInfo", leonidaTile: "exeInfo", warmTile: "warm", modeTile: "mode", testTile: "test" }[ui.hl];
  return about || "ready";
}

/** Replaces an element's markup only when it changed, so focus and hover stay put. */
function setHTML(el, html) {
  if (el.dataset.html === html) return;
  el.innerHTML = html;
  el.dataset.html = html;
}

/* ---------- Home ---------- */

/** The status line in the top bar: the time, the settings in use, and whether START works. */
function renderTube() {
  const tube = $("tube");
  const b = blocker();
  const [state, word, label] = !V ? ["", "CHECKING", "Checking what's there"]
    : !V.has_game ? ["warn", "GAME FOLDER MISSING", "Not ready: game folder missing"]
    : !V.has_exe ? ["warn", "LEONIDA.EXE MISSING", "Not ready: leonida.exe missing"]
    : testing() ? ["busy", "TEST RUNNING", "A test is running"]
    : ui.running ? ["busy", "RUNNING", "The game is running"]
    : b ? ["busy", "BUSY", b.label] : ["ready", "READY", "Ready to play"];
  tube.dataset.state = state;
  tube.setAttribute("aria-label", label);
  $("tubeWord").textContent = word;
  $("hdrMode").textContent = !V ? "" : V.advanced
    ? `MY OWN CHOICES${V.changes ? `, ${V.changes} CHANGED` : ""}` : "RECOMMENDED";
  renderClock();
}

function renderClock() {
  const now = new Date();
  const day = now.toLocaleDateString("en-GB", { weekday: "short" });
  const time = now.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
  $("clock").textContent = `${day} ${time}`;
}

/** The bottom bar: the switch list's source on the left, the keys on the right. */
function renderBar() {
  const busy = !V || ui.listsLoading;
  const kind = !V ? "" : { github: "ok", saved: "warn" }[V.source] || "off";
  setHTML($("barSource"), busy
    ? `<span class="ring" aria-hidden="true"></span><span class="bar-text">${!V ? "Getting ready" : "Checking GitHub for new switches"}</span>`
    : `<span class="sq ${kind}" aria-hidden="true"></span><span class="bar-text">${esc(V.source_label)}</span>`);
  $("barSource").title = V ? V.source_detail : "";
  const lines = {
    play: "Starting the game. It opens in its own window, with its log beside it.",
    warm: "Warming up the shaders. The game opens in its own window and closes by itself.",
  };
  $("startLine").textContent = lines[ui.starting] || "";
  document.body.classList.toggle("starting", !!ui.starting && !ui.startSeen);
  renderHints();
}

/** Which keys do something here, and what Enter does on the focused thing. */
function renderHints() {
  const el = document.activeElement;
  const onValue = !!(el && el.closest && el.closest(".dd"));
  $("hintChange").hidden = !onValue;
  $("hintMove").hidden = ui.panel !== "";
  $("hintSelectWord").textContent = el && el.id === "playBtn" ? "Start" : onValue ? "List" : "Select";
  $("hintBack").classList.toggle("idle", !ui.panel && !ui.hero && !dd.button);
}

function btn(act, text, kind = "ghost", extra = "") {
  return `<button class="btn btn-${kind}" type="button" data-act="${act}" data-key="${act}"${extra}>${text}</button>`;
}
function link(act, text, extra = "") {
  return `<button class="link-btn" type="button" data-act="${act}" data-key="${act}"${extra}>${text}</button>`;
}

function renderHero() {
  const hero = $("hero");
  const state = heroState();
  let title = "", text = "", textHtml = "", actions = "", list = "", facts = [];
  if (state === "loading") {
    title = "Getting ready";
    text = "Checking for your game and Leonida Legacy.";
  } else if (state === "game" || state === "pick") {
    const busy = ui.searching ? " disabled" : "";
    if (state === "pick") {
      title = "Which copy is yours?";
      text = `Found ${plural(ui.found.length, "folder", "folders")} with MicrosoftGame.config inside. Pick the one you play.`;
      list = ui.found.map((g) => {
        const current = V.game_dir === g.dir ? ' aria-current="true"' : "";
        return `<button class="found-item" type="button" data-act="choose" data-dir="${esc(g.dir)}" data-key="choose:${esc(g.dir)}"${current}><strong>${esc(g.name)}</strong><span>${esc(g.dir)}</span></button>`;
      }).join("");
    } else {
      title = V.has_game ? "Change the game folder" : "Leonida can't find your game yet";
      text = ui.searching
        ? "Looking through your drives for a folder with MicrosoftGame.config inside. This can take up to a minute."
        : "Pick the folder you installed the game to. It's the one with MicrosoftGame.config inside.";
    }
    actions =
      btn("search", ui.searching ? "Looking in your drives..." : state === "pick" ? "Look again" : "Find it automatically", state === "pick" ? "ghost" : "warm", busy) +
      btn("pickFolder", "Pick the folder") +
      link("pickExe", "Pick the game's .exe instead") +
      (V.has_game ? link("keep", "Keep the current folder") : "");
  } else if (state === "leonida" && !V.has_exe && U().game === "download") {
    title = "Get Leonida Legacy";
    text = "Download the newest ready-made build from GitHub. It needs nothing else installed and goes into the " +
      "launcher's own folder. If you build Leonida Legacy yourself, find or pick your leonida.exe instead.";
    actions =
      btn("installBuild", ui.busy === "leonida" ? "Downloading..." : downloadLabel(), "warm", ui.busy ? " disabled" : "") +
      btn("findExe", ui.findingExe ? "Looking..." : "Find my own build", "ghost", ui.findingExe ? " disabled" : "") +
      btn("pickLeonida", "Pick leonida.exe") +
      link("buildSteps", "Open the build steps") +
      (U().previous ? link("usePrevious", `Use the previous build (${esc(U().previous)})`) : "");
  } else if (state === "leonida") {
    title = V.has_exe ? "Change leonida.exe" : "Find Leonida Legacy";
    textHtml = "The launcher needs leonida.exe, which you get by building Leonida Legacy once. " +
      "The build steps walk you through it; the last one is <code>cargo build --release -p leonida</code> in its folder.";
    actions =
      btn("findExe", ui.findingExe ? "Looking..." : "Find it automatically", "warm", ui.findingExe ? " disabled" : "") +
      btn("pickLeonida", "Pick leonida.exe") +
      link("buildSteps", "Open the build steps") +
      (V.has_exe ? link("keep", "Keep the current one") : "");
    if (V.has_exe) actions += buildActions(false);
  } else if (state === "launcherNews") {
    const u = U();
    title = `Launcher ${u.launcher_new} is out`;
    text = `You have ${u.launcher}. Update now downloads the new launcher from GitHub, checks it against its ` +
      "checksum, puts it in place of this one and starts it again. Your settings stay. Later hides this until a newer one comes out.";
    actions =
      btn("updateLauncher", ui.busy === "launcher" ? "Updating..." : "Update now", "warm", ui.busy ? " disabled" : "") +
      btn("laterLauncher", "Later") +
      link("launcherPage", "What's new");
    facts = [["You have", u.launcher], ["Newest", u.launcher_new]];
  } else if (state === "buildNews") {
    const u = U();
    title = `Leonida build ${u.newest} is out`;
    text = `You have build ${u.have_build}. Newer builds bring fixes, often for speed too. Update downloads build ` +
      `${u.newest} (${u.newest_size}) from GitHub and switches to it. Build ${u.have_build} stays, so you can go back. ` +
      "Later hides this until a newer build comes out.";
    actions =
      btn("installBuild", ui.busy === "leonida" ? "Downloading..." : "Update", "warm", ui.busy ? " disabled" : "") +
      btn("laterBuild", "Later") +
      link("buildPage", "What's new");
    facts = [["You have", `Build ${u.have_build}`], ["Newest", `Build ${u.newest}`]];
  } else if (state === "gameInfo") {
    title = "Game folder";
    text = "The folder you installed the game to, with MicrosoftGame.config inside. Press Enter to change it.";
    facts = [["Game", V.game_name || leaf(V.game_dir)], ["Folder", V.game_dir]];
  } else if (state === "exeInfo") {
    const u = U();
    title = "Leonida Legacy";
    text = {
      behind: `Build ${u.have_build}. Build ${u.newest} is out. START runs the build you have until you update.`,
      own: `Your own build, from your checkout. The newest ready-made build is ${u.newest}. Switch to the ready-made ` +
        "builds, or update your checkout to the newest code.",
      old: `A build from before build numbers. The newest ready-made build is ${u.newest}. Switch to the ready-made ` +
        "builds, or update your checkout to the newest code.",
      current: `Build ${u.have_build}, the newest. START runs it with your settings.`,
    }[u.game] || "The leonida.exe START runs with your settings. Press Enter to pick another one.";
    facts = [["Build", u.have], ["Program", V.leonida_exe], ["Folder", V.checkout]];
    actions = buildActions(true);
  } else if (state === "warm") {
    title = "Warm up shaders";
    text = (U().warm ? "You switched to a new build: warm up once before you play, as its shaders are translated again. " : "") +
      "Translates the game's shaders ahead of time, so play doesn't stutter while that happens. " +
      "Do it once before your first game, after each new Leonida build and after changing a shader switch.";
    facts = [["Takes", "A few minutes, in a new window"], ["Now", warmBlock() || "Press Enter to start it"]];
  } else if (state === "mode") {
    title = "Picture and speed";
    text = V.advanced
      ? "You're using your own choices: profiles and switches you picked. Press Enter to change them."
      : "Leonida picks the settings for your graphics card. Nothing to set. Press Enter to pick your own.";
    facts = [["In use", V.advanced ? `My own choices${V.changes ? `, ${V.changes} changed` : ""}` : "Recommended"], ["START uses", V.uses]];
  } else if (state === "test") {
    title = "Test a setting";
    text = "Try one setting and see what it does. The launcher starts the game, measures, and writes a short report " +
      "you can share. You read it before anything is shared.";
  } else if (state === "running") {
    title = "The game is running";
    text = "Close it before you start it again. Settings stay as they are.";
  } else {
    title = "Start the game";
    text = V.advanced
      ? "You're using your own choices. Change them under Settings."
      : "Leonida picks the settings for your graphics card.";
    facts = [["Game", V.game_name || leaf(V.game_dir)], ["Leonida", `In ${leaf(V.checkout)}`], ["Uses", V.uses]];
  }
  const problem = ui.problem ? `<p class="problem" role="alert">${esc(ui.problem)}</p>` : "";
  const dl = facts.length
    ? `<dl class="facts">${facts.map(([k, v]) => `<dt>${esc(k)}</dt><dd>${esc(v)}</dd>`).join("")}</dl>`
    : "";
  setHTML(hero,
    `<h2>${esc(title)}</h2><p class="lead">${textHtml || esc(text)}</p>` +
    (actions ? `<div class="actions">${actions}</div>` : "") +
    problem +
    (list ? `<div class="found">${list}</div>` : "") + dl);
}

function renderPlay() {
  const b = blocker();
  const tile = $("playTile");
  tile.classList.toggle("blocked", !V || !!b);
  $("playBtn").setAttribute("aria-disabled", String(!V || !!b));
  // The fix for a missing piece is in the detail pane beside this row, and Enter here moves the
  // highlight to the missing row.
  let foot;
  if (!V) {
    foot = "Checking...";
  } else if (b && b.tile) {
    foot = `Can't start yet: ${b.tile === "gameTile" ? "game folder missing" : "leonida.exe missing"}`;
  } else if (b) {
    foot = `${b.label} ${b.text}`;
  } else {
    foot = ui.starting === "play" ? "Starting..." : `Uses: ${V.uses}`;
  }
  setHTML($("playFoot"), esc(foot));
}

/** One row of the Play list: a label with a short line under it, and a value on the right. */
function menuRow(id, label, line, value, kind = "") {
  setHTML($(id),
    `<span class="mrow-text"><span class="label">${esc(label)}</span><span class="mrow-line">${esc(line)}</span></span>` +
    `<span class="mrow-val ${kind}">${esc(value)}</span>`);
}

function stepTile(id, n, title, found, line, tip) {
  const el = $(id);
  el.classList.toggle("needs", !found);
  el.title = tip;
  menuRow(id, title, line, found ? "Found" : "Missing", found ? "ok" : "warn");
}

function warmBlock() {
  if (!V) return "Checking...";
  if (!V.has_game || !V.has_exe) return "Needs the game folder and leonida.exe first.";
  if (testing()) return "Not while a test runs.";
  if (ui.running) return "Not while the game is running.";
  return "";
}

function renderTiles() {
  if (!V) return;
  const gameLine = V.has_game
    ? V.game_name || leaf(V.game_dir)
    : V.game_dir ? "No MicrosoftGame.config there" : "Not found yet";
  stepTile("gameTile", 1, "Game folder", V.has_game, gameLine, V.game_dir || "Pick the folder you installed the game to");
  const u = U();
  const exeLine = {
    behind: `Build ${u.have_build}. Build ${u.newest} is out`,
    own: `Your own build. Newest ready-made: ${u.newest}`,
    old: `An older build without a number. Newest ready-made: ${u.newest}`,
    current: `Build ${u.have_build}, the newest`,
  }[u.game] || `In ${leaf(V.checkout)}`;
  stepTile("leonidaTile", 2, "Leonida", V.has_exe,
    V.has_exe ? exeLine : u.game === "download" ? `Not on this PC yet. Build ${u.newest} is ready to download` : "leonida.exe not found",
    V.leonida_exe || "Find or pick leonida.exe");

  const launcherRow = $("launcherNews");
  launcherRow.hidden = !launcherNews();
  if (!launcherRow.hidden) {
    menuRow("launcherNews", `LAUNCHER ${u.launcher_new} IS OUT`, `You have ${u.launcher}. Updating takes a few seconds.`,
      ui.busy === "launcher" ? "Updating..." : "Update", "warn");
  }
  const buildRow = $("buildNews");
  buildRow.hidden = !buildNews();
  if (!buildRow.hidden) {
    menuRow("buildNews", `LEONIDA BUILD ${u.newest} IS OUT`, `You have build ${u.have_build}. Download ${u.newest_size}.`,
      ui.busy === "leonida" ? "Downloading..." : "Update", "warn");
  }

  const warm = $("warmTile");
  const blocked = warmBlock();
  warm.setAttribute("aria-disabled", String(!!blocked));
  warm.setAttribute("aria-busy", String(ui.starting === "warm"));
  const note = ui.starting === "warm" ? "Starting the warm-up..." : blocked ||
    (u.warm ? "New build: warm up once before you play." : "Once before your first game. Takes a few minutes.");
  warm.classList.toggle("off", !!blocked);
  warm.classList.toggle("changed", u.warm && !blocked);
  menuRow("warmTile", "Warm up shaders", note, blocked ? "Not now" : "Start");

  const own = V.advanced;
  $("modeTile").classList.toggle("changed", own);
  menuRow("modeTile", "Picture and speed", own ? "Profiles and switches you picked" : "Leonida picks for your card",
    own ? `Own${V.changes ? `, ${V.changes} changed` : ""}` : "Recommended");
}

/* ---------- Panel ---------- */

const shown = (value) => ({ "": "Default", 0: "Off (0)", 1: "On (1)" })[value] || value;
const RISK = { safe: ["safe", "Safe"], experimental: ["experimental", "Experimental"], "may crash": ["crash", "May crash"] };

/** Default names Leonida's own value when the list knows it, which then isn't offered twice. */
function switchRow(s, withAbout) {
  const value = V.values[s.name] || "";
  const own = s.values.filter((v) => v !== s.default || v === value);
  const values = own.includes(value) || !value ? own : own.concat(value);
  const plain = { 0: "off", 1: "on" }[s.default] || s.default;
  const options = [{ value: "", label: s.default ? `Default (${plain})` : "Default" }]
    .concat(values.map((v) => ({ value: v, label: shown(v) })));
  const risk = RISK[s.risk] ? `<span class="tag ${RISK[s.risk][0]}">${RISK[s.risk][1]}</span>` : "";
  const build = s.build ? `<span class="tag">needs ${esc(s.build)}</span>` : "";
  const id = `switch-${s.name}`;
  return `<div class="row${value ? " changed" : ""}">` +
    `<label class="row-title" id="${esc(id)}-label" for="${esc(id)}">${esc(s.title || s.name)}</label>` +
    `<div class="row-name"><span><span class="prefix">LEONIDA_</span>${esc(s.name)}</span>${risk}${build}</div>` +
    (withAbout && s.about ? `<p class="row-about">${esc(s.about)}</p>` : "") +
    `<div class="row-ctl">${dropdown(id, options, value, ` data-switch="${esc(s.name)}" aria-labelledby="${esc(id)}-label"`)}</div></div>`;
}

/** Rows, with an about text that several switches share written once above them. */
function switchRows(list) {
  const count = {};
  for (const s of list) count[s.about] = (count[s.about] || 0) + 1;
  const done = new Set();
  let html = "";
  for (const s of list) {
    if (!s.about || count[s.about] < 2) {
      html += switchRow(s, true);
    } else if (!done.has(s.about)) {
      done.add(s.about);
      const rows = list.filter((x) => x.about === s.about).map((x) => switchRow(x, false)).join("");
      html += `<div class="cluster"><p class="cluster-note">${esc(s.about)}</p>${rows}</div>`;
    }
  }
  return html;
}

function chip(p, picked) {
  const on = picked.has(p.name);
  const measured = p.measured ? ` data-measured="${esc(p.measured)}" title="${esc(p.measured)}"` : "";
  return `<button type="button" class="pchip" data-profile="${esc(p.name)}" data-key="p:${esc(p.name)}" aria-pressed="${on}"${measured}>` +
    `<span class="box">${ICON.check}</span>${esc(p.name)}</button>`;
}

function renderProfiles() {
  const picked = new Set(V.picked);
  const ticked = V.profiles.filter((p) => picked.has(p.name)).length;
  $("clearProfiles").hidden = ticked === 0;
  if (!V.profiles.length) {
    $("profilesHint").textContent = "The launcher has no profile list to show: there's no Leonida Legacy folder beside leonida.exe. " +
      (V.profiles_detail ? `${V.profiles_detail} ` : "") +
      "You can still name one under Extra lines as LEONIDA_PROFILE=name.";
    setHTML($("profileGroups"), "");
    return;
  }
  $("profilesHint").textContent = ticked
    ? "Your ticks replace the card's picks. Tick those too if you want to keep them."
    : "Sets of switches measured on real cards. Leonida already turns on the ones that match your card. Tick one to choose yourself.";
  const groups = [
    ["Picked by your card", (p) => !p.local && !p.opt_in, "Leonida turns these on by itself when they match your graphics card."],
    ["Opt-in", (p) => !p.local && p.opt_in, "Off unless you tick them. Point at one to see what was measured."],
  ];
  let html = "";
  for (const [name, keep, note] of groups) {
    const list = V.profiles.filter(keep);
    if (!list.length) continue;
    html += `<div class="pgroup"><p class="pgroup-name">${name}</p><div class="pchips">${list.map((p) => chip(p, picked)).join("")}</div>` +
      `<p class="pnote" data-note="${esc(note)}">${esc(note)}</p></div>`;
  }
  const own = V.profiles.filter((p) => p.local);
  if (own.length) {
    const ownTicked = own.filter((p) => picked.has(p.name)).length;
    const open = ui.open.own ?? ownTicked > 0;
    html += `<details class="sub" data-open="own"${open ? " open" : ""}><summary>Your own (${own.length})${ownTicked ? `, ${ownTicked} ticked` : ""}${ICON.chev}</summary>` +
      `<p class="muted">From gpu-profiles.local.json in your Leonida Legacy folder.</p>` +
      `<div class="pchips">${own.map((p) => chip(p, picked)).join("")}</div></details>`;
  }
  setHTML($("profileGroups"), html);
}

function tint(command) {
  return command.split("\n").map((line) => {
    let m;
    if ((m = line.match(/^(\$env:[\w]+) = (.*)$/))) return `<span class="kw">${esc(m[1])}</span> = <span class="str">${esc(m[2])}</span>`;
    if ((m = line.match(/^Set-Location -LiteralPath (.*)$/))) return `<span class="kw">Set-Location</span> -LiteralPath <span class="str">${esc(m[1])}</span>`;
    if ((m = line.match(/^& ('(?:[^']|'')*')(.*)$/))) return `<span class="kw">&amp;</span> <span class="str">${esc(m[1])}</span>${esc(m[2])}`;
    return esc(line);
  }).join("\n");
}

function renderPanel() {
  if (!V) return;
  document.querySelector(`input[name=mode][value=${V.advanced ? "advanced" : "recommended"}]`).checked = true;
  $("advanced").classList.toggle("recommended", !V.advanced);
  $("unused").hidden = V.advanced;
  $("advCount").textContent = V.changes ? `${V.changes} changed` : "";
  renderProfiles();

  const source = $("source");
  source.title = V.source_detail;
  source.querySelector(".dot").className = `dot ${{ github: "ok", saved: "saved" }[V.source] || ""}`;
  $("sourceLabel").textContent = ui.listsLoading ? "Checking GitHub..." : V.source_label;
  $("refresh").disabled = ui.listsLoading;

  setHTML($("mainSwitches"), switchRows(V.switches.filter((s) => !s.build)));
  const tests = V.switches.filter((s) => s.build);
  $("testBuilds").hidden = !tests.length;
  const testChanged = tests.filter((s) => V.values[s.name]).length;
  $("testBuildsTitle").textContent = `Test builds (${tests.length})${testChanged ? `, ${testChanged} changed` : ""}`;
  if (ui.open.tests === undefined && testChanged) $("testBuilds").open = true;
  setHTML($("testSwitches"), switchRows(tests));

  if (document.activeElement !== $("extra")) $("extra").value = V.extra;
  setHTML($("runCmd"), tint(V.command));
}

function render() {
  const active = document.activeElement;
  const key = active && active.dataset ? active.dataset.key : null;
  renderTube();
  renderPlay();
  renderTiles();
  // A row that went away (an update answered Later, or done) hands the highlight back to START.
  if ($(ui.hl).hidden) ui.hl = "playTile";
  renderHero();
  renderPanel();
  renderAbout();
  const moved = renderTest();
  renderBar();
  for (const row of document.querySelectorAll("#tiles .mrow")) row.classList.toggle("hl", row.id === ui.hl);
  if (dd.button && !document.contains(dd.button)) closeList(false, false);
  document.body.classList.toggle("still", document.hidden || ui.running || testing());
  if (key && !document.contains(active)) {
    const again = document.querySelector(`[data-key="${CSS.escape(key)}"]`);
    if (again) again.focus({ preventScroll: true });
  }
  if (moved && ui.panel === "testPanel") {
    $("testBody").scrollTop = 0;
    $("testBody").focus({ preventScroll: true });
  }
}

/* ---------- Toasts ---------- */

function toast(text, kind = "info") {
  const box = $("toasts");
  const el = document.createElement("div");
  // "keep" is an info toast that stays until it is closed, for steps the player must follow.
  el.className = `toast ${kind === "keep" ? "info" : kind}`;
  if (kind === "problem") el.setAttribute("role", "alert");
  el.innerHTML = `<p>${esc(text)}</p><button type="button" aria-label="Close">${ICON.close}</button>`;
  box.prepend(el);
  while (box.children.length > 2) box.lastElementChild.remove();
  let timer = 0;
  const close = () => {
    clearTimeout(timer);
    el.classList.add("out");
    setTimeout(() => el.remove(), reduced.matches ? 0 : 180);
  };
  el.querySelector("button").addEventListener("click", close);
  if (kind === "problem" || kind === "keep") return;
  let left = 4000, since = Date.now();
  timer = setTimeout(close, left);
  el.addEventListener("mouseenter", () => {
    clearTimeout(timer);
    left -= Date.now() - since;
  });
  el.addEventListener("mouseleave", () => {
    since = Date.now();
    timer = setTimeout(close, Math.max(left, 1000));
  });
}

const problem = (error) => toast(String(error), "problem");

/* ---------- Drop-downs ---------- */

// A select-only combobox: the button keeps focus, the one list shows the options of the open one.
/** The options of each drop-down on screen, by the button's id: [{ value, label }]. */
const DD = {};
const dd = { button: null, active: -1, typed: "", typedAt: 0 };

/** A value between left and right arrows. The arrows step through the values and wrap; the value
 *  itself opens the full list. */
function dropdown(id, options, value, attrs = "") {
  DD[id] = options;
  const current = options.find((o) => o.value === value) || options[0];
  const arrow = (step, name, icon) =>
    `<button type="button" class="arr" tabindex="-1" data-step="${step}" data-for="${esc(id)}" aria-label="${name}">${icon}</button>`;
  return `<span class="sel">${arrow(-1, "Previous value", ICON.left)}` +
    `<button type="button" class="dd" id="${esc(id)}" role="combobox" aria-haspopup="listbox" aria-expanded="false"` +
    ` aria-controls="ddList" data-key="${esc(id)}" data-value="${esc(current.value)}"${attrs}>` +
    `<span class="dd-value">${esc(current.label)}</span></button>${arrow(1, "Next value", ICON.right)}</span>`;
}

/** Steps the selector `button` by `d` values, wrapping at the ends. */
function stepValue(button, d) {
  const options = DD[button.id];
  if (!options || options.length < 2) return;
  const i = Math.max(options.findIndex((o) => o.value === button.dataset.value), 0);
  const next = options[(i + d + options.length) % options.length];
  picked(button, next.value);
}

function openList(button, at) {
  const options = DD[button.id];
  if (!options) return;
  const list = $("ddList");
  // Inside the open panel, so the modal dialog still holds it.
  const host = button.closest(".panel") || document.body;
  if (list.parentElement !== host) host.appendChild(list);
  const current = options.findIndex((o) => o.value === button.dataset.value);
  list.innerHTML = options.map((o, i) =>
    `<li role="option" id="dd-${i}" data-i="${i}" aria-selected="${i === current}">${esc(o.label)}</li>`).join("");
  list.setAttribute("aria-labelledby", button.getAttribute("aria-labelledby") || button.id);
  list.hidden = false;
  dd.button = button;
  dd.typed = "";
  button.setAttribute("aria-expanded", "true");
  place(button, list);
  setActive(at === "first" ? 0 : at === "last" ? options.length - 1 : Math.max(current, 0));
}

/** Below the button, or above it when there's more room there; never past the window. */
function place(button, list) {
  const r = button.getBoundingClientRect();
  const width = Math.max(r.width, 160);
  list.style.width = `${width}px`;
  list.style.maxHeight = "";
  const tall = Math.min(list.scrollHeight, 288);
  const below = innerHeight - r.bottom - 12, above = r.top - 12;
  const up = below < tall && above > below;
  const room = Math.min(tall, up ? above : below);
  list.style.maxHeight = `${room}px`;
  list.style.top = `${up ? r.top - 4 - room : r.bottom + 4}px`;
  list.style.left = `${Math.min(Math.max(8, r.right - width), innerWidth - width - 8)}px`;
}

function setActive(i, scroll = true) {
  const items = $("ddList").children;
  if (!dd.button || !items.length) return;
  i = Math.min(Math.max(i, 0), items.length - 1);
  if (items[dd.active]) items[dd.active].classList.remove("active");
  dd.active = i;
  items[i].classList.add("active");
  dd.button.setAttribute("aria-activedescendant", items[i].id);
  if (scroll) items[i].scrollIntoView({ block: "nearest" });
}

/** Closes the open list; `pick` takes the active option, `focus` puts focus back on the button. */
function closeList(pick, focus) {
  const button = dd.button;
  if (!button) return;
  const option = pick && DD[button.id] ? DD[button.id][dd.active] : null;
  const list = $("ddList");
  list.hidden = true;
  list.innerHTML = "";
  button.setAttribute("aria-expanded", "false");
  button.removeAttribute("aria-activedescendant");
  dd.button = null;
  dd.active = -1;
  if (focus && document.contains(button)) button.focus({ preventScroll: true });
  if (option && option.value !== button.dataset.value) picked(button, option.value);
}

function picked(button, value) {
  if (button.dataset.switch) {
    const switches = Object.assign({}, V.values);
    if (value) switches[button.dataset.switch] = value;
    else delete switches[button.dataset.switch];
    changeOwn({ switches });
  } else if (button.dataset.tvalue) {
    T.values[button.dataset.tvalue] = value;
    render();
  }
}

function typeAhead(ch) {
  const now = Date.now();
  dd.typed = now - dd.typedAt > 600 ? ch : dd.typed + ch;
  dd.typedAt = now;
  const options = DD[dd.button.id];
  const typed = dd.typed.toLowerCase();
  // The same letter again steps through the options that start with it.
  const same = [...typed].every((c) => c === typed[0]);
  const text = same ? typed[0] : typed;
  const from = same ? dd.active + 1 : dd.active;
  for (let n = 0; n < options.length; n++) {
    const i = (from + n) % options.length;
    if (options[i].label.toLowerCase().startsWith(text)) return setActive(i);
  }
}

/** Keys on a drop-down button; true when the key was used. */
function ddKey(event, button) {
  const k = event.key;
  const letter = k.length === 1 && k !== " " && !event.ctrlKey && !event.metaKey && !event.altKey;
  if (dd.button !== button) {
    if ((k === "ArrowLeft" || k === "ArrowRight") && !event.altKey) {
      stepValue(button, k === "ArrowLeft" ? -1 : 1);
    } else if (["ArrowDown", "ArrowUp", "Enter", " ", "Home", "End"].includes(k)) {
      openList(button, k === "Home" ? "first" : k === "End" ? "last" : "");
    } else if (letter && !/^[qe]$/i.test(k)) {
      openList(button);
      typeAhead(k);
    } else {
      return false;
    }
    event.preventDefault();
    return true;
  }
  const typing = Date.now() - dd.typedAt < 600 && dd.typed;
  const step = { ArrowDown: 1, ArrowUp: -1, PageDown: 10, PageUp: -10 }[k];
  if ((k === "ArrowUp" && event.altKey) || k === "Enter" || (k === " " && !typing)) closeList(true, true);
  else if (step && !event.altKey) setActive(dd.active + step);
  else if (k === "Home" || k === "End") setActive(k === "Home" ? 0 : Infinity);
  else if (k === "Escape") closeList(false, true);
  else if (k === "Tab") {
    closeList(true, false);
    return false;
  } else if (letter || k === " ") typeAhead(k);
  else if (!(k === "ArrowDown" && event.altKey)) return false;
  event.preventDefault();
  return true;
}

$("ddList").addEventListener("mousedown", (event) => event.preventDefault());
$("ddList").addEventListener("mousemove", (event) => {
  const item = event.target.closest("[role=option]");
  if (item && Number(item.dataset.i) !== dd.active) setActive(Number(item.dataset.i), false);
});
document.addEventListener("scroll", (event) => {
  if (dd.button && event.target !== $("ddList")) closeList(false, false);
}, true);
window.addEventListener("resize", () => closeList(false, false));
document.addEventListener("focusout", (event) => {
  if (event.target === dd.button && event.relatedTarget) closeList(false, false);
});

/* ---------- Test a setting ---------- */

/** The test as the back end last told it, and the answers so far. */
const T = {
  setup: null,
  loading: false,
  error: "",
  test: null,
  live: null,
  report: null,
  pick: "",
  values: {},
  before: "",
  where: "",
  whereMsg: "",
  felt: "",
  looked: "",
  black: "",
  other: "",
  said: [],
  sure: false, // asking whether to cancel
  busy: false,
  poll: 0,
  screen: "",
};
const LIMIT = 1900;
const STEP_OF = { idle: -1, loading: 0, error: 0, unfinished: 0, pick: 0, plan: 1 };
const SAID = [["closed", "I closed it"], ["stopped", "It closed by itself"], ["froze", "It froze"]];
const TFIELD = { tpick: "pick", tbefore: "before", tfelt: "felt", tlooked: "looked", tblack: "black" };

const live = () => !!(T.live && !T.live.ended);
const testing = () => live() || !!(V && V.testing);
const clock = (s) => {
  s = Math.max(0, Math.floor(s));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

function testScreen() {
  if (T.loading) return "loading";
  if (live()) return "run";
  if (!T.test) return T.error && !T.setup ? "error" : !T.setup ? "idle" : T.setup.unfinished ? "unfinished" : "pick";
  if (T.report) return "report";
  if (T.test.done === 0) return "plan";
  return T.test.done < T.test.of ? "between" : "ask";
}

function radios(name, title, options, value, attrs = "") {
  return `<fieldset class="t-q"><legend>${esc(title)}</legend><div class="t-opts">` +
    options.map(([v, label]) =>
      `<label class="t-opt"><input type="radio" name="${name}" value="${esc(v)}" data-key="${name}:${esc(v)}"${attrs}` +
      `${v === value ? " checked" : ""}><span>${esc(label)}</span></label>`).join("") +
    `</div></fieldset>`;
}

const KIND_WORDS = {
  speed: "Two runs of about 3 minutes",
  crash: "Play until it stops, up to 10 minutes",
  baseline: "One run of about 3 minutes",
};

function choiceCard(c, suggested) {
  const on = T.pick === c.key;
  const id = `tc-${c.key}`;
  const risk = RISK[c.risk] ? `<span class="tag ${RISK[c.risk][0]}">${RISK[c.risk][1]}</span>` : "";
  const build = c.build ? `<span class="tag">needs ${esc(c.build)}</span>` : "";
  const name = c.kind === "switch" ? `LEONIDA_${c.name}` : c.kind === "profile" ? `profile ${c.name}` : "";
  const notes = [c.why, c.tested && `You tested this on ${c.tested}.`]
    .filter(Boolean).map((n) => `<span class="t-small">${esc(n)}</span>`).join("");
  // The maintainers' own figures are long and technical: there for whoever wants them.
  const measured = c.measured
    ? `<details class="t-more" data-open="m:${esc(c.key)}"${ui.open[`m:${c.key}`] ? " open" : ""}>` +
      `<summary>What was measured${ICON.chev}</summary><p>${esc(c.measured)}</p></details>`
    : "";
  const options = c.values.map((v) => ({ value: v, label: shown(v) }));
  const value = on && c.values.length > 1
    ? `<div class="t-value"><span id="${esc(id)}-v">Value to test</span>` +
      dropdown(`tv-${c.key}`, options, T.values[c.key] || c.value, ` data-tvalue="${esc(c.key)}" aria-labelledby="${esc(id)}-v"`) + `</div>`
    : "";
  return `<div class="t-option"><label class="mode t-choice${suggested ? " suggested" : ""}">` +
    `<input type="radio" name="tpick" value="${esc(c.key)}" data-key="tp:${esc(c.key)}"${on ? " checked" : ""}>` +
    `<span class="mode-dot" aria-hidden="true"></span><span class="mode-text"><strong>${esc(c.title)}</strong>` +
    `<span class="t-meta">${name ? `<span class="t-mono">${esc(name)}</span>` : ""}${risk}${build}` +
    `<span>${esc(KIND_WORDS[c.test] || "")}</span></span>` +
    (c.about ? `<span>${esc(c.about)}</span>` : "") + notes + `</span></label>${value}${measured}</div>`;
}

function allChoices() {
  const s = T.setup;
  return [s.suggested].concat(s.choices, s.branch);
}

const SCREENS = {
  idle: () => "",
  loading: () =>
    `<div class="t-wait"><span class="spinner" aria-hidden="true"></span><p>Looking at your PC and leonida.exe. This takes a few seconds and doesn't start the game.</p></div>`,
  error: () => `<p class="t-problem">${esc(T.error)}</p><div class="t-actions">${btn("tClose", "Close")}</div>`,
  unfinished: () => {
    const u = T.setup.unfinished;
    return `<div class="t-card"><p class="t-kicker">Not finished</p><h3>${esc(u.title)}${valueLabel(u.value_label)}</h3>` +
      `<p>You started this test on ${esc(u.created)} and did ${u.done} of ${u.of} runs. You can do the rest now.</p></div>` +
      `<div class="t-actions">${btn("tKeep", "Do the rest", "warm")}${btn("tDrop", "Throw it away and pick again")}</div>`;
  },
  pick: () => {
    const s = T.setup;
    if (!allChoices().some((c) => c.key === T.pick)) T.pick = s.suggested.key;
    const chosen = allChoices().find((c) => c.key === T.pick);
    let html = s.problem ? `<p class="t-note">${esc(s.problem)}</p>` : "";
    html += `<p class="t-pc"><span>Your card</span>${esc(s.card || "not reported")}<span>Build</span>${esc(s.leonida || "unknown")}</p>`;
    html += `<h3 class="t-h">Suggested for your card</h3><div class="t-choices">${choiceCard(s.suggested, true)}</div>`;
    if (s.choices.length) {
      html += `<h3 class="t-h">Or pick another</h3><div class="t-choices">${s.choices.map((c) => choiceCard(c)).join("")}</div>`;
    }
    if (s.branch.length) {
      const open = ui.open.tbranch ?? s.branch.some((c) => c.key === T.pick);
      html += `<details class="sub" data-open="tbranch"${open ? " open" : ""}><summary>For test builds (${s.branch.length})${ICON.chev}</summary>` +
        `<p class="muted">These only work with a branch build of Leonida Legacy.</p>` +
        `<div class="t-choices">${s.branch.map((c) => choiceCard(c)).join("")}</div></details>`;
    }
    if (chosen.test === "crash") {
      html += radios("tbefore", "Without this change, how often does the game stop?",
        [["keeps stopping", "Every time I play"], ["sometimes", "Now and then"]], T.before);
    }
    if (T.error) html += `<p class="t-problem">${esc(T.error)}</p>`;
    return html + `<div class="t-actions t-sticky">${btn("tBegin", "Next", "warm", T.busy ? " disabled" : "")}` +
      `<span class="t-picked">${esc(chosen.title)}</span></div>`;
  },
  plan: () => {
    const t = T.test;
    const warmed = T.live && T.live.warm && T.live.ended;
    let html = testCard(t, "Testing");
    html += `<ol class="t-how">${howTo(t).map((li) => `<li>${esc(li)}</li>`).join("")}</ol>`;
    if (!t.warmup.startsWith("done")) {
      html += `<div class="t-warm"><h3>First, warm up the shaders</h3>` +
        `<p>The game starts once without you and translates its shaders, so the stutter of doing that doesn't count against the setting. ` +
        `It takes a few minutes and closes by itself. Needed once per leonida.exe.</p>` +
        (warmed && !T.live.warm_ok ? `<p class="t-problem">The warm-up didn't finish. Try it again.</p>` : "") +
        btn("tWarm", "Warm up now", "warm", T.busy ? " disabled" : "") + `</div>`;
    } else {
      if (warmed && T.live.warm_ok) html += `<p class="t-ok">The shaders are warmed up.</p>`;
      html += `<div class="t-actions">${btn("tStart", t.of > 1 ? `Start run 1 of ${t.of}` : "Start the run", "warm", T.busy ? " disabled" : "")}</div>`;
    }
    return html + cancelLink();
  },
  run: () => {
    const l = T.live;
    const stop = btn("tStop", l.stopping ? "Stopping..." : "It froze, stop it", "ghost", l.stopping ? " disabled" : "") +
      `<p class="muted">${l.warm ? "Only if nothing has changed for a long time." : "Only if the game froze. Otherwise close the game the usual way."}</p>`;
    if (l.warm) {
      return `<div class="t-run"><p class="t-kicker">Warming up shaders</p><p class="t-clock" aria-live="off">${clock(l.elapsed_s)}</p>` +
        `<p>The game window opens and closes by itself. You don't need to play.</p></div><div class="t-actions">${stop}</div>`;
    }
    const line = l.stopping ? "Stopping the game..."
      : !l.frames ? "Waiting for the game. Load your save when it's up, then play."
      : l.beeped ? "That's enough. Close the game the usual way."
      : `Keep playing. Two beeps at ${clock(l.play_s)} say you can stop.`;
    return `<div class="t-run"><p class="t-kicker">Run ${l.number} of ${l.of}</p>` +
      `<p class="t-clock" aria-live="off">${clock(l.played_s)}<span>of ${clock(l.play_s)} played</span></p>` +
      `<progress class="t-bar" max="${Math.round(l.play_s)}" value="${Math.min(Math.round(l.played_s), Math.round(l.play_s))}" aria-label="Time played"></progress>` +
      `<p>${esc(line)}</p></div><div class="t-actions">${stop}</div>`;
  },
  between: () => {
    const t = T.test;
    return ended(t) + saidQuestion(t) + whereField("So you can play the same stretch in the next run.") +
      (T.error ? `<p class="t-problem">${esc(T.error)}</p>` : "") +
      `<div class="t-actions">${btn("tStart", `Start run ${t.done + 1} of ${t.of}`, "warm", T.busy ? " disabled" : "")}</div>` + cancelLink();
  },
  ask: () => {
    const t = T.test;
    const both = t.of > 1;
    let html = ended(t) + saidQuestion(t);
    if (t.kind === "speed" && both) {
      html += radios("tfelt", "Did the second run feel smoother, the same, or worse than the first?",
        [["smoother", "Smoother"], ["about the same", "The same"], ["worse", "Worse"], ["can't tell", "Can't tell"]], T.felt);
    }
    html += radios("tlooked", `Did anything look wrong${both ? " in either run" : ""}, like missing or flickering things?`,
      [["no", "No"], ["yes", "Yes"], ["didn't look", "Didn't look"]], T.looked);
    html += radios("tblack", "Did the screen go black?",
      both ? [["no", "No"], ["yes, in one run", "In one run"], ["yes, in both", "In both"]] : [["no", "No"], ["yes, in one run", "Yes"]], T.black);
    html += whereField("Helps others play the same stretch.");
    html += `<div class="t-q"><label class="t-label" for="tOther">Anything else? <span>Optional. Words only, no file paths or your name.</span></label>` +
      `<textarea class="well t-text" id="tOther" data-field="other" data-key="tOther" rows="3" maxlength="600" spellcheck="true"></textarea></div>`;
    if (T.error) html += `<p class="t-problem">${esc(T.error)}</p>`;
    return html + `<div class="t-actions">${btn("tReport", "Show the report", "warm", T.busy ? " disabled" : "")}</div>` + cancelLink();
  },
  report: () => {
    const r = T.report, t = T.test;
    let html = "";
    if (r.left_out.length) {
      html += `<div class="t-note warn"><p>Your note was left out because it had your user name or a path in it. Please write it again without that.</p>` +
        btn("tEdit", "Change my answers") + `</div>`;
    }
    html += `<p class="muted">This is everything that gets shared: figures and your words. No files, screenshots, paths or your user name. ` +
      `Nothing is sent until you paste it yourself.</p>`;
    html += `<pre class="well t-report" tabindex="0" aria-label="The report">${esc(r.text)}</pre>`;
    html += `<p class="t-count${r.over ? " over" : ""}">${r.chars.toLocaleString("en-GB")} of ${LIMIT.toLocaleString("en-GB")} characters` +
      (r.over ? ". That's more than one Discord message holds; Discord offers to send it as a file, which is fine." : "") + `</p>`;
    html += `<div class="t-actions">${btn("tCopy", "Copy and open Discord", "warm")}${btn("tEdit", "Change my answers")}</div>`;
    html += `<p class="muted">In #test-results, press Ctrl+V, then Enter.</p>`;
    html += `<p class="t-links">${link("tInvite", "Not on the Discord server yet? Join it")}${link("tFolder", "Open the tests folder")}` +
      `${link("tAgain", t.of > 1 ? "Do both runs again" : "Do the run again")}</p>`;
    return html + `<div class="t-actions">${btn("tDone", "Done")}</div>`;
  },
};

/** A profile's value is always "on", which says nothing next to its title. */
const valueLabel = (label) => (label && label !== "on" ? ` <em>${esc(label)}</em>` : "");

function testCard(t, kicker) {
  return `<div class="t-card"><p class="t-kicker">${esc(kicker)}</p><h3>${esc(t.title)}${valueLabel(t.value_label)}</h3></div>`;
}

function howTo(t) {
  const steps = [];
  if (t.of > 1) {
    steps.push("The launcher starts the game twice. One run has the setting and one doesn't. It doesn't say which, so you judge by feel alone.");
  } else if (t.kind === "baseline") {
    steps.push("The launcher starts the game once, with nothing changed.");
  } else {
    steps.push("The launcher starts the game once, with the setting.");
  }
  steps.push(t.kind === "crash"
    ? "Play as you usually do until the game stops. If it keeps going, two beeps after 10 minutes say you can close it."
    : "Each time, load the same save and play the same stretch. Two beeps after about 3 and a half minutes say you can close the game the usual way.");
  steps.push("If the game freezes, come back here and press It froze, stop it.");
  steps.push("Close other heavy programs first, and keep a laptop plugged in.");
  if (t.where_) steps.push(`Last time you were: ${t.where_}`);
  return steps;
}

function ended(t) {
  const last = t.last;
  if (!last) return "";
  const figures = last.figures ? ` ${clock(last.played_s)} of play measured.`
    : last.played_s > 0 ? "" : " The game logged no frame times.";
  let html = `<div class="t-card"><p class="t-kicker">Run ${t.done} of ${t.of} done</p><p>${esc(last.words)}.${figures}</p></div>`;
  if (last.short && !last.stopped) {
    html += `<div class="t-note"><p>That's ${clock(last.played_s)} of play. Runs need about 3 minutes to be compared, so this one won't count for much.</p>` +
      btn("tRetry", "Play this run again") + `</div>`;
  }
  return html;
}

function saidQuestion(t) {
  const i = t.done - 1, last = t.last;
  if (!last) return "";
  if (T.said[i] === undefined) T.said[i] = last.ending === "froze" ? "froze" : last.stopped ? "stopped" : "";
  const title = last.ending === "closed"
    ? "The game ended without an error. Did you close it, or did it close by itself?"
    : "How did it end for you?";
  return radios("tsaid", title, SAID, T.said[i], ` data-run="${i}"`);
}

function whereField(why) {
  const msg = T.whereMsg ? `<p class="t-problem" id="tWhereMsg">${esc(T.whereMsg)}</p>` : "";
  return `<div class="t-q"><label class="t-label" for="tWhere">Where in the game were you? <span>Optional. ${esc(why)}</span></label>` +
    `<input class="t-input" id="tWhere" data-field="where" data-key="tWhere" maxlength="300" autocomplete="off"` +
    ` placeholder="For example: driving north from the motel"${msg ? ' aria-describedby="tWhereMsg"' : ""}>${msg}</div>`;
}

function cancelLink() {
  if (!T.sure) return `<p class="t-links">${link("tCancel", "Cancel this test")}</p>`;
  return `<p class="confirm"><span>Cancel this test? The runs so far are thrown away.</span>` +
    `<button type="button" class="btn btn-small btn-danger" data-act="tCancelYes" data-key="tCancelYes">Cancel the test</button>` +
    `<button type="button" class="btn btn-small btn-ghost" data-act="tCancelNo" data-key="tCancelNo">Keep going</button></p>`;
}

/** Draws the panel and the tile; true when the panel moved to another screen. */
function renderTest() {
  renderTestTile();
  const screen = testScreen();
  // One step per run, named by number only: the window never says which run has the setting.
  const of = (T.live && T.live.of) || (T.test && T.test.of) || 2;
  const runs = of > 1 ? Array.from({ length: of }, (_, i) => `Run ${i + 1}`) : ["Run"];
  const steps = ["Pick", "Before", ...runs, "Questions", "Report"];
  const at = screen === "run" ? (T.live.warm ? 1 : 1 + T.live.number)
    : screen === "between" ? 2 + T.test.done
    : screen === "ask" ? 2 + of
    : screen === "report" ? 3 + of
    : STEP_OF[screen];
  setHTML($("testSteps"), steps.map((s, i) =>
    `<li class="${i < at ? "done" : ""}"${i === at ? ' aria-current="step"' : ""}><span>${i < at ? ICON.check : i + 1}</span><b>${s}</b></li>`).join(""));
  setHTML($("testBody"), SCREENS[screen]());
  renderTestSide(screen);
  for (const el of $("testBody").querySelectorAll("[data-field]")) {
    const value = T[el.dataset.field];
    if (document.activeElement !== el && el.value !== value) el.value = value;
  }
  const moved = screen !== T.screen;
  T.screen = screen;
  return moved;
}

function renderTestTile() {
  if (!V) return;
  const ready = V.has_game && V.has_exe;
  const busy = live();
  const note = busy ? (T.live.warm ? "Warming up shaders..." : `Run ${T.live.number} of ${T.live.of} is going`)
    : T.test && !T.report ? `${T.test.done} of ${T.test.of} runs done. Open to go on.`
    : ready ? "Try one setting, then share what it did."
    : "Needs the game folder and leonida.exe first.";
  $("testTile").setAttribute("aria-disabled", String(!ready));
  $("testTile").classList.toggle("off", !ready);
  const going = busy || (T.test && !T.report);
  menuRow("testTile", "Test a setting", note, busy ? "Running" : going ? "Go on" : "Open", going ? "ok" : "");
}

/** The Test tab's side pane: what this step is for, in plain words. */
const SIDE = {
  idle: ["Test a setting", "Try one setting and see what it does. The launcher starts the game, measures, and writes a short report you can share."],
  loading: ["Getting ready", "Looking at your PC and leonida.exe. This doesn't start the game."],
  error: ["That didn't work", "The reason is on the left. Close it and try again later."],
  unfinished: ["Not finished", "A test you started earlier still has runs to do. Do the rest, or throw it away and pick again."],
  pick: ["Pick a setting", "The one at the top suits your card. Each choice says how long it takes. Next shows what to do before you play."],
  plan: ["Before you play", "Load the same save each run and play the same stretch, so the runs can be compared. Close other heavy programs and keep a laptop plugged in."],
  run: ["In the game", "The launcher measures by itself. A warm-up needs no playing. In a run, play as usual until two beeps say you can stop, then close the game the usual way."],
  between: ["Next run", "One run has the setting and one doesn't. The launcher doesn't say which, so you judge by feel alone."],
  ask: ["A few questions", "Your answers go into the report next to the figures. Nothing is sent anywhere from here."],
  report: ["Your report", "It has figures and your words, no paths, logs or game files. Nothing is sent until you paste it yourself."],
};

function renderTestSide(screen) {
  const [title, text] = SIDE[screen] || SIDE.idle;
  setHTML($("testSide"), `<h2>${esc(title)}</h2><p class="lead">${esc(text)}</p>`);
}

function takeStatus(status) {
  T.live = status.live;
  T.test = status.test;
  if (!T.test) T.report = null;
  else if (T.test.report && !T.report) {
    T.report = { text: T.test.report, chars: T.test.chars, over: T.test.chars > LIMIT, left_out: [] };
  }
  if (T.test && !T.where) T.where = T.test.where_;
  if (live()) poll();
}

function poll() {
  if (T.poll) return;
  T.poll = setInterval(async () => {
    let status;
    try {
      status = await invoke("test_status");
    } catch (error) {
      return;
    }
    takeStatus(status);
    if (!live()) {
      clearInterval(T.poll);
      T.poll = 0;
      T.busy = false;
      if (!ui.panel) openPanel(true, "testPanel");
    }
    render();
  }, 1000);
}

/** Calls the back end; a failure is said in a toast. */
async function call(command, args) {
  try {
    return { ok: true, value: await invoke(command, args) };
  } catch (error) {
    problem(error);
    return { ok: false };
  }
}

async function refreshStatus() {
  const r = await call("test_status");
  if (r.ok) takeStatus(r.value);
  render();
}

function clearAnswers(keepWhere) {
  Object.assign(T, { felt: "", looked: "", black: "", other: "", said: [], whereMsg: "", report: null, error: "", sure: false });
  if (!keepWhere) T.where = "";
}

async function openTest() {
  if (!V) return;
  if (!V.has_game || !V.has_exe) return flash(V.has_game ? "leonidaTile" : "gameTile");
  openPanel(true, "testPanel");
  T.error = "";
  const r = await call("test_status");
  if (r.ok) takeStatus(r.value);
  if (!T.test && !live()) {
    T.loading = true;
    render();
    try {
      T.setup = await invoke("test_setup");
      T.pick = "";
      T.values = {};
      T.before = "";
    } catch (error) {
      T.setup = null;
      T.error = String(error);
    }
    T.loading = false;
  }
  render();
}

/** Keeps where the player was, cleaned by the back end; says so when it had to leave it out. */
async function checkWhere() {
  const r = await call("test_where", { text: T.where });
  T.whereMsg = r.ok ? r.value : "";
  render();
  if (T.whereMsg && $("tWhere")) $("tWhere").focus();
  return T.whereMsg;
}

async function startRun(warm) {
  if (T.busy) return;
  if (!warm && T.test.done > 0 && (await checkWhere())) return;
  T.busy = true;
  T.error = "";
  render();
  const r = await call("test_start", { warm });
  if (r.ok) {
    toast(warm ? "Warming up. The game window opens in a moment." : "The game is starting. Load your save and play.", "done");
    await refreshStatus();
  }
  if (!live()) T.busy = false;
  render();
}

async function begin() {
  const c = allChoices().find((x) => x.key === T.pick) || T.setup.suggested;
  if (c.test === "crash" && !T.before) {
    T.error = "Say how often the game stops without the change first.";
    return render();
  }
  T.busy = true;
  render();
  const before = c.test === "crash" ? T.before : "";
  const r = await call("test_begin", { begin: { key: c.key, value: T.values[c.key] || c.value, before } });
  T.busy = false;
  if (r.ok) {
    T.test = r.value;
    T.live = null;
    clearAnswers(false);
  }
  render();
}

async function makeReport() {
  if (T.busy) return;
  T.busy = true;
  render();
  const answers = { where_: T.where, felt: T.felt, looked: T.looked, black: T.black, other: T.other, said: T.said.map((s) => s || "") };
  const r = await call("test_report", { answers });
  T.busy = false;
  if (r.ok) T.report = r.value;
  render();
}

const TEST_ACTIONS = {
  tClose: () => openPanel(false),
  tKeep: async () => {
    const r = await call("test_resume", { keep: true });
    if (r.ok) {
      T.setup.unfinished = null;
      clearAnswers(false);
      await refreshStatus();
    }
  },
  tDrop: async () => {
    const r = await call("test_resume", { keep: false });
    if (r.ok) T.setup.unfinished = null;
    render();
  },
  tBegin: begin,
  tWarm: () => startRun(true),
  tStart: () => startRun(false),
  tStop: async () => {
    if ((await call("test_stop")).ok) refreshStatus();
  },
  tRetry: async () => {
    const r = await call("test_retry");
    if (r.ok) {
      T.test = r.value;
      T.live = null;
      T.said.length = T.test.done;
      render();
    }
  },
  tReport: makeReport,
  tEdit: () => {
    T.report = null;
    render();
  },
  tCopy: async () => {
    if (!(await copy(T.report.text))) return problem("Couldn't copy. Click in the report, press Ctrl+A, then Ctrl+C.");
    toast("Copied the report. In #test-results, press Ctrl+V, then Enter.", "done");
    call("open_discord");
  },
  tInvite: () => call("open_invite"),
  tFolder: () => call("open_tests"),
  tAgain: async () => {
    const r = await call("test_again");
    if (r.ok) {
      T.test = r.value;
      T.live = null;
      clearAnswers(true);
      render();
    }
  },
  tCancel: () => {
    T.sure = true;
    render();
  },
  tCancelNo: () => {
    T.sure = false;
    render();
  },
  tCancelYes: async () => {
    if (!(await call("test_cancel")).ok) return;
    T.test = null;
    T.live = null;
    clearAnswers(false);
    openTest();
  },
  tDone: async () => {
    await call("test_cancel");
    T.test = null;
    T.live = null;
    T.setup = null;
    clearAnswers(false);
    openPanel(false);
    render();
  },
};

/* ---------- Actions ---------- */

function flash(id) {
  openPanel(false);
  const el = $(id);
  el.classList.remove("flash");
  void el.offsetWidth;
  el.classList.add("flash");
  setTimeout(() => el.classList.remove("flash"), reduced.matches ? 2000 : 1200);
  el.focus({ preventScroll: true });
}

function focusHero() {
  const first = $("hero").querySelector("button:not([disabled])");
  if (first) first.focus({ preventScroll: true });
}

async function search() {
  if (ui.searching) return;
  ui.searching = true;
  ui.problem = "";
  ui.found = [];
  render();
  try {
    const result = await invoke("find_game");
    V = result.view;
    const games = result.games;
    if (!games.length) {
      ui.problem = result.timed_out
        ? "Nothing found in a minute of looking. Pick the folder by hand."
        : "Nothing found on your drives. Pick the folder by hand.";
    } else if (games.length === 1 && V.game_dir === games[0].dir) {
      ui.hero = null;
      toast("Found your game", "done");
    } else {
      ui.found = games;
    }
  } catch (error) {
    ui.problem = String(error);
  }
  ui.searching = false;
  render();
  if (ui.found.length || ui.problem) focusHero();
}

function doneChanging(message) {
  ui.hero = null;
  ui.found = [];
  ui.problem = "";
  if (message) toast(message, "done");
}

async function chooseGame(dir) {
  try {
    V = await invoke("choose_game", { dir });
    doneChanging("Using that game folder");
  } catch (error) {
    ui.problem = String(error);
  }
  render();
}

async function pickGame(exe) {
  try {
    const result = await invoke("pick_game", { exe });
    V = result.view;
    if (result.picked) doneChanging("Using that game folder");
  } catch (error) {
    ui.problem = String(error);
  }
  render();
}

async function findExe() {
  if (ui.findingExe) return;
  ui.findingExe = true;
  ui.problem = "";
  render();
  try {
    V = await invoke("find_leonida");
    doneChanging("Found leonida.exe");
    checkUpdates(false);
  } catch (error) {
    ui.hero = "leonida";
    ui.problem = String(error);
  }
  ui.findingExe = false;
  render();
}

async function pickLeonida() {
  try {
    const result = await invoke("pick_leonida");
    V = result.view;
    if (result.picked) {
      doneChanging("Using that leonida.exe");
      checkUpdates(false);
    }
  } catch (error) {
    ui.problem = String(error);
  }
  render();
}

async function launch(what) {
  if (ui.starting) return;
  ui.starting = what;
  ui.startSeen = false;
  render();
  let started = false;
  try {
    toast(await invoke("launch", { what }), "done");
    started = true;
  } catch (error) {
    problem(error);
  }
  if (started) {
    for (const ms of [1500, 4000, 7000, 10000]) setTimeout(checkRunning, ms);
  }
  setTimeout(() => {
    ui.starting = "";
    render();
  }, started ? 3000 : 0);
}

/** Asks the task list whether leonida.exe runs. Only after a start and when the window comes
 *  back into view: a standing poll would spawn tasklist behind the game while it is measured. */
async function checkRunning() {
  if (ui.checking || !V) return;
  ui.checking = true;
  try {
    const running = await invoke("is_running");
    if (running !== ui.running) {
      ui.running = running;
      render();
    }
  } catch (error) {
    // Nothing to tell: the next look tries again.
  }
  ui.checking = false;
}

/** At start nothing is said: the built-in list works, and the dot and its tip tell the rest.
 *  Refresh says what GitHub answered, in the back end's words. */
async function refreshLists(asked) {
  if (ui.listsLoading) return;
  ui.listsLoading = true;
  render();
  try {
    V = await invoke("refresh_lists");
    if (asked) toast(V.source_detail, V.source === "github" ? "done" : "info");
  } catch (error) {
    if (asked) problem(error);
  }
  ui.listsLoading = false;
  render();
}

/** Asks the back end what is out, which asks GitHub at most once an hour unless the player asked.
 *  Unasked, nothing is said: the rows and About show what there is. */
async function checkUpdates(asked) {
  if (!V || (asked && ui.busy)) return;
  if (asked) {
    ui.busy = "check";
    render();
  }
  try {
    V = await invoke("check_updates", { now: !!asked });
    if (asked) toast(updateSummary(), U().note ? "info" : "done");
  } catch (error) {
    if (asked) problem(error);
  }
  if (asked) ui.busy = "";
  render();
}

/** Runs one update step that ends in a new view, with `busy` naming it meanwhile. */
async function updateStep(busy, command, args) {
  if (ui.busy) return null;
  ui.busy = busy;
  ui.problem = "";
  render();
  let result = null;
  try {
    result = await invoke(command, args);
  } catch (error) {
    problem(error);
  }
  ui.busy = "";
  render();
  return result;
}

async function updateLauncher() {
  const said = await updateStep("launcher", "update_launcher");
  // On success the new launcher starts and this one closes; otherwise the player has steps to do.
  if (said) toast(said, said.startsWith("Starting") ? "done" : "keep");
}

async function installBuild() {
  const view = await updateStep("leonida", "install_build");
  if (!view) return;
  V = view;
  doneChanging();
  ui.hl = "warmTile";
  render();
  $("warmTile").focus({ preventScroll: true });
  toast(`Using Leonida build ${U().have_build}. Warm up the shaders once before you play: a new build translates them again.`, "keep");
}

async function later(what) {
  try {
    V = await invoke("update_later", { what });
    toast("Hidden until a newer one comes out. About still shows what you have.", "info");
  } catch (error) {
    problem(error);
  }
  render();
  if (!ui.panel) $("playBtn").focus({ preventScroll: true });
}

async function usePrevious() {
  try {
    V = await invoke("use_previous");
    doneChanging("Using the previous build");
  } catch (error) {
    problem(error);
  }
  render();
  checkUpdates(false);
}

async function updateCheckout() {
  const said = await updateStep("checkout", "update_checkout");
  if (said) toast(said, "done");
}

/** The user's own choices as the back end stores them, with `change` applied. */
async function save(change) {
  const choices = Object.assign(
    { advanced: V.advanced, profiles: V.picked.slice(), switches: Object.assign({}, V.values), extra: $("extra").value },
    change,
  );
  V = Object.assign({}, V, { advanced: choices.advanced, picked: choices.profiles, values: choices.switches, extra: choices.extra });
  render();
  try {
    V = await invoke("save_choices", { choices });
  } catch (error) {
    problem(error);
  }
  render();
}

/** A change under Advanced: it only counts with My own choices on, so that comes on too. */
function changeOwn(change) {
  if (!V.advanced) toast("Switched to your own choices", "info");
  save(Object.assign({ advanced: true }, change));
}

async function copy(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch (error) {
    const area = document.createElement("textarea");
    area.value = text;
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    const done = document.execCommand("copy");
    area.remove();
    return done;
  }
}

const PANEL_TILE = { panel: "modeTile", testPanel: "testTile" };
const TABS = ["", "panel", "testPanel", "about"];
const TAB_PANEL = { "": "home", panel: "panel", testPanel: "testPanel", about: "about" };

/** Shows the tab `id` ("" is Play), or goes back to Play. Back on Play, the highlight returns to
 *  the row that opened the tab. */
function openPanel(open, id = ui.panel) {
  const next = open ? id : "";
  if (next === ui.panel) return;
  const from = ui.panel;
  closeList(false, false);
  ui.panel = next;
  for (const tab of TABS) {
    const on = tab === next;
    $(TAB_PANEL[tab]).hidden = !on;
    $(`tab-${TAB_PANEL[tab]}`).setAttribute("aria-selected", String(on));
    $(`tab-${TAB_PANEL[tab]}`).tabIndex = on ? 0 : -1;
  }
  for (const [panel, tile] of Object.entries(PANEL_TILE)) $(tile).setAttribute("aria-expanded", String(next === panel));
  document.body.dataset.tab = TAB_PANEL[next];
  if (!next) $(PANEL_TILE[from] || (ui.hl === "playTile" ? "playBtn" : ui.hl)).focus({ preventScroll: true });
  else if (document.activeElement === document.body || !document.activeElement.closest("#tabs")) {
    $(`tab-${TAB_PANEL[next]}`).focus({ preventScroll: true });
  }
  renderHints();
}

/** A tab chosen from the tab row or with Q and E. Test loads its setup on the way in. */
function pickTab(id) {
  if (id === "testPanel") return openTest();
  if (id) openPanel(true, id);
  else openPanel(false);
}

function cycleTab(d) {
  const i = TABS.indexOf(ui.panel);
  pickTab(TABS[(i + d + TABS.length) % TABS.length]);
}

/** Esc: closes the start screen, then a value list, then the game picker, then goes to Play. */
function back() {
  if (document.body.classList.contains("starting")) {
    ui.startSeen = true;
    return render();
  }
  if (dd.button) return closeList(false, true);
  if (ui.panel) return openPanel(false);
  if (ui.hero) {
    ACTIONS.keep();
    $(ui.hl || "playBtn").focus({ preventScroll: true });
  }
}

/** The Play rows in order, as the elements that take focus. */
const playRows = () => [$("playBtn"), $("launcherNews"), $("buildNews"), $("gameTile"), $("leonidaTile"), $("warmTile"),
  $("modeTile"), $("testTile")].filter((row) => !row.hidden);

function moveRow(d) {
  const rows = playRows();
  const at = rows.indexOf(document.activeElement);
  const i = at < 0 ? Math.max(rows.findIndex((r) => rowOf(r).id === ui.hl), 0) : (at + d + rows.length) % rows.length;
  rows[i].focus();
  rows[i].scrollIntoView({ block: "nearest" });
}

const rowOf = (el) => el.closest(".mrow");

/** Puts the highlight bar on the Play row `id`. */
function highlight(id) {
  if (!id || ui.hl === id) return;
  ui.hl = id;
  for (const row of document.querySelectorAll("#tiles .mrow")) row.classList.toggle("hl", row.id === id);
  renderHero();
}

const ACTIONS = {
  ...TEST_ACTIONS,
  search,
  pickFolder: () => pickGame(false),
  pickExe: () => pickGame(true),
  keep: () => {
    doneChanging();
    render();
  },
  findExe,
  pickLeonida,
  buildSteps: () => invoke("open_build_steps").catch(problem),
  creditFork: () => invoke("open_credit", { which: "fork" }).catch(problem),
  creditVictory: () => invoke("open_credit", { which: "victory" }).catch(problem),
  checkNow: () => checkUpdates(true),
  updateLauncher,
  laterLauncher: () => later("launcher"),
  launcherPage: () => invoke("open_release", { which: "launcher" }).catch(problem),
  installBuild,
  laterBuild: () => later("leonida"),
  buildPage: () => invoke("open_release", { which: "leonida" }).catch(problem),
  usePrevious,
  updateCheckout,
};

document.addEventListener("click", (event) => {
  const target = event.target;
  const option = target.closest("#ddList [role=option]");
  if (option) {
    setActive(Number(option.dataset.i), false);
    closeList(true, true);
    return;
  }
  const arrow = target.closest(".arr");
  if (arrow) {
    const button = $(arrow.dataset.for);
    closeList(false, false);
    if (button) {
      button.focus({ preventScroll: true });
      stepValue(button, Number(arrow.dataset.step));
    }
    return;
  }
  const hint = target.closest("[data-hint]");
  if (hint) return HINTS[hint.dataset.hint]();
  const tab = target.closest("[role=tab]");
  if (tab) return pickTab(tab.dataset.tab);
  const ddButton = target.closest(".dd");
  if (dd.button && ddButton !== dd.button) closeList(false, false);
  if (ddButton) {
    if (dd.button === ddButton) closeList(false, true);
    else openList(ddButton);
    return;
  }
  const act = target.closest("[data-act]");
  if (act) {
    if (act.dataset.act === "choose") chooseGame(act.dataset.dir);
    else ACTIONS[act.dataset.act]();
    return;
  }
  if (!V) return;
  if (target.closest("#playBtn")) {
    const b = blocker();
    if (!b) launch("play");
    else if (b.tile) flash(b.tile);
    else toast(b.text, "info");
    return;
  }
  if (target.closest("#gameTile, #leonidaTile")) {
    const game = !!target.closest("#gameTile");
    ui.hero = game ? "game" : "leonida";
    ui.problem = "";
    if (!game) ui.found = [];
    render();
    focusHero();
    return;
  }
  if (target.closest("#warmTile")) {
    const b = blocker();
    if (!b) launch("warm");
    else if (b.tile) flash(b.tile);
    else toast("The game is running. Warm up after you close it.", "info");
    return;
  }
  const news = target.closest("#launcherNews, #buildNews");
  if (news) {
    highlight(news.id);
    return focusHero();
  }
  if (target.closest("#modeTile")) return openPanel(true, "panel");
  if (target.closest("#testTile")) return openTest();

  const chipEl = target.closest("[data-profile]");
  if (chipEl) {
    const name = chipEl.dataset.profile;
    const profiles = V.picked.includes(name) ? V.picked.filter((x) => x !== name) : V.picked.concat(name);
    changeOwn({ profiles });
    return;
  }
  if (target.closest("#clearProfiles")) {
    const listed = new Set(V.profiles.map((p) => p.name));
    changeOwn({ profiles: V.picked.filter((x) => !listed.has(x)) });
    return;
  }
  if (target.closest("#refresh")) return refreshLists(true);
  if (target.closest("#copyCmd")) {
    copy(V.command).then((done) => (done ? toast("Copied the run command", "done") : problem("Couldn't copy. Select the command and press Ctrl+C.")));
    return;
  }
  if (target.closest("#resetAll")) {
    if (!V.changes) {
      if (V.advanced) {
        save({ advanced: false });
        toast("Back on Recommended", "done");
      } else {
        toast("Already on Recommended. There's nothing to reset.", "info");
      }
      return;
    }
    $("confirmText").textContent = `This clears ${plural(V.changes, "choice", "choices")} and goes back to Recommended.`;
    $("confirmReset").hidden = false;
    $("doReset").focus();
    return;
  }
  if (target.closest("#keepAll")) {
    $("confirmReset").hidden = true;
    $("resetAll").focus();
    return;
  }
  if (target.closest("#doReset")) {
    $("confirmReset").hidden = true;
    $("extra").value = "";
    save({ advanced: false, profiles: [], switches: {}, extra: "" });
    toast("Cleared your own choices", "done");
    $("resetAll").focus();
  }
});

document.addEventListener("change", (event) => {
  const el = event.target;
  if (!V) return;
  if (el.name === "mode") {
    const advanced = el.value === "advanced";
    if (advanced) $("advanced").open = true;
    save({ advanced });
    return;
  }
  if (el.name === "tsaid") {
    T.said[Number(el.dataset.run)] = el.value;
    render();
  } else if (TFIELD[el.name]) {
    T[TFIELD[el.name]] = el.value;
    T.error = "";
    render();
  }
});

document.addEventListener("input", (event) => {
  const field = event.target.dataset && event.target.dataset.field;
  if (field) T[field] = event.target.value;
});
document.addEventListener("focusout", (event) => {
  if (event.target.id === "tWhere" && T.test) checkWhere();
});

// Saved on every key: closing the window tears the page down before pagehide could save a
// pending change, and the file is a few hundred bytes.
$("extra").addEventListener("input", () => changeOwn({ extra: $("extra").value }));

// Opt-in figures: shown under the group while a chip is pointed at or focused.
function showMeasured(event) {
  const chipEl = event.target.closest && event.target.closest(".pchip");
  const group = chipEl && chipEl.closest(".pgroup");
  if (!group) return;
  const note = group.querySelector(".pnote");
  const enter = event.type === "mouseover" || event.type === "focusin";
  note.textContent = enter && chipEl.dataset.measured ? chipEl.dataset.measured : note.dataset.note;
}
for (const type of ["mouseover", "mouseout", "focusin", "focusout"]) document.addEventListener(type, showMeasured);

document.addEventListener("toggle", (event) => {
  const el = event.target;
  if (el.id === "advanced") {
    try {
      localStorage.setItem("advancedOpen", el.open ? "1" : "0");
    } catch (error) {
      // Remembering is a convenience; without storage it starts closed.
    }
  } else if (el.id === "testBuilds") {
    ui.open.tests = el.open;
  } else if (el.dataset && el.dataset.open) {
    ui.open[el.dataset.open] = el.open;
  }
}, true);

/** What each key hint in the bottom bar does when clicked. */
const HINTS = {
  move: () => moveRow(1),
  change: () => {
    const button = document.activeElement && document.activeElement.closest(".dd");
    if (button) stepValue(button, 1);
  },
  select: () => {
    const el = document.activeElement;
    if (el && el !== document.body) el.click();
    else $("playBtn").focus();
  },
  back,
  tabs: () => cycleTab(1),
};

const typing = (el) => !!(el && el.closest && el.closest("input, textarea, select, [contenteditable]"));

document.addEventListener("keydown", (event) => {
  const k = event.key, el = event.target;
  const button = el.closest && el.closest(".dd");
  if (button && ddKey(event, button)) return;
  if (k === "Escape") {
    event.preventDefault();
    return back();
  }
  if (event.ctrlKey || event.altKey || event.metaKey || typing(el)) return;
  if (k === "q" || k === "Q" || k === "e" || k === "E") {
    event.preventDefault();
    return cycleTab(k.toLowerCase() === "q" ? -1 : 1);
  }
  if (el.closest && el.closest("#tabs") && (k === "ArrowLeft" || k === "ArrowRight")) {
    event.preventDefault();
    return cycleTab(k === "ArrowLeft" ? -1 : 1);
  }
  // Up and down walk the Play rows like a pad's cursor, when focus is on them or nowhere.
  const onRows = el === document.body || (el.closest && el.closest("#tiles"));
  if (!ui.panel && onRows && (k === "ArrowDown" || k === "ArrowUp")) {
    event.preventDefault();
    moveRow(k === "ArrowDown" ? 1 : -1);
  }
});

// The highlight bar follows focus on the Play rows. The pointer moves focus with it, so the bar,
// the pane, the Enter hint and the Enter key all mean the same row. While the game picker or the
// leonida.exe steps are open, the pointer leaves focus where it is.
document.addEventListener("focusin", (event) => {
  const row = event.target.closest && event.target.closest("#tiles .mrow");
  if (row) highlight(row.id);
  renderHints();
});
document.addEventListener("mouseover", (event) => {
  const row = event.target.closest && event.target.closest("#tiles .mrow");
  if (!row || ui.hero) return;
  const target = row.id === "playTile" ? $("playBtn") : row;
  if (document.activeElement !== target) target.focus({ preventScroll: true });
});
// The arrows keep focus on their value, so the keys go on working after a click.
document.addEventListener("mousedown", (event) => {
  if (event.target.closest && event.target.closest(".arr, .hint")) event.preventDefault();
});
setInterval(renderClock, 20000);

// Coming back to the window also asks about updates; the back end goes to GitHub at most once an
// hour.
document.addEventListener("visibilitychange", () => {
  document.body.classList.toggle("still", document.hidden || ui.running);
  if (!document.hidden) {
    checkRunning();
    checkUpdates(false);
  }
});
window.addEventListener("focus", () => {
  checkRunning();
  checkUpdates(false);
});

try {
  $("advanced").open = localStorage.getItem("advancedOpen") === "1";
} catch (error) {
  // Starts closed.
}
// Focus goes to the test body when its screen changes.
$("testBody").tabIndex = -1;
buildScene();
render();
invoke("view")
  .then((view) => {
    V = view;
    render();
    // The highlight starts on START, or on the missing piece when there is one.
    const b = blocker();
    if (!ui.panel && (document.activeElement === document.body || document.activeElement === $("playBtn"))) {
      $(b && b.tile ? b.tile : "playBtn").focus({ preventScroll: true });
    }
    if (view.notice) problem(view.notice);
    checkRunning();
    refreshLists(false);
    checkUpdates(false);
  })
  .catch(problem);
