#!/bin/sh

set -e

# Set env variables for nomos-node.
NET_NODE_KEY=$(./etc/nomos/register_node.sh)
CONSENSUS_PRIV_KEY=$NET_NODE_KEY

node_ids=$(etcdctl get "/node/" --prefix --keys-only)
for node_id in $node_ids; do
	node_key=$(etcdctl get "/config${node_id}/key" --print-value-only)
	node_ip=$(etcdctl get "/config${node_id}/ip" --print-value-only)
	node_multiaddr="/ip4/${node_ip}/tcp/3000"

	if [ -z "$OVERLAY_NODES" ]; then
		OVERLAY_NODES=$node_key
		NET_INITIAL_PEERS=$node_multiaddr
	else
		OVERLAY_NODES="${OVERLAY_NODES},${node_key}"
		NET_INITIAL_PEERS="${NET_INITIAL_PEERS},${node_multiaddr}"
	fi
done

echo "I am a container ${HOSTNAME} node ${NET_NODE_KEY}"

exec /usr/bin/nomos-node /etc/nomos/config.yaml
