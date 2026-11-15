use crate::desktop::{HEIGHT, Scene, WIDTH, render_layers};

/// The original slot menu reads an uncompressed 100×75 BGR bitmap.
pub(super) fn thumbnail(scene: &Scene) -> Vec<u8> {
    let mut pixels = vec![0; WIDTH * HEIGHT];
    render_layers(scene, &mut pixels, &mut vec![0; WIDTH * HEIGHT]);
    let mut bitmap = vec![0; 54 + 100 * 75 * 3];
    bitmap[..2].copy_from_slice(b"BM");
    for (offset, value) in [
        (2, 22554u32),
        (10, 54),
        (14, 40),
        (18, 100),
        (22, 75),
        (34, 22500),
    ] {
        bitmap[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bitmap[26..28].copy_from_slice(&1u16.to_le_bytes());
    bitmap[28..30].copy_from_slice(&24u16.to_le_bytes());
    for row in 0..75 {
        for column in 0..100 {
            let pixel = pixels[(74 - row) * 8 * WIDTH + column * 8];
            let offset = 54 + (row * 100 + column) * 3;
            bitmap[offset..offset + 3].copy_from_slice(&[
                pixel as u8,
                (pixel >> 8) as u8,
                (pixel >> 16) as u8,
            ]);
        }
    }
    bitmap
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bitmap_has_the_original_dimensions_and_pixel_format() {
        let bitmap = thumbnail(&Scene::default());
        assert_eq!(bitmap.len(), 22554);
        let decoded = image::load_from_memory(&bitmap).unwrap().into_rgb8();
        assert_eq!(decoded.dimensions(), (100, 75));
        assert!(decoded.pixels().all(|pixel| pixel.0 == [0; 3]));
    }
}
