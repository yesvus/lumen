use hex_color::HexColor;
use iced::{Color, theme::palette};
use serde::{
    Deserialize, Deserializer,
    de::{self, MapAccess, Visitor},
};

fn hex_to_color(hex: HexColor) -> Color {
    Color::from_rgb8(hex.r, hex.g, hex.b)
}

fn hex_to_pair(hex: HexColor, text: Option<HexColor>, text_fallback: Color) -> palette::Pair {
    palette::Pair::new(
        hex_to_color(hex),
        text.map(hex_to_color).unwrap_or(text_fallback),
    )
}

#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(untagged)]
pub enum AppearanceColor {
    Simple(HexColor),
    Complete {
        base: HexColor,
        strong: Option<HexColor>,
        weak: Option<HexColor>,
        text: Option<HexColor>,
    },
}

impl AppearanceColor {
    pub fn get_base(&self) -> Color {
        match self {
            AppearanceColor::Simple(color) => hex_to_color(*color),
            AppearanceColor::Complete { base, .. } => hex_to_color(*base),
        }
    }

    pub fn get_text(&self) -> Option<Color> {
        match self {
            AppearanceColor::Simple(_) => None,
            AppearanceColor::Complete { text, .. } => text.map(hex_to_color),
        }
    }

    pub fn get_weak_pair(&self, text_fallback: Color) -> Option<palette::Pair> {
        match self {
            AppearanceColor::Simple(_) => None,
            AppearanceColor::Complete { weak, text, .. } => {
                weak.map(|color| hex_to_pair(color, *text, text_fallback))
            }
        }
    }

    pub fn get_strong_pair(&self, text_fallback: Color) -> Option<palette::Pair> {
        match self {
            AppearanceColor::Simple(_) => None,
            AppearanceColor::Complete { strong, text, .. } => {
                strong.map(|color| hex_to_pair(color, *text, text_fallback))
            }
        }
    }
}

#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(untagged)]
pub enum BackgroundAppearanceColor {
    Simple(HexColor),
    Complete {
        base: HexColor,
        weakest: Option<HexColor>,
        weaker: Option<HexColor>,
        weak: Option<HexColor>,
        neutral: Option<HexColor>,
        strong: Option<HexColor>,
        stronger: Option<HexColor>,
        strongest: Option<HexColor>,
        text: Option<HexColor>,
    },
}

impl BackgroundAppearanceColor {
    pub fn get_base(&self) -> Color {
        match self {
            BackgroundAppearanceColor::Simple(color) => hex_to_color(*color),
            BackgroundAppearanceColor::Complete { base, .. } => hex_to_color(*base),
        }
    }

    pub fn get_text(&self) -> Option<Color> {
        match self {
            BackgroundAppearanceColor::Simple(_) => None,
            BackgroundAppearanceColor::Complete { text, .. } => text.map(hex_to_color),
        }
    }

    pub fn get_pair(&self, level: BackgroundLevel, text_fallback: Color) -> Option<palette::Pair> {
        match self {
            BackgroundAppearanceColor::Simple(_) => None,
            BackgroundAppearanceColor::Complete {
                weakest,
                weaker,
                weak,
                neutral,
                strong,
                stronger,
                strongest,
                text,
                ..
            } => {
                let hex = match level {
                    BackgroundLevel::Weakest => *weakest,
                    BackgroundLevel::Weaker => *weaker,
                    BackgroundLevel::Weak => *weak,
                    BackgroundLevel::Neutral => *neutral,
                    BackgroundLevel::Strong => *strong,
                    BackgroundLevel::Stronger => *stronger,
                    BackgroundLevel::Strongest => *strongest,
                };
                hex.map(|h| hex_to_pair(h, *text, text_fallback))
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum BackgroundLevel {
    Weakest,
    Weaker,
    Weak,
    Neutral,
    Strong,
    Stronger,
    Strongest,
}

#[derive(Deserialize, Default, Copy, Clone, Eq, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum BarSurface {
    #[default]
    Transparent,
    Solid,
}

#[derive(Deserialize, Default, Copy, Clone, Eq, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum RadiusSize {
    #[default]
    None,
    Sm,
    Md,
    Lg,
    Xl,
}

#[derive(Deserialize, Default, Copy, Clone, Eq, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum SpaceSize {
    #[default]
    None,
    Xxs,
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
    Xxl,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CssShorthand<T> {
    One(T),
    Many(Vec<T>),
}

impl<T: Copy> CssShorthand<T> {
    fn expand<E: serde::de::Error>(self) -> Result<[T; 4], E> {
        let values = match self {
            CssShorthand::One(v) => return Ok([v, v, v, v]),
            CssShorthand::Many(v) => v,
        };
        match values.as_slice() {
            [a] => Ok([*a, *a, *a, *a]),
            [a, b] => Ok([*a, *b, *a, *b]),
            [a, b, c, d] => Ok([*a, *b, *c, *d]),
            _ => Err(E::custom("expected 1, 2 or 4 values (CSS shorthand)")),
        }
    }
}

/// Per-corner radius selection, deserialized with CSS `border-radius` shorthand:
/// 1 value = all corners, 2 = `[top-left+bottom-right, top-right+bottom-left]`,
/// 4 = `[top-left, top-right, bottom-right, bottom-left]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct BarRadius {
    pub top_left: RadiusSize,
    pub top_right: RadiusSize,
    pub bottom_right: RadiusSize,
    pub bottom_left: RadiusSize,
}

impl<'de> Deserialize<'de> for BarRadius {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let [top_left, top_right, bottom_right, bottom_left] =
            CssShorthand::<RadiusSize>::deserialize(deserializer)?.expand()?;
        Ok(Self {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        })
    }
}

/// Per-edge margin selection, deserialized with CSS `margin` shorthand:
/// 1 value = all edges, 2 = `[vertical, horizontal]`, 4 = `[top, right, bottom, left]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct BarMargin {
    pub top: SpaceSize,
    pub right: SpaceSize,
    pub bottom: SpaceSize,
    pub left: SpaceSize,
}

impl<'de> Deserialize<'de> for BarMargin {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let [top, right, bottom, left] =
            CssShorthand::<SpaceSize>::deserialize(deserializer)?.expand()?;
        Ok(Self {
            top,
            right,
            bottom,
            left,
        })
    }
}

#[derive(Deserialize, Default, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct BarAppearance {
    pub surface: BarSurface,
    pub radius: BarRadius,
    pub margin: BarMargin,
}

#[derive(Deserialize, Default, Clone, Copy, Debug)]
#[serde(default)]
pub struct MenuAppearance {
    pub backdrop: f32,
}

/// A layer-shell surface. Each one is drawn with its own theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Bar,
    Menu,
    Osd,
    Notifications,
}

impl Surface {
    pub const ALL: [Surface; 4] = [
        Surface::Bar,
        Surface::Menu,
        Surface::Osd,
        Surface::Notifications,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Opacity {
    default: f32,
    bar: Option<f32>,
    menu: Option<f32>,
    osd: Option<f32>,
    notifications: Option<f32>,
}

impl Opacity {
    pub fn get(&self, surface: Surface) -> f32 {
        match surface {
            Surface::Bar => self.bar,
            Surface::Menu => self.menu,
            Surface::Osd => self.osd,
            Surface::Notifications => self.notifications,
        }
        .unwrap_or(self.default)
    }
}

impl Default for Opacity {
    fn default() -> Self {
        Self {
            default: 1.0,
            bar: None,
            menu: None,
            osd: None,
            notifications: None,
        }
    }
}

impl<'de> Deserialize<'de> for Opacity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OpacityVisitor;

        impl<'de> Visitor<'de> for OpacityVisitor {
            type Value = Opacity;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(
                    "a number between 0.0 and 1.0, or a table with `default` and per-surface overrides",
                )
            }

            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Opacity, E> {
                Ok(Opacity {
                    default: check_opacity(v).map_err(E::custom)?,
                    ..Opacity::default()
                })
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Opacity, E> {
                self.visit_f64(v as f64)
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Opacity, E> {
                self.visit_f64(v as f64)
            }

            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Opacity, M::Error> {
                #[derive(Deserialize)]
                struct Table {
                    default: Option<f64>,
                    bar: Option<f64>,
                    menu: Option<f64>,
                    osd: Option<f64>,
                    notifications: Option<f64>,
                }

                let table = Table::deserialize(de::value::MapAccessDeserializer::new(map))?;
                let check =
                    |v: Option<f64>| v.map(check_opacity).transpose().map_err(de::Error::custom);

                Ok(Opacity {
                    default: check(table.default)?.unwrap_or(1.0),
                    bar: check(table.bar)?,
                    menu: check(table.menu)?,
                    osd: check(table.osd)?,
                    notifications: check(table.notifications)?,
                })
            }
        }

        deserializer.deserialize_any(OpacityVisitor)
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Appearance {
    pub font_name: Option<String>,
    #[serde(deserialize_with = "scale_factor_deserializer")]
    pub scale_factor: f64,
    pub opacity: Opacity,
    pub bar: BarAppearance,
    pub menu: MenuAppearance,
    pub background_color: BackgroundAppearanceColor,
    pub primary_color: AppearanceColor,
    pub success_color: AppearanceColor,
    pub warning_color: AppearanceColor,
    pub danger_color: AppearanceColor,
    pub text_color: AppearanceColor,
    pub workspace_colors: Vec<AppearanceColor>,
    pub special_workspace_colors: Option<Vec<AppearanceColor>>,
    /// Blur the wallpaper behind lumen's translucent surfaces via
    /// `ext-background-effect-v1`. No-op where the protocol is unsupported.
    pub blur: BlurMode,
}

/// When to ask the compositor for background blur.
#[derive(Deserialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BlurMode {
    /// Blur when `opacity < 1`; blurring an opaque surface would not show.
    #[default]
    Auto,
    /// Always ask, even at full opacity.
    Always,
    /// Never ask.
    Never,
}

impl BlurMode {
    pub fn enabled(self, opacity: f32) -> bool {
        match self {
            Self::Auto => opacity < 1.0,
            Self::Always => true,
            Self::Never => false,
        }
    }
}

static PRIMARY: HexColor = HexColor::rgb(122, 162, 247);

fn scale_factor_deserializer<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = f64::deserialize(deserializer)?;

    if v <= 0.0 {
        return Err(serde::de::Error::custom(
            "Scale factor must be greater than 0.0",
        ));
    }

    if v > 2.0 {
        return Err(serde::de::Error::custom(
            "Scale factor cannot be greater than 2.0",
        ));
    }

    Ok(v)
}

fn check_opacity(v: f64) -> Result<f32, &'static str> {
    if v < 0.0 {
        return Err("Opacity cannot be negative");
    }

    if v > 1.0 {
        return Err("Opacity cannot be greater than 1.0");
    }

    Ok(v as f32)
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            font_name: Some("Open Sans".to_string()),
            scale_factor: 1.0,
            opacity: Opacity {
                default: 0.85,
                bar: Some(0.55),
                menu: None,
                osd: None,
                notifications: None,
            },
            bar: BarAppearance {
                surface: BarSurface::Solid,
                radius: BarRadius::default(),
                margin: BarMargin::default(),
            },
            menu: MenuAppearance::default(),
            background_color: BackgroundAppearanceColor::Complete {
                base: HexColor::rgb(26, 27, 38),
                weakest: None,
                weaker: None,
                weak: Some(HexColor::rgb(36, 39, 58)),
                neutral: None,
                strong: Some(HexColor::rgb(65, 72, 104)),
                stronger: None,
                strongest: None,
                text: None,
            },
            primary_color: AppearanceColor::Simple(HexColor::rgb(0x58, 0x9d, 0xf6)),
            success_color: AppearanceColor::Simple(HexColor::rgb(158, 206, 106)),
            warning_color: AppearanceColor::Simple(HexColor::rgb(224, 175, 104)),
            danger_color: AppearanceColor::Simple(HexColor::rgb(247, 118, 142)),
            text_color: AppearanceColor::Simple(HexColor::rgb(0xde, 0xde, 0xde)),
            workspace_colors: vec![
                AppearanceColor::Simple(PRIMARY),
                AppearanceColor::Simple(HexColor::rgb(158, 206, 106)),
            ],
            special_workspace_colors: None,
            blur: BlurMode::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_shorthand_expands_one_value_to_all_four() {
        let shorthand = CssShorthand::One(RadiusSize::Lg);
        let expanded = shorthand.expand::<de::value::Error>().unwrap();
        assert_eq!(
            expanded,
            [
                RadiusSize::Lg,
                RadiusSize::Lg,
                RadiusSize::Lg,
                RadiusSize::Lg
            ]
        );
    }

    #[test]
    fn css_shorthand_expands_two_values_symmetrically() {
        let shorthand = CssShorthand::Many(vec![SpaceSize::Sm, SpaceSize::Md]);
        let expanded = shorthand.expand::<de::value::Error>().unwrap();
        assert_eq!(
            expanded,
            [SpaceSize::Sm, SpaceSize::Md, SpaceSize::Sm, SpaceSize::Md]
        );
    }

    #[test]
    fn css_shorthand_rejects_three_values() {
        let shorthand = CssShorthand::Many(vec![SpaceSize::Sm, SpaceSize::Md, SpaceSize::Lg]);
        assert!(shorthand.expand::<de::value::Error>().is_err());
    }

    #[test]
    fn opacity_rejects_out_of_range_values() {
        assert!(check_opacity(-0.1).is_err());
        assert!(check_opacity(1.1).is_err());
        assert_eq!(check_opacity(0.5), Ok(0.5));
    }

    #[test]
    fn opacity_deserializes_from_bare_number() {
        let opacity = Opacity::deserialize(toml::Value::Float(0.5)).unwrap();
        assert_eq!(opacity.get(Surface::Bar), 0.5);
    }

    #[test]
    fn opacity_deserializes_per_surface_overrides() {
        let table: toml::Table = toml::from_str("default = 0.9\nbar = 0.4").unwrap();
        let opacity = Opacity::deserialize(toml::Value::Table(table)).unwrap();
        assert_eq!(opacity.get(Surface::Bar), 0.4);
        assert_eq!(opacity.get(Surface::Menu), 0.9);
    }

    #[test]
    fn blur_mode_auto_depends_on_opacity() {
        assert!(BlurMode::Auto.enabled(0.5));
        assert!(!BlurMode::Auto.enabled(1.0));
        assert!(BlurMode::Always.enabled(1.0));
        assert!(!BlurMode::Never.enabled(0.5));
    }
}
