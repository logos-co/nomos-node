# BUILD IMAGE ---------------------------------------------------------

FROM rust:1.72.0-slim-bullseye AS builder

# Using backports for go 1.19
RUN echo 'deb http://deb.debian.org/debian bullseye-backports main' \
    >> /etc/apt/sources.list

# Dependecies for publishing documentation and building waku-bindings.
RUN apt-get update && apt-get install -yq \
    git clang \
    golang-src/bullseye-backports \
    golang-doc/bullseye-backports \
    golang/bullseye-backports

WORKDIR /nomos
COPY . . 

RUN cargo build --release -p nomos-node --no-default-features --features libp2p

# NODE IMAGE ----------------------------------------------------------

FROM bitnami/minideb:latest

LABEL maintainer="augustinas@status.im"
LABEL source="https://github.com/logos-co/nomos-node"
LABEL description="Nomos node image"

# nomos default ports
EXPOSE 3000 8080 9000 60000	

COPY --from=builder /nomos/target/release/nomos-node /usr/bin/nomos-node
COPY nodes/nomos-node/config.yaml /etc/nomos/config.yaml

ENTRYPOINT ["nomos-node"]
