//! Support mapping based Cone geometry.

use nalgebra::na;
use math::Scalar;

/// Implicit description of a cylinder geometry with its principal axis aligned with the `y` axis.
#[deriving(PartialEq, Show, Clone, Encodable, Decodable)]
pub struct Cone {
    half_height: Scalar,
    radius:      Scalar,
    margin:      Scalar
}

impl Cone {
    /// Creates a new cone.
    ///
    /// # Arguments:
    /// * `half_height` - the half length of the cone along the `y` axis.
    /// * `radius` - the length of the cone along all other axis.
    pub fn new(half_height: Scalar, radius: Scalar) -> Cone {
        Cone::new_with_margin(half_height, radius, na::cast(0.04f64))
    }

    /// Creates a new cone with a custom margin.
    ///
    /// # Arguments:
    /// * `half_height` - the half length of the cone along the `y` axis.
    /// * `radius` - the length of the cone along all other axis.
    /// * `margin` - the  cone margin.
    pub fn new_with_margin(half_height: Scalar, radius: Scalar, margin: Scalar) -> Cone {
        assert!(half_height.is_positive() && radius.is_positive());

        Cone {
            half_height: half_height,
            radius:      radius,
            margin:      margin
        }
    }
}

impl Cone {
    /// The cone half length along the `y` axis.
    #[inline]
    pub fn half_height(&self) -> Scalar {
        self.half_height.clone()
    }

    /// The radius of the cone along all but the `y` axis.
    #[inline]
    pub fn radius(&self) -> Scalar {
        self.radius.clone()
    }

    /// The margin around the cone.
    #[inline]
    pub fn margin(&self) -> Scalar {
        self.margin.clone()
    }
}
