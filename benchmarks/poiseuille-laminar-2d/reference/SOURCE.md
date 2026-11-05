# Analytic reference — Poiseuille channel

**Type:** Closed-form solution of the steady laminar Navier–Stokes equations for parallel flow between infinite plates (or 2D channel with spanwise homogeneity).

**Derivation (kinematic pressure \(p\), viscosity \(\nu\)):**

\[
0 = -\frac{\partial p}{\partial x} + \nu \frac{\partial^2 u}{\partial y^2},\quad
u(0)=u(H)=0
\quad\Rightarrow\quad
u(y)=\frac{-\partial_x p}{2\nu}\,y(H-y).
\]

\[
U_\mathrm{max}=u(H/2)=\frac{-\partial_x p\,H^2}{8\nu},\qquad
q'=\int_0^H u\,\mathrm{d}y=\frac{2}{3}U_\mathrm{max}H.
\]

**Licence:** Mathematical identity; no third-party dataset.

**Implementation:** `vortia::verification::analyticalU` / `analyticalUmax` / `analyticalFlowPerDepth`.
