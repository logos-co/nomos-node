# BUILD IMAGE ---------------------------------------------------------

FROM rust:1.67.0-slim-bullseye AS builder

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

RUN cargo build --release -p mockpool-node

# NODE IMAGE ----------------------------------------------------------

FROM bitnami/minideb:latest

LABEL maintainer="augustinas@status.im"
LABEL source="https://github.com/logos-co/nomos-research"
LABEL description="Nomos node image"

# nomos default ports
EXPOSE 3000 8080 9000 60000	

COPY --from=builder /nomos/target/release/mockpool-node /usr/bin/nomos-node
COPY config.yml.example /etc/nomos/config.yml

ENTRYPOINT ["nomos-node"]
