//! Benchmarks for native plugin operations.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use openracing_native_plugin::{
    CURRENT_ABI_VERSION, NativePluginConfig, SpscChannel, check_abi_compatibility,
};

const BENCH_CHANNEL_CAPACITY: u32 = 256;

fn bench_abi_check(c: &mut Criterion) {
    c.bench_function("abi_check_compatible", |b| {
        b.iter(|| check_abi_compatibility(black_box(CURRENT_ABI_VERSION)))
    });

    c.bench_function("abi_check_incompatible", |b| {
        b.iter(|| check_abi_compatibility(black_box(999)))
    });
}

fn bench_config_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("config_creation");

    group.bench_function("default", |b| b.iter(NativePluginConfig::default));

    group.bench_function("strict", |b| b.iter(NativePluginConfig::strict));

    group.bench_function("permissive", |b| b.iter(NativePluginConfig::permissive));

    group.bench_function("development", |b| b.iter(NativePluginConfig::development));

    group.finish();
}

fn bench_spsc_channel(c: &mut Criterion) {
    let frame_size = 64;
    let channel = SpscChannel::new(frame_size).expect("Failed to create channel");
    let frame = vec![0x42u8; frame_size];

    c.bench_function("spsc_write", |b| {
        let writer = channel.writer();
        let frame = frame.clone();
        b.iter(|| {
            let _ = writer.try_write(black_box(&frame));
        })
    });

    c.bench_function("spsc_read", |b| {
        let reader = channel.reader();
        let mut buffer = vec![0u8; frame_size];
        b.iter(|| {
            let _ = reader.try_read(black_box(&mut buffer));
        })
    });
}

fn bench_spsc_different_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("spsc_sizes");

    for size in [16, 64, 256, 1024, 4096].iter() {
        let frame_size = *size;
        let channel = SpscChannel::with_capacity(frame_size, BENCH_CHANNEL_CAPACITY)
            .expect("Failed to create channel");
        let frame = vec![0x42u8; frame_size];

        group.bench_with_input(BenchmarkId::new("write", size), &frame_size, |b, _| {
            let writer = channel.writer();
            let frame = frame.clone();
            b.iter(|| {
                let _ = writer.try_write(black_box(&frame));
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_abi_check,
    bench_config_creation,
    bench_spsc_channel,
    bench_spsc_different_sizes,
);

criterion_main!(benches);
