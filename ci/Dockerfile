FROM rust:alpine

LABEL maintainer="augustinas@status.im"
LABEL source="https://github.com/logos-co/nomos-research"
LABEL description="nomos-research ci build image"

# Dependecies for publishing documentation.
RUN apk add --no-cache \
    openssh-client-common \
    openssh-client-default \
    git py3-pip musl-dev go

# Configure Go
ENV GOROOT /usr/lib/go
ENV GOPATH /go
ENV PATH /go/bin:$PATH

RUN mkdir -p ${GOPATH}/src ${GOPATH}/bin

RUN pip install ghp-import

# Jenkins user needs a specific UID/GID to work.
RUN addgroup -g 1001 jenkins \
 && adduser -D -u 1001 -G jenkins jenkins
