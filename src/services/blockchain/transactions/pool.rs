use solana_client::rpc_client::RpcClient;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

pub struct ConnectionPool {
    base_client: Arc<RpcClient>,
    pool: Arc<RwLock<Vec<Arc<RpcClient>>>>,
}

impl ConnectionPool {
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            base_client: rpc_client,
            pool: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn get_connection(&self) -> Arc<RpcClient> {
        let mut pool = self.pool.write().await;

        if let Some(conn) = pool.pop() {
            debug!("Reusing existing connection from pool");
            return conn;
        }

        let new_conn = Arc::new(RpcClient::new(self.base_client.url()));
        debug!("Created new RPC connection for pool");
        new_conn
    }

    pub async fn return_connection(&self, conn: Arc<RpcClient>) {
        let mut pool = self.pool.write().await;
        pool.push(conn);
        debug!("Returned connection to pool (size: {})", pool.len());
    }

    pub fn client(&self) -> &RpcClient {
        &self.base_client
    }

    pub fn arc_client(&self) -> Arc<RpcClient> {
        self.base_client.clone()
    }
}
