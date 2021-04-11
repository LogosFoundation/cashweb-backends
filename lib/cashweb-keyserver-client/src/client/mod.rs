//!

pub mod services;

use std::{error, fmt};

use bytes::Bytes;
use hyper::{client::Client as HyperClient, client::HttpConnector, http::uri::InvalidUri};
use hyper_tls::HttpsConnector;
use secp256k1::key::PublicKey;
use thiserror::Error;
use tower_service::Service;
use tower_util::ServiceExt;

use crate::models::*;
use services::*;

/// Error associated with sending a request to a keyserver.
#[derive(Debug, Error)]
pub enum KeyserverError<E: fmt::Display + error::Error + 'static> {
    /// Invalid URI.
    #[error(transparent)]
    Uri(InvalidUri),
    /// Error executing the service method.
    #[error("failed to execute service method: {0}")]
    Error(#[from] E),
}

/// The [`AddressMetadata`] paired with its [`PublicKey`], the raw [`AuthWrapper`] and a [`POP token`].
///
/// [`POP token`]: https://github.com/cashweb/specifications/blob/master/proof-of-payment-token/specification.mediawiki
#[derive(Clone, Debug)]
pub struct MetadataPackage {
    /// [`POP token`] attached to the response.
    ///
    /// [`POP token`]: https://github.com/cashweb/specifications/blob/master/proof-of-payment-token/specification.mediawiki
    pub token: String,
    /// Public key of the metadata.
    pub public_key: PublicKey,
    /// The address metadata.
    pub metadata: AddressMetadata,
    /// The raw [`AuthWrapper`]
    pub raw_auth_wrapper: Bytes,
}

/// The raw [`AuthWrapper`] paired with a [`POP token`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawAuthWrapperPackage {
    /// [`POP token`] attached to the response.
    ///
    /// [`POP token`]: https://github.com/cashweb/specifications/blob/master/proof-of-payment-token/specification.mediawiki
    pub token: String,
    /// The raw [`AuthWrapper`].
    pub raw_auth_wrapper: Bytes,
}

/// `KeyserverClient` allows queries to specific keyservers.
#[derive(Clone, Debug)]
pub struct KeyserverClient<S> {
    inner_client: S,
}

impl<S> KeyserverClient<S> {
    /// Create a new client from a [`Service`].
    ///
    /// [`Service`]: tower_service::Service
    pub fn from_service(service: S) -> Self {
        Self {
            inner_client: service,
        }
    }
}

impl Default for KeyserverClient<HyperClient<HttpConnector>> {
    fn default() -> Self {
        Self {
            inner_client: HyperClient::new(),
        }
    }
}

impl KeyserverClient<HyperClient<HttpConnector>> {
    /// Create a new HTTP client.
    pub fn new() -> Self {
        Default::default()
    }
}

impl KeyserverClient<HyperClient<HttpsConnector<HttpConnector>>> {
    /// Create new HTTPS client.
    pub fn new_tls() -> Self {
        let https = HttpsConnector::new();
        Self {
            inner_client: HyperClient::builder().build(https),
        }
    }
}

impl<S> KeyserverClient<S>
where
    Self: Service<(Uri, GetPeers), Response = Peers>,
    Self: Sync + Clone + Send + 'static,
    <Self as Service<(Uri, GetPeers)>>::Error: fmt::Display + std::error::Error,
    <Self as Service<(Uri, GetPeers)>>::Future: Send + Sync + 'static,
{
    /// Get [`Peers`] from a keyserver.
    pub async fn get_peers(
        &self,
        keyserver_url: &str,
    ) -> Result<Peers, KeyserverError<<Self as Service<(Uri, GetPeers)>>::Error>> {
        // Construct URI
        let full_path = format!("{}/peers", keyserver_url);
        let uri: Uri = full_path.parse().map_err(KeyserverError::Uri)?;

        // Construct request
        let request = (uri, GetPeers);

        self.clone()
            .oneshot(request)
            .await
            .map_err(KeyserverError::Error)
    }
}

impl<S> KeyserverClient<S>
where
    Self: Service<(Uri, GetMetadata), Response = MetadataPackage>,
    Self: Sync + Clone + Send + 'static,
    <Self as Service<(Uri, GetMetadata)>>::Error: fmt::Display + std::error::Error,
    <Self as Service<(Uri, GetMetadata)>>::Future: Send + Sync + 'static,
{
    /// Get [`AddressMetadata`] from a server. The result is wrapped in [`MetadataPackage`].
    pub async fn get_metadata(
        &self,
        keyserver_url: &str,
        address: &str,
    ) -> Result<MetadataPackage, KeyserverError<<Self as Service<(Uri, GetMetadata)>>::Error>> {
        // Construct URI
        let full_path = format!("{}/keys/{}", keyserver_url, address);
        let uri: Uri = full_path.parse().map_err(KeyserverError::Uri)?;

        // Construct request
        let request = (uri, GetMetadata);

        self.clone()
            .oneshot(request)
            .await
            .map_err(KeyserverError::Error)
    }
}

impl<S> KeyserverClient<S>
where
    Self: Service<(Uri, PutMetadata), Response = ()>,
    Self: Sync + Clone + Send + 'static,
    <Self as Service<(Uri, PutMetadata)>>::Error: fmt::Display + std::error::Error,
    <Self as Service<(Uri, PutMetadata)>>::Future: Send + Sync + 'static,
{
    /// Put [`AuthWrapper`] to a keyserver.
    pub async fn put_metadata(
        &self,
        keyserver_url: &str,
        address: &str,
        auth_wrapper: AuthWrapper,
        token: String,
    ) -> Result<(), KeyserverError<<Self as Service<(Uri, PutMetadata)>>::Error>> {
        // Construct URI
        let full_path = format!("{}/keys/{}", keyserver_url, address);
        let uri: Uri = full_path.parse().map_err(KeyserverError::Uri)?;

        // Construct request
        let request = (
            uri,
            PutMetadata {
                token,
                auth_wrapper,
            },
        );

        // Get response
        self.clone()
            .oneshot(request)
            .await
            .map_err(KeyserverError::Error)
    }
}

impl<S> KeyserverClient<S>
where
    Self: Service<(Uri, PutRawAuthWrapper), Response = ()>,
    Self: Sync + Clone + Send + 'static,
    <Self as Service<(Uri, PutRawAuthWrapper)>>::Error: std::error::Error,
    <Self as Service<(Uri, PutRawAuthWrapper)>>::Future: Send + Sync + 'static,
{
    /// Put raw [`AuthWrapper`] to a keyserver.
    pub async fn put_raw_metadata(
        &self,
        keyserver_url: &str,
        address: &str,
        raw_auth_wrapper: Vec<u8>,
        token: String,
    ) -> Result<(), KeyserverError<<Self as Service<(Uri, PutRawAuthWrapper)>>::Error>> {
        // Construct URI
        let full_path = format!("{}/keys/{}", keyserver_url, address);
        let uri: Uri = full_path.parse().map_err(KeyserverError::Uri)?;

        // Construct request
        let request = (
            uri,
            PutRawAuthWrapper {
                token,
                raw_auth_wrapper,
            },
        );

        // Get response
        self.clone()
            .oneshot(request)
            .await
            .map_err(KeyserverError::Error)
    }
}
