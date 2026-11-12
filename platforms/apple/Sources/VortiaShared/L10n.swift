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
        case mean, standardDeviation, slice, axis, position, profile, distance, value
        case engineStarting, engineLost, engineMissing, dismiss, settings
        case contour, flowSetup, nozzle, inletDiameter, throatDiameter, outletDiameter
        case convergingLength, divergingLength, makeNozzle, nozzleInvalid, segment, length, onAxis
        case curvature, straighten, bendOut, bendIn, removeVertex, contourHint, dragToMove
        case inlet, outlet, axisBoundary, reservoir, unassigned, wall, noContour, noContourBody
        case globalSize, wallSize, wedgeAngle, gasModel, flowModel, inviscid, molecularWeight
        case specificHeat, dynamicViscosity, conductivity, boundaryHint, initialPressure
        case initialTemperature, calculation, automaticStop, iterationLimit, saveEvery
        case acousticCourant, processes, processesHint, advancedCriteria, checkEvery
        case minimumIterations, stableChecks, massTolerance, fieldTolerance, equationTolerance
        case regionName, pressurePa, temperatureK, viscosityRatio, convergence, iteration
        case massError, localMassError, fieldChange, updateRate, compressibleTitle, compressibleBody
    }

    private static let english: [Key: String] = [
        .appName: "Vortia", .edition: "Vortia Native",
        .editionBody:
            "The macOS edition, built with SwiftUI. Vortia Nodalia, the other edition, can stay installed beside it.",
        .newProject: "New Project…", .openProject: "Open Project…", .saveProject: "Save",
        .welcomeTitle: "Fluid engineering without black boxes",
        .welcomeBody: "Create a project to start from a lid-driven cavity, or open one you already have.",
        .geometry: "Geometry", .mesh: "Mesh", .physics: "Physics", .time: "Time and numerics",
        .run: "Run", .results: "Results",
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
        .position: "Position", .profile: "Profile along the vertical centre line",
        .distance: "Distance (m)", .value: "Value",
        .engineStarting: "Starting the engine…", .engineLost: "The engine stopped",
        .engineMissing: "The engine is not running.", .dismiss: "Dismiss", .settings: "Settings",
        .contour: "Contour", .flowSetup: "Flow setup", .nozzle: "Parametric nozzle",
        .inletDiameter: "Inlet diameter (mm)", .throatDiameter: "Throat diameter (mm)",
        .outletDiameter: "Outlet diameter (mm)", .convergingLength: "Converging length (mm)",
        .divergingLength: "Diverging length (mm)", .makeNozzle: "Make the nozzle",
        .nozzleInvalid: "The throat must be the narrowest section, and every size positive.",
        .segment: "Segment", .length: "Length", .onAxis: "On Y = 0: this segment is the axis.",
        .curvature: "Curvature (bulge)", .straighten: "Straight", .bendOut: "Bend out", .bendIn: "Bend in",
        .removeVertex: "Remove its end vertex",
        .contourHint: "Drag a vertex to move it. Click a segment to bend it or remove a vertex. X is the axis of rotation, Y the radius.",
        .dragToMove: "Drag to move", .inlet: "Total-pressure inlet", .outlet: "Pressure outlet",
        .axisBoundary: "Axis", .reservoir: "Open reservoir", .unassigned: "Unassigned", .wall: "Wall",
        .noContour: "No contour yet", .noContourBody: "Make a nozzle in Contour to set up compressible flow.",
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
    ]

    private static let spanish: [Key: String] = [
        .appName: "Vortia", .edition: "Vortia Native",
        .editionBody:
            "La edición para macOS, hecha con SwiftUI. Vortia Nodalia, la otra edición, puede seguir instalada a su lado.",
        .newProject: "Nuevo proyecto…", .openProject: "Abrir proyecto…", .saveProject: "Guardar",
        .welcomeTitle: "Ingeniería de fluidos sin cajas negras",
        .welcomeBody: "Crea un proyecto para empezar con una cavidad con tapa móvil, o abre uno que ya tengas.",
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
        .position: "Posición", .profile: "Perfil por la línea central vertical",
        .distance: "Distancia (m)", .value: "Valor",
        .engineStarting: "Iniciando el motor…", .engineLost: "El motor se detuvo",
        .engineMissing: "El motor no está en marcha.", .dismiss: "Descartar", .settings: "Ajustes",
        .contour: "Contorno", .flowSetup: "Configuración del flujo", .nozzle: "Tobera paramétrica",
        .inletDiameter: "Diámetro de entrada (mm)", .throatDiameter: "Diámetro de garganta (mm)",
        .outletDiameter: "Diámetro de salida (mm)", .convergingLength: "Longitud convergente (mm)",
        .divergingLength: "Longitud divergente (mm)", .makeNozzle: "Crear la tobera",
        .nozzleInvalid: "La garganta debe ser la sección más estrecha y todas las medidas positivas.",
        .segment: "Tramo", .length: "Longitud", .onAxis: "Está en Y = 0: este tramo es el eje.",
        .curvature: "Curvatura (bulge)", .straighten: "Recto", .bendOut: "Curvar hacia fuera", .bendIn: "Curvar hacia dentro",
        .removeVertex: "Quitar su vértice final",
        .contourHint: "Arrastra un vértice para moverlo. Pulsa un tramo para curvarlo o quitar un vértice. X es el eje de giro e Y el radio.",
        .dragToMove: "Arrastra para mover", .inlet: "Entrada de presión total", .outlet: "Salida de presión",
        .axisBoundary: "Eje", .reservoir: "Depósito abierto", .unassigned: "Sin asignar", .wall: "Pared",
        .noContour: "Aún no hay contorno", .noContourBody: "Crea una tobera en Contorno para configurar el flujo compresible.",
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
    ]

    public static func t(
        _ key: Key, languageCode: String? = Locale.current.language.languageCode?.identifier
    ) -> String {
        let table = languageCode == "es" ? spanish : english
        return table[key] ?? english[key] ?? key.rawValue
    }
}
