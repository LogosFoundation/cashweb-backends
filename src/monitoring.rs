use lazy_static::lazy_static;
use prometheus::{CounterVec, HistogramVec};
use warp::filters::log::Info;

use prometheus_static_metric::make_static_metric;

use crate::*;

make_static_metric! {
    pub label_enum Method {
        delete,
        get,
        post,
        put,
        other
    }

    pub label_enum Route {
        index,
        messages,
        payloads,
        payments,
        profiles,
        ws,
        other
    }

    pub struct RequestTotalCounter: Counter {
        "method" => Method,
        "route" => Route
    }

    pub struct RequestDurationHistogram: Histogram {
        "method" => Method,
        "route" => Route
    }
}

impl From<&http::Method> for Method {
    fn from(method: &http::Method) -> Method {
        match method {
            &http::Method::GET => Method::get,
            &http::Method::POST => Method::post,
            &http::Method::PUT => Method::put,
            &http::Method::DELETE => Method::delete,
            _ => Method::other,
        }
    }
}

impl From<&str> for Route {
    fn from(path: &str) -> Self {
        let path_len = path.len();
        if path_len >= MESSAGES_PATH.len() && &path[1..MESSAGES_PATH.len() + 1] == MESSAGES_PATH {
            Route::messages
        } else if path_len >= PROFILE_PATH.len() && &path[1..PROFILE_PATH.len() + 1] == PROFILE_PATH
        {
            Route::profiles
        } else if path_len >= PAYMENTS_PATH.len()
            && &path[1..PAYMENTS_PATH.len() + 1] == PAYMENTS_PATH
        {
            Route::payloads
        } else if path_len >= PAYLOADS_PATH.len()
            && &path[1..PAYLOADS_PATH.len() + 1] == PAYLOADS_PATH
        {
            Route::payloads
        } else if path_len == WS_PATH.len() && &path[1..WS_PATH.len() + 1] == WS_PATH {
            Route::ws
        } else if path == "/" {
            Route::index
        } else {
            Route::other
        }
    }
}

// Prometheus metrics
lazy_static! {
    // Request counter
    pub static ref HTTP_TOTAL_VEC: CounterVec = prometheus::register_counter_vec!(
        "http_requests_total",
        "Total number of HTTP requests.",
        &["method", "route"]
    )
    .unwrap();
    pub static ref HTTP_TOTAL: RequestTotalCounter = RequestTotalCounter::from(&HTTP_TOTAL_VEC);

    // Request duration
    pub static ref HTTP_ELAPSED_VEC: HistogramVec = prometheus::register_histogram_vec!(
        "http_request_duration_seconds",
        "Histogram of elapsed times.",
        &["method", "route"]
    )
    .unwrap();
    pub static ref HTTP_ELAPSED: RequestDurationHistogram = RequestDurationHistogram::from(&HTTP_ELAPSED_VEC);
}

pub fn measure(info: Info) {
    let method: Method = info.method().into();
    let route: Route = info.path().into();

    // Increment request counter
    HTTP_TOTAL.get(method).get(route).inc();

    // Observe duration
    let duration_secs = info.elapsed().as_secs_f64();
    HTTP_ELAPSED.get(method).get(route).observe(duration_secs);

    println!("observed");
}

pub fn export() -> Vec<u8> {
    let metric_families = prometheus::gather();

    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    buffer
}