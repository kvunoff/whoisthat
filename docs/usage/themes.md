# Themes & Responsive Layout

WhoisThat is crafted for terminal aesthetics and responsiveness, featuring curated color palettes and an adaptive layout engine that tailors the interface to your window dimensions.

---

## Curated Color Themes

WhoisThat includes 4 curated palettes designed for high readability, contrast, and visual harmony:

| Theme Name | Style & Atmosphere | Accent Highlights |
| :--- | :--- | :--- |
| **Graphite Dark** *(Default)* | Modern monochrome graphite & slate aesthetic with obsidian depth | Platinum silver, crisp white, emerald |
| **Graphite Light** | Minimalist architectural monochrome paper & chalk light theme | Ink black, charcoal, deep forest green |
| **Tokyo Night** | Refined deep midnight blue background inspired by Tokyo at dusk | Neon cyan, soft sky blue, coral red |
| **Dracula** | Vibrant gothic contrast with deep vampire purple tones | Neon pink, electric purple, spring green |

### Switching Themes
1. Open the Settings tab (<kbd>5</kbd> or cycle with <kbd>[</kbd> / <kbd>]</kbd>).
2. Highlight the **Theme** option.
3. Press <kbd>Enter</kbd>, <kbd>Space</kbd>, or <kbd>→</kbd> / <kbd>l</kbd> to cycle forward, or <kbd>←</kbd> / <kbd>h</kbd> to cycle backward with **instant live preview**.
4. The selected theme is saved persistently to `~/.config/whoisthat/config.toml`.

---

## Responsive Adaptive Layout Engine

Terminals come in all shapes and sizes—from a compact 80x24 window to tiled splits on an ultrawide monitor. WhoisThat features a custom viewport geometry engine that classifies window dimensions into tiers:

### Width Tiers
* **Compact (< 80 columns)**: Automatically collapses side-by-side splits into a full-width single panel. Press <kbd>Tab</kbd> to switch focus between the Profile Tree and Server Details views.
* **Medium (80–125 columns)**: Balanced side-by-side layout (40% tree, 60% details/routing). Press <kbd>Tab</kbd> to toggle focus between panes.
* **Wide (126–160 columns)**: Expanded details view with full server parameters, TLS properties, and multi-column statistics.
* **UltraWide (> 160 columns)**: Dual-column details cards, extended traffic history, and verbose routing paths.

### Height Tiers
* **Tiny (< 16 rows)**: Compresses header and hides auxiliary cards to maximize server list visibility.
* **Short (16–24 rows)**: Standard layout with compact status footer.
* **Normal (25–42 rows)**: Full layout including latency metrics, subscription quota bars, and interface cards.
* **Tall (> 42 rows)**: Expanded logs pane and 60-second full-height traffic charts.

### Terminal Safety Threshold
If your terminal window is resized below **45 columns × 8 rows**, rendering is temporarily suspended, and a clean minimum-size warning is displayed to prevent buffer overflow and visual artifacts.
