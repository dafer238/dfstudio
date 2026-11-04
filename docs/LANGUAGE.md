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
a mixture (`'CO2[0.9]&Argon[0.1]'`, quotes required).

Liquid metals, molten salts and thermal oils (liquid only):
`Sodium`, `NaK`, `Lead`, `LBE`, `SolarSalt`, `Therminol66`, `Therminol72`,
`TherminolVP1`/`DowthermA`, `TherminolD12`, `DowthermJ`, `DowthermQ`,
`Syltherm800`, `SylthermXLT` (see MIGRATING_FROM_EES.md §4.6).

Humid air, in model units (CoolProp `HAPropsSI`; per kg dry air):
`enthalpy`, `entropy`, `volume`, `temperature`, `cp`, `humrat`, `relhum`,
`wetbulb`, `dewpoint` of `AirH2O` with inputs `T`, `P`, `w`, `R`, `B`
(wet bulb), `D` (dew point), `h`, e.g. `w = humrat(AirH2O, T=T1, P=P1, R=0.5)`.

Ideal gas from one input: a call with only one state input uses the
ideal-gas model of the fluid, e.g. `h = enthalpy(CO2, T=T[1])`. Allowed:
`enthalpy`, `entropy`, `intenergy`, `cp`, `cv` from `T`, and `temperature`
from `h` or `u`. Values come from the CoolProp ideal-gas part (the equation
of state at 1 Pa); entropy is at 1 atm. For `CO2`, `Water`/`H2O`, `N2`,
`O2`, `CO`, `H2`, `Ar`, `He`, `CH4`, `NH3`, `SO2` the thermochemical reference is used:
h(25 °C) = enthalpy of formation and s = absolute entropy at 1 atm
(NIST-JANAF). Other fluids keep the CoolProp reference.

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
