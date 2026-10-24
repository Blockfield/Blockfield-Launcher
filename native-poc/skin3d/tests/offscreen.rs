use skin3d::{Cape, Model, RenderSize, Renderer, Skin};

#[test]
#[ignore = "requires a headless wgpu software adapter; see README.md"]
fn software_frames_are_deterministic_animated_and_transparent() {
    let skin = Skin::from_png(include_bytes!("../fixtures/uv-skin.png"), Model::Auto).unwrap();
    let cape = Cape::from_png(include_bytes!("../fixtures/uv-cape.png")).unwrap();
    let mut renderer = pollster::block_on(Renderer::new(
        RenderSize::default(),
        &skin,
        Some(&cape),
        true,
    ))
    .unwrap();
    let first = renderer.render_frame(0.0, 0.0).unwrap();
    assert_eq!(first.rgba.len(), 220 * 300 * 4);
    assert_eq!(&first.rgba[..4], &[0; 4]);
    let opaque = first
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] == 255)
        .count();
    assert!(
        (8000..50000).contains(&opaque),
        "unexpected silhouette: {opaque} opaque pixels"
    );
    assert!(
        first
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[3] > 0 && p[3] < 255),
        "FXAA/outer transparency missing"
    );
    assert_ne!(first, renderer.render_frame(1.0, 0.0).unwrap());
    assert_ne!(first, renderer.render_frame(0.0, 0.5).unwrap());
    assert_eq!(first, renderer.render_frame(0.0, 0.0).unwrap());
    renderer.controls.set_dragging(0.0, true).unwrap();
    renderer.controls.drag_by(20.0, 10.0, 300.0).unwrap();
    assert_ne!(first, renderer.render_frame(0.0, 0.0).unwrap());
    assert!(renderer.render_frame(f64::NAN, 0.0).is_err());
    assert!(renderer.render_frame(1.0, f32::INFINITY).is_err());

    let with_cape = renderer.render_frame(4.0, 0.0).unwrap();
    let mut bare =
        pollster::block_on(Renderer::new(RenderSize::default(), &skin, None, true)).unwrap();
    bare.controls = renderer.controls.clone();
    assert_ne!(with_cape, bare.render_frame(4.0, 0.0).unwrap());
    let slim = Skin::from_png(include_bytes!("../fixtures/uv-slim.png"), Model::Auto).unwrap();
    let mut slim_renderer = pollster::block_on(Renderer::new(
        RenderSize::default(),
        &slim,
        Some(&cape),
        true,
    ))
    .unwrap();
    assert_ne!(first, slim_renderer.render_frame(0.0, 0.0).unwrap());
    let legacy =
        Skin::from_png(include_bytes!("../fixtures/uv-legacy.png"), Model::Classic).unwrap();
    let mut legacy_renderer = pollster::block_on(Renderer::new(
        RenderSize {
            scale_factor: 1.25,
            ..Default::default()
        },
        &legacy,
        None,
        true,
    ))
    .unwrap();
    let frame = legacy_renderer.render_frame(0.0, 0.0).unwrap();
    assert_eq!((frame.width, frame.height), (275, 375));
    assert_eq!(frame.rgba.len(), 275 * 375 * 4);
}
