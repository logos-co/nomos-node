use std::{env, path::PathBuf};

fn main() {
    let project_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let proto_includes = project_dir.join("proto");

    let dispersal_proto = project_dir.join("proto/dispersal.proto");
    let replication_proto = project_dir.join("proto/replication.proto");
    let sampling_proto = project_dir.join("proto/sampling.proto");

    prost_build::compile_protos(
        &[dispersal_proto, replication_proto, sampling_proto],
        &[proto_includes],
    )
    .unwrap();
}
