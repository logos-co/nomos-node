# BUILD IMAGE ---------------------------------------------------------

FROM rust:1.79.0-slim-bullseye AS builder

WORKDIR /nomos
COPY . . 

# Install dependencies needed for building RocksDB and etcd.
RUN apt-get update && apt-get install -yq \
    git clang etcd-client libssl-dev pkg-config

RUN cargo build --release --all --features metrics

# NODE IMAGE ----------------------------------------------------------

FROM bitnami/minideb:latest

LABEL maintainer="augustinas@status.im" \
      source="https://github.com/logos-co/nomos-node" \
      description="Nomos testnet image"

# nomos default ports
EXPOSE 3000 8080 9000 60000	

COPY --from=builder /nomos/target/release/nomos-node /usr/bin/nomos-node
COPY --from=builder /nomos/target/release/nomos-cli /usr/bin/nomos-cli
COPY --from=builder /usr/bin/etcdctl /usr/bin/etcdctl
COPY nodes/nomos-node/config.yaml /etc/nomos/config.yaml

ENTRYPOINT ["/usr/bin/nomos-node"]
