use std::any::TypeId;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use frame_metadata::{
  DecodeDifferent, DecodeDifferentArray, FunctionArgumentMetadata, FunctionMetadata,
  RuntimeMetadata, RuntimeMetadataPrefixed, StorageEntryType, StorageHasher, META_RESERVED,
};
use parity_scale_codec::{Encode, Output};

use rhai::plugin::NativeCallContext;
use rhai::{Dynamic, Engine, EvalAltResult, FnPtr, Map as RMap, INT};

use crate::client::Client;
use crate::types::{EnumVariants, TypeLookup, TypeMeta, TypeRef};

fn decode_meta<B: 'static, O: 'static>(
  encoded: &DecodeDifferent<B, O>,
) -> Result<&O, Box<EvalAltResult>> {
  match encoded {
    DecodeDifferent::Decoded(val) => Ok(val),
    _ => Err(format!("Failed to decode value.").into()),
  }
}

#[derive(Clone)]
pub struct Metadata {
  modules: HashMap<String, ModuleMetadata>,
  idx_map: HashMap<u8, String>,
}

impl Metadata {
  pub fn new(client: &Client, lookup: &TypeLookup) -> Result<Self, Box<EvalAltResult>> {
    // Get runtime metadata.
    let metadata_prefixed = client.get_metadata()?;

    Self::from_meta(metadata_prefixed, lookup)
  }

  pub fn from_meta(
    metadata_prefixed: RuntimeMetadataPrefixed,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    if metadata_prefixed.0 != META_RESERVED {
      return Err(format!("Invalid metadata prefix {}", metadata_prefixed.0).into());
    }

    // Get versioned metadata.
    let md = match metadata_prefixed.1 {
      RuntimeMetadata::V12(v12) => v12,
      _ => {
        return Err(format!("Unsupported metadata version").into());
      }
    };

    let mut api_md = Self {
      modules: HashMap::new(),
      idx_map: HashMap::new(),
    };

    // Top-level event/error types.
    let mut mod_events = EnumVariants::new();
    let mut mod_errors = EnumVariants::new();

    // Decode module metadata.
    decode_meta(&md.modules)?
      .iter()
      .try_for_each(|m| -> Result<(), Box<EvalAltResult>> {
        let m = ModuleMetadata::from_meta(m, lookup)?;
        let name = m.name.clone();
        mod_events.insert_at(m.index, &name, m.event_ref.clone());
        mod_errors.insert_at(m.index, &name, m.error_ref.clone());
        api_md.idx_map.insert(m.index, name.clone());
        api_md.modules.insert(name, m);
        Ok(())
      })?;

    let raw_event_ref = lookup.insert_meta("RawEvent", TypeMeta::Enum(mod_events));
    lookup.insert("Event", raw_event_ref);
    let raw_error_ref = lookup.insert_meta("RawError", TypeMeta::Enum(mod_errors));
    lookup.insert("Error", raw_error_ref);

    Ok(api_md)
  }

  pub fn add_encode_calls(
    &self,
    engine: &mut Engine,
    globals: &mut HashMap<String, Dynamic>,
  ) -> Result<(), Box<EvalAltResult>> {
    // Register each module as a global constant.
    for (_, module) in &self.modules {
      module.add_encode_calls(engine, globals)?;
    }

    Ok(())
  }

  fn modules(&mut self) -> Vec<Dynamic> {
    self.modules.values().cloned().map(Dynamic::from).collect()
  }

  pub fn get_module(&self, name: &str) -> Option<&ModuleMetadata> {
    self.modules.get(name)
  }

  fn find_error(&self, mod_idx: INT, err_idx: INT) -> Dynamic {
    let idx = mod_idx as u8;
    self
      .idx_map
      .get(&idx)
      .and_then(|mod_name| self.modules.get(mod_name))
      .map_or(Dynamic::UNIT, |module| module.find_error(err_idx))
  }

  fn indexer_get(&mut self, name: String) -> Result<Dynamic, Box<EvalAltResult>> {
    let m = self
      .modules
      .get(&name)
      .cloned()
      .ok_or_else(|| format!("Module {} not found", name))?;
    Ok(Dynamic::from(m))
  }
}

#[derive(Clone)]
pub struct ModuleMetadata {
  name: String,
  index: u8,
  storage_prefix: String,
  storage: HashMap<String, StorageMetadata>,
  funcs: HashMap<String, FuncMetadata>,
  events: HashMap<String, EventMetadata>,
  constants: HashMap<String, ConstMetadata>,
  errors: HashMap<String, ErrorMetadata>,
  err_idx_map: HashMap<u8, String>,
  event_ref: Option<TypeRef>,
  error_ref: Option<TypeRef>,
}

impl ModuleMetadata {
  fn from_meta(
    md: &frame_metadata::ModuleMetadata,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    let mod_idx = md.index;
    let mod_name = decode_meta(&md.name)?;
    let mut module = Self {
      name: mod_name.clone(),
      index: mod_idx,
      storage_prefix: "".into(),
      storage: HashMap::new(),
      funcs: HashMap::new(),
      events: HashMap::new(),
      constants: HashMap::new(),
      errors: HashMap::new(),
      err_idx_map: HashMap::new(),
      event_ref: None,
      error_ref: None,
    };

    // Decode module functions.
    if let Some(calls) = &md.calls {
      decode_meta(calls)?.iter().enumerate().try_for_each(
        |(func_idx, md)| -> Result<(), Box<EvalAltResult>> {
          let func = FuncMetadata::from_meta(&mod_name, mod_idx, func_idx as u8, md, lookup)?;
          let name = func.name.clone();
          module.funcs.insert(name, func);
          Ok(())
        },
      )?;
    }

    // Decode module storage.
    if let Some(storage) = &md.storage {
      let md = decode_meta(storage)?;
      let mod_prefix = decode_meta(&md.prefix)?;
      decode_meta(&md.entries)?
        .iter()
        .try_for_each(|md| -> Result<(), Box<EvalAltResult>> {
          let storage = StorageMetadata::from_meta(mod_prefix, md, lookup)?;
          let name = storage.name.clone();
          module.storage.insert(name, storage);
          Ok(())
        })?;
      module.storage_prefix = mod_prefix.into();
    }

    // Decode module events.
    if let Some(events) = &md.event {
      // Module RawEvent type.
      let mut raw_events = EnumVariants::new();

      decode_meta(events)?.iter().enumerate().try_for_each(
        |(event_idx, md)| -> Result<(), Box<EvalAltResult>> {
          let (event, ty_ref) =
            EventMetadata::from_meta(&mod_name, mod_idx, event_idx as u8, md, lookup)?;
          let name = event.name.clone();
          raw_events.insert_at(event.event_idx, &name, ty_ref);
          module.events.insert(name, event);
          Ok(())
        },
      )?;
      module.event_ref = Some(lookup.insert_meta(
        &format!("{}::RawEvent", mod_name),
        TypeMeta::Enum(raw_events),
      ));
    }

    // Decode module constants.
    decode_meta(&md.constants)?.iter().enumerate().try_for_each(
      |(const_idx, md)| -> Result<(), Box<EvalAltResult>> {
        let constant = ConstMetadata::from_meta(&mod_name, mod_idx, const_idx as u8, md, lookup)?;
        let name = constant.name.clone();
        module.constants.insert(name, constant);
        Ok(())
      },
    )?;

    // Decode module errors.
    // Module RawError type.
    let mut raw_errors = EnumVariants::new();

    decode_meta(&md.errors)?.iter().enumerate().try_for_each(
      |(error_idx, md)| -> Result<(), Box<EvalAltResult>> {
        let error = ErrorMetadata::from_meta(&mod_name, mod_idx, error_idx as u8, md)?;
        let name = error.name.clone();
        raw_errors.insert_at(error.error_idx, &name, None);
        module.err_idx_map.insert(error.error_idx, name.clone());
        module.errors.insert(name, error);
        Ok(())
      },
    )?;
    module.error_ref = Some(lookup.insert_meta(
      &format!("{}::RawError", mod_name),
      TypeMeta::Enum(raw_errors),
    ));

    Ok(module)
  }

  fn find_error(&self, err_idx: INT) -> Dynamic {
    let idx = err_idx as u8;
    self
      .err_idx_map
      .get(&idx)
      .and_then(|err_name| self.errors.get(err_name))
      .cloned()
      .map_or(Dynamic::UNIT, Dynamic::from)
  }

  pub fn add_encode_calls(
    &self,
    engine: &mut Engine,
    globals: &mut HashMap<String, Dynamic>,
  ) -> Result<(), Box<EvalAltResult>> {
    let mut map = RMap::new();
    for (name, func) in &self.funcs {
      map.insert(name.into(), func.add_encode_calls(engine)?);
    }

    globals.insert(self.name.clone(), map.into());
    Ok(())
  }

  fn index(&mut self) -> INT {
    self.index as INT
  }

  fn name(&mut self) -> String {
    self.name.clone()
  }

  fn funcs(&mut self) -> Vec<Dynamic> {
    self.funcs.values().cloned().map(Dynamic::from).collect()
  }

  fn events(&mut self) -> Vec<Dynamic> {
    self.events.values().cloned().map(Dynamic::from).collect()
  }

  fn constants(&mut self) -> Vec<Dynamic> {
    self
      .constants
      .values()
      .cloned()
      .map(Dynamic::from)
      .collect()
  }

  fn errors(&mut self) -> Vec<Dynamic> {
    self.errors.values().cloned().map(Dynamic::from).collect()
  }

  fn storage(&mut self) -> Vec<Dynamic> {
    self.storage.values().cloned().map(Dynamic::from).collect()
  }

  pub fn get_storage(&self, name: &str) -> Option<&StorageMetadata> {
    self.storage.get(name)
  }

  fn to_string(&mut self) -> String {
    format!("ModuleMetadata: {}", self.name)
  }

  fn indexer_get(&mut self, name: String) -> Result<Dynamic, Box<EvalAltResult>> {
    // Look for storage value matching that name.
    if let Some(storage) = self.storage.get(&name) {
      Ok(Dynamic::from(storage.clone()))
    } else {
      // If no matching storage, look for a matching call.
      if let Some(func) = self.funcs.get(&name) {
        Ok(Dynamic::from(func.clone()))
      } else {
        Err(format!("Storage or function {} not found", name).into())
      }
    }
  }
}

#[derive(Debug, Clone)]
pub struct NamedType {
  name: String,
  ty_meta: TypeRef,
}

impl NamedType {
  pub fn new(name: &str, lookup: &TypeLookup) -> Result<Self, Box<EvalAltResult>> {
    let ty_meta = lookup.parse_type(name)?;
    let named = Self {
      name: name.into(),
      ty_meta,
    };

    Ok(named)
  }

  pub fn encode_value(&self, param: Dynamic, data: &mut EncodedArgs) -> Result<(), Box<EvalAltResult>> {
    self.ty_meta.encode_value(param, data)
  }

  pub fn decode(&self, data: Vec<u8>) -> Result<Dynamic, Box<EvalAltResult>> {
    self.ty_meta.decode(data)
  }

  fn get_name(&mut self) -> String {
    self.name.clone()
  }

  fn get_meta(&mut self) -> TypeRef {
    self.ty_meta.clone()
  }

  fn to_string(&mut self) -> String {
    format!("{}: {:?}", self.name, self.ty_meta)
  }
}

#[derive(Debug, Clone)]
pub struct KeyHasher {
  pub hashers: Vec<StorageHasher>,
  pub types: Vec<NamedType>,
}

impl KeyHasher {
  pub fn encode_map_key(&self, key: Dynamic) -> Result<Vec<u8>, Box<EvalAltResult>> {
    let mut buf = EncodedArgs::new();
    match self.types.len() {
      0 => Err(format!("This storage isn't a map type."))?,
      1 => {
        self.types[0].encode_value(key, &mut buf)?;
      },
      _ => {
        Err(format!("This storage isn't a double map type."))?;
      },
    }
    Ok(buf.into_inner())
  }

  pub fn encode_double_map_key(&self, key1: Dynamic, key2: Dynamic) -> Result<(Vec<u8>, Vec<u8>), Box<EvalAltResult>> {
    let mut buf1 = EncodedArgs::new();
    let mut buf2 = EncodedArgs::new();
    match self.types.len() {
      2 => {
        self.types[0].encode_value(key1, &mut buf1)?;
        self.types[1].encode_value(key2, &mut buf2)?;
      },
      _ => Err(format!("This storage isn't a double map type."))?,
    }
    Ok((buf1.into_inner(), buf2.into_inner()))
  }
}

#[derive(Clone)]
pub struct StorageMetadata {
  pub prefix: String,
  pub name: String,
  pub key_hasher: Option<KeyHasher>,
  pub value_ty: NamedType,
  pub docs: Docs,
}

impl StorageMetadata {
  fn from_meta(
    prefix: &str,
    md: &frame_metadata::StorageEntryMetadata,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    let (key_hasher, value) = match &md.ty {
      StorageEntryType::Plain(value) => (None, value.clone()),
      StorageEntryType::Map {
        hasher, key, value, ..
      } => {
        let hasher = KeyHasher {
          hashers: vec![hasher.clone()],
          types: vec![NamedType::new(decode_meta(key)?, lookup)?],
        };
        (Some(hasher), value.clone())
      }
      StorageEntryType::DoubleMap {
        hasher,
        key1,
        key2_hasher,
        key2,
        value,
      } => {
        let hasher = KeyHasher {
          hashers: vec![hasher.clone(), key2_hasher.clone()],
          types: vec![
            NamedType::new(decode_meta(key1)?, lookup)?,
            NamedType::new(decode_meta(key2)?, lookup)?,
          ],
        };
        (Some(hasher), value.clone())
      }
    };
    let storage = Self {
      prefix: prefix.into(),
      name: decode_meta(&md.name)?.clone(),
      key_hasher,
      value_ty: NamedType::new(decode_meta(&value)?, lookup)?,
      docs: Docs::from_meta(&md.documentation)?,
    };

    Ok(storage)
  }

  pub fn encode_map_key(&self, key: Dynamic) -> Result<Vec<u8>, Box<EvalAltResult>> {
    match &self.key_hasher {
      Some(hasher) => hasher.encode_map_key(key),
      None => Err(format!("This storage type doesn't have keys.").into()),
    }
  }

  pub fn encode_double_map_key(&self, key1: Dynamic, key2: Dynamic) -> Result<(Vec<u8>, Vec<u8>), Box<EvalAltResult>> {
    match &self.key_hasher {
      Some(hasher) => hasher.encode_double_map_key(key1, key2),
      None => Err(format!("This storage type doesn't have keys.").into()),
    }
  }

  pub fn decode_value(&self, data: Vec<u8>) -> Result<Dynamic, Box<EvalAltResult>> {
    self.value_ty.decode(data)
  }

  fn title(&mut self) -> String {
    self.docs.title()
  }

  fn docs(&mut self) -> String {
    self.docs.to_string()
  }

  fn to_string(&mut self) -> String {
    format!(
      "StorageMetadata: {}, key_hasher: {:?}, value: {:?}",
      self.name, self.key_hasher, self.value_ty
    )
  }
}

#[derive(Clone)]
pub struct EventMetadata {
  mod_name: String,
  name: String,
  event_idx: u8,
  args: Vec<NamedType>,
  docs: Docs,
}

impl EventMetadata {
  fn from_meta(
    mod_name: &str,
    _mod_idx: u8,
    event_idx: u8,
    md: &frame_metadata::EventMetadata,
    lookup: &TypeLookup,
  ) -> Result<(Self, Option<TypeRef>), Box<EvalAltResult>> {
    let mut event = Self {
      mod_name: mod_name.into(),
      name: decode_meta(&md.name)?.clone(),
      event_idx,
      args: Vec::new(),
      docs: Docs::from_meta(&md.documentation)?,
    };

    let mut event_tuple = Vec::new();

    // Decode event arguments.
    decode_meta(&md.arguments)?
      .iter()
      .try_for_each(|name| -> Result<(), Box<EvalAltResult>> {
        let arg = NamedType::new(name, lookup)?;
        event_tuple.push(arg.ty_meta.clone());
        event.args.push(arg);
        Ok(())
      })?;

    let event_ref = if event_tuple.len() > 0 {
      let type_name = format!("{}::RawEvent::{}", mod_name, event.name);
      Some(lookup.insert_meta(&type_name, TypeMeta::Tuple(event_tuple)))
    } else {
      None
    };

    Ok((event, event_ref))
  }

  fn args(&mut self) -> Dynamic {
    let args: Vec<Dynamic> = self
      .args
      .iter()
      .map(|arg| Dynamic::from(arg.clone()))
      .collect();
    Dynamic::from(args)
  }

  fn title(&mut self) -> String {
    self.docs.title()
  }

  fn docs(&mut self) -> String {
    self.docs.to_string()
  }

  fn to_string(&mut self) -> String {
    let args = self
      .args
      .iter_mut()
      .map(|a| a.to_string())
      .collect::<Vec<String>>()
      .join(", ");
    format!("Event: {}.{}({})", self.mod_name, self.name, args)
  }
}

#[derive(Clone)]
pub struct ConstMetadata {
  mod_name: String,
  name: String,
  const_ty: NamedType,
  docs: Docs,
}

impl ConstMetadata {
  fn from_meta(
    mod_name: &str,
    _mod_idx: u8,
    _const_idx: u8,
    md: &frame_metadata::ModuleConstantMetadata,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    let ty = decode_meta(&md.ty)?;
    let const_ty = NamedType::new(ty, lookup)?;
    Ok(Self {
      mod_name: mod_name.into(),
      name: decode_meta(&md.name)?.clone(),
      const_ty,
      docs: Docs::from_meta(&md.documentation)?,
    })
  }

  fn title(&mut self) -> String {
    self.docs.title()
  }

  fn docs(&mut self) -> String {
    self.docs.to_string()
  }

  fn to_string(&mut self) -> String {
    format!(
      "Constant: {}.{}({})",
      self.mod_name,
      self.name,
      self.const_ty.to_string()
    )
  }
}

#[derive(Clone)]
pub struct ErrorMetadata {
  mod_name: String,
  name: String,
  error_idx: u8,
  docs: Docs,
}

impl ErrorMetadata {
  fn from_meta(
    mod_name: &str,
    _mod_idx: u8,
    error_idx: u8,
    md: &frame_metadata::ErrorMetadata,
  ) -> Result<Self, Box<EvalAltResult>> {
    Ok(Self {
      mod_name: mod_name.into(),
      name: decode_meta(&md.name)?.clone(),
      error_idx,
      docs: Docs::from_meta(&md.documentation)?,
    })
  }

  fn index(&mut self) -> INT {
    self.error_idx as INT
  }

  fn name(&mut self) -> String {
    self.name.clone()
  }

  fn title(&mut self) -> String {
    self.docs.title()
  }

  fn docs(&mut self) -> String {
    self.docs.to_string()
  }

  fn to_string(&mut self) -> String {
    format!("Error: {}.{}", self.mod_name, self.name)
  }
}

#[derive(Clone, Encode)]
pub struct EncodedCall(u8, u8, EncodedArgs);

impl EncodedCall {
  pub fn len(&mut self) -> i64 {
    2 + self.2.len()
  }

  fn to_string(&mut self) -> String {
    let encoded = self.encode();
    format!("0x{}", hex::encode(&encoded))
  }

  pub fn into_call(self) -> (u8, u8, EncodedArgs) {
    (self.0, self.1, self.2)
  }
}

#[derive(Clone, Default)]
pub struct EncodedArgs {
  data: Vec<u8>,
  compact: bool,
}

impl EncodedArgs {
  pub fn new() -> Self {
    Self {
      data: Vec::with_capacity(256),
      compact: false,
    }
  }

  pub fn is_compact(&self) -> bool {
    self.compact
  }

  pub fn set_compact(&mut self, compact: bool) {
    self.compact = compact;
  }

  pub fn encode<T: Encode>(&mut self, val: T) {
    val.encode_to(&mut self.data)
  }

  pub fn write(&mut self, bytes: &[u8]) {
    self.data.extend(bytes);
  }

  pub fn len(&mut self) -> i64 {
    self.data.len() as i64
  }

  fn to_string(&mut self) -> String {
    format!("0x{}", hex::encode(&self.data))
  }

  pub fn into_inner(self) -> Vec<u8> {
    self.data
  }
}

impl Encode for EncodedArgs {
  fn size_hint(&self) -> usize {
    self.data.len()
  }

  fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
    dest.write(&self.data)
  }
}

impl Deref for EncodedArgs {
  type Target = Vec<u8>;

  fn deref(&self) -> &Self::Target {
    &self.data
  }
}

impl DerefMut for EncodedArgs {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.data
  }
}

#[derive(Clone)]
pub struct FuncMetadata {
  mod_name: String,
  name: String,
  mod_idx: u8,
  func_idx: u8,
  args: Vec<FuncArg>,
  docs: Docs,
}

impl FuncMetadata {
  fn from_meta(
    mod_name: &str,
    mod_idx: u8,
    func_idx: u8,
    md: &FunctionMetadata,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    let mut func = Self {
      mod_name: mod_name.into(),
      name: decode_meta(&md.name)?.clone(),
      mod_idx,
      func_idx,
      args: Vec::new(),
      docs: Docs::from_meta(&md.documentation)?,
    };

    // Decode function arguments.
    decode_meta(&md.arguments)?
      .iter()
      .try_for_each(|md| -> Result<(), Box<EvalAltResult>> {
        let arg = FuncArg::from_meta(md, lookup)?;
        func.args.push(arg);
        Ok(())
      })?;

    Ok(func)
  }

  pub fn add_encode_calls(&self, engine: &mut Engine) -> Result<Dynamic, Box<EvalAltResult>> {
    let full_name = format!("{}_{}", self.mod_name, self.name);
    let mut args = vec![TypeId::of::<RMap>(), TypeId::of::<FuncMetadata>()];
    let args_len = self.args.len();
    if args_len > 0 {
      args.extend([TypeId::of::<Dynamic>()].repeat(args_len));
    }
    #[allow(deprecated)]
    engine.register_raw_fn(&full_name, &args, encode_call);

    let mut encode_call = FnPtr::new(full_name)?;
    encode_call.add_curry(Dynamic::from(self.clone()));
    Ok(Dynamic::from(encode_call))
  }

  fn args(&mut self) -> Dynamic {
    let args: Vec<Dynamic> = self
      .args
      .iter()
      .map(|arg| Dynamic::from(arg.clone()))
      .collect();
    Dynamic::from(args)
  }

  fn title(&mut self) -> String {
    self.docs.title()
  }

  fn docs(&mut self) -> String {
    self.docs.to_string()
  }

  fn encode_call(&self, params: &[&mut Dynamic]) -> Result<EncodedCall, Box<EvalAltResult>> {
    let mut data = EncodedArgs::new();
    self.encode_params(params, &mut data)?;
    Ok(EncodedCall(self.mod_idx, self.func_idx, data))
  }

  fn encode_params(
    &self,
    params: &[&mut Dynamic],
    data: &mut EncodedArgs,
  ) -> Result<(), Box<EvalAltResult>> {
    let param_len = params.len();
    if param_len > self.args.len() {
      Err(format!("Too many parameters"))?
    }
    for (idx, arg) in self.args.iter().enumerate() {
      if let Some(param) = params.get(idx).map(|p| (*p).clone()) {
        arg.encode_value(param, data)?;
      } else {
        // TODO: Check if parameter is optional.
        Err(format!("Too many parameters"))?
      }
    }
    Ok(())
  }

  fn to_string(&mut self) -> String {
    let args = self
      .args
      .iter_mut()
      .map(|a| a.to_string())
      .collect::<Vec<String>>()
      .join(", ");
    format!("Func: {}.{}({})", self.mod_name, self.name, args)
  }
}

#[derive(Clone)]
pub struct FuncArg {
  name: String,
  ty: NamedType,
}

impl FuncArg {
  fn from_meta(
    md: &FunctionArgumentMetadata,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    let arg = Self {
      name: decode_meta(&md.name)?.clone(),
      ty: NamedType::new(decode_meta(&md.ty)?, lookup)?,
    };

    Ok(arg)
  }

  fn encode_value(&self, param: Dynamic, data: &mut EncodedArgs) -> Result<(), Box<EvalAltResult>> {
    self.ty.encode_value(param, data)
  }

  fn get_name(&mut self) -> String {
    self.name.clone()
  }

  fn get_type(&mut self) -> String {
    self.ty.name.clone()
  }

  fn get_meta(&mut self) -> TypeRef {
    self.ty.ty_meta.clone()
  }

  fn to_string(&mut self) -> String {
    format!("{}: {:?}", self.name, self.ty.ty_meta)
  }
}

#[derive(Clone)]
pub struct Docs {
  lines: Vec<String>,
}

impl Docs {
  fn from_meta(md: &DecodeDifferentArray<&'static str, String>) -> Result<Self, Box<EvalAltResult>> {
    Ok(Self {
      lines: decode_meta(md)?.clone(),
    })
  }

  pub fn title(&mut self) -> String {
    self
      .lines
      .first()
      .map(|s| s.trim().into())
      .unwrap_or_default()
  }

  fn to_string(&mut self) -> String {
    self.lines.join("\n")
  }
}

fn encode_call(
  _ctx: NativeCallContext,
  args: &mut [&mut Dynamic],
) -> Result<EncodedCall, Box<EvalAltResult>> {
  let func = args
    .get(1)
    .and_then(|a| (*a).clone().try_cast::<FuncMetadata>())
    .ok_or_else(|| format!("Missing arg 0."))?;
  func.encode_call(&args[2..])
}

pub fn init_engine(
  engine: &mut Engine,
  globals: &mut HashMap<String, Dynamic>,
  client: &Client,
  lookup: &TypeLookup,
) -> Result<Metadata, Box<EvalAltResult>> {
  engine
    .register_type_with_name::<Metadata>("Metadata")
    .register_get("modules", Metadata::modules)
    .register_fn(
      "find_error",
      |md: &mut Metadata, mod_idx: INT, err_idx: INT| md.find_error(mod_idx, err_idx),
    )
    .register_indexer_get_result(Metadata::indexer_get)
    .register_type_with_name::<ModuleMetadata>("ModuleMetadata")
    .register_get("name", ModuleMetadata::name)
    .register_get("index", ModuleMetadata::index)
    .register_get("funcs", ModuleMetadata::funcs)
    .register_get("events", ModuleMetadata::events)
    .register_get("constants", ModuleMetadata::constants)
    .register_get("errors", ModuleMetadata::errors)
    .register_get("storage", ModuleMetadata::storage)
    .register_fn("to_string", ModuleMetadata::to_string)
    .register_indexer_get_result(ModuleMetadata::indexer_get)
    .register_type_with_name::<StorageMetadata>("StorageMetadata")
    .register_fn("to_string", StorageMetadata::to_string)
    .register_get("title", StorageMetadata::title)
    .register_get("docs", StorageMetadata::docs)
    .register_type_with_name::<FuncMetadata>("FuncMetadata")
    .register_fn("to_string", FuncMetadata::to_string)
    .register_get("args", FuncMetadata::args)
    .register_get("title", FuncMetadata::title)
    .register_get("docs", FuncMetadata::docs)
    .register_type_with_name::<FuncArg>("FuncArg")
    .register_fn("to_string", FuncArg::to_string)
    .register_fn("name", FuncArg::get_name)
    .register_fn("type", FuncArg::get_type)
    .register_fn("meta", FuncArg::get_meta)
    .register_type_with_name::<EventMetadata>("EventMetadata")
    .register_fn("to_string", EventMetadata::to_string)
    .register_get("args", EventMetadata::args)
    .register_get("title", EventMetadata::title)
    .register_get("docs", EventMetadata::docs)
    .register_type_with_name::<ConstMetadata>("ConstMetadata")
    .register_fn("to_string", ConstMetadata::to_string)
    .register_get("title", ConstMetadata::title)
    .register_get("docs", ConstMetadata::docs)
    .register_type_with_name::<ErrorMetadata>("ErrorMetadata")
    .register_get("name", ErrorMetadata::name)
    .register_get("index", ErrorMetadata::index)
    .register_fn("to_string", ErrorMetadata::to_string)
    .register_get("title", ErrorMetadata::title)
    .register_get("docs", ErrorMetadata::docs)
    .register_type_with_name::<NamedType>("NamedType")
    .register_fn("to_string", NamedType::to_string)
    .register_get("name", NamedType::get_name)
    .register_get("meta", NamedType::get_meta)
    .register_type_with_name::<EncodedArgs>("EncodedArgs")
    .register_fn("len", EncodedArgs::len)
    .register_fn("to_string", EncodedArgs::to_string)
    .register_type_with_name::<EncodedCall>("EncodedCall")
    .register_fn("len", EncodedCall::len)
    .register_fn("to_string", EncodedCall::to_string)
    .register_type_with_name::<Docs>("Docs")
    .register_fn("to_string", Docs::to_string)
    .register_get("title", Docs::title);

  let metadata = Metadata::new(client, lookup)?;

  lookup.custom_encode("Call", TypeId::of::<EncodedCall>(), |value, data| {
    let call = value.cast::<EncodedCall>();
    data.encode(call);
    Ok(())
  })?;

  // Register each module as a global constant.
  metadata.add_encode_calls(engine, globals)?;

  globals.insert("METADATA".into(), Dynamic::from(metadata.clone()));
  Ok(metadata)
}
