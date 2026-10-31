import assert from "node:assert/strict";
import { test } from "node:test";
import { defaults, normalize, placeKey } from "../../ui/settings.js";

test("missing or broken settings give the defaults", () => {
  assert.deepEqual(normalize(null), defaults());
  assert.deepEqual(normalize("nope"), defaults());
});

test("values out of range are clamped or dropped", () => {
  const s = normalize({ antiLag: 9, opacity: 0.1, theme: "unknown", lang: "it" });
  assert.equal(s.antiLag, 5);
  assert.equal(s.opacity, 0.3);
  assert.equal(s.theme, "standard");
  assert.equal(s.lang, "fr");
});

test("favorites keep valid places only, six at most", () => {
  const good = { kind: "npc", id: 37, name: "Amine", detail: "Village", mapId: 9, x: 3, y: 1 };
  const s = normalize({
    profile: "p2",
    profiles: [{ id: "p2", name: "Iop", favorites: [good, { kind: "npc", x: "3" }, ...Array(8).fill(good)] }],
  });
  assert.equal(s.profile, "p2");
  assert.equal(s.profiles[0].favorites.length, 6);
  assert.deepEqual(s.profiles[0].favorites[0], good);
});

test("an unknown current profile falls back to the first one", () => {
  const s = normalize({ profile: "gone", profiles: [{ id: "a", favorites: [] }] });
  assert.equal(s.profile, "a");
});

test("custom positions need coordinates and get a safe colour", () => {
  const s = normalize({ customs: [{ id: "c1", name: "Champ", x: 1, y: 2, color: "red" }, { id: "c2", name: "Sans x" }] });
  assert.equal(s.customs.length, 1);
  assert.equal(s.customs[0].color, "#f0ff4f");
});

test("corrections keep a position or a zaap, nothing else", () => {
  const s = normalize({
    fixes: { "npc:1:2": { x: 4, y: 5 }, "npc:3:4": { zaap: { x: 1, y: 1 } }, "npc:5:6": { note: "x" } },
  });
  assert.deepEqual(Object.keys(s.fixes), ["npc:1:2", "npc:3:4"]);
});

test("place keys separate the positions of one NPC", () => {
  assert.notEqual(placeKey({ kind: "npc", id: 1, mapId: 2 }), placeKey({ kind: "npc", id: 1, mapId: 3 }));
  assert.equal(placeKey({ kind: "custom", id: "c1" }), "custom:c1");
});

test("a favorite custom position keeps its text id", () => {
  const s = normalize({ profiles: [{ id: "p1", favorites: [{ kind: "custom", id: "c42", name: "Champ", x: 1, y: 2 }] }] });
  assert.equal(s.profiles[0].favorites[0].id, "c42");
});
