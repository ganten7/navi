use std::collections::{HashMap, HashSet};
use rayon::prelude::*;
use crate::db::RawNode;

// ── physics constants ─────────────────────────────────────────────────────────
pub const REPULSION: f64 = 16000.0;
pub const SPRING_K:  f64 = 0.055;
pub const SPRING_L:  f64 = 110.0;
pub const GRAVITY:   f64 = 0.020;
pub const DAMPING:   f64 = 0.90;
pub const BASE_DT:   f64 = 1.0 / 60.0;
pub const DT_CAP:    f64 = 0.05;
pub const R_MIN:     f32 = 5.0;
pub const R_MAX:     f32 = 22.0;

const LAYOUT_DUR: f32 = 1.3; // seconds for layout transition animation

#[derive(Debug, Clone)]
pub struct Node {
    pub id:      String,
    pub title:   String,
    pub file:    String,
    pub level:   i64,
    pub pos:     i64,
    pub mtime:   i64,
    pub aliases: Vec<String>,
    pub tags:    Vec<String>,
    pub x: f64, pub y: f64,
    pub vx: f64, pub vy: f64,
    pub pinned: bool,
    pub degree: usize,
    pub radius: f32,
}

struct LayoutAnim {
    from:           Vec<(f64, f64)>,
    to:             Vec<(f64, f64)>,
    t:              f32,
    resume_physics: bool, // true = restart force-directed after landing; false = breathing mode
}

pub struct Graph {
    pub nodes:      HashMap<String, usize>,
    pub node_list:  Vec<Node>,
    pub edges:      Vec<(usize, usize)>,
    pub adj:        HashMap<String, HashSet<String>>,
    pub tag_colors: HashMap<String, [u8; 3]>,
    pub physics_on: bool,
    pub step_count: u64,
    layout_anim:    Option<LayoutAnim>,
    physics_ease:   f32, // 0 → 1 ramp after layout animation so forces fade in, not snap
    settle_frames:  u32, // consecutive frames below velocity threshold; auto-pauses at limit
}

impl Graph {
    pub fn new(raw_nodes: Vec<RawNode>, raw_edges: Vec<(String, String)>) -> Self {
        let n = raw_nodes.len();
        let mut nodes:     HashMap<String, usize> = HashMap::with_capacity(n);
        let mut node_list: Vec<Node>              = Vec::with_capacity(n);

        for (i, rn) in raw_nodes.iter().enumerate() {
            let node = Node {
                id:      rn.id.clone(),
                title:   if rn.title.is_empty() { rn.id[..rn.id.len().min(12)].to_string() } else { rn.title.clone() },
                file:    rn.file.clone(),
                level:   rn.level,
                pos:     rn.pos,
                mtime:   rn.mtime,
                aliases: rn.aliases.clone(),
                tags:    rn.tags.clone(),
                x: 0.0, y: 0.0,
                vx: lcg_uniform(i as u64 * 2 + 3) * 4.0 - 2.0,
                vy: lcg_uniform(i as u64 * 2 + 4) * 4.0 - 2.0,
                pinned: false, degree: 0, radius: R_MIN,
            };
            nodes.insert(rn.id.clone(), i);
            node_list.push(node);
        }

        let mut edges: Vec<(usize, usize)>                = Vec::new();
        let mut adj:   HashMap<String, HashSet<String>>   = HashMap::new();
        for nid in node_list.iter().map(|n| n.id.clone()) { adj.insert(nid, HashSet::new()); }
        for (src, dst) in &raw_edges {
            if let (Some(&si), Some(&di)) = (nodes.get(src), nodes.get(dst)) {
                edges.push((si, di));
                node_list[si].degree += 1;
                node_list[di].degree += 1;
                adj.entry(src.clone()).or_default().insert(dst.clone());
                adj.entry(dst.clone()).or_default().insert(src.clone());
            }
        }

        let max_deg  = node_list.iter().map(|n| n.degree).max().unwrap_or(1).max(1);
        let log_span = (max_deg as f32 + 1.0).ln_1p();
        for nd in node_list.iter_mut() {
            nd.radius = R_MIN + (R_MAX - R_MIN) * (nd.degree as f32 + 1.0).ln_1p() / log_span;
        }

        // Initial placement: disk (phyllotaxis) so physics settles into a
        // natural filled-circle shape like Obsidian's graph view.
        let positions = positions_disk(&node_list, &edges, n);
        for (i, (x, y)) in positions.iter().enumerate() {
            node_list[i].x = *x;
            node_list[i].y = *y;
        }

        let mut all_tags: Vec<String> = node_list.iter().flat_map(|n| n.tags.iter().cloned()).collect();
        all_tags.sort(); all_tags.dedup();
        let phi = 0.618033988749895f64;
        let mut tag_colors: HashMap<String, [u8; 3]> = HashMap::new();
        for (i, tag) in all_tags.iter().enumerate() {
            let h = (i as f64 * phi) % 1.0;
            tag_colors.insert(tag.clone(), hsv_to_rgb(h, 0.62, 0.88));
        }

        Graph { nodes, node_list, edges, adj, tag_colors,
                physics_on: true, step_count: 0, layout_anim: None, physics_ease: 1.0,
                settle_frames: 0 }
    }

    // ── Layout transitions ────────────────────────────────────────────────────

    pub fn positions_radial(&self) -> Vec<(f64, f64)> {
        positions_radial(&self.node_list, &self.edges, self.node_list.len())
    }

    pub fn positions_ring(&self) -> Vec<(f64, f64)> {
        positions_ring(&self.node_list, &self.edges, self.node_list.len())
    }

    pub fn positions_disk(&self) -> Vec<(f64, f64)> {
        positions_disk(&self.node_list, &self.edges, self.node_list.len())
    }

    /// Column — BFS shells as vertical strips (L→R), nodes spread vertically.
    pub fn positions_column(&self) -> Vec<(f64, f64)> {
        positions_layered(&self.node_list, &self.edges, self.node_list.len(), true)
    }

    /// Row — BFS shells as horizontal strips (T→B), nodes spread horizontally.
    pub fn positions_row(&self) -> Vec<(f64, f64)> {
        positions_layered(&self.node_list, &self.edges, self.node_list.len(), false)
    }

    /// Top-down tree — hub at top, each BFS shell becomes a horizontal row.
    /// Nodes within each row sorted by parent position to reduce crossings.
    pub fn positions_tree(&self) -> Vec<(f64, f64)> {
        positions_tree(&self.node_list, &self.edges, self.node_list.len())
    }

    /// Begin a smooth transition to `targets`. Nodes float to their new
    /// positions over LAYOUT_DUR seconds; physics resumes after.
    pub fn begin_layout_transition(&mut self, targets: Vec<(f64, f64)>, resume_physics: bool) {
        let from = self.node_list.iter().map(|n| (n.x, n.y)).collect();
        self.layout_anim = Some(LayoutAnim { from, to: targets, t: 0.0, resume_physics });
        self.physics_on  = true;
    }

    pub fn is_layout_animating(&self) -> bool { self.layout_anim.is_some() }

    // ── Step ─────────────────────────────────────────────────────────────────

    /// Resume full force-directed physics from the current node positions,
    /// easing forces in so nodes don't snap from their current layout.
    pub fn resume_physics(&mut self) {
        self.physics_on   = true;
        self.physics_ease = 0.0;
    }

    pub fn step(&mut self, dt: f64) {
        let dt = dt.min(DT_CAP);
        let n  = self.node_list.len();
        if n == 0 { return; }

        // Layout animation: smoothstep lerp, no physics
        if let Some(ref mut anim) = self.layout_anim {
            self.physics_on = true; // keep step() ticking
            anim.t = (anim.t + dt as f32 / LAYOUT_DUR).min(1.0);
            let t    = anim.t;
            let ease = (t * t * (3.0 - 2.0 * t)) as f64;
            for i in 0..n {
                let nd = &mut self.node_list[i];
                nd.x  = anim.from[i].0 + (anim.to[i].0 - anim.from[i].0) * ease;
                nd.y  = anim.from[i].1 + (anim.to[i].1 - anim.from[i].1) * ease;
                nd.vx = 0.0; nd.vy = 0.0;
            }
            if anim.t >= 1.0 {
                let resume = anim.resume_physics;
                self.layout_anim = None;
                if resume {
                    self.resume_physics(); // ease forces back in so nodes settle naturally
                } else {
                    self.physics_on = false; // breathing mode — holds layout positions
                }
            }
            return;
        }

        if !self.physics_on {
            // Breathing is handled purely visually in the painter — no position mutation here.
            self.step_count = self.step_count.wrapping_add(1);
            return;
        }

        // Full force-directed physics — ease forces in after a layout to avoid snapping
        if self.physics_ease < 1.0 {
            self.physics_ease = (self.physics_ease + dt as f32 / 3.5).min(1.0);
        }
        let ease = self.physics_ease as f64;

        let mut fx = vec![0.0f64; n];
        let mut fy = vec![0.0f64; n];

        {
            let pts: Vec<(f64, f64)> = self.node_list.iter().map(|nd| (nd.x, nd.y)).collect();
            let bh = bh_build(&pts);
            // Each node queries the read-only tree independently — embarrassingly parallel.
            let rep_forces: Vec<(f64, f64)> = pts.par_iter().enumerate()
                .map_with(Vec::with_capacity(64), |stk, (i, &(x, y))| bh_force(&bh, x, y, i, stk))
                .collect();
            for (i, (rfx, rfy)) in rep_forces.into_iter().enumerate() {
                fx[i] += rfx; fy[i] += rfy;
            }
        }

        for &(si, di) in &self.edges {
            let dx   = self.node_list[di].x - self.node_list[si].x;
            let dy   = self.node_list[di].y - self.node_list[si].y;
            let dist = dx.hypot(dy).max(1e-4);
            let f    = SPRING_K * (dist - SPRING_L) / dist;
            fx[si] += dx * f; fy[si] += dy * f;
            fx[di] -= dx * f; fy[di] -= dy * f;
        }

        self.step_count = self.step_count.wrapping_add(1);
        let t    = self.step_count as f64 * 0.006;
        const PHI: f64 = 1.618033988749895;
        for i in 0..n {
            let phase = i as f64 * PHI;
            fx[i] += (t + phase).sin() * 0.35;
            fy[i] += (t * 1.272 + phase * 0.849).cos() * 0.35;
        }

        // Scale ALL forces (including gravity) by ease so nothing snaps after layout animation
        let g = GRAVITY * ease;
        for i in 0..n { fx[i] *= ease; fy[i] *= ease; }

        let damp  = DAMPING.powf(dt / BASE_DT);
        let max_r = (n as f64 * 15.0).max(350.0);
        const MAX_V: f64 = 650.0;
        for i in 0..n {
            let nd = &mut self.node_list[i];
            if nd.pinned { nd.vx = 0.0; nd.vy = 0.0; continue; }
            nd.vx = (nd.vx + (fx[i] - nd.x * g) * dt) * damp;
            nd.vy = (nd.vy + (fy[i] - nd.y * g) * dt) * damp;
            // Cap velocity before position update so no single step can escape
            let spd = nd.vx.hypot(nd.vy);
            if spd > MAX_V { nd.vx = nd.vx / spd * MAX_V; nd.vy = nd.vy / spd * MAX_V; }
            nd.x += nd.vx * dt;
            nd.y += nd.vy * dt;
            // Hard radial clamp — any node that slips past loses its velocity
            let d = nd.x.hypot(nd.y);
            if d > max_r {
                let s = max_r / d;
                nd.x *= s; nd.y *= s;
                nd.vx *= 0.1; nd.vy *= 0.1;
            }
        }

        // Auto-pause: if all nodes are near-still for ~1.5 s, switch to breathing mode
        let max_v = self.node_list.iter().map(|nd| nd.vx.hypot(nd.vy)).fold(0.0f64, f64::max);
        if max_v < 1.5 && self.physics_ease >= 1.0 {
            self.settle_frames += 1;
            if self.settle_frames > 90 { self.physics_on = false; self.settle_frames = 0; }
        } else {
            self.settle_frames = 0;
        }
    }

    pub fn bfs(&self, start_id: &str, hops: usize) -> HashSet<String> {
        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(start_id.to_string());
        let mut frontier = visited.clone();
        for _ in 0..hops {
            let mut nxt = HashSet::new();
            for nid in &frontier {
                for nb in self.adj.get(nid).into_iter().flatten() {
                    if visited.insert(nb.clone()) { nxt.insert(nb.clone()); }
                }
            }
            frontier = nxt;
        }
        visited
    }

    pub fn transplant_positions(&mut self, old: &Graph) {
        for nd in &mut self.node_list {
            if let Some(&oi) = old.nodes.get(&nd.id) {
                let on = &old.node_list[oi];
                nd.x = on.x; nd.y = on.y;
                nd.vx = on.vx; nd.vy = on.vy;
            }
        }
    }
}

// ── Layout position calculators ───────────────────────────────────────────────

/// Radial shell layout: hub at origin, BFS shells as concentric rings.
/// Nodes within each ring sorted by parent angle to minimise crossings.
fn positions_radial(node_list: &[Node], edges: &[(usize, usize)], n: usize) -> Vec<(f64, f64)> {
    if n == 0 { return Vec::new(); }
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(s, d) in edges { adj[s].push(d); adj[d].push(s); }

    let hub       = (0..n).max_by_key(|&i| node_list[i].degree).unwrap_or(0);
    let unvisited = n + 1;
    let mut shell = vec![unvisited; n];
    shell[hub] = 0;
    let mut shells: Vec<Vec<usize>> = vec![vec![hub]];
    let mut frontier = vec![hub];

    while !frontier.is_empty() {
        let sh = shells.len();
        let mut next = Vec::new();
        let mut sh_nodes = Vec::new();
        for &cur in &frontier {
            for &nb in &adj[cur] {
                if shell[nb] == unvisited {
                    shell[nb] = sh; sh_nodes.push(nb); next.push(nb);
                }
            }
        }
        if !sh_nodes.is_empty() { shells.push(sh_nodes); }
        frontier = next;
    }

    // Disconnected nodes — collected but NOT added to shells.
    // They get their own compact cluster near the hub so gravity keeps them stable.
    let disc: Vec<usize> = (0..n).filter(|&i| shell[i] == unvisited).collect();

    let n_shells = shells.len().saturating_sub(1).max(1);
    let ring_gap = SPRING_L.max(80.0 + n as f64 * 4.0 / n_shells as f64);

    let mut positions = vec![(0.0_f64, 0.0_f64); n];
    // hub at origin
    positions[hub] = (0.0, 0.0);

    for (sh_idx, sh_nodes) in shells.iter().enumerate() {
        if sh_idx == 0 { continue; }
        let r     = sh_idx as f64 * ring_gap;
        let count = sh_nodes.len();

        let mut sorted: Vec<(usize, f64)> = sh_nodes.iter().map(|&ni| {
            let pa = adj[ni].iter()
                .filter(|&&nb| shell[nb] < sh_idx)
                .map(|&nb| positions[nb].1.atan2(positions[nb].0))
                .next().unwrap_or(0.0);
            (ni, pa)
        }).collect();
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        for (pos, &(ni, _)) in sorted.iter().enumerate() {
            let angle = std::f64::consts::TAU * pos as f64 / count as f64;
            let jx = lcg_uniform((ni as u64).wrapping_mul(6364136223846793005).wrapping_add(1)) * 12.0 - 6.0;
            let jy = lcg_uniform((ni as u64).wrapping_mul(6364136223846793005).wrapping_add(2)) * 12.0 - 6.0;
            positions[ni] = (r * angle.cos() + jx, r * angle.sin() + jy);
        }
    }

    // Orphans: compact cluster at gravity equilibrium (r ≈ perturbation/GRAVITY ≈ 25wu).
    // Placed in the bottom-left quadrant so they're visually distinct from
    // connected rings and never on the same radial lines.
    if !disc.is_empty() {
        let orphan_r   = 28.0_f64;
        let arc_start  = std::f64::consts::PI * 1.15;
        let arc_spread = std::f64::consts::PI * 0.45;
        let dcount     = disc.len();
        for (i, &ni) in disc.iter().enumerate() {
            let t     = if dcount == 1 { 0.5 } else { i as f64 / (dcount - 1) as f64 };
            let angle = arc_start + arc_spread * t;
            let jx = lcg_uniform((ni as u64).wrapping_mul(6364136223846793005).wrapping_add(7)) * 8.0 - 4.0;
            let jy = lcg_uniform((ni as u64).wrapping_mul(6364136223846793005).wrapping_add(8)) * 8.0 - 4.0;
            positions[ni] = (orphan_r * angle.cos() + jx, orphan_r * angle.sin() + jy);
        }
    }

    positions
}

/// Ring layout: all nodes on a single circle, BFS-ordered from hub.
fn positions_ring(node_list: &[Node], edges: &[(usize, usize)], n: usize) -> Vec<(f64, f64)> {
    if n == 0 { return Vec::new(); }
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(s, d) in edges { adj[s].push(d); adj[d].push(s); }

    let hub = (0..n).max_by_key(|&i| node_list[i].degree).unwrap_or(0);
    let mut order   = Vec::with_capacity(n);
    let mut visited = vec![false; n];
    let mut queue   = std::collections::VecDeque::new();
    queue.push_back(hub); visited[hub] = true;
    while let Some(cur) = queue.pop_front() {
        order.push(cur);
        let mut nbs: Vec<usize> = adj[cur].iter().copied().filter(|&nb| !visited[nb]).collect();
        nbs.sort_by_key(|&nb| std::cmp::Reverse(node_list[nb].degree));
        for nb in nbs { visited[nb] = true; queue.push_back(nb); }
    }
    for i in 0..n { if !visited[i] { order.push(i); } }

    let r = (60.0_f64).max((380.0_f64).min(55.0 + n as f64 * 2.8));
    let mut positions = vec![(0.0_f64, 0.0_f64); n];
    for (pos, &ni) in order.iter().enumerate() {
        let angle = std::f64::consts::TAU * pos as f64 / n as f64;
        let jx = lcg_uniform((ni as u64).wrapping_mul(6364136223846793005).wrapping_add(1)) * 12.0 - 6.0;
        let jy = lcg_uniform((ni as u64).wrapping_mul(6364136223846793005).wrapping_add(2)) * 12.0 - 6.0;
        positions[ni] = (r * angle.cos() + jx, r * angle.sin() + jy);
    }
    positions
}

/// Disk layout: shell-based placement at SPRING_L ring gap.
/// Nodes start at their physics equilibrium distance so physics barely needs
/// to move them — edges never cross on spawn because each shell's nodes are
/// angle-sorted by their parent's position.  Physics then relaxes the layout
/// into a natural filled circle (Obsidian-style).
fn positions_disk(node_list: &[Node], edges: &[(usize, usize)], n: usize) -> Vec<(f64, f64)> {
    if n == 0 { return Vec::new(); }
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(s, d) in edges { adj[s].push(d); adj[d].push(s); }

    let hub       = (0..n).max_by_key(|&i| node_list[i].degree).unwrap_or(0);
    let unvisited = n + 1;
    let mut shell = vec![unvisited; n];
    shell[hub] = 0;
    let mut shells: Vec<Vec<usize>> = vec![vec![hub]];
    let mut frontier = vec![hub];
    while !frontier.is_empty() {
        let sh = shells.len();
        let mut next = Vec::new(); let mut sh_nodes = Vec::new();
        for &cur in &frontier {
            for &nb in &adj[cur] {
                if shell[nb] == unvisited { shell[nb] = sh; sh_nodes.push(nb); next.push(nb); }
            }
        }
        if !sh_nodes.is_empty() { shells.push(sh_nodes); }
        frontier = next;
    }
    let disc: Vec<usize> = (0..n).filter(|&i| shell[i] == unvisited).collect();

    // Ring gap = SPRING_L so connected nodes begin exactly at their rest length
    let ring_gap = SPRING_L;

    let mut positions = vec![(0.0_f64, 0.0_f64); n];
    positions[hub] = (0.0, 0.0);

    for (sh_idx, sh_nodes) in shells.iter().enumerate() {
        if sh_idx == 0 { continue; }
        let r     = sh_idx as f64 * ring_gap;
        let count = sh_nodes.len();
        // Sort by parent angle → no crossings between adjacent shells
        let mut sorted: Vec<(usize, f64)> = sh_nodes.iter().map(|&ni| {
            let pa = adj[ni].iter()
                .filter(|&&nb| shell[nb] < sh_idx)
                .map(|&nb| positions[nb].1.atan2(positions[nb].0))
                .next().unwrap_or(0.0);
            (ni, pa)
        }).collect();
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        for (pos, &(ni, _)) in sorted.iter().enumerate() {
            let angle = std::f64::consts::TAU * pos as f64 / count as f64;
            let jx = lcg_uniform((ni as u64).wrapping_mul(6364136223846793005).wrapping_add(1)) * 8.0 - 4.0;
            let jy = lcg_uniform((ni as u64).wrapping_mul(6364136223846793005).wrapping_add(2)) * 8.0 - 4.0;
            positions[ni] = (r * angle.cos() + jx, r * angle.sin() + jy);
        }
    }

    // Orphans near origin — at gravity equilibrium, won't drift
    if !disc.is_empty() {
        let orphan_r  = 28.0_f64;
        let arc_start = std::f64::consts::PI * 1.15;
        let arc_span  = std::f64::consts::PI * 0.45;
        let dc = disc.len();
        for (i, &ni) in disc.iter().enumerate() {
            let t     = if dc == 1 { 0.5 } else { i as f64 / (dc - 1) as f64 };
            let angle = arc_start + arc_span * t;
            let jx = lcg_uniform((ni as u64).wrapping_mul(6364136223846793005).wrapping_add(7)) * 8.0 - 4.0;
            let jy = lcg_uniform((ni as u64).wrapping_mul(6364136223846793005).wrapping_add(8)) * 8.0 - 4.0;
            positions[ni] = (orphan_r * angle.cos() + jx, orphan_r * angle.sin() + jy);
        }
    }
    prerelax(positions, edges, n)
}

/// Generic layered layout used by both Column and Row.
/// `column_mode=true`  → connected shells advance left→right, orphans placed
///                        in a separate cluster BELOW the connected graph.
/// `column_mode=false` → connected shells advance top→bottom, orphans placed
///                        in a separate cluster to the RIGHT of the connected graph.
/// Orphans are NEVER placed inline with connected nodes so they cannot be
/// mistaken for part of a chain.
fn positions_layered(node_list: &[Node], edges: &[(usize, usize)], n: usize, column_mode: bool) -> Vec<(f64, f64)> {
    if n == 0 { return Vec::new(); }
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(s, d) in edges { adj[s].push(d); adj[d].push(s); }

    let hub       = (0..n).max_by_key(|&i| node_list[i].degree).unwrap_or(0);
    let unvisited = n + 1;
    let mut shell = vec![unvisited; n];
    shell[hub] = 0;
    let mut shells: Vec<Vec<usize>> = vec![vec![hub]];
    let mut frontier = vec![hub];
    while !frontier.is_empty() {
        let sh = shells.len();
        let mut next = Vec::new(); let mut sh_nodes = Vec::new();
        for &cur in &frontier {
            for &nb in &adj[cur] {
                if shell[nb] == unvisited { shell[nb] = sh; sh_nodes.push(nb); next.push(nb); }
            }
        }
        if !sh_nodes.is_empty() { shells.push(sh_nodes); }
        frontier = next;
    }
    // Collect disconnected nodes — kept completely separate, never added to shells
    let orphans: Vec<usize> = (0..n).filter(|&i| shell[i] == unvisited).collect();

    // main_gap ≈ SPRING_L so physics equilibrium matches the layout.
    // cross_gap larger so labels never crowd each other.
    let main_gap  = SPRING_L * 1.1;
    let cross_gap = SPRING_L * 1.8;
    let n_shells  = shells.len();
    let total_main = (n_shells as f64 - 1.0) * main_gap;
    // Furthest extent of connected graph on the main axis
    let connected_main_max = total_main / 2.0;

    let mut positions = vec![(0.0_f64, 0.0_f64); n];

    // Place connected shells
    for (sh_idx, sh_nodes) in shells.iter().enumerate() {
        let main  = sh_idx as f64 * main_gap - total_main / 2.0;
        let count = sh_nodes.len();
        let total_cross = (count as f64 - 1.0) * cross_gap;

        // Sort by parent's cross-axis position so edges don't cross between shells
        let mut sorted: Vec<(usize, f64)> = sh_nodes.iter().map(|&ni| {
            let cp = adj[ni].iter()
                .filter(|&&nb| shell[nb] < sh_idx)
                .map(|&nb| if column_mode { positions[nb].1 } else { positions[nb].0 })
                .next().unwrap_or(0.0);
            (ni, cp)
        }).collect();
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        for (pos, &(ni, _)) in sorted.iter().enumerate() {
            let cross = pos as f64 * cross_gap - total_cross / 2.0;
            positions[ni] = if column_mode { (main, cross) } else { (cross, main) };
        }
    }

    // Orphans always go to the RIGHT of the entire connected graph, spread in y.
    // This guarantees no drawn edge (which stays within the connected x-range)
    // can pass through an orphan node regardless of layout mode.
    if !orphans.is_empty() {
        let max_conn_x = shells.iter().flatten()
            .map(|&i| positions[i].0)
            .fold(0.0_f64, f64::max);
        // Cluster orphans in a phyllotaxis spiral to the right — no straight-line
        // alignment possible so they can never look like they sit on a connection.
        let orphan_cx = max_conn_x + main_gap * 1.8;
        let cluster_r = cross_gap * (orphans.len() as f64).sqrt().max(1.0) * 0.55;
        let golden    = std::f64::consts::TAU * (1.0 - 1.0 / 1.618033988749895);
        for (i, &ni) in orphans.iter().enumerate() {
            let r     = (i as f64 / orphans.len().max(1) as f64).sqrt() * cluster_r;
            let theta = i as f64 * golden;
            positions[ni] = (orphan_cx + r * theta.cos(), r * theta.sin());
        }
    }

    positions
}

/// Proper top-down tree using subtree-width allocation (simplified Reingold–Tilford).
/// Each subtree occupies a non-overlapping horizontal band, so no edges can cross.
fn positions_tree(node_list: &[Node], edges: &[(usize, usize)], n: usize) -> Vec<(f64, f64)> {
    if n == 0 { return Vec::new(); }
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(s, d) in edges { adj[s].push(d); adj[d].push(s); }

    let root = (0..n).max_by_key(|&i| node_list[i].degree).unwrap_or(0);

    // Build spanning tree via BFS; each node gets exactly one parent
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut depth   = vec![0usize; n];
    let mut bfs_ord = Vec::with_capacity(n);
    let mut visited = vec![false; n];
    let mut queue   = std::collections::VecDeque::new();
    queue.push_back(root); visited[root] = true;
    while let Some(cur) = queue.pop_front() {
        bfs_ord.push(cur);
        let mut nbs: Vec<usize> = adj[cur].iter().copied().filter(|&nb| !visited[nb]).collect();
        nbs.sort_by_key(|&nb| std::cmp::Reverse(node_list[nb].degree));
        for nb in nbs {
            visited[nb] = true;
            children[cur].push(nb);
            depth[nb] = depth[cur] + 1;
            queue.push_back(nb);
        }
    }
    let mut disconnected: Vec<usize> = (0..n).filter(|&i| !visited[i]).collect();

    let unit    = SPRING_L * 1.8; // wide enough for labels; ~matches cross-axis rest length
    let row_gap = SPRING_L * 1.1;

    // Bottom-up: each leaf = width 1, each internal node = sum of children widths
    let mut width = vec![1.0_f64; n];
    for &ni in bfs_ord.iter().rev() {
        if !children[ni].is_empty() {
            width[ni] = children[ni].iter().map(|&c| width[c]).sum();
        }
    }

    let max_depth = depth.iter().copied().max().unwrap_or(0);
    let total_h   = max_depth as f64 * row_gap;

    let mut positions  = vec![(0.0_f64, 0.0_f64); n];
    let mut left_edge  = vec![0.0_f64; n];
    left_edge[root] = -(width[root] * unit) / 2.0;

    // Top-down: place each node at the centre of its subtree's horizontal band
    for &ni in &bfs_ord {
        let x = left_edge[ni] + width[ni] * unit / 2.0;
        let y = depth[ni] as f64 * row_gap - total_h / 2.0;
        positions[ni] = (x, y);

        // Distribute children left-to-right within this node's band
        let mut cursor = left_edge[ni];
        for &ci in &children[ni] {
            left_edge[ci] = cursor;
            cursor += width[ci] * unit;
        }
    }

    // Disconnected nodes go to the RIGHT of the tree, spread in y.
    // Placing them below would put them in the path of the tree's vertical edges.
    if !disconnected.is_empty() {
        let max_tree_x = bfs_ord.iter()
            .map(|&i| positions[i].0)
            .fold(0.0_f64, f64::max);
        let orphan_cx = max_tree_x + unit * 1.8;
        let cluster_r = unit * (disconnected.len() as f64).sqrt().max(1.0) * 0.55;
        let golden    = std::f64::consts::TAU * (1.0 - 1.0 / 1.618033988749895);
        for (i, &ni) in disconnected.iter().enumerate() {
            let r     = (i as f64 / disconnected.len().max(1) as f64).sqrt() * cluster_r;
            let theta = i as f64 * golden;
            positions[ni] = (orphan_cx + r * theta.cos(), r * theta.sin());
        }
    }
    positions
}

/// Run simplified physics on `pos` until near-equilibrium, then return the
/// settled coordinates.  Operates on a pure copy — the Graph is not mutated.
// ── Barnes-Hut O(n log n) repulsion ──────────────────────────────────────────

const BH_THETA: f64 = 0.9; // opening angle; lower = more accurate, higher = faster

struct BHCell {
    x0: f64, y0: f64, x1: f64, y1: f64, // bounds
    mass: f64,
    cx: f64, cy: f64,                    // center of mass
    ch: [u32; 4],                        // children; u32::MAX = absent
    body: i32,                           // ≥0 leaf idx, -1 internal, -2 empty
}

impl BHCell {
    fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x0, y0, x1, y1, mass: 0.0, cx: 0.0, cy: 0.0, ch: [u32::MAX; 4], body: -2 }
    }
    fn quad(&self, x: f64, y: f64) -> usize {
        let mx = (self.x0 + self.x1) * 0.5;
        let my = (self.y0 + self.y1) * 0.5;
        (x >= mx) as usize | (((y >= my) as usize) << 1)
    }
    fn child_bounds(&self, q: usize) -> (f64, f64, f64, f64) {
        let mx = (self.x0 + self.x1) * 0.5;
        let my = (self.y0 + self.y1) * 0.5;
        match q {
            0 => (self.x0, self.y0, mx,       my      ),
            1 => (mx,      self.y0, self.x1,  my      ),
            2 => (self.x0, my,      mx,       self.y1 ),
            _ => (mx,      my,      self.x1,  self.y1 ),
        }
    }
}

fn bh_insert(pool: &mut Vec<BHCell>, start: usize, bx: f64, by: f64, bi: usize) {
    let mut ci = start;
    loop {
        match pool[ci].body {
            -2 => {
                pool[ci].body = bi as i32;
                pool[ci].mass = 1.0;
                pool[ci].cx   = bx;
                pool[ci].cy   = by;
                return;
            }
            -1 => {
                let m = pool[ci].mass;
                pool[ci].cx   = (pool[ci].cx * m + bx) / (m + 1.0);
                pool[ci].cy   = (pool[ci].cy * m + by) / (m + 1.0);
                pool[ci].mass = m + 1.0;
                let q  = pool[ci].quad(bx, by);
                let ch = pool[ci].ch[q];
                if ch == u32::MAX {
                    let (cx0, cy0, cx1, cy1) = pool[ci].child_bounds(q);
                    let new = pool.len() as u32;
                    pool.push(BHCell::new(cx0, cy0, cx1, cy1));
                    pool[ci].ch[q] = new;
                    ci = new as usize;
                } else {
                    ci = ch as usize;
                }
            }
            _ => {
                // Leaf → split into internal, then re-insert both bodies
                let ob = pool[ci].body as usize;
                let (ox, oy) = (pool[ci].cx, pool[ci].cy);
                if (ox - bx).abs() + (oy - by).abs() < 1e-9 {
                    pool[ci].mass += 1.0; // coincident: just accumulate mass
                    return;
                }
                pool[ci].body = -1;
                pool[ci].ch   = [u32::MAX; 4];
                pool[ci].mass = 0.0;
                bh_insert(pool, ci, ox, oy, ob);
                bh_insert(pool, ci, bx, by, bi);
                return;
            }
        }
    }
}

fn bh_force(pool: &[BHCell], bx: f64, by: f64, self_bi: usize, stack: &mut Vec<usize>) -> (f64, f64) {
    let t2 = BH_THETA * BH_THETA;
    stack.clear();
    stack.push(0);
    let (mut fx, mut fy) = (0.0f64, 0.0f64);
    while let Some(ci) = stack.pop() {
        let c = &pool[ci];
        if c.body == -2 || c.body == self_bi as i32 { continue; }
        let dx = c.cx - bx;
        let dy = c.cy - by;
        let d2 = dx * dx + dy * dy;
        if d2 < 0.01 { continue; }
        let size = (c.x1 - c.x0).max(c.y1 - c.y0);
        if c.body >= 0 || size * size < t2 * d2 {
            let d = d2.sqrt();
            let f = REPULSION * c.mass / (d2 * d);
            fx -= dx * f;
            fy -= dy * f;
        } else {
            for &ch in &c.ch { if ch != u32::MAX { stack.push(ch as usize); } }
        }
    }
    (fx, fy)
}

fn bh_build(pts: &[(f64, f64)]) -> Vec<BHCell> {
    let n = pts.len();
    if n == 0 { return vec![]; }
    let (mut x0, mut x1, mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for &(x, y) in pts {
        x0 = x0.min(x); x1 = x1.max(x);
        y0 = y0.min(y); y1 = y1.max(y);
    }
    let pad = 100.0;
    let mut pool = Vec::with_capacity(n * 4);
    pool.push(BHCell::new(x0 - pad, y0 - pad, x1 + pad, y1 + pad));
    for (i, &(x, y)) in pts.iter().enumerate() { bh_insert(&mut pool, 0, x, y, i); }
    pool
}

fn prerelax(
    mut pos: Vec<(f64, f64)>,
    edges:   &[(usize, usize)],
    n:       usize,
) -> Vec<(f64, f64)> {
    if n == 0 { return pos; }
    let steps = (2_000_000usize / (n * n).max(1)).clamp(30, 240);
    let mut vel: Vec<(f64, f64)> = vec![(0.0, 0.0); n];
    let dt    = 1.0 / 60.0_f64;
    let damp  = DAMPING.powf(dt / BASE_DT);
    let max_r = (n as f64 * 15.0).max(350.0);
    const MV: f64 = 650.0;

    for _ in 0..steps {
        let mut fx = vec![0.0f64; n];
        let mut fy = vec![0.0f64; n];

        {
            let bh = bh_build(&pos);
            let rep_forces: Vec<(f64, f64)> = pos.par_iter().enumerate()
                .map_with(Vec::with_capacity(64), |stk, (i, &(x, y))| bh_force(&bh, x, y, i, stk))
                .collect();
            for (i, (rfx, rfy)) in rep_forces.into_iter().enumerate() {
                fx[i] += rfx; fy[i] += rfy;
            }
        }
        for &(si, di) in edges {
            let dx   = pos[di].0 - pos[si].0;
            let dy   = pos[di].1 - pos[si].1;
            let dist = dx.hypot(dy).max(1e-4);
            let f    = SPRING_K * (dist - SPRING_L) / dist;
            fx[si] += dx * f; fy[si] += dy * f;
            fx[di] -= dx * f; fy[di] -= dy * f;
        }
        for i in 0..n {
            vel[i].0 = (vel[i].0 + (fx[i] - pos[i].0 * GRAVITY) * dt) * damp;
            vel[i].1 = (vel[i].1 + (fy[i] - pos[i].1 * GRAVITY) * dt) * damp;
            let spd = vel[i].0.hypot(vel[i].1);
            if spd > MV { vel[i].0 = vel[i].0 / spd * MV; vel[i].1 = vel[i].1 / spd * MV; }
            pos[i].0 += vel[i].0 * dt;
            pos[i].1 += vel[i].1 * dt;
            let d = pos[i].0.hypot(pos[i].1);
            if d > max_r { let s = max_r / d; pos[i].0 *= s; pos[i].1 *= s; vel[i] = (0.0, 0.0); }
        }
    }
    pos
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn lcg_uniform(seed: u64) -> f64 {
    let x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (x >> 11) as f64 / (1u64 << 53) as f64
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> [u8; 3] {
    let i = (h * 6.0) as u32;
    let f = h * 6.0 - i as f64;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match i % 6 { 0=>(v,t,p), 1=>(q,v,p), 2=>(p,v,t), 3=>(p,q,v), 4=>(t,p,v), _=>(v,p,q) };
    [(r*255.0) as u8, (g*255.0) as u8, (b*255.0) as u8]
}
