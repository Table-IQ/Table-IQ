"""Package reviewed editable sources and curated media; exclude runtime/private state."""
from pathlib import Path
import argparse
import hashlib
import json
import re
import zipfile

ROOT=Path(__file__).resolve().parent
parser=argparse.ArgumentParser()
parser.add_argument("output",type=Path)
parser.add_argument("--version",default="v5",choices=["v2","v3","v4","v5"])
args=parser.parse_args()
excluded={"node_modules",".hyperframes",".thumbnails",".argo",".waveform-cache",".transcode-cache","__pycache__","renders","videos"}
files=[]
projects=['motion','story','walkthrough']
used={}
for project in projects:
    text='\n'.join(p.read_text() for p in [ROOT/project/'index.html',*(ROOT/project/'compositions').glob('*.html')])
    used[project]=set(re.findall(r"assets/[^\s'\"<>\)]+",text))
for source in sorted(ROOT.rglob("*")):
    rel=source.relative_to(ROOT)
    if any(part in excluded for part in rel.parts) or source.is_symlink() or not source.is_file():
        continue
    if "review" in rel.parts and source.suffix==".png":
        continue
    if source.name==".DS_Store":
        continue
    if args.version=='v5':
        if len(rel.parts)>2 and rel.parts[0] in projects and rel.parts[1]=='assets':
            asset=Path(*rel.parts[1:]).as_posix()
            notice=source.suffix=='.txt' and 'fonts' in rel.parts
            if asset not in used[rel.parts[0]] and not notice:continue
        if len(rel.parts)>1 and rel.parts[0]=='production-media':
            if rel.parts[1]=='audio-v5' and source.suffix=='.wav':continue  # derived; project stems are included
            if rel.parts[1]=='voices' and source.suffix=='.wav' and not source.name.startswith(('v4-','v5-')):continue
            if source.suffix=='.mp4' and source.stem not in ['tabs','browse','manual-query','ask','mcp']:continue
            if len(rel.parts)==2 and source.suffix=='.wav' and source.name!='launch-score-v4.wav':continue
    files.append((source,rel))
args.output.parent.mkdir(parents=True,exist_ok=True)
with zipfile.ZipFile(args.output,"w",compression=zipfile.ZIP_DEFLATED,compresslevel=6) as archive:
    for source,rel in files:
        archive.write(source,f"TableIQ-Launch-Films-{args.version}/"+rel.as_posix())
print(json.dumps({"file":str(args.output),"files":len(files),"bytes":args.output.stat().st_size,"sha256":hashlib.sha256(args.output.read_bytes()).hexdigest()}))
