use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};

#[derive(Clone)]
struct InstanceData {
    transform: Mat4,
    color: Color,
}

fn main() {
    App::new()
        // Darker background for contrast
        .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.03)))
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, ui_overlay)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
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

    let instances = build_instances(10_000);
    for inst in &instances {
        // Per-instance material to support per-seed color.
        let mat = materials.add(StandardMaterial {
            base_color: inst.color,
            // Make colors pop regardless of lighting
            perceptual_roughness: 1.0,
            metallic: 0.0,
            unlit: true,
            ..default()
        });
        commands.spawn(PbrBundle {
            mesh: mesh.clone(),
            material: mat,
            transform: Transform::from_matrix(inst.transform),
            ..default()
        });
    }
}

fn build_instances(n: usize) -> Vec<InstanceData> {
    let divergence = 137.5_f32.to_radians();
    let k = 0.1_f32;
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

fn ui_overlay(mut egui_ctx: EguiContexts) {
    let ctx = egui_ctx.ctx_mut();
    egui::Window::new("Controls").show(ctx, |ui| {
        ui.label("Sunflower Fractal Toy");
        ui.label("- Camera at (0, 0, 25), looking at origin");
        ui.label("- 10,000 colored seeds (rainbow)");
    });
}
