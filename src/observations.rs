use lazy_static::lazy_static;
use prometheus::{CounterVec, HistogramVec, Opts, Registry};
use warp::filters::log::{Info, Log};

use prometheus_static_metric::make_static_metric;

make_static_metric! {
    pub label_enum Method {
        post,
        get,
        put,
        delete,
        other
    }

    pub label_enum Route {
        messages,
        profiles,
        ws,
        index,
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
        if path_len >= 9 && &path[1..9] == "messages" {
            Route::messages
        } else if path_len >= 9 && &path[1..9] == "profiles" {
            Route::profiles
        } else if path_len == 3 && &path[1..3] == "ws" {
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
        "Total number of HTTP requests.",
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
