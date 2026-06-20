pub mod server;

pub use server::DashboardServer;

use tokio::sync::broadcast;
use std::sync::Arc;

pub struct DashboardState {
    pub tx: broadcast::Sender<String>,
}

impl DashboardState {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self { tx }
    }

    pub fn broadcast(&self, msg: &str) {
        let _ = self.tx.send(msg.to_string());
    }
}

pub type SharedState = Arc<DashboardState>;
