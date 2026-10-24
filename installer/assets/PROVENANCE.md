# Installer artwork provenance

- Created: 2026-10-24.
- Tool: built-in imagegen (not CLI/API fallback).
- Approved reference: `docs/design/installer-reference.png`.
- Production asset: `installer/assets/character.png`, 1536 × 1024, RGBA with genuine transparent pixels.
- Art direction: original adult anime woman with silver-white hair, blue eyes, a snowflake ornament, fully covering white/ice-blue winter coat, reaching hand; restrained SnowDown ice-blue palette.

Final asset prompt:

> Use case: background-extraction. Edit target: supplied SnowDown installer concept image. Extract ONLY the original anime woman at the left as a production transparent PNG asset for this exact installer. Preserve her identity, face, silver-white hair, blue eyes, small snowflake hair ornament, white and pale blue fully covering winter coat, reaching hand, pose, detailed anime drawing, soft clean colors. Preserve the original character artwork as closely as possible. Preserve the original canvas aspect ratio 1536:1024 and her original placement and size on the LEFT of that canvas; leave the RIGHT half empty and transparent so live UI controls can be placed there. Keep the original complete visible hair and hand silhouette and lower coat edge. Remove EVERYTHING ELSE: the desktop background, pale blue enclosing swoosh, white right-side installation panel, panel outline, all text, all logo, buttons, checkbox, path, minimize and close icons, any shadows on the former background, floating snowflake decorations outside the person. Transparent background with real alpha, including between separated hair locks and outside coat and hand contours, no checkerboard drawn into the pixels, no rectangular background. No new character, no redesign, no text, no UI. This is the final cutout illustration asset, not another UI mockup.

Typography, borders, panel curves, icons and all interactive elements are drawn by application code, rather than burned into the bitmap. `docs/design/installer-ready.png`, `installer-progress.png` and `installer-complete.png` are off-screen renders of the implemented interface.
