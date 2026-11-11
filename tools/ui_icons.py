"""Vortia tool icons, in NDL's bold style: one grid (24), one stroke (2.25), round caps and joins, 3 px of clear margin.
Each entry is a list of SVG fragments. `dot(x, y)` is a filled node; `d="..."` strings are stroked paths."""
import os

def P(d, **kw):
    extra = ''.join(f' {k.replace("_", "-")}="{v}"' for k, v in kw.items())
    return f'<path d="{d}"{extra}/>'
def C(x, y, r, **kw):
    extra = ''.join(f' {k.replace("_", "-")}="{v}"' for k, v in kw.items())
    return f'<circle cx="{x}" cy="{y}" r="{r}"{extra}/>'
def R(x, y, w, h, rx=1.5, **kw):
    extra = ''.join(f' {k.replace("_", "-")}="{v}"' for k, v in kw.items())
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}"{extra}/>'
def dot(x, y, r=1.9):
    return C(x, y, r, fill='#000', stroke='none')
DASH = dict(stroke_dasharray='2 3')
SOFT = dict(fill='#000', fill_opacity='0.18')

I = {
 # --- edit ---
 'undo': [P('M8 5 3.5 9.5 8 14'), P('M3.5 9.5h11a5.25 5.25 0 0 1 0 10.5H11')],
 'redo': [P('M16 5l4.5 4.5L16 14'), P('M20.5 9.5h-11a5.25 5.25 0 0 0 0 10.5H13')],
 'copy': [R(4, 8, 12, 12), P('M8 8V5.5A1.5 1.5 0 0 1 9.5 4h9A1.5 1.5 0 0 1 20 5.5v9a1.5 1.5 0 0 1-1.5 1.5H16')],
 'paste': [P('M9 5H6.5A1.5 1.5 0 0 0 5 6.5v13A1.5 1.5 0 0 0 6.5 21h11a1.5 1.5 0 0 0 1.5-1.5v-13A1.5 1.5 0 0 0 17.5 5H15'), R(9, 3, 6, 4, 1), P('M9 12h6M9 16h4')],
 'delete': [P('M4 7h16'), P('M9.5 7V5a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1v2'), P('M6.5 7l.8 12.1a1.5 1.5 0 0 0 1.5 1.4h6.4a1.5 1.5 0 0 0 1.5-1.4L17.5 7'), P('M10 11v5.5M14 11v5.5')],
 'close': [P('M6 6l12 12M18 6 6 18')],
 'check': [P('M4.5 12.5l5 5 10-11')],
 'chevron-down': [P('M6 9.5l6 6 6-6')],
 'chevron-up': [P('M6 14.5l6-6 6 6')],
 'chevron-right': [P('M9.5 6l6 6-6 6')],
 'search': [C(10.5, 10.5, 6.5), P('M15.5 15.5l5 5')],
 'settings': [P('M4 7h8M18 7h2'), C(15, 7, 2.5), P('M4 17h2M12 17h8'), C(9, 17, 2.5)],
 # --- files ---
 'new': [P('M13.5 3h-6A1.5 1.5 0 0 0 6 4.5v15A1.5 1.5 0 0 0 7.5 21h9a1.5 1.5 0 0 0 1.5-1.5v-12z'), P('M13.5 3v4.5H18'), P('M12 11.5v6M9 14.5h6')],
 'open': [P('M3.5 18.5v-12A1.5 1.5 0 0 1 5 5h4l2 2.5h8A1.5 1.5 0 0 1 20.5 9v9.5A1.5 1.5 0 0 1 19 20H5a1.5 1.5 0 0 1-1.5-1.5z'), P('M3.5 11h17')],
 'save': [P('M5.5 4H16l4 4v10.5a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18.5v-13A1.5 1.5 0 0 1 5.5 4z'), P('M8 4v4.5h6.5V4'), P('M8 20v-6h8v6')],
 'import': [P('M12 3.5v11'), P('M7.5 10.5 12 15l4.5-4.5'), P('M4 16v3a1.5 1.5 0 0 0 1.5 1.5h13A1.5 1.5 0 0 0 20 19v-3')],
 'export-image': [P('M12 6H5A1.5 1.5 0 0 0 3.5 7.5v11A1.5 1.5 0 0 0 5 20h11a1.5 1.5 0 0 0 1.5-1.5V12'), P('M3.5 16.5l4-4 6 6'), P('M15.5 3.5h5v5'), P('M20.5 3.5l-6 6')],
 'camera': [P('M4.5 7.5h3L9 5h6l1.5 2.5h3A1.5 1.5 0 0 1 21 9v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18V9a1.5 1.5 0 0 1 1.5-1.5z'), C(12, 13, 3.5)],
 'undock': [P('M10 6H5.5A1.5 1.5 0 0 0 4 7.5v11A1.5 1.5 0 0 0 5.5 20h11a1.5 1.5 0 0 0 1.5-1.5V14'), P('M14 4h6v6'), P('M20 4l-8 8')],
 # --- run ---
 'run': [P('M7.5 4.5v15L20 12z')],
 'stop': [R(5.5, 5.5, 13, 13, 2)],
 # --- view ---
 'select': [P('M5 3.5l5.4 15.2 2.3-6.2 6.2-2.3z')],
 'pan': [P('M12 3v18M3 12h18'), P('M9.5 5.5 12 3l2.5 2.5M9.5 18.5 12 21l2.5-2.5M5.5 9.5 3 12l2.5 2.5M18.5 9.5 21 12l-2.5 2.5')],
 'orbit': [C(12, 12, 2.5), P('M19.9 10.5a8 8 0 1 0-.9 5.4'), P('M20.5 5.5v5h-5')],
 'zoom': [C(10.5, 10.5, 6.5), P('M15.5 15.5l5 5'), P('M10.5 8v5M8 10.5h5')],
 'fit': [P('M4 8.5v-3A1.5 1.5 0 0 1 5.5 4h3M15.5 4h3A1.5 1.5 0 0 1 20 5.5v3M20 15.5v3a1.5 1.5 0 0 1-1.5 1.5h-3M8.5 20h-3A1.5 1.5 0 0 1 4 18.5v-3'), R(8.5, 8.5, 7, 7, 1)],
 'view': [P('M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z'), C(12, 12, 3)],
 'wireframe': [P('M12 3l8 4.5v9L12 21l-8-4.5v-9z'), P('M4 7.5l8 4.5 8-4.5M12 12v9')],
 'surface': [P('M12 3l8 4.5-8 4.5-8-4.5z', **SOFT), P('M12 3l8 4.5v9L12 21l-8-4.5v-9z'), P('M4 7.5l8 4.5 8-4.5M12 12v9')],
 # --- sketch ---
 'geometry': [P('M5 18.5V8.5l7-4 7 5.5v8.5z'), dot(5, 18.5), dot(5, 8.5), dot(12, 4.5), dot(19, 10), dot(19, 18.5)],
 'sketch': [P('M4 20l1-4.5L16.5 4a2.1 2.1 0 0 1 3 3L8 18.5z'), P('M14.5 6l3.5 3.5')],
 'line': [P('M6.5 17.5l11-11'), C(5, 19, 2), C(19, 5, 2)],
 'rectangle': [R(4.5, 6.5, 15, 11, 0.5), dot(4.5, 6.5), dot(19.5, 17.5)],
 'circle': [C(12, 12, 8), dot(12, 12, 1.5)],
 'radius': [C(12, 12, 8), P('M12 12l5.6-5.6'), dot(12, 12, 1.5)],
 'nozzle': [P('M3 6c5 0 6 3.5 9 3.5S17 7 21 5'), P('M3 18c5 0 6-3.5 9-3.5s5 2.5 9 4.5'), P('M3 12h18', **DASH)],
 'vertex-add': [P('M4 19.5l5.5-9 5 4.5'), C(9.5, 10.5, 2), P('M18 4v6M15 7h6')],
 'vertex-move': [C(12, 12, 2.2), P('M12 3v5M12 16v5M3 12h5M16 12h5'), P('M9.5 5.5 12 3l2.5 2.5M9.5 18.5 12 21l2.5-2.5M5.5 9.5 3 12l2.5 2.5M18.5 9.5 21 12l-2.5 2.5')],
 'segment-split': [P('M3.5 12h5.5M15 12h5.5'), C(12, 12, 2.5), P('M12 4.5v3M12 16.5v3')],
 'profile-close': [P('M9 19.5H5.5v-11l6.5-4 6.5 4v11H15'), P('M9 19.5h6', **DASH), dot(9, 19.5, 1.6), dot(15, 19.5, 1.6)],
 'dimension': [P('M4 5v14M20 5v14'), P('M4 12h16'), P('M7.5 9 4.5 12l3 3M16.5 9l3 3-3 3')],
 'fillet': [P('M5 20v-8a7 7 0 0 1 7-7h8'), P('M5 10V5h5', **DASH)],
 'revolve': [P('M12 2.500v19', **DASH), P('M12 6.500h3.500a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H12'), P('M12 6.500H8.500a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2H12', **DASH)],
 'coordinates': [P('M5 4v15h15'), P('M2.500 6.500 5 4l2.500 2.500M17.500 16.500 20 19l-2.500 2.500'), dot(14, 10, 2.1)],
 # --- mesh and physics ---
 'mesh': [R(4, 4, 16, 16, 1), P('M4 9.330h16M4 14.670h16M9.330 4v16M14.670 4v16')],
 'mesh-cut': [P('M4 4v16h16z'), P('M4 12h8v8M4 16h12M8 8v12')],
 'boundary': [P('M9 5h10v14H9', **DASH), P('M5 4v16', stroke_width='3')],
 'material': [P('M12 3.500l8.500 4.500-8.500 4.500L3.500 8z'), P('M3.500 12l8.500 4.500 8.500-4.500'), P('M3.500 16l8.500 4.500 8.500-4.500')],
 'solver': [R(6, 6, 12, 12), R(10, 10, 4, 4, 0.5), P('M9.500 3v3M14.500 3v3M9.500 18v3M14.500 18v3M3 9.500h3M3 14.500h3M18 9.500h3M18 14.500h3')],
 'fluent': [P('M3 7.500c3-3 6 3 9 0s6-3 9 0'), P('M3 12.500c3-3 6 3 9 0s6-3 9 0'), P('M3 17.500c3-3 6 3 9 0s6-3 9 0')],
 # --- results ---
 'chart': [P('M4 4v16h16'), P('M7.500 15.500l3.500-4 3 2.500 5-6.500')],
 'histogram': [P('M3.500 20.500h17'), R(5, 12, 3.5, 8.5, 0.5), R(10.25, 5.5, 3.5, 15, 0.5), R(15.5, 9, 3.5, 11.5, 0.5)],
 'probe': [C(12, 12, 6), P('M12 2.500v5M12 16.500v5M2.500 12h5M16.500 12h5'), dot(12, 12, 1.4)],
 'slice': [P('M8 5.500l8-2.500v15.500l-8 2.500z', **SOFT), P('M8 5.500l8-2.500v15.500l-8 2.500z'), P('M2.500 12h5.500M16 12h5.500')],
 'vectors': [P('M3.500 6.500h11'), P('M11.500 3.500l3 3-3 3'), P('M3.500 12h17'), P('M17.500 9l3 3-3 3'), P('M3.500 17.500h8'), P('M8.500 14.500l3 3-3 3')],
}

HEAD = '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round">'
if __name__ == '__main__':
    # Always writes next to the sources it belongs to: no path comes from the command line.
    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'resources', 'icons', 'ui')
    for name, parts in sorted(I.items()):
        with open(os.path.join(out, name + '.svg'), 'w') as handle:
            handle.write(HEAD + ''.join(parts) + '</svg>\n')
    print(len(I))
