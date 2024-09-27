#!/bin/sh

set -e

export CFG_FILE_PATH="/etc/nomos/config.yaml" \
       CFG_SERVER_ADDR="http://cfgsync:4400" \
       CFG_HOST_IP=$(hostname -i)

/usr/bin/cfgsync-client && \
    exec /usr/bin/nomos-node CFG_FILE_PATH --with-metrics --log-backend gelf --log-addr graylog:12201
