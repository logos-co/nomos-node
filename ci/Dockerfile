FROM rust:1.80.0-slim-bullseye

LABEL maintainer="augustinas@status.im" \
      source="https://github.com/logos-co/nomos-node" \
      description="nomos-node ci build image"

# Dependecies for publishing documentation.
RUN apt-get update && apt-get install -yq \
    libssl-dev openssh-client git python3-pip clang \
    pkg-config protobuf-compiler

RUN pip install ghp-import
RUN rustup component add rustfmt clippy

RUN cargo install cargo-binstall
RUN cargo binstall -y cargo-risczero
RUN cargo risczero install

# Jenkins user needs a specific UID/GID to work.
RUN groupadd -g 1001 jenkins \
 && useradd -u 1001 -g jenkins jenkins
