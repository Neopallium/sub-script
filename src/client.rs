use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use frame_metadata::RuntimeMetadataPrefixed;
use sp_core::sr25519::Pair;
use sp_runtime::{generic, traits};

use serde::{Deserialize, Serialize};

use substrate_api_client::extrinsic::{compose_extrinsic_offline, xt_primitives::*};
use substrate_api_client::rpc::{json_req::*, XtStatus};
use substrate_api_client::{Api, Hash, StorageValue};

use rhai::serde::from_dynamic;
use rhai::{Dynamic, Engine, EvalAltResult, Scope};

use crate::metadata::EncodedCall;
use crate::types::{TypeLookup, TypeRef};
use crate::users::User;

pub type SignedBlock = generic::SignedBlock<Block>;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Block {
  extrinsics: Vec<String>,
  header: generic::Header<u32, traits::BlakeTwo256>,
}

impl Block {
  pub fn find_extrinsic(&self, xthex: &str) -> Option<usize> {
    self.extrinsics.iter().position(|xt| xt == xthex)
  }

  pub fn to_string(&mut self) -> String {
    format!("{:?}", self)
  }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum Phase {
  ApplyExtrinsic(u32),
  Finalization,
  Initialization,
}

#[derive(Clone, Debug, Default)]
pub struct NamedEvent(String, Dynamic);

impl NamedEvent {
  pub fn name(&mut self) -> String {
    self.0.clone()
  }

  pub fn args(&mut self) -> Dynamic {
    self.1.clone()
  }

  pub fn to_string(&mut self) -> String {
    format!("{}({:#?})", self.0, self.1)
  }
}

#[derive(Clone, Debug, Default)]
pub struct EventList(Vec<NamedEvent>);

impl EventList {
  pub fn to_string(&mut self) -> String {
    format!("Events: {:#?}", self.0)
  }

  pub fn into_inner(self) -> Vec<NamedEvent> {
    self.0
  }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct EventRecord {
  pub phase: Phase,
  pub event: HashMap<String, HashMap<String, Dynamic>>,
  pub topics: Vec<Hash>,
}

impl EventRecord {
  pub fn to_string(&mut self) -> String {
    format!("{:#?}", self)
  }

  pub fn into_named(self) -> NamedEvent {
    // Two nested maps, should only have one item each.
    if let Some((mod_name, event)) = self.event.into_iter().next() {
      if let Some((name, event)) = event.into_iter().next() {
        NamedEvent(format!("{}.{}", mod_name, name), event)
      } else {
        NamedEvent(format!("{}", mod_name), Dynamic::UNIT)
      }
    } else {
      NamedEvent("()".into(), Dynamic::UNIT)
    }
  }
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct EventRecords(Vec<EventRecord>);

impl EventRecords {
  pub fn filter(&mut self, phase: Phase) {
    self.0.retain(|ev| ev.phase == phase);
  }

  pub fn into_event_list(self) -> EventList {
    EventList(self.0.into_iter().map(|ev| ev.into_named()).collect())
  }

  pub fn to_string(&mut self) -> String {
    format!("{:#?}", self.0)
  }
}

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

  pub fn get_block(&self, hash: Option<Hash>) -> Result<Option<Block>, Box<EvalAltResult>> {
    Ok(self.get_signed_block(hash)?.map(|signed| signed.block))
  }

  pub fn get_signed_block(
    &self,
    hash: Option<Hash>,
  ) -> Result<Option<SignedBlock>, Box<EvalAltResult>> {
    Ok(match self.get_request(chain_get_block(hash).to_string())? {
      Some(data) => {
        let signed = serde_json::from_str(&data).map_err(|e| e.to_string())?;
        Some(signed)
      }
      None => None,
    })
  }

  pub fn get_request(&self, req: String) -> Result<Option<String>, Box<EvalAltResult>> {
    Ok(self.api.get_request(req).map_err(|e| e.to_string())?)
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

  pub fn get_events(&self, block: Option<Hash>) -> Result<Dynamic, Box<EvalAltResult>> {
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

  fn submit(&self, xthex: String) -> Result<(Option<Hash>, String), Box<EvalAltResult>> {
    let hash = self
      .api
      .send_extrinsic(xthex.clone(), XtStatus::InBlock)
      .map_err(|e| e.to_string())?;

    Ok((hash, xthex))
  }

  pub fn submit_call(
    &self,
    user: &User,
    call: EncodedCall,
  ) -> Result<(Option<Hash>, String), Box<EvalAltResult>> {
    let xthex = compose_extrinsic_offline(
      &user.pair,
      call.into_call(),
      user.nonce,
      generic::Era::Immortal,
      self.api.genesis_hash,
      self.api.genesis_hash,
      self.api.runtime_version.spec_version,
      self.api.runtime_version.transaction_version,
    )
    .hex_encode();

    self.submit(xthex)
  }

  pub fn submit_unsigned(
    &self,
    call: EncodedCall,
  ) -> Result<(Option<Hash>, String), Box<EvalAltResult>> {
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

  pub fn get_block(&self, hash: Option<Hash>) -> Result<Option<Block>, Box<EvalAltResult>> {
    self.inner.read().unwrap().get_block(hash)
  }

  pub fn get_request(&self, req: String) -> Result<Option<String>, Box<EvalAltResult>> {
    self.inner.read().unwrap().get_request(req)
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

  pub fn get_events(&self, block: Option<Hash>) -> Result<Dynamic, Box<EvalAltResult>> {
    self.inner.read().unwrap().get_events(block)
  }

  pub fn get_nonce(&self, account: AccountId) -> Result<Option<u32>, Box<EvalAltResult>> {
    self.inner.read().unwrap().get_nonce(account)
  }

  pub fn submit_call(
    &self,
    user: &User,
    call: EncodedCall,
  ) -> Result<ExtrinsicCallResult, Box<EvalAltResult>> {
    self
      .inner
      .read()
      .unwrap()
      .submit_call(user, call)
      .map(|(block, xthex)| ExtrinsicCallResult::new(self, block, xthex))
  }

  pub fn submit_unsigned(
    &self,
    call: EncodedCall,
  ) -> Result<ExtrinsicCallResult, Box<EvalAltResult>> {
    self
      .inner
      .read()
      .unwrap()
      .submit_unsigned(call)
      .map(|(block, xthex)| ExtrinsicCallResult::new(self, block, xthex))
  }

  pub fn inner(&self) -> Arc<RwLock<InnerClient>> {
    self.inner.clone()
  }
}

#[derive(Clone)]
pub struct ExtrinsicCallResult {
  client: Client,
  hash: Option<Hash>,
  xthex: String,
  idx: Option<u32>,
  events: Option<EventList>,
}

impl ExtrinsicCallResult {
  pub fn new(client: &Client, hash: Option<Hash>, xthex: String) -> Self {
    Self {
      client: client.clone(),
      hash,
      xthex,
      idx: None,
      events: None,
    }
  }

  fn load_events(&mut self) -> Result<(), Box<EvalAltResult>> {
    if self.events.is_some() {
      return Ok(());
    }
    let events = match self.hash {
      Some(hash) => {
        // Load block and find the index of our extrinsic.
        let xt_idx = match self.client.get_block(Some(hash))? {
          Some(block) => block.find_extrinsic(&self.xthex),
          None => None,
        };
        self.idx = xt_idx.map(|idx| idx as u32);
        let mut events: EventRecords = from_dynamic(&self.client.get_events(Some(hash))?)?;
        if let Some(idx) = self.idx {
          events.filter(Phase::ApplyExtrinsic(idx));
        }
        events.into_event_list()
      }
      None => EventList::default(),
    };

    self.events = Some(events);
    Ok(())
  }

  pub fn events_filtered(&mut self, prefix: &str) -> Result<Vec<Dynamic>, Box<EvalAltResult>> {
    self.load_events()?;
    match &self.events {
      Some(events) => {
        let filtered = events
          .0
          .iter()
          .filter(|ev| ev.0.starts_with(prefix))
          .cloned()
          .map(|ev| Dynamic::from(ev))
          .collect::<Vec<_>>();
        Ok(filtered)
      }
      None => Ok(vec![]),
    }
  }

  pub fn events(&mut self) -> Result<Vec<Dynamic>, Box<EvalAltResult>> {
    self.events_filtered("")
  }

  pub fn result(&mut self) -> Result<Dynamic, Box<EvalAltResult>> {
    // Look for event `System.ExtrinsicSuccess` or `System.ExtrinsicFailed`
    // to get the Extrinsic result.
    let mut events = self.events_filtered("System.Extrinsic")?;
    // Just return the last found event.  Should only be one.
    match events.pop() {
      Some(result) => Ok(result),
      None => Ok(Dynamic::UNIT),
    }
  }

  pub fn is_success(&mut self) -> Result<bool, Box<EvalAltResult>> {
    // Look for event `System.ExtrinsicSuccess`.
    let events = self.events_filtered("System.ExtrinsicSuccess")?;
    Ok(events.len() > 0)
  }

  pub fn block(&mut self) -> Result<Dynamic, Box<EvalAltResult>> {
    match self.hash {
      Some(hash) => match self.client.get_block(Some(hash))? {
        Some(block) => Ok(Dynamic::from(block)),
        None => Ok(Dynamic::UNIT),
      },
      None => Ok(Dynamic::UNIT),
    }
  }

  pub fn xthex(&mut self) -> String {
    self.xthex.clone()
  }

  pub fn to_string(&mut self) -> String {
    match &self.hash {
      Some(hash) => {
        format!("InBlock: {:?}", hash)
      }
      None => {
        format!("NoBlock")
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
    .register_type_with_name::<Block>("Block")
    .register_fn("to_string", Block::to_string)
    .register_type_with_name::<EventRecords>("EventRecords")
    .register_fn("to_string", EventRecords::to_string)
    .register_type_with_name::<EventRecord>("EventRecord")
    .register_fn("to_string", EventRecord::to_string)
    .register_type_with_name::<NamedEvent>("Event")
    .register_get("name", NamedEvent::name)
    .register_get("args", NamedEvent::args)
    .register_fn("to_string", NamedEvent::to_string)
    .register_type_with_name::<EventList>("EventList")
    .register_fn("to_string", EventList::to_string)
    .register_type_with_name::<ExtrinsicCallResult>("ExtrinsicCallResult")
    .register_result_fn("events", ExtrinsicCallResult::events_filtered)
    .register_get_result("events", ExtrinsicCallResult::events)
    .register_get_result("block", ExtrinsicCallResult::block)
    .register_get_result("result", ExtrinsicCallResult::result)
    .register_get_result("is_success", ExtrinsicCallResult::is_success)
    .register_get("xthex", ExtrinsicCallResult::xthex)
    .register_fn("to_string", ExtrinsicCallResult::to_string);
}

pub fn init_scope(
  url: &str,
  lookup: &TypeLookup,
  scope: &mut Scope<'_>,
) -> Result<Client, Box<EvalAltResult>> {
  let client = Client::connect(url, lookup)?;
  scope.push_constant("CLIENT", client.clone());

  Ok(client)
}
