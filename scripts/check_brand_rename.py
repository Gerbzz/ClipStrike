"""Lightweight display-name and compatibility contracts; no native build required."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]

def read(path):
    return (ROOT / path).read_text(encoding="utf-8")

class RenameContract(unittest.TestCase):
    def test_current_display_name(self):
        main = read('apps/filmcraft/src/main.rs')
        self.assertIn('.with_title("FilmStrike")', main)
        self.assertIn('.set_title("FilmStrike")', main)
        self.assertIn('"FILMSTRIKE".to_owned()', read('crates/ui-egui/src/brand.rs'))
        self.assertIn('ui.heading("FilmStrike")', read('crates/ui-egui/src/panels/dialogs.rs'))
        build = read('apps/filmcraft/build.rs')
        self.assertIn('"ProductName", "FilmStrike"', build)
        self.assertIn('"FileDescription", "FilmStrike video editor"', build)

    def test_native_menu_display_name(self):
        menu = read("apps/filmcraft/src/native_menu.rs")
        for marker in ['Submenu::new("FilmStrike", true)',
                       '"app.about", "About FilmStrike"',
                       'hide(Some("Hide FilmStrike"))',
                       'quit(Some("Quit FilmStrike"))']:
            self.assertIn(marker, menu)

    def test_stable_launch_and_storage_identity(self):
        main = read('apps/filmcraft/src/main.rs')
        for marker in ['"local.clipstrike.editor"', 'env("CLIPSTRIKE_DATA_DIR")',
                       'join("ClipStrike")', 'join("Library/Application Support/ClipStrike")',
                       'join("clipstrike")', 'join(".local/share/clipstrike")',
                       'var_os("CLIPSTRIKE_CONTROL_PORT")']:
            self.assertIn(marker, main)
        self.assertRegex(main, r'eframe::run_native\(\s*"ClipStrike"')
        cargo = read('apps/filmcraft/Cargo.toml')
        for name in ['filmcraft', 'clipstrike', 'clipstrike-audio-next', 'clipstrike-seek-next']:
            self.assertIn(f'name = "{name}"', cargo)
        self.assertIn('default-run = "clipstrike"', cargo)
        self.assertIn('"OriginalFilename", "clipstrike.exe"', read('apps/filmcraft/build.rs'))
        self.assertIn('pub const LOG_FILE: &str = "clipstrike.log"', read('apps/filmcraft/src/logging.rs'))

    def test_project_and_persistent_widget_identity(self):
        legacy = read('apps/filmcraft/src/legacy.rs')
        for marker in ['"clipstrike.asset"', '"clipstrike.capture_sources"', 'ClipStrike legacy source JSON']:
            self.assertIn(marker, legacy)
        self.assertIn('egui::Id::new("About ClipStrike")', read('crates/ui-egui/src/panels/dialogs.rs'))
        self.assertIn('egui::Id::new("ClipStrike hit an error")', read('crates/ui-egui/src/lib.rs'))
        self.assertIn('"FilmStrike Project", "fcproj"', read('crates/ui-egui/src/lib.rs'))

    def test_translations_and_documented_legacy_commands(self):
        for path in (ROOT / 'crates/ui-egui/src/i18n').glob('*.tsv'):
            self.assertNotIn('ClipStrike', path.read_text(encoding="utf-8"), str(path))
        readme = read('README.md')
        self.assertIn('FilmStrike (formerly ClipStrike)', readme)
        self.assertIn('--bin clipstrike', readme)
        self.assertIn('assets/branding/ClipStrike.png', readme)
        self.assertIn('historical image', readme)

if __name__ == '__main__':
    unittest.main()
