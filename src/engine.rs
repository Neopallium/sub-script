pub use rhai::{Dynamic, Engine, EvalAltResult, Position};

#[cfg(not(feature = "no_optimize"))]
use rhai::OptimizationLevel;

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

pub fn init_engine() -> Engine {
  let mut engine = Engine::new();

  #[cfg(not(feature = "no_optimize"))]
  engine.set_optimization_level(OptimizationLevel::Full);

  // init modules.
  let users = crate::users::init_engine(&mut engine);

  crate::client::init_engine(&mut engine);

  let api = crate::api::init_engine("ws://127.0.0.1:9944", &mut engine);

  // "Globals"
  engine.on_var(move |name: &str, _, _| match name {
    "USER" => Ok(Some(users.clone())),
    "API" => Ok(Some(api.clone())),
    _ => Ok(None),
  });

  engine
}
