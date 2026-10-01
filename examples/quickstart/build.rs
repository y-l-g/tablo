fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=src");
    tablo_build::tailwind().expect("the Tailwind build runs");
}
