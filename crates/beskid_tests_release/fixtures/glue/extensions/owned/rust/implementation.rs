//! Ordinary Rust implementation of the v0.6.1 Glue extension fixture. Records and enums are the
//! owner types the Beskid declarations name; callbacks arrive as `&mut dyn FnMut`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub tag: u8,
    pub x: i64,
    pub weight: f64,
    pub visible: bool,
    pub mark: char,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Red,
    Green,
    Blue,
}

pub fn glue_point(value: Point) -> Point {
    Point {
        tag: value.tag.wrapping_add(1),
        x: value.x.wrapping_mul(2),
        weight: value.weight + 0.5,
        visible: !value.visible,
        mark: value.mark,
    }
}

pub fn glue_next_color(value: Color) -> Color {
    match value {
        Color::Red => Color::Green,
        Color::Green => Color::Blue,
        Color::Blue => Color::Red,
    }
}

pub fn glue_sum_each(count: i64, visit: &mut dyn FnMut(i64) -> i64) -> i64 {
    (0..count).map(|index| visit(index)).sum()
}

pub fn glue_count_keep(count: i64, keep: &mut dyn FnMut(i64, Color) -> bool) -> i64 {
    let colors = [Color::Red, Color::Green, Color::Blue];
    (0..count).filter(|index| keep(*index, colors[(*index % 3) as usize])).count() as i64
}

/// Calls back into Beskid through the generated consumer module; -1 reports a failed call.
pub fn glue_reenter(value: i64) -> i64 {
    crate::beskid::ext_double(value).map_or(-1, |doubled| doubled + 1)
}
