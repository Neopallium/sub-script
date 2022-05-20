use std::any::TypeId;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use frame_metadata::{
  RuntimeMetadata, RuntimeMetadataPrefixed,
};
#[cfg(any(
	feature = "v13",
	feature = "v12",
))]
use frame_metadata::decode_different::{
  DecodeDifferent, DecodeDifferentArray,
};
use frame_support::{
  Blake2_128, Blake2_128Concat, Blake2_256, StorageHasher as StorageHasherTrait, Twox128, Twox256,
  Twox64Concat,
};
#[cfg(feature = "v14")]
use scale_info::{
  form::PortableForm,
  PortableRegistry,
  TypeDef,
  Variant, Field,
};
use parity_scale_codec::{Encode, Output};
use sp_core::{self, storage::StorageKey};

use rhai::plugin::NativeCallContext;
use rhai::{Dynamic, Engine, EvalAltResult, FnPtr, Map as RMap, INT};

use crate::client::Client;
use crate::types::{EnumVariants, TypeLookup, TypeMeta, TypeRef};

#[cfg(feature = "v14")]
use crate::types::{get_type_name, is_type_compact};

#[cfg(any(
	feature = "v13",
	feature = "v12",
))]
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
  pub fn from_runtime_metadata(
    metadata_prefixed: RuntimeMetadataPrefixed,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    // Get versioned metadata.
    let md = match metadata_prefixed.1 {
      #[cfg(feature = "v12")]
      RuntimeMetadata::V12(v12) => {
        if metadata_prefixed.0 != frame_metadata::v12::META_RESERVED {
          return Err(format!("Invalid metadata prefix {}", metadata_prefixed.0).into());
        }

        Self::from_v12_metadata(v12, lookup)?
      },
      #[cfg(feature = "v13")]
      RuntimeMetadata::V13(v13) => {
        if metadata_prefixed.0 != frame_metadata::v13::META_RESERVED {
          return Err(format!("Invalid metadata prefix {}", metadata_prefixed.0).into());
        }

        Self::from_v13_metadata(v13, lookup)?
      }
      #[cfg(feature = "v14")]
      RuntimeMetadata::V14(v14) => {
        if metadata_prefixed.0 != frame_metadata::META_RESERVED {
          return Err(format!("Invalid metadata prefix {}", metadata_prefixed.0).into());
        }

        Self::from_v14_metadata(v14, lookup)?
      }
      _ => {
        return Err(format!("Unsupported metadata version").into());
      }
    };
    Ok(md)
  }

  #[cfg(feature = "v12")]
  fn from_v12_metadata(
    md: frame_metadata::v12::RuntimeMetadataV12,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    let mut api_md = Self {
      modules: HashMap::new(),
      idx_map: HashMap::new(),
    };

    // Top-level event/error/call types.
    let mut mod_events = EnumVariants::new();
    let mut mod_errors = EnumVariants::new();
    let mut mod_calls = EnumVariants::new();

    // Decode module metadata.
    decode_meta(&md.modules)?
      .iter()
      .try_for_each(|m| -> Result<(), Box<EvalAltResult>> {
        let m = ModuleMetadata::from_v12_meta(m, lookup)?;
        let name = m.name.clone();
        mod_events.insert_at(m.index, &name, m.event_ref.clone());
        mod_errors.insert_at(m.index, &name, m.error_ref.clone());
        mod_calls.insert_at(m.index, &name, m.call_ref.clone());
        api_md.idx_map.insert(m.index, name.clone());
        api_md.modules.insert(name, m);
        Ok(())
      })?;

    let raw_event_ref = lookup.insert_meta("RawEvent", TypeMeta::Enum(mod_events));
    lookup.insert("Event", raw_event_ref);
    let raw_error_ref = lookup.insert_meta("RawError", TypeMeta::Enum(mod_errors));
    lookup.insert("DispatchErrorModule", raw_error_ref);
    // Define 'RuntimeCall' type.
    lookup.insert_meta("RuntimeCall", TypeMeta::Enum(mod_calls));

    Ok(api_md)
  }

  #[cfg(feature = "v13")]
  fn from_v13_metadata(
    md: frame_metadata::v13::RuntimeMetadataV13,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    let mut api_md = Self {
      modules: HashMap::new(),
      idx_map: HashMap::new(),
    };

    // Top-level event/error/call types.
    let mut mod_events = EnumVariants::new();
    let mut mod_errors = EnumVariants::new();
    let mut mod_calls = EnumVariants::new();

    // Decode module metadata.
    decode_meta(&md.modules)?
      .iter()
      .try_for_each(|m| -> Result<(), Box<EvalAltResult>> {
        let m = ModuleMetadata::from_v13_meta(m, lookup)?;
        let name = m.name.clone();
        mod_events.insert_at(m.index, &name, m.event_ref.clone());
        mod_errors.insert_at(m.index, &name, m.error_ref.clone());
        mod_calls.insert_at(m.index, &name, m.call_ref.clone());
        api_md.idx_map.insert(m.index, name.clone());
        api_md.modules.insert(name, m);
        Ok(())
      })?;

    let raw_event_ref = lookup.insert_meta("RawEvent", TypeMeta::Enum(mod_events));
    lookup.insert("Event", raw_event_ref);
    let raw_error_ref = lookup.insert_meta("RawError", TypeMeta::Enum(mod_errors));
    lookup.insert("DispatchErrorModule", raw_error_ref);
    let call_ref = lookup.insert_meta("Call", TypeMeta::Enum(mod_calls));
    lookup.insert("Call", call_ref);

    Ok(api_md)
  }

  #[cfg(feature = "v14")]
  fn from_v14_metadata(
    md: frame_metadata::v14::RuntimeMetadataV14,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    let mut api_md = Self {
      modules: HashMap::new(),
      idx_map: HashMap::new(),
    };

    // Import types from registry.
    lookup.import_v14_types(&md.types)?;

    // Top-level event/error/call types.
    let mut mod_events = EnumVariants::new();
    let mut mod_errors = EnumVariants::new();
    let mut mod_calls = EnumVariants::new();

    // Decode module metadata.
    md.pallets
      .iter()
      .try_for_each(|m| -> Result<(), Box<EvalAltResult>> {
        let m = ModuleMetadata::from_v14_meta(m, &md.types, lookup)?;
        let name = m.name.clone();
        mod_events.insert_at(m.index, &name, m.event_ref.clone());
        mod_errors.insert_at(m.index, &name, m.error_ref.clone());
        mod_calls.insert_at(m.index, &name, m.call_ref.clone());
        api_md.idx_map.insert(m.index, name.clone());
        api_md.modules.insert(name, m);
        Ok(())
      })?;

    let raw_event_ref = lookup.insert_meta("RawEvent", TypeMeta::Enum(mod_events));
    lookup.insert("Event", raw_event_ref);
    let raw_error_ref = lookup.insert_meta("RawError", TypeMeta::Enum(mod_errors));
    lookup.insert("DispatchErrorModule", raw_error_ref);
    let call_ref = lookup.insert_meta("Call", TypeMeta::Enum(mod_calls));
    lookup.insert("Call", call_ref);

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

  pub fn get_storage(
    &self,
    module: &str,
    storage: &str,
  ) -> Result<&StorageMetadata, Box<EvalAltResult>> {
    Ok(
      self
        .get_module(module)
        .and_then(|m| m.get_storage(storage))
        .ok_or_else(|| format!("Can't find storage: {}.{}", module, storage))?,
    )
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
  call_ref: Option<TypeRef>,
}

impl ModuleMetadata {
  #[cfg(feature = "v12")]
  fn from_v12_meta(
    md: &frame_metadata::v12::ModuleMetadata,
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
      call_ref: None,
    };

    // Decode module functions.
    if let Some(calls) = &md.calls {
      // Module RawCall type.
      let mut raw_calls = EnumVariants::new();

      decode_meta(calls)?.iter().enumerate().try_for_each(
        |(func_idx, md)| -> Result<(), Box<EvalAltResult>> {
          let (func, ty_ref) = FuncMetadata::from_v12_meta(&mod_name, mod_idx, func_idx as u8, md, lookup)?;
          let name = func.name.clone();
          raw_calls.insert_at(func.func_idx, &name, ty_ref);
          module.funcs.insert(name, func);
          Ok(())
        },
      )?;
      module.call_ref = Some(lookup.insert_meta(
        &format!("{}::RawCall", mod_name),
        TypeMeta::Enum(raw_calls),
      ));
    }

    // Decode module storage.
    if let Some(storage) = &md.storage {
      let md = decode_meta(storage)?;
      let mod_prefix = decode_meta(&md.prefix)?;
      decode_meta(&md.entries)?
        .iter()
        .try_for_each(|md| -> Result<(), Box<EvalAltResult>> {
          let storage = StorageMetadata::from_v12_meta(mod_prefix, md, lookup)?;
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
            EventMetadata::from_v12_meta(&mod_name, mod_idx, event_idx as u8, md, lookup)?;
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
    decode_meta(&md.constants)?
      .iter()
      .try_for_each(|md| -> Result<(), Box<EvalAltResult>> {
        let constant = ConstMetadata::from_v12_meta(&mod_name, md, lookup)?;
        let name = constant.name.clone();
        module.constants.insert(name, constant);
        Ok(())
      })?;

    // Decode module errors.
    // Module RawError type.
    let mut raw_errors = EnumVariants::new();

    decode_meta(&md.errors)?.iter().enumerate().try_for_each(
      |(error_idx, md)| -> Result<(), Box<EvalAltResult>> {
        let error = ErrorMetadata::from_v12_meta(&mod_name, mod_idx, error_idx as u8, md)?;
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

  #[cfg(feature = "v13")]
  fn from_v13_meta(
    md: &frame_metadata::v13::ModuleMetadata,
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
      call_ref: None,
    };

    // Decode module functions.
    if let Some(calls) = &md.calls {
      // Module RawCall type.
      let mut raw_calls = EnumVariants::new();

      decode_meta(calls)?.iter().enumerate().try_for_each(
        |(func_idx, md)| -> Result<(), Box<EvalAltResult>> {
          let (func, ty_ref) = FuncMetadata::from_v13_meta(&mod_name, mod_idx, func_idx as u8, md, lookup)?;
          let name = func.name.clone();
          raw_calls.insert_at(func.func_idx, &name, ty_ref);
          module.funcs.insert(name, func);
          Ok(())
        },
      )?;
      module.call_ref = Some(lookup.insert_meta(
        &format!("{}::RawCall", mod_name),
        TypeMeta::Enum(raw_calls),
      ));
    }

    // Decode module storage.
    if let Some(storage) = &md.storage {
      let md = decode_meta(storage)?;
      let mod_prefix = decode_meta(&md.prefix)?;
      decode_meta(&md.entries)?
        .iter()
        .try_for_each(|md| -> Result<(), Box<EvalAltResult>> {
          let storage = StorageMetadata::from_v13_meta(mod_prefix, md, lookup)?;
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
            EventMetadata::from_v13_meta(&mod_name, mod_idx, event_idx as u8, md, lookup)?;
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
    decode_meta(&md.constants)?
      .iter()
      .try_for_each(|md| -> Result<(), Box<EvalAltResult>> {
        let constant = ConstMetadata::from_v13_meta(&mod_name, md, lookup)?;
        let name = constant.name.clone();
        module.constants.insert(name, constant);
        Ok(())
      })?;

    // Decode module errors.
    // Module RawError type.
    let mut raw_errors = EnumVariants::new();

    decode_meta(&md.errors)?.iter().enumerate().try_for_each(
      |(error_idx, md)| -> Result<(), Box<EvalAltResult>> {
        let error = ErrorMetadata::from_v13_meta(&mod_name, mod_idx, error_idx as u8, md)?;
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


  #[cfg(feature = "v14")]
  fn from_v14_meta(
    md: &frame_metadata::v14::PalletMetadata<PortableForm>,
    types: &PortableRegistry,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    let mod_idx = md.index;
    let mod_name = &md.name;
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
      call_ref: None,
    };

    // Decode module functions.
    if let Some(calls) = &md.calls {
      // Module RawCall type.
      let mut raw_calls = EnumVariants::new();

      let call_ty = types.resolve(calls.ty.id())
        .expect("Missing Pallet call type");
      match call_ty.type_def() {
        TypeDef::Variant(v) => {
          v.variants().iter().try_for_each(
            |md| -> Result<(), Box<EvalAltResult>> {
              let (func, ty_ref) = FuncMetadata::from_v14_meta(&mod_name, mod_idx, md, types, lookup)?;
              let name = func.name.clone();
              raw_calls.insert_at(func.func_idx, &name, ty_ref);
              module.funcs.insert(name, func);
              Ok(())
            },
          )?;
        }
        _ => {
          unimplemented!("Only Variant type supported for Pallet Call type.");
        }
      }
      module.call_ref = Some(lookup.insert_meta(
        &format!("{}::RawCall", mod_name),
        TypeMeta::Enum(raw_calls),
      ));
    }

    // Decode module storage.
    if let Some(storage) = &md.storage {
      let mod_prefix = &storage.prefix;
      storage.entries
        .iter()
        .try_for_each(|md| -> Result<(), Box<EvalAltResult>> {
          let storage = StorageMetadata::from_v14_meta(mod_prefix, md, types, lookup)?;
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

      let event_ty = types.resolve(events.ty.id())
        .expect("Missing Pallet event type");
      match event_ty.type_def() {
        TypeDef::Variant(v) => {
          v.variants().iter().try_for_each(
            |md| -> Result<(), Box<EvalAltResult>> {
              let (event, ty_ref) =
                EventMetadata::from_v14_meta(&mod_name, mod_idx, md, types, lookup)?;
              let name = event.name.clone();
              raw_events.insert_at(event.event_idx, &name, ty_ref);
              module.events.insert(name, event);
              Ok(())
            },
          )?;
        }
        _ => {
          unimplemented!("Only Variant type supported for Pallet Event type.");
        }
      }
      module.event_ref = Some(lookup.insert_meta(
        &format!("{}::RawEvent", mod_name),
        TypeMeta::Enum(raw_events),
      ));
    }

    // Decode module constants.
    md.constants
      .iter()
      .try_for_each(|md| -> Result<(), Box<EvalAltResult>> {
        let constant = ConstMetadata::from_v14_meta(&mod_name, md, types, lookup)?;
        let name = constant.name.clone();
        module.constants.insert(name, constant);
        Ok(())
      })?;

    // Decode module errors.
    if let Some(error) = &md.error {
      // Module RawError type.
      let mut raw_errors = EnumVariants::new();

      let extra_bytes = lookup.parse_type("[u8; 3]")?;
      let error_ty = types.resolve(error.ty.id())
        .expect("Missing Pallet error type");
      match error_ty.type_def() {
        TypeDef::Variant(v) => {
          v.variants().iter().try_for_each(
            |md| -> Result<(), Box<EvalAltResult>> {
              let error = ErrorMetadata::from_v14_meta(&mod_name, mod_idx, md)?;
              let name = error.name.clone();
              raw_errors.insert_at(error.error_idx, &name, Some(extra_bytes.clone()));
              module.err_idx_map.insert(error.error_idx, name.clone());
              module.errors.insert(name, error);
              Ok(())
            },
          )?;
        }
        _ => {
          unimplemented!("Only Variant type supported for Pallet Error type.");
        }
      }
      module.error_ref = Some(lookup.insert_meta(
        &format!("{}::RawError", mod_name),
        TypeMeta::Enum(raw_errors),
      ));
    }

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

  #[cfg(feature = "v14")]
  pub fn new_type(ty_id: u32, types: &PortableRegistry, lookup: &TypeLookup) -> Result<Self, Box<EvalAltResult>> {
    let ty = types.resolve(ty_id)
      .ok_or_else(|| format!("Failed to resolve type."))?;
    let name = get_type_name(ty, types, false);
    let ty_meta = lookup.parse_type(&name)?;
    let named = Self {
      name: name.into(),
      ty_meta,
    };

    Ok(named)
  }

  #[cfg(feature = "v14")]
  pub fn new_field_type(md: &Field<PortableForm>, types: &PortableRegistry, lookup: &TypeLookup) -> Result<Self, Box<EvalAltResult>> {
    let ty = types.resolve(md.ty().id())
      .ok_or_else(|| format!("Failed to resolve type."))?;
    //let name = get_type_name(ty, types);
    let name = md.type_name().map(|ty_name| {
        // Trim junk from `type_name`.
        let name = if ty_name.starts_with("/*«*/") {
          let end = ty_name.len() - 6;
          &ty_name[6..end]
        } else {
          &ty_name[..]
        };
        if is_type_compact(ty) {
          format!("Compact<{}>", name)
        } else {
          name.to_string()
        }
      }).unwrap_or_else(|| {
        get_type_name(ty, types, false)
      });
    let ty_meta = lookup.parse_type(&name)?;
    let named = Self {
      name: name.into(),
      ty_meta,
    };

    Ok(named)
  }

  pub fn encode_value(
    &self,
    param: Dynamic,
    data: &mut EncodedArgs,
  ) -> Result<(), Box<EvalAltResult>> {
    self.ty_meta.encode_value(param, data)
  }

  fn encode(&self, param: Dynamic) -> Result<Vec<u8>, Box<EvalAltResult>> {
    let mut data = EncodedArgs::new();
    self.ty_meta.encode_value(param, &mut data)?;
    Ok(data.into_inner())
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

#[derive(Debug, Clone, Copy)]
pub enum KeyHasherType {
	Blake2_128,
	Blake2_256,
	Blake2_128Concat,
	Twox128,
	Twox256,
	Twox64Concat,
	Identity,
}

#[cfg(feature = "v12")]
impl From<&frame_metadata::v12::StorageHasher> for KeyHasherType {
  fn from(hasher: &frame_metadata::v12::StorageHasher) -> Self {
    use frame_metadata::v12::StorageHasher;
    match hasher {
      StorageHasher::Blake2_128 => Self::Blake2_128,
      StorageHasher::Blake2_256 => Self::Blake2_256,
      StorageHasher::Blake2_128Concat => Self::Blake2_128Concat,
      StorageHasher::Twox128 => Self::Twox128,
      StorageHasher::Twox256 => Self::Twox256,
      StorageHasher::Twox64Concat => Self::Twox64Concat,
      StorageHasher::Identity => Self::Identity,
    }
  }
}

#[cfg(feature = "v13")]
impl From<&frame_metadata::v13::StorageHasher> for KeyHasherType {
  fn from(hasher: &frame_metadata::v13::StorageHasher) -> Self {
    use frame_metadata::v13::StorageHasher;
    match hasher {
      StorageHasher::Blake2_128 => Self::Blake2_128,
      StorageHasher::Blake2_256 => Self::Blake2_256,
      StorageHasher::Blake2_128Concat => Self::Blake2_128Concat,
      StorageHasher::Twox128 => Self::Twox128,
      StorageHasher::Twox256 => Self::Twox256,
      StorageHasher::Twox64Concat => Self::Twox64Concat,
      StorageHasher::Identity => Self::Identity,
    }
  }
}

#[cfg(feature = "v14")]
impl From<&frame_metadata::v14::StorageHasher> for KeyHasherType {
  fn from(hasher: &frame_metadata::v14::StorageHasher) -> Self {
    use frame_metadata::v14::StorageHasher;
    match hasher {
      StorageHasher::Blake2_128 => Self::Blake2_128,
      StorageHasher::Blake2_256 => Self::Blake2_256,
      StorageHasher::Blake2_128Concat => Self::Blake2_128Concat,
      StorageHasher::Twox128 => Self::Twox128,
      StorageHasher::Twox256 => Self::Twox256,
      StorageHasher::Twox64Concat => Self::Twox64Concat,
      StorageHasher::Identity => Self::Identity,
    }
  }
}

#[derive(Debug, Clone)]
pub struct KeyHasher {
  pub type_hashers: Vec<(NamedType, KeyHasherType)>,
}

impl KeyHasher {
  pub fn encode_map_key(&self, key: Dynamic) -> Result<Vec<u8>, Box<EvalAltResult>> {
    let mut buf = EncodedArgs::new();
    match self.type_hashers.len() {
      0 => Err(format!("This storage isn't a map type."))?,
      1 => {
        let (ty, _) = &self.type_hashers[0];
        ty.encode_value(key, &mut buf)?;
      }
      _ => {
        Err(format!("This storage isn't a double map type."))?;
      }
    }
    Ok(buf.into_inner())
  }

  pub fn encode_double_map_key(
    &self,
    key1: Dynamic,
    key2: Dynamic,
  ) -> Result<(Vec<u8>, Vec<u8>), Box<EvalAltResult>> {
    let mut buf1 = EncodedArgs::new();
    let mut buf2 = EncodedArgs::new();
    match self.type_hashers.len() {
      2 => {
        let (ty, _) = &self.type_hashers[0];
        ty.encode_value(key1, &mut buf1)?;
        let (ty, _) = &self.type_hashers[1];
        ty.encode_value(key2, &mut buf2)?;
      }
      _ => Err(format!("This storage isn't a double map type."))?,
    }
    Ok((buf1.into_inner(), buf2.into_inner()))
  }

  fn hash_key(
    &self,
    buf: &mut Vec<u8>,
    idx: usize,
    key: Dynamic,
  ) -> Result<(), Box<EvalAltResult>> {
    let (ty, _) = &self.type_hashers[idx];
    let key = ty.encode(key)?;
    self.raw_hash_key(buf, idx, key)
  }

  fn raw_hash_key(
    &self,
    buf: &mut Vec<u8>,
    idx: usize,
    key: Vec<u8>,
  ) -> Result<(), Box<EvalAltResult>> {
    let (_, hasher) = &self.type_hashers[idx];
    match hasher {
      KeyHasherType::Blake2_128 => {
        buf.extend(Blake2_128::hash(&key));
      }
      KeyHasherType::Blake2_256 => {
        buf.extend(Blake2_256::hash(&key));
      }
      KeyHasherType::Blake2_128Concat => {
        buf.extend(Blake2_128Concat::hash(&key));
      }
      KeyHasherType::Twox128 => {
        buf.extend(Twox128::hash(&key));
      }
      KeyHasherType::Twox256 => {
        buf.extend(Twox256::hash(&key));
      }
      KeyHasherType::Twox64Concat => {
        buf.extend(Twox64Concat::hash(&key));
      }
      KeyHasherType::Identity => {
        buf.extend(&key);
      }
    }
    Ok(())
  }

  pub fn get_map_key(
    &self,
    mut buf: Vec<u8>,
    key: Dynamic,
  ) -> Result<StorageKey, Box<EvalAltResult>> {
    match self.type_hashers.len() {
      0 => Err(format!("This storage isn't a map type."))?,
      1 => {
        self.hash_key(&mut buf, 0, key)?;
      }
      _ => {
        Err(format!("This storage isn't a double map type."))?;
      }
    }
    Ok(StorageKey(buf))
  }

  pub fn get_double_map_key(
    &self,
    mut buf: Vec<u8>,
    key1: Dynamic,
    key2: Dynamic,
  ) -> Result<StorageKey, Box<EvalAltResult>> {
    match self.type_hashers.len() {
      2 => {
        self.hash_key(&mut buf, 0, key1)?;
        self.hash_key(&mut buf, 1, key2)?;
      }
      _ => Err(format!("This storage isn't a double map type."))?,
    }
    Ok(StorageKey(buf))
  }

  pub fn get_double_map_prefix(
    &self,
    mut buf: Vec<u8>,
    key1: Dynamic,
  ) -> Result<StorageKey, Box<EvalAltResult>> {
    match self.type_hashers.len() {
      2 => {
        if !key1.is::<()>() {
          self.hash_key(&mut buf, 0, key1)?;
        }
      }
      _ => Err(format!("This storage isn't a double map type."))?,
    }
    Ok(StorageKey(buf))
  }

  pub fn raw_map_key(
    &self,
    mut buf: Vec<u8>,
    key: Vec<u8>,
  ) -> Result<StorageKey, Box<EvalAltResult>> {
    match self.type_hashers.len() {
      0 => Err(format!("This storage isn't a map type."))?,
      1 => {
        self.raw_hash_key(&mut buf, 0, key)?;
      }
      _ => {
        Err(format!("This storage isn't a double map type."))?;
      }
    }
    Ok(StorageKey(buf))
  }

  pub fn raw_double_map_key(
    &self,
    mut buf: Vec<u8>,
    key1: Vec<u8>,
    key2: Vec<u8>,
  ) -> Result<StorageKey, Box<EvalAltResult>> {
    match self.type_hashers.len() {
      2 => {
        self.raw_hash_key(&mut buf, 0, key1)?;
        self.raw_hash_key(&mut buf, 1, key2)?;
      }
      _ => Err(format!("This storage isn't a double map type."))?,
    }
    Ok(StorageKey(buf))
  }

  fn hasher_name(&mut self) -> String {
    let hashers = self
      .type_hashers
      .iter_mut()
      .map(|(t, h)| {
        format!("{}: {:?}", t.get_name(), h)
      })
      .collect::<Vec<String>>()
      .join(", ");
    format!("Hasher: {}", hashers)
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
  #[cfg(feature = "v12")]
  fn from_v12_meta(
    prefix: &str,
    md: &frame_metadata::v12::StorageEntryMetadata,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    use frame_metadata::v12::StorageEntryType;
    let (key_hasher, value) = match &md.ty {
      StorageEntryType::Plain(value) => (None, value.clone()),
      StorageEntryType::Map {
        hasher, key, value, ..
      } => {
        let ty = NamedType::new(decode_meta(key)?, lookup)?;
        let hasher = KeyHasher {
          type_hashers: vec![(ty, hasher.into())],
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
        let ty1 = NamedType::new(decode_meta(key1)?, lookup)?;
        let ty2 = NamedType::new(decode_meta(key2)?, lookup)?;
        let hasher = KeyHasher {
          type_hashers: vec![(ty1, hasher.into()), (ty2, key2_hasher.into())],
        };
        (Some(hasher), value.clone())
      }
    };
    let storage = Self {
      prefix: prefix.into(),
      name: decode_meta(&md.name)?.clone(),
      key_hasher,
      value_ty: NamedType::new(decode_meta(&value)?, lookup)?,
      docs: Docs::from_v12_meta(&md.documentation)?,
    };

    Ok(storage)
  }

  #[cfg(feature = "v13")]
  fn from_v13_meta(
    prefix: &str,
    md: &frame_metadata::v13::StorageEntryMetadata,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    use frame_metadata::v13::StorageEntryType;
    let (key_hasher, value) = match &md.ty {
      StorageEntryType::Plain(value) => (None, value.clone()),
      StorageEntryType::Map {
        hasher, key, value, ..
      } => {
        let ty = NamedType::new(decode_meta(key)?, lookup)?;
        let hasher = KeyHasher {
          type_hashers: vec![(ty, hasher.into())],
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
        let ty1 = NamedType::new(decode_meta(key1)?, lookup)?;
        let ty2 = NamedType::new(decode_meta(key2)?, lookup)?;
        let hasher = KeyHasher {
          type_hashers: vec![(ty1, hasher.into()), (ty2, key2_hasher.into())],
        };
        (Some(hasher), value.clone())
      }
      StorageEntryType::NMap { hashers, keys, value } => {
        let type_hashers = decode_meta(keys)?.iter()
          .zip(decode_meta(hashers)?.iter())
          .map(|(key, hasher)| {
            let ty = NamedType::new(key, lookup)?;
            Ok((ty, hasher.into()))
          })
          .collect::<Result<Vec<_>, Box<EvalAltResult>>>()?;
        let hasher = KeyHasher {
          type_hashers,
        };
        (Some(hasher), value.clone())
      }
    };
    let storage = Self {
      prefix: prefix.into(),
      name: decode_meta(&md.name)?.clone(),
      key_hasher,
      value_ty: NamedType::new(decode_meta(&value)?, lookup)?,
      docs: Docs::from_v13_meta(&md.documentation)?,
    };

    Ok(storage)
  }

  #[cfg(feature = "v14")]
  fn from_v14_meta(
    prefix: &str,
    md: &frame_metadata::v14::StorageEntryMetadata<PortableForm>,
    types: &PortableRegistry,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    use frame_metadata::v14::StorageEntryType;
    let (key_hasher, value) = match &md.ty {
      StorageEntryType::Plain(value) => (None, value.clone()),
      StorageEntryType::Map {
        hashers, key, value, ..
      } => {
        match hashers.as_slice() {
          [hasher] => {
            let ty = NamedType::new_type(key.id(), types, lookup)?;
            let hasher = KeyHasher {
              type_hashers: vec![(ty, hasher.into())],
            };
            (Some(hasher), value.clone())
          }
          hashers => {
            let ty = NamedType::new_type(key.id(), types, lookup)?;
            let hasher = KeyHasher {
              type_hashers: hashers.iter()
                .map(|hasher| (ty.clone(), hasher.into())).collect(),
            };
            (Some(hasher), value.clone())
          }
        }
      }
    };
    let storage = Self {
      prefix: prefix.into(),
      name: md.name.to_string(),
      key_hasher,
      value_ty: NamedType::new_type(value.id(), types, lookup)?,
      docs: Docs::from_v14_meta(md.docs.as_slice()),
    };

    Ok(storage)
  }

  fn get_prefix_key(&self) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(512);
    bytes.extend(sp_core::twox_128(self.prefix.as_bytes()));
    bytes.extend(sp_core::twox_128(self.name.as_bytes()));
    bytes
  }

  pub fn get_value_key(&self) -> Result<StorageKey, Box<EvalAltResult>> {
    match &self.key_hasher {
      Some(_) => Err(format!("This storage type expected key(s).").into()),
      None => Ok(StorageKey(self.get_prefix_key())),
    }
  }

  pub fn get_map_key(&self, key: Dynamic) -> Result<StorageKey, Box<EvalAltResult>> {
    match &self.key_hasher {
      Some(hasher) => {
        let prefix = self.get_prefix_key();
        hasher.get_map_key(prefix, key)
      }
      None => Err(format!("This storage type doesn't have keys.").into()),
    }
  }

  pub fn get_double_map_key(
    &self,
    key1: Dynamic,
    key2: Dynamic,
  ) -> Result<StorageKey, Box<EvalAltResult>> {
    match &self.key_hasher {
      Some(hasher) => {
        let prefix = self.get_prefix_key();
        hasher.get_double_map_key(prefix, key1, key2)
      }
      None => Err(format!("This storage type doesn't have keys.").into()),
    }
  }

  pub fn get_map_prefix(&self) -> Result<StorageKey, Box<EvalAltResult>> {
    match &self.key_hasher {
      Some(_hasher) => {
        let prefix = self.get_prefix_key();
        Ok(StorageKey(prefix))
      }
      None => Err(format!("This storage type doesn't have keys.").into()),
    }
  }

  pub fn get_double_map_prefix(&self, key1: Dynamic) -> Result<StorageKey, Box<EvalAltResult>> {
    match &self.key_hasher {
      Some(hasher) => {
        let prefix = self.get_prefix_key();
        hasher.get_double_map_prefix(prefix, key1)
      }
      None => Err(format!("This storage type doesn't have keys.").into()),
    }
  }

  pub fn raw_map_key(&self, key: Vec<u8>) -> Result<StorageKey, Box<EvalAltResult>> {
    match &self.key_hasher {
      Some(hasher) => {
        let prefix = self.get_prefix_key();
        hasher.raw_map_key(prefix, key)
      }
      None => Err(format!("This storage type doesn't have keys.").into()),
    }
  }

  pub fn raw_double_map_key(
    &self,
    key1: Vec<u8>,
    key2: Vec<u8>,
  ) -> Result<StorageKey, Box<EvalAltResult>> {
    match &self.key_hasher {
      Some(hasher) => {
        let prefix = self.get_prefix_key();
        hasher.raw_double_map_key(prefix, key1, key2)
      }
      None => Err(format!("This storage type doesn't have keys.").into()),
    }
  }

  pub fn encode_map_key(&self, key: Dynamic) -> Result<Vec<u8>, Box<EvalAltResult>> {
    match &self.key_hasher {
      Some(hasher) => hasher.encode_map_key(key),
      None => Err(format!("This storage type doesn't have keys.").into()),
    }
  }

  pub fn encode_double_map_key(
    &self,
    key1: Dynamic,
    key2: Dynamic,
  ) -> Result<(Vec<u8>, Vec<u8>), Box<EvalAltResult>> {
    match &self.key_hasher {
      Some(hasher) => hasher.encode_double_map_key(key1, key2),
      None => Err(format!("This storage type doesn't have keys.").into()),
    }
  }

  pub fn decode_value(&self, data: Vec<u8>) -> Result<Dynamic, Box<EvalAltResult>> {
    self.value_ty.decode(data)
  }

  fn name(&mut self) -> String {
    self.name.clone()
  }

  fn hasher_name(&mut self) -> String {
    match &mut self.key_hasher {
      Some(key_hasher) => key_hasher.hasher_name(),
      None => format!("None"),
    }
  }

  fn value_type_name(&mut self) -> String {
    self.value_ty.get_name()
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
  #[cfg(feature = "v12")]
  fn from_v12_meta(
    mod_name: &str,
    _mod_idx: u8,
    event_idx: u8,
    md: &frame_metadata::v12::EventMetadata,
    lookup: &TypeLookup,
  ) -> Result<(Self, Option<TypeRef>), Box<EvalAltResult>> {
    let mut event = Self {
      mod_name: mod_name.into(),
      name: decode_meta(&md.name)?.clone(),
      event_idx,
      args: Vec::new(),
      docs: Docs::from_v12_meta(&md.documentation)?,
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

  #[cfg(feature = "v13")]
  fn from_v13_meta(
    mod_name: &str,
    _mod_idx: u8,
    event_idx: u8,
    md: &frame_metadata::v13::EventMetadata,
    lookup: &TypeLookup,
  ) -> Result<(Self, Option<TypeRef>), Box<EvalAltResult>> {
    let mut event = Self {
      mod_name: mod_name.into(),
      name: decode_meta(&md.name)?.clone(),
      event_idx,
      args: Vec::new(),
      docs: Docs::from_v13_meta(&md.documentation)?,
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

  #[cfg(feature = "v14")]
  fn from_v14_meta(
    mod_name: &str,
    _mod_idx: u8,
    md: &Variant<PortableForm>,
    types: &PortableRegistry,
    lookup: &TypeLookup,
  ) -> Result<(Self, Option<TypeRef>), Box<EvalAltResult>> {
    let mut event = Self {
      mod_name: mod_name.into(),
      name: md.name().clone(),
      event_idx: md.index(),
      args: Vec::new(),
      docs: Docs::from_v14_meta(&md.docs()),
    };

    let mut event_tuple = Vec::new();

    // Decode event arguments.
    md.fields()
      .iter()
      .try_for_each(|md| -> Result<(), Box<EvalAltResult>> {
        let arg = NamedType::new_field_type(md, types, lookup)?;
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
  #[cfg(feature = "v12")]
  fn from_v12_meta(
    mod_name: &str,
    md: &frame_metadata::v12::ModuleConstantMetadata,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    let ty = decode_meta(&md.ty)?;
    let const_ty = NamedType::new(ty, lookup)?;
    Ok(Self {
      mod_name: mod_name.into(),
      name: decode_meta(&md.name)?.clone(),
      const_ty,
      docs: Docs::from_v12_meta(&md.documentation)?,
    })
  }

  #[cfg(feature = "v13")]
  fn from_v13_meta(
    mod_name: &str,
    md: &frame_metadata::v13::ModuleConstantMetadata,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    let ty = decode_meta(&md.ty)?;
    let const_ty = NamedType::new(ty, lookup)?;
    Ok(Self {
      mod_name: mod_name.into(),
      name: decode_meta(&md.name)?.clone(),
      const_ty,
      docs: Docs::from_v13_meta(&md.documentation)?,
    })
  }

  #[cfg(feature = "v14")]
  fn from_v14_meta(
    mod_name: &str,
    md: &frame_metadata::v14::PalletConstantMetadata<PortableForm>,
    types: &PortableRegistry,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    let const_ty = NamedType::new_type(md.ty.id(), types, lookup)?;
    Ok(Self {
      mod_name: mod_name.into(),
      name: md.name.clone(),
      const_ty,
      docs: Docs::from_v14_meta(&md.docs),
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
  #[cfg(feature = "v12")]
  fn from_v12_meta(
    mod_name: &str,
    _mod_idx: u8,
    error_idx: u8,
    md: &frame_metadata::v12::ErrorMetadata,
  ) -> Result<Self, Box<EvalAltResult>> {
    Ok(Self {
      mod_name: mod_name.into(),
      name: decode_meta(&md.name)?.clone(),
      error_idx,
      docs: Docs::from_v12_meta(&md.documentation)?,
    })
  }

  #[cfg(feature = "v13")]
  fn from_v13_meta(
    mod_name: &str,
    _mod_idx: u8,
    error_idx: u8,
    md: &frame_metadata::v13::ErrorMetadata,
  ) -> Result<Self, Box<EvalAltResult>> {
    Ok(Self {
      mod_name: mod_name.into(),
      name: decode_meta(&md.name)?.clone(),
      error_idx,
      docs: Docs::from_v13_meta(&md.documentation)?,
    })
  }

  #[cfg(feature = "v14")]
  fn from_v14_meta(
    mod_name: &str,
    _mod_idx: u8,
    md: &Variant<PortableForm>,
  ) -> Result<Self, Box<EvalAltResult>> {
    Ok(Self {
      mod_name: mod_name.into(),
      name: md.name().clone(),
      error_idx: md.index(),
      docs: Docs::from_v14_meta(&md.docs()),
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
  #[cfg(feature = "v12")]
  fn from_v12_meta(
    mod_name: &str,
    mod_idx: u8,
    func_idx: u8,
    md: &frame_metadata::v12::FunctionMetadata,
    lookup: &TypeLookup,
  ) -> Result<(Self, Option<TypeRef>), Box<EvalAltResult>> {
    let mut func = Self {
      mod_name: mod_name.into(),
      name: decode_meta(&md.name)?.clone(),
      mod_idx,
      func_idx,
      args: Vec::new(),
      docs: Docs::from_v12_meta(&md.documentation)?,
    };

    let mut func_tuple = Vec::new();

    // Decode function arguments.
    decode_meta(&md.arguments)?
      .iter()
      .try_for_each(|md| -> Result<(), Box<EvalAltResult>> {
        let arg = FuncArg::from_v12_meta(md, lookup)?;
        func_tuple.push(arg.ty.ty_meta.clone());
        func.args.push(arg);
        Ok(())
      })?;

    let func_ref = if func_tuple.len() > 0 {
      let type_name = format!("{}::RawFunc::{}", mod_name, func.name);
      Some(lookup.insert_meta(&type_name, TypeMeta::Tuple(func_tuple)))
    } else {
      None
    };

    Ok((func, func_ref))
  }

  #[cfg(feature = "v13")]
  fn from_v13_meta(
    mod_name: &str,
    mod_idx: u8,
    func_idx: u8,
    md: &frame_metadata::v13::FunctionMetadata,
    lookup: &TypeLookup,
  ) -> Result<(Self, Option<TypeRef>), Box<EvalAltResult>> {
    let mut func = Self {
      mod_name: mod_name.into(),
      name: decode_meta(&md.name)?.clone(),
      mod_idx,
      func_idx,
      args: Vec::new(),
      docs: Docs::from_v13_meta(&md.documentation)?,
    };

    let mut func_tuple = Vec::new();

    // Decode function arguments.
    decode_meta(&md.arguments)?
      .iter()
      .try_for_each(|md| -> Result<(), Box<EvalAltResult>> {
        let arg = FuncArg::from_v13_meta(md, lookup)?;
        func_tuple.push(arg.ty.ty_meta.clone());
        func.args.push(arg);
        Ok(())
      })?;

    let func_ref = if func_tuple.len() > 0 {
      let type_name = format!("{}::RawFunc::{}", mod_name, func.name);
      Some(lookup.insert_meta(&type_name, TypeMeta::Tuple(func_tuple)))
    } else {
      None
    };

    Ok((func, func_ref))
  }

  #[cfg(feature = "v14")]
  fn from_v14_meta(
    mod_name: &str,
    mod_idx: u8,
    md: &Variant<PortableForm>,
    types: &PortableRegistry,
    lookup: &TypeLookup,
  ) -> Result<(Self, Option<TypeRef>), Box<EvalAltResult>> {
    let mut func = Self {
      mod_name: mod_name.into(),
      name: md.name().clone(),
      mod_idx,
      func_idx: md.index(),
      args: Vec::new(),
      docs: Docs::from_v14_meta(&md.docs()),
    };

    let mut func_tuple = Vec::new();

    // Decode function arguments.
    md.fields()
      .iter()
      .try_for_each(|md| -> Result<(), Box<EvalAltResult>> {
        let arg = FuncArg::from_v14_meta(md, types, lookup)?;
        func_tuple.push(arg.ty.ty_meta.clone());
        func.args.push(arg);
        Ok(())
      })?;

    let func_ref = if func_tuple.len() > 0 {
      let type_name = format!("{}::RawFunc::{}", mod_name, func.name);
      Some(lookup.insert_meta(&type_name, TypeMeta::Tuple(func_tuple)))
    } else {
      None
    };

    Ok((func, func_ref))
  }

  pub fn add_encode_calls(&self, engine: &mut Engine) -> Result<Dynamic, Box<EvalAltResult>> {
    let full_name = format!("{}_{}", self.mod_name, self.name);
    let mut args = vec![TypeId::of::<RMap>(), TypeId::of::<FuncMetadata>()];
    let args_len = self.args.len();
    if args_len > 0 {
      args.extend([TypeId::of::<Dynamic>()].repeat(args_len));
    }
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

  fn index(&mut self) -> INT {
    self.func_idx as INT
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
  #[cfg(feature = "v12")]
  fn from_v12_meta(
    md: &frame_metadata::v12::FunctionArgumentMetadata,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    let arg = Self {
      name: decode_meta(&md.name)?.clone(),
      ty: NamedType::new(decode_meta(&md.ty)?, lookup)?,
    };

    Ok(arg)
  }

  #[cfg(feature = "v13")]
  fn from_v13_meta(
    md: &frame_metadata::v13::FunctionArgumentMetadata,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    let arg = Self {
      name: decode_meta(&md.name)?.clone(),
      ty: NamedType::new(decode_meta(&md.ty)?, lookup)?,
    };

    Ok(arg)
  }

  #[cfg(feature = "v14")]
  fn from_v14_meta(
    md: &Field<PortableForm>,
    types: &PortableRegistry,
    lookup: &TypeLookup,
  ) -> Result<Self, Box<EvalAltResult>> {
    let arg = Self {
      name: md.name().cloned().unwrap_or_default(),
      ty: NamedType::new_field_type(md, types, lookup)?,
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
  #[cfg(feature = "v12")]
  fn from_v12_meta(
    md: &DecodeDifferentArray<&'static str, String>,
  ) -> Result<Self, Box<EvalAltResult>> {
    Ok(Self {
      lines: decode_meta(md)?.clone(),
    })
  }

  #[cfg(feature = "v13")]
  fn from_v13_meta(
    md: &DecodeDifferentArray<&'static str, String>,
  ) -> Result<Self, Box<EvalAltResult>> {
    Ok(Self {
      lines: decode_meta(md)?.clone(),
    })
  }

  #[cfg(feature = "v14")]
  fn from_v14_meta(
    docs: &[String],
  ) -> Self {
    Self {
      lines: docs.to_vec(),
    }
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
    .register_get("name", StorageMetadata::name)
    .register_get("value_type_name", StorageMetadata::value_type_name)
    .register_get("hasher_name", StorageMetadata::hasher_name)
    .register_get("title", StorageMetadata::title)
    .register_get("docs", StorageMetadata::docs)
    .register_type_with_name::<FuncMetadata>("FuncMetadata")
    .register_fn("to_string", FuncMetadata::to_string)
    .register_get("args", FuncMetadata::args)
    .register_get("index", FuncMetadata::index)
    .register_get("name", FuncMetadata::name)
    .register_get("title", FuncMetadata::title)
    .register_get("docs", FuncMetadata::docs)
    .register_type_with_name::<FuncArg>("FuncArg")
    .register_fn("to_string", FuncArg::to_string)
    .register_get("name", FuncArg::get_name)
    .register_get("type", FuncArg::get_type)
    .register_get("meta", FuncArg::get_meta)
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
    .register_fn("to_string", ErrorMetadata::to_string)
    .register_get("name", ErrorMetadata::name)
    .register_get("index", ErrorMetadata::index)
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
    .register_fn("encode", |call: &mut EncodedCall| call.encode())
    .register_type_with_name::<Docs>("Docs")
    .register_fn("to_string", Docs::to_string)
    .register_get("title", Docs::title);

  let metadata = client.get_metadata();

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
