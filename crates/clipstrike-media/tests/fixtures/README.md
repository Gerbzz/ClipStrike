# Synthetic media fixture

`preview-with-audio.mp4` contains a generated moving test pattern and a generated 440 Hz tone. It contains no gameplay capture, imported audio, or user media. ClipStrike's media and editor export tests embed this small file at compile time.

Generated on 2026-10-11 UTC with `ffmpeg version 8.0.1-full_build-www.gyan.dev` from FFmpeg's `testsrc2` and `sine` lavfi sources. From this directory, with no existing output file:

```powershell
ffmpeg -hide_banner -loglevel error -n -f lavfi -i 'testsrc2=size=320x180:rate=10:duration=2' -f lavfi -i 'sine=frequency=440:sample_rate=44100:duration=2' -map 0:v:0 -map 1:a:0 -map_metadata -1 -t 2 -c:v mpeg4 -q:v 4 -pix_fmt yuv420p -r 10 -c:a aac -b:a 96k -ar 44100 -ac 1 -movflags +faststart preview-with-audio.mp4
```

SHA-256: `B280623E91600F615689B6C49D271EE3751E97C1305267C82B820E5D10C514D9` (119,096 bytes). A second run with the same command and FFmpeg build produced the same hash. FFprobe reports 2.000 seconds, 320×180 MPEG-4 video at 10 fps, and mono 44.1 kHz AAC audio.
