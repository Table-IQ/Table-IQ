"""Build editable, static HyperFrames storyboard poses from shipped real-app assets.

This creates no video and simulates no product interaction. Run from any directory.
"""
from pathlib import Path
import html
import json

ROOT = Path(__file__).resolve().parent

SHOTS = [
    dict(id="hook", start=0, end=3, title="Still counting query tabs?", act="Hook",
         words="Large type lands first; the actual editor enters from the lower edge. No logo intro.",
         action="9-frame title wipe; 15-frame horizontal handoff to the workspace.",
         source="Real native 0.8.17 editor, before manual Run. Editorial title outside the app.",
         sound="Opening beat; one restrained accent on ‘query tabs’.",
         body='<h1 class="hook-title"><span>Still counting</span><span class="signal">query tabs?</span></h1><img class="hook-editor" src="assets/ai-generated.png" alt="Actual idle native SQL editor" data-layout-allow-overflow>'),
    dict(id="room", start=3, end=7, title="Keep your investigation together", act="Problem",
         words="The missing shot is three real tabs opening. This still establishes the workspace; no tab interaction is fabricated.",
         action="Record orders → customers → query tabs; hold the third tab open.",
         source="Real native 0.8.15 workspace crop. Multi-tab capture pickup required.",
         sound="Clicks on the real tab-opening actions, once recorded.",
         body='<h1 class="heading two-line">Keep your investigation together</h1><img class="workspace" src="assets/workspace.png" alt="Real storefront database workspace">'),
    dict(id="client", start=7, end=12, title="Free. Open source. Local first.", act="Solution",
         words="Name the ordinary database client before the AI or MCP features. The client is local first; cloud AI is optional.",
         action="Wordmark reveals once, then the real workspace expands to fill the next shot.",
         source="Supplied TableIQ vector wordmark and native 0.8.15 workspace.",
         sound="Bed resolves at the product reveal; gentle handoff accent.",
         body='<img class="brand" src="assets/tableiq-logo-horizontal-light.svg" alt="TableIQ"><h1 class="solution-title"><span>Free. Open source.</span><span class="signal">Local first.</span></h1><p class="category">A database client for Mac</p><img class="solution-workspace" src="assets/workspace.png" alt="Real native database workspace">'),
    dict(id="workflow", start=12, end=21, title="Browse. Query. Review.", act="Proof",
         words="Establish the database, then isolate table, schema and editor actions. Use the same sample database throughout.",
         action="Real table selection → schema → SQL editor; 12–18 frame camera moves, then holds.",
         source="Native 0.8.15 schema and native 0.8.17 editor stills. Matching-version recording pickup required.",
         sound="Three quiet action clicks; headings stay still while read.",
         body='<h1 class="heading">Browse. Query. Review.</h1><img class="workflow-schema" src="assets/schema.png" alt="Real sample database schema"><img class="workflow-sql" src="assets/ai-generated.png" alt="Real SQL editor before execution">'),
    dict(id="byok", start=21, end=27, title="AI, with your own key", act="Optional AI",
         words="AI helps when requested. Settings are genuine, and no API credential is visible.",
         action="Push into real provider selection, then hand off to the investigation’s question.",
         source="Real native 0.8.15 AI settings crop; configuration only. Refresh on 0.8.17 before final cut.",
         sound="One settings click; music stays under proposed narration.",
         body='<h1 class="heading">AI, with your own key</h1><img class="settings" src="assets/ai-settings.png" alt="Real provider settings without a visible credential"><p class="support">Optional AI · Provider API charges apply</p>'),
    dict(id="review", start=27, end=37, title="Ask. Review the SQL.", act="AI proof",
         words="Question summary: paid orders with no shipped shipment. Show the real generated SQL while the editor is idle.",
         action="Record exact question submission; cut to verified generated SQL; focus EXISTS and NOT EXISTS without rewriting the result.",
         source="Genuine 0.8.17 Gemini-generated SQL. A continuous prompt-to-editor recording is still required.",
         sound="Question submission click; any condensed generation wait is labeled.",
         body='<h1 class="heading">Ask. Review the SQL.</h1><p class="question">Paid orders with no shipped shipment</p><img class="sql-proof" src="assets/sql-crop.png" alt="Actual Gemini-generated SQL and manual Run control, cropped from the idle editor">'),
    dict(id="result", start=37, end=44, title="You choose when to run", act="Payoff",
         words="The actual returned rows are order 102 and order 104, totaling 7,300 cents in this sample.",
         action="Hold on manual Run; show returned rows at normal speed. Before/after stills do not simulate a live click.",
         source="Genuine native 0.8.17 result crop; exact rows independently checked against SQLite.",
         sound="Brief bed reduction before manual Run; one restrained result accent.",
         body='<h1 class="heading">You choose when to run</h1><img class="result-proof" src="assets/result-crop.png" alt="Actual rows: 102 Northstar Demo 4200, 104 Acme Demo 3100"><p class="result-line">2 orders <span>· 7,300 cents</span></p>'),
    dict(id="activity", start=44, end=50, title="See your agent’s queries", act="MCP extension",
         words="Only now introduce agent integration. This is a separately recorded local Codex session, not a continuation of the Gemini question.",
         action="Open actual MCP Activity; isolate discovery → schema → run_query calls. Keep client and query context visible.",
         source="Real native 0.8.15 Activity crop from the published Argo film. Current-toolbar entry-point pickup required.",
         sound="Activity click; separate-session narration bridge.",
         body='<h1 class="heading">See your agent’s queries</h1><img class="activity-proof" src="assets/activity.png" alt="Recorded Codex calls in actual MCP Activity"><p class="session-note">Separate Codex session · Real MCP Activity</p>'),
    dict(id="download", start=50, end=56, title="Get the Mac developer preview", act="CTA",
         words="One destination: tableiq.ideolyst.com. Mac Apple silicon preview available; Windows and Linux coming soon.",
         action="Wordmark resolves into a stable end card. Hold the download URL for at least three seconds.",
         source="Supplied wordmark; current public preview release metadata. No new platforms implied.",
         sound="Final musical resolution and a clean ending, with no reference soundtrack reuse.",
         body='<img class="end-brand" src="assets/tableiq-logo-horizontal-light.svg" alt="TableIQ"><h1 class="cta-title"><span>Get the Mac</span><span>developer preview</span></h1><p class="url">tableiq.ideolyst.com</p><p class="cta-meta">Built with Rust · No TableIQ account required</p><p class="platform">Mac Apple silicon · Unsigned / not notarized</p><p class="soon">Windows and Linux coming soon</p>'),
]

CSS = """
@font-face{font-family:Instrument;src:url('assets/fonts/instrument-sans-latin-500-normal.woff2');font-weight:500}
@font-face{font-family:Instrument;src:url('assets/fonts/instrument-sans-latin-600-normal.woff2');font-weight:600}
*{box-sizing:border-box}html,body{margin:0;width:1920px;height:1080px;overflow:hidden;background:#F1F6FB;color:#0C1A2B;font-family:Instrument,system-ui,sans-serif;font-weight:500}
#root{position:relative;width:100%;height:100%;overflow:hidden}.scene{position:relative;width:100%;height:100%;overflow:hidden;background:#F1F6FB}
h1,p{margin:0}h1{font-size:96px;line-height:1.05;letter-spacing:-3px;font-weight:600}.signal{color:#0B7480}h1 span{display:block}
.heading{position:absolute;left:192px;top:106px;max-width:1536px;font-size:88px}.two-line{max-width:1420px}
img{display:block}.hook-title{position:absolute;left:192px;top:100px;font-size:116px}.hook-editor{position:absolute;left:192px;top:420px;width:1536px;height:520px;object-fit:cover;object-position:0 0;border-radius:12px}
.workspace{position:absolute;left:192px;top:350px;width:1536px;height:590px;object-fit:cover;object-position:50% 30%;border-radius:12px}
.brand{position:absolute;left:192px;top:130px;width:410px}.solution-title{position:absolute;left:192px;top:315px;font-size:102px}.category{position:absolute;left:192px;top:615px;font-size:40px;color:#354A62}.solution-workspace{position:absolute;left:1115px;top:115px;width:610px;height:830px;object-fit:cover;object-position:95% 50%;border-radius:12px}
.workflow-schema{position:absolute;left:192px;top:360px;width:510px;height:510px;object-fit:contain;border-radius:12px}.workflow-sql{position:absolute;left:748px;top:330px;width:980px;height:610px;object-fit:cover;object-position:0 0;border-radius:12px}
.settings{position:absolute;left:390px;top:300px;width:1140px;height:600px;object-fit:contain}.support{position:absolute;left:192px;top:934px;font-size:32px;color:#354A62}
.question{position:absolute;left:192px;top:235px;font-size:38px;color:#0B7480}.sql-proof{position:absolute;left:192px;top:380px;width:1536px;height:500px;object-fit:contain;object-position:0 0;border-radius:12px}
.result-proof{position:absolute;left:192px;top:330px;width:1536px;height:420px;object-fit:contain;object-position:0 0}.result-line{position:absolute;left:192px;top:810px;font-size:88px;letter-spacing:-2px}.result-line span{color:#0B7480}
.activity-proof{position:absolute;left:192px;top:295px;width:1536px;height:610px;object-fit:contain;object-position:0 0}.session-note{position:absolute;left:192px;top:932px;font-size:32px;color:#354A62}
.end-brand{position:absolute;left:192px;top:120px;width:360px}.cta-title{position:absolute;left:192px;top:290px;font-size:108px}.url{position:absolute;left:192px;top:590px;font-size:64px;color:#0B7480;letter-spacing:-1px}.cta-meta{position:absolute;left:192px;top:735px;font-size:36px}.platform{position:absolute;left:192px;top:820px;font-size:30px;color:#354A62}.soon{position:absolute;left:192px;top:872px;font-size:30px;color:#354A62}
.evidence{position:absolute;left:192px;bottom:37px;max-width:1536px;font-size:28px;line-height:1.2;color:#475D76}
"""

def build():
    (ROOT / "compositions").mkdir(exist_ok=True)
    (ROOT / "boards").mkdir(exist_ok=True)
    hosts = []
    for s in SHOTS:
        sid = s["id"]
        duration = s["end"] - s["start"]
        caption = "Native 0.8.17 · Idle SQL editor" if sid == "hook" else s["source"]
        scene = f'''<template><div id="{sid}" class="scene" data-composition-id="{sid}" data-width="1920" data-height="1080" data-duration="{duration}" data-no-timeline>
{s["body"]}<p class="evidence">Storyboard pose · {html.escape(caption)}</p></div>
<script>window.__timelines["{sid}"]=gsap.timeline({{paused:true}});</script></template>'''
        (ROOT / "compositions" / f"{sid}.html").write_text(scene)
        hosts.append(f'<div id="{sid}" class="clip" data-composition-id="{sid}" data-start="{s["start"]}" data-duration="{duration}" data-track-index="0" data-track-kind="graphics" data-composition-src="compositions/{sid}.html"></div>')
    doc = f'''<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=1920,height=1080"><title>TableIQ client launch v2 — storyboard poses</title><script src="https://cdn.jsdelivr.net/npm/gsap@3.15.0/dist/gsap.min.js"></script><style>{CSS}</style></head><body><div id="root" data-composition-id="main" data-width="1920" data-height="1080" data-duration="56" data-start="0" data-no-timeline>{''.join(hosts)}</div><script>window.__timelines["main"]=gsap.timeline({{paused:true}});</script></body></html>'''
    (ROOT / "index.html").write_text(doc)
    (ROOT / "shots.json").write_text(json.dumps(SHOTS, indent=2, ensure_ascii=False) + "\n")
    md = ['---', 'format: 1920x1080', 'duration: 56s', 'message: Free, open-source database client with optional BYOK AI and visible agent queries', 'arc: Hook → Problem → Solution → Proof → Payoff → MCP extension → Download', 'audience: Developers investigating databases on Mac', 'mode: autonomous', '---', '', 'Static poses only; no new animated film or voiceover has been rendered.', '']
    for i, s in enumerate(SHOTS, 1):
        mid = (s['start'] + s['end']) / 2
        md += [f'## Frame {i} — {s["title"]} ({s["start"]}–{s["end"]}s)', '', f'- act: {s["act"]}', '- status: built', f'- src: compositions/{s["id"]}.html', f'- duration: {s["end"] - s["start"]}s', f'- poster: {mid:g}s', f'- scene: {s["words"]}', f'- reads: “{s["title"]}” holds after landing; the real product action follows.', f'- sound: {s["sound"]}', '', s['action'], '', 'Evidence: ' + s['source'], '']
    (ROOT / "STORYBOARD.md").write_text('\n'.join(md))

if __name__ == "__main__":
    build()
