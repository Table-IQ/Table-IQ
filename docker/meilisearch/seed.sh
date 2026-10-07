#!/bin/sh
# Healthcheck and seed for the `meilisearch` service: exit 0 only once the data is there.
# Seeds through the REST API and waits for each task; the `tiq-reader` API key (a read-only key
# TableIQ's tests connect with) is created last, so an interrupted seed is redone from scratch.
M=${M:-http://127.0.0.1:7700}
AUTH="Authorization: Bearer $MEILI_MASTER_KEY"
set -o pipefail

curl -sf -H "$AUTH" "$M/keys" | grep -q '"name":"tiq-reader"' && exit 0
curl -sf "$M/health" | grep -q available || exit 1

# `send METHOD PATH [CONTENT-TYPE]` with the body on stdin; waits for the task it enqueues.
send() {
  reply=$(curl -sf -X "$1" -H "$AUTH" -H "Content-Type: ${3:-application/json}" "$M$2" --data-binary @-) || return 1
  uid=${reply#*\"taskUid\":}
  uid=${uid%%[!0-9]*}
  case $uid in '' | *[!0-9]*) return 1 ;; esac
  n=0
  while [ $n -lt 120 ]; do
    status=$(curl -sf -H "$AUTH" "$M/tasks/$uid") || return 1
    case $status in
      *'"status":"succeeded"'*) return 0 ;;
      *'"status":"failed"'* | *'"status":"canceled"'*) return 1 ;;
    esac
    n=$((n + 1))
    sleep 1
  done
  return 1
}

for i in movies logs empty; do
  echo '{}' | send DELETE "/indexes/$i" || true
done
echo '{"uid":"movies","primaryKey":"id"}' | send POST /indexes || exit 1
echo '{"uid":"logs","primaryKey":"id"}' | send POST /indexes || exit 1
echo '{"uid":"empty","primaryKey":"id"}' | send POST /indexes || exit 1

echo '{
  "searchableAttributes": ["title", "overview", "tagline", "genres"],
  "filterableAttributes": ["genres", "release_year", "rating", "in_catalog", "director.country"],
  "sortableAttributes": ["release_year", "rating", "title"]
}' | send PATCH /indexes/movies/settings || exit 1
echo '{
  "searchableAttributes": ["message"],
  "filterableAttributes": ["level", "service", "status", "duration_ms"],
  "sortableAttributes": ["duration_ms", "status"]
}' | send PATCH /indexes/logs/settings || exit 1

# 500 movies: every JSON type, arrays (genres, tags), objects (director), an array of objects (cast), null (tagline).
awk 'BEGIN {
  split("Quiet Crimson Hollow Distant Golden Broken Silent Electric Paper Midnight", adj, " ")
  split("River Garden Engine Harbor Letter Mountain Orchard Signal Tower Window", noun, " ")
  split("Ada_Lovelace Alan_Turing Grace_Hopper Edsger_Dijkstra Donald_Knuth Barbara_Liskov Ken_Thompson Margaret_Hamilton Linus_Torvalds Frances_Allen Tim_Berners-Lee Radia_Perlman", au, " ")
  split("Drama Comedy Thriller Documentary Animation Horror", ge, " ")
  split("classic new bestseller signed reprint translated", tg, " ")
  split("US UK DE JP", co, " ")
  for (i = 1; i <= 500; i++) {
    d = au[i % 12 + 1]; gsub("_", " ", d)
    title = sprintf("The %s %s%s", adj[i % 10 + 1], noun[int(i / 10) % 10 + 1], i > 100 ? sprintf(" (part %d)", int(i / 100)) : "")
    tagline = (i % 5 == 0) ? sprintf("\"Tagline number %d\"", i) : "null"
    ncast = i % 4; cast = ""
    for (j = 1; j <= ncast; j++)
      cast = cast (j > 1 ? "," : "") sprintf("{\"name\":\"Actor %d\",\"billing\":%d}", (i + j) % 9, j)
    printf "{\"id\":%d,\"title\":\"%s\",\"overview\":\"A story of %s and the %s, number %d.\",\"genres\":[\"%s\",\"%s\"],\"release_year\":%d,\"rating\":%.1f,\"in_catalog\":%s,\"tags\":[\"%s\",\"%s\"],\"tagline\":%s,\"director\":{\"name\":\"%s\",\"country\":\"%s\"},\"cast\":[%s]}\n", \
      i, title, tolower(adj[i % 10 + 1]), tolower(noun[int(i / 10) % 10 + 1]), i, ge[i % 6 + 1], ge[(i + 2) % 6 + 1], 1900 + (i * 7) % 126, (i * 37 % 50) / 10, (i % 3 == 0) ? "false" : "true", tg[i % 6 + 1], tg[(i + 2) % 6 + 1], tagline, d, co[i % 4 + 1], cast
  }
  # Text that must survive the round trip: quotes, non-ASCII.
  print "{\"id\":9001,\"title\":\"Ünïcødé — 日本語 \\\"quoted\\\"\",\"overview\":\"Zoë Ångström writes poems.\",\"genres\":[],\"release_year\":2001,\"rating\":4.5,\"in_catalog\":true,\"tags\":[],\"tagline\":null,\"director\":{\"name\":\"Zoë Ångström\",\"country\":\"JP\"},\"cast\":[]}"
}' | send POST /indexes/movies/documents application/x-ndjson || exit 1

# 5000 log lines, one a minute from 2026-01-01.
awk 'BEGIN {
  split("debug info info warn error", lv, " ")
  split("api web worker billing", sv, " ")
  split("200 200 201 404 500 503", st, " ")
  for (i = 0; i < 5000; i++)
    printf "{\"id\":%d,\"ts\":\"2026-01-%02dT%02d:%02d:00Z\",\"level\":\"%s\",\"service\":\"%s\",\"status\":%s,\"duration_ms\":%.1f,\"message\":\"request %d handled by %s\"}\n", \
      i, 1 + int(i / 1440), int(i % 1440 / 60), i % 60, lv[i % 5 + 1], sv[i % 4 + 1], st[i % 6 + 1], (i * 13) % 2000 + 0.5, i, sv[i % 4 + 1]
}' | send POST /indexes/logs/documents application/x-ndjson || exit 1

# The marker, last: a key that can search and read (a "search key" that can also list indexes).
echo '{"name":"tiq-reader","description":"TableIQ test fixture","actions":["search","documents.get","indexes.get","settings.get","stats.get","version"],"indexes":["*"],"expiresAt":null}' |
  curl -sf -X POST -H "$AUTH" -H 'Content-Type: application/json' "$M/keys" --data-binary @- >/dev/null || exit 1
exit 0
