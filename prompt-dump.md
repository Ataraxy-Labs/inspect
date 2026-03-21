# System Prompt

You are a precision code reviewer. Find only high-confidence, concrete correctness bugs.

Review protocol — follow this order strictly:

PHASE 1 (no tools): Read all provided code. For EACH entity with a "Contract:" block, verify the implementation satisfies the contract. Check:
- Name/contract mismatch: if a method is named getX or documented "@return X", does it ACTUALLY return X? Flag if it returns a generic/default value instead.
- Fluent/builder misuse: are return values from fluent/builder APIs captured? If discarded, the operation is a no-op.
- Dead code: are any computed results unused or overwritten?
- Guard removal: were safety checks (assertions, null guards) removed?

PHASE 2 (tools): Use read/grep ONLY to confirm or refute your Phase 1 suspicions. Do not explore broadly.

Do NOT report: style, naming, missing tests, documentation, suggestions, or issues in deleted-only code.

Respond with ONLY a JSON object:
{"issues": [{"issue": "description naming exact function/variable and the concrete bug", "evidence": "exact code snippet", "severity": "critical|high|medium", "file": "path/to/file"}]}

Return {"issues": []} if no bugs.

---

# User Prompt

# PR: Code review
Top 17 entities across 6 files (from 17 total)

## Detector Findings (8)
- **[MEDIUM] variable-near-miss** `SpansBuffer` src/sentry/spans/buffer.py:113
  Identifier changed from `span_buffer_timeout_secs` to similar `span_buffer_root_timeout_secs` — possible wrong-variable usage
  `offset = self.span_buffer_root_timeout_secs` | risk=Low(0.15) deps=16
- **[MEDIUM] variable-near-miss** `process_spans` src/sentry/spans/buffer.py:77
  Identifier changed from `span_buffer_timeout_secs` to similar `span_buffer_root_timeout_secs` — possible wrong-variable usage
  `offset = self.span_buffer_root_timeout_secs` | risk=Low(0.10) deps=3
- **[MEDIUM] variable-near-miss** `_load_segment_data` src/sentry/spans/buffer.py:18
  Identifier changed from `sscan` to similar `zscan` — possible wrong-variable usage
  `p.zscan(key, cursor=cursor, count=self.segment_page_size)` | risk=Critical(1.47) deps=2
- **[MEDIUM] type-change-propagation** `SpansBuffer` src/sentry/spans/buffer.py:142
  Type `SpansBuffer` was modified but 8 dependent(s) were not updated in this diff: __reduce__, ProcessSpansStrategyFactory, create_with_partitions, SpanFlusher, __init__ and 3 more
  `Unchanged dependents: __reduce__, ProcessSpansStrategyFactory, create_with_partitions, SpanFlusher, __init__ and 3 more` | risk=Low(0.15) deps=16
- **[MEDIUM] type-change-propagation** `Span` src/sentry/spans/buffer.py:113
  Type `Span` was modified but 1 dependent(s) were not updated in this diff: _group_by_parent
  `Unchanged dependents: _group_by_parent` | risk=Low(0.10) deps=3
- **[HIGH] removed-guard** `lines 41-60` src/sentry/scripts/spans/add-buffer.lua:1
  Guard/assertion removed: `if not is_root_span and redis.call("scard", span_key) > 0 then` — safety check may be lost
  `if not is_root_span and redis.call("scard", span_key) > 0 then` | risk=Medium(0.38) deps=0
- **[HIGH] removed-guard** `SpansBuffer` src/sentry/spans/buffer.py:1
  Guard/assertion removed: `if len(payloads[key]) > self.max_segment_spans:` — safety check may be lost
  `if len(payloads[key]) > self.max_segment_spans:` | risk=Low(0.15) deps=16
- **[HIGH] removed-guard** `_load_segment_data` src/sentry/spans/buffer.py:1
  Guard/assertion removed: `if len(payloads[key]) > self.max_segment_spans:` — safety check may be lost
  `if len(payloads[key]) > self.max_segment_spans:` | risk=Critical(1.47) deps=2

## src/sentry/spans/buffer.py (Critical, 4 entities)
### `_load_segment_data` (function, modified) :417-463
risk=Critical(1.47) blast=22 deps=2 | callers: SpansBuffer, flush_segments
```
def _load_segment_data(self, segment_keys: list[SegmentKey]) -> dict[SegmentKey, list[bytes]]:
        """
        Loads the segments from Redis, given a list of segment keys. Segments
        exceeding a certain size are skipped, and an error is logged.

        :param segment_keys: List of segment keys to load.
        :return: Dictionary mapping segment keys to lists of span payloads.
        """

        payloads: dict[SegmentKey, list[bytes]] = {key: [] for key in segment_keys}
        cursors = {key: 0 for key in segment_keys}
        sizes = {key: 0 for key in segment_keys}

        whi
... (1424 more chars)
```

### `SpansBuffer` (class, modified) :142-484
risk=Low(0.15) blast=19 deps=16 | callers: __reduce__, ProcessSpansStrategyFactory, create_with_partitions, process_batch +12
```
class SpansBuffer:
    def __init__(
        self,
        assigned_shards: list[int],
        span_buffer_timeout_secs: int = 60,
        span_buffer_root_timeout_secs: int = 10,
        segment_page_size: int = 100,
        max_segment_bytes: int = 10 * 1024 * 1024,  # 10 MiB
        max_segment_spans: int = 1001,
        redis_ttl: int = 3600,
    ):
        self.assigned_shards = list(assigned_shards)
        self.span_buffer_timeout_secs = span_buffer_timeout_secs
        self.span_buffer_root_timeout_secs = span_buffer_root_timeout_secs
        self.segment_page_size = segment_page_size

... (13860 more chars)
```

### `process_spans` (function, modified) :178-279
risk=Low(0.10) blast=20 deps=3 | callers: SpansBuffer, process_batch, test_backpressure
```
def process_spans(self, spans: Sequence[Span], now: int):
        """
        :param spans: List of to-be-ingested spans.
        :param now: The current time to be used for setting expiration/flush
            deadlines. Used for unit-testing and managing backlogging behavior.
        """

        queue_keys = []
        is_root_span_count = 0
        has_root_span_count = 0
        min_redirect_depth = float("inf")
        max_redirect_depth = float("-inf")

        with metrics.timer("spans.buffer.process_spans.push_payloads"):
            trees = self._group_by_parent(spans)

            w
... (3993 more chars)
```

### `Span` (class, modified) :113-130
risk=Low(0.10) blast=22 deps=3 | callers: SpansBuffer, process_spans, _group_by_parent
```
class Span(NamedTuple):
    trace_id: str
    span_id: str
    parent_span_id: str | None
    project_id: int
    payload: bytes
    end_timestamp_precise: float
    is_segment_span: bool = False

    def effective_parent_id(self):
        # Note: For the case where the span's parent is in another project, we
        # will still flush the segment-without-root-span as one unit, just
        # after span_buffer_timeout_secs rather than
        # span_buffer_root_timeout_secs.
        if self.is_segment_span:
            return self.span_id
        else:
            return self.parent_span_id or self.span_id
```

## src/sentry/spans/consumers/process/factory.py (High, 1 entities)
### `process_batch` (function, modified) :124-148
risk=High(0.59) blast=3 deps=2 | callers: ProcessSpansStrategyFactory, create_with_partitions
```
def process_batch(
    buffer: SpansBuffer, values: Message[ValuesBatch[tuple[int, KafkaPayload]]]
) -> int:
    min_timestamp = None
    spans = []
    for value in values.payload:
        timestamp, payload = value.payload
        if min_timestamp is None or timestamp < min_timestamp:
            min_timestamp = timestamp

        val = cast(SpanEvent, rapidjson.loads(payload.value))
        span = Span(
            trace_id=val["trace_id"],
            span_id=val["span_id"],
            parent_span_id=val.get("parent_span_id"),
            project_id=val["project_id"],
            payload=
... (322 more chars)
```

## tests/sentry/spans/consumers/process/test_consumer.py (Medium, 1 entities)
### `test_basic` (function, modified) :10-76
risk=Medium(0.47) blast=0 deps=0
```
def test_basic(monkeypatch):
    # Flush very aggressively to make test pass instantly
    monkeypatch.setattr("time.sleep", lambda _: None)

    topic = Topic("test")
    messages: list[KafkaPayload] = []

    fac = ProcessSpansStrategyFactory(
        max_batch_size=10,
        max_batch_time=10,
        num_processes=1,
        max_flush_segments=10,
        input_block_size=None,
        output_block_size=None,
        produce_to_pipe=messages.append,
    )

    commits = []

    def add_commit(offsets, force=False):
        commits.append(offsets)

    step = fac.create_with_partitions(ad
... (1250 more chars)
```

## tests/sentry/spans/consumers/process/test_flusher.py (Medium, 1 entities)
### `test_backpressure` (function, modified) :15-87
risk=Medium(0.47) blast=0 deps=0
```
def test_backpressure(monkeypatch):
    # Flush very aggressively to make join() faster
    monkeypatch.setattr("time.sleep", lambda _: None)

    buffer = SpansBuffer(assigned_shards=list(range(1)))

    messages = []

    def append(msg):
        messages.append(msg)
        sleep(1.0)

    flusher = SpanFlusher(
        buffer,
        max_flush_segments=1,
        max_memory_percentage=1.0,
        produce_to_pipe=append,
        next_step=Noop(),
    )

    now = time.time()

    for i in range(200):
        trace_id = f"{i:0>32x}"

        spans = [
            Span(
                payl
... (1399 more chars)
```

## tests/sentry/spans/test_buffer.py (Medium, 6 entities)
### `test_basic` (function, modified) :115-181
risk=Medium(0.47) blast=0 deps=0
```
@pytest.mark.parametrize(
    "spans",
    list(
        itertools.permutations(
            [
                Span(
                    payload=_payload(b"a" * 16),
                    trace_id="a" * 32,
                    span_id="a" * 16,
                    parent_span_id="b" * 16,
                    project_id=1,
                    end_timestamp_precise=1700000000.0,
                ),
                Span(
                    payload=_payload(b"d" * 16),
                    trace_id="a" * 32,
                    span_id="d" * 16,
                    parent_span_id="b" * 16,
          
... (1568 more chars)
```

### `test_deep` (function, modified) :184-251
risk=Low(0.07) blast=0 deps=0
```
@pytest.mark.parametrize(
    "spans",
    list(
        itertools.permutations(
            [
                Span(
                    payload=_payload(b"d" * 16),
                    trace_id="a" * 32,
                    span_id="d" * 16,
                    parent_span_id="b" * 16,
                    project_id=1,
                    end_timestamp_precise=1700000000.0,
                ),
                _SplitBatch(),
                Span(
                    payload=_payload(b"b" * 16),
                    trace_id="a" * 32,
                    span_id="b" * 16,
                    pare
... (1523 more chars)
```

### `test_deep2` (function, modified) :254-329
risk=Low(0.07) blast=0 deps=0
```
@pytest.mark.parametrize(
    "spans",
    list(
        itertools.permutations(
            [
                Span(
                    payload=_payload(b"e" * 16),
                    trace_id="a" * 32,
                    span_id="e" * 16,
                    parent_span_id="d" * 16,
                    project_id=1,
                    end_timestamp_precise=1700000000.0,
                ),
                Span(
                    payload=_payload(b"d" * 16),
                    trace_id="a" * 32,
                    span_id="d" * 16,
                    parent_span_id="b" * 16,
          
... (1857 more chars)
```

### `test_flush_rebalance` (function, modified) :491-522
risk=Low(0.07) blast=0 deps=0
```
def test_flush_rebalance(buffer: SpansBuffer):
    spans = [
        Span(
            payload=_payload(b"a" * 16),
            trace_id="a" * 32,
            span_id="a" * 16,
            parent_span_id=None,
            project_id=1,
            is_segment_span=True,
            end_timestamp_precise=1700000000.0,
        )
    ]

    process_spans(spans, buffer, now=0)
    assert_ttls(buffer.client)

    assert buffer.flush_segments(now=5) == {}
    rv = buffer.flush_segments(now=11)
    assert rv == {
        _segment_id(1, "a" * 32, "a" * 16): FlushedSegment(
            queue_key=mock.AN
... (300 more chars)
```

### `test_parent_in_other_project` (function, modified) :332-406
risk=Low(0.07) blast=0 deps=0
```
@pytest.mark.parametrize(
    "spans",
    list(
        itertools.permutations(
            [
                Span(
                    payload=_payload(b"c" * 16),
                    trace_id="a" * 32,
                    span_id="c" * 16,
                    parent_span_id="b" * 16,
                    project_id=1,
                    end_timestamp_precise=1700000000.0,
                ),
                Span(
                    payload=_payload(b"d" * 16),
                    trace_id="a" * 32,
                    span_id="d" * 16,
                    parent_span_id="b" * 16,
          
... (1855 more chars)
```

### `test_parent_in_other_project_and_nested_is_segment_span` (function, modified) :409-488
risk=Low(0.07) blast=0 deps=0
```
@pytest.mark.parametrize(
    "spans",
    shallow_permutations(
        [
            Span(
                payload=_payload(b"c" * 16),
                trace_id="a" * 32,
                span_id="c" * 16,
                parent_span_id="d" * 16,
                project_id=1,
                is_segment_span=True,
                end_timestamp_precise=1700000000.0,
            ),
            Span(
                payload=_payload(b"d" * 16),
                trace_id="a" * 32,
                span_id="d" * 16,
                parent_span_id="b" * 16,
                project_id=1,
              
... (1894 more chars)
```

## src/sentry/scripts/spans/add-buffer.lua (Medium, 4 entities)
### `lines 41-60` (chunk, modified) :0-0
risk=Medium(0.38) blast=0 deps=0
```
redis.call("expire", main_redirect_key, set_timeout)

local span_count = 0

local set_key = string.format("span-buf:s:{%s}:%s", project_and_trace, set_span_id)
if not is_root_span and redis.call("zcard", span_key) > 0 then
    span_count = redis.call("zunionstore", set_key, 2, set_key, span_key)
    redis.call("unlink", span_key)
end

local parent_key = string.format("span-buf:s:{%s}:%s", project_and_trace, parent_span_id)
if set_span_id ~= parent_span_id and redis.call("zcard", parent_key) > 0 then
    span_count = redis.call("zunionstore", set_key, 2, set_key, parent_key)
    redis.call("unlink", parent_key)
end
redis.call("expire", set_key, set_timeout)

if span_count == 0 then
    span_count = redis.call("zcard", set_key)
end
```

### `lines 61-62` (chunk, deleted) :0-0
risk=Low(0.05) blast=0 deps=0
```

return {redirect_depth, span_key, set_key, has_root_span}
```

### `lines 61-72` (chunk, added) :0-0
risk=Low(0.05) blast=0 deps=0
```

if span_count > 1000 then
    redis.call("zpopmin", set_key, span_count - 1000)
end

local has_root_span_key = string.format("span-buf:hrs:%s", set_key)
local has_root_span = redis.call("get", has_root_span_key) == "1" or is_root_span
if has_root_span then
    redis.call("setex", has_root_span_key, set_timeout, "1")
end

return {redirect_depth, span_key, set_key, has_root_span}
```

### `lines 21-40` (chunk, modified) :0-0
risk=Low(0.04) blast=0 deps=0
```
local parent_span_id = ARGV[3]
local set_timeout = tonumber(ARGV[4])

local span_key = string.format("span-buf:s:{%s}:%s", project_and_trace, span_id)
local main_redirect_key = string.format("span-buf:sr:{%s}", project_and_trace)

local set_span_id = parent_span_id
local redirect_depth = 0

for i = 0, 1000 do
    local new_set_span = redis.call("hget", main_redirect_key, set_span_id)
    redirect_depth = i
    if not new_set_span or new_set_span == set_span_id then
        break
    end

    set_span_id = new_set_span
end

redis.call("hset", main_redirect_key, span_id, set_span_id)
```
