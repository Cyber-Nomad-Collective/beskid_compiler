//! Human-readable elapsed time formatting.

use std::time::Duration;

pub fn format_duration(duration: Duration) -> String {
    let millis = duration.as_millis();
    if millis < 1_000 {
        return format!("{millis}ms");
    }
    let secs = duration.as_secs_f64();
    if secs < 60.0 {
        return format!("{secs:.1}s");
    }
    let minutes = duration.as_secs() / 60;
    let seconds = duration.as_secs() % 60;
    format!("{minutes}m {seconds:02}s")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_subsecond_and_long() {
        assert_eq!(format_duration(Duration::from_millis(120)), "120ms");
        assert_eq!(format_duration(Duration::from_secs(3)), "3.0s");
        assert_eq!(format_duration(Duration::from_secs(125)), "2m 05s");
    }
}
