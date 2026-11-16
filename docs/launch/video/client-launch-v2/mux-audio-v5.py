"""Replace sound while preserving the approved H.264 picture stream exactly."""
from pathlib import Path
import argparse
import subprocess

ROOT = Path(__file__).resolve().parent
p = argparse.ArgumentParser()
p.add_argument('input', type=Path)
p.add_argument('output', type=Path)
p.add_argument('--treatment', choices=['motion', 'story', 'walkthrough'], required=True)
args = p.parse_args()
assert args.input.resolve() != args.output.resolve()
if args.treatment == 'motion':
    audio = ROOT / 'production-media/audio-v5/motion-bed.wav'
elif args.treatment == 'walkthrough':
    audio = ROOT / 'walkthrough/.argo/tableiq-walkthrough-v5/narration-aligned.wav'
    if not audio.exists():
        raise SystemExit('Finish the official Argo alignment/export before muxing the walkthrough.')
else:
    audio = ROOT / 'production-media/audio-v5/narrated-mix.wav'
args.output.parent.mkdir(parents=True, exist_ok=True)
subprocess.run([
    'ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', str(args.input), '-i', str(audio),
    '-map', '0:v:0', '-map', '1:a:0', '-c:v', 'copy', '-af', 'alimiter=limit=0.707946:level=false',
    '-ar', '48000', '-ac', '2', '-c:a', 'aac', '-b:a', '192k', '-t', '75',
    '-map_metadata', '-1', '-map_chapters', '-1', '-movflags', '+faststart', '-y', str(args.output),
], check=True)
