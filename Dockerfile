FROM rust:latest as cargo-build

# Install dependencies
RUN apt-get update && apt-get install -y \
    clang-3.9 \
    libssl-dev

WORKDIR /usr/src/cash-relay

# Dummy compile
COPY Cargo.toml Cargo.lock ./
RUN mkdir src/
RUN echo "fn main() {println!(\"failed to replace dummy build\")}" > src/main.rs
RUN cargo build --release
RUN rm -f target/release/deps/cash_relay*

# Compile
COPY . .
RUN cargo build --release

FROM ubuntu:latest

RUN apt-get update && apt-get install -y libssl-dev

COPY --from=cargo-build /usr/src/cash-relay/target/release/cash-relay /usr/local/bin/cash-relay

CMD ["cash-relay"]