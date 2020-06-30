#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![allow(elided_lifetimes_in_paths)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! `cashweb-bitcoin-client` is a library providing [`KeyserverClient`] which allows
//! interaction with specific keyservers and [`KeyserverManager`]
//! which allows sampling and aggregation over multiple keyservers.

mod client;
mod manager;
#[allow(missing_docs)]
pub mod models;

pub use client::*;
pub use manager::*;
