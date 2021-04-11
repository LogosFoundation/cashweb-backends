fn main() {
    prost_build::compile_protos(&["src/proto/wrapper.proto"], &["src/"]).unwrap();
}
