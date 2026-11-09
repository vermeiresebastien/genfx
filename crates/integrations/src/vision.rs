//! On-device vision inference via **ONNX Runtime** (`ort` crate, statically
//! linked — no runtime DLL ships). Feature `vision`.
//!
//! Implements **monocular depth** (G173) for the Depth Anything v2 ONNX export:
//!   input  `pixel_values`    f32 [1,3,H,W] ImageNet-normalized RGB
//!   output `predicted_depth` f32 [1,H,W]   relative depth (higher = nearer)
//!
//! Model path resolves like `flux_catalog::models`: `FLUX_MODEL_DEPTH` dir, else
//! `FLUX_MODELS_DIR/depth`, containing `model.onnx`.

#![cfg(feature = "vision")]

use std::path::{Path, PathBuf};

use ort::session::Session;
use ort::value::Value;

pub struct DepthModel {
    session: Session,
    input_name: String,
    /// Fixed square side the model is driven at (Depth Anything default 518).
    fixed: usize,
}

#[derive(Debug, Clone)]
pub struct DepthResult {
    pub width: usize,
    pub height: usize,
    /// Depth 0..1 normalized across the frame (1 = nearest).
    pub depth: Vec<f32>,
}

impl DepthModel {
    /// Load a model from an explicit .onnx path, driving it at `fixed x fixed`.
    pub fn from_onnx(path: &Path, fixed: usize) -> Result<Self, String> {
        let session = Session::builder()
            .and_then(|b| b.commit_from_file(path))
            .map_err(|e| format!("load onnx: {e}"))?;
        // First input name (Depth Anything: "pixel_values").
        let input_name = session
            .inputs
            .first()
            .map(|i| i.name.clone())
            .ok_or("model has no inputs")?;
        Ok(Self {
            session,
            input_name,
            fixed,
        })
    }

    /// Resolve and load the depth model from the environment.
    pub fn load_default() -> Result<Self, String> {
        let dir = depth_model_dir().ok_or_else(|| {
            "no depth model dir (set FLUX_MODEL_DEPTH or FLUX_MODELS_DIR)".to_string()
        })?;
        let onnx = find_onnx(&dir).ok_or_else(|| format!("no .onnx in {}", dir.display()))?;
        let size = std::env::var("FLUX_DEPTH_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(518);
        Self::from_onnx(&onnx, size)
    }

    /// Run depth inference on an RGB8 image (`width*height*3` bytes), resized to
    /// the model's fixed square input internally.
    pub fn infer_rgb(
        &mut self,
        rgb: &[u8],
        width: usize,
        height: usize,
    ) -> Result<DepthResult, String> {
        let side = self.fixed.max(14);
        let buf = preprocess(rgb, width, height, side, side);
        let input = Value::from_array(([1usize, 3, side, side], buf))
            .map_err(|e| format!("input tensor: {e}"))?;
        let outputs = self
            .session
            .run(ort::inputs![self.input_name.as_str() => input])
            .map_err(|e| format!("run: {e}"))?;
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("output: {e}"))?;
        let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
        let (oh, ow, is4d) = match dims.as_slice() {
            [1, h, w] => (*h, *w, false),
            [1, 1, h, w] => (*h, *w, true),
            _ => return Err(format!("unexpected depth shape {dims:?}")),
        };
        let mut depth = Vec::with_capacity(oh * ow);
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for y in 0..oh {
            for x in 0..ow {
                let d = if is4d {
                    data[y * ow + x]
                } else {
                    data[y * ow + x]
                };
                depth.push(d);
                min = min.min(d);
                max = max.max(d);
            }
        }
        let range = (max - min).max(1e-6);
        for d in &mut depth {
            *d = (*d - min) / range;
        }
        Ok(DepthResult {
            width: ow,
            height: oh,
            depth,
        })
    }
}

fn depth_model_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("FLUX_MODEL_DEPTH") {
        return Some(PathBuf::from(d));
    }
    if let Ok(base) = std::env::var("FLUX_MODELS_DIR") {
        let d = Path::new(&base).join("depth");
        if d.exists() {
            return Some(d);
        }
    }
    None
}

fn find_onnx(dir: &Path) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("FLUX_DEPTH_ONNX") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    for name in ["model.onnx", "depth.onnx"] {
        let p = dir.join(name);
        if p.exists() {
            return Some(p);
        }
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().map(|x| x == "onnx").unwrap_or(false))
}

/// Bilinear RGB resize + ImageNet normalization into NCHW f32.
fn preprocess(rgb: &[u8], w: usize, h: usize, tw: usize, th: usize) -> Vec<f32> {
    let mean = [0.485f32, 0.456, 0.406];
    let std = [0.229f32, 0.224f32, 0.225f32];
    let mut out = vec![0f32; 3 * th * tw];
    for ty in 0..th {
        let sy = (ty as f32 + 0.5) * (h as f32 / th as f32) - 0.5;
        for tx in 0..tw {
            let sx = (tx as f32 + 0.5) * (w as f32 / tw as f32) - 0.5;
            let (r, g, b) = bilinear(rgb, w, h, sx, sy);
            let idx = ty * tw + tx;
            out[idx] = (r / 255.0 - mean[0]) / std[0];
            out[th * tw + idx] = (g / 255.0 - mean[1]) / std[1];
            out[2 * th * tw + idx] = (b / 255.0 - mean[2]) / std[2];
        }
    }
    out
}

fn bilinear(rgb: &[u8], w: usize, h: usize, x: f32, y: f32) -> (f32, f32, f32) {
    let x0 = x.floor().max(0.0) as isize;
    let y0 = y.floor().max(0.0) as isize;
    let x1 = (x0 + 1).min(w as isize - 1);
    let y1 = (y0 + 1).min(h as isize - 1);
    let x0 = x0.min(w as isize - 1);
    let y0 = y0.min(h as isize - 1);
    let fx = (x - x0 as f32).clamp(0.0, 1.0);
    let fy = (y - y0 as f32).clamp(0.0, 1.0);
    let px = |xx: isize, yy: isize| -> (f32, f32, f32) {
        let i = (yy as usize * w + xx as usize) * 3;
        (rgb[i] as f32, rgb[i + 1] as f32, rgb[i + 2] as f32)
    };
    let p00 = px(x0, y0);
    let p10 = px(x1, y0);
    let p01 = px(x0, y1);
    let p11 = px(x1, y1);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let top = (
        lerp(p00.0, p10.0, fx),
        lerp(p00.1, p10.1, fx),
        lerp(p00.2, p10.2, fx),
    );
    let bot = (
        lerp(p01.0, p11.0, fx),
        lerp(p01.1, p11.1, fx),
        lerp(p01.2, p11.2, fx),
    );
    (
        lerp(top.0, bot.0, fy),
        lerp(top.1, bot.1, fy),
        lerp(top.2, bot.2, fy),
    )
}

/// Map normalized depth (0..1, nearer=higher) to grayscale RGBA8 (near = bright).
pub fn depth_to_rgba(depth: &DepthResult) -> Vec<u8> {
    let mut out = vec![0u8; depth.width * depth.height * 4];
    for (i, &d) in depth.depth.iter().enumerate() {
        let v = (d.clamp(0.0, 1.0) * 255.0) as u8;
        let o = i * 4;
        out[o] = v;
        out[o + 1] = v;
        out[o + 2] = v;
        out[o + 3] = 255;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_is_nchw_normalized() {
        let rgb = vec![128u8; 14 * 14 * 3];
        let buf = preprocess(&rgb, 14, 14, 14, 14);
        assert_eq!(buf.len(), 3 * 14 * 14);
        assert!(buf[0].abs() < 0.6);
    }

    #[test]
    fn depth_to_rgba_grayscale() {
        let r = DepthResult {
            width: 2,
            height: 2,
            depth: vec![0.0, 1.0, 0.5, 0.25],
        };
        let px = depth_to_rgba(&r);
        assert_eq!(px.len(), 16);
        assert_eq!(px[0], 0);
        assert_eq!(px[4], 255);
        assert_eq!(px[3], 255);
    }

    /// End-to-end real inference — runs against whatever model is configured.
    #[test]
    fn real_depth_inference_when_model_present() {
        let model = match DepthModel::load_default() {
            Ok(m) => m,
            Err(e) => {
                eprintln!("no usable depth model; skipping ({e})");
                return;
            }
        };
        let mut model = model;
        let (w, h) = (64usize, 64usize);
        let mut rgb = vec![40u8; w * h * 3];
        for y in 16..48 {
            for x in 16..48 {
                let i = (y * w + x) * 3;
                rgb[i] = 230;
                rgb[i + 1] = 230;
                rgb[i + 2] = 230;
            }
        }
        let depth = model.infer_rgb(&rgb, w, h).expect("infer");
        assert!(depth.width > 0 && depth.height > 0);
        assert_eq!(depth.depth.len(), depth.width * depth.height);
        assert!(depth.depth.iter().all(|&d| (0.0..=1.0).contains(&d)));
        let min = depth.depth.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = depth
            .depth
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(max - min > 0.1, "depth should vary across the image");
    }
}
