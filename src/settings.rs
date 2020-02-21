use std::net::SocketAddr;

use clap::App;
use config::{Config, ConfigError, File};
use serde::Deserialize;

// use crate::bitcoin::Network;

const FOLDER_DIR: &str = ".relay";
const DEFAULT_BIND: &str = "127.0.0.1:8080";
const DEFAULT_RPC_ADDR: &str = "127.0.0.1:18443";
const DEFAULT_RPC_USER: &str = "user";
const DEFAULT_RPC_PASSWORD: &str = "password";
const DEFAULT_NETWORK: &str = "regnet";
const DEFAULT_MESSAGE_LIMIT: usize = 1024 * 1024 * 20; // 20MB
const DEFAULT_FILTER_LIMIT: usize = 1024 * 1024; // 1MB

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub bind: SocketAddr,
    pub rpc_addr: String,
    pub rpc_username: String,
    pub rpc_password: String,
    pub secret: String,
    pub db_path: String,
    // pub network: Network,
    pub limits: Limits,
}

#[derive(Debug, Deserialize)]
pub struct Limits {
    pub message_size: u64,
    pub filter_size: u64,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let mut s = Config::new();

        // Set defaults
        let yaml = load_yaml!("cli.yml");
        let matches = App::from_yaml(yaml)
            .about(crate_description!())
            .author(crate_authors!("\n"))
            .version(crate_version!())
            .get_matches();
        let home_dir = match dirs::home_dir() {
            Some(some) => some,
            None => return Err(ConfigError::Message("no home directory".to_string())),
        };
        s.set_default("bind", DEFAULT_BIND)?;
        s.set_default("rpc_addr", DEFAULT_RPC_ADDR)?;
        s.set_default("rpc_username", DEFAULT_RPC_USER)?;
        s.set_default("rpc_password", DEFAULT_RPC_PASSWORD)?;
        s.set_default("secret", "secret")?; // TODO: Remove
        let mut default_db = home_dir.clone();
        default_db.push(format!("{}/db", FOLDER_DIR));
        s.set_default("db_path", default_db.to_str())?;
        s.set_default("network", DEFAULT_NETWORK)?;
        s.set_default("limits.message_size", DEFAULT_MESSAGE_LIMIT as i64)?;
        s.set_default("limits.filter_size", DEFAULT_FILTER_LIMIT as i64)?;

        // Load config from file
        let mut default_config = home_dir;
        default_config.push(format!("{}/config", FOLDER_DIR));
        let default_config_str = default_config.to_str().unwrap();
        let config_path = matches.value_of("config").unwrap_or(default_config_str);
        s.merge(File::with_name(config_path).required(false))?;

        // Set bind address from cmd line
        if let Some(bind) = matches.value_of("bind") {
            s.set("bind", bind)?;
        }

        // Set node IP from cmd line
        if let Some(node_ip) = matches.value_of("rpc-addr") {
            s.set("rpc_addr", node_ip)?;
        }

        // Set rpc port from cmd line
        if let Ok(rpc_port) = value_t!(matches, "rpc-port", i64) {
            s.set("rpc_port", rpc_port)?;
        }

        // Set rpc username from cmd line
        if let Some(rpc_username) = matches.value_of("rpc-username") {
            s.set("rpc_username", rpc_username)?;
        }

        // Set rpc password from cmd line
        if let Some(rpc_password) = matches.value_of("rpc-password") {
            s.set("rpc_password", rpc_password)?;
        }

        // Set zmq port from cmd line
        if let Ok(node_zmq_port) = value_t!(matches, "zmq-port", i64) {
            s.set("zmq_port", node_zmq_port)?;
        }

        // Set secret from cmd line
        if let Some(secret) = matches.value_of("secret") {
            s.set("secret", secret)?;
        }

        // Set db from cmd line
        if let Some(db_path) = matches.value_of("db-path") {
            s.set("db_path", db_path)?;
        }

        // Set the bitcoin network
        if let Some(network) = matches.value_of("network") {
            s.set("network", network)?;
        }

        s.try_into()
    }
}
