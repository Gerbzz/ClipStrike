# FilmStrike display-name migration

FilmStrike is the current display name of the editor formerly called ClipStrike.
This patch changes UI labels, translations, the code-drawn wordmark, Windows product
and description fields, and current introductory documentation. The approved
kingfisher artwork is unchanged, including its original filenames and provenance.
Historical screenshots and source/license attribution retain the name they recorded.

## Compatibility contract

This is deliberately not a data, package, executable or repository migration:

- Continue invoking `clipstrike`, `clipstrike-audio-next` and `clipstrike-seek-next`;
  no `filmstrike` executable alias is added. Cargo packages remain `filmcraft*`.
- `CLIPSTRIKE_DATA_DIR`, `CLIPSTRIKE_CONTROL_PORT` and `CLIPSTRIKE_REQUIRE_WINRES`
  keep their meanings. Command-line parsing and version output are unchanged.
- `local.clipstrike.editor`, the legacy eframe application name and explicit
  persistent widget IDs remain stable.
- Windows `%APPDATA%/ClipStrike`, macOS `~/Library/Application Support/ClipStrike`,
  and XDG `clipstrike` data locations remain unchanged. Preferences, autosaves,
  recovery files and `clipstrike*.log` files are not relocated or rewritten.
- `.fcproj`, `.fcgt`, `.cstrike`, embedded metadata keys, importer provenance,
  media references and source files are unchanged. Existing projects need no conversion.
- `Gerbzz/ClipStrike` remains the repository. Historical revisions, especially
  `fdf2f6ac8828374b803577ae1f6b310b73860075` used by DemoStrike, are retained.
- PWDS launcher's `clipstrike` catalog ID, repository mapping and executable paths
  remain valid. Updating its visible catalog label is a separate coordinated change.
- Existing Office checkouts, binaries and active video jobs are not changed by this
  source patch. Install/deploy only after their owner coordinates a safe update.

Reverting this display-only patch restores the old labels without moving user data.

## Separate repository rename decision

An administrative rename to `Gerbzz/FilmStrike` is not part of this patch. Before
that change, recheck namespace availability, external release consumers, launcher's
repository allowlist, pinned Git dependencies, CI actions and any Pages site.
GitHub normally redirects repository web URLs and Git operations, but does not
redirect hosted Action references or project-site URLs. Never recreate a repository
at the old name: that breaks redirects. Preserve historical pins and verify a fresh
DemoStrike dependency resolution before updating canonical URLs.

Reference: [GitHub repository rename documentation](https://docs.github.com/en/repositories/creating-and-managing-repositories/renaming-a-repository).
