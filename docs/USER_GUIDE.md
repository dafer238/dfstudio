# DeltaF Studio user guide

DeltaF Studio is an equation-oriented environment for thermodynamic modelling.
You write the equations of a model in any order; DeltaF Studio works out which
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
  models in DeltaF Studio.

Contents

1. [Installation and start-up](#1-installation-and-start-up)
2. [A first model in five minutes](#2-a-first-model-in-five-minutes)
3. [The window](#3-the-window)
4. [Writing equations](#4-writing-equations)
5. [Units](#5-units)
6. [Solving, and understanding why a model does not solve](#6-solving-and-understanding-why-a-model-does-not-solve)
7. [Results: the Solution tab](#7-results-the-solution-tab)
8. [Studies: the Analysis tab and control loops](#8-studies)
9. [Plots](#9-plots)
10. [Cycle diagram (Define cycle tab)](#10-cycle-diagram)
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
| Windows 10/11 (x64) | `DeltaF_Studio-<version>-windows-x64-setup.exe` | The installer asks whether to install **for all users** (needs administrator rights, installs to `C:\Program Files\DeltaF Studio`) or **for you only** (no administrator rights, installs to `%LOCALAPPDATA%\Programs\DeltaF Studio`), then offers these options, all checked by default except the desktop icon: a Start menu entry; adding DeltaF Studio to the PATH (your user PATH for a per-user install, the system PATH for an all-users install), so `dfstudio` and `dfs` run from any terminal; and opening `.df`/`.dfs` and `.ees` files with DeltaF Studio on double-click. A portable `.zip` is also provided. |
| macOS 11+ (Apple silicon and Intel) | `DeltaF_Studio-<version>-macos-universal.dmg` | Drag *DeltaF Studio* to *Applications*. The app is not notarised by Apple: the first time, right-click it and choose *Open*. |
| Linux (x86-64, glibc 2.38 or newer: Ubuntu 24.04, Debian 13, Fedora 39 or later) | `DeltaF_Studio-<version>-linux-x86_64.AppImage` | `chmod +x` the file and run it. `./DeltaF_Studio-*.AppImage dfs run model.dfs` runs the command-line solver. |

Other ways to install on Windows, after the direct downloads above, which
always have the latest version:

* **winget**: `winget install dafer238.DeltaFStudio` in a terminal installs
  the same installer (add `--scope user` to install for you only).
  `winget upgrade dafer238.DeltaFStudio` updates it; new versions reach
  winget automatically with each release. If Windows shows a security
  warning when you download the installer or the portable zip, install with
  winget instead.
* **Microsoft Store**:
  [DeltaF Studio in the Microsoft Store](https://apps.microsoft.com/detail/9NSJN0W1J4D3).
  The Store version installs without any warning and updates itself, but
  new versions reach it later than the other options, after Microsoft
  reviews each one. It contains the desktop app only, with the `dfstudio`
  command and the file associations; the `dfs` command-line runner comes
  with the installer and winget.

Every package from the releases page contains the desktop app, the `dfs`
command-line runner, the CoolProp library and the example projects (in the
`examples` folder of the installation; open them with *File > Load
example…*). Examples installed under Program Files are read-only: use *Save
as* to keep your changes.

On Windows, if another program was already chosen as the default for
`.ees` files, Windows keeps that choice and may ask which program to use
the next time you open one: pick DeltaF Studio and *Always*. DeltaF Studio is
also listed under *Open with* for every file type it registers. On macOS,
double-clicking a `.df` or `.dfs` file starts DeltaF Studio but does not load
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
| Toolbar | Solve button and the tabs: Equations, Solution (sub-tabs Variables, Arrays), Analysis (Parametric table, Optimization), Plots, Define cycle (Diagram, Components) |
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
  solved, and the toolbar's Solve button is hidden (F2 still solves).
* **Completion** (`Ctrl+Space`, or while typing): functions, property
  names, fluids, variables, directives and, inside `[...]`, units.
  Signature help appears inside calls. The list follows what is being
  written:
  * after `enthalpy(` it opens by itself with the fluids, those the model
    already uses first, then common working fluids, then every other one;
  * after `enthalpy(Water, ` it offers the state inputs not given yet
    (`P=`, `T=`, `h=` ...), and after `P=` the variables that are
    pressures first (by their inferred dimension, else by their name, e.g.
    `P_cond`), then the rest;
  * after `12 [` the units, those of the left-hand variable's quantity first;
  * at the start of a line variables, keywords and directives; inside an
    expression variables, functions and constants.

  A name that starts with the typed text comes first, then one where it
  starts a word (`cond` finds `P_cond`), then a fuzzy match. **Tab**
  accepts the selected suggestion; `Enter` starts a new line and closes the
  list; arrow keys, Page Up/Down and the mouse wheel move through it.
  *Edit > Suggestions shown* sets the height of the list, in rows.
* **Insert menu / Library.** *Insert* opens the Library window at one of
  its sections; *Help > Library* opens it too. Every entry can be searched
  and put at the editor's cursor (double-click or *Insert*):
  * **Property call builder**: choose the fluid model (pure and pseudo-pure
    fluids, water IAPWS-IF97, incompressible liquids and solutions,
    mixtures, humid air), the fluid, the property and its state inputs. The
    inputs offer only valid names for the chosen function (one input for
    `t_sat`, three with `P` for humid air, the ideal-gas option) and a list
    of the model's variables of the right quantity, with their values. The
    call is shown as it will be written (with `;` in decimal-comma
    scripts); *Evaluate* computes it, with the numbers given or with the
    model's current values, before it is inserted.
  * **Property functions**, **math functions**, **constants**, **keywords
    and directives** (inserted as a template, e.g. a `Duplicate ... End`
    block), **state inputs** with their unit in the model, and **units** by
    quantity.
  * **Fluids**: every fluid by group (CoolProp, liquids and heat-transfer
    fluids, humid air) with molar mass, critical temperature and pressure;
    *Builder* uses it in the property call builder.

  A whole line (a builder call with a result variable, a template) goes on
  a new line after the cursor's line, or into it when it is empty.
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

DeltaF Studio separates three things:

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
   comment `h[1] = ... {kJ/kg}`, per quantity with *Units > Units per
   quantity*, or all at once with *Units > Display units preset*. Only
   dimensionally compatible units are offered; the dimension of each
   variable is inferred from the equations.

### 5.1 Starting from a preset and changing single quantities

*Units > Units per quantity* lists the physical quantities (temperature,
pressure, specific enthalpy, mass flow rate, molar enthalpy, ...) with the
unit each is shown in and, in brackets, how many variables of the model
have that quantity. Pick a unit to show **every** variable of that quantity
in it; *Preset* (or *As in the script*, before any preset) goes back. For
example: *Display units preset > Engineering*, then *Units per quantity >
Mass flow rate > t/h*: temperatures in °C, pressures in bar, enthalpies in
kJ/kg and mass flows in t/h. Quantities you changed are shown in bold.

A value is shown in the first unit found in this order:

| | Set with | Reset by |
|---|---|---|
| 1. the variable's own unit | unit column of the Variables panel or the Initial values table | a preset; a new unit for its quantity; *Clear per-variable units* |
| 2. the unit of its quantity | *Units per quantity* | a preset |
| 3. the unit of the last preset | *Display units preset* | another preset; *Reset display units* |
| 4. the script's unit, else the model unit system | `$Units`, `{unit}`, `[unit]` after the equation | |

Dimensionless values (`-`, `%`) are only changed per variable. The preset
and the per-quantity units are saved in the `.df` file (`unit_prefs` in the
`PROJ` section); files from earlier versions open with their per-variable
units and no preset.

**Mass and molar units.** kJ/kg and kJ/kmol are different quantities, not
two units of one: converting needs the molar mass of the fluid, and a
variable does not know which fluid it belongs to (a cycle may carry water
and CO₂). So *Specific enthalpy* offers only per-mass units, and molar
units apply to variables whose dimension is per amount of substance.
Define those in the script, for example

```
h_m[1] = h[1]*molarmass(Water)    // kJ/kmol in the Engineering system
```

and choose their unit under *Molar enthalpy / energy*. The amount of
substance is counted in kmol in the metric unit systems, as the molar
mass is in kg/kmol, so `h*molarmass(...)` is in kJ/kmol and
`R#/molarmass(Nitrogen)` is the specific gas constant in kJ/kg-K.

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

* Give guesses close to the expected solution in *Equations > Initial
  values* (section 6.5) or in the script: `$Guess h[3] = 3000, T[5] = 400`
  (state enthalpies and entropies of a cycle help most).
* Bound variables that must stay in a range: `$Bounds x = 0 .. 1`.
  Temperatures, pressures and qualities used as property inputs are bounded
  automatically.
* Check that the unit system matches the literals (section 5).
* A singular Jacobian usually means the specification is not independent
  (e.g. the sum of balances that does not involve the unknowns): look at the
  failing block in the Dependencies view.
* Once a model has solved, the next solve starts from the previous solution
  (warm start). A `.df` project keeps its last solution, so a model that
  needed good guesses solves again right away when the project is reopened.

### 6.5 Initial values (Equations > Initial values)

Like EES's Variable Information, this table lists, for each variable solved
iteratively (in a Newton block):

| Column | Meaning |
|---|---|
| Start | Value the last solve started from, and where it came from: *guess*, *last* (last solution) or *auto* |
| Guess | Starting value. Empty: the last solution, else an automatic value (25 °C for temperatures, 1 bar for pressures, 0.5 for qualities, 1 otherwise) |
| Lower, Upper | Bounds. Empty: none, or the automatic bound shown in grey (temperature above 0 K, pressure above 0, quality 0 to 1, for variables used as property inputs) |
| Unit | Unit of the start, guess, bounds and solution; the same display unit as in the other tabs |
| Solution | Current value |

Type a value and press Enter; clear a cell to go back to automatic. Values
keep their unit: changing the unit converts them. A guess or bound typed
here replaces a `$Guess` or `$Bounds` of the script (whose values are shown
in grey). *All variables* lists every variable, *Use last solution as
guesses* copies the current solution into the Guess column, and the table is
saved in the `.df` project.

## 7. Results: the Solution tab

* **Variables** sub-tab: every variable with value and unit (filter, sort,
  *Copy table (TSV)* for Excel), block statistics and control-loop logs.
* **Arrays** sub-tab: one column per array, one row per index (the state table
  of a cycle). *Copy table (TSV)* and *Export CSV…* write it in display
  units with full precision.

## 8. Studies

The **Analysis** tab has two sub-tabs.

* **Parametric table**: choose input columns (each row fixes them; their
  `x = constant` equations are disabled automatically) and output columns,
  fill rows (linear spacing), run. Rows are solved in parallel with warm
  starts. *Copy (TSV)* exports the table. A project holds any number of
  tables, each with its own inputs, outputs and rows: the bar above the
  table selects one, and *+ New table*, *Name*, *Duplicate* and *Delete*
  manage them. Plots choose their table in *Table* (shown when there is
  more than one).
* **Optimization** holds goal seek and optimisation; the result of each
  is shown next to its own *Run* / *Optimise* button.
  * **Goal seek / parameter matching**: conditions `Y = value` or
    `Y1 = Y2` with as many varied variables as conditions.
  * **Optimisation**: maximise or minimise a variable over bounded decision
    variables (Brent for one variable, Nelder–Mead, differential evolution),
    with a convergence plot (best objective per evaluation, in display
    units; hover for the values). *Apply to script* writes the optimum into
    the `X = value` equations (keeping literal units) and re-solves.
* **Control loops** in the script: `$Control vary P[7] to make T[2] = 333`
  are applied on every solve.

To free a variable for a study it must be an input: if it is *calculated*
(section 6.2), remove one of the specifications that determine it.

## 9. Plots

The Plots tab holds any number of plots, saved with the project.

* **XY**: columns of a parametric table, arrays by index, or the
  optimisation history. Per series: line colour, line style and width;
  marker colour, shape and size (the marker colour follows the line colour
  until you change it; right-click its button to make it follow again, so
  e.g. a black dashed line with red square markers is possible); legend
  text and **Right y axis** (secondary axis, e.g. efficiencies next to
  powers; its grid is aligned with round numbers of the left axis).
  An empty *y label* / *y2 label* is filled automatically from the series
  on that axis: one series gives `name [unit]`, several give their names
  separated by spaces, with one unit when they share it
  (`W_net Q_in [kW]`) or each its own (`eta_th [-] W_net [kW]`).
* **Property diagram**: T-s, P-h, P-v, h-s and T-h with saturation dome,
  isobars through the states and the cycle path; map the arrays holding
  T, P, h, s, v and give the state order (`1,2,3,4,1; 5,6`).
  Without further options, consecutive states at equal pressure are joined
  along the isobar and the others by a straight line. **Compute
  processes** joins every pair of consecutive states by the process the two
  states describe, classified from the ratio r of the actual to the
  isentropic enthalpy change, r = (h_b − h_a)/(h_s(P_b, s_a) − h_a):
  * *compression* (pressure rise, 1 ≤ r ≤ 5) and *expansion* (pressure
    drop, 0.05 ≤ r ≤ 1): the path at the constant polytropic (small-stage)
    efficiency that ends at the outlet state (Dixon & Hall, 2014, ch. 2),
    labelled with the isentropic efficiency η_s = 1/r or r;
  * *throttling* (pressure drop, |r| < 0.05): h constant;
  * *heating* and *cooling* (anything else, including equal pressures):
    along the isobar, or, with a pressure loss, with P varying linearly with
    h between the inlet and outlet isobars; phase-change points are
    included, so the path is flat across the dome.
* **3D**: x, y, z from a parametric table (surface for a full grid,
  scatter otherwise).

**Logarithmic axes** (*log x*, *log y*; on by default for the pressure
of P-h and P-v diagrams) are drawn as on log paper: strong lines at each
decade and fainter lines at 2, 3, ... 9 times the decade, which crowd
towards the next decade. Decades are labelled; within a few decades 2 and 5
are labelled too, and every line when less than a decade is visible.

*Export Python…* writes a self-contained matplotlib script with the data.
It uses the [SciencePlots](https://github.com/garrettj403/SciencePlots)
style (`pip install SciencePlots`) when it is installed: serif fonts,
inward ticks on all four sides and a single-column figure size
(`FIGSIZE = (3.5, 2.6)` inches at the top of the script; `(7.0, 4.3)` for a
full page width). Text is set with LaTeX when a LaTeX installation is found
(labels are escaped for it) and with matplotlib's own renderer otherwise.
Without SciencePlots the script runs with the matplotlib defaults.

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

The **Define cycle** tab has two sub-tabs: **Diagram** (this section) and
**Components** (section 11). A flowsheet whose streams are bound to state numbers, so labels read
`T[k]`, `P[k]`, `h[k]`, ... of the solution.

Layout: the **left sidebar** holds the component palette, the templates
and, when a component or stream is selected, its settings. The **right
sidebar** has two tabs, **KPI** and **States**, and below them the settings
of the whole diagram: *Lock*, *Stream labels* (style, size, cross
quadrants, stream variables, fluid properties) and *Diagram* (KPI box,
title, *Export PDF report…*, *Fit view*).

* **Templates**: the *Template* row under the palette inserts a complete,
  labelled cycle (simple Rankine, simple and recuperated Brayton,
  vapour-compression refrigeration). In an empty project the template also brings its
  equations, so it solves at once; in an existing drawing it is placed to
  the right, with state numbers that continue after the highest one in use.
* Drag components from the library; drag from an outlet port (red) to an
  inlet port (blue) to connect. `R` rotates, `M` mirrors, `Del` deletes.
* **Component library**, in collapsible groups with an icon each:

  | Group | Components |
  |---|---|
  | Turbines | turbine |
  | Compressors and fans | compressor, fan |
  | Pumps | pump |
  | Heat exchangers | heat exchanger, heater / boiler, cooler / condenser |
  | HRSG | economizer, evaporator, superheater, duct burner, SCR, duct, stack, fan |
  | Condensers and cooling | water-cooled condenser, air-cooled condenser (ACC), cooler, cooling tower |
  | Valves and piping | valve, pipe, mixer, splitter |
  | Sources and sinks | source, sink, stack |
  | Electrical | generator, motor |
  | Annotations | text |

  The specialised items use the results of a general one: economizer,
  evaporator, superheater and water-cooled condenser are two-stream heat
  exchangers (gas or steam = hot side, water = cold side), a fan is a
  compressor, a duct burner a heater, an ACC a cooler, SCR and duct are
  pipes (pressure loss) and a stack is a sink. The Components tab lists the
  diagram's components under the same groups.
* **Connection points** of turbines, compressors, pumps, mixers and
  splitters: select the component and choose, per port, where the stream
  meets it (*Connection points* in the left panel), or unlock streams and
  drag a stream end onto one of the orange circles drawn around the
  selected component. Turbines and compressors have points axially on
  either end face (3 or 4 points) and radially on the tilted upper or lower
  casing line at one of five stages (stage 1 is the first in the flow
  direction: the inlet end of a turbine, the suction end of a compressor);
  pumps have 3 points on each side, mixers and splitters one per side. Each
  machine keeps one inlet and one outlet; a multi-section turbine is drawn
  as turbines in series. Combined with rotate and mirror this gives any
  orientation with short, straight streams.
* Default ports: a pump's triangle points along the flow, with the inlet
  and outlet opposite each other on that axis; a mixer's two inlets are
  opposite each other with the outlet at 90°, and a splitter's two outlets
  are opposite each other at 90° to the inlet. Diagrams saved by 0.2 or
  earlier keep their streams where they were.
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
* **KPIs** (KPI tab): label + variable; *Suggest* proposes powers, heats and
  efficiencies. The KPI box can be hidden from the canvas.
* **States** tab: one row per state number. The columns are the stream
  variables ticked under *Stream labels*, then the mass flow and the
  entropy from the model (an entropy the model does not compute is taken
  from CoolProp when the fluid's reference matches), then from CoolProp
  at the stream's P and h: vapour quality `x` (or *liquid*, *vapour*,
  *supercritical*), `dT_sat` (degree of superheating `T - T_dew`, positive,
  or of subcooling `T_bubble - T`, negative, at the stream pressure;
  *sat* inside the dome), density, cp and the fluid used. *Copy table
  (TSV)* copies it with names and units for a spreadsheet.
* *Export PDF report…* writes the diagram, the KPI table and the state table
  (A4 or A3 landscape); also `dfstudio --report model.df out.pdf`. The PDF
  keeps the canvas labels: cross or box style, state numbers shown or
  hidden, label size, positions, stream colours and the cross unit legend.

## 11. Component results

Double-click a component on the diagram, or open the **Components** sub-tab of *Define cycle*
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
| Ctrl+Space | completion (Tab accepts) |
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
