//! Sample Rust source used by the CodeWidget demo.

/// Sample Rust code rendered by the interactive demo.
pub const SAMPLE_RUST_CODE: &str = r#"pub struct Counter {
    value: usize,
}

impl Counter {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn increment(&mut self) {
        self.value += 1;
    }

    pub fn value(&self) -> usize {
        self.value
    }
}

fn main() {
    let mut counter = Counter::new();
    counter.increment();
    println!("count = {}", counter.value());
}
"#;
