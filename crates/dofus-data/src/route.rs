//! Walking graph between map zones and the zaap-or-walk plan to reach a map.

use crate::world::{Lang, MapInfo, Names, World};
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// Surface world map, the one `/travel` coordinates refer to.
const SURFACE: i32 = 1;
/// shortcut: a zaap hop is priced as a fixed number of walked maps (loading + click), tune if routes feel off.
const ZAAP_COST: u32 = 5;
/// Added to a path's cost for each gated move, so a path anyone can walk always wins; the low bits
/// still count the maps walked.
const GATE: u32 = 1 << 16;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ZaapStop {
    pub map_id: i64,
    pub x: i32,
    pub y: i32,
    pub name: String,
}

/// How to reach a map. The UI turns it into `/zaap` and `/travel` commands.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Zaap with the shortest walk to the target, when one is connected.
    pub zaap: Option<ZaapStop>,
    /// Maps to walk from that zaap.
    pub zaap_steps: Option<u32>,
    /// Maps to walk from the start position, when one was given and is connected.
    pub walk_steps: Option<u32>,
    /// Whether going through the zaap beats walking from the start position.
    pub take_zaap: bool,
    /// Whether the chosen route needs a gated move (quest, item…): other dimensions, islands…
    pub gated: bool,
}

pub struct Atlas {
    pub world: World,
    index: HashMap<i64, usize>,
    /// Graph vertices (walkable zones) of each map; empty for maps the world graph leaves out.
    zones: Vec<Vec<usize>>,
    /// Per vertex: `(next vertex, cost)`.
    adjacency: Vec<Vec<(usize, u32)>>,
    /// Per vertex: `(zaap map index, cost)` of the cheapest zaap leading there.
    nearest_zaap: Vec<Option<(usize, u32)>>,
    surface_at: HashMap<(i32, i32), usize>,
    sub_area_names: HashMap<i64, Names>,
}

/// Cheapest cost from `sources` (`(vertex, origin)`) to every vertex, with the origin it came from.
fn shortest(
    adjacency: &[Vec<(usize, u32)>],
    sources: &[(usize, usize)],
) -> Vec<Option<(usize, u32)>> {
    let mut best: Vec<Option<(usize, u32)>> = vec![None; adjacency.len()];
    let mut heap = BinaryHeap::new();
    for &(v, origin) in sources {
        if best[v].is_none() {
            best[v] = Some((origin, 0));
            heap.push(Reverse((0, v)));
        }
    }
    while let Some(Reverse((cost, v))) = heap.pop() {
        let (origin, known) = best[v].expect("queued vertices are reached");
        if cost > known {
            continue;
        }
        for &(n, step) in &adjacency[v] {
            let c = cost + step;
            if best[n].is_none_or(|(_, k)| c < k) {
                best[n] = Some((origin, c));
                heap.push(Reverse((c, n)));
            }
        }
    }
    best
}

impl Atlas {
    pub fn new(world: World) -> Self {
        let index: HashMap<i64, usize> = world
            .maps
            .iter()
            .enumerate()
            .map(|(i, m)| (m.id, i))
            .collect();
        let mut vertex: HashMap<(usize, u8), usize> = HashMap::new();
        let mut zones = vec![Vec::new(); world.maps.len()];
        let mut adjacency: Vec<Vec<(usize, u32)>> = Vec::new();
        let mut vertex_of = |map: usize, zone: u8, adjacency: &mut Vec<Vec<(usize, u32)>>| {
            *vertex.entry((map, zone)).or_insert_with(|| {
                adjacency.push(Vec::new());
                zones[map].push(adjacency.len() - 1);
                adjacency.len() - 1
            })
        };
        for (i, m) in world.maps.iter().enumerate() {
            for e in &m.exits {
                let Some(&to) = index.get(&e.1) else {
                    continue;
                };
                let from = vertex_of(i, e.0, &mut adjacency);
                let to = vertex_of(to, e.2, &mut adjacency);
                adjacency[from].push((to, if e.3 { GATE + 1 } else { 1 }));
            }
        }
        for list in &mut adjacency {
            // Keep the cheapest of duplicate moves.
            list.sort_unstable();
            list.dedup_by_key(|(to, _)| *to);
        }
        let mut surface_at: HashMap<(i32, i32), usize> = HashMap::new();
        for (i, m) in world
            .maps
            .iter()
            .enumerate()
            .filter(|(_, m)| m.world_map == SURFACE)
        {
            // Several maps can share coordinates; the best connected one is the outdoor map.
            let better = surface_at
                .get(&(m.x, m.y))
                .is_none_or(|&j| m.exits.len() > world.maps[j].exits.len());
            if better {
                surface_at.insert((m.x, m.y), i);
            }
        }
        let zaaps: Vec<(usize, usize)> = world
            .zaaps
            .iter()
            .filter_map(|z| index.get(z))
            .flat_map(|&m| zones[m].iter().map(move |&v| (v, m)))
            .collect();
        let nearest_zaap = shortest(&adjacency, &zaaps);
        let sub_area_names = world
            .sub_areas
            .iter()
            .map(|s| (s.id, s.names.clone()))
            .collect();
        Atlas {
            world,
            index,
            zones,
            adjacency,
            nearest_zaap,
            surface_at,
            sub_area_names,
        }
    }

    pub fn map(&self, id: i64) -> Option<&MapInfo> {
        self.index.get(&id).map(|&i| &self.world.maps[i])
    }

    pub fn sub_area_names(&self, map: &MapInfo) -> Option<&Names> {
        self.sub_area_names.get(&map.sub_area)
    }

    pub fn sub_area_name(&self, map: &MapInfo, lang: Lang) -> &str {
        self.sub_area_names(map).map_or("", |n| n.get(lang))
    }

    pub fn surface_map_at(&self, x: i32, y: i32) -> Option<&MapInfo> {
        self.surface_at.get(&(x, y)).map(|&i| &self.world.maps[i])
    }

    /// Walking costs from one map to every vertex, for comparing against a zaap.
    pub fn distances_from(&self, map_id: i64) -> Option<Vec<Option<u32>>> {
        let start = *self.index.get(&map_id)?;
        // The zone the player stands in is unknown: any zone of the map may start the walk.
        let sources: Vec<(usize, usize)> = self.zones[start].iter().map(|&v| (v, start)).collect();
        if sources.is_empty() {
            return None;
        }
        Some(
            shortest(&self.adjacency, &sources)
                .into_iter()
                .map(|b| b.map(|(_, c)| c))
                .collect(),
        )
    }

    fn stop(&self, map: &MapInfo, lang: Lang) -> ZaapStop {
        ZaapStop {
            map_id: map.id,
            x: map.x,
            y: map.y,
            name: self.sub_area_name(map, lang).to_owned(),
        }
    }

    /// Every active zaap, for picking one by hand.
    pub fn zaaps(&self, lang: Lang) -> Vec<ZaapStop> {
        let mut out: Vec<ZaapStop> = self
            .world
            .zaaps
            .iter()
            .filter_map(|&z| self.map(z))
            .map(|m| self.stop(m, lang))
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn plan(&self, target: &MapInfo, from: Option<&[Option<u32>]>, lang: Lang) -> Plan {
        let mut i = self.index[&target.id];
        // A map the world graph leaves out is reached through the surface map at its coordinates.
        if self.zones[i].is_empty() {
            i = self
                .surface_at
                .get(&(target.x, target.y))
                .copied()
                .unwrap_or(i);
        }
        let zones = &self.zones[i];
        let zaap = zones
            .iter()
            .filter_map(|&v| self.nearest_zaap[v])
            .min_by_key(|&(_, c)| c);
        let walk = from.and_then(|d| zones.iter().filter_map(|&v| d[v]).min());
        let take_zaap = match (zaap, walk) {
            (Some((_, z)), Some(w)) => z + ZAAP_COST < w,
            (Some(_), None) => true,
            (None, _) => false,
        };
        let chosen = if take_zaap {
            zaap.map(|(_, c)| c)
        } else {
            walk
        };
        Plan {
            zaap: zaap.map(|(z, _)| self.stop(&self.world.maps[z], lang)),
            zaap_steps: zaap.map(|(_, c)| c % GATE),
            walk_steps: walk.map(|c| c % GATE),
            take_zaap,
            gated: chosen.is_some_and(|c| c >= GATE),
        }
    }

    /// Plan to the surface map at `x,y` (custom positions, corrected NPCs).
    pub fn plan_at(
        &self,
        x: i32,
        y: i32,
        from: Option<&[Option<u32>]>,
        lang: Lang,
    ) -> Option<Plan> {
        self.surface_map_at(x, y).map(|m| self.plan(m, from, lang))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::world::{Exit, SubArea};

    pub(crate) fn names(fr: &str) -> Names {
        Names {
            fr: fr.into(),
            ..Default::default()
        }
    }

    fn map(id: i64, x: i32, exits: Vec<Exit>) -> MapInfo {
        MapInfo {
            id,
            x,
            y: 0,
            world_map: SURFACE,
            sub_area: 1,
            exits,
        }
    }

    /// Eight surface maps in a row at y = 0, x = 0..7, linked both ways; zaap on x = 0.
    pub(crate) fn line_world() -> World {
        let maps = (0..8)
            .map(|x| {
                let mut exits = Vec::new();
                if x > 0 {
                    exits.push(Exit(1, 99 + x, 1, false));
                }
                if x < 7 {
                    exits.push(Exit(1, 101 + x, 1, false));
                }
                map(100 + x, x as i32, exits)
            })
            .collect();
        World {
            game_version: "test".into(),
            maps,
            sub_areas: vec![SubArea {
                id: 1,
                names: names("Astrub"),
                area: names("Astrub"),
            }],
            zaaps: vec![100],
            npcs: vec![],
            markers: vec![],
        }
    }

    fn steps_to(atlas: &Atlas, id: i64) -> Option<u32> {
        atlas
            .plan(atlas.map(id).unwrap(), None, Lang::Fr)
            .zaap_steps
    }

    #[test]
    fn takes_the_zaap_without_a_start_position() {
        let atlas = Atlas::new(line_world());
        let plan = atlas.plan(atlas.map(107).unwrap(), None, Lang::Fr);
        assert_eq!(
            plan.zaap.as_ref().map(|z| (z.map_id, z.name.as_str())),
            Some((100, "Astrub"))
        );
        assert_eq!(plan.zaap_steps, Some(7));
        assert!(plan.take_zaap);
    }

    #[test]
    fn walks_when_the_target_is_close() {
        let atlas = Atlas::new(line_world());
        let from = atlas.distances_from(105).unwrap();
        let plan = atlas.plan(atlas.map(107).unwrap(), Some(&from), Lang::Fr);
        assert!(!plan.take_zaap);
        assert_eq!(plan.walk_steps, Some(2));
        assert_eq!(
            plan.zaap_steps,
            Some(7),
            "the zaap stays available for the zaap copy mode"
        );
    }

    #[test]
    fn one_way_moves_are_not_walked_back() {
        let mut w = line_world();
        // A drop from 200 lands on 107, with no way up.
        w.maps.push(map(200, 8, vec![Exit(1, 107, 1, false)]));
        let atlas = Atlas::new(w);
        assert_eq!(steps_to(&atlas, 200), None);
    }

    #[test]
    fn a_wall_splits_a_map_into_zones() {
        let mut w = line_world();
        w.maps[7].exits.push(Exit(1, 108, 1, false));
        w.maps.push(map(108, 8, vec![Exit(1, 107, 1, false)]));
        // Zone 2 of 108 only opens onto 109.
        w.maps[8].exits.push(Exit(2, 109, 1, false));
        w.maps.push(map(109, 9, vec![Exit(1, 108, 2, false)]));
        let atlas = Atlas::new(w);
        assert_eq!(steps_to(&atlas, 108), Some(8));
        assert_eq!(
            steps_to(&atlas, 109),
            None,
            "no crossing from zone 1 to zone 2"
        );
    }

    #[test]
    fn gated_moves_only_when_nothing_else_reaches() {
        let mut w = line_world();
        // A gated shortcut from the zaap to 107, and a dead end only reached through a gate.
        w.maps[0].exits.push(Exit(1, 107, 1, true));
        w.maps[0].exits.push(Exit(1, 300, 1, true));
        w.maps.push(map(300, -1, vec![]));
        let atlas = Atlas::new(w);
        assert_eq!(steps_to(&atlas, 107), Some(7), "the open path wins");
        assert!(!atlas.plan(atlas.map(107).unwrap(), None, Lang::Fr).gated);
        assert_eq!(steps_to(&atlas, 300), Some(1));
        assert!(atlas.plan(atlas.map(300).unwrap(), None, Lang::Fr).gated);
    }

    #[test]
    fn map_outside_the_graph_uses_the_surface_map() {
        let mut w = line_world();
        w.maps.push(MapInfo {
            world_map: -1,
            ..map(400, 6, vec![])
        });
        let atlas = Atlas::new(w);
        assert_eq!(steps_to(&atlas, 400), Some(6));
    }

    #[test]
    fn plans_to_bare_coordinates() {
        let atlas = Atlas::new(line_world());
        assert_eq!(
            atlas
                .plan_at(4, 0, None, Lang::Fr)
                .and_then(|p| p.zaap_steps),
            Some(4)
        );
        assert_eq!(atlas.plan_at(40, 40, None, Lang::Fr), None);
    }

    #[test]
    fn lists_zaaps_by_name() {
        let atlas = Atlas::new(line_world());
        assert_eq!(
            atlas
                .zaaps(Lang::Fr)
                .iter()
                .map(|z| z.map_id)
                .collect::<Vec<_>>(),
            vec![100]
        );
    }
}
