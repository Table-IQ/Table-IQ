// Generate the approved narration locally using the pinned Argo/Kokoro pipeline.
// npm ci in walkthrough/ first. No network provider keys or customer data.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {KokoroEngine} from './walkthrough/node_modules/@argo-video/cli/dist/tts/engines/kokoro.js';
import {parseWavHeader} from './walkthrough/node_modules/@argo-video/cli/dist/tts/engine.js';
const root=path.dirname(fileURLToPath(import.meta.url));
const dir=path.join(root,'production-media/voices');
const lines=[
 ['v4-hook','Hitting Table Plus’s two-tab free limit? Meet Table eye cue.'],
 ['v4-offer','A free, open-source database client for Mac. Open another table, another query, and keep your work in view.'],
 ['v4-client','Coming from D Beaver? Try the same everyday task here. Browse your data, inspect relationships, and write your S Q L.'],
 ['v4-investigation','Which paid orders haven’t shipped? Follow payments to shipments, review the query, and run it. Two matching orders. There’s your answer.'],
 ['v4-ai','Want help writing S Q L? Use your own A I key for a draft. Review it before you run it. A I is optional.'],
 ['v4-activity','Using a coding agent? Inspect the database queries it sends through Table eye cue.'],
 ['v4-download','Get the free Mac developer preview at Table eye cue dot Ideolyst dot com.'],
];
const engine=new KokoroEngine({dtype:'q8',device:'cpu'});
const lengths=[];
for(const [id,text] of lines){
 const dest=path.join(dir,id+'.wav');
 if(!fs.existsSync(dest)) fs.writeFileSync(dest,await engine.generate(text,{voice:'af_heart',speed:1.05}));
 const h=parseWavHeader(fs.readFileSync(dest));
 lengths.push({id,duration:h.durationMs/1000});
 console.log(id,h.durationMs/1000);
}
for(const [name,rows,key] of [['lines.json',lines,x=>x[0]],['durations.json',lengths,x=>x.id]]){
 const dest=path.join(dir,name);
 const old=JSON.parse(fs.readFileSync(dest)).filter(x=>!String(key(x)).startsWith('v4-'));
 fs.writeFileSync(dest,JSON.stringify([...old,...rows],null,2)+'\n');
}
