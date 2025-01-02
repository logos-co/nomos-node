#!/bin/sh

set -e

export RUST_BACKTRACE=1

exec /usr/bin/cfgsync-server /etc/nomos/cfgsync.yaml
