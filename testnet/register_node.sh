#!/bin/sh

# NODE_MASK is set via compose.yml file.

node_key_from_id() {
	LENGTH=$(echo -n $NODE_ID | wc -c)
	echo $(echo $NODE_MASK | sed "s/.\{${LENGTH}\}$/${NODE_ID}/")
}

END=$DOCKER_REPLICAS
NODE_ID=1
NODE_IP=$(hostname -i)
NODE_KEY=$(node_key_from_id)

register_node() {
	## Conditional transaction to set node config key if it doesn't exist.
	## Newlines in EOF block are important, more info here:
	## https://github.com/etcd-io/etcd/tree/main/etcdctl#examples-3
	etcdctl txn <<EOF
mod("/node/${NODE_ID}") = "0"

put /node/${NODE_ID} "${NODE_ID}"
put /config/node/${NODE_ID}/key "${NODE_KEY}"
put /config/node/${NODE_ID}/ip "${NODE_IP}"


EOF
}

while [ $NODE_ID -le $END ]; do
	result=$(register_node)

	# Check if the key was registered or already exists
	if [ "$result" != "FAILURE" ]; then
		break
	else
		NODE_ID=$(($NODE_ID + 1))
		NODE_KEY=$(node_key_from_id)
	fi
done

if [ $NODE_ID -gt $END ]; then
	echo "Reached the limit without registering a NODE_ID."
	return 1
fi

echo $NODE_KEY
