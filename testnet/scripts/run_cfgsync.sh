#!/bin/sh

set -e

exec /usr/bin/cfgsync-server /etc/nomos/cfgsync.yaml
