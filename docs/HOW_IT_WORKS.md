# How DF Studio solves a model

This document describes what happens between pressing **Solve** and seeing
the results: how the equations are read, how the program decides which
equation computes which variable, the numerical methods and tolerances, where
the thermophysical properties come from, and how the studies (parametric
tables, goal seek, optimisation, control loops) are built on top. Every
algorithm cites the publication it follows, and every statement points to the
source file that implements it, so a result can be traced and checked.

Contents

1. [Overview](#1-overview)
2. [From text to equations](#2-from-text-to-equations)
3. [Units and dimensions](#3-units-and-dimensions)
4. [Structural analysis](#4-structural-analysis)
5. [Numerical solution](#5-numerical-solution)
6. [Thermophysical properties](#6-thermophysical-properties)
7. [Studies](#7-studies)
8. [Diagnostics: reading a failed solve](#8-diagnostics-reading-a-failed-solve)
9. [Verification](#9-verification)
10. [Limitations](#10-limitations)
11. [References](#11-references)

## 1. Overview

A model is a set of equations written in any order, with the unknown on
either side. DF Studio does not execute the script from top to bottom.
Instead it treats it as a system of nonlinear algebraic equations
F(x) = 0, with one unknown per variable, and solves it in four stages:

```
 script ──► parse ──► compile to tapes ──► structural analysis ──► block solver ──► results
           (AST)      (one residual per     (matching, DM split,    (explicit
                       equation, units       block-triangular        evaluation or
                       converted)            order)                  Newton per block)
```

| Stage | Source file | Result |
|---|---|---|
| Lexing and parsing | `crates/dfcore/src/lexer.rs`, `parser.rs` | syntax tree, with errors recovered line by line |
| Compilation | `compile.rs`, `tape.rs`, `units.rs` | one residual tape per equation, in the model unit system |
| Structural analysis | `structure.rs` | which equation determines which variable, and in what order |
| Numerical solution | `solver.rs` | values of all variables, per-block reports, residuals |
| Properties | `props.rs`, `liquids.rs` | real-fluid, ideal-gas, humid-air and liquid properties |
| Studies | `studies.rs` | parametric tables, goal seek, optimisation, control loops |

The same pipeline runs in the desktop app, in the `dfs` command-line runner
and in the tests, so results do not depend on how the model is run.

## 2. From text to equations

### 2.1 Parsing

The lexer (`lexer.rs`) turns the text into tokens: names (case-insensitive),
numbers, unit literals (`25 [C]`), operators and comments (`"..."`,
`{...}`, `// ...`). A bracket after a number is a unit; a bracket after a
name is an array index (`T[3]`). Scripts written with a decimal comma and
`;` as list separator (`f(a; b)`, `1,5`) are recognised and read as
`f(a, b)` and `1.5` (`locale.rs`).

The parser (`parser.rs`) is a recursive-descent parser with error recovery:
a line that cannot be parsed is reported and skipped, and the rest of the
model is still analysed. This is what lets the editor colour variables while
you type an incomplete model.

### 2.2 Compilation to tapes

The compiler (`compile.rs`) turns each equation `lhs = rhs` into a
**residual** r = lhs − rhs, and records it as a *tape*: a flat list of
operations in single-assignment form, where every operation refers only to
earlier ones (`tape.rs`). On the way:

* `Duplicate i = a, b ... End` blocks and `Sum(...)`/`Product(...)` are
  unrolled; array elements such as `T[3]` become ordinary scalar unknowns.
  Constants used as array bounds are evaluated first.
* Unit literals are converted to the model unit system (§3).
* Property calls (`enthalpy(Water, P=P[1], T=T[1])`), user `Function`s and
  `Procedure`s become *external* operations on the tape, which know how to
  return a value and its partial derivatives (§5.3).
* A second form of the residual, the **Newton form**, is built with the
  top-level denominators cleared: `a/b = c/d` becomes `a·d = c·b` when the
  denominators depend on unknowns. For an efficiency definition such as
  `eta_p = (h_s[2] - h[1]) / (h[2] - h[1])`, the residual in Newton form,
  `eta_p·(h[2] − h[1]) − (h_s[2] − h[1])`, is *linear* in h[2], and the
  pole at h[2] = h[1] disappears. The solver iterates on the Newton form and
  checks convergence on the original form.
* The top-level **additive terms** of each side are recorded. They define
  the scale of the equation used in the convergence test (§5.4).

Evaluating a tape is one pass over a vector without memory allocation, and
the reverse pass that gives the exact gradient (§5.3) costs a small constant
times the forward pass, whatever the number of variables.

### 2.3 Guesses and bounds

Every unknown starts from a guess: `$Guess T = 300` or the guess column of
the Variables panel; otherwise the last solution (*warm start*) when there
is one; otherwise 1. `$Bounds x = lo, hi` limits a variable: every Newton
step is clamped to the bounds (§5.2).

## 3. Units and dimensions

Equations are evaluated in the **model unit system** (`units.rs`), for
example Engineering: °C, bar, kJ/kg, kW. A literal with a unit
(`10 [bar]`) is converted into it once, at compile time. Property functions
convert their inputs to SI before calling CoolProp and convert the result
back, at the call boundary. Every conversion is affine,
SI = a·value + b, so it costs one multiply-add, and the derivative of the
conversion (a) is applied to the property gradients.

Each variable may also have a **display unit**, applied only when a value is
shown. Display units never change the numbers the solver works with.

**Dimensional inference** (`dims.rs`) propagates dimensions through the
equations until nothing changes: both sides of an equation and all additive
terms must share a dimension, products and quotients combine exponents, and
property calls know the dimensions of their inputs and result. Numbers
without a unit are wildcards. The result picks default display units, offers
only compatible units in the Variables panel, and flags equations whose terms
have different dimensions ("unit check" entries in the Problems panel). The
unit check is a warning: it never changes the solution.

Temperature differences need care: `[K]` and `[C]` are absolute
temperatures; use `[dK]` or `[deltaC]` for a difference (see the user guide,
§5).

## 4. Structural analysis

Before any number is computed, DF Studio analyses the *structure* of the
system: which variable appears in which equation (the incidence pattern).
This is done in `structure.rs`, and is fast enough (linear or close to
linear in the model size) to run on every keystroke.

### 4.1 Matching equations to unknowns

The model is a bipartite graph: equations on one side, unknowns on the
other, and an edge where a variable appears in an equation. A **maximum
matching** pairs as many equations as possible with distinct unknowns. The
implementation starts with a greedy matching (equations with the fewest
unknowns first, which matches most assignments `x = ...` immediately) and
completes it by depth-first search for augmenting paths (Berge, 1957). This
is the simple O(n·m) augmenting-path method; for engineering models, where
most equations are matched greedily, it is effectively linear.

### 4.2 Over-, well- and under-determined parts

From the matching, the coarse **Dulmage–Mendelsohn decomposition**
(Dulmage and Mendelsohn, 1958; Pothen and Fan, 1990) splits the system into
three parts:

* **under-determined**: variables reachable from an unmatched variable by
  alternating paths. These cannot be computed yet: more equations or fixed
  values are needed. They are shown in **orange**, and their number gives the
  **degrees of freedom**.
* **over-determined**: equations reachable from an unmatched equation. Some
  of these equations are redundant (if consistent) or contradictory. Shown in
  **red**.
* **well-determined**: everything else, which can be solved. Shown in
  **green**.

This decomposition is unique: it does not depend on which maximum matching
was found. That is why the editor can say *which* variables are pending,
not only *how many* equations are missing.

### 4.3 Block-triangular order

The well-determined part is put in **block lower-triangular** form with
Tarjan's strongly connected components algorithm (Tarjan, 1972; Duff and
Reid, 1978). Each strongly connected component is a **block**: a set of
equations that must be solved simultaneously. The blocks are ordered so that
each one only needs variables computed by earlier blocks.

A typical cycle model breaks into many blocks of size 1. The Rankine example
(`examples/rankine.dfs`, 25 equations) is 25 blocks of size 1: no equation
has to be solved together with another one. A recuperated cycle where an
energy balance couples two unknown enthalpies gives a block of size 2, and
so on. Solving a sequence of small blocks is much more robust and faster
than one Newton iteration on the full system, and a failure is localised to
one block (§8).

The dependency view in the app (*Equations > Dependencies*) draws exactly
this block structure.

## 5. Numerical solution

`solver.rs` walks the blocks in order. Default options
(`SolveOptions::default`): at most 250 iterations per block, relative
tolerance 1e-10, default guess 1.

### 5.1 Explicit blocks

A block of size 1 whose equation has its unknown alone on one side, and
nowhere on the other (`x = f(known variables)`), is **evaluated directly**:
no iteration, no guess needed. Most of a typical model is solved this way.

### 5.2 Newton blocks

Every other block (an implicit size-1 block such as
`eta_t = (h[3] - h[4]) / (h[3] - h_s[4])` for h[4], or a coupled block of
size n) is solved by a damped Newton–Raphson method on the scaled residuals
of the Newton form (Dennis and Schnabel, 1996, ch. 6; Nocedal and Wright,
2006, ch. 11):

1. Evaluate the residual vector r and the exact Jacobian J of the block
   (§5.3).
2. Solve J·Δx = −r by LU factorisation with partial pivoting. The matrix is
   treated as singular when a pivot is below 1e-14 times the largest one.
3. If J is singular, take a **Levenberg–Marquardt** step instead (Moré,
   1978): (JᵀJ + λ·diag(JᵀJ) + λI)·Δx = −Jᵀr, with λ starting at 1e-6 and
   multiplied by 10 on each failure (up to 1e6). After a successful step, λ
   is divided by 10, and below 1e-9 the method returns to pure Newton steps.
4. **Line search** (backtracking, Armijo condition): try x + t·Δx with
   t = 1, ½, ¼, … (up to 40 halvings), clamped to the variable bounds,
   and accept the first t for which the squared residual norm f = ‖r‖²
   decreases sufficiently: f(t) ≤ f + 10⁻⁴·t·f′ (f′ = −2f along a Newton
   step) or f(t) < f·(1 − 10⁻⁴·t). Trial points where a property call
   fails (outside the fluid's range) are rejected like any other bad point.
   If no t is accepted, the step is discarded and the damping λ increased.
5. Stop when the largest scaled residual of the *original* equations
   is ≤ 1e-10 (converged); or when the iterations run out, the line search
   fails at λ = 1e6, or the iterates stagnate (relative step < 1e-15). A
   stagnated block whose residual is below √1e-10 = 1e-5 is accepted and
   reported as "converged to reduced tolerance".

Unknowns of a block that did not converge keep their last iterate, and the
following blocks are still solved with those values, so that as much of the
model as possible is evaluated; the solution is marked as not converged and
the first failing block is reported.

### 5.3 Exact derivatives

Jacobians are not approximated by finite differences of the whole equation.
Each tape is differentiated by **reverse-mode automatic differentiation**
(Griewank and Walther, 2008): one forward pass stores the intermediate
values, one backward pass accumulates ∂r/∂xᵢ for every variable of the
equation. The result is exact to rounding error.

External operations supply their own partial derivatives:

* **CoolProp properties**: the analytic partial derivatives of the
  Helmholtz-energy equation of state, `AbstractState::first_partial_deriv`
  (∂out/∂in₁ at constant in₂), converted to the model units. Where CoolProp
  does not provide them (for example some two-phase states), central finite
  differences of the property call are used.
* **User `Function`s and `Procedure`s**, liquid-metal and molten-salt
  correlations, humid air: central finite differences with a relative step
  of 10⁻⁶ (Dennis and Schnabel, 1996, §5.6); a one-sided difference is used
  when one side lies outside the valid range.

### 5.4 Scaling and the convergence test

Thermodynamic models mix numbers of very different sizes (pressures in Pa,
efficiencies of order 1, powers in MW). An absolute tolerance would be too
strict for one equation and meaningless for another. Each residual is
therefore divided by the **largest absolute additive term** of its
equation:

    scaled residual = (lhs − rhs) / max |term|        (1 if all terms are ~0)

For an energy balance `0 = m₁h₁ + m₂h₂ − m₃h₃`, the scale is the largest
mhᵢ product, so 1e-10 means "balanced to ten significant digits relative to
the largest flow of energy", whatever its unit. Inside the Newton iteration
the scales are frozen and refreshed every 8 iterations, so that a poor start
point does not set them permanently.

### 5.5 After the solve

When all blocks are done, the residuals of all equations whose variables are
known are evaluated, and the largest one is reported in the status bar.
The message then says, in this order:

* the first block that did not converge, with its variables and the reason;
* otherwise the degrees of freedom left (under-determined variables);
* otherwise the redundant equations, checked against √tolerance: "redundant
  (consistent)" when they hold, "not satisfied" when they contradict the
  rest of the model.

## 6. Thermophysical properties

### 6.1 Real fluids: CoolProp

Pure fluids, pseudo-pure fluids, mixtures and incompressible fluids come
from **CoolProp** (Bell et al., 2014), loaded as a shared library shipped
with DF Studio (`props.rs`). Pure-fluid properties are computed from the
reference Helmholtz-energy equations of state that CoolProp implements (for
example IAPWS-95 for water, Span–Wagner for CO₂); the reference for each
fluid is listed in the CoolProp documentation.

DF Studio uses CoolProp's low-level `AbstractState` interface rather than
`PropsSI`: one state object per thread and fluid is created once and
reused, which avoids parsing the fluid string and rebuilding the equation of
state on every call. A property call with inputs (in₁, in₂) maps onto a
CoolProp *flash* (P–T, P–h, P–s, h–s, T–x, ...). Fluid names are CoolProp
names or aliases (`Water`, `CO2`, `R134a`, `IF97::Water`,
`'CO2[0.9]&Argon[0.1]'`).

**Temperature with enthalpy or internal energy.** CoolProp cannot flash
from (T, h) or (T, u). DF Studio solves these by a one-dimensional search in
pressure along the isotherm, on three branches: vapour (from 1 Pa to the
saturation pressure), two-phase (the quality follows from the saturated
values) and compressed liquid (from the saturation pressure to the upper
limit of the equation of state); above the critical temperature, a single
branch. In a liquid, h depends very weakly on P, so (T, h) can be matched
both by a compressed liquid and by a wet state of tiny quality; the liquid
root is taken when its pressure is below the critical pressure (a pump
outlet or condensate), otherwise the wet state. Give P instead of T when it
is known.

**Implicit inversions.** Any other combination can also be solved
implicitly: writing `h = enthalpy(Water, P=P, T=T)` with h and P known makes
T an unknown of a Newton block, which converges in a few iterations using
the analytic derivative ∂h/∂T at constant P.

Quality is returned as −1 in single-phase states (CoolProp convention).

### 6.2 Ideal gases

A property call with a single state input (`enthalpy(CO2, T=T)`) uses the
ideal-gas model of the fluid: CoolProp's equation of state evaluated at a
pressure low enough (1 Pa) for the residual part to vanish, so cp⁰(T) is
the fluid's own ideal-gas heat capacity; u = h − RT, cv = cp − R.
For CO₂, H₂O, N₂, O₂, CO, H₂, Ar, He, CH₄, NH₃ and SO₂ the thermochemical
reference state is used: h at 298.15 K equals the standard enthalpy of
formation and s at 298.15 K and 1 atm the absolute entropy, both from the
NIST-JANAF tables (Chase, 1998). Enthalpies of reacting mixtures (combustion)
are therefore consistent. Other fluids keep the CoolProp reference.

### 6.3 Humid air

Psychrometric functions (`enthalpy(AirH2O, T=, P=, w=)`, `humrat`,
`relhum`, `wetbulb`, `dewpoint`, ...) call CoolProp's `HAPropsSI`, which
implements the ASHRAE RP-1485 real-gas model (Herrmann, Kretzschmar and
Gatley, 2009). Specific quantities are per kilogram of dry air.

### 6.4 Liquid metals, molten salts and thermal oils

Fluids that CoolProp does not provide are modelled as incompressible liquids
with temperature-dependent properties (`liquids.rs`): lead and lead–bismuth
eutectic from the OECD/NEA handbook (2015), solar salt (60 % NaNO₃,
40 % KNO₃) from Zavoico (2001), plus sodium, NaK and heat-transfer oils
(see [MIGRATING_FROM_EES.md](MIGRATING_FROM_EES.md) §4.6 for the full list
and sources). With cp = cp(T),

    h(T, P) = ∫ cp dT + (P − P_ref)/ρ(T),    s(T) = ∫ cp/T dT,

with h = s = 0 at the melting point and P_ref = 101 325 Pa, the convention
of CoolProp's incompressible backend.

## 7. Studies

All studies (`studies.rs`) work the same way: they **fix** some variables
(the inputs chosen by the study) and **deactivate** the equations that set
those variables to constants in the script (for example `P[2] = 150`). The
structural plan of §4 is computed once for that set of fixed variables and
reused, so each evaluation of a study is exactly one block-wise solve. A
study whose inputs would leave the system over- or under-specified is
refused with a message naming the variables.

### 7.1 Parametric tables

Each row is a set of input values. Rows are split into contiguous chunks
that are solved **in parallel** (one thread per core, `rayon`); inside a
chunk, each row starts from the solution of the previous row (continuation),
which makes sweeps robust and fast.

### 7.2 Goal seek and parameter matching

*Vary X to make Y = value* is first tried **simultaneously**: Y is fixed,
X becomes an unknown, and the structural analysis and Newton blocks solve
everything at once. If that system is not square or does not converge, an
outer loop varies X with a safeguarded secant method (step limited to four
times the previous one), solving the model at each step.

Several conditions with as many varied variables (including
`Y1 = Y2`: vary X until two variables are equal) are solved simultaneously
first, then, if needed, by an outer quasi-Newton loop: finite-difference
Jacobian, **Broyden** rank-one updates (Broyden, 1965; Dennis and Schnabel,
1996, §8.1) and backtracking on the max-norm.

### 7.3 Optimisation

Minimise or maximise any variable over bounded decision variables. Points
where the model does not converge are given an infinite objective, so the
optimiser moves away from them.

* **Brent** (one variable): golden-section search with parabolic
  interpolation (Brent, 1973, ch. 5).
* **Nelder–Mead** (several variables, local): the simplex method (Nelder and
  Mead, 1965) with the bounds enforced by projection, started from the
  values in the script.
* **Differential evolution** (several variables, global): DE/rand/1/bin
  (Storn and Price, 1997) with F = 0.7, CR = 0.9 and a population of
  10 × (number of variables), between 16 and 60, evaluated in parallel. The
  random generator is seeded deterministically, so a run is reproducible.

### 7.4 Control loops

`$Control vary X to make Y = value` in the script applies a goal seek on
every solve. Several controls are solved together with the method of §7.2.

## 8. Diagnostics: reading a failed solve

Because the model is solved block by block, a failure is always localised:

* **"block k (n eq.) did not converge for a, b: ..."** names the block, its
  unknowns and the reason ("line search failed", "no convergence after 250
  iterations", "cannot evaluate at the initial guess: ..." with the property
  error). Everything before that block is solved; look at its equations in
  the dependency view and give better guesses or bounds for its variables.
* **Degrees of freedom left**: the orange variables need more equations or
  fixed values.
* **Redundant equations not satisfied**: the red equations contradict the
  rest of the model, typically a variable specified twice.
* The **largest residual** in the status bar is the largest scaled residual
  over all evaluated equations (§5.4).

## 9. Verification

`cargo test --workspace` runs the test suite on Windows, macOS and Linux on
every change (`.github/workflows/ci.yml`). Among others:

| Test | Checks |
|---|---|
| `tape::grad_matches_fd` | reverse-mode derivatives agree with finite differences |
| `structure` tests | matching, over/under-determined split and block order on small systems |
| `solve::nonlinear_block` | a coupled nonlinear block converges with Newton |
| `solve::rankine_cycle_coolprop` | saturated liquid at 0.1 bar gives h = 191.81 kJ/kg (IAPWS-95), and the cycle efficiency is in the expected range |
| `solve::implicit_property_inversion` | T found implicitly from h = enthalpy(P, T) equals temperature(P, h) to 1e-6 |
| `solve::temperature_and_enthalpy_fix_a_state` | the (T, h) search of §6.1 returns the liquid state |
| `solve::ideal_gas_from_one_input` | ideal-gas enthalpies and entropies with the JANAF reference |
| `solve::humid_air_liquids_and_latent_heat` | psychrometrics, liquid-metal correlations, latent heat |
| `studies::*` | parametric rows in parallel, goal seek, multi-target matching, optimisation, control loops |
| `templates::templates_solve_and_label_every_stream` | the bundled cycle templates solve and every stream shows its state |
| `dffile` tests | project files round-trip and damaged sections are detected |

The examples can be checked independently: `dfs run examples/rankine.dfs`
prints every variable with its unit, which can be compared with steam
tables or another solver.

## 10. Limitations

* Newton's method converges locally. A block started far from its solution
  can fail or, for equations with several roots, converge to a different
  root. Give guesses (`$Guess`) and bounds (`$Bounds`) for the unknowns of
  implicit blocks; the warm start from the last solution usually takes care
  of this once a model has been solved.
* The convergence test is on the scaled residual (§5.4), not on the error of
  the variables. A badly conditioned block (nearly dependent equations) can
  satisfy the test while its variables are poorly determined; the
  Levenberg–Marquardt fall-back makes such blocks solve rather than fail, so
  check the results of blocks reported with large iteration counts.
* Property accuracy is that of the underlying equation of state and its
  validity range. Near the critical point, and for mixtures, CoolProp flashes
  can fail or be slow.
* Optimisation results are local for Brent and Nelder–Mead. Differential
  evolution explores the whole box but is not guaranteed to find the global
  optimum within the evaluation budget.

## 11. References

* Bell, I.H., Wronski, J., Quoilin, S., Lemort, V. (2014). Pure and pseudo-pure fluid thermophysical property evaluation and the open-source thermophysical property library CoolProp. *Industrial & Engineering Chemistry Research* 53(6), 2498–2508. doi:10.1021/ie4033999
* Berge, C. (1957). Two theorems in graph theory. *Proceedings of the National Academy of Sciences* 43(9), 842–844. doi:10.1073/pnas.43.9.842
* Brent, R.P. (1973). *Algorithms for Minimization without Derivatives*. Prentice-Hall (ch. 4, root finding; ch. 5, minimisation).
* Broyden, C.G. (1965). A class of methods for solving nonlinear simultaneous equations. *Mathematics of Computation* 19(92), 577–593. doi:10.1090/S0025-5718-1965-0198670-6
* Chase, M.W. (1998). NIST-JANAF Thermochemical Tables, 4th ed. *Journal of Physical and Chemical Reference Data*, Monograph 9.
* Dennis, J.E., Schnabel, R.B. (1996). *Numerical Methods for Unconstrained Optimization and Nonlinear Equations*. SIAM. doi:10.1137/1.9781611971200
* Duff, I.S., Reid, J.K. (1978). An implementation of Tarjan's algorithm for the block triangularization of a matrix. *ACM Transactions on Mathematical Software* 4(2), 137–147. doi:10.1145/355780.355785
* Dulmage, A.L., Mendelsohn, N.S. (1958). Coverings of bipartite graphs. *Canadian Journal of Mathematics* 10, 517–534. doi:10.4153/CJM-1958-052-0
* Griewank, A., Walther, A. (2008). *Evaluating Derivatives: Principles and Techniques of Algorithmic Differentiation*, 2nd ed. SIAM. doi:10.1137/1.9780898717761
* Herrmann, S., Kretzschmar, H.-J., Gatley, D.P. (2009). Thermodynamic properties of real moist air, dry air, steam, water, and ice (ASHRAE RP-1485). *HVAC&R Research* 15(5), 961–986. doi:10.1080/10789669.2009.10390874
* Moré, J.J. (1978). The Levenberg–Marquardt algorithm: implementation and theory. In *Numerical Analysis*, Lecture Notes in Mathematics 630, 105–116. Springer. doi:10.1007/BFb0067700
* Nelder, J.A., Mead, R. (1965). A simplex method for function minimization. *The Computer Journal* 7(4), 308–313. doi:10.1093/comjnl/7.4.308
* Nocedal, J., Wright, S.J. (2006). *Numerical Optimization*, 2nd ed. Springer. doi:10.1007/978-0-387-40065-5
* OECD/NEA (2015). *Handbook on Lead-bismuth Eutectic Alloy and Lead Properties, Materials Compatibility, Thermal-hydraulics and Technologies*. NEA No. 7268.
* Pothen, A., Fan, C.-J. (1990). Computing the block triangular form of a sparse matrix. *ACM Transactions on Mathematical Software* 16(4), 303–324. doi:10.1145/98267.98287
* Storn, R., Price, K. (1997). Differential evolution: a simple and efficient heuristic for global optimization over continuous spaces. *Journal of Global Optimization* 11, 341–359. doi:10.1023/A:1008202821328
* Tarjan, R.E. (1972). Depth-first search and linear graph algorithms. *SIAM Journal on Computing* 1(2), 146–160. doi:10.1137/0201010
* Zavoico, A.B. (2001). *Solar Power Tower Design Basis Document*. SAND2001-2100, Sandia National Laboratories.
