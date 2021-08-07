FROM rust:latest as cargo-build

# Install dependencies
RUN apt-get update && apt-get install -y \
    clang-3.9 \
    libssl-dev \
    libzmq3-dev

WORKDIR /app/

# Dummy compile
# COPY Cargo.toml Cargo.lock ./
# RUN ls -la
# RUN cargo build --release --all-features
# RUN rm -f target/release/deps/keyserver*

# Compile
COPY . .
RUN cargo build --release --all-features

FROM ubuntu:latest

RUN apt-get update && apt-get install -y libssl-dev libzmq3-dev

COPY --from=cargo-build /app/target/release/keyserver /usr/local/bin/keyserver
COPY --from=cargo-build /app/target/release/cash-relay /usr/local/bin/cash-relay

ENTRYPOINT ["keyserver"]
