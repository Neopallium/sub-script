use std::sync::{Arc, RwLock};

use sp_core::{sr25519, Pair};
use sp_runtime::{AccountId32, MultiSignature};

use dashmap::DashMap;

use rhai::{Dynamic, Engine, EvalAltResult, INT};

use crate::client::{Client, ExtrinsicCallResult};
use crate::metadata::EncodedCall;

pub type AccountId = AccountId32;

#[derive(Clone)]
pub struct User {
  pub pair: sr25519::Pair,
  pub nonce: u32,
  pub name: String,
  account: AccountId,
  client: Client,
}

impl User {
  fn new(client: Client, name: &str) -> Result<Self, Box<EvalAltResult>> {
    log::info!("New user: {}", name);
    let seed = format!("//{}", name);
    let pair = sr25519::Pair::from_string(&seed, None).map_err(|e| format!("{:?}", e))?;
    let account = AccountId::new(pair.public().into());
    Ok(Self {
      name: name.into(),
      pair,
      account,
      nonce: 0u32,
      client,
    })
  }

  pub fn public(&self) -> sr25519::Public {
    self.pair.public()
  }

  pub fn acc(&self) -> AccountId {
    self.account.clone()
  }

  fn nonce(&self) -> INT {
    self.nonce as INT
  }

  pub fn sign_data(&self, data: Vec<u8>) -> MultiSignature {
    MultiSignature::Sr25519(self.pair.sign(&data[..]))
  }

  pub fn submit_call(
    &mut self,
    call: EncodedCall,
  ) -> Result<ExtrinsicCallResult, Box<EvalAltResult>> {
    // Check if we need to load the `nonce` for this user.
    if self.nonce == 0u32 {
      self.nonce = self.client.get_nonce(self.acc())?.unwrap_or(0);
    }
    let res = self.client.submit_call(self, call)?;

    // Only update the nonce if the extrinsic executed.
    self.nonce += 1;

    Ok(res)
  }

  fn to_string(&self) -> String {
    self.name.clone()
  }
}

#[derive(Clone)]
pub struct SharedUser(Arc<RwLock<User>>);

impl SharedUser {
  pub fn public(&self) -> sr25519::Public {
    self.0.read().unwrap().public()
  }

  pub fn acc(&mut self) -> AccountId {
    self.0.read().unwrap().acc()
  }

  fn nonce(&mut self) -> INT {
    self.0.read().unwrap().nonce()
  }

  pub fn sign_data(&mut self, data: Vec<u8>) -> MultiSignature {
    self.0.read().unwrap().sign_data(data)
  }

  pub fn submit_call(
    &mut self,
    call: EncodedCall,
  ) -> Result<ExtrinsicCallResult, Box<EvalAltResult>> {
    self.0.write().unwrap().submit_call(call)
  }

  fn to_string(&mut self) -> String {
    self.0.read().unwrap().to_string()
  }
}

pub struct InnerUsers {
  users: DashMap<String, Dynamic>,
  account_map: DashMap<AccountId, Dynamic>,
  client: Client,
}

impl InnerUsers {
  pub fn new(client: Client) -> Self {
    Self {
      users: DashMap::new(),
      account_map: DashMap::new(),
      client,
    }
  }

  pub fn find_by_account(&self, acc: AccountId) -> Dynamic {
    self.account_map.get(&acc).as_deref().cloned().unwrap_or(Dynamic::UNIT)
  }

  fn get_user(&self, name: String) -> Result<Dynamic, Box<EvalAltResult>> {
    // Try save user.  If another thread generated the user first, then use that user.
    use dashmap::mapref::entry::Entry;
    Ok(match self.users.entry(name) {
      Entry::Occupied(entry) => entry.get().clone(),
      Entry::Vacant(entry) => {
        // Generate new user.
        let user = User::new(self.client.clone(), entry.key())?;
        let acc = user.acc();
        // Create a shared wrapper for the user.
        let shared = Dynamic::from(SharedUser(Arc::new(RwLock::new(user))));

        self.account_map.insert(acc, shared.clone());
        entry.insert(shared.clone());
        shared
      }
    })
  }
}

#[derive(Clone)]
pub struct Users(Arc<InnerUsers>);

impl Users {
  pub fn new(client: Client) -> Self {
    Self(Arc::new(InnerUsers::new(client)))
  }

  pub fn find_by_account(&mut self, acc: AccountId) -> Dynamic {
    self.0.find_by_account(acc)
  }

  fn get_user(&mut self, name: String) -> Result<Dynamic, Box<EvalAltResult>> {
    self.0.get_user(name)
  }
}

pub fn init_engine(engine: &mut Engine, client: &Client) -> Users {
  engine
    .register_type_with_name::<SharedUser>("User")
    .register_get("acc", SharedUser::acc)
    .register_get("nonce", SharedUser::nonce)
    .register_fn("to_string", SharedUser::to_string)
    .register_fn("sign", SharedUser::sign_data)
    .register_result_fn("submit", SharedUser::submit_call)
    .register_type_with_name::<AccountId>("AccountId")
    .register_fn("to_string", |acc: &mut AccountId| acc.to_string())
    .register_fn("==", |acc1: AccountId, acc2: AccountId| acc1 == acc2)
    .register_type_with_name::<Users>("Users")
    .register_fn("new_users", Users::new)
    .register_fn("find_by_account", Users::find_by_account)
    .register_indexer_get_result(Users::get_user);
  Users::new(client.clone())
}
