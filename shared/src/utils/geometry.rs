use bevy::prelude::Vec2;
use parry2d::math::Point;
use parry2d::query::PointQuery;
use parry2d::shape::Segment;

/// Checks if point p is contained in [a, b]
pub fn segment_contains_point(a: &Vec2, b: &Vec2, p: &Vec2) -> bool {
    let segment = Segment::new(Point::new(a.x, a.y), Point::new(b.x, b.y));
    segment.contains_local_point(&Point::new(p.x, p.y))
}

pub fn project_on_segment(a: &Vec2, b: &Vec2, p: &Vec2) -> Vec2 {
    let segment = Segment::new(Point::new(a.x, a.y), Point::new(b.x, b.y));
    let proj = segment.project_local_point(&Point::new(p.x, p.y), true);
    Vec2::new(proj.point.x, proj.point.y)
}

pub fn ray_segment_intersection(
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
    segment_start: Vec2,
    segment_end: Vec2,
) -> Option<f32> {
    let segment = segment_end - segment_start;
    let denominator = cross(direction, segment);
    if denominator.abs() <= f32::EPSILON {
        return None;
    }

    let offset = segment_start - origin;
    let ray_distance = cross(offset, segment) / denominator;
    let segment_fraction = cross(offset, direction) / denominator;

    if ray_distance >= 0.0
        && ray_distance <= max_distance
        && segment_fraction >= 0.0
        && segment_fraction <= 1.0
    {
        Some(ray_distance)
    } else {
        None
    }
}

fn cross(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_projection() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(0.0, 100.0);
        assert_eq!(segment_contains_point(&a, &b, &Vec2::new(0.0, 50.0)), true);

        assert_eq!(
            project_on_segment(&a, &b, &Vec2::new(10.0, 50.0)),
            Vec2::new(0.0, 50.0)
        );
    }

    #[test]
    fn test_ray_segment_intersection() {
        assert_eq!(
            ray_segment_intersection(
                Vec2::ZERO,
                Vec2::Y,
                10.0,
                Vec2::new(-1.0, 5.0),
                Vec2::new(1.0, 5.0),
            ),
            Some(5.0),
        );
        assert_eq!(
            ray_segment_intersection(
                Vec2::ZERO,
                Vec2::Y,
                10.0,
                Vec2::new(1.0, 0.0),
                Vec2::new(1.0, 5.0),
            ),
            None,
        );
        assert_eq!(
            ray_segment_intersection(
                Vec2::ZERO,
                Vec2::Y,
                4.0,
                Vec2::new(-1.0, 5.0),
                Vec2::new(1.0, 5.0),
            ),
            None,
        );
    }
}
