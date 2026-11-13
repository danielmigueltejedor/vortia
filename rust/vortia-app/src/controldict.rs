//! Editing the run settings of a case: the times, the step and the write interval of its
//! `system/controlDict`. The file is edited as text, one value at a time, so its comments, its
//! layout and everything else in it stay as they were.

use std::path::Path;

use vortia_foamfile::write;

use crate::summary::Summary;

/// The settings of the run that the app edits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunSettings {
    pub start_time: f64,
    pub end_time: f64,
    pub delta_t: f64,
    pub write_interval: f64,
}

impl RunSettings {
    /// Says what is wrong with the settings, in words for the person who typed them.
    pub fn check(&self) -> Result<(), String> {
        let all = [
            self.start_time,
            self.end_time,
            self.delta_t,
            self.write_interval,
        ];
        if all.iter().any(|v| !v.is_finite()) {
            return Err("Every setting has to be a number.".to_owned());
        }
        if self.end_time <= self.start_time {
            return Err("The end time has to be after the start time.".to_owned());
        }
        if self.delta_t <= 0.0 {
            return Err("The time step has to be greater than zero.".to_owned());
        }
        if self.write_interval <= 0.0 {
            return Err("The write interval has to be greater than zero.".to_owned());
        }
        Ok(())
    }
}

/// The settings as the person types them: one text per number.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SettingsForm {
    pub start_time: String,
    pub end_time: String,
    pub delta_t: String,
    pub write_interval: String,
}

impl SettingsForm {
    /// The settings of a case as it is now; a value the case does not give is left empty.
    pub fn from_summary(summary: &Summary) -> Self {
        let text = |v: Option<f64>| v.map(write::scalar).unwrap_or_default();
        Self {
            start_time: text(summary.start_time),
            end_time: text(summary.end_time),
            delta_t: text(summary.delta_t),
            write_interval: text(summary.write_interval),
        }
    }

    /// The numbers typed, or what is wrong with them.
    pub fn parse(&self) -> Result<RunSettings, String> {
        let number = |name: &str, text: &str| {
            text.trim()
                .parse::<f64>()
                .map_err(|_| format!("'{name}' is not a number."))
        };
        let settings = RunSettings {
            start_time: number("Start time", &self.start_time)?,
            end_time: number("End time", &self.end_time)?,
            delta_t: number("Time step", &self.delta_t)?,
            write_interval: number("Write interval", &self.write_interval)?,
        };
        settings.check()?;
        Ok(settings)
    }
}

/// The keys the app edits, with how each is read from a `RunSettings`.
type Reader = fn(&RunSettings) -> f64;

const KEYS: [(&str, Reader); 4] = [
    ("startTime", |s| s.start_time),
    ("endTime", |s| s.end_time),
    ("deltaT", |s| s.delta_t),
    ("writeInterval", |s| s.write_interval),
];

/// `text` with the four settings set; a key that is not in the file is added at the end.
pub fn apply(text: &str, settings: &RunSettings) -> Result<String, String> {
    settings.check()?;
    let mut text = text.to_owned();
    for (key, get) in KEYS {
        text = set_entry(&text, key, &write::scalar(get(settings)))?;
    }
    Ok(text)
}

/// Sets the top-level entry `key` to `value`, keeping the spacing and what follows it.
pub(crate) fn set_entry(text: &str, key: &str, value: &str) -> Result<String, String> {
    let mut depth = 0_i32;
    let mut in_block_comment = false;
    let mut out = String::with_capacity(text.len() + value.len());
    let mut found = false;
    for line in text.split_inclusive('\n') {
        let code = code_of(line, &mut in_block_comment);
        let at_top = depth == 0;
        depth += code.matches('{').count() as i32 - code.matches('}').count() as i32;
        if at_top
            && !found
            && let Some(replaced) = replace_in_line(line, &code, key, value)?
        {
            out.push_str(&replaced);
            found = true;
            continue;
        }
        out.push_str(line);
    }
    if !found {
        if !out.ends_with('\n') && !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("{key:<15} {value};\n"));
    }
    Ok(out)
}

/// The part of a line that is code: without comments. `in_block` carries `/* … */` over lines.
pub(crate) fn code_of(line: &str, in_block: &mut bool) -> String {
    let mut code = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if *in_block {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                *in_block = false;
            }
            code.push(' ');
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            *in_block = true;
            code.push_str("  ");
        } else if c == '/' && chars.peek() == Some(&'/') {
            // The rest of the line is a comment; keep its length so positions still match.
            code.push(' ');
            code.extend(chars.by_ref().map(|_| ' '));
        } else {
            code.push(c);
        }
    }
    code
}

/// If this line is `key value;`, the line with the value replaced. An entry that does not fit on
/// one line is refused, since replacing it by position would break it.
pub(crate) fn replace_in_line(
    line: &str,
    code: &str,
    key: &str,
    value: &str,
) -> Result<Option<String>, String> {
    let trimmed = code.trim_start();
    let indent = code.len() - trimmed.len();
    let Some(after_key) = trimmed.strip_prefix(key) else {
        return Ok(None);
    };
    // `startTime` is not `startTimeOffset`.
    if !after_key.starts_with(char::is_whitespace) {
        return Ok(None);
    }
    let value_start = indent + key.len() + (after_key.len() - after_key.trim_start().len());
    let Some(end) = code.rfind(';') else {
        return Err(format!(
            "'{key}' is written over several lines; edit it in the file."
        ));
    };
    if end < value_start
        || code
            .get(value_start..end)
            .is_none_or(|v| v.trim().is_empty())
    {
        return Err(format!("'{key}' has no value to replace."));
    }
    let (Some(head), Some(tail)) = (line.get(..value_start), line.get(end..)) else {
        return Err(format!("'{key}' could not be edited."));
    };
    Ok(Some(format!("{head}{value}{tail}")))
}

/// Writes the settings into the case's controlDict, atomically.
pub fn save(case: &Path, settings: &RunSettings) -> Result<(), String> {
    settings.check()?;
    let path = case.join("system").join("controlDict");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    let edited = apply(&text, settings)?;
    write::atomic(&path, edited.as_bytes()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOZZLE: &str = "\
FoamFile
{
    version     2.0;
    format      ascii;
    class       dictionary;
    object      controlDict;
}
application     rhoSimpleFoam;
startFrom       startTime;
startTime       0;
stopAt          endTime;
endTime         500; // the last step
deltaT          1;
writeControl    timeStep;
writeInterval   100;
purgeWrite      0;
";

    fn settings(end: f64, dt: f64, write: f64) -> RunSettings {
        RunSettings {
            start_time: 0.0,
            end_time: end,
            delta_t: dt,
            write_interval: write,
        }
    }

    #[test]
    fn the_four_values_change_and_nothing_else_does() {
        let edited = apply(NOZZLE, &settings(1000.0, 0.5, 250.0)).unwrap();
        let expected = NOZZLE
            .replace("endTime         500;", "endTime         1000;")
            .replace("deltaT          1;", "deltaT          0.5;")
            .replace("writeInterval   100;", "writeInterval   250;");
        assert_eq!(edited, expected);
        // The comment after a value, and the key that only starts like another, are left.
        assert!(edited.contains("// the last step"));
        assert!(edited.contains("stopAt          endTime;"));
        assert!(edited.contains("startFrom       startTime;"));
    }

    #[test]
    fn small_numbers_are_written_the_way_the_engine_reads_them() {
        let edited = apply(NOZZLE, &settings(1e-3, 1e-7, 1e-4)).unwrap();
        assert!(edited.contains("deltaT          1e-7;"), "{edited}");
        assert!(edited.contains("endTime         0.001; // the last step"));
    }

    #[test]
    fn a_missing_key_is_added_and_a_nested_one_is_not_the_top_level_one() {
        let text = "FoamFile\n{\n    endTime 9;\n}\nstartTime 0;\ndeltaT 1;\nendTime 5;\n";
        let edited = apply(text, &settings(10.0, 1.0, 2.0)).unwrap();
        assert!(edited.starts_with("FoamFile\n{\n    endTime 9;\n}\n"));
        assert!(edited.contains("endTime 10;\n"));
        assert!(edited.contains("writeInterval   2;\n"));
        assert_eq!(edited.matches("endTime").count(), 2);
    }

    #[test]
    fn commented_out_lines_are_not_the_entry() {
        let text = "// endTime 1;\n/* endTime 2;\nendTime 3; */\nstartTime 0;\nendTime 5;\ndeltaT 1;\nwriteInterval 1;\n";
        let edited = apply(text, &settings(8.0, 1.0, 1.0)).unwrap();
        assert!(edited.starts_with("// endTime 1;\n/* endTime 2;\nendTime 3; */\n"));
        assert!(edited.contains("\nendTime 8;\n"));
    }

    #[test]
    fn an_entry_that_spans_lines_is_refused_and_not_mangled() {
        let text = "endTime\n    5;\n";
        assert!(
            apply(text, &settings(8.0, 1.0, 1.0))
                .unwrap_err()
                .contains("endTime")
        );
    }

    #[test]
    fn settings_that_cannot_run_are_refused_with_the_reason() {
        for (bad, word) in [
            (settings(0.0, 1.0, 1.0), "after"),
            (settings(5.0, 0.0, 1.0), "step"),
            (settings(5.0, -1.0, 1.0), "step"),
            (settings(5.0, 1.0, 0.0), "interval"),
            (settings(f64::NAN, 1.0, 1.0), "number"),
            (settings(f64::INFINITY, 1.0, 1.0), "number"),
        ] {
            assert!(bad.check().unwrap_err().contains(word), "{bad:?}");
            assert!(apply(NOZZLE, &bad).is_err());
        }
    }

    #[test]
    fn the_form_shows_the_case_and_reads_what_is_typed() {
        let summary = Summary {
            start_time: Some(0.0),
            end_time: Some(500.0),
            delta_t: Some(1e-5),
            write_interval: None,
            ..Summary::default()
        };
        let mut form = SettingsForm::from_summary(&summary);
        assert_eq!(
            (
                form.end_time.as_str(),
                form.delta_t.as_str(),
                form.write_interval.as_str()
            ),
            ("500", "1e-5", "")
        );
        assert!(form.parse().unwrap_err().contains("Write interval"));
        form.write_interval = " 1e2 ".into();
        assert_eq!(
            form.parse().unwrap(),
            RunSettings {
                start_time: 0.0,
                end_time: 500.0,
                delta_t: 1e-5,
                write_interval: 100.0,
            }
        );
        form.end_time = "soon".into();
        assert!(form.parse().unwrap_err().contains("End time"));
        form.end_time = "0".into();
        assert!(form.parse().unwrap_err().contains("after"));
    }

    #[test]
    fn saving_rewrites_the_file_of_the_case_and_the_engine_reads_it_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("system/controlDict");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, NOZZLE).unwrap();
        save(dir.path(), &settings(40.0, 2.0, 10.0)).unwrap();
        let read = crate::summary::read(dir.path());
        assert_eq!(
            (
                read.start_time,
                read.end_time,
                read.delta_t,
                read.write_interval
            ),
            (Some(0.0), Some(40.0), Some(2.0), Some(10.0))
        );
        assert_eq!(read.application.as_deref(), Some("rhoSimpleFoam"));
        // A bad request leaves the file as it was.
        let before = std::fs::read_to_string(&path).unwrap();
        assert!(save(dir.path(), &settings(0.0, 1.0, 1.0)).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        // And a case without a controlDict says so.
        assert!(save(&dir.path().join("nowhere"), &settings(5.0, 1.0, 1.0)).is_err());
    }
}
