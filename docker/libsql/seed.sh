#!/bin/bash
# Healthcheck and seed for the `libsql` service (the image has no curl): exit 0 only once
# seed.sql has been applied. Hrana over HTTP, by hand: one POST to /v2/pipeline per call.
export LC_ALL=C # so ${#1} is the body's length in bytes
post() {
  exec 3<>/dev/tcp/127.0.0.1/8080 || return 1
  printf 'POST /v2/pipeline HTTP/1.0\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: %d\r\n\r\n%s' "${#1}" "$1" >&3
  cat <&3
  exec 3<&-
}
post '{"requests":[{"type":"execute","stmt":{"sql":"SELECT v FROM _tiq_meta WHERE k = '"'seeded'"'","want_rows":true}},{"type":"close"}]}' | grep -q '"value":"1"' && exit 0
# seed.sql has no quotes, backslashes or tabs, so joining its lines is all the JSON it needs.
# One `sequence` request runs it on one stream; its BEGIN/COMMIT make it all-or-nothing.
post '{"requests":[{"type":"sequence","sql":"'"$(grep -v '^--' /seed.sql | tr '\n' ' ')"'"},{"type":"close"}]}' >/dev/null
exit 1 # the next beat finds the marker
