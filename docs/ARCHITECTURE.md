# DeltaF Studio — architecture and design decisions

DeltaF Studio is a desktop, cross-platform, equation-oriented solver for
engineering models, with features for thermodynamic cycles. This document
describes how the code is organised, the design decisions and their
reasons, the `.df` project file format and how the program is packaged.
The mathematics of the solver (structural analysis, Newton method,
tolerances, properties, studies) is in [HOW_IT_WORKS.md](HOW_IT_WORKS.md).

## 1. Code organisation

```
dfstudio/
├── crates/dfcore   library: language, units, properties, structural analysis, solver, studies, .df format
├── crates/dfcli    `dfs` command-line runner (batch runs, CI, scripting)
├── crates/dfapp    desktop application (egui/eframe): editor, plots, cycle diagram, reports
├── python/dfpy     Python reader for .df projects (post-processing)
├── examples/       example models (also bundled with the installers)
├── packaging/      icons and installer scripts for Windows, macOS and Linux
└── docs/           documentation
```

`dfcore` has no user-interface code and is used unchanged by the app, the
command line and the tests. Its modules:

| Module | Role |
|---|---|
| `lexer`, `parser`, `ast` | text to syntax tree, with error recovery |
| `locale` | decimal comma / `;` list separator |
| `compile`, `tape` | equations to residual tapes; automatic differentiation |
| `units`, `dims` | unit systems, conversions, dimensional inference |
| `structure` | matching, Dulmage–Mendelsohn decomposition, block order |
| `solver` | explicit evaluation and Newton / Levenberg–Marquardt per block |
| `funcs` | user `Function` / `Procedure` interpreter |
| `props`, `liquids` | CoolProp, ideal gas, humid air, liquid metals and salts |
| `studies` | parametric tables, goal seek, optimisation, control loops |
| `diagrams` | saturation dome and isobars for property plots |
| `dffile` | the `.df` binary container |
| `ees` | import of EES files (text and binary), to help users move existing models |

## 2. Design decisions

### D1 — Rust for the whole program

The solver is the performance-critical part (thousands of equations, many
property calls, parametric sweeps of hundreds of runs). Rust gives native
speed without a garbage collector, memory safety, painless multi-threading
(`rayon`) and single-binary distribution on Windows, macOS and Linux.

### D2 — egui/eframe as desktop shell

| Option | Pros | Cons |
|---|---|---|
| **egui/eframe (chosen)** | Pure Rust, one executable per platform, GPU rendering, immediate mode makes live feedback simple, `egui_plot` for interactive plots | Fewer ready-made widgets (code editor and 3D plots are custom) |
| Tauri + web front-end | Rich widgets | Two languages and toolchains, IPC overhead for large result sets, webview differences per OS |
| Qt / PySide | Mature widgets | Python in the hot path or a C-ABI bridge; heavy packaging |
| Electron | Rich widgets | Large footprint, slow start |

### D3 — Python only where it adds value

All interactive work (editing, solving, plotting) stays in Rust for latency.
Python is offered for post-processing: `dfpy` reads `.df` files into
`numpy`/`pandas` with the standard library only, and plots can be exported
as self-contained matplotlib scripts.

### D4 — A declarative equation language

Models are equations in any order, not assignments in sequence, so the
same model answers different questions (fix the efficiency and compute the
power, or fix the power and compute the efficiency) without being
rewritten. The syntax covers arrays (`T[i]`, `Duplicate`, `Sum`), property
functions with named inputs (`enthalpy(Water, P=P[1], T=T[1])`), functions
and procedures with `If`, `Repeat`, `While` and `For`, and directives
(`$Guess`, `$Bounds`, `$Units`, `$UnitSystem`, `$Control`). It is compatible
with EES scripts, so existing models can be opened and run while users move
over ([MIGRATING_FROM_EES.md](MIGRATING_FROM_EES.md)). Reference:
[LANGUAGE.md](LANGUAGE.md).

### D5 — Model unit system and per-variable display units

Equations are evaluated in one *model unit system* (default engineering:
°C, bar, kJ/kg, kW). Literals carrying a unit (`10 [bar]`) are converted
into it, property functions convert at the call boundary, and every variable
may be shown in its own *display unit*. Conversions are affine and only
happen at boundaries, so they cost nothing inside the Newton iterations.
In the application a display unit can be set per variable, per quantity
(dimension), or for all quantities with a preset; the finer choice wins.

### D6 — Structure first, numbers second

The structural analysis runs on every edit (in a background thread, about
120 ms after typing stops) and needs no numbers. It gives the live colouring
of the editor, the degrees of freedom, and the block order the solver uses,
so a model is decomposed into small blocks before any iteration starts (see
HOW_IT_WORKS.md §4).

### D7 — Performance

* Tapes are flat vectors evaluated without allocation in the hot loop.
* The structural plan is computed once per set of fixed variables and reused
  for every row of a parametric table and every optimiser evaluation.
* Parametric rows and differential-evolution populations are solved in
  parallel; CoolProp state objects are per thread.
* The Rankine example (25 equations) solves in about 3 ms on a laptop-class
  CPU; most of the time of larger cycle models is spent inside CoolProp
  flashes.

## 3. Plots

* egui_plot for 2D; the 3D view is a small orthographic painter (yaw/pitch
  rotation, painter's algorithm depth ordering of the surface quads) so no
  GPU 3D dependency is added.
* Property diagrams (`dfcore::diagrams`): the saturation dome is sampled from
  the triple point to just below the critical temperature with points
  clustered towards T_c; isobars are swept in temperature and the two-phase
  segment is filled by a quality sweep at the saturation pressure (flat in
  T-s). Heat-addition/rejection paths are evaluated along P = const in
  entropy, so they follow the real curvature instead of a chord. With
  *Compute processes* every segment is classified from its two states
  (compression, expansion, throttling, heat transfer) and drawn with the
  same process paths as the component reports (`components::path`):
  constant polytropic efficiency for turbomachines, P linear in h for heat
  exchangers, constant h for valves.
* Colours: Okabe–Ito qualitative palette (colour-blind safe) for series and
  viridis for 3D surfaces (Okabe & Ito 2008; van der Walt & Smith 2015).
* Export: the same data is written into a matplotlib script (Hunter 2007),
  which is the recommended path for publication-quality vector figures.

## 4. Project file format `.df`

`.df` is DeltaF Studio's own binary format (not a renamed ZIP or other
container). It is specified in `crates/dfcore/src/dffile.rs`; a reader for
Python is in `python/dfpy`.

```
offset size  field
0      8     signature 89 44 46 0D 0A 1A 0A 00   ("\x89DF\r\n\x1a\n\0")
8      4     format version major.minor (u16 + u16)
12     4     flags (reserved)
16     4     number of sections N
20     8     offset of the section table
28     4     CRC-32 of bytes 0..28
32     ...   section payloads
T      ...   section table: tag(4) codec(1) reserved(1) name length(2)
             offset(8) stored length(8) raw length(8) CRC-32 of raw payload(4) name
```

| Section | Content |
|---|---|
| `META` | generator, creation / modification time, title (JSON) |
| `SCRP` | equations (UTF-8) |
| `PROJ` | application state: unit system, display units (per variable, and preset + per quantity in `unit_prefs`), parametric table, goal-seek and optimisation forms, plots (JSON) |
| `RSLT` | last solution: names, units, f64 values, known flags (binary) |
| `TABL` | numeric tables, e.g. the parametric results (binary, row-major f64) |
| `FILE` | attached files (data tables, images, references), raw bytes |

Design decisions:

* **Signature** after the PNG signature (ISO/IEC 15948:2004, §5.2): the
  non-ASCII first byte rejects text tools, and the CR-LF, Ctrl-Z, LF bytes
  reveal a file damaged by a text-mode transfer.
* **Integrity**: CRC-32 (ISO 3309 / ITU-T V.42 polynomial) over the header
  and over every uncompressed payload; the reader reports which section is
  damaged instead of loading wrong numbers.
* **Compression** per section with zlib (RFC 1950/1951), only when it saves
  space, so the format is readable from any language with zlib.
* **Numbers** are stored as IEEE 754 binary64 little-endian: lossless, and
  loading needs no text parsing.
* **Evolution**: a reader rejects a newer major version, while unknown
  section tags (added in later minor versions) are kept and written back
  unchanged. Every field of `PROJ` has a default, so older files keep
  opening.
* **Safety**: saves are atomic (temporary file + rename); all lengths are
  bounds-checked before allocation; extraction refuses file names that
  would escape the target folder.

Tools: *File → Save* (`.df` by default, `.dfs` for script only), *File →
Project files…* (attach / extract / remove), drag-and-drop to open;
`dfs info|pack|unpack|run` on the command line; `dfpy.load()` in Python.

## 5. Cycle diagram, KPIs and report

* The flowsheet is data (`CycleDiagram` in the project state): components
  with position, quarter-turn rotation and mirror; streams between
  (component, port) pairs with a state number. Component symbols are lists
  of primitives (polygons, circles, text) in local coordinates, so the same
  definition is drawn on screen and in the PDF.
* Binding: stream labels read `base[k]` for the chosen array bases from the
  current solution and convert them to the display units; nothing is
  copied, so every solve updates the diagram.
* Routing: each stream leaves and enters its ports through a short stub; the
  orthogonal path between the stubs is the best of a small set of two- and
  three-segment candidates scored by length, bends and crossings of
  component boxes. Labels are placed on the first side of a component that
  no stream crosses.
* PDF: a small writer for PDF 1.4 (ISO 32000-1:2008) with the standard
  Helvetica fonts (WinAnsi encoding, AFM widths for alignment); the report
  needs no external tool, unlike a LaTeX-based export.
* Parameter matching / multi-target goal seek reuse the `$Control` machinery:
  `Y1 = Y2` becomes `__match = Y1 − Y2` driven to zero; n conditions with n
  varied variables are solved simultaneously (targets fixed, varied
  variables freed) or, if that fails, by Broyden's method (Broyden 1965).

## 6. Distribution

* **CoolProp** is a prebuilt shared library (`CoolProp.dll`,
  `libCoolProp.dylib`, `libCoolProp.so`) from the `coolprop-sys` crates. It
  is loaded at run time (`props.rs`, module `cplib`) from, in order:
  `$DFSTUDIO_COOLPROP`, the executable's folder, `../lib` (AppImage),
  `../Frameworks` and `../Resources` (macOS bundle), and finally the build
  folder (development). An installed copy therefore never depends on the
  machine it was built on.
* **Examples** are written by the app itself at packaging time
  (`dfstudio --write-examples DIR`): each bundled template is solved and
  saved as a `.df` project with its cycle diagram. The app looks for them
  next to the executable (`examples`), in `../Resources/examples` and in
  `../share/dfstudio/examples`.
* **Packages**: Inno Setup installer and portable zip (Windows), universal
  `.app` in a DMG (macOS), AppImage (Linux). Scripts are in `packaging/`;
  how to build them and how releases are made is in
  [BUILDING.md](BUILDING.md).

## References

* Broyden, C.G. (1965). A class of methods for solving nonlinear simultaneous equations. *Mathematics of Computation* 19(92), 577–593. doi:10.1090/S0025-5718-1965-0198670-6
* Dixon, S.L., Hall, C.A. (2014). *Fluid Mechanics and Thermodynamics of Turbomachinery*, 7th ed. Butterworth-Heinemann. doi:10.1016/C2011-0-05059-1 (polytropic efficiency; process paths of the component results)
* Hunter, J.D. (2007). Matplotlib: a 2D graphics environment. *Computing in Science & Engineering* 9(3), 90–95. doi:10.1109/MCSE.2007.55
* ISO 32000-1:2008. *Document management — Portable document format — Part 1: PDF 1.7*. International Organization for Standardization.
* ISO/IEC 15948:2004. *Portable Network Graphics (PNG): Functional specification*.
* Okabe, M., Ito, K. (2008). *Color Universal Design (CUD): how to make figures and presentations that are friendly to colorblind people*. J*Fly. https://jfly.uni-koeln.de/color/
* van der Walt, S., Smith, N. (2015). *A better default colormap for Matplotlib* (viridis). SciPy 2015. https://bids.github.io/colormap/

The solver references are listed in [HOW_IT_WORKS.md](HOW_IT_WORKS.md#11-references).
