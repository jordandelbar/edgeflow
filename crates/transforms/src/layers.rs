use serde::{Deserialize, Serialize};

// Wire format: [ ndim: u8 | shape: [u32-LE; ndim] | dtype: u8 (1=f32) | data: bytes ]

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Layer {
    FloatToTensor {
        n_features: usize,
    },
    Normalize {
        mean: Vec<f32>,
        std: Vec<f32>,
    },
    ClassifierOutput {
        labels: Vec<String>,
    },
    RawTensorOutput,
    /// Decode raw JPEG/PNG bytes, resize to (width × height), normalise to [0, 1],
    /// and reorder from HWC to CHW.  Output tensor shape: [1, 3, height, width].
    ImageToTensor {
        width: usize,
        height: usize,
    },
    /// Decode a YOLO-style detection tensor ([1, 4+classes, num_boxes]), apply
    /// confidence thresholding and greedy IoU NMS, and serialise survivors to JSON.
    /// Bounding box coordinates are normalised to [0, 1] by dividing by model_size.
    DetectionOutput {
        labels: Vec<String>,
        conf_threshold: f32,
        iou_threshold: f32,
        /// The square input size the model was trained on (e.g. 640 for YOLOv8).
        model_size: usize,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PipelineConfig {
    pub steps: Vec<Layer>,
}

pub fn run_pipeline(config: &PipelineConfig, input: Vec<u8>) -> Vec<u8> {
    let mut data = input;
    for step in &config.steps {
        data = run_layer(step, data);
    }
    data
}

fn run_layer(layer: &Layer, input: Vec<u8>) -> Vec<u8> {
    match layer {
        Layer::FloatToTensor { n_features } => float_to_tensor(*n_features, &input),
        Layer::Normalize { mean, std } => normalize(mean, std, &input),
        Layer::ClassifierOutput { labels } => classifier_output(labels, &input),
        Layer::RawTensorOutput => input,
        Layer::ImageToTensor { width, height } => image_to_tensor(*width, *height, &input),
        Layer::DetectionOutput {
            labels,
            conf_threshold,
            iou_threshold,
            model_size,
        } => detection_output(labels, *conf_threshold, *iou_threshold, *model_size, &input),
    }
}

fn encode_tensor(shape: &[usize], data: &[f32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + shape.len() * 4 + 1 + data.len() * 4);
    buf.push(shape.len() as u8);
    for &dim in shape {
        buf.extend_from_slice(&(dim as u32).to_le_bytes());
    }
    buf.push(1u8); // dtype = f32
    for &v in data {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf
}

fn decode_tensor(buf: &[u8]) -> (Vec<usize>, Vec<f32>) {
    let mut pos = 0;
    let ndim = buf[pos] as usize;
    pos += 1;
    let mut shape = Vec::with_capacity(ndim);
    for _ in 0..ndim {
        let dim = u32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize;
        shape.push(dim);
        pos += 4;
    }
    pos += 1; // skip dtype
    let values = buf[pos..]
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    (shape, values)
}

fn float_to_tensor(n_features: usize, raw: &[u8]) -> Vec<u8> {
    let data: Vec<f32> = raw
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    encode_tensor(&[1, n_features], &data)
}

fn normalize(mean: &[f32], std: &[f32], input: &[u8]) -> Vec<u8> {
    let (shape, data) = decode_tensor(input);
    let n = mean.len();
    let normalized: Vec<f32> = data
        .iter()
        .enumerate()
        .map(|(i, &v)| (v - mean[i % n]) / std[i % n])
        .collect();
    encode_tensor(&shape, &normalized)
}

fn classifier_output(labels: &[String], input: &[u8]) -> Vec<u8> {
    let (_, probs) = decode_tensor(input);
    let (class_id, &confidence) = probs
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or((0, &0.0));
    let label = labels
        .get(class_id)
        .map(|s| s.as_str())
        .unwrap_or("unknown");
    let confidence = (confidence * 10000.0).round() / 10000.0;
    format!(r#"{{"class_id":{class_id},"label":"{label}","confidence":{confidence}}}"#).into_bytes()
}

fn image_to_tensor(width: usize, height: usize, input: &[u8]) -> Vec<u8> {
    use image::imageops::FilterType;

    let img = image::load_from_memory(input)
        .expect("image_to_tensor: failed to decode image bytes")
        .to_rgb8();
    let img = image::imageops::resize(&img, width as u32, height as u32, FilterType::CatmullRom);
    let pixels = img.as_raw(); // HWC, u8 [0, 255]

    let spatial = height * width;
    let mut chw = vec![0f32; 3 * spatial];
    for y in 0..height {
        for x in 0..width {
            let src = (y * width + x) * 3;
            let dst = y * width + x;
            chw[dst] = pixels[src] as f32 / 255.0; // R
            chw[spatial + dst] = pixels[src + 1] as f32 / 255.0; // G
            chw[2 * spatial + dst] = pixels[src + 2] as f32 / 255.0; // B
        }
    }

    encode_tensor(&[1, 3, height, width], &chw)
}

/// Intersection-over-Union for two axis-aligned boxes in (x1,y1,x2,y2) format.
fn iou(ax1: f32, ay1: f32, ax2: f32, ay2: f32, bx1: f32, by1: f32, bx2: f32, by2: f32) -> f32 {
    let ix1 = ax1.max(bx1);
    let iy1 = ay1.max(by1);
    let ix2 = ax2.min(bx2);
    let iy2 = ay2.min(by2);
    let inter = (ix2 - ix1).max(0.0) * (iy2 - iy1).max(0.0);
    if inter == 0.0 {
        return 0.0;
    }
    let area_a = (ax2 - ax1) * (ay2 - ay1);
    let area_b = (bx2 - bx1) * (by2 - by1);
    inter / (area_a + area_b - inter)
}

fn detection_output(
    labels: &[String],
    conf_threshold: f32,
    iou_threshold: f32,
    model_size: usize,
    input: &[u8],
) -> Vec<u8> {
    let (shape, data) = decode_tensor(input);
    assert_eq!(
        shape.len(),
        3,
        "detection_output: expected 3-D tensor [batch, 4+classes, boxes]"
    );
    let num_preds = shape[1]; // 4 bbox coords + num_classes
    let num_boxes = shape[2];
    assert!(
        num_preds > 4,
        "detection_output: tensor must have at least 5 prediction rows (4 bbox + 1 class)"
    );
    let num_classes = num_preds - 4;
    let scale = model_size as f32;

    struct Det {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        confidence: f32,
        class_id: usize,
    }

    // Decode boxes and apply confidence threshold.
    // Memory layout: data[pred_row * num_boxes + box_idx]
    let mut dets: Vec<Det> = Vec::new();
    for i in 0..num_boxes {
        let xc = data[0 * num_boxes + i];
        let yc = data[1 * num_boxes + i];
        let w = data[2 * num_boxes + i];
        let h = data[3 * num_boxes + i];

        let (class_id, confidence) = (0..num_classes)
            .map(|j| (j, data[(4 + j) * num_boxes + i]))
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or((0, 0.0));

        if confidence < conf_threshold {
            continue;
        }

        dets.push(Det {
            x1: (xc - w / 2.0) / scale,
            y1: (yc - h / 2.0) / scale,
            x2: (xc + w / 2.0) / scale,
            y2: (yc + h / 2.0) / scale,
            confidence,
            class_id,
        });
    }

    // Greedy NMS: sort by confidence descending, suppress high-IoU duplicates.
    dets.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let n = dets.len();
    let mut suppressed = vec![false; n];
    let mut kept: Vec<usize> = Vec::new();
    for i in 0..n {
        if suppressed[i] {
            continue;
        }
        kept.push(i);
        for j in (i + 1)..n {
            if suppressed[j] {
                continue;
            }
            let a = &dets[i];
            let b = &dets[j];
            if iou(a.x1, a.y1, a.x2, a.y2, b.x1, b.y1, b.x2, b.y2) >= iou_threshold {
                suppressed[j] = true;
            }
        }
    }

    // Serialise surviving detections to a JSON array.
    let entries: Vec<String> = kept
        .iter()
        .map(|&i| {
            let d = &dets[i];
            let label = labels.get(d.class_id).map(|s| s.as_str()).unwrap_or("unknown");
            let conf = (d.confidence * 10000.0).round() / 10000.0;
            format!(
                r#"{{"class_id":{},"label":"{}","confidence":{},"bbox":[{:.4},{:.4},{:.4},{:.4}]}}"#,
                d.class_id, label, conf, d.x1, d.y1, d.x2, d.y2
            )
        })
        .collect();

    format!("[{}]", entries.join(",")).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── helpers ────────────────────────────────────────────────────────────────

    /// Encode a solid-colour image as PNG bytes.
    fn solid_png(width: u32, height: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
        use image::codecs::png::PngEncoder;
        use image::{ExtendedColorType, ImageEncoder};
        let pixels: Vec<u8> = (0..height)
            .flat_map(|_| (0..width).flat_map(|_| [r, g, b]))
            .collect();
        let mut buf = Vec::new();
        PngEncoder::new(&mut buf)
            .write_image(&pixels, width, height, ExtendedColorType::Rgb8)
            .unwrap();
        buf
    }

    /// Build a tensor in [1, 4+num_classes, num_boxes] layout from a list of
    /// (xc, yc, w, h, class_id, confidence) tuples (coords in model-pixel space).
    fn detection_tensor(boxes: &[(f32, f32, f32, f32, usize, f32)], num_classes: usize) -> Vec<u8> {
        let num_boxes = boxes.len();
        let num_preds = 4 + num_classes;
        let mut data = vec![0f32; num_preds * num_boxes];
        for (i, &(xc, yc, w, h, class_id, conf)) in boxes.iter().enumerate() {
            data[0 * num_boxes + i] = xc;
            data[1 * num_boxes + i] = yc;
            data[2 * num_boxes + i] = w;
            data[3 * num_boxes + i] = h;
            data[(4 + class_id) * num_boxes + i] = conf;
        }
        encode_tensor(&[1, num_preds, num_boxes], &data)
    }

    // ── ImageToTensor ──────────────────────────────────────────────────────────

    #[test]
    fn image_to_tensor_output_shape() {
        let png = solid_png(100, 80, 128, 64, 32);
        let out = image_to_tensor(640, 640, &png);
        let (shape, data) = decode_tensor(&out);
        assert_eq!(shape, vec![1, 3, 640, 640]);
        assert_eq!(data.len(), 3 * 640 * 640);
    }

    #[test]
    fn image_to_tensor_white_normalises_to_one() {
        let png = solid_png(4, 4, 255, 255, 255);
        let out = image_to_tensor(4, 4, &png);
        let (_, data) = decode_tensor(&out);
        for v in &data {
            assert!((v - 1.0).abs() < 1e-5, "expected 1.0, got {v}");
        }
    }

    #[test]
    fn image_to_tensor_black_normalises_to_zero() {
        let png = solid_png(4, 4, 0, 0, 0);
        let out = image_to_tensor(4, 4, &png);
        let (_, data) = decode_tensor(&out);
        for v in &data {
            assert!(v.abs() < 1e-5, "expected 0.0, got {v}");
        }
    }

    #[test]
    fn image_to_tensor_chw_channel_ordering() {
        // Pure-red image: R=255, G=0, B=0
        let png = solid_png(2, 2, 255, 0, 0);
        let out = image_to_tensor(2, 2, &png);
        let (shape, data) = decode_tensor(&out);
        assert_eq!(shape, vec![1, 3, 2, 2]);
        let spatial = 2 * 2;
        // R plane (indices 0..spatial) should be ~1.0
        for i in 0..spatial {
            assert!((data[i] - 1.0).abs() < 1e-5, "R[{i}]={}", data[i]);
        }
        // G plane should be 0.0
        for i in spatial..2 * spatial {
            assert!(data[i].abs() < 1e-5, "G[{i}]={}", data[i]);
        }
        // B plane should be 0.0
        for i in 2 * spatial..3 * spatial {
            assert!(data[i].abs() < 1e-5, "B[{i}]={}", data[i]);
        }
    }

    #[test]
    fn image_to_tensor_resize_changes_shape() {
        // Input is 10×20; target is 4×4 — shape must reflect target, not source.
        let png = solid_png(10, 20, 100, 150, 200);
        let out = image_to_tensor(4, 4, &png);
        let (shape, _) = decode_tensor(&out);
        assert_eq!(shape, vec![1, 3, 4, 4]);
    }

    // ── DetectionOutput ────────────────────────────────────────────────────────

    #[test]
    fn detection_output_basic_detection() {
        let labels = vec!["cat".to_string(), "dog".to_string()];
        let tensor = detection_tensor(&[(320.0, 320.0, 128.0, 128.0, 0, 0.9)], 2);
        let out = detection_output(&labels, 0.5, 0.7, 640, &tensor);
        let json = String::from_utf8(out).unwrap();
        assert!(json.contains(r#""label":"cat""#));
        assert!(json.contains(r#""class_id":0"#));
        assert!(json.contains(r#""confidence":0.9"#));
    }

    #[test]
    fn detection_output_respects_conf_threshold() {
        let labels = vec!["cat".to_string()];
        let tensor = detection_tensor(
            &[
                (100.0, 100.0, 50.0, 50.0, 0, 0.8), // above
                (200.0, 200.0, 50.0, 50.0, 0, 0.3), // below
            ],
            1,
        );
        let out = detection_output(&labels, 0.5, 0.7, 640, &tensor);
        let dets: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let arr = dets.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["confidence"].as_f64().unwrap(), 0.8);
    }

    #[test]
    fn detection_output_nms_suppresses_overlapping_boxes() {
        let labels = vec!["cat".to_string()];
        // Two near-identical boxes + one far box.
        let tensor = detection_tensor(
            &[
                (320.0, 320.0, 200.0, 200.0, 0, 0.9), // kept (highest conf)
                (322.0, 322.0, 200.0, 200.0, 0, 0.8), // suppressed (heavy overlap)
                (550.0, 550.0, 50.0, 50.0, 0, 0.85),  // kept (no overlap)
            ],
            1,
        );
        let out = detection_output(&labels, 0.5, 0.5, 640, &tensor);
        let dets: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(dets.as_array().unwrap().len(), 2);
    }

    #[test]
    fn detection_output_empty_when_all_below_threshold() {
        let labels = vec!["cat".to_string()];
        let tensor = detection_tensor(&[(320.0, 320.0, 128.0, 128.0, 0, 0.1)], 1);
        let out = detection_output(&labels, 0.5, 0.7, 640, &tensor);
        assert_eq!(out, b"[]");
    }

    #[test]
    fn detection_output_bbox_normalised_coords() {
        // Box centred at (320, 320), w=h=128 in 640×640 space.
        // Expected xyxy: x1=256/640, y1=256/640, x2=384/640, y2=384/640
        let labels = vec!["cat".to_string()];
        let tensor = detection_tensor(&[(320.0, 320.0, 128.0, 128.0, 0, 0.9)], 1);
        let out = detection_output(&labels, 0.5, 0.7, 640, &tensor);
        let dets: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let bbox = &dets[0]["bbox"];
        let x1 = bbox[0].as_f64().unwrap() as f32;
        let y1 = bbox[1].as_f64().unwrap() as f32;
        let x2 = bbox[2].as_f64().unwrap() as f32;
        let y2 = bbox[3].as_f64().unwrap() as f32;
        assert!((x1 - 256.0 / 640.0).abs() < 1e-3, "x1={x1}");
        assert!((y1 - 256.0 / 640.0).abs() < 1e-3, "y1={y1}");
        assert!((x2 - 384.0 / 640.0).abs() < 1e-3, "x2={x2}");
        assert!((y2 - 384.0 / 640.0).abs() < 1e-3, "y2={y2}");
    }

    #[test]
    fn detection_output_unknown_label_fallback() {
        let labels = vec!["cat".to_string()]; // only 1 label, class_id=1 is out of range
        let tensor = detection_tensor(&[(320.0, 320.0, 64.0, 64.0, 1, 0.9)], 2);
        let out = detection_output(&labels, 0.5, 0.7, 640, &tensor);
        let json = String::from_utf8(out).unwrap();
        assert!(json.contains(r#""label":"unknown""#));
    }

    #[test]
    fn detection_output_sorted_by_confidence() {
        let labels = vec!["cat".to_string()];
        let tensor = detection_tensor(
            &[
                (100.0, 100.0, 40.0, 40.0, 0, 0.6),
                (300.0, 300.0, 40.0, 40.0, 0, 0.95),
                (500.0, 100.0, 40.0, 40.0, 0, 0.75),
            ],
            1,
        );
        let out = detection_output(&labels, 0.5, 0.7, 640, &tensor);
        let dets: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let arr = dets.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        // Must be in descending confidence order
        let confs: Vec<f64> = arr
            .iter()
            .map(|d| d["confidence"].as_f64().unwrap())
            .collect();
        assert!(
            confs[0] >= confs[1] && confs[1] >= confs[2],
            "not sorted: {confs:?}"
        );
    }

    // ── iou ────────────────────────────────────────────────────────────────────

    #[test]
    fn iou_identical_boxes_is_one() {
        assert!((iou(0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn iou_non_overlapping_boxes_is_zero() {
        assert_eq!(iou(0.0, 0.0, 1.0, 1.0, 2.0, 2.0, 3.0, 3.0), 0.0);
    }

    #[test]
    fn iou_half_overlap() {
        // Box A: (0,0)-(2,1), Box B: (1,0)-(3,1) — 50% overlap
        let v = iou(0.0, 0.0, 2.0, 1.0, 1.0, 0.0, 3.0, 1.0);
        assert!((v - 1.0 / 3.0).abs() < 1e-6, "iou={v}");
    }
}
