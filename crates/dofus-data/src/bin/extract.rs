//! Extracts `data/world.json`, the map icons in `ui/icons/` and the boss portraits in `ui/icons/monster/`
//! from a Dofus 3 install, then fetches the NPC faces missing from `ui/icons/npc/` from Ankama's renderer.
//! Usage: `cargo run --release -p dofus-data --bin extract -- [<Dofus-dofus3 folder>] [--out <file>] [--icons <folder>]`

use dofus_data::extract::{aa_bundle, content, default_game_dir, extract};
use dofus_data::icons::{self, UI_ICONS};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (mut game, mut out, mut icon_dir) = (
        None,
        PathBuf::from("data/world.json"),
        PathBuf::from("ui/icons"),
    );
    while let Some(a) = args.next() {
        let target = match a.as_str() {
            "--out" => &mut out,
            "--icons" => &mut icon_dir,
            _ => {
                game = Some(PathBuf::from(a));
                continue;
            }
        };
        let Some(path) = args.next() else {
            eprintln!("{a} needs a path");
            return ExitCode::FAILURE;
        };
        *target = PathBuf::from(path);
    }
    let Some(game) = game.or_else(default_game_dir) else {
        eprintln!("pass the Dofus-dofus3 install folder");
        return ExitCode::FAILURE;
    };
    let world = match extract(&game) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("extraction failed for {}: {e}", game.display());
            return ExitCode::FAILURE;
        }
    };
    let linked = world.maps.iter().filter(|m| !m.exits.is_empty()).count();
    let json = serde_json::to_string(&world).expect("world serializes");
    if let Err(e) = std::fs::write(&out, json) {
        eprintln!("cannot write {}: {e}", out.display());
        return ExitCode::FAILURE;
    }
    println!(
        "Dofus {}: {} maps ({linked} linked), {} zaaps, {} NPCs, {} markers -> {}",
        world.game_version,
        world.maps.len(),
        world.zaaps.len(),
        world.npcs.len(),
        world.markers.len(),
        out.display()
    );
    let map_icons: BTreeSet<String> = world
        .markers
        .iter()
        .map(|m| m.gfx.to_string())
        .chain(UI_ICONS.map(String::from))
        .collect();
    let portraits: BTreeSet<String> = world
        .markers
        .iter()
        .filter_map(|m| Some(m.boss.as_ref()?.gfx))
        .map(|gfx| gfx.to_string())
        .collect();
    let monsters = content(&game).join("Picto/Monsters/monster_assets_1x.bundle");
    let jobs = [
        (
            aa_bundle(&game, "uidarkstone_assets_all.bundle"),
            map_icons,
            icon_dir.clone(),
        ),
        (Ok(monsters), portraits, icon_dir.join("monster")),
    ];
    for (bundle, wanted, dir) in jobs {
        let written = match bundle.and_then(|b| icons::export(&b, &wanted, &dir)) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("icon export failed: {e}");
                return ExitCode::FAILURE;
            }
        };
        println!("{} icons -> {}", written.len(), dir.display());
        let missing: Vec<&String> = wanted.difference(&written).collect();
        if !missing.is_empty() {
            // The app falls back to its own picture; the extraction still succeeds.
            eprintln!("not found in the client: {missing:?}");
        }
    }
    let looks: BTreeSet<String> = world
        .npcs
        .iter()
        .filter(|n| !n.look.is_empty())
        .map(|n| n.look.clone())
        .collect();
    match icons::fetch_faces(&looks, &icon_dir.join("npc")) {
        Ok((n, failed)) => {
            println!(
                "{} NPC faces ({n} fetched) -> {}",
                looks.len() - failed.len(),
                icon_dir.join("npc").display()
            );
            if !failed.is_empty() {
                // The app shows the quest icon for these.
                eprintln!("renderer could not draw {} looks: {failed:?}", failed.len());
            }
        }
        Err(e) => {
            // Data and client icons are written: rerun later to finish the faces.
            eprintln!("NPC faces incomplete: {e}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
