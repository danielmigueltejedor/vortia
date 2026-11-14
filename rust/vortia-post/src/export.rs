//! A case's fields as files other tools read: a table of the cells for a spreadsheet or a
//! script, and a VTK unstructured grid for ParaView.
//!
//! The grid keeps the mesh as it is: each cell is a polyhedron of its own faces, so nothing is
//! split or interpolated, and a field is written as the cell values the solver has.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use crate::case::{Case, FieldValues};
use crate::error::{PostError, Result};

/// Most cells an export takes; beyond it the text would not be read by anything.
pub const MAX_EXPORT_CELLS: usize = 20_000_000;

impl Case {
    /// The fields to write: those named, or all the volume fields of `time` when none are.
    fn export_fields(&self, time: &str, names: &[String]) -> Result<Vec<(String, FieldValues)>> {
        if self.mesh.n_cells() > MAX_EXPORT_CELLS {
            return Err(PostError::Invalid(format!(
                "the mesh has {} cells; an export takes at most {MAX_EXPORT_CELLS}",
                self.mesh.n_cells()
            )));
        }
        let names = if names.is_empty() {
            self.field_names(time)?
        } else {
            names.to_vec()
        };
        let mut fields = Vec::with_capacity(names.len());
        for name in names {
            let values = self.field(time, &name)?;
            if values.len() != self.mesh.n_cells() {
                return Err(PostError::Invalid(format!(
                    "{name} has {} values, but the mesh has {} cells",
                    values.len(),
                    self.mesh.n_cells()
                )));
            }
            fields.push((name, values));
        }
        Ok(fields)
    }

    /// One row per cell: its number, centre and volume, then the fields (`names`, or every
    /// volume field of `time` when there are none).
    pub fn cells_csv(&self, time: &str, names: &[String]) -> Result<String> {
        let fields = self.export_fields(time, names)?;
        let mut out = String::from("cell,x,y,z,volume");
        for (name, values) in &fields {
            for column in values.component_names(name) {
                out.push(',');
                out.push_str(&column);
            }
        }
        out.push('\n');
        for cell in 0..self.mesh.n_cells() {
            let centre = self
                .geometry
                .cell_centres
                .get(cell)
                .copied()
                .unwrap_or_default();
            let volume = self.geometry.cell_volumes.get(cell).copied().unwrap_or(0.0);
            let _ = write!(
                out,
                "{cell},{:e},{:e},{:e},{volume:e}",
                centre.x, centre.y, centre.z
            );
            for (_, values) in &fields {
                for v in values.at(cell).unwrap_or_default() {
                    let _ = write!(out, ",{v:e}");
                }
            }
            out.push('\n');
        }
        Ok(out)
    }

    /// The mesh and fields as a VTK XML unstructured grid (`.vtu`, ASCII), every cell a
    /// polyhedron of its faces with outward-pointing orientation.
    pub fn vtu(&self, time: &str, names: &[String]) -> Result<String> {
        let fields = self.export_fields(time, names)?;
        let points = self.mesh.points();
        let cells = self.mesh.n_cells();
        let mut out = String::new();
        let _ = writeln!(
            out,
            "<?xml version=\"1.0\"?>\n<VTKFile type=\"UnstructuredGrid\" version=\"1.0\" \
             byte_order=\"LittleEndian\">\n<UnstructuredGrid>\n\
             <Piece NumberOfPoints=\"{}\" NumberOfCells=\"{cells}\">",
            points.len()
        );

        out.push_str(
            "<Points>\n<DataArray type=\"Float64\" NumberOfComponents=\"3\" format=\"ascii\">\n",
        );
        for p in points {
            let _ = writeln!(out, "{:e} {:e} {:e}", p.x, p.y, p.z);
        }
        out.push_str("</DataArray>\n</Points>\n<Cells>\n");

        let mut connectivity = String::new();
        let mut offsets = String::new();
        let mut faces = String::new();
        let mut face_offsets = String::new();
        let (mut connected, mut written) = (0usize, 0usize);
        for cell in 0..cells {
            let own = self.cell_faces.get(cell).map_or(&[][..], Vec::as_slice);
            let mut unique = BTreeSet::new();
            let _ = write!(faces, "{}", own.len());
            written += 1;
            for &f in own {
                let ring = self.mesh.face(f);
                // A face points out of its owner; the neighbour sees it the other way round.
                let reversed = self.mesh.owner().get(f) != Some(&cell);
                let _ = write!(faces, " {}", ring.len());
                written += 1;
                let order: Vec<usize> = if reversed {
                    ring.iter().rev().copied().collect()
                } else {
                    ring.to_vec()
                };
                for i in order {
                    unique.insert(i);
                    let _ = write!(faces, " {i}");
                    written += 1;
                }
            }
            faces.push('\n');
            let _ = writeln!(face_offsets, "{written}");
            for i in &unique {
                let _ = write!(connectivity, "{i} ");
            }
            connectivity.push('\n');
            connected += unique.len();
            let _ = writeln!(offsets, "{connected}");
        }
        for (name, body) in [("connectivity", &connectivity), ("offsets", &offsets)] {
            let _ = writeln!(
                out,
                "<DataArray type=\"Int64\" Name=\"{name}\" format=\"ascii\">"
            );
            out.push_str(body);
            out.push_str("</DataArray>\n");
        }
        out.push_str("<DataArray type=\"UInt8\" Name=\"types\" format=\"ascii\">\n");
        for _ in 0..cells {
            out.push_str("42\n");
        }
        out.push_str("</DataArray>\n");
        for (name, body) in [("faces", &faces), ("faceoffsets", &face_offsets)] {
            let _ = writeln!(
                out,
                "<DataArray type=\"Int64\" Name=\"{name}\" format=\"ascii\">"
            );
            out.push_str(body);
            out.push_str("</DataArray>\n");
        }
        out.push_str("</Cells>\n<CellData>\n");
        for (name, values) in &fields {
            let _ = writeln!(
                out,
                "<DataArray type=\"Float64\" Name=\"{}\" NumberOfComponents=\"{}\" format=\"ascii\">",
                xml_escape(name),
                values.components()
            );
            for cell in 0..cells {
                let row: Vec<String> = values
                    .at(cell)
                    .unwrap_or_default()
                    .iter()
                    .map(|v| format!("{v:e}"))
                    .collect();
                let _ = writeln!(out, "{}", row.join(" "));
            }
            out.push_str("</DataArray>\n");
        }
        out.push_str("</CellData>\n</Piece>\n</UnstructuredGrid>\n</VTKFile>\n");
        Ok(out)
    }
}

fn xml_escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            c => c.to_string(),
        })
        .collect()
}
