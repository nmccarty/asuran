use criterion::*;
use futures::future::join_all;
use libasuran::chunker::slicer::buzhash::*;
use libasuran::chunker::slicer::fastcdc::*;
use libasuran::chunker::slicer::*;
use libasuran::repository::backend::mem::Mem;
use libasuran::repository::*;
use rand::Rng;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::task;

async fn slice_and_store<'a>(
    data: &'a [u8],
    repo: Repository<impl Backend>,
    mut slicer: impl Slicer<&'a [u8]>,
) {
    slicer.add_reader(data);
    let slicer = slicer.into_iter();

    let mut futs = Vec::new();
    for slice in slicer {
        let mut repo = repo.clone();
        futs.push(task::spawn(async move { repo.write_chunk(slice).await }));
    }

    let _results = join_all(futs).await;
}

fn get_repo(key: Key) -> Repository<impl Backend> {
    let settings = ChunkSettings {
        compression: Compression::ZStd { level: 1 },
        encryption: Encryption::new_aes256ctr(),
        hmac: HMAC::Blake2bp,
    };
    let backend = Mem::new(settings);
    Repository::with(backend, settings, key)
}

fn bench(c: &mut Criterion) {
    let mut zero = Vec::<u8>::new();
    let mut rand = Vec::<u8>::new();
    let size = 32000000;
    let mut rng = rand::thread_rng();
    for _ in 0..size {
        zero.push(0);
        rand.push(rng.gen());
    }

    let mut rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("Fastcdc chunk and store");
    group.throughput(Throughput::Bytes(size as u64));
    group.measurement_time(Duration::new(30, 0));
    group.sample_size(20);
    group.bench_function("fastcdc 32M zero", |b| {
        b.iter(|| {
            rt.block_on(async {
                let repo = get_repo(Key::random(32));
                slice_and_store(&zero[..], repo, FastCDC::new_defaults()).await
            });
        })
    });
    group.bench_function("fastcdc 32M rand", |b| {
        b.iter(|| {
            rt.block_on(async {
                let repo = get_repo(Key::random(32));
                slice_and_store(&rand[..], repo, FastCDC::new_defaults()).await
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
                slice_and_store(&zero[..], repo, BuzHash::new_defaults(0)).await
            });
        })
    });
    group.bench_function("buzhash 32M rand", |b| {
        b.iter(|| {
            rt.block_on(async {
                let repo = get_repo(Key::random(32));
                slice_and_store(&zero[..], repo, BuzHash::new_defaults(0)).await
            });
        })
    });
    group.finish();
}
criterion_group!(benches, bench);
criterion_main!(benches);
