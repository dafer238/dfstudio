# Migrating from EES

DF Studio reads the EES equation language (Klein & Nellis, 2012) and solves
models the same way: an unordered set of equations, decomposed into blocks
and solved with Newton's method. Most EES models therefore run after a few
mechanical edits. This chapter lists those edits, the places where the two
programs *intentionally* behave differently, and the EES features that have
no equivalent yet.

The [user guide](USER_GUIDE.md) explains the program itself, and
[LANGUAGE.md](LANGUAGE.md) the full language.

EES is a product of F-Chart Software; DF Studio is independent and not
affiliated with it. This guide only describes how to run existing EES
models in DF Studio.

Contents

1. [Getting the equations out of EES](#1-getting-the-equations-out-of-ees)
2. [Syntax that carries over unchanged](#2-syntax-that-carries-over-unchanged)
3. [Units: the most important difference](#3-units-the-most-important-difference)
4. [Fluids and property functions](#4-fluids-and-property-functions)
5. [Functions, procedures and directives](#5-functions-procedures-and-directives)
6. [EES windows and their DF Studio equivalents](#6-ees-windows-and-their-df-studio-equivalents)
7. [Features without an equivalent](#7-features-without-an-equivalent)
8. [Step-by-step checklist](#8-step-by-step-checklist)
9. [Worked example](#9-worked-example)
10. [References](#10-references)

## 1. Getting the equations out of EES

Open the `.ees` / `.EES` file directly (*File > Open*, or
`dfs check model.ees`). Two kinds exist and both are read:

* **Text files** (older versions, or saved as text) open as they are.
* **Binary files** written by EES for Windows hold a short header, the
  equations (plain or RTF, when the Equations window used bold, colours or
  highlighting) and then binary sections. DF Studio extracts:
  * the **equations**, with the formatting removed and EES's hidden
    `{$ID$...}`, `{$PX$...}` comments dropped;
  * the **Variable Information** of the variables that appear in the
    equations, appended at the end of the script as `$Units` (units that DF
    Studio recognises), `$Bounds` (finite limits) and `$Guess` lines. The
    guesses are the values of the last EES solution, so a large model
    starts from a converged point.

  Parametric and lookup tables, plots, the Diagram window and the Formatted
  Equations are not imported. The layout of the binary sections is not
  published by f-Chart; it was read from EES 10.5–10.8 files. If a file
  from another version has a different layout, its equations still open,
  without the Variable Information.

DF Studio never overwrites an `.ees` file: *Save* asks for a `.df` or `.dfs`
name.

**Decimal comma.** EES on a Spanish (or other European) Windows locale
writes `1,5` and separates arguments and lists with `;`:
`enthalpy(Water; T=T[1]; P=1,5)`. DF Studio recognises this form (a `;`
inside parentheses, or a comma between digits in an equation) and solves it
as it is. *Edit > Decimal and list separators* converts the whole script to
`1.5` and `f(a, b)` or back; comments are left untouched, and the
conversion is undoable (`Ctrl+Z`). Imported EES files keep the form they
were written in.

The EES header lines are directives. DF Studio understands `$UnitSystem`
(see Section 3) and ignores the purely EES-specific ones (`$Keyboard`,
`$TabStops`, `$CheckUnits`, `$Warnings`, `$Arrays`, `$Complex`, ...) with an
*Info* message in the Problems panel.

## 2. Syntax that carries over unchanged

| Construct | Example | Notes |
|---|---|---|
| Comments | `"text"`, `{text}`, `//` | `{...}` may be nested |
| Equations in any order | `h[2] = h[1] + w` | either side may hold the unknown |
| Several equations per line | `a = 1; b = 2` | |
| Arrays | `T[1]`, `T[i+1]`, `m_dot[3]` | indices must be constants |
| Loops | `Duplicate i = 1, N ... End` | `N` must be a constant |
| Sums and products | `sum(Q[i], i=1, N)`, `product(...)` | |
| EES `IF` function | `y = IF(a, b, x_lt, x_eq, x_gt)` | |
| Math functions | `ln`, `log10`, `exp`, `sqrt`, `abs`, `min`, `max`, `sin`, ... | trigonometry in radians unless `DEG` is set |
| Constants | `pi`, `g#`, `R#`, `sigma#`, `k#`, `N_A#`, `h#`, `c#` | |
| Property calls | `h = enthalpy(Water, T=T[1], P=P[1])` | same names and named inputs |
| Functions and procedures | `Function`, `Procedure`, `Call`, `:=` | see Section 5 |
| Guesses, bounds | `$Guess`, `$Bounds` directives, or the Variables panel | EES keeps these in the Variable Information dialog |

Names are case-insensitive, as in EES, and may contain `|` (the EES
formatting mark for super- and subscripts, as in `h|o_f`). The *Format script* command
(`Shift+Alt+F`) re-indents a pasted model, which helps when the EES text
has lost its layout.

## 3. Units: the most important difference

EES evaluates every equation in the numbers you write. A bracketed unit
(`P = 10 [bar]`) is a label used for unit *checking*; EES does not change
the number, and conversions are explicit (`convert(bar, kPa)`,
`converttemp(C, K, T)`).

DF Studio *converts* every bracketed literal into the model unit system.

| Line | EES, unit system kPa / °C | DF Studio, unit system kPa / °C |
|---|---|---|
| `P = 10 [bar]` | P = 10 (unit warning) | P = 1000 |
| `T = 300 [K]` | T = 300 (unit warning) | T = 26.85 |
| `W = 5 [MW]` (kJ system) | W = 5 | W = 5000 (kW) |
| `dT = 5 [K]` (°C system) | dT = 5 | dT = −268.15: an absolute temperature |
| `dT = 5 [dK]` | not EES syntax | dT = 5 |
| `P = 10*convert(bar, kPa)` | P = 1000 | P = 1000 (same) |

What this means in practice:

1. **Set the same unit system.** Copy the EES unit system into a
   `$UnitSystem` line, for example `$UnitSystem SI MASS DEG BAR KJ C`, or
   pick it in the *Units* menu. Accepted words: `SI`, `Eng`, `Imperial`,
   `MASS`, `K C F R`, `Pa kPa bar MPa atm psia`, `J kJ Btu`, `DEG RAD`.
   `MOLAR` is not supported: DF Studio is mass based (see Section 7).
2. **Literals already in the model units** give the same numbers in both
   programs. A model whose units were consistent in EES (no unit warnings)
   needs no change.
3. **`convert(from, to)` and `converttemp(from, to, T)` work as in EES**:
   `convert` returns the factor between two units of the same dimension
   (`convert(bar, kPa)` = 100) and `converttemp` converts a temperature
   value between scales. Do not combine them with a bracketed unit on the
   same number, which DF Studio already converts (`10 [bar]*convert(bar,
   kPa)` would convert twice).
4. **Temperature differences.** In EES a difference written in `[K]` or
   `[C]` is fine because nothing is converted. In DF Studio write it without
   a unit or with `[dK]` / `[deltaC]`. Display hints follow the same rule:
   use `{dK}`, not `{C}`, on a pinch or an approach temperature.
5. **Trailing unit comments** such as `h[1] = ... {kJ/kg}` set the *display*
   unit of the variable, as EES users already write them. They never change
   the model value.

Dimensions are inferred from the equations, and the Problems panel shows
equations whose terms have different dimensions (grey "unit check"
entries). This is the counterpart of the EES unit checker.

## 4. Fluids and property functions

### 4.1 Property library

EES uses its own fluid property implementations, while DF Studio uses
CoolProp (Bell et al., 2014). For most pure fluids both implement the same
reference equations of state (for example IAPWS-95 for water, Span & Wagner,
1996, for CO2), so properties agree to the accuracy of the equations of
state. Differences larger than about 0.1 % point to a different fluid model
or reference state, not to the solver.

### 4.2 Fluid names

| EES name | DF Studio / CoolProp fluid | Remark |
|---|---|---|
| `Water`, `Steam`, `Steam_IAPWS`, `R718`, `H2O` | `Water` (IAPWS-95) | EES uses IAPWS-95 for all four names ([EES help: Steam_IAPWS](https://fchartsoftware.com/ees/eeshelp/steam_iapws.htm)) |
| `Steam_NBS` | `Water` (IAPWS-95) | EES uses the older Haar, Gallagher & Kell (1984) formulation; differences are small but not zero |
| — | `'IF97::Water'` | IAPWS-IF97 industrial formulation, when a model needs it |
| `CarbonDioxide`, `CO2`, `R744` | `CO2` (Span & Wagner) | |
| `Air`, `Air_ha` | `Air` (pseudo-pure, Lemmon et al.) | real fluid, see below |
| `Nitrogen`/`N2`, `Oxygen`/`O2`, `Argon`/`Ar`, `Hydrogen`/`H2`, `Helium`/`He`, `Methane`/`CH4`, `Ammonia`/`NH3`/`R717` | CoolProp pure fluid | |
| `R134a`, `R245fa`, `Toluene`, ... | the same CoolProp name | |
| `Sodium`/`Na`, `NaK` | `INCOMP::LiqNa`, `INCOMP::NaK` (CoolProp incompressible liquids) | liquid only |
| `Lead`/`Pb`, `LBE`/`LeadBismuth` | `LIQ::Lead`, `LIQ::LBE` (OECD/NEA, 2015) | liquid only, see 4.6 |
| `SolarSalt`, `NitrateSalt`, `Salt(60NaNO3-40KNO3)` | `LIQ::SolarSalt` (Zavoico, 2001) | liquid, 220–600 °C |
| `Therminol66`, `Therminol72`, `TherminolVP1` (= `DowthermA`), `TherminolD12`, `DowthermJ`, `DowthermQ`, `Syltherm800`, `SylthermXLT` | CoolProp incompressibles `INCOMP::T66`, `T72`, `TVP1`, `TD12`, `DowJ`, `DowQ`, `S800`, `XLT` | liquid only |
| mixtures, brines | `'CO2[0.9]&Argon[0.1]'`, `'INCOMP::MEG-30%'` | quotes required |

**Ideal-gas versus real-fluid names.** In EES, chemical formulas such as
`CO2`, `H2O`, `N2`, `O2` and the name `Air` select *ideal-gas* models
(JANAF data), while `CarbonDioxide`, `Water`, `Nitrogen`, `Oxygen` and
`Air_ha` select real-fluid equations of state. DF Studio always uses the
real fluid. Consequences:

* EES ideal-gas calls with one state input (`enthalpy(CO2, T=T1)`,
  `temperature(N2, h=h1)`) also use the ideal gas in DF Studio, with the
  EES reference: h(298.15 K) = enthalpy of formation, s = absolute entropy
  at 1 atm (NIST-JANAF; Chase, 1998). They give the same values as EES
  within the accuracy of the two data sets.
* With **two** inputs (`entropy(H2O, T=T0, P=p_partial)`), DF Studio uses
  the real fluid, while EES still uses the ideal gas. Results differ where
  the real fluid is not an ideal gas (for H2O at 25 °C and a low partial
  pressure CoolProp may even give liquid water), and the absolute values
  differ because of the reference state. Exergy and combustion models that
  rely on formation enthalpies need the one-input form or a check.
* Ideal-gas enthalpies in EES include an enthalpy of formation for species
  such as CO2 and H2O, so their *absolute* values differ completely from the
  real-fluid values. Energy balances use differences and are unaffected, as
  long as all states of one stream use the same model.
* Near saturation or at high pressure (supercritical CO2!) the real fluid is
  the correct model; an EES model that used `CO2` there was in error.

### 4.3 Reference states

Absolute values of h, s and u depend on the reference state of each
equation of state, which may differ between EES and CoolProp. For example,
CoolProp gives h = 505.8 kJ/kg for CO2 at 25 °C and 1.01325 bar. Compare
differences (Δh, Δs, efficiencies, duties) when validating a migrated
model, not absolute enthalpies.

### 4.4 Quality

`quality(...)` returns the vapour fraction in the two-phase region in both
programs. Outside it, EES returns −100 (subcooled) or 100 (superheated),
while DF Studio returns −1, the CoolProp convention. Rewrite tests such as
`IF(x, 99, ...)` or `x > 1` accordingly, or test the phase with temperatures.

### 4.5 Humid air

EES `AirH2O` calls work unchanged, in model units: `enthalpy`, `entropy`,
`volume`, `temperature`, `cp`, `humrat`, `relhum`, `wetbulb` and
`dewpoint`, with the EES inputs `T`, `P`, `w` (humidity ratio), `R`
(relative humidity, 0–1), `B` (wet bulb), `D` (dew point) and `h`; `P` and
two more are required. They call CoolProp's `HAPropsSI` (real-gas mixture
model of Herrmann, Kretzschmar & Gatley, 2009); h, s and v are per kg of
dry air, as in EES.

```
w    = humrat(AirH2O, T=T_db, P=P_amb, R=0.6)
T_wb = wetbulb(AirH2O, T=T_db, P=P_amb, w=w)
h_a  = enthalpy(AirH2O, T=T_db, P=P_amb, w=w)     {kJ/kg dry air}
```

The CoolProp forms `HAPropsSI(...)` and `PropsSI(...)` (SI units) remain
available for anything without a named equivalent.

### 4.6 Liquid metals, molten salts and thermal oils

These are incompressible liquids: h(T, P) = ∫cp dT + (P − P_ref)/ρ(T) and
s = ∫cp/T dT, the convention of CoolProp's incompressible fluids. Inputs:
(T, P), (P, h), (P, s), (T, h) and, where a vapour-pressure correlation
exists (sodium, lead, LBE), `p_sat(Lead, T=...)` and `t_sat(...)`.
`density`, `cp`, `viscosity`, `conductivity` and `prandtl` are available;
there is no vapour phase or critical point, so `quality` returns −1.
Absolute h and s use each source's reference state (DF Studio's own
liquids: h = s = 0 at the melting point), so compare differences.
`enthalpy_vaporization(Fluid, T=...)` (or `P=`) gives h_g − h_f for fluids
with a two-phase region.

## 5. Functions, procedures and directives

`Function` and `Procedure` blocks are copied unchanged:

```
Function lmtd(dT1, dT2)
  If abs(dT1 - dT2) < 1e-6 Then
    lmtd := dT1
  Else
    lmtd := (dT1 - dT2)/ln(dT1/dT2)
  EndIf
End

Procedure split(m, f : m1, m2)
  m1 := f*m
  m2 := (1 - f)*m
End

Call split(m[1], 0.3 : m[2], m[3])
```

Inside them: `If ... Then ... ElseIf ... Else ... EndIf` (also on one
line), `Repeat ... Until`, `While ... End`, `For i = a To b [Step s] ...
End`, `Return`. Two limitations: arrays cannot be used inside function
bodies yet, and `Module`/`Subprogram` (equation-based sub-models) are not
supported; inline their equations, or use `Duplicate` for repeated units.

Directives:

| EES | DF Studio |
|---|---|
| `$UnitSystem ...` | same (Section 3) |
| Variable Information: guess, bounds, display unit | `$Guess h[2] = 300`, `$Bounds x = 0 .. 1`, `$Units T[*] = C`, or the Variables panel |
| `$Include`, `$Common`, `$IF`/`$IFNOT`, `$Reference`, `$Private` | ignored (Info message) |
| Min/Max with a varied variable | Optimize tab, or `$Control vary P[7] to make T[2] = 333` for a target |

## 6. EES windows and their DF Studio equivalents

| EES | DF Studio |
|---|---|
| Equations window | Equations > Editor |
| Formatted Equations | not needed: *Format script* lays out the text |
| Solution window | Solution tab |
| Arrays window | Arrays tab (copy as TSV, export CSV) |
| Residuals window, *Check/Format* | Equations > Dependencies (blocks, residuals, what determines a variable), Problems panel, degrees of freedom in the status bar |
| Variable Information | Variables panel (guess, bounds, display unit, role filter) |
| Parametric Table | Parametric tab |
| Min/Max | Optimize tab |
| Plot windows, property plots | Plots tab (x–y, secondary y axis, T-s, P-h, h-s with saturation dome) |
| Diagram window | Cycle tab: flowsheet drawn from components, with stream values, component reports (Components tab) and a PDF report |
| Lookup Table window | tables can be attached to a `.df` project, but no lookup functions yet (Section 7) |
| hovering a plot | the tooltip lists every series at the hovered x (both branches of a closed cycle) |

## 7. Features without an equivalent

| EES feature | Workaround |
|---|---|
| `Lookup`, `Interpolate`, `TableValue`, `LookupRow` | fit the data (polynomial) and write it as an equation, or a `Function` with `If` branches |
| `Integral`, integral tables | discretise with `Duplicate` and `sum` (trapezoidal rule) |
| string variables (`F$ = 'Water'`) | write the fluid name in each call |
| molar property units (`MOLAR`) | multiply mass-specific values by `molarmass(...)` |
| `Module`, `Subprogram` | inline the equations |
| `$Include`, `$Common` | paste the included file |
| ideal-gas fluids (`CO2`, `N2`, `Air` with one input) | real fluid with two inputs (Section 4.2) |
| uncertainty propagation | not available |
| complex numbers (`$Complex`) | not available |

## 8. Step-by-step checklist

1. Copy the equations into a new script (Section 1).
2. Add the EES `$UnitSystem` line if it is not already there.
3. Read the Problems panel, which is updated as you type (`dfs check
   model.dfs` gives the same list on the command line). Work through the
   errors top to bottom:
   unknown functions (Section 7), single-input property calls and
   string variables (Section 4).
4. Search for `convert(`, `converttemp(` and bracketed units that differ
   from the model units, and apply Section 3. Change temperature
   differences in `[K]`/`[C]` to `[dK]` or no unit.
5. Search for `quality(` and adapt tests on ±100 (Section 4.4).
6. Check the degrees of freedom in the status bar. "0 dof" is the goal; open
   variables appear orange in the Variables panel, and the role filter
   lists *Input*, *Calculated* and *Pending* variables.
7. Solve. If a block does not converge, copy the EES solution values as
   guesses (`$Guess`, or the Variables panel) for the variables of that
   block, which the Dependencies view lists.
8. Validate against the EES solution using differences and efficiencies
   (Section 4.3). Agreement to four significant figures is typical when both
   programs use the same equation of state.
9. Optionally draw the flowsheet in the Cycle tab and attach variables to
   the streams to obtain component reports.
10. Save as `.df` to keep tables, plots and the solution with the model.

## 9. Worked example

EES (unit system SI, kPa, °C, kJ):

```
$UnitSystem SI MASS DEG KPA C KJ
P[1] = 10*convert(bar, kPa)          "boiler pressure"
T[1] = 500 [C]
h[1] = enthalpy(Steam, P=P[1], T=T[1])
s[1] = entropy(Steam, P=P[1], T=T[1])
P[2] = 10 [kPa]
h_s[2] = enthalpy(Steam, P=P[2], s=s[1])
eta_t = 0.85
h[2] = h[1] - eta_t*(h[1] - h_s[2])
x[2] = quality(Steam, P=P[2], h=h[2])
DT_sub = 3 [C]                       "subcooling"
T[3] = temperature(Steam, P=P[2], x=0) - DT_sub
h_air = enthalpy(Air, T=T[3])
```

DF Studio:

```
$UnitSystem SI MASS DEG KPA C KJ
P[1] = 10 [bar]                      "boiler pressure, converted to 1000 kPa"
T[1] = 500 [C]
h[1] = enthalpy(Steam, P=P[1], T=T[1])
s[1] = entropy(Steam, P=P[1], T=T[1])
P[2] = 10 [kPa]
h_s[2] = enthalpy(Steam, P=P[2], s=s[1])
eta_t = 0.85
h[2] = h[1] - eta_t*(h[1] - h_s[2])
x[2] = quality(Steam, P=P[2], h=h[2])    "-1 if single phase, not +-100"
DT_sub = 3 [dK]                      "subcooling: a difference"
T[3] = temperature(Steam, P=P[2], x=0) - DT_sub
h_air = enthalpy(Air, T=T[3], P=101.325)   "real fluid: two inputs"
```

Only three lines change: the `convert` factor, the unit of the
temperature difference, and the second state input of the air call.

## 10. References

* Bell, I. H., Wronski, J., Quoilin, S., & Lemort, V. (2014). Pure and
  pseudo-pure fluid thermophysical property evaluation and the open-source
  thermophysical property library CoolProp. *Industrial & Engineering
  Chemistry Research*, 53(6), 2498–2508. https://doi.org/10.1021/ie4033999
* Chase, M. W. (1998). *NIST-JANAF Thermochemical Tables*, 4th ed.
  *Journal of Physical and Chemical Reference Data*, Monograph 9.
* Herrmann, S., Kretzschmar, H.-J., & Gatley, D. P. (2009). Thermodynamic
  properties of real moist air, dry air, steam, water, and ice (RP-1485).
  *HVAC&R Research*, 15(5), 961–986.
  https://doi.org/10.1080/10789669.2009.10390874
* Klein, S. A., & Nellis, G. F. (2012). *Mastering EES*. f-Chart Software.
* Klein, S. A. (2024). *EES: Engineering Equation Solver, manual*.
  f-Chart Software. https://fchartsoftware.com/ees/
* Lemmon, E. W., Jacobsen, R. T., Penoncello, S. G., & Friend, D. G.
  (2000). Thermodynamic properties of air and mixtures of nitrogen, argon,
  and oxygen from 60 to 2000 K at pressures to 2000 MPa. *Journal of
  Physical and Chemical Reference Data*, 29(3), 331–385.
  https://doi.org/10.1063/1.1285884
* OECD/NEA (2015). *Handbook on Lead-bismuth Eutectic Alloy and Lead
  Properties, Materials Compatibility, Thermal-hydraulics and Technologies*,
  2015 edition, NEA No. 7268. OECD Nuclear Energy Agency.
* Span, R., & Wagner, W. (1996). A new equation of state for carbon dioxide
  covering the fluid region from the triple-point temperature to 1100 K at
  pressures up to 800 MPa. *Journal of Physical and Chemical Reference
  Data*, 25(6), 1509–1596. https://doi.org/10.1063/1.555991
* Wagner, W., & Pruß, A. (2002). The IAPWS formulation 1995 for the
  thermodynamic properties of ordinary water substance for general and
  scientific use. *Journal of Physical and Chemical Reference Data*, 31(2),
  387–535. https://doi.org/10.1063/1.1461829
* Zavoico, A. B. (2001). *Solar Power Tower Design Basis Document*
  (SAND2001-2100). Sandia National Laboratories.
  https://doi.org/10.2172/786629
