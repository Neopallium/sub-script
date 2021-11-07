use std::sync::Arc;

use frame_metadata::RuntimeMetadataPrefixed;
use sp_runtime::generic::Era;
use sp_core::sr25519::Pair;

use substrate_api_client::Api;
use substrate_api_client::rpc::XtStatus;
use substrate_api_client::extrinsic::{
  compose_extrinsic_offline,
  xt_primitives::*,
};

use rhai::{Engine, EvalAltResult};

use super::metadata::*;

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

  pub fn submit_call(&mut self, call: EncodedCall) -> Result<String, Box<EvalAltResult>> {
    let xt = if let Some(signer) = &self.api.signer {
      compose_extrinsic_offline(
        signer,
        call.into_call(),
        self.api.get_nonce(),
        Era::Immortal,
        self.api.genesis_hash,
        self.api.genesis_hash,
        self.api.runtime_version.spec_version,
        self.api.runtime_version.transaction_version,
      ).hex_encode()
    } else {
      (UncheckedExtrinsicV4 {
        signature: None,
        function: call.into_call(),
      }).hex_encode()
    };
    let hash = self.api.send_extrinsic(xt, XtStatus::InBlock)
      .map_err(|e| e.to_string())?;

    Ok(hash.map(|hash| format!("{:x}", hash))
      .unwrap_or_default())
  }
}

pub fn init_engine(engine: &mut Engine) {
  engine
    .register_type_with_name::<Client>("Client")
    .register_result_fn("new_client", Client::connect)
    .register_result_fn("submit_call", Client::submit_call)
    .register_fn("print_metadata", Client::print_metadata);
}
