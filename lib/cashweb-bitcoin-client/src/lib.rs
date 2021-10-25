#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]

//! `cashweb-bitcoin-client` is a library providing a [`BitcoinClient`] with
//! basic asynchronous methods for interacting with bitcoind.
use async_trait::async_trait;
use hex::FromHexError;
use hyper::{
    client::{connect::Connect, HttpConnector},
     Client as HyperClient,
};
use hyper_tls::HttpsConnector;
use json_rpc::{
    clients::{
        http::Client as JsonClient,
    },
    prelude::{JsonError, RequestFactory, RpcError},
};
use serde_json::Value;
use thiserror::Error;

/// Standard HTTP client.
pub type HttpClient = HyperClient<HttpConnector>;

/// Standard HTTPs client.
pub type HttpsClient = HyperClient<HttpsConnector<HttpConnector>>;

/// Error associated with the Bitcoin RPC.
#[derive(Debug, Error)]
pub enum NodeError {
    /// Error connecting to bitcoind.
    #[error("Connection error: {0}")]
    RpcConnectError(String),
    /// bitcoind responded with an JSON-RPC error.
    #[error("{0:?}")]
    Rpc(RpcError),
    /// Failed to deserialize response JSON.
    #[error(transparent)]
    Json(JsonError),
    /// The response JSON was empty.
    #[error("empty response")]
    EmptyResponse,
    /// Failed to decode hexidecimal response.
    #[error(transparent)]
    HexDecode(#[from] FromHexError),
}


/// Bitcoin Client function traits
#[async_trait]
pub trait BitcoinClient {
    /// Send a raw transaction to bitcoind
    async fn send_tx(&self, raw_tx: &[u8]) -> Result<String, NodeError>;
    /// Get a new receiving address from the bitcoin daemon
    async fn get_new_addr(&self) -> Result<String, NodeError>;
    /// Get a raw bitcoin transaction by txid
    async fn get_raw_transaction(&self, tx_id: &[u8]) -> Result<Vec<u8>, NodeError>;
}

/// Basic Bitcoin JSON-RPC client.
#[derive(Clone, Debug)]
pub struct BitcoinClientHTTP(JsonClient<HyperClient<HttpConnector>>);

impl BitcoinClientHTTP {
    /// Create a new HTTP [`BitcoinClient`].
    pub fn new(endpoint: String, username: String, password: String) -> Self {
        BitcoinClientHTTP(JsonClient::new(endpoint, Some(username), Some(password)))
    }
}

/// Basic HTTPS Bitcoin JSON-RPC client.
#[derive(Clone, Debug)]
pub struct BitcoinClientTLS(JsonClient<HyperClient<HttpsConnector<HttpConnector>>>);

impl BitcoinClientTLS {
    /// Create a new HTTPS [`BitcoinClient`].
    pub fn new(endpoint: String, username: String, password: String) -> Self {
        BitcoinClientTLS(JsonClient::new_tls(
            endpoint,
            Some(username),
            Some(password),
        ))
    }
}

type BitcoinJsonClient<C> = JsonClient<HyperClient<C>>;
trait Connectable: Connect + Clone + Send + Sync + 'static {}
impl<T: Connect + Clone + Send + Sync + 'static> Connectable for T {}

async fn get_new_addr<C: Connectable>(client: &BitcoinJsonClient<C>) -> Result<String, NodeError> {
    let request = client
        .build_request()
        .method("getnewaddress")
        .finish()
        .unwrap();
    let response = client
        .send(request)
        .await
        .map_err(|err| NodeError::RpcConnectError(err.to_string()))?;
    if response.is_error() {
        return Err(NodeError::Rpc(response.error().unwrap()));
    }
    response
        .into_result()
        .ok_or(NodeError::EmptyResponse)?
        .map_err(NodeError::Json)
}

async fn send_tx<C: Connectable>(
    client: &BitcoinJsonClient<C>,
    raw_tx: &[u8],
) -> Result<String, NodeError> {
    let request = client
        .build_request()
        .method("sendrawtransaction")
        .params(vec![Value::String(hex::encode(raw_tx))])
        .finish()
        .unwrap();
    let response = client
        .send(request)
        .await
        .map_err(|err| NodeError::RpcConnectError(err.to_string()))?;
    if response.is_error() {
        let err = response.error().unwrap();
        return Err(NodeError::Rpc(err));
    }
    response
        .into_result()
        .ok_or(NodeError::EmptyResponse)?
        .map_err(NodeError::Json)
}

/// Calls the `getrawtransaction` method.
async fn get_raw_transaction<C: Connectable>(
    client: &BitcoinJsonClient<C>,
    tx_id: &[u8],
) -> Result<Vec<u8>, NodeError> {
    let request = client
        .build_request()
        .method("getrawtransaction")
        .params(vec![Value::String(hex::encode(tx_id))])
        .finish()
        .unwrap();
    let response = client
        .send(request)
        .await
        .map_err(|err| NodeError::RpcConnectError(err.to_string()))?;
    if response.is_error() {
        return Err(NodeError::Rpc(response.error().unwrap()));
    }
    let tx_hex: String = response
        .into_result()
        .ok_or(NodeError::EmptyResponse)?
        .map_err(NodeError::Json)?;
    hex::decode(tx_hex).map_err(Into::into)
}

#[async_trait]
impl BitcoinClient for BitcoinClientTLS {
    /// Calls the `getnewaddress` method.
    async fn get_new_addr(&self) -> Result<String, NodeError> {
        get_new_addr(&self.0).await
    }

    /// Calls the `sendrawtransaction` method.
    async fn send_tx(&self, raw_tx: &[u8]) -> Result<String, NodeError> {
        send_tx(&self.0, raw_tx).await
    }

    /// Calls the `getrawtransaction` method.
    async fn get_raw_transaction(&self, tx_id: &[u8]) -> Result<Vec<u8>, NodeError> {
        get_raw_transaction(&self.0, tx_id).await
    }
}

#[async_trait]
impl BitcoinClient for BitcoinClientHTTP {
    /// Calls the `getnewaddress` method.
    async fn get_new_addr(&self) -> Result<String, NodeError> {
        get_new_addr(&self.0).await
    }

    /// Calls the `sendrawtransaction` method.
    async fn send_tx(&self, raw_tx: &[u8]) -> Result<String, NodeError> {
        send_tx(&self.0, raw_tx).await
    }

    /// Calls the `getrawtransaction` method.
    async fn get_raw_transaction(&self, tx_id: &[u8]) -> Result<Vec<u8>, NodeError> {
        get_raw_transaction(&self.0, tx_id).await
    }
}
