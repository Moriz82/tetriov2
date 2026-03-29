//! Screenshot capture — platform-specific implementations.

#[cfg(target_os = "linux")]
mod platform {
    use std::process::Command;

    /// Capture a specific screen region via grim (Wayland). Returns RGBA pixels.
    pub fn capture_region(x: u32, y: u32, w: u32, h: u32) -> Option<(Vec<u8>, u32, u32)> {
        let geometry = format!("{},{} {}x{}", x, y, w, h);
        let output = Command::new("grim")
            .args(["-g", &geometry, "-t", "ppm", "-"])
            .output()
            .ok()?;
        if !output.status.success() {
            eprintln!("[grim] stderr: {}", String::from_utf8_lossy(&output.stderr));
            return None;
        }
        super::parse_ppm(&output.stdout)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::process::Command;

    /// Capture a specific screen region via macOS screencapture. Returns RGBA pixels.
    pub fn capture_region(x: u32, y: u32, w: u32, h: u32) -> Option<(Vec<u8>, u32, u32)> {
        // screencapture -R x,y,w,h -t png -x (silent) to stdout via temp file
        let tmp = "/tmp/tetrio-bot-capture.png";
        let rect = format!("{},{},{},{}", x, y, w, h);
        let status = Command::new("screencapture")
            .args(["-R", &rect, "-t", "png", "-x", tmp])
            .status()
            .ok()?;
        if !status.success() { return None; }

        // Read PNG and decode to RGBA
        let png_data = std::fs::read(tmp).ok()?;
        super::decode_png(&png_data)
    }
}

pub use platform::capture_region;

fn parse_ppm(data: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let mut newlines = 0;
    let mut header_end = 0;
    for (i, &b) in data.iter().enumerate() {
        if b == b'\n' {
            newlines += 1;
            if newlines == 3 {
                header_end = i + 1;
                break;
            }
        }
    }
    if newlines < 3 { return None; }

    let header = std::str::from_utf8(&data[..header_end]).ok()?;
    let mut lines = header.lines();
    let magic = lines.next()?;
    if magic != "P6" { return None; }

    let dims = loop {
        let line = lines.next()?;
        if !line.starts_with('#') { break line; }
    };
    let mut parts = dims.split_whitespace();
    let w: u32 = parts.next()?.parse().ok()?;
    let h: u32 = parts.next()?.parse().ok()?;

    let pixel_data = &data[header_end..];
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for chunk in pixel_data.chunks(3) {
        if chunk.len() == 3 {
            rgba.push(chunk[0]);
            rgba.push(chunk[1]);
            rgba.push(chunk[2]);
            rgba.push(255);
        }
    }
    Some((rgba, w, h))
}

/// Minimal PNG decoder — handles uncompressed IDAT for screencapture output.
/// Falls back to sips conversion if needed.
#[cfg(target_os = "macos")]
fn decode_png(png_data: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    // Convert PNG to PPM via sips + pipe, then parse
    use std::process::Command;
    let tmp_ppm = "/tmp/tetrio-bot-capture.ppm";
    let status = Command::new("sips")
        .args(["-s", "format", "ppm", "/tmp/tetrio-bot-capture.png",
               "--out", tmp_ppm])
        .output()
        .ok()?;
    if !status.status.success() { return None; }
    let ppm_data = std::fs::read(tmp_ppm).ok()?;
    parse_ppm(&ppm_data)
}
