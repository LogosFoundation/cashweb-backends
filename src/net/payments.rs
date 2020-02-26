use std::fmt;

use bitcoin::{
    consensus::encode::Error as BitcoinError, util::psbt::serialize::Deserialize, Transaction,
    TxOut,
};
use bitcoincash_addr::{
    base58::DecodingError as Base58Error, cashaddr::DecodingError as CashAddrError, Address,
};
use cashweb::payments::{
    wallet::{Wallet as WalletGeneric, WalletError},
    PreprocessingError,
};
use json_rpc::clients::http::HttpConnector;
use warp::{http::Response, hyper::Body, reject::Reject};

use crate::{
    bitcoin::{BitcoinClient, BitcoinError as NodeError},
    models::bip70::{Payment, Output},
    SETTINGS
};

pub type Wallet = WalletGeneric<Vec<u8>, TxOut>;

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

pub fn payment_error_recovery(err: &PaymentError) -> Response<Body> {
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
        .body(Body::from(err.to_string()))
        .unwrap()
}

pub async fn process_payment(
    payment: Payment,
    wallet: Wallet,
) -> Result<Response<Body>, PaymentError> {
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
    Ok(Response::builder().body(Body::empty()).unwrap())
}

#[derive(Debug)]
pub enum PaymentRequestError {
    Address(CashAddrError, Base58Error),
    Bitcoin(NodeError),
    MismatchedNetwork
}

pub async fn generate_payment_request(
    addr: &Address,
    wallet: Wallet,
    bitcoin_client: BitcoinClient<HttpConnector>,
) -> Result<Response<Body>, PaymentRequestError> {
    let output_addr_str = bitcoin_client
        .get_new_addr()
        .await
        .map_err(PaymentRequestError::Bitcoin)?;
    let output_addr = Address::decode(&output_addr_str)
        .map_err(|(cash_err, base58_err)| PaymentRequestError::Address(cash_err, base58_err))?;

    // Generate output
    let p2pkh_script_pre: [u8; 3] = [118, 169, 20];
    let p2pkh_script_post: [u8; 2] = [136, 172];
    let script = [&p2pkh_script_pre[..], output_addr.as_body(), &p2pkh_script_post[..]].concat();
    let output = Output {
        amount: Some(SETTINGS.token_fee),
        script 
    };
    let cleanup = wallet.add_outputs(addr.as_body().to_vec(), vec![]);
    tokio::spawn(cleanup);

    Ok(Response::builder().status(402).body(Body::empty()).unwrap())
}
