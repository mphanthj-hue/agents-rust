use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use futures_util::{StreamExt, SinkExt};
use std::sync::Arc;
use super::DashboardState;

pub struct DashboardServer {
    state: Arc<DashboardState>,
    port: u16,
}

impl DashboardServer {
    pub fn new(port: u16) -> Self {
        Self {
            state: Arc::new(DashboardState::new()),
            port,
        }
    }

    #[allow(dead_code)]
    pub fn state(&self) -> Arc<DashboardState> {
        self.state.clone()
    }

    pub async fn start(&self) -> Result<(), String> {
        let addr: SocketAddr = format!("127.0.0.1:{}", self.port).parse()
            .map_err(|e| format!("Invalid addr: {}", e))?;
        let listener = TcpListener::bind(addr).await
            .map_err(|e| format!("Bind lỗi: {}", e))?;

        eprintln!("[dashboard] WebSocket server on ws://{}", addr);

        let state = self.state.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, peer)) => {
                        let state = state.clone();
                        tokio::spawn(handle_connection(stream, peer, state));
                    }
                    Err(e) => {
                        eprintln!("[dashboard] Accept lỗi: {}", e);
                    }
                }
            }
        });

        // Also serve HTTP trang dashboard
        let http_port = self.port + 1;
        let http_addr: SocketAddr = format!("127.0.0.1:{}", http_port).parse().unwrap();
        let http_listener = TcpListener::bind(http_addr).await
            .map_err(|e| format!("HTTP bind lỗi: {}", e))?;

        eprintln!("[dashboard] HTTP server on http://{}", http_addr);

        tokio::spawn(async move {
            loop {
                match http_listener.accept().await {
                    Ok((stream, _peer)) => {
                        tokio::spawn(serve_http(stream));
                    }
                    Err(e) => {
                        eprintln!("[dashboard] HTTP accept lỗi: {}", e);
                    }
                }
            }
        });

        Ok(())
    }
}

async fn handle_connection(stream: TcpStream, peer: SocketAddr, state: Arc<DashboardState>) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("[dashboard] WS handshake lỗi từ {}: {}", peer, e);
            return;
        }
    };

    let mut rx = state.tx.subscribe();
    let (mut write, _read) = ws_stream.split();

    while let Ok(msg) = rx.recv().await {
        if write.send(Message::Text(msg)).await.is_err() {
            break;
        }
    }
}

async fn serve_http(stream: TcpStream) {
    use tokio::io::{AsyncWriteExt, AsyncReadExt};

    let html = DASHBOARD_HTML;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(), html
    );

    let mut stream = stream;
    let mut buf = [0u8; 1024];
    let _ = stream.read(&mut buf).await;
    let _ = stream.write_all(response.as_bytes()).await;
}

const DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="vi">
<head>
<meta charset="UTF-8">
<title>Agents-Rust Dashboard</title>
<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: system-ui, -apple-system, sans-serif; background: #0d1117; color: #c9d1d9; padding: 20px; }
h1 { color: #58a6ff; margin-bottom: 20px; font-size: 1.5em; }
#log { background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 16px; height: 80vh; overflow-y: auto; font-family: 'JetBrains Mono', 'Fira Code', monospace; font-size: 13px; line-height: 1.6; }
.entry { margin: 4px 0; padding: 4px 8px; border-radius: 4px; }
.entry.info { color: #58a6ff; }
.entry.ok { color: #3fb950; }
.entry.err { color: #f85149; background: #2d1b1b; }
.entry.warn { color: #d29922; }
.entry.system { color: #8b949e; font-style: italic; }
.timestamp { color: #484f58; margin-right: 8px; }
.status { display: flex; gap: 16px; margin-bottom: 16px; padding: 12px 16px; background: #161b22; border: 1px solid #30363d; border-radius: 8px; }
.status-item { text-align: center; }
.status-item .num { font-size: 1.8em; font-weight: bold; display: block; }
.status-item .label { font-size: 0.8em; color: #8b949e; }
</style>
</head>
<body>
<h1>🔍 Agents-Rust Dashboard</h1>
<div class="status">
<div class="status-item"><span class="num" id="total">0</span><span class="label">Total Tasks</span></div>
<div class="status-item"><span class="num" id="success" style="color:#3fb950">0</span><span class="label">Success</span></div>
<div class="status-item"><span class="num" id="fail" style="color:#f85149">0</span><span class="label">Failed</span></div>
</div>
<div id="log">Đang chờ kết nối WebSocket...</div>
<script>
let total=0, succ=0, fail=0;
const log = document.getElementById('log');
const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
const ws = new WebSocket(protocol + '//' + location.host.replace(/:(\d+)/, (_, p) => ':' + (parseInt(p)-1)));
ws.onmessage = (e) => {
const entry = document.createElement('div');
entry.className = 'entry';
const ts = new Date().toLocaleTimeString();
const span = document.createElement('span');
span.className = 'timestamp';
span.textContent = '['+ts+'] ';
entry.appendChild(span);
try {
const data = JSON.parse(e.data);
entry.innerHTML += (data.msg || e.data);
if (data.type === 'ok' || data.type === 'success') { entry.classList.add('ok'); succ++; total++; }
else if (data.type === 'err' || data.type === 'error') { entry.classList.add('err'); fail++; total++; }
else if (data.type === 'system') { entry.classList.add('system'); }
} catch {
entry.innerHTML += e.data;
}
log.appendChild(entry);
log.scrollTop = log.scrollHeight;
document.getElementById('total').textContent = total;
document.getElementById('success').textContent = succ;
document.getElementById('fail').textContent = fail;
};
ws.onopen = () => log.innerHTML = '';
</script>
</body>
</html>"#;
