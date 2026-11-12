//! The Windows icon of a file / folder / exe as a PNG (port of `iconForPath` in `src/main/launch/meta.ts`).
use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY};

use crate::win::{on_com_thread, sha1_hex, wide};

/// Icon edge in pixels.
pub const ICON_SIZE: i32 = 64;

/// Writes the shell icon of `path` to `<icons_dir>\<sha1(path.lower)>.png` and returns that file name.
/// `None` if Windows has no icon for it. Runs its own COM thread, so it is safe to call from anywhere (it blocks).
pub fn icon_for_path(path: &Path, icons_dir: &Path) -> Option<String> {
    let p = path.to_string_lossy().into_owned();
    let (w, h, rgba) = on_com_thread(move || shell_icon_rgba(&p, ICON_SIZE)).flatten()?;
    let file = format!("{}.png", sha1_hex(&path.to_string_lossy().to_lowercase()));
    let png = encode_png(w, h, &rgba)?;
    std::fs::create_dir_all(icons_dir).ok()?;
    std::fs::write(icons_dir.join(&file), png).ok()?;
    Some(file)
}

/// RGBA pixels (top-down, straight alpha) of the shell icon. Must run on a COM-initialized thread.
fn shell_icon_rgba(path: &str, size: i32) -> Option<(u32, u32, Vec<u8>)> {
    let wpath = wide(path);
    // SAFETY: plain COM/GDI calls; the HBITMAP is deleted before returning.
    unsafe {
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(PCWSTR(wpath.as_ptr()), None).ok()?;
        let hbmp = factory.GetImage(SIZE { cx: size, cy: size }, SIIGBF_ICONONLY).ok()?;
        let out = hbitmap_rgba(hbmp);
        let _ = DeleteObject(HGDIOBJ::from(hbmp));
        out
    }
}

unsafe fn hbitmap_rgba(hbmp: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    let mut bm = BITMAP::default();
    let got = unsafe {
        GetObjectW(HGDIOBJ::from(hbmp), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _))
    };
    if got == 0 || bm.bmWidth <= 0 || bm.bmHeight == 0 {
        return None;
    }
    let (w, h) = (bm.bmWidth, bm.bmHeight.abs());
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bgra = vec![0u8; (w * h * 4) as usize];
    let lines = unsafe {
        let dc = GetDC(None);
        let n = GetDIBits(dc, hbmp, 0, h as u32, Some(bgra.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        n
    };
    if lines != h {
        return None;
    }
    let rgba = bgra_to_rgba(&bgra);
    if rgba.chunks_exact(4).all(|p| p[3] == 0) {
        return None; // blank image
    }
    Some((w as u32, h as u32, rgba))
}

/// BGRA (possibly premultiplied, possibly without alpha) → straight RGBA.
pub fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let has_alpha = bgra.chunks_exact(4).any(|p| p[3] != 0);
    // Shell icons come back premultiplied; only un-premultiply when every pixel is consistent with that.
    let premultiplied = has_alpha && bgra.chunks_exact(4).all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
    let mut out = Vec::with_capacity(bgra.len());
    for p in bgra.chunks_exact(4) {
        let (b, g, r) = (p[0], p[1], p[2]);
        if !has_alpha {
            out.extend_from_slice(&[r, g, b, 255]); // a 24-bit style bitmap: fully opaque
            continue;
        }
        let a = p[3];
        if premultiplied && a > 0 && a < 255 {
            let un = |c: u8| ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
            out.extend_from_slice(&[un(r), un(g), un(b), a]);
        } else {
            out.extend_from_slice(&[r, g, b, a]);
        }
    }
    out
}

fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut buf, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(buf)
}
