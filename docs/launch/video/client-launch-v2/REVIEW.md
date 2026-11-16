# v5 audio review

The founder selected soft, very quiet background music with clear narration. The approved picture and script are preserved. Each v5 H.264 picture payload matches its v4 master exactly; all three remain 75s/1080p/30fps. The guided walkthrough is first on the site.

The original raw voice was around−22.9dBFS RMS while the music, after its 0.14 gain, was around−32.9dBFS: only about 10 dB separation. The new prepared stems measure approximately 25 dB separation during active speech. Narration uses the same speaker and words, 4% slower pitch-preserving delivery, gentle tonal softening, and−17 LUFS utterance levelling. Music uses a filtered, compressed−36 LUFS base and another 6 dB reduction around utterances, with smooth ramps. The motion-only score is−30 LUFS. Final muxing preserves these relationships; there is no global gain boost.

The words and pronunciation inputs are unchanged. Caption times are scaled to the retimed audio. The query/Run/result timings still fit the original evidence sequence. Official Argo alignment reports zero overflow. Film-specific checks and encoded media measurements are in `delivery/technical-verification-v5.json`.

Audio has been measured and compared, not independently listened to. The founder’s listening feedback is the final judgment of tone. The backing remains the same original TableIQ score, softened; a different melody or recorded narrator would require a separate creative choice.

Historical reviews, failed iterations and redundant rendered screenshots were removed from the current source tree. Previous source packages remain available on the GitHub release and in Git history. Current native AI and MCP provenance is retained.
