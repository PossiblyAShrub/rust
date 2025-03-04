//! Greatest lower bound. See [`lattice`].

use super::combine::CombineFields;
use super::lattice::{self, LatticeDir};
use super::InferCtxt;
use super::Subtype;

use crate::infer::combine::ConstEquateRelation;
use crate::traits::{ObligationCause, PredicateObligation};
use rustc_middle::ty::relate::{Relate, RelateResult, TypeRelation};
use rustc_middle::ty::{self, Ty, TyCtxt};

/// "Greatest lower bound" (common subtype)
pub struct Glb<'combine, 'infcx, 'tcx> {
    fields: &'combine mut CombineFields<'infcx, 'tcx>,
    a_is_expected: bool,
}

impl<'combine, 'infcx, 'tcx> Glb<'combine, 'infcx, 'tcx> {
    pub fn new(
        fields: &'combine mut CombineFields<'infcx, 'tcx>,
        a_is_expected: bool,
    ) -> Glb<'combine, 'infcx, 'tcx> {
        Glb { fields, a_is_expected }
    }
}

impl<'tcx> TypeRelation<'tcx> for Glb<'_, '_, 'tcx> {
    fn tag(&self) -> &'static str {
        "Glb"
    }

    fn intercrate(&self) -> bool {
        assert!(!self.fields.infcx.intercrate);
        false
    }

    fn tcx(&self) -> TyCtxt<'tcx> {
        self.fields.tcx()
    }

    fn param_env(&self) -> ty::ParamEnv<'tcx> {
        self.fields.param_env
    }

    fn a_is_expected(&self) -> bool {
        self.a_is_expected
    }

    fn mark_ambiguous(&mut self) {
        bug!("mark_ambiguous used outside of coherence");
    }

    fn relate_with_variance<T: Relate<'tcx>>(
        &mut self,
        variance: ty::Variance,
        _info: ty::VarianceDiagInfo<'tcx>,
        a: T,
        b: T,
    ) -> RelateResult<'tcx, T> {
        match variance {
            ty::Invariant => self.fields.equate(self.a_is_expected).relate(a, b),
            ty::Covariant => self.relate(a, b),
            // FIXME(#41044) -- not correct, need test
            ty::Bivariant => Ok(a),
            ty::Contravariant => self.fields.lub(self.a_is_expected).relate(a, b),
        }
    }

    fn tys(&mut self, a: Ty<'tcx>, b: Ty<'tcx>) -> RelateResult<'tcx, Ty<'tcx>> {
        lattice::super_lattice_tys(self, a, b)
    }

    fn regions(
        &mut self,
        a: ty::Region<'tcx>,
        b: ty::Region<'tcx>,
    ) -> RelateResult<'tcx, ty::Region<'tcx>> {
        debug!("{}.regions({:?}, {:?})", self.tag(), a, b);

        let origin = Subtype(Box::new(self.fields.trace.clone()));
        // GLB(&'static u8, &'a u8) == &RegionLUB('static, 'a) u8 == &'static u8
        Ok(self.fields.infcx.inner.borrow_mut().unwrap_region_constraints().lub_regions(
            self.tcx(),
            origin,
            a,
            b,
        ))
    }

    fn consts(
        &mut self,
        a: ty::Const<'tcx>,
        b: ty::Const<'tcx>,
    ) -> RelateResult<'tcx, ty::Const<'tcx>> {
        self.fields.infcx.super_combine_consts(self, a, b)
    }

    fn binders<T>(
        &mut self,
        a: ty::Binder<'tcx, T>,
        b: ty::Binder<'tcx, T>,
    ) -> RelateResult<'tcx, ty::Binder<'tcx, T>>
    where
        T: Relate<'tcx>,
    {
        // GLB of a binder and itself is just itself
        if a == b {
            return Ok(a);
        }

        debug!("binders(a={:?}, b={:?})", a, b);
        if a.skip_binder().has_escaping_bound_vars() || b.skip_binder().has_escaping_bound_vars() {
            // When higher-ranked types are involved, computing the GLB is
            // very challenging, switch to invariance. This is obviously
            // overly conservative but works ok in practice.
            self.relate_with_variance(
                ty::Variance::Invariant,
                ty::VarianceDiagInfo::default(),
                a,
                b,
            )?;
            Ok(a)
        } else {
            Ok(ty::Binder::dummy(self.relate(a.skip_binder(), b.skip_binder())?))
        }
    }
}

impl<'combine, 'infcx, 'tcx> LatticeDir<'infcx, 'tcx> for Glb<'combine, 'infcx, 'tcx> {
    fn infcx(&self) -> &'infcx InferCtxt<'tcx> {
        self.fields.infcx
    }

    fn cause(&self) -> &ObligationCause<'tcx> {
        &self.fields.trace.cause
    }

    fn add_obligations(&mut self, obligations: Vec<PredicateObligation<'tcx>>) {
        self.fields.obligations.extend(obligations)
    }

    fn relate_bound(&mut self, v: Ty<'tcx>, a: Ty<'tcx>, b: Ty<'tcx>) -> RelateResult<'tcx, ()> {
        let mut sub = self.fields.sub(self.a_is_expected);
        sub.relate(v, a)?;
        sub.relate(v, b)?;
        Ok(())
    }

    fn define_opaque_types(&self) -> bool {
        self.fields.define_opaque_types
    }
}

impl<'tcx> ConstEquateRelation<'tcx> for Glb<'_, '_, 'tcx> {
    fn const_equate_obligation(&mut self, a: ty::Const<'tcx>, b: ty::Const<'tcx>) {
        self.fields.add_const_equate_obligation(self.a_is_expected, a, b);
    }
}
