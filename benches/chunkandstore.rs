use criterion::*;
use futures::executor::{block_on, ThreadPool};
use futures::future::join_all;
use futures::task::SpawnExt;
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
    pool: impl SpawnExt,
) {
    slicer.add_reader(data);
    let slicer = slicer.into_iter();
    let size = 1;

    let mut futs = Vec::new();
    let mut count = 0;
    let mut buffer = Vec::new();
    for slice in slicer {
        count += 1;
        buffer.push(slice);
        if count >= size {
            let old_buffer = std::mem::replace(&mut buffer, Vec::new());
            let repo = repo.clone();

            futs.push(
                pool.spawn_with_handle(async move { repo.write_chunks(old_buffer).await })
                    .unwrap(),
            );
        }
    }

    let results = join_all(futs).await;
}

fn get_repo(key: Key) -> (Repository<impl Backend>, impl SpawnExt) {
    let pool = ThreadPool::new().unwrap();
    let settings = ChunkSettings {
        compression: Compression::ZStd { level: 1 },
        encryption: Encryption::new_aes256ctr(),
        hmac: HMAC::Blake2bp,
    };
    let backend = Mem::new(settings, &pool);
    (Repository::with(backend, settings, key, pool.clone()), pool)
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
            let (repo, pool) = get_repo(Key::random(32));
            block_on(slice_and_store(
                &zero[..],
                repo,
                FastCDC::new_defaults(),
                pool,
            ))
        })
    });
    group.bench_function("fastcdc 32M rand", |b| {
        b.iter(|| {
            let (repo, pool) = get_repo(Key::random(32));
            block_on(slice_and_store(
                &rand[..],
                repo,
                FastCDC::new_defaults(),
                pool,
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
            let (repo, pool) = get_repo(Key::random(32));
            block_on(slice_and_store(
                &zero[..],
                repo,
                BuzHash::new_defaults(0),
                pool,
            ))
        })
    });
    group.bench_function("buzhash 32M rand", |b| {
        b.iter(|| {
            let (repo, pool) = get_repo(Key::random(32));
            block_on(slice_and_store(
                &zero[..],
                repo,
                BuzHash::new_defaults(0),
                pool,
            ))
        })
    });
    group.finish();
}
criterion_group!(benches, bench);
criterion_main!(benches);
