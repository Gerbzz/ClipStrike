#[cfg(windows)]
#[ignore]
#[test]
fn windows_launch_smoke_stays_alive() {
    use std::time::Duration;

    let mut child = clipstrike_ui::native_smoke::launch_clipstrike_desktop()
        .expect("desktop app should launch for smoke testing");

    std::thread::sleep(Duration::from_secs(2));
    assert!(
        child
            .try_wait()
            .expect("launcher process should be readable")
            .is_none()
    );
    let _ = child.kill();
}

#[cfg(windows)]
#[ignore]
#[test]
fn windows_import_smoke_inserts_clip() {
    use std::{fs, process::Command};

    let appdata_dir = std::env::temp_dir().join("clipstrike-native-appdata");
    fs::create_dir_all(&appdata_dir).expect("create appdata dir");
    unsafe {
        std::env::set_var("APPDATA", &appdata_dir);
    }

    let fixture_dir = std::env::temp_dir().join("clipstrike-native-import-fixture");
    fs::create_dir_all(&fixture_dir).expect("create import fixture dir");
    let fixture_path = fixture_dir.join("0203 (1)-1.mp4");
    fs::write(&fixture_path, b"clipstrike import fixture").expect("write fixture");
    unsafe {
        std::env::set_var("CLIPSTRIKE_TEST_MEDIA_PATHS", &fixture_path);
    }

    let mut child = clipstrike_ui::native_smoke::launch_clipstrike_desktop()
        .expect("desktop app should launch for smoke testing");

    let pid = child.id();
    let asset_name = "0203 (1)-1.mp4";
    let script = r#"
param([int]$TargetPid)
Add-Type -AssemblyName UIAutomationClient
$root = [System.Windows.Automation.AutomationElement]::RootElement
$pidCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
    $TargetPid
)
$winCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Window
)
$topCond = New-Object System.Windows.Automation.AndCondition($pidCond, $winCond)

$window = $null
for ($i = 0; $i -lt 60 -and -not $window; $i++) {
    $window = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $topCond)
    if (-not $window) { Start-Sleep -Milliseconds 250 }
}
if (-not $window) { throw "ClipStrike window not found for PID $TargetPid" }

$importCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    "Import"
)
$importButton = $null
for ($i = 0; $i -lt 60 -and -not $importButton; $i++) {
    $importButton = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $importCond)
    if (-not $importButton) { Start-Sleep -Milliseconds 250 }
}
if (-not $importButton) { throw "Import button not found" }
$importInvoke = $importButton.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
$importInvoke.Invoke()

$assetCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    "{ASSET_NAME}"
)
$asset = $null
for ($i = 0; $i -lt 80 -and -not $asset; $i++) {
    $asset = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $assetCond)
    if (-not $asset) { Start-Sleep -Milliseconds 250 }
}
if (-not $asset) { throw "Imported asset row not found" }

$row = [System.Windows.Automation.TreeWalker]::ControlViewWalker.GetParent($asset)
if (-not $row) { throw "Could not resolve imported asset row parent" }
$plusCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    "+"
)
$rowPlus = $row.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $plusCond)
if (-not $rowPlus) { throw "Imported asset + button not found" }
$plusInvoke = $rowPlus.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
$plusInvoke.Invoke()

$clipCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    "__clipstrike_test_clip_count__: 1"
)
$clip = $null
for ($i = 0; $i -lt 80 -and -not $clip; $i++) {
    $clip = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $clipCond)
    if (-not $clip) { Start-Sleep -Milliseconds 250 }
}
if (-not $clip) { throw "Expected the clip count label to update after clicking imported asset +" }
"#.replace("{ASSET_NAME}", asset_name);

    let script_path = std::env::temp_dir().join(format!("clipstrike_uia_import_{}.ps1", pid));
    fs::write(&script_path, script).expect("should write UIA script");

    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            script_path
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "-TargetPid",
            &pid.to_string(),
        ])
        .output()
        .expect("should run UIAutomation script");

    let _ = child.kill();
    unsafe {
        std::env::remove_var("CLIPSTRIKE_TEST_MEDIA_PATHS");
    }

    assert!(
        output.status.success(),
        "uia smoke test failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(not(windows))]
#[test]
fn windows_launch_smoke_stays_alive() {
    assert!(true);
}

#[cfg(not(windows))]
#[test]
fn windows_import_smoke_inserts_clip() {
    assert!(true);
}

#[cfg(windows)]
#[ignore]
#[test]
fn windows_recovery_smoke_shows_banner() {
    use std::{fs, process::Command};

    let appdata_dir = std::env::temp_dir().join("clipstrike-native-recovery-appdata");
    fs::create_dir_all(&appdata_dir).expect("create recovery appdata dir");
    unsafe {
        std::env::set_var("APPDATA", &appdata_dir);
    }

    let checkpoint_path = clipstrike_ui::app_config::AppConfig::untitled_recovery_checkpoint_path();
    if let Some(parent) = checkpoint_path.parent() {
        fs::create_dir_all(parent).expect("create recovery dir");
    }
    clipstrike_core::project::Project::new("Recovered", 1920, 1080, 30.0)
        .save(&checkpoint_path)
        .expect("seed recovery checkpoint");

    let mut child = clipstrike_ui::native_smoke::launch_clipstrike_desktop()
        .expect("desktop app should launch for smoke testing");

    let pid = child.id();
    let script = r#"
param([int]$TargetPid)
Add-Type -AssemblyName UIAutomationClient
$root = [System.Windows.Automation.AutomationElement]::RootElement
$pidCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
    $TargetPid
)
$winCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Window
)
$topCond = New-Object System.Windows.Automation.AndCondition($pidCond, $winCond)

$window = $null
for ($i = 0; $i -lt 60 -and -not $window; $i++) {
    $window = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $topCond)
    if (-not $window) { Start-Sleep -Milliseconds 250 }
}
if (-not $window) { throw "ClipStrike window not found for PID $TargetPid" }

$bannerCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    "Recovery checkpoint available"
)
$banner = $null
for ($i = 0; $i -lt 80 -and -not $banner; $i++) {
    $banner = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $bannerCond)
    if (-not $banner) { Start-Sleep -Milliseconds 250 }
}
if (-not $banner) { throw "Recovery banner not found" }

$buttonCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    "Restore Recovery Checkpoint"
)
$button = $null
for ($i = 0; $i -lt 80 -and -not $button; $i++) {
    $button = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $buttonCond)
    if (-not $button) { Start-Sleep -Milliseconds 250 }
}
if (-not $button) { throw "Recovery prompt button not found" }

"#
    .to_string();

    let script_path = std::env::temp_dir().join(format!("clipstrike_uia_recovery_{}.ps1", pid));
    fs::write(&script_path, script).expect("should write UIA script");

    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            script_path
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "-TargetPid",
            &pid.to_string(),
        ])
        .output()
        .expect("should run UIAutomation script");

    let _ = child.kill();
    let _ = fs::remove_file(checkpoint_path);

    assert!(
        output.status.success(),
        "uia smoke test failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(windows)]
#[ignore]
#[test]
fn windows_native_smoke_suite() {
    windows_launch_smoke_stays_alive();
    windows_import_smoke_inserts_clip();
    windows_detection_labels_smoke_updates_label();
    windows_motion_track_smoke_applies_keyframes();
    windows_recovery_smoke_shows_banner();
}

#[cfg(not(windows))]
#[test]
fn windows_native_smoke_suite() {
    assert!(true);
}

#[cfg(windows)]
#[ignore]
#[test]
fn windows_detection_labels_smoke_updates_label() {
    use std::{fs, process::Command};

    let mut child = clipstrike_ui::native_smoke::launch_clipstrike_desktop()
        .expect("desktop app should launch for smoke testing");

    let pid = child.id();
    let script = r#"
param([int]$TargetPid)
Add-Type -AssemblyName UIAutomationClient
$root = [System.Windows.Automation.AutomationElement]::RootElement
$pidCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
    $TargetPid
)
$winCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Window
)
$topCond = New-Object System.Windows.Automation.AndCondition($pidCond, $winCond)

$window = $null
for ($i = 0; $i -lt 60 -and -not $window; $i++) {
    $window = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $topCond)
    if (-not $window) { Start-Sleep -Milliseconds 250 }
}
if (-not $window) { throw "ClipStrike window not found for PID $TargetPid" }

$addCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    "+"
)
$button = $null
for ($i = 0; $i -lt 60 -and -not $button; $i++) {
    $button = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $addCond)
    if (-not $button) { Start-Sleep -Milliseconds 250 }
}
if (-not $button) { throw "Project Files + button not found" }
$invoke = $button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
$invoke.Invoke()

$motionCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    "Apply Detection Labels (Test)"
)
$motionButton = $null
for ($i = 0; $i -lt 60 -and -not $motionButton; $i++) {
    $motionButton = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $motionCond)
    if (-not $motionButton) { Start-Sleep -Milliseconds 250 }
}
if (-not $motionButton) { throw "Detection labels debug button not found" }
$motionInvoke = $motionButton.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
$motionInvoke.Invoke()

$labelsCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    "__clipstrike_test_detection_label_count__: 1"
)
$labels = $null
for ($i = 0; $i -lt 60 -and -not $labels; $i++) {
    $labels = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $labelsCond)
    if (-not $labels) { Start-Sleep -Milliseconds 250 }
}
if (-not $labels) { throw "Expected the detection label count to update after clicking the debug button" }
"#.to_string();

    let script_path = std::env::temp_dir().join(format!("clipstrike_uia_labels_{}.ps1", pid));
    fs::write(&script_path, script).expect("should write UIA script");

    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            script_path
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "-TargetPid",
            &pid.to_string(),
        ])
        .output()
        .expect("should run UIAutomation script");

    let _ = child.kill();

    assert!(
        output.status.success(),
        "uia smoke test failed:
stdout:
{}
stderr:
{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(windows)]
#[ignore]
#[test]
fn windows_motion_track_smoke_applies_keyframes() {
    use std::{fs, process::Command};

    let mut child = clipstrike_ui::native_smoke::launch_clipstrike_desktop()
        .expect("desktop app should launch for smoke testing");

    let pid = child.id();
    let script = r#"
param([int]$TargetPid)
Add-Type -AssemblyName UIAutomationClient
$root = [System.Windows.Automation.AutomationElement]::RootElement
$pidCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
    $TargetPid
)
$winCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Window
)
$topCond = New-Object System.Windows.Automation.AndCondition($pidCond, $winCond)

$window = $null
for ($i = 0; $i -lt 60 -and -not $window; $i++) {
    $window = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $topCond)
    if (-not $window) { Start-Sleep -Milliseconds 250 }
}
if (-not $window) { throw "ClipStrike window not found for PID $TargetPid" }

$addCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    "+"
)
$button = $null
for ($i = 0; $i -lt 60 -and -not $button; $i++) {
    $button = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $addCond)
    if (-not $button) { Start-Sleep -Milliseconds 250 }
}
if (-not $button) { throw "Project Files + button not found" }
$invoke = $button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
$invoke.Invoke()

$motionCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    "Apply Motion Track (Test)"
)
$motionButton = $null
for ($i = 0; $i -lt 60 -and -not $motionButton; $i++) {
    $motionButton = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $motionCond)
    if (-not $motionButton) { Start-Sleep -Milliseconds 250 }
}
if (-not $motionButton) { throw "Motion track debug button not found" }
$motionInvoke = $motionButton.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
$motionInvoke.Invoke()

$trackCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    "__clipstrike_test_motion_track_count__: 1"
)
$track = $null
for ($i = 0; $i -lt 80 -and -not $track; $i++) {
    $track = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $trackCond)
    if (-not $track) { Start-Sleep -Milliseconds 250 }
}
if (-not $track) { throw "Expected the motion track label to update after clicking the debug button" }
"#.to_string();

    let script_path = std::env::temp_dir().join(format!("clipstrike_uia_motion_{}.ps1", pid));
    fs::write(&script_path, script).expect("should write UIA script");

    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            script_path
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "-TargetPid",
            &pid.to_string(),
        ])
        .output()
        .expect("should run UIAutomation script");

    let _ = child.kill();

    assert!(
        output.status.success(),
        "uia smoke test failed:
stdout:
{}
stderr:
{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
