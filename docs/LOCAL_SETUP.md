# ClipStrike Local Setup

This repo is developed on Windows with two known GStreamer/MSVC SDK locations:

- Installed SDK: `C:\Program Files\gstreamer\1.0\msvc_x86_64`
- Repo-local fallback: `W:\Youtube\clipstrike\gst_extracted\PFiles64\gstreamer\1.0\msvc_x86_64`

Use the launcher script when you want the repo to configure the runtime automatically:

- `run_clipstrike_gstreamer.cmd`

## Backend Environment

When running GStreamer-backed Cargo commands in a fresh shell, set these variables to the SDK root:

```powershell
$env:GST_ROOT = 'C:\Program Files\gstreamer\1.0\msvc_x86_64'
$env:PATH = "$env:GST_ROOT\bin;$env:PATH"
$env:GST_PLUGIN_SYSTEM_PATH = "$env:GST_ROOT\lib\gstreamer-1.0"
$env:GST_PLUGIN_PATH = "$env:GST_ROOT\lib\gstreamer-1.0"
$env:PKG_CONFIG = "$env:GST_ROOT\bin\pkg-config.exe"
$env:PKG_CONFIG_PATH = "$env:GST_ROOT\lib\pkgconfig;$env:GST_ROOT\lib\gstreamer-1.0\pkgconfig"
$env:GSTREAMER_1_0_ROOT_MSVC_X86_64 = $env:GST_ROOT
```

## Sanity Checks

Verify the SDK is visible before running backend tests:

```powershell
& "$env:PKG_CONFIG" --modversion glib-2.0
& "$env:PKG_CONFIG" --modversion gstreamer-1.0
& "$env:GST_ROOT\bin\gst-inspect-1.0.exe" --version
```

If those commands succeed, the Windows GStreamer toolchain is available for backend export and playback verification.
