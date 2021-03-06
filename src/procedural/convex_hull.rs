use std::num::{Bounded, Zero};
use nalgebra::na::{Identity, Vec2, Vec3, Rot3, Mat3, Norm, FloatVec, Col, Diag};
use nalgebra::na;
use math::Scalar;
use utils;
use procedural::{Polyline, TriMesh, UnifiedIndexBuffer};
use bounding_volume;
use implicit;

// FIXME: factorize with the one on hacd.
fn normalize(coords: &mut [Vec3<Scalar>]) -> (Vec3<Scalar>, Scalar) {
    let (mins, maxs) = bounding_volume::point_cloud_aabb(&Identity::new(), coords.as_slice());
    let diag = na::norm(&(maxs - mins));
    let _2: Scalar = na::cast(2.0f64);
    let center: Vec3<Scalar> = (mins + maxs) / _2;

    for c in coords.mut_iter() {
        *c = (*c - center) / diag;
    }

    (center, diag)
}

fn denormalize(coords: &mut [Vec3<Scalar>], center: &Vec3<Scalar>, diag: &Scalar) {
    for c in coords.mut_iter() {
        *c = *c * *diag + *center;
    }
}


/// Computes the convex hull of a set of 3d points.
pub fn convex_hull3d(points: &[Vec3<Scalar>]) -> TriMesh<Scalar, Vec3<Scalar>> {
    assert!(points.len() != 0, "Cannot compute the convex hull of an empty set of point.");

    let mut points = Vec::from_slice(points);

    let (norm_center, norm_diag) = normalize(points.as_mut_slice());

    let mut undecidable_points  = Vec::new();
    let mut horizon_loop_facets = Vec::new();
    let mut horizon_loop_ids    = Vec::new();
    let mut removed_facets      = Vec::new();

    let mut triangles;
    let     denormalizer;
    
    match get_initial_mesh(points.as_mut_slice(), &mut undecidable_points) {
        Facets(facets, denorm)   => {
            triangles    = facets;
            denormalizer = denorm;
        },
        ResultMesh(mut mesh) => {
            denormalize(mesh.coords.as_mut_slice(), &norm_center, &norm_diag);
            return mesh
        }
    }

    let mut i = 0;
    while i != triangles.len() {
        horizon_loop_facets.clear();
        horizon_loop_ids.clear();

        if !triangles[i].valid {
            i = i + 1;
            continue;
        }

        // FIXME: use triangles[i].furthest_point instead.
        let pt_id = support_point(&triangles[i].normal,
                                  points.as_slice(),
                                  triangles[i].visible_points.as_slice());

        match pt_id {
            Some(point) => {
                removed_facets.clear();

                triangles.get_mut(i).valid = false;
                removed_facets.push(i);

                for j in range(0u, 3) {
                    compute_silhouette(triangles[i].adj[j],
                                       triangles[i].indirect_adj_id[j],
                                       point,
                                       &mut horizon_loop_facets,
                                       &mut horizon_loop_ids,
                                       points.as_slice(),
                                       &mut removed_facets,
                                       triangles.as_mut_slice());
                }

                if horizon_loop_facets.is_empty() {
                    // Due to inaccuracies, the silhouette could not be computed
                    // (the point seems to be visible from… every triangle).
                    let mut any_valid = false;
                    for j in range(i + 1, triangles.len()) {
                        if triangles[j].valid {
                            any_valid = true;
                        }
                    }

                    if any_valid {
                        println!("Warning: exitting an unfinished work.");
                    }

                    // FIXME: this is verry harsh.
                    triangles.get_mut(i).valid = true;
                    break;
                }

                attach_and_push_facets_3d(horizon_loop_facets.as_slice(),
                                          horizon_loop_ids.as_slice(),
                                          point,
                                          points.as_slice(),
                                          &mut triangles,
                                          removed_facets.as_slice(),
                                          &mut undecidable_points);
            },
            None => { }
        }

        i = i + 1;
    }

    let mut idx = Vec::new();

    for facet in triangles.iter() {
        if facet.valid {
            idx.push(Vec3::new(facet.pts[0] as u32, facet.pts[1] as u32, facet.pts[2] as u32));
        }
    }

    utils::remove_unused_points(&mut points, idx.as_mut_slice());

    assert!(points.len() != 0, "Internal error: empty output mesh.");

    for point in points.mut_iter() {
        *point = denormalizer * *point;
    }

    denormalize(points.as_mut_slice(), &norm_center, &norm_diag);

    TriMesh::new(points, None, None, Some(UnifiedIndexBuffer(idx)))
}

enum InitialMesh {
    Facets(Vec<TriangleFacet>, Mat3<Scalar>),
    ResultMesh(TriMesh<Scalar, Vec3<Scalar>>)
}

fn build_degenerate_mesh_point(point: Vec3<Scalar>) -> TriMesh<Scalar, Vec3<Scalar>> {
    let ta = Vec3::new(0u32, 0, 0);
    let tb = Vec3::new(0u32, 0, 0);

    TriMesh::new(vec!(point), None, None, Some(UnifiedIndexBuffer(vec!(ta, tb))))
}

fn build_degenerate_mesh_segment(dir: &Vec3<Scalar>, points: &[Vec3<Scalar>]) -> TriMesh<Scalar, Vec3<Scalar>> {
    let a = implicit::point_cloud_support_point(dir, points);
    let b = implicit::point_cloud_support_point(&-dir, points);

    let ta = Vec3::new(0u32, 1, 0);
    let tb = Vec3::new(1u32, 0, 0);

    TriMesh::new(vec!(a, b), None, None, Some(UnifiedIndexBuffer(vec!(ta, tb))))
}

fn get_initial_mesh(points: &mut [Vec3<Scalar>], undecidable: &mut Vec<uint>) -> InitialMesh {
    /*
     * Compute the eigenvectors to see if the input datas live on a subspace.
     */
    let cov              = utils::cov(points);
    let (eigvec, eigval) = na::eigen_qr(&cov, &Float::epsilon(), 1000);
    let mut eigpairs = [ (eigvec.col(0), eigval.x), (eigvec.col(1), eigval.y), (eigvec.col(2), eigval.z) ];

    /*
     * Sort in deacreasing order wrt. eigenvalues.
     */
    eigpairs.sort_by(|a, b| {
        if *a.ref1() > *b.ref1() {
            Less // `Less` and `Greater` are reversed.
        }
        else if *a.ref1() < *b.ref1() {
            Greater
        }
        else {
            Equal
        }
    });

    /*
     * Count the dimension the data lives in.
     */
    let mut dim = 0;
    while dim < 3 {
        if na::approx_eq_eps(eigpairs[dim].ref1(), &na::zero(), &na::cast(1.0e-7f64)) {
            break;
        }

        dim = dim + 1;
    }

    match dim {
        0 => {
            // The hull is a point.
            ResultMesh(build_degenerate_mesh_point(points[0]))
        },
        1 => {
            // The hull is a segment.
            ResultMesh(build_degenerate_mesh_segment(eigpairs[0].ref0(), points))
        },
        2 => {
            // The hull is a triangle.
            // Project into the principal plane…
            let axis1 = eigpairs[0].ref0();
            let axis2 = eigpairs[1].ref0();

            let mut subspace_points = Vec::with_capacity(points.len());

            for point in points.iter() {
                subspace_points.push(Vec2::new(na::dot(point, axis1), na::dot(point, axis2)))
            }

            // … and compute the 2d convex hull.
            let idx = convex_hull2d_idx(subspace_points.as_slice());

            // Finalize the result, triangulating the polyline.
            let npoints = idx.len();
            let coords  = idx.move_iter().map(|i| points[i].clone()).collect();
            let mut triangles = Vec::with_capacity(npoints + npoints - 4);

            let a = 0u32;

            for id in range(1u32, npoints as u32 - 1) {
                triangles.push(Vec3::new(a, id, id + 1));
                triangles.push(Vec3::new(id, a, id + 1));
            }

            ResultMesh(TriMesh::new(coords, None, None, Some(UnifiedIndexBuffer(triangles))))
        },
        3 => {
            // The hull is a polyedra.
            // Find a initial triangle lying on the principal plane…
            let _1: Scalar = na::one();
            let diag: Mat3<Scalar> = Diag::from_diag(&Vec3::new(_1 / eigval.x, _1 / eigval.y, _1 / eigval.z));
            let icov = eigvec * diag * na::transpose(&eigvec);

            for point in points.mut_iter() {
                *point = icov * *point;
            }

            let p1 = support_point_2(eigpairs[0].ref0(), points, [].as_slice()).unwrap();
            let p2 = support_point_2(&-eigpairs[0].val0(), points, [].as_slice()).unwrap();

            let mut max_area = na::zero();
            let mut p3       = Bounded::max_value();

            for (i, point) in points.iter().enumerate() {
                let area = utils::triangle_area(&points[p1], &points[p2], &points[i]);

                if area > max_area {
                    max_area = area ;
                    p3 = i;
                }
            }

            assert!(p3 != Bounded::max_value(), "Internal convex hull error: no triangle found.");

            // Build two facets with opposite normals
            let mut f1 = TriangleFacet::new(p1, p2, p3, points);
            let mut f2 = TriangleFacet::new(p2, p1, p3, points);

            // Link the facets together
            f1.set_facets_adjascency(1, 1, 1, 0, 2, 1);
            f2.set_facets_adjascency(0, 0, 0, 0, 2, 1);

            let mut facets = vec!(f1, f2);

            // … and attribute visible points to each one of them.
            // FIXME: refactor this with the two others.
            let mut ignored = 0u;
            for point in range(0, points.len()) {
                if point == p1 || point == p2 || point == p3 {
                    continue;
                }

                let mut furthest      = Bounded::max_value();
                let mut furthest_dist = na::zero();

                for (i, curr_facet) in facets.iter().enumerate() {
                    if curr_facet.can_be_seen_by(point, points) {
                        let dist = curr_facet.distance_to_point(point, points);

                        if dist > furthest_dist {
                            furthest      = i;
                            furthest_dist = dist;
                        }
                    }
                }

                if furthest != Bounded::max_value() {
                    facets.get_mut(furthest).add_visible_point(point, points);
                }
                else {
                    undecidable.push(point);
                    ignored = ignored + 1;
                }

                // If none of the facet can be seen from the point, it is naturally deleted.
            }

            verify_facet_links(0, facets.as_slice());
            verify_facet_links(1, facets.as_slice());

            Facets(facets, cov)
        },
        _ => unreachable!()
    }
}

fn support_point<N: Float, V: FloatVec<N>>(direction: &V, points : &[V], idx: &[uint]) -> Option<uint> {
    let mut argmax = None;
    let _M: N      = Bounded::max_value();
    let mut max    = -_M;

    for i in idx.iter() {
        let dot = na::dot(direction, &points[*i]);

        if dot > max {
            argmax = Some(*i);
            max    = dot;
        }
    }

    argmax
}

// FIXME: uggly, find a way to refactor all the support point functions!
fn support_point_2<N: Float, V: FloatVec<N>>(direction: &V, points : &[V], except: &[uint]) -> Option<uint> {
    let mut argmax = None;
    let _M: N      = Bounded::max_value();
    let mut max    = -_M;

    for (id, pt) in points.iter().enumerate() {
        if except.contains(&id) {
            continue;
        }

        let dot = na::dot(direction, pt);

        if dot > max {
            argmax = Some(id);
            max    = dot;
        }
    }

    argmax
}

fn compute_silhouette(facet:          uint,
                      indirectID :    uint,
                      point:          uint,
                      out_facets:     &mut Vec<uint>,
                      out_adj_idx:    &mut Vec<uint>,
                      points:         &[Vec3<Scalar>],
                      removedFacets : &mut Vec<uint>,
                      triangles:      &mut [TriangleFacet]) {
    if triangles[facet].valid {
        if !triangles[facet].can_be_seen_by_or_is_affinely_dependent_with_contour(point, points, indirectID) {
            out_facets.push(facet);
            out_adj_idx.push(indirectID);
        }
        else {
            triangles[facet].valid = false; // The facet must be removed from the convex hull.
            removedFacets.push(facet);

            compute_silhouette(triangles[facet].adj[(indirectID + 1) % 3],
                               triangles[facet].indirect_adj_id[(indirectID + 1) % 3],
                               point,
                               out_facets,
                               out_adj_idx,
                               points,
                               removedFacets,
                               triangles);
            compute_silhouette(triangles[facet].adj[(indirectID + 2) % 3],
                               triangles[facet].indirect_adj_id[(indirectID + 2) % 3],
                               point,
                               out_facets,
                               out_adj_idx,
                               points,
                               removedFacets,
                               triangles);
        }
    }
}

fn verify_facet_links(ifacet: uint, facets: &[TriangleFacet]) {
    let facet = &facets[ifacet];

    for i in range(0u, 3) {
        let adji = &facets[facet.adj[i]];

        assert!(
            adji.adj[facet.indirect_adj_id[i]] == ifacet &&
            adji.first_point_from_edge(facet.indirect_adj_id[i]) == facet.second_point_from_edge(adji.indirect_adj_id[facet.indirect_adj_id[i]]) &&
            adji.second_point_from_edge(facet.indirect_adj_id[i]) == facet.first_point_from_edge(adji.indirect_adj_id[facet.indirect_adj_id[i]]))
    }
}

fn attach_and_push_facets_3d(horizon_loop_facets: &[uint],
                             horizon_loop_ids:    &[uint],
                             point:               uint,
                             points:              &[Vec3<Scalar>],
                             triangles:           &mut Vec<TriangleFacet>,
                             removed_facets:      &[uint],
                             undecidable:         &mut Vec<uint>) {
    // The horizon is built to be in CCW order.
    let mut new_facets = Vec::with_capacity(horizon_loop_facets.len());

    // Create new facets.
    let mut adj_facet:  uint;
    let mut indirectId: uint;

    for i in range(0, horizon_loop_facets.len()) {
        adj_facet  = horizon_loop_facets[i];
        indirectId = horizon_loop_ids[i];

        let facet = TriangleFacet::new(point,
                                       (*triangles)[adj_facet].second_point_from_edge(indirectId),
                                       (*triangles)[adj_facet].first_point_from_edge(indirectId),
                                       points);
        new_facets.push(facet);
    }

    // Link the facets together.
    for i in range(0, horizon_loop_facets.len()) {
        let prev_facet;

        if i == 0 {
            prev_facet = triangles.len() + horizon_loop_facets.len() - 1;
        }
        else {
            prev_facet = triangles.len() + i - 1;
        }

        let middle_facet = horizon_loop_facets[i];
        let next_facet   = triangles.len() + (i + 1) % horizon_loop_facets.len();
        let middle_id    = horizon_loop_ids[i];

        new_facets.get_mut(i).set_facets_adjascency(prev_facet, middle_facet, next_facet,
                                                    2         , middle_id   , 0);
        triangles.get_mut(middle_facet).adj[middle_id] = triangles.len() + i; // The future id of curr_facet.
        triangles.get_mut(middle_facet).indirect_adj_id[middle_id] = 1;
    }

    // Assign to each facets some of the points which can see it.
    // FIXME: refactor this with the others.
    for curr_facet in removed_facets.iter() {
        for visible_point in (*triangles)[*curr_facet].visible_points.iter() {
            if *visible_point == point {
                continue;
            }

            let mut furthest      = Bounded::max_value();
            let mut furthest_dist = na::zero();

            for (i, curr_facet) in new_facets.mut_iter().enumerate() {
                if curr_facet.can_be_seen_by(*visible_point, points) {
                    let dist = curr_facet.distance_to_point(*visible_point, points);

                    if dist > furthest_dist {
                        furthest      = i;
                        furthest_dist = dist;
                    }
                }
            }

            if furthest != Bounded::max_value() {
                new_facets.get_mut(furthest).add_visible_point(*visible_point, points);
            }

            // If none of the facet can be seen from the point, it is naturally deleted.
        }
    }

    // Try to assign collinear points to one of the new facets.
    let mut i = 0;

    while i != undecidable.len() {
        let mut furthest      = Bounded::max_value();
        let mut furthest_dist = na::zero();
        let undecidable_point = (*undecidable)[i];

        for (j, curr_facet) in new_facets.mut_iter().enumerate() {
            if curr_facet.can_be_seen_by(undecidable_point, points) {
                let dist = curr_facet.distance_to_point(undecidable_point, points);

                if dist > furthest_dist {
                    furthest      = j;
                    furthest_dist = dist;
                }
            }
        }

        if furthest != Bounded::max_value() {
            new_facets.get_mut(furthest).add_visible_point(undecidable_point, points);
            let _ = undecidable.swap_remove(i);
        }
        else {
            i = i + 1;
        }
    }

    // Push facets.
    // FIXME: can we avoid the tmp vector `new_facets` ?
    for curr_facet in new_facets.move_iter() {
        triangles.push(curr_facet);
    }
}


struct TriangleFacet {
    valid:             bool,
    normal:            Vec3<Scalar>,
    adj:               [uint, ..3],
    indirect_adj_id:   [uint, ..3],
    pts:               [uint, ..3],
    visible_points:    Vec<uint>,
    furthest_point:    uint,
    furthest_distance: Scalar
}


impl TriangleFacet {
    pub fn new(p1: uint, p2: uint, p3: uint, points: &[Vec3<Scalar>]) -> TriangleFacet {
        let p1p2 = points[p2] - points[p1];
        let p1p3 = points[p3] - points[p1];

        let mut normal = na::cross(&p1p2, &p1p3);
        if normal.normalize().is_zero() {
            let a = points[p1];
            let b = points[p2];
            let c = points[p3];
            println!("pts ids: {} {} {}", p1, p2, p3);
            println!("pts: {} {} {}", a, b, c);
            println!("{}", utils::is_affinely_dependent_triangle(&a, &b, &c));
            println!("{}", utils::is_affinely_dependent_triangle(&a, &c, &b));
            println!("{}", utils::is_affinely_dependent_triangle(&b, &a, &c));
            println!("{}", utils::is_affinely_dependent_triangle(&b, &c, &a));
            println!("{}", utils::is_affinely_dependent_triangle(&c, &a, &b));
            println!("{}", utils::is_affinely_dependent_triangle(&c, &b, &a));

            fail!("Convex hull failure: a facet must not be affinely dependent.");
        }

        TriangleFacet {
            valid:             true,
            normal:            normal,
            adj:               [0, 0, 0],
            indirect_adj_id:   [0, 0, 0],
            pts:               [p1, p2, p3],
            visible_points:    Vec::new(),
            furthest_point:    Bounded::max_value(),
            furthest_distance: na::zero()
        }
    }

    pub fn add_visible_point(&mut self, pid: uint, points: &[Vec3<Scalar>]) {
        let dist = self.distance_to_point(pid, points);

        if dist > self.furthest_distance {
            self.furthest_distance = dist;
            self.furthest_point    = pid;
        }

        self.visible_points.push(pid);
    }

    pub fn distance_to_point(&self, point: uint, points: &[Vec3<Scalar>]) -> Scalar {
        na::dot(&self.normal, &(points[point] - points[self.pts[0]]))
    }

    pub fn set_facets_adjascency(&mut self,
                                 adj1:   uint,
                                 adj2:   uint,
                                 adj3:   uint,
                                 idAdj1: uint,
                                 idAdj2: uint,
                                 idAdj3: uint) {
        self.indirect_adj_id[0] = idAdj1;
        self.indirect_adj_id[1] = idAdj2;
        self.indirect_adj_id[2] = idAdj3;

        self.adj[0] = adj1;
        self.adj[1] = adj2;
        self.adj[2] = adj3;
    }

    pub fn first_point_from_edge(&self, id: uint) -> uint {
        self.pts[id]
    }

    pub fn second_point_from_edge(&self, id: uint) -> uint {
        self.pts[(id + 1) % 3]
    }

    /*
    pub fn opposite_point_to_edge(&self, id: uint) -> uint {
        self.pts[(id + 2) % 3]
    }
    */

    pub fn can_be_seen_by(&self, point: uint, points: &[Vec3<Scalar>]) -> bool {
        let p0 = &points[self.pts[0]];
        let p1 = &points[self.pts[1]];
        let p2 = &points[self.pts[2]];
        let pt = &points[point];

        let _eps: Scalar = Float::epsilon();

        na::dot(&(*pt - *p0), &self.normal) > _eps * na::cast(100.0f64) &&
        !utils::is_affinely_dependent_triangle(p0, p1, pt) &&
        !utils::is_affinely_dependent_triangle(p0, p2, pt) &&
        !utils::is_affinely_dependent_triangle(p1, p2, pt)
    }

    pub fn can_be_seen_by_or_is_affinely_dependent_with_contour(&self,
                                                                point:  uint,
                                                                points: &[Vec3<Scalar>],
                                                                edge:   uint) -> bool {
        let p0 = &points[self.first_point_from_edge(edge)];
        let p1 = &points[self.second_point_from_edge(edge)];
        let pt = &points[point];

        let aff_dep = utils::is_affinely_dependent_triangle(p0, p1, pt) ||
                      utils::is_affinely_dependent_triangle(p0, pt, p1) ||
                      utils::is_affinely_dependent_triangle(p1, p0, pt) ||
                      utils::is_affinely_dependent_triangle(p1, pt, p0) ||
                      utils::is_affinely_dependent_triangle(pt, p0, p1) ||
                      utils::is_affinely_dependent_triangle(pt, p1, p0);

        na::dot(&(*pt - *p0), &self.normal) >= na::zero() || aff_dep
    }
}


/// Computes the convex hull of a set of 2d points.
pub fn convex_hull2d(points: &[Vec2<Scalar>]) -> Polyline<Scalar, Vec2<Scalar>> {
    let idx     = convex_hull2d_idx(points);
    let mut pts = Vec::new();

    for id in idx.move_iter() {
        pts.push(points[id].clone());
    }

    Polyline::new(pts, None)
}

/// Computes the convex hull of a set of 2d points and returns only the indices of the hull
/// vertices.
pub fn convex_hull2d_idx(points: &[Vec2<Scalar>]) -> Vec<uint> {
    let mut undecidable_points = Vec::new();
    let mut segments           = get_initial_polyline(points, &mut undecidable_points);

    let mut i = 0;
    while i != segments.len() {
        if !segments[i].valid {
            i = i + 1;
            continue;
        }

        let pt_id = support_point(&segments[i].normal,
                                  points,
                                  segments[i].visible_points.as_slice());

        match pt_id {
            Some(point) => {
                segments.get_mut(i).valid = false;

                attach_and_push_facets_2d(segments[i].prev,
                                          segments[i].next,
                                          point,
                                          points.as_slice(),
                                          &mut segments,
                                          i,
                                          &mut undecidable_points);
            },
            None => { }
        }

        i = i + 1;
    }

    let mut idx        = Vec::new();
    let mut curr_facet = 0;

    while !segments[curr_facet].valid {
        curr_facet = curr_facet + 1
    }

    let first_facet = curr_facet;

    loop {
        let curr = &segments[curr_facet];

        assert!(curr.valid);

        idx.push(curr.pts[0]);

        curr_facet = curr.next;

        if curr_facet == first_facet {
            break;
        }
    }

    idx
}

pub fn get_initial_polyline(points: &[Vec2<Scalar>], undecidable: &mut Vec<uint>) -> Vec<SegmentFacet> {
    let mut res = Vec::new();

    assert!(points.len() >= 2);

    let p1     = support_point_2(&Vec2::x(), points, [].as_slice()).unwrap();
    let mut p2 = p1;

    let direction = [
        -Vec2::x(),
        Vec2::y(),
        -Vec2::y()
    ];

    for dir in direction.iter() {
        p2 = support_point_2(dir, points, [].as_slice()).unwrap();

        let p1p2 = points[p2] - points[p1];

        if !na::sqnorm(&p1p2).is_zero() {
            break;
        }
    }

    assert!(p1 != p2, "Failed to build the 2d convex hull of this point cloud.");

    // Build two facets with opposite normals.
    let mut f1 = SegmentFacet::new(p1, p2, 1, 1, points);
    let mut f2 = SegmentFacet::new(p2, p1, 0, 0, points);

    // Attribute points to each facet.
    for i in range(1, points.len()) {
        if i == p2 {
            continue;
        }
        if f1.can_be_seen_by(i, points) {
            f1.visible_points.push(i);
        }
        else if f2.can_be_seen_by(i, points) {
            f2.visible_points.push(i);
        }
        else { // The point is collinear.
            undecidable.push(i);
        }
    }

    res.push(f1);
    res.push(f2);

    res
}

fn attach_and_push_facets_2d(prev_facet:    uint,
                             next_facet:    uint,
                             point:         uint,
                             points:        &[Vec2<Scalar>],
                             segments:      &mut Vec<SegmentFacet>,
                             removed_facet: uint,
                             undecidable:   &mut Vec<uint>) {

    let new_facet1_id = segments.len();
    let new_facet2_id = new_facet1_id + 1;
    let prev_pt       = (*segments)[prev_facet].pts[1];
    let next_pt       = (*segments)[next_facet].pts[0];

    let mut new_facet1 = SegmentFacet::new(prev_pt, point, prev_facet, new_facet2_id, points);
    let mut new_facet2 = SegmentFacet::new(point, next_pt, new_facet1_id, next_facet, points);

    segments.get_mut(prev_facet).next = new_facet1_id;
    segments.get_mut(next_facet).prev = new_facet2_id;

    // Assign to each facets some of the points which can see it.
    for visible_point in (*segments)[removed_facet].visible_points.iter() {
        if *visible_point == point {
            continue;
        }

        if new_facet1.can_be_seen_by(*visible_point, points) {
            new_facet1.visible_points.push(*visible_point);
        }
        else if new_facet2.can_be_seen_by(*visible_point, points) {
            new_facet2.visible_points.push(*visible_point);
        }
        // If none of the facet can be seen from the point, it is naturally deleted.
    }

    // Try to assign collinear points to one of the new facets
    let mut i = 0;

    while i != undecidable.len() {
        if new_facet1.can_be_seen_by((*undecidable)[i], points) {
            new_facet1.visible_points.push((*undecidable)[i]);
            let _ = undecidable.swap_remove(i);
        }
        else if new_facet2.can_be_seen_by((*undecidable)[i], points) {
            new_facet2.visible_points.push((*undecidable)[i]);
            let _ = undecidable.swap_remove(i);
        }
        else {
            i = i + 1;
        }
    }

    segments.push(new_facet1);
    segments.push(new_facet2);
}

struct SegmentFacet {
    pub valid:          bool,
    pub normal:         Vec2<Scalar>,
    pub next:           uint,
    pub prev:           uint,
    pub pts:            [uint, ..2],
    pub visible_points: Vec<uint>
}

impl SegmentFacet {
    pub fn new(p1: uint, p2: uint, prev: uint, next: uint, points: &[Vec2<Scalar>]) -> SegmentFacet {
        let p1p2 = points[p2] - points[p1];

        let mut normal = Vec2::new(-p1p2.y, p1p2.x);
        if normal.normalize().is_zero() {
            fail!("Convex hull failure: a segment must not be affinely dependent.");
        }

        SegmentFacet {
            valid:          true,
            normal:         normal,
            prev:           prev,
            next:           next,
            pts:            [p1, p2],
            visible_points: Vec::new()
        }
    }

    pub fn can_be_seen_by(&self, point: uint, points: &[Vec2<Scalar>]) -> bool {
        let p0 = &points[self.pts[0]];
        let pt = &points[point];

        let _eps: Scalar = Float::epsilon();

        na::dot(&(*pt - *p0), &self.normal) > _eps * na::cast(100.0f64)
    }
}



#[cfg(test)]
mod test {
    use nalgebra::na::Vec2;
    use procedural;

    #[test]
    fn test_simple_convex_hull2d() {
        let points = [
            Vec2::new(4.723881, 3.597233),
            Vec2::new(3.333363, 3.429991),
            Vec2::new(3.137215, 2.812263)
            ];

        let chull = super::convex_hull2d(points);

        assert!(chull.coords.len() == 3);
    }

    #[test]
    fn test_ball_convex_hull() {
        // This trigerred a failure to an affinely dependent facet.
        let sphere = procedural::sphere(&0.4f32, 20, 20, true);
        let points = sphere.coords;
        let chull  = procedural::convex_hull3d(points.as_slice());

        // dummy test, we are just checking that the construction did not fail.
        assert!(chull.coords.len() == chull.coords.len());
    }
}
