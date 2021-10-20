use std::sync::{Arc, RwLock};
use std::fs::File;
use std::io::BufReader;

use serde_json::{
  Value, Map,
};

use rhai::{Engine, EvalAltResult, Scope};

use indexmap::map::IndexMap;

pub type TypeRef = u32;

#[derive(Debug, Clone)]
pub enum TypeMeta {
  /// Zero-sized `()`
  Unit,
  /// (width, signed)
  Integer(u8, bool),
  Bool,
  Option(TypeRef),
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

  Unparsed(String),
}

impl Default for TypeMeta {
  fn default() -> Self {
    Self::Unit
  }
}

#[derive(Clone)]
pub struct Types {
  types: IndexMap<String, TypeMeta>,
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
        self.insert(name, TypeMeta::Enum(variants));
      }
      Value::Object(obj) => {
        let variants = obj.iter().try_fold(IndexMap::new(), |mut map, (var_name, val)| -> Result<_, Box<EvalAltResult>> {
          match val.as_str() {
            Some("") => {
              map.insert(var_name.to_string(), None);
              Ok(map)
            }
            Some(var_def) => {
              let (type_ref, _) = self.parse_type(var_def)?;
              map.insert(var_name.to_string(), Some(type_ref));
              Ok(map)
            }
            None => Err(format!("Expected json string for enum {}: got {:?}", name, val).into()),
          }
        })?;
        self.insert(name, TypeMeta::Enum(variants));
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
          let (type_ref, _) = self.parse_type(field_def)?;
          map.insert(field_name.to_string(), type_ref);
          Ok(map)
        }
        None => Err(format!("Expected json string for struct {} field {}: got {:?}", name, field_name, val).into()),
      }
    })?;
    self.insert(name, TypeMeta::Struct(fields));
    Ok(())
  }

  fn parse_named_type(&mut self, name: &str, def: &str) -> Result<TypeRef, Box<EvalAltResult>> {
    let (_, type_meta) = self.parse_type(def)?;

    let type_ref = self.insert(name, type_meta);
    Ok(type_ref)
  }

  fn parse_type(&mut self, def: &str) -> Result<(TypeRef, TypeMeta), Box<EvalAltResult>> {
    let def = def.trim();
    // Try to resolve the type.
    let (type_ref, type_meta) = self.resolve(def);

    // Check if type is unparsed.
    match &type_meta {
      TypeMeta::Unparsed(def) => {
        let meta = match def.chars().last() {
          Some('>') => {
            // Handle: Vec<T>, Option<T>, Compact<T>
            let (wrap, ty) = def
              .strip_suffix('>')
              .and_then(|s| s.split_once('<'))
              .map(|(wrap, ty)| (wrap.trim(), ty.trim()))
              .ok_or_else(|| format!("Failed to parse Vec/Option/Compact: {}", def))?;
            let (wrap_ref, _) = self.parse_type(ty)?;
            match wrap {
              "Vec" => {
                TypeMeta::Vector(wrap_ref)
              }
              "Option" => {
                TypeMeta::Option(wrap_ref)
              }
              "Compact" => {
                TypeMeta::Compact(wrap_ref)
              }
              _ => {
                return Err(format!("Unknown wrapper: {}", def).into());
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
                let (type_ref, _) = self.parse_type(val)?;
                vec.push(type_ref);
                Ok(vec)
              })?;
            // Handle tuples.
            TypeMeta::Tuple(defs)
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
            let (slice_ref, _) = self.parse_type(slice_ty)?;
            TypeMeta::Slice(slice_len, slice_ref)
          }
          _ => {
            type_meta
          }
        };
        if let Some((_, val)) = self.types.get_index_mut(type_ref as usize) {
          *val = meta.clone();
        }
        Ok((type_ref, meta))
      }
      _ => Ok((type_ref, type_meta)),
    }
  }

  pub fn resolve(&mut self, name: &str) -> (TypeRef, TypeMeta) {
    let entry = self.types.entry(name.into());
    let type_ref = entry.index() as TypeRef;
    let type_meta = entry.or_insert_with(|| TypeMeta::Unparsed(name.into()));
    (type_ref, type_meta.clone())
  }

  pub fn insert(&mut self, name: &str, type_def: TypeMeta) -> TypeRef {
    use indexmap::map::Entry;
    let entry = self.types.entry(name.into());
    let type_ref = entry.index() as TypeRef;
    match entry {
      Entry::Occupied(mut entry) => {
        let old = entry.insert(type_def);
        // Already exists.  Check that it is a `TypeMeta::Unparsed`.
        match old {
          TypeMeta::Unparsed(_) => (),
          _ => {
            eprintln!("REDEFINE TYPE: {}", name);
          }
        }
      },
      Entry::Vacant(entry) => {
        entry.insert(type_def);
      }
    }
    type_ref
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

  pub fn insert(&self, name: &str, type_def: TypeMeta) -> TypeRef {
    let mut t = self.types.write().unwrap();
    t.insert(name, type_def)
  }
}

pub fn init_engine(engine: &mut Engine) {
  engine
    .register_type_with_name::<TypeLookup>("TypeLookup")
    ;
}

pub fn init_scope(schema: &str, scope: &mut Scope<'_>) -> Result<TypeLookup, Box<EvalAltResult>> {
  let mut types = Types::new();

  // basic types.
  types.insert("u8", TypeMeta::Integer(1, false));
  types.insert("u16", TypeMeta::Integer(2, false));
  types.insert("u32", TypeMeta::Integer(4, false));
  types.insert("u64", TypeMeta::Integer(8, false));
  types.insert("u128", TypeMeta::Integer(16, false));
  types.insert("bool", TypeMeta::Bool);
  types.insert("Text", TypeMeta::String);
  types.insert("Option<bool>", TypeMeta::OptionBool);

  // Define some standard types.
  let standard = serde_json::json!({
    "Balance": "u128",
    "AccountId": "[u8; 32]",
    "AccountIndex": "u32",
    "BlockNumber": "u32",
    "Permill": "u32",
    "Perbill": "u32",
    "H256": "[u8; 32]",
    "H512": "[u8; 64]",
    "Hash": "H256",
    "MultiAddress": {
      "_enum": {
        "Id": "AccountId",
        "Index": "AccountIndex",
        "Raw": "Vec<u8>",
        "Address32": "[u8; 32]",
        "Address20": "[u8; 20]",
      }
    }
  });
  types.parse_schema_types(standard.as_object().unwrap())?;

  types.load_schema(schema)?;

  // Dump types.
  /*
  for (idx, (key, meta)) in types.types.iter().enumerate() {
    eprintln!("Type[{}]: {} => {:#?}", idx, key, meta);
  }
  */

  // Collect list of unresolved types.
  let unresolved = types.types.iter()
    .filter_map(|(key, meta)| {
      match meta {
        TypeMeta::Unparsed(def) if key != def => {
          Some((key.to_string(), def.to_string()))
        }
        _ => None,
      }
    }).collect::<Vec<_>>();

  // Try to resolve list of unresolved types.
  for (name, def) in unresolved {
    let (type_ref, _) = types.resolve(&def);
    types.insert(&name, TypeMeta::NewType(type_ref));
  }

  /*
  for (key, meta) in types.types.iter() {
    match meta {
      TypeMeta::Unparsed(def) => {
        eprintln!("--------- Unparsed: {} => {}", key, def);
      }
      _ => (),
    }
  }
  */

  let lookup = TypeLookup::from_types(types);
  scope.push_constant("Types", lookup.clone());
  Ok(lookup)
}
