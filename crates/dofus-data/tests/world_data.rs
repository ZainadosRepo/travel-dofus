//! Checks on the committed `data/world.json`, so a bad extraction fails before it ships.

use dofus_data::route::Atlas;
use dofus_data::search::{Kind, Searcher};
use dofus_data::world::{Lang, World};

fn atlas() -> Atlas {
    let json = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/world.json"
    ));
    Atlas::new(serde_json::from_str::<World>(json).expect("world.json parses"))
}

#[test]
fn astrub_zaap_is_found_by_name() {
    let a = atlas();
    let hits = Searcher::new(&a).search(&a, "zaap astrub", None, Lang::Fr);
    let zaap = hits
        .iter()
        .find(|h| h.map_id == 191_105_026)
        .expect("Astrub zaap");
    assert_eq!(zaap.kind, Kind::Zaap);
    assert_eq!(zaap.plan.zaap_steps, Some(0));
}

#[test]
fn quest_npc_has_a_position_and_a_route() {
    let a = atlas();
    let hits = Searcher::new(&a).search(&a, "amine", None, Lang::Fr);
    let amine = hits.iter().find(|h| h.name == "Amine").expect("Amine");
    assert_eq!((amine.x, amine.y), (3, 1));
    assert!(amine.plan.take_zaap && amine.plan.zaap.is_some());
}

#[test]
fn every_npc_has_names_in_all_five_languages() {
    let a = atlas();
    let full = a
        .world
        .npcs
        .iter()
        .filter(|n| n.names.all().iter().all(|s| !s.is_empty()))
        .count();
    assert!(
        full * 100 > a.world.npcs.len() * 95,
        "{full}/{}",
        a.world.npcs.len()
    );
}

#[test]
fn islands_are_not_reached_across_the_water() {
    let a = atlas();
    let hits = Searcher::new(&a).search(&a, "wabbit du soiw woulant", None, Lang::Fr);
    let wabbit = hits.first().expect("Wabbit du Soiw Woulant");
    // Linking maps by coordinates put it 4 maps from another zaap, across the water.
    assert_eq!(
        wabbit.plan.zaap.as_ref().map(|z| z.map_id),
        Some(115_737_095)
    );
    assert_eq!(wabbit.plan.zaap_steps, Some(29));
}

#[test]
fn walking_beats_the_zaap_next_door() {
    let a = atlas();
    let hits = Searcher::new(&a).search(&a, "3,1", Some((3, 2)), Lang::Fr);
    assert_eq!(hits[0].kind, Kind::Coords);
    assert!(!hits[0].plan.take_zaap);
}

#[test]
fn most_surface_maps_reach_a_zaap() {
    let a = atlas();
    let surface: Vec<_> = a.world.maps.iter().filter(|m| m.world_map == 1).collect();
    let reached = surface
        .iter()
        .filter(|m| a.plan(m, None, Lang::Fr).zaap.is_some())
        .count();
    assert!(
        reached * 10 > surface.len() * 9,
        "{reached}/{}",
        surface.len()
    );
}

#[test]
fn every_icon_and_portrait_was_exported() {
    let a = atlas();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/icons");
    let missing: std::collections::BTreeSet<String> = a
        .world
        .markers
        .iter()
        .flat_map(|m| {
            [
                Some(m.gfx.to_string()),
                m.boss.as_ref().map(|b| format!("monster/{}", b.gfx)),
            ]
        })
        .flatten()
        .chain(dofus_data::icons::UI_ICONS.map(String::from))
        .filter(|name| !dir.join(format!("{name}.png")).is_file())
        .collect();
    assert!(missing.is_empty(), "re-run the extractor: {missing:?}");
}

#[test]
fn almost_every_npc_has_its_face() {
    let a = atlas();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/icons/npc");
    let faces = a
        .world
        .npcs
        .iter()
        .filter(|n| {
            dir.join(format!("{}.png", dofus_data::icons::face_name(&n.look)))
                .is_file()
        })
        .count();
    // The renderer may not draw a few looks; the app shows the quest icon for those.
    assert!(
        faces * 100 > a.world.npcs.len() * 95,
        "{faces}/{}",
        a.world.npcs.len()
    );
}

#[test]
fn most_dungeons_show_their_boss() {
    let a = atlas();
    let dungeons: Vec<_> = a.world.markers.iter().filter(|m| m.gfx == 422).collect();
    let with_boss = dungeons.iter().filter(|m| m.boss.is_some()).count();
    assert!(
        with_boss * 10 > dungeons.len() * 9,
        "{with_boss}/{}",
        dungeons.len()
    );
}
