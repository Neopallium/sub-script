use std::collections::HashMap;

use rhai::{Dynamic, Engine, EvalAltResult};

use crate::client::Client;
use crate::types::TypeLookup;

pub mod ledger;

#[cfg(feature = "polymesh")]
pub mod polymesh;

pub fn init_engine(
  engine: &mut Engine,
  globals: &mut HashMap<String, Dynamic>,
  client: &Client,
  lookup: &TypeLookup,
) -> Result<(), Box<EvalAltResult>> {
  ledger::init_engine(engine, globals, client, lookup)?;

  #[cfg(feature = "polymesh")]
  polymesh::init_engine(engine, globals, client, lookup)?;

  Ok(())
}
