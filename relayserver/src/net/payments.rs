use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use bitcoincash_addr::{base58, cashaddr, Address};
use cashweb::{
    bitcoin::{
        transaction::{self, Transaction},
        Decodable,
    },
    bitcoin_client::{BitcoinClient, BitcoinClientHTTP, NodeError},
    payments::bip70::{Output, Payment, PaymentAck, PaymentDetails, PaymentRequest},
    payments::{
        wallet::{self, UnexpectedOutputs},
        PreprocessingError,
    },
    token::schemes::hmac_bearer::HmacScheme,
};
use prost::Message as _;
use thiserror::Error;
use tracing::info;
use warp::{
    http::{header::AUTHORIZATION, Response},
    hyper::Body,
    reject::Reject,
};

use crate::{net::ToResponse, PAYMENTS_PATH, SETTINGS};

pub type Wallet = wallet::Wallet<Vec<u8>, Output>;

#[derive(Debug, Error)]
pub enum PaymentError {
    #[error("preprocessing failed: {0}")]
    Preprocess(PreprocessingError),
    #[error(transparent)]
    Wallet(UnexpectedOutputs),
    #[error("malformed tx: {0}")]
    MalformedTx(transaction::DecodeError),
    #[error("missing merchant data")]
    MissingMerchantData,
    #[error("bitcoin request failed: {0}")]
    Node(NodeError),
}

impl Reject for PaymentError {}

impl ToResponse for PaymentError {
    fn to_status(&self) -> u16 {
        match self {
            PaymentError::Preprocess(err) => match err {
                PreprocessingError::MissingAcceptHeader => 406,
                PreprocessingError::MissingContentTypeHeader => 415,
                PreprocessingError::PaymentDecode(_) => 400,
            },
            PaymentError::Wallet(_) => 404,
            PaymentError::MalformedTx(_) => 400,
            PaymentError::MissingMerchantData => 400,
            PaymentError::Node(err) => match err {
                NodeError::Rpc(_) => 400,
                _ => 500,
            },
        }
    }
}

pub async fn process_payment(
    payment: Payment,
    wallet: Wallet,
    bitcoin_client: BitcoinClientHTTP,
    token_state: Arc<HmacScheme>,
) -> Result<Response<Body>, PaymentError> {
    let txs_res: Result<Vec<Transaction>, transaction::DecodeError> = payment
        .transactions
        .iter()
        .map(|raw_tx: &Vec<u8>| Transaction::decode(&mut raw_tx.as_slice()))
        .collect();
    let txs = txs_res.map_err(PaymentError::MalformedTx)?;
    let outputs: Vec<Output> = txs
        .into_iter()
        .map(move |tx| tx.outputs)
        .flatten()
        .map(move |output| Output {
            amount: Some(output.value),
            script: output.script.into_bytes(),
        })
        .collect();

    let pubkey_hash = payment
        .merchant_data
        .as_ref()
        .ok_or(PaymentError::MissingMerchantData)?;

    info!(message = "checking wallet", outputs = ?outputs, address_payload = ?pubkey_hash);
    wallet
        .recv_outputs(pubkey_hash, &outputs)
        .map_err(PaymentError::Wallet)?;

    for tx in &payment.transactions {
        bitcoin_client
            .send_tx(tx)
            .await
            .map_err(PaymentError::Node)?;
    }

    // Construct token
    let token = format!("POP {}", token_state.construct_token(pubkey_hash));

    // Create PaymentAck
    let memo = Some(SETTINGS.payments.memo.clone());
    let payment_ack = PaymentAck { payment, memo };

    // Encode payment ack
    let mut raw_ack = Vec::with_capacity(payment_ack.encoded_len());
    payment_ack.encode(&mut raw_ack).unwrap();

    Ok(Response::builder()
        .header(AUTHORIZATION, token)
        .body(Body::from(raw_ack))
        .unwrap())
}

#[derive(Error, Debug)]
pub enum PaymentRequestError {
    #[error("address decoding failed: {0}, {1}")]
    Address(cashaddr::DecodingError, base58::DecodingError),
    #[error("failed to retrieve address from bitcoind: {0}")]
    Node(NodeError),
    #[error("mismatched network")]
    MismatchedNetwork,
}

pub async fn generate_payment_request(
    addr: Address,
    wallet: Wallet,
    bitcoin_client: BitcoinClientHTTP,
) -> Result<Response<Body>, PaymentRequestError> {
    let output_addr_str = bitcoin_client
        .get_new_addr()
        .await
        .map_err(PaymentRequestError::Node)?;
    let output_addr = Address::decode(&output_addr_str)
        .map_err(|(cash_err, base58_err)| PaymentRequestError::Address(cash_err, base58_err))?;

    // Generate output
    let p2pkh_script_pre: [u8; 3] = [118, 169, 20];
    let p2pkh_script_post: [u8; 2] = [136, 172];
    let script = [
        &p2pkh_script_pre[..],
        output_addr.as_body(),
        &p2pkh_script_post[..],
    ]
    .concat();
    let output = Output {
        amount: Some(SETTINGS.payments.token_fee),
        script,
    };
    let cleanup = wallet.add_outputs(addr.as_body().to_vec(), vec![output.clone()]);
    info!(message = "added to wallet", output = ?output, address_payload = ?addr.as_body());
    tokio::spawn(cleanup);

    // Valid interval
    let current_time = SystemTime::now();
    let expiry_time = current_time + Duration::from_millis(SETTINGS.payments.timeout);

    let payment_details = PaymentDetails {
        network: Some(SETTINGS.network.to_string()),
        time: current_time.duration_since(UNIX_EPOCH).unwrap().as_secs(),
        expires: Some(expiry_time.duration_since(UNIX_EPOCH).unwrap().as_secs()),
        memo: None,
        merchant_data: Some(addr.into_body()),
        outputs: vec![output],
        payment_url: Some(format!("/{}", PAYMENTS_PATH)),
    };
    let mut serialized_payment_details = Vec::with_capacity(payment_details.encoded_len());
    payment_details
        .encode(&mut serialized_payment_details)
        .unwrap();

    // Generate payment invoice
    // TODO: Signing
    let pki_type = Some("none".to_string());
    let payment_invoice = PaymentRequest {
        pki_type,
        pki_data: None,
        payment_details_version: Some(1),
        serialized_payment_details,
        signature: None,
    };
    let mut payment_invoice_raw = Vec::with_capacity(payment_invoice.encoded_len());
    payment_invoice.encode(&mut payment_invoice_raw).unwrap();

    Ok(Response::builder()
        .status(402)
        .body(Body::from(payment_invoice_raw))
        .unwrap())
}
