use std::num::Zero;
use nalgebra::na::{Indexable, Rotate, Transform, Norm};
use nalgebra::na;
use implicit::{Implicit, HasMargin, PreferedSamplingDirections};
use geom::Cone;
use math::{Scalar, Vect};

impl HasMargin for Cone {
    #[inline]
    fn margin(&self) -> Scalar {
        self.margin()
    }
}

impl<_M: Transform<Vect> + Rotate<Vect>>
Implicit<Vect, _M> for Cone {
    #[inline]
    fn support_point_without_margin(&self, m: &_M, dir: &Vect) -> Vect {
        let local_dir = m.inv_rotate(dir);

        let mut vres = local_dir.clone();

        vres.set(1, na::zero());

        if vres.normalize().is_zero() {
            vres = na::zero();

            if local_dir.at(1).is_negative() {
                vres.set(1, -self.half_height())
            }
            else {
                vres.set(1, self.half_height())
            }
        }
        else {
            vres = vres * self.radius();
            vres.set(1, -self.half_height());

            if na::dot(&local_dir, &vres) < local_dir.at(1) * self.half_height() {
                vres = na::zero();
                vres.set(1, self.half_height())
            }
        }

        m.transform(&vres)
    }
}

impl<_M: Rotate<Vect>>
PreferedSamplingDirections<Vect, _M> for Cone {
    #[inline(always)]
    fn sample(&self, transform: &_M, f: |Vect| -> ()) {
        // Sample along the principal axis
        let mut v: Vect = na::zero();
        v.set(1, na::one());

        let rv = transform.rotate(&v);
        f(-rv);
        f(rv);
    }
}
