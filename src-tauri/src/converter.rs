//! Встроенный быстрый конвертер файлов для Полки (изображения, JSON/CSV/Markdown/HTML, ZIP, Base64, опционально ffmpeg).

use crate::log;
use crate::tools::ShelfItem;
use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageFormat};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Instant;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, Serialize)]
pub struct ConvertTarget {
    pub id: String,
    pub label: String,
    pub hint: String,
    pub supports_quality: bool,
    pub supports_resize: bool,
    pub via_canvas: bool,
}

#[derive(Clone, Serialize)]
pub struct ConvertProbe {
    pub path: String,
    pub name: String,
    pub ext: String,
    pub size: u64,
    pub kind: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub has_ffmpeg: bool,
    pub targets: Vec<ConvertTarget>,
}

#[derive(Clone, Serialize)]
pub struct ConvertResult {
    pub item: ShelfItem,
    pub src_size: u64,
    pub out_size: u64,
    pub ms: u64,
}

fn has_ffmpeg() -> bool {
    static FFMPEG: OnceLock<bool> = OnceLock::new();
    *FFMPEG.get_or_init(|| {
        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-version");
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.output().map(|o| o.status.success()).unwrap_or(false)
    })
}

fn tgt(id: &str, label: &str, hint: &str, q: bool, r: bool, via_canvas: bool) -> ConvertTarget {
    ConvertTarget {
        id: id.into(),
        label: label.into(),
        hint: hint.into(),
        supports_quality: q,
        supports_resize: r,
        via_canvas,
    }
}

fn is_raster_ext(ext: &str) -> bool {
    matches!(
        ext,
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "ico" | "tif" | "tiff" | "tga" | "svg" | "gif"
    )
}

fn is_archive_ext(ext: &str) -> bool {
    matches!(ext, "zip" | "tar" | "tgz" | "gz" | "7z" | "rar")
}

fn is_media_ext(ext: &str) -> bool {
    matches!(
        ext,
        "mp4" | "mkv" | "mov" | "avi" | "webm" | "wmv" | "mp3" | "wav" | "ogg" | "flac" | "m4a"
    )
}

#[tauri::command]
pub fn convert_probe(path: String) -> Result<ConvertProbe, String> {
    let p = Path::new(&path);
    let meta = fs::metadata(p).map_err(|e| format!("Файл недоступен: {e}"))?;
    let name = p
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.clone());
    let ext = p
        .extension()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let lower_name = name.to_ascii_lowercase();
    let ffmpeg_ok = has_ffmpeg();

    let (width, height) = if matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "bmp" | "ico" | "tif" | "tiff" | "tga"
    ) {
        image::image_dimensions(p)
            .map(|(w, h)| (Some(w), Some(h)))
            .unwrap_or((None, None))
    } else {
        (None, None)
    };

    let mut targets = Vec::new();
    let kind;

    if meta.is_dir() {
        kind = "folder";
        targets.push(tgt("zip", "ZIP", "Упаковать папку в архив", false, false, false));
    } else if is_raster_ext(&ext) {
        kind = "image";
        let can_rust_decode = matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "bmp" | "ico" | "tif" | "tiff" | "tga"
        );
        let canvas_decode = !can_rust_decode; // webp, svg, gif -> через WebView canvas

        if ext != "webp" {
            targets.push(tgt("webp", "WEBP", "Лёгкий веб-формат с прозрачностью", true, true, true));
        }
        if ext != "png" {
            targets.push(tgt("png", "PNG", "Без потерь + прозрачность", false, true, canvas_decode));
        }
        if !matches!(ext.as_str(), "jpg" | "jpeg") {
            targets.push(tgt("jpg", "JPG", "Компактное фото с настройкой качества", true, true, canvas_decode));
        }
        if ext != "ico" {
            targets.push(tgt("ico", "ICO", "Иконка Windows (до 256×256)", false, false, canvas_decode));
        }
        if ext != "bmp" && !canvas_decode {
            targets.push(tgt("bmp", "BMP", "Без сжатия", false, true, false));
        }
        if ext != "tiff" && ext != "tif" && !canvas_decode {
            targets.push(tgt("tiff", "TIFF", "Полиграфия и архив", false, true, false));
        }
        targets.push(tgt("zip", "ZIP", "Сжать в архив", false, false, false));
    } else if ext == "json" {
        kind = "json";
        targets.push(tgt("csv", "CSV", "Таблица для Excel / Sheets", false, false, false));
        targets.push(tgt("md", "Markdown", "Аккуратная MD-таблица", false, false, false));
        targets.push(tgt("html", "HTML", "Красивая веб-таблица отчёт", false, false, false));
        targets.push(tgt("pretty", "Pretty JSON", "Красивое форматирование (2 пробела)", false, false, false));
        targets.push(tgt("min", "Min JSON", "Сжать в одну строку", false, false, false));
        targets.push(tgt("zip", "ZIP", "Архив", false, false, false));
    } else if matches!(ext.as_str(), "csv" | "tsv") {
        kind = "csv";
        targets.push(tgt("json", "JSON", "Структурированный массив объектов", false, false, false));
        targets.push(tgt("md", "Markdown", "Таблица для заметок и GitHub", false, false, false));
        targets.push(tgt("html", "HTML", "Тёмная стилизованная таблица", false, false, false));
        targets.push(tgt("zip", "ZIP", "Архив", false, false, false));
    } else if matches!(ext.as_str(), "md" | "markdown") {
        kind = "markdown";
        targets.push(tgt("html", "HTML", "Готовая оформленная страница", false, false, false));
        targets.push(tgt("txt", "TXT", "Чистый текст без разметки", false, false, false));
        targets.push(tgt("zip", "ZIP", "Архив", false, false, false));
    } else if lower_name.ends_with(".b64.txt") || ext == "b64" {
        kind = "b64";
        targets.push(tgt("unb64", "Декодировать", "Восстановить исходный файл из Base64", false, false, false));
        targets.push(tgt("zip", "ZIP", "Архив", false, false, false));
    } else if matches!(
        ext.as_str(),
        "txt" | "log" | "ini" | "conf" | "yml" | "yaml" | "xml" | "toml"
    ) {
        kind = "text";
        targets.push(tgt("html", "HTML", "Читаемый документ", false, false, false));
        targets.push(tgt("json", "JSON", "Построчный массив строк", false, false, false));
        targets.push(tgt("b64", "Base64", "Текстовое Base64-представление", false, false, false));
        targets.push(tgt("zip", "ZIP", "Архив", false, false, false));
    } else if is_archive_ext(&ext) {
        kind = "archive";
        targets.push(tgt("extract", "Распаковать", "Извлечь содержимое в папку рядом", false, false, false));
        targets.push(tgt("b64", "Base64", "Закодировать в Base64", false, false, false));
    } else if is_media_ext(&ext) {
        kind = "media";
        if ffmpeg_ok {
            if ext != "mp3" {
                targets.push(tgt("mp3", "MP3", "Извлечь / перекодировать аудио", false, false, false));
            }
            if ext != "wav" {
                targets.push(tgt("wav", "WAV", "Аудио без сжатия", false, false, false));
            }
            if ext != "mp4" && matches!(ext.as_str(), "mkv" | "mov" | "avi" | "webm" | "wmv") {
                targets.push(tgt("mp4", "MP4", "Универсальное видео H.264", false, false, false));
            }
            if matches!(ext.as_str(), "mp4" | "mov" | "mkv" | "webm" | "avi") {
                targets.push(tgt("gif", "GIF", "Анимация 12 FPS", false, false, false));
            }
        }
        targets.push(tgt("zip", "ZIP", "Сжать в архив", false, false, false));
    } else {
        kind = "file";
        targets.push(tgt("zip", "ZIP", "Упаковать в ZIP-архив", false, false, false));
        targets.push(tgt("b64", "Base64", "Закодировать в Base64 (.b64.txt)", false, false, false));
    }

    Ok(ConvertProbe {
        path,
        name,
        ext,
        size: meta.len(),
        kind: kind.into(),
        width,
        height,
        has_ffmpeg: ffmpeg_ok,
        targets,
    })
}

pub fn unique_out_path(src: &Path, stem_suffix: &str, new_ext: &str) -> PathBuf {
    let parent = src.parent().unwrap_or_else(|| Path::new("."));
    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let base_stem = if stem_suffix.is_empty() {
        stem
    } else {
        format!("{stem}_{stem_suffix}")
    };
    let clean_ext = new_ext.trim_start_matches('.');
    let candidate = if clean_ext.is_empty() {
        parent.join(&base_stem)
    } else {
        parent.join(format!("{base_stem}.{clean_ext}"))
    };
    if !candidate.exists() {
        return candidate;
    }
    for i in 2..1000 {
        let next = if clean_ext.is_empty() {
            parent.join(format!("{base_stem} ({i})"))
        } else {
            parent.join(format!("{base_stem} ({i}).{clean_ext}"))
        };
        if !next.exists() {
            return next;
        }
    }
    candidate
}

fn shelf_item_for(p: &Path) -> ShelfItem {
    let meta = fs::metadata(p).ok();
    let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
    ShelfItem {
        path: p.to_string_lossy().into_owned(),
        name: p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        size: if is_dir {
            0
        } else {
            meta.map(|m| m.len()).unwrap_or(0)
        },
        is_dir,
    }
}

fn convert_image(
    src: &Path,
    target: &str,
    quality: u8,
    max_side: Option<u32>,
) -> Result<PathBuf, String> {
    let mut img: DynamicImage =
        image::open(src).map_err(|e| format!("Не удалось прочитать изображение: {e}"))?;

    if target == "ico" {
        let (w, h) = img.dimensions();
        let edge = max_side.unwrap_or(256).clamp(16, 256);
        if w > edge || h > edge {
            img = img.resize(edge, edge, FilterType::Lanczos3);
        }
        // Квадратный холст с прозрачными полями для аккуратной иконки
        let (rw, rh) = img.dimensions();
        if rw != rh {
            let side = rw.max(rh);
            let mut canvas = image::RgbaImage::new(side, side);
            let ox = (side - rw) / 2;
            let oy = (side - rh) / 2;
            image::imageops::overlay(&mut canvas, &img.to_rgba8(), ox as i64, oy as i64);
            img = DynamicImage::ImageRgba8(canvas);
        }
    } else if let Some(side) = max_side.filter(|&s| s >= 16) {
        let (w, h) = img.dimensions();
        if w > side || h > side {
            img = img.resize(side, side, FilterType::Lanczos3);
        }
    }

    let out = unique_out_path(src, "", target);
    match target {
        "jpg" | "jpeg" => {
            let rgb = img.to_rgb8();
            let mut file = fs::File::create(&out).map_err(|e| e.to_string())?;
            let q = quality.clamp(10, 100);
            let mut enc = JpegEncoder::new_with_quality(&mut file, q);
            enc.encode_image(&rgb).map_err(|e| e.to_string())?;
        }
        "png" => img
            .save_with_format(&out, ImageFormat::Png)
            .map_err(|e| e.to_string())?,
        "bmp" => img
            .save_with_format(&out, ImageFormat::Bmp)
            .map_err(|e| e.to_string())?,
        "ico" => img
            .save_with_format(&out, ImageFormat::Ico)
            .map_err(|e| e.to_string())?,
        "tiff" | "tif" => img
            .save_with_format(&out, ImageFormat::Tiff)
            .map_err(|e| e.to_string())?,
        _ => return Err(format!("Неподдерживаемый формат: {target}")),
    }
    Ok(out)
}

fn parse_csv_line(line: &str, delim: char) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cur.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == delim {
            fields.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    fields.push(cur);
    fields
}

fn detect_delim(header: &str) -> char {
    let tabs = header.matches('\t').count();
    let semis = header.matches(';').count();
    let commas = header.matches(',').count();
    if tabs >= semis && tabs >= commas && tabs > 0 {
        '\t'
    } else if semis > commas {
        ';'
    } else {
        ','
    }
}

pub fn csv_to_table(raw: &str) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let text = raw.trim_start_matches('\u{feff}');
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let first = lines.next().ok_or("CSV пуст")?;
    let delim = detect_delim(first);
    let headers = parse_csv_line(first, delim);
    let rows: Vec<Vec<String>> = lines.map(|l| parse_csv_line(l, delim)).collect();
    Ok((headers, rows))
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn val_cell(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

pub fn json_to_table(raw: &str) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let val: serde_json::Value =
        serde_json::from_str(raw.trim_start_matches('\u{feff}')).map_err(|e| format!("JSON ошибка: {e}"))?;
    match val {
        serde_json::Value::Array(arr) => {
            if arr.is_empty() {
                return Ok((vec!["value".into()], vec![]));
            }
            if arr.iter().all(|v| v.is_object()) {
                let mut cols = Vec::new();
                for item in &arr {
                    if let Some(obj) = item.as_object() {
                        for k in obj.keys() {
                            if !cols.contains(k) {
                                cols.push(k.clone());
                            }
                        }
                    }
                }
                let rows = arr
                    .iter()
                    .map(|item| {
                        cols.iter()
                            .map(|c| item.get(c).map(val_cell).unwrap_or_default())
                            .collect()
                    })
                    .collect();
                Ok((cols, rows))
            } else {
                let rows = arr.iter().map(|v| vec![val_cell(v)]).collect();
                Ok((vec!["value".into()], rows))
            }
        }
        serde_json::Value::Object(obj) => {
            let rows = obj.iter().map(|(k, v)| vec![k.clone(), val_cell(v)]).collect();
            Ok((vec!["key".into(), "value".into()], rows))
        }
        other => Ok((vec!["value".into()], vec![vec![val_cell(&other)]])),
    }
}

fn table_to_markdown(headers: &[String], rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    let esc = |s: &str| s.replace('|', "\\|").replace('\n', " ");
    out.push('|');
    for h in headers {
        out.push(' ');
        out.push_str(&esc(h));
        out.push_str(" |");
    }
    out.push('\n');
    out.push('|');
    for _ in headers {
        out.push_str(" --- |");
    }
    out.push('\n');
    for row in rows {
        out.push('|');
        for i in 0..headers.len() {
            let cell = row.get(i).map(|s| s.as_str()).unwrap_or("");
            out.push(' ');
            out.push_str(&esc(cell));
            out.push_str(" |");
        }
        out.push('\n');
    }
    out
}

fn html_esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn wrap_html(title: &str, body: &str) -> String {
    format!(
        "<!doctype html><html lang=\"ru\"><head><meta charset=\"utf-8\"><title>{}</title>\
        <style>body{{background:#0e0f12;color:#ece7de;font:14px/1.6 system-ui,sans-serif;max-width:980px;margin:32px auto;padding:0 20px}}\
        table{{border-collapse:collapse;width:100%;margin:16px 0;background:#16181e;border-radius:10px;overflow:hidden}}\
        th,td{{padding:9px 12px;border-bottom:1px solid #262933;text-align:left}}th{{background:#1e212a;color:#ff944d}}\
        pre,code{{background:#181b22;border-radius:6px;font-family:Consolas,monospace}}pre{{padding:14px;overflow:auto}}\
        code{{padding:2px 5px}}blockquote{{border-left:3px solid #ff7a3d;margin:12px 0;padding:4px 14px;color:#b8b2a6}}\
        a{{color:#ff944d}}h1,h2,h3{{color:#fff6ea}}</style></head><body>{}</body></html>",
        html_esc(title),
        body
    )
}

fn table_to_html(title: &str, headers: &[String], rows: &[Vec<String>]) -> String {
    let mut b = format!("<h2>{}</h2><table><thead><tr>", html_esc(title));
    for h in headers {
        b.push_str(&format!("<th>{}</th>", html_esc(h)));
    }
    b.push_str("</tr></thead><tbody>");
    for row in rows {
        b.push_str("<tr>");
        for i in 0..headers.len() {
            let cell = row.get(i).map(|s| s.as_str()).unwrap_or("");
            b.push_str(&format!("<td>{}</td>", html_esc(cell)));
        }
        b.push_str("</tr>");
    }
    b.push_str("</tbody></table>");
    wrap_html(title, &b)
}

pub fn markdown_to_html(title: &str, md: &str) -> String {
    let mut body = String::new();
    let mut in_code = false;
    let mut in_list = false;
    for line in md.lines() {
        let t = line.trim_end();
        if t.starts_with("```") {
            if in_list {
                body.push_str("</ul>");
                in_list = false;
            }
            if in_code {
                body.push_str("</code></pre>");
                in_code = false;
            } else {
                body.push_str("<pre><code>");
                in_code = true;
            }
            continue;
        }
        if in_code {
            body.push_str(&html_esc(line));
            body.push('\n');
            continue;
        }
        if t.starts_with("- ") || t.starts_with("* ") {
            if !in_list {
                body.push_str("<ul>");
                in_list = true;
            }
            body.push_str(&format!("<li>{}</li>", html_esc(&t[2..])));
            continue;
        }
        if in_list {
            body.push_str("</ul>");
            in_list = false;
        }
        if let Some(rest) = t.strip_prefix("### ") {
            body.push_str(&format!("<h3>{}</h3>", html_esc(rest)));
        } else if let Some(rest) = t.strip_prefix("## ") {
            body.push_str(&format!("<h2>{}</h2>", html_esc(rest)));
        } else if let Some(rest) = t.strip_prefix("# ") {
            body.push_str(&format!("<h1>{}</h1>", html_esc(rest)));
        } else if let Some(rest) = t.strip_prefix("> ") {
            body.push_str(&format!("<blockquote>{}</blockquote>", html_esc(rest)));
        } else if t.is_empty() {
            body.push_str("<br>");
        } else {
            body.push_str(&format!("<p>{}</p>", html_esc(t)));
        }
    }
    if in_list {
        body.push_str("</ul>");
    }
    if in_code {
        body.push_str("</code></pre>");
    }
    wrap_html(title, &body)
}

fn parse_scalar(s: &str) -> serde_json::Value {
    let t = s.trim();
    if t.eq_ignore_ascii_case("true") {
        serde_json::Value::Bool(true)
    } else if t.eq_ignore_ascii_case("false") {
        serde_json::Value::Bool(false)
    } else if let Ok(n) = t.parse::<i64>() {
        serde_json::Value::Number(n.into())
    } else if let Ok(f) = t.parse::<f64>() {
        serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| serde_json::Value::String(s.to_string()))
    } else {
        serde_json::Value::String(s.to_string())
    }
}

fn zip_single_or_many(paths: &[PathBuf], out: &Path) -> Result<(), String> {
    if paths.is_empty() {
        return Err("Нет файлов для архива".into());
    }
    // Windows 10/11 tar.exe -a -cf <out.zip> <names...>
    let first = &paths[0];
    let common_parent = first.parent().unwrap_or_else(|| Path::new("."));
    let mut cmd = Command::new("tar.exe");
    cmd.arg("-a").arg("-cf").arg(out);
    let all_same_dir = paths.iter().all(|p| p.parent() == Some(common_parent));
    if all_same_dir {
        cmd.arg("-C").arg(common_parent);
        for p in paths {
            if let Some(name) = p.file_name() {
                cmd.arg(name);
            }
        }
    } else {
        for p in paths {
            if let (Some(par), Some(name)) = (p.parent(), p.file_name()) {
                cmd.arg("-C").arg(par).arg(name);
            }
        }
    }
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out_cmd = cmd.output().map_err(|e| format!("tar.exe: {e}"))?;
    if !out_cmd.status.success() {
        return Err(format!(
            "Ошибка архивации: {}",
            String::from_utf8_lossy(&out_cmd.stderr)
        ));
    }
    Ok(())
}

fn convert_data_or_archive(src: &Path, ext: &str, target: &str) -> Result<PathBuf, String> {
    let title = src
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Kapla".into());

    match (ext, target) {
        (_, "zip") => {
            let out = unique_out_path(src, "", "zip");
            zip_single_or_many(&[src.to_path_buf()], &out)?;
            Ok(out)
        }
        (_, "extract") => {
            let stem = src
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "extracted".into());
            let dir = unique_out_path(src, "", "");
            let dir = if dir == src {
                src.with_file_name(format!("{stem}_unpacked"))
            } else {
                dir
            };
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let mut cmd = Command::new("tar.exe");
            cmd.arg("-xf").arg(src).arg("-C").arg(&dir);
            #[cfg(windows)]
            cmd.creation_flags(CREATE_NO_WINDOW);
            let res = cmd.output().map_err(|e| format!("tar.exe: {e}"))?;
            if !res.status.success() {
                return Err(format!(
                    "Не удалось распаковать архив: {}",
                    String::from_utf8_lossy(&res.stderr)
                ));
            }
            Ok(dir)
        }
        (_, "b64") => {
            let bytes = fs::read(src).map_err(|e| e.to_string())?;
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
            let out = unique_out_path(src, "", "b64.txt");
            fs::write(&out, encoded).map_err(|e| e.to_string())?;
            Ok(out)
        }
        (_, "unb64") => {
            let txt = fs::read_to_string(src).map_err(|e| e.to_string())?;
            let clean: String = txt.chars().filter(|c| !c.is_whitespace()).collect();
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(clean)
                .map_err(|e| format!("Некорректный Base64: {e}"))?;
            let orig_name = title
                .strip_suffix(".b64.txt")
                .or_else(|| title.strip_suffix(".b64"))
                .unwrap_or("decoded.bin");
            let base = src.with_file_name(orig_name);
            let out = if base.exists() {
                unique_out_path(
                    &base,
                    "decoded",
                    base.extension()
                        .map(|e| e.to_string_lossy().into_owned())
                        .unwrap_or_default()
                        .as_str(),
                )
            } else {
                base
            };
            fs::write(&out, decoded).map_err(|e| e.to_string())?;
            Ok(out)
        }
        ("json", "csv" | "md" | "html") => {
            let raw = fs::read_to_string(src).map_err(|e| e.to_string())?;
            let (cols, rows) = json_to_table(&raw)?;
            let out = unique_out_path(src, "", target);
            let content = match target {
                "csv" => {
                    let mut s = String::from("\u{feff}");
                    s.push_str(
                        &cols
                            .iter()
                            .map(|c| csv_escape(c))
                            .collect::<Vec<_>>()
                            .join(","),
                    );
                    s.push('\n');
                    for r in &rows {
                        let line = (0..cols.len())
                            .map(|i| csv_escape(r.get(i).map(|v| v.as_str()).unwrap_or("")))
                            .collect::<Vec<_>>()
                            .join(",");
                        s.push_str(&line);
                        s.push('\n');
                    }
                    s
                }
                "md" => table_to_markdown(&cols, &rows),
                _ => table_to_html(&title, &cols, &rows),
            };
            fs::write(&out, content).map_err(|e| e.to_string())?;
            Ok(out)
        }
        ("json", "pretty" | "min") => {
            let raw = fs::read_to_string(src).map_err(|e| e.to_string())?;
            let v: serde_json::Value =
                serde_json::from_str(raw.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
            let out = unique_out_path(src, target, "json");
            let formatted = if target == "pretty" {
                serde_json::to_string_pretty(&v)
            } else {
                serde_json::to_string(&v)
            }
            .map_err(|e| e.to_string())?;
            fs::write(&out, formatted).map_err(|e| e.to_string())?;
            Ok(out)
        }
        ("csv" | "tsv", "json" | "md" | "html") => {
            let raw = fs::read_to_string(src).map_err(|e| e.to_string())?;
            let (cols, rows) = csv_to_table(&raw)?;
            let out = unique_out_path(src, "", target);
            let content = match target {
                "json" => {
                    let objs: Vec<serde_json::Value> = rows
                        .iter()
                        .map(|r| {
                            let mut map = serde_json::Map::new();
                            for (i, c) in cols.iter().enumerate() {
                                let v = r.get(i).map(|s| s.as_str()).unwrap_or("");
                                map.insert(c.clone(), parse_scalar(v));
                            }
                            serde_json::Value::Object(map)
                        })
                        .collect();
                    serde_json::to_string_pretty(&objs).map_err(|e| e.to_string())?
                }
                "md" => table_to_markdown(&cols, &rows),
                _ => table_to_html(&title, &cols, &rows),
            };
            fs::write(&out, content).map_err(|e| e.to_string())?;
            Ok(out)
        }
        ("md" | "markdown", "html") => {
            let raw = fs::read_to_string(src).map_err(|e| e.to_string())?;
            let out = unique_out_path(src, "", "html");
            fs::write(&out, markdown_to_html(&title, &raw)).map_err(|e| e.to_string())?;
            Ok(out)
        }
        ("md" | "markdown", "txt") => {
            let raw = fs::read_to_string(src).map_err(|e| e.to_string())?;
            let clean: String = raw
                .lines()
                .map(|l| {
                    l.trim_start_matches('#')
                        .trim_start_matches('>')
                        .trim_start()
                })
                .collect::<Vec<_>>()
                .join("\n");
            let out = unique_out_path(src, "", "txt");
            fs::write(&out, clean).map_err(|e| e.to_string())?;
            Ok(out)
        }
        (_, "html") => {
            let raw = fs::read_to_string(src).map_err(|e| e.to_string())?;
            let out = unique_out_path(src, "", "html");
            let body = format!("<h3>{}</h3><pre>{}</pre>", html_esc(&title), html_esc(&raw));
            fs::write(&out, wrap_html(&title, &body)).map_err(|e| e.to_string())?;
            Ok(out)
        }
        (_, "json") => {
            let raw = fs::read_to_string(src).map_err(|e| e.to_string())?;
            let lines: Vec<&str> = raw.lines().collect();
            let obj = serde_json::json!({ "file": title, "lines": lines });
            let out = unique_out_path(src, "", "json");
            let s = serde_json::to_string_pretty(&obj).map_err(|e| e.to_string())?;
            fs::write(&out, s).map_err(|e| e.to_string())?;
            Ok(out)
        }
        _ if is_media_ext(ext) && has_ffmpeg() => {
            let out = unique_out_path(src, "", target);
            let mut cmd = Command::new("ffmpeg");
            cmd.arg("-y").arg("-i").arg(src);
            if target == "gif" {
                cmd.args(["-vf", "fps=12,scale=640:-1:flags=lanczos"]);
            }
            cmd.arg(&out);
            #[cfg(windows)]
            cmd.creation_flags(CREATE_NO_WINDOW);
            let res = cmd.output().map_err(|e| format!("ffmpeg: {e}"))?;
            if !res.status.success() {
                return Err("ffmpeg не смог конвертировать файл".into());
            }
            Ok(out)
        }
        _ => Err(format!("Конвертация {ext} → {target} не поддерживается")),
    }
}

#[tauri::command]
pub async fn convert_file(
    path: String,
    target: String,
    quality: Option<u8>,
    max_side: Option<u32>,
) -> Result<ConvertResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let t0 = Instant::now();
        let src = PathBuf::from(&path);
        let src_meta = fs::metadata(&src).map_err(|e| format!("Файл недоступен: {e}"))?;
        let src_size = src_meta.len();
        let ext = src
            .extension()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();

        let is_rust_img = matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "bmp" | "ico" | "tif" | "tiff" | "tga"
        ) && matches!(
            target.as_str(),
            "png" | "jpg" | "jpeg" | "bmp" | "ico" | "tiff" | "tif"
        );

        let out = if is_rust_img {
            convert_image(&src, &target, quality.unwrap_or(90), max_side)?
        } else {
            convert_data_or_archive(&src, &ext, &target)?
        };

        let out_size = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        let ms = t0.elapsed().as_millis() as u64;
        log::line(format!(
            "convert {} -> {} ({src_size}B -> {out_size}B, {ms}ms)",
            src.display(),
            out.display()
        ));
        Ok(ConvertResult {
            item: shelf_item_for(&out),
            src_size,
            out_size,
            ms,
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// Читает исходный файл для браузерной WebView2-конвертации (`webp`/`svg`/`gif` через Canvas).
#[tauri::command]
pub fn convert_read_b64(path: String) -> Result<String, String> {
    let p = Path::new(&path);
    let meta = fs::metadata(p).map_err(|e| e.to_string())?;
    if meta.len() > 28 * 1024 * 1024 {
        return Err("Файл слишком большой для WebView-конвертации (>28 МБ)".into());
    }
    let bytes = fs::read(p).map_err(|e| e.to_string())?;
    let ext = p
        .extension()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

/// Принимает сконвертированные PNG/WebP/JPEG байты от Canvas, при необходимости дожимает в ICO/PNG/JPG и сохраняет.
#[tauri::command]
pub async fn convert_save_canvas(
    src_path: String,
    target: String,
    bytes: Vec<u8>,
) -> Result<ConvertResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let t0 = Instant::now();
        let src = PathBuf::from(&src_path);
        let src_size = fs::metadata(&src).map(|m| m.len()).unwrap_or(0);
        let out = unique_out_path(&src, "", &target);

        if matches!(target.as_str(), "webp" | "png" | "jpg" | "jpeg") {
            fs::write(&out, &bytes).map_err(|e| e.to_string())?;
        } else if target == "ico" {
            let mut img =
                image::load_from_memory(&bytes).map_err(|e| format!("PNG/ICO decode: {e}"))?;
            let (w, h) = img.dimensions();
            if w > 256 || h > 256 {
                img = img.resize(256, 256, FilterType::Lanczos3);
            }
            img.save_with_format(&out, ImageFormat::Ico)
                .map_err(|e| e.to_string())?;
        } else {
            return Err("Неподдерживаемый формат сохранения".into());
        }

        let out_size = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        let ms = t0.elapsed().as_millis() as u64;
        log::line(format!("canvas convert -> {} ({out_size}B)", out.display()));
        Ok(ConvertResult {
            item: shelf_item_for(&out),
            src_size,
            out_size,
            ms,
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
pub async fn convert_zip_bundle(paths: Vec<String>) -> Result<ConvertResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let t0 = Instant::now();
        let valid: Vec<PathBuf> = paths
            .into_iter()
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .collect();
        if valid.is_empty() {
            return Err("Нет доступных файлов на Полке".into());
        }
        let src_size: u64 = valid
            .iter()
            .filter_map(|p| fs::metadata(p).ok())
            .map(|m| if m.is_dir() { 0 } else { m.len() })
            .sum();
        let base_dir = valid[0].parent().unwrap_or_else(|| Path::new("."));
        let stub = base_dir.join("kapla_shelf.zip");
        let out = unique_out_path(&stub, "", "zip");
        zip_single_or_many(&valid, &out)?;
        let out_size = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        Ok(ConvertResult {
            item: shelf_item_for(&out),
            src_size,
            out_size,
            ms: t0.elapsed().as_millis() as u64,
        })
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_csv_and_json_roundtrip() {
        let (cols, rows) = csv_to_table("name;age;ok\n\"Иван, П.\";25;true\nАня;30;false\n").unwrap();
        assert_eq!(cols, vec!["name", "age", "ok"]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0], "Иван, П.");

        let (jcols, jrows) =
            json_to_table(r#"[{"a":1,"b":"x"},{"a":2,"c":true}]"#).unwrap();
        assert_eq!(jcols, vec!["a", "b", "c"]);
        assert_eq!(jrows[0], vec!["1", "x", ""]);
        assert_eq!(jrows[1], vec!["2", "", "true"]);
    }

    #[test]
    fn converts_markdown_to_html_safely() {
        let html = markdown_to_html("Тест", "# Привет\n- пункт <1>\n> цитата");
        assert!(html.contains("<h1>Привет</h1>"));
        assert!(html.contains("<li>пункт &lt;1&gt;</li>"));
        assert!(html.contains("<blockquote>цитата</blockquote>"));
    }

    #[test]
    fn converts_real_png_to_jpg_and_ico() {
        let dir = std::env::temp_dir().join("kapla_conv_test");
        let _ = fs::create_dir_all(&dir);
        let src = dir.join("sample.png");
        let img = image::RgbaImage::from_pixel(300, 150, image::Rgba([255, 122, 61, 255]));
        DynamicImage::ImageRgba8(img)
            .save_with_format(&src, ImageFormat::Png)
            .unwrap();

        let jpg = convert_image(&src, "jpg", 85, Some(128)).unwrap();
        assert!(jpg.exists());
        assert_eq!(image::image_dimensions(&jpg).unwrap(), (128, 64));

        let ico = convert_image(&src, "ico", 90, None).unwrap();
        assert!(ico.exists());
        assert_eq!(image::image_dimensions(&ico).unwrap(), (256, 256));

        let _ = fs::remove_dir_all(&dir);
    }
}
