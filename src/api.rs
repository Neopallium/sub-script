use rhai::{Engine};

#[derive(Clone)]
pub struct Api {
}

impl Api {
}

pub fn init_engine(engine: &mut Engine) {
  engine
    .register_type_with_name::<Api>("Api")
    ;
}
