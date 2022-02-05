pub mod database {
    include!(concat!(env!("OUT_DIR"), "/database.rs"));
}

pub mod broadcast {
    include!(concat!(env!("OUT_DIR"), "/broadcast.rs"));
}
