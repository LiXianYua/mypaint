/// A mapping from inputs to output values via piecewise-linear control points.
/// Corresponds to MyPaintMapping in mypaint-mapping.c.
pub struct Mapping {
    base_value: f32,
    points_list: Vec<ControlPoints>,
    inputs_used: usize,
}

struct ControlPoints {
    xvalues: [f32; 64],
    yvalues: [f32; 64],
    n: usize,
}

impl Mapping {
    pub fn new(num_inputs: usize) -> Self {
        let points_list = (0..num_inputs)
            .map(|_| ControlPoints {
                xvalues: [0.0; 64],
                yvalues: [0.0; 64],
                n: 0,
            })
            .collect();
        Self {
            base_value: 0.0,
            points_list,
            inputs_used: 0,
        }
    }

    pub fn get_base_value(&self) -> f32 {
        self.base_value
    }

    pub fn set_base_value(&mut self, value: f32) {
        self.base_value = value;
    }

    pub fn set_n(&mut self, input: usize, n: usize) {
        assert!(input < self.points_list.len());
        assert!(n <= 64);
        assert!(n != 1, "cannot build mapping with only one point");
        let p = &mut self.points_list[input];
        if n != 0 && p.n == 0 {
            self.inputs_used += 1;
        }
        if n == 0 && p.n != 0 {
            self.inputs_used -= 1;
        }
        p.n = n;
    }

    pub fn get_n(&self, input: usize) -> usize {
        assert!(input < self.points_list.len());
        self.points_list[input].n
    }

    pub fn set_point(&mut self, input: usize, index: usize, x: f32, y: f32) {
        assert!(input < self.points_list.len());
        let p = &mut self.points_list[input];
        assert!(index < p.n);
        if index > 0 {
            assert!(x >= p.xvalues[index - 1]);
        }
        p.xvalues[index] = x;
        p.yvalues[index] = y;
    }

    pub fn get_point(&self, input: usize, index: usize) -> (f32, f32) {
        assert!(input < self.points_list.len());
        let p = &self.points_list[input];
        assert!(index < p.n);
        (p.xvalues[index], p.yvalues[index])
    }

    pub fn is_constant(&self) -> bool {
        self.inputs_used == 0
    }

    pub fn inputs_used_n(&self) -> usize {
        self.inputs_used
    }

    /// Calculate the mapping output given input values.
    /// Corresponds to `mypaint_mapping_calculate` in mypaint-mapping.c:146.
    pub fn calculate(&self, data: &[f32]) -> f32 {
        let mut result = self.base_value;
        if self.inputs_used == 0 {
            return result;
        }
        for (j, p) in self.points_list.iter().enumerate() {
            if p.n > 0 {
                let x = data[j];
                let mut x0 = p.xvalues[0];
                let mut y0 = p.yvalues[0];
                let mut x1 = p.xvalues[1];
                let mut y1 = p.yvalues[1];
                let mut i = 2;
                while i < p.n && x > x1 {
                    x0 = x1;
                    y0 = y1;
                    x1 = p.xvalues[i];
                    y1 = p.yvalues[i];
                    i += 1;
                }
                let y = if x0 == x1 || y0 == y1 {
                    y0
                } else {
                    (y1 * (x - x0) + y0 * (x1 - x)) / (x1 - x0)
                };
                result += y;
            }
        }
        result
    }

    /// Calculate mapping with a single input value.
    /// Corresponds to `mypaint_mapping_calculate_single_input` in mypaint-mapping.c:191.
    pub fn calculate_single(&self, input: f32) -> f32 {
        self.calculate(std::slice::from_ref(&input))
    }
}
