use bitcoin::{
    consensus::encode::Error as BitcoinError, util::psbt::serialize::Deserialize, Transaction,
    TxOut,
};
use cashweb::payments::{
    wallet::{Wallet, WalletError},
    PreprocessingError,
};
use warp::{http::Response, reject::Reject};

use crate::models::bip70::Payment;

#[derive(Debug)]
pub enum PaymentError {
    Preprocess(PreprocessingError),
    Wallet(WalletError),
    MalformedTx(BitcoinError),
    MissingMerchantData,
}

impl Reject for PaymentError {}

pub async fn process_payment(
    payment: Payment,
    wallet: Wallet<Vec<u8>, TxOut>,
) -> Result<Response<()>, PaymentError> {
    let txs_res: Result<Vec<Transaction>, BitcoinError> = payment
        .transactions
        .iter()
        .map(|raw_tx| Transaction::deserialize(raw_tx))
        .collect();
    let txs = txs_res.map_err(PaymentError::MalformedTx)?;
    let outputs: Vec<TxOut> = txs.into_iter().map(move |tx| tx.output).flatten().collect();

    let pubkey_hash = payment
        .merchant_data
        .as_ref()
        .ok_or(PaymentError::MissingMerchantData)?;

    wallet
        .recv_outputs(pubkey_hash, &outputs)
        .map_err(PaymentError::Wallet)?;

    // TODO: Submit to chain
    Ok(Response::builder().body(()).unwrap())
}
