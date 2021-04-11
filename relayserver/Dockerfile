FROM rust:latest as cargo-build

# Install dependencies
RUN apt-get update && apt-get install -y \
    clang-3.9 \
    libssl-dev

WORKDIR /app

# Dummy compile
COPY Cargo.toml Cargo.lock ./
RUN mkdir src/
RUN echo "fn main() {println!(\"failed to replace dummy build\")}" > src/main.rs
RUN cargo build --release --all-features
RUN rm -f target/release/deps/cash_relay*

# Compile
COPY . .
RUN cargo build --release --all-features

FROM ubuntu:latest

RUN apt-get update && apt-get install -y libssl-dev

COPY --from=cargo-build /app/target/release/cash-relay /usr/local/bin/cash-relay

ENTRYPOINT ["cash-relay"]
