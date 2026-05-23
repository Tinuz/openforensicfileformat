fn main() {
    println!("cargo:rerun-if-changed=proto/offf_access.proto");

    let protoc = protoc_bin_vendored::protoc_bin_path().expect("failed to find bundled protoc");
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_server(true)
        .compile_protos(&["proto/offf_access.proto"], &["proto"])
        .expect("failed to compile gRPC proto");
}
