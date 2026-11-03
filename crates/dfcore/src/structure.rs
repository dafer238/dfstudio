//! Structural analysis of the equation system.
//!
//! 1. A maximum bipartite matching between equations and unknowns: a
//!    greedy start (equations with fewest unknowns first) completed by
//!    depth-first augmenting-path search (Berge, PNAS 43(9), 1957; the
//!    simple O(n·m) method, not the phased Hopcroft–Karp variant).
//! 2. The coarse Dulmage–Mendelsohn decomposition (Dulmage & Mendelsohn,
//!    Can. J. Math. 10, 1958; Pothen & Fan, ACM TOMS 16(4), 1990) splits the
//!    system into an over-determined, a well-determined and an
//!    under-determined part. This gives the instant "which variables can be
//!    solved with the current equations" feedback of the editor.
//! 3. Tarjan's strongly connected components (SIAM J. Comput. 1(2), 1972)
//!    on the matched graph produce the block lower-triangular (BLT) order
//!    (Duff & Reid, ACM TOMS 4(2), 1978): the system is solved as a
//!    sequence of small blocks instead of one large Newton problem.

use crate::tape::VarId;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum VarStatus {
    /// Value imposed from outside (parametric table, goal-seek, ...).
    Fixed,
    /// Determined by the well-posed part of the system.
    Determined,
    /// Determined, but by an over-specified subsystem (some equations are
    /// redundant or contradictory).
    OverDetermined,
    /// Not determined yet: more equations (or fixed values) are needed.
    UnderDetermined,
    /// Does not appear in any equation.
    Unused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EqStatus {
    Determined,
    /// Matched equation of an over-determined part.
    OverDetermined,
    /// Unmatched equation: redundant (if satisfied) or contradictory.
    Redundant,
    UnderDetermined,
    /// Disabled (replaced by a fixed value in a study).
    Inactive,
}

#[derive(Debug, Clone, Serialize)]
pub struct Block {
    pub eqs: Vec<u32>,
    pub vars: Vec<VarId>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Structure {
    pub n_vars: usize,
    pub n_eqs: usize,
    pub var_status: Vec<VarStatus>,
    pub eq_status: Vec<EqStatus>,
    pub eq_match: Vec<Option<VarId>>,
    pub var_match: Vec<Option<u32>>,
    /// Blocks of the over- and well-determined parts in solve order.
    pub blocks: Vec<Block>,
    /// Block index of each determined variable.
    pub var_block: Vec<Option<u32>>,
    /// Unmatched equations (checked after solving).
    pub redundant: Vec<u32>,
    /// Degrees of freedom still open = |V_under| - |E_under|.
    pub dof: usize,
}

impl Structure {
    pub fn is_square(&self) -> bool {
        self.dof == 0 && self.redundant.is_empty()
    }
    pub fn count(&self, s: VarStatus) -> usize {
        self.var_status.iter().filter(|v| **v == s).count()
    }
}

/// `eq_vars[e]` lists the variables of equation `e`; `fixed[v]` marks known
/// variables, which are ignored in the analysis.
pub fn analyze(n_vars: usize, eq_vars: &[&[VarId]], fixed: &[bool]) -> Structure {
    analyze_active(n_vars, eq_vars, fixed, None)
}

/// As [`analyze`], ignoring equations with `active[e] == false`.
pub fn analyze_active(n_vars: usize, eq_vars: &[&[VarId]], fixed: &[bool], active: Option<&[bool]>) -> Structure {
    let n_eqs = eq_vars.len();
    let is_active = |e: usize| active.map(|a| a[e]).unwrap_or(true);
    // adjacency restricted to unknowns
    let adj: Vec<Vec<VarId>> =
        eq_vars.iter().enumerate().map(|(e, vs)| if is_active(e) { vs.iter().copied().filter(|v| !fixed[*v as usize]).collect() } else { Vec::new() }).collect();
    let mut var_eqs: Vec<Vec<u32>> = vec![Vec::new(); n_vars];
    for (e, vs) in adj.iter().enumerate() {
        for &v in vs {
            var_eqs[v as usize].push(e as u32);
        }
    }

    // ---------------------------------------------------------- matching
    let mut eq_match: Vec<Option<VarId>> = vec![None; n_eqs];
    let mut var_match: Vec<Option<u32>> = vec![None; n_vars];
    // cheap greedy initialisation: prefer equations with a single unknown
    let mut order: Vec<usize> = (0..n_eqs).collect();
    order.sort_by_key(|&e| adj[e].len());
    for &e in &order {
        if let Some(&v) = adj[e].iter().find(|v| var_match[**v as usize].is_none()) {
            eq_match[e] = Some(v);
            var_match[v as usize] = Some(e as u32);
        }
    }
    // augmenting paths (iterative DFS with visit stamps)
    let mut stamp = vec![0u32; n_vars];
    let mut cur = 0u32;
    for &e0 in &order {
        if eq_match[e0].is_some() || adj[e0].is_empty() {
            continue;
        }
        cur += 1;
        // stack of (eq, next edge index); path recorded by parent var
        let mut stack: Vec<(u32, usize)> = vec![(e0 as u32, 0)];
        let mut via: Vec<VarId> = Vec::new(); // var chosen at each stack level
        let mut found = false;
        while let Some(&mut (e, ref mut k)) = stack.last_mut() {
            let vs = &adj[e as usize];
            if *k >= vs.len() {
                stack.pop();
                via.pop();
                continue;
            }
            let v = vs[*k];
            *k += 1;
            if stamp[v as usize] == cur {
                continue;
            }
            stamp[v as usize] = cur;
            match var_match[v as usize] {
                None => {
                    via.push(v);
                    found = true;
                    break;
                }
                Some(e2) => {
                    via.push(v);
                    stack.push((e2, 0));
                }
            }
        }
        if found {
            // stack[i].0 gets via[i]
            for (i, (e, _)) in stack.iter().enumerate() {
                let v = via[i];
                eq_match[*e as usize] = Some(v);
                var_match[v as usize] = Some(*e);
            }
        }
    }

    // ---------------------------------------------------------- DM coarse
    let mut var_status = vec![VarStatus::Unused; n_vars];
    let mut eq_status = vec![EqStatus::Determined; n_eqs];
    for v in 0..n_vars {
        if fixed[v] {
            var_status[v] = VarStatus::Fixed;
        } else if !var_eqs[v].is_empty() {
            var_status[v] = VarStatus::Determined;
        }
    }
    // under: from unmatched vars, var -> eq (any edge) -> matched var
    let mut q: Vec<VarId> = (0..n_vars as VarId).filter(|&v| var_status[v as usize] == VarStatus::Determined && var_match[v as usize].is_none()).collect();
    let mut under_eq = vec![false; n_eqs];
    for &v in &q {
        var_status[v as usize] = VarStatus::UnderDetermined;
    }
    while let Some(v) = q.pop() {
        for &e in &var_eqs[v as usize] {
            if under_eq[e as usize] {
                continue;
            }
            under_eq[e as usize] = true;
            eq_status[e as usize] = EqStatus::UnderDetermined;
            if let Some(v2) = eq_match[e as usize] {
                if var_status[v2 as usize] != VarStatus::UnderDetermined {
                    var_status[v2 as usize] = VarStatus::UnderDetermined;
                    q.push(v2);
                }
            }
        }
    }
    // over: from unmatched eqs, eq -> var (any edge) -> matched eq
    let mut qe: Vec<u32> = (0..n_eqs as u32).filter(|&e| eq_match[e as usize].is_none() && is_active(e as usize)).collect();
    for e in 0..n_eqs {
        if !is_active(e) {
            eq_status[e] = EqStatus::Inactive;
        }
    }
    let redundant = qe.clone();
    let mut over_eq = vec![false; n_eqs];
    for &e in &qe {
        over_eq[e as usize] = true;
        eq_status[e as usize] = EqStatus::Redundant;
    }
    while let Some(e) = qe.pop() {
        for &v in &adj[e as usize] {
            if var_status[v as usize] == VarStatus::OverDetermined {
                continue;
            }
            var_status[v as usize] = VarStatus::OverDetermined;
            if let Some(e2) = var_match[v as usize] {
                if !over_eq[e2 as usize] {
                    over_eq[e2 as usize] = true;
                    eq_status[e2 as usize] = EqStatus::OverDetermined;
                    qe.push(e2);
                }
            }
        }
    }

    // ---------------------------------------------------------- BLT (Tarjan)
    // nodes: matched equations outside the under-determined part
    let solvable = |e: usize| eq_match[e].is_some() && !under_eq[e];
    let mut index = vec![u32::MAX; n_eqs];
    let mut low = vec![0u32; n_eqs];
    let mut on_stack = vec![false; n_eqs];
    let mut st: Vec<u32> = Vec::new();
    let mut next = 0u32;
    let mut blocks: Vec<Block> = Vec::new();
    for root in 0..n_eqs {
        if !solvable(root) || index[root] != u32::MAX {
            continue;
        }
        let mut call: Vec<(u32, usize)> = vec![(root as u32, 0)];
        index[root] = next;
        low[root] = next;
        next += 1;
        st.push(root as u32);
        on_stack[root] = true;
        while let Some(&mut (e, ref mut k)) = call.last_mut() {
            let eu = e as usize;
            // successors: equations that compute the other variables of e
            let mut pushed = false;
            while *k < adj[eu].len() {
                let v = adj[eu][*k];
                *k += 1;
                if Some(v) == eq_match[eu] {
                    continue;
                }
                let Some(w) = var_match[v as usize] else { continue };
                let wu = w as usize;
                if !solvable(wu) {
                    continue;
                }
                if index[wu] == u32::MAX {
                    index[wu] = next;
                    low[wu] = next;
                    next += 1;
                    st.push(w);
                    on_stack[wu] = true;
                    call.push((w, 0));
                    pushed = true;
                    break;
                } else if on_stack[wu] {
                    low[eu] = low[eu].min(index[wu]);
                }
            }
            if pushed {
                continue;
            }
            // finished e
            call.pop();
            if let Some(&(p, _)) = call.last() {
                low[p as usize] = low[p as usize].min(low[eu]);
            }
            if low[eu] == index[eu] {
                let mut b = Block { eqs: Vec::new(), vars: Vec::new() };
                loop {
                    let x = st.pop().unwrap();
                    on_stack[x as usize] = false;
                    b.eqs.push(x);
                    b.vars.push(eq_match[x as usize].unwrap());
                    if x == e {
                        break;
                    }
                }
                b.eqs.reverse();
                b.vars.reverse();
                blocks.push(b);
            }
        }
    }
    let mut var_block = vec![None; n_vars];
    for (i, b) in blocks.iter().enumerate() {
        for &v in &b.vars {
            var_block[v as usize] = Some(i as u32);
        }
    }
    let n_under_v = var_status.iter().filter(|s| **s == VarStatus::UnderDetermined).count();
    let n_under_e = under_eq.iter().filter(|x| **x).count();
    Structure { n_vars, n_eqs, var_status, eq_status, eq_match, var_match, blocks, var_block, redundant, dof: n_under_v.saturating_sub(n_under_e) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blt_order_and_dm() {
        // e0: x0 = 1 ; e1: x1 + x2 = x0 ; e2: x1 - x2 = 0 ; e3: x3 = x1 + x4 (x4 free)
        let eqs: Vec<Vec<VarId>> = vec![vec![0], vec![0, 1, 2], vec![1, 2], vec![1, 3, 4]];
        let refs: Vec<&[VarId]> = eqs.iter().map(|v| v.as_slice()).collect();
        let s = analyze(5, &refs, &[false; 5]);
        assert_eq!(s.blocks.len(), 2);
        assert_eq!(s.blocks[0].vars, vec![0]);
        assert_eq!(s.blocks[1].vars.len(), 2);
        assert_eq!(s.var_status[3], VarStatus::UnderDetermined);
        assert_eq!(s.var_status[4], VarStatus::UnderDetermined);
        assert_eq!(s.dof, 1);
        // fixing x4 closes the system
        let mut fx = [false; 5];
        fx[4] = true;
        let s = analyze(5, &refs, &fx);
        assert_eq!(s.dof, 0);
        assert_eq!(s.blocks.len(), 3);
        assert!(s.is_square());
    }
    #[test]
    fn overdetermined() {
        // x = 1 ; x = 2 ; y = x
        let eqs: Vec<Vec<VarId>> = vec![vec![0], vec![0], vec![1, 0]];
        let refs: Vec<&[VarId]> = eqs.iter().map(|v| v.as_slice()).collect();
        let s = analyze(2, &refs, &[false; 2]);
        assert_eq!(s.redundant.len(), 1);
        assert_eq!(s.var_status[0], VarStatus::OverDetermined);
        assert_eq!(s.var_status[1], VarStatus::Determined);
    }
}
