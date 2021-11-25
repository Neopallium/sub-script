use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use sp_core::{sr25519, Pair};
use sp_runtime::AccountId32;

use rhai::{Dynamic, Engine, EvalAltResult, Scope, INT};

use crate::client::{Client, ExtrinsicCallResult};
use crate::metadata::EncodedCall;

pub type AccountId = AccountId32;

#[derive(Clone)]
pub struct User {
  pub pair: sr25519::Pair,
  pub nonce: u32,
  name: String,
  account: AccountId,
  client: Client,
}

impl User {
  fn new(client: Client, name: &str) -> Result<Self, Box<EvalAltResult>> {
    eprintln!("New user: {}", name);
    let seed = format!("//{}", name);
    let pair = sr25519::Pair::from_string(&seed, None).map_err(|e| format!("{:?}", e))?;
    let account = AccountId::new(pair.public().into());
    let nonce = client.get_nonce(account.clone())?.unwrap_or(0);
    Ok(Self {
      name: name.into(),
      pair,
      account,
      nonce,
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

  pub fn submit_call(
    &mut self,
    call: EncodedCall,
  ) -> Result<ExtrinsicCallResult, Box<EvalAltResult>> {
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

#[derive(Clone)]
pub struct Users {
  users: HashMap<String, Dynamic>,
  account_map: HashMap<AccountId, Dynamic>,
  client: Client,
}

impl Users {
  pub fn new(client: Client) -> Self {
    Self {
      users: HashMap::new(),
      account_map: HashMap::new(),
      client,
    }
  }

  pub fn find_by_account(&mut self, acc: AccountId) -> Dynamic {
    self.account_map.get(&acc).cloned().unwrap_or(Dynamic::UNIT)
  }

  fn get_user(&mut self, name: String) -> Result<Dynamic, Box<EvalAltResult>> {
    use std::collections::hash_map::Entry;
    Ok(match self.users.entry(name) {
      Entry::Occupied(entry) => entry.get().clone(),
      Entry::Vacant(entry) => {
        let user = User::new(self.client.clone(), entry.key())?;
        let acc = user.acc();
        // Create a shared wrapper for the user.
        let shared = Dynamic::from(SharedUser(Arc::new(RwLock::new(user))));
        entry.insert(shared.clone());
        self.account_map.insert(acc, shared.clone());
        shared
      }
    })
  }
}

pub fn init_engine(engine: &mut Engine) {
  engine
    .register_type_with_name::<SharedUser>("User")
    .register_get("acc", SharedUser::acc)
    .register_get("nonce", SharedUser::nonce)
    .register_fn("to_string", SharedUser::to_string)
    .register_result_fn("submit", SharedUser::submit_call)
    .register_type_with_name::<AccountId>("AccountId")
    .register_fn("to_string", |acc: &mut AccountId| acc.to_string())
    .register_fn("==", |acc1: AccountId, acc2: AccountId| acc1 == acc2)
    .register_type_with_name::<Users>("Users")
    .register_fn("new_users", Users::new)
    .register_fn("find_by_account", Users::find_by_account)
    .register_indexer_get_result(Users::get_user);
}

pub fn init_scope(client: &Client, scope: &mut Scope) {
  scope.push_constant("USER", Users::new(client.clone()));
}
