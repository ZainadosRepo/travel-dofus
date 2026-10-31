//! Extracts `data/world.json` from a Dofus 3 install.
//! Usage: `cargo run --release -p dofus-data --bin extract -- [<Dofus-dofus3 folder>] [--out <file>]`

use dofus_data::extract::{default_game_dir, extract};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (mut game, mut out) = (None, PathBuf::from("data/world.json"));
    while let Some(a) = args.next() {
        if a != "--out" {
            game = Some(PathBuf::from(a));
            continue;
        }
        let Some(path) = args.next() else {
            eprintln!("--out needs a file path");
            return ExitCode::FAILURE;
        };
        out = PathBuf::from(path);
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
    ExitCode::SUCCESS
}
