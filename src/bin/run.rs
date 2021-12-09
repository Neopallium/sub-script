use sub_script::engine::*;

use std::path::PathBuf;
use std::{fs::File, io::Read, process::exit};

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

  let mut contents = String::new();

  let filename = match opt.script.as_path().canonicalize() {
    Err(err) => {
      eprintln!("Error script file path: {:?}\n{}", opt.script, err);
      exit(1);
    }
    Ok(f) => match f.strip_prefix(std::env::current_dir().unwrap().canonicalize().unwrap()) {
      Ok(f) => f.into(),
      _ => f,
    },
  };

  let mut f = match File::open(&filename) {
    Err(err) => {
      eprintln!(
        "Error reading script file: {}\n{}",
        filename.to_string_lossy(),
        err
      );
      exit(1);
    }
    Ok(f) => f,
  };

  contents.clear();

  if let Err(err) = f.read_to_string(&mut contents) {
    eprintln!(
      "Error reading script file: {}\n{}",
      filename.to_string_lossy(),
      err
    );
    exit(1);
  }

  let contents = if contents.starts_with("#!") {
    // Skip shebang
    &contents[contents.find('\n').unwrap_or(0)..]
  } else {
    &contents[..]
  };

  let engine_opts = opt.into_engine_opts();
  let (engine, mut scope) =
    init_engine(&engine_opts).map_err(|e| anyhow!("Failed to initial engine: {:?}", e))?;

  if let Err(err) = engine
    .compile(contents)
    .map_err(|err| err.into())
    .and_then(|mut ast| {
      ast.set_source(filename.to_string_lossy().to_string());
      engine.run_ast_with_scope(&mut scope, &ast)
    })
  {
    let filename = filename.to_string_lossy();

    eprintln!("{:=<1$}", "", filename.len());
    eprintln!("{}", filename);
    eprintln!("{:=<1$}", "", filename.len());
    eprintln!("");

    eprint_error(contents, *err);
  }

  Ok(())
}
