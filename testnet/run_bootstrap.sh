#!/bin/sh

set -e

echo "I am a bootstrap container $HOSTNAME node $NET_NODE_KEY"

exec /usr/bin/nomos-node /etc/nomos/config.yaml
