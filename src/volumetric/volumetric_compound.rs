use std::num::Zero;
use nalgebra::na;
use volumetric::{Volumetric, InertiaTensor};
use geom::{Compound, CompoundData};
use math::{Scalar, Vect, AngularInertia};

impl Volumetric for CompoundData {
    fn surface(&self) -> Scalar {
        let mut stot: Scalar = na::zero();

        let geoms = self.geoms();
        let props = self.mass_properties_list();

        for (_, &(ref spart, _, _, _)) in geoms.iter().zip(props.iter()) {
            stot = stot + *spart;
        }

        stot
    }

    fn volume(&self) -> Scalar {
        let mut mtot: Scalar = na::zero();

        let geoms = self.geoms();
        let props = self.mass_properties_list();

        for (_, &(_, ref mpart, _, _)) in geoms.iter().zip(props.iter()) {
            mtot = mtot + *mpart;
        }

        mtot
    }

    fn center_of_mass(&self) -> Vect {
        let mut mtot: Scalar = na::zero();
        let mut ctot: Vect   = na::zero();
        let mut gtot: Vect   = na::zero(); // geometric center.

        let geoms = self.geoms();
        let props = self.mass_properties_list();

        for (&(ref m, _), &(_, ref mpart, ref cpart, _)) in geoms.iter().zip(props.iter()) {
            mtot = mtot + *mpart;
            ctot = ctot + m * *cpart * *mpart;
            gtot = gtot + m * *cpart;
        }

        if mtot.is_zero() {
            gtot
        }
        else {
            ctot / mtot
        }
    }

    fn unit_angular_inertia(&self) -> AngularInertia {
        let mut itot: AngularInertia = na::zero();

        let com   = self.center_of_mass();
        let geoms = self.geoms();
        let props = self.mass_properties_list();

        for (&(ref m, _), &(_, ref mpart, ref cpart, ref ipart)) in geoms.iter().zip(props.iter()) {
            itot = itot + ipart.to_world_space(m).to_relative_wrt_point(mpart, &(m * *cpart - com));
        }

        itot
    }

    /// The mass properties of this `CompoundData`.
    ///
    /// If `density` is not zero, it will be multiplied with the density of every object of the
    /// compound geometry.
    fn mass_properties(&self, density: &Scalar) -> (Scalar, Vect, AngularInertia) {
        let mut mtot: Scalar         = na::zero();
        let mut itot: AngularInertia = na::zero();
        let mut ctot: Vect           = na::zero();
        let mut gtot: Vect           = na::zero(); // egometric center.

        let geoms = self.geoms();
        let props = self.mass_properties_list();

        for (&(ref m, _), &(_, ref mpart, ref cpart, _)) in geoms.iter().zip(props.iter()) {
            mtot = mtot + *mpart;
            ctot = ctot + m * *cpart * *mpart;
            gtot = gtot + m * *cpart;
        }

        if mtot.is_zero() {
            ctot = gtot;
        }
        else {
            ctot = ctot / mtot;
        }

        for (&(ref m, _), &(_, ref mpart, ref cpart, ref ipart)) in geoms.iter().zip(props.iter()) {
            itot = itot + ipart.to_world_space(m).to_relative_wrt_point(mpart, &(m * *cpart - ctot));
        }

        (mtot * *density, ctot, itot * *density)
    }
}

impl Volumetric for Compound {
    fn surface(&self) -> Scalar {
        self.surface()
    }

    fn volume(&self) -> Scalar {
        self.mass()
    }

    fn center_of_mass(&self) -> Vect {
        self.center_of_mass().clone()
    }

    fn unit_angular_inertia(&self) -> AngularInertia {
        self.angular_inertia().clone()
    }
}
