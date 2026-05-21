use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2, epaint::FontId};
use navi_core::{Graph, Node};
use std::collections::HashSet;
use crate::theme::Theme;

// ── Age ───────────────────────────────────────────────────────────────────────

pub fn age_days(mtime: i64, now_secs: f64) -> f64 {
    ((now_secs - mtime as f64) / 86400.0).max(0.0)
}

pub fn age_stage(mtime: i64, now_secs: f64) -> usize {
    const BREAKS: &[f64] = &[7.0, 30.0, 90.0, 270.0, 540.0];
    let d = age_days(mtime, now_secs);
    for (i, &t) in BREAKS.iter().enumerate() { if d <= t { return i; } }
    BREAKS.len()
}


// Continuous age parameter: 0.0 = just created, 1.0 = fully settled.
// Exponential approach with ~180-day half-life so notes feel fresh for weeks,
// visibly settled over months, deeply integrated past a year.
fn compute_age_t(mtime: i64, now_secs: f64) -> f32 {
    let days = age_days(mtime, now_secs) as f32;
    1.0 - (-days / 180.0).exp()
}

// ── Color helpers ─────────────────────────────────────────────────────────────

fn lerp_col(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t).round() as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t).round() as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t).round() as u8,
    )
}

// C: Temperature drift.
// Fresh end: 18% blend toward the theme's warm accent (node_sel — amber or cyan
//   complement), so the colour reads as "active/present" in the palette's own language.
// Settled end: 40% blend toward the background, so old nodes visibly sink into
//   the canvas — clearly darker, clearly cooler, clearly integrated.
fn age_color(base: Color32, bg: Color32, warm_accent: Color32, t: f32) -> Color32 {
    if t < 0.35 {
        // Fresh zone: warm-accent tint fades back to true colour
        let u = t / 0.35;
        lerp_col(lerp_col(base, warm_accent, 0.18), base, u)
    } else {
        // Settled zone: true colour drifts toward background (max 40%)
        lerp_col(base, bg, 0.40 * ((t - 0.35) / 0.65))
    }
}

fn brighten(c: Color32, amount: f32) -> Color32 {
    Color32::from_rgb(
        (c.r() as f32 + (255.0 - c.r() as f32) * amount) as u8,
        (c.g() as f32 + (255.0 - c.g() as f32) * amount) as u8,
        (c.b() as f32 + (255.0 - c.b() as f32) * amount) as u8,
    )
}

fn dim(c: Color32, amt: u8) -> Color32 {
    Color32::from_rgb(
        c.r().saturating_sub(amt),
        c.g().saturating_sub(amt),
        c.b().saturating_sub(amt),
    )
}

// ── Painter ───────────────────────────────────────────────────────────────────

pub struct GraphPainter<'a> {
    pub graph:      &'a Graph,
    pub theme:      Theme,
    pub pan:        Vec2,
    pub zoom:       f32,
    pub hovered:    Option<&'a str>,
    pub selected:   Option<&'a str>,
    pub hidden:     &'a HashSet<String>,
    pub faded:      &'a HashSet<String>,
    pub hov_adj:    HashSet<String>,
    pub show_tags:  bool,
    pub show_age:   bool,
    pub now_secs:   f64,
    pub graph_rect: Rect,
}

impl<'a> GraphPainter<'a> {
    pub fn w2s(&self, wx: f64, wy: f64) -> Pos2 {
        let cx = self.graph_rect.left() + self.graph_rect.width()  / 2.0;
        let cy = self.graph_rect.top()  + self.graph_rect.height() / 2.0;
        Pos2::new(cx + (wx as f32) * self.zoom + self.pan.x,
                  cy + (wy as f32) * self.zoom + self.pan.y)
    }

    pub fn s2w(&self, sx: f32, sy: f32) -> (f64, f64) {
        let cx = self.graph_rect.left() + self.graph_rect.width()  / 2.0;
        let cy = self.graph_rect.top()  + self.graph_rect.height() / 2.0;
        (((sx - cx - self.pan.x) / self.zoom) as f64,
         ((sy - cy - self.pan.y) / self.zoom) as f64)
    }

    fn node_base_colors(&self, nd: &Node) -> (Color32, Color32) {
        let t = &self.theme;
        if self.show_tags && !nd.tags.is_empty() {
            if let Some(&tc) = self.graph.tag_colors.get(&nd.tags[0]) {
                let body = Color32::from_rgb(tc[0], tc[1], tc[2]);
                let rim  = Color32::from_rgb(
                    (tc[0] as u16 + 38).min(255) as u8,
                    (tc[1] as u16 + 38).min(255) as u8,
                    (tc[2] as u16 + 38).min(255) as u8,
                );
                return (body, rim);
            }
        }
        (t.node, t.node_rim)
    }

    fn node_colors(&self, nd: &Node, is_sel: bool, is_hov: bool, is_fade: bool) -> (Color32, Color32) {
        let t = &self.theme;
        if is_hov && !is_sel { return (t.node_rim, t.node_hov); }
        let (body, rim) = self.node_base_colors(nd);
        if is_fade { return (dim(rim, 70), dim(body, 70)); }
        (rim, body)
    }

    pub fn paint_grid(&self, painter: &Painter) {
        let t    = &self.theme;
        let rect = self.graph_rect;
        let col  = Color32::from_rgba_unmultiplied(t.grid.r(), t.grid.g(), t.grid.b(), 200);
        let sp   = (12.0 * self.zoom).max(3.0);
        let cx   = rect.left() + rect.width()  / 2.0 + self.pan.x;
        let cy   = rect.top()  + rect.height() / 2.0 + self.pan.y;
        let ox   = ((cx % sp) + sp) % sp;
        let oy   = ((cy % sp) + sp) % sp;
        let s    = Stroke::new(0.5, col);
        let mut x = rect.left() + ox - sp;
        while x <= rect.right() + sp {
            painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], s);
            x += sp;
        }
        let mut y = rect.top() + oy - sp;
        while y <= rect.bottom() + sp {
            painter.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], s);
            y += sp;
        }
    }

    pub fn paint_edges(&self, painter: &Painter) {
        let t        = &self.theme;
        let edge_col = Color32::from_rgba_unmultiplied(t.edge.r(),    t.edge.g(),    t.edge.b(),    190);
        let edge_hi  = Color32::from_rgba_unmultiplied(t.edge_hi.r(), t.edge_hi.g(), t.edge_hi.b(), 255);
        for &(si, di) in &self.graph.edges {
            let a = &self.graph.node_list[si];
            let b = &self.graph.node_list[di];
            if self.hidden.contains(&a.id) || self.hidden.contains(&b.id) { continue; }
            if self.faded.contains(&a.id)  && self.faded.contains(&b.id)  { continue; }
            let p1 = self.w2s(a.x, a.y);
            let p2 = self.w2s(b.x, b.y);
            let r  = self.graph_rect;
            if p1.x < r.left()-10.0   && p2.x < r.left()-10.0   { continue; }
            if p1.x > r.right()+10.0  && p2.x > r.right()+10.0  { continue; }
            if p1.y < r.top()-10.0    && p2.y < r.top()-10.0    { continue; }
            if p1.y > r.bottom()+10.0 && p2.y > r.bottom()+10.0 { continue; }
            let hov = self.hov_adj.contains(&a.id) && self.hov_adj.contains(&b.id)
                || (Some(a.id.as_str()) == self.hovered && self.hov_adj.contains(&b.id))
                || (Some(b.id.as_str()) == self.hovered && self.hov_adj.contains(&a.id));
            painter.line_segment([p1, p2], Stroke::new(1.0, if hov { edge_hi } else { edge_col }));
        }
    }

    pub fn paint_nodes(&self, painter: &Painter) {
        for (node_idx, nd) in self.graph.node_list.iter().enumerate() {
            if self.hidden.contains(&nd.id) { continue; }
            let sc = self.w2s(nd.x, nd.y);
            // Visual breathing — purely cosmetic screen-space offset, actual world
            // position (used for hit detection, edges, physics) is unchanged.
            let phase = node_idx as f32 * 1.618033988749895_f32;
            let bt    = self.now_secs as f32;
            let dsc   = sc + Vec2::new(
                (bt * 0.32 + phase).sin()            * 4.0,
                (bt * 0.27 + phase * 1.272).cos()    * 4.0,
            );
            let pr = (nd.radius * self.zoom).max(2.0);
            let r  = self.graph_rect;
            if sc.x + pr*4.0 < r.left()-10.0   || sc.x - pr*4.0 > r.right()+10.0  { continue; }
            if sc.y + pr*4.0 < r.top()-10.0    || sc.y - pr*4.0 > r.bottom()+10.0 { continue; }

            let is_sel  = Some(nd.id.as_str()) == self.selected;
            let is_hov  = Some(nd.id.as_str()) == self.hovered;
            let is_fade = self.faded.contains(&nd.id)
                || (self.hovered.is_some() && !self.hov_adj.contains(&nd.id) && !is_sel && !is_hov);

            let (rim_col, body_col) = self.node_colors(nd, is_sel, is_hov, is_fade);

            // Age parameter — bypassed for selected/hovered so they always read true colour
            let age_t = if !is_sel && !is_hov && self.show_age && nd.mtime > 0 {
                compute_age_t(nd.mtime, self.now_secs)
            } else {
                0.0_f32
            };

            // C: temperature drift — warm accent tint when fresh, sinks toward bg when settled
            let wa   = self.theme.node_sel; // warm accent (amber, cyan, etc. per theme)
            let body = age_color(body_col, self.theme.bg, wa, age_t);
            let rim  = age_color(rim_col,  self.theme.bg, wa, age_t);

            // Selection ring — tight accent just outside the node
            if is_sel {
                let sc = self.theme.node_selr;
                painter.circle_stroke(dsc, pr * 1.28, Stroke::new(1.8,
                    Color32::from_rgba_unmultiplied(sc.r(), sc.g(), sc.b(), 195)));
            }

            // Glow fades with age so settled nodes don't radiate
            let glow_base = if is_fade { 3u8 } else { 9 };
            let glow_a    = (glow_base as f32 * (1.0 - age_t * 0.75)) as u8;
            painter.circle_filled(dsc, pr * 1.18,
                Color32::from_rgba_unmultiplied(rim.r(), rim.g(), rim.b(), glow_a));

            painter.circle_filled(dsc, pr, body);

            // D: rim softness — crisp when fresh, dissolves as node settles
            let rim_a = (255.0 * (1.0 - age_t).powf(0.55)) as u8;
            let rim_w = (pr * 0.25).max(1.0).min(3.0) * (1.0 - age_t * 0.40).max(0.0);
            if rim_a > 4 && rim_w > 0.3 {
                painter.circle_stroke(dsc, pr, Stroke::new(rim_w,
                    Color32::from_rgba_unmultiplied(rim.r(), rim.g(), rim.b(), rim_a)));
            }

            // F: highlight depth — present surface when fresh, flattens with age
            if pr >= 5.0 {
                let hl_a = (28.0 * (1.0 - age_t).powf(0.45)) as u8;
                if hl_a > 2 {
                    painter.circle_filled(
                        dsc + Vec2::new(-pr * 0.27, -pr * 0.27),
                        (pr * 0.28).max(1.0),
                        Color32::from_rgba_unmultiplied(255, 255, 255, hl_a),
                    );
                }
            }

            // Inner ring for headline nodes
            if nd.level > 0 && pr >= 5.0 {
                painter.circle_stroke(dsc, pr * 0.33, Stroke::new(1.0, rim));
            }
        }
    }

    pub fn paint_labels(&self, painter: &Painter) {
        let t = &self.theme;
        if self.zoom < 0.25 { return; }
        let alpha_base = (((self.zoom - 0.25) / 0.35) * 255.0).min(255.0) as u8;
        let font = FontId::proportional(13.0);
        // Adaptive truncation: long titles overlap at low zoom
        let max_chars: usize = if self.zoom < 0.55 { 8 }
                               else if self.zoom < 1.0 { 15 }
                               else { usize::MAX };
        for (node_idx, nd) in self.graph.node_list.iter().enumerate() {
            if self.hidden.contains(&nd.id) { continue; }
            let sc = self.w2s(nd.x, nd.y);
            // Match breathing offset so label stays under its node
            let phase = node_idx as f32 * 1.618033988749895_f32;
            let bt    = self.now_secs as f32;
            let dsc   = sc + Vec2::new(
                (bt * 0.32 + phase).sin()         * 4.0,
                (bt * 0.27 + phase * 1.272).cos() * 4.0,
            );
            let r = self.graph_rect;
            if dsc.x < r.left()-200.0 || dsc.x > r.right()+200.0 { continue; }
            if dsc.y < r.top()-20.0   || dsc.y > r.bottom()+20.0  { continue; }
            let is_hi   = Some(nd.id.as_str()) == self.hovered || Some(nd.id.as_str()) == self.selected;
            let is_fade = self.faded.contains(&nd.id)
                || (self.hovered.is_some() && !self.hov_adj.contains(&nd.id) && !is_hi);
            let alpha = if is_fade { (alpha_base / 5).max(0) } else { alpha_base };
            if alpha < 5 { continue; }
            let base_col = if is_hi { t.label_hov } else { t.label };
            let col  = Color32::from_rgba_unmultiplied(base_col.r(), base_col.g(), base_col.b(), alpha);
            let pr   = (nd.radius * self.zoom).max(2.0);
            let title = if nd.title.chars().count() > max_chars {
                nd.title.chars().take(max_chars).collect::<String>() + "…"
            } else {
                nd.title.clone()
            };
            painter.text(Pos2::new(dsc.x, dsc.y + pr + 8.0),
                egui::Align2::CENTER_TOP, &title, font.clone(), col);
        }
    }

    pub fn node_at(&self, sx: f32, sy: f32) -> Option<&str> {
        let mut best_d = f32::INFINITY;
        let mut best: Option<&str> = None;
        for nd in &self.graph.node_list {
            if self.hidden.contains(&nd.id) { continue; }
            let sc = self.w2s(nd.x, nd.y);
            let d  = (sc - Pos2::new(sx, sy)).length();
            let hr = (nd.radius * self.zoom).max(9.0);
            if d <= hr && d < best_d { best_d = d; best = Some(&nd.id); }
        }
        best
    }
}
