#!/bin/sh
# Healthcheck and seed for the `weaviate` service (the image has busybox, no curl or bash): exit 0
# only once the data is there. REST by wget, DELETE by nc. The `Empty` class is created last and is
# the marker, so an interrupted seed is redone from scratch.
export LC_ALL=C # so awk prints dots
K=$AUTHENTICATION_APIKEY_ALLOWED_KEYS
B=http://127.0.0.1:8080

# `get PATH`: the body on stdout; a non-2xx reply fails.
get() { wget -q -O - -T 20 --header "Authorization: Bearer $K" "$B$1"; }
# `post PATH FILE`: the same, with FILE as the JSON body.
post() { wget -q -O - -T 100 --header "Authorization: Bearer $K" --header 'Content-Type: application/json' --post-file "$2" "$B$1"; }
del() { printf 'DELETE %s HTTP/1.0\r\nAuthorization: Bearer %s\r\n\r\n' "$1" "$K" | nc -w 5 127.0.0.1 8080 >/dev/null; }

# `schema FILE`: create a class; `batch FILE N`: N objects, each of which must be stored.
schema() { post /v1/schema "$1" >/dev/null || exit 1; }
batch() {
  out=$(post /v1/batch/objects "$1") || exit 1
  [ "$(echo "$out" | grep -o '"status":"SUCCESS"' | wc -l)" -eq "$2" ] || exit 1
}

get /v1/schema/Empty >/dev/null 2>&1 && exit 0
get /v1/.well-known/ready >/dev/null 2>&1 || exit 1

for c in Place Doc Reserved Empty; do del "/v1/schema/$c"; done
T=$(mktemp)

# Place: 501 objects. Properties of every type: text, int, number, boolean, date, text[], geo, a nested
# object; a note that is a string, null or missing; two ints past 2^53 (as exact floats), one past 32 bits;
# one object (9001) is all unicode.
cat >"$T" <<'JSON'
{"class":"Place","vectorizer":"none","vectorIndexConfig":{"distance":"cosine"},"properties":[
{"name":"name","dataType":["text"]},
{"name":"city","dataType":["text"],"tokenization":"field"},
{"name":"kind","dataType":["text"],"tokenization":"field"},
{"name":"year","dataType":["int"]},
{"name":"rating","dataType":["number"]},
{"name":"open","dataType":["boolean"]},
{"name":"created","dataType":["date"]},
{"name":"tags","dataType":["text[]"],"tokenization":"field"},
{"name":"location","dataType":["geoCoordinates"]},
{"name":"country","dataType":["object"],"nestedProperties":[{"name":"code","dataType":["text"]},{"name":"eu","dataType":["boolean"]}]},
{"name":"note","dataType":["text"]},
{"name":"big","dataType":["int"]}]}
JSON
schema "$T"
awk 'BEGIN {
  split("London Paris Berlin Tokyo Madrid Rome Lisbon Vienna", city, " ")
  split("GB FR DE JP ES IT PT AT", code, " ")
  split("cafe museum park bridge market tower", kind, " ")
  printf "{\"objects\":["
  for (i = 1; i <= 500; i++) {
    note = (i % 5 == 0) ? sprintf(",\"note\":\"Note number %d\"", i) : ((i % 10 == 1) ? ",\"note\":null" : "")
    big = (i == 50) ? ",\"big\":9007199254740994" : ((i == 500) ? ",\"big\":9007199254740998" : ((i == 51) ? ",\"big\":3000000000" : ""))
    printf "%s{\"class\":\"Place\",\"id\":\"00000000-0000-4000-8000-%012d\",\"vector\":[%.4f,%.4f,%.4f,%.4f],\"properties\":{\"name\":\"Place %d\",\"city\":\"%s\",\"kind\":\"%s\",\"country\":{\"code\":\"%s\",\"eu\":%s},\"tags\":[\"%s\",\"%s\"],\"year\":%d,\"rating\":%.1f,\"open\":%s,\"location\":{\"latitude\":%.4f,\"longitude\":%.4f},\"created\":\"2026-01-%02dT%02d:00:00Z\"%s%s}}", \
      (i > 1 ? "," : ""), i, sin(i), cos(i), sin(2 * i), cos(3 * i), i, city[i % 8 + 1], kind[i % 6 + 1], code[i % 8 + 1], (i % 8 == 0) ? "false" : "true", kind[i % 6 + 1], kind[(i + 2) % 6 + 1], 1900 + (i * 7) % 126, (i * 37 % 50) / 10, (i % 3 == 0) ? "false" : "true", 40 + sin(i) * 10, cos(i) * 20, 1 + i % 28, i % 24, note, big
  }
  printf ",{\"class\":\"Place\",\"id\":\"00000000-0000-4000-8000-000000009001\",\"vector\":[0.5,0.5,0.5,0.5],\"properties\":{\"name\":\"Ünïcødé — 日本語 \\\"quoted\\\"\",\"city\":\"Zürich\",\"kind\":\"cafe\",\"country\":{\"code\":\"CH\",\"eu\":false},\"tags\":[],\"year\":2001,\"rating\":4.5,\"open\":true,\"location\":{\"latitude\":47.3769,\"longitude\":8.5417},\"created\":\"2026-02-01T00:00:00Z\"}}]}"
}' >"$T"
batch "$T" 501

# Doc: 5000 objects in ten batches.
cat >"$T" <<'JSON'
{"class":"Doc","vectorizer":"none","vectorIndexConfig":{"distance":"cosine"},"properties":[
{"name":"n","dataType":["int"]},
{"name":"group","dataType":["text"],"tokenization":"field"},
{"name":"title","dataType":["text"]}]}
JSON
schema "$T"
for b in 0 1 2 3 4 5 6 7 8 9; do
  awk -v b="$b" 'BEGIN {
    printf "{\"objects\":["
    for (i = b * 500 + 1; i <= b * 500 + 500; i++) {
      printf "%s{\"class\":\"Doc\",\"id\":\"10000000-0000-4000-8000-%012d\",\"vector\":[%.4f,%.4f,%.4f,%.4f],\"properties\":{\"n\":%d,\"group\":\"g%d\",\"title\":\"Doc %d\"}}", \
        (i > b * 500 + 1 ? "," : ""), i, sin(i), cos(i), sin(2 * i), cos(2 * i), i, i % 10, i
    }
    printf "]}"
  }' >"$T"
  batch "$T" 500
done

# Reserved: properties named like the row fields TableIQ adds (`_id` and `id` are Weaviate'"'"'s own, refused).
cat >"$T" <<'JSON'
{"class":"Reserved","vectorizer":"none","properties":[{"name":"_vector","dataType":["text"]},{"name":"_created","dataType":["text"]},{"name":"city","dataType":["text"]}]}
JSON
schema "$T"
cat >"$T" <<'JSON'
{"objects":[{"class":"Reserved","id":"20000000-0000-4000-8000-000000000001","vector":[0.1,0.2,0.3,0.4],"properties":{"_vector":"mine","_created":"mine too","city":"Reserved"}}]}
JSON
batch "$T" 1

# The marker, last.
cat >"$T" <<'JSON'
{"class":"Empty","vectorizer":"none","properties":[{"name":"x","dataType":["text"]}]}
JSON
schema "$T"
rm -f "$T"
exit 0
