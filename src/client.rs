use std::any::TypeId;
use std::convert::TryFrom;
use std::sync::{Arc, RwLock};

use hex::FromHex;

use frame_metadata::RuntimeMetadataPrefixed;
use parity_scale_codec::{Compact, Decode, Encode};
use sp_core::{
  crypto::{set_default_ss58_version, Ss58AddressFormat},
  hashing::blake2_256,
  storage::{StorageData, StorageKey},
  Pair, H256,
};
use sp_runtime::{
  generic::{self, Era},
  traits, MultiSignature,
};
use sp_version::RuntimeVersion;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use dashmap::DashMap;

use rust_decimal::{prelude::ToPrimitive, Decimal};

use rhai::serde::from_dynamic;
use rhai::{Dynamic, Engine, EvalAltResult, Map as RMap, INT};

use crate::metadata::{EncodedCall, Metadata};
use crate::rpc::*;
use crate::types::{TypeLookup, TypeRef};
use crate::users::{AccountId, User};

pub type TxHash = H256;
pub type BlockHash = H256;

pub type SignedBlock = generic::SignedBlock<Block>;
pub type GenericAddress = sp_runtime::MultiAddress<AccountId, ()>;

pub type AdditionalSigned = (u32, u32, BlockHash, BlockHash, (), (), ());

#[derive(Clone, Debug, Encode, Decode)]
pub struct Extra(Era, Compact<u32>, Compact<u128>);

impl Extra {
  pub fn new(era: Era, nonce: u32) -> Self {
    Self(era, nonce.into(), 0u128.into())
  }
}

pub struct SignedPayload<'a>((&'a EncodedCall, &'a Extra, AdditionalSigned));

impl<'a> SignedPayload<'a> {
  pub fn new(call: &'a EncodedCall, extra: &'a Extra, additional: AdditionalSigned) -> Self {
    Self((call, extra, additional))
  }
}

impl<'a> Encode for SignedPayload<'a> {
  fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
    self.0.using_encoded(|payload| {
      if payload.len() > 256 {
        f(&blake2_256(payload)[..])
      } else {
        f(payload)
      }
    })
  }
}

/// Current version of the [`UncheckedExtrinsic`] format.
pub const EXTRINSIC_VERSION: u8 = 4;

#[derive(Clone)]
pub struct ExtrinsicV4 {
  pub signature: Option<(GenericAddress, MultiSignature, Extra)>,
  pub call: EncodedCall,
}

impl ExtrinsicV4 {
  pub fn signed(account: AccountId, sig: MultiSignature, extra: Extra, call: EncodedCall) -> Self {
    Self {
      signature: Some((GenericAddress::from(account), sig, extra)),
      call,
    }
  }

  pub fn unsigned(call: EncodedCall) -> Self {
    Self {
      signature: None,
      call,
    }
  }

  pub fn to_hex(&self) -> String {
    let mut hex = hex::encode(self.encode());
    hex.insert_str(0, "0x");
    hex
  }
}

impl Encode for ExtrinsicV4 {
  fn encode(&self) -> Vec<u8> {
    let mut buf = Vec::with_capacity(512);

    // 1 byte version id and signature if signed.
    match &self.signature {
      Some(sig) => {
        buf.push(EXTRINSIC_VERSION | 0b1000_0000);
        sig.encode_to(&mut buf);
      }
      None => {
        buf.push(EXTRINSIC_VERSION | 0b0111_1111);
      }
    }
    self.call.encode_to(&mut buf);

    buf.encode()
  }
}

#[derive(Clone, Debug, Deserialize)]
pub struct AccountInfo {
  pub nonce: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransactionStatus {
  Future,
  Ready,
  Broadcast(Vec<String>),
  InBlock(BlockHash),
  Retracted(BlockHash),
  FinalityTimeout(BlockHash),
  Finalized(BlockHash),
  Usurped(TxHash),
  Dropped,
  Invalid,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug)]
pub struct EventRecord {
  pub phase: Phase,
  pub name: String,
  pub args: Dynamic,
  pub topics: Vec<BlockHash>,
}

impl EventRecord {
  pub fn name(&mut self) -> String {
    self.name.clone()
  }

  pub fn args(&mut self) -> Dynamic {
    self.args.clone()
  }

  pub fn to_string(&mut self) -> String {
    format!("{:#?}", self)
  }

  pub fn from_dynamic(val: Dynamic) -> Result<Self, Box<EvalAltResult>> {
    let mut map = val.try_cast::<RMap>().ok_or("Expected Map")?;

    // Decod event name and args from two nested maps,
    // should only have one item in each map.
    let event = map
      .remove("event")
      .ok_or("Missing field 'event'")?
      .try_cast::<RMap>()
      .ok_or("Expected Map")?;
    let (name, args) = match event.into_iter().next() {
      Some((mod_name, map2)) => {
        let map2 = map2.try_cast::<RMap>().ok_or("Expected Map")?;
        match map2.into_iter().next() {
          Some((name, args)) => (format!("{}.{}", mod_name, name), args),
          None => (format!("{}", mod_name), Dynamic::UNIT),
        }
      }
      None => ("()".into(), Dynamic::UNIT),
    };

    Ok(Self {
      phase: from_dynamic(map.get("phase").ok_or("Missing field 'phase'")?)?,
      name,
      args,
      topics: from_dynamic(map.get("topics").ok_or("Missing field 'topics'")?)?,
    })
  }
}

#[derive(Clone, Debug, Default)]
pub struct EventRecords(Vec<EventRecord>);

impl EventRecords {
  pub fn filter(&mut self, phase: Phase) {
    self.0.retain(|ev| ev.phase == phase);
  }

  pub fn to_string(&mut self) -> String {
    format!("{:#?}", self.0)
  }

  pub fn from_dynamic(val: Dynamic) -> Result<Self, Box<EvalAltResult>> {
    let arr = val.try_cast::<Vec<Dynamic>>().ok_or("Expected Array")?;
    Ok(Self(
      arr
        .into_iter()
        .map(EventRecord::from_dynamic)
        .collect::<Result<Vec<EventRecord>, _>>()?,
    ))
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainProperties {
  pub ss58_format: u16,
  pub token_decimals: u32,
  pub token_symbol: String,
}

pub struct InnerClient {
  rpc: RpcHandler,
  runtime_version: RuntimeVersion,
  genesis_hash: BlockHash,
  metadata: Metadata,
  event_records: TypeRef,
  account_info: TypeRef,
  cached_blocks: DashMap<BlockHash, Block>,
  cached_events: DashMap<BlockHash, Dynamic>,
}

impl InnerClient {
  pub fn new(
    rpc: RpcHandler,
    lookup: &TypeLookup,
  ) -> Result<Arc<RwLock<Self>>, Box<EvalAltResult>> {
    let runtime_version = Self::rpc_get_runtime_version(&rpc)?;
    let genesis_hash = Self::rpc_get_genesis_hash(&rpc)?;
    let runtime_metadata = Self::rpc_get_runtime_metadata(&rpc)?;
    let metadata = Metadata::from_runtime_metadata(runtime_metadata, lookup)?;

    let event_records = lookup.resolve("EventRecords");
    let account_info = lookup.resolve("AccountInfo");
    Ok(Arc::new(RwLock::new(Self {
      rpc,
      runtime_version,
      genesis_hash,
      metadata,
      event_records,
      account_info,
      cached_blocks: DashMap::new(),
      cached_events: DashMap::new(),
    })))
  }

  /// Get runtime version from rpc node.
  fn rpc_get_runtime_version(rpc: &RpcHandler) -> Result<RuntimeVersion, Box<EvalAltResult>> {
    Ok(
      rpc
        .call_method("state_getRuntimeVersion", Value::Null)?
        .ok_or_else(|| format!("Failed to get RuntimeVersion from node."))?,
    )
  }

  /// Get genesis hash from rpc node.
  fn rpc_get_genesis_hash(rpc: &RpcHandler) -> Result<BlockHash, Box<EvalAltResult>> {
    Ok(
      rpc
        .call_method("chain_getBlockHash", json!([0u64]))?
        .ok_or_else(|| format!("Failed to get genesis hash from node."))?,
    )
  }

  /// Get metadata from rpc node.
  fn rpc_get_runtime_metadata(
    rpc: &RpcHandler,
  ) -> Result<RuntimeMetadataPrefixed, Box<EvalAltResult>> {
    let hex: String = rpc
      .call_method("state_getMetadata", json!([]))?
      .ok_or_else(|| format!("Failed to get Metadata from node."))?;

    let bytes = Vec::from_hex(&hex[2..]).map_err(|e| e.to_string())?;
    Ok(RuntimeMetadataPrefixed::decode(&mut bytes.as_slice()).map_err(|e| e.to_string())?)
  }

  pub fn get_metadata(&self) -> Metadata {
    self.metadata.clone()
  }

  pub fn get_signed_extra(&self) -> AdditionalSigned {
    (
      self.runtime_version.spec_version,
      self.runtime_version.transaction_version,
      self.genesis_hash,
      self.genesis_hash,
      (),
      (),
      (),
    )
  }

  pub fn get_block(&self, hash: Option<BlockHash>) -> Result<Option<Block>, Box<EvalAltResult>> {
    // Only check for cached blocks when the hash is provided.
    Ok(if let Some(hash) = hash {
      let block = self.cached_blocks.get(&hash);
      if block.is_some() {
        block.as_deref().cloned()
      } else {
        let block = self
          .get_signed_block(Some(hash))?
          .map(|signed| signed.block);
        if let Some(block) = &block {
          // Cache new block.
          self.cached_blocks.insert(hash, block.clone());
        }
        block
      }
    } else {
      self.get_signed_block(hash)?.map(|signed| signed.block)
    })
  }

  pub fn get_chain_properties(&self) -> Result<Option<ChainProperties>, Box<EvalAltResult>> {
    self.rpc.call_method("system_properties", json!([]))
  }

  pub fn get_signed_block(
    &self,
    hash: Option<BlockHash>,
  ) -> Result<Option<SignedBlock>, Box<EvalAltResult>> {
    self.rpc.call_method("chain_getBlock", json!([hash]))
  }

  pub fn get_storage_keys_paged(
    &self,
    prefix: &StorageKey,
    count: u32,
    start_key: Option<&StorageKey>,
  ) -> Result<Vec<StorageKey>, Box<EvalAltResult>> {
    self
      .rpc
      .call_method(
        "state_getKeysPaged",
        json!([prefix, count, start_key.unwrap_or(prefix)]),
      )
      .map(|res| res.unwrap_or_default())
  }

  pub fn get_storage_by_key(
    &self,
    key: StorageKey,
    at_block: Option<BlockHash>,
  ) -> Result<Option<StorageData>, Box<EvalAltResult>> {
    self
      .rpc
      .call_method("state_getStorage", json!([key, at_block]))
  }

  pub fn get_storage_by_keys(
    &self,
    keys: &[StorageKey],
    at_block: Option<BlockHash>,
  ) -> Result<Vec<Option<StorageData>>, Box<EvalAltResult>> {
    let tokens: Vec<RequestToken> = keys
      .into_iter()
      .map(|k| {
        self
          .rpc
          .async_call_method("state_getStorage", json!([k, at_block]))
      })
      .collect::<Result<Vec<_>, Box<EvalAltResult>>>()?;
    self.rpc.get_responses(tokens.as_slice())
  }

  pub fn get_storage_value(
    &self,
    module: &str,
    storage: &str,
    at_block: Option<BlockHash>,
  ) -> Result<Option<StorageData>, Box<EvalAltResult>> {
    let md = self.metadata.get_storage(module, storage)?;
    let key = md.get_value_key()?;
    self.get_storage_by_key(key, at_block)
  }

  pub fn get_storage_map(
    &self,
    module: &str,
    storage: &str,
    key: Vec<u8>,
    at_block: Option<BlockHash>,
  ) -> Result<Option<StorageData>, Box<EvalAltResult>> {
    let md = self.metadata.get_storage(module, storage)?;
    let key = md.raw_map_key(key)?;
    self.get_storage_by_key(key, at_block)
  }

  pub fn get_storage_double_map(
    &self,
    module: &str,
    storage: &str,
    key1: Vec<u8>,
    key2: Vec<u8>,
    at_block: Option<BlockHash>,
  ) -> Result<Option<StorageData>, Box<EvalAltResult>> {
    let md = self.metadata.get_storage(module, storage)?;
    let key = md.raw_double_map_key(key1, key2)?;
    self.get_storage_by_key(key, at_block)
  }

  fn get_block_events(&self, hash: Option<BlockHash>) -> Result<Dynamic, Box<EvalAltResult>> {
    match self.get_storage_value("System", "Events", hash)? {
      Some(value) => Ok(self.event_records.decode(value.0)?),
      None => Ok(Dynamic::UNIT),
    }
  }

  pub fn get_events(&self, hash: Option<BlockHash>) -> Result<Dynamic, Box<EvalAltResult>> {
    if let Some(hash) = hash {
      let events = self.cached_events.get(&hash);
      if let Some(events) = events {
        Ok(events.clone())
      } else {
        let events = self.get_block_events(Some(hash))?;
        // Cache new events.
        self.cached_events.insert(hash, events.clone());
        Ok(events)
      }
    } else {
      self.get_block_events(hash)
    }
  }

  pub fn get_account_info(
    &self,
    account: AccountId,
  ) -> Result<Option<Dynamic>, Box<EvalAltResult>> {
    match self.get_storage_map("System", "Account", account.encode(), None)? {
      Some(value) => {
        // Decode chain's 'AccountInfo' value.
        Ok(Some(self.account_info.decode(value.0)?))
      }
      None => Ok(None),
    }
  }

  pub fn get_nonce(&self, account: AccountId) -> Result<Option<u32>, Box<EvalAltResult>> {
    match self.get_account_info(account)? {
      Some(value) => {
        // Get nonce.
        let account_info: AccountInfo = from_dynamic(&value)?;
        Ok(Some(account_info.nonce))
      }
      None => Ok(None),
    }
  }

  pub fn get_request_block_hash(
    &self,
    token: RequestToken,
  ) -> Result<Option<BlockHash>, Box<EvalAltResult>> {
    let hash = loop {
      let status = self.rpc.get_update(token)?;
      match status {
        Some(TransactionStatus::InBlock(hash))
        | Some(TransactionStatus::Finalized(hash))
        | Some(TransactionStatus::FinalityTimeout(hash)) => {
          break Some(hash);
        }
        Some(TransactionStatus::Future) => {
          log::warn!("Transaction in future (maybe nonce issue)");
        }
        Some(TransactionStatus::Ready) => {
          log::debug!("Transaction ready.");
        }
        Some(TransactionStatus::Broadcast(nodes)) => {
          log::debug!("Transaction broadcast: {:?}", nodes);
        }
        Some(TransactionStatus::Retracted(hash)) => {
          log::error!("Transaction retracted: {:?}", hash);
        }
        Some(TransactionStatus::Usurped(tx_hash)) => {
          log::error!(
            "Transaction was replaced by another in the pool: {:?}",
            tx_hash
          );
          break None;
        }
        Some(TransactionStatus::Dropped) => {
          log::error!("Transaction dropped.");
          break None;
        }
        Some(TransactionStatus::Invalid) => {
          log::error!("Transaction invalid.");
          break None;
        }
        None => {
          break None;
        }
      }
    };
    self.rpc.close_request(token)?;

    Ok(hash)
  }

  pub fn submit(&self, xthex: String) -> Result<(RequestToken, String), Box<EvalAltResult>> {
    let token = self.rpc.subscribe(
      "author_submitAndWatchExtrinsic",
      json!([xthex]),
      "author_unwatchExtrinsic",
    )?;
    Ok((token, xthex))
  }

  pub fn submit_call(
    &self,
    user: &User,
    call: EncodedCall,
  ) -> Result<(RequestToken, String), Box<EvalAltResult>> {
    let extra = Extra::new(Era::Immortal, user.nonce);
    let payload = SignedPayload::new(&call, &extra, self.get_signed_extra());

    let sig = payload.using_encoded(|p| user.pair.sign(p));

    let xt = ExtrinsicV4::signed(user.acc(), sig.into(), extra, call);
    let xthex = xt.to_hex();

    self.submit(xthex)
  }

  pub fn submit_unsigned(
    &self,
    call: EncodedCall,
  ) -> Result<(RequestToken, String), Box<EvalAltResult>> {
    let xthex = ExtrinsicV4::unsigned(call).to_hex();

    self.submit(xthex)
  }
}

#[derive(Clone)]
pub struct Client {
  inner: Arc<RwLock<InnerClient>>,
}

impl Client {
  pub fn connect(rpc: RpcHandler, lookup: &TypeLookup) -> Result<Self, Box<EvalAltResult>> {
    Ok(Self {
      inner: InnerClient::new(rpc, lookup)?,
    })
  }

  pub fn get_metadata(&self) -> Metadata {
    self.inner.read().unwrap().get_metadata()
  }

  pub fn get_signed_extra(&self) -> AdditionalSigned {
    self.inner.read().unwrap().get_signed_extra()
  }

  pub fn get_chain_properties(&self) -> Result<Option<ChainProperties>, Box<EvalAltResult>> {
    self.inner.read().unwrap().get_chain_properties()
  }

  pub fn get_block(&self, hash: Option<BlockHash>) -> Result<Option<Block>, Box<EvalAltResult>> {
    self.inner.read().unwrap().get_block(hash)
  }

  pub fn get_storage_keys_paged(
    &self,
    prefix: &StorageKey,
    count: u32,
    start_key: Option<&StorageKey>,
  ) -> Result<Vec<StorageKey>, Box<EvalAltResult>> {
    self
      .inner
      .read()
      .unwrap()
      .get_storage_keys_paged(prefix, count, start_key)
  }

  pub fn get_storage_by_key(
    &self,
    key: StorageKey,
    at_block: Option<BlockHash>,
  ) -> Result<Option<StorageData>, Box<EvalAltResult>> {
    self.inner.read().unwrap().get_storage_by_key(key, at_block)
  }

  pub fn get_storage_by_keys(
    &self,
    keys: &[StorageKey],
    at_block: Option<BlockHash>,
  ) -> Result<Vec<Option<StorageData>>, Box<EvalAltResult>> {
    self
      .inner
      .read()
      .unwrap()
      .get_storage_by_keys(keys, at_block)
  }

  pub fn get_storage_value(
    &self,
    prefix: &str,
    key_name: &str,
    at_block: Option<BlockHash>,
  ) -> Result<Option<StorageData>, Box<EvalAltResult>> {
    self
      .inner
      .read()
      .unwrap()
      .get_storage_value(prefix, key_name, at_block)
  }

  pub fn get_storage_map(
    &self,
    prefix: &str,
    key_name: &str,
    map_key: Vec<u8>,
    at_block: Option<BlockHash>,
  ) -> Result<Option<StorageData>, Box<EvalAltResult>> {
    self
      .inner
      .read()
      .unwrap()
      .get_storage_map(prefix, key_name, map_key, at_block)
  }

  pub fn get_storage_double_map(
    &self,
    prefix: &str,
    storage_name: &str,
    key1: Vec<u8>,
    key2: Vec<u8>,
    at_block: Option<BlockHash>,
  ) -> Result<Option<StorageData>, Box<EvalAltResult>> {
    self
      .inner
      .read()
      .unwrap()
      .get_storage_double_map(prefix, storage_name, key1, key2, at_block)
  }

  pub fn get_events(&self, block: Option<BlockHash>) -> Result<Dynamic, Box<EvalAltResult>> {
    self.inner.read().unwrap().get_events(block)
  }

  pub fn get_nonce(&self, account: AccountId) -> Result<Option<u32>, Box<EvalAltResult>> {
    self.inner.read().unwrap().get_nonce(account)
  }

  pub fn get_request_block_hash(
    &self,
    token: RequestToken,
  ) -> Result<Option<BlockHash>, Box<EvalAltResult>> {
    self.inner.read().unwrap().get_request_block_hash(token)
  }

  pub fn submit(&self, xthex: String) -> Result<ExtrinsicCallResult, Box<EvalAltResult>> {
    self
      .inner
      .read()
      .unwrap()
      .submit(xthex)
      .map(|(token, xthex)| ExtrinsicCallResult::new(self, token, xthex))
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
      .map(|(token, xthex)| ExtrinsicCallResult::new(self, token, xthex))
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
      .map(|(token, xthex)| ExtrinsicCallResult::new(self, token, xthex))
  }

  pub fn inner(&self) -> Arc<RwLock<InnerClient>> {
    self.inner.clone()
  }
}

#[derive(Clone)]
pub struct ExtrinsicCallResult {
  client: Client,
  loaded: bool,
  token: RequestToken,
  hash: Option<BlockHash>,
  xthex: String,
  idx: Option<u32>,
  events: Option<EventRecords>,
}

impl ExtrinsicCallResult {
  pub fn new(client: &Client, token: RequestToken, xthex: String) -> Self {
    Self {
      client: client.clone(),
      loaded: false,
      token,
      hash: None,
      xthex,
      idx: None,
      events: None,
    }
  }

  fn get_block_hash(&mut self) -> Result<(), Box<EvalAltResult>> {
    if self.loaded {
      return Ok(());
    }

    self.loaded = true;
    self.hash = self.client.get_request_block_hash(self.token)?;

    Ok(())
  }

  pub fn is_in_block(&mut self) -> Result<bool, Box<EvalAltResult>> {
    self.get_block_hash()?;
    Ok(self.hash.is_some())
  }

  pub fn block_hash(&mut self) -> Result<String, Box<EvalAltResult>> {
    self.get_block_hash()?;
    Ok(self.hash.unwrap_or_default().to_string())
  }

  fn load_events(&mut self) -> Result<(), Box<EvalAltResult>> {
    if self.events.is_some() {
      return Ok(());
    }
    self.get_block_hash()?;
    let events = match self.hash {
      Some(hash) => {
        // Load block and find the index of our extrinsic.
        let xt_idx = match self.client.get_block(Some(hash))? {
          Some(block) => block.find_extrinsic(&self.xthex),
          None => None,
        };
        self.idx = xt_idx.map(|idx| idx as u32);
        let mut events = EventRecords::from_dynamic(self.client.get_events(Some(hash))?)?;
        if let Some(idx) = self.idx {
          events.filter(Phase::ApplyExtrinsic(idx));
        }
        events
      }
      None => EventRecords::default(),
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
          .filter(|ev| ev.name.starts_with(prefix))
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
    self.get_block_hash()?;
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
    let _ = self.get_block_hash();
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

pub fn init_engine(
  rpc: &RpcHandler,
  engine: &mut Engine,
  lookup: &TypeLookup,
) -> Result<Client, Box<EvalAltResult>> {
  engine
    .register_type_with_name::<Client>("Client")
    .register_result_fn("submit_unsigned", Client::submit_unsigned)
    .register_type_with_name::<Block>("Block")
    .register_fn("to_string", Block::to_string)
    .register_type_with_name::<EventRecords>("EventRecords")
    .register_fn("to_string", EventRecords::to_string)
    .register_type_with_name::<EventRecord>("EventRecord")
    .register_get("name", EventRecord::name)
    .register_get("args", EventRecord::args)
    .register_fn("to_string", EventRecord::to_string)
    .register_type_with_name::<ExtrinsicCallResult>("ExtrinsicCallResult")
    .register_result_fn("events", ExtrinsicCallResult::events_filtered)
    .register_get_result("events", ExtrinsicCallResult::events)
    .register_get_result("block", ExtrinsicCallResult::block)
    .register_get_result("block_hash", ExtrinsicCallResult::block_hash)
    .register_get_result("result", ExtrinsicCallResult::result)
    .register_get_result("is_success", ExtrinsicCallResult::is_success)
    .register_get_result("is_in_block", ExtrinsicCallResult::is_in_block)
    .register_get("xthex", ExtrinsicCallResult::xthex)
    .register_fn("to_string", ExtrinsicCallResult::to_string);

  let client = Client::connect(rpc.clone(), lookup)?;

  // Get Chain properties.
  let chain_props = client.get_chain_properties()?;
  // Set default ss58 format.
  let ss58_format = chain_props
    .as_ref()
    .and_then(|p| Ss58AddressFormat::try_from(p.ss58_format).ok());
  if let Some(ss58_format) = ss58_format {
    set_default_ss58_version(ss58_format);
  }

  // Get the `tokenDecimals` value from the chain properties.
  let token_decimals = chain_props.as_ref().map(|p| p.token_decimals).unwrap_or(0);
  let balance_scale = 10u128.pow(token_decimals);
  log::info!(
    "token_deciamls: {:?}, balance_scale={:?}",
    token_decimals,
    balance_scale
  );
  lookup.custom_encode("Balance", TypeId::of::<INT>(), move |value, data| {
    let mut val = value.cast::<INT>() as u128;
    val *= balance_scale;
    if data.is_compact() {
      data.encode(Compact::<u128>(val));
    } else {
      data.encode(val);
    }
    Ok(())
  })?;
  lookup.custom_encode("Balance", TypeId::of::<Decimal>(), move |value, data| {
    let mut dec = value.cast::<Decimal>();
    dec *= Decimal::from(balance_scale);
    let val = dec
      .to_u128()
      .ok_or_else(|| format!("Expected unsigned integer"))?;
    if data.is_compact() {
      data.encode(Compact::<u128>(val));
    } else {
      data.encode(val);
    }
    Ok(())
  })?;
  lookup.custom_decode("Balance", move |mut input| {
    let mut val = Decimal::from(u128::decode(&mut input)?);
    val /= Decimal::from(balance_scale);
    Ok(Dynamic::from_decimal(val))
  })?;
  lookup.custom_decode("Compact<Balance>", move |mut input| {
    let num = Compact::<u128>::decode(&mut input)?;
    let mut val = Decimal::from(num.0);
    val /= Decimal::from(balance_scale);
    Ok(Dynamic::from_decimal(val))
  })?;

  Ok(client)
}
