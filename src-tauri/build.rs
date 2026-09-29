fn main() {
    println!("cargo:rerun-if-env-changed=VERSO_SIGNING_KEY");
    tauri_build::build()
}
