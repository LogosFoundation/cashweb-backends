fn main() {
    prost_build::compile_protos(&["src/proto/database.proto"], &["src/"]).unwrap();
    prost_build::compile_protos(&["src/pubsub/proto/broadcast.proto"], &["src/"]).unwrap();
}
