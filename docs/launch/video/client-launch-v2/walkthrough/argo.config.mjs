import {defineConfig,engines} from '@argo-video/cli';
export default defineConfig({
  demosDir:'demos',outputDir:'videos',
  tts:{engine:engines.kokoro({dtype:'q8',device:'cpu'}),defaultVoice:'af_heart',defaultSpeed:1.05},
  video:{width:1920,height:1080,fps:30,deviceScaleFactor:1},
  export:{preset:'slow',crf:17,audio:{loudnorm:true}},
  overlays:{autoBackground:false}
});
