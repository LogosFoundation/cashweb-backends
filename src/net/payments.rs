use std::fmt;

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

impl fmt::Display for PaymentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let printable = match self {
            Self::Preprocess(err) => return err.fmt(f),
            Self::Wallet(err) => return err.fmt(f),
            Self::MalformedTx(err) => return err.fmt(f),
            Self::MissingMerchantData => "missing merchant data",
        };
        f.write_str(printable)
    }
}

impl Reject for PaymentError {}

pub fn payment_error_recovery(err: &PaymentError) -> Response<String> {
    let code = match err {
        PaymentError::Preprocess(err) => match err {
            PreprocessingError::MissingAcceptHeader => 406,
            PreprocessingError::MissingContentTypeHeader => 415,
            PreprocessingError::PaymentDecode(_) => 400,
        },
        PaymentError::Wallet(err) => match err {
            WalletError::NotFound => 404,
            WalletError::InvalidOutputs => 400,
        },
        PaymentError::MalformedTx(_) => 400,
        PaymentError::MissingMerchantData => 400,
    };
    Response::builder()
        .status(code)
        .body(err.to_string())
        .unwrap()
}

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
