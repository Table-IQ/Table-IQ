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

def prepare_focus_captures(record, mcp_times):
    """Keep a single source crop so authored cameras can move without baked cuts.

    Can be called independently to replace only these two visual assets; it never
    writes voices, music, captions, or the other captured evidence.
    """
    times={k:v/1000 for k,v in json.loads((record/".argo/capture-v2/.timing.json").read_text()).items()}
    expanded=mcp_times["expanded"]
    specs=[
        ("browse",record/".argo/capture-v2/video.mp4",times["browse"],times["byok"]-0.03,9,(430,70,2210,1390),(1920,1208)),
        ("mcp",record/".argo/capture-mcp-v2/video.mp4",max(0,expanded-1.6),mcp_times["end"]-0.05,8,(430,70,2210,1000),(1920,868)),
    ]
    out={}
    for name,source,start,end,duration,crop,size in specs:
        target=DEST/(name+".mp4")
        clip(source,start,end,duration,crop,target,size)
        caption="Real TableIQ development capture · Synthetic data · Edited time" if name=="browse" else "Development capture · Local MCP client · Separate sample session"
        out[name]={"kind":"real-app-recording","source_capture":source.parent.name,"source_range_seconds":[start,end],"crop":crop,"output_size":size,"duration":duration,"caption":caption,"sha256":hashlib.sha256(target.read_bytes()).hexdigest(),"framing":"One consistent wide crop; smooth focus is authored in the composition camera. Final source frame is held to fill the existing scene slot."}
        if name=="browse":
            out[name]["action_reference"]={"order_selection":0,"structure_state":1.8,"schema_state":3.4,"timing_note":"State windows corroborated against the capture script and actual source frames; subframe click telemetry is not present."}
        else:
            out[name]["action_reference"]={"show_full_sql":round(expanded-start,3),"timing_note":"Exact expanded mark relative to selected source range."}
    return out

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
    manifest.update(prepare_focus_captures(record, mcp_times))
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
