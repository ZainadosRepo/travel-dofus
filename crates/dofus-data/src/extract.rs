//! Builds a [`World`] from a Dofus 3 install, read only.

use crate::i18n::Texts;
use crate::unity::{read_bundle, SerializedFile, MONO_BEHAVIOUR};
use crate::world::{Lang, MapInfo, Marker, Names, Npc, SubArea, World};
use crate::{err, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

/// Quest objective types whose first parameter is an NPC to meet ("Aller voir", "Ramener à"…).
const NPC_OBJECTIVE_TYPES: [i64; 6] = [1, 2, 3, 9, 10, 12];

/// Where the Ankama Launcher installs Dofus 3, when the platform location is known.
pub fn default_game_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(|d| PathBuf::from(d).join("Ankama").join("Dofus-dofus3"))
    } else {
        // shortcut: install path unknown outside Windows, pass the folder to the extractor explicitly.
        None
    }
}

fn content(game: &Path) -> PathBuf {
    game.join("Dofus_Data")
        .join("StreamingAssets")
        .join("Content")
}

/// The `Version=` line of `StreamingAssets/version`, e.g. `3.7.11.11`.
pub fn installed_version(game: &Path) -> Result<String> {
    let text = fs::read_to_string(
        game.join("Dofus_Data")
            .join("StreamingAssets")
            .join("version"),
    )?;
    match text.lines().find_map(|l| l.strip_prefix("Version=")) {
        Some(v) => Ok(v.trim().to_owned()),
        None => err("no Version= line in StreamingAssets/version"),
    }
}

/// Rows of a `<Table>DataRoot` bundle, as their `[SerializeReference]` data objects.
fn table(game: &Path, name: &str) -> Result<Vec<Value>> {
    let path = content(game)
        .join("Data")
        .join(format!("data_assets_{name}dataroot.asset.bundle"));
    let bytes = fs::read(&path).map_err(|e| crate::Error(format!("{}: {e}", path.display())))?;
    for file in read_bundle(&bytes)? {
        let Ok(sf) = SerializedFile::parse(&file.data) else {
            continue;
        };
        for o in &sf.objects {
            if sf.class_id(o) != MONO_BEHAVIOUR {
                continue;
            }
            let root = sf
                .read(o)
                .map_err(|e| crate::Error(format!("{name}: {e}")))?;
            let Some(refs) = root.pointer("/references/RefIds").and_then(Value::as_array) else {
                return err(format!("{name}: no references registry"));
            };
            return Ok(refs.iter().filter_map(|r| r.get("data").cloned()).collect());
        }
    }
    err(format!("{name}: no DataRoot object"))
}

fn int(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or_default()
}

/// Walking links between maps, from the map bundles: `(map id, [top, bottom, left, right])`.
fn map_neighbours(game: &Path) -> Result<HashMap<i64, Vec<i64>>> {
    let dir = content(game).join("Map").join("Data");
    let mut out = HashMap::new();
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        let is_map = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("mapdata_assets_world_"));
        if !is_map {
            continue;
        }
        let context = |e: crate::Error| crate::Error(format!("{}: {e}", path.display()));
        let bytes = fs::read(&path)?;
        for file in read_bundle(&bytes).map_err(context)? {
            let Ok(sf) = SerializedFile::parse(&file.data) else {
                continue;
            };
            for o in &sf.objects {
                if sf.class_id(o) != MONO_BEHAVIOUR {
                    continue;
                }
                let f = sf
                    .read_fields_until(o, "rightNeighbourId")
                    .map_err(context)?;
                // Two layouts ship side by side: fields at the top level with an `id`, or nested in
                // `mapData` with the id only in the object name (`map_<id>`).
                let from_name = || {
                    f.get("m_Name")?
                        .as_str()?
                        .strip_prefix("map_")?
                        .parse()
                        .ok()
                };
                let Some(id) = f.get("id").and_then(Value::as_i64).or_else(from_name) else {
                    continue;
                };
                let f = f.get("mapData").and_then(Value::as_object).unwrap_or(&f);
                let n = [
                    "topNeighbourId",
                    "bottomNeighbourId",
                    "leftNeighbourId",
                    "rightNeighbourId",
                ]
                .iter()
                .filter_map(|k| f.get(*k).and_then(Value::as_i64))
                .filter(|&m| m > 0)
                .collect();
                out.insert(id, n);
            }
        }
    }
    Ok(out)
}

pub fn extract(game: &Path) -> Result<World> {
    let texts = Lang::ALL
        .iter()
        .map(|lang| {
            let path = content(game)
                .join("I18n")
                .join(format!("{}.bin", lang.code()));
            Texts::parse(fs::read(&path)?)
        })
        .collect::<Result<Vec<_>>>()?;
    let name = |row: &Value, key: &str| {
        let id = int(row, key);
        let [fr, en, de, es, pt] = [0, 1, 2, 3, 4].map(|i| texts[i].get(id).unwrap_or_default());
        Names { fr, en, de, es, pt }
    };

    let mut neighbours = map_neighbours(game)?;
    for row in table(game, "mapscrollactions")? {
        let id = int(&row, "id");
        for side in ["top", "bottom", "left", "right"] {
            let target = int(&row, &format!("{side}MapId"));
            if int(&row, &format!("{side}Exists")) != 0 && target > 0 {
                neighbours.entry(id).or_default().push(target);
            }
        }
    }

    let maps: Vec<MapInfo> = table(game, "mapsinformation")?
        .iter()
        .map(|m| {
            let id = int(m, "id");
            let mut n = neighbours.remove(&id).unwrap_or_default();
            n.sort_unstable();
            n.dedup();
            MapInfo {
                id,
                x: int(m, "posX") as i32,
                y: int(m, "posY") as i32,
                world_map: int(m, "worldMap") as i32,
                sub_area: int(m, "subAreaId"),
                neighbours: n,
            }
        })
        .collect();

    let areas: HashMap<i64, Names> = table(game, "areas")?
        .iter()
        .map(|a| (int(a, "id"), name(a, "nameId")))
        .collect();
    let sub_areas = table(game, "subareas")?
        .iter()
        .map(|s| SubArea {
            id: int(s, "id"),
            names: name(s, "nameId"),
            area: areas.get(&int(s, "areaId")).cloned().unwrap_or_default(),
        })
        .collect();

    let zaaps = table(game, "waypoints")?
        .iter()
        .filter(|w| int(w, "activated") != 0)
        .map(|w| int(w, "mapId"))
        .collect();

    let mut npc_maps: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
    for q in table(game, "quests")? {
        for start in q
            .get("startPosition")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            npc_maps
                .entry(int(start, "npcId"))
                .or_default()
                .insert(int(start, "mapId"));
        }
    }
    for o in table(game, "questobjectives")? {
        let (map, npc) = (
            int(&o, "mapId"),
            o.get("parameters").map_or(0, |p| int(p, "parameter0")),
        );
        if NPC_OBJECTIVE_TYPES.contains(&int(&o, "typeId")) && map > 0 && npc > 0 {
            npc_maps.entry(npc).or_default().insert(map);
        }
    }
    let npcs = table(game, "npcs")?
        .iter()
        .filter_map(|n| {
            let maps: Vec<i64> = npc_maps
                .get(&int(n, "id"))?
                .iter()
                .copied()
                .filter(|&m| m > 0)
                .collect();
            let names = name(n, "nameId");
            (!maps.is_empty() && !names.is_empty()).then(|| Npc {
                id: int(n, "id"),
                names,
                maps,
            })
        })
        .collect();

    let categories: HashMap<i64, Names> = table(game, "hintcategories")?
        .iter()
        .map(|c| (int(c, "id"), name(c, "nameId")))
        .collect();
    let markers = table(game, "hints")?
        .iter()
        .map(|h| Marker {
            names: name(h, "nameId"),
            category: categories
                .get(&int(h, "categoryId"))
                .cloned()
                .unwrap_or_default(),
            map_id: int(h, "mapId"),
            x: int(h, "x") as i32,
            y: int(h, "y") as i32,
        })
        .filter(|m| !m.names.is_empty())
        .collect();

    Ok(World {
        game_version: installed_version(game)?,
        maps,
        sub_areas,
        zaaps,
        npcs,
        markers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs a local Dofus 3 install: `cargo test -p dofus-data -- --ignored`.
    #[test]
    #[ignore]
    fn extracts_the_installed_game() {
        let game = default_game_dir().expect("Dofus install path");
        let world = extract(&game).unwrap();
        assert!(world.zaaps.contains(&191_105_026), "Astrub zaap");
        assert!(world.npcs.len() > 1000);
        assert!(
            world
                .maps
                .iter()
                .filter(|m| !m.neighbours.is_empty())
                .count()
                > 10_000
        );
    }
}
