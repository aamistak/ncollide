use math::Scalar;

/// A Ball geometry.
#[deriving(PartialEq, Show, Clone, Encodable, Decodable)]
pub struct Ball {
    radius: Scalar
}

impl Ball {
    /// Creates a new ball from its radius and center.
    #[inline]
    pub fn new(radius: Scalar) -> Ball {
        Ball { radius: radius }
    }
}

impl Ball {
    /// The ball radius.
    #[inline]
    pub fn radius(&self) -> Scalar {
        self.radius.clone()
    }
}
