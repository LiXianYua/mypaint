fn main() {
    use libmypaint::BrushSetting;
    let s = BrushSetting::from_cname("elliptical_dab_ratio");
    println!("elliptical_dab_ratio → {:?}", s);
    println!(
        "EllipticalDabRatio idx = {}",
        BrushSetting::EllipticalDabRatio as usize
    );

    // 加载 charcoal 并打印 base value
    use libmypaint::Brush;
    let json = std::fs::read_to_string("tests/brushes/charcoal.myb").unwrap();
    let mut b = Brush::new();
    let _ = b.from_string(&json);
    let v = b.get_base_value(BrushSetting::EllipticalDabRatio);
    println!("loaded EllipticalDabRatio base = {}", v);
    let v2 = b.get_base_value(BrushSetting::EllipticalDabAngle);
    println!("loaded EllipticalDabAngle base = {}", v2);
    let v3 = b.get_base_value(BrushSetting::Hardness);
    println!("loaded Hardness base = {}", v3);
}
