#!/bin/sh

set -e

./etc/nomos/install_etcd.sh

echo "I am a bootstrap node"

NODE_KEY=0000000000000000000000000000000000000000000000000000000000000001

exec /usr/bin/nomos-node /etc/nomos/config.yaml --node-key=$NODE_KEY
