//! Dimensional inference ("unit checking").
//!
//! Dimensions are propagated through the equations until a fixed point:
//! both sides of an equation and all additive terms share a dimension,
//! products/quotients combine exponents, property calls know the dimension
//! of their result and inputs. Plain numbers without a unit are treated as
//! wildcards. The result is used to pick default display units
//! and to offer only compatible units in the variable table.

use crate::compile::Model;
use crate::tape::{Op, Tape, F1, F2};
use crate::units::{parse_unit, Dim};

#[derive(Clone, Copy, PartialEq, Debug)]
enum D {
    Unknown,
    Wild,
    K(Dim),
}

fn half(d: Dim) -> Option<Dim> {
    let mut r = [0i8; 7];
    for i in 0..7 {
        if d.0[i] % 2 != 0 {
            return None;
        }
        r[i] = d.0[i] / 2;
    }
    Some(Dim(r))
}

pub fn infer(m: &Model) -> (Vec<Option<Dim>>, Vec<u32>) {
    let n = m.vars.len();
    let mut dims: Vec<Option<Dim>> = vec![None; n];
    for (i, v) in m.vars.iter().enumerate() {
        let decl = m.display_units.get(&(i as u32)).or(v.unit_hint.as_ref());
        if let Some(u) = decl.and_then(|s| parse_unit(s).ok()) {
            dims[i] = Some(u.dim);
        }
    }
    let mut conflicts = Vec::new();
    for pass in 0..30 {
        let mut changed = false;
        let last = pass == 29;
        for (ei, eq) in m.eqs.iter().enumerate() {
            let t = &eq.tape;
            let mut nd = vec![D::Unknown; t.ops.len()];
            let mut conflict = false;
            // bottom-up
            for i in 0..t.ops.len() {
                nd[i] = match &t.ops[i] {
                    Op::Const(_) => match eq.const_dims.iter().find(|(k, _)| *k as usize == i) {
                        Some((_, d)) => D::K(*d),
                        None => D::Wild,
                    },
                    Op::Var(v) => dims[*v as usize].map(D::K).unwrap_or(D::Unknown),
                    Op::F1(f, a) => {
                        let a = nd[*a as usize];
                        match f {
                            F1::Neg | F1::Abs | F1::Floor | F1::Ceil | F1::Round | F1::Trunc => a,
                            F1::Sqrt => match a {
                                D::K(d) => half(d).map(D::K).unwrap_or(D::Unknown),
                                o => o,
                            },
                            F1::Sign | F1::Step | F1::Not => D::Wild,
                            F1::Opaque => D::Unknown,
                            _ => D::K(Dim::NONE),
                        }
                    }
                    Op::F2(f, a, b) => {
                        let (x, y) = (nd[*a as usize], nd[*b as usize]);
                        match f {
                            F2::Add | F2::Sub | F2::Min | F2::Max | F2::Mod => match (x, y) {
                                (D::K(p), D::K(q)) => {
                                    if p != q {
                                        conflict = true;
                                    }
                                    D::K(p)
                                }
                                (D::K(p), _) | (_, D::K(p)) => D::K(p),
                                (D::Wild, D::Wild) => D::Wild,
                                _ => D::Unknown,
                            },
                            F2::Mul => match (x, y) {
                                (D::K(p), D::K(q)) => D::K(p.mul(q)),
                                (D::K(p), D::Wild) | (D::Wild, D::K(p)) => D::K(p),
                                (D::Wild, D::Wild) => D::Wild,
                                _ => D::Unknown,
                            },
                            F2::Div => match (x, y) {
                                (D::K(p), D::K(q)) => D::K(p.div(q)),
                                (D::K(p), D::Wild) => D::K(p),
                                (D::Wild, D::K(q)) => D::K(Dim::NONE.div(q)),
                                (D::Wild, D::Wild) => D::Wild,
                                _ => D::Unknown,
                            },
                            F2::Pow => match (x, &t.ops[*b as usize]) {
                                (D::K(p), Op::Const(e)) if e.fract() == 0.0 => D::K(p.powi(*e as i32)),
                                (D::K(p), _) if p.is_none() => D::K(p),
                                (D::Wild, _) => D::Wild,
                                _ => D::Unknown,
                            },
                            F2::Atan2 => D::K(Dim::NONE),
                            _ => D::Wild,
                        }
                    }
                    Op::Select(_, a, b) => match (nd[*a as usize], nd[*b as usize]) {
                        (D::K(p), _) | (_, D::K(p)) => D::K(p),
                        (D::Wild, D::Wild) => D::Wild,
                        _ => D::Unknown,
                    },
                    Op::Ext(f, _, _) => match f.dims() {
                        Some((o, _)) => D::K(o),
                        None => D::Unknown,
                    },
                };
            }
            // top-down
            let mut target: Vec<Option<Dim>> = vec![None; t.ops.len()];
            let side = match (nd[eq.lhs as usize], nd[eq.rhs as usize]) {
                (D::K(p), D::K(q)) => {
                    if p != q {
                        conflict = true;
                    }
                    Some(p)
                }
                (D::K(p), _) | (_, D::K(p)) => Some(p),
                _ => None,
            };
            target[eq.lhs as usize] = side;
            target[eq.rhs as usize] = side;
            for i in (0..t.ops.len()).rev() {
                // property inputs carry their own dimension
                if let Op::Ext(f, st, l) = &t.ops[i] {
                    if let Some((_, ins)) = f.dims() {
                        for (k, &a) in t.args[*st as usize..(*st + *l) as usize].iter().enumerate() {
                            if let Some(d) = ins.get(k) {
                                if target[a as usize].is_none() {
                                    target[a as usize] = Some(*d);
                                }
                            }
                        }
                    }
                }
                let Some(tg) = target[i] else { continue };
                let known = |j: u32| match nd[j as usize] {
                    D::K(d) => Some(d),
                    D::Wild => Some(Dim::NONE),
                    D::Unknown => None,
                };
                let set = |target: &mut Vec<Option<Dim>>, j: u32, d: Dim| {
                    if target[j as usize].is_none() {
                        target[j as usize] = Some(d);
                    }
                };
                match &t.ops[i] {
                    Op::Var(v) => {
                        if dims[*v as usize].is_none() && !last {
                            dims[*v as usize] = Some(tg);
                            changed = true;
                        }
                    }
                    Op::F1(F1::Neg | F1::Abs | F1::Floor | F1::Ceil | F1::Round | F1::Trunc, a) => set(&mut target, *a, tg),
                    Op::F2(F2::Add | F2::Sub | F2::Min | F2::Max, a, b) => {
                        set(&mut target, *a, tg);
                        set(&mut target, *b, tg);
                    }
                    Op::Select(_, a, b) => {
                        set(&mut target, *a, tg);
                        set(&mut target, *b, tg);
                    }
                    Op::F2(F2::Mul, a, b) => {
                        if let Some(da) = known(*a) {
                            set(&mut target, *b, tg.div(da));
                        } else if let Some(db) = known(*b) {
                            set(&mut target, *a, tg.div(db));
                        }
                    }
                    Op::F2(F2::Div, a, b) => {
                        if let Some(db) = known(*b) {
                            set(&mut target, *a, tg.mul(db));
                        } else if let Some(da) = known(*a) {
                            set(&mut target, *b, da.div(tg));
                        }
                    }
                    _ => {}
                }
            }
            if last && conflict {
                conflicts.push(ei as u32);
            }
        }
        if !changed && !last {
            // one more pass to collect conflicts
            for (ei, eq) in m.eqs.iter().enumerate() {
                if has_conflict(eq.tape.clone(), eq, &dims) {
                    conflicts.push(ei as u32);
                }
            }
            break;
        }
    }
    (dims, conflicts)
}

fn has_conflict(t: Tape, eq: &crate::compile::Equation, dims: &[Option<Dim>]) -> bool {
    let mut nd: Vec<Option<Dim>> = vec![None; t.ops.len()];
    let mut bad = false;
    for i in 0..t.ops.len() {
        nd[i] = match &t.ops[i] {
            Op::Const(_) => eq.const_dims.iter().find(|(k, _)| *k as usize == i).map(|x| x.1),
            Op::Var(v) => dims[*v as usize],
            Op::F1(F1::Neg | F1::Abs, a) => nd[*a as usize],
            Op::F2(F2::Add | F2::Sub, a, b) => match (nd[*a as usize], nd[*b as usize]) {
                (Some(p), Some(q)) => {
                    if p != q {
                        bad = true;
                    }
                    Some(p)
                }
                (p, q) => p.or(q),
            },
            Op::F2(F2::Mul, a, b) => match (nd[*a as usize], nd[*b as usize]) {
                (Some(p), Some(q)) => Some(p.mul(q)),
                _ => None,
            },
            Op::F2(F2::Div, a, b) => match (nd[*a as usize], nd[*b as usize]) {
                (Some(p), Some(q)) => Some(p.div(q)),
                _ => None,
            },
            Op::Ext(f, _, _) => f.dims().map(|d| d.0),
            _ => None,
        };
    }
    if let (Some(p), Some(q)) = (nd[eq.lhs as usize], nd[eq.rhs as usize]) {
        if p != q {
            bad = true;
        }
    }
    bad
}
