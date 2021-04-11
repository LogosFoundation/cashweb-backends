#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]

//! `cashweb` is a collection of useful components, designed for quick integration of the following protocols:
//! * [Proof-of-Payment Authorization Framework](https://github.com/cashweb/specifications/blob/master/proof-of-payment-token/specification.mediawiki)
//! * [Authorization Wrapper Protocol](https://github.com/cashweb/specifications/blob/master/authorization-wrapper-protocol/specification.mediawiki)
//! * [Keyserver Protocol](https://github.com/cashweb/specifications/blob/master/keyserver-protocol/specification.mediawiki)
//! * [Relay Server Protocol](https://github.com/cashweb/specifications/blob/master/relay-server-protocol/specification.mediawiki)

#[doc(inline)]
pub use auth_wrapper;
#[doc(inline)]
pub use bitcoin;
#[doc(inline)]
pub use bitcoin_client;
#[doc(inline)]
pub use keyserver;
#[doc(inline)]
pub use keyserver_client;
#[doc(inline)]
pub use payments;
#[doc(inline)]
pub use relay;
#[doc(inline)]
pub use relay_client;
#[doc(inline)]
pub use secp256k1;
#[doc(inline)]
pub use token;
