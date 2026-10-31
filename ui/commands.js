// Turns a place and its plan into the text the copy action puts in the clipboard.

export const COPY_MODES = [
  { id: "go", label: "Trajet" },
  { id: "both", label: "/zaap + /travel" },
  { id: "travel", label: "/travel" },
  { id: "zaap", label: "/zaap" },
  { id: "pos", label: "Position" },
  { id: "name", label: "Nom" },
];

export function nextMode(id, step = 1) {
  const i = COPY_MODES.findIndex((m) => m.id === id);
  return COPY_MODES[(i + step + COPY_MODES.length) % COPY_MODES.length].id;
}

/** The zaap to use: the one forced for this place, else the plan's nearest. */
export function zaapOf(place, forced) {
  return forced ?? place.plan?.zaap ?? null;
}

function route(place, zaap, antiLag) {
  const travel = `/travel ${place.x},${place.y}`;
  if (!zaap) return travel;
  const hop = `/zaap ${zaap.x},${zaap.y}`;
  if (zaap.x === place.x && zaap.y === place.y) return hop;
  // The repeated /zaap gives the destination map time to load before /travel runs.
  return [...Array(antiLag).fill(hop), travel].join(";");
}

/**
 * @param {string} mode one of COPY_MODES ids
 * @param {{name: string, x: number, y: number, plan?: object}} place
 * @param {{antiLag: number, zaap?: object}} options `zaap` forces a zaap for this place
 */
export function commandFor(mode, place, { antiLag, zaap: forced }) {
  const zaap = zaapOf(place, forced);
  switch (mode) {
    case "name":
      return place.name;
    case "pos":
      return `[${place.x},${place.y}]`;
    case "travel":
      return `/travel ${place.x},${place.y}`;
    case "zaap":
      return zaap ? `/zaap ${zaap.x},${zaap.y}` : `/travel ${place.x},${place.y}`;
    case "both":
      return route(place, zaap, antiLag);
    default:
      return route(place, forced || place.plan?.takeZaap ? zaap : null, antiLag);
  }
}
