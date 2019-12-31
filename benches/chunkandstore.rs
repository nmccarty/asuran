use criterion::*;
use futures::executor::{block_on, ThreadPool};
use futures::future::join_all;
use libasuran::chunker::slicer::buzhash::*;
use libasuran::chunker::slicer::fastcdc::*;
use libasuran::chunker::slicer::*;
use libasuran::repository::backend::mem::Mem;
use libasuran::repository::*;
use rand::Rng;
use std::time::Duration;

async fn slice_and_store<'a>(
    data: &'a [u8],
    repo: Repository<impl Backend>,
    mut slicer: impl Slicer<&'a [u8]>,
) {
    slicer.add_reader(data);
    let mut s = Vec::new();
    for slice in slicer {
        s.push(repo.write_chunk(slice));
    }

    join_all(s).await;
}

fn get_repo(key: Key) -> Repository<impl Backend> {
    let pool = ThreadPool::new().unwrap();
    let settings = ChunkSettings {
        compression: Compression::ZStd { level: 1 },
        encryption: Encryption::new_aes256ctr(),
        hmac: HMAC::Blake2bp,
    };
    let backend = libasuran::repository::backend::mem::Mem::new(settings, &pool);
    Repository::with(backend, settings, key, pool)
}

fn bench(c: &mut Criterion) {
    let mut zero = Vec::<u8>::new();
    let mut rand = Vec::<u8>::new();
    let size = 32000000;
    let mut rng = rand::thread_rng();
    for i in 0..size {
        zero.push(0);
        rand.push(rng.gen());
    }

    let mut group = c.benchmark_group("Fastcdc chunk and store");
    group.throughput(Throughput::Bytes(size as u64));
    group.measurement_time(Duration::new(30, 0));
    group.sample_size(20);
    group.bench_function("fastcdc 32M zero", |b| {
        b.iter(|| {
            block_on(slice_and_store(
                &zero[..],
                get_repo(Key::random(32)),
                FastCDC::new_defaults(),
            ))
        })
    });
    group.bench_function("fastcdc 32M rand", |b| {
        b.iter(|| {
            block_on(slice_and_store(
                &rand[..],
                get_repo(Key::random(32)),
                FastCDC::new_defaults(),
            ))
        })
    });
    group.finish();

    let mut group = c.benchmark_group("Buzhash chunk and store");
    group.throughput(Throughput::Bytes(size as u64));
    group.measurement_time(Duration::new(30, 0));
    group.sample_size(20);
    group.bench_function("buzhash 32M zero", |b| {
        b.iter(|| {
            block_on(slice_and_store(
                &zero[..],
                get_repo(Key::random(32)),
                BuzHash::new_defaults(0),
            ))
        })
    });
    group.bench_function("buzhash 32M rand", |b| {
        b.iter(|| {
            block_on(slice_and_store(
                &rand[..],
                get_repo(Key::random(32)),
                BuzHash::new_defaults(0),
            ))
        })
    });
    group.finish();
}
criterion_group!(benches, bench);
criterion_main!(benches);
