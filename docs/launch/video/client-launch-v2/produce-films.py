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
    ("hook",3,["Still counting", "query tabs?"],None,"hook",None),
    ("room",4,["Keep your investigation", "together."],None,"tabs",None),
    ("client",5,["Free. Open source.","Local first."],None,"brand",None),
    ("workflow",9,["Browse. Query. Review."],"browse","workspace",None),
    ("byok",6,["AI, with your own key"],None,"settings",None),
    ("ask",10,["Ask. Review the SQL."],"ask","query",None),
    ("result",7,["You choose when to run"],"result","result",None),
    ("activity",6,["See your agent’s queries"],"mcp","activity",None),
    ("download",6,["Get the Mac", "developer preview"],None,"end",None),
]
WALK = [
    ("hook",3,["Still counting", "query tabs?"],None,"hook","hook"),
    ("room",5.5,["Keep your investigation", "together."],None,"tabs","room"),
    ("client",5.5,["Free. Open source.","Local first."],None,"brand","client"),
    ("workflow",7,["Browse. Query. Review."],"browse","workspace","workflow"),
    ("byok",5,["AI, with your own key"],None,"settings","byok"),
    ("ask",8,["Ask. Review the SQL."],"ask","query","ask"),
    ("result",8,["You choose when to run"],"result","result","result"),
    ("activity",7.5,["See your agent’s queries"],"mcp","activity","activity"),
    ("download",7.5,["Get the Mac", "developer preview"],None,"end","download"),
]
STORY = [
    ("hook",5.5,["Paid.","Still unshipped."],None,"story-hook","story-hook"),
    ("client",7.5,["Keep the investigation", "together."],None,"brand","story-client"),
    ("workflow",7,["Browse. Query. Review."],"browse","workspace","workflow"),
    ("byok",7,["AI, with your own key"],None,"settings","story-ai"),
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
#root{position:relative;width:100%;height:100%;overflow:hidden}.scene,.stage{position:relative;width:100%;height:100%;overflow:hidden;background:var(--ground,#F1F6FB)}
h1,p{margin:0}h1{font-size:88px;font-weight:600;line-height:1.06;letter-spacing:-2.5px}h1 span{display:block}.signal{color:var(--signal,#0B7480)}
.title-mask{position:absolute;left:192px;top:106px;width:1536px;overflow:hidden;padding:8px 0}.heading{max-width:1536px}.two-lines .title-mask{top:70px}.two-lines h1{font-size:80px}
.panel{position:absolute;left:96px;top:282px;width:1728px;height:680px;overflow:hidden;background:#0C1A2B}.screen{position:absolute;inset:0;width:100%;height:100%;object-fit:cover;object-position:top left}.camera{position:absolute;inset:0;transform-origin:0% 0%}
.note{position:absolute;left:192px;bottom:36px;font-size:28px;line-height:1.2;color:#475D76;max-width:1536px}.support{position:absolute;left:192px;top:222px;font-size:34px;color:#354A62}.kicker{position:absolute;left:192px;top:130px;font-size:32px;color:#475D76;letter-spacing:1px}
.hook .title-mask{top:280px}.hook h1{font-size:144px;line-height:1.02;letter-spacing:-5px}.hook .heading span:last-child{color:var(--signal,#0B7480)}.hook .hook-deck{position:absolute;left:200px;top:700px;font-size:44px;color:#354A62}
.tabs .title-mask{top:284px}.tabs h1{font-size:116px;line-height:1.16;letter-spacing:-4px}.tabs .heading span:last-child{color:#0B7480;font-size:144px}.tabs .context-line{position:absolute;left:200px;top:725px;font-size:44px;color:#354A62}
.brand .brand-mark{position:absolute;left:192px;top:160px;width:620px}.brand .title-mask{top:420px}.brand h1{font-size:116px;line-height:1.12;letter-spacing:-4px}.brand .heading span:last-child{color:#0B7480}.brand .category{position:absolute;left:198px;top:785px;font-size:46px;color:#354A62}.brand .support{left:198px;top:860px;font-size:34px}
.settings .title-mask{top:130px}.settings h1{font-size:100px}.settings .byok-type{position:absolute;left:182px;top:360px;font-size:242px;line-height:1;letter-spacing:-14px;font-weight:600;color:#0B7480}.settings .byok-expansion{position:absolute;left:198px;top:650px;font-size:46px}.settings .byok-optional{position:absolute;left:1110px;top:420px;font-size:64px;line-height:1.14;max-width:620px}.settings .byok-optional span{display:block}.settings .support{top:820px;font-size:36px;color:#475D76}
.query .support{top:235px;color:#0B7480}.query .transcript-label{position:absolute;right:192px;top:235px;font-size:34px;color:#475D76}.query .clause{position:absolute;top:370px;width:728px}.query .clause-paid{left:192px}.query .clause-unshipped{left:1000px}.query .clause h2{font-size:46px;line-height:1.1;font-weight:500;letter-spacing:-1px;margin:0 0 48px}.query .clause pre{margin:0;font:400 34px/48px Plex,monospace;white-space:pre;color:#0C1A2B}.sql-signal{color:#0B7480;font-weight:400}.query .negation{color:#0C1A2B}.query.two-lines .support{display:none}.query.two-lines .transcript-label{top:270px}.query.two-lines .clause{top:380px}
.result .panel{left:192px;top:325px;width:1536px;height:500px}.result .screen{object-fit:contain;object-position:top left}.result .result-total{position:absolute;left:192px;top:850px;font-size:76px;letter-spacing:-2px;color:#0B7480}.result .still-label{position:absolute;left:192px;top:235px;font-size:36px;color:#475D76}.result .note{bottom:30px}
.workspace .screen,.activity .screen{height:auto;object-fit:contain}.activity .panel{top:315px;height:640px}.activity .screen{object-fit:contain;object-position:top left}.activity .support{top:235px;color:#475D76}
.end .brand-mark{position:absolute;left:192px;top:130px;width:460px}.end .title-mask{top:330px}.end h1{font-size:110px}.end .url{position:absolute;left:192px;top:620px;font-size:72px;color:#0B7480;letter-spacing:-2px}.end .meta{position:absolute;left:192px;top:790px;font-size:34px}.end .platform{position:absolute;left:192px;top:855px;font-size:30px;color:#354A62}.end .soon{position:absolute;left:192px;top:905px;font-size:30px;color:#354A62}
.story-mode.hook .title-mask{top:270px}.story-mode.hook h1{font-size:144px}.story-mode.hook .story-question{position:absolute;left:200px;top:700px;font-size:48px;color:#354A62}.story-mode.brand h1{font-size:106px}.story-mode.brand .category{top:760px}.story-mode.brand .support{top:835px}
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
        if kind == "hook":
            body += '<p class="kicker">A better place for your database work</p><p class="hook-deck">A tab limit should not interrupt an investigation.</p>'
        elif kind == "tabs":
            body += '<p class="kicker">One investigation. Room to work.</p><p class="context-line">Tables. Schema. SQL.</p>'
        if capture and kind != "query":
            body += f'<div class="panel" id="{prefix}-panel"><div class="camera" id="{prefix}-camera" data-layout-allow-overflow><video id="{prefix}-screen" class="clip screen" src="assets/{capture}.mp4" data-start="0" data-duration="{dur}" data-track-index="1" muted playsinline></video></div></div>'
        if kind == "brand":
            body += '<img id="'+prefix+'-mark" class="brand-mark" src="assets/wordmark.svg" alt="TableIQ"><p class="category">A database client for Mac</p>'
            if name=="story": body += '<p class="support">Free and open source · Local first</p>'
        elif kind == "settings":
            body += f'<p id="{prefix}-byok" class="byok-type">BYOK</p><p class="byok-expansion">Bring your own key</p><p id="{prefix}-optional" class="byok-optional"><span>Your provider.</span><span>When you want AI.</span></p><p class="support">AI is optional · Provider API charges apply</p>'
        elif kind == "query":
            paid="WHERE EXISTS (\n  SELECT 1\n  FROM main.payments AS p\n  WHERE p.order_id = o.id\n    AND p.status = 'paid'\n)"
            unshipped="AND NOT EXISTS (\n  SELECT 1\n  FROM main.shipments AS s\n  WHERE s.order_id = o.id\n    AND s.status = 'shipped'\n)"
            original=(MEDIA/'native-generated.sql').read_text()
            assert all(' '.join(x.split()) in ' '.join(original.split()) for x in [paid,unshipped]), 'SQL excerpt differs from actual native output'
            pcode=html.escape(paid).replace("p.status = &#x27;paid&#x27;",'<span class="sql-signal">p.status = &#x27;paid&#x27;</span>')
            scode=html.escape(unshipped).replace('AND NOT EXISTS',f'<span id="{prefix}-negation" class="negation">AND NOT EXISTS</span>').replace("s.status = &#x27;shipped&#x27;",f'<span id="{prefix}-shipped">s.status = &#x27;shipped&#x27;</span>')
            body += '<p class="support">Paid payments. No shipped shipment.</p><p class="transcript-label">Verified SQL excerpts</p>'
            body += f'<div class="clause clause-paid"><h2>A paid payment exists</h2><pre>{pcode}</pre></div><div id="{prefix}-unshipped" class="clause clause-unshipped"><h2>No shipped shipment</h2><pre>{scode}</pre></div>'
            src_note="Verified native v0.8.17 SQL excerpts · Full SQL in source"
        elif kind == "result":
            body += f'<p class="still-label">Native v0.8.17 result still · After Run</p><p id="{prefix}-total" class="result-total">2 orders · 7,300 cents</p>'
        elif kind == "activity":
            body += '<p class="support">Local MCP client · Separate sample session</p>'
        elif kind == "story-hook":
            body += '<p class="kicker">A synthetic storefront investigation</p><p class="story-question">Which orders need your attention?</p>'
        elif kind == "end":
            body += f'<img id="{prefix}-mark" class="brand-mark" src="assets/wordmark.svg" alt="TableIQ"><p id="{prefix}-url" class="url" data-hold>tableiq.ideolyst.com</p><p class="meta">Built with Rust · No TableIQ account required</p><p class="platform">Mac Apple silicon · Unsigned / not notarized</p><p class="soon">Windows and Linux coming soon</p>'
        if kind == "result": src_note = "Verified native AI example · After manual Run · Still"
        if capture: body += '<p class="note">'+html.escape(src_note)+'</p>'
        classes = ["scene",kind]
        if sid=="hook":classes.append("hook")
        if len(lines)>1 and sid!="hook" and kind not in ["brand","end"]:classes.append("two-lines")
        if name=="story":classes.append("story-mode")
        # Directional type handoffs adapt the registry text-match-cut principle:
        # outgoing accelerates, incoming continues the direction and settles.
        # Proof panels appear in full; no wiping across evidence or fake UI clicks.
        motion = ['const tl=gsap.timeline({paused:true});']
        if kind == "hook" or kind == "story-hook":
            motion.append(f'tl.to("#{prefix}-stage",{{y:-72,duration:0.24,ease:"power2.in"}},{dur-0.24});')
        elif kind == "tabs":
            motion.append(f'tl.fromTo("#{prefix}-stage",{{y:72}},{{y:0,duration:0.34,ease:"power2.out"}},0);')
            motion.append(f'tl.to("#{prefix}-stage",{{x:-100,duration:0.24,ease:"power2.in"}},{dur-0.24});')
        elif kind == "brand":
            axis = 'y' if name == 'story' else 'x'
            motion.append(f'tl.fromTo("#{prefix}-stage",{{{axis}:100}},{{{axis}:0,duration:0.38,ease:"power2.out"}},0);')
            motion.append(f'tl.fromTo("#{prefix}-mark",{{opacity:0}},{{opacity:1,duration:0.28}},0);')
            motion.append(f'tl.to("#{prefix}-heading",{{y:-48,duration:0.24,ease:"power2.in"}},{dur-0.24});')
        elif kind == "settings":
            motion.append(f'tl.fromTo("#{prefix}-heading",{{y:70}},{{y:0,duration:0.42,ease:"power3.out"}},0);')
            motion.append(f'tl.fromTo("#{prefix}-byok",{{x:-120,opacity:0}},{{x:0,opacity:1,duration:0.5,ease:"power3.out"}},0.15);')
            motion.append(f'tl.fromTo("#{prefix}-optional",{{opacity:0}},{{opacity:1,duration:0.35}},0.8);')
            motion.append(f'tl.to("#{prefix}-heading",{{x:-60,duration:0.24,ease:"power2.in"}},{dur-0.24});')
        elif kind == "query":
            motion.append(f'tl.fromTo("#{prefix}-heading",{{x:60}},{{x:0,duration:0.34,ease:"power2.out"}},0);')
            motion.append(f'tl.fromTo("#{prefix}-unshipped",{{opacity:0,x:50}},{{opacity:1,x:0,duration:0.4,ease:"power3.out"}},2.2);')
            motion.append(f'tl.to("#{prefix}-negation",{{color:"#0B7480",duration:0.25}},4.9);')
            motion.append(f'tl.to("#{prefix}-shipped",{{color:"#0B7480",duration:0.25}},6.9);')
        elif kind == "workspace":
            motion.append(f'tl.fromTo("#{prefix}-heading",{{y:48}},{{y:0,duration:0.36,ease:"power2.out"}},0);')
            # One wide source crop: the same surface stays alive across all moves.
            # Source state windows: order selection 0s, Structure ~1.8s, ERD ~3.4s.
            motion.append(f'tl.to("#{prefix}-camera",{{scale:1.4,x:0,y:0,duration:0.42,ease:"power3.inOut"}},0.12);')
            motion.append(f'tl.to("#{prefix}-camera",{{scale:1,x:0,y:0,duration:0.32,ease:"power3.inOut"}},1.35);')
            motion.append(f'tl.to("#{prefix}-camera",{{scale:1.24,x:0,y:0,duration:0.38,ease:"power3.inOut"}},1.8);')
            motion.append(f'tl.to("#{prefix}-camera",{{scale:1,x:0,y:0,duration:0.3,ease:"power3.inOut"}},2.95);')
            # First show schema context, then move into the relevant relationships.
            motion.append(f'tl.to("#{prefix}-camera",{{scale:1,x:0,y:-200,duration:0.36,ease:"power3.inOut"}},3.4);')
            motion.append(f'tl.to("#{prefix}-camera",{{scale:1.55,x:-120,y:-640,duration:0.6,ease:"power3.inOut"}},4.05);')
            motion.append(f'tl.to("#{prefix}-camera",{{scale:1,x:0,y:-200,duration:0.5,ease:"power3.inOut"}},{dur-1.1});')
        elif kind == "activity":
            # Source expanded mark 2.959 - selected start 1.359 = local 1.6s.
            # Focus the actual expanded Tool/SQL/Rows/Status region, then return.
            motion.append(f'tl.to("#{prefix}-camera",{{scale:1.35,x:-660,y:-70,duration:0.55,ease:"power3.inOut"}},1.6);')
            motion.append(f'tl.to("#{prefix}-camera",{{scale:1,x:0,y:0,duration:0.5,ease:"power3.inOut"}},{dur-1.1});')
        elif kind == "result":
            motion.append(f'tl.fromTo("#{prefix}-total",{{opacity:0,y:30}},{{opacity:1,y:0,duration:0.45,ease:"power3.out"}},2.6);')
        elif kind == "end":
            motion.append(f'tl.fromTo("#{prefix}-mark",{{x:60,opacity:0}},{{x:0,opacity:1,duration:0.45,ease:"power3.out"}},0);')
            motion.append(f'tl.fromTo("#{prefix}-url",{{opacity:0}},{{opacity:1,duration:0.3}},0.6);')
        # Additional text arrives on the narration's semantic beats, then holds.
        if kind == "story-hook":
            motion.append(f'tl.fromTo("#{prefix}-heading span:nth-child(2)",{{opacity:0,y:30}},{{opacity:1,y:0,duration:0.4,ease:"power3.out"}},1.25);')
            motion.append(f'tl.fromTo("#{prefix} .story-question",{{opacity:0}},{{opacity:1,duration:0.3}},3.3);')
        elif kind == "tabs":
            motion.append(f'tl.fromTo("#{prefix} .context-line",{{opacity:0}},{{opacity:1,duration:0.3}},2.0);')
        elif kind == "brand":
            motion.append(f'tl.fromTo("#{prefix}-heading span:nth-child(2)",{{opacity:0}},{{opacity:1,duration:0.3}},1.35);')
            category_at = 3.2 if name == "story" else 2.5
            motion.append(f'tl.fromTo("#{prefix} .category",{{opacity:0}},{{opacity:1,duration:0.3}},{category_at});')
            if name == "story": motion.append(f'tl.fromTo("#{prefix} .support",{{opacity:0}},{{opacity:1,duration:0.3}},4.4);')
        elif kind == "settings":
            motion.append(f'tl.fromTo("#{prefix} .support",{{opacity:0}},{{opacity:1,duration:0.3}},{dur-3});')
        body = f'<div id="{prefix}-stage" class="stage" data-layout-allow-overflow>{body}</div>'
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
    board=['---','format: 1920x1080',f'duration: {offset}s','message: Free database client first; optional BYOK AI and visible MCP calls','arc: Hook → Problem → Solution → Proof → Result → MCP → Download','audience: Developers using Mac database clients','mode: autonomous','---','','AI proof uses the actual verified native SQL and result. SQL is transcribed as an editorial graphic; result stills are identified. Only client workflow and MCP use real development recordings; hook, problem, brand and BYOK are editorial typography. Native results remain labelled stills. Camera punch-ins follow selected source actions; no clicks or cursor effects are simulated.','']
    for i,s in enumerate(manifest,1):
        title=' '.join(s['title'])
        board += [f"## Frame {i} — {title} ({s['start']:g}–{s['start']+s['duration']:g}s)",'','- status: animated',f"- src: compositions/{s['id']}.html",f"- duration: {s['duration']}s",f"- poster: {s['start']+s['duration']/2:g}s",f'- scene: {title}',f"- reads: “{title}” (0.4–{s['duration']-0.2:g}s); the real task content holds below.",f"- sound: {'Narration and music' if s['voice'] else 'Music'}",'',s['environment'] or 'Supplied brand and verified platform metadata.','']
    (project / 'STORYBOARD.md').write_text('\n'.join(board))
    (project / ".gitignore").write_text('node_modules/\n.hyperframes/\n.thumbnails/\n.argo/\n*.mp4\nassets/*.wav\n')
    print(name, offset)

if __name__ == "__main__":
    for name,shots in [("motion",MOTION),("story",STORY),("walkthrough",WALK)]:
        build(name,shots)
