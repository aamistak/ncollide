use std::any::AnyRefExt;
use nalgebra::na;
use data::hash_map::HashMap;
use data::hash::UintTWHash;
use bounding_volume::{BoundingVolume, HasAABB};
use broad::Dispatcher;
use narrow::{CollisionDetector, GeomGeomDispatcher, GeomGeomCollisionDetector,
             DynamicCollisionDetector, CollisionDetectorFactory, Contact};
use geom::{Geom, ConcaveGeom};
use math::Matrix;

/// Collision detector between a concave geometry and another geometry.
pub struct ConcaveGeomGeom<G1, G2> {
    sub_detectors: HashMap<uint, Box<GeomGeomCollisionDetector+Send>, UintTWHash>,
    to_delete:     Vec<uint>,
    interferences: Vec<uint>
}

impl<G1, G2> ConcaveGeomGeom<G1, G2> {
    /// Creates a new collision detector between a concave geometry and another geometry.
    pub fn new() -> ConcaveGeomGeom<G1, G2> {
        ConcaveGeomGeom {
            sub_detectors: HashMap::new_with_capacity(5, UintTWHash::new()),
            to_delete:     Vec::new(),
            interferences: Vec::new()
        }
    }
}

impl<G1: ConcaveGeom, G2: Geom> ConcaveGeomGeom<G1, G2> {
    fn do_update(&mut self,
                 dispatcher: &GeomGeomDispatcher,
                 m1:         &Matrix,
                 g1:         &G1,
                 m2:         &Matrix,
                 g2:         &G2,
                 swap:       bool) {
        // Find new collisions
        let ls_m2    = na::inv(m1).expect("The transformation `m1` must be inversible.") * *m2;
        let ls_aabb2 = g2.aabb(&ls_m2);
        let g2       = g2 as &Geom;

        g1.approx_interferences_with_aabb(&ls_aabb2, &mut self.interferences);

        for i in self.interferences.iter() {
            let detector = g1.map_part_at(*i, |_, g1| {
                if swap {
                    dispatcher.dispatch(g2, g1)
                }
                else {
                    dispatcher.dispatch(g1, g2)
                }
            });

            match detector {
                Some(detector) => {
                    let _ = self.sub_detectors.insert_or_replace(*i, detector, false);
                },
                None => { }
            }
        }

        self.interferences.clear();

        // Update all collisions
        for detector in self.sub_detectors.elements_mut().mut_iter() {
            let key = detector.key;
            if ls_aabb2.intersects(g1.aabb_at(key)) {
                g1.map_transformed_part_at(m1, key, |m1, g1| {
                    if swap {
                        detector.value.update(dispatcher, m2, g2, m1, g1);
                    }
                    else {
                        detector.value.update(dispatcher, m1, g1, m2, g2);
                    }
                });
            }
            else {
                // FIXME: ask the detector if it wants to be removed or not
                self.to_delete.push(key);
            }
        }

        // Remove outdated sub detectors
        for i in self.to_delete.iter() {
            self.sub_detectors.remove(i);
        }

        self.to_delete.clear();
    }
}

impl<G1: 'static + ConcaveGeom, G2: 'static + Geom>
GeomGeomCollisionDetector for ConcaveGeomGeom<G1, G2> {
    fn update(&mut self,
              dispatcher: &GeomGeomDispatcher,
              m1:         &Matrix,
              g1:         &Geom,
              m2:         &Matrix,
              g2:         &Geom) {
        self.do_update(dispatcher,
                       m1,
                       g1.downcast_ref::<G1>().expect("Invalid geometry."),
                       m2,
                       g2.downcast_ref::<G2>().expect("Invalid geometry."),
                       false);
    }

    fn num_colls(&self) -> uint {
        let mut res = 0;

        for detector in self.sub_detectors.elements().iter() {
            res = res + detector.value.num_colls()
        }

        res
    }

    fn colls(&self, out: &mut Vec<Contact>) {
        for detector in self.sub_detectors.elements().iter() {
            detector.value.colls(out);
        }
    }
}

impl<G1: ConcaveGeom, G2: Geom>
DynamicCollisionDetector<G1, G2> for ConcaveGeomGeom<G1, G2> { }

/// Collision detector between a geometry and a concave geometry.
pub struct GeomConcaveGeom<G1, G2> {
    sub_detector: ConcaveGeomGeom<G2, G1>
}

impl<G1, G2> GeomConcaveGeom<G1, G2> {
    /// Creates a new collision detector between a geometry and a concave geometry.
    pub fn new() -> GeomConcaveGeom<G1, G2> {
        GeomConcaveGeom {
            sub_detector: ConcaveGeomGeom::new()
        }
    }
}

impl<G1: 'static + Geom, G2: 'static + ConcaveGeom>
GeomGeomCollisionDetector for GeomConcaveGeom<G1, G2> {
    fn update(&mut self,
              dispatcher: &GeomGeomDispatcher,
              m1:         &Matrix,
              g1:         &Geom,
              m2:         &Matrix,
              g2:         &Geom) {
        self.sub_detector.do_update(dispatcher,
                                    m2,
                                    g2.downcast_ref::<G2>().expect("Invalid geometry."),
                                    m1,
                                    g1.downcast_ref::<G1>().expect("Invalid geometry."),
                                    true);
    }

    fn num_colls(&self) -> uint {
        self.sub_detector.num_colls()
    }

    fn colls(&self, out: &mut Vec<Contact>) {
        self.sub_detector.colls(out)
    }
}

impl<G1: Geom, G2: ConcaveGeom>
DynamicCollisionDetector<G1, G2> for GeomConcaveGeom<G1, G2> { }

/*
 *
 * Custom factories
 *
 */
/// Structure implementing `CollisionDetectorFactory` in order to create a new `ConcaveGeomGeom`
/// collision detector.
pub struct ConcaveGeomGeomFactory<G1, G2>;

impl<G1: 'static + ConcaveGeom, G2: 'static + Geom>
CollisionDetectorFactory for ConcaveGeomGeomFactory<G1, G2> {
    fn build(&self) -> Box<GeomGeomCollisionDetector + Send> {
        let res: ConcaveGeomGeom<G1, G2> = ConcaveGeomGeom::new();
        box res as Box<GeomGeomCollisionDetector + Send>
    }
}

/// Structure implementing `CollisionDetectorFactory` in order to create a new `GeomConcaveGeom`
/// collision detector.
pub struct GeomConcaveGeomFactory<G1, G2>;

impl<G1: 'static + Geom,
     G2: 'static + ConcaveGeom>
CollisionDetectorFactory for GeomConcaveGeomFactory<G1, G2> {
    fn build(&self) -> Box<GeomGeomCollisionDetector + Send> {
        let res: GeomConcaveGeom<G1, G2> = GeomConcaveGeom::new();
        box res as Box<GeomGeomCollisionDetector + Send>
    }
}
