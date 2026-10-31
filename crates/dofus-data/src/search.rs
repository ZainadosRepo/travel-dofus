//! Free-text search over NPCs, zaaps and map markers in any client language, or a `x,y` position.

use crate::route::{Atlas, Plan};
use crate::world::{Lang, Names};
use serde::Serialize;
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

const LIMIT: usize = 60;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Coords,
    Zaap,
    Npc,
    Marker,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub kind: Kind,
    /// Stable per kind: NPC id, zaap map id, marker index, 0 for a position.
    pub id: i64,
    pub name: String,
    /// Sub-area for NPCs and positions, area for zaaps, category for markers.
    pub detail: String,
    pub map_id: i64,
    pub x: i32,
    pub y: i32,
    pub plan: Plan,
}

/// Lowercase without accents, so "ecaflip" finds "Écaflip".
pub fn normalize(s: &str) -> String {
    s.nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .map(|c| if c == '’' { '\'' } else { c })
        .collect()
}

/// `4,-18`, `4 -18` or `[4, -18]`.
pub fn parse_coords(q: &str) -> Option<(i32, i32)> {
    let q = q.trim().trim_start_matches('[').trim_end_matches(']');
    let mut parts = q
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|p| !p.is_empty());
    let (x, y) = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    parts.next().is_none().then_some((x, y))
}

struct Candidate {
    kind: Kind,
    id: i64,
    names: Names,
    detail: Names,
    /// Normalized names and details in every language, for matching.
    keys: Vec<String>,
    details: Vec<String>,
    map_id: i64,
    x: i32,
    y: i32,
}

fn normalized(names: &Names) -> Vec<String> {
    let mut out: Vec<String> = names
        .all()
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| normalize(s))
        .collect();
    out.sort();
    out.dedup();
    out
}

pub struct Searcher {
    candidates: Vec<Candidate>,
}

impl Searcher {
    pub fn new(atlas: &Atlas) -> Self {
        let mut candidates = Vec::new();
        let mut push = |kind, id, names: Names, detail: Names, map_id, x, y| {
            let (keys, details) = (normalized(&names), normalized(&detail));
            candidates.push(Candidate {
                kind,
                id,
                names,
                detail,
                keys,
                details,
                map_id,
                x,
                y,
            });
        };
        for &z in &atlas.world.zaaps {
            let Some(m) = atlas.map(z) else { continue };
            let Some(sub_area) = atlas.world.sub_areas.iter().find(|s| s.id == m.sub_area) else {
                continue;
            };
            push(
                Kind::Zaap,
                m.id,
                sub_area.names.clone(),
                sub_area.area.clone(),
                m.id,
                m.x,
                m.y,
            );
        }
        for npc in &atlas.world.npcs {
            for m in npc.maps.iter().filter_map(|&id| atlas.map(id)) {
                let detail = atlas.sub_area_names(m).cloned().unwrap_or_default();
                push(Kind::Npc, npc.id, npc.names.clone(), detail, m.id, m.x, m.y);
            }
        }
        for (i, mk) in atlas.world.markers.iter().enumerate() {
            push(
                Kind::Marker,
                i as i64,
                mk.names.clone(),
                mk.category.clone(),
                mk.map_id,
                mk.x,
                mk.y,
            );
        }
        Searcher { candidates }
    }

    pub fn search(
        &self,
        atlas: &Atlas,
        query: &str,
        from: Option<(i32, i32)>,
        lang: Lang,
    ) -> Vec<Place> {
        let distances = from
            .and_then(|(x, y)| atlas.surface_map_at(x, y))
            .and_then(|m| atlas.distances_from(m.id));
        let plan_for = |map_id: i64, x: i32, y: i32| {
            let target = atlas.map(map_id).or_else(|| atlas.surface_map_at(x, y));
            target.map_or(
                Plan {
                    zaap: None,
                    zaap_steps: None,
                    walk_steps: None,
                    take_zaap: false,
                },
                |m| atlas.plan(m, distances.as_deref(), lang),
            )
        };

        let mut hits: Vec<(u8, &Candidate)> = Vec::new();
        let mut places = Vec::new();
        if let Some((x, y)) = parse_coords(query) {
            if let Some(m) = atlas.surface_map_at(x, y) {
                places.push(Place {
                    kind: Kind::Coords,
                    id: 0,
                    name: format!("[{x},{y}]"),
                    detail: atlas.sub_area_name(m, lang).to_owned(),
                    map_id: m.id,
                    x,
                    y,
                    plan: plan_for(m.id, x, y),
                });
            }
            hits.extend(
                self.candidates
                    .iter()
                    .filter(|c| c.x == x && c.y == y)
                    .map(|c| (0, c)),
            );
        } else {
            let q = normalize(query);
            let mut words: Vec<&str> = q.split_whitespace().collect();
            let zaap_only = words.first() == Some(&"zaap");
            if zaap_only {
                words.remove(0);
            }
            if words.is_empty() && !zaap_only {
                return places;
            }
            let first = words.first().copied().unwrap_or_default();
            for c in &self.candidates {
                if zaap_only && c.kind != Kind::Zaap {
                    continue;
                }
                let found = |w: &&str| c.keys.iter().chain(&c.details).any(|k| k.contains(*w));
                if !words.iter().all(found) {
                    continue;
                }
                let rank = if c.keys.iter().any(|k| k.starts_with(first)) {
                    0
                } else if c.keys.iter().any(|k| k.contains(first)) {
                    1
                } else {
                    2
                };
                hits.push((rank, c));
            }
        }
        hits.sort_by(|(ra, a), (rb, b)| {
            ra.cmp(rb)
                .then(a.kind.cmp(&b.kind))
                .then_with(|| a.names.get(lang).cmp(b.names.get(lang)))
        });
        places.extend(hits.into_iter().take(LIMIT).map(|(_, c)| Place {
            kind: c.kind,
            id: c.id,
            name: c.names.get(lang).to_owned(),
            detail: c.detail.get(lang).to_owned(),
            map_id: c.map_id,
            x: c.x,
            y: c.y,
            plan: plan_for(c.map_id, c.x, c.y),
        }));
        places
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::tests::{line_world, names};
    use crate::world::{Marker, Npc};

    fn atlas() -> Atlas {
        let mut w = line_world();
        w.npcs.push(Npc {
            id: 1,
            names: Names {
                fr: "Écaflip l'Ancien".into(),
                en: "Ecaflip the Elder".into(),
                ..Default::default()
            },
            maps: vec![106],
        });
        w.markers.push(Marker {
            names: names("Banque"),
            category: names("Divers"),
            map_id: 103,
            x: 3,
            y: 0,
        });
        Atlas::new(w)
    }

    #[test]
    fn normalizes_accents_and_case() {
        assert_eq!(normalize("Écaflip L’Ancien"), "ecaflip l'ancien");
    }

    #[test]
    fn parses_positions() {
        assert_eq!(parse_coords("4,-18"), Some((4, -18)));
        assert_eq!(parse_coords(" [3, -17] "), Some((3, -17)));
        assert_eq!(parse_coords("-55 15"), Some((-55, 15)));
        assert_eq!(parse_coords("kerub"), None);
        assert_eq!(parse_coords("1,2,3"), None);
    }

    #[test]
    fn finds_an_npc_without_accents_and_plans_the_route() {
        let a = atlas();
        let hits = Searcher::new(&a).search(&a, "ecaflip", None, Lang::Fr);
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].kind, hits[0].id), (Kind::Npc, 1));
        assert_eq!(hits[0].detail, "Astrub");
        assert_eq!(hits[0].plan.zaap_steps, Some(6));
    }

    #[test]
    fn searches_every_language_and_shows_the_chosen_one() {
        let a = atlas();
        let hits = Searcher::new(&a).search(&a, "elder", None, Lang::Fr);
        assert_eq!(hits[0].name, "Écaflip l'Ancien");
        let hits = Searcher::new(&a).search(&a, "ancien", None, Lang::En);
        assert_eq!(hits[0].name, "Ecaflip the Elder");
    }

    #[test]
    fn zaap_keyword_lists_only_zaaps() {
        let a = atlas();
        let hits = Searcher::new(&a).search(&a, "zaap", None, Lang::Fr);
        assert!(!hits.is_empty() && hits.iter().all(|h| h.kind == Kind::Zaap));
    }

    #[test]
    fn a_position_returns_the_map_and_what_stands_on_it() {
        let a = atlas();
        let hits = Searcher::new(&a).search(&a, "3,0", Some((5, 0)), Lang::Fr);
        assert_eq!(hits[0].kind, Kind::Coords);
        assert_eq!(hits[1].name, "Banque");
        assert!(!hits[0].plan.take_zaap);
        assert_eq!(hits[0].plan.walk_steps, Some(2));
    }

    #[test]
    fn empty_query_returns_nothing() {
        let a = atlas();
        assert!(Searcher::new(&a)
            .search(&a, "  ", None, Lang::Fr)
            .is_empty());
    }
}
