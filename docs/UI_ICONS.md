# Iconos de la interfaz

Vortia incluye un conjunto propio de 55 iconos vectoriales para sus acciones.
Se redibujaron por completo en la versión 0.5.0 con una única norma:

- rejilla de 24 px con 3 px de margen libre;
- trazo de 1,75 px, extremos y uniones redondeados;
- nodos (vértices, puntos de control) como círculos rellenos;
- líneas discontinuas solo para lo que no es geometría real (ejes, guías, el
  segmento que falta);
- relleno suave solo para indicar una cara o un plano.

Los iconos son dibujos originales de este proyecto, con la misma licencia que
el resto del código. No se copian ni adaptan de otros conjuntos.

## Dónde están

`resources/icons/ui/<nombre>.svg`, servidos como `:/vortia/icons/<nombre>.svg`.
Se generan con:

```sh
python3 tools/ui_icons.py resources/icons/ui
```

`tools/ui_icons.py` contiene la definición de cada icono; para cambiar uno se
edita ahí y se regenera, no se retoca el SVG.

## Color

Los SVG se dibujan en negro y solo importa su opacidad: `UiIconEngine`
(`vortia/ui/UiIcon.hpp`) los pinta con el color de texto del tema en cada
estado (normal, desactivado, seleccionado), así que siguen el modo claro u
oscuro sin duplicar archivos.

## Añadir un icono

1. Definirlo en `tools/ui_icons.py` con las primitivas existentes.
2. Regenerar y añadir el archivo a `resources/vortia.qrc`.
3. Comprobarlo a 20 px en claro y oscuro, también desactivado.
