#!/bin/sh
# Healthcheck and seed for the rqlite services (the image is Alpine: busybox wget and sh, no
# curl): exit 0 only once seed.sql has been applied. $SEED_AUTH, when set, is a full
# Authorization header value for a server that wants credentials.
url=http://127.0.0.1:4001
# busybox wget: a header goes in even when empty, so only one without credentials is kept plain.
get() { wget -q -O - --header "Authorization: ${SEED_AUTH:-none}" "$url/db/query?q=SELECT+v+FROM+_tiq_meta+WHERE+k%3D%27seeded%27" 2>/dev/null; }
get | grep -q '"values":\[\["1"\]\]' && exit 0
# One POST of the whole file to /db/load (rqlite's SQL-text import), applied as one batch.
wget -q -O /dev/null --header "Authorization: ${SEED_AUTH:-none}" --header 'Content-Type: text/plain' \
  --post-file /seed.sql "$url/db/load" 2>/dev/null
exit 1 # the next beat finds the marker
