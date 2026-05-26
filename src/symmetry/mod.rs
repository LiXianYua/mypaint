//! Symmetry painting support. Corresponds to mypaint-symmetry.c/h.

use crate::util::matrix::Transform;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymmetryType {
    Vertical,
    Horizontal,
    VertHorz,
    Rotational,
    Snowflake,
}

pub struct SymmetryState {
    pub symmetry_type: SymmetryType,
    pub center_x: f32,
    pub center_y: f32,
    pub angle: f32,
    pub num_lines: f32,
}

pub struct SymmetryData {
    state_current: SymmetryState,
    state_pending: SymmetryState,
    pending_changes: bool,
    active: bool,
    symmetry_matrices: Vec<Transform>,
}

impl Default for SymmetryData {
    fn default() -> Self {
        Self {
            state_current: SymmetryState {
                symmetry_type: SymmetryType::Vertical,
                center_x: 0.0, center_y: 0.0,
                angle: 0.0, num_lines: 0.0,
            },
            state_pending: SymmetryState {
                symmetry_type: SymmetryType::Vertical,
                center_x: 0.0, center_y: 0.0,
                angle: 0.0, num_lines: 0.0,
            },
            pending_changes: false,
            active: false,
            symmetry_matrices: Vec::new(),
        }
    }
}

impl SymmetryData {
    pub fn set_pending(&mut self, active: bool, center_x: f32, center_y: f32,
        symmetry_angle: f32, symmetry_type: SymmetryType, rot_symmetry_lines: i32)
    {
        self.state_pending = SymmetryState {
            symmetry_type, center_x, center_y,
            angle: symmetry_angle,
            num_lines: rot_symmetry_lines as f32,
        };
        self.active = active;
        self.pending_changes = true;
    }

    pub fn update(&mut self) {
        if !self.pending_changes { return; }
        self.state_current = SymmetryState {
            symmetry_type: self.state_pending.symmetry_type,
            center_x: self.state_pending.center_x,
            center_y: self.state_pending.center_y,
            angle: self.state_pending.angle,
            num_lines: self.state_pending.num_lines,
        };
        self.pending_changes = false;
        self.recalculate_matrices();
    }

    fn recalculate_matrices(&mut self) {
        if !self.active {
            self.symmetry_matrices.clear();
            return;
        }

        let angle_rad = self.state_current.angle.to_radians();
        let cos_a = angle_rad.cos();
        let sin_a = angle_rad.sin();
        let cx = self.state_current.center_x;
        let cy = self.state_current.center_y;

        match self.state_current.symmetry_type {
            SymmetryType::Vertical => {
                // Mirror across vertical line at center_x
                self.symmetry_matrices = vec![
                    Transform::identity(),
                    Transform {
                        xx: -1.0, xy: 0.0, yx: 0.0, yy: 1.0,
                        x0: 2.0 * cx, y0: 0.0,
                    },
                ];
            }
            SymmetryType::Horizontal => {
                self.symmetry_matrices = vec![
                    Transform::identity(),
                    Transform {
                        xx: 1.0, xy: 0.0, yx: 0.0, yy: -1.0,
                        x0: 0.0, y0: 2.0 * cy,
                    },
                ];
            }
            SymmetryType::VertHorz => {
                self.symmetry_matrices = vec![
                    Transform::identity(),
                    Transform {
                        xx: -1.0, xy: 0.0, yx: 0.0, yy: 1.0,
                        x0: 2.0 * cx, y0: 0.0,
                    },
                    Transform {
                        xx: 1.0, xy: 0.0, yx: 0.0, yy: -1.0,
                        x0: 0.0, y0: 2.0 * cy,
                    },
                    Transform {
                        xx: -1.0, xy: 0.0, yx: 0.0, yy: -1.0,
                        x0: 2.0 * cx, y0: 2.0 * cy,
                    },
                ];
            }
            SymmetryType::Rotational => {
                let n = self.state_current.num_lines as i32;
                let mut mats = Vec::with_capacity(n.max(1) as usize);
                for i in 0..n {
                    let theta = angle_rad + (2.0 * std::f32::consts::PI * i as f32) / n as f32;
                    let c = theta.cos();
                    let s = theta.sin();
                    mats.push(Transform {
                        xx: c, xy: -s, yx: s, yy: c,
                        x0: cx - cx * c + cy * s,
                        y0: cy - cx * s - cy * c,
                    });
                }
                self.symmetry_matrices = mats;
            }
            SymmetryType::Snowflake => {
                let n = self.state_current.num_lines as i32;
                let mut mats = Vec::with_capacity((n * 2).max(2) as usize);
                for i in 0..n {
                    let theta = angle_rad + (2.0 * std::f32::consts::PI * i as f32) / n as f32;
                    let c = theta.cos();
                    let s = theta.sin();
                    mats.push(Transform {
                        xx: c, xy: -s, yx: s, yy: c,
                        x0: cx - cx * c + cy * s,
                        y0: cy - cx * s - cy * c,
                    });
                    // Reflection
                    mats.push(Transform {
                        xx: -c, xy: s, yx: s, yy: c,
                        x0: cx + cx * c - cy * s,
                        y0: cy - cx * s - cy * c,
                    });
                }
                self.symmetry_matrices = mats;
            }
        }
    }

    pub fn num_symmetry_points(&self) -> usize {
        if !self.active { return 1; }
        self.symmetry_matrices.len().max(1)
    }

    pub fn transform_point(&self, index: usize, x: f32, y: f32) -> (f32, f32) {
        if index == 0 || self.symmetry_matrices.is_empty() {
            return (x, y);
        }
        if index >= self.symmetry_matrices.len() {
            return (x, y);
        }
        let t = &self.symmetry_matrices[index];
        t.transform_point(x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_inactive_returns_input_unchanged() {
        let sd = SymmetryData::default();
        let (x, y) = sd.transform_point(0, 5.0, 7.0);
        assert_eq!((x, y), (5.0, 7.0));
        assert_eq!(sd.num_symmetry_points(), 1);
    }

    #[test]
    fn vertical_symmetry_mirrors_x() {
        let mut sd = SymmetryData::default();
        sd.set_pending(true, 100.0, 0.0, 0.0, SymmetryType::Vertical, 0);
        sd.update();
        assert_eq!(sd.num_symmetry_points(), 2);
        // 第二个点是 x 镜像
        let (x, y) = sd.transform_point(1, 10.0, 50.0);
        assert!((x - 190.0).abs() < 1e-5, "expected x=190, got {x}");
        assert!((y - 50.0).abs() < 1e-5);
    }

    #[test]
    fn horizontal_symmetry_mirrors_y() {
        let mut sd = SymmetryData::default();
        sd.set_pending(true, 0.0, 100.0, 0.0, SymmetryType::Horizontal, 0);
        sd.update();
        assert_eq!(sd.num_symmetry_points(), 2);
        let (x, y) = sd.transform_point(1, 10.0, 30.0);
        assert!((x - 10.0).abs() < 1e-5);
        assert!((y - 170.0).abs() < 1e-5);
    }

    #[test]
    fn verthorz_produces_four_points() {
        let mut sd = SymmetryData::default();
        sd.set_pending(true, 0.0, 0.0, 0.0, SymmetryType::VertHorz, 0);
        sd.update();
        assert_eq!(sd.num_symmetry_points(), 4);
    }
}
