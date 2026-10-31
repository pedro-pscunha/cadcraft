//! scratch: zz_save_as <in> <out.dxf|out.dwg> <R2000|R2004|R2007|R2010|R2013|R2018>
use cadcraft_io::{SaveOptions, Version};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let d = cadcraft_io::read(&bytes, &a[1]).unwrap();
    let version = Version::ALL.into_iter().find(|v| format!("{v:?}") == a[3]).expect("version");
    let out = cadcraft_io::write_with(&d, &a[2], &SaveOptions { version: Some(version), binary: false }).unwrap();
    std::fs::write(&a[2], out).unwrap();
}
