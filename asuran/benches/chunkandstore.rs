use asuran::chunker::*;
use asuran::repository::backend::mem::Mem;
use asuran::repository::*;
use criterion::*;
use futures::future::join_all;
use rand::prelude::*;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::task;

// Returns (zeros, random)
fn get_test_data(size: usize) -> (Vec<u8>, Vec<u8>) {
    let mut vec = vec![0_u8; size];
    rand::thread_rng().fill_bytes(&mut vec);
    (vec![0_u8; size], vec)
}

async fn slice_and_store(
    data: &'static [u8],
    repo: Repository<impl BackendClone>,
    chunker: impl AsyncChunker,
) {
    let slicer = chunker.chunk_slice(data);

    let mut futs = Vec::new();
    for slice in slicer {
        let mut repo = repo.clone();
        futs.push(task::spawn(async move {
            repo.write_chunk(slice.unwrap()).await
        }));
    }

    let _results = join_all(futs).await;
}

fn get_repo(key: Key) -> Repository<impl BackendClone> {
    let settings = ChunkSettings {
        compression: Compression::ZStd { level: 1 },
        encryption: Encryption::new_aes256ctr(),
        hmac: HMAC::Blake2bp,
    };
    let backend = Mem::new(settings, key.clone());
    Repository::with(backend, settings, key, num_cpus::get())
}

fn bench(c: &mut Criterion) {
    let size = 32_000_000;
    let (zeros, rand) = get_test_data(size);
    // Intentinally leak zeros and random to get an &'static
    let zeros: &'static [u8] = Box::leak(Box::new(zeros));
    let rand: &'static [u8] = Box::leak(Box::new(rand));

    let mut rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("Fastcdc chunk and store");
    group.throughput(Throughput::Bytes(size as u64));
    group.measurement_time(Duration::new(30, 0));
    group.sample_size(20);
    group.bench_function("fastcdc 32M zero", |b| {
        b.iter(|| {
            rt.block_on(async {
                let repo = get_repo(Key::random(32));
                slice_and_store(zeros, repo, FastCDC::default()).await
            });
        })
    });
    group.bench_function("fastcdc 32M rand", |b| {
        b.iter(|| {
            rt.block_on(async {
                let repo = get_repo(Key::random(32));
                slice_and_store(rand, repo, FastCDC::default()).await
            });
        })
    });
    group.finish();

    let mut group = c.benchmark_group("Buzhash chunk and store");
    group.throughput(Throughput::Bytes(size as u64));
    group.measurement_time(Duration::new(30, 0));
    group.sample_size(20);
    group.bench_function("buzhash 32M zero", |b| {
        b.iter(|| {
            rt.block_on(async {
                let repo = get_repo(Key::random(32));
                slice_and_store(zeros, repo, BuzHash::new(0, 4095, 14)).await
            });
        })
    });
    group.bench_function("buzhash 32M rand", |b| {
        b.iter(|| {
            rt.block_on(async {
                let repo = get_repo(Key::random(32));
                slice_and_store(rand, repo, BuzHash::new(0, 4095, 14)).await
            });
        })
    });
    group.finish();
}
criterion_group!(benches, bench);
criterion_main!(benches);
