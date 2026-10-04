//! 剪切板图片的编解码与落盘。原图按内容哈希存成 PNG，另存一张缩略图给列表用。

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};
use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapDecoder, BitmapEncoder, BitmapInterpolationMode, BitmapPixelFormat,
    BitmapTransform, ColorManagementMode, ExifOrientationMode,
};
use windows::Storage::Streams::{DataReader, InMemoryRandomAccessStream};

/// 列表缩略图的最大边长。按两倍像素密度给 160px 的槽位留余量。
const THUMB_MAX_EDGE: u32 = 320;

pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

pub struct Saved {
    pub path: PathBuf,
    pub thumb_path: PathBuf,
    pub width: u32,
    pub height: u32,
}

/// 原图与缩略图都写在 `%APPDATA%\com.daihao.dogear\images\` 下，文件名用内容哈希。
pub fn save(app: &AppHandle, png: &[u8], hash: &str) -> Result<Saved, String> {
    let decoded = decode(png)?;
    let dir = image_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|err| format!("无法创建图片目录: {err}"))?;

    let path = dir.join(format!("{hash}.png"));
    if !path.exists() {
        std::fs::write(&path, png).map_err(|err| format!("无法写入图片: {err}"))?;
    }

    let thumb_path = dir.join(format!("{hash}-thumb.png"));
    if !thumb_path.exists() {
        let thumb = thumbnail(png, THUMB_MAX_EDGE)?;
        std::fs::write(&thumb_path, &thumb).map_err(|err| format!("无法写入缩略图: {err}"))?;
    }

    Ok(Saved {
        path,
        thumb_path,
        width: decoded.width,
        height: decoded.height,
    })
}

pub fn read_as_data_url(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|err| format!("无法读取图片: {err}"))?;
    Ok(format!("data:image/png;base64,{}", base64(&bytes)))
}

/// 把一帧 BGRA 像素编码成 PNG。
pub fn encode(width: u32, height: u32, bgra: &[u8]) -> Result<Vec<u8>, String> {
    let expected = (width as usize) * (height as usize) * 4;
    if bgra.len() < expected {
        return Err("像素数据长度不足".into());
    }
    let stream = InMemoryRandomAccessStream::new().map_err(|err| err.to_string())?;
    let encoder_id = BitmapEncoder::PngEncoderId().map_err(|err| err.to_string())?;
    let encoder = BitmapEncoder::CreateAsync(encoder_id, &stream)
        .map_err(|err| err.to_string())?
        .get()
        .map_err(|err| format!("创建 PNG 编码器失败: {err}"))?;
    encoder
        .SetPixelData(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Straight,
            width,
            height,
            96.0,
            96.0,
            &bgra[..expected],
        )
        .map_err(|err| err.to_string())?;
    encoder
        .FlushAsync()
        .map_err(|err| err.to_string())?
        .get()
        .map_err(|err| format!("PNG 编码失败: {err}"))?;
    read_stream(&stream)
}

pub fn decode(png: &[u8]) -> Result<Decoded, String> {
    let decoder = open_decoder(png)?;
    let width = decoder.PixelWidth().map_err(|err| err.to_string())?;
    let height = decoder.PixelHeight().map_err(|err| err.to_string())?;
    if width == 0 || height == 0 {
        return Err("图片尺寸为 0".into());
    }
    let provider = decoder
        .GetPixelDataAsync()
        .map_err(|err| err.to_string())?
        .get()
        .map_err(|err| format!("读取像素失败: {err}"))?;
    let bgra = provider
        .DetachPixelData()
        .map_err(|err| err.to_string())?
        .to_vec();
    Ok(Decoded {
        width,
        height,
        bgra,
    })
}

fn thumbnail(png: &[u8], max_edge: u32) -> Result<Vec<u8>, String> {
    let decoder = open_decoder(png)?;
    let width = decoder.PixelWidth().map_err(|err| err.to_string())?;
    let height = decoder.PixelHeight().map_err(|err| err.to_string())?;
    if width == 0 || height == 0 {
        return Err("图片尺寸为 0".into());
    }
    if width <= max_edge && height <= max_edge {
        return Ok(png.to_vec());
    }
    let scale = max_edge as f64 / width.max(height) as f64;
    let target_w = ((width as f64) * scale).round().max(1.0) as u32;
    let target_h = ((height as f64) * scale).round().max(1.0) as u32;

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
    let provider = decoder
        .GetPixelDataTransformedAsync(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Straight,
            &transform,
            ExifOrientationMode::IgnoreExifOrientation,
            ColorManagementMode::DoNotColorManage,
        )
        .map_err(|err| err.to_string())?
        .get()
        .map_err(|err| format!("缩放失败: {err}"))?;
    let bgra = provider
        .DetachPixelData()
        .map_err(|err| err.to_string())?
        .to_vec();
    encode(target_w, target_h, &bgra)
}

fn open_decoder(png: &[u8]) -> Result<BitmapDecoder, String> {
    let stream = write_stream(png)?;
    BitmapDecoder::CreateAsync(&stream)
        .map_err(|err| err.to_string())?
        .get()
        .map_err(|err| format!("图片解码失败: {err}"))
}

fn write_stream(bytes: &[u8]) -> Result<InMemoryRandomAccessStream, String> {
    use windows::Storage::Streams::DataWriter;

    let stream = InMemoryRandomAccessStream::new().map_err(|err| err.to_string())?;
    let output = stream.GetOutputStreamAt(0).map_err(|err| err.to_string())?;
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
    Ok(stream)
}

fn read_stream(stream: &InMemoryRandomAccessStream) -> Result<Vec<u8>, String> {
    let size = stream.Size().map_err(|err| err.to_string())?;
    if size == 0 {
        return Err("编码结果为空".into());
    }
    if size > u32::MAX as u64 {
        return Err("图片过大".into());
    }
    let input = stream.GetInputStreamAt(0).map_err(|err| err.to_string())?;
    let reader = DataReader::CreateDataReader(&input).map_err(|err| err.to_string())?;
    reader
        .LoadAsync(size as u32)
        .map_err(|err| err.to_string())?
        .get()
        .map_err(|err| err.to_string())?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes).map_err(|err| err.to_string())?;
    Ok(bytes)
}

fn image_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("images"))
        .map_err(|err| format!("无法定位数据目录: {err}"))
}

/// 用于去重与文件名，不用于安全校验，所以 FNV-1a 足够。
pub fn hash(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(triple >> 18) as usize & 63] as char);
        out.push(TABLE[(triple >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(triple >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[triple as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{base64, decode, encode, hash};

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"hello world"), "aGVsbG8gd29ybGQ=");
    }

    #[test]
    fn hash_is_stable_and_distinct() {
        assert_eq!(hash(b"dogear"), hash(b"dogear"));
        assert_ne!(hash(b"dogear"), hash(b"dogears"));
    }

    #[test]
    fn png_round_trip_keeps_pixels() {
        let bgra = vec![
            0, 0, 255, 255, // 蓝
            0, 255, 0, 255, // 绿
            255, 0, 0, 255, // 红
            255, 255, 255, 255,
        ];
        let png = encode(2, 2, &bgra).expect("编码应当成功");
        assert_eq!(&png[1..4], b"PNG");
        let decoded = decode(&png).expect("解码应当成功");
        assert_eq!((decoded.width, decoded.height), (2, 2));
        assert_eq!(decoded.bgra, bgra);
    }
}
