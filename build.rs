fn main() {
    prost_build::compile_protos(
        &[
            "src/proto/pop/addressmetadata.proto",
            "src/proto/pop/paymentrequest.proto",
            "src/proto/messaging/messages.proto",
        ],
        &["src/"],
    )
    .unwrap();
}
