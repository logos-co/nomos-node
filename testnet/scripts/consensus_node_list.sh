#!/bin/sh

set -e

# This node id will be used to generate consensus node list.
tmp_node_id=0
# OVERLAY_NODES might be set in compose.yml.
tmp_overlay_nodes=$OVERLAY_NODES

# All spawned nodes should be added to consensus configuration.
for i in $(seq 1 $LIBP2P_REPLICAS); do
	tmp_node_id=$((tmp_node_id + 1))
	node_key=$(/etc/nomos/scripts/node_key_from_id.sh "$LIBP2P_NODE_MASK" "$tmp_node_id")

	if [ -z "$tmp_overlay_nodes" ]; then
		tmp_overlay_nodes=$node_key
	else
		tmp_overlay_nodes="${tmp_overlay_nodes},${node_key}"
	fi
done

echo "${tmp_overlay_nodes}"
