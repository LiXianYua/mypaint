use mypaint::mapping::Mapping;

#[test]
fn test_mapping_constant_returns_base_value() {
    let mut m = Mapping::new(4);
    m.set_base_value(0.5);
    let inputs = [0.0; 4];
    assert!((m.calculate(&inputs) - 0.5).abs() < 1e-6);
}

#[test]
fn test_mapping_single_input_linear_interpolation() {
    let mut m = Mapping::new(4);
    m.set_base_value(0.0);
    m.set_n(0, 2);
    m.set_point(0, 0, 0.0, 0.0);
    m.set_point(0, 1, 1.0, 1.0);
    let inputs = [0.5, 0.0, 0.0, 0.0];
    let result = m.calculate(&inputs);
    assert!((result - 0.5).abs() < 1e-6, "expected 0.5, got {result}");
}

#[test]
fn test_mapping_is_constant() {
    let mut m = Mapping::new(4);
    m.set_base_value(1.0);
    assert!(m.is_constant());
    m.set_n(0, 2);
    assert!(!m.is_constant());
}

#[test]
fn test_mapping_out_of_bounds_extrapolates_first_segment() {
    let mut m = Mapping::new(4);
    m.set_base_value(0.0);
    m.set_n(0, 2);
    m.set_point(0, 0, 0.2, 0.2);
    m.set_point(0, 1, 0.8, 0.8);
    let inputs = [0.0, 0.0, 0.0, 0.0];
    let result = m.calculate(&inputs);
    // x < x0 → linear interpolation of first segment: (0.8*(0-0.2)+0.2*(0.8-0))/(0.8-0.2) = 0
    assert!((result - 0.0).abs() < 1e-6, "expected 0.0, got {result}");
}

#[test]
fn test_mapping_multiple_inputs_add() {
    let mut m = Mapping::new(4);
    m.set_base_value(0.0);
    m.set_n(0, 2);
    m.set_point(0, 0, 0.0, 1.0);
    m.set_point(0, 1, 1.0, 2.0);
    m.set_n(1, 2);
    m.set_point(1, 0, 0.0, 3.0);
    m.set_point(1, 1, 1.0, 4.0);
    let inputs = [0.5, 0.5, 0.0, 0.0];
    let result = m.calculate(&inputs);
    // input 0 → 1.5, input 1 → 3.5, base 0.0 → total 5.0
    assert!((result - 5.0).abs() < 1e-6, "expected 5.0, got {result}");
}
