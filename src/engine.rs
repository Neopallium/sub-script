use std::collections::HashMap;

pub use rhai::{Dynamic, Engine, EvalAltResult, Position, Scope};

#[cfg(not(feature = "no_optimize"))]
use rhai::OptimizationLevel;

use crate::{client, metadata, plugins, types, users};

pub fn eprint_error(input: &str, mut err: EvalAltResult) {
  fn eprint_line(lines: &[&str], pos: Position, err_msg: &str) {
    let line = pos.line().unwrap();
    let line_no = format!("{}: ", line);

    eprintln!("{}{}", line_no, lines[line - 1]);
    eprintln!(
      "{:>1$} {2}",
      "^",
      line_no.len() + pos.position().unwrap(),
      err_msg
    );
    eprintln!("");
  }

  let lines: Vec<_> = input.split('\n').collect();

  // Print error
  let pos = err.take_position();

  if pos.is_none() {
    // No position
    eprintln!("{}", err);
  } else {
    // Specific position
    eprint_line(&lines, pos, &err.to_string())
  }
}

pub fn init_engine(url: &str) -> Result<(Engine, Scope<'static>), Box<EvalAltResult>> {
  let schema = "schema.json";

  let mut engine = Engine::new();
  let mut globals = HashMap::new();
  let scope = Scope::new();

  #[cfg(not(feature = "no_optimize"))]
  engine.set_optimization_level(OptimizationLevel::Full);
  engine.set_max_expr_depths(64, 64);

  // Initialize types, client, users, metadata and plugins.
  let lookup = types::init_engine(&mut engine, &schema)?;
  let client = client::init_engine(&mut engine, url, &lookup)?;
  let users = users::init_engine(&mut engine, &client);
  metadata::init_engine(&mut engine, &mut globals, &client, &lookup)?;
  plugins::init_engine(&mut engine, &mut globals, &client, &lookup)?;

  globals.insert("CLIENT".into(), Dynamic::from(client));
  globals.insert("USER".into(), Dynamic::from(users));
  globals.insert("Types".into(), Dynamic::from(lookup));
  // Globals Hack.
  engine.on_var(move |name, _, _| {
    let val = globals.get(name).cloned();
    Ok(val)
  });

  Ok((engine, scope))
}
