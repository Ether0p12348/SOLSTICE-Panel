use crate::led::model::LedColor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinLedProgram {
    pub mode: u8,
    pub speed: u8,
    pub color_index: u8,
    pub estimated_color: LedColor,
    pub source_effect_id: String,
}

pub fn estimated_frame(program: &BuiltinLedProgram, led_count: usize) -> Vec<LedColor> {
    let count = led_count.max(1);
    vec![program.estimated_color; count]
}
