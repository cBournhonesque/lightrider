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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_projection() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(0.0, 100.0);
        assert_eq!(segment_contains_point(&a, &b, &Vec2::new(0.0, 50.0)), true);

        assert_eq!(project_on_segment(&a, &b, &Vec2::new(10.0, 50.0)), Vec2::new(0.0, 50.0));
    }

}