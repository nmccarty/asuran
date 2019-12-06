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
        .map(|x| UnpackedChunk::new(x, cs, repo.key().clone()))
        .collect();

    repo.write_unpacked_chunks_parallel(slices);
}

fn get_repo(key: Key) -> Repository<impl Backend> {
    let settings = ChunkSettings {
        compression: Compression::ZStd { level: 1 },
        encryption: Encryption::new_aes256ctr(),
        hmac: HMAC::Blake2bp,
    };
    let backend = libasuran::repository::backend::mem::Mem::new(settings);
    Repository::new(
        backend,
        settings.compression,
        settings.hmac,
        settings.encryption,
        key,
    )
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
            slice_and_store(
                &zero[..],
                get_repo(Key::random(32)),
                FastCDC::new_defaults(),
            )
        })
    });
    group.bench_function("fastcdc 32M rand", |b| {
        b.iter(|| {
            slice_and_store(
                &rand[..],
                get_repo(Key::random(32)),
                FastCDC::new_defaults(),
            )
        })
    });
    group.finish();

    let mut group = c.benchmark_group("Fastcdc parallel chunk and store");
    group.throughput(Throughput::Bytes(size as u64));
    group.measurement_time(Duration::new(30, 0));
    group.sample_size(20);
    group.bench_function("fastcdc parallel 32M zero", |b| {
        b.iter(|| {
            slice_and_store_par(
                &zero[..],
                get_repo(Key::random(32)),
                FastCDC::new_defaults(),
            )
        })
    });
    group.bench_function("fastcdc parallel 32M rand", |b| {
        b.iter(|| {
            slice_and_store_par(
                &rand[..],
                get_repo(Key::random(32)),
                FastCDC::new_defaults(),
            )
        })
    });
    group.finish();

    let mut group = c.benchmark_group("Buzhash chunk and store");
    group.throughput(Throughput::Bytes(size as u64));
    group.measurement_time(Duration::new(30, 0));
    group.sample_size(20);
    group.bench_function("buzhash 32M zero", |b| {
        b.iter(|| {
            slice_and_store(
                &zero[..],
                get_repo(Key::random(32)),
                BuzHash::new_defaults(0),
            )
        })
    });
    group.bench_function("buzhash 32M rand", |b| {
        b.iter(|| {
            slice_and_store(
                &rand[..],
                get_repo(Key::random(32)),
                BuzHash::new_defaults(0),
            )
        })
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
