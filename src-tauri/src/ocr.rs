use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use windows::core::HSTRING;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapDecoder, BitmapInterpolationMode, BitmapPixelFormat, BitmapTransform,
    ColorManagementMode, ExifOrientationMode,
};
use windows::Graphics::Imaging::SoftwareBitmap;
use windows::Media::Ocr::{OcrEngine, OcrResult};
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrReport {
    pub text: String,
    pub raw_text: String,
    pub language: String,
    pub elapsed_ms: u64,
    pub had_cjk_internal_spaces: bool,
    pub available_languages: Vec<String>,
    pub warning: Option<String>,
}

/// 低于这个边长的图基本不会有可读文字，识别了也只是噪声。
const MIN_EDGE: u32 = 40;

static QUEUE: OnceLock<Sender<i64>> = OnceLock::new();

/// 串行后台队列：图片入库后排进来，识别完通过 `ocr-updated` 通知前端。
pub fn start_queue(app: AppHandle) {
    let (tx, rx) = channel::<i64>();
    if QUEUE.set(tx).is_err() {
        return;
    }
    std::thread::spawn(move || {
        for id in rx {
            recognize_clip(&app, id);
        }
    });
}

pub fn enqueue(id: i64) {
    if let Some(tx) = QUEUE.get() {
        let _ = tx.send(id);
    }
}

fn recognize_clip(app: &AppHandle, id: i64) {
    let Some(state) = app.try_state::<Mutex<crate::state::AppState>>() else {
        return;
    };
    let Some((path, width, height)) = state.lock().ok().and_then(|guard| {
        let clip = guard.clips.iter().find(|clip| clip.id == id)?;
        let path = clip.file_path.clone()?;
        Some((PathBuf::from(path), clip.width, clip.height))
    }) else {
        return;
    };

    let too_small = width.unwrap_or(0) < MIN_EDGE || height.unwrap_or(0) < MIN_EDGE;
    let outcome = if too_small {
        Err(String::new())
    } else {
        recognize_png(&path)
    };

    let status = {
        let Ok(mut guard) = state.lock() else {
            return;
        };
        let Some(clip) = guard.clips.iter_mut().find(|clip| clip.id == id) else {
            return;
        };
        match &outcome {
            Ok(report) if report.text.trim().is_empty() => {
                clip.ocr_status = "empty".into();
                clip.ocr_lang = Some(report.language.clone());
            }
            Ok(report) => {
                clip.ocr_status = "done".into();
                clip.ocr_text = Some(report.text.clone());
                clip.ocr_lang = Some(report.language.clone());
            }
            Err(err) if err.is_empty() => clip.ocr_status = "empty".into(),
            Err(_) => clip.ocr_status = "failed".into(),
        }
        clip.ocr_status.clone()
    };

    match &outcome {
        Ok(report) => eprintln!(
            "图片文字识别完成：{} 字，语言 {}，耗时 {}ms",
            report.text.chars().count(),
            report.language,
            report.elapsed_ms
        ),
        Err(err) if err.is_empty() => eprintln!("图片太小，跳过文字识别"),
        Err(err) => eprintln!("图片文字识别失败: {err}"),
    }
    let _ = app.emit("ocr-updated", status);
}

pub fn recognize_png(path: &Path) -> Result<OcrReport, String> {
    let bytes = std::fs::read(path).map_err(|err| format!("无法读取图片: {err}"))?;
    if bytes.is_empty() {
        return Err("图片文件是空的".into());
    }
    let available = available_languages()?;
    let max_edge = OcrEngine::MaxImageDimension().unwrap_or(2600);
    let started = Instant::now();
    // 同一张放大后的图给两个引擎用，避免英文图被中文引擎认成错字。
    let bitmap = decode_png(&bytes, max_edge)?;
    let (engine, mut language, warning) = pick_engine(&available)?;
    let first = recognize_bitmap(&engine, &bitmap)?;
    let had_spaces = has_cjk_internal_space(&first);
    let mut text = first.clone();
    if prefers_latin(&first) {
        if let Some((english, tag)) = engine_for(&["en-US", "en-GB", "en"]) {
            let second = recognize_bitmap(&english, &bitmap)?;
            if latin_letters(&second) > latin_letters(&first) {
                text = second;
                language = tag;
            }
        }
    }
    Ok(OcrReport {
        text,
        raw_text: first,
        language,
        elapsed_ms: started.elapsed().as_millis() as u64,
        had_cjk_internal_spaces: had_spaces,
        available_languages: available,
        warning,
    })
}

fn recognize_bitmap(engine: &OcrEngine, bitmap: &SoftwareBitmap) -> Result<String, String> {
    let result = engine
        .RecognizeAsync(bitmap)
        .map_err(|err| format!("RecognizeAsync 失败: {err}"))?
        .get()
        .map_err(|err| format!("识别等待失败: {err}"))?;
    Ok(strip_cjk_internal_spaces(&layout_text(&result)?))
}

fn engine_for(tags: &[&str]) -> Option<(OcrEngine, String)> {
    for tag in tags {
        let Some(engine) = try_language(tag) else {
            continue;
        };
        let actual = engine
            .RecognizerLanguage()
            .ok()
            .and_then(|lang| lang.LanguageTag().ok())
            .map(|tag| tag.to_string())
            .unwrap_or_else(|| (*tag).to_string());
        return Some((engine, actual));
    }
    None
}

/// 中文引擎碰到纯英文会硬认成汉字。拉丁字母已经占多数时，再拿英文引擎对一次。
fn prefers_latin(text: &str) -> bool {
    let latin = latin_letters(text);
    latin >= 8 && latin * 2 >= cjk_letters(text)
}

fn latin_letters(text: &str) -> usize {
    text.chars().filter(|ch| ch.is_ascii_alphanumeric()).count()
}

fn cjk_letters(text: &str) -> usize {
    text.chars().filter(|ch| is_cjk_letter(*ch)).count()
}

struct PlacedWord {
    text: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// 按词的位置重排：先上到下，再左到右。相邻汉字不加空格，英文词之间才加。
fn layout_text(result: &OcrResult) -> Result<String, String> {
    let lines = result.Lines().map_err(|err| err.to_string())?;
    let line_count = lines.Size().map_err(|err| err.to_string())?;
    let mut words = Vec::new();
    for index in 0..line_count {
        let line = lines.GetAt(index).map_err(|err| err.to_string())?;
        let line_words = line.Words().map_err(|err| err.to_string())?;
        let word_count = line_words.Size().map_err(|err| err.to_string())?;
        for word_index in 0..word_count {
            let word = line_words.GetAt(word_index).map_err(|err| err.to_string())?;
            let text = word.Text().map_err(|err| err.to_string())?.to_string();
            if text.trim().is_empty() {
                continue;
            }
            let rect = word.BoundingRect().map_err(|err| err.to_string())?;
            words.push(PlacedWord {
                text,
                x: rect.X,
                y: rect.Y,
                w: rect.Width,
                h: rect.Height,
            });
        }
    }
    if words.is_empty() {
        return Ok(String::new());
    }
    words.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut rows: Vec<Vec<PlacedWord>> = Vec::new();
    for word in words {
        let same_row = rows.last().is_some_and(|row| {
            let anchor = row.iter().map(|item| item.y).sum::<f32>() / row.len() as f32;
            let height = row.iter().fold(word.h, |max, item| max.max(item.h)).max(1.0);
            (word.y - anchor).abs() <= height * 0.5
        });
        if same_row {
            rows.last_mut().expect("刚确认过这一行存在").push(word);
        } else {
            rows.push(vec![word]);
        }
    }

    let mut lines_out = Vec::with_capacity(rows.len());
    for mut row in rows {
        row.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        lines_out.push(join_row(&row));
    }
    Ok(lines_out.join("\n"))
}

fn join_row(row: &[PlacedWord]) -> String {
    let mut out = String::new();
    for (index, word) in row.iter().enumerate() {
        if index > 0 {
            let prev = &row[index - 1];
            let gap = word.x - (prev.x + prev.w);
            let prev_cjk = prev.text.chars().last().is_some_and(is_cjk_letter);
            let next_cjk = word.text.chars().next().is_some_and(is_cjk_letter);
            if !(prev_cjk && next_cjk) && gap > prev.h.max(1.0) * 0.12 {
                out.push(' ');
            }
        }
        out.push_str(word.text.trim());
    }
    out
}

fn available_languages() -> Result<Vec<String>, String> {
    let langs = OcrEngine::AvailableRecognizerLanguages().map_err(|err| err.to_string())?;
    let mut names = Vec::new();
    let count = langs.Size().map_err(|err| err.to_string())?;
    for index in 0..count {
        let lang = langs.GetAt(index).map_err(|err| err.to_string())?;
        let tag = lang.LanguageTag().map_err(|err| err.to_string())?;
        names.push(tag.to_string());
    }
    Ok(names)
}

fn pick_engine(available: &[String]) -> Result<(OcrEngine, String, Option<String>), String> {
    if available.is_empty() {
        return Err(
            "系统没有可用的 OCR 语言包。可用语言：无。请到 Windows 设置 → 时间和语言 → 语言和区域，为需要的语言安装语言包（含光学字符识别）。"
                .into(),
        );
    }

    let preferred = ["zh-Hans-CN", "zh-CN", "zh-Hans", "en-US", "en-GB", "en"];
    for tag in preferred {
        if let Some(engine) = try_language(tag) {
            let actual = engine
                .RecognizerLanguage()
                .ok()
                .and_then(|lang| lang.LanguageTag().ok())
                .map(|tag| tag.to_string())
                .unwrap_or_else(|| tag.to_string());
            let warning = if !actual.to_ascii_lowercase().starts_with("zh") {
                Some(format!(
                    "未使用中文识别引擎（当前 {actual}）。可用语言：{}",
                    available.join(", ")
                ))
            } else {
                None
            };
            return Ok((engine, actual, warning));
        }
    }

    match OcrEngine::TryCreateFromUserProfileLanguages() {
        Ok(engine) => {
            let actual = engine
                .RecognizerLanguage()
                .ok()
                .and_then(|lang| lang.LanguageTag().ok())
                .map(|tag| tag.to_string())
                .unwrap_or_else(|| "user-profile".into());
            Ok((
                engine,
                actual.clone(),
                Some(format!(
                    "预设语言都不可用，已改用系统语言 {actual}。可用语言：{}",
                    available.join(", ")
                )),
            ))
        }
        Err(err) => Err(format!(
            "无法创建 OCR 引擎（{err}）。可用语言：{}",
            available.join(", ")
        )),
    }
}

fn try_language(tag: &str) -> Option<OcrEngine> {
    let lang = Language::CreateLanguage(&HSTRING::from(tag)).ok()?;
    if !OcrEngine::IsLanguageSupported(&lang).unwrap_or(false) {
        return None;
    }
    OcrEngine::TryCreateFromLanguage(&lang).ok()
}

fn decode_png(bytes: &[u8], max_edge: u32) -> Result<windows::Graphics::Imaging::SoftwareBitmap, String> {
    let stream = InMemoryRandomAccessStream::new().map_err(|err| err.to_string())?;
    let output = stream
        .GetOutputStreamAt(0)
        .map_err(|err| err.to_string())?;
    let writer = DataWriter::CreateDataWriter(&output).map_err(|err| err.to_string())?;
    writer.WriteBytes(bytes).map_err(|err| err.to_string())?;
    writer
        .StoreAsync()
        .map_err(|err| err.to_string())?
        .get()
        .map_err(|err| err.to_string())?;
    writer
        .FlushAsync()
        .map_err(|err| err.to_string())?
        .get()
        .map_err(|err| err.to_string())?;
    stream.Seek(0).map_err(|err| err.to_string())?;

    let decoder = BitmapDecoder::CreateAsync(&stream)
        .map_err(|err| err.to_string())?
        .get()
        .map_err(|err| format!("PNG 解码失败: {err}"))?;
    let width = decoder.PixelWidth().map_err(|err| err.to_string())?;
    let height = decoder.PixelHeight().map_err(|err| err.to_string())?;
    if width == 0 || height == 0 {
        return Err("图片尺寸为 0".into());
    }

    let (target_w, target_h) = scaled_size(width, height, max_edge);
    if target_w == width && target_h == height {
        return decoder
            .GetSoftwareBitmapConvertedAsync(BitmapPixelFormat::Bgra8, BitmapAlphaMode::Premultiplied)
            .map_err(|err| err.to_string())?
            .get()
            .map_err(|err| err.to_string());
    }

    let transform = BitmapTransform::new().map_err(|err| err.to_string())?;
    transform
        .SetScaledWidth(target_w)
        .map_err(|err| err.to_string())?;
    transform
        .SetScaledHeight(target_h)
        .map_err(|err| err.to_string())?;
    transform
        .SetInterpolationMode(BitmapInterpolationMode::Fant)
        .map_err(|err| err.to_string())?;
    decoder
        .GetSoftwareBitmapTransformedAsync(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Premultiplied,
            &transform,
            ExifOrientationMode::IgnoreExifOrientation,
            ColorManagementMode::DoNotColorManage,
        )
        .map_err(|err| err.to_string())?
        .get()
        .map_err(|err| err.to_string())
}

/// 小图放大到引擎吃得下的尺寸，字号大约翻倍后错字会少一截；过大的图则缩小到上限内。
fn scaled_size(width: u32, height: u32, max_edge: u32) -> (u32, u32) {
    let long = width.max(height).max(1) as f64;
    let cap = if max_edge == 0 { long } else { max_edge as f64 };
    let scale = if long > cap {
        cap / long
    } else {
        (cap / long).min(2.5)
    };
    if (scale - 1.0).abs() < 0.1 {
        return (width, height);
    }
    let target_w = ((width as f64) * scale).round().clamp(1.0, cap) as u32;
    let target_h = ((height as f64) * scale).round().clamp(1.0, cap) as u32;
    (target_w.max(1), target_h.max(1))
}

fn is_cjk(ch: char) -> bool {
    is_cjk_letter(ch)
        || matches!(ch, '\u{3000}'..='\u{303F}' | '\u{FF00}'..='\u{FFEF}')
}

fn is_cjk_letter(ch: char) -> bool {
    matches!(
        ch,
        '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}'
    )
}

fn has_cjk_internal_space(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    chars.windows(3).any(|window| {
        is_cjk(window[0]) && (window[1] == ' ' || window[1] == '\u{3000}') && is_cjk(window[2])
    })
}

fn strip_cjk_internal_spaces(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if (ch == ' ' || ch == '\u{3000}')
            && index > 0
            && index + 1 < chars.len()
            && is_cjk(chars[index - 1])
            && is_cjk(chars[index + 1])
        {
            index += 1;
            continue;
        }
        out.push(ch);
        index += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{cjk_letters, join_row, prefers_latin, scaled_size, PlacedWord};
    use super::recognize_png;
    use std::path::PathBuf;

    #[test]
    fn small_images_scale_up_and_large_ones_fit() {
        assert_eq!(scaled_size(400, 200, 2600), (1000, 500));
        assert_eq!(scaled_size(4000, 2000, 2600).0, 2600);
        assert_eq!(scaled_size(2400, 1200, 2600), (2400, 1200));
    }

    #[test]
    fn english_text_asks_for_the_latin_engine() {
        assert!(prefers_latin("Dogear OCR check 2026"));
        assert!(!prefers_latin("折角剪切板测试"));
        assert_eq!(cjk_letters("汉字"), 2);
    }

    #[test]
    fn cjk_words_stay_together_and_english_keeps_a_space() {
        let row = [
            PlacedWord { text: "折角".into(), x: 0.0, y: 0.0, w: 40.0, h: 20.0 },
            PlacedWord { text: "测试".into(), x: 42.0, y: 0.0, w: 40.0, h: 20.0 },
            PlacedWord { text: "OCR".into(), x: 100.0, y: 0.0, w: 36.0, h: 20.0 },
        ];
        assert_eq!(join_row(&row), "折角测试 OCR");
    }

    #[test]
    fn probe_ocr_samples() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/ocr-samples");
        assert!(dir.is_dir(), "缺少 tests/ocr-samples");
        let mut count = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("png") {
                continue;
            }
            count += 1;
            match recognize_png(&path) {
                Ok(report) => {
                    println!(
                        "OK {}\n  lang={} elapsed={}ms cjk_spaces={} warning={:?}\n  raw={:?}\n  text={:?}",
                        path.display(),
                        report.language,
                        report.elapsed_ms,
                        report.had_cjk_internal_spaces,
                        report.warning,
                        report.raw_text,
                        report.text
                    );
                }
                Err(err) => println!("ERR {} {err}", path.display()),
            }
        }
        assert!(count >= 3, "样例不足");
    }
}
