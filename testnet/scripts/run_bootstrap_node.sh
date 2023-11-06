#!/bin/sh

set -e

CONSENSUS_PRIV_KEY=$BOOTSTRAP_NODE_KEY
DA_VOTER_KEY=$BOOTSTRAP_NODE_KEY
NET_NODE_KEY=$BOOTSTRAP_NODE_KEY
OVERLAY_NODES=$(/etc/nomos/scripts/consensus_node_list.sh)

export CONSENSUS_PRIV_KEY \
       DA_VOTER_KEY \
       OVERLAY_NODES \
       NET_NODE_KEY

echo "I am a container ${HOSTNAME} node ${NET_NODE_KEY}"
echo "CONSENSUS_PRIV_KEY: ${CONSENSUS_PRIV_KEY}"
echo "DA_VOTER_KEY: ${DA_VOTER_KEY}"
echo "OVERLAY_NODES: ${OVERLAY_NODES}"

exec /usr/bin/nomos-node /etc/nomos/bootstrap_config.yaml
