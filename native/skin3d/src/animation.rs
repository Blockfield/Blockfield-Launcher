use crate::{Result, invalid};
use glam::Vec3;
use std::f32::consts::{FRAC_PI_2, PI};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkingPose {
    pub head: Vec3,
    pub left_arm: Vec3,
    pub right_arm: Vec3,
    pub left_leg_x: f32,
    pub right_leg_x: f32,
    pub cape_x: f32,
}

/// Absolute animation time, in seconds; speed 0.6 × upstream natural speed 8.
/// `t=0` is the first upstream animate() pose, before its progress increment.
pub fn walking_pose(t_seconds: f64) -> WalkingPose {
    let t = t_seconds * 0.6 * 8.0;
    WalkingPose {
        head: Vec3::new(
            (t / 5.0).sin() as f32 * 0.1,
            (t / 4.0).sin() as f32 * 0.2,
            0.0,
        ),
        left_arm: Vec3::new(
            (t + std::f64::consts::PI).sin() as f32 * 0.5,
            0.0,
            t.cos() as f32 * 0.03 + PI * 0.02,
        ),
        right_arm: Vec3::new(
            t.sin() as f32 * 0.5,
            0.0,
            (t + std::f64::consts::PI).cos() as f32 * 0.03 - PI * 0.02,
        ),
        left_leg_x: t.sin() as f32 * 0.5,
        right_leg_x: (t + std::f64::consts::PI).sin() as f32 * 0.5,
        cape_x: (t / 1.5).sin() as f32 * 0.06 + PI * 0.06,
    }
}

pub fn auto_rotate_yaw(t_seconds: f64) -> f32 {
    (t_seconds * 0.6).rem_euclid(std::f64::consts::TAU) as f32
}

/// OrbitControls defaults: no damping, pan or zoom; both drag axes use CSS height.
/// Event timestamps use the same caller-owned active clock as render_frame().
#[derive(Clone, Debug, Default)]
pub struct OrbitControls {
    pub(crate) yaw: f32,
    pub(crate) elevation: f32,
    drag_started: Option<f64>,
    paused_seconds: f64,
    last_event: f64,
}

impl OrbitControls {
    pub fn set_dragging(&mut self, t_seconds: f64, dragging: bool) -> Result<()> {
        self.rotation_time(t_seconds)?;
        match (self.drag_started, dragging) {
            (None, true) => self.drag_started = Some(t_seconds),
            (Some(start), false) => {
                self.paused_seconds += t_seconds - start;
                self.drag_started = None;
            }
            _ => {}
        }
        self.last_event = t_seconds;
        Ok(())
    }

    pub fn drag_by(&mut self, dx: f32, dy: f32, logical_height: f32) -> Result<()> {
        if self.drag_started.is_none() {
            return Err(invalid("begin dragging before applying pointer deltas"));
        }
        if !dx.is_finite()
            || !dy.is_finite()
            || !logical_height.is_finite()
            || logical_height <= 0.0
        {
            return Err(invalid(
                "drag deltas and positive logical height must be finite",
            ));
        }
        let radians_per_pixel = std::f64::consts::TAU / f64::from(logical_height);
        self.yaw = (f64::from(self.yaw) - radians_per_pixel * f64::from(dx))
            .rem_euclid(std::f64::consts::TAU) as f32;
        self.elevation = (f64::from(self.elevation) + radians_per_pixel * f64::from(dy))
            .clamp(f64::from(-FRAC_PI_2 + 1e-6), f64::from(FRAC_PI_2 - 1e-6))
            as f32;
        Ok(())
    }

    pub fn is_dragging(&self) -> bool {
        self.drag_started.is_some()
    }

    pub(crate) fn rotation_time(&self, t_seconds: f64) -> Result<f64> {
        if !t_seconds.is_finite() || t_seconds < self.last_event {
            return Err(invalid(
                "time must be finite, nonnegative and at/after the last drag event",
            ));
        }
        Ok(self.drag_started.unwrap_or(t_seconds) - self.paused_seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-6, "{a} != {b}");
    }

    #[test]
    fn upstream_walking_angles_at_zero_and_quarter_stride() {
        let p = walking_pose(0.0);
        near(p.left_leg_x, 0.0);
        near(p.left_arm.z, PI * 0.02 + 0.03);
        near(p.right_arm.z, -PI * 0.02 - 0.03);
        near(p.cape_x, PI * 0.06);
        let p = walking_pose(std::f64::consts::FRAC_PI_2 / 4.8);
        near(p.left_leg_x, 0.5);
        near(p.right_leg_x, -0.5);
        near(p.left_arm.x, -0.5);
        near(p.right_arm.x, 0.5);
        near(p.head.y, (PI / 8.0).sin() * 0.2);
        near(p.head.x, (PI / 10.0).sin() * 0.1);
        near(p.cape_x, (PI / 3.0).sin() * 0.06 + PI * 0.06);
        near(auto_rotate_yaw(2.0), 1.2);
    }

    #[test]
    fn drag_pauses_only_auto_rotation_and_resumes_without_jump() {
        let mut c = OrbitControls::default();
        c.set_dragging(1.0, true).unwrap();
        c.drag_by(75.0, 25.0, 300.0).unwrap();
        near(c.yaw, 3.0 * FRAC_PI_2);
        near(c.elevation, PI / 6.0);
        assert_eq!(c.rotation_time(3.0).unwrap(), 1.0);
        assert_ne!(walking_pose(1.0), walking_pose(3.0));
        c.set_dragging(3.0, false).unwrap();
        assert_eq!(c.rotation_time(3.0).unwrap(), 1.0);
        assert_eq!(c.rotation_time(4.0).unwrap(), 2.0);
        c.set_dragging(5.0, true).unwrap();
        c.set_dragging(6.0, false).unwrap();
        assert_eq!(c.rotation_time(7.0).unwrap(), 4.0);
        assert!(c.set_dragging(5.0, true).is_err());
        assert!(c.rotation_time(f64::NAN).is_err());
        assert!(c.drag_by(1.0, 0.0, 300.0).is_err());
        c.set_dragging(8.0, true).unwrap();
        c.drag_by(f32::MAX, f32::MAX, f32::MIN_POSITIVE).unwrap();
        assert!(c.yaw.is_finite() && c.elevation.is_finite());
    }
}
