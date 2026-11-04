# Changelog

All notable changes to DeltaF Studio (formerly DF Studio) are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Windows: an MSIX package for the Microsoft Store, with the `dfstudio`
  command and the `.df`, `.dfs` and `.ees` file associations (the `dfs`
  command-line solver comes with the installer and winget). The
  Store signs it, so it installs without the SmartScreen warning.
- Windows: winget manifest (`dafer238.DeltaFStudio`); each release is
  submitted to winget automatically.
- Windows code signing through the SignPath Foundation open-source program
  (pending approval): the release workflow can have SignPath sign
  `dfstudio.exe`, `dfs.exe` and the installer. The policy is in
  `CODE_SIGNING.md` and at https://deltafstudio.com/code-signing.
- `dfs.exe` carries the icon and version information (product name and
  version) on Windows, like `dfstudio.exe`; the installer too.

## [1.0.1] - 2026-11-04

### Changed
- Renamed the product from DF Studio to DeltaF Studio, to avoid confusion
  with an unrelated product of the same name. The name changes in the window
  title, menus, dialogs, installer, Start menu and desktop shortcuts, macOS app
  bundle, Linux desktop entry, release file names (`DeltaF_Studio-<version>-*`)
  and documentation. Everything else keeps `dfstudio`: the executables
  (`dfstudio`, `dfs`), the crates, the Python package `dfpy`, the `.df` and
  `.dfs` file formats and the repository. Projects saved by DF Studio open
  unchanged.
- The website moved to https://deltafstudio.com. The support links in
  *Help > Support DeltaF Studio* point there; the old dfstudio.neodafer.com
  addresses, including the payment links in 1.0.0, redirect to it.
- Windows: upgrading from 1.0.0 or earlier keeps the existing install folder
  and file associations and replaces the old "DF Studio" shortcuts with
  "DeltaF Studio" ones.

## [1.0.0] - 2026-11-04

First stable release.

### Support
- *Help > Support DF Studio*: how the project is funded and the payment
  options (GitHub Sponsors; one-time, monthly or yearly payments through
  Stripe; managing a subscription). The same information is in the README,
  on the website and behind the repository's *Sponsor* button.
- DF Studio stays free software under the AGPL-3.0, unchanged. Personal,
  academic and educational use is free of charge. Commercial users are asked,
  not required, to consider paying; the licence places no limit on
  commercial use.

## [0.3.0] - 2026-11-04

### Editor
- Completion follows the context: fluids after `enthalpy(` (the model's
  first, then common ones), the missing state inputs after a comma, and
  variables of the right quantity after `P=`, `T=` ... (pressures first
  after `P=`); units of the left-hand variable's quantity after `12 [`.
  Matches inside names (`cond` finds `P_cond`) and fuzzy matches are
  offered after the prefix matches.
- The completion list is taller and its height is set in *Edit >
  Suggestions shown* (rows). Scrolling it with the mouse wheel or the
  arrow keys no longer jumps back to the first suggestion; Page Up/Down
  move a page.
- Only Tab accepts a suggestion; Enter inserts a new line.
- *Insert* menu and Library window: a property call builder (fluid
  model: HEOS, IF97, incompressible, mixture, humid air; property; valid
  state inputs with the model's variables of the right quantity; preview
  of the code and *Evaluate*), and searchable lists of property and math
  functions, fluids (with molar mass and critical point), state inputs,
  constants, keywords and directives (as templates) and units, all
  insertable at the cursor. Also under *Help > Library*.

### Tabs and studies
- Fewer top-level tabs, with sub-tabs: **Solution** (Variables, Arrays),
  **Analysis** (Parametric table, Optimization: goal seek and
  optimisation) and **Define cycle** (Diagram, Components).
- Several parametric tables per project, each with its own inputs,
  outputs and rows (*+ New table*, name, duplicate, delete); XY and 3D
  plots choose their table. Projects store every table and keep the first
  one where older versions read it, so their files still open.
- The results of goal seek and optimisation are shown below their own
  *Run* / *Optimise* button instead of at the top of the tab.
- The optimisation convergence plot shows the objective in display units
  and has the hover tooltip of the other plots.

### Plots
- Separate colours for the line and the markers of a series (e.g. a black
  dashed line with red square markers); also in the Python export
  (`mfc`, `mec`). Older plots keep one colour for both.
- With several series on the same y axis, the automatic axis label lists
  their names separated by spaces instead of disappearing (one unit when
  they share it).

### Units
- *Units > Units per quantity*: choose the display unit of each physical
  quantity (temperature, pressure, specific and molar enthalpy, mass and
  molar flow, ...) on top of a preset, e.g. Engineering with mass flows in
  t/h. A preset resets these choices; a unit set for a single variable in
  the Variables panel still wins over its quantity. Saved in `.df` files
  (`unit_prefs`); older files open unchanged.
- New quantities: molar enthalpy/energy (kJ/kmol, J/mol, ...), molar
  entropy/heat capacity, molar flow rate, amount of substance. A per-mass
  value is not shown per mole (that needs the fluid's molar mass): define
  `h_m = h*molarmass(Fluid)` and choose its unit (User guide §5.1).
- `molarmass(Fluid)`, `Tcrit(Fluid)` and `Pcrit(Fluid)` carry their
  dimension, so the variables computed from them get a unit.

### Fixed
- Molar quantities in the metric unit systems count the amount of
  substance in kmol, consistent with the molar mass in kg/kmol: `R#` is
  8.314 kJ/kmol-K in Engineering (was 0.008314, so `R#/molarmass(...)`
  was 1000 times too small) and `h*molarmass(...)` is in kJ/kmol.

## [0.2.1] - 2026-11-03

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

[Unreleased]: https://github.com/dafer238/dfstudio/compare/v1.0.1...HEAD
[1.0.1]: https://github.com/dafer238/dfstudio/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/dafer238/dfstudio/compare/v0.3.0...v1.0.0
[0.3.0]: https://github.com/dafer238/dfstudio/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/dafer238/dfstudio/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/dafer238/dfstudio/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/dafer238/dfstudio/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/dafer238/dfstudio/releases/tag/v0.1.0
