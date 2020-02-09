use criterion::*;
use libasuran::chunker::slicer::buzhash::*;
use libasuran::chunker::slicer::fastcdc::*;
use libasuran::chunker::slicer::*;
use rand::Rng;
use std::io::Read;
use std::time::Duration;

fn fast_cdc_chunk(r: impl Read + Send + Clone) {
    let mut slicer = FastCDC::new_defaults();
    slicer.add_reader(r);
    for s in slicer {
        black_box(s);
    }
}

fn buzhash_chunk(r: impl Read + Send + Clone) {
    let mut slicer = BuzHash::new_defaults(0);
    slicer.add_reader(r);
    for s in slicer {
        black_box(s);
    }
}

pub fn bench(c: &mut Criterion) {
    let mut zero = Vec::<u8>::new();
    let mut rand = Vec::<u8>::new();
    let size = 320000;
    let mut rng = rand::thread_rng();
    for _ in 0..size {
        zero.push(0);
        rand.push(rng.gen());
    }

    let mut group = c.benchmark_group("fastcdc");
    group.throughput(Throughput::Bytes(size as u64));
    group.measurement_time(Duration::new(30, 0));
    group.sample_size(20);
    group.bench_function("fastcdc 32M zero", |b| b.iter(|| fast_cdc_chunk(&zero[..])));
    group.bench_function("fastcdc 32M rand", |b| b.iter(|| fast_cdc_chunk(&rand[..])));
    group.finish();

    let mut group = c.benchmark_group("buzhash");
    group.throughput(Throughput::Bytes(size as u64));
    group.measurement_time(Duration::new(30, 0));
    group.sample_size(20);
    group.bench_function("buzhash 32M zero", |b| b.iter(|| buzhash_chunk(&zero[..])));
    group.bench_function("buzhash 32M rand", |b| b.iter(|| buzhash_chunk(&rand[..])));
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
