#!/usr/bin/env python3
"""Regenerate the engine logo strip and grid in dist/index.html from the app repo.

usage: python3 scripts/engines.py /path/to/Table-IQ

Reads src/lib/profiles.json and src/assets/engines/*.svg from the app, copies the
logos into dist/img/engines/, and rewrites the HTML between the
<!-- engines:strip --> and <!-- engines:grid --> markers. A profile with
tested=false is shown as Beta, the same rule the app's picker uses.
"""
import html, json, pathlib, re, shutil, sys

app = pathlib.Path(sys.argv[1])
site = pathlib.Path(__file__).resolve().parent.parent / "dist"
profiles = json.loads((app / "src/lib/profiles.json").read_text())

# Native drivers, mirroring ENGINE_LABELS in src/components/ui/EngineIcon.tsx.
SQL = [("postgres", "PostgreSQL"), ("mysql", "MySQL"), ("sqlserver", "SQL Server"), ("sqlite", "SQLite"), ("duckdb", "DuckDB")]
DOC = [("mongodb", "MongoDB"), ("redis", "Redis")]
HTTP = [("clickhouse", "ClickHouse"), ("libsql", "Turso / libSQL"), ("elasticsearch", "Elasticsearch"), ("opensearch", "OpenSearch"),
        ("rqlite", "rqlite"), ("meilisearch", "Meilisearch"), ("qdrant", "Qdrant"), ("influxdb", "InfluxDB 3"), ("chroma", "Chroma"), ("weaviate", "Weaviate")]
CLOUD = ("rds-", "azure-", "cloudsql-", "neon", "supabase", "alloydb")

out = site / "img/engines"
out.mkdir(parents=True, exist_ok=True)
for key, _ in SQL + DOC + HTTP:
    shutil.copy(app / f"src/assets/engines/{key}.svg", out / f"{key}.svg")
for extra in ("README.md", "LICENSE-devicon"):
    shutil.copy(app / f"src/assets/engines/{extra}", out / extra)

def logo(key, label):
    return f'<li class="eng"><span class="mark"><img src="/img/engines/{key}.svg" alt="" width="24" height="24" loading="lazy"></span>{html.escape(label)}</li>'

# Compatible products: the vendor mark from img/engines/profiles/ (see SOURCES.md there), else the app's monogram.
def product(k, p):
    beta = "" if p["tested"] else ' <span class="beta">Beta</span>'
    marks = sorted((out / "profiles").glob(f"{k}.*"))
    mark = (f'<span class="mark"><img src="/img/engines/profiles/{marks[0].name}" alt="" width="24" height="24" loading="lazy"></span>' if marks
            else f'<span class="mark mono" style="background:{p["color"]}" aria-hidden="true">{html.escape(p["monogram"])}</span>')
    return f'<li class="eng">{mark}{html.escape(p["label"])}{beta}</li>'

# Verified first, then Beta: the order the app's picker uses.
profiles = dict(sorted(profiles.items(), key=lambda kv: not kv[1]["tested"]))

seen, cloud = set(), []
for k, p in profiles.items():
    if k.startswith(CLOUD) and p["label"] not in seen:
        seen.add(p["label"])
        cloud.append(product(k, p))
pg = [product(k, p) for k, p in profiles.items() if p["base"] == "postgres" and not k.startswith(CLOUD)]
my = [product(k, p) for k, p in profiles.items() if p["base"] == "mysql" and not k.startswith(CLOUD) and k != "mariadb"]
total = len(SQL + DOC + HTTP) + len({p["label"] for p in profiles.values()})

groups = [
    ("Full SQL client", "Browse, query, edit, structure, ERD, backup and restore.", [logo(*e) for e in SQL] + [product("mariadb", profiles["mariadb"])]),
    ("Postgres compatible", "Runs on the PostgreSQL driver.", pg),
    ("MySQL compatible", "Runs on the MySQL driver.", my),
    ("Managed cloud", "Your hosted Postgres and MySQL, on the same drivers.", cloud),
    ("Document and key-value", "Dedicated browsers with staged edits.", [logo(*e) for e in DOC]),
    ("Search, analytics and vector", "Read-only over HTTP.", [logo(*e) for e in HTTP]),
]
grid = "\n".join(
    f'<div class="eng-group"><h3>{t}</h3><p>{d}</p><ul class="eng-list">{"".join(items)}</ul></div>' for t, d, items in groups)
strip_keys = ["postgres", "mysql", "sqlserver", "sqlite", "duckdb", "mongodb", "redis", "clickhouse", "elasticsearch", "qdrant"]
labels = dict(SQL + DOC + HTTP)
strip = (f'<p>Connects to {total} databases and cloud services</p><ul class="logo-strip">'
         + "".join(f'<li><img src="/img/engines/{k}.svg" alt="{labels[k]}" title="{labels[k]}" width="32" height="32"></li>' for k in strip_keys)
         + f'<li class="more"><a href="#engine-list">+{total - len(strip_keys)} more</a></li></ul>')

index = site / "index.html"
s = index.read_text()
for name, body in (("strip", strip), ("grid", grid)):
    s, n = re.subn(rf"(<!-- engines:{name} -->).*?(<!-- /engines:{name} -->)", lambda m: f"{m[1]}\n{body}\n{m[2]}", s, flags=re.S)
    assert n == 1, f"marker engines:{name} missing"
index.write_text(s)
print(f"{total} engines; {len(pg)} pg, {len(my)} mysql, {len(cloud)} cloud")
