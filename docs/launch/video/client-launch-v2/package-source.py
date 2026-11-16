"""Package reviewed editable sources and curated media; exclude runtime/private state."""
from pathlib import Path
import argparse
import hashlib
import json
import zipfile

ROOT=Path(__file__).resolve().parent
parser=argparse.ArgumentParser()
parser.add_argument("output",type=Path)
parser.add_argument("--version",default="v3",choices=["v2","v3","v4"])
args=parser.parse_args()
excluded={"node_modules",".hyperframes",".thumbnails",".argo",".waveform-cache",".transcode-cache","__pycache__","renders","videos"}
files=[]
for source in sorted(ROOT.rglob("*")):
    rel=source.relative_to(ROOT)
    if any(part in excluded for part in rel.parts) or source.is_symlink() or not source.is_file():
        continue
    if "review" in rel.parts and source.suffix==".png":
        continue
    if source.name==".DS_Store":
        continue
    files.append((source,rel))
args.output.parent.mkdir(parents=True,exist_ok=True)
with zipfile.ZipFile(args.output,"w",compression=zipfile.ZIP_DEFLATED,compresslevel=6) as archive:
    for source,rel in files:
        archive.write(source,f"TableIQ-Launch-Films-{args.version}/"+rel.as_posix())
print(json.dumps({"file":str(args.output),"files":len(files),"bytes":args.output.stat().st_size,"sha256":hashlib.sha256(args.output.read_bytes()).hexdigest()}))
