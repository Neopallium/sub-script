use rhai::{Engine, EvalAltResult, Scope};

use crate::metadata::*;

#[derive(Clone)]
pub struct ApiLookup {
  metadata: Metadata,
}

impl ApiLookup {
  pub fn new(metadata: Metadata) -> Result<Self, Box<EvalAltResult>> {
    Ok(Self {
      metadata,
    })
  }
}

pub fn init_engine(engine: &mut Engine) {
  engine
    .register_type_with_name::<ApiLookup>("ApiLookup")
    ;
}

pub fn init_scope(metadata: Metadata, scope: &mut Scope<'_>) -> Result<(), Box<EvalAltResult>> {
  let lookup = ApiLookup::new(metadata)?;
  scope.push_constant("API", lookup);

  Ok(())
}
