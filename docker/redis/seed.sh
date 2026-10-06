#!/bin/sh
# The healthcheck of the `redis` service: seed it once. "Once" is the `tiq:seeded` marker,
# written last — an edit or delete of seed data never triggers a re-seed, while a seed that
# died half way (no marker) starts over from an empty server. The seeded TTLs are years long
# (1 and 2): the marker keeps the server from being re-seeded, so a key that could expire during a
# dev session or a CI run would break the tests that read it for good. Tests treat these keys as
# read-only; every test that writes uses a key prefix of its own and deletes it.
cli() { redis-cli -p "${PORT:-6379}" "$@"; }
[ "$(cli PING 2>/dev/null)" = PONG ] || exit 1
[ "$(cli EXISTS tiq:seeded)" = 1 ] && exit 0

out=$(
  {
    echo 'FLUSHALL'
    cat <<'DATA'
SET demo:1 hello
SET demo:2 world EX 31536000
RPUSH list:a x y z
SADD set:a red green blue
HSET hash:a name Ada role engineer lang rust
ZADD zset:a 1 one 2 two 3 three
XADD stream:a 1700000000000-0 type login user ada
XADD stream:a 1700000000001-0 type logout user ada
XADD stream:a 1700000000002-0 type login user linus
JSON.SET json:a $ '{"name":"Ada","tags":["math","engines"],"address":{"city":"London","zip":"N1"}}'
SET ttl:short v EX 31536000
SET ttl:long v EX 63072000
SET bin:1 "\xff\xfe\x00abc"
DATA
    # A hierarchy to fold: user:<n>:name / user:<n>:email.
    for i in $(seq 1 25); do
      echo "SET user:$i:name name-$i"
      echo "SET user:$i:email u$i@example.com"
    done
    # 5000 items: pages of 100 / 1000 are exercised on it.
    seq 1 5000 | sed 's/^/RPUSH list:big item-/'
    echo 'SELECT 1'
    echo 'SET other:db1 yes'
  } | cli
)
case "$out" in
  *ERR*) echo "$out" | grep ERR >&2; exit 1 ;;
esac
cli SET tiq:seeded 1 >/dev/null
