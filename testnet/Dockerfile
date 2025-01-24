# BUILD IMAGE ---------------------------------------------------------

FROM rust:1.82.0-slim-bookworm AS builder

WORKDIR /nomos
COPY . . 

# Install dependencies needed for building RocksDB and etcd.
RUN apt-get update && apt-get install -yq \
    git gcc g++ clang libssl-dev pkg-config

RUN cargo install cargo-binstall --version 1.10.19
RUN cargo binstall -y cargo-risczero
RUN cargo risczero install

RUN cargo build --release

# NODE IMAGE ----------------------------------------------------------

FROM bitnami/minideb:bookworm

LABEL maintainer="augustinas@status.im" \
      source="https://github.com/logos-co/nomos-node" \
      description="Nomos testnet image"

# nomos default ports
EXPOSE 3000 8080 9000 60000

RUN apt-get update && apt-get install -y libssl3

COPY --from=builder /nomos/target/release/nomos-node /usr/bin/nomos-node
COPY --from=builder /nomos/target/release/nomos-executor /usr/bin/nomos-executor
COPY --from=builder /nomos/target/release/nomos-cli /usr/bin/nomos-cli
COPY --from=builder /nomos/target/release/cfgsync-server /usr/bin/cfgsync-server
COPY --from=builder /nomos/target/release/cfgsync-client /usr/bin/cfgsync-client

ENTRYPOINT ["/usr/bin/nomos-node"]
