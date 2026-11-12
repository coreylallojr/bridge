use bridge_sys::icons::{bgra_to_rgba, icon_for_path};

const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

#[test]
fn calculator_icon_is_written_as_a_png_named_by_the_lowercase_path_hash() {
    let dir = tempfile::tempdir().unwrap();
    let exe = std::path::Path::new(r"C:\Windows\System32\calc.exe");
    let file = icon_for_path(exe, dir.path()).expect("calc.exe has an icon");
    // sha1("c:\windows\system32\calc.exe")
    assert_eq!(file.len(), 40 + 4);
    assert!(file.ends_with(".png"));
    let same = icon_for_path(std::path::Path::new(r"c:\WINDOWS\system32\CALC.EXE"), dir.path()).unwrap();
    assert_eq!(file, same, "the name ignores case");
    let bytes = std::fs::read(dir.path().join(&file)).unwrap();
    assert_eq!(&bytes[..8], &PNG_MAGIC);
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let reader = decoder.read_info().unwrap();
    assert_eq!((reader.info().width, reader.info().height), (64, 64));
}

#[test]
fn a_folder_has_an_icon_too() {
    let dir = tempfile::tempdir().unwrap();
    assert!(icon_for_path(std::path::Path::new(r"C:\Windows"), dir.path()).is_some());
}

#[test]
fn a_path_that_does_not_exist_gives_none() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(icon_for_path(std::path::Path::new(r"C:\Bridge-test-nowhere\x.exe"), dir.path()), None);
}

#[test]
fn premultiplied_bgra_becomes_straight_rgba() {
    // opaque red, half-transparent premultiplied white, fully transparent
    let bgra = [0, 0, 255, 255, 128, 128, 128, 128, 0, 0, 0, 0];
    assert_eq!(bgra_to_rgba(&bgra), vec![255, 0, 0, 255, 255, 255, 255, 128, 0, 0, 0, 0]);
}

#[test]
fn a_bitmap_without_alpha_is_opaque() {
    let bgra = [10, 20, 30, 0, 40, 50, 60, 0];
    assert_eq!(bgra_to_rgba(&bgra), vec![30, 20, 10, 255, 60, 50, 40, 255]);
}
