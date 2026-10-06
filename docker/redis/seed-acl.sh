#!/bin/sh
# The healthcheck of the `redis-acl` service (REDISCLI_AUTH is the `default` user's password):
# seed it once, marker last. `agent` may touch `agent:*` only.
cli() { redis-cli "$@"; }
[ "$(cli PING 2>/dev/null)" = PONG ] || exit 1
[ "$(cli EXISTS tiq:seeded)" = 1 ] && exit 0
cli FLUSHALL >/dev/null
cli SET agent:1 mine >/dev/null
cli RPUSH agent:list a b c >/dev/null
cli SET other:1 secret >/dev/null
cli SET tiq:seeded 1 >/dev/null
