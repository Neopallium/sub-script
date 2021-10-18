use std::sync::Arc;

use frame_metadata::RuntimeMetadataPrefixed;
use sp_core::sr25519::Pair;

use substrate_api_client::Api;

use rhai::{Engine, EvalAltResult};

pub type ClientApi = Arc<Api<Pair>>;

#[derive(Clone)]
pub struct Client {
  api: ClientApi,
}

impl Client {
  pub fn connect(url: &str) -> Result<Self, Box<EvalAltResult>> {
    let api = Api::new(url.into()).map_err(|e| e.to_string())?;
    Ok(Self { api: Arc::new(api) })
  }

  pub fn connect_with_signer(signer: Pair, url: &str) -> Result<Self, Box<EvalAltResult>> {
    let api = Api::new(url.into())
      .and_then(|api| api.set_signer(signer))
      .map_err(|e| e.to_string())?;
    Ok(Self { api: Arc::new(api) })
  }

  fn print_metadata(&mut self) {
    self.api.metadata.print_overview();
  }

  pub fn get_metadata(&self) -> Result<RuntimeMetadataPrefixed, Box<EvalAltResult>> {
    Ok(self.api.get_metadata().map_err(|e| e.to_string())?)
  }
}

pub fn init_engine(engine: &mut Engine) {
  engine
    .register_type_with_name::<Client>("Client")
    .register_result_fn("new_client", Client::connect)
    .register_fn("print_metadata", Client::print_metadata);
}
