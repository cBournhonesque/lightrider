/// Different layers for collision
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CollideLayer {
    Player,
    Food,
    Wall,
}
