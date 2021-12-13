use std::collections::HashMap;

pub use rhai::{Dynamic, Engine, EvalAltResult, Position, Scope};

#[cfg(not(feature = "no_optimize"))]
use rhai::OptimizationLevel;

use crate::{client, metadata, plugins, rpc, storage, types, users};

#[derive(Debug, Clone)]
pub struct EngineOptions {
  pub url: String,
  pub substrate_types: String,
  pub custom_types: String,
  pub args: Vec<String>,
}

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

pub fn init_engine(opts: &EngineOptions) -> Result<(Engine, Scope), Box<EvalAltResult>> {
  let mut engine = Engine::new();
  let mut globals = HashMap::new();
  let mut scope = Scope::new();

  #[cfg(not(feature = "no_optimize"))]
  engine.set_optimization_level(OptimizationLevel::Full);
  engine.set_max_expr_depths(64, 64);

  // Initialize types, client, users, metadata and plugins.
  let rpc_manager = rpc::init_engine(&mut engine)?;
  let rpc = rpc_manager.get_client(&opts.url)?;

  let lookup = types::init_engine(&mut engine, &opts)?;
  let client = client::init_engine(&rpc, &mut engine, &lookup)?;
  let users = users::init_engine(&mut engine, &client);
  let metadata = metadata::init_engine(&mut engine, &mut globals, &client, &lookup)?;
  let storage = storage::init_engine(&mut engine, &client, &metadata);
  plugins::init_engine(&mut engine, &mut globals, &client, &lookup)?;

  // Setup globals for easy access.
  globals.insert("CLIENT".into(), Dynamic::from(client));
  globals.insert("RPC_MANAGER".into(), Dynamic::from(rpc_manager));
  globals.insert("RPC".into(), Dynamic::from(rpc));
  globals.insert("Types".into(), Dynamic::from(lookup));
  globals.insert("STORAGE".into(), Dynamic::from(storage));

  // Make sure there is only one shared copy of `Users`.
  let users = Dynamic::from(users).into_shared();
  globals.insert("USER".into(), users);
  // Convert script arguments.
  let args = opts
    .args
    .iter()
    .cloned()
    .map(|arg| Dynamic::from(arg))
    .collect::<Vec<Dynamic>>();
  scope.push("ARG", args);

  // For easier access to globals.
  engine.on_var(move |name, _, _| {
    let val = globals.get(name).cloned();
    Ok(val)
  });

  Ok((engine, scope))
}
