fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .build_client(false)
        .type_attribute(".", "#[derive(serde::Serialize,serde::Deserialize)]")
        .compile(&["proto/profile.proto"], &["proto"])?;
    Ok(())
}
