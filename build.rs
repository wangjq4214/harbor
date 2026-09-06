use std::{env, fs::File, path::PathBuf};

const WINDOW_ICON_SIZE: u32 = 256;
const ICON_SIZES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, WINDOW_ICON_SIZE];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=assets/harbor.png");

    let output_dir = PathBuf::from(env::var("OUT_DIR")?);
    let source = image::open("assets/harbor.png")?.into_rgba8();
    let window_icon = image::imageops::resize(
        &source,
        WINDOW_ICON_SIZE,
        WINDOW_ICON_SIZE,
        image::imageops::FilterType::Lanczos3,
    );
    window_icon.save_with_format(
        output_dir.join("harbor-window.png"),
        image::ImageFormat::Png,
    )?;

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }

    let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);
    for size in ICON_SIZES {
        let resized =
            image::imageops::resize(&source, size, size, image::imageops::FilterType::Lanczos3);
        let image = ico::IconImage::from_rgba_data(size, size, resized.into_raw());
        icon_dir.add_entry(ico::IconDirEntry::encode_as_png(&image)?);
    }

    let icon_path = output_dir.join("harbor.ico");
    icon_dir.write(File::create(&icon_path)?)?;
    winresource::WindowsResource::new()
        .set_icon(icon_path.to_str().ok_or("non-Unicode icon output path")?)
        .compile()?;
    Ok(())
}
