FROM rust:1.74.0-slim-bullseye

LABEL maintainer="augustinas@status.im" \
      source="https://github.com/logos-co/nomos-node" \
      description="nomos-node ci build image"

# Using backports for go 1.19
RUN echo 'deb http://deb.debian.org/debian bullseye-backports main' \
    >> /etc/apt/sources.list

# Dependecies for publishing documentation.
RUN apt-get update && apt-get install -yq \
    libssl-dev openssh-client git python3-pip clang \
    pkg-config

RUN pip install ghp-import
RUN rustup component add rustfmt clippy

# Jenkins user needs a specific UID/GID to work.
RUN groupadd -g 1001 jenkins \
 && useradd -u 1001 -g jenkins jenkins
