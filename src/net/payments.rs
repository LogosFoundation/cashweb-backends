use std::{sync::Arc, time::Duration};

use bitcoin::{
    consensus::encode::Error as BitcoinError, util::psbt::serialize::Deserialize, Transaction,
    TxOut,
};
use cashweb::process::PreprocessingError;
use dashmap::DashMap;
use tokio::time::delay_for;
use warp::reject::Reject;

use crate::models::bip70::Payment;

pub enum WalletError {
    NotFound,
    IncorrectAmount,
}

pub enum PaymentError {
    Preprocess(PreprocessingError),
    Wallet(WalletError),
    MalformedTx(BitcoinError),
}

impl Reject for PaymentError {}

#[derive(Clone)]
pub struct Wallet {
    timeout: Duration,
    pending: Arc<DashMap<Vec<u8>, u64>>, // script:amount
}

impl Wallet {
    pub fn new(timeout: Duration) -> Self {
        Wallet {
            timeout,
            pending: Default::default(),
        }
    }

    pub fn add_output(&self, script: Vec<u8>, amount: u64) {
        let script_inner = script.clone();
        self.pending.insert(script_inner, amount);

        // Remove from pending map after timeout
        let pending_inner = self.pending.clone();
        let timeout_inner = self.timeout;
        let cleanup = async move {
            delay_for(timeout_inner).await;
            pending_inner.remove(&script);
        };
        tokio::spawn(cleanup);
    }

    pub fn recv_output(&self, script: Vec<u8>, amount: u64) -> Result<(), WalletError> {
        // TODO: Use conditional remove here
        let expected_amount = self
            .pending
            .get(&script)
            .ok_or(WalletError::NotFound)?
            .value();
        if expected_amount == &amount {
            self.pending.remove(&script);
            Ok(())
        } else {
            Err(WalletError::IncorrectAmount)
        }
    }
}

pub fn process_payment(payment: Payment, wallet: Wallet) -> Result<(), PaymentError> {
    let outputs: Vec<TxOut> = payment
        .transactions
        .iter()
        .map(|raw_tx| Transaction::deserialize(raw_tx))
        .map(|tx_res| tx_res.map(|tx| tx.output))
        .flatten()
        .collect();
}
