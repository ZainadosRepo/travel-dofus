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

/// Fetches the face of each look missing from `out` (64×64 PNG), removes faces no look uses any more,
/// and returns how many were fetched plus the looks the renderer could not draw (it times out or
/// errors on some). Stops at the first refusal (403, 429): what was fetched stays, the next run resumes.
pub fn fetch_faces(looks: &BTreeSet<String>, out: &Path) -> Result<(usize, Vec<String>)> {
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
    let (mut fetched, mut failed) = (0, Vec::new());
    for look in looks {
        let path = out.join(format!("{}.png", face_name(look)));
        if path.is_file() {
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
        if code == "200" {
            fetched += 1;
        } else {
            // Keep no error page under a face's name (curl may not have created the file at all).
            let _ = fs::remove_file(&path);
            if code == "403" || code == "429" {
                return err(format!("renderer refused ({code}) after {fetched} faces"));
            }
            failed.push(look.clone());
        }
        std::thread::sleep(POLITE_DELAY);
    }
    Ok((fetched, failed))
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
