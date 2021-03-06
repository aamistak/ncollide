use std::num::Zero;
use nalgebra::na::{Norm, Translation};
use nalgebra::na;
use math::{Scalar, Vect, RotationMatrix, Matrix};
use bounding_volume::{BoundingVolume, BoundingSphere};

// FIXME: make a structure 'cone' ?
#[deriving(Show, PartialEq, Clone, Encodable, Decodable)]
/// A normal cone with a bounding sphere.
pub struct SpacializedCone {
    sphere:  BoundingSphere,
    axis:    Vect,
    hangle:  Scalar,
}

impl SpacializedCone {
    /// Creates a new spacialized cone with a given bounding sphere, axis, and half-angle.
    pub fn new(sphere: BoundingSphere, axis: Vect, hangle: Scalar) -> SpacializedCone {
        let axis = na::normalize(&axis);

        unsafe { SpacializedCone::new_normalized(sphere, axis, hangle) }
    }

    /// Creates a new spacialized cone with a given bounding sphere, unit axis, and half-angle.
    pub unsafe fn new_normalized(sphere: BoundingSphere, axis: Vect, hangle: Scalar) -> SpacializedCone {
        SpacializedCone {
            sphere:  sphere,
            axis:    axis,
            hangle:  hangle
        }
    }

    /// The bounding sphere of this spacialized cone.
    #[inline]
    pub fn sphere<'a>(&'a self) -> &'a BoundingSphere {
        &self.sphere
    }

    /// This cone axis.
    #[inline]
    pub fn axis<'a>(&'a self) -> &'a Vect {
        &self.axis
    }

    /// This cone half angle.
    #[inline]
    pub fn hangle(&self) -> Scalar {
        self.hangle.clone()
    }

    /// Transforms the spacialized cone by `m`.
    pub fn transform_by(&self, m: &Matrix) -> SpacializedCone {
        unsafe {
            let sphere = self.sphere.transform_by(m);
            let axis   = na::rotate(m, &self.axis);
            SpacializedCone::new_normalized(sphere, axis, self.hangle)
        }
    }

    // FIXME: create a Cone bounding volume and move this method to it.
    /// Tests whether the given direction is inside of the cone.
    #[inline]
    pub fn contains_direction(&self, dir: &Vect) -> bool {
        let angle = na::dot(&self.axis, dir);
        let angle = na::clamp(angle, -na::one::<Scalar>(), na::one()).acos();

        angle <= self.hangle
    }
}

#[not_dim4]
impl BoundingVolume for SpacializedCone {
    #[inline]
    fn intersects(&self, other: &SpacializedCone) -> bool {
        if self.sphere.intersects(&other.sphere) {
            let dangle = na::dot(&self.axis, &(-other.axis));
            let dangle = na::clamp(dangle, -na::one::<Scalar>(), na::one()).acos();
            let angsum = self.hangle + other.hangle;

            dangle <= angsum
        }
        else {
            false
        }
    }

    #[inline]
    fn contains(&self, other: &SpacializedCone) -> bool {
        if self.sphere.contains(&other.sphere) {
            fail!("Not yet implemented.")
        }
        else {
            false
        }
    }

    #[inline]
    fn merge(&mut self, other: &SpacializedCone) {
        self.sphere.merge(&other.sphere);

        // merge the cone
        let alpha = na::clamp(na::dot(&self.axis, &other.axis), -na::one::<Scalar>(), na::one()).acos();

        let mut rot_axis = na::cross(&self.axis, &other.axis);
        if !rot_axis.normalize().is_zero() {
            let dangle = (alpha - self.hangle + other.hangle) * na::cast(0.5f64);
            let rot    = na::append_rotation(&na::one::<RotationMatrix>(), &(rot_axis * dangle));

            self.axis    = rot * self.axis;
            self.hangle  = na::clamp((self.hangle + other.hangle + alpha) * na::cast(0.5f64), na::zero(), Float::pi());
        }
        else {
            // This happens if alpha ~= 0 or alpha ~= pi.
            if alpha > na::one() { // NOTE: 1.0 is just a randomly chosen number in-between 0 and pi.
                // alpha ~= pi
                self.hangle = alpha;
            }
            else {
                // alpha ~= 0, do nothing.
            }
        }
    }

    #[inline]
    fn merged(&self, other: &SpacializedCone) -> SpacializedCone {
        let mut res = self.clone();

        res.merge(other);

        res
    }
}

impl Translation<Vect> for SpacializedCone {
    #[inline]
    fn translation(&self) -> Vect {
        self.sphere.center().clone()
    }

    #[inline]
    fn inv_translation(&self) -> Vect {
        -self.sphere.translation()
    }

    #[inline]
    fn append_translation(&mut self, dv: &Vect) {
        self.sphere.append_translation(dv);
    }

    #[inline]
    fn append_translation_cpy(sc: &SpacializedCone, dv: &Vect) -> SpacializedCone {
        SpacializedCone::new(
            Translation::append_translation_cpy(&sc.sphere, dv),
            sc.axis.clone(),
            sc.hangle.clone())
    }

    #[inline]
    fn prepend_translation(&mut self, dv: &Vect) {
        self.sphere.append_translation(dv)
    }

    #[inline]
    fn prepend_translation_cpy(sc: &SpacializedCone, dv: &Vect) -> SpacializedCone {
        Translation::append_translation_cpy(sc, dv)
    }

    #[inline]
    fn set_translation(&mut self, v: Vect) {
        self.sphere.set_translation(v)
    }
}

#[cfg(test)]
mod test {
    use nalgebra::na::Vec3;
    use nalgebra::na;
    use super::SpacializedCone;
    use bounding_volume::{BoundingVolume, BoundingSphere};
    use math::Scalar;

    #[test]
    #[dim3]
    fn test_merge_vee() {
        let sp   = BoundingSphere::new(na::zero(), na::one());
        let pi: Scalar = Float::pi();
        let pi_12 = pi / na::cast(12.0f64);
        let a    = SpacializedCone::new(sp.clone(), Vec3::new(1.0, 1.0, 0.0), pi_12);
        let b    = SpacializedCone::new(sp.clone(), Vec3::new(-1.0, 1.0, 0.0), pi_12);

        let ab = a.merged(&b);

        assert!(na::approx_eq(&ab.hangle, &(pi_12 * na::cast(4.0f64))))
    }
}
