use criterion::*;
use libasuran::chunker::slicer::buzhash::*;
use libasuran::chunker::slicer::fastcdc::*;
use libasuran::chunker::slicer::*;
use libasuran::repository::*;
use rand::Rng;
use rayon::prelude::*;
use std::io::Read;
use std::mem::drop;
use std::time::Duration;
use tempfile::{tempdir, TempDir};

fn slice_and_store<'a>(
    data: &'a [u8],
    mut repo: Repository<impl Backend>,
    mut slicer: impl Slicer<&'a [u8]>,
) {
    slicer.add_reader(data);
    let mut slices = Vec::new();
    let mut slice = slicer.take_slice();
    while slice.is_some() {
        slices.push(slice.unwrap());
        slice = slicer.take_slice();
    }

    for s in slices.into_iter() {
        repo.write_chunk(s);
    }
}

fn slice_and_store_par<'a>(
    data: &'a [u8],
    mut repo: Repository<impl Backend>,
    mut slicer: impl Slicer<&'a [u8]>,
) {
    slicer.add_reader(data);
    let cs = repo.chunk_settings();
    let mut slices = Vec::new();
    let mut slice = slicer.take_slice();
    while slice.is_some() {
        slices.push(slice.unwrap());
        slice = slicer.take_slice();
    }
    let slices: Vec<UnpackedChunk> = slices
        .into_par_iter()
        .map(|x| UnpackedChunk::new(x, &cs, repo.key()))
        .collect();

    repo.write_unpacked_chunks_parallel(slices);
}

fn get_repo(key: Key) -> (Repository<FileSystem>, TempDir) {
    let root_dir = tempdir().unwrap();
    let root_path = root_dir.path().display().to_string();

    let backend = FileSystem::new_test(&root_path);
    (
        Repository::new(
            backend,
            Compression::ZStd { level: 0 },
            HMAC::Blake2bp,
            Encryption::new_aes256ctr(),
            key,
        ),
        root_dir,
    )
}

fn bench(c: &mut Criterion) {
    let mut zero = Vec::<u8>::new();
    let mut rand = Vec::<u8>::new();
    let size = 128000000;
    let mut rng = rand::thread_rng();
    for i in 0..size {
        zero.push(0);
        rand.push(rng.gen());
    }

    let (mut repo, f) = get_repo(Key::random(32));

    let mut group = c.benchmark_group("Fastcdc chunk and store");
    group.throughput(Throughput::Bytes(size as u64));
    group.measurement_time(Duration::new(60, 0));
    group.sample_size(10);
    group.bench_function("fastcdc 128M zero", |b| {
        b.iter(|| slice_and_store(&zero[..], repo.clone(), FastCDC::new_defaults()))
    });
    group.bench_function("fastcdc 128M rand", |b| {
        b.iter(|| slice_and_store(&rand[..], repo.clone(), FastCDC::new_defaults()))
    });
    group.finish();

    let mut group = c.benchmark_group("Fastcdc parallel chunk and store");
    group.throughput(Throughput::Bytes(size as u64));
    group.measurement_time(Duration::new(60, 0));
    group.sample_size(10);
    group.bench_function("fastcdc parallel 128M zero", |b| {
        b.iter(|| slice_and_store_par(&zero[..], repo.clone(), FastCDC::new_defaults()))
    });
    group.bench_function("fastcdc parallel 128M rand", |b| {
        b.iter(|| slice_and_store_par(&rand[..], repo.clone(), FastCDC::new_defaults()))
    });
    group.finish();

    let mut group = c.benchmark_group("Buzhash chunk and store");
    group.throughput(Throughput::Bytes(size as u64));
    group.measurement_time(Duration::new(60, 0));
    group.sample_size(10);
    group.bench_function("buzhash 128M zero", |b| {
        b.iter(|| slice_and_store(&zero[..], repo.clone(), BuzHash::new_defaults(0)))
    });
    group.bench_function("buzhash 128M rand", |b| {
        b.iter(|| slice_and_store(&rand[..], repo.clone(), BuzHash::new_defaults(0)))
    });
    group.finish();

    drop(repo);
}

criterion_group!(benches, bench);
criterion_main!(benches);
