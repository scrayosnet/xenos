fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("proto/profile.proto")?;
    println!("Build proto successfully");
    Ok(())
}
