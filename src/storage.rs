use rhai::{Dynamic, Engine, EvalAltResult};

use crate::client::Client;
use crate::metadata::*;

#[derive(Clone)]
pub struct Storage {
  client: Client,
  metadata: Metadata,
}

impl Storage {
  pub fn new(client: Client, metadata: &Metadata) -> Self {
    Self {
      client,
      metadata: metadata.clone(),
    }
  }

  pub fn get_value(&mut self, mod_name: &str, storage_name: &str) -> Result<Dynamic, Box<EvalAltResult>> {
    let md = self.metadata.get_module(mod_name)
      .and_then(|m| m.get_storage(storage_name))
      .ok_or_else(|| format!("Can't find storage: {}.{}", mod_name, storage_name))?;

    match self.client.get_storage_value(&md.prefix, &md.name, None)? {
      Some(value) => {
        md.decode_value((*value).into())
      }
      None => {
        Ok(Dynamic::UNIT)
      }
    }
  }

  pub fn get_map(&mut self, mod_name: &str, storage_name: &str, key: Dynamic) -> Result<Dynamic, Box<EvalAltResult>> {
    let md = self.metadata.get_module(mod_name)
      .and_then(|m| m.get_storage(storage_name))
      .ok_or_else(|| format!("Can't find storage: {}.{}", mod_name, storage_name))?;

    // Encode map key.
    let key = md.encode_map_key(key)?;

    match self.client.get_storage_map(&md.prefix, &md.name, key, None)? {
      Some(value) => {
        md.decode_value((*value).into())
      }
      None => {
        Ok(Dynamic::UNIT)
      }
    }
  }

  pub fn get_double_map(&mut self, mod_name: &str, storage_name: &str, key1: Dynamic, key2: Dynamic) -> Result<Dynamic, Box<EvalAltResult>> {
    let md = self.metadata.get_module(mod_name)
      .and_then(|m| m.get_storage(storage_name))
      .ok_or_else(|| format!("Can't find storage: {}.{}", mod_name, storage_name))?;

    // Encode double map keys.
    let (key1, key2) = md.encode_double_map_key(key1, key2)?;

    match self.client.get_storage_double_map(&md.prefix, &md.name, key1, key2, None)? {
      Some(value) => {
        md.decode_value((*value).into())
      }
      None => {
        Ok(Dynamic::UNIT)
      }
    }
  }
}

pub fn init_engine(engine: &mut Engine, client: &Client, metadata: &Metadata) -> Storage {
  engine
    .register_type_with_name::<Storage>("Storage")
    .register_result_fn("value", Storage::get_value)
    .register_result_fn("map", Storage::get_map)
    .register_result_fn("double_map", Storage::get_double_map);
  Storage::new(client.clone(), metadata)
}
