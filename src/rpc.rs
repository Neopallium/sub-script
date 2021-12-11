use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;

use serde::Deserialize;
use serde_json::{json, Value};

use dashmap::DashMap;

use rhai::serde::from_dynamic;
use rhai::{Dynamic, Engine, EvalAltResult};

use ws::{Factory, Handler, Message, Sender, WebSocket};

#[derive(Debug, Deserialize)]
pub struct RpcError {
  pub code: i64,
  pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct RpcRespParams {
  result: Option<Value>,
  subscription: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct RpcResp {
  jsonrpc: String,
  pub error: Option<RpcError>,

  // Request response.
  id: Option<u64>,
  result: Option<Value>,

  // Subscription response.
  method: Option<Value>,
  params: Option<RpcRespParams>,
}

pub struct Request {
  pub id: u64,
  pub subscription: Option<Subscription>,
  pub result: Option<Value>,
}

impl Request {
  fn is_subscription(&self) -> bool {
    self.subscription.is_some()
  }
}

pub struct Subscription {
  pub topic: Option<String>,
  pub unsub: String,
}

pub struct InnerRpcClient {
  url: String,
  next_id: AtomicU64,
  requests: DashMap<u64, Request>,
  subscriptions: DashMap<String, u64>,
  out: RwLock<Option<Sender>>,
}

impl InnerRpcClient {
  fn new(url: &str) -> Arc<Self> {
    Arc::new(Self {
      url: url.into(),
      next_id: 1u64.into(),
      requests: DashMap::new(),
      subscriptions: DashMap::new(),
      out: RwLock::new(None),
    })
  }

  pub fn call_method(&self, method: &str, params: Value) -> Result<(), Box<EvalAltResult>> {
    let req = self.new_request(None);
    self.send(method, params, req)
  }

  pub fn subscribe(&self, method: &str, params: Value, unsub: &str) -> Result<(), Box<EvalAltResult>> {
    let req = self.new_request(Some(Subscription {
      topic: None,
      unsub: unsub.into(),
    }));
    self.send(method, params, req)
  }

  fn get_next_id(&self) -> u64 {
    self.next_id.fetch_add(1, Ordering::Relaxed)
  }

  fn new_request(&self, subscription: Option<Subscription>) -> u64 {
    let id = self.get_next_id();
    let request = Request {
      id,
      subscription,
      result: None,
    };
    self.requests.insert(id, request);
    id
  }

  fn send(&self, method: &str, params: Value, id: u64) -> Result<(), Box<EvalAltResult>> {
    let msg = json!({
      "jsonrpc": "2.0",
      "id": id,
      "method": method,
      "params": params,
    }).to_string();
    eprintln!("send_msg({:?})", msg);
    let out = self.out.read().unwrap();
    match &*out {
      Some(out) => {
        out.send(msg).map_err(|e| e.to_string())?;
      }
      None => {
        eprintln!("Not connected yet.");
      }
    }
    Ok(())
  }

  fn set_out(&self, ws: Sender) {
    let mut out = self.out.write().unwrap();
    *out = Some(ws);
  }

  fn get_subscription_id(&self, topic: Option<&str>) -> Option<u64> {
    topic.and_then(|topic| self.subscriptions.get(topic))
      .map(|id| *id)
  }

  fn update_request(&self, id: u64, result: Option<Value>, topic: Option<&str>) -> Result<(), ws::Error> {
    match self.requests.get_mut(&id) {
      Some(mut req) => {
        if req.is_subscription() && topic.is_none() {
          eprintln!("Subscription update: {:?}", result);
          // Subscribe started.  (result == topic).
          if let Some(topic) = result.as_ref().and_then(|v| v.as_str()) {
            eprintln!("Map subscription to request id: {} -> {}", topic, id);
            // Map subscription topic to request id.
            self.subscriptions.insert(topic.into(), id);
          } else {
            eprintln!("Unhandled result from subscribe request: {:?}", result);
          }
        } else {
          eprintln!("req update: {:?}", result);
          req.result = result;
        }
      }
      None => {
        eprintln!("Unknown request id: {}", id);
      }
    }
    Ok(())
  }

  fn on_resp(&self, resp: RpcResp) -> Result<(), ws::Error> {
    if resp.jsonrpc != "2.0" {
      eprintln!("Unknown jsonrpc version: {:?}", resp.jsonrpc);
    }
    if let Some(id) = resp.id {
      return self.update_request(id, resp.result, None);
    } else if resp.method.is_some() {
      // Subscription response.
      if let Some(params) = resp.params {
        let topic = params.subscription.as_ref().and_then(|s| s.as_str());
        if let Some(id) = self.get_subscription_id(topic) {
          return self.update_request(id, params.result, topic);
        } else {
          eprintln!("Unknown subscription: {:?}", params);
          return Ok(());
        }
      }
    }
    eprintln!("Unhandled message: {:?}", resp);
    Ok(())
  }

  fn on_message(&self, msg: Message) -> Result<(), ws::Error> {
    eprintln!("on_msg({:?})", msg);
    match &msg {
      Message::Text(msg) => {
        let resp: RpcResp = serde_json::from_str(msg)
          .map_err(|e| new_error(e.to_string()))?;
        self.on_resp(resp)?;
      },
      Message::Binary(_) => {
        Err(new_error(format!("Can't handle binary messages yet")))?;
      }
    }
    Ok(())
  }
}

#[derive(Clone)]
pub struct RpcClient(Arc<InnerRpcClient>);

impl std::ops::Deref for RpcClient {
  type Target = InnerRpcClient;
  fn deref(&self) -> &InnerRpcClient {
    &*self.0
  }
}

impl RpcClient {
  pub fn new(url: &str) -> Result<Self, Box<EvalAltResult>> {
    let client = Self(InnerRpcClient::new(url));
    client.spawn().map_err(|e| e.to_string())?;
    Ok(client)
  }

  fn spawn(&self) -> Result<(), ws::Error> {
    let mut ws = WebSocket::new(self.clone())?;
    let url = url::Url::parse(&self.url)
      .map_err(|e| new_error(e.to_string()))?;
    self.set_out(ws.broadcaster());
    ws.connect(url)?;
    thread::Builder::new().name("RpcClient".into()).spawn(move || {
      ws.run()
    })?;
    Ok(())
  }
}

impl Handler for RpcClient {
  fn on_message(&mut self, msg: Message) -> Result<(), ws::Error> {
    self.0.on_message(msg)
  }
}

impl Factory for RpcClient {
  type Handler = RpcClient;

  fn connection_made(&mut self, ws: Sender) -> RpcClient {
    self.set_out(ws);
    self.clone()
  }
}

struct InnerRpcManager {
  clients: DashMap<String, RpcClient>,
}

#[derive(Clone)]
pub struct RpcManager(Arc<InnerRpcManager>);

impl RpcManager {
  pub fn new() -> Self {
    Self(Arc::new(InnerRpcManager {
      clients: DashMap::new(),
    }))
  }

  pub fn get_client(&self, url: &str) -> Result<RpcClient, Box<EvalAltResult>> {
    if let Some(client) = self.0.clients.get(url) {
      return Ok(client.clone());
    }
    let client = RpcClient::new(url)?;
    self.0.clients.insert(url.into(), client.clone());
    Ok(client)
  }
}

fn new_error(msg: String) -> ws::Error {
  ws::Error::new(ws::ErrorKind::Internal, msg)
}

pub fn init_engine(
  engine: &mut Engine,
  _url: &str,
) -> Result<RpcManager, Box<EvalAltResult>> {
  engine
    .register_type_with_name::<RpcClient>("RpcClient")
    .register_result_fn("new_rpc_client", RpcClient::new)
    .register_result_fn("call_method", |client: &mut RpcClient, method: &str, params: Dynamic| {
      let params: Value = from_dynamic(&params)?;
      client.call_method(method, params)
    })
    .register_result_fn("subscribe", |client: &mut RpcClient, method: &str, params: Dynamic, unsub: &str| {
      let params: Value = from_dynamic(&params)?;
      client.subscribe(method, params, unsub)
    })
    .register_type_with_name::<RpcManager>("RpcManager")
    .register_result_fn("get_client", |rpc: &mut RpcManager, url: &str| rpc.get_client(url));

  let rpc = RpcManager::new();
  Ok(rpc)
}
