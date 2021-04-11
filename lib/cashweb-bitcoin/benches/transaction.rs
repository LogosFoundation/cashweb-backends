use criterion::{black_box, criterion_group, criterion_main, Criterion};

use cashweb_bitcoin::{transaction::Transaction, Decodable, Encodable};

fn decode(mut raw_tx: &[u8]) -> Transaction {
    Transaction::decode(&mut raw_tx).unwrap()
}

fn encode(tx: &Transaction) -> Vec<u8> {
    let mut buffer = Vec::with_capacity(tx.encoded_len());
    tx.encode(&mut buffer).unwrap();
    buffer
}

fn transaction_encoding_benchmark(c: &mut Criterion) {
    let tx_hex = "c47d5ad60485cb2f7a825587b95ea665a593769191382852f3514a486d7a7a11d220b62c54000000000663655253acab8c3cf32b0285b040e50dcf6987ddf7c385b3665048ad2f9317b9e0c5ba0405d8fde4129b00000000095251ab00ac65635300ffffffff549fe963ee410d6435bb2ed3042a7c294d0c7382a83edefba8582a2064af3265000000000152fffffffff7737a85e0e94c2d19cd1cde47328ece04b3e33cd60f24a8a345da7f2a96a6d0000000000865ab6a0051656aab28ff30d5049613ea020000000005ac51000063f06df1050000000008ac63516aabac5153afef5901000000000700656500655253688bc00000000000086aab5352526a53521ff1d5ff";
    let raw_tx = hex::decode(tx_hex).unwrap();
    c.bench_function("transaction decode", |b| {
        b.iter(|| decode(black_box(&raw_tx)))
    });
    let tx = decode(&raw_tx);
    c.bench_function("transaction encode", |b| b.iter(|| encode(black_box(&tx))));
}

criterion_group!(benches, transaction_encoding_benchmark);
criterion_main!(benches);
