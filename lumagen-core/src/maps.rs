//! Procedural PBR map generator — port of the mockup's canvas renderer.
//! One shared layout (seeded) → 8 pixel-aligned maps as RGBA8 buffers.

use egui::Color32;

use crate::data::MapId;

pub const TEX_SIZE: usize = 400;
pub const THUMB_SIZE: usize = 64;

/// Encode an RGBA8 buffer (`side`² pixels) as a PNG byte stream. Provider reference images
/// must be real PNGs — raw RGBA labeled as PNG is not one.
pub fn encode_png_rgba(rgba: &[u8], side: usize) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, side as u32, side as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(out)
}

/// Stretch a decoded image to a square at its LONGER side, as RGBA8 `(pixels, side)`.
/// For image-EDIT results this is the registration-preserving normalization: the model's
/// output maps to the square reference frame — cropping would cut content relative to the
/// albedo.
pub fn stretch_square_rgba(img: &image::DynamicImage) -> (Vec<u8>, usize) {
    let (w, h) = (img.width(), img.height());
    let side = w.max(h).max(1);
    // Square input converts straight from the borrow — a `clone()` to unify the branches would
    // copy the full decoded buffer (up to ~1 GiB at 16K²) a second time.
    let resized = if w == h {
        img.to_rgba8()
    } else {
        img.resize_exact(side, side, image::imageops::FilterType::Lanczos3).into_rgba8()
    };
    (resized.into_raw(), side as usize)
}

/// Center-crop a decoded image to its largest square at NATIVE resolution, as RGBA8.
/// Returns `(pixels, side)`. This is the "master" — full model output, kept for export.
pub fn crop_square_rgba(img: &image::DynamicImage) -> (Vec<u8>, usize) {
    let (w, h) = (img.width(), img.height());
    let side = w.min(h).max(1);
    // Square input converts straight from the borrow (see stretch_square_rgba).
    let cropped = if w == h {
        img.to_rgba8()
    } else {
        img.crop_imm((w - side) / 2, (h - side) / 2, side, side).into_rgba8()
    };
    (cropped.into_raw(), side as usize)
}

/// Resize a square RGBA8 buffer (`src` side) to `dst` side (Lanczos3). No-op when equal.
pub fn resize_square_rgba(rgba: &[u8], src: usize, dst: usize) -> Vec<u8> {
    if src == dst || dst == 0 {
        return rgba.to_vec();
    }
    // SIMD Lanczos3 via fast_image_resize — same filter and backend as export's resize, so
    // previews and exported files agree.
    use fast_image_resize as fr;
    let Ok(src_img) = fr::images::Image::from_vec_u8(src as u32, src as u32, rgba.to_vec(), fr::PixelType::U8x4) else {
        // Mismatched buffer/side: hand back the input unscaled (callers validate sizes
        // upstream).
        return rgba.to_vec();
    };
    let mut dst_img = fr::images::Image::new(dst as u32, dst as u32, fr::PixelType::U8x4);
    let mut resizer = fr::Resizer::new();
    match resizer.resize(
        &src_img,
        &mut dst_img,
        &fr::ResizeOptions::new().resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3)),
    ) {
        Ok(()) => dst_img.into_vec(),
        Err(e) => {
            tracing::warn!(src, dst, "SIMD resize failed ({e}); returning source unscaled");
            rgba.to_vec()
        }
    }
}

/// Fit a decoded image to the square working size: center-crop to the largest square, then
/// resize to `size`² RGBA8. Providers return whatever resolution/aspect the model produced
/// (e.g. 1024×768); cropping rather than stretching keeps the texture undistorted.
pub fn fit_to_working_rgba(img: &image::DynamicImage, size: usize) -> Vec<u8> {
    let (w, h) = (img.width(), img.height());
    let side = w.min(h).max(1);
    // Square input crops nothing — convert (or resize) straight from the borrow
    // (see stretch_square_rgba).
    if w == h {
        return if side as usize == size {
            img.to_rgba8().into_raw()
        } else {
            img.resize_exact(size as u32, size as u32, image::imageops::FilterType::Lanczos3)
                .into_rgba8()
                .into_raw()
        };
    }
    let cropped = img.crop_imm((w - side) / 2, (h - side) / 2, side, side);
    let resized = if side as usize == size {
        cropped.into_rgba8()
    } else {
        cropped
            .resize_exact(size as u32, size as u32, image::imageops::FilterType::Lanczos3)
            .into_rgba8()
    };
    resized.into_raw()
}

// ── PRNG (mulberry32, same as mockup) ────────────────────────────────────

pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(seed)
    }
    pub fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x6D2B79F5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 0x3D));
        ((t ^ (t >> 14)) as f64 / 4294967296.0) as f32
    }
    #[allow(dead_code)]
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.next_f32() * (hi - lo)
    }
}

// ── Layout ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum PlateClass {
    Vent,
    Glass,
    Metal,
    Painted,
}

#[derive(Clone, Copy)]
pub struct Plate {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub cls: PlateClass,
    pub tone: f32,
    #[allow(dead_code)]
    pub rough: f32,
}

#[derive(Clone, Copy)]
pub struct Strip {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy)]
pub struct Led {
    pub x: f32,
    pub y: f32,
    pub color: Color32,
}

#[derive(Clone, Copy)]
pub struct Scratch {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

pub struct Layout {
    pub plates: Vec<Plate>,
    pub rivets: Vec<(f32, f32)>,
    pub strips: Vec<Strip>,
    pub leds: Vec<Led>,
    pub scratches: Vec<Scratch>,
    pub warn: Plate,
}

pub fn build_layout(seed: u32) -> Layout {
    let mut r = Rng::new(seed);
    let nx = 3 + (r.next_f32() * 2.0).floor() as usize;
    let ny = 3 + (r.next_f32() * 2.0).floor() as usize;

    let mut cols = vec![0.0f32];
    for i in 1..nx {
        cols.push(i as f32 / nx as f32 + (r.next_f32() - 0.5) * 0.05);
    }
    cols.push(1.0);
    let mut rows = vec![0.0f32];
    for i in 1..ny {
        rows.push(i as f32 / ny as f32 + (r.next_f32() - 0.5) * 0.05);
    }
    rows.push(1.0);

    let mut plates = Vec::new();
    for i in 0..nx {
        for j in 0..ny {
            let t = r.next_f32();
            let mut cls = if t < 0.10 {
                PlateClass::Vent
            } else if t < 0.20 {
                PlateClass::Glass
            } else {
                PlateClass::Metal
            };
            if cls == PlateClass::Metal && r.next_f32() < 0.45 {
                cls = PlateClass::Painted;
            }
            plates.push(Plate {
                x: cols[i],
                y: rows[j],
                w: cols[i + 1] - cols[i],
                h: rows[j + 1] - rows[j],
                cls,
                tone: r.next_f32() - 0.5,
                rough: 0.3 + r.next_f32() * 0.4,
            });
        }
    }

    let mut rivets = Vec::new();
    for p in &plates {
        if p.cls == PlateClass::Metal || p.cls == PlateClass::Painted {
            let pd = 0.022;
            for (x, y) in [
                (p.x + pd, p.y + pd),
                (p.x + p.w - pd, p.y + pd),
                (p.x + pd, p.y + p.h - pd),
                (p.x + p.w - pd, p.y + p.h - pd),
            ] {
                if r.next_f32() < 0.85 {
                    rivets.push((x, y));
                }
            }
        }
    }

    let a = plates[(r.next_f32() * plates.len() as f32).floor() as usize % plates.len()];
    let b = plates[(r.next_f32() * plates.len() as f32).floor() as usize % plates.len()];
    let strips = vec![
        Strip {
            x: a.x + 0.12 * a.w,
            y: a.y + a.h * 0.5,
            w: a.w * 0.76,
            h: 0.016,
        },
        Strip {
            x: b.x + b.w * 0.5,
            y: b.y + 0.12 * b.h,
            w: 0.016,
            h: b.h * 0.76,
        },
    ];

    let led_cols = [
        Color32::from_rgb(0x8f, 0xe3, 0xff),
        Color32::from_rgb(0x7d, 0xff, 0xc4),
        Color32::from_rgb(0xff, 0xcf, 0x6b),
        Color32::from_rgb(0xff, 0x8f, 0x8f),
    ];
    let mut leds = Vec::new();
    for _ in 0..7 {
        leds.push(Led {
            x: 0.08 + r.next_f32() * 0.84,
            y: 0.08 + r.next_f32() * 0.84,
            color: led_cols[(r.next_f32() * 4.0).floor() as usize % 4],
        });
    }

    let mut scratches = Vec::new();
    for _ in 0..26 {
        let x = r.next_f32();
        let y = r.next_f32();
        let an = r.next_f32() * std::f32::consts::TAU;
        let l = 0.02 + r.next_f32() * 0.07;
        scratches.push(Scratch {
            x1: x,
            y1: y,
            x2: x + an.cos() * l,
            y2: y + an.sin() * l,
        });
    }

    let warn = plates[(r.next_f32() * plates.len() as f32).floor() as usize % plates.len()];

    Layout {
        plates,
        rivets,
        strips,
        leds,
        scratches,
        warn,
    }
}

// ── CPU canvas ───────────────────────────────────────────────────────────

pub struct Canvas {
    pub px: Vec<u8>,
    pub size: usize,
    /// Bytes per row — `size * 4` for this packed square RGBA8 buffer, carried explicitly so
    /// a padded layout could be introduced without touching every accessor.
    pub stride: usize,
    pub alpha: bool,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            px: vec![0; TEX_SIZE * TEX_SIZE * 4],
            size: TEX_SIZE,
            stride: TEX_SIZE * 4,
            alpha: false,
        }
    }
}

impl Canvas {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn fill(&mut self, c: Color32) {
        // `px` is always size²·4 bytes, so `as_chunks_mut`'s remainder is statically empty.
        let (pixels, _) = self.px.as_chunks_mut::<4>();
        let px = [c.r(), c.g(), c.b(), c.a()];
        for p in pixels {
            *p = px;
        }
    }
    #[inline]
    fn blend(&mut self, x: i32, y: i32, c: Color32, a: f32) {
        if x < 0 || y < 0 || x >= self.size as i32 || y >= self.size as i32 {
            return;
        }
        let i = y as usize * self.stride + x as usize * 4;
        let ia = 1.0 - a;
        self.px[i] = (c.r() as f32 * a + self.px[i] as f32 * ia).min(255.0) as u8;
        self.px[i + 1] = (c.g() as f32 * a + self.px[i + 1] as f32 * ia).min(255.0) as u8;
        self.px[i + 2] = (c.b() as f32 * a + self.px[i + 2] as f32 * ia).min(255.0) as u8;
        self.px[i + 3] = if self.alpha {
            (c.a() as f32 * a + self.px[i + 3] as f32 * ia).min(255.0) as u8
        } else {
            255
        };
    }
    #[inline]
    fn set(&mut self, x: i32, y: i32, c: Color32) {
        if x < 0 || y < 0 || x >= self.size as i32 || y >= self.size as i32 {
            return;
        }
        let i = y as usize * self.stride + x as usize * 4;
        self.px[i] = c.r();
        self.px[i + 1] = c.g();
        self.px[i + 2] = c.b();
        self.px[i + 3] = c.a();
    }
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color32) {
        self.rect_alpha(x, y, w, h, c, 1.0);
    }
    pub fn rect_alpha(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color32, a: f32) {
        let x0 = x.floor() as i32;
        let y0 = y.floor() as i32;
        let x1 = (x + w).ceil() as i32;
        let y1 = (y + h).ceil() as i32;
        for yy in y0..y1 {
            for xx in x0..x1 {
                self.blend(xx, yy, c, a);
            }
        }
    }
    pub fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color32, lw: f32, a: f32) {
        self.rect_alpha(x, y, w, lw, c, a);
        self.rect_alpha(x, y + h - lw, w, lw, c, a);
        self.rect_alpha(x, y, lw, h, c, a);
        self.rect_alpha(x + w - lw, y, lw, h, c, a);
    }
    /// Visit every pixel whose center falls inside the disc at (`cx`, `cy`) with radius
    /// `rad`, passing `(canvas, x, y, dx, dist)` where `dx` is the x-offset from the center.
    fn for_each_in_disc(&mut self, cx: f32, cy: f32, rad: f32, mut f: impl FnMut(&mut Self, i32, i32, f32, f32)) {
        let x0 = (cx - rad).floor() as i32;
        let y0 = (cy - rad).floor() as i32;
        let x1 = (cx + rad).ceil() as i32;
        let y1 = (cy + rad).ceil() as i32;
        for yy in y0..=y1 {
            for xx in x0..=x1 {
                let dx = xx as f32 + 0.5 - cx;
                let dy = yy as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt();
                if d < rad {
                    f(self, xx, yy, dx, d);
                }
            }
        }
    }
    pub fn circle(&mut self, cx: f32, cy: f32, rad: f32, c: Color32, a: f32) {
        self.for_each_in_disc(cx, cy, rad, |s, xx, yy, _dx, d| {
            let edge = ((rad - d) / 0.8).min(1.0);
            s.blend(xx, yy, c, a * edge);
        });
    }
    /// Radial gradient circle (center alpha a → edge 0).
    pub fn radial(&mut self, cx: f32, cy: f32, rad: f32, c: Color32, a: f32) {
        self.for_each_in_disc(cx, cy, rad, |s, xx, yy, _dx, d| s.blend(xx, yy, c, a * (1.0 - d / rad)));
    }
    /// Horizontal gradient circle between two colors (left → right).
    pub fn circle_grad_x(&mut self, cx: f32, cy: f32, rad: f32, c0: Color32, c1: Color32) {
        self.for_each_in_disc(cx, cy, rad, |s, xx, yy, dx, _d| {
            let t = ((dx / rad) * 0.5 + 0.5).clamp(0.0, 1.0);
            let cc = Color32::from_rgb(lerp(c0.r(), c1.r(), t), lerp(c0.g(), c1.g(), t), lerp(c0.b(), c1.b(), t));
            s.set(xx, yy, cc);
        });
    }
    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, c: Color32, lw: f32, a: f32) {
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = (dx * dx + dy * dy).sqrt().max(0.001);
        let steps = (len * 2.0).ceil() as i32;
        let half = lw / 2.0;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let px = x1 + dx * t;
            let py = y1 + dy * t;
            self.rect_alpha(px - half, py - half, lw, lw, c, a);
        }
    }
    /// Box blur. Non-positive radii are a no-op: a negative radius has an empty sample
    /// window (its per-pixel count divisor would be zero) and radius 0 is identity.
    pub fn blur(&mut self, radius: i32) {
        if radius <= 0 {
            return;
        }
        let n = self.size;
        let stride = self.stride;
        let mut tmp = vec![0u8; self.px.len()];
        // horizontal
        for y in 0..n {
            for x in 0..n {
                let mut acc = [0u32; 4];
                let mut cnt = 0u32;
                for k in -radius..=radius {
                    let xx = x as i32 + k;
                    if xx >= 0 && xx < n as i32 {
                        let i = y * stride + xx as usize * 4;
                        acc[0] += self.px[i] as u32;
                        acc[1] += self.px[i + 1] as u32;
                        acc[2] += self.px[i + 2] as u32;
                        acc[3] += self.px[i + 3] as u32;
                        cnt += 1;
                    }
                }
                let i = y * stride + x * 4;
                tmp[i] = (acc[0] / cnt) as u8;
                tmp[i + 1] = (acc[1] / cnt) as u8;
                tmp[i + 2] = (acc[2] / cnt) as u8;
                tmp[i + 3] = (acc[3] / cnt) as u8;
            }
        }
        // vertical
        let mut out = vec![0u8; tmp.len()];
        for y in 0..n {
            for x in 0..n {
                let mut acc = [0u32; 4];
                let mut cnt = 0u32;
                for k in -radius..=radius {
                    let yy = y as i32 + k;
                    if yy >= 0 && yy < n as i32 {
                        let i = yy as usize * stride + x * 4;
                        acc[0] += tmp[i] as u32;
                        acc[1] += tmp[i + 1] as u32;
                        acc[2] += tmp[i + 2] as u32;
                        acc[3] += tmp[i + 3] as u32;
                        cnt += 1;
                    }
                }
                let i = y * stride + x * 4;
                out[i] = (acc[0] / cnt) as u8;
                out[i + 1] = (acc[1] / cnt) as u8;
                out[i + 2] = (acc[2] / cnt) as u8;
                out[i + 3] = (acc[3] / cnt) as u8;
            }
        }
        self.px = out;
    }
    /// Deterministic speckle noise.
    pub fn speckle(&mut self, count: usize, amount: f32, seed: u32) {
        let mut r = Rng::new(seed ^ 0x5EED);
        let n = self.size * self.size;
        for _ in 0..count {
            // next_f32's f64→f32 narrowing can round to exactly 1.0 (seed-deterministic,
            // e.g. 2506854331), which would index one past the buffer — hence the `% n`.
            let idx = (r.next_f32() * n as f32).floor() as usize % n;
            let v = (r.next_f32() - 0.5) * amount * 255.0;
            let i = idx * 4;
            for ch in 0..3 {
                self.px[i + ch] = (self.px[i + ch] as f32 + v).clamp(0.0, 255.0) as u8;
            }
        }
    }
}

#[inline]
fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t) as u8
}

fn gy(v: f32) -> Color32 {
    let n = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(n, n, n)
}

fn hsl(h: f32, s: f32, l: f32) -> Color32 {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - ((hp % 2.0) - 1.0).abs());
    let (r1, g1, b1) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    Color32::from_rgb(((r1 + m) * 255.0) as u8, ((g1 + m) * 255.0) as u8, ((b1 + m) * 255.0) as u8)
}

// ── Map rendering ────────────────────────────────────────────────────────

#[tracing::instrument(skip(layout), fields(?id, seed))]
pub fn render_map(id: MapId, layout: &Layout, seed: u32) -> Vec<u8> {
    let s = TEX_SIZE as f32;
    let p = |v: f32| v * s;
    let mut c = Canvas::new();
    let l = layout;

    match id {
        MapId::Albedo => {
            c.fill(Color32::from_rgb(0x2f, 0x34, 0x3b));
            for pl in &l.plates {
                let col = match pl.cls {
                    PlateClass::Painted => hsl(212.0, 0.13, 0.60 + pl.tone * 0.14),
                    PlateClass::Metal => hsl(214.0, 0.11, 0.40 + pl.tone * 0.13),
                    PlateClass::Vent => Color32::from_rgb(0x21, 0x24, 0x29),
                    PlateClass::Glass => Color32::from_rgb(0x16, 0x20, 0x3a),
                };
                c.rect(p(pl.x), p(pl.y), p(pl.w), p(pl.h), col);
                if pl.cls == PlateClass::Vent {
                    let mut yy = p(pl.y) + 6.0;
                    while yy < p(pl.y) + p(pl.h) - 4.0 {
                        c.line(p(pl.x) + 4.0, yy, p(pl.x) + p(pl.w) - 4.0, yy, Color32::BLACK, 2.0, 0.55);
                        yy += 7.0;
                    }
                }
            }
            // hazard stripes
            let w = l.warn;
            let (wa, wb, ww, wh) = (p(w.x), p(w.y), p(w.w), p(w.h));
            let hx = wa + ww * 0.5;
            let hy = wb + wh * 0.62;
            let hw = ww * 0.44;
            let hh = wh * 0.3;
            let mut i = -(hh as i32);
            while (i as f32) < hw {
                let yellow = ((i / 12) % 2) != 0;
                let col = if yellow {
                    Color32::from_rgb(0xd9, 0xa4, 0x41)
                } else {
                    Color32::from_rgb(0x1c, 0x1a, 0x12)
                };
                // clipped parallelogram stripe
                for yy in 0..(hh as i32) {
                    let t = yy as f32 / hh;
                    let sx = hx + i as f32 + t * hh;
                    let x_start = sx.max(hx);
                    let x_end = (sx + 7.0).min(hx + hw);
                    if x_end > x_start {
                        c.rect(x_start, hy + yy as f32, x_end - x_start, 1.0, col);
                    }
                }
                i += 12;
            }
            // seams
            for pl in &l.plates {
                c.stroke_rect(p(pl.x) + 0.5, p(pl.y) + 0.5, p(pl.w) - 1.0, p(pl.h) - 1.0, Color32::BLACK, 1.4, 0.5);
            }
            for pl in &l.plates {
                c.stroke_rect(p(pl.x) + 1.5, p(pl.y) + 1.5, p(pl.w) - 3.0, p(pl.h) - 3.0, Color32::WHITE, 1.0, 0.05);
            }
            for rv in &l.rivets {
                c.circle(p(rv.0), p(rv.1), 3.0, Color32::BLACK, 0.4);
                c.circle(p(rv.0) - 0.8, p(rv.1) - 0.8, 1.3, Color32::WHITE, 0.12);
            }
            for st in &l.strips {
                c.rect(p(st.x), p(st.y), p(st.w), p(st.h), Color32::from_rgb(0xbf, 0xea, 0xff));
            }
            for led in &l.leds {
                c.circle(p(led.x), p(led.y), 1.7, led.color, 1.0);
            }
            for sc in &l.scratches {
                c.line(p(sc.x1), p(sc.y1), p(sc.x2), p(sc.y2), Color32::WHITE, 1.0, 0.06);
            }
            c.speckle(2200, 0.05, seed);
        }
        MapId::Roughness => {
            c.fill(gy(0.5));
            for pl in &l.plates {
                let mut v = match pl.cls {
                    PlateClass::Painted => 0.72,
                    PlateClass::Metal => 0.36,
                    PlateClass::Vent => 0.8,
                    PlateClass::Glass => 0.12,
                };
                v += pl.tone * 0.06;
                c.rect(p(pl.x), p(pl.y), p(pl.w), p(pl.h), gy(v));
            }
            for pl in &l.plates {
                c.stroke_rect(p(pl.x) + 0.5, p(pl.y) + 0.5, p(pl.w) - 1.0, p(pl.h) - 1.0, Color32::BLACK, 1.4, 0.35);
            }
            for rv in &l.rivets {
                c.circle(p(rv.0), p(rv.1), 3.0, gy(0.28), 1.0);
            }
            for st in &l.strips {
                c.rect(p(st.x), p(st.y), p(st.w), p(st.h), gy(0.25));
            }
            for sc in &l.scratches {
                c.line(p(sc.x1), p(sc.y1), p(sc.x2), p(sc.y2), Color32::BLACK, 1.0, 0.3);
            }
            c.speckle(1600, 0.05, seed);
        }
        MapId::Metallic => {
            c.fill(Color32::BLACK);
            for pl in &l.plates {
                let v = match pl.cls {
                    PlateClass::Metal => 0.96,
                    PlateClass::Vent => 0.9,
                    PlateClass::Painted => 0.08,
                    PlateClass::Glass => 0.0,
                };
                c.rect(p(pl.x), p(pl.y), p(pl.w), p(pl.h), gy(v));
                if pl.cls == PlateClass::Painted {
                    c.stroke_rect(p(pl.x) + 0.5, p(pl.y) + 0.5, p(pl.w) - 1.0, p(pl.h) - 1.0, gy(0.85), 1.4, 1.0);
                }
            }
            for rv in &l.rivets {
                c.circle(p(rv.0), p(rv.1), 2.6, gy(0.95), 1.0);
            }
        }
        MapId::Normal => {
            c.fill(Color32::from_rgb(128, 128, 255));
            for pl in &l.plates {
                let (a, b, w, h) = (p(pl.x), p(pl.y), p(pl.w), p(pl.h));
                c.line(a, b, a + w, b, Color32::from_rgb(150, 128, 255), 2.4, 1.0);
                c.line(a, b, a, b + h, Color32::from_rgb(150, 128, 255), 2.4, 1.0);
                c.line(a + w, b, a + w, b + h, Color32::from_rgb(106, 128, 255), 2.4, 1.0);
                c.line(a, b + h, a + w, b + h, Color32::from_rgb(106, 128, 255), 2.4, 1.0);
            }
            for rv in &l.rivets {
                c.circle_grad_x(p(rv.0), p(rv.1), 3.0, Color32::from_rgb(96, 128, 255), Color32::from_rgb(160, 128, 255));
            }
            for pl in &l.plates {
                if pl.cls == PlateClass::Vent {
                    let (a, b, w, h) = (p(pl.x), p(pl.y), p(pl.w), p(pl.h));
                    let mut yy = b + 6.0;
                    while yy < b + h - 4.0 {
                        c.line(a + 4.0, yy, a + w - 4.0, yy, Color32::from_rgb(150, 120, 255), 1.6, 1.0);
                        c.line(a + 4.0, yy + 2.0, a + w - 4.0, yy + 2.0, Color32::from_rgb(110, 136, 255), 1.6, 1.0);
                        yy += 7.0;
                    }
                }
            }
            for sc in &l.scratches {
                c.line(p(sc.x1), p(sc.y1), p(sc.x2), p(sc.y2), Color32::from_rgb(150, 128, 255), 1.0, 0.5);
            }
        }
        MapId::Displacement => {
            c.fill(gy(0.5));
            for pl in &l.plates {
                let mut v = match pl.cls {
                    PlateClass::Painted => 0.62,
                    PlateClass::Metal => 0.55,
                    PlateClass::Vent => 0.3,
                    PlateClass::Glass => 0.4,
                };
                v += pl.tone * 0.05;
                c.rect(p(pl.x), p(pl.y), p(pl.w), p(pl.h), gy(v));
            }
            let w = l.warn;
            c.rect(p(w.x) + p(w.w) * 0.2, p(w.y) + p(w.h) * 0.2, p(w.w) * 0.6, p(w.h) * 0.6, gy(0.72));
            for pl in &l.plates {
                c.stroke_rect(p(pl.x) + 1.0, p(pl.y) + 1.0, p(pl.w) - 2.0, p(pl.h) - 2.0, gy(0.24), 4.0, 1.0);
            }
            c.blur(1);
        }
        MapId::Ao => {
            c.fill(gy(0.96));
            for pl in &l.plates {
                c.stroke_rect(p(pl.x), p(pl.y), p(pl.w), p(pl.h), Color32::BLACK, 6.0, 0.45);
            }
            for pl in &l.plates {
                if pl.cls == PlateClass::Vent || pl.cls == PlateClass::Glass {
                    let v = if pl.cls == PlateClass::Vent { 0.5 } else { 0.62 };
                    c.rect(p(pl.x) + 2.0, p(pl.y) + 2.0, p(pl.w) - 4.0, p(pl.h) - 4.0, gy(v));
                }
            }
            for rv in &l.rivets {
                c.radial(p(rv.0), p(rv.1), 6.0, Color32::BLACK, 0.5);
            }
            c.blur(1);
        }
        MapId::Emission => {
            c.fill(Color32::BLACK);
            for st in &l.strips {
                c.radial(
                    p(st.x) + p(st.w) / 2.0,
                    p(st.y) + p(st.h) / 2.0,
                    p(st.w).max(p(st.h)),
                    Color32::from_rgb(0x8f, 0xe3, 0xff),
                    0.5,
                );
                c.rect(p(st.x), p(st.y), p(st.w), p(st.h), Color32::from_rgb(0xa9, 0xec, 0xff));
            }
            for led in &l.leds {
                c.radial(p(led.x), p(led.y), 6.0, led.color, 0.55);
                c.circle(p(led.x), p(led.y), 2.0, led.color, 1.0);
            }
        }
        MapId::Transparency => {
            c.alpha = true;
            c.fill(Color32::TRANSPARENT);
            for pl in &l.plates {
                if pl.cls != PlateClass::Glass {
                    let col = if pl.cls == PlateClass::Vent { gy(0.85) } else { Color32::WHITE };
                    c.rect(p(pl.x), p(pl.y), p(pl.w), p(pl.h), col);
                }
            }
            for pl in &l.plates {
                if pl.cls != PlateClass::Glass {
                    c.stroke_rect(p(pl.x) + 0.5, p(pl.y) + 0.5, p(pl.w) - 1.0, p(pl.h) - 1.0, Color32::WHITE, 2.0, 0.5);
                }
            }
        }
    }
    c.px
}

/// Nearest-neighbor downscale to thumbnail size.
pub fn make_thumb(px: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; THUMB_SIZE * THUMB_SIZE * 4];
    for y in 0..THUMB_SIZE {
        for x in 0..THUMB_SIZE {
            // Rational center sample, floor((x + 0.5) · TEX/THUMB): the ratio is not integral
            // (400/64 = 6.25), so a truncated integer scale would never sample the right/bottom
            // ~4% of the source.
            let sx = (x * TEX_SIZE + TEX_SIZE / 2) / THUMB_SIZE;
            let sy = (y * TEX_SIZE + TEX_SIZE / 2) / THUMB_SIZE;
            let si = (sy * TEX_SIZE + sx) * 4;
            let di = (y * THUMB_SIZE + x) * 4;
            out[di..di + 4].copy_from_slice(&px[si..si + 4]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    /// RGB (not RGBA) gradient so the helpers' format-conversion path is exercised too.
    fn gradient_rgb(w: u32, h: u32) -> image::DynamicImage {
        image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |x, y| image::Rgb([x as u8, y as u8, 7])))
    }

    #[test]
    fn stretch_square_rgba_passes_square_input_through() {
        let (px, side) = stretch_square_rgba(&gradient_rgb(3, 3));
        assert_eq!(side, 3);
        assert_eq!(px.len(), 3 * 3 * 4);
        assert_eq!(&px[..4], &[0, 0, 7, 255]);
        assert_eq!(&px[(2 * 3 + 2) * 4..], &[2, 2, 7, 255]);
    }

    #[test]
    fn crop_square_rgba_center_crops_the_longer_axis() {
        let (px, side) = crop_square_rgba(&gradient_rgb(4, 2));
        assert_eq!(side, 2);
        // Center crop keeps columns 1..3 of the 4×2 source.
        assert_eq!(px, vec![1, 0, 7, 255, 2, 0, 7, 255, 1, 1, 7, 255, 2, 1, 7, 255]);
    }

    #[test]
    fn fit_to_working_rgba_passes_square_input_at_working_size_through() {
        let px = fit_to_working_rgba(&gradient_rgb(3, 3), 3);
        assert_eq!(px.len(), 3 * 3 * 4);
        assert_eq!(&px[..4], &[0, 0, 7, 255]);
        assert_eq!(&px[(2 * 3 + 2) * 4..], &[2, 2, 7, 255]);
    }

    /// Seed 2506854331's first draw makes `next_f32` round to exactly 1.0 (the f64→f32
    /// narrowing rounds quotients within 2⁻²⁵ of 1.0 up), which unguarded indexes one past
    /// the pixel buffer.
    #[test]
    fn speckle_stays_in_bounds_when_next_f32_rounds_to_one() {
        let mut c = Canvas::new();
        c.speckle(2200, 0.05, 2_506_854_331);
        assert_eq!(c.px.len(), TEX_SIZE * TEX_SIZE * 4);
    }

    #[rstest]
    #[case::negative(-3)]
    #[case::zero(0)]
    fn blur_with_non_positive_radius_is_identity(#[case] radius: i32) {
        let mut c = Canvas::new();
        c.fill(Color32::from_rgb(40, 90, 160));
        c.rect(4.0, 4.0, 40.0, 20.0, Color32::WHITE);
        let before = c.px.clone();
        c.blur(radius);
        assert_eq!(c.px, before);
    }

    /// The 400/64 scale is not integral (6.25): truncating it to 6 samples only source
    /// pixels ≤ 381, cropping the right/bottom edge out of every thumbnail.
    #[test]
    fn make_thumb_samples_the_full_source_extent() {
        let mut px = vec![0u8; TEX_SIZE * TEX_SIZE * 4];
        for y in 0..TEX_SIZE {
            for x in 0..TEX_SIZE {
                if x >= 382 || y >= 382 {
                    px[(y * TEX_SIZE + x) * 4..][..4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
        }
        let thumb = make_thumb(&px);
        let corner = ((THUMB_SIZE - 1) * THUMB_SIZE + (THUMB_SIZE - 1)) * 4;
        assert_eq!(
            &thumb[corner..corner + 4],
            &[255; 4],
            "bottom-right thumb pixel must sample the source's edge region"
        );
        let right = (32 * THUMB_SIZE + (THUMB_SIZE - 1)) * 4;
        assert_eq!(&thumb[right..right + 4], &[255; 4], "right thumb column must sample the source's right edge");
        let bottom = ((THUMB_SIZE - 1) * THUMB_SIZE + 32) * 4;
        assert_eq!(&thumb[bottom..bottom + 4], &[255; 4], "bottom thumb row must sample the source's bottom edge");
    }
}
