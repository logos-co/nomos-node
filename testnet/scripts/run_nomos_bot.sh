#!/bin/sh

echo "I am a container ${HOSTNAME} bot"

while true
do
    /usr/bin/nomos-cli chat --author nomos-ghost --message "$(date +%H:%M:%S) ~ ping" --network-config /etc/nomos/cli_config.yml --node-addr http://bootstrap:18080
    sleep 10
done
