use std::collections::HashMap;

use sp_core::sr25519;
use sp_core::Pair;

use rhai::{Dynamic, Engine, EvalAltResult, Scope};

use crate::client::Client;

#[derive(Clone)]
pub struct User {
  name: String,
  pair: sr25519::Pair,
}

impl User {
  fn new(name: &str) -> Result<Self, Box<EvalAltResult>> {
    eprintln!("New user: {}", name);
    let seed = format!("//{}", name);
    let pair = sr25519::Pair::from_string(&seed, None).map_err(|e| format!("{:?}", e))?;
    Ok(Self {
      name: name.into(),
      pair,
    })
  }

  pub fn connect(&mut self, url: &str) -> Result<Client, Box<EvalAltResult>> {
    Client::connect_with_signer(self.pair.clone(), url)
  }

  pub fn public(&self) -> sr25519::Public {
    self.pair.public()
  }

  pub fn acc(&mut self) -> Account {
    Account(self.public())
  }

  fn seed(&mut self) -> String {
    hex::encode(&self.pair.to_raw_vec())
  }

  fn to_string(&mut self) -> String {
    self.name.clone()
  }
}

#[derive(Clone)]
pub struct Account(pub sr25519::Public);

impl Account {
  pub fn to_string(&mut self) -> String {
    hex::encode(&self.0)
  }
}

#[derive(Clone)]
pub struct Users {
  users: HashMap<String, Dynamic>,
}

impl Users {
  pub fn new() -> Self {
    Self {
      users: HashMap::new(),
    }
  }

  fn get_user(&mut self, name: String) -> Result<Dynamic, Box<EvalAltResult>> {
    use std::collections::hash_map::Entry;
    Ok(match self.users.entry(name) {
      Entry::Occupied(entry) => entry.get().clone(),
      Entry::Vacant(entry) => {
        let user = Dynamic::from(User::new(entry.key())?).into_shared();
        entry.insert(user.clone());
        user
      }
    })
  }
}

pub fn init_engine(engine: &mut Engine) {
  engine
    .register_type_with_name::<User>("User")
    .register_get("acc", User::acc)
    .register_get("seed", User::seed)
    .register_result_fn("connect", User::connect)
    .register_fn("to_string", User::to_string)
    .register_fn("to_debug", User::to_string)
    .register_type_with_name::<Account>("Account")
    .register_fn("to_string", Account::to_string)
    .register_type_with_name::<Users>("Users")
    .register_fn("new_users", Users::new)
    .register_indexer_get_result(Users::get_user);
}

pub fn init_scope(scope: &mut Scope) {
  scope.push_constant("USER", Users::new());
}
