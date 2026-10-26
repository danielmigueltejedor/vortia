"""Conformal CAD partitions and reproducible region-specific wedge meshes.

Preview and final generation use this same pipeline. Cuts are internal mesh
interfaces, never physical wall patches. All coordinates in settings are mm.
"""
import hashlib
import json
import math


def number(value, positive=False):
    if type(value) not in (int, float) or not math.isfinite(value) or (positive and value <= 0):
        raise ValueError('Mesh coordinates and sizes must be finite; sizes must be positive')
    return value


def point(value):
    if not isinstance(value, list) or len(value) != 2:
        raise ValueError('A mesh point needs X and Y coordinates in mm')
    return [number(x) for x in value]


def validate_mesh(sketch, mesh):
    pts = sketch.get('vertices', [])
    if not sketch.get('closed') or sketch.get('units') != 'mm' or not 3 <= len(pts) <= 512:
        raise ValueError('Use one closed fluid contour in millimetres')
    for p in pts:
        point(p)
        if p[1] < 0: raise ValueError('The fluid contour must lie above the axis Y=0')
    size, wall = number(mesh.get('sizeMm'), True), number(mesh.get('wallSizeMm'), True)
    angle = number(mesh.get('wedgeAngle'), True)
    if wall > size or not .1 <= angle <= 5:
        raise ValueError('Use a wall size no larger than the global size and a wedge angle from 0.1 to 5 degrees')
    cuts, regions = mesh.get('partitions', []), mesh.get('regions', [])
    if not isinstance(cuts, list) or len(cuts) > 32 or not isinstance(regions, list) or len(regions) > 128:
        raise ValueError('At most 32 partition cuts and 128 region controls are supported')
    for cut in cuts:
        a, b = point(cut['from']), point(cut['to'])
        if math.dist(a, b) < 1e-6: raise ValueError('Partition endpoints must be distinct')
    perimeter = sum(math.dist(pts[i], pts[(i+1) % len(pts)]) for i in range(len(pts)))
    extent = max(max(p[i] for p in pts)-min(p[i] for p in pts) for i in (0, 1))
    for region in regions:
        point(region['seed']); local = number(region['sizeMm'], True)
        if region.get('method', 'triangles') not in ('triangles', 'quads'):
            raise ValueError('Unsupported regional mesh method')
        if not isinstance(region.get('name', ''), str) or len(region.get('name', '')) > 120:
            raise ValueError('Mesh region names must contain at most 120 characters')
        if extent/local > 20000: raise ValueError('A regional cell size is excessive for this domain')
    if extent/size > 2000 or perimeter/wall > 100000:
        raise ValueError('The requested mesh resolution is excessive; increase the cell sizes')
    if mesh.get('engine', 'occ-regions-v1') != 'occ-regions-v1':
        raise ValueError('Unsupported regional mesh engine')


def outline(gmsh, surface):
    remaining = []
    for _, curve in gmsh.model.getBoundary([(2, surface)], oriented=False):
        low, high = gmsh.model.getParametrizationBounds(1, curve)
        count = 1 if gmsh.model.getType(1, curve) == 'Line' else 48
        coordinates = gmsh.model.getValue(1, curve, [float(low[0])+(float(high[0])-float(low[0]))*i/count for i in range(count+1)])
        remaining.append([[float(coordinates[3*i])*1000, float(coordinates[3*i+1])*1000] for i in range(count+1)])
    loops = []
    while remaining:
        loop = remaining.pop()
        while math.dist(loop[-1], loop[0]) > 1e-5:
            if not remaining: raise ValueError('Cannot reconstruct the partition boundary')
            candidates = [(math.dist(loop[-1], edge[end]), i, end) for i, edge in enumerate(remaining) for end in (0, -1)]
            distance, index, end = min(candidates)
            if distance > 1e-5: raise ValueError('Partition boundaries are disconnected')
            edge = remaining.pop(index)
            if end == -1: edge.reverse()
            loop.extend(edge[1:])
        loops.append(loop)
    return loops


def repair_axis_quads(gmsh, faces, top_for, volume_for, angle):
    """Split only quad columns that would have seven or fewer than six vertices.

    A quad touching the axis at one vertex revolves to a seven-vertex solid,
    unsupported by MSH2 hex/prism/pyramid types. Two planar triangles give two
    conformal pyramids instead. All lateral interfaces and BC tags are retained.
    """
    if not any(len(gmsh.model.mesh.getElementsByType(3, face)[0]) for face in faces): return 0
    tags, xyz, _ = gmsh.model.mesh.getNodes()
    coords = {int(t): [float(x) for x in xyz[3*i:3*i+3]] for i, t in enumerate(tags)}
    key = lambda p: tuple(round(x, 10) for x in p)
    node_at = {key(p): t for t, p in coords.items()}
    def back(t):
        x, y, _ = coords[t]
        return node_at[key([x, y*math.cos(angle), y*math.sin(angle)])]
    def determinant(a, b, c, d):
        u = [coords[b][i]-coords[a][i] for i in range(3)]
        v = [coords[c][i]-coords[a][i] for i in range(3)]
        w = [coords[d][i]-coords[a][i] for i in range(3)]
        return (u[1]*v[2]-u[2]*v[1])*w[0]+(u[2]*v[0]-u[0]*v[2])*w[1]+(u[0]*v[1]-u[1]*v[0])*w[2]
    repaired = 0
    for face in faces:
        volume = volume_for[face]
        hex_tags, hex_nodes = gmsh.model.mesh.getElementsByType(5, volume)
        hexes = {frozenset(int(t) for t in hex_nodes[8*i:8*i+8]): int(tag) for i, tag in enumerate(hex_tags)}
        back_tags, back_nodes = gmsh.model.mesh.getElementsByType(3, top_for[face])
        backs = {frozenset(int(t) for t in back_nodes[4*i:4*i+4]): int(tag) for i, tag in enumerate(back_tags)}
        quad_tags, quad_nodes = gmsh.model.mesh.getElementsByType(3, face)
        for index, tag in enumerate(quad_tags):
            quad = [int(t) for t in quad_nodes[4*index:4*index+4]]
            axis = [i for i, t in enumerate(quad) if math.hypot(coords[t][1], coords[t][2]) < 1e-10]
            if len(axis) not in (1, 3): continue
            candidates = [((0,1,2),(0,2,3)), ((0,1,3),(1,2,3))]
            if len(axis) == 1: candidates = [candidates[axis[0] % 2]]
            triangles = None
            for candidate in candidates:
                test = [[quad[i] for i in tri] for tri in candidate]
                if all(abs((coords[t[1]][0]-coords[t[0]][0])*(coords[t[2]][1]-coords[t[0]][1])-(coords[t[1]][1]-coords[t[0]][1])*(coords[t[2]][0]-coords[t[0]][0])) > 1e-20 for t in test): triangles = test; break
            if triangles is None: raise ValueError('A quadrilateral degenerates on the axis; refine or use triangles')
            opposite = [back(t) for t in quad]
            solid = hexes.get(frozenset(quad+opposite)); top = backs.get(frozenset(opposite))
            if solid is None or top is None: raise ValueError('Cannot safely split an axis quadrilateral')
            gmsh.model.mesh.removeElements(3, volume, [solid]); gmsh.model.mesh.removeElements(2, face, [int(tag)]); gmsh.model.mesh.removeElements(2, top_for[face], [top])
            for triangle in triangles:
                other = [back(t) for t in triangle]
                gmsh.model.mesh.addElementsByType(face, 2, [], triangle)
                gmsh.model.mesh.addElementsByType(top_for[face], 2, [], other)
                axial = [t for t in triangle if t == back(t)]; radial = [t for t in triangle if t != back(t)]
                if len(axial) == 1:
                    a, b = radial; nodes = [a, b, back(b), back(a), axial[0]]
                    if determinant(nodes[0],nodes[1],nodes[3],nodes[4]) < 0: nodes = [nodes[0],nodes[3],nodes[2],nodes[1],nodes[4]]
                    gmsh.model.mesh.addElementsByType(volume, 7, [], nodes)
                elif len(axial) == 2:
                    nodes = [axial[0],axial[1],radial[0],back(radial[0])]
                    if determinant(*nodes) < 0: nodes[0],nodes[1] = nodes[1],nodes[0]
                    gmsh.model.mesh.addElementsByType(volume, 4, [], nodes)
                else: raise ValueError('Unsupported axis intersection')
            repaired += 1
    return repaired


def build(gmsh, root, sketch, cfg, preview=False):
    mesh = cfg['mesh']; validate_mesh(sketch, mesh)
    gmsh.model.add('partitioned_fluid')
    occ = gmsh.model.occ
    points = [[x/1000, y/1000] for x, y in sketch['vertices']]
    tags = [occ.addPoint(x, y, 0) for x, y in points]
    curves = []
    for i, a in enumerate(points):
        j = (i+1) % len(points); b = points[j]
        bulge = sketch.get('segmentBulges', [0]*len(points))[i]
        number(bulge)
        if abs(bulge) > 1 or math.dist(a, b) < 1e-9: raise ValueError('Invalid boundary arc or coincident vertices')
        if abs(bulge) < 1e-14: curve = occ.addLine(tags[i], tags[j])
        else:
            k = (1-bulge*bulge)/(4*bulge)
            cx, cy = (a[0]+b[0])/2-(b[1]-a[1])*k, (a[1]+b[1])/2+(b[0]-a[0])*k
            radius = math.hypot(a[0]-cx, a[1]-cy); start = math.atan2(a[1]-cy, a[0]-cx); sweep = 4*math.atan(bulge)
            mid = occ.addPoint(cx+radius*math.cos(start+sweep/2), cy+radius*math.sin(start+sweep/2), 0)
            curve = occ.addCircleArc(tags[i], mid, tags[j], center=False)
        curves.append(curve)
    surface = occ.addPlaneSurface([occ.addCurveLoop(curves)])
    cuts = [occ.addLine(occ.addPoint(*[v/1000 for v in cut['from']], 0), occ.addPoint(*[v/1000 for v in cut['to']], 0)) for cut in mesh.get('partitions', [])]
    parents = {curve: i for i, curve in enumerate(curves)}
    if cuts:
        entities, mapping = occ.fragment([(1, t) for t in curves]+[(2, surface)], [(1, t) for t in cuts])
        faces = sorted({tag for dim, tag in entities if dim == 2})
        parents = {tag: i for i, children in enumerate(mapping[:len(curves)]) for dim, tag in children if dim == 1}
        occ.synchronize()
        for index, children in enumerate(mapping[len(curves)+1:]):
            if not any(len(gmsh.model.getAdjacencies(1, tag)[0]) >= 2 for dim, tag in children if dim == 1):
                raise ValueError(f'Partition {index+1} does not divide a face; join the contour or another partition')
    else:
        faces = [surface]; occ.synchronize()
    if len(faces) > 128: raise ValueError('Too many mesh regions')
    outlines = {face: outline(gmsh, face) for face in faces}
    assignments = {}; matched = set()
    for face in faces:
        controls = [(i, r) for i, r in enumerate(mesh.get('regions', [])) if gmsh.model.isInside(2, face, [r['seed'][0]/1000, r['seed'][1]/1000, 0])]
        if len(controls) > 1:
            if not preview: raise ValueError('Several saved mesh controls now refer to one region; update the partition controls')
            controls = []
        if controls: matched.add(controls[0][0]); assignments[face] = dict(controls[0][1])
        else: assignments[face] = dict(sizeMm=mesh['sizeMm'], method='triangles', name='')
    estimate = sum(occ.getMass(2, face)/(control['sizeMm']/1000)**2 for face, control in assignments.items())
    if estimate > (200000 if preview else 1000000):
        raise ValueError('The estimated mesh exceeds the cell budget; increase regional sizes')
    angle = math.radians(mesh['wedgeAngle'])
    # Revolve all faces together so shared curves become shared volume faces.
    # Keep CAD coordinates planar until meshing: OCC transforms can renumber edges.
    occ.revolve([(2, f) for f in faces], 0, 0, 0, 1, 0, 0, angle, [1], [], True)
    occ.synchronize()
    # OCC can omit already-shared swept volumes from the returned dimTags when
    # several cuts meet at a vertex ("extrusion layer cycle"). The synchronized
    # model contains the complete topology; this fresh model has no other solids.
    volumes = sorted(tag for _, tag in gmsh.model.getEntities(3))
    if len(volumes) != len(faces):
        raise ValueError('The partition sweep did not create one volume per region')
    physical = {'wedgeFront': faces, 'wedgeBack': []}; top_for = {}; volume_for = {}; internal = set()
    for volume in volumes:
        borders = [tag for _, tag in gmsh.model.getBoundary([(3, volume)], oriented=False)]
        fronts = [f for f in faces if f in borders]
        if len(fronts) != 1 or fronts[0] in volume_for:
            raise ValueError('The partition sweep has ambiguous region boundaries')
        front = fronts[0]
        volume_for[front] = volume
        for tag in borders:
            if tag in faces: continue
            if len(gmsh.model.getAdjacencies(2, tag)[0]) == 2:
                internal.add(tag); continue
            _, y, z = occ.getCenterOfMass(2, tag)
            if abs(math.atan2(z, y)-angle) < 1e-7:
                physical['wedgeBack'].append(tag); top_for[front] = tag; continue
            edges = {t for _, t in gmsh.model.getBoundary([(2, tag)], oriented=False)}
            originals = {parents[t] for t in edges if t in parents}
            if len(originals) != 1: raise ValueError('Cannot retain an exterior boundary assignment after partitioning')
            index = originals.pop(); kind = cfg['boundaries'][index]['type']
            if kind != 'axis': physical.setdefault(cfg['boundaries'][index]['name'], []).append(tag)
    if len(top_for) != len(faces): raise ValueError('A wedge region is missing its opposite face')
    for name, surfaces in physical.items(): gmsh.model.addPhysicalGroup(2, sorted(set(surfaces)), name=name)
    gmsh.model.addPhysicalGroup(3, volumes, name='fluid')
    fields = []
    maximum = max([mesh['sizeMm']]+[r['sizeMm'] for r in assignments.values()])/1000
    minimum = min([mesh['wallSizeMm']]+[r['sizeMm'] for r in assignments.values()])/1000
    for face, control in assignments.items():
        constant = gmsh.model.mesh.field.add('MathEval'); gmsh.model.mesh.field.setString(constant, 'F', str(control['sizeMm']/1000))
        restricted = gmsh.model.mesh.field.add('Restrict'); gmsh.model.mesh.field.setNumber(restricted, 'InField', constant)
        gmsh.model.mesh.field.setNumbers(restricted, 'SurfacesList', [face, top_for[face]])
        edges = sorted({tag for f in (face, top_for[face]) for _, tag in gmsh.model.getBoundary([(2, f)], oriented=False)})
        gmsh.model.mesh.field.setNumbers(restricted, 'CurvesList', edges); fields.append(restricted)
        if control.get('method') == 'quads':
            gmsh.model.mesh.setRecombine(2, face); gmsh.model.mesh.setRecombine(2, top_for[face])
    walls = [tag for tag, parent in parents.items() if cfg['boundaries'][parent]['type'] == 'wall']
    if walls:
        distance = gmsh.model.mesh.field.add('Distance'); gmsh.model.mesh.field.setNumbers(distance, 'CurvesList', walls); gmsh.model.mesh.field.setNumber(distance, 'Sampling', 100)
        threshold = gmsh.model.mesh.field.add('Threshold')
        for key, value in dict(InField=distance, SizeMin=mesh['wallSizeMm']/1000, SizeMax=maximum, DistMin=minimum, DistMax=10*maximum).items(): gmsh.model.mesh.field.setNumber(threshold, key, value)
        fields.append(threshold)
    combined = gmsh.model.mesh.field.add('Min'); gmsh.model.mesh.field.setNumbers(combined, 'FieldsList', fields); gmsh.model.mesh.field.setAsBackgroundMesh(combined)
    for key, value in {'Mesh.MeshSizeMin': minimum, 'Mesh.MeshSizeMax': maximum, 'Mesh.MeshSizeFromPoints': 0, 'Mesh.MeshSizeFromCurvature': 0, 'Mesh.MshFileVersion': 2.2, 'General.NumThreads': 1, 'Mesh.RandomSeed': 1}.items(): gmsh.option.setNumber(key, value)
    gmsh.model.mesh.generate(3)
    axis_splits = repair_axis_quads(gmsh, faces, top_for, volume_for, angle)
    count = sum(len(tags) for tags in gmsh.model.mesh.getElements(3)[1])
    if count > (200000 if preview else 1000000): raise ValueError('The mesh exceeds the preview limit (200,000 cells) or final limit (1,000,000 cells); increase cell sizes')
    node_tags, coordinates, _ = gmsh.model.mesh.getNodes(); node_map = {int(tag): i for i, tag in enumerate(node_tags)}
    nodes = [[float(coordinates[3*i])*1000, math.hypot(float(coordinates[3*i+1]), float(coordinates[3*i+2]))*1000] for i in range(len(node_tags))]
    cells = []; regions = []
    for index, face in enumerate(faces):
        control = assignments[face]; local = []; triangles = quads = 0
        types, _, connectivities = gmsh.model.mesh.getElements(2, face)
        for kind, connectivity in zip(types, connectivities):
            if kind not in (2, 3): raise ValueError('Unsupported preview element type')
            width = 3 if kind == 2 else 4
            for start in range(0, len(connectivity), width): local.append([node_map[int(t)] for t in connectivity[start:start+width]])
            if kind == 2: triangles += len(connectivity)//width
            else: quads += len(connectivity)//width
        if not local: raise ValueError('A region has no mesh elements')
        seed = [sum(nodes[n][axis] for n in local[0])/len(local[0]) for axis in (0, 1)]
        identifier = control.get('id') or 'region_'+hashlib.sha256(json.dumps(outlines[face], sort_keys=True).encode()).hexdigest()[:12]
        region = dict(id=identifier, name=control.get('name') or f'Region {index+1}', automaticName=not bool(control.get('name')), seed=control.get('seed', seed), sizeMm=control['sizeMm'], method=control.get('method', 'triangles'), loops=outlines[face], triangles=triangles, quads=quads, cells=len(local))
        regions.append(region); cells.extend([[index, *cell] for cell in local])
    used = sorted({n for cell in cells for n in cell[1:]}); compact = {n: i for i, n in enumerate(used)}
    nodes = [nodes[n] for n in used]; cells = [[cell[0], *[compact[n] for n in cell[1:]]] for cell in cells]
    result = dict(schema=1, engine='occ-regions-v1', regions=regions, nodes=nodes, cells=cells, volumeCells=count, internalInterfaces=len(internal), axisQuadSplits=axis_splits, wedgeAngle=mesh['wedgeAngle'], unmatchedControls=[r for i, r in enumerate(mesh.get('regions', [])) if i not in matched])
    (root/'mesh-preview.json').write_text(json.dumps(result, separators=(',', ':'))+'\n')
    report = dict(mesher='Gmsh', version=gmsh.__version__, cells=count, wedgeAngle=mesh['wedgeAngle'], regions=regions, internalInterfaces=len(internal), axisQuadSplits=axis_splits, engine='occ-regions-v1', scope='mesh generation only; quality checked by OpenFOAM')
    (root/'mesh-report.json').write_text(json.dumps(report, indent=2)+'\n')
    if not preview:
        # Center the wedge on the no-swirl XY plane, matching the existing solver.
        c, s = math.cos(-angle/2), math.sin(-angle/2)
        gmsh.model.mesh.affineTransform([1,0,0,0, 0,c,-s,0, 0,s,c,0, 0,0,0,1])
        gmsh.write(str(root/'mesh.msh'))
        occ.rotate([(3, v) for v in volumes], 0, 0, 0, 1, 0, 0, -angle/2); occ.synchronize()
        gmsh.write(str(root/'geometry/domain.brep'))
    return result
