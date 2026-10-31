//! The world model the extractor writes to `data/world.json` and the app embeds.

use serde::{Deserialize, Serialize};

/// The five languages the client ships texts for (`Content/I18n/<lang>.bin`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    Fr,
    En,
    De,
    Es,
    Pt,
}

impl Lang {
    pub const ALL: [Lang; 5] = [Lang::Fr, Lang::En, Lang::De, Lang::Es, Lang::Pt];

    pub fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
            Lang::De => "de",
            Lang::Es => "es",
            Lang::Pt => "pt",
        }
    }
}

/// A game text in every client language.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Names {
    pub fr: String,
    pub en: String,
    pub de: String,
    pub es: String,
    pub pt: String,
}

impl Names {
    /// The text in `lang`, falling back to French when that translation is missing.
    pub fn get(&self, lang: Lang) -> &str {
        let text = match lang {
            Lang::Fr => &self.fr,
            Lang::En => &self.en,
            Lang::De => &self.de,
            Lang::Es => &self.es,
            Lang::Pt => &self.pt,
        };
        if text.is_empty() {
            &self.fr
        } else {
            text
        }
    }

    pub fn all(&self) -> [&str; 5] {
        [&self.fr, &self.en, &self.de, &self.es, &self.pt]
    }

    pub fn is_empty(&self) -> bool {
        self.all().iter().all(|s| s.is_empty())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct World {
    /// Game version the data was extracted from, e.g. `3.7.11.11`.
    pub game_version: String,
    pub maps: Vec<MapInfo>,
    pub sub_areas: Vec<SubArea>,
    /// Map ids holding an active zaap.
    pub zaaps: Vec<i64>,
    pub npcs: Vec<Npc>,
    pub markers: Vec<Marker>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapInfo {
    pub id: i64,
    pub x: i32,
    pub y: i32,
    pub world_map: i32,
    pub sub_area: i64,
    /// Moves out of this map, from the world graph the client routes `/travel` with.
    pub exits: Vec<Exit>,
}

/// One move from a walkable zone of a map to a zone of another: `[from zone, to map, to zone,
/// gated]`. Walls, cliffs or water split some maps into zones that do not reach each other;
/// `gated` moves need a quest, an item or another condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exit(pub u8, pub i64, pub u8, pub bool);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubArea {
    pub id: i64,
    pub names: Names,
    pub area: Names,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Npc {
    pub id: i64,
    pub names: Names,
    pub maps: Vec<i64>,
}

/// A point the game shows on its world map (bank, workshop, dungeon, transport…).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marker {
    pub names: Names,
    pub category: Names,
    pub map_id: i64,
    pub x: i32,
    pub y: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_translation_falls_back_to_french() {
        let n = Names {
            fr: "Banque".into(),
            en: "Bank".into(),
            ..Default::default()
        };
        assert_eq!(n.get(Lang::En), "Bank");
        assert_eq!(n.get(Lang::De), "Banque");
    }
}
