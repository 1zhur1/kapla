//! Погода на Главную: запрос wttr.in без API-ключей, кэш 8 минут и ручной выбор города.

use crate::log;
use parking_lot::Mutex;
use serde::Serialize;
use std::process::Command;
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, Debug, Serialize)]
pub struct WeatherInfo {
    pub city: String,
    pub temp_c: i32,
    pub feels_like_c: i32,
    pub desc: String,
    pub icon: String,
    pub humidity: u8,
    pub wind_kmph: u32,
    pub min_c: Option<i32>,
    pub max_c: Option<i32>,
}

static CACHE: Mutex<Option<(String, Instant, WeatherInfo)>> = Mutex::new(None);

fn encode_city(city: &str) -> String {
    let mut out = String::new();
    for b in city.trim().as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => out.push(*b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn code_to_icon_ru(code: u32) -> (&'static str, &'static str) {
    match code {
        113 => ("sun", "Ясно"),
        116 => ("partly", "Малооблачно"),
        119 | 122 => ("cloud", "Облачно"),
        143 | 248 | 260 => ("fog", "Туман"),
        200 | 386 | 389 | 392 | 395 => ("storm", "Гроза"),
        179 | 182 | 185 | 227 | 230 | 320..=338 | 368..=377 => ("snow", "Снег"),
        _ => ("rain", "Дождь"),
    }
}

pub fn parse_wttr_json(raw: &str, city_override: Option<&str>) -> Result<WeatherInfo, String> {
    let v: serde_json::Value =
        serde_json::from_str(raw.trim()).map_err(|e| format!("Ошибка ответа погоды: {e}"))?;
    let cc = v["current_condition"]
        .get(0)
        .ok_or("Пустой ответ погоды")?;

    let temp_c = cc["temp_C"]
        .as_str()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(0);
    let feels_like_c = cc["FeelsLikeC"]
        .as_str()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(temp_c);
    let humidity = cc["humidity"]
        .as_str()
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(0);
    let wind_kmph = cc["windspeedKmph"]
        .as_str()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    let code = cc["weatherCode"]
        .as_str()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(116);

    let (icon, ru_fallback) = code_to_icon_ru(code);
    let raw_desc = cc["lang_ru"][0]["value"]
        .as_str()
        .or_else(|| cc["weatherDesc"][0]["value"].as_str())
        .unwrap_or("")
        .trim();
    // wttr.in иногда отдаёт lang_ru на английском — тогда используем лаконичное русское описание по коду.
    let has_cyrillic = raw_desc.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c));
    let desc = if !raw_desc.is_empty() && has_cyrillic {
        raw_desc.to_string()
    } else {
        ru_fallback.to_string()
    };

    let area = v["nearest_area"][0]["areaName"][0]["value"]
        .as_str()
        .unwrap_or("")
        .trim();
    let city = match city_override.map(str::trim).filter(|s| !s.is_empty()) {
        Some(c) if !area.is_empty() => format!("{c}"),
        Some(c) => c.to_string(),
        None if !area.is_empty() => area.to_string(),
        None => "Местная".to_string(),
    };

    let day = &v["weather"][0];
    let min_c = day["mintempC"].as_str().and_then(|s| s.parse::<i32>().ok());
    let max_c = day["maxtempC"].as_str().and_then(|s| s.parse::<i32>().ok());

    Ok(WeatherInfo {
        city,
        temp_c,
        feels_like_c,
        desc,
        icon: icon.into(),
        humidity,
        wind_kmph,
        min_c,
        max_c,
    })
}

#[tauri::command]
pub async fn weather_now(city: Option<String>, force: Option<bool>) -> Result<WeatherInfo, String> {
    let clean = city.unwrap_or_default().trim().to_string();
    let key = clean.to_lowercase();
    if !force.unwrap_or(false) {
        if let Some((k, at, info)) = &*CACHE.lock() {
            if k == &key && at.elapsed() < Duration::from_secs(480) {
                return Ok(info.clone());
            }
        }
    }

    tauri::async_runtime::spawn_blocking(move || {
        let url = if clean.is_empty() {
            "https://wttr.in/?format=j1&lang=ru".to_string()
        } else {
            format!("https://wttr.in/{}?format=j1&lang=ru", encode_city(&clean))
        };
        let mut cmd = Command::new("curl.exe");
        cmd.args(["-sL", "--max-time", "6", &url]);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let out = cmd.output().map_err(|e| format!("curl: {e}"))?;
        if !out.status.success() || out.stdout.is_empty() {
            return Err("Не удалось получить погоду".into());
        }
        let body = String::from_utf8_lossy(&out.stdout);
        let info = parse_wttr_json(&body, if clean.is_empty() { None } else { Some(&clean) })?;
        *CACHE.lock() = Some((key, Instant::now(), info.clone()));
        log::line(format!("weather updated: {} {}°C", info.city, info.temp_c));
        Ok(info)
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wttr_weather_payload() {
        let sample = r#"{
          "current_condition": [{
            "temp_C": "9",
            "FeelsLikeC": "6",
            "humidity": "59",
            "windspeedKmph": "19",
            "weatherCode": "176",
            "lang_ru": [{"value": "Patchy rain nearby"}]
          }],
          "nearest_area": [{"areaName": [{"value": "Moscow"}]}],
          "weather": [{"mintempC": "3", "maxtempC": "11"}]
        }"#;
        let info = parse_wttr_json(sample, None).unwrap();
        assert_eq!(info.city, "Moscow");
        assert_eq!(info.temp_c, 9);
        assert_eq!(info.feels_like_c, 6);
        assert_eq!(info.icon, "rain");
        assert_eq!(info.desc, "Дождь");
        assert_eq!(info.min_c, Some(3));
        assert_eq!(info.max_c, Some(11));
    }

    #[test]
    fn encodes_city_names_and_handles_missing_min_max() {
        assert_eq!(encode_city(" New York "), "New+York");
        assert!(encode_city("Санкт-Петербург").contains("%D0%A1"));

        let sample = r#"{
          "current_condition": [{
            "temp_C": "-4",
            "FeelsLikeC": "-9",
            "humidity": "80",
            "windspeedKmph": "12",
            "weatherCode": "338",
            "lang_ru": [{"value": "Сильный снег"}]
          }],
          "nearest_area": []
        }"#;
        let info = parse_wttr_json(sample, Some("Казань")).unwrap();
        assert_eq!(info.city, "Казань");
        assert_eq!(info.desc, "Сильный снег");
        assert_eq!(info.icon, "snow");
        assert_eq!(info.min_c, None);
        assert_eq!(info.max_c, None);
    }
}
