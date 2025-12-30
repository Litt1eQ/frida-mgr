use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundApp {
    pub package: String,
    pub activity: Option<String>,
    pub process: String,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundComponent {
    pub package: String,
    pub activity: String,
    pub line_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRecord {
    pub pid: u32,
    pub process: String,
}

static DUMPSYS_ACTIVITY_COMPONENT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?:mResumedActivity|ResumedActivity|mFocusedActivity):\s+ActivityRecord\{[^\}]*\s(?P<component>[A-Za-z0-9_\.]+/\.*[A-Za-z0-9_\.$]+)\b",
    )
    .expect("valid regex")
});

static DUMPSYS_WINDOW_COMPONENT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?:mCurrentFocus|mFocusedApp)=\S*\{[^\}]*\s(?P<component>[A-Za-z0-9_\.]+/\.*[A-Za-z0-9_\.$]+)\b",
    )
    .expect("valid regex")
});

static PROCESS_RECORD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?P<pid>\d+):(?P<process>[A-Za-z0-9_\.]+(?:(?::|\.)[A-Za-z0-9_\.]+)*)/")
        .expect("valid regex")
});

pub fn parse_component(component: &str, line_index: usize) -> Option<ForegroundComponent> {
    let (package, activity) = component.split_once('/')?;
    let activity = activity
        .strip_prefix('.')
        .map(|suffix| format!("{}.{}", package, suffix))
        .unwrap_or_else(|| activity.to_string());

    Some(ForegroundComponent {
        package: package.to_string(),
        activity,
        line_index,
    })
}

pub fn parse_foreground_component_from_dumpsys_activity_activities(
    output: &str,
) -> Option<ForegroundComponent> {
    for (idx, line) in output.lines().enumerate() {
        if let Some(caps) = DUMPSYS_ACTIVITY_COMPONENT_RE.captures(line) {
            let component = caps.name("component")?.as_str();
            return parse_component(component, idx);
        }
    }
    None
}

pub fn parse_foreground_component_from_dumpsys_window_windows(
    output: &str,
) -> Option<ForegroundComponent> {
    for (idx, line) in output.lines().enumerate() {
        if let Some(caps) = DUMPSYS_WINDOW_COMPONENT_RE.captures(line) {
            let component = caps.name("component")?.as_str();
            return parse_component(component, idx);
        }
    }
    None
}

pub fn find_process_name_near_activity_record(
    output: &str,
    start_line: usize,
    package: &str,
) -> Option<String> {
    find_process_record_near_activity_record(output, start_line, package).map(|r| r.process)
}

pub fn find_process_record_near_activity_record(
    output: &str,
    start_line: usize,
    package: &str,
) -> Option<ProcessRecord> {
    let lines: Vec<&str> = output.lines().collect();
    let end = (start_line + 250).min(lines.len());
    let package_colon = format!("{}:", package);
    let package_dot = format!("{}.", package);

    for line in &lines[start_line..end] {
        if let Some(caps) = PROCESS_RECORD_RE.captures(line) {
            let pid: u32 = caps.name("pid")?.as_str().parse().ok()?;
            let process = caps.name("process")?.as_str();
            if process == package
                || process.starts_with(&package_colon)
                || process.starts_with(&package_dot)
            {
                return Some(ProcessRecord {
                    pid,
                    process: process.to_string(),
                });
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_foreground_from_activity_activities() {
        let output = r#"
  mResumedActivity: ActivityRecord{abcd u0 com.example/.MainActivity t123}
        "#;
        let fg = parse_foreground_component_from_dumpsys_activity_activities(output).unwrap();
        assert_eq!(fg.package, "com.example");
        assert_eq!(fg.activity, "com.example.MainActivity");
    }

    #[test]
    fn parses_foreground_from_window_windows() {
        let output = r#"
  mCurrentFocus=Window{123 u0 com.example/.MainActivity}
        "#;
        let fg = parse_foreground_component_from_dumpsys_window_windows(output).unwrap();
        assert_eq!(fg.package, "com.example");
        assert_eq!(fg.activity, "com.example.MainActivity");
    }

    #[test]
    fn finds_process_name_nearby() {
        let output = r#"
  mResumedActivity: ActivityRecord{abcd u0 com.example/.MainActivity t123}
    app=ProcessRecord{aa 4242:com.example/u0a123}
        "#;
        let fg = parse_foreground_component_from_dumpsys_activity_activities(output).unwrap();
        let record =
            find_process_record_near_activity_record(output, fg.line_index, &fg.package).unwrap();
        assert_eq!(record.pid, 4242);
        assert_eq!(record.process, "com.example");
    }

    #[test]
    fn finds_process_name_with_suffix() {
        let output = r#"
  mResumedActivity: ActivityRecord{abcd u0 com.example/.MainActivity t123}
    app=ProcessRecord{aa 4242:com.example:remote/u0a123}
        "#;
        let fg = parse_foreground_component_from_dumpsys_activity_activities(output).unwrap();
        let record =
            find_process_record_near_activity_record(output, fg.line_index, &fg.package).unwrap();
        assert_eq!(record.pid, 4242);
        assert_eq!(record.process, "com.example:remote");
    }
}
