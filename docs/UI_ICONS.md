# Vortia UI icon language

Vortia includes an original, consistently stroked vector set for the main CAD/CFD actions.
Icons are built as Qt resources and resolve from `:/vortia/icons/<name>.svg`.
No fonts or online assets are required, and native Qt icons remain a fallback.

This changes visual identity only; menu/ribbon actions retain the same functional
QAction connections. The blue accent (#338bcc) is chosen for visibility across
dark and light themes.

Mac QA: check icon rendering at 1× and 2× scaling, active/disabled controls,
both themes and different ribbon widths. Verify Qt's SVG image plugin is
available in the final application distribution.
