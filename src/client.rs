use std::sync::{Arc, RwLock};

use frame_metadata::RuntimeMetadataPrefixed;
use sp_core::sr25519::Pair;
use sp_runtime::generic::Era;

use substrate_api_client::extrinsic::{compose_extrinsic_offline, xt_primitives::*};
use substrate_api_client::rpc::XtStatus;
use substrate_api_client::{Api, Hash, StorageValue};

use rhai::{Dynamic, Engine, EvalAltResult, Scope};

use crate::metadata::EncodedCall;
use crate::users::User;

pub struct InnerClient {
  api: Api<Pair>,
}

impl InnerClient {
  pub fn from_api(api: Api<Pair>) -> Arc<RwLock<Self>> {
    Arc::new(RwLock::new(Self { api }))
  }

  pub fn check_url(&self, url: &str) -> bool {
    self.api.url == url
  }

  pub fn url(&self) -> &str {
    &self.api.url
  }

  pub fn print_metadata(&self) {
    self.api.metadata.print_overview();
  }

  pub fn get_metadata(&self) -> Result<RuntimeMetadataPrefixed, Box<EvalAltResult>> {
    Ok(self.api.get_metadata().map_err(|e| e.to_string())?)
  }

  pub fn get_storage_value(
    &self,
    prefix: &str,
    key_name: &str,
    at_block: Option<Hash>,
  ) -> Result<Option<StorageValue>, Box<EvalAltResult>> {
    Ok(
      self
        .api
        .get_storage_value(prefix, key_name, at_block)
        .map_err(|e| e.to_string())?,
    )
  }

  pub fn get_nonce(&self, account: AccountId) -> Result<Option<u32>, Box<EvalAltResult>> {
    let nonce = self
      .api
      .get_account_info(&account)
      .map(|info| info.map(|info| info.nonce))
      .map_err(|e| e.to_string())?;
    Ok(nonce)
  }

  pub fn submit_call(
    &self,
    user: &User,
    call: EncodedCall,
  ) -> Result<Option<Hash>, Box<EvalAltResult>> {
    let xthex = compose_extrinsic_offline(
      &user.pair,
      call.into_call(),
      user.nonce,
      Era::Immortal,
      self.api.genesis_hash,
      self.api.genesis_hash,
      self.api.runtime_version.spec_version,
      self.api.runtime_version.transaction_version,
    )
    .hex_encode();

    let hash = self
      .api
      .send_extrinsic(xthex, XtStatus::InBlock)
      .map_err(|e| e.to_string())?;

    Ok(hash)
  }

  pub fn submit_unsigned(&self, call: EncodedCall) -> Result<Option<Hash>, Box<EvalAltResult>> {
    let xthex = (UncheckedExtrinsicV4 {
      signature: None,
      function: call.into_call(),
    })
    .hex_encode();
    let hash = self
      .api
      .send_extrinsic(xthex, XtStatus::InBlock)
      .map_err(|e| e.to_string())?;

    Ok(hash)
  }
}

#[derive(Clone)]
pub struct Client {
  inner: Arc<RwLock<InnerClient>>,
}

impl Client {
  pub fn connect(url: &str) -> Result<Self, Box<EvalAltResult>> {
    let api = Api::new(url.into()).map_err(|e| e.to_string())?;
    Ok(Self {
      inner: InnerClient::from_api(api),
    })
  }

  pub fn check_url(&self, url: &str) -> bool {
    self.inner.read().unwrap().check_url(url)
  }

  pub fn print_metadata(&mut self) {
    self.inner.read().unwrap().print_metadata()
  }

  pub fn get_metadata(&self) -> Result<RuntimeMetadataPrefixed, Box<EvalAltResult>> {
    self.inner.read().unwrap().get_metadata()
  }

  pub fn get_storage_value(
    &mut self,
    prefix: &str,
    key_name: &str,
    at_block: Option<Hash>,
  ) -> Result<Dynamic, Box<EvalAltResult>> {
    let value = self
      .inner
      .read()
      .unwrap()
      .get_storage_value(prefix, key_name, at_block)?;
    match value {
      Some(value) => {
        let data = Vec::from(&*value);
        Ok(Dynamic::from(data))
      }
      None => Ok(Dynamic::UNIT),
    }
  }

  pub fn get_nonce(&self, account: AccountId) -> Result<Option<u32>, Box<EvalAltResult>> {
    self.inner.read().unwrap().get_nonce(account)
  }

  pub fn submit_call(
    &self,
    user: &User,
    call: EncodedCall,
  ) -> Result<Option<Hash>, Box<EvalAltResult>> {
    self.inner.read().unwrap().submit_call(user, call)
  }

  pub fn submit_unsigned(&self, call: EncodedCall) -> Result<Option<Hash>, Box<EvalAltResult>> {
    self.inner.read().unwrap().submit_unsigned(call)
  }

  pub fn inner(&self) -> Arc<RwLock<InnerClient>> {
    self.inner.clone()
  }
}

pub fn init_engine(engine: &mut Engine) {
  engine
    .register_type_with_name::<Client>("Client")
    .register_result_fn("get_storage_value", Client::get_storage_value)
    .register_result_fn("submit_unsigned", Client::submit_unsigned)
    .register_fn("print_metadata", Client::print_metadata);
}

pub fn init_scope(url: &str, scope: &mut Scope<'_>) -> Result<Client, Box<EvalAltResult>> {
  let client = Client::connect(url)?;
  scope.push_constant("CLIENT", client.clone());

  Ok(client)
}
