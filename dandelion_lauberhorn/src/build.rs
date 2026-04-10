// on first run, create manually with bindgen (include in a setup script or so)

fn main() {
    let bindings = bindgen::Builder::default()
    .header("../liblauberhorn/include/lauberhorn.h")
    .generate()
    .expect("Failed to generate bindings")
    .write_to_file(
        std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap())
            .join("bindings.rs")
    )
    .expect("Failed to write bindings");
}