# Changelog

All notable changes to DF Studio are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Cycle diagram
- Pump drawn with the triangle along the flow, inlet and outlet opposite
  each other; mixer inlets opposite each other with the outlet at 90°;
  splitter outlets opposite each other at 90° to the inlet. Diagrams saved
  by 0.2 or earlier keep their streams where they were.
- Mixers and splitters have a connection point on each side.
- Connection points on the canvas: with streams unlocked, the free points
  of the selected component show as circles; drag a stream end onto one
  to move the port there.

## [0.2.0] - 2026-11-03

### Cycle diagram
- Component library in collapsible groups with monochrome icons that follow
  the theme: turbines, compressors and fans, pumps, heat exchangers, HRSG
  (economizer, evaporator, superheater, duct burner, SCR, duct, stack,
  fan), condensers and cooling (water-cooled condenser, air-cooled
  condenser, cooler, cooling tower), valves and piping, sources and sinks,
  electrical, annotations. The Components tab uses the same groups.
- Turbine and compressor connection points at five stages on each tilted
  casing line (diagrams saved by 0.1 keep their points).
- Right sidebar with *KPI* and *States* tabs, and below them all the
  diagram settings (lock, stream labels, title, PDF export, fit view). The
  left sidebar keeps the palette, the templates and the selected
  component's or stream's settings.
- The state table adds mass flow, entropy, vapour quality,
  superheating/subcooling degree, density, cp and the fluid, and has a
  *Copy table (TSV)* button.
- The PDF report keeps the label style, state numbering, label size and
  positions, stream colours and the cross unit legend of the canvas, and
  uses the same state table.

### Solver
- Automatic starting values: 25 °C for temperatures, 1 bar for pressures and
  0.5 for qualities instead of 1 in model units (1 K and 1 Pa in SI).
- Automatic bounds for variables used directly as property inputs
  (temperature above 0 K, pressure above 0, quality 0 to 1), which never
  exclude a valid solution.
- Warm start keeps the last converged value of each variable when a later
  solve fails, and starts from the solution saved in a `.df` project when it
  is opened.
- *Equations > Initial values*: guesses and bounds per variable with a unit
  picker, the value each variable started from and its source; saved in the
  project.

### Plots
- Logarithmic axes are drawn as on log paper: decade lines and fainter
  lines at 2..9 times each decade (and 1.1, 1.2, ... when zoomed in), with
  labels at the decades.
- *Export Python* uses the SciencePlots style when installed, LaTeX text when
  LaTeX is available (labels escaped), a journal-column figure size, and
  minor grid lines on log axes.

### Interface
- Monochrome icons, following the light or dark theme, on the entries of
  the File, Edit, Solve, Units, View and Help menus, with the keyboard
  shortcuts right-aligned.
- The toolbar's Solve button is hidden while *Solve > Live solve while
  typing* is on (F2 still solves).

### Examples
- New recuperated Brayton example (air, recuperator by effectiveness, LMTD
  function).
- Every example sets the mass flow of each state (`m[i]` with `Duplicate`),
  so the cycle diagram shows the flow of every stream, and is commented to
  introduce the language and the cycle.
- The example projects include a solved parametric table, a goal seek or an
  optimisation and plots (property diagram, XY or 3D).

### Verification
- Tests of the property pipeline against IAPWS-95 (Tables 7 and 8),
  IAPWS-IF97 (Tables 5 and 15), NIST-JANAF and exact thermodynamic
  identities in five unit systems; solver tests against closed-form
  solutions and a direct CoolProp calculation. See `docs/VERIFICATION.md`.
- `examples/ees_crosscheck.dfs`: a 199-equation model that runs unchanged in
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
