use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::any::TypeId;

use frame_metadata::{
  DecodeDifferent, DecodeDifferentArray,
  FunctionArgumentMetadata, FunctionMetadata,
  RuntimeMetadata, RuntimeMetadataPrefixed, META_RESERVED,
  StorageEntryType, StorageHasher,
};
use parity_scale_codec::{Encode, Output};

use rhai::{Dynamic, Engine, EvalAltResult, FnPtr, Map as RMap, Scope};
use rhai::plugin::NativeCallContext;

use indexmap::map::IndexMap;

use super::types::{TypeLookup, TypeRef, TypeMeta};

fn decode<B: 'static, O: 'static>(
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
}

impl Metadata {
  pub fn new(url: &str, lookup: &TypeLookup) -> Result<Self, Box<EvalAltResult>> {
    let client = crate::client::Client::connect(url)?;

    // Get runtime metadata.
    let metadata_prefixed = client.get_metadata()?;

    Self::decode(metadata_prefixed, lookup)
  }

  pub fn decode(
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
    };

    // Top-level event type.
    let mut mod_events = IndexMap::new();

    // Decode module metadata.
    decode(&md.modules)?
      .iter()
      .try_for_each(|m| -> Result<(), Box<EvalAltResult>> {
        let m = ModuleMetadata::decode(m, lookup)?;
        let name = m.name.clone();
        if let Some(event_ref) = &m.event_ref {
          mod_events.insert(name.clone(), Some(event_ref.clone()));
        }
        api_md.modules.insert(name, m);
        Ok(())
      })?;

    let raw_event_ref = lookup.insert_meta("RawEvent", TypeMeta::Enum(mod_events));
    lookup.insert("Event", raw_event_ref);

    Ok(api_md)
  }

  pub fn add_encode_calls(&self, engine: &mut Engine, scope: &mut Scope<'_>) -> Result<(), Box<EvalAltResult>> {
    // Register each module as a global constant.
    for (_, module) in &self.modules {
      module.add_encode_calls(engine, scope)?;
    }

    Ok(())
  }

  fn modules(&mut self) -> Vec<Dynamic> {
    self.modules.values().cloned().map(Dynamic::from).collect()
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
  event_ref: Option<TypeRef>,
}

impl ModuleMetadata {
  fn decode(md: &frame_metadata::ModuleMetadata, lookup: &TypeLookup) -> Result<Self, Box<EvalAltResult>> {
    let mod_idx = md.index;
    let mod_name = decode(&md.name)?;
    let mut module = Self {
      name: mod_name.clone(),
      index: mod_idx,
      storage_prefix: "".into(),
      storage: HashMap::new(),
      funcs: HashMap::new(),
      events: HashMap::new(),
      event_ref: None,
    };

    // Decode module functions.
    if let Some(calls) = &md.calls {
      decode(calls)?.iter().enumerate().try_for_each(
        |(func_idx, md)| -> Result<(), Box<EvalAltResult>> {
          let func = FuncMetadata::decode(&mod_name, mod_idx, func_idx as u8, md, lookup)?;
          let name = func.name.clone();
          module.funcs.insert(name, func);
          Ok(())
        },
      )?;
    }

    // Decode module storage.
    if let Some(storage) = &md.storage {
      let md = decode(storage)?;
      let mod_prefix = decode(&md.prefix)?;
      decode(&md.entries)?
        .iter()
        .try_for_each(|md| -> Result<(), Box<EvalAltResult>> {
          let storage = StorageMetadata::decode(mod_prefix, md, lookup)?;
          let name = storage.name.clone();
          module.storage.insert(name, storage);
          Ok(())
        })?;
      module.storage_prefix = mod_prefix.into();
    }

    // Decode module events.
    if let Some(events) = &md.event {
      // Module RawEvent type.
      let mut raw_events = IndexMap::new();

      decode(events)?.iter().enumerate().try_for_each(
        |(event_idx, md)| -> Result<(), Box<EvalAltResult>> {
          let (event, ty_ref) = EventMetadata::decode(&mod_name, mod_idx, event_idx as u8, md, lookup)?;
          let name = event.name.clone();
          raw_events.insert(name.clone(), ty_ref);
          module.events.insert(name, event);
          Ok(())
        },
      )?;
      module.event_ref = Some(lookup.insert_meta(&format!("{}::RawEvent", mod_name), TypeMeta::Enum(raw_events)));
    }

    Ok(module)
  }

  pub fn add_encode_calls(&self, engine: &mut Engine, scope: &mut Scope<'_>) -> Result<(), Box<EvalAltResult>> {
    let mut map = RMap::new();
    for (name, func) in &self.funcs {
      map.insert(name.into(), func.add_encode_calls(engine)?);
    }

    scope.push_dynamic(self.name.clone(), map.into());
    Ok(())
  }

  fn funcs(&mut self) -> Vec<Dynamic> {
    self.funcs.values().cloned().map(Dynamic::from).collect()
  }

  fn events(&mut self) -> Vec<Dynamic> {
    self.events.values().cloned().map(Dynamic::from).collect()
  }

  fn storage(&mut self) -> Vec<Dynamic> {
    self.storage.values().cloned().map(Dynamic::from).collect()
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

  fn encode_value(&self, param: Dynamic, data: &mut EncodedArgs) -> Result<(), Box<EvalAltResult>> {
    self.ty_meta.encode_value(param, data)
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
  hashers: Vec<StorageHasher>,
  types: Vec<NamedType>,
}

#[derive(Clone)]
pub struct StorageMetadata {
  prefix: String,
  name: String,
  key_hasher: Option<KeyHasher>,
  value_ty: NamedType,
  docs: Docs,
}

impl StorageMetadata {
  fn decode(prefix: &str, md: &frame_metadata::StorageEntryMetadata, lookup: &TypeLookup) -> Result<Self, Box<EvalAltResult>> {
    let (key_hasher, value) = match &md.ty {
      StorageEntryType::Plain(value) => {
        (None, value.clone())
      },
      StorageEntryType::Map{ hasher, key, value, .. } => {
        let hasher = KeyHasher {
          hashers: vec![hasher.clone()],
          types: vec![NamedType::new(decode(key)?, lookup)?],
        };
        (Some(hasher), value.clone())
      },
      StorageEntryType::DoubleMap{ hasher, key1, key2_hasher, key2, value} => {
        let hasher = KeyHasher {
          hashers: vec![hasher.clone(), key2_hasher.clone()],
          types: vec![
            NamedType::new(decode(key1)?, lookup)?,
            NamedType::new(decode(key2)?, lookup)?
          ],
        };
        (Some(hasher), value.clone())
      },
    };
    let storage = Self {
      prefix: prefix.into(),
      name: decode(&md.name)?.clone(),
      key_hasher,
      value_ty: NamedType::new(decode(&value)?, lookup)?,
      docs: Docs::decode(&md.documentation)?,
    };

    Ok(storage)
  }

  fn title(&mut self) -> String {
    self.docs.title()
  }

  fn docs(&mut self) -> String {
    self.docs.to_string()
  }

  fn to_string(&mut self) -> String {
    format!("StorageMetadata: {}, key_hasher: {:?}, value: {:?}", self.name, self.key_hasher, self.value_ty)
  }
}

#[derive(Clone)]
pub struct EventMetadata {
  mod_name: String,
  name: String,
  mod_idx: u8,
  event_idx: u8,
  args: Vec<NamedType>,
  docs: Docs,
}

impl EventMetadata {
  fn decode(
    mod_name: &str,
    mod_idx: u8,
    event_idx: u8,
    md: &frame_metadata::EventMetadata,
    lookup: &TypeLookup,
  ) -> Result<(Self, Option<TypeRef>), Box<EvalAltResult>> {
    let mut event = Self {
      mod_name: mod_name.into(),
      name: decode(&md.name)?.clone(),
      mod_idx,
      event_idx,
      args: Vec::new(),
      docs: Docs::decode(&md.documentation)?,
    };

    let mut event_tuple = Vec::new();

    // Decode event arguments.
    decode(&md.arguments)?
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
    let args: Vec<Dynamic> = self.args.iter().map(|arg| Dynamic::from(arg.clone())).collect();
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
pub struct EncodedArgs{
  data: Vec<u8>,
  compact: bool,
}

impl EncodedArgs {
  pub fn new() -> Self {
    Self{
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

  pub fn len(&mut self) -> i64 {
    self.data.len() as i64
  }

  fn to_string(&mut self) -> String {
    format!("0x{}", hex::encode(&self.data))
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
  fn decode(
    mod_name: &str,
    mod_idx: u8,
    func_idx: u8,
    md: &FunctionMetadata,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    let mut func = Self {
      mod_name: mod_name.into(),
      name: decode(&md.name)?.clone(),
      mod_idx,
      func_idx,
      args: Vec::new(),
      docs: Docs::decode(&md.documentation)?,
    };

    // Decode function arguments.
    decode(&md.arguments)?
      .iter()
      .try_for_each(|md| -> Result<(), Box<EvalAltResult>> {
        let arg = FuncArg::decode(md, lookup)?;
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
    let args: Vec<Dynamic> = self.args.iter().map(|arg| Dynamic::from(arg.clone())).collect();
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

  fn encode_params(&self, params: &[&mut Dynamic], data: &mut EncodedArgs) -> Result<(), Box<EvalAltResult>> {
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
  fn decode(md: &FunctionArgumentMetadata, lookup: &TypeLookup) -> Result<Self, Box<EvalAltResult>> {
    let arg = Self {
      name: decode(&md.name)?.clone(),
      ty: NamedType::new(decode(&md.ty)?, lookup)?,
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
  fn decode(md: &DecodeDifferentArray<&'static str, String>) -> Result<Self, Box<EvalAltResult>> {
    Ok(Self {
      lines: decode(md)?.clone(),
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

fn encode_call(_ctx: NativeCallContext, args: &mut [&mut Dynamic]) -> Result<EncodedCall, Box<EvalAltResult>> {
  let func = args.get(1).and_then(|a| (*a).clone().try_cast::<FuncMetadata>()).ok_or_else(|| format!("Missing arg 0."))?;
  func.encode_call(&args[2..])
}

pub fn init_engine(engine: &mut Engine) {
  engine
    .register_type_with_name::<Metadata>("Metadata")
    .register_get("modules", Metadata::modules)
    .register_indexer_get_result(Metadata::indexer_get)

    .register_type_with_name::<ModuleMetadata>("ModuleMetadata")
    .register_get("funcs", ModuleMetadata::funcs)
    .register_get("events", ModuleMetadata::events)
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
}

pub fn init_scope(url: &str, lookup: &TypeLookup, engine: &mut Engine, scope: &mut Scope<'_>) -> Result<Metadata, Box<EvalAltResult>> {
  let metadata = Metadata::new(url, lookup)?;
  scope.push_constant("METADATA", metadata.clone());

  lookup.custom_encode_type("Call", TypeId::of::<EncodedCall>(), |value, data| {
    let call = value.cast::<EncodedCall>();
    data.encode(call);
    Ok(())
  })?;

  // Register each module as a global constant.
  metadata.add_encode_calls(engine, scope)?;

  Ok(metadata)
}
