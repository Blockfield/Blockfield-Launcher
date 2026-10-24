//! Headless, caller-driven skinview3d 3.4.2 model port. See PORTING.md for provenance
//! and README.md for the visibility, time, drag and straight-alpha RGBA contract.

mod animation;
mod model;
mod renderer;
mod texture;

pub use animation::{OrbitControls, WalkingPose, auto_rotate_yaw, walking_pose};
pub use renderer::{Frame, RenderSize, Renderer};
pub use texture::{Cape, Model, Skin};

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

fn invalid(message: &str) -> Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message).into()
}
