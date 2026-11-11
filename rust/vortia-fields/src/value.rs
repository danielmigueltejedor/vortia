//! The types a field can hold, and how each reads and writes.

use vortia_core::Vec3;
use vortia_foamfile::Value;
use vortia_foamfile::write::scalar;

pub trait FieldValue: Copy + PartialEq + Default + std::fmt::Debug {
    /// `scalar`, `vector`: the word in `volScalarField` and `List<scalar>`.
    const NAME: &'static str;
    const CLASS_NAME: &'static str;

    fn from_value(v: &Value) -> Option<Self>;
    fn to_value(self) -> Value;
}

impl FieldValue for f64 {
    const NAME: &'static str = "scalar";
    const CLASS_NAME: &'static str = "Scalar";

    fn from_value(v: &Value) -> Option<Self> {
        v.as_scalar()
    }

    fn to_value(self) -> Value {
        Value::Scalar(self)
    }
}

impl FieldValue for Vec3 {
    const NAME: &'static str = "vector";
    const CLASS_NAME: &'static str = "Vector";

    fn from_value(v: &Value) -> Option<Self> {
        match v.as_list()? {
            [x, y, z] => Some(Vec3::new(x.as_scalar()?, y.as_scalar()?, z.as_scalar()?)),
            _ => None,
        }
    }

    fn to_value(self) -> Value {
        Value::List(vec![
            Value::Scalar(self.x),
            Value::Scalar(self.y),
            Value::Scalar(self.z),
        ])
    }
}

/// One value as it appears in a file.
pub fn format<T: FieldValue>(v: T) -> String {
    match v.to_value() {
        Value::Scalar(s) => scalar(s),
        Value::List(items) => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(Value::as_scalar)
                .map(scalar)
                .collect();
            format!("({})", parts.join(" "))
        }
        _ => String::new(),
    }
}
