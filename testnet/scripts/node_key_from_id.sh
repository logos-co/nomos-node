#!/bin/sh

set -e

if [ -z "$1" ] || [ -z "$2" ]; then
    echo "Usage: $0 <libp2p_node_mask> <node_id>"
    exit 1
fi

libp2p_node_mask=$1
node_id=$2

node_key_from_id() {
	echo "${libp2p_node_mask}" | sed "s/.\{${#node_id}\}$/${node_id}/"
}

node_key_from_id
