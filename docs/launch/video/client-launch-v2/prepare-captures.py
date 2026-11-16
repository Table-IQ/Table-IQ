"""Freeze real capture ranges and genuine native proof stills into portable assets.

No UI is reconstructed. Still-derived assets are explicitly identified in the manifest.
"""
from pathlib import Path
import argparse
import hashlib
import json
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent
DEST = ROOT / "production-media"

def run(args):
    subprocess.run(["ffmpeg","-hide_banner","-loglevel","error",*args],check=True)

def clip(source, start, end, seconds, crop, target, output_size=(1920,800)):
    # Reading holds repeat genuine frames. The manifest preserves the selected source range.
    x,y,w,h = crop
    ow,oh = output_size
    filt=f"crop={w}:{h}:{x}:{y},scale={ow}:{oh}:force_original_aspect_ratio=decrease:flags=lanczos,pad={ow}:{oh}:(ow-iw)/2:(oh-ih)/2:color=0x0C1A2B,setsar=1,fps=30,tpad=stop_mode=clone:stop_duration={seconds}"
    run(["-ss",str(start),"-to",str(end),"-i",str(source),"-an","-vf",filt,"-t",str(seconds),"-c:v","libx264","-preset","fast","-crf","17","-pix_fmt","yuv420p","-movflags","+faststart","-y",str(target)])

def still(source, seconds, crop, target):
    x,y,w,h=crop
    filt=f"crop={w}:{h}:{x}:{y},scale=1920:800:force_original_aspect_ratio=decrease:flags=lanczos,pad=1920:800:(ow-iw)/2:(oh-ih)/2:color=0x0C1A2B,setsar=1"
    run(["-loop","1","-framerate","30","-i",str(source),"-an","-vf",filt,"-t",str(seconds),"-c:v","libx264","-preset","fast","-crf","16","-pix_fmt","yuv420p","-movflags","+faststart","-y",str(target)])

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--capture-root",type=Path,required=True)
    parser.add_argument("--recording-project",type=Path,required=True)
    args=parser.parse_args()
    p=args.capture_root; record=args.recording_project
    DEST.mkdir(exist_ok=True)
    times={k:v/1000 for k,v in json.loads((record/".argo/capture-v2/.timing.json").read_text()).items()}
    raw=record/".argo/capture-v2/video.mp4"
    mcp_raw=record/".argo/capture-mcp-v2/video.mp4"
    mcp_times=json.loads((p/"mcp-capture-marks.json").read_text())
    manifest={}
    # All activity below is genuine React UI talking to the real Rust command handlers.
    specs=[
        ("intro",raw,times["intro"],times["tabs"]-0.06,6,(430,90,2100,1020)),
        ("tabs",raw,times["tabs"],times["browse"]-0.02,6,(430,102,1500,620)),
        ("tabstrip",raw,times["browse"]-0.6,times["browse"]-0.03,8,(430,102,1460,98)),
        ("byok",raw,times["byok-ready"]-0.2,times["byok-close"]-0.05,7,(740,470,1460,608)),
    ]
    for name,source,start,end,seconds,crop in specs:
        target=DEST/(name+".mp4")
        output_size=(1920,132) if name=="tabstrip" else (1920,800)
        clip(source,start,end,seconds,crop,target,output_size)
        caption="Real TableIQ development capture · Synthetic data · Edited time"
        manifest[name]={"kind":"real-app-recording","source_name":source.name,"source_capture":source.parent.name,"source_range_seconds":[start,end],"crop":crop,"output_size":output_size,"duration":seconds,"caption":caption,"sha256":hashlib.sha256(target.read_bytes()).hexdigest()}
    # Ordinary client actions, then a readable close-up of the actual relationships.
    browse_parts=[
        (times["browse"],times["browse"]+1.8,1.8,(430,70,2210,1000)),
        (times["browse"]+1.8,times["byok"]-1.8,1.6,(430,120,1600,800)),
        (times["byok"]-1.8,times["byok"]-1.0,0.8,(430,150,2210,1400)),
        (times["byok"]-1.0,times["byok"]-0.03,4.8,(610,650,1240,520)),
    ]
    inputs=[];filters=[]
    for i,(start,end,seconds,crop) in enumerate(browse_parts):
        f=DEST/f"browse-{i}.mp4";clip(raw,start,end,seconds,crop,f);inputs += ["-i",str(f)];filters.append(f"[{i}:v]")
    run([*inputs,"-filter_complex",''.join(filters)+"concat=n=4:v=1:a=0[v]","-map","[v]","-an","-c:v","libx264","-preset","fast","-crf","17","-pix_fmt","yuv420p","-movflags","+faststart","-y",str(DEST/"browse.mp4")])
    manifest["browse"]={"kind":"real-app-recording","source_capture":"capture-v2","cuts":[{"range":[a,b],"duration":d,"crop":c} for a,b,d,c in browse_parts],"duration":9,"caption":"Real TableIQ development capture · Synthetic data · Edited time","sha256":hashlib.sha256((DEST/"browse.mp4").read_bytes()).hexdigest()}
    # Establish Activity, then punch into the actual Tool/SQL/Rows/Status columns.
    # The two consecutive source ranges preserve the real Expand interaction.
    close_at=max(0,mcp_times["expanded"]-0.2)
    wide_start=max(0,close_at-1.4)
    clip(mcp_raw,wide_start,close_at,1.4,(430,70,2210,1000),DEST/"mcp-wide.mp4")
    clip(mcp_raw,close_at,mcp_times["end"]-0.05,6.6,(1100,105,1540,640),DEST/"mcp-close.mp4")
    run(["-i",str(DEST/"mcp-wide.mp4"),"-i",str(DEST/"mcp-close.mp4"),"-filter_complex","[0:v][1:v]concat=n=2:v=1:a=0[v]","-map","[v]","-an","-c:v","libx264","-crf","17","-preset","fast","-pix_fmt","yuv420p","-movflags","+faststart","-y",str(DEST/"mcp.mp4")])
    manifest["mcp"]={"kind":"real-app-recording","source_capture":"capture-mcp-v2","source_ranges_seconds":[[wide_start,close_at],[close_at,mcp_times["end"]-0.05]],"crops":[[430,70,2210,1000],[1100,105,1540,640]],"duration":8,"caption":"Development capture · Local MCP client · Separate sample session","sha256":hashlib.sha256((DEST/"mcp.mp4").read_bytes()).hexdigest()}
    # These are actual native captures from the passed acceptance case, not new live output.
    for name,source,seconds,crop,caption in [
        ("ask",ROOT/"assets/ai-generated.png",10,(214,49,760,236),"Verified native AI example · SQL before Run · Still"),
        ("result",ROOT/"assets/ai-result.png",8,(442,565,698,252),"Verified native AI example · After manual Run · Still"),
    ]:
        target=DEST/(name+".mp4");still(source,seconds,crop,target)
        manifest[name]={"kind":"genuine-native-still","source_name":source.name,"crop":crop,"duration":seconds,"caption":caption,"sha256":hashlib.sha256(target.read_bytes()).hexdigest()}
    # Reuse the original TableIQ launch soundtrack; retain a short silent tail for longer CTAs.
    run(["-i",str(p/"existing-launch-score.wav"),"-af","afade=t=out:st=53:d=3,apad=whole_dur=60","-t","60","-c:a","pcm_s16le","-ar","48000","-ac","2","-y",str(DEST/"launch-score.wav")])
    shutil.copytree(p/"voices",DEST/"voices",dirs_exist_ok=True)
    shutil.copy2(p/"generated.sql",DEST/"native-generated.sql")
    shutil.copy2(p/"mcp-calls.json",DEST/"mcp-calls.json")
    (DEST/"manifest.json").write_text(json.dumps(manifest,indent=2,ensure_ascii=False)+'\n')
    print(json.dumps({"prepared":list(manifest),"destination":str(DEST)}))

if __name__=="__main__":main()
