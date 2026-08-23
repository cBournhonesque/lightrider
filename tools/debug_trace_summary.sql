CREATE OR REPLACE VIEW debug_events AS
SELECT *
FROM read_ndjson_auto(getvariable('trace_glob'), union_by_name = true, ignore_errors = true);

CREATE OR REPLACE VIEW snake_heads AS
SELECT
    process_id,
    role,
    schedule,
    sample_point,
    tick_id,
    entity,
    fields.room_id::UBIGINT AS room_id,
    fields.player_id_bits::UBIGINT AS player_id_bits,
    fields.head_x::DOUBLE AS head_x,
    fields.head_y::DOUBLE AS head_y,
    fields.direction::VARCHAR AS direction,
    fields.speed::DOUBLE AS speed,
    fields.tail_total_length::DOUBLE AS tail_total_length,
    fields.tail_length_current::DOUBLE AS tail_length_current,
    fields.is_predicted::BOOLEAN AS is_predicted,
    fields.is_interpolated::BOOLEAN AS is_interpolated,
    coalesce(fields.has_simulation_authority::BOOLEAN, false) AS has_simulation_authority,
    fields.is_controlled::BOOLEAN AS is_controlled,
    fields.is_bot::BOOLEAN AS is_bot
FROM debug_events
WHERE kind = 'snake_head';

SELECT 'events_by_kind' AS section, target, kind, count(*) AS rows
FROM debug_events
GROUP BY target, kind
ORDER BY rows DESC, target, kind;

SELECT
    'perf_frame_by_process' AS section,
    role,
    process_id,
    count(*) AS samples,
    avg(CAST(fields.frame_delta_avg_ms AS DOUBLE)) AS avg_frame_ms,
    max(CAST(fields.frame_delta_max_ms AS DOUBLE)) AS max_frame_ms,
    avg(CAST(fields.fps AS DOUBLE)) AS avg_fps,
    avg(CAST(fields.link_count AS DOUBLE)) AS avg_links,
    avg(CAST(fields.link_rtt_avg_ms AS DOUBLE)) AS avg_rtt_ms,
    avg(CAST(fields.link_jitter_avg_ms AS DOUBLE)) AS avg_jitter_ms,
    max(CAST(fields.link_recv_buffered AS BIGINT)) AS max_recv_buffered,
    max(CAST(fields.link_send_buffered AS BIGINT)) AS max_send_buffered
FROM debug_events
WHERE kind = 'perf_frame'
GROUP BY role, process_id
ORDER BY role, process_id;

SELECT
    'transport_by_process' AS section,
    process_id,
    kind,
    count(*) AS rows,
    sum(coalesce(CAST(fields.bytes AS BIGINT), CAST(fields.send_bytes AS BIGINT), 0)) AS bytes
FROM debug_events
WHERE target = 'lightyear_debug::transport'
GROUP BY process_id, kind
ORDER BY process_id, kind;

WITH transport_fragments AS (
    SELECT
        process_id,
        kind,
        coalesce(
            CAST(frame_index AS BIGINT),
            CAST(fields.local_tick AS BIGINT),
            CAST(fields.remote_tick AS BIGINT),
            CAST(tick_id AS BIGINT),
            -1
        ) AS frame_ref
    FROM debug_events
    WHERE target = 'lightyear_debug::transport'
        AND (
            lower(kind) LIKE '%fragment%'
            OR lower(CAST(fields.packet_type AS VARCHAR)) LIKE '%fragment%'
        )
)
SELECT
    'transport_fragments_by_process' AS section,
    process_id,
    kind,
    sum(fragments_per_frame) AS fragments,
    max(fragments_per_frame) AS max_fragments_per_frame
FROM (
    SELECT
        process_id,
        kind,
        frame_ref,
        count(*) AS fragments_per_frame
    FROM transport_fragments
    GROUP BY process_id, kind, frame_ref
) AS per_tick
GROUP BY process_id, kind
ORDER BY process_id, kind;

SELECT
    'rollback_requests_by_process' AS section,
    process_id,
    count(*) AS rollbacks,
    min(CAST(fields.local_tick AS BIGINT)) AS min_local_tick,
    max(CAST(fields.local_tick AS BIGINT)) AS max_local_tick,
    avg(CAST(fields.rollback_delta AS DOUBLE)) AS avg_rollback_delta,
    max(CAST(fields.rollback_delta AS BIGINT)) AS max_rollback_delta
FROM debug_events
WHERE target = 'lightyear_debug::prediction'
    AND kind = 'rollback_requested'
GROUP BY process_id
ORDER BY rollbacks DESC;

SELECT
    'rollback_delta_histogram' AS section,
    CAST(fields.rollback_delta AS BIGINT) AS rollback_delta,
    count(*) AS rollbacks
FROM debug_events
WHERE target = 'lightyear_debug::prediction'
    AND kind = 'rollback_requested'
GROUP BY rollback_delta
ORDER BY rollback_delta;

SELECT
    'rollback_mismatch_components' AS section,
    replace(CAST(fields.component AS VARCHAR), '\"', '') AS component,
    count(*) AS mismatches
FROM debug_events
WHERE target = 'lightyear_debug::prediction'
    AND kind = 'rollback_value_mismatch'
GROUP BY component
ORDER BY mismatches DESC;

WITH rollbacks AS (
    SELECT
        process_id,
        CAST(fields.local_tick AS BIGINT) AS local_tick,
        CAST(fields.rollback_delta AS BIGINT) AS rollback_delta
    FROM debug_events
    WHERE target = 'lightyear_debug::prediction'
        AND kind = 'rollback_requested'
),
controlled_heads AS (
    SELECT
        process_id,
        tick_id,
        max(speed) AS speed
    FROM snake_heads
    WHERE schedule = 'FixedLast'
        AND is_predicted
        AND is_controlled
    GROUP BY process_id, tick_id
)
SELECT
    'rollback_controlled_speed_buckets' AS section,
    rollbacks.process_id,
    floor(coalesce(controlled_heads.speed, 0.0) * 4.0) / 4.0 AS speed_bucket,
    count(*) AS rollbacks,
    avg(rollback_delta) AS avg_rollback_delta,
    max(rollback_delta) AS max_rollback_delta
FROM rollbacks
JOIN controlled_heads
    ON controlled_heads.process_id = rollbacks.process_id
    AND controlled_heads.tick_id = rollbacks.local_tick
GROUP BY rollbacks.process_id, speed_bucket
ORDER BY rollbacks.process_id, speed_bucket;

SELECT
    'confirmed_history_stale_mismatch_components' AS section,
    replace(CAST(fields.component AS VARCHAR), '\"', '') AS component,
    count(*) AS stale_skips
FROM debug_events
WHERE target = 'lightyear_debug::prediction'
    AND kind = 'confirmed_history_stale_skip_mismatch'
GROUP BY component
ORDER BY stale_skips DESC;

SELECT 'snake_samples_by_schedule' AS section, role, schedule, sample_point, count(*) AS rows
FROM snake_heads
GROUP BY role, schedule, sample_point
ORDER BY role, schedule, sample_point;

WITH fixed_last AS (
    SELECT *
    FROM snake_heads
    WHERE schedule = 'FixedLast'
),
movement AS (
    SELECT
        role,
        process_id,
        entity,
        player_id_bits,
        is_predicted,
        is_interpolated,
        has_simulation_authority,
        is_controlled,
        is_bot,
        tick_id,
        sqrt(
            pow(head_x - lag(head_x) OVER entity_ticks, 2)
            + pow(head_y - lag(head_y) OVER entity_ticks, 2)
        ) AS step_distance
    FROM fixed_last
    WINDOW entity_ticks AS (
        PARTITION BY role, process_id, entity, player_id_bits
        ORDER BY tick_id
    )
),
movement_by_entity AS (
    SELECT
        role,
        process_id,
        entity,
        player_id_bits,
        any_value(is_predicted) AS is_predicted,
        any_value(is_interpolated) AS is_interpolated,
        any_value(has_simulation_authority) AS has_simulation_authority,
        any_value(is_controlled) AS is_controlled,
        any_value(is_bot) AS is_bot,
        count(*) AS samples,
        sum(CASE WHEN step_distance > 0.001 THEN 1 ELSE 0 END) AS moved_ticks,
        sum(coalesce(step_distance, 0.0)) AS total_distance,
        max(coalesce(step_distance, 0.0)) AS max_step_distance
    FROM movement
    GROUP BY role, process_id, entity, player_id_bits
)
SELECT
    'snake_movement_by_entity' AS section,
    role,
    process_id,
    entity,
    player_id_bits,
    is_predicted,
    is_interpolated,
    has_simulation_authority,
    is_controlled,
    is_bot,
    samples,
    moved_ticks,
    total_distance,
    max_step_distance
FROM movement_by_entity
ORDER BY role, process_id, entity
LIMIT 50;

WITH fixed_last AS (
    SELECT *
    FROM snake_heads
    WHERE schedule = 'FixedLast'
),
movement AS (
    SELECT
        role,
        process_id,
        entity,
        player_id_bits,
        tick_id,
        sqrt(
            pow(head_x - lag(head_x) OVER entity_ticks, 2)
            + pow(head_y - lag(head_y) OVER entity_ticks, 2)
        ) AS step_distance
    FROM fixed_last
    WINDOW entity_ticks AS (
        PARTITION BY role, process_id, entity, player_id_bits
        ORDER BY tick_id
    )
),
movement_by_entity AS (
    SELECT
        role,
        process_id,
        entity,
        player_id_bits,
        count(*) AS samples,
        sum(CASE WHEN step_distance > 0.001 THEN 1 ELSE 0 END) AS moved_ticks
    FROM movement
    GROUP BY role, process_id, entity, player_id_bits
)
SELECT
    'stationary_server_snakes' AS section,
    role,
    process_id,
    entity,
    player_id_bits,
    samples,
    moved_ticks
FROM movement_by_entity
WHERE role = 'server' AND samples >= 3 AND moved_ticks = 0
ORDER BY process_id, entity
LIMIT 50;

SELECT
    'invariant_violations' AS section,
    role,
    tick_id,
    entity,
    to_json(fields)->>'invariant' AS invariant,
    to_json(fields)->>'detail' AS detail
FROM debug_events
WHERE kind = 'snake_invariant_violation'
ORDER BY tick_id, role, entity
LIMIT 50;

WITH fixed_heads AS (
    SELECT *
    FROM snake_heads
    WHERE schedule IN ('FixedUpdate', 'FixedLast')
),
head_deltas AS (
    SELECT
        role,
        process_id,
        schedule,
        entity,
        player_id_bits,
        is_predicted,
        is_interpolated,
        is_controlled,
        tick_id,
        head_x - lag(head_x) OVER movement AS dx,
        head_y - lag(head_y) OVER movement AS dy,
        speed
    FROM fixed_heads
    WINDOW movement AS (
        PARTITION BY
            role,
            process_id,
            schedule,
            entity,
            player_id_bits,
            is_predicted,
            is_interpolated,
            is_controlled
        ORDER BY tick_id
    )
)
SELECT
    'largest_fixed_head_deltas' AS section,
    role,
    process_id,
    schedule,
    entity,
    player_id_bits,
    tick_id,
    dx,
    dy,
    speed
FROM head_deltas
WHERE dx IS NOT NULL OR dy IS NOT NULL
ORDER BY sqrt(coalesce(dx, 0.0) * coalesce(dx, 0.0) + coalesce(dy, 0.0) * coalesce(dy, 0.0)) DESC
LIMIT 25;
