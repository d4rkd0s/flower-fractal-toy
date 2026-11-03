use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::math::EulerRot;
use bevy::prelude::*;
use bevy::window::{PresentMode, PrimaryWindow, WindowLevel, WindowResolution};
use bevy::winit::{UpdateMode, WinitSettings};
use bevy_egui::{EguiContexts, EguiPlugin, egui};
use std::f32::consts::TAU;
use std::time::Duration;

/// Number of shared materials. Seeds are bucketed by hue into this palette so
/// Bevy can instance them (one draw call per material) instead of issuing one
/// draw call per seed. 2° steps are indistinguishable from per-seed hues.
const PALETTE_SIZE: usize = 180;

#[derive(Clone)]
struct LayerParams {
    n: usize,
    divergence_deg: f32,
    scale_k: f32,
    dish_amp: f32,
    dish_exp: f32,           // base dish exponent (UI-controlled)
    effective_dish_exp: f32, // modulated exponent used in build
    cap_amp: f32,
    tangent_tilt: f32,
    // Breathe mode controls
    breathe_mode: bool,
    breathe_amp: f32,   // amplitude added to dish_exp
    breathe_speed: f32, // Hz
    // Spin mode controls
    spin_mode: bool,
    spin_rate_deg_per_sec: f32, // degrees per second
    // Seed visual size multiplier
    seed_size: f32,
    // Integrated breathe phase (radians) so speed changes never jump the sine.
    breathe_phase: f32,
    // Drift mode: every parameter wanders toward random targets, forever.
    drift: bool,
    drift_targets: Option<DriftTargets>,
    dirty: bool,
}

/// Which parameters drift, their safe ranges, and the current target of each.
/// Order: scale_k, dish_amp, dish_exp, cap_amp, tangent_tilt, seed_size,
/// breathe_amp, breathe_speed, spin_rate.
const DRIFT_RANGES: [(f32, f32); 9] = [
    (0.10, 0.20),   // scale_k
    (0.50, 4.00),   // dish_amp
    (0.50, 4.00),   // dish_exp
    (0.00, 2.00),   // cap_amp
    (0.00, 1.00),   // tangent_tilt
    (1.00, 3.50),   // seed_size
    (0.50, 2.00),   // breathe_amp
    (0.20, 1.20),   // breathe_speed (Hz)
    (-0.08, 0.08),  // spin rate (deg/s)
];

#[derive(Clone, Copy)]
struct DriftTargets {
    target: [f32; 9],
    /// Seconds for a value to close ~63% of the gap to its target.
    tau: [f32; 9],
}

impl LayerParams {
    fn drift_values(&mut self) -> [&mut f32; 9] {
        [
            &mut self.scale_k,
            &mut self.dish_amp,
            &mut self.dish_exp,
            &mut self.cap_amp,
            &mut self.tangent_tilt,
            &mut self.seed_size,
            &mut self.breathe_amp,
            &mut self.breathe_speed,
            &mut self.spin_rate_deg_per_sec,
        ]
    }
}

/// Tiny xorshift64* generator; enough for picking drift targets.
#[derive(Resource)]
struct Rng(u64);

impl Rng {
    fn seeded() -> Self {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Rng(t | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in [0, 1).
    fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }
}

/// Immutable, `Copy` snapshot of the geometry inputs for one layer, so seed
/// placement can run on every core without touching the ECS resource.
#[derive(Clone, Copy)]
struct LayerGeom {
    divergence: f32,
    k: f32,
    r_max: f32,
    dish_amp: f32,
    dish_exp: f32,
    cap_amp: f32,
    tilt: f32,
    seed_size: f32,
    dirty: bool,
}

impl LayerGeom {
    fn from_params(p: &LayerParams) -> Self {
        let k = p.scale_k;
        Self {
            divergence: p.divergence_deg.to_radians(),
            k,
            r_max: k * (p.n as f32).sqrt(),
            dish_amp: p.dish_amp,
            dish_exp: p.effective_dish_exp,
            cap_amp: p.cap_amp,
            tilt: p.tangent_tilt,
            seed_size: p.seed_size,
            dirty: p.dirty,
        }
    }

    #[inline]
    fn point(&self, i: f32) -> Vec3 {
        let theta = i * self.divergence;
        let r = self.k * i.sqrt();
        let (s, c) = theta.sin_cos();
        let r_ratio = if self.r_max > 0.0 { r / self.r_max } else { 0.0 };
        let mut z = self.dish_amp * r_ratio.powf(self.dish_exp);
        if self.cap_amp != 0.0 {
            z += self.cap_amp * (1.0 - r_ratio * r_ratio).max(0.0).sqrt();
        }
        Vec3::new(r * c, r * s, z)
    }

    /// Position and tangent-aligned rotation of seed `i`.
    #[inline]
    fn place(&self, i: u32) -> (Vec3, Quat) {
        let p = self.point(i as f32);
        let d = self.point(i as f32 + 1.0) - p; // forward difference
        let yaw = d.y.atan2(d.x);
        let horiz = (d.x * d.x + d.y * d.y).sqrt();
        let pitch = d.z.atan2(horiz) * self.tilt;
        (p, Quat::from_euler(EulerRot::ZYX, yaw, -pitch, 0.0))
    }

    #[inline]
    fn transform(&self, i: u32) -> Transform {
        let (translation, rotation) = self.place(i);
        Transform {
            translation,
            rotation,
            scale: Vec3::splat(self.seed_size),
        }
    }
}

#[derive(Resource)]
struct PhylloParams {
    layers: Vec<LayerParams>,
}

#[derive(Resource, Clone)]
struct SeedMesh(Handle<Mesh>);

#[derive(Resource)]
struct SeedPalette(Vec<Handle<StandardMaterial>>);

#[derive(Component)]
struct Seed {
    layer: u32,
    index: u32,
}

// Keep a stable list of seed entities to avoid per-frame spawn/despawn churn
#[derive(Resource, Default)]
struct SeedEntities {
    lists: Vec<Vec<Entity>>,
}

/// Command-line configuration.
#[derive(Resource, Clone, Copy)]
struct Config {
    /// Run as a desktop wallpaper: borderless, covers the screen, sits under
    /// every other window, ignores the mouse, no controls.
    wallpaper: bool,
    /// Frame-rate cap (frames per second).
    fps: f64,
    /// Parameters drift randomly forever (default on in wallpaper mode).
    drift: bool,
}

fn parse_args() -> Config {
    let mut cfg = Config {
        wallpaper: false,
        fps: 60.0,
        drift: false,
    };
    let mut fps_set = false;
    let mut drift_set = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--wallpaper" | "-w" => cfg.wallpaper = true,
            "--drift" => {
                cfg.drift = true;
                drift_set = true;
            }
            "--no-drift" => {
                cfg.drift = false;
                drift_set = true;
            }
            "--fps" => {
                if let Some(v) = args.next().and_then(|v| v.parse::<f64>().ok()) {
                    cfg.fps = v.clamp(1.0, 240.0);
                    fps_set = true;
                }
            }
            "-h" | "--help" => {
                println!(
                    "flower-fractal-toy [--wallpaper] [--fps N] [--drift|--no-drift]\n\n  \
                     --wallpaper, -w   run as the desktop wallpaper (macOS: below icons, all Spaces)\n  \
                     --fps N           cap updates at N frames per second (default 60; 30 in wallpaper mode)\n  \
                     --drift           let every parameter wander randomly forever (default in wallpaper mode)\n  \
                     --no-drift        keep parameters fixed unless changed in the UI"
                );
                std::process::exit(0);
            }
            _ => {}
        }
    }
    if cfg.wallpaper && !fps_set {
        cfg.fps = 30.0;
    }
    if cfg.wallpaper && !drift_set {
        cfg.drift = true;
    }
    cfg
}

fn main() {
    let cfg = parse_args();

    let window = if cfg.wallpaper {
        Window {
            title: "Flower Fractal Wallpaper".into(),
            decorations: false,
            resizable: false,
            focused: false,
            // Stay hidden until the native window has been dropped to
            // desktop level so it never flashes above other windows.
            visible: false,
            window_level: WindowLevel::AlwaysOnBottom,
            present_mode: PresentMode::AutoVsync,
            resolution: WindowResolution::new(1280.0, 800.0),
            ..default()
        }
    } else {
        Window {
            title: "Flower Fractal Toy".into(),
            present_mode: PresentMode::AutoVsync,
            ..default()
        }
    };

    let frame = Duration::from_secs_f64(1.0 / cfg.fps);

    let mut app = App::new();
    app
        // Darker background for contrast
        .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.03)))
        .insert_resource(cfg)
        .insert_resource(Rng::seeded())
        // Same cap whether or not the window has focus: a wallpaper never has focus.
        .insert_resource(WinitSettings {
            focused_mode: UpdateMode::reactive_low_power(frame),
            unfocused_mode: UpdateMode::reactive_low_power(frame),
        })
        .insert_resource(PhylloParams {
            layers: vec![
                LayerParams {
                    // Layer 1 defaults (per new starting values)
                    n: 7_000,
                    divergence_deg: 142.2505,
                    scale_k: 0.160,
                    dish_amp: 2.0,
                    dish_exp: 2.0,
                    effective_dish_exp: 2.0,
                    cap_amp: 0.0,
                    tangent_tilt: 0.53,
                    breathe_mode: true,
                    breathe_amp: 2.0,
                    breathe_speed: 0.65, // Hz
                    spin_mode: true,
                    spin_rate_deg_per_sec: 0.02,
                    seed_size: 1.60,
                    breathe_phase: 0.0,
                    drift: cfg.drift,
                    drift_targets: None,
                    dirty: true,
                },
                LayerParams {
                    // Layer 2 defaults (per new starting values)
                    n: 10_000,
                    divergence_deg: 143.854,
                    scale_k: 0.140,
                    dish_amp: 2.0,
                    dish_exp: 2.0,
                    effective_dish_exp: 2.0,
                    cap_amp: 0.0,
                    tangent_tilt: 0.70,
                    breathe_mode: true,
                    breathe_amp: 2.0,
                    breathe_speed: 0.80,
                    spin_mode: true,
                    spin_rate_deg_per_sec: 0.01,
                    seed_size: 2.40,
                    breathe_phase: 0.0,
                    drift: cfg.drift,
                    drift_targets: None,
                    dirty: true,
                },
            ],
        })
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(window),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (animate_update, sync_seed_count, place_seeds).chain(),
        );

    if cfg.wallpaper {
        #[cfg(target_os = "macos")]
        app.add_systems(Update, macos_wallpaper::pin_to_desktop);
        #[cfg(not(target_os = "macos"))]
        app.add_systems(Update, show_window_once);
    } else {
        app.add_plugins(EguiPlugin)
            .add_systems(Update, ui_overlay);
    }

    app.run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Camera. Seeds are unlit, so tonemapping is explicitly off: colours come
    // out exactly as specified and the tonemapping pass is skipped entirely.
    // No light is spawned: the materials are unlit, so a light (and the
    // shadow-map passes the old one triggered) contributed nothing.
    commands.spawn(Camera3dBundle {
        camera: Camera {
            hdr: false,
            ..default()
        },
        tonemapping: Tonemapping::None,
        transform: Transform::from_xyz(0.0, 0.0, 25.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });

    let mesh = meshes.add(Mesh::from(Cuboid::new(0.05, 0.05, 0.05)));
    commands.insert_resource(SeedMesh(mesh));

    // One shared material per hue bucket (see PALETTE_SIZE).
    let palette = (0..PALETTE_SIZE)
        .map(|b| {
            let hue = (b as f32 + 0.5) / PALETTE_SIZE as f32 * 360.0;
            materials.add(StandardMaterial {
                base_color: Color::hsl(hue, 0.85, 0.55),
                perceptual_roughness: 1.0,
                metallic: 0.0,
                unlit: true,
                ..default()
            })
        })
        .collect();
    commands.insert_resource(SeedPalette(palette));
    commands.insert_resource(SeedEntities {
        lists: vec![Vec::new(), Vec::new()],
    });
}

#[inline]
fn palette_index(i: usize, n: usize) -> usize {
    ((i * PALETTE_SIZE) / n.max(1)).min(PALETTE_SIZE - 1)
}

//

/// Random walk for one layer: each parameter eases toward its own target;
/// when it arrives, a new target and a new timescale are drawn.
fn drift_layer(layer: &mut LayerParams, dt: f32, rng: &mut Rng) {
    let mut targets = layer.drift_targets.unwrap_or_else(|| {
        let mut t = DriftTargets {
            target: [0.0; 9],
            tau: [0.0; 9],
        };
        for (k, (lo, hi)) in DRIFT_RANGES.iter().enumerate() {
            t.target[k] = rng.range(*lo, *hi);
            t.tau[k] = rng.range(15.0, 45.0);
        }
        t
    });

    let mut changed = false;
    for (k, value) in layer.drift_values().into_iter().enumerate() {
        let (lo, hi) = DRIFT_RANGES[k];
        let gap = targets.target[k] - *value;
        // Close to target: pick the next one. Threshold is 1% of the range.
        if gap.abs() < (hi - lo) * 0.01 {
            targets.target[k] = rng.range(lo, hi);
            targets.tau[k] = rng.range(15.0, 45.0);
            continue;
        }
        let step = gap * (1.0 - (-dt / targets.tau[k]).exp());
        if step != 0.0 {
            *value = (*value + step).clamp(lo, hi);
            changed = true;
        }
    }
    layer.drift_targets = Some(targets);
    if changed {
        layer.dirty = true;
    }
}

fn animate_update(time: Res<Time>, mut params: ResMut<PhylloParams>, mut rng: ResMut<Rng>) {
    let dt = time.delta_seconds();
    for layer in params.layers.iter_mut() {
        if layer.drift {
            drift_layer(layer, dt, &mut rng);
        } else {
            layer.drift_targets = None;
        }

        // Breathe mode (dish exponent). Phase is integrated so a change in
        // breathe speed (slider or drift) bends the curve instead of jumping it.
        if layer.breathe_mode {
            layer.breathe_phase = (layer.breathe_phase + layer.breathe_speed * dt * TAU) % TAU;
            let sine = layer.breathe_phase.sin();
            let eff = layer.dish_exp + layer.breathe_amp * sine;
            // Keep exponent non-negative and trigger rebuild when it changes
            let eff = eff.max(0.0);
            if (eff - layer.effective_dish_exp).abs() > f32::EPSILON {
                layer.effective_dish_exp = eff;
                layer.dirty = true;
            }
        } else if (layer.effective_dish_exp - layer.dish_exp).abs() > f32::EPSILON {
            // Ensure effective follows base when breathe is off
            layer.effective_dish_exp = layer.dish_exp;
            layer.dirty = true;
        }

        // Spin mode (divergence in deg)
        if layer.spin_mode {
            let delta = time.delta_seconds() * layer.spin_rate_deg_per_sec;
            // Wrap into [0, 180)
            let new_val = (layer.divergence_deg + delta).rem_euclid(180.0);
            if (new_val - layer.divergence_deg).abs() > f32::EPSILON {
                layer.divergence_deg = new_val;
                layer.dirty = true;
            }
        }
    }
}

/// Spawn or despawn seed entities so each layer has exactly `n`. Newly
/// spawned seeds get their final transform immediately. When `n` changes,
/// every seed in that layer is re-bucketed so hues stay spread over 0–360°.
fn sync_seed_count(
    mut commands: Commands,
    seed_mesh: Res<SeedMesh>,
    palette: Res<SeedPalette>,
    params: Res<PhylloParams>,
    mut registry: ResMut<SeedEntities>,
    mut q_materials: Query<&mut Handle<StandardMaterial>, With<Seed>>,
) {
    // Ensure registry has a list for each layer
    if registry.lists.len() < params.layers.len() {
        registry
            .lists
            .resize_with(params.layers.len(), Vec::new);
    } else if registry.lists.len() > params.layers.len() {
        // Despawn any entities from extra lists and shrink
        for extra in registry.lists.drain(params.layers.len()..) {
            for e in extra {
                commands.entity(e).despawn();
            }
        }
    }

    for (li, layer) in params.layers.iter().enumerate() {
        let list = &mut registry.lists[li];
        let n = layer.n;
        let current = list.len();
        if current == n {
            continue;
        }

        let geom = LayerGeom::from_params(layer);

        if current < n {
            for i in current..n {
                let e = commands
                    .spawn((
                        PbrBundle {
                            mesh: seed_mesh.0.clone(),
                            material: palette.0[palette_index(i, n)].clone(),
                            transform: geom.transform(i as u32),
                            ..default()
                        },
                        Seed {
                            layer: li as u32,
                            index: i as u32,
                        },
                    ))
                    .id();
                list.push(e);
            }
        } else {
            for e in list.drain(n..) {
                commands.entity(e).despawn();
            }
        }

        // Re-bucket the seeds that already existed.
        for (i, e) in list.iter().enumerate().take(current.min(n)) {
            if let Ok(mut mat) = q_materials.get_mut(*e) {
                let want = &palette.0[palette_index(i, n)];
                if *mat != *want {
                    *mat = want.clone();
                }
            }
        }
    }
}

/// Recompute the transform of every seed in a dirty layer, in parallel.
fn place_seeds(mut params: ResMut<PhylloParams>, mut q: Query<(&Seed, &mut Transform)>) {
    if !params.layers.iter().any(|l| l.dirty) {
        return;
    }
    let geoms: Vec<LayerGeom> = params.layers.iter().map(LayerGeom::from_params).collect();

    q.par_iter_mut().for_each(|(seed, mut t)| {
        let Some(g) = geoms.get(seed.layer as usize) else {
            return;
        };
        if !g.dirty {
            return;
        }
        *t = g.transform(seed.index);
    });

    for layer in params.layers.iter_mut() {
        layer.dirty = false;
    }
}

/// Non-macOS wallpaper mode: just reveal the window once it exists.
#[allow(dead_code)]
fn show_window_once(mut q: Query<&mut Window, With<PrimaryWindow>>, mut done: Local<bool>) {
    if *done {
        return;
    }
    if let Ok(mut w) = q.get_single_mut() {
        w.visible = true;
        *done = true;
    }
}

#[cfg(target_os = "macos")]
mod macos_wallpaper {
    use super::*;
    use bevy::window::RawHandleWrapper;
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSScreen, NSView, NSWindowCollectionBehavior,
    };
    use objc2_foundation::MainThreadMarker;
    use raw_window_handle::RawWindowHandle;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGWindowLevelForKey(key: i32) -> i32;
    }
    /// `kCGDesktopIconWindowLevelKey` from CGWindowLevel.h.
    const K_CG_DESKTOP_ICON_WINDOW_LEVEL_KEY: i32 = 18;

    /// Runs once the native window exists: drop it to just below the desktop
    /// icons (above the system wallpaper), stretch it over the main screen,
    /// show it on every Space, let clicks pass through, and keep the app out
    /// of the Dock and Cmd-Tab. Then make it visible.
    ///
    /// The `NonSend<WinitWindows>` parameter is what forces Bevy to run this
    /// system on the main thread; AppKit traps if it is called from anywhere
    /// else.
    pub fn pin_to_desktop(
        mut done: Local<bool>,
        _main_thread: NonSend<bevy::winit::WinitWindows>,
        mut q: Query<(&RawHandleWrapper, &mut Window), With<PrimaryWindow>>,
    ) {
        if *done {
            return;
        }
        let Ok((handle, mut window)) = q.get_single_mut() else {
            return;
        };
        let RawWindowHandle::AppKit(appkit) = handle.window_handle else {
            return;
        };

        // SAFETY: the NonSend parameter above pins this system to the main
        // thread, and `ns_view` is a live NSView owned by winit for as long
        // as the window entity (and its RawHandleWrapper) exists.
        unsafe {
            let mtm = MainThreadMarker::new_unchecked();
            let view: &NSView = &*appkit.ns_view.as_ptr().cast::<NSView>();
            let Some(ns_window) = view.window() else {
                return;
            };

            let level = CGWindowLevelForKey(K_CG_DESKTOP_ICON_WINDOW_LEVEL_KEY) - 1;
            ns_window.setLevel(level as isize);
            ns_window.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::IgnoresCycle
                    | NSWindowCollectionBehavior::FullScreenNone,
            );
            ns_window.setIgnoresMouseEvents(true);
            ns_window.setHasShadow(false);
            if let Some(screen) = NSScreen::mainScreen(mtm) {
                ns_window.setFrame_display(screen.frame(), true);
            }
            ns_window.orderFront(None);

            NSApplication::sharedApplication(mtm)
                .setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        }

        window.visible = true;
        *done = true;
    }
}

fn ui_overlay(mut egui_ctx: EguiContexts, mut params: ResMut<PhylloParams>) {
    let ctx = egui_ctx.ctx_mut();
    egui::Window::new("Controls").show(ctx, |ui| {
        ui.label("Flower Fractal Toy");

        for (idx, layer) in params.layers.iter_mut().enumerate() {
            ui.separator();
            egui::CollapsingHeader::new(format!("Layer {}", idx + 1))
                .default_open(true)
                .show(ui, |ui| {
                    ui.checkbox(&mut layer.drift, "Drift (parameters wander randomly, forever)");

                    let mut n = layer.n as u32;
                    if ui
                        .add(egui::Slider::new(&mut n, 1..=50_000).text("n seeds"))
                        .changed()
                    {
                        layer.n = n as usize;
                        layer.dirty = true;
                    }

                    // High-precision divergence control: slider with fine step + numeric input
                    let mut div_changed = false;
                    ui.horizontal(|ui| {
                        div_changed |= ui
                            .add(
                                egui::Slider::new(&mut layer.divergence_deg, 0.0..=180.0)
                                    .step_by(0.0001)
                                    .text("divergence (deg)"),
                            )
                            .changed();
                        div_changed |= ui
                            .add(
                                egui::DragValue::new(&mut layer.divergence_deg)
                                    .range(0.0..=180.0)
                                    .speed(0.01)
                                    .max_decimals(6),
                            )
                            .changed();
                    });
                    if div_changed {
                        layer.dirty = true;
                    }

                    if ui
                        .add(egui::Slider::new(&mut layer.scale_k, 0.01..=0.5).text("scale k"))
                        .changed()
                    {
                        layer.dirty = true;
                    }
                    if ui
                        .add(egui::Slider::new(&mut layer.dish_amp, 0.0..=5.0).text("dish amp"))
                        .changed()
                    {
                        layer.dirty = true;
                    }
                    // Base dish exponent; effective value is modulated in breathe mode
                    if ui
                        .add(egui::Slider::new(&mut layer.dish_exp, 0.0..=5.0).text("dish exp (base)"))
                        .changed()
                    {
                        layer.dirty = true;
                    }
                    if ui
                        .add(egui::Slider::new(&mut layer.cap_amp, 0.0..=5.0).text("cap amp"))
                        .changed()
                    {
                        layer.dirty = true;
                    }
                    if ui
                        .add(egui::Slider::new(&mut layer.tangent_tilt, 0.0..=1.0).text("tangent tilt"))
                        .changed()
                    {
                        layer.dirty = true;
                    }

                    if ui
                        .add(
                            egui::Slider::new(&mut layer.seed_size, 0.1..=5.0)
                                .step_by(0.01)
                                .text("seed size"),
                        )
                        .changed()
                    {
                        layer.dirty = true;
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        let mut changed = false;
                        changed |= ui.checkbox(&mut layer.breathe_mode, "Breathe mode").changed();
                        changed |= ui
                            .add(egui::Slider::new(&mut layer.breathe_amp, 0.0..=2.0).text("breathe amp"))
                            .changed();
                        changed |= ui
                            .add(egui::Slider::new(&mut layer.breathe_speed, 0.0..=2.0).text("breathe speed (Hz)"))
                            .changed();
                        if changed {
                            layer.dirty = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        let mut changed = false;
                        changed |= ui.checkbox(&mut layer.spin_mode, "Spin mode").changed();
                        changed |= ui
                            .add(
                                egui::Slider::new(&mut layer.spin_rate_deg_per_sec, -2.0..=2.0)
                                    .step_by(0.001)
                                    .text("spin rate (deg/s)"),
                            )
                            .changed();
                        // Precise numeric control to ensure ~0.01 per tick adjustments
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut layer.spin_rate_deg_per_sec)
                                    .range(-2.0..=2.0)
                                    .speed(0.01)
                                    .max_decimals(3),
                            )
                            .changed();
                        if changed {
                            layer.dirty = true;
                        }
                    });
                });
        }
    });
}
