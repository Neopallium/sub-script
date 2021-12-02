use rhai::{Engine, EvalAltResult, Scope};

use crate::client::Client;
use crate::types::TypeLookup;

pub mod ledger;
pub use ::ledger::*;

pub fn init_engine(engine: &mut Engine) {
  ledger::init_engine(engine);
}

pub fn init_scope(
  client: &Client,
  lookup: &TypeLookup,
  engine: &mut Engine,
  scope: &mut Scope<'_>,
) -> Result<(), Box<EvalAltResult>> {
  ledger::init_scope(client, lookup, engine, scope)?;

  Ok(())
}
