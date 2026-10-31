//! Walking graph between maps and the zaap-or-walk plan to reach a map.

use crate::world::{Lang, MapInfo, Names, World};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};

/// Surface world map, the one `/travel` coordinates refer to.
const SURFACE: i32 = 1;
/// shortcut: a zaap hop is priced as a fixed number of walked maps (loading + click), tune if routes feel off.
const ZAAP_COST: u32 = 5;

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
}

pub struct Atlas {
    pub world: World,
    index: HashMap<i64, usize>,
    adjacency: Vec<Vec<usize>>,
    nearest_zaap: Vec<Option<(usize, u32)>>,
    surface_at: HashMap<(i32, i32), usize>,
    sub_area_names: HashMap<i64, Names>,
}

fn bfs(adjacency: &[Vec<usize>], sources: &[usize]) -> Vec<Option<(usize, u32)>> {
    let mut best = vec![None; adjacency.len()];
    let mut queue = VecDeque::new();
    for &s in sources {
        if best[s].is_none() {
            best[s] = Some((s, 0));
            queue.push_back(s);
        }
    }
    while let Some(m) = queue.pop_front() {
        let (origin, d) = best[m].expect("queued maps are reached");
        for &n in &adjacency[m] {
            if best[n].is_none() {
                best[n] = Some((origin, d + 1));
                queue.push_back(n);
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
        // Map edges are stored one way; walking works both ways.
        let mut adjacency = vec![Vec::new(); world.maps.len()];
        for (i, m) in world.maps.iter().enumerate() {
            for n in m.neighbours.iter().filter_map(|n| index.get(n)) {
                adjacency[i].push(*n);
                adjacency[*n].push(i);
            }
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
                .is_none_or(|&j| adjacency[i].len() > adjacency[j].len());
            if better {
                surface_at.insert((m.x, m.y), i);
            }
        }
        // About 2 200 surface maps list neighbour ids that exist nowhere in the client; link those by
        // coordinates instead so they still reach a zaap.
        let isolated: Vec<usize> = (0..world.maps.len())
            .filter(|&i| world.maps[i].world_map == SURFACE && adjacency[i].is_empty())
            .collect();
        for i in isolated {
            let m = &world.maps[i];
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if let Some(&n) = surface_at.get(&(m.x + dx, m.y + dy)) {
                    adjacency[i].push(n);
                    adjacency[n].push(i);
                }
            }
        }
        for list in &mut adjacency {
            list.sort_unstable();
            list.dedup();
        }
        let zaaps: Vec<usize> = world
            .zaaps
            .iter()
            .filter_map(|z| index.get(z).copied())
            .collect();
        let nearest_zaap = bfs(&adjacency, &zaaps);
        let sub_area_names = world
            .sub_areas
            .iter()
            .map(|s| (s.id, s.names.clone()))
            .collect();
        Atlas {
            world,
            index,
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

    /// Walking distances from one map to every other, for comparing against a zaap.
    pub fn distances_from(&self, map_id: i64) -> Option<Vec<Option<u32>>> {
        let start = *self.index.get(&map_id)?;
        Some(
            bfs(&self.adjacency, &[start])
                .into_iter()
                .map(|b| b.map(|(_, d)| d))
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
        let i = self.index[&target.id];
        // Interiors are entered through a door: route to the surface map at the same coordinates.
        let i = match target.world_map {
            SURFACE => i,
            _ => self
                .surface_at
                .get(&(target.x, target.y))
                .copied()
                .unwrap_or(i),
        };
        let walk_steps = from.and_then(|d| d[i]);
        let zaap = self.nearest_zaap[i];
        let take_zaap = match (zaap, walk_steps) {
            (Some((_, d)), Some(w)) => d + ZAAP_COST < w,
            (Some(_), None) => true,
            (None, _) => false,
        };
        Plan {
            zaap: zaap.map(|(z, _)| self.stop(&self.world.maps[z], lang)),
            zaap_steps: zaap.map(|(_, d)| d),
            walk_steps,
            take_zaap,
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
    use crate::world::SubArea;

    pub(crate) fn names(fr: &str) -> Names {
        Names {
            fr: fr.into(),
            ..Default::default()
        }
    }

    /// Eight surface maps in a row at y = 0, x = 0..7, each linked to the next; zaap on x = 0.
    pub(crate) fn line_world() -> World {
        let maps = (0..8)
            .map(|x| MapInfo {
                id: 100 + x,
                x: x as i32,
                y: 0,
                world_map: SURFACE,
                sub_area: 1,
                neighbours: if x < 7 { vec![101 + x] } else { vec![] },
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
    fn isolated_map_is_linked_by_coordinates() {
        let mut w = line_world();
        w.maps.push(MapInfo {
            id: 200,
            x: 8,
            y: 0,
            world_map: SURFACE,
            sub_area: 1,
            neighbours: vec![999],
        });
        let atlas = Atlas::new(w);
        assert_eq!(
            atlas
                .plan(atlas.map(200).unwrap(), None, Lang::Fr)
                .zaap_steps,
            Some(8)
        );
    }

    #[test]
    fn interior_is_routed_through_the_surface_map() {
        let mut w = line_world();
        w.maps.push(MapInfo {
            id: 300,
            x: 6,
            y: 0,
            world_map: -1,
            sub_area: 1,
            neighbours: vec![998],
        });
        let atlas = Atlas::new(w);
        assert_eq!(
            atlas
                .plan(atlas.map(300).unwrap(), None, Lang::Fr)
                .zaap_steps,
            Some(6)
        );
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

    #[test]
    fn edges_are_walkable_both_ways() {
        let atlas = Atlas::new(line_world());
        assert_eq!(atlas.distances_from(107).unwrap()[0], Some(7));
    }
}
