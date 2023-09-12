#!/bin/sh

set -e

if [ ! -f /usr/bin/etcdctl ]; then
	./etc/nomos/etcd/install_etcd.sh
fi


NODE_ID=$(./etc/nomos/etcd/register_etcd.sh)
NODE_KEY=$(printf '%064s' $(printf '%x' $NODE_ID) | tr ' ' '0')

NET_NODE_KEY=$NODE_KEY
CONSENSUS_NODE_KEY=$NODE_KEY

echo "I am a libp2p container $HOSTNAME node $NODE_KEY"

exec /usr/bin/nomos-node /etc/nomos/config.yaml
