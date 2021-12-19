use rhai::{Dynamic, Engine, EvalAltResult, INT};

use sp_core::storage::StorageKey;

use crate::client::Client;
use crate::metadata::*;

#[derive(Clone)]
pub struct StorageKeysPaged {
  client: Client,
  md: StorageMetadata,
  prefix: StorageKey,
  count: u32,
  start_key: Option<StorageKey>,
  finished: bool
}

impl StorageKeysPaged {
  fn new(client: &Client, md: &StorageMetadata, prefix: StorageKey) -> Self {
    Self {
      client: client.clone(),
      md: md.clone(),
      prefix,
      count: 100,
      start_key: None,
      finished: false,
    }
  }

  fn set_page_count(&mut self, count: INT) {
    self.count = count as u32;
  }

  fn is_finished(&mut self) -> bool {
    self.finished
  }

  fn has_more(&mut self) -> bool {
    !self.finished
  }

  fn next(
    &mut self,
  ) -> Result<Dynamic, Box<EvalAltResult>> {
    if self.finished {
      // No more pages.
      return Ok(Dynamic::UNIT);
    }
    let keys = self
      .client
      .get_storage_keys_paged(&self.prefix, self.count, self.start_key.as_ref())?;
    if keys.len() < self.count as usize {
      self.finished = true;
      if keys.len() == 0 {
        // Empty page, no more storage values.
        return Ok(Dynamic::UNIT);
      }
    } else {
      self.start_key = keys.last().cloned();
    }

    let result: Vec<Dynamic> = self.client
      .get_storage_by_keys(&keys, None)?
      .into_iter()
      .map(|val| match val {
        Some(val) => self.md.decode_value(val.0),
        None => Ok(Dynamic::UNIT),
      })
      .collect::<Result<_, _>>()?;
    Ok(Dynamic::from(result))
  }
}

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
      Some(value) => md.decode_value(value.0),
      None => Ok(Dynamic::UNIT),
    }
  }

  fn get_by_keys(
    &self,
    md: &StorageMetadata,
    keys: &[StorageKey],
  ) -> Result<Vec<Dynamic>, Box<EvalAltResult>> {
    self
      .client
      .get_storage_by_keys(keys, None)?
      .into_iter()
      .map(|val| match val {
        Some(val) => md.decode_value(val.0),
        None => Ok(Dynamic::UNIT),
      })
      .collect()
  }

  fn get_keys_paged(
    &self,
    md: &StorageMetadata,
    prefix: StorageKey,
  ) -> Result<StorageKeysPaged, Box<EvalAltResult>> {
    Ok(StorageKeysPaged::new(&self.client, &md, prefix))
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

  pub fn get_map_paged(
    &mut self,
    mod_name: &str,
    storage_name: &str,
  ) -> Result<StorageKeysPaged, Box<EvalAltResult>> {
    let md = self.metadata.get_storage(mod_name, storage_name)?;
    let prefix = md.get_map_prefix()?;
    self.get_keys_paged(md, prefix)
  }

  pub fn get_map_keys(
    &mut self,
    mod_name: &str,
    storage_name: &str,
    keys: Vec<Dynamic>,
  ) -> Result<Vec<Dynamic>, Box<EvalAltResult>> {
    let md = self.metadata.get_storage(mod_name, storage_name)?;
    let keys = keys
      .into_iter()
      .map(|k| md.get_map_key(k))
      .collect::<Result<Vec<_>, Box<EvalAltResult>>>()?;
    self.get_by_keys(md, &keys)
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

  pub fn get_double_paged(
    &mut self,
    mod_name: &str,
    storage_name: &str,
    key1: Dynamic,
  ) -> Result<StorageKeysPaged, Box<EvalAltResult>> {
    let md = self.metadata.get_storage(mod_name, storage_name)?;
    let prefix = md.get_double_map_prefix(key1)?;
    self.get_keys_paged(md, prefix)
  }
}

pub fn init_engine(engine: &mut Engine, client: &Client, metadata: &Metadata) -> Storage {
  engine
    .register_type_with_name::<Storage>("Storage")
    .register_result_fn("value", Storage::get_value)
    .register_result_fn("map", Storage::get_map)
    .register_result_fn("map_keys", Storage::get_map_keys)
    .register_result_fn("double_map", Storage::get_double_map)
    .register_result_fn("map_paged", Storage::get_map_paged)
    .register_result_fn("double_paged", Storage::get_double_paged)
    .register_type_with_name::<StorageKeysPaged>("StorageKeysPaged")
    .register_get("is_finished", StorageKeysPaged::is_finished)
    .register_get("has_more", StorageKeysPaged::has_more)
    .register_fn("set_page_count", StorageKeysPaged::set_page_count)
    .register_result_fn("next", StorageKeysPaged::next);
  Storage::new(client.clone(), metadata)
}
