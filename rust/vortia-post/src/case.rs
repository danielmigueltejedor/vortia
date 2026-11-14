//! A case on disk, opened for looking into: its mesh and geometry, and the
//! fields of any of its times.

use std::path::{Path, PathBuf};

use vortia_core::{Aabb, Vec3};
use vortia_fields::{Field, FieldValue, Location, Time};
use vortia_foamfile::FoamFile;
use vortia_mesh::{MeshGeometry, Patch, PatchKind, PolyMesh};

use crate::error::{PostError, Result};
use crate::stats::{PatchStats, VolumeStats};

/// Most points one line may be sampled at.
pub const MAX_LINE_POINTS: usize = 100_000;

/// The values of a field, one per cell or per patch face.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValues {
    Scalar(Vec<f64>),
    Vector(Vec<Vec3>),
}

impl FieldValues {
    pub fn len(&self) -> usize {
        match self {
            Self::Scalar(v) => v.len(),
            Self::Vector(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Numbers per value: 1 for a scalar, 3 for a vector.
    pub fn components(&self) -> usize {
        match self {
            Self::Scalar(_) => 1,
            Self::Vector(_) => 3,
        }
    }

    /// Value `i` as its components.
    pub fn at(&self, i: usize) -> Option<Vec<f64>> {
        match self {
            Self::Scalar(v) => v.get(i).map(|&s| vec![s]),
            Self::Vector(v) => v.get(i).map(|p| vec![p.x, p.y, p.z]),
        }
    }

    /// The names of the components, for a table's header.
    pub fn component_names(&self, field: &str) -> Vec<String> {
        match self {
            Self::Scalar(_) => vec![field.to_owned()],
            Self::Vector(_) => ["x", "y", "z"]
                .iter()
                .map(|c| format!("{field}_{c}"))
                .collect(),
        }
    }
}

/// One patch of a [`FieldReport`].
#[derive(Debug, Clone, PartialEq)]
pub struct PatchReport {
    pub name: String,
    /// `patch`, `wall`, `wedge`, …
    pub kind: String,
    pub stats: PatchStats,
    /// What flows out through it; `None` when the time has no `phi`.
    pub flux: Option<f64>,
}

/// A field summed up over the domain and each patch.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldReport {
    pub time: String,
    pub field: String,
    pub volume: VolumeStats,
    pub patches: Vec<PatchReport>,
}

/// One point of a line, and what the field holds in the cell around it.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    /// Distance from the line's start.
    pub distance: f64,
    pub point: Vec3,
    /// The cell holding the point; `None` when the point is outside the mesh.
    pub cell: Option<usize>,
    /// The field's components there; empty outside the mesh.
    pub value: Vec<f64>,
}

#[derive(Debug)]
pub struct Case {
    path: PathBuf,
    pub(crate) mesh: PolyMesh,
    pub(crate) geometry: MeshGeometry,
    pub(crate) cell_faces: Vec<Vec<usize>>,
}

impl Case {
    /// Reads `<path>/constant/polyMesh` and works out its geometry.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_owned();
        let mesh = PolyMesh::read_case(&path)?;
        let geometry = MeshGeometry::compute(&mesh);
        let mut cell_faces = vec![Vec::new(); mesh.n_cells()];
        for (face, &cell) in mesh.owner().iter().enumerate() {
            if let Some(list) = cell_faces.get_mut(cell) {
                list.push(face);
            }
        }
        for (face, &cell) in mesh.neighbour().iter().enumerate() {
            if let Some(list) = cell_faces.get_mut(cell) {
                list.push(face);
            }
        }
        Ok(Self {
            path,
            mesh,
            geometry,
            cell_faces,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn mesh(&self) -> &PolyMesh {
        &self.mesh
    }

    pub fn geometry(&self) -> &MeshGeometry {
        &self.geometry
    }

    /// Every time directory, in order.
    pub fn times(&self) -> Result<Vec<Time>> {
        Ok(vortia_fields::times(&self.path)?)
    }

    /// The name of the last time written.
    pub fn latest_time(&self) -> Result<String> {
        vortia_fields::latest(&self.path)?
            .map(|t| t.name)
            .ok_or_else(|| {
                PostError::Invalid(format!("{} has no time directories", self.path.display()))
            })
    }

    /// The name of every volume field of `time`, scalar or vector.
    pub fn field_names(&self, time: &str) -> Result<Vec<String>> {
        let dir = self.time_dir(time)?;
        let entries = std::fs::read_dir(&dir).map_err(|source| {
            PostError::File(vortia_foamfile::FoamError::Io { path: dir, source })
        })?;
        let mut names = Vec::new();
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Ok(file) = FoamFile::open(&path) else {
                continue;
            };
            let Ok((Some(header), _)) = file.tokens() else {
                continue;
            };
            if matches!(header.class.as_str(), "volScalarField" | "volVectorField") {
                let name = entry.file_name().to_string_lossy().into_owned();
                names.push(name.strip_suffix(".gz").unwrap_or(&name).to_owned());
            }
        }
        names.sort();
        Ok(names)
    }

    /// The value of a volume field in every cell.
    pub fn field(&self, time: &str, name: &str) -> Result<FieldValues> {
        let (path, class) = self.field_file(time, name)?;
        match class.as_str() {
            "volScalarField" => Ok(FieldValues::Scalar(self.cells::<f64>(&path)?)),
            "volVectorField" => Ok(FieldValues::Vector(self.cells::<Vec3>(&path)?)),
            other => Err(PostError::Invalid(format!(
                "{name} is a {other}; only volScalarField and volVectorField can be looked into"
            ))),
        }
    }

    /// The value of a volume field on each face of a patch: the condition's
    /// own values where it writes them, the neighbouring cell's otherwise
    /// (`zeroGradient` writes none).
    pub fn patch_field(&self, time: &str, name: &str, patch: &str) -> Result<FieldValues> {
        let patch = self.patch(patch)?;
        let (path, class) = self.field_file(time, name)?;
        match class.as_str() {
            "volScalarField" => Ok(FieldValues::Scalar(self.on_patch::<f64>(&path, patch)?)),
            "volVectorField" => Ok(FieldValues::Vector(self.on_patch::<Vec3>(&path, patch)?)),
            other => Err(PostError::Invalid(format!(
                "{name} is a {other}; only volScalarField and volVectorField can be looked into"
            ))),
        }
    }

    /// What flows out through a patch: the sum of the face flux `phi` there,
    /// positive out of the domain. `phi` is volumetric for an incompressible
    /// solver and mass flux for a compressible one.
    pub fn flux(&self, time: &str, patch: &str) -> Result<f64> {
        let patch = self.patch(patch)?;
        let field = self.flux_field(time)?;
        Self::patch_flux(&field, patch)
    }

    fn flux_field(&self, time: &str) -> Result<Field<f64>> {
        let path = self.field_path(time, "phi")?;
        let field = Field::<f64>::read(&path)?;
        if field.location != Location::Faces {
            return Err(PostError::Invalid("phi is not a face field".into()));
        }
        Ok(field)
    }

    fn patch_flux(phi: &Field<f64>, patch: &Patch) -> Result<f64> {
        let pf = phi
            .patch_field(patch)
            .ok_or_else(|| PostError::Invalid(format!("phi has no entry for '{}'", patch.name)))?;
        let values = pf
            .value
            .as_ref()
            .and_then(|d| d.expand(patch.size))
            .ok_or_else(|| {
                PostError::Invalid(format!("phi holds no values on '{}'", patch.name))
            })?;
        Ok(values.iter().sum())
    }

    /// A field over the whole domain and on every patch, reading it once.
    /// The flux through each patch comes with it when the time has a `phi`.
    pub fn report(&self, time: &str, name: &str) -> Result<FieldReport> {
        let (path, class) = self.field_file(time, name)?;
        let (cells, faces) = match class.as_str() {
            "volScalarField" => self.everywhere::<f64>(&path, FieldValues::Scalar)?,
            "volVectorField" => self.everywhere::<Vec3>(&path, FieldValues::Vector)?,
            other => {
                return Err(PostError::Invalid(format!(
                    "{name} is a {other}; only volScalarField and volVectorField can be looked into"
                )));
            }
        };
        let volume = VolumeStats::of(&cells, &self.geometry.cell_volumes)
            .ok_or_else(|| PostError::Invalid("the mesh has no cells".into()))?;
        let phi = self.flux_field(time).ok();
        let patches = faces
            .into_iter()
            .filter_map(|(patch, values)| {
                let areas: Vec<f64> = patch
                    .faces()
                    .map(|f| self.geometry.face_areas.get(f).map_or(0.0, |a| a.norm()))
                    .collect();
                Some(PatchReport {
                    name: patch.name.clone(),
                    kind: patch.kind.name().to_owned(),
                    stats: PatchStats::of(&values, &areas)?,
                    flux: phi
                        .as_ref()
                        .and_then(|phi| Self::patch_flux(phi, patch).ok()),
                })
            })
            .collect();
        Ok(FieldReport {
            time: time.to_owned(),
            field: name.to_owned(),
            volume,
            patches,
        })
    }

    /// The box around the mesh.
    pub fn bounds(&self) -> Aabb {
        let mut points = self.mesh.points().iter();
        let first = points.next().copied().unwrap_or_default();
        let mut bounds = Aabb {
            min: first,
            max: first,
        };
        for &p in points {
            bounds.extend(p);
        }
        bounds
    }

    /// Area, integral, average and range of a field over a patch.
    pub fn patch_stats(&self, time: &str, name: &str, patch: &str) -> Result<PatchStats> {
        let values = self.patch_field(time, name, patch)?;
        let patch = self.patch(patch)?;
        let areas: Vec<f64> = patch
            .faces()
            .map(|f| self.geometry.face_areas.get(f).map_or(0.0, |a| a.norm()))
            .collect();
        PatchStats::of(&values, &areas)
            .ok_or_else(|| PostError::Invalid(format!("patch '{}' has no faces", patch.name)))
    }

    /// Volume, integral, average and range of a field over the whole domain.
    pub fn volume_stats(&self, time: &str, name: &str) -> Result<VolumeStats> {
        let values = self.field(time, name)?;
        VolumeStats::of(&values, &self.geometry.cell_volumes)
            .ok_or_else(|| PostError::Invalid("the mesh has no cells".into()))
    }

    /// The cell that holds `point`, if any.
    pub fn locate(&self, point: Vec3) -> Option<usize> {
        (0..self.mesh.n_cells()).find(|&c| self.contains(c, point))
    }

    /// The value of a field in the cell holding `point`.
    pub fn probe(&self, time: &str, name: &str, point: Vec3) -> Result<Sample> {
        let values = self.field(time, name)?;
        Ok(self.sample_at(&values, point, 0.0))
    }

    /// `points` values of a field along the line from `start` to `end`, the
    /// ends included, each the value of the cell around it (OpenFOAM's
    /// `cell` interpolation).
    pub fn sample_line(
        &self,
        time: &str,
        name: &str,
        start: Vec3,
        end: Vec3,
        points: usize,
    ) -> Result<(FieldValues, Vec<Sample>)> {
        if points == 0 || points > MAX_LINE_POINTS {
            return Err(PostError::Invalid(format!(
                "a line takes between 1 and {MAX_LINE_POINTS} points, not {points}"
            )));
        }
        let finite = |v: Vec3| v.x.is_finite() && v.y.is_finite() && v.z.is_finite();
        if !(finite(start) && finite(end)) {
            return Err(PostError::Invalid("the line's ends are not finite".into()));
        }
        let values = self.field(time, name)?;
        let length = (end - start).norm();
        let last = points.saturating_sub(1).max(1) as f64;
        let samples = (0..points)
            .map(|i| {
                let t = i as f64 / last;
                self.sample_at(&values, start + (end - start) * t, length * t)
            })
            .collect();
        Ok((values, samples))
    }

    fn sample_at(&self, values: &FieldValues, point: Vec3, distance: f64) -> Sample {
        let cell = self.locate(point);
        let value = cell.and_then(|c| values.at(c)).unwrap_or_default();
        Sample {
            distance,
            point,
            cell,
            value,
        }
    }

    /// Whether `point` is on the inner side of every face of `cell`.
    fn contains(&self, cell: usize, point: Vec3) -> bool {
        let Some(faces) = self.cell_faces.get(cell) else {
            return false;
        };
        let h = self
            .geometry
            .cell_volumes
            .get(cell)
            .map_or(0.0, |v| v.abs().cbrt());
        faces.iter().all(|&f| {
            let (Some(&centre), Some(&area), Some(&owner)) = (
                self.geometry.face_centres.get(f),
                self.geometry.face_areas.get(f),
                self.mesh.owner().get(f),
            ) else {
                return false;
            };
            let outward = if owner == cell { area } else { area * -1.0 };
            (point - centre).dot(outward) <= 1.0e-9 * area.norm() * h
        })
    }

    fn patch(&self, name: &str) -> Result<&Patch> {
        let patch = self.mesh.patch(name).ok_or_else(|| {
            let known: Vec<&str> = self
                .mesh
                .patches()
                .iter()
                .map(|p| p.name.as_str())
                .collect();
            PostError::Invalid(format!(
                "no patch '{name}' (the patches are {})",
                known.join(", ")
            ))
        })?;
        if patch.kind == PatchKind::Empty {
            return Err(PostError::Invalid(format!(
                "patch '{name}' is an empty patch and holds no values"
            )));
        }
        Ok(patch)
    }

    fn time_dir(&self, time: &str) -> Result<PathBuf> {
        // A time is a directory name, never a path.
        if time.is_empty() || time.contains(['/', '\\']) || time == ".." || time == "." {
            return Err(PostError::Invalid(format!("'{time}' is not a time")));
        }
        let dir = self.path.join(time);
        if dir.is_dir() {
            Ok(dir)
        } else {
            Err(PostError::Invalid(format!(
                "{} has no time '{time}'",
                self.path.display()
            )))
        }
    }

    fn field_path(&self, time: &str, name: &str) -> Result<PathBuf> {
        if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
            return Err(PostError::Invalid(format!("'{name}' is not a field name")));
        }
        let path = self.time_dir(time)?.join(name);
        if path.is_file() || path.with_file_name(format!("{name}.gz")).is_file() {
            Ok(path)
        } else {
            Err(PostError::Invalid(format!(
                "time {time} has no field '{name}'"
            )))
        }
    }

    fn field_file(&self, time: &str, name: &str) -> Result<(PathBuf, String)> {
        let path = self.field_path(time, name)?;
        let file = FoamFile::open(&path)?;
        let (header, _) = file.tokens()?;
        let class = header.map(|h| h.class).unwrap_or_default();
        Ok((path, class))
    }

    fn cells<T: FieldValue>(&self, path: &Path) -> Result<Vec<T>> {
        Ok(self.read_field::<T>(path)?.1)
    }

    fn on_patch<T: FieldValue>(&self, path: &Path, patch: &Patch) -> Result<Vec<T>> {
        let (field, cells) = self.read_field::<T>(path)?;
        self.face_values(&field, &cells, patch)
    }

    fn read_field<T: FieldValue>(&self, path: &Path) -> Result<(Field<T>, Vec<T>)> {
        let field = Field::<T>::read(path)?;
        field.check_against(&self.mesh)?;
        let cells = field
            .internal
            .expand(self.mesh.n_cells())
            .ok_or_else(|| PostError::Invalid("the values do not fit the mesh".into()))?;
        Ok((field, cells))
    }

    /// The cells, and the values on each patch that holds any, from one read.
    fn everywhere<T: FieldValue>(
        &self,
        path: &Path,
        wrap: fn(Vec<T>) -> FieldValues,
    ) -> Result<(FieldValues, Vec<(&Patch, FieldValues)>)> {
        let (field, cells) = self.read_field::<T>(path)?;
        let mut patches = Vec::new();
        for patch in self
            .mesh
            .patches()
            .iter()
            .filter(|p| p.kind != PatchKind::Empty && p.size > 0)
        {
            patches.push((patch, wrap(self.face_values(&field, &cells, patch)?)));
        }
        Ok((wrap(cells), patches))
    }

    fn face_values<T: FieldValue>(
        &self,
        field: &Field<T>,
        cells: &[T],
        patch: &Patch,
    ) -> Result<Vec<T>> {
        let entry = field.patch_field(patch);
        if let Some(values) = entry
            .and_then(|pf| pf.value.as_ref())
            .and_then(|d| d.expand(patch.size))
        {
            return Ok(values);
        }
        // `noSlip` writes no value: the fluid at the wall does not move.
        if entry.is_some_and(|pf| pf.kind == "noSlip") {
            return Ok(vec![T::default(); patch.size]);
        }
        patch
            .faces()
            .map(|f| {
                self.mesh
                    .owner()
                    .get(f)
                    .and_then(|&c| cells.get(c))
                    .copied()
                    .ok_or_else(|| PostError::Invalid("a patch face has no cell".into()))
            })
            .collect()
    }
}
