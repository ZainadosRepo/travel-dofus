//! Pictures written as PNG for the app: map marker and quest icons (`uidarkstone` UI bundle), boss
//! portraits (`Content/Picto/Monsters`), and NPC faces, which the client draws live and Ankama's
//! public look renderer draws for us.

use crate::unity::{read_bundle, SerializedFile};
use crate::{err, Result};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

const TEXTURE_2D: i32 = 28;
const RGBA32: i64 = 4;
const DXT5: i64 = 12;
const BC7: i64 = 25;

/// Client textures the app shows besides the markers' `gfx`: NPC (quest "!"), position (flag), custom position (pin).
pub const UI_ICONS: [&str; 3] = ["quest", "destination", "flag"];

/// The renderer Ankama's own site draws characters with; `face` frames the head and shoulders.
const RENDERER: &str = "https://static.ankama.com/dofus/renderer/look";
/// Pause between two renderer requests: one extraction per patch, no need to hurry.
const POLITE_DELAY: Duration = Duration::from_millis(150);

/// File name of an NPC face: a hash (FNV-1a) of its look, so a patch that changes a look fetches it again.
pub fn face_name(look: &str) -> String {
    let hash = look.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

#[derive(Debug, PartialEq)]
enum Fetch {
    Done,
    Refused,
    /// The renderer cannot draw this look: it hangs until curl's timeout, or answers an error.
    Unavailable,
    /// No answer at all (offline, DNS): says nothing about the look, so it is retried next run.
    Unreachable,
}

/// Classifies one renderer request from curl's exit code and the HTTP status it printed.
fn classify(curl_exit: Option<i32>, http: &str) -> Fetch {
    const CURL_TIMEOUT: i32 = 28;
    match http {
        "200" => Fetch::Done,
        "403" | "429" => Fetch::Refused,
        _ if http.starts_with('4') || http.starts_with('5') => Fetch::Unavailable,
        _ if curl_exit == Some(CURL_TIMEOUT) => Fetch::Unavailable,
        _ => Fetch::Unreachable,
    }
}

const UNAVAILABLE_HEADER: &str =
    "# NPC faces Ankama's renderer could not draw; the extractor skips them. Delete this file to retry.\n";

/// Face names listed in `path`, one per line (`#` comments); none when the file does not exist.
fn read_names(path: &Path) -> Result<BTreeSet<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(String::from)
            .collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeSet::new()),
        Err(e) => Err(e.into()),
    }
}

/// Writes the names sorted under a header, or removes the file when there are none.
fn write_names(path: &Path, names: &BTreeSet<String>) -> Result<()> {
    if names.is_empty() {
        return match fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        };
    }
    let body: String = names.iter().map(|n| format!("{n}\n")).collect();
    Ok(fs::write(path, format!("{UNAVAILABLE_HEADER}{body}"))?)
}

/// Fetches the face of each look missing from `out` (64×64 PNG), removes faces no look uses any more,
/// and returns how many were fetched plus the looks the renderer could not draw (it times out or
/// errors on some). Those are listed in `unavailable` and skipped by later runs instead of costing a
/// 15 s timeout each time; a changed look has a new name, so it is tried again. Stops at the first
/// refusal (403, 429): what was fetched stays, the next run resumes.
pub fn fetch_faces(
    looks: &BTreeSet<String>,
    out: &Path,
    unavailable: &Path,
) -> Result<(usize, Vec<String>)> {
    fs::create_dir_all(out)?;
    let wanted: BTreeSet<String> = looks
        .iter()
        .map(|l| format!("{}.png", face_name(l)))
        .collect();
    for entry in fs::read_dir(out)?.filter_map(|e| e.ok()) {
        if !wanted.contains(&*entry.file_name().to_string_lossy()) {
            fs::remove_file(entry.path())?;
        }
    }
    // Only names of looks still in use and still without a face stay listed.
    let mut skipped: BTreeSet<String> = read_names(unavailable)?
        .into_iter()
        .filter(|n| wanted.contains(&format!("{n}.png")) && !out.join(format!("{n}.png")).is_file())
        .collect();
    let (mut fetched, mut failed, mut refused) = (0, Vec::new(), None);
    for look in looks {
        let name = face_name(look);
        let path = out.join(format!("{name}.png"));
        if path.is_file() {
            continue;
        }
        if skipped.contains(&name) {
            failed.push(look.clone());
            continue;
        }
        let hex: String = look.bytes().map(|b| format!("{b:02x}")).collect();
        let url = format!("{RENDERER}/{hex}/face/1/64_64-0.png");
        let status = Command::new("curl")
            .args(["-s", "-m", "15", "-o"])
            .arg(&path)
            .args(["-w", "%{http_code}", &url])
            .output()
            .map_err(|e| crate::Error(format!("curl: {e}")))?;
        let code = String::from_utf8_lossy(&status.stdout).trim().to_owned();
        let result = classify(status.status.code(), &code);
        if result == Fetch::Done {
            fetched += 1;
        } else {
            // Keep no error page under a face's name (curl may not have created the file at all).
            let _ = fs::remove_file(&path);
            if result == Fetch::Refused {
                refused = Some(code);
                break;
            }
            if result == Fetch::Unavailable {
                skipped.insert(name);
            }
            failed.push(look.clone());
        }
        std::thread::sleep(POLITE_DELAY);
    }
    // Written before a refusal is reported, so the next run does not wait on these looks again.
    write_names(unavailable, &skipped)?;
    match refused {
        Some(code) => err(format!("renderer refused ({code}) after {fetched} faces")),
        None => Ok((fetched, failed)),
    }
}

/// Writes `<out>/<name>.png` for each wanted texture name of a bundle and returns the names written.
pub fn export(path: &Path, wanted: &BTreeSet<String>, out: &Path) -> Result<BTreeSet<String>> {
    let context = |e: crate::Error| crate::Error(format!("{}: {e}", path.display()));
    let bytes = fs::read(path)?;
    let files = read_bundle(&bytes).map_err(context)?;
    let streams: HashMap<&str, &[u8]> = files
        .iter()
        .map(|f| (f.path.as_str(), f.data.as_slice()))
        .collect();
    fs::create_dir_all(out)?;
    let mut written = BTreeSet::new();
    for file in &files {
        let Ok(sf) = SerializedFile::parse(&file.data) else {
            continue;
        };
        for o in &sf.objects {
            if sf.class_id(o) != TEXTURE_2D {
                continue;
            }
            let tex = sf.read(o).map_err(context)?;
            let name = tex["m_Name"].as_str().unwrap_or_default();
            if !wanted.contains(name) {
                continue;
            }
            let rgba = pixels(&tex, &streams).map_err(|e| crate::Error(format!("{name}: {e}")))?;
            let (w, h) = (int(&tex, "m_Width"), int(&tex, "m_Height"));
            write_png(&out.join(format!("{name}.png")), w, h, &rgba)?;
            written.insert(name.to_owned());
        }
    }
    Ok(written)
}

fn int(v: &Value, key: &str) -> u32 {
    v.get(key).and_then(Value::as_u64).unwrap_or_default() as u32
}

/// Top-down RGBA pixels of a texture, from its object or its `.resS` stream.
fn pixels(tex: &Value, streams: &HashMap<&str, &[u8]>) -> Result<Vec<u8>> {
    let (w, h) = (int(tex, "m_Width") as usize, int(tex, "m_Height") as usize);
    let stream = &tex["m_StreamData"];
    let source = stream["path"].as_str().unwrap_or_default();
    let data: Vec<u8> = if source.is_empty() {
        tex["image data"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_u64)
                    .map(|b| b as u8)
                    .collect()
            })
            .unwrap_or_default()
    } else {
        // `archive:/CAB-…/CAB-….resS` names a file of the same bundle.
        let key = source.rsplit('/').next().unwrap_or(source);
        let (offset, size) = (
            stream["offset"].as_u64().unwrap_or(0) as usize,
            int(stream, "size") as usize,
        );
        match streams.get(key).and_then(|s| s.get(offset..offset + size)) {
            Some(bytes) => bytes.to_vec(),
            None => return err(format!("stream {key} out of range")),
        }
    };
    let block = |decode: fn(&[u8], &mut [u8], usize)| -> Result<Vec<u8>> {
        if w % 4 != 0 || h % 4 != 0 || data.len() < w * h {
            return err(format!("{w}x{h} texture with {} bytes", data.len()));
        }
        Ok(blocks(&data, w, h, decode))
    };
    let rgba = match tex["m_TextureFormat"].as_i64() {
        Some(RGBA32) if data.len() >= w * h * 4 => data[..w * h * 4].to_vec(),
        Some(DXT5) => block(bcdec_rs::bc3)?,
        Some(BC7) => block(bcdec_rs::bc7)?,
        Some(f) => return err(format!("texture format {f} not supported")),
        None => return err("no texture format"),
    };
    // Unity stores rows bottom-up.
    Ok(rgba.chunks(w * 4).rev().flatten().copied().collect())
}

/// Decodes 16-byte 4×4 blocks (BC3, BC7) into an RGBA image `w` pixels wide.
fn blocks(src: &[u8], w: usize, h: usize, decode: fn(&[u8], &mut [u8], usize)) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    for (i, b) in src.as_chunks::<16>().0.iter().take(w * h / 16).enumerate() {
        let (x, y) = (i % (w / 4) * 4, i / (w / 4) * 4);
        decode(b, &mut out[(y * w + x) * 4..], w * 4);
    }
    out
}

fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<()> {
    let file = fs::File::create(path)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(rgba))
        .map_err(|e| crate::Error(format!("{}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_names_are_stable_fnv1a_hashes() {
        assert_eq!(face_name(""), "cbf29ce484222325");
        assert_eq!(face_name("a"), "af63dc4c8601ec8c");
    }

    #[test]
    fn only_timeouts_and_error_answers_mark_a_look_unavailable() {
        assert_eq!(classify(Some(0), "200"), Fetch::Done);
        assert_eq!(classify(Some(0), "429"), Fetch::Refused);
        assert_eq!(classify(Some(0), "403"), Fetch::Refused);
        assert_eq!(classify(Some(28), "000"), Fetch::Unavailable);
        assert_eq!(classify(Some(0), "500"), Fetch::Unavailable);
        assert_eq!(classify(Some(0), "404"), Fetch::Unavailable);
        // Offline or DNS failure: not the look's fault.
        assert_eq!(classify(Some(6), "000"), Fetch::Unreachable);
        assert_eq!(classify(Some(7), "000"), Fetch::Unreachable);
        assert_eq!(classify(None, ""), Fetch::Unreachable);
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("travel-dofus-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("npc")).unwrap();
        dir
    }

    #[test]
    fn listed_looks_are_skipped_and_the_list_keeps_only_looks_in_use() {
        let dir = scratch("unavailable");
        let (npc, list) = (dir.join("npc"), dir.join("npc-faces-unavailable.txt"));
        fs::write(npc.join(format!("{}.png", face_name("drawn"))), b"png").unwrap();
        fs::write(npc.join("gone.png"), b"png").unwrap();
        let text = format!("# old\n{}\nnot-a-look-any-more\n", face_name("hangs"));
        fs::write(&list, text).unwrap();
        let looks: BTreeSet<String> = ["drawn", "hangs"].map(String::from).into();

        // Every look has a face or is listed, so no request is made.
        let (fetched, failed) = fetch_faces(&looks, &npc, &list).unwrap();

        assert_eq!((fetched, failed), (0, vec!["hangs".to_owned()]));
        assert!(!npc.join("gone.png").exists());
        let kept = read_names(&list).unwrap();
        assert_eq!(kept, [face_name("hangs")].into());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_list_goes_away_once_every_listed_look_has_a_face() {
        let dir = scratch("drawn-later");
        let (npc, list) = (dir.join("npc"), dir.join("npc-faces-unavailable.txt"));
        fs::write(npc.join(format!("{}.png", face_name("fixed"))), b"png").unwrap();
        fs::write(&list, format!("{}\n", face_name("fixed"))).unwrap();
        let looks: BTreeSet<String> = ["fixed"].map(String::from).into();

        fetch_faces(&looks, &npc, &list).unwrap();

        assert!(!list.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn blocks_land_left_to_right_then_down() {
        // BC3 blocks of one colour each: alpha 255, colour endpoint 0 used by every pixel.
        let solid = |rgb565: u16| {
            let mut b = [0u8; 16];
            b[0] = 255;
            b[8..10].copy_from_slice(&rgb565.to_le_bytes());
            b
        };
        let src = [solid(0xF800), solid(0x001F), solid(0x07E0), solid(0xFFFF)].concat();
        let px = blocks(&src, 8, 8, bcdec_rs::bc3);
        let at = |x: usize, y: usize| &px[(y * 8 + x) * 4..(y * 8 + x) * 4 + 4];
        assert_eq!(at(3, 3), [255, 0, 0, 255]);
        assert_eq!(at(4, 0), [0, 0, 255, 255]);
        assert_eq!(at(0, 4), [0, 255, 0, 255]);
        assert_eq!(at(7, 7), [255, 255, 255, 255]);
    }
}
