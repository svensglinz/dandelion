fn main() {
    println!("cargo:rustc-link-lib=dylib=lauberhorn");
    println!("cargo:rustc-link-lib=dylib=tirpc"); // temporarily depends on this. may have to build runtime library withhout dependencies or this statically baked in...
}
