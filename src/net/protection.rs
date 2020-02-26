use std::{
    fmt,
    sync::Arc,
    time::{Duration, SystemTime},
};

use bitcoin::TxOut;
use bitcoincash_addr::Address;
use cashweb::{
    payments::wallet::Wallet,
    token::{extract_pop, schemes::hmac_bearer::*, TokenValidator},
};
use http::header::HeaderMap;
use json_rpc::clients::http::HttpConnector;
use warp::{http::Response, hyper::Body, reject::Reject};

use crate::bitcoin::BitcoinClient;

#[derive(Debug)]
pub enum ProtectionError {
    MissingToken(
        Arc<HmacTokenScheme>,
        Wallet<Vec<u8>, TxOut>,
        BitcoinClient<HttpConnector>,
    ),
    Validation(ValidationError),
}

impl fmt::Display for ProtectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingToken(_, _, _) => f.write_str("missing token"),
            Self::Validation(err) => err.fmt(f),
        }
    }
}

pub async fn protection_error_recovery(err: ProtectionError) -> Response<Body> {
    match err {
        ProtectionError::Validation(_) => {
            return Response::builder()
                .status(400)
                .body(Body::from(err.to_string()))
                .unwrap()
        }
        ProtectionError::MissingToken(token_scheme, wallet, bitcoin_client) => {
            // Valid interval
            let current_time = SystemTime::now();
            // let expiry_time = current_time + Duration::from_secs(VALID_DURATION);

            // Get new addr and add to wallet
            // let wallet_state_inner = self.wallet_state.clone();
            // let client_inner = self.client.clone();

            // let new_addr = async move {
            //     let addr_opt = client_inner.get_new_addr().await;
            //     match addr_opt {
            //         Ok(addr_str) => {
            //             let addr =
            //                 Address::decode(&addr_str).map_err(|(cash_err, base58_err)| {
            //                     ServerError::Address(cash_err, base58_err)
            //                 })?;
            //             let network: Network = addr.network.clone().into();
            //             if network != SETTINGS.network || addr.hash_type != HashType::Key {
            //                 return Err(
            //                     ServerError::Payment(PaymentError::MismatchedNetwork).into()
            //                 );
            //                 // TODO: Finer grained error here
            //             }
            //             let addr_raw = addr.into_body();
            //             wallet_state_inner.add(addr_raw.clone());
            //             Ok(addr_raw)
            //         }
            //         Err(_e) => Err(ServerError::Payment(PaymentError::AddrFetchFailed).into()),
            //     }
            // };

            // // Generate merchant data
            // let base_url = format!("{}://{}", scheme, host);

            // let response = new_addr.and_then(move |addr_raw| {
            //     // Generate outputs
            //     let outputs = generate_outputs(&addr_raw);

            //     // Collect payment details
            //     let payment_url = Some(format!("{}{}", base_url, PAYMENT_PATH));
            //     let payment_details = PaymentDetails {
            //         network: Some(SETTINGS.network.to_string()),
            //         time: current_time.duration_since(UNIX_EPOCH).unwrap().as_secs(),
            //         expires: Some(expiry_time.duration_since(UNIX_EPOCH).unwrap().as_secs()),
            //         memo: None,
            //         merchant_data: Some(put_addr.into_body()),
            //         outputs,
            //         payment_url,
            //     };
            //     let mut serialized_payment_details =
            //         Vec::with_capacity(payment_details.encoded_len());
            //     payment_details
            //         .encode(&mut serialized_payment_details)
            //         .unwrap();

            //     // Generate payment invoice
            //     let pki_type = Some("none".to_string());
            //     let payment_invoice = PaymentRequest {
            //         pki_type,
            //         pki_data: None,
            //         payment_details_version: Some(1),
            //         serialized_payment_details,
            //         signature: None,
            //     };
            //     let mut payment_invoice_raw = Vec::with_capacity(payment_invoice.encoded_len());
            //     payment_invoice.encode(&mut payment_invoice_raw).unwrap();

            //     HttpResponse::PaymentRequired()
            //         .content_type("application/bitcoincash-paymentrequest")
            //         .header("Content-Transfer-Encoding", "binary")
            //         .body(payment_invoice_raw)
            // });

            // // Respond
            // return Box::pin(response.map_ok(move |resp| req.into_response(resp)));
            unreachable!()
        }
    }
}

impl Reject for ProtectionError {}

pub async fn pop_protection(
    addr: Address,
    header_map: HeaderMap,
    token_scheme: Arc<HmacTokenScheme>,
    wallet: Wallet<Vec<u8>, TxOut>,
    bitcoin_client: BitcoinClient<HttpConnector>,
) -> Result<Address, ProtectionError> {
    let pop_token = extract_pop(&header_map).ok_or(ProtectionError::MissingToken(
        token_scheme.clone(),
        wallet,
        bitcoin_client,
    ))?;
    token_scheme
        .validate_token(addr.as_body().to_vec(), pop_token)
        .await
        .map_err(ProtectionError::Validation)?;
    Ok(addr)
}
