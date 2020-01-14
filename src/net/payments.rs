use std::{
    pin::Pin,
    str,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use actix_service::{Service, Transform};
use actix_web::{
    dev::{Body, ServiceRequest, ServiceResponse},
    http::{
        header::{HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, PRAGMA},
        Method,
    },
    web, Error, HttpRequest, HttpResponse, ResponseError,
};
use bitcoin::{util::psbt::serialize::Deserialize, Transaction};
use bitcoincash_addr::{Address, HashType};
use bytes::BytesMut;
use futures::{
    future::{err, ok, ready, Ready},
    prelude::*,
    task::{Context, Poll},
};
use json_rpc::clients::http::HttpConnector;
use prost::Message;

use crate::{bitcoin::*, models::bip70::*, SETTINGS};

use super::errors::*;

use crate::crypto::token::*;

const PAYMENT_PATH: &str = "/payments";
pub const VALID_DURATION: u64 = 30;

/// Payment handler
pub async fn payment_handler(
    req: HttpRequest,
    mut payload: web::Payload,
    data: web::Data<(BitcoinClient<HttpConnector>, WalletState)>,
) -> Result<HttpResponse, ServerError> {
    // Check headers
    let headers = req.headers();
    if headers.get(CONTENT_TYPE)
        != Some(&HeaderValue::from_str("application/bitcoincash-payment").unwrap())
    {
        return Err(PaymentError::Accept.into());
    }
    if headers.get(ACCEPT)
        != Some(&HeaderValue::from_str("application/bitcoincash-paymentack").unwrap())
    {
        return Err(PaymentError::Content.into());
    }

    // Read and parse payment proto
    let mut payment_raw = BytesMut::new();
    while let Some(item) = payload.next().await {
        payment_raw.extend_from_slice(&item.map_err(ServerError::Buffer)?);
    }
    let payment = Payment::decode(&payment_raw[..]).map_err(|_| PaymentError::Decode)?;

    // Parse tx
    let tx_raw = match payment.transactions.get(0) {
        Some(some) => some,
        None => return Err(PaymentError::NoTx.into()),
    };

    // Assume first tx
    let tx = Transaction::deserialize(tx_raw).map_err(PaymentError::from)?;

    // Check outputs
    let wallet_data = &data.1;
    if !wallet_data.check_outputs(tx) {
        return Err(ServerError::Payment(PaymentError::InvalidOutputs));
    }

    // Send tx
    let bitcoin_client = &data.0;
    bitcoin_client
        .send_tx(tx_raw)
        .await
        .map_err(PaymentError::TxReject)?;

    // Create payment ack
    let memo = Some("Thanks for your custom!".to_string());
    let payment_ack = PaymentAck { payment, memo };

    // Encode payment ack
    let mut raw_ack = Vec::with_capacity(payment_ack.encoded_len());
    payment_ack.encode(&mut raw_ack).unwrap();

    // Get merchant data
    let merchant_data = payment_ack
        .payment
        .merchant_data
        .ok_or(PaymentError::NoMerchantDat)?;

    // Generate token
    let url_safe_config = base64::Config::new(base64::CharacterSet::UrlSafe, false);
    let token = base64::encode_config(
        &generate_token(&merchant_data, SETTINGS.secret.as_bytes()),
        url_safe_config,
    );

    // Generate response
    Ok(HttpResponse::Accepted()
        .header(AUTHORIZATION, format!("POP {}", token))
        .header(PRAGMA, "no-cache")
        .body(raw_ack))
}

/*
Payment middleware
*/
pub struct CheckPayment {
    client: BitcoinClient<HttpConnector>,
    wallet_state: WalletState,
    protected_method: Method,
}

impl CheckPayment {
    pub fn new(
        client: BitcoinClient<HttpConnector>,
        wallet_state: WalletState,
        protected_method: Method,
    ) -> Self {
        CheckPayment {
            client,
            wallet_state,
            protected_method,
        }
    }
}

impl<S> Transform<S> for CheckPayment
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<Body>, Error = Error>,
    S::Future: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<Body>;
    type Error = Error;
    type InitError = ();
    type Transform = CheckPaymentMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(CheckPaymentMiddleware {
            service,
            client: self.client.clone(),
            wallet_state: self.wallet_state.clone(),
            protected_method: self.protected_method.clone(),
        }))
    }
}
pub struct CheckPaymentMiddleware<S> {
    service: S,
    client: BitcoinClient<HttpConnector>,
    wallet_state: WalletState,
    protected_method: Method,
}

impl<S> Service for CheckPaymentMiddleware<S>
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<Body>, Error = Error>,
    S::Response: 'static,
    S::Future: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<Body>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: ServiceRequest) -> Self::Future {
        // Only pay for put
        if req.method() != self.protected_method {
            return Box::pin(self.service.call(req));
        }

        // Get request data
        let conn_info = req.connection_info().clone();
        let scheme = conn_info.scheme().to_owned();
        let host = conn_info.host().to_owned();

        // Decode put address
        let put_addr_str = req
            .match_info()
            .get("addr")
            .expect("wrapped route with no {addr}"); // TODO: This is safe when wrapping a {addr}
        let put_addr = match Address::decode(put_addr_str) {
            Ok(ok) => ok,
            Err((cash_err, base58_err)) => {
                return Box::pin(err(ServerError::Address(cash_err, base58_err).into()))
            }
        };

        // Grab token query from authorization header then query string
        let token_str: String = match req.headers().get(AUTHORIZATION) {
            Some(some) => match some.to_str() {
                Ok(auth_str) => {
                    if auth_str.len() >= 4 && &auth_str[0..4] == "POP " {
                        auth_str[4..].to_string()
                    } else {
                        return Box::pin(err(
                            ServerError::Payment(PaymentError::InvalidAuth).into()
                        ));
                    }
                }
                Err(_) => {
                    return Box::pin(err(ServerError::Payment(PaymentError::InvalidAuth).into()))
                }
            },
            None => {
                // If no token found then generate invoice

                // Valid interval
                let current_time = SystemTime::now();
                let expiry_time = current_time + Duration::from_secs(VALID_DURATION);

                // Get new addr and add to wallet
                let wallet_state_inner = self.wallet_state.clone();
                let client_inner = self.client.clone();
                let new_addr = async move {
                    let addr_opt = client_inner.get_new_addr().await;
                    match addr_opt {
                        Ok(addr_str) => {
                            let addr =
                                Address::decode(&addr_str).map_err(|(cash_err, base58_err)| {
                                    ServerError::Address(cash_err, base58_err)
                                })?;
                            let network: Network = addr.network.clone().into();
                            if network != SETTINGS.network || addr.hash_type != HashType::Key {
                                return Err(
                                    ServerError::Payment(PaymentError::MismatchedNetwork).into()
                                );
                                // TODO: Finer grained error here
                            }
                            let addr_raw = addr.into_body();
                            wallet_state_inner.add(addr_raw.clone());
                            Ok(addr_raw)
                        }
                        Err(_e) => Err(ServerError::Payment(PaymentError::AddrFetchFailed).into()),
                    }
                };

                // Generate merchant data
                let base_url = format!("{}://{}", scheme, host);

                let response = new_addr.and_then(move |addr_raw| {
                    // Generate outputs
                    let outputs = generate_outputs(&addr_raw);

                    // Collect payment details
                    let payment_url = Some(format!("{}{}", base_url, PAYMENT_PATH));
                    let payment_details = PaymentDetails {
                        network: Some(SETTINGS.network.to_string()),
                        time: current_time.duration_since(UNIX_EPOCH).unwrap().as_secs(),
                        expires: Some(expiry_time.duration_since(UNIX_EPOCH).unwrap().as_secs()),
                        memo: None,
                        merchant_data: Some(put_addr.into_body()),
                        outputs,
                        payment_url,
                    };
                    let mut serialized_payment_details =
                        Vec::with_capacity(payment_details.encoded_len());
                    payment_details
                        .encode(&mut serialized_payment_details)
                        .unwrap();

                    // Generate payment invoice
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

                    HttpResponse::PaymentRequired()
                        .content_type("application/bitcoincash-paymentrequest")
                        .header("Content-Transfer-Encoding", "binary")
                        .body(payment_invoice_raw)
                });

                // Respond
                return Box::pin(response.map_ok(move |resp| req.into_response(resp)));
            }
        };

        // Decode token
        let url_safe_config = base64::Config::new(base64::CharacterSet::UrlSafe, false);
        let token = match base64::decode_config(&token_str, url_safe_config) {
            Ok(some) => some,
            Err(_) => {
                return Box::pin(ok(req.into_response(
                    ServerError::Payment(PaymentError::InvalidAuth).error_response(),
                )))
            }
        };

        // Validate
        if !validate_token(put_addr.as_body(), SETTINGS.secret.as_bytes(), &token) {
            Box::pin(ok(req.into_response(
                ServerError::Payment(PaymentError::InvalidAuth).error_response(),
            )))
        } else {
            Box::pin(self.service.call(req))
        }
    }
}
