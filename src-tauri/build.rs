fn main() {
    println!("cargo:rerun-if-changed=../tools/sidecars");
    tauri_build::build()
}
