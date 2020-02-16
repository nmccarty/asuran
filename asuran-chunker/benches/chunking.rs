use asuran_chunker::*;
use criterion::*;
use rand::prelude::*;
use std::io::Read;
use std::time::Duration;

const SIZE: usize = 16_000_000;

// Returns (zeros, random)
fn get_test_data(size: usize) -> (Vec<u8>, Vec<u8>) {
    let mut vec = vec![0_u8; size];
    rand::thread_rng().fill_bytes(&mut vec);
    (vec![0_u8; size], vec)
}

fn chunk_boxed(read: impl Read + 'static, chunker: impl Chunker) {
    let iterator = chunker.chunk_boxed(Box::new(read));
    for chunk in iterator {
        black_box(chunk).unwrap();
    }
}

fn chunk_slice(read: &'static [u8], chunker: impl Chunker) {
    let iterator = chunker.chunk_slice(read);
    for chunk in iterator {
        black_box(chunk).unwrap();
    }
}

fn bench_fastcdc(c: &mut Criterion) {
    let (zeros, random) = get_test_data(SIZE);
    // Intentinally leak zeros and random to get an &'static
    let zeros: &'static [u8] = Box::leak(Box::new(zeros));
    let random: &'static [u8] = Box::leak(Box::new(random));
    let mut group = c.benchmark_group("fastcdc");

    group.throughput(Throughput::Bytes(SIZE as u64));
    group.measurement_time(Duration::new(30, 0));
    group.sample_size(30);

    group.bench_function("boxed zeros", |b| {
        b.iter(|| chunk_boxed(black_box(zeros), FastCDC::default()))
    });

    group.bench_function("boxed random", |b| {
        b.iter(|| chunk_boxed(black_box(random), FastCDC::default()))
    });

    group.bench_function("sliced zeros", |b| {
        b.iter(|| chunk_slice(zeros, FastCDC::default()))
    });

    group.bench_function("sliced random", |b| {
        b.iter(|| chunk_slice(random, FastCDC::default()))
    });

    group.finish();
}

fn bench_buzhash(c: &mut Criterion) {
    let (zeros, random) = get_test_data(SIZE);
    // Intentinally leak zeros and random to get an &'static
    let zeros: &'static [u8] = Box::leak(Box::new(zeros));
    let random: &'static [u8] = Box::leak(Box::new(random));
    let mut group = c.benchmark_group("buzhash");

    let chunker = BuzHash::new(0, 4095, 14);

    group.throughput(Throughput::Bytes(SIZE as u64));
    group.measurement_time(Duration::new(60, 0));
    group.sample_size(30);

    group.bench_function("boxed zeros", |b| {
        b.iter(|| chunk_boxed(black_box(zeros), chunker))
    });

    group.bench_function("boxed random", |b| {
        b.iter(|| chunk_boxed(black_box(random), chunker))
    });

    group.bench_function("sliced zeros", |b| b.iter(|| chunk_slice(zeros, chunker)));

    group.bench_function("sliced random", |b| b.iter(|| chunk_slice(random, chunker)));

    group.finish();
}

criterion_group!(benches, bench_fastcdc, bench_buzhash);
criterion_main!(benches);
