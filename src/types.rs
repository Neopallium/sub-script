use std::any::TypeId;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fs::File;
use std::io::BufReader;
use std::sync::{Arc, RwLock};

use parity_scale_codec::{Compact, Decode, Encode, Error as PError, Input};
use serde_json::{Map, Value};

use sp_runtime::generic::Era;

use rust_decimal::{prelude::ToPrimitive, Decimal};

use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, Map as RMap};
use smartstring::{LazyCompact, SmartString};

use indexmap::map::IndexMap;

use super::engine::EngineOptions;
use super::metadata::EncodedArgs;
use super::users::{AccountId, SharedUser};

#[derive(Clone, Debug, Default)]
pub struct EnumVariant {
  idx: u8,
  name: String,
  type_ref: Option<TypeRef>,
}

#[derive(Clone, Debug, Default)]
pub struct EnumVariants {
  variants: Vec<Option<EnumVariant>>,
  name_map: HashMap<String, u8>,
}

impl EnumVariants {
  pub fn new() -> Self {
    Default::default()
  }

  pub fn insert_at(&mut self, idx: u8, name: &str, type_ref: Option<TypeRef>) {
    let len = idx as usize;
    while len > self.variants.len() {
      self.variants.push(None);
    }
    let insert_idx = self.insert(name, type_ref);
    assert!(insert_idx == idx);
  }

  pub fn insert(&mut self, name: &str, type_ref: Option<TypeRef>) -> u8 {
    let idx = self.variants.len() as u8;
    self.variants.push(Some(EnumVariant {
      idx,
      name: name.into(),
      type_ref,
    }));
    self.name_map.insert(name.into(), idx);
    idx
  }

  pub fn get_by_idx(&self, idx: u8) -> Option<&EnumVariant> {
    self.variants.get(idx as usize).and_then(|v| v.as_ref())
  }

  pub fn get_by_name(&self, name: &str) -> Option<&EnumVariant> {
    self
      .name_map
      .get(name)
      .and_then(|idx| self.get_by_idx(*idx))
  }
}

#[derive(Clone)]
pub struct WrapEncodeFn(Arc<dyn Fn(Dynamic, &mut EncodedArgs) -> Result<(), Box<EvalAltResult>>>);

impl WrapEncodeFn {
  pub fn encode_value(
    &self,
    value: Dynamic,
    data: &mut EncodedArgs,
  ) -> Result<(), Box<EvalAltResult>> {
    self.0(value, data)
  }
}

pub struct BoxedInput<'a>(Box<&'a mut dyn Input>);

impl<'a> BoxedInput<'a> {
  pub fn new(input: &'a mut dyn Input) -> Self {
    let boxed = Box::new(input);
    Self(boxed)
  }
}

impl<'a> Input for BoxedInput<'a> {
  fn remaining_len(&mut self) -> Result<Option<usize>, PError> {
    self.0.remaining_len()
  }

  fn read(&mut self, into: &mut [u8]) -> Result<(), PError> {
    self.0.read(into)
  }
}

#[derive(Clone)]
pub struct WrapDecodeFn(Arc<dyn Fn(BoxedInput) -> Result<Dynamic, PError>>);

impl WrapDecodeFn {
  pub fn decode_value<I: Input>(&self, input: &mut I) -> Result<Dynamic, PError> {
    let boxed = BoxedInput::new(input);
    self.0(boxed)
  }
}

#[derive(Clone)]
pub struct CustomType {
  encode_map: HashMap<TypeId, WrapEncodeFn>,
  decode: Option<WrapDecodeFn>,
  type_meta: Box<TypeMeta>,
}

impl std::fmt::Debug for CustomType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_fmt(format_args!("CustomType({:?})", self.type_meta))
  }
}

impl CustomType {
  pub fn new(type_meta: TypeMeta) -> Self {
    Self {
      encode_map: Default::default(),
      decode: None,
      type_meta: Box::new(type_meta),
    }
  }

  pub fn custom_encode(&mut self, type_id: TypeId, func: WrapEncodeFn) {
    self.encode_map.insert(type_id, func);
  }

  pub fn custom_decode(&mut self, func: WrapDecodeFn) {
    self.decode = Some(func);
  }

  pub fn encode_value(
    &self,
    value: Dynamic,
    data: &mut EncodedArgs,
  ) -> Result<(), Box<EvalAltResult>> {
    let type_id = value.type_id();
    log::debug!("encode Custom: type_id={:?}", type_id);
    if let Some(func) = self.encode_map.get(&type_id) {
      func.encode_value(value, data)
    } else {
      self.type_meta.encode_value(value, data)
    }
  }

  pub fn decode_value<I: Input>(
    &self,
    input: &mut I,
    _is_compact: bool,
  ) -> Result<Dynamic, PError> {
    match &self.decode {
      Some(func) => func.decode_value(input),
      None => self.type_meta.decode_value(input, false),
    }
  }
}

#[derive(Clone)]
pub struct TypeRef(Arc<RwLock<TypeMeta>>);

impl TypeRef {
  fn to_string(&mut self) -> String {
    format!("TypeRef: {:?}", self.0.read().unwrap())
  }

  pub fn custom_encode(&self, type_id: TypeId, func: WrapEncodeFn) {
    self.0.write().unwrap().custom_encode(type_id, func)
  }

  pub fn custom_decode(&self, func: WrapDecodeFn) {
    self.0.write().unwrap().custom_decode(func)
  }

  pub fn encode_value(
    &self,
    value: Dynamic,
    data: &mut EncodedArgs,
  ) -> Result<(), Box<EvalAltResult>> {
    self.0.read().unwrap().encode_value(value, data)
  }

  pub fn decode_value<I: Input>(&self, input: &mut I, is_compact: bool) -> Result<Dynamic, PError> {
    self.0.read().unwrap().decode_value(input, is_compact)
  }

  pub fn encode(&self, value: Dynamic) -> Result<Vec<u8>, Box<EvalAltResult>> {
    let mut data = EncodedArgs::new();
    self.encode_value(value, &mut data)?;
    Ok(data.into_inner())
  }

  pub fn decode(&self, data: Vec<u8>) -> Result<Dynamic, Box<EvalAltResult>> {
    Ok(
      self
        .decode_value(&mut &data[..], false)
        .map_err(|e| e.to_string())?,
    )
  }

  pub fn encode_mut(&mut self, value: Dynamic) -> Result<Vec<u8>, Box<EvalAltResult>> {
    self.encode(value)
  }

  pub fn decode_mut(&mut self, data: Vec<u8>) -> Result<Dynamic, Box<EvalAltResult>> {
    self.decode(data)
  }

  pub fn is_u8(&self) -> bool {
    let self_meta = self.0.read().unwrap();
    match &*self_meta {
      TypeMeta::Integer(1, false) => true,
      _ => false,
    }
  }
}

impl From<TypeMeta> for TypeRef {
  fn from(meta: TypeMeta) -> Self {
    TypeRef(Arc::new(RwLock::new(meta.clone())))
  }
}

impl std::fmt::Debug for TypeRef {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
    let meta = self.0.read().unwrap();
    match &*meta {
      TypeMeta::NewType(name, _) => f.write_fmt(format_args!("NewType({})", name)),
      _ => meta.fmt(f),
    }
  }
}

#[derive(Debug, Clone)]
pub enum TypeMeta {
  /// Zero-sized `()`
  Unit,
  /// (width, signed)
  Integer(u8, bool),
  Bool,
  Option(TypeRef),
  Box(TypeRef),
  /// Special case for `Option<bool>`
  OptionBool,
  /// (ok, err)
  Result(TypeRef, TypeRef),
  Vector(TypeRef),
  /// Fixed length.
  Slice(usize, TypeRef),
  String,

  Tuple(Vec<TypeRef>),
  Struct(IndexMap<String, TypeRef>),
  Enum(EnumVariants),

  Compact(TypeRef),
  NewType(String, TypeRef),

  Unresolved(String),

  CustomType(CustomType),
}

impl Default for TypeMeta {
  fn default() -> Self {
    Self::Unit
  }
}

impl TypeMeta {
  fn to_string(&mut self) -> String {
    format!("TypeMeta: {:?}", self)
  }

  fn make_custom_type(&mut self) {
    match self {
      TypeMeta::CustomType(_) => {
        // already wrapped.
        return;
      }
      _ => (),
    }
    let meta = self.clone();
    *self = TypeMeta::CustomType(CustomType::new(meta));
  }

  pub fn custom_encode(&mut self, type_id: TypeId, func: WrapEncodeFn) {
    self.make_custom_type();
    match self {
      TypeMeta::CustomType(custom) => {
        custom.custom_encode(type_id, func);
      }
      _ => unreachable!(),
    }
  }

  pub fn custom_decode(&mut self, func: WrapDecodeFn) {
    self.make_custom_type();
    match self {
      TypeMeta::CustomType(custom) => {
        custom.custom_decode(func);
      }
      _ => unreachable!(),
    }
  }

  pub fn encode_value(
    &self,
    value: Dynamic,
    data: &mut EncodedArgs,
  ) -> Result<(), Box<EvalAltResult>> {
    log::debug!("encode TypeMeta: {:?}", self);
    match self {
      TypeMeta::Unit => (),
      TypeMeta::Integer(len, signed) => {
        if let Some(num) = value.as_int().ok() {
          match (len, signed) {
            (_, false) if data.is_compact() => data.encode(Compact::<u128>(num as u128)),
            (1, true) => data.encode(num as i8),
            (1, false) => data.encode(num as u8),
            (2, true) => data.encode(num as i16),
            (2, false) => data.encode(num as u16),
            (4, true) => data.encode(num as i32),
            (4, false) => data.encode(num as u32),
            (8, true) => data.encode(num as i64),
            (8, false) => data.encode(num as u64),
            (16, true) => data.encode(num as i128),
            (16, false) => data.encode(num as u128),
            _ => Err(format!("Unsupported integer type: {:?}", self))?,
          }
        } else if let Some(mut dec) = value.as_decimal().ok() {
          match (len, signed) {
            (_, false) if data.is_compact() => {
              dec *= Decimal::from(1000_000u64);
              let num = dec
                .to_u128()
                .ok_or_else(|| format!("Expected unsigned integer"))?;
              data.encode(Compact::<u128>(num))
            }
            (1, true) => data.encode(
              dec
                .to_i8()
                .ok_or_else(|| format!("Integer too large for `i8`"))?,
            ),
            (1, false) => data.encode(
              dec
                .to_u8()
                .ok_or_else(|| format!("Integer too large for `u8` or negative."))?,
            ),
            (2, true) => data.encode(
              dec
                .to_i16()
                .ok_or_else(|| format!("Integer too large for `i16`"))?,
            ),
            (2, false) => data.encode(
              dec
                .to_u16()
                .ok_or_else(|| format!("Integer too large for `u16` or negative."))?,
            ),
            (4, true) => data.encode(
              dec
                .to_i32()
                .ok_or_else(|| format!("Integer too large for `i32`"))?,
            ),
            (4, false) => data.encode(
              dec
                .to_u32()
                .ok_or_else(|| format!("Integer too large for `u32` or negative."))?,
            ),
            (8, true) => data.encode(
              dec
                .to_i64()
                .ok_or_else(|| format!("Integer too large for `i64`"))?,
            ),
            (8, false) => data.encode(
              dec
                .to_u64()
                .ok_or_else(|| format!("Integer too large for `u64` or negative."))?,
            ),
            (16, signed) => {
              // TODO: Add support for other decimal scales.
              dec *= Decimal::from(1000_000u64);
              if *signed {
                data.encode(
                  dec
                    .to_i128()
                    .ok_or_else(|| format!("Integer too large for `u128`."))?,
                )
              } else {
                data.encode(
                  dec
                    .to_u128()
                    .ok_or_else(|| format!("Expected a non-negative integer/decimal."))?,
                )
              }
            }
            _ => Err(format!("Unsupported integer type: {:?}", self))?,
          }
        } else {
          Err(format!(
            "Expected an integer or decimal value, got {:?}",
            value
          ))?;
        }
      }
      TypeMeta::Bool => data.encode(value.as_bool()?),
      TypeMeta::Option(type_ref) => {
        if value.is::<()>() {
          // None
          data.encode(0u8);
        } else {
          // Some
          data.encode(1u8);
          type_ref.encode_value(value, data)?
        }
      }
      TypeMeta::OptionBool => data.encode(value.as_bool().ok()),
      TypeMeta::Vector(type_ref) => {
        if value.is::<Array>() {
          let values = value.cast::<Array>();
          // Encode vector length.
          data.encode(Compact::<u64>(values.len() as u64));
          for value in values.into_iter() {
            type_ref.encode_value(value, data)?
          }
        } else {
          Err(format!("Expected a vector, got {:?}", value.type_id()))?;
        }
      }
      TypeMeta::Slice(len, type_ref) => {
        if value.is::<Array>() {
          let values = value.cast::<Array>();
          if values.len() != *len {
            Err(format!(
              "Wrong slice length: Expected {} got {}",
              len,
              values.len()
            ))?;
          }
          for value in values.into_iter() {
            type_ref.encode_value(value, data)?
          }
          return Ok(());
        } else if type_ref.is_u8() {
          let type_id = value.type_id();
          // Handle fixed-length byte arrays: [u8; len]
          if type_id == TypeId::of::<SharedUser>() && *len == 32 {
            let user = value.cast::<SharedUser>();
            data.encode(user.public());
            return Ok(());
          } else if type_id == TypeId::of::<ImmutableString>() {
            let s = value.into_immutable_string()?;
            if s.len() == *len {
              // Write fixed-length string as bytes.
              data.write(s.as_bytes());
            } else if s.len() >= (*len * 2) {
              // Maybe Hex-encoded string.
              let bytes = if s.starts_with("0x") {
                hex::decode(&s.as_bytes()[2..]).map_err(|e| e.to_string())?
              } else {
                hex::decode(s.as_bytes()).map_err(|e| e.to_string())?
              };
              data.write(&bytes[..]);
            } else {
              // Failed to convert string to fixed-length byte array.
              return Err(format!(
                "Unhandled slice type: {:?}, from string='{}'",
                self, s
              ))?;
            }
            return Ok(());
          }
        }
        // Unhandled slice type.
        return Err(format!(
          "Unhandled slice type: {:?}, value={:?}",
          self, value
        ))?;
      }
      TypeMeta::String => {
        let s = value.into_immutable_string()?;
        data.encode(s.as_str());
      }

      TypeMeta::Tuple(types) => {
        if value.is::<Array>() {
          let values = value.cast::<Array>();
          if values.len() != types.len() {
            Err(format!(
              "Wrong Tuple length: Expected {} got {}",
              types.len(),
              values.len()
            ))?;
          }
          for (type_ref, value) in types.iter().zip(values.into_iter()) {
            type_ref.encode_value(value, data)?
          }
        } else {
          Err(format!("Expected a Tuple, got {:?}", value.type_id()))?;
        }
      }
      TypeMeta::Struct(fields) => {
        if value.is::<RMap>() {
          let map = value.cast::<RMap>();
          for (name, type_ref) in fields {
            let name: SmartString<LazyCompact> = name.into();
            if let Some(value) = map.get(&name) {
              type_ref.encode_value(value.clone(), data)?;
            } else {
              Err(format!("Missing field `{}` in Struct", name))?;
            }
          }
        } else {
          Err(format!("Expected a Struct, got {:?}", value.type_id()))?;
        }
      }
      TypeMeta::Enum(variants) => {
        if value.is::<RMap>() {
          let map = value.cast::<RMap>();
          let mut encoded = false;
          for (name, value) in map.into_iter() {
            if let Some(variant) = variants.get_by_name(name.as_str()) {
              if encoded {
                // Only allow encoding one Enum variant.
                Err(format!("Can't encode multiple Enum variants."))?;
              }
              encoded = true;
              // Encode enum variant idx.
              data.encode(variant.idx);
              if let Some(type_ref) = &variant.type_ref {
                type_ref.encode_value(value, data)?;
              }
            } else {
              Err(format!("Unknown Enum variant: {}.", name))?;
            }
          }
          // At least one Enum variant must be encoded.
          if !encoded {
            Err(format!("Enum is empty, must provide at least one variant."))?;
          }
        } else {
          Err(format!("Expected a Enum, got {:?}", value.type_id()))?;
        }
      }

      TypeMeta::Compact(type_ref) => {
        let old = data.is_compact();
        data.set_compact(true);
        let res = type_ref.encode_value(value, data);
        data.set_compact(old);
        res?
      }
      TypeMeta::Box(type_ref) | TypeMeta::NewType(_, type_ref) => {
        type_ref.encode_value(value, data)?
      }

      TypeMeta::CustomType(custom) => custom.encode_value(value, data)?,
      TypeMeta::Unresolved(type_def) => Err(format!("Unresolved type: {}", type_def))?,
      _ => Err(format!("Unhandled type: {:?}", self))?,
    }
    Ok(())
  }

  pub fn decode_value<I: Input>(&self, input: &mut I, is_compact: bool) -> Result<Dynamic, PError> {
    let val = match self {
      TypeMeta::Unit => Dynamic::UNIT,
      TypeMeta::Integer(len, signed) => match (len, signed) {
        (_, false) if is_compact => {
          let val = Compact::<u128>::decode(input)?.0;
          match i64::try_from(val) {
            Ok(val) => Dynamic::from_int(val),
            Err(_) => {
              let dec = Decimal::from(val);
              Dynamic::from_decimal(dec)
            }
          }
        }
        (1, true) => Dynamic::from_int(i8::decode(input)? as i64),
        (1, false) => Dynamic::from_int(u8::decode(input)? as i64),
        (2, true) => Dynamic::from_int(i16::decode(input)? as i64),
        (2, false) => Dynamic::from_int(u16::decode(input)? as i64),
        (4, true) => Dynamic::from_int(i32::decode(input)? as i64),
        (4, false) => Dynamic::from_int(u32::decode(input)? as i64),
        (8, true) => Dynamic::from_int(i64::decode(input)?),
        (8, false) => {
          let val = u64::decode(input)?;
          match i64::try_from(val) {
            Ok(val) => Dynamic::from_int(val),
            Err(_) => {
              let dec = Decimal::from(val);
              Dynamic::from_decimal(dec)
            }
          }
        }
        (16, true) => {
          let val = i128::decode(input)?;
          let dec = Decimal::from(val);
          Dynamic::from_decimal(dec)
        }
        (16, false) => {
          let val = u128::decode(input)?;
          let dec = Decimal::from(val);
          Dynamic::from_decimal(dec)
        }
        _ => Err("Unsupported integer type")?,
      },
      TypeMeta::Bool => {
        let val = input.read_byte()?;
        Dynamic::from_bool(val == 1)
      }
      TypeMeta::Option(type_ref) => {
        let val = input.read_byte()?;
        if val == 1 {
          type_ref.decode_value(input, false)?
        } else {
          Dynamic::UNIT
        }
      }
      TypeMeta::OptionBool => {
        let val = input.read_byte()?;
        if val == 1 {
          Dynamic::from_bool(true)
        } else if val == 2 {
          Dynamic::from_bool(false)
        } else {
          Dynamic::UNIT
        }
      }
      TypeMeta::Result(ok_ref, err_ref) => {
        let val = input.read_byte()?;
        let mut map = RMap::new();
        if val == 0 {
          map.insert("Ok".into(), ok_ref.decode_value(input, false)?);
        } else {
          map.insert("Err".into(), err_ref.decode_value(input, false)?);
        }
        Dynamic::from(map)
      }
      TypeMeta::Vector(type_ref) => {
        let len = Compact::<u64>::decode(input)?.0;
        let mut vec = Vec::new();
        for _ in 0..len {
          vec.push(type_ref.decode_value(input, false)?);
        }
        Dynamic::from(vec)
      }
      TypeMeta::Slice(len, type_ref) => {
        let mut vec = Vec::with_capacity(*len as usize);
        for _ in 0..*len {
          vec.push(type_ref.decode_value(input, false)?);
        }
        Dynamic::from(vec)
      }
      TypeMeta::String => {
        let val = String::decode(input)?;
        Dynamic::from(val)
      }

      TypeMeta::Tuple(types) => {
        let mut vec = Vec::with_capacity(types.len());
        for type_ref in types {
          vec.push(type_ref.decode_value(input, false)?);
        }
        Dynamic::from(vec)
      }
      TypeMeta::Struct(fields) => {
        let mut map = RMap::new();
        for (name, type_ref) in fields {
          log::debug!("decode Struct field: {}", name);
          map.insert(name.into(), type_ref.decode_value(input, false)?);
        }
        Dynamic::from(map)
      }
      TypeMeta::Enum(variants) => {
        let val = input.read_byte()?;
        match variants.get_by_idx(val) {
          Some(variant) => {
            let name = &variant.name;
            log::debug!("decode Enum variant: {}", name);
            let mut map = RMap::new();
            if let Some(type_ref) = &variant.type_ref {
              map.insert(name.into(), type_ref.decode_value(input, false)?);
            } else {
              map.insert(name.into(), Dynamic::UNIT);
            }
            Dynamic::from(map)
          }
          None => {
            log::debug!(
              "invalid variant: {}, remaining: {:?}, variants={:?}",
              val,
              input.remaining_len()?,
              variants
            );
            Err("Error decoding Enum, invalid variant.")?
          }
        }
      }

      TypeMeta::Compact(type_ref) => type_ref.decode_value(input, true)?,
      TypeMeta::Box(type_ref) | TypeMeta::NewType(_, type_ref) => {
        type_ref.decode_value(input, false)?
      }

      TypeMeta::CustomType(custom) => custom.decode_value(input, false)?,
      TypeMeta::Unresolved(type_def) => {
        log::error!("Unresolved type: {}", type_def);
        Err("Unresolved type")?
      }
    };
    Ok(val)
  }
}

#[derive(Clone)]
pub struct Types {
  types: IndexMap<String, TypeRef>,
}

impl Types {
  pub fn new() -> Self {
    Self {
      types: IndexMap::new(),
    }
  }

  pub fn load_schema(&mut self, filename: &str) -> Result<(), Box<EvalAltResult>> {
    let file = File::open(filename).map_err(|e| e.to_string())?;

    let schema: serde_json::Value =
      serde_json::from_reader(BufReader::new(file)).map_err(|e| e.to_string())?;

    let schema = schema
      .as_object()
      .expect("Invalid schema, expected object.");

    let types = match schema.get("types") {
      Some(val) => val.as_object().unwrap_or(schema),
      _ => schema,
    };
    self.parse_schema_types(types)?;

    Ok(())
  }

  fn parse_schema_types(&mut self, types: &Map<String, Value>) -> Result<(), Box<EvalAltResult>> {
    for (name, val) in types.iter() {
      match val {
        Value::String(val) => {
          self.parse_named_type(name, val)?;
        }
        Value::Object(map) => {
          if let Some(variants) = map.get("_enum") {
            self.parse_enum(name, variants)?;
          } else {
            self.parse_struct(name, map)?;
          }
        }
        _ => {
          eprintln!("UNHANDLED JSON VALUE: {} => {:?}", name, val);
        }
      }
    }
    Ok(())
  }

  fn parse_enum(&mut self, name: &str, variants: &Value) -> Result<(), Box<EvalAltResult>> {
    match variants {
      Value::Array(arr) => {
        let variants = arr
          .iter()
          .try_fold(EnumVariants::new(), |mut variants, val| {
            match val.as_str() {
              Some(name) => {
                variants.insert(name, None);
                Ok(variants)
              }
              None => Err(format!(
                "Expected json string for enum {}: got {:?}",
                name, val
              )),
            }
          })?;
        self.insert_meta(name, TypeMeta::Enum(variants));
      }
      Value::Object(obj) => {
        let variants = obj.iter().try_fold(
          EnumVariants::new(),
          |mut variants, (var_name, val)| -> Result<_, Box<EvalAltResult>> {
            match val.as_str() {
              Some("") => {
                variants.insert(var_name, None);
                Ok(variants)
              }
              Some(var_def) => {
                let type_meta = self.parse_type(var_def)?;
                variants.insert(var_name, Some(type_meta));
                Ok(variants)
              }
              None => Err(format!("Expected json string for enum {}: got {:?}", name, val).into()),
            }
          },
        )?;
        self.insert_meta(name, TypeMeta::Enum(variants));
      }
      _ => {
        return Err(format!("Invalid json for `_enum`: {:?}", variants).into());
      }
    }
    Ok(())
  }

  fn parse_struct(
    &mut self,
    name: &str,
    def: &Map<String, Value>,
  ) -> Result<(), Box<EvalAltResult>> {
    let fields = def.iter().try_fold(
      IndexMap::new(),
      |mut map, (field_name, val)| -> Result<_, Box<EvalAltResult>> {
        match val.as_str() {
          Some(field_def) => {
            let type_meta = self.parse_type(field_def)?;
            map.insert(field_name.to_string(), type_meta);
            Ok(map)
          }
          None => Err(
            format!(
              "Expected json string for struct {} field {}: got {:?}",
              name, field_name, val
            )
            .into(),
          ),
        }
      },
    )?;
    self.insert_meta(name, TypeMeta::Struct(fields));
    Ok(())
  }

  pub fn parse_named_type(&mut self, name: &str, def: &str) -> Result<TypeRef, Box<EvalAltResult>> {
    let type_ref = self.parse_type(def)?;

    Ok(self.insert_meta(name, TypeMeta::NewType(name.into(), type_ref)))
  }

  pub fn parse_type(&mut self, name: &str) -> Result<TypeRef, Box<EvalAltResult>> {
    let name = name
      .trim()
      .replace("\r", "")
      .replace("\n", "")
      .replace("T::", "");
    // Try to resolve the type.
    let type_ref = self.resolve(&name);
    let mut type_meta = type_ref.0.write().unwrap();

    // Check if type is unresolved.
    match &*type_meta {
      TypeMeta::Unresolved(def) => {
        // Try parsing it.
        let new_meta = self.parse(def)?;
        *type_meta = new_meta;
      }
      _ => (),
    }
    Ok(type_ref.clone())
  }

  fn parse(&mut self, def: &str) -> Result<TypeMeta, Box<EvalAltResult>> {
    match def.chars().last() {
      Some('>') => {
        // Handle: Vec<T>, Option<T>, Compact<T>
        let (wrap, ty) = def
          .strip_suffix('>')
          .and_then(|s| s.split_once('<'))
          .map(|(wrap, ty)| (wrap.trim(), ty.trim()))
          .ok_or_else(|| format!("Failed to parse Vec/Option/Compact: {}", def))?;
        match wrap {
          "Vec" => {
            let wrap_ref = self.parse_type(ty)?;
            Ok(TypeMeta::Vector(wrap_ref))
          }
          "Option" => {
            let wrap_ref = self.parse_type(ty)?;
            Ok(TypeMeta::Option(wrap_ref))
          }
          "Compact" => {
            let wrap_ref = self.parse_type(ty)?;
            Ok(TypeMeta::Compact(wrap_ref))
          }
          "Box" => {
            let wrap_ref = self.parse_type(ty)?;
            Ok(TypeMeta::Box(wrap_ref))
          }
          "Result" => {
            let (ok_ref, err_ref) = match ty.split_once(',') {
              Some((ok_ty, err_ty)) => {
                let ok_ref = self.parse_type(ok_ty)?;
                let err_ref = self.parse_type(err_ty)?;
                (ok_ref, err_ref)
              }
              None => {
                let ok_ref = self.parse_type(ty)?;
                let err_ref = self.parse_type("Error")?;
                (ok_ref, err_ref)
              }
            };
            Ok(TypeMeta::Result(ok_ref, err_ref))
          }
          "PhantomData" | "sp_std::marker::PhantomData" => Ok(TypeMeta::Unit),
          generic => {
            // Some generic type.
            if self.types.contains_key(generic) {
              Ok(TypeMeta::NewType(generic.into(), self.resolve(generic)))
            } else {
              Ok(TypeMeta::Unresolved(def.into()))
            }
          }
        }
      }
      Some(')') => {
        let defs = def
          .trim_matches(|c| c == '(' || c == ')')
          .split_terminator(',')
          .filter_map(|s| {
            let s = s.trim();
            if s != "" {
              Some(s)
            } else {
              None
            }
          })
          .try_fold(
            Vec::new(),
            |mut vec, val| -> Result<_, Box<EvalAltResult>> {
              let type_ref = self.parse_type(val)?;
              vec.push(type_ref);
              Ok(vec)
            },
          )?;
        // Handle tuples.
        Ok(TypeMeta::Tuple(defs))
      }
      Some(']') => {
        let (slice_ty, slice_len) = def
          .trim_matches(|c| c == '[' || c == ']')
          .split_once(';')
          .and_then(|(ty, len)| {
            // parse slice length.
            len.trim().parse::<usize>().ok().map(|l| (ty.trim(), l))
          })
          .ok_or_else(|| format!("Failed to parse slice: {}", def))?;
        // Handle slices.
        let slice_ref = self.parse_type(slice_ty)?;
        Ok(TypeMeta::Slice(slice_len, slice_ref))
      }
      _ => Ok(TypeMeta::Unresolved(def.into())),
    }
  }

  pub fn resolve(&mut self, name: &str) -> TypeRef {
    let entry = self.types.entry(name.into());
    let type_ref = entry.or_insert_with(|| TypeRef::from(TypeMeta::Unresolved(name.into())));
    type_ref.clone()
  }

  pub fn insert_meta(&mut self, name: &str, type_def: TypeMeta) -> TypeRef {
    self.insert(name, TypeRef::from(type_def))
  }

  pub fn insert(&mut self, name: &str, type_ref: TypeRef) -> TypeRef {
    use indexmap::map::Entry;
    let entry = self.types.entry(name.into());
    match entry {
      Entry::Occupied(entry) => {
        let old_ref = entry.get();
        let mut old_meta = old_ref.0.write().unwrap();
        // Already exists.  Check that it is a `TypeMeta::Unresolved`.
        match &*old_meta {
          TypeMeta::Unresolved(_) => {
            *old_meta = TypeMeta::NewType(name.into(), type_ref.clone());
          }
          _ => {
            eprintln!("REDEFINE TYPE: {}", name);
          }
        }
        old_ref.clone()
      }
      Entry::Vacant(entry) => {
        entry.insert(type_ref.clone());
        type_ref
      }
    }
  }

  /// Dump types.
  pub fn dump_types(&self) {
    for (idx, (key, type_ref)) in self.types.iter().enumerate() {
      eprintln!("Type[{}]: {} => {:#?}", idx, key, type_ref);
    }
  }

  /// Dump unresolved types.
  pub fn dump_unresolved(&self) {
    for (key, type_ref) in self.types.iter() {
      let meta = type_ref.0.read().unwrap();
      match &*meta {
        TypeMeta::Unresolved(def) => {
          eprintln!("--------- Unresolved: {} => {}", key, def);
        }
        _ => (),
      }
    }
  }

  pub fn custom_encode<F>(
    &mut self,
    name: &str,
    type_id: TypeId,
    func: F,
  ) -> Result<(), Box<EvalAltResult>>
  where
    F: 'static + Fn(Dynamic, &mut EncodedArgs) -> Result<(), Box<EvalAltResult>>,
  {
    let func = WrapEncodeFn(Arc::new(func));
    let type_ref = self.parse_type(name)?;
    type_ref.custom_encode(type_id, func);
    Ok(())
  }

  pub fn custom_decode<F>(&mut self, name: &str, func: F) -> Result<(), Box<EvalAltResult>>
  where
    F: 'static + Fn(BoxedInput) -> Result<Dynamic, PError>,
  {
    let func = WrapDecodeFn(Arc::new(func));
    let type_ref = self.parse_type(name)?;
    type_ref.custom_decode(func);
    Ok(())
  }
}

#[derive(Clone)]
pub struct TypeLookup {
  types: Arc<RwLock<Types>>,
}

impl TypeLookup {
  pub fn new() -> Self {
    Self {
      types: Arc::new(RwLock::new(Types::new())),
    }
  }

  pub fn from_types(types: Types) -> Self {
    Self {
      types: Arc::new(RwLock::new(types)),
    }
  }

  pub fn parse_named_type(&self, name: &str, def: &str) -> Result<TypeRef, Box<EvalAltResult>> {
    let mut t = self.types.write().unwrap();
    t.parse_named_type(name, def)
  }

  pub fn parse_type(&self, def: &str) -> Result<TypeRef, Box<EvalAltResult>> {
    let mut t = self.types.write().unwrap();
    t.parse_type(def)
  }

  pub fn resolve(&self, name: &str) -> TypeRef {
    let mut t = self.types.write().unwrap();
    t.resolve(name)
  }

  pub fn insert_meta(&self, name: &str, type_meta: TypeMeta) -> TypeRef {
    let mut t = self.types.write().unwrap();
    t.insert_meta(name, type_meta)
  }

  pub fn insert(&self, name: &str, type_def: TypeRef) -> TypeRef {
    let mut t = self.types.write().unwrap();
    t.insert(name, type_def)
  }

  pub fn dump_types(&mut self) {
    self.types.read().unwrap().dump_types();
  }

  pub fn dump_unresolved(&mut self) {
    self.types.read().unwrap().dump_unresolved();
  }

  pub fn custom_encode<F>(
    &self,
    name: &str,
    type_id: TypeId,
    func: F,
  ) -> Result<(), Box<EvalAltResult>>
  where
    F: 'static + Fn(Dynamic, &mut EncodedArgs) -> Result<(), Box<EvalAltResult>>,
  {
    let mut t = self.types.write().unwrap();
    t.custom_encode(name, type_id, func)
  }

  pub fn custom_decode<F>(&self, name: &str, func: F) -> Result<(), Box<EvalAltResult>>
  where
    F: 'static + Fn(BoxedInput) -> Result<Dynamic, PError>,
  {
    let mut t = self.types.write().unwrap();
    t.custom_decode(name, func)
  }
}

pub fn init_engine(
  engine: &mut Engine,
  opts: &EngineOptions,
) -> Result<TypeLookup, Box<EvalAltResult>> {
  engine
    .register_type_with_name::<TypeLookup>("TypeLookup")
    .register_fn("dump_types", TypeLookup::dump_types)
    .register_fn("dump_unresolved", TypeLookup::dump_unresolved)
    .register_result_fn(
      "parse_named_type",
      |lookup: &mut TypeLookup, name: &str, def: &str| {
        TypeLookup::parse_named_type(lookup, name, def)
      },
    )
    .register_result_fn("parse_type", |lookup: &mut TypeLookup, def: &str| {
      TypeLookup::parse_type(lookup, def)
    })
    .register_fn("resolve", |lookup: &mut TypeLookup, name: &str| {
      TypeLookup::resolve(lookup, name)
    })
    .register_type_with_name::<Types>("Types")
    .register_type_with_name::<TypeMeta>("TypeMeta")
    .register_fn("to_string", TypeMeta::to_string)
    .register_type_with_name::<TypeRef>("TypeRef")
    .register_fn("to_string", TypeRef::to_string)
    .register_result_fn("encode", TypeRef::encode_mut)
    .register_result_fn("decode", TypeRef::decode_mut)
    .register_type_with_name::<Era>("Era")
    .register_fn("era_immortal", || Era::immortal())
    .register_fn("era_mortal", |period: i64, current: i64| {
      Era::mortal(period as u64, current as u64)
    })
    .register_fn("encode", |era: &mut Era| era.encode())
    .register_fn("to_string", |era: &mut Era| format!("{:?}", era));
  let mut types = Types::new();

  // Primitive types.
  types.insert_meta("u8", TypeMeta::Integer(1, false));
  types.insert_meta("u16", TypeMeta::Integer(2, false));
  types.insert_meta("u32", TypeMeta::Integer(4, false));
  types.insert_meta("u64", TypeMeta::Integer(8, false));
  types.insert_meta("u128", TypeMeta::Integer(16, false));
  types.insert_meta("i8", TypeMeta::Integer(1, true));
  types.insert_meta("i16", TypeMeta::Integer(2, true));
  types.insert_meta("i32", TypeMeta::Integer(4, true));
  types.insert_meta("i64", TypeMeta::Integer(8, true));
  types.insert_meta("i128", TypeMeta::Integer(16, true));
  types.insert_meta("bool", TypeMeta::Bool);
  types.insert_meta("Text", TypeMeta::String);
  types.insert_meta("Option<bool>", TypeMeta::OptionBool);

  // Load standard substrate types.
  types.load_schema(&opts.substrate_types)?;
  // Load custom chain types.
  types.load_schema(&opts.custom_types)?;

  // Custom encodings.
  types.custom_encode("Era", TypeId::of::<Era>(), |value, data| {
    let era = value.cast::<Era>();
    data.encode(era);
    Ok(())
  })?;
  types.custom_decode("Era", |mut input| {
    let era = Era::decode(&mut input)?;
    Ok(Dynamic::from(era))
  })?;

  types.custom_encode("AccountId", TypeId::of::<SharedUser>(), |value, data| {
    let user = value.cast::<SharedUser>();
    data.encode(user.public());
    Ok(())
  })?;
  types.custom_encode("AccountId", TypeId::of::<AccountId>(), |value, data| {
    data.encode(value.cast::<AccountId>());
    Ok(())
  })?;
  types.custom_decode("AccountId", |mut input| {
    Ok(Dynamic::from(AccountId::decode(&mut input)?))
  })?;

  types.custom_encode("MultiAddress", TypeId::of::<SharedUser>(), |value, data| {
    let user = value.cast::<SharedUser>();
    // Encode variant idx.
    data.encode(0u8); // MultiAddress::Id
    data.encode(user.public());
    Ok(())
  })?;

  let lookup = TypeLookup::from_types(types);
  Ok(lookup)
}
