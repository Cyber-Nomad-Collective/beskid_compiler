#[unsafe(no_mangle)]
pub extern "C-unwind" fn math_floor(value: f64) -> f64 {
    value.floor()
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn math_ceil(value: f64) -> f64 {
    value.ceil()
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn math_sqrt(value: f64) -> f64 {
    value.sqrt()
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn math_log(value: f64) -> f64 {
    value.ln()
}

#[cfg(test)]
mod tests {
    use super::{math_ceil, math_floor, math_log, math_sqrt};

    #[test]
    fn math_exports_follow_f64_semantics() {
        assert_eq!(math_floor(3.75), 3.0);
        assert_eq!(math_ceil(3.25), 4.0);
        assert_eq!(math_sqrt(81.0), 9.0);
        assert!((math_log(std::f64::consts::E) - 1.0).abs() < f64::EPSILON);
    }
}
