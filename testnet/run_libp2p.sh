#!/bin/sh

set -e

./etc/nomos/install_etcd.sh

NODE_ID=$(./etc/nomos/register_etcd.sh)
NODE_KEY=$(printf '%064s' $(printf '%x' $NODE_ID) | tr ' ' '0')

echo "I am a libp2p containet $HOSTNAME node $NODE_KEY"

exec /usr/bin/nomos-node /etc/nomos/config.yaml --node-key=$NODE_KEY
