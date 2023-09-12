#!/bin/sh

set -e

if [ ! -f /usr/bin/etcdctl ]; then
	./etc/nomos/etcd/install_etcd.sh
fi

echo "I am a bootstrap node"

NODE_KEY=1000000000000000000000000000000000000000000000000000000000000000

exec /usr/bin/nomos-node /etc/nomos/config.yaml --node-key=$NODE_KEY
