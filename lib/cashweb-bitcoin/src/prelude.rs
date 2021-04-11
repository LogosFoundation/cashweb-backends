//! A "prelude" for crates using the `cashweb-bitcoin` crate.
//!
//! This prelude is similar to the standard library's prelude in that you'll
//! almost always want to import its entire contents, but unlike the
//! standard library's prelude you'll have to do so manually:
//!
//! ```
//! # #[allow(unused_imports)]
//! use cashweb_bitcoin::prelude::*;
//! ```
//!
//! The prelude may grow over time as additional items see ubiquitous use.

#[doc(inline)]
pub use crate::{
    transaction::{
        input::{DecodeError as InputDecodeError, Input},
        outpoint::{DecodeError as OutpointDecodeError, Outpoint},
        output::{DecodeError as OutputDecodeError, Output},
        script::Script,
        DecodeError as TransactionDecodeError, Transaction,
    },
    var_int::{DecodeError as VarIntDecodeError, VarInt},
};
