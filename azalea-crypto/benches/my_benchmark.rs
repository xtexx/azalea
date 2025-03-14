use azalea_crypto::{create_cipher, decrypt_packet, encrypt_packet};
use criterion::{Criterion, criterion_group, criterion_main};

fn bench(c: &mut Criterion) {
    let (mut enc, dec) = create_cipher(b"0123456789abcdef");

    let mut packet = [0u8; 65536];
    for (i, item) in packet.iter_mut().enumerate() {
        *item = i as u8;
    }

    c.bench_function("Encrypt 64kb", |b| {
        b.iter(|| encrypt_packet(&mut enc.clone(), &mut packet.clone()))
    });

    encrypt_packet(&mut enc, &mut packet);

    c.bench_function("Decrypt 64kb", |b| {
        b.iter(|| decrypt_packet(&mut dec.clone(), &mut packet.clone()))
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
