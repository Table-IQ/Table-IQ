#!/bin/bash
# Healthcheck and seed for the `chroma` service (the image has bash and awk but no curl): exit 0
# only once the data is there. REST v2 by hand over /dev/tcp. The `empty` collection is created
# last and is the marker, so an interrupted seed is redone from scratch.
export LC_ALL=C # so the sizes are in bytes and awk prints dots
DB=/api/v2/tenants/default_tenant/databases/default_database/collections

# `req METHOD PATH [FILE]`: the whole reply (head and body) on stdout; FILE is the request body.
req() {
  local len=0
  [ -n "$3" ] && len=$(wc -c <"$3")
  exec 3<>/dev/tcp/127.0.0.1/8000 || return 1
  {
    printf '%s %s HTTP/1.0\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: %d\r\n\r\n' "$1" "$2" "$len"
    [ -n "$3" ] && cat "$3"
  } >&3
  cat <&3
  exec 3<&-
}

# `must EXPECT METHOD PATH [FILE]`: leave (so the next beat starts over) unless the reply is a 2xx holding EXPECT.
must() {
  local reply
  reply=$(req "${@:2}") || exit 1
  [[ $reply == HTTP/*" 2"* && $reply == *"$1"* ]] || exit 1
}

# `create NAME PURPOSE`: make a collection; its id on stdout.
create() {
  local reply
  echo "{\"name\":\"$1\",\"metadata\":{\"fixture\":true,\"purpose\":\"$2\"},\"configuration\":{\"hnsw\":{\"space\":\"cosine\"}}}" >"$T"
  reply=$(req POST "$DB" "$T") || exit 1
  [[ $reply == HTTP/*" 200 "* ]] || exit 1
  sed -n 's/.*"id":"\([0-9a-f-]\{36\}\)".*/\1/p' <<<"$reply" | head -n1
}

req GET "$DB/empty" | head -n1 | grep -q ' 200 ' && exit 0
req GET /api/v2/heartbeat | head -n1 | grep -q ' 200 ' || exit 1

for c in places docs empty; do req DELETE "$DB/$c" >/dev/null; done
T=$(mktemp)

# places: 503 records with explicit 4-dim embeddings (so no embedding function is needed). Metadata is
# flat (string, integer, float, boolean). Two more: `unicode` (all unicode, including a quote) and
# `reserved` (metadata keys named like the row's own fields, and no document); `big` holds an
# integer beyond 2^53 on every 50th record.
ID=$(create places "places, every metadata type")
[ -n "$ID" ] || exit 1
awk 'BEGIN {
  split("London Paris Berlin Tokyo Madrid Rome Lisbon Vienna", city, " ")
  split("GB FR DE JP ES IT PT AT", code, " ")
  split("cafe museum park bridge market tower", kind, " ")
  printf "{\"ids\":["
  for (i = 1; i <= 500; i++) printf "%s\"place-%04d\"", (i > 1 ? "," : ""), i
  printf ",\"unicode\",\"reserved\",\"nodoc\"],\"embeddings\":["
  for (i = 1; i <= 500; i++) printf "%s[%.4f,%.4f,%.4f,%.4f]", (i > 1 ? "," : ""), sin(i), cos(i), sin(2 * i), cos(3 * i)
  printf ",[0.5,0.5,0.5,0.5],[0.1,0.2,0.3,0.4],[0.9,0.1,0.1,0.1]],\"documents\":["
  for (i = 1; i <= 500; i++) printf "%s\"A %s in %s, number %d. The river runs past it.\"", (i > 1 ? "," : ""), kind[i % 6 + 1], city[i % 8 + 1], i
  printf ",\"Ünïcødé — 日本語 \\\"quoted\\\"\",null,null],\"metadatas\":["
  for (i = 1; i <= 500; i++) {
    printf "%s{\"name\":\"Place %d\",\"city\":\"%s\",\"kind\":\"%s\",\"country\":\"%s\",\"eu\":%s,\"year\":%d,\"rating\":%.1f,\"open\":%s,\"created\":\"2026-01-%02dT%02d:00:00Z\"", \
      (i > 1 ? "," : ""), i, city[i % 8 + 1], kind[i % 6 + 1], code[i % 8 + 1], (i % 8 == 0) ? "false" : "true", 1900 + (i * 7) % 126, (i * 37 % 50) / 10, (i % 3 == 0) ? "false" : "true", 1 + i % 28, i % 24
    if (i % 50 == 0) printf ",\"big\":90071992547410%02d", i / 50
    printf "}"
  }
  printf ",{\"name\":\"Ünïcødé\",\"city\":\"Zürich\",\"kind\":\"cafe\",\"country\":\"CH\",\"eu\":false,\"year\":2001,\"rating\":4.5,\"open\":true},"
  printf "{\"_id\":\"mine\",\"_document\":\"mine too\",\"_embedding\":\"mine three\",\"city\":\"Reserved\"},null]}"
}' >"$T"
must '{}' POST "$DB/$ID/add" "$T"

# docs: 5000 records in ten batches.
ID=$(create docs "5000 records, to page through")
[ -n "$ID" ] || exit 1
for b in 0 1 2 3 4 5 6 7 8 9; do
  awk -v b="$b" 'BEGIN {
    lo = b * 500 + 1; hi = lo + 499
    printf "{\"ids\":["
    for (i = lo; i <= hi; i++) printf "%s\"doc-%05d\"", (i > lo ? "," : ""), i
    printf "],\"embeddings\":["
    for (i = lo; i <= hi; i++) printf "%s[%.4f,%.4f,%.4f,%.4f]", (i > lo ? "," : ""), sin(i), cos(i), sin(2 * i), cos(2 * i)
    printf "],\"documents\":["
    for (i = lo; i <= hi; i++) printf "%s\"Document %d of the pile\"", (i > lo ? "," : ""), i
    printf "],\"metadatas\":["
    for (i = lo; i <= hi; i++) printf "%s{\"n\":%d,\"group\":\"g%d\"}", (i > lo ? "," : ""), i, i % 10
    printf "]}"
  }' >"$T"
  must '{}' POST "$DB/$ID/add" "$T"
done

# The marker, last.
create empty "no records" >/dev/null
rm -f "$T"
exit 0
