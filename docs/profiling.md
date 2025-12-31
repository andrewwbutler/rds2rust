# Profiling Notes

These commands are useful for comparing mmap vs chunked extraction on large R objects.
Streaming is the default; include `--no-streaming` to force materialization if you want a baseline.

## Large RDS fixture

Sparse matrix (data.matrix):

```bash
/usr/bin/time -v target/debug/rds-extract convert /path/to/large.rds /tmp/rds_extract_data_matrix_mmap --object-kind sparse-matrix --object-path data.matrix --manifest manifest.json
/usr/bin/time -v target/debug/rds-extract convert /path/to/large.rds /tmp/rds_extract_data_matrix_chunked --object-kind sparse-matrix --object-path data.matrix --chunked --manifest manifest.json
```

Dense matrix (data["slot.value"]):

```bash
/usr/bin/time -v target/debug/rds-extract convert /path/to/large.rds /tmp/rds_extract_data_scale_mmap --object-kind dense-matrix --object-path 'data["slot.value"]' --manifest manifest.json
/usr/bin/time -v target/debug/rds-extract convert /path/to/large.rds /tmp/rds_extract_data_scale_chunked --object-kind dense-matrix --object-path 'data["slot.value"]' --chunked --manifest manifest.json
```

Streaming variants (avoid materializing lazy vectors):

```bash
/usr/bin/time -v target/debug/rds-extract convert /path/to/large.rds /tmp/rds_extract_data_matrix_stream --object-kind sparse-matrix --object-path data.matrix --streaming --chunked --chunk-size-mb 4 --manifest manifest.json
```

CLI sanity check:

```bash
target/debug/rds-extract convert /path/to/large.rds /tmp/rds_extract_data_matrix_stream --object-kind sparse-matrix --object-path data.matrix --streaming --chunked --chunk-size-mb 4 --manifest manifest.json
```

## Chunked Cache Metrics

Chunked reads track cache hits/misses, bytes read, and prefetches. For profiling, use a small
Rust harness to run a representative extraction and then call
`ChunkedRdsSource::cache_metrics()` to inspect cache behavior.

Example helper:

```bash
cargo run --bin rds-cache-profile -- /path/to/large.rds data.matrix
```

## Full read baseline

```bash
/usr/bin/time -v target/debug/rds-read /path/to/large.rds --trusted
```

## Streaming writer sanity

Use `extract_vectors_streaming` in tests or add a small harness for measuring
streaming writer overhead on mixed paths.
