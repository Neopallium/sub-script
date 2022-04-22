use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::thread::{spawn, JoinHandle};

use std::path::PathBuf;
use std::{fs::File, io::Read};

pub use rhai::{AST, Dynamic, Engine, EvalAltResult, Position, ParseError, Scope};

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

pub fn read_script(script: &PathBuf) -> Result<(String, String), Box<EvalAltResult>> {
  let mut contents = String::new();

  let filename = match script.as_path().canonicalize() {
    Err(err) => {
      Err(format!("Error script file path: {:?}\n{}", script, err))?
    }
    Ok(f) => match f.strip_prefix(std::env::current_dir().unwrap().canonicalize().unwrap()) {
      Ok(f) => f.into(),
      _ => f,
    },
  };

  let mut f = match File::open(&filename) {
    Err(err) => {
      Err(format!(
        "Error reading script file: {}\n{}",
        filename.to_string_lossy(),
        err
      ))?
    }
    Ok(f) => f,
  };

  if let Err(err) = f.read_to_string(&mut contents) {
    Err(format!(
      "Error reading script file: {}\n{}",
      filename.to_string_lossy(),
      err
    ))?;
  }

  let contents = if contents.starts_with("#!") {
    // Skip shebang
    &contents[contents.find('\n').unwrap_or(0)..]
  } else {
    &contents[..]
  };
  let filename = filename.to_string_lossy();

  Ok((contents.to_string(), filename.to_string()))
}

pub fn eprint_script_error(path: &PathBuf, err: EvalAltResult) {
  let (contents, filename) = match read_script(path) {
    Ok(v) => v,
    Err(err) => {
      eprintln!("{:?}", err);
      return;
    }
  };

  eprintln!("{:=<1$}", "", filename.len());
  eprintln!("{}", filename);
  eprintln!("{:=<1$}", "", filename.len());
  eprintln!("");

  eprint_error(&contents, err);
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

#[derive(Clone)]
pub struct TaskHandle(Arc<RwLock<Option<JoinHandle<Result<Dynamic, Box<EvalAltResult>>>>>>);

impl TaskHandle {
  fn new(handle: JoinHandle<Result<Dynamic, Box<EvalAltResult>>>) -> Self {
     Self(Arc::new(RwLock::new(Some(handle))))
  }

  pub fn join(&mut self) -> Result<Dynamic, Box<EvalAltResult>> {
    match self.0.write().unwrap().take() {
      Some(handle) => {
        handle.join().map_err(|err| format!("Failed to join thread: {:?}", err))?
      }
      _ => Err(format!("Already joined task"))?,
    }
  }
}

#[derive(Clone)]
pub struct SharedEngine(Arc<RwLock<Engine>>);

impl SharedEngine {
  fn new(engine: Engine) -> Self {
     Self(Arc::new(RwLock::new(engine)))
  }

  pub fn compile(&self, script: &str) -> Result<AST, Box<EvalAltResult>> {
    Ok(self.0.read().unwrap().compile(script)?)
  }

  pub fn compile_file(&self, path: PathBuf) -> Result<AST, Box<EvalAltResult>> {
    let (contents, filename) = read_script(&path)?;
    let mut ast = self.0.read().unwrap().compile(contents)?;
    ast.set_source(filename);
    Ok(ast)
  }

  pub fn run_ast_with_scope(&self, scope: &mut Scope, ast: &AST) -> Result<(), Box<EvalAltResult>> {
    self.0.read().unwrap().run_ast_with_scope(scope, ast)
  }

  pub fn eval_ast_with_scope(&self, scope: &mut Scope, ast: &AST) -> Result<Dynamic, Box<EvalAltResult>> {
    self.0.read().unwrap().eval_ast_with_scope(scope, ast)
  }

  pub fn run_file_with_scope(&self, scope: &mut Scope, path: PathBuf) -> Result<(), Box<EvalAltResult>> {
    let ast = self.compile_file(path)?;
    self.0.read().unwrap().run_ast_with_scope(scope, &ast)
  }

  pub fn spawn_task(&mut self, script: &str) -> Result<TaskHandle, Box<EvalAltResult>> {
    let ast = self.compile(script)?;
    self.spawn_task_ast_args(ast, Dynamic::UNIT)
  }

  pub fn spawn_task_args(&mut self, script: &str, args: Dynamic) -> Result<TaskHandle, Box<EvalAltResult>> {
    let ast = self.compile(script)?;
    self.spawn_task_ast_args(ast, args)
  }

  pub fn spawn_file_task(&mut self, file: &str) -> Result<TaskHandle, Box<EvalAltResult>> {
    let ast = self.compile_file(file.into())?;
    self.spawn_task_ast_args(ast, Dynamic::UNIT)
  }

  pub fn spawn_file_task_args(&mut self, file: &str, args: Dynamic) -> Result<TaskHandle, Box<EvalAltResult>> {
    let ast = self.compile_file(file.into())?;
    self.spawn_task_ast_args(ast, args)
  }

  fn spawn_task_ast_args(&mut self, ast: AST, args: Dynamic) -> Result<TaskHandle, Box<EvalAltResult>> {
    let engine = self.clone();
    let handle = spawn(move || {
        let mut scope = engine.new_scope(args);
        engine.eval_ast_with_scope(&mut scope, &ast)
    });
    Ok(TaskHandle::new(handle))
  }

  fn new_scope(&self, args: Dynamic) -> Scope {
    let mut scope = Scope::new();
    scope.push("ARG", args);
    scope.push("ENGINE", self.clone());
    scope
  }

  pub fn args_to_scope(&self, args: &[String]) -> Scope {
    // Convert script arguments.
    let args = args
      .into_iter()
      .cloned()
      .map(|arg| Dynamic::from(arg))
      .collect::<Vec<Dynamic>>();
  
    self.new_scope(args.into())
  }
}

pub fn init_engine(opts: &EngineOptions) -> Result<SharedEngine, Box<EvalAltResult>> {
  let mut engine = Engine::new();
  let mut globals = HashMap::new();

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

  // For easier access to globals.
  engine.on_var(move |name, _, _| {
    let val = globals.get(name).cloned();
    Ok(val)
  });

  engine
    .register_type_with_name::<SharedEngine>("Engine")
    .register_result_fn("spawn_task", SharedEngine::spawn_task)
    .register_result_fn("spawn_task_args", SharedEngine::spawn_task_args)
    .register_result_fn("spawn_file_task", SharedEngine::spawn_file_task)
    .register_result_fn("spawn_file_task_args", SharedEngine::spawn_file_task_args)
    .register_type_with_name::<TaskHandle>("TaskHandle")
    .register_result_fn("join", TaskHandle::join);

  Ok(SharedEngine::new(engine))
}
