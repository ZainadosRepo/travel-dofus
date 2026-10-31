// Shape of settings.json. What comes back from disk is checked here; the rest of the UI trusts it.

export const THEMES = [
  ["standard", "Standard"],
  ["bonta", "Bonta"],
  ["brakmar", "Brâkmar"],
  ["tribute", "Tribute"],
  ["goldandsteel", "Gold and Steel"],
  ["belladone", "Belladone"],
  ["unicorn", "Unicorn"],
  ["emeraldmine", "Emerald Mine"],
  ["sufokia", "Sufokia"],
  ["pandala", "Pandala"],
  ["wabbit", "Wabbit"],
];
export const LANGS = [
  ["fr", "Français"],
  ["en", "English"],
  ["de", "Deutsch"],
  ["es", "Español"],
  ["pt", "Português"],
];
export const ICONS = ["📍", "🏠", "🏦", "⚒️", "🌾", "🌲", "⛏️", "🐟", "⚔️", "🐉", "💰", "🧪", "🛡️", "🐎", "⭐", "❤️", "🚩"];
export const MAX_FAVORITES = 6;
const KINDS = ["coords", "zaap", "npc", "marker", "custom"];
const COLOR = /^#[0-9a-f]{6}$/i;

export function defaults() {
  return {
    theme: "standard",
    lang: "fr",
    antiLag: 2,
    opacity: 1,
    copyMode: "go",
    from: "",
    profile: "p1",
    profiles: [{ id: "p1", name: "Principal", favorites: [] }],
    customs: [],
    fixes: {},
  };
}

const str = (v, fallback = "") => (typeof v === "string" ? v : fallback);
const int = (v) => (Number.isInteger(v) ? v : null);
const oneOf = (v, list, fallback) => (list.includes(v) ? v : fallback);
const clamp = (v, min, max, fallback) => (typeof v === "number" && Number.isFinite(v) ? Math.min(max, Math.max(min, v)) : fallback);

/** A saved place (favorite): what is needed to show it and route to it again. */
export function placeRef(v) {
  if (!v || typeof v !== "object" || !KINDS.includes(v.kind)) return null;
  const x = int(v.x);
  const y = int(v.y);
  if (x === null || y === null) return null;
  const id = Number.isInteger(v.id) || (typeof v.id === "string" && v.id) ? v.id : 0;
  return { kind: v.kind, id, name: str(v.name), detail: str(v.detail), mapId: int(v.mapId) ?? 0, x, y };
}

function zaapRef(v) {
  if (!v || typeof v !== "object" || int(v.x) === null || int(v.y) === null) return null;
  return { mapId: int(v.mapId) ?? 0, x: v.x, y: v.y, name: str(v.name) };
}

function custom(v) {
  if (!v || typeof v !== "object" || !str(v.id) || int(v.x) === null || int(v.y) === null) return null;
  return {
    id: v.id,
    name: str(v.name, "Position"),
    x: v.x,
    y: v.y,
    icon: oneOf(v.icon, ICONS, ICONS[0]),
    color: COLOR.test(str(v.color)) ? v.color : "#f0ff4f",
  };
}

function fix(v) {
  if (!v || typeof v !== "object") return null;
  const out = { name: str(v.name) };
  if (int(v.x) !== null && int(v.y) !== null) Object.assign(out, { x: v.x, y: v.y });
  const zaap = zaapRef(v.zaap);
  if (zaap) out.zaap = zaap;
  return "x" in out || out.zaap ? out : null;
}

export function normalize(raw) {
  const d = defaults();
  if (!raw || typeof raw !== "object") return d;
  const profiles = (Array.isArray(raw.profiles) ? raw.profiles : [])
    .filter((p) => p && typeof p === "object" && str(p.id))
    .map((p) => ({
      id: p.id,
      name: str(p.name, "Profil") || "Profil",
      favorites: (Array.isArray(p.favorites) ? p.favorites : []).map(placeRef).filter(Boolean).slice(0, MAX_FAVORITES),
    }));
  if (profiles.length === 0) profiles.push(...d.profiles);
  const fixes = {};
  for (const [key, value] of Object.entries(raw.fixes && typeof raw.fixes === "object" ? raw.fixes : {})) {
    const f = fix(value);
    if (f) fixes[key] = f;
  }
  return {
    theme: oneOf(raw.theme, THEMES.map(([id]) => id), d.theme),
    lang: oneOf(raw.lang, LANGS.map(([id]) => id), d.lang),
    antiLag: Math.round(clamp(raw.antiLag, 1, 5, d.antiLag)),
    opacity: clamp(raw.opacity, 0.3, 1, d.opacity),
    copyMode: str(raw.copyMode, d.copyMode),
    from: str(raw.from),
    profile: profiles.some((p) => p.id === raw.profile) ? raw.profile : profiles[0].id,
    profiles,
    customs: (Array.isArray(raw.customs) ? raw.customs : []).map(custom).filter(Boolean),
    fixes,
  };
}

/** Key of a place in `fixes` and in favorites. */
export function placeKey(place) {
  return `${place.kind}:${place.kind === "custom" ? place.id : `${place.id}:${place.mapId}`}`;
}
