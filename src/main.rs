use bevy::prelude::*;
use bevy_egui::EguiPlugin;

#[derive(Clone)]
struct InstanceData {
    transform: Mat4,
    color: Color,
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Camera
    commands.spawn(Camera3dBundle::default());

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
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.8, 0.7, 0.5),
        ..default()
    });

    let instances = build_instances(10_000);
    for inst in &instances {
        commands.spawn(PbrBundle {
            mesh: mesh.clone(),
            material: material.clone(),
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
            InstanceData {
                transform,
                color: Color::srgb(0.8, 0.7, 0.5),
            }
        })
        .collect()
}
