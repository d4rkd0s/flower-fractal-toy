use bevy::math::EulerRot;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, egui};

#[derive(Clone)]
struct InstanceData {
    transform: Mat4,
    color: Color,
}

#[derive(Resource)]
struct PhylloParams {
    n: usize,
    divergence_deg: f32,
    scale_k: f32,
    dish_amp: f32,
    dish_exp: f32,
    cap_amp: f32,
    tangent_tilt: f32,
    dirty: bool,
}

#[derive(Resource, Clone)]
struct SeedMesh(Handle<Mesh>);

#[derive(Component)]
struct Seed;

fn main() {
    App::new()
        // Darker background for contrast
        .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.03)))
        .insert_resource(PhylloParams {
            n: 10_000,
            divergence_deg: 137.5,
            scale_k: 0.1,
            dish_amp: 2.0,
            dish_exp: 2.0,
            cap_amp: 0.0,
            tangent_tilt: 1.0,
            dirty: true,
        })
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, (ui_overlay, rebuild_if_dirty))
        .run();
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    // Camera
    commands.spawn(Camera3dBundle {
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
}

fn build_instances(params: &PhylloParams) -> Vec<InstanceData> {
    let divergence = params.divergence_deg.to_radians();
    let n = params.n;
    let k = params.scale_k;
    let r_max = k * (n as f32).sqrt();

    (0..n)
        .map(|i| {
            let i_f = i as f32;
            let theta = i_f * divergence;
            let r = k * i_f.sqrt();
            let x = r * theta.cos();
            let y = r * theta.sin();

            let r_ratio = r / r_max;
            let mut z = params.dish_amp * r_ratio.powf(params.dish_exp);
            if params.cap_amp != 0.0 {
                let cap_term = (1.0 - r_ratio * r_ratio).max(0.0).sqrt();
                z += params.cap_amp * cap_term;
            }

            // Forward difference for tangent orientation
            let i_next = i_f + 1.0;
            let theta_next = i_next * divergence;
            let r_next = k * i_next.sqrt();
            let x_next = r_next * theta_next.cos();
            let y_next = r_next * theta_next.sin();
            let r_next_ratio = r_next / r_max;
            let mut z_next = params.dish_amp * r_next_ratio.powf(params.dish_exp);
            if params.cap_amp != 0.0 {
                let cap_term = (1.0 - r_next_ratio * r_next_ratio).max(0.0).sqrt();
                z_next += params.cap_amp * cap_term;
            }

            let dx = x_next - x;
            let dy = y_next - y;
            let dz = z_next - z;
            let yaw = dy.atan2(dx);
            let horiz = (dx * dx + dy * dy).sqrt();
            let pitch = dz.atan2(horiz) * params.tangent_tilt;
            let rot = Quat::from_euler(EulerRot::ZYX, yaw, -pitch, 0.0);

            let transform = Mat4::from_rotation_translation(rot, Vec3::new(x, y, z));

            // Rainbow hue across the sequence
            let hue = (i_f / n as f32) * 360.0;
            let color = Color::hsl(hue, 0.85, 0.55);
            InstanceData { transform, color }
        })
        .collect()
}

fn rebuild_if_dirty(
    mut commands: Commands,
    seed_mesh: Res<SeedMesh>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut params: ResMut<PhylloParams>,
    seeds: Query<Entity, With<Seed>>,
) {
    if !params.dirty {
        return;
    }

    for e in &seeds {
        commands.entity(e).despawn();
    }

    let instances = build_instances(&params);
    for inst in instances {
        let mat = materials.add(StandardMaterial {
            base_color: inst.color,
            // Make colors pop regardless of lighting
            perceptual_roughness: 1.0,
            metallic: 0.0,
            unlit: true,
            ..default()
        });
        commands.spawn((
            PbrBundle {
                mesh: seed_mesh.0.clone(),
                material: mat,
                transform: Transform::from_matrix(inst.transform),
                ..default()
            },
            Seed,
        ));
    }

    params.dirty = false;
}

fn ui_overlay(mut egui_ctx: EguiContexts, mut params: ResMut<PhylloParams>) {
    let ctx = egui_ctx.ctx_mut();
    egui::Window::new("Controls").show(ctx, |ui| {
        ui.label("Sunflower Fractal Toy");

        let mut n = params.n as u32;
        if ui
            .add(egui::Slider::new(&mut n, 1..=50_000).text("n seeds"))
            .changed()
        {
            params.n = n as usize;
            params.dirty = true;
        }
        if ui
            .add(
                egui::Slider::new(&mut params.divergence_deg, 0.0..=180.0).text("divergence (deg)"),
            )
            .changed()
        {
            params.dirty = true;
        }
        if ui
            .add(egui::Slider::new(&mut params.scale_k, 0.01..=0.5).text("scale k"))
            .changed()
        {
            params.dirty = true;
        }
        if ui
            .add(egui::Slider::new(&mut params.dish_amp, 0.0..=5.0).text("dish amp"))
            .changed()
        {
            params.dirty = true;
        }
        if ui
            .add(egui::Slider::new(&mut params.dish_exp, 0.5..=5.0).text("dish exp"))
            .changed()
        {
            params.dirty = true;
        }
        if ui
            .add(egui::Slider::new(&mut params.cap_amp, 0.0..=5.0).text("cap amp"))
            .changed()
        {
            params.dirty = true;
        }
        if ui
            .add(egui::Slider::new(&mut params.tangent_tilt, 0.0..=1.0).text("tangent tilt"))
            .changed()
        {
            params.dirty = true;
        }
    });
}
