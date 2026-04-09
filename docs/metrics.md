# Runtime Metrics (feature = `metrics`)

## Counters
- `rt_metrics_alloc_calls`
- `rt_metrics_alloc_bytes`
- `rt_metrics_str_concat_calls`
- `rt_metrics_str_concat_bytes`
- `rt_metrics_event_subscribe_calls`
- `rt_metrics_event_unsubscribe_calls`
- `rt_metrics_event_get_handler_calls`
- `rt_metrics_heap_total_bytes`
- `rt_metrics_heap_live_bytes`
- `rt_metrics_heap_fragmentation_bytes`

## Notes
- Counters use saturating arithmetic.
- Metrics are collected under active runtime root scope.
- When `metrics` is disabled, these symbols are not exported.
