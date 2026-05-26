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
    fields.is_controlled::BOOLEAN AS is_controlled,
    fields.is_bot::BOOLEAN AS is_bot
FROM debug_events
WHERE kind = 'snake_head';

SELECT 'events_by_kind' AS section, target, kind, count(*) AS rows
FROM debug_events
GROUP BY target, kind
ORDER BY rows DESC, target, kind;

SELECT 'snake_samples_by_schedule' AS section, role, schedule, sample_point, count(*) AS rows
FROM snake_heads
GROUP BY role, schedule, sample_point
ORDER BY role, schedule, sample_point;

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
