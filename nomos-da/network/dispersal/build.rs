use std::{env, path::PathBuf};

fn main() {
    let project_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let dispersal_includes = project_dir.join("proto");
    let dispersal_proto = project_dir.join("proto/dispersal.proto");

    prost_build::compile_protos(&[dispersal_proto], &[dispersal_includes]).unwrap();
}
