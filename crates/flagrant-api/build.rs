fn main() -> std::io::Result<()> {
    // Always use the vendored protoc so builds don't depend on whatever (if anything) is
    // installed system-wide.
    let protoc = protoc_bin_vendored::protoc_bin_path()
        .expect("Could not locate vendored protoc binary");
    // SAFETY: build scripts are single-threaded at this point; no concurrent env access.
    unsafe { std::env::set_var("PROTOC", protoc) };

    tonic_prost_build::configure()
        .build_client(false)
        .compile_protos(&["proto/features.proto"], &["proto"])?;

    println!("cargo:rerun-if-changed=proto/features.proto");
    Ok(())
}
