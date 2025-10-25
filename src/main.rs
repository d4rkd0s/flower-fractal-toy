use bevy::math::EulerRot;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, egui};
use std::f32::consts::TAU;

//

#[derive(Clone)]
struct LayerParams {
    n: usize,
    divergence_deg: f32,
    scale_k: f32,
    dish_amp: f32,
    dish_exp: f32,              // base dish exponent (UI-controlled)
    effective_dish_exp: f32,    // modulated exponent used in build
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
    dirty: bool,
}

#[derive(Resource)]
struct PhylloParams {
    layers: Vec<LayerParams>,
}

#[derive(Resource, Clone)]
struct SeedMesh(Handle<Mesh>);

#[derive(Component)]
struct Seed;

// Keep a stable list of seed entities to avoid per-frame spawn/despawn churn
#[derive(Resource, Default)]
struct SeedEntities {
    lists: Vec<Vec<Entity>>,
}

fn main() {
    App::new()
        // Darker background for contrast
        .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.03)))
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
                    dirty: true,
                },
            ],
        })
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, (animate_update, ui_overlay, rebuild_if_dirty))
        .run();
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    // Camera
    // Use no tonemapping and disable HDR to avoid LUT assets,
    // which reduces dependency size while keeping colors accurate
    commands.spawn(Camera3dBundle {
        camera: Camera {
            hdr: false,
            ..default()
        },
        transform: Transform::from_xyz(0.0, 0.0, 25.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });

    // Light
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 1500.0,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..default()
    });

    let mesh = meshes.add(Mesh::from(Cuboid::new(0.05, 0.05, 0.05)));
    commands.insert_resource(SeedMesh(mesh));
    commands.insert_resource(SeedEntities { lists: vec![Vec::new(), Vec::new()] });
}

//

fn animate_update(time: Res<Time>, mut params: ResMut<PhylloParams>) {
    for layer in params.layers.iter_mut() {
        // Breathe mode (dish exponent)
        if layer.breathe_mode {
            let t = time.elapsed_seconds();
            let sine = (layer.breathe_speed * t * TAU).sin();
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

fn rebuild_if_dirty(
    mut commands: Commands,
    seed_mesh: Res<SeedMesh>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut params: ResMut<PhylloParams>,
    mut registry: ResMut<SeedEntities>,
    mut q_transforms: Query<&mut Transform>,
) {
    // Ensure registry has a list for each layer
    if registry.lists.len() < params.layers.len() {
        registry
            .lists
            .resize_with(params.layers.len(), || Vec::new());
    } else if registry.lists.len() > params.layers.len() {
        // Despawn any entities from extra lists and shrink
        for extra in registry.lists.drain(params.layers.len()..) {
            for e in extra {
                commands.entity(e).despawn();
            }
        }
    }

    for (li, layer) in params.layers.iter_mut().enumerate() {
        if !layer.dirty {
            continue;
        }

        // Adjust entity count for this layer
        let list = &mut registry.lists[li];
        let n = layer.n;
        let current = list.len();
        if current < n {
            for i in current..n {
                let hue = (i as f32 / n as f32) * 360.0;
                let color = Color::hsl(hue, 0.85, 0.55);
                let mat = materials.add(StandardMaterial {
                    base_color: color,
                    perceptual_roughness: 1.0,
                    metallic: 0.0,
                    unlit: true,
                    ..default()
                });
                let e = commands
                    .spawn((
                        PbrBundle {
                            mesh: seed_mesh.0.clone(),
                            material: mat,
                            transform: Transform::IDENTITY,
                            ..default()
                        },
                        Seed,
                    ))
                    .id();
                list.push(e);
            }
        } else if current > n {
            for e in list.drain(n..) {
                commands.entity(e).despawn();
            }
        }

        // Recompute transforms for all seeds in this layer
        let divergence = layer.divergence_deg.to_radians();
        let k = layer.scale_k;
        let r_max = k * (n as f32).sqrt();

        for (i, e) in list.iter().enumerate() {
            let i_f = i as f32;
            let theta = i_f * divergence;
            let r = k * i_f.sqrt();
            let x = r * theta.cos();
            let y = r * theta.sin();

            let r_ratio = if r_max > 0.0 { r / r_max } else { 0.0 };
            let mut z = layer.dish_amp * r_ratio.powf(layer.effective_dish_exp);
            if layer.cap_amp != 0.0 {
                let cap_term = (1.0 - r_ratio * r_ratio).max(0.0).sqrt();
                z += layer.cap_amp * cap_term;
            }

            // Forward difference for tangent orientation
            let i_next = i_f + 1.0;
            let theta_next = i_next * divergence;
            let r_next = k * i_next.sqrt();
            let x_next = r_next * theta_next.cos();
            let y_next = r_next * theta_next.sin();
            let r_next_ratio = if r_max > 0.0 { r_next / r_max } else { 0.0 };
            let mut z_next = layer.dish_amp * r_next_ratio.powf(layer.effective_dish_exp);
            if layer.cap_amp != 0.0 {
                let cap_term = (1.0 - r_next_ratio * r_next_ratio).max(0.0).sqrt();
                z_next += layer.cap_amp * cap_term;
            }

            let dx = x_next - x;
            let dy = y_next - y;
            let dz = z_next - z;
            let yaw = dy.atan2(dx);
            let horiz = (dx * dx + dy * dy).sqrt();
            let pitch = dz.atan2(horiz) * layer.tangent_tilt;
            let rot = Quat::from_euler(EulerRot::ZYX, yaw, -pitch, 0.0);

            if let Ok(mut t) = q_transforms.get_mut(*e) {
                t.translation = Vec3::new(x, y, z);
                t.rotation = rot;
                t.scale = Vec3::splat(layer.seed_size);
            }
        }

        layer.dirty = false;
    }
}

fn ui_overlay(mut egui_ctx: EguiContexts, mut params: ResMut<PhylloParams>) {
    let ctx = egui_ctx.ctx_mut();
    egui::Window::new("Controls").show(ctx, |ui| {
        ui.label("Sunflower Fractal Toy");

        for (idx, layer) in params.layers.iter_mut().enumerate() {
            ui.separator();
            egui::CollapsingHeader::new(format!("Layer {}", idx + 1))
                .default_open(true)
                .show(ui, |ui| {
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
