use std::pin::Pin;

use bitcoincash_addr::Address;
use futures_core::{
    task::{Context, Poll},
    Future,
};
use hyper::{body, Body};
use prost::Message as _;
use tower_service::Service;

use crate::models::messaging::{MessageSet};
use super::{Database, errors::{GetError, PushError, GetFiltersError}};

pub struct GetMessagesRequest {
    address: String,
    start: u64,
    count: Option<u64>,
    take: bool
}

impl Service<GetMessagesRequest> for Database {
    type Response = Vec<u8>;
    type Error = ();
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: GetMessagesRequest) -> Self::Future {
        let db_inner = self.clone();
        let fut = async move {
            // Convert address
            let addr = Address::decode(&request.address).unwrap();

            // Grab metadata from DB
            let message_set = db_inner.get_messages(&addr.into_body(), request.start, request.count).unwrap();

            // Serialize messages
            let mut raw_payload = Vec::with_capacity(message_set.encoded_len());
            message_set.encode(&mut raw_payload).unwrap();

            // Respond
            Ok(raw_payload)
        };
        Box::pin(fut)
    }
}

pub struct PushMessageRequest {
    address: String,
    body: Body
}

impl Service<PushMessageRequest> for Database {
    type Response = ();
    type Error = PushError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: PushMessageRequest) -> Self::Future {
        let db_inner = self.clone();
        let fut = async move {
            // Convert address
            let addr = Address::decode(&request.address)?;

            // Decode messages
            let body_raw = body::aggregate(request.body).await.map_err(PushError::Buffer)?;
            let message_page = MessageSet::decode(body_raw).map_err(PushError::MessageDecode)?;

            Ok(())
        };
        Box::pin(fut)
    }
}

pub struct GetFiltersRequest {
    address: String,
}

impl Service<GetFiltersRequest> for Database {
    type Response = Vec<u8>;
    type Error = GetFiltersError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: GetFiltersRequest) -> Self::Future {
        let db_inner = self.clone();
        let fut = async move {
            // Convert address
            let addr = Address::decode(&request.address)?;

            // Get filters
            let mut filters = db_inner.get_filters(addr.as_body())?.ok_or(GetFiltersError::NotFound)?;

            // Don't show private filters
            if let Some(price_filter) = &filters.price_filter {
                if !price_filter.public {
                    filters.price_filter = None;
                }
            }

            // Serialize messages
            let mut raw_payload = Vec::with_capacity(filters.encoded_len());
            filters.encode(&mut raw_payload).unwrap();

            Ok(raw_payload)
        };
        Box::pin(fut)
    }
}