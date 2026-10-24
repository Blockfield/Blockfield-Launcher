use crate::{Result, invalid};
use image::{ImageReader, Rgba, RgbaImage};
use std::io::Cursor;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Model {
    Auto,
    Classic,
    Slim,
}

#[derive(Clone, Debug)]
pub struct Skin {
    pub(crate) pixels: RgbaImage,
    model: Model,
}

#[derive(Clone, Debug)]
pub struct Cape {
    pub(crate) pixels: RgbaImage,
}

fn decode(png: &[u8]) -> Result<RgbaImage> {
    let mut reader = ImageReader::with_format(Cursor::new(png), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1024);
    limits.max_image_height = Some(1024);
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    let mut pixels = reader.decode()?.to_rgba8();
    // CanvasTexture's source canvas loses RGB under fully transparent pixels.
    for pixel in pixels.pixels_mut().filter(|p| p[3] == 0) {
        *pixel = Rgba([0; 4]);
    }
    Ok(pixels)
}

impl Skin {
    /// PNG bytes only; integer multiples of 64×64 or legacy 64×32, up to 1024px.
    pub fn from_png(png: &[u8], model: Model) -> Result<Self> {
        Self::from_pixels(decode(png)?, model)
    }

    fn from_pixels(source: RgbaImage, model: Model) -> Result<Self> {
        let (w, h) = source.dimensions();
        if w == 0 || w % 64 != 0 || (h != w && h * 2 != w) {
            return Err(invalid("skin must be a multiple of 64×64 or 64×32"));
        }
        let scale = w / 64;
        let legacy = w != h;
        let opaque = source.pixels().all(|p| p[3] == 255);
        let mut pixels = RgbaImage::new(w, w);
        image::imageops::replace(&mut pixels, &source, 0, 0);
        if legacy {
            for (sx, sy, dx, dy, rw, rh) in [
                (4, 16, 20, 48, 4, 4),
                (8, 16, 24, 48, 4, 4),
                (0, 20, 24, 52, 4, 12),
                (4, 20, 20, 52, 4, 12),
                (8, 20, 16, 52, 4, 12),
                (12, 20, 28, 52, 4, 12),
                (44, 16, 36, 48, 4, 4),
                (48, 16, 40, 48, 4, 4),
                (40, 20, 40, 52, 4, 12),
                (44, 20, 36, 52, 4, 12),
                (48, 20, 32, 52, 4, 12),
                (52, 20, 44, 52, 4, 12),
            ] {
                for y in 0..rh * scale {
                    for x in 0..rw * scale {
                        pixels.put_pixel(
                            dx * scale + x,
                            dy * scale + y,
                            *source.get_pixel((sx + rw) * scale - x - 1, sy * scale + y),
                        );
                    }
                }
            }
        }
        if opaque {
            // Upstream removes layer backgrounds only when the whole source is opaque.
            for (x, y, rw, rh) in [
                (40, 0, 16, 8),
                (32, 8, 32, 8),
                (4, 32, 8, 4),
                (0, 36, 16, 12),
                (20, 32, 16, 4),
                (16, 36, 24, 12),
                (44, 32, 8, 4),
                (40, 36, 24, 12),
                (4, 48, 8, 4),
                (0, 52, 16, 12),
                (52, 48, 8, 4),
                (48, 52, 16, 12),
            ] {
                if legacy && y >= 32 {
                    continue;
                }
                for py in y * scale..(y + rh) * scale {
                    for px in x * scale..(x + rw) * scale {
                        pixels.put_pixel(px, py, Rgba([0; 4]));
                    }
                }
            }
        }
        let model = match model {
            Model::Auto => infer_model(&pixels),
            explicit => explicit,
        };
        Ok(Self { pixels, model })
    }

    pub fn model(&self) -> Model {
        self.model
    }
}

fn infer_model(pixels: &RgbaImage) -> Model {
    let s = pixels.width() / 64;
    let mut transparent = false;
    let mut black = true;
    let mut white = true;
    for (x, y, w, h) in [
        (50, 16, 2, 4),
        (54, 20, 2, 12),
        (42, 48, 2, 4),
        (46, 52, 2, 12),
    ] {
        for py in y * s..(y + h) * s {
            for px in x * s..(x + w) * s {
                let p = pixels.get_pixel(px, py).0;
                transparent |= p[3] != 255;
                black &= p == [0, 0, 0, 255];
                white &= p == [255; 4];
            }
        }
    }
    if transparent || black || white {
        Model::Slim
    } else {
        Model::Classic
    }
}

impl Cape {
    /// Accepts the upstream 64×32, 22×17 and 46×22 cape layouts (integer scales).
    pub fn from_png(png: &[u8]) -> Result<Self> {
        let source = decode(png)?;
        let (w, h) = source.dimensions();
        let base = [(64, 32), (22, 17), (46, 22)]
            .into_iter()
            .find(|&(bw, bh)| w > 0 && w % bw == 0 && w / bw * bh == h)
            .ok_or_else(|| invalid("cape must be a multiple of 64×32, 22×17 or 46×22"))?;
        let scale = w / base.0;
        let mut pixels = RgbaImage::new(64 * scale, 32 * scale);
        image::imageops::replace(&mut pixels, &source, 0, 0);
        Ok(Self { pixels })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_faces_are_mirrored_and_overlays_empty() {
        let source = RgbaImage::from_fn(64, 32, |x, y| Rgba([x as u8, y as u8, 80, 255]));
        let skin = Skin::from_pixels(source, Model::Classic).unwrap();
        assert_eq!(skin.pixels.get_pixel(20, 52).0, [7, 20, 80, 255]);
        assert_eq!(skin.pixels.get_pixel(23, 63).0, [4, 31, 80, 255]);
        assert_eq!(skin.pixels.get_pixel(36, 52).0, [47, 20, 80, 255]);
        assert_eq!(skin.pixels.get_pixel(40, 8)[3], 0);
        assert_eq!(skin.pixels.get_pixel(4, 36)[3], 0);
    }

    #[test]
    fn opaque_background_cleanup_and_explicit_model() {
        let pixels = RgbaImage::from_pixel(64, 64, Rgba([70, 80, 90, 255]));
        let skin = Skin::from_pixels(pixels, Model::Slim).unwrap();
        assert_eq!(skin.model(), Model::Slim);
        assert_eq!(skin.pixels.get_pixel(20, 36)[3], 0);
        assert_eq!(skin.pixels.get_pixel(20, 20)[3], 255);
    }

    #[test]
    fn auto_model_uses_all_four_unused_regions() {
        let mut pixels = RgbaImage::from_pixel(64, 64, Rgba([70, 80, 90, 255]));
        assert_eq!(infer_model(&pixels), Model::Classic);
        pixels.put_pixel(46, 52, Rgba([70, 80, 90, 254]));
        assert_eq!(infer_model(&pixels), Model::Slim);
        for colour in [[0, 0, 0, 255], [255; 4]] {
            let pixels = RgbaImage::from_pixel(64, 64, Rgba(colour));
            assert_eq!(infer_model(&pixels), Model::Slim);
        }
    }

    #[test]
    fn transparent_source_preserves_translucent_outer_layer() {
        let mut pixels = RgbaImage::from_pixel(64, 64, Rgba([70, 80, 90, 255]));
        pixels.put_pixel(40, 8, Rgba([90, 100, 110, 128]));
        let skin = Skin::from_pixels(pixels, Model::Classic).unwrap();
        assert_eq!(skin.pixels.get_pixel(40, 8)[3], 128);
    }

    #[test]
    fn bundled_textures_and_invalid_input() {
        assert_eq!(
            Skin::from_png(include_bytes!("../fixtures/uv-skin.png"), Model::Auto)
                .unwrap()
                .model(),
            Model::Classic
        );
        assert_eq!(
            Skin::from_png(include_bytes!("../fixtures/uv-slim.png"), Model::Auto)
                .unwrap()
                .model(),
            Model::Slim
        );
        assert!(
            Skin::from_png(include_bytes!("../fixtures/uv-legacy.png"), Model::Classic).is_ok()
        );
        assert_eq!(
            Cape::from_png(include_bytes!("../fixtures/uv-cape.png"))
                .unwrap()
                .pixels
                .dimensions(),
            (64, 32)
        );
        assert!(Skin::from_png(b"invalid", Model::Auto).is_err());
        assert!(Skin::from_pixels(RgbaImage::new(64, 31), Model::Classic).is_err());
        assert!(Skin::from_pixels(RgbaImage::new(63, 63), Model::Classic).is_err());
        assert!(Cape::from_png(include_bytes!("../fixtures/uv-skin.png")).is_err());
    }

    #[test]
    fn hd_skin_cape_padding_and_decode_limits() {
        fn encode(pixels: RgbaImage) -> Vec<u8> {
            let mut bytes = Cursor::new(Vec::new());
            pixels
                .write_to(&mut bytes, image::ImageFormat::Png)
                .unwrap();
            bytes.into_inner()
        }
        for (width, height) in [(22, 17), (46, 22), (128, 64)] {
            let png = encode(RgbaImage::from_pixel(
                width,
                height,
                Rgba([90, 80, 70, 255]),
            ));
            let cape = Cape::from_png(&png).unwrap();
            let scale = if width == 128 { 2 } else { 1 };
            assert_eq!(cape.pixels.dimensions(), (64 * scale, 32 * scale));
            assert_eq!(cape.pixels.get_pixel(0, 0).0, [90, 80, 70, 255]);
            assert_eq!(cape.pixels.get_pixel(width - 1, height - 1)[3], 255);
            if width != 128 {
                assert_eq!(cape.pixels.get_pixel(width, 0)[3], 0);
            }
        }
        let png = encode(RgbaImage::from_pixel(128, 64, Rgba([90, 80, 70, 255])));
        let skin = Skin::from_png(&png, Model::Classic).unwrap();
        assert_eq!(skin.pixels.dimensions(), (128, 128));
        assert_eq!(skin.pixels.get_pixel(40, 104).0, [90, 80, 70, 255]);
        let oversized = encode(RgbaImage::new(1088, 1088));
        assert!(Skin::from_png(&oversized, Model::Auto).is_err());
        let transparent_rgb = encode(RgbaImage::from_pixel(64, 64, Rgba([100, 200, 150, 0])));
        assert_eq!(
            Skin::from_png(&transparent_rgb, Model::Classic)
                .unwrap()
                .pixels
                .get_pixel(8, 8)
                .0,
            [0; 4]
        );
    }
}
