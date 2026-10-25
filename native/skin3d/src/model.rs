use crate::{Model, WalkingPose};
use bytemuck::{Pod, Zeroable};
use glam::{EulerRot, Mat4, Quat, Vec3};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

pub(crate) struct Part {
    pub vertices: Vec<Vertex>,
    pub center: Vec3,
    pub outer: bool,
    pub biased: bool,
    pub cape: bool,
}

/// Three BoxGeometry vertex order: +X, -X, +Y, -Y, +Z, -Z; TL, TR, BL, BR.
fn box_vertices(size: Vec3, uv_origin: [f32; 2], uv_size: Vec3, texture: [f32; 2]) -> Vec<Vertex> {
    let [u, v] = uv_origin;
    let Vec3 { x: w, y: h, z: d } = uv_size;
    let rects = [
        [u + w + d, v + d, d, h],
        [u, v + d, d, h],
        [u + d, v, w, d],
        [u + w + d, v, w, d],
        [u + d, v + d, w, h],
        [u + w + 2.0 * d, v + d, w, h],
    ];
    let planes = [
        (Vec3::X, -Vec3::Z, Vec3::Y, size.z, size.y, size.x),
        (-Vec3::X, Vec3::Z, Vec3::Y, size.z, size.y, size.x),
        (Vec3::Y, Vec3::X, -Vec3::Z, size.x, size.z, size.y),
        (-Vec3::Y, Vec3::X, Vec3::Z, size.x, size.z, size.y),
        (Vec3::Z, Vec3::X, Vec3::Y, size.x, size.y, size.z),
        (-Vec3::Z, -Vec3::X, Vec3::Y, size.x, size.y, size.z),
    ];
    let mut vertices = Vec::with_capacity(36);
    for (face, ((normal, right, up, width, height, depth), [x, y, rw, rh])) in
        planes.into_iter().zip(rects).enumerate()
    {
        let corners = [(-0.5, 0.5), (0.5, 0.5), (-0.5, -0.5), (0.5, -0.5)];
        for index in [0, 2, 1, 2, 3, 1] {
            let (cx, cy) = corners[index];
            let position = normal * (depth / 2.0) + right * (cx * width) + up * (cy * height);
            let py = if face == 3 { cy + 0.5 } else { 0.5 - cy };
            vertices.push(Vertex {
                position: position.to_array(),
                normal: normal.to_array(),
                // wgpu PNG uploads use top-left UVs; upstream's CanvasTexture flips Y.
                uv: [
                    (x + (cx + 0.5) * rw) / texture[0],
                    (y + py * rh) / texture[1],
                ],
            });
        }
    }
    vertices
}

fn joint(position: Vec3, rotation: Vec3) -> Mat4 {
    Mat4::from_rotation_translation(
        Quat::from_euler(EulerRot::XYZ, rotation.x, rotation.y, rotation.z),
        position,
    )
}

pub(crate) fn player_parts(model: Model, cape: bool, pose: WalkingPose, yaw: f32) -> Vec<Part> {
    let skin = Mat4::from_rotation_y(yaw) * Mat4::from_translation(Vec3::Y * 8.0);
    let arm_width = if model == Model::Slim { 3.0 } else { 4.0 };
    let arm_offset = if model == Model::Slim { 0.5 } else { 1.0 };
    let definitions = [
        (
            Vec3::splat(8.0),
            [0.0, 0.0],
            [32.0, 0.0],
            joint(Vec3::ZERO, pose.head),
            Vec3::Y * 4.0,
            1.0,
            false,
        ),
        (
            Vec3::new(8.0, 12.0, 4.0),
            [16.0, 16.0],
            [16.0, 32.0],
            joint(Vec3::Y * -6.0, Vec3::ZERO),
            Vec3::ZERO,
            0.5,
            false,
        ),
        (
            Vec3::new(arm_width, 12.0, 4.0),
            [40.0, 16.0],
            [40.0, 32.0],
            joint(Vec3::new(-5.0, -2.0, 0.0), pose.right_arm),
            Vec3::new(-arm_offset, -4.0, 0.0),
            0.5,
            true,
        ),
        (
            Vec3::new(arm_width, 12.0, 4.0),
            [32.0, 48.0],
            [48.0, 48.0],
            joint(Vec3::new(5.0, -2.0, 0.0), pose.left_arm),
            Vec3::new(arm_offset, -4.0, 0.0),
            0.5,
            true,
        ),
        (
            Vec3::new(4.0, 12.0, 4.0),
            [0.0, 16.0],
            [0.0, 32.0],
            joint(Vec3::new(-1.9, -12.0, -0.1), Vec3::X * pose.right_leg_x),
            Vec3::Y * -6.0,
            0.5,
            true,
        ),
        (
            Vec3::new(4.0, 12.0, 4.0),
            [16.0, 48.0],
            [0.0, 48.0],
            joint(Vec3::new(1.9, -12.0, -0.1), Vec3::X * pose.left_leg_x),
            Vec3::Y * -6.0,
            0.5,
            true,
        ),
    ];
    let mut parts = Vec::with_capacity(13);
    for (size, inner_uv, outer_uv, pivot, offset, inflation, biased) in definitions {
        let transform = skin * pivot * Mat4::from_translation(offset);
        for outer in [false, true] {
            let actual_size = size + Vec3::splat(if outer { inflation } else { 0.0 });
            parts.push(make_part(
                actual_size,
                if outer { outer_uv } else { inner_uv },
                size,
                [64.0, 64.0],
                transform,
                outer,
                biased,
            ));
        }
    }
    if cape {
        let transform = Mat4::from_rotation_y(yaw)
            * joint(
                Vec3::new(0.0, 8.0, -2.0),
                Vec3::new(pose.cape_x, std::f32::consts::PI, 0.0),
            )
            * Mat4::from_translation(Vec3::new(0.0, -8.0, 0.5));
        let mut part = make_part(
            Vec3::new(10.0, 16.0, 1.0),
            [0.0, 0.0],
            Vec3::new(10.0, 16.0, 1.0),
            [64.0, 32.0],
            transform,
            true,
            false,
        );
        part.cape = true;
        parts.push(part);
    }
    parts
}

fn make_part(
    size: Vec3,
    uv: [f32; 2],
    uv_size: Vec3,
    texture: [f32; 2],
    transform: Mat4,
    outer: bool,
    biased: bool,
) -> Part {
    let mut vertices = box_vertices(size, uv, uv_size, texture);
    for vertex in &mut vertices {
        vertex.position = transform
            .transform_point3(Vec3::from_array(vertex.position))
            .to_array();
        vertex.normal = transform
            .transform_vector3(Vec3::from_array(vertex.normal))
            .normalize()
            .to_array();
    }
    Part {
        vertices,
        center: transform.transform_point3(Vec3::ZERO),
        outer,
        biased,
        cape: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Skin, walking_pose};

    #[test]
    fn transformed_vertices_match_upstream_player_object() {
        for line in include_str!("../fixtures/upstream-vertices.csv")
            .lines()
            .skip(1)
        {
            let values: Vec<f64> = line.split(',').map(|v| v.parse().unwrap()).collect();
            let model = if values[0] == 0.0 {
                Model::Classic
            } else {
                Model::Slim
            };
            let parts = player_parts(
                model,
                true,
                walking_pose(values[1]),
                crate::auto_rotate_yaw(values[1]),
            );
            let vertex = parts[values[2] as usize].vertices[values[3] as usize];
            let actual = [
                vertex.position.as_slice(),
                vertex.normal.as_slice(),
                vertex.uv.as_slice(),
            ]
            .concat();
            for (actual, expected) in actual.into_iter().zip(&values[4..]) {
                assert!(
                    (f64::from(actual) - expected).abs() < 1e-5,
                    "{actual} != {expected}: {line}"
                );
            }
        }
    }

    #[test]
    fn known_face_uv_pixel_picks_and_bottom_orientation() {
        let skin =
            Skin::from_png(include_bytes!("../fixtures/uv-skin.png"), Model::Classic).unwrap();
        let vertices = box_vertices(Vec3::splat(8.0), [0.0; 2], Vec3::splat(8.0), [64.0; 2]);
        for (face, expected) in [
            (0, [20, 12]),
            (1, [4, 12]),
            (2, [12, 4]),
            (3, [20, 4]),
            (4, [12, 12]),
            (5, [28, 12]),
        ] {
            let vs = &vertices[face * 6..face * 6 + 6];
            let u = vs.iter().map(|v| v.uv[0]).sum::<f32>() / 6.0;
            let v = vs.iter().map(|v| v.uv[1]).sum::<f32>() / 6.0;
            let xy = [(u * 64.0) as u32, (v * 64.0) as u32];
            assert_eq!(xy, expected);
            assert_eq!(
                skin.pixels.get_pixel(xy[0], xy[1]).0,
                [10 + 3 * xy[0] as u8, 5 + 3 * xy[1] as u8, 160, 255]
            );
        }
        assert_eq!(vertices[3 * 6].uv, [16.0 / 64.0, 8.0 / 64.0]);
        assert_eq!(vertices[4 * 6].uv, [8.0 / 64.0, 8.0 / 64.0]);
        for triangle in vertices.as_chunks::<3>().0 {
            let a = Vec3::from_array(triangle[0].position);
            let b = Vec3::from_array(triangle[1].position);
            let c = Vec3::from_array(triangle[2].position);
            assert!(
                (b - a)
                    .cross(c - a)
                    .dot(Vec3::from_array(triangle[0].normal))
                    > 0.0
            );
        }
    }

    #[test]
    fn classic_slim_width_inflation_and_cape_dimensions() {
        for (model, width) in [(Model::Classic, 4.0), (Model::Slim, 3.0)] {
            let parts = player_parts(model, true, walking_pose(0.0), 0.0);
            let span = |part: &Part, normal: Vec3| {
                let values: Vec<_> = part
                    .vertices
                    .iter()
                    .map(|v| Vec3::from_array(v.position).dot(normal))
                    .collect();
                values.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                    - values.iter().copied().fold(f32::INFINITY, f32::min)
            };
            let normal = Vec3::from_array(parts[4].vertices[0].normal);
            assert!((span(&parts[4], normal) - width).abs() < 1e-5);
            assert!((span(&parts[5], normal) - width - 0.5).abs() < 1e-5);
            assert!((span(&parts[0], Vec3::X) - 8.0).abs() < 1e-5);
            assert!((span(&parts[1], Vec3::X) - 9.0).abs() < 1e-5);
            let cape_normal = Vec3::from_array(parts[12].vertices[0].normal);
            assert!((span(&parts[12], cape_normal) - 10.0).abs() < 1e-5);
        }
    }
}
