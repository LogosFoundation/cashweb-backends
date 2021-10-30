use ring::digest::{Context, SHA256};

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut sha256_context = Context::new(&SHA256);
    sha256_context.update(data);
    sha256_context.finish().as_ref().try_into().unwrap()
}
