pub mod address_metadata {
    pub use cashweb::protobuf::address_metadata::*;
}

pub mod bip70 {
    pub use cashweb::protobuf::bip70::*;
}

pub mod messaging {
    include!(concat!(env!("OUT_DIR"), "/messaging.rs"));
}

pub mod filters {
    include!(concat!(env!("OUT_DIR"), "/filters.rs"));
}
