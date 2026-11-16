import {defineConfig,engines} from '@argo-video/cli';
export default defineConfig({
  demosDir:'demos',outputDir:'videos',
  tts:{engine:engines.kokoro({dtype:'q8',device:'cpu'}),defaultVoice:'af_heart',defaultSpeed:1.05},
  video:{width:1920,height:1080,fps:30,deviceScaleFactor:1},
  // Stems are already levelled; final normalization would raise the quiet bed.
  export:{preset:'slow',crf:17,audio:{loudnorm:false}},
  overlays:{autoBackground:false}
});
