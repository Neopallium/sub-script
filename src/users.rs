use std::collections::HashMap;

use sp_core::{sr25519, Pair};
use sp_runtime::AccountId32;

use rhai::{Dynamic, Engine, EvalAltResult, Scope};

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

  pub fn acc(&mut self) -> AccountId {
    self.account.clone()
  }

  fn seed(&mut self) -> String {
    hex::encode(&self.pair.to_raw_vec())
  }

  pub fn submit_call(&mut self, call: EncodedCall) -> Result<ExtrinsicCallResult, Box<EvalAltResult>> {
    let res = self.client.submit_call(self, call)?;

    // Only update the nonce if the extrinsic executed.
    self.nonce += 1;

    Ok(res)
  }

  fn to_string(&mut self) -> String {
    self.name.clone()
  }
}

#[derive(Clone)]
pub struct Users {
  users: HashMap<String, Dynamic>,
  client: Client,
}

impl Users {
  pub fn new(client: Client) -> Self {
    Self {
      users: HashMap::new(),
      client,
    }
  }

  fn get_user(&mut self, name: String) -> Result<Dynamic, Box<EvalAltResult>> {
    use std::collections::hash_map::Entry;
    Ok(match self.users.entry(name) {
      Entry::Occupied(entry) => entry.get().clone(),
      Entry::Vacant(entry) => {
        let user = User::new(self.client.clone(), entry.key())?;
        let val = Dynamic::from(user).into_shared();
        entry.insert(val.clone());
        val
      }
    })
  }
}

pub fn init_engine(engine: &mut Engine) {
  engine
    .register_type_with_name::<User>("User")
    .register_get("acc", User::acc)
    .register_get("seed", User::seed)
    .register_fn("to_string", User::to_string)
    .register_fn("to_debug", User::to_string)
    .register_result_fn("submit", User::submit_call)
    .register_type_with_name::<AccountId>("AccountId")
    .register_fn("to_string", |acc: &mut AccountId| acc.to_string())
    .register_type_with_name::<Users>("Users")
    .register_fn("new_users", Users::new)
    .register_indexer_get_result(Users::get_user);
}

pub fn init_scope(client: &Client, scope: &mut Scope) {
  scope.push_constant("USER", Users::new(client.clone()));
}
