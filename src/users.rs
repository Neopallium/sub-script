use std::collections::HashMap;

use sp_core::sr25519;
use sp_core::Pair;

use rhai::{Dynamic, Engine, EvalAltResult};

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

  fn to_string(&mut self) -> String {
    self.name.clone()
  }
}

#[derive(Clone)]
pub struct Users {
  users: HashMap<String, Dynamic>,
}

impl Users {
  fn new() -> Self {
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

pub fn init_engine(engine: &mut Engine) -> Dynamic {
  engine
    .register_type_with_name::<User>("User")
    .register_result_fn("connect", User::connect)
    .register_fn("to_string", User::to_string)
    .register_fn("to_debug", User::to_string)
    .register_type_with_name::<Users>("Users")
    .register_fn("new_users", Users::new)
    .register_indexer_get_result(Users::get_user);

  Dynamic::from(Users::new()).into_shared()
}
