//! PlayerHead avatar pipeline: the Tauri `PlayerHead.tsx` + `player-face.ts` port.
//!
//! Tauri crops the 8x8 face at (8,8) plus the hat overlay at (40,8) off the
//! resolved skin, caches one face per username, and drops the entry when a
//! skin upload succeeds (`forgetFace`). The Slint tree only renders: every
//! avatar slot shows the bundled default head with the cached face on top.
//!
//! The cache keeps `SharedPixelBuffer`s (`slint::Image` is not `Send`); the
//! UI thread converts to `Image` when painting a slot.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use slint::{Image, Model, Rgba8Pixel, SharedPixelBuffer, VecModel, Weak};
use tokio::runtime::Handle;

use crate::{AppWindow, StatsRow};

const FACE_X: u32 = 8;
const FACE_Y: u32 = 8;
const HAT_X: u32 = 40;
const HAT_Y: u32 = 8;
const FACE_SIZE: u32 = 8;
const DATA_URL_PREFIX: &str = "data:image/png;base64,";

/// `^[A-Za-z0-9_]{3,16}$`, like `PlayerHead.tsx`: invalid names never fetch.
pub fn valid_username(name: &str) -> bool {
    let bytes = name.as_bytes();
    (3..=16).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

/// Crop the 8x8 face (base layer plus hat overlay) out of raw skin PNG bytes.
/// Returns `None` for undecodable or undersized skins; the caller then shows
/// only the bundled default head, like the Tauri `null` face.
pub fn face_from_skin(png: &[u8]) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let image = image::load_from_memory(png).ok()?.to_rgba8();
    let (width, height) = (image.width(), image.height());
    if width < HAT_X + FACE_SIZE || height < HAT_Y + FACE_SIZE {
        return None;
    }
    let mut face = SharedPixelBuffer::new(FACE_SIZE, FACE_SIZE);
    let pixels = face.make_mut_slice();
    for y in 0..FACE_SIZE {
        for x in 0..FACE_SIZE {
            let base = image.get_pixel(FACE_X + x, FACE_Y + y).0;
            let over = image.get_pixel(HAT_X + x, HAT_Y + y).0;
            pixels[(y * FACE_SIZE + x) as usize] = blend_over(base, over);
        }
    }
    Some(face)
}

/// Source-over composite of one overlay pixel onto one base pixel.
fn blend_over(base: [u8; 4], over: [u8; 4]) -> Rgba8Pixel {
    let (oa, ba) = (f32::from(over[3]) / 255.0, f32::from(base[3]) / 255.0);
    let out_a = oa + ba * (1.0 - oa);
    if out_a <= 0.0 {
        return Rgba8Pixel {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
    }
    let mix = |bc: u8, oc: u8| {
        ((f32::from(oc) * oa + f32::from(bc) * ba * (1.0 - oa)) / out_a).round() as u8
    };
    Rgba8Pixel {
        r: mix(base[0], over[0]),
        g: mix(base[1], over[1]),
        b: mix(base[2], over[2]),
        a: (out_a * 255.0).round() as u8,
    }
}

/// Whole pipeline for one resolved `preview_skin` data URL.
pub fn face_from_data_url(url: &str) -> Option<Image> {
    let png = face_png_from_data_url(url)?;
    face_image_from_skin(&png)
}

fn face_png_from_data_url(url: &str) -> Option<Vec<u8>> {
    let encoded = url.strip_prefix(DATA_URL_PREFIX)?;
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()
}

pub fn face_image_from_skin(png: &[u8]) -> Option<Image> {
    face_from_skin(png).map(Image::from_rgba8)
}

type FaceMap = HashMap<String, SharedPixelBuffer<Rgba8Pixel>>;

fn cache() -> &'static Mutex<FaceMap> {
    static CACHE: std::sync::OnceLock<Mutex<FaceMap>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn inflight() -> &'static Mutex<HashSet<String>> {
    static INFLIGHT: std::sync::OnceLock<Mutex<HashSet<String>>> = std::sync::OnceLock::new();
    INFLIGHT.get_or_init(|| Mutex::new(HashSet::new()))
}

pub fn cached_face(username: &str) -> Option<Image> {
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(username)
        .cloned()
        .map(Image::from_rgba8)
}

fn claim_inflight(username: &str) -> bool {
    inflight()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(username.to_owned())
}

fn release_inflight(username: &str) {
    inflight()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(username);
}

/// Test/fixture hook: seed the cache without network.
pub fn store_face(username: &str, face: SharedPixelBuffer<Rgba8Pixel>) {
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(username.to_owned(), face);
}

/// Skin-upload invalidation, mirrors Tauri `forgetFace`.
pub fn forget_face(username: &str) {
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(username);
}

/// Clear the whole cache (logout / tests only).
pub fn clear_cache() {
    cache().lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// Deterministic fixture face from the bundled 64x64 test skin, so goldens
/// and Layer-1 tests render the same overlay every run.
pub fn fixture_face() -> Image {
    face_image_from_skin(include_bytes!("../assets/fixtures/skin.png")).unwrap_or_default()
}

/// Patch fixture faces into every avatar slot (rows, self card, shell).
pub fn apply_fixture_faces(app: &AppWindow) {
    let face = fixture_face();
    patch_rows(app, &face);
    app.set_stats_self_face(face.clone());
    app.set_shell_face(face);
}

fn patch_rows(app: &AppWindow, face: &Image) {
    let rows = app.get_stats_rows();
    let Some(vec) = rows.as_any().downcast_ref::<VecModel<StatsRow>>() else {
        return;
    };
    for index in 0..vec.row_count() {
        if let Some(mut row) = vec.row_data(index) {
            row.face = face.clone();
            vec.set_row_data(index, row);
        }
    }
}

/// Shell slot follows the signed-in username; empty until its face resolves.
pub fn refresh_shell(app: &AppWindow) {
    let face = cached_face(app.get_username().as_str()).unwrap_or_default();
    app.set_shell_face(face);
}

/// Fetch every uncached name through `preview_skin` off the UI thread, then
/// paint the rows/self/shell that still show that name. Rows that paged away
/// in the meantime match nothing and stay untouched.
///
/// Parallel lookups are capped: a full board fans out to ~27 names, and
/// uncapped `preview_skin` chains hammer the skin hosts.
pub const MAX_CONCURRENT_FACE_FETCHES: usize = 8;

fn face_semaphore() -> &'static Arc<tokio::sync::Semaphore> {
    static SEMAPHORE: std::sync::OnceLock<Arc<tokio::sync::Semaphore>> = std::sync::OnceLock::new();
    SEMAPHORE.get_or_init(|| Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_FACE_FETCHES)))
}

pub fn request_faces(
    weak: &Weak<AppWindow>,
    handle: &Handle,
    skins_base: Option<String>,
    usernames: Vec<String>,
) {
    let mut distinct = usernames;
    distinct.sort();
    distinct.dedup();
    for username in distinct {
        if !valid_username(&username)
            || cached_face(&username).is_some()
            || !claim_inflight(&username)
        {
            continue;
        }
        let weak = weak.clone();
        let skins_base = skins_base.clone();
        handle.spawn(async move {
            // One permit per in-flight lookup, released when the task ends.
            let _permit = face_semaphore().clone().acquire_owned().await.ok();
            let face = fetch_face(&skins_base, &username).await;
            if let Some(face) = face.as_ref() {
                store_face(&username, face.clone());
            }
            release_inflight(&username);
            let username = username.clone();
            let _ = weak.upgrade_in_event_loop(move |app| {
                if let Some(face) = cached_face(&username) {
                    patch_named_row(&app, &username, &face);
                    if app.get_stats_self_name().as_str() == username {
                        app.set_stats_self_face(face.clone());
                    }
                    if app.get_username().as_str() == username {
                        app.set_shell_face(face);
                    }
                }
            });
        });
    }
}

fn patch_named_row(app: &AppWindow, username: &str, face: &Image) {
    let rows = app.get_stats_rows();
    let Some(vec) = rows.as_any().downcast_ref::<VecModel<StatsRow>>() else {
        return;
    };
    for index in 0..vec.row_count() {
        let matches = vec
            .row_data(index)
            .is_some_and(|row| row.name.as_str() == username);
        if matches && let Some(mut row) = vec.row_data(index) {
            row.face = face.clone();
            vec.set_row_data(index, row);
        }
    }
}

async fn fetch_face(
    skins_base: &Option<String>,
    username: &str,
) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let preview = launcher_net::skins::preview_skin(
        skins_base.clone(),
        username.to_owned(),
        None,
        None,
        false,
    )
    .await
    .ok()?;
    face_png_from_data_url(preview.skin.as_deref()?).and_then(|png| face_from_skin(&png))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face_pixel(face: &SharedPixelBuffer<Rgba8Pixel>, x: u32, y: u32) -> Rgba8Pixel {
        face.as_slice()[(y * FACE_SIZE + x) as usize]
    }

    /// 64x64 test skin: red base face, blue hat overlay, transparent rest.
    fn test_skin() -> Vec<u8> {
        let mut image = image::RgbaImage::new(64, 64);
        for y in 0..8 {
            for x in 0..8 {
                image.put_pixel(8 + x, 8 + y, image::Rgba([255, 0, 0, 255]));
                image.put_pixel(40 + x, 8 + y, image::Rgba([0, 0, 255, 255]));
            }
        }
        let mut png = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("encode test skin");
        png
    }

    #[test]
    fn username_gate_mirrors_player_head_tsx() {
        assert!(valid_username("CaptureOperator"));
        assert!(valid_username("abc"));
        assert!(valid_username("Player_123456789"));
        assert!(!valid_username("ab"));
        assert!(!valid_username("0123456789abcdefg"));
        assert!(!valid_username("evil name"));
        assert!(!valid_username("evil-name"));
        assert!(!valid_username(""));
        assert!(!valid_username(" Notch"));
    }

    #[test]
    fn opaque_overlay_wins_over_base() {
        let face = face_from_skin(&test_skin()).expect("face");
        assert_eq!((face.width(), face.height()), (8, 8));
        let blue = Rgba8Pixel {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        };
        assert_eq!(face_pixel(&face, 0, 0), blue);
        assert_eq!(face_pixel(&face, 7, 7), blue);
    }

    #[test]
    fn transparent_overlay_keeps_base() {
        let mut image = image::RgbaImage::new(64, 64);
        for y in 0..8 {
            for x in 0..8 {
                image.put_pixel(8 + x, 8 + y, image::Rgba([0, 255, 0, 255]));
            }
        }
        let mut png = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("encode");
        let face = face_from_skin(&png).expect("face");
        assert_eq!(
            face_pixel(&face, 3, 3),
            Rgba8Pixel {
                r: 0,
                g: 255,
                b: 0,
                a: 255,
            }
        );
    }

    #[test]
    fn undersized_or_broken_skins_have_no_face() {
        assert!(face_from_skin(b"not a png").is_none());
        let mut tiny = image::RgbaImage::new(8, 8);
        tiny.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        let mut png = Vec::new();
        tiny.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("encode");
        assert!(face_from_skin(&png).is_none());
    }

    #[test]
    fn data_url_roundtrip_crops() {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(test_skin());
        let url = format!("{DATA_URL_PREFIX}{encoded}");
        let face = face_from_data_url(&url).expect("face");
        assert_eq!((face.size().width, face.size().height), (8, 8));
        assert!(face_from_data_url("https://example/skin.png").is_none());
        assert!(face_from_data_url(&format!("{DATA_URL_PREFIX}!!!")).is_none());
    }

    #[test]
    fn cache_stores_and_forgets_per_username() {
        clear_cache();
        assert!(cached_face("Notch").is_none());
        store_face("Notch", SharedPixelBuffer::new(8, 8));
        assert!(cached_face("Notch").is_some());
        forget_face("Notch");
        assert!(cached_face("Notch").is_none());
        store_face("A", SharedPixelBuffer::new(8, 8));
        store_face("B", SharedPixelBuffer::new(8, 8));
        forget_face("A");
        assert!(cached_face("A").is_none());
        assert!(cached_face("B").is_some());
        clear_cache();
    }

    #[test]
    fn bundled_fixture_skin_crops() {
        let face = fixture_face();
        assert_eq!((face.size().width, face.size().height), (8, 8));
    }

    #[test]
    fn face_lookups_are_bounded() {
        assert!((1..=16).contains(&MAX_CONCURRENT_FACE_FETCHES));
        let mut permits = Vec::new();
        for _ in 0..MAX_CONCURRENT_FACE_FETCHES {
            permits.push(
                face_semaphore()
                    .try_acquire()
                    .expect("permit within the face-fetch bound"),
            );
        }
        assert!(
            face_semaphore().try_acquire().is_err(),
            "face fetches must stay bounded"
        );
        drop(permits);
        assert!(face_semaphore().try_acquire().is_ok());
    }
}
