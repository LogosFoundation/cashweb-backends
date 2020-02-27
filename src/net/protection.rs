use std::{fmt, sync::Arc};

use bitcoincash_addr::Address;
use cashweb::token::{extract_pop, schemes::hmac_bearer::*, TokenValidator};
use http::header::HeaderMap;
use json_rpc::clients::http::HttpConnector;
use warp::{http::Response, hyper::Body, reject::Reject};

use crate::{
    bitcoin::BitcoinClient,
    net::payments::{generate_payment_request, Wallet},
};

#[derive(Debug)]
pub enum ProtectionError {
    MissingToken(Address, Wallet, BitcoinClient<HttpConnector>),
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

pub async fn protection_error_recovery(err: &ProtectionError) -> Response<Body> {
    match err {
        ProtectionError::Validation(_) => Response::builder()
            .status(400)
            .body(Body::from(err.to_string()))
            .unwrap(),
        ProtectionError::MissingToken(addr, wallet, bitcoin_client) => {
            // TODO: Remove clones here
            match generate_payment_request(addr.clone(), wallet.clone(), bitcoin_client.clone()).await {
                Ok(ok) => ok,
                Err(err) => Response::builder()
                    .status(400)
                    .body(Body::from(err.to_string()))
                    .unwrap(), // Err(err) => match err {
                               //     PaymentRequestError::Address(err) => Response::builder()
                               //         .status(400)
                               //         .body(Body::from(format!("{}, {}", err.0, err.1)))
                               //         .unwrap(),
                               //     PaymentRequestError::MismatchedNetwork =>
                               // }
            }
            // let response =

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

            //     HttpResponse::PaymentRequired()
            //         .content_type("application/bitcoincash-paymentrequest")
            //         .header("Content-Transfer-Encoding", "binary")
            //         .body(payment_invoice_raw)
            // });

            // // Respond
            // return Box::pin(response.map_ok(move |resp| req.into_response(resp)));
        }
    }
}

impl Reject for ProtectionError {}

pub async fn pop_protection(
    addr: Address,
    header_map: HeaderMap,
    token_scheme: Arc<HmacTokenScheme>,
    wallet: Wallet,
    bitcoin_client: BitcoinClient<HttpConnector>,
) -> Result<Address, ProtectionError> {
    match extract_pop(&header_map) {
        Some(pop_token) => {
            token_scheme
                .validate_token(addr.as_body().to_vec(), pop_token)
                .await
                .map_err(ProtectionError::Validation)?;
            Ok(addr)
        }
        None => Err(ProtectionError::MissingToken(addr, wallet, bitcoin_client)),
    }
}
