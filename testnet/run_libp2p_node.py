#!/usr/bin/env python3

import os
import time
import subprocess
import etcd3

def node_key_from_id(node_id, node_mask):
    return node_mask[:-len(str(node_id))] + str(node_id)

def get_etcd_client():
    etcd_endpoints = os.environ.get('ETCDCTL_ENDPOINTS', 'etcd:2379').split(',')
    host, port = etcd_endpoints[0].split(':')
    return etcd3.client(host=host, port=port)

def register_etcd(client, node_id, node_key, node_ip) -> bool:
    is_success, txn = client.transaction(
        compare=[client.transactions.value(f"/node/{node_id}") == str(0)],
        success=[
            client.transactions.put(f"/node/{node_id}", str(node_id)),
            client.transactions.put(f"/config/node/{node_id}/key", str(node_key)),
            client.transactions.put(f"/config/node/{node_ip}/ip", str(node_ip))
        ],
        failure=[]
    )
    return is_success

def register_and_run(node_mask, node_ip, replicas):
    etcd_client = get_etcd_client()
    node_id = 1
    node_key = 0
    time.sleep(2)

    while node_id <= replicas:
        if register_etcd(etcd_client, node_id, node_key, node_ip):
            break
        else:
            node_id += 1
            node_key = node_key_from_id(node_id, node_mask)

    if node_id > replicas:
        print("Reached the limit without registering a {}.".format(node_id))
        return 1

if __name__ == "__main__":
    node_mask = os.environ.get("NODE_MASK", "")
    node_ip = subprocess.check_output(['hostname', '-i']).decode().strip()
    replicas = int(os.environ.get('DOCKER_REPLICAS', 0))

    register_and_run(node_mask, node_ip, replicas)
