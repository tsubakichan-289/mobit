use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use num_complex::Complex64;

const VIEW_X_MIN: f64 = -4.0;
const VIEW_X_MAX: f64 = 4.0;
const VIEW_Y_MIN: f64 = 0.0;
const VIEW_Y_MAX: f64 = 4.0;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 760.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Mobius Orbit Visualizer",
        options,
        Box::new(|_cc| Ok(Box::<MobiusApp>::default())),
    )
}

struct MobiusApp {
    n: usize,
    selected_z: Option<Complex64>,
    highlighted_path: Option<Vec<Action>>,
    highlighted_target: Option<Complex64>,
    scroll_accum: f32,
}

impl Default for MobiusApp {
    fn default() -> Self {
        Self {
            n: 4,
            selected_z: Some(Complex64::new(0.0, 1.0)),
            highlighted_path: None,
            highlighted_target: None,
            scroll_accum: 0.0,
        }
    }
}

impl eframe::App for MobiusApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();
        self.scroll_accum += ctx.input(|i| i.raw_scroll_delta.y);
        let wheel_step = 120.0_f32;
        while self.scroll_accum >= wheel_step {
            self.n = (self.n + 1).min(10);
            self.scroll_accum -= wheel_step;
        }
        while self.scroll_accum <= -wheel_step {
            self.n = self.n.saturating_sub(1).max(1);
            self.scroll_accum += wheel_step;
        }
        egui::TopBottomPanel::top("controls").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("n (iterations / recursion depth)");
                ui.add(egui::Slider::new(&mut self.n, 1..=10));
                if self.selected_z.is_none() {
                    self.selected_z = Some(Complex64::new(0.0, 1.0));
                }
                if let Some(z) = self.selected_z {
                    ui.separator();
                    ui.label(format!("selected z = {:.5} + {:.5}i", z.re, z.im));
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let total = ui.available_rect_before_wrap();
            let margin = 8.0;
            let half_width = (total.width() - margin) * 0.5;
            let left_rect = Rect::from_min_size(total.min, Vec2::new(half_width, total.height()));
            let right_rect = Rect::from_min_size(
                Pos2::new(total.min.x + half_width + margin, total.min.y),
                Vec2::new(half_width, total.height()),
            );

            let left_response = ui.allocate_rect(left_rect, Sense::hover());
            let right_response = ui.allocate_rect(right_rect, Sense::click_and_drag());
            let painter = ui.painter();

            let selected = self.selected_z.unwrap_or(Complex64::new(0.0, 1.0));
            let l_orbit = iterate_orbit(selected, self.n, Action::L);
            let r_orbit = iterate_orbit(selected, self.n, Action::R);
            let linv_orbit = iterate_orbit(selected, self.n, Action::LInv);
            let rinv_orbit = iterate_orbit(selected, self.n, Action::RInv);
            let mut rec_edges = Vec::new();
            let mut rec_nodes = vec![OrbitNode {
                z: selected,
                path: Vec::new(),
            }];
            let mut path_buf = Vec::new();
            build_recursive_tree(
                selected,
                self.n,
                None,
                &mut path_buf,
                &mut rec_edges,
                &mut rec_nodes,
            );
            let highlighted_orbit = self
                .highlighted_path
                .as_ref()
                .map(|path| orbit_from_actions(selected, path));

            draw_upper_half_plane(
                painter,
                left_rect,
                &l_orbit,
                &r_orbit,
                &linv_orbit,
                &rinv_orbit,
                &rec_edges,
                highlighted_orbit.as_deref(),
                selected,
                left_response.hovered(),
            );

            let clickable = draw_unit_disk(
                painter,
                right_rect,
                &l_orbit,
                &r_orbit,
                &linv_orbit,
                &rinv_orbit,
                &rec_edges,
                &rec_nodes,
                highlighted_orbit.as_deref(),
                self.highlighted_target,
                selected,
                right_response.hovered(),
            );

            if right_response.dragged() || right_response.drag_started() {
                if let Some(pointer) = right_response.interact_pointer_pos() {
                    if let Some(z) = pick_disk_point(pointer, right_rect) {
                        self.selected_z = Some(z);
                        self.highlighted_path = None;
                        self.highlighted_target = None;
                    }
                }
            } else if right_response.clicked() {
                if let Some(pointer) = right_response.interact_pointer_pos() {
                    if let Some(node) = pick_circle(pointer, &clickable) {
                        self.highlighted_path = Some(node.path.clone());
                        self.highlighted_target = Some(node.z);
                    } else if let Some(z) = pick_disk_point(pointer, right_rect) {
                        self.selected_z = Some(z);
                        self.highlighted_path = None;
                        self.highlighted_target = None;
                    }
                }
            }
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    L,
    R,
    LInv,
    RInv,
}

fn apply_action(z: Complex64, action: Action) -> Complex64 {
    match action {
        Action::L => z + Complex64::new(1.0, 0.0),
        Action::R => z / (z + Complex64::new(1.0, 0.0)),
        Action::LInv => z - Complex64::new(1.0, 0.0),
        Action::RInv => {
            let one = Complex64::new(1.0, 0.0);
            z / (one - z)
        }
    }
}

fn inverse_action(action: Action) -> Action {
    match action {
        Action::L => Action::LInv,
        Action::R => Action::RInv,
        Action::LInv => Action::L,
        Action::RInv => Action::R,
    }
}

#[derive(Clone)]
struct OrbitNode {
    z: Complex64,
    path: Vec<Action>,
}

#[derive(Clone)]
struct ClickableNode {
    pos: Pos2,
    z: Complex64,
    path: Vec<Action>,
}

fn iterate_orbit(start: Complex64, n: usize, action: Action) -> Vec<Complex64> {
    let mut out = Vec::with_capacity(n + 1);
    let mut z = start;
    out.push(z);
    for _ in 0..n {
        z = apply_action(z, action);
        if !z.re.is_finite() || !z.im.is_finite() {
            break;
        }
        out.push(z);
    }
    out
}

fn build_recursive_tree(
    root: Complex64,
    depth: usize,
    prev_action: Option<Action>,
    path: &mut Vec<Action>,
    edges: &mut Vec<(Complex64, Complex64, Action)>,
    nodes: &mut Vec<OrbitNode>,
) {
    if depth == 0 {
        return;
    }
    let actions = [Action::L, Action::R, Action::LInv, Action::RInv];
    for action in actions {
        if let Some(prev) = prev_action {
            if action == inverse_action(prev) {
                continue;
            }
        }
        let child = apply_action(root, action);
        if child.re.is_finite() && child.im.is_finite() {
            edges.push((root, child, action));
            path.push(action);
            nodes.push(OrbitNode {
                z: child,
                path: path.clone(),
            });
            build_recursive_tree(child, depth - 1, Some(action), path, edges, nodes);
            path.pop();
        }
    }
}

fn orbit_from_actions(start: Complex64, actions: &[Action]) -> Vec<Complex64> {
    let mut orbit = Vec::with_capacity(actions.len() + 1);
    let mut z = start;
    orbit.push(z);
    for &action in actions {
        z = apply_action(z, action);
        if !z.re.is_finite() || !z.im.is_finite() {
            break;
        }
        orbit.push(z);
    }
    orbit
}

fn s_transform(z: Complex64) -> Complex64 {
    -Complex64::new(1.0, 0.0) / z
}

fn in_f(z: Complex64) -> bool {
    z.im > 0.0 && z.re.abs() <= 0.5 && z.norm() >= 1.0
}

fn in_fs(z: Complex64) -> bool {
    let pre = s_transform(z);
    pre.re.is_finite() && pre.im.is_finite() && in_f(pre)
}

fn cayley_to_disk(z: Complex64) -> Complex64 {
    (z - Complex64::new(0.0, 1.0)) / (z + Complex64::new(0.0, 1.0))
}

fn disk_to_upper(w: Complex64) -> Option<Complex64> {
    let one = Complex64::new(1.0, 0.0);
    let i = Complex64::new(0.0, 1.0);
    let denom = one - w;
    if denom.norm() < 1e-10 {
        return None;
    }
    Some(i * (one + w) / denom)
}

fn world_to_screen_upper(rect: Rect, z: Complex64) -> Option<Pos2> {
    if z.im < -1e-6 || !z.re.is_finite() || !z.im.is_finite() {
        return None;
    }
    let x = (z.re - VIEW_X_MIN) / (VIEW_X_MAX - VIEW_X_MIN);
    let y = (z.im - VIEW_Y_MIN) / (VIEW_Y_MAX - VIEW_Y_MIN);
    Some(Pos2::new(
        rect.left() + rect.width() * x as f32,
        rect.bottom() - rect.height() * y as f32,
    ))
}

fn world_to_screen_disk(rect: Rect, w: Complex64) -> Option<Pos2> {
    if !w.re.is_finite() || !w.im.is_finite() {
        return None;
    }
    let r2 = w.re * w.re + w.im * w.im;
    if r2 > 1.0 + 1e-9 {
        return None;
    }
    let center = rect.center();
    let radius = 0.47 * rect.width().min(rect.height());
    Some(Pos2::new(
        center.x + (w.re as f32) * radius,
        center.y - (w.im as f32) * radius,
    ))
}

fn draw_axes_upper(painter: &egui::Painter, rect: Rect) {
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, Color32::DARK_GRAY),
        egui::StrokeKind::Outside,
    );
    let x0 = world_to_screen_upper(rect, Complex64::new(0.0, 0.01))
        .map(|p| p.x)
        .unwrap_or(rect.center().x);
    painter.line_segment(
        [
            Pos2::new(rect.left(), rect.bottom()),
            Pos2::new(rect.right(), rect.bottom()),
        ],
        Stroke::new(1.0, Color32::GRAY),
    );
    painter.line_segment(
        [Pos2::new(x0, rect.top()), Pos2::new(x0, rect.bottom())],
        Stroke::new(1.0, Color32::GRAY),
    );
}

fn f_boundary_points() -> Vec<Complex64> {
    let mut pts = Vec::new();
    let y0 = (3.0f64).sqrt() * 0.5;
    let n_side = 40usize;
    let n_arc = 120usize;
    let n_top = 80usize;
    for i in 0..=n_side {
        let t = i as f64 / n_side as f64;
        let y = VIEW_Y_MAX - t * (VIEW_Y_MAX - y0);
        pts.push(Complex64::new(0.5, y));
    }
    for i in 0..=n_arc {
        let t = i as f64 / n_arc as f64;
        let theta = std::f64::consts::PI / 3.0 + t * (std::f64::consts::PI / 3.0);
        pts.push(Complex64::new(theta.cos(), theta.sin()));
    }
    for i in 0..=n_side {
        let t = i as f64 / n_side as f64;
        let y = y0 + t * (VIEW_Y_MAX - y0);
        pts.push(Complex64::new(-0.5, y));
    }
    for i in 0..=n_top {
        let t = i as f64 / n_top as f64;
        let x = -0.5 + t;
        pts.push(Complex64::new(x, VIEW_Y_MAX));
    }
    pts
}

fn fs_boundary_points() -> Vec<Complex64> {
    let mut pts = Vec::new();
    let n_side = 120usize;
    let n_top = 140usize;

    // Left arc: center -1, radius 1, from 0 to -1/2 + i*sqrt(3)/2
    for i in 0..=n_side {
        let t = i as f64 / n_side as f64;
        let theta = 0.0 + t * (std::f64::consts::PI / 3.0);
        pts.push(Complex64::new(-1.0 + theta.cos(), theta.sin()));
    }
    // Top arc on |z|=1: from -1/2 + i*sqrt(3)/2 to 1/2 + i*sqrt(3)/2
    for i in 0..=n_top {
        let t = i as f64 / n_top as f64;
        let theta = 2.0 * std::f64::consts::PI / 3.0 - t * (std::f64::consts::PI / 3.0);
        pts.push(Complex64::new(theta.cos(), theta.sin()));
    }
    // Right arc: center +1, radius 1, from 1/2 + i*sqrt(3)/2 back to 0
    for i in 0..=n_side {
        let t = i as f64 / n_side as f64;
        let theta = 2.0 * std::f64::consts::PI / 3.0 + t * (std::f64::consts::PI / 3.0);
        pts.push(Complex64::new(1.0 + theta.cos(), theta.sin()));
    }
    pts
}

fn draw_fan_polygon_upper(
    painter: &egui::Painter,
    rect: Rect,
    boundary_z: &[Complex64],
    center_z: Complex64,
    fill: Color32,
    stroke: Color32,
) {
    let Some(c) = world_to_screen_upper(rect, center_z) else {
        return;
    };
    let boundary: Vec<Pos2> = boundary_z
        .iter()
        .copied()
        .filter_map(|z| world_to_screen_upper(rect, z))
        .collect();
    if boundary.len() < 3 {
        return;
    }

    let mut mesh = egui::epaint::Mesh::default();
    let ci = mesh.vertices.len() as u32;
    mesh.colored_vertex(c, fill);
    for &p in &boundary {
        mesh.colored_vertex(p, fill);
    }
    for i in 0..boundary.len() {
        let a = ci + 1 + i as u32;
        let b = ci + 1 + ((i + 1) % boundary.len()) as u32;
        mesh.add_triangle(ci, a, b);
    }
    painter.add(egui::Shape::mesh(mesh));

    for pair in boundary.windows(2) {
        painter.line_segment([pair[0], pair[1]], Stroke::new(1.5, stroke));
    }
    painter.line_segment(
        [*boundary.last().unwrap_or(&boundary[0]), boundary[0]],
        Stroke::new(1.5, stroke),
    );
}

fn f_boundary_points_disk() -> Vec<Complex64> {
    let mut pts = Vec::new();
    let y0 = (3.0f64).sqrt() * 0.5;
    let y_max = 1.0e6f64;
    let n_side = 180usize;
    let n_arc = 180usize;

    // Infinity cusp (y -> +infty) under Cayley transform.
    pts.push(Complex64::new(1.0, 0.0));

    // Right vertical boundary x=1/2 from large y down to y0.
    for i in 0..=n_side {
        let t = i as f64 / n_side as f64;
        let y = y_max.powf(1.0 - t) * y0.powf(t);
        pts.push(cayley_to_disk(Complex64::new(0.5, y)));
    }

    // Unit-circle arc |z|=1 from theta=pi/3 to 2pi/3.
    for i in 0..=n_arc {
        let t = i as f64 / n_arc as f64;
        let theta = std::f64::consts::PI / 3.0 + t * (std::f64::consts::PI / 3.0);
        pts.push(cayley_to_disk(Complex64::new(theta.cos(), theta.sin())));
    }

    // Left vertical boundary x=-1/2 from y0 up to large y.
    for i in 0..=n_side {
        let t = i as f64 / n_side as f64;
        let y = y0.powf(1.0 - t) * y_max.powf(t);
        pts.push(cayley_to_disk(Complex64::new(-0.5, y)));
    }

    pts
}

fn draw_fan_polygon_in_disk(
    painter: &egui::Painter,
    rect: Rect,
    boundary_w: &[Complex64],
    fill: Color32,
    stroke: Color32,
) {
    let center = world_to_screen_disk(rect, Complex64::new(0.0, 0.0));
    let boundary: Vec<Pos2> = boundary_w
        .iter()
        .copied()
        .filter_map(|w| world_to_screen_disk(rect, w))
        .collect();
    if boundary.len() < 3 {
        return;
    }
    let Some(c) = center else {
        return;
    };

    let mut mesh = egui::epaint::Mesh::default();
    let ci = mesh.vertices.len() as u32;
    mesh.colored_vertex(c, fill);
    for &p in &boundary {
        mesh.colored_vertex(p, fill);
    }
    for i in 0..boundary.len() {
        let a = ci + 1 + i as u32;
        let b = ci + 1 + ((i + 1) % boundary.len()) as u32;
        mesh.add_triangle(ci, a, b);
    }
    painter.add(egui::Shape::mesh(mesh));

    for pair in boundary.windows(2) {
        painter.line_segment([pair[0], pair[1]], Stroke::new(1.5, stroke));
    }
    painter.line_segment(
        [*boundary.last().unwrap_or(&boundary[0]), boundary[0]],
        Stroke::new(1.5, stroke),
    );
}

fn draw_domain_background_upper(painter: &egui::Painter, rect: Rect) {
    let f_boundary = f_boundary_points();
    let fs_boundary = fs_boundary_points();
    draw_fan_polygon_upper(
        painter,
        rect,
        &f_boundary,
        Complex64::new(0.0, 2.5),
        Color32::from_rgba_unmultiplied(250, 120, 120, 28),
        Color32::from_rgb(250, 130, 130),
    );
    draw_fan_polygon_upper(
        painter,
        rect,
        &fs_boundary,
        Complex64::new(0.0, 0.95),
        Color32::from_rgba_unmultiplied(120, 170, 250, 28),
        Color32::from_rgb(130, 180, 255),
    );
}

fn draw_orbit_path_upper(
    painter: &egui::Painter,
    rect: Rect,
    orbit: &[Complex64],
    color: Color32,
    radius: f32,
) {
    for pair in orbit.windows(2) {
        if let (Some(a), Some(b)) = (
            world_to_screen_upper(rect, pair[0]),
            world_to_screen_upper(rect, pair[1]),
        ) {
            painter.line_segment([a, b], Stroke::new(1.5, color));
        }
    }
    for &z in orbit {
        if let Some(p) = world_to_screen_upper(rect, z) {
            painter.circle_filled(p, radius, color);
        }
    }
}

fn draw_recursive_upper(
    painter: &egui::Painter,
    rect: Rect,
    edges: &[(Complex64, Complex64, Action)],
) {
    for &(a, b, action) in edges {
        let c = match action {
            Action::L => Color32::from_rgba_unmultiplied(255, 120, 120, 110),
            Action::R => Color32::from_rgba_unmultiplied(120, 180, 255, 110),
            Action::LInv => Color32::from_rgba_unmultiplied(255, 210, 120, 110),
            Action::RInv => Color32::from_rgba_unmultiplied(140, 235, 220, 110),
        };
        if let (Some(pa), Some(pb)) = (
            world_to_screen_upper(rect, a),
            world_to_screen_upper(rect, b),
        ) {
            painter.line_segment([pa, pb], Stroke::new(1.0, c));
        }
    }
}

fn draw_domain_background_disk(painter: &egui::Painter, rect: Rect) {
    let f_boundary_w = f_boundary_points_disk();
    let fs_boundary_w: Vec<Complex64> = fs_boundary_points()
        .into_iter()
        .map(cayley_to_disk)
        .collect();

    draw_fan_polygon_in_disk(
        painter,
        rect,
        &f_boundary_w,
        Color32::from_rgba_unmultiplied(250, 120, 120, 28),
        Color32::from_rgb(250, 130, 130),
    );
    draw_fan_polygon_in_disk(
        painter,
        rect,
        &fs_boundary_w,
        Color32::from_rgba_unmultiplied(120, 170, 250, 28),
        Color32::from_rgb(130, 180, 255),
    );
}

fn draw_upper_half_plane(
    painter: &egui::Painter,
    rect: Rect,
    l_orbit: &[Complex64],
    r_orbit: &[Complex64],
    linv_orbit: &[Complex64],
    rinv_orbit: &[Complex64],
    rec_edges: &[(Complex64, Complex64, Action)],
    highlighted_orbit: Option<&[Complex64]>,
    selected: Complex64,
    hovered: bool,
) {
    painter.rect_filled(
        rect,
        0.0,
        if hovered {
            Color32::from_rgb(20, 20, 26)
        } else {
            Color32::from_rgb(16, 16, 20)
        },
    );
    draw_domain_background_upper(painter, rect);
    draw_axes_upper(painter, rect);
    draw_recursive_upper(painter, rect, rec_edges);
    draw_orbit_path_upper(
        painter,
        rect,
        l_orbit,
        Color32::from_rgb(255, 110, 110),
        3.0,
    );
    draw_orbit_path_upper(
        painter,
        rect,
        r_orbit,
        Color32::from_rgb(110, 170, 255),
        3.0,
    );
    draw_orbit_path_upper(
        painter,
        rect,
        linv_orbit,
        Color32::from_rgb(255, 210, 120),
        3.0,
    );
    draw_orbit_path_upper(
        painter,
        rect,
        rinv_orbit,
        Color32::from_rgb(140, 235, 220),
        3.0,
    );
    if let Some(orbit) = highlighted_orbit {
        draw_orbit_path_upper(painter, rect, orbit, Color32::YELLOW, 4.5);
    }
    if let Some(p) = world_to_screen_upper(rect, selected) {
        painter.circle_stroke(p, 7.0, Stroke::new(2.0, Color32::YELLOW));
    }
    painter.text(
        rect.left_top() + Vec2::new(8.0, 8.0),
        egui::Align2::LEFT_TOP,
        "Upper half-plane",
        egui::FontId::proportional(15.0),
        Color32::WHITE,
    );
}

fn draw_orbit_path_disk(
    painter: &egui::Painter,
    rect: Rect,
    orbit: &[Complex64],
    color: Color32,
    radius: f32,
) {
    for pair in orbit.windows(2) {
        let a = cayley_to_disk(pair[0]);
        let b = cayley_to_disk(pair[1]);
        if let (Some(pa), Some(pb)) = (world_to_screen_disk(rect, a), world_to_screen_disk(rect, b))
        {
            painter.line_segment([pa, pb], Stroke::new(1.5, color));
        }
    }
    for &z in orbit {
        let w = cayley_to_disk(z);
        if let Some(p) = world_to_screen_disk(rect, w) {
            painter.circle_filled(p, radius, color);
        }
    }
}

fn draw_recursive_disk(
    painter: &egui::Painter,
    rect: Rect,
    edges: &[(Complex64, Complex64, Action)],
) {
    for &(a, b, action) in edges {
        let c = match action {
            Action::L => Color32::from_rgba_unmultiplied(255, 120, 120, 100),
            Action::R => Color32::from_rgba_unmultiplied(120, 180, 255, 100),
            Action::LInv => Color32::from_rgba_unmultiplied(255, 210, 120, 100),
            Action::RInv => Color32::from_rgba_unmultiplied(140, 235, 220, 100),
        };
        let wa = cayley_to_disk(a);
        let wb = cayley_to_disk(b);
        if let (Some(pa), Some(pb)) = (
            world_to_screen_disk(rect, wa),
            world_to_screen_disk(rect, wb),
        ) {
            painter.line_segment([pa, pb], Stroke::new(1.0, c));
        }
    }
}

fn draw_unit_disk(
    painter: &egui::Painter,
    rect: Rect,
    l_orbit: &[Complex64],
    r_orbit: &[Complex64],
    linv_orbit: &[Complex64],
    rinv_orbit: &[Complex64],
    rec_edges: &[(Complex64, Complex64, Action)],
    rec_nodes: &[OrbitNode],
    highlighted_orbit: Option<&[Complex64]>,
    highlighted_target: Option<Complex64>,
    selected: Complex64,
    hovered: bool,
) -> Vec<ClickableNode> {
    painter.rect_filled(
        rect,
        0.0,
        if hovered {
            Color32::from_rgb(18, 24, 30)
        } else {
            Color32::from_rgb(14, 20, 24)
        },
    );
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, Color32::DARK_GRAY),
        egui::StrokeKind::Outside,
    );
    let center = rect.center();
    let radius = 0.47 * rect.width().min(rect.height());
    painter.circle_stroke(center, radius, Stroke::new(2.0, Color32::LIGHT_GRAY));
    draw_domain_background_disk(painter, rect);

    draw_recursive_disk(painter, rect, rec_edges);
    draw_orbit_path_disk(
        painter,
        rect,
        l_orbit,
        Color32::from_rgb(255, 110, 110),
        3.0,
    );
    draw_orbit_path_disk(
        painter,
        rect,
        r_orbit,
        Color32::from_rgb(110, 170, 255),
        3.0,
    );
    draw_orbit_path_disk(
        painter,
        rect,
        linv_orbit,
        Color32::from_rgb(255, 210, 120),
        3.0,
    );
    draw_orbit_path_disk(
        painter,
        rect,
        rinv_orbit,
        Color32::from_rgb(140, 235, 220),
        3.0,
    );
    if let Some(orbit) = highlighted_orbit {
        draw_orbit_path_disk(painter, rect, orbit, Color32::YELLOW, 4.5);
    }

    let mut clickable = Vec::new();
    for node in rec_nodes {
        let w = cayley_to_disk(node.z);
        if let Some(p) = world_to_screen_disk(rect, w) {
            painter.circle_filled(p, 4.0, Color32::from_rgb(210, 210, 210));
            if let Some(target) = highlighted_target {
                if (node.z - target).norm() < 1e-9 {
                    painter.circle_stroke(p, 6.5, Stroke::new(2.0, Color32::YELLOW));
                }
            }
            clickable.push(ClickableNode {
                pos: p,
                z: node.z,
                path: node.path.clone(),
            });
        }
    }
    if let Some(sel) = world_to_screen_disk(rect, cayley_to_disk(selected)) {
        painter.circle_stroke(sel, 8.0, Stroke::new(2.0, Color32::YELLOW));
    }
    painter.text(
        rect.left_top() + Vec2::new(8.0, 8.0),
        egui::Align2::LEFT_TOP,
        "Unit disk (Cayley)",
        egui::FontId::proportional(15.0),
        Color32::WHITE,
    );
    clickable
}

fn pick_circle(pointer: Pos2, circles: &[ClickableNode]) -> Option<&ClickableNode> {
    let mut best_index: Option<(f32, usize)> = None;
    for (i, node) in circles.iter().enumerate() {
        let d2 = pointer.distance_sq(node.pos);
        if d2 <= 9.0f32 * 9.0 {
            match best_index {
                Some((best_d2, _)) if d2 >= best_d2 => {}
                _ => best_index = Some((d2, i)),
            }
        }
    }
    best_index.map(|(_, i)| &circles[i])
}

fn pick_disk_point(pointer: Pos2, rect: Rect) -> Option<Complex64> {
    let center = rect.center();
    let radius = 0.47 * rect.width().min(rect.height());
    let x = (pointer.x - center.x) / radius;
    let y = (center.y - pointer.y) / radius;
    let w = Complex64::new(x as f64, y as f64);
    if w.norm_sqr() >= 1.0 {
        return None;
    }
    disk_to_upper(w).filter(|z| z.im > 0.0 && z.re.is_finite() && z.im.is_finite())
}
