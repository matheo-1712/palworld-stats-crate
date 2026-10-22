//! Generic GVAS (Unreal Engine 5.1 save game) property reader.
//!
//! Every property payload is decoded in isolation from its `size`-bounded
//! sub-slice. A payload that cannot be decoded (unknown type, decode error,
//! or bytes left over) is kept verbatim as [`Value::Raw`] and reported as a
//! [`ParseWarning`]; parsing then continues after the payload.

use crate::{Error, ParseWarning, Result};

/// The GVAS file header.
#[derive(Debug, Clone, PartialEq)]
pub struct GvasHeader {
    pub save_game_version: i32,
    /// Engine version, e.g. `"5.1.1-0+++UE5+Release-5.1"`.
    pub engine: String,
    pub save_class: String,
}

/// A decoded property value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i32),
    Int64(i64),
    UInt32(u32),
    Float(f32),
    Str(String),
    Name(String),
    Bool(bool),
    Byte(u8),
    Enum(String),
    Guid([u8; 16]),
    /// UE `DateTime` ticks (100 ns since 0001-01-01).
    DateTime(u64),
    Struct {
        type_name: String,
        props: Props,
    },
    Array(Vec<Value>),
    Bytes(Vec<u8>),
    Map(Vec<(Value, Value)>),
    /// Skipped or undecodable payload, kept verbatim.
    Raw(Vec<u8>),
}

/// An ordered property list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Props(pub Vec<(String, Value)>);

impl Props {
    /// First property with this name.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.0.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    /// Properties of a [`Value::Struct`].
    pub fn struct_props(&self, name: &str) -> Option<&Props> {
        match self.get(name)? {
            Value::Struct { props, .. } => Some(props),
            _ => None,
        }
    }

    /// `Int`, `Int64`, `UInt32` or `Byte` as `i64`.
    pub fn int(&self, name: &str) -> Option<i64> {
        match self.get(name)? {
            Value::Int(v) => Some(i64::from(*v)),
            Value::Int64(v) => Some(*v),
            Value::UInt32(v) => Some(i64::from(*v)),
            Value::Byte(v) => Some(i64::from(*v)),
            _ => None,
        }
    }

    /// `Str`, `Name` or `Enum`.
    pub fn string(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            Value::Str(s) | Value::Name(s) | Value::Enum(s) => Some(s),
            _ => None,
        }
    }

    pub fn bool(&self, name: &str) -> Option<bool> {
        match self.get(name)? {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn guid(&self, name: &str) -> Option<[u8; 16]> {
        match self.get(name)? {
            Value::Guid(g) => Some(*g),
            _ => None,
        }
    }

    /// `DateTime` in UE ticks.
    pub fn datetime(&self, name: &str) -> Option<u64> {
        match self.get(name)? {
            Value::DateTime(t) => Some(*t),
            _ => None,
        }
    }
}

/// Options steering [`Reader::properties`].
pub struct ParseOptions<'a> {
    /// Full dotted paths (e.g. ".worldSaveData.CharacterSaveParameterMap") whose MapProperty
    /// StructProperty keys are property lists rather than a raw 16-byte Guid.
    pub struct_keys: &'a [&'a str],
    /// Returns true for paths to keep as Value::Raw without decoding (skipped by size).
    pub skip: &'a dyn Fn(&str) -> bool,
}

/// Fixed-layout UE math structs, kept as [`Value::Raw`].
const RAW_STRUCTS: &[&str] = &[
    "Vector",
    "Vector2D",
    "Vector4",
    "Quat",
    "Rotator",
    "LinearColor",
    "Color",
    "IntPoint",
    "IntVector",
    "Box",
    "Box2D",
    "Plane",
    "Matrix",
];

/// Type-specific part of a property tag.
enum TagHeader {
    None,
    Struct(String),
    Inner(String),
    Map(String, String),
    Bool(bool),
}

/// Bounds-checked little-endian reader over GVAS bytes.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    /// Absolute offset of `data[0]` in the original buffer (for error offsets).
    base: usize,
    warnings: Vec<ParseWarning>,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self::with_base(data, 0)
    }

    fn with_base(data: &'a [u8], base: usize) -> Self {
        Reader {
            data,
            pos: 0,
            base,
            warnings: Vec::new(),
        }
    }

    pub fn into_warnings(self) -> Vec<ParseWarning> {
        self.warnings
    }

    /// Read the GVAS file header.
    pub fn header(&mut self) -> Result<GvasHeader> {
        if self.bytes(4)? != b"GVAS" {
            return Err(self.err_at(0, "missing GVAS magic"));
        }
        let save_game_version = self.i32()?;
        let _ue4 = self.u32()?;
        if save_game_version >= 3 {
            let _ue5 = self.u32()?;
        }
        let (major, minor, patch) = (self.u16()?, self.u16()?, self.u16()?);
        let build = self.u32()?;
        let branch = self.fstring()?;
        let _custom_format = self.i32()?;
        let count = self.i32()?;
        let count = usize::try_from(count)
            .map_err(|_| self.err(format!("negative custom version count {count}")))?;
        let len = count
            .checked_mul(20)
            .ok_or_else(|| self.err("custom version count overflow"))?;
        self.bytes(len)?;
        let save_class = self.fstring()?;
        Ok(GvasHeader {
            save_game_version,
            engine: format!("{major}.{minor}.{patch}-{build}+{branch}"),
            save_class,
        })
    }

    /// Read property tags until the `"None"` terminator.
    pub fn properties(&mut self, path: &str, opts: &ParseOptions) -> Result<Props> {
        let mut props = Vec::new();
        loop {
            let name = self.fstring()?;
            if name == "None" {
                return Ok(Props(props));
            }
            let value = self.property(&name, path, opts)?;
            props.push((name, value));
        }
    }

    /// Read one property tag (after its name) and its contained payload.
    fn property(&mut self, name: &str, path: &str, opts: &ParseOptions) -> Result<Value> {
        let ty = self.fstring()?;
        let size = self.u64()?;
        let header = match ty.as_str() {
            "StructProperty" => {
                let st = self.fstring()?;
                self.bytes(16)?;
                TagHeader::Struct(st)
            }
            "ArrayProperty" | "SetProperty" | "ByteProperty" | "EnumProperty" => {
                TagHeader::Inner(self.fstring()?)
            }
            "MapProperty" => TagHeader::Map(self.fstring()?, self.fstring()?),
            "BoolProperty" => TagHeader::Bool(self.u8()? != 0),
            _ => TagHeader::None,
        };
        if self.u8()? != 0 {
            self.bytes(16)?;
        }
        let size = usize::try_from(size).map_err(|_| self.err(format!("size {size} too large")))?;
        let start = self.base + self.pos;
        let payload = self.bytes(size)?;
        let prop_path = format!("{path}.{name}");
        if (opts.skip)(&prop_path) {
            return Ok(Value::Raw(payload.to_vec()));
        }
        let mut sub = Reader::with_base(payload, start);
        let outcome = sub.payload(&ty, &header, &prop_path, opts).and_then(|v| {
            if sub.eof() {
                Ok(v)
            } else {
                Err(sub.err(format!("{} unconsumed payload bytes", sub.remaining())))
            }
        });
        match outcome {
            Ok(v) => {
                self.warnings.append(&mut sub.warnings);
                Ok(v)
            }
            Err(e) => {
                self.warnings.push(ParseWarning {
                    path: prop_path,
                    message: format!("{ty}: {e}"),
                });
                Ok(Value::Raw(payload.to_vec()))
            }
        }
    }

    fn payload(
        &mut self,
        ty: &str,
        header: &TagHeader,
        path: &str,
        opts: &ParseOptions,
    ) -> Result<Value> {
        Ok(match (ty, header) {
            ("IntProperty", _) => Value::Int(self.i32()?),
            ("Int64Property", _) => Value::Int64(self.i64()?),
            ("UInt32Property", _) => Value::UInt32(self.u32()?),
            ("FloatProperty", _) => Value::Float(self.f32()?),
            ("StrProperty", _) => Value::Str(self.fstring()?),
            ("NameProperty", _) => Value::Name(self.fstring()?),
            ("BoolProperty", TagHeader::Bool(b)) => Value::Bool(*b),
            ("EnumProperty", _) => Value::Enum(self.fstring()?),
            ("ByteProperty", TagHeader::Inner(enum_name)) => {
                if enum_name == "None" {
                    Value::Byte(self.u8()?)
                } else {
                    Value::Enum(self.fstring()?)
                }
            }
            ("StructProperty", TagHeader::Struct(st)) => {
                let len = self.remaining();
                self.struct_value(st, path, opts, len)?
            }
            ("ArrayProperty", TagHeader::Inner(inner)) => self.array(inner, path, opts)?,
            ("MapProperty", TagHeader::Map(k, v)) => self.map(k, v, path, opts)?,
            ("SetProperty", _) => Value::Raw(self.bytes(self.remaining())?.to_vec()),
            _ => return Err(self.err(format!("unknown property type {ty}"))),
        })
    }

    /// A struct value of type `st`; `raw_len` is the byte length used for math structs.
    fn struct_value(
        &mut self,
        st: &str,
        path: &str,
        opts: &ParseOptions,
        raw_len: usize,
    ) -> Result<Value> {
        Ok(match st {
            "Guid" => Value::Guid(self.guid()?),
            "DateTime" => Value::DateTime(self.u64()?),
            _ if RAW_STRUCTS.contains(&st) => Value::Raw(self.bytes(raw_len)?.to_vec()),
            _ => Value::Struct {
                type_name: st.to_string(),
                props: self.properties(path, opts)?,
            },
        })
    }

    fn array(&mut self, inner: &str, path: &str, opts: &ParseOptions) -> Result<Value> {
        let count = self.u32()? as usize;
        Ok(match inner {
            "ByteProperty" => Value::Bytes(self.bytes(count)?.to_vec()),
            "StructProperty" => {
                let prop_name = self.fstring()?;
                let _prop_type = self.fstring()?;
                let size = self.u64()?;
                let st = self.fstring()?;
                // 16-byte struct guid + 1 optional-guid flag byte.
                self.bytes(17)?;
                let size = usize::try_from(size)
                    .map_err(|_| self.err(format!("size {size} too large")))?;
                let elem_len = size.checked_div(count).unwrap_or(0);
                let elem_path = format!("{path}.{prop_name}");
                let mut values = Vec::with_capacity(count.min(self.remaining()));
                for _ in 0..count {
                    values.push(self.struct_value(&st, &elem_path, opts, elem_len)?);
                }
                Value::Array(values)
            }
            "NameProperty" | "EnumProperty" | "StrProperty" | "IntProperty" => {
                let mut values = Vec::with_capacity(count.min(self.remaining()));
                for _ in 0..count {
                    values.push(match inner {
                        "NameProperty" => Value::Name(self.fstring()?),
                        "EnumProperty" => Value::Enum(self.fstring()?),
                        "StrProperty" => Value::Str(self.fstring()?),
                        _ => Value::Int(self.i32()?),
                    });
                }
                Value::Array(values)
            }
            _ => Value::Raw(self.bytes(self.remaining())?.to_vec()),
        })
    }

    fn map(&mut self, key: &str, value: &str, path: &str, opts: &ParseOptions) -> Result<Value> {
        let _removed = self.u32()?;
        let count = self.u32()? as usize;
        let struct_key = opts.struct_keys.contains(&path);
        let key_path = format!("{path}.Key");
        let value_path = format!("{path}.Value");
        let mut entries = Vec::with_capacity(count.min(self.remaining()));
        for _ in 0..count {
            let k = self.map_element(key, &key_path, opts, !struct_key)?;
            let v = self.map_element(value, &value_path, opts, false)?;
            entries.push((k, v));
        }
        Ok(Value::Map(entries))
    }

    fn map_element(
        &mut self,
        ty: &str,
        path: &str,
        opts: &ParseOptions,
        struct_is_guid: bool,
    ) -> Result<Value> {
        Ok(match ty {
            "NameProperty" => Value::Name(self.fstring()?),
            "EnumProperty" => Value::Enum(self.fstring()?),
            "StrProperty" => Value::Str(self.fstring()?),
            "IntProperty" => Value::Int(self.i32()?),
            "Int64Property" => Value::Int64(self.i64()?),
            "UInt32Property" => Value::UInt32(self.u32()?),
            "BoolProperty" => Value::Bool(self.u8()? != 0),
            "StructProperty" if struct_is_guid => Value::Guid(self.guid()?),
            "StructProperty" => Value::Struct {
                type_name: "StructProperty".to_string(),
                props: self.properties(path, opts)?,
            },
            _ => return Err(self.err(format!("unsupported map element type {ty}"))),
        })
    }

    // ---- bounded primitives (also used by RawData decoders) ----------------

    pub(crate) fn err(&self, msg: impl Into<String>) -> Error {
        self.err_at(self.pos, msg)
    }

    fn err_at(&self, pos: usize, msg: impl Into<String>) -> Error {
        Error::Gvas {
            offset: self.base + pos,
            msg: msg.into(),
        }
    }

    /// Current position, relative to this reader's slice.
    #[allow(dead_code)]
    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub(crate) fn eof(&self) -> bool {
        self.pos >= self.data.len()
    }

    /// Next `n` bytes, or `Err` if fewer remain.
    pub(crate) fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let data: &'a [u8] = self.data;
        match self
            .pos
            .checked_add(n)
            .and_then(|end| data.get(self.pos..end))
        {
            Some(out) => {
                self.pos += n;
                Ok(out)
            }
            None => Err(self.err(format!("need {n} bytes, {} remaining", self.remaining()))),
        }
    }

    fn array_n<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.bytes(N)?);
        Ok(out)
    }

    pub(crate) fn u8(&mut self) -> Result<u8> {
        let [b] = self.array_n::<1>()?;
        Ok(b)
    }
    pub(crate) fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array_n()?))
    }
    pub(crate) fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.array_n()?))
    }
    pub(crate) fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array_n()?))
    }
    pub(crate) fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.array_n()?))
    }
    pub(crate) fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.array_n()?))
    }
    pub(crate) fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.array_n()?))
    }
    pub(crate) fn guid(&mut self) -> Result<[u8; 16]> {
        self.array_n()
    }

    /// UE FString: `i32` len; `>0` 8-bit incl. NUL, `<0` UTF-16LE units incl. NUL.
    pub(crate) fn fstring(&mut self) -> Result<String> {
        let len = self.i32()?;
        if len == 0 {
            return Ok(String::new());
        }
        if len > 0 {
            let b = self.bytes(len as usize)?;
            let b = b.strip_suffix(&[0]).unwrap_or(b);
            Ok(match std::str::from_utf8(b) {
                Ok(s) => s.to_string(),
                // Latin-1 fallback.
                Err(_) => b.iter().map(|&c| char::from(c)).collect(),
            })
        } else {
            let n = (len.unsigned_abs() as usize)
                .checked_mul(2)
                .ok_or_else(|| self.err("FString length overflow"))?;
            let b = self.bytes(n)?;
            let (pairs, _) = b.as_chunks::<2>();
            let mut units: Vec<u16> = pairs.iter().map(|&p| u16::from_le_bytes(p)).collect();
            if units.last() == Some(&0) {
                units.pop();
            }
            Ok(String::from_utf16_lossy(&units))
        }
    }
}

/// Convenience: header + top-level properties.
pub fn parse(data: &[u8], opts: &ParseOptions) -> Result<(GvasHeader, Props, Vec<ParseWarning>)> {
    let mut r = Reader::new(data);
    let header = r.header()?;
    let props = r.properties("", opts)?;
    Ok((header, props, r.into_warnings()))
}

#[cfg(test)]
pub(crate) mod build {
    //! Byte builders for synthetic GVAS test data.

    pub(crate) fn fstring(buf: &mut Vec<u8>, s: &str) {
        if s.is_empty() {
            buf.extend_from_slice(&0i32.to_le_bytes());
            return;
        }
        let len = s.len() as i32 + 1;
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(s.as_bytes());
        buf.push(0);
    }

    /// Negative-length (UTF-16LE) FString.
    pub(crate) fn fstring_utf16(buf: &mut Vec<u8>, s: &str) {
        let units: Vec<u16> = s.encode_utf16().collect();
        let len = -(units.len() as i32 + 1);
        buf.extend_from_slice(&len.to_le_bytes());
        for u in units {
            buf.extend_from_slice(&u.to_le_bytes());
        }
        buf.extend_from_slice(&[0, 0]);
    }

    /// Property tag: name, type, u64 size, `extra_header`, guid flag 0, payload.
    pub(crate) fn tag(
        buf: &mut Vec<u8>,
        name: &str,
        ty: &str,
        extra_header: &[u8],
        payload: &[u8],
    ) {
        fstring(buf, name);
        fstring(buf, ty);
        buf.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        buf.extend_from_slice(extra_header);
        buf.push(0);
        buf.extend_from_slice(payload);
    }

    pub(crate) fn none(buf: &mut Vec<u8>) {
        fstring(buf, "None");
    }

    /// Minimal valid header: version 3, 0 custom versions, class "/Script/Test".
    pub(crate) fn header_bytes() -> Vec<u8> {
        let mut b = b"GVAS".to_vec();
        b.extend_from_slice(&3i32.to_le_bytes());
        b.extend_from_slice(&522u32.to_le_bytes());
        b.extend_from_slice(&1009u32.to_le_bytes());
        for v in [5u16, 1, 1] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&0u32.to_le_bytes());
        fstring(&mut b, "++UE5+Release-5.1");
        b.extend_from_slice(&3i32.to_le_bytes());
        b.extend_from_slice(&0i32.to_le_bytes());
        fstring(&mut b, "/Script/Test");
        b
    }

    /// `fstring` into a fresh Vec (for header extras / payloads).
    pub(crate) fn fs(s: &str) -> Vec<u8> {
        let mut v = Vec::new();
        fstring(&mut v, s);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::build::*;
    use super::*;

    fn no_skip(_: &str) -> bool {
        false
    }

    fn opts() -> ParseOptions<'static> {
        ParseOptions {
            struct_keys: &[],
            skip: &no_skip,
        }
    }

    fn doc(props: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
        let mut b = header_bytes();
        props(&mut b);
        none(&mut b);
        b.extend_from_slice(&[0, 0, 0, 0]);
        b
    }

    fn guid_struct_header(st: &str) -> Vec<u8> {
        let mut h = fs(st);
        h.extend_from_slice(&[0u8; 16]);
        h
    }

    #[test]
    fn header_parses() {
        let data = doc(|_| {});
        let (h, props, warnings) = parse(&data, &opts()).unwrap();
        assert_eq!(h.save_class, "/Script/Test");
        assert_eq!(h.save_game_version, 3);
        assert_eq!(h.engine, "5.1.1-0+++UE5+Release-5.1");
        assert!(props.0.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn bad_magic_is_err() {
        let mut data = doc(|_| {});
        data[0] = b'X';
        assert!(parse(&data, &opts()).is_err());
    }

    #[test]
    fn scalar_properties() {
        let data = doc(|b| {
            tag(b, "I", "IntProperty", &[], &42i32.to_le_bytes());
            tag(b, "L", "Int64Property", &[], &(-5i64).to_le_bytes());
            tag(b, "U", "UInt32Property", &[], &7u32.to_le_bytes());
            tag(b, "F", "FloatProperty", &[], &1.5f32.to_le_bytes());
            tag(b, "S", "StrProperty", &[], &fs("héllo"));
            tag(b, "N", "NameProperty", &[], &fs("SomeName"));
            tag(b, "B", "BoolProperty", &[1], &[]);
            tag(b, "Bf", "BoolProperty", &[0], &[]);
            tag(b, "Y", "ByteProperty", &fs("None"), &[80]);
            tag(b, "YE", "ByteProperty", &fs("EPalFoo"), &fs("EPalFoo::Bar"));
            tag(b, "E", "EnumProperty", &fs("EPalX"), &fs("EPalX::Y"));
        });
        let (_, p, w) = parse(&data, &opts()).unwrap();
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(p.int("I"), Some(42));
        assert_eq!(p.int("L"), Some(-5));
        assert_eq!(p.int("U"), Some(7));
        assert_eq!(p.get("F"), Some(&Value::Float(1.5)));
        assert_eq!(p.string("S"), Some("héllo"));
        assert_eq!(p.string("N"), Some("SomeName"));
        assert_eq!(p.get("N"), Some(&Value::Name("SomeName".into())));
        assert_eq!(p.bool("B"), Some(true));
        assert_eq!(p.bool("Bf"), Some(false));
        assert_eq!(p.int("Y"), Some(80));
        assert_eq!(p.string("YE"), Some("EPalFoo::Bar"));
        assert_eq!(p.string("E"), Some("EPalX::Y"));
        assert_eq!(p.int("missing"), None);
        assert_eq!(p.string("I"), None);
    }

    #[test]
    fn utf16_string() {
        let data = doc(|b| {
            let mut payload = Vec::new();
            fstring_utf16(&mut payload, "Lóutre✓");
            tag(b, "S", "StrProperty", &[], &payload);
        });
        let (_, p, w) = parse(&data, &opts()).unwrap();
        assert!(w.is_empty());
        assert_eq!(p.string("S"), Some("Lóutre✓"));
    }

    #[test]
    fn nested_struct_and_guid() {
        let g: [u8; 16] = core::array::from_fn(|i| i as u8 + 1);
        let data = doc(|b| {
            let mut inner = Vec::new();
            tag(
                &mut inner,
                "G",
                "StructProperty",
                &guid_struct_header("Guid"),
                &g,
            );
            tag(
                &mut inner,
                "T",
                "StructProperty",
                &guid_struct_header("DateTime"),
                &123u64.to_le_bytes(),
            );
            tag(
                &mut inner,
                "V",
                "StructProperty",
                &guid_struct_header("Vector"),
                &[9u8; 24],
            );
            none(&mut inner);
            tag(b, "S", "StructProperty", &guid_struct_header("Foo"), &inner);
        });
        let (_, p, w) = parse(&data, &opts()).unwrap();
        assert!(w.is_empty(), "{w:?}");
        let s = p.struct_props("S").unwrap();
        assert_eq!(s.guid("G"), Some(g));
        assert_eq!(s.datetime("T"), Some(123));
        assert_eq!(s.get("V"), Some(&Value::Raw(vec![9u8; 24])));
        match p.get("S") {
            Some(Value::Struct { type_name, .. }) => assert_eq!(type_name, "Foo"),
            other => panic!("{other:?}"),
        }
    }

    fn map_header(k: &str, v: &str) -> Vec<u8> {
        let mut h = fs(k);
        h.extend(fs(v));
        h
    }

    #[test]
    fn map_name_to_int_and_bool() {
        let data = doc(|b| {
            let mut m = Vec::new();
            m.extend_from_slice(&0u32.to_le_bytes());
            m.extend_from_slice(&2u32.to_le_bytes());
            m.extend(fs("a"));
            m.extend_from_slice(&1i32.to_le_bytes());
            m.extend(fs("b"));
            m.extend_from_slice(&(-2i32).to_le_bytes());
            tag(
                b,
                "MI",
                "MapProperty",
                &map_header("NameProperty", "IntProperty"),
                &m,
            );

            let mut m = Vec::new();
            m.extend_from_slice(&0u32.to_le_bytes());
            m.extend_from_slice(&1u32.to_le_bytes());
            m.extend(fs("x"));
            m.push(1);
            tag(
                b,
                "MB",
                "MapProperty",
                &map_header("NameProperty", "BoolProperty"),
                &m,
            );
        });
        let (_, p, w) = parse(&data, &opts()).unwrap();
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(
            p.get("MI"),
            Some(&Value::Map(vec![
                (Value::Name("a".into()), Value::Int(1)),
                (Value::Name("b".into()), Value::Int(-2)),
            ]))
        );
        assert_eq!(
            p.get("MB"),
            Some(&Value::Map(vec![(
                Value::Name("x".into()),
                Value::Bool(true)
            )]))
        );
    }

    #[test]
    fn map_struct_key_default_guid_vs_struct_keys() {
        // A map whose StructProperty key is a property list, parsed with and
        // without its path in `struct_keys`; then a map with a real Guid key.
        let g = [0xABu8; 16];
        let mut value = Vec::new();
        tag(&mut value, "Level", "IntProperty", &[], &5i32.to_le_bytes());
        none(&mut value);

        // Key as a property list.
        let mut key_props = Vec::new();
        tag(
            &mut key_props,
            "PlayerUId",
            "StructProperty",
            &guid_struct_header("Guid"),
            &g,
        );
        none(&mut key_props);

        let mut m = Vec::new();
        m.extend_from_slice(&0u32.to_le_bytes());
        m.extend_from_slice(&1u32.to_le_bytes());
        m.extend(&key_props);
        m.extend(&value);
        let data = doc(|b| {
            tag(
                b,
                "Chars",
                "MapProperty",
                &map_header("StructProperty", "StructProperty"),
                &m,
            )
        });

        // With the path in struct_keys: key decodes as a property list.
        let keys = [".Chars"];
        let o = ParseOptions {
            struct_keys: &keys,
            skip: &no_skip,
        };
        let (_, p, w) = parse(&data, &o).unwrap();
        assert!(w.is_empty(), "{w:?}");
        let Some(Value::Map(entries)) = p.get("Chars") else {
            panic!("not a map")
        };
        assert_eq!(entries.len(), 1);
        let Value::Struct { props: kp, .. } = &entries[0].0 else {
            panic!("key not struct: {:?}", entries[0].0)
        };
        assert_eq!(kp.guid("PlayerUId"), Some(g));
        let Value::Struct { props: vp, .. } = &entries[0].1 else {
            panic!("value not struct")
        };
        assert_eq!(vp.int("Level"), Some(5));

        // Without: the same bytes are read with a 16-byte Guid key, which
        // misaligns the value -> contained as Raw with a warning.
        let (_, p, w) = parse(&data, &opts()).unwrap();
        assert_eq!(p.get("Chars"), Some(&Value::Raw(m.clone())));
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].path, ".Chars");

        // And a map whose key really is a raw Guid decodes by default.
        let mut m2 = Vec::new();
        m2.extend_from_slice(&0u32.to_le_bytes());
        m2.extend_from_slice(&1u32.to_le_bytes());
        m2.extend_from_slice(&g);
        m2.extend(&value);
        let data2 = doc(|b| {
            tag(
                b,
                "Chars",
                "MapProperty",
                &map_header("StructProperty", "StructProperty"),
                &m2,
            )
        });
        let (_, p, w) = parse(&data2, &opts()).unwrap();
        assert!(w.is_empty(), "{w:?}");
        let Some(Value::Map(entries)) = p.get("Chars") else {
            panic!("not a map")
        };
        assert_eq!(entries[0].0, Value::Guid(g));
    }

    #[test]
    fn byte_array_becomes_bytes() {
        let data = doc(|b| {
            let mut a = 4u32.to_le_bytes().to_vec();
            a.extend_from_slice(&[1, 2, 3, 4]);
            tag(b, "Raw", "ArrayProperty", &fs("ByteProperty"), &a);
        });
        let (_, p, w) = parse(&data, &opts()).unwrap();
        assert!(w.is_empty());
        assert_eq!(p.get("Raw"), Some(&Value::Bytes(vec![1, 2, 3, 4])));
    }

    #[test]
    fn struct_and_name_and_int_arrays() {
        let data = doc(|b| {
            let mut a = 2u32.to_le_bytes().to_vec();
            a.extend(fs("x"));
            a.extend(fs("y"));
            tag(b, "Names", "ArrayProperty", &fs("NameProperty"), &a);

            let mut a = 2u32.to_le_bytes().to_vec();
            a.extend_from_slice(&3i32.to_le_bytes());
            a.extend_from_slice(&4i32.to_le_bytes());
            tag(b, "Ints", "ArrayProperty", &fs("IntProperty"), &a);

            let mut elems = Vec::new();
            for i in 0..2i32 {
                tag(&mut elems, "Lv", "IntProperty", &[], &i.to_le_bytes());
                none(&mut elems);
            }
            let mut a = 2u32.to_le_bytes().to_vec();
            a.extend(fs("Items"));
            a.extend(fs("StructProperty"));
            a.extend_from_slice(&(elems.len() as u64).to_le_bytes());
            a.extend(fs("Item"));
            a.extend_from_slice(&[0u8; 17]);
            a.extend(&elems);
            tag(b, "Items", "ArrayProperty", &fs("StructProperty"), &a);
        });
        let (_, p, w) = parse(&data, &opts()).unwrap();
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(
            p.get("Names"),
            Some(&Value::Array(vec![
                Value::Name("x".into()),
                Value::Name("y".into())
            ]))
        );
        assert_eq!(
            p.get("Ints"),
            Some(&Value::Array(vec![Value::Int(3), Value::Int(4)]))
        );
        let Some(Value::Array(items)) = p.get("Items") else {
            panic!()
        };
        assert_eq!(items.len(), 2);
        let Value::Struct { type_name, props } = &items[1] else {
            panic!()
        };
        assert_eq!(type_name, "Item");
        assert_eq!(props.int("Lv"), Some(1));
    }

    #[test]
    fn unknown_property_type_becomes_raw() {
        let data = doc(|b| {
            tag(b, "Weird", "WeirdProperty", &[], &[1, 2, 3]);
            tag(b, "After", "IntProperty", &[], &7i32.to_le_bytes());
        });
        let (_, p, w) = parse(&data, &opts()).unwrap();
        assert_eq!(p.get("Weird"), Some(&Value::Raw(vec![1, 2, 3])));
        assert_eq!(p.int("After"), Some(7));
        assert_eq!(w.len(), 1);
        assert!(w[0].path.ends_with("Weird"), "{:?}", w[0]);
    }

    #[test]
    fn unconsumed_payload_becomes_raw() {
        let data = doc(|b| {
            tag(b, "Long", "IntProperty", &[], &[1, 0, 0, 0, 9]);
            tag(b, "After", "IntProperty", &[], &7i32.to_le_bytes());
        });
        let (_, p, w) = parse(&data, &opts()).unwrap();
        assert_eq!(p.get("Long"), Some(&Value::Raw(vec![1, 0, 0, 0, 9])));
        assert_eq!(p.int("After"), Some(7));
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].path, ".Long");
    }

    #[test]
    fn nested_warning_path_is_dotted() {
        let data = doc(|b| {
            let mut inner = Vec::new();
            tag(&mut inner, "Odd", "WeirdProperty", &[], &[0]);
            tag(&mut inner, "Ok", "IntProperty", &[], &1i32.to_le_bytes());
            none(&mut inner);
            tag(b, "S", "StructProperty", &guid_struct_header("Foo"), &inner);
        });
        let (_, p, w) = parse(&data, &opts()).unwrap();
        assert_eq!(p.struct_props("S").unwrap().int("Ok"), Some(1));
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].path, ".S.Odd");
    }

    #[test]
    fn skip_predicate_keeps_raw_without_warning() {
        let data = doc(|b| {
            let mut inner = Vec::new();
            tag(&mut inner, "Big", "IntProperty", &[], &5i32.to_le_bytes());
            none(&mut inner);
            tag(b, "S", "StructProperty", &guid_struct_header("Foo"), &inner);
            tag(b, "Junk", "WeirdProperty", &[], &[1, 2]);
            tag(b, "After", "IntProperty", &[], &7i32.to_le_bytes());
        });
        let skip = |p: &str| p == ".S" || p == ".Junk";
        let o = ParseOptions {
            struct_keys: &[],
            skip: &skip,
        };
        let (_, p, w) = parse(&data, &o).unwrap();
        assert!(w.is_empty(), "{w:?}");
        assert!(matches!(p.get("S"), Some(Value::Raw(b)) if !b.is_empty()));
        assert_eq!(p.get("Junk"), Some(&Value::Raw(vec![1, 2])));
        assert_eq!(p.int("After"), Some(7));
    }

    #[test]
    fn truncated_input_is_err_not_panic() {
        let g = [7u8; 16];
        let data = doc(|b| {
            tag(b, "I", "IntProperty", &[], &42i32.to_le_bytes());
            tag(b, "S", "StrProperty", &[], &fs("héllo"));
            tag(b, "B", "BoolProperty", &[1], &[]);
            let mut inner = Vec::new();
            tag(
                &mut inner,
                "G",
                "StructProperty",
                &guid_struct_header("Guid"),
                &g,
            );
            none(&mut inner);
            tag(
                b,
                "St",
                "StructProperty",
                &guid_struct_header("Foo"),
                &inner,
            );
            let mut m = Vec::new();
            m.extend_from_slice(&0u32.to_le_bytes());
            m.extend_from_slice(&1u32.to_le_bytes());
            m.extend(fs("a"));
            m.extend_from_slice(&1i32.to_le_bytes());
            tag(
                b,
                "M",
                "MapProperty",
                &map_header("NameProperty", "IntProperty"),
                &m,
            );
            let mut a = 2u32.to_le_bytes().to_vec();
            a.extend_from_slice(&[1, 2]);
            tag(b, "A", "ArrayProperty", &fs("ByteProperty"), &a);
        });
        assert!(parse(&data, &opts()).is_ok());
        for len in 0..data.len() {
            let _ = parse(&data[..len], &opts());
        }
        // Absurd lengths must not allocate or panic either.
        let mut bad = header_bytes();
        bad.extend_from_slice(&i32::MIN.to_le_bytes());
        assert!(parse(&bad, &opts()).is_err());
        let mut bad = header_bytes();
        fstring(&mut bad, "X");
        fstring(&mut bad, "IntProperty");
        bad.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(parse(&bad, &opts()).is_err());
    }

    #[test]
    fn reader_primitives_are_bounded() {
        let mut r = Reader::new(&[1, 0, 0]);
        assert!(r.i32().is_err());
        assert_eq!(r.remaining(), 3);
        assert_eq!(r.u8().unwrap(), 1);
        assert!(!r.eof());
        assert!(r.guid().is_err());
    }
}
