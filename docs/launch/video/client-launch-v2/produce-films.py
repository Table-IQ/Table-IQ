"""Build the three approved launch-film treatments from genuine captured app media.

All media lives inside each editable project. No product state is fabricated here.
Run after prepare-captures.py has populated production-media/ and voices/.
"""
from pathlib import Path
import html
import json
import shutil

ROOT = Path(__file__).resolve().parent
MEDIA = ROOT / "production-media"

# id, duration, title lines, capture asset, source crop kind, narration id
MOTION = [
    ("hook",3,["Still counting", "query tabs?"],"intro","hook",None),
    ("room",4,["Keep your investigation together"],"tabs","tabs",None),
    ("client",5,["Free. Open source.","Local first."],"tabstrip","brand",None),
    ("workflow",9,["Browse. Query. Review."],"browse","workspace",None),
    ("byok",6,["AI, with your own key"],"byok","settings",None),
    ("ask",10,["Ask. Review the SQL."],"ask","query",None),
    ("result",7,["You choose when to run"],"result","result",None),
    ("activity",6,["See your agent’s queries"],"mcp","activity",None),
    ("download",6,["Get the Mac", "developer preview"],None,"end",None),
]
WALK = [
    ("hook",3,["Still counting", "query tabs?"],"intro","hook","hook"),
    ("room",5.5,["Keep your investigation together"],"tabs","tabs","room"),
    ("client",5.5,["Free. Open source.","Local first."],"tabstrip","brand","client"),
    ("workflow",7,["Browse. Query. Review."],"browse","workspace","workflow"),
    ("byok",5,["AI, with your own key"],"byok","settings","byok"),
    ("ask",8,["Ask. Review the SQL."],"ask","query","ask"),
    ("result",8,["You choose when to run"],"result","result","result"),
    ("activity",7.5,["See your agent’s queries"],"mcp","activity","activity"),
    ("download",7.5,["Get the Mac", "developer preview"],None,"end","download"),
]
STORY = [
    ("hook",5.5,["Paid.","Still unshipped."],"result","story-hook","story-hook"),
    ("client",7.5,["Keep the investigation together"],"tabstrip","brand","story-client"),
    ("workflow",7,["Browse. Query. Review."],"browse","workspace","workflow"),
    ("byok",7,["AI, with your own key"],"byok","settings","story-ai"),
    ("ask",8,["Which paid orders", "have no shipped shipment?"],"ask","query","ask"),
    ("result",8,["Two orders to investigate"],"result","result","story-result"),
    ("activity",6,["See your agent’s queries"],"mcp","activity","story-mcp"),
    ("download",8.5,["Get the Mac", "developer preview"],None,"end","download"),
]

CSS = """
@font-face{font-family:Instrument;src:url('assets/fonts/instrument-sans-latin-500-normal.woff2');font-weight:500}
@font-face{font-family:Instrument;src:url('assets/fonts/instrument-sans-latin-600-normal.woff2');font-weight:600}
@font-face{font-family:Plex;src:url('assets/fonts/ibm-plex-mono-latin-400-normal.woff2');font-weight:400}
*{box-sizing:border-box}html,body{margin:0;width:1920px;height:1080px;overflow:hidden;background:var(--ground,#F1F6FB);color:var(--ink,#0C1A2B);font-family:Instrument,system-ui,sans-serif;font-weight:500}
#root{position:relative;width:100%;height:100%;overflow:hidden}.scene{position:relative;width:100%;height:100%;overflow:hidden;background:var(--ground,#F1F6FB)}
h1,p{margin:0}h1{font-size:88px;font-weight:600;line-height:1.05;letter-spacing:-2.5px}h1 span{display:block}.signal{color:var(--signal,#0B7480)}
.title-mask{position:absolute;left:192px;top:94px;width:1536px;overflow:hidden;padding:6px 0}.heading{max-width:1536px}.two-lines .title-mask{top:60px}.two-lines h1{font-size:80px}
.panel{position:absolute;left:128px;top:274px;width:1664px;height:682px;overflow:hidden;border-radius:12px;background:#0C1A2B}.screen{position:absolute;inset:0;width:100%;height:100%;object-fit:cover;object-position:top left;transform-origin:50% 0}.camera{position:absolute;inset:0;overflow:hidden;transform-origin:50% 0}
.note{position:absolute;left:192px;bottom:36px;font-size:28px;line-height:1.2;color:#475D76;max-width:1536px}.support{position:absolute;left:192px;top:220px;font-size:34px;color:#354A62}.two-lines .support{top:245px}.two-lines .panel{top:310px;height:640px}
.hook .title-mask{top:100px}.hook h1{font-size:116px;line-height:1.02}.hook .panel{top:440px;height:510px}.hook .heading span:last-child{color:var(--signal,#0B7480)}
.brand .title-mask{left:192px;top:260px;width:1536px}.brand h1{font-size:100px}.brand .brand-mark{position:absolute;left:192px;top:112px;width:380px}.brand .panel{left:192px;top:690px;width:1536px;height:210px;background:transparent}.brand .screen{object-fit:contain;object-position:50% 50%}.brand .category{position:absolute;left:192px;top:540px;font-size:44px;color:#354A62;max-width:1200px}.brand .support{top:610px;width:1200px}
.settings .panel{left:192px;top:280px;width:1536px;height:650px;background:transparent}.settings .screen{object-fit:contain}.settings .support{top:210px;color:#475D76}
.query .panel{top:340px;height:610px}.query .support{top:225px;color:#0B7480}.query.two-lines .panel{top:350px;height:600px}.query.two-lines .support{display:none}
.query .sql-transcript{position:absolute;left:192px;top:316px;width:1536px;margin:0;font:400 30px/35px Plex,monospace;white-space:pre;color:#0C1A2B}.query .transcript-label{position:absolute;right:192px;top:224px;font-size:40px;color:#475D76}.query.two-lines .sql-transcript{top:330px}.query.two-lines .transcript-label{top:250px}.sql-signal{color:#0B7480;font-weight:400}
.query .clause{position:absolute;top:345px;width:728px}.query .clause-paid{left:192px}.query .clause-unshipped{left:1000px}.query .clause h2{font-size:48px;line-height:1.1;font-weight:500;letter-spacing:-1px;margin:0 0 54px}.query .clause pre{margin:0;font:400 34px/48px Plex,monospace;white-space:pre;color:#0C1A2B}.query.two-lines .clause{top:360px}.query .negation{color:#0C1A2B}
.result .panel{left:192px;top:305px;width:1536px;height:500px}.result .screen{object-fit:contain;object-position:top left}.result .result-total{position:absolute;left:192px;top:830px;font-size:70px;letter-spacing:-1.5px;color:#0B7480}.result .note{bottom:32px}
.result .still-label{position:absolute;left:192px;top:220px;font-size:48px;color:#475D76}
.activity .panel{left:128px;top:300px;width:1664px;height:640px}.activity .screen{object-fit:contain;object-position:top left}.activity .support{top:210px;color:#475D76}
.end .brand-mark{position:absolute;left:192px;top:110px;width:350px}.end .title-mask{top:290px;width:1536px}.end h1{font-size:108px}.end .url{position:absolute;left:192px;top:590px;font-size:64px;color:#0B7480;letter-spacing:-1px}.end .meta{position:absolute;left:192px;top:745px;font-size:34px}.end .platform{position:absolute;left:192px;top:825px;font-size:30px;color:#354A62}.end .soon{position:absolute;left:192px;top:877px;font-size:30px;color:#354A62}
.story-mode.hook .title-mask{top:110px}.story-mode.hook h1{font-size:116px}.story-mode.hook .panel{left:1000px;top:340px;width:760px;height:350px;background:transparent}.story-mode.hook .screen{object-fit:contain;object-position:center}.story-mode.hook .title-mask{width:800px}.story-mode.hook .story-question{position:absolute;left:192px;top:525px;font-size:44px;color:#354A62;max-width:750px}.story-mode.hook .hook-still-label{position:absolute;left:1000px;top:280px;font-size:36px;color:#475D76}.story-mode.brand .title-mask{width:1536px}.story-mode.brand h1{font-size:90px}.story-mode.brand .category{top:510px}.story-mode.brand .support{top:585px}
"""

def build(name, shots):
    project = ROOT / name
    assets = project / "assets"
    assets.mkdir(exist_ok=True)
    (project / "compositions").mkdir(exist_ok=True)
    (assets / "fonts").mkdir(exist_ok=True)
    for f in (ROOT / "assets/fonts").iterdir():
        shutil.copy2(f, assets / "fonts" / f.name)
    shutil.copy2(ROOT / "assets/tableiq-logo-horizontal-light.svg", assets / "wordmark.svg")
    capture_manifest = json.loads((MEDIA / "manifest.json").read_text())
    for filename in ["intro.mp4","tabs.mp4","tabstrip.mp4","browse.mp4","byok.mp4","ask.mp4","result.mp4","mcp.mp4","launch-score.wav"]:
        shutil.copy2(MEDIA / filename, assets / filename)
    hosts, audio, manifest = [], [], []
    voice_lengths = {v["id"]:v["duration"] for v in json.loads((MEDIA / "voices/durations.json").read_text())}
    offset = 0.0
    for number, (sid,dur,lines,capture,kind,voice) in enumerate(shots):
        prefix = f"{name}-{sid}"
        src_note = capture_manifest.get(capture,{}).get("caption", "")
        heading = '<h1 id="'+prefix+'-heading" class="heading">'+''.join('<span>'+html.escape(x)+'</span>' for x in lines)+'</h1>'
        body = '<div class="title-mask">'+heading+'</div>'
        if capture and kind != "query":
            body += f'<div class="panel" id="{prefix}-panel"><div class="camera" id="{prefix}-camera"><video id="{prefix}-screen" class="clip screen" src="assets/{capture}.mp4" data-start="0" data-duration="{dur}" data-track-index="1" muted playsinline></video></div></div>'
        if kind == "brand":
            body += '<img id="'+prefix+'-mark" class="brand-mark" src="assets/wordmark.svg" alt="TableIQ"><p class="category">A database client for Mac</p>'
            if name=="story": body += '<p class="support">Free and open source · Local first</p>'
        elif kind == "settings":
            body += '<p class="support">Optional AI · Provider API charges apply</p>'
        elif kind == "query":
            paid="WHERE EXISTS (\n  SELECT 1\n  FROM main.payments AS p\n  WHERE p.order_id = o.id\n    AND p.status = 'paid'\n)"
            unshipped="AND NOT EXISTS (\n  SELECT 1\n  FROM main.shipments AS s\n  WHERE s.order_id = o.id\n    AND s.status = 'shipped'\n)"
            original=(MEDIA/'native-generated.sql').read_text()
            assert all(' '.join(x.split()) in ' '.join(original.split()) for x in [paid,unshipped]), 'SQL excerpt differs from actual native output'
            pcode=html.escape(paid).replace("p.status = &#x27;paid&#x27;",'<span class="sql-signal">p.status = &#x27;paid&#x27;</span>')
            scode=html.escape(unshipped).replace('AND NOT EXISTS',f'<span id="{prefix}-negation" class="negation">AND NOT EXISTS</span>').replace("s.status = &#x27;shipped&#x27;",f'<span id="{prefix}-shipped">s.status = &#x27;shipped&#x27;</span>')
            body += '<p class="support">Paid payments. No shipped shipment.</p><p class="transcript-label">Verified SQL excerpts</p>'
            body += f'<div class="clause clause-paid"><h2>A paid payment exists</h2><pre>{pcode}</pre></div><div id="{prefix}-unshipped" class="clause clause-unshipped"><h2>No shipped shipment</h2><pre>{scode}</pre></div>'
            src_note="Excerpts from the verified native AI output · Full SQL in source"
        elif kind == "result":
            body += f'<p class="still-label">Native result still · After Run</p><p id="{prefix}-total" class="result-total">2 orders · 7,300 cents</p>'
        elif kind == "activity":
            body += '<p class="support">Local MCP client · Separate sample session</p>'
        elif kind == "story-hook":
            body += '<p class="story-question">Which orders need your attention?</p><p class="hook-still-label">Verified sample result · Still</p>'
        elif kind == "end":
            body += f'<img id="{prefix}-mark" class="brand-mark" src="assets/wordmark.svg" alt="TableIQ"><p id="{prefix}-url" class="url">tableiq.ideolyst.com</p><p class="meta">Built with Rust · No TableIQ account required</p><p class="platform">Mac Apple silicon · Unsigned / not notarized</p><p class="soon">Windows and Linux coming soon</p>'
        if capture: body += '<p class="note">'+html.escape(src_note)+'</p>'
        classes = ["scene",kind]
        if sid=="hook":classes.append("hook")
        if len(lines)>1 and sid!="hook" and kind not in ["brand","end"]:classes.append("two-lines")
        if name=="story":classes.append("story-mode")
        speed = 0.35 if name=="motion" else 0.5
        motion = [f'const tl=gsap.timeline({{paused:true}});']
        # First frame already carries the hook. Subsequent titles reveal once and hold.
        if sid != "hook":
            if number % 3 == 0:
                motion.append(f'tl.fromTo("#{prefix}-heading",{{clipPath:"inset(0 100% 0 0)"}},{{clipPath:"inset(0 0% 0 0)",duration:{speed},ease:"power2.out"}},0.08);')
            elif number % 3 == 1:
                motion.append(f'tl.fromTo("#{prefix}-heading",{{x:140}},{{x:0,duration:{speed},ease:"power3.out"}},0);')
            else:
                motion.append(f'tl.fromTo("#{prefix}-heading",{{scale:0.94,opacity:0.2}},{{scale:1,opacity:1,duration:{speed},ease:"power3.out",transformOrigin:"0% 50%"}},0);')
        if capture and kind != "query":
            if kind in ["tabs","workspace","query"]:
                zoom = 1.08 if kind=="workspace" else 1.025
                motion.append(f'tl.fromTo("#{prefix}-camera",{{scale:1}},{{scale:{zoom},duration:0.6,ease:"power3.inOut"}},1.2);')
                if dur > 7: motion.append(f'tl.to("#{prefix}-camera",{{scale:1,duration:0.55,ease:"power3.inOut"}},{dur-1.1});')
            elif sid=="hook":
                motion.append(f'tl.fromTo("#{prefix}-panel",{{x:120}},{{x:0,duration:0.5,ease:"power3.out"}},0);')
            else:
                motion.append(f'tl.fromTo("#{prefix}-panel",{{clipPath:"inset(0 0 100% 0)"}},{{clipPath:"inset(0 0 0% 0)",duration:0.45,ease:"power3.out"}},0.15);')
        if kind in ["brand","end"]:
            motion.append(f'tl.fromTo("#{prefix}-mark",{{clipPath:"inset(0 100% 0 0)"}},{{clipPath:"inset(0 0% 0 0)",duration:0.45,ease:"power3.out"}},0.05);')
        if kind=="result":
            motion.append(f'tl.fromTo("#{prefix}-total",{{opacity:0,y:24}},{{opacity:1,y:0,duration:0.4,ease:"power3.out"}},2.6);')
        if kind=="query":
            motion.append(f'tl.fromTo("#{prefix}-unshipped",{{opacity:0,y:24}},{{opacity:1,y:0,duration:0.4,ease:"power3.out"}},2.2);')
            motion.append(f'tl.to("#{prefix}-negation",{{color:"#0B7480",duration:0.25}},4.9);')
            motion.append(f'tl.to("#{prefix}-shipped",{{color:"#0B7480",duration:0.25}},6.9);')
        if kind=="end":
            motion.append(f'tl.fromTo("#{prefix}-url",{{opacity:0}},{{opacity:1,duration:0.3}},0.6);')
        motion.append(f'window.__timelines["{prefix}"]=tl;')
        scene = f'<template><div id="{prefix}" class="{" ".join(classes)}" data-composition-id="{prefix}" data-width="1920" data-height="1080" data-duration="{dur}">{body}</div><script>{"".join(motion)}</script></template>'
        (project / "compositions" / f"{sid}.html").write_text(scene)
        hosts.append(f'<div id="{prefix}" class="clip" data-composition-id="{prefix}" data-composition-src="compositions/{sid}.html" data-start="{offset}" data-duration="{dur}" data-track-index="0" data-track-kind="graphics" data-beat="{sid}"></div>')
        if voice:
            shutil.copy2(MEDIA / "voices" / f"{voice}.wav", assets / f"voice-{sid}.wav")
            speech = voice_lengths[voice]
            assert speech + 0.2 < dur, f"Voice does not fit {name}/{sid}"
            audio.append(f'<audio id="{prefix}-voice" class="clip" src="assets/voice-{sid}.wav" data-start="{offset+0.2}" data-duration="{speech}" data-track-index="10" data-volume="1"></audio>')
        manifest.append(dict(id=sid,start=offset,duration=dur,title=lines,capture=capture,voice=voice,environment=src_note))
        offset += dur
    # Reused original TableIQ launch score. Narrated versions duck the bed in the final mix.
    audio.append(f'<audio id="{name}-music" class="clip" src="assets/launch-score.wav" data-start="0" data-duration="{offset}" data-track-index="12" data-volume="{0.7 if name=="motion" else 0.14}"></audio>')
    vars_ = html.escape(json.dumps([{"id":"ground","type":"color","label":"Background","default":"#F1F6FB"},{"id":"ink","type":"color","label":"Text","default":"#0C1A2B"},{"id":"signal","type":"color","label":"Accent","default":"#0B7480"}]),quote=True)
    doc = f'<!doctype html><html lang="en" data-composition-variables="{vars_}"><head><meta charset="utf-8"><meta name="viewport" content="width=1920,height=1080"><title>TableIQ {name} launch film</title><script src="https://cdn.jsdelivr.net/npm/gsap@3.15.0/dist/gsap.min.js"></script><style>{CSS}</style></head><body><div id="root" data-composition-id="main" data-width="1920" data-height="1080" data-duration="{offset}" data-fps="30" data-start="0">{"".join(hosts)}{"".join(audio)}</div><script>window.__timelines["main"]=gsap.timeline({{paused:true}});</script></body></html>'
    (project / "index.html").write_text(doc)
    (project / "film-plan.json").write_text(json.dumps(dict(treatment=name,duration=offset,shots=manifest),indent=2,ensure_ascii=False)+'\n')
    board=['---','format: 1920x1080',f'duration: {offset}s','message: Free database client first; optional BYOK AI and visible MCP calls','arc: Hook → Problem → Solution → Proof → Result → MCP → Download','audience: Developers using Mac database clients','mode: autonomous','---','','AI proof uses the actual verified native SQL and result. SQL is transcribed as an editorial graphic; result stills are identified. Client/settings/MCP actions are real development captures.','']
    for i,s in enumerate(manifest,1):
        title=' '.join(s['title'])
        board += [f"## Frame {i} — {title} ({s['start']:g}–{s['start']+s['duration']:g}s)",'','- status: animated',f"- src: compositions/{s['id']}.html",f"- duration: {s['duration']}s",f"- poster: {s['start']+s['duration']/2:g}s",f'- scene: {title}',f"- reads: “{title}” (0.4–{s['duration']-0.2:g}s); the real task content holds below.",f"- sound: {'Narration and music' if s['voice'] else 'Music'}",'',s['environment'] or 'Supplied brand and verified platform metadata.','']
    (project / 'STORYBOARD.md').write_text('\n'.join(board))
    (project / ".gitignore").write_text('node_modules/\n.hyperframes/\n.thumbnails/\n.argo/\n*.mp4\nassets/*.wav\n')
    print(name, offset)

if __name__ == "__main__":
    for name,shots in [("motion",MOTION),("story",STORY),("walkthrough",WALK)]:
        build(name,shots)
