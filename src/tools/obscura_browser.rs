use std::collections::HashMap;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, OnceCell, oneshot};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::tungstenite::Message;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

struct CdpClient {
    write: Arc<Mutex<futures_util::stream::SplitSink<WsStream, Message>>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    event_tx: tokio::sync::broadcast::Sender<Value>,
    msg_id: AtomicU64,
}

impl CdpClient {
    async fn connect(ws_url: &str) -> Result<Self, String> {
        let (ws_stream, _) = connect_async(ws_url)
            .await
            .map_err(|e| format!("WebSocket connection failed: {}", e))?;
        let (write, mut read) = ws_stream.split();
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = pending.clone();
        let (event_tx, _) = tokio::sync::broadcast::channel(64);
        let event_tx_clone = event_tx.clone();
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(val) = serde_json::from_str::<Value>(&text) {
                            if let Some(id) = val.get("id").and_then(|v| v.as_u64()) {
                                let mut map = pending_clone.lock().await;
                                if let Some(sender) = map.remove(&id) {
                                    let _ = sender.send(val);
                                }
                            } else if val.get("method").is_some() {
                                let _ = event_tx_clone.send(val);
                            }
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
        });
        Ok(Self {
            write: Arc::new(Mutex::new(write)),
            pending,
            event_tx,
            msg_id: AtomicU64::new(1),
        })
    }

    async fn send(&self, method: &str, params: Value) -> Result<Value, String> {
        self.send_raw(method, params, None).await
    }

    async fn send_with_session(
        &self,
        method: &str,
        params: Value,
        session_id: &str,
    ) -> Result<Value, String> {
        self.send_raw(method, params, Some(session_id)).await
    }

    async fn send_raw(
        &self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
    ) -> Result<Value, String> {
        let id = self.msg_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        let mut cmd = json!({
            "id": id,
            "method": method,
            "params": params,
        });
        if let Some(sid) = session_id {
            cmd["sessionId"] = json!(sid);
        }

        let msg = Message::Text(serde_json::to_string(&cmd).unwrap().into());
        self.write
            .lock()
            .await
            .send(msg)
            .await
            .map_err(|e| format!("Send failed: {}", e))?;

        let response = tokio::time::timeout(Duration::from_secs(30), rx)
            .await
            .map_err(|_| format!("Timeout waiting for CDP response to {}", method))?
            .map_err(|_| format!("CDP channel closed for {}", method))?;

        if let Some(err) = response.get("error") {
            return Err(format!(
                "CDP error: {}",
                err.get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
            ));
        }
        Ok(response.get("result").cloned().unwrap_or(json!(null)))
    }

    fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<Value> {
        self.event_tx.subscribe()
    }
}

pub struct ObscuraBrowser {
    cdp: CdpClient,
    session_id: String,
    _process: Option<Child>,
}

impl ObscuraBrowser {
    pub async fn new() -> Result<Self, String> {
        let (child, browser_ws_url) = spawn_obscura().await?;
        let cdp = CdpClient::connect(&browser_ws_url).await?;

        let mut event_rx = cdp.subscribe_events();

        cdp.send("Target.createTarget", json!({"url": "about:blank"}))
            .await?;

        let session_id = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match event_rx.recv().await {
                    Ok(event) => {
                        if event.get("method").and_then(|v| v.as_str())
                            == Some("Target.attachedToTarget")
                        {
                            if let Some(sid) = event
                                .get("params")
                                .and_then(|p| p.get("sessionId"))
                                .and_then(|v| v.as_str())
                            {
                                return sid.to_string();
                            }
                        }
                    }
                    Err(_) => continue,
                }
            }
        })
        .await
        .map_err(|_| "Timeout waiting for Target.attachedToTarget event".to_string())?;

        cdp.send_with_session("Page.enable", json!({}), &session_id)
            .await
            .ok();
        cdp.send_with_session("Runtime.enable", json!({}), &session_id)
            .await
            .ok();
        cdp.send_with_session("DOM.enable", json!({}), &session_id)
            .await
            .ok();

        Ok(Self {
            cdp,
            session_id,
            _process: Some(child),
        })
    }

    pub async fn navigate(&self, url: &str) -> Result<String, String> {
        self.cdp
            .send_with_session("Page.navigate", json!({"url": url}), &self.session_id)
            .await?;
        tokio::time::sleep(Duration::from_secs(3)).await;
        let title = self.evaluate_js("document.title").await.unwrap_or_default();
        let text = self
            .evaluate_js("document.body.innerText")
            .await
            .unwrap_or_default();
        let preview: String = text.chars().take(5000).collect();
        let truncated = if text.len() > 5000 {
            format!("\n\n... (truncated, {} total chars)", text.len())
        } else {
            String::new()
        };
        Ok(format!("Title: {}\n\n{}{}", title, preview, truncated))
    }

    pub async fn evaluate_js(&self, script: &str) -> Result<String, String> {
        let result = self
            .cdp
            .send_with_session(
                "Runtime.evaluate",
                json!({
                    "expression": script,
                    "returnByValue": true,
                }),
                &self.session_id,
            )
            .await?;
        Ok(result
            .get("result")
            .and_then(|r| r.get("value"))
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default())
    }

    pub async fn click(&self, selector: &str) -> Result<(), String> {
        let result = self
            .cdp
            .send_with_session(
                "Runtime.evaluate",
                json!({
                    "expression": format!(
                        "(function(){{ const el = document.querySelector('{}'); if(!el) return null; const r = el.getBoundingClientRect(); return {{x: r.left + r.width/2, y: r.top + r.height/2}}; }})()",
                        selector.replace('\\', "\\\\").replace('\'', "\\'")
                    ),
                    "returnByValue": true,
                }),
                &self.session_id,
            )
            .await?;

        let x = result
            .get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.get("x"))
            .and_then(|v| v.as_f64())
            .ok_or_else(|| format!("Element not found: {}", selector))?;
        let y = result
            .get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.get("y"))
            .and_then(|v| v.as_f64())
            .ok_or_else(|| format!("Element not found: {}", selector))?;

        self.cdp
            .send_with_session(
                "Input.dispatchMouseEvent",
                json!({
                    "type": "mousePressed",
                    "x": x,
                    "y": y,
                    "button": "left",
                    "clickCount": 1,
                }),
                &self.session_id,
            )
            .await?;
        self.cdp
            .send_with_session(
                "Input.dispatchMouseEvent",
                json!({
                    "type": "mouseReleased",
                    "x": x,
                    "y": y,
                    "button": "left",
                    "clickCount": 1,
                }),
                &self.session_id,
            )
            .await?;
        Ok(())
    }

    pub async fn fill(&self, selector: &str, value: &str) -> Result<(), String> {
        self.cdp
            .send_with_session(
                "Runtime.evaluate",
                json!({
                    "expression": format!(
                        r#"(function(){{ const el = document.querySelector('{}'); if(!el) return; el.focus(); el.value = {}; el.dispatchEvent(new Event('input', {{ bubbles: true }})); el.dispatchEvent(new Event('change', {{ bubbles: true }})); }})()"#,
                        selector.replace('\\', "\\\\").replace('\'', "\\'"),
                        serde_json::to_string(value).map_err(|e| format!("JSON error: {}", e))?
                    ),
                }),
                &self.session_id,
            )
            .await?;
        Ok(())
    }

    pub async fn screenshot(&self, full_page: bool) -> Result<Vec<u8>, String> {
        let mut params = json!({"format": "png"});
        if full_page {
            params["fullPage"] = json!(true);
        }
        let result = self.cdp
            .send_with_session("Page.captureScreenshot", params, &self.session_id)
            .await?;
        let base64_str = result.get("data")
            .and_then(|v| v.as_str())
            .ok_or("No screenshot data in CDP response")?;
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(base64_str)
            .map_err(|e| format!("Base64 decode failed: {}", e))?;
        Ok(bytes)
    }
}

async fn spawn_obscura() -> Result<(Child, String), String> {
    let preferred_port = std::env::var("OBSCURA_CDP_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(9222);

    let actual_port = if port_available(preferred_port) {
        preferred_port
    } else {
        (preferred_port + 1..preferred_port + 100)
            .find(|&p| port_available(p))
            .ok_or_else(|| {
                format!(
                    "Port {} and next 99 ports are all in use",
                    preferred_port
                )
            })?
    };

    let child = Command::new("obscura")
        .args(["serve", "--port", &actual_port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            format!(
                "Cannot start Obscura: {}\nInstall with: cargo install obscura",
                e
            )
        })?;

    let browser_ws_url = get_browser_ws_url(actual_port).await?;
    Ok((child, browser_ws_url))
}

fn port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

async fn get_browser_ws_url(port: u16) -> Result<String, String> {
    let url = format!("http://127.0.0.1:{}/json/version", port);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        match client.get(&url).send().await {
            Ok(r) if r.status().is_success() => {
                let body = r.text().await.unwrap_or_default();
                if let Ok(info) = serde_json::from_str::<Value>(&body) {
                    if let Some(ws_url) = info.get("webSocketDebuggerUrl").and_then(|v| v.as_str()) {
                        return Ok(ws_url.to_string());
                    }
                }
            }
            _ => {}
        }
        if std::time::Instant::now() > deadline {
            return Err("Timed out waiting for Obscura WebSocket URL".to_string());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

static BROWSER: OnceCell<Arc<ObscuraBrowser>> = OnceCell::const_new();

pub async fn get_browser() -> Result<Arc<ObscuraBrowser>, String> {
    BROWSER
        .get_or_try_init(|| async { ObscuraBrowser::new().await.map(Arc::new) })
        .await
        .map(|b| b.clone())
}
