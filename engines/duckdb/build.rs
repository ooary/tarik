fn main() {
    // Make the engine binary find its DuckDB runtime library next to itself,
    // which is how the portable release ships it (tarik-engine-duckdb +
    // libduckdb.so / duckdb.dll in the same directory).
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    println!("cargo:rerun-if-changed=build.rs");
}
