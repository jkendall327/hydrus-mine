//! Oracle bridge: decode reference input and export concrete native objects.
use hydrus_downloader_exchange::{decode_png, decode_text, encode_png, encode_text};
use std::{env, fs};
fn main() -> anyhow::Result<()> {
    let args = env::args().collect::<Vec<_>>();
    anyhow::ensure!(args.len() == 4, "exchange INPUT OUTPUT.json OUTPUT.png");
    let input = fs::read(&args[1])?;
    let definitions = if input.starts_with(b"\x89PNG") {
        decode_png(&input)?
    } else {
        decode_text(std::str::from_utf8(&input)?)?
    };
    fs::write(&args[2], encode_text(&definitions)?)?;
    fs::write(&args[3], encode_png(&definitions)?)?;
    Ok(())
}
