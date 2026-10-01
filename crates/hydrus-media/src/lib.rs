//! Everything needed to import a file, as a pure library: detect its type
//! from content, read its metadata, hash it, make a thumbnail, and compute
//! the similarity hashes — with results identical to the reference client's
//! (see `README.md` for the exactness guarantees per item).
//!
//! There is no global state and no database access. [`MediaTools`] holds only
//! the ffmpeg configuration, is `Clone + Send + Sync`, and every method can
//! run concurrently (e.g. on a rayon pool).
//!
//! ```no_run
//! use hydrus_media::{MediaTools, ThumbnailSpec};
//! # fn main() -> Result<(), hydrus_media::MediaError> {
//! let tools = MediaTools::new();
//! let analysis = tools.analyse("some/file.jpg".as_ref(), &ThumbnailSpec::default())?;
//! println!("{:?} {:?}", analysis.info.mime, analysis.hashes.sha256);
//! # Ok(()) }
//! ```
//!
//! Module map:
//! - [`detect`](MediaTools::detect_mime): content sniffing (`GetMime`).
//! - [`MediaTools::inspect`]: size, resolution, duration, frames, audio, words (`GetFileInfo`).
//! - [`hashes`]: sha256/md5/sha1/sha512 in one pass.
//! - [`MediaTools::load_image`]: the Pillow-equivalent decode behind pixel
//!   hashes, perceptual hashes and thumbnails.
//! - [`perceptual_hash`], [`blurhash()`], [`thumbnail_resolution`].

pub mod animation;
mod blurhash;
mod detect;
pub mod encode;
pub mod error;
pub mod ffmpeg;
mod formats;
pub mod hashes;
mod imaging;
pub mod jpeg;
pub mod mimes;
mod phash;
mod text;
mod thumbnail;
mod tools;
pub mod visual;

pub use blurhash::blurhash;
pub use detect::set_comic_book_detection;
pub use error::MediaError;
pub use ffmpeg::Ffmpeg;
pub use hashes::{FileHashes, hash_bytes, hash_file};
pub use imaging::{Raster, TransparencyStrictness, set_transparency_strictness};
pub use phash::{BLANK_PERCEPTUAL_HASH, is_blank as is_blank_perceptual_hash, perceptual_hash};
pub use thumbnail::{
    Thumbnail, ThumbnailFormat, ThumbnailScale, ThumbnailSpec, thumbnail_resolution,
};
pub use tools::{Analysis, FileFlags, FileInfo, MediaTools};

/// Decode image bytes the way the reference's `GenerateNumPyImage` does
/// (EXIF rotation, sRGB colour management, RGB/RGBA, useless alpha dropped).
///
/// This covers formats decoded natively (JPEG, PNG, GIF, WebP, BMP, ICO,
/// TIFF, QOI); use [`MediaTools::load_image`] for everything else.
pub fn decode_image(data: &[u8]) -> error::Result<Raster> {
    tools::raster_from_bytes(data, true)
}

/// OpenCV-exact resizes, exposed for tests and callers that need the
/// reference's exact resampling (`cv2.resize` on u8 images).
pub mod resample {
    use crate::imaging::cv;
    pub use crate::imaging::cv::Interpolation;
    pub use crate::imaging::cvx::{
        gaussian_blur_f32, gaussian_blur_u8, resize_area_f32, rgb_to_lab,
    };

    /// `cv2.resize(image, (width, height), interpolation=...)`.
    pub fn resize(
        image: &crate::Raster,
        width: u32,
        height: u32,
        interpolation: Interpolation,
    ) -> crate::Raster {
        cv::resize(image, width, height, interpolation)
    }

    /// `cv2.cvtColor(image, cv2.COLOR_RGB2GRAY)`.
    pub fn rgb_to_gray(image: &crate::Raster) -> crate::Raster {
        cv::rgb_to_gray(image)
    }
}
