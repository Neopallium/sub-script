use std::collections::HashMap;

use sp_core::sr25519;
use sp_core::Pair;

use rhai::{Dynamic, Engine};

#[derive(Clone)]
pub struct User {
  name: String,
  pair: sr25519::Pair,
}

impl User {
  fn new(name: &str) -> Self {
    eprintln!("New user: {}", name);
    let seed = format!("//{}", name);
    let pair = sr25519::Pair::from_string(&seed, None)
      .expect("Failed to generate user");
    Self {
      name: name.into(),
      pair
    }
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

  fn get_user(&mut self, name: String) -> Dynamic {
    self.users.entry(name).or_insert_with_key(|name| {
      Dynamic::from(User::new(name)).into_shared()
    }).clone()
  }
}

pub fn init_engine(engine: &mut Engine) -> Users {
  engine
    .register_type_with_name::<User>("User")
    .register_fn("to_string", User::to_string)
    .register_fn("to_debug", User::to_string)

    .register_type_with_name::<Users>("Users")
    .register_fn("new_users", Users::new)
    .register_indexer_get(Users::get_user)
    ;

  Users::new()
}
