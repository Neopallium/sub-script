use std::sync::Arc;

use sp_core::sr25519;

use substrate_api_client::{Api};

use rhai::{Engine};

#[derive(Clone)]
pub struct Client {
  api: Arc<Api<sr25519::Pair>>,
}

impl Client {
  pub fn connect(url: &str) -> Self {
    let api = Api::new(url.into())
      .expect("Failed to connect");
    Self {
      api: Arc::new(api),
    }
  }

  fn print_metadata(&mut self) {
    self.api.metadata.print_overview();
  }
}

pub fn init_engine(engine: &mut Engine) {
  engine
    .register_type_with_name::<Client>("Client")
    .register_fn("connect", Client::connect)
    .register_fn("print_metadata", Client::print_metadata)
    ;
}
