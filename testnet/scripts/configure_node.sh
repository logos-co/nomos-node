#!/bin/sh

set -e

node_key_from_id() {
	echo "${LIBP2P_NODE_MASK}" | sed "s/.\{${#NODE_ID}\}$/${NODE_ID}/"
}

# This node id will be used to generate consensus node list.
NODE_ID=0

# All nodes spawned nodes should added to consensus configuration.
for i in $(seq 1 $LIBP2P_REPLICAS); do
	NODE_ID=$((NODE_ID + 1))
	NODE_KEY=$(node_key_from_id)
	if [ -z "$OVERLAY_NODES" ]; then
		OVERLAY_NODES=$NODE_KEY
	else
		OVERLAY_NODES="${OVERLAY_NODES},${NODE_KEY}"
	fi
done

# Set env variables for nomos-node.
NET_NODE_KEY=$(./etc/nomos/scripts/register_node.sh)
CONSENSUS_PRIV_KEY=$NET_NODE_KEY

node_ids=$(etcdctl get "/node/" --prefix --keys-only)
for node_id in $node_ids; do
	node_key=$(etcdctl get "/config${node_id}/key" --print-value-only)
	node_ip=$(etcdctl get "/config${node_id}/ip" --print-value-only)
	node_multiaddr="/ip4/${node_ip}/tcp/3000"

	if [ -z "$OVERLAY_NODES" ]; then
		NET_INITIAL_PEERS=$node_multiaddr
	else
		NET_INITIAL_PEERS="${NET_INITIAL_PEERS},${node_multiaddr}"
	fi
done

export CONSENSUS_PRIV_KEY \
       OVERLAY_NODES \
       NET_NODE_KEY \
       NET_INITIAL_PEERS

echo "I am a container ${HOSTNAME} node ${NET_NODE_KEY}"

exec /usr/bin/nomos-node /etc/nomos/libp2p_config.yaml
