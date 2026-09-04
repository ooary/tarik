fn main() {
    // Linux portable releases place libduckdb.so beside the engine. Windows
    // resolves duckdb.dll from the executable directory without an rpath.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
