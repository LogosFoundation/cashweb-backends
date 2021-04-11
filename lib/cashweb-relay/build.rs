fn main() {
    prost_build::compile_protos(&["src/proto/messaging.proto"], &["src/"]).unwrap();
}
