import Foundation

/// The interface's words, in the two languages Vortia ships. Anything but Spanish gets English.
public enum L10n {
    public enum Key: String, CaseIterable, Sendable {
        case appName, edition, editionBody
        case newProject, openProject, saveProject, welcomeTitle, welcomeBody
        case geometry, mesh, physics, time, run, results
        case domain, minimum, maximum, size, cells, grading, patches, patchKind, totalCells
        case fluid, viscosity, solver, transient, steady, turbulence, laminar, kOmegaSST
        case intensity, lengthScale, boundaries, velocity, pressure, initialField
        case startTime, endTime, timeStep, writeInterval, adjustTimeStep, maxCourant
        case numerics, timeScheme, correctors, toleranceP, toleranceU
        case checks, noIssues, reynolds, courant, openFoam, openFoamMissing, detectAgain
        case runMesh, runSolve, stop, idle, meshing, solving, finished, failed
        case residuals, simulatedTime, log, noResults, noResultsBody, field, statistics
        case mean, standardDeviation, slice, axis, position, distance, value, view, threeD
        case engineStarting, engineLost, engineMissing, dismiss
        case nozzle, inletDiameter, throatDiameter, outletDiameter
        case convergingLength, divergingLength, makeNozzle, nozzleInvalid, segment, length, onAxis
        case curvature, straighten, bendOut, bendIn, contourHint
        case inlet, outlet, axisBoundary, reservoir, unassigned, wall
        case globalSize, wallSize, wedgeAngle, gasModel, flowModel, inviscid, molecularWeight
        case specificHeat, dynamicViscosity, conductivity, boundaryHint, initialPressure
        case initialTemperature, calculation, automaticStop, iterationLimit, saveEvery
        case acousticCourant, processes, processesHint, advancedCriteria, checkEvery
        case minimumIterations, stableChecks, massTolerance, fieldTolerance, equationTolerance
        case regionName, pressurePa, temperatureK, viscosityRatio, convergence, iteration
        case massError, localMassError, fieldChange, updateRate, compressibleTitle, compressibleBody
        case engineering, reportName, centre, direction, references, kinematicPressure
        case useDensityField, constantDensity, referencePressure, referenceDensity, referenceSpeed
        case referenceArea, analyse, reportNote
        case project, inspector, recentProjects, openRecent, clearRecents, stagesMenu, simulationMenu, toggleInspector, nextStep, guideProject, guideGeometry, guideMesh
        case guidePhysics, guideCalculation, guideResults, engineAndSolver, notFound, resultData, meshReady, meshPending, openBoundaries, toolSelect
        case toolAddPoint, toolPan, toolSelectHint, toolAddPointHint, toolPanHint, zoomIn, zoomOut, fitView, undo, redo
        case vertices, vertex, segments, perimeter, closedProfile, openProfile, coordinateX, coordinateY, deleteVertex, splitSegment
        case nozzleReplaces, meshHint, generateMesh, meshSketchNote, estimate, converged, fieldPressure, fieldSpeed, fieldTemperature, fieldMach
        case fieldDensity, fieldTurbulentViscosity, fieldTurbulentEnergy, goToCalculation, sections, planeXY, planeXZ, planeYZ, mirrorAxis, probe
        case probeHelp, saveImage, profileShort, analysisShort, histogram, profileLine, alongAxis, verticalCentre
    }

    private static let english: [Key: String] = [
        .appName: "Vortia", .edition: "Vortia Native",
        .editionBody:
            "The macOS edition, built with SwiftUI. Vortia Nodalia, the other edition, can stay installed beside it.",
        .newProject: "New Project…", .openProject: "Open Project…", .saveProject: "Save",
        .welcomeTitle: "Engineering the flow",
        .welcomeBody: "Create a project to start from a nozzle you can reshape at will, or open one you already have.",
        .geometry: "Geometry", .mesh: "Mesh", .physics: "Physics", .time: "Time and numerics",
        .run: "Calculation", .results: "Results",
        .domain: "Domain", .minimum: "Minimum", .maximum: "Maximum", .size: "Size",
        .cells: "Cells", .grading: "Grading", .patches: "Boundary patches", .patchKind: "Type",
        .totalCells: "Total cells",
        .fluid: "Fluid", .viscosity: "Kinematic viscosity (m²/s)", .solver: "Solver",
        .transient: "Transient (icoFoam)", .steady: "Steady (simpleFoam)",
        .turbulence: "Turbulence", .laminar: "Laminar", .kOmegaSST: "k-ω SST",
        .intensity: "Intensity", .lengthScale: "Length scale (m)", .boundaries: "Boundary conditions",
        .velocity: "Velocity", .pressure: "Pressure", .initialField: "Initial field",
        .startTime: "Start (s)", .endTime: "End (s)", .timeStep: "Time step (s)",
        .writeInterval: "Write every (steps)", .adjustTimeStep: "Adjust the time step",
        .maxCourant: "Maximum Courant", .numerics: "Numerics", .timeScheme: "Time scheme",
        .correctors: "PISO correctors", .toleranceP: "Pressure tolerance",
        .toleranceU: "Velocity tolerance",
        .checks: "Checks", .noIssues: "The case is ready to run.", .reynolds: "Reynolds",
        .courant: "Courant", .openFoam: "OpenFOAM",
        .openFoamMissing: "OpenFOAM was not found. Vortia Native carries its own; if this persists, reinstall the app.",
        .detectAgain: "Detect again",
        .runMesh: "Mesh", .runSolve: "Solve", .stop: "Stop", .idle: "Ready",
        .meshing: "Meshing…", .solving: "Solving…", .finished: "Finished", .failed: "Stopped",
        .residuals: "Residuals", .simulatedTime: "Simulated time", .log: "Log",
        .noResults: "No results yet", .noResultsBody: "Solve the case to see its fields here.",
        .field: "Field", .statistics: "Statistics", .mean: "Mean",
        .standardDeviation: "Standard deviation", .slice: "Slice", .axis: "Normal axis",
        .position: "Position", .distance: "Distance (m)", .value: "Value", .view: "View", .threeD: "3D surface",
        .engineStarting: "Starting the engine…", .engineLost: "The engine stopped",
        .engineMissing: "The engine is not running.", .dismiss: "Dismiss", .nozzle: "Parametric nozzle",
        .inletDiameter: "Inlet diameter (mm)", .throatDiameter: "Throat diameter (mm)",
        .outletDiameter: "Outlet diameter (mm)", .convergingLength: "Converging length (mm)",
        .divergingLength: "Diverging length (mm)", .makeNozzle: "Make the nozzle",
        .nozzleInvalid: "The throat must be the narrowest section, and every size positive.",
        .segment: "Segment", .length: "Length", .onAxis: "On Y = 0: this segment is the axis.",
        .curvature: "Curvature (bulge)", .straighten: "Straight", .bendOut: "Bend out", .bendIn: "Bend in",
        .contourHint: "Pick a vertex to type its coordinates, or a segment to bend or split it. X is the axis of rotation and Y the radius, in millimetres.",
        .inlet: "Total-pressure inlet", .outlet: "Pressure outlet",
        .axisBoundary: "Axis", .reservoir: "Open reservoir", .unassigned: "Unassigned", .wall: "Wall",
        .globalSize: "Global size (mm)", .wallSize: "Near-wall size (mm)", .wedgeAngle: "Wedge angle (°)",
        .gasModel: "Gas and model", .flowModel: "Flow model", .inviscid: "Inviscid (Euler · slip walls)",
        .molecularWeight: "Molecular weight (kg/kmol)", .specificHeat: "Specific heat Cp (J/kg/K)",
        .dynamicViscosity: "Dynamic viscosity (Pa s)", .conductivity: "Thermal conductivity (W/m/K)",
        .boundaryHint: "Pressures are absolute Pa. Inlet pressure and temperature are total values; outlet pressure is static.",
        .initialPressure: "Initial absolute pressure (Pa)", .initialTemperature: "Initial temperature (K)",
        .calculation: "Calculation", .automaticStop: "Stop automatically when the flow stabilizes",
        .iterationLimit: "Safety iteration limit", .saveEvery: "Save every N iterations",
        .acousticCourant: "Acoustic Courant limit", .processes: "CPU processes (0 = automatic)",
        .processesHint: "Parallel runs use an isolated working folder; small meshes may run faster with fewer processes.",
        .advancedCriteria: "Advanced convergence criteria", .checkEvery: "Check every N iterations",
        .minimumIterations: "Minimum iterations before acceptance", .stableChecks: "Consecutive successful checks",
        .massTolerance: "Relative mass error limit", .fieldTolerance: "Relative field change limit",
        .equationTolerance: "Normalized update rate limit", .regionName: "Region name",
        .pressurePa: "Pressure (Pa)", .temperatureK: "Temperature (K)", .viscosityRatio: "Viscosity ratio",
        .convergence: "Convergence", .iteration: "Iteration", .massError: "Global mass error",
        .localMassError: "Local mass error", .fieldChange: "Field change", .updateRate: "Update rate",
        .compressibleTitle: "Compressible flow",
        .compressibleBody: "For an axisymmetric nozzle or duct, draw its contour instead of a box. This project switches to the contour workflow.",
        .engineering: "Section analysis", .reportName: "Report name", .centre: "Centre",
        .direction: "Forward direction", .references: "Reference values",
        .kinematicPressure: "Pressure is kinematic (p/ρ, incompressible solvers)",
        .useDensityField: "Use the density field", .constantDensity: "Density (kg/m³)",
        .referencePressure: "Reference pressure (Pa)", .referenceDensity: "Reference density (kg/m³)",
        .referenceSpeed: "Reference speed (m/s)", .referenceArea: "Reference area (m²)",
        .analyse: "Analyse", .reportNote:
            "From the stored fields only. Convergence, model suitability and mesh independence need their own evidence.",
        .project: "Project",
        .inspector: "Inspector",
        .recentProjects: "Recent projects",
        .openRecent: "Open Recent",
        .clearRecents: "Clear Menu",
        .stagesMenu: "Stages",
        .simulationMenu: "Simulation",
        .toggleInspector: "Show or Hide the Inspector",
        .nextStep: "Next step",
        .guideProject: "Review the project",
        .guideGeometry: "Draw a closed fluid contour",
        .guideMesh: "Choose the cell sizes and mesh it",
        .guidePhysics: "Give every boundary its condition",
        .guideCalculation: "Run the calculation",
        .guideResults: "Explore the results",
        .engineAndSolver: "Engine and solver",
        .notFound: "Not found",
        .resultData: "Result data",
        .meshReady: "meshed",
        .meshPending: "not meshed yet",
        .openBoundaries: "open boundaries",
        .toolSelect: "Select",
        .toolAddPoint: "Add vertex",
        .toolPan: "Move the view",
        .toolSelectHint: "Drag vertices, or the background to move the view.",
        .toolAddPointHint: "Click on a segment to put a vertex there.",
        .toolPanHint: "Drag to move the view; pinch to zoom.",
        .zoomIn: "Zoom in",
        .zoomOut: "Zoom out",
        .fitView: "Fit the view",
        .undo: "Undo",
        .redo: "Redo",
        .vertices: "Vertices",
        .vertex: "Vertex",
        .segments: "Segments",
        .perimeter: "Perimeter",
        .closedProfile: "Closed profile",
        .openProfile: "Open profile",
        .coordinateX: "X coordinate (mm)",
        .coordinateY: "Y coordinate (mm)",
        .deleteVertex: "Delete the vertex",
        .splitSegment: "Split in two",
        .nozzleReplaces: "Replaces the contour; it can be undone.",
        .meshHint: "The near-wall size refines the cells along walls; the wedge angle is the slice of the axisymmetric domain that is meshed.",
        .generateMesh: "Generate the mesh",
        .meshSketchNote: "A sketch of the sizes, not the mesh itself",
        .estimate: "estimated count",
        .converged: "Converged",
        .fieldPressure: "Pressure",
        .fieldSpeed: "Speed",
        .fieldTemperature: "Temperature",
        .fieldMach: "Mach number",
        .fieldDensity: "Density",
        .fieldTurbulentViscosity: "Turbulent viscosity",
        .fieldTurbulentEnergy: "Turbulent kinetic energy",
        .goToCalculation: "Go to Calculation",
        .sections: "Sections",
        .planeXY: "Plane XY · side",
        .planeXZ: "Plane XZ · top",
        .planeYZ: "Plane YZ · front",
        .mirrorAxis: "Mirror about the axis",
        .probe: "Probe",
        .probeHelp: "Read the value under the pointer",
        .saveImage: "Save image",
        .profileShort: "Profile",
        .analysisShort: "Analysis",
        .histogram: "Histogram",
        .profileLine: "Sample along",
        .alongAxis: "The axis (X)",
        .verticalCentre: "The vertical centre line",
    ]

    private static let spanish: [Key: String] = [
        .appName: "Vortia", .edition: "Vortia Native",
        .editionBody:
            "La edición para macOS, hecha con SwiftUI. Vortia Nodalia, la otra edición, puede seguir instalada a su lado.",
        .newProject: "Nuevo proyecto…", .openProject: "Abrir proyecto…", .saveProject: "Guardar",
        .welcomeTitle: "Ingeniería del flujo",
        .welcomeBody: "Crea un proyecto para empezar con una tobera que puedes rehacer a tu medida, o abre uno que ya tengas.",
        .geometry: "Geometría", .mesh: "Malla", .physics: "Física", .time: "Tiempo y numérica",
        .run: "Cálculo", .results: "Resultados",
        .domain: "Dominio", .minimum: "Mínimo", .maximum: "Máximo", .size: "Tamaño",
        .cells: "Celdas", .grading: "Gradación", .patches: "Contornos", .patchKind: "Tipo",
        .totalCells: "Celdas en total",
        .fluid: "Fluido", .viscosity: "Viscosidad cinemática (m²/s)", .solver: "Solver",
        .transient: "Transitorio (icoFoam)", .steady: "Estacionario (simpleFoam)",
        .turbulence: "Turbulencia", .laminar: "Laminar", .kOmegaSST: "k-ω SST",
        .intensity: "Intensidad", .lengthScale: "Escala de longitud (m)",
        .boundaries: "Condiciones de contorno", .velocity: "Velocidad", .pressure: "Presión",
        .initialField: "Campo inicial",
        .startTime: "Inicio (s)", .endTime: "Fin (s)", .timeStep: "Paso de tiempo (s)",
        .writeInterval: "Guardar cada (pasos)", .adjustTimeStep: "Ajustar el paso de tiempo",
        .maxCourant: "Courant máximo", .numerics: "Numérica", .timeScheme: "Esquema temporal",
        .correctors: "Correctores PISO", .toleranceP: "Tolerancia de presión",
        .toleranceU: "Tolerancia de velocidad",
        .checks: "Comprobaciones", .noIssues: "El caso está listo para calcular.",
        .reynolds: "Reynolds", .courant: "Courant", .openFoam: "OpenFOAM",
        .openFoamMissing: "No se encontró OpenFOAM. Vortia Native trae el suyo; si sigue así, reinstala la app.",
        .detectAgain: "Volver a detectar",
        .runMesh: "Mallar", .runSolve: "Calcular", .stop: "Detener", .idle: "Listo",
        .meshing: "Mallando…", .solving: "Calculando…", .finished: "Terminado",
        .failed: "Detenido",
        .residuals: "Residuos", .simulatedTime: "Tiempo simulado", .log: "Registro",
        .noResults: "Aún no hay resultados", .noResultsBody: "Calcula el caso para ver aquí sus campos.",
        .field: "Campo", .statistics: "Estadísticas", .mean: "Media",
        .standardDeviation: "Desviación típica", .slice: "Corte", .axis: "Eje normal",
        .position: "Posición", .distance: "Distancia (m)", .value: "Valor", .view: "Vista", .threeD: "Superficie 3D",
        .engineStarting: "Iniciando el motor…", .engineLost: "El motor se detuvo",
        .engineMissing: "El motor no está en marcha.", .dismiss: "Descartar", .nozzle: "Tobera paramétrica",
        .inletDiameter: "Diámetro de entrada (mm)", .throatDiameter: "Diámetro de garganta (mm)",
        .outletDiameter: "Diámetro de salida (mm)", .convergingLength: "Longitud convergente (mm)",
        .divergingLength: "Longitud divergente (mm)", .makeNozzle: "Crear la tobera",
        .nozzleInvalid: "La garganta debe ser la sección más estrecha y todas las medidas positivas.",
        .segment: "Tramo", .length: "Longitud", .onAxis: "Está en Y = 0: este tramo es el eje.",
        .curvature: "Curvatura (bulge)", .straighten: "Recto", .bendOut: "Curvar hacia fuera", .bendIn: "Curvar hacia dentro",
        .contourHint: "Elige un vértice para escribir sus coordenadas, o un tramo para curvarlo o dividirlo. X es el eje de giro e Y el radio, en milímetros.",
        .inlet: "Entrada de presión total", .outlet: "Salida de presión",
        .axisBoundary: "Eje", .reservoir: "Depósito abierto", .unassigned: "Sin asignar", .wall: "Pared",
        .globalSize: "Tamaño global (mm)", .wallSize: "Cerca de pared (mm)", .wedgeAngle: "Ángulo de cuña (°)",
        .gasModel: "Gas y modelo", .flowModel: "Modelo de flujo", .inviscid: "No viscoso (Euler · paredes deslizantes)",
        .molecularWeight: "Masa molecular (kg/kmol)", .specificHeat: "Calor específico Cp (J/kg/K)",
        .dynamicViscosity: "Viscosidad dinámica (Pa s)", .conductivity: "Conductividad térmica (W/m/K)",
        .boundaryHint: "Las presiones son absolutas en Pa. En la entrada, presión y temperatura son totales; en la salida, la presión es estática.",
        .initialPressure: "Presión absoluta inicial (Pa)", .initialTemperature: "Temperatura inicial (K)",
        .calculation: "Cálculo", .automaticStop: "Parar automáticamente cuando el flujo se estabilice",
        .iterationLimit: "Límite de iteraciones de seguridad", .saveEvery: "Guardar cada N iteraciones",
        .acousticCourant: "Límite de Courant acústico", .processes: "Procesos de CPU (0 = automático)",
        .processesHint: "Los cálculos en paralelo usan una carpeta de trabajo aislada; las mallas pequeñas pueden ir más rápido con menos procesos.",
        .advancedCriteria: "Criterios de convergencia avanzados", .checkEvery: "Comprobar cada N iteraciones",
        .minimumIterations: "Iteraciones mínimas antes de aceptar", .stableChecks: "Comprobaciones correctas seguidas",
        .massTolerance: "Límite de error relativo de masa", .fieldTolerance: "Límite de cambio relativo de campo",
        .equationTolerance: "Límite de ritmo de actualización normalizado", .regionName: "Nombre de la región",
        .pressurePa: "Presión (Pa)", .temperatureK: "Temperatura (K)", .viscosityRatio: "Relación de viscosidad",
        .convergence: "Convergencia", .iteration: "Iteración", .massError: "Error global de masa",
        .localMassError: "Error local de masa", .fieldChange: "Cambio de campo", .updateRate: "Ritmo de actualización",
        .compressibleTitle: "Flujo compresible",
        .compressibleBody: "Para una tobera o un conducto axisimétrico, dibuja su contorno en lugar de una caja. El proyecto pasa al flujo de trabajo por contorno.",
        .engineering: "Análisis de sección", .reportName: "Nombre del informe", .centre: "Centrar",
        .direction: "Sentido positivo", .references: "Valores de referencia",
        .kinematicPressure: "La presión es cinemática (p/ρ, solvers incompresibles)",
        .useDensityField: "Usar el campo de densidad", .constantDensity: "Densidad (kg/m³)",
        .referencePressure: "Presión de referencia (Pa)", .referenceDensity: "Densidad de referencia (kg/m³)",
        .referenceSpeed: "Velocidad de referencia (m/s)", .referenceArea: "Área de referencia (m²)",
        .analyse: "Analizar", .reportNote:
            "Solo a partir de los campos guardados. La convergencia, la idoneidad del modelo y la independencia de malla necesitan su propia evidencia.",
        .project: "Proyecto",
        .inspector: "Inspector",
        .recentProjects: "Proyectos recientes",
        .openRecent: "Abrir reciente",
        .clearRecents: "Borrar menú",
        .stagesMenu: "Etapas",
        .simulationMenu: "Simulación",
        .toggleInspector: "Mostrar u ocultar el inspector",
        .nextStep: "Siguiente paso",
        .guideProject: "Revisa el proyecto",
        .guideGeometry: "Dibuja un contorno de fluido cerrado",
        .guideMesh: "Elige el tamaño de celda y malla",
        .guidePhysics: "Asigna a cada contorno su condición",
        .guideCalculation: "Lanza el cálculo",
        .guideResults: "Explora los resultados",
        .engineAndSolver: "Motor y solver",
        .notFound: "No encontrado",
        .resultData: "Datos de resultado",
        .meshReady: "mallado",
        .meshPending: "aún sin mallar",
        .openBoundaries: "contornos abiertos",
        .toolSelect: "Seleccionar",
        .toolAddPoint: "Añadir vértice",
        .toolPan: "Mover la vista",
        .toolSelectHint: "Arrastra vértices, o el fondo para mover la vista.",
        .toolAddPointHint: "Pulsa sobre un tramo para poner ahí un vértice.",
        .toolPanHint: "Arrastra para mover la vista; pellizca para acercar.",
        .zoomIn: "Acercar",
        .zoomOut: "Alejar",
        .fitView: "Ajustar la vista",
        .undo: "Deshacer",
        .redo: "Rehacer",
        .vertices: "Vértices",
        .vertex: "Vértice",
        .segments: "Tramos",
        .perimeter: "Perímetro",
        .closedProfile: "Perfil cerrado",
        .openProfile: "Perfil abierto",
        .coordinateX: "Coordenada X (mm)",
        .coordinateY: "Coordenada Y (mm)",
        .deleteVertex: "Eliminar el vértice",
        .splitSegment: "Dividir en dos",
        .nozzleReplaces: "Sustituye el contorno; se puede deshacer.",
        .meshHint: "El tamaño cerca de pared refina las celdas junto a las paredes; el ángulo de cuña es la porción del dominio axisimétrico que se malla.",
        .generateMesh: "Generar la malla",
        .meshSketchNote: "Un esquema de los tamaños, no la malla en sí",
        .estimate: "estimación",
        .converged: "Convergido",
        .fieldPressure: "Presión",
        .fieldSpeed: "Velocidad",
        .fieldTemperature: "Temperatura",
        .fieldMach: "Número de Mach",
        .fieldDensity: "Densidad",
        .fieldTurbulentViscosity: "Viscosidad turbulenta",
        .fieldTurbulentEnergy: "Energía cinética turbulenta",
        .goToCalculation: "Ir a Cálculo",
        .sections: "Cortes",
        .planeXY: "Plano XY · lateral",
        .planeXZ: "Plano XZ · superior",
        .planeYZ: "Plano YZ · frontal",
        .mirrorAxis: "Reflejar sobre el eje",
        .probe: "Sonda",
        .probeHelp: "Lee el valor bajo el puntero",
        .saveImage: "Guardar imagen",
        .profileShort: "Perfil",
        .analysisShort: "Análisis",
        .histogram: "Histograma",
        .profileLine: "Muestrear a lo largo de",
        .alongAxis: "El eje (X)",
        .verticalCentre: "La línea central vertical",
    ]

    /// The title of a report metric, as the desktop app words it.
    public static func metric(
        _ key: String, languageCode: String? = Locale.current.language.languageCode?.identifier
    ) -> String {
        let titles = languageCode == "es" ? spanishMetrics : englishMetrics
        return titles[key] ?? key
    }

    private static let englishMetrics: [String: String] = [
        "area_m2": "Section area (m²)", "massFlow_kg_s": "Signed mass flow (kg/s)",
        "forwardMassFlow_kg_s": "Forward mass flow (kg/s)", "reverseMassFlow_kg_s": "Reverse mass flow (kg/s)",
        "volumeFlow_m3_s": "Signed volume flow (m³/s)", "meanPressure_Pa": "Area-mean pressure (Pa)",
        "meanDensity_kg_m3": "Area-mean density (kg/m³)", "meanNormalVelocity_m_s": "Area-mean normal speed (m/s)",
        "momentumFlux_N": "Normal momentum flux (N)", "pressureLoad_N": "Pressure term (N)",
        "normalResultant_N": "Normal section resultant (N)", "kineticPower_W": "Signed kinetic power (W)",
        "normalCoefficient": "Normal section coefficient", "meanCp": "Mean pressure coefficient",
    ]

    private static let spanishMetrics: [String: String] = [
        "area_m2": "Área de la sección (m²)", "massFlow_kg_s": "Caudal másico con signo (kg/s)",
        "forwardMassFlow_kg_s": "Caudal en sentido positivo (kg/s)", "reverseMassFlow_kg_s": "Caudal en sentido inverso (kg/s)",
        "volumeFlow_m3_s": "Caudal volumétrico con signo (m³/s)", "meanPressure_Pa": "Presión media por área (Pa)",
        "meanDensity_kg_m3": "Densidad media por área (kg/m³)", "meanNormalVelocity_m_s": "Velocidad normal media por área (m/s)",
        "momentumFlux_N": "Flujo normal de cantidad de movimiento (N)", "pressureLoad_N": "Término de presión (N)",
        "normalResultant_N": "Resultante normal de sección (N)", "kineticPower_W": "Potencia cinética con signo (W)",
        "normalCoefficient": "Coeficiente normal de sección", "meanCp": "Coeficiente medio de presión",
    ]

    public static func t(
        _ key: Key, languageCode: String? = Locale.current.language.languageCode?.identifier
    ) -> String {
        let table = languageCode == "es" ? spanish : english
        return table[key] ?? english[key] ?? key.rawValue
    }
}
