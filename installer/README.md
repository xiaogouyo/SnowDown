# SnowDown shaped installer

The public `SnowDown_<version>_x64-setup.exe` is a self-contained WPF frontend for the Tauri NSIS payload. It uses the Windows .NET Framework, with no additional .NET Desktop runtime download. The app itself remains Tauri/Rust/Vue.

`Installer.xaml` defines a transparent, borderless window, live path field, folder picker, custom buttons and install/progress/completion states. `assets/character.png` contains the original anime illustration with alpha transparency. The UI scales to fit the desktop work area. Background and character regions allow dragging; the controls remain keyboard accessible.

`InstallerCore.cs` validates paths and disk space, verifies the embedded payload SHA256 before running it, starts NSIS silently, validates installed files and removes only its own unique temporary directory. NSIS still owns WebView2 detection, file copying, registry entries, shortcuts and uninstall behavior. Actual installation phases are reported through a per-run status file. No invented percentage is displayed. A running target client is reported to the user rather than terminated by the shaped installer's backend.

## Build

```powershell
npm run desktop:build
# Rebuild only the frontend after bundling the NSIS payload:
npm run installer:build
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-portable.ps1
```

The public installer is built at `src-tauri/target/release/bundle/snowdown/`. The internal NSIS payload remains in `bundle/nsis/`; publish the shaped frontend from `bundle/snowdown/`.

The build uses the system .NET Framework C# compiler and embeds the layout, artwork, app icon and NSIS payload. `scripts/build-installer.ps1` also runs installer core checks and verifies payload extraction. Windows 10/11 with .NET Framework 4.6.2+ is required (normally included with supported Windows versions).

## Verification

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tests/installer-integration.ps1
```

The integration script builds a payload with a unique QA product name and registry identity, installs through the shaped frontend into an isolated Unicode path, verifies file integrity and registration, reinstalls, checks the real completion phase, and uninstalls. It verifies that an unrelated sentinel file survives both reinstall and uninstall. It does not install over the user's SnowDown or change their account data.

`--verify-payload` verifies embedded payload extraction. `/S "/D=D:\Apps\SnowDown"` supports silent deployment. `--render-preview=<absolute-path>` renders the program's own layout off-screen for visual QA; an optional second argument `progress` or `complete` selects another state. It performs no installation or desktop interaction.

## Artwork

The user approved `docs/design/installer-reference.png` on 2026-10-24. The illustration was extracted with the built-in imagegen tool, preserving the original character and removing the UI/background. The final prompt and provenance are in `assets/PROVENANCE.md`. No remote assets are needed at runtime.
