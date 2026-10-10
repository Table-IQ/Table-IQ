#!/bin/bash
# Healthcheck and seed for the InfluxDB services: exit 0 only once the data is there.
# $INFLUX_TOKEN, when set, is the bearer token of a server that wants one.
#
# `tiq` holds `cpu` (20000 points: 8 hosts in 4 regions, every field type, NULLs by missing
# fields, int64 max/min, unicode) and `events` (5000 points). `tiq_other` is written LAST and is
# the marker: a write that fails stops the script (set -e, curl -f) before it, so the next beat
# starts over (points are keyed by tags + time, so writing them twice changes nothing).
set -eo pipefail
I=http://127.0.0.1:8181
auth=()
[ -n "$INFLUX_TOKEN" ] && auth=(-H "Authorization: Bearer $INFLUX_TOKEN")

databases() { curl -sf "${auth[@]}" "$I/api/v3/configure/database?format=json"; }
databases 2>/dev/null | grep -q '"tiq_other"' && exit 0
curl -sf "${auth[@]}" "$I/health" >/dev/null || exit 1

# `write DB` reads line protocol (nanosecond-free: timestamps are in seconds) from stdin.
write() { curl -sf "${auth[@]}" -X POST "$I/api/v3/write_lp?db=$1&precision=second" --data-binary @- >/dev/null; }

# cpu: 20000 points, 1700000000 + 60 s per pair of points (two hosts share each timestamp, so a
# page ordered by time alone would be ambiguous). Missing fields are NULLs.
awk 'BEGIN {
  split("us-east us-west eu-west 東京", region, " ")
  for (i = 0; i < 20000; i++) {
    h = i % 8
    t = 1700000000 + int(i / 2) * 60
    f = ""
    sep = ""
    if (i % 7 != 0) { f = f sep sprintf("usage_user=%s", (i * 37 % 10000) / 100); sep = "," }
    if (i % 5 != 0) { f = f sep sprintf("usage_system=%s", (i * 13 % 5000) / 100); sep = "," }
    f = f sep sprintf("cores=%di", i); sep = ","
    if (i % 5 == 0) f = f sep sprintf("label=\"héllo wörld 🚀 #%d\"", i)
    else if (i % 11 == 0) f = f sep "label=\"\""
    if (i % 9 != 0) f = f sep sprintf("online=%s", i % 2 == 0 ? "true" : "false")
    printf "cpu,host=host-%d,region=%s %s %d\n", h, region[h % 4 + 1], f, t
  }
}' | write tiq

# Two points that hold the int64 extremes next to the ordinary ones (their own timestamps).
printf '%s\n' \
  'cpu,host=host-x,region=edge usage_user=3,cores=9223372036854775807i,label="max" 1699999000' \
  'cpu,host=host-x,region=edge usage_user=4,cores=-9223372036854775808i,label="min" 1699999060' | write tiq

# events: 5000 points, every 30 s; `seq` is always there, the other fields come and go.
awk 'BEGIN {
  split("view click buy share", kind, " ")
  for (i = 0; i < 5000; i++) {
    f = sprintf("seq=%di", i)
    if (i % 4 != 0) f = f sprintf(",amount=%s", (i % 1000) / 4)
    if (i % 3 != 0) f = f sprintf(",payload=\"event-%d\"", i)
    if (i % 5 != 0) f = f sprintf(",count=%di", i % 50)
    printf "events,kind=%s %s %d\n", kind[i % 4 + 1], f, 1700000000 + i * 30
  }
}' | write tiq

# The marker, last, in a second database.
printf '%s\n' 'ledger,account=a1 balance=100.5,note="opening" 1700000000' 'ledger,account=a2 balance=-3.25,note="é" 1700000060' | write tiq_other
exit 0
