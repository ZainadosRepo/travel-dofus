//! The UI settings (profiles, favorites, custom positions, NPC corrections, display choices) kept as one
//! JSON file in the app config folder. The UI owns their shape and normalizes what it reads back.

use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

/// Far above any real settings file: refuses a runaway write before it reaches the disk.
const MAX_BYTES: usize = 1 << 20;

#[derive(Debug, Serialize, PartialEq)]
pub struct Loaded {
    /// `Null` when there is no usable file: the UI starts from its defaults.
    pub settings: Value,
    /// Set when a corrupt file was moved aside; saving over the path is then safe.
    pub warning: Option<String>,
}

/// An `Err` means the file is still there but could not be read: the UI must not save over it.
pub fn load(path: &Path) -> Result<Loaded, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Ok(Loaded {
                settings: Value::Null,
                warning: None,
            })
        }
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    match serde_json::from_str(&text) {
        Ok(settings) => Ok(Loaded {
            settings,
            warning: None,
        }),
        Err(e) => {
            let backup = path.with_extension("json.bad");
            fs::rename(path, &backup)
                .map_err(|r| format!("{}: {e}, mise de côté impossible : {r}", path.display()))?;
            let warning = format!(
                "réglages illisibles ({e}), copie gardée dans {}",
                backup.display()
            );
            Ok(Loaded {
                settings: Value::Null,
                warning: Some(warning),
            })
        }
    }
}

/// Writes next to the target then renames, so a crash mid-write keeps the previous file.
pub fn save(path: &Path, settings: &Value) -> Result<(), String> {
    if !settings.is_object() {
        return Err("les réglages doivent être un objet JSON".into());
    }
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    if text.len() > MAX_BYTES {
        return Err(format!("réglages trop gros ({} octets)", text.len()));
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("travel-dofus-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("settings.json")
    }

    #[test]
    fn missing_file_reads_as_null() {
        let loaded = load(&temp("missing")).unwrap();
        assert_eq!((loaded.settings, loaded.warning), (Value::Null, None));
    }

    #[test]
    fn saves_then_loads_the_same_object() {
        let path = temp("roundtrip");
        let settings = json!({ "antiLag": 3, "profiles": [{ "id": "p1", "favorites": [] }] });
        save(&path, &settings).unwrap();
        assert_eq!(load(&path).unwrap().settings, settings);
    }

    #[test]
    fn moves_a_corrupt_file_aside_and_warns() {
        let path = temp("corrupt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ not json").unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.settings, Value::Null);
        assert!(loaded.warning.is_some());
        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(path.with_extension("json.bad")).unwrap(),
            "{ not json"
        );
    }

    #[test]
    fn refuses_non_objects() {
        assert!(save(&temp("array"), &json!([1, 2])).is_err());
    }
}
