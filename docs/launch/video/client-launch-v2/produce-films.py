"""Build the three approved 75-second launch films from genuine product evidence.

Shared production-media is owned by the capture/audio pipeline. This generator
copies those assets but never invents app states, mouse activity, or AI output.
Run again after new capture/voice assets arrive; historical reviews are retained.
"""
from pathlib import Path
import html
import json
import shutil

ROOT = Path(__file__).resolve().parent
MEDIA = ROOT / 'production-media'
SHOTS = [
    ('hook', 8, ['Hitting TablePlus’s', 'two-tab free limit?'], None),
    ('offer', 10, ['Free. Open source.', 'A database client for Mac.'], 'tabs'),
    ('client', 9, ['Coming from DBeaver?'], 'browse'),
    ('investigation', 16, ['Which paid orders haven’t shipped?'], 'manual-query'),
    ('ai', 13, ['Want help writing SQL?'], 'ask'),
    ('activity', 9, ['See your coding agent’s queries.'], 'mcp'),
    ('download', 10, ['Get the free Mac', 'developer preview.'], None),
]
# Public constants retained for tooling that imports the generator.
MOTION = STORY = WALK = SHOTS

CSS = """
@font-face{font-family:Instrument;src:url('assets/fonts/instrument-sans-latin-500-normal.woff2');font-weight:500}
@font-face{font-family:Instrument;src:url('assets/fonts/instrument-sans-latin-600-normal.woff2');font-weight:600}
@font-face{font-family:Plex;src:url('assets/fonts/ibm-plex-mono-latin-400-normal.woff2');font-weight:400}
*{box-sizing:border-box}html,body{margin:0;width:1920px;height:1080px;overflow:hidden;background:var(--ground,#F1F6FB);color:var(--ink,#0C1A2B);font-family:Instrument,system-ui,sans-serif;font-weight:500}
#root{position:relative;width:100%;height:100%;overflow:hidden}.scene,.stage{position:relative;width:100%;height:100%;overflow:hidden;background:var(--ground,#F1F6FB)}
h1,h2,p{margin:0}h1,h2{font-weight:600}h1{font-size:90px;line-height:1.07;letter-spacing:-3px}h1 span{display:block}.signal{color:var(--signal,#0B7480)}
.eyebrow{position:absolute;left:192px;top:114px;font:400 28px/1.25 Plex,monospace;color:#475D76}.title{position:absolute;left:192px;top:180px;width:1536px}.rule{position:absolute;left:192px;top:163px;width:1536px;height:2px;background:#B9CBD7;transform-origin:left center}
.deck{position:absolute;left:192px;top:294px;font-size:36px;line-height:1.3;color:#354A62}.note{position:absolute;left:192px;top:978px;width:1536px;font-size:28px;line-height:1.2;color:#475D76}.panel{position:absolute;left:96px;top:350px;width:1728px;height:600px;overflow:hidden;background:#0C1A2B}.camera{position:absolute;inset:0;transform-origin:0 0}.screen{position:absolute;left:0;top:0;width:100%;height:auto;object-fit:contain;object-position:top left}.guide{position:absolute;left:192px;top:297px;width:1536px;font-size:32px;line-height:1.3;color:#0B7480}.guide span{display:inline-block;margin-right:44px}.proof-title{font-size:70px;letter-spacing:-2px}.phase{position:absolute;left:192px;top:180px;width:1536px}.phase h1{font-size:76px}.brand-mark{width:600px;height:auto}.full{position:absolute;inset:0;background:#F1F6FB}.hook-question{position:absolute;left:192px;top:245px;width:1536px}.hook-question h1{font-size:126px;line-height:1.06;letter-spacing:-5px}.hook-question h1 span:last-child{color:#0B7480}.hook-question .signal{font-size:156px}.hook-context{position:absolute;left:192px;top:688px;font-size:38px;line-height:1.25;color:#354A62}.meet{position:absolute;left:192px;top:270px;width:1536px}.meet p{font-size:58px;margin-bottom:48px}.meet .brand-mark{width:900px}.meet .meet-deck{font-size:38px;color:#354A62;margin-top:70px}
.offer-type{position:absolute;left:192px;top:280px;width:1536px}.offer-type h1{font-size:132px;letter-spacing:-5px}.offer-type p{font-size:50px;margin-top:50px;color:#354A62}.offer-proof .title{top:180px}.offer-proof .panel{top:350px;height:600px}.offer-proof .guide{top:285px}.offer-proof .screen{height:100%;object-fit:cover}
.ai-card{position:absolute;left:192px;top:310px;width:1536px}.byok{font-size:216px;font-weight:600;letter-spacing:-12px;line-height:1;color:#0B7480}.expansion{font-size:44px;margin-top:28px}.optional{position:absolute;left:880px;top:34px;font-size:58px;line-height:1.12;width:600px}.optional span{display:block}.ai-charge{position:absolute;left:192px;top:850px;font-size:32px;color:#475D76}.ai-example .panel{left:192px;top:365px;width:1536px;height:476px}.ai-example .screen{height:100%;object-fit:contain}.ai-example .panel{width:1142px}.ai-review-note{position:absolute;left:1390px;top:410px;width:340px;font-size:48px;line-height:1.2;color:#0B7480}.ai-review-note span{display:block;margin-bottom:28px}.ai-example .guide{top:285px}.ai-example .ai-charge{top:895px}.investigation .panel{left:192px;top:350px;width:1536px;height:600px}.walkthrough.investigation .panel{left:96px;top:295px;width:1728px;height:655px}.end-mark{position:absolute;left:192px;top:130px;width:500px}.download .title{top:315px}.download .title h1{font-size:112px}.url{position:absolute;left:192px;top:625px;font-size:76px;letter-spacing:-2px;color:#0B7480}.platform{position:absolute;left:192px;top:790px;font-size:34px}.disclaimer{position:absolute;left:192px;top:850px;font-size:32px;color:#354A62}.soon{position:absolute;left:192px;top:905px;font-size:30px;color:#475D76}
/* Three authored treatments: kinetic display, editorial, and guided app. */
.motion .eyebrow{letter-spacing:1px}.motion .rule{background:#0B7480;height:4px;width:150px}.motion .hook-question h1{font-size:136px}.motion .hook-context{top:735px}.motion .phase h1{font-size:84px}.motion .ai-card{top:335px}
.story .eyebrow{font-size:28px}.story .title{top:190px}.story .title h1{font-size:76px;letter-spacing:-2px}.story .panel{left:192px;width:1536px;top:360px;height:590px}.story .guide{top:298px}.story .hook-question{top:280px}.story .hook-question h1{font-size:112px}.story .hook-context{top:700px}.story .meet{top:300px}.story .offer-type h1{font-size:120px}.story .offer-proof .panel{top:360px}.story .phase h1{font-size:70px}.story .ai-example .panel{top:365px;width:1142px;height:476px}.story.download .title{top:330px}.story.download .title h1{font-size:108px}
.walkthrough .rule{top:162px}.walkthrough .title{top:181px}.walkthrough .title h1,.walkthrough .phase h1{font-size:68px;letter-spacing:-2px}.walkthrough .panel{top:295px;height:655px}.walkthrough .guide{top:252px;font-size:30px}.walkthrough .hook-question h1{font-size:118px}.walkthrough .hook-context{top:700px}.walkthrough .offer-proof .panel{top:330px;height:620px}.walkthrough .offer-proof .guide{top:275px}.walkthrough .ai-example .panel{top:350px;width:1142px;height:476px}.walkthrough .ai-example .guide{top:270px}.walkthrough.download .title{top:320px}.walkthrough.download .title h1{font-size:106px}
"""


def text(tag, value, cls='', ident=''):
    return f'<{tag}' + (f' class="{cls}"' if cls else '') + (f' id="{ident}"' if ident else '') + f'>{html.escape(value)}</{tag}>'


def heading(lines, ident='', cls=''):
    return f'<h1 id="{ident}" class="{cls}">' + ''.join(f'<span>{html.escape(s)}</span>' for s in lines) + '</h1>'


def video(prefix, asset, start, duration):
    return f'<div class="panel" id="{prefix}-panel"><div class="camera" id="{prefix}-camera" data-layout-allow-overflow><video id="{prefix}-screen" class="clip screen" src="assets/{asset}.mp4" data-start="{start}" data-hf-media-start-basis="local" data-duration="{duration}" data-track-index="1" muted playsinline></video></div></div>'


def build(name, shots=SHOTS):
    project = ROOT / name
    assets = project / 'assets'
    compositions = project / 'compositions'
    assets.mkdir(exist_ok=True)
    compositions.mkdir(exist_ok=True)
    (assets / 'fonts').mkdir(exist_ok=True)
    # Historical sources live in release archives and Git history, not duplicate
    # source trees inside every active project.
    active = {x[0] + '.html' for x in shots}
    for f in compositions.glob('*.html'):
        if f.name not in active: f.unlink()
    for f in (ROOT / 'assets/fonts').iterdir(): shutil.copy2(f, assets / 'fonts' / f.name)
    shutil.copy2(ROOT / 'assets/tableiq-logo-horizontal-light.svg', assets / 'wordmark.svg')
    pending = []
    for f in ['tabs.mp4', 'browse.mp4', 'manual-query.mp4', 'ask.mp4', 'mcp.mp4']:
        if (MEDIA / f).exists(): shutil.copy2(MEDIA / f, assets / f)
        else: pending.append(f)
    shutil.copy2(MEDIA / 'audio-v5' / ('motion-bed.wav' if name=='motion' else 'narrated-bed.wav'), assets / 'music-v5.wav')
    captures = json.loads((MEDIA / 'manifest.json').read_text())
    lengths = {v['id']: v['duration'] for v in json.loads((MEDIA / 'voices/durations.json').read_text())}
    hosts, audio, plan = [], [], []
    offset = 0
    for number, (sid, duration, lines, capture) in enumerate(shots, 1):
        p = f'{name}-{sid}'
        tl = ['const tl=gsap.timeline({paused:true});']
        def reveal(selector, at=0, axis='y', distance=40, time=.4):
            tl.append(f'tl.fromTo("{selector}",{{{axis}:{distance},opacity:0}},{{{axis}:0,opacity:1,duration:{time},ease:"power3.out"}},{at});')
        def swap(outgoing, incoming, at):
            tl.append(f'tl.to("{outgoing}",{{opacity:0,duration:0.12}},{at});')
            reveal(incoming, at+.12, 'y', 44, .38)
        def camera(scale, x, y, at, time=.55):
            tl.append(f'tl.to("#{p}-camera",{{scale:{scale},x:{x},y:{y},duration:{time},ease:"power3.inOut"}},{at});')
        section_labels = {
            'motion': ['A DATABASE CLIENT FOR MAC', 'ROOM FOR YOUR WORK', 'EVERYDAY DATABASE WORK', 'FOLLOW THE DATA', 'OPTIONAL AI', 'OPTIONAL AGENT WORKFLOW', 'START WITH YOUR DATABASE'],
            'story': ['01 / THE INTERRUPTION', '02 / A PLACE TO WORK', '03 / THE FAMILIAR TASK', '04 / ONE INVESTIGATION', '05 / A DRAFT, IF YOU WANT IT', '06 / KEEP THE QUERY IN VIEW', '07 / TRY IT ON YOUR MAC'],
            'walkthrough': ['TABLEIQ / MAC DEVELOPER PREVIEW', '01 / OPEN YOUR WORK', '02 / BROWSE YOUR DATABASE', '03 / WRITE · REVIEW · RUN', '04 / OPTIONAL AI ASSISTANCE', '05 / OPTIONAL MCP ACTIVITY', 'TABLEIQ / FREE & OPEN SOURCE'],
        }
        body = text('p', section_labels[name][number-1], 'eyebrow') + '<div class="rule" aria-hidden="true"></div>'
        note = ''
        if sid == 'hook':
            body += f'<div id="{p}-question" class="hook-question" data-hold>' + heading(['Hitting TablePlus’s', 'two-tab free limit?'], p+'-heading') + '</div>'
            body += text('p', 'Keep your database work in view.', 'hook-context', p+'-context')
            body += f'<div id="{p}-meet" class="meet" data-hold><p>Meet</p><img class="brand-mark" src="assets/wordmark.svg" alt="TableIQ"><p class="meet-deck">For Mac backend & full-stack developers.</p></div>'
            tl.append(f'tl.to("#{p}-question",{{y:-36,opacity:0,duration:.24,ease:"power2.in"}},3.4);')
            tl.append(f'tl.to("#{p}-context",{{opacity:0,duration:.18}},3.4);')
            reveal('#'+p+'-meet',3.62,'y',50,.42)
        elif sid == 'offer':
            body += f'<div id="{p}-type" class="offer-type" data-hold>' + heading(['Free. Open source.'],p+'-heading') + '<p>A database client for Mac.</p></div>'
            body += f'<div id="{p}-proof" class="offer-proof"><div class="title">' + heading(['Another table. Another query.'],p+'-proof-heading','proof-title') + '</div><p class="guide">Keep your work in view.</p>' + video(p,'tabs',3.5,6.5) + '</div>'
            swap('#'+p+'-type','#'+p+'-proof',3.36)
            note = 'Real TableIQ development capture · Synthetic data · Edited time'
        elif sid == 'client':
            body += '<div class="title">'+heading(lines,p+'-heading','proof-title')+'</div>'
            body += f'<p class="guide" id="{p}-guide">Try the same everyday task here.</p>' + video(p,'browse',0,9)
            # State windows are measured from the continuous source recording.
            camera(1.28,0,0,.15,.4)
            camera(1,0,0,1.25,.35)
            camera(1.2,0,0,1.8,.4)
            camera(1,0,0,2.9,.35)
            camera(1,0,-200,3.4,.4)
            camera(1.5,-120,-620,4.1,.65)
            camera(1,0,-200,7.7,.6)
            note = 'Browse data · Inspect relationships · Write SQL'
        elif sid == 'investigation':
            body += f'<div id="{p}-question" class="phase">'+heading(lines,p+'-heading')+'</div>'
            body += f'<div id="{p}-review" class="phase">'+heading(['Review the query.'],p+'-review-heading')+'</div>'
            body += f'<div id="{p}-run" class="phase">'+heading(['Run it.'],p+'-run-heading')+'</div>'
            body += f'<div id="{p}-answer" class="phase">'+heading(['Two matching orders.'],p+'-answer-heading')+'</div>'
            body += f'<p class="guide">Paid payments → Orders → No shipped shipment</p>' + video(p,'manual-query',0,16)
            swap('#'+p+'-question','#'+p+'-review',2.5)
            # Source action positions may be updated by the capture pipeline.
            action = captures.get('manual-query',{}).get('action_reference',{})
            run_at = action.get('run', 5.1)
            result_at = action.get('result', 5.5)
            swap('#'+p+'-review','#'+p+'-run',max(run_at-.45,4.6))
            swap('#'+p+'-run','#'+p+'-answer',max(result_at+.35,6))
            camera(1.1,0,0,2.4,.55)
            camera(1,0,0,max(run_at-.65,3.9),.45)
            camera(1.6,-25,-350,result_at+.75,.7)
            camera(1,0,0,14.5,.6)
            note = captures.get('manual-query',{}).get('caption','Real TableIQ development capture · Manual SQL · Synthetic data')
        elif sid == 'ai':
            body += f'<div id="{p}-intro"><div class="title">'+heading(lines,p+'-heading')+'</div>'
            body += f'<div class="ai-card" data-hold><p class="byok" id="{p}-byok">BYOK</p><p class="expansion">Bring your own AI key.</p><p class="optional" id="{p}-optional"><span>For a draft.</span><span>When you want it.</span></p></div><p class="ai-charge">AI is optional · Provider API charges apply</p></div>'
            body += f'<div id="{p}-example" class="ai-example"><div class="title">'+heading(['Review it before you run it.'],p+'-example-heading','proof-title')+'</div><p class="guide">Verified native AI example · SQL before Run · Still</p>'+video(p,'ask',6.3,6.7)+'<p class="ai-review-note"><span>A draft.</span><span>Your review.</span><span>Your Run.</span></p><p class="ai-charge">AI is optional · Use your own key · Provider API charges apply</p></div>'
            reveal('#'+p+'-byok',.2,'x',-100,.5)
            reveal('#'+p+'-optional',1.3,'x',45,.4)
            swap('#'+p+'-intro','#'+p+'-example',6.16)
        elif sid == 'activity':
            body += '<div class="title">'+heading(lines,p+'-heading','proof-title')+'</div>'
            body += '<p class="guide">Inspect the database queries sent through TableIQ.</p>'+video(p,'mcp',0,9)
            camera(1.35,-600 if name == 'story' else -660,-70,1.6,.55)
            camera(1,0,0,7.6,.55)
            note = 'Development capture · Local MCP client · Separate sample session'
        elif sid == 'download':
            body = f'<img id="{p}-mark" class="end-mark" src="assets/wordmark.svg" alt="TableIQ"><div class="title">'+heading(lines,p+'-heading')+'</div>'
            body += f'<p id="{p}-url" class="url" data-hold>tableiq.ideolyst.com</p><p class="platform">Mac · Apple silicon</p><p class="disclaimer">Developer preview · Unsigned / not notarized</p><p class="soon">Windows and Linux coming soon</p>'
            reveal('#'+p+'-mark',0,'x',48,.45)
            reveal('#'+p+'-url',.55,'y',28,.35)
        if note: body += text('p',note,'note')
        if sid not in ['hook','offer','download']:
            reveal('#'+p+'-heading',0,'x' if name=='motion' else 'y',45 if name=='motion' else 24,.36)
        # Holds serve reading and real app actions. No artificial ambient drift.
        if sid in ['hook','offer','ai','download']:
            pass  # Reading holds are annotated on their visible content.
        tl.append(f'window.__timelines["{p}"]=tl;')
        scene = f'<template><div id="{p}" class="scene {name} {sid}" data-composition-id="{p}" data-width="1920" data-height="1080" data-duration="{duration}"><div class="stage">{body}</div></div><script>{"".join(tl)}</script></template>'
        (compositions / f'{sid}.html').write_text(scene)
        hosts.append(f'<div id="{p}" class="clip" data-composition-id="{p}" data-composition-src="compositions/{sid}.html" data-start="{offset}" data-duration="{duration}" data-track-index="0" data-track-kind="graphics" data-beat="{sid}"></div>')
        voice = None if name=='motion' else 'v5-'+sid
        if voice:
            source = MEDIA / 'voices' / f'{voice}.wav'
            if source.exists() and voice in lengths:
                shutil.copy2(source, assets / f'voice-{sid}.wav')
                speech = lengths[voice]
                assert speech+.2 < duration, f'Voice does not fit {name}/{sid}: {speech}'
                audio.append(f'<audio id="{p}-voice" class="clip" src="assets/voice-{sid}.wav" data-start="{offset+.2}" data-duration="{speech}" data-track-index="10" data-volume="1"></audio>')
            else: pending.append('voices/'+voice+'.wav')
        plan.append(dict(id=sid,start=offset,duration=duration,title=lines,capture=capture,voice=voice,environment=note))
        offset += duration
    assert offset == 75
    audio.append(f'<audio id="{name}-music" class="clip" src="assets/music-v5.wav" data-start="0" data-duration="75" data-track-index="12" data-volume="1"></audio>')
    variables = html.escape(json.dumps([dict(id='ground',type='color',label='Pearl',default='#F1F6FB'),dict(id='ink',type='color',label='Ledger Ink',default='#0C1A2B'),dict(id='signal',type='color',label='Lumen Deep',default='#0B7480')]),quote=True)
    doc = f'<!doctype html><html lang="en" data-composition-variables="{variables}"><head><meta charset="utf-8"><meta name="viewport" content="width=1920,height=1080"><title>TableIQ {name} — 75-second launch film</title><script src="https://cdn.jsdelivr.net/npm/gsap@3.15.0/dist/gsap.min.js"></script><style>{CSS}</style></head><body><div id="root" data-composition-id="main" data-width="1920" data-height="1080" data-duration="75" data-fps="30" data-start="0">{"".join(hosts)}{"".join(audio)}</div><script>window.__timelines["main"]=gsap.timeline({{paused:true}});</script></body></html>'
    (project / 'index.html').write_text(doc)
    (project / 'film-plan.json').write_text(json.dumps(dict(revision='v5-audio',treatment=name,duration=75,shots=plan),indent=2,ensure_ascii=False)+'\n')
    treatment = {'motion':'Music-only kinetic display: large semantic type, directional title handoffs, concise evidence labels.','story':'Narrated editorial: numbered chapters, measured headline scale, real evidence presented as an investigation.','walkthrough':'Narrated guided app: largest app viewport, compact headlines and task guidance; official Argo finishing is a separate delivery step.'}[name]
    spec = f'''# {name.title()} — approved 75-second film\n\nAudience: Mac backend and full-stack developers considering TableIQ for ordinary database work.\n\n{treatment}\n\n1920 × 1080, 30 fps, exactly 2,250 frames. Pearl #F1F6FB, Ledger Ink #0C1A2B, Lumen Deep #0B7480. Instrument Sans 500/600 and IBM Plex Mono, bundled locally. Key type 68–156 px, body 30–58 px, source notes 28 px. Titles inside 192 px side margins. Real app crops use 96 px or 192 px side margins and hold long enough to read.\n\nApproved timing: hook 0–8; offer 8–18; client 18–27; investigation 27–43; AI 43–56; activity 56–65; download 65–75.\n\nOrdinary client work occupies the first 43 seconds. The investigation shows manually written SQL and a real Run before the two-row result. AI is optional, BYOK, with provider charges disclosed. The native AI evidence is expressly a still, not a fabricated continuous generation. MCP is an optional separate sample session.\n\nCamera changes only accompany measured source state changes. Browse: table 0, Structure 1.8, ERD 3.4 seconds. MCP: expanded SQL 1.6 seconds. Camera returns to context before each cut. No invented typing, cursor, response, or result. No drift on readable holds.\n\nMotion uses masked editorial title replacement and finite GSAP transforms; 0.36–0.55 second entrances, 0.55–0.65 second camera changes, power3.out/power3.inOut. Source video playback remains entirely framework owned. Every shot is an editable sub-composition.\n\nNo blanket faster/better/RAM claim; DBeaver is not portrayed as paid. TablePlus claim is limited to the free version’s two-tab limit. CTA states Mac Apple silicon, unsigned/not notarized preview, Windows/Linux coming soon.\n\nDelivery gates: complete HyperFrames check, seven scene midpoints, hook handoff, manual Run/result transition, native AI still label, and full CTA hold. Narrated captions and final audio are prepared separately by the delivery pipeline.\n'''
    (project / 'SPEC.md').write_text(spec)
    board = [f'# {name.title()} v5 storyboard', '', treatment, '', '| Time | Shot | Visible evidence |', '|---|---|---|']
    for s in plan:
        board.append(f'| {s["start"]}–{s["start"]+s["duration"]} s | {" ".join(s["title"])} | {s["capture"] or "Editorial typography / supplied wordmark"} |')
    board += ['', 'All media remains truthful: continuous wide browse and MCP sources, actual manual SQL Run, and a separately labelled verified native AI still. Final source frames may hold; clips never loop. Earlier sources are archived in the GitHub release and Git history.', '']
    (project / 'STORYBOARD.md').write_text('\n'.join(board))
    print(name, '75s', 'pending: '+', '.join(pending) if pending else 'all assets present')


if __name__ == '__main__':
    for treatment in ['motion','story','walkthrough']:
        build(treatment)
