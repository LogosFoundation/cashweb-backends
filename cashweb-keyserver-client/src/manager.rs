use std::{collections::HashSet, fmt, str::FromStr, sync::Arc};

use hyper::{
    client::Client as HyperClient,
    client::HttpConnector,
    http::uri::{InvalidUri, PathAndQuery},
    Body, Request, Response, Uri,
};
use prost::Message as _;
use rand::seq::SliceRandom;
use tokio::sync::RwLock;
use tower_service::Service;
use tower_util::ServiceExt;

use crate::{
    client::{services::*, KeyserverClient, MetadataPackage},
    models::{AuthWrapper, Peer, Peers},
};

/// KeyserverManager wraps a client and allows sampling and selecting of queries across a set of keyservers.
#[derive(Clone, Debug)]
pub struct KeyserverManager<S> {
    inner_client: KeyserverClient<S>,
    uris: Arc<RwLock<Vec<Uri>>>,
}

impl<S> KeyserverManager<S> {
    /// Creates a new manager from URIs and a client.
    pub fn from_service(service: S, uris: Vec<Uri>) -> Self {
        Self {
            inner_client: KeyserverClient::from_service(service),
            uris: Arc::new(RwLock::new(uris)),
        }
    }

    /// Get shared reference the [`Uri`]s.
    pub fn get_uris(&self) -> Arc<RwLock<Vec<Uri>>> {
        self.uris.clone()
    }

    /// Converts the manager into the underlying client.
    pub fn into_client(self) -> KeyserverClient<S> {
        self.inner_client
    }
}

impl KeyserverManager<HyperClient<HttpConnector>> {
    /// Create a HTTP manager.
    pub fn new(uris: Vec<String>) -> Result<Self, InvalidUri> {
        let uris: Result<Vec<Uri>, _> = uris.into_iter().map(|uri| uri.parse()).collect();
        let uris = uris?;
        Ok(Self {
            inner_client: KeyserverClient::new(),
            uris: Arc::new(RwLock::new(uris)),
        })
    }
}

/// Takes a URI and appends a path to it.
///
/// This panics if `new_path` is invalid.
fn append_path(uri: Uri, new_path: &str) -> Uri {
    let mut parts = uri.into_parts();
    let path_and_query_opt = &mut parts.path_and_query;
    let new_path_query_str = if let Some(path_and_query) = path_and_query_opt {
        let path = path_and_query.path();
        if path.ends_with('/') {
            let mut trimmed = path.to_string();
            trimmed.pop();
            format!(
                "{}{}{}",
                trimmed,
                new_path,
                path_and_query.query().unwrap_or_default()
            )
        } else {
            format!(
                "{}{}{}",
                path,
                new_path,
                path_and_query.query().unwrap_or_default()
            )
        }
    } else {
        new_path.to_string()
    };
    *path_and_query_opt = Some(PathAndQuery::from_str(&new_path_query_str).unwrap()); // TODO: Double check this is safe

    Uri::from_parts(parts).unwrap()
}

/// Choose from a random subset of URIs.
pub fn uniform_random_sampler(uris: &[Uri], size: usize) -> Vec<Uri> {
    let mut rng = &mut rand::thread_rng();
    uris.choose_multiple(&mut rng, size).cloned().collect()
}

/// Select best [`AuthWrapper`] from a list.
///
/// [`AuthWrapper`]: auth_wrapper::AuthWrapper
pub fn select_auth_wrapper(
    metadatas: Vec<(Uri, MetadataPackage)>,
) -> Option<(Uri, MetadataPackage)> {
    metadatas
        .into_iter()
        .max_by_key(move |(_, package)| package.metadata.timestamp)
}

/// Aggregate a collection of [`Peers`] into a single structure.
pub fn aggregate_peers(peers: Vec<(Uri, Peers)>) -> Peers {
    let peers = peers
        .into_iter()
        .map(move |(_, peer)| peer.peers)
        .flatten()
        .collect();
    Peers { peers }
}

/// Response to a sample query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SampleResponse<R, E> {
    /// Paired [`Uri`] and response.
    pub response: Option<(Uri, R)>,
    /// The errors paired with the [`Uri`] of the keyserver they originated at.
    pub errors: Vec<(Uri, E)>,
}

impl<R, E> SampleResponse<R, E>
where
    R: fmt::Debug,
    E: fmt::Debug,
{
    /// Create a sample response from a list of results.
    pub fn select<F: FnOnce(Vec<(Uri, R)>) -> Option<(Uri, R)>>(
        responses: Vec<(Uri, Result<R, E>)>,
        selector: F,
    ) -> Self {
        let (oks, errors): (Vec<_>, Vec<_>) =
            responses.into_iter().partition(|(_, res)| res.is_ok());
        let oks = oks
            .into_iter()
            .map(|(uri, res)| (uri, res.unwrap()))
            .collect();
        let errors = errors
            .into_iter()
            .map(|(uri, res)| (uri, res.unwrap_err()))
            .collect();

        let response = selector(oks);

        SampleResponse { response, errors }
    }
}

/// Response to an aggregation query.
#[derive(Debug)]
pub struct AggregateResponse<R, E> {
    /// The aggregated response of the sample.
    pub response: R,
    /// The errors paired with the [`Uri`] of the keyserver they originated at.
    pub errors: Vec<(Uri, E)>,
}

impl<R, E> AggregateResponse<R, E>
where
    R: fmt::Debug,
    E: fmt::Debug,
{
    /// Create a sample response from a list of results.
    pub fn aggregate<F: FnOnce(Vec<(Uri, R)>) -> R>(
        responses: Vec<(Uri, Result<R, E>)>,
        aggregator: F,
    ) -> Self {
        let (oks, errors): (Vec<_>, Vec<_>) =
            responses.into_iter().partition(|(_, res)| res.is_ok());
        let oks = oks
            .into_iter()
            .map(|(uri, res)| (uri, res.unwrap()))
            .collect();
        let errors = errors
            .into_iter()
            .map(|(uri, res)| (uri, res.unwrap_err()))
            .collect();

        let response = aggregator(oks);

        AggregateResponse { response, errors }
    }
}

impl<S> KeyserverManager<S>
where
    S: Service<Request<Body>, Response = Response<Body>>,
    S: Send + Clone + 'static,
    S::Future: Send,
    S::Error: fmt::Debug + fmt::Display + Send,
{
    /// Perform a uniform sample of metadata over keyservers and select the latest.
    pub async fn uniform_sample_metadata(
        &self,
        address: &str,
        sample_size: usize,
    ) -> Result<
        SampleResponse<MetadataPackage, <KeyserverClient<S> as Service<(Uri, GetMetadata)>>::Error>,
        SampleError<<KeyserverClient<S> as Service<(Uri, GetMetadata)>>::Error>,
    > {
        let uris = self.uris.read().await.clone();
        let uris = uris
            .into_iter()
            .map(|uri| append_path(uri, &format!("/keys/{}", address)))
            .collect::<Vec<Uri>>();
        let uris = uniform_random_sampler(&uris, sample_size);
        let sample_request = SampleRequest {
            request: GetMetadata,
            uris,
        };

        let responses = self.inner_client.clone().oneshot(sample_request).await?;
        let sample_response = SampleResponse::select(responses, select_auth_wrapper);

        Ok(sample_response)
    }

    /// Collect all peers from keyservers.
    pub async fn collect_peers(
        &self,
    ) -> Result<
        AggregateResponse<Peers, <KeyserverClient<S> as Service<(Uri, GetPeers)>>::Error>,
        SampleError<<KeyserverClient<S> as Service<(Uri, GetPeers)>>::Error>,
    > {
        let uris = self.uris.read().await.clone();
        let uris = uris
            .into_iter()
            .map(|uri| append_path(uri, "/peers"))
            .collect::<Vec<Uri>>();
        let sample_request = SampleRequest {
            uris,
            request: GetPeers,
        };
        let responses = self.inner_client.clone().oneshot(sample_request).await?;

        let aggregate_response = AggregateResponse::aggregate(responses, aggregate_peers);

        Ok(aggregate_response)
    }

    /// Crawl peers.
    #[allow(clippy::mutable_key_type)]
    pub async fn crawl_peers(
        &self,
    ) -> Result<
        AggregateResponse<Peers, <KeyserverClient<S> as Service<(Uri, GetPeers)>>::Error>,
        SampleError<<KeyserverClient<S> as Service<(Uri, GetPeers)>>::Error>,
    > {
        let read_uris = self.uris.read().await;
        let mut found_uris: HashSet<_> = read_uris.iter().cloned().collect();

        let mut total: HashSet<_> = read_uris.iter().cloned().collect();

        let mut total_errors = Vec::new();
        while !found_uris.is_empty() {
            // Get sample
            let uris = found_uris
                .drain()
                .map(|uri| append_path(uri, "/peers"))
                .collect();
            let sample_request = SampleRequest {
                uris,
                request: GetPeers,
            };
            let responses: Vec<_> = self.inner_client.clone().oneshot(sample_request).await?;

            let AggregateResponse { response, errors } =
                AggregateResponse::aggregate(responses, aggregate_peers);

            // Aggregate errors
            total_errors.extend(errors);

            // Aggregate URIs
            let mut found_uris: HashSet<_> = response
                .peers
                .iter()
                .filter_map(|peer| peer.url.parse::<Uri>().ok())
                .collect();

            // Only keep new URIs
            found_uris = found_uris.difference(&total).cloned().collect();
            total = total.union(&found_uris).cloned().collect();
        }

        let response = Peers {
            peers: total
                .into_iter()
                .map(|uri| Peer {
                    url: uri.to_string(),
                })
                .collect(),
        };
        Ok(AggregateResponse {
            response,
            errors: total_errors,
        })
    }

    /// Perform a uniform broadcast of metadata over keyservers and select the latest.
    pub async fn uniform_broadcast_metadata(
        &self,
        address: &str,
        auth_wrapper: AuthWrapper,
        token: String,
        sample_size: usize,
    ) -> Result<
        AggregateResponse<(), <KeyserverClient<S> as Service<(Uri, PutMetadata)>>::Error>,
        SampleError<<KeyserverClient<S> as Service<(Uri, PutMetadata)>>::Error>,
    > {
        let read_uris = self.uris.read().await;
        let uris = uniform_random_sampler(&read_uris, sample_size)
            .into_iter()
            .map(|uri| append_path(uri, &format!("/keys/{}", address)))
            .collect::<Vec<Uri>>();

        // Construct body
        let mut raw_auth_wrapper = Vec::with_capacity(auth_wrapper.encoded_len());
        auth_wrapper.encode(&mut raw_auth_wrapper).unwrap();

        let request = PutRawAuthWrapper {
            token,
            raw_auth_wrapper,
        };
        let sample_request = SampleRequest { uris, request };
        let responses = self.inner_client.clone().call(sample_request).await?;

        Ok(AggregateResponse::aggregate(responses, |_| ()))
    }

    /// Perform a uniform broadcast of raw metadata over keyservers and select the latest.
    pub async fn uniform_broadcast_raw_metadata(
        &self,
        address: &str,
        raw_auth_wrapper: Vec<u8>,
        token: String,
        sample_size: usize,
    ) -> Result<
        AggregateResponse<(), <KeyserverClient<S> as Service<(Uri, PutMetadata)>>::Error>,
        SampleError<<KeyserverClient<S> as Service<(Uri, PutMetadata)>>::Error>,
    > {
        let read_uris = self.uris.read().await;
        let uris = uniform_random_sampler(&read_uris, sample_size)
            .into_iter()
            .map(|uri| append_path(uri, &format!("/keys/{}", address)))
            .collect::<Vec<Uri>>();

        let request = PutRawAuthWrapper {
            token,
            raw_auth_wrapper,
        };
        let sample_request = SampleRequest { uris, request };
        let responses = self.inner_client.clone().call(sample_request).await?;

        Ok(AggregateResponse::aggregate(responses, |_| ()))
    }
}
