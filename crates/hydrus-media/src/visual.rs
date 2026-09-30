//! "Are these two images visual duplicates?" (the reference's
//! `ClientVisualData`), for duplicates auto-resolution.
//!
//! Each image gets a quick summary ([`VisualData`]: its L*a*b* histograms)
//! and a detailed one ([`VisualDataTiled`]: L*a*b* histograms of 16x16
//! tiles, and an edge map). A pair is first compared by the summaries
//! ([`similar_simple`]); only if that looks promising are the detailed ones
//! compared ([`similar_regional`]), which gives the confidence.
//!
//! The image work follows OpenCV exactly where the reference's numbers are
//! integers (blur, resize and colour conversion of 8-bit images) and in
//! OpenCV's own order of operations where they are floats; the statistics
//! are the reference's.

use crate::imaging::Raster;
use crate::imaging::cv::{self, Interpolation};
use crate::imaging::cvx;

/// `VISUAL_DUPLICATES_RESULT_*`.
pub const NOT: u8 = 0;
pub const PROBABLY: u8 = 40;
pub const VERY_PROBABLY: u8 = 60;
pub const ALMOST_CERTAINLY: u8 = 85;
pub const NEAR_PERFECT: u8 = 100;

const BLUR_SIGMA: f64 = 0.95;
const LAB_RESOLUTION: usize = 1024;
const BINS: usize = 256;
const TILES: usize = 16;
const EDGE_PERCEPTUAL: u32 = 2048;
const EDGE_RESOLUTION: usize = 256;

/// L*a*b* histograms (densities over 256 bins).
#[derive(Debug, Clone, PartialEq)]
pub struct LabHistograms {
    pub l: Vec<f32>,
    pub a: Vec<f32>,
    pub b: Vec<f32>,
}

impl LabHistograms {
    /// Not a flat colour: enough non-empty bins to compare.
    pub fn is_interesting(&self) -> bool {
        let nonzero = |h: &[f32]| h.iter().filter(|&&v| v != 0.0).count();
        nonzero(&self.l) + nonzero(&self.a) + nonzero(&self.b) > 24
    }
}

/// An image's quick summary.
#[derive(Debug, Clone, PartialEq)]
pub struct VisualData {
    pub resolution: (u32, u32),
    pub lab: LabHistograms,
    pub alpha: Option<Vec<f32>>,
}

/// An image's detailed summary.
#[derive(Debug, Clone, PartialEq)]
pub struct VisualDataTiled {
    pub resolution: (u32, u32),
    pub had_alpha: bool,
    /// Tile histograms, column by column (x outer, y inner).
    pub tiles: Vec<LabHistograms>,
    /// The edge map's R, G and B planes, 256x256.
    pub edges: [Vec<f32>; 3],
}

fn too_low_resolution(r: (u32, u32)) -> bool {
    r.0 < 32 || r.1 < 32
}

/// `numpy.histogram(..., bins=256, range=(0, 255))`'s bin for an 8-bit value.
fn bin(v: u8) -> usize {
    ((usize::from(v) * BINS) / 255).min(BINS - 1)
}

/// The counts as `density=True` gives them: count / bin width / total.
fn density(counts: &[u64; BINS]) -> Vec<f32> {
    let n: u64 = counts.iter().sum();
    let width = 255.0f64 / BINS as f64;
    counts
        .iter()
        .map(|&c| (c as f64 / width / n as f64) as f32)
        .collect()
}

/// `numpy.histogram(values, bins=256, range=(0, 255), density=True)` of
/// 8-bit values.
fn histogram(values: impl Iterator<Item = u8>) -> Vec<f32> {
    let mut counts = [0u64; BINS];
    for v in values {
        counts[bin(v)] += 1;
    }
    density(&counts)
}

/// Histograms of interleaved 8-bit L, a, b.
fn lab_histograms(lab: &[u8]) -> LabHistograms {
    let mut counts = [[0u64; BINS]; 3];
    for p in lab.chunks_exact(3) {
        for c in 0..3 {
            counts[c][bin(p[c])] += 1;
        }
    }
    LabHistograms {
        l: density(&counts[0]),
        a: density(&counts[1]),
        b: density(&counts[2]),
    }
}

/// Histograms of each tile of a square interleaved Lab image `side` pixels
/// across, column by column.
fn tile_histograms(lab: &[u8], side: usize) -> Vec<LabHistograms> {
    let tile = side / TILES;
    let mut counts = vec![[[0u64; BINS]; 3]; TILES * TILES];
    for (i, p) in lab.chunks_exact(3).enumerate() {
        let (tx, ty) = ((i % side) / tile, (i / side) / tile);
        if tx >= TILES || ty >= TILES {
            continue;
        }
        let t = &mut counts[tx * TILES + ty];
        for c in 0..3 {
            t[c][bin(p[c])] += 1;
        }
    }
    counts
        .iter()
        .map(|t| LabHistograms {
            l: density(&t[0]),
            a: density(&t[1]),
            b: density(&t[2]),
        })
        .collect()
}

/// The image's RGB and alpha planes, blurred to smooth jpeg artifacts
/// (`BlurRGBNumPy` at the reference's sigma).
fn blurred_rgb(image: &Raster) -> Raster {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let cn = usize::from(image.channels());
    let src = image.data();
    let planes: Vec<Vec<u8>> = (0..3)
        .map(|c| {
            let plane: Vec<u8> = if cn >= 3 {
                src.chunks_exact(cn).map(|p| p[c]).collect()
            } else {
                src.chunks_exact(cn).map(|p| p[0]).collect()
            };
            cvx::gaussian_blur_u8(&plane, w, h, BLUR_SIGMA)
        })
        .collect();
    let mut rgb = vec![0u8; w * h * 3];
    for i in 0..w * h {
        for c in 0..3 {
            rgb[i * 3 + c] = planes[c][i];
        }
    }
    Raster::new(w as u32, h as u32, 3, rgb).expect("sized for its dimensions")
}

/// `GenerateImageVisualDataNumPy`.
pub fn visual_data(image: &Raster) -> VisualData {
    let resolution = (image.width(), image.height());
    let (w, h) = (image.width() as usize, image.height() as usize);
    let alpha = (image.channels() == 4).then(|| {
        let plane: Vec<u8> = image.data().chunks_exact(4).map(|p| p[3]).collect();
        let blurred = cvx::gaussian_blur_u8(&plane, w, h, BLUR_SIGMA);
        let as_raster =
            Raster::new(w as u32, h as u32, 1, blurred).expect("sized for its dimensions");
        let resized = cv::resize(
            &as_raster,
            LAB_RESOLUTION as u32,
            LAB_RESOLUTION as u32,
            Interpolation::Area,
        );
        histogram(resized.data().iter().copied())
    });
    let rgb = blurred_rgb(image);
    let resized = cv::resize(
        &rgb,
        LAB_RESOLUTION as u32,
        LAB_RESOLUTION as u32,
        Interpolation::Area,
    );
    let lab = cvx::rgb_to_lab(resized.data());
    VisualData {
        resolution,
        lab: lab_histograms(&lab),
        alpha,
    }
}

/// `GenerateImageVisualDataTiledNumPy`.
pub fn visual_data_tiled(image: &Raster) -> VisualDataTiled {
    let resolution = (image.width(), image.height());
    let rgb = blurred_rgb(image);

    // the edge map, from the image scaled to fit 2048x2048 keeping its ratio
    let spec = crate::ThumbnailSpec {
        bounding: (EDGE_PERCEPTUAL, EDGE_PERCEPTUAL),
        scale: crate::ThumbnailScale::ToFit,
        dpr_percent: 100,
        ..crate::ThumbnailSpec::default()
    };
    let (ew, eh) = crate::thumbnail_resolution(Some(resolution.0), Some(resolution.1), &spec)
        .unwrap_or(resolution);
    let scaled = cv::resize(&rgb, ew, eh, Interpolation::Area);
    let (ew, eh) = (ew as usize, eh as usize);
    let float: Vec<f32> = scaled.data().iter().map(|&v| f32::from(v)).collect();
    let mut dog = vec![0f32; float.len()];
    for c in 0..3 {
        let plane: Vec<f32> = float.iter().skip(c).step_by(3).copied().collect();
        let blurred = cvx::gaussian_blur_f32(&plane, ew, eh, 10.0);
        for (i, (v, b)) in plane.iter().zip(&blurred).enumerate() {
            dog[i * 3 + c] = v - b;
        }
    }
    let edge = cvx::resize_area_f32(&dog, ew, eh, 3, EDGE_RESOLUTION, EDGE_RESOLUTION);
    let edges = [0, 1, 2].map(|c| edge.iter().skip(c).step_by(3).copied().collect());

    // tile histograms of the image scaled to 1024x1024
    let resized = cv::resize(
        &rgb,
        LAB_RESOLUTION as u32,
        LAB_RESOLUTION as u32,
        Interpolation::Area,
    );
    let lab = cvx::rgb_to_lab(resized.data());
    let tiles = tile_histograms(&lab, LAB_RESOLUTION);
    VisualDataTiled {
        resolution,
        had_alpha: image.channels() == 4,
        tiles,
        edges,
    }
}

/// `GetHistogramNormalisedWassersteinDistance`: 0 the same, 1 as different
/// as can be.
fn wasserstein(a: &[f32], b: &[f32]) -> f64 {
    let mut running = 0f32;
    let mut total = 0f64;
    for (x, y) in a.iter().zip(b) {
        running += x - y;
        total += f64::from(running.abs());
    }
    total / (a.len() - 1) as f64
}

/// `GetVisualDataWassersteinDistanceScore`: (either is interesting, score).
fn lab_score(a: &LabHistograms, b: &LabHistograms) -> (bool, f64) {
    let l = wasserstein(&a.l, &b.l);
    let aa = wasserstein(&a.a, &b.a);
    let bb = wasserstein(&a.b, &b.b);
    (
        a.is_interesting() || b.is_interesting(),
        0.6 * l + 0.2 * aa + 0.2 * bb,
    )
}

/// `FilesHaveDifferentRatio`: the ratios are not within 15%.
fn different_ratio(a: (u32, u32), b: (u32, u32)) -> bool {
    let ra = f64::from(a.0) / f64::from(a.1);
    let rb = f64::from(b.0) / f64::from(b.1);
    !(rb * 0.85 <= ra && ra <= rb * 1.15)
}

/// `FilesAreVisuallySimilarSimple`: (worth a closer look, result).
pub fn similar_simple(a: &VisualData, b: &VisualData) -> (bool, u8) {
    if different_ratio(a.resolution, b.resolution) {
        return (false, NOT);
    }
    if too_low_resolution(a.resolution) || too_low_resolution(b.resolution) {
        return (false, NOT);
    }
    match (&a.alpha, &b.alpha) {
        (Some(x), Some(y)) => {
            if wasserstein(x, y) > 0.005 {
                return (false, NOT);
            }
        }
        (None, None) => {}
        _ => return (false, NOT),
    }
    let (interesting, score) = lab_score(&a.lab, &b.lab);
    if interesting && score < 0.01 {
        (true, PROBABLY)
    } else {
        (false, NOT)
    }
}

/// Skewness of values, as `skewness_numpy`.
fn skewness(values: &[f64]) -> f64 {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    let std = var.sqrt();
    if std == 0.0 {
        return 0.0;
    }
    let third = values.iter().map(|v| (v - mean).powi(3)).sum::<f64>() / n;
    third / std.powi(3)
}

/// `FilesAreVisuallySimilarRegionalEdgeMapRaw`: (largest point difference,
/// largest absolute skew pull).
pub fn edge_map_raw(a: &VisualDataTiled, b: &VisualDataTiled) -> (f64, f64) {
    let tile = EDGE_RESOLUTION / TILES;
    let mut largest = 0f64;
    let mut pulls = Vec::with_capacity(3);
    for c in 0..3 {
        let diff: Vec<f32> = a.edges[c]
            .iter()
            .zip(&b.edges[c])
            .map(|(x, y)| x - y)
            .collect();
        let peak = diff.iter().fold(0f32, |m, v| m.max(v.abs()));
        largest = largest.max(f64::from(peak));
        let mut scores = Vec::with_capacity(TILES * TILES);
        for ty in 0..TILES {
            for tx in 0..TILES {
                let mut sum = 0f64;
                for y in ty * tile..(ty + 1) * tile {
                    for x in tx * tile..(tx + 1) * tile {
                        sum += f64::from(diff[y * EDGE_RESOLUTION + x].abs());
                    }
                }
                scores.push(sum / (tile * tile) as f64);
            }
        }
        pulls.push(skewness(&scores) * f64::from(peak));
    }
    (largest, pulls.into_iter().fold(f64::MIN, f64::max))
}

/// `FilesAreVisuallySimilarRegionalEdgeMap`.
fn edge_map_verdict(a: &VisualDataTiled, b: &VisualDataTiled) -> (bool, u8) {
    let (largest, pull) = edge_map_raw(a, b);
    if pull > 16.5 {
        return (false, NOT);
    }
    if largest < 3.0 {
        (true, NEAR_PERFECT)
    } else if largest < 11.0 {
        (true, ALMOST_CERTAINLY)
    } else if largest < 19.0 {
        (true, VERY_PROBABLY)
    } else {
        (false, NOT)
    }
}

/// The tile histogram statistics (`FilesAreVisuallySimilarRegionalLabHistogramsRaw`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileStats {
    pub max: f64,
    pub mean: f64,
    pub variance: f64,
    pub skew: f64,
    pub skew_pull: f64,
    pub mixed_perfect: bool,
    pub some_perfect: bool,
    pub none_interesting: bool,
}

pub fn tile_stats(a: &VisualDataTiled, b: &VisualDataTiled) -> TileStats {
    let data: Vec<(bool, f64)> = a
        .tiles
        .iter()
        .zip(&b.tiles)
        .map(|(x, y)| lab_score(x, y))
        .collect();
    let none_interesting = !data.iter().any(|(i, _)| *i);
    let some_perfect = data.iter().any(|(i, s)| *i && *s < 0.000_000_1);
    let scores: Vec<f64> = data.iter().map(|(_, s)| *s).collect();
    let max = scores.iter().copied().fold(f64::MIN, f64::max);
    let n = scores.len() as f64;
    let mean = scores.iter().sum::<f64>() / n;
    let variance = scores.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / n;
    let skew = skewness(&scores);
    let skew_pull = skew * max * 1000.0;
    TileStats {
        max,
        mean,
        variance,
        skew,
        skew_pull,
        mixed_perfect: some_perfect && max > 0.0001 && skew_pull > 8.0,
        some_perfect,
        none_interesting,
    }
}

/// `FilesAreVisuallySimilarRegionalLabHistograms`.
fn tiles_verdict(a: &VisualDataTiled, b: &VisualDataTiled) -> (bool, u8) {
    let s = tile_stats(a, b);
    if s.skew_pull > 50.0
        || s.variance > 0.000_003_5
        || s.mean > 0.003
        || s.max > 0.01
        || s.mixed_perfect
        || s.none_interesting
    {
        return (false, NOT);
    }
    if s.max < 0.001 && s.mean < 0.0001 && s.variance < 0.000_001 && s.skew_pull < 1.5 {
        (true, NEAR_PERFECT)
    } else if s.max < 0.004 && s.mean < 0.0015 && s.variance < 0.000_001 && s.skew_pull < 5.0 {
        (true, ALMOST_CERTAINLY)
    } else {
        (true, VERY_PROBABLY)
    }
}

/// `FilesAreVisuallySimilarRegional`: (similar, confidence).
pub fn similar_regional(a: &VisualDataTiled, b: &VisualDataTiled) -> (bool, u8) {
    if a.had_alpha != b.had_alpha {
        return (false, NOT);
    }
    if different_ratio(a.resolution, b.resolution) {
        return (false, NOT);
    }
    if too_low_resolution(a.resolution) || too_low_resolution(b.resolution) {
        return (false, NOT);
    }
    let (edge_ok, edge) = edge_map_verdict(a, b);
    if !edge_ok {
        return (false, edge);
    }
    let (lab_ok, lab) = tiles_verdict(a, b);
    if edge < lab {
        (edge_ok, edge)
    } else {
        (lab_ok, lab)
    }
}

/// The whole test (`PairComparatorRelativeVisualDuplicates.Test`): the
/// confidence that two images are visual duplicates.
pub fn visual_duplicates(a: &Raster, b: &Raster) -> u8 {
    let (simple_ok, _) = similar_simple(&visual_data(a), &visual_data(b));
    if !simple_ok {
        return NOT;
    }
    similar_regional(&visual_data_tiled(a), &visual_data_tiled(b)).1
}
