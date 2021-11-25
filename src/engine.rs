pub use rhai::{Dynamic, Engine, EvalAltResult, Position, Scope};

#[cfg(not(feature = "no_optimize"))]
use rhai::OptimizationLevel;

use crate::{client, metadata, types, users};

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
  let mut scope = Scope::new();

  #[cfg(not(feature = "no_optimize"))]
  engine.set_optimization_level(OptimizationLevel::Full);
  engine.set_max_expr_depths(64, 64);

  // Register types with engine.
  users::init_engine(&mut engine);
  client::init_engine(&mut engine);
  types::init_engine(&mut engine);
  metadata::init_engine(&mut engine);

  // Initialize scope with some globals.
  let lookup = types::init_scope(&schema, &mut scope)?;
  let client = client::init_scope(url, &lookup, &mut scope)?;
  users::init_scope(&client, &mut scope);
  metadata::init_scope(&client, &lookup, &mut engine, &mut scope)?;

  Ok((engine, scope))
}
