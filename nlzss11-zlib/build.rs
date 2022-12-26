fn main() {
    let mut cmake = cmake::Config::new("zlib-ng");
    cmake.define("BUILD_SHARED_LIBS", "OFF")
    .define("ZLIB_COMPAT", "OFF")
    .define("ZLIB_ENABLE_TESTS", "OFF")
    .define("WITH_GZFILEOP", "ON");
    let install_dir = cmake.build();
    let libdir = install_dir.join("lib");
    println!(
        "cargo:rustc-link-search={}",
        libdir.to_str().unwrap()
    );
    let target = std::env::var("TARGET").unwrap();
    let libname = if target.contains("windows") && target.contains("msvc") {
        "zlibstatic"
    } else {
        "zlib"
    };
    println!(
        "cargo:rustc-link-lib={}",
        libname
    );
}
