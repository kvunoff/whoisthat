# Themes & Responsive Layout

WhoisThat is crafted for terminal aesthetics and responsiveness, featuring curated color palettes and an adaptive layout engine that tailors the interface to your window dimensions.

---

## Curated Color Themes

WhoisThat includes 8 dark palettes designed for high readability, contrast, and visual harmony:

| Theme Name | Style & Atmosphere | Accent Highlights |
| :--- | :--- | :--- |
| **Tokyo Night** *(Default)* | Deep midnight blue background inspired by Tokyo at dusk | Neon cyan, soft purple, magenta |
| **Catppuccin Mocha** | Soothing pastel palette on dark velvety mocha | Lavender, sky blue, peach |
| **Nord** | Arctic-inspired cool blue and slate grays | Frost teal, polar night, snow storm |
| **Dracula** | Vibrant gothic contrast with deep purple tones | Bright pink, purple, neon green |
| **Gruvbox Dark** | Retro groove with earthy, warm tones | Warm amber, forest green, terracotta |
| **Solarized Dark** | Precision-engineered palette with low eyestrain | Solar cyan, blue, olive |
| **Cyberpunk** | High-energy synthwave neon | Electric yellow, hot magenta, cyan |
| **Monokai Pro** | Professional code editor palette with balanced contrast | Sunshine yellow, rose, mint green |

### Switching Themes
1. Open the Settings tab (<kbd>5</kbd> or <kbd>s</kbd>).
2. Highlight the **Theme** option.
3. Press <kbd>Enter</kbd> or <kbd>Space</kbd> to cycle themes with **instant live preview**.
4. The selected theme is saved persistently to `~/.config/whoisthat/config.toml`.

---

## Responsive Adaptive Layout Engine

Terminals come in all shapes and sizes—from a compact 80x24 window to tiled splits on an ultrawide monitor. WhoisThat features a custom viewport geometry engine that classifies window dimensions into tiers:

### Width Tiers
* **Compact (< 80 columns)**: Automatically collapses side-by-side splits into a full-width single panel. Press <kbd>Tab</kbd> to toggle between the Profile Tree and Server Details.
* **Medium (80–125 columns)**: Balanced side-by-side layout (40% tree, 60% details/routing).
* **Wide (126–160 columns)**: Expanded details view with full server parameters, TLS properties, and multi-column statistics.
* **UltraWide (> 160 columns)**: Dual-column details cards, extended traffic history, and verbose routing paths.

### Height Tiers
* **Tiny (< 16 rows)**: Compresses header and hides auxiliary cards to maximize server list visibility.
* **Short (16–24 rows)**: Standard layout with compact status footer.
* **Normal (25–42 rows)**: Full layout including latency metrics, subscription quota bars, and interface cards.
* **Tall (> 42 rows)**: Expanded logs pane and 60-second full-height traffic charts.

### Terminal Safety Threshold
If your terminal window is resized below **45 columns × 8 rows**, rendering is temporarily suspended, and a clean minimum-size warning is displayed to prevent buffer overflow and visual artifacts.
