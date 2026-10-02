use std::path::Path;
use std::time::Instant;

use serde::Serialize;
use windows::core::HSTRING;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapDecoder, BitmapInterpolationMode, BitmapPixelFormat, BitmapTransform,
    ColorManagementMode, ExifOrientationMode,
};
use windows::Media::Ocr::OcrEngine;
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

pub fn recognize_png(path: &Path) -> Result<OcrReport, String> {
    let bytes = std::fs::read(path).map_err(|err| format!("无法读取图片: {err}"))?;
    if bytes.is_empty() {
        return Err("图片文件是空的".into());
    }
    let available = available_languages()?;
    let (engine, language, warning) = pick_engine(&available)?;
    let max_edge = OcrEngine::MaxImageDimension().unwrap_or(2600);
    let started = Instant::now();
    let bitmap = decode_png(&bytes, max_edge)?;
    let result = engine
        .RecognizeAsync(&bitmap)
        .map_err(|err| format!("RecognizeAsync 失败: {err}"))?
        .get()
        .map_err(|err| format!("识别等待失败: {err}"))?;
    let raw = result.Text().map_err(|err| err.to_string())?.to_string();
    let had_spaces = has_cjk_internal_space(&raw);
    let text = strip_cjk_internal_spaces(&raw);
    Ok(OcrReport {
        text,
        raw_text: raw,
        language,
        elapsed_ms: started.elapsed().as_millis() as u64,
        had_cjk_internal_spaces: had_spaces,
        available_languages: available,
        warning,
    })
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

    if max_edge > 0 && (width > max_edge || height > max_edge) {
        let scale = max_edge as f64 / width.max(height) as f64;
        let transform = BitmapTransform::new().map_err(|err| err.to_string())?;
        transform
            .SetScaledWidth(((width as f64) * scale).round().max(1.0) as u32)
            .map_err(|err| err.to_string())?;
        transform
            .SetScaledHeight(((height as f64) * scale).round().max(1.0) as u32)
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
    } else {
        decoder
            .GetSoftwareBitmapConvertedAsync(BitmapPixelFormat::Bgra8, BitmapAlphaMode::Premultiplied)
            .map_err(|err| err.to_string())?
            .get()
            .map_err(|err| err.to_string())
    }
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch,
        '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{3000}'..='\u{303F}'
            | '\u{FF00}'..='\u{FFEF}'
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
    use super::recognize_png;
    use std::path::PathBuf;

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
