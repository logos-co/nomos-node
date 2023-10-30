#!/bin/sh

set -e

node_key_from_id() {
	echo "${LIBP2P_NODE_MASK}" | sed "s/.\{${#NODE_ID}\}$/${NODE_ID}/"
}

CONSENSUS_PRIV_KEY=$BOOTSTRAP_NODE_KEY
NET_NODE_KEY=$BOOTSTRAP_NODE_KEY

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

export CONSENSUS_PRIV_KEY \
       OVERLAY_NODES \
	   NET_NODE_KEY

echo "I am a container ${HOSTNAME} node ${BOOTSTRAP_NODE_KEY}"

exec /usr/bin/nomos-node /etc/nomos/bootstrap_config.yaml
