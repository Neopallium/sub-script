use std::sync::{Arc, RwLock};

use frame_metadata::RuntimeMetadataPrefixed;
use sp_core::sr25519::Pair;
use sp_runtime::generic::Era;

use substrate_api_client::extrinsic::{compose_extrinsic_offline, xt_primitives::*};
use substrate_api_client::rpc::XtStatus;
use substrate_api_client::{Api, Hash, StorageValue};

use rhai::{Dynamic, Engine, EvalAltResult, Scope};

use crate::users::User;
use crate::types::{TypeLookup, TypeRef};
use crate::metadata::EncodedCall;

pub struct InnerClient {
  api: Api<Pair>,
  event_records: TypeRef,
}

impl InnerClient {
  pub fn new(api: Api<Pair>, lookup: &TypeLookup) -> Arc<RwLock<Self>> {
    let event_records = lookup.resolve("EventRecords");
    Arc::new(RwLock::new(Self { api, event_records }))
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

  pub fn get_events(&self, block: Option<Hash>, _xthex: Option<&str>) -> Result<Dynamic, Box<EvalAltResult>> {
    match self.get_storage_value("System", "Events", block)? {
      Some(value) => {
        let data = Vec::from(&*value);
        Ok(self.event_records.decode(data)?)
      }
      None => Ok(Dynamic::UNIT),
    }
  }

  pub fn get_nonce(&self, account: AccountId) -> Result<Option<u32>, Box<EvalAltResult>> {
    let nonce = self
      .api
      .get_account_info(&account)
      .map(|info| info.map(|info| info.nonce))
      .map_err(|e| e.to_string())?;
    Ok(nonce)
  }

  fn submit(&self, xthex: String) -> Result<Option<(Hash, String)>, Box<EvalAltResult>> {
    let res = self
      .api
      .send_extrinsic(xthex.clone(), XtStatus::InBlock)
      .map_err(|e| e.to_string())?
      .map(|hash| (hash, xthex));

    Ok(res)
  }

  pub fn submit_call(
    &self,
    user: &User,
    call: EncodedCall,
  ) -> Result<Option<(Hash, String)>, Box<EvalAltResult>> {
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

    self.submit(xthex)
  }

  pub fn submit_unsigned(&self, call: EncodedCall) -> Result<Option<(Hash, String)>, Box<EvalAltResult>> {
    let xthex = (UncheckedExtrinsicV4 {
      signature: None,
      function: call.into_call(),
    })
    .hex_encode();

    self.submit(xthex)
  }
}

#[derive(Clone)]
pub struct Client {
  inner: Arc<RwLock<InnerClient>>,
}

impl Client {
  pub fn connect(url: &str, lookup: &TypeLookup) -> Result<Self, Box<EvalAltResult>> {
    let api = Api::new(url.into()).map_err(|e| e.to_string())?;
    Ok(Self {
      inner: InnerClient::new(api, lookup),
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

  pub fn get_events(&self, block: Option<Hash>, xthex: Option<&str>) -> Result<Dynamic, Box<EvalAltResult>> {
    self.inner.read().unwrap().get_events(block, xthex)
  }

  pub fn get_nonce(&self, account: AccountId) -> Result<Option<u32>, Box<EvalAltResult>> {
    self.inner.read().unwrap().get_nonce(account)
  }

  pub fn submit_call(&self, user: &User, call: EncodedCall) -> Result<ExtrinsicCallResult, Box<EvalAltResult>> {
    self.inner.read().unwrap().submit_call(user, call)
      .map(|res| ExtrinsicCallResult::new(self, res))
  }

  pub fn submit_unsigned(&self, call: EncodedCall) -> Result<ExtrinsicCallResult, Box<EvalAltResult>> {
    self.inner.read().unwrap().submit_unsigned(call)
      .map(|res| ExtrinsicCallResult::new(self, res))
  }

  pub fn inner(&self) -> Arc<RwLock<InnerClient>> {
    self.inner.clone()
  }
}

#[derive(Clone)]
pub struct BlockRef {
  client: Client,
  block: Hash,
}

impl BlockRef {
  pub fn new(client: &Client, block: Hash) -> Self {
    Self {
      client: client.clone(),
      block,
    }
  }

  pub fn events(&mut self) -> Result<Dynamic, Box<EvalAltResult>> {
    self.get_events(None)
  }

  pub fn get_events(&self, xthex: Option<&str>) -> Result<Dynamic, Box<EvalAltResult>> {
    self.client.get_events(Some(self.block), xthex)
  }

  pub fn to_string(&mut self) -> String {
    format!("Block: {:?}", self.block)
  }
}

#[derive(Clone)]
pub enum ExtrinsicCallResult {
  NoBlock,
  InBlock(BlockRef, String),
}

impl ExtrinsicCallResult {
  pub fn new(client: &Client, res: Option<(Hash, String)>) -> Self {
    match res {
      Some((block, xthex)) => Self::InBlock(BlockRef::new(client, block), xthex),
      None => Self::NoBlock,
    }
  }

  pub fn events(&mut self) -> Result<Dynamic, Box<EvalAltResult>> {
    use ExtrinsicCallResult::*;
    let events =match self {
      NoBlock => Dynamic::UNIT,
      InBlock(block, xthex) => Dynamic::from(block.get_events(Some(xthex))?),
    };

    Ok(events)
  }

  pub fn to_string(&mut self) -> String {
    use ExtrinsicCallResult::*;
    match self {
      NoBlock => {
        format!("NoBlock")
      }
      InBlock(block, _xthex) => {
        format!("InBlock: {:?}", block.block)
      }
    }
  }
}

pub fn init_engine(engine: &mut Engine) {
  engine
    .register_type_with_name::<Client>("Client")
    .register_result_fn("get_storage_value", Client::get_storage_value)
    .register_result_fn("submit_unsigned", Client::submit_unsigned)
    .register_fn("print_metadata", Client::print_metadata)

    .register_type_with_name::<BlockRef>("BlockRef")
    .register_result_fn("events", BlockRef::events)
    .register_fn("to_string", BlockRef::to_string)

    .register_type_with_name::<ExtrinsicCallResult>("ExtrinsicCallResult")
    .register_result_fn("events", ExtrinsicCallResult::events)
    .register_fn("to_string", ExtrinsicCallResult::to_string)
    ;
}

pub fn init_scope(url: &str, lookup: &TypeLookup, scope: &mut Scope<'_>) -> Result<Client, Box<EvalAltResult>> {
  let client = Client::connect(url, lookup)?;
  scope.push_constant("CLIENT", client.clone());

  Ok(client)
}
