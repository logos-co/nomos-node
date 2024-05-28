# BUILD IMAGE ---------------------------------------------------------

FROM rust:1.78.0-slim-bullseye AS builder

WORKDIR /nomos
COPY . . 

# Install dependencies needed for building RocksDB.
RUN apt-get update && apt-get install -yq \
    git clang libssl-dev pkg-config

RUN cargo build --release -p nomos-node

# NODE IMAGE ----------------------------------------------------------

FROM bitnami/minideb:latest

LABEL maintainer="augustinas@status.im" \
      source="https://github.com/logos-co/nomos-node" \
      description="Nomos node image"

# nomos default ports
EXPOSE 3000 8080 9000 60000	

COPY --from=builder /nomos/target/release/nomos-node /usr/bin/nomos-node
COPY nodes/nomos-node/config.yaml /etc/nomos/config.yaml

ENTRYPOINT ["nomos-node"]
