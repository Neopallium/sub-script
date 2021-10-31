use std::sync::{Arc, RwLock};
use std::fs::File;
use std::io::BufReader;

use serde_json::{
  Value, Map,
};

use rhai::{Engine, EvalAltResult, Scope};

use indexmap::map::IndexMap;

#[derive(Clone)]
pub struct TypeRef(Arc<RwLock<TypeMeta>>);

impl TypeRef {
  fn to_string(&mut self) -> String {
    format!("TypeRef: {:?}", self.0.read().unwrap())
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
    meta.fmt(f)
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
  NewType(TypeRef),

  Unresolved(String),
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
    let name = name.trim().replace("T::", "");
    // Try to resolve the type.
    let type_ref = self.resolve(&name)?;
    let mut type_meta = type_ref.0.write().unwrap();

    // Check if type is unresolved.
    match &*type_meta {
      TypeMeta::Unresolved(def) => {
        // Try parsing it.
        let new_meta = self.parse(def)?;
        *type_meta = new_meta;
        /*
        let meta = TypeRef::from(self.parse(def)?);
        if let Some(val) = self.types.get_mut(&name) {
          *val = meta.clone();
        }
        Ok(meta)
        */
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
            let wrap_meta = self.parse_type(ty)?;
            Ok(TypeMeta::Vector(wrap_meta))
          }
          "Option" => {
            let wrap_meta = self.parse_type(ty)?;
            Ok(TypeMeta::Option(wrap_meta))
          }
          "Compact" => {
            let wrap_meta = self.parse_type(ty)?;
            Ok(TypeMeta::Compact(wrap_meta))
          }
          "Box" => {
            let wrap_meta = self.parse_type(ty)?;
            Ok(TypeMeta::Box(wrap_meta))
          }
          "Result" => {
            let (ok_meta, err_meta) = match ty.split_once(',') {
              Some((ok_ty, err_ty)) => {
                let ok_meta = self.parse_type(ok_ty)?;
                let err_meta = self.parse_type(err_ty)?;
                (ok_meta, err_meta)
              }
              None => {
                let ok_meta = self.parse_type(ty)?;
                let err_meta = self.parse_type("Error")?;
                (ok_meta, err_meta)
              }
            };
            Ok(TypeMeta::Result(ok_meta, err_meta))
          }
          generic => {
            // Some generic type.
            if self.types.contains_key(generic) {
              Ok(TypeMeta::NewType(self.resolve(generic)?))
            } else {
              Ok(TypeMeta::NewType(self.resolve(def)?))
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
            let type_meta = self.parse_type(val)?;
            vec.push(type_meta);
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
        let slice_meta = self.parse_type(slice_ty)?;
        Ok(TypeMeta::Slice(slice_len, slice_meta))
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

  pub fn insert(&mut self, name: &str, type_def: TypeRef) -> TypeRef {
    use indexmap::map::Entry;
    let entry = self.types.entry(name.into());
    match entry {
      Entry::Occupied(entry) => {
        let new_meta = type_def.0.read().unwrap().clone();

        let old_ref = entry.get();
        let mut old_meta = old_ref.0.write().unwrap();
        // Already exists.  Check that it is a `TypeMeta::Unresolved`.
        match &*old_meta {
          TypeMeta::Unresolved(_) => {
            *old_meta = new_meta;
          },
          _ => {
            eprintln!("REDEFINE TYPE: {}", name);
          }
        }
        old_ref.clone()
      },
      Entry::Vacant(entry) => {
        entry.insert(type_def.clone());
        type_def
      }
    }
  }

  /// Dump types.
  pub fn dump_types(&self) {
    for (idx, (key, meta)) in self.types.iter().enumerate() {
      eprintln!("Type[{}]: {} => {:#?}", idx, key, meta.0.read().unwrap());
    }
  }

  /// Dump unresolved types.
  pub fn dump_unresolved(&self) {
    for (key, meta) in self.types.iter() {
      let meta = meta.0.read().unwrap();
      match &*meta {
        TypeMeta::Unresolved(def) => {
          eprintln!("--------- Unresolved: {} => {}", key, def);
        }
        _ => (),
      }
    }
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
}

pub fn init_engine(engine: &mut Engine) {
  engine
    .register_type_with_name::<TypeLookup>("TypeLookup")
    .register_fn("dump_types", TypeLookup::dump_types)
    .register_fn("dump_unresolved", TypeLookup::dump_unresolved)
    .register_result_fn("parse_named_type", |lookup: &mut TypeLookup, name: &str, def: &str| TypeLookup::parse_named_type(lookup, name, def))
    .register_result_fn("parse_type", |lookup: &mut TypeLookup, def: &str| TypeLookup::parse_type(lookup, def))
    .register_type_with_name::<Types>("Types")
    .register_type_with_name::<TypeMeta>("TypeMeta")
    .register_fn("to_string", TypeMeta::to_string)
    .register_type_with_name::<TypeRef>("TypeRef")
    .register_fn("to_string", TypeRef::to_string)
    ;
}

pub fn init_scope(schema: &str, scope: &mut Scope<'_>) -> Result<TypeLookup, Box<EvalAltResult>> {
  let mut types = Types::new();

  // basic types.
  types.insert_meta("u8", TypeMeta::Integer(1, false));
  types.insert_meta("u16", TypeMeta::Integer(2, false));
  types.insert_meta("u32", TypeMeta::Integer(4, false));
  types.insert_meta("u64", TypeMeta::Integer(8, false));
  types.insert_meta("u128", TypeMeta::Integer(16, false));
  types.insert_meta("bool", TypeMeta::Bool);
  types.insert_meta("Text", TypeMeta::String);
  types.insert_meta("Option<bool>", TypeMeta::OptionBool);

  // Load standard substrate types.
  types.load_schema("init_types.json")?;

  types.load_schema(schema)?;

  let lookup = TypeLookup::from_types(types);
  scope.push_constant("Types", lookup.clone());
  Ok(lookup)
}
