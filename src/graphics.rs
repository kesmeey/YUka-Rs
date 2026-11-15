//! YKG000 image containers and raw animation records.
//!
//! Format references: AtomCrafty/yukatool2 (GPL-3.0),
//! src/Yuka.Core/IO/Formats/Ykg.cs and Graphics/Animation.cs.
//! GNP signature normalization also follows the format documented by
//! morkt/GARbro, ArcFormats/Yuka/ImageYKG.cs (MIT).

use crate::Result;
use serde::Serialize;
use std::borrow::Cow;
use std::ops::Range;

const HEADER_SIZE: usize = 0x40;
const FRAME_SIZE: usize = 0x20;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const GNP_SIGNATURE: &[u8; 8] = b"\x89GNP\r\n\x1a\n";

#[derive(Debug, Clone, Serialize)]
pub struct YkgHeader {
    pub encryption: u16,
    pub header_length: u32,
    pub reserved: [u8; 28],
    pub color_offset: u32,
    pub color_length: u32,
    pub alpha_offset: u32,
    pub alpha_length: u32,
    pub frame_offset: u32,
    pub frame_length: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Png,
    Gnp,
    Bmp,
    Unknown,
}

#[derive(Debug, Serialize)]
pub struct ImagePlane<'a> {
    pub offset: usize,
    pub length: usize,
    pub format: ImageFormat,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Exact container payload, before GNP signature normalization.
    #[serde(skip)]
    pub data: &'a [u8],
}

impl ImagePlane<'_> {
    /// Return a standard image payload without transcoding the pixels.
    pub fn normalized_bytes(&self) -> Cow<'_, [u8]> {
        if self.format == ImageFormat::Gnp {
            let mut result = self.data.to_vec();
            result[..8].copy_from_slice(PNG_SIGNATURE);
            Cow::Owned(result)
        } else {
            Cow::Borrowed(self.data)
        }
    }

    pub fn extension(&self) -> &'static str {
        match self.format {
            ImageFormat::Png | ImageFormat::Gnp => "png",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Unknown => "bin",
        }
    }
}

/// Raw animation fields. Units, flags and action behavior need runtime evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AnimationFrame {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub duration: u32,
    pub flags: u32,
    pub unknown: u32,
    pub action: u32,
}

#[derive(Debug, Serialize)]
pub struct Ykg<'a> {
    pub header: YkgHeader,
    pub color: Option<ImagePlane<'a>>,
    pub alpha: Option<ImagePlane<'a>>,
    pub frames: Vec<AnimationFrame>,
    #[serde(skip)]
    pub frame_data: &'a [u8],
}

impl<'a> Ykg<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        if data.len() < HEADER_SIZE {
            return Err("YKG header is truncated (requires 64 bytes)".into());
        }
        if &data[..6] != b"YKG000" {
            return Err("unsupported YKG signature (expected YKG000)".into());
        }
        let header = YkgHeader {
            encryption: u16::from_le_bytes([data[6], data[7]]),
            header_length: read_u32(data, 8),
            reserved: data[12..40].try_into()?,
            color_offset: read_u32(data, 40),
            color_length: read_u32(data, 44),
            alpha_offset: read_u32(data, 48),
            alpha_length: read_u32(data, 52),
            frame_offset: read_u32(data, 56),
            frame_length: read_u32(data, 60),
        };
        if header.encryption != 0 {
            return Err(format!("unsupported YKG encryption flag {}", header.encryption).into());
        }
        if header.header_length as usize != HEADER_SIZE {
            return Err(format!("unsupported YKG header length {}", header.header_length).into());
        }

        let color_range = section_range(
            data.len(),
            header.color_offset,
            header.color_length,
            "color",
        )?;
        let alpha_range = section_range(
            data.len(),
            header.alpha_offset,
            header.alpha_length,
            "alpha",
        )?;
        let frame_range = section_range(
            data.len(),
            header.frame_offset,
            header.frame_length,
            "frames",
        )?;
        let present = [&color_range, &alpha_range, &frame_range];
        for (index, left) in present.iter().enumerate() {
            for right in present.iter().skip(index + 1) {
                if let (Some(left), Some(right)) = (left, right)
                    && left.start < right.end
                    && right.start < left.end
                {
                    return Err("overlapping YKG sections".into());
                }
            }
        }

        let color = color_range
            .map(|range| parse_plane(data, range))
            .transpose()?;
        let alpha = alpha_range
            .map(|range| parse_plane(data, range))
            .transpose()?;
        let frame_data = frame_range.map_or(&[][..], |range| &data[range]);
        if !frame_data.len().is_multiple_of(FRAME_SIZE) {
            return Err("YKG animation length is not a multiple of 32 bytes".into());
        }
        let frames = frame_data
            .chunks_exact(FRAME_SIZE)
            .map(|frame| AnimationFrame {
                x: read_u32(frame, 0),
                y: read_u32(frame, 4),
                width: read_u32(frame, 8),
                height: read_u32(frame, 12),
                duration: read_u32(frame, 16),
                flags: read_u32(frame, 20),
                unknown: read_u32(frame, 24),
                action: read_u32(frame, 28),
            })
            .collect();
        Ok(Self {
            header,
            color,
            alpha,
            frames,
            frame_data,
        })
    }
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        data[offset..offset + 4]
            .try_into()
            .expect("validated field range"),
    )
}

fn section_range(
    total: usize,
    offset: u32,
    length: u32,
    name: &str,
) -> Result<Option<Range<usize>>> {
    if length == 0 {
        if offset != 0 {
            return Err(format!("YKG {name} has an offset but no length").into());
        }
        return Ok(None);
    }
    let start = offset as usize;
    let end = start
        .checked_add(length as usize)
        .ok_or("YKG section range overflow")?;
    if start < HEADER_SIZE || end > total {
        return Err(format!("YKG {name} range {start}..{end} is outside the payload").into());
    }
    Ok(Some(start..end))
}

fn parse_plane(data: &[u8], range: Range<usize>) -> Result<ImagePlane<'_>> {
    let payload = &data[range.clone()];
    let format = if payload.starts_with(PNG_SIGNATURE) {
        ImageFormat::Png
    } else if payload.starts_with(GNP_SIGNATURE) {
        ImageFormat::Gnp
    } else if payload.starts_with(b"BM") {
        ImageFormat::Bmp
    } else {
        ImageFormat::Unknown
    };
    let dimensions = match format {
        ImageFormat::Png | ImageFormat::Gnp => Some(png_dimensions(payload)?),
        ImageFormat::Bmp => Some(bmp_dimensions(payload)?),
        ImageFormat::Unknown => None,
    };
    Ok(ImagePlane {
        offset: range.start,
        length: range.len(),
        format,
        width: dimensions.map(|d| d.0),
        height: dimensions.map(|d| d.1),
        data: payload,
    })
}

/// Validate the chunk boundaries and basic layout, without decompressing pixels
/// or claiming to validate CRCs. PNG and GNP share all bytes after the signature.
fn png_dimensions(data: &[u8]) -> Result<(u32, u32)> {
    if data.len() < 33 || &data[8..16] != b"\0\0\0\rIHDR" {
        return Err("YKG PNG/GNP has a truncated or invalid IHDR".into());
    }
    let width = u32::from_be_bytes(data[16..20].try_into()?);
    let height = u32::from_be_bytes(data[20..24].try_into()?);
    if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return Err("YKG PNG/GNP has invalid dimensions".into());
    }
    let mut position = 8usize;
    let mut has_image_data = false;
    while position < data.len() {
        let header_end = position.checked_add(8).ok_or("PNG chunk range overflow")?;
        if header_end > data.len() {
            return Err("YKG PNG/GNP chunk header is truncated".into());
        }
        let length = u32::from_be_bytes(data[position..position + 4].try_into()?) as usize;
        let end = header_end
            .checked_add(length)
            .and_then(|n| n.checked_add(4))
            .ok_or("PNG chunk range overflow")?;
        if end > data.len() {
            return Err("YKG PNG/GNP chunk payload is truncated".into());
        }
        let kind = &data[position + 4..header_end];
        if kind == b"IDAT" {
            has_image_data = true;
        }
        if kind == b"IEND" {
            if length != 0 || !has_image_data {
                return Err("YKG PNG/GNP has an invalid IEND or lacks IDAT".into());
            }
            return Ok((width, height));
        }
        position = end;
    }
    Err("YKG PNG/GNP is missing IEND".into())
}

fn bmp_dimensions(data: &[u8]) -> Result<(u32, u32)> {
    if data.len() < 26 {
        return Err("YKG BMP header is truncated".into());
    }
    let file_length = read_u32(data, 2) as usize;
    let pixel_offset = read_u32(data, 10) as usize;
    let dib_length = read_u32(data, 14) as usize;
    let header_end = 14usize
        .checked_add(dib_length)
        .ok_or("BMP header range overflow")?;
    if header_end > data.len()
        || pixel_offset < header_end
        || pixel_offset > data.len()
        || file_length > data.len()
    {
        return Err("YKG BMP contains an invalid header or pixel range".into());
    }
    let (width, height) = if dib_length == 12 {
        (
            u16::from_le_bytes(data[18..20].try_into()?) as u32,
            u16::from_le_bytes(data[20..22].try_into()?) as u32,
        )
    } else if dib_length >= 40 {
        let width = i32::from_le_bytes(data[18..22].try_into()?);
        let height = i32::from_le_bytes(data[22..26].try_into()?);
        if width <= 0 || height == i32::MIN {
            return Err("YKG BMP has invalid signed dimensions".into());
        }
        (width as u32, height.unsigned_abs())
    } else {
        return Err(format!("unsupported BMP DIB header length {dib_length}").into());
    };
    if width == 0 || height == 0 {
        return Err("YKG BMP has zero dimensions".into());
    }
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_u32(data: &mut [u8], offset: usize, value: u32) {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn tiny_png() -> Vec<u8> {
        // Complete 1x1 RGBA PNG, independent of any copyrighted game image.
        [
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 96, 0, 2,
            0, 0, 5, 0, 1, 165, 246, 69, 64, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ]
        .to_vec()
    }

    fn container(color: &[u8], alpha: &[u8], frames: &[u8]) -> Vec<u8> {
        let mut data = vec![0u8; HEADER_SIZE];
        data[..6].copy_from_slice(b"YKG000");
        set_u32(&mut data, 8, HEADER_SIZE as u32);
        for (offset, payload) in [(40, color), (48, alpha), (56, frames)] {
            if !payload.is_empty() {
                let start = data.len() as u32;
                set_u32(&mut data, offset, start);
                set_u32(&mut data, offset + 4, payload.len() as u32);
                data.extend_from_slice(payload);
            }
        }
        data
    }

    #[test]
    fn separates_planes_and_preserves_unknown_frame_bits() {
        let png = tiny_png();
        let mut gnp = png.clone();
        gnp[..8].copy_from_slice(GNP_SIGNATURE);
        let mut frames = vec![0u8; FRAME_SIZE];
        set_u32(&mut frames, 8, 1);
        set_u32(&mut frames, 12, 1);
        set_u32(&mut frames, 16, 400);
        set_u32(&mut frames, 20, 6);
        set_u32(&mut frames, 24, 0x11223344);
        set_u32(&mut frames, 28, 9);
        let data = container(&gnp, &png, &frames);
        let graphic = Ykg::parse(&data).unwrap();
        let color = graphic.color.unwrap();
        assert_eq!(color.format, ImageFormat::Gnp);
        assert_eq!((color.width, color.height), (Some(1), Some(1)));
        assert_eq!(color.normalized_bytes().as_ref(), png);
        assert_eq!(color.data, gnp);
        assert_eq!(graphic.alpha.unwrap().data, png);
        assert_eq!(graphic.frames[0].flags, 6);
        assert_eq!(graphic.frames[0].unknown, 0x11223344);
        assert_eq!(graphic.frames[0].action, 9);
        assert_eq!(graphic.frame_data, frames);
    }

    #[test]
    fn rejects_truncated_ranges_and_overlaps() {
        let original = container(&tiny_png(), &tiny_png(), &[0u8; FRAME_SIZE]);
        for length in 0..original.len() {
            assert!(
                Ykg::parse(&original[..length]).is_err(),
                "accepted prefix length {length}"
            );
        }
        let mut overlap = original.clone();
        set_u32(&mut overlap, 48, HEADER_SIZE as u32);
        assert!(
            Ykg::parse(&overlap)
                .unwrap_err()
                .to_string()
                .contains("overlapping")
        );
        let mut wrapped = original;
        set_u32(&mut wrapped, 40, u32::MAX);
        set_u32(&mut wrapped, 44, u32::MAX);
        assert!(Ykg::parse(&wrapped).is_err());
    }

    #[test]
    fn rejects_partial_frames_and_truncated_png_chunks() {
        assert!(Ykg::parse(&container(&tiny_png(), &[], &[0u8; 31])).is_err());
        let mut png = tiny_png();
        png.truncate(png.len() - 1);
        assert!(Ykg::parse(&container(&png, &[], &[])).is_err());
        let mut png = tiny_png();
        png[33..37].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(Ykg::parse(&container(&png, &[], &[])).is_err());
    }

    #[test]
    fn preserves_unknown_plane_format() {
        let data = container(b"unrecognized image encoding", &[], &[]);
        let graphic = Ykg::parse(&data).unwrap();
        let color = graphic.color.unwrap();
        assert_eq!(color.format, ImageFormat::Unknown);
        assert_eq!(color.width, None);
        assert_eq!(color.extension(), "bin");
        assert_eq!(color.normalized_bytes().as_ref(), color.data);
    }

    #[test]
    #[ignore = "requires local game data via YUKA_GAME_DIR"]
    fn local_game_graphics() {
        use crate::archive::Archive;
        use std::collections::BTreeMap;
        use std::path::PathBuf;

        let directory = PathBuf::from(
            std::env::var_os("YUKA_GAME_DIR")
                .expect("set YUKA_GAME_DIR to the local game directory"),
        );
        let mut paths: Vec<_> = std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("ykc"))
            })
            .collect();
        paths.sort();
        let mut summary = BTreeMap::new();
        let mut total = 0u64;
        for path in paths {
            let mut archive = Archive::open(&path).unwrap();
            let indices: Vec<_> = archive
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.name.to_ascii_lowercase().ends_with(".ykg"))
                .map(|(index, _)| index)
                .collect();
            let mut counts = BTreeMap::new();
            for index in indices {
                let data = archive.read_entry(index, 256 * 1024 * 1024).unwrap();
                let graphic = Ykg::parse(&data)
                    .unwrap_or_else(|error| panic!("{}: {error}", archive.entries[index].name));
                *counts.entry("images").or_insert(0u64) += 1;
                *counts.entry("animation_frames").or_insert(0) += graphic.frames.len() as u64;
                *counts.entry("animated_images").or_insert(0) +=
                    u64::from(!graphic.frames.is_empty());
                *counts.entry("separate_alpha").or_insert(0) += u64::from(graphic.alpha.is_some());
                if let Some(plane) = graphic.color {
                    let format = match plane.format {
                        ImageFormat::Png => "png",
                        ImageFormat::Gnp => "gnp",
                        ImageFormat::Bmp => "bmp",
                        ImageFormat::Unknown => "unknown",
                    };
                    *counts.entry(format).or_insert(0) += 1;
                }
                total += 1;
            }
            if !counts.is_empty() {
                summary.insert(
                    path.file_name().unwrap().to_string_lossy().into_owned(),
                    counts,
                );
            }
        }
        assert!(total > 0, "no YKG samples found");
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    }
}
