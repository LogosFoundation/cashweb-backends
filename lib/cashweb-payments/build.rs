fn main() {
    prost_build::compile_protos(&["src/proto/paymentrequest.proto"], &["src/"]).unwrap();
}
