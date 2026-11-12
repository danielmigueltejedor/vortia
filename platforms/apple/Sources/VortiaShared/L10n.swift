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
        case mean, standardDeviation, slice, axis, position, profile, distance, value, view, threeD
        case engineStarting, engineLost, engineMissing, dismiss, settings
        case engineering, reportName, centre, direction, references, kinematicPressure
        case useDensityField, constantDensity, referencePressure, referenceDensity, referenceSpeed
        case referenceArea, analyse, reportNote
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
        .distance: "Distance (m)", .value: "Value", .view: "View", .threeD: "3D surface",
        .engineStarting: "Starting the engine…", .engineLost: "The engine stopped",
        .engineMissing: "The engine is not running.", .dismiss: "Dismiss", .settings: "Settings",
        .engineering: "Section analysis", .reportName: "Report name", .centre: "Centre",
        .direction: "Forward direction", .references: "Reference values",
        .kinematicPressure: "Pressure is kinematic (p/ρ, incompressible solvers)",
        .useDensityField: "Use the density field", .constantDensity: "Density (kg/m³)",
        .referencePressure: "Reference pressure (Pa)", .referenceDensity: "Reference density (kg/m³)",
        .referenceSpeed: "Reference speed (m/s)", .referenceArea: "Reference area (m²)",
        .analyse: "Analyse", .reportNote:
            "From the stored fields only. Convergence, model suitability and mesh independence need their own evidence.",
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
        .distance: "Distancia (m)", .value: "Valor", .view: "Vista", .threeD: "Superficie 3D",
        .engineStarting: "Iniciando el motor…", .engineLost: "El motor se detuvo",
        .engineMissing: "El motor no está en marcha.", .dismiss: "Descartar", .settings: "Ajustes",
        .engineering: "Análisis de sección", .reportName: "Nombre del informe", .centre: "Centrar",
        .direction: "Sentido positivo", .references: "Valores de referencia",
        .kinematicPressure: "La presión es cinemática (p/ρ, solvers incompresibles)",
        .useDensityField: "Usar el campo de densidad", .constantDensity: "Densidad (kg/m³)",
        .referencePressure: "Presión de referencia (Pa)", .referenceDensity: "Densidad de referencia (kg/m³)",
        .referenceSpeed: "Velocidad de referencia (m/s)", .referenceArea: "Área de referencia (m²)",
        .analyse: "Analizar", .reportNote:
            "Solo a partir de los campos guardados. La convergencia, la idoneidad del modelo y la independencia de malla necesitan su propia evidencia.",
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
