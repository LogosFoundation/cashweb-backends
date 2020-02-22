fn main() {
    prost_build::compile_protos(
        &[
            "src/proto/relay/messages.proto",
            "src/proto/relay/filters.proto",
        ],
        &["src/"],
    )
    .unwrap();
}
