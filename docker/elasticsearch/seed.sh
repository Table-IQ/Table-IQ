#!/bin/bash
# Healthcheck and seed for the `elasticsearch` service: exit 0 only once the data is there.
# Seeds through the REST API; the `.tiq_meta` marker (a dot-index, which TableIQ hides) is
# written last, so an interrupted seed is redone from scratch on the next beat.
ES=${ES:-http://127.0.0.1:9200}
# A failed stage of a pipe (the awk generator, or curl) fails the whole pipe.
set -o pipefail
curl -sf "$ES/.tiq_meta/_doc/seeded" | grep -q '"found":true' && exit 0
curl -sf "$ES/_cluster/health?wait_for_status=yellow&timeout=30s" >/dev/null || exit 1

put() { curl -sf -X "$1" "$ES/$2" -H 'Content-Type: application/json' -d "$3" >/dev/null || exit 1; }
# `return`, not `exit`: it runs on the right of a pipe, in a subshell, so only the caller's
# `|| exit 1` can stop the script before the marker. The reply is read whole (not through
# `grep -q`, which would close the pipe on curl); `"errors":true` is a failed bulk.
bulk() {
  local reply
  reply=$(curl -sf -X POST "$ES/_bulk?refresh=true" -H 'Content-Type: application/x-ndjson' --data-binary @-) || return 1
  case $reply in *'"errors":false'*) ;; *) return 1 ;; esac
}

curl -s -X DELETE "$ES/books,logs-2026,empty-index" >/dev/null

put PUT books '{
  "mappings": { "properties": {
    "title":     { "type": "text", "fields": { "raw": { "type": "keyword" } } },
    "author":    { "type": "keyword" },
    "genre":     { "type": "keyword" },
    "year":      { "type": "long" },
    "published": { "type": "date" },
    "rating":    { "type": "float" },
    "in_print":  { "type": "boolean" },
    "tags":      { "type": "keyword" },
    "note":      { "type": "text" },
    "publisher": { "properties": { "name": { "type": "keyword" }, "country": { "type": "keyword" } } },
    "reviews":   { "type": "nested", "properties": {
      "user": { "type": "keyword" }, "stars": { "type": "long" }, "text": { "type": "text" } } }
  } }
}'
put PUT logs-2026 '{
  "mappings": { "properties": {
    "@timestamp":  { "type": "date" },
    "level":       { "type": "keyword" },
    "service":     { "type": "keyword" },
    "status":      { "type": "long" },
    "duration_ms": { "type": "float" },
    "message":     { "type": "text" }
  } }
}'
put PUT empty-index '{ "mappings": { "properties": { "name": { "type": "keyword" } } } }'

# 500 books: every field type, an array (tags), nested (reviews), an object (publisher), null (note).
awk 'BEGIN {
  split("Quiet Crimson Hollow Distant Golden Broken Silent Electric Paper Midnight", adj, " ")
  split("River Garden Engine Harbor Letter Mountain Orchard Signal Tower Window", noun, " ")
  split("Ada_Lovelace Alan_Turing Grace_Hopper Edsger_Dijkstra Donald_Knuth Barbara_Liskov Ken_Thompson Margaret_Hamilton Linus_Torvalds Frances_Allen Tim_Berners-Lee Radia_Perlman", au, " ")
  split("fiction science history poetry biography travel", ge, " ")
  split("classic new bestseller signed reprint translated", tg, " ")
  split("Northwind Contoso Fabrikam Initech Hooli", pu, " ")
  split("US UK DE JP", co, " ")
  for (i = 1; i <= 500; i++) {
    a = au[i % 12 + 1]; gsub("_", " ", a)
    year = 1900 + (i * 7) % 126
    title = sprintf("The %s %s%s", adj[i % 10 + 1], noun[int(i / 10) % 10 + 1], i > 100 ? sprintf(" (vol. %d)", int(i / 100)) : "")
    note = (i % 5 == 0) ? sprintf("\"Margin note %d\"", i) : "null"
    nrev = i % 4; rev = ""
    for (j = 1; j <= nrev; j++)
      rev = rev (j > 1 ? "," : "") sprintf("{\"user\":\"u%d\",\"stars\":%d,\"text\":\"Review %d of book %d\"}", (i + j) % 9, (i + j) % 5 + 1, j, i)
    printf "{\"index\":{\"_index\":\"books\",\"_id\":\"b%d\"}}\n", i
    printf "{\"title\":\"%s\",\"author\":\"%s\",\"genre\":\"%s\",\"year\":%d,\"published\":\"%d-%02d-%02dT00:00:00Z\",\"rating\":%.1f,\"in_print\":%s,\"tags\":[\"%s\",\"%s\"],\"note\":%s,\"publisher\":{\"name\":\"%s\",\"country\":\"%s\"},\"reviews\":[%s]}\n", \
      title, a, ge[i % 6 + 1], year, year, i % 12 + 1, i % 28 + 1, (i * 37 % 50) / 10, (i % 3 == 0) ? "false" : "true", tg[i % 6 + 1], tg[(i + 2) % 6 + 1], note, pu[i % 5 + 1], co[i % 4 + 1], rev
  }
  # Text that must survive the round trip: quotes, non-ASCII, a very long word.
  print "{\"index\":{\"_index\":\"books\",\"_id\":\"b-unicode\"}}"
  print "{\"title\":\"Ünïcødé — 日本語 \\\"quoted\\\"\",\"author\":\"Zoë Ångström\",\"genre\":\"poetry\",\"year\":2001,\"published\":\"2001-09-09T01:46:40Z\",\"rating\":4.5,\"in_print\":true,\"tags\":[],\"note\":null,\"publisher\":{\"name\":\"Ünï\",\"country\":\"JP\"},\"reviews\":[]}"
}' | bulk || exit 1

# 5000 log lines, one a minute from 2026-01-01.
awk 'BEGIN {
  split("debug info info warn error", lv, " ")
  split("api web worker billing", sv, " ")
  split("200 200 201 404 500 503", st, " ")
  for (i = 0; i < 5000; i++) {
    printf "{\"index\":{\"_index\":\"logs-2026\",\"_id\":\"l%d\"}}\n", i
    printf "{\"@timestamp\":\"2026-01-%02dT%02d:%02d:00Z\",\"level\":\"%s\",\"service\":\"%s\",\"status\":%s,\"duration_ms\":%.1f,\"message\":\"request %d handled by %s\"}\n", \
      1 + int(i / 1440), int(i % 1440 / 60), i % 60, lv[i % 5 + 1], sv[i % 4 + 1], st[i % 6 + 1], (i * 13) % 2000 + 0.5, i, sv[i % 4 + 1]
  }
}' | bulk || exit 1

put PUT '.tiq_meta/_doc/seeded?refresh=true' '{"v":1}'
exit 0
