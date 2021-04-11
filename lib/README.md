<h1 align="center">
  Cash:Web Utility Library
</h1>

<p align="center">
  <a href="https://github.com/cashweb/cashweb-rs/actions">
    <img alt="Build Status" src="https://github.com/cashweb/cashweb-rs/workflows/CI/badge.svg">
  </a>

  <a href="LICENSE">
    <img alt="License" src="https://img.shields.io/badge/license-MIT-blue.svg">
  </a>

  <a href="https://crates.io/crates/cashweb">
    <img alt="Cargo" src="https://img.shields.io/crates/v/cashweb.svg">
  </a>

  <a href="https://docs.rs/cashweb">
    <img alt="Cargo" src="https://docs.rs/cashweb/badge.svg">
  </a>
</p>

`cashweb` is a collection of useful components, designed for quick integration of the following protocols:
* [Proof-of-Payment Authorization Framework](https://github.com/cashweb/specifications/blob/master/proof-of-payment-token/specification.mediawiki)
* [Authorization Wrapper Protocol](https://github.com/cashweb/specifications/blob/master/authorization-wrapper-protocol/specification.mediawiki)
* [Keyserver Protocol](https://github.com/cashweb/specifications/blob/master/keyserver-protocol/specification.mediawiki)
* [Relay Server Protocol](https://github.com/cashweb/specifications/blob/master/relay-server-protocol/specification.mediawiki)

## Usage

Add this to your `cargo.toml`

```toml
cashweb-rs = "0.1.0-alpha.7"
```

The current version requires Rust 1.39 or later.
