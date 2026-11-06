# Blasius reference source

| Field | Value |
|---|---|
| Correlation | Local skin friction \(c_f = 0.664 / \sqrt{\mathrm{Re}_x}\) |
| Thickness | \(\delta_{99}/x \approx 5.0 / \sqrt{\mathrm{Re}_x}\) (Howarth / Schlichting convention) |
| Profile | Classical Blasius \(f'(\eta)\) table (\(\eta = y\sqrt{U_\infty/(\nu x)}\)) |
| Primary literature | H. Schlichting, *Boundary-Layer Theory* (any standard edition reproducing the Blasius table) |
| Licence / terms | Textbook correlations and standard table values; reproduced for verification only |
| Units | \(c_f\) dimensionless; \(\eta\) dimensionless; velocities as \(u/U_\infty\) |

Extraction: table samples in `vortia::verification::blasiusFPrimeTable()` match commonly published Blasius \(f'\) columns used in BL courses. If a future PR replaces them with a digitised table from a specific edition, cite page/table number here.
