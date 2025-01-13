FROM rust:1.82.0-slim-bookworm

LABEL maintainer="augustinas@status.im" \
      source="https://github.com/logos-co/nomos-node" \
      description="nomos-node ci build image"

# Dependecies for publishing documentation.
RUN apt-get update && apt-get install -yq \
    libssl-dev openssh-client git gcc g++ \ 
    clang pkg-config protobuf-compiler

RUN rustup component add rustfmt clippy

# Jenkins user needs a specific UID/GID to work.
RUN groupadd -g 1001 jenkins \
    && useradd -u 1001 -g jenkins jenkins

# Create necessary directories and assign correct permissions
RUN mkdir -p /home/jenkins/.local/share/cargo-risczero/toolchains \
    && chown -R jenkins:jenkins /home/jenkins/.local \
    && mkdir -p /home/jenkins/.cargo \
    && chown -R jenkins:jenkins /home/jenkins/.cargo

USER jenkins

RUN cargo install cargo-binstall --version 1.10.19
RUN cargo binstall -y cargo-risczero
RUN cargo risczero install
