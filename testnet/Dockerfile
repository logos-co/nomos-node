# BUILD IMAGE ---------------------------------------------------------

FROM rust:1.72.0-slim-bullseye AS builder

# Using backports for go 1.19
RUN echo 'deb http://deb.debian.org/debian bullseye-backports main' \
    >> /etc/apt/sources.list

# Dependecies for publishing documentation.
RUN apt-get update && apt-get install -yq \
    git clang etcd-client 

WORKDIR /nomos
COPY . . 

RUN cargo build --release -p nomos-node
RUN cargo build --release -p mixnode

# NODE IMAGE ----------------------------------------------------------

FROM bitnami/minideb:latest

LABEL maintainer="augustinas@status.im" \
      source="https://github.com/logos-co/nomos-node" \
      description="Nomos testnet image"

# nomos default ports
EXPOSE 3000 8080 9000 60000	

COPY --from=builder /nomos/target/release/nomos-node /usr/bin/nomos-node
COPY --from=builder /nomos/target/release/mixnode /usr/bin/mixnode
COPY --from=builder /usr/bin/etcdctl /usr/bin/etcdctl
COPY nodes/nomos-node/config.yaml /etc/nomos/config.yaml

ENTRYPOINT ["/usr/bin/nomos-node"]
