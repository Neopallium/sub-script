pub use rhai::{Dynamic, Engine, EvalAltResult, Position, Scope};

#[cfg(not(feature = "no_optimize"))]
use rhai::OptimizationLevel;

use crate::{
  users,
  client,
  metadata,
  api,
};

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
  let mut engine = Engine::new();
  let mut scope = Scope::new();

  #[cfg(not(feature = "no_optimize"))]
  engine.set_optimization_level(OptimizationLevel::Full);

  // Register types with engine.
  users::init_engine(&mut engine);
  client::init_engine(&mut engine);
  metadata::init_engine(&mut engine);
  api::init_engine(&mut engine);

  // Initialize scope with some globals.
  users::init_scope(&mut scope);
  let md = metadata::init_scope(url, &mut scope)?;
  api::init_scope(md, &mut scope)?;

  Ok((engine, scope))
}
