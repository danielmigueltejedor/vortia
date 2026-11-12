import SceneKit
import SwiftUI
import VortiaKit

/// The domain's surface in 3D, coloured by the field: orbit with the trackpad, zoom with a
/// pinch. Drawn by SceneKit from the triangles the engine sends, with no VTK in the app.
struct FieldScene: View {
    var surface: FieldTriangles
    @State private var scene: SCNScene?
    @State private var builtFor: FieldTriangles?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let scene {
                SceneView(
                    scene: scene, pointOfView: scene.rootNode.childNode(withName: "camera", recursively: false),
                    options: [.allowsCameraControl, .autoenablesDefaultLighting])
                .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            if let range = surface.range, range.count == 2 {
                ColorLegend(range: range[0]...max(range[1], range[0])).frame(maxWidth: 360)
            }
        }
        .task(id: surface) {
            guard builtFor != surface else { return }
            scene = FieldScene.makeScene(surface)
            builtFor = surface
        }
    }

    /// Unindexed triangles become one geometry with a colour per vertex and a flat normal per
    /// face. The model is centred and scaled to a unit box, so any domain frames the same way.
    static func makeScene(_ surface: FieldTriangles) -> SCNScene {
        let scene = SCNScene()
        let count = surface.positions.count / 3
        guard count >= 3 else { return scene }

        var low = SIMD3<Float>(repeating: .infinity)
        var high = SIMD3<Float>(repeating: -.infinity)
        for v in 0..<count {
            let p = SIMD3(surface.positions[v * 3], surface.positions[v * 3 + 1], surface.positions[v * 3 + 2])
            low = pointwiseMin(low, p)
            high = pointwiseMax(high, p)
        }
        let centre = (low + high) / 2
        let extent = max((high - low).max(), 1e-12)

        let range = surface.range ?? [0, 1]
        let span = max(Float(range[safe: 1] - range[safe: 0]), 1e-30)
        let coloured = surface.range != nil
        var vertices: [SCNVector3] = []
        var normals: [SCNVector3] = []
        var colours: [Float] = []
        vertices.reserveCapacity(count)
        normals.reserveCapacity(count)
        colours.reserveCapacity(count * 4)
        for t in 0..<(count / 3) {
            var corner: [SIMD3<Float>] = []
            for k in 0..<3 {
                let v = t * 3 + k
                let p = SIMD3(surface.positions[v * 3], surface.positions[v * 3 + 1], surface.positions[v * 3 + 2])
                corner.append((p - centre) / extent)
            }
            let n = simd_normalize(simd_cross(corner[1] - corner[0], corner[2] - corner[0]))
            let normal = n.x.isFinite ? n : SIMD3<Float>(0, 0, 1)
            for k in 0..<3 {
                vertices.append(SCNVector3(corner[k].x, corner[k].y, corner[k].z))
                normals.append(SCNVector3(normal.x, normal.y, normal.z))
                let value = surface.values[safe: t * 3 + k]
                let (r, g, b) =
                    coloured
                    ? Colormap.rgb(Double((value - Float(range[safe: 0])) / span))
                    : (0.62, 0.66, 0.72)
                colours.append(contentsOf: [Float(r), Float(g), Float(b), 1])
            }
        }

        let colourData = colours.withUnsafeBufferPointer { Data(buffer: $0) }
        let colourSource = SCNGeometrySource(
            data: colourData, semantic: .color, vectorCount: vertices.count,
            usesFloatComponents: true, componentsPerVector: 4,
            bytesPerComponent: MemoryLayout<Float>.size, dataOffset: 0,
            dataStride: MemoryLayout<Float>.size * 4)
        let indices = (0..<Int32(vertices.count)).map { $0 }
        let element = SCNGeometryElement(indices: indices, primitiveType: .triangles)
        let geometry = SCNGeometry(
            sources: [SCNGeometrySource(vertices: vertices), SCNGeometrySource(normals: normals), colourSource],
            elements: [element])
        let material = SCNMaterial()
        material.lightingModel = .lambert
        material.isDoubleSided = true
        geometry.materials = [material]
        scene.rootNode.addChildNode(SCNNode(geometry: geometry))

        let camera = SCNNode()
        camera.name = "camera"
        camera.camera = SCNCamera()
        camera.camera?.zNear = 0.01
        camera.position = SCNVector3(1.1, 0.9, 1.6)
        camera.look(at: SCNVector3(0, 0, 0))
        scene.rootNode.addChildNode(camera)
        return scene
    }
}

extension Array where Element == Float {
    subscript(safe index: Int) -> Float {
        indices.contains(index) ? self[index] : 0
    }
}
