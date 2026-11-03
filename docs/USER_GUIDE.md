# DF Studio user guide

DF Studio is an equation-oriented environment for thermodynamic modelling.
You write the equations of a model in any order; DF Studio works out which
equation determines which variable, solves the system with CoolProp
properties, and post-processes the results (tables, studies, plots, a
flowsheet with component reports, PDF reports).

This guide shows how to use the program. Related documents:

* [LANGUAGE.md](LANGUAGE.md): the equation language;
* [HOW_IT_WORKS.md](HOW_IT_WORKS.md): how a model is analysed and solved,
  with the algorithms, tolerances and references (useful for a methodology
  section);
* [ARCHITECTURE.md](ARCHITECTURE.md): code organisation, design decisions and
  the `.df` file format;
* [MIGRATING_FROM_EES.md](MIGRATING_FROM_EES.md): running existing EES
  models in DF Studio.

Contents

1. [Installation and start-up](#1-installation-and-start-up)
2. [A first model in five minutes](#2-a-first-model-in-five-minutes)
3. [The window](#3-the-window)
4. [Writing equations](#4-writing-equations)
5. [Units](#5-units)
6. [Solving, and understanding why a model does not solve](#6-solving-and-understanding-why-a-model-does-not-solve)
7. [Results: Solution and Arrays](#7-results-solution-and-arrays)
8. [Studies: parametric tables, goal seek, optimisation, control loops](#8-studies)
9. [Plots](#9-plots)
10. [Cycle diagram](#10-cycle-diagram)
11. [Component results](#11-component-results)
12. [Files: `.dfs`, `.df`, auto save, attachments](#12-files)
13. [Command line and Python](#13-command-line-and-python)
14. [Keyboard shortcuts](#14-keyboard-shortcuts)
15. [Troubleshooting](#15-troubleshooting)

---

## 1. Installation and start-up

Download the installer for your system from the
[releases page](https://github.com/dafer238/dfstudio/releases):

| System | File | Notes |
|---|---|---|
| Windows 10/11 (x64) | `DF_Studio-<version>-windows-x64-setup.exe` | The installer asks whether to install **for all users** (needs administrator rights, installs to `C:\Program Files\DF Studio`) or **for you only** (no administrator rights, installs to `%LOCALAPPDATA%\Programs\DF Studio`), then offers these options, all checked by default except the desktop icon: a Start menu entry; adding DF Studio to the PATH (your user PATH for a per-user install, the system PATH for an all-users install), so `dfstudio` and `dfs` run from any terminal; and opening `.df`/`.dfs` and `.ees` files with DF Studio on double-click. A portable `.zip` is also provided. |
| macOS 11+ (Apple silicon and Intel) | `DF_Studio-<version>-macos-universal.dmg` | Drag *DF Studio* to *Applications*. The app is not notarised by Apple: the first time, right-click it and choose *Open*. |
| Linux (x86-64, glibc 2.38 or newer: Ubuntu 24.04, Debian 13, Fedora 39 or later) | `DF_Studio-<version>-linux-x86_64.AppImage` | `chmod +x` the file and run it. `./DF_Studio-*.AppImage dfs run model.dfs` runs the command-line solver. |

Every package contains the desktop app, the `dfs` command-line runner, the
CoolProp library and the example projects (in the `examples` folder of the
installation; open them with *File > Load example…*). Examples installed
under Program Files are read-only: use *Save as* to keep your changes.

On Windows, if another program was already chosen as the default for
`.ees` files, Windows keeps that choice and may ask which program to use
the next time you open one: pick DF Studio and *Always*. DF Studio is
also listed under *Open with* for every file type it registers. On macOS,
double-clicking a `.df` or `.dfs` file starts DF Studio but does not load
the file yet; use *File > Open* or drop the file on the window.

To build from source instead (Rust ≥ 1.85; CoolProp is fetched
automatically as a prebuilt library; see [BUILDING.md](BUILDING.md)):

```bash
cargo build --release
./target/release/dfstudio                       # desktop app
./target/release/dfstudio model.df              # open a project or script
./target/release/dfs run examples/rankine.dfs   # command line
```

The app restores the last session (script, settings, plots, diagram) when it
starts without a file. You can also drag a `.df` or `.dfs` file onto the window.

## 2. A first model in five minutes

1. *File > New* (`Ctrl+N`) and type:

   ```
   "Simple Rankine cycle"
   P[1] = 0.1 [bar];  x[1] = 0                    "pump inlet: saturated liquid"
   h[1] = enthalpy(Water, P=P[1], x=x[1]);  s[1] = entropy(Water, P=P[1], x=x[1])
   P[2] = 100 [bar]
   h_s[2] = enthalpy(Water, P=P[2], s=s[1])
   eta_p = 0.8
   h[2] = h[1] + (h_s[2] - h[1])/eta_p
   ```

   While you type, the variable names and the line numbers turn **green**
   (determined), **orange** (more equations needed) or **red**
   (over-specified). The Variables panel on the right counts the open
   degrees of freedom.
2. Press **F2** (or *Solve*). Values appear in the Variables panel; the
   status bar reports the number of blocks and the largest residual.
3. Click a variable in the panel: the editor dims every line that does not
   contain it, so you see at once where it is used.
4. *File > Save as…* and choose `.df` (full project) or `.dfs` (text script).
   For a new project the first *Save as* switches **auto save** on.

`File > Load example…` opens the examples folder to pick a model. Each
example is a commented script that introduces the language, with a cycle
diagram, a solved parametric table, a goal seek or optimisation and plots:

| Example | Shows |
|---|---|
| Simple Rankine | property calls with two inputs, an efficiency equation solved for its unknown, `Duplicate`, a two-input parametric table and its 3D surface, a T-s diagram, goal seek |
| Simple Brayton | real-gas air, back-work ratio, efficiency and specific work against the pressure ratio (two y axes), optimisation of the pressure ratio |
| Recuperated Brayton | a heat exchanger by its effectiveness, a user `Function` (LMTD), property calls inside `Duplicate`, goal seek and optimisation |
| Refrigeration | saturation functions, superheat and subcooling, a mass flow that results from the capacity, P-h diagram, goal seek on the COP |

The installers write these projects from the scripts in the `examples`
folder of the source tree (`dfstudio --write-examples DIR`).

## 3. The window

| Area | Content |
|---|---|
| Menu bar | File, **Edit**, Solve, Units, View, Help |
| Toolbar | Solve button and the tabs: Equations, Solution, Arrays, Parametric, Goal seek / Optimise, Plots, Cycle diagram, Components |
| Variables panel (Equations tab) | status, role, value and display unit of every variable |
| Problems panel (bottom) | errors, warnings, over-specified equations, failed blocks; click to jump to the line |
| Status bar | last action, auto-save indicator, compile / analysis / solve times |

The Equations tab has two sub-views: **Editor** and **Dependencies**
(section 6.3).

## 4. Writing equations

The full syntax is in [LANGUAGE.md](LANGUAGE.md). In short:

* one equation per line, or several separated by `;`, in any order and with
  the unknown on either side: `h[2] - h[1] = w_p`;
* comments: `"..."`, `{...}` (both may span lines) and `// ...`;
* arrays `T[1]`, `T[i+1]` with `Duplicate i = 1, N ... End`, `Sum(...)`;
* properties: `enthalpy(CO2, P=P[1], T=T[1])` and the other property
  functions, in model units; `PropsSI`, `HAPropsSI` in SI;
* `Function` / `Procedure` for logic (`If`, `Repeat`, `While`, `For`).

### 4.1 Editor aids

* **Live structure.** Each edit is re-analysed about 120 ms after you stop
  typing; with *Solve > Live solve while typing* the determined part is also
  solved.
* **Completion** (`Ctrl+Space`): functions, property names, fluids,
  variables, directives and, inside `[...]`, units. Signature help appears
  inside calls.
* **Errors** are underlined; the Problems panel lists them.
* **Bracket matching.** Hover over a `(`, `[` or `{` (or its closing
  partner), or put the caret next to one, and both brackets of the pair
  are highlighted in blue; a bracket without a partner is highlighted in
  red. Braces are matched as (nested) comments.
* **Clipboard.** `Ctrl+C`, `Ctrl+X` and `Ctrl+V` use the system clipboard,
  so text can be copied to and from other programs.
* **Focus** a variable by clicking it in the Variables panel (click again,
  or `Esc`, to clear): lines that do not contain it are dimmed and its
  occurrences are marked.

### 4.2 Edit menu

| Command | Effect |
|---|---|
| Undo / Redo (`Ctrl+Z`, `Ctrl+Y` or `Ctrl+Shift+Z`) | editor history |
| Find (`Ctrl+F`), Replace (`Ctrl+H`) | opens the find bar above the editor; see below |
| Format script (`Shift+Alt+F`) | indents the bodies of `Function`, `Procedure`, `Duplicate`, `If`, `Repeat`, `While`, `For`; aligns trailing comments at the *comment column*; puts one space around top-level `=` and `:=` and after `;` and `,`; removes trailing spaces and repeated blank lines. The equations never change; formatting is undoable. |
| Format on save | format before every save (on by default) |
| Format when the editor loses focus | format when you click outside the editor (on by default) |
| Comment column, Indent | layout options (default 60 and 4) |

### 4.3 Find and replace

`Ctrl+F` (or *Edit > Find*) opens the find bar; `Ctrl+H` (or *Edit >
Replace*) opens it with the replace row. A selection on one line becomes
the search text; a selection over several lines becomes the search range.

* Every match is highlighted, and the bar shows "k of N". `Enter` or `F3`
  goes to the next match, `Shift+Enter` or `Shift+F3` to the previous one
  (both wrap around).
* **Aa** matches case; **ab|** matches whole words only (`T` then skips the
  `T` in `T_1`); **In selection** limits search and replace to the text
  selected when it is switched on (marked in blue in the line numbers).
* **Find all** lists every match with its line; click one to go to it.
* **Replace** (or `Enter` in the replace field) replaces the selected match
  and moves to the next one. **Replace all** replaces every match (in the
  selection, when *In selection* is on); **Before cursor** and **After
  cursor** replace only the matches before or from the cursor (the start
  of the selection).
* Each replacement is one undo step (`Ctrl+Z` in the editor). `Esc` closes
  the bar.

## 5. Units

DF Studio separates three things:

1. **Model unit system**: the units in which every equation is evaluated
   (*Units* menu, or `$UnitSystem` in the script, which takes precedence).
   Presets: Engineering (°C, bar, kJ), SI (K, Pa, J), SI kPa (K, kPa, kJ),
   Imperial (°F, psia, Btu), or a custom one: `$UnitSystem SI C bar kJ`.
2. **Unit literals**: a number followed by `[unit]` is *converted* to the
   model unit system: `P = 10 [bar]` gives 1000 in a kPa model. Use
   `[dK]`, `[deltaC]` for temperature *differences*; `dT = 5 [K]` is read
   as an absolute temperature (−268.15 °C in a °C model). Write differences
   without a unit or with `[dK]`.
   A unit at the end of an equation, after a space, applies to the
   variable the equation computes: in `a = b - c/d + e [bar]`, where `d` is
   the unknown, `d` is given the unit bar (shown in bar, and checked
   against bar). The value is in model units, like every other value.
3. **Display units**: how a value is shown. Set them per variable in the
   Variables panel, with `$Units T[*] = C, W_net = MW`, with a trailing
   comment `h[1] = ... {kJ/kg}`, or all at once with *Units > Display units
   preset*. Only dimensionally compatible units are offered; the dimension
   of each variable is inferred from the equations.

The structural check also flags equations whose terms have different
dimensions (Problems panel, grey "unit check" entries).

## 6. Solving, and understanding why a model does not solve

### 6.1 What happens when you press Solve

1. The equations are matched to the unknowns (each equation determines one
   variable) and split into the smallest blocks that must be solved together
   (Dulmage–Mendelsohn decomposition and Tarjan's strongly connected
   components; see [HOW_IT_WORKS.md](HOW_IT_WORKS.md) §4).
2. Blocks are solved in order. A block with one equation in one unknown,
   written explicitly, is a direct assignment; any other block is solved by
   Newton's method with line search, starting from the guesses.
3. The status bar reports blocks, iterations and the largest scaled
   residual; a failed block is named in the Problems panel.

### 6.2 Degrees of freedom and roles

The Variables panel header shows *determined / pending / over-specified*
counts and the open degrees of freedom. The role filter shows one group:

* **Inputs**: set directly by a constant (`x = 5 [bar]`) or fixed by a
  study;
* **Calculated**: computed from the equations;
* **Pending/over**: not determined yet (missing specifications) or
  over-specified (an equation too many: the Problems panel names it).

### 6.3 Dependencies view (Equations > Dependencies)

For the focused variable it shows how it is computed:

* the **block** that computes it, its type (explicit or Newton), iterations,
  residual and message; the matched equation and the other equations of the
  block, each with its residual;
* **Reads**: the variables the block uses (its direct inputs);
* **Read by**: the variables computed directly from it;
* **Solve path**: every block that must be solved before it, in order.

The **graph** below shows the whole model (default) or the focused variable
with everything upstream (blue) and downstream (orange), limited by
*Depth* if you wish. An edge u → v means "the block computing v reads u";
boxes group variables solved simultaneously. Click a node to focus it,
double-click it to open its equation; `Ctrl+scroll` zooms.

Typical uses: find why a variable is not free (e.g. a split fraction fixed by
an isothermal-mixing specification and a pinch constraint in the same
block), find the large Newton block that fails, see what a change of input
affects.

### 6.4 Convergence tips

* Give guesses close to the expected solution: `$Guess h[3] = 3000, T[5] = 400`
  (state enthalpies and entropies of a cycle help most).
* Bound variables that must stay in a range: `$Bounds x = 0 .. 1`.
* Check that the unit system matches the literals (section 5).
* A singular Jacobian usually means the specification is not independent
  (e.g. the sum of balances that does not involve the unknowns): look at the
  failing block in the Dependencies view.
* Once a model has solved, the next solve starts from the previous solution
  (warm start).

## 7. Results: Solution and Arrays

* **Solution** tab: every variable with value and unit (filter, sort,
  *Copy table (TSV)* for Excel), block statistics and control-loop logs.
* **Arrays** tab: one column per array, one row per index (the state table
  of a cycle). *Copy table (TSV)* and *Export CSV…* write it in display
  units with full precision.

## 8. Studies

* **Parametric**: choose input columns (each row fixes them; their
  `x = constant` equations are disabled automatically) and output columns,
  fill rows (linear spacing), run. Rows are solved in parallel with warm
  starts. *Copy (TSV)* exports the table.
* **Goal seek / parameter matching**: conditions `Y = value` or `Y1 = Y2`
  with as many varied variables as conditions.
* **Optimise**: maximise or minimise a variable over bounded decision
  variables (Brent for one variable, Nelder–Mead, differential evolution),
  with a convergence plot. *Apply to script* writes the optimum into the
  `X = value` equations (keeping literal units) and re-solves.
* **Control loops** in the script: `$Control vary P[7] to make T[2] = 333`
  are applied on every solve.

To free a variable for a study it must be an input: if it is *calculated*
(section 6.2), remove one of the specifications that determine it.

## 9. Plots

The Plots tab holds any number of plots, saved with the project.

* **XY**: columns of the parametric table, arrays by index, or the
  optimisation history. Per series: colour, line style, width, marker,
  legend text and **Right y axis** (secondary axis, e.g. efficiencies next to
  powers; its grid is aligned with round numbers of the left axis).
* **Property diagram**: T-s, P-h, P-v, h-s and T-h with saturation dome,
  isobars through the states and the cycle path; map the arrays holding
  T, P, h, s, v and give the state order (`1,2,3,4,1; 5,6`).
* **3D**: x, y, z from the parametric table (surface for a full grid,
  scatter otherwise).

*Export Python…* writes a self-contained matplotlib script with the data.

Hovering any plot (XY plots, property diagrams and the charts of the
component results) shows a vertical guide and a tooltip with the value of
every computed series at that x; where a curve crosses the guide more than
once (the top and bottom of a cycle in a T-s diagram) each crossing is
listed. Helper lines (saturation dome, isobars, isentrope, pinch marker)
are not read out. The guide snaps to computed points: within 10 px to a
state point, the start or end of a curve or the pinch, otherwise within
6 px to any computed point; only between points is the value interpolated
linearly. On a property diagram the tooltip names the state it snapped to.

## 10. Cycle diagram

A flowsheet whose streams are bound to state numbers, so labels read
`T[k]`, `P[k]`, `h[k]`, ... of the solution.

* **Templates**: the *Template* row under the palette inserts a complete,
  labelled cycle (simple Rankine, simple and recuperated Brayton,
  vapour-compression refrigeration). In an empty project the template also brings its
  equations, so it solves at once; in an existing drawing it is placed to
  the right, with state numbers that continue after the highest one in use.
* Drag components from the palette; drag from an outlet port (red) to an
  inlet port (blue) to connect. `R` rotates, `M` mirrors, `Del` deletes.
* **Connection points** of turbines, compressors and pumps: select the
  component and choose, per port, where the stream meets the casing:
  axially on either end face (3 or 4 points) or radially on the upper or
  lower casing (3 points each); pumps have 3 points on each side. Combined
  with rotate and mirror this gives any orientation with short, straight
  streams.
* Streams route with as few bends as possible. When you release a moved
  component it snaps (within 15 px) so that a port lines up with the port it
  connects to, which gives straight streams.
* Select a stream to set its **state number**, **colour** (fluid presets)
  and **fluid** (auto: from the colour preset or the model's main fluid; or
  a CoolProp name; or *none / mixture*).
* *Stream labels*, two styles:
  * **cross** (default, the usual heat-balance layout): four numbers around a cross,
    P | T over m | h, with the state number just above and a unit legend
    in the top-left corner. The quadrants are detected from the array
    names (`P`, `T`, `m`, `h`) or chosen in the panel;
  * **box**: `T = 550 C` lines for the arrays ticked in the panel, plus
    optionally **density, specific volume and cp** (from CoolProp at the
    stream's P, h).

  Labels follow the zoom (*Resize labels with the view*, on by default;
  off keeps a fixed size on screen), in small (default), medium or large.
  A new diagram uses cross labels centred on the stream line (*Cross: on
  the stream*) without state numbers; *beside the stream* puts the label in
  the middle of its stream, on the side clear of components.
  Hover a stream to see all its values.
* **Lock**: components, streams, stream labels and text can be locked
  against moving or deleting; a new diagram starts with everything locked
  (untick *components* and *streams* in the Lock section to edit it). With labels
  unlocked, drag one to slide it **along its stream**, or choose *Drag
  labels: freely* (or hold Shift) to place it anywhere; *Reset label
  position* undoes it.
* The internal passes of exchangers, heaters, coolers and cooling towers,
  and the outline of turbines, compressors, pumps, valves, pipes, sources
  and sinks, take the colour of their streams. Component colours follow
  the light or dark theme (*View > Theme*).
* **Undo / redo**: `Ctrl+Z` undoes the last diagram edit (move, rotate,
  colour, connection, state number, deletion, labels, KPIs, options) and
  `Ctrl+Y` or `Ctrl+Shift+Z` redoes it; also *Edit > Undo / Redo* while
  the diagram or the Components tab is shown. A drag is one step; pan and
  zoom are not edits. While a text field has the cursor, `Ctrl+Z` undoes
  typing in that field. The history is cleared when another project is
  opened.
* Double-click an empty part of the canvas to fit the drawing to the view.
* **KPIs**: label + variable; *Suggest* proposes powers, heats and
  efficiencies. The KPI box can be hidden from the canvas.
* *Export PDF report…* writes the diagram, the KPI table and the state table
  (A4 or A3 landscape); also `dfstudio --report model.df out.pdf`.

## 11. Component results

Double-click a component on the diagram, or open the **Components** tab
(list by type; choose a component and the view: Summary, Streams, a table,
a chart, Parameters). Each report lists the streams in and out (fluid, m, T,
P, h, s, vapour quality x, degree of subcooling or superheating, ρ, cp),
then:

| Component | Results |
|---|---|
| Heat exchanger | duty of each side, heat loss (kW and %), thermal efficiency, effectiveness, end approaches, LMTD, **internal pinch over 400 duty steps plus the points where a stream changes phase** (warning when it is below the end approaches or when the temperatures cross), UA = Σ dQ/LMTD, NTU, area from U, pressure drops, **stepwise T-Q diagram**, process paths |
| Turbine, compressor, pump | power to/from the fluid, pressure ratio, **isentropic efficiency** and isentropic outlet state, volume flows, pump head and hydraulic efficiency, expansion/compression line with the isentrope, shaft connection |
| Generator, motor | machines on the shaft, gearbox/mechanical, electrical and VFD losses, electric power, shaft-to-grid efficiency, energy-chain chart |
| Pipe, valve | pressure drop (absolute, % of outlet and inlet), heat loss, temperature drop, velocity from a diameter |
| Heater, cooler | duty, thermodynamic mean temperature Δh/Δs, exergy of the heat |
| Mixer, splitter | mass and energy balances, inlet temperature difference, split fractions |
| Cooling tower | range, wet-bulb temperature and approach, effectiveness, evaporation, L/G, fan power |
| all with streams | entropy generation and exergy destruction T0·S_gen (dead state T0 set in the Components tab) |

**Parameters** (component panel or the Parameters view) are numbers in SI
or variable names, or products such as `eta_mec_t*eta_mec_gear`: `U` of a
heat exchanger, `eta_mech`, `eta_elec`, `eta_vfd` and the shaft machines of
generators and motors, `RH` and `fan` of a cooling tower, `D` of a pipe.

Process paths are evaluated with CoolProp:

* **compression and expansion** follow a path of constant polytropic
  (small-stage) efficiency: in each of 60 pressure steps
  dh = dh_s/η_p (compression) or η_p·dh_s (expansion), with η_p chosen so
  that the path ends at the outlet state (Dixon and Hall, 2014). Entropy
  then rises monotonically; an outlet below the isentrope (isentropic
  efficiency above 100 %) is flagged;
* **heat exchange and pipes**: P and h linear along the process at 120
  points, plus the exact points where the stream meets the saturation
  lines, so boiling and condensation corners lie on the dome;
* **valves**: isenthalpic, P logarithmic;
* the **T-Q diagram** uses 400 equal duty steps plus the phase-change
  points of both streams; UA and the pinch come from this profile. A
  dashed vertical line marks where the temperature difference is smallest,
  annotated *Pinch* (inside the exchanger), *Min. approach* (at an end) or
  *Cross* (negative), with ΔT and the duty fraction.

A chart opened in its own tab of the Components view fills the panel.

The saturation dome is drawn with 240 points. These profiles show what
happens inside the component, so an internal pinch or a temperature cross
is visible even when the end temperatures look fine.
Streams without a known fluid (e.g. a gas mixture defined by your own
functions) are drawn linear in h. A fluid chosen automatically is used only
if CoolProp reproduces the model's T(P, h) of the state.

## 12. Files

| | `.dfs` (script) | `.df` (project) |
|---|---|---|
| Content | equations only (plain text) | equations + display units, studies, plots, cycle diagram, last solution, tables, attached files, metadata |
| Format | UTF-8 text | binary container, compressed and checksummed (ARCHITECTURE.md §4) |
| Use | version control, diffs, sharing equations | sharing a complete, ready-to-open study |

`dfs pack` and `dfs unpack` convert between them.

**EES files** (`.ees`, `.EES`, text or binary) open directly, so models
written in EES can be run without retyping them: the equations and the
Variable Information (units, bounds, last solution as guesses) are
imported; tables, plots and diagrams are not. See MIGRATING_FROM_EES.md §1.

**Decimal comma.** Scripts written as `1,5` and `f(a; b)` (Spanish
or other European locales) are recognised and solved as they are. *Edit > Decimal
and list separators* converts the whole script between `1.5 / f(a, b)` and
`1,5 / f(a; b)`; formatting keeps the form of the script.

**Auto save** (*File > Auto save*): saves to the current file 3 s after the
last change. It is off for files you open (so opening a reference model never
modifies it) and switched on for a new project after its first *Save as*.
The status bar shows *auto save* when it is active.

**Project files** (*File > Project files…*): attach data, images or
references to a `.df`; they travel with it.

## 13. Command line and Python

```bash
dfs run model.dfs [--units eng|si|si-kpa|imperial] # solve and print
dfs check model.dfs                                # structure only
dfs info model.df                                  # sections of a project
dfs pack model.dfs -o model.df [--attach f.csv] [--project state.json] [--table name=file.csv]
dfs unpack model.df outdir/                        # script, project.json, results.csv, tables, files
dfstudio --report model.df report.pdf              # cycle report without a window
```

`dfs` exits with status 0 on success, 1 or 2 when the file cannot be read
or the model has errors, and 3 when `dfs run` does not converge, so it can
be used in scripts and continuous integration.

Python reader (standard library only; numpy and pandas optional), from a
clone of the repository: `pip install ./python`.

```python
import dfpy
p = dfpy.load("model.df")
p.results["eta_th"]; p.results.array("T"); p.tables["Parametric"].to_pandas()
```

## 14. Keyboard shortcuts

| Key | Action |
|---|---|
| F2 | solve |
| Ctrl+S / Ctrl+O / Ctrl+N | save / open / new |
| Ctrl+Z, Ctrl+Y (Ctrl+Shift+Z) | undo, redo |
| Shift+Alt+F | format script |
| Ctrl+Space | completion |
| Ctrl+F, Ctrl+H | find, find and replace |
| Enter / Shift+Enter, F3 / Shift+F3 | next / previous match |
| Esc | close completion, clear the focus |
| R, M, Del (cycle diagram) | rotate, mirror, delete |
| Ctrl+Z, Ctrl+Y (cycle diagram) | undo, redo diagram edits |
| Ctrl+scroll | zoom (dependency graph, diagram) |

## 15. Troubleshooting

| Symptom | Cause and remedy |
|---|---|
| A temperature is −268 °C | a temperature difference written with `[K]` or `[C]`: use `[dK]` or no unit |
| A value is 1000× too large | a literal in another unit than intended, or a display unit (e.g. `{kJ/kWh}` on a quantity computed in other units): check section 5 |
| "block n did not converge" | poor guesses or an infeasible specification: open the variable in Dependencies, add `$Guess` values, check the specification range |
| Orange variables remain | degrees of freedom open: add specifications (Variables panel, *Pending/over* filter) |
| Red equation | over-specified: remove it or free one of its variables |
| Cannot vary a variable in a study | it is *calculated*: remove one of the specifications that determine it (Dependencies shows which) |
| Component results show "—" | the stream has no state number, or the arrays are not named T, P, h, s, m, or the fluid is unknown (set it on the stream) |
| CoolProp error for a fluid name | use a CoolProp name or alias (`CO2`, `Water`, `R134a`, `'INCOMP::MEG-30%'`) |
