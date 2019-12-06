pub mod db;

pub mod models {
    pub mod pop {
        include!(concat!(env!("OUT_DIR"), "/models.rs"));
    }
    pub mod messaging {
        include!(concat!(env!("OUT_DIR"), "/messaging.rs"));
    }
}

fn main() {
    println!("Hello, world!");
}
