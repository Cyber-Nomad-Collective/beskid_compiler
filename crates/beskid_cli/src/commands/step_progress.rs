use std::time::Duration;

pub fn log_step(step: usize, total: usize, name: &str, detail: impl AsRef<str>) {
    eprintln!("[{step}/{total}] {name} — {}", detail.as_ref());
}

pub fn log_step_with_duration(step: usize, total: usize, name: &str, duration: Duration) {
    log_step(step, total, name, format!("done in {}", format_duration(duration)));
}

pub fn format_duration(duration: Duration) -> String {
    if duration.as_millis() >= 1_000 {
        format!("{:.2}s", duration.as_secs_f64())
    } else if duration.as_micros() >= 1_000 {
        format!("{:.2}ms", duration.as_secs_f64() * 1_000.0)
    } else {
        format!("{}µs", duration.as_micros())
    }
}
