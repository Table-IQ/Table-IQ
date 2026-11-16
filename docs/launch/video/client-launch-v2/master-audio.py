"""Two-pass loudness mastering, preserving the already reviewed video stream."""
import argparse
import json
import subprocess
from pathlib import Path

parser=argparse.ArgumentParser()
parser.add_argument("input",type=Path)
parser.add_argument("output",type=Path)
args=parser.parse_args()
assert args.input.resolve()!=args.output.resolve(), "Use a separate output file."
measurement=subprocess.run([
    "ffmpeg","-hide_banner","-i",str(args.input),"-af",
    "loudnorm=I=-16:TP=-2:LRA=11:print_format=json","-f","null","-"
],capture_output=True,text=True,check=True)
d=json.JSONDecoder().raw_decode(measurement.stderr[measurement.stderr.rfind("{"):])[0]
filt=(f"loudnorm=I=-16:TP=-2:LRA=11:measured_I={d['input_i']}:"
      f"measured_TP={d['input_tp']}:measured_LRA={d['input_lra']}:"
      f"measured_thresh={d['input_thresh']}:offset={d['target_offset']}:linear=true")
subprocess.run([
    "ffmpeg","-hide_banner","-loglevel","error","-i",str(args.input),
    "-map","0:v:0","-map","0:a:0","-c:v","copy","-af",filt,
    "-ar","48000","-ac","2","-c:a","aac","-b:a","192k",
    "-movflags","+faststart","-y",str(args.output)
],check=True)
