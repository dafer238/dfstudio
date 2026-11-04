# Verification

How DeltaF Studio's results are checked, and how to compare them with EES.

## 1. Automated tests

They run on every pull request (`cargo test --workspace`, on Linux,
Windows and macOS).

### 1.1 Thermophysical properties (`crates/dfcore/tests/reference.rs`)

| Test | Reference | Tolerance |
|---|---|---|
| Water, single phase: p, cv, w, s at 7 (T, ρ) states | IAPWS R6-95(2018), Table 7 | 1e-6 relative |
| Water, saturation at 275, 450, 625 K: p_s, ρ', ρ'', h', h'', s', s'' | IAPWS R6-95(2018), Table 8 | 2e-6 |
| `IF97::Water`, regions 1 and 2: v, h, s | IAPWS R7-97(2012), Tables 5 and 15 | 1e-8 |
| Critical points of CO2, R134a and water; CO2 triple-point pressure | Span & Wagner (1996); Tillner-Roth & Baehr (1994); IAPWS | 1e-7 to 1e-3 |
| One-input (ideal-gas) h and s of CO2, H2O, N2, O2 at 298.15 K | NIST-JANAF (Chase, 1998): Δh_f and S° | 2e-4, 5e-4 |
| (∂h/∂s)_P = T and (∂h/∂P)_s = v for water, CO2, R134a, air and ammonia in five unit systems | exact identities | 5e-6, 2e-5 |
| Clausius–Clapeyron dp_s/dT = Δh/(T Δv), and g' = g'' at saturation | exact identities | 1e-6 |
| Flash round trips (P,h), (P,s), (T,h), (T,ρ) → state; lever rule; quality | consistency | 1e-8 |
| Humid air: W at saturation from p_ws with the enhancement factor; dew point; relative humidity | ASHRAE Fundamentals ch. 1; Hyland & Wexler (1983) | 0.1 % |

The identities in several unit systems check the conversion of every input
and output, not only the fluid model.

### 1.2 Solver (`crates/dfcore/tests/verification.rs`)

| Test | What it checks |
|---|---|
| Air-standard Brayton, constant cp, written implicitly | T2, T4, w_net and η against the closed form, 1e-12 |
| Counter-flow exchanger written with the LMTD (coupled nonlinear block) | duty against the ε-NTU closed form, 1e-10 |
| Colebrook–White friction factor (implicit) | against an independent fixed-point solution, 1e-10 |
| Rankine example | every state, the power and η against the same cycle computed directly with CoolProp in SI, without the language, units or solver; first law closes to 1e-10 |
| The same cycle in SI (K, Pa, J), Eng, SI (C, kPa, kJ) and Imperial | identical η and T4 |
| `converttemp` | the result is a number in the target scale, never re-converted as a model temperature |
| EES cross-check model (below) | solves; its energy balances close; the implicit Colebrook equation and a `Repeat` loop in a `Function` agree |

Every solver test also requires every equation of the model to hold at the
solution (scaled residual below 1e-8), not only the blocks the solver
reports on.

### 1.3 What these tests found and fixed

* A block could be reported as converged at a *spurious root*. Newton's
  method works on the equations with their denominators cleared; with an
  LMTD, `Q ln(dT1/dT2) = UA (dT1 - dT2)` holds at dT1 = dT2 although the
  equation as written is 0/0 there. The solver now refuses a point where the
  equations as written cannot be evaluated, retries the block on the
  equations as written from a start where the block's own definitions are
  evaluated first, and never reports a solution whose residual is not
  finite.
* `Steam_IAPWS` was read as the IAPWS-IF97 industrial formulation. EES uses
  IAPWS-95 for `Water`, `Steam`, `Steam_IAPWS` and `R718`
  ([EES help](https://fchartsoftware.com/ees/eeshelp/steam_iapws.htm)), and
  so does DeltaF Studio now.
* `converttemp(C, K, T)` gave a result that was displayed as a temperature
  in the model unit (a value in K labelled °C, and converted again by a
  display unit). It is now a plain number.
* Creating CoolProp states from several threads at once could crash (see
  the 0.2.0 changelog).

## 2. Cross-check against EES

[`examples/ees_crosscheck.dfs`](../examples/ees_crosscheck.dfs) runs
unchanged in EES and in DeltaF Studio. It has 199 equations in four parts:

* **A.** A reheat-regenerative Rankine cycle (water) with an open feedwater
  heater, a condenser solved with an LMTD `Function` (cooling-water outlet
  temperature implicit), and a feedwater pipe with an implicit Colebrook
  friction factor, a `Repeat` loop version of it, and Dittus–Boelter heat
  transfer.
* **B.** An R134a refrigeration cycle with superheat and subcooling that
  cools and dehumidifies moist air (`AirH2O`).
* **C.** A recuperated CO2 Brayton cycle (`CarbonDioxide`).
* **D.** A three-stage intercooled air compressor with `Air_ha`.

It uses `Function` (with `If` and `Repeat`), `Procedure`/`Call`,
`Duplicate`, arrays, `Sum`, `min`/`max`, `convert`, `converttemp` and the
property functions h, s, v, T, P, x, ρ, cp, μ, k, Pr, T_sat, P_sat and the
latent heat.

**To compare:** open EES, paste the file into the Equations window, solve,
and compare with the tables below. The unit system is `SI MASS DEG KPA C KJ`: pressures in kPa,
temperatures in °C, energies in kJ, so powers and duties are in kW and
flows in kg/s.

**What to expect.** Water (IAPWS-95), CO2 (Span & Wagner), R134a
(Tillner-Roth & Baehr), air (Lemmon et al., 2000) and humid air use the same
reference equations in both programs, so temperatures, pressures,
qualities, flows, powers and efficiencies should agree to about 1e-5 or
better. Absolute h and s of R134a, CO2 and humid air may differ by a
constant when the reference states differ; differences and everything
derived from them must still agree. A larger difference points to a
different fluid model, a different correlation (EES transport properties
may use other correlations than CoolProp's, so μ, k, Pr, Re and the
friction factor may differ by up to about 1 %), or an error worth
investigating.

### Outcome

The EES column was copied from the EES Solution window, which shows four
significant figures, so differences below about 5e-4 are display rounding.
The two programs agree to within that rounding on every quantity except:

* **Humid air (part B), 0.5 %.** EES gives humidity ratios about 0.5 %
  lower: `w_a1` = 0.01331 against 0.013373 at 30 °C, 50 %. A hand
  calculation for an ideal-gas mixture without the enhancement factor
  gives 0.621945·p_v/(p − p_v) = 0.013313, the EES value. DeltaF Studio uses
  CoolProp's real-gas humid-air model (Hyland & Wexler with the
  enhancement factor), which reproduces the ASHRAE saturation tables (the
  `humid_air_saturation` test). The cooling load, refrigerant flow,
  compressor power and condenser duty inherit a 0.2 % difference. The COP
  does not, because it depends on the refrigerant only.
* **Water transport properties (part A), 0.1 %.** μ differs by 0.12 % and
  Pr by 0.1 %, with k equal to four figures, so Re differs by 0.12 % and the
  Dittus–Boelter coefficient by 0.05 %. These are differences between
  viscosity correlations and are well within their stated uncertainty. The
  friction factor agrees to four figures.
* **R134a saturation pressure at −5 °C, 0.065 %** (243.5 against 243.34
  kPa). Every derived quantity of the refrigerant cycle (COP, discharge
  temperature, quality) agrees to four figures.
* **Absolute CO2 enthalpies** differ by a constant 506.8 kJ/kg
  (`h_c3max`, `h_c6min`) because of a different reference state. All the
  differences, and everything computed from them, agree.

### Results

### A. Reheat-regenerative Rankine (water, IAPWS-95)

| Variable | DeltaF Studio | EES | Rel. diff. |
|---|---:|---:|---:|
| `h[1]` [kJ/kg] | 173.8398 | | |
| `h[2]` [kJ/kg] | 174.58608 | | |
| `h[3]` [kJ/kg] | 670.37724 | | |
| `h[4]` [kJ/kg] | 690.1146 | | |
| `h[5]` [kJ/kg] | 3450.3989 | | |
| `h[6]` [kJ/kg] | 3039.9249 | | |
| `h[7]` [kJ/kg] | 3569.7029 | | |
| `h[8]` [kJ/kg] | 3124.3925 | | |
| `h[9]` [kJ/kg] | 2435.0684 | | |
| `T[2]` [C] | 41.56235 | | |
| `T[4]` [C] | 161.4229 | | |
| `T[6]` [C] | 318.27464 | | |
| `T[8]` [C] | 329.9744 | | |
| `T[9]` [C] | 41.508765 | | |
| `x[9]`  | 0.94125071 | | |
| `y`  | 0.16807583 | 0.1681 | +1.4e-04 |
| `W_hp`  | 41047.397 | 41047 | -9.7e-06 |
| `W_lp`  | 101877.58 | 101878 | +4.1e-06 |
| `W_pumps`  | 2035.8205 | 2036 | +8.8e-05 |
| `W_net`  | 140889.16 | 140889 | -1.1e-06 |
| `Q_boiler`  | 329006.23 | 329006 | -7.0e-07 |
| `Q_cond`  | 188117.07 | 188117 | -3.7e-07 |
| `eta_th`  | 0.42822641 | 0.4282 | -6.2e-05 |
| `eta_carnot_mean`  | 0.48924491 | 0.4892 | -9.2e-05 |
| `T_sat_cond` [C] | 41.508765 | 41.51 | +3.0e-05 |
| `h_fg_cond` [kJ/kg] | 2402.3659 | 2402 | -1.5e-04 |
| `T_cw_out`  | 30.500996 | 30.5 | -3.3e-05 |
| `m_cw`  | 4284.3492 | 4285 | +1.5e-04 |
| `rho_fw` [kg/m^3] | 914.43818 | 914.4 | -4.2e-05 |
| `mu_fw` [Pa-s] | 0.00017241 | 0.0001722 | -1.2e-03 |
| `Re_fw`  | 3692531.2 | 3.697e+06 | +1.2e-03 |
| `f_fw`  | 0.01432716 | 0.01433 | +2.0e-04 |
| `f_loop`  | 0.01432716 | 0.01433 | +2.0e-04 |
| `DELTAP_fw`  | 59.530189 | 59.53 | -3.2e-06 |
| `Pr_fw`  | 1.0750499 | 1.074 | -9.8e-04 |
| `htc_fw`  | 14614.652 | 14622 | +5.0e-04 |

### B. R134a refrigeration and moist-air cooler

| Variable | DeltaF Studio | EES | Rel. diff. |
|---|---:|---:|---:|
| `P_ev` [kPa] | 243.34237 | 243.5 | +6.5e-04 |
| `P_cd` [kPa] | 1016.593 | 1017 | +4.0e-04 |
| `T_r[2]` [C] | 59.574287 | | |
| `x_r[4]`  | 0.28973561 | | |
| `COP`  | 3.632798 | 3.633 | +5.6e-05 |
| `COP_carnot`  | 5.9588889 | 5.959 | +1.9e-05 |
| `w_a1`  | 0.01337259 | 0.01331 | -4.7e-03 |
| `w_a2`  | 0.01001332 | 0.009961 | -5.2e-03 |
| `T_wb1` [C] | 22.000878 | 22 | -4.0e-05 |
| `T_dp1` [C] | 18.450825 | 18.45 | -4.5e-05 |
| `Q_evap`  | 49.5731 | 49.48 | -1.9e-03 |
| `m_r`  | 0.33477583 | 0.3341 | -2.0e-03 |
| `W_comp`  | 13.645983 | 13.62 | -1.9e-03 |
| `Q_cd`  | 63.219084 | 63.09 | -2.0e-03 |
| `SHR`  | 0.66599266 | 0.666 | +1.1e-05 |

### C. Recuperated CO2 Brayton (Span & Wagner)

| Variable | DeltaF Studio | EES | Rel. diff. |
|---|---:|---:|---:|
| `T_c[2]` [C] | 93.539753 | | |
| `T_c[3]` [C] | 295.42516 | | |
| `T_c[5]` [C] | 391.97369 | | |
| `T_c[6]` [C] | 120.3454 | | |
| `rho_c[1]` [kg/m^3] | 331.49902 | | |
| `rho_c[4]` [kg/m^3] | 133.27614 | | |
| `cp_c1` [kJ/kg-K] | 12.110963 | 12.12 | +7.5e-04 |
| `Q_max_rc` [kJ/kg] | 345.61147 | 345.6 | -3.3e-05 |
| `W_tc`  | 5745.6405 | 5746 | +6.3e-05 |
| `W_cc`  | 1639.569 | 1639 | -3.5e-04 |
| `Q_hc`  | 12577.116 | 12577 | -9.2e-06 |
| `eta_c`  | 0.32647161 | 0.3265 | +8.7e-05 |
| `DT_min_rc` [C] | 26.805648 | 26.81 | +1.6e-04 |

### D. Intercooled air compressor (Air_ha, Lemmon)

| Variable | DeltaF Studio | EES | Rel. diff. |
|---|---:|---:|---:|
| `T_out[1]` [C] | 115.04234 | | |
| `T_out[2]` [C] | 115.13526 | | |
| `T_out[3]` [C] | 115.35377 | | |
| `w_st[1]` [kJ/kg] | 100.72525 | | |
| `w_total` [kJ/kg] | 302.20376 | 302.2 | -1.2e-05 |
| `Z_out`  | 1.0025283 | 1.003 | +4.7e-04 |

## References

* IAPWS R6-95(2018). Revised Release on the IAPWS Formulation 1995 for the
  Thermodynamic Properties of Ordinary Water Substance for General and
  Scientific Use.
* IAPWS R7-97(2012). Revised Release on the IAPWS Industrial Formulation
  1997 for the Thermodynamic Properties of Water and Steam.
* Span, R., Wagner, W. (1996). J. Phys. Chem. Ref. Data 25, 1509–1596.
* Tillner-Roth, R., Baehr, H. D. (1994). J. Phys. Chem. Ref. Data 23,
  657–729.
* Lemmon, E. W., Jacobsen, R. T., Penoncello, S. G., Friend, D. G. (2000).
  J. Phys. Chem. Ref. Data 29, 331–385.
* Chase, M. W. (1998). NIST-JANAF Thermochemical Tables, 4th ed.
* ASHRAE Handbook — Fundamentals, ch. 1 (Psychrometrics); Hyland, R. W.,
  Wexler, A. (1983). ASHRAE Trans. 89(2A), 500–519.
* Bell, I. H., Wronski, J., Quoilin, S., Lemort, V. (2014). Ind. Eng. Chem.
  Res. 53, 2498–2508 (CoolProp).
