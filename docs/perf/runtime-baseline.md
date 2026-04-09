# Runtime Performance Baseline

## Run benchmarks

```bash
cargo bench -p beskid_runtime
```

## Current benchmark set
- `runtime/str_concat_64b`

## Baseline policy
- Keep benchmark output history in PRs touching runtime hot paths.
- Regressions >10% should be explained (algorithmic change, instrumentation, or environment variance).
- CI perf lane is report-only initially.
