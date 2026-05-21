use egui::Color32;

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub name: &'static str,
    pub bg:        Color32,
    pub grid:      Color32,
    pub edge:      Color32,
    pub edge_hi:   Color32,
    pub node:      Color32,
    pub node_rim:  Color32,
    pub node_hov:  Color32,
    pub node_sel:  Color32,
    pub node_selr: Color32,
    pub label:     Color32,
    pub label_hov: Color32,
    pub bar_bg:    Color32,
    pub bar_line:  Color32,
    pub bar_text:  Color32,
}

impl Theme {
    pub fn glow(&self) -> Color32 {
        let c = self.node;
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 28)
    }
}

// ── Navi palette ──────────────────────────────────────────────────────────────
// Source: macro photo of a black butterfly on vivid blue salvia flowers with a
// glowing cyan bokeh orb.  All five Navi variants share the same background
// architecture, bar, labels, and amber complement anchor.  Only nodes, edges,
// and highlights differ — each drawn from a different family in the palette.

// Shared background infrastructure (same across all Navi variants)
//   bg       #101e2e  Deep Night Blue    (btop main_bg)
//   grid     #192e48  Dark Cerulean      (btop meter_bg — one luminance step up)
//   bar_bg   #0a1c34  Oxford Blue        (btop inactive_bg)
//   bar_line #265080  Payne's Grey blue  (btop div_line)
//   bar_text #a8c8d8  Powder Blue        (btop graph_text)
//   label    #7898b8  Cadet Grey         (btop inactive_fg)
//   label_hov#e0f0ff  Alice Blue         (btop main_fg)
//   node_sel #f09030  Amber              (complement anchor — the one warm hue)

pub const THEMES: &[Theme] = &[
    // ── 0 · Salvia ────────────────────────────────────────────────────────────
    // Periwinkle nodes / Electric Cyan ghost-light rim.
    // The salvia flowers (electric periwinkle #5578ff) meet the bokeh orb
    // (#40e8ff) — the signature Navi look.
    Theme {
        name: "Salvia",
        bg:        Color32::from_rgb( 16,  30,  46),  // #101e2e Deep Night Blue
        grid:      Color32::from_rgb( 25,  46,  72),  // #192e48 Dark Cerulean
        edge:      Color32::from_rgb( 38,  80, 128),  // #265080 Payne's Grey blue
        edge_hi:   Color32::from_rgb( 64, 232, 255),  // #40e8ff Electric Cyan
        node:      Color32::from_rgb( 85, 120, 255),  // #5578ff Electric Periwinkle
        node_rim:  Color32::from_rgb( 64, 232, 255),  // #40e8ff Electric Cyan
        node_hov:  Color32::from_rgb(  0, 216, 248),  // #00d8f8 Vivid Cerulean
        node_sel:  Color32::from_rgb(240, 144,  48),  // #f09030 Amber
        node_selr: Color32::from_rgb(156, 242, 255),  // #9cf2ff soft cyan — rim brightened
        label:     Color32::from_rgb(120, 152, 184),  // #7898b8 Cadet Grey
        label_hov: Color32::from_rgb(224, 240, 255),  // #e0f0ff Alice Blue
        bar_bg:    Color32::from_rgb( 10,  28,  52),  // #0a1c34 Oxford Blue
        bar_line:  Color32::from_rgb( 38,  80, 128),  // #265080 Payne's Grey blue
        bar_text:  Color32::from_rgb(168, 200, 216),  // #a8c8d8 Powder Blue
    },
    // ── 1 · Bokeh ─────────────────────────────────────────────────────────────
    // Pure cyan family — the glowing bokeh orb made into nodes.
    // Pacific Blue nodes (#00b8d8) ringed in Electric Cyan (#40e8ff).
    // Edges pull from Deep Teal (#023040), the darkest cyan bg in the palette.
    Theme {
        name: "Bokeh",
        bg:        Color32::from_rgb( 16,  30,  46),  // #101e2e
        grid:      Color32::from_rgb( 25,  46,  72),  // #192e48
        edge:      Color32::from_rgb( 26,  96, 112),  // #1a6070 mid teal
        edge_hi:   Color32::from_rgb( 64, 232, 255),  // #40e8ff Electric Cyan
        node:      Color32::from_rgb(  0, 184, 216),  // #00b8d8 Pacific Blue
        node_rim:  Color32::from_rgb( 64, 232, 255),  // #40e8ff Electric Cyan
        node_hov:  Color32::from_rgb(  0, 216, 248),  // #00d8f8 Vivid Cerulean
        node_sel:  Color32::from_rgb(240, 144,  48),  // #f09030 Amber
        node_selr: Color32::from_rgb(156, 242, 255),  // #9cf2ff soft cyan — rim brightened
        label:     Color32::from_rgb(120, 152, 184),  // #7898b8 Cadet Grey
        label_hov: Color32::from_rgb(224, 240, 255),  // #e0f0ff Alice Blue
        bar_bg:    Color32::from_rgb( 10,  28,  52),  // #0a1c34 Oxford Blue
        bar_line:  Color32::from_rgb( 38,  80, 128),  // #265080 Payne's Grey blue
        bar_text:  Color32::from_rgb(168, 200, 216),  // #a8c8d8 Powder Blue
    },
    // ── 2 · Stem ──────────────────────────────────────────────────────────────
    // The cool stem greens beneath the salvia flowers.
    // Caribbean Green nodes (#00e898) / Aquamarine rim (#00f0b0).
    // Edges from Hunter Green (#052818) — darkest green bg in the palette.
    Theme {
        name: "Stem",
        bg:        Color32::from_rgb( 16,  30,  46),  // #101e2e
        grid:      Color32::from_rgb( 25,  46,  72),  // #192e48
        edge:      Color32::from_rgb( 20, 100,  56),  // #146438 mid green
        edge_hi:   Color32::from_rgb(  0, 240, 176),  // #00f0b0 Aquamarine
        node:      Color32::from_rgb(  0, 232, 152),  // #00e898 Caribbean Green
        node_rim:  Color32::from_rgb(  0, 240, 176),  // #00f0b0 Aquamarine
        node_hov:  Color32::from_rgb( 64, 216, 112),  // #40d870 Malachite
        node_sel:  Color32::from_rgb(240, 144,  48),  // #f09030 Amber
        node_selr: Color32::from_rgb(128, 248, 216),  // #80f8d8 soft aqua — rim brightened
        label:     Color32::from_rgb(120, 152, 184),  // #7898b8 Cadet Grey
        label_hov: Color32::from_rgb(224, 240, 255),  // #e0f0ff Alice Blue
        bar_bg:    Color32::from_rgb( 10,  28,  52),  // #0a1c34 Oxford Blue
        bar_line:  Color32::from_rgb( 38,  80, 128),  // #265080 Payne's Grey blue
        bar_text:  Color32::from_rgb(168, 200, 216),  // #a8c8d8 Powder Blue
    },
    // ── 3 · Petal ─────────────────────────────────────────────────────────────
    // The iridescent wing of the black butterfly — violet and magenta.
    // Amethyst nodes (#a868d8) / Electric Violet rim (#c040f8).
    // Edges from Violet Black (#1e0a48) — darkest magenta bg in the palette.
    Theme {
        name: "Petal",
        bg:        Color32::from_rgb( 16,  30,  46),  // #101e2e
        grid:      Color32::from_rgb( 25,  46,  72),  // #192e48
        edge:      Color32::from_rgb( 72,  24, 144),  // #481890 Dark Indigo
        edge_hi:   Color32::from_rgb(224,  48, 255),  // #e030ff Electric Magenta
        node:      Color32::from_rgb(168, 104, 216),  // #a868d8 Amethyst
        node_rim:  Color32::from_rgb(192,  64, 248),  // #c040f8 Electric Violet
        node_hov:  Color32::from_rgb(224,  48, 255),  // #e030ff Electric Magenta
        node_sel:  Color32::from_rgb(240, 144,  48),  // #f09030 Amber
        node_selr: Color32::from_rgb(223, 159, 251),  // #df9ffb soft lavender — rim brightened
        label:     Color32::from_rgb(120, 152, 184),  // #7898b8 Cadet Grey
        label_hov: Color32::from_rgb(224, 240, 255),  // #e0f0ff Alice Blue
        bar_bg:    Color32::from_rgb( 10,  28,  52),  // #0a1c34 Oxford Blue
        bar_line:  Color32::from_rgb( 38,  80, 128),  // #265080 Payne's Grey blue
        bar_text:  Color32::from_rgb(168, 200, 216),  // #a8c8d8 Powder Blue
    },
    // ── 4 · Azure ─────────────────────────────────────────────────────────────
    // The deeper blue-cooler family — calmer than Salvia, still fully Navi.
    // Cobalt Blue nodes (#3d7fff) / Azure Blue rim (#1a9aff).
    // Edges from Midnight Indigo (#082060).
    Theme {
        name: "Azure",
        bg:        Color32::from_rgb( 16,  30,  46),  // #101e2e
        grid:      Color32::from_rgb( 25,  46,  72),  // #192e48
        edge:      Color32::from_rgb( 24,  56, 160),  // #1838a0 Ultramarine
        edge_hi:   Color32::from_rgb( 26, 154, 255),  // #1a9aff Azure Blue
        node:      Color32::from_rgb( 61, 127, 255),  // #3d7fff Cobalt Blue
        node_rim:  Color32::from_rgb( 26, 154, 255),  // #1a9aff Azure Blue
        node_hov:  Color32::from_rgb( 64, 232, 255),  // #40e8ff Electric Cyan
        node_sel:  Color32::from_rgb(240, 144,  48),  // #f09030 Amber
        node_selr: Color32::from_rgb(140, 204, 255),  // #8cccff soft sky blue — rim brightened
        label:     Color32::from_rgb(120, 152, 184),  // #7898b8 Cadet Grey
        label_hov: Color32::from_rgb(224, 240, 255),  // #e0f0ff Alice Blue
        bar_bg:    Color32::from_rgb( 10,  28,  52),  // #0a1c34 Oxford Blue
        bar_line:  Color32::from_rgb( 38,  80, 128),  // #265080 Payne's Grey blue
        bar_text:  Color32::from_rgb(168, 200, 216),  // #a8c8d8 Powder Blue
    },
    // ── 5 · Ruby ──────────────────────────────────────────────────────────────
    // Fully independent background — wine-dark near-black (#1a0810) so the
    // crimson nodes feel emitted rather than placed.  Cool-shifted ruby crimson
    // (192, 24, 64) follows the palette rule: reds bent toward blue.
    // Rim: Flamingo (#f85888).  Connections: Claret (#6a1018).
    // Selection flips the complement — Electric Cyan (#40e8ff) glows against
    // warm red the same way amber glows against cool blue in the other variants.
    Theme {
        name: "Ruby",
        bg:        Color32::from_rgb( 26,   8,  16),  // #1a0810 wine-dark near-black
        grid:      Color32::from_rgb( 56,  16,  28),  // #38101c dark maroon
        edge:      Color32::from_rgb(106,  16,  24),  // #6a1018 Claret
        edge_hi:   Color32::from_rgb(255,  96,  96),  // #ff6060 Bittersweet
        node:      Color32::from_rgb(192,  24,  64),  // deep ruby crimson
        node_rim:  Color32::from_rgb(248,  88, 136),  // #f85888 Flamingo
        node_hov:  Color32::from_rgb(255,  96,  96),  // #ff6060 Bittersweet
        node_sel:  Color32::from_rgb( 64, 232, 255),  // #40e8ff Electric Cyan — cool complement
        node_selr: Color32::from_rgb(251, 168, 192),  // #fba8c0 soft rose — rim brightened
        label:     Color32::from_rgb(184, 136, 152),  // warm rose-grey
        label_hov: Color32::from_rgb(240, 220, 228),  // warm near-white
        bar_bg:    Color32::from_rgb( 16,   4,   8),  // #100408 near-black
        bar_line:  Color32::from_rgb(106,  16,  24),  // #6a1018 Claret
        bar_text:  Color32::from_rgb(200, 120, 120),  // #c87878 Dusty Rose
    },
    // ── Legacy themes ─────────────────────────────────────────────────────────
    Theme {
        name: "Obsidian",
        bg:        Color32::from_rgb( 13,  13,  20),
        grid:      Color32::from_rgb( 48,  48,  75),
        edge:      Color32::from_rgb( 60,  60, 100),
        edge_hi:   Color32::from_rgb(160, 110, 255),
        node:      Color32::from_rgb(124,  77, 255),
        node_rim:  Color32::from_rgb(175, 140, 255),
        node_hov:  Color32::from_rgb(185, 155, 255),
        node_sel:  Color32::from_rgb(255, 200,  40),
        node_selr: Color32::from_rgb(212, 192, 255),  // #d4c0ff soft pale purple — rim brightened
        label:     Color32::from_rgb(185, 180, 215),
        label_hov: Color32::from_rgb(255, 255, 255),
        bar_bg:    Color32::from_rgb( 18,  18,  30),
        bar_line:  Color32::from_rgb( 48,  48,  75),
        bar_text:  Color32::from_rgb(140, 135, 175),
    },
    Theme {
        name: "Forest",
        bg:        Color32::from_rgb(  8,  18,  10),
        grid:      Color32::from_rgb( 32,  60,  36),
        edge:      Color32::from_rgb( 40,  80,  50),
        edge_hi:   Color32::from_rgb( 80, 220, 100),
        node:      Color32::from_rgb( 50, 180,  80),
        node_rim:  Color32::from_rgb(100, 220, 130),
        node_hov:  Color32::from_rgb( 80, 200, 110),
        node_sel:  Color32::from_rgb(255, 200,  40),
        node_selr: Color32::from_rgb(176, 238, 192),  // #b0eec0 soft mint — rim brightened
        label:     Color32::from_rgb(170, 210, 175),
        label_hov: Color32::from_rgb(220, 255, 220),
        bar_bg:    Color32::from_rgb( 12,  24,  14),
        bar_line:  Color32::from_rgb( 30,  60,  35),
        bar_text:  Color32::from_rgb(120, 175, 130),
    },
    Theme {
        name: "Ocean",
        bg:        Color32::from_rgb(  8,  14,  28),
        grid:      Color32::from_rgb( 28,  50,  88),
        edge:      Color32::from_rgb( 30,  60, 110),
        edge_hi:   Color32::from_rgb( 60, 150, 255),
        node:      Color32::from_rgb( 40, 130, 220),
        node_rim:  Color32::from_rgb( 80, 170, 250),
        node_hov:  Color32::from_rgb( 60, 155, 240),
        node_sel:  Color32::from_rgb(255, 200,  40),
        node_selr: Color32::from_rgb(168, 212, 252),  // #a8d4fc soft periwinkle — rim brightened
        label:     Color32::from_rgb(160, 190, 225),
        label_hov: Color32::from_rgb(200, 230, 255),
        bar_bg:    Color32::from_rgb( 12,  20,  38),
        bar_line:  Color32::from_rgb( 28,  50,  90),
        bar_text:  Color32::from_rgb(100, 145, 195),
    },
    Theme {
        name: "Ember",
        bg:        Color32::from_rgb( 20,  10,   8),
        grid:      Color32::from_rgb( 68,  34,  26),
        edge:      Color32::from_rgb(100,  40,  20),
        edge_hi:   Color32::from_rgb(255, 100,  50),
        node:      Color32::from_rgb(220,  80,  40),
        node_rim:  Color32::from_rgb(250, 130,  80),
        node_hov:  Color32::from_rgb(240, 105,  60),
        node_sel:  Color32::from_rgb(255, 240,  40),
        node_selr: Color32::from_rgb(252, 192, 168),  // #fcc0a8 soft peach — rim brightened
        label:     Color32::from_rgb(215, 185, 170),
        label_hov: Color32::from_rgb(255, 240, 230),
        bar_bg:    Color32::from_rgb( 28,  14,  10),
        bar_line:  Color32::from_rgb( 60,  30,  20),
        bar_text:  Color32::from_rgb(175, 135, 120),
    },
    Theme {
        name: "Mono",
        bg:        Color32::from_rgb( 12,  12,  12),
        grid:      Color32::from_rgb( 50,  50,  50),
        edge:      Color32::from_rgb( 80,  80,  80),
        edge_hi:   Color32::from_rgb(200, 200, 200),
        node:      Color32::from_rgb(170, 170, 170),
        node_rim:  Color32::from_rgb(210, 210, 210),
        node_hov:  Color32::from_rgb(200, 200, 200),
        node_sel:  Color32::from_rgb(255, 200,  40),
        node_selr: Color32::from_rgb(232, 232, 232),  // #e8e8e8 soft silver-white — rim brightened
        label:     Color32::from_rgb(175, 175, 175),
        label_hov: Color32::from_rgb(240, 240, 240),
        bar_bg:    Color32::from_rgb( 18,  18,  18),
        bar_line:  Color32::from_rgb( 45,  45,  45),
        bar_text:  Color32::from_rgb(130, 130, 130),
    },
];
