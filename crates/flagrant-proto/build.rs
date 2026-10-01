fn main() -> std::io::Result<()> {
    // Relies on `protoc` being installed on the host machine and available on PATH
    // (or pointed to via the PROTOC env var).
    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_protos(&["proto/features.proto"], &["proto"])?;

    println!("cargo:rerun-if-changed=proto/features.proto");

    Ok(())
}
