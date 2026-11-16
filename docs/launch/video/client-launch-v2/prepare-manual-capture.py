"""Prepare the continuous, genuine Argo SQL/Run capture for the approved 16s slot.

Usage: python3 prepare-manual-capture.py /path/to/recording-project
The recording project must contain the successful capture-manual-v4 session.
"""
from pathlib import Path
import hashlib
import json
import subprocess
import sys

root = Path(__file__).resolve().parent
capture = Path(sys.argv[1]) / '.argo/capture-manual-v4'
source = capture / 'video.mp4'
marks = {k: v / 1000 for k, v in json.loads((capture / '.timing.json').read_text()).items()}
out = root / 'production-media/manual-query.mp4'
subprocess.run([
    'ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', str(source), '-an',
    '-vf', 'trim=end=6.066667,setpts=PTS-STARTPTS,crop=2110:800:430:110,scale=1920:728:flags=lanczos,setsar=1,fps=30,tpad=stop_mode=clone:stop_duration=16',
    '-t', '16', '-c:v', 'libx264', '-preset', 'fast', '-crf', '17',
    '-pix_fmt', 'yuv420p', '-movflags', '+faststart', '-y', str(out),
], check=True)
manifest = root / 'production-media/manifest.json'
data = json.loads(manifest.read_text())
data['manual-query'] = {
    'kind': 'real-app-recording', 'source_capture': 'capture-manual-v4',
    'source_range_seconds': [0, 6.066667],
    'crop': [430, 110, 2110, 800], 'output_size': [1920, 728], 'duration': 16,
    'caption': 'Real TableIQ development capture · Synthetic data · Manual SQL and Run',
    'sha256': hashlib.sha256(out.read_bytes()).hexdigest(),
    'action_reference': {'input_start': marks['typing'], 'query_ready': marks['query-ready'], 'run': marks['run-click'], 'result': marks['result-ready']},
    'framing': 'One continuous source crop; genuine final result frame held to16s. Actual UI input and Run against the real Rust backend.',
}
manifest.write_text(json.dumps(data, indent=2, ensure_ascii=False) + '\n')
print(json.dumps(data['manual-query']))
