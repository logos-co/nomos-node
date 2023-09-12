#!/bin/sh

set -e

ETCD_VER=v3.4.15
ARCH=linux-amd64
DOWNLOAD_URL="https://github.com/etcd-io/etcd/releases/download/${ETCD_VER}/etcd-${ETCD_VER}-${ARCH}.tar.gz"
ARCHIVE="etcd-${ETCD_VER}-${ARCH}.tar.gz"

install_packages curl ca-certificates

# Download and extract etcdctl
curl -L $DOWNLOAD_URL -o $ARCHIVE
tar xzf $ARCHIVE etcd-${ETCD_VER}-${ARCH}/etcdctl --strip-components=1

# Move to /usr/bin and cleanup
mv etcdctl /usr/bin/
rm -f $ARCHIVE
