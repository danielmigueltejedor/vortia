# Iconos de la interfaz

Los iconos de Vortia son los de [Nodalia Design](https://github.com/danielmigueltejedor/nodalia-design)
(`@nodalia/icons`), el conjunto que comparten todas las apps de Nodalia. Así un
mismo concepto (una línea, una acotación, las referencias a objeto) tiene el
mismo icono en Vortia, Forja y el resto. Las herramientas de dibujo técnico
están en el grupo `cad` de ese conjunto.

La norma de dibujo es la de NDL:

- rejilla de 24 px con 3 px de margen libre;
- trazo de 1,75 px, extremos y uniones redondeados;
- nodos (vértices, puntos de control) como círculos rellenos;
- líneas discontinuas solo para lo que no es geometría real (ejes, guías, el
  segmento que falta);
- relleno suave solo para indicar una cara o un plano.

## Dónde están

`resources/icons/ui/<nombre>.svg`, servidos como `:/vortia/icons/<nombre>.svg`.
Son copias sin cambios de `packages/icons/dist/svg/` de nodalia-design; la
revisión de la que salen está en `resources/icons/ui/NDL_REV`. No se editan
aquí: un icono se cambia en nodalia-design y se vuelve a copiar.

```sh
# en nodalia-design
npm run build -w @nodalia/icons
# en vortia
python3 tools/sync_ndl_icons.py <nodalia-design>
```

El script copia exactamente los iconos que lista `resources/vortia.qrc` y falla
si alguno no existe en NDL.

## Color

Los SVG usan `currentColor` y solo importa su opacidad: `UiIconEngine`
(`vortia/ui/UiIcon.hpp`) los pinta con el color de texto del tema en cada
estado (normal, desactivado, seleccionado), así que siguen el modo claro u
oscuro sin duplicar archivos. El campo `accent` de los iconos CAD no se usa en
Qt: aquí los iconos son de un solo color.

## Añadir un icono

1. Buscarlo en nodalia-design (`dist/preview.html` los muestra por grupo). Si
   no existe, se añade allí, no aquí.
2. Añadir su línea a `resources/vortia.qrc` y ejecutar el script.
3. Comprobarlo a 20 px en claro y oscuro, también desactivado.
