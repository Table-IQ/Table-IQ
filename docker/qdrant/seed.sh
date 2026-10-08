#!/bin/bash
# Healthcheck and seed for the `qdrant` service (the image has no curl): exit 0 only once the data
# is there. REST by hand over /dev/tcp. The `empty` collection is created last and is the marker,
# so an interrupted seed is redone from scratch.
export LC_ALL=C # so the sizes are in bytes and awk prints dots
KEY=$QDRANT__SERVICE__API_KEY

# `req METHOD PATH [FILE]`: the whole reply (head and body) on stdout; FILE is the request body.
req() {
  local len=0
  [ -n "$3" ] && len=$(wc -c <"$3")
  exec 3<>/dev/tcp/127.0.0.1/6333 || return 1
  {
    printf '%s %s HTTP/1.0\r\nHost: localhost\r\napi-key: %s\r\nContent-Type: application/json\r\nContent-Length: %d\r\n\r\n' "$1" "$2" "$KEY" "$len"
    [ -n "$3" ] && cat "$3"
  } >&3
  cat <&3
  exec 3<&-
}

# `must EXPECT METHOD PATH [FILE]`: leave (so the next beat starts over) unless the reply is a 200 holding EXPECT.
must() {
  local reply
  reply=$(req "${@:2}") || exit 1
  [[ $reply == HTTP/*" 200 "* && $reply == *"$1"* ]] || exit 1
}

req GET /collections/empty | head -n1 | grep -q ' 200 ' && exit 0
req GET /readyz | head -n1 | grep -q ' 200 ' || exit 1

for c in places docs empty; do req DELETE "/collections/$c" >/dev/null; done
T=$(mktemp)

# places: 501 points. Ids are integers (1-400) and UUIDs (401-500); payloads hold every JSON type,
# nested objects, arrays, nulls and a geo point; one point (9001) is all unicode.
echo '{"vectors":{"size":4,"distance":"Cosine"}}' >"$T"
must '"result":true' PUT /collections/places "$T"
awk 'BEGIN {
  split("London Paris Berlin Tokyo Madrid Rome Lisbon Vienna", city, " ")
  split("GB FR DE JP ES IT PT AT", code, " ")
  split("cafe museum park bridge market tower", kind, " ")
  printf "{\"points\":["
  for (i = 1; i <= 500; i++) {
    id = (i <= 400) ? i : sprintf("\"00000000-0000-4000-8000-%012d\"", i)
    note = (i % 5 == 0) ? sprintf("\"Note number %d\"", i) : "null"
    printf "%s{\"id\":%s,\"vector\":[%.4f,%.4f,%.4f,%.4f],\"payload\":{\"name\":\"Place %d\",\"city\":\"%s\",\"kind\":\"%s\",\"country\":{\"code\":\"%s\",\"eu\":%s},\"tags\":[\"%s\",\"%s\"],\"year\":%d,\"rating\":%.1f,\"open\":%s,\"note\":%s,\"location\":{\"lat\":%.4f,\"lon\":%.4f},\"created\":\"2026-01-%02dT%02d:00:00Z\"}}", \
      (i > 1 ? "," : ""), id, sin(i), cos(i), sin(2 * i), cos(3 * i), i, city[i % 8 + 1], kind[i % 6 + 1], code[i % 8 + 1], (i % 8 == 0) ? "false" : "true", kind[i % 6 + 1], kind[(i + 2) % 6 + 1], 1900 + (i * 7) % 126, (i * 37 % 50) / 10, (i % 3 == 0) ? "false" : "true", note, 40 + sin(i) * 10, cos(i) * 20, 1 + i % 28, i % 24
  }
  printf ",{\"id\":9001,\"vector\":[0.5,0.5,0.5,0.5],\"payload\":{\"name\":\"Ünïcødé — 日本語 \\\"quoted\\\"\",\"city\":\"Zürich\",\"kind\":\"cafe\",\"country\":{\"code\":\"CH\",\"eu\":false},\"tags\":[],\"year\":2001,\"rating\":4.5,\"open\":true,\"note\":null,\"location\":{\"lat\":47.3769,\"lon\":8.5417},\"created\":\"2026-02-01T00:00:00Z\"}}]}"
}' >"$T"
must '"status":"completed"' PUT '/collections/places/points?wait=true' "$T"
for idx in 'city keyword' 'kind keyword' 'country.code keyword' 'year integer' 'rating float' 'location geo' 'created datetime'; do
  set -- $idx
  echo "{\"field_name\":\"$1\",\"field_schema\":\"$2\"}" >"$T"
  must '"status":"completed"' PUT '/collections/places/index?wait=true' "$T"
done

# docs: 5000 points with two named vectors, in ten batches.
echo '{"vectors":{"title":{"size":4,"distance":"Cosine"},"body":{"size":8,"distance":"Dot"}}}' >"$T"
must '"result":true' PUT /collections/docs "$T"
for b in 0 1 2 3 4 5 6 7 8 9; do
  awk -v b="$b" 'BEGIN {
    printf "{\"points\":["
    for (i = b * 500 + 1; i <= b * 500 + 500; i++) {
      printf "%s{\"id\":%d,\"vector\":{\"title\":[%.4f,%.4f,%.4f,%.4f],\"body\":[%.4f,%.4f,%.4f,%.4f,%.4f,%.4f,%.4f,%.4f]},\"payload\":{\"n\":%d,\"group\":\"g%d\",\"title\":\"Doc %d\"}}", \
        (i > b * 500 + 1 ? "," : ""), i, sin(i), cos(i), sin(2 * i), cos(2 * i), sin(i), cos(i), sin(3 * i), cos(3 * i), sin(4 * i), cos(4 * i), sin(5 * i), cos(5 * i), i, i % 10, i
    }
    printf "]}"
  }' >"$T"
  must '"status":"completed"' PUT '/collections/docs/points?wait=true' "$T"
done
for idx in 'group keyword' 'n integer'; do
  set -- $idx
  echo "{\"field_name\":\"$1\",\"field_schema\":\"$2\"}" >"$T"
  must '"status":"completed"' PUT '/collections/docs/index?wait=true' "$T"
done

# The marker, last.
echo '{"vectors":{"size":4,"distance":"Cosine"}}' >"$T"
must '"result":true' PUT /collections/empty "$T"
rm -f "$T"
exit 0
