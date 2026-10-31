import assert from "node:assert/strict";
import { test } from "node:test";
import { commandFor, nextMode } from "../../ui/commands.js";

const zaap = { mapId: 1, x: -2, y: 0, name: "Village d'Amakna" };
const npc = { name: "Amine", x: 3, y: 1, plan: { zaap, zaapSteps: 6, walkSteps: null, takeZaap: true } };

test("route repeats /zaap anti-lag times before /travel", () => {
  assert.equal(commandFor("go", npc, { antiLag: 2 }), "/zaap -2,0;/zaap -2,0;/travel 3,1");
  assert.equal(commandFor("go", npc, { antiLag: 1 }), "/zaap -2,0;/travel 3,1");
});

test("route walks when the plan says walking wins", () => {
  const near = { ...npc, plan: { ...npc.plan, takeZaap: false, walkSteps: 2 } };
  assert.equal(commandFor("go", near, { antiLag: 2 }), "/travel 3,1");
  assert.equal(commandFor("both", near, { antiLag: 2 }), "/zaap -2,0;/zaap -2,0;/travel 3,1");
});

test("a zaap forced for the place wins over the plan", () => {
  const forced = { mapId: 2, x: 5, y: -18, name: "Astrub" };
  const near = { ...npc, plan: { ...npc.plan, takeZaap: false } };
  assert.equal(commandFor("go", near, { antiLag: 1, zaap: forced }), "/zaap 5,-18;/travel 3,1");
  assert.equal(commandFor("zaap", near, { antiLag: 1, zaap: forced }), "/zaap 5,-18");
});

test("standing on the zaap needs only /zaap", () => {
  const onZaap = { name: "Village", x: -2, y: 0, plan: { ...npc.plan, zaapSteps: 0 } };
  assert.equal(commandFor("go", onZaap, { antiLag: 3 }), "/zaap -2,0");
});

test("without any zaap every route falls back to /travel", () => {
  const island = { name: "Île", x: 9, y: 9, plan: { zaap: null, zaapSteps: null, walkSteps: null, takeZaap: false } };
  assert.equal(commandFor("go", island, { antiLag: 2 }), "/travel 9,9");
  assert.equal(commandFor("zaap", island, { antiLag: 2 }), "/travel 9,9");
});

test("position and name modes", () => {
  assert.equal(commandFor("pos", npc, { antiLag: 2 }), "[3,1]");
  assert.equal(commandFor("name", npc, { antiLag: 2 }), "Amine");
});

test("copy modes cycle both ways", () => {
  assert.equal(nextMode("go"), "both");
  assert.equal(nextMode("name"), "go");
  assert.equal(nextMode("go", -1), "name");
});
