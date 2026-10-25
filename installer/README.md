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

The build uses the system .NET Framework C# compiler and embeds the layout, artwork, app icon and NSIS payload. Building also requires 7-Zip (`7z.exe`/`7zz.exe` on PATH, the standard Program Files installation, or `SNOWDOWN_7ZIP` pointing to the executable). The installed-file manifest is hashed from files extracted from the actual NSIS payload: Tauri changes the executable's bundle-type marker during packaging and restores the standalone build afterward, so hashing `target/release/snowdown.exe` would incorrectly reject a successful installation. `scripts/build-installer.ps1` also runs installer core checks and verifies payload extraction. Windows 10/11 with .NET Framework 4.6.2+ is required (normally included with supported Windows versions); end users do not need 7-Zip.

## Verification

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tests/installer-manifest.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tests/installer-payload.ps1 -InstallerPath release/SnowDown_0.2.7_x64-setup.exe
powershell -NoProfile -ExecutionPolicy Bypass -File tests/installer-integration.ps1
```

The manifest regression reads the archived public v0.2.5 installer and verifies the digest of its actual packaged EXE plus all 16 resources. Recompiling its NSIS script against the restored standalone EXE does not reproduce Tauri's packaging-time binary patch.

The production-payload check reads the final distributable, validates its embedded payload digest, extracts its actual files and calls that installer's own `VerifyManifest`. Normal builds run this check automatically; no installation or user registry changes are performed by this check.

The integration script builds a payload with a unique QA product name and registry identity, installs through the shaped frontend into an isolated Unicode path, verifies file integrity and registration, reinstalls, checks the real completion phase, and uninstalls. It verifies that an unrelated sentinel file survives both reinstall and uninstall. It does not install over the user's SnowDown or change their account data.

`--verify-payload` verifies embedded payload extraction. `/S "/D=D:\Apps\SnowDown"` supports silent deployment. `--render-preview=<absolute-path>` renders the program's own layout off-screen for visual QA; an optional second argument `progress` or `complete` selects another state. It performs no installation or desktop interaction.

## Artwork

The user approved `docs/design/installer-reference.png` on 2026-10-24. The illustration was extracted with the built-in imagegen tool, preserving the original character and removing the UI/background. The final prompt and provenance are in `assets/PROVENANCE.md`. No remote assets are needed at runtime.
