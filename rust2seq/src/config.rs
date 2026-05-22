//! In-memory config consumed by the driver.

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub style: StyleConfig,
}

#[derive(Debug, Clone)]
pub struct StyleConfig {
    pub shadowing: bool,
    pub round_corner: u32,
    pub default_font_name: String,
    pub default_font_size: u32,
    pub sequence_arrow_thickness: f32,
    pub sequence_message_align: String,
    pub response_message_below_arrow: bool,
    pub participant_padding: u32,
    pub box_padding: u32,
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            shadowing: false,
            round_corner: 8,
            default_font_name: "Helvetica".into(),
            default_font_size: 12,
            sequence_arrow_thickness: 1.4,
            sequence_message_align: "center".into(),
            response_message_below_arrow: true,
            participant_padding: 24,
            box_padding: 12,
        }
    }
}
