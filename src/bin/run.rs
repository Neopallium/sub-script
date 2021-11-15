use sub_script::engine::*;

use std::{env, fs::File, io::Read, path::Path, process::exit};

use anyhow::{anyhow, Result};

fn main() -> Result<()> {
  dotenv::dotenv().ok();
  env_logger::init();

  let url = "ws://127.0.0.1:9944";

  let mut contents = String::new();

  let (engine, mut scope) = init_engine(&url)
    .map_err(|e| anyhow!("Failed to initial engine: {:?}", e))?;

  for filename in env::args().skip(1) {
    let filename = match Path::new(&filename).canonicalize() {
      Err(err) => {
        eprintln!("Error script file path: {}\n{}", filename, err);
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
  }

  Ok(())
}
