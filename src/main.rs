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

fn build_instances(n: usize, divergence_deg: f32, k: f32) -> Vec<InstanceData> {
    let divergence = divergence_deg.to_radians();
    (0..n)
        .map(|i| {
            let i_f = i as f32;
            let theta = i_f * divergence;
            let r = k * i_f.sqrt();
            let x = r * theta.cos();
            let y = r * theta.sin();
            let transform = Mat4::from_translation(Vec3::new(x, y, 0.0));
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

    let instances = build_instances(params.n, params.divergence_deg, params.scale_k);
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
    });
}
