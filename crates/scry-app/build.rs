use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=assets/skry.svg");
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("Missing Cargo output directory")?);
    let source = fs::read("assets/skry.svg")?;
    let tree = resvg::usvg::Tree::from_data(&source, &resvg::usvg::Options::default())?;
    let mut icons = ico::IconDir::new(ico::ResourceType::Icon);
    for size in [16_u16, 20, 24, 32, 40, 48, 64, 128, 256] {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(u32::from(size), u32::from(size))
            .ok_or("Cannot allocate icon pixmap")?;
        let scale = f32::from(size) / tree.size().width();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        // Rune and ICO both take straight-alpha sRGB, not tiny-skia's premultiplied data.
        let rgba: Vec<u8> = pixmap
            .pixels()
            .iter()
            .flat_map(|pixel| {
                let pixel = pixel.demultiply();
                [pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()]
            })
            .collect();
        if size == 128 {
            fs::write(out.join("skry.rgba"), &rgba)?;
        }
        let image = ico::IconImage::from_rgba_data(u32::from(size), u32::from(size), rgba);
        icons.add_entry(ico::IconDirEntry::encode(&image)?);
    }
    let icon_path = out.join("skry.ico");
    icons.write(fs::File::create(&icon_path)?)?;
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon(icon_path.to_str().ok_or("Icon path is not UTF-8")?)
            .set("ProductName", "Skry")
            .set("FileDescription", "Skry streaming app")
            .compile()?;
    }
    Ok(())
}
