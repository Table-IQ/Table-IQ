"""Prepare the selected quiet music / clear narration mix without changing picture.

Requires FFmpeg. The existing approved narration is kept, with pitch-preserving
4% slower delivery and a light tonal softening. Music is independently levelled;
never normalize the final mix back up, which would undo the quiet background.
"""
from pathlib import Path
import json
import subprocess

ROOT = Path(__file__).resolve().parent
MEDIA = ROOT / 'production-media'
OUT = MEDIA / 'audio-v5'
OUT.mkdir(exist_ok=True)
PACE = 0.96
STARTS = {'hook': 0, 'offer': 8, 'client': 18, 'investigation': 27, 'ai': 43, 'activity': 56, 'download': 65}

def run(args):
    return subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', *args], check=True, capture_output=True, text=True)

def duration(path):
    return float(subprocess.check_output(['ffprobe', '-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0', str(path)]))

def level(source, dest, target, peak, before='anull', rate=24000, channels=1):
    result = subprocess.run(['ffmpeg', '-hide_banner', '-i', str(source), '-af', f'{before},loudnorm=I={target}:TP={peak}:LRA=11:print_format=json', '-f', 'null', '-'], check=True, capture_output=True, text=True)
    info = json.JSONDecoder().raw_decode(result.stderr[result.stderr.rfind('{'):])[0]
    norm = f"loudnorm=I={target}:TP={peak}:LRA=11:measured_I={info['input_i']}:measured_TP={info['input_tp']}:measured_LRA={info['input_lra']}:measured_thresh={info['input_thresh']}:offset={info['target_offset']}:linear=true"
    run(['-i', str(source), '-af', f'{before},{norm}', '-ar', str(rate), '-ac', str(channels), '-c:a', 'pcm_f32le', '-y', str(dest)])

lengths = []
for sid in STARTS:
    source = MEDIA / 'voices' / f'v4-{sid}.wav'
    dest = MEDIA / 'voices' / f'v5-{sid}.wav'
    # Keep the approved speaker, words and pitch; use a slightly gentler pace.
    level(source, dest, -17, -4, f'atempo={PACE},highpass=f=80,equalizer=f=250:t=q:w=1.2:g=-1.5,equalizer=f=3200:t=q:w=1.6:g=-1')
    lengths.append({'id': 'v5-' + sid, 'duration': duration(dest)})

voice_dir = MEDIA / 'voices'
for filename, new, key in [('durations.json', lengths, lambda x: x['id']), ('lines.json', [[key.replace('v4-', 'v5-'), text] for key, text in json.loads((voice_dir / 'lines.json').read_text()) if key.startswith('v4-')], lambda x: x[0])]:
    old = [x for x in json.loads((voice_dir / filename).read_text()) if not key(x).startswith('v5-')]
    (voice_dir / filename).write_text(json.dumps(old + new, indent=2) + '\n')

tone = 'highpass=f=120,lowpass=f=1800,equalizer=f=1600:t=q:w=1.4:g=-6,acompressor=threshold=0.12:ratio=2:attack=25:release=350:makeup=1'
level(MEDIA / 'launch-score-v4.wav', OUT / 'bed-base.wav', -36, -9, tone, 48000, 2)
level(MEDIA / 'launch-score-v4.wav', OUT / 'motion-bed.wav', -30, -9, tone, 48000, 2)

# Smooth, bounded attenuation around each utterance. The music never rises above
# its already quiet base level, and does not pump during every short word pause.
envelopes = []
for item in lengths:
    sid = item['id'][3:]
    start = STARTS[sid] + .2
    end = start + item['duration'] + .15
    a, b, c, d = max(0, start - .15), start, end, min(75, end + .8)
    envelopes.append(f'if(lt(t,{a}),1,if(lt(t,{b}),1-0.5*(t-{a})/({b}-{a}),if(lt(t,{c}),0.5,if(lt(t,{d}),0.5+0.5*(t-{c})/({d}-{c}),1))))')
envelope = '1'
for value in envelopes:
    envelope = f'min({envelope},{value})'
run(['-i', str(OUT / 'bed-base.wav'), '-af', f"volume='{envelope}':eval=frame,afade=t=in:d=1,afade=t=out:st=71:d=4", '-ar', '48000', '-ac', '2', '-c:a', 'pcm_f32le', '-y', str(OUT / 'narrated-bed.wav')])

# The exact same prepared stems are available in the editable compositions.
args, filters, inputs = [], [], []
for i, (sid, start) in enumerate(STARTS.items()):
    args += ['-i', str(voice_dir / f'v5-{sid}.wav')]
    filters.append(f'[{i}:a]adelay={round((start+.2)*1000)}:all=1[v{i}]')
    inputs.append(f'[v{i}]')
filters.append(''.join(inputs) + f'amix=inputs={len(inputs)}:duration=longest:normalize=0,apad,atrim=duration=75[voice]')
run([*args, '-filter_complex', ';'.join(filters), '-map', '[voice]', '-ar', '48000', '-ac', '2', '-c:a', 'pcm_f32le', '-y', str(OUT / 'narration.wav')])
run(['-i', str(OUT / 'narration.wav'), '-i', str(OUT / 'narrated-bed.wav'), '-filter_complex', '[0:a][1:a]amix=inputs=2:duration=first:normalize=0,alimiter=limit=0.707946:level=false[mix]', '-map', '[mix]', '-ar', '48000', '-ac', '2', '-c:a', 'pcm_f32le', '-y', str(OUT / 'narrated-mix.wav')])

report = {'choice': 'Soft, very quiet background music with clear narration', 'pace_multiplier': PACE, 'voice_target_lufs': -17, 'music_base_target_lufs': -36, 'music_duck_db': -6.02, 'motion_music_target_lufs': -30, 'duration_seconds': 75, 'voices': lengths, 'no_final_loudness_boost': True, 'review_limit': 'Measured and compared; no independent human listening assessment.'}
(OUT / 'mix-plan.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report))
