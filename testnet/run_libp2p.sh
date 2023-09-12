#!/bin/sh

set -e

if [ ! -f /usr/bin/etcdctl ]; then
	./etc/nomos/etcd/install_etcd.sh
fi


NET_NODE_KEY=$(./etc/nomos/etcd/register_etcd.sh)
CONSENSUS_NODE_KEY=$NET_NODE_KEY

sleep 2

node_ids=$(etcdctl get "/node/" --prefix --keys-only)
for node_id in $node_ids; do
	node_key=$(etcdctl get "/config$node_id/key" --print-value-only)
	node_ip=$(etcdctl get "/config$node_id/ip" --print-value-only)
	node_multiaddr="/ip4/$node_ip/tcp/3000"

	if [ -z "$OVERLAY_NODES" ]; then
		OVERLAY_NODES="$node_key"
		NET_INITIAL_PEERS=$node_multiaddr
	else
		OVERLAY_NODES="$OVERLAY_NODES,$node_key"
		NET_INITIAL_PEERS="$NET_INITIAL_PEERS,$node_multiaddr"
	fi
done

echo "I am a libp2p container $HOSTNAME node $NET_NODE_KEY"

exec /usr/bin/nomos-node /etc/nomos/config.yaml
