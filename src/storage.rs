use rhai::{Dynamic, Engine, EvalAltResult};

use sp_core::storage::StorageKey;

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

  fn get_by_key(
    &self,
    md: &StorageMetadata,
    key: StorageKey,
  ) -> Result<Dynamic, Box<EvalAltResult>> {
    match self.client.get_storage_by_key(key, None)? {
      Some(value) => md.decode_value((*value).into()),
      None => Ok(Dynamic::UNIT),
    }
  }

  pub fn get_value(
    &mut self,
    mod_name: &str,
    storage_name: &str,
  ) -> Result<Dynamic, Box<EvalAltResult>> {
    let md = self.metadata.get_storage(mod_name, storage_name)?;
    let key = md.get_value_key()?;
    self.get_by_key(md, key)
  }

  pub fn get_map(
    &mut self,
    mod_name: &str,
    storage_name: &str,
    key: Dynamic,
  ) -> Result<Dynamic, Box<EvalAltResult>> {
    let md = self.metadata.get_storage(mod_name, storage_name)?;
    let key = md.get_map_key(key)?;
    self.get_by_key(md, key)
  }

  pub fn get_double_map(
    &mut self,
    mod_name: &str,
    storage_name: &str,
    key1: Dynamic,
    key2: Dynamic,
  ) -> Result<Dynamic, Box<EvalAltResult>> {
    let md = self.metadata.get_storage(mod_name, storage_name)?;
    let key = md.get_double_map_key(key1, key2)?;
    self.get_by_key(md, key)
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
