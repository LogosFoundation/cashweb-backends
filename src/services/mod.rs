use std::task::{Context, Poll};

use futures_util::future;
use hyper::service::Service;
use hyper::{Body, Request, Response, Server};

const ROOT: &'static str = "/";
