# Changelog

All notable changes to DeltaF Studio (formerly DF Studio) are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Cycle diagram (Diagram and Components sub-tabs) and Plots: the sidebars
  can be hidden for a larger view, with `Ctrl+B` (left), `Ctrl+Shift+B`
  (right) or the small arrow at the top of each sidebar; the choice is
  remembered.

### Changed
- Cycle diagram, Components tab: the selected sub-tab (for example the T-Q
  diagram) stays selected when another component is picked in the list.
  Each component kind always shows all its sub-tabs; one that cannot be
  computed yet says what it needs (solve the model, connect the streams,
  set the stream fluid).

### Fixed
- Cycle diagram, Components tab: the scroll bar of the component list sits
  at the edge of the sidebar instead of next to the names.

## [1.3.0] - 2026-11-10

### Added
- Mixture editor: properties at a reference state chosen by the user
  (15 °C and 101.325 kPa by default, ISO 13443): molar mass in kg/kmol,
  density in kg/m³, cp in kJ/kg-K, and LHV and HHV in kJ/kg (also kJ/kmol
  and MJ/Nm³) at the reference temperature, by Kirchhoff's law as in
  ISO 6976.
- Mixture editor: a flow of the mixture or of its flue gas in kg/s, kg/h,
  t/h, kmol/s, kmol/h, Nm³/s, Nm³/h, Nm³/day or Sm³/h, shown in the other
  units with the thermal input. 1 Nm³ is 44.61503 mol (ideal gas at 0 °C
  and 101.325 kPa, DIN 1343).
- Combustion products itemised: fuel, air and each product in mol %,
  dry mol %, mass %, kmol, kg and Nm³ per second, hour or day, with the O2
  content wet and dry and the water dew point of the flue gas.

### Fixed
- The button that clears the Library search showed a box instead of ×.

## [1.2.0] - 2026-11-10

### Added
- Humid air under other names: `HumidAir`, `Humid_Air`, `MoistAir`,
  `Moist_Air`, `AirWater` and `Air_H2O` are the same as `AirH2O` in the
  humid-air functions (`humrat(HumidAir, T=T, P=P, R=0.5)`), in any case.
- Thermophysical functions, as in EES: `kinematicviscosity`,
  `thermaldiffusivity`, `isentropicexponent` (the real-fluid exponent
  −(v/P)(∂P/∂v)ₛ, cp/cv for an ideal gas), `isothermalcompress` (in 1/model
  pressure unit), `volexpcoef` (1/K), and the fluid constants `v_crit`,
  `t_triple`, `p_triple`, `acentricfactor` and `normalboilingpt`. The ideal
  gas gives the isentropic exponent, compressibility and expansion
  coefficient too, and the liquids of DeltaF Studio the kinematic viscosity,
  diffusivity and expansion coefficient.
- Special functions: `erf`, `erfc`, `gamma_`, `factorial`, Bessel functions
  of order 0 and 1 (`bessel_j0` ... `bessel_k1`), and `average(a, b, ...)`.
- `Call Error('message XXXF2', x)` in functions and procedures stops the
  calculation with the message, the value written in place of `XXX`.
- Export simplified script: `PropsSI`, `Props1SI` and `HAPropsSI` are
  written as the named property and `AirH2O` functions, with their SI
  arguments converted to model units (`T + 273.15`, `P*1e5` and a trailing
  `/1000` simplify away). `For` and `While` loops become a guarded
  `Repeat ... Until`, `ElseIf` a nested `If`, `not` a flipped comparison,
  and the operands of `and` / `or` get parentheses, since EES evaluates
  them left to right. `atan2` becomes a helper function written at the top,
  and functions and procedures are moved before the first equation.
- Lookup tables from CSV or text files attached to the project (or next to
  the script): `lookup`, `interpolate` (cubic), `interpolate1` (linear),
  `interpolate2` (quadratic), `interpolate3` and `interpolate2dm`, as in
  EES. A header `T [K]` gives a column a unit, converted to the model units;
  `;` or tab separators accept a decimal comma.
- `integral(f, x, a, b)`: 20-point Gauss–Legendre quadrature of an
  expression, also of property calls (`integral(cp(Air, T=T)/T, T, 300, 600)`).
- String variables (`F$ = 'R134a'`, `G$ = F$`) for fluid, table and column
  names; `$Constant Name# = value [unit]` and `$Constant Name#$ = 'text'`;
  `$Include file` (one level) for shared functions and equations.
- Arrays inside functions and procedures (`T[i] := ...`), and procedure
  outputs into array elements.
- `Module` / `Subprogram`: a block of equations instantiated at each
  `Call` with its own local variables (`pump1.h1`, `pump2.h1`).
- `enthalpy_formation`, `lhv` and `hhv` of fuels and of mixtures
  (`lhv('Methane[0.9]&Ethane[0.1]')`), from NIST-JANAF formation enthalpies.
- `Call Warning('text XXXF2', x)` in functions and procedures: the solve
  goes on and the message is listed after it.
- Molar unit system (`$UnitSystem SI MOLAR C kPa kJ`): h, u, s, cp, cv, v
  and density per kmol, as in EES.
- Ideal-gas mixtures: `enthalpy('Nitrogen[0.75]&CO2[0.1]&Water[0.15]::Ideal', T=T, P=P)`,
  the Gibbs–Dalton mixture of the components' ideal-gas properties with
  thermochemical reference states, so that the enthalpies of reactants and
  products balance (adiabatic flame temperature of methane in air: 2327 K).
  It needs no binary interaction parameters (flue gas with SO2).
- Property call builder: a mixture editor with fractions in mol % (default),
  mass % or vol % (= mol % for gases), presets (natural gas with and
  without sulfur as H2S, lean natural gas, dry air, biogas, syngas, LPG,
  R407C, R410A, R404A), the products of complete combustion of the mixture
  with an excess-air ratio λ, and its heating values.

### Fixed
- `=` compares in the condition of `If`, `ElseIf`, `While` and `Until`
  (`If x = 1 Then`), as in EES; it was a syntax error.
- `PropsSI` results no longer take the model unit of their property: they
  are SI (J/kg, Pa, K), and were shown as kJ/kg, bar or °C.
- Export simplified script: an argument written in parentheses, such as
  `T=(T1 + T2)/2`, lost its opening parenthesis.
- Mixtures at high temperature: CoolProp's flash from P with h, s, u or
  density fails above about the highest temperature of a component's
  equation (natural gas from 300–500 °C, syngas from 700 °C), so expansions
  of hot gases had no solution. The state is now found by a search in T on
  P–T states, to machine precision.
- A `$Constant` cannot redefine a built-in constant (`c#`, `g#`, ...).

## [1.1.0] - 2026-11-09

### Added
- Fluid aliases: every fluid can be written under its usual names, in any
  case. `CO2`, `CarbonDioxide` and `R744` are the same fluid; so are
  `Water`, `H2O` and `Steam`, `Propane`, `C3H8` and `R290`, `N2` and
  `Nitrogen`, and so on (chemical formulas, ASHRAE refrigerant numbers and
  EES names for about 40 common fluids, plus all of CoolProp's own
  aliases). Mixture components and backend-prefixed names are mapped too
  (`'CarbonDioxide[0.9]&Ar[0.1]'`, `'IF97::Steam'`). The fluid table of the
  Library shows the other names of each fluid and finds a fluid by any of
  them, and completion offers them.
- Property model in the fluid name: `CO2::Ideal` (ideal gas) and
  `CO2::Real` (real fluid), with any alias. Without a suffix one state
  input still gives the ideal gas and two the real fluid. The ideal gas now
  takes two inputs too: `entropy(CO2::Ideal, T=T, P=P)` gives
  s°(T) − R ln(P/1 atm), and `density`, `volume`, `pressure`,
  `temperature` (from P and s, P and v, h, ...), `soundspeed` and
  `compressibilityfactor` follow P v = R T. The Library builder's ideal-gas
  option writes `Fluid::Ideal`.
- *File > Export simplified script…*: the equations rewritten in the
  classic EES-style syntax, to copy to the clipboard or save as text. One
  equation per line, numbers converted to model units with the original in
  a comment, a `$UnitSystem` line, the unit of each computed variable as a
  comment, fluid names by property model (formula for the ideal gas, full
  name for the real fluid), and `ifelse`, `pow`, `**` and `d=`
  translated. Whatever has no equivalent is marked *NOT TRANSLATED* on its
  line and listed in a header. Also `dfs export model.dfs`.
- Property diagrams: *Compute processes* joins consecutive states by the
  process they describe instead of a straight line. Each segment is
  classified from its two states by the ratio of the actual to the
  isentropic enthalpy change: compression and expansion follow the path of
  constant polytropic efficiency that ends at the outlet (labelled with the
  isentropic efficiency), throttling keeps h constant, and heat transfer
  follows the isobar (or P linear in h between the inlet and outlet
  isobars when there is a pressure loss).

## [1.0.3] - 2026-11-06

### Fixed
- Windows: `dfstudio.exe` and `dfs.exe` failed to start on a Windows
  installation without the Microsoft Visual C++ Redistributable
  (`STATUS_DLL_NOT_FOUND`, VCRUNTIME140.dll). The C runtime is now linked
  statically, and the release workflow checks that no executable depends
  on the Visual C++ runtime.

## [1.0.2] - 2026-11-05

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
- Monochrome icons before the main tab names (Equations, Solution,
  Analysis, Plots, Define cycle), so the tabs tell apart at a glance.
- A plot of a parametric table without results (not run since it was
  created or changed) says so and points to Run table, instead of an
  empty plot.

### Changed
- Define cycle: the component groups of the palette start collapsed, and
  the component, stream, stream label and text locks start off, in new
  diagrams. Diagrams saved with locks on keep them.

### Fixed
- Plots: the y-axis label no longer touches the divider of the left panel.
- Parametric table: adding an output column to a table with results (after
  a run, or opened from a `.df` project) closed the program. The results
  are now cleared when a column is added, as they already were when one
  was removed.

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

[Unreleased]: https://github.com/dafer238/dfstudio/compare/v1.3.0...HEAD
[1.3.0]: https://github.com/dafer238/dfstudio/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/dafer238/dfstudio/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/dafer238/dfstudio/compare/v1.0.3...v1.1.0
[1.0.3]: https://github.com/dafer238/dfstudio/compare/v1.0.2...v1.0.3
[1.0.2]: https://github.com/dafer238/dfstudio/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/dafer238/dfstudio/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/dafer238/dfstudio/compare/v0.3.0...v1.0.0
[0.3.0]: https://github.com/dafer238/dfstudio/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/dafer238/dfstudio/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/dafer238/dfstudio/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/dafer238/dfstudio/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/dafer238/dfstudio/releases/tag/v0.1.0
