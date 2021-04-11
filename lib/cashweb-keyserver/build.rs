fn main() {
    prost_build::compile_protos(&["src/proto/keyserver.proto"], &["src/"]).unwrap();
}
