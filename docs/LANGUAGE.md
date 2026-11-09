# DeltaF Studio equation language

A model is a set of equations written in any order; names are
case-insensitive. The syntax is compatible with EES scripts, so existing
models run unchanged; the differences are listed in
[MIGRATING_FROM_EES.md](MIGRATING_FROM_EES.md). How the equations are
analysed and solved is described in [HOW_IT_WORKS.md](HOW_IT_WORKS.md).

## Equations

```
"comments in double quotes"   {or braces, which may be {nested}}   // or line comments
P[1] = 10 [bar]               // a literal with a unit is converted to the model unit system
dT = 5 [dK]                   // temperature difference: [dK] or [deltaC], never [K]/[C]
h[1] = enthalpy(CarbonDioxide, P=P[1], T=T[1])     {kJ/kg}  <- a trailing unit comment sets the display unit
eta = (h[1] - h[2]) / (h[1] - h_s[2])               // any order, any side
a = b - c/d + e [bar]         // a unit after an equation: unit of the variable it computes (here d)
a = 1; b = 2                  // ';' separates equations on one line
```

Operators: `+ - * / ^` (also `**`), comparisons `< <= > >= <>`, `and`, `or`, `not`.

Units after an equation: a `[unit]` at the end of an equation, preceded by a
space, is the unit of the variable that equation computes (found from the
structure of the model, the same matching the Dependencies view shows). It
does not convert anything: values stay in model units. If the equation does
not compute a single variable, a warning says the unit is ignored. Write
array indices without a space (`T[1]`): `T [m]` is the scalar `T` with a
unit.

Names: letters, digits, `_` and `|` (a formatting mark in imported
scripts, e.g. `h|o_f` for a superscript). Case-insensitive.

Decimal comma: a script may also be written as `x = 1,5` and
`f(a; b)` (Spanish and other European locales). It is recognised from a `;`
inside parentheses (or in a directive, `Duplicate` or `For` line) or a comma
between digits in an equation, and read as `1.5` / `f(a, b)`; a `;` between
equations keeps its meaning. *Edit > Decimal and list separators* converts
a script from one form to the other.

## Arrays and loops

```
N = 10
Duplicate i = 1, N
   T[i+1] = T[i] + dT
End
Q_tot = Sum(Q[i], i=1, N)
```

`Duplicate` bounds and array indices must be constants (numbers, loop
indices, or variables set to a constant somewhere in the model).

## Built-in functions

Math: `sin cos tan asin acos atan atan2 sinh cosh tanh exp ln log10 sqrt abs
sign floor ceil round trunc step min max mod pow`, `ifelse(cond, a, b)`,
`if(a, b, x, y, z)` (x if a<b, y if a=b, z if a>b), `sum`, `product`.

Constants: `pi`, `g#`, `R#`, `sigma#`, `k#`, `N_A#`, `h#`, `c#`.

## Thermophysical properties (CoolProp)

Named-input functions, in model units:

| Function | Example |
|---|---|
| `enthalpy`, `entropy`, `intenergy`, `temperature`, `pressure`, `density`, `volume`, `quality`, `cp`, `cv`, `viscosity`, `conductivity`, `soundspeed`, `prandtl`, `compressibilityfactor` | `s = entropy(Water, P=P1, h=h1)` |
| `surfacetension` | `sigma = surfacetension(Water, T=T1)` |
| `molarmass`, `t_crit`, `p_crit` | `M = molarmass(CO2)` |
| `t_sat`, `p_sat` | `Ts = t_sat(R134a, P=P1)` |

State inputs: `T, P, h, s, u, v, d (rho), x (quality)`. T with h (or u),
which CoolProp cannot flash directly, is solved by a search on pressure. In a
liquid, h hardly depends on P, so when h is a little above h_f(T) both a
compressed liquid and a wet state with a tiny quality match: the liquid is
taken if its pressure is below p_crit. Give P when it is known. Quality returns
`-1` in single-phase states (CoolProp convention; EES scripts that test for ±100 need adapting).

Fluid names: any CoolProp name or alias (`CO2`, `CarbonDioxide`, `Water`,
`R134a`, `Air`, …), a backend prefix (`'IF97::Water'`, `'INCOMP::MEG-30%'`) or
a mixture (`'CO2[0.9]&Argon[0.1]'`, quotes required). Fluid names are not
case-sensitive, and the usual names of a fluid are aliases of each other,
also inside mixtures and after a backend prefix:

| Fluid | Also written |
|---|---|
| `Water` | `H2O`, `Steam`, `Steam_IAPWS`, `Steam_NBS`, `R718` |
| `CO2` | `CarbonDioxide`, `R744` |
| `Air` | `Air_ha`, `R729` |
| `Nitrogen`, `Oxygen`, `Argon`, `Hydrogen`, `Helium`, `Neon` | `N2`, `O2`, `Ar`, `H2`, `He`, `Ne` (and `R728`, `R732`, `R740`, `R702`, `R704`, `R720`) |
| `CarbonMonoxide`, `SulfurDioxide`, `HydrogenSulfide`, `NitrousOxide`, `Ammonia` | `CO`, `SO2`, `H2S`, `N2O`, `NH3` (`R717`) |
| `Methane`, `Ethane`, `Propane`, `Butane`, `IsoButane`, `Pentane` | `CH4` (`R50`), `C2H6` (`R170`), `C3H8` (`R290`), `C4H10` (`R600`), `R600a`, `C5H12` (`R601`) |
| `Ethylene`, `Propylene`, `Methanol`, `Ethanol`, `Benzene`, `Toluene` | `C2H4` (`R1150`), `C3H6` (`R1270`), `CH3OH`, `C2H5OH`, `C6H6`, `C7H8` |

The full list is `FLUID_ALIASES` in `crates/dfcore/src/props.rs`, and the
fluid table of the Library shows the other names of every fluid.

Liquid metals, molten salts and thermal oils (liquid only):
`Sodium`, `NaK`, `Lead`, `LBE`, `SolarSalt`, `Therminol66`, `Therminol72`,
`TherminolVP1`/`DowthermA`, `TherminolD12`, `DowthermJ`, `DowthermQ`,
`Syltherm800`, `SylthermXLT` (see MIGRATING_FROM_EES.md §4.6).

Humid air, in model units (CoolProp `HAPropsSI`; per kg dry air):
`enthalpy`, `entropy`, `volume`, `temperature`, `cp`, `humrat`, `relhum`,
`wetbulb`, `dewpoint` of `AirH2O` with inputs `T`, `P`, `w`, `R`, `B`
(wet bulb), `D` (dew point), `h`, e.g. `w = humrat(AirH2O, T=T1, P=P1, R=0.5)`.

Ideal gas or real fluid. A fluid name may carry the property model as a
suffix: `CO2::Ideal` (ideal gas) or `CO2::Real` (real fluid, equation of
state), with any alias (`CarbonDioxide::Ideal`, `H2O::Real`). Without a
suffix the model follows the number of state inputs: one input gives the
ideal gas, two the real fluid.

```
h1 = enthalpy(CO2, T=200)                    // one input: ideal gas
h2 = enthalpy(CO2, T=200, P=1.013)           // two inputs: real fluid
s3 = entropy(CO2::Ideal, T=200, P=1.013)     // ideal gas, s(T, P)
h4 = enthalpy(CarbonDioxide::Real, T=200, P=80)
```

Ideal-gas relations: h(T) and cp0(T) are the fluid's own ideal-gas part of
the CoolProp equation of state (the equation of state at 1 Pa), u = h − R T,
cv = cp − R, s(T, P) = s°(T) − R ln(P / 1 atm), P v = R T, Z = 1 and speed of
sound √(cp/cv R T). Inputs: `T`, `h` or `u` alone (for `enthalpy`,
`intenergy`, `cp`, `cv`, `temperature`, and `entropy` at 1 atm), or two of
`T, P, h, u, s, v, d` that fix T and P (T with h is not a state, since h
depends on T only). `quality`, `viscosity`, `conductivity`, `prandtl`,
`surfacetension` and the saturation functions need the real fluid; `::Real`
needs two inputs. For `CO2`, `Water`/`H2O`, `N2`, `O2`, `CO`, `H2`, `Ar`,
`He`, `CH4`, `NH3`, `SO2` the ideal gas uses the thermochemical reference:
h(25 °C) = enthalpy of formation and s = absolute entropy at 1 atm
(NIST-JANAF; Chase, 1998), as EES does. Other fluids keep the CoolProp
reference, and the real fluid always does, so do not mix the two models in
one energy balance.

Latent heat: `enthalpy_vaporization(Water, T=100)` (or `P=`).

Unit conversion: `convert(bar, kPa)` (= 100, a factor) and
`converttemp(C, K, T)`.

CoolProp style, in SI units: `PropsSI('H','P',p,'T',t,'CO2')`,
`Props1SI('CO2','Tcrit')`, `HAPropsSI('W','T',t,'P',p,'R',rh)`.

## Functions and procedures (logic and flow)

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
  m2 := (1-f)*m
End

Call split(m[1], 0.3 : m[2], m[3])
```

Statements inside functions: assignment (`:=` or `=`), `If/Then/ElseIf/Else/EndIf`
(also single-line), `Repeat ... Until cond`, `While cond ... End`,
`For i = a To b [Step s] ... End`, `Call`, `Return`.

## Directives

```
$UnitSystem Eng            // or SI | Imperial | custom: SI C bar kJ  (also K kPa MPa psia atm J Btu, deg/rad)
$Guess h[2] = 300, T[5] = 400
$Bounds x = 0 .. 1
$Units T[*] = C, P[*] = bar, W_net = MW
$Control vary P[7] to make T_gas[2] = 333       // control loop (phase 4)
```
