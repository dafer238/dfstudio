# Changelog

All notable changes to DF Studio are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Verification
- Tests of the property pipeline against IAPWS-95 (Tables 7 and 8),
  IAPWS-IF97 (Tables 5 and 15), NIST-JANAF and exact thermodynamic
  identities in five unit systems; solver tests against closed-form
  solutions and a direct CoolProp calculation. See `docs/VERIFICATION.md`.
- `examples/ees_crosscheck.dfs`: a 198-equation model that runs unchanged in
  EES, for comparing the two programs.

### Fixed
- The solver could report convergence at a spurious root created by
  clearing a denominator (for example an LMTD at dT1 = dT2, where the
  equation is 0/0), with an infinite residual. Such points are refused, the
  block is retried on the equations as written, and a solution whose
  residual cannot be evaluated is never reported as converged.
- `Steam_IAPWS` (EES) now maps to IAPWS-95 like `Water`, as in EES; it was
  read as IAPWS-IF97. `R718` is accepted as water.
- `converttemp(...)` results were labelled and display-converted as model
  temperatures; they are plain numbers now.
- A rare crash ("double free or corruption") when several threads created
  CoolProp states at the same time, e.g. a parametric table or optimisation
  starting its workers: creating a state now has exclusive access to
  CoolProp. The states of a worker thread are released when it ends, so
  repeated studies no longer accumulate memory.

## [0.1.1] - 2026-11-03

### Distribution
- Windows installer: options, checked by default, to add a Start menu
  entry, add DF Studio to the PATH (the user PATH for a per-user install,
  the system PATH for an all-users install; removed on uninstall), and open
  `.df`/`.dfs` and `.ees` files with DF Studio on double-click. DF Studio is
  also listed under *Open with* for these file types.

## [0.1.0] - 2026-11-03

First public release.

### Equation solver
- Equations in any order, arrays, `Duplicate`, `Sum`, `Function` and
  `Procedure` blocks with `If`, `Repeat`, `While` and `For`.
- Live structural analysis while typing: determined, pending and
  over-specified variables, degrees of freedom.
- Block decomposition of the system; explicit evaluation where possible,
  Newton's method with exact derivatives, line search and a
  Levenberg–Marquardt fall-back elsewhere.
- Unit systems, unit literals, per-variable display units and dimensional
  checking.

### Properties
- CoolProp real fluids, mixtures and incompressibles; ideal gases with
  thermochemical reference states; humid air; liquid metals, molten salts
  and thermal oils.
- Temperature–enthalpy and temperature–internal-energy states.

### Studies and plots
- Parametric tables solved in parallel, goal seek, multi-variable matching,
  optimisation (Brent, Nelder–Mead, differential evolution), `$Control`
  loops.
- XY, 3D and property diagrams (T–s, P–h, h–s, P–v, T–h); export to a
  matplotlib script.

### Cycle diagram
- Flowsheet bound to the state arrays, KPIs, component reports (balances,
  pinch, UA, exergy), PDF report.
- Templates: simple Rankine, simple Brayton, vapour-compression
  refrigeration.
- Undo and redo of diagram edits.

### Editor and files
- Completion, signature help, find and replace, formatting, auto save.
- `.df` project files (single checksummed file with studies, plots,
  diagram, results and attachments); `dfs` command line; `dfpy` Python
  reader.
- Opens EES files (text and binary) to help move existing models.

### Distribution
- Windows installer (all users or current user, no administrator rights
  needed for the latter) and portable zip; universal macOS DMG; Linux
  AppImage. Example projects included.

[Unreleased]: https://github.com/dafer238/dfstudio/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/dafer238/dfstudio/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/dafer238/dfstudio/releases/tag/v0.1.0
