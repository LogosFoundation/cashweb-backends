<h1 align="center">
  Cash:web Relay
</h1>

<p align="center">
  An end-to-end encrypted message relay server.
</p>

<p align="center">
  <a href="https://github.com/cashweb/cash-relay/actions">
    <img alt="Build Status" src="https://github.com/cashweb/cash-relay/workflows/CI/badge.svg">
  </a>

  <a href="LICENSE">
    <img alt="License" src="https://img.shields.io/badge/license-MIT-blue.svg">
  </a>
</p>

This repository hosts a reference implementation of the Cash:web relay protocol. The goal is to provide a way for clients to push and pull end-to-end encrypted messages to and from inboxes which are cryptographically attached to Bitcoin Cash addresses. The messages are supplemented with a "stamp" (a Bitcoin Cash transaction) to prevent spam.

It is intended that clients communicate with the relay servers in conjunction with the [cash:web keyserver](https://github.com/cashweb/keyserver-rs) to provide a fully-fledged message relay system.

## Running a Server

### Setting up Bitcoin

Bitcoin must be running with [RPC](https://bitcoin.org/en/developer-reference#remote-procedure-calls-rpcs) enabled.

### Enabling Prometheus (optional)

One can optionally enable a [Prometheus](https://prometheus.io/) exporter, by compiling using the `--feature monitoring` feature flag.

### Build

Install [Rust](https://www.rust-lang.org/tools/install) then

```bash
sudo apt install -y clang pkg-config libssl-dev
cargo build --release
```

The executable will be located at `./target/release/cash-relay`.

### Configuration

Settings may be given by `JSON`, `TOML`, `YAML`, `HJSON` and `INI` files and, by default, are located at `~/.relay/config.*`. 

The `--config` argument will override the default location for the configuration file. Additional command-line arguments, given in the example below, will override the values given in the configuration file. Executing `cash-relay --help` will give an exhaustive list of options available.

All data sizes are given in bytes, prices in satoshis, and durations in milliseconds.

In TOML format, the default values are as follows:

```toml
# The bind address for the server
# --bind
bind = "127.0.0.1:8080"

# Bind address for the prometheus exporter
# --bind-prom
bind_prom = "127.0.0.1:9095"

# Bitcoin network
# --network
# NOTE: Allowed values are "mainnet", "testnet", and "regtest".
network = "regtest"

# Database path
# --db-path
db_path = "~/.relay/db"

[bitcoin_rpc]
# Bitcoin RPC address
# --rpc-addr
address = "http://127.0.0.1:18443"

# Bitcoin RPC username
# --rpc-username
username = "user"

# Bitcoin RPC password
# --rpc-password
password = "password"

[limits]
# Maximum message size (20 Mb)
message_size = 20_971_520

# Maximum profile size (512 Kb)
profile_size = 524_288

# Maximum payment size (3 Kb)
payment_size = 3_072

[payments]
# The payment timeout
timeout = 60_000

# The price of a POP token
token_fee = 100_000

# BIP70 payment memo
memo = "Thanks for your custom!"

# HMAC secret, given in hexidecimal
# --hmac-secret
# NOTE: This will not be given a default value in release compilation due to security considerations.
hmac_secret = "1234"

```

### Running

```bash
./target/release/cash-relay [OPTIONS]
```

Alternatively, copy `./static/` folder and `cash-relay` to a directory and run `cash-relay` from there.
