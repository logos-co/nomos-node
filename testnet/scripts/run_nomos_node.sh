#!/bin/sh

set -e

# Set env variables for nomos-node.
NET_NODE_KEY=$(/etc/nomos/scripts/register_node.sh)
CONSENSUS_PRIV_KEY=$NET_NODE_KEY
DA_VOTER=$CONSENSUS_PRIV_KEY
OVERLAY_NODES=$(/etc/nomos/scripts/consensus_node_list.sh)

node_ids=$(etcdctl get "/node/" --prefix --keys-only)
for node_id in $node_ids; do
	node_key=$(etcdctl get "/config${node_id}/key" --print-value-only)
	node_ip=$(etcdctl get "/config${node_id}/ip" --print-value-only)
	node_multiaddr="/ip4/${node_ip}/tcp/3000"

	if [ -z "$NET_INITIAL_PEERS" ]; then
		NET_INITIAL_PEERS=$node_multiaddr
	else
		NET_INITIAL_PEERS="${NET_INITIAL_PEERS},${node_multiaddr}"
	fi
done

export CONSENSUS_PRIV_KEY \
       DA_VOTER \
       OVERLAY_NODES \
       NET_NODE_KEY \
       NET_INITIAL_PEERS

echo "I am a container ${HOSTNAME} node ${NET_NODE_KEY}"
echo "CONSENSUS_PRIV_KEY: ${CONSENSUS_PRIV_KEY}"
echo "DA_VOTER: ${DA_VOTER}"
echo "OVERLAY_NODES: ${OVERLAY_NODES}"
echo "NET_INITIAL_PEERS: ${NET_INITIAL_PEERS}"

exec /usr/bin/nomos-node /etc/nomos/libp2p_config.yaml --with-metrics
