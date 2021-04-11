//! This module contains lower-level primitives for working with the [`KeyserverClient`].

use std::{fmt, pin::Pin};

use futures_core::{
    task::{Context, Poll},
    Future,
};
use futures_util::future::{join, join_all};
use hyper::{
    body::{aggregate, to_bytes},
    http::header::AUTHORIZATION,
    http::Method,
    Body, Error as HyperError, Request, Response, StatusCode,
};
pub use hyper::{
    client::{connect::Connect, HttpConnector},
    Uri,
};
use prost::{DecodeError, Message as _};
use thiserror::Error;
use tower_service::Service;

use super::{KeyserverClient, MetadataPackage, RawAuthWrapperPackage};
use crate::models::*;

type FutResponse<Response, Error> =
    Pin<Box<dyn Future<Output = Result<Response, Error>> + 'static + Send>>;

/// Represents a request for the [`Peers`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetPeers;

/// Error associated with getting [`Peers`] from a keyserver.
#[derive(Debug, Error)]
pub enum GetPeersError<E: fmt::Debug + fmt::Display> {
    /// Error while processing the body.
    #[error("processing body failed: {0}")]
    Body(HyperError),
    /// A connection error occured.
    #[error("connection failure: {0}")]
    Service(E),
    /// Error while decoding the body.
    #[error("body decoding failure: {0}")]
    Decode(DecodeError),
    /// Unexpected status code.
    #[error("unexpected status code: {0}")]
    UnexpectedStatusCode(u16),
    /// Peering is disabled on the keyserver.
    #[error("peering disabled")]
    PeeringDisabled,
}

impl<S> Service<(Uri, GetPeers)> for KeyserverClient<S>
where
    S: Service<Request<Body>, Response = Response<Body>>,
    S: Send + Clone + 'static,
    S::Error: fmt::Debug,
    <S as Service<Request<Body>>>::Error: fmt::Display,
    <S as Service<Request<Body>>>::Future: Send,
{
    type Response = Peers;
    type Error = GetPeersError<S::Error>;
    type Future = FutResponse<Self::Response, Self::Error>;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner_client
            .poll_ready(context)
            .map_err(GetPeersError::Service)
    }

    fn call(&mut self, (uri, _): (Uri, GetPeers)) -> Self::Future {
        let mut client = self.inner_client.clone();
        let http_request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Body::empty())
            .unwrap(); // This is safe

        let fut = async move {
            let response = client
                .call(http_request)
                .await
                .map_err(Self::Error::Service)?;
            match response.status() {
                StatusCode::OK => (),
                StatusCode::NOT_IMPLEMENTED => return Err(Self::Error::PeeringDisabled),
                code => return Err(Self::Error::UnexpectedStatusCode(code.as_u16())),
            }
            let body = response.into_body();
            let buf = aggregate(body).await.map_err(Self::Error::Body)?;
            let peers = Peers::decode(buf).map_err(Self::Error::Decode)?;
            Ok(peers)
        };
        Box::pin(fut)
    }
}

/// Represents a request for the raw [`AuthWrapper`].
///
/// This will not error on invalid bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GetRawAuthWrapper;

/// Error associated with getting raw [`AuthWrapper`] from a keyserver.
#[derive(Debug, Error)]
pub enum GetRawAuthWrapperError<E: fmt::Debug + fmt::Display> {
    /// Error while processing the body.
    #[error("processing body failed: {0}")]
    Body(HyperError),
    /// A connection error occured.
    #[error("connection failure: {0}")]
    Service(E),
    /// Unexpected status code.
    #[error("unexpected status code: {0}")]
    UnexpectedStatusCode(u16),
    /// POP token missing from headers.
    #[error("missing token")]
    MissingToken,
}

impl<S> Service<(Uri, GetRawAuthWrapper)> for KeyserverClient<S>
where
    S: Service<Request<Body>, Response = Response<Body>>,
    S: Send + Clone + 'static,
    S::Future: Send,
    S::Error: fmt::Debug + fmt::Display,
{
    type Response = RawAuthWrapperPackage;
    type Error = GetRawAuthWrapperError<S::Error>;
    type Future = FutResponse<Self::Response, Self::Error>;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner_client
            .poll_ready(context)
            .map_err(GetRawAuthWrapperError::Service)
    }

    fn call(&mut self, (uri, _): (Uri, GetRawAuthWrapper)) -> Self::Future {
        let mut client = self.inner_client.clone();
        let http_request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Body::empty())
            .unwrap(); // This is safe
        let fut = async move {
            // Get response
            let response = client
                .call(http_request)
                .await
                .map_err(Self::Error::Service)?;

            // Check status code
            // TODO: Fix this
            match response.status() {
                StatusCode::OK => (),
                code => return Err(Self::Error::UnexpectedStatusCode(code.as_u16())),
            }

            #[allow(clippy::borrow_interior_mutable_const)]
            let token = response
                .headers()
                .into_iter()
                .find(|(name, value)| {
                    *name == AUTHORIZATION && value.as_bytes()[..4] == b"POP "[..]
                })
                .ok_or(Self::Error::MissingToken)?
                .0
                .to_string();

            // Aggregate body
            let body = response.into_body();
            let raw_auth_wrapper = to_bytes(body).await.map_err(Self::Error::Body)?;

            Ok(RawAuthWrapperPackage {
                token,
                raw_auth_wrapper,
            })
        };
        Box::pin(fut)
    }
}

/// Represents a request for the [`AddressMetadata`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetMetadata;

/// Error associated with getting [`AddressMetadata`] from a keyserver.
#[derive(Debug, Error)]
pub enum GetMetadataError<E: fmt::Debug + fmt::Display> {
    /// Error while decoding the [`AddressMetadata`]
    #[error("metadata decoding failure: {0}")]
    MetadataDecode(DecodeError),
    /// Error while decoding the [`AuthWrapper`].
    #[error("authwrapper decoding failure: {0}")]
    AuthWrapperDecode(DecodeError),
    /// Error while parsing the [`AuthWrapper`].
    #[error("authwrapper parsing failure: {0}")]
    AuthWrapperParse(ParseError),
    /// Error while parsing the [`AuthWrapper`].
    #[error("authwrapper verification failure: {0}")]
    AuthWrapperVerify(VerifyError),
    /// Error while processing the body.
    #[error("processing body failed: {0}")]
    Body(HyperError),
    /// A connection error occured.
    #[error("connection failure: {0}")]
    Service(E),
    /// Unexpected status code.
    #[error("unexpected status code: {0}")]
    UnexpectedStatusCode(u16),
    /// POP token missing from headers.
    #[error("missing token")]
    MissingToken,
}

impl<S> Service<(Uri, GetMetadata)> for KeyserverClient<S>
where
    S: Service<Request<Body>, Response = Response<Body>>,
    S: Send + Clone + 'static,
    S::Future: Send,
    S::Error: fmt::Debug + fmt::Display,
{
    type Response = MetadataPackage;
    type Error = GetMetadataError<S::Error>;
    type Future = FutResponse<Self::Response, Self::Error>;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner_client
            .poll_ready(context)
            .map_err(GetMetadataError::Service)
    }

    fn call(&mut self, (uri, _): (Uri, GetMetadata)) -> Self::Future {
        let mut client = self.inner_client.clone();
        let http_request = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Body::empty())
            .unwrap(); // This is safe
        let fut = async move {
            // Get response
            let response = client
                .call(http_request)
                .await
                .map_err(Self::Error::Service)?;

            // Check status code
            // TODO: Fix this
            match response.status() {
                StatusCode::OK => (),
                code => return Err(Self::Error::UnexpectedStatusCode(code.as_u16())),
            }

            #[allow(clippy::borrow_interior_mutable_const)]
            let token = response
                .headers()
                .into_iter()
                .find(|(name, value)| {
                    *name == AUTHORIZATION && value.as_bytes()[..4] == b"POP "[..]
                })
                .ok_or(Self::Error::MissingToken)?
                .0
                .to_string();

            // Deserialize and decode body
            let body = response.into_body();
            let raw_auth_wrapper = to_bytes(body).await.map_err(Self::Error::Body)?;
            let auth_wrapper = AuthWrapper::decode(raw_auth_wrapper.clone())
                .map_err(Self::Error::AuthWrapperDecode)?;

            // Parse auth wrapper
            let parsed_auth_wrapper = auth_wrapper
                .parse()
                .map_err(Self::Error::AuthWrapperParse)?;

            // Verify signature
            parsed_auth_wrapper
                .verify()
                .map_err(Self::Error::AuthWrapperVerify)?;

            // Decode metadata
            let metadata = AddressMetadata::decode(&mut parsed_auth_wrapper.payload.as_slice())
                .map_err(Self::Error::MetadataDecode)?;

            Ok(MetadataPackage {
                token,
                public_key: parsed_auth_wrapper.public_key,
                metadata,
                raw_auth_wrapper,
            })
        };
        Box::pin(fut)
    }
}

/// Request for putting [`AuthWrapper`] to the keyserver.
#[derive(Debug, Clone, PartialEq)]
pub struct PutMetadata {
    /// POP authorization token.
    pub token: String,
    /// The [`AuthWrapper`] to be put to the keyserver.
    pub auth_wrapper: AuthWrapper,
}

/// Error associated with putting [`AddressMetadata`] to the keyserver.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PutMetadataError<E: fmt::Debug + fmt::Display> {
    /// A connection error occured.
    #[error("connection failure: {0}")]
    Service(E),
    /// Unexpected status code.
    #[error("unexpected status code: {0}")]
    UnexpectedStatusCode(u16),
}

impl<S> Service<(Uri, PutMetadata)> for KeyserverClient<S>
where
    S: Service<Request<Body>, Response = Response<Body>>,
    S: Send + Clone + 'static,
    S::Error: fmt::Debug + fmt::Display,
    S::Future: Send,
{
    type Response = ();
    type Error = PutMetadataError<S::Error>;
    type Future = FutResponse<Self::Response, Self::Error>;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner_client
            .poll_ready(context)
            .map_err(PutMetadataError::Service)
    }

    fn call(&mut self, (uri, request): (Uri, PutMetadata)) -> Self::Future {
        let mut client = self.inner_client.clone();

        // Construct body
        let mut body = Vec::with_capacity(request.auth_wrapper.encoded_len());
        request.auth_wrapper.encode(&mut body).unwrap();

        let http_request = Request::builder()
            .method(Method::PUT)
            .uri(uri)
            .header(AUTHORIZATION, request.token)
            .body(Body::from(body))
            .unwrap(); // This is safe

        let fut = async move {
            // Get response
            let response = client
                .call(http_request)
                .await
                .map_err(Self::Error::Service)?;

            // Check status code
            // TODO: Fix this
            match response.status() {
                StatusCode::OK => (),
                code => return Err(Self::Error::UnexpectedStatusCode(code.as_u16())),
            }

            Ok(())
        };
        Box::pin(fut)
    }
}

/// Request for putting a raw [`AuthWrapper`] to the keyserver.
#[derive(Debug, Clone, PartialEq)]
pub struct PutRawAuthWrapper {
    /// POP authorization token.
    pub token: String,
    /// The raw [`AuthWrapper`] to be put to the keyserver.
    pub raw_auth_wrapper: Vec<u8>,
}

impl<S> Service<(Uri, PutRawAuthWrapper)> for KeyserverClient<S>
where
    S: Service<Request<Body>, Response = Response<Body>>,
    S: Send + Clone + 'static,
    S::Error: fmt::Debug + fmt::Display,
    S::Future: Send,
{
    type Response = ();
    type Error = PutMetadataError<S::Error>;
    type Future = FutResponse<Self::Response, Self::Error>;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner_client
            .poll_ready(context)
            .map_err(PutMetadataError::Service)
    }

    fn call(&mut self, (uri, request): (Uri, PutRawAuthWrapper)) -> Self::Future {
        let mut client = self.inner_client.clone();

        // Construct body
        let body = request.raw_auth_wrapper;

        let http_request = Request::builder()
            .method(Method::PUT)
            .uri(uri)
            .header(AUTHORIZATION, request.token)
            .body(Body::from(body))
            .unwrap(); // This is safe

        let fut = async move {
            // Get response
            let response = client
                .call(http_request)
                .await
                .map_err(Self::Error::Service)?;

            // Check status code
            // TODO: Fix this
            match response.status() {
                StatusCode::OK => (),
                code => return Err(Self::Error::UnexpectedStatusCode(code.as_u16())),
            }

            Ok(())
        };
        Box::pin(fut)
    }
}

/// Request for performing multiple requests to a range of keyservers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleRequest<T> {
    /// The [`Uri`]s of the targetted keyservers.
    pub uris: Vec<Uri>,
    /// The request to be broadcast.
    pub request: T,
}

/// Error associated with sending sample requests.
#[derive(Debug, Error)]
pub enum SampleError<E: fmt::Debug + fmt::Display> {
    /// Error while polling service.
    #[error("polling failure: {0}")]
    Poll(E),
    /// Sample totally failed. Contains errors paired with the [`Uri`] of the keyserver they originated at.
    #[error("sampling failure: {0:?}")] // TODO: Make this prettier
    Sample(Vec<(Uri, E)>),
}

impl<S, T> Service<SampleRequest<T>> for KeyserverClient<S>
where
    T: Send + 'static + Clone + Sized,
    S: Send + Clone + 'static,
    Self: Service<(Uri, T)>,
    <Self as Service<(Uri, T)>>::Response: Send + fmt::Debug,
    <Self as Service<(Uri, T)>>::Error: fmt::Debug + fmt::Display + Send,
    <Self as Service<(Uri, T)>>::Future: Send,
{
    #[allow(clippy::type_complexity)]
    type Response = Vec<(
        Uri,
        Result<<Self as Service<(Uri, T)>>::Response, <Self as Service<(Uri, T)>>::Error>,
    )>;
    type Error = SampleError<<Self as Service<(Uri, T)>>::Error>;
    type Future = FutResponse<Self::Response, Self::Error>;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_ready(context).map_err(SampleError::Poll)
    }

    fn call(&mut self, SampleRequest { uris, request }: SampleRequest<T>) -> Self::Future {
        let mut inner_client = self.clone();

        let fut = async move {
            // Collect futures
            let response_futs = uris.into_iter().map(move |uri| {
                let response_fut = inner_client.call((uri.clone(), request.clone()));
                let uri_fut = async move { uri };
                join(uri_fut, response_fut)
            });
            let responses: Vec<(Uri, Result<_, _>)> = join_all(response_futs).await;

            // If no successes then return all errors
            if responses.iter().all(|(_, res)| res.is_err()) {
                let errors = responses
                    .into_iter()
                    .map(|(uri, result)| (uri, result.unwrap_err()))
                    .collect();
                return Err(SampleError::Sample(errors));
            }

            Ok(responses)
        };
        Box::pin(fut)
    }
}
