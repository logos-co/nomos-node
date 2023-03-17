FROM rust:1.67.0-slim-bullseye

LABEL maintainer="augustinas@status.im"
LABEL source="https://github.com/logos-co/nomos-research"
LABEL description="nomos-research ci build image"

# Using backports for go 1.19
RUN echo 'deb http://deb.debian.org/debian bullseye-backports main' \
    >> /etc/apt/sources.list

# Dependecies for publishing documentation and building waku-bindings.
RUN apt-get update && apt-get install -yq \
    openssh-client git python3-pip clang \
    golang-src/bullseye-backports \
    golang-doc/bullseye-backports \
    golang/bullseye-backports

RUN pip install ghp-import
RUN rustup component add rustfmt clippy

# Jenkins user needs a specific UID/GID to work.
RUN groupadd -g 1001 jenkins \
 && useradd -u 1001 -g jenkins jenkins
