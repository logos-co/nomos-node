#!/bin/sh

echo "I am a libp2p node $HOSTNAME"

# Using container name as the end of node key.
# TODO: For persistence between runs, node key needs to be the same.
NODE_KEY=0000000000000000000000000000000000000000000000000000$HOSTNAME

exec /usr/bin/nomos-node /etc/nomos/config.yaml --node-key=$NODE_KEY
