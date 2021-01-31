//! This module contains the [`Wallet`] struct which allows for basic caching and payment of invoices.

use std::{fmt, sync::Arc, time::Duration};

use dashmap::DashMap;
use thiserror::Error;
use tokio::time::sleep;

/// Received unexpected outputs.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("received unexpected outputs")]
pub struct UnexpectedOutputs;

/// Provides a simple interface to allow parallel caching and retrieval of UTXOs.
#[derive(Clone)]
pub struct Wallet<K, O> {
    timeout: Duration,
    pending: Arc<DashMap<K, Vec<O>>>, // script:amount
}

// NOTE: CHALK will remove the need for this manual impl
impl<K: fmt::Debug + std::cmp::Eq, O: fmt::Debug> fmt::Debug for Wallet<K, O>
where
    K: fmt::Debug + std::cmp::Eq + std::hash::Hash,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Wallet {{\n\ttimeout: {:?},\n\tpending: {:?}\n}}",
            self.timeout, self.pending
        )
    }
}

impl<K, O> Wallet<K, O>
where
    K: std::hash::Hash + std::cmp::Eq,
    K: Clone + Send + Sync + 'static,
    O: std::cmp::PartialEq + Sync + Send + 'static,
{
    /// Create a new [`Wallet`] where the payments are cached for a given [`Duration`].
    pub fn new(timeout: Duration) -> Self {
        Wallet {
            timeout,
            pending: Default::default(),
        }
    }

    /// Synchronously adds outputs to the wallet and returns a delayed Future removing the output.
    pub fn add_outputs(
        &self,
        key: K,
        outputs: Vec<O>,
    ) -> impl std::future::Future<Output = ()> + Send + 'static {
        // TODO: Check whether pre-existing?
        let key_inner = key.clone();
        self.pending.insert(key, outputs);

        let pending_inner = self.pending.clone();
        let timeout_inner = self.timeout;

        // Remove from pending map after timeout
        async move {
            sleep(timeout_inner).await;
            pending_inner.remove(&key_inner);
        }
    }

    /// Removes an output from the wallet, else raises an error.
    pub fn recv_outputs(&self, key: &K, outputs: &[O]) -> Result<(), UnexpectedOutputs> {
        let check_subset = |_: &K, expected_outputs: &Vec<O>| {
            expected_outputs
                .iter()
                .all(|output| outputs.contains(output))
        };

        if self.pending.remove_if(key, check_subset).is_some() {
            Ok(())
        } else {
            Err(UnexpectedOutputs)
        }
    }
}
