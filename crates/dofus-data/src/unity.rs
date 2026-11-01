//! Minimal reader for Unity asset bundles (UnityFS v6-8, LZMA/LZ4) and the SerializedFile (v22+)
//! inside them. Objects are decoded through the type trees the files embed, including the
//! `[SerializeReference]` registry Dofus uses for its data tables.

use crate::{err, Result};
use serde_json::{Map, Number, Value};
use std::collections::HashMap;
use std::io::Cursor;

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
}

macro_rules! num {
    ($name:ident, $t:ty, $from:ident) => {
        fn $name(&mut self) -> Result<$t> {
            let b = self.take(std::mem::size_of::<$t>())?;
            Ok(<$t>::$from(
                b.try_into().expect("take returned the requested length"),
            ))
        }
    };
}

impl<'a> Reader<'a> {
    fn new(d: &'a [u8], p: usize) -> Self {
        Reader { d, p }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.p.checked_add(n).filter(|&e| e <= self.d.len());
        match end {
            Some(e) => {
                let s = &self.d[self.p..e];
                self.p = e;
                Ok(s)
            }
            None => err(format!(
                "read past end: {} + {} > {}",
                self.p,
                n,
                self.d.len()
            )),
        }
    }

    num!(u8, u8, from_le_bytes);
    num!(i16, i16, from_le_bytes);
    num!(u16, u16, from_le_bytes);
    num!(i32, i32, from_le_bytes);
    num!(u32, u32, from_le_bytes);
    num!(i64, i64, from_le_bytes);
    num!(u64, u64, from_le_bytes);
    num!(f32, f32, from_le_bytes);
    num!(f64, f64, from_le_bytes);
    num!(u32_be, u32, from_be_bytes);
    num!(i32_be, i32, from_be_bytes);
    num!(i64_be, i64, from_be_bytes);
    num!(u16_be, u16, from_be_bytes);

    fn cstr(&mut self) -> Result<String> {
        let rest = &self.d[self.p..];
        let Some(len) = rest.iter().position(|&b| b == 0) else {
            return err("unterminated string");
        };
        let s = String::from_utf8_lossy(&rest[..len]).into_owned();
        self.p += len + 1;
        Ok(s)
    }

    fn align4(&mut self) {
        self.p = (self.p + 3) & !3;
    }

    /// A length read from the file, refused when it cannot fit in what is left.
    fn len(&mut self, item_size: usize) -> Result<usize> {
        let n = self.i32()?;
        let left = self.d.len().saturating_sub(self.p);
        match usize::try_from(n) {
            Ok(n) if n.saturating_mul(item_size.max(1)) <= left => Ok(n),
            _ => err(format!("bad length {n} at {}", self.p)),
        }
    }
}

/// One file stored in a bundle (usually a `CAB-…` SerializedFile, sometimes a `.resS` blob).
pub struct BundleFile {
    pub path: String,
    pub data: Vec<u8>,
}

fn decompress(kind: u32, src: &[u8], size: usize) -> Result<Vec<u8>> {
    match kind {
        0 => Ok(src.to_vec()),
        1 => {
            let mut out = Vec::with_capacity(size);
            let options = lzma_rs::decompress::Options {
                // Unity writes the 5-byte LZMA header without the 8-byte size field.
                unpacked_size: lzma_rs::decompress::UnpackedSize::UseProvided(Some(size as u64)),
                ..Default::default()
            };
            lzma_rs::lzma_decompress_with_options(&mut Cursor::new(src), &mut out, &options)
                .map_err(|e| crate::Error(format!("lzma: {e}")))?;
            Ok(out)
        }
        2 | 3 => {
            lz4_flex::block::decompress(src, size).map_err(|e| crate::Error(format!("lz4: {e}")))
        }
        k => err(format!("unsupported compression {k}")),
    }
}

pub fn read_bundle(bytes: &[u8]) -> Result<Vec<BundleFile>> {
    let mut r = Reader::new(bytes, 0);
    if r.cstr()? != "UnityFS" {
        return err("not a UnityFS bundle");
    }
    let version = r.u32_be()?;
    r.cstr()?;
    r.cstr()?;
    r.i64_be()?;
    let (c_info, u_info, flags) = (r.u32_be()? as usize, r.u32_be()? as usize, r.u32_be()?);
    if version >= 7 {
        r.p = (r.p + 15) & !15;
    }
    let info_raw = if flags & 0x80 != 0 {
        &bytes[bytes.len().saturating_sub(c_info)..]
    } else {
        r.take(c_info)?
    };
    let info = decompress(flags & 0x3f, info_raw, u_info)?;
    let mut i = Reader::new(&info, 16);
    let block_count = i.i32_be()?;
    let mut blocks = Vec::new();
    for _ in 0..block_count {
        blocks.push((i.u32_be()? as usize, i.u32_be()? as usize, i.u16_be()?));
    }
    let node_count = i.i32_be()?;
    let mut nodes = Vec::new();
    for _ in 0..node_count {
        let (offset, size) = (i.i64_be()? as usize, i.i64_be()? as usize);
        i.u32_be()?;
        nodes.push((offset, size, i.cstr()?));
    }
    if flags & 0x200 != 0 {
        r.p = (r.p + 15) & !15;
    }
    let mut data = Vec::new();
    for (u, c, f) in blocks {
        data.extend(decompress(u32::from(f & 0x3f), r.take(c)?, u)?);
    }
    nodes
        .into_iter()
        .map(
            |(offset, size, path)| match data.get(offset..offset + size) {
                Some(d) => Ok(BundleFile {
                    path,
                    data: d.to_vec(),
                }),
                None => err(format!("node {path} out of range")),
            },
        )
        .collect()
}

const COMMON: &[&str] = &[
    "AABB",
    "AnimationClip",
    "AnimationCurve",
    "AnimationState",
    "Array",
    "Base",
    "BitField",
    "bitset",
    "bool",
    "char",
    "ColorRGBA",
    "Component",
    "data",
    "deque",
    "double",
    "dynamic_array",
    "FastPropertyName",
    "first",
    "float",
    "Font",
    "GameObject",
    "Generic Mono",
    "GradientNEW",
    "GUID",
    "GUIStyle",
    "int",
    "list",
    "long long",
    "map",
    "Matrix4x4f",
    "MdFour",
    "MonoBehaviour",
    "MonoScript",
    "m_ByteSize",
    "m_Curve",
    "m_EditorClassIdentifier",
    "m_EditorHideFlags",
    "m_Enabled",
    "m_ExtensionPtr",
    "m_GameObject",
    "m_Index",
    "m_IsArray",
    "m_IsStatic",
    "m_MetaFlag",
    "m_Name",
    "m_ObjectHideFlags",
    "m_PrefabInternal",
    "m_PrefabParentObject",
    "m_Script",
    "m_StaticEditorFlags",
    "m_Type",
    "m_Version",
    "Object",
    "pair",
    "PPtr<Component>",
    "PPtr<GameObject>",
    "PPtr<Material>",
    "PPtr<MonoBehaviour>",
    "PPtr<MonoScript>",
    "PPtr<Object>",
    "PPtr<Prefab>",
    "PPtr<Sprite>",
    "PPtr<TextAsset>",
    "PPtr<Texture>",
    "PPtr<Texture2D>",
    "PPtr<Transform>",
    "Prefab",
    "Quaternionf",
    "Rectf",
    "RectInt",
    "RectOffset",
    "second",
    "set",
    "short",
    "size",
    "SInt16",
    "SInt32",
    "SInt64",
    "SInt8",
    "staticvector",
    "string",
    "TextAsset",
    "TextMesh",
    "Texture",
    "Texture2D",
    "Transform",
    "TypelessData",
    "UInt16",
    "UInt32",
    "UInt64",
    "UInt8",
    "unsigned int",
    "unsigned long long",
    "unsigned short",
    "vector",
    "Vector2f",
    "Vector3f",
    "Vector4f",
    "m_ScriptingClassIdentifier",
    "Gradient",
    "Type*",
    "int2_storage",
    "int3_storage",
    "BoundsInt",
    "m_CorrespondingSourceObject",
    "m_PrefabInstance",
    "m_PrefabAsset",
    "FileSize",
    "Hash128",
    "RenderingLayerMask",
];

/// Unity's built-in type-tree strings, addressed by their offset in the NUL-separated buffer.
fn common_string(offset: u32) -> Option<&'static str> {
    let mut at = 0;
    for s in COMMON {
        if at == offset {
            return Some(s);
        }
        at += s.len() as u32 + 1;
    }
    None
}

#[derive(Debug, Clone)]
pub struct TypeNode {
    pub ty: String,
    pub name: String,
    pub align: bool,
    pub children: Vec<TypeNode>,
}

fn read_tree(r: &mut Reader) -> Result<TypeNode> {
    let count = r.len(32)?;
    let buf_size = r.len(1)?;
    let mut flat = Vec::with_capacity(count);
    for _ in 0..count {
        r.u16()?;
        let level = r.u8()?;
        r.u8()?;
        let (ty, name) = (r.u32()?, r.u32()?);
        r.i32()?;
        r.i32()?;
        let meta = r.u32()?;
        r.u64()?;
        flat.push((level, ty, name, meta & 0x4000 != 0));
    }
    let strings = r.take(buf_size)?;
    let s = |off: u32| -> String {
        if off & 0x8000_0000 != 0 {
            let off = off & 0x7fff_ffff;
            return common_string(off).map_or_else(|| format!("?common{off}"), str::to_owned);
        }
        let rest = strings.get(off as usize..).unwrap_or_default();
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        String::from_utf8_lossy(&rest[..end]).into_owned()
    };
    let nodes: Vec<(u8, TypeNode)> = flat
        .into_iter()
        .map(|(level, ty, name, align)| {
            (
                level,
                TypeNode {
                    ty: s(ty),
                    name: s(name),
                    align,
                    children: vec![],
                },
            )
        })
        .collect();
    if nodes.is_empty() {
        return err("empty type tree");
    }
    fn build(nodes: &[(u8, TypeNode)], i: usize) -> (TypeNode, usize) {
        let (level, node) = &nodes[i];
        let mut node = node.clone();
        let mut j = i + 1;
        while j < nodes.len() && nodes[j].0 > *level {
            let (child, next) = build(nodes, j);
            node.children.push(child);
            j = next;
        }
        (node, j)
    }
    Ok(build(&nodes, 0).0)
}

struct SerializedType {
    class_id: i32,
    tree: TypeNode,
}

/// A `[SerializeReference]` class: (class, namespace, assembly).
type RefKey = (String, String, String);

fn read_types(r: &mut Reader, is_ref: bool) -> Result<Vec<(SerializedType, Option<RefKey>)>> {
    let count = r.len(1)?;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let class_id = r.i32()?;
        r.u8()?;
        let script_index = r.i16()?;
        if (is_ref && script_index >= 0) || class_id < 0 || class_id == 114 {
            r.take(16)?;
        }
        r.take(16)?;
        let tree = read_tree(r)?;
        let key = if is_ref {
            Some((r.cstr()?, r.cstr()?, r.cstr()?))
        } else {
            let deps = r.len(4)?;
            r.take(deps * 4)?;
            None
        };
        out.push((SerializedType { class_id, tree }, key));
    }
    Ok(out)
}

pub struct ObjectInfo {
    pub path_id: i64,
    start: usize,
    size: usize,
    type_index: usize,
}

pub struct SerializedFile<'a> {
    data: &'a [u8],
    types: Vec<SerializedType>,
    pub objects: Vec<ObjectInfo>,
    ref_types: HashMap<RefKey, TypeNode>,
}

pub const MONO_BEHAVIOUR: i32 = 114;

impl<'a> SerializedFile<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        let mut r = Reader::new(data, 8);
        let version = r.u32_be()?;
        if version < 22 {
            return err(format!(
                "SerializedFile v{version} not supported (need 22+)"
            ));
        }
        r.p = 32;
        let data_offset =
            usize::try_from(r.i64_be()?).map_err(|_| crate::Error("bad data offset".into()))?;
        r.p = 48;
        r.cstr()?;
        r.i32()?;
        if r.u8()? != 1 {
            return err("file has no type tree");
        }
        let types: Vec<SerializedType> = read_types(&mut r, false)?
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let count = r.len(1)?;
        let mut objects = Vec::with_capacity(count);
        for _ in 0..count {
            r.align4();
            let path_id = r.i64()?;
            let start = data_offset
                + usize::try_from(r.i64()?)
                    .map_err(|_| crate::Error("bad object offset".into()))?;
            let size = r.u32()? as usize;
            let type_index =
                usize::try_from(r.i32()?).map_err(|_| crate::Error("bad type index".into()))?;
            if type_index >= types.len() || start + size > data.len() {
                return err("object entry out of range");
            }
            objects.push(ObjectInfo {
                path_id,
                start,
                size,
                type_index,
            });
        }
        for _ in 0..r.len(1)? {
            r.i32()?;
            r.align4();
            r.i64()?;
        }
        for _ in 0..r.len(1)? {
            r.cstr()?;
            r.take(16)?;
            r.i32()?;
            r.cstr()?;
        }
        let ref_types = read_types(&mut r, true)?
            .into_iter()
            .filter_map(|(t, key)| key.map(|k| (k, t.tree)))
            .collect();
        Ok(SerializedFile {
            data,
            types,
            objects,
            ref_types,
        })
    }

    pub fn class_id(&self, o: &ObjectInfo) -> i32 {
        self.types[o.type_index].class_id
    }

    pub fn tree(&self, o: &ObjectInfo) -> &TypeNode {
        &self.types[o.type_index].tree
    }

    pub fn read(&self, o: &ObjectInfo) -> Result<Value> {
        let mut v = ValueReader {
            r: Reader::new(&self.data[o.start..o.start + o.size], 0),
            refs: &self.ref_types,
        };
        v.read(self.tree(o))
    }
}

struct ValueReader<'a, 'r> {
    r: Reader<'a>,
    refs: &'r HashMap<RefKey, TypeNode>,
}

impl ValueReader<'_, '_> {
    fn read(&mut self, node: &TypeNode) -> Result<Value> {
        let r = &mut self.r;
        let v = match node.ty.as_str() {
            "SInt8" => Value::from(r.u8()? as i8),
            "UInt8" | "char" => Value::from(r.u8()?),
            "bool" => Value::from(r.u8()? != 0),
            "SInt16" | "short" => Value::from(r.i16()?),
            "UInt16" | "unsigned short" => Value::from(r.u16()?),
            "SInt32" | "int" => Value::from(r.i32()?),
            "UInt32" | "unsigned int" | "Type*" => Value::from(r.u32()?),
            "SInt64" | "long long" => Value::from(r.i64()?),
            "UInt64" | "unsigned long long" | "FileSize" => Value::from(r.u64()?),
            "float" => Number::from_f64(f64::from(r.f32()?)).map_or(Value::Null, Value::Number),
            "double" => Number::from_f64(r.f64()?).map_or(Value::Null, Value::Number),
            "string" => {
                let n = r.len(1)?;
                let s = String::from_utf8_lossy(r.take(n)?).into_owned();
                r.align4();
                Value::String(s)
            }
            // Pixels of textures stored inside the object (small icons), kept for `icons`.
            "TypelessData" => {
                let n = r.len(1)?;
                Value::from(r.take(n)?.to_vec())
            }
            "ReferencedObject" => self.referenced(node)?,
            "Array" => self.array(node)?,
            _ => match node.children.first() {
                Some(first) if first.ty == "Array" => self.array(first)?,
                _ => {
                    let mut m = Map::new();
                    for c in &node.children {
                        m.insert(c.name.clone(), self.read(c)?);
                    }
                    Value::Object(m)
                }
            },
        };
        if node.align {
            self.r.align4();
        }
        Ok(v)
    }

    fn array(&mut self, arr: &TypeNode) -> Result<Value> {
        let Some(item) = arr.children.get(1) else {
            return err("array without item type");
        };
        if matches!(item.ty.as_str(), "UInt8" | "char") && item.children.is_empty() {
            let n = self.r.len(1)?;
            self.r.take(n)?;
            return Ok(Value::Null);
        }
        let n = self.r.len(1)?;
        (0..n)
            .map(|_| self.read(item))
            .collect::<Result<Vec<_>>>()
            .map(Value::Array)
    }

    fn referenced(&mut self, node: &TypeNode) -> Result<Value> {
        let child = |name: &str| node.children.iter().find(|c| c.name == name);
        let (Some(rid_node), Some(type_node)) = (child("rid"), child("type")) else {
            return err("ReferencedObject without rid/type");
        };
        let rid = self.read(rid_node)?;
        let ty = self.read(type_node)?;
        let field = |k: &str| {
            ty.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let key = (field("class"), field("ns"), field("asm"));
        let mut out = Map::new();
        out.insert("rid".into(), rid);
        if key.0.is_empty() {
            return Ok(Value::Object(out));
        }
        let Some(tree) = self.refs.get(&key) else {
            return err(format!("no type tree for referenced class {}", key.0));
        };
        let mut data = Map::new();
        for c in &tree.children {
            data.insert(c.name.clone(), self.read(c)?);
        }
        out.insert("class".into(), Value::String(key.0));
        out.insert("data".into(), Value::Object(data));
        Ok(Value::Object(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_strings_sit_at_unity_offsets() {
        assert_eq!(common_string(0), Some("AABB"));
        assert_eq!(common_string(55), Some("Base"));
        assert_eq!(common_string(222), Some("int"));
        assert_eq!(common_string(263), Some("MonoBehaviour"));
        assert_eq!(common_string(427), Some("m_Name"));
        assert_eq!(common_string(840), Some("string"));
        assert_eq!(common_string(1), None);
    }

    #[test]
    fn refuses_a_length_larger_than_the_buffer() {
        let data = 1000i32.to_le_bytes();
        assert!(Reader::new(&data, 0).len(1).is_err());
    }

    #[test]
    fn rejects_non_bundles() {
        assert!(read_bundle(b"NotUnity\0").is_err());
    }
}
