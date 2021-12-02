use std::any::TypeId;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::convert::TryFrom;

use byteorder::{LittleEndian, WriteBytesExt};

use rhai::{Dynamic, Engine, EvalAltResult, Scope};

use sp_core::{ed25519, sr25519};
use sp_runtime::generic;

use substrate_api_client::extrinsic::xt_primitives::*;

use ledger_apdu::{APDUAnswer, APDUCommand, APDUErrorCodes};
use ledger_transport::APDUTransport;

use sp_core::Encode;

use crate::client::{Client, ExtrinsicCallResult};
use crate::users::AccountId;
use crate::types::TypeLookup;
use crate::metadata::EncodedCall;

pub const HIGH_BIT: u32 = 0x8000_0000;
pub const CHUNK_SIZE: usize = 250;

// Commands.
pub const INS_GET_ADDR: u8 = 0x01;
pub const INS_SIGN: u8 = 0x02;

// SIGN P1:
pub const SIGN_INIT: u8 = 0x00;
pub const SIGN_ADD: u8 = 0x01;
pub const SIGN_LAST: u8 = 0x02;
// Scheme P2:
pub const SCHEME_ED25519: u8 = 0x00;
pub const SCHEME_SR25519: u8 = 0x01;

// SLIP0044
pub const SLIP0044_POLYMESH: u32 = 595;

// APP
pub const APP_POLYMESH: u8 = 0x91;

#[derive(Clone)]
pub struct Ledger {
  transport: Arc<APDUTransport>,
}

impl Ledger {
  pub fn new_hid() -> Result<Self, Box<EvalAltResult>> {
    let transport = ledger::TransportNativeHID::new()
      .map_err(|e| e.to_string())?;
    Ok(Self {
      transport: Arc::new(APDUTransport::new(transport))
    })
  }

  pub fn exchange(&self, command: APDUCommand) -> Result<APDUAnswer, Box<EvalAltResult>> {
    log::debug!("Ledger cmd: {:?}", command);
    Ok(futures::executor::block_on(
      self.transport.exchange(&command)
    ).map_err(|e| e.to_string())?)
  }
}

#[derive(Clone, Default, Debug)]
pub struct AddressBip44 {
  slip0044: u32,
  account: u32,
  change: u32,
  address_index: u32,
}

impl AddressBip44 {
  pub fn new(slip0044: u32, account: u32, change: u32, address_index: u32) -> Self {
    Self {
      slip0044,
      account,
      change,
      address_index
    }
  }

  pub fn encode(&self) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.write_u32::<LittleEndian>(HIGH_BIT | 44).unwrap();
    buf.write_u32::<LittleEndian>(HIGH_BIT | self.slip0044).unwrap();
    buf.write_u32::<LittleEndian>(HIGH_BIT | self.account).unwrap();
    buf.write_u32::<LittleEndian>(HIGH_BIT | self.change).unwrap();
    buf.write_u32::<LittleEndian>(HIGH_BIT | self.address_index).unwrap();
    buf
  }
}

#[derive(Clone)]
pub struct SubstrateApp {
  ledger: Ledger,
  client: Client,
  cla: u8,
  slip0044: u32,
  account_id: AccountId,
  address: AddressBip44,
  scheme: u8,
  nonce: u32,
}

impl SubstrateApp {
  pub fn new_polymesh(ledger: Ledger, client: Client) -> Result<Self, Box<EvalAltResult>> {
    let mut app = Self {
      ledger,
      client,
      cla: APP_POLYMESH,
      slip0044: SLIP0044_POLYMESH,
      account_id: Default::default(),
      address: AddressBip44::new(SLIP0044_POLYMESH, 0, 0, 0),
      scheme: SCHEME_ED25519,
      nonce: 0,
    };
    app.update_account()?;

    Ok(app)
  }

  fn send_cmd(&self, ins: u8, p1: u8, p2: u8, data: Vec<u8>) -> Result<Vec<u8>, Box<EvalAltResult>> {
    Ok(Self::is_error(self.ledger.exchange(APDUCommand {
      cla: self.cla,
      ins,
      p1,
      p2,
      data,
    })?)?)
  }

  fn update_account(&mut self) -> Result<(), Box<EvalAltResult>> {
    // Initial command.
    let res = self.send_cmd(
      INS_GET_ADDR,
      0x00,
      self.scheme,
      self.address.encode(),
    )?;

    let len = res.len();
    log::debug!("-- GET_ADDR: len={}", len);
    self.account_id = AccountId::try_from(&res[0..32]).unwrap();
    let address = String::from_utf8_lossy(&res[32..len]);
    log::debug!("  -- address: {:?}", address);

    self.nonce = self.client.get_nonce(self.account_id.clone())?.unwrap_or(0);
    log::debug!("  Leaded nonce[{}] for account: {:?}", self.nonce, self.account_id);
    Ok(())
  }

  pub fn get_account_id(&self) -> AccountId {
    self.account_id.clone()
  }

  pub fn set_address(&mut self, account: u32, change: u32, address_index: u32) -> Result<(), Box<EvalAltResult>> {
    self.address = AddressBip44::new(self.slip0044, account, change, address_index);
    self.update_account()
  }

  fn is_error(res: APDUAnswer) -> Result<Vec<u8>, Box<EvalAltResult>> {
    if res.retcode == APDUErrorCodes::NoError as u16 {
      Ok(res.data)
    } else {
      log::error!("Ledger: error: code={:#X?}", res.retcode);
      Err(ledger_apdu::map_apdu_error_description(res.retcode).into())
    }
  }

  pub fn sign(&self, data: Vec<u8>) -> Result<Vec<u8>, Box<EvalAltResult>> {
    // Initial command.  First chunk.
    let mut resp = self.send_cmd(
      INS_SIGN,
      SIGN_INIT,
      self.scheme,
      self.address.encode(),
    )?;

    // Message chunks.
    let mut chunks = data.chunks(CHUNK_SIZE).peekable();
    while let Some(chunk) = chunks.next() {
      let p1 = if chunks.peek().is_some() {
        SIGN_ADD
      } else {
        SIGN_LAST
      };
      resp = self.send_cmd(
        INS_SIGN,
        p1,
        self.scheme,
        chunk.into(),
      )?;
    }

    Ok(resp)
  }

  pub fn submit_call(
    &mut self,
    call: EncodedCall,
  ) -> Result<ExtrinsicCallResult, Box<EvalAltResult>> {
    let era_nonce = GenericExtra::new(generic::Era::Immortal, self.nonce);
    let call = call.into_call();
    let payload = SignedPayload::from_raw(
      &call, &era_nonce, self.client.get_signed_extra()
    );

    let signature = self.sign(payload.encode())?;
    log::debug!("signature res: len={}, sig[0]={}", signature.len(), signature[0]);
    let sig = match self.scheme {
      SCHEME_ED25519 => {
        ed25519::Signature::from_slice(&signature[1..]).into()
      }
      SCHEME_SR25519 => {
        sr25519::Signature::from_slice(&signature[1..]).into()
      }
      scheme => {
        panic!("Unsupported signature scheme: {}", scheme);
      }
    };

    let xt = UncheckedExtrinsicV4::new_signed(
      call,
      GenericAddress::from(self.account_id.clone()),
      sig,
      era_nonce,
    );
    let xthex = xt.hex_encode();

    let res = self.client.submit(xthex)?;

    // Only update the nonce if the call was executed.
    self.nonce += 1;

    Ok(res)
  }
}

#[derive(Clone)]
pub struct SharedApp(Arc<RwLock<SubstrateApp>>);

impl SharedApp {
  pub fn acc(&mut self) -> AccountId {
    self.0.read().unwrap().get_account_id()
  }

  pub fn submit_call(
    &mut self,
    call: EncodedCall,
  ) -> Result<ExtrinsicCallResult, Box<EvalAltResult>> {
    self.0.write().unwrap().submit_call(call)
  }
}

#[derive(Clone)]
pub struct LedgerApps {
  ledgers: HashMap<String, Ledger>,
  apps: HashMap<String, SharedApp>,
  client: Client,
}

impl LedgerApps {
  pub fn new(client: Client) -> Self {
    Self {
      ledgers: HashMap::new(),
      apps: HashMap::new(),
      client,
    }
  }

  fn get_ledger(&mut self, ledger_type: &str) -> Result<Ledger, Box<EvalAltResult>> {
    use std::collections::hash_map::Entry;
    Ok(match self.ledgers.entry(ledger_type.into()) {
      Entry::Occupied(entry) => entry.get().clone(),
      Entry::Vacant(entry) => {
        log::info!("Create new ledger: {}", ledger_type);
        let ledger = match ledger_type {
          "HID" => {
            Ledger::new_hid()?
          }
          _ => {
            panic!("Unsupported ledger type: {}", ledger_type);
          }
        };
        entry.insert(ledger.clone());
        ledger
      }
    })
  }

  fn get_app(&mut self, ledger_app: &str) -> Result<Dynamic, Box<EvalAltResult>> {
    use std::collections::hash_map::Entry;

    // Normalize name for lookup.
    let (app_name, ledger_type) = ledger_app.split_once(':')
      .map(|(app, ledger)| (app.trim(), ledger.trim()))
      .ok_or_else(|| format!("Failed to parse ledger_app: {}", ledger_app))?;
    let parsed_name = format!("{}:{}", app_name, ledger_type);

    // Get ledger.
    let ledger = self.get_ledger(ledger_type)?;

    Ok(match self.apps.entry(parsed_name) {
      Entry::Occupied(entry) => Dynamic::from(entry.get().clone()),
      Entry::Vacant(entry) => {
        log::info!("Create new ledger app: {}", app_name);
        let app = match app_name {
          "Polymesh" => {
            SubstrateApp::new_polymesh(ledger, self.client.clone())?
          }
          _ => {
            panic!("Unsupported ledger app: {}", app_name);
          }
        };
        let app = SharedApp(Arc::new(RwLock::new(app)));
        entry.insert(app.clone());
        Dynamic::from(app)
      }
    })
  }
}

pub fn init_engine(engine: &mut Engine) {
  engine
    .register_type_with_name::<SharedApp>("LedgerApp")
    .register_get("acc", SharedApp::acc)
    .register_result_fn("submit", SharedApp::submit_call)
    .register_type_with_name::<LedgerApps>("LedgerApps")
    .register_result_fn("get_app", LedgerApps::get_app);
}

pub fn init_scope(
  client: &Client,
  lookup: &TypeLookup,
  _engine: &mut Engine,
  scope: &mut Scope<'_>,
) -> Result<(), Box<EvalAltResult>> {
  scope.push_constant("LedgerApps", LedgerApps::new(client.clone()));

  lookup.custom_encode("AccountId", TypeId::of::<SharedApp>(), |value, data| {
    let mut app = value.cast::<SharedApp>();
    data.encode(app.acc());
    Ok(())
  })?;

  lookup.custom_encode("MultiAddress", TypeId::of::<SharedApp>(), |value, data| {
    let mut app = value.cast::<SharedApp>();
    data.encode(0u8); // MultiAddress::Id
    data.encode(app.acc());
    Ok(())
  })?;

  lookup.custom_encode("Signatory", TypeId::of::<SharedApp>(), |value, data| {
    let mut app = value.cast::<SharedApp>();
    data.encode(1u8); // Signatory::Account
    data.encode(app.acc());
    Ok(())
  })?;

  Ok(())
}
