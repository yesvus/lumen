use crate::i18n::{UnitSystem, unit_system};
use log::warn;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct TempoModuleConfig {
    pub clock_format: String,
    #[serde(default)]
    pub formats: Vec<String>,
    #[serde(default)]
    pub timezones: Vec<String>,
    #[serde(default)]
    pub weather_location: Option<WeatherLocation>,
    pub weather_indicator: WeatherIndicator,
    #[serde(default)]
    pub wind_speed_unit: Option<WindSpeedUnit>,
}

impl TempoModuleConfig {
    pub fn resolved_wind_speed_unit(&self) -> WindSpeedUnit {
        match self.wind_speed_unit {
            Some(unit) => unit,
            None => match unit_system() {
                UnitSystem::Imperial => WindSpeedUnit::Mph,
                UnitSystem::Metric => WindSpeedUnit::Kmh,
            },
        }
    }

    pub(super) fn validate(&mut self) {
        if let Some(WeatherLocation::Coordinates(lat, lon)) = &mut self.weather_location {
            let clamped_lat = lat.clamp(-90.0, 90.0);
            let clamped_lon = lon.clamp(-180.0, 180.0);
            if *lat != clamped_lat || *lon != clamped_lon {
                warn!(
                    "tempo.weather_location latitude ({lat}) must be in -90..=90 and longitude ({lon}) in -180..=180, setting to ({clamped_lat}, {clamped_lon})"
                );
                *lat = clamped_lat;
                *lon = clamped_lon;
            }
        }
    }
}

#[derive(Deserialize, Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindSpeedUnit {
    #[default]
    Kmh,
    Mph,
    Ms,
}

impl WindSpeedUnit {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Kmh => "km/h",
            Self::Mph => "mph",
            Self::Ms => "m/s",
        }
    }

    pub fn api_param(self) -> &'static str {
        match self {
            Self::Kmh => "kmh",
            Self::Mph => "mph",
            Self::Ms => "ms",
        }
    }
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq, Eq)]
pub enum WeatherIndicator {
    #[default]
    IconAndTemperature,
    Icon,
    None,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
pub enum WeatherLocation {
    #[default]
    Current,
    City(String),
    Coordinates(f32, f32),
}

impl std::hash::Hash for WeatherLocation {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            WeatherLocation::Current => {}
            WeatherLocation::City(city) => city.hash(state),
            WeatherLocation::Coordinates(lat, lon) => {
                lat.to_bits().hash(state);
                lon.to_bits().hash(state);
            }
        }
    }
}

impl Default for TempoModuleConfig {
    fn default() -> Self {
        Self {
            clock_format: "%a %d %b %R".to_string(),
            formats: vec![],
            timezones: vec![],
            weather_location: None,
            weather_indicator: WeatherIndicator::IconAndTemperature,
            wind_speed_unit: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_clamps_out_of_range_coordinates() {
        let mut config = TempoModuleConfig {
            weather_location: Some(WeatherLocation::Coordinates(120.0, -200.0)),
            ..TempoModuleConfig::default()
        };
        config.validate();
        assert_eq!(
            config.weather_location,
            Some(WeatherLocation::Coordinates(90.0, -180.0))
        );
    }

    #[test]
    fn validate_leaves_in_range_coordinates_untouched() {
        let mut config = TempoModuleConfig {
            weather_location: Some(WeatherLocation::Coordinates(45.0, -122.0)),
            ..TempoModuleConfig::default()
        };
        config.validate();
        assert_eq!(
            config.weather_location,
            Some(WeatherLocation::Coordinates(45.0, -122.0))
        );
    }
}
