use std::sync::{Arc, RwLock};
use std::fs::File;
use std::io::BufReader;
use std::any::TypeId;
use std::collections::HashMap;

use serde_json::{
  Value, Map,
};
use parity_scale_codec::Compact;

use rust_decimal::{Decimal, prelude::ToPrimitive};

use smartstring::{LazyCompact, SmartString};
use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, Map as RMap, Scope};

use indexmap::map::IndexMap;

use super::users::User;
use super::metadata::EncodedArgs;

#[derive(Clone)]
pub struct TypeRef(Arc<RwLock<TypeMeta>>);

impl TypeRef {
  fn to_string(&mut self) -> String {
    format!("TypeRef: {:?}", self.0.read().unwrap())
  }

  pub fn custom_encode<F>(&self, type_id: TypeId, func: F)
    where F: 'static + Fn(Dynamic, &mut EncodedArgs) -> Result<(), Box<EvalAltResult>>
  {
    self.0.write().unwrap().custom_encode(type_id, func)
  }

  pub fn encode_value(&self, param: Dynamic, data: &mut EncodedArgs) -> Result<(), Box<EvalAltResult>> {
    self.0.read().unwrap().encode_value(param, data)
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

pub type EncodeFn = dyn Fn(Dynamic, &mut EncodedArgs) -> Result<(), Box<EvalAltResult>>;
#[derive(Clone)]
pub struct WrapEncodeFn(Arc<EncodeFn>);

impl WrapEncodeFn {
  pub fn encode_value(&self, value: Dynamic, data: &mut EncodedArgs) -> Result<(), Box<EvalAltResult>> {
    self.0(value, data)
  }
}

impl std::fmt::Debug for WrapEncodeFn {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("EncodeFunction")
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
  Enum(IndexMap<String, Option<TypeRef>>),

  Compact(TypeRef),
  NewType(String, TypeRef),

  Unresolved(String),

  CustomEncode(HashMap<TypeId, WrapEncodeFn>, Box<TypeMeta>),
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

  fn make_custom_encode(&mut self) {
    match self {
      TypeMeta::CustomEncode(_, _) => {
        // already wrapped.
        return;
      }
      _ => (),
    }
    let meta = self.clone();
    *self = TypeMeta::CustomEncode(Default::default(), Box::new(meta));
  }

  pub fn custom_encode<F>(&mut self, type_id: TypeId, func: F)
    where F: 'static + Fn(Dynamic, &mut EncodedArgs) -> Result<(), Box<EvalAltResult>>
  {
    self.make_custom_encode();
    match self {
      TypeMeta::CustomEncode(type_map, _) => {
        let func = WrapEncodeFn(Arc::new(func));
        type_map.insert(type_id, func);
      }
      _ => unreachable!(),
    }
  }

  pub fn encode_value(&self, value: Dynamic, data: &mut EncodedArgs) -> Result<(), Box<EvalAltResult>> {
    match self {
      TypeMeta::Unit => (),
      TypeMeta::Integer(len, signed) => {
        if let Some(num) = value.as_int().ok() {
          match (len, signed) {
            (_, false) if data.is_compact() => {
              data.encode(Compact::<u128>(num as u128))
            }
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
              let num = dec.to_u128()
                .ok_or_else(|| format!("Expected unsigned integer"))?;
              data.encode(Compact::<u128>(num))
            }
            (1, true) => data.encode(dec.to_i8()
              .ok_or_else(|| format!("Integer too large for `i8`"))?),
            (1, false) => data.encode(dec.to_u8()
              .ok_or_else(|| format!("Integer too large for `u8` or negative."))?),
            (2, true) => data.encode(dec.to_i16()
              .ok_or_else(|| format!("Integer too large for `i16`"))?),
            (2, false) => data.encode(dec.to_u16()
              .ok_or_else(|| format!("Integer too large for `u16` or negative."))?),
            (4, true) => data.encode(dec.to_i32()
              .ok_or_else(|| format!("Integer too large for `i32`"))?),
            (4, false) => data.encode(dec.to_u32()
              .ok_or_else(|| format!("Integer too large for `u32` or negative."))?),
            (8, true) => data.encode(dec.to_i64()
              .ok_or_else(|| format!("Integer too large for `i64`"))?),
            (8, false) => data.encode(dec.to_u64()
              .ok_or_else(|| format!("Integer too large for `u64` or negative."))?),
            (16, signed) => {
              // TODO: Add support for other decimal scales.
              dec *= Decimal::from(1000_000u64);
              if *signed {
                data.encode(dec.to_i128()
                  .ok_or_else(|| format!("Integer too large for `u128`."))?)
              } else {
                data.encode(dec.to_u128()
                  .ok_or_else(|| format!("Expected a non-negative integer/decimal."))?)
              }
            }
            _ => Err(format!("Unsupported integer type: {:?}", self))?,
          }
        } else {
          Err(format!("Expected an integer or decimal value, got {:?}", value))?;
        }
      },
      TypeMeta::Bool => {
        data.encode(value.as_bool()?)
      },
      TypeMeta::Option(type_ref) => {
        if value.is::<()>() {
          // None
          data.encode(0u8);
        } else {
          // Some
          data.encode(1u8);
          type_ref.encode_value(value, data)?
        }
      },
      TypeMeta::OptionBool => {
        data.encode(value.as_bool().ok())
      },
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
      },
      TypeMeta::Slice(len, type_ref) => {
        if value.is::<Array>() {
          let values = value.cast::<Array>();
          if values.len() != *len {
            Err(format!("Wrong slice length: Expected {} got {}", len, values.len()))?;
          }
          for value in values.into_iter() {
            type_ref.encode_value(value, data)?
          }
        } else {
          if *len == 32 && value.is::<User>() {
            let user = value.cast::<User>();
            data.encode(user.public());
          } else {
            Err(format!("Unhandled slice type: {:?}, value={:?}", self, value))?;
          }
        }
      },
      TypeMeta::String => {
        let s = value.into_immutable_string()?;
        data.encode(s.as_str());
      },

      TypeMeta::Tuple(types) => {
        if value.is::<Array>() {
          let values = value.cast::<Array>();
          if values.len() != types.len() {
            Err(format!("Wrong Tuple length: Expected {} got {}", types.len(), values.len()))?;
          }
          for (type_ref, value) in types.iter().zip(values.into_iter()) {
            type_ref.encode_value(value, data)?
          }
        } else {
          Err(format!("Expected a Tuple, got {:?}", value.type_id()))?;
        }
      },
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
      },
      TypeMeta::Enum(variants) => {
        if value.is::<RMap>() {
          let map = value.cast::<RMap>();
          let mut encoded = false;
          for (name, value) in map.into_iter() {
            if let Some((idx, _, type_ref)) = variants.get_full(name.as_str()) {
              if encoded {
                // Only allow encoding one Enum variant.
                Err(format!("Can't encode multiple Enum variants."))?;
              }
              encoded = true;
              // Encode enum variant idx.
              data.encode(idx as u8);
              if let Some(type_ref) = type_ref {
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
      },

      TypeMeta::Compact(type_ref) => {
        let old = data.is_compact();
        data.set_compact(true);
        let res = type_ref.encode_value(value, data);
        data.set_compact(old);
        res?
      },
      TypeMeta::Box(type_ref) | TypeMeta::NewType(_, type_ref) => {
        type_ref.encode_value(value, data)?
      },

      TypeMeta::CustomEncode(type_map, type_meta) => {
        let type_id = value.type_id();
        if let Some(func) = type_map.get(&type_id) {
          func.encode_value(value, data)?
        } else {
          type_meta.encode_value(value, data)?
        }
      },
      TypeMeta::Unresolved(type_def) => {
        Err(format!("Unresolved type: {}", type_def))?
      },
      _ => {
        Err(format!("Unhandled type: {:?}", self))?
      },
    }
    Ok(())
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

    let schema: serde_json::Value = serde_json::from_reader(BufReader::new(file)).map_err(|e| e.to_string())?;

    let schema = schema.as_object()
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
        let variants = arr.iter().try_fold(IndexMap::new(), |mut map, val| {
          match val.as_str() {
            Some(name) => {
              map.insert(name.to_string(), None);
              Ok(map)
            }
            None => Err(format!("Expected json string for enum {}: got {:?}", name, val)),
          }
        })?;
        self.insert_meta(name, TypeMeta::Enum(variants));
      }
      Value::Object(obj) => {
        let variants = obj.iter().try_fold(IndexMap::new(), |mut map, (var_name, val)| -> Result<_, Box<EvalAltResult>> {
          match val.as_str() {
            Some("") => {
              map.insert(var_name.to_string(), None);
              Ok(map)
            }
            Some(var_def) => {
              let type_meta = self.parse_type(var_def)?;
              map.insert(var_name.to_string(), Some(type_meta));
              Ok(map)
            }
            None => Err(format!("Expected json string for enum {}: got {:?}", name, val).into()),
          }
        })?;
        self.insert_meta(name, TypeMeta::Enum(variants));
      }
      _ => {
        return Err(format!("Invalid json for `_enum`: {:?}", variants).into());
      }
    }
    Ok(())
  }

  fn parse_struct(&mut self, name: &str, def: &Map<String, Value>) -> Result<(), Box<EvalAltResult>> {
    let fields = def.iter().try_fold(IndexMap::new(), |mut map, (field_name, val)| -> Result<_, Box<EvalAltResult>> {
      match val.as_str() {
        Some(field_def) => {
          let type_meta = self.parse_type(field_def)?;
          map.insert(field_name.to_string(), type_meta);
          Ok(map)
        }
        None => Err(format!("Expected json string for struct {} field {}: got {:?}", name, field_name, val).into()),
      }
    })?;
    self.insert_meta(name, TypeMeta::Struct(fields));
    Ok(())
  }

  pub fn parse_named_type(&mut self, name: &str, def: &str) -> Result<TypeRef, Box<EvalAltResult>> {
    let type_ref = self.parse_type(def)?;

    self.insert(name, type_ref.clone());
    Ok(type_ref)
  }

  pub fn parse_type(&mut self, name: &str) -> Result<TypeRef, Box<EvalAltResult>> {
    let name = name.trim()
      .replace("\r", "")
      .replace("\n", "")
      .replace("T::", "");
    // Try to resolve the type.
    let type_ref = self.resolve(&name)?;
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
          "PhantomData" | "sp_std::marker::PhantomData" => {
            Ok(TypeMeta::Unit)
          }
          generic => {
            // Some generic type.
            if self.types.contains_key(generic) {
              Ok(TypeMeta::NewType(generic.into(), self.resolve(generic)?))
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
          .try_fold(Vec::new(), |mut vec, val| -> Result<_, Box<EvalAltResult>> {
            let type_ref = self.parse_type(val)?;
            vec.push(type_ref);
            Ok(vec)
          })?;
        // Handle tuples.
        Ok(TypeMeta::Tuple(defs))
      }
      Some(']') => {
        let (slice_ty, slice_len) = def
          .trim_matches(|c| c == '[' || c == ']')
          .split_once(';')
          .and_then(|(ty, len)| {
            // parse slice length.
            len.trim().parse::<usize>().ok()
              .map(|l| (ty.trim(), l))
          }).ok_or_else(|| format!("Failed to parse slice: {}", def))?;
        // Handle slices.
        let slice_ref = self.parse_type(slice_ty)?;
        Ok(TypeMeta::Slice(slice_len, slice_ref))
      }
      _ => {
        Ok(TypeMeta::Unresolved(def.into()))
      }
    }
  }

  pub fn resolve(&mut self, name: &str) -> Result<TypeRef, Box<EvalAltResult>> {
    let entry = self.types.entry(name.into());
    let type_ref = entry.or_insert_with(|| TypeRef::from(TypeMeta::Unresolved(name.into())));
    Ok(type_ref.clone())
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
          },
          _ => {
            eprintln!("REDEFINE TYPE: {}", name);
          }
        }
        old_ref.clone()
      },
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

  pub fn custom_encode_type<F>(&mut self, name: &str, type_id: TypeId, func: F) -> Result<(), Box<EvalAltResult>>
    where F: 'static + Fn(Dynamic, &mut EncodedArgs) -> Result<(), Box<EvalAltResult>>
  {
    let type_ref = self.parse_type(name)?;
    type_ref.custom_encode(type_id, func);
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

  pub fn resolve(&self, name: &str) -> Result<TypeRef, Box<EvalAltResult>> {
    let mut t = self.types.write().unwrap();
    Ok(t.resolve(name)?)
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

  pub fn custom_encode_type<F>(&self, name: &str, type_id: TypeId, func: F) -> Result<(), Box<EvalAltResult>>
    where F: 'static + Fn(Dynamic, &mut EncodedArgs) -> Result<(), Box<EvalAltResult>>
  {
    let mut t = self.types.write().unwrap();
    t.custom_encode_type(name, type_id, func)
  }
}

pub fn init_engine(engine: &mut Engine) {
  engine
    .register_type_with_name::<TypeLookup>("TypeLookup")
    .register_fn("dump_types", TypeLookup::dump_types)
    .register_fn("dump_unresolved", TypeLookup::dump_unresolved)
    .register_result_fn("parse_named_type", |lookup: &mut TypeLookup, name: &str, def: &str| TypeLookup::parse_named_type(lookup, name, def))
    .register_result_fn("parse_type", |lookup: &mut TypeLookup, def: &str| TypeLookup::parse_type(lookup, def))
    .register_result_fn("resolve", |lookup: &mut TypeLookup, name: &str| TypeLookup::resolve(lookup, name))
    .register_type_with_name::<Types>("Types")
    .register_type_with_name::<TypeMeta>("TypeMeta")
    .register_fn("to_string", TypeMeta::to_string)
    .register_type_with_name::<TypeRef>("TypeRef")
    .register_fn("to_string", TypeRef::to_string)
    ;
}

pub fn init_scope(schema: &str, scope: &mut Scope<'_>) -> Result<TypeLookup, Box<EvalAltResult>> {
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
  types.load_schema("init_types.json")?;

  types.load_schema(schema)?;

  // Custom encodings.
  types.custom_encode_type("AccountId", TypeId::of::<User>(), |value, data| {
    let user = value.cast::<User>();
    data.encode(user.public());
    Ok(())
  })?;
  types.custom_encode_type("MultiAddress", TypeId::of::<User>(), |value, data| {
    let user = value.cast::<User>();
    // Encode variant idx.
    data.encode(0u8); // MultiAddress::Id
    data.encode(user.public());
    Ok(())
  })?;
  types.custom_encode_type("Ticker", TypeId::of::<ImmutableString>(), |value, data| {
    let value = value.cast::<ImmutableString>();
    if value.len() == 12 {
      data.encode(value.as_str());
    } else {
      let mut ticker = [0u8; 12];
      for (idx, b) in value.as_str().as_bytes().iter().take(12).enumerate() {
        ticker[idx] = *b;
      }
      data.encode(&ticker);
    }
    Ok(())
  })?;

  let lookup = TypeLookup::from_types(types);
  scope.push_constant("Types", lookup.clone());
  Ok(lookup)
}
