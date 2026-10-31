import { COPY_MODES, commandFor, nextMode } from "./commands.js";
import { ICONS, LANGS, MAX_FAVORITES, THEMES, defaults, normalize, placeKey, placeRef } from "./settings.js";

const { invoke } = window.__TAURI__.core;
const appWindow = window.__TAURI__.window.getCurrentWindow();

const KIND_LABEL = { coords: "Position", zaap: "Zaap", npc: "PNJ", marker: "Repère", custom: "Perso" };
const EMPTY_PLAN = { zaap: null, zaapSteps: null, walkSteps: null, takeZaap: false, gated: false };
const COORDS = /^\s*\[?\s*(-?\d+)\s*[,; ]\s*(-?\d+)\s*\]?\s*$/;

const $ = (id) => document.getElementById(id);
const els = Object.fromEntries(
  [
    "profile", "query", "from", "mode", "results", "empty", "banner", "footer", "toast", "menu",
    "settings", "theme", "lang", "antilag", "antilag-value", "opacity", "opacity-value", "customs-list", "fixes-list",
    "editor", "editor-form", "editor-title", "editor-fields", "editor-error",
  ].map((id) => [id, $(id)]),
);

let settings = defaults();
/** False when the settings file exists but could not be read: never save over it. */
let canSave = true;
let places = [];
let selected = 0;
let requestId = 0;
let showingFavorites = false;
let toastTimer;
let saveTimer;
let searchTimer;

// ---------- helpers

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function button(label, className, onClick, title) {
  const b = el("button", className, label);
  b.type = "button";
  if (title) b.title = title;
  b.addEventListener("click", (event) => {
    event.stopPropagation();
    onClick(event);
  });
  return b;
}

const steps = (n) => `${n} carte${n > 1 ? "s" : ""}`;
const normalizeText = (s) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
const profile = () => settings.profiles.find((p) => p.id === settings.profile) ?? settings.profiles[0];
const fixOf = (place) => settings.fixes[placeKey(place)];
const favoriteIndex = (place) => profile().favorites.findIndex((f) => placeKey(f) === placeKey(place));

function parseCoords(text) {
  const m = COORDS.exec(text);
  return m ? { x: Number(m[1]), y: Number(m[2]) } : null;
}

function toast(text, isError = false) {
  els.toast.textContent = text;
  els.toast.classList.toggle("error", isError);
  els.toast.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (els.toast.hidden = true), isError ? 4000 : 1800);
}

async function copy(text) {
  try {
    await navigator.clipboard.writeText(text);
    toast(`Copié : ${text}`);
  } catch (error) {
    toast(`Copie impossible : ${error.message ?? error}`, true);
  }
}

function copyPlace(place) {
  copy(commandFor(settings.copyMode, place, { antiLag: settings.antiLag, zaap: fixOf(place)?.zaap }));
}

// ---------- settings

function save() {
  clearTimeout(saveTimer);
  if (!canSave) return;
  saveTimer = setTimeout(async () => {
    try {
      await invoke("save_settings", { settings });
    } catch (error) {
      toast(`Réglages non enregistrés : ${error}`, true);
    }
  }, 300);
}

function change(mutate, { refresh = true } = {}) {
  mutate(settings);
  save();
  applyLook();
  renderProfiles();
  renderMode();
  if (els.settings.open) renderSettingsPanel();
  if (refresh) runSearch();
}

function applyLook() {
  document.documentElement.dataset.theme = settings.theme;
  document.documentElement.style.setProperty("--opacity", String(settings.opacity));
}

// ---------- places

async function planAt(x, y) {
  const plan = await invoke("plan_at", { x, y, from: settings.from || null, lang: settings.lang });
  return plan ?? EMPTY_PLAN;
}

/** Applies the user's correction, keeping the original position for favorites and restore. */
async function resolve(saved) {
  // A favorite custom position follows later edits of that position.
  const custom = saved.kind === "custom" && settings.customs.find((c) => c.id === saved.id);
  const place = custom ? customPlace(custom) : saved;
  const fix = fixOf(place);
  const base = { ...place, baseX: place.x, baseY: place.y, fixed: Boolean(fix) };
  if (fix && "x" in fix) return { ...base, x: fix.x, y: fix.y, plan: await planAt(fix.x, fix.y) };
  return base.plan ? base : { ...base, plan: await planAt(place.x, place.y) };
}

function customPlace(c) {
  return { kind: "custom", id: c.id, name: c.name, detail: "Position perso", mapId: 0, x: c.x, y: c.y, icon: c.icon, color: c.color };
}

async function placesFor(query) {
  if (!query.trim()) return Promise.all(profile().favorites.map(resolve));
  const q = normalizeText(query.trim());
  const customs = settings.customs.filter((c) => normalizeText(c.name).includes(q)).map(customPlace);
  const found = await invoke("search", { query, from: settings.from || null, lang: settings.lang });
  return Promise.all([...customs, ...found].map(resolve));
}

async function runSearch() {
  const id = ++requestId;
  const query = els.query.value;
  try {
    const result = await placesFor(query);
    if (id !== requestId) return;
    places = result;
    showingFavorites = !query.trim();
    selected = Math.min(selected, Math.max(0, places.length - 1));
    render();
  } catch (error) {
    toast(`Recherche impossible : ${error}`, true);
  }
}

function scheduleSearch() {
  clearTimeout(searchTimer);
  selected = 0;
  searchTimer = setTimeout(runSearch, 120);
}

// ---------- rendering

function routeText(place) {
  const forced = fixOf(place)?.zaap;
  const { zaap, zaapSteps, walkSteps, takeZaap, gated } = place.plan;
  if (forced) return `Zaap ${forced.name} [${forced.x},${forced.y}] (choisi)`;
  const gate = gated ? " · accès sous condition" : "";
  if (takeZaap && zaap) {
    return zaapSteps === 0 ? `Sur le zaap ${zaap.name}` : `Zaap ${zaap.name} [${zaap.x},${zaap.y}] · ${steps(zaapSteps)}${gate}`;
  }
  if (walkSteps != null) return `À pied : ${steps(walkSteps)}${gate}${zaap ? ` · zaap à ${steps(zaapSteps)}` : ""}`;
  return "Aucun zaap relié";
}

function badge(place) {
  if (place.kind !== "custom") return el("span", `kind ${place.kind}`, KIND_LABEL[place.kind]);
  const icon = el("span", "kind custom", place.icon);
  icon.style.setProperty("--custom", place.color);
  return icon;
}

function card(place, index) {
  const li = el("li", "card");
  li.classList.toggle("selected", index === selected);
  const fav = favoriteIndex(place);

  const title = el("div", "title");
  if (showingFavorites) title.append(el("span", "slot", String(index + 1)));
  title.append(badge(place), el("span", "name", place.name));
  if (place.fixed) title.append(el("span", "tag", "corrigé"));

  const meta = el("div", "meta", `${place.detail ? `${place.detail} · ` : ""}[${place.x},${place.y}]`);
  const actions = el("div", "actions");
  const modeLabel = COPY_MODES.find((m) => m.id === settings.copyMode)?.label ?? "Copier";
  actions.append(
    button(modeLabel, "btn primary", () => copyPlace(place), "Copier (Entrée)"),
    button(fav >= 0 ? "★" : "☆", `btn secondary star${fav >= 0 ? " on" : ""}`, () => toggleFavorite(place), fav >= 0 ? "Retirer des favoris" : "Ajouter aux favoris"),
    button("⋯", "btn secondary", (event) => openMenu(place, event.currentTarget), "Plus d'actions"),
  );
  const bottom = el("div", "bottom");
  bottom.append(el("span", "route", routeText(place)), actions);
  li.append(title, meta, bottom);

  li.addEventListener("click", () => {
    select(index);
    copyPlace(place);
  });
  li.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    select(index);
    openMenu(place, li);
  });
  if (showingFavorites) makeDraggable(li, index);
  return li;
}

function makeDraggable(li, index) {
  li.draggable = true;
  li.addEventListener("dragstart", (event) => {
    event.dataTransfer.setData("text/plain", String(index));
    li.classList.add("dragging");
  });
  li.addEventListener("dragend", () => li.classList.remove("dragging"));
  li.addEventListener("dragover", (event) => event.preventDefault());
  li.addEventListener("drop", (event) => {
    event.preventDefault();
    const from = Number(event.dataTransfer.getData("text/plain"));
    if (!Number.isInteger(from) || from === index) return;
    change((s) => {
      const favorites = s.profiles.find((p) => p.id === s.profile).favorites;
      favorites.splice(index, 0, ...favorites.splice(from, 1));
    });
  });
}

function render() {
  els.results.replaceChildren(...places.map(card));
  els.empty.hidden = places.length > 0;
  els.empty.textContent = showingFavorites
    ? "Tape un nom de PNJ, une zone, « zaap » ou une position. ☆ ajoute un favori (Alt + 1 à 6)."
    : "Aucun résultat.";
}

function select(index) {
  selected = Math.max(0, Math.min(index, places.length - 1));
  [...els.results.children].forEach((li, i) => li.classList.toggle("selected", i === selected));
  els.results.children[selected]?.scrollIntoView({ block: "nearest" });
}

function renderMode() {
  const mode = COPY_MODES.find((m) => m.id === settings.copyMode) ?? COPY_MODES[0];
  els.mode.textContent = `Copie : ${mode.label}`;
}

function renderProfiles() {
  els.profile.replaceChildren(...settings.profiles.map((p) => new Option(p.name, p.id)));
  els.profile.value = settings.profile;
}

// ---------- favorites, corrections, custom positions

function toggleFavorite(place) {
  const ref = placeRef({ ...place, x: place.baseX ?? place.x, y: place.baseY ?? place.y });
  const at = favoriteIndex(ref);
  if (at < 0 && profile().favorites.length >= MAX_FAVORITES) {
    toast(`${MAX_FAVORITES} favoris maximum par profil`, true);
    return;
  }
  change((s) => {
    const favorites = s.profiles.find((p) => p.id === s.profile).favorites;
    if (at >= 0) favorites.splice(at, 1);
    else favorites.push(ref);
  });
}

function setFix(place, patch) {
  change((s) => {
    const key = placeKey(place);
    const next = { ...s.fixes[key], name: place.name, ...patch };
    if (next.zaap === null) delete next.zaap;
    if ("x" in next || next.zaap) s.fixes[key] = next;
    else delete s.fixes[key];
  });
}

function restore(key) {
  change((s) => delete s.fixes[key]);
}

function editPosition(place) {
  openEditor({
    title: `Position de ${place.name}`,
    fields: [{ name: "pos", label: "Position (x,y)", type: "coords", value: `${place.x},${place.y}` }],
    onSave: ({ pos }) => setFix(place, pos),
  });
}

async function chooseZaap(place) {
  const zaaps = await invoke("zaaps", { lang: settings.lang });
  const current = fixOf(place)?.zaap;
  openEditor({
    title: `Zaap pour ${place.name}`,
    fields: [
      {
        name: "zaap",
        label: "Zaap",
        type: "select",
        value: current ? String(current.mapId) : "",
        options: [["", "Le plus proche (automatique)"], ...zaaps.map((z) => [String(z.mapId), `${z.name} [${z.x},${z.y}]`])],
      },
    ],
    onSave: ({ zaap }) => setFix(place, { zaap: zaaps.find((z) => String(z.mapId) === zaap) ?? null }),
  });
}

function editCustom(existing) {
  openEditor({
    title: existing ? "Modifier la position" : "Nouvelle position",
    fields: [
      { name: "name", label: "Nom", type: "text", value: existing?.name ?? "" },
      { name: "pos", label: "Position (x,y)", type: "coords", value: existing ? `${existing.x},${existing.y}` : settings.from },
      { name: "icon", label: "Icône", type: "select", value: existing?.icon ?? ICONS[0], options: ICONS.map((i) => [i, i]) },
      { name: "color", label: "Couleur", type: "color", value: existing?.color ?? "#f0ff4f" },
    ],
    onSave: ({ name, pos, icon, color }) =>
      change((s) => {
        const value = { id: existing?.id ?? `c${Date.now()}`, name: name || "Position", ...pos, icon, color };
        const at = s.customs.findIndex((c) => c.id === value.id);
        if (at >= 0) s.customs[at] = value;
        else s.customs.push(value);
      }),
  });
}

function deleteCustom(id) {
  change((s) => {
    s.customs = s.customs.filter((c) => c.id !== id);
    delete s.fixes[`custom:${id}`];
    for (const p of s.profiles) p.favorites = p.favorites.filter((f) => !(f.kind === "custom" && f.id === id));
  });
}

// ---------- menu

function closeMenu() {
  els.menu.hidden = true;
}

function openMenu(place, anchor) {
  const items = [];
  if (place.kind === "npc" || place.kind === "marker") items.push(["Corriger la position", () => editPosition(place)]);
  if (place.kind !== "zaap") items.push(["Choisir le zaap", () => chooseZaap(place)]);
  if (place.fixed) items.push(["Rétablir l'original", () => restore(placeKey(place))]);
  if (place.kind === "custom") {
    const c = settings.customs.find((x) => x.id === place.id);
    if (c) items.push(["Modifier", () => editCustom(c)], ["Supprimer", () => deleteCustom(c.id)]);
  }
  for (const mode of COPY_MODES) {
    items.push([`Copier : ${mode.label}`, () => copy(commandFor(mode.id, place, { antiLag: settings.antiLag, zaap: fixOf(place)?.zaap }))]);
  }
  els.menu.replaceChildren(
    ...items.map(([label, action]) => {
      const item = button(label, "menu-item", () => {
        closeMenu();
        action();
      });
      item.setAttribute("role", "menuitem");
      return item;
    }),
  );
  els.menu.hidden = false;
  const box = anchor.getBoundingClientRect();
  const top = Math.min(box.bottom + 4, window.innerHeight - els.menu.offsetHeight - 8);
  els.menu.style.top = `${Math.max(8, top)}px`;
  els.menu.style.left = `${Math.max(8, Math.min(box.right - els.menu.offsetWidth, window.innerWidth - els.menu.offsetWidth - 8))}px`;
  els.menu.querySelector("button")?.focus();
}

// ---------- editor dialog

let onEditorSave = null;

function openEditor({ title, fields, onSave }) {
  els["editor-title"].textContent = title;
  els["editor-error"].hidden = true;
  els["editor-fields"].replaceChildren(
    ...fields.map((f) => {
      const label = el("label", "field", f.label);
      let input;
      if (f.type === "select") {
        input = el("select");
        input.append(...f.options.map(([value, text]) => new Option(text, value)));
      } else {
        input = el("input");
        input.type = f.type === "color" ? "color" : "text";
        if (f.type === "coords") input.placeholder = "4,-18";
      }
      input.name = f.name;
      input.dataset.type = f.type;
      input.value = f.value ?? "";
      label.append(input);
      return label;
    }),
  );
  onEditorSave = onSave;
  els.editor.showModal();
  els["editor-fields"].querySelector("input, select")?.focus();
}

els["editor-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  const values = {};
  for (const input of els["editor-fields"].querySelectorAll("input, select")) {
    if (input.dataset.type === "coords") {
      const pos = parseCoords(input.value);
      if (!pos) {
        els["editor-error"].textContent = "Position attendue au format x,y (par exemple 4,-18).";
        els["editor-error"].hidden = false;
        input.focus();
        return;
      }
      values[input.name] = pos;
    } else {
      values[input.name] = input.value.trim();
    }
  }
  els.editor.close();
  onEditorSave?.(values);
});

// ---------- settings panel

function renderSettingsPanel() {
  els.theme.value = settings.theme;
  els.lang.value = settings.lang;
  els.antilag.value = String(settings.antiLag);
  els["antilag-value"].textContent = String(settings.antiLag);
  els.opacity.value = String(Math.round(settings.opacity * 100));
  els["opacity-value"].textContent = `${Math.round(settings.opacity * 100)} %`;
  $("profile-delete").disabled = settings.profiles.length < 2;

  els["customs-list"].replaceChildren(
    ...settings.customs.map((c) => {
      const li = el("li");
      const icon = el("span", "kind custom", c.icon);
      icon.style.setProperty("--custom", c.color);
      li.append(icon, el("span", "grow", `${c.name} [${c.x},${c.y}]`), button("Modifier", "btn secondary", () => editCustom(c)), button("Supprimer", "btn secondary", () => deleteCustom(c.id)));
      return li;
    }),
  );
  if (!settings.customs.length) els["customs-list"].append(el("li", "help", "Aucune position perso."));

  const fixes = Object.entries(settings.fixes);
  els["fixes-list"].replaceChildren(
    ...fixes.map(([key, fix]) => {
      const what = [fix.x !== undefined ? `[${fix.x},${fix.y}]` : "", fix.zaap ? `zaap ${fix.zaap.name}` : ""].filter(Boolean).join(" · ");
      const li = el("li");
      li.append(el("span", "grow", `${fix.name || key} → ${what}`), button("Rétablir", "btn secondary", () => restore(key)));
      return li;
    }),
  );
  if (!fixes.length) els["fixes-list"].append(el("li", "help", "Aucune correction. Clic droit sur un résultat pour en faire une."));
}

function askProfileName(title, value, onSave) {
  openEditor({ title, fields: [{ name: "name", label: "Nom", type: "text", value }], onSave: ({ name }) => name && onSave(name) });
}

$("profile-add").addEventListener("click", () =>
  askProfileName("Nouveau profil", "", (name) =>
    change((s) => {
      const id = `p${Date.now()}`;
      s.profiles.push({ id, name, favorites: [] });
      s.profile = id;
    }),
  ),
);
$("profile-rename").addEventListener("click", () =>
  askProfileName("Renommer le profil", profile().name, (name) => change(() => (profile().name = name), { refresh: false })),
);
$("profile-delete").addEventListener("click", () => {
  if (settings.profiles.length < 2 || !confirm(`Supprimer le profil « ${profile().name} » et ses favoris ?`)) return;
  change((s) => {
    s.profiles = s.profiles.filter((p) => p.id !== s.profile);
    s.profile = s.profiles[0].id;
  });
});
$("custom-add").addEventListener("click", () => editCustom(null));

for (const [value, label] of THEMES) els.theme.append(new Option(label, value));
for (const [value, label] of LANGS) els.lang.append(new Option(label, value));
els.theme.addEventListener("change", () => change((s) => (s.theme = els.theme.value), { refresh: false }));
els.lang.addEventListener("change", () => change((s) => (s.lang = els.lang.value)));
els.antilag.addEventListener("input", () => change((s) => (s.antiLag = Number(els.antilag.value)), { refresh: false }));
els.opacity.addEventListener("input", () => change((s) => (s.opacity = Number(els.opacity.value) / 100), { refresh: false }));

for (const dialog of [els.settings, els.editor]) {
  dialog.addEventListener("click", (event) => {
    if (event.target.closest("[data-close]")) dialog.close();
  });
}
$("open-settings").addEventListener("click", () => {
  renderSettingsPanel();
  els.settings.showModal();
});

// ---------- keyboard and window

els.profile.addEventListener("change", () => change((s) => (s.profile = els.profile.value)));
els.mode.addEventListener("click", () => {
  change((s) => (s.copyMode = nextMode(s.copyMode)), { refresh: false });
  render();
});
els.from.addEventListener("input", () => {
  change((s) => (s.from = els.from.value.trim()), { refresh: false });
  scheduleSearch();
});
els.query.addEventListener("input", scheduleSearch);
els.query.addEventListener("keydown", (event) => {
  if (event.key === "ArrowDown") select(selected + 1);
  else if (event.key === "ArrowUp") select(selected - 1);
  else if (event.key === "Enter" && places[selected]) copyPlace(places[selected]);
  else if (event.key === "Tab" && !event.ctrlKey && !event.altKey) {
    change((s) => (s.copyMode = nextMode(s.copyMode, event.shiftKey ? -1 : 1)), { refresh: false });
    render();
  } else if (event.key === "Escape") {
    els.query.value = "";
    scheduleSearch();
  } else return;
  event.preventDefault();
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && !els.menu.hidden) return closeMenu();
  const digit = /^Digit([1-6])$/.exec(event.code);
  if (!digit || els.settings.open || els.editor.open) return;
  const typing = event.target.matches?.("input, select, textarea");
  if (!event.altKey && typing) return;
  const favorite = profile().favorites[Number(digit[1]) - 1];
  if (!favorite) return;
  event.preventDefault();
  resolve(favorite).then(copyPlace);
});
document.addEventListener("click", (event) => {
  if (!els.menu.hidden && !els.menu.contains(event.target)) closeMenu();
});

$("close").addEventListener("click", () => appWindow.close());
$("minimize").addEventListener("click", () => appWindow.minimize());

// ---------- start

async function showStatus() {
  try {
    const s = await invoke("data_status");
    els.footer.textContent = `Dofus ${s.dataVersion} · ${s.npcs} PNJ · ${s.zaaps} zaaps · ${s.markers} repères`;
    els.footer.title = els.footer.textContent;
    if (s.installedVersion && s.installedVersion !== s.dataVersion) {
      els.banner.textContent = `Dofus est en ${s.installedVersion}, les données datent de ${s.dataVersion} : relance l'extraction.`;
      els.banner.hidden = false;
    }
    if (s.shortcutError) toast(`Raccourci Ctrl + Alt + P indisponible : ${s.shortcutError}`, true);
  } catch (error) {
    els.footer.textContent = `État des données indisponible : ${error}`;
  }
}

async function start() {
  try {
    const loaded = await invoke("load_settings");
    settings = normalize(loaded.settings);
    if (loaded.warning) toast(loaded.warning, true);
  } catch (error) {
    canSave = false;
    toast(`Réglages non lus, ils ne seront pas enregistrés : ${error}`, true);
  }
  els.from.value = settings.from;
  applyLook();
  renderProfiles();
  renderMode();
  await runSearch();
  showStatus();
}

start();
