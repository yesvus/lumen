use serde::Deserialize;

/// The niri column-grouped tab strip, ported from Colonnade
/// (github.com/yesvus/colonnade) via its toolkit-independent
/// `colonnade-core` crate. See that repo's BEHAVIOR.md for the
/// interaction spec these knobs tune.
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct ColonnadeModuleConfig {
    /// Each column's tab width is `width_fraction * tab_width_scale_px`
    /// -- see `colonnade_core::column::group`'s doc comment.
    pub tab_width_scale_px: f64,
    /// Floor so a tiny width_fraction never produces a degenerate tab.
    pub min_tab_width_px: i32,
    /// Real pixel budget for the visible tab group on the bloomed
    /// workspace -- however many tabs fit at their true width is how
    /// many show.
    pub max_group_width_px: i32,
    /// Caps collapsed-marker and overflow-tick glyph strings at this many
    /// characters.
    pub max_overflow_glyphs: usize,
    /// A tab's drawn height in pixels.
    pub tab_height_px: f32,
    /// When true (default), each tab's width tracks its real niri column
    /// width. When false, every tab gets a uniform `tab_width_scale_px`.
    pub dynamic_tab_width: bool,
    /// When true (default), each tab's SVG icon is recolored to a flat
    /// silhouette in the tab's own text color (also picking up the same
    /// focus-dimming as the title) instead of its native multi-color
    /// artwork. Raster icons have no such filter available and always
    /// keep their native colors regardless of this setting.
    pub monochrome_tab_icons: bool,
}

impl Default for ColonnadeModuleConfig {
    fn default() -> Self {
        Self {
            tab_width_scale_px: 200.0,
            min_tab_width_px: 40,
            max_group_width_px: 700,
            max_overflow_glyphs: 12,
            tab_height_px: 22.0,
            dynamic_tab_width: true,
            monochrome_tab_icons: true,
        }
    }
}
