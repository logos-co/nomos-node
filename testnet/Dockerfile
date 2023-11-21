# BUILD IMAGE ---------------------------------------------------------

FROM rust:1.74.0-slim-bullseye AS builder

# Using backports for go 1.19
RUN echo 'deb http://deb.debian.org/debian bullseye-backports main' \
    >> /etc/apt/sources.list

# Dependecies for publishing documentation.
RUN apt-get update && apt-get install -yq \
    git clang etcd-client libssl-dev pkg-config

WORKDIR /nomos
COPY . . 

RUN cargo build --release --all

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
RUN install_packages python3 python3-etcd3

ENTRYPOINT ["/usr/bin/nomos-node"]
