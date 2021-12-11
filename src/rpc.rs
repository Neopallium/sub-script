use std::sync::atomic::{AtomicU16, AtomicU32, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;

use serde::Deserialize;
use serde_json::{json, Value};

use dashmap::DashMap;

use rhai::serde::from_dynamic;
use rhai::{Dynamic, Engine, EvalAltResult};

use ws::{Factory, Handler, Message, WebSocket};

pub type ConnectionId = u16;
pub type RequestId = u32;

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct RequestToken(pub ConnectionId, pub RequestId);

impl RequestToken {
  fn req_id(&self) -> RequestId {
    self.1
  }
}

#[derive(Debug, Clone)]
pub enum ResponseUpdate {
  Reply(RequestToken, Result<Option<Value>, RpcError>),
  Update(RequestToken, Option<Value>),
  Closed(RequestToken),
}

impl ResponseUpdate {
  fn get_token(&self) -> RequestToken {
    match &self {
      ResponseUpdate::Reply(token, _) => *token,
      ResponseUpdate::Update(token, _) => *token,
      ResponseUpdate::Closed(token) => *token,
    }
  }

  fn to_string(&mut self) -> String {
    format!("{:?}", self)
  }
}

type RespSender = crossbeam_channel::Sender<ResponseUpdate>;
type RespReceiver = crossbeam_channel::Receiver<ResponseUpdate>;

#[derive(Debug, Clone, Deserialize)]
pub struct RpcError {
  pub code: i64,
  pub message: String,
}

#[derive(Debug, Deserialize)]
struct RpcRespParams {
  result: Option<Value>,
  subscription: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RpcResp {
  jsonrpc: String,
  error: Option<RpcError>,

  // Request response.
  id: Option<RequestId>,
  result: Option<Value>,

  // Subscription response.
  method: Option<Value>,
  params: Option<RpcRespParams>,
}

struct RpcRequest {
  method: String,
  params: Value,
  is_subscription: bool,
  unsub: Option<String>,
  resp_tx: Option<RespSender>,
}

impl RpcRequest {
  pub fn call_method(method: &str, params: Value, resp_tx: RespSender) -> Self {
    Self {
      method: method.into(),
      params,
      is_subscription: false,
      unsub: None,
      resp_tx: Some(resp_tx),
    }
  }

  pub fn subscribe(method: &str, params: Value, resp_tx: RespSender, unsub: &str) -> Self {
    Self {
      method: method.into(),
      params,
      is_subscription: true,
      unsub: Some(unsub.into()),
      resp_tx: Some(resp_tx),
    }
  }

  pub fn unsubscribe(method: &str, topic: &str) -> Self {
    Self {
      method: method.into(),
      params: json!([topic]),
      is_subscription: false,
      unsub: None,
      resp_tx: None,
    }
  }

  fn into_request(self, id: RequestId) -> (String, RequestData) {
    let msg = json!({
      "jsonrpc": "2.0",
      "id": id,
      "method": self.method,
      "params": self.params,
    });

    let data = RequestData {
      is_subscription: self.is_subscription,
      unsub: self.unsub,
      topic: None,
      resp_tx: self.resp_tx,
    };
    (msg.to_string(), data)
  }
}

struct RequestData {
  is_subscription: bool,
  unsub: Option<String>,
  topic: Option<String>,
  resp_tx: Option<RespSender>,
}

impl RequestData {
  fn send(&self, resp: ResponseUpdate) -> bool {
    if let Some(resp_tx) = &self.resp_tx {
      resp_tx.send(resp).is_ok()
    } else {
      false
    }
  }
}

pub struct Subscription {
  pub topic: Option<String>,
  pub unsub: String,
}

pub struct InnerRpcConnection {
  id: ConnectionId,
  url: String,
  next_id: AtomicU32,
  requests: DashMap<RequestId, RequestData>,
  subscriptions: DashMap<String, RequestId>,
  out: RwLock<Option<ws::Sender>>,
}

impl InnerRpcConnection {
  fn new(id: ConnectionId, url: &str) -> Arc<Self> {
    Arc::new(Self {
      id: id,
      url: url.into(),
      next_id: 1.into(),
      requests: DashMap::new(),
      subscriptions: DashMap::new(),
      out: RwLock::new(None),
    })
  }

  fn get_next_id(&self) -> RequestId {
    self.next_id.fetch_add(1, Ordering::Relaxed) as RequestId
  }

  fn add_request(&self, req: RpcRequest) -> (String, RequestToken) {
    let id = self.get_next_id();
    let token = RequestToken(self.id, id);
    let (msg, data) = req.into_request(id);
    self.requests.insert(id, data);
    (msg, token)
  }

  fn unsubscribe(&self, unsub: &str, topic: &str) -> Result<RequestToken, Box<EvalAltResult>> {
    let req = RpcRequest::unsubscribe(unsub, topic);
    self.send(req)
  }

  fn close_request(&self, token: RequestToken) -> Result<(), Box<EvalAltResult>> {
    let id = token.req_id();
    match self.requests.remove(&id) {
      Some((_, req)) => {
        eprintln!("Close Request: {:?}", token);
        req.send(ResponseUpdate::Closed(token));
        // Make sure to cleanup any subscriptions.
        if let Some(topic) = req.topic {
          self.subscriptions.remove(&topic);
          if let Some(unsub) = req.unsub {
            self.unsubscribe(&unsub, &topic)?;
          }
        }
      }
      None => {
        eprintln!("Unknown request id: {}", id);
      }
    }
    Ok(())
  }

  fn send(&self, req: RpcRequest) -> Result<RequestToken, Box<EvalAltResult>> {
    let (msg, token) = self.add_request(req);
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
    Ok(token)
  }

  fn set_out(&self, ws: ws::Sender) {
    let mut out = self.out.write().unwrap();
    *out = Some(ws);
  }

  fn get_subscription_id(&self, topic: Option<&str>) -> Option<RequestId> {
    topic.and_then(|topic| self.subscriptions.get(topic))
      .map(|id| *id)
  }

  fn request_error(&self, id: RequestId, error: RpcError) -> Result<(), ws::Error> {
    let token = RequestToken(self.id, id);
    match self.requests.remove(&id) {
      Some((_, req)) => {
        eprintln!("Request error: {:?}", error);
        req.send(ResponseUpdate::Reply(token, Err(error)));
        // Make sure to cleanup any subscriptions.
        if let Some(topic) = req.topic {
          self.subscriptions.remove(&topic);
        }
      }
      None => {
        eprintln!("Unknown request id: {}", id);
      }
    }
    Ok(())
  }

  fn request_reply(&self, id: RequestId, result: Option<Value>) -> Result<(), ws::Error> {
    let token = RequestToken(self.id, id);
    match self.requests.get_mut(&id) {
      Some(mut req) if req.is_subscription => {
        eprintln!("Subscription update: {:?}", result);
        // Subscribe started.  (result == topic).
        if let Some(topic) = result.as_ref().and_then(|v| v.as_str()) {
          eprintln!("Map subscription to request id: {} -> {}", topic, id);
          // Map subscription topic to request id.
          self.subscriptions.insert(topic.into(), id);
          req.topic = Some(topic.into());
        } else {
          eprintln!("Unhandled result from subscribe request: {:?}", result);
        }
      }
      Some(req) => {
        eprintln!("Request reply: {:?}", result);
        req.send(ResponseUpdate::Reply(token, Ok(result)));
        // Drop reference `req` so we can remove it without deadlocking.
        drop(req);
        self.requests.remove(&id);
      }
      None => {
        eprintln!("Unknown request id: {}", id);
      }
    }
    Ok(())
  }

  fn request_update(&self, id: RequestId, result: Option<Value>) -> Result<(), ws::Error> {
    match self.requests.get(&id) {
      Some(req) => {
        let token = RequestToken(self.id, id);
        eprintln!("subscription update: {:?}", result);
        let sent = req.send(ResponseUpdate::Update(token, result));

        if !sent {
          // Drop reference `req` so we can remove it without deadlocking.
          drop(req);
          // Try to close and unsubscribe.
          let _ = self.close_request(token);
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
      if let Some(error) = resp.error {
        return self.request_error(id, error);
      } else {
        return self.request_reply(id, resp.result);
      }
    } else if resp.method.is_some() {
      // Subscription response.
      if let Some(params) = resp.params {
        let topic = params.subscription.as_ref().and_then(|s| s.as_str());
        if let Some(id) = self.get_subscription_id(topic) {
          return self.request_update(id, params.result);
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
pub struct RpcConnection(Arc<InnerRpcConnection>);

impl std::ops::Deref for RpcConnection {
  type Target = InnerRpcConnection;
  fn deref(&self) -> &InnerRpcConnection {
    &*self.0
  }
}

impl RpcConnection {
  pub fn new(id: ConnectionId, url: &str) -> Result<Self, Box<EvalAltResult>> {
    let client = Self(InnerRpcConnection::new(id, url));
    client.spawn().map_err(|e| e.to_string())?;
    Ok(client)
  }

  fn spawn(&self) -> Result<(), ws::Error> {
    let mut ws = WebSocket::new(self.clone())?;
    let url = url::Url::parse(&self.url)
      .map_err(|e| new_error(e.to_string()))?;
    self.set_out(ws.broadcaster());
    ws.connect(url)?;
    thread::Builder::new().name("RpcConnection".into()).spawn(move || {
      ws.run()
    })?;
    Ok(())
  }
}

impl Handler for RpcConnection {
  fn on_message(&mut self, msg: Message) -> Result<(), ws::Error> {
    self.0.on_message(msg)
  }
}

impl Factory for RpcConnection {
  type Handler = RpcConnection;

  fn connection_made(&mut self, ws: ws::Sender) -> RpcConnection {
    self.set_out(ws);
    self.clone()
  }
}

pub struct InnerRpcHandler {
  conn: RpcConnection,
  resp_tx: RespSender,
  resp_rx: RespReceiver,
  updates: DashMap<RequestToken, ResponseUpdate>,
}

impl InnerRpcHandler {
  fn new(conn: RpcConnection) -> Arc<Self> {
    let (resp_tx, resp_rx) = crossbeam_channel::unbounded();
    Arc::new(Self {
      conn,
      resp_tx,
      resp_rx,
      updates: DashMap::new(),
    })
  }

  pub fn close_request(&self, token: RequestToken) -> Result<(), Box<EvalAltResult>> {
    self.conn.close_request(token)
  }

  pub fn get_response(&self, token: RequestToken) -> Result<ResponseUpdate, Box<EvalAltResult>> {
    if let Some((_, resp)) = self.updates.remove(&token) {
      eprintln!("------ response was already received: {:?}", token);
      return Ok(resp);
    }
    eprintln!("------ get updates.");
    match self.get_updates(Some(token))? {
      Some(resp) => Ok(resp),
      None => Err(format!("Failed to get response for request: {:?}", token))?,
    }
  }

  fn get_sender(&self) -> RespSender {
    self.resp_tx.clone()
  }

  pub fn get_update(&self) -> Result<ResponseUpdate, Box<EvalAltResult>> {
    Ok(self.resp_rx.recv()
      .map_err(|_| format!("RpcConnection closed"))?)
  }

  fn get_one_update(&self, wait: bool) -> Result<Option<ResponseUpdate>, Box<EvalAltResult>> {
    if wait {
      Ok(Some(self.resp_rx.recv()
        .map_err(|_| format!("RpcConnection closed"))?))
    } else {
      use crossbeam_channel::TryRecvError::*;
      match self.resp_rx.try_recv() {
        Ok(resp) => Ok(Some(resp)),
        Err(Empty) => Ok(None),
        Err(Disconnected) => {
          Err(format!("RpcConnection closed"))?
        },
      }
    }
  }

  fn get_updates(&self, wait_for: Option<RequestToken>) -> Result<Option<ResponseUpdate>, Box<EvalAltResult>> {
    let wait = wait_for.is_some();
    loop {
      match self.get_one_update(wait)? {
        Some(resp) => {
          let token = resp.get_token();
          if wait_for == Some(token) {
            eprintln!("------ got response we wanted: {:?}", token);
            return Ok(Some(resp));
          }
          eprintln!("------ cache response: {:?}", token);
          self.updates.insert(token, resp);
        }
        None => {
          break;
        }
      }
    }
    return Ok(None);
  }
}

#[derive(Clone)]
pub struct RpcHandler(Arc<InnerRpcHandler>);

impl std::ops::Deref for RpcHandler {
  type Target = InnerRpcHandler;
  fn deref(&self) -> &InnerRpcHandler {
    &*self.0
  }
}

impl RpcHandler {
  pub fn new(conn: RpcConnection) -> Self {
    Self(InnerRpcHandler::new(conn))
  }

  pub fn call_method(&self, method: &str, params: Value) -> Result<RequestToken, Box<EvalAltResult>> {
    let req = RpcRequest::call_method(method, params, self.get_sender());
    self.0.conn.send(req)
  }

  pub fn subscribe(&self, method: &str, params: Value, unsub: &str) -> Result<RequestToken, Box<EvalAltResult>> {
    let req = RpcRequest::subscribe(method, params, self.get_sender(), unsub);
    self.0.conn.send(req)
  }
}

struct InnerRpcManager {
  next_id: AtomicU16,
  connections: DashMap<String, RpcConnection>,
}

impl InnerRpcManager {
  fn get_next_id(&self) -> ConnectionId {
    self.next_id.fetch_add(1, Ordering::Relaxed) as ConnectionId
  }
}

#[derive(Clone)]
pub struct RpcManager(Arc<InnerRpcManager>);

impl RpcManager {
  pub fn new() -> Self {
    Self(Arc::new(InnerRpcManager {
      next_id: 1.into(),
      connections: DashMap::new(),
    }))
  }

  fn get_connection(&self, url: &str) -> Result<RpcConnection, Box<EvalAltResult>> {
    if let Some(connection) = self.0.connections.get(url) {
      return Ok(connection.clone());
    }
    let id = self.0.get_next_id();
    let connection = RpcConnection::new(id, url)?;
    self.0.connections.insert(url.into(), connection.clone());
    Ok(connection)
  }

  pub fn get_client(&self, url: &str) -> Result<RpcHandler, Box<EvalAltResult>> {
    let conn = self.get_connection(url)?;
    Ok(RpcHandler::new(conn))
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
    .register_type_with_name::<RpcConnection>("RpcConnection")
    .register_type_with_name::<RequestToken>("RequestToken")
    .register_type_with_name::<ResponseUpdate>("ResponseUpdate")
    .register_fn("to_string", ResponseUpdate::to_string)
    .register_type_with_name::<RpcHandler>("RpcHandler")
    .register_result_fn("call_method", |client: &mut RpcHandler, method: &str, params: Dynamic| {
      let params: Value = from_dynamic(&params)?;
      client.call_method(method, params)
    })
    .register_result_fn("subscribe", |client: &mut RpcHandler, method: &str, params: Dynamic, unsub: &str| {
      let params: Value = from_dynamic(&params)?;
      client.subscribe(method, params, unsub)
    })
    .register_result_fn("get_response", |client: &mut RpcHandler, token: RequestToken| {
      client.get_response(token)
    })
    .register_result_fn("close_request", |client: &mut RpcHandler, token: RequestToken| {
      client.close_request(token)
    })
    .register_type_with_name::<RpcManager>("RpcManager")
    .register_result_fn("get_client", |rpc: &mut RpcManager, url: &str| rpc.get_client(url));

  let rpc = RpcManager::new();
  Ok(rpc)
}
