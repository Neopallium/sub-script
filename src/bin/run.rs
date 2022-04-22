use sub_script::engine::*;

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "sub-script")]
struct Opt {
  #[structopt(short, env = "NODE_URL", default_value = "ws://127.0.0.1:9944")]
  url: String,

  #[structopt(short, env = "SUBSTRATE_TYPES", default_value = "init_types.json")]
  substrate_types: String,

  #[structopt(short, env = "CUSTOM_TYPES", default_value = "schema.json")]
  custom_types: String,

  #[structopt(name = "SCRIPT", parse(from_os_str))]
  script: PathBuf,

  #[structopt(name = "arg")]
  args: Vec<String>,
}

impl Opt {
  fn into_engine_opts(self) -> EngineOptions {
    EngineOptions {
      url: self.url,
      substrate_types: self.substrate_types,
      custom_types: self.custom_types,
      args: self.args,
    }
  }
}

fn main() -> Result<()> {
  dotenv::dotenv().ok();
  env_logger::init();

  let opt = Opt::from_args();

  let script = opt.script.clone();

  let engine_opts = opt.into_engine_opts();
  let engine =
    init_engine(&engine_opts).map_err(|e| anyhow!("Failed to initial engine: {:?}", e))?;

  let mut scope = engine.args_to_scope(&engine_opts.args[..]);

  match engine.run_file_with_scope(&mut scope, script.clone()) {
    Err(err) => {
      eprint_script_error(&script, *err);
    }
    _ => (),
  }

  Ok(())
}
