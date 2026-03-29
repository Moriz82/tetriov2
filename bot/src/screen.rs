//! Screenshot capture via grim (Wayland/Hyprland)
//! Captures only a specific region for speed on multi-monitor setups.

use std::process::Command;

/// Capture a specific screen region. Returns RGBA pixels.
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
    parse_ppm(&output.stdout)
}

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
