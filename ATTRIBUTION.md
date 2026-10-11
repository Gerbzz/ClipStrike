# Bundled source and asset attribution

This selected source subset is based on FilmCraft commit `2ff3f5e266838700c292a35aaf762e3d4dcef980`. The donor crates retain their MIT/Apache license and original source notices; see `LICENSE-MIT`, `LICENSE-APACHE`, and `NOTICE`. The first-party ClipStrike editor changes have no selected license grant.

| Bundled item | Origin | Terms |
| --- | --- | --- |
| `assets/fonts/Inter-*.ttf` | The Inter Project Authors | OFL-1.1, `assets/fonts/OFL-Inter.txt` |
| `assets/fonts/JetBrainsMono-Regular.ttf` | The JetBrains Mono Project Authors | OFL-1.1, `assets/fonts/OFL-JetBrainsMono.txt` |
| `assets/fonts/NotoSerif-Regular.ttf` | The Noto Project Authors | OFL-1.1, `assets/fonts/OFL-NotoSerif.txt` |
| `crates/ui-egui/src/i18n/fr.tsv`, `ru.tsv`, `uk.tsv` | FilmCraft contributors; details in adjacent `.attribution` files | Donor MIT OR Apache-2.0 |
| Donor code-drawn UI icons, media generators, graphics templates and procedural looks | FilmCraft contributors; source files under selected `crates/` | Donor MIT OR Apache-2.0 |
| `crates/platform/src/nvenc/ffi.rs` | NVIDIA `nvEncodeAPI.h` structures transcribed by FilmCraft contributors | MIT notice retained in file header |
| `apps/filmcraft/tests/fixtures/synthetic-chronological.cstrike` | New, invented ClipStrike importer test data; no match or captured media | First-party license not selected |
| Code-drawn ClipStrike wordmark and window icon | ClipStrike contributors; `crates/ui-egui/src/brand.rs`, `apps/filmcraft/src/main.rs` | First-party license not selected |

Each bundled font has an adjacent `.attribution` sidecar and its OFL text. The importer fixture also has a sidecar describing its synthetic origin. Optional `craft-fonts` input is not bundled. Upstream ArtCraft marks, donor app icons, screenshots, promotional media and reference projects are excluded.
