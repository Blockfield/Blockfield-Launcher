use skin3d::{Cape, Model, RenderSize, Renderer, Result, Skin};
use std::{path::PathBuf, time::Instant};

fn main() -> Result<()> {
    let mut output = PathBuf::from("frames");
    let mut count = 121usize;
    let mut software = false;
    let mut fixture = String::from("classic");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" => output = args.next().ok_or("--output needs a directory")?.into(),
            "--frames" => count = args.next().ok_or("--frames needs a count")?.parse()?,
            "--fixture" => {
                fixture = args
                    .next()
                    .ok_or("--fixture needs classic, slim or legacy")?
            }
            "--software" => software = true,
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    if !(1..=121).contains(&count) {
        return Err("--frames must be 1..=121 (t=0..2 at 60 fps)".into());
    }
    let (png, model): (&[u8], _) = match fixture.as_str() {
        "classic" => (include_bytes!("../fixtures/uv-skin.png"), Model::Auto),
        "slim" => (include_bytes!("../fixtures/uv-slim.png"), Model::Auto),
        "legacy" => (include_bytes!("../fixtures/uv-legacy.png"), Model::Classic),
        _ => return Err("--fixture must be classic, slim or legacy".into()),
    };
    let skin = Skin::from_png(png, model)?;
    let cape = Cape::from_png(include_bytes!("../fixtures/uv-cape.png"))?;
    for scale_factor in [1.0, 1.25, 1.5] {
        let size = RenderSize {
            scale_factor,
            ..Default::default()
        };
        let directory = output.join(format!("{fixture}-{scale_factor:.2}x"));
        std::fs::create_dir_all(&directory)?;
        let mut renderer = pollster::block_on(Renderer::new(size, &skin, Some(&cape), software))?;
        println!("adapter={:?}", renderer.adapter_info());
        let first = renderer.render_frame(0.0, 0.0)?;
        let mut times = Vec::with_capacity(count);
        for index in 0..count {
            let start = Instant::now();
            let frame = renderer.render_frame(index as f64 / 60.0, 0.0)?;
            times.push(start.elapsed().as_secs_f64() * 1000.0);
            image::save_buffer(
                directory.join(format!("frame-{index:03}.png")),
                &frame.rgba,
                frame.width,
                frame.height,
                image::ColorType::Rgba8,
            )?;
        }
        assert_eq!(
            first,
            renderer.render_frame(0.0, 0.0)?,
            "frame depends on call history"
        );
        times.sort_by(f64::total_cmp);
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        let report = format!(
            "fixture={fixture}\nscale={scale_factor}\nsize={}x{}\nframes={count}\nfps=60\nfirst_time_seconds=0\nlast_time_seconds={}\nadapter={:?}\nrender_readback_mean_ms={mean:.3}\nrender_readback_median_ms={:.3}\nrender_readback_p95_ms={:.3}\nrender_readback_max_ms={:.3}\nrepeat_frame_identical=true\nPNG encoding and file IO excluded; one warm-up frame excluded.\n",
            first.width,
            first.height,
            (count - 1) as f64 / 60.0,
            renderer.adapter_info(),
            times[times.len() / 2],
            times[((times.len() - 1) as f64 * 0.95).ceil() as usize],
            times[times.len() - 1]
        );
        println!("{report}");
        std::fs::write(directory.join("timings.txt"), report)?;
    }
    Ok(())
}
