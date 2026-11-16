"""Caption the delivered films using checked local narration and measured word times.

The intervals below follow the local Whisper transcription of each individual voice
clip. Spelling is corrected against the narration script, including TableIQ, AI,
SQL, and the exact sample question. Motion has descriptive text, not a voiceover.
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
CUES = {
    "hook": [(0,1.6,"Still counting query tabs?")],
    "room": [(0,2.3,"When one investigation needs another query,"),(2.75,4.25,"a tab limit breaks the flow.")],
    "client": [(0.15,3.95,"TableIQ is a free, open-source database client for Mac.")],
    "workflow": [(0,2.3,"Browse your tables, inspect your schema,"),(2.6,4.1,"and keep your SQL together.")],
    "byok": [(0,2.95,"When you want help, use your own AI provider key.")],
    "ask": [(0.1,2.4,"Ask which paid orders have no shipped shipment."),(3.1,4.5,"Review the generated SQL.")],
    "result": [(0,1.65,"You decide when to run it."),(2,5.5,"Two matching orders, totaling 7,300 cents.")],
    "activity": [(0,1.65,"Working with a coding agent?"),(2,5.65,"Open MCP Activity to inspect its calls and queries.")],
    "download": [(0,1,"Built with Rust."),(1.3,2.85,"Get the Mac developer preview."),(3.2,5.25,"No TableIQ account required.")],
    "story-hook": [(0,1.15,"A customer has paid."),(1.15,3.05,"Their order still has not shipped."),(3.3,3.95,"Which one?")],
    "story-client": [(0,2.9,"Keep the investigation together in TableIQ,"),(3.25,6.2,"a free, open-source database client for Mac.")],
    "story-ai": [(0,2.25,"Bring your own AI key when you want help."),(2.75,4.95,"Review the generated query before running it.")],
    "story-result": [(0,2.25,"Two orders match in this sample database."),(2.95,3.6,"Check the rows."),(3.85,5.15,"Keep control of the query.")],
    "story-mcp": [(0,3.05,"Inspect your coding agent’s database queries, too.")],
}

def stamp(t):
    ms=round(t*1000)
    return f"{ms//3600000:02d}:{ms//60000%60:02d}:{ms//1000%60:02d}.{ms%1000:03d}"

for name in ["motion","story","walkthrough"]:
    plan=json.loads((ROOT/name/"film-plan.json").read_text())
    cues=[]
    if name=="motion":
        cues.append((0,2.8,"[Music] Still counting query tabs?"))
        for shot in plan["shots"][1:]:
            cues.append((shot["start"]+.25,shot["start"]+shot["duration"]-.15," ".join(shot["title"])))
    else:
        for shot in plan["shots"]:
            for start,end,text in CUES.get(shot["voice"],[]):
                assert end+.2<=shot["duration"], (name,shot["id"],end)
                cues.append((shot["start"]+.2+start,shot["start"]+.2+end,text))
    out=["WEBVTT",""]
    for i,(start,end,text) in enumerate(cues,1):
        assert 0<=start<end<=plan["duration"]
        out += [str(i),f"{stamp(start)} --> {stamp(end)}",text,""]
    dest=ROOT/"renders"/f"tableiq-launch-{name}-v2.vtt"
    dest.parent.mkdir(exist_ok=True)
    dest.write_text("\n".join(out),encoding="utf-8")
    print(dest.name,len(cues))
