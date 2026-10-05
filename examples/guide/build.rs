//! The guide crate is never served, so no stylesheet is built: this only
//! provides the empty file `tailwind::stylesheet!()` reads at compile time.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    let out = std::env::var("OUT_DIR").expect("OUT_DIR is set");
    std::fs::write(format!("{out}/tailwind.css"), "").expect("write the stylesheet");
}
