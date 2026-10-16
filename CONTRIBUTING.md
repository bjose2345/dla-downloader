# Contributing

Thanks for helping improve DLA Downloader.

## Development rules

- Use the exact dependency versions already pinned in the workspace.
- Network, filesystem and account work stays in Rust behind Tauri commands; the
  interface receives views, never paths it did not get from Rust or
  credentials.
- Put every interface string into all eight catalogs in
  `tauri2/ui/src/i18n/locales/`; `python3 tauri2/tools/i18n.py check` must pass.
- Do not commit credentials, service tokens, signing keys, logs or downloaded
  archives.
- Add focused tests for changed behaviour and run the checks in
  [Building](docs/BUILDING.md#checks).

## Pull requests

Explain the user-visible outcome, important design choices and the checks you
ran. Keep unrelated formatting or refactors out of the same change.

By contributing, you agree that your contribution is licensed under
AGPL-3.0-only with the rest of the project.
