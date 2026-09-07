//! Procedural icon generation — pure `std`, zero external dependencies.
//!
//! This module is shared by three consumers, so it must stay dependency-free:
//! - `build.rs` (embedded resource icon),
//! - `examples/gen-icon.rs` (static `assets/icon.ico`),
//! - unit tests (`rustc --edition 2021 --test src/icon.rs`).
//!
//! Rendering is done with 4x4 supersampling per pixel, which anti-aliases every
//! edge (eye outline, iris, pupil, highlight) uniformly without a hand-tuned
//! distance-field smoothstep for each layer.

/// Render the EyeFlow "eye" logo as `size * size` RGBA8 pixels.
///
/// Layout: row-major, top-left origin, straight (non-premultiplied) alpha.
/// Every pixel is the average of a 4x4 supersample grid.
pub fn render_rgba(size: u32) -> Vec<u8> {
    const SS: u32 = 4;
    let s = size as f32;
    let inv = 1.0 / (SS * SS) as f32;

    // --- Geometry, normalized to a [0, 1] canvas -------------------------
    // Almond (eye outline) = intersection of two disks of radius `lid_r`
    // centered at (0, +k) and (0, -k).  Their intersection is a convex lens:
    // half-width `a` on the horizontal axis, half-height `b` vertically.
    let a = 0.42f32; // half-width  -> ~8% transparent margin on each side
    let b = 0.21f32; // half-height -> 2:1 almond aspect
    let k = (a * a - b * b) / (2.0 * b); // circle-center offset
    let lid_r = b + k; // radius of the two lid circles

    // Iris / pupil. Slightly enlarged on tiny canvases so the eye still reads.
    let iris_r = if size <= 20 { 0.16 } else { 0.145 };
    let pupil_r = if size <= 20 { 0.075 } else { 0.062 };
    // The catchlight is dropped below ~32px where it would dissolve into mush.
    let has_highlight = size >= 32;
    let (hl_x, hl_y, hl_r) = (-0.048f32, -0.052f32, 0.030f32);

    // Eyelid stroke: 1..2 device px, scaled with size (1 px at 16px is already
    // proportionally bold: 6% of the canvas width).
    let stroke_half = (s / 128.0).clamp(1.0, 2.0) * 0.5 / s;

    // --- Palette (linear [0, 1] RGB) -------------------------------------
    let white = [0.957f32, 0.969, 0.980]; // #F4F7FA eye white (slightly cool)
    let iris_in = [0.227f32, 0.627, 0.847]; // #3AA0D8 iris center
    let iris_out = [0.122f32, 0.435, 0.659]; // #1F6FA8 iris rim
    let pupil = [0.063f32, 0.094, 0.125]; // #101820 pupil
    let stroke = [0.106f32, 0.227, 0.322]; // #1B3A52 eyelid outline

    let mut out = vec![0u8; (size as usize) * (size as usize) * 4];

    for y in 0..size {
        for x in 0..size {
            let (mut r, mut g, mut bl, mut cov_sum) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);

            for sy in 0..SS {
                for sx in 0..SS {
                    let fx = (x as f32 + (sx as f32 + 0.5) / SS as f32) / s - 0.5;
                    let fy = (y as f32 + (sy as f32 + 0.5) / SS as f32) / s - 0.5;

                    // Intersection of convex sets: signed distance is the max.
                    let d_eye = sd_disk(fx, fy, 0.0, k, lid_r).max(sd_disk(fx, fy, 0.0, -k, lid_r));

                    let mut c = [0.0f32; 3];
                    let mut cov = 0.0f32;
                    if d_eye.abs() <= stroke_half {
                        // Eyelid outline straddles the almond boundary.
                        cov = 1.0;
                        c = stroke;
                    } else if d_eye < 0.0 {
                        cov = 1.0;
                        let rad = (fx * fx + fy * fy).sqrt();
                        if rad < iris_r {
                            if rad < pupil_r {
                                c = pupil;
                            } else {
                                let t = rad / iris_r;
                                c = [
                                    iris_in[0] + (iris_out[0] - iris_in[0]) * t,
                                    iris_in[1] + (iris_out[1] - iris_in[1]) * t,
                                    iris_in[2] + (iris_out[2] - iris_in[2]) * t,
                                ];
                            }
                            // Catchlight on top of iris/pupil, upper-left.
                            if has_highlight && sd_disk(fx, fy, hl_x, hl_y, hl_r) < 0.0 {
                                c = [1.0, 1.0, 1.0];
                            }
                        } else {
                            c = white;
                        }
                    }

                    r += c[0] * cov;
                    g += c[1] * cov;
                    bl += c[2] * cov;
                    cov_sum += cov;
                }
            }

            // Accumulate premultiplied, emit straight alpha.
            let alpha = cov_sum * inv;
            let i = ((y * size + x) * 4) as usize;
            if alpha > 0.0 {
                out[i] = unit_u8(r * inv / alpha);
                out[i + 1] = unit_u8(g * inv / alpha);
                out[i + 2] = unit_u8(bl * inv / alpha);
                out[i + 3] = unit_u8(alpha);
            }
        }
    }
    out
}

/// Encode the rendered logo as a self-contained ICO file (one 32bpp BMP entry
/// per requested size, no PNG entries, no external crates).
///
/// Layout: ICONDIR (6 bytes) + N x ICONDIRENTRY (16 bytes) + N image blobs.
/// Each blob is a 40-byte BITMAPINFOHEADER (with `biHeight = 2 * height`,
/// the extra half being the AND mask) followed by the bottom-up 32bpp BGRA
/// XOR bitmap and a zeroed 1bpp row-aligned AND mask (alpha lives in the
/// 32-bit channel, so the mask marks every pixel opaque).
pub fn encode_ico(sizes: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes()); // image count

    let mut offset = (6 + 16 * sizes.len()) as u32;
    let mut blobs = Vec::with_capacity(sizes.len());
    for &s in sizes {
        let blob = encode_bmp_entry(s);
        let len = blob.len() as u32;
        let dim = if s >= 256 { 0u8 } else { s as u8 }; // 256 is encoded as 0
        out.push(dim); // width
        out.push(dim); // height
        out.push(0); // palette color count
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // color planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&len.to_le_bytes()); // bytes of resource
        out.extend_from_slice(&offset.to_le_bytes()); // resource offset
        offset += len;
        blobs.push(blob);
    }
    for blob in blobs {
        out.extend_from_slice(&blob);
    }
    out
}

/// One ICO image blob: BITMAPINFOHEADER + XOR (BGRA, bottom-up) + AND mask.
fn encode_bmp_entry(size: u32) -> Vec<u8> {
    let rgba = render_rgba(size);
    let s = size;
    let xor_size = s * s * 4; // 32bpp BGRA
    let and_stride = s.div_ceil(32) * 4; // 1bpp rows padded to 4 bytes
    let and_size = and_stride * s;

    let mut buf = Vec::with_capacity((40 + xor_size + and_size) as usize);

    // BITMAPINFOHEADER
    buf.extend_from_slice(&40u32.to_le_bytes()); // biSize
    buf.extend_from_slice(&(s as i32).to_le_bytes()); // biWidth
    buf.extend_from_slice(&((s * 2) as i32).to_le_bytes()); // biHeight: XOR + AND
    buf.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    buf.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    buf.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    buf.extend_from_slice(&(xor_size + and_size).to_le_bytes()); // biSizeImage
    buf.extend_from_slice(&0u32.to_le_bytes()); // biXPelsPerMeter
    buf.extend_from_slice(&0u32.to_le_bytes()); // biYPelsPerMeter
    buf.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
    buf.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant

    // XOR bitmap: BGRA, rows bottom-up.
    for row in (0..s).rev() {
        for x in 0..s {
            let i = ((row * s + x) * 4) as usize;
            buf.push(rgba[i + 2]); // B
            buf.push(rgba[i + 1]); // G
            buf.push(rgba[i]); // R
            buf.push(rgba[i + 3]); // A
        }
    }
    // AND mask: all zero (fully opaque; transparency comes from the alpha byte).
    buf.resize(buf.len() + and_size as usize, 0);
    buf
}

/// Signed distance to a disk boundary (negative inside).
fn sd_disk(px: f32, py: f32, cx: f32, cy: f32, r: f32) -> f32 {
    let (dx, dy) = (px - cx, py - cy);
    (dx * dx + dy * dy).sqrt() - r
}

/// Map a [0, 1] float channel to a u8 with rounding.
fn unit_u8(v: f32) -> u8 {
    (v * 255.0 + 0.5).min(255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_16_has_exact_byte_length() {
        assert_eq!(render_rgba(16).len(), 16 * 16 * 4);
    }

    #[test]
    fn center_is_opaque_dark_pupil_and_corners_transparent() {
        let img = render_rgba(16);
        let px = |x: u32, y: u32| -> [u8; 4] {
            let i = ((y * 16 + x) * 4) as usize;
            [img[i], img[i + 1], img[i + 2], img[i + 3]]
        };

        // Center: fully opaque and dark (pupil).
        let c = px(8, 8);
        assert_eq!(c[3], 255, "center alpha");
        let lum = (c[0] as u32 + c[1] as u32 + c[2] as u32) / 3;
        assert!(lum < 80, "center should be dark pupil, luminance {lum}");

        // All four corners: fully transparent margin.
        for (x, y) in [(0, 0), (15, 0), (0, 15), (15, 15)] {
            assert_eq!(px(x, y)[3], 0, "corner ({x},{y}) alpha");
        }
    }

    #[test]
    fn ico_structure_is_self_consistent() {
        let sizes = [16u32, 32, 256];
        let ico = encode_ico(&sizes);

        // ICONDIR: reserved=0, type=1 (icon), count=3, all little-endian.
        assert_eq!(&ico[0..6], &[0, 0, 1, 0, 3, 0]);

        let mut offset = (6 + 16 * sizes.len()) as usize;
        for (i, &s) in sizes.iter().enumerate() {
            let e = 6 + 16 * i;
            let dim = if s >= 256 { 0u8 } else { s as u8 };
            assert_eq!(ico[e], dim, "entry {i} width");
            assert_eq!(ico[e + 1], dim, "entry {i} height");
            assert_eq!(u16::from_le_bytes([ico[e + 6], ico[e + 7]]), 32, "bpp");

            let res_len = u32::from_le_bytes(ico[e + 8..e + 12].try_into().unwrap()) as usize;
            let res_off = u32::from_le_bytes(ico[e + 12..e + 16].try_into().unwrap()) as usize;
            assert_eq!(res_off, offset, "entry {i} offset");

            // BITMAPINFOHEADER inside the blob.
            assert_eq!(
                u32::from_le_bytes(ico[offset..offset + 4].try_into().unwrap()),
                40
            );
            assert_eq!(
                i32::from_le_bytes(ico[offset + 8..offset + 12].try_into().unwrap()),
                (s * 2) as i32,
                "biHeight = 2 * height"
            );

            let and_stride = (s.div_ceil(32) * 4) as usize;
            assert_eq!(
                res_len,
                40 + s as usize * s as usize * 4 + and_stride * s as usize
            );
            offset += res_len;
        }
        assert_eq!(offset, ico.len(), "total length matches directory walk");
    }
}
