// Complete the walkthrough through the pinned official Argo import/alignment/export workflow.
import fs from 'node:fs';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import {importVideo} from './node_modules/@argo-video/cli/dist/import.js';
import {ClipCache} from './node_modules/@argo-video/cli/dist/tts/cache.js';
import {alignClips} from './node_modules/@argo-video/cli/dist/tts/align.js';
import {parseWavHeader,createWavBuffer} from './node_modules/@argo-video/cli/dist/tts/engine.js';

const input=process.argv[2];
if(!input)throw new Error('Usage: node finish-argo.mjs <HyperFrames-picture.mp4> [v2|v3|v4|v5]');
const version=process.argv[3]??'v5';
if(!['v2','v3','v4','v5'].includes(version))throw new Error('Unknown film revision');
const demo=`tableiq-walkthrough-${version}`;
const plan=JSON.parse(fs.readFileSync('film-plan.json'));
const lines=new Map(JSON.parse(fs.readFileSync('../production-media/voices/lines.json')));
await importVideo({videoPath:path.resolve(input),cwd:process.cwd(),demo,demosDir:'demos',force:true});
const cache=new ClipCache(process.cwd());
const entries=[],clips=[],marks={};
for(const shot of plan.shots){
  if(!shot.voice)continue;
  const entry={scene:shot.id,text:lines.get(shot.voice),voice:'af_heart',speed:version==='v5'?1.008:1.05};
  const wav=fs.readFileSync(`assets/voice-${shot.id}.wav`);
  cache.cacheClip(demo,entry,wav);
  const h=parseWavHeader(wav),samples=new Float32Array(h.dataSize/4);
  for(let i=0;i<samples.length;i++)samples[i]=wav.readFloatLE(h.dataOffset+4*i);
  marks[shot.id]=Math.round((shot.start+0.2)*1000);
  entries.push(entry);clips.push({scene:shot.id,durationMs:h.durationMs,samples});
}
const aligned=alignClips(marks,clips,plan.duration*1000,24000);
if(aligned.overflowMs>20)throw new Error(`Narration overflow: ${aligned.overflowMs}ms`);
const base=`.argo/${demo}`;
fs.writeFileSync(`demos/${demo}.scenes.json`,JSON.stringify(entries,null,2));
fs.writeFileSync(`${base}/.timing.json`,JSON.stringify(marks,null,2));
fs.writeFileSync(`${base}/narration-voice.wav`,createWavBuffer(aligned.samples,24000));
fs.writeFileSync('argo-placements.json',JSON.stringify(aligned.placements,null,2));
const music=version==='v5'?'assets/music-v5.wav':version==='v4'?'assets/launch-score-v4.wav':'assets/launch-score.wav';
const gain=version==='v5'?1:0.14;
execFileSync('ffmpeg',['-hide_banner','-loglevel','error','-i',`${base}/narration-voice.wav`,'-i',music,'-filter_complex',`[1:a]volume=${gain}[m];[0:a][m]amix=inputs=2:duration=first:normalize=0,alimiter=limit=0.707946:level=false[a]`,'-map','[a]','-ar','48000','-ac','2','-c:a','pcm_s16le','-y',`${base}/narration-aligned.wav`],{stdio:'inherit'});
execFileSync(process.execPath,['node_modules/@argo-video/cli/bin/argo.js','export',demo],{stdio:'inherit'});
if(['v4','v5'].includes(version)){
  // Argo supplies aligned/exported sound. Preserve the reviewed HyperFrames picture:
  // the additional Argo picture transcode changes the measured brand colours.
  const exported=`videos/${demo}.mp4`,preserved=`videos/${demo}-picture-preserved.mp4`;
  execFileSync('ffmpeg',['-hide_banner','-loglevel','error','-i',path.resolve(input),'-i',exported,'-map','0:v:0','-map','1:a:0','-c','copy','-map_metadata','-1','-movflags','+faststart','-y',preserved],{stdio:'inherit'});
  fs.renameSync(preserved,exported);
}
console.log(`Argo export complete: videos/${demo}.mp4; overflow ${aligned.overflowMs}ms`);
