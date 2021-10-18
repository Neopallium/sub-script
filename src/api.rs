use std::collections::HashMap;

use frame_metadata::{
  DecodeDifferent, DecodeDifferentArray, FunctionArgumentMetadata, FunctionMetadata,
  ModuleMetadata, RuntimeMetadata, RuntimeMetadataPrefixed, StorageEntryMetadata, META_RESERVED,
};

use rhai::{Dynamic, Engine, EvalAltResult};

fn decode<B: 'static, O: 'static>(
  encoded: &DecodeDifferent<B, O>,
) -> Result<&O, Box<EvalAltResult>> {
  match encoded {
    DecodeDifferent::Decoded(val) => Ok(val),
    _ => Err(format!("Failed to decode value.").into()),
  }
}

#[derive(Clone)]
pub struct ApiMetadata {
  url: String,
  modules: HashMap<String, Module>,
}

impl ApiMetadata {
  pub fn new(url: &str) -> Result<Self, Box<EvalAltResult>> {
    let client = crate::client::Client::connect(url)?;

    // Get runtime metadata.
    let metadata_prefixed = client.get_metadata()?;

    Self::decode(url, metadata_prefixed)
  }

  pub fn decode(
    url: &str,
    metadata_prefixed: RuntimeMetadataPrefixed,
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
      url: url.into(),
      modules: HashMap::new(),
    };

    // Decode module metadata.
    decode(&md.modules)?
      .iter()
      .try_for_each(|m| -> Result<(), Box<EvalAltResult>> {
        let m = Module::decode(m)?;
        let name = m.name.clone();
        api_md.modules.insert(name, m);
        Ok(())
      })?;

    Ok(api_md)
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
pub struct Module {
  name: String,
  index: u8,
  storage_prefix: String,
  storage: HashMap<String, Storage>,
  funcs: HashMap<String, ModuleFunc>,
}

impl Module {
  fn decode(md: &ModuleMetadata) -> Result<Self, Box<EvalAltResult>> {
    let mod_idx = md.index;
    let mod_name = decode(&md.name)?;
    let mut module = Self {
      name: mod_name.clone(),
      index: mod_idx,
      storage_prefix: "".into(),
      storage: HashMap::new(),
      funcs: HashMap::new(),
    };

    // Decode module functions.
    if let Some(calls) = &md.calls {
      decode(calls)?.iter().enumerate().try_for_each(
        |(func_idx, md)| -> Result<(), Box<EvalAltResult>> {
          let func = ModuleFunc::decode(&mod_name, mod_idx, func_idx as u8, md)?;
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
          let storage = Storage::decode(mod_prefix, md)?;
          let name = storage.name.clone();
          module.storage.insert(name, storage);
          Ok(())
        })?;
      module.storage_prefix = mod_prefix.into();
    }

    Ok(module)
  }

  fn funcs(&mut self) -> Vec<Dynamic> {
    self.funcs.values().cloned().map(Dynamic::from).collect()
  }

  fn to_string(&mut self) -> String {
    format!("Module: {}", self.name)
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

#[derive(Clone)]
pub struct Storage {
  prefix: String,
  name: String,
  docs: Docs,
}

impl Storage {
  fn decode(prefix: &str, md: &StorageEntryMetadata) -> Result<Self, Box<EvalAltResult>> {
    let storage = Self {
      prefix: prefix.into(),
      name: decode(&md.name)?.clone(),
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
    format!("Storage: {}", self.name)
  }
}

#[derive(Clone)]
pub struct ModuleFunc {
  mod_name: String,
  name: String,
  mod_idx: u8,
  func_idx: u8,
  args: Vec<FuncArg>,
  docs: Docs,
}

impl ModuleFunc {
  fn decode(
    mod_name: &str,
    mod_idx: u8,
    func_idx: u8,
    md: &FunctionMetadata,
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
        let arg = FuncArg::decode(md)?;
        func.args.push(arg);
        Ok(())
      })?;

    Ok(func)
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
    format!("Func: {}.{}({})", self.mod_name, self.name, args)
  }
}

#[derive(Clone)]
pub struct FuncArg {
  name: String,
  ty: String,
}

impl FuncArg {
  fn decode(md: &FunctionArgumentMetadata) -> Result<Self, Box<EvalAltResult>> {
    let arg = Self {
      name: decode(&md.name)?.clone(),
      ty: decode(&md.ty)?.clone(),
    };

    Ok(arg)
  }

  fn to_string(&mut self) -> String {
    format!("{}: {}", self.name, self.ty)
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

pub fn init_engine(url: &str, engine: &mut Engine) -> Dynamic {
  let lookup = ApiMetadata::new(url).expect("Failed to load API");

  engine
    .register_type_with_name::<ApiMetadata>("ApiMetadata")
    .register_get("modules", ApiMetadata::modules)
    .register_indexer_get_result(ApiMetadata::indexer_get)
    .register_type_with_name::<Module>("Module")
    .register_get("funcs", Module::funcs)
    .register_fn("to_string", Module::to_string)
    .register_indexer_get_result(Module::indexer_get)
    .register_type_with_name::<Storage>("ModuleStorage")
    .register_fn("to_string", Storage::to_string)
    .register_get("title", Storage::title)
    .register_get("docs", Storage::docs)
    .register_type_with_name::<ModuleFunc>("ModuleFunc")
    .register_fn("to_string", ModuleFunc::to_string)
    .register_get("title", ModuleFunc::title)
    .register_get("docs", ModuleFunc::docs)
    .register_type_with_name::<FuncArg>("FuncArg")
    .register_fn("to_string", FuncArg::to_string)
    .register_type_with_name::<Docs>("Docs")
    .register_fn("to_string", Docs::to_string)
    .register_get("title", Docs::title);

  Dynamic::from(lookup).into_shared()
}
