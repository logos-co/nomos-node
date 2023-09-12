#!/bin/sh

END=10
NODE_ID=1
NODE_IP=$(hostname -i)

register_node() {
	## Conditional transaction to set node config key if it doesn't exist.
	## White spaces in EOF block is important, more info here:
	## https://github.com/etcd-io/etcd/tree/main/etcdctl#examples-3
	etcdctl --endpoints=etcd:2379 txn <<EOF
mod("config/$NODE_ID") = "0"

put config/$NODE_ID "$NODE_ID"
put config/$NODE_ID/ip "$NODE_IP"


EOF
}

while [ $NODE_ID -le $END ]; do
	result=$(register_node)

    # Check if the key was registered or already exists
    if [ "$result" != "FAILURE" ]; then
        break
    else
        NODE_ID=$((NODE_ID + 1))
    fi
done

if [ $NODE_ID -gt $END ]; then
    echo "Reached the limit without registering a NODE_ID."
fi

echo $NODE_ID
