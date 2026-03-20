@echo off
setlocal
set "GST_ROOT=%~dp0gst_extracted\PFiles64\gstreamer\1.0\msvc_x86_64"
if not exist "%GST_ROOT%\bin\glib-2.0-0.dll" (
  set "GST_ROOT=%ProgramFiles%\gstreamer\1.0\msvc_x86_64"
)
if not exist "%GST_ROOT%\bin\glib-2.0-0.dll" (
  echo GStreamer runtime not found in local gst_extracted or Program Files.
  exit /b 1
)
set "PATH=%GST_ROOT%\bin;%PATH%"
set "GST_PLUGIN_SYSTEM_PATH=%GST_ROOT%\lib\gstreamer-1.0"
set "GST_PLUGIN_PATH=%GST_ROOT%\lib\gstreamer-1.0"
set "PKG_CONFIG=%GST_ROOT%\bin\pkg-config.exe"
set "PKG_CONFIG_PATH=%GST_ROOT%\lib\pkgconfig;%GST_ROOT%\lib\gstreamer-1.0\pkgconfig"
"%USERPROFILE%\.cargo\bin\cargo.exe" run -p clipstrike-app --features gstreamer-backend
