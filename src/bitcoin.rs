use json_rpc::{clients::http::HttpConnector, prelude::*};

use serde_json::Value;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Mainnet = 0,
    Testnet = 1,
    Regnet = 2,
}

impl From<bitcoincash_addr::Network> for Network {
    fn from(network: bitcoincash_addr::Network) -> Network {
        match network {
            bitcoincash_addr::Network::Main => Network::Mainnet,
            bitcoincash_addr::Network::Test => Network::Testnet,
            bitcoincash_addr::Network::Regtest => Network::Regnet,
        }
    }
}

impl Into<bitcoincash_addr::Network> for Network {
    fn into(self) -> bitcoincash_addr::Network {
        match self {
            Network::Mainnet => bitcoincash_addr::Network::Main,
            Network::Testnet => bitcoincash_addr::Network::Test,
            Network::Regnet => bitcoincash_addr::Network::Regtest,
        }
    }
}

impl ToString for Network {
    fn to_string(&self) -> String {
        match self {
            Network::Mainnet => "mainnet".to_string(),
            Network::Testnet => "testnet".to_string(),
            Network::Regnet => "regnet".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BitcoinClient<C>(HttpClient<C>);

impl BitcoinClient<HttpConnector> {
    pub fn new(endpoint: String, username: String, password: String) -> Self {
        BitcoinClient(HttpClient::new(endpoint, Some(username), Some(password)))
    }
}

impl<C> std::ops::Deref for BitcoinClient<C> {
    type Target = HttpClient<C>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
pub enum BitcoinError {
    Http(HttpError),
    Rpc(RpcError),
    Json(JsonError),
    EmptyResponse,
}

impl<C> BitcoinClient<C>
where
    C: Connect + Clone + Send + Sync + 'static,
{
    pub async fn get_new_addr(&self) -> Result<String, BitcoinError> {
        let request = self
            .build_request()
            .method("getnewaddress")
            .finish()
            .unwrap();
        let response = self.send(request).await.map_err(BitcoinError::Http)?;
        if response.is_error() {
            return Err(BitcoinError::Rpc(response.error().unwrap()));
        }
        response
            .into_result()
            .ok_or(BitcoinError::EmptyResponse)?
            .map_err(BitcoinError::Json)
    }

    pub async fn send_tx(&self, raw_tx: &[u8]) -> Result<String, BitcoinError> {
        let request = self
            .build_request()
            .method("sendrawtransaction")
            .params(vec![Value::String(hex::encode(raw_tx))])
            .finish()
            .unwrap();
        let response = self.send(request).await.map_err(BitcoinError::Http)?;
        if response.is_error() {
            let err = response.error().unwrap();
            return Err(BitcoinError::Rpc(err));
        }
        response
            .into_result()
            .ok_or(BitcoinError::EmptyResponse)?
            .map_err(BitcoinError::Json)
    }
}
