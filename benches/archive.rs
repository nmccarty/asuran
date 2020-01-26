use criterion::*;
use libasuran::chunker::slicer::fastcdc::*;
use libasuran::chunker::slicer::*;
use libasuran::chunker::*;
use libasuran::manifest::archive::Extent;
use libasuran::manifest::*;
use libasuran::repository::backend::mem::Mem;
use libasuran::repository::*;
use rand::prelude::*;
use std::time::Duration;
use tokio::runtime::Runtime;

// Quick and dirty compressible random data generation
// Uses a counter with chance of getting reset to a random value with each byte
fn compressable_random(mut rng: impl Rng, length: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(length);
    let cut_off = 75_u8;
    let mut byte: u8 = rng.gen();
    for _ in 0..length {
        let coin: u8 = rng.gen();
        if coin < cut_off {
            byte = rng.gen();
        } else {
            byte = byte.wrapping_add(1);
        }
        output.push(byte);
    }
    output
}

async fn store<'a>(
    data: &'static [u8],
    mut repo: Repository<impl Backend>,
    slicer: impl SlicerSettings<&'static [u8]> + SlicerSettings<std::io::Empty> + 'static,
) {
    let mut manifest = Manifest::load(&repo);
    let chunker = Chunker::new(slicer);
    let mut archive = Archive::new("test");
    let extents = vec![(
        Extent {
            start: 0,
            end: data.len() as u64 - 1,
        },
        data,
    )];
    archive
        .put_sparse_object(&chunker, &mut repo, "", extents)
        .await
        .unwrap();
    manifest.commit_archive(&mut repo, archive).await;
    repo.close().await;
}

fn get_repo(key: Key) -> Repository<impl Backend> {
    let settings = ChunkSettings {
        compression: Compression::ZStd { level: 1 },
        encryption: Encryption::new_aes256ctr(),
        hmac: HMAC::Blake3,
    };
    let backend = Mem::new(settings);
    Repository::with(backend, settings, key)
}

fn bench(c: &mut Criterion) {
    let size = 64_000_000;
    let data = compressable_random(thread_rng(), size);
    let data = Box::new(data);
    let data: &'static [u8] = Box::leak(data);

    let mut rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("Blake3 archive");

    group.throughput(Throughput::Bytes(size as u64));
    group.measurement_time(Duration::new(30, 0));
    group.sample_size(20);
    group.bench_function("fastcdc AES256 ZSTD-1", |b| {
        b.iter(|| {
            rt.block_on(async {
                let repo = get_repo(Key::random(32));
                let slicer: FastCDC<&[u8]> = FastCDC::new_defaults();
                store(data, repo, slicer.copy_settings()).await;
            });
        })
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
